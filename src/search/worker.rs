use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use crate::file_engine::{FileEngine, LineIndex};
use super::regex_search::RegexSearcher;
use super::text_search::LiteralSearcher;
use super::types::{SearchQuery, SearchResultMatch, SearchStatus};

const SEARCH_CHUNK_SIZE: usize = 1024 * 1024; // 1 MB streaming chunks
const OVERLAP_SIZE: usize = 512;               // Overlap across chunk borders
const MAX_STORED_MATCHES: usize = 5_000_000;   // Store up to 5,000,000 navigable matches in memory
const MAX_SNIPPET_MATCHES: usize = 5_000;      // Extract text snippets for first 5,000 matches in results table

pub struct SearchWorker;

impl SearchWorker {
    pub fn spawn(
        engine: Arc<FileEngine>,
        _line_index: Arc<LineIndex>,
        query: SearchQuery,
        matches_out: Arc<RwLock<Vec<SearchResultMatch>>>,
        status_out: Arc<RwLock<SearchStatus>>,
        cancel: Arc<AtomicBool>,
    ) -> JoinHandle<()> {
        thread::Builder::new()
            .name("search-worker".to_string())
            .spawn(move || {
                let file_size = engine.size();
                if file_size == 0 || query.is_empty() {
                    *status_out.write().unwrap() = SearchStatus::Completed {
                        matches_found: 0,
                        elapsed_secs: 0.0,
                    };
                    return;
                }

                let start_time = Instant::now();
                let mut last_progress_time = Instant::now();
                let mut matches_count = 0;

                let mut file = match File::open(engine.path()) {
                    Ok(f) => f,
                    Err(e) => {
                        *status_out.write().unwrap() = SearchStatus::Error(format!("Cannot open file: {}", e));
                        return;
                    }
                };

                let regex_searcher = if query.is_regex || !query.case_sensitive || query.whole_word {
                    match RegexSearcher::new(&query) {
                        Ok(s) => Some(s),
                        Err(e) => {
                            *status_out.write().unwrap() = SearchStatus::Error(format!("Invalid regex: {}", e));
                            return;
                        }
                    }
                } else {
                    None
                };

                let pattern_bytes = query.pattern.as_bytes();
                let literal_searcher = if regex_searcher.is_none() {
                    Some(LiteralSearcher::new(pattern_bytes, query.whole_word))
                } else {
                    None
                };

                let mut current_offset: u64 = 0;
                let mut buffer = vec![0u8; SEARCH_CHUNK_SIZE + OVERLAP_SIZE];
                let mut last_match_end: u64 = 0;
                let mut current_line: usize = 1;

                while current_offset < file_size {
                    if cancel.load(Ordering::Relaxed) {
                        *status_out.write().unwrap() = SearchStatus::Cancelled {
                            matches_found: matches_count,
                        };
                        return;
                    }

                    let read_target = SEARCH_CHUNK_SIZE + OVERLAP_SIZE;
                    let remaining_file = (file_size - current_offset) as usize;
                    let to_read = read_target.min(remaining_file);

                    let _ = file.seek(SeekFrom::Start(current_offset));
                    let bytes_read = match file.read(&mut buffer[..to_read]) {
                        Ok(0) => break,
                        Ok(n) => n,
                        Err(_) => break,
                    };

                    let active_slice = &buffer[..bytes_read];

                    let advance = if bytes_read > OVERLAP_SIZE && current_offset + (bytes_read as u64) < file_size {
                        bytes_read - OVERLAP_SIZE
                    } else {
                        bytes_read
                    };

                    // Find matches within active slice
                    let raw_matches: Vec<(usize, usize)> = if let Some(ref r) = regex_searcher {
                        r.find_matches(active_slice)
                    } else if let Some(ref lit) = literal_searcher {
                        lit.find_matches(active_slice)
                    } else {
                        Vec::new()
                    };

                    let mut chunk_items = Vec::new();
                    let mut line_cursor = 0;
                    for (start_in_chunk, match_len) in raw_matches {
                        // Ignore matches that fall beyond the non-overlapping window unless at EOF
                        if start_in_chunk >= advance && current_offset + (bytes_read as u64) < file_size {
                            continue;
                        }

                        let abs_start = current_offset + (start_in_chunk as u64);
                        let abs_end = abs_start + (match_len as u64);

                        if abs_start < last_match_end {
                            continue;
                        }
                        last_match_end = abs_end;
                        matches_count += 1;

                        if start_in_chunk > line_cursor {
                            current_line += memchr::memchr_iter(b'\n', &active_slice[line_cursor..start_in_chunk]).count();
                            line_cursor = start_in_chunk;
                        }
                        let line_number = current_line;

                        if matches_count <= MAX_STORED_MATCHES {
                            let snippet = if matches_count <= MAX_SNIPPET_MATCHES {
                                Self::extract_snippet(active_slice, start_in_chunk, match_len)
                            } else {
                                String::new()
                            };

                            chunk_items.push(SearchResultMatch {
                                line_number,
                                byte_offset: abs_start,
                                match_length: match_len,
                                snippet,
                            });
                        }
                    }

                    if !chunk_items.is_empty() {
                        let mut list = matches_out.write().unwrap();
                        list.extend(chunk_items);
                    }

                    if advance > line_cursor {
                        current_line += memchr::memchr_iter(b'\n', &active_slice[line_cursor..advance]).count();
                    }

                    current_offset += advance as u64;

                    if last_progress_time.elapsed().as_millis() >= 250 {
                        let elapsed = start_time.elapsed().as_secs_f64();
                        let speed = if elapsed > 0.0 {
                            ((current_offset as f64) / (1024.0 * 1024.0) / elapsed) as u64
                        } else {
                            0
                        };
                        let pct = ((current_offset as f64 / file_size as f64) * 100.0).min(100.0) as f32;

                        *status_out.write().unwrap() = SearchStatus::Searching {
                            progress_pct: pct,
                            matches_found: matches_count,
                            speed_mb_s: speed,
                        };
                        last_progress_time = Instant::now();
                    }
                }

                let elapsed_secs = start_time.elapsed().as_secs_f64();
                *status_out.write().unwrap() = SearchStatus::Completed {
                    matches_found: matches_count,
                    elapsed_secs,
                };
            })
            .expect("Failed to spawn search worker thread")
    }

    fn extract_snippet(slice: &[u8], match_start: usize, match_len: usize) -> String {
        let prefix_start = match_start.saturating_sub(40);
        let mut line_start = prefix_start;
        for i in (prefix_start..match_start).rev() {
            if slice[i] == b'\n' {
                line_start = i + 1;
                break;
            }
        }

        let match_end = match_start + match_len;
        let suffix_end = (match_end + 60).min(slice.len());
        let mut line_end = suffix_end;
        for i in match_end..suffix_end {
            if slice[i] == b'\n' || slice[i] == b'\r' {
                line_end = i;
                break;
            }
        }

        let raw = &slice[line_start..line_end];
        let mut text = String::from_utf8_lossy(raw).trim().to_string();
        if text.len() > 120 {
            text.truncate(120);
            text.push_str("...");
        }
        text
    }
}
