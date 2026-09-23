use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crossbeam_channel::Sender;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

#[derive(Debug, Clone)]
pub struct ExtractionProgress {
    pub records_extracted: u64,
    pub bytes_processed: u64,
    pub total_bytes: u64,
    pub is_finished: bool,
}

pub struct StreamingFieldExtractor;

impl StreamingFieldExtractor {
    /// Escapes a CSV field per RFC 4180
    pub fn escape_csv_field(val: &str, delimiter: u8) -> String {
        let delim_char = delimiter as char;
        if val.contains(delim_char) || val.contains('"') || val.contains('\n') || val.contains('\r') {
            let escaped = val.replace('"', "\"\"");
            format!("\"{}\"", escaped)
        } else {
            val.to_string()
        }
    }

    /// Look up a field value from the extracted record map with flexible path and case matching
    pub fn get_field_value<'a>(
        map: &'a HashMap<String, String>,
        field: &str,
        record_tag: &str,
    ) -> Option<&'a str> {
        let field_clean = field.trim();
        // 1. Direct match
        if let Some(v) = map.get(field_clean) {
            return Some(v.as_str());
        }

        // 2. Strip record_tag prefix if present (e.g. "job/title" -> "title")
        let rec_tag_prefix = format!("{}/", record_tag);
        let stripped = field_clean.trim_start_matches('/').trim_start_matches(&rec_tag_prefix);
        if let Some(v) = map.get(stripped) {
            return Some(v.as_str());
        }

        // 3. Dot vs slash conversion ("salary.min" <-> "salary/min")
        let slash_version = field_clean.replace('.', "/");
        if let Some(v) = map.get(&slash_version) {
            return Some(v.as_str());
        }
        let dot_version = field_clean.replace('/', ".");
        if let Some(v) = map.get(&dot_version) {
            return Some(v.as_str());
        }

        // 4. Leaf tag match (e.g. "job/city" -> "city")
        let leaf = field_clean.rsplit('/').next().unwrap_or(field_clean);
        if let Some(v) = map.get(leaf) {
            return Some(v.as_str());
        }

        // 5. Attribute syntax: "@id" <-> "id"
        if leaf.starts_with('@') {
            let bare = &leaf[1..];
            if let Some(v) = map.get(bare) {
                return Some(v.as_str());
            }
        } else {
            let with_at = format!("@{}", leaf);
            if let Some(v) = map.get(&with_at) {
                return Some(v.as_str());
            }
        }

        // 6. Case-insensitive fallback
        let lower_field = field_clean.to_ascii_lowercase();
        let lower_stripped = stripped.to_ascii_lowercase();
        let lower_leaf = leaf.to_ascii_lowercase();
        for (k, v) in map {
            let lk = k.to_ascii_lowercase();
            if lk == lower_field || lk == lower_stripped || lk == lower_leaf {
                return Some(v.as_str());
            }
        }

        None
    }

    /// Extracts fields from an XML stream and writes CSV rows to the writer.
    pub fn extract_xml<R: Read, W: Write>(
        reader: R,
        writer: &mut W,
        record_tag: &str,
        fields: &[String],
        delimiter: u8,
        include_headers: bool,
        cancel: Arc<AtomicBool>,
        total_bytes: u64,
        progress_tx: Option<Sender<ExtractionProgress>>,
    ) -> Result<u64, String> {
        let buf_reader = BufReader::with_capacity(64 * 1024, reader);
        let mut xml_reader = Reader::from_reader(buf_reader);
        xml_reader.config_mut().expand_empty_elements = true;
        xml_reader.config_mut().check_end_names = false;
        xml_reader.config_mut().trim_text(false);

        let delim_str = (delimiter as char).to_string();
        let mut out_writer = BufWriter::with_capacity(64 * 1024, writer);

        // Write CSV Header
        if include_headers {
            let header_row = fields
                .iter()
                .map(|f| Self::escape_csv_field(f.trim(), delimiter))
                .collect::<Vec<_>>()
                .join(&delim_str);
            out_writer.write_all(header_row.as_bytes()).map_err(|e| e.to_string())?;
            out_writer.write_all(b"\n").map_err(|e| e.to_string())?;
        }

        let mut buf = Vec::with_capacity(512);
        let mut in_record = false;
        let mut current_record_values: HashMap<String, String> = HashMap::new();
        let mut child_stack: Vec<String> = Vec::with_capacity(16);
        let mut records_count: u64 = 0;
        let mut last_report = std::time::Instant::now();
        let clean_record_tag = record_tag.trim().split(':').last().unwrap_or(record_tag).trim();

        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Extraction cancelled by user".to_string());
            }

            match xml_reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let raw_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let tag = raw_name.split(':').last().unwrap_or(&raw_name).to_string();

                    if !in_record {
                        if tag.eq_ignore_ascii_case(clean_record_tag) {
                            in_record = true;
                            current_record_values.clear();
                            child_stack.clear();

                            // Read attributes of the record element
                            for attr in e.attributes().flatten() {
                                let k = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                                let v = attr.decode_and_unescape_value(xml_reader.decoder())
                                    .map(|cow| cow.into_owned())
                                    .unwrap_or_else(|_| String::from_utf8_lossy(attr.value.as_ref()).into_owned());
                                current_record_values.insert(format!("@{}", k), v.clone());
                                current_record_values.insert(k, v);
                            }
                        }
                    } else {
                        // Inside record: child element
                        child_stack.push(tag.clone());
                        let path = child_stack.join("/");

                        for attr in e.attributes().flatten() {
                            let k = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                            let v = attr.decode_and_unescape_value(xml_reader.decoder())
                                .map(|cow| cow.into_owned())
                                .unwrap_or_else(|_| String::from_utf8_lossy(attr.value.as_ref()).into_owned());
                            current_record_values.insert(format!("{}@{}", path, k), v.clone());
                            current_record_values.insert(format!("{}@{}", tag, k), v.clone());
                            current_record_values.insert(format!("@{}", k), v.clone());
                            current_record_values.insert(k, v);
                        }
                    }
                }
                Ok(Event::Text(ref t)) => {
                    if in_record && !child_stack.is_empty() {
                        let text = t.unescape()
                            .map(|cow| cow.into_owned())
                            .unwrap_or_else(|_| String::from_utf8_lossy(t.as_ref()).into_owned());
                        let path = child_stack.join("/");
                        let leaf = child_stack.last().unwrap().clone();
                        current_record_values.entry(path).or_default().push_str(&text);
                        if leaf != child_stack.join("/") {
                            current_record_values.entry(leaf).or_default().push_str(&text);
                        }
                    }
                }
                Ok(Event::CData(ref c)) => {
                    if in_record && !child_stack.is_empty() {
                        let text = String::from_utf8_lossy(c.as_ref()).into_owned();
                        let path = child_stack.join("/");
                        let leaf = child_stack.last().unwrap().clone();
                        current_record_values.entry(path).or_default().push_str(&text);
                        if leaf != child_stack.join("/") {
                            current_record_values.entry(leaf).or_default().push_str(&text);
                        }
                    }
                }
                Ok(Event::End(ref e)) => {
                    let raw_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let tag = raw_name.split(':').last().unwrap_or(&raw_name).to_string();

                    if in_record {
                        if tag.eq_ignore_ascii_case(clean_record_tag) && (child_stack.is_empty() || child_stack.iter().all(|s| !s.eq_ignore_ascii_case(clean_record_tag))) {
                            // Record complete! Output row
                            let row = fields
                                .iter()
                                .map(|f| {
                                    let val = Self::get_field_value(&current_record_values, f, clean_record_tag).unwrap_or("");
                                    Self::escape_csv_field(val.trim(), delimiter)
                                })
                                .collect::<Vec<_>>()
                                .join(&delim_str);

                            out_writer.write_all(row.as_bytes()).map_err(|e| e.to_string())?;
                            out_writer.write_all(b"\n").map_err(|e| e.to_string())?;

                            records_count += 1;
                            in_record = false;
                            child_stack.clear();
                            current_record_values.clear();
                        } else {
                            if let Some(pos) = child_stack.iter().rposition(|s| s.eq_ignore_ascii_case(&tag)) {
                                child_stack.truncate(pos);
                            } else if !child_stack.is_empty() {
                                child_stack.pop();
                            }
                        }

                        if last_report.elapsed().as_millis() >= 100 {
                            last_report = std::time::Instant::now();
                            if let Some(ref tx) = progress_tx {
                                let _ = tx.try_send(ExtractionProgress {
                                    records_extracted: records_count,
                                    bytes_processed: xml_reader.buffer_position() as u64,
                                    total_bytes,
                                    is_finished: false,
                                });
                            }
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(format!("XML error: {}", e)),
                _ => {}
            }
        }

        out_writer.flush().map_err(|e| e.to_string())?;

        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(ExtractionProgress {
                records_extracted: records_count,
                bytes_processed: total_bytes,
                total_bytes,
                is_finished: true,
            });
        }

        Ok(records_count)
    }

    /// Extracts fields from a JSON array of objects and writes CSV rows.
    pub fn extract_json<R: Read, W: Write>(
        reader: R,
        writer: &mut W,
        fields: &[String],
        delimiter: u8,
        include_headers: bool,
        cancel: Arc<AtomicBool>,
        total_bytes: u64,
        progress_tx: Option<Sender<ExtractionProgress>>,
    ) -> Result<u64, String> {
        let mut buf_reader = std::io::BufReader::with_capacity(64 * 1024, reader);
        let delim_str = (delimiter as char).to_string();
        let mut out_writer = BufWriter::with_capacity(64 * 1024, writer);

        if include_headers {
            let header_row = fields
                .iter()
                .map(|f| Self::escape_csv_field(f, delimiter))
                .collect::<Vec<_>>()
                .join(&delim_str);
            out_writer.write_all(header_row.as_bytes()).map_err(|e| e.to_string())?;
            out_writer.write_all(b"\n").map_err(|e| e.to_string())?;
        }

        let mut in_string = false;
        let mut escaped = false;
        let mut cur_str = String::new();
        let mut cur_key: Option<String> = None;
        let mut cur_val = String::new();
        let mut cur_record: HashMap<String, String> = HashMap::new();
        let mut object_depth: usize = 0;
        let mut array_depth: usize = 0;
        let mut records_count: u64 = 0;
        let mut bytes_read: u64 = 0;
        let mut byte_buf = [0u8; 16 * 1024];
        let mut last_report = std::time::Instant::now();

        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Extraction cancelled by user".to_string());
            }

            let n = buf_reader.read(&mut byte_buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            bytes_read += n as u64;

            for &b in &byte_buf[..n] {
                if in_string {
                    if escaped {
                        escaped = false;
                        cur_str.push(b as char);
                    } else if b == b'\\' {
                        escaped = true;
                    } else if b == b'"' {
                        in_string = false;
                        if cur_key.is_none() {
                            cur_key = Some(std::mem::take(&mut cur_str));
                        } else {
                            cur_val = std::mem::take(&mut cur_str);
                        }
                    } else {
                        cur_str.push(b as char);
                    }
                    continue;
                }

                match b {
                    b'"' => {
                        in_string = true;
                        escaped = false;
                        cur_str.clear();
                    }
                    b'[' => {
                        array_depth += 1;
                    }
                    b']' => {
                        array_depth = array_depth.saturating_sub(1);
                    }
                    b'{' => {
                        object_depth += 1;
                        if object_depth == 1 || (array_depth >= 1 && object_depth == 1) {
                            cur_record.clear();
                        }
                    }
                    b'}' => {
                        if let (Some(k), val) = (cur_key.take(), cur_val.trim()) {
                            if !val.is_empty() {
                                cur_record.insert(k, val.to_string());
                            }
                        }
                        cur_val.clear();

                        let is_record = object_depth == 1 || (array_depth >= 1 && object_depth == 1);
                        if is_record && !cur_record.is_empty() {
                            let row = fields
                                .iter()
                                .map(|f| {
                                    let v = Self::get_field_value(&cur_record, f, "").unwrap_or("");
                                    Self::escape_csv_field(v.trim(), delimiter)
                                })
                                .collect::<Vec<_>>()
                                .join(&delim_str);

                            out_writer.write_all(row.as_bytes()).map_err(|e| e.to_string())?;
                            out_writer.write_all(b"\n").map_err(|e| e.to_string())?;
                            records_count += 1;
                            cur_record.clear();
                        }

                        object_depth = object_depth.saturating_sub(1);
                    }
                    b',' => {
                        if let (Some(k), val) = (cur_key.take(), cur_val.trim()) {
                            if !val.is_empty() {
                                cur_record.insert(k, val.to_string());
                            }
                        }
                        cur_val.clear();
                    }
                    b':' => {}
                    _ => {
                        if cur_key.is_some() && !b.is_ascii_whitespace() {
                            cur_val.push(b as char);
                        }
                    }
                }
            }

            if last_report.elapsed().as_millis() >= 100 {
                last_report = std::time::Instant::now();
                if let Some(ref tx) = progress_tx {
                    let _ = tx.try_send(ExtractionProgress {
                        records_extracted: records_count,
                        bytes_processed: bytes_read,
                        total_bytes,
                        is_finished: false,
                    });
                }
            }
        }

        out_writer.flush().map_err(|e| e.to_string())?;

        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(ExtractionProgress {
                records_extracted: records_count,
                bytes_processed: bytes_read,
                total_bytes,
                is_finished: true,
            });
        }

        Ok(records_count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_extract_xml_to_csv() {
        let xml = r#"<catalog>
            <book id="bk101"><title>Rust In Action</title><price>39.99</price></book>
            <book id="bk102"><title>Programming Rust</title><price>49.99</price></book>
        </catalog>"#;

        let mut out = Vec::new();
        let cancel = Arc::new(AtomicBool::new(false));
        let fields = vec!["@id".to_string(), "title".to_string(), "price".to_string()];

        let count = StreamingFieldExtractor::extract_xml(
            Cursor::new(xml.as_bytes()),
            &mut out,
            "book",
            &fields,
            b',',
            true,
            cancel,
            xml.len() as u64,
            None,
        ).unwrap();

        assert_eq!(count, 2);
        let csv = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "@id,title,price");
        assert_eq!(lines[1], "bk101,Rust In Action,39.99");
        assert_eq!(lines[2], "bk102,Programming Rust,49.99");
    }

    #[test]
    fn test_extract_json_to_csv() {
        let json = r#"[
            {"id": "1", "name": "Alice, Smith", "score": "95"},
            {"id": "2", "name": "Bob", "score": "88"}
        ]"#;

        let mut out = Vec::new();
        let cancel = Arc::new(AtomicBool::new(false));
        let fields = vec!["id".to_string(), "name".to_string(), "score".to_string()];

        let count = StreamingFieldExtractor::extract_json(
            Cursor::new(json.as_bytes()),
            &mut out,
            &fields,
            b',',
            true,
            cancel,
            json.len() as u64,
            None,
        ).unwrap();

        assert_eq!(count, 2);
        let csv = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "id,name,score");
        assert_eq!(lines[1], "1,\"Alice, Smith\",95");
        assert_eq!(lines[2], "2,Bob,88");
    }

    #[test]
    fn test_extract_complex_xml_with_cdata_empty_tags_and_prefixes() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<jobfeed>
   <provider>TTEC Holdings</provider>
   <jobs>
      <job>
         <refcode>04CQJ</refcode>
         <title>Customer Experience Specialist</title>
         <company>TTEC</company>
         <url><![CDATA[https://example.com/job1?a=1&b=2]]></url>
         <city>Melbourne</city>
         <postalcode/>
         <description><![CDATA[<p>Customer Experience Specialist (CXS) on-site</p>]]></description>
      </job>
      <job>
         <refcode>05XYZ</refcode>
         <title>Senior Architect</title>
         <company>Acme Corp</company>
         <url><![CDATA[https://example.com/job2]]></url>
         <city>Austin</city>
         <postalcode>78701</postalcode>
         <description><![CDATA[<p>Architect role</p>]]></description>
      </job>
   </jobs>
</jobfeed>"#;

        let mut out = Vec::new();
        let cancel = Arc::new(AtomicBool::new(false));
        // Test with prefix "job/refcode", different casing "Title", CDATA url, empty postalcode
        let fields = vec![
            "job/refcode".to_string(),
            "Title".to_string(),
            "company".to_string(),
            "url".to_string(),
            "postalcode".to_string(),
        ];

        let count = StreamingFieldExtractor::extract_xml(
            Cursor::new(xml.as_bytes()),
            &mut out,
            "job",
            &fields,
            b',',
            true,
            cancel,
            xml.len() as u64,
            None,
        ).unwrap();

        assert_eq!(count, 2);
        let csv = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "job/refcode,Title,company,url,postalcode");
        assert_eq!(lines[1], "04CQJ,Customer Experience Specialist,TTEC,https://example.com/job1?a=1&b=2,");
        assert_eq!(lines[2], "05XYZ,Senior Architect,Acme Corp,https://example.com/job2,78701");
    }
}
