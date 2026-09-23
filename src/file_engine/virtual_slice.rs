#[derive(Debug, Clone, PartialEq)]
pub enum SliceSource {
    Query(String),                                // XPath or JSONPath query string
    Search(String),                               // Text or Regex search query
    Range { start_line: usize, end_line: usize }, // Line range
}

#[derive(Debug, Clone)]
pub struct VirtualSlice {
    pub label: String,
    pub source: SliceSource,
    pub matching_lines: Vec<usize>, // 1-based physical line numbers
    pub total_file_lines: usize,
}

impl VirtualSlice {
    pub fn new(label: String, source: SliceSource, matching_lines: Vec<usize>, total_file_lines: usize) -> Self {
        Self {
            label,
            source,
            matching_lines,
            total_file_lines,
        }
    }

    #[inline]
    pub fn line_count(&self) -> usize {
        self.matching_lines.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.matching_lines.is_empty()
    }

    /// Translates a 1-based virtual line number to its original 1-based physical line number
    #[inline]
    pub fn virtual_to_physical(&self, virtual_line: usize) -> Option<usize> {
        if virtual_line == 0 || virtual_line > self.matching_lines.len() {
            None
        } else {
            Some(self.matching_lines[virtual_line - 1])
        }
    }

    /// Translates an original 1-based physical line number to its 1-based virtual line number if present
    pub fn physical_to_virtual(&self, physical_line: usize) -> Option<usize> {
        self.matching_lines
            .binary_search(&physical_line)
            .ok()
            .map(|idx| idx + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_slice_mapping() {
        let lines = vec![5, 12, 45, 100, 250];
        let slice = VirtualSlice::new(
            "Errors".to_string(),
            SliceSource::Search("ERROR".to_string()),
            lines,
            1000,
        );

        assert_eq!(slice.line_count(), 5);
        assert_eq!(slice.virtual_to_physical(1), Some(5));
        assert_eq!(slice.virtual_to_physical(3), Some(45));
        assert_eq!(slice.virtual_to_physical(6), None);

        assert_eq!(slice.physical_to_virtual(45), Some(3));
        assert_eq!(slice.physical_to_virtual(50), None);
    }
}
