use diesel::prelude::*;
use diesel::sql_types::Text;

use crate::db::DbConnection;
use crate::db::from_row::TagRecord;
use crate::db::schema::tags;
use crate::model::Tag;

diesel::define_sql_function!(fn lower(value: Text) -> Text);

pub fn insert_tag(conn: &DbConnection, tag: &Tag) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(tags::table)
            .values(TagRecord::from(tag))
            .execute(c)
            .map(|_| ())
    })
}

pub fn get_tag_by_id(conn: &DbConnection, id: &str) -> QueryResult<Tag> {
    conn.with(|c| {
        tags::table
            .filter(tags::id.eq(id))
            .select(TagRecord::as_select())
            .first(c)
            .map(Into::into)
    })
}

pub fn get_tag_by_project_and_name(
    conn: &DbConnection,
    project_id: &str,
    name: &str,
) -> QueryResult<Tag> {
    conn.with(|c| {
        tags::table
            .filter(tags::project_id.eq(project_id))
            .filter(lower(tags::name).eq(name.to_lowercase()))
            .select(TagRecord::as_select())
            .first(c)
            .map(Into::into)
    })
}

pub fn list_tags_by_project(conn: &DbConnection, project_id: &str) -> QueryResult<Vec<Tag>> {
    conn.with(|c| {
        tags::table
            .filter(tags::project_id.eq(project_id))
            .order(tags::name)
            .select(TagRecord::as_select())
            .load(c)
            .map(|rows: Vec<TagRecord>| rows.into_iter().map(Into::into).collect())
    })
}

pub fn update_tag(conn: &DbConnection, tag: &Tag) -> QueryResult<()> {
    conn.with(|c| {
        diesel::update(tags::table.filter(tags::id.eq(&tag.id)))
            .set(TagRecord::from(tag))
            .execute(c)
            .map(|_| ())
    })
}

pub fn delete_tag(conn: &DbConnection, id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(tags::table.filter(tags::id.eq(id)))
            .execute(c)
            .map(|_| ())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::project::{insert_project, make_test_project};

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        db
    }

    fn make_tag(id: &str, name: &str) -> Tag {
        Tag {
            id: id.to_string(),
            project_id: "p1".to_string(),
            name: name.to_string(),
            color: Some("#ff0000".into()),
            post_template_slug: None,
            created_at: 1000,
            updated_at: 2000,
        }
    }

    #[test]
    fn insert_and_get_by_id() {
        let db = setup();
        insert_tag(db.conn(), &make_tag("t1", "rust")).unwrap();
        let tag = get_tag_by_id(db.conn(), "t1").unwrap();
        assert_eq!(tag.name, "rust");
        assert_eq!(tag.color.as_deref(), Some("#ff0000"));
    }

    #[test]
    fn get_by_project_and_name_case_insensitive() {
        let db = setup();
        insert_tag(db.conn(), &make_tag("t1", "Rust")).unwrap();
        let tag = get_tag_by_project_and_name(db.conn(), "p1", "rust").unwrap();
        assert_eq!(tag.id, "t1");
        let tag = get_tag_by_project_and_name(db.conn(), "p1", "RUST").unwrap();
        assert_eq!(tag.id, "t1");
    }

    #[test]
    fn list_ordered_by_name() {
        let db = setup();
        insert_tag(db.conn(), &make_tag("t1", "zebra")).unwrap();
        insert_tag(db.conn(), &make_tag("t2", "alpha")).unwrap();
        let list = list_tags_by_project(db.conn(), "p1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "alpha");
        assert_eq!(list[1].name, "zebra");
    }

    #[test]
    fn update_tag_fields() {
        let db = setup();
        let mut tag = make_tag("t1", "rust");
        insert_tag(db.conn(), &tag).unwrap();
        tag.name = "go".into();
        tag.color = None;
        tag.updated_at = 9999;
        update_tag(db.conn(), &tag).unwrap();
        let fetched = get_tag_by_id(db.conn(), "t1").unwrap();
        assert_eq!(fetched.name, "go");
        assert!(fetched.color.is_none());
    }

    #[test]
    fn delete_tag_removes_row() {
        let db = setup();
        insert_tag(db.conn(), &make_tag("t1", "rust")).unwrap();
        delete_tag(db.conn(), "t1").unwrap();
        assert!(get_tag_by_id(db.conn(), "t1").is_err());
    }

    #[test]
    fn duplicate_name_rejected() {
        let db = setup();
        insert_tag(db.conn(), &make_tag("t1", "rust")).unwrap();
        let result = insert_tag(db.conn(), &make_tag("t2", "rust"));
        assert!(result.is_err());
    }
}
