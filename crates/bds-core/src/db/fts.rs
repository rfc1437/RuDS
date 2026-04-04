use rust_stemmers::{Algorithm, Stemmer};
use rusqlite::Connection;

/// Create FTS5 virtual tables at runtime (not in migrations per spec).
pub fn ensure_fts_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS posts_fts USING fts5(
            post_id UNINDEXED,
            content
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS media_fts USING fts5(
            media_id UNINDEXED,
            content
        );"
    )?;
    Ok(())
}

/// Map ISO 639-1 language code to Snowball stemmer algorithm.
fn stemmer_for_language(lang: &str) -> Algorithm {
    match lang {
        "ar" => Algorithm::Arabic,
        "da" => Algorithm::Danish,
        "de" => Algorithm::German,
        "el" => Algorithm::Greek,
        "en" => Algorithm::English,
        "es" => Algorithm::Spanish,
        "fi" => Algorithm::Finnish,
        "fr" => Algorithm::French,
        "hu" => Algorithm::Hungarian,
        "hy" | "id" => Algorithm::English, // no dedicated stemmer, fallback
        "it" => Algorithm::Italian,
        "nb" | "nn" | "no" => Algorithm::Norwegian,
        "nl" => Algorithm::Dutch,
        "pt" => Algorithm::Portuguese,
        "ro" => Algorithm::Romanian,
        "ru" => Algorithm::Russian,
        "sv" => Algorithm::Swedish,
        "ta" => Algorithm::Tamil,
        "tr" => Algorithm::Turkish,
        _ => Algorithm::English,
    }
}

/// Stem a text string using the Snowball stemmer for the given language.
pub fn stem_text(text: &str, language: &str) -> String {
    let algo = stemmer_for_language(language);
    let stemmer = Stemmer::create(algo);
    text.split_whitespace()
        .map(|word| stemmer.stem(word).into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Index a post in the FTS table. Concatenates title, excerpt, content, tags,
/// categories, and all translation text. Pre-stems before inserting.
pub fn index_post(
    conn: &Connection,
    post_id: &str,
    title: &str,
    excerpt: Option<&str>,
    content: Option<&str>,
    tags: &[String],
    categories: &[String],
    translation_texts: &[String],
    language: &str,
) -> rusqlite::Result<()> {
    // Remove existing entry
    remove_post_from_index(conn, post_id)?;

    let mut parts: Vec<&str> = vec![title];
    if let Some(exc) = excerpt {
        parts.push(exc);
    }
    if let Some(cnt) = content {
        parts.push(cnt);
    }
    let tags_str = tags.join(" ");
    parts.push(&tags_str);
    let cats_str = categories.join(" ");
    parts.push(&cats_str);

    let combined = parts.join(" ");
    let mut full_text = stem_text(&combined, language);

    // Add translation texts (stemmed with their own implied language or same)
    for t in translation_texts {
        full_text.push(' ');
        full_text.push_str(&stem_text(t, language));
    }

    conn.execute(
        "INSERT INTO posts_fts (post_id, content) VALUES (?1, ?2)",
        rusqlite::params![post_id, full_text],
    )?;
    Ok(())
}

/// Index a media item in the FTS table.
pub fn index_media(
    conn: &Connection,
    media_id: &str,
    title: Option<&str>,
    alt: Option<&str>,
    caption: Option<&str>,
    original_name: &str,
    tags: &[String],
    translation_texts: &[String],
    language: &str,
) -> rusqlite::Result<()> {
    remove_media_from_index(conn, media_id)?;

    let mut parts: Vec<&str> = Vec::new();
    if let Some(t) = title {
        parts.push(t);
    }
    if let Some(a) = alt {
        parts.push(a);
    }
    if let Some(c) = caption {
        parts.push(c);
    }
    parts.push(original_name);
    let tags_str = tags.join(" ");
    parts.push(&tags_str);

    let combined = parts.join(" ");
    let mut full_text = stem_text(&combined, language);

    for t in translation_texts {
        full_text.push(' ');
        full_text.push_str(&stem_text(t, language));
    }

    conn.execute(
        "INSERT INTO media_fts (media_id, content) VALUES (?1, ?2)",
        rusqlite::params![media_id, full_text],
    )?;
    Ok(())
}

/// Remove a post from the FTS index.
pub fn remove_post_from_index(conn: &Connection, post_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM posts_fts WHERE post_id = ?1",
        rusqlite::params![post_id],
    )?;
    Ok(())
}

/// Remove a media item from the FTS index.
pub fn remove_media_from_index(conn: &Connection, media_id: &str) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM media_fts WHERE media_id = ?1",
        rusqlite::params![media_id],
    )?;
    Ok(())
}

