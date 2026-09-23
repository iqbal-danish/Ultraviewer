use crate::formats::FileType;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::collections::HashMap;
use std::io::Cursor;

/// Resolves exact XPath and JSONPath for a given line and cursor position.
/// Uses a streaming XML state machine that correctly respects CDATA, comments,
/// namespaces, sibling indices, and attributes without erroneous tag accumulation.
pub struct PathResolver;

impl PathResolver {
    /// Resolves the path string for the given file type, target line, byte offsets, and column.
    pub fn resolve_path(
        file_type: Option<FileType>,
        _target_line: usize,
        line_start_offset: u64,
        line_end_offset: u64,
        target_col: Option<usize>,
        target_text: &str,
        file_bytes: Option<&[u8]>,
    ) -> Option<String> {
        match file_type {
            Some(FileType::Xml) => {
                if let Some(bytes) = file_bytes {
                    Some(Self::resolve_xml_xpath_from_bytes(
                        bytes,
                        line_start_offset,
                        line_end_offset,
                        target_col,
                        target_text,
                    ))
                } else {
                    Some(Self::resolve_xml_xpath_from_line(target_text, target_col))
                }
            }
            Some(FileType::Json) => {
                if let Some(bytes) = file_bytes {
                    Some(Self::resolve_json_path_from_bytes(
                        bytes,
                        line_start_offset as usize,
                        line_end_offset as usize,
                        target_col,
                        target_text,
                    ))
                } else {
                    Some(Self::resolve_json_path_fallback(target_text))
                }
            }
            _ => None,
        }
    }

    /// Resolves exact XPath using quick-xml streaming up to the target line offset.
    /// CDATA sections (with <br>, <li>, etc.) and comments are completely ignored.
    pub fn resolve_xml_xpath_from_bytes(
        bytes: &[u8],
        line_start_offset: u64,
        line_end_offset: u64,
        target_col: Option<usize>,
        target_text: &str,
    ) -> String {
        let selected_attr = target_col.and_then(|col| Self::extract_attribute_at_col(target_text, col));

        // Limit the parse window: if the target offset is within the file, slice up to line_end_offset + 512
        let max_parse_offset = ((line_end_offset as usize) + 512).min(bytes.len());
        let parse_slice = &bytes[..max_parse_offset];

        let mut reader = Reader::from_reader(Cursor::new(parse_slice));
        reader.config_mut().expand_empty_elements = false;
        reader.config_mut().check_end_names = false;

        let mut buf = Vec::with_capacity(512);
        // stack: (tag_name, sibling_index_under_parent, is_on_target_line)
        let mut stack: Vec<(String, usize, bool)> = Vec::new();
        // Sibling count map for each depth level
        let mut sibling_maps: Vec<HashMap<String, usize>> = vec![HashMap::new()];

        loop {
            let event_start_pos = reader.buffer_position();
            if event_start_pos > line_end_offset {
                break;
            }

            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let tag_pos = reader.buffer_position();
                    let is_on_target = tag_pos >= line_start_offset && tag_pos <= (line_end_offset + 2);

                    let parent_map = sibling_maps.last_mut().unwrap();
                    let count = parent_map.entry(tag_name.clone()).or_insert(0);
                    *count += 1;
                    let current_index = *count;

                    stack.push((tag_name, current_index, is_on_target));
                    sibling_maps.push(HashMap::new());

                    if tag_pos > line_end_offset {
                        break;
                    }
                }
                Ok(Event::End(_)) => {
                    let tag_pos = reader.buffer_position();
                    // If the element on top of the stack was opened ON the target line,
                    // keep it as the active element for this line even if it closes on the same line.
                    if let Some((_, _, opened_on_target)) = stack.last() {
                        if !*opened_on_target || tag_pos > line_end_offset {
                            stack.pop();
                            if sibling_maps.len() > 1 {
                                sibling_maps.pop();
                            }
                        }
                    }
                    if tag_pos > line_end_offset {
                        break;
                    }
                }
                Ok(Event::Empty(e)) => {
                    let tag_name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
                    let tag_pos = reader.buffer_position();
                    let is_on_target = tag_pos >= line_start_offset && tag_pos <= (line_end_offset + 2);

                    let parent_map = sibling_maps.last_mut().unwrap();
                    let count = parent_map.entry(tag_name.clone()).or_insert(0);
                    *count += 1;
                    let current_index = *count;

                    if is_on_target {
                        stack.push((tag_name, current_index, true));
                    }

                    if tag_pos > line_end_offset {
                        break;
                    }
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
            buf.clear();
        }

        if stack.is_empty() {
            if let Some(attr) = selected_attr {
                return format!("/@{}", attr);
            }
            return "/".to_string();
        }

        let mut xpath = String::new();
        for (tag, idx, _) in &stack {
            xpath.push('/');
            xpath.push_str(tag);
            if *idx > 1 {
                xpath.push_str(&format!("[{}]", idx));
            }
        }

        if let Some(attr) = selected_attr {
            xpath.push_str(&format!("/@{}", attr));
        }

        xpath
    }

