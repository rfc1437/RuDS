use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::schema::scripts;
use crate::model::Script;

pub fn insert_script(conn: &DbConnection, s: &Script) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(scripts::table)
            .values(s.clone())
            .execute(c)
            .map(|_| ())
    })
}

pub fn get_script_by_id(conn: &DbConnection, id: &str) -> QueryResult<Script> {
    conn.with(|c| {
        scripts::table
            .filter(scripts::id.eq(id))
            .select(Script::as_select())
            .first(c)
    })
}

pub fn get_script_by_slug(
    conn: &DbConnection,
    project_id: &str,
    slug: &str,
) -> QueryResult<Script> {
    conn.with(|c| {
        scripts::table
            .filter(scripts::project_id.eq(project_id))
            .filter(scripts::slug.eq(slug))
            .select(Script::as_select())
            .first(c)
    })
}

pub fn list_scripts_by_project(conn: &DbConnection, project_id: &str) -> QueryResult<Vec<Script>> {
    conn.with(|c| {
        scripts::table
            .filter(scripts::project_id.eq(project_id))
            .order(scripts::title)
            .select(Script::as_select())
            .load(c)
    })
}

pub fn update_script(conn: &DbConnection, s: &Script) -> QueryResult<()> {
    conn.with(|c| {
        diesel::update(scripts::table.filter(scripts::id.eq(&s.id)))
            .set(s.clone())
            .execute(c)
            .map(|_| ())
    })
}

pub fn delete_script(conn: &DbConnection, id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(scripts::table.filter(scripts::id.eq(id)))
            .execute(c)
            .map(|_| ())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::model::{ScriptKind, ScriptStatus};

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
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
