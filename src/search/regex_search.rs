use regex::bytes::{Regex, RegexBuilder};
use super::types::SearchQuery;

pub fn compile_regex(query: &SearchQuery) -> Result<Regex, regex::Error> {
    let raw_pattern = if query.is_regex {
        if query.whole_word {
            format!(r"\b(?:{})\b", query.pattern)
        } else {
            query.pattern.clone()
        }
    } else {
        let escaped = regex::escape(&query.pattern);
        if query.whole_word {
            format!(r"\b(?:{})\b", escaped)
        } else {
            escaped
        }
    };

    RegexBuilder::new(&raw_pattern)
        .case_insensitive(!query.case_sensitive)
        .build()
}

pub struct RegexSearcher {
    regex: Regex,
}

impl RegexSearcher {
    pub fn new(query: &SearchQuery) -> Result<Self, regex::Error> {
        let regex = compile_regex(query)?;
        Ok(Self { regex })
    }

    pub fn find_matches(&self, slice: &[u8]) -> Vec<(usize, usize)> {
        self.regex
            .find_iter(slice)
            .map(|m| (m.start(), m.len()))
            .collect()
    }
}
