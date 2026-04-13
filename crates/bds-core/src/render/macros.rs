use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Value as JsonValue};

#[derive(Debug, Clone, Default)]
pub(crate) struct MacroRenderContext {
    pub roots: Map<String, JsonValue>,
    pub post_id: Option<String>,
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
        "youtube" => Some(render_youtube(&args)),
        "vimeo" => Some(render_vimeo(&args)),
        "photo_archive" => Some(render_photo_archive(&args, context)),
        "tag_cloud" => Some(render_tag_cloud(&args, context)),
        _ => Some(unsupported_macro(name)),
    }
}

fn unsupported_macro(name: &str) -> String {
    format!(
        "<section class=\"macro-unsupported\"><p>Unsupported macro: <code>{}</code></p></section>",
        escape_html_attr(name),
    )
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
    if trimmed.len() >= 2 {
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            return JsonValue::String(trimmed[1..trimmed.len() - 1].to_string());
        }
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
    let images = args
        .get("images")
        .and_then(JsonValue::as_array)
        .cloned()
        .or_else(|| linked_media(context));

    let Some(images) = images else {
        return empty_block(
            &format!("macro-gallery gallery-cols-{columns}"),
            "gallery-empty",
            "No media available.",
        );
    };
    if images.is_empty() {
        return empty_block(
            &format!("macro-gallery gallery-cols-{columns}"),
            "gallery-empty",
            "No media available.",
        );
    }

    let gallery_id = context.post_id.as_deref().unwrap_or("gallery");
    let mut html = format!(
        "<section class=\"macro-gallery gallery-cols-{columns}\"><div class=\"gallery-container\">"
    );
    for image in images {
        let Some(path) = image_path(&image) else {
            continue;
        };
        let title = image_title(&image);
        let alt = image_alt(&image, title.as_deref());
        html.push_str(&format!(
            "<a class=\"gallery-item\" href=\"{}\" data-lightbox=\"{}\"{}><img src=\"{}\" alt=\"{}\" loading=\"lazy\" /></a>",
            escape_html_attr(&path),
            escape_html_attr(gallery_id),
            title
                .as_deref()
                .map(|value| format!(" data-title=\"{}\"", escape_html_attr(value)))
                .unwrap_or_default(),
            escape_html_attr(&path),
            escape_html_attr(&alt),
        ));
    }
    html.push_str("</div></section>");
    html
}

fn render_youtube(args: &HashMap<String, JsonValue>) -> String {
    let video_id = args.get("id").map(stringify_scalar).unwrap_or_default();
    if video_id.is_empty() {
        return empty_block("macro-youtube", "gallery-empty", "Missing YouTube video id.");
    }
    format!(
        "<section class=\"macro-youtube\"><iframe src=\"https://www.youtube.com/embed/{}\" title=\"YouTube video\" loading=\"lazy\" allow=\"accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture\" allowfullscreen></iframe></section>",
        escape_html_attr(&video_id),
    )
}

fn render_vimeo(args: &HashMap<String, JsonValue>) -> String {
    let video_id = args.get("id").map(stringify_scalar).unwrap_or_default();
    if video_id.is_empty() {
        return empty_block("macro-vimeo", "gallery-empty", "Missing Vimeo video id.");
    }
    format!(
        "<section class=\"macro-vimeo\"><iframe src=\"https://player.vimeo.com/video/{}\" title=\"Vimeo video\" loading=\"lazy\" allow=\"autoplay; fullscreen; picture-in-picture\" allowfullscreen></iframe></section>",
        escape_html_attr(&video_id),
    )
}

fn render_photo_archive(args: &HashMap<String, JsonValue>, context: &MacroRenderContext) -> String {
    let media = args
        .get("media")
        .and_then(JsonValue::as_array)
        .cloned()
        .or_else(|| linked_media(context));

    let Some(media) = media else {
        return empty_block("macro-photo-archive", "photo-archive-empty", "No photos available.");
    };
    if media.is_empty() {
        return empty_block("macro-photo-archive", "photo-archive-empty", "No photos available.");
    }

    let mut grouped = BTreeMap::<String, Vec<JsonValue>>::new();
    for item in media {
        let bucket = item
            .get("file_path")
            .and_then(JsonValue::as_str)
            .and_then(month_bucket)
            .unwrap_or_else(|| "Archive".to_string());
        grouped.entry(bucket).or_default().push(item);
    }

    let single_month = grouped.len() == 1;
    let mut html = format!(
        "<section class=\"macro-photo-archive{}\"><div class=\"photo-archive-container\">",
        if single_month { " photo-archive-single-month" } else { "" }
    );
    for (bucket, items) in grouped.into_iter().rev() {
        html.push_str("<section class=\"photo-archive-month\">");
        html.push_str(&format!(
            "<header class=\"photo-archive-month-label\"><span>{}</span></header>",
            escape_html(&bucket),
        ));
        html.push_str("<div class=\"photo-archive-gallery\">");
        for item in items {
            let Some(path) = image_path(&item) else {
                continue;
            };
            let title = image_title(&item);
            let alt = image_alt(&item, title.as_deref());
            html.push_str(&format!(
                "<a class=\"photo-archive-item\" href=\"{}\"{}><img src=\"{}\" alt=\"{}\" loading=\"lazy\" /></a>",
                escape_html_attr(&path),
                title
                    .as_deref()
                    .map(|value| format!(" data-title=\"{}\"", escape_html_attr(value)))
                    .unwrap_or_default(),
                escape_html_attr(&path),
                escape_html_attr(&alt),
            ));
        }
        html.push_str("</div></section>");
    }
    html.push_str("</div></section>");
    html
}

