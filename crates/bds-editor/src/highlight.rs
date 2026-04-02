use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

/// Syntax highlighter using syntect.
pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    theme_name: String,
}

/// A line of highlighted text: spans of (style, text).
pub type HighlightedLine = Vec<(Style, String)>;

impl Highlighter {
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            theme_name: "base16-ocean.dark".to_string(),
        }
    }

    /// Find the syntax definition for a file extension.
    pub fn syntax_for_extension(&self, ext: &str) -> &SyntaxReference {
        self.syntax_set
            .find_syntax_by_extension(ext)
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text())
    }

    /// Highlight all lines of the given text for a specific syntax.
    pub fn highlight_lines(&self, text: &str, syntax: &SyntaxReference) -> Vec<HighlightedLine> {
        use syntect::easy::HighlightLines;

        let theme = &self.theme_set.themes[&self.theme_name];
        let mut h = HighlightLines::new(syntax, theme);
        let mut result = Vec::new();

        for line in text.lines() {
            let line_with_newline = format!("{line}\n");
            let ranges = h
                .highlight_line(&line_with_newline, &self.syntax_set)
                .unwrap_or_default();
            let styled: HighlightedLine = ranges
                .into_iter()
                .map(|(style, text)| (style, text.to_string()))
                .collect();
            result.push(styled);
        }

        result
    }
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlight_markdown() {
        let h = Highlighter::new();
        let syntax = h.syntax_for_extension("md");
        let lines = h.highlight_lines("# Hello\n\nSome text.", syntax);
        assert_eq!(lines.len(), 3);
        assert!(!lines[0].is_empty());
    }

    #[test]
    fn plain_text_fallback() {
        let h = Highlighter::new();
        let syntax = h.syntax_for_extension("nonexistent");
        // Should not panic, falls back to plain text
        let lines = h.highlight_lines("hello", syntax);
        assert_eq!(lines.len(), 1);
    }
}
