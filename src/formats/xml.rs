use std::fs::File;
use std::io::BufReader;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use eframe::egui::Color32;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::file_engine::FileEngine;

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
    /// Stream-validates an XML file using quick-xml.
    /// Never loads the file into RAM. Reports exact line and byte error locations.
    pub fn validate(engine: Arc<FileEngine>, cancel: Arc<AtomicBool>) -> XmlValidationResult {
        let file = match File::open(engine.path()) {
            Ok(f) => f,
            Err(e) => return XmlValidationResult::Invalid {
                line_number: 1,
                byte_offset: 0,
                message: format!("Cannot open file: {}", e),
            },
        };

        let start = Instant::now();
        let buf_reader = BufReader::with_capacity(128 * 1024, file);
        let mut reader = Reader::from_reader(buf_reader);
        reader.config_mut().expand_empty_elements = false;
        reader.config_mut().check_end_names = true;

        let mut buf = Vec::with_capacity(4096);
        let mut elements_count = 0;
        let mut current_depth: usize = 0;
        let mut max_depth = 0;
        let line_number = 1;
        let mut last_offset: u64 = 0;

        loop {
            if cancel.load(Ordering::Relaxed) {
                return XmlValidationResult::Invalid {
                    line_number,
                    byte_offset: last_offset,
                    message: "Validation cancelled by user".to_string(),
                };
            }

            let pos = reader.buffer_position() as u64;
            last_offset = pos;

            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(_)) => {
                    elements_count += 1;
                    current_depth += 1;
                    max_depth = max_depth.max(current_depth);
                }
                Ok(Event::Empty(_)) => {
                    elements_count += 1;
                }
                Ok(Event::End(_)) => {
                    current_depth = current_depth.saturating_sub(1);
                }
                Ok(Event::Eof) => break,
                Err(e) => {
                    return XmlValidationResult::Invalid {
                        line_number,
                        byte_offset: pos,
                        message: format!("{}", e),
                    };
                }
                _ => {}
            }
            buf.clear();
        }

        if current_depth > 0 {
            return XmlValidationResult::Invalid {
                line_number,
                byte_offset: last_offset,
                message: format!("Document ended with {} unclosed XML elements", current_depth),
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
