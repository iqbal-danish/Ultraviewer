use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;
use eframe::egui::Color32;

use super::formatter::{FormatAction, FormattingProgress};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonHighlightSpan {
    pub text: String,
    pub color: Color32,
}

pub struct PythonSyntaxHighlighter;

impl PythonSyntaxHighlighter {
    pub fn highlight_line(line: &str) -> Vec<PythonHighlightSpan> {
        Self::highlight_line_themed(line, true)
    }

    pub fn highlight_line_themed(line: &str, dark_mode: bool) -> Vec<PythonHighlightSpan> {
        let (col_keyword, col_builtin, col_string, col_number, col_comment, col_decorator, col_ident, col_punct) = if dark_mode {
            (
                Color32::from_rgb(198, 120, 221), // Keyword #C678DD (Purple / Orchid)
                Color32::from_rgb(86, 182, 194),  // Built-in #56B6C2 (Cyan)
                Color32::from_rgb(152, 195, 121), // String #98C379 (Green)
                Color32::from_rgb(209, 154, 102), // Number #D19A66 (Orange)
                Color32::from_rgb(92, 99, 112),   // Comment #5C6370 (Muted Slate)
                Color32::from_rgb(97, 175, 239),  // Decorator #61AFEF (Blue)
                Color32::from_rgb(224, 108, 117), // Function identifier #E06C75 (Coral)
                Color32::from_rgb(171, 178, 191), // Punctuation / Plain #ABB2BF (Silver)
            )
        } else {
            (
                Color32::from_rgb(175, 0, 219),   // Purple
                Color32::from_rgb(0, 128, 128),   // Teal / Cyan
                Color32::from_rgb(163, 21, 21),   // Dark Red
                Color32::from_rgb(9, 134, 88),    // Green
                Color32::from_rgb(0, 128, 0),     // Dark Green Comment
                Color32::from_rgb(0, 0, 255),     // Blue
                Color32::from_rgb(113, 44, 0),    // Brown / Rust
                Color32::from_rgb(30, 30, 30),    // Dark Charcoal
            )
        };

        let mut spans = Vec::new();
        let chars: Vec<char> = line.chars().collect();
        let len = chars.len();
        let mut idx = 0;

        while idx < len {
            let ch = chars[idx];

            // 1. Whitespace
            if ch.is_whitespace() {
                let start = idx;
                while idx < len && chars[idx].is_whitespace() {
                    idx += 1;
                }
                let text: String = chars[start..idx].iter().collect();
                spans.push(PythonHighlightSpan { text, color: col_punct });
                continue;
            }

            // 2. Comments (# to end of line)
            if ch == '#' {
                let text: String = chars[idx..].iter().collect();
                spans.push(PythonHighlightSpan { text, color: col_comment });
                break;
            }

            // 3. Decorator (@...)
            if ch == '@' {
                let start = idx;
                idx += 1;
                while idx < len && (chars[idx].is_alphanumeric() || chars[idx] == '_' || chars[idx] == '.') {
                    idx += 1;
                }
                let text: String = chars[start..idx].iter().collect();
                spans.push(PythonHighlightSpan { text, color: col_decorator });
                continue;
            }

            // 4. Strings: triple-quoted """ or ''' or single-quoted " or '
            let is_str_prefix = (ch == 'f' || ch == 'r' || ch == 'b' || ch == 'u')
                && idx + 1 < len
                && (chars[idx + 1] == '"' || chars[idx + 1] == '\'');

            let str_start_idx = if is_str_prefix { idx + 1 } else { idx };
            let quote_char = chars[str_start_idx];

            if quote_char == '"' || quote_char == '\'' {
                let is_triple = str_start_idx + 2 < len
                    && chars[str_start_idx + 1] == quote_char
                    && chars[str_start_idx + 2] == quote_char;

                let start = idx;
                if is_triple {
                    idx = str_start_idx + 3;
                    while idx + 2 < len {
                        if chars[idx] == '\\' {
                            idx += 2;
                        } else if chars[idx] == quote_char && chars[idx + 1] == quote_char && chars[idx + 2] == quote_char {
                            idx += 3;
                            break;
                        } else {
                            idx += 1;
                        }
                    }
                    if idx > len { idx = len; }
                } else {
                    idx = str_start_idx + 1;
                    while idx < len {
                        if chars[idx] == '\\' {
                            idx += 2;
                        } else if chars[idx] == quote_char {
                            idx += 1;
                            break;
                        } else {
                            idx += 1;
                        }
                    }
                }
                let text: String = chars[start..idx.min(len)].iter().collect();
                spans.push(PythonHighlightSpan { text, color: col_string });
                continue;
            }

            // 5. Numbers
            if ch.is_ascii_digit() || (ch == '.' && idx + 1 < len && chars[idx + 1].is_ascii_digit()) {
                let start = idx;
                idx += 1;
                while idx < len && (chars[idx].is_ascii_alphanumeric() || chars[idx] == '.' || chars[idx] == '_') {
                    idx += 1;
                }
                let text: String = chars[start..idx].iter().collect();
                spans.push(PythonHighlightSpan { text, color: col_number });
                continue;
            }

            // 6. Identifiers / Keywords
            if ch.is_alphabetic() || ch == '_' {
                let start = idx;
                while idx < len && (chars[idx].is_alphanumeric() || chars[idx] == '_') {
                    idx += 1;
                }
                let word: String = chars[start..idx].iter().collect();

                let color = match word.as_str() {
                    // Python keywords (Purple)
                    "def" | "async" | "await" | "class" | "import" | "from" | "return" | "if"
                    | "elif" | "else" | "while" | "for" | "in" | "try" | "except" | "finally"
                    | "with" | "as" | "lambda" | "yield" | "pass" | "break" | "continue"
                    | "raise" | "assert" | "global" | "nonlocal" | "del" | "and" | "or"
                    | "not" | "is" => col_keyword,

                    // Built-ins and constants (Cyan)
                    "True" | "False" | "None" | "self" | "cls" | "print" | "len" | "range"
                    | "isinstance" | "int" | "str" | "float" | "bool" | "list" | "dict"
                    | "set" | "tuple" | "open" | "type" | "super" | "enumerate" | "zip"
                    | "map" | "filter" | "any" | "all" | "sum" | "min" | "max" | "abs"
                    | "round" | "Exception" | "ValueError" | "TypeError" | "KeyError"
                    | "IndexError" | "FileNotFoundError" | "RuntimeError" => col_builtin,

                    // Function call identifier if followed by '('
                    _ => {
                        let next_non_ws = chars[idx..].iter().find(|&&c| !c.is_whitespace());
                        if next_non_ws == Some(&'(') {
                            col_ident
                        } else {
                            col_punct
                        }
                    }
                };

                spans.push(PythonHighlightSpan { text: word, color });
                continue;
            }

            // 7. Punctuation / Operators
            let start = idx;
            idx += 1;
            let text: String = chars[start..idx].iter().collect();
            spans.push(PythonHighlightSpan { text, color: col_punct });
        }

        spans
    }
}

