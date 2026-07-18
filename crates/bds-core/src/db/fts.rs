use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_types::{BigInt, Text};
use diesel::sqlite::Sqlite;
use rust_stemmers::{Algorithm, Stemmer};

use crate::db::DbConnection as Connection;
use crate::db::schema::{post_translations, posts};
use crate::util::calendar_range_unix_ms;

diesel::define_sql_function!(fn instr(haystack: Text, needle: Text) -> diesel::sql_types::Integer);
diesel::define_sql_function!(fn lower(value: Text) -> Text);

#[derive(QueryableByName)]
#[diesel(check_for_backend(Sqlite))]
struct PostIdRow {
    #[diesel(sql_type = Text)]
    post_id: String,
}

#[derive(QueryableByName)]
#[diesel(check_for_backend(Sqlite))]
struct MediaIdRow {
    #[diesel(sql_type = Text)]
    media_id: String,
}

#[derive(QueryableByName)]
#[diesel(check_for_backend(Sqlite))]
struct TableCountRow {
    #[diesel(sql_type = BigInt)]
    count: i64,
}

#[derive(QueryableByName)]
#[diesel(check_for_backend(Sqlite))]
struct ColumnNameRow {
    #[diesel(sql_type = Text)]
    name: String,
}

const CREATE_FTS_TABLES: &str = "CREATE VIRTUAL TABLE IF NOT EXISTS posts_fts USING fts5(
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
        );";

/// Whether both application FTS5 virtual tables are present.
pub fn tables_exist(conn: &Connection) -> QueryResult<bool> {
    conn.with(|c| {
        diesel::sql_query(
            "SELECT COUNT(*) AS count FROM sqlite_master \
             WHERE type = 'table' AND name IN ('posts_fts', 'media_fts')",
        )
        .get_result::<TableCountRow>(c)
        .map(|row| row.count == 2)
    })
}

/// Create FTS5 virtual tables at runtime (not in migrations per spec).
///
/// Schema follows specs/schema.allium: multi-column FTS5 with separate fields
/// for weighted search. Not content-sync — we manually manage stemmed content.
pub fn ensure_fts_tables(conn: &Connection) -> QueryResult<()> {
    conn.with(|c| c.batch_execute(CREATE_FTS_TABLES))
}

/// Whether the runtime-managed FTS tables have the current deployed schema.
pub fn schema_is_current(conn: &Connection) -> QueryResult<bool> {
    conn.with(|c| {
        let post_columns =
            diesel::sql_query("SELECT name FROM pragma_table_info('posts_fts') ORDER BY cid")
                .load::<ColumnNameRow>(c)?
                .into_iter()
                .map(|row| row.name)
                .collect::<Vec<_>>();
        let media_columns =
            diesel::sql_query("SELECT name FROM pragma_table_info('media_fts') ORDER BY cid")
                .load::<ColumnNameRow>(c)?
                .into_iter()
                .map(|row| row.name)
                .collect::<Vec<_>>();

        Ok(post_columns
            == [
                "post_id",
                "title",
                "excerpt",
                "content",
                "tags",
                "categories",
            ]
            && media_columns
                == [
                    "media_id",
                    "title",
                    "alt",
                    "caption",
                    "original_name",
                    "tags",
                ])
    })
}

/// Replace the derived FTS tables without touching user-authored data.
pub fn recreate_tables(conn: &Connection) -> QueryResult<()> {
    conn.with(|c| {
        c.batch_execute("DROP TABLE IF EXISTS posts_fts; DROP TABLE IF EXISTS media_fts;")?;
        c.batch_execute(CREATE_FTS_TABLES)
    })
}

#[cfg(test)]
pub(crate) fn install_deployed_schema_for_test(conn: &Connection) -> QueryResult<()> {
    conn.with(|c| {
        c.batch_execute(
            "DROP TABLE posts_fts;
             DROP TABLE media_fts;
             CREATE VIRTUAL TABLE posts_fts USING fts5(post_id UNINDEXED, content);
             CREATE VIRTUAL TABLE media_fts USING fts5(media_id UNINDEXED, content);",
        )
    })
}

