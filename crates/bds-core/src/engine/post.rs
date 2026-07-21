use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use crate::db::DbConnection as Connection;
use regex::Regex;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::db::fts;
use crate::db::queries::post as qp;
use crate::db::queries::post_link as ql;
use crate::db::queries::post_translation as qt;
use crate::engine::{EngineError, EngineResult, domain_events};
use crate::model::{DomainEntity, NotificationAction, Post, PostLink, PostStatus, PostTranslation};
use crate::util::frontmatter::{
    read_post_file, read_translation_file, write_post_file, write_translation_file,
};
use crate::util::{
    atomic_write_str, ensure_unique, now_unix_ms, post_file_path, slugify, translation_file_path,
};

/// Report returned by `rebuild_posts_from_filesystem`.
#[derive(Debug, Default)]
pub struct RebuildReport {
    pub posts_created: usize,
    pub posts_updated: usize,
    pub translations_created: usize,
    pub translations_updated: usize,
    pub errors: Vec<String>,
}

/// Create a new draft post.
#[expect(
    clippy::too_many_arguments,
    reason = "arguments are the user-supplied post fields"
)]
pub fn create_post(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    title: &str,
    content: Option<&str>,
    tags: Vec<String>,
    categories: Vec<String>,
    author: Option<&str>,
    language: Option<&str>,
    template_slug: Option<&str>,
) -> EngineResult<Post> {
    let id = Uuid::new_v4().to_string();
    let slug_source = if title.is_empty() { "untitled" } else { title };
    let base_slug = slugify(slug_source);
    let base_slug = if base_slug.is_empty() {
        "untitled".to_string()
    } else {
        base_slug
    };
    let slug = ensure_unique(&base_slug, |candidate| {
        qp::get_post_by_project_and_slug(conn, project_id, candidate).is_ok()
    });

    let now = now_unix_ms();
    let post = Post {
        id,
        project_id: project_id.to_string(),
        title: title.to_string(),
        slug,
        excerpt: None,
        content: content.map(|s| s.to_string()),
        status: PostStatus::Draft,
        author: author.map(|s| s.to_string()),
        language: language.map(|s| s.to_string()),
        do_not_translate: false,
        template_slug: template_slug.map(|s| s.to_string()),
        file_path: String::new(),
        checksum: None,
        tags,
        categories,
        published_title: None,
        published_content: None,
        published_tags: None,
        published_categories: None,
        published_excerpt: None,
        created_at: now,
        updated_at: now,
        published_at: None,
    };

    qp::insert_post(conn, &post)?;

    // Index for FTS
    fts_index_post(conn, data_dir, &post)?;

    emit_post(&post, NotificationAction::Created);
    crate::engine::embedding::sync_post_best_effort(conn, data_dir, &post);

    Ok(post)
}

/// Update a post's fields.
#[expect(
    clippy::too_many_arguments,
    reason = "optional arguments represent independent post field changes"
)]
pub fn update_post(
    conn: &Connection,
    data_dir: &Path,
    post_id: &str,
    title: Option<&str>,
    slug: Option<&str>,
    excerpt: Option<Option<&str>>,
    content: Option<&str>,
    tags: Option<Vec<String>>,
    categories: Option<Vec<String>>,
    author: Option<Option<&str>>,
    language: Option<Option<&str>>,
    template_slug: Option<Option<&str>>,
    do_not_translate: Option<bool>,
) -> EngineResult<Post> {
    let mut post = qp::get_post_by_id(conn, post_id)?;

    // Slug frozen after first publish
    if slug.is_some() && post.published_at.is_some() {
        return Err(EngineError::Conflict(
            "slug cannot be changed after publishing".to_string(),
        ));
    }

    // Slug uniqueness check
    if let Some(new_slug) = slug
        && new_slug != post.slug
        && qp::get_post_by_project_and_slug(conn, &post.project_id, new_slug).is_ok()
    {
        return Err(EngineError::Conflict(format!(
            "slug '{new_slug}' already exists in this project"
        )));
    }

    let published_metadata_changed = post.status == PostStatus::Published
        && (title.is_some_and(|value| post.title != value)
            || excerpt.is_some_and(|value| post.excerpt.as_deref() != value)
            || tags
                .as_ref()
                .is_some_and(|value| post.tags.as_slice() != value.as_slice())
            || categories
                .as_ref()
                .is_some_and(|value| post.categories.as_slice() != value.as_slice())
            || author.is_some_and(|value| post.author.as_deref() != value)
            || language.is_some_and(|value| post.language.as_deref() != value)
            || do_not_translate.is_some_and(|value| post.do_not_translate != value));
    let published_body = if post.status == PostStatus::Published
        && (published_metadata_changed || content.is_some())
    {
        resolve_post_fts_content(data_dir, &post)?
    } else {
        None
    };
    let reopen_published = published_metadata_changed
        || (post.status == PostStatus::Published
            && content.is_some_and(|value| published_body.as_deref() != Some(value)));
    let content_changed = content.is_some_and(|value| {
        if post.status == PostStatus::Published {
            published_body.as_deref() != Some(value)
        } else {
            post.content.as_deref() != Some(value)
        }
    });
    let rewrite_published_template = post.status == PostStatus::Published
        && !reopen_published
        && template_slug
            .is_some_and(|value| value.is_some() && post.template_slug.as_deref() != value);

    if let Some(t) = title {
        post.title = t.to_string();
    }
    if let Some(s) = slug {
        post.slug = s.to_string();
    }
    if let Some(exc) = excerpt {
        post.excerpt = exc.map(|s| s.to_string());
    }
    if let Some(c) = content
        && (post.status != PostStatus::Published || reopen_published)
    {
        post.content = Some(c.to_string());
    }
    if let Some(t) = tags {
        post.tags = t;
    }
    if let Some(c) = categories {
        post.categories = c;
    }
    if let Some(a) = author {
        post.author = a.map(|s| s.to_string());
    }
    if let Some(l) = language {
        post.language = l.map(|s| s.to_string());
    }
    if let Some(ts) = template_slug {
        post.template_slug = ts.map(|s| s.to_string());
    }
    if let Some(dnt) = do_not_translate {
        post.do_not_translate = dnt;
    }

    if reopen_published {
        if post.content.is_none() {
            post.content = published_body;
        }
        post.status = PostStatus::Draft;
    }

    post.updated_at = now_unix_ms();
    qp::update_post(conn, &post)?;

    if content_changed {
        sync_post_links(conn, &post, post.content.as_deref().unwrap_or_default())?;
    }

    if rewrite_published_template {
        rewrite_published_post(conn, data_dir, &post.id)?;
    }

    // Re-index FTS
    fts_index_post(conn, data_dir, &post)?;

    emit_post(&post, NotificationAction::Updated);
    crate::engine::embedding::sync_post_best_effort(conn, data_dir, &post);

    Ok(post)
}

/// Rewrite a published post file from the current database frontmatter while
/// retaining the body that lives in the file.
pub fn rewrite_published_post(
    conn: &Connection,
    data_dir: &Path,
    post_id: &str,
) -> EngineResult<()> {
    let post = qp::get_post_by_id(conn, post_id)?;
    if post.status == PostStatus::Published && !post.file_path.is_empty() {
        rewrite_post_file_from_database(data_dir, &post)?;
    }
    Ok(())
}

pub(crate) fn rewrite_post_file_from_database(data_dir: &Path, post: &Post) -> EngineResult<()> {
    let path = data_dir.join(&post.file_path);
    let body = post.content.clone().unwrap_or_else(|| {
        fs::read_to_string(&path)
            .ok()
            .and_then(|content| read_post_file(&content).ok().map(|(_, body)| body))
            .unwrap_or_default()
    });
    atomic_write_str(&path, &write_post_file(post, &body))?;
    Ok(())
}

/// Publish a post: write file, clear content, set published_at.
pub fn publish_post(conn: &Connection, data_dir: &Path, post_id: &str) -> EngineResult<Post> {
    let post = qp::get_post_by_id(conn, post_id)?;

    // Require Draft or Archived status
    match post.status {
        PostStatus::Draft | PostStatus::Archived => {}
        PostStatus::Published => {
            return Err(EngineError::Conflict(
                "post is already published".to_string(),
            ));
        }
    }

    conn.begin_savepoint()?;
    match publish_post_in_savepoint(conn, data_dir, post) {
        Ok(post) => {
            conn.release_savepoint()?;
            emit_post(&post, NotificationAction::Updated);
            crate::engine::embedding::sync_post_best_effort(conn, data_dir, &post);
            Ok(post)
        }
        Err(error) => {
            let _ = conn.rollback_savepoint();
            Err(error)
        }
    }
}

