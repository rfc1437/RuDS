use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use base64::Engine as _;
use diesel::prelude::*;
use serde_json::{Value, json};

use crate::db::DbConnection as Connection;
use crate::db::queries::{media, post, post_link, post_media, script, template};
use crate::db::schema::ai_models;
use crate::engine::{EngineError, EngineResult, chat_surfaces};
use crate::util::frontmatter::read_post_file;

pub fn model_supports_tools(conn: &Connection, model: &str) -> EngineResult<bool> {
    let catalog_value = conn.with(|connection| {
        ai_models::table
            .filter(ai_models::model_id.eq(model))
            .select(ai_models::tool_call)
            .first::<i32>(connection)
            .optional()
    })?;
    Ok(catalog_value.map_or_else(
        || {
            let model = model.to_ascii_lowercase();
            model.contains("gpt")
                || model.contains("claude")
                || model.contains("tool")
                || model.contains("qwen")
                || model.contains("mistral")
        },
        |value| value != 0,
    ))
}

pub fn system_prompt(conn: &Connection, project_id: &str) -> EngineResult<String> {
    let posts = post::list_posts_by_project(conn, project_id)?;
    let media_count = media::count_media_by_project(conn, project_id)?;
    let tags = posts
        .iter()
        .flat_map(|post| post.tags.iter())
        .collect::<BTreeSet<_>>()
        .len();
    let categories = posts
        .iter()
        .flat_map(|post| post.categories.iter())
        .collect::<BTreeSet<_>>()
        .len();
    let configured = crate::engine::settings::get(conn, "ai.system_prompt")?.unwrap_or_default();
    let contract = format!(
        "You are the conversational assistant for this blog project. Use tools when facts from the project are needed. Never invent project content or identifiers. There are {} posts, {media_count} media items, {tags} tags, and {categories} categories. Keep answers concise and use GitHub-flavored Markdown when useful. Use render tools for structured data, comparisons, and forms; their payloads are native UI data, never HTML or JavaScript.",
        posts.len()
    );
    Ok(if configured.trim().is_empty() {
        contract
    } else {
        format!("{}\n\n{contract}", configured.trim())
    })
}

