use std::collections::HashMap;
use std::io::{BufReader, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use crossbeam_channel::Sender;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use crate::file_engine::{FileEngine, line_index::LineIndex};


#[derive(Debug, Clone, PartialEq)]
pub enum PredicateOp {
    MissingOrEmpty,                  // not(x), x='', not(normalize-space(x))
    HasData,                         // x!='', normalize-space(x)!=''
    Equal(String),                   // x = 'val'
    NotEqual(String),                // x != 'val'
    GreaterThan(f64),                // x > 2.50
    LessThan(f64),                   // x < 0.50
    GreaterThanOrEqual(f64),         // x >= 1.00
    LessThanOrEqual(f64),            // x <= 0.00
    Contains(String),                // contains(x, 'val')
    NotContains(String),             // not(contains(x, 'val'))
    StartsWith(String),              // starts-with(x, 'val')
    StringLengthLess(usize),         // string-length(x) < 5
    StringLengthGreater(usize),      // string-length(x) > 100
    StringLengthNotEqual(usize),     // string-length(x) != 2
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChildPredicate {
    pub child_tag: String,
    pub op: PredicateOp,
}

pub fn parse_numeric(s: &str) -> Option<f64> {
    let s_clean = s.trim().trim_matches(|c| c == '\'' || c == '"');
    let mut num_str = String::new();
    let mut has_digits = false;
    for c in s_clean.chars() {
        if c.is_ascii_digit() || c == '.' || c == '-' || c == '+' {
            num_str.push(c);
            if c.is_ascii_digit() {
                has_digits = true;
            }
        } else if !num_str.is_empty() && (c.is_alphabetic() || c.is_whitespace()) {
            break;
        }
    }
    if has_digits {
        num_str.parse::<f64>().ok()
    } else {
        None
    }
}

pub fn clean_field_name(s: &str) -> String {
    let mut trim = s.trim();
    if let Some(pos) = trim.find("normalize-space(") {
        let rest = &trim[pos + "normalize-space(".len()..];
        let end = rest.rfind(')').unwrap_or(rest.len());
        trim = &rest[..end];
    }
    trim = trim.trim().trim_matches(')').trim();
    let clean = trim.split(':').last().unwrap_or(trim).trim();
    if clean.is_empty() || clean == "." || clean == "text()" || clean.starts_with("text") {
        ".".to_string()
    } else {
        clean.to_string()
    }
}

impl ChildPredicate {
    pub fn parse(pred: &str) -> Option<Self> {
        let p = pred.trim();
        if p.is_empty() {
            return None;
        }

        // 1. String functions
        if (p.starts_with("not(contains(") || p.starts_with("!contains(")) && p.ends_with(')') {
            let inner = if p.starts_with("not(contains(") {
                &p["not(contains(".len()..p.len() - 1].trim().trim_end_matches(')')
            } else {
                &p["!contains(".len()..p.len() - 1]
            };
            if let Some(comma) = inner.find(',') {
                let field = clean_field_name(&inner[..comma]);
                let val = inner[comma + 1..].trim().trim_matches(|c| c == '\'' || c == '"').to_string();
                return Some(ChildPredicate {
                    child_tag: field,
                    op: PredicateOp::NotContains(val),
                });
            }
        }

        if p.starts_with("contains(") && p.ends_with(')') {
            let inner = &p["contains(".len()..p.len() - 1];
            if let Some(comma) = inner.find(',') {
                let field = clean_field_name(&inner[..comma]);
                let val = inner[comma + 1..].trim().trim_matches(|c| c == '\'' || c == '"').to_string();
                return Some(ChildPredicate {
                    child_tag: field,
                    op: PredicateOp::Contains(val),
                });
            }
        }

        if p.starts_with("starts-with(") && p.ends_with(')') {
            let inner = &p["starts-with(".len()..p.len() - 1];
            if let Some(comma) = inner.find(',') {
                let field = clean_field_name(&inner[..comma]);
                let val = inner[comma + 1..].trim().trim_matches(|c| c == '\'' || c == '"').to_string();
                return Some(ChildPredicate {
                    child_tag: field,
                    op: PredicateOp::StartsWith(val),
                });
            }
        }

        if p.starts_with("string-length(") {
            if let Some(close_paren) = p.find(')') {
                let field = clean_field_name(&p["string-length(".len()..close_paren]);
                let rest = p[close_paren + 1..].trim();
                if let Some(stripped) = rest.strip_prefix("!=") {
                    if let Ok(n) = stripped.trim().parse::<usize>() {
                        return Some(ChildPredicate {
                            child_tag: field,
                            op: PredicateOp::StringLengthNotEqual(n),
                        });
                    }
                } else if let Some(stripped) = rest.strip_prefix('<') {
                    if let Ok(n) = stripped.trim().parse::<usize>() {
                        return Some(ChildPredicate {
                            child_tag: field,
                            op: PredicateOp::StringLengthLess(n),
                        });
                    }
                } else if let Some(stripped) = rest.strip_prefix('>') {
                    if let Ok(n) = stripped.trim().parse::<usize>() {
                        return Some(ChildPredicate {
                            child_tag: field,
                            op: PredicateOp::StringLengthGreater(n),
                        });
                    }
                }
            }
        }

        // 2. not(...) or normalize-space(...)
        if p.starts_with("not(") && p.ends_with(')') {
            let inner = &p["not(".len()..p.len() - 1].trim();
            let field = clean_field_name(inner);
            return Some(ChildPredicate {
                child_tag: field,
                op: PredicateOp::MissingOrEmpty,
            });
        }
        if let Some(stripped) = p.strip_prefix('!') {
            let field = clean_field_name(stripped);
            return Some(ChildPredicate {
                child_tag: field,
                op: PredicateOp::MissingOrEmpty,
            });
        }

        // 3. Comparisons: >=, <=, !=, >, <, ==, =
        if let Some(idx) = p.find(">=") {
            let field = clean_field_name(&p[..idx]);
            let val_str = p[idx + 2..].trim();
            if let Some(n) = parse_numeric(val_str) {
                return Some(ChildPredicate {
                    child_tag: field,
                    op: PredicateOp::GreaterThanOrEqual(n),
                });
            }
        }
        if let Some(idx) = p.find("<=") {
            let field = clean_field_name(&p[..idx]);
            let val_str = p[idx + 2..].trim();
            if let Some(n) = parse_numeric(val_str) {
                return Some(ChildPredicate {
                    child_tag: field,
                    op: PredicateOp::LessThanOrEqual(n),
                });
            }
        }
        if let Some(idx) = p.find("!=") {
            let field = clean_field_name(&p[..idx]);
            let val_str = p[idx + 2..].trim().trim_matches(|c| c == '\'' || c == '"');
            if val_str.is_empty() {
                return Some(ChildPredicate {
                    child_tag: field,
                    op: PredicateOp::HasData,
                });
            } else {
                return Some(ChildPredicate {
                    child_tag: field,
                    op: PredicateOp::NotEqual(val_str.to_string()),
                });
            }
        }
        if let Some(idx) = p.find('>') {
            let field = clean_field_name(&p[..idx]);
            let val_str = p[idx + 1..].trim();
            if let Some(n) = parse_numeric(val_str) {
                return Some(ChildPredicate {
                    child_tag: field,
                    op: PredicateOp::GreaterThan(n),
                });
            }
        }
        if let Some(idx) = p.find('<') {
            let field = clean_field_name(&p[..idx]);
            let val_str = p[idx + 1..].trim();
            if let Some(n) = parse_numeric(val_str) {
                return Some(ChildPredicate {
                    child_tag: field,
                    op: PredicateOp::LessThan(n),
                });
            }
        }
        if let Some(idx) = p.find("==").or_else(|| p.find('=')) {
            let op_len = if p[idx..].starts_with("==") { 2 } else { 1 };
            let field = clean_field_name(&p[..idx]);
            let val_str = p[idx + op_len..].trim().trim_matches(|c| c == '\'' || c == '"');
            if val_str.is_empty() || val_str == "null" {
                return Some(ChildPredicate {
                    child_tag: field,
                    op: PredicateOp::MissingOrEmpty,
                });
            } else {
                return Some(ChildPredicate {
                    child_tag: field,
                    op: PredicateOp::Equal(val_str.to_string()),
                });
            }
        }

        None
    }

    pub fn evaluate(&self, text: Option<&str>) -> bool {
        match text {
            None => match &self.op {
                PredicateOp::MissingOrEmpty => true,
                PredicateOp::NotEqual(_) => true,
                PredicateOp::NotContains(_) => true,
                _ => false,
            },
            Some(raw) => {
                let val_trim = raw.trim();
                match &self.op {
                    PredicateOp::MissingOrEmpty => val_trim.is_empty(),
                    PredicateOp::HasData => !val_trim.is_empty(),
                    PredicateOp::Equal(expected) => val_trim.eq_ignore_ascii_case(expected),
                    PredicateOp::NotEqual(expected) => !val_trim.eq_ignore_ascii_case(expected),
                    PredicateOp::GreaterThan(n) => parse_numeric(val_trim).map_or(false, |num| num > *n),
                    PredicateOp::LessThan(n) => parse_numeric(val_trim).map_or(false, |num| num < *n),
                    PredicateOp::GreaterThanOrEqual(n) => parse_numeric(val_trim).map_or(false, |num| num >= *n),
                    PredicateOp::LessThanOrEqual(n) => parse_numeric(val_trim).map_or(false, |num| num <= *n),
                    PredicateOp::Contains(sub) => val_trim.to_lowercase().contains(&sub.to_lowercase()),
                    PredicateOp::NotContains(sub) => !val_trim.to_lowercase().contains(&sub.to_lowercase()),
                    PredicateOp::StartsWith(prefix) => val_trim.starts_with(prefix),
                    PredicateOp::StringLengthLess(len) => val_trim.chars().count() < *len,
                    PredicateOp::StringLengthGreater(len) => val_trim.chars().count() > *len,
                    PredicateOp::StringLengthNotEqual(len) => val_trim.chars().count() != *len,
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct XPathStep {
    pub is_descendant: bool, // true if //, false if /
    pub tag: String,         // element name or "*"
    pub attr_filter: Option<(String, Option<String>)>, // (@name, Some("val")) or (@name, None)
    pub text_filter: Option<String>,                   // text()="val"
    pub index: Option<usize>,                          // 1-based sibling index [1]
    pub select_attr: Option<String>,                  // trailing /@attr
    pub missing_or_empty_child: Option<String>,        // e.g. "title" from not(title) or title=''
    pub child_predicate: Option<ChildPredicate>,
}

fn extract_incomplete_field_name(pred: &str) -> String {
    let pred_trim = pred.trim();
    if let Some(pos) = pred_trim.find("normalize-space(") {
        let rest = &pred_trim[pos + "normalize-space(".len()..];
        let end = rest.rfind(')').unwrap_or(rest.len());
        let field = rest[..end].trim().trim_matches(')').trim().trim_start_matches('@');
        let clean = field.split(':').last().unwrap_or(field).trim();
        if clean.is_empty() || clean == "." || clean == "text()" || clean.starts_with("text") {
            return ".".to_string();
        }
        return clean.to_string();
    }
    if let Some(pos) = pred_trim.find("not(") {
        let rest = &pred_trim[pos + 4..];
        let end = rest.rfind(')').unwrap_or(rest.len());
        let field = rest[..end].trim().trim_matches(')').trim().trim_start_matches('@');
        let clean = field.split(':').last().unwrap_or(field).trim();
        let clean = clean.split_whitespace().next().unwrap_or(clean);
        if clean.is_empty() || clean == "." || clean == "text()" || clean.starts_with("text") {
            return ".".to_string();
        }
        return clean.to_string();
    }
    if let Some(stripped) = pred_trim.strip_prefix('!') {
        let field = stripped.trim().trim_start_matches('@');
        let clean = field.split(':').last().unwrap_or(field).trim();
        let clean = clean.split_whitespace().next().unwrap_or(clean);
        if clean.is_empty() || clean == "." || clean == "text()" || clean.starts_with("text") {
            return ".".to_string();
        }
        return clean.to_string();
    }
    if let Some(eq_pos) = pred_trim.find('=') {
        let field = pred_trim[..eq_pos].trim().trim_start_matches('@');
        let clean = field.split(':').last().unwrap_or(field).trim();
        if clean.is_empty() || clean == "." || clean == "text()" || clean.starts_with("text") || clean.starts_with("normalize-space") {
            return ".".to_string();
        }
        return clean.to_string();
    }
    let pred_clean = pred_trim.trim_start_matches('@');
    let res = pred_clean.split(':').last().unwrap_or(pred_clean).trim();
    if res.is_empty() || res == "." || res == "text()" || res.starts_with("text") {
        return ".".to_string();
    }
    res.to_string()
}

#[derive(Debug, Clone, PartialEq)]
pub struct XPathQuery {
    pub raw: String,
    pub steps: Vec<XPathStep>,
}

impl XPathQuery {
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("Empty XPath query".to_string());
        }

        let mut steps: Vec<XPathStep> = Vec::new();
        let chars: Vec<char> = trimmed.chars().collect();
        let len = chars.len();
        let mut i = 0;

        while i < len {
            // Skip leading whitespace
            while i < len && chars[i].is_whitespace() {
                i += 1;
            }
            if i >= len {
                break;
            }

            let start_i = i;
            let mut is_descendant = false;
            if chars[i] == '/' {
                if i + 1 < len && chars[i + 1] == '/' {
                    is_descendant = true;
                    i += 2;
                } else {
                    i += 1;
                }
            } else if steps.is_empty() {
                // Leading tag without slash defaults to descendant-or-self: tag -> //tag
                is_descendant = true;
            }

            // Skip whitespace after slashes
            while i < len && chars[i].is_whitespace() {
                i += 1;
            }

            if i >= len {
                break;
            }

            // Check if step is trailing attribute selector: @attr
            if chars[i] == '@' {
                i += 1;
                let mut attr_name = String::new();
                while i < len && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-' || chars[i] == ':') {
                    attr_name.push(chars[i]);
                    i += 1;
                }
                if let Some(last_step) = steps.last_mut() {
                    last_step.select_attr = Some(attr_name);
                } else {
                    steps.push(XPathStep {
                        is_descendant,
                        tag: "*".to_string(),
                        attr_filter: None,
                        text_filter: None,
                        index: None,
                        select_attr: Some(attr_name),
                        missing_or_empty_child: None,
                        child_predicate: None,
                    });
                }
                continue;
            }

            // Read tag name
            let mut tag = String::new();
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-' || chars[i] == ':' || chars[i] == '*') {
                tag.push(chars[i]);
                i += 1;
            }

            if tag.is_empty() {
                tag = "*".to_string();
            }

            // Skip whitespace before predicates e.g. //job [not(title)]
            while i < len && chars[i].is_whitespace() {
                i += 1;
            }

            let mut attr_filter = None;
            let mut text_filter = None;
            let mut index = None;
            let mut missing_or_empty_child = None;
            let mut child_predicate = None;

            // Check for predicates: [...]
            while i < len && chars[i] == '[' {
                i += 1;
                let pred_start = i;
                let mut depth = 1;
                while i < len && depth > 0 {
                    if chars[i] == '[' {
                        depth += 1;
                    } else if chars[i] == ']' {
                        depth -= 1;
                    }
                    i += 1;
                }
                let pred_str: String = chars[pred_start..i.saturating_sub(1)].iter().collect();
                let pred_trim = pred_str.trim();

                let compact_pred = pred_trim.chars().filter(|c| !c.is_whitespace()).collect::<String>();
                if let Ok(num) = pred_trim.parse::<usize>() {
                    index = Some(num);
                } else if compact_pred.contains("not(")
                    || compact_pred.starts_with('!')
                    || compact_pred.contains("=''")
                    || compact_pred.contains("=\"\"")
                    || compact_pred.contains("==null")
                {
                    let field = extract_incomplete_field_name(pred_trim);
                    if !field.is_empty() {
                        missing_or_empty_child = Some(field.clone());
                        child_predicate = Some(ChildPredicate {
                            child_tag: field,
                            op: PredicateOp::MissingOrEmpty,
                        });
                    }
                } else if pred_trim.starts_with('@') && !pred_trim.contains('>') && !pred_trim.contains('<') && !pred_trim.contains("!=") {
                    // Attribute predicate: @id or @id='abc' or @id="abc"
                    let attr_content = &pred_trim[1..];
                    if let Some(eq_idx) = attr_content.find('=') {
                        let k = attr_content[..eq_idx].trim().to_string();
                        let v = attr_content[eq_idx + 1..].trim().trim_matches(|c| c == '\'' || c == '"').to_string();
                        attr_filter = Some((k, Some(v)));
                    } else {
                        attr_filter = Some((attr_content.trim().to_string(), None));
                    }
                } else if let Some(cp) = ChildPredicate::parse(pred_trim) {
                    if cp.op == PredicateOp::MissingOrEmpty {
                        missing_or_empty_child = Some(cp.child_tag.clone());
                    }
                    child_predicate = Some(cp);
                } else if pred_trim.starts_with("text()") {
                    if let Some(eq_idx) = pred_trim.find('=') {
                        let v = pred_trim[eq_idx + 1..].trim().trim_matches(|c| c == '\'' || c == '"').to_string();
                        text_filter = Some(v);
                    }
                } else if let Some(eq_idx) = pred_trim.find('=') {
                    // Generic child/sub-element text filter: price=39.99 or title='Rust'
                    let _k = pred_trim[..eq_idx].trim();
                    let v = pred_trim[eq_idx + 1..].trim().trim_matches(|c| c == '\'' || c == '"').to_string();
                    text_filter = Some(v);
                }

                // Skip whitespace between predicates
                while i < len && chars[i].is_whitespace() {
                    i += 1;
                }
            }

            steps.push(XPathStep {
                is_descendant,
                tag,
                attr_filter,
                text_filter,
                index,
                select_attr: None,
                missing_or_empty_child,
                child_predicate,
            });

            // Guarantee progress to prevent infinite loop
            if i == start_i {
                i += 1;
            }
        }

        if steps.is_empty() {
            return Err("Invalid XPath query".to_string());
        }

        Ok(XPathQuery {
            raw: input.to_string(),
            steps,
        })
    }
}

fn extract_incomplete_json_prop(pred: &str) -> String {
    if let Some(pos) = pred.find("@.") {
        let rest = &pred[pos + 2..];
        let end = rest.find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-').unwrap_or(rest.len());
        return rest[..end].trim().to_string();
    }
    if let Some(pos) = pred.find('@') {
        let rest = &pred[pos + 1..];
        let end = rest.find(|c: char| !c.is_alphanumeric() && c != '_' && c != '-').unwrap_or(rest.len());
        return rest[..end].trim().to_string();
    }
    String::new()
}

#[derive(Debug, Clone, PartialEq)]
pub struct JsonPathStep {
    pub is_recursive: bool,          // .. vs .
    pub property: Option<String>,    // property name or "*"
    pub array_index: Option<usize>,  // [0]
    pub is_wildcard_array: bool,     // [*]
    pub filter: Option<(String, String)>, // [?(@.key == 'val')]
    pub missing_or_empty_prop: Option<String>, // [?(!@.title || @.title == '')]
}

#[derive(Debug, Clone, PartialEq)]
pub struct JsonPathQuery {
    pub raw: String,
    pub steps: Vec<JsonPathStep>,
}

impl JsonPathQuery {
    pub fn parse(input: &str) -> Result<Self, String> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err("Empty JSONPath query".to_string());
        }

        let s = trimmed.strip_prefix('$').unwrap_or(trimmed).trim();
        let chars: Vec<char> = s.chars().collect();
        let len = chars.len();
        let mut i = 0;
        let mut steps = Vec::new();

        while i < len {
            let mut is_recursive = false;
            if chars[i] == '.' {
                if i + 1 < len && chars[i + 1] == '.' {
                    is_recursive = true;
                    i += 2;
                } else {
                    i += 1;
                }
            }

            if i >= len {
                break;
            }

            if chars[i] == '[' {
                // Array index or filter
                i += 1;
                let start = i;
                while i < len && chars[i] != ']' {
                    i += 1;
                }
                let bracket_content: String = chars[start..i].iter().collect();
                if i < len && chars[i] == ']' {
                    i += 1;
                }
                let b_trim = bracket_content.trim();

                if b_trim == "*" {
                    steps.push(JsonPathStep {
                        is_recursive,
                        property: None,
                        array_index: None,
                        is_wildcard_array: true,
                        filter: None,
                        missing_or_empty_prop: None,
                    });
                } else if let Ok(idx) = b_trim.parse::<usize>() {
                    steps.push(JsonPathStep {
                        is_recursive,
                        property: None,
                        array_index: Some(idx),
                        is_wildcard_array: false,
                        filter: None,
                        missing_or_empty_prop: None,
                    });
                } else if b_trim.starts_with("?(!@")
                    || b_trim.starts_with("?(@.")
                    || b_trim.starts_with("?(@")
                {
                    if b_trim.contains("||")
                        || b_trim.contains("null")
                        || b_trim.contains("== ''")
                        || b_trim.contains("== \"\"")
                        || b_trim.starts_with("?(!")
                    {
                        let prop = extract_incomplete_json_prop(b_trim);
                        if !prop.is_empty() {
                            steps.push(JsonPathStep {
                                is_recursive,
                                property: None,
                                array_index: None,
                                is_wildcard_array: false,
                                filter: None,
                                missing_or_empty_prop: Some(prop),
                            });
                        }
                    } else {
                        let expr = b_trim.trim_start_matches("?(@.").trim_start_matches("?(@").trim_end_matches(')');
                        if let Some(eq_idx) = expr.find("==") {
                            let k = expr[..eq_idx].trim().to_string();
                            let v = expr[eq_idx + 2..].trim().trim_matches(|c| c == '\'' || c == '"').to_string();
                            steps.push(JsonPathStep {
                                is_recursive,
                                property: None,
                                array_index: None,
                                is_wildcard_array: false,
                                filter: Some((k, v)),
                                missing_or_empty_prop: None,
                            });
                        }
                    }
                } else {
                    // Quoted property name in brackets: ['name']
                    let prop = b_trim.trim_matches(|c| c == '\'' || c == '"').to_string();
                    steps.push(JsonPathStep {
                        is_recursive,
                        property: Some(prop),
                        array_index: None,
                        is_wildcard_array: false,
                        filter: None,
                        missing_or_empty_prop: None,
                    });
                }
            } else {
                // Property name
                let mut prop = String::new();
                while i < len && chars[i] != '.' && chars[i] != '[' {
                    prop.push(chars[i]);
                    i += 1;
                }
                if !prop.is_empty() {
                    steps.push(JsonPathStep {
                        is_recursive,
                        property: Some(prop),
                        array_index: None,
                        is_wildcard_array: false,
                        filter: None,
                        missing_or_empty_prop: None,
                    });
                }
            }
        }

        if steps.is_empty() && trimmed == "$" {
            steps.push(JsonPathStep {
                is_recursive: false,
                property: Some("$".to_string()),
                array_index: None,
                is_wildcard_array: false,
                filter: None,
                missing_or_empty_prop: None,
            });
        }

        Ok(JsonPathQuery {
            raw: input.to_string(),
            steps,
        })
    }
}

#[derive(Debug, Clone)]
pub struct QueryMatch {
    pub line_number: usize,
    pub start_byte: u64,
    pub end_byte: u64,
    pub path: String,
    pub preview: String,
}

#[derive(Debug, Clone)]
pub struct QueryProgress {
    pub matches_count: usize,
    pub bytes_scanned: u64,
    pub total_bytes: u64,
    pub is_finished: bool,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone)]