    /// Fast fallback when raw file bytes are not provided
    pub fn resolve_xml_xpath_from_line(line_text: &str, target_col: Option<usize>) -> String {
        let selected_attr = target_col.and_then(|col| Self::extract_attribute_at_col(line_text, col));
        let trimmed = line_text.trim();
        if let Some(start) = trimmed.find('<') {
            if let Some(end) = trimmed[start..].find(|c: char| c.is_whitespace() || c == '>' || c == '/') {
                let tag = trimmed[start + 1..start + end].trim_start_matches('/');
                if !tag.is_empty() && !tag.starts_with('!') && !tag.starts_with('?') {
                    if let Some(attr) = selected_attr {
                        return format!("/{}/@{}", tag, attr);
                    }
                    return format!("/{}", tag);
                }
            }
        }
        if let Some(attr) = selected_attr {
            return format!("/@{}", attr);
        }
        "/".to_string()
    }

    /// Resolves exact JSONPath by streaming JSON structure and tracking object/array nesting
    pub fn resolve_json_path_from_bytes(
        bytes: &[u8],
        line_start_offset: usize,
        line_end_offset: usize,
        _target_col: Option<usize>,
        target_text: &str,
    ) -> String {
        let max_parse_offset = (line_end_offset + 1024).min(bytes.len());
        let parse_slice = &bytes[..max_parse_offset];

        #[derive(Debug, Clone)]
        enum Frame {
            Object { key: Option<String> },
            Array { key: Option<String>, index: usize },
        }

        let mut stack: Vec<Frame> = Vec::new();
        let mut last_key: Option<String> = None;
        let mut current_string = String::new();
        let mut in_string = false;
        let mut escape = false;
        let mut is_after_key = false;

        let mut idx = 0;
        let len = parse_slice.len();

        while idx < len {
            if idx >= line_end_offset {
                break;
            }

            let b = parse_slice[idx];

            if in_string {
                if escape {
                    escape = false;
                    current_string.push(b as char);
                } else if b == b'\\' {
                    escape = true;
                } else if b == b'"' {
                    in_string = false;
                    // Check if this string was a key (inside an object before colon)
                    if !is_after_key {
                        last_key = Some(current_string.clone());
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
                    escape = false;
                    current_string.clear();
                }
                b':' => {
                    is_after_key = true;
                }
                b'{' => {
                    is_after_key = false;
                    let key = last_key.take();
                    stack.push(Frame::Object { key });
                }
                b'}' => {
                    is_after_key = false;
                    last_key = None;
                    if idx < line_start_offset {
                        stack.pop();
                    }
                }
                b'[' => {
                    is_after_key = false;
                    let key = last_key.take();
                    stack.push(Frame::Array { key, index: 0 });
                }
                b']' => {
                    is_after_key = false;
                    last_key = None;
                    if idx < line_start_offset {
                        stack.pop();
                    }
                }
                b',' => {
                    is_after_key = false;
                    last_key = None;
                    if let Some(Frame::Array { index, .. }) = stack.last_mut() {
                        *index += 1;
                    }
                }
                _ => {}
            }

            idx += 1;
        }

        // Build path from stack
        let mut path = "$".to_string();
        for frame in &stack {
            match frame {
                Frame::Object { key: Some(k) } => {
                    path.push('.');
                    path.push_str(k);
                }
                Frame::Array { key, index } => {
                    if let Some(k) = key {
                        path.push('.');
                        path.push_str(k);
                    }
                    path.push_str(&format!("[{}]", index));
                }
                _ => {}
            }
        }

        // Check if target line itself has a key (e.g. `"action": "D"`)
        if let Some(key) = Self::extract_json_key(target_text) {
            path.push('.');
            path.push_str(&key);
        }

        path
    }

    pub fn resolve_json_path_fallback(target_text: &str) -> String {
        if let Some(key) = Self::extract_json_key(target_text) {
            format!("$.{}", key)
        } else {
            "$".to_string()
        }
    }

    fn extract_json_key(line: &str) -> Option<String> {
        let trimmed = line.trim();
        if let Some(colon_pos) = trimmed.find(':') {
            let before_colon = trimmed[..colon_pos].trim();
            if before_colon.starts_with('"') && before_colon.ends_with('"') && before_colon.len() >= 2 {
                return Some(before_colon[1..before_colon.len() - 1].to_string());
            }
        }
        None
    }

    /// Extracts an attribute name if the cursor column is inside `name="value"` or `name='value'`
    pub fn extract_attribute_at_col(line: &str, col: usize) -> Option<String> {
        let bytes = line.as_bytes();
        let len = bytes.len();
        if col >= len {
            return None;
        }

        let mut idx = 0;
        while idx < len {
            if bytes[idx] == b'<' {
                idx += 1;
                // Skip tag name
                while idx < len && !bytes[idx].is_ascii_whitespace() && bytes[idx] != b'>' {
                    idx += 1;
                }
                // Parse attributes
                while idx < len && bytes[idx] != b'>' {
                    while idx < len && bytes[idx].is_ascii_whitespace() {
                        idx += 1;
                    }
                    if idx >= len || bytes[idx] == b'>' || bytes[idx] == b'/' {
                        break;
                    }

                    let attr_start = idx;
                    while idx < len && bytes[idx] != b'=' && !bytes[idx].is_ascii_whitespace() && bytes[idx] != b'>' {
                        idx += 1;
                    }
                    let attr_name = line[attr_start..idx].to_string();

                    while idx < len && bytes[idx].is_ascii_whitespace() {
                        idx += 1;
                    }

                    if idx < len && bytes[idx] == b'=' {
                        idx += 1;
                        while idx < len && bytes[idx].is_ascii_whitespace() {
                            idx += 1;
                        }
                        if idx < len && (bytes[idx] == b'"' || bytes[idx] == b'\'') {
                            let quote = bytes[idx];
                            idx += 1;
                            while idx < len && bytes[idx] != quote {
                                idx += 1;
                            }
                            if idx < len && bytes[idx] == quote {
                                idx += 1;
                            }
                            let val_end = idx;

                            if col >= attr_start && col <= val_end {
                                return Some(attr_name);
                            }
                        }
                    }
                }
            } else {
                idx += 1;
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xml_with_cdata_and_html_tags() {
        // Reproduces the exact user bug where CDATA containing <br> and <li> was corrupting the path
        let xml = r#"<source>
  <job>
    <title>DevOps Engineer</title>
    <description><![CDATA[Join our team!<br><br><br><ul><li>item 1</li><li>item 2</li></ul><br>]]></description>
    <cpc><![CDATA[0.43]]></cpc>
    <education><![CDATA[]]></education>
  </job>
  <job>
    <title>Rust Developer</title>
    <cpc><![CDATA[0.99]]></cpc>
  </job>
</source>"#;

        let bytes = xml.as_bytes();
        // Offset of `<cpc><![CDATA[0.43]]></cpc>` in the first job
        let cpc_line = "    <cpc><![CDATA[0.43]]></cpc>";
        let start_offset = xml.find(cpc_line).unwrap() as u64;
        let end_offset = start_offset + cpc_line.len() as u64;

        let xpath = PathResolver::resolve_xml_xpath_from_bytes(
            bytes,
            start_offset,
            end_offset,
            None,
            cpc_line,
        );

        // MUST be /source/job/cpc, NOT accumulating br, li, or extra job/description tags!
        assert_eq!(xpath, "/source/job/cpc");
    }

    #[test]
    fn test_xml_second_sibling_job() {
        let xml = r#"<source>
  <job>
    <title>DevOps Engineer</title>
    <cpc><![CDATA[0.43]]></cpc>
  </job>
  <job>
    <title>Rust Developer</title>
    <cpc><![CDATA[0.99]]></cpc>
  </job>
</source>"#;

        let bytes = xml.as_bytes();
        let second_cpc = "    <cpc><![CDATA[0.99]]></cpc>";
        let start_offset = xml.find(second_cpc).unwrap() as u64;
        let end_offset = start_offset + second_cpc.len() as u64;

        let xpath = PathResolver::resolve_xml_xpath_from_bytes(
            bytes,
            start_offset,
            end_offset,
            None,
            second_cpc,
        );

        assert_eq!(xpath, "/source/job[2]/cpc");
    }

    #[test]
    fn test_xml_attribute_selection() {
        let xml = r#"<source>
  <job id="job101" location="remote">
    <title>Rust Developer</title>
  </job>
</source>"#;

        let bytes = xml.as_bytes();
        let job_line = "  <job id=\"job101\" location=\"remote\">";
        let start_offset = xml.find(job_line).unwrap() as u64;
        let end_offset = start_offset + job_line.len() as u64;

        // Cursor at column 10 (inside id="job101")
        let xpath = PathResolver::resolve_xml_xpath_from_bytes(
            bytes,
            start_offset,
            end_offset,
            Some(10),
            job_line,
        );

        assert_eq!(xpath, "/source/job/@id");
    }

    #[test]
    fn test_json_sibling_keys_not_accumulated() {
        let json = r#"{
  "jobtitle": "Software Engineer",
  "jobbody": "Write code",
  "postingid": 12345,
  "id": "1101523",
  "provider": "Indeed",
  "code": "US",
  "name": "Tech Corp",
  "url": "https://example.com",
  "source": "api",
  "attributes": [
    { "id": 1, "name": "Rust" },
    { "id": 2, "name": "egui" }
  ],
  "action": "D",
  "activedate": "2025-04-03T12:04:16.4753883Z"
}"#;

        let bytes = json.as_bytes();
        let action_line = "  \"action\": \"D\",";
        let start_offset = json.find(action_line).unwrap();
        let end_offset = start_offset + action_line.len();

        let path = PathResolver::resolve_json_path_from_bytes(
            bytes,
            start_offset,
            end_offset,
            None,
            action_line,
        );

        assert_eq!(path, "$.action");
    }
}
