use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::formats::formatter::FormattingProgress;
use crate::formats::FileType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferredType {
    String,
    Integer,
    Float,
    Boolean,
    Null,
    Mixed,
    Object,
    Array,
}

impl InferredType {
    pub fn as_str(&self) -> &'static str {
        match self {
            InferredType::String => "String",
            InferredType::Integer => "Integer",
            InferredType::Float => "Float",
            InferredType::Boolean => "Boolean",
            InferredType::Null => "Null",
            InferredType::Mixed => "Mixed",
            InferredType::Object => "Object",
            InferredType::Array => "Array",
        }
    }

    pub fn combine(self, other: InferredType) -> InferredType {
        if self == other {
            return self;
        }
        if self == InferredType::Null {
            return other;
        }
        if other == InferredType::Null {
            return self;
        }
        if (self == InferredType::Integer && other == InferredType::Float)
            || (self == InferredType::Float && other == InferredType::Integer)
        {
            return InferredType::Float;
        }
        InferredType::Mixed
    }
}

#[derive(Debug, Clone)]
pub struct ValueFrequency {
    pub value: String,
    pub count: u64,
    pub percentage: f32,
}

#[derive(Debug, Clone)]
pub struct FieldStats {
    pub name: String,
    pub inferred_type: InferredType,
    pub total_occurrences: u64,
    pub null_count: u64,
    pub presence_pct: f32,
    pub min_value: Option<String>,
    pub max_value: Option<String>,
    pub min_len: usize,
    pub max_len: usize,
    pub cardinality_approx: u64,
    pub top_values: Vec<ValueFrequency>,
}

#[derive(Debug, Clone)]
pub struct AnalysisProgress {
    pub bytes_processed: u64,
    pub total_bytes: u64,
    pub records_scanned: u64,
    pub speed_mb_s: f64,
    pub progress_pct: f32,
    pub elapsed_secs: f64,
    pub is_finished: bool,
    pub error: Option<String>,
}

impl AnalysisProgress {
    pub fn new(total_bytes: u64) -> Self {
        Self {
            bytes_processed: 0,
            total_bytes,
            records_scanned: 0,
            speed_mb_s: 0.0,
            progress_pct: 0.0,
            elapsed_secs: 0.0,
            is_finished: false,
            error: None,
        }
    }