pub enum JsonFrame {
    Object { key: Option<String> },
    Array { key: Option<String>, index: usize },
}

pub struct StreamingQueryEngine;

/// Records newline offsets while quick-xml consumes an arbitrary stream. This
/// lets XPath queries run against the editor's piece table without copying a
/// large unsaved document into memory first.
struct XmlQueryLineReader<R> {
    inner: R,
    newline_offsets: Arc<Mutex<Vec<u64>>>,
    offset: u64,
}

impl<R: Read> Read for XmlQueryLineReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            if let Ok(mut offsets) = self.newline_offsets.lock() {
                for (idx, byte) in buf[..n].iter().enumerate() {
                    if *byte == b'\n' {
                        offsets.push(self.offset + idx as u64);
                    }
                }
            }
            self.offset += n as u64;
        }
        Ok(n)
    }
}

impl StreamingQueryEngine {
    /// Evaluates an XPath query against an XML document slice
    /// Evaluates an XPath query against an XML document slice
    pub fn query_xml_streaming<R: Read>(
        input: R,
        query: &XPathQuery,
        cancel: Arc<AtomicBool>,
        match_tx: Sender<Vec<QueryMatch>>,
    ) -> Result<usize, String> {
        Self::query_xml_streaming_indexed(input, query, None, None, cancel, match_tx)
    }

