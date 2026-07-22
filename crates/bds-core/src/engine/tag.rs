use std::path::Path;

use crate::db::DbConnection as Connection;
use uuid::Uuid;

use crate::db::queries::post as post_q;
use crate::db::queries::tag as tag_q;
use crate::engine::meta;
use crate::engine::{EngineError, EngineResult, domain_events};
use crate::model::metadata::TagEntry;
use crate::model::{DomainEntity, NotificationAction, Tag};
use crate::util::now_unix_ms;

/// Create a new tag. Case-insensitive duplicate check.
pub fn create_tag(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    name: &str,
    color: Option<&str>,
) -> EngineResult<Tag> {
    // Check for case-insensitive duplicate
    if tag_q::get_tag_by_project_and_name(conn, project_id, name).is_ok() {
        return Err(EngineError::Conflict(format!(
            "tag '{name}' already exists"
        )));
    }

    let now = now_unix_ms();
    let tag = Tag {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        name: name.to_string(),
        color: color.map(|s| s.to_string()),
        post_template_slug: None,
        created_at: now,
        updated_at: now,
    };
    tag_q::insert_tag(conn, &tag)?;
    rewrite_tags_json(conn, data_dir, project_id)?;
    emit_tag(&tag, NotificationAction::Created);
    Ok(tag)
}

/// Update a tag's name, color, and/or post_template_slug.
pub fn update_tag(
    conn: &Connection,
    data_dir: &Path,
    tag_id: &str,
    name: Option<&str>,
    color: Option<&str>,
    post_template_slug: Option<&str>,
) -> EngineResult<()> {
    let mut tag = tag_q::get_tag_by_id(conn, tag_id)
        .map_err(|_| EngineError::NotFound(format!("tag {tag_id}")))?;

    if let Some(n) = name {
        tag.name = n.to_string();
    }
    if let Some(c) = color {
        tag.color = if c.is_empty() {
            None
        } else {
            Some(c.to_string())
        };
    }
    if let Some(pts) = post_template_slug {
        tag.post_template_slug = if pts.is_empty() {
            None
        } else {
            Some(pts.to_string())
        };
    }
    tag.updated_at = now_unix_ms();
    tag_q::update_tag(conn, &tag)?;
    rewrite_tags_json(conn, data_dir, &tag.project_id)?;
    emit_tag(&tag, NotificationAction::Updated);
    Ok(())
}

/// Delete a tag: remove its exact name from posts, delete from DB, rewrite tags.json.
/// Tag entity lookup remains case-insensitive, but portable post tag arrays
/// follow bDS2 and Allium's exact string membership semantics.
pub fn delete_tag(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    tag_id: &str,
) -> EngineResult<()> {
    let tag = tag_q::get_tag_by_id(conn, tag_id)
        .map_err(|_| EngineError::NotFound(format!("tag {tag_id}")))?;

    let modified = remove_tag_name_from_posts(conn, project_id, &tag.name)?;
    tag_q::delete_tag(conn, tag_id)?;
    rewrite_tags_json(conn, data_dir, project_id)?;
    flush_post_frontmatter(conn, data_dir, &modified)?;
    emit_tag(&tag, NotificationAction::Deleted);
    Ok(())
}

/// Rename a tag: replace its exact name in posts, update the DB and tags.json.
/// Tag entity lookup remains case-insensitive, but portable post tag arrays
/// follow bDS2 and Allium's exact string membership semantics.
pub fn rename_tag(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    tag_id: &str,
    new_name: &str,
) -> EngineResult<()> {
    let mut tag = tag_q::get_tag_by_id(conn, tag_id)
        .map_err(|_| EngineError::NotFound(format!("tag {tag_id}")))?;
    let old_name = tag.name.clone();

    // Update all posts: replace old name with new name in tag arrays
    let posts = post_q::list_posts_by_project(conn, project_id)?;
    let now = now_unix_ms();
    let mut modified = Vec::new();
    for mut post in posts {
        if post.tags.iter().any(|t| t == &old_name) {
            post.tags = post
                .tags
                .into_iter()
                .map(|t| {
                    if t == old_name {
                        new_name.to_string()
                    } else {
                        t
                    }
                })
                .collect();
            post.updated_at = now;
            post_q::update_post(conn, &post)?;
            modified.push(post.id.clone());
        }
    }

    tag.name = new_name.to_string();
    tag.updated_at = now;
    tag_q::update_tag(conn, &tag)?;
    rewrite_tags_json(conn, data_dir, project_id)?;
    flush_post_frontmatter(conn, data_dir, &modified)?;
    emit_tag(&tag, NotificationAction::Updated);
    Ok(())
}