fn publish_post_in_savepoint(
    conn: &Connection,
    data_dir: &Path,
    mut post: Post,
) -> EngineResult<Post> {
    let post_id = post.id.clone();
    let old_rel_path = post.file_path.clone();

    // Compute file_path from created_at + slug.
    let rel_path = post_file_path(post.created_at, &post.slug);
    let abs_path = data_dir.join(&rel_path);

    // Get body: from post.content (draft) or read from existing file (re-publish after archive)
    let body = if let Some(ref c) = post.content {
        c.clone()
    } else if abs_path.exists() {
        let file_content = fs::read_to_string(&abs_path)?;
        let (_fm, body) = read_post_file(&file_content).map_err(EngineError::Parse)?;
        body
    } else {
        String::new()
    };

    // Write frontmatter+body to filesystem
    let now = now_unix_ms();
    let published_at = post.published_at.unwrap_or(now);
    post.published_at = Some(published_at);
    post.status = PostStatus::Published;
    post.file_path = rel_path.clone();
    post.updated_at = now;

    let file_content = write_post_file(&post, &body);
    atomic_write_str(&abs_path, &file_content)?;
    if !old_rel_path.is_empty() && old_rel_path != rel_path {
        match fs::remove_file(data_dir.join(&old_rel_path)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }

    // Persist the published record without touching the legacy published_* columns.
    post.content = None;
    qp::update_post(conn, &post)?;

    // Publish all translations
    let translations = qt::list_post_translations_by_post(conn, &post_id)?;
    for mut t in translations {
        publish_translation(conn, data_dir, &mut t, &post)?;
    }

    sync_post_links(conn, &post, &body)?;

    // Re-index FTS
    fts_index_post(conn, data_dir, &post)?;

    Ok(post)
}

/// Archive a post.
pub fn archive_post(conn: &Connection, data_dir: &Path, post_id: &str) -> EngineResult<()> {
    let mut post = qp::get_post_by_id(conn, post_id)?;
    if post.status == PostStatus::Archived {
        return Err(EngineError::Conflict(
            "post is already archived".to_string(),
        ));
    }
    let now = now_unix_ms();
    post.status = PostStatus::Archived;
    post.updated_at = now;
    qp::update_post(conn, &post)?;
    fts_index_post(conn, data_dir, &post)?;
    emit_post(&post, NotificationAction::Updated);
    crate::engine::embedding::sync_post_best_effort(conn, data_dir, &post);
    Ok(())
}

/// Restore an archived post to an editable draft.
pub fn unarchive_post(conn: &Connection, data_dir: &Path, post_id: &str) -> EngineResult<Post> {
    let mut post = qp::get_post_by_id(conn, post_id)?;
    if post.status != PostStatus::Archived {
        return Err(EngineError::Conflict(
            "cannot unarchive a post that is not archived".to_string(),
        ));
    }

    post.content = Some(restore_content_for_unarchive(data_dir, &post));
    post.status = PostStatus::Draft;
    post.updated_at = now_unix_ms();
    qp::update_post(conn, &post)?;
    fts_index_post(conn, data_dir, &post)?;
    emit_post(&post, NotificationAction::Updated);
    crate::engine::embedding::sync_post_best_effort(conn, data_dir, &post);
    Ok(post)
}

fn restore_content_for_unarchive(data_dir: &Path, post: &Post) -> String {
    post.content.clone().unwrap_or_else(|| {
        if post.file_path.is_empty() {
            return String::new();
        }
        fs::read_to_string(data_dir.join(&post.file_path))
            .ok()
            .and_then(|raw| read_post_file(&raw).ok().map(|(_, body)| body))
            .unwrap_or_default()
    })
}

/// Discard database changes and restore the canonical file version.
pub fn discard_post_draft(conn: &Connection, data_dir: &Path, post_id: &str) -> EngineResult<Post> {
    let mut post = qp::get_post_by_id(conn, post_id)?;
    if post.file_path.is_empty() {
        return Err(EngineError::NotFound(format!(
            "canonical file for post {post_id}"
        )));
    }

    let abs_path = data_dir.join(&post.file_path);
    let raw = fs::read_to_string(&abs_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            EngineError::NotFound(format!("canonical file for post {post_id}"))
        } else {
            EngineError::Io(error)
        }
    })?;
    let (frontmatter, body) = read_post_file(&raw).map_err(EngineError::Parse)?;

    post.title = frontmatter.title;
    post.slug = frontmatter.slug;
    post.excerpt = frontmatter.excerpt;
    post.author = frontmatter.author;
    post.language = frontmatter.language;
    post.template_slug = frontmatter.template_slug;
    post.do_not_translate = frontmatter.do_not_translate;
    post.tags = frontmatter.tags;
    post.categories = frontmatter.categories;
    post.content = None;
    post.status = post_status_from_frontmatter(&frontmatter.status);
    post.checksum = None;
    post.created_at = frontmatter.created_at;
    post.updated_at = frontmatter.updated_at;
    post.published_at = frontmatter.published_at;
    conn.begin_savepoint()?;
    match (|| {
        qp::update_post(conn, &post)?;
        sync_post_links(conn, &post, &body)?;
        fts_index_post(conn, data_dir, &post)?;
        Ok(post)
    })() {
        Ok(post) => {
            conn.release_savepoint()?;
            emit_post(&post, NotificationAction::Updated);
            crate::engine::embedding::sync_post_best_effort(conn, data_dir, &post);
            Ok(post)
        }
        Err(error) => {
            let _ = conn.rollback_savepoint();
            Err(error)
        }
    }
}

fn post_status_from_frontmatter(status: &str) -> PostStatus {
    match status {
        "published" => PostStatus::Published,
        "archived" => PostStatus::Archived,
        _ => PostStatus::Draft,
    }
}

/// Delete a post and all related data.
pub fn delete_post(conn: &Connection, data_dir: &Path, post_id: &str) -> EngineResult<()> {
    let post = qp::get_post_by_id(conn, post_id)?;
    let linked_media_ids = crate::db::queries::post_media::list_post_media_by_post(conn, post_id)?
        .into_iter()
        .map(|link| link.media_id)
        .collect::<Vec<_>>();

    // Delete .md file if exists
    if !post.file_path.is_empty() {
        let abs_path = data_dir.join(&post.file_path);
        if abs_path.exists() {
            fs::remove_file(&abs_path)?;
        }
    }

    // Delete all translation files
    let translations = qt::list_post_translations_by_post(conn, post_id)?;
    for t in &translations {
        if !t.file_path.is_empty() {
            let abs_path = data_dir.join(&t.file_path);
            if abs_path.exists() {
                fs::remove_file(&abs_path)?;
            }
        }
    }

    // Delete all translations from DB
    qt::delete_all_translations_for_post(conn, post_id)?;

    // Delete post links (source and target)
    ql::delete_links_by_source(conn, post_id)?;
    ql::delete_links_by_target(conn, post_id)?;

    // Delete post-media associations
    crate::db::queries::post_media::delete_post_media_by_post(conn, post_id)?;

    // Remove from FTS
    fts::remove_post_from_index(conn, post_id)?;

    // Delete post from DB
    qp::delete_post(conn, post_id)?;

    for media_id in linked_media_ids {
        match crate::engine::media::sync_media_sidecar(conn, data_dir, &media_id) {
            Ok(()) | Err(EngineError::NotFound(_)) => {}
            Err(error) => return Err(error),
        }
    }

    crate::engine::embedding::remove_post_best_effort(conn, data_dir, &post.project_id, post_id);

    emit_post(&post, NotificationAction::Deleted);

    Ok(())
}

fn emit_post(post: &Post, action: NotificationAction) {
    domain_events::entity_changed(&post.project_id, DomainEntity::Post, &post.id, action);
}

/// Compute the canonical URL for a post: /{YYYY}/{MM}/{DD}/{slug}
pub fn canonical_url(created_at_ms: i64, slug: &str) -> String {
    let (y, m, d) = crate::util::timestamp::year_month_day_from_unix_ms(created_at_ms);
    format!("/{y}/{m}/{d}/{slug}")
}

/// Upsert a translation for a post.
pub fn upsert_translation(
    conn: &Connection,
    data_dir: &Path,
    post_id: &str,
    language: &str,
    title: &str,
    excerpt: Option<&str>,
    content: Option<&str>,
) -> EngineResult<PostTranslation> {
    upsert_translation_with_mode(
        conn, data_dir, post_id, language, title, excerpt, content, true,
    )
}