pub struct PythonStreamingFormatter;

impl PythonStreamingFormatter {
    /// Formats Python source code with clean indentation and whitespace normalization.
    pub fn format<R: Read, W: Write>(
        reader: R,
        mut writer: W,
        action: FormatAction,
        cancel: Arc<AtomicBool>,
        total_bytes: u64,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<u64, String> {
        let start = Instant::now();
        let buf_reader = BufReader::with_capacity(256 * 1024, reader);
        let mut processed_bytes: u64 = 0;
        let mut consecutive_blank_lines = 0;

        let indent_str = match action {
            FormatAction::Beautify { indent_size, use_tabs } => {
                if use_tabs {
                    "\t".to_string()
                } else {
                    " ".repeat(indent_size)
                }
            }
            FormatAction::Minify => "".to_string(),
        };

        for line_res in buf_reader.lines() {
            if cancel.load(Ordering::Relaxed) {
                return Err("Formatting cancelled by user".to_string());
            }

            let raw_line = line_res.map_err(|e| format!("Failed to read line: {}", e))?;
            processed_bytes += raw_line.len() as u64 + 1;

            if let Some(ref p) = progress {
                if let Ok(mut p_guard) = p.write() {
                    p_guard.update(processed_bytes, start);
                }
            }

            let trimmed_end = raw_line.trim_end();

            // Handle blank lines: normalize max 2 blank lines (PEP 8)
            if trimmed_end.trim().is_empty() {
                consecutive_blank_lines += 1;
                if consecutive_blank_lines <= 2 {
                    writeln!(writer).map_err(|e| format!("Write error: {}", e))?;
                }
                continue;
            }

            consecutive_blank_lines = 0;

            // Normalize indentation tabs vs spaces if beautifying
            let line_to_write = if !indent_str.is_empty() {
                let leading_ws_count = trimmed_end.chars().take_while(|c| c.is_whitespace()).count();
                let content = &trimmed_end[leading_ws_count..];
                let leading_spaces = trimmed_end[..leading_ws_count].replace('\t', "    ");
                let indent_level = (leading_spaces.len() + 3) / 4;
                format!("{}{}", indent_str.repeat(indent_level), content)
            } else {
                trimmed_end.to_string()
            };

            writeln!(writer, "{}", line_to_write).map_err(|e| format!("Write error: {}", e))?;
        }

        writer.flush().map_err(|e| format!("Flush error: {}", e))?;
        drop(writer);

        if let Some(ref p) = progress {
            if let Ok(mut p_guard) = p.write() {
                p_guard.update(total_bytes, start);
                p_guard.progress_pct = 100.0;
                p_guard.eta_secs = Some(0.0);
                if p_guard.defer_finish {
                    p_guard.is_finalizing = true;
                } else {
                    p_guard.is_finished = true;
                }
            }
        }

        Ok(processed_bytes)
    }

    /// Formats a Python file from disk to an output path.
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
            let defer_finish = prog.read().unwrap().defer_finish;
            let mut new_prog = FormattingProgress::new(total_bytes);
            new_prog.defer_finish = defer_finish;
            *prog.write().unwrap() = new_prog;
        }

