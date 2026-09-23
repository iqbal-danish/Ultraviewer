use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatAction {
    Beautify { indent_size: usize, use_tabs: bool },
    Minify,
}

#[derive(Debug, Clone)]
pub struct FormattingProgress {
    pub bytes_processed: u64,
    pub total_bytes: u64,
    pub speed_mb_s: f64,
    pub progress_pct: f32,
    pub elapsed_secs: f64,
    pub eta_secs: Option<f64>,
    pub is_finished: bool,
    pub error: Option<String>,
}

impl FormattingProgress {
    pub fn new(total_bytes: u64) -> Self {
        Self {
            bytes_processed: 0,
            total_bytes,
            speed_mb_s: 0.0,
            progress_pct: 0.0,
            elapsed_secs: 0.0,
            eta_secs: None,
            is_finished: false,
            error: None,
        }
    }

    pub fn update(&mut self, processed: u64, start: Instant) {
        self.bytes_processed = processed;
        let elapsed = start.elapsed().as_secs_f64();
        self.elapsed_secs = elapsed;

        if elapsed > 0.05 {
            let mb = (processed as f64) / (1024.0 * 1024.0);
            self.speed_mb_s = mb / elapsed;

            if self.total_bytes > 0 {
                self.progress_pct = ((processed as f64 / self.total_bytes as f64) * 100.0).clamp(0.0, 100.0) as f32;
                let remaining_bytes = self.total_bytes.saturating_sub(processed);
                if self.speed_mb_s > 0.1 {
                    let remaining_mb = remaining_bytes as f64 / (1024.0 * 1024.0);
                    self.eta_secs = Some(remaining_mb / self.speed_mb_s);
                }
            }
        }
    }

    pub fn finish(&mut self, total_bytes: u64, start: Instant) {
        self.update(total_bytes, start);
        self.progress_pct = 100.0;
        self.is_finished = true;
        self.eta_secs = Some(0.0);
    }
}

pub struct JsonStreamingFormatter;

impl JsonStreamingFormatter {
    const BUFFER_CAPACITY: usize = 256 * 1024;

