use crate::file_engine::{Encoding, FileEngine};

/// Lines shorter than this are displayed in full; longer lines get a clickable
/// [truncated] badge that opens the Full Line Inspector.
/// 16 384 characters covers 99.9 % of real-world long lines (minified JSON etc.)
pub const MAX_DISPLAY_LINE_LEN: usize = 16_384;

#[derive(Debug, Clone)]
pub struct ViewportLine {
    pub line_number: usize,
    pub byte_offset: u64,
    pub text: String,
    /// True when the line was cut at MAX_DISPLAY_LINE_LEN. The full content
    /// must be fetched from the engine via `read_range(byte_offset, …)`.
    pub is_truncated: bool,
}

#[derive(Debug, Clone)]
pub struct Viewport {
    pub start_offset: u64,
    pub lines: Vec<ViewportLine>,
    pub total_file_size: u64,
    pub encoding: Encoding,
    pub max_display_line_len: usize,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            start_offset: 0,
            lines: Vec::new(),
            total_file_size: 0,
            encoding: Encoding::Utf8,
            max_display_line_len: MAX_DISPLAY_LINE_LEN,
        }
    }
}

impl Viewport {
    pub fn new() -> Self {
        Self::default()
    }

    /// Loads lines using the background LineIndex for exact line-number positioning.
    pub fn load_lines_indexed(
        &mut self,
        engine: &FileEngine,
        line_index: &crate::file_engine::LineIndex,
        target_line: usize,
        desired_line_count: usize,
    ) {
        self.total_file_size = engine.size();
        self.encoding = engine.detect_encoding();
        self.lines.clear();

        if engine.is_empty() {
            return;
        }

        let line = target_line.max(1);
        let start_offset = if let Some(offset) = line_index.line_to_byte_offset(engine, line) {
            offset
        } else {
            let total_known = line_index.total_lines().max(1);
            if line_index.is_complete() {
                engine.size()
            } else {
                let ratio = (line as f64) / (total_known as f64);
                ((ratio * engine.size() as f64) as u64).min(engine.size())
            }
        };

        self.start_offset = start_offset;
        let actual_start_line = line_index.byte_offset_to_line(engine, start_offset);
        let max_window_bytes = (desired_line_count * 512).max(65536);
        self.load_from_engine(engine, start_offset, max_window_bytes, actual_start_line);
        if self.lines.len() > desired_line_count {
            self.lines.truncate(desired_line_count);
        }

        // Safety fallback: if viewport is empty but the file has content, load the tail of the file
        if self.lines.is_empty() && !engine.is_empty() {
            let total = line_index.total_lines().max(1);
            let fallback_line = total.saturating_sub(desired_line_count / 2).max(1);
            if fallback_line < line {
                if let Some(offset) = line_index.line_to_byte_offset(engine, fallback_line) {
                    self.start_offset = offset;
                    let actual = line_index.byte_offset_to_line(engine, offset);
                    self.load_from_engine(engine, offset, max_window_bytes, actual);
                    if self.lines.len() > desired_line_count {
                        self.lines.truncate(desired_line_count);
                    }
                }
            }
        }
    }

    /// Loads lines specifically from a VirtualSlice (instant virtual sub-file).
    pub fn load_virtual_lines(
        &mut self,
        engine: &FileEngine,
        line_index: &crate::file_engine::LineIndex,
        slice: &crate::file_engine::VirtualSlice,
        target_virtual_line: usize,
        desired_line_count: usize,
    ) {
        self.total_file_size = engine.size();
        self.encoding = engine.detect_encoding();
        self.lines.clear();

        if engine.is_empty() || slice.matching_lines.is_empty() {
            return;
        }

        let start_idx = target_virtual_line.saturating_sub(1).min(slice.matching_lines.len() - 1);
        let count = desired_line_count.min(slice.matching_lines.len() - start_idx);

        for i in 0..count {
            let physical_line = slice.matching_lines[start_idx + i];
            if let Some(offset) = line_index.line_to_byte_offset(engine, physical_line) {
                let max_len = 4096.min((engine.size() - offset) as usize);
                if let Ok(bytes) = engine.read_range(offset, max_len) {
                    let end_pos = memchr::memchr(b'\n', bytes).unwrap_or(bytes.len());
                    let mut line_bytes = &bytes[..end_pos];
                    if line_bytes.ends_with(b"\r") {
                        line_bytes = &line_bytes[..line_bytes.len() - 1];
                    }
                    let text = String::from_utf8_lossy(line_bytes).into_owned();
                    self.lines.push(ViewportLine {
                        line_number: physical_line,
                        byte_offset: offset,
                        text,
                        is_truncated: false,
                    });
                }
            }
        }
    }