pub fn tool_specs() -> Vec<Value> {
    vec![
        spec(
            "get_blog_stats",
            "Return aggregate project statistics.",
            json!({}),
        ),
        spec(
            "check_term",
            "Check whether a term is used as a tag or category.",
            json!({"term": {"type": "string"}}),
        ),
        spec(
            "search_posts",
            "Search post titles, slugs, excerpts, bodies, tags, and categories.",
            json!({"query": {"type": "string"}, "language": {"type": "string"}, "limit": {"type": "integer"}}),
        ),
        spec(
            "read_post",
            "Read one post by id.",
            json!({"post_id": {"type": "string"}}),
        ),
        spec(
            "read_post_by_slug",
            "Read one post by slug.",
            json!({"slug": {"type": "string"}}),
        ),
        spec(
            "list_posts",
            "List posts, optionally filtering by status, tag, or category.",
            json!({"status": {"type": "string"}, "tag": {"type": "string"}, "category": {"type": "string"}, "limit": {"type": "integer"}}),
        ),
        spec(
            "count_posts",
            "Count posts and optionally group by status, tag, or category.",
            json!({"group_by": {"type": "string", "enum": ["status", "tag", "category"]}}),
        ),
        spec(
            "update_post_metadata",
            "Update title, excerpt, tags, or categories on a post.",
            json!({"post_id": {"type": "string"}, "title": {"type": "string"}, "excerpt": {"type": ["string", "null"]}, "tags": {"type": "array", "items": {"type": "string"}}, "categories": {"type": "array", "items": {"type": "string"}}}),
        ),
        spec(
            "list_media",
            "List project media.",
            json!({"limit": {"type": "integer"}}),
        ),
        spec(
            "get_media",
            "Get one media item.",
            json!({"media_id": {"type": "string"}}),
        ),
        spec(
            "view_image",
            "Return a local image thumbnail as a data URL for visual inspection.",
            json!({"media_id": {"type": "string"}, "size": {"type": "string", "enum": ["small", "medium", "large"]}}),
        ),
        spec(
            "update_media_metadata",
            "Update title, alt text, caption, or tags on media.",
            json!({"media_id": {"type": "string"}, "title": {"type": ["string", "null"]}, "alt": {"type": ["string", "null"]}, "caption": {"type": ["string", "null"]}, "tags": {"type": "array", "items": {"type": "string"}}}),
        ),
        spec("list_tags", "List tags and usage counts.", json!({})),
        spec(
            "list_categories",
            "List categories and usage counts.",
            json!({}),
        ),
        spec(
            "get_post_backlinks",
            "Get posts that link to a post.",
            json!({"post_id": {"type": "string"}}),
        ),
        spec(
            "get_post_outlinks",
            "Get posts linked from a post.",
            json!({"post_id": {"type": "string"}}),
        ),
        spec(
            "get_post_media",
            "Get media linked to a post.",
            json!({"post_id": {"type": "string"}}),
        ),
        spec(
            "get_media_posts",
            "Get posts that use a media item.",
            json!({"media_id": {"type": "string"}}),
        ),
        spec("list_templates", "List templates.", json!({})),
        spec(
            "read_template",
            "Read a template by id.",
            json!({"template_id": {"type": "string"}}),
        ),
        spec("list_scripts", "List scripts.", json!({})),
        spec(
            "read_script",
            "Read a script by id.",
            json!({"script_id": {"type": "string"}}),
        ),
        spec(
            "navigate",
            "Open a project area or entity in the application.",
            json!({
                "action": {"type": "string", "enum": ["open_post", "open_media", "open_settings", "open_chat", "switch_view", "toggle_sidebar", "toggle_panel", "toggle_assistant_sidebar"]},
                "destination": {"type": "string", "enum": ["posts", "pages", "media", "templates", "scripts", "tags", "chat", "import", "git", "settings"]},
                "entity_id": {"type": "string"},
                "value": {"type": "string"}
            }),
        ),
        spec(
            "render_card",
            "Render an information card with optional allow-listed actions.",
            json!({
                "title": {"type": "string"}, "subtitle": {"type": "string"}, "body": {"type": "string"},
                "actions": {"type": "array", "items": {"type": "object", "properties": {
                    "label": {"type": "string"}, "action": {"type": "string"}, "payload": {"type": "object"}
                }, "required": ["label", "action"]}}
            }),
        ),
        spec(
            "render_chart",
            "Render a native chart; heatmap and stacked-bar series use labelled segments.",
            json!({
                "chartType": {"type": "string", "enum": ["bar", "stacked-bar", "line", "area", "pie", "donut", "heatmap"]},
                "chart_type": {"type": "string", "enum": ["bar", "stacked-bar", "line", "area", "pie", "donut", "heatmap"]},
                "title": {"type": "string"},
                "series": {"type": "array", "items": {"type": "object", "properties": {
                    "label": {"type": "string"}, "value": {"type": "number"},
                    "segments": {"type": "array", "items": {"type": "object", "properties": {
                        "label": {"type": "string"}, "value": {"type": "number"}
                    }, "required": ["label", "value"]}}
                }, "required": ["label"]}}
            }),
        ),
        spec(
            "render_form",
            "Render a native form that submits its current values with an allow-listed action.",
            json!({
                "title": {"type": "string"},
                "fields": {"type": "array", "items": {"type": "object", "properties": {
                    "key": {"type": "string"}, "label": {"type": "string"},
                    "inputType": {"type": "string", "enum": ["text", "textarea", "select", "checkbox", "date", "number"]},
                    "input_type": {"type": "string", "enum": ["text", "textarea", "select", "checkbox", "date", "number"]},
                    "placeholder": {"type": "string"}, "defaultValue": {}, "default_value": {}, "required": {"type": "boolean"},
                    "options": {"type": "array", "items": {"type": "object", "properties": {
                        "label": {"type": "string"}, "value": {"type": "string"}
                    }}}
                }, "required": ["key", "label", "inputType"]}},
                "submitLabel": {"type": "string"}, "submit_label": {"type": "string"},
                "submitAction": {"type": "string"}, "submit_action": {"type": "string"}
            }),
        ),
        spec(
            "render_list",
            "Render a native list.",
            json!({"title": {"type": "string"}, "items": {"type": "array", "items": {"type": "string"}}}),
        ),
        spec(
            "render_metric",
            "Render a prominent metric.",
            json!({"label": {"type": "string"}, "value": {"type": "string"}}),
        ),
        spec(
            "render_mindmap",
            "Render a native hierarchical mind map.",
            json!({"title": {"type": "string"}, "nodes": {"type": "array", "items": {"type": "object", "properties": {
                "id": {"type": "string"}, "label": {"type": "string"}, "children": {"type": "array", "items": {"type": "string"}}
            }, "required": ["label"]}}}),
        ),
        spec(
            "render_table",
            "Render a native data table.",
            json!({
                "title": {"type": "string"}, "columns": {"type": "array", "items": {"type": "string"}},
                "rows": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}}
            }),
        ),
        spec(
            "render_tabs",
            "Render switchable tabs containing nested native surfaces or text.",
            json!({"title": {"type": "string"}, "tabs": {"type": "array", "items": {"type": "object", "properties": {
                "label": {"type": "string"}, "content": {"type": "array", "items": {"type": "object"}}
            }, "required": ["label", "content"]}}}),
        ),
    ]
}

