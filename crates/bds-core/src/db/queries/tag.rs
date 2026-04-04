use rusqlite::{params, Connection};

use crate::db::from_row::{tag_from_row, TAG_COLUMNS};
use crate::model::Tag;

pub fn insert_tag(conn: &Connection, tag: &Tag) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO tags (id, project_id, name, color, post_template_slug, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            tag.id,
            tag.project_id,
            tag.name,
            tag.color,
            tag.post_template_slug,
            tag.created_at,
            tag.updated_at,
        ],
    )?;
    Ok(())
}

pub fn get_tag_by_id(conn: &Connection, id: &str) -> rusqlite::Result<Tag> {
    conn.query_row(
        &format!("SELECT {TAG_COLUMNS} FROM tags WHERE id = ?1"),
        params![id],
        tag_from_row,
    )
}

pub fn get_tag_by_project_and_name(
    conn: &Connection,
    project_id: &str,
    name: &str,
) -> rusqlite::Result<Tag> {
    conn.query_row(
        &format!(
            "SELECT {TAG_COLUMNS} FROM tags WHERE project_id = ?1 AND LOWER(name) = LOWER(?2)"
        ),
        params![project_id, name],
        tag_from_row,
    )
}

pub fn list_tags_by_project(conn: &Connection, project_id: &str) -> rusqlite::Result<Vec<Tag>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {TAG_COLUMNS} FROM tags WHERE project_id = ?1 ORDER BY name"
    ))?;
    let rows = stmt.query_map(params![project_id], tag_from_row)?;
    rows.collect()
}

pub fn update_tag(conn: &Connection, tag: &Tag) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE tags SET name = ?1, color = ?2, post_template_slug = ?3, updated_at = ?4
         WHERE id = ?5",
        params![
            tag.name,
            tag.color,
            tag.post_template_slug,
            tag.updated_at,
            tag.id,
        ],
    )?;
    Ok(())
}

pub fn delete_tag(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::db::Database;

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
