use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use liquid::ParserBuilder;
use liquid::partials::{EagerCompiler, InMemorySource};
use liquid_core::model::ScalarCow;
use liquid_core::parser::FilterArguments;
use liquid_core::{
    Display_filter, Expression, Filter, FilterParameters, FilterReflection, FromFilterParameters,
    ParseFilter, Runtime, Value, ValueView,
};
use serde::Serialize;
use serde_json::{Map as JsonMap, Value as JsonValue};
use thiserror::Error;

use crate::i18n::translate_render;
use crate::render::macros::{MacroRenderContext, expand_builtin_macros};
use crate::render::render_markdown_to_html;
use crate::scripting::{HostApi, UnavailableHost};
use crate::util::slugify;

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
    render_liquid_template_with_host(
        template_source,
        partials,
        context,
        Arc::new(UnavailableHost),
    )
}

pub(crate) fn render_liquid_template_with_host<T: Serialize>(
    template_source: &str,
    partials: &HashMap<String, String>,
    context: &T,
    host: Arc<dyn HostApi>,
) -> Result<String, RenderError> {
    let mut compiled_partials: EagerCompiler<InMemorySource> = EagerCompiler::empty();
    for (name, content) in partials {
        compiled_partials.add(format!("{name}.liquid"), content.clone());
    }

    let parser = ParserBuilder::with_stdlib()
        .filter(I18n)
        .filter(Markdown { host })
        .filter(Slugify)
        .partials(compiled_partials)
        .build()?;
    let template = parser.parse(template_source)?;
    let globals = liquid::to_object(context)?;
    Ok(template.render(&globals)?)
}

#[derive(Clone, ParseFilter, FilterReflection)]
#[filter(
    name = "slugify",
    description = "Convert text to the canonical post slug format.",
    parsed(SlugifyFilter)
)]
struct Slugify;

#[derive(Debug, Default, Display_filter)]
#[name = "slugify"]
struct SlugifyFilter;

impl Filter for SlugifyFilter {
    fn evaluate(
        &self,
        input: &dyn ValueView,
        _runtime: &dyn Runtime,
    ) -> liquid_core::Result<Value> {
        Ok(Value::scalar(slugify(input.to_kstr().as_str())))
    }
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
        Ok(Value::scalar(translate_render(
            language.as_str(),
            key.as_str(),
        )))
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

#[derive(Clone, FilterReflection)]
#[filter(
    name = "markdown",
    description = "Render markdown to HTML and rewrite preview URLs.",
    parameters(MarkdownArgs),
    parsed(MarkdownFilter)
)]
struct Markdown {
    host: Arc<dyn HostApi>,
}

impl ParseFilter for Markdown {
    fn parse(&self, args: FilterArguments<'_>) -> liquid_core::Result<Box<dyn Filter>> {
        Ok(Box::new(MarkdownFilter {
            args: MarkdownArgs::from_args(args)?,
            host: Arc::clone(&self.host),
        }))
    }

    fn reflection(&self) -> &dyn FilterReflection {
        self
    }
}

#[derive(Display_filter)]
#[name = "markdown"]
struct MarkdownFilter {
    args: MarkdownArgs,
    host: Arc<dyn HostApi>,
}

impl fmt::Debug for MarkdownFilter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MarkdownFilter")
            .finish_non_exhaustive()
    }
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
        let macro_context = MacroRenderContext {
            roots: collect_macro_roots(runtime),
            post_id: args
                .post_id
                .as_ref()
                .and_then(|value| value.as_scalar().map(|scalar| scalar.to_kstr().to_string()))
                .or_else(|| {
                    runtime
                        .try_get(&[ScalarCow::new("post"), ScalarCow::new("id")])
                        .and_then(|value| {
                            value.as_scalar().map(|scalar| scalar.to_kstr().to_string())
                        })
                }),
            host: Arc::clone(&self.host),
        };

        let expanded = expand_builtin_macros(markdown.as_str(), &macro_context);
        let rendered = render_markdown_to_html(&expanded);
        Ok(Value::scalar(rewrite_rendered_html_urls(
            &rendered,
            &rewrite_context,
        )))
    }
}