pub fn execute(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    name: &str,
    arguments: &Value,
) -> EngineResult<Value> {
    match name {
        "blog_stats" | "get_blog_stats" => blog_stats(conn, project_id),
        "check_term" => check_term(conn, project_id, required_str(arguments, "term")?),
        "search_posts" => search_posts(conn, project_id, arguments),
        "read_post" => {
            let item = post::get_post_by_id(conn, required_id(arguments, "post_id", "postId")?)?;
            ensure_project(&item.project_id, project_id)?;
            post_detail(data_dir, item)
        }
        "read_post_by_slug" => {
            let item = post::get_post_by_project_and_slug(
                conn,
                project_id,
                required_str(arguments, "slug")?,
            )?;
            post_detail(data_dir, item)
        }
        "list_posts" => list_posts(conn, project_id, arguments),
        "count_posts" => count_posts(conn, project_id, arguments),
        "update_post_metadata" => update_post_metadata(conn, data_dir, project_id, arguments),
        "list_media" => list_media(conn, project_id, arguments),
        "get_media" => {
            let item =
                media::get_media_by_id(conn, required_id(arguments, "media_id", "mediaId")?)?;
            ensure_project(&item.project_id, project_id)?;
            Ok(json!({"success": true, "media": item}))
        }
        "view_image" => view_image(conn, data_dir, project_id, arguments),
        "update_media_metadata" => update_media_metadata(conn, data_dir, project_id, arguments),
        "list_tags" => counted_terms(conn, project_id, true),
        "list_categories" => counted_terms(conn, project_id, false),
        "get_post_backlinks" => post_links(conn, project_id, arguments, true),
        "get_post_outlinks" => post_links(conn, project_id, arguments, false),
        "get_post_media" => linked_media(conn, project_id, arguments),
        "get_media_posts" => linked_posts(conn, project_id, arguments),
        "list_templates" => Ok(json!({
            "templates": template::list_templates_by_project(conn, project_id)?,
        })),
        "read_template" => {
            let item = template::get_template_by_id(
                conn,
                required_id(arguments, "template_id", "templateId")?,
            )?;
            ensure_project(&item.project_id, project_id)?;
            Ok(json!({"success": true, "template": item}))
        }
        "list_scripts" => Ok(json!({
            "scripts": script::list_scripts_by_project(conn, project_id)?,
        })),
        "read_script" => {
            let item =
                script::get_script_by_id(conn, required_id(arguments, "script_id", "scriptId")?)?;
            ensure_project(&item.project_id, project_id)?;
            Ok(json!({"success": true, "script": item}))
        }
        "navigate" => navigate(arguments),
        name if chat_surfaces::RENDER_TOOL_NAMES.contains(&name) => {
            Ok(chat_surfaces::render_tool_result(name, arguments)
                .expect("render tool allow-list and result builder must stay in sync"))
        }
        _ => Ok(json!({"success": false, "error": "unknown_tool", "name": name})),
    }
}