/// Upsert a translation produced by the automatic translation engine without
/// reopening its published canonical source.
pub(crate) fn upsert_automatic_translation(
    conn: &Connection,
    data_dir: &Path,
    post_id: &str,
    language: &str,
    title: &str,
    excerpt: Option<&str>,
    content: Option<&str>,
) -> EngineResult<PostTranslation> {
    upsert_translation_with_mode(
        conn, data_dir, post_id, language, title, excerpt, content, false,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "the final flag distinguishes manual and automatic translation sources"
)]
fn upsert_translation_with_mode(
    conn: &Connection,
    data_dir: &Path,
    post_id: &str,
    language: &str,
    title: &str,
    excerpt: Option<&str>,
    content: Option<&str>,
    manual_edit: bool,
) -> EngineResult<PostTranslation> {
    let mut post = qp::get_post_by_id(conn, post_id)?;
    if post.do_not_translate {
        return Err(EngineError::Validation(
            "cannot create translation for a do-not-translate post".to_string(),
        ));
    }
    let now = now_unix_ms();
    conn.begin_savepoint()?;
    let result = (|| {
        let translation =
            match qt::get_post_translation_by_post_and_language(conn, post_id, language) {
                Ok(mut translation) => {
                    let published_body = if translation.status == PostStatus::Published
                        && !translation.file_path.is_empty()
                    {
                        let raw = fs::read_to_string(data_dir.join(&translation.file_path))?;
                        let (_, body) = read_translation_file(&raw).map_err(EngineError::Parse)?;
                        Some(body)
                    } else {
                        translation.content.clone()
                    };
                    let affects_published_content = translation.title != title
                        || translation.excerpt.as_deref() != excerpt
                        || content.is_some_and(|value| published_body.as_deref() != Some(value));
                    if translation.status == PostStatus::Published && affects_published_content {
                        translation.status = PostStatus::Draft;
                        translation.content = published_body;
                    }
                    translation.title = title.to_string();
                    translation.excerpt = excerpt.map(str::to_string);
                    if let Some(content) = content {
                        translation.content = Some(content.to_string());
                    }
                    translation.updated_at = now;
                    qt::update_post_translation(conn, &translation)?;
                    translation
                }
                Err(diesel::result::Error::NotFound) => {
                    let translation = PostTranslation {
                        id: Uuid::new_v4().to_string(),
                        project_id: post.project_id.clone(),
                        translation_for: post_id.to_string(),
                        language: language.to_string(),
                        title: title.to_string(),
                        excerpt: excerpt.map(str::to_string),
                        content: content.map(str::to_string),
                        status: PostStatus::Draft,
                        file_path: String::new(),
                        checksum: None,
                        created_at: now,
                        updated_at: now,
                        published_at: None,
                    };
                    qt::insert_post_translation(conn, &translation)?;
                    translation
                }
                Err(error) => return Err(error.into()),
            };

        let source_reopened =
            manual_edit && post.status == PostStatus::Published && !post.file_path.is_empty();
        if source_reopened {
            post.content = Some(restore_content_for_unarchive(data_dir, &post));
            post.status = PostStatus::Draft;
            post.updated_at = now;
            qp::update_post(conn, &post)?;
        }
        fts_index_post(conn, data_dir, &post)?;
        Ok((translation, source_reopened))
    })();

    match result {
        Ok((translation, source_reopened)) => {
            conn.release_savepoint()?;
            if source_reopened {
                emit_post(&post, NotificationAction::Updated);
                crate::engine::embedding::sync_post_best_effort(conn, data_dir, &post);
            }
            Ok(translation)
        }
        Err(error) => {
            let _ = conn.rollback_savepoint();
            Err(error)
        }
    }
}

/// Delete a translation.
pub fn delete_translation(
    conn: &Connection,
    data_dir: &Path,
    translation_id: &str,
) -> EngineResult<()> {
    let t = qt::get_post_translation_by_id(conn, translation_id)?;

    // Delete file if exists
    if !t.file_path.is_empty() {
        let abs_path = data_dir.join(&t.file_path);
        if abs_path.exists() {
            fs::remove_file(&abs_path)?;
        }
    }

    qt::delete_post_translation(conn, translation_id)?;

    // Re-index FTS for parent post
    if let Ok(post) = qp::get_post_by_id(conn, &t.translation_for) {
        fts_index_post(conn, data_dir, &post)?;
    }

    Ok(())
}

/// Publish one draft translation without republishing its canonical post.
pub fn publish_post_translation(
    conn: &Connection,
    data_dir: &Path,
    translation_id: &str,
) -> EngineResult<PostTranslation> {
    let mut translation = qt::get_post_translation_by_id(conn, translation_id)?;
    if translation.status == PostStatus::Published {
        return Ok(translation);
    }
    let post = qp::get_post_by_id(conn, &translation.translation_for)?;
    conn.begin_savepoint()?;
    match publish_translation(conn, data_dir, &mut translation, &post) {
        Ok(()) => {
            fts_index_post(conn, data_dir, &post)?;
            conn.release_savepoint()?;
            Ok(translation)
        }
        Err(error) => {
            let _ = conn.rollback_savepoint();
            Err(error)
        }
    }
}

/// Rebuild posts from filesystem. Walk posts/ dir, parse .md files, upsert into DB.
pub fn rebuild_posts_from_filesystem(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<RebuildReport> {
    rebuild_posts_from_filesystem_with_progress(conn, data_dir, project_id, None)
}

/// Per-item progress callback: (current_item, total_items, item_description).
pub type ItemProgressFn = Box<dyn Fn(usize, usize, &str) + Send>;

/// Like `rebuild_posts_from_filesystem` but with optional per-item progress.
pub fn rebuild_posts_from_filesystem_with_progress(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    on_item: Option<ItemProgressFn>,
) -> EngineResult<RebuildReport> {
    let mut report = RebuildReport::default();
    let posts_dir = data_dir.join("posts");

    if !posts_dir.exists() {
        return Ok(report);
    }

    // Collect all .md files
    let mut canonical_files = Vec::new();
    let mut translation_files = Vec::new();

    for entry in WalkDir::new(&posts_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str());
        if ext != Some("md") {
            continue;
        }

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if is_translation_filename(stem) {
            translation_files.push(path.to_path_buf());
        } else {
            canonical_files.push(path.to_path_buf());
        }
    }

    let total = canonical_files.len() + translation_files.len();

    // Process canonical posts first
    for (i, path) in canonical_files.iter().enumerate() {
        if let Some(ref cb) = on_item {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
            cb(i + 1, total, name);
        }
        match rebuild_canonical_post(conn, data_dir, project_id, path) {
            Ok(created) => {
                if created {
                    report.posts_created += 1;
                } else {
                    report.posts_updated += 1;
                }
            }
            Err(e) => {
                report.errors.push(format!("{}: {e}", path.display()));
            }
        }
    }

    // Process translations
    let offset = canonical_files.len();
    for (i, path) in translation_files.iter().enumerate() {
        if let Some(ref cb) = on_item {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
            cb(offset + i + 1, total, name);
        }
        match rebuild_translation(conn, data_dir, project_id, path) {
            Ok(created) => {
                if created {
                    report.translations_created += 1;
                } else {
                    report.translations_updated += 1;
                }
            }
            Err(e) => {
                report.errors.push(format!("{}: {e}", path.display()));
            }
        }
    }

    // Resolve links after every canonical post exists so filesystem order cannot
    // make links to later files disappear.
    rebuild_all_links(conn, data_dir, project_id)?;

    // Re-index FTS for all posts in this project
    let posts = qp::list_posts_by_project(conn, project_id)?;
    for post in &posts {
        fts_index_post(conn, data_dir, post)?;
    }

    Ok(report)
}

