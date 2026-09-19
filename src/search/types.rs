#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    pub pattern: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub is_regex: bool,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            case_sensitive: false,
            whole_word: false,
            is_regex: false,
        }
    }
}

impl SearchQuery {
    pub fn is_empty(&self) -> bool {
        self.pattern.trim().is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct SearchResultMatch {
    pub line_number: usize,
    pub byte_offset: u64,
    pub match_length: usize,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SearchStatus {
    Idle,
    Searching {
        progress_pct: f32,
        matches_found: usize,
        speed_mb_s: u64,
    },
    Completed {
        matches_found: usize,
        elapsed_secs: f64,
    },
    Cancelled {
        matches_found: usize,
    },
    Error(String),
}
