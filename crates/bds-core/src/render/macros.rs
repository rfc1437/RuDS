use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use chrono::{Datelike, TimeZone, Utc};
use serde_json::{Map, Value as JsonValue};

use crate::i18n::translate_render;

const GALLERY_TEMPLATE: &str =
    include_str!("../../../../assets/starter-templates/macros/gallery.liquid");
const YOUTUBE_TEMPLATE: &str =
    include_str!("../../../../assets/starter-templates/macros/youtube.liquid");
const VIMEO_TEMPLATE: &str =
    include_str!("../../../../assets/starter-templates/macros/vimeo.liquid");
const PHOTO_ARCHIVE_TEMPLATE: &str =
    include_str!("../../../../assets/starter-templates/macros/photo-archive.liquid");
const TAG_CLOUD_TEMPLATE: &str =
    include_str!("../../../../assets/starter-templates/macros/tag-cloud.liquid");

#[derive(Clone)]
pub(crate) struct MacroRenderContext {
    pub roots: Map<String, JsonValue>,
    pub post_id: Option<String>,
    pub host: Arc<dyn crate::scripting::HostApi>,
}

impl Default for MacroRenderContext {
    fn default() -> Self {
        Self {
            roots: Map::new(),
            post_id: None,
            host: Arc::new(crate::scripting::UnavailableHost),
        }
    }
}

pub(crate) fn expand_builtin_macros(markdown: &str, context: &MacroRenderContext) -> String {
    let mut expanded = String::with_capacity(markdown.len());
    let mut cursor = 0;

    while let Some(offset) = markdown[cursor..].find("[[") {
        let start = cursor + offset;
        expanded.push_str(&markdown[cursor..start]);

        let body_start = start + 2;
        let Some(end_offset) = markdown[body_start..].find("]]") else {
            expanded.push_str(&markdown[start..]);
            return expanded;
        };
        let end = body_start + end_offset;
        let invocation = &markdown[body_start..end];
        let raw = &markdown[start..end + 2];

        if let Some(rendered) = render_macro(invocation, context) {
            expanded.push_str(&rendered);
        } else {
            expanded.push_str(raw);
        }
        cursor = end + 2;
    }

    expanded.push_str(&markdown[cursor..]);
    expanded
}

fn render_macro(invocation: &str, context: &MacroRenderContext) -> Option<String> {
    let tokens = tokenize_invocation(invocation);
    let name = tokens.first()?.as_str();
    let mut args = HashMap::new();
    for token in tokens.iter().skip(1) {
        let (key, raw_value) = token.split_once('=')?;
        args.insert(key.to_string(), resolve_token(raw_value, context));
    }

    match name {
        "gallery" => Some(render_gallery(&args, context)),
        "youtube" => Some(render_youtube(&args, context)),
        "vimeo" => Some(render_vimeo(&args, context)),
        "photo_archive" => Some(render_photo_archive(&args, context)),
        "tag_cloud" => Some(render_tag_cloud(&args, context)),
        _ => render_script_macro(name, &args, context),
    }
}

fn render_script_macro(
    name: &str,
    args: &HashMap<String, JsonValue>,
    context: &MacroRenderContext,
) -> Option<String> {
    let definition = context.roots.get("macro_scripts")?.get(name)?;
    let source = definition.get("source")?.as_str()?;
    let entrypoint = definition
        .get("entrypoint")
        .and_then(JsonValue::as_str)
        .unwrap_or("render");
    let env = serde_json::json!({
        "isPreview": context.roots.get("is_preview").and_then(JsonValue::as_bool).unwrap_or(false),
        "mainLanguage": context.roots.get("main_language").and_then(JsonValue::as_str).unwrap_or("en"),
        "language": context.roots.get("language").and_then(JsonValue::as_str).unwrap_or("en"),
        "languagePrefix": context.roots.get("language_prefix").and_then(JsonValue::as_str).unwrap_or(""),
        "hook": "markdown",
        "source": { "kind": if context.post_id.is_some() { "post" } else { "page" } },
        "translations": context.roots.get("translations").cloned().unwrap_or(JsonValue::Array(Vec::new())),
    });
    let params = serde_json::to_value(args).ok()?;
    match crate::scripting::execute_many_with_host(
        source,
        entrypoint,
        &[params, env],
        crate::scripting::ExecutionKind::Macro,
        &crate::scripting::ExecutionControl::default(),
        Arc::clone(&context.host),
    ) {
        Ok(result) => Some(match result.value {
            JsonValue::Null => String::new(),
            JsonValue::String(value) => value,
            value => value.to_string(),
        }),
        Err(_) => Some(String::new()),
    }
}

