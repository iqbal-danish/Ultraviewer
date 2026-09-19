use memchr::memmem::Finder;

pub fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub fn check_word_boundary(slice: &[u8], match_start: usize, match_end: usize) -> bool {
    if match_start > 0 {
        let prev = slice[match_start - 1];
        if is_word_char(prev) {
            return false;
        }
    }
    if match_end < slice.len() {
        let next = slice[match_end];
        if is_word_char(next) {
            return false;
        }
    }
    true
}

pub struct LiteralSearcher<'a> {
    finder: Finder<'a>,
    whole_word: bool,
    pattern_len: usize,
}

impl<'a> LiteralSearcher<'a> {
    pub fn new(pattern: &'a [u8], whole_word: bool) -> Self {
        Self {
            finder: Finder::new(pattern),
            whole_word,
            pattern_len: pattern.len(),
        }
    }

    /// Finds all match start indices in the provided slice.
    pub fn find_matches(&self, slice: &[u8]) -> Vec<(usize, usize)> {
        let mut results = Vec::new();
        for pos in self.finder.find_iter(slice) {
            let match_end = pos + self.pattern_len;
            if self.whole_word && !check_word_boundary(slice, pos, match_end) {
                continue;
            }
            results.push((pos, self.pattern_len));
        }
        results
    }
}