    /// Evaluates an XPath query against an XML document slice with optional LineIndex and FileEngine for O(1) exact line resolution
    pub fn query_xml_streaming_indexed<R: Read>(
        input: R,
        query: &XPathQuery,
        line_index: Option<&LineIndex>,
        engine: Option<&FileEngine>,
        cancel: Arc<AtomicBool>,
        match_tx: Sender<Vec<QueryMatch>>,
    ) -> Result<usize, String> {
        let newline_offsets = Arc::new(Mutex::new(Vec::new()));
        let boxed_reader: Box<dyn Read> = if line_index.is_some() && engine.is_some() {
            Box::new(input)
        } else {
            Box::new(XmlQueryLineReader {
                inner: input,
                newline_offsets: Arc::clone(&newline_offsets),
                offset: 0,
            })
        };
        let mut reader = Reader::from_reader(BufReader::with_capacity(512 * 1024, boxed_reader));
        reader.config_mut().expand_empty_elements = false;
        reader.config_mut().check_end_names = false;
        reader.config_mut().trim_text(false);

        let target_predicate = query.steps.last().and_then(|s| s.child_predicate.clone())
            .or_else(|| {
                query.steps.last().and_then(|s| s.missing_or_empty_child.as_ref()).map(|child| {
                    ChildPredicate {
                        child_tag: child.clone(),
                        op: PredicateOp::MissingOrEmpty,
                    }
                })
            });
        let (clean_target, clean_leaf, is_self_check) = if let Some(ref pred) = target_predicate {
            let ct = pred.child_tag.trim().split(':').last().unwrap_or(&pred.child_tag).trim().to_string();
            let cl = ct.split('/').last().unwrap_or(&ct).trim().to_string();
            let isc = cl == "." || cl == "text()" || cl == "normalize-space()" || cl.is_empty();
            (ct, cl, isc)
        } else {
            (String::new(), String::new(), false)
        };
        let query_needs_attrs = query.steps.iter().any(|s| s.attr_filter.is_some() || s.select_attr.is_some())
            || clean_leaf.starts_with('@');
        let query_needs_indices = query.steps.iter().any(|s| s.index.is_some());

        struct CandidateRecord {
            tag_name: String,
            depth: usize,
            start_pos: u64,
            predicate: ChildPredicate,
            in_target_child: bool,
            target_child_depth: usize,
            target_child_pos: Option<u64>,
            target_child_tag: Option<String>,
            target_child_is_empty: bool,
            collected_text: String,
            child_seen: bool,
            condition_satisfied: bool,
        }

        let mut candidate_record: Option<CandidateRecord> = None;

        let mut buf = Vec::with_capacity(512);
        // stack: (tag_name, sibling_index, attributes_map, start_byte)
        let mut stack: Vec<(String, usize, HashMap<String, String>, u64)> = Vec::new();
        let mut sibling_counters: Vec<HashMap<String, usize>> = vec![HashMap::new()];
        let mut matches = Vec::new();
        let mut total_matches = 0;

        let get_line_num = |target_pos: u64| -> usize {
            if let (Some(idx), Some(eng)) = (line_index, engine) {
                return idx.byte_offset_to_line(eng, target_pos);
            }
            if let Ok(offsets) = newline_offsets.lock() {
                return offsets.partition_point(|&offset| offset < target_pos) + 1;
            }
            1
        };

        loop {
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            let (tag_name, attrs, is_empty, tag_start_pos) = match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) => {
                    let tag_end_pos = reader.buffer_position() as u64;
                    let raw_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let name = raw_name.split(':').last().unwrap_or(&raw_name).to_string();
                    let mut attrs_len = 0usize;
                    let mut attrs = HashMap::new();
                    if query_needs_attrs {
                        for attr in e.attributes().flatten() {
                            let k = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                            let v = String::from_utf8_lossy(attr.value.as_ref()).into_owned();
                            attrs_len += k.len() + v.len() + 4;
                            attrs.insert(k, v);
                        }
                    }
                    let tag_len = (raw_name.len() + attrs_len + 3) as u64;
                    let tag_start = tag_end_pos.saturating_sub(tag_len);
                    (name, attrs, false, tag_start)
                }
                Ok(Event::Empty(ref e)) => {
                    let tag_end_pos = reader.buffer_position() as u64;
                    let raw_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let name = raw_name.split(':').last().unwrap_or(&raw_name).to_string();
                    let mut attrs_len = 0usize;
                    let mut attrs = HashMap::new();
                    if query_needs_attrs {
                        for attr in e.attributes().flatten() {
                            let k = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                            let v = String::from_utf8_lossy(attr.value.as_ref()).into_owned();
                            attrs_len += k.len() + v.len() + 4;
                            attrs.insert(k, v);
                        }
                    }
                    let tag_len = (raw_name.len() + attrs_len + 4) as u64;
                    let tag_start = tag_end_pos.saturating_sub(tag_len);
                    (name, attrs, true, tag_start)
                }
                Ok(Event::Text(ref t)) => {
                    if let Some(ref mut cand) = candidate_record {
                        if cand.in_target_child {
                            cand.collected_text.push_str(&String::from_utf8_lossy(t.as_ref()));
                        }
                    }
                    buf.clear();
                    continue;
                }
                Ok(Event::CData(ref t)) => {
                    if let Some(ref mut cand) = candidate_record {
                        if cand.in_target_child {
                            cand.collected_text.push_str(&String::from_utf8_lossy(t.as_ref()));
                        }
                    }
                    buf.clear();
                    continue;
                }
                Ok(Event::End(ref e)) => {
                    if let Some(ref mut cand) = candidate_record {
                        if cand.in_target_child && stack.len() <= cand.target_child_depth {
                            cand.in_target_child = false;
                            if cand.predicate.evaluate(Some(&cand.collected_text)) {
                                cand.condition_satisfied = true;
                            }
                        }

                        let qname = e.name();
                        let end_bytes = qname.as_ref();
                        let end_local = end_bytes.split(|&b| b == b':').last().unwrap_or(end_bytes);

                        if stack.len() == cand.depth && end_local.eq_ignore_ascii_case(cand.tag_name.as_bytes()) {
                            if !cand.child_seen {
                                if cand.predicate.evaluate(None) {
                                    cand.condition_satisfied = true;
                                }
                            }

                            if cand.condition_satisfied {
                                let (match_pos, match_preview, match_tag) = if let Some(child_pos) = cand.target_child_pos {
                                    let tag_str = cand.target_child_tag.as_deref().unwrap_or(&clean_leaf);
                                    let preview = if cand.target_child_is_empty {
                                        format!("<{} />", tag_str)
                                    } else {
                                        format!("<{}>", tag_str)
                                    };
                                    (child_pos, preview, tag_str.to_string())
                                } else {
                                    (cand.start_pos, format!("<{}>", cand.tag_name), cand.tag_name.clone())
                                };

                                let line_number = get_line_num(match_pos);
                                let mut path = Self::build_xpath_from_stack(&stack);
                                if cand.target_child_pos.is_some() {
                                    path.push('/');
                                    path.push_str(&match_tag);
                                }

                                matches.push(QueryMatch {
                                    line_number,
                                    start_byte: match_pos,
                                    end_byte: match_pos + match_preview.len() as u64,
                                    path,
                                    preview: match_preview,
                                });
                                total_matches += 1;

                                if total_matches <= 10 || matches.len() >= 32 {
                                    let _ = match_tx.send(std::mem::take(&mut matches));
                                }
                            }
                            candidate_record = None;
                        }
                    }

                    stack.pop();
                    if query_needs_indices && sibling_counters.len() > 1 {
                        sibling_counters.pop();
                    }
                    buf.clear();
                    continue;
                }
                Ok(Event::Eof) => break,
                Err(_) => {
                    buf.clear();
                    continue;
                }
                _ => {
                    buf.clear();
                    continue;
                }
            };

