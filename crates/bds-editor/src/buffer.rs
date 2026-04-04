use ropey::Rope;

/// Rope-based text buffer with edit operations and cursor management.
pub struct EditorBuffer {
    rope: Rope,
    cursor_line: usize,
    cursor_col: usize,
    /// Vertical scroll offset in lines.
    scroll_offset: usize,
}

impl EditorBuffer {
    pub fn new(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            cursor_line: 0,
            cursor_col: 0,
            scroll_offset: 0,
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

    pub fn set_cursor(&mut self, line: usize, col: usize) {
        self.cursor_line = line.min(self.line_count().saturating_sub(1));
        let line_len = self.current_line_len();
        self.cursor_col = col.min(line_len);
    }

    /// Insert text at the current cursor position.
    pub fn insert(&mut self, text: &str) {
        let char_idx = self.cursor_char_idx();
        self.rope.insert(char_idx, text);
        for c in text.chars() {
            if c == '\n' {
                self.cursor_line += 1;
                self.cursor_col = 0;
            } else {
                self.cursor_col += 1;
            }
        }
    }

    /// Delete one character before the cursor (backspace).
    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let char_idx = self.cursor_char_idx();
            self.rope.remove(char_idx - 1..char_idx);
            self.cursor_col -= 1;
        } else if self.cursor_line > 0 {
            let prev_line_len = self
                .rope
                .line(self.cursor_line - 1)
                .len_chars()
                .saturating_sub(1);
            let char_idx = self.cursor_char_idx();
            self.rope.remove(char_idx - 1..char_idx);
            self.cursor_line -= 1;
            self.cursor_col = prev_line_len;
        }
    }

    /// Delete one character after the cursor (delete key).
    pub fn delete_forward(&mut self) {
        let char_idx = self.cursor_char_idx();
        if char_idx < self.rope.len_chars() {
            self.rope.remove(char_idx..char_idx + 1);
        }
    }

    /// Move cursor up one line.
    pub fn move_up(&mut self) {
        if self.cursor_line > 0 {
            self.cursor_line -= 1;
            let line_len = self.current_line_len();
            self.cursor_col = self.cursor_col.min(line_len);
        }
    }

    /// Move cursor down one line.
    pub fn move_down(&mut self) {
        if self.cursor_line + 1 < self.line_count() {
            self.cursor_line += 1;
            let line_len = self.current_line_len();
            self.cursor_col = self.cursor_col.min(line_len);
        }
    }

    /// Move cursor left one character.
    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_line > 0 {
            self.cursor_line -= 1;
            self.cursor_col = self.current_line_len();
        }
    }

    /// Move cursor right one character.
    pub fn move_right(&mut self) {
        let line_len = self.current_line_len();
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        } else if self.cursor_line + 1 < self.line_count() {
            self.cursor_line += 1;
            self.cursor_col = 0;
        }
    }

    /// Move cursor to start of line.
    pub fn move_home(&mut self) {
        self.cursor_col = 0;
    }

    /// Move cursor to end of line.
    pub fn move_end(&mut self) {
        self.cursor_col = self.current_line_len();
    }

    /// Scroll so the cursor is visible within the given viewport height (in lines).
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

    /// Scroll by a delta (positive = down, negative = up).
    pub fn scroll_by(&mut self, delta: isize) {
        let max_scroll = self.line_count().saturating_sub(1);
        if delta < 0 {
            self.scroll_offset = self.scroll_offset.saturating_sub((-delta) as usize);
        } else {
            self.scroll_offset = (self.scroll_offset + delta as usize).min(max_scroll);
        }
    }

    fn cursor_char_idx(&self) -> usize {
        let line_start = self.rope.line_to_char(self.cursor_line);
        line_start + self.cursor_col
    }

    fn current_line_len(&self) -> usize {
        if self.cursor_line < self.rope.len_lines() {
            let line = self.rope.line(self.cursor_line);
            let len = line.len_chars();
            if len > 0 && line.char(len - 1) == '\n' {
                len - 1
            } else {
                len
            }
        } else {
            0
        }
    }
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
        buf.move_up(); // at top, no-op
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
        buf.move_down(); // at bottom, no-op
        assert_eq!(buf.cursor(), (2, 1));
    }

    #[test]
    fn move_up_clamps_col_to_shorter_line() {
        let mut buf = EditorBuffer::new("long line\nhi");
        buf.set_cursor(0, 9);
        buf.move_down();
        assert_eq!(buf.cursor(), (1, 2)); // "hi" is only 2 chars
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
    fn scroll_ensures_cursor_visible() {
        let mut buf = EditorBuffer::new("a\nb\nc\nd\ne\nf\ng\nh\ni\nj");
        buf.set_cursor(8, 0); // line 8
        buf.ensure_cursor_visible(5); // viewport shows 5 lines
        assert_eq!(buf.scroll_offset(), 4); // scroll so line 8 is visible
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
