use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde_json::json;

use crate::db::DbConnection as Connection;
use crate::db::queries::{
    media as qm, media_translation as qmt, post as qp, post_media, post_translation,
};
use crate::engine::ai::{
    self, MediaTranslationResult, OneShotOperation, OneShotRequest, OneShotResponse,
    TranslationResult,
};
use crate::engine::{EngineError, EngineResult};
use crate::model::{Media, Post, PostStatus};
use crate::util::frontmatter::read_post_file;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FillMissingTranslationsReport {
    pub translated_posts: usize,
    pub translated_media: usize,
    pub failed_count: usize,
    pub warned_count: usize,
    pub nothing_to_do: bool,
    pub errors: Vec<String>,
}

pub fn configured_languages(main_language: &str, blog_languages: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    std::iter::once(main_language.to_string())
        .chain(blog_languages.iter().cloned())
        .map(|language| normalize_language(&language))
        .filter(|language| !language.is_empty() && seen.insert(language.clone()))
        .collect()
}

pub fn missing_languages(
    conn: &Connection,
    post: &Post,
    configured: &[String],
) -> EngineResult<Vec<String>> {
    if post.do_not_translate {
        return Ok(Vec::new());
    }
    let source = normalize_language(post.language.as_deref().unwrap_or("en"));
    let existing = post_translation::list_post_translations_by_post(conn, &post.id)?
        .into_iter()
        .map(|translation| normalize_language(&translation.language))
        .collect::<HashSet<_>>();
    Ok(configured
        .iter()
        .filter(|language| **language != source && !existing.contains(*language))
        .cloned()
        .collect())
}

