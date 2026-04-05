use std::fs;
use std::path::Path;

use rusqlite::Connection;
use uuid::Uuid;

use crate::db::queries::template as qt;
use crate::engine::{EngineError, EngineResult};
use crate::model::{Template, TemplateKind, TemplateStatus};
use crate::util::frontmatter::{write_template_file, TemplateFrontmatter};
use crate::util::{atomic_write_str, ensure_unique, now_unix_ms, slugify};

/// Create a new draft template. Content stored in DB, no file written.
pub fn create_template(
    conn: &Connection,
    project_id: &str,
    title: &str,
    kind: TemplateKind,
    content: &str,
) -> EngineResult<Template> {
    let slug_source = if title.is_empty() { "untitled" } else { title };
    let base_slug = slugify(slug_source);
    let base_slug = if base_slug.is_empty() {
        "untitled".to_string()
    } else {
        base_slug
    };
    let slug = ensure_unique(&base_slug, |candidate| {
        qt::get_template_by_slug(conn, project_id, candidate).is_ok()
    });

    let now = now_unix_ms();
    let id = Uuid::new_v4().to_string();
    let tpl = Template {
        id,
        project_id: project_id.to_string(),
        slug,
        title: title.to_string(),
        kind,
        enabled: true,
        version: 1,
        file_path: String::new(),
        status: TemplateStatus::Draft,
        content: Some(content.to_string()),
        created_at: now,
        updated_at: now,
    };

    qt::insert_template(conn, &tpl)?;
    Ok(tpl)
}

/// Update a template's metadata and/or content. Bumps version.
pub fn update_template(
    conn: &Connection,
    template_id: &str,
    project_id: &str,
    title: Option<&str>,
    slug: Option<&str>,
    kind: Option<TemplateKind>,
    enabled: Option<bool>,
    content: Option<&str>,
) -> EngineResult<Template> {
    let mut tpl = qt::get_template_by_id(conn, template_id)?;

    // Slug uniqueness check
    if let Some(new_slug) = slug {
        if new_slug != tpl.slug {
            if qt::get_template_by_slug(conn, project_id, new_slug).is_ok() {
                return Err(EngineError::Conflict(format!(
                    "template slug '{new_slug}' already exists"
                )));
            }
        }
    }

    if let Some(t) = title {
        tpl.title = t.to_string();
    }
    if let Some(s) = slug {
        tpl.slug = s.to_string();
    }
    if let Some(k) = kind {
        tpl.kind = k;
    }
    if let Some(e) = enabled {
        tpl.enabled = e;
    }
    if let Some(c) = content {
        tpl.content = Some(c.to_string());
    }

    // If published, transition back to draft on edit
    if tpl.status == TemplateStatus::Published {
        // Reload content from file if needed
        if tpl.content.is_none() && !tpl.file_path.is_empty() {
            // Content will come from the file when we read for publish
            // For update we need the new content from the caller
        }
        tpl.status = TemplateStatus::Draft;
    }

    tpl.version += 1;
    tpl.updated_at = now_unix_ms();
    qt::update_template(conn, &tpl)?;
    Ok(tpl)
}

/// Save template content (editor save). Updates DB, bumps version.
pub fn save_template(
    conn: &Connection,
    template_id: &str,
    content: &str,
) -> EngineResult<Template> {
    let mut tpl = qt::get_template_by_id(conn, template_id)?;
    tpl.content = Some(content.to_string());
    tpl.version += 1;
    tpl.updated_at = now_unix_ms();

    // If published, transition back to draft
    if tpl.status == TemplateStatus::Published {
        tpl.status = TemplateStatus::Draft;
    }

    qt::update_template(conn, &tpl)?;
    Ok(tpl)
}

/// Validate Liquid template syntax. Returns Ok(()) or a parse error message.
pub fn validate_template(content: &str) -> Result<(), String> {
    // Check for basic Liquid syntax correctness:
    // Matching {% %} tags, matching {{ }}
    validate_liquid_brackets(content)?;
    validate_liquid_blocks(content)?;
    Ok(())
}

