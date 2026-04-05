use std::fs;
use std::path::Path;

use rusqlite::Connection;
use walkdir::WalkDir;

use crate::db::queries::script as qs;
use crate::engine::{EngineError, EngineResult};
use crate::model::{Script, ScriptKind, ScriptStatus};
use crate::util::frontmatter::read_script_file;
use crate::util::now_unix_ms;

/// Report returned by `rebuild_scripts_from_filesystem`.
#[derive(Debug, Default)]
pub struct ScriptRebuildReport {
    pub created: usize,
    pub updated: usize,
    pub errors: Vec<String>,
}

/// Parse a script kind string from frontmatter into a `ScriptKind`.
fn parse_script_kind(s: &str) -> Result<ScriptKind, String> {
    match s {
        "macro" => Ok(ScriptKind::Macro),
        "utility" => Ok(ScriptKind::Utility),
        "transform" => Ok(ScriptKind::Transform),
        other => Err(format!("unknown script kind: '{other}'")),
    }
}

/// Rebuild scripts from the filesystem into the database.
///
/// Walks the `scripts/` directory for `*.lua` and `*.py` files, parses each
/// via frontmatter, and either creates or updates the corresponding DB row.
/// Published scripts (those present on disk) have `content = None` in the DB.
pub fn rebuild_scripts_from_filesystem(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<ScriptRebuildReport> {
    let mut report = ScriptRebuildReport::default();
    let scripts_dir = data_dir.join("scripts");

    if !scripts_dir.exists() {
        return Ok(report);
    }

    for entry in WalkDir::new(&scripts_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str());
        if ext != Some("lua") && ext != Some("py") {
            continue;
        }

        match rebuild_single_script(conn, data_dir, project_id, path) {
            Ok(created) => {
                if created {
                    report.created += 1;
                } else {
                    report.updated += 1;
                }
            }
            Err(e) => {
                report.errors.push(format!("{}: {e}", path.display()));
            }
        }
    }

    Ok(report)
}

