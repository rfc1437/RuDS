//! Translation validation — checks DB rows and filesystem files for issues.

use std::path::Path;

use crate::db::DbConnection as Connection;

use crate::db::queries::{post as post_q, post_translation};
use crate::engine::EngineResult;
use crate::model::PostStatus;

/// Normalize a language code for comparison (lowercase, strip region).
fn norm_lang(code: &str) -> String {
    code.split(['-', '_']).next().unwrap_or("").to_lowercase()
}

/// Check if a file stem looks like a translation (e.g. "slug.de").
fn is_translation_stem(stem: &str) -> bool {
    if let Some(dot_pos) = stem.rfind('.') {
        let suffix = &stem[dot_pos + 1..];
        suffix.len() == 2 && suffix.chars().all(|c| c.is_ascii_lowercase())
    } else {
        false
    }
}

/// A single validation issue.
#[derive(Debug, Clone)]
pub struct TranslationIssue {
    pub post_id: String,
    pub translation_id: Option<String>,
    pub file_path: Option<String>,
    pub language: String,
    pub kind: TranslationIssueKind,
}

#[derive(Debug, Clone)]
pub enum TranslationIssueKind {
    MissingSourcePost,
    SameLanguageAsCanonical,
    DoNotTranslateHasTranslations,
    ContentInDatabase,
    /// Published post is missing a translation for a configured blog language.
    MissingTranslation,
}

/// Result of translation validation.
#[derive(Debug, Clone)]
pub struct TranslationValidationReport {
    pub db_issues: Vec<TranslationIssue>,
    pub fs_issues: Vec<TranslationIssue>,
    pub checked_db_rows: usize,
    pub checked_fs_files: usize,
}

/// Per-item progress callback: (current_item, total_items, item_description).
pub type ItemProgressFn = Box<dyn Fn(usize, usize, &str) + Send>;

/// Validate all translations in a project against consistency rules.
pub fn validate_translations(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    blog_languages: &[String],
    main_language: &str,
) -> EngineResult<TranslationValidationReport> {
    validate_translations_with_progress(
        conn,
        data_dir,
        project_id,
        blog_languages,
        main_language,
        None,
    )
}

