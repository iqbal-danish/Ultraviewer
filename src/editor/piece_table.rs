use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use crate::file_engine::FileEngine;
use crate::formats::formatter::FormattingProgress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PieceSource {
    Original { offset: u64, length: u64 },
    Add { offset: u64, length: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    pub source: PieceSource,
    pub length: u64,
}

impl Piece {
    pub fn original(offset: u64, length: u64) -> Self {
        Self {
            source: PieceSource::Original { offset, length },
            length,
        }
    }

    pub fn add(offset: u64, length: u64) -> Self {
        Self {
            source: PieceSource::Add { offset, length },
            length,
        }
    }
}

/// Zero-DOM Piece Table for multi-gigabyte editing with strictly bounded memory
#[derive(Debug, Clone)]
pub struct PieceTable {
    pub original_length: u64,
    pub add_buffer: Vec<u8>,
    pub pieces: Vec<Piece>,
}

impl PieceTable {
    const CHUNK_BUFFER_SIZE: usize = 256 * 1024; // 256 KB streaming chunks

    /// Create a new PieceTable referencing an existing file
    pub fn new(original_length: u64) -> Self {
        let pieces = if original_length > 0 {
            vec![Piece::original(0, original_length)]
        } else {
            Vec::new()
        };

        Self {
            original_length,
            add_buffer: Vec::new(),
            pieces,
        }
    }

    /// Total length of the document after all edits
    pub fn total_length(&self) -> u64 {
        self.pieces.iter().map(|p| p.length).sum()
    }

    /// Number of pieces in the table
    pub fn piece_count(&self) -> usize {
        self.pieces.len()
    }

    /// Memory consumed by the add buffer in bytes
    pub fn add_buffer_memory(&self) -> usize {
        self.add_buffer.len()
    }

    /// Find the piece index and offset within that piece corresponding to a document byte offset
    fn find_piece_at(&self, mut offset: u64) -> (usize, u64) {
        for (i, piece) in self.pieces.iter().enumerate() {
            if offset < piece.length {
                return (i, offset);
            }
            offset -= piece.length;
        }
        (self.pieces.len(), 0)
    }

    /// Split a piece at a relative offset inside that piece
    fn split_piece_at(&mut self, piece_idx: usize, offset_in_piece: u64) {
        if piece_idx >= self.pieces.len() || offset_in_piece == 0 || offset_in_piece >= self.pieces[piece_idx].length {
            return;
        }

        let piece = self.pieces[piece_idx];
        let len1 = offset_in_piece;
        let len2 = piece.length - offset_in_piece;

        let (p1, p2) = match piece.source {
            PieceSource::Original { offset, .. } => (
                Piece::original(offset, len1),
                Piece::original(offset + len1, len2),
            ),
            PieceSource::Add { offset, .. } => (
                Piece::add(offset, len1),
                Piece::add(offset + len1, len2),
            ),
        };

        self.pieces[piece_idx] = p1;
        self.pieces.insert(piece_idx + 1, p2);
    }

    /// Insert text at an arbitrary byte offset
    pub fn insert(&mut self, offset: u64, text: &[u8]) {
        if text.is_empty() {
            return;
        }

        let add_offset = self.add_buffer.len() as u64;
        self.add_buffer.extend_from_slice(text);
        let new_piece = Piece::add(add_offset, text.len() as u64);

        if self.pieces.is_empty() {
            self.pieces.push(new_piece);
            return;
        }

        let total_len = self.total_length();
        if offset >= total_len {
            // Append at the end
            self.pieces.push(new_piece);
            return;
        }

        let (piece_idx, offset_in_piece) = self.find_piece_at(offset);
        if offset_in_piece == 0 {
            self.pieces.insert(piece_idx, new_piece);
        } else {
            self.split_piece_at(piece_idx, offset_in_piece);
            self.pieces.insert(piece_idx + 1, new_piece);
        }
    }

    /// Delete a range of bytes [offset, offset + length)
    pub fn delete(&mut self, offset: u64, length: u64) {
        if length == 0 || self.pieces.is_empty() {
            return;
        }

        let total_len = self.total_length();
        if offset >= total_len {
            return;
        }

        let end_offset = (offset + length).min(total_len);
        let actual_len = end_offset - offset;
        if actual_len == 0 {
            return;
        }

        // Split at end_offset first so indices before it remain valid
        let (end_idx, end_sub) = self.find_piece_at(end_offset);
        if end_sub > 0 && end_idx < self.pieces.len() {
            self.split_piece_at(end_idx, end_sub);
        }

        // Split at start offset
        let (start_idx, start_sub) = self.find_piece_at(offset);
        let remove_start = if start_sub > 0 && start_idx < self.pieces.len() {
            self.split_piece_at(start_idx, start_sub);
            start_idx + 1
        } else {
            start_idx
        };

        // Find new end index
        let (new_end_idx, _) = self.find_piece_at(end_offset);
        if new_end_idx >= remove_start {
            self.pieces.drain(remove_start..new_end_idx);
        }
    }

    /// Replace a range of bytes with new text
    pub fn replace(&mut self, offset: u64, old_len: u64, new_text: &[u8]) {
        self.delete(offset, old_len);
        self.insert(offset, new_text);
    }

    /// Read a specific slice of bytes from the document into a vector
    pub fn read_range(
        &self,
        engine: &FileEngine,
        offset: u64,
        length: usize,
        out: &mut Vec<u8>,
    ) -> Result<(), String> {
        out.clear();
        if length == 0 {
            return Ok(());
        }

        let total_len = self.total_length();
        if offset >= total_len {
            return Ok(());
        }

        let read_len = (length as u64).min(total_len - offset);
        let target_end = offset + read_len;

        let mut curr_offset: u64 = 0;
        for piece in &self.pieces {
            let piece_start = curr_offset;
            let piece_end = curr_offset + piece.length;

            if piece_end > offset && piece_start < target_end {
                let overlap_start = offset.max(piece_start);
                let overlap_end = target_end.min(piece_end);
                let sub_offset = overlap_start - piece_start;
                let sub_len = (overlap_end - overlap_start) as usize;

                match piece.source {
                    PieceSource::Original { offset: orig_off, .. } => {
                        let file_read_offset = orig_off + sub_offset;
                        let slice = engine.read_range(file_read_offset, sub_len)
                            .map_err(|e| format!("Read from file engine failed: {}", e))?;
                        out.extend_from_slice(slice);
                    }
                    PieceSource::Add { offset: add_off, .. } => {
                        let add_read_offset = (add_off + sub_offset) as usize;
                        let slice = &self.add_buffer[add_read_offset..add_read_offset + sub_len];
                        out.extend_from_slice(slice);
                    }
                }
            }

            curr_offset = piece_end;
            if curr_offset >= target_end {
                break;
            }
        }

        Ok(())
    }

    /// Stream the entire document to a writer using 256 KB chunks without allocating full file in memory
    pub fn stream_write<W: Write>(
        &self,
        engine: &FileEngine,
        writer: &mut W,
        cancel: Arc<AtomicBool>,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<u64, String> {
        let start_time = Instant::now();
        let total_bytes = self.total_length();
        let mut total_written: u64 = 0;
        let mut last_progress = Instant::now();

        if let Some(ref prog) = progress {
            prog.write().unwrap().total_bytes = total_bytes;
        }

        for piece in &self.pieces {
            if cancel.load(Ordering::Relaxed) {
                return Err("Save cancelled by user".to_string());
            }

            match piece.source {
                PieceSource::Original { offset: orig_off, length } => {
                    let mut piece_pos: u64 = 0;
                    while piece_pos < length {
                        if cancel.load(Ordering::Relaxed) {
                            return Err("Save cancelled by user".to_string());
                        }

                        let chunk_size = ((length - piece_pos) as usize).min(Self::CHUNK_BUFFER_SIZE);
                        let slice = engine.read_range(orig_off + piece_pos, chunk_size)
                            .map_err(|e| format!("Failed to read source chunk from file engine: {}", e))?;

                        writer.write_all(slice).map_err(|e| e.to_string())?;
                        piece_pos += chunk_size as u64;
                        total_written += chunk_size as u64;

                        if last_progress.elapsed().as_millis() >= 100 {
                            if let Some(ref prog) = progress {
                                prog.write().unwrap().update(total_written, start_time);
                            }
                            last_progress = Instant::now();
                        }
                    }
                }
                PieceSource::Add { offset: add_off, length } => {
                    let mut piece_pos: u64 = 0;
                    while piece_pos < length {
                        if cancel.load(Ordering::Relaxed) {
                            return Err("Save cancelled by user".to_string());
                        }

                        let chunk_size = ((length - piece_pos) as usize).min(Self::CHUNK_BUFFER_SIZE);
                        let start_idx = (add_off + piece_pos) as usize;
                        let slice = &self.add_buffer[start_idx..start_idx + chunk_size];

                        writer.write_all(slice).map_err(|e| e.to_string())?;
                        piece_pos += chunk_size as u64;
                        total_written += chunk_size as u64;

                        if last_progress.elapsed().as_millis() >= 100 {
                            if let Some(ref prog) = progress {
                                prog.write().unwrap().update(total_written, start_time);
                            }
                            last_progress = Instant::now();
                        }
                    }
                }
            }
        }

        writer.flush().map_err(|e| e.to_string())?;

        if let Some(ref prog) = progress {
            prog.write().unwrap().finish(total_written, start_time);
        }

        Ok(total_written)
    }
}