fn tokenize_invocation(invocation: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;

    for ch in invocation.chars() {
        if let Some(active_quote) = quote {
            current.push(ch);
            if ch == active_quote {
                quote = None;
            }
            continue;
        }

        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                current.push(ch);
            }
            ' ' | '\n' | '\t' | '\r' => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        tokens.push(current);
    }

    tokens
}

fn resolve_token(raw: &str, context: &MacroRenderContext) -> JsonValue {
    let trimmed = raw.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        return JsonValue::String(trimmed[1..trimmed.len() - 1].to_string());
    }

    match trimmed {
        "true" => return JsonValue::Bool(true),
        "false" => return JsonValue::Bool(false),
        "null" => return JsonValue::Null,
        _ => {}
    }

    if let Ok(value) = trimmed.parse::<i64>() {
        return JsonValue::from(value);
    }
    if let Ok(value) = trimmed.parse::<f64>() {
        return JsonValue::from(value);
    }

    lookup_path(trimmed, context).unwrap_or_else(|| JsonValue::String(trimmed.to_string()))
}

fn lookup_path(path: &str, context: &MacroRenderContext) -> Option<JsonValue> {
    let mut segments = path.split('.');
    let first = segments.next()?;
    let mut current = context.roots.get(first)?.clone();

    for segment in segments {
        current = match current {
            JsonValue::Object(map) => map.get(segment)?.clone(),
            JsonValue::Array(values) => values.get(segment.parse::<usize>().ok()?)?.clone(),
            _ => return None,
        };
    }

    Some(current)
}

fn render_gallery(args: &HashMap<String, JsonValue>, context: &MacroRenderContext) -> String {
    let columns = args
        .get("columns")
        .and_then(value_as_u64)
        .map(|value| value.clamp(1, 6) as usize)
        .unwrap_or(3);
    let mut images = args
        .get("images")
        .and_then(JsonValue::as_array)
        .cloned()
        .or_else(|| linked_media(context))
        .unwrap_or_default()
        .into_iter()
        .filter(is_image)
        .collect::<Vec<_>>();
    images.sort_by(media_newest_first);
    let gallery_id = format!("gallery-{}", context.post_id.as_deref().unwrap_or_default());
    let items = images
        .into_iter()
        .filter_map(|image| {
            Some(serde_json::json!({
                "media_path": ensure_leading_slash(&image_path(&image)?),
                "title": image_title(&image).unwrap_or_default(),
                "alt": image_alt(&image),
                "group_name": gallery_id,
            }))
        })
        .collect::<Vec<_>>();

    render_macro_template(
        macro_template(context, "gallery", GALLERY_TEMPLATE),
        serde_json::json!({
            "columns": columns,
            "post_id": context.post_id.as_deref().unwrap_or_default(),
            "items": items,
            "caption": args.get("caption").map(stringify_scalar),
            "no_items_label": render_translation(context, "render.gallery.empty"),
        }),
    )
}

fn render_youtube(args: &HashMap<String, JsonValue>, context: &MacroRenderContext) -> String {
    render_macro_template(
        macro_template(context, "youtube", YOUTUBE_TEMPLATE),
        serde_json::json!({
            "id": args.get("id").map(stringify_scalar).unwrap_or_default(),
            "title": macro_title(args, context, "render.video.youtubeTitle"),
        }),
    )
}

