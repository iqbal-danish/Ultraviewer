pub mod regex_search;
pub mod text_search;
pub mod types;
pub mod worker;

pub use regex_search::RegexSearcher;
pub use text_search::LiteralSearcher;
pub use types::{SearchQuery, SearchResultMatch, SearchStatus};
pub use worker::SearchWorker;
