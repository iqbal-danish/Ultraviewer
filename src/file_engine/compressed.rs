use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use crossbeam_channel::Sender;
use flate2::read::GzDecoder;
use zip::ZipArchive;

use crate::formats::FileType;

const BUFFER_SIZE: usize = 64 * 1024; // 64 KB streaming buffer
const PROGRESS_INTERVAL_MS: u128 = 80;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionType {
    None,
    Gzip,
    Zip,
}

#[derive(Debug, Clone)]
pub struct ZipEntryInfo {
    pub index: usize,
    pub name: String,
    pub uncompressed_size: u64,
    pub compressed_size: u64,
    pub is_dir: bool,
    pub file_type: Option<FileType>,
}

#[derive(Debug, Clone)]
pub enum DecompressStatus {
    Decompressing {
        bytes_decompressed: u64,
        file_name: String,
    },
    Completed {
        output_path: PathBuf,
        total_bytes: u64,
        original_archive: PathBuf,
        display_name: String,
    },
    Cancelled,
    Error(String),
}

pub struct CompressedEngine;

impl CompressedEngine {
    /// Detects whether a file is a Gzip (.gz) stream or a ZIP (.zip) archive
    /// using both magic bytes and file extension fallbacks.
    pub fn detect_compression(path: &Path) -> CompressionType {
        // 1. Try reading the first 4 bytes for magic headers
        if let Ok(mut file) = File::open(path) {
            let mut magic = [0u8; 4];
            if let Ok(n) = file.read(&mut magic) {
                if n >= 2 && magic[0] == 0x1F && magic[1] == 0x8B {
                    return CompressionType::Gzip;
                }
                if n >= 4 && magic[0] == 0x50 && magic[1] == 0x4B {
                    // Standard PK headers: PK\x03\x04 (file), PK\x05\x06 (empty archive), PK\x07\x08 (spanned)
                    if (magic[2] == 0x03 && magic[3] == 0x04)
                        || (magic[2] == 0x05 && magic[3] == 0x06)
                        || (magic[2] == 0x07 && magic[3] == 0x08)
                    {
                        return CompressionType::Zip;
                    }
                }
            }
        }

        // 2. Fallback to extension check
        if let Some(ext) = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) {
            if ext == "gz" || ext == "gzip" {
                return CompressionType::Gzip;
            }
            if ext == "zip" {
                return CompressionType::Zip;
            }
        }

