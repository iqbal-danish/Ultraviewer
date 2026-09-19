pub mod detector;
pub mod formatter;
pub mod json;
pub mod xml;

pub use detector::{FileType, FormatDetector};
pub use formatter::{FormatAction, FormattingProgress, JsonStreamingFormatter, XmlStreamingFormatter};
pub use json::{JsonHighlightSpan, JsonStructureIndexer, JsonSyntaxHighlighter, JsonTreeNode, JsonValidationResult, JsonValidator};
pub use xml::{XmlHighlightSpan, XmlStructureIndexer, XmlSyntaxHighlighter, XmlTreeNode, XmlValidationResult, XmlValidator};

