use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

use crate::file_engine::FileEngine;
use crate::formats::formatter::FormattingProgress;
use super::document::EditorDocument;

pub struct SaveManager;

impl SaveManager {
    const WRITE_BUFFER_SIZE: usize = 256 * 1024; // 256 KB buffered writes

    /// Stream changes to a temporary file next to the original file for crash-safe atomic saving
    pub fn save_in_place(
        document: &EditorDocument,
        engine: &FileEngine,
        cancel: Arc<AtomicBool>,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<PathBuf, String> {
        let src_path = engine.path();
        let tmp_path = match src_path.file_name().and_then(|n| n.to_str()) {
            Some(name) => src_path.with_file_name(format!("{}.tmp_save", name)),
            None => src_path.with_extension("tmp_save"),
        };

        let file = File::create(&tmp_path)
            .map_err(|e| format!("Failed to create temporary save file {:?}: {}", tmp_path, e))?;
        let mut writer = BufWriter::with_capacity(Self::WRITE_BUFFER_SIZE, file);

        document.piece_table.stream_write(engine, &mut writer, cancel, progress)?;

        let file = writer.into_inner().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| format!("Failed to sync temporary save file: {}", e))?;

        Ok(tmp_path)
    }

    /// Stream changes directly to a new target file
    pub fn save_as<P: AsRef<Path>>(
        document: &EditorDocument,
        engine: &FileEngine,
        target_path: P,
        cancel: Arc<AtomicBool>,
        progress: Option<Arc<RwLock<FormattingProgress>>>,
    ) -> Result<(), String> {
        let path = target_path.as_ref();
        let file = File::create(path)
            .map_err(|e| format!("Failed to create save-as file {:?}: {}", path, e))?;
        let mut writer = BufWriter::with_capacity(Self::WRITE_BUFFER_SIZE, file);

        document.piece_table.stream_write(engine, &mut writer, cancel, progress)?;

        let file = writer.into_inner().map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| format!("Failed to sync save-as file: {}", e))?;

        Ok(())
    }
}
