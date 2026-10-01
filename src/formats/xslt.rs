use std::io::{BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::file_engine::FileEngine;
use crate::file_engine::line_index::LineIndex;

#[derive(Debug, Clone, PartialEq)]
pub enum XsltValidationResult {
    Valid {
        elements_count: usize,
        templates_count: usize,
        version: String,
        elapsed_secs: f64,
    },
    Invalid {
        line_number: usize,
        byte_offset: u64,
        message: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct ChooseState {
    pub has_when: bool,
    pub has_otherwise: bool,
}

struct LineTrackingReader<R: Read> {
    inner: R,
    current_byte: u64,
    line_starts: Arc<Mutex<Vec<u64>>>,
}

impl<R: Read> LineTrackingReader<R> {
    fn new(inner: R, line_starts: Arc<Mutex<Vec<u64>>>) -> Self {
        {
            let mut ls = line_starts.lock().unwrap();
            ls.clear();
            ls.push(0); // Line 1 starts at byte 0
        }
        Self {
            inner,
            current_byte: 0,
            line_starts,
        }
    }
}

impl<R: Read> Read for LineTrackingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            let mut byte = self.current_byte;
            let mut new_starts = Vec::new();

            for &b in &buf[..n] {
                byte += 1;
                if b == b'\n' {
                    new_starts.push(byte);
                }
            }

            self.current_byte = byte;

            if !new_starts.is_empty() {
                if let Ok(mut ls) = self.line_starts.lock() {
                    ls.extend(new_starts);
                }
            }
        }
        Ok(n)
    }
}

pub struct XsltValidator;

impl XsltValidator {
    const VALID_TOP_LEVEL: [&'static str; 13] = [
        "template", "variable", "param", "output", "include", "import", "key",
        "attribute-set", "decimal-format", "namespace-alias", "strip-space", "preserve-space", "function",
    ];

    /// Quick heuristic to check if a document is an XSLT stylesheet.
    pub fn is_xslt(snippet: &str) -> bool {
        let s = snippet.to_ascii_lowercase();
        s.contains("<xsl:stylesheet")
            || s.contains("<xsl:transform")
            || s.contains("http://www.w3.org/1999/xsl/transform")
    }

