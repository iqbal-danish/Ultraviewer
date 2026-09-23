use std::fs::File;
use std::io::{BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use eframe::egui::Color32;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::file_engine::FileEngine;
use crate::file_engine::line_index::LineIndex;

struct LineTrackingReader<R: Read> {
    inner: R,
    current_byte: u64,
    current_line: usize,
    checkpoints: Arc<Mutex<Vec<(u64, usize)>>>,
    last_checkpoint_byte: u64,
}

impl<R: Read> LineTrackingReader<R> {
    fn new(inner: R, checkpoints: Arc<Mutex<Vec<(u64, usize)>>>) -> Self {
        checkpoints.lock().unwrap().push((0, 1));
        Self {
            inner,
            current_byte: 0,
            current_line: 1,
            checkpoints,
            last_checkpoint_byte: 0,
        }
    }
}

impl<R: Read> Read for LineTrackingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            let mut line = self.current_line;
            let mut byte = self.current_byte;
            let mut last_cp = self.last_checkpoint_byte;
            let mut new_cps = Vec::new();

            for &b in &buf[..n] {
                byte += 1;
                if b == b'\n' {
                    line += 1;
                    if byte - last_cp >= 65536 {
                        new_cps.push((byte, line));
                        last_cp = byte;
                    }
                }
            }

            self.current_line = line;
            self.current_byte = byte;
            self.last_checkpoint_byte = last_cp;

            if !new_cps.is_empty() {
                if let Ok(mut cps) = self.checkpoints.lock() {
                    cps.extend(new_cps);
                }
            }
        }
        Ok(n)
    }
}

#[derive(Debug, Clone)]
pub struct XmlHighlightSpan {
    pub text: String,
    pub color: Color32,
}

pub struct XmlSyntaxHighlighter;

impl XmlSyntaxHighlighter {
    pub fn highlight_line(line: &str) -> Vec<XmlHighlightSpan> {
        Self::highlight_line_themed(line, true)
    }