fn render_vimeo(args: &HashMap<String, JsonValue>, context: &MacroRenderContext) -> String {
    render_macro_template(
        macro_template(context, "vimeo", VIMEO_TEMPLATE),
        serde_json::json!({
            "id": args.get("id").map(stringify_scalar).unwrap_or_default(),
            "title": macro_title(args, context, "render.video.vimeoTitle"),
        }),
    )
}

fn render_photo_archive(args: &HashMap<String, JsonValue>, context: &MacroRenderContext) -> String {
    let mut media = args
        .get("media")
        .and_then(JsonValue::as_array)
        .cloned()
        .or_else(|| project_media(context))
        .unwrap_or_default()
        .into_iter()
        .filter(is_image)
        .collect::<Vec<_>>();
    media.sort_by(media_newest_first);

    let year = args.get("year").and_then(value_as_i32);
    let month = args.get("month").and_then(value_as_u32);
    if args.contains_key("year") && year.is_none() {
        media.clear();
    } else if let Some(year) = year {
        media.retain(|item| {
            media_archive_month(item).is_some_and(|(item_year, item_month)| {
                item_year == year && month.is_none_or(|month| item_month == month)
            })
        });
    } else {
        media.truncate(200);
    }

    let mut grouped = BTreeMap::<(i32, u32), Vec<JsonValue>>::new();
    for item in media {
        if let Some(bucket) = media_archive_month(&item) {
            grouped.entry(bucket).or_default().push(item);
        }
    }
    let month_limit = if year.is_none() { 10 } else { usize::MAX };
    let months = grouped
        .into_iter()
        .rev()
        .take(month_limit)
        .map(|((year, month), items)| {
            let items = items
                .into_iter()
                .filter_map(|item| {
                    Some(serde_json::json!({
                        "media_path": ensure_leading_slash(&image_path(&item)?),
                        "title": image_archive_title(&item).unwrap_or_default(),
                        "alt": image_alt(&item),
                        "group_name": format!("{year}-{month}"),
                    }))
                })
                .collect::<Vec<_>>();
            serde_json::json!({
                "label": format!(
                    "{} {year}",
                    render_translation(context, &format!("render.month.{month}"))
                ),
                "items": items,
            })
        })
        .collect::<Vec<_>>();

    render_macro_template(
        macro_template(context, "photo-archive", PHOTO_ARCHIVE_TEMPLATE),
        serde_json::json!({
            "root_classes": "macro-photo-archive",
            "data_attrs": [],
            "months": months,
            "no_items_label": render_translation(context, "render.photoArchive.empty"),
        }),
    )
}

fn render_tag_cloud(args: &HashMap<String, JsonValue>, context: &MacroRenderContext) -> String {
    let tags = args
        .get("tags")
        .and_then(JsonValue::as_array)
        .cloned()
        .or_else(|| tag_items(context));

    let orientation = normalize_tag_cloud_orientation(args.get("orientation"));
    let width = tag_cloud_dimension(args.get("width"), 320, 1600, 900);
    let height = tag_cloud_dimension(args.get("height"), 180, 900, 420);
    let words_json = tags
        .as_deref()
        .map(|tags| build_tag_cloud_words(tags, context))
        .filter(|words| !words.is_empty())
        .and_then(|words| serde_json::to_string(&words).ok())
        .map(|words| escape_html_attr(&words));

    render_macro_template(
        macro_template(context, "tag-cloud", TAG_CLOUD_TEMPLATE),
        serde_json::json!({
            "orientation": orientation,
            "words_json": words_json,
            "width": width,
            "height": height,
            "aria_label": render_translation(context, "render.tagCloud.ariaLabel"),
            "no_items_label": render_translation(context, "render.tagCloud.empty"),
        }),
    )
}

