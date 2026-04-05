use rust_stemmers::{Algorithm, Stemmer};
use rusqlite::Connection;

/// Create FTS5 virtual tables at runtime (not in migrations per spec).
///
/// Schema follows specs/schema.allium: multi-column FTS5 with separate fields
/// for weighted search. Not content-sync — we manually manage stemmed content.
pub fn ensure_fts_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE VIRTUAL TABLE IF NOT EXISTS posts_fts USING fts5(
            post_id UNINDEXED,
            title,
            excerpt,
            content,
            tags,
            categories
        );
        CREATE VIRTUAL TABLE IF NOT EXISTS media_fts USING fts5(
            media_id UNINDEXED,
            title,
            alt,
            caption,
            original_name,
            tags
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

/// Index a post in the FTS table with separate columns per spec.
///
/// Concatenates translation text into the content column (stemmed per-language).
pub fn index_post(
    conn: &Connection,
    post_id: &str,
    title: &str,
    excerpt: Option<&str>,
    content: Option<&str>,
    tags: &[String],
    categories: &[String],
    translations: &[(String, String)], // (text, language) pairs
    language: &str,
) -> rusqlite::Result<()> {
    // Remove existing entry
    remove_post_from_index(conn, post_id)?;

    let stemmed_title = stem_text(title, language);
    let stemmed_excerpt = stem_text(excerpt.unwrap_or(""), language);

    // Content column: post content + all translation texts
    let mut content_parts = Vec::new();
    if let Some(cnt) = content {
        content_parts.push(stem_text(cnt, language));
    }
    for (text, trans_lang) in translations {
        content_parts.push(stem_text(text, trans_lang));
    }
    let stemmed_content = content_parts.join(" ");

    let stemmed_tags = stem_text(&tags.join(" "), language);
    let stemmed_categories = stem_text(&categories.join(" "), language);

    conn.execute(
        "INSERT INTO posts_fts (post_id, title, excerpt, content, tags, categories) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![post_id, stemmed_title, stemmed_excerpt, stemmed_content, stemmed_tags, stemmed_categories],
    )?;
    Ok(())
}

/// Index a media item in the FTS table with separate columns per spec.
pub fn index_media(
    conn: &Connection,
    media_id: &str,
    title: Option<&str>,
    alt: Option<&str>,
    caption: Option<&str>,
    original_name: &str,
    tags: &[String],
    translations: &[(String, String)], // (text, language) pairs
    language: &str,
) -> rusqlite::Result<()> {
    remove_media_from_index(conn, media_id)?;

    let stemmed_title = stem_text(title.unwrap_or(""), language);
    let stemmed_alt = stem_text(alt.unwrap_or(""), language);

    // Caption column: media caption + all translation texts
    let mut caption_parts = Vec::new();
    if let Some(c) = caption {
        caption_parts.push(stem_text(c, language));
    }
    for (text, trans_lang) in translations {
        caption_parts.push(stem_text(text, trans_lang));
    }
    let stemmed_caption = caption_parts.join(" ");

    let stemmed_name = stem_text(original_name, language);
    let stemmed_tags = stem_text(&tags.join(" "), language);

    conn.execute(
        "INSERT INTO media_fts (media_id, title, alt, caption, original_name, tags) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![media_id, stemmed_title, stemmed_alt, stemmed_caption, stemmed_name, stemmed_tags],
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

/// Filters for post search.
#[derive(Default)]
pub struct PostSearchFilters<'a> {
    pub status: Option<&'a str>,
    pub tags: Option<&'a [String]>,
    pub categories: Option<&'a [String]>,
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Search result envelope with pagination metadata per spec.
#[derive(Debug)]
pub struct SearchResults {
    pub post_ids: Vec<String>,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
}

/// Search posts with filters. Returns matching post IDs with pagination metadata.
pub fn search_posts_filtered(
    conn: &Connection,
    query: &str,
    language: &str,
    filters: &PostSearchFilters,
) -> rusqlite::Result<SearchResults> {
    // Get FTS matches first
    let fts_ids = search_posts(conn, query, language)?;
    if fts_ids.is_empty() {
        return Ok(SearchResults { post_ids: vec![], total: 0, offset: filters.offset.unwrap_or(0), limit: filters.limit.unwrap_or(0) });
    }

    // Apply filters by querying posts table
    let placeholders: Vec<String> = fts_ids.iter().enumerate().map(|(i, _)| format!("?{}", i + 1)).collect();
    let mut sql = format!(
        "SELECT id FROM posts WHERE id IN ({}) ",
        placeholders.join(",")
    );
    let mut params: Vec<Box<dyn rusqlite::types::ToSql>> = fts_ids.iter().map(|id| Box::new(id.clone()) as Box<dyn rusqlite::types::ToSql>).collect();
    let mut param_idx = fts_ids.len() + 1;

    if let Some(status) = filters.status {
        sql.push_str(&format!("AND status = ?{param_idx} "));
        params.push(Box::new(status.to_string()));
        param_idx += 1;
    }

    if let Some(tags) = filters.tags {
        // Filter posts whose JSON tags array contains ALL specified tags (case-insensitive)
        for tag in tags {
            sql.push_str(&format!(
                "AND EXISTS (SELECT 1 FROM json_each(posts.tags) WHERE LOWER(json_each.value) = LOWER(?{param_idx})) "
            ));
            params.push(Box::new(tag.clone()));
            param_idx += 1;
        }
    }

    if let Some(categories) = filters.categories {
        // Filter posts whose JSON categories array contains ALL specified categories (case-insensitive)
        for cat in categories {
            sql.push_str(&format!(
                "AND EXISTS (SELECT 1 FROM json_each(posts.categories) WHERE LOWER(json_each.value) = LOWER(?{param_idx})) "
            ));
            params.push(Box::new(cat.clone()));
            param_idx += 1;
        }
    }

    if let Some(year) = filters.year {
        // Filter by year from created_at (unix ms)
        let start = chrono::NaiveDate::from_ymd_opt(year, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
        let end = chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
        sql.push_str(&format!("AND created_at >= ?{param_idx} AND created_at < ?{} ", param_idx + 1));
        params.push(Box::new(start));
        params.push(Box::new(end));
        param_idx += 2;
    }

    if let Some(month) = filters.month {
        if let Some(year) = filters.year {
            let (end_year, end_month) = if month == 12 { (year + 1, 1) } else { (year, month as i32 + 1) };
            let start = chrono::NaiveDate::from_ymd_opt(year, month, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
            let end = chrono::NaiveDate::from_ymd_opt(end_year, end_month as u32, 1).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp_millis();
            sql.push_str(&format!("AND created_at >= ?{param_idx} AND created_at < ?{} ", param_idx + 1));
            params.push(Box::new(start));
            params.push(Box::new(end));
            param_idx += 2;
        }
    }

    let _ = param_idx; // suppress unused warning

    // First get total count (without LIMIT/OFFSET)
    let count_sql = sql.replace("SELECT id FROM posts", "SELECT COUNT(*) FROM posts");
    let total: usize = {
        let mut stmt = conn.prepare(&count_sql)?;
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
        stmt.query_row(params_refs.as_slice(), |row| row.get::<_, usize>(0))?
    };

    sql.push_str("ORDER BY created_at DESC ");

    let offset = filters.offset.unwrap_or(0);
    let limit = filters.limit.unwrap_or(total);

    if filters.limit.is_some() {
        sql.push_str(&format!("LIMIT {limit} "));
        sql.push_str(&format!("OFFSET {offset} "));
    }

    let mut stmt = conn.prepare(&sql)?;
    let params_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |row| {
        row.get::<_, String>(0)
    })?;
    let post_ids: Vec<String> = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(SearchResults { post_ids, total, offset, limit })
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
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
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
    fn fts_multi_column_schema() {
        let db = setup();
        // Verify posts_fts has the expected columns by inserting into each
        db.conn().execute(
            "INSERT INTO posts_fts (post_id, title, excerpt, content, tags, categories) VALUES ('p1', 'T', 'E', 'C', 'tg', 'ct')",
            [],
        ).unwrap();
        // Verify media_fts has the expected columns
        db.conn().execute(
            "INSERT INTO media_fts (media_id, title, alt, caption, original_name, tags) VALUES ('m1', 'T', 'A', 'C', 'N', 'tg')",
            [],
        ).unwrap();
    }

    #[test]
    fn stem_german() {
        let stemmed = stem_text("Programmierung Entwicklung", "de");
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
    fn stem_french() {
        let stemmed = stem_text("programmation développement", "fr");
        assert!(!stemmed.is_empty());
        assert_ne!(stemmed, "programmation développement");
    }

    #[test]
    fn stem_spanish() {
        let stemmed = stem_text("programación desarrollo", "es");
        assert!(!stemmed.is_empty());
        assert_ne!(stemmed, "programación desarrollo");
    }

    #[test]
    fn stem_italian() {
        let stemmed = stem_text("programmazione sviluppo", "it");
        assert!(!stemmed.is_empty());
        assert_ne!(stemmed, "programmazione sviluppo");
    }

    #[test]
    fn stem_portuguese() {
        let stemmed = stem_text("programação desenvolvimento", "pt");
        assert!(!stemmed.is_empty());
        assert_ne!(stemmed, "programação desenvolvimento");
    }

    #[test]
    fn stem_russian() {
        let stemmed = stem_text("программирование разработка", "ru");
        assert!(!stemmed.is_empty());
        assert_ne!(stemmed, "программирование разработка");
    }

    #[test]
    fn stem_swedish() {
        let stemmed = stem_text("utvecklingen datorerna", "sv");
        assert!(!stemmed.is_empty());
        assert_ne!(stemmed, "utvecklingen datorerna");
    }

    #[test]
    fn stem_dutch() {
        let stemmed = stem_text("programmering ontwikkeling", "nl");
        assert!(!stemmed.is_empty());
        assert_ne!(stemmed, "programmering ontwikkeling");
    }

    #[test]
    fn stem_turkish() {
        let stemmed = stem_text("programlama geliştirilmesi", "tr");
        assert!(!stemmed.is_empty());
        assert_ne!(stemmed, "programlama geliştirilmesi");
    }

    #[test]
    fn stem_fallback_uses_english() {
        // Unknown language falls back to English stemmer
        let stemmed_unknown = stem_text("running", "xx");
        let stemmed_english = stem_text("running", "en");
        assert_eq!(stemmed_unknown, stemmed_english);
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
            &[("Esmeralda English".into(), "en".into())],
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

    #[test]
    fn translations_stemmed_with_own_language() {
        let db = setup();
        // German post with English translation
        index_post(
            db.conn(), "p1", "Programmierung", None, Some("Deutsche Entwicklung"),
            &[], &[],
            &[("English development programming".into(), "en".into())],
            "de",
        ).unwrap();

        // Search with English stemming should find via English translation
        let results = search_posts(db.conn(), "develop", "en").unwrap();
        assert_eq!(results, vec!["p1"]);
    }

    #[test]
    fn search_by_title_field() {
        let db = setup();
        index_post(db.conn(), "p1", "Unique Title Here", None, Some("body text"), &[], &[], &[], "en").unwrap();

        // Search for title content
        let results = search_posts(db.conn(), "unique", "en").unwrap();
        assert_eq!(results, vec!["p1"]);
    }

    #[test]
    fn search_by_tags() {
        let db = setup();
        index_post(db.conn(), "p1", "Post", None, None, &["photography".into()], &[], &[], "en").unwrap();

        let results = search_posts(db.conn(), "photography", "en").unwrap();
        assert_eq!(results, vec!["p1"]);
    }

    #[test]
    fn search_by_categories() {
        let db = setup();
        index_post(db.conn(), "p1", "Post", None, None, &[], &["article".into()], &[], "en").unwrap();

        let results = search_posts(db.conn(), "article", "en").unwrap();
        assert_eq!(results, vec!["p1"]);
    }

    #[test]
    fn search_filtered_by_status() {
        let db = setup();
        // Insert a post in the posts table (needed for the filter query to join)
        use crate::db::queries::project::{insert_project, make_test_project};
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        db.conn().execute(
            "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
             VALUES ('post1', 'p1', 'Test Post', 'test', 'published', 1700000000000, 1700000000000)",
            [],
        ).unwrap();
        db.conn().execute(
            "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
             VALUES ('post2', 'p1', 'Draft Post', 'draft-post', 'draft', 1700000000000, 1700000000000)",
            [],
        ).unwrap();

        index_post(db.conn(), "post1", "Test Post", None, None, &[], &[], &[], "en").unwrap();
        index_post(db.conn(), "post2", "Draft Post", None, None, &[], &[], &[], "en").unwrap();

        let filters = PostSearchFilters {
            status: Some("published"),
            ..Default::default()
        };
        let results = search_posts_filtered(db.conn(), "post", "en", &filters).unwrap();
        assert_eq!(results.post_ids, vec!["post1"]);
        assert_eq!(results.total, 1);
    }

    #[test]
    fn search_filtered_by_year() {
        let db = setup();
        use crate::db::queries::project::{insert_project, make_test_project};
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();

        // 2024-06-15 in unix ms
        let ts_2024: i64 = 1718409600000;
        // 2023-06-15 in unix ms
        let ts_2023: i64 = 1686873600000;

        db.conn().execute(
            "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
             VALUES ('p2024', 'p1', 'Year 2024', 'y2024', 'draft', ?1, ?1)",
            rusqlite::params![ts_2024],
        ).unwrap();
        db.conn().execute(
            "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
             VALUES ('p2023', 'p1', 'Year 2023', 'y2023', 'draft', ?1, ?1)",
            rusqlite::params![ts_2023],
        ).unwrap();

        index_post(db.conn(), "p2024", "Year 2024", None, None, &[], &[], &[], "en").unwrap();
        index_post(db.conn(), "p2023", "Year 2023", None, None, &[], &[], &[], "en").unwrap();

        let filters = PostSearchFilters {
            year: Some(2024),
            ..Default::default()
        };
        let results = search_posts_filtered(db.conn(), "year", "en", &filters).unwrap();
        assert_eq!(results.post_ids, vec!["p2024"]);
        assert_eq!(results.total, 1);
    }

    #[test]
    fn search_filtered_with_limit_and_offset() {
        let db = setup();
        use crate::db::queries::project::{insert_project, make_test_project};
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();

        for i in 0..5 {
            let id = format!("p{i}");
            let slug = format!("post-{i}");
            db.conn().execute(
                "INSERT INTO posts (id, project_id, title, slug, status, created_at, updated_at)
                 VALUES (?1, 'p1', 'Searchable', ?2, 'draft', ?3, ?3)",
                rusqlite::params![id, slug, 1700000000000i64 - i as i64 * 1000],
            ).unwrap();
            index_post(db.conn(), &id, "Searchable", None, None, &[], &[], &[], "en").unwrap();
        }

        let filters = PostSearchFilters {
            limit: Some(2),
            offset: Some(1),
            ..Default::default()
        };
        let results = search_posts_filtered(db.conn(), "searchable", "en", &filters).unwrap();
        assert_eq!(results.post_ids.len(), 2);
        assert_eq!(results.total, 5);
        assert_eq!(results.offset, 1);
        assert_eq!(results.limit, 2);
    }
}
