use std::fs::File;
use std::io::{self, Error, ErrorKind};
use std::path::{Path, PathBuf};
use super::encoding::Encoding;
use super::mmap::MmapHandle;

pub struct FileEngine {
    path: PathBuf,
    size: u64,
    mmap: MmapHandle,
    encoding: Encoding,
}

impl FileEngine {
    /// Opens a file from disk using memory mapping.
    /// Does not load the entire file into memory.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let file = File::open(&path_buf)?;
        let metadata = file.metadata()?;
        let size = metadata.len();

        let mmap = MmapHandle::open(&file, size)?;

        // Inspect header bytes (up to 64KB) for fast encoding detection
        let header_len = (size.min(65536)) as usize;
        let header_slice = if header_len > 0 {
            &mmap.as_slice()[..header_len]
        } else {
            &[]
        };
        let encoding = Encoding::detect(header_slice);

        Ok(Self {
            path: path_buf,
            size,
            mmap,
            encoding,
        })
    }

    /// Creates an empty unmapped FileEngine instance.
    pub fn empty() -> Self {
        Self {
            path: PathBuf::new(),
            size: 0,
            mmap: MmapHandle::Empty,
            encoding: Encoding::Utf8,
        }
    }

    /// Closes the file and unmaps the memory.
    pub fn close(&mut self) {
        self.mmap = MmapHandle::Empty;
        self.size = 0;
        self.path = PathBuf::new();
    }

    /// Returns the file size in bytes.
    #[inline]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Returns the path to the opened file.
    #[inline]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the detected encoding.
    #[inline]
    pub fn detect_encoding(&self) -> Encoding {
        self.encoding
    }

    /// Reads a sub-range [offset..offset+len] as a zero-copy byte slice.
    pub fn read_range(&self, offset: u64, len: usize) -> io::Result<&[u8]> {
        let full = self.mmap.as_slice();
        let start = offset as usize;
        if start > full.len() {
            return Err(Error::new(ErrorKind::UnexpectedEof, "Offset beyond file boundary"));
        }
        let end = (start + len).min(full.len());
        Ok(&full[start..end])
    }

    /// Returns a single byte at the given offset.
    #[inline]
    pub fn get_byte(&self, offset: u64) -> Option<u8> {
        let full = self.mmap.as_slice();
        full.get(offset as usize).copied()
    }

    /// Returns the full memory-mapped slice.
    #[inline]
    pub fn get_slice(&self) -> &[u8] {
        self.mmap.as_slice()
    }

    /// Returns true if the file is 0 bytes.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}