fn build_tag_cloud_words(tags: &[JsonValue], context: &MacroRenderContext) -> Vec<JsonValue> {
    let min_count = tags
        .iter()
        .filter_map(|tag| tag.get("post_count").and_then(value_as_u64))
        .min()
        .unwrap_or(1);
    let max_count = tags
        .iter()
        .filter_map(|tag| tag.get("post_count").and_then(value_as_u64))
        .max()
        .unwrap_or(min_count.max(1));

    let mut words = tags
        .iter()
        .filter_map(|tag| {
            let name = tag.get("name").and_then(JsonValue::as_str)?;
            let encoded_name = encode_path_segment(name);
            let count = tag.get("post_count").and_then(value_as_u64).unwrap_or(1);
            let size = if max_count == min_count {
                35.0
            } else {
                14.0 + (((count - min_count) as f64 / (max_count - min_count) as f64) * 42.0)
            };
            let color = tag
                .get("color")
                .and_then(JsonValue::as_str)
                .map(ToOwned::to_owned)
                .or_else(|| {
                    context
                        .roots
                        .get("tag_color_by_name")
                        .and_then(JsonValue::as_object)
                        .and_then(|colors| colors.get(name))
                        .and_then(JsonValue::as_str)
                        .map(ToOwned::to_owned)
                });
            let mut word = Map::from_iter([
                ("text".into(), JsonValue::String(name.into())),
                ("count".into(), JsonValue::from(count)),
                (
                    "url".into(),
                    JsonValue::String(format!("/tag/{encoded_name}/")),
                ),
                ("size".into(), JsonValue::from(size.round() as u64)),
            ]);
            if let Some(color) = color.filter(|color| !color.is_empty()) {
                word.insert("color".into(), JsonValue::String(color));
            }
            Some(JsonValue::Object(word))
        })
        .collect::<Vec<_>>();
    words.sort_by(|left, right| {
        right["count"]
            .as_u64()
            .cmp(&left["count"].as_u64())
            .then_with(|| {
                left["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_lowercase()
                    .cmp(&right["text"].as_str().unwrap_or_default().to_lowercase())
            })
    });
    words
}

fn render_macro_template(template: &str, assigns: JsonValue) -> String {
    crate::render::render_liquid_template(template, &HashMap::new(), &assigns).unwrap_or_default()
}

fn macro_template<'a>(context: &'a MacroRenderContext, name: &str, bundled: &'a str) -> &'a str {
    context
        .roots
        .get("macro_templates")
        .and_then(JsonValue::as_object)
        .and_then(|templates| templates.get(name))
        .and_then(JsonValue::as_str)
        .unwrap_or(bundled)
}

pub(super) fn bundled_macro_templates() -> HashMap<String, String> {
    HashMap::from([
        ("gallery".into(), GALLERY_TEMPLATE.into()),
        ("youtube".into(), YOUTUBE_TEMPLATE.into()),
        ("vimeo".into(), VIMEO_TEMPLATE.into()),
        ("photo-archive".into(), PHOTO_ARCHIVE_TEMPLATE.into()),
        ("tag-cloud".into(), TAG_CLOUD_TEMPLATE.into()),
    ])
}

fn encode_path_segment(value: &str) -> String {
    use std::fmt::Write;

    value.bytes().fold(String::new(), |mut encoded, byte| {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            write!(encoded, "%{byte:02X}").expect("writing to a String cannot fail");
        }
        encoded
    })
}

fn linked_media(context: &MacroRenderContext) -> Option<Vec<JsonValue>> {
    lookup_path("post.linked_media", context)
        .and_then(|value| value.as_array().cloned())
        .or_else(|| {
            let post_id = context.post_id.as_deref()?;
            context
                .roots
                .get("post_data_json_by_id")?
                .get(post_id)?
                .get("linked_media")?
                .as_array()
                .cloned()
        })
}

fn project_media(context: &MacroRenderContext) -> Option<Vec<JsonValue>> {
    lookup_path("project.media", context).and_then(|value| value.as_array().cloned())
}

fn tag_items(context: &MacroRenderContext) -> Option<Vec<JsonValue>> {
    lookup_path("Tags", context)
        .and_then(|value| value.as_array().cloned())
        .or_else(|| lookup_path("post_tags", context).and_then(|value| value.as_array().cloned()))
}

fn is_image(media: &JsonValue) -> bool {
    media
        .get("mime_type")
        .and_then(JsonValue::as_str)
        .is_none_or(|mime_type| mime_type.starts_with("image/"))
}

