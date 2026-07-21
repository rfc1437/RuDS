use std::collections::HashSet;

#[test]
fn linked_editor_highlights_markdown() {
    // Retain the dylib that previously interposed bds-editor's regex symbols.
    let _ = std::hint::black_box(
        bds_server::run_headless
            as fn(bds_server::boot::BootMode, bds_server::ServerConfig) -> anyhow::Result<()>,
    );
    let highlighter = bds_editor::highlighter();
    assert_eq!(
        highlighter.syntax_for_extension("md").name,
        "Markdown with Macros"
    );
    let colors = highlighter
        .highlight_lines(
            "plain\n# Heading\n[link](https://example.com)\n`code`",
            highlighter.syntax_for_extension("md"),
        )
        .into_iter()
        .flatten()
        .map(|(style, _)| style.foreground)
        .collect::<HashSet<_>>();

    assert!(colors.len() > 1, "linked editor should use syntax colors");
}
