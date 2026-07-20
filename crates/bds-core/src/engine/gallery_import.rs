use std::collections::HashSet;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use rayon::prelude::*;
use serde_json::json;

use crate::db::Database;
use crate::engine::{ai, media, post_media};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedGalleryImage {
    pub media_id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GalleryImportOutcome {
    pub path: PathBuf,
    pub result: Result<ImportedGalleryImage, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GalleryImportReport {
    pub selected_count: usize,
    pub outcomes: Vec<GalleryImportOutcome>,
}

pub fn active_ai_endpoint_configured(conn: &crate::db::DbConnection, offline_mode: bool) -> bool {
    ai::active_endpoint(conn, offline_mode)
        .is_ok_and(|endpoint| !endpoint.url.trim().is_empty() && !endpoint.model.trim().is_empty())
}

pub fn translation_targets(
    main_language: Option<&str>,
    blog_languages: &[String],
    source_language: &str,
) -> Vec<String> {
    let mut seen = HashSet::new();
    main_language
        .into_iter()
        .chain(blog_languages.iter().map(String::as_str))
        .filter(|language| !language.is_empty() && *language != source_language)
        .filter(|language| seen.insert((*language).to_string()))
        .map(str::to_string)
        .collect()
}

pub fn process_paths_concurrently<T, F>(
    paths: Vec<PathBuf>,
    concurrency: usize,
    process: F,
) -> Result<Vec<T>, String>
where
    T: Send,
    F: Fn(usize, PathBuf) -> T + Send + Sync,
{
    rayon::ThreadPoolBuilder::new()
        .num_threads(concurrency.clamp(1, 8))
        .build()
        .map_err(|error| error.to_string())
        .map(|pool| {
            pool.install(|| {
                paths
                    .into_par_iter()
                    .enumerate()
                    .map(|(index, path)| process(index, path))
                    .collect()
            })
        })
}

pub fn import_gallery_images(
    db_path: &Path,
    data_dir: &Path,
    project_id: &str,
    post_id: &str,
    paths: Vec<PathBuf>,
    source_language: &str,
    offline_mode: bool,
) -> GalleryImportReport {
    let selected_count = paths.len();
    let metadata = crate::engine::meta::read_project_json(data_dir).ok();
    let concurrency = metadata
        .as_ref()
        .map(|metadata| metadata.image_import_concurrency.clamp(1, 8) as usize)
        .unwrap_or(4);
    let targets = translation_targets(
        metadata
            .as_ref()
            .and_then(|metadata| metadata.main_language.as_deref()),
        metadata
            .as_ref()
            .map(|metadata| metadata.blog_languages.as_slice())
            .unwrap_or_default(),
        source_language,
    );
    let ai_available = Database::open(db_path)
        .ok()
        .is_some_and(|db| active_ai_endpoint_configured(db.conn(), offline_mode));
    let first_sort_order = Database::open(db_path)
        .ok()
        .and_then(|db| post_media::list_media_for_post(db.conn(), post_id).ok())
        .map(|media| media.len() as i32)
        .unwrap_or(0);

    let process_path = |index: usize, path: PathBuf| GalleryImportOutcome {
        result: import_gallery_image(
            db_path,
            data_dir,
            project_id,
            post_id,
            &path,
            source_language,
            first_sort_order + index as i32,
            ai_available,
            offline_mode,
            &targets,
        ),
        path,
    };
    let outcomes = match process_paths_concurrently(paths.clone(), concurrency, process_path) {
        Ok(outcomes) => outcomes,
        Err(error) => paths
            .into_iter()
            .map(|path| GalleryImportOutcome {
                path,
                result: Err(error.clone()),
            })
            .collect(),
    };

    GalleryImportReport {
        selected_count,
        outcomes,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "one worker receives the immutable batch context"
)]
fn import_gallery_image(
    db_path: &Path,
    data_dir: &Path,
    project_id: &str,
    post_id: &str,
    path: &Path,
    source_language: &str,
    sort_order: i32,
    ai_available: bool,
    offline_mode: bool,
    translation_targets: &[String],
) -> Result<ImportedGalleryImage, String> {
    let db = Database::open(db_path).map_err(|error| error.to_string())?;
    let imported = import_and_link_image(
        db.conn(),
        data_dir,
        project_id,
        post_id,
        path,
        source_language,
        sort_order,
    )
    .map_err(|error| error.to_string())?;

    let title = if ai_available {
        enrich_imported_image(
            db.conn(),
            data_dir,
            &imported,
            offline_mode,
            translation_targets,
        )
        .unwrap_or_else(|_| imported.original_name.clone())
    } else {
        imported.original_name.clone()
    };

    Ok(ImportedGalleryImage {
        media_id: imported.id,
        title,
    })
}

pub fn import_and_link_image(
    conn: &crate::db::DbConnection,
    data_dir: &Path,
    project_id: &str,
    post_id: &str,
    path: &Path,
    source_language: &str,
    sort_order: i32,
) -> crate::engine::EngineResult<crate::model::Media> {
    let original_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "image".to_string());
    let imported = media::import_media(
        conn,
        data_dir,
        project_id,
        path,
        &original_name,
        None,
        None,
        None,
        None,
        Some(source_language),
        Vec::new(),
    )?;
    post_media::link_media_to_post(
        conn,
        data_dir,
        project_id,
        post_id,
        &imported.id,
        sort_order,
    )?;
    Ok(imported)
}

/// Apply the shared gallery AI enrichment and translation pipeline to one
/// already-imported image. Returns the generated title when AI was available.
pub fn enrich_imported_image(
    conn: &crate::db::DbConnection,
    data_dir: &Path,
    imported: &crate::model::Media,
    offline_mode: bool,
    translation_targets: &[String],
) -> Result<String, String> {
    let image_data_url = build_ai_image_data_url(
        data_dir,
        &imported.id,
        &imported.file_path,
        &imported.mime_type,
    )?;
    let response = ai::run_one_shot(
        conn,
        offline_mode,
        &ai::OneShotRequest {
            operation: ai::OneShotOperation::AnalyzeImage,
            content: json!({
                "title": imported.title,
                "alt": imported.alt,
                "caption": imported.caption,
                "filename": imported.original_name,
                "mime_type": imported.mime_type,
                "image_data_url": image_data_url,
            }),
        },
    )
    .map_err(|error| error.to_string())?;
    let ai::OneShotResponse::ImageAnalysis(analysis) = response.0 else {
        return Err("AI returned an unexpected response".to_string());
    };
    media::update_media(
        conn,
        data_dir,
        &imported.id,
        Some(Some(&analysis.title)),
        Some(Some(&analysis.alt)),
        Some(Some(&analysis.caption)),
        None,
        None,
        None,
    )
    .map_err(|error| error.to_string())?;

    for target in translation_targets {
        let Ok((ai::OneShotResponse::MediaTranslation(translation), _)) = ai::run_one_shot(
            conn,
            offline_mode,
            &ai::OneShotRequest {
                operation: ai::OneShotOperation::TranslateMedia {
                    target_language: target.clone(),
                },
                content: json!({
                    "title": analysis.title,
                    "alt": analysis.alt,
                    "caption": analysis.caption,
                }),
            },
        ) else {
            continue;
        };
        let _ = media::upsert_media_translation(
            conn,
            data_dir,
            &imported.id,
            target,
            Some(&translation.title),
            Some(&translation.alt),
            Some(&translation.caption),
        );
    }

    Ok(if analysis.title.is_empty() {
        imported.original_name.clone()
    } else {
        analysis.title
    })
}

pub fn build_ai_image_data_url(
    data_dir: &Path,
    media_id: &str,
    file_path: &str,
    mime_type: &str,
) -> Result<String, String> {
    if !mime_type.starts_with("image/") {
        return Err("AI image analysis requires an image".to_string());
    }

    let source_path = data_dir.join(file_path.trim_start_matches('/'));
    let thumbnail_path = data_dir.join(crate::util::thumbnail_path(media_id, "ai", "jpg"));
    if !thumbnail_path.exists() {
        crate::util::thumbnail::generate_all_thumbnails(
            &source_path,
            &data_dir.join("thumbnails"),
            media_id,
        )
        .map_err(|error| format!("failed to generate AI thumbnail: {error}"))?;
    }
    let bytes = std::fs::read(&thumbnail_path)
        .map_err(|error| format!("failed to read AI thumbnail: {error}"))?;
    Ok(format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;
    use std::time::Duration;

    use tempfile::TempDir;

    use crate::db::queries::post::insert_post;
    use crate::db::queries::post::make_test_post;
    use crate::db::queries::post_media::{list_post_media_by_post, unlink_media};
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::db::{Database, fts::ensure_fts_tables};
    use crate::engine::ai::{AiEndpointConfig, AiEndpointKind, save_endpoint};
    use crate::engine::media::rebuild_media_from_filesystem;
    use crate::model::metadata::ProjectMetadata;

    use super::{import_gallery_images, process_paths_concurrently, translation_targets};

    fn setup() -> (Database, TempDir) {
        let dir = TempDir::new().unwrap();
        let db = Database::open(&dir.path().join("bds.db")).unwrap();
        db.migrate().unwrap();
        ensure_fts_tables(db.conn()).unwrap();
        insert_project(db.conn(), &make_test_project("p1", "Gallery")).unwrap();
        insert_post(db.conn(), &make_test_post("post1", "p1", "gallery")).unwrap();
        crate::engine::meta::write_project_json(
            dir.path(),
            &ProjectMetadata {
                name: "Gallery".to_string(),
                description: None,
                public_url: None,
                main_language: Some("en".to_string()),
                default_author: None,
                max_posts_per_page: 50,
                image_import_concurrency: 2,
                blogmark_category: None,
                pico_theme: None,
                semantic_similarity_enabled: false,
                blog_languages: vec![
                    "de".to_string(),
                    "en".to_string(),
                    "fr".to_string(),
                    "de".to_string(),
                ],
            },
        )
        .unwrap();
        (db, dir)
    }

    fn spawn_ai_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        thread::spawn(move || {
            for stream in listener.incoming().take(3) {
                let mut stream = stream.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 16_384];
                loop {
                    let read = stream.read(&mut buffer).unwrap();
                    if read == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..read]);
                    let Some(header_end) = request.windows(4).position(|part| part == b"\r\n\r\n")
                    else {
                        continue;
                    };
                    let headers = String::from_utf8_lossy(&request[..header_end]);
                    let content_length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length: ")
                                .and_then(|value| value.parse::<usize>().ok())
                        })
                        .unwrap_or(0);
                    if request.len() >= header_end + 4 + content_length {
                        break;
                    }
                }
                let request = String::from_utf8_lossy(&request);
                let content = if request.contains("Translate the media metadata into de") {
                    r#"{"title":"Berg","alt":"Ein Berg","caption":"Morgenlicht"}"#
                } else if request.contains("Translate the media metadata into fr") {
                    r#"{"title":"Montagne","alt":"Une montagne","caption":"Lumière du matin"}"#
                } else {
                    r#"{"title":"Mountain","alt":"A mountain","caption":"Morning light"}"#
                };
                let body = serde_json::json!({
                    "choices": [{"message": {"content": content}}]
                })
                .to_string();
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body,
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        format!("http://{address}")
    }

    #[test]
    fn targets_are_unique_and_exclude_source_language() {
        assert_eq!(
            translation_targets(
                Some("en"),
                &["de".to_string(), "en".to_string(), "de".to_string()],
                "fr",
            ),
            vec!["en".to_string(), "de".to_string()]
        );
    }

    #[test]
    fn partial_failure_keeps_successful_image_linked_and_rebuildable_without_ai() {
        let (db, dir) = setup();
        let image = dir.path().join("photo.jpg");
        fs::write(&image, b"jpeg data").unwrap();
        let invalid = dir.path().join("notes.txt");
        fs::write(&invalid, b"not an image").unwrap();

        let report = import_gallery_images(
            &dir.path().join("bds.db"),
            dir.path(),
            "p1",
            "post1",
            vec![image, invalid],
            "en",
            false,
        );

        assert_eq!(report.selected_count, 2);
        assert!(report.outcomes[0].result.is_ok());
        assert!(report.outcomes[1].result.is_err());

        let links = list_post_media_by_post(db.conn(), "post1").unwrap();
        assert_eq!(links.len(), 1);
        let media_id = links[0].media_id.clone();
        let sidecar = fs::read_to_string(
            dir.path().join(
                crate::db::queries::media::get_media_by_id(db.conn(), &media_id)
                    .unwrap()
                    .sidecar_path,
            ),
        )
        .unwrap();
        assert!(sidecar.contains("linkedPostIds: [\"post1\"]"));

        unlink_media(db.conn(), "post1", &media_id).unwrap();
        rebuild_media_from_filesystem(db.conn(), dir.path(), "p1").unwrap();
        assert_eq!(
            list_post_media_by_post(db.conn(), "post1").unwrap().len(),
            1
        );
    }

    #[test]
    fn configured_local_ai_enriches_metadata_and_unique_translation_targets() {
        let (db, dir) = setup();
        save_endpoint(
            db.conn(),
            &AiEndpointConfig {
                kind: AiEndpointKind::Airplane,
                url: spawn_ai_server(),
                model: "local-vision".to_string(),
                api_key: None,
            },
        )
        .unwrap();
        let image = dir.path().join("mountain.png");
        image::DynamicImage::new_rgb8(8, 8).save(&image).unwrap();

        let report = import_gallery_images(
            &dir.path().join("bds.db"),
            dir.path(),
            "p1",
            "post1",
            vec![image],
            "en",
            true,
        );

        let imported = report.outcomes[0].result.as_ref().unwrap();
        assert_eq!(imported.title, "Mountain");
        let media =
            crate::db::queries::media::get_media_by_id(db.conn(), &imported.media_id).unwrap();
        assert_eq!(media.title.as_deref(), Some("Mountain"));
        assert_eq!(media.alt.as_deref(), Some("A mountain"));
        assert_eq!(media.caption.as_deref(), Some("Morning light"));
        let translations = crate::db::queries::media_translation::list_media_translations_by_media(
            db.conn(),
            &media.id,
        )
        .unwrap();
        assert_eq!(
            translations
                .iter()
                .map(|translation| translation.language.as_str())
                .collect::<Vec<_>>(),
            vec!["de", "fr"]
        );
        for language in ["de", "fr"] {
            assert!(
                dir.path()
                    .join(crate::util::media_translation_sidecar_path(
                        &media.file_path,
                        language,
                    ))
                    .is_file()
            );
        }
    }

    #[test]
    fn analysis_failure_does_not_remove_the_post_link() {
        let (db, dir) = setup();
        save_endpoint(
            db.conn(),
            &AiEndpointConfig {
                kind: AiEndpointKind::Airplane,
                url: "http://127.0.0.1:9".to_string(),
                model: "unavailable-local-model".to_string(),
                api_key: None,
            },
        )
        .unwrap();
        let image = dir.path().join("linked.png");
        image::DynamicImage::new_rgb8(8, 8).save(&image).unwrap();

        let report = import_gallery_images(
            &dir.path().join("bds.db"),
            dir.path(),
            "p1",
            "post1",
            vec![image],
            "en",
            true,
        );

        assert!(report.outcomes[0].result.is_ok());
        assert_eq!(
            list_post_media_by_post(db.conn(), "post1").unwrap().len(),
            1
        );
    }

    #[test]
    fn concurrent_processor_clamps_worker_count_to_one_through_eight() {
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let paths = (0..24)
            .map(|index| PathBuf::from(format!("{index}.jpg")))
            .collect();
        let results = process_paths_concurrently(paths, 99, {
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            move |_index, path| {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                maximum.fetch_max(current, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(5));
                active.fetch_sub(1, Ordering::SeqCst);
                path
            }
        })
        .unwrap();
        assert_eq!(results.len(), 24);
        assert!((1..=8).contains(&maximum.load(Ordering::SeqCst)));

        assert_eq!(
            process_paths_concurrently(vec![PathBuf::from("one.jpg")], 0, |_, path| path)
                .unwrap()
                .len(),
            1
        );
    }
}