fn media_newest_first(left: &JsonValue, right: &JsonValue) -> std::cmp::Ordering {
    right
        .get("created_at")
        .and_then(JsonValue::as_i64)
        .cmp(&left.get("created_at").and_then(JsonValue::as_i64))
        .then_with(|| {
            left.get("id")
                .and_then(JsonValue::as_str)
                .cmp(&right.get("id").and_then(JsonValue::as_str))
        })
}

fn media_archive_month(media: &JsonValue) -> Option<(i32, u32)> {
    media
        .get("created_at")
        .and_then(JsonValue::as_i64)
        .and_then(|timestamp| Utc.timestamp_millis_opt(timestamp).single())
        .map(|timestamp| (timestamp.year(), timestamp.month()))
        .or_else(|| {
            media
                .get("file_path")
                .and_then(JsonValue::as_str)
                .and_then(month_bucket)
        })
}

fn normalize_tag_cloud_orientation(value: Option<&JsonValue>) -> &'static str {
    match value
        .map(stringify_scalar)
        .unwrap_or_default()
        .trim()
        .to_lowercase()
        .as_str()
    {
        "mixed_hv" | "mixed-hv" | "hv" | "horizontal_vertical" => "mixed-hv",
        "mixed_diagonal" | "mixed-diagonal" | "diagonal" | "diag" => "mixed-diagonal",
        _ => "horizontal",
    }
}

fn tag_cloud_dimension(value: Option<&JsonValue>, min: u64, max: u64, default: u64) -> u64 {
    value
        .and_then(value_as_u64)
        .filter(|value| (min..=max).contains(value))
        .unwrap_or(default)
}

fn macro_title(
    args: &HashMap<String, JsonValue>,
    context: &MacroRenderContext,
    translation_key: &str,
) -> String {
    args.get("title")
        .map(stringify_scalar)
        .filter(|title| !title.trim().is_empty())
        .unwrap_or_else(|| render_translation(context, translation_key))
}

fn render_translation(context: &MacroRenderContext, key: &str) -> String {
    let language = context
        .roots
        .get("language")
        .and_then(JsonValue::as_str)
        .unwrap_or("en");
    translate_render(language, key)
}

fn image_path(image: &JsonValue) -> Option<String> {
    image
        .get("file_path")
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned)
}

fn ensure_leading_slash(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

fn image_title(image: &JsonValue) -> Option<String> {
    image
        .get("caption")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            image
                .get("title")
                .and_then(JsonValue::as_str)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| image_name(image))
}

fn image_archive_title(image: &JsonValue) -> Option<String> {
    image
        .get("title")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| image_name(image))
}

fn image_alt(image: &JsonValue) -> String {
    image
        .get("alt")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            image
                .get("title")
                .and_then(JsonValue::as_str)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .or_else(|| image_name(image))
        .unwrap_or_default()
}

fn image_name(image: &JsonValue) -> Option<String> {
    image
        .get("original_name")
        .or_else(|| image.get("filename"))
        .and_then(JsonValue::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn value_as_u64(value: &JsonValue) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| number.try_into().ok()))
        .or_else(|| value.as_str().and_then(|number| number.parse().ok()))
}

fn value_as_u32(value: &JsonValue) -> Option<u32> {
    value_as_u64(value)
        .and_then(|number| number.try_into().ok())
        .filter(|month| (1..=12).contains(month))
}

fn value_as_i32(value: &JsonValue) -> Option<i32> {
    value
        .as_i64()
        .and_then(|number| number.try_into().ok())
        .or_else(|| value.as_str().and_then(|number| number.parse().ok()))
}

fn stringify_scalar(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => String::new(),
        JsonValue::Bool(boolean) => boolean.to_string(),
        JsonValue::Number(number) => number.to_string(),
        JsonValue::String(text) => text.clone(),
        JsonValue::Array(_) | JsonValue::Object(_) => value.to_string(),
    }
}

fn month_bucket(path: &str) -> Option<(i32, u32)> {
    let segments = path.trim_matches('/').split('/').collect::<Vec<_>>();
    match segments.as_slice() {
        ["media", year, month, ..] if year.len() == 4 && month.len() == 2 => {
            Some((year.parse().ok()?, month.parse().ok()?))
        }
        _ => None,
    }
}

