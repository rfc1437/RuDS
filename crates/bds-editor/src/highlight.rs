use std::collections::HashMap;
use std::sync::LazyLock;

use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

/// Global highlighter singleton (expensive to create, safe to share).
static GLOBAL_HIGHLIGHTER: LazyLock<Highlighter> = LazyLock::new(Highlighter::new);

/// Return a reference to the global syntax highlighter.
pub fn highlighter() -> &'static Highlighter {
    &GLOBAL_HIGHLIGHTER
}

/// Syntax highlighter using syntect with line-level caching.
pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
    theme_name: String,
    /// Extension aliases (e.g. "liquid" → "html")
    extension_aliases: HashMap<String, String>,
}

/// A line of highlighted text: spans of (style, text).
pub type HighlightedLine = Vec<(Style, String)>;

impl Highlighter {
    pub fn new() -> Self {
        let mut extension_aliases = HashMap::new();
        // Liquid templates use HTML syntax highlighting
        extension_aliases.insert("liquid".into(), "html".into());
        extension_aliases.insert("njk".into(), "html".into());

        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
            theme_name: "base16-ocean.dark".to_string(),
            extension_aliases,
        }
    }

    /// Find the syntax definition for a file extension.
    /// Resolves aliases (e.g. liquid → html) before lookup.
    pub fn syntax_for_extension(&self, ext: &str) -> &SyntaxReference {
        let resolved = self
            .extension_aliases
            .get(ext)
            .map(|s| s.as_str())
            .unwrap_or(ext);
        self.syntax_set
            .find_syntax_by_extension(resolved)
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

    /// Highlight only a visible range of lines (start..start+count) for a given text.
    /// Re-parses from the beginning to maintain correct parse state, but only
    /// collects highlight output for the visible window.
    pub fn highlight_visible_range(
        &self,
        text: &str,
        syntax: &SyntaxReference,
        start: usize,
        count: usize,
    ) -> Vec<HighlightedLine> {
        use syntect::easy::HighlightLines;

        let theme = &self.theme_set.themes[&self.theme_name];
        let mut h = HighlightLines::new(syntax, theme);
        let mut result = Vec::new();
        let end = start + count;

        for (idx, line) in text.lines().enumerate() {
            let line_with_newline = format!("{line}\n");
            let ranges = h
                .highlight_line(&line_with_newline, &self.syntax_set)
                .unwrap_or_default();
            if idx >= start && idx < end {
                let styled: HighlightedLine = ranges
                    .into_iter()
                    .map(|(style, text)| (style, text.to_string()))
                    .collect();
                result.push(styled);
            }
            if idx >= end {
                break;
            }
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

    #[test]
    fn liquid_uses_html_syntax() {
        let h = Highlighter::new();
        let liquid_syntax = h.syntax_for_extension("liquid");
        let html_syntax = h.syntax_for_extension("html");
        assert_eq!(liquid_syntax.name, html_syntax.name);
    }

    #[test]
    fn json_syntax_available() {
        let h = Highlighter::new();
        let syntax = h.syntax_for_extension("json");
        assert_ne!(syntax.name, "Plain Text");
    }

    #[test]
    fn yaml_syntax_available() {
        let h = Highlighter::new();
        let syntax = h.syntax_for_extension("yaml");
        assert_ne!(syntax.name, "Plain Text");
    }

    #[test]
    fn lua_syntax_available() {
        let h = Highlighter::new();
        let syntax = h.syntax_for_extension("lua");
        assert_ne!(syntax.name, "Plain Text");
    }

    #[test]
    fn highlight_visible_range_returns_subset() {
        let h = Highlighter::new();
        let syntax = h.syntax_for_extension("md");
        let text = "# Line 1\nLine 2\nLine 3\nLine 4\nLine 5";
        let visible = h.highlight_visible_range(text, syntax, 1, 2);
        assert_eq!(visible.len(), 2);
    }
}