/// Rebuild a single script from a `.lua` or `.py` file.
/// Returns `true` if created, `false` if updated.
fn rebuild_single_script(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    path: &Path,
) -> EngineResult<bool> {
    let content = fs::read_to_string(path)?;
    let (fm, _body) = read_script_file(&content).map_err(EngineError::Parse)?;

    let rel_path = path
        .strip_prefix(data_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let kind = parse_script_kind(&fm.kind).map_err(EngineError::Parse)?;
    let now = now_unix_ms();

    // File exists on disk -> Published; content is None in DB
    let status = ScriptStatus::Published;

    let existing = qs::get_script_by_id(conn, &fm.id);
    match existing {
        Ok(mut script) => {
            script.slug = fm.slug;
            script.title = fm.title;
            script.kind = kind;
            script.entrypoint = fm.entrypoint;
            script.enabled = fm.enabled;
            script.version = fm.version;
            script.file_path = rel_path;
            script.status = status;
            script.content = None;
            script.created_at = fm.created_at;
            script.updated_at = now;
            qs::update_script(conn, &script)?;
            Ok(false)
        }
        Err(_) => {
            let script = Script {
                id: fm.id,
                project_id: project_id.to_string(),
                slug: fm.slug,
                title: fm.title,
                kind,
                entrypoint: fm.entrypoint,
                enabled: fm.enabled,
                version: fm.version,
                file_path: rel_path,
                status,
                content: None,
                created_at: fm.created_at,
                updated_at: now,
            };
            qs::insert_script(conn, &script)?;
            Ok(true)
        }
    }
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
    fn rebuild_creates_script_from_lua() {
        let (db, dir) = setup();
        let scripts_dir = dir.path().join("scripts");
        fs::create_dir_all(&scripts_dir).unwrap();

        let content = "\
---
id: \"lua-script-1\"
slug: \"my-macro\"
title: \"My Macro\"
kind: \"macro\"
entrypoint: \"render\"
enabled: true
version: 1
createdAt: \"2024-01-01T00:00:00.000Z\"
updatedAt: \"2024-01-01T00:00:00.000Z\"
---
function render()
  return \"<p>hello</p>\"
end
";
        fs::write(scripts_dir.join("my-macro.lua"), content).unwrap();

        let report =
            rebuild_scripts_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(report.created, 1);
        assert_eq!(report.updated, 0);
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);

        let script = qs::get_script_by_id(db.conn(), "lua-script-1").unwrap();
        assert_eq!(script.slug, "my-macro");
        assert_eq!(script.title, "My Macro");
        assert_eq!(script.kind, ScriptKind::Macro);
        assert_eq!(script.entrypoint, "render");
        assert!(script.enabled);
        assert_eq!(script.version, 1);
        assert_eq!(script.status, ScriptStatus::Published);
        assert!(script.content.is_none(), "published script should have content=None in DB");
        assert_eq!(script.file_path, "scripts/my-macro.lua");
    }

    #[test]
    fn rebuild_creates_script_from_py() {
        let (db, dir) = setup();
        let scripts_dir = dir.path().join("scripts");
        fs::create_dir_all(&scripts_dir).unwrap();

        let content = "\"\"\"\n\
---
id: \"py-script-1\"
slug: \"my-utility\"
title: \"My Utility\"
kind: \"utility\"
entrypoint: \"main\"
enabled: true
version: 1
createdAt: \"2024-01-01T00:00:00.000Z\"
updatedAt: \"2024-01-01T00:00:00.000Z\"
---
\"\"\"
def main():
    print(\"hello\")
";
        fs::write(scripts_dir.join("my-utility.py"), content).unwrap();

        let report =
            rebuild_scripts_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(report.created, 1);
        assert_eq!(report.updated, 0);
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);

        let script = qs::get_script_by_id(db.conn(), "py-script-1").unwrap();
        assert_eq!(script.slug, "my-utility");
        assert_eq!(script.title, "My Utility");
        assert_eq!(script.kind, ScriptKind::Utility);
        assert_eq!(script.entrypoint, "main");
        assert_eq!(script.status, ScriptStatus::Published);
        assert!(script.content.is_none());
        assert_eq!(script.file_path, "scripts/my-utility.py");
    }

    #[test]
    fn rebuild_updates_existing() {
        let (db, dir) = setup();

        // Insert a script in DB first
        let existing = Script {
            id: "existing-script".to_string(),
            project_id: "p1".to_string(),
            slug: "old-script".to_string(),
            title: "Old Script".to_string(),
            kind: ScriptKind::Macro,
            entrypoint: "render".to_string(),
            enabled: false,
            version: 1,
            file_path: "scripts/old-script.lua".to_string(),
            status: ScriptStatus::Draft,
            content: Some("old content".to_string()),
            created_at: 1000,
            updated_at: 2000,
        };
        qs::insert_script(db.conn(), &existing).unwrap();

        // Write updated file
        let scripts_dir = dir.path().join("scripts");
        fs::create_dir_all(&scripts_dir).unwrap();

        let content = "\
---
id: \"existing-script\"
slug: \"updated-script\"
title: \"Updated Script\"
kind: \"transform\"
entrypoint: \"process\"
enabled: true
version: 5
createdAt: \"2024-01-01T00:00:00.000Z\"
updatedAt: \"2024-01-01T00:00:00.000Z\"
---
function process()
  return \"updated\"
end
";
        fs::write(scripts_dir.join("updated-script.lua"), content).unwrap();

        let report =
            rebuild_scripts_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(report.created, 0);
        assert_eq!(report.updated, 1);
        assert!(report.errors.is_empty());

        let script = qs::get_script_by_id(db.conn(), "existing-script").unwrap();
        assert_eq!(script.slug, "updated-script");
        assert_eq!(script.title, "Updated Script");
        assert_eq!(script.kind, ScriptKind::Transform);
        assert_eq!(script.entrypoint, "process");
        assert!(script.enabled);
        assert_eq!(script.version, 5);
        assert_eq!(script.status, ScriptStatus::Published);
        assert!(script.content.is_none());
    }

    #[test]
    fn rebuild_idempotent() {
        let (db, dir) = setup();
        let scripts_dir = dir.path().join("scripts");
        fs::create_dir_all(&scripts_dir).unwrap();

        let content = "\
---
id: \"idem-script\"
slug: \"idem\"
title: \"Idempotent\"
kind: \"utility\"
entrypoint: \"run\"
enabled: true
version: 1
createdAt: \"2024-01-01T00:00:00.000Z\"
updatedAt: \"2024-01-01T00:00:00.000Z\"
---
function run() end
";
        fs::write(scripts_dir.join("idem.lua"), content).unwrap();

        // First run
        let r1 =
            rebuild_scripts_from_filesystem(db.conn(), dir.path(), "p1").unwrap();
        assert_eq!(r1.created, 1);
        assert_eq!(r1.updated, 0);

        // Second run - should update, not create
        let r2 =
            rebuild_scripts_from_filesystem(db.conn(), dir.path(), "p1").unwrap();
        assert_eq!(r2.created, 0);
        assert_eq!(r2.updated, 1);

        // Still only one script in DB
        let list = qs::list_scripts_by_project(db.conn(), "p1").unwrap();
        assert_eq!(list.len(), 1);
    }
}