    /// Loads an initial window of lines from the file engine.
    /// Reads at most `max_window_bytes` from `start_offset` and parses lines.
    /// Never loads the entire file into memory.
    pub fn load_from_engine(
        &mut self,
        engine: &FileEngine,
        start_offset: u64,
        max_window_bytes: usize,
        start_line_num: usize,
    ) {
        self.total_file_size = engine.size();
        self.encoding = engine.detect_encoding();
        self.start_offset = start_offset;
        self.lines.clear();

        if engine.is_empty() || start_offset >= engine.size() {
            return;
        }

        let bom_skip = if start_offset == 0 {
            self.encoding.bom_length() as u64
        } else {
            0
        };

        let effective_start = start_offset + bom_skip;
        if effective_start >= engine.size() {
            return;
        }

        let read_len = max_window_bytes.min((engine.size() - effective_start) as usize);
        let raw_chunk = match engine.read_range(effective_start, read_len) {
            Ok(bytes) => bytes,
            Err(_) => return,
        };

        match self.encoding {
            Encoding::Utf16Le | Encoding::Utf16Be => {
                self.parse_utf16_lines(raw_chunk, effective_start, start_line_num);
            }
            _ => {
                self.parse_utf8_lines(raw_chunk, effective_start, start_line_num);
            }
        }
    }

    /// Loads lines directly from a PieceTable instead of the disk engine.
    /// This ensures deleted lines, inserted lines, and edits are strictly respected
    /// and never resurrected from the unedited disk file.
    pub fn load_from_piece_table(
        &mut self,
        piece_table: &crate::editor::PieceTable,
        engine: &FileEngine,
        start_offset: u64,
        max_window_bytes: usize,
        start_line_num: usize,
    ) {
        let total_bytes = piece_table.total_length();
        self.total_file_size = total_bytes;
        self.encoding = engine.detect_encoding();
        self.start_offset = start_offset;
        self.lines.clear();

        if total_bytes == 0 || start_offset >= total_bytes {
            return;
        }

        let bom_skip = if start_offset == 0 {
            self.encoding.bom_length() as u64
        } else {
            0
        };

        let effective_start = start_offset + bom_skip;
        if effective_start >= total_bytes {
            return;
        }

        let read_len = max_window_bytes.min((total_bytes - effective_start) as usize);
        let mut raw_chunk = Vec::with_capacity(read_len);
        if piece_table.read_range(engine, effective_start, read_len, &mut raw_chunk).is_err() {
            return;
        }

        match self.encoding {
            Encoding::Utf16Le | Encoding::Utf16Be => {
                self.parse_utf16_lines(&raw_chunk, effective_start, start_line_num);
            }
            _ => {
                self.parse_utf8_lines(&raw_chunk, effective_start, start_line_num);
            }
        }
    }

    fn parse_utf8_lines(&mut self, chunk: &[u8], base_offset: u64, start_line: usize) {
        let mut line_num = start_line;
        let mut current_offset = base_offset;
        let mut line_start = 0;

        for i in 0..chunk.len() {
            if chunk[i] == b'\n' {
                let mut line_end = i;
                if line_end > line_start && chunk[line_end - 1] == b'\r' {
                    line_end -= 1;
                }

                let line_bytes = &chunk[line_start..line_end];
                let (text, is_truncated) = self.decode_and_sanitize(line_bytes);

                self.lines.push(ViewportLine {
                    line_number: line_num,
                    byte_offset: current_offset,
                    text,
                    is_truncated,
                });

                line_num += 1;
                current_offset = base_offset + (i as u64) + 1;
                line_start = i + 1;
            }
        }

        // Remainder line if chunk ended without newline
        if line_start < chunk.len() {
            let line_bytes = &chunk[line_start..];
            let (text, is_truncated) = self.decode_and_sanitize(line_bytes);
            self.lines.push(ViewportLine {
                line_number: line_num,
                byte_offset: current_offset,
                text,
                is_truncated,
            });
        }
    }

    fn parse_utf16_lines(&mut self, chunk: &[u8], base_offset: u64, start_line: usize) {
        let decoded = self.encoding.decode_chunk(chunk);
        let mut line_num = start_line;
        let mut current_offset = base_offset;

        for line in decoded.lines() {
            let (text, is_truncated) = if line.len() > self.max_display_line_len {
                (
                    format!("{}... [truncated]", &line[..self.max_display_line_len]),
                    true,
                )
            } else {
                (line.to_string(), false)
            };

            self.lines.push(ViewportLine {
                line_number: line_num,
                byte_offset: current_offset,
                text,
                is_truncated,
            });

            line_num += 1;
            current_offset += (line.len() * 2) as u64 + 4; // approximate byte distance for UTF-16
        }
    }

    /// Returns `(display_text, was_truncated)`.
    fn decode_and_sanitize(&self, bytes: &[u8]) -> (String, bool) {
        let raw_str = String::from_utf8_lossy(bytes);
        if raw_str.len() > self.max_display_line_len {
            let mut truncated = raw_str[..self.max_display_line_len].to_string();
            truncated.push_str("... [truncated]");
            (truncated, true)
        } else {
            (raw_str.into_owned(), false)
        }
    }

    /// Apply line overrides from active editing document
    pub fn apply_line_overrides(&mut self, overrides: &std::collections::HashMap<usize, String>) {
        if overrides.is_empty() {
            return;
        }
        for line in &mut self.lines {
            if let Some(new_text) = overrides.get(&line.line_number) {
                line.text = new_text.clone();
                line.is_truncated = false; // edited lines are never truncated
            }
        }
    }
}
