use std::path::Path;

use base64::Engine as _;
use serde_json::{Value, json};

use crate::db::DbConnection;
use crate::db::queries::{media as media_q, post as post_q, post_link, post_media, tag as tag_q};
use crate::engine::{EngineError, EngineResult};
use crate::model::{Media, Post};

use super::active_project;

const PAGE_SIZE: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceContent {
    pub uri: String,
    pub mime_type: String,
    pub text: Option<String>,
    pub blob: Option<String>,
}

impl ResourceContent {
    fn json(uri: &str, value: &Value) -> EngineResult<Self> {
        Ok(Self {
            uri: uri.to_string(),
            mime_type: "application/json".into(),
            text: Some(serde_json::to_string(value)?),
            blob: None,
        })
    }
}

pub fn list() -> Vec<Value> {
    [
        ("project", "Active project", "bds://project"),
        ("posts", "Blog posts", "bds://posts"),
        ("media", "Media", "bds://media"),
        ("tags", "Tags", "bds://tags"),
        ("categories", "Categories", "bds://categories"),
        ("stats", "Blog statistics", "bds://stats"),
    ]
    .into_iter()
    .map(|(name, title, uri)| {
        json!({
            "name": name,
            "title": title,
            "uri": uri,
            "mimeType": "application/json"
        })
    })
    .collect()
}

pub fn templates() -> Vec<Value> {
    [
        ("posts", "Paginated blog posts", "bds://posts{?cursor}"),
        ("media", "Paginated media", "bds://media{?cursor}"),
        (
            "post media",
            "Media linked to a post",
            "bds://posts/{id}/media",
        ),
        (
            "media image",
            "Original media bytes",
            "bds://media/{id}/image",
        ),
    ]
    .into_iter()
    .map(|(name, title, uri_template)| {
        json!({
            "name": name,
            "title": title,
            "uriTemplate": uri_template
        })
    })
    .collect()
}

pub fn read(conn: &DbConnection, uri: &str) -> EngineResult<ResourceContent> {
    let url = url::Url::parse(uri)
        .map_err(|_| EngineError::Validation("invalid MCP resource URI".into()))?;
    if url.scheme() != "bds" {
        return Err(EngineError::NotFound(uri.into()));
    }
    let host = url
        .host_str()
        .ok_or_else(|| EngineError::NotFound(uri.into()))?;
    let path = url.path().trim_matches('/');
    let (project, data_dir) = active_project(conn)?;
    let value = match (host, path) {
        ("project", "") => project_resource(&project, &data_dir),
        ("posts", "") => posts_page(conn, &project.id, cursor_offset(&url)?),
        ("media", "") => media_page(conn, &project.id, cursor_offset(&url)?),
        ("tags", "") => tags(conn, &project.id),
        ("categories", "") => categories(conn, &project.id, &data_dir),
        ("stats", "") => stats(conn, &project.id, &data_dir),
        ("posts", path) => {
            let parts = path.split('/').collect::<Vec<_>>();
            match parts.as_slice() {
                [post_id] => post_detail_by_id(conn, &project.id, &data_dir, post_id),
                [post_id, "media"] => post_media_items(conn, &project.id, post_id),
                _ => return Err(EngineError::NotFound(uri.into())),
            }
        }
        ("media", path) => {
            let parts = path.split('/').collect::<Vec<_>>();
            match parts.as_slice() {
                [media_id] => media_detail_by_id(conn, &project.id, media_id),
                [media_id, "image"] => {
                    return media_image(conn, &project.id, &data_dir, media_id, uri);
                }
                _ => return Err(EngineError::NotFound(uri.into())),
            }
        }
        _ => return Err(EngineError::NotFound(uri.into())),
    }?;
    ResourceContent::json(uri, &value)
}

fn project_resource(project: &crate::model::Project, data_dir: &Path) -> EngineResult<Value> {
    let metadata = crate::engine::meta::read_project_json(data_dir).ok();
    Ok(json!({
        "id": project.id,
        "name": project.name,
        "slug": project.slug,
        "description": project.description,
        "public_url": metadata.as_ref().and_then(|value| value.public_url.clone()),
        "main_language": metadata.as_ref().and_then(|value| value.main_language.clone()),
        "blog_languages": metadata.map(|value| value.blog_languages).unwrap_or_default()
    }))
}