    pub fn update(&mut self, processed: u64, records: u64, start: Instant) {
        self.bytes_processed = processed;
        self.records_scanned = records;
        let elapsed = start.elapsed().as_secs_f64();
        self.elapsed_secs = elapsed;

        if elapsed > 0.05 {
            let mb = (processed as f64) / (1024.0 * 1024.0);
            self.speed_mb_s = mb / elapsed;

            if self.total_bytes > 0 {
                self.progress_pct = ((processed as f64 / self.total_bytes as f64) * 100.0).clamp(0.0, 100.0) as f32;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AnalysisReport {
    pub total_records: u64,
    pub total_bytes: u64,
    pub elapsed_secs: f64,
    pub throughput_mb_s: f64,
    pub fields: Vec<FieldStats>,
    pub record_tag: Option<String>,
}

impl AnalysisReport {
    pub fn to_json_pretty(&self) -> String {
        use std::fmt::Write;
        let mut out = String::with_capacity(1024 * 16);
        let _ = writeln!(out, "{{");
        let _ = writeln!(out, "  \"total_records\": {},", self.total_records);
        let _ = writeln!(out, "  \"total_bytes\": {},", self.total_bytes);
        let _ = writeln!(out, "  \"elapsed_secs\": {:.4},", self.elapsed_secs);
        let _ = writeln!(out, "  \"throughput_mb_s\": {:.2},", self.throughput_mb_s);
        let _ = writeln!(out, "  \"fields_count\": {},", self.fields.len());
        let _ = writeln!(out, "  \"fields\": [");

        for (i, f) in self.fields.iter().enumerate() {
            let _ = writeln!(out, "    {{");
            let _ = writeln!(out, "      \"name\": {:?},", f.name);
            let _ = writeln!(out, "      \"inferred_type\": {:?},", f.inferred_type.as_str());
            let _ = writeln!(out, "      \"total_occurrences\": {},", f.total_occurrences);
            let _ = writeln!(out, "      \"null_count\": {},", f.null_count);
            let _ = writeln!(out, "      \"presence_pct\": {:.2},", f.presence_pct);
            let _ = writeln!(out, "      \"min_value\": {:?},", f.min_value);
            let _ = writeln!(out, "      \"max_value\": {:?},", f.max_value);
            let _ = writeln!(out, "      \"min_length\": {},", f.min_len);
            let _ = writeln!(out, "      \"max_length\": {},", f.max_len);
            let _ = writeln!(out, "      \"cardinality_approx\": {},", f.cardinality_approx);
            let _ = writeln!(out, "      \"top_frequencies\": [");
            for (j, freq) in f.top_values.iter().enumerate() {
                let comma = if j + 1 < f.top_values.len() { "," } else { "" };
                let _ = writeln!(
                    out,
                    "        {{ \"value\": {:?}, \"count\": {}, \"pct\": {:.2} }}{}",
                    freq.value, freq.count, freq.percentage, comma
                );
            }
            let _ = writeln!(out, "      ]");
            let field_comma = if i + 1 < self.fields.len() { "," } else { "" };
            let _ = writeln!(out, "    }}{}", field_comma);
        }

        let _ = writeln!(out, "  ]");
        let _ = writeln!(out, "}}");
        out
    }
}


#[inline]
fn truncate_to_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        s
    } else {
        let mut idx = max_bytes;
        while idx > 0 && !s.is_char_boundary(idx) {
            idx -= 1;
        }
        &s[..idx]
    }
}

/// Bounded frequency counter for streaming high-cardinality distribution analysis
struct BoundedFrequencyTracker {
    counts: HashMap<String, u64>,
    max_entries: usize,
    watermark: usize,
    total_distinct_seen: u64,
}

impl BoundedFrequencyTracker {
    fn new(max_entries: usize) -> Self {
        let max_entries = max_entries.max(150_000);
        let watermark = max_entries + 50_000;
        Self {
            counts: HashMap::with_capacity(1024),
            max_entries,
            watermark,
            total_distinct_seen: 0,
        }
    }

    #[inline]
    fn insert(&mut self, val: &str) {
        let key = truncate_to_boundary(val, 128);

        if let Some(c) = self.counts.get_mut(key) {
            *c += 1;
            return;
        }

        self.total_distinct_seen += 1;

        if self.counts.len() < self.watermark {
            self.counts.insert(key.to_string(), 1);
        } else {
            // Fast batch prune infrequent entries
            let mut threshold = 1;
            while self.counts.len() > self.max_entries && threshold <= 10 {
                self.counts.retain(|_, &mut count| count > threshold);
                threshold += 1;
            }
            if self.counts.len() < self.watermark {
                self.counts.insert(key.to_string(), 1);
            }
        }
    }

    fn top_values(&self, total_records: u64, limit: usize) -> Vec<ValueFrequency> {
        let mut list: Vec<(String, u64)> = self.counts.iter().map(|(k, &v)| (k.clone(), v)).collect();
        list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        list.into_iter()
            .take(limit)
            .map(|(value, count)| {
                let percentage = if total_records > 0 {
                    (count as f32 / total_records as f32) * 100.0
                } else {
                    0.0
                };
                ValueFrequency {
                    value,
                    count,
                    percentage,
                }
            })
            .collect()
    }
}

/// Accumulator for a single field during scanning
struct FieldAccumulator {
    name: String,
    inferred_type: Option<InferredType>,
    occurrences: u64,
    null_count: u64,
    min_value: Option<String>,
    max_value: Option<String>,
    min_len: usize,
    max_len: usize,
    tracker: BoundedFrequencyTracker,
}

impl FieldAccumulator {
    fn new(name: String) -> Self {
        Self {
            name,
            inferred_type: None,
            occurrences: 0,
            null_count: 0,
            min_value: None,
            max_value: None,
            min_len: usize::MAX,
            max_len: 0,
            tracker: BoundedFrequencyTracker::new(256),
        }
    }

    #[inline]
    fn record_value(&mut self, val: &str, raw_type: InferredType) {
        self.occurrences += 1;

        if raw_type == InferredType::Null || val == "null" || val.is_empty() {
            self.null_count += 1;
            self.inferred_type = Some(match self.inferred_type {
                Some(curr) => curr.combine(InferredType::Null),
                None => InferredType::Null,
            });
            return;
        }

        self.inferred_type = Some(match self.inferred_type {
            Some(curr) => curr.combine(raw_type),
            None => raw_type,
        });

        let len = val.len();
        self.min_len = self.min_len.min(len);
        self.max_len = self.max_len.max(len);

        let val_bounded = truncate_to_boundary(val, 128);

        match self.min_value.as_ref() {
            Some(curr) if val_bounded < curr.as_str() => self.min_value = Some(val_bounded.to_string()),
            None => self.min_value = Some(val_bounded.to_string()),
            _ => {}
        }

        match self.max_value.as_ref() {
            Some(curr) if val_bounded > curr.as_str() => self.max_value = Some(val_bounded.to_string()),
            None => self.max_value = Some(val_bounded.to_string()),
            _ => {}
        }

        self.tracker.insert(val);
    }

    fn to_stats(self, total_records: u64) -> FieldStats {
        let non_null = self.occurrences.saturating_sub(self.null_count);
        let presence_pct = if total_records > 0 {
            (non_null as f32 / total_records as f32) * 100.0
        } else {
            0.0
        };

        FieldStats {
            name: self.name,
            inferred_type: self.inferred_type.unwrap_or(InferredType::String),
            total_occurrences: self.occurrences,
            null_count: self.null_count,
            presence_pct,
            min_value: self.min_value,
            max_value: self.max_value,
            min_len: if self.min_len == usize::MAX { 0 } else { self.min_len },
            max_len: self.max_len,
            cardinality_approx: self.tracker.total_distinct_seen,
            top_values: self.tracker.top_values(total_records, 200_000),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContainerKind {
    RootArray,
    RootObject,
    ObjectFromKey,
    ArrayFromKey,
    ObjectInArray,
    ArrayInArray,
}

#[inline]
fn build_field_name(path_stack: &[String], leaf_key: &str) -> String {
    if path_stack.is_empty() {
        leaf_key.to_string()
    } else if leaf_key.is_empty() {
        path_stack.join(".")
    } else {
        format!("{}.{}", path_stack.join("."), leaf_key)
    }
}

pub struct StreamingFieldAnalyzer;

impl StreamingFieldAnalyzer {
    const BUFFER_CAPACITY: usize = 256 * 1024;

    /// Infer primitive type from raw string value
    #[inline]
    fn infer_primitive_type(val: &str) -> InferredType {
        let trimmed = val.trim();
        if trimmed == "null" || trimmed.is_empty() {
            return InferredType::Null;
        }
        if trimmed.len() > 64 {
            return InferredType::String;
        }
        if trimmed.eq_ignore_ascii_case("true") || trimmed.eq_ignore_ascii_case("false") {
            return InferredType::Boolean;
        }
        if trimmed.parse::<i64>().is_ok() {
            return InferredType::Integer;
        }
        if trimmed.parse::<f64>().is_ok() {
            return InferredType::Float;
        }
        InferredType::String
    }

    /// Profile a JSON file/stream using streaming tokenization without DOM allocation,
    /// supporting deeply nested objects, arrays of objects, and hierarchical field paths.
    pub fn analyze_json<R: Read>(
        mut reader: R,
        _total_bytes: u64,
        cancel: Arc<AtomicBool>,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<AnalysisReport, String> {
        let start_time = Instant::now();
        let mut in_buffer = vec![0u8; Self::BUFFER_CAPACITY];
        let mut total_processed: u64 = 0;
        let mut total_records: u64 = 0;
        let mut array_records_count: u64 = 0;
        let mut accumulators: HashMap<String, FieldAccumulator> = HashMap::new();
        let mut last_progress_report = Instant::now();

        let mut in_string = false;
        let mut escaped = false;
        let mut string_buf = Vec::with_capacity(256);
        let mut val_buf = Vec::with_capacity(64);

        let mut container_stack: Vec<ContainerKind> = Vec::with_capacity(32);
        let mut path_stack: Vec<String> = Vec::with_capacity(32);
        let mut pending_key: Option<String> = None;
        let mut expecting_colon = false;

        let flush_val = |val_buf: &mut Vec<u8>,
                             pending_key: &mut Option<String>,
                             path_stack: &[String],
                             container_stack: &[ContainerKind],
                             accumulators: &mut HashMap<String, FieldAccumulator>| {
            if val_buf.is_empty() {
                return;
            }
            let val_str = String::from_utf8_lossy(val_buf).trim().to_string();
            val_buf.clear();
            if val_str.is_empty() {
                return;
            }
            let ptype = Self::infer_primitive_type(&val_str);

            if let Some(key) = pending_key.take() {
                let field_name = build_field_name(path_stack, &key);
                let acc = accumulators.entry(field_name.clone()).or_insert_with(|| FieldAccumulator::new(field_name));
                acc.record_value(&val_str, ptype);
            } else if let Some(last) = container_stack.last() {
                if matches!(last, ContainerKind::ArrayFromKey | ContainerKind::ArrayInArray) {
                    let field_name = build_field_name(path_stack, "");
                    if !field_name.is_empty() {
                        let acc = accumulators.entry(field_name.clone()).or_insert_with(|| FieldAccumulator::new(field_name));
                        acc.record_value(&val_str, ptype);
                    }
                }
            }
        };

        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Analysis cancelled by user".to_string());
            }

            let bytes_read = reader.read(&mut in_buffer).map_err(|e| e.to_string())?;
            if bytes_read == 0 {
                break;
            }

            total_processed += bytes_read as u64;
            let slice = &in_buffer[..bytes_read];
            let mut idx = 0;

            while idx < bytes_read {
                let b = slice[idx];

                if in_string {
                    if escaped {
                        escaped = false;
                        if string_buf.len() < 512 {
                            string_buf.push(b);
                        }
                    } else if b == b'\\' {
                        escaped = true;
                    } else if b == b'"' {
                        in_string = false;
                        let text = String::from_utf8_lossy(&string_buf).to_string();
                        string_buf.clear();

                        let in_object = container_stack.last().map_or(false, |c| {
                            matches!(c, ContainerKind::RootObject | ContainerKind::ObjectFromKey | ContainerKind::ObjectInArray)
                        });

                        if in_object && pending_key.is_none() {
                            // This string is a property KEY
                            pending_key = Some(text);
                            expecting_colon = true;
                        } else if let Some(key) = pending_key.take() {
                            // This string is a property VALUE
                            let field_name = build_field_name(&path_stack, &key);
                            let acc = accumulators.entry(field_name.clone()).or_insert_with(|| FieldAccumulator::new(field_name));
                            acc.record_value(&text, InferredType::String);
                        } else {
                            // Value inside array
                            let field_name = build_field_name(&path_stack, "");
                            if !field_name.is_empty() {
                                let acc = accumulators.entry(field_name.clone()).or_insert_with(|| FieldAccumulator::new(field_name));
                                acc.record_value(&text, InferredType::String);
                            }
                        }
                    } else if string_buf.len() < 512 {
                        string_buf.push(b);
                    }
                    idx += 1;
                    continue;
                }

                if b.is_ascii_whitespace() {
                    flush_val(&mut val_buf, &mut pending_key, &path_stack, &container_stack, &mut accumulators);
                    idx += 1;
                    continue;
                }

                match b {
                    b'"' => {
                        flush_val(&mut val_buf, &mut pending_key, &path_stack, &container_stack, &mut accumulators);
                        in_string = true;
                        escaped = false;
                        string_buf.clear();
                    }
                    b':' => {
                        expecting_colon = false;
                    }
                    b'{' => {
                        flush_val(&mut val_buf, &mut pending_key, &path_stack, &container_stack, &mut accumulators);
                        expecting_colon = false;

                        if container_stack.is_empty() {
                            total_records += 1;
                            container_stack.push(ContainerKind::RootObject);
                        } else if container_stack.last() == Some(&ContainerKind::RootArray) {
                            total_records += 1;
                            container_stack.push(ContainerKind::ObjectInArray);
                        } else if let Some(key) = pending_key.take() {
                            // Nested object from a property key: "foo": {
                            let field_name = build_field_name(&path_stack, &key);
                            let acc = accumulators.entry(field_name.clone()).or_insert_with(|| FieldAccumulator::new(field_name));
                            acc.record_value("{}", InferredType::Object);

                            path_stack.push(key);
                            container_stack.push(ContainerKind::ObjectFromKey);
                        } else {
                            if container_stack.len() == 2 && container_stack[0] == ContainerKind::RootObject {
                                array_records_count += 1;
                            }
                            container_stack.push(ContainerKind::ObjectInArray);
                        }
                    }
                    b'}' => {
                        flush_val(&mut val_buf, &mut pending_key, &path_stack, &container_stack, &mut accumulators);
                        if let Some(top) = container_stack.pop() {
                            if top == ContainerKind::ObjectFromKey {
                                path_stack.pop();
                            }
                        }
                        pending_key = None;
                        expecting_colon = false;
                    }
                    b'[' => {
                        flush_val(&mut val_buf, &mut pending_key, &path_stack, &container_stack, &mut accumulators);
                        expecting_colon = false;

                        if container_stack.is_empty() {
                            container_stack.push(ContainerKind::RootArray);
                        } else if let Some(key) = pending_key.take() {
                            // Nested array from a property key: "foo": [
                            let field_name = build_field_name(&path_stack, &key);
                            let acc = accumulators.entry(field_name.clone()).or_insert_with(|| FieldAccumulator::new(field_name));
                            acc.record_value("[]", InferredType::Array);

                            path_stack.push(format!("{}[]", key));
                            container_stack.push(ContainerKind::ArrayFromKey);
                        } else {
                            container_stack.push(ContainerKind::ArrayInArray);
                        }
                    }
                    b']' => {
                        flush_val(&mut val_buf, &mut pending_key, &path_stack, &container_stack, &mut accumulators);
                        if let Some(top) = container_stack.pop() {
                            if top == ContainerKind::ArrayFromKey {
                                path_stack.pop();
                            }
                        }
                        pending_key = None;
                        expecting_colon = false;
                    }
                    b',' => {
                        flush_val(&mut val_buf, &mut pending_key, &path_stack, &container_stack, &mut accumulators);
                        pending_key = None;
                        expecting_colon = false;
                    }
                    _ => {
                        if !expecting_colon && val_buf.len() < 64 {
                            val_buf.push(b);
                        }
                    }
                }
                idx += 1;
            }

            if last_progress_report.elapsed().as_millis() >= 100 {
                if let Some(ref prog) = progress {
                    prog.write().unwrap().update(total_processed, start_time);
                }
                last_progress_report = Instant::now();
            }
        }

        flush_val(&mut val_buf, &mut pending_key, &path_stack, &container_stack, &mut accumulators);

        let elapsed = start_time.elapsed().as_secs_f64();
        let throughput = if elapsed > 0.0 {
            (total_processed as f64 / (1024.0 * 1024.0)) / elapsed
        } else {
            0.0
        };

        if total_records <= 1 && array_records_count > 0 {
            total_records = array_records_count;
        } else if total_records == 0 {
            total_records = 1;
        }

        let mut fields: Vec<FieldStats> = accumulators
            .into_values()
            .map(|acc| acc.to_stats(total_records))
            .collect();
        fields.sort_by(|a, b| b.total_occurrences.cmp(&a.total_occurrences).then_with(|| a.name.cmp(&b.name)));

        Ok(AnalysisReport {
            total_records,
            total_bytes: total_processed,
            elapsed_secs: elapsed,
            throughput_mb_s: throughput,
            fields,
            record_tag: None,
        })
    }

    /// Profile a CSV / TSV file using streaming line reader
    pub fn analyze_csv<R: Read>(
        reader: R,
        _total_bytes: u64,
        delimiter: u8,
        cancel: Arc<AtomicBool>,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<AnalysisReport, String> {
        let start_time = Instant::now();
        let mut buf_reader = BufReader::with_capacity(Self::BUFFER_CAPACITY, reader);
        let mut total_processed: u64 = 0;
        let mut total_records: u64 = 0;
        let mut headers: Vec<String> = Vec::new();
        let mut accumulators: Vec<FieldAccumulator> = Vec::new();
        let mut last_progress_report = Instant::now();

        let mut line = String::with_capacity(1024);

        while buf_reader.read_line(&mut line).map_err(|e| e.to_string())? > 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err("Analysis cancelled by user".to_string());
            }

            total_processed += line.len() as u64;
            let trimmed = line.trim_end();

            if trimmed.is_empty() {
                line.clear();
                continue;
            }

            // Split line taking into account quotes
            let mut fields = Vec::new();
            let mut in_quote = false;
            let mut start = 0;
            let bytes = trimmed.as_bytes();

            for (i, &b) in bytes.iter().enumerate() {
                if b == b'"' {
                    in_quote = !in_quote;
                } else if b == delimiter && !in_quote {
                    let field = &trimmed[start..i];
                    fields.push(field.trim_matches('"').trim());
                    start = i + 1;
                }
            }
            if start <= trimmed.len() {
                fields.push(trimmed[start..].trim_matches('"').trim());
            }

            if headers.is_empty() {
                // First row is headers
                headers = fields.iter().enumerate().map(|(idx, f)| {
                    if f.is_empty() {
                        format!("column_{}", idx + 1)
                    } else {
                        f.to_string()
                    }
                }).collect();
                accumulators = headers.iter().map(|h| FieldAccumulator::new(h.clone())).collect();
            } else {
                total_records += 1;
                for (idx, field_val) in fields.into_iter().enumerate() {
                    if idx >= accumulators.len() {
                        let name = format!("column_{}", idx + 1);
                        accumulators.push(FieldAccumulator::new(name));
                    }
                    let ptype = Self::infer_primitive_type(field_val);
                    accumulators[idx].record_value(field_val, ptype);
                }
            }

            line.clear();

            if last_progress_report.elapsed().as_millis() >= 100 {
                if let Some(ref prog) = progress {
                    prog.write().unwrap().update(total_processed, start_time);
                }
                last_progress_report = Instant::now();
            }
        }

        let elapsed = start_time.elapsed().as_secs_f64();
        let throughput = if elapsed > 0.0 {
            (total_processed as f64 / (1024.0 * 1024.0)) / elapsed
        } else {
            0.0
        };

        if total_records == 0 {
            total_records = 1;
        }

        let mut fields: Vec<FieldStats> = accumulators
            .into_iter()
            .map(|acc| acc.to_stats(total_records))
            .collect();
        fields.sort_by(|a, b| b.total_occurrences.cmp(&a.total_occurrences).then_with(|| a.name.cmp(&b.name)));

        Ok(AnalysisReport {
            total_records,
            total_bytes: total_processed,
            elapsed_secs: elapsed,
            throughput_mb_s: throughput,
            fields,
            record_tag: None,
        })
    }

    /// Profile an XML file/stream using streaming quick-xml reader,
    /// capturing all fields, CDATA sections, nested structures, objects, and attributes.
    pub fn analyze_xml<R: Read>(
        reader: R,
        total_bytes: u64,
        cancel: Arc<AtomicBool>,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<AnalysisReport, String> {
        let buf_reader = BufReader::with_capacity(Self::BUFFER_CAPACITY, reader);
        Self::analyze_xml_from_reader(Reader::from_reader(buf_reader), total_bytes, cancel, progress)
    }

    /// High-performance XML analyzer operating on any BufRead or in-memory slice (e.g. mmap)
    pub fn analyze_xml_from_reader<B: BufRead>(
        mut xml_reader: Reader<B>,
        _total_bytes: u64,
        cancel: Arc<AtomicBool>,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<AnalysisReport, String> {
        struct TagStackItem {
            name_bytes: [u8; 32],
            name_len: u8,
            path_len_before: usize,
            has_children: bool,
        }

        struct RecordCandidate {
            name: String,
            depth: usize,
            total_count: u64,
            with_children_count: u64,
        }

        let start_time = Instant::now();
        xml_reader.config_mut().expand_empty_elements = false;
        xml_reader.config_mut().trim_text(true);

        let mut buf = Vec::with_capacity(8192);
        let mut accumulators: Vec<FieldAccumulator> = Vec::with_capacity(128);
        let mut path_to_field_id: HashMap<Vec<u8>, usize> = HashMap::with_capacity(128);

        let mut last_progress_report = Instant::now();
        let mut tag_stack: Vec<TagStackItem> = Vec::with_capacity(32);
        let mut path_buf: Vec<u8> = Vec::with_capacity(256);
        let mut text_buf: Vec<u8> = Vec::with_capacity(4096);

        let mut record_candidates: Vec<RecordCandidate> = Vec::with_capacity(16);
        let mut event_count: u32 = 0;

        loop {
            event_count = event_count.wrapping_add(1);
            if (event_count & 0x1FFF) == 0 {
                if cancel.load(Ordering::Relaxed) {
                    return Err("Analysis cancelled by user".to_string());
                }
                if last_progress_report.elapsed().as_millis() >= 100 {
                    let pos = xml_reader.buffer_position() as u64;
                    if let Some(ref prog) = progress {
                        prog.write().unwrap().update(pos, start_time);
                    }
                    last_progress_report = Instant::now();
                }
            }

            match xml_reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let raw_name = e.name();
                    let name_bytes = raw_name.as_ref();

                    if let Some(parent) = tag_stack.last_mut() {
                        parent.has_children = true;
                    }

                    let current_depth = tag_stack.len();
                    let path_len_before = path_buf.len();

                    if current_depth >= 2 {
                        if !path_buf.is_empty() {
                            path_buf.push(b'/');
                        }
                        path_buf.extend_from_slice(name_bytes);
                    }

                    // Record attributes
                    for attr in e.attributes().flatten() {
                        let attr_key = attr.key.as_ref();
                        let val_bytes = attr.value.as_ref();
                        let val = String::from_utf8_lossy(val_bytes);
                        let ptype = Self::infer_primitive_type(&val);

                        let attr_path = if current_depth >= 2 {
                            let mut p = Vec::with_capacity(path_buf.len() + 1 + attr_key.len());
                            p.extend_from_slice(&path_buf);
                            p.push(b'@');
                            p.extend_from_slice(attr_key);
                            p
                        } else {
                            let mut p = Vec::with_capacity(name_bytes.len() + 2 + attr_key.len());
                            p.push(b'@');
                            p.extend_from_slice(name_bytes);
                            p.push(b':');
                            p.extend_from_slice(attr_key);
                            p
                        };

                        let id = match path_to_field_id.get(&attr_path) {
                            Some(&id) => id,
                            None => {
                                let name = String::from_utf8_lossy(&attr_path).to_string();
                                let id = accumulators.len();
                                accumulators.push(FieldAccumulator::new(name));
                                path_to_field_id.insert(attr_path, id);
                                id
                            }
                        };
                        accumulators[id].record_value(&val, ptype);
                    }

                    let mut stored_name = [0u8; 32];
                    let copy_len = name_bytes.len().min(32);
                    stored_name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);

                    tag_stack.push(TagStackItem {
                        name_bytes: stored_name,
                        name_len: copy_len as u8,
                        path_len_before,
                        has_children: false,
                    });

                    text_buf.clear();
                }
                Ok(Event::Empty(e)) => {
                    let raw_name = e.name();
                    let name_bytes = raw_name.as_ref();

                    if let Some(parent) = tag_stack.last_mut() {
                        parent.has_children = true;
                    }

                    let current_depth = tag_stack.len();
                    if current_depth >= 2 {
                        let mut empty_path = Vec::with_capacity(path_buf.len() + 1 + name_bytes.len());
                        if !path_buf.is_empty() {
                            empty_path.extend_from_slice(&path_buf);
                            empty_path.push(b'/');
                        }
                        empty_path.extend_from_slice(name_bytes);

                        for attr in e.attributes().flatten() {
                            let attr_key = attr.key.as_ref();
                            let val_bytes = attr.value.as_ref();
                            let val = String::from_utf8_lossy(val_bytes);
                            let ptype = Self::infer_primitive_type(&val);

                            let mut attr_path = Vec::with_capacity(empty_path.len() + 1 + attr_key.len());
                            attr_path.extend_from_slice(&empty_path);
                            attr_path.push(b'@');
                            attr_path.extend_from_slice(attr_key);

                            let id = match path_to_field_id.get(&attr_path) {
                                Some(&id) => id,
                                None => {
                                    let name = String::from_utf8_lossy(&attr_path).to_string();
                                    let id = accumulators.len();
                                    accumulators.push(FieldAccumulator::new(name));
                                    path_to_field_id.insert(attr_path, id);
                                    id
                                }
                            };
                            accumulators[id].record_value(&val, ptype);
                        }

                        let id = match path_to_field_id.get(&empty_path) {
                            Some(&id) => id,
                            None => {
                                let name = String::from_utf8_lossy(&empty_path).to_string();
                                let id = accumulators.len();
                                accumulators.push(FieldAccumulator::new(name));
                                path_to_field_id.insert(empty_path, id);
                                id
                            }
                        };
                        accumulators[id].null_count += 1;
                        accumulators[id].occurrences += 1;
                    } else {
                        for attr in e.attributes().flatten() {
                            let attr_key = attr.key.as_ref();
                            let val_bytes = attr.value.as_ref();
                            let val = String::from_utf8_lossy(val_bytes);
                            let ptype = Self::infer_primitive_type(&val);

                            let mut attr_path = Vec::with_capacity(name_bytes.len() + 2 + attr_key.len());
                            attr_path.push(b'@');
                            attr_path.extend_from_slice(name_bytes);
                            attr_path.push(b':');
                            attr_path.extend_from_slice(attr_key);

                            let id = match path_to_field_id.get(&attr_path) {
                                Some(&id) => id,
                                None => {
                                    let name = String::from_utf8_lossy(&attr_path).to_string();
                                    let id = accumulators.len();
                                    accumulators.push(FieldAccumulator::new(name));
                                    path_to_field_id.insert(attr_path, id);
                                    id
                                }
                            };
                            accumulators[id].record_value(&val, ptype);
                        }
                    }
                }
                Ok(Event::Text(t)) => {
                    text_buf.extend_from_slice(t.as_ref());
                }
                Ok(Event::CData(c)) => {
                    text_buf.extend_from_slice(c.as_ref());
                }
                Ok(Event::End(_)) => {
                    if let Some(tag_info) = tag_stack.pop() {
                        let depth = tag_stack.len();
                        let tag_name_slice = &tag_info.name_bytes[..tag_info.name_len as usize];

                        if (depth == 1 || depth == 2) && tag_info.name_len > 0 {
                            if let Some(cand) = record_candidates.iter_mut().find(|c| c.depth == depth && c.name.as_bytes() == tag_name_slice) {
                                cand.total_count += 1;
                                if tag_info.has_children {
                                    cand.with_children_count += 1;
                                }
                            } else if record_candidates.len() < 32 {
                                let name = String::from_utf8_lossy(tag_name_slice).to_string();
                                record_candidates.push(RecordCandidate {
                                    name,
                                    depth,
                                    total_count: 1,
                                    with_children_count: if tag_info.has_children { 1 } else { 0 },
                                });
                            }
                        }

                        if depth >= 1 && depth < 32 {
                            let has_children = tag_info.has_children;
                            let target_path: &[u8] = if depth >= 2 {
                                &path_buf
                            } else {
                                tag_name_slice
                            };

                            if !target_path.is_empty() {
                                let trimmed = std::str::from_utf8(&text_buf)
                                    .map(|s| s.trim())
                                    .unwrap_or("");

                                if !trimmed.is_empty() {
                                    let ptype = Self::infer_primitive_type(trimmed);
                                    let id = match path_to_field_id.get(target_path) {
                                        Some(&id) => id,
                                        None => {
                                            let name = String::from_utf8_lossy(target_path).to_string();
                                            let id = accumulators.len();
                                            accumulators.push(FieldAccumulator::new(name));
                                            path_to_field_id.insert(target_path.to_vec(), id);
                                            id
                                        }
                                    };
                                    accumulators[id].record_value(trimmed, ptype);
                                } else if has_children {
                                    let id = match path_to_field_id.get(target_path) {
                                        Some(&id) => id,
                                        None => {
                                            let name = String::from_utf8_lossy(target_path).to_string();
                                            let id = accumulators.len();
                                            accumulators.push(FieldAccumulator::new(name));
                                            path_to_field_id.insert(target_path.to_vec(), id);
                                            id
                                        }
                                    };
                                    accumulators[id].occurrences += 1;
                                    accumulators[id].inferred_type = Some(InferredType::Object);
                                } else if depth >= 2 {
                                    let id = match path_to_field_id.get(target_path) {
                                        Some(&id) => id,
                                        None => {
                                            let name = String::from_utf8_lossy(target_path).to_string();
                                            let id = accumulators.len();
                                            accumulators.push(FieldAccumulator::new(name));
                                            path_to_field_id.insert(target_path.to_vec(), id);
                                            id
                                        }
                                    };
                                    accumulators[id].null_count += 1;
                                    accumulators[id].occurrences += 1;
                                }
                            }
                        }

                        path_buf.truncate(tag_info.path_len_before);
                        text_buf.clear();
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("XML error: {}", e)),
                _ => {}
            }

            buf.clear();
        }

        let elapsed = start_time.elapsed().as_secs_f64();
        let total_processed = xml_reader.buffer_position() as u64;
        let throughput = if elapsed > 0.0 {
            (total_processed as f64 / (1024.0 * 1024.0)) / elapsed
        } else {
            0.0
        };

        // Determine repeating record element
        let best_record = record_candidates
            .iter()
            .filter(|c| c.with_children_count > 1)
            .min_by_key(|c| (c.depth, -(c.with_children_count as i64)))
            .map(|c| (c.name.clone(), c.with_children_count))
            .or_else(|| {
                record_candidates
                    .iter()
                    .filter(|c| c.total_count > 1)
                    .min_by_key(|c| (c.depth, -(c.total_count as i64)))
                    .map(|c| (c.name.clone(), c.total_count))
            })
            .or_else(|| {
                record_candidates
                    .iter()
                    .filter(|c| c.depth == 1)
                    .max_by_key(|c| c.total_count)
                    .map(|c| (c.name.clone(), c.total_count))
            });

        let (detected_record_tag, total_records) = if let Some((tag, count)) = best_record {
            (Some(tag), count)
        } else {
            (None, 1)
        };

        let mut fields: Vec<FieldStats> = accumulators
            .into_iter()
            .map(|acc| acc.to_stats(total_records))
            .collect();
        fields.sort_by(|a, b| b.total_occurrences.cmp(&a.total_occurrences).then_with(|| a.name.cmp(&b.name)));

        Ok(AnalysisReport {
            total_records,
            total_bytes: total_processed,
            elapsed_secs: elapsed,
            throughput_mb_s: throughput,
            fields,
            record_tag: detected_record_tag,
        })
    }

    /// Profile a file on disk according to its detected format
    pub fn analyze_file<P: AsRef<Path>>(
        file_path: P,
        file_type: FileType,
        cancel: Arc<AtomicBool>,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<AnalysisReport, String> {
        let file = File::open(file_path.as_ref()).map_err(|e| format!("Failed to open file: {}", e))?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        let total_bytes = metadata.len();

        if let Some(ref prog) = progress {
            *prog.write().unwrap() = FormattingProgress::new(total_bytes);
        }

        let res = match file_type {
            FileType::Json => Self::analyze_json(file, total_bytes, Arc::clone(&cancel), progress.clone()),
            FileType::Csv => Self::analyze_csv(file, total_bytes, b',', Arc::clone(&cancel), progress.clone()),
            FileType::Xml => Self::analyze_xml(file, total_bytes, Arc::clone(&cancel), progress.clone()),
            _ => Self::analyze_csv(file, total_bytes, b',', Arc::clone(&cancel), progress.clone()),
        };

        if let Some(ref prog) = progress {
            let mut p = prog.write().unwrap();
            p.is_finished = true;
            p.progress_pct = 100.0;
            if let Err(ref e) = res {
                p.error = Some(e.clone());
            }
        }

        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    #[test]
    fn test_analyze_xml_cdata_and_nested_fields() {
        let xml = r#"<source xmlns:fo="http://www.w3.org/1999/XSL/Format">
  <job>
    <referencenumber>604233562</referencenumber>
    <url><![CDATA[https://kimco.thejobnetwork.com/job/123]]></url>
    <company><![CDATA[Kimco Staffing]]></company>
    <category><![CDATA[General]]></category>
    <CategoryID>28</CategoryID>
    <city><![CDATA[Garden Grove]]></city>
    <state><![CDATA[CA]]></state>
    <country><![CDATA[US]]></country>
    <description><![CDATA[<span>Software Engineer</span>]]></description>
    <salary>
      <min>50000</min>
      <max>85000</max>
      <currency>USD</currency>
    </salary>
  </job>
</source>"#;

        let cancel = Arc::new(AtomicBool::new(false));
        let report = StreamingFieldAnalyzer::analyze_xml(Cursor::new(xml), xml.len() as u64, cancel, None).unwrap();

        assert_eq!(report.total_records, 1);

        let field_names: Vec<&str> = report.fields.iter().map(|f| f.name.as_str()).collect();

        // Must contain all direct CDATA fields
        assert!(field_names.contains(&"url"), "Missing url");
        assert!(field_names.contains(&"company"), "Missing company");
        assert!(field_names.contains(&"category"), "Missing category");
        assert!(field_names.contains(&"city"), "Missing city");
        assert!(field_names.contains(&"state"), "Missing state");
        assert!(field_names.contains(&"country"), "Missing country");
        assert!(field_names.contains(&"description"), "Missing description");

        // Must contain plain text fields
        assert!(field_names.contains(&"referencenumber"), "Missing referencenumber");
        assert!(field_names.contains(&"CategoryID"), "Missing CategoryID");

        // Must contain nested fields and container
        assert!(field_names.contains(&"salary"), "Missing salary container");
        assert!(field_names.contains(&"salary/min"), "Missing salary/min");
        assert!(field_names.contains(&"salary/max"), "Missing salary/max");
        assert!(field_names.contains(&"salary/currency"), "Missing salary/currency");

        // Verify values
        let city_stat = report.fields.iter().find(|f| f.name == "city").unwrap();
        assert_eq!(city_stat.inferred_type, InferredType::String);
        assert_eq!(city_stat.min_value.as_deref(), Some("Garden Grove"));

        let cat_stat = report.fields.iter().find(|f| f.name == "CategoryID").unwrap();
        assert_eq!(cat_stat.inferred_type, InferredType::Integer);
        assert_eq!(cat_stat.min_value.as_deref(), Some("28"));

        assert_eq!(report.record_tag, Some("job".to_string()));
    }

    #[test]
    fn test_analyze_xml_with_header_metadata_tags() {
        let xml = r#"<source xmlns:fo="http://www.w3.org/1999/XSL/Format">
  <publisher>TheJobNetwork</publisher>
  <publisherurl>https://thejobnetwork.com</publisherurl>
  <lastBuildDate>2026-09-22</lastBuildDate>
  <job>
    <referencenumber>101</referencenumber>
    <title>Rust Developer</title>
  </job>
  <job>
    <referencenumber>102</referencenumber>
    <title>Backend Engineer</title>
  </job>
  <job>
    <referencenumber>103</referencenumber>
    <title>Systems Architect</title>
  </job>
</source>"#;

        let cancel = Arc::new(AtomicBool::new(false));
        let report = StreamingFieldAnalyzer::analyze_xml(Cursor::new(xml), xml.len() as u64, cancel, None).unwrap();

        assert_eq!(report.total_records, 3);
        assert_eq!(report.record_tag, Some("job".to_string()));
    }
}