fn blog_stats(conn: &Connection, project_id: &str) -> EngineResult<Value> {
    let posts = post::list_posts_by_project(conn, project_id)?;
    let media_count = media::count_media_by_project(conn, project_id)?;
    let templates = template::list_templates_by_project(conn, project_id)?.len();
    let scripts = script::list_scripts_by_project(conn, project_id)?.len();
    let tags = posts
        .iter()
        .flat_map(|item| &item.tags)
        .collect::<BTreeSet<_>>()
        .len();
    let categories = posts
        .iter()
        .flat_map(|item| &item.categories)
        .collect::<BTreeSet<_>>()
        .len();
    Ok(json!({
        "posts": posts.len(), "media": media_count, "templates": templates,
        "scripts": scripts, "tags": tags, "categories": categories,
    }))
}

fn check_term(conn: &Connection, project_id: &str, term: &str) -> EngineResult<Value> {
    let term = term.to_lowercase();
    let posts = post::list_posts_by_project(conn, project_id)?;
    let tag_count = posts
        .iter()
        .filter(|item| item.tags.iter().any(|value| value.to_lowercase() == term))
        .count();
    let category_count = posts
        .iter()
        .filter(|item| {
            item.categories
                .iter()
                .any(|value| value.to_lowercase() == term)
        })
        .count();
    Ok(json!({"term": term, "tag_posts": tag_count, "category_posts": category_count}))
}

fn search_posts(conn: &Connection, project_id: &str, arguments: &Value) -> EngineResult<Value> {
    let query = required_str(arguments, "query")?;
    let language = arguments
        .get("language")
        .and_then(Value::as_str)
        .unwrap_or("en");
    let limit = limit(arguments);
    let mut matches = Vec::new();
    for id in crate::db::fts::search_posts(conn, query, language)? {
        if let Ok(item) = post::get_post_by_id(conn, &id)
            && item.project_id == project_id
        {
            matches.push(post_summary(&item));
            if matches.len() == limit {
                break;
            }
        }
    }
    Ok(json!({"posts": matches, "count": matches.len()}))
}

fn list_posts(conn: &Connection, project_id: &str, arguments: &Value) -> EngineResult<Value> {
    let status = arguments.get("status").and_then(Value::as_str);
    let tag = arguments.get("tag").and_then(Value::as_str);
    let category = arguments.get("category").and_then(Value::as_str);
    let items = post::list_posts_by_project(conn, project_id)?
        .into_iter()
        .filter(|item| status.is_none_or(|value| item.status.as_str() == value))
        .filter(|item| tag.is_none_or(|value| contains_case_insensitive(&item.tags, value)))
        .filter(|item| {
            category.is_none_or(|value| contains_case_insensitive(&item.categories, value))
        })
        .take(limit(arguments))
        .map(|item| post_summary(&item))
        .collect::<Vec<_>>();
    Ok(json!({"posts": items, "count": items.len()}))
}

fn count_posts(conn: &Connection, project_id: &str, arguments: &Value) -> EngineResult<Value> {
    let items = post::list_posts_by_project(conn, project_id)?;
    let Some(group_by) = arguments
        .get("group_by")
        .or_else(|| arguments.get("groupBy"))
        .and_then(Value::as_str)
    else {
        return Ok(json!({"total_posts": items.len()}));
    };
    let mut groups = BTreeMap::<String, usize>::new();
    for item in &items {
        let values: Vec<String> = match group_by {
            "status" => vec![item.status.as_str().to_string()],
            "tag" => item.tags.clone(),
            "category" => item.categories.clone(),
            _ => {
                return Err(EngineError::Validation(format!(
                    "unsupported post grouping: {group_by}"
                )));
            }
        };
        for value in values {
            *groups.entry(value).or_default() += 1;
        }
    }
    Ok(json!({"total_posts": items.len(), "group_by": group_by, "groups": groups}))
}

