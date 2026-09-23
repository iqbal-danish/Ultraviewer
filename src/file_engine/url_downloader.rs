use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use crossbeam_channel::Sender;
use flate2::read::GzDecoder;

const BUFFER_SIZE: usize = 64 * 1024; // 64 KB streaming chunks
const PROGRESS_INTERVAL_MS: u128 = 100; // UI progress update interval

#[derive(Debug, Clone)]
pub enum DownloadStatus {
    Connecting,
    Downloading {
        bytes_downloaded: u64,
        total_bytes: Option<u64>,
        speed_mb_s: f64,
        elapsed_secs: f64,
    },
    Decompressing {
        bytes_decompressed: u64,
    },
    Completed {
        file_path: PathBuf,
        file_size: u64,
        is_decompressed: bool,
        elapsed_secs: f64,
    },
    Failed {
        error: String,
    },
    Cancelled,
}

pub struct UrlDownloader;

impl UrlDownloader {
    /// Spawns a background thread that streams an HTTP/HTTPS URL directly to disk.
    /// Supports automatic `.gz` streaming decompression and cooperative cancellation.
    pub fn spawn_download(
        url: String,
        custom_headers: Vec<(String, String)>,
        custom_output_dir: Option<PathBuf>,
        cancel: Arc<AtomicBool>,
        progress_tx: Sender<DownloadStatus>,
    ) -> JoinHandle<()> {
        thread::Builder::new()
            .name("url-downloader".to_string())
            .spawn(move || {
                Self::run_download(url, custom_headers, custom_output_dir, cancel, progress_tx);
            })
            .expect("Failed to spawn url-downloader thread")
    }

    fn run_download(
        url: String,
        custom_headers: Vec<(String, String)>,
        custom_output_dir: Option<PathBuf>,
        cancel: Arc<AtomicBool>,
        progress_tx: Sender<DownloadStatus>,
    ) {
        let _ = progress_tx.send(DownloadStatus::Connecting);

        // 1. Determine destination directory
        let download_dir = custom_output_dir.unwrap_or_else(|| {
            std::env::temp_dir().join("ultraviewer_downloads")
        });
        if let Err(e) = fs::create_dir_all(&download_dir) {
            let _ = progress_tx.send(DownloadStatus::Failed {
                error: format!("Cannot create download folder: {}", e),
            });
            return;
        }

        // 2. Derive base filename from URL
        let url_clean = url.split('?').next().unwrap_or(&url);
        let raw_filename = url_clean.rsplit('/').next().unwrap_or("feed.xml").trim();
        let safe_filename = if raw_filename.is_empty() || raw_filename == "/" {
            "feed.xml".to_string()
        } else {
            raw_filename.to_string()
        };

        let is_gzip = safe_filename.ends_with(".gz") || safe_filename.ends_with(".gzip");

        // Temporary unique download file path
        let timestamp = Instant::now().elapsed().as_nanos();
        let temp_download_path = download_dir.join(format!("{}_{}", timestamp, safe_filename));

        // 3. Configure ureq agent
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(30))
            .timeout_read(Duration::from_secs(60))
            .build();

        let mut req = agent.get(&url)
            .set("User-Agent", "UltraViewer/1.0");

        for (k, v) in custom_headers {
            if !k.trim().is_empty() && !v.trim().is_empty() {
                req = req.set(k.trim(), v.trim());
            }
        }

        let resp = match req.call() {
            Ok(r) => r,
            Err(e) => {
                let _ = progress_tx.send(DownloadStatus::Failed {
                    error: format!("HTTP Request failed: {}", e),
                });
                return;
            }
        };

        if resp.status() < 200 || resp.status() >= 300 {
            let _ = progress_tx.send(DownloadStatus::Failed {
                error: format!("HTTP Error {}: {}", resp.status(), resp.status_text()),
            });
            return;
        }

        let total_bytes: Option<u64> = resp.header("Content-Length")
            .and_then(|v| v.parse::<u64>().ok());

        // Check if server indicated gzip via Content-Encoding (even if URL didn't end in .gz)
        let server_gzip = resp.header("Content-Encoding")
            .map(|e| e.to_lowercase().contains("gzip"))
            .unwrap_or(false);
        let should_decompress = is_gzip || server_gzip;

