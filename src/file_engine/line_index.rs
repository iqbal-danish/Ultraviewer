use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::RwLock;
use crate::file_engine::{Encoding, FileEngine};

pub const CHECKPOINT_INTERVAL: usize = 64;

pub struct LineIndex {
    file_size: u64,
    encoding: Encoding,
    checkpoints: RwLock<Vec<u64>>,
    total_lines: AtomicUsize,
    indexed_bytes: AtomicU64,
    is_complete: AtomicBool,
    indexing_speed_mb: AtomicU64,
}

impl LineIndex {
    pub fn new(file_size: u64, encoding: Encoding) -> Self {
        let bom_len = encoding.bom_length() as u64;
        let initial_checkpoints = if file_size > 0 {
            vec![bom_len]
        } else {
            Vec::new()
        };

        let initial_lines = if file_size > 0 { 1 } else { 0 };

        Self {
            file_size,
            encoding,
            checkpoints: RwLock::new(initial_checkpoints),
            total_lines: AtomicUsize::new(initial_lines),
            indexed_bytes: AtomicU64::new(0),
            is_complete: AtomicBool::new(file_size == 0),
            indexing_speed_mb: AtomicU64::new(0),
        }
    }

    #[inline]
    pub fn total_lines(&self) -> usize {
        self.total_lines.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn indexed_bytes(&self) -> u64 {
        self.indexed_bytes.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn is_complete(&self) -> bool {
        self.is_complete.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn speed_mb_s(&self) -> u64 {
        self.indexing_speed_mb.load(Ordering::Relaxed)
    }

    pub fn progress_pct(&self) -> f32 {
        if self.file_size == 0 {
            return 100.0;
        }
        let indexed = self.indexed_bytes() as f64;
        let total = self.file_size as f64;
        ((indexed / total) * 100.0).min(100.0) as f32
    }

    /// Add checkpoints and update line counts in batch.
    pub fn add_checkpoints_batch(&self, new_checkpoints: &[u64], added_lines: usize, bytes_processed: u64) {
        if !new_checkpoints.is_empty() {
            let mut cps = self.checkpoints.write().unwrap();
            cps.extend_from_slice(new_checkpoints);
        }
        self.total_lines.fetch_add(added_lines, Ordering::Release);
        self.indexed_bytes.store(bytes_processed, Ordering::Release);
    }

    pub fn set_speed(&self, mb_per_sec: u64) {
        self.indexing_speed_mb.store(mb_per_sec, Ordering::Relaxed);
    }

    pub fn mark_complete(&self, total_final_lines: usize) {
        self.total_lines.store(total_final_lines, Ordering::Release);
        self.indexed_bytes.store(self.file_size, Ordering::Release);
        self.is_complete.store(true, Ordering::Release);
    }

    /// Map 1-based line number to its exact byte offset.
    pub fn line_to_byte_offset(&self, engine: &FileEngine, target_line: usize) -> Option<u64> {
        if target_line == 0 || self.file_size == 0 {
            return None;
        }
        if target_line == 1 {
            return Some(self.encoding.bom_length() as u64);
        }

        let cp_idx = (target_line - 1) / CHECKPOINT_INTERVAL;
        let remaining_lines = (target_line - 1) % CHECKPOINT_INTERVAL;

        let start_offset = {
            let cps = self.checkpoints.read().unwrap();
            if cp_idx >= cps.len() {
                return None;
            }
            cps[cp_idx]
        };

        if remaining_lines == 0 {
            return Some(start_offset);
        }

        // Forward scan using memchr for remaining lines
        let scan_len = (32768).min(self.file_size.saturating_sub(start_offset) as usize);
        if scan_len == 0 {
            return Some(start_offset);
        }

        let slice = engine.read_range(start_offset, scan_len).ok()?;
        let mut count = 0;
        for pos in memchr::memchr_iter(b'\n', slice) {
            count += 1;
            if count == remaining_lines {
                let line_byte = start_offset + (pos as u64) + 1;
                return Some(line_byte.min(self.file_size));
            }
        }

        // If not found in first 32KB chunk, read larger chunk
        let larger_scan = (262144).min(self.file_size.saturating_sub(start_offset) as usize);
        let slice = engine.read_range(start_offset, larger_scan).ok()?;
        let mut count = 0;
        for pos in memchr::memchr_iter(b'\n', slice) {
            count += 1;
            if count == remaining_lines {
                let line_byte = start_offset + (pos as u64) + 1;
                return Some(line_byte.min(self.file_size));
            }
        }

        None
    }

    /// Map arbitrary byte offset to 1-based line number.
    pub fn byte_offset_to_line(&self, engine: &FileEngine, target_offset: u64) -> usize {
        if self.file_size == 0 || target_offset == 0 {
            return 1;
        }
        let clamped_offset = target_offset.min(self.file_size);

        let (cp_idx, base_offset) = {
            let cps = self.checkpoints.read().unwrap();
            match cps.binary_search(&clamped_offset) {
                Ok(exact_idx) => (exact_idx, cps[exact_idx]),
                Err(insert_idx) => {
                    if insert_idx == 0 {
                        (0, self.encoding.bom_length() as u64)
                    } else {
                        (insert_idx - 1, cps[insert_idx - 1])
                    }
                }
            }
        };

        let base_line = cp_idx * CHECKPOINT_INTERVAL + 1;
        if clamped_offset <= base_offset {
            return base_line;
        }

        let delta_len = (clamped_offset - base_offset) as usize;
        let slice = match engine.read_range(base_offset, delta_len) {
            Ok(s) => s,
            Err(_) => return base_line,
        };

        let additional_lines = memchr::memchr_iter(b'\n', slice).count();
        base_line + additional_lines
    }
}