fn update_post_metadata(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    arguments: &Value,
) -> EngineResult<Value> {
    let id = required_id(arguments, "post_id", "postId")?;
    let existing = post::get_post_by_id(conn, id)?;
    ensure_project(&existing.project_id, project_id)?;
    if !["title", "excerpt", "tags", "categories"]
        .iter()
        .any(|key| arguments.get(key).is_some())
    {
        return Err(EngineError::Validation(
            "no post metadata updates provided".to_string(),
        ));
    }
    let excerpt = optional_nullable_str(arguments, "excerpt")?;
    let item = crate::engine::post::update_post(
        conn,
        data_dir,
        id,
        arguments.get("title").and_then(Value::as_str),
        None,
        excerpt,
        None,
        optional_string_array(arguments, "tags")?,
        optional_string_array(arguments, "categories")?,
        None,
        None,
        None,
        None,
    )?;
    Ok(json!({"success": true, "post": post_summary(&item)}))
}

fn list_media(conn: &Connection, project_id: &str, arguments: &Value) -> EngineResult<Value> {
    let items = media::list_media_by_project(conn, project_id)?
        .into_iter()
        .take(limit(arguments))
        .collect::<Vec<_>>();
    Ok(json!({"media": items, "count": items.len()}))
}

fn update_media_metadata(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    arguments: &Value,
) -> EngineResult<Value> {
    let id = required_id(arguments, "media_id", "mediaId")?;
    let existing = media::get_media_by_id(conn, id)?;
    ensure_project(&existing.project_id, project_id)?;
    if !["title", "alt", "caption", "tags"]
        .iter()
        .any(|key| arguments.get(key).is_some())
    {
        return Err(EngineError::Validation(
            "no media metadata updates provided".to_string(),
        ));
    }
    let item = crate::engine::media::update_media(
        conn,
        data_dir,
        id,
        optional_nullable_str(arguments, "title")?,
        optional_nullable_str(arguments, "alt")?,
        optional_nullable_str(arguments, "caption")?,
        None,
        None,
        optional_string_array(arguments, "tags")?,
    )?;
    Ok(json!({"success": true, "media": item}))
}

fn view_image(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    arguments: &Value,
) -> EngineResult<Value> {
    let item = media::get_media_by_id(conn, required_id(arguments, "media_id", "mediaId")?)?;
    ensure_project(&item.project_id, project_id)?;
    if !item.mime_type.starts_with("image/") {
        return Ok(json!({"success": false, "error": "not_image", "mime_type": item.mime_type}));
    }
    let size = arguments
        .get("size")
        .and_then(Value::as_str)
        .unwrap_or("medium");
    if !["small", "medium", "large"].contains(&size) {
        return Err(EngineError::Validation(format!(
            "unsupported thumbnail size: {size}"
        )));
    }
    let path = data_dir.join(crate::util::thumbnail_path(&item.id, size, "webp"));
    if !path.is_file() {
        return Ok(json!({"success": false, "error": "thumbnail_not_available"}));
    }
    let encoded = base64::engine::general_purpose::STANDARD.encode(fs::read(path)?);
    Ok(json!({
        "success": true,
        "media": item,
        "data_url": format!("data:image/webp;base64,{encoded}"),
    }))
}

fn post_links(
    conn: &Connection,
    project_id: &str,
    arguments: &Value,
    incoming: bool,
) -> EngineResult<Value> {
    let id = required_id(arguments, "post_id", "postId")?;
    let source = post::get_post_by_id(conn, id)?;
    ensure_project(&source.project_id, project_id)?;
    let links = if incoming {
        post_link::list_links_by_target(conn, id)?
    } else {
        post_link::list_links_by_source(conn, id)?
    };
    let mut items = Vec::with_capacity(links.len());
    for link in links {
        let linked_id = if incoming {
            &link.source_post_id
        } else {
            &link.target_post_id
        };
        let linked = post::get_post_by_id(conn, linked_id)?;
        ensure_project(&linked.project_id, project_id)?;
        items.push(json!({
            "post": post_summary(&linked),
            "link_text": link.link_text,
        }));
    }
    if incoming {
        Ok(json!({"success": true, "post_id": id, "linked_by": items}))
    } else {
        Ok(json!({"success": true, "post_id": id, "links_to": items}))
    }
}