        let mut reader = resp.into_reader();
        let mut file = match File::create(&temp_download_path) {
            Ok(f) => BufWriter::with_capacity(BUFFER_SIZE, f),
            Err(e) => {
                let _ = progress_tx.send(DownloadStatus::Failed {
                    error: format!("Cannot create local file {}: {}", temp_download_path.display(), e),
                });
                return;
            }
        };

        // 4. Stream data to disk with progress tracking
        let start_time = Instant::now();
        let mut last_progress_time = Instant::now();
        let mut bytes_downloaded: u64 = 0;
        let mut buffer = [0u8; BUFFER_SIZE];

        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(file);
                let _ = fs::remove_file(&temp_download_path);
                let _ = progress_tx.send(DownloadStatus::Cancelled);
                return;
            }

            match reader.read(&mut buffer) {
                Ok(0) => break, // EOF reached
                Ok(n) => {
                    if let Err(e) = file.write_all(&buffer[..n]) {
                        drop(file);
                        let _ = fs::remove_file(&temp_download_path);
                        let _ = progress_tx.send(DownloadStatus::Failed {
                            error: format!("Disk write error: {}", e),
                        });
                        return;
                    }
                    bytes_downloaded += n as u64;

                    if last_progress_time.elapsed().as_millis() >= PROGRESS_INTERVAL_MS {
                        let elapsed_secs = start_time.elapsed().as_secs_f64();
                        let speed_mb_s = if elapsed_secs > 0.0 {
                            (bytes_downloaded as f64 / (1024.0 * 1024.0)) / elapsed_secs
                        } else {
                            0.0
                        };

                        let _ = progress_tx.send(DownloadStatus::Downloading {
                            bytes_downloaded,
                            total_bytes,
                            speed_mb_s,
                            elapsed_secs,
                        });
                        last_progress_time = Instant::now();
                    }
                }
                Err(e) => {
                    drop(file);
                    let _ = fs::remove_file(&temp_download_path);
                    let _ = progress_tx.send(DownloadStatus::Failed {
                        error: format!("Network stream error: {}", e),
                    });
                    return;
                }
            }
        }

        if let Err(e) = file.flush() {
            let _ = fs::remove_file(&temp_download_path);
            let _ = progress_tx.send(DownloadStatus::Failed {
                error: format!("Failed to flush downloaded file: {}", e),
            });
            return;
        }
        drop(file);

        let elapsed_secs = start_time.elapsed().as_secs_f64();

        // 5. Automatic Gzip Decompression if needed
        if should_decompress {
            let decompressed_name = if safe_filename.ends_with(".gz") {
                safe_filename[..safe_filename.len() - 3].to_string()
            } else if safe_filename.ends_with(".gzip") {
                safe_filename[..safe_filename.len() - 5].to_string()
            } else {
                format!("{}.xml", safe_filename)
            };

            let final_decompressed_path = download_dir.join(&decompressed_name);
            let _ = progress_tx.send(DownloadStatus::Decompressing {
                bytes_decompressed: 0,
            });

            match Self::decompress_gzip(&temp_download_path, &final_decompressed_path, &cancel, &progress_tx) {
                Ok(decompressed_size) => {
                    // Remove temporary compressed download file
                    let _ = fs::remove_file(&temp_download_path);
                    let _ = progress_tx.send(DownloadStatus::Completed {
                        file_path: final_decompressed_path,
                        file_size: decompressed_size,
                        is_decompressed: true,
                        elapsed_secs,
                    });
                }
                Err(e) => {
                    let _ = fs::remove_file(&temp_download_path);
                    let _ = progress_tx.send(DownloadStatus::Failed {
                        error: format!("Gzip decompression failed: {}", e),
                    });
                }
            }
        } else {
            // Uncompressed file ready to view directly
            let final_path = download_dir.join(&safe_filename);
            let _ = fs::rename(&temp_download_path, &final_path).or_else(|_| {
                fs::copy(&temp_download_path, &final_path).and_then(|_| fs::remove_file(&temp_download_path))
            });

            let _ = progress_tx.send(DownloadStatus::Completed {
                file_path: final_path,
                file_size: bytes_downloaded,
                is_decompressed: false,
                elapsed_secs,
            });
        }
    }

    fn decompress_gzip(
        src_gz: &Path,
        dst_uncompressed: &Path,
        cancel: &Arc<AtomicBool>,
        progress_tx: &Sender<DownloadStatus>,
    ) -> Result<u64, String> {
        let in_file = File::open(src_gz).map_err(|e| format!("Cannot open compressed file: {}", e))?;
        let gz_decoder = GzDecoder::new(BufReader::with_capacity(BUFFER_SIZE, in_file));
        let mut reader = BufReader::with_capacity(BUFFER_SIZE, gz_decoder);

        let out_file = File::create(dst_uncompressed).map_err(|e| format!("Cannot create target file: {}", e))?;
        let mut writer = BufWriter::with_capacity(BUFFER_SIZE, out_file);

        let mut buffer = [0u8; BUFFER_SIZE];
        let mut decompressed_total: u64 = 0;
        let mut last_update = Instant::now();

        loop {
            if cancel.load(Ordering::Relaxed) {
                drop(writer);
                let _ = fs::remove_file(dst_uncompressed);
                return Err("Decompression cancelled by user".to_string());
            }

            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => {
                    writer.write_all(&buffer[..n]).map_err(|e| format!("Write error during decompression: {}", e))?;
                    decompressed_total += n as u64;

                    if last_update.elapsed().as_millis() >= PROGRESS_INTERVAL_MS {
                        let _ = progress_tx.send(DownloadStatus::Decompressing {
                            bytes_decompressed: decompressed_total,
                        });
                        last_update = Instant::now();
                    }
                }
                Err(e) => {
                    drop(writer);
                    let _ = fs::remove_file(dst_uncompressed);
                    return Err(format!("Corrupt gzip archive: {}", e));
                }
            }
        }

        writer.flush().map_err(|e| format!("Flush error during decompression: {}", e))?;
        Ok(decompressed_total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;

    #[test]
    fn test_decompress_gzip_streaming() {
        let temp_dir = std::env::temp_dir().join("ultraviewer_test_gzip");
        let _ = fs::create_dir_all(&temp_dir);

        let original_data = b"<feed><title>Test Feed Streaming Gzip</title><item>Data 123</item></feed>";
        let gz_path = temp_dir.join("test_feed.xml.gz");
        let out_path = temp_dir.join("test_feed.xml");

        // Compress data to gz_path
        {
            let file = File::create(&gz_path).unwrap();
            let mut encoder = GzEncoder::new(file, Compression::default());
            encoder.write_all(original_data).unwrap();
            encoder.finish().unwrap();
        }

        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, _rx) = crossbeam_channel::unbounded();

        let decompressed_size = UrlDownloader::decompress_gzip(&gz_path, &out_path, &cancel, &tx).unwrap();
        assert_eq!(decompressed_size, original_data.len() as u64);

        let read_back = fs::read(&out_path).unwrap();
        assert_eq!(&read_back, original_data);

        let _ = fs::remove_file(&gz_path);
        let _ = fs::remove_file(&out_path);
        let _ = fs::remove_dir(&temp_dir);
    }

    #[test]
    fn test_decompress_gzip_cancellation() {
        let temp_dir = std::env::temp_dir().join("ultraviewer_test_cancel");
        let _ = fs::create_dir_all(&temp_dir);

        let original_data = vec![b'A'; 256 * 1024];
        let gz_path = temp_dir.join("test_cancel.xml.gz");
        let out_path = temp_dir.join("test_cancel.xml");

        {
            let file = File::create(&gz_path).unwrap();
            let mut encoder = GzEncoder::new(file, Compression::default());
            encoder.write_all(&original_data).unwrap();
            encoder.finish().unwrap();
        }

        // Cancel immediately
        let cancel = Arc::new(AtomicBool::new(true));
        let (tx, _rx) = crossbeam_channel::unbounded();

        let result = UrlDownloader::decompress_gzip(&gz_path, &out_path, &cancel, &tx);
        assert!(result.is_err());
        assert!(!out_path.exists(), "Target file should have been cleaned up on cancel");

        let _ = fs::remove_file(&gz_path);
        let _ = fs::remove_dir(&temp_dir);
    }
}

