use pulldown_cmark::{Options, Parser, html};

pub fn render_markdown_to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut output = String::new();
    html::push_html(&mut output, parser);
    output
}

#[cfg(test)]
mod tests {
    use super::render_markdown_to_html;

    #[test]
    fn renders_commonmark_to_html() {
        let rendered = render_markdown_to_html("# Title\n\nSome *text*.");
        assert!(rendered.contains("<h1>Title</h1>"));
        assert!(rendered.contains("<p>Some <em>text</em>.</p>"));
    }
}