fn linked_media(conn: &Connection, project_id: &str, arguments: &Value) -> EngineResult<Value> {
    let id = required_id(arguments, "post_id", "postId")?;
    let item = post::get_post_by_id(conn, id)?;
    ensure_project(&item.project_id, project_id)?;
    let mut items = Vec::new();
    for link in post_media::list_post_media_by_post(conn, id)? {
        ensure_project(&link.project_id, project_id)?;
        let item = media::get_media_by_id(conn, &link.media_id)?;
        ensure_project(&item.project_id, project_id)?;
        items.push(json!({"media": item, "sort_order": link.sort_order}));
    }
    Ok(json!({"success": true, "post_id": id, "media": items}))
}

fn linked_posts(conn: &Connection, project_id: &str, arguments: &Value) -> EngineResult<Value> {
    let id = required_id(arguments, "media_id", "mediaId")?;
    let item = media::get_media_by_id(conn, id)?;
    ensure_project(&item.project_id, project_id)?;
    let mut items = Vec::new();
    for link in post_media::list_post_media_by_media(conn, id)? {
        ensure_project(&link.project_id, project_id)?;
        let item = post::get_post_by_id(conn, &link.post_id)?;
        ensure_project(&item.project_id, project_id)?;
        items.push(json!({"post": post_summary(&item), "sort_order": link.sort_order}));
    }
    Ok(json!({"success": true, "media_id": id, "posts": items}))
}

fn counted_terms(conn: &Connection, project_id: &str, tags: bool) -> EngineResult<Value> {
    let mut counts = BTreeMap::<String, usize>::new();
    for item in post::list_posts_by_project(conn, project_id)? {
        for term in if tags { &item.tags } else { &item.categories } {
            *counts.entry(term.clone()).or_default() += 1;
        }
    }
    let values = counts
        .into_iter()
        .map(|(name, count)| json!({"name": name, "count": count}))
        .collect::<Vec<_>>();
    Ok(if tags {
        json!({"tags": values, "count": values.len()})
    } else {
        json!({"categories": values, "count": values.len()})
    })
}

fn navigate(arguments: &Value) -> EngineResult<Value> {
    let action = arguments.get("action").and_then(Value::as_str);
    let value = arguments
        .get("value")
        .or_else(|| arguments.get("entity_id"))
        .or_else(|| arguments.get("entityId"))
        .and_then(Value::as_str);
    let (destination, entity_id) = match action {
        Some("open_post" | "openPost") => ("posts", required_navigation_value(value, "post")?),
        Some("open_media" | "openMedia") => ("media", required_navigation_value(value, "media")?),
        Some("open_chat" | "openChat") => ("chat", required_navigation_value(value, "chat")?),
        Some("open_settings" | "openSettings") => ("settings", None),
        Some("switch_view" | "switchView") => {
            (required_navigation_value(value, "view")?.unwrap(), None)
        }
        Some("toggle_sidebar" | "toggleSidebar") => ("toggle_sidebar", None),
        Some("toggle_panel" | "togglePanel") => ("toggle_panel", None),
        Some("toggle_assistant_sidebar" | "toggleAssistantSidebar") => {
            ("toggle_assistant_sidebar", None)
        }
        Some(action) => {
            return Err(EngineError::Validation(format!(
                "unsupported navigation action: {action}"
            )));
        }
        None => (required_str(arguments, "destination")?, value),
    };
    if ![
        "posts",
        "pages",
        "media",
        "templates",
        "scripts",
        "tags",
        "chat",
        "import",
        "git",
        "settings",
        "toggle_sidebar",
        "toggle_panel",
        "toggle_assistant_sidebar",
    ]
    .contains(&destination)
    {
        return Err(EngineError::Validation(format!(
            "unsupported navigation destination: {destination}"
        )));
    }
    Ok(json!({
        "success": true,
        "navigation": {
            "destination": destination,
            "entity_id": entity_id,
        }
    }))
}

