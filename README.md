# UltraViewer 🚀

> **High-Performance Large-File Editor & Data Intelligence Studio for Windows**  
> Built in Rust to instantly open, stream-query, analyze, format, and edit multi-gigabyte XML, JSON, and CSV files (50 GB+) with < 50 MB RAM footprint.

[![Rust](https://img.shields.io/badge/rust-1.80%2B-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-Windows%2010%20%7C%2011%20(64--bit)-blue.svg)]()
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-green.svg)]()
[![Tests](https://img.shields.io/badge/tests-73%20passed-brightgreen.svg)]()

---

## ⚡ Why UltraViewer?

Traditional text editors and IDEs (VS Code, Notepad++, Sublime) either freeze, exhaust RAM, or refuse to open files larger than 1–2 GB. Feed processors and data engineers often work with **10 GB to 50 GB+** job feeds, catalog exports, database dumps, and server logs.

**UltraViewer** is built from the ground up in Rust around a single core philosophy: **"Open First. Process Later."**

- **Instant Opening (< 5 ms)**: Memory-maps files directly from the OS page cache via `memmap2`. Zero memory allocation for the full file.
- **Bounded Memory Footprint**: Operates comfortably under 50 MB RAM regardless of whether your file is 10 MB, 1 GB, 10 GB, or 50 GB.
- **Background Checkpoint Indexing**: Line counting and position indexing run asynchronously with smooth, cooperative cancellation.
- **Live Stream Querying**: Native streaming XPath 1.0 and JSONPath engines evaluate complex filters over gigabytes in seconds.
- **URL & Gzip Stream Downloader**: Download feeds directly from HTTP/HTTPS with on-the-fly streaming gzip decompression.
- **Piece Table In-Place Editing**: Type, delete, split, join, and find-and-replace with unlimited Undo/Redo and crash-safe atomic saves.

---

## ✨ Key Features

### 1. 🔍 Streaming XPath & JSONPath Query Engine
Evaluate targeted queries over multi-gigabyte datasets without memory explosion:
- **Empty / Missing Field Detection**: Identify data quality issues instantly.
  - XML: `//job[not(normalize-space(title))]` or `//job[not(normalize-space(description))]`
  - JSON: `$.jobs[?(!@.title)]` or `$.jobs[?(!@.description)]`
- **Numeric & Pricing Filters**:
  - XML: `//job[number(cpc) > 1.0]` or `//job[number(cpc) < 0.25]`
  - JSON: `$.jobs[?(@.cpc > 1.0)]` or `$.jobs[?(@.cpc < 0.25)]`
- **Geographic & State Quality Checks**:
  - XML: `//job[state != 'US' and country != 'US']`
  - JSON: `$.jobs[?(@.state != 'US' && @.country != 'US')]`
- **URL & Link Quality**:
  - XML: `//job[not(starts-with(url, 'https://'))]`
- **Incomplete Record Hunter**:
  - XML: `//job[not(normalize-space(title)) or not(normalize-space(url)) or not(normalize-space(description))]`
  - JSON: `$.jobs[?(!@.title || !@.url || !@.description)]`
- **Context-Aware Suggestions**: Click on any breadcrumb tag to see smart, auto-generated suggestions tailored specifically to that field and its parent feed container.

### 2. 🌐 URL Downloader & Streaming Gzip Engine
- Open any remote XML/JSON/CSV feed via `Ctrl+U` or the Open URL modal.
- Real-time progress metrics: downloaded bytes, transfer speed, elapsed time, and ETA.
- **On-The-Fly Gzip Decompression**: Automatically detects `.gz` / `.gzip` headers and stream-decompresses directly to the disk cache (`%APPDATA%\ultraviewer\downloads`) with low memory overhead.
- Instantly opens the decompressed feed in the editor when download finishes.

### 3. ✍️ In-Place Editing & Piece Table Architecture
- **VS Code-Grade Virtualized Viewport**: Direct cursor placement, single-click row editing, word selection, and multi-line selection.
- **Line Editing Operations**: Press `Enter` to split lines, `Backspace`/`Delete` to join lines.
- **Case Transformations**: Convert selection or active line to UPPERCASE (`Ctrl+Shift+U`) or lowercase (`Ctrl+Shift+L`).
- **Batch Find & Replace**: Case-sensitive, whole-word, and regular expression replacements across the entire file or selection.
- **Crash-Safe Atomic Saves**: Writes to a temp buffer and executes an OS atomic rename to guarantee file integrity.

### 4. 📊 Data Intelligence & Tabular Grid
- **Field & Schema Analyzer**: Scans feeds to compute field presence, missing percentages, value cardinality, and sample distributions.
- **Field Extractor**: Stream-extracts selected fields to tabular CSV files for reporting.
- **Interactive CSV / TSV Grid Viewer**: High-performance tabular viewer with column sorting, row virtualization, and delimiter detection.
- **Virtual Slice Viewer**: Inspect and export designated byte ranges from massive files without loading surrounding data.

### 5. 🎨 Modern Developer UI (egui)
- Clean dark theme with One Dark syntax highlighting.
- **Dynamic Gutter**: Auto-calculates line number column width with fixed chevrons that stay pinned on horizontal scrolls.
- **Overview Ruler Heatmap**: Visual bar on the right displaying search matches and active line indicators.
- **Fold / Unfold Code Blocks**: Fold nested XML tags and JSON objects to declutter massive hierarchies.
- **Full Line Inspector**: Pop out extremely long lines into a dedicated modal without slowing down viewport scrolling.

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
| :--- | :--- |
| `Ctrl + O` | Open Local File |
| `Ctrl + U` | Open URL / Download Remote Feed |
| `Ctrl + S` | Save File (Atomic) |
| `Ctrl + F` | Toggle Search Bar (Text / Regex) |
| `Ctrl + Q` | Toggle Query Bar (XPath / JSONPath) |
| `Ctrl + G` | Jump to Line Number |
| `Ctrl + Z` | Undo |
| `Ctrl + Y` | Redo |
| `Ctrl + Shift + U` | Transform Selection to UPPERCASE |
| `Ctrl + Shift + L` | Transform Selection to lowercase |
| `Alt + Z` | Toggle Word Wrap |
| `F3` / `Shift + F3` | Next / Previous Search Match |
| `Esc` | Close Active Modal / Bar |

---

## 🏗️ Architecture Overview

```
┌───────────────────────────────────────────────────────────┐
│                    UltraViewer UI (egui)                  │
│  Breadcrumb Bar │ Editor Viewport │ Search & Query Bars   │
└─────────────────────────────┬─────────────────────────────┘
                              │
       ┌──────────────────────┼──────────────────────┐
       ▼                      ▼                      ▼
┌──────────────┐      ┌──────────────┐      ┌─────────────────┐
│ Piece Table  │      │ Memory-Mapped│      │ Streaming Query │
│   Document   │      │ File Engine  │      │ & Index Engine  │
│ (Undo/Redo)  │      │  (memmap2)   │      │ (XPath/JSONPath)│
└──────────────┘      └──────────────┘      └─────────────────┘
       │                      │                      │
       └──────────────────────┼──────────────────────┘
                              ▼
           OS Virtual Memory / Disk File System
```

- **`FileEngine`**: Manages OS-level memory mapping with graceful zero-byte file handling and BOM/encoding resolution.
- **`PieceTable`**: Tracks document modifications in-memory as an append-only sequence of original file slices and added buffers.
- **`LineIndexer`**: Incrementally scans line feeds in background threads, reporting progress checkpoints without blocking the UI thread.
- **`QueryEngine`**: Streaming lexer/parser that executes XPath and JSONPath queries with short-circuit evaluation.

---

## 🚀 Getting Started

### Prerequisites
- **Rust 1.80+** (with Cargo)
- **Windows 10 / 11 64-bit** (Linux and macOS supported)

### Build from Source
```bash
# Clone the repository
git clone https://github.com/iqbal-danish/Ultraviewer.git
cd Ultraviewer

# Build optimized release binary
cargo build --release
```

The executable will be located at `target\release\ultraviewer.exe` (or `UltraViewer.exe` in the project root).

### Running UltraViewer
```bash
# Launch GUI
cargo run --release

# Or launch directly with a target file:
cargo run --release -- path\to\large_feed.xml
```

### Running Test Suite
```bash
cargo test
```
All 73 unit and integration tests verify query parsing, streaming decompression, piece table edits, formatting, and gutter layout.

---

## 🧪 Synthetic Dataset Generator (`ultraviewer-gen`)

UltraViewer includes a built-in high-speed CLI tool to generate multi-gigabyte synthetic XML and JSON datasets (modeled as realistic job feeds) directly to disk with minimal memory overhead (< 2 MB RAM):

```bash
# Generate a 100 MB XML feed
cargo run --release --bin ultraviewer-gen -- --format xml --size 100MB --out feed_100mb.xml

# Generate a 1 GB JSON feed
cargo run --release --bin ultraviewer-gen -- --format json --size 1GB --out feed_1gb.json

# Generate a 10 GB XML feed for stress testing
cargo run --release --bin ultraviewer-gen -- --format xml --size 10GB --out feed_10gb.xml
```

---

## 📈 Benchmark Highlights

| Metric | 100 MB File | 1 GB File | 10 GB File |
| :--- | :--- | :--- | :--- |
| **Open Latency** | < 2 ms | < 3 ms | < 5 ms |
| **RAM Usage (RSS)** | ~35 MB | ~42 MB | ~48 MB |
| **Line Indexing Speed**| ~1.2 GB/s | ~1.2 GB/s | ~1.2 GB/s |
| **XPath Evaluation** | Instant | Progressive Stream | Progressive Stream |
| **Zero-Copy Guarantee**| Direct OS mmap | Direct OS mmap | Direct OS mmap |

---

## 📄 License

This project is licensed under the MIT License or Apache-2.0 License at your option.