/// Batch maintenance path. Generated post translations are published, while
/// per-item failures are accumulated and never abort the batch.
pub fn fill_missing_translations(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    main_language: &str,
    blog_languages: &[String],
    offline_mode: bool,
    mut on_progress: impl FnMut(f32, &str) -> bool,
) -> EngineResult<FillMissingTranslationsReport> {
    fill_missing_translations_with(
        conn,
        data_dir,
        project_id,
        main_language,
        blog_languages,
        &mut |post, language| translate_post_ai(conn, offline_mode, post, language),
        &mut |media, language| translate_media_ai(conn, offline_mode, media, language),
        &mut on_progress,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "testable translation orchestration dependencies"
)]
fn fill_missing_translations_with(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    main_language: &str,
    blog_languages: &[String],
    post_translator: &mut dyn FnMut(&Post, &str) -> EngineResult<TranslationResult>,
    media_translator: &mut dyn FnMut(&Media, &str) -> EngineResult<MediaTranslationResult>,
    on_progress: &mut dyn FnMut(f32, &str) -> bool,
) -> EngineResult<FillMissingTranslationsReport> {
    let configured = configured_languages(main_language, blog_languages);
    if configured.len() <= 1 {
        return Ok(FillMissingTranslationsReport {
            nothing_to_do: true,
            ..Default::default()
        });
    }
    let posts = qp::list_posts_by_project(conn, project_id)?;
    if !on_progress(0.0, "Scanning published posts") {
        return Err(EngineError::Validation("cancelled".to_string()));
    }
    let mut work = Vec::new();
    for post in posts
        .into_iter()
        .filter(|post| post.status == PostStatus::Published && !post.do_not_translate)
    {
        if !on_progress(0.0, "Scanning published posts") {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
        for language in missing_languages(conn, &post, &configured)? {
            work.push((post.clone(), language));
        }
    }
    if work.is_empty() {
        return Ok(FillMissingTranslationsReport {
            nothing_to_do: true,
            ..Default::default()
        });
    }

    let mut report = FillMissingTranslationsReport::default();
    for (index, (post, language)) in work.iter().enumerate() {
        if !on_progress(
            0.15 + (index as f32 / work.len() as f32) * 0.85,
            &format!("{} → {language}", post.title),
        ) {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
        match translate_one_post(
            conn,
            data_dir,
            post,
            language,
            true,
            post_translator,
            media_translator,
        ) {
            Ok(media_count) => {
                report.translated_posts += 1;
                report.translated_media += media_count;
            }
            Err(error) => {
                report.failed_count += 1;
                report
                    .errors
                    .push(format!("{} ({language}): {error}", post.title));
            }
        }
    }
    if !on_progress(1.0, "Translation batch complete") {
        return Err(EngineError::Validation("cancelled".to_string()));
    }
    Ok(report)
}

/// Reactive manual-save path. Generated translations remain drafts.
pub fn translate_missing_for_post(
    conn: &Connection,
    data_dir: &Path,
    post_id: &str,
    main_language: &str,
    blog_languages: &[String],
    offline_mode: bool,
    is_cancelled: impl Fn() -> bool,
) -> EngineResult<FillMissingTranslationsReport> {
    let post = qp::get_post_by_id(conn, post_id)?;
    let configured = configured_languages(main_language, blog_languages);
    let targets = missing_languages(conn, &post, &configured)?;
    let mut report = FillMissingTranslationsReport {
        nothing_to_do: targets.is_empty(),
        ..Default::default()
    };
    for language in targets {
        if is_cancelled() {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
        let result = translate_one_post(
            conn,
            data_dir,
            &post,
            &language,
            false,
            &mut |post, language| translate_post_ai(conn, offline_mode, post, language),
            &mut |media, language| translate_media_ai(conn, offline_mode, media, language),
        );
        match result {
            Ok(media_count) => {
                report.translated_posts += 1;
                report.translated_media += media_count;
            }
            Err(error) => {
                report.failed_count += 1;
                report
                    .errors
                    .push(format!("{} ({language}): {error}", post.title));
            }
        }
    }
    Ok(report)
}

fn translate_one_post(
    conn: &Connection,
    data_dir: &Path,
    post: &Post,
    language: &str,
    auto_publish: bool,
    post_translator: &mut dyn FnMut(&Post, &str) -> EngineResult<TranslationResult>,
    media_translator: &mut dyn FnMut(&Media, &str) -> EngineResult<MediaTranslationResult>,
) -> EngineResult<usize> {
    if post.do_not_translate {
        return Ok(0);
    }
    let body = post_body(data_dir, post)?;
    if body.trim().is_empty() {
        return Err(EngineError::Validation("no content to translate".into()));
    }
    let mut input = post.clone();
    input.content = Some(body);
    let translated = post_translator(&input, language)?;
    let translation = crate::engine::post::upsert_translation(
        conn,
        data_dir,
        &post.id,
        language,
        &translated.title,
        Some(&translated.excerpt),
        Some(&translated.content),
    )?;
    if auto_publish {
        crate::engine::post::publish_post_translation(conn, data_dir, &translation.id)?;
    }

    let mut translated_media = 0;
    for link in post_media::list_post_media_by_post(conn, &post.id)? {
        let media = qm::get_media_by_id(conn, &link.media_id)?;
        let source = normalize_language(media.language.as_deref().unwrap_or(""));
        if source.is_empty() || source == language {
            continue;
        }
        if qmt::get_media_translation_by_media_and_language(conn, &media.id, language).is_ok() {
            continue;
        }
        let translated = media_translator(&media, language)?;
        crate::engine::media::upsert_media_translation(
            conn,
            data_dir,
            &media.id,
            language,
            Some(&translated.title),
            Some(&translated.alt),
            Some(&translated.caption),
        )?;
        translated_media += 1;
    }
    Ok(translated_media)
}

fn post_body(data_dir: &Path, post: &Post) -> EngineResult<String> {
    if let Some(content) = &post.content {
        return Ok(content.clone());
    }
    if post.file_path.is_empty() {
        return Ok(String::new());
    }
    let raw = fs::read_to_string(data_dir.join(&post.file_path))?;
    read_post_file(&raw)
        .map(|(_, body)| body)
        .map_err(EngineError::Parse)
}

fn translate_post_ai(
    conn: &Connection,
    offline_mode: bool,
    post: &Post,
    language: &str,
) -> EngineResult<TranslationResult> {
    match ai::run_one_shot(
        conn,
        offline_mode,
        &OneShotRequest {
            operation: OneShotOperation::TranslatePost {
                target_language: language.to_string(),
            },
            content: json!({
                "title": post.title,
                "excerpt": post.excerpt,
                "content": post.content,
            }),
        },
    )? {
        OneShotResponse::Translation(result) => Ok(result),
        _ => Err(EngineError::Parse(
            "unexpected post translation response".into(),
        )),
    }
}

fn translate_media_ai(
    conn: &Connection,
    offline_mode: bool,
    media: &Media,
    language: &str,
) -> EngineResult<MediaTranslationResult> {
    match ai::run_one_shot(
        conn,
        offline_mode,
        &OneShotRequest {
            operation: OneShotOperation::TranslateMedia {
                target_language: language.to_string(),
            },
            content: json!({
                "title": media.title,
                "alt": media.alt,
                "caption": media.caption,
            }),
        },
    )? {
        OneShotResponse::MediaTranslation(result) => Ok(result),
        _ => Err(EngineError::Parse(
            "unexpected media translation response".into(),
        )),
    }
}

fn normalize_language(language: &str) -> String {
    language
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .trim()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::fts::ensure_fts_tables;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::engine::post::{create_post, publish_post};
    use tempfile::TempDir;

    #[test]
    fn batch_translates_only_missing_languages_and_publishes() {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        ensure_fts_tables(db.conn()).unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = TempDir::new().unwrap();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Hello",
            Some("Body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap();
        let mut requested = Vec::new();
        let report = fill_missing_translations_with(
            db.conn(),
            dir.path(),
            "p1",
            "en",
            &["de".into(), "fr".into(), "de-DE".into()],
            &mut |_post, language| {
                requested.push(language.to_string());
                Ok(TranslationResult {
                    title: format!("Title {language}"),
                    excerpt: format!("Excerpt {language}"),
                    content: format!("Body {language}"),
                })
            },
            &mut |_media, _language| unreachable!(),
            &mut |_, _| true,
        )
        .unwrap();

        assert_eq!(requested, vec!["de", "fr"]);
        assert_eq!(report.translated_posts, 2);
        for language in ["de", "fr"] {
            let translation = post_translation::get_post_translation_by_post_and_language(
                db.conn(),
                &post.id,
                language,
            )
            .unwrap();
            assert_eq!(translation.status, PostStatus::Published);
            assert!(dir.path().join(&translation.file_path).is_file());
            assert!(translation.content.is_none());
        }
    }

    #[test]
    fn skips_do_not_translate_posts() {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        ensure_fts_tables(db.conn()).unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = TempDir::new().unwrap();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Private",
            Some("Body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let post = crate::engine::post::update_post(
            db.conn(),
            dir.path(),
            &post.id,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(true),
        )
        .unwrap();
        publish_post(db.conn(), dir.path(), &post.id).unwrap();
        let report = fill_missing_translations_with(
            db.conn(),
            dir.path(),
            "p1",
            "en",
            &["de".into()],
            &mut |_, _| panic!("translator must not run"),
            &mut |_, _| panic!("translator must not run"),
            &mut |_, _| true,
        )
        .unwrap();
        assert!(report.nothing_to_do);
    }

    #[test]
    fn batch_stops_when_progress_callback_cancels() {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = TempDir::new().unwrap();

        let result = fill_missing_translations_with(
            db.conn(),
            dir.path(),
            "p1",
            "en",
            &["de".into()],
            &mut |_, _| panic!("translator must not run"),
            &mut |_, _| panic!("translator must not run"),
            &mut |_, _| false,
        );

        assert!(matches!(result, Err(EngineError::Validation(message)) if message == "cancelled"));
    }
}