    /// Stream-validates an XSLT document with semantic stylesheet rules, required attributes,
    /// nesting hierarchy, and XPath syntax checks.
    pub fn validate<R: Read>(
        reader: R,
        line_index: Option<&LineIndex>,
        engine: Option<&FileEngine>,
        cancel: Arc<AtomicBool>,
    ) -> XsltValidationResult {
        let start = Instant::now();
        let line_starts = Arc::new(Mutex::new(Vec::with_capacity(1024)));
        let tracking_reader = LineTrackingReader::new(reader, Arc::clone(&line_starts));
        let buf_reader = BufReader::with_capacity(512 * 1024, tracking_reader);
        let mut xml_reader = Reader::from_reader(buf_reader);
        xml_reader.config_mut().expand_empty_elements = false;
        xml_reader.config_mut().trim_text(false);
        xml_reader.config_mut().check_end_names = true;

        let get_line_num = |offset: u64| -> usize {
            if let Ok(ls) = line_starts.lock() {
                if !ls.is_empty() {
                    return match ls.binary_search(&offset) {
                        Ok(i) => i + 1,
                        Err(i) => i.max(1),
                    };
                }
            }
            if let (Some(idx), Some(eng)) = (line_index, engine) {
                if eng.size() > 0 && offset <= eng.size() {
                    return idx.byte_offset_to_line(eng, offset);
                }
            }
            1
        };

        let mut buf = Vec::with_capacity(4096);
        let mut elements_count = 0;
        let mut templates_count = 0;
        let mut stylesheet_version = "1.0".to_string();

        let mut tag_stack: Vec<(String, u64)> = Vec::with_capacity(64);
        let mut root_seen = false;
        let mut inside_stylesheet = false;
        let mut choose_stack: Vec<ChooseState> = Vec::new();

        loop {
            if cancel.load(Ordering::Relaxed) {
                return XsltValidationResult::Invalid {
                    line_number: 1,
                    byte_offset: 0,
                    message: "Validation cancelled".to_string(),
                };
            }

            match xml_reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let offset = xml_reader.error_position();
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let (prefix, local_name) = Self::split_tag(&tag_name);

                    elements_count += 1;

                    // 1. Root element validation
                    if !root_seen {
                        root_seen = true;
                        if local_name != "stylesheet" && local_name != "transform" {
                            return XsltValidationResult::Invalid {
                                line_number: get_line_num(offset),
                                byte_offset: offset,
                                message: format!("Root element of XSLT must be <xsl:stylesheet> or <xsl:transform>, found <{}>", tag_name),
                            };
                        }

                        // Check namespace and version
                        let mut has_xsl_ns = false;
                        let mut has_version = false;

                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                let key = String::from_utf8_lossy(a.key.as_ref());
                                let val = String::from_utf8_lossy(a.value.as_ref());
                                if (key.starts_with("xmlns:") && val.contains("http://www.w3.org/1999/XSL/Transform"))
                                    || (key == "xmlns" && val.contains("http://www.w3.org/1999/XSL/Transform"))
                                {
                                    has_xsl_ns = true;
                                }
                                if key == "version" {
                                    has_version = true;
                                    stylesheet_version = val.to_string();
                                }
                            }
                        }

                        if !has_xsl_ns {
                            return XsltValidationResult::Invalid {
                                line_number: get_line_num(offset),
                                byte_offset: offset,
                                message: "XSLT root element missing mandatory namespace declaration 'xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"'".to_string(),
                            };
                        }
                        if !has_version {
                            return XsltValidationResult::Invalid {
                                line_number: get_line_num(offset),
                                byte_offset: offset,
                                message: "XSLT root element missing mandatory 'version' attribute (e.g. version=\"1.0\")".to_string(),
                            };
                        }

                        inside_stylesheet = true;
                        tag_stack.push((tag_name, offset));
                        buf.clear();
                        continue;
                    }

                    // 2. Top-level element validation
                    if tag_stack.len() == 1 && inside_stylesheet && (prefix == "xsl" || prefix.is_empty()) {
                        if !Self::VALID_TOP_LEVEL.contains(&local_name.as_str()) {
                            return XsltValidationResult::Invalid {
                                line_number: get_line_num(offset),
                                byte_offset: offset,
                                message: format!(
                                    "Element '<{}>' is not allowed as a top-level child of <xsl:stylesheet>. Allowed top-level elements: {}",
                                    tag_name,
                                    Self::VALID_TOP_LEVEL.join(", ")
                                ),
                            };
                        }
                    }

                    // 3. Template count
                    if local_name == "template" && (prefix == "xsl" || inside_stylesheet) {
                        templates_count += 1;
                    }