fn escape_html_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::{
        GALLERY_TEMPLATE, MacroRenderContext, PHOTO_ARCHIVE_TEMPLATE, TAG_CLOUD_TEMPLATE,
        VIMEO_TEMPLATE, YOUTUBE_TEMPLATE, expand_builtin_macros, render_macro_template,
    };

    fn compact_html(html: &str) -> String {
        html.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .replace("> <", "><")
    }

    #[test]
    fn bundled_macro_templates_use_supported_liquid_syntax() {
        for (name, template) in [
            ("gallery", GALLERY_TEMPLATE),
            ("youtube", YOUTUBE_TEMPLATE),
            ("vimeo", VIMEO_TEMPLATE),
            ("photo-archive", PHOTO_ARCHIVE_TEMPLATE),
            ("tag-cloud", TAG_CLOUD_TEMPLATE),
        ] {
            let result = crate::render::page_renderer::validate_liquid_template_syntax(template);
            assert!(
                result.is_ok(),
                "{name} macro template must parse: {result:?}"
            );
        }
    }

    #[test]
    fn macro_template_assignments_do_not_escape_the_isolated_scope() {
        let assigns = serde_json::json!({"visible": "outside"});
        let rendered = render_macro_template(
            "{{ visible }}|{% assign visible = 'inside' %}{{ visible }}",
            assigns.clone(),
        );

        assert_eq!(rendered, "outside|inside");
        assert_eq!(assigns["visible"], "outside");
    }

    #[test]
    fn expands_gallery_and_tag_cloud_macros() {
        let mut roots = serde_json::Map::new();
        roots.insert(
            "post".to_string(),
            serde_json::json!({
                "linked_media": [
                    {"file_path": "/media/2026/04/one.jpg", "title": "One", "alt": "Alt one"},
                    {"file_path": "/media/2026/04/two.jpg", "caption": "Two"}
                ]
            }),
        );
        roots.insert(
            "post_tags".to_string(),
            serde_json::json!([
                {"name": "Rust", "slug": "rust", "post_count": 4, "color": "#ff6600"},
                {"name": "Iced", "slug": "iced", "post_count": 2}
            ]),
        );
        roots.insert(
            "tag_color_by_name".to_string(),
            serde_json::json!({"Iced": "#0088cc"}),
        );

        let html = expand_builtin_macros(
            "[[gallery images=post.linked_media columns=2]]\n\n[[tag_cloud tags=post_tags]]",
            &MacroRenderContext {
                roots,
                post_id: Some("post-1".to_string()),
                ..MacroRenderContext::default()
            },
        );

        assert!(html.contains("macro-gallery gallery-cols-2"));
        assert!(html.contains("data-lightbox=\"gallery-post-1\""));
        assert!(html.contains("macro-tag-cloud"));
        assert!(html.contains("data-tag-cloud=\"true\""));
    }

    #[test]
    fn built_in_macros_match_the_bds2_template_markup() {
        let mut roots = serde_json::Map::new();
        roots.insert("language".into(), serde_json::json!("en"));
        roots.insert(
            "post".into(),
            serde_json::json!({
                "linked_media": [{
                    "id": "image-1",
                    "file_path": "/media/2026/04/one.jpg",
                    "mime_type": "image/jpeg",
                    "title": "One & only",
                    "alt": "Alt <one>"
                }]
            }),
        );
        roots.insert(
            "project".into(),
            serde_json::json!({
                "media": [{
                    "id": "image-1",
                    "file_path": "/media/2026/04/one.jpg",
                    "mime_type": "image/jpeg",
                    "title": "One & only",
                    "alt": "Alt <one>"
                }]
            }),
        );
        roots.insert(
            "post_tags".into(),
            serde_json::json!([
                {"name": "Rust & SQLite", "slug": "ignored", "post_count": 4, "color": "#ff6600"},
                {"name": "Iced", "slug": "also-ignored", "post_count": 2}
            ]),
        );
        let context = MacroRenderContext {
            roots,
            post_id: Some("post-1".into()),
            ..MacroRenderContext::default()
        };

        let youtube = compact_html(&expand_builtin_macros("[[youtube id=abc123]]", &context));
        assert_eq!(
            youtube,
            compact_html(
                "<div class=\"macro-youtube\"> <iframe src=\"https://www.youtube.com/embed/abc123?rel=0\" title=\"YouTube video\" frameborder=\"0\" allow=\"accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture\" allowfullscreen ></iframe> </div>"
            )
        );

        let vimeo = compact_html(&expand_builtin_macros("[[vimeo id=98765]]", &context));
        assert_eq!(
            vimeo,
            compact_html(
                "<div class=\"macro-vimeo\"> <iframe src=\"https://player.vimeo.com/video/98765\" title=\"Vimeo video\" frameborder=\"0\" allow=\"autoplay; fullscreen; picture-in-picture\" allowfullscreen ></iframe> </div>"
            )
        );

        let gallery = compact_html(&expand_builtin_macros(
            "[[gallery columns=2 caption='A caption']]",
            &context,
        ));
        assert_eq!(
            gallery,
            compact_html(
                "<div class=\"macro-gallery gallery-cols-2\" data-post-id=\"post-1\" data-columns=\"2\" data-lightbox=\"true\" > <div class=\"gallery-container gallery-lightbox\"><a class=\"gallery-item\" href=\"/media/2026/04/one.jpg\" data-lightbox=\"gallery-post-1\" data-title=\"One &amp; only\" > <img src=\"/media/2026/04/one.jpg\" alt=\"Alt &lt;one&gt;\" loading=\"lazy\" /> </a></div><figcaption class=\"gallery-caption\">A caption</figcaption></div>"
            )
        );

        let archive = compact_html(&expand_builtin_macros("[[photo_archive]]", &context));
        assert_eq!(
            archive,
            compact_html(
                "<div class=\"macro-photo-archive\"> <div class=\"photo-archive-container\"><div class=\"photo-archive-month-wrapper\"> <div class=\"photo-archive-month\"> <div class=\"photo-archive-month-label\"> <span>April 2026</span> </div> <div class=\"photo-archive-gallery\"><a class=\"photo-archive-item\" href=\"/media/2026/04/one.jpg\" data-lightbox=\"2026-4\" data-title=\"One &amp; only\" > <img src=\"/media/2026/04/one.jpg\" alt=\"Alt &lt;one&gt;\" loading=\"lazy\" /> </a></div> </div> </div></div> </div>"
            )
        );

        let tag_cloud = compact_html(&expand_builtin_macros(
            "[[tag_cloud orientation=diagonal width=640 height=300]]",
            &context,
        ));
        assert!(tag_cloud.starts_with(
            "<div class=\"macro-tag-cloud\" data-tag-cloud=\"true\" data-orientation=\"mixed-diagonal\" data-color-distribution=\"quantile\" data-color-easing=\"0.7\" data-color-theme=\"pico\" data-tag-cloud-words=\""
        ), "{tag_cloud}");
        assert!(tag_cloud.contains("&quot;url&quot;:&quot;/tag/Rust%20%26%20SQLite/&quot;"));
        assert!(tag_cloud.contains("data-width=\"640\" data-height=\"300\""));
        assert!(tag_cloud.ends_with(
            &compact_html("> <svg class=\"tag-cloud-canvas\" viewBox=\"0 0 640 300\" preserveAspectRatio=\"xMidYMid meet\" aria-label=\"Tag cloud\" ></svg></div>")
        ));
    }

    #[test]
    fn video_macros_with_missing_ids_still_use_the_localized_templates() {
        for (language, youtube_title, vimeo_title) in [
            ("en", "YouTube video", "Vimeo video"),
            ("de", "YouTube-Video", "Vimeo-Video"),
            ("fr", "Vidéo YouTube", "Vidéo Vimeo"),
            ("it", "Video YouTube", "Video Vimeo"),
            ("es", "Vídeo de YouTube", "Vídeo de Vimeo"),
        ] {
            let mut roots = serde_json::Map::new();
            roots.insert("language".into(), serde_json::json!(language));
            let context = MacroRenderContext {
                roots,
                ..MacroRenderContext::default()
            };
            let rendered = expand_builtin_macros("[[youtube]] [[vimeo]]", &context);

            assert!(rendered.contains("https://www.youtube.com/embed/?rel=0"));
            assert!(rendered.contains("https://player.vimeo.com/video/"));
            assert!(rendered.contains(&format!("title=\"{youtube_title}\"")));
            assert!(rendered.contains(&format!("title=\"{vimeo_title}\"")));
            assert!(!rendered.contains("Missing"));
        }
    }

    #[test]
    fn empty_macro_states_use_render_domain_translations() {
        let mut roots = serde_json::Map::new();
        roots.insert("language".into(), serde_json::json!("de"));
        let context = MacroRenderContext {
            roots,
            ..MacroRenderContext::default()
        };

        let rendered =
            expand_builtin_macros("[[gallery]] [[photo_archive]] [[tag_cloud]]", &context);

        assert!(rendered.contains("Keine verknüpften Bilder gefunden."));
        assert!(rendered.contains("Keine Fotos für dieses Archiv gefunden."));
        assert!(rendered.contains("Keine Tags gefunden."));
    }

    #[test]
    fn project_macro_templates_override_the_bundled_defaults() {
        let mut roots = serde_json::Map::new();
        roots.insert(
            "macro_templates".into(),
            serde_json::json!({
                "gallery": "<aside data-columns=\"{{ columns }}\">custom gallery</aside>"
            }),
        );

        let rendered = expand_builtin_macros(
            "[[gallery columns=4]]",
            &MacroRenderContext {
                roots,
                ..MacroRenderContext::default()
            },
        );

        assert_eq!(rendered, "<aside data-columns=\"4\">custom gallery</aside>");
    }

    #[test]
    fn template_rendering_preserves_macro_clamps_aliases_and_archive_filters() {
        let mut roots = serde_json::Map::new();
        roots.insert(
            "project".into(),
            serde_json::json!({
                "media": [
                    {"file_path": "/media/2026/04/new.jpg", "mime_type": "image/jpeg"},
                    {"file_path": "/media/2025/03/old.jpg", "mime_type": "image/jpeg"}
                ]
            }),
        );
        roots.insert(
            "post_tags".into(),
            serde_json::json!([{"name": "Rust", "post_count": 1}]),
        );
        let context = MacroRenderContext {
            roots,
            ..MacroRenderContext::default()
        };

        let rendered = expand_builtin_macros(
            "[[gallery columns=99]] [[tag_cloud orientation=hv width=99999 height=1]] [[photo_archive year=2025 month=3]]",
            &context,
        );

        assert!(rendered.contains("gallery-cols-6"));
        assert!(rendered.contains("data-orientation=\"mixed-hv\""));
        assert!(rendered.contains("data-width=\"900\" data-height=\"420\""));
        assert!(rendered.contains("/media/2025/03/old.jpg"));
        assert!(!rendered.contains("/media/2026/04/new.jpg"));
    }

    #[test]
    fn leaves_unknown_macros_verbatim() {
        let markdown = "Before [[future_macro value='x']] after";
        assert_eq!(
            expand_builtin_macros(markdown, &MacroRenderContext::default()),
            markdown
        );
    }

    #[test]
    fn script_macro_receives_named_params_and_environment() {
        let mut roots = serde_json::Map::new();
        roots.insert(
            "macro_scripts".into(),
            serde_json::json!({
                "notice": {
                    "source": "function render(params, env) return '<aside>' .. params.text .. ':' .. env.language .. '</aside>' end",
                    "entrypoint": "render"
                }
            }),
        );
        roots.insert("language".into(), serde_json::Value::String("de".into()));
        let rendered = expand_builtin_macros(
            "[[notice text=Hallo]]",
            &MacroRenderContext {
                roots,
                post_id: Some("post-1".into()),
                ..MacroRenderContext::default()
            },
        );
        assert_eq!(rendered, "<aside>Hallo:de</aside>");
    }
}
