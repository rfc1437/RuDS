use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::from_row::{TemplateRecord, convert, convert_all};
use crate::db::schema::templates;
use crate::model::Template;

pub fn insert_template(conn: &DbConnection, t: &Template) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(templates::table)
            .values(TemplateRecord::from(t))
            .execute(c)
            .map(|_| ())
    })
}

pub fn get_template_by_id(conn: &DbConnection, id: &str) -> QueryResult<Template> {
    conn.with(|c| {
        templates::table
            .filter(templates::id.eq(id))
            .select(TemplateRecord::as_select())
            .first(c)
            .and_then(convert)
    })
}

pub fn get_template_by_slug(
    conn: &DbConnection,
    project_id: &str,
    slug: &str,
) -> QueryResult<Template> {
    conn.with(|c| {
        templates::table
            .filter(templates::project_id.eq(project_id))
            .filter(templates::slug.eq(slug))
            .select(TemplateRecord::as_select())
            .first(c)
            .and_then(convert)
    })
}

pub fn list_templates_by_project(
    conn: &DbConnection,
    project_id: &str,
) -> QueryResult<Vec<Template>> {
    conn.with(|c| {
        templates::table
            .filter(templates::project_id.eq(project_id))
            .order(templates::title)
            .select(TemplateRecord::as_select())
            .load(c)
            .and_then(convert_all)
    })
}

pub fn update_template(conn: &DbConnection, t: &Template) -> QueryResult<()> {
    conn.with(|c| {
        diesel::update(templates::table.filter(templates::id.eq(&t.id)))
            .set(TemplateRecord::from(t))
            .execute(c)
            .map(|_| ())
    })
}

pub fn delete_template(conn: &DbConnection, id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(templates::table.filter(templates::id.eq(id)))
            .execute(c)
            .map(|_| ())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::project::{insert_project, make_test_project};
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