fn post_detail(data_dir: &Path, item: crate::model::Post) -> EngineResult<Value> {
    let body = post_body(data_dir, &item)?;
    Ok(json!({"success": true, "post": item, "body": body}))
}

fn post_body(data_dir: &Path, item: &crate::model::Post) -> EngineResult<String> {
    if let Some(content) = item.content.as_deref() {
        return Ok(content.to_string());
    }
    if item.file_path.is_empty() {
        return Ok(String::new());
    }
    let raw = fs::read_to_string(data_dir.join(&item.file_path))?;
    let (_, body) = read_post_file(&raw)
        .map_err(|error| EngineError::Parse(format!("invalid post file: {error}")))?;
    Ok(body)
}

fn post_summary(item: &crate::model::Post) -> Value {
    json!({
        "id": item.id, "title": item.title, "slug": item.slug,
        "excerpt": item.excerpt, "status": item.status, "tags": item.tags,
        "categories": item.categories, "created_at": item.created_at,
        "updated_at": item.updated_at,
    })
}

fn spec(name: &str, description: &str, properties: Value) -> Value {
    let required = properties
        .as_object()
        .into_iter()
        .flat_map(|values| values.iter())
        .filter(|(_, schema)| !schema.get("type").is_some_and(Value::is_array))
        .filter(|(name, _)| {
            matches!(
                name.as_str(),
                "term"
                    | "query"
                    | "post_id"
                    | "slug"
                    | "media_id"
                    | "template_id"
                    | "script_id"
                    | "destination"
            )
        })
        .map(|(name, _)| Value::String(name.clone()))
        .collect::<Vec<_>>();
    let required = if name == "navigate" {
        Vec::new()
    } else {
        required
    };
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false,
            }
        }
    })
}

fn required_navigation_value<'a>(
    value: Option<&'a str>,
    target: &str,
) -> EngineResult<Option<&'a str>> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(Some)
        .ok_or_else(|| {
            EngineError::Validation(format!("navigation {target} identifier is required"))
        })
}

fn required_str<'a>(arguments: &'a Value, key: &str) -> EngineResult<&'a str> {
    arguments
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| EngineError::Validation(format!("tool argument {key} is required")))
}

fn required_id<'a>(arguments: &'a Value, snake: &str, camel: &str) -> EngineResult<&'a str> {
    arguments
        .get(snake)
        .or_else(|| arguments.get(camel))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| EngineError::Validation(format!("tool argument {snake} is required")))
}

fn ensure_project(actual: &str, expected: &str) -> EngineResult<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(EngineError::NotFound("project entity".to_string()))
    }
}

fn limit(arguments: &Value) -> usize {
    arguments
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(25)
        .clamp(1, 100) as usize
}

fn contains_case_insensitive(values: &[String], needle: &str) -> bool {
    values
        .iter()
        .any(|value| value.eq_ignore_ascii_case(needle))
}

fn optional_string_array(arguments: &Value, key: &str) -> EngineResult<Option<Vec<String>>> {
    let Some(value) = arguments.get(key) else {
        return Ok(None);
    };
    let values = value
        .as_array()
        .ok_or_else(|| EngineError::Validation(format!("tool argument {key} must be an array")))?
        .iter()
        .map(|value| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                EngineError::Validation(format!("tool argument {key} must contain strings"))
            })
        })
        .collect::<EngineResult<Vec<_>>>()?;
    Ok(Some(values))
}

