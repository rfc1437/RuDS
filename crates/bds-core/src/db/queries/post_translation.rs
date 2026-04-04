use rusqlite::{params, Connection};

use crate::db::from_row::{post_status_to_str, post_translation_from_row, POST_TRANSLATION_COLUMNS};
use crate::model::PostTranslation;

pub fn insert_post_translation(
    conn: &Connection,
    t: &PostTranslation,
) -> rusqlite::Result<()> {
    if !t.status.is_valid_for_translation() {
        return Err(rusqlite::Error::InvalidParameterName(
            "translation status must be draft or published".to_string(),
        ));
    }
    conn.execute(
        "INSERT INTO post_translations (
            id, project_id, translation_for, language, title, excerpt, content,
            status, file_path, checksum, created_at, updated_at, published_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            t.id,
            t.project_id,
            t.translation_for,
            t.language,
            t.title,
            t.excerpt,
            t.content,
            post_status_to_str(&t.status),
            t.file_path,
            t.checksum,
            t.created_at,
            t.updated_at,
            t.published_at,
        ],
    )?;
    Ok(())
}

pub fn get_post_translation_by_id(
    conn: &Connection,
    id: &str,
) -> rusqlite::Result<PostTranslation> {
    conn.query_row(
        &format!("SELECT {POST_TRANSLATION_COLUMNS} FROM post_translations WHERE id = ?1"),
        params![id],
        post_translation_from_row,
    )
}

pub fn get_post_translation_by_post_and_language(
    conn: &Connection,
    translation_for: &str,
    language: &str,
) -> rusqlite::Result<PostTranslation> {
    conn.query_row(
        &format!(
            "SELECT {POST_TRANSLATION_COLUMNS} FROM post_translations
             WHERE translation_for = ?1 AND language = ?2"
        ),
        params![translation_for, language],
        post_translation_from_row,
    )
}

pub fn list_post_translations_by_post(
    conn: &Connection,
    translation_for: &str,
) -> rusqlite::Result<Vec<PostTranslation>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {POST_TRANSLATION_COLUMNS} FROM post_translations
         WHERE translation_for = ?1 ORDER BY language"
    ))?;
    let rows = stmt.query_map(params![translation_for], post_translation_from_row)?;
    rows.collect()
}

pub fn update_post_translation(
    conn: &Connection,
    t: &PostTranslation,
) -> rusqlite::Result<()> {
    if !t.status.is_valid_for_translation() {
        return Err(rusqlite::Error::InvalidParameterName(
            "translation status must be draft or published".to_string(),
        ));
    }
    conn.execute(
        "UPDATE post_translations SET
            title = ?1, excerpt = ?2, content = ?3, status = ?4,
            file_path = ?5, checksum = ?6, updated_at = ?7, published_at = ?8
         WHERE id = ?9",
        params![
            t.title,
            t.excerpt,
            t.content,
            post_status_to_str(&t.status),
            t.file_path,
            t.checksum,
            t.updated_at,
            t.published_at,
            t.id,
        ],
    )?;
    Ok(())
}

pub fn delete_post_translation(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM post_translations WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn delete_all_translations_for_post(
    conn: &Connection,
    translation_for: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM post_translations WHERE translation_for = ?1",
        params![translation_for],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::db::Database;
    use crate::model::PostStatus;

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        // insert a parent post
        db.conn()
            .execute(
                "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
                 VALUES ('post1', 'p1', 'Hello', 'hello', 'draft', 1000, 1000)",
                [],
            )
            .unwrap();
        db
    }

    fn make_translation(id: &str, lang: &str) -> PostTranslation {
        PostTranslation {
            id: id.to_string(),
            project_id: "p1".to_string(),
            translation_for: "post1".to_string(),
            language: lang.to_string(),
            title: format!("Title {lang}"),
            excerpt: Some("excerpt".into()),
            content: Some("body".into()),
            status: PostStatus::Draft,
            file_path: format!("posts/hello.{lang}.md"),
            checksum: None,
            created_at: 1000,
            updated_at: 2000,
            published_at: None,
        }
    }

    #[test]
    fn insert_and_get_by_id() {
        let db = setup();
        insert_post_translation(db.conn(), &make_translation("t1", "de")).unwrap();
        let t = get_post_translation_by_id(db.conn(), "t1").unwrap();
        assert_eq!(t.language, "de");
        assert_eq!(t.title, "Title de");
    }

    #[test]
    fn get_by_post_and_language() {
        let db = setup();
        insert_post_translation(db.conn(), &make_translation("t1", "de")).unwrap();
        let t = get_post_translation_by_post_and_language(db.conn(), "post1", "de").unwrap();
        assert_eq!(t.id, "t1");
    }

    #[test]
    fn list_by_post() {
        let db = setup();
        insert_post_translation(db.conn(), &make_translation("t1", "de")).unwrap();
        insert_post_translation(db.conn(), &make_translation("t2", "fr")).unwrap();
        let list = list_post_translations_by_post(db.conn(), "post1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].language, "de");
        assert_eq!(list[1].language, "fr");
    }

    #[test]
    fn update_translation() {
        let db = setup();
        let mut t = make_translation("t1", "de");
        insert_post_translation(db.conn(), &t).unwrap();
        t.title = "Neuer Titel".into();
        t.updated_at = 9999;
        update_post_translation(db.conn(), &t).unwrap();
        let fetched = get_post_translation_by_id(db.conn(), "t1").unwrap();
        assert_eq!(fetched.title, "Neuer Titel");
        assert_eq!(fetched.updated_at, 9999);
    }

    #[test]
    fn delete_single() {
        let db = setup();
        insert_post_translation(db.conn(), &make_translation("t1", "de")).unwrap();
        delete_post_translation(db.conn(), "t1").unwrap();
        assert!(get_post_translation_by_id(db.conn(), "t1").is_err());
    }

    #[test]
    fn delete_all_for_post() {
        let db = setup();
        insert_post_translation(db.conn(), &make_translation("t1", "de")).unwrap();
        insert_post_translation(db.conn(), &make_translation("t2", "fr")).unwrap();
        delete_all_translations_for_post(db.conn(), "post1").unwrap();
        let list = list_post_translations_by_post(db.conn(), "post1").unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn duplicate_language_rejected() {
        let db = setup();
        insert_post_translation(db.conn(), &make_translation("t1", "de")).unwrap();
        let result = insert_post_translation(db.conn(), &make_translation("t2", "de"));
        assert!(result.is_err());
    }

    #[test]
    fn archived_status_rejected_on_insert() {
        let db = setup();
        let mut t = make_translation("t1", "de");
        t.status = PostStatus::Archived;
        let result = insert_post_translation(db.conn(), &t);
        assert!(result.is_err());
    }

    #[test]
    fn archived_status_rejected_on_update() {
        let db = setup();
        let mut t = make_translation("t1", "de");
        insert_post_translation(db.conn(), &t).unwrap();
        t.status = PostStatus::Archived;
        let result = update_post_translation(db.conn(), &t);
        assert!(result.is_err());
    }
}