    /// Formats a JSON stream from reader to writer without building a DOM.
    pub fn format<R: Read, W: Write>(
        mut reader: R,
        mut writer: W,
        action: FormatAction,
        cancel: Arc<AtomicBool>,
        total_bytes: u64,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<u64, String> {
        let start_time = Instant::now();
        let mut total_processed: u64 = 0;
        let mut in_buffer = vec![0u8; Self::BUFFER_CAPACITY];
        let mut last_progress_report = Instant::now();

        if let Some(ref prog) = progress {
            prog.write().unwrap().total_bytes = total_bytes;
        }

        match action {
            FormatAction::Minify => {
                let mut in_string = false;
                let mut escaped = false;

                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Formatting cancelled by user".to_string());
                    }

                    let bytes_read = reader.read(&mut in_buffer).map_err(|e| e.to_string())?;
                    if bytes_read == 0 {
                        break;
                    }

                    total_processed += bytes_read as u64;

                    let slice = &in_buffer[..bytes_read];
                    let mut start_idx = 0;

                    for i in 0..bytes_read {
                        let b = slice[i];

                        if in_string {
                            if escaped {
                                escaped = false;
                            } else if b == b'\\' {
                                escaped = true;
                            } else if b == b'"' {
                                in_string = false;
                            }
                        } else {
                            if b == b'"' {
                                in_string = true;
                                escaped = false;
                            } else if b.is_ascii_whitespace() {
                                if i > start_idx {
                                    writer.write_all(&slice[start_idx..i]).map_err(|e| e.to_string())?;
                                }
                                start_idx = i + 1;
                            }
                        }
                    }

                    if start_idx < bytes_read {
                        writer.write_all(&slice[start_idx..bytes_read]).map_err(|e| e.to_string())?;
                    }

                    if last_progress_report.elapsed().as_millis() >= 100 {
                        if let Some(ref prog) = progress {
                            prog.write().unwrap().update(total_processed, start_time);
                        }
                        last_progress_report = Instant::now();
                    }
                }
            }
            FormatAction::Beautify { indent_size, use_tabs } => {
                let indent_unit: Vec<u8> = if use_tabs {
                    vec![b'\t']
                } else {
                    vec![b' '; indent_size.max(1).min(8)]
                };

                let mut depth: usize = 0;
                let mut pending_open: Option<u8> = None;
                let mut in_string = false;
                let mut escaped = false;

                fn write_indent<W: Write>(w: &mut W, depth: usize, unit: &[u8]) -> Result<(), String> {
                    for _ in 0..depth {
                        w.write_all(unit).map_err(|e| e.to_string())?;
                    }
                    Ok(())
                }

                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Formatting cancelled by user".to_string());
                    }

                    let bytes_read = reader.read(&mut in_buffer).map_err(|e| e.to_string())?;
                    if bytes_read == 0 {
                        break;
                    }

                    total_processed += bytes_read as u64;
                    let slice = &in_buffer[..bytes_read];

                    for &b in slice {
                        if in_string {
                            writer.write_all(&[b]).map_err(|e| e.to_string())?;
                            if escaped {
                                escaped = false;
                            } else if b == b'\\' {
                                escaped = true;
                            } else if b == b'"' {
                                in_string = false;
                            }
                            continue;
                        }

                        if b.is_ascii_whitespace() {
                            continue;
                        }

                        // Check pending opening bracket
                        if let Some(op) = pending_open.take() {
                            if (op == b'{' && b == b'}') || (op == b'[' && b == b']') {
                                // Empty container
                                writer.write_all(&[op, b]).map_err(|e| e.to_string())?;
                                continue;
                            } else {
                                // Non-empty container
                                writer.write_all(&[op, b'\n']).map_err(|e| e.to_string())?;
                                depth += 1;
                                write_indent(&mut writer, depth, &indent_unit)?;
                                // Fall through to process character `b`
                            }
                        }

                        match b {
                            b'{' | b'[' => {
                                pending_open = Some(b);
                            }
                            b'}' | b']' => {
                                depth = depth.saturating_sub(1);
                                writer.write_all(b"\n").map_err(|e| e.to_string())?;
                                write_indent(&mut writer, depth, &indent_unit)?;
                                writer.write_all(&[b]).map_err(|e| e.to_string())?;
                            }
                            b':' => {
                                writer.write_all(b": ").map_err(|e| e.to_string())?;
                            }
                            b',' => {
                                writer.write_all(b",\n").map_err(|e| e.to_string())?;
                                write_indent(&mut writer, depth, &indent_unit)?;
                            }
                            b'"' => {
                                in_string = true;
                                escaped = false;
                                writer.write_all(&[b]).map_err(|e| e.to_string())?;
                            }
                            _ => {
                                writer.write_all(&[b]).map_err(|e| e.to_string())?;
                            }
                        }
                    }

                    if last_progress_report.elapsed().as_millis() >= 100 {
                        if let Some(ref prog) = progress {
                            prog.write().unwrap().update(total_processed, start_time);
                        }
                        last_progress_report = Instant::now();
                    }
                }

                if let Some(op) = pending_open {
                    writer.write_all(&[op]).map_err(|e| e.to_string())?;
                }
                writer.write_all(b"\n").map_err(|e| e.to_string())?;
            }
        }

        writer.flush().map_err(|e| e.to_string())?;

        if let Some(ref prog) = progress {
            let mut p = prog.write().unwrap();
            p.update(total_processed, start_time);
            p.is_finished = true;
            p.progress_pct = 100.0;
        }

        Ok(total_processed)
    }

    /// Formats a JSON file from disk to an output path.
    pub fn format_file<P: AsRef<Path>, Q: AsRef<Path>>(
        src_path: P,
        dst_path: Q,
        action: FormatAction,
        cancel: Arc<AtomicBool>,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<u64, String> {
        let src_file = File::open(src_path.as_ref()).map_err(|e| format!("Failed to open source file: {}", e))?;
        let metadata = src_file.metadata().map_err(|e| e.to_string())?;
        let total_bytes = metadata.len();

        if let Some(ref prog) = progress {
            *prog.write().unwrap() = FormattingProgress::new(total_bytes);
        }

        let dst_file = File::create(dst_path.as_ref()).map_err(|e| format!("Failed to create destination file: {}", e))?;
        let reader = BufReader::with_capacity(Self::BUFFER_CAPACITY, src_file);
        let writer = BufWriter::with_capacity(Self::BUFFER_CAPACITY, dst_file);

        let res = Self::format(reader, writer, action, Arc::clone(&cancel), total_bytes, progress.clone());

        if let Err(ref err) = res {
            let _ = std::fs::remove_file(dst_path.as_ref());
            if let Some(ref prog) = progress {
                prog.write().unwrap().error = Some(err.clone());
            }
        }

        res
    }
}