/// Search posts by full-text query. Returns matching post IDs.
pub fn search_posts(conn: &Connection, query: &str, language: &str) -> rusqlite::Result<Vec<String>> {
    let stemmed = stem_text(query, language);
    let mut stmt = conn.prepare(
        "SELECT post_id FROM posts_fts WHERE posts_fts MATCH ?1 ORDER BY rank"
    )?;
    let rows = stmt.query_map(rusqlite::params![stemmed], |row| {
        row.get::<_, String>(0)
    })?;
    rows.collect()
}

/// Search media by full-text query. Returns matching media IDs.
pub fn search_media(conn: &Connection, query: &str, language: &str) -> rusqlite::Result<Vec<String>> {
    let stemmed = stem_text(query, language);
    let mut stmt = conn.prepare(
        "SELECT media_id FROM media_fts WHERE media_fts MATCH ?1 ORDER BY rank"
    )?;
    let rows = stmt.query_map(rusqlite::params![stemmed], |row| {
        row.get::<_, String>(0)
    })?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate();
        ensure_fts_tables(db.conn()).unwrap();
        db
    }

    #[test]
    fn fts_tables_created() {
        let db = setup();
        let count: i64 = db.conn()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('posts_fts', 'media_fts')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn fts_tables_idempotent() {
        let db = setup();
        ensure_fts_tables(db.conn()).unwrap(); // second call should not error
    }

    #[test]
    fn stem_german() {
        let stemmed = stem_text("Programmierung Entwicklung", "de");
        // German stemmer should reduce these words
        assert!(!stemmed.is_empty());
        assert_ne!(stemmed, "Programmierung Entwicklung");
    }

    #[test]
    fn stem_english() {
        let stemmed = stem_text("running jumps", "en");
        assert!(stemmed.contains("run"));
        assert!(stemmed.contains("jump"));
    }

    #[test]
    fn index_and_search_post() {
        let db = setup();
        index_post(
            db.conn(),
            "post-1",
            "Esmeralda Spider",
            Some("A macro photo"),
            Some("Beautiful spider web"),
            &["fotografie".into(), "natur".into()],
            &["picture".into()],
            &["Esmeralda English".into()],
            "en",
        ).unwrap();

        let results = search_posts(db.conn(), "spider", "en").unwrap();
        assert_eq!(results, vec!["post-1"]);
    }

    #[test]
    fn index_and_search_media() {
        let db = setup();
        index_media(
            db.conn(),
            "media-1",
            Some("Sunset Photo"),
            Some("Beautiful sunset over mountains"),
            None,
            "sunset.jpg",
            &["nature".into()],
            &[],
            "en",
        ).unwrap();

        let results = search_media(db.conn(), "sunset", "en").unwrap();
        assert_eq!(results, vec!["media-1"]);
    }

    #[test]
    fn search_no_results() {
        let db = setup();
        let results = search_posts(db.conn(), "nonexistent", "en").unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn remove_from_index() {
        let db = setup();
        index_post(
            db.conn(), "p1", "Test", None, None,
            &[], &[], &[], "en",
        ).unwrap();
        assert_eq!(search_posts(db.conn(), "test", "en").unwrap().len(), 1);

        remove_post_from_index(db.conn(), "p1").unwrap();
        assert!(search_posts(db.conn(), "test", "en").unwrap().is_empty());
    }

    #[test]
    fn reindex_replaces_old() {
        let db = setup();
        index_post(db.conn(), "p1", "Alpha", None, None, &[], &[], &[], "en").unwrap();
        index_post(db.conn(), "p1", "Beta", None, None, &[], &[], &[], "en").unwrap();

        assert!(search_posts(db.conn(), "alpha", "en").unwrap().is_empty());
        assert_eq!(search_posts(db.conn(), "beta", "en").unwrap().len(), 1);
    }
}
