use std::fs;
use std::path::Path;

use rusqlite::Connection;
use uuid::Uuid;

use crate::db::queries::script as qs;
use crate::engine::{EngineError, EngineResult};
use crate::model::{Script, ScriptKind, ScriptStatus};
use crate::util::frontmatter::{write_script_file, ScriptFrontmatter};
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
    let script = Script {
        id,
        project_id: project_id.to_string(),
        slug,
        title: title.to_string(),
        kind,
        entrypoint: entrypoint.unwrap_or("render").to_string(),
        enabled: true,
        version: 1,
        file_path: String::new(),
        status: ScriptStatus::Draft,
        content: Some(content.to_string()),
        created_at: now,
        updated_at: now,
    };

    qs::insert_script(conn, &script)?;
    Ok(script)
}

/// Update a script's metadata and/or content. Bumps version.
pub fn update_script(
    conn: &Connection,
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

    // Slug uniqueness
    if let Some(new_slug) = slug {
        if new_slug != script.slug {
            if qs::get_script_by_slug(conn, project_id, new_slug).is_ok() {
                return Err(EngineError::Conflict(format!(
                    "script slug '{new_slug}' already exists"
                )));
            }
        }
    }

    if let Some(t) = title {
        script.title = t.to_string();
    }
    if let Some(s) = slug {
        script.slug = s.to_string();
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

    // If published, transition back to draft on edit
    if script.status == ScriptStatus::Published {
        script.status = ScriptStatus::Draft;
    }

    script.version += 1;
    script.updated_at = now_unix_ms();
    qs::update_script(conn, &script)?;
    Ok(script)
}

/// Save script content (editor save). Updates DB, bumps version.
pub fn save_script(
    conn: &Connection,
    script_id: &str,
    content: &str,
) -> EngineResult<Script> {
    let mut script = qs::get_script_by_id(conn, script_id)?;
    script.content = Some(content.to_string());
    script.version += 1;
    script.updated_at = now_unix_ms();

    if script.status == ScriptStatus::Published {
        script.status = ScriptStatus::Draft;
    }

    qs::update_script(conn, &script)?;
    Ok(script)
}

/// Validate Lua script syntax. Returns Ok(()) or a parse error message.
pub fn validate_script_syntax(content: &str) -> Result<(), String> {
    // Basic validation: balanced block structures
    validate_lua_blocks(content)?;
    Ok(())
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
pub fn publish_script(
    conn: &Connection,
    data_dir: &Path,
    script_id: &str,
) -> EngineResult<Script> {
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
        return Err(EngineError::Conflict(
            "script is not published".to_string(),
        ));
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

    Ok(script)
}

/// Delete a script and its file.
pub fn delete_script(
    conn: &Connection,
    data_dir: &Path,
    script_id: &str,
) -> EngineResult<()> {
    let script = qs::get_script_by_id(conn, script_id)?;

    // Delete file if exists
    if !script.file_path.is_empty() {
        let abs_path = data_dir.join(&script.file_path);
        if abs_path.exists() {
            fs::remove_file(&abs_path)?;
        }
    }

    qs::delete_script(conn, script_id)?;
    Ok(())
}

// --- Helper functions ---

fn script_kind_to_frontmatter(kind: &ScriptKind) -> String {
    match kind {
        ScriptKind::Macro => "macro".to_string(),
        ScriptKind::Utility => "utility".to_string(),
        ScriptKind::Transform => "transform".to_string(),
    }
}

fn validate_lua_blocks(content: &str) -> Result<(), String> {
    // Track block-level nesting for function/end, if/end, for/end, while/end, do/end
    let mut depth: i32 = 0;

    for line in content.lines() {
        let trimmed = line.trim();
        // Skip comments
        if trimmed.starts_with("--") {
            continue;
        }

        // Count block openers
        for word in trimmed.split_whitespace() {
            match word {
                "function" | "if" | "for" | "while" | "do" => depth += 1,
                "end" | "end," | "end;" => {
                    depth -= 1;
                    if depth < 0 {
                        return Err("unexpected 'end' without matching block opener".to_string());
                    }
                }
                _ => {}
            }
        }

        // Handle "then" on if lines (don't double-count)
        // Handle inline "function() ... end"
    }

    if depth > 0 {
        return Err(format!("{depth} unclosed block(s) — missing 'end'"));
    }
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
    fn create_draft_script() {
        let (db, _dir) = setup();
        let s = create_script(db.conn(), "p1", "My Script", ScriptKind::Macro, "return 'hi'", None).unwrap();
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
        let s = create_script(db.conn(), "p1", "Util", ScriptKind::Utility, "", Some("main")).unwrap();
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
        let (db, _dir) = setup();
        let s = create_script(db.conn(), "p1", "S", ScriptKind::Macro, "old", None).unwrap();
        let updated = update_script(db.conn(), &s.id, "p1", Some("New"), None, None, None, None, Some("new")).unwrap();
        assert_eq!(updated.title, "New");
        assert_eq!(updated.content, Some("new".to_string()));
        assert_eq!(updated.version, 2);
    }

    #[test]
    fn save_script_updates_content() {
        let (db, _dir) = setup();
        let s = create_script(db.conn(), "p1", "S", ScriptKind::Macro, "old", None).unwrap();
        let saved = save_script(db.conn(), &s.id, "new body").unwrap();
        assert_eq!(saved.content, Some("new body".to_string()));
        assert_eq!(saved.version, 2);
    }

    #[test]
    fn publish_and_unpublish_script() {
        let (db, dir) = setup();
        let s = create_script(db.conn(), "p1", "Pub", ScriptKind::Macro, "function render()\n  return 'hi'\nend", None).unwrap();

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
        let s = create_script(db.conn(), "p1", "Del", ScriptKind::Macro, "function render()\nend", None).unwrap();
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
