//! bds-editor PoC integration test.
//!
//! M0 validation: verifies that the editor components (buffer + highlighter)
//! work together — highlighted markdown rendering, keyboard input simulation,
//! and cursor movement. The Iced widget (CodeEditor) requires a renderer and
//! cannot be tested without a windowing system; this test covers the
//! non-GUI integration surface.

use bds_editor::{EditorBuffer, Highlighter};

// ── PoC: highlighted markdown rendering ──

#[test]
fn highlight_markdown_document() {
    let md = "# Title\n\nA paragraph with **bold** text.\n\n- item one\n- item two\n";
    let hl = Highlighter::new();
    let syntax = hl.syntax_for_extension("md");
    let lines = hl.highlight_lines(md, syntax);

    // 6 content lines in the markdown
    assert_eq!(lines.len(), 6);
    // Each line has at least one styled span
    for (i, line) in lines.iter().enumerate() {
        assert!(!line.is_empty(), "line {i} should have styled spans");
    }
    // The heading line should contain "# Title"
    let heading_text: String = lines[0].iter().map(|(_, t)| t.as_str()).collect();
    assert!(
        heading_text.contains("# Title"),
        "heading line should contain '# Title', got: {heading_text}"
    );
}

#[test]
fn highlight_multiple_syntaxes() {
    let hl = Highlighter::new();

    // Markdown
    let md_syntax = hl.syntax_for_extension("md");
    let md_lines = hl.highlight_lines("# Hello", md_syntax);
    assert_eq!(md_lines.len(), 1);

    // YAML (used in frontmatter)
    let yaml_syntax = hl.syntax_for_extension("yaml");
    let yaml_lines = hl.highlight_lines("key: value", yaml_syntax);
    assert_eq!(yaml_lines.len(), 1);

    // HTML (used in templates)
    let html_syntax = hl.syntax_for_extension("html");
    let html_lines = hl.highlight_lines("<div>hello</div>", html_syntax);
    assert_eq!(html_lines.len(), 1);
}

// ── PoC: keyboard input simulation ──

#[test]
fn type_text_into_buffer() {
    let mut buf = EditorBuffer::new("");
    // Simulate typing "Hello, world!"
    for c in "Hello, world!".chars() {
        buf.insert(&c.to_string());
    }
    assert_eq!(buf.text(), "Hello, world!");
    assert_eq!(buf.cursor(), (0, 13));
}

#[test]
fn type_multiline_text() {
    let mut buf = EditorBuffer::new("");
    buf.insert("line one");
    buf.insert("\n");
    buf.insert("line two");
    buf.insert("\n");
    buf.insert("line three");

    assert_eq!(buf.line_count(), 3);
    assert_eq!(buf.cursor(), (2, 10));

    let text = buf.text();
    assert!(text.contains("line one\n"));
    assert!(text.contains("line two\n"));
    assert!(text.contains("line three"));
}

#[test]
fn backspace_and_retype() {
    let mut buf = EditorBuffer::new("");
    buf.insert("Helo");
    // Fix typo: backspace removes 'o', then type correct chars
    buf.backspace(); // "Hel" cursor at 3
    buf.insert("lo"); // "Hello"
    assert_eq!(buf.text(), "Hello");
}

#[test]
fn delete_forward_mid_line() {
    let mut buf = EditorBuffer::new("abcdef");
    buf.set_cursor(0, 3); // after 'c'
    buf.delete_forward(); // remove 'd'
    assert_eq!(buf.text(), "abcef");
    assert_eq!(buf.cursor(), (0, 3));
}

// ── PoC: cursor movement ──

#[test]
fn cursor_navigation_full_cycle() {
    let mut buf = EditorBuffer::new("first line\nsecond line\nthird line");

    // Start at (0,0), move to end of first line
    buf.move_end();
    assert_eq!(buf.cursor(), (0, 10));

    // Move down to second line
    buf.move_down();
    assert_eq!(buf.cursor(), (1, 10));

    // Move home
    buf.move_home();
    assert_eq!(buf.cursor(), (1, 0));

    // Move down to third line
    buf.move_down();
    assert_eq!(buf.cursor(), (2, 0));

    // Move right 5 times
    for _ in 0..5 {
        buf.move_right();
    }
    assert_eq!(buf.cursor(), (2, 5));

    // Move up twice to first line
    buf.move_up();
    buf.move_up();
    assert_eq!(buf.cursor(), (0, 5));

    // Move left to beginning with wrap
    for _ in 0..6 {
        buf.move_left();
    }
    assert_eq!(buf.cursor(), (0, 0));
}

#[test]
fn cursor_wrap_across_lines() {
    let mut buf = EditorBuffer::new("abc\ndef");

    // Move right past end of first line wraps to second
    buf.set_cursor(0, 3);
    buf.move_right();
    assert_eq!(buf.cursor(), (1, 0));

    // Move left at start of second line wraps to first
    buf.move_left();
    assert_eq!(buf.cursor(), (0, 3));
}

// ── PoC: scrolling ──

#[test]
fn scroll_keeps_cursor_visible() {
    // 20 lines of content
    let text: String = (0..20).map(|i| format!("line {i}\n")).collect();
    let mut buf = EditorBuffer::new(&text);

    // Simulate moving cursor to line 15
    for _ in 0..15 {
        buf.move_down();
    }
    assert_eq!(buf.cursor().0, 15);

    // With a viewport of 10 lines, ensure scroll tracks
    buf.ensure_cursor_visible(10);
    let scroll = buf.scroll_offset();
    assert!(
        scroll <= 15 && 15 < scroll + 10,
        "cursor line 15 should be visible in viewport starting at {scroll}"
    );
}

#[test]
fn scroll_by_manual() {
    let text: String = (0..50).map(|i| format!("line {i}\n")).collect();
    let mut buf = EditorBuffer::new(&text);

    buf.scroll_by(10);
    assert_eq!(buf.scroll_offset(), 10);

    buf.scroll_by(-3);
    assert_eq!(buf.scroll_offset(), 7);
}

// ── PoC: edit + highlight round-trip ──

#[test]
fn edit_then_rehighlight() {
    let mut buf = EditorBuffer::new("# Hello\n\nSome text.");
    let hl = Highlighter::new();
    let syntax = hl.syntax_for_extension("md");

    // Initial highlight
    let lines1 = hl.highlight_lines(&buf.text(), syntax);
    assert_eq!(lines1.len(), 3);

    // Edit: add a new line
    buf.set_cursor(2, 10);
    buf.insert("\n\n## Subheading");

    // Re-highlight after edit
    let lines2 = hl.highlight_lines(&buf.text(), syntax);
    assert_eq!(lines2.len(), 5); // 3 original + blank + subheading
    let subheading_text: String = lines2[4].iter().map(|(_, t)| t.as_str()).collect();
    assert!(subheading_text.contains("## Subheading"));
}
