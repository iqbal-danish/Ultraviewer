use std::path::Path;
use crate::file_engine::Encoding;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    Xml,
    Json,
    Csv,
    PlainText,
    Binary,
}

impl FileType {
    pub fn name(&self) -> &'static str {
        match self {
            FileType::Xml => "XML",
            FileType::Json => "JSON",
            FileType::Csv => "CSV",
            FileType::PlainText => "Plain Text",
            FileType::Binary => "Binary",
        }
    }
}

pub struct FormatDetector;

impl FormatDetector {
    /// Detect the file type using both content inspection and extension hint.
    pub fn detect(header: &[u8], encoding: Encoding, path: Option<&Path>) -> FileType {
        if header.is_empty() {
            return FileType::PlainText;
        }

        let bom_len = encoding.bom_length();
        let payload = if header.len() > bom_len {
            &header[bom_len..]
        } else {
            header
        };

        // For UTF-16, decode a snippet into string
        let snippet = match encoding {
            Encoding::Utf16Le | Encoding::Utf16Be => encoding.decode_chunk(payload),
            _ => {
                // If contains null bytes in UTF-8 mode, likely binary
                let null_count = payload.iter().take(1024).filter(|&&b| b == 0).count();
                if null_count > 0 {
                    return FileType::Binary;
                }
                String::from_utf8_lossy(payload)
            }
        };

        let trimmed = snippet.trim_start();
        if trimmed.is_empty() {
            return FileType::PlainText;
        }

        // XML detection: starts with <?xml or <!DOCTYPE or <tag
        if trimmed.starts_with("<?xml")
            || trimmed.starts_with("<!DOCTYPE")
            || (trimmed.starts_with('<') && trimmed.chars().nth(1).map_or(false, |c| c.is_ascii_alphabetic() || c == '_' || c == '?' || c == '!'))
        {
            return FileType::Xml;
        }

        // JSON detection: starts with { or [
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            return FileType::Json;
        }

        // CSV detection: check for commas/delimiters with uniform counts across initial lines
        let lines: Vec<&str> = trimmed.lines().take(5).collect();
        if lines.len() >= 2 {
            let comma_count_first = lines[0].matches(',').count();
            if comma_count_first > 0 {
                let consistent = lines.iter().skip(1).all(|l| l.matches(',').count() == comma_count_first);
                if consistent {
                    return FileType::Csv;
                }
            }
        }

        // Check path extension hint as fallback
        if let Some(path) = path {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                match ext.to_ascii_lowercase().as_str() {
                    "xml" => return FileType::Xml,
                    "json" => return FileType::Json,
                    "csv" | "tsv" => return FileType::Csv,
                    "txt" | "log" | "md" | "rs" | "c" | "cpp" | "h" | "py" | "js" | "html" => {
                        return FileType::PlainText
                    }
                    _ => {}
                }
            }
        }

        FileType::PlainText
    }
}