fn render_tag_cloud(args: &HashMap<String, JsonValue>, context: &MacroRenderContext) -> String {
    let tags = args
        .get("tags")
        .and_then(JsonValue::as_array)
        .cloned()
        .or_else(|| tag_items(context));

    let Some(tags) = tags else {
        return empty_block("macro-tag-cloud", "tag-cloud-empty", "No tags available.");
    };
    if tags.is_empty() {
        return empty_block("macro-tag-cloud", "tag-cloud-empty", "No tags available.");
    }

    let words = build_tag_cloud_words(&tags, context);
    if words.is_empty() {
        return empty_block("macro-tag-cloud", "tag-cloud-empty", "No tags available.");
    }

    let words_json = serde_json::to_string(&words).unwrap_or_else(|_| "[]".to_string());
    format!(
        "<section class=\"macro-tag-cloud\" data-tag-cloud=\"true\" data-color-distribution=\"quantile\" data-color-theme=\"pico\" data-color-easing=\"0.7\" data-width=\"900\" data-height=\"420\" data-orientation=\"mixed-diagonal\" data-tag-cloud-words='{}'><svg class=\"tag-cloud-canvas\" aria-label=\"Tag cloud\"></svg></section>",
        escape_html_attr(&words_json),
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

    tags.iter()
        .filter_map(|tag| {
            let name = tag.get("name").and_then(JsonValue::as_str)?;
            let slug = tag
                .get("slug")
                .and_then(JsonValue::as_str)
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| name.to_lowercase().replace(' ', "-"));
            let count = tag.get("post_count").and_then(value_as_u64).unwrap_or(1);
            let size = if max_count == min_count {
                36.0
            } else {
                18.0 + (((count - min_count) as f64 / (max_count - min_count) as f64) * 38.0)
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
            Some(serde_json::json!({
                "text": name,
                "count": count,
                "url": format!("/tag/{slug}"),
                "size": size.round(),
                "color": color,
            }))
        })
        .collect()
}

fn linked_media(context: &MacroRenderContext) -> Option<Vec<JsonValue>> {
    lookup_path("post.linked_media", context).and_then(|value| value.as_array().cloned())
}

fn tag_items(context: &MacroRenderContext) -> Option<Vec<JsonValue>> {
    lookup_path("post_tags", context)
        .and_then(|value| value.as_array().cloned())
        .or_else(|| lookup_path("Tags", context).and_then(|value| value.as_array().cloned()))
}

fn image_path(image: &JsonValue) -> Option<String> {
    image.get("file_path").and_then(JsonValue::as_str).map(ToOwned::to_owned)
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
}

fn image_alt(image: &JsonValue, fallback: Option<&str>) -> String {
    image
        .get("alt")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| fallback.map(ToOwned::to_owned))
        .unwrap_or_default()
}

fn value_as_u64(value: &JsonValue) -> Option<u64> {
    value.as_u64().or_else(|| value.as_i64().map(|number| number.max(0) as u64))
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

fn month_bucket(path: &str) -> Option<String> {
    let segments = path.trim_matches('/').split('/').collect::<Vec<_>>();
    match segments.as_slice() {
        ["media", year, month, ..] if year.len() == 4 && month.len() == 2 => Some(format!("{year}-{month}")),
        _ => None,
    }
}

fn empty_block(wrapper_class: &str, message_class: &str, message: &str) -> String {
    format!(
        "<section class=\"{}\"><p class=\"{}\">{}</p></section>",
        wrapper_class,
        message_class,
        escape_html(message),
    )
}

fn escape_html(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn escape_html_attr(value: &str) -> String {
    escape_html(value).replace('"', "&quot;").replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::{MacroRenderContext, expand_builtin_macros};

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
            },
        );

        assert!(html.contains("macro-gallery gallery-cols-2"));
        assert!(html.contains("data-lightbox=\"post-1\""));
        assert!(html.contains("macro-tag-cloud"));
        assert!(html.contains("data-tag-cloud=\"true\""));
    }
}