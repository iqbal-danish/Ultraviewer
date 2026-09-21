use std::fs::File;
use std::io::{BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use eframe::egui::Color32;

use crate::file_engine::FileEngine;

#[derive(Debug, Clone)]
pub struct JsonHighlightSpan {
    pub text: String,
    pub color: Color32,
}

pub struct JsonSyntaxHighlighter;

impl JsonSyntaxHighlighter {
    pub fn highlight_line(line: &str) -> Vec<JsonHighlightSpan> {
        Self::highlight_line_themed(line, true)
    }

    pub fn highlight_line_themed(line: &str, dark_mode: bool) -> Vec<JsonHighlightSpan> {
        let mut spans = Vec::new();
        let bytes = line.as_bytes();
        let len = bytes.len();
        let mut idx = 0;

        let (col_key, col_string, col_number, col_bool_null, col_bracket, col_punct, col_whitespace) = if dark_mode {
            (
                Color32::from_rgb(224, 108, 117), // Keys in #E06C75 (Coral Red)
                Color32::from_rgb(152, 195, 121), // Strings in #98C379 (Green)
                Color32::from_rgb(209, 154, 102), // Numbers in #D19A66 (Orange)
                Color32::from_rgb(86, 182, 194),  // Booleans / Null in #56B6C2 (Cyan)
                Color32::from_rgb(171, 178, 191), // Brackets in #ABB2BF
                Color32::from_rgb(171, 178, 191), // Punctuation in #ABB2BF
                Color32::from_rgb(171, 178, 191), // Whitespace
            )
        } else {
            (
                Color32::from_rgb(4, 81, 165),    // Deep Blue (Keys)
                Color32::from_rgb(163, 21, 21),   // Dark Red (Strings)
                Color32::from_rgb(9, 134, 88),    // Green (Numbers)
                Color32::from_rgb(0, 0, 255),     // Blue (Booleans / Null)
                Color32::from_rgb(129, 31, 144),  // Purple ({ } [ ])
                Color32::from_rgb(50, 50, 50),    // Dark Gray (: ,)
                Color32::from_rgb(30, 30, 30),    // Whitespace
            )
        };

        while idx < len {
            // Whitespace
            if bytes[idx].is_ascii_whitespace() {
                let start = idx;
                while idx < len && bytes[idx].is_ascii_whitespace() {
                    idx += 1;
                }
                spans.push(JsonHighlightSpan {
                    text: line[start..idx].to_string(),
                    color: col_whitespace,
                });
                continue;
            }

            // Strings and Keys
            if bytes[idx] == b'"' {
                let start = idx;
                idx += 1;
                let mut escaped = false;
                while idx < len {
                    if escaped {
                        escaped = false;
                        idx += 1;
                    } else if bytes[idx] == b'\\' {
                        escaped = true;
                        idx += 1;
                    } else if bytes[idx] == b'"' {
                        idx += 1;
                        break;
                    } else {
                        idx += 1;
                    }
                }

                // Lookahead: is this string followed by optional whitespace and a colon ':'? If so, it's a key!
                let mut lookahead = idx;
                while lookahead < len && bytes[lookahead].is_ascii_whitespace() {
                    lookahead += 1;
                }
                let is_key = lookahead < len && bytes[lookahead] == b':';

                let color = if is_key { col_key } else { col_string };
                spans.push(JsonHighlightSpan {
                    text: line[start..idx].to_string(),
                    color,
                });
                continue;
            }

            // Structural punctuation
            match bytes[idx] {
                b'{' | b'}' | b'[' | b']' => {
                    spans.push(JsonHighlightSpan {
                        text: (bytes[idx] as char).to_string(),
                        color: col_bracket,
                    });
                    idx += 1;
                    continue;
                }
                b':' | b',' => {
                    spans.push(JsonHighlightSpan {
                        text: (bytes[idx] as char).to_string(),
                        color: col_punct,
                    });
                    idx += 1;
                    continue;
                }
                _ => {}
            }

            // Numbers: [+-]?[0-9]+(\.[0-9]+)?([eE][+-]?[0-9]+)?
            if bytes[idx].is_ascii_digit() || bytes[idx] == b'-' {
                let start = idx;
                if bytes[idx] == b'-' {
                    idx += 1;
                }
                while idx < len && (bytes[idx].is_ascii_digit() || bytes[idx] == b'.' || bytes[idx] == b'e' || bytes[idx] == b'E' || bytes[idx] == b'+' || bytes[idx] == b'-') {
                    idx += 1;
                }
                spans.push(JsonHighlightSpan {
                    text: line[start..idx].to_string(),
                    color: col_number,
                });
                continue;
            }

            // Identifiers / Literals: true, false, null
            if bytes[idx].is_ascii_alphabetic() {
                let start = idx;
                while idx < len && bytes[idx].is_ascii_alphabetic() {
                    idx += 1;
                }
                let word = &line[start..idx];
                let color = if word == "true" || word == "false" || word == "null" {
                    col_bool_null
                } else {
                    col_punct
                };
                spans.push(JsonHighlightSpan {
                    text: word.to_string(),
                    color,
                });
                continue;
            }

            // Catch-all single char
            spans.push(JsonHighlightSpan {
                text: (bytes[idx] as char).to_string(),
                color: col_punct,
            });
            idx += 1;
        }

        spans
    }
}

#[derive(Debug, Clone)]
pub enum JsonValidationResult {
    Valid {
        objects_count: usize,
        arrays_count: usize,
        max_depth: usize,
        elapsed_secs: f64,
    },
    Invalid {
        line_number: usize,
        byte_offset: u64,
        message: String,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContainerState {
    InObjectExpectKeyOrClose,
    InObjectExpectKey,
    InObjectExpectColon,
    InObjectExpectVal,
    InObjectExpectCommaOrClose,
    InArrayExpectValOrClose,
    InArrayExpectVal,
    InArrayExpectCommaOrClose,
}

pub struct JsonValidator;

impl JsonValidator {
    /// Streaming zero-DOM validation of a JSON file using a 256 KB reusable chunk buffer.
    /// Never loads the file or DOM into memory, maintaining a bounded working set (< 15 MB)
    /// even on files up to 50 GB.
    pub fn validate(engine: Arc<FileEngine>, cancel: Arc<AtomicBool>) -> JsonValidationResult {
        let start = Instant::now();
        let file = match File::open(engine.path()) {
            Ok(f) => f,
            Err(e) => return JsonValidationResult::Invalid {
                line_number: 1,
                byte_offset: 0,
                message: format!("Cannot open file: {}", e),
            },
        };

        let mut reader = BufReader::with_capacity(256 * 1024, file);
        let mut buffer = vec![0u8; 256 * 1024];

        let mut current_offset: u64 = 0;
        let mut line_number: usize = 1;

        let mut stack: Vec<ContainerState> = Vec::with_capacity(64);
        let mut max_depth = 0;
        let mut objects_count = 0;
        let mut arrays_count = 0;
        let mut root_seen = false;
        let mut completed_root = false;

        let mut in_string = false;
        let mut escaped = false;
        let mut string_start_line = 1;
        let mut string_start_pos = 0;

        let mut literal_buf: Vec<u8> = Vec::with_capacity(64);
        let mut literal_start_line = 1;
        let mut literal_start_pos = 0;

        loop {
            if cancel.load(Ordering::Relaxed) {
                return JsonValidationResult::Invalid {
                    line_number,
                    byte_offset: current_offset,
                    message: "Validation cancelled".to_string(),
                };
            }

            let bytes_read = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) => return JsonValidationResult::Invalid {
                    line_number,
                    byte_offset: current_offset,
                    message: format!("Read error: {}", e),
                },
            };

            let slice = &buffer[..bytes_read];
            let mut idx = 0;

            while idx < bytes_read {
                let b = slice[idx];
                let byte_pos = current_offset + idx as u64;

                if in_string {
                    if b == b'\n' {
                        line_number += 1;
                    }
                    if escaped {
                        escaped = false;
                    } else if b == b'\\' {
                        escaped = true;
                    } else if b == b'"' {
                        in_string = false;
                        // Completed string literal
                        if let Some(top) = stack.last_mut() {
                            match top {
                                ContainerState::InObjectExpectKeyOrClose | ContainerState::InObjectExpectKey => {
                                    *top = ContainerState::InObjectExpectColon;
                                }
                                ContainerState::InObjectExpectVal => {
                                    *top = ContainerState::InObjectExpectCommaOrClose;
                                }
                                ContainerState::InArrayExpectValOrClose | ContainerState::InArrayExpectVal => {
                                    *top = ContainerState::InArrayExpectCommaOrClose;
                                }
                                _ => return JsonValidationResult::Invalid {
                                    line_number: string_start_line,
                                    byte_offset: string_start_pos,
                                    message: "Unexpected string in container state".to_string(),
                                },
                            }
                        } else if !root_seen {
                            root_seen = true;
                            completed_root = true;
                        }
                    }
                    idx += 1;
                    continue;
                }

                // If currently collecting a literal (number, bool, null)
                if !literal_buf.is_empty() {
                    if b.is_ascii_whitespace() || b == b',' || b == b'}' || b == b']' || b == b':' {
                        let lit = &literal_buf;
                        let is_valid = lit == b"true" || lit == b"false" || lit == b"null"
                            || (!lit.is_empty() && (lit[0].is_ascii_digit() || lit[0] == b'-'));
                        if !is_valid {
                            return JsonValidationResult::Invalid {
                                line_number: literal_start_line,
                                byte_offset: literal_start_pos,
                                message: format!("Invalid literal '{}'", String::from_utf8_lossy(lit)),
                            };
                        }

                        if let Some(top) = stack.last_mut() {
                            match top {
                                ContainerState::InObjectExpectVal => {
                                    *top = ContainerState::InObjectExpectCommaOrClose;
                                }
                                ContainerState::InArrayExpectValOrClose | ContainerState::InArrayExpectVal => {
                                    *top = ContainerState::InArrayExpectCommaOrClose;
                                }
                                _ => return JsonValidationResult::Invalid {
                                    line_number: literal_start_line,
                                    byte_offset: literal_start_pos,
                                    message: format!("Unexpected literal '{}' in object key position", String::from_utf8_lossy(lit)),
                                },
                            }
                        } else if !root_seen {
                            root_seen = true;
                            completed_root = true;
                        }

                        literal_buf.clear();
                        // Do not advance idx; reprocess this delimiter byte
                    } else {
                        literal_buf.push(b);
                        idx += 1;
                        continue;
                    }
                }

                if b == b'\n' {
                    line_number += 1;
                    idx += 1;
                    continue;
                }

                if b.is_ascii_whitespace() {
                    idx += 1;
                    continue;
                }

                if completed_root && !stack.is_empty() {
                    // Still inside something
                } else if completed_root {
                    return JsonValidationResult::Invalid {
                        line_number,
                        byte_offset: byte_pos,
                        message: format!("Unexpected trailing character '{}' after root JSON document", b as char),
                    };
                }

                match b {
                    b'{' => {
                        if let Some(top) = stack.last_mut() {
                            match top {
                                ContainerState::InObjectExpectVal => {
                                    *top = ContainerState::InObjectExpectCommaOrClose;
                                }
                                ContainerState::InArrayExpectValOrClose | ContainerState::InArrayExpectVal => {
                                    *top = ContainerState::InArrayExpectCommaOrClose;
                                }
                                _ => return JsonValidationResult::Invalid {
                                    line_number,
                                    byte_offset: byte_pos,
                                    message: "Unexpected '{'".to_string(),
                                },
                            }
                        } else if root_seen {
                            return JsonValidationResult::Invalid {
                                line_number,
                                byte_offset: byte_pos,
                                message: "Multiple root elements in JSON document".to_string(),
                            };
                        } else {
                            root_seen = true;
                        }

                        objects_count += 1;
                        stack.push(ContainerState::InObjectExpectKeyOrClose);
                        max_depth = max_depth.max(stack.len());
                        idx += 1;
                    }
                    b'}' => {
                        match stack.pop() {
                            Some(ContainerState::InObjectExpectKeyOrClose) | Some(ContainerState::InObjectExpectCommaOrClose) => {
                                if stack.is_empty() {
                                    completed_root = true;
                                }
                            }
                            Some(ContainerState::InObjectExpectKey) => return JsonValidationResult::Invalid {
                                line_number,
                                byte_offset: byte_pos,
                                message: "Unexpected trailing comma before '}'".to_string(),
                            },
                            _ => return JsonValidationResult::Invalid {
                                line_number,
                                byte_offset: byte_pos,
                                message: "Unexpected closing brace '}'".to_string(),
                            },
                        }
                        idx += 1;
                    }
                    b'[' => {
                        if let Some(top) = stack.last_mut() {
                            match top {
                                ContainerState::InObjectExpectVal => {
                                    *top = ContainerState::InObjectExpectCommaOrClose;
                                }
                                ContainerState::InArrayExpectValOrClose | ContainerState::InArrayExpectVal => {
                                    *top = ContainerState::InArrayExpectCommaOrClose;
                                }
                                _ => return JsonValidationResult::Invalid {
                                    line_number,
                                    byte_offset: byte_pos,
                                    message: "Unexpected '['".to_string(),
                                },
                            }
                        } else if root_seen {
                            return JsonValidationResult::Invalid {
                                line_number,
                                byte_offset: byte_pos,
                                message: "Multiple root elements in JSON document".to_string(),
                            };
                        } else {
                            root_seen = true;
                        }

                        arrays_count += 1;
                        stack.push(ContainerState::InArrayExpectValOrClose);
                        max_depth = max_depth.max(stack.len());
                        idx += 1;
                    }
                    b']' => {
                        match stack.pop() {
                            Some(ContainerState::InArrayExpectValOrClose) | Some(ContainerState::InArrayExpectCommaOrClose) => {
                                if stack.is_empty() {
                                    completed_root = true;
                                }
                            }
                            Some(ContainerState::InArrayExpectVal) => return JsonValidationResult::Invalid {
                                line_number,
                                byte_offset: byte_pos,
                                message: "Unexpected trailing comma before ']'".to_string(),
                            },
                            _ => return JsonValidationResult::Invalid {
                                line_number,
                                byte_offset: byte_pos,
                                message: "Unexpected closing bracket ']'".to_string(),
                            },
                        }
                        idx += 1;
                    }
                    b':' => {
                        match stack.last_mut() {
                            Some(top @ ContainerState::InObjectExpectColon) => {
                                *top = ContainerState::InObjectExpectVal;
                            }
                            _ => return JsonValidationResult::Invalid {
                                line_number,
                                byte_offset: byte_pos,
                                message: "Unexpected colon ':'".to_string(),
                            },
                        }
                        idx += 1;
                    }
                    b',' => {
                        match stack.last_mut() {
                            Some(top @ ContainerState::InObjectExpectCommaOrClose) => {
                                *top = ContainerState::InObjectExpectKey;
                            }
                            Some(top @ ContainerState::InArrayExpectCommaOrClose) => {
                                *top = ContainerState::InArrayExpectVal;
                            }
                            _ => return JsonValidationResult::Invalid {
                                line_number,
                                byte_offset: byte_pos,
                                message: "Unexpected comma ',' (e.g. trailing comma or misplaced)".to_string(),
                            },
                        }
                        idx += 1;
                    }
                    b'"' => {
                        in_string = true;
                        escaped = false;
                        string_start_line = line_number;
                        string_start_pos = byte_pos;
                        idx += 1;
                    }
                    _ => {
                        literal_buf.push(b);
                        literal_start_line = line_number;
                        literal_start_pos = byte_pos;
                        idx += 1;
                    }
                }
            }

            current_offset += bytes_read as u64;
        }

        if in_string {
            return JsonValidationResult::Invalid {
                line_number: string_start_line,
                byte_offset: string_start_pos,
                message: "Unterminated string literal".to_string(),
            };
        }

        if !literal_buf.is_empty() {
            let lit = &literal_buf;
            let is_valid = lit == b"true" || lit == b"false" || lit == b"null"
                || (!lit.is_empty() && (lit[0].is_ascii_digit() || lit[0] == b'-'));
            if !is_valid {
                return JsonValidationResult::Invalid {
                    line_number: literal_start_line,
                    byte_offset: literal_start_pos,
                    message: format!("Invalid literal '{}'", String::from_utf8_lossy(lit)),
                };
            }
            if let Some(top) = stack.last_mut() {
                match top {
                    ContainerState::InObjectExpectVal => {
                        *top = ContainerState::InObjectExpectCommaOrClose;
                    }
                    ContainerState::InArrayExpectValOrClose | ContainerState::InArrayExpectVal => {
                        *top = ContainerState::InArrayExpectCommaOrClose;
                    }
                    _ => return JsonValidationResult::Invalid {
                        line_number: literal_start_line,
                        byte_offset: literal_start_pos,
                        message: format!("Unexpected literal '{}' in object key position", String::from_utf8_lossy(lit)),
                    },
                }
            } else if !root_seen {
                root_seen = true;
            }
        }

        if !stack.is_empty() {
            return JsonValidationResult::Invalid {
                line_number,
                byte_offset: current_offset,
                message: format!("Unclosed JSON structure ({} unclosed container(s))", stack.len()),
            };
        }

        if !root_seen {
            return JsonValidationResult::Invalid {
                line_number: 1,
                byte_offset: 0,
                message: "Empty document (no JSON elements found)".to_string(),
            };
        }

        JsonValidationResult::Valid {
            objects_count,
            arrays_count,
            max_depth,
            elapsed_secs: start.elapsed().as_secs_f64(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct JsonTreeNode {
    pub name: String,
    pub summary: Option<String>,
    pub byte_offset: u64,
    pub line_number: usize,
    pub is_array: bool,
    pub children: Vec<JsonTreeNode>,
}

pub struct JsonStructureIndexer;

impl JsonStructureIndexer {
    const MAX_NODES: usize = 5000;
    const _MAX_DEPTH: usize = 3;

    /// Build a compact hierarchical structure tree of the JSON document.
    pub fn build_tree(engine: Arc<FileEngine>, cancel: Arc<AtomicBool>) -> Arc<JsonTreeNode> {
        let file = match File::open(engine.path()) {
            Ok(f) => f,
            Err(_) => return Arc::new(JsonTreeNode {
                name: "Document".to_string(),
                summary: None,
                byte_offset: 0,
                line_number: 1,
                is_array: false,
                children: Vec::new(),
            }),
        };

        let mut reader = BufReader::with_capacity(256 * 1024, file);
        let mut buffer = vec![0u8; 256 * 1024];

        let mut root = JsonTreeNode {
            name: "root".to_string(),
            summary: None,
            byte_offset: 0,
            line_number: 1,
            is_array: false,
            children: Vec::new(),
        };

        let mut current_offset: u64 = 0;
        let mut line_number: usize = 1;
        let mut total_nodes = 0;

        let mut current_key: Option<String> = None;
        let mut array_index_stack: Vec<usize> = Vec::new();

        let mut in_string = false;
        let mut escaped = false;
        let mut string_buf = Vec::with_capacity(64);

        loop {
            if cancel.load(Ordering::Relaxed) || total_nodes >= Self::MAX_NODES {
                break;
            }

            let bytes_read = match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };

            let slice = &buffer[..bytes_read];
            let mut idx = 0;

            while idx < bytes_read && total_nodes < Self::MAX_NODES {
                let b = slice[idx];
                let byte_pos = current_offset + idx as u64;

                if in_string {
                    if b == b'\n' {
                        line_number += 1;
                    }
                    if escaped {
                        escaped = false;
                        if string_buf.len() < 128 {
                            string_buf.push(b);
                        }
                    } else if b == b'\\' {
                        escaped = true;
                    } else if b == b'"' {
                        in_string = false;
                        let text = String::from_utf8_lossy(&string_buf).to_string();
                        string_buf.clear();

                        // Lookahead in remaining slice for ':'
                        let mut look = idx + 1;
                        while look < bytes_read && slice[look].is_ascii_whitespace() {
                            look += 1;
                        }
                        if look < bytes_read && slice[look] == b':' {
                            current_key = Some(text);
                            idx = look + 1;
                            continue;
                        }
                    } else {
                        if string_buf.len() < 128 {
                            string_buf.push(b);
                        }
                    }
                    idx += 1;
                    continue;
                }

                if b == b'\n' {
                    line_number += 1;
                    idx += 1;
                    continue;
                }

                if b.is_ascii_whitespace() {
                    idx += 1;
                    continue;
                }

                match b {
                    b'"' => {
                        in_string = true;
                        escaped = false;
                        string_buf.clear();
                        idx += 1;
                    }
                    b'[' => {
                        array_index_stack.push(0);

                        let node_name = current_key.take().unwrap_or_else(|| "array".to_string());
                        if root.children.len() < 100 {
                            root.children.push(JsonTreeNode {
                                name: node_name,
                                summary: Some("Array".to_string()),
                                byte_offset: byte_pos,
                                line_number,
                                is_array: true,
                                children: Vec::new(),
                            });
                            total_nodes += 1;
                        }
                        idx += 1;
                    }
                    b']' => {
                        array_index_stack.pop();
                        idx += 1;
                    }
                    b'{' => {
                        let node_name = if let Some(key) = current_key.take() {
                            key
                        } else if let Some(last_idx) = array_index_stack.last_mut() {
                            let name = format!("[{}]", *last_idx);
                            *last_idx += 1;
                            name
                        } else {
                            "object".to_string()
                        };

                        if let Some(parent) = root.children.last_mut() {
                            if parent.is_array && parent.children.len() < 500 {
                                parent.children.push(JsonTreeNode {
                                    name: node_name,
                                    summary: None,
                                    byte_offset: byte_pos,
                                    line_number,
                                    is_array: false,
                                    children: Vec::new(),
                                });
                                total_nodes += 1;
                            } else if parent.is_array {
                                total_nodes += 1;
                                if parent.children.len() >= 500 && root.children.len() == 1 {
                                    break;
                                }
                            } else if root.children.len() < 100 {
                                root.children.push(JsonTreeNode {
                                    name: node_name,
                                    summary: None,
                                    byte_offset: byte_pos,
                                    line_number,
                                    is_array: false,
                                    children: Vec::new(),
                                });
                                total_nodes += 1;
                            }
                        } else {
                            root.name = node_name;
                            root.byte_offset = byte_pos;
                            root.line_number = line_number;
                        }
                        idx += 1;
                    }
                    _ => {
                        idx += 1;
                    }
                }
            }

            current_offset += bytes_read as u64;
        }

        Arc::new(root)
    }
}
