use std::collections::BTreeMap;

use chrono::{Datelike, TimeZone as _, Utc};
use serde_json::{Map, Value, json};

use crate::db::DbConnection;
use crate::db::queries::{media as media_q, media_translation, post as post_q, post_translation};
use crate::engine::{EngineError, EngineResult};
use crate::model::{Post, ProposalKind};

use super::resources::{post_detail, post_summary};
use super::{active_project, create_proposal, optional_string, required_string, string_array};

const MAX_PAGE_SIZE: usize = 50;

pub fn list() -> Vec<Value> {
    [
        tool(
            "check_term",
            "Check Term",
            "Check whether a term is a category, a tag, or both, with post counts.",
            object_schema(json!({"term": string_schema("Term to check")}), &["term"]),
            true,
        ),
        tool(
            "search_posts",
            "Search Posts",
            "Full-text and filtered post search with pagination, backlinks, and outgoing links.",
            post_query_schema(false),
            true,
        ),
        tool(
            "count_posts",
            "Count Posts",
            "Count filtered posts grouped by year, month, tag, category, or status.",
            count_schema(),
            true,
        ),
        tool(
            "read_post_by_slug",
            "Read Post By Slug",
            "Read full post content and metadata, optionally in a translated language.",
            object_schema(
                json!({
                    "slug": string_schema("Post slug"),
                    "language": string_schema("Optional translation language")
                }),
                &["slug"],
            ),
            true,
        ),
        tool(
            "get_post_translations",
            "Get Post Translations",
            "List every translation for a post.",
            id_schema("postId", "Post ID"),
            true,
        ),
        tool(
            "get_media_translations",
            "Get Media Translations",
            "List every translated metadata record for a media item.",
            id_schema("mediaId", "Media ID"),
            true,
        ),
        tool(
            "upsert_media_translation",
            "Propose Media Translation",
            "Propose translated media metadata for explicit desktop approval.",
            object_schema(
                json!({
                    "mediaId": string_schema("Media ID"),
                    "language": string_schema("Language code"),
                    "title": string_schema("Translated title"),
                    "alt": string_schema("Translated alt text"),
                    "caption": string_schema("Translated caption")
                }),
                &["mediaId", "language"],
            ),
            false,
        ),
        tool(
            "draft_post",
            "Draft Post",
            "Propose a post. No post is created until explicit desktop approval.",
            object_schema(
                json!({
                    "title": string_schema("Post title"),
                    "content": string_schema("Markdown body"),
                    "excerpt": string_schema("Excerpt"),
                    "tags": string_array_schema("Tags"),
                    "categories": string_array_schema("Categories"),
                    "author": string_schema("Author"),
                    "language": string_schema("Language")
                }),
                &["title", "content"],
            ),
            false,
        ),
        tool(
            "propose_script",
            "Propose Script",
            "Validate and propose a Lua script for explicit desktop approval.",
            object_schema(
                json!({
                    "title": string_schema("Script title"),
                    "kind": {"type":"string","enum":["macro","utility","transform"]},
                    "content": string_schema("Lua source"),
                    "entrypoint": string_schema("Entrypoint function")
                }),
                &["title", "kind", "content"],
            ),
            false,
        ),
        tool(
            "propose_template",
            "Propose Template",
            "Validate and propose a Liquid template for explicit desktop approval.",
            object_schema(
                json!({
                    "title": string_schema("Template title"),
                    "kind": {"type":"string","enum":["post","list","not-found","partial"]},
                    "content": string_schema("Liquid source")
                }),
                &["title", "kind", "content"],
            ),
            false,
        ),
        tool(
            "propose_media_metadata",
            "Propose Media Metadata",
            "Propose media metadata changes for explicit desktop approval.",
            object_schema(
                json!({
                    "mediaId": string_schema("Media ID"),
                    "title": nullable_string_schema("Title"),
                    "alt": nullable_string_schema("Alt text"),
                    "caption": nullable_string_schema("Caption"),
                    "tags": string_array_schema("Tags")
                }),
                &["mediaId"],
            ),
            false,
        ),
        tool(
            "propose_post_metadata",
            "Propose Post Metadata",
            "Propose post metadata changes for explicit desktop approval.",
            object_schema(
                json!({
                    "postId": string_schema("Post ID"),
                    "title": string_schema("Title"),
                    "excerpt": nullable_string_schema("Excerpt"),
                    "tags": string_array_schema("Tags"),
                    "categories": string_array_schema("Categories")
                }),
                &["postId"],
            ),
            false,
        ),
    ]
    .into_iter()
    .collect()
}