fn cursor_offset(url: &url::Url) -> EngineResult<usize> {
    let Some(cursor) = url
        .query_pairs()
        .find_map(|(key, value)| (key == "cursor").then(|| value.into_owned()))
    else {
        return Ok(0);
    };
    if cursor.is_empty() {
        return Err(EngineError::Validation("invalid cursor".into()));
    }
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| EngineError::Validation("invalid cursor".into()))?;
    let value: Value = serde_json::from_slice(&decoded)
        .map_err(|_| EngineError::Validation("invalid cursor".into()))?;
    value["offset"]
        .as_u64()
        .map(|offset| offset as usize)
        .ok_or_else(|| EngineError::Validation("invalid cursor".into()))
}

fn encode_cursor(offset: usize) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_vec(&json!({"offset": offset})).unwrap_or_default())
}

fn page(items: Vec<Value>, total: usize, offset: usize) -> Value {
    let mut value = json!({
        "items": items,
        "total": total,
        "offset": offset,
        "limit": PAGE_SIZE
    });
    let next = offset.saturating_add(PAGE_SIZE);
    if next < total {
        value["nextCursor"] = Value::String(encode_cursor(next));
    }
    value
}

fn posts_page(conn: &DbConnection, project_id: &str, offset: usize) -> EngineResult<Value> {
    let posts = post_q::list_posts_by_project(conn, project_id)?;
    let total = posts.len();
    let items = posts
        .into_iter()
        .skip(offset)
        .take(PAGE_SIZE)
        .map(|post| post_summary(conn, &post))
        .collect::<EngineResult<Vec<_>>>()?;
    Ok(page(items, total, offset))
}

fn media_page(conn: &DbConnection, project_id: &str, offset: usize) -> EngineResult<Value> {
    let media = media_q::list_media_by_project(conn, project_id)?;
    let total = media.len();
    let items = media
        .into_iter()
        .skip(offset)
        .take(PAGE_SIZE)
        .map(|item| media_summary(&item))
        .collect();
    Ok(page(items, total, offset))
}

pub(crate) fn post_summary(conn: &DbConnection, post: &Post) -> EngineResult<Value> {
    Ok(json!({
        "id": post.id,
        "title": post.title,
        "slug": post.slug,
        "status": post.status,
        "tags": post.tags,
        "categories": post.categories,
        "created_at": post.created_at,
        "backlinks": linked_posts(conn, &post.id, false)?,
        "linksTo": linked_posts(conn, &post.id, true)?
    }))
}

fn linked_posts(conn: &DbConnection, post_id: &str, outgoing: bool) -> EngineResult<Vec<Value>> {
    let links = if outgoing {
        post_link::list_links_by_source(conn, post_id)?
            .into_iter()
            .map(|link| link.target_post_id)
            .collect::<Vec<_>>()
    } else {
        post_link::list_links_by_target(conn, post_id)?
            .into_iter()
            .map(|link| link.source_post_id)
            .collect::<Vec<_>>()
    };
    Ok(links
        .into_iter()
        .filter_map(|id| post_q::get_post_by_id(conn, &id).ok())
        .map(|post| json!({"id": post.id, "title": post.title, "slug": post.slug}))
        .collect())
}

pub(crate) fn post_detail(
    conn: &DbConnection,
    data_dir: &Path,
    post: &Post,
) -> EngineResult<Value> {
    let mut value = serde_json::to_value(post)?;
    value["content"] = Value::String(post_body(data_dir, post));
    value["backlinks"] = Value::Array(linked_posts(conn, &post.id, false)?);
    value["linksTo"] = Value::Array(linked_posts(conn, &post.id, true)?);
    let translations =
        crate::db::queries::post_translation::list_post_translations_by_post(conn, &post.id)?;
    let mut languages = post.language.clone().into_iter().collect::<Vec<_>>();
    languages.extend(
        translations
            .into_iter()
            .map(|translation| translation.language),
    );
    languages.sort();
    languages.dedup();
    value["availableLanguages"] = serde_json::to_value(languages)?;
    Ok(value)
}

fn post_body(data_dir: &Path, post: &Post) -> String {
    if let Some(content) = &post.content {
        return content.clone();
    }
    if post.file_path.is_empty() {
        return String::new();
    }
    std::fs::read_to_string(data_dir.join(&post.file_path))
        .ok()
        .and_then(|source| {
            crate::util::frontmatter::read_post_file(&source)
                .ok()
                .map(|(_, body)| body)
        })
        .unwrap_or_default()
}

