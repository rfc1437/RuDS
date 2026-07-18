use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::from_row::MediaTranslationRecord;
use crate::db::schema::media_translations;
use crate::model::MediaTranslation;

pub fn insert_media_translation(conn: &DbConnection, t: &MediaTranslation) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(media_translations::table)
            .values(MediaTranslationRecord::from(t))
            .execute(c)
            .map(|_| ())
    })
}

pub fn get_media_translation_by_media_and_language(
    conn: &DbConnection,
    translation_for: &str,
    language: &str,
) -> QueryResult<MediaTranslation> {
    conn.with(|c| {
        media_translations::table
            .filter(media_translations::translation_for.eq(translation_for))
            .filter(media_translations::language.eq(language))
            .select(MediaTranslationRecord::as_select())
            .first(c)
            .map(Into::into)
    })
}

pub fn list_media_translations_by_media(
    conn: &DbConnection,
    translation_for: &str,
) -> QueryResult<Vec<MediaTranslation>> {
    conn.with(|c| {
        media_translations::table
            .filter(media_translations::translation_for.eq(translation_for))
            .order(media_translations::language)
            .select(MediaTranslationRecord::as_select())
            .load(c)
            .map(|rows: Vec<MediaTranslationRecord>| rows.into_iter().map(Into::into).collect())
    })
}

pub fn upsert_media_translation(conn: &DbConnection, t: &MediaTranslation) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(media_translations::table)
            .values(MediaTranslationRecord::from(t))
            .on_conflict((
                media_translations::translation_for,
                media_translations::language,
            ))
            .do_update()
            .set((
                media_translations::title.eq(&t.title),
                media_translations::alt.eq(&t.alt),
                media_translations::caption.eq(&t.caption),
                media_translations::updated_at.eq(t.updated_at),
            ))
            .execute(c)
            .map(|_| ())
    })
}

pub fn delete_media_translation(
    conn: &DbConnection,
    translation_for: &str,
    language: &str,
) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(
            media_translations::table
                .filter(media_translations::translation_for.eq(translation_for))
                .filter(media_translations::language.eq(language)),
        )
        .execute(c)
        .map(|_| ())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::media::{insert_media, make_test_media};
    use crate::db::queries::project::{insert_project, make_test_project};

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
        assert!(get_media_translation_by_media_and_language(db.conn(), "m1", "de").is_err());
    }
}