pub fn drop_post_index(conn: &Connection) -> QueryResult<()> {
    conn.with(|c| c.batch_execute("DROP TABLE posts_fts"))
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

/// Structured translation data for FTS indexing of posts.
pub struct PostTranslationFts {
    pub title: String,
    pub excerpt: Option<String>,
    pub content: Option<String>,
    pub language: String,
}

/// Structured translation data for FTS indexing of media.
pub struct MediaTranslationFts {
    pub title: Option<String>,
    pub alt: Option<String>,
    pub caption: Option<String>,
    pub language: String,
}

/// Index a post in the FTS table with separate columns per spec.
///
/// Translation titles go to the title column, excerpts to excerpt, content to content.
#[expect(
    clippy::too_many_arguments,
    reason = "FTS columns mirror the persisted post fields"
)]
pub fn index_post(
    conn: &Connection,
    post_id: &str,
    title: &str,
    excerpt: Option<&str>,
    content: Option<&str>,
    tags: &[String],
    categories: &[String],
    translations: &[PostTranslationFts],
    language: &str,
) -> QueryResult<()> {
    // Remove existing entry
    remove_post_from_index(conn, post_id)?;

    // Title column: post title + all translation titles
    let mut title_parts = vec![stem_text(title, language)];
    for t in translations {
        title_parts.push(stem_text(&t.title, &t.language));
    }
    let stemmed_title = title_parts.join(" ");

    // Excerpt column: post excerpt + all translation excerpts
    let mut excerpt_parts = vec![stem_text(excerpt.unwrap_or(""), language)];
    for t in translations {
        if let Some(ref exc) = t.excerpt {
            excerpt_parts.push(stem_text(exc, &t.language));
        }
    }
    let stemmed_excerpt = excerpt_parts.join(" ");

    // Content column: post content + all translation content
    let mut content_parts = Vec::new();
    if let Some(cnt) = content {
        content_parts.push(stem_text(cnt, language));
    }
    for t in translations {
        if let Some(ref cnt) = t.content {
            content_parts.push(stem_text(cnt, &t.language));
        }
    }
    let stemmed_content = content_parts.join(" ");

    let stemmed_tags = stem_text(&tags.join(" "), language);
    let stemmed_categories = stem_text(&categories.join(" "), language);

    conn.with(|c| diesel::sql_query("INSERT INTO posts_fts (post_id, title, excerpt, content, tags, categories) VALUES (?, ?, ?, ?, ?, ?)")
        .bind::<Text, _>(post_id).bind::<Text, _>(stemmed_title).bind::<Text, _>(stemmed_excerpt)
        .bind::<Text, _>(stemmed_content).bind::<Text, _>(stemmed_tags).bind::<Text, _>(stemmed_categories)
        .execute(c).map(|_| ()))
}

/// Index a media item in the FTS table with separate columns per spec.
///
/// Translation titles go to the title column, alts to alt, captions to caption.
#[expect(
    clippy::too_many_arguments,
    reason = "FTS columns mirror the persisted media fields"
)]
pub fn index_media(
    conn: &Connection,
    media_id: &str,
    title: Option<&str>,
    alt: Option<&str>,
    caption: Option<&str>,
    original_name: &str,
    tags: &[String],
    translations: &[MediaTranslationFts],
    language: &str,
) -> QueryResult<()> {
    remove_media_from_index(conn, media_id)?;

    // Title column: media title + all translation titles
    let mut title_parts = vec![stem_text(title.unwrap_or(""), language)];
    for t in translations {
        if let Some(ref ttl) = t.title {
            title_parts.push(stem_text(ttl, &t.language));
        }
    }
    let stemmed_title = title_parts.join(" ");

    // Alt column: media alt + all translation alts
    let mut alt_parts = vec![stem_text(alt.unwrap_or(""), language)];
    for t in translations {
        if let Some(ref a) = t.alt {
            alt_parts.push(stem_text(a, &t.language));
        }
    }
    let stemmed_alt = alt_parts.join(" ");

    // Caption column: media caption + all translation captions
    let mut caption_parts = Vec::new();
    if let Some(c) = caption {
        caption_parts.push(stem_text(c, language));
    }
    for t in translations {
        if let Some(ref cap) = t.caption {
            caption_parts.push(stem_text(cap, &t.language));
        }
    }
    let stemmed_caption = caption_parts.join(" ");

    let stemmed_name = stem_text(original_name, language);
    let stemmed_tags = stem_text(&tags.join(" "), language);

    conn.with(|c| diesel::sql_query("INSERT INTO media_fts (media_id, title, alt, caption, original_name, tags) VALUES (?, ?, ?, ?, ?, ?)")
        .bind::<Text, _>(media_id).bind::<Text, _>(stemmed_title).bind::<Text, _>(stemmed_alt)
        .bind::<Text, _>(stemmed_caption).bind::<Text, _>(stemmed_name).bind::<Text, _>(stemmed_tags)
        .execute(c).map(|_| ()))
}

/// Remove a post from the FTS index.
pub fn remove_post_from_index(conn: &Connection, post_id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::sql_query("DELETE FROM posts_fts WHERE post_id = ?")
            .bind::<Text, _>(post_id)
            .execute(c)
            .map(|_| ())
    })
}

