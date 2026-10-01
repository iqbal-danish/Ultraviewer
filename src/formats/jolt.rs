use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug, Clone, PartialEq)]
pub enum JoltValidationResult {
    Valid {
        operations_count: usize,
        operations: Vec<String>,
        elapsed_secs: f64,
    },
    Invalid {
        line_number: usize,
        byte_offset: u64,
        message: String,
    },
}

pub struct JoltValidator;

impl JoltValidator {
    const STANDARD_OPS: [&'static str; 5] = ["shift", "default", "remove", "sort", "cardinality"];

    /// Validate whether a text or reader is a semantically valid JOLT specification.
    /// Checks JSON grammar, root array structure, recognized operations, mandatory 'spec' blocks,
    /// and DSL wildcard / reference expressions (&, @, $, *, #, [], ()).
    pub fn validate<R: Read>(mut reader: R, cancel: Arc<AtomicBool>) -> JoltValidationResult {
        let start = Instant::now();

        // 1. Read input up to 50MB (JOLT specs are usually < 1MB)
        let mut bytes = Vec::new();
        let mut chunk = [0u8; 64 * 1024];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return JoltValidationResult::Invalid {
                    line_number: 1,
                    byte_offset: 0,
                    message: "Validation cancelled".to_string(),
                };
            }
            match reader.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    bytes.extend_from_slice(&chunk[..n]);
                    if bytes.len() > 50 * 1024 * 1024 {
                        return JoltValidationResult::Invalid {
                            line_number: 1,
                            byte_offset: 0,
                            message: "File exceeds 50MB maximum supported size for JOLT specification validation".to_string(),
                        };
                    }
                }
                Err(e) => {
                    return JoltValidationResult::Invalid {
                        line_number: 1,
                        byte_offset: 0,
                        message: format!("Read error: {}", e),
                    };
                }
            }
        }

        if bytes.is_empty() {
            return JoltValidationResult::Invalid {
                line_number: 1,
                byte_offset: 0,
                message: "Empty document (no JOLT specification found)".to_string(),
            };
        }

        let text = match std::str::from_utf8(&bytes) {
            Ok(s) => s,
            Err(e) => {
                return JoltValidationResult::Invalid {
                    line_number: 1,
                    byte_offset: e.valid_up_to() as u64,
                    message: "Invalid UTF-8 encoding in JOLT specification".to_string(),
                };
            }
        };

        // 2. Parse JSON
        let json_val: serde_json::Value = match serde_json::from_str(text) {
            Ok(v) => v,
            Err(err) => {
                return JoltValidationResult::Invalid {
                    line_number: err.line(),
                    byte_offset: Self::calc_offset_from_line_col(text, err.line(), err.column()),
                    message: format!("JSON Syntax Error: {}", err),
                };
            }
        };

        // 3. Root structure check: JOLT spec must be an Array of Operation Objects
        let op_array = match json_val {
            serde_json::Value::Array(arr) => arr,
            serde_json::Value::Object(map) => {
                // Single operation object root is sometimes used in relaxed contexts
                vec![serde_json::Value::Object(map)]
            }
            _ => {
                return JoltValidationResult::Invalid {
                    line_number: 1,
                    byte_offset: 0,
                    message: "JOLT specification must be a JSON array of operation objects (e.g. [ { \"operation\": \"shift\", \"spec\": ... } ])".to_string(),
                };
            }
        };

        if op_array.is_empty() {
            return JoltValidationResult::Invalid {
                line_number: 1,
                byte_offset: 0,
                message: "JOLT specification array is empty (expected at least one transformation operation)".to_string(),
            };
        }

        let mut operations_list = Vec::with_capacity(op_array.len());

        for (idx, item) in op_array.iter().enumerate() {
            if cancel.load(Ordering::Relaxed) {
                return JoltValidationResult::Invalid {
                    line_number: 1,
                    byte_offset: 0,
                    message: "Validation cancelled".to_string(),
                };
            }

            let obj = match item {
                serde_json::Value::Object(map) => map,
                _ => {
                    let err_target = format!("[{}]", idx);
                    let (line, offset) = Self::find_token_position(text, &err_target, 0);
                    return JoltValidationResult::Invalid {
                        line_number: line,
                        byte_offset: offset,
                        message: format!("JOLT entry at index {} must be a JSON Object, found {}", idx, item),
                    };
                }
            };

            // Check "operation" field
            let op_str = match obj.get("operation") {
                Some(serde_json::Value::String(s)) => s.trim(),
                Some(other) => {
                    let (line, offset) = Self::find_token_position(text, "\"operation\"", 0);
                    return JoltValidationResult::Invalid {
                        line_number: line,
                        byte_offset: offset,
                        message: format!("Operation at index {} must be a string, found {}", idx, other),
                    };
                }
                None => {
                    let (line, offset) = Self::find_token_position(text, "{", idx);
                    return JoltValidationResult::Invalid {
                        line_number: line,
                        byte_offset: offset,
                        message: format!("Operation object at index {} is missing the mandatory 'operation' field", idx),
                    };
                }
            };

            // Validate operation name
            let is_standard = Self::STANDARD_OPS.contains(&op_str);
            let is_custom_class = !is_standard && op_str.contains('.') && op_str.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '$');

            if !is_standard && !is_custom_class {
                let (line, offset) = Self::find_token_position(text, &format!("\"{}\"", op_str), 0);
                return JoltValidationResult::Invalid {
                    line_number: line,
                    byte_offset: offset,
                    message: format!(
                        "Unknown JOLT operation '{}'. Expected one of ['shift', 'default', 'remove', 'sort', 'cardinality'] or a fully-qualified Java class",
                        op_str
                    ),
                };
            }

            operations_list.push(op_str.to_string());

            // Check "spec" field: mandatory for shift, default, remove, cardinality
            let spec_val = obj.get("spec");
            if op_str != "sort" {
                match spec_val {
                    Some(s) if !s.is_null() => {
                        // Validate spec content
                        if let Err((bad_token, err_msg)) = Self::validate_spec_value(s) {
                            let (line, offset) = Self::find_token_position(text, &bad_token, 0);
                            return JoltValidationResult::Invalid {
                                line_number: line,
                                byte_offset: offset,
                                message: format!("JOLT spec error in operation '{}': {}", op_str, err_msg),
                            };
                        }
                    }
                    _ => {
                        let (line, offset) = Self::find_token_position(text, &format!("\"{}\"", op_str), 0);
                        return JoltValidationResult::Invalid {
                            line_number: line,
                            byte_offset: offset,
                            message: format!("Operation '{}' at index {} requires a 'spec' definition", op_str, idx),
                        };
                    }
                }
            }
        }

        JoltValidationResult::Valid {
            operations_count: operations_list.len(),
            operations: operations_list,
            elapsed_secs: start.elapsed().as_secs_f64(),
        }
    }

    /// Recursively validates the syntax and wildcards inside a "spec" definition.
    fn validate_spec_value(val: &serde_json::Value) -> Result<(), (String, String)> {
        match val {
            serde_json::Value::Object(map) => {
                for (key, child_val) in map {
                    Self::validate_spec_key(key)?;
                    Self::validate_spec_value(child_val)?;
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    Self::validate_spec_value(item)?;
                }
            }
            serde_json::Value::String(s) => {
                Self::validate_spec_rhs_string(s)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Validates LHS key in JOLT spec (e.g., wildcards *, &, @, $, #, (a|b)).
    fn validate_spec_key(key: &str) -> Result<(), (String, String)> {
        let chars: Vec<char> = key.chars().collect();
        let len = chars.len();

        // Check balanced parentheses '(' and ')'
        let mut paren_depth = 0;
        for &c in &chars {
            if c == '(' {
                paren_depth += 1;
            } else if c == ')' {
                if paren_depth == 0 {
                    return Err((key.to_string(), format!("Unmatched closing parenthesis ')' in key '{}'", key)));
                }
                paren_depth -= 1;
            }
        }
        if paren_depth > 0 {
            return Err((key.to_string(), format!("Unclosed parenthesis '(' in key '{}'", key)));
        }

        // Check balanced brackets '[' and ']'
        let mut bracket_depth = 0;
        for &c in &chars {
            if c == '[' {
                bracket_depth += 1;
            } else if c == ']' {
                if bracket_depth == 0 {
                    return Err((key.to_string(), format!("Unmatched closing bracket ']' in key '{}'", key)));
                }
                bracket_depth -= 1;
            }
        }
        if bracket_depth > 0 {
            return Err((key.to_string(), format!("Unclosed bracket '[' in key '{}'", key)));
        }

        // Check ampersand syntax: & or &0 or &(1,2)
        let mut i = 0;
        while i < len {
            if chars[i] == '&' {
                if i + 1 < len && chars[i + 1] == '(' {
                    // Expect closing ')'
                    let close = chars[i + 2..].iter().position(|&c| c == ')');
                    if close.is_none() {
                        return Err((key.to_string(), format!("Unclosed ampersand lookup '&(' in key '{}'", key)));
                    }
                }
            }
            i += 1;
        }

        // Check value lookup '@' syntax: @ or @0 or @(1,path)
        let mut j = 0;
        while j < len {
            if chars[j] == '@' {
                if j + 1 < len && chars[j + 1] == '(' {
                    let close = chars[j + 2..].iter().position(|&c| c == ')');
                    if close.is_none() {
                        return Err((key.to_string(), format!("Unclosed value lookup '@(' in key '{}'", key)));
                    }
                }
            }
            j += 1;
        }

        // Check key lookup '$' syntax
        let mut k = 0;
        while k < len {
            if chars[k] == '$' {
                if k + 1 < len && chars[k + 1] == '(' {
                    let close = chars[k + 2..].iter().position(|&c| c == ')');
                    if close.is_none() {
                        return Err((key.to_string(), format!("Unclosed key lookup '$(' in key '{}'", key)));
                    }
                }
            }
            k += 1;
        }

        Ok(())
    }

    /// Validates RHS string target expression in JOLT spec (e.g. "a.b[&1].c").
    fn validate_spec_rhs_string(s: &str) -> Result<(), (String, String)> {
        let chars: Vec<char> = s.chars().collect();
        let len = chars.len();

        let mut bracket_depth = 0;
        let mut paren_depth = 0;

        for &c in &chars {
            if c == '[' {
                bracket_depth += 1;
            } else if c == ']' {
                if bracket_depth == 0 {
                    return Err((s.to_string(), format!("Unmatched closing bracket ']' in RHS expression '{}'", s)));
                }
                bracket_depth -= 1;
            } else if c == '(' {
                paren_depth += 1;
            } else if c == ')' {
                if paren_depth == 0 {
                    return Err((s.to_string(), format!("Unmatched closing parenthesis ')' in RHS expression '{}'", s)));
                }
                paren_depth -= 1;
            }
        }

        if bracket_depth > 0 {
            return Err((s.to_string(), format!("Unclosed bracket '[' in RHS expression '{}'", s)));
        }
        if paren_depth > 0 {
            return Err((s.to_string(), format!("Unclosed parenthesis '(' in RHS expression '{}'", s)));
        }

        // Check ampersand in RHS
        let mut i = 0;
        while i < len {
            if chars[i] == '&' {
                if i + 1 < len && chars[i + 1] == '(' {
                    let close = chars[i + 2..].iter().position(|&c| c == ')');
                    if close.is_none() {
                        return Err((s.to_string(), format!("Unclosed ampersand lookup '&(' in RHS expression '{}'", s)));
                    }
                }
            }
            i += 1;
        }

        Ok(())
    }

    /// Returns (line_number, byte_offset) by finding the given token in text.
    fn find_token_position(text: &str, token: &str, skip_count: usize) -> (usize, u64) {
        let mut found_count = 0;
        let mut search_from = 0;

        while let Some(pos) = text[search_from..].find(token) {
            let abs_pos = search_from + pos;
            if found_count >= skip_count {
                let line_num = 1 + text[..abs_pos].chars().filter(|&c| c == '\n').count();
                return (line_num, abs_pos as u64);
            }
            found_count += 1;
            search_from = abs_pos + token.len();
        }

        // Fallback to line 1
        (1, 0)
    }

    fn calc_offset_from_line_col(text: &str, line: usize, col: usize) -> u64 {
        let mut current_line = 1;
        let mut offset = 0;

        for (byte_idx, c) in text.char_indices() {
            if current_line == line {
                return (byte_idx + col.saturating_sub(1)) as u64;
            }
            if c == '\n' {
                current_line += 1;
            }
            offset = byte_idx;
        }

        offset as u64
    }

    /// Quick heuristic to check if a document is likely a JOLT specification.
    pub fn is_jolt_spec(text: &str) -> bool {
        let trimmed = text.trim();
        (trimmed.starts_with('[') || trimmed.starts_with('{'))
            && (trimmed.contains("\"operation\"") || trimmed.contains("'operation'"))
            && (trimmed.contains("\"shift\"")
                || trimmed.contains("\"default\"")
                || trimmed.contains("\"remove\"")
                || trimmed.contains("\"sort\"")
                || trimmed.contains("\"cardinality\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_jolt_spec() {
        let spec = r#"[
            {
                "operation": "shift",
                "spec": {
                    "rating": {
                        "primary": {
                            "value": "Rating",
                            "max": "RatingRange"
                        },
                        "*": {
                            "value": "SecondaryRatings.&1.Value",
                            "max": "SecondaryRatings.&1.Range"
                        }
                    }
                }
            },
            {
                "operation": "default",
                "spec": {
                    "Timestamp": 0
                }
            },
            {
                "operation": "sort"
            }
        ]"#;

        let cancel = Arc::new(AtomicBool::new(false));
        let res = JoltValidator::validate(std::io::Cursor::new(spec), cancel);
        match res {
            JoltValidationResult::Valid { operations_count, operations, .. } => {
                assert_eq!(operations_count, 3);
                assert_eq!(operations, vec!["shift", "default", "sort"]);
            }
            JoltValidationResult::Invalid { message, line_number, .. } => {
                panic!("Expected valid, got error on line {}: {}", line_number, message);
            }
        }
    }

    #[test]
    fn test_invalid_jolt_unknown_operation() {
        let spec = r#"[
            {
                "operation": "shifttt",
                "spec": { "a": "b" }
            }
        ]"#;

        let cancel = Arc::new(AtomicBool::new(false));
        let res = JoltValidator::validate(std::io::Cursor::new(spec), cancel);
        match res {
            JoltValidationResult::Invalid { message, .. } => {
                assert!(message.contains("Unknown JOLT operation 'shifttt'"));
            }
            _ => panic!("Expected invalid result"),
        }
    }

    #[test]
    fn test_invalid_jolt_missing_spec() {
        let spec = r#"[
            {
                "operation": "shift"
            }
        ]"#;

        let cancel = Arc::new(AtomicBool::new(false));
        let res = JoltValidator::validate(std::io::Cursor::new(spec), cancel);
        match res {
            JoltValidationResult::Invalid { message, .. } => {
                assert!(message.contains("requires a 'spec' definition"));
            }
            _ => panic!("Expected invalid result"),
        }
    }

    #[test]
    fn test_invalid_jolt_unclosed_wildcard() {
        let spec = r#"[
            {
                "operation": "shift",
                "spec": {
                    "&(1,value": "out"
                }
            }
        ]"#;

        let cancel = Arc::new(AtomicBool::new(false));
        let res = JoltValidator::validate(std::io::Cursor::new(spec), cancel);
        match res {
            JoltValidationResult::Invalid { message, .. } => {
                assert!(message.contains("Unclosed parenthesis"));
            }
            _ => panic!("Expected invalid result"),
        }
    }
}