    pub fn highlight_line_themed(line: &str, dark_mode: bool) -> Vec<XmlHighlightSpan> {
        let mut spans = Vec::new();
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut idx = 0;

        let (col_tag, col_elem, col_attr, col_val, col_comment, col_cdata, col_text) = if dark_mode {
            (
                Color32::from_rgb(171, 178, 191), // Brackets < > / in #ABB2BF
                Color32::from_rgb(224, 108, 117), // Element tag names in #E06C75 (Coral Red)
                Color32::from_rgb(209, 154, 102), // Attribute names in #D19A66 (Orange)
                Color32::from_rgb(152, 195, 121), // Attribute values in #98C379 (Green)
                Color32::from_rgb(92, 99, 112),   // Comments in #5C6370 (Muted Slate)
                Color32::from_rgb(152, 195, 121), // CDATA content in #98C379 (Green)
                Color32::from_rgb(171, 178, 191), // Text content in #ABB2BF (Silver)
            )
        } else {
            (
                Color32::from_rgb(0, 0, 220),     // Deep Blue
                Color32::from_rgb(163, 21, 21),   // Dark Red (Element)
                Color32::from_rgb(200, 0, 0),     // Red (Attribute)
                Color32::from_rgb(0, 0, 180),     // Blue (Value)
                Color32::from_rgb(0, 128, 0),     // Green (Comment)
                Color32::from_rgb(140, 90, 0),    // Dark Gold (CDATA)
                Color32::from_rgb(30, 30, 30),    // Dark Charcoal (Text)
            )
        };

        while idx < len {
            // Check for comments
            if idx + 4 <= len && &bytes[idx..idx + 4] == b"<!--" {
                let start = idx;
                idx += 4;
                while idx + 3 <= len && &bytes[idx..idx + 3] != b"-->" {
                    idx += 1;
                }
                if idx + 3 <= len {
                    idx += 3;
                } else {
                    idx = len;
                }
                spans.push(XmlHighlightSpan {
                    text: line[start..idx].to_string(),
                    color: col_comment,
                });
                continue;
            }

            // Check for CDATA
            if idx + 9 <= len && &bytes[idx..idx + 9] == b"<![CDATA[" {
                let start = idx;
                idx += 9;
                while idx + 3 <= len && &bytes[idx..idx + 3] != b"]]>" {
                    idx += 1;
                }
                if idx + 3 <= len {
                    idx += 3;
                } else {
                    idx = len;
                }
                spans.push(XmlHighlightSpan {
                    text: line[start..idx].to_string(),
                    color: col_cdata,
                });
                continue;
            }

            // Check for tags
            if bytes[idx] == b'<' {
                let _tag_start = idx;
                idx += 1;

                let is_closing = idx < len && bytes[idx] == b'/';
                if is_closing {
                    idx += 1;
                }
                let is_decl = idx < len && bytes[idx] == b'?';
                if is_decl {
                    idx += 1;
                }

                // Element name
                let name_start = idx;
                while idx < len && (bytes[idx].is_ascii_alphanumeric() || bytes[idx] == b':' || bytes[idx] == b'_' || bytes[idx] == b'-') {
                    idx += 1;
                }
                let name_end = idx;

                // Push opening bracket
                let prefix = if is_decl { "<?" } else if is_closing { "</" } else { "<" };
                spans.push(XmlHighlightSpan {
                    text: prefix.to_string(),
                    color: col_tag,
                });

                // Push element name
                if name_end > name_start {
                    spans.push(XmlHighlightSpan {
                        text: line[name_start..name_end].to_string(),
                        color: col_elem,
                    });
                }

                // Parse attributes inside tag until '>'
                while idx < len && bytes[idx] != b'>' {
                    if bytes[idx].is_ascii_whitespace() {
                        let ws_start = idx;
                        while idx < len && bytes[idx].is_ascii_whitespace() {
                            idx += 1;
                        }
                        spans.push(XmlHighlightSpan {
                            text: line[ws_start..idx].to_string(),
                            color: col_text,
                        });
                        continue;
                    }

                    if bytes[idx] == b'/' || bytes[idx] == b'?' {
                        spans.push(XmlHighlightSpan {
                            text: (bytes[idx] as char).to_string(),
                            color: col_tag,
                        });
                        idx += 1;
                        continue;
                    }

                    // Attribute name
                    let attr_start = idx;
                    while idx < len && bytes[idx] != b'=' && !bytes[idx].is_ascii_whitespace() && bytes[idx] != b'>' && bytes[idx] != b'/' {
                        idx += 1;
                    }
                    if idx > attr_start {
                        spans.push(XmlHighlightSpan {
                            text: line[attr_start..idx].to_string(),
                            color: col_attr,
                        });
                    }

                    // Equals sign
                    if idx < len && bytes[idx] == b'=' {
                        spans.push(XmlHighlightSpan {
                            text: "=".to_string(),
                            color: col_tag,
                        });
                        idx += 1;
                    }

                    // Attribute value
                    if idx < len && (bytes[idx] == b'"' || bytes[idx] == b'\'') {
                        let quote = bytes[idx];
                        let val_start = idx;
                        idx += 1;
                        while idx < len && bytes[idx] != quote {
                            idx += 1;
                        }
                        if idx < len && bytes[idx] == quote {
                            idx += 1;
                        }
                        spans.push(XmlHighlightSpan {
                            text: line[val_start..idx].to_string(),
                            color: col_val,
                        });
                    }
                }

                if idx < len && bytes[idx] == b'>' {
                    spans.push(XmlHighlightSpan {
                        text: ">".to_string(),
                        color: col_tag,
                    });
                    idx += 1;
                }
                continue;
            }

            // Normal text content between tags
            let text_start = idx;
            while idx < len && bytes[idx] != b'<' {
                idx += 1;
            }
            if idx > text_start {
                spans.push(XmlHighlightSpan {
                    text: line[text_start..idx].to_string(),
                    color: col_text,
                });
            }
        }

        if spans.is_empty() {
            spans.push(XmlHighlightSpan {
                text: line.to_string(),
                color: col_text,
            });
        }

        spans
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum XmlValidationResult {
    Valid {
        elements_count: usize,
        max_depth: usize,
        elapsed_secs: f64,
    },
    Invalid {
        line_number: usize,
        byte_offset: u64,
        message: String,
    },
}

pub struct XmlValidator;

impl XmlValidator {
    /// Stream-validates an XML stream using quick-xml and an explicit element stack.
    /// Never loads the file or DOM into RAM. Reports exact line numbers and byte error locations.
    pub fn validate<R: Read>(
        reader: R,
        line_index: Option<&LineIndex>,
        engine: Option<&FileEngine>,
        cancel: Arc<AtomicBool>,
    ) -> XmlValidationResult {
        let start = Instant::now();
        let checkpoints = Arc::new(Mutex::new(Vec::with_capacity(512)));
        let tracking_reader = LineTrackingReader::new(reader, Arc::clone(&checkpoints));
        let buf_reader = BufReader::with_capacity(512 * 1024, tracking_reader);
        let mut xml_reader = Reader::from_reader(buf_reader);
        xml_reader.config_mut().expand_empty_elements = false;
        xml_reader.config_mut().trim_text(false);
        xml_reader.config_mut().check_end_names = true;

        let mut buf = Vec::with_capacity(4096);
        let mut elements_count = 0;
        let mut max_depth = 0;

        enum TagEntry {
            Inline([u8; 32], u8, u64),
            Heap(Vec<u8>, u64),
        }
        impl TagEntry {
            #[inline]
            fn as_bytes(&self) -> &[u8] {
                match self {
                    TagEntry::Inline(buf, len, _) => &buf[..*len as usize],
                    TagEntry::Heap(v, _) => v.as_slice(),
                }
            }
            #[inline]
            fn open_offset(&self) -> u64 {
                match self {
                    TagEntry::Inline(_, _, pos) => *pos,
                    TagEntry::Heap(_, pos) => *pos,
                }
            }
        }

        let mut tag_stack: Vec<TagEntry> = Vec::with_capacity(64);
        let mut root_opened = false;
        let mut root_closed = false;

        let get_line_num = |offset: u64| -> usize {
            if let (Some(idx), Some(eng)) = (line_index, engine) {
                return idx.byte_offset_to_line(eng, offset);
            }
            if let Ok(cps) = checkpoints.lock() {
                if !cps.is_empty() {
                    match cps.binary_search_by_key(&offset, |&(b, _)| b) {
                        Ok(i) => return cps[i].1,
                        Err(i) => {
                            if i > 0 {
                                return cps[i - 1].1;
                            }
                        }
                    }
                }
            }
            1
        };

        loop {
            if cancel.load(Ordering::Relaxed) {
                let pos = xml_reader.buffer_position() as u64;
                return XmlValidationResult::Invalid {
                    line_number: get_line_num(pos),
                    byte_offset: pos,
                    message: "Validation cancelled by user".to_string(),
                };
            }

            let pos = xml_reader.buffer_position() as u64;

            match xml_reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    if root_closed {
                        return XmlValidationResult::Invalid {
                            line_number: get_line_num(pos),
                            byte_offset: pos,
                            message: format!(
                                "Multiple root elements: extra element <{}> found after root element closed",
                                String::from_utf8_lossy(e.name().as_ref())
                            ),
                        };
                    }
                    root_opened = true;
                    elements_count += 1;

                    // Zero-allocation duplicate attribute validation for typical elements (<= 8 attributes)
                    let mut attr_count = 0;
                    let mut attr_keys: [([u8; 32], u8); 8] = [([0; 32], 0); 8];
                    let mut heap_attr_keys: Option<Vec<Vec<u8>>> = None;
                    for attr_res in e.attributes() {
                        match attr_res {
                            Ok(attr) => {
                                let key = attr.key.as_ref();
                                let key_len = key.len();
                                if let Some(ref mut heap) = heap_attr_keys {
                                    if heap.iter().any(|k| k.as_slice() == key) {
                                        return XmlValidationResult::Invalid {
                                            line_number: get_line_num(pos),
                                            byte_offset: pos,
                                            message: format!(
                                                "Duplicate attribute '{}' on element <{}>",
                                                String::from_utf8_lossy(key),
                                                String::from_utf8_lossy(e.name().as_ref())
                                            ),
                                        };
                                    }
                                    heap.push(key.to_vec());
                                } else if attr_count < 8 && key_len <= 32 {
                                    for (prev_key, prev_len) in &attr_keys[..attr_count] {
                                        if *prev_len as usize == key_len && &prev_key[..key_len] == key {
                                            return XmlValidationResult::Invalid {
                                                line_number: get_line_num(pos),
                                                byte_offset: pos,
                                                message: format!(
                                                    "Duplicate attribute '{}' on element <{}>",
                                                    String::from_utf8_lossy(key),
                                                    String::from_utf8_lossy(e.name().as_ref())
                                                ),
                                            };
                                        }
                                    }
                                    let mut k_buf = [0u8; 32];
                                    k_buf[..key_len].copy_from_slice(key);
                                    attr_keys[attr_count] = (k_buf, key_len as u8);
                                    attr_count += 1;
                                } else {
                                    let mut heap: Vec<Vec<u8>> = attr_keys[..attr_count]
                                        .iter()
                                        .map(|(k, len)| k[..*len as usize].to_vec())
                                        .collect();
                                    if heap.iter().any(|k| k.as_slice() == key) {
                                        return XmlValidationResult::Invalid {
                                            line_number: get_line_num(pos),
                                            byte_offset: pos,
                                            message: format!(
                                                "Duplicate attribute '{}' on element <{}>",
                                                String::from_utf8_lossy(key),
                                                String::from_utf8_lossy(e.name().as_ref())
                                            ),
                                        };
                                    }
                                    heap.push(key.to_vec());
                                    heap_attr_keys = Some(heap);
                                }
                            }
                            Err(attr_err) => {
                                return XmlValidationResult::Invalid {
                                    line_number: get_line_num(pos),
                                    byte_offset: pos,
                                    message: format!("Attribute syntax error: {}", attr_err),
                                };
                            }
                        }
                    }

                    let name = e.name();
                    let name_bytes = name.as_ref();
                    if name_bytes.len() <= 32 {
                        let mut b = [0u8; 32];
                        b[..name_bytes.len()].copy_from_slice(name_bytes);
                        tag_stack.push(TagEntry::Inline(b, name_bytes.len() as u8, pos));
                    } else {
                        tag_stack.push(TagEntry::Heap(name_bytes.to_vec(), pos));
                    }
                    max_depth = max_depth.max(tag_stack.len());
                }
                Ok(Event::Empty(e)) => {
                    if root_closed {
                        return XmlValidationResult::Invalid {
                            line_number: get_line_num(pos),
                            byte_offset: pos,
                            message: format!(
                                "Multiple root elements: extra element <{}/> found after root element closed",
                                String::from_utf8_lossy(e.name().as_ref())
                            ),
                        };
                    }
                    if !root_opened {
                        root_opened = true;
                        root_closed = true;
                    }
                    elements_count += 1;

                    let mut attr_count = 0;
                    let mut attr_keys: [([u8; 32], u8); 8] = [([0; 32], 0); 8];
                    let mut heap_attr_keys: Option<Vec<Vec<u8>>> = None;
                    for attr_res in e.attributes() {
                        match attr_res {
                            Ok(attr) => {
                                let key = attr.key.as_ref();
                                let key_len = key.len();
                                if let Some(ref mut heap) = heap_attr_keys {
                                    if heap.iter().any(|k| k.as_slice() == key) {
                                        return XmlValidationResult::Invalid {
                                            line_number: get_line_num(pos),
                                            byte_offset: pos,
                                            message: format!(
                                                "Duplicate attribute '{}' on element <{}>",
                                                String::from_utf8_lossy(key),
                                                String::from_utf8_lossy(e.name().as_ref())
                                            ),
                                        };
                                    }
                                    heap.push(key.to_vec());
                                } else if attr_count < 8 && key_len <= 32 {
                                    for (prev_key, prev_len) in &attr_keys[..attr_count] {
                                        if *prev_len as usize == key_len && &prev_key[..key_len] == key {
                                            return XmlValidationResult::Invalid {
                                                line_number: get_line_num(pos),
                                                byte_offset: pos,
                                                message: format!(
                                                    "Duplicate attribute '{}' on element <{}>",
                                                    String::from_utf8_lossy(key),
                                                    String::from_utf8_lossy(e.name().as_ref())
                                                ),
                                            };
                                        }
                                    }
                                    let mut k_buf = [0u8; 32];
                                    k_buf[..key_len].copy_from_slice(key);
                                    attr_keys[attr_count] = (k_buf, key_len as u8);
                                    attr_count += 1;
                                } else {
                                    let mut heap: Vec<Vec<u8>> = attr_keys[..attr_count]
                                        .iter()
                                        .map(|(k, len)| k[..*len as usize].to_vec())
                                        .collect();
                                    if heap.iter().any(|k| k.as_slice() == key) {
                                        return XmlValidationResult::Invalid {
                                            line_number: get_line_num(pos),
                                            byte_offset: pos,
                                            message: format!(
                                                "Duplicate attribute '{}' on element <{}>",
                                                String::from_utf8_lossy(key),
                                                String::from_utf8_lossy(e.name().as_ref())
                                            ),
                                        };
                                    }
                                    heap.push(key.to_vec());
                                    heap_attr_keys = Some(heap);
                                }
                            }
                            Err(attr_err) => {
                                return XmlValidationResult::Invalid {
                                    line_number: get_line_num(pos),
                                    byte_offset: pos,
                                    message: format!("Attribute syntax error: {}", attr_err),
                                };
                            }
                        }
                    }
                }
                Ok(Event::End(e)) => {
                    let name_bytes = e.name();
                    if let Some(top) = tag_stack.pop() {
                        if top.as_bytes() != name_bytes.as_ref() {
                            return XmlValidationResult::Invalid {
                                line_number: get_line_num(pos),
                                byte_offset: pos,
                                message: format!(
                                    "Mismatched closing tag: expected </{}> (opened at line {}), but found </{}>",
                                    String::from_utf8_lossy(top.as_bytes()),
                                    get_line_num(top.open_offset()),
                                    String::from_utf8_lossy(name_bytes.as_ref())
                                ),
                            };
                        }
                        if tag_stack.is_empty() {
                            root_closed = true;
                        }
                    } else {
                        return XmlValidationResult::Invalid {
                            line_number: get_line_num(pos),
                            byte_offset: pos,
                            message: format!(
                                "Unexpected closing tag </{}> without matching start tag",
                                String::from_utf8_lossy(name_bytes.as_ref())
                            ),
                        };
                    }
                }
                Ok(Event::Text(t)) => {
                    if root_closed {
                        let text = t.as_ref();
                        if text.iter().any(|&b| !b.is_ascii_whitespace()) {
                            return XmlValidationResult::Invalid {
                                line_number: get_line_num(pos),
                                byte_offset: pos,
                                message: "Illegal non-whitespace content found after root element closed".to_string(),
                            };
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    let err_pos = xml_reader.buffer_position() as u64;
                    return XmlValidationResult::Invalid {
                        line_number: get_line_num(err_pos),
                        byte_offset: err_pos,
                        message: format!("XML syntax error: {}", e),
                    };
                }
                _ => {}
            }
            buf.clear();
        }

        if !root_opened {
            return XmlValidationResult::Invalid {
                line_number: 1,
                byte_offset: 0,
                message: "Empty XML document: no root element found".to_string(),
            };
        }

        if let Some(top) = tag_stack.pop() {
            let open_offset = top.open_offset();
            let open_line = get_line_num(open_offset);
            return XmlValidationResult::Invalid {
                line_number: open_line,
                byte_offset: open_offset,
                message: format!(
                    "Unclosed XML element <{}> (opened at line {})",
                    String::from_utf8_lossy(top.as_bytes()),
                    open_line
                ),
            };
        }

        XmlValidationResult::Valid {
            elements_count,
            max_depth,
            elapsed_secs: start.elapsed().as_secs_f64(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct XmlTreeNode {
    pub name: String,
    pub summary: String,
    pub line_number: usize,
    pub byte_offset: u64,
    pub children: Vec<XmlTreeNode>,
}

pub struct XmlStructureIndexer;

impl XmlStructureIndexer {
    /// Builds a compact hierarchical tree (capped at depth 3 and 5,000 nodes)
    /// representing the document structure for quick navigation.
    pub fn build_tree(engine: Arc<FileEngine>, cancel: Arc<AtomicBool>) -> Option<XmlTreeNode> {
        let file = File::open(engine.path()).ok()?;
        let buf_reader = BufReader::with_capacity(128 * 1024, file);
        let mut reader = Reader::from_reader(buf_reader);
        reader.config_mut().expand_empty_elements = false;

        let mut buf = Vec::with_capacity(4096);
        let mut root: Option<XmlTreeNode> = None;
        let mut stack: Vec<XmlTreeNode> = Vec::new();

        let mut total_nodes = 0;
        const MAX_NODES: usize = 5000;
        const MAX_DEPTH: usize = 3;

        loop {
            if cancel.load(Ordering::Relaxed) || total_nodes >= MAX_NODES {
                break;
            }

            let pos = reader.buffer_position() as u64;

            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    if stack.len() < MAX_DEPTH {
                        let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                        let summary = Self::extract_attributes_summary(&e);
                        let node = XmlTreeNode {
                            name,
                            summary,
                            line_number: 1, // Will map or link via offset
                            byte_offset: pos,
                            children: Vec::new(),
                        };
                        stack.push(node);
                        total_nodes += 1;
                    } else {
                        let end_name = e.name().as_ref().to_vec();
                        let mut skip_buf = Vec::new();
                        let _ = reader.read_to_end_into(quick_xml::name::QName(&end_name), &mut skip_buf);
                    }
                }
                Ok(Event::Empty(e)) => {
                    if stack.len() < MAX_DEPTH && total_nodes < MAX_NODES {
                        let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                        let summary = Self::extract_attributes_summary(&e);
                        let node = XmlTreeNode {
                            name,
                            summary,
                            line_number: 1,
                            byte_offset: pos,
                            children: Vec::new(),
                        };
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(node);
                        }
                        total_nodes += 1;
                    }
                }
                Ok(Event::End(_)) => {
                    if let Some(finished_node) = stack.pop() {
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(finished_node);
                        } else {
                            root = Some(finished_node);
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }

        if root.is_none() && !stack.is_empty() {
            root = Some(stack.remove(0));
        }

        root
    }

    fn extract_attributes_summary(e: &quick_xml::events::BytesStart) -> String {
        let mut summary = String::new();
        for attr in e.attributes().flatten() {
            let key = String::from_utf8_lossy(attr.key.as_ref());
            let val = String::from_utf8_lossy(&attr.value);
            if key == "id" || key == "name" || key == "title" || key == "type" {
                if !summary.is_empty() {
                    summary.push(' ');
                }
                summary.push_str(&format!("{}=\"{}\"", key, val));
            }
        }
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_validator_catches_unclosed_category_tag() {
        let invalid_xml = r#"<source>
  <job>
    <title><![CDATA[Estimator]]></title>
    <category><![CDATA[General]]>
    <CategoryID>28</CategoryID>
    <city><![CDATA[Garden Grove]]></city>
  </job>
</source>"#;

        let cancel = Arc::new(AtomicBool::new(false));
        let res = XmlValidator::validate(std::io::Cursor::new(invalid_xml), None, None, cancel);
        match res {
            XmlValidationResult::Invalid { message, .. } => {
                assert!(message.contains("category"), "Expected error to mention 'category', got: {}", message);
                assert!(message.contains("job") || message.contains("expected"), "Expected error to mention mismatched tag, got: {}", message);
            }
            XmlValidationResult::Valid { .. } => {
                panic!("Expected XML to be invalid due to missing </category> closing tag!");
            }
        }
    }

    #[test]
    fn test_xml_validator_valid_xml() {
        let valid_xml = r#"<source>
  <job>
    <title><![CDATA[Estimator]]></title>
    <category><![CDATA[General]]></category>
    <CategoryID>28</CategoryID>
  </job>
</source>"#;

        let cancel = Arc::new(AtomicBool::new(false));
        let res = XmlValidator::validate(std::io::Cursor::new(valid_xml), None, None, cancel);
        match res {
            XmlValidationResult::Valid { elements_count, max_depth, .. } => {
                assert_eq!(elements_count, 5);
                assert_eq!(max_depth, 3);
            }
            XmlValidationResult::Invalid { message, .. } => {
                panic!("Expected XML to be valid, but got: {}", message);
            }
        }
    }
}

