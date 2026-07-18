use rusqlite::{Connection, params};

use crate::db::from_row::{
    SCRIPT_COLUMNS, script_from_row, script_kind_to_str, script_status_to_str,
};
use crate::model::Script;

pub fn insert_script(conn: &Connection, s: &Script) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO scripts (
            id, project_id, slug, title, kind, entrypoint, enabled, version,
            file_path, status, content, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            s.id,
            s.project_id,
            s.slug,
            s.title,
            script_kind_to_str(&s.kind),
            s.entrypoint,
            s.enabled as i64,
            s.version,
            s.file_path,
            script_status_to_str(&s.status),
            s.content,
            s.created_at,
            s.updated_at,
        ],
    )?;
    Ok(())
}

pub fn get_script_by_id(conn: &Connection, id: &str) -> rusqlite::Result<Script> {
    conn.query_row(
        &format!("SELECT {SCRIPT_COLUMNS} FROM scripts WHERE id = ?1"),
        params![id],
        script_from_row,
    )
}

pub fn get_script_by_slug(
    conn: &Connection,
    project_id: &str,
    slug: &str,
) -> rusqlite::Result<Script> {
    conn.query_row(
        &format!("SELECT {SCRIPT_COLUMNS} FROM scripts WHERE project_id = ?1 AND slug = ?2"),
        params![project_id, slug],
        script_from_row,
    )
}

pub fn list_scripts_by_project(
    conn: &Connection,
    project_id: &str,
) -> rusqlite::Result<Vec<Script>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SCRIPT_COLUMNS} FROM scripts WHERE project_id = ?1 ORDER BY title"
    ))?;
    let rows = stmt.query_map(params![project_id], script_from_row)?;
    rows.collect()
}

pub fn update_script(conn: &Connection, s: &Script) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE scripts SET
            slug = ?1, title = ?2, kind = ?3, entrypoint = ?4, enabled = ?5,
            version = ?6, file_path = ?7, status = ?8, content = ?9, updated_at = ?10
         WHERE id = ?11",
        params![
            s.slug,
            s.title,
            script_kind_to_str(&s.kind),
            s.entrypoint,
            s.enabled as i64,
            s.version,
            s.file_path,
            script_status_to_str(&s.status),
            s.content,
            s.updated_at,
            s.id,
        ],
    )?;
    Ok(())
}

pub fn delete_script(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM scripts WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::model::{ScriptKind, ScriptStatus};

    fn setup() -> Database {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        db
    }

    fn make_script(id: &str, slug: &str) -> Script {
        Script {
            id: id.to_string(),
            project_id: "p1".to_string(),
            slug: slug.to_string(),
            title: format!("Script {slug}"),
            kind: ScriptKind::Macro,
            entrypoint: "render".to_string(),
            enabled: true,
            version: 1,
            file_path: format!("scripts/{slug}.lua"),
            status: ScriptStatus::Published,
            content: Some("return html".into()),
            created_at: 1000,
            updated_at: 2000,
        }
    }

    #[test]
    fn insert_and_get_by_id() {
        let db = setup();
        insert_script(db.conn(), &make_script("s1", "gallery")).unwrap();
        let s = get_script_by_id(db.conn(), "s1").unwrap();
        assert_eq!(s.slug, "gallery");
        assert_eq!(s.kind, ScriptKind::Macro);
        assert_eq!(s.entrypoint, "render");
        assert!(s.enabled);
    }

    #[test]
    fn get_by_slug() {
        let db = setup();
        insert_script(db.conn(), &make_script("s1", "gallery")).unwrap();
        let s = get_script_by_slug(db.conn(), "p1", "gallery").unwrap();
        assert_eq!(s.id, "s1");
    }

    #[test]
    fn list_by_project() {
        let db = setup();
        let mut s1 = make_script("s1", "zebra");
        s1.title = "Zebra".into();
        let mut s2 = make_script("s2", "alpha");
        s2.title = "Alpha".into();
        insert_script(db.conn(), &s1).unwrap();
        insert_script(db.conn(), &s2).unwrap();
        let list = list_scripts_by_project(db.conn(), "p1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].title, "Alpha");
    }

    #[test]
    fn update_script_fields() {
        let db = setup();
        let mut s = make_script("s1", "gallery");
        insert_script(db.conn(), &s).unwrap();
        s.kind = ScriptKind::Utility;
        s.enabled = false;
        s.version = 3;
        s.status = ScriptStatus::Draft;
        s.content = None;
        s.updated_at = 9999;
        update_script(db.conn(), &s).unwrap();
        let fetched = get_script_by_id(db.conn(), "s1").unwrap();
        assert_eq!(fetched.kind, ScriptKind::Utility);
        assert!(!fetched.enabled);
        assert_eq!(fetched.version, 3);
        assert_eq!(fetched.status, ScriptStatus::Draft);
        assert!(fetched.content.is_none());
    }

    #[test]
    fn delete_removes_script() {
        let db = setup();
        insert_script(db.conn(), &make_script("s1", "gallery")).unwrap();
        delete_script(db.conn(), "s1").unwrap();
        assert!(get_script_by_id(db.conn(), "s1").is_err());
    }

    #[test]
    fn duplicate_slug_rejected() {
        let db = setup();
        insert_script(db.conn(), &make_script("s1", "gallery")).unwrap();
        let result = insert_script(db.conn(), &make_script("s2", "gallery"));
        assert!(result.is_err());
    }
}