pub fn call(conn: &DbConnection, name: &str, params: Value) -> EngineResult<Value> {
    if !params.is_object() {
        return Err(EngineError::Validation(
            "tool arguments must be an object".into(),
        ));
    }
    match name {
        "check_term" => check_term(conn, &params),
        "search_posts" => search_posts(conn, &params),
        "count_posts" => count_posts(conn, &params),
        "read_post_by_slug" => read_post_by_slug(conn, &params),
        "get_post_translations" => get_post_translations(conn, &params),
        "get_media_translations" => get_media_translations(conn, &params),
        "upsert_media_translation" => propose_media_translation(conn, &params),
        "draft_post" => propose(conn, ProposalKind::DraftPost, None, &params),
        "propose_script" => propose_script(conn, &params),
        "propose_template" => propose_template(conn, &params),
        "propose_media_metadata" => propose_media_metadata(conn, &params),
        "propose_post_metadata" => propose_post_metadata(conn, &params),
        _ => Err(EngineError::NotFound(format!("MCP tool {name}"))),
    }
}

fn tool(name: &str, title: &str, description: &str, schema: Value, read_only: bool) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": description,
        "inputSchema": schema,
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": false,
            "openWorldHint": false
        }
    })
}

fn string_schema(description: &str) -> Value {
    json!({"type":"string","description":description})
}

fn nullable_string_schema(description: &str) -> Value {
    json!({"type":["string","null"],"description":description})
}

fn string_array_schema(description: &str) -> Value {
    json!({"type":"array","items":{"type":"string"},"description":description})
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    let mut schema = json!({
        "type": "object",
        "properties": properties,
        "additionalProperties": false
    });
    if !required.is_empty() {
        schema["required"] = serde_json::to_value(required).unwrap_or_default();
    }
    schema
}

fn id_schema(field: &str, description: &str) -> Value {
    let mut properties = Map::new();
    properties.insert(field.into(), string_schema(description));
    object_schema(Value::Object(properties), &[field])
}

fn post_query_schema(query_required: bool) -> Value {
    object_schema(
        json!({
            "query": string_schema("Full-text query"),
            "category": string_schema("Category filter"),
            "tags": string_array_schema("All required tags"),
            "language": string_schema("Available language"),
            "missingTranslationLanguage": string_schema("Missing translation language"),
            "year": {"type":"integer"},
            "month": {"type":"integer","minimum":1,"maximum":12},
            "status": {"type":"string","enum":["draft","published","archived"]},
            "offset": {"type":"integer","minimum":0},
            "limit": {"type":"integer","minimum":1,"maximum":MAX_PAGE_SIZE}
        }),
        if query_required { &["query"] } else { &[] },
    )
}

fn count_schema() -> Value {
    let mut schema = post_query_schema(false);
    schema["properties"]["groupBy"] = json!({"type":"array","items":{"type":"string","enum":["year","month","tag","category","status"]},"minItems":1});
    schema["required"] = json!(["groupBy"]);
    schema
}

fn check_term(conn: &DbConnection, params: &Value) -> EngineResult<Value> {
    let term = required_string(params, "term")?;
    let (project, _) = active_project(conn)?;
    let posts = post_q::list_posts_by_project(conn, &project.id)?;
    let normalized = term.to_lowercase();
    let tag_count = posts
        .iter()
        .filter(|post| post.tags.iter().any(|tag| tag.to_lowercase() == normalized))
        .count();
    let category_count = posts
        .iter()
        .filter(|post| {
            post.categories
                .iter()
                .any(|category| category.to_lowercase() == normalized)
        })
        .count();
    Ok(json!({
        "is_category": category_count > 0,
        "category_post_count": category_count,
        "is_tag": tag_count > 0,
        "tag_post_count": tag_count
    }))
}

