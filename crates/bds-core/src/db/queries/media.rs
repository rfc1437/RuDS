use rusqlite::{params, Connection};

use crate::db::from_row::{media_from_row, MEDIA_COLUMNS};
use crate::model::Media;

fn tags_to_json(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".into())
}

pub fn insert_media(conn: &Connection, m: &Media) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO media (
            id, project_id, filename, original_name, mime_type, size,
            width, height, title, alt, caption, author, language,
            file_path, sidecar_path, checksum, tags, created_at, updated_at
         ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6,
            ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?19
         )",
        params![
            m.id,
            m.project_id,
            m.filename,
            m.original_name,
            m.mime_type,
            m.size,
            m.width,
            m.height,
            m.title,
            m.alt,
            m.caption,
            m.author,
            m.language,
            m.file_path,
            m.sidecar_path,
            m.checksum,
            tags_to_json(&m.tags),
            m.created_at,
            m.updated_at,
        ],
    )?;
    Ok(())
}

pub fn get_media_by_id(conn: &Connection, id: &str) -> rusqlite::Result<Media> {
    conn.query_row(
        &format!("SELECT {MEDIA_COLUMNS} FROM media WHERE id = ?1"),
        params![id],
        media_from_row,
    )
}

pub fn list_media_by_project(conn: &Connection, project_id: &str) -> rusqlite::Result<Vec<Media>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEDIA_COLUMNS} FROM media WHERE project_id = ?1 ORDER BY created_at DESC"
    ))?;
    let rows = stmt.query_map(params![project_id], media_from_row)?;
    rows.collect()
}

pub fn update_media(conn: &Connection, m: &Media) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE media SET
            filename = ?1, original_name = ?2, mime_type = ?3, size = ?4,
            width = ?5, height = ?6, title = ?7, alt = ?8, caption = ?9,
            author = ?10, language = ?11, file_path = ?12, sidecar_path = ?13,
            checksum = ?14, tags = ?15, updated_at = ?16
         WHERE id = ?17",
        params![
            m.filename,
            m.original_name,
            m.mime_type,
            m.size,
            m.width,
            m.height,
            m.title,
            m.alt,
            m.caption,
            m.author,
            m.language,
            m.file_path,
            m.sidecar_path,
            m.checksum,
            tags_to_json(&m.tags),
            m.updated_at,
            m.id,
        ],
    )?;
    Ok(())
}

pub fn delete_media(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM media WHERE id = ?1", params![id])?;
    Ok(())
}

/// Test helper: create a minimal Media value (available to sibling test modules).
#[cfg(test)]
pub fn make_test_media(id: &str, project_id: &str) -> Media {
    Media {
        id: id.to_string(),
        project_id: project_id.to_string(),
        filename: format!("{id}.jpg"),
        original_name: "photo.jpg".to_string(),
        mime_type: "image/jpeg".to_string(),
        size: 50000,
        width: Some(1920),
        height: Some(1080),
        title: None,
        alt: None,
        caption: None,
        author: None,
        language: None,
        file_path: format!("/media/{id}.jpg"),
        sidecar_path: format!("/media/{id}.jpg.meta"),
        checksum: None,
        tags: vec![],
        created_at: 1000,
        updated_at: 2000,
    }
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

    fn make_media_item(id: &str) -> Media {
        Media {
            id: id.to_string(),
            project_id: "p1".to_string(),
            filename: "abc.jpg".to_string(),
            original_name: "photo.jpg".to_string(),
            mime_type: "image/jpeg".to_string(),
            size: 50000,
            width: Some(1920),
            height: Some(1080),
            title: Some("Sunset".into()),
            alt: Some("A sunset".into()),
            caption: None,
            author: Some("Bob".into()),
            language: Some("en".into()),
            file_path: "/media/abc.jpg".to_string(),
            sidecar_path: "/media/abc.jpg.meta".to_string(),
            checksum: Some("hash1".into()),
            tags: vec!["nature".into()],
            created_at: 1000,
            updated_at: 2000,
        }
    }

    #[test]
    fn insert_and_get_by_id() {
        let db = setup();
        let m = make_media_item("m1");
        insert_media(db.conn(), &m).unwrap();
        let fetched = get_media_by_id(db.conn(), "m1").unwrap();
        assert_eq!(fetched.original_name, "photo.jpg");
        assert_eq!(fetched.width, Some(1920));
        assert_eq!(fetched.height, Some(1080));
        assert_eq!(fetched.tags, vec!["nature"]);
        assert_eq!(fetched.size, 50000);
    }

    #[test]
    fn list_by_project() {
        let db = setup();
        let mut m1 = make_media_item("m1");
        m1.created_at = 1000;
        let mut m2 = make_media_item("m2");
        m2.filename = "def.jpg".into();
        m2.created_at = 2000;
        insert_media(db.conn(), &m1).unwrap();
        insert_media(db.conn(), &m2).unwrap();
        let list = list_media_by_project(db.conn(), "p1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "m2");
    }

    #[test]
    fn update_media_fields() {
        let db = setup();
        let mut m = make_media_item("m1");
        insert_media(db.conn(), &m).unwrap();
        m.title = Some("New Title".into());
        m.tags = vec!["updated".into()];
        m.updated_at = 9999;
        update_media(db.conn(), &m).unwrap();
        let fetched = get_media_by_id(db.conn(), "m1").unwrap();
        assert_eq!(fetched.title.as_deref(), Some("New Title"));
        assert_eq!(fetched.tags, vec!["updated"]);
    }

    #[test]
    fn delete_removes_media() {
        let db = setup();
        insert_media(db.conn(), &make_media_item("m1")).unwrap();
        delete_media(db.conn(), "m1").unwrap();
        assert!(get_media_by_id(db.conn(), "m1").is_err());
    }
}
