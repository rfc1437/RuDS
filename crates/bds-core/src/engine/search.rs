//! Full-text search reindexing engine functions.

use std::fs;
use std::path::Path;

use crate::db::DbConnection as Connection;

use crate::db::fts;
use crate::db::queries::{
    media as media_q, media_translation, post as post_q, post_translation, project as project_q,
    setting,
};
use crate::engine::{EngineError, EngineResult, domain_events};
use crate::util::frontmatter::{read_post_file, read_translation_file};
use crate::util::now_unix_ms;

const REBUILD_REQUIRED_SETTING: &str = "app.search-index-rebuild-required";

/// Deterministic offline language fallback used when no permitted AI endpoint
/// is configured. This intentionally mirrors the legacy application's small
/// heuristic; it is a notice-worthy fallback, not a language model.
pub fn detect_language(text: &str) -> &'static str {
    let normalized = text.to_lowercase();
    if normalized.trim().is_empty() {
        "en"
    } else if normalized.contains(['ä', 'ö', 'ü', 'ß']) {
        "de"
    } else if normalized.contains([
        'à', 'â', 'ç', 'é', 'è', 'ê', 'ë', 'î', 'ï', 'ô', 'ù', 'û', 'ÿ', 'œ',
    ]) {
        "fr"
    } else if normalized.contains(['ñ', '¡', '¿']) {
        "es"
    } else {
        detect_language_from_hints(&normalized)
    }
}

fn detect_language_from_hints(text: &str) -> &'static str {
    let padded = format!(" {text} ");
    let scores = [
        (
            "de",
            [" der ", " die ", " das ", " und ", " ist ", " nicht "],
        ),
        ("fr", [" le ", " la ", " les ", " et ", " est ", " pas "]),
        ("es", [" el ", " la ", " los ", " y ", " es ", " no "]),
    ];
    scores
        .into_iter()
        .map(|(language, hints)| {
            let score = hints
                .into_iter()
                .filter(|hint| padded.contains(hint))
                .count();
            (language, score)
        })
        .max_by_key(|(_, score)| *score)
        .filter(|(_, score)| *score >= 2)
        .map_or("en", |(language, _)| language)
}

/// Result of a full reindex operation.
pub struct ReindexReport {
    pub posts_indexed: usize,
    pub media_indexed: usize,
}

/// Per-item progress callback: (current_item, total_items, item_description).
pub type ItemProgressFn = Box<dyn Fn(usize, usize, &str) + Send>;

/// Repair a missing or previously deployed FTS schema and report whether its
/// derived content still needs to be rebuilt.
pub fn prepare_search_index(conn: &Connection) -> EngineResult<bool> {
    if fts::schema_is_current(conn)? {
        return search_index_rebuild_required(conn);
    }

    let projects = project_q::list_projects(conn)?;
    let has_content = projects.iter().try_fold(false, |has_content, project| {
        Ok::<_, diesel::result::Error>(
            has_content
                || post_q::count_posts_by_project(conn, &project.id)? > 0
                || media_q::count_media_by_project(conn, &project.id)? > 0,
        )
    })?;

    conn.begin_savepoint()?;
    let result = (|| {
        fts::recreate_tables(conn)?;
        setting::set_setting_value(
            conn,
            REBUILD_REQUIRED_SETTING,
            if has_content { "true" } else { "false" },
            now_unix_ms(),
        )?;
        Ok::<_, diesel::result::Error>(())
    })();
    match result {
        Ok(()) => conn.release_savepoint()?,
        Err(error) => {
            let _ = conn.rollback_savepoint();
            return Err(error.into());
        }
    }
    domain_events::settings_changed(None, REBUILD_REQUIRED_SETTING);

    Ok(has_content)
}

