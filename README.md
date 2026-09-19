# UltraViewer

> **UltraViewer** is designed for extremely large files using memory-efficient, virtualized, background-processing architecture.

Inspired by high-performance large-file editors such as EmEditor, UltraViewer is built from the ground up in Rust to open, view, search, analyze, format, and edit multi-gigabyte files (architectural target: 50 GB+) on Windows 10/11 64-bit without freezing the UI or loading entire files into RAM.

---

## Core Philosophy: *Open First. Process Later.*

When opening a huge file:
1. **Instant Opening**: Open the file handle immediately.
2. **Zero-Copy Memory Mapping**: Leverage OS virtual memory mapping (`memmap2`) instead of reading whole files into RAM.
3. **Cheap Detection**: Detect file size, encoding (UTF-8, UTF-8 BOM, UTF-16 LE/BE), and file type (XML, JSON, CSV, text) using initial byte headers without full file scans.
4. **Virtualized Viewport**: Display only the lines currently visible in the active viewport window.
5. **Bounded Memory**: Memory footprint remains small (< 50 MB) whether opening a 10 MB, 1 GB, 10 GB, or 50 GB file.

---

## Phase 1 Status & Features

UltraViewer is currently at **v0.1.0 (Phase 1: Foundation)**:

- [x] High-performance memory-mapped file engine (`FileEngine`) with safe 0-byte file handling.
- [x] Instant file opening & size detection.
- [x] Content-first encoding detection (UTF-8, UTF-8 BOM, UTF-16 LE, UTF-16 BE).
- [x] Content-first file type detection (XML, JSON, CSV, Plain Text, Binary).
- [x] Virtualized viewport rendering first visible chunk with line numbers.
- [x] Native Windows file picker (`rfd`) and drag-and-drop file loading.
- [x] Viewport navigation (First, Previous Chunk, Next Chunk, Last, Jump to Byte Offset).
- [x] Live status bar displaying real-time metrics: file size, encoding, type, visible lines, open latency, and process RSS working-set memory.
- [x] Streaming synthetic file generator CLI (`ultraviewer-gen`) for generating 100 MB – 50 GB XML/JSON job-feed test files directly to disk without memory overhead.
- [x] Unit and integration test suite.

---

## Building and Running

### Prerequisites
* Rust 1.80+ (with Cargo)
* Windows 10/11 64-bit (or Linux / macOS)

### Build UltraViewer GUI
```bash
cargo build --release
```

### Run UltraViewer
```bash
# Launch the graphical application
cargo run --release

# Or launch directly with a file path:
cargo run --release -- path\to\largefile.xml
```

### Run Tests
```bash
cargo test
```

---

## Synthetic Test File Generator

UltraViewer includes a dedicated CLI utility (`ultraviewer-gen`) that generates realistic, multi-gigabyte synthetic XML and JSON datasets (structured as job feeds) streaming directly to disk with minimal memory overhead (< 2 MB RAM).

### Usage
```bash
# Generate a 100 MB XML file
cargo run --release --bin ultraviewer-gen -- --format xml --size 100MB --out test_100mb.xml

# Generate a 1 GB JSON file
cargo run --release --bin ultraviewer-gen -- --format json --size 1GB --out test_1gb.json

# Generate larger files (5 GB, 10 GB, 50 GB)
cargo run --release --bin ultraviewer-gen -- --format xml --size 10GB --out test_10gb.xml
```

---

## Benchmark & Performance Methodology

During testing, UltraViewer tracks actual performance metrics:
- **File Open Time**: Elapsed time from user selection until the initial viewport is rendered.
- **Process Memory (RSS)**: Working set memory consumed by the application, verified via OS process metrics.
- **Throughput**: For streaming generation, search, and indexing operations.

### Preliminary Results (Phase 1)
- **100 MB XML File**:
  - Open Latency: `< 2 ms`
  - Memory Usage (RSS): `~30-45 MB` (GUI runtime overhead; does not allocate 100 MB buffer)
- **Zero-Copy Guarantee**: Slices are mapped directly from the OS page cache; no `std::fs::read_to_string` is ever performed.

---

## Roadmap

* **Phase 1 [Complete]**: Foundation — Memory-mapped file engine, virtualized viewport, encoding & type detection, desktop UI.
* **Phase 2 [Complete]**: Large File Viewer — Background checkpoint line indexer, smooth virtual scrolling over millions of lines, cooperative cancellation.
* **Phase 3 [Complete]**: Search — Multi-threaded text/regex background search with streaming results panel.
* **Phase 4 [Complete]**: XML Intelligence — Streaming event parser, element highlighting, syntax tree, and validation.
* **Phase 5 [Complete]**: JSON Intelligence — Streaming token parser, structure tree, syntax highlighting, and validation.
* **Phase 6 [Complete]**: Streaming Formatter — Beautify & minify XML/JSON without loading entire files into RAM.
* **Phase 7**: Field Analyzer — Streaming high-cardinality value frequency and schema analysis.
* **Phase 8**: Editing Engine — Piece table / rope chunked document model with crash-safe atomic saves.
