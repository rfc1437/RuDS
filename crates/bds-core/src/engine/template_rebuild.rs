use std::fs;
use std::path::Path;

use crate::db::DbConnection as Connection;
use walkdir::WalkDir;

use crate::db::queries::template as qt;
use crate::engine::{EngineError, EngineResult};
use crate::model::{Template, TemplateKind, TemplateStatus};
use crate::util::frontmatter::read_template_file;
use crate::util::now_unix_ms;

/// Report returned by `rebuild_templates_from_filesystem`.
#[derive(Debug, Default)]
pub struct TemplateRebuildReport {
    pub created: usize,
    pub updated: usize,
    pub errors: Vec<String>,
}

/// Parse a template kind string from frontmatter into a `TemplateKind`.
fn parse_template_kind(s: &str) -> Result<TemplateKind, String> {
    match s {
        "post" => Ok(TemplateKind::Post),
        "list" => Ok(TemplateKind::List),
        "not_found" | "notFound" | "not-found" => Ok(TemplateKind::NotFound),
        "partial" => Ok(TemplateKind::Partial),
        other => Err(format!("unknown template kind: '{other}'")),
    }
}

/// Rebuild templates from the filesystem into the database.
///
/// Walks the `templates/` directory for `*.liquid` files, parses each via
/// frontmatter, and either creates or updates the corresponding DB row.
/// Published templates (those present on disk) have `content = None` in the DB.
pub fn rebuild_templates_from_filesystem(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<TemplateRebuildReport> {
    let mut report = TemplateRebuildReport::default();
    let templates_dir = data_dir.join("templates");

    if !templates_dir.exists() {
        return Ok(report);
    }

    for entry in WalkDir::new(&templates_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str());
        if ext != Some("liquid") {
            continue;
        }

        match rebuild_single_template(conn, data_dir, project_id, path) {
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

/// Rebuild a single template from a `.liquid` file.
/// Returns `true` if created, `false` if updated.
pub(crate) fn rebuild_single_template(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    path: &Path,
) -> EngineResult<bool> {
    let content = fs::read_to_string(path)?;
    let (fm, _body) = read_template_file(&content).map_err(EngineError::Parse)?;

    let rel_path = path
        .strip_prefix(data_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let kind = parse_template_kind(&fm.kind).map_err(EngineError::Parse)?;
    let now = now_unix_ms();

    // File exists on disk -> Published; content is None in DB
    let status = TemplateStatus::Published;

    let existing = qt::get_template_by_id(conn, &fm.id);
    match existing {
        Ok(mut tpl) => {
            tpl.slug = fm.slug;
            tpl.title = fm.title;
            tpl.kind = kind;
            tpl.enabled = fm.enabled;
            tpl.version = fm.version;
            tpl.file_path = rel_path;
            tpl.status = status;
            tpl.content = None;
            tpl.created_at = fm.created_at;
            tpl.updated_at = now;
            qt::update_template(conn, &tpl)?;
            Ok(false)
        }
        Err(_) => {
            let tpl = Template {
                id: fm.id,
                project_id: project_id.to_string(),
                slug: fm.slug,
                title: fm.title,
                kind,
                enabled: fm.enabled,
                version: fm.version,
                file_path: rel_path,
                status,
                content: None,
                created_at: fm.created_at,
                updated_at: now,
            };
            qt::insert_template(conn, &tpl)?;
            Ok(true)
        }
    }
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
    fn rebuild_creates_template() {
        let (db, dir) = setup();
        let tpl_dir = dir.path().join("templates");
        fs::create_dir_all(&tpl_dir).unwrap();

        let content = "\
---
id: \"aaa-bbb-ccc\"
slug: \"my-template\"
title: \"My Template\"
kind: \"post\"
enabled: true
version: 1
createdAt: \"2024-01-01T00:00:00.000Z\"
updatedAt: \"2024-01-01T00:00:00.000Z\"
---
<div>hello</div>
";
        fs::write(tpl_dir.join("my-template.liquid"), content).unwrap();

        let report = rebuild_templates_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(report.created, 1);
        assert_eq!(report.updated, 0);
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);

        let tpl = qt::get_template_by_id(db.conn(), "aaa-bbb-ccc").unwrap();
        assert_eq!(tpl.slug, "my-template");
        assert_eq!(tpl.title, "My Template");
        assert_eq!(tpl.kind, TemplateKind::Post);
        assert!(tpl.enabled);
        assert_eq!(tpl.version, 1);
        assert_eq!(tpl.status, TemplateStatus::Published);
        assert!(
            tpl.content.is_none(),
            "published template should have content=None in DB"
        );
        assert_eq!(tpl.file_path, "templates/my-template.liquid");
    }

    #[test]
    fn rebuild_updates_existing() {
        let (db, dir) = setup();

        // Insert a template in DB first
        let existing = Template {
            id: "existing-id".to_string(),
            project_id: "p1".to_string(),
            slug: "old-slug".to_string(),
            title: "Old Title".to_string(),
            kind: TemplateKind::Post,
            enabled: false,
            version: 1,
            file_path: "templates/old-slug.liquid".to_string(),
            status: TemplateStatus::Draft,
            content: Some("<p>old</p>".to_string()),
            created_at: 1000,
            updated_at: 2000,
        };
        qt::insert_template(db.conn(), &existing).unwrap();

        // Write updated file
        let tpl_dir = dir.path().join("templates");
        fs::create_dir_all(&tpl_dir).unwrap();

        let content = "\
---
id: \"existing-id\"
slug: \"updated-slug\"
title: \"Updated Title\"
kind: \"list\"
enabled: true
version: 3
createdAt: \"2024-01-01T00:00:00.000Z\"
updatedAt: \"2024-01-01T00:00:00.000Z\"
---
<div>updated body</div>
";
        fs::write(tpl_dir.join("updated-slug.liquid"), content).unwrap();

        let report = rebuild_templates_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(report.created, 0);
        assert_eq!(report.updated, 1);
        assert!(report.errors.is_empty());

        let tpl = qt::get_template_by_id(db.conn(), "existing-id").unwrap();
        assert_eq!(tpl.slug, "updated-slug");
        assert_eq!(tpl.title, "Updated Title");
        assert_eq!(tpl.kind, TemplateKind::List);
        assert!(tpl.enabled);
        assert_eq!(tpl.version, 3);
        assert_eq!(tpl.status, TemplateStatus::Published);
        assert!(tpl.content.is_none());
    }

    #[test]
    fn rebuild_ignores_non_liquid_files() {
        let (db, dir) = setup();
        let tpl_dir = dir.path().join("templates");
        fs::create_dir_all(&tpl_dir).unwrap();

        fs::write(tpl_dir.join("readme.txt"), "not a template").unwrap();
        fs::write(tpl_dir.join("styles.css"), "body {}").unwrap();

        let report = rebuild_templates_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(report.created, 0);
        assert_eq!(report.updated, 0);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn rebuild_idempotent() {
        let (db, dir) = setup();
        let tpl_dir = dir.path().join("templates");
        fs::create_dir_all(&tpl_dir).unwrap();

        let content = "\
---
id: \"idem-id\"
slug: \"idem\"
title: \"Idempotent\"
kind: \"partial\"
enabled: true
version: 1
createdAt: \"2024-01-01T00:00:00.000Z\"
updatedAt: \"2024-01-01T00:00:00.000Z\"
---
<span>partial</span>
";
        fs::write(tpl_dir.join("idem.liquid"), content).unwrap();

        // First run
        let r1 = rebuild_templates_from_filesystem(db.conn(), dir.path(), "p1").unwrap();
        assert_eq!(r1.created, 1);
        assert_eq!(r1.updated, 0);

        // Second run - should update, not create
        let r2 = rebuild_templates_from_filesystem(db.conn(), dir.path(), "p1").unwrap();
        assert_eq!(r2.created, 0);
        assert_eq!(r2.updated, 1);

        // Still only one template in DB
        let list = qt::list_templates_by_project(db.conn(), "p1").unwrap();
        assert_eq!(list.len(), 1);
    }
}