/// Remove a media item from the FTS index.
pub fn remove_media_from_index(conn: &Connection, media_id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::sql_query("DELETE FROM media_fts WHERE media_id = ?")
            .bind::<Text, _>(media_id)
            .execute(c)
            .map(|_| ())
    })
}

/// Search posts by full-text query. Returns matching post IDs.
pub fn search_posts(conn: &Connection, query: &str, language: &str) -> QueryResult<Vec<String>> {
    let stemmed = stem_text(query, language);
    conn.with(|c| {
        diesel::sql_query("SELECT post_id FROM posts_fts WHERE posts_fts MATCH ? ORDER BY rank")
            .bind::<Text, _>(stemmed)
            .load::<PostIdRow>(c)
            .map(|rows| rows.into_iter().map(|row| row.post_id).collect())
    })
}

/// Filters for post search.
#[derive(Default)]
pub struct PostSearchFilters<'a> {
    pub status: Option<&'a str>,
    pub tags: Option<&'a [String]>,
    pub categories: Option<&'a [String]>,
    pub language: Option<&'a str>,
    pub missing_translation_language: Option<&'a str>,
    pub year: Option<i32>,
    pub month: Option<u32>,
    pub from: Option<i64>,
    pub to: Option<i64>,
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
) -> QueryResult<SearchResults> {
    // Get FTS matches first
    let fts_ids = search_posts(conn, query, language)?;
    if fts_ids.is_empty() {
        return Ok(SearchResults {
            post_ids: vec![],
            total: 0,
            offset: filters.offset.unwrap_or(0),
            limit: filters.limit.unwrap_or(0),
        });
    }

    let offset = filters.offset.unwrap_or(0);
    let mut post_ids = conn.with(|c| {
        let mut query = posts::table.filter(posts::id.eq_any(fts_ids)).into_boxed();
        if let Some(status) = filters.status {
            query = query.filter(posts::status.eq(status));
        }
        if let Some(tags) = filters.tags {
            for tag in tags {
                query = query.filter(
                    instr(
                        lower(posts::tags),
                        serde_json::to_string(&tag.to_lowercase()).unwrap(),
                    )
                    .gt(0),
                );
            }
        }
        if let Some(categories) = filters.categories {
            for category in categories {
                query = query.filter(
                    instr(
                        lower(posts::categories),
                        serde_json::to_string(&category.to_lowercase()).unwrap(),
                    )
                    .gt(0),
                );
            }
        }
        if let Some(year) = filters.year {
            let (start, end) = calendar_range_unix_ms(year, filters.month).ok_or_else(|| {
                diesel::result::Error::SerializationError("invalid calendar range".into())
            })?;
            query = query.filter(posts::created_at.ge(start).and(posts::created_at.lt(end)));
        }
        if let Some(language) = filters.language {
            query = query.filter(posts::language.eq(language).or(posts::language.is_null()));
        }
        if let Some(language) = filters.missing_translation_language {
            query = query.filter(diesel::dsl::not(diesel::dsl::exists(
                post_translations::table
                    .filter(post_translations::translation_for.eq(posts::id))
                    .filter(post_translations::language.eq(language)),
            )));
        }
        if let Some(from) = filters.from {
            query = query.filter(posts::created_at.ge(from));
        }
        if let Some(to) = filters.to {
            query = query.filter(posts::created_at.le(to));
        }
        query
            .order(posts::created_at.desc())
            .select(posts::id)
            .load::<String>(c)
    })?;
    let total = post_ids.len();
    let limit = filters.limit.unwrap_or(total);
    post_ids = post_ids.into_iter().skip(offset).take(limit).collect();
    Ok(SearchResults {
        post_ids,
        total,
        offset,
        limit,
    })
}

