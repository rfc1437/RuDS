use std::fs;
use std::path::Path;

use crate::db::DbConnection as Connection;
use crate::db::schema::{posts, tags};
use diesel::prelude::*;
use uuid::Uuid;

use crate::db::queries::template as qt;
use crate::engine::{EngineError, EngineResult, domain_events};
use crate::model::{DomainEntity, NotificationAction, Template, TemplateKind, TemplateStatus};
use crate::util::frontmatter::{TemplateFrontmatter, write_template_file};
use crate::util::{atomic_write_str, ensure_unique, now_unix_ms, slugify};

const ALLOWED_LIQUID_TAGS: &[&str] = &[
    "if", "elsif", "else", "endif", "for", "endfor", "assign", "render",
];
const ALLOWED_LIQUID_FILTERS: &[&str] = &[
    "escape",
    "url_encode",
    "default",
    "append",
    "i18n",
    "markdown",
    "slugify",
];

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
    emit_template(&tpl, NotificationAction::Created);
    Ok(tpl)
}

/// Update a template's metadata and/or content. Bumps version.
#[expect(
    clippy::too_many_arguments,
    reason = "optional arguments represent independent template field changes"
)]
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
    if let Some(new_slug) = slug
        && new_slug != tpl.slug
        && qt::get_template_by_slug(conn, project_id, new_slug).is_ok()
    {
        return Err(EngineError::Conflict(format!(
            "template slug '{new_slug}' already exists"
        )));
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
    emit_template(&tpl, NotificationAction::Updated);
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
    emit_template(&tpl, NotificationAction::Updated);
    Ok(tpl)
}

/// Validate Liquid template syntax. Returns Ok(()) or a parse error message.
pub fn validate_template(content: &str) -> Result<(), String> {
    validate_liquid_subset(content)?;
    crate::render::validate_liquid_template_syntax(content)
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

    let body = tpl.content.clone().unwrap_or_default();

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

    emit_template(&tpl, NotificationAction::Updated);

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
            let (_fm, body) = crate::util::frontmatter::read_template_file(&file_content)
                .map_err(EngineError::Parse)?;
            tpl.content = Some(body);
        }
    }

    tpl.status = TemplateStatus::Draft;
    tpl.updated_at = now_unix_ms();
    qt::update_template(conn, &tpl)?;

    emit_template(&tpl, NotificationAction::Updated);

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
    emit_template(&tpl, NotificationAction::Deleted);
    Ok(())
}

fn emit_template(template: &Template, action: NotificationAction) {
    domain_events::entity_changed(
        &template.project_id,
        DomainEntity::Template,
        &template.id,
        action,
    );
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

fn validate_liquid_subset(content: &str) -> Result<(), String> {
    let mut cursor = 0;
    while let Some((offset, is_tag)) = next_liquid_markup(&content[cursor..]) {
        let markup_start = cursor + offset + 2;
        let close = if is_tag { "%}" } else { "}}" };
        let Some(close_offset) = find_unquoted(&content[markup_start..], close) else {
            break;
        };
        let markup_end = markup_start + close_offset;
        let markup = content[markup_start..markup_end]
            .trim()
            .trim_start_matches('-')
            .trim_end_matches('-')
            .trim();

        let tag = is_tag.then(|| identifier_at_start(markup)).flatten();
        if let Some(tag) = tag
            && !ALLOWED_LIQUID_TAGS.contains(&tag)
        {
            return Err(format!("unsupported tag: {tag}"));
        }

        validate_liquid_filters(markup)?;
        if let Some(tag @ ("if" | "elsif")) = tag {
            validate_liquid_operators(markup[tag.len()..].trim_start())?;
        }
        cursor = markup_end + close.len();
    }
    Ok(())
}

fn next_liquid_markup(content: &str) -> Option<(usize, bool)> {
    match (content.find("{%"), content.find("{{")) {
        (Some(tag), Some(output)) if tag <= output => Some((tag, true)),
        (Some(_), Some(output)) => Some((output, false)),
        (Some(tag), None) => Some((tag, true)),
        (None, Some(output)) => Some((output, false)),
        (None, None) => None,
    }
}

fn find_unquoted(content: &str, needle: &str) -> Option<usize> {
    let bytes = content.as_bytes();
    let needle = needle.as_bytes();
    let mut quote = None;
    let mut escaped = false;
    let mut index = 0;
    while index + needle.len() <= bytes.len() {
        let byte = bytes[index];
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
        } else if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
        } else if &bytes[index..index + needle.len()] == needle {
            return Some(index);
        }
        index += 1;
    }
    None
}

