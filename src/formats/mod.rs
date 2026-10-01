pub mod detector;
pub mod formatter;
pub mod jolt;
pub mod json;
pub mod path_resolver;
pub mod query_engine;
pub mod query_suggestions;
pub mod xml;
pub mod xslt;

pub use detector::{FileType, FormatDetector};
pub use formatter::{FormatAction, FormattingProgress, JsonStreamingFormatter, XmlStreamingFormatter};
pub use jolt::{JoltValidationResult, JoltValidator};
pub use json::{JsonHighlightSpan, JsonStructureIndexer, JsonSyntaxHighlighter, JsonTreeNode, JsonValidationResult, JsonValidator};
pub use path_resolver::PathResolver;
pub use query_engine::{ChildPredicate, JsonPathQuery, PredicateOp, QueryMatch, QueryProgress, StreamingQueryEngine, XPathQuery};
pub use query_suggestions::{DiscoveredTags, QuerySuggestion, SuggestionCategory, SuggestionKind};
pub use xml::{XmlHighlightSpan, XmlStructureIndexer, XmlSyntaxHighlighter, XmlTreeNode, XmlValidationResult, XmlValidator};
pub use xslt::{XsltValidationResult, XsltValidator};