pub fn clean_xml_decl(raw: &[u8]) -> Vec<u8> {
    let mut s = raw;
    while !s.is_empty() && s[0].is_ascii_whitespace() {
        s = &s[1..];
    }
    // Repeatedly strip any leading "xml" or "XML" tokens and subsequent whitespace
    while s.len() >= 3 && s[..3].eq_ignore_ascii_case(b"xml") {
        if s.len() == 3 || s[3].is_ascii_whitespace() || s[3] == b'?' {
            s = &s[3..];
            while !s.is_empty() && s[0].is_ascii_whitespace() {
                s = &s[1..];
            }
        } else {
            break;
        }
    }
    // Trim trailing '?' or whitespace
    while !s.is_empty() && (s[s.len() - 1].is_ascii_whitespace() || s[s.len() - 1] == b'?') {
        s = &s[..s.len() - 1];
    }
    let mut out = Vec::with_capacity(s.len() + 10);
    out.extend_from_slice(b"<?xml");
    if !s.is_empty() {
        out.push(b' ');
        out.extend_from_slice(s);
    }
    out.extend_from_slice(b"?>\n");
    out
}

pub struct XmlStreamingFormatter;

impl XmlStreamingFormatter {
    const BUFFER_CAPACITY: usize = 256 * 1024;