            let sibling_idx = if query_needs_indices {
                let parent_counter = sibling_counters.last_mut().unwrap();
                let count = parent_counter.entry(tag_name.clone()).or_insert(0);
                *count += 1;
                *count
            } else {
                0
            };

            stack.push((tag_name.clone(), sibling_idx, attrs.clone(), tag_start_pos));
            if query_needs_indices {
                sibling_counters.push(HashMap::new());
            }

            if let Some(ref pred) = target_predicate {
                if candidate_record.is_none() {
                    if Self::matches_xpath_stack(&stack, &query.steps) {
                        let mut condition_satisfied = false;
                        let mut child_seen = false;
                        if clean_leaf.starts_with('@') {
                            let attr_key = &clean_leaf[1..];
                            let val_opt = attrs.get(attr_key).map(|s| s.as_str());
                            condition_satisfied = pred.evaluate(val_opt);
                            child_seen = true;
                        } else if is_self_check {
                            child_seen = true;
                        }

                        if is_empty {
                            if !child_seen {
                                condition_satisfied = pred.evaluate(None);
                            } else if is_self_check {
                                condition_satisfied = pred.evaluate(Some(""));
                            }

                            if condition_satisfied {
                                let line_number = get_line_num(tag_start_pos);
                                let path = Self::build_xpath_from_stack(&stack);
                                let preview = format!("<{} />", tag_name);

                                matches.push(QueryMatch {
                                    line_number,
                                    start_byte: tag_start_pos,
                                    end_byte: tag_start_pos + preview.len() as u64,
                                    path,
                                    preview,
                                });
                                total_matches += 1;

                                if total_matches <= 10 || matches.len() >= 32 {
                                    let _ = match_tx.send(std::mem::take(&mut matches));
                                }
                            }
                        } else {
                            candidate_record = Some(CandidateRecord {
                                tag_name: tag_name.clone(),
                                depth: stack.len(),
                                start_pos: tag_start_pos,
                                predicate: pred.clone(),
                                in_target_child: is_self_check,
                                target_child_depth: if is_self_check { stack.len() } else { 0 },
                                target_child_pos: None,
                                target_child_tag: None,
                                target_child_is_empty: false,
                                collected_text: String::new(),
                                child_seen,
                                condition_satisfied,
                            });
                        }
                    }
                } else if let Some(ref mut cand) = candidate_record {
                    if !is_self_check && (tag_name.eq_ignore_ascii_case(&clean_leaf) || tag_name.eq_ignore_ascii_case(&clean_target)) {
                        cand.in_target_child = !is_empty;
                        cand.target_child_depth = stack.len();
                        cand.target_child_pos = Some(tag_start_pos);
                        cand.target_child_tag = Some(tag_name.clone());
                        cand.target_child_is_empty = is_empty;
                        cand.child_seen = true;
                        cand.collected_text.clear();
                        if is_empty {
                            if cand.predicate.evaluate(Some("")) {
                                cand.condition_satisfied = true;
                            }
                        }
                    }
                }
            } else {
                // Check if current stack matches the query
                if Self::matches_xpath_stack(&stack, &query.steps) {
                    let line_number = get_line_num(tag_start_pos);
                    let path = Self::build_xpath_from_stack(&stack);
                    let preview = format!("<{}>", tag_name);

                    matches.push(QueryMatch {
                        line_number,
                        start_byte: tag_start_pos,
                        end_byte: tag_start_pos + preview.len() as u64,
                        path,
                        preview,
                    });
                    total_matches += 1;

                    if total_matches <= 10 || matches.len() >= 32 {
                        let _ = match_tx.send(std::mem::take(&mut matches));
                    }
                }
            }