/// Publish a template: write frontmatter+body to file, clear content in DB.
pub fn publish_template(
    conn: &Connection,
    data_dir: &Path,
    template_id: &str,
) -> EngineResult<Template> {
    let mut tpl = qt::get_template_by_id(conn, template_id)?;

    if tpl.status == TemplateStatus::Published {
        return Err(EngineError::Conflict(
            "template is already published".to_string(),
        ));
    }

    let body = tpl
        .content
        .clone()
        .unwrap_or_default();

    // Validate before publishing
    validate_template(&body).map_err(EngineError::Validation)?;

    let now = now_unix_ms();
    let rel_path = format!("templates/{}.liquid", tpl.slug);
    let abs_path = data_dir.join(&rel_path);

    // Ensure templates/ directory exists
    if let Some(parent) = abs_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Build frontmatter
    let fm = TemplateFrontmatter {
        id: tpl.id.clone(),
        project_id: Some(tpl.project_id.clone()),
        slug: tpl.slug.clone(),
        title: tpl.title.clone(),
        kind: template_kind_to_frontmatter(&tpl.kind),
        enabled: tpl.enabled,
        version: tpl.version,
        created_at: tpl.created_at,
        updated_at: now,
    };
    let file_content = write_template_file(&fm, &body);
    atomic_write_str(&abs_path, &file_content)?;

    // Update DB
    tpl.status = TemplateStatus::Published;
    tpl.file_path = rel_path;
    tpl.content = None;
    tpl.updated_at = now;
    qt::update_template(conn, &tpl)?;

    Ok(tpl)
}

/// Unpublish a template: read content from file back into DB, set status to draft.
pub fn unpublish_template(
    conn: &Connection,
    data_dir: &Path,
    template_id: &str,
) -> EngineResult<Template> {
    let mut tpl = qt::get_template_by_id(conn, template_id)?;

    if tpl.status != TemplateStatus::Published {
        return Err(EngineError::Conflict(
            "template is not published".to_string(),
        ));
    }

    // Read body from file
    if !tpl.file_path.is_empty() {
        let abs_path = data_dir.join(&tpl.file_path);
        if abs_path.exists() {
            let file_content = fs::read_to_string(&abs_path)?;
            let (_fm, body) =
                crate::util::frontmatter::read_template_file(&file_content)
                    .map_err(EngineError::Parse)?;
            tpl.content = Some(body);
        }
    }

    tpl.status = TemplateStatus::Draft;
    tpl.updated_at = now_unix_ms();
    qt::update_template(conn, &tpl)?;

    Ok(tpl)
}

/// Delete a template. Checks for references from posts and tags.
pub fn delete_template(
    conn: &Connection,
    data_dir: &Path,
    template_id: &str,
    force: bool,
) -> EngineResult<()> {
    let tpl = qt::get_template_by_id(conn, template_id)?;

    // Check references
    let referencing_posts = count_posts_using_template(conn, &tpl.slug)?;
    let referencing_tags = count_tags_using_template(conn, &tpl.slug)?;

    if (referencing_posts > 0 || referencing_tags > 0) && !force {
        return Err(EngineError::Conflict(format!(
            "template is referenced by {} posts and {} tags",
            referencing_posts, referencing_tags
        )));
    }

    // Force: null out references
    if force {
        if referencing_posts > 0 {
            null_template_slug_on_posts(conn, &tpl.slug)?;
        }
        if referencing_tags > 0 {
            null_template_slug_on_tags(conn, &tpl.slug)?;
        }
    }

    // Delete file if exists
    if !tpl.file_path.is_empty() {
        let abs_path = data_dir.join(&tpl.file_path);
        if abs_path.exists() {
            fs::remove_file(&abs_path)?;
        }
    }

    qt::delete_template(conn, template_id)?;
    Ok(())
}

// --- Helper functions ---

fn template_kind_to_frontmatter(kind: &TemplateKind) -> String {
    match kind {
        TemplateKind::Post => "post".to_string(),
        TemplateKind::List => "list".to_string(),
        TemplateKind::NotFound => "not_found".to_string(),
        TemplateKind::Partial => "partial".to_string(),
    }
}