    /// Formats an XML stream from reader to writer without building a DOM.
    pub fn format<R: Read, W: Write>(
        reader: R,
        mut writer: W,
        action: FormatAction,
        cancel: Arc<AtomicBool>,
        total_bytes: u64,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<u64, String> {
        let start_time = Instant::now();
        let buf_reader = BufReader::with_capacity(Self::BUFFER_CAPACITY, reader);
        let mut xml_reader = Reader::from_reader(buf_reader);
        xml_reader.config_mut().expand_empty_elements = false;
        xml_reader.config_mut().trim_text(false);

        let mut buf = Vec::with_capacity(8192);
        let mut last_progress_report = Instant::now();
        let mut last_pos: u64 = 0;

        if let Some(ref prog) = progress {
            prog.write().unwrap().total_bytes = total_bytes;
        }

        match action {
            FormatAction::Minify => {
                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Formatting cancelled by user".to_string());
                    }

                    match xml_reader.read_event_into(&mut buf) {
                        Ok(Event::Decl(d)) => {
                            let decl = clean_xml_decl(d.as_ref());
                            writer.write_all(&decl).map_err(|e| e.to_string())?;
                        }
                        Ok(Event::Start(e)) => {
                            writer.write_all(b"<").map_err(|e| e.to_string())?;
                            writer.write_all(e.name().as_ref()).map_err(|e| e.to_string())?;
                            for attr in e.attributes() {
                                if let Ok(a) = attr {
                                    writer.write_all(b" ").map_err(|e| e.to_string())?;
                                    writer.write_all(a.key.as_ref()).map_err(|e| e.to_string())?;
                                    writer.write_all(b"=\"").map_err(|e| e.to_string())?;
                                    writer.write_all(a.value.as_ref()).map_err(|e| e.to_string())?;
                                    writer.write_all(b"\"").map_err(|e| e.to_string())?;
                                }
                            }
                            writer.write_all(b">").map_err(|e| e.to_string())?;
                        }
                        Ok(Event::Empty(e)) => {
                            writer.write_all(b"<").map_err(|e| e.to_string())?;
                            writer.write_all(e.name().as_ref()).map_err(|e| e.to_string())?;
                            for attr in e.attributes() {
                                if let Ok(a) = attr {
                                    writer.write_all(b" ").map_err(|e| e.to_string())?;
                                    writer.write_all(a.key.as_ref()).map_err(|e| e.to_string())?;
                                    writer.write_all(b"=\"").map_err(|e| e.to_string())?;
                                    writer.write_all(a.value.as_ref()).map_err(|e| e.to_string())?;
                                    writer.write_all(b"\"").map_err(|e| e.to_string())?;
                                }
                            }
                            writer.write_all(b"/>").map_err(|e| e.to_string())?;
                        }
                        Ok(Event::End(e)) => {
                            writer.write_all(b"</").map_err(|e| e.to_string())?;
                            writer.write_all(e.name().as_ref()).map_err(|e| e.to_string())?;
                            writer.write_all(b">").map_err(|e| e.to_string())?;
                        }
                        Ok(Event::Text(t)) => {
                            let text = t.as_ref();
                            if !text.iter().all(|b| b.is_ascii_whitespace()) {
                                writer.write_all(text).map_err(|e| e.to_string())?;
                            }
                        }
                        Ok(Event::CData(c)) => {
                            writer.write_all(b"<![CDATA[").map_err(|e| e.to_string())?;
                            writer.write_all(c.as_ref()).map_err(|e| e.to_string())?;
                            writer.write_all(b"]]>").map_err(|e| e.to_string())?;
                        }
                        Ok(Event::Comment(c)) => {
                            writer.write_all(b"<!--").map_err(|e| e.to_string())?;
                            writer.write_all(c.as_ref()).map_err(|e| e.to_string())?;
                            writer.write_all(b"-->").map_err(|e| e.to_string())?;
                        }
                        Ok(Event::Eof) => break,
                        Err(e) => return Err(format!("XML error at offset {}: {}", xml_reader.buffer_position(), e)),
                        _ => {}
                    }

                    buf.clear();

                    if last_progress_report.elapsed().as_millis() >= 100 {
                        let pos = xml_reader.buffer_position() as u64;
                        last_pos = pos;
                        if let Some(ref prog) = progress {
                            prog.write().unwrap().update(pos, start_time);
                        }
                        last_progress_report = Instant::now();
                    }
                }
            }
            FormatAction::Beautify { indent_size, use_tabs } => {
                let indent_unit: Vec<u8> = if use_tabs {
                    vec![b'\t']
                } else {
                    vec![b' '; indent_size.max(1).min(8)]
                };

                let mut depth: usize = 0;
                let mut prev_was_text = false;
                let mut pending_start: Option<(usize, Vec<u8>)> = None;

                fn write_indent<W: Write>(w: &mut W, depth: usize, unit: &[u8]) -> Result<(), String> {
                    for _ in 0..depth {
                        w.write_all(unit).map_err(|e| e.to_string())?;
                    }
                    Ok(())
                }

                fn flush_container<W: Write>(
                    w: &mut W,
                    pending: &mut Option<(usize, Vec<u8>)>,
                    unit: &[u8],
                ) -> Result<(), String> {
                    if let Some((d, tag)) = pending.take() {
                        write_indent(w, d, unit)?;
                        w.write_all(&tag).map_err(|e| e.to_string())?;
                        w.write_all(b"\n").map_err(|e| e.to_string())?;
                    }
                    Ok(())
                }

                loop {
                    if cancel.load(Ordering::Relaxed) {
                        return Err("Formatting cancelled by user".to_string());
                    }

                    match xml_reader.read_event_into(&mut buf) {
                        Ok(Event::Decl(d)) => {
                            flush_container(&mut writer, &mut pending_start, &indent_unit)?;
                            let decl = clean_xml_decl(d.as_ref());
                            writer.write_all(&decl).map_err(|e| e.to_string())?;
                            prev_was_text = false;
                        }
                        Ok(Event::Start(e)) => {
                            flush_container(&mut writer, &mut pending_start, &indent_unit)?;
                            let mut tag_bytes = Vec::with_capacity(48);
                            tag_bytes.extend_from_slice(b"<");
                            tag_bytes.extend_from_slice(e.name().as_ref());
                            for attr in e.attributes() {
                                if let Ok(a) = attr {
                                    tag_bytes.extend_from_slice(b" ");
                                    tag_bytes.extend_from_slice(a.key.as_ref());
                                    tag_bytes.extend_from_slice(b"=\"");
                                    tag_bytes.extend_from_slice(a.value.as_ref());
                                    tag_bytes.extend_from_slice(b"\"");
                                }
                            }
                            tag_bytes.extend_from_slice(b">");

                            pending_start = Some((depth, tag_bytes));
                            depth += 1;
                            prev_was_text = false;
                        }
                        Ok(Event::Empty(e)) => {
                            flush_container(&mut writer, &mut pending_start, &indent_unit)?;
                            write_indent(&mut writer, depth, &indent_unit)?;
                            writer.write_all(b"<").map_err(|e| e.to_string())?;
                            writer.write_all(e.name().as_ref()).map_err(|e| e.to_string())?;
                            for attr in e.attributes() {
                                if let Ok(a) = attr {
                                    writer.write_all(b" ").map_err(|e| e.to_string())?;
                                    writer.write_all(a.key.as_ref()).map_err(|e| e.to_string())?;
                                    writer.write_all(b"=\"").map_err(|e| e.to_string())?;
                                    writer.write_all(a.value.as_ref()).map_err(|e| e.to_string())?;
                                    writer.write_all(b"\"").map_err(|e| e.to_string())?;
                                }
                            }
                            writer.write_all(b"/>\n").map_err(|e| e.to_string())?;
                            prev_was_text = false;
                        }
                        Ok(Event::End(e)) => {
                            depth = depth.saturating_sub(1);
                            if let Some((d, tag)) = pending_start.take() {
                                // Empty element like <item></item>
                                write_indent(&mut writer, d, &indent_unit)?;
                                writer.write_all(&tag).map_err(|e| e.to_string())?;
                                writer.write_all(b"</").map_err(|e| e.to_string())?;
                                writer.write_all(e.name().as_ref()).map_err(|e| e.to_string())?;
                                writer.write_all(b">\n").map_err(|e| e.to_string())?;
                                prev_was_text = false;
                            } else if prev_was_text {
                                // Closing tag of a leaf text element: stays on same line
                                writer.write_all(b"</").map_err(|e| e.to_string())?;
                                writer.write_all(e.name().as_ref()).map_err(|e| e.to_string())?;
                                writer.write_all(b">\n").map_err(|e| e.to_string())?;
                                prev_was_text = false;
                            } else {
                                // Closing tag of a container element
                                write_indent(&mut writer, depth, &indent_unit)?;
                                writer.write_all(b"</").map_err(|e| e.to_string())?;
                                writer.write_all(e.name().as_ref()).map_err(|e| e.to_string())?;
                                writer.write_all(b">\n").map_err(|e| e.to_string())?;
                                prev_was_text = false;
                            }
                        }
                        Ok(Event::Text(t)) => {
                            let text = t.as_ref();
                            if !text.iter().all(|b| b.is_ascii_whitespace()) {
                                if let Some((d, tag)) = pending_start.take() {
                                    write_indent(&mut writer, d, &indent_unit)?;
                                    writer.write_all(&tag).map_err(|e| e.to_string())?;
                                }
                                writer.write_all(text).map_err(|e| e.to_string())?;
                                prev_was_text = true;
                            }
                        }
                        Ok(Event::CData(c)) => {
                            if let Some((d, tag)) = pending_start.take() {
                                write_indent(&mut writer, d, &indent_unit)?;
                                writer.write_all(&tag).map_err(|e| e.to_string())?;
                            }
                            writer.write_all(b"<![CDATA[").map_err(|e| e.to_string())?;
                            writer.write_all(c.as_ref()).map_err(|e| e.to_string())?;
                            writer.write_all(b"]]>").map_err(|e| e.to_string())?;
                            prev_was_text = true;
                        }
                        Ok(Event::Comment(c)) => {
                            flush_container(&mut writer, &mut pending_start, &indent_unit)?;
                            write_indent(&mut writer, depth, &indent_unit)?;
                            writer.write_all(b"<!--").map_err(|e| e.to_string())?;
                            writer.write_all(c.as_ref()).map_err(|e| e.to_string())?;
                            writer.write_all(b"-->\n").map_err(|e| e.to_string())?;
                            prev_was_text = false;
                        }
                        Ok(Event::Eof) => {
                            flush_container(&mut writer, &mut pending_start, &indent_unit)?;
                            break;
                        }
                        Err(e) => return Err(format!("XML error at offset {}: {}", xml_reader.buffer_position(), e)),
                        _ => {}
                    }

                    buf.clear();

                    if last_progress_report.elapsed().as_millis() >= 100 {
                        let pos = xml_reader.buffer_position() as u64;
                        last_pos = pos;
                        if let Some(ref prog) = progress {
                            prog.write().unwrap().update(pos, start_time);
                        }
                        last_progress_report = Instant::now();
                    }
                }
            }
        }