fn filtered_posts(conn: &DbConnection, params: &Value) -> EngineResult<Vec<Post>> {
    let (project, _) = active_project(conn)?;
    let mut posts = post_q::list_posts_by_project(conn, &project.id)?;
    let query = optional_string(params, "query")
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let fts_matches = if query.is_empty() {
        None
    } else {
        let language = optional_string(params, "language").unwrap_or("en");
        Some(
            crate::db::fts::search_posts(conn, &query, language)?
                .into_iter()
                .collect::<std::collections::HashSet<_>>(),
        )
    };
    let category = optional_string(params, "category").map(str::to_lowercase);
    let tags = string_array(params, "tags")
        .into_iter()
        .map(|tag| tag.to_lowercase())
        .collect::<Vec<_>>();
    let language = optional_string(params, "language").map(str::to_lowercase);
    let missing_language =
        optional_string(params, "missingTranslationLanguage").map(str::to_lowercase);
    let status = optional_string(params, "status");
    let year = integer(params, "year")?.map(|value| value as i32);
    let month = integer(params, "month")?.map(|value| value as u32);
    if month.is_some() && year.is_none() {
        return Err(EngineError::Validation("month requires year".into()));
    }
    posts.retain(|post| {
        if fts_matches
            .as_ref()
            .is_some_and(|matches| !matches.contains(&post.id))
        {
            return false;
        }
        if category.as_ref().is_some_and(|wanted| {
            !post
                .categories
                .iter()
                .any(|value| value.to_lowercase() == *wanted)
        }) {
            return false;
        }
        if tags.iter().any(|wanted| {
            !post
                .tags
                .iter()
                .any(|value| value.to_lowercase() == *wanted)
        }) {
            return false;
        }
        if status.is_some_and(|wanted| post.status.as_str() != wanted) {
            return false;
        }
        if let Some(wanted) = &language {
            let canonical = post
                .language
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(wanted));
            let translated =
                post_translation::get_post_translation_by_post_and_language(conn, &post.id, wanted)
                    .is_ok();
            if !canonical && !translated {
                return false;
            }
        }
        if let Some(wanted) = &missing_language {
            let canonical = post
                .language
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(wanted));
            let translated =
                post_translation::get_post_translation_by_post_and_language(conn, &post.id, wanted)
                    .is_ok();
            if canonical || translated {
                return false;
            }
        }
        if let Some(wanted_year) = year {
            let Some(date) = Utc.timestamp_millis_opt(post.created_at).single() else {
                return false;
            };
            if date.year() != wanted_year || month.is_some_and(|wanted| date.month() != wanted) {
                return false;
            }
        }
        true
    });
    Ok(posts)
}

fn search_posts(conn: &DbConnection, params: &Value) -> EngineResult<Value> {
    let posts = filtered_posts(conn, params)?;
    let total = posts.len();
    let offset = unsigned(params, "offset")?.unwrap_or(0);
    let limit = unsigned(params, "limit")?.unwrap_or(MAX_PAGE_SIZE);
    if limit == 0 || limit > MAX_PAGE_SIZE {
        return Err(EngineError::Validation(format!(
            "limit must be between 1 and {MAX_PAGE_SIZE}"
        )));
    }
    let posts = posts
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(|post| post_summary(conn, &post))
        .collect::<EngineResult<Vec<_>>>()?;
    Ok(json!({
        "total": total,
        "offset": offset,
        "limit": limit,
        "hasMore": offset.saturating_add(limit) < total,
        "posts": posts
    }))
}

fn count_posts(conn: &DbConnection, params: &Value) -> EngineResult<Value> {
    let group_by = string_array(params, "groupBy");
    if group_by.is_empty()
        || group_by.iter().any(|dimension| {
            !["year", "month", "tag", "category", "status"].contains(&dimension.as_str())
        })
    {
        return Err(EngineError::Validation("invalid groupBy".into()));
    }
    let posts = filtered_posts(conn, params)?;
    let total = posts.len();
    let mut counts = BTreeMap::<String, (Map<String, Value>, usize)>::new();
    for post in posts {
        for row in group_rows(&post, &group_by) {
            let key = serde_json::to_string(&row)?;
            counts
                .entry(key)
                .and_modify(|(_, count)| *count += 1)
                .or_insert((row, 1));
        }
    }
    let groups = counts
        .into_values()
        .map(|(mut row, count)| {
            row.insert("count".into(), json!(count));
            Value::Object(row)
        })
        .collect::<Vec<_>>();
    Ok(json!({"groups": groups, "totalPosts": total}))
}

