use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crate::file_engine::{Encoding, FileEngine, LineIndex, CHECKPOINT_INTERVAL};

const BUFFER_CHUNK_SIZE: usize = 256 * 1024; // 256 KB reusable streaming buffer

pub struct LineIndexer;

impl LineIndexer {
    /// Spawns a background thread that scans the file and populates the line index progressively.
    /// Reads using a streaming chunk buffer to keep process working set bounded (< 20 MB)
    /// even on 1 GB – 50 GB files.
    pub fn spawn(
        engine: Arc<FileEngine>,
        index: Arc<LineIndex>,
        cancel: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
        thread::Builder::new()
            .name("line-indexer".to_string())
            .spawn(move || {
                let file_size = engine.size();
                if file_size == 0 {
                    index.mark_complete(0);
                    return;
                }

                let encoding = engine.detect_encoding();
                match encoding {
                    Encoding::Utf16Le | Encoding::Utf16Be => {
                        Self::index_utf16(engine, index, cancel, encoding);
                    }
                    _ => {
                        Self::index_utf8(engine, index, cancel);
                    }
                }
            })
            .expect("Failed to spawn line indexer thread")
    }

    fn index_utf8(
        engine: Arc<FileEngine>,
        index: Arc<LineIndex>,
        cancel: Arc<AtomicBool>,
    ) {
        let file_size = engine.size();
        let path = engine.path();

        // Use a separate streaming file reader to avoid inflating process working set with mmap pages
        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(_) => {
                index.mark_complete(1);
                return;
            }
        };

        let bom_len = engine.detect_encoding().bom_length() as u64;
        if bom_len > 0 {
            let _ = file.seek(SeekFrom::Start(bom_len));
        }

        let mut buffer = vec![0u8; BUFFER_CHUNK_SIZE];
        let mut current_offset: u64 = bom_len;
        let mut total_lines_found: usize = 1;
        let mut last_checkpoint_line: usize = 1;

        let mut batch_checkpoints = Vec::with_capacity(1024);
        let mut batch_lines = 0;

        let start_time = Instant::now();
        let mut last_speed_update = Instant::now();

        loop {
            if cancel.load(Ordering::Relaxed) {
                return;
            }

            let bytes_read = match file.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => n,
                Err(_) => break,
            };

            let slice = &buffer[..bytes_read];

            for pos in memchr::memchr_iter(b'\n', slice) {
                total_lines_found += 1;
                let next_line_offset = current_offset + (pos as u64) + 1;

                if total_lines_found - last_checkpoint_line >= CHECKPOINT_INTERVAL {
                    if next_line_offset < file_size {
                        batch_checkpoints.push(next_line_offset);
                    }
                    last_checkpoint_line = total_lines_found;
                }
                batch_lines += 1;

                if batch_checkpoints.len() >= 512 || batch_lines >= 16384 {
                    index.add_checkpoints_batch(&batch_checkpoints, batch_lines, current_offset + (pos as u64) + 1);
                    batch_checkpoints.clear();
                    batch_lines = 0;
                }
            }

            current_offset += bytes_read as u64;

            if !batch_checkpoints.is_empty() || batch_lines > 0 {
                index.add_checkpoints_batch(&batch_checkpoints, batch_lines, current_offset);
                batch_checkpoints.clear();
                batch_lines = 0;
            } else {
                index.add_checkpoints_batch(&[], 0, current_offset);
            }

            if last_speed_update.elapsed().as_millis() >= 300 {
                let elapsed = start_time.elapsed().as_secs_f64();
                if elapsed > 0.0 {
                    let mb_per_sec = ((current_offset as f64) / (1024.0 * 1024.0) / elapsed) as u64;
                    index.set_speed(mb_per_sec);
                }
                last_speed_update = Instant::now();
            }
        }

        index.mark_complete(total_lines_found);
    }

    fn index_utf16(
        engine: Arc<FileEngine>,
        index: Arc<LineIndex>,
        cancel: Arc<AtomicBool>,
        encoding: Encoding,
    ) {
        let file_size = engine.size();
        let path = engine.path();

        let mut file = match File::open(path) {
            Ok(f) => f,
            Err(_) => {
                index.mark_complete(1);
                return;
            }
        };

        let bom_len = encoding.bom_length() as u64;
        if bom_len > 0 {
            let _ = file.seek(SeekFrom::Start(bom_len));
        }

        let mut buffer = vec![0u8; BUFFER_CHUNK_SIZE];
        let mut current_offset: u64 = bom_len;
        let mut total_lines_found: usize = 1;
        let mut last_checkpoint_line: usize = 1;
        let mut batch_checkpoints = Vec::with_capacity(1024);
        let mut batch_lines = 0;

        let nl_pattern: [u8; 2] = match encoding {
            Encoding::Utf16Le => [b'\n', 0],
            _ => [0, b'\n'],
        };

        loop {
            if cancel.load(Ordering::Relaxed) {
                return;
            }

            let bytes_read = match file.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };

            let slice = &buffer[..bytes_read];
            let mut i = 0;
            while i + 1 < slice.len() {
                if slice[i] == nl_pattern[0] && slice[i + 1] == nl_pattern[1] {
                    total_lines_found += 1;
                    let next_line_offset = current_offset + (i as u64) + 2;

                    if total_lines_found - last_checkpoint_line >= CHECKPOINT_INTERVAL {
                        if next_line_offset < file_size {
                            batch_checkpoints.push(next_line_offset);
                        }
                        last_checkpoint_line = total_lines_found;
                    }
                    batch_lines += 1;
                    i += 2;
                } else {
                    i += 2;
                }
            }

            current_offset += bytes_read as u64;
            index.add_checkpoints_batch(&batch_checkpoints, batch_lines, current_offset);
            batch_checkpoints.clear();
            batch_lines = 0;
        }

        index.mark_complete(total_lines_found);
    }
}
