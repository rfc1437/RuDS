use std::fs;
use std::path::Path;

use crate::db::DbConnection as Connection;
use uuid::Uuid;

use crate::db::queries::script as qs;
use crate::engine::{EngineError, EngineResult, domain_events};
use crate::model::{DomainEntity, NotificationAction, Script, ScriptKind, ScriptStatus};
use crate::util::frontmatter::{ScriptFrontmatter, write_script_file};
use crate::util::paths::script_file_path;
use crate::util::{atomic_write_str, ensure_unique, now_unix_ms, slugify};

/// Create a new draft script. Content stored in DB, no file written.
pub fn create_script(
    conn: &Connection,
    project_id: &str,
    title: &str,
    kind: ScriptKind,
    content: &str,
    entrypoint: Option<&str>,
) -> EngineResult<Script> {
    let slug_source = if title.is_empty() { "untitled" } else { title };
    let base_slug = slugify(slug_source);
    let base_slug = if base_slug.is_empty() {
        "untitled".to_string()
    } else {
        base_slug
    };
    let slug = ensure_unique(&base_slug, |candidate| {
        qs::get_script_by_slug(conn, project_id, candidate).is_ok()
    });

    let now = now_unix_ms();
    let id = Uuid::new_v4().to_string();
    let entrypoint = entrypoint.unwrap_or(match &kind {
        ScriptKind::Macro => "render",
        ScriptKind::Utility | ScriptKind::Transform => "main",
    });
    let script = Script {
        id,
        project_id: project_id.to_string(),
        slug,
        title: title.to_string(),
        kind,
        entrypoint: entrypoint.to_string(),
        enabled: true,
        version: 1,
        file_path: String::new(),
        status: ScriptStatus::Draft,
        content: Some(content.to_string()),
        created_at: now,
        updated_at: now,
    };

    qs::insert_script(conn, &script)?;
    emit_script(&script, NotificationAction::Created);
    Ok(script)
}

/// Update a script's metadata and/or content. Bumps version.
#[expect(
    clippy::too_many_arguments,
    reason = "optional arguments represent independent script field changes"
)]
pub fn update_script(
    conn: &Connection,
    data_dir: &Path,
    script_id: &str,
    project_id: &str,
    title: Option<&str>,
    slug: Option<&str>,
    kind: Option<ScriptKind>,
    entrypoint: Option<&str>,
    enabled: Option<bool>,
    content: Option<&str>,
) -> EngineResult<Script> {
    let mut script = qs::get_script_by_id(conn, script_id)?;
    if script.project_id != project_id {
        return Err(EngineError::NotFound(format!("script {script_id}")));
    }
    let original_slug = script.slug.clone();
    let original_file_path = script.file_path.clone();
    let was_published = script.status == ScriptStatus::Published;
    let effective_content = script.content.clone().unwrap_or_else(|| {
        if original_file_path.is_empty() {
            String::new()
        } else {
            read_published_script_body(data_dir, &original_file_path)
        }
    });
    let content_changed = content.is_some_and(|new_content| new_content != effective_content);

    if let Some(requested_slug) = slug {
        let slug = slugify(requested_slug);
        let slug = if slug.is_empty() {
            "script".to_string()
        } else {
            slug
        };
        script.slug = ensure_unique(&slug, |candidate| {
            qs::get_script_by_slug(conn, project_id, candidate)
                .is_ok_and(|existing| existing.id != script_id)
        });
    }

    if let Some(t) = title {
        script.title = t.to_string();
    }
    if let Some(k) = kind {
        script.kind = k;
    }
    if let Some(ep) = entrypoint {
        script.entrypoint = ep.to_string();
    }
    if let Some(e) = enabled {
        script.enabled = e;
    }
    if let Some(c) = content {
        script.content = Some(c.to_string());
    }

    let slug_changed = script.slug != original_slug;
    let published_body = if !original_file_path.is_empty()
        && (slug_changed || (was_published && !content_changed))
    {
        Some(effective_content)
    } else {
        None
    };
    if slug_changed && published_body.is_some() {
        script.file_path = script_file_path(&script.slug);
    }

    if was_published && content_changed {
        script.status = ScriptStatus::Draft;
    }

    script.version += 1;
    script.updated_at = now_unix_ms();
    qs::update_script(conn, &script)?;
    if let Some(body) = published_body {
        rewrite_renamed_script_file(data_dir, &original_file_path, &script, &body)?;
    }
    emit_script(&script, NotificationAction::Updated);
    Ok(script)
}