fn group_rows(post: &Post, dimensions: &[String]) -> Vec<Map<String, Value>> {
    let Some((dimension, rest)) = dimensions.split_first() else {
        return vec![Map::new()];
    };
    let values = match dimension.as_str() {
        "year" => Utc
            .timestamp_millis_opt(post.created_at)
            .single()
            .map(|date| vec![json!(date.year())])
            .unwrap_or_else(|| vec![Value::Null]),
        "month" => Utc
            .timestamp_millis_opt(post.created_at)
            .single()
            .map(|date| vec![json!(date.month())])
            .unwrap_or_else(|| vec![Value::Null]),
        "tag" => values_or_null(&post.tags),
        "category" => values_or_null(&post.categories),
        "status" => vec![json!(post.status.as_str())],
        _ => vec![Value::Null],
    };
    values
        .into_iter()
        .flat_map(|value| {
            group_rows(post, rest).into_iter().map({
                let dimension = dimension.clone();
                move |mut row| {
                    row.insert(dimension.clone(), value.clone());
                    row
                }
            })
        })
        .collect()
}

fn values_or_null(values: &[String]) -> Vec<Value> {
    if values.is_empty() {
        vec![Value::Null]
    } else {
        values.iter().map(|value| json!(value)).collect()
    }
}

fn read_post_by_slug(conn: &DbConnection, params: &Value) -> EngineResult<Value> {
    let slug = required_string(params, "slug")?;
    let (project, data_dir) = active_project(conn)?;
    let post = post_q::get_post_by_project_and_slug(conn, &project.id, slug)
        .map_err(|_| EngineError::NotFound(format!("post slug {slug}")))?;
    let Some(language) = optional_string(params, "language") else {
        return Ok(json!({"post": post_detail(conn, &data_dir, &post)?}));
    };
    if post
        .language
        .as_deref()
        .is_some_and(|canonical| canonical.eq_ignore_ascii_case(language))
    {
        return Ok(json!({"post": post_detail(conn, &data_dir, &post)?}));
    }
    let translation =
        post_translation::get_post_translation_by_post_and_language(conn, &post.id, language)
            .map_err(|_| EngineError::NotFound(format!("post translation {language}")))?;
    let mut detail = post_detail(conn, &data_dir, &post)?;
    detail["title"] = json!(translation.title);
    detail["excerpt"] = json!(translation.excerpt);
    detail["content"] = json!(translation_content(&data_dir, &translation));
    detail["language"] = json!(translation.language);
    detail["canonicalLanguage"] = json!(post.language);
    Ok(json!({"post": detail}))
}

fn translation_content(
    data_dir: &std::path::Path,
    translation: &crate::model::PostTranslation,
) -> String {
    if let Some(content) = &translation.content {
        return content.clone();
    }
    std::fs::read_to_string(data_dir.join(&translation.file_path))
        .ok()
        .and_then(|source| {
            crate::util::frontmatter::read_translation_file(&source)
                .ok()
                .map(|(_, body)| body)
        })
        .unwrap_or_default()
}

fn get_post_translations(conn: &DbConnection, params: &Value) -> EngineResult<Value> {
    let id = required_string(params, "postId")?;
    let (project, data_dir) = active_project(conn)?;
    let post = post_q::get_post_by_id(conn, id)
        .map_err(|_| EngineError::NotFound(format!("post {id}")))?;
    if post.project_id != project.id {
        return Err(EngineError::NotFound(format!("post {id}")));
    }
    let translations = post_translation::list_post_translations_by_post(conn, id)?
        .into_iter()
        .map(|translation| {
            let mut value = serde_json::to_value(&translation)?;
            value["content"] = json!(translation_content(&data_dir, &translation));
            Ok(value)
        })
        .collect::<EngineResult<Vec<_>>>()?;
    Ok(json!({"translations": translations}))
}

