use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::from_row::{PostTranslationRecord, convert, convert_all};
use crate::db::schema::post_translations;
use crate::model::PostTranslation;

pub fn insert_post_translation(conn: &DbConnection, t: &PostTranslation) -> QueryResult<()> {
    if !t.status.is_valid_for_translation() {
        return Err(diesel::result::Error::SerializationError(
            "translation status must be draft or published".into(),
        ));
    }
    conn.with(|c| {
        diesel::insert_into(post_translations::table)
            .values(PostTranslationRecord::from(t))
            .execute(c)
            .map(|_| ())
    })
}

pub fn get_post_translation_by_id(conn: &DbConnection, id: &str) -> QueryResult<PostTranslation> {
    conn.with(|c| {
        post_translations::table
            .filter(post_translations::id.eq(id))
            .select(PostTranslationRecord::as_select())
            .first(c)
            .and_then(convert)
    })
}

pub fn get_post_translation_by_post_and_language(
    conn: &DbConnection,
    translation_for: &str,
    language: &str,
) -> QueryResult<PostTranslation> {
    conn.with(|c| {
        post_translations::table
            .filter(post_translations::translation_for.eq(translation_for))
            .filter(post_translations::language.eq(language))
            .select(PostTranslationRecord::as_select())
            .first(c)
            .and_then(convert)
    })
}

pub fn list_post_translations_by_post(
    conn: &DbConnection,
    translation_for: &str,
) -> QueryResult<Vec<PostTranslation>> {
    conn.with(|c| {
        post_translations::table
            .filter(post_translations::translation_for.eq(translation_for))
            .order(post_translations::language)
            .select(PostTranslationRecord::as_select())
            .load(c)
            .and_then(convert_all)
    })
}

pub fn update_post_translation(conn: &DbConnection, t: &PostTranslation) -> QueryResult<()> {
    if !t.status.is_valid_for_translation() {
        return Err(diesel::result::Error::SerializationError(
            "translation status must be draft or published".into(),
        ));
    }
    conn.with(|c| {
        diesel::update(post_translations::table.filter(post_translations::id.eq(&t.id)))
            .set(PostTranslationRecord::from(t))
            .execute(c)
            .map(|_| ())
    })
}

pub fn delete_post_translation(conn: &DbConnection, id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(post_translations::table.filter(post_translations::id.eq(id)))
            .execute(c)
            .map(|_| ())
    })
}

pub fn delete_all_translations_for_post(
    conn: &DbConnection,
    translation_for: &str,
) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(
            post_translations::table.filter(post_translations::translation_for.eq(translation_for)),
        )
        .execute(c)
        .map(|_| ())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::post::{insert_post, make_test_post};
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::model::PostStatus;

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        insert_post(db.conn(), &make_test_post("post1", "p1", "hello")).unwrap();
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