/// Save script content (editor save). Updates DB, bumps version.
pub fn save_script(
    conn: &Connection,
    data_dir: &Path,
    script_id: &str,
    content: &str,
) -> EngineResult<Script> {
    let mut script = qs::get_script_by_id(conn, script_id)?;
    let was_published = script.status == ScriptStatus::Published;
    let effective_content = script.content.clone().unwrap_or_else(|| {
        if script.file_path.is_empty() {
            String::new()
        } else {
            read_published_script_body(data_dir, &script.file_path)
        }
    });
    let content_changed = content != effective_content;
    script.content = Some(content.to_string());
    script.version += 1;
    script.updated_at = now_unix_ms();

    if was_published && content_changed {
        script.status = ScriptStatus::Draft;
    }

    qs::update_script(conn, &script)?;
    if was_published && !content_changed && !script.file_path.is_empty() {
        rewrite_renamed_script_file(data_dir, &script.file_path, &script, &effective_content)?;
    }
    emit_script(&script, NotificationAction::Updated);
    Ok(script)
}

/// Validate Lua script syntax. Returns Ok(()) or a parse error message.
pub fn validate_script_syntax(content: &str) -> Result<(), String> {
    crate::scripting::validate(content)
}

/// Discover entrypoint function names from Lua source.
/// Returns list of function names, with "main" always first if present.
pub fn discover_entrypoints(content: &str) -> Vec<String> {
    let mut funcs = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("function ") {
            // Extract function name: "function name(" or "function name (..."
            let rest = trimmed.strip_prefix("function ").unwrap_or("").trim();
            if let Some(name) = rest.split(&['(', ' '][..]).next() {
                let name = name.trim();
                if !name.is_empty() && !name.contains('.') && !name.contains(':') {
                    funcs.push(name.to_string());
                }
            }
        }
    }

    // Ensure "main" is first if present
    if let Some(pos) = funcs.iter().position(|n| n == "main") {
        funcs.remove(pos);
        funcs.insert(0, "main".to_string());
    }

    funcs
}

/// Publish a script: write frontmatter+body to file, clear content in DB.
pub fn publish_script(conn: &Connection, data_dir: &Path, script_id: &str) -> EngineResult<Script> {
    let mut script = qs::get_script_by_id(conn, script_id)?;

    if script.status == ScriptStatus::Published {
        return Err(EngineError::Conflict(
            "script is already published".to_string(),
        ));
    }

    let body = script.content.clone().unwrap_or_default();

    // Validate before publishing
    validate_script_syntax(&body).map_err(EngineError::Validation)?;

    let now = now_unix_ms();
    let rel_path = format!("scripts/{}.lua", script.slug);
    let abs_path = data_dir.join(&rel_path);

    // Ensure scripts/ directory exists
    if let Some(parent) = abs_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let fm = ScriptFrontmatter {
        id: script.id.clone(),
        project_id: Some(script.project_id.clone()),
        slug: script.slug.clone(),
        title: script.title.clone(),
        kind: script_kind_to_frontmatter(&script.kind),
        entrypoint: script.entrypoint.clone(),
        enabled: script.enabled,
        version: script.version,
        created_at: script.created_at,
        updated_at: now,
    };
    let file_content = write_script_file(&fm, &body);
    atomic_write_str(&abs_path, &file_content)?;

    script.status = ScriptStatus::Published;
    script.file_path = rel_path;
    script.content = None;
    script.updated_at = now;
    qs::update_script(conn, &script)?;

    emit_script(&script, NotificationAction::Updated);

    Ok(script)
}

