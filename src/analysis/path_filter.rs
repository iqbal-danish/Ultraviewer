use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use crossbeam_channel::Sender;

#[derive(Debug, Clone)]
pub struct FilterProgress {
    pub bytes_processed: u64,
    pub total_bytes: u64,
    pub matches_count: u64,
    pub is_complete: bool,
}

pub struct StreamingPathFilter;

impl StreamingPathFilter {
    pub fn filter_streaming(
        src_path: PathBuf,
        dst_path: Option<PathBuf>,
        query: String,
        cancel: Arc<AtomicBool>,
        progress_tx: Sender<FilterProgress>,
    ) -> Result<u64, String> {
        let file = File::open(&src_path)
            .map_err(|e| format!("Failed to open source file: {}", e))?;
        let total_bytes = file.metadata().map(|m| m.len()).unwrap_or(0);
        let mut reader = BufReader::with_capacity(256 * 1024, file);

        let mut writer = if let Some(ref out) = dst_path {
            let out_file = File::create(out)
                .map_err(|e| format!("Failed to create destination file: {}", e))?;
            Some(BufWriter::with_capacity(256 * 1024, out_file))
        } else {
            None
        };

        let mut line_buf = String::new();
        let mut bytes_read: u64 = 0;
        let mut matches_count: u64 = 0;
        let query_lower = query.to_lowercase();
        let mut last_progress_report = std::time::Instant::now();

        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("Filtering cancelled by user.".to_string());
            }

            line_buf.clear();
            let n = reader.read_line(&mut line_buf)
                .map_err(|e| format!("Read error: {}", e))?;
            if n == 0 {
                break;
            }

            bytes_read += n as u64;

            if line_buf.to_lowercase().contains(&query_lower) {
                matches_count += 1;
                if let Some(ref mut w) = writer {
                    let _ = w.write_all(line_buf.as_bytes());
                }
            }

            if last_progress_report.elapsed().as_millis() >= 100 {
                last_progress_report = std::time::Instant::now();
                let _ = progress_tx.try_send(FilterProgress {
                    bytes_processed: bytes_read,
                    total_bytes,
                    matches_count,
                    is_complete: false,
                });
            }
        }

        if let Some(mut w) = writer {
            let _ = w.flush();
        }

        let _ = progress_tx.try_send(FilterProgress {
            bytes_processed: bytes_read,
            total_bytes,
            matches_count,
            is_complete: true,
        });

        Ok(matches_count)
    }
}