                    // 4. Element specific semantic checks
                    if prefix == "xsl" || (inside_stylesheet && local_name.starts_with("xsl:")) {
                        if let Err(msg) = Self::check_element_rules(
                            &local_name,
                            e,
                            &tag_stack,
                            &mut choose_stack,
                        ) {
                            return XsltValidationResult::Invalid {
                                line_number: get_line_num(offset),
                                byte_offset: offset,
                                message: msg,
                            };
                        }

                        // 5. XPath checks in attributes (select, test, match)
                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                let key = String::from_utf8_lossy(a.key.as_ref());
                                if key == "select" || key == "test" || key == "match" {
                                    let xpath = String::from_utf8_lossy(a.value.as_ref());
                                    if let Err(xpath_err) = Self::validate_xpath(&xpath) {
                                        return XsltValidationResult::Invalid {
                                            line_number: get_line_num(offset),
                                            byte_offset: offset,
                                            message: format!("Invalid XPath expression in attribute '{}'=\"{}\": {}", key, xpath, xpath_err),
                                        };
                                    }
                                }
                            }
                        }
                    }

                    tag_stack.push((tag_name, offset));
                }

                Ok(Event::Empty(ref e)) => {
                    let offset = xml_reader.error_position();
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let (prefix, local_name) = Self::split_tag(&tag_name);

                    elements_count += 1;

                    if prefix == "xsl" || (inside_stylesheet && local_name.starts_with("xsl:")) {
                        if let Err(msg) = Self::check_element_rules(
                            &local_name,
                            e,
                            &tag_stack,
                            &mut choose_stack,
                        ) {
                            return XsltValidationResult::Invalid {
                                line_number: get_line_num(offset),
                                byte_offset: offset,
                                message: msg,
                            };
                        }

                        for attr in e.attributes() {
                            if let Ok(a) = attr {
                                let key = String::from_utf8_lossy(a.key.as_ref());
                                if key == "select" || key == "test" || key == "match" {
                                    let xpath = String::from_utf8_lossy(a.value.as_ref());
                                    if let Err(xpath_err) = Self::validate_xpath(&xpath) {
                                        return XsltValidationResult::Invalid {
                                            line_number: get_line_num(offset),
                                            byte_offset: offset,
                                            message: format!("Invalid XPath in attribute '{}'=\"{}\": {}", key, xpath, xpath_err),
                                        };
                                    }
                                }
                            }
                        }
                    }
                }

                Ok(Event::End(_)) => {
                    let offset = xml_reader.error_position();

                    if let Some((start_name, _)) = tag_stack.pop() {
                        let (_, start_local) = Self::split_tag(&start_name);

                        if start_local == "choose" {
                            if let Some(choose_state) = choose_stack.pop() {
                                if !choose_state.has_when {
                                    return XsltValidationResult::Invalid {
                                        line_number: get_line_num(offset),
                                        byte_offset: offset,
                                        message: "<xsl:choose> must contain at least one <xsl:when> element".to_string(),
                                    };
                                }
                            }
                        }
                    }
                }

                Ok(Event::Eof) => break,

                Err(err) => {
                    let offset = xml_reader.error_position();
                    return XsltValidationResult::Invalid {
                        line_number: get_line_num(offset),
                        byte_offset: offset,
                        message: format!("XML Parse Error: {}", err),
                    };
                }

                _ => {}
            }

            buf.clear();
        }

        if !root_seen {
            return XsltValidationResult::Invalid {
                line_number: 1,
                byte_offset: 0,
                message: "Empty document (no XSLT elements found)".to_string(),
            };
        }

        if !tag_stack.is_empty() {
            let (unclosed_tag, pos) = tag_stack.last().unwrap();
            return XsltValidationResult::Invalid {
                line_number: get_line_num(*pos),
                byte_offset: *pos,
                message: format!("Unclosed XSLT element <{}>", unclosed_tag),
            };
        }

        XsltValidationResult::Valid {
            elements_count,
            templates_count,
            version: stylesheet_version,
            elapsed_secs: start.elapsed().as_secs_f64(),
        }
    }

    fn check_element_rules(
        local_name: &str,
        e: &quick_xml::events::BytesStart,
        tag_stack: &[(String, u64)],
        choose_stack: &mut Vec<ChooseState>,
    ) -> Result<(), String> {
        let parent = tag_stack.last().map(|(name, _)| Self::split_tag(name).1);

        match local_name {
            "template" => {
                let has_match = Self::has_attr(e, "match");
                let has_name = Self::has_attr(e, "name");
                if !has_match && !has_name {
                    return Err("<xsl:template> requires either a 'match' or 'name' attribute".to_string());
                }
            }
            "value-of" => {
                if !Self::has_attr(e, "select") {
                    return Err("<xsl:value-of> requires a 'select' attribute".to_string());
                }
            }
            "for-each" => {
                if !Self::has_attr(e, "select") {
                    return Err("<xsl:for-each> requires a 'select' attribute".to_string());
                }
            }
            "if" => {
                if !Self::has_attr(e, "test") {
                    return Err("<xsl:if> requires a 'test' attribute".to_string());
                }
            }
            "choose" => {
                choose_stack.push(ChooseState {
                    has_when: false,
                    has_otherwise: false,
                });
            }
            "when" => {
                if parent.as_deref() != Some("choose") {
                    return Err("<xsl:when> must be a direct child of <xsl:choose>".to_string());
                }
                if let Some(cs) = choose_stack.last_mut() {
                    if cs.has_otherwise {
                        return Err("<xsl:when> cannot appear after <xsl:otherwise> in <xsl:choose>".to_string());
                    }
                    cs.has_when = true;
                }
                if !Self::has_attr(e, "test") {
                    return Err("<xsl:when> requires a 'test' attribute".to_string());
                }
            }
            "otherwise" => {
                if parent.as_deref() != Some("choose") {
                    return Err("<xsl:otherwise> must be a direct child of <xsl:choose>".to_string());
                }
                if let Some(cs) = choose_stack.last_mut() {
                    if cs.has_otherwise {
                        return Err("<xsl:choose> cannot contain multiple <xsl:otherwise> elements".to_string());
                    }
                    cs.has_otherwise = true;
                }
            }
            "variable" | "param" | "with-param" => {
                if !Self::has_attr(e, "name") {
                    return Err(format!("<xsl:{}> requires a 'name' attribute", local_name));
                }
            }
            "call-template" => {
                if !Self::has_attr(e, "name") {
                    return Err("<xsl:call-template> requires a 'name' attribute".to_string());
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn has_attr(e: &quick_xml::events::BytesStart, attr_name: &str) -> bool {
        e.attributes().filter_map(|a| a.ok()).any(|a| {
            let key = String::from_utf8_lossy(a.key.as_ref());
            key == attr_name
        })
    }

    fn split_tag(tag: &str) -> (String, String) {
        if let Some(pos) = tag.find(':') {
            (tag[..pos].to_string(), tag[pos + 1..].to_string())
        } else {
            (String::new(), tag.to_string())
        }
    }

    /// Validates balanced parentheses, brackets, and quotes in an XPath expression.
    pub fn validate_xpath(expr: &str) -> Result<(), String> {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            return Err("Empty XPath expression".to_string());
        }

        let chars: Vec<char> = trimmed.chars().collect();
        let len = chars.len();

        let mut paren_depth = 0;
        let mut bracket_depth = 0;
        let mut in_single_quote = false;
        let mut in_double_quote = false;

        let mut idx = 0;
        while idx < len {
            let c = chars[idx];

            if in_single_quote {
                if c == '\'' {
                    in_single_quote = false;
                }
                idx += 1;
                continue;
            }

            if in_double_quote {
                if c == '"' {
                    in_double_quote = false;
                }
                idx += 1;
                continue;
            }

            match c {
                '\'' => in_single_quote = true,
                '"' => in_double_quote = true,
                '(' => paren_depth += 1,
                ')' => {
                    if paren_depth == 0 {
                        return Err("Unmatched closing parenthesis ')'".to_string());
                    }
                    paren_depth -= 1;
                }
                '[' => {
                    bracket_depth += 1;
                    // Check for empty predicate []
                    if idx + 1 < len && chars[idx + 1] == ']' {
                        return Err("Empty XPath predicate '[]'".to_string());
                    }
                }
                ']' => {
                    if bracket_depth == 0 {
                        return Err("Unmatched closing bracket ']'".to_string());
                    }
                    bracket_depth -= 1;
                }
                '/' => {
                    // Check for illegal triple slash ///
                    if idx + 2 < len && chars[idx + 1] == '/' && chars[idx + 2] == '/' {
                        return Err("Illegal triple slash '///' in XPath expression".to_string());
                    }
                }
                _ => {}
            }

            idx += 1;
        }

        if in_single_quote {
            return Err("Unclosed single quote in XPath string".to_string());
        }
        if in_double_quote {
            return Err("Unclosed double quote in XPath string".to_string());
        }
        if paren_depth > 0 {
            return Err("Unclosed parenthesis '(' in XPath expression".to_string());
        }
        if bracket_depth > 0 {
            return Err("Unclosed bracket '[' in XPath expression".to_string());
        }

        // Check dangling binary operators at end of expression
        let dangling_tokens = ["and", "or", "+", "-", "*", "div", "mod", "|", "=", "!=", "<", ">", "<=", ">="];
        for tok in dangling_tokens {
            if trimmed.ends_with(&format!(" {}", tok)) || trimmed == tok {
                return Err(format!("XPath expression cannot end with operator '{}'", tok));
            }
        }

        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_xslt_stylesheet() {
        let xslt = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
            <xsl:output method="xml" indent="yes" />
            <xsl:template match="/root">
                <catalog>
                    <xsl:for-each select="item">
                        <entry name="{name}">
                            <xsl:value-of select="title" />
                        </entry>
                    </xsl:for-each>
                </catalog>
            </xsl:template>
        </xsl:stylesheet>"#;

        let cancel = Arc::new(AtomicBool::new(false));
        let res = XsltValidator::validate(std::io::Cursor::new(xslt), None, None, cancel);
        match res {
            XsltValidationResult::Valid { templates_count, version, .. } => {
                assert_eq!(templates_count, 1);
                assert_eq!(version, "1.0");
            }
            XsltValidationResult::Invalid { message, line_number, .. } => {
                panic!("Expected valid XSLT, got error on line {}: {}", line_number, message);
            }
        }
    }

    #[test]
    fn test_invalid_xslt_template_missing_match_and_name() {
        let xslt = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
            <xsl:template>
                <item />
            </xsl:template>
        </xsl:stylesheet>"#;

        let cancel = Arc::new(AtomicBool::new(false));
        let res = XsltValidator::validate(std::io::Cursor::new(xslt), None, None, cancel);
        match res {
            XsltValidationResult::Invalid { message, .. } => {
                assert!(message.contains("<xsl:template> requires either a 'match' or 'name' attribute"));
            }
            _ => panic!("Expected invalid result"),
        }
    }

    #[test]
    fn test_invalid_xslt_value_of_missing_select() {
        let xslt = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
            <xsl:template match="/">
                <xsl:value-of />
            </xsl:template>
        </xsl:stylesheet>"#;

        let cancel = Arc::new(AtomicBool::new(false));
        let res = XsltValidator::validate(std::io::Cursor::new(xslt), None, None, cancel);
        match res {
            XsltValidationResult::Invalid { message, .. } => {
                assert!(message.contains("<xsl:value-of> requires a 'select' attribute"));
            }
            _ => panic!("Expected invalid result"),
        }
    }

    #[test]
    fn test_invalid_xslt_unclosed_xpath_bracket() {
        let xslt = r#"<?xml version="1.0" encoding="UTF-8"?>
        <xsl:stylesheet version="1.0" xmlns:xsl="http://www.w3.org/1999/XSL/Transform">
            <xsl:template match="/">
                <xsl:value-of select="catalog/book[@id='1'" />
            </xsl:template>
        </xsl:stylesheet>"#;

        let cancel = Arc::new(AtomicBool::new(false));
        let res = XsltValidator::validate(std::io::Cursor::new(xslt), None, None, cancel);
        match res {
            XsltValidationResult::Invalid { message, .. } => {
                assert!(message.contains("Unclosed bracket '['"));
            }
            _ => panic!("Expected invalid result"),
        }
    }
}
