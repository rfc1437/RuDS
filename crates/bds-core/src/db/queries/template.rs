use rusqlite::{params, Connection};

use crate::db::from_row::{
    template_from_row, template_kind_to_str, template_status_to_str, TEMPLATE_COLUMNS,
};
use crate::model::Template;

pub fn insert_template(conn: &Connection, t: &Template) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO templates (
            id, project_id, slug, title, kind, enabled, version,
            file_path, status, content, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            t.id,
            t.project_id,
            t.slug,
            t.title,
            template_kind_to_str(&t.kind),
            t.enabled as i64,
            t.version,
            t.file_path,
            template_status_to_str(&t.status),
            t.content,
            t.created_at,
            t.updated_at,
        ],
    )?;
    Ok(())
}

pub fn get_template_by_id(conn: &Connection, id: &str) -> rusqlite::Result<Template> {
    conn.query_row(
        &format!("SELECT {TEMPLATE_COLUMNS} FROM templates WHERE id = ?1"),
        params![id],
        template_from_row,
    )
}

pub fn get_template_by_slug(
    conn: &Connection,
    project_id: &str,
    slug: &str,
) -> rusqlite::Result<Template> {
    conn.query_row(
        &format!(
            "SELECT {TEMPLATE_COLUMNS} FROM templates WHERE project_id = ?1 AND slug = ?2"
        ),
        params![project_id, slug],
        template_from_row,
    )
}

pub fn list_templates_by_project(
    conn: &Connection,
    project_id: &str,
) -> rusqlite::Result<Vec<Template>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TEMPLATE_COLUMNS} FROM templates WHERE project_id = ?1 ORDER BY title"
    ))?;
    let rows = stmt.query_map(params![project_id], template_from_row)?;
    rows.collect()
}

pub fn update_template(conn: &Connection, t: &Template) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE templates SET
            slug = ?1, title = ?2, kind = ?3, enabled = ?4, version = ?5,
            file_path = ?6, status = ?7, content = ?8, updated_at = ?9
         WHERE id = ?10",
        params![
            t.slug,
            t.title,
            template_kind_to_str(&t.kind),
            t.enabled as i64,
            t.version,
            t.file_path,
            template_status_to_str(&t.status),
            t.content,
            t.updated_at,
            t.id,
        ],
    )?;
    Ok(())
}

pub fn delete_template(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM templates WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::db::Database;
    use crate::model::{TemplateKind, TemplateStatus};

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        db
    }

    fn make_tpl(id: &str, slug: &str) -> Template {
        Template {
            id: id.to_string(),
            project_id: "p1".to_string(),
            slug: slug.to_string(),
            title: format!("Template {slug}"),
            kind: TemplateKind::Post,
            enabled: true,
            version: 1,
            file_path: format!("templates/{slug}.liquid"),
            status: TemplateStatus::Published,
            content: Some("html".into()),
            created_at: 1000,
            updated_at: 2000,
        }
    }

    #[test]
    fn insert_and_get_by_id() {
        let db = setup();
        insert_template(db.conn(), &make_tpl("t1", "default")).unwrap();
        let t = get_template_by_id(db.conn(), "t1").unwrap();
        assert_eq!(t.slug, "default");
        assert_eq!(t.kind, TemplateKind::Post);
        assert!(t.enabled);
    }

    #[test]
    fn get_by_slug() {
        let db = setup();
        insert_template(db.conn(), &make_tpl("t1", "default")).unwrap();
        let t = get_template_by_slug(db.conn(), "p1", "default").unwrap();
        assert_eq!(t.id, "t1");
    }

    #[test]
    fn list_by_project() {
        let db = setup();
        let mut t1 = make_tpl("t1", "zebra");
        t1.title = "Zebra".into();
        let mut t2 = make_tpl("t2", "alpha");
        t2.title = "Alpha".into();
        insert_template(db.conn(), &t1).unwrap();
        insert_template(db.conn(), &t2).unwrap();
        let list = list_templates_by_project(db.conn(), "p1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].title, "Alpha");
    }

    #[test]
    fn update_template_fields() {
        let db = setup();
        let mut t = make_tpl("t1", "default");
        insert_template(db.conn(), &t).unwrap();
        t.kind = TemplateKind::List;
        t.enabled = false;
        t.version = 5;
        t.status = TemplateStatus::Draft;
        t.content = None;
        t.updated_at = 9999;
        update_template(db.conn(), &t).unwrap();
        let fetched = get_template_by_id(db.conn(), "t1").unwrap();
        assert_eq!(fetched.kind, TemplateKind::List);
        assert!(!fetched.enabled);
        assert_eq!(fetched.version, 5);
        assert_eq!(fetched.status, TemplateStatus::Draft);
        assert!(fetched.content.is_none());
    }

    #[test]
    fn delete_removes_template() {
        let db = setup();
        insert_template(db.conn(), &make_tpl("t1", "default")).unwrap();
        delete_template(db.conn(), "t1").unwrap();
        assert!(get_template_by_id(db.conn(), "t1").is_err());
    }

    #[test]
    fn duplicate_slug_rejected() {
        let db = setup();
        insert_template(db.conn(), &make_tpl("t1", "default")).unwrap();
        let result = insert_template(db.conn(), &make_tpl("t2", "default"));
        assert!(result.is_err());
    }
}