/// Unpublish a script: read content from file back into DB, set status to draft.
pub fn unpublish_script(
    conn: &Connection,
    data_dir: &Path,
    script_id: &str,
) -> EngineResult<Script> {
    let mut script = qs::get_script_by_id(conn, script_id)?;

    if script.status != ScriptStatus::Published {
        return Err(EngineError::Conflict("script is not published".to_string()));
    }

    if !script.file_path.is_empty() {
        let abs_path = data_dir.join(&script.file_path);
        if abs_path.exists() {
            let file_content = fs::read_to_string(&abs_path)?;
            let (_fm, body) = crate::util::frontmatter::read_script_file(&file_content)
                .map_err(EngineError::Parse)?;
            script.content = Some(body);
        }
    }

    script.status = ScriptStatus::Draft;
    script.updated_at = now_unix_ms();
    qs::update_script(conn, &script)?;

    emit_script(&script, NotificationAction::Updated);

    Ok(script)
}

/// Delete a script and its file.
pub fn delete_script(conn: &Connection, data_dir: &Path, script_id: &str) -> EngineResult<()> {
    let script = qs::get_script_by_id(conn, script_id)?;

    // Delete file if exists
    if !script.file_path.is_empty() {
        let abs_path = data_dir.join(&script.file_path);
        if abs_path.exists() {
            fs::remove_file(&abs_path)?;
        }
    }

    qs::delete_script(conn, script_id)?;
    emit_script(&script, NotificationAction::Deleted);
    Ok(())
}

fn emit_script(script: &Script, action: NotificationAction) {
    domain_events::entity_changed(&script.project_id, DomainEntity::Script, &script.id, action);
}

// --- Helper functions ---

fn script_kind_to_frontmatter(kind: &ScriptKind) -> String {
    match kind {
        ScriptKind::Macro => "macro".to_string(),
        ScriptKind::Utility => "utility".to_string(),
        ScriptKind::Transform => "transform".to_string(),
    }
}

fn read_published_script_body(data_dir: &Path, file_path: &str) -> String {
    fs::read_to_string(data_dir.join(file_path))
        .ok()
        .and_then(|file| crate::util::frontmatter::read_script_file(&file).ok())
        .map(|(_, body)| body)
        .unwrap_or_default()
}

