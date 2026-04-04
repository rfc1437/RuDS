use rusqlite::{params, Connection};

use crate::db::from_row::{media_translation_from_row, MEDIA_TRANSLATION_COLUMNS};
use crate::model::MediaTranslation;

pub fn insert_media_translation(
    conn: &Connection,
    t: &MediaTranslation,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO media_translations (
            id, project_id, translation_for, language, title, alt, caption,
            created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            t.id,
            t.project_id,
            t.translation_for,
            t.language,
            t.title,
            t.alt,
            t.caption,
            t.created_at,
            t.updated_at,
        ],
    )?;
    Ok(())
}

pub fn get_media_translation_by_media_and_language(
    conn: &Connection,
    translation_for: &str,
    language: &str,
) -> rusqlite::Result<MediaTranslation> {
    conn.query_row(
        &format!(
            "SELECT {MEDIA_TRANSLATION_COLUMNS} FROM media_translations
             WHERE translation_for = ?1 AND language = ?2"
        ),
        params![translation_for, language],
        media_translation_from_row,
    )
}

pub fn list_media_translations_by_media(
    conn: &Connection,
    translation_for: &str,
) -> rusqlite::Result<Vec<MediaTranslation>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {MEDIA_TRANSLATION_COLUMNS} FROM media_translations
         WHERE translation_for = ?1 ORDER BY language"
    ))?;
    let rows = stmt.query_map(params![translation_for], media_translation_from_row)?;
    rows.collect()
}

pub fn upsert_media_translation(
    conn: &Connection,
    t: &MediaTranslation,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO media_translations (
            id, project_id, translation_for, language, title, alt, caption,
            created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(translation_for, language) DO UPDATE SET
            title = excluded.title,
            alt = excluded.alt,
            caption = excluded.caption,
            updated_at = excluded.updated_at",
        params![
            t.id,
            t.project_id,
            t.translation_for,
            t.language,
            t.title,
            t.alt,
            t.caption,
            t.created_at,
            t.updated_at,
        ],
    )?;
    Ok(())
}

pub fn delete_media_translation(
    conn: &Connection,
    translation_for: &str,
    language: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM media_translations WHERE translation_for = ?1 AND language = ?2",
        params![translation_for, language],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries::media::{insert_media, make_test_media};
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::db::Database;

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        insert_media(db.conn(), &make_test_media("m1", "p1")).unwrap();
        db
    }

    fn make_mt(id: &str, lang: &str) -> MediaTranslation {
        MediaTranslation {
            id: id.to_string(),
            project_id: "p1".to_string(),
            translation_for: "m1".to_string(),
            language: lang.to_string(),
            title: Some(format!("Title {lang}")),
            alt: Some(format!("Alt {lang}")),
            caption: None,
            created_at: 1000,
            updated_at: 2000,
        }
    }

    #[test]
    fn insert_and_get() {
        let db = setup();
        insert_media_translation(db.conn(), &make_mt("mt1", "de")).unwrap();
        let t = get_media_translation_by_media_and_language(db.conn(), "m1", "de").unwrap();
        assert_eq!(t.title.as_deref(), Some("Title de"));
    }

    #[test]
    fn list_by_media() {
        let db = setup();
        insert_media_translation(db.conn(), &make_mt("mt1", "de")).unwrap();
        insert_media_translation(db.conn(), &make_mt("mt2", "fr")).unwrap();
        let list = list_media_translations_by_media(db.conn(), "m1").unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].language, "de");
        assert_eq!(list[1].language, "fr");
    }

    #[test]
    fn upsert_inserts_then_updates() {
        let db = setup();
        let mut t = make_mt("mt1", "de");
        upsert_media_translation(db.conn(), &t).unwrap();
        let fetched = get_media_translation_by_media_and_language(db.conn(), "m1", "de").unwrap();
        assert_eq!(fetched.title.as_deref(), Some("Title de"));

        t.title = Some("Neuer Titel".into());
        t.updated_at = 9999;
        upsert_media_translation(db.conn(), &t).unwrap();
        let fetched = get_media_translation_by_media_and_language(db.conn(), "m1", "de").unwrap();
        assert_eq!(fetched.title.as_deref(), Some("Neuer Titel"));
        assert_eq!(fetched.updated_at, 9999);
    }

    #[test]
    fn delete_translation() {
        let db = setup();
        insert_media_translation(db.conn(), &make_mt("mt1", "de")).unwrap();
        delete_media_translation(db.conn(), "m1", "de").unwrap();
        assert!(
            get_media_translation_by_media_and_language(db.conn(), "m1", "de").is_err()
        );
    }
}
