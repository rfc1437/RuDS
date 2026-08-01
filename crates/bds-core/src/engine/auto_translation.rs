use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::db::DbConnection as Connection;
use crate::db::queries::{
    media as qm, media_translation as qmt, post as qp, post_media, post_translation,
};
use crate::engine::ai::{self, MediaTranslationResult, OneShotResponse, TranslationResult};
use crate::engine::{EngineError, EngineResult};
use crate::i18n::{UiLocale, translate, translate_with};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FillMissingTranslationsProgress {
    ScanningPublishedPosts,
    TranslatingPost { title: String, language: String },
    Complete,
}

impl FillMissingTranslationsProgress {
    pub fn localized(&self, locale: UiLocale) -> String {
        match self {
            Self::ScanningPublishedPosts => {
                translate(locale, "engine.progress.scanningPublishedPosts")
            }
            Self::TranslatingPost { title, language } => translate_with(
                locale,
                "engine.progress.translatingPost",
                &[("title", title), ("language", language)],
            ),
            Self::Complete => translate(locale, "engine.progress.translationBatchComplete"),
        }
    }
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
    mut on_progress: impl FnMut(f32, &FillMissingTranslationsProgress) -> bool,
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
    on_progress: &mut dyn FnMut(f32, &FillMissingTranslationsProgress) -> bool,
) -> EngineResult<FillMissingTranslationsReport> {
    let configured = configured_languages(main_language, blog_languages);
    if configured.len() <= 1 {
        return Ok(FillMissingTranslationsReport {
            nothing_to_do: true,
            ..Default::default()
        });
    }
    let posts = qp::list_posts_by_project(conn, project_id)?;
    if !on_progress(
        0.0,
        &FillMissingTranslationsProgress::ScanningPublishedPosts,
    ) {
        return Err(EngineError::Validation("cancelled".to_string()));
    }
    let mut work = Vec::new();
    for post in posts
        .into_iter()
        .filter(|post| post.status == PostStatus::Published && !post.do_not_translate)
    {
        if !on_progress(
            0.0,
            &FillMissingTranslationsProgress::ScanningPublishedPosts,
        ) {
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
            &FillMissingTranslationsProgress::TranslatingPost {
                title: post.title.clone(),
                language: language.clone(),
            },
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
    if !on_progress(1.0, &FillMissingTranslationsProgress::Complete) {
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
        merge_reactive_translation_result(
            &mut report,
            &post,
            &language,
            translate_one_post(
                conn,
                data_dir,
                &post,
                &language,
                false,
                &mut |post, language| translate_post_ai(conn, offline_mode, post, language),
                &mut |media, language| translate_media_ai(conn, offline_mode, media, language),
            ),
        );
    }
    Ok(report)
}

/// Translate one language from the configured target set. This is the reactive
/// editor path: generated translations remain drafts, and a retry after the
/// post translation was saved resumes any still-missing media translations.
pub fn translate_missing_language_for_post(
    conn: &Connection,
    data_dir: &Path,
    post_id: &str,
    configured_languages: &[String],
    language: &str,
    offline_mode: bool,
    is_cancelled: impl Fn() -> bool,
) -> EngineResult<FillMissingTranslationsReport> {
    translate_missing_language_for_post_with(
        conn,
        data_dir,
        post_id,
        configured_languages,
        language,
        &mut |post, language| translate_post_ai(conn, offline_mode, post, language),
        &mut |media, language| translate_media_ai(conn, offline_mode, media, language),
        is_cancelled,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "testable translation orchestration dependencies"
)]
fn translate_missing_language_for_post_with(
    conn: &Connection,
    data_dir: &Path,
    post_id: &str,
    configured_languages: &[String],
    language: &str,
    post_translator: &mut dyn FnMut(&Post, &str) -> EngineResult<TranslationResult>,
    media_translator: &mut dyn FnMut(&Media, &str) -> EngineResult<MediaTranslationResult>,
    is_cancelled: impl Fn() -> bool,
) -> EngineResult<FillMissingTranslationsReport> {
    let post = qp::get_post_by_id(conn, post_id)?;
    let language = normalize_language(language);
    let source = normalize_language(post.language.as_deref().unwrap_or("en"));
    if post.do_not_translate
        || language == source
        || !configured_languages
            .iter()
            .any(|configured| normalize_language(configured) == language)
    {
        return Ok(FillMissingTranslationsReport {
            nothing_to_do: true,
            ..Default::default()
        });
    }
    if is_cancelled() {
        return Err(EngineError::Validation("cancelled".to_string()));
    }
    let translation_exists =
        post_translation::get_post_translation_by_post_and_language(conn, &post.id, &language)
            .is_ok();
    let mut report = FillMissingTranslationsReport::default();
    let result = if translation_exists {
        translate_missing_media(conn, data_dir, &post, &language, media_translator)
    } else {
        translate_one_post(
            conn,
            data_dir,
            &post,
            &language,
            false,
            post_translator,
            media_translator,
        )
    };
    match result {
        Ok(media_count) => {
            report.translated_posts += usize::from(!translation_exists);
            report.translated_media += media_count;
            report.nothing_to_do = translation_exists && media_count == 0;
        }
        Err(error) => {
            report.failed_count += 1;
            report
                .errors
                .push(format!("{} ({language}): {error}", post.title));
        }
    }
    Ok(report)
}

fn merge_reactive_translation_result(
    report: &mut FillMissingTranslationsReport,
    post: &Post,
    language: &str,
    result: EngineResult<usize>,
) {
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
    for (field, source, translated_value) in [
        ("title", post.title.as_str(), translated.title.as_str()),
        (
            "excerpt",
            post.excerpt.as_deref().unwrap_or(""),
            translated.excerpt.as_str(),
        ),
        (
            "content",
            input.content.as_deref().unwrap_or(""),
            translated.content.as_str(),
        ),
    ] {
        if !source.trim().is_empty() && translated_value.trim().is_empty() {
            return Err(EngineError::Validation(format!(
                "post translation returned empty {field}"
            )));
        }
    }
    let translation = crate::engine::post::upsert_automatic_translation(
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

    translate_missing_media(conn, data_dir, post, language, media_translator)
}

fn translate_missing_media(
    conn: &Connection,
    data_dir: &Path,
    post: &Post,
    language: &str,
    media_translator: &mut dyn FnMut(&Media, &str) -> EngineResult<MediaTranslationResult>,
) -> EngineResult<usize> {
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
        &ai::post_translation_request(
            &post.title,
            post.excerpt.as_deref(),
            post.content.as_deref().unwrap_or_default(),
            language,
        ),
    )? {
        (OneShotResponse::Translation(result), _usage) => Ok(result),
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
        &ai::media_translation_request(
            media.title.as_deref(),
            media.alt.as_deref(),
            media.caption.as_deref(),
            language,
        ),
    )? {
        (OneShotResponse::MediaTranslation(result), _usage) => Ok(result),
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
    use crate::db::queries::media::{insert_media, make_test_media};
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::engine::post::{create_post, publish_post, upsert_translation};
    use crate::model::PostMedia;
    use tempfile::TempDir;

    #[test]
    fn progress_events_use_the_selected_ui_locale() {
        assert_eq!(
            FillMissingTranslationsProgress::ScanningPublishedPosts.localized(UiLocale::De),
            "Veröffentlichte Beiträge werden durchsucht…"
        );
        assert_eq!(
            FillMissingTranslationsProgress::TranslatingPost {
                title: "Hallo".into(),
                language: "fr".into(),
            }
            .localized(UiLocale::Fr),
            "Traduction de Hallo → fr"
        );
        assert_eq!(
            FillMissingTranslationsProgress::Complete.localized(UiLocale::Es),
            "Lote de traducciones completado"
        );
    }

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
        let canonical = crate::db::queries::post::get_post_by_id(db.conn(), &post.id).unwrap();
        assert_eq!(canonical.status, PostStatus::Published);
        assert!(canonical.content.is_none());
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
    fn blank_ai_translation_is_rejected_without_creating_a_record() {
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
        let result = translate_one_post(
            db.conn(),
            dir.path(),
            &post,
            "de",
            false,
            &mut |_post, _language| {
                Ok(TranslationResult {
                    title: String::new(),
                    excerpt: String::new(),
                    content: String::new(),
                })
            },
            &mut |_media, _language| unreachable!(),
        );

        assert!(result.is_err_and(|error| error.to_string().contains("empty")));
        assert!(
            post_translation::get_post_translation_by_post_and_language(db.conn(), &post.id, "de")
                .is_err()
        );
    }

    #[test]
    fn automatic_translation_notifies_open_post_consumers() {
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
        let events = crate::engine::domain_events::subscribe();

        let translated_media = translate_one_post(
            db.conn(),
            dir.path(),
            &post,
            "de",
            false,
            &mut |_post, _language| {
                Ok(TranslationResult {
                    title: "Hallo".into(),
                    excerpt: "".into(),
                    content: "Inhalt".into(),
                })
            },
            &mut |_media, _language| unreachable!(),
        )
        .unwrap();

        assert_eq!(translated_media, 0);
        assert!(events.drain().iter().any(|event| matches!(
            event,
            crate::model::DomainEvent::EntityChanged {
                project_id,
                entity: crate::model::DomainEntity::Post,
                entity_id,
                action: crate::model::NotificationAction::Updated,
            } if project_id == "p1" && entity_id == &post.id
        )));
    }

    #[test]
    fn reactive_language_translation_is_a_no_op_when_translation_exists() {
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
        upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "Hallo",
            None,
            Some("Inhalt"),
        )
        .unwrap();

        let report = translate_missing_language_for_post(
            db.conn(),
            dir.path(),
            &post.id,
            &["en".to_string(), "de".to_string()],
            "de",
            true,
            || false,
        )
        .unwrap();

        assert!(report.nothing_to_do);
        assert_eq!(report.translated_posts, 0);
    }

    #[test]
    fn reactive_retry_resumes_missing_media_after_post_translation_was_saved() {
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
        upsert_translation(
            db.conn(),
            dir.path(),
            &post.id,
            "de",
            "Hallo",
            None,
            Some("Inhalt"),
        )
        .unwrap();
        let mut media = make_test_media("media-1", "p1");
        media.language = Some("en".to_string());
        media.file_path = "media/media-1.jpg".to_string();
        media.sidecar_path = "media/media-1.jpg.meta".to_string();
        insert_media(db.conn(), &media).unwrap();
        post_media::link_media(
            db.conn(),
            &PostMedia {
                id: "link-1".to_string(),
                project_id: "p1".to_string(),
                post_id: post.id.clone(),
                media_id: media.id.clone(),
                sort_order: 0,
                created_at: 1,
            },
        )
        .unwrap();

        let mut media_attempts = 0;
        let report = translate_missing_language_for_post_with(
            db.conn(),
            dir.path(),
            &post.id,
            &["en".to_string(), "de".to_string()],
            "de",
            &mut |_, _| panic!("the existing post translation must not be regenerated"),
            &mut |_, _| {
                media_attempts += 1;
                Ok(MediaTranslationResult {
                    title: "Foto".to_string(),
                    alt: "Bild".to_string(),
                    caption: "Beschreibung".to_string(),
                })
            },
            || false,
        )
        .unwrap();

        assert_eq!(media_attempts, 1);
        assert_eq!(report.translated_posts, 0);
        assert_eq!(report.translated_media, 1);
        assert!(!report.nothing_to_do);
        assert!(
            qmt::get_media_translation_by_media_and_language(db.conn(), &media.id, "de").is_ok()
        );
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
