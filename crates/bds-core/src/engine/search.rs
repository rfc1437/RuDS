//! Full-text search reindexing engine functions.

use rusqlite::Connection;

use crate::db::fts;
use crate::db::queries::{media as media_q, media_translation, post as post_q, post_translation};
use crate::engine::EngineResult;

/// Result of a full reindex operation.
pub struct ReindexReport {
    pub posts_indexed: usize,
    pub media_indexed: usize,
}

/// Per-item progress callback: (current_item, total_items, item_description).
pub type ItemProgressFn = Box<dyn Fn(usize, usize, &str) + Send>;

/// Drop and rebuild the entire FTS index for all posts and media in a project.
pub fn reindex_all(
    conn: &Connection,
    project_id: &str,
    main_language: &str,
) -> EngineResult<ReindexReport> {
    reindex_all_with_progress(conn, project_id, main_language, None)
}

/// Like `reindex_all` but with optional per-item progress.
pub fn reindex_all_with_progress(
    conn: &Connection,
    project_id: &str,
    main_language: &str,
    on_item: Option<ItemProgressFn>,
) -> EngineResult<ReindexReport> {
    // Wipe existing FTS content
    conn.execute("DELETE FROM posts_fts", [])?;
    conn.execute("DELETE FROM media_fts", [])?;

    // Reindex all posts
    let posts = post_q::list_posts_by_project(conn, project_id)?;

    // Reindex all media
    let media_items = media_q::list_media_by_project(conn, project_id)?;

    let total = posts.len() + media_items.len();

    let mut posts_indexed = 0;
    for (i, post) in posts.iter().enumerate() {
        if let Some(ref cb) = on_item {
            cb(i + 1, total, &post.title);
        }
        let translations = post_translation::list_post_translations_by_post(conn, &post.id)?;

        let trans_pairs: Vec<(String, String)> = translations
            .iter()
            .map(|t| {
                let text = [
                    t.title.as_str(),
                    t.excerpt.as_deref().unwrap_or(""),
                    t.content.as_deref().unwrap_or(""),
                ]
                .join(" ");
                (text, t.language.clone())
            })
            .collect();

        let language = post.language.as_deref().unwrap_or(main_language);
        fts::index_post(
            conn,
            &post.id,
            &post.title,
            post.excerpt.as_deref(),
            post.content.as_deref(),
            &post.tags,
            &post.categories,
            &trans_pairs,
            language,
        )?;

        posts_indexed += 1;
    }

    let offset = posts.len();
    let mut media_indexed = 0;
    for (i, m) in media_items.iter().enumerate() {
        if let Some(ref cb) = on_item {
            cb(offset + i + 1, total, &m.original_name);
        }
        let translations = media_translation::list_media_translations_by_media(conn, &m.id)?;

        let trans_pairs: Vec<(String, String)> = translations
            .iter()
            .map(|t| {
                let text = [
                    t.title.as_deref().unwrap_or(""),
                    t.alt.as_deref().unwrap_or(""),
                    t.caption.as_deref().unwrap_or(""),
                ]
                .join(" ");
                (text, t.language.clone())
            })
            .collect();

        let language = m.language.as_deref().unwrap_or(main_language);
        fts::index_media(
            conn,
            &m.id,
            m.title.as_deref(),
            m.alt.as_deref(),
            m.caption.as_deref(),
            &m.original_name,
            &m.tags,
            &trans_pairs,
            language,
        )?;

        media_indexed += 1;
    }

    Ok(ReindexReport {
        posts_indexed,
        media_indexed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::fts::ensure_fts_tables;
    use crate::engine;

    fn setup() -> (Database, String) {
        let db = Database::open_in_memory().unwrap();
        let _ = db.migrate();
        ensure_fts_tables(db.conn()).unwrap();

        let tmp = tempfile::tempdir().unwrap();
        let project = engine::project::create_project(
            db.conn(),
            "Test Project",
            Some(tmp.path().to_str().unwrap()),
        )
        .unwrap();

        (db, project.id)
    }

    #[test]
    fn reindex_empty_project() {
        let (db, project_id) = setup();
        let report = reindex_all(db.conn(), &project_id, "en").unwrap();
        assert_eq!(report.posts_indexed, 0);
        assert_eq!(report.media_indexed, 0);
    }

    #[test]
    fn reindex_with_posts() {
        let (db, project_id) = setup();
        let tmp = tempfile::tempdir().unwrap();

        engine::post::create_post(
            db.conn(),
            tmp.path(),
            &project_id,
            "Test Post",
            Some("Body content"),
            vec!["tag1".into()],
            vec![],
            None,
            None,
            None,
        )
        .unwrap();

        let report = reindex_all(db.conn(), &project_id, "en").unwrap();
        assert_eq!(report.posts_indexed, 1);
        assert_eq!(report.media_indexed, 0);

        // Verify searchable
        let results = crate::db::fts::search_posts(db.conn(), "test", "en").unwrap();
        assert_eq!(results.len(), 1);
    }
}
