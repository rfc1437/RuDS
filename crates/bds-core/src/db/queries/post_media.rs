use rusqlite::{Connection, params};

use crate::db::from_row::{POST_MEDIA_COLUMNS, post_media_from_row};
use crate::model::PostMedia;

pub fn link_media(conn: &Connection, pm: &PostMedia) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO post_media (id, project_id, post_id, media_id, sort_order, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            pm.id,
            pm.project_id,
            pm.post_id,
            pm.media_id,
            pm.sort_order,
            pm.created_at,
        ],
    )?;
    Ok(())
}

pub fn unlink_media(conn: &Connection, post_id: &str, media_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM post_media WHERE post_id = ?1 AND media_id = ?2",
        params![post_id, media_id],
    )?;
    Ok(())
}

pub fn list_post_media_by_post(
    conn: &Connection,
    post_id: &str,
) -> rusqlite::Result<Vec<PostMedia>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {POST_MEDIA_COLUMNS} FROM post_media WHERE post_id = ?1 ORDER BY sort_order"
    ))?;
    let rows = stmt.query_map(params![post_id], post_media_from_row)?;
    rows.collect()
}

pub fn list_post_media_by_media(
    conn: &Connection,
    media_id: &str,
) -> rusqlite::Result<Vec<PostMedia>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {POST_MEDIA_COLUMNS} FROM post_media WHERE media_id = ?1 ORDER BY created_at"
    ))?;
    let rows = stmt.query_map(params![media_id], post_media_from_row)?;
    rows.collect()
}

pub fn update_sort_order(
    conn: &Connection,
    post_id: &str,
    media_id: &str,
    sort_order: i32,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE post_media SET sort_order = ?1 WHERE post_id = ?2 AND media_id = ?3",
        params![sort_order, post_id, media_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::media::{insert_media, make_test_media};
    use crate::db::queries::project::{insert_project, make_test_project};

    fn setup() -> Database {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        db.conn()
            .execute(
                "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
                 VALUES ('post1', 'p1', 'Hello', 'hello', 'draft', 1000, 1000)",
                [],
            )
            .unwrap();
        insert_media(db.conn(), &make_test_media("m1", "p1")).unwrap();
        insert_media(db.conn(), &make_test_media("m2", "p1")).unwrap();
        db
    }

    fn make_pm(id: &str, media_id: &str, order: i32) -> PostMedia {
        PostMedia {
            id: id.to_string(),
            project_id: "p1".to_string(),
            post_id: "post1".to_string(),
            media_id: media_id.to_string(),
            sort_order: order,
            created_at: 1000,
        }
    }

    #[test]
    fn link_and_list_by_post() {
        let db = setup();
        link_media(db.conn(), &make_pm("pm1", "m1", 1)).unwrap();
        link_media(db.conn(), &make_pm("pm2", "m2", 0)).unwrap();
        let list = list_post_media_by_post(db.conn(), "post1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].media_id, "m2"); // sort_order 0 first
        assert_eq!(list[1].media_id, "m1");
    }

    #[test]
    fn list_by_media() {
        let db = setup();
        link_media(db.conn(), &make_pm("pm1", "m1", 0)).unwrap();
        let list = list_post_media_by_media(db.conn(), "m1").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].post_id, "post1");
    }

    #[test]
    fn unlink_removes_association() {
        let db = setup();
        link_media(db.conn(), &make_pm("pm1", "m1", 0)).unwrap();
        unlink_media(db.conn(), "post1", "m1").unwrap();
        let list = list_post_media_by_post(db.conn(), "post1").unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn update_sort_order_changes_value() {
        let db = setup();
        link_media(db.conn(), &make_pm("pm1", "m1", 0)).unwrap();
        update_sort_order(db.conn(), "post1", "m1", 10).unwrap();
        let list = list_post_media_by_post(db.conn(), "post1").unwrap();
        assert_eq!(list[0].sort_order, 10);
    }

    #[test]
    fn duplicate_post_media_rejected() {
        let db = setup();
        link_media(db.conn(), &make_pm("pm1", "m1", 0)).unwrap();
        let result = link_media(db.conn(), &make_pm("pm2", "m1", 1));
        assert!(result.is_err());
    }
}