fn rewrite_renamed_script_file(
    data_dir: &Path,
    original_file_path: &str,
    script: &Script,
    body: &str,
) -> EngineResult<()> {
    let frontmatter = ScriptFrontmatter {
        id: script.id.clone(),
        project_id: Some(script.project_id.clone()),
        slug: script.slug.clone(),
        title: script.title.clone(),
        kind: script_kind_to_frontmatter(&script.kind),
        entrypoint: script.entrypoint.clone(),
        enabled: script.enabled,
        version: script.version,
        created_at: script.created_at,
        updated_at: script.updated_at,
    };
    atomic_write_str(
        &data_dir.join(&script.file_path),
        &write_script_file(&frontmatter, body),
    )?;
    if original_file_path != script.file_path {
        match fs::remove_file(data_dir.join(original_file_path)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
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
    fn create_draft_script() {
        let (db, _dir) = setup();
        let s = create_script(
            db.conn(),
            "p1",
            "My Script",
            ScriptKind::Macro,
            "return 'hi'",
            None,
        )
        .unwrap();
        assert_eq!(s.title, "My Script");
        assert_eq!(s.slug, "my-script");
        assert_eq!(s.kind, ScriptKind::Macro);
        assert_eq!(s.status, ScriptStatus::Draft);
        assert_eq!(s.content, Some("return 'hi'".to_string()));
        assert_eq!(s.entrypoint, "render");
        assert!(s.file_path.is_empty());
        assert!(s.enabled);
    }

    #[test]
    fn create_with_custom_entrypoint() {
        let (db, _dir) = setup();
        let s = create_script(
            db.conn(),
            "p1",
            "Util",
            ScriptKind::Utility,
            "",
            Some("main"),
        )
        .unwrap();
        assert_eq!(s.entrypoint, "main");
    }

    #[test]
    fn create_deduplicates_slug() {
        let (db, _dir) = setup();
        let s1 = create_script(db.conn(), "p1", "Test", ScriptKind::Macro, "", None).unwrap();
        let s2 = create_script(db.conn(), "p1", "Test", ScriptKind::Macro, "", None).unwrap();
        assert_eq!(s1.slug, "test");
        assert_eq!(s2.slug, "test-2");
    }

    #[test]
    fn update_script_bumps_version() {
        let (db, dir) = setup();
        let s = create_script(db.conn(), "p1", "S", ScriptKind::Macro, "old", None).unwrap();
        let updated = update_script(
            db.conn(),
            dir.path(),
            &s.id,
            "p1",
            Some("New"),
            None,
            None,
            None,
            None,
            Some("new"),
        )
        .unwrap();
        assert_eq!(updated.title, "New");
        assert_eq!(updated.content, Some("new".to_string()));
        assert_eq!(updated.version, 2);
    }

    #[test]
    fn update_script_slug_normalizes_and_uniquifies() {
        let (db, dir) = setup();
        create_script(db.conn(), "p1", "Alpha", ScriptKind::Utility, "", None).unwrap();
        let script = create_script(db.conn(), "p1", "Beta", ScriptKind::Utility, "", None).unwrap();

        let updated = update_script(
            db.conn(),
            dir.path(),
            &script.id,
            "p1",
            None,
            Some(" Alpha! "),
            None,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(updated.slug, "alpha-2");
        assert!(updated.file_path.is_empty());

        let unchanged = update_script(
            db.conn(),
            dir.path(),
            &script.id,
            "p1",
            None,
            Some(" Alpha 2 "),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(unchanged.slug, "alpha-2");
    }

    #[test]
    fn update_published_script_slug_rewrites_and_renames_file() {
        let (db, dir) = setup();
        let script = create_script(
            db.conn(),
            "p1",
            "Published Script",
            ScriptKind::Utility,
            "function main()\nend",
            None,
        )
        .unwrap();
        let published = publish_script(db.conn(), dir.path(), &script.id).unwrap();
        let old_path = dir.path().join(&published.file_path);

        let updated = update_script(
            db.conn(),
            dir.path(),
            &script.id,
            "p1",
            None,
            Some(" Renamed Script! "),
            None,
            None,
            None,
            None,
        )
        .unwrap();

        assert_eq!(updated.slug, "renamed-script");
        assert_eq!(updated.file_path, "scripts/renamed-script.lua");
        assert!(!old_path.exists());
        let new_file = fs::read_to_string(dir.path().join(&updated.file_path)).unwrap();
        let (frontmatter, body) = crate::util::frontmatter::read_script_file(&new_file).unwrap();
        assert_eq!(frontmatter.slug, "renamed-script");
        assert_eq!(body, "function main()\nend");
        assert_eq!(updated.status, ScriptStatus::Published);
    }

    #[test]
    fn published_script_metadata_update_stays_published_and_rewrites_frontmatter() {
        let (db, dir) = setup();
        let script = create_script(
            db.conn(),
            "p1",
            "Published Script",
            ScriptKind::Utility,
            "function main()\nend",
            None,
        )
        .unwrap();
        let published = publish_script(db.conn(), dir.path(), &script.id).unwrap();

        let updated = update_script(
            db.conn(),
            dir.path(),
            &script.id,
            "p1",
            Some("Renamed title"),
            None,
            Some(ScriptKind::Transform),
            Some("transform"),
            Some(false),
            None,
        )
        .unwrap();

        assert_eq!(updated.status, ScriptStatus::Published);
        let file = fs::read_to_string(dir.path().join(&published.file_path)).unwrap();
        let (frontmatter, body) = crate::util::frontmatter::read_script_file(&file).unwrap();
        assert_eq!(frontmatter.title, "Renamed title");
        assert_eq!(frontmatter.kind, "transform");
        assert_eq!(frontmatter.entrypoint, "transform");
        assert!(!frontmatter.enabled);
        assert_eq!(frontmatter.version, updated.version);
        assert_eq!(body, "function main()\nend");
    }

    #[test]
    fn published_script_content_change_reopens_draft_and_preserves_published_file() {
        let (db, dir) = setup();
        let script = create_script(
            db.conn(),
            "p1",
            "Published Script",
            ScriptKind::Utility,
            "function main()\n  return 'old'\nend",
            None,
        )
        .unwrap();
        let published = publish_script(db.conn(), dir.path(), &script.id).unwrap();

        let updated = update_script(
            db.conn(),
            dir.path(),
            &script.id,
            "p1",
            None,
            None,
            None,
            None,
            None,
            Some("function main()\n  return 'new'\nend"),
        )
        .unwrap();

        assert_eq!(updated.status, ScriptStatus::Draft);
        assert_eq!(
            updated.content.as_deref(),
            Some("function main()\n  return 'new'\nend")
        );
        let file = fs::read_to_string(dir.path().join(&published.file_path)).unwrap();
        let (_, body) = crate::util::frontmatter::read_script_file(&file).unwrap();
        assert_eq!(body, "function main()\n  return 'old'\nend");
    }

    #[test]
    fn identical_published_script_save_stays_published() {
        let (db, dir) = setup();
        let script = create_script(
            db.conn(),
            "p1",
            "Published Script",
            ScriptKind::Utility,
            "function main()\nend",
            None,
        )
        .unwrap();
        let published = publish_script(db.conn(), dir.path(), &script.id).unwrap();

        let saved = save_script(db.conn(), dir.path(), &script.id, "function main()\nend").unwrap();

        assert_eq!(saved.status, ScriptStatus::Published);
        let file = fs::read_to_string(dir.path().join(&published.file_path)).unwrap();
        let (frontmatter, body) = crate::util::frontmatter::read_script_file(&file).unwrap();
        assert_eq!(frontmatter.version, saved.version);
        assert_eq!(body, "function main()\nend");
    }

    #[test]
    fn save_script_updates_content() {
        let (db, dir) = setup();
        let s = create_script(db.conn(), "p1", "S", ScriptKind::Macro, "old", None).unwrap();
        let saved = save_script(db.conn(), dir.path(), &s.id, "new body").unwrap();
        assert_eq!(saved.content, Some("new body".to_string()));
        assert_eq!(saved.version, 2);
    }

    #[test]
    fn publish_and_unpublish_script() {
        let (db, dir) = setup();
        let s = create_script(
            db.conn(),
            "p1",
            "Pub",
            ScriptKind::Macro,
            "function render()\n  return 'hi'\nend",
            None,
        )
        .unwrap();

        let published = publish_script(db.conn(), dir.path(), &s.id).unwrap();
        assert_eq!(published.status, ScriptStatus::Published);
        assert!(published.content.is_none());
        assert_eq!(published.file_path, "scripts/pub.lua");
        assert!(dir.path().join("scripts/pub.lua").exists());

        let unpublished = unpublish_script(db.conn(), dir.path(), &published.id).unwrap();
        assert_eq!(unpublished.status, ScriptStatus::Draft);
        assert!(unpublished.content.is_some());
    }

    #[test]
    fn delete_script_removes_file() {
        let (db, dir) = setup();
        let s = create_script(
            db.conn(),
            "p1",
            "Del",
            ScriptKind::Macro,
            "function render()\nend",
            None,
        )
        .unwrap();
        publish_script(db.conn(), dir.path(), &s.id).unwrap();
        assert!(dir.path().join("scripts/del.lua").exists());

        delete_script(db.conn(), dir.path(), &s.id).unwrap();
        assert!(!dir.path().join("scripts/del.lua").exists());
        assert!(qs::get_script_by_id(db.conn(), &s.id).is_err());
    }

    #[test]
    fn discover_entrypoints_finds_functions() {
        let code = "function render()\n  return 'hi'\nend\n\nfunction helper(x)\n  return x\nend";
        let funcs = discover_entrypoints(code);
        assert_eq!(funcs, vec!["render", "helper"]);
    }

    #[test]
    fn discover_entrypoints_main_first() {
        let code = "function helper()\nend\n\nfunction main()\nend";
        let funcs = discover_entrypoints(code);
        assert_eq!(funcs[0], "main");
    }

    #[test]
    fn validate_valid_lua() {
        assert!(validate_script_syntax("function render()\n  return 'hi'\nend").is_ok());
    }

    #[test]
    fn validate_unclosed_function() {
        assert!(validate_script_syntax("function render()\n  return 'hi'").is_err());
    }

    #[test]
    fn validate_extra_end() {
        assert!(validate_script_syntax("end").is_err());
    }
}