fn optional_nullable_str<'a>(
    arguments: &'a Value,
    key: &str,
) -> EngineResult<Option<Option<&'a str>>> {
    match arguments.get(key) {
        None => Ok(None),
        Some(Value::Null) => Ok(Some(None)),
        Some(Value::String(value)) => Ok(Some(Some(value))),
        Some(_) => Err(EngineError::Validation(format!(
            "tool argument {key} must be a string or null"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::media::{insert_media, make_test_media};
    use crate::db::queries::post::{insert_post, make_test_post};
    use crate::db::queries::post_link::insert_post_link;
    use crate::db::queries::post_media::link_media;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::model::{PostLink, PostMedia};

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        insert_post(db.conn(), &make_test_post("source", "p1", "source")).unwrap();
        insert_post(db.conn(), &make_test_post("target", "p1", "target")).unwrap();
        insert_media(db.conn(), &make_test_media("media1", "p1")).unwrap();
        insert_post_link(
            db.conn(),
            &PostLink {
                id: "link1".into(),
                source_post_id: "source".into(),
                target_post_id: "target".into(),
                link_text: Some("read next".into()),
                created_at: 1,
            },
        )
        .unwrap();
        link_media(
            db.conn(),
            &PostMedia {
                id: "post-media1".into(),
                project_id: "p1".into(),
                post_id: "source".into(),
                media_id: "media1".into(),
                sort_order: 3,
                created_at: 1,
            },
        )
        .unwrap();
        db
    }

    #[test]
    fn relationship_tools_return_project_entities() {
        let db = setup();
        let dir = tempfile::tempdir().unwrap();
        let outlinks = execute(
            db.conn(),
            dir.path(),
            "p1",
            "get_post_outlinks",
            &json!({"post_id": "source"}),
        )
        .unwrap();
        assert_eq!(outlinks["links_to"][0]["post"]["id"], "target");
        assert_eq!(outlinks["links_to"][0]["link_text"], "read next");

        let backlinks = execute(
            db.conn(),
            dir.path(),
            "p1",
            "get_post_backlinks",
            &json!({"post_id": "target"}),
        )
        .unwrap();
        assert_eq!(backlinks["linked_by"][0]["post"]["id"], "source");

        let post_media = execute(
            db.conn(),
            dir.path(),
            "p1",
            "get_post_media",
            &json!({"post_id": "source"}),
        )
        .unwrap();
        assert_eq!(post_media["media"][0]["media"]["id"], "media1");
        assert_eq!(post_media["media"][0]["sort_order"], 3);

        let media_posts = execute(
            db.conn(),
            dir.path(),
            "p1",
            "get_media_posts",
            &json!({"media_id": "media1"}),
        )
        .unwrap();
        assert_eq!(media_posts["posts"][0]["post"]["id"], "source");
    }

    #[test]
    fn view_image_is_bounded_to_generated_image_thumbnails() {
        let db = setup();
        let dir = tempfile::tempdir().unwrap();
        let relative = crate::util::thumbnail_path("media1", "medium", "webp");
        let path = dir.path().join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"thumbnail").unwrap();

        let result = execute(
            db.conn(),
            dir.path(),
            "p1",
            "view_image",
            &json!({"media_id": "media1", "size": "medium"}),
        )
        .unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["data_url"], "data:image/webp;base64,dGh1bWJuYWls");

        let invalid = execute(
            db.conn(),
            dir.path(),
            "p1",
            "view_image",
            &json!({"media_id": "media1", "size": "original"}),
        );
        assert!(matches!(invalid, Err(EngineError::Validation(_))));
    }

    #[test]
    fn structured_render_tools_are_advertised_and_return_inert_native_data() {
        let specs = tool_specs();
        let names = specs
            .iter()
            .filter_map(|spec| spec.pointer("/function/name").and_then(Value::as_str))
            .collect::<BTreeSet<_>>();
        for name in chat_surfaces::RENDER_TOOL_NAMES {
            assert!(names.contains(name), "missing tool schema for {name}");
        }
        let chart = specs
            .iter()
            .find(|spec| {
                spec.pointer("/function/name").and_then(Value::as_str) == Some("render_chart")
            })
            .unwrap();
        assert!(
            chart
                .pointer("/function/parameters/properties/chartType")
                .is_some()
        );
        assert!(
            chart
                .pointer("/function/parameters/properties/chart_type")
                .is_some()
        );

        let db = setup();
        let dir = tempfile::tempdir().unwrap();
        let raw = json!({"title": "<script>alert(1)</script>", "body": "<b>data</b>"});
        let result = execute(db.conn(), dir.path(), "p1", "render_card", &raw).unwrap();
        assert_eq!(result["type"], "card");
        assert_eq!(result["title"], raw["title"]);
        assert_eq!(result["body"], raw["body"]);
        assert!(result.get("html").is_none());
        assert!(result.get("javascript").is_none());
    }
}
