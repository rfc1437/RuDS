use rusqlite::{Connection, params};

use crate::db::from_row::{POST_LINK_COLUMNS, post_link_from_row};
use crate::model::PostLink;

pub fn insert_post_link(conn: &Connection, link: &PostLink) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO post_links (id, source_post_id, target_post_id, link_text, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            link.id,
            link.source_post_id,
            link.target_post_id,
            link.link_text,
            link.created_at,
        ],
    )?;
    Ok(())
}

pub fn delete_links_by_source(conn: &Connection, source_post_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM post_links WHERE source_post_id = ?1",
        params![source_post_id],
    )?;
    Ok(())
}

pub fn list_links_by_source(
    conn: &Connection,
    source_post_id: &str,
) -> rusqlite::Result<Vec<PostLink>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {POST_LINK_COLUMNS} FROM post_links WHERE source_post_id = ?1 ORDER BY created_at"
    ))?;
    let rows = stmt.query_map(params![source_post_id], post_link_from_row)?;
    rows.collect()
}

pub fn list_links_by_target(
    conn: &Connection,
    target_post_id: &str,
) -> rusqlite::Result<Vec<PostLink>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {POST_LINK_COLUMNS} FROM post_links WHERE target_post_id = ?1 ORDER BY created_at"
    ))?;
    let rows = stmt.query_map(params![target_post_id], post_link_from_row)?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::project::{insert_project, make_test_project};

    fn setup() -> Database {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let c = db.conn();
        c.execute(
            "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
             VALUES ('a', 'p1', 'A', 'a', 'draft', 1000, 1000)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
             VALUES ('b', 'p1', 'B', 'b', 'draft', 1000, 1000)",
            [],
        )
        .unwrap();
        c.execute(
            "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
             VALUES ('c', 'p1', 'C', 'c', 'draft', 1000, 1000)",
            [],
        )
        .unwrap();
        db
    }

    fn make_link(id: &str, src: &str, tgt: &str) -> PostLink {
        PostLink {
            id: id.to_string(),
            source_post_id: src.to_string(),
            target_post_id: tgt.to_string(),
            link_text: Some("see also".into()),
            created_at: 1000,
        }
    }

    #[test]
    fn insert_and_list_by_source() {
        let db = setup();
        insert_post_link(db.conn(), &make_link("l1", "a", "b")).unwrap();
        insert_post_link(db.conn(), &make_link("l2", "a", "c")).unwrap();
        let links = list_links_by_source(db.conn(), "a").unwrap();
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn list_by_target() {
        let db = setup();
        insert_post_link(db.conn(), &make_link("l1", "a", "c")).unwrap();
        insert_post_link(db.conn(), &make_link("l2", "b", "c")).unwrap();
        let links = list_links_by_target(db.conn(), "c").unwrap();
        assert_eq!(links.len(), 2);
    }

    #[test]
    fn delete_by_source() {
        let db = setup();
        insert_post_link(db.conn(), &make_link("l1", "a", "b")).unwrap();
        insert_post_link(db.conn(), &make_link("l2", "a", "c")).unwrap();
        delete_links_by_source(db.conn(), "a").unwrap();
        let links = list_links_by_source(db.conn(), "a").unwrap();
        assert!(links.is_empty());
    }
}