        CompressionType::None
    }

    /// Helper to get or create the UltraViewer temporary decompressed directory.
    pub fn get_temp_decompressed_dir() -> PathBuf {
        let dir = std::env::temp_dir().join("UltraViewer").join("decompressed");
        if !dir.exists() {
            let _ = fs::create_dir_all(&dir);
        }
        dir
    }

    /// Determines the best decompressed filename from a .gz archive name.
    /// E.g. "products.xml.gz" -> "products.xml"
    /// If no sub-extension exists (e.g. "feed.gz"), inspects initial bytes or defaults to .txt.
    pub fn infer_decompressed_name(original_path: &Path) -> String {
        let file_name = original_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("feed");

        let lower = file_name.to_lowercase();
        if lower.ends_with(".xml.gz") || lower.ends_with(".xml.gzip") {
            return file_name[..file_name.len() - if lower.ends_with(".xml.gz") { 3 } else { 5 }].to_string();
        }
        if lower.ends_with(".json.gz") || lower.ends_with(".json.gzip") {
            return file_name[..file_name.len() - if lower.ends_with(".json.gz") { 3 } else { 5 }].to_string();
        }
        if lower.ends_with(".csv.gz") || lower.ends_with(".csv.gzip") {
            return file_name[..file_name.len() - if lower.ends_with(".csv.gz") { 3 } else { 5 }].to_string();
        }
        if lower.ends_with(".tsv.gz") || lower.ends_with(".tsv.gzip") {
            return file_name[..file_name.len() - if lower.ends_with(".tsv.gz") { 3 } else { 5 }].to_string();
        }
        if lower.ends_with(".gz") {
            let stripped = &file_name[..file_name.len() - 3];
            if stripped.contains('.') {
                return stripped.to_string();
            }
            return format!("{}.xml", stripped);
        }
        if lower.ends_with(".gzip") {
            let stripped = &file_name[..file_name.len() - 5];
            if stripped.contains('.') {
                return stripped.to_string();
            }
            return format!("{}.xml", stripped);
        }

        format!("{}.uncompressed", file_name)
    }

    /// Decompresses a local .gz file into the temp cache directory in the background.
    pub fn start_decompress_gzip(
        src_path: &Path,
        cancel: Arc<AtomicBool>,
        progress_tx: Sender<DecompressStatus>,
    ) {
        let src = src_path.to_path_buf();
        let target_name = Self::infer_decompressed_name(&src);
        let out_dir = Self::get_temp_decompressed_dir();
        let out_path = out_dir.join(format!("{}_{}", std::process::id(), target_name));

        std::thread::Builder::new()
            .name("gzip-decompressor".to_string())
            .spawn(move || {
                let in_file = match File::open(&src) {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = progress_tx.send(DecompressStatus::Error(format!("Cannot open .gz file: {}", e)));
                        return;
                    }
                };

                let gz = GzDecoder::new(BufReader::with_capacity(BUFFER_SIZE, in_file));
                let mut reader = BufReader::with_capacity(BUFFER_SIZE, gz);

                let out_file = match File::create(&out_path) {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = progress_tx.send(DecompressStatus::Error(format!("Cannot create decompressed file: {}", e)));
                        return;
                    }
                };
                let mut writer = BufWriter::with_capacity(BUFFER_SIZE, out_file);

                let mut buffer = [0u8; BUFFER_SIZE];
                let mut total_bytes: u64 = 0;
                let mut last_progress = Instant::now();
                let display_name = src.file_name().and_then(|f| f.to_str()).unwrap_or("feed.gz").to_string();

                loop {
                    if cancel.load(Ordering::Relaxed) {
                        drop(writer);
                        let _ = fs::remove_file(&out_path);
                        let _ = progress_tx.send(DecompressStatus::Cancelled);
                        return;
                    }

                    match reader.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Err(e) = writer.write_all(&buffer[..n]) {
                                drop(writer);
                                let _ = fs::remove_file(&out_path);
                                let _ = progress_tx.send(DecompressStatus::Error(format!("Write error: {}", e)));
                                return;
                            }
                            total_bytes += n as u64;

                            if last_progress.elapsed().as_millis() >= PROGRESS_INTERVAL_MS {
                                let _ = progress_tx.send(DecompressStatus::Decompressing {
                                    bytes_decompressed: total_bytes,
                                    file_name: display_name.clone(),
                                });
                                last_progress = Instant::now();
                            }
                        }
                        Err(e) => {
                            drop(writer);
                            let _ = fs::remove_file(&out_path);
                            let _ = progress_tx.send(DecompressStatus::Error(format!("Corrupt gzip archive: {}", e)));
                            return;
                        }
                    }
                }

                if let Err(e) = writer.flush() {
                    drop(writer);
                    let _ = fs::remove_file(&out_path);
                    let _ = progress_tx.send(DecompressStatus::Error(format!("Flush error: {}", e)));
                    return;
                }

                let _ = progress_tx.send(DecompressStatus::Completed {
                    output_path: out_path,
                    total_bytes,
                    original_archive: src,
                    display_name: target_name,
                });
            })
            .expect("Failed to spawn gzip decompressor thread");
    }

    /// Inspects a .zip archive's Central Directory (runs in < 5ms).
    pub fn inspect_zip_archive(zip_path: &Path) -> Result<Vec<ZipEntryInfo>, String> {
        let file = File::open(zip_path).map_err(|e| format!("Cannot open zip archive: {}", e))?;
        let mut archive = ZipArchive::new(file).map_err(|e| format!("Invalid zip archive: {}", e))?;

        let mut entries = Vec::new();
        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index(i) {
                let name = entry.name().to_string();
                let is_dir = entry.is_dir();
                let uncompressed_size = entry.size();
                let compressed_size = entry.compressed_size();

                let file_type = if !is_dir {
                    let lower = name.to_lowercase();
                    if lower.ends_with(".xml") {
                        Some(FileType::Xml)
                    } else if lower.ends_with(".json") {
                        Some(FileType::Json)
                    } else if lower.ends_with(".csv") || lower.ends_with(".tsv") {
                        Some(FileType::Csv)
                    } else {
                        Some(FileType::PlainText)
                    }
                } else {
                    None
                };

                entries.push(ZipEntryInfo {
                    index: i,
                    name,
                    uncompressed_size,
                    compressed_size,
                    is_dir,
                    file_type,
                });
            }
        }

        Ok(entries)
    }

    /// Extracts a specific entry from a .zip archive into the temp cache directory in the background.
    pub fn start_extract_zip_entry(
        zip_path: &Path,
        entry_index: usize,
        cancel: Arc<AtomicBool>,
        progress_tx: Sender<DecompressStatus>,
    ) {
        let zip_src = zip_path.to_path_buf();
        let out_dir = Self::get_temp_decompressed_dir();

        std::thread::Builder::new()
            .name("zip-extractor".to_string())
            .spawn(move || {
                let file = match File::open(&zip_src) {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = progress_tx.send(DecompressStatus::Error(format!("Cannot open zip file: {}", e)));
                        return;
                    }
                };

                let mut archive = match ZipArchive::new(file) {
                    Ok(a) => a,
                    Err(e) => {
                        let _ = progress_tx.send(DecompressStatus::Error(format!("Corrupt zip archive: {}", e)));
                        return;
                    }
                };

                let mut entry = match archive.by_index(entry_index) {
                    Ok(e) => e,
                    Err(e) => {
                        let _ = progress_tx.send(DecompressStatus::Error(format!("Entry index out of range: {}", e)));
                        return;
                    }
                };

                let raw_name = entry.name().to_string();
                let safe_name = Path::new(&raw_name)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("extracted_file");

                let out_path = out_dir.join(format!("{}_{}_{}", std::process::id(), entry_index, safe_name));

                let out_file = match File::create(&out_path) {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = progress_tx.send(DecompressStatus::Error(format!("Cannot create output file: {}", e)));
                        return;
                    }
                };
                let mut writer = BufWriter::with_capacity(BUFFER_SIZE, out_file);

                let mut buffer = [0u8; BUFFER_SIZE];
                let mut total_bytes: u64 = 0;
                let mut last_progress = Instant::now();

                loop {
                    if cancel.load(Ordering::Relaxed) {
                        drop(writer);
                        let _ = fs::remove_file(&out_path);
                        let _ = progress_tx.send(DecompressStatus::Cancelled);
                        return;
                    }

                    match entry.read(&mut buffer) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Err(e) = writer.write_all(&buffer[..n]) {
                                drop(writer);
                                let _ = fs::remove_file(&out_path);
                                let _ = progress_tx.send(DecompressStatus::Error(format!("Write error: {}", e)));
                                return;
                            }
                            total_bytes += n as u64;

                            if last_progress.elapsed().as_millis() >= PROGRESS_INTERVAL_MS {
                                let _ = progress_tx.send(DecompressStatus::Decompressing {
                                    bytes_decompressed: total_bytes,
                                    file_name: safe_name.to_string(),
                                });
                                last_progress = Instant::now();
                            }
                        }
                        Err(e) => {
                            drop(writer);
                            let _ = fs::remove_file(&out_path);
                            let _ = progress_tx.send(DecompressStatus::Error(format!("Failed to decompress entry: {}", e)));
                            return;
                        }
                    }
                }

                if let Err(e) = writer.flush() {
                    drop(writer);
                    let _ = fs::remove_file(&out_path);
                    let _ = progress_tx.send(DecompressStatus::Error(format!("Flush error: {}", e)));
                    return;
                }

                let _ = progress_tx.send(DecompressStatus::Completed {
                    output_path: out_path,
                    total_bytes,
                    original_archive: zip_src,
                    display_name: safe_name.to_string(),
                });
            })
            .expect("Failed to spawn zip extractor thread");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn test_gzip_detection_and_decompression() {
        let temp_dir = std::env::temp_dir().join("ultraviewer_test_compressed");
        let _ = fs::create_dir_all(&temp_dir);

        let gz_path = temp_dir.join("feed_sample.xml.gz");
        let content = b"<catalog><item id=\"1\">Sample Product</item></catalog>";

        // Create test .gz
        {
            let file = File::create(&gz_path).unwrap();
            let mut encoder = GzEncoder::new(file, Compression::default());
            encoder.write_all(content).unwrap();
            encoder.finish().unwrap();
        }

        // Test detection
        assert_eq!(CompressedEngine::detect_compression(&gz_path), CompressionType::Gzip);

        // Test infer filename
        assert_eq!(CompressedEngine::infer_decompressed_name(&gz_path), "feed_sample.xml");

        // Test decompression
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();
        CompressedEngine::start_decompress_gzip(&gz_path, cancel, tx);

        let mut completed_path = None;
        while let Ok(msg) = rx.recv() {
            if let DecompressStatus::Completed { output_path, total_bytes, .. } = msg {
                assert_eq!(total_bytes, content.len() as u64);
                let decompressed_data = fs::read(&output_path).unwrap();
                assert_eq!(decompressed_data, content);
                completed_path = Some(output_path);
                break;
            }
        }

        if let Some(p) = completed_path {
            let _ = fs::remove_file(p);
        }
        let _ = fs::remove_file(&gz_path);
    }

    #[test]
    fn test_zip_detection_inspection_and_extraction() {
        let temp_dir = std::env::temp_dir().join("ultraviewer_test_compressed_zip");
        let _ = fs::create_dir_all(&temp_dir);

        let zip_path = temp_dir.join("feeds_archive.zip");
        let xml_data = b"<feed><title>Feed 1</title></feed>";
        let json_data = b"{\"name\": \"Feed 2\"}";

        // Create test .zip
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options = SimpleFileOptions::default();

            zip.start_file("data/feed1.xml", options).unwrap();
            zip.write_all(xml_data).unwrap();

            zip.start_file("data/feed2.json", options).unwrap();
            zip.write_all(json_data).unwrap();

            zip.finish().unwrap();
        }

        // Test detection
        assert_eq!(CompressedEngine::detect_compression(&zip_path), CompressionType::Zip);

        // Test inspection
        let entries = CompressedEngine::inspect_zip_archive(&zip_path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "data/feed1.xml");
        assert_eq!(entries[0].file_type, Some(FileType::Xml));
        assert_eq!(entries[0].uncompressed_size, xml_data.len() as u64);

        assert_eq!(entries[1].name, "data/feed2.json");
        assert_eq!(entries[1].file_type, Some(FileType::Json));
        assert_eq!(entries[1].uncompressed_size, json_data.len() as u64);

        // Test extraction
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();
        CompressedEngine::start_extract_zip_entry(&zip_path, 0, cancel, tx);

        let mut completed_path = None;
        while let Ok(msg) = rx.recv() {
            if let DecompressStatus::Completed { output_path, total_bytes, .. } = msg {
                assert_eq!(total_bytes, xml_data.len() as u64);
                let extracted = fs::read(&output_path).unwrap();
                assert_eq!(extracted, xml_data);
                completed_path = Some(output_path);
                break;
            }
        }

        if let Some(p) = completed_path {
            let _ = fs::remove_file(p);
        }
        let _ = fs::remove_file(&zip_path);
    }
}