fn post_detail_by_id(
    conn: &DbConnection,
    project_id: &str,
    data_dir: &Path,
    id: &str,
) -> EngineResult<Value> {
    let post = post_q::get_post_by_id(conn, id)
        .map_err(|_| EngineError::NotFound(format!("post {id}")))?;
    ensure_project(project_id, &post.project_id, "post", id)?;
    post_detail(conn, data_dir, &post)
}

pub(crate) fn media_summary(media: &Media) -> Value {
    json!({
        "id": media.id,
        "filename": media.filename,
        "title": media.title,
        "alt": media.alt,
        "caption": media.caption,
        "tags": media.tags
    })
}

fn media_detail_by_id(conn: &DbConnection, project_id: &str, id: &str) -> EngineResult<Value> {
    let media = media_q::get_media_by_id(conn, id)
        .map_err(|_| EngineError::NotFound(format!("media {id}")))?;
    ensure_project(project_id, &media.project_id, "media", id)?;
    Ok(serde_json::to_value(media)?)
}

fn post_media_items(conn: &DbConnection, project_id: &str, post_id: &str) -> EngineResult<Value> {
    let post = post_q::get_post_by_id(conn, post_id)
        .map_err(|_| EngineError::NotFound(format!("post {post_id}")))?;
    ensure_project(project_id, &post.project_id, "post", post_id)?;
    let items = post_media::list_post_media_by_post(conn, post_id)?
        .into_iter()
        .filter_map(|link| media_q::get_media_by_id(conn, &link.media_id).ok())
        .map(|media| media_summary(&media))
        .collect::<Vec<_>>();
    Ok(json!({"items": items}))
}

fn media_image(
    conn: &DbConnection,
    project_id: &str,
    data_dir: &Path,
    media_id: &str,
    uri: &str,
) -> EngineResult<ResourceContent> {
    let media = media_q::get_media_by_id(conn, media_id)
        .map_err(|_| EngineError::NotFound(format!("media {media_id}")))?;
    ensure_project(project_id, &media.project_id, "media", media_id)?;
    let bytes = std::fs::read(data_dir.join(&media.file_path))
        .map_err(|_| EngineError::NotFound(format!("media file {media_id}")))?;
    Ok(ResourceContent {
        uri: uri.to_string(),
        mime_type: media.mime_type,
        text: None,
        blob: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
    })
}

fn tags(conn: &DbConnection, project_id: &str) -> EngineResult<Value> {
    let posts = post_q::list_posts_by_project(conn, project_id)?;
    let items = tag_q::list_tags_by_project(conn, project_id)?
        .into_iter()
        .map(|tag| {
            let count = posts
                .iter()
                .filter(|post| post.tags.iter().any(|name| name == &tag.name))
                .count();
            json!({"name": tag.name, "color": tag.color, "post_count": count})
        })
        .collect::<Vec<_>>();
    Ok(json!({"items": items}))
}

fn categories(conn: &DbConnection, project_id: &str, data_dir: &Path) -> EngineResult<Value> {
    let posts = post_q::list_posts_by_project(conn, project_id)?;
    let names = crate::engine::meta::read_categories_json(data_dir)
        .unwrap_or_else(|_| post_q::distinct_post_categories(conn, project_id).unwrap_or_default());
    let items = names
        .into_iter()
        .map(|name| {
            let count = posts
                .iter()
                .filter(|post| post.categories.iter().any(|value| value == &name))
                .count();
            json!({"name": name, "post_count": count})
        })
        .collect::<Vec<_>>();
    Ok(json!({"items": items}))
}

fn stats(conn: &DbConnection, project_id: &str, data_dir: &Path) -> EngineResult<Value> {
    let categories = categories(conn, project_id, data_dir)?;
    Ok(json!({
        "post_count": post_q::count_posts_by_project(conn, project_id)?,
        "media_count": media_q::count_media_by_project(conn, project_id)?,
        "tag_count": tag_q::list_tags_by_project(conn, project_id)?.len(),
        "category_count": categories["items"].as_array().map_or(0, Vec::len)
    }))
}

fn ensure_project(expected: &str, actual: &str, entity: &str, id: &str) -> EngineResult<()> {
    if expected == actual {
        Ok(())
    } else {
        Err(EngineError::NotFound(format!("{entity} {id}")))
    }
}