fn collect_macro_roots(runtime: &dyn Runtime) -> JsonMap<String, JsonValue> {
    let mut roots = JsonMap::new();

    for key in [
        "post",
        "post_data_json_by_id",
        "post_tags",
        "tag_color_by_name",
        "project",
        "Tags",
        "macro_scripts",
        "language",
        "language_prefix",
        "main_language",
        "is_preview",
        "translations",
    ] {
        if let Some(value) = runtime.try_get(&[ScalarCow::new(key)]) {
            roots.insert(key.to_string(), liquid_value_to_json(value.as_view()));
        }
    }

    if !roots.contains_key("Tags")
        && let Some(tags) = roots.get("post_tags").cloned()
    {
        roots.insert("Tags".to_string(), tags);
    }

    roots
}

fn liquid_value_to_json(value: &dyn ValueView) -> JsonValue {
    if value.is_nil() {
        return JsonValue::Null;
    }

    if let Some(scalar) = value.as_scalar() {
        if let Some(boolean) = scalar.to_bool() {
            return JsonValue::Bool(boolean);
        }
        if let Some(integer) = scalar.to_integer() {
            return JsonValue::from(integer);
        }
        if let Some(float) = scalar.to_float() {
            return JsonValue::from(float);
        }
        return JsonValue::String(scalar.to_kstr().to_string());
    }

    if let Some(array) = value.as_array() {
        return JsonValue::Array(array.values().map(liquid_value_to_json).collect());
    }

    if let Some(object) = value.as_object() {
        let mapped = object
            .iter()
            .map(|(key, value)| (key.to_string(), liquid_value_to_json(value)))
            .collect();
        return JsonValue::Object(mapped);
    }

    JsonValue::String(value.to_kstr().to_string())
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

pub(crate) fn rewrite_rendered_html_urls(
    html: &str,
    rewrite_context: &impl RewriteContextView,
) -> String {
    let rewritten = rewrite_attribute_urls(html, "href", |href| {
        normalize_preview_href(href, rewrite_context)
    });
    rewrite_attribute_urls(&rewritten, "src", |src| {
        normalize_preview_src(src, rewrite_context)
    })
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
        ["post" | "posts", year, month, slug]
            if year.len() == 4 && month.chars().all(|ch| ch.is_ascii_digit()) =>
        {
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
    use super::{
        RewriteContextView, render_liquid_template, render_liquid_template_with_host,
        rewrite_rendered_html_urls,
    };
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::scripting::HostApi;
    use serde_json::{Value, json};

    struct PostsHost;

    impl HostApi for PostsHost {
        fn call(
            &self,
            namespace: &str,
            method: &str,
            _arguments: Vec<Value>,
        ) -> Result<Value, String> {
            match (namespace, method) {
                ("posts", "get_all") => Ok(json!([{"id":"one"}, {"id":"two"}])),
                _ => Err("unsupported test capability".into()),
            }
        }
    }

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

    #[test]
    fn exposes_slugify_liquid_filter() {
        let rendered = render_liquid_template(
            "{{ title | slugify }}",
            &HashMap::new(),
            &serde_json::json!({"title": "Über die Brücke"}),
        )
        .unwrap();

        assert_eq!(rendered, "ueber-die-bruecke");
    }

    #[test]
    fn script_macros_use_the_supplied_project_host() {
        let rendered = render_liquid_template_with_host(
            "{{ post.content | markdown }}",
            &HashMap::new(),
            &json!({
                "post": {"id": "post-1", "content": "[[count]]"},
                "macro_scripts": {
                    "count": {
                        "source": "function render() return tostring(#bds.posts.get_all()) end",
                        "entrypoint": "render"
                    }
                }
            }),
            Arc::new(PostsHost),
        )
        .unwrap();

        assert_eq!(rendered, "<p>2</p>\n");
    }
}