fn validate_liquid_brackets(content: &str) -> Result<(), String> {
    // Check balanced {{ }} and {% %}
    let mut in_output = false;
    let mut in_tag = false;
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if i + 1 < len {
            let pair = (chars[i], chars[i + 1]);
            match pair {
                ('{', '{') if !in_output && !in_tag => {
                    in_output = true;
                    i += 2;
                    continue;
                }
                ('}', '}') if in_output => {
                    in_output = false;
                    i += 2;
                    continue;
                }
                ('{', '%') if !in_output && !in_tag => {
                    in_tag = true;
                    i += 2;
                    continue;
                }
                ('%', '}') if in_tag => {
                    in_tag = false;
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        i += 1;
    }

    if in_output {
        return Err("unclosed {{ output tag".to_string());
    }
    if in_tag {
        return Err("unclosed {% tag".to_string());
    }
    Ok(())
}

fn validate_liquid_blocks(content: &str) -> Result<(), String> {
    // Check that block tags are balanced (if/endif, for/endfor)
    let mut if_depth: i32 = 0;
    let mut for_depth: i32 = 0;

    // Simple regex-free scanner for {% tag %} blocks
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        if i + 1 < len && chars[i] == '{' && chars[i + 1] == '%' {
            // Find closing %}
            let start = i + 2;
            if let Some(end_offset) = content[start..].find("%}") {
                let tag_content = content[start..start + end_offset].trim();
                let tag_content = tag_content.trim_start_matches('-').trim_end_matches('-').trim();
                let first_word = tag_content
                    .split_whitespace()
                    .next()
                    .unwrap_or("");

                match first_word {
                    "if" => if_depth += 1,
                    "endif" => {
                        if_depth -= 1;
                        if if_depth < 0 {
                            return Err("unexpected {% endif %} without matching {% if %}".to_string());
                        }
                    }
                    "for" => for_depth += 1,
                    "endfor" => {
                        for_depth -= 1;
                        if for_depth < 0 {
                            return Err("unexpected {% endfor %} without matching {% for %}".to_string());
                        }
                    }
                    _ => {}
                }
                i = start + end_offset + 2;
                continue;
            }
        }
        i += 1;
    }

    if if_depth > 0 {
        return Err(format!("{if_depth} unclosed {{% if %}} block(s)"));
    }
    if for_depth > 0 {
        return Err(format!("{for_depth} unclosed {{% for %}} block(s)"));
    }
    Ok(())
}

fn count_posts_using_template(conn: &Connection, slug: &str) -> EngineResult<usize> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM posts WHERE template_slug = ?1",
        rusqlite::params![slug],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}

fn count_tags_using_template(conn: &Connection, slug: &str) -> EngineResult<usize> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM tags WHERE post_template_slug = ?1",
        rusqlite::params![slug],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}

fn null_template_slug_on_posts(conn: &Connection, slug: &str) -> EngineResult<()> {
    conn.execute(
        "UPDATE posts SET template_slug = NULL WHERE template_slug = ?1",
        rusqlite::params![slug],
    )?;
    Ok(())
}