/// Merge multiple source tags into one target tag.
/// For each source: update posts (remove source name, add target name if not present), delete source.
/// Source and target membership in portable post tag arrays is exact, matching
/// bDS2 and Allium; tag entity lookup and uniqueness remain case-insensitive.
pub fn merge_tags(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    source_ids: &[&str],
    target_id: &str,
) -> EngineResult<()> {
    let target_tag = tag_q::get_tag_by_id(conn, target_id)
        .map_err(|_| EngineError::NotFound(format!("target tag {target_id}")))?;

    let mut all_modified = Vec::new();
    let mut deleted_tags = Vec::new();
    for &source_id in source_ids {
        let source_tag = tag_q::get_tag_by_id(conn, source_id)
            .map_err(|_| EngineError::NotFound(format!("source tag {source_id}")))?;

        let posts = post_q::list_posts_by_project(conn, project_id)?;
        let now = now_unix_ms();
        for mut post in posts {
            let has_source = post.tags.iter().any(|t| t == &source_tag.name);
            if has_source {
                // Remove source tag name
                post.tags.retain(|t| t != &source_tag.name);
                // Add target tag name if not already present
                if !post.tags.iter().any(|t| t == &target_tag.name) {
                    post.tags.push(target_tag.name.clone());
                }
                post.updated_at = now;
                post_q::update_post(conn, &post)?;
                if !all_modified.contains(&post.id) {
                    all_modified.push(post.id.clone());
                }
            }
        }

        tag_q::delete_tag(conn, source_id)?;
        deleted_tags.push(source_tag);
    }

    rewrite_tags_json(conn, data_dir, project_id)?;
    flush_post_frontmatter(conn, data_dir, &all_modified)?;
    for tag in &deleted_tags {
        emit_tag(tag, NotificationAction::Deleted);
    }
    emit_tag(&target_tag, NotificationAction::Updated);
    Ok(())
}

fn emit_tag(tag: &Tag, action: NotificationAction) {
    domain_events::entity_changed(&tag.project_id, DomainEntity::Tag, &tag.id, action);
}

/// Import tags from meta/tags.json into DB, preserving colors and properties.
/// Creates new tags or updates existing ones with file-based properties.
pub fn import_tags_from_file(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<()> {
    let entries = match meta::read_tags_json(data_dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()), // File doesn't exist or is invalid — nothing to import
    };

    let now = now_unix_ms();
    for entry in &entries {
        let name = entry.name.trim();
        if name.is_empty() {
            continue;
        }
        match tag_q::get_tag_by_project_and_name(conn, project_id, name) {
            Ok(existing) => {
                // Update color/post_template_slug from file if provided
                if entry.color.is_some() || entry.post_template_slug.is_some() {
                    let mut updated = existing;
                    if entry.color.is_some() {
                        updated.color = entry.color.clone();
                    }
                    if entry.post_template_slug.is_some() {
                        updated.post_template_slug = entry.post_template_slug.clone();
                    }
                    updated.updated_at = now;
                    tag_q::update_tag(conn, &updated)?;
                }
            }
            Err(_) => {
                let tag = Tag {
                    id: Uuid::new_v4().to_string(),
                    project_id: project_id.to_string(),
                    name: name.to_string(),
                    color: entry.color.clone(),
                    post_template_slug: entry.post_template_slug.clone(),
                    created_at: now,
                    updated_at: now,
                };
                tag_q::insert_tag(conn, &tag)?;
            }
        }
    }
    Ok(())
}

/// Sync tags from all posts: collect unique tag names, create missing tags in DB.
/// This is additive only — it does NOT rewrite tags.json.
pub fn sync_tags_from_posts(conn: &Connection, project_id: &str) -> EngineResult<Vec<Tag>> {
    let posts = post_q::list_posts_by_project(conn, project_id)?;

    // Collect all unique tag names from posts (preserve original casing per spec).
    // Use a case-insensitive set to avoid duplicates while keeping the first-seen casing.
    let mut seen_lower = std::collections::HashSet::new();
    let mut tag_names = Vec::new();
    for post in &posts {
        for tag_name in &post.tags {
            if seen_lower.insert(tag_name.to_lowercase()) {
                tag_names.push(tag_name.clone());
            }
        }
    }

    // Create any tags that don't exist yet (using original casing)
    let now = now_unix_ms();
    for name in &tag_names {
        if tag_q::get_tag_by_project_and_name(conn, project_id, name).is_err() {
            let tag = Tag {
                id: Uuid::new_v4().to_string(),
                project_id: project_id.to_string(),
                name: name.clone(),
                color: None,
                post_template_slug: None,
                created_at: now,
                updated_at: now,
            };
            tag_q::insert_tag(conn, &tag)?;
        }
    }
    let all_tags = tag_q::list_tags_by_project(conn, project_id)?;
    Ok(all_tags)
}

