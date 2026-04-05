use ropey::Rope;

use crate::history::{EditAction, UndoHistory};

/// Selection anchor and head (cursor). Both are (line, col) positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor_line: usize,
    pub anchor_col: usize,
    pub head_line: usize,
    pub head_col: usize,
}

impl Selection {
    /// Returns (start, end) as (line, col) pairs in document order.
    pub fn ordered(&self) -> ((usize, usize), (usize, usize)) {
        let a = (self.anchor_line, self.anchor_col);
        let h = (self.head_line, self.head_col);
        if a <= h { (a, h) } else { (h, a) }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor_line == self.head_line && self.anchor_col == self.head_col
    }
}

/// Rope-based text buffer with edit operations, cursor, selection, and undo/redo.
pub struct EditorBuffer {
    rope: Rope,
    cursor_line: usize,
    cursor_col: usize,
    /// Vertical scroll offset in lines.
    scroll_offset: usize,
    /// Selection state (None = no selection).
    selection: Option<Selection>,
    /// Undo/redo history.
    history: UndoHistory,
    /// Whether the buffer has been modified since last save/checkpoint.
    dirty: bool,
    /// Soft wrap enabled.
    soft_wrap: bool,
}

impl EditorBuffer {
    pub fn new(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            cursor_line: 0,
            cursor_col: 0,
            scroll_offset: 0,
            selection: None,
            history: UndoHistory::new(),
            dirty: false,
            soft_wrap: false,
        }
    }

    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn line(&self, idx: usize) -> Option<ropey::RopeSlice<'_>> {
        if idx < self.rope.len_lines() {
            Some(self.rope.line(idx))
        } else {
            None
        }
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_line, self.cursor_col)
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn selection(&self) -> Option<&Selection> {
        self.selection.as_ref()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }

    pub fn soft_wrap(&self) -> bool {
        self.soft_wrap
    }

    pub fn set_soft_wrap(&mut self, wrap: bool) {
        self.soft_wrap = wrap;
    }

    pub fn set_cursor(&mut self, line: usize, col: usize) {
        self.cursor_line = line.min(self.line_count().saturating_sub(1));
        let line_len = self.current_line_len();
        self.cursor_col = col.min(line_len);
    }

    /// Clear the current selection.
    pub fn clear_selection(&mut self) {
        self.selection = None;
    }

    /// Start or extend a selection from the current cursor to the new cursor position.
    fn extend_selection(&mut self, new_line: usize, new_col: usize) {
        let sel = self.selection.get_or_insert(Selection {
            anchor_line: self.cursor_line,
            anchor_col: self.cursor_col,
            head_line: self.cursor_line,
            head_col: self.cursor_col,
        });
        sel.head_line = new_line;
        sel.head_col = new_col;
    }

    /// Set selection explicitly (for double-click word select, etc).
    pub fn set_selection(&mut self, anchor_line: usize, anchor_col: usize, head_line: usize, head_col: usize) {
        self.selection = Some(Selection {
            anchor_line,
            anchor_col,
            head_line,
            head_col,
        });
    }

    /// Get the selected text, or empty string if no selection.
    pub fn selected_text(&self) -> String {
        match &self.selection {
            None => String::new(),
            Some(sel) if sel.is_empty() => String::new(),
            Some(sel) => {
                let (start, end) = sel.ordered();
                let start_idx = self.pos_to_char_idx(start.0, start.1);
                let end_idx = self.pos_to_char_idx(end.0, end.1);
                self.rope.slice(start_idx..end_idx).to_string()
            }
        }
    }

    /// Delete the selected text and place cursor at start of selection.
    /// Returns the deleted text.
    pub fn delete_selection(&mut self) -> String {
        let sel = match self.selection.take() {
            Some(s) if !s.is_empty() => s,
            _ => return String::new(),
        };
        let (start, end) = sel.ordered();
        let start_idx = self.pos_to_char_idx(start.0, start.1);
        let end_idx = self.pos_to_char_idx(end.0, end.1);
        let deleted: String = self.rope.slice(start_idx..end_idx).to_string();

        self.history.push(EditAction::Delete {
            pos: start_idx,
            text: deleted.clone(),
        });

        self.rope.remove(start_idx..end_idx);
        self.cursor_line = start.0;
        self.cursor_col = start.1;
        self.dirty = true;
        deleted
    }

    /// Insert text at the current cursor position. Deletes selection first if any.
    pub fn insert(&mut self, text: &str) {
        if self.selection.is_some() && self.selection.as_ref().map_or(false, |s| !s.is_empty()) {
            self.delete_selection();
        }
        let char_idx = self.cursor_char_idx();
        self.rope.insert(char_idx, text);
        self.history.push(EditAction::Insert {
            pos: char_idx,
            text: text.to_string(),
        });
        for c in text.chars() {
            if c == '\n' {
                self.cursor_line += 1;
                self.cursor_col = 0;
            } else {
                self.cursor_col += 1;
            }
        }
        self.dirty = true;
    }

    /// Delete one character before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.selection.as_ref().map_or(false, |s| !s.is_empty()) {
            self.delete_selection();
            return;
        }
        if self.cursor_col > 0 {
            let char_idx = self.cursor_char_idx();
            let deleted: String = self.rope.slice(char_idx - 1..char_idx).to_string();
            self.history.push(EditAction::Delete {
                pos: char_idx - 1,
                text: deleted,
            });
            self.rope.remove(char_idx - 1..char_idx);
            self.cursor_col -= 1;
            self.dirty = true;
        } else if self.cursor_line > 0 {
            let prev_line_len = self.line_len(self.cursor_line - 1);
            let char_idx = self.cursor_char_idx();
            let deleted: String = self.rope.slice(char_idx - 1..char_idx).to_string();
            self.history.push(EditAction::Delete {
                pos: char_idx - 1,
                text: deleted,
            });
            self.rope.remove(char_idx - 1..char_idx);
            self.cursor_line -= 1;
            self.cursor_col = prev_line_len;
            self.dirty = true;
        }
    }

    /// Delete one character after the cursor (delete key).
    pub fn delete_forward(&mut self) {
        if self.selection.as_ref().map_or(false, |s| !s.is_empty()) {
            self.delete_selection();
            return;
        }
        let char_idx = self.cursor_char_idx();
        if char_idx < self.rope.len_chars() {
            let deleted: String = self.rope.slice(char_idx..char_idx + 1).to_string();
            self.history.push(EditAction::Delete {
                pos: char_idx,
                text: deleted,
            });
            self.rope.remove(char_idx..char_idx + 1);
            self.dirty = true;
        }
    }

    // ── Cursor movement ──────────────────────────────────────

    pub fn move_up(&mut self) {
        self.clear_selection();
        self.move_up_inner();
    }

    pub fn move_down(&mut self) {
        self.clear_selection();
        self.move_down_inner();
    }

    pub fn move_left(&mut self) {
        self.clear_selection();
        self.move_left_inner();
    }

    pub fn move_right(&mut self) {
        self.clear_selection();
        self.move_right_inner();
    }

    pub fn move_home(&mut self) {
        self.clear_selection();
        self.cursor_col = 0;
    }

    pub fn move_end(&mut self) {
        self.clear_selection();
        self.cursor_col = self.current_line_len();
    }

    /// Move cursor to start of previous word.
    pub fn move_word_left(&mut self) {
        self.clear_selection();
        self.move_word_left_inner();
    }

    /// Move cursor to end of next word.
    pub fn move_word_right(&mut self) {
        self.clear_selection();
        self.move_word_right_inner();
    }

    /// Move cursor up by a page (viewport height).
    pub fn move_page_up(&mut self, page_lines: usize) {
        self.clear_selection();
        let target = self.cursor_line.saturating_sub(page_lines);
        self.cursor_line = target;
        let line_len = self.current_line_len();
        self.cursor_col = self.cursor_col.min(line_len);
    }

    /// Move cursor down by a page (viewport height).
    pub fn move_page_down(&mut self, page_lines: usize) {
        self.clear_selection();
        let max_line = self.line_count().saturating_sub(1);
        let target = (self.cursor_line + page_lines).min(max_line);
        self.cursor_line = target;
        let line_len = self.current_line_len();
        self.cursor_col = self.cursor_col.min(line_len);
    }

    // ── Selection movement ───────────────────────────────────

    pub fn select_up(&mut self) {
        let (new_line, new_col) = self.calc_up();
        self.extend_selection(new_line, new_col);
        self.cursor_line = new_line;
        self.cursor_col = new_col;
    }

    pub fn select_down(&mut self) {
        let (new_line, new_col) = self.calc_down();
        self.extend_selection(new_line, new_col);
        self.cursor_line = new_line;
        self.cursor_col = new_col;
    }

    pub fn select_left(&mut self) {
        let (new_line, new_col) = self.calc_left();
        self.extend_selection(new_line, new_col);
        self.cursor_line = new_line;
        self.cursor_col = new_col;
    }

    pub fn select_right(&mut self) {
        let (new_line, new_col) = self.calc_right();
        self.extend_selection(new_line, new_col);
        self.cursor_line = new_line;
        self.cursor_col = new_col;
    }

    pub fn select_home(&mut self) {
        self.extend_selection(self.cursor_line, 0);
        self.cursor_col = 0;
    }

    pub fn select_end(&mut self) {
        let end = self.current_line_len();
        self.extend_selection(self.cursor_line, end);
        self.cursor_col = end;
    }

    pub fn select_word_left(&mut self) {
        let (new_line, new_col) = self.calc_word_left();
        self.extend_selection(new_line, new_col);
        self.cursor_line = new_line;
        self.cursor_col = new_col;
    }

    pub fn select_word_right(&mut self) {
        let (new_line, new_col) = self.calc_word_right();
        self.extend_selection(new_line, new_col);
        self.cursor_line = new_line;
        self.cursor_col = new_col;
    }

    pub fn select_page_up(&mut self, page_lines: usize) {
        let target = self.cursor_line.saturating_sub(page_lines);
        let col = self.cursor_col.min(self.line_len(target));
        self.extend_selection(target, col);
        self.cursor_line = target;
        self.cursor_col = col;
    }

    pub fn select_page_down(&mut self, page_lines: usize) {
        let max_line = self.line_count().saturating_sub(1);
        let target = (self.cursor_line + page_lines).min(max_line);
        let col = self.cursor_col.min(self.line_len(target));
        self.extend_selection(target, col);
        self.cursor_line = target;
        self.cursor_col = col;
    }

    /// Select all text.
    pub fn select_all(&mut self) {
        let last_line = self.line_count().saturating_sub(1);
        let last_col = self.line_len(last_line);
        self.selection = Some(Selection {
            anchor_line: 0,
            anchor_col: 0,
            head_line: last_line,
            head_col: last_col,
        });
        self.cursor_line = last_line;
        self.cursor_col = last_col;
    }

    /// Select the word at the given position (double-click).
    pub fn select_word_at(&mut self, line: usize, col: usize) {
        self.set_cursor(line, col);
        let line_text = match self.line(self.cursor_line) {
            Some(l) => {
                let s: String = l.chars().collect();
                s.trim_end_matches('\n').to_string()
            }
            None => return,
        };
        if line_text.is_empty() {
            return;
        }
        let col = self.cursor_col.min(line_text.len());
        let chars: Vec<char> = line_text.chars().collect();

        // Find word boundaries
        let mut start = col;
        while start > 0 && is_word_char(chars[start - 1]) {
            start -= 1;
        }
        let mut end = col;
        while end < chars.len() && is_word_char(chars[end]) {
            end += 1;
        }

        if start == end && col < chars.len() {
            // Click on non-word char: select just that character
            end = col + 1;
            start = col;
        }

        self.set_selection(self.cursor_line, start, self.cursor_line, end);
        self.cursor_col = end;
    }

    // ── Undo/Redo ────────────────────────────────────────────

    pub fn undo(&mut self) {
        if let Some(action) = self.history.undo() {
            match action {
                EditAction::Insert { pos, text } => {
                    let end = pos + text.chars().count();
                    self.rope.remove(pos..end);
                    let (line, col) = self.char_idx_to_pos(pos);
                    self.cursor_line = line;
                    self.cursor_col = col;
                }
                EditAction::Delete { pos, text } => {
                    self.rope.insert(pos, &text);
                    let end_pos = pos + text.chars().count();
                    let (line, col) = self.char_idx_to_pos(end_pos);
                    self.cursor_line = line;
                    self.cursor_col = col;
                }
            }
            self.clear_selection();
            self.dirty = true;
        }
    }

    pub fn redo(&mut self) {
        if let Some(action) = self.history.redo() {
            match action {
                EditAction::Insert { pos, text } => {
                    self.rope.insert(pos, &text);
                    let end_pos = pos + text.chars().count();
                    let (line, col) = self.char_idx_to_pos(end_pos);
                    self.cursor_line = line;
                    self.cursor_col = col;
                }
                EditAction::Delete { pos, text } => {
                    let end = pos + text.chars().count();
                    self.rope.remove(pos..end);
                    let (line, col) = self.char_idx_to_pos(pos);
                    self.cursor_line = line;
                    self.cursor_col = col;
                }
            }
            self.clear_selection();
            self.dirty = true;
        }
    }

    /// Mark an undo group boundary (e.g. after each keystroke pause).
    pub fn mark_undo_group(&mut self) {
        self.history.mark_group();
    }

    // ── Scrolling ────────────────────────────────────────────

    pub fn ensure_cursor_visible(&mut self, visible_lines: usize) {
        if visible_lines == 0 {
            return;
        }
        if self.cursor_line < self.scroll_offset {
            self.scroll_offset = self.cursor_line;
        } else if self.cursor_line >= self.scroll_offset + visible_lines {
            self.scroll_offset = self.cursor_line - visible_lines + 1;
        }
    }

    /// Ensure cursor is visible using visual line index (for word-wrap aware scrolling).
    pub fn ensure_visual_line_visible(&mut self, visual_line: usize, visible_lines: usize, max_visual: usize) {
        if visible_lines == 0 {
            return;
        }
        if visual_line < self.scroll_offset {
            self.scroll_offset = visual_line;
        } else if visual_line >= self.scroll_offset + visible_lines {
            self.scroll_offset = (visual_line - visible_lines + 1).min(max_visual);
        }
    }

    pub fn scroll_by(&mut self, delta: isize) {
        self.scroll_by_clamped(delta, self.line_count().saturating_sub(1));
    }

    /// Scroll by `delta` visual lines, clamped to `max_visual`.
    pub fn scroll_by_clamped(&mut self, delta: isize, max_visual: usize) {
        if delta < 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub((-delta) as usize);
        } else {
            self.scroll_offset = (self.scroll_offset + delta as usize).min(max_visual);
        }
    }

    /// Set scroll offset directly, clamped to max.
    pub fn set_scroll(&mut self, offset: usize, max_visual: usize) {
        self.scroll_offset = offset.min(max_visual);
    }

    // ── Internal helpers ─────────────────────────────────────

    fn cursor_char_idx(&self) -> usize {
        self.pos_to_char_idx(self.cursor_line, self.cursor_col)
    }

    fn pos_to_char_idx(&self, line: usize, col: usize) -> usize {
        if line >= self.rope.len_lines() {
            self.rope.len_chars()
        } else {
            let line_start = self.rope.line_to_char(line);
            let line_len = self.line_len(line);
            line_start + col.min(line_len)
        }
    }

    fn char_idx_to_pos(&self, idx: usize) -> (usize, usize) {
        let idx = idx.min(self.rope.len_chars());
        let line = self.rope.char_to_line(idx);
        let line_start = self.rope.line_to_char(line);
        let col = idx - line_start;
        (line, col)
    }

    fn current_line_len(&self) -> usize {
        self.line_len(self.cursor_line)
    }

    fn line_len(&self, line: usize) -> usize {
        if line < self.rope.len_lines() {
            let l = self.rope.line(line);
            let len = l.len_chars();
            if len > 0 && l.char(len - 1) == '\n' {
                len - 1
            } else {
                len
            }
        } else {
            0
        }
    }

    // ── Movement calculation helpers (no selection changes) ──

    fn move_up_inner(&mut self) {
        let (line, col) = self.calc_up();
        self.cursor_line = line;
        self.cursor_col = col;
    }

    fn move_down_inner(&mut self) {
        let (line, col) = self.calc_down();
        self.cursor_line = line;
        self.cursor_col = col;
    }

    fn move_left_inner(&mut self) {
        let (line, col) = self.calc_left();
        self.cursor_line = line;
        self.cursor_col = col;
    }

    fn move_right_inner(&mut self) {
        let (line, col) = self.calc_right();
        self.cursor_line = line;
        self.cursor_col = col;
    }

    fn move_word_left_inner(&mut self) {
        let (line, col) = self.calc_word_left();
        self.cursor_line = line;
        self.cursor_col = col;
    }

    fn move_word_right_inner(&mut self) {
        let (line, col) = self.calc_word_right();
        self.cursor_line = line;
        self.cursor_col = col;
    }

    fn calc_up(&self) -> (usize, usize) {
        if self.cursor_line > 0 {
            let new_line = self.cursor_line - 1;
            let new_col = self.cursor_col.min(self.line_len(new_line));
            (new_line, new_col)
        } else {
            (self.cursor_line, self.cursor_col)
        }
    }

    fn calc_down(&self) -> (usize, usize) {
        if self.cursor_line + 1 < self.line_count() {
            let new_line = self.cursor_line + 1;
            let new_col = self.cursor_col.min(self.line_len(new_line));
            (new_line, new_col)
        } else {
            (self.cursor_line, self.cursor_col)
        }
    }

    fn calc_left(&self) -> (usize, usize) {
        if self.cursor_col > 0 {
            (self.cursor_line, self.cursor_col - 1)
        } else if self.cursor_line > 0 {
            let new_line = self.cursor_line - 1;
            (new_line, self.line_len(new_line))
        } else {
            (0, 0)
        }
    }

    fn calc_right(&self) -> (usize, usize) {
        let line_len = self.current_line_len();
        if self.cursor_col < line_len {
            (self.cursor_line, self.cursor_col + 1)
        } else if self.cursor_line + 1 < self.line_count() {
            (self.cursor_line + 1, 0)
        } else {
            (self.cursor_line, self.cursor_col)
        }
    }

    fn calc_word_left(&self) -> (usize, usize) {
        if self.cursor_col == 0 && self.cursor_line == 0 {
            return (0, 0);
        }
        if self.cursor_col == 0 {
            let new_line = self.cursor_line - 1;
            return (new_line, self.line_len(new_line));
        }
        let line_text = match self.line(self.cursor_line) {
            Some(l) => {
                let s: String = l.chars().collect();
                s.trim_end_matches('\n').to_string()
            }
            None => return (self.cursor_line, 0),
        };
        let chars: Vec<char> = line_text.chars().collect();
        let mut col = self.cursor_col.min(chars.len());
        // Skip spaces
        while col > 0 && !is_word_char(chars[col - 1]) {
            col -= 1;
        }
        // Skip word chars
        while col > 0 && is_word_char(chars[col - 1]) {
            col -= 1;
        }
        (self.cursor_line, col)
    }

    fn calc_word_right(&self) -> (usize, usize) {
        let line_len = self.current_line_len();
        if self.cursor_col >= line_len {
            if self.cursor_line + 1 < self.line_count() {
                return (self.cursor_line + 1, 0);
            }
            return (self.cursor_line, self.cursor_col);
        }
        let line_text = match self.line(self.cursor_line) {
            Some(l) => {
                let s: String = l.chars().collect();
                s.trim_end_matches('\n').to_string()
            }
            None => return (self.cursor_line, self.cursor_col),
        };
        let chars: Vec<char> = line_text.chars().collect();
        let mut col = self.cursor_col;
        // Skip word chars
        while col < chars.len() && is_word_char(chars[col]) {
            col += 1;
        }
        // Skip spaces
        while col < chars.len() && !is_word_char(chars[col]) {
            col += 1;
        }
        (self.cursor_line, col)
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_buffer() {
        let buf = EditorBuffer::new("hello\nworld");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.cursor(), (0, 0));
    }

    #[test]
    fn insert_text() {
        let mut buf = EditorBuffer::new("hello");
        buf.set_cursor(0, 5);
        buf.insert(" world");
        assert_eq!(buf.text(), "hello world");
        assert_eq!(buf.cursor(), (0, 11));
    }

    #[test]
    fn backspace() {
        let mut buf = EditorBuffer::new("hello");
        buf.set_cursor(0, 5);
        buf.backspace();
        assert_eq!(buf.text(), "hell");
        assert_eq!(buf.cursor(), (0, 4));
    }

    #[test]
    fn insert_newline() {
        let mut buf = EditorBuffer::new("hello");
        buf.set_cursor(0, 5);
        buf.insert("\n");
        assert_eq!(buf.line_count(), 2);
        assert_eq!(buf.cursor(), (1, 0));
    }

    #[test]
    fn backspace_at_line_start_joins_lines() {
        let mut buf = EditorBuffer::new("hello\nworld");
        buf.set_cursor(1, 0);
        buf.backspace();
        assert_eq!(buf.text(), "helloworld");
        assert_eq!(buf.cursor(), (0, 5));
    }

    #[test]
    fn delete_forward() {
        let mut buf = EditorBuffer::new("hello");
        buf.set_cursor(0, 0);
        buf.delete_forward();
        assert_eq!(buf.text(), "ello");
        assert_eq!(buf.cursor(), (0, 0));
    }

    #[test]
    fn delete_forward_at_end_is_noop() {
        let mut buf = EditorBuffer::new("hi");
        buf.set_cursor(0, 2);
        buf.delete_forward();
        assert_eq!(buf.text(), "hi");
    }

    #[test]
    fn delete_forward_joins_lines() {
        let mut buf = EditorBuffer::new("hello\nworld");
        buf.set_cursor(0, 5);
        buf.delete_forward();
        assert_eq!(buf.text(), "helloworld");
    }

    #[test]
    fn move_up() {
        let mut buf = EditorBuffer::new("abc\ndef\nghi");
        buf.set_cursor(2, 1);
        buf.move_up();
        assert_eq!(buf.cursor(), (1, 1));
        buf.move_up();
        assert_eq!(buf.cursor(), (0, 1));
        buf.move_up();
        assert_eq!(buf.cursor(), (0, 1));
    }

    #[test]
    fn move_down() {
        let mut buf = EditorBuffer::new("abc\ndef\nghi");
        buf.set_cursor(0, 1);
        buf.move_down();
        assert_eq!(buf.cursor(), (1, 1));
        buf.move_down();
        assert_eq!(buf.cursor(), (2, 1));
        buf.move_down();
        assert_eq!(buf.cursor(), (2, 1));
    }

    #[test]
    fn move_up_clamps_col_to_shorter_line() {
        let mut buf = EditorBuffer::new("long line\nhi");
        buf.set_cursor(0, 9);
        buf.move_down();
        assert_eq!(buf.cursor(), (1, 2));
    }

    #[test]
    fn move_left_right() {
        let mut buf = EditorBuffer::new("abc");
        buf.set_cursor(0, 1);
        buf.move_right();
        assert_eq!(buf.cursor(), (0, 2));
        buf.move_left();
        assert_eq!(buf.cursor(), (0, 1));
    }

    #[test]
    fn move_left_wraps_to_previous_line() {
        let mut buf = EditorBuffer::new("abc\ndef");
        buf.set_cursor(1, 0);
        buf.move_left();
        assert_eq!(buf.cursor(), (0, 3));
    }

    #[test]
    fn move_right_wraps_to_next_line() {
        let mut buf = EditorBuffer::new("abc\ndef");
        buf.set_cursor(0, 3);
        buf.move_right();
        assert_eq!(buf.cursor(), (1, 0));
    }

    #[test]
    fn move_home_end() {
        let mut buf = EditorBuffer::new("hello world");
        buf.set_cursor(0, 5);
        buf.move_home();
        assert_eq!(buf.cursor(), (0, 0));
        buf.move_end();
        assert_eq!(buf.cursor(), (0, 11));
    }

    #[test]
    fn word_movement() {
        let mut buf = EditorBuffer::new("hello world  test");
        buf.set_cursor(0, 0);
        buf.move_word_right();
        assert_eq!(buf.cursor(), (0, 6)); // after "hello "
        buf.move_word_right();
        assert_eq!(buf.cursor(), (0, 13)); // after "world  "
        buf.move_word_left();
        assert_eq!(buf.cursor(), (0, 6)); // back to "world"
        buf.move_word_left();
        assert_eq!(buf.cursor(), (0, 0)); // back to start
    }

    #[test]
    fn page_movement() {
        let mut buf = EditorBuffer::new("a\nb\nc\nd\ne\nf\ng\nh\ni\nj");
        buf.set_cursor(0, 0);
        buf.move_page_down(5);
        assert_eq!(buf.cursor(), (5, 0));
        buf.move_page_up(3);
        assert_eq!(buf.cursor(), (2, 0));
    }

    #[test]
    fn selection_shift_right() {
        let mut buf = EditorBuffer::new("hello");
        buf.set_cursor(0, 0);
        buf.select_right();
        buf.select_right();
        buf.select_right();
        assert_eq!(buf.selected_text(), "hel");
    }

    #[test]
    fn selection_delete() {
        let mut buf = EditorBuffer::new("hello world");
        buf.set_cursor(0, 0);
        buf.select_right();
        buf.select_right();
        buf.select_right();
        buf.select_right();
        buf.select_right();
        let deleted = buf.delete_selection();
        assert_eq!(deleted, "hello");
        assert_eq!(buf.text(), " world");
    }

    #[test]
    fn select_word_at() {
        let mut buf = EditorBuffer::new("hello world");
        buf.select_word_at(0, 3); // inside "hello"
        assert_eq!(buf.selected_text(), "hello");
    }

    #[test]
    fn select_all() {
        let mut buf = EditorBuffer::new("hello\nworld");
        buf.select_all();
        assert_eq!(buf.selected_text(), "hello\nworld");
    }

    #[test]
    fn undo_insert() {
        let mut buf = EditorBuffer::new("hello");
        buf.set_cursor(0, 5);
        buf.insert(" world");
        assert_eq!(buf.text(), "hello world");
        buf.undo();
        assert_eq!(buf.text(), "hello");
    }

    #[test]
    fn redo_insert() {
        let mut buf = EditorBuffer::new("hello");
        buf.set_cursor(0, 5);
        buf.insert(" world");
        buf.undo();
        buf.redo();
        assert_eq!(buf.text(), "hello world");
    }

    #[test]
    fn undo_backspace() {
        let mut buf = EditorBuffer::new("hello");
        buf.set_cursor(0, 5);
        buf.backspace();
        assert_eq!(buf.text(), "hell");
        buf.undo();
        assert_eq!(buf.text(), "hello");
    }

    #[test]
    fn insert_replaces_selection() {
        let mut buf = EditorBuffer::new("hello world");
        buf.set_cursor(0, 0);
        buf.select_right();
        buf.select_right();
        buf.select_right();
        buf.select_right();
        buf.select_right();
        buf.insert("hi");
        assert_eq!(buf.text(), "hi world");
    }

    #[test]
    fn dirty_tracking() {
        let mut buf = EditorBuffer::new("hello");
        assert!(!buf.is_dirty());
        buf.insert("x");
        assert!(buf.is_dirty());
        buf.set_dirty(false);
        assert!(!buf.is_dirty());
    }

    #[test]
    fn scroll_ensures_cursor_visible() {
        let mut buf = EditorBuffer::new("a\nb\nc\nd\ne\nf\ng\nh\ni\nj");
        buf.set_cursor(8, 0);
        buf.ensure_cursor_visible(5);
        assert_eq!(buf.scroll_offset(), 4);
    }

    #[test]
    fn scroll_by_positive() {
        let mut buf = EditorBuffer::new("a\nb\nc\nd\ne");
        buf.scroll_by(2);
        assert_eq!(buf.scroll_offset(), 2);
    }

    #[test]
    fn scroll_by_negative() {
        let mut buf = EditorBuffer::new("a\nb\nc\nd\ne");
        buf.scroll_by(3);
        buf.scroll_by(-1);
        assert_eq!(buf.scroll_offset(), 2);
    }

    #[test]
    fn scroll_by_clamps_to_zero() {
        let mut buf = EditorBuffer::new("a\nb\nc");
        buf.scroll_by(-10);
        assert_eq!(buf.scroll_offset(), 0);
    }
}