/// Like `validate_translations` but with optional per-item progress.
pub fn validate_translations_with_progress(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    blog_languages: &[String],
    main_language: &str,
    on_item: Option<ItemProgressFn>,
) -> EngineResult<TranslationValidationReport> {
    let posts = post_q::list_posts_by_project(conn, project_id)?;

    let post_map: std::collections::HashMap<String, &crate::model::Post> =
        posts.iter().map(|p| (p.id.clone(), p)).collect();

    // Phase 1: database validation
    let mut db_issues = Vec::new();
    let mut checked_db_rows = 0usize;
    let post_count = posts.len();

    for (i, post) in posts.iter().enumerate() {
        if let Some(ref cb) = on_item {
            cb(i + 1, post_count, &post.title);
        }
        let translations = post_translation::list_post_translations_by_post(conn, &post.id)?;

        for t in &translations {
            checked_db_rows += 1;

            // Check: translation language != canonical language
            let post_lang = post.language.as_deref().unwrap_or("en");
            if norm_lang(post_lang) == norm_lang(&t.language) {
                db_issues.push(TranslationIssue {
                    post_id: post.id.clone(),
                    translation_id: Some(t.id.clone()),
                    file_path: Some(t.file_path.clone()),
                    language: t.language.clone(),
                    kind: TranslationIssueKind::SameLanguageAsCanonical,
                });
            }

            // Check: do_not_translate
            if post.do_not_translate {
                db_issues.push(TranslationIssue {
                    post_id: post.id.clone(),
                    translation_id: Some(t.id.clone()),
                    file_path: Some(t.file_path.clone()),
                    language: t.language.clone(),
                    kind: TranslationIssueKind::DoNotTranslateHasTranslations,
                });
            }

            // Check: published translation should not have content in DB
            if t.status == PostStatus::Published && t.content.is_some() {
                db_issues.push(TranslationIssue {
                    post_id: post.id.clone(),
                    translation_id: Some(t.id.clone()),
                    file_path: Some(t.file_path.clone()),
                    language: t.language.clone(),
                    kind: TranslationIssueKind::ContentInDatabase,
                });
            }
        }

        // Check: published, translatable posts must have translations
        // for each configured blog language (spec: ValidateTranslations rule)
        if post.status == PostStatus::Published && !post.do_not_translate {
            let available: std::collections::HashSet<String> = translations
                .iter()
                .map(|t| norm_lang(&t.language))
                .collect();
            let post_lang = norm_lang(post.language.as_deref().unwrap_or(main_language));
            let main_norm = norm_lang(main_language);

            for lang in blog_languages {
                let lang_norm = norm_lang(lang);
                if lang_norm == main_norm || lang_norm == post_lang {
                    continue;
                }
                if !available.contains(&lang_norm) {
                    db_issues.push(TranslationIssue {
                        post_id: post.id.clone(),
                        translation_id: None,
                        file_path: None,
                        language: lang.clone(),
                        kind: TranslationIssueKind::MissingTranslation,
                    });
                }
            }
        }
    }

    // Phase 2: filesystem validation
    let mut fs_issues = Vec::new();
    let mut checked_fs_files = 0usize;

    let posts_dir = data_dir.join("posts");
    if posts_dir.exists() {
        // Collect translation files first so we know the total
        let translation_files: Vec<_> = walkdir::WalkDir::new(&posts_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
            .filter(|e| {
                let stem = e.path().file_stem().and_then(|s| s.to_str()).unwrap_or("");
                is_translation_stem(stem)
            })
            .collect();

        let fs_total = translation_files.len();

        for (i, entry) in translation_files.iter().enumerate() {
            let path = entry.path();
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");

            if let Some(ref cb) = on_item {
                cb(i + 1, fs_total, stem);
            }

            checked_fs_files += 1;

            // Parse frontmatter to check translation_for
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let Some((yaml_str, _body)) = crate::util::frontmatter::split_frontmatter(&content)
            else {
                continue;
            };

            let Ok(fm) = crate::util::frontmatter::TranslationFrontmatter::from_yaml(yaml_str)
            else {
                continue;
            };

            let translation_for = &fm.translation_for;
            let language = &fm.language;

            if translation_for.is_empty() {
                continue;
            }

            // Check source post exists
            if !post_map.contains_key(translation_for.as_str()) {
                fs_issues.push(TranslationIssue {
                    post_id: translation_for.clone(),
                    translation_id: None,
                    file_path: Some(path.to_string_lossy().to_string()),
                    language: language.clone(),
                    kind: TranslationIssueKind::MissingSourcePost,
                });
            } else if let Some(source) = post_map.get(translation_for.as_str()) {
                // Check same language
                let post_lang = source.language.as_deref().unwrap_or("en");
                if norm_lang(post_lang) == norm_lang(language) {
                    fs_issues.push(TranslationIssue {
                        post_id: translation_for.clone(),
                        translation_id: None,
                        file_path: Some(path.to_string_lossy().to_string()),
                        language: language.clone(),
                        kind: TranslationIssueKind::SameLanguageAsCanonical,
                    });
                }

                // Check do_not_translate
                if source.do_not_translate {
                    fs_issues.push(TranslationIssue {
                        post_id: translation_for.clone(),
                        translation_id: None,
                        file_path: Some(path.to_string_lossy().to_string()),
                        language: language.clone(),
                        kind: TranslationIssueKind::DoNotTranslateHasTranslations,
                    });
                }
            }
        }
    }

    Ok(TranslationValidationReport {
        db_issues,
        fs_issues,
        checked_db_rows,
        checked_fs_files,
    })
}
