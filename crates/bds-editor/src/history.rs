/// A single edit action that can be undone/redone.
#[derive(Debug, Clone)]
pub enum EditAction {
    Insert { pos: usize, text: String },
    Delete { pos: usize, text: String },
}

/// Undo/redo history with edit grouping support.
pub struct UndoHistory {
    undo_stack: Vec<EditAction>,
    redo_stack: Vec<EditAction>,
    /// Group boundary marker: index in undo_stack where last group ended.
    last_group_boundary: usize,
}

impl UndoHistory {
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            last_group_boundary: 0,
        }
    }

    /// Push an edit action onto the undo stack. Clears redo stack.
    pub fn push(&mut self, action: EditAction) {
        self.undo_stack.push(action);
        self.redo_stack.clear();
    }

    /// Mark a group boundary (e.g. after a pause in typing).
    pub fn mark_group(&mut self) {
        self.last_group_boundary = self.undo_stack.len();
    }

    /// Pop a single undo action.
    pub fn undo(&mut self) -> Option<EditAction> {
        let action = self.undo_stack.pop()?;
        self.redo_stack.push(action.clone());
        if self.last_group_boundary > self.undo_stack.len() {
            self.last_group_boundary = self.undo_stack.len();
        }
        Some(action)
    }

    /// Pop a single redo action.
    pub fn redo(&mut self) -> Option<EditAction> {
        let action = self.redo_stack.pop()?;
        self.undo_stack.push(action.clone());
        Some(action)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_undo() {
        let mut h = UndoHistory::new();
        h.push(EditAction::Insert {
            pos: 0,
            text: "hello".into(),
        });
        assert!(h.can_undo());
        let action = h.undo().unwrap();
        match action {
            EditAction::Insert { pos, text } => {
                assert_eq!(pos, 0);
                assert_eq!(text, "hello");
            }
            _ => panic!("Expected Insert"),
        }
        assert!(!h.can_undo());
    }

    #[test]
    fn undo_redo_cycle() {
        let mut h = UndoHistory::new();
        h.push(EditAction::Insert {
            pos: 0,
            text: "a".into(),
        });
        h.undo();
        assert!(h.can_redo());
        let action = h.redo().unwrap();
        match action {
            EditAction::Insert { text, .. } => assert_eq!(text, "a"),
            _ => panic!("Expected Insert"),
        }
    }

    #[test]
    fn new_push_clears_redo() {
        let mut h = UndoHistory::new();
        h.push(EditAction::Insert {
            pos: 0,
            text: "a".into(),
        });
        h.undo();
        h.push(EditAction::Insert {
            pos: 0,
            text: "b".into(),
        });
        assert!(!h.can_redo());
    }
}