fn get_media_translations(conn: &DbConnection, params: &Value) -> EngineResult<Value> {
    let id = required_string(params, "mediaId")?;
    let (project, _) = active_project(conn)?;
    let media = media_q::get_media_by_id(conn, id)
        .map_err(|_| EngineError::NotFound(format!("media {id}")))?;
    if media.project_id != project.id {
        return Err(EngineError::NotFound(format!("media {id}")));
    }
    Ok(json!({
        "translations": media_translation::list_media_translations_by_media(conn, id)?
    }))
}

fn propose_media_translation(conn: &DbConnection, params: &Value) -> EngineResult<Value> {
    let id = required_string(params, "mediaId")?;
    required_string(params, "language")?;
    let (project, _) = active_project(conn)?;
    let media = media_q::get_media_by_id(conn, id)
        .map_err(|_| EngineError::NotFound(format!("media {id}")))?;
    if media.project_id != project.id {
        return Err(EngineError::NotFound(format!("media {id}")));
    }
    propose(
        conn,
        ProposalKind::ProposeMediaTranslation,
        Some(id),
        params,
    )
}

fn propose_script(conn: &DbConnection, params: &Value) -> EngineResult<Value> {
    required_string(params, "title")?;
    let content = required_string(params, "content")?;
    required_string(params, "kind")?
        .parse::<crate::model::ScriptKind>()
        .map_err(EngineError::Validation)?;
    crate::engine::script::validate_script_syntax(content).map_err(EngineError::Validation)?;
    propose(conn, ProposalKind::ProposeScript, None, params)
}

fn propose_template(conn: &DbConnection, params: &Value) -> EngineResult<Value> {
    required_string(params, "title")?;
    let content = required_string(params, "content")?;
    required_string(params, "kind")?
        .parse::<crate::model::TemplateKind>()
        .map_err(EngineError::Validation)?;
    crate::engine::template::validate_template(content).map_err(EngineError::Validation)?;
    propose(conn, ProposalKind::ProposeTemplate, None, params)
}

fn propose_media_metadata(conn: &DbConnection, params: &Value) -> EngineResult<Value> {
    let id = required_string(params, "mediaId")?;
    let (project, _) = active_project(conn)?;
    let media = media_q::get_media_by_id(conn, id)
        .map_err(|_| EngineError::NotFound(format!("media {id}")))?;
    if media.project_id != project.id {
        return Err(EngineError::NotFound(format!("media {id}")));
    }
    propose(conn, ProposalKind::ProposeMediaMetadata, Some(id), params)
}

fn propose_post_metadata(conn: &DbConnection, params: &Value) -> EngineResult<Value> {
    let id = required_string(params, "postId")?;
    let (project, _) = active_project(conn)?;
    let post = post_q::get_post_by_id(conn, id)
        .map_err(|_| EngineError::NotFound(format!("post {id}")))?;
    if post.project_id != project.id {
        return Err(EngineError::NotFound(format!("post {id}")));
    }
    propose(conn, ProposalKind::ProposePostMetadata, Some(id), params)
}

fn propose(
    conn: &DbConnection,
    kind: ProposalKind,
    entity_id: Option<&str>,
    params: &Value,
) -> EngineResult<Value> {
    if kind == ProposalKind::DraftPost {
        required_string(params, "title")?;
        required_string(params, "content")?;
    }
    let (project, _) = active_project(conn)?;
    let proposal = create_proposal(conn, kind, &project.id, entity_id, params)?;
    Ok(json!({
        "proposalId": proposal.id,
        "status": proposal.status,
        "expiresAt": proposal.expires_at,
        "message": "Pending explicit approval in RuDS Settings"
    }))
}

fn integer(value: &Value, key: &str) -> EngineResult<Option<i64>> {
    match value.get(key) {
        None => Ok(None),
        Some(value) => value
            .as_i64()
            .map(Some)
            .ok_or_else(|| EngineError::Validation(format!("{key} must be an integer"))),
    }
}

fn unsigned(value: &Value, key: &str) -> EngineResult<Option<usize>> {
    integer(value, key)?.map_or(Ok(None), |value| {
        usize::try_from(value)
            .map(Some)
            .map_err(|_| EngineError::Validation(format!("{key} cannot be negative")))
    })
}