pub fn search_index_rebuild_required(conn: &Connection) -> EngineResult<bool> {
    match setting::get_setting_by_key(conn, REBUILD_REQUIRED_SETTING) {
        Ok(value) => Ok(value.value == "true"),
        Err(diesel::result::Error::NotFound) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Atomically recreate and rebuild the shared FTS index for every project.
pub fn rebuild_search_index(
    conn: &Connection,
    on_item: Option<ItemProgressFn>,
) -> EngineResult<ReindexReport> {
    conn.begin_savepoint()?;
    let result = (|| {
        fts::recreate_tables(conn)?;
        let report = index_all_projects(conn, on_item.as_ref())?;
        setting::set_setting_value(conn, REBUILD_REQUIRED_SETTING, "false", now_unix_ms())?;
        Ok(report)
    })();
    match result {
        Ok(report) => {
            conn.release_savepoint()?;
            domain_events::settings_changed(None, REBUILD_REQUIRED_SETTING);
            Ok(report)
        }
        Err(error) => {
            let _ = conn.rollback_savepoint();
            Err(error)
        }
    }
}

/// Reindex one project without disturbing rows belonging to other projects.
pub fn reindex_project(
    conn: &Connection,
    project_id: &str,
    on_item: Option<ItemProgressFn>,
) -> EngineResult<ReindexReport> {
    let project = project_q::get_project_by_id(conn, project_id)?;
    let total = post_q::count_posts_by_project(conn, project_id)? as usize
        + media_q::count_media_by_project(conn, project_id)? as usize;
    let mut current = 0;
    index_project(
        conn,
        project_id,
        project.data_path.as_deref(),
        total,
        &mut current,
        on_item.as_ref(),
    )
}

fn index_all_projects(
    conn: &Connection,
    on_item: Option<&ItemProgressFn>,
) -> EngineResult<ReindexReport> {
    let projects = project_q::list_projects(conn)?;
    let total = projects.iter().try_fold(0usize, |total, project| {
        Ok::<_, diesel::result::Error>(
            total
                + post_q::count_posts_by_project(conn, &project.id)? as usize
                + media_q::count_media_by_project(conn, &project.id)? as usize,
        )
    })?;
    let mut current = 0;
    let mut report = ReindexReport {
        posts_indexed: 0,
        media_indexed: 0,
    };

    for project in projects {
        let indexed = index_project(
            conn,
            &project.id,
            project.data_path.as_deref(),
            total,
            &mut current,
            on_item,
        )?;
        report.posts_indexed += indexed.posts_indexed;
        report.media_indexed += indexed.media_indexed;
    }

    Ok(report)
}

fn index_project(
    conn: &Connection,
    project_id: &str,
    data_path: Option<&str>,
    total: usize,
    current: &mut usize,
    on_item: Option<&ItemProgressFn>,
) -> EngineResult<ReindexReport> {
    let data_dir = data_path.map(Path::new);
    let main_language = data_dir
        .and_then(|path| crate::engine::meta::read_project_json(path).ok())
        .and_then(|metadata| metadata.main_language)
        .unwrap_or_else(|| "en".to_string());
    let mut report = ReindexReport {
        posts_indexed: 0,
        media_indexed: 0,
    };

    for post in post_q::list_posts_by_project(conn, project_id)? {
        *current += 1;
        if let Some(callback) = on_item {
            callback(*current, total, &post.title);
        }
        let translations = post_translation::list_post_translations_by_post(conn, &post.id)?;
        let translation_data = translations
            .iter()
            .map(|translation| {
                Ok(fts::PostTranslationFts {
                    title: translation.title.clone(),
                    excerpt: translation.excerpt.clone(),
                    content: data_dir
                        .map(|dir| resolve_translation_fts_content(dir, translation))
                        .transpose()?
                        .flatten()
                        .or_else(|| translation.content.clone()),
                    language: translation.language.clone(),
                })
            })
            .collect::<EngineResult<Vec<_>>>()?;
        let content = data_dir
            .map(|dir| resolve_post_fts_content(dir, &post))
            .transpose()?
            .flatten()
            .or_else(|| post.content.clone());
        fts::index_post(
            conn,
            &post.id,
            &post.title,
            post.excerpt.as_deref(),
            content.as_deref(),
            &post.tags,
            &post.categories,
            &translation_data,
            post.language.as_deref().unwrap_or(&main_language),
        )?;
        report.posts_indexed += 1;
    }

    for media in media_q::list_media_by_project(conn, project_id)? {
        *current += 1;
        if let Some(callback) = on_item {
            callback(*current, total, &media.original_name);
        }
        let translations = media_translation::list_media_translations_by_media(conn, &media.id)?;
        let translation_data = translations
            .iter()
            .map(|translation| fts::MediaTranslationFts {
                title: translation.title.clone(),
                alt: translation.alt.clone(),
                caption: translation.caption.clone(),
                language: translation.language.clone(),
            })
            .collect::<Vec<_>>();
        fts::index_media(
            conn,
            &media.id,
            media.title.as_deref(),
            media.alt.as_deref(),
            media.caption.as_deref(),
            &media.original_name,
            &media.tags,
            &translation_data,
            media.language.as_deref().unwrap_or(&main_language),
        )?;
        report.media_indexed += 1;
    }

    Ok(report)
}

fn resolve_post_fts_content(
    data_dir: &Path,
    post: &crate::model::Post,
) -> EngineResult<Option<String>> {
    if post.content.is_some() {
        return Ok(post.content.clone());
    }
    if post.file_path.is_empty() {
        return Ok(None);
    }
    let raw = fs::read_to_string(data_dir.join(&post.file_path))?;
    let (_fm, body) = read_post_file(&raw).map_err(EngineError::Parse)?;
    Ok(Some(body))
}

fn resolve_translation_fts_content(
    data_dir: &Path,
    translation: &crate::model::PostTranslation,
) -> EngineResult<Option<String>> {
    if translation.content.is_some() {
        return Ok(translation.content.clone());
    }
    if translation.file_path.is_empty() {
        return Ok(None);
    }
    let raw = fs::read_to_string(data_dir.join(&translation.file_path))?;
    let (_fm, body) = read_translation_file(&raw).map_err(EngineError::Parse)?;
    Ok(Some(body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::fts::{self, ensure_fts_tables};
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
    fn rebuild_empty_index() {
        let (db, _) = setup();
        let report = rebuild_search_index(db.conn(), None).unwrap();
        assert_eq!(report.posts_indexed, 0);
        assert_eq!(report.media_indexed, 0);
    }

    #[test]
    fn rebuild_with_posts() {
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

        let report = rebuild_search_index(db.conn(), None).unwrap();
        assert_eq!(report.posts_indexed, 1);
        assert_eq!(report.media_indexed, 0);

        // Verify searchable
        let results = crate::db::fts::search_posts(db.conn(), "test", "en").unwrap();
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn rebuild_indexes_published_post_and_translation_bodies_from_files() {
        let db = Database::open_in_memory().unwrap();
        let _ = db.migrate();
        ensure_fts_tables(db.conn()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let project = engine::project::create_project(
            db.conn(),
            "Published Project",
            Some(dir.path().to_str().unwrap()),
        )
        .unwrap();
        let post = engine::post::create_post(
            db.conn(),
            dir.path(),
            &project.id,
            "Published Rebuild",
            Some("distinctive platypusnova body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        engine::post::upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "Veröffentlichter Neuaufbau",
            None,
            Some("markantes schmetterlingskomet wort"),
        )
        .unwrap();
        engine::post::publish_post(db.conn(), dir.path(), &post.id).unwrap();
        fts::remove_post_from_index(db.conn(), &post.id).unwrap();

        let report = reindex_project(db.conn(), &project.id, None).unwrap();

        assert_eq!(report.posts_indexed, 1);
        assert_eq!(
            fts::search_posts(db.conn(), "platypusnova", "en").unwrap(),
            vec![post.id.clone()]
        );
        assert_eq!(
            fts::search_posts(db.conn(), "schmetterlingskomet", "de").unwrap(),
            vec![post.id]
        );
    }

    #[test]
    fn shared_rebuild_indexes_every_project() {
        let (db, first_project_id) = setup();
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let second_project = engine::project::create_project(
            db.conn(),
            "Second Project",
            Some(second_dir.path().to_str().unwrap()),
        )
        .unwrap();

        for (project_id, data_dir, title) in [
            (
                &first_project_id,
                first_dir.path(),
                "First Project Searchable",
            ),
            (
                &second_project.id,
                second_dir.path(),
                "Second Project Searchable",
            ),
        ] {
            engine::post::create_post(
                db.conn(),
                data_dir,
                project_id,
                title,
                Some("body"),
                vec![],
                vec![],
                None,
                Some("en"),
                None,
            )
            .unwrap();
        }

        let report = rebuild_search_index(db.conn(), None).unwrap();
        let results = crate::db::fts::search_posts(db.conn(), "searchable", "en").unwrap();

        assert_eq!(report.posts_indexed, 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn project_reindex_leaves_other_project_rows_intact() {
        let (db, first_project_id) = setup();
        let first_dir = tempfile::tempdir().unwrap();
        let second_dir = tempfile::tempdir().unwrap();
        let second = engine::project::create_project(
            db.conn(),
            "Second Project",
            Some(second_dir.path().to_str().unwrap()),
        )
        .unwrap();
        for (project_id, data_dir, title) in [
            (&first_project_id, first_dir.path(), "First Searchable"),
            (&second.id, second_dir.path(), "Second Searchable"),
        ] {
            engine::post::create_post(
                db.conn(),
                data_dir,
                project_id,
                title,
                Some("body"),
                vec![],
                vec![],
                None,
                Some("en"),
                None,
            )
            .unwrap();
        }

        reindex_project(db.conn(), &first_project_id, None).unwrap();
        let results = crate::db::fts::search_posts(db.conn(), "searchable", "en").unwrap();

        assert_eq!(results.len(), 2);
    }
}