fn null_template_slug_on_tags(conn: &Connection, slug: &str) -> EngineResult<()> {
    conn.execute(
        "UPDATE tags SET post_template_slug = NULL WHERE post_template_slug = ?1",
        rusqlite::params![slug],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::db::Database;
    use tempfile::TempDir;

    fn setup() -> (Database, TempDir) {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = TempDir::new().unwrap();
        (db, dir)
    }

    #[test]
    fn create_draft_template() {
        let (db, _dir) = setup();
        let tpl = create_template(db.conn(), "p1", "My Template", TemplateKind::Post, "<p>hello</p>").unwrap();
        assert_eq!(tpl.title, "My Template");
        assert_eq!(tpl.slug, "my-template");
        assert_eq!(tpl.kind, TemplateKind::Post);
        assert_eq!(tpl.status, TemplateStatus::Draft);
        assert_eq!(tpl.content, Some("<p>hello</p>".to_string()));
        assert!(tpl.file_path.is_empty());
        assert!(tpl.enabled);
        assert_eq!(tpl.version, 1);
    }

    #[test]
    fn create_template_deduplicates_slug() {
        let (db, _dir) = setup();
        let t1 = create_template(db.conn(), "p1", "Test", TemplateKind::Post, "").unwrap();
        let t2 = create_template(db.conn(), "p1", "Test", TemplateKind::Post, "").unwrap();
        assert_eq!(t1.slug, "test");
        assert_eq!(t2.slug, "test-2");
    }

    #[test]
    fn update_template_bumps_version() {
        let (db, _dir) = setup();
        let tpl = create_template(db.conn(), "p1", "Tpl", TemplateKind::Post, "old").unwrap();
        let updated = update_template(
            db.conn(), &tpl.id, "p1",
            Some("New Title"), None, None, None, Some("new content"),
        ).unwrap();
        assert_eq!(updated.title, "New Title");
        assert_eq!(updated.content, Some("new content".to_string()));
        assert_eq!(updated.version, 2);
    }

    #[test]
    fn update_template_slug_conflict() {
        let (db, _dir) = setup();
        create_template(db.conn(), "p1", "Alpha", TemplateKind::Post, "").unwrap();
        let t2 = create_template(db.conn(), "p1", "Beta", TemplateKind::Post, "").unwrap();
        let result = update_template(db.conn(), &t2.id, "p1", None, Some("alpha"), None, None, None);
        assert!(result.is_err());
    }

    #[test]
    fn save_template_updates_content() {
        let (db, _dir) = setup();
        let tpl = create_template(db.conn(), "p1", "Tpl", TemplateKind::Post, "old").unwrap();
        let saved = save_template(db.conn(), &tpl.id, "new content").unwrap();
        assert_eq!(saved.content, Some("new content".to_string()));
        assert_eq!(saved.version, 2);
    }

    #[test]
    fn publish_and_unpublish_template() {
        let (db, dir) = setup();
        let tpl = create_template(db.conn(), "p1", "Pub", TemplateKind::Post, "<div>body</div>").unwrap();

        // Publish
        let published = publish_template(db.conn(), dir.path(), &tpl.id).unwrap();
        assert_eq!(published.status, TemplateStatus::Published);
        assert!(published.content.is_none());
        assert_eq!(published.file_path, "templates/pub.liquid");
        assert!(dir.path().join("templates/pub.liquid").exists());

        // Unpublish
        let unpublished = unpublish_template(db.conn(), dir.path(), &published.id).unwrap();
        assert_eq!(unpublished.status, TemplateStatus::Draft);
        assert_eq!(unpublished.content, Some("<div>body</div>".to_string()));
    }

    #[test]
    fn publish_requires_draft() {
        let (db, dir) = setup();
        let tpl = create_template(db.conn(), "p1", "Tpl", TemplateKind::Post, "body").unwrap();
        publish_template(db.conn(), dir.path(), &tpl.id).unwrap();
        let result = publish_template(db.conn(), dir.path(), &tpl.id);
        assert!(result.is_err());
    }

    #[test]
    fn delete_template_removes_file() {
        let (db, dir) = setup();
        let tpl = create_template(db.conn(), "p1", "Del", TemplateKind::Post, "body").unwrap();
        publish_template(db.conn(), dir.path(), &tpl.id).unwrap();
        assert!(dir.path().join("templates/del.liquid").exists());

        delete_template(db.conn(), dir.path(), &tpl.id, false).unwrap();
        assert!(!dir.path().join("templates/del.liquid").exists());
        assert!(qt::get_template_by_id(db.conn(), &tpl.id).is_err());
    }

    #[test]
    fn validate_valid_liquid() {
        assert!(validate_template("<div>{{ title }}</div>").is_ok());
        assert!(validate_template("{% if x %}<p>y</p>{% endif %}").is_ok());
        assert!(validate_template("{% for p in posts %}{{ p.title }}{% endfor %}").is_ok());
    }

    #[test]
    fn validate_unclosed_output() {
        assert!(validate_template("<div>{{ title </div>").is_err());
    }

    #[test]
    fn validate_unclosed_if() {
        assert!(validate_template("{% if x %}<p>y</p>").is_err());
    }

    #[test]
    fn validate_unmatched_endif() {
        assert!(validate_template("<p>y</p>{% endif %}").is_err());
    }

    #[test]
    fn validate_unclosed_for() {
        assert!(validate_template("{% for p in posts %}{{ p.title }}").is_err());
    }
}
