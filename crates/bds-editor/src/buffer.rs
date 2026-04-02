use ropey::Rope;

/// Rope-based text buffer with edit operations.
pub struct EditorBuffer {
    rope: Rope,
    /// Cursor byte offset within the rope.
    cursor_line: usize,
    cursor_col: usize,
}

impl EditorBuffer {
    pub fn new(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            cursor_line: 0,
            cursor_col: 0,
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

    pub fn set_cursor(&mut self, line: usize, col: usize) {
        self.cursor_line = line.min(self.line_count().saturating_sub(1));
        let line_len = self.current_line_len();
        self.cursor_col = col.min(line_len);
    }

    /// Insert text at the current cursor position.
    pub fn insert(&mut self, text: &str) {
        let char_idx = self.cursor_char_idx();
        self.rope.insert(char_idx, text);
        // Advance cursor past inserted text
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
            // Join with previous line
            let prev_line_len = self
                .rope
                .line(self.cursor_line - 1)
                .len_chars()
                .saturating_sub(1); // subtract newline
            let char_idx = self.cursor_char_idx();
            self.rope.remove(char_idx - 1..char_idx);
            self.cursor_line -= 1;
            self.cursor_col = prev_line_len;
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
            // Subtract trailing newline if present
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
}
