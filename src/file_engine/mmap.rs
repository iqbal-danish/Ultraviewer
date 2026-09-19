use std::fs::File;
use std::io::Result;
use std::ops::Deref;
use memmap2::Mmap;

/// Wrapper around `memmap2::Mmap` that gracefully handles 0-byte files.
pub enum MmapHandle {
    Empty,
    Mapped(Mmap),
}

impl MmapHandle {
    /// Memory maps the given file. If size is 0, returns `MmapHandle::Empty`
    /// without calling the OS mmap syscall.
    pub fn open(file: &File, size: u64) -> Result<Self> {
        if size == 0 {
            return Ok(MmapHandle::Empty);
        }

        // SAFETY: We open the file read-only and assume the underlying file
        // is not concurrently truncated by another process.
        let mmap = unsafe {
            memmap2::MmapOptions::new().map(file)?
        };

        Ok(MmapHandle::Mapped(mmap))
    }

    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        match self {
            MmapHandle::Empty => &[],
            MmapHandle::Mapped(mmap) => mmap.deref(),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
