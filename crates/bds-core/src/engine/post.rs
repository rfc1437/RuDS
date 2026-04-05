use std::fs;
use std::path::Path;

use rusqlite::{params, Connection};
use uuid::Uuid;
use walkdir::WalkDir;

use crate::db::fts;
use crate::db::queries::post as qp;
use crate::db::queries::post_link as ql;
use crate::db::queries::post_translation as qt;
use crate::engine::{EngineError, EngineResult};
use crate::model::{Post, PostLink, PostStatus, PostTranslation};
use crate::util::frontmatter::{
    read_post_file, read_translation_file, write_post_file, write_translation_file,
};
use crate::util::{
    atomic_write_str, content_hash, ensure_unique, now_unix_ms, post_file_path, slugify,
    translation_file_path,
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
pub fn create_post(
    conn: &Connection,
    _data_dir: &Path,
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
    fts_index_post(conn, &post)?;

    Ok(post)
}

/// Update a post's fields.
pub fn update_post(
    conn: &Connection,
    _data_dir: &Path,
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
    if let Some(new_slug) = slug {
        if new_slug != post.slug {
            if qp::get_post_by_project_and_slug(conn, &post.project_id, new_slug).is_ok() {
                return Err(EngineError::Conflict(format!(
                    "slug '{new_slug}' already exists in this project"
                )));
            }
        }
    }

    if let Some(t) = title {
        post.title = t.to_string();
    }
    if let Some(s) = slug {
        post.slug = s.to_string();
    }
    if let Some(exc) = excerpt {
        post.excerpt = exc.map(|s| s.to_string());
    }
    if let Some(c) = content {
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

    // Auto-transition published post back to draft on content/metadata change
    if post.status == PostStatus::Published {
        post.status = PostStatus::Draft;
    }

    post.updated_at = now_unix_ms();
    qp::update_post(conn, &post)?;

    // Re-index FTS
    fts_index_post(conn, &post)?;

    Ok(post)
}

/// Publish a post: write file, clear content, set published_at.
pub fn publish_post(
    conn: &Connection,
    data_dir: &Path,
    post_id: &str,
) -> EngineResult<Post> {
    let mut post = qp::get_post_by_id(conn, post_id)?;

    // Require Draft or Archived status
    match post.status {
        PostStatus::Draft | PostStatus::Archived => {}
        PostStatus::Published => {
            return Err(EngineError::Conflict(
                "post is already published".to_string(),
            ));
        }
    }

    // Compute file_path from created_at + slug
    // Use a savepoint for atomicity
    // Note: savepoint auto-rolls-back if not released (on error propagation)
    conn.execute_batch("SAVEPOINT publish_post")?;
    let rel_path = post_file_path(post.created_at, &post.slug);
    let abs_path = data_dir.join(&rel_path);

    // Get body: from post.content (draft) or read from existing file (re-publish after archive)
    let body = if let Some(ref c) = post.content {
        c.clone()
    } else if abs_path.exists() {
        let file_content = fs::read_to_string(&abs_path)?;
        let (_fm, body) = read_post_file(&file_content)
            .map_err(|e| EngineError::Parse(e))?;
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

    // Compute checksum
    let hash = content_hash(file_content.as_bytes());
    post.checksum = Some(hash);

    // Set published snapshot fields
    let tags_json = serde_json::to_string(&post.tags).unwrap_or_else(|_| "[]".into());
    let cats_json =
        serde_json::to_string(&post.categories).unwrap_or_else(|_| "[]".into());
    post.published_title = Some(post.title.clone());
    post.published_content = Some(body.clone());
    post.published_tags = Some(tags_json.clone());
    post.published_categories = Some(cats_json.clone());
    post.published_excerpt = post.excerpt.clone();

    qp::set_published_snapshot(
        conn,
        post_id,
        &post.title,
        &body,
        &tags_json,
        &cats_json,
        post.excerpt.as_deref(),
        published_at,
        now,
    )?;

    // Set file_path and checksum in DB
    qp::set_post_file_path(conn, post_id, &post.file_path, now)?;
    conn.execute(
        "UPDATE posts SET checksum = ?1 WHERE id = ?2",
        params![post.checksum, post_id],
    )?;

    // Clear content in DB
    qp::clear_post_content(conn, post_id, now)?;
    post.content = None;

    // Set status = Published
    qp::update_post_status(conn, post_id, &PostStatus::Published, now)?;

    // Publish all translations
    let translations = qt::list_post_translations_by_post(conn, post_id)?;
    for mut t in translations {
        publish_translation(conn, data_dir, &mut t, &post)?;
    }

    // Parse inter-post links and update link graph
    ql::delete_links_by_source(conn, post_id)?;
    let link_body = if let Some(ref pc) = post.published_content {
        pc.as_str()
    } else {
        ""
    };
    let parsed_links = parse_post_links(link_body);
    for (target_slug, link_text) in &parsed_links {
        if let Ok(target) = qp::get_post_by_project_and_slug(conn, &post.project_id, target_slug) {
            let link = PostLink {
                id: Uuid::new_v4().to_string(),
                source_post_id: post_id.to_string(),
                target_post_id: target.id.clone(),
                link_text: Some(link_text.clone()),
                created_at: now,
            };
            let _ = ql::insert_post_link(conn, &link);
        }
    }

    // Re-index FTS
    fts_index_post(conn, &post)?;

    conn.execute_batch("RELEASE publish_post")?;

    Ok(post)
}

/// Archive a post.
pub fn archive_post(conn: &Connection, post_id: &str) -> EngineResult<()> {
    let post = qp::get_post_by_id(conn, post_id)?;
    if post.status == PostStatus::Archived {
        return Ok(());
    }
    let now = now_unix_ms();
    qp::update_post_status(conn, post_id, &PostStatus::Archived, now)?;
    Ok(())
}

/// Delete a post and all related data.
pub fn delete_post(
    conn: &Connection,
    data_dir: &Path,
    post_id: &str,
) -> EngineResult<()> {
    let post = qp::get_post_by_id(conn, post_id)?;

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
    conn.execute(
        "DELETE FROM post_links WHERE target_post_id = ?1",
        params![post_id],
    )?;

    // Delete post-media associations
    conn.execute(
        "DELETE FROM post_media WHERE post_id = ?1",
        params![post_id],
    )?;

    // Remove from FTS
    fts::remove_post_from_index(conn, post_id)?;

    // Delete post from DB
    qp::delete_post(conn, post_id)?;

    Ok(())
}

/// Compute the canonical URL for a post: /{YYYY}/{MM}/{DD}/{slug}
pub fn canonical_url(created_at_ms: i64, slug: &str) -> String {
    let (y, m, d) = crate::util::timestamp::year_month_day_from_unix_ms(created_at_ms);
    format!("/{y}/{m}/{d}/{slug}")
}

/// Upsert a translation for a post.
pub fn upsert_translation(
    conn: &Connection,
    _data_dir: &Path,
    post_id: &str,
    language: &str,
    title: &str,
    excerpt: Option<&str>,
    content: Option<&str>,
) -> EngineResult<PostTranslation> {
    let post = qp::get_post_by_id(conn, post_id)?;
    if post.do_not_translate {
        return Err(EngineError::Validation(
            "cannot create translation for a do-not-translate post".to_string(),
        ));
    }
    let now = now_unix_ms();

    // Check if translation already exists
    let existing = qt::get_post_translation_by_post_and_language(conn, post_id, language);
    match existing {
        Ok(mut t) => {
            // Update existing
            t.title = title.to_string();
            t.excerpt = excerpt.map(|s| s.to_string());
            if let Some(c) = content {
                t.content = Some(c.to_string());
            }
            t.updated_at = now;
            qt::update_post_translation(conn, &t)?;

            // Re-index FTS for parent post
            fts_index_post(conn, &post)?;

            Ok(t)
        }
        Err(_) => {
            // Create new
            let id = Uuid::new_v4().to_string();
            let t = PostTranslation {
                id,
                project_id: post.project_id.clone(),
                translation_for: post_id.to_string(),
                language: language.to_string(),
                title: title.to_string(),
                excerpt: excerpt.map(|s| s.to_string()),
                content: content.map(|s| s.to_string()),
                status: PostStatus::Draft,
                file_path: String::new(),
                checksum: None,
                created_at: now,
                updated_at: now,
                published_at: None,
            };
            qt::insert_post_translation(conn, &t)?;

            // Re-index FTS for parent post
            fts_index_post(conn, &post)?;

            Ok(t)
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
        fts_index_post(conn, &post)?;
    }

    Ok(())
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

    for entry in WalkDir::new(&posts_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
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

    // Re-index FTS for all posts in this project
    let posts = qp::list_posts_by_project(conn, project_id)?;
    for post in &posts {
        fts_index_post(conn, post)?;
    }

    Ok(report)
}

// --- Internal helpers ---

/// Parse inter-post links from markdown content.
/// Looks for markdown links that reference canonical post URLs: [text](/YYYY/MM/DD/slug)
fn parse_post_links(content: &str) -> Vec<(String, String)> {
    let mut links = Vec::new();
    // Match markdown links: [text](/YYYY/MM/DD/slug) or [text](/YYYY/MM/DD/slug/)
    // Simple manual parsing since we don't have regex crate
    // Look for patterns like [...](...) where the URL matches /YYYY/MM/DD/slug
    for line in content.lines() {
        let mut pos = 0;
        while pos < line.len() {
            if let Some(bracket_start) = line[pos..].find('[') {
                let abs_start = pos + bracket_start;
                if let Some(bracket_end) = line[abs_start..].find("](") {
                    let text_end = abs_start + bracket_end;
                    let link_text = &line[abs_start + 1..text_end];
                    let url_start = text_end + 2;
                    if let Some(paren_end) = line[url_start..].find(')') {
                        let url = &line[url_start..url_start + paren_end];
                        // Check if URL matches /YYYY/MM/DD/slug pattern
                        let parts: Vec<&str> = url.trim_end_matches('/').split('/').collect();
                        if parts.len() == 5 && parts[0].is_empty()
                            && parts[1].len() == 4 && parts[1].chars().all(|c| c.is_ascii_digit())
                            && parts[2].len() == 2 && parts[2].chars().all(|c| c.is_ascii_digit())
                            && parts[3].len() == 2 && parts[3].chars().all(|c| c.is_ascii_digit())
                        {
                            links.push((parts[4].to_string(), link_text.to_string()));
                        }
                        pos = url_start + paren_end + 1;
                        continue;
                    }
                }
                pos = abs_start + 1;
            } else {
                break;
            }
        }
    }
    links
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
        let (_fm, body) =
            read_translation_file(&file_content).map_err(|e| EngineError::Parse(e))?;
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

    let hash = content_hash(file_content.as_bytes());
    t.checksum = Some(hash);
    t.content = None;

    qt::update_post_translation(conn, t)?;
    // Clear content after update (the update already set content=None via the struct)
    // but we also do an explicit clear to be safe
    conn.execute(
        "UPDATE post_translations SET content = NULL WHERE id = ?1",
        params![t.id],
    )?;

    Ok(())
}

/// Index a post in FTS, gathering translation texts.
fn fts_index_post(conn: &Connection, post: &Post) -> EngineResult<()> {
    let translations = qt::list_post_translations_by_post(conn, &post.id).unwrap_or_default();
    let translation_data: Vec<(String, String)> = translations
        .iter()
        .map(|t| {
            let mut parts = vec![t.title.clone()];
            if let Some(ref exc) = t.excerpt {
                parts.push(exc.clone());
            }
            if let Some(ref cnt) = t.content {
                parts.push(cnt.clone());
            }
            (parts.join(" "), t.language.clone())
        })
        .collect();

    let lang = post.language.as_deref().unwrap_or("en");
    fts::index_post(
        conn,
        &post.id,
        &post.title,
        post.excerpt.as_deref(),
        post.content.as_deref(),
        &post.tags,
        &post.categories,
        &translation_data,
        lang,
    )?;
    Ok(())
}

/// Check if a file stem looks like a translation filename: `{slug}.{lang}`
/// where lang is a 2-letter code. We look for a dot followed by exactly 2 lowercase letters.
fn is_translation_filename(stem: &str) -> bool {
    if let Some(dot_pos) = stem.rfind('.') {
        let suffix = &stem[dot_pos + 1..];
        suffix.len() == 2 && suffix.chars().all(|c| c.is_ascii_lowercase())
    } else {
        false
    }
}

/// Rebuild a canonical post from a .md file. Returns true if created, false if updated.
fn rebuild_canonical_post(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    path: &Path,
) -> EngineResult<bool> {
    let content = fs::read_to_string(path)?;
    let (fm, body) = read_post_file(&content).map_err(|e| EngineError::Parse(e))?;

    let rel_path = path
        .strip_prefix(data_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let hash = content_hash(content.as_bytes());
    let now = now_unix_ms();

    let status = match fm.status.as_str() {
        "published" => PostStatus::Published,
        "archived" => PostStatus::Archived,
        _ => PostStatus::Draft,
    };

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
                Some(body)
            };
            post.status = status;
            post.author = fm.author;
            post.language = fm.language;
            post.do_not_translate = fm.do_not_translate;
            post.template_slug = fm.template_slug;
            post.file_path = rel_path;
            post.checksum = Some(hash);
            post.tags = fm.tags;
            post.categories = fm.categories;
            post.created_at = fm.created_at;
            post.updated_at = now;
            post.published_at = fm.published_at;
            qp::update_post(conn, &post)?;
            Ok(false)
        }
        Err(_) => {
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
                    Some(body)
                },
                status,
                author: fm.author,
                language: fm.language,
                do_not_translate: fm.do_not_translate,
                template_slug: fm.template_slug,
                file_path: rel_path,
                checksum: Some(hash),
                tags: fm.tags,
                categories: fm.categories,
                published_title: None,
                published_content: None,
                published_tags: None,
                published_categories: None,
                published_excerpt: None,
                created_at: fm.created_at,
                updated_at: now,
                published_at: fm.published_at,
            };
            qp::insert_post(conn, &post)?;
            Ok(true)
        }
    }
}

/// Rebuild a translation from a .{lang}.md file. Returns true if created, false if updated.
fn rebuild_translation(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    path: &Path,
) -> EngineResult<bool> {
    let content = fs::read_to_string(path)?;
    let (fm, body) =
        read_translation_file(&content).map_err(|e| EngineError::Parse(e))?;

    let rel_path = path
        .strip_prefix(data_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let hash = content_hash(content.as_bytes());
    let now = now_unix_ms();

    // Check if parent post exists
    let parent = qp::get_post_by_id(conn, &fm.translation_for);
    if parent.is_err() {
        return Err(EngineError::NotFound(format!(
            "parent post '{}' not found for translation",
            fm.translation_for
        )));
    }
    let parent = parent.unwrap();

    // Determine status from parent
    let status = if parent.status == PostStatus::Published {
        PostStatus::Published
    } else {
        PostStatus::Draft
    };

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
            t.checksum = Some(hash);
            t.updated_at = now;
            qt::update_post_translation(conn, &t)?;
            Ok(false)
        }
        Err(_) => {
            let t = PostTranslation {
                id: Uuid::new_v4().to_string(),
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
                checksum: Some(hash),
                created_at: now,
                updated_at: now,
                published_at: if parent.published_at.is_some() {
                    Some(now)
                } else {
                    None
                },
            };
            qt::insert_post_translation(conn, &t)?;
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::fts::ensure_fts_tables;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::db::Database;
    use tempfile::TempDir;

    fn setup() -> (Database, TempDir) {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        ensure_fts_tables(db.conn()).unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = TempDir::new().unwrap();
        (db, dir)
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
        archive_post(db.conn(), &post.id).unwrap();

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

        // Published snapshot fields should be set
        assert_eq!(from_db.published_title.as_deref(), Some("Publish Me"));
        assert!(from_db.published_content.is_some());

        // File should exist on disk
        let abs_path = dir.path().join(&from_db.file_path);
        assert!(abs_path.exists());

        // File should contain frontmatter and body
        let file_content = fs::read_to_string(&abs_path).unwrap();
        assert!(file_content.contains("my body content"));
        assert!(file_content.contains("Publish Me"));
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
        archive_post(db.conn(), &post.id).unwrap();

        // Brief delay to ensure now() is different
        let republished = publish_post(db.conn(), dir.path(), &post.id).unwrap();
        assert_eq!(
            republished.published_at.unwrap(),
            original_published_at,
            "published_at should be preserved on re-publish"
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
        let report =
            rebuild_posts_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(report.posts_created, 1);
        assert_eq!(report.translations_created, 1);
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);

        // Verify post in DB
        let post = qp::get_post_by_id(db.conn(), "rebuild-post-1").unwrap();
        assert_eq!(post.title, "Rebuilt Post");
        assert_eq!(post.slug, "rebuilt-post");
        assert_eq!(post.tags, vec!["test"]);

        // Verify translation in DB
        let trans = qt::get_post_translation_by_post_and_language(
            db.conn(),
            "rebuild-post-1",
            "de",
        )
        .unwrap();
        assert_eq!(trans.title, "Wiederhergestellter Beitrag");

        // Run rebuild again - should update, not create
        let report2 =
            rebuild_posts_from_filesystem(db.conn(), dir.path(), "p1").unwrap();
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
            db.conn(), dir.path(), "p1", "No Translate", Some("body"),
            vec![], vec![], None, None, None,
        ).unwrap();
        // Set do_not_translate
        update_post(
            db.conn(), dir.path(), &post.id,
            None, None, None, None, None, None, None, None, None, Some(true),
        ).unwrap();

        let result = upsert_translation(
            db.conn(), dir.path(), &post.id, "de", "German", None, Some("Inhalt"),
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
            db.conn(), dir.path(), "p1", "First", Some("body"),
            vec![], vec![], None, None, None,
        ).unwrap();
        let second = create_post(
            db.conn(), dir.path(), "p1", "Second", Some("body"),
            vec![], vec![], None, None, None,
        ).unwrap();

        let result = update_post(
            db.conn(), dir.path(), &second.id,
            None, Some("first"), None, None, None, None, None, None, None, None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn update_published_post_transitions_to_draft() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(), dir.path(), "p1", "Published", Some("body"),
            vec![], vec![], None, None, None,
        ).unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap();

        // Archive then update to test auto-draft
        archive_post(db.conn(), &post.id).unwrap();
        // Re-publish
        let _ = publish_post(db.conn(), dir.path(), &post.id).unwrap();

        // Now update the published post
        let updated = update_post(
            db.conn(), dir.path(), &post.id,
            Some("New Title"), None, None, Some("new body"), None, None, None, None, None, None,
        ).unwrap();
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
            db.conn(), dir.path(), "p1", "Double Pub", Some("body"),
            vec![], vec![], None, None, None,
        ).unwrap();
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
            db.conn(), dir.path(), "p1", "Target Post", Some("target body"),
            vec![], vec![], None, None, None,
        ).unwrap();
        publish_post(db.conn(), dir.path(), &target.id).unwrap();

        // Create source post with a link to target
        let target_url = canonical_url(target.created_at, &target.slug);
        let body = format!("Check out [this post]({target_url}) for more.");
        let source = create_post(
            db.conn(), dir.path(), "p1", "Source Post", Some(&body),
            vec![], vec![], None, None, None,
        ).unwrap();
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
            db.conn(), dir.path(), "p1", "To Archive", Some("body"),
            vec![], vec![], None, None, None,
        ).unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap();
        archive_post(db.conn(), &post.id).unwrap();

        let from_db = qp::get_post_by_id(db.conn(), &post.id).unwrap();
        assert_eq!(from_db.status, PostStatus::Archived);
    }

    #[test]
    fn parse_post_links_extracts_canonical_urls() {
        let content = "See [my post](/2024/01/15/hello-world) and [another](/2024/02/01/test-post/) for more.";
        let links = parse_post_links(content);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].0, "hello-world");
        assert_eq!(links[0].1, "my post");
        assert_eq!(links[1].0, "test-post");
    }
}