fn identifier_at_start(content: &str) -> Option<&str> {
    let end = content
        .find(|character: char| !is_liquid_identifier(character))
        .unwrap_or(content.len());
    (end > 0).then(|| &content[..end])
}

fn is_liquid_identifier(character: char) -> bool {
    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
}

fn validate_liquid_filters(markup: &str) -> Result<(), String> {
    for pipe in unquoted_byte_positions(markup, b'|') {
        let after_pipe = markup[pipe + 1..].trim_start();
        if let Some(filter) = identifier_at_start(after_pipe)
            && !ALLOWED_LIQUID_FILTERS.contains(&filter)
        {
            return Err(format!("unsupported filter: {filter}"));
        }
    }
    Ok(())
}

fn validate_liquid_operators(markup: &str) -> Result<(), String> {
    let bytes = markup.as_bytes();
    let mut quote = None;
    let mut escaped = false;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            index += 1;
            continue;
        }
        if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
            index += 1;
            continue;
        }

        let operator = match (byte, bytes.get(index + 1).copied()) {
            (b'!', Some(b'=')) => Some("!="),
            (b'>', Some(b'=')) => Some(">="),
            (b'<', Some(b'=')) => Some("<="),
            (b'<', _) => Some("<"),
            _ => None,
        };
        if let Some(operator) = operator {
            return Err(format!("unsupported operator: {operator}"));
        }

        if bytes[index..].starts_with(b"contains")
            && is_word_boundary(bytes.get(index.wrapping_sub(1)).copied())
            && is_word_boundary(bytes.get(index + "contains".len()).copied())
            && !markup[..index].trim_end().is_empty()
            && !markup[index + "contains".len()..].trim_start().is_empty()
            && !markup[..index].trim_end().ends_with('.')
        {
            return Err("unsupported operator: contains".to_string());
        }
        index += 1;
    }
    Ok(())
}

fn is_word_boundary(byte: Option<u8>) -> bool {
    byte.is_none_or(|byte| !byte.is_ascii_alphanumeric() && !matches!(byte, b'_' | b'-'))
}

fn unquoted_byte_positions(content: &str, target: u8) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut quote = None;
    let mut escaped = false;
    for (index, byte) in content.bytes().enumerate() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
        } else if matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
        } else if byte == target {
            positions.push(index);
        }
    }
    positions
}

fn count_posts_using_template(conn: &Connection, slug: &str) -> EngineResult<usize> {
    let count: i64 = conn.with(|c| {
        posts::table
            .filter(posts::template_slug.eq(slug))
            .count()
            .get_result(c)
    })?;
    Ok(count as usize)
}

fn count_tags_using_template(conn: &Connection, slug: &str) -> EngineResult<usize> {
    let count: i64 = conn.with(|c| {
        tags::table
            .filter(tags::post_template_slug.eq(slug))
            .count()
            .get_result(c)
    })?;
    Ok(count as usize)
}

fn null_template_slug_on_posts(conn: &Connection, slug: &str) -> EngineResult<()> {
    conn.with(|c| {
        diesel::update(posts::table.filter(posts::template_slug.eq(slug)))
            .set(posts::template_slug.eq(None::<String>))
            .execute(c)
    })?;
    Ok(())
}