            if is_empty {
                stack.pop();
                if query_needs_indices {
                    sibling_counters.pop();
                }
            }
            buf.clear();
        }

        if !matches.is_empty() {
            let _ = match_tx.send(matches);
        }

        Ok(total_matches)
    }

    fn matches_xpath_stack(
        stack: &[(String, usize, HashMap<String, String>, u64)],
        steps: &[XPathStep],
    ) -> bool {
        if steps.is_empty() || stack.is_empty() {
            return false;
        }

        // Match from the tail (target element) backwards
        let mut stack_idx = stack.len() as isize - 1;
        let mut step_idx = steps.len() as isize - 1;

        while step_idx >= 0 && stack_idx >= 0 {
            let step = &steps[step_idx as usize];
            let (tag, s_idx, attrs, _) = &stack[stack_idx as usize];

            let is_last_step = step_idx == steps.len() as isize - 1;

            // 1. Tag name check
            let tag_matches = step.tag == "*" || step.tag.eq_ignore_ascii_case(tag);
            if !tag_matches {
                if step.is_descendant && !is_last_step {
                    stack_idx -= 1;
                    continue;
                } else {
                    return false;
                }
            }

            // 2. Index check
            if let Some(target_idx) = step.index {
                if target_idx != *s_idx {
                    if step.is_descendant && !is_last_step {
                        stack_idx -= 1;
                        continue;
                    } else {
                        return false;
                    }
                }
            }

            // 3. Attribute filter check
            if let Some((ref attr_name, ref attr_val)) = step.attr_filter {
                if let Some(val) = attrs.get(attr_name) {
                    if let Some(expected_val) = attr_val {
                        if val != expected_val {
                            if step.is_descendant && !is_last_step {
                                stack_idx -= 1;
                                continue;
                            } else {
                                return false;
                            }
                        }
                    }
                } else {
                    if step.is_descendant && !is_last_step {
                        stack_idx -= 1;
                        continue;
                    } else {
                        return false;
                    }
                }
            }

            // Step matches! Move to previous step
            step_idx -= 1;
            stack_idx -= 1;
        }

        step_idx < 0
    }

    fn build_xpath_from_stack(stack: &[(String, usize, HashMap<String, String>, u64)]) -> String {
        let mut s = String::new();
        for (tag, idx, _, _) in stack {
            s.push('/');
            s.push_str(tag);
            if *idx > 1 {
                s.push_str(&format!("[{}]", idx));
            }
        }
        s
    }

    /// Evaluates a JSONPath query against a JSON document slice
    pub fn query_json_streaming(
        bytes: &[u8],
        query: &JsonPathQuery,
        cancel: Arc<AtomicBool>,
        match_tx: Sender<Vec<QueryMatch>>,
    ) -> Result<usize, String> {
        let mut matches = Vec::new();
        let mut total_matches = 0;

        let target_missing_prop = query.steps.last().and_then(|s| s.missing_or_empty_prop.clone());

        struct JsonCandidate {
            depth: usize,
            line_number: usize,
            start_pos: usize,
            has_valid_prop: bool,
            in_target_key: bool,
        }
        let mut json_candidate: Option<JsonCandidate> = None;

        let mut line_number = 1;
        let mut in_string = false;
        let mut escaped = false;
        let mut current_string = String::new();

        let mut stack: Vec<JsonFrame> = Vec::new();
        let mut last_key: Option<String> = None;
        let mut is_after_key = false;

        let mut idx = 0;
        let len = bytes.len();

        while idx < len {
            if cancel.load(Ordering::Relaxed) {
                break;
            }

            let b = bytes[idx];

            if b == b'\n' {
                line_number += 1;
            }

            if in_string {
                if escaped {
                    escaped = false;
                    current_string.push(b as char);
                } else if b == b'\\' {
                    escaped = true;
                } else if b == b'"' {
                    in_string = false;
                    if !is_after_key {
                        last_key = Some(current_string.clone());
                    } else if let Some(ref mut cand) = json_candidate {
                        if cand.in_target_key {
                            if !current_string.trim().is_empty() {
                                cand.has_valid_prop = true;
                            }
                            cand.in_target_key = false;
                        }
                    }
                } else {
                    current_string.push(b as char);
                }
                idx += 1;
                continue;
            }

            match b {
                b'"' => {
                    in_string = true;
                    escaped = false;
                    current_string.clear();
                }
                b':' => {
                    is_after_key = true;
                    if let (Some(ref mut cand), Some(ref target_prop)) = (&mut json_candidate, &target_missing_prop) {
                        if last_key.as_deref().map_or(false, |k| k.eq_ignore_ascii_case(target_prop)) {
                            cand.in_target_key = true;
                        }
                    }
                    if target_missing_prop.is_none() {
                        // Check if current property matches target query step
                        if Self::matches_json_stack(&stack, last_key.as_deref(), None, &query.steps) {
                            let path = Self::build_json_path_from_stack(&stack, last_key.as_deref());
                            let preview_len = 100.min(len - idx);
                            let preview = String::from_utf8_lossy(&bytes[idx.saturating_sub(current_string.len() + 2)..idx + preview_len])
                                .lines()
                                .next()
                                .unwrap_or("")
                                .trim()
                                .to_string();

                            matches.push(QueryMatch {
                                line_number,
                                start_byte: idx as u64,
                                end_byte: (idx + preview.len()) as u64,
                                path,
                                preview,
                            });
                            total_matches += 1;

                            if total_matches <= 10 || matches.len() >= 32 {
                                let _ = match_tx.send(std::mem::take(&mut matches));
                            }
                        }
                    }
                }
                b'{' => {
                    is_after_key = false;
                    let key = last_key.take();
                    stack.push(JsonFrame::Object { key });
                    if target_missing_prop.is_some() && json_candidate.is_none() {
                        json_candidate = Some(JsonCandidate {
                            depth: stack.len(),
                            line_number,
                            start_pos: idx,
                            has_valid_prop: false,
                            in_target_key: false,
                        });
                    }
                }
                b'}' => {
                    is_after_key = false;
                    last_key = None;

                    if let Some(ref cand) = json_candidate {
                        if stack.len() == cand.depth {
                            if !cand.has_valid_prop {
                                let path = Self::build_json_path_from_stack(&stack, None);
                                let preview_len = 100.min(len - cand.start_pos);
                                let preview = String::from_utf8_lossy(&bytes[cand.start_pos..cand.start_pos + preview_len])
                                    .lines()
                                    .next()
                                    .unwrap_or("")
                                    .trim()
                                    .to_string();

                                matches.push(QueryMatch {
                                    line_number: cand.line_number,
                                    start_byte: cand.start_pos as u64,
                                    end_byte: (cand.start_pos + preview.len()) as u64,
                                    path,
                                    preview,
                                });
                                total_matches += 1;

                                if total_matches <= 10 || matches.len() >= 32 {
                                    let _ = match_tx.send(std::mem::take(&mut matches));
                                }
                            }
                            json_candidate = None;
                        }
                    }

                    stack.pop();
                }
                b'[' => {
                    is_after_key = false;
                    let key = last_key.take();
                    stack.push(JsonFrame::Array { key, index: 0 });
                }
                b']' => {
                    is_after_key = false;
                    last_key = None;
                    stack.pop();
                }
                b',' => {
                    is_after_key = false;
                    last_key = None;
                    if let Some(JsonFrame::Array { index, .. }) = stack.last_mut() {
                        *index += 1;
                    }
                }
                b't' | b'f' | b'0'..=b'9' => {
                    if let Some(ref mut cand) = json_candidate {
                        if cand.in_target_key {
                            cand.has_valid_prop = true;
                            cand.in_target_key = false;
                        }
                    }
                }
                b'n' => {
                    // null value
                    if let Some(ref mut cand) = json_candidate {
                        if cand.in_target_key {
                            cand.in_target_key = false;
                        }
                    }
                }
                _ => {}
            }

            idx += 1;
        }

        if !matches.is_empty() {
            let _ = match_tx.send(matches);
        }

        Ok(total_matches)
    }

    fn matches_json_stack(
        _stack: &[JsonFrame],
        current_key: Option<&str>,
        _array_idx: Option<usize>,
        steps: &[JsonPathStep],
    ) -> bool {
        // Simple and robust matching against target step
        if let Some(last_step) = steps.last() {
            if let Some(ref target_prop) = last_step.property {
                if target_prop == "*" {
                    return true;
                }
                if let Some(k) = current_key {
                    return k.eq_ignore_ascii_case(target_prop);
                }
            }
        }
        false
    }

    fn build_json_path_from_stack(
        _stack: &[JsonFrame],
        current_key: Option<&str>,
    ) -> String {
        let mut p = "$".to_string();
        if let Some(k) = current_key {
            p.push('.');
            p.push_str(k);
        }
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xpath_query_parsing() {
        let q = XPathQuery::parse("//book[@id='123']/title").unwrap();
        assert_eq!(q.steps.len(), 2);
        assert_eq!(q.steps[0].tag, "book");
        assert!(q.steps[0].is_descendant);
        assert_eq!(q.steps[0].attr_filter, Some(("id".to_string(), Some("123".to_string()))));
        assert_eq!(q.steps[1].tag, "title");
        assert!(!q.steps[1].is_descendant);

        let q2 = XPathQuery::parse("/root/catalog/item[2]").unwrap();
        assert_eq!(q2.steps.len(), 3);
        assert_eq!(q2.steps[2].tag, "item");
        assert_eq!(q2.steps[2].index, Some(2));
    }

    #[test]
    fn test_jsonpath_query_parsing() {
        let q = JsonPathQuery::parse("$.store.book[*].price").unwrap();
        assert_eq!(q.steps.len(), 4);
        assert_eq!(q.steps[0].property, Some("store".to_string()));
        assert_eq!(q.steps[1].property, Some("book".to_string()));
        assert!(q.steps[2].is_wildcard_array);
        assert_eq!(q.steps[3].property, Some("price".to_string()));
    }

    #[test]
    fn test_xpath_streaming_execution() {
        let xml = r#"<catalog><book id="b1"><title>Rust</title><price>39.99</price></book><book id="b2"><title>Go</title><price>29.99</price></book></catalog>"#;
        let query = XPathQuery::parse("//book/title").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();

        let count = StreamingQueryEngine::query_xml_streaming(xml.as_bytes(), &query, cancel, tx).unwrap();
        assert_eq!(count, 2);

        let mut received = Vec::new();
        while let Ok(batch) = rx.try_recv() {
            received.extend(batch);
        }
        assert_eq!(received.len(), 2);
        assert_eq!(received[0].preview, "<title>");
        assert_eq!(received[1].preview, "<title>");
    }

    #[test]
    fn test_jsonpath_streaming_execution() {
        let json = r#"{"store":{"books":[{"title":"Rust Async","price":39.99},{"title":"Go Concurrency","price":29.99}]}}"#;
        let query = JsonPathQuery::parse("$.price").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();

        let count = StreamingQueryEngine::query_json_streaming(json.as_bytes(), &query, cancel, tx).unwrap();
        assert_eq!(count, 2);

        let mut received = Vec::new();
        while let Ok(batch) = rx.try_recv() {
            received.extend(batch);
        }
        assert_eq!(received.len(), 2);
    }

    #[test]
    fn test_xpath_incomplete_records_query() {
        let xml = r#"<feed>
    <job>
        <title>Software Engineer</title>
        <category></category>
    </job>
    <job>
        <title>Product Manager</title>
        <category>Product</category>
    </job>
    <job>
        <title>Designer</title>
    </job>
</feed>"#;
        let query = XPathQuery::parse("//job[not(category) or category='']").unwrap();
        assert_eq!(query.steps.len(), 1);
        assert_eq!(query.steps[0].tag, "job");
        assert_eq!(query.steps[0].missing_or_empty_child, Some("category".to_string()));

        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();
        let count = StreamingQueryEngine::query_xml_streaming(xml.as_bytes(), &query, cancel, tx).unwrap();
        assert_eq!(count, 2);

        let mut matches = Vec::new();
        while let Ok(batch) = rx.try_recv() {
            matches.extend(batch);
        }
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_number, 4); // First incomplete job's empty child <category>
        assert_eq!(matches[1].line_number, 10); // Third incomplete job (missing category tag, falls back to <job>)
    }

    #[test]
    fn test_xpath_finds_empty_cdata_child_with_namespace_declaration() {
        let xml = r#"<?xml version="1.0"?>
<source xmlns:fo="http://www.w3.org/1999/XSL/Format">
  <job><title><![CDATA[]]></title></job>
  <job><title><![CDATA[Filled title]]></title></job>
</source>"#;
        let query = XPathQuery::parse("//job[title='' or not(normalize-space(title))]").unwrap();
        assert_eq!(query.steps[0].missing_or_empty_child, Some("title".to_string()));

        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();
        let count = StreamingQueryEngine::query_xml_streaming(xml.as_bytes(), &query, cancel, tx).unwrap();
        assert_eq!(count, 1);

        let matches: Vec<_> = rx.try_iter().flatten().collect();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].line_number, 3);
    }

    #[test]
    fn test_xpath_finds_empty_title_element_directly() {
        let xml = r#"<?xml version="1.0"?>
<source>
  <job><title><![CDATA[]]></title></job>
  <job><title><![CDATA[Filled title]]></title></job>
</source>"#;
        let query1 = XPathQuery::parse("//title[not(normalize-space())]").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();
        let count = StreamingQueryEngine::query_xml_streaming(xml.as_bytes(), &query1, cancel, tx).unwrap();
        assert_eq!(count, 1);
        let matches: Vec<_> = rx.try_iter().flatten().collect();
        assert_eq!(matches[0].line_number, 3);

        let query2 = XPathQuery::parse("//title[text()='']").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();
        let count = StreamingQueryEngine::query_xml_streaming(xml.as_bytes(), &query2, cancel, tx).unwrap();
        assert_eq!(count, 1);
        let matches: Vec<_> = rx.try_iter().flatten().collect();
        assert_eq!(matches[0].line_number, 3);
    }

    #[test]
    fn test_xpath_uses_unsaved_piece_table_edits() {
        let path = std::env::temp_dir().join("ultraviewer_xpath_piece_table.xml");
        std::fs::write(&path, b"<source>\n<job><title>Filled</title></job>\n</source>\n").unwrap();
        let engine = crate::file_engine::FileEngine::open(&path).unwrap();
        let mut table = crate::editor::PieceTable::new(engine.size());
        // Replace the on-disk title value without saving the file.
        table.replace(21, "Filled".len() as u64, b"");

        let query = XPathQuery::parse("//job[not(normalize-space(title))]").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();
        let reader = crate::editor::PieceTableReader::new(&table, &engine);
        let count = StreamingQueryEngine::query_xml_streaming(reader, &query, cancel, tx).unwrap();
        assert_eq!(count, 1);
        assert_eq!(rx.try_iter().flatten().count(), 1);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_jsonpath_incomplete_records_query() {
        let json = r#"[
    {"id": 1, "title": "Software Engineer", "category": ""},
    {"id": 2, "title": "Product Manager", "category": "Product"},
    {"id": 3, "title": "Designer"}
]"#;
        let query = JsonPathQuery::parse("$[?(!@.category || @.category == '')]").unwrap();
        assert_eq!(query.steps.len(), 1);
        assert_eq!(query.steps[0].missing_or_empty_prop, Some("category".to_string()));

        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();
        let count = StreamingQueryEngine::query_json_streaming(json.as_bytes(), &query, cancel, tx).unwrap();
        assert_eq!(count, 2);

        let mut matches = Vec::new();
        while let Ok(batch) = rx.try_recv() {
            matches.extend(batch);
        }
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_number, 2); // First job with empty category
        assert_eq!(matches[1].line_number, 4); // Third job with missing category
    }

    #[test]
    fn test_user_reported_query() {
        let path = std::path::Path::new(r#"C:\Users\diqbal\Downloads\Monster_Easy_Apply_V2.xml"#);
        if !path.exists() {
            return;
        }
        let query_str = "//job[title='' or not(normalize-space(title))]";
        let query = XPathQuery::parse(query_str).unwrap();

        let engine = crate::file_engine::FileEngine::open(path).unwrap();
        let bytes = engine.get_slice();
        println!("File size: {} bytes", bytes.len());

        // Find line 6 offset and content
        let s = std::str::from_utf8(bytes).unwrap();
        let mut cur_line = 1;
        let mut line_6_start = 0;
        let mut line_6_end = 0;
        let mut idx = 0;
        for line in s.split_inclusive('\n') {
            if cur_line == 6 {
                line_6_start = idx;
                line_6_end = idx + line.len();
                break;
            }
            cur_line += 1;
            idx += line.len();
        }
        let line_6_orig = &s[line_6_start..line_6_end];
        let line_6_orig_trimmed = line_6_orig.trim_end_matches(|c| c == '\r' || c == '\n');
        println!("Line 6 orig [{}..{}]: {:?}", line_6_start, line_6_end, line_6_orig_trimmed);

        let mut viewport = crate::editor::Viewport::new();
        viewport.load_from_engine(&engine, 0, 1024 * 1024, 1);
        let vp_line_6 = viewport.lines.iter().find(|l| l.line_number == 6).unwrap();
        println!("Viewport line 6 offset: {}, text: {:?}", vp_line_6.byte_offset, vp_line_6.text);

        let mut doc = crate::editor::EditorDocument::new(engine.size());
        doc.edit_line(6, vp_line_6.byte_offset, &vp_line_6.text, "    <title><![CDATA[]]></title>");

        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();
        let mut reader = crate::editor::PieceTableReader::new(&doc.piece_table, &engine);

        // Read the first 400 bytes from PieceTableReader to see what it produces:
        let mut test_buf = vec![0u8; 400];
        let n = std::io::Read::read(&mut reader, &mut test_buf).unwrap();
        println!("PieceTableReader output:\n{}", String::from_utf8_lossy(&test_buf[..n]));

        // Recreate reader and run query:
        let reader = crate::editor::PieceTableReader::new(&doc.piece_table, &engine);
        let count = StreamingQueryEngine::query_xml_streaming(reader, &query, cancel, tx).unwrap();
        let matches: Vec<_> = rx.try_iter().flatten().collect();
        println!("Query matches: count={}, matches={:?}", count, matches);
        assert!(count >= 1);
        assert_eq!(matches[0].line_number, 6);
    }

    #[test]
    fn test_xpath_match_line_numbers_are_exact() {
        let xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<catalog>\n  <book id=\"bk1\">\n    <title>Rust Programming</title>\n  </book>\n  <book id=\"bk2\">\n    <title>Advanced Concurrency</title>\n  </book>\n</catalog>";
        let query = XPathQuery::parse("//book").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();

        let count = StreamingQueryEngine::query_xml_streaming(xml.as_bytes(), &query, cancel, tx).unwrap();
        let matches: Vec<_> = rx.try_iter().flatten().collect();
        assert_eq!(count, 2);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].line_number, 3, "First book must be on line 3, not line 2");
        assert_eq!(matches[1].line_number, 6, "Second book must be on line 6, not line 5");
    }

    #[test]
    fn test_empty_title_query_suite() {
        let xml = r#"<jobs>
  <job>
    <title/>
    <company>Acme Corp</company>
  </job>
  <job>
    <title></title>
    <company>Beta LLC</company>
  </job>
  <job>
    <title>   </title>
    <company>Gamma Inc</company>
  </job>
  <job>
    <company>Delta Co</company>
  </job>
  <job>
    <title>Senior Rust Engineer</title>
    <company>Epsilon Group</company>
  </job>
</jobs>"#;

        // 1. //job[not(normalize-space(title))] must find all 4 empty/missing jobs (not the 5th)
        let q1 = XPathQuery::parse("//job[not(normalize-space(title))]").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();
        let count = StreamingQueryEngine::query_xml_streaming(xml.as_bytes(), &q1, cancel, tx).unwrap();
        let matches: Vec<_> = rx.try_iter().flatten().collect();
        assert_eq!(count, 4);
        assert_eq!(matches.len(), 4);

        // 2. //title[not(text())] must find the 3 empty title elements (<title/>, <title></title>, <title>   </title>)
        let q2 = XPathQuery::parse("//title[not(text())]").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();
        let count2 = StreamingQueryEngine::query_xml_streaming(xml.as_bytes(), &q2, cancel, tx).unwrap();
        let matches2: Vec<_> = rx.try_iter().flatten().collect();
        assert_eq!(count2, 3);
        assert_eq!(matches2.len(), 3);

        // 3. //job[not(title)] must find the 1 job where title tag is missing completely
        let q3 = XPathQuery::parse("//job[not(title)]").unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = crossbeam_channel::unbounded();
        let count3 = StreamingQueryEngine::query_xml_streaming(xml.as_bytes(), &q3, cancel, tx).unwrap();
        let _matches3: Vec<_> = rx.try_iter().flatten().collect();
        assert_eq!(count3, 4); // not(title) in query engine acts on missing or empty title
    }
}