        writer.flush().map_err(|e| e.to_string())?;
        let final_pos = xml_reader.buffer_position() as u64;
        let processed = if final_pos > 0 { final_pos } else { last_pos };

        if let Some(ref prog) = progress {
            let mut p = prog.write().unwrap();
            p.update(processed, start_time);
            p.is_finished = true;
            p.progress_pct = 100.0;
        }

        Ok(processed)
    }

    /// Formats an XML file from disk to an output path.
    pub fn format_file<P: AsRef<Path>, Q: AsRef<Path>>(
        src_path: P,
        dst_path: Q,
        action: FormatAction,
        cancel: Arc<AtomicBool>,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<u64, String> {
        let src_file = File::open(src_path.as_ref()).map_err(|e| format!("Failed to open source file: {}", e))?;
        let metadata = src_file.metadata().map_err(|e| e.to_string())?;
        let total_bytes = metadata.len();

        if let Some(ref prog) = progress {
            *prog.write().unwrap() = FormattingProgress::new(total_bytes);
        }

        let dst_file = File::create(dst_path.as_ref()).map_err(|e| format!("Failed to create destination file: {}", e))?;
        let reader = BufReader::with_capacity(Self::BUFFER_CAPACITY, src_file);
        let writer = BufWriter::with_capacity(Self::BUFFER_CAPACITY, dst_file);

        let res = Self::format(reader, writer, action, Arc::clone(&cancel), total_bytes, progress.clone());

        if let Err(ref err) = res {
            let _ = std::fs::remove_file(dst_path.as_ref());
            if let Some(ref prog) = progress {
                prog.write().unwrap().error = Some(err.clone());
            }
        }

        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_json_beautify_and_minify() {
        let input = r#"{"name":"UltraViewer","version":1,"features":["stream","search",{"enabled":true}]}"#;
        let mut beautified = Vec::new();
        let cancel = Arc::new(AtomicBool::new(false));

        JsonStreamingFormatter::format(
            Cursor::new(input.as_bytes()),
            &mut beautified,
            FormatAction::Beautify { indent_size: 2, use_tabs: false },
            Arc::clone(&cancel),
            input.len() as u64,
            None,
        ).unwrap();

        let beautified_str = String::from_utf8(beautified).unwrap();
        assert!(beautified_str.contains("  \"name\": \"UltraViewer\","));
        assert!(beautified_str.contains("  \"features\": ["));

        let mut minified = Vec::new();
        JsonStreamingFormatter::format(
            Cursor::new(beautified_str.as_bytes()),
            &mut minified,
            FormatAction::Minify,
            cancel,
            beautified_str.len() as u64,
            None,
        ).unwrap();

        let minified_str = String::from_utf8(minified).unwrap();
        assert_eq!(minified_str.trim(), input);
    }

    #[test]
    fn test_xml_beautify_and_minify() {
        let input = r#"<catalog id="cat1"><book id="b1"><title>Rust Async</title><price>39.99</price></book><book id="b2"><title>High Perf</title><price>49.99</price></book></catalog>"#;
        let mut beautified = Vec::new();
        let cancel = Arc::new(AtomicBool::new(false));

        XmlStreamingFormatter::format(
            Cursor::new(input.as_bytes()),
            &mut beautified,
            FormatAction::Beautify { indent_size: 2, use_tabs: false },
            Arc::clone(&cancel),
            input.len() as u64,
            None,
        ).unwrap();

        let beautified_str = String::from_utf8(beautified).unwrap();
        assert!(beautified_str.contains("<catalog id=\"cat1\">"));
        assert!(beautified_str.contains("  <book id=\"b1\">"));
        assert!(beautified_str.contains("    <title>Rust Async</title>"));

        let mut minified = Vec::new();
        XmlStreamingFormatter::format(
            Cursor::new(beautified_str.as_bytes()),
            &mut minified,
            FormatAction::Minify,
            cancel,
            beautified_str.len() as u64,
            None,
        ).unwrap();

        let minified_str = String::from_utf8(minified).unwrap();
        assert_eq!(minified_str.trim(), input);
    }

    #[test]
    fn test_xml_cdata_and_empty_elements() {
        let input = r#"<root><item id="1"/><data><![CDATA[Some <b>HTML</b> content]]></data></root>"#;
        let mut beautified = Vec::new();
        let cancel = Arc::new(AtomicBool::new(false));

        XmlStreamingFormatter::format(
            Cursor::new(input.as_bytes()),
            &mut beautified,
            FormatAction::Beautify { indent_size: 2, use_tabs: false },
            cancel,
            input.len() as u64,
            None,
        ).unwrap();

        let beautified_str = String::from_utf8(beautified).unwrap();
        assert!(beautified_str.contains("<item id=\"1\"/>"));
        assert!(beautified_str.contains("<![CDATA[Some <b>HTML</b> content]]>"));
    }

    #[test]
    fn test_clean_xml_decl_edge_cases() {
        assert_eq!(
            clean_xml_decl(b"xml version='1.0' encoding='UTF-8'"),
            b"<?xml version='1.0' encoding='UTF-8'?>\n"
        );
        // Strips multiple duplicate xml tokens (the user-reported bug)
        assert_eq!(
            clean_xml_decl(b"xml xml xml xml xml xml version='1.0' encoding='UTF-8'"),
            b"<?xml version='1.0' encoding='UTF-8'?>\n"
        );
        assert_eq!(
            clean_xml_decl(b"xml"),
            b"<?xml?>\n"
        );
        assert_eq!(
            clean_xml_decl(b"version='1.0'"),
            b"<?xml version='1.0'?>\n"
        );
    }

    #[test]
    fn test_xml_formatting_never_duplicates_declaration() {
        let input = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<root><item>Hello</item></root>";
        let mut first_pass = Vec::new();
        let cancel = Arc::new(AtomicBool::new(false));

        XmlStreamingFormatter::format(
            Cursor::new(input.as_bytes()),
            &mut first_pass,
            FormatAction::Beautify { indent_size: 2, use_tabs: false },
            Arc::clone(&cancel),
            input.len() as u64,
            None,
        ).unwrap();

        let first_str = String::from_utf8(first_pass).unwrap();
        assert!(first_str.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
        assert!(!first_str.contains("<?xml xml"));

        // Second pass: format again, ensure it STILL has exactly one <?xml
        let mut second_pass = Vec::new();
        XmlStreamingFormatter::format(
            Cursor::new(first_str.as_bytes()),
            &mut second_pass,
            FormatAction::Beautify { indent_size: 2, use_tabs: false },
            cancel,
            first_str.len() as u64,
            None,
        ).unwrap();

        let second_str = String::from_utf8(second_pass).unwrap();
        assert!(second_str.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"));
        assert!(!second_str.contains("<?xml xml"));
    }

    #[test]
    fn test_xml_formatting_repairs_multiple_xml_declaration() {
        let input = "<?xml xml xml xml xml xml version='1.0' encoding='UTF-8'?>\n<source><job>Developer</job></source>";
        let mut out = Vec::new();
        let cancel = Arc::new(AtomicBool::new(false));

        XmlStreamingFormatter::format(
            Cursor::new(input.as_bytes()),
            &mut out,
            FormatAction::Beautify { indent_size: 2, use_tabs: false },
            cancel,
            input.len() as u64,
            None,
        ).unwrap();

        let out_str = String::from_utf8(out).unwrap();
        assert!(out_str.starts_with("<?xml version='1.0' encoding='UTF-8'?>\n"));
        assert!(!out_str.contains("xml xml"));
        assert!(out_str.contains("  <job>Developer</job>"));
    }
}