fn null_template_slug_on_tags(conn: &Connection, slug: &str) -> EngineResult<()> {
    conn.with(|c| {
        diesel::update(tags::table.filter(tags::post_template_slug.eq(slug)))
            .set(tags::post_template_slug.eq(None::<String>))
            .execute(c)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::project::{insert_project, make_test_project};
    use tempfile::TempDir;

    fn setup() -> (Database, TempDir) {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = TempDir::new().unwrap();
        (db, dir)
    }

    #[test]
    fn create_draft_template() {
        let (db, _dir) = setup();
        let tpl = create_template(
            db.conn(),
            "p1",
            "My Template",
            TemplateKind::Post,
            "<p>hello</p>",
        )
        .unwrap();
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
            db.conn(),
            &tpl.id,
            "p1",
            Some("New Title"),
            None,
            None,
            None,
            Some("new content"),
        )
        .unwrap();
        assert_eq!(updated.title, "New Title");
        assert_eq!(updated.content, Some("new content".to_string()));
        assert_eq!(updated.version, 2);
    }

    #[test]
    fn update_template_slug_conflict() {
        let (db, _dir) = setup();
        create_template(db.conn(), "p1", "Alpha", TemplateKind::Post, "").unwrap();
        let t2 = create_template(db.conn(), "p1", "Beta", TemplateKind::Post, "").unwrap();
        let result = update_template(
            db.conn(),
            &t2.id,
            "p1",
            None,
            Some("alpha"),
            None,
            None,
            None,
        );
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
        let tpl = create_template(
            db.conn(),
            "p1",
            "Pub",
            TemplateKind::Post,
            "<div>body</div>",
        )
        .unwrap();

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
    fn validate_rejects_unsupported_liquid_tags() {
        for tag in [
            "unless",
            "case",
            "capture",
            "tablerow",
            "cycle",
            "increment",
        ] {
            let content = format!("{{% {tag} foo %}}bar{{% end{tag} %}}");
            assert_eq!(
                validate_template(&content),
                Err(format!("unsupported tag: {tag}"))
            );
        }
    }

    #[test]
    fn validate_rejects_unsupported_liquid_filters() {
        for filter in ["upcase", "downcase", "date", "truncate", "split", "reverse"] {
            let content = format!("{{{{ title | {filter} }}}}");
            assert_eq!(
                validate_template(&content),
                Err(format!("unsupported filter: {filter}"))
            );
        }
    }

    #[test]
    fn validate_rejects_unsupported_liquid_operators() {
        for operator in ["!=", "<", ">=", "<=", "contains"] {
            let content = format!("{{% if title {operator} other %}}yes{{% endif %}}");
            assert_eq!(
                validate_template(&content),
                Err(format!("unsupported operator: {operator}"))
            );
        }
    }

    #[test]
    fn validate_allows_the_complete_liquid_subset() {
        for filter in [
            "escape",
            "url_encode",
            "default: fallback",
            "append: suffix",
            "i18n: language",
            "markdown: post.id, posts, post_paths, media_paths, language, prefix",
            "slugify",
        ] {
            let result = validate_template(&format!("{{{{ title | {filter} }}}}"));
            assert!(
                result.is_ok(),
                "supported filter {filter} was rejected: {result:?}"
            );
        }
        for content in [
            "{% if title == other %}yes{% elsif total > 0 %}more{% else %}no{% endif %}",
            "{% if published %}yes{% endif %}",
            "{% if a == b and c > d %}yes{% endif %}",
            "{% if a == b or c == blank %}yes{% endif %}",
            "{% assign href = '/posts/' | append: post.slug %}",
            "{% render 'partials/card', post: post %}",
            "{%- for post in posts -%}{{- post.title | escape -}}{%- endfor -%}",
            "{% if values.size > 0 and map[key] %}yes{% endif %}",
        ] {
            let result = validate_template(content);
            assert!(
                result.is_ok(),
                "supported syntax was rejected: {content}: {result:?}"
            );
        }
    }

    #[test]
    fn bundled_starter_templates_conform_to_the_published_liquid_subset() {
        for (name, content) in [
            (
                "single-post",
                include_str!("../../../../assets/starter-templates/single-post.liquid"),
            ),
            (
                "post-list",
                include_str!("../../../../assets/starter-templates/post-list.liquid"),
            ),
            (
                "not-found",
                include_str!("../../../../assets/starter-templates/not-found.liquid"),
            ),
            (
                "head",
                include_str!("../../../../assets/starter-templates/partials/head.liquid"),
            ),
            (
                "language-switcher",
                include_str!(
                    "../../../../assets/starter-templates/partials/language-switcher.liquid"
                ),
            ),
            (
                "menu-items",
                include_str!("../../../../assets/starter-templates/partials/menu-items.liquid"),
            ),
            (
                "menu",
                include_str!("../../../../assets/starter-templates/partials/menu.liquid"),
            ),
        ] {
            let result = validate_template(content);
            assert!(result.is_ok(), "starter template {name}: {result:?}");
        }
    }

    #[test]
    fn publish_rejects_unsupported_filter_and_leaves_template_draft() {
        let (db, dir) = setup();
        let template = create_template(
            db.conn(),
            "p1",
            "Strict",
            TemplateKind::Post,
            "{{ title | upcase }}",
        )
        .unwrap();

        let error = publish_template(db.conn(), dir.path(), &template.id).unwrap_err();

        assert!(error.to_string().contains("unsupported filter: upcase"));
        let reloaded = qt::get_template_by_id(db.conn(), &template.id).unwrap();
        assert_eq!(reloaded.status, TemplateStatus::Draft);
        assert_eq!(reloaded.content.as_deref(), Some("{{ title | upcase }}"));
        assert!(!dir.path().join("templates/strict.liquid").exists());
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
