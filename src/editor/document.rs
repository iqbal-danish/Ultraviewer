use std::collections::HashMap;
use super::piece_table::PieceTable;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditOperation {
    ReplaceLine {
        line_number: usize,
        byte_offset: u64,
        old_text: String,
        new_text: String,
    },
    InsertText {
        offset: u64,
        text: String,
    },
    DeleteText {
        offset: u64,
        deleted_text: String,
    },
}

#[derive(Debug, Clone, Default)]
pub struct UndoStack {
    pub undo_list: Vec<EditOperation>,
    pub redo_list: Vec<EditOperation>,
    pub save_index: usize,
}

impl UndoStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_list.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_list.is_empty()
    }

    pub fn is_dirty(&self) -> bool {
        self.undo_list.len() != self.save_index
    }

    pub fn mark_saved(&mut self) {
        self.save_index = self.undo_list.len();
    }
}

/// Represents the active editing session, combining PieceTable, Undo/Redo, and line-level overrides
#[derive(Debug, Clone)]
pub struct EditorDocument {
    pub piece_table: PieceTable,
    pub undo_stack: UndoStack,
    /// Fast line-level cache for modified lines in the viewport
    pub modified_lines: HashMap<usize, String>,
}

impl EditorDocument {
    pub fn new(original_size: u64) -> Self {
        Self {
            piece_table: PieceTable::new(original_size),
            undo_stack: UndoStack::new(),
            modified_lines: HashMap::new(),
        }
    }

    /// Check whether the document has unsaved modifications
    pub fn is_dirty(&self) -> bool {
        self.undo_stack.is_dirty()
    }

    pub fn can_undo(&self) -> bool {
        self.undo_stack.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.undo_stack.can_redo()
    }

    /// Number of edits performed in the session
    pub fn edit_count(&self) -> usize {
        self.undo_stack.undo_list.len()
    }

    /// Get line content override if line has been edited
    pub fn get_line_override(&self, line_number: usize) -> Option<&str> {
        self.modified_lines.get(&line_number).map(|s| s.as_str())
    }

    /// Edit a specific line's content
    pub fn edit_line(
        &mut self,
        line_number: usize,
        byte_offset: u64,
        old_text: &str,
        new_text: &str,
    ) {
        if old_text == new_text {
            return;
        }

        let op = EditOperation::ReplaceLine {
            line_number,
            byte_offset,
            old_text: old_text.to_string(),
            new_text: new_text.to_string(),
        };

        // Apply to piece table
        self.piece_table.replace(byte_offset, old_text.len() as u64, new_text.as_bytes());

        // Update line override cache
        self.modified_lines.insert(line_number, new_text.to_string());

        // Push to undo stack, clear redo
        self.undo_stack.undo_list.push(op);
        self.undo_stack.redo_list.clear();
    }

    /// Undo the most recent edit operation
    pub fn undo(&mut self) -> bool {
        let Some(op) = self.undo_stack.undo_list.pop() else {
            return false;
        };

        match op {
            EditOperation::ReplaceLine {
                line_number,
                byte_offset,
                ref old_text,
                ref new_text,
            } => {
                // Revert in piece table
                self.piece_table.replace(byte_offset, new_text.len() as u64, old_text.as_bytes());

                // Revert in modified_lines cache
                // If there was an earlier edit for this line in undo_list, find it, else remove
                let mut prev_val = None;
                for prev_op in self.undo_stack.undo_list.iter().rev() {
                    if let EditOperation::ReplaceLine { line_number: l, new_text: prev_new, .. } = prev_op {
                        if *l == line_number {
                            prev_val = Some(prev_new.clone());
                            break;
                        }
                    }
                }

                if let Some(prev) = prev_val {
                    self.modified_lines.insert(line_number, prev);
                } else {
                    self.modified_lines.remove(&line_number);
                }
            }
            EditOperation::InsertText { offset, ref text } => {
                self.piece_table.delete(offset, text.len() as u64);
            }
            EditOperation::DeleteText { offset, ref deleted_text } => {
                self.piece_table.insert(offset, deleted_text.as_bytes());
            }
        }

        self.undo_stack.redo_list.push(op);
        true
    }

    /// Redo the most recently undone edit operation
    pub fn redo(&mut self) -> bool {
        let Some(op) = self.undo_stack.redo_list.pop() else {
            return false;
        };

        match op {
            EditOperation::ReplaceLine {
                line_number,
                byte_offset,
                ref old_text,
                ref new_text,
            } => {
                // Apply in piece table
                self.piece_table.replace(byte_offset, old_text.len() as u64, new_text.as_bytes());
                self.modified_lines.insert(line_number, new_text.clone());
            }
            EditOperation::InsertText { offset, ref text } => {
                self.piece_table.insert(offset, text.as_bytes());
            }
            EditOperation::DeleteText { offset, ref deleted_text } => {
                self.piece_table.delete(offset, deleted_text.len() as u64);
            }
        }

        self.undo_stack.undo_list.push(op);
        true
    }

    /// Mark the document clean upon successful save
    pub fn mark_saved(&mut self) {
        self.undo_stack.mark_saved();
    }
}
