use std::collections::HashMap;
use std::sync::LazyLock;

use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::{SyntaxDefinition, SyntaxReference, SyntaxSet};

const MARKDOWN_WITH_MACROS_SYNTAX: &str =
    include_str!("../syntaxes/Markdown with Macros.sublime-syntax");
const LIQUID_SYNTAX: &str = include_str!("../syntaxes/Liquid.sublime-syntax");

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
    /// Extension aliases used by the application editors.
    extension_aliases: HashMap<String, String>,
}

/// A line of highlighted text: spans of (style, text).
pub type HighlightedLine = Vec<(Style, String)>;

impl Highlighter {
    pub fn new() -> Self {
        let mut syntax_builder = SyntaxSet::load_defaults_newlines().into_builder();
        for (definition, name) in [
            (MARKDOWN_WITH_MACROS_SYNTAX, "Markdown with Macros"),
            (LIQUID_SYNTAX, "Liquid"),
        ] {
            syntax_builder.add(
                SyntaxDefinition::load_from_str(definition, true, Some(name))
                    .unwrap_or_else(|error| panic!("invalid {name} syntax definition: {error}")),
            );
        }

        let mut extension_aliases = HashMap::new();
        extension_aliases.insert("md".into(), "bds-md".into());
        extension_aliases.insert("markdown".into(), "bds-md".into());
        extension_aliases.insert("njk".into(), "html".into());

        Self {
            syntax_set: syntax_builder.build(),
            theme_set: ThemeSet::load_defaults(),
            theme_name: "base16-ocean.dark".to_string(),
            extension_aliases,
        }
    }

    /// Find the syntax definition for a file extension.
    /// Resolves application aliases before lookup.
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

    fn foreground_for<'a>(
        line: &'a HighlightedLine,
        fragment: &str,
    ) -> &'a syntect::highlighting::Color {
        &line
            .iter()
            .find(|(_, text)| text.contains(fragment))
            .unwrap_or_else(|| panic!("missing highlighted fragment {fragment:?} in {line:?}"))
            .0
            .foreground
    }

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
    fn markdown_uses_bds_macro_syntax() {
        let h = Highlighter::new();
        assert_eq!(h.syntax_for_extension("md").name, "Markdown with Macros");
    }

    #[test]
    fn liquid_uses_dedicated_syntax() {
        let h = Highlighter::new();
        assert_eq!(h.syntax_for_extension("liquid").name, "Liquid");
    }

    #[test]
    fn markdown_macro_components_receive_distinct_styles() {
        let h = Highlighter::new();
        let lines = h.highlight_lines(
            "plain\n[[gallery columns=\"3\"]]",
            h.syntax_for_extension("md"),
        );
        let plain = foreground_for(&lines[0], "plain");
        assert_ne!(
            foreground_for(&lines[1], "[[gallery"),
            plain,
            "macro name should be highlighted: {:?}",
            lines[1]
        );
        assert_ne!(
            foreground_for(&lines[1], "columns"),
            plain,
            "attribute name should be highlighted: {:?}",
            lines[1]
        );
        assert_ne!(
            foreground_for(&lines[1], "\"3\""),
            plain,
            "attribute value should be highlighted: {:?}",
            lines[1]
        );
    }

    #[test]
    fn markdown_constructs_receive_distinct_styles() {
        let h = Highlighter::new();
        let lines = h.highlight_lines(
            "plain\n# Heading\n[link](https://example.com)\n`code`\n**strong**\n*emphasis*\n- item",
            h.syntax_for_extension("md"),
        );
        let plain = foreground_for(&lines[0], "plain");
        for (line, fragment) in [
            (1, "Heading"),
            (2, "link"),
            (3, "code"),
            (4, "strong"),
            (5, "emphasis"),
            (6, "- "),
        ] {
            assert_ne!(
                foreground_for(&lines[line], fragment),
                plain,
                "Markdown fragment {fragment:?} should be colored: {:?}",
                lines[line]
            );
        }
    }

    #[test]
    fn liquid_tags_filters_and_comments_receive_distinct_styles() {
        let h = Highlighter::new();
        let lines = h.highlight_lines(
            "plain\n{{ post.title | escape }}\n{% if post %}body{% endif %}\n{% comment %}hidden{% endcomment %}",
            h.syntax_for_extension("liquid"),
        );
        let plain = foreground_for(&lines[0], "plain");
        assert_ne!(foreground_for(&lines[1], "escape"), plain);
        assert_ne!(foreground_for(&lines[2], "if"), plain);
        assert_ne!(foreground_for(&lines[3], "hidden"), plain);
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
    fn editor_syntaxes_produce_distinct_foreground_colors() {
        let h = Highlighter::new();
        for (extension, source) in [
            ("md", "# Heading\n[link](https://example.com)\n`code`"),
            (
                "liquid",
                "<h1>{{ post.title }}</h1>\n{% if post %}Body{% endif %}",
            ),
            (
                "lua",
                "local answer = function(value)\n  -- comment\n  return value + 42\nend",
            ),
        ] {
            let colors = h
                .highlight_lines(source, h.syntax_for_extension(extension))
                .into_iter()
                .flatten()
                .map(|(style, _)| style.foreground)
                .collect::<std::collections::HashSet<_>>();
            assert!(colors.len() > 1, "{extension} should use syntax colors");
        }
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