/// Discover tags from posts and rewrite tags.json to match the resulting DB state.
pub fn discover_tags(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<Vec<Tag>> {
    let tags = sync_tags_from_posts(conn, project_id)?;
    rewrite_tags_json(conn, data_dir, project_id)?;
    Ok(tags)
}

/// Rewrite meta/tags.json from DB state.
pub fn rewrite_tags_json(conn: &Connection, data_dir: &Path, project_id: &str) -> EngineResult<()> {
    let tags = tag_q::list_tags_by_project(conn, project_id)?;
    let entries: Vec<TagEntry> = tags
        .into_iter()
        .map(|t| TagEntry {
            name: t.name,
            color: t.color,
            post_template_slug: t.post_template_slug,
        })
        .collect();
    meta::write_tags_json(data_dir, &entries)?;
    Ok(())
}

// ── helpers ─────────────────────────────────────────────────────────

/// Rewrite frontmatter files for posts whose tags were modified.
/// Only rewrites published posts (that have file_path set).
fn flush_post_frontmatter(
    conn: &Connection,
    data_dir: &Path,
    post_ids: &[String],
) -> EngineResult<()> {
    use crate::util::atomic_write_str;
    use crate::util::frontmatter::write_post_file;

    for post_id in post_ids {
        let post = post_q::get_post_by_id(conn, post_id)?;
        if !post.file_path.is_empty() {
            let abs_path = data_dir.join(&post.file_path);
            if abs_path.exists() {
                // Read existing body from file
                let content = std::fs::read_to_string(&abs_path)?;
                let (_fm, body) = crate::util::frontmatter::read_post_file(&content)
                    .map_err(EngineError::Parse)?;
                // Rewrite with updated frontmatter
                let file_content = write_post_file(&post, &body);
                atomic_write_str(&abs_path, &file_content)?;
            }
        }
    }
    Ok(())
}

fn remove_tag_name_from_posts(
    conn: &Connection,
    project_id: &str,
    tag_name: &str,
) -> EngineResult<Vec<String>> {
    let posts = post_q::list_posts_by_project(conn, project_id)?;
    let now = now_unix_ms();
    let mut modified = Vec::new();
    for mut post in posts {
        if post.tags.iter().any(|t| t == tag_name) {
            post.tags.retain(|t| t != tag_name);
            post.updated_at = now;
            post_q::update_post(conn, &post)?;
            modified.push(post.id.clone());
        }
    }
    Ok(modified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::post::insert_post;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::model::{Post, PostStatus};
    use tempfile::TempDir;

    fn setup() -> (Database, TempDir) {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("meta")).unwrap();
        // Seed tags.json
        std::fs::write(dir.path().join("meta/tags.json"), "[]").unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        (db, dir)
    }

    fn make_post(id: &str, slug: &str, tags: Vec<String>) -> Post {
        Post {
            id: id.to_string(),
            project_id: "p1".to_string(),
            title: format!("Post {id}"),
            slug: slug.to_string(),
            excerpt: None,
            content: Some("body".into()),
            status: PostStatus::Draft,
            author: None,
            language: None,
            do_not_translate: false,
            template_slug: None,
            file_path: format!("posts/{slug}.md"),
            checksum: None,
            tags,
            categories: vec![],
            published_title: None,
            published_content: None,
            published_tags: None,
            published_categories: None,
            published_excerpt: None,
            created_at: 1000,
            updated_at: 2000,
            published_at: None,
        }
    }

    #[test]
    fn create_tag_and_rewrite_json() {
        let (db, dir) = setup();
        let tag = create_tag(db.conn(), dir.path(), "p1", "rust", Some("#ff0000")).unwrap();
        assert_eq!(tag.name, "rust");
        assert_eq!(tag.color.as_deref(), Some("#ff0000"));

        // Verify tags.json was written
        let entries = meta::read_tags_json(dir.path()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "rust");
    }

    #[test]
    fn create_tag_duplicate_rejected() {
        let (db, dir) = setup();
        create_tag(db.conn(), dir.path(), "p1", "rust", None).unwrap();
        let result = create_tag(db.conn(), dir.path(), "p1", "Rust", None);
        assert!(result.is_err());
    }

    #[test]
    fn update_tag_fields() {
        let (db, dir) = setup();
        let tag = create_tag(db.conn(), dir.path(), "p1", "rust", None).unwrap();
        update_tag(
            db.conn(),
            dir.path(),
            &tag.id,
            Some("go"),
            Some("#00ff00"),
            None,
        )
        .unwrap();

        let entries = meta::read_tags_json(dir.path()).unwrap();
        assert_eq!(entries[0].name, "go");
        assert_eq!(entries[0].color.as_deref(), Some("#00ff00"));
    }

    #[test]
    fn delete_tag_removes_from_posts() {
        let (db, dir) = setup();
        let tag = create_tag(db.conn(), dir.path(), "p1", "rust", None).unwrap();
        insert_post(
            db.conn(),
            &make_post("x1", "hello", vec!["rust".into(), "web".into()]),
        )
        .unwrap();

        delete_tag(db.conn(), dir.path(), "p1", &tag.id).unwrap();

        let post = crate::db::queries::post::get_post_by_id(db.conn(), "x1").unwrap();
        assert!(!post.tags.contains(&"rust".to_string()));
        assert!(post.tags.contains(&"web".to_string()));

        let entries = meta::read_tags_json(dir.path()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn rename_tag_updates_posts() {
        let (db, dir) = setup();
        let tag = create_tag(db.conn(), dir.path(), "p1", "rust", None).unwrap();
        insert_post(db.conn(), &make_post("x1", "hello", vec!["rust".into()])).unwrap();

        rename_tag(db.conn(), dir.path(), "p1", &tag.id, "golang").unwrap();

        let post = crate::db::queries::post::get_post_by_id(db.conn(), "x1").unwrap();
        assert!(post.tags.contains(&"golang".to_string()));
        assert!(!post.tags.contains(&"rust".to_string()));

        let entries = meta::read_tags_json(dir.path()).unwrap();
        assert_eq!(entries[0].name, "golang");
    }

    #[test]
    fn merge_tags_combines_into_target() {
        let (db, dir) = setup();
        let t1 = create_tag(db.conn(), dir.path(), "p1", "rs", None).unwrap();
        let t2 = create_tag(db.conn(), dir.path(), "p1", "rust", None).unwrap();
        let t3 = create_tag(db.conn(), dir.path(), "p1", "target", None).unwrap();

        insert_post(db.conn(), &make_post("x1", "a", vec!["rs".into()])).unwrap();
        insert_post(
            db.conn(),
            &make_post("x2", "b", vec!["rust".into(), "target".into()]),
        )
        .unwrap();

        merge_tags(db.conn(), dir.path(), "p1", &[&t1.id, &t2.id], &t3.id).unwrap();

        // Post x1 should now have "target"
        let p1 = crate::db::queries::post::get_post_by_id(db.conn(), "x1").unwrap();
        assert!(p1.tags.contains(&"target".to_string()));
        assert!(!p1.tags.contains(&"rs".to_string()));

        // Post x2 should still have "target" only once
        let p2 = crate::db::queries::post::get_post_by_id(db.conn(), "x2").unwrap();
        assert_eq!(p2.tags.iter().filter(|t| *t == "target").count(), 1);

        // Source tags deleted from DB
        let all = crate::db::queries::tag::list_tags_by_project(db.conn(), "p1").unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "target");
    }

    #[test]
    fn tag_mutations_match_post_tag_names_exactly() {
        let (db, dir) = setup();
        let renamed = create_tag(db.conn(), dir.path(), "p1", "rust", None).unwrap();
        insert_post(
            db.conn(),
            &make_post("rename", "rename", vec!["Rust".into(), "rust".into()]),
        )
        .unwrap();

        rename_tag(db.conn(), dir.path(), "p1", &renamed.id, "golang").unwrap();

        assert_eq!(
            post_q::get_post_by_id(db.conn(), "rename").unwrap().tags,
            vec!["Rust", "golang"]
        );

        let source = create_tag(db.conn(), dir.path(), "p1", "source", None).unwrap();
        let target = create_tag(db.conn(), dir.path(), "p1", "target", None).unwrap();
        insert_post(
            db.conn(),
            &make_post(
                "merge",
                "merge",
                vec!["SOURCE".into(), "source".into(), "Target".into()],
            ),
        )
        .unwrap();

        merge_tags(db.conn(), dir.path(), "p1", &[&source.id], &target.id).unwrap();

        assert_eq!(
            post_q::get_post_by_id(db.conn(), "merge").unwrap().tags,
            vec!["SOURCE", "Target", "target"]
        );

        let deleted = create_tag(db.conn(), dir.path(), "p1", "delete", None).unwrap();
        insert_post(
            db.conn(),
            &make_post("delete", "delete", vec!["DELETE".into(), "delete".into()]),
        )
        .unwrap();

        delete_tag(db.conn(), dir.path(), "p1", &deleted.id).unwrap();

        assert_eq!(
            post_q::get_post_by_id(db.conn(), "delete").unwrap().tags,
            vec!["DELETE"]
        );
    }

    #[test]
    fn sync_tags_from_posts_creates_missing() {
        let (db, _dir) = setup();
        insert_post(
            db.conn(),
            &make_post("x1", "a", vec!["rust".into(), "web".into()]),
        )
        .unwrap();
        insert_post(
            db.conn(),
            &make_post("x2", "b", vec!["web".into(), "go".into()]),
        )
        .unwrap();

        let tags = sync_tags_from_posts(db.conn(), "p1").unwrap();
        assert_eq!(tags.len(), 3);
        let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"go"));
        assert!(names.contains(&"rust"));
        assert!(names.contains(&"web"));
    }

    #[test]
    fn discover_tags_rewrites_tags_json() {
        let (db, dir) = setup();
        insert_post(
            db.conn(),
            &make_post("x1", "a", vec!["rust".into(), "web".into()]),
        )
        .unwrap();

        let tags = discover_tags(db.conn(), dir.path(), "p1").unwrap();
        assert_eq!(tags.len(), 2);

        let entries = meta::read_tags_json(dir.path()).unwrap();
        let names = entries
            .into_iter()
            .map(|entry| entry.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["rust".to_string(), "web".to_string()]);
    }

    #[test]
    fn import_tags_from_file_preserves_colors() {
        let (db, dir) = setup();
        // Write tags.json with colors
        let entries = vec![
            TagEntry {
                name: "rust".into(),
                color: Some("#ff0000".into()),
                post_template_slug: None,
            },
            TagEntry {
                name: "web".into(),
                color: None,
                post_template_slug: Some("blog".into()),
            },
        ];
        meta::write_tags_json(dir.path(), &entries).unwrap();

        import_tags_from_file(db.conn(), dir.path(), "p1").unwrap();

        let tags = tag_q::list_tags_by_project(db.conn(), "p1").unwrap();
        assert_eq!(tags.len(), 2);
        let rust_tag = tags.iter().find(|t| t.name == "rust").unwrap();
        assert_eq!(rust_tag.color.as_deref(), Some("#ff0000"));
        let web_tag = tags.iter().find(|t| t.name == "web").unwrap();
        assert_eq!(web_tag.post_template_slug.as_deref(), Some("blog"));
    }

    #[test]
    fn rewrite_tags_json_matches_db() {
        let (db, dir) = setup();
        create_tag(db.conn(), dir.path(), "p1", "zebra", None).unwrap();
        create_tag(db.conn(), dir.path(), "p1", "alpha", None).unwrap();

        let entries = meta::read_tags_json(dir.path()).unwrap();
        assert_eq!(entries[0].name, "alpha");
        assert_eq!(entries[1].name, "zebra");
    }

    #[test]
    fn rename_tag_flushes_post_frontmatter() {
        let (db, dir) = setup();
        // Create and publish a post with a tag
        use crate::db::fts::ensure_fts_tables;
        ensure_fts_tables(db.conn()).unwrap();
        let post = crate::engine::post::create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Tagged Post",
            Some("body content"),
            vec!["rust".into()],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();
        crate::engine::post::publish_post(db.conn(), dir.path(), &post.id).unwrap();

        let tag = create_tag(db.conn(), dir.path(), "p1", "rust", None).unwrap();
        rename_tag(db.conn(), dir.path(), "p1", &tag.id, "golang").unwrap();

        // Read the file from disk and verify tag was updated
        let from_db = crate::db::queries::post::get_post_by_id(db.conn(), &post.id).unwrap();
        let file_content = std::fs::read_to_string(dir.path().join(&from_db.file_path)).unwrap();
        assert!(
            file_content.contains("golang"),
            "frontmatter should contain renamed tag"
        );
        assert!(
            !file_content.contains("rust"),
            "frontmatter should not contain old tag name"
        );
    }
}
