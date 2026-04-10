use std::collections::HashMap;

use liquid::ParserBuilder;
use liquid::partials::{EagerCompiler, InMemorySource};
use liquid_core::{
    Display_filter, Expression, Filter, FilterParameters, FilterReflection,
    FromFilterParameters, ParseFilter, Runtime, Value, ValueView,
};
use serde::Serialize;
use thiserror::Error;

use crate::i18n::translate_render;
use crate::render::render_markdown_to_html;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("liquid error: {0}")]
    Liquid(#[from] liquid::Error),
}

#[derive(Debug, Clone, Default)]
struct HtmlRewriteContext {
    canonical_post_path_by_slug: HashMap<String, String>,
    canonical_media_path_by_source_path: HashMap<String, String>,
}

pub fn render_liquid_template<T: Serialize>(
    template_source: &str,
    partials: &HashMap<String, String>,
    context: &T,
) -> Result<String, RenderError> {
    let mut compiled_partials: EagerCompiler<InMemorySource> = EagerCompiler::empty();
    for (name, content) in partials {
        compiled_partials.add(format!("{name}.liquid"), content.clone());
    }

    let parser = ParserBuilder::with_stdlib()
        .filter(I18n)
        .filter(Markdown)
        .partials(compiled_partials)
        .build()?;
    let template = parser.parse(template_source)?;
    let globals = liquid::to_object(context)?;
    Ok(template.render(&globals)?)
}

#[derive(Debug, FilterParameters)]
struct I18nArgs {
    #[parameter(description = "Render language", arg_type = "str")]
    language: Expression,
}

#[derive(Clone, ParseFilter, FilterReflection)]
#[filter(
    name = "i18n",
    description = "Translate a render key for a content language.",
    parameters(I18nArgs),
    parsed(I18nFilter)
)]
struct I18n;

#[derive(Debug, FromFilterParameters, Display_filter)]
#[name = "i18n"]
struct I18nFilter {
    #[parameters]
    args: I18nArgs,
}

impl Filter for I18nFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> liquid_core::Result<Value> {
        let args = self.args.evaluate(runtime)?;
        let key = input.to_kstr();
        let language = args.language.to_kstr();
        Ok(Value::scalar(translate_render(language.as_str(), key.as_str())))
    }
}

#[derive(Debug, FilterParameters)]
struct MarkdownArgs {
    #[parameter(description = "Post id", arg_type = "str")]
    post_id: Option<Expression>,
    #[parameter(description = "Post data by id", arg_type = "any")]
    post_data_json_by_id: Option<Expression>,
    #[parameter(description = "Canonical post path map", arg_type = "any")]
    canonical_post_path_by_slug: Option<Expression>,
    #[parameter(description = "Canonical media path map", arg_type = "any")]
    canonical_media_path_by_source_path: Option<Expression>,
    #[parameter(description = "Render language", arg_type = "str")]
    language: Option<Expression>,
    #[parameter(description = "Language prefix", arg_type = "str")]
    language_prefix: Option<Expression>,
}

#[derive(Clone, ParseFilter, FilterReflection)]
#[filter(
    name = "markdown",
    description = "Render markdown to HTML and rewrite preview URLs.",
    parameters(MarkdownArgs),
    parsed(MarkdownFilter)
)]
struct Markdown;

#[derive(Debug, FromFilterParameters, Display_filter)]
#[name = "markdown"]
struct MarkdownFilter {
    #[parameters]
    args: MarkdownArgs,
}

impl Filter for MarkdownFilter {
    fn evaluate(&self, input: &dyn ValueView, runtime: &dyn Runtime) -> liquid_core::Result<Value> {
        let args = self.args.evaluate(runtime)?;
        let markdown = input.to_kstr();
        let rewrite_context = HtmlRewriteContext {
            canonical_post_path_by_slug: args
                .canonical_post_path_by_slug
                .as_ref()
                .map(value_to_string_map)
                .unwrap_or_default(),
            canonical_media_path_by_source_path: args
                .canonical_media_path_by_source_path
                .as_ref()
                .map(value_to_string_map)
                .unwrap_or_default(),
        };

        let rendered = render_markdown_to_html(markdown.as_str());
        Ok(Value::scalar(rewrite_rendered_html_urls(&rendered, &rewrite_context)))
    }
}