/// Search media by full-text query. Returns matching media IDs.
pub fn search_media(conn: &Connection, query: &str, language: &str) -> QueryResult<Vec<String>> {
    let stemmed = stem_text(query, language);
    conn.with(|c| {
        diesel::sql_query("SELECT media_id FROM media_fts WHERE media_fts MATCH ? ORDER BY rank")
            .bind::<Text, _>(stemmed)
            .load::<MediaIdRow>(c)
            .map(|rows| rows.into_iter().map(|row| row.media_id).collect())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::post::{insert_post, make_test_post};
    use crate::model::PostStatus;

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        ensure_fts_tables(db.conn()).unwrap();
        db
    }

    fn insert_test_post(
        db: &Database,
        id: &str,
        slug: &str,
        title: &str,
        status: PostStatus,
        timestamp: i64,
    ) {
        let mut post = make_test_post(id, "p1", slug);
        post.title = title.into();
        post.status = status;
        post.created_at = timestamp;
        post.updated_at = timestamp;
        insert_post(db.conn(), &post).unwrap();
    }

    #[test]
    fn fts_tables_created() {
        let db = setup();
        assert!(search_posts(db.conn(), "nothing", "en").unwrap().is_empty());
        assert!(search_media(db.conn(), "nothing", "en").unwrap().is_empty());
    }

    #[test]
    fn fts_tables_idempotent() {
        let db = setup();
        ensure_fts_tables(db.conn()).unwrap(); // second call should not error
    }

    #[test]
    fn fts_multi_column_schema() {
        let db = setup();
        index_post(
            db.conn(),
            "p1",
            "T",
            Some("E"),
            Some("C"),
            &["tg".into()],
            &["ct".into()],
            &[],
            "en",
        )
        .unwrap();
        index_media(
            db.conn(),
            "m1",
            Some("T"),
            Some("A"),
            Some("C"),
            "N",
            &["tg".into()],
            &[],
            "en",
        )
        .unwrap();
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
            &[PostTranslationFts {
                title: "Esmeralda English".into(),
                excerpt: None,
                content: None,
                language: "en".into(),
            }],
            "en",
        )
        .unwrap();

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
        )
        .unwrap();

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
        index_post(db.conn(), "p1", "Test", None, None, &[], &[], &[], "en").unwrap();
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
            db.conn(),
            "p1",
            "Programmierung",
            None,
            Some("Deutsche Entwicklung"),
            &[],
            &[],
            &[PostTranslationFts {
                title: "English development programming".into(),
                excerpt: None,
                content: Some("English development programming".into()),
                language: "en".into(),
            }],
            "de",
        )
        .unwrap();

        // Search with English stemming should find via English translation
        let results = search_posts(db.conn(), "develop", "en").unwrap();
        assert_eq!(results, vec!["p1"]);
    }

    #[test]
    fn search_by_title_field() {
        let db = setup();
        index_post(
            db.conn(),
            "p1",
            "Unique Title Here",
            None,
            Some("body text"),
            &[],
            &[],
            &[],
            "en",
        )
        .unwrap();

        // Search for title content
        let results = search_posts(db.conn(), "unique", "en").unwrap();
        assert_eq!(results, vec!["p1"]);
    }

    #[test]
    fn search_by_tags() {
        let db = setup();
        index_post(
            db.conn(),
            "p1",
            "Post",
            None,
            None,
            &["photography".into()],
            &[],
            &[],
            "en",
        )
        .unwrap();

        let results = search_posts(db.conn(), "photography", "en").unwrap();
        assert_eq!(results, vec!["p1"]);
    }

    #[test]
    fn search_by_categories() {
        let db = setup();
        index_post(
            db.conn(),
            "p1",
            "Post",
            None,
            None,
            &[],
            &["article".into()],
            &[],
            "en",
        )
        .unwrap();

        let results = search_posts(db.conn(), "article", "en").unwrap();
        assert_eq!(results, vec!["p1"]);
    }

    #[test]
    fn search_filtered_by_status() {
        let db = setup();
        // Insert a post in the posts table (needed for the filter query to join)
        use crate::db::queries::project::{insert_project, make_test_project};
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        insert_test_post(
            &db,
            "post1",
            "test",
            "Test Post",
            PostStatus::Published,
            1700000000000,
        );
        insert_test_post(
            &db,
            "post2",
            "draft-post",
            "Draft Post",
            PostStatus::Draft,
            1700000000000,
        );

        index_post(
            db.conn(),
            "post1",
            "Test Post",
            None,
            None,
            &[],
            &[],
            &[],
            "en",
        )
        .unwrap();
        index_post(
            db.conn(),
            "post2",
            "Draft Post",
            None,
            None,
            &[],
            &[],
            &[],
            "en",
        )
        .unwrap();

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

        insert_test_post(
            &db,
            "p2024",
            "y2024",
            "Year 2024",
            PostStatus::Draft,
            ts_2024,
        );
        insert_test_post(
            &db,
            "p2023",
            "y2023",
            "Year 2023",
            PostStatus::Draft,
            ts_2023,
        );

        index_post(
            db.conn(),
            "p2024",
            "Year 2024",
            None,
            None,
            &[],
            &[],
            &[],
            "en",
        )
        .unwrap();
        index_post(
            db.conn(),
            "p2023",
            "Year 2023",
            None,
            None,
            &[],
            &[],
            &[],
            "en",
        )
        .unwrap();

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
            insert_test_post(
                &db,
                &id,
                &slug,
                "Searchable",
                PostStatus::Draft,
                1700000000000i64 - i as i64 * 1000,
            );
            index_post(
                db.conn(),
                &id,
                "Searchable",
                None,
                None,
                &[],
                &[],
                &[],
                "en",
            )
            .unwrap();
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