/// Rebuild the inter-post link graph for every post in a project, resolving
/// draft bodies from the database and published bodies from the filesystem.
pub fn rebuild_all_links(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<usize> {
    let posts = qp::list_posts_by_project(conn, project_id)?;
    let mut link_count = 0;

    for post in &posts {
        // Get post content: from DB or filesystem
        let content = if let Some(ref content) = post.content {
            content.clone()
        } else if !post.file_path.is_empty() {
            let abs_path = data_dir.join(&post.file_path);
            fs::read_to_string(&abs_path)
                .ok()
                .and_then(|raw| read_post_file(&raw).ok().map(|(_, body)| body))
                .unwrap_or_default()
        } else {
            String::new()
        };

        link_count += sync_post_links(conn, post, &content)?;
    }

    Ok(link_count)
}

// --- Internal helpers ---

/// Replace one post's outgoing link graph from its resolved body.
pub fn sync_post_links(conn: &Connection, post: &Post, body: &str) -> EngineResult<usize> {
    conn.begin_savepoint()?;
    let result = (|| {
        ql::delete_links_by_source(conn, &post.id)?;
        let now = now_unix_ms();
        let mut inserted = HashSet::new();
        let mut link_count = 0;
        for (target_slug, link_text) in parse_post_links(body) {
            let target =
                match qp::get_post_by_project_and_slug(conn, &post.project_id, &target_slug) {
                    Ok(target) => target,
                    Err(diesel::result::Error::NotFound) => continue,
                    Err(error) => return Err(error.into()),
                };
            if !inserted.insert((target.id.clone(), link_text.clone())) {
                continue;
            }
            ql::insert_post_link(
                conn,
                &PostLink {
                    id: Uuid::new_v4().to_string(),
                    source_post_id: post.id.clone(),
                    target_post_id: target.id,
                    link_text,
                    created_at: now,
                },
            )?;
            link_count += 1;
        }
        Ok(link_count)
    })();
    match result {
        Ok(link_count) => {
            conn.release_savepoint()?;
            Ok(link_count)
        }
        Err(error) => {
            let _ = conn.rollback_savepoint();
            Err(error)
        }
    }
}

/// Parse Markdown and HTML links into bDS2-compatible post slugs and labels.
fn parse_post_links(content: &str) -> Vec<(String, Option<String>)> {
    static MARKDOWN_LINK: OnceLock<Regex> = OnceLock::new();
    static HTML_LINK: OnceLock<Regex> = OnceLock::new();
    let markdown = MARKDOWN_LINK
        .get_or_init(|| Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").expect("valid Markdown link regex"));
    let html = HTML_LINK.get_or_init(|| {
        Regex::new(r#"(?is)<a\s+[^>]*href=["']([^"']+)["'][^>]*>(.*?)</a>"#)
            .expect("valid HTML link regex")
    });
    let markdown_links = markdown.captures_iter(content).filter_map(|captures| {
        parsed_post_link(captures.get(2)?.as_str(), captures.get(1)?.as_str())
    });
    let html_links = html.captures_iter(content).filter_map(|captures| {
        parsed_post_link(captures.get(1)?.as_str(), captures.get(2)?.as_str())
    });
    markdown_links.chain(html_links).collect()
}

fn parsed_post_link(href: &str, text: &str) -> Option<(String, Option<String>)> {
    static HTML_TAG: OnceLock<Regex> = OnceLock::new();
    let base = url::Url::parse("https://ruds.invalid/").expect("valid internal link base");
    let url = base.join(href.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let slug = post_slug_from_path(url.path())?;
    let plain_text = HTML_TAG
        .get_or_init(|| Regex::new(r"<[^>]+>").expect("valid HTML tag regex"))
        .replace_all(text, "");
    let plain_text = plain_text.trim();
    Some((
        slug.to_string(),
        (!plain_text.is_empty()).then(|| plain_text.to_string()),
    ))
}

fn post_slug_from_path(path: &str) -> Option<&str> {
    let segments = path
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    match segments.as_slice() {
        [year, month, day, slug]
            if is_year(year) && is_month_or_day(month) && is_month_or_day(day) =>
        {
            Some(slug)
        }
        [language, year, month, day, slug]
            if is_language(language)
                && is_year(year)
                && is_month_or_day(month)
                && is_month_or_day(day) =>
        {
            Some(slug)
        }
        [slug] => Some(slug),
        [language, slug] if is_language(language) => Some(slug),
        _ => None,
    }
}

fn is_year(value: &str) -> bool {
    value.len() == 4 && value.chars().all(|character| character.is_ascii_digit())
}

fn is_month_or_day(value: &str) -> bool {
    value.len() == 2 && value.chars().all(|character| character.is_ascii_digit())
}

fn is_language(value: &str) -> bool {
    value.len() == 2
        && value
            .chars()
            .all(|character| character.is_ascii_lowercase())
}

/// Publish a single translation: write file, clear content, set status.
fn publish_translation(
    conn: &Connection,
    data_dir: &Path,
    t: &mut PostTranslation,
    post: &Post,
) -> EngineResult<()> {
    let rel_path = translation_file_path(post.created_at, &post.slug, &t.language);
    let abs_path = data_dir.join(&rel_path);

    // Get body
    let body = if let Some(ref c) = t.content {
        c.clone()
    } else if abs_path.exists() {
        let file_content = fs::read_to_string(&abs_path)?;
        let (_fm, body) = read_translation_file(&file_content).map_err(EngineError::Parse)?;
        body
    } else {
        String::new()
    };

    let now = now_unix_ms();
    t.status = PostStatus::Published;
    t.file_path = rel_path;
    t.published_at = Some(t.published_at.unwrap_or(now));
    t.updated_at = now;

    let file_content = write_translation_file(t, &body);
    atomic_write_str(&abs_path, &file_content)?;

    t.content = None;

    qt::update_post_translation(conn, t)?;
    Ok(())
}

/// Index a post in FTS, gathering translation texts.
fn fts_index_post(conn: &Connection, data_dir: &Path, post: &Post) -> EngineResult<()> {
    let translations = qt::list_post_translations_by_post(conn, &post.id).unwrap_or_default();
    let translation_data: Vec<fts::PostTranslationFts> = translations
        .iter()
        .map(|t| {
            Ok(fts::PostTranslationFts {
                title: t.title.clone(),
                excerpt: t.excerpt.clone(),
                content: resolve_translation_fts_content(data_dir, t)?,
                language: t.language.clone(),
            })
        })
        .collect::<EngineResult<_>>()?;

    let main_language = crate::engine::meta::read_project_json(data_dir)
        .ok()
        .and_then(|metadata| metadata.main_language)
        .unwrap_or_else(|| "en".to_string());
    let content = resolve_post_fts_content(data_dir, post)?;
    let lang = post.language.as_deref().unwrap_or(&main_language);
    fts::index_post(
        conn,
        &post.id,
        &post.title,
        post.excerpt.as_deref(),
        content.as_deref(),
        &post.tags,
        &post.categories,
        &translation_data,
        lang,
    )?;
    Ok(())
}

fn resolve_post_fts_content(data_dir: &Path, post: &Post) -> EngineResult<Option<String>> {
    if post.content.is_some() {
        return Ok(post.content.clone());
    }
    if post.file_path.is_empty() {
        return Ok(None);
    }
    let raw = fs::read_to_string(data_dir.join(&post.file_path))?;
    let (_fm, body) = read_post_file(&raw).map_err(EngineError::Parse)?;
    Ok(Some(body))
}

fn resolve_translation_fts_content(
    data_dir: &Path,
    translation: &PostTranslation,
) -> EngineResult<Option<String>> {
    if translation.content.is_some() {
        return Ok(translation.content.clone());
    }
    if translation.file_path.is_empty() {
        return Ok(None);
    }
    let raw = fs::read_to_string(data_dir.join(&translation.file_path))?;
    let (_fm, body) = read_translation_file(&raw).map_err(EngineError::Parse)?;
    Ok(Some(body))
}

/// Check if a file stem looks like a translation filename: `{slug}.{lang}`
/// where lang is a 2-letter code. We look for a dot followed by exactly 2 lowercase letters.
pub(crate) fn is_translation_filename(stem: &str) -> bool {
    if let Some(dot_pos) = stem.rfind('.') {
        let suffix = &stem[dot_pos + 1..];
        suffix.len() == 2 && suffix.chars().all(|c| c.is_ascii_lowercase())
    } else {
        false
    }
}

/// Rebuild a canonical post from a .md file. Returns true if created, false if updated.
pub(crate) fn rebuild_canonical_post(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    path: &Path,
) -> EngineResult<bool> {
    let content = fs::read_to_string(path)?;
    let (fm, body) = read_post_file(&content).map_err(EngineError::Parse)?;

    let rel_path = path
        .strip_prefix(data_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let status = post_status_from_frontmatter(&fm.status);

    // Check if post exists in DB by id
    let existing = qp::get_post_by_id(conn, &fm.id);
    match existing {
        Ok(mut post) => {
            // Update existing post
            post.title = fm.title;
            post.slug = fm.slug;
            post.excerpt = fm.excerpt;
            post.content = if status == PostStatus::Published {
                None
            } else {
                Some(body.clone())
            };
            post.status = status;
            post.author = fm.author;
            post.language = fm.language;
            post.do_not_translate = fm.do_not_translate;
            post.template_slug = fm.template_slug;
            post.file_path = rel_path;
            post.checksum = None;
            post.tags = fm.tags;
            post.categories = fm.categories;
            post.created_at = fm.created_at;
            post.updated_at = fm.updated_at;
            post.published_at = fm.published_at;
            qp::update_post(conn, &post)?;
            sync_post_links(conn, &post, &body)?;
            Ok(false)
        }
        Err(diesel::result::Error::NotFound) => {
            // Insert new post
            let post = Post {
                id: fm.id,
                project_id: project_id.to_string(),
                title: fm.title,
                slug: fm.slug,
                excerpt: fm.excerpt,
                content: if status == PostStatus::Published {
                    None
                } else {
                    Some(body.clone())
                },
                status,
                author: fm.author,
                language: fm.language,
                do_not_translate: fm.do_not_translate,
                template_slug: fm.template_slug,
                file_path: rel_path,
                checksum: None,
                tags: fm.tags,
                categories: fm.categories,
                published_title: None,
                published_content: None,
                published_tags: None,
                published_categories: None,
                published_excerpt: None,
                created_at: fm.created_at,
                updated_at: fm.updated_at,
                published_at: fm.published_at,
            };
            qp::insert_post(conn, &post)?;
            sync_post_links(conn, &post, &body)?;
            Ok(true)
        }
        Err(error) => Err(error.into()),
    }
}

/// Rebuild a translation from a .{lang}.md file. Returns true if created, false if updated.
pub(crate) fn rebuild_translation(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    path: &Path,
) -> EngineResult<bool> {
    let content = fs::read_to_string(path)?;
    let (fm, body) = read_translation_file(&content).map_err(EngineError::Parse)?;

    let rel_path = path
        .strip_prefix(data_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    // Check if parent post exists
    let parent = qp::get_post_by_id(conn, &fm.translation_for);
    if parent.is_err() {
        return Err(EngineError::NotFound(format!(
            "parent post '{}' not found for translation",
            fm.translation_for
        )));
    }
    let parent = parent.unwrap();

    // Current files carry independent translation metadata. Legacy files fall back to the
    // canonical post values for compatibility.
    let status = match fm.status.as_deref() {
        Some("published") => PostStatus::Published,
        Some("draft") => PostStatus::Draft,
        _ if parent.status == PostStatus::Published => PostStatus::Published,
        _ => PostStatus::Draft,
    };
    let created_at = fm.created_at.unwrap_or(parent.created_at);
    let updated_at = fm.updated_at.unwrap_or(parent.updated_at);
    let published_at = fm.published_at.or(parent.published_at);

    // Check if translation exists
    let existing =
        qt::get_post_translation_by_post_and_language(conn, &fm.translation_for, &fm.language);
    match existing {
        Ok(mut t) => {
            t.title = fm.title;
            t.excerpt = fm.excerpt;
            t.content = if status == PostStatus::Published {
                None
            } else {
                Some(body)
            };
            t.status = status;
            t.file_path = rel_path;
            t.checksum = None;
            t.created_at = created_at;
            t.updated_at = updated_at;
            t.published_at = published_at;
            qt::update_post_translation(conn, &t)?;
            Ok(false)
        }
        Err(_) => {
            let t = PostTranslation {
                id: fm.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
                project_id: project_id.to_string(),
                translation_for: fm.translation_for,
                language: fm.language,
                title: fm.title,
                excerpt: fm.excerpt,
                content: if status == PostStatus::Published {
                    None
                } else {
                    Some(body)
                },
                status,
                file_path: rel_path,
                checksum: None,
                created_at,
                updated_at,
                published_at,
            };
            qt::insert_post_translation(conn, &t)?;
            Ok(true)
        }
    }
}

// ───────────────────────────────────────────────────────────
// M3: Editor Actions
// ───────────────────────────────────────────────────────────

/// Insert a link to another post in the editor buffer.
/// Returns the Markdown link syntax.
pub fn post_insert_link(slug: &str) -> String {
    format!("[title](/YYYY/MM/DD/{slug})")
}

/// Insert a media reference in the editor buffer.
/// Returns Markdown with a host-absolute media URL suitable for rendered HTML.
pub fn post_insert_media(media_path: &str, is_image: bool, original_name: &str) -> String {
    let url = format!("/{}", media_path.trim_start_matches('/'));
    if is_image {
        format!("![]({url})")
    } else {
        format!("[{original_name}]({url})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::fts::ensure_fts_tables;
    use crate::db::queries::project::{insert_project, make_test_project};
    use tempfile::TempDir;

    fn setup() -> (Database, TempDir) {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        ensure_fts_tables(db.conn()).unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = TempDir::new().unwrap();
        (db, dir)
    }

    fn create_published_post(db: &Database, dir: &TempDir, title: &str, body: &str) -> Post {
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            title,
            Some(body),
            vec!["tag".into()],
            vec!["category".into()],
            Some("Alice"),
            Some("en"),
            None,
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap()
    }

    #[test]
    fn create_post_generates_slug_and_draft() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Hello World",
            Some("body text"),
            vec!["rust".into()],
            vec!["tech".into()],
            Some("Alice"),
            Some("en"),
            None,
        )
        .unwrap();

        assert_eq!(post.slug, "hello-world");
        assert_eq!(post.status, PostStatus::Draft);
        assert_eq!(post.title, "Hello World");
        assert_eq!(post.tags, vec!["rust"]);
        assert_eq!(post.categories, vec!["tech"]);
        assert_eq!(post.content.as_deref(), Some("body text"));
        assert!(post.file_path.is_empty());
        assert!(post.published_at.is_none());

        // Verify it's in DB
        let fetched = qp::get_post_by_id(db.conn(), &post.id).unwrap();
        assert_eq!(fetched.slug, "hello-world");
        assert_eq!(fetched.status, PostStatus::Draft);
    }

    #[test]
    fn inserted_media_uses_host_absolute_renderable_paths() {
        assert_eq!(
            post_insert_media("media/2026/07/image.png", true, "image.png"),
            "![](/media/2026/07/image.png)"
        );
        assert_eq!(
            post_insert_media("/media/2026/07/file.pdf", false, "file.pdf"),
            "[file.pdf](/media/2026/07/file.pdf)"
        );
    }

    #[test]
    fn publish_indexes_post_body_from_file_when_db_content_is_cleared() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Published Search",
            Some("distinctive quokkafire body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap();
        let stored = qp::get_post_by_id(db.conn(), &post.id).unwrap();
        assert!(stored.content.is_none());
        assert_eq!(
            crate::db::fts::search_posts(db.conn(), "quokkafire", "en").unwrap(),
            vec![post.id]
        );
    }

    #[test]
    fn publish_indexes_translation_body_from_file_when_db_content_is_cleared() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Translated Search",
            Some("canonical body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "Übersetzte Suche",
            None,
            Some("markantes drachenfalter wort"),
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap();
        assert_eq!(
            crate::db::fts::search_posts(db.conn(), "drachenfalter", "de").unwrap(),
            vec![post.id]
        );
    }

    #[test]
    fn create_post_empty_title_uses_untitled() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "",
            None,
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(post.slug, "untitled");
    }

    #[test]
    fn create_post_unique_slugs() {
        let (db, dir) = setup();
        let p1 = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Dupe",
            None,
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        let p2 = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Dupe",
            None,
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(p1.slug, "dupe");
        assert_eq!(p2.slug, "dupe-2");
    }

    #[test]
    fn update_post_changes_fields() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Original",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();

        let updated = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            Some("Updated Title"),
            Some("new-slug"),
            Some(Some("new excerpt")),
            Some("new body"),
            Some(vec!["tag1".into()]),
            Some(vec!["cat1".into()]),
            Some(Some("Bob")),
            Some(Some("de")),
            None,
            Some(true),
        )
        .unwrap();

        assert_eq!(updated.title, "Updated Title");
        assert_eq!(updated.slug, "new-slug");
        assert_eq!(updated.excerpt.as_deref(), Some("new excerpt"));
        assert_eq!(updated.content.as_deref(), Some("new body"));
        assert_eq!(updated.tags, vec!["tag1"]);
        assert_eq!(updated.categories, vec!["cat1"]);
        assert_eq!(updated.author.as_deref(), Some("Bob"));
        assert_eq!(updated.language.as_deref(), Some("de"));
        assert!(updated.do_not_translate);
    }

    #[test]
    fn draft_content_updates_replace_outgoing_post_links_without_publishing() {
        let (db, dir) = setup();
        let first = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "First Target",
            Some("first"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        let second = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Second Target",
            Some("second"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        let source = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Source",
            Some("no links"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();

        update_post(
            db.conn(),
            dir.path(),
            &source.id,
            None,
            None,
            None,
            Some("[first](/2024/01/01/first-target) [first](/2024/01/01/first-target) [second](/2024/01/01/second-target)"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let links = ql::list_links_by_source(db.conn(), &source.id).unwrap();
        assert_eq!(links.len(), 2);
        assert!(links.iter().any(|link| link.target_post_id == first.id));
        assert!(links.iter().any(|link| link.target_post_id == second.id));

        update_post(
            db.conn(),
            dir.path(),
            &source.id,
            None,
            None,
            None,
            Some("[second](/2024/01/01/second-target)"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let links = ql::list_links_by_source(db.conn(), &source.id).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_post_id, second.id);
    }

    #[test]
    fn identical_published_update_stays_published() {
        let (db, dir) = setup();
        let post = create_published_post(&db, &dir, "Published", "body");

        let updated = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            Some("Published"),
            None,
            Some(None),
            Some("body"),
            Some(vec!["tag".into()]),
            Some(vec!["category".into()]),
            Some(Some("Alice")),
            Some(Some("en")),
            Some(None),
            Some(false),
        )
        .unwrap();

        assert_eq!(updated.status, PostStatus::Published);
        assert_eq!(updated.content, None);
    }

    #[test]
    fn published_updates_ignore_legacy_snapshot_values() {
        let (db, dir) = setup();
        let mut post = create_published_post(&db, &dir, "Published", "canonical body");
        post.published_title = Some("Different Legacy Title".into());
        post.published_content = Some("replacement body".into());
        post.published_tags = Some("[\"legacy\"]".into());
        qp::update_post(db.conn(), &post).unwrap();

        let identical = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            Some("Published"),
            None,
            None,
            Some("canonical body"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(identical.status, PostStatus::Published);

        let changed = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            None,
            None,
            None,
            Some("replacement body"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(changed.status, PostStatus::Draft);
        assert_eq!(changed.content.as_deref(), Some("replacement body"));
    }

    #[test]
    fn published_title_change_reopens_draft_with_file_body() {
        let (db, dir) = setup();
        let post = create_published_post(&db, &dir, "Published", "body");

        let updated = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            Some("Changed"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(updated.status, PostStatus::Draft);
        assert_eq!(updated.content.as_deref(), Some("body"));
    }

    #[test]
    fn published_template_slug_only_change_stays_published() {
        let (db, dir) = setup();
        let mut post = create_published_post(&db, &dir, "Published", "body");
        post.checksum = Some("caller-checksum".into());
        qp::update_post(db.conn(), &post).unwrap();
        let post_path = dir.path().join(&post.file_path);
        let original_file = fs::read_to_string(&post_path).unwrap();

        let updated = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(Some("page")),
            None,
        )
        .unwrap();

        assert_eq!(updated.status, PostStatus::Published);
        assert_eq!(updated.template_slug.as_deref(), Some("page"));
        assert_eq!(updated.content, None);
        assert_eq!(updated.published_at, post.published_at);
        assert_eq!(updated.checksum.as_deref(), Some("caller-checksum"));

        let rewritten_file = fs::read_to_string(post_path).unwrap();
        assert_ne!(rewritten_file, original_file);
        let (frontmatter, body) = read_post_file(&rewritten_file).unwrap();
        assert_eq!(frontmatter.template_slug.as_deref(), Some("page"));
        assert_eq!(frontmatter.published_at, post.published_at);
        assert_eq!(body, "body");
    }

    #[test]
    fn published_content_change_reopens_draft_with_new_body() {
        let (db, dir) = setup();
        let post = create_published_post(&db, &dir, "Published", "body");

        let updated = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            None,
            None,
            None,
            Some("changed body"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(updated.status, PostStatus::Draft);
        assert_eq!(updated.content.as_deref(), Some("changed body"));
    }

    #[test]
    fn update_post_slug_frozen_after_publish() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Frozen Slug",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();

        // Publish the post
        publish_post(db.conn(), dir.path(), &post.id).unwrap();

        // Archive so we can try updating
        archive_post(db.conn(), dir.path(), &post.id).unwrap();

        // Try to change slug - should fail
        let result = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            None,
            Some("different-slug"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );

        assert!(result.is_err());
        match result.unwrap_err() {
            EngineError::Conflict(_) => {} // expected
            other => panic!("expected Conflict, got: {other}"),
        }
    }

    #[test]
    fn publish_post_writes_file_and_clears_content() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Publish Me",
            Some("my body content"),
            vec!["rust".into()],
            vec!["tech".into()],
            None,
            None,
            None,
        )
        .unwrap();

        let published = publish_post(db.conn(), dir.path(), &post.id).unwrap();

        // Status should be Published
        assert_eq!(published.status, PostStatus::Published);

        // Content should be None in DB
        let from_db = qp::get_post_by_id(db.conn(), &post.id).unwrap();
        assert!(from_db.content.is_none());

        // file_path should be set
        assert!(!from_db.file_path.is_empty());

        // published_at should be set
        assert!(from_db.published_at.is_some());

        // Legacy published snapshot columns stay empty, matching bDS2.
        assert!(from_db.published_title.is_none());
        assert!(from_db.published_content.is_none());
        assert!(from_db.published_tags.is_none());
        assert!(from_db.published_categories.is_none());
        assert!(from_db.published_excerpt.is_none());

        // File should exist on disk
        let abs_path = dir.path().join(&from_db.file_path);
        assert!(abs_path.exists());

        // File should contain frontmatter and body
        let file_content = fs::read_to_string(&abs_path).unwrap();
        assert!(file_content.contains("my body content"));
        assert!(file_content.contains("Publish Me"));
    }

    #[test]
    fn publish_post_preserves_legacy_snapshot_values_without_using_them() {
        let (db, dir) = setup();
        let mut post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Publish Me",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        post.published_title = Some("Legacy Title".into());
        post.published_content = Some("Legacy Body".into());
        post.published_tags = Some("[\"legacy\"]".into());
        post.published_categories = Some("[\"old\"]".into());
        post.published_excerpt = Some("Legacy Excerpt".into());
        qp::update_post(db.conn(), &post).unwrap();

        publish_post(db.conn(), dir.path(), &post.id).unwrap();

        let published = qp::get_post_by_id(db.conn(), &post.id).unwrap();
        assert_eq!(published.published_title, post.published_title);
        assert_eq!(published.published_content, post.published_content);
        assert_eq!(published.published_tags, post.published_tags);
        assert_eq!(published.published_categories, post.published_categories);
        assert_eq!(published.published_excerpt, post.published_excerpt);
    }

    #[test]
    fn publish_replaces_divergent_post_path_and_ignores_missing_old_file() {
        let (db, dir) = setup();
        let mut post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Moved Post",
            Some("current body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        post.file_path = "posts/legacy/moved-post.md".to_string();
        qp::update_post(db.conn(), &post).unwrap();
        let old_path = dir.path().join(&post.file_path);
        fs::create_dir_all(old_path.parent().unwrap()).unwrap();
        fs::write(&old_path, "stale legacy file").unwrap();

        let published = publish_post(db.conn(), dir.path(), &post.id).unwrap();

        assert_ne!(published.file_path, post.file_path);
        assert!(!old_path.exists());
        assert!(dir.path().join(&published.file_path).is_file());
        assert_eq!(
            qp::get_post_by_id(db.conn(), &post.id).unwrap().file_path,
            published.file_path
        );

        let mut missing = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Missing Legacy Post",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        missing.file_path = "posts/legacy/already-gone.md".to_string();
        qp::update_post(db.conn(), &missing).unwrap();

        let published_missing = publish_post(db.conn(), dir.path(), &missing.id).unwrap();
        assert!(dir.path().join(&published_missing.file_path).is_file());
    }

    #[test]
    fn translation_publish_retains_divergent_old_file_like_bds2() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Translated Move",
            Some("body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let mut translation = upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "Verschoben",
            None,
            Some("Inhalt"),
        )
        .unwrap();
        translation.file_path = "posts/legacy/translated-move.de.md".to_string();
        qt::update_post_translation(db.conn(), &translation).unwrap();
        let old_path = dir.path().join(&translation.file_path);
        fs::create_dir_all(old_path.parent().unwrap()).unwrap();
        fs::write(&old_path, "legacy translation").unwrap();

        publish_post(db.conn(), dir.path(), &post.id).unwrap();

        let published = qt::get_post_translation_by_id(db.conn(), &translation.id).unwrap();
        assert_ne!(published.file_path, translation.file_path);
        assert!(old_path.is_file());
        assert!(dir.path().join(&published.file_path).is_file());
    }

    #[test]
    fn discard_post_draft_restores_published_state() {
        let (db, dir) = setup();
        let published_target = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Published Target",
            None,
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        let draft_target = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Draft Target",
            None,
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Discard Me",
            Some("[published](/2024/01/01/published-target)"),
            vec!["one".into()],
            vec!["cat".into()],
            Some("Alice"),
            Some("en"),
            None,
        )
        .unwrap();

        let published = publish_post(db.conn(), dir.path(), &post.id).unwrap();
        let updated = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            Some("Changed Title"),
            None,
            Some(Some("changed excerpt")),
            Some("[draft](/2024/01/01/draft-target)"),
            Some(vec!["two".into()]),
            Some(vec!["other".into()]),
            Some(Some("Bob")),
            Some(Some("de")),
            None,
            Some(true),
        )
        .unwrap();

        assert_eq!(updated.status, PostStatus::Draft);
        assert_eq!(
            updated.content.as_deref(),
            Some("[draft](/2024/01/01/draft-target)")
        );
        let draft_links = ql::list_links_by_source(db.conn(), &post.id).unwrap();
        assert_eq!(draft_links.len(), 1);
        assert_eq!(draft_links[0].target_post_id, draft_target.id);

        let discarded = discard_post_draft(db.conn(), dir.path(), &post.id).unwrap();
        assert_eq!(discarded.status, PostStatus::Published);
        assert_eq!(discarded.title, published.title);
        assert_eq!(discarded.excerpt, published.excerpt);
        assert_eq!(discarded.tags, vec!["one"]);
        assert_eq!(discarded.categories, vec!["cat"]);
        assert_eq!(discarded.content, None);
        assert_eq!(discarded.language.as_deref(), Some("en"));
        let restored_links = ql::list_links_by_source(db.conn(), &post.id).unwrap();
        assert_eq!(restored_links.len(), 1);
        assert_eq!(restored_links[0].target_post_id, published_target.id);
        assert!(!discarded.do_not_translate);
    }

    #[test]
    fn discard_post_draft_requires_the_canonical_file() {
        let (db, dir) = setup();
        let post = create_published_post(&db, &dir, "Published Title", "published body");
        let draft = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            Some("Draft Title"),
            None,
            None,
            Some("draft body"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        fs::remove_file(dir.path().join(&draft.file_path)).unwrap();

        let result = discard_post_draft(db.conn(), dir.path(), &post.id);

        assert!(matches!(result, Err(EngineError::NotFound(_))));
        let unchanged = qp::get_post_by_id(db.conn(), &post.id).unwrap();
        assert_eq!(unchanged.title, "Draft Title");
        assert_eq!(unchanged.content.as_deref(), Some("draft body"));
        assert_eq!(unchanged.status, PostStatus::Draft);
    }

    #[test]
    fn discard_post_draft_uses_the_file_instead_of_published_at_as_guard() {
        let (db, dir) = setup();
        let post = create_published_post(&db, &dir, "Published Title", "published body");
        let mut draft = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            Some("Draft Title"),
            None,
            None,
            Some("draft body"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        draft.published_at = None;
        qp::update_post(db.conn(), &draft).unwrap();

        let discarded = discard_post_draft(db.conn(), dir.path(), &post.id).unwrap();

        assert_eq!(discarded.title, "Published Title");
        assert_eq!(discarded.content, None);
        assert_eq!(discarded.status, PostStatus::Published);
        assert_eq!(discarded.published_at, post.published_at);
    }

    #[test]
    fn discard_post_draft_restores_status_from_frontmatter() {
        let (db, dir) = setup();
        let post = create_published_post(&db, &dir, "Published Title", "published body");
        update_post(
            db.conn(),
            dir.path(),
            &post.id,
            Some("Draft Title"),
            None,
            None,
            Some("draft body"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let post_path = dir.path().join(&post.file_path);
        let file = fs::read_to_string(&post_path).unwrap();
        fs::write(
            &post_path,
            file.replacen("status: published", "status: archived", 1),
        )
        .unwrap();

        let discarded = discard_post_draft(db.conn(), dir.path(), &post.id).unwrap();

        assert_eq!(discarded.status, PostStatus::Archived);
        assert_eq!(discarded.content, None);
    }

    #[test]
    fn discard_works_after_deployed_search_index_schema_is_repaired() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Deployed Schema",
            Some("published body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap();
        update_post(
            db.conn(),
            dir.path(),
            &post.id,
            Some("Draft Title"),
            None,
            None,
            Some("draft body"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();

        crate::db::fts::install_deployed_schema_for_test(db.conn()).unwrap();

        assert!(crate::engine::search::prepare_search_index(db.conn()).unwrap());
        crate::engine::search::rebuild_search_index(db.conn(), None).unwrap();
        let discarded = discard_post_draft(db.conn(), dir.path(), &post.id).unwrap();

        assert_eq!(discarded.title, "Deployed Schema");
        assert_eq!(discarded.status, PostStatus::Published);
        assert!(!crate::engine::search::search_index_rebuild_required(db.conn()).unwrap());
    }

    #[test]
    fn discard_rolls_back_when_search_index_update_fails() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Published Title",
            Some("published body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap();
        update_post(
            db.conn(),
            dir.path(),
            &post.id,
            Some("Draft Title"),
            None,
            None,
            Some("draft body"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        crate::db::fts::install_deployed_schema_for_test(db.conn()).unwrap();

        assert!(discard_post_draft(db.conn(), dir.path(), &post.id).is_err());
        let unchanged = qp::get_post_by_id(db.conn(), &post.id).unwrap();
        assert_eq!(unchanged.title, "Draft Title");
        assert_eq!(unchanged.status, PostStatus::Draft);
        assert_eq!(unchanged.content.as_deref(), Some("draft body"));
    }

    #[test]
    fn publish_preserves_published_at_on_republish() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Republish",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();

        let published = publish_post(db.conn(), dir.path(), &post.id).unwrap();
        let original_published_at = published.published_at.unwrap();

        // Archive then re-publish
        archive_post(db.conn(), dir.path(), &post.id).unwrap();

        // Brief delay to ensure now() is different
        let republished = publish_post(db.conn(), dir.path(), &post.id).unwrap();
        assert_eq!(
            republished.published_at.unwrap(),
            original_published_at,
            "published_at should be preserved on re-publish"
        );
    }

    #[test]
    fn publish_preserves_caller_supplied_checksums() {
        let (db, dir) = setup();
        let mut post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Imported",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        post.checksum = Some("bds2-post-checksum".into());
        qp::update_post(db.conn(), &post).unwrap();
        let mut translation = upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "Importiert",
            None,
            Some("Inhalt"),
        )
        .unwrap();
        translation.checksum = Some("bds2-translation-checksum".into());
        qt::update_post_translation(db.conn(), &translation).unwrap();

        let published = publish_post(db.conn(), dir.path(), &post.id).unwrap();
        let published_translation =
            qt::get_post_translation_by_id(db.conn(), &translation.id).unwrap();

        assert_eq!(published.checksum.as_deref(), Some("bds2-post-checksum"));
        assert_eq!(
            published_translation.checksum.as_deref(),
            Some("bds2-translation-checksum")
        );
    }

    #[test]
    fn delete_post_removes_everything() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Delete Me",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();

        // Add a translation
        upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "German Title",
            None,
            Some("German body"),
        )
        .unwrap();

        // Publish to create files
        publish_post(db.conn(), dir.path(), &post.id).unwrap();

        // Verify files exist
        let from_db = qp::get_post_by_id(db.conn(), &post.id).unwrap();
        let post_file = dir.path().join(&from_db.file_path);
        assert!(post_file.exists());

        // Delete
        delete_post(db.conn(), dir.path(), &post.id).unwrap();

        // Post should be gone from DB
        assert!(qp::get_post_by_id(db.conn(), &post.id).is_err());

        // Translations should be gone
        let trans = qt::list_post_translations_by_post(db.conn(), &post.id).unwrap();
        assert!(trans.is_empty());

        // Post file should be gone
        assert!(!post_file.exists());
    }

    #[test]
    fn delete_post_removes_post_from_linked_media_sidecars() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Delete Linked Post",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap();
        let surviving_post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Keep Linked Post",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();

        let insert_media = |id: &str| {
            let mut media = crate::db::queries::media::make_test_media(id, "p1");
            media.file_path = format!("media/{id}.jpg");
            media.sidecar_path = format!("media/{id}.jpg.meta");
            crate::db::queries::media::insert_media(db.conn(), &media).unwrap();
            let sidecar_path = dir.path().join(&media.sidecar_path);
            fs::create_dir_all(sidecar_path.parent().unwrap()).unwrap();
            fs::write(
                &sidecar_path,
                crate::util::sidecar::MediaSidecar::from_media(&media, &[]).to_string(),
            )
            .unwrap();
            (media, sidecar_path)
        };
        let (first_media, first_sidecar_path) = insert_media("media1");
        let (second_media, second_sidecar_path) = insert_media("media2");

        crate::engine::post_media::link_media_to_post(
            db.conn(),
            dir.path(),
            "p1",
            &post.id,
            &first_media.id,
            0,
        )
        .unwrap();
        crate::engine::post_media::link_media_to_post(
            db.conn(),
            dir.path(),
            "p1",
            &surviving_post.id,
            &first_media.id,
            0,
        )
        .unwrap();
        crate::engine::post_media::link_media_to_post(
            db.conn(),
            dir.path(),
            "p1",
            &post.id,
            &second_media.id,
            1,
        )
        .unwrap();

        delete_post(db.conn(), dir.path(), &post.id).unwrap();

        let first_sidecar =
            crate::util::sidecar::read_sidecar(&fs::read_to_string(&first_sidecar_path).unwrap())
                .unwrap();
        assert_eq!(first_sidecar.linked_post_ids, vec![surviving_post.id]);
        let second_sidecar =
            crate::util::sidecar::read_sidecar(&fs::read_to_string(&second_sidecar_path).unwrap())
                .unwrap();
        assert!(second_sidecar.linked_post_ids.is_empty());
    }

    #[test]
    fn upsert_translation_create_and_update() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Parent",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();

        // Create translation
        let t = upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "German Title",
            Some("excerpt"),
            Some("Inhalt"),
        )
        .unwrap();
        assert_eq!(t.language, "de");
        assert_eq!(t.title, "German Title");

        // Update same translation
        let t2 = upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "Neuer Titel",
            None,
            Some("Neuer Inhalt"),
        )
        .unwrap();
        assert_eq!(t2.id, t.id);
        assert_eq!(t2.title, "Neuer Titel");
    }

    #[test]
    fn manual_translation_edit_reopens_translation_and_canonical_drafts() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Parent",
            Some("body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "Titel",
            None,
            Some("Inhalt"),
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap();

        let events = domain_events::subscribe();
        let edited = upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "Neuer Titel",
            None,
            Some("Neuer Inhalt"),
        )
        .unwrap();

        assert_eq!(edited.status, PostStatus::Draft);
        assert_eq!(edited.content.as_deref(), Some("Neuer Inhalt"));
        let reopened = qp::get_post_by_id(db.conn(), &post.id).unwrap();
        assert_eq!(reopened.status, PostStatus::Draft);
        assert_eq!(reopened.content.as_deref(), Some("body"));
        assert!(events.drain().iter().any(|event| matches!(
            event,
            crate::model::DomainEvent::EntityChanged {
                entity: DomainEntity::Post,
                entity_id,
                action: NotificationAction::Updated,
                ..
            } if entity_id == &post.id
        )));
    }

    #[test]
    fn delete_translation_removes_file_and_db() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Parent",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();

        let t = upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "German",
            None,
            Some("Inhalt"),
        )
        .unwrap();

        // Publish to create files
        publish_post(db.conn(), dir.path(), &post.id).unwrap();

        // Translation should have a file now
        let t_from_db = qt::get_post_translation_by_id(db.conn(), &t.id).unwrap();
        assert!(!t_from_db.file_path.is_empty());
        let t_file = dir.path().join(&t_from_db.file_path);
        assert!(t_file.exists());

        // Delete translation
        delete_translation(db.conn(), dir.path(), &t.id).unwrap();

        // Should be gone from DB
        assert!(qt::get_post_translation_by_id(db.conn(), &t.id).is_err());

        // File should be gone
        assert!(!t_file.exists());
    }

    #[test]
    fn rebuild_from_filesystem() {
        let (db, dir) = setup();

        // Create fixture files
        let posts_dir = dir.path().join("posts").join("2024").join("01");
        fs::create_dir_all(&posts_dir).unwrap();

        // Write a canonical post
        let post_content = "---\n\
            id: rebuild-post-1\n\
            title: Rebuilt Post\n\
            slug: rebuilt-post\n\
            status: published\n\
            createdAt: '2024-01-15T12:00:00.000Z'\n\
            updatedAt: '2024-01-15T12:00:00.000Z'\n\
            tags:\n  - test\n\
            categories: []\n\
            publishedAt: '2024-01-15T12:00:00.000Z'\n\
            ---\nHello from rebuild!\n";
        fs::write(posts_dir.join("rebuilt-post.md"), post_content).unwrap();

        // Write a translation
        let trans_content = "---\n\
            translationFor: rebuild-post-1\n\
            language: de\n\
            title: Wiederhergestellter Beitrag\n\
            ---\nHallo vom Rebuild!\n";
        fs::write(posts_dir.join("rebuilt-post.de.md"), trans_content).unwrap();

        // Run rebuild
        let report = rebuild_posts_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(report.posts_created, 1);
        assert_eq!(report.translations_created, 1);
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);

        // Verify post in DB
        let post = qp::get_post_by_id(db.conn(), "rebuild-post-1").unwrap();
        assert_eq!(post.title, "Rebuilt Post");
        assert_eq!(post.slug, "rebuilt-post");
        assert_eq!(post.tags, vec!["test"]);
        assert_eq!(post.updated_at, 1_705_320_000_000);
        assert_eq!(post.checksum, None);

        // Verify translation in DB
        let trans =
            qt::get_post_translation_by_post_and_language(db.conn(), "rebuild-post-1", "de")
                .unwrap();
        assert_eq!(trans.title, "Wiederhergestellter Beitrag");
        assert_eq!(trans.checksum, None);

        // Run rebuild again - should update, not create
        let report2 = rebuild_posts_from_filesystem(db.conn(), dir.path(), "p1").unwrap();
        assert_eq!(report2.posts_created, 0);
        assert_eq!(report2.posts_updated, 1);
        assert_eq!(report2.translations_created, 0);
        assert_eq!(report2.translations_updated, 1);
    }

    #[test]
    fn is_translation_filename_works() {
        assert!(is_translation_filename("hello.de"));
        assert!(is_translation_filename("hello.en"));
        assert!(is_translation_filename("my-post.fr"));
        assert!(!is_translation_filename("hello"));
        // Note: "md" is 2 lowercase letters so is_translation_filename("hello.md") = true.
        // In practice this never arises because file_stem() of "hello.md" is "hello",
        // not "hello.md". Only "hello.md.md" would produce stem "hello.md".
        assert!(!is_translation_filename("hello.123"));
        assert!(!is_translation_filename("hello.D"));
    }

    #[test]
    fn do_not_translate_guard_rejects_translation() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "No Translate",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        // Set do_not_translate
        update_post(
            db.conn(),
            dir.path(),
            &post.id,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(true),
        )
        .unwrap();

        let result = upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "German",
            None,
            Some("Inhalt"),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            EngineError::Validation(_) => {}
            other => panic!("expected Validation, got: {other}"),
        }
    }

    #[test]
    fn update_post_slug_uniqueness_enforced() {
        let (db, dir) = setup();
        create_post(
            db.conn(),
            dir.path(),
            "p1",
            "First",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        let second = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Second",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();

        let result = update_post(
            db.conn(),
            dir.path(),
            &second.id,
            None,
            Some("first"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn update_published_post_transitions_to_draft() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Published",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap();

        // Archive then update to test auto-draft
        archive_post(db.conn(), dir.path(), &post.id).unwrap();
        // Re-publish
        let _ = publish_post(db.conn(), dir.path(), &post.id).unwrap();

        // Now update the published post
        let updated = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            Some("New Title"),
            None,
            None,
            Some("new body"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(updated.status, PostStatus::Draft);
    }

    #[test]
    fn canonical_url_format() {
        // 2005-11-13T12:00:00.000Z = 1131883200000
        let url = canonical_url(1131883200000, "esmeralda");
        assert_eq!(url, "/2005/11/13/esmeralda");
    }

    #[test]
    fn publish_post_already_published_rejected() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Double Pub",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap();

        let result = publish_post(db.conn(), dir.path(), &post.id);
        assert!(result.is_err());
        match result.unwrap_err() {
            EngineError::Conflict(_) => {}
            other => panic!("expected Conflict, got: {other}"),
        }
    }

    #[test]
    fn publish_post_updates_link_graph() {
        let (db, dir) = setup();
        // Create target post first
        let target = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Target Post",
            Some("target body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &target.id).unwrap();

        // Create source post with a link to target
        let target_url = canonical_url(target.created_at, &target.slug);
        let body = format!("Check out [this post]({target_url}) for more.");
        let source = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Source Post",
            Some(&body),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &source.id).unwrap();

        // Verify link was created
        let links = ql::list_links_by_source(db.conn(), &source.id).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target_post_id, target.id);
    }

    #[test]
    fn archive_from_published_status() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "To Archive",
            Some("body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        let published = publish_post(db.conn(), dir.path(), &post.id).unwrap();
        let post_path = dir.path().join(&published.file_path);
        let published_file = fs::read(&post_path).unwrap();
        archive_post(db.conn(), dir.path(), &post.id).unwrap();

        let from_db = qp::get_post_by_id(db.conn(), &post.id).unwrap();
        assert_eq!(from_db.status, PostStatus::Archived);
        assert_eq!(from_db.content, None);
        assert_eq!(fs::read(post_path).unwrap(), published_file);
    }

    #[test]
    fn update_archived_post_keeps_archived_status() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Archived",
            Some("draft body"),
            vec![],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        archive_post(db.conn(), dir.path(), &post.id).unwrap();

        let updated = update_post(
            db.conn(),
            dir.path(),
            &post.id,
            Some("Changed while archived"),
            None,
            None,
            Some("changed body"),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(updated.status, PostStatus::Archived);
        assert_eq!(updated.title, "Changed while archived");
        assert_eq!(updated.content.as_deref(), Some("changed body"));
    }

    #[test]
    fn unarchive_published_post_restores_body_from_file() {
        let (db, dir) = setup();
        let post = create_published_post(&db, &dir, "Published", "file body");
        archive_post(db.conn(), dir.path(), &post.id).unwrap();
        let archived = qp::get_post_by_id(db.conn(), &post.id).unwrap();
        assert_eq!(archived.content, None);

        let unarchived = unarchive_post(db.conn(), dir.path(), &post.id).unwrap();

        assert_eq!(unarchived.status, PostStatus::Draft);
        assert_eq!(unarchived.content.as_deref(), Some("file body"));
        assert!(unarchived.updated_at >= archived.updated_at);
        let from_db = qp::get_post_by_id(db.conn(), &post.id).unwrap();
        assert_eq!(from_db.status, PostStatus::Draft);
        assert_eq!(from_db.content.as_deref(), Some("file body"));
    }

    #[test]
    fn parse_post_links_extracts_canonical_urls() {
        let content = "See [dated](/2024/01/15/hello-world), [localized](/de/2024/02/01/test-post/), [short](/notes), [localized short](/fr/article), and <a class='post' href='/2025/03/04/html-post'><strong>HTML</strong> post</a>.";
        let links = parse_post_links(content);
        assert_eq!(links.len(), 5);
        assert_eq!(links[0].0, "hello-world");
        assert_eq!(links[0].1.as_deref(), Some("dated"));
        assert_eq!(links[1].0, "test-post");
        assert_eq!(links[2].0, "notes");
        assert_eq!(links[3].0, "article");
        assert_eq!(links[4].0, "html-post");
        assert_eq!(links[4].1.as_deref(), Some("HTML post"));
    }
}