        let dst_file = File::create(dst_path.as_ref()).map_err(|e| format!("Failed to create destination file: {}", e))?;
        let reader = BufReader::with_capacity(256 * 1024, src_file);
        let writer = BufWriter::with_capacity(256 * 1024, dst_file);

        let res = Self::format(reader, writer, action, Arc::clone(&cancel), total_bytes, progress.clone());

        if let Err(ref err) = res {
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

    #[test]
    fn test_python_syntax_highlighting_colors_keywords_and_strings() {
        let line = "async def fetch_page(client, timeout=20.0): # fetches page";
        let spans = PythonSyntaxHighlighter::highlight_line_themed(line, true);

        // Keywords in purple
        assert!(spans.iter().any(|s| s.text == "async" && s.color == Color32::from_rgb(198, 120, 221)));
        assert!(spans.iter().any(|s| s.text == "def" && s.color == Color32::from_rgb(198, 120, 221)));

        // Number in orange
        assert!(spans.iter().any(|s| s.text == "20.0" && s.color == Color32::from_rgb(209, 154, 102)));

        // Comment in slate
        assert!(spans.iter().any(|s| s.text.contains("# fetches page") && s.color == Color32::from_rgb(92, 99, 112)));
    }

    #[test]
    fn test_python_syntax_highlighting_triple_quotes() {
        let line = r#""""Docstring title""""#;
        let spans = PythonSyntaxHighlighter::highlight_line_themed(line, true);
        assert_eq!(spans[0].text, r#""""Docstring title""""#);
        assert_eq!(spans[0].color, Color32::from_rgb(152, 195, 121));
    }

    #[test]
    fn test_python_streaming_formatter_normalizes_indentation_and_blank_lines() {
        let raw_py = "def foo():\n\t\tprint('hello')\n\n\n\n\nprint('done')\n";
        let cancel = Arc::new(AtomicBool::new(false));
        let mut out = Vec::new();

        let res = PythonStreamingFormatter::format(
            raw_py.as_bytes(),
            &mut out,
            FormatAction::Beautify { indent_size: 4, use_tabs: false },
            cancel,
            raw_py.len() as u64,
            None,
        );

        assert!(res.is_ok());
        let formatted = String::from_utf8(out).unwrap();
        // Tabs normalized to 8 spaces (2 levels of 4)
        assert!(formatted.contains("        print('hello')"));
        // Multiple consecutive blank lines normalized
        assert!(!formatted.contains("\n\n\n\n"));
    }
}