fn value_to_string_map(value: &impl ValueView) -> HashMap<String, String> {
    value
        .as_object()
        .map(|object| {
            object
                .iter()
                .map(|(key, value)| (key.to_string(), value.to_kstr().into_owned().to_string()))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn rewrite_rendered_html_urls(html: &str, rewrite_context: &impl RewriteContextView) -> String {
    let rewritten = rewrite_attribute_urls(html, "href", |href| normalize_preview_href(href, rewrite_context));
    rewrite_attribute_urls(&rewritten, "src", |src| normalize_preview_src(src, rewrite_context))
}

pub(crate) trait RewriteContextView {
    fn canonical_post_path_by_slug(&self) -> &HashMap<String, String>;
    fn canonical_media_path_by_source_path(&self) -> &HashMap<String, String>;
}

impl RewriteContextView for HtmlRewriteContext {
    fn canonical_post_path_by_slug(&self) -> &HashMap<String, String> {
        &self.canonical_post_path_by_slug
    }

    fn canonical_media_path_by_source_path(&self) -> &HashMap<String, String> {
        &self.canonical_media_path_by_source_path
    }
}

fn rewrite_attribute_urls(
    html: &str,
    attribute: &str,
    normalize: impl Fn(&str) -> String,
) -> String {
    let mut result = String::with_capacity(html.len());
    let mut cursor = 0;

    while let Some(offset) = html[cursor..].find(attribute) {
        let attr_start = cursor + offset;
        result.push_str(&html[cursor..attr_start]);
        result.push_str(attribute);

        let after_attr = attr_start + attribute.len();
        if !html[after_attr..].starts_with('=') {
            cursor = after_attr;
            continue;
        }
        result.push('=');

        let quote_index = after_attr + 1;
        let Some(quote) = html[quote_index..].chars().next() else {
            cursor = after_attr + 1;
            continue;
        };
        if quote != '\'' && quote != '"' {
            cursor = after_attr + 1;
            continue;
        }
        result.push(quote);

        let value_start = quote_index + quote.len_utf8();
        let Some(value_end_rel) = html[value_start..].find(quote) else {
            result.push_str(&html[value_start..]);
            return result;
        };
        let value_end = value_start + value_end_rel;
        result.push_str(&normalize(&html[value_start..value_end]));
        result.push(quote);
        cursor = value_end + quote.len_utf8();
    }

    result.push_str(&html[cursor..]);
    result
}

fn normalize_preview_href(raw_href: &str, rewrite_context: &impl RewriteContextView) -> String {
    if raw_href.is_empty() {
        return raw_href.to_string();
    }

    let (path_part, suffix) = split_path_suffix(raw_href.trim());
    if let Some(media_lookup_key) = extract_media_lookup_key(path_part) {
        let canonical = rewrite_context
            .canonical_media_path_by_source_path()
            .get(&media_lookup_key)
            .cloned()
            .unwrap_or_else(|| format!("/{media_lookup_key}"));
        return format!("{canonical}{suffix}");
    }

    if is_external_or_special_url(raw_href) {
        return raw_href.to_string();
    }

    if let Some(normalized) = normalize_day_route(path_part) {
        return format!("{normalized}{suffix}");
    }

    if let Some(slug) = extract_post_slug(path_part) {
        let canonical = rewrite_context
            .canonical_post_path_by_slug()
            .get(&slug)
            .cloned()
            .unwrap_or_else(|| format!("/posts/{slug}"));
        return format!("{canonical}{suffix}");
    }

    if let Some(media_source_key) = extract_media_lookup_key(path_part) {
        let canonical = rewrite_context
            .canonical_media_path_by_source_path()
            .get(&media_source_key)
            .cloned()
            .unwrap_or_else(|| format!("/{media_source_key}"));
        return format!("{canonical}{suffix}");
    }

    raw_href.to_string()
}

fn normalize_preview_src(raw_src: &str, rewrite_context: &impl RewriteContextView) -> String {
    if raw_src.is_empty() {
        return raw_src.to_string();
    }

    let (path_part, suffix) = split_path_suffix(raw_src.trim());
    if let Some(media_source_key) = extract_media_lookup_key(path_part) {
        let canonical = rewrite_context
            .canonical_media_path_by_source_path()
            .get(&media_source_key)
            .cloned()
            .unwrap_or_else(|| format!("/{media_source_key}"));
        return format!("{canonical}{suffix}");
    }

    if is_external_or_special_url(raw_src) {
        return raw_src.to_string();
    }

    raw_src.to_string()
}

fn is_external_or_special_url(value: &str) -> bool {
    let normalized = value.trim();
    if normalized.is_empty() {
        return false;
    }
    if normalized.starts_with('#') || normalized.starts_with("//") {
        return true;
    }

    let mut seen_alpha = false;
    for ch in normalized.chars() {
        if ch == ':' {
            return seen_alpha;
        }
        if ch.is_ascii_alphanumeric() || matches!(ch, '+' | '.' | '-') {
            seen_alpha = true;
            continue;
        }
        break;
    }

    false
}

fn split_path_suffix(value: &str) -> (&str, &str) {
    let split_index = value.find(['?', '#']).unwrap_or(value.len());
    (&value[..split_index], &value[split_index..])
}

fn normalize_day_route(path: &str) -> Option<String> {
    let segments: Vec<_> = path.trim_start_matches('/').split('/').collect();
    if segments.len() != 4 {
        return None;
    }
    let [year, month, day, slug] = segments.as_slice() else {
        return None;
    };
    if year.len() != 4 || !year.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let month = month.parse::<u32>().ok()?;
    let day = day.parse::<u32>().ok()?;
    if slug.is_empty() {
        return None;
    }
    Some(format!("/{year}/{month:02}/{day:02}/{slug}"))
}

fn extract_post_slug(path: &str) -> Option<String> {
    let trimmed = path.trim_start_matches('/');
    let segments: Vec<_> = trimmed.split('/').collect();
    match segments.as_slice() {
        ["post" | "posts", slug] => Some(trim_html_suffix(slug)),
        ["post" | "posts", year, month, slug] if year.len() == 4 && month.chars().all(|ch| ch.is_ascii_digit()) => {
            Some(trim_html_suffix(slug))
        }
        _ => None,
    }
}

fn extract_media_lookup_key(path: &str) -> Option<String> {
    if let Some(media_id) = path.trim().strip_prefix("bds-media://") {
        let media_id = media_id.trim();
        if media_id.is_empty() {
            return None;
        }
        return Some(format!("bds-media://{media_id}"));
    }

    let trimmed = path.trim_start_matches('/');
    let segments: Vec<_> = trimmed.split('/').collect();
    match segments.as_slice() {
        ["media", year, month, filename] if year.len() == 4 && month.len() == 2 => {
            Some(format!("media/{year}/{month}/{}", filename.to_lowercase()))
        }
        _ => None,
    }
}

fn trim_html_suffix(value: &str) -> String {
    value
        .trim_end_matches(".html")
        .trim_end_matches(".htm")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{rewrite_rendered_html_urls, RewriteContextView};
    use std::collections::HashMap;

    struct TestRewriteContext {
        canonical_post_path_by_slug: HashMap<String, String>,
        canonical_media_path_by_source_path: HashMap<String, String>,
    }

    impl RewriteContextView for TestRewriteContext {
        fn canonical_post_path_by_slug(&self) -> &HashMap<String, String> {
            &self.canonical_post_path_by_slug
        }

        fn canonical_media_path_by_source_path(&self) -> &HashMap<String, String> {
            &self.canonical_media_path_by_source_path
        }
    }

    #[test]
    fn rewrites_bds_media_image_src_to_canonical_media_path() {
        let context = TestRewriteContext {
            canonical_post_path_by_slug: HashMap::new(),
            canonical_media_path_by_source_path: HashMap::from([(
                "bds-media://media-1".to_string(),
                "/media/2026/04/media-1.png".to_string(),
            )]),
        };

        let html = rewrite_rendered_html_urls(
            "<p><img src=\"bds-media://media-1\" alt=\"\" /></p>",
            &context,
        );

        assert!(html.contains("src=\"/media/2026/04/media-1.png\""));
    }
}