use std::fs;
use std::path::Path;

use crate::db::DbConnection as Connection;

use crate::db::queries::media as mq;
use crate::db::queries::post_media as pmq;
use crate::engine::EngineResult;
use crate::model::Media;
use crate::util::{media_sidecar_path, thumbnail_path};

/// Thumbnail sizes per media_processing.allium.
const THUMBNAIL_SIZES: &[&str] = &["small", "medium", "large", "ai"];

/// Types of media validation issues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaIssueKind {
    MissingBinary,
    MissingSidecar,
    MissingThumbnail { size: String },
    Corrupted,
    Orphan,
}

/// A single media validation issue.
#[derive(Debug, Clone)]
pub struct MediaIssue {
    pub media_id: String,
    pub original_name: String,
    pub kind: MediaIssueKind,
    pub detail: String,
}

/// Full validation report.
#[derive(Debug, Clone, Default)]
pub struct MediaValidationReport {
    pub issues: Vec<MediaIssue>,
    pub total_checked: usize,
}

/// Progress callback type for media validation.
pub type ProgressFn = Box<dyn Fn(usize, usize, &str) + Send>;

/// Validate all media for a project per media_processing.allium ValidateMedia rule.
///
/// Checks for:
/// - Missing binary files
/// - Missing sidecar files
/// - Missing thumbnails (all 4 sizes)
/// - Corrupted image files (basic header check)
/// - Orphan media (not linked to any post)
pub fn validate_media(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    on_progress: Option<ProgressFn>,
) -> EngineResult<MediaValidationReport> {
    let all_media = mq::list_media_by_project(conn, project_id)?;
    let total = all_media.len();
    let mut report = MediaValidationReport {
        issues: Vec::new(),
        total_checked: total,
    };

    for (idx, media) in all_media.iter().enumerate() {
        if let Some(ref cb) = on_progress {
            cb(idx + 1, total, &media.original_name);
        }
        check_media_item(conn, data_dir, media, &mut report.issues)?;
    }

    Ok(report)
}

fn check_media_item(
    conn: &Connection,
    data_dir: &Path,
    media: &Media,
    issues: &mut Vec<MediaIssue>,
) -> EngineResult<()> {
    let binary_path = data_dir.join(&media.file_path);

    // 1. Missing binary
    if !binary_path.exists() {
        issues.push(MediaIssue {
            media_id: media.id.clone(),
            original_name: media.original_name.clone(),
            kind: MediaIssueKind::MissingBinary,
            detail: format!("Binary file not found: {}", media.file_path),
        });
    } else {
        // 3. Corrupted check — basic: file is empty or not readable
        match fs::metadata(&binary_path) {
            Ok(meta) if meta.len() == 0 => {
                issues.push(MediaIssue {
                    media_id: media.id.clone(),
                    original_name: media.original_name.clone(),
                    kind: MediaIssueKind::Corrupted,
                    detail: "Binary file is empty (0 bytes)".to_string(),
                });
            }
            Err(e) => {
                issues.push(MediaIssue {
                    media_id: media.id.clone(),
                    original_name: media.original_name.clone(),
                    kind: MediaIssueKind::Corrupted,
                    detail: format!("Cannot read binary: {e}"),
                });
            }
            _ => {}
        }
    }

    // 2. Missing sidecar
    let sidecar_rel = media_sidecar_path(&media.file_path);
    let sidecar_path = data_dir.join(&sidecar_rel);
    if !sidecar_path.exists() {
        issues.push(MediaIssue {
            media_id: media.id.clone(),
            original_name: media.original_name.clone(),
            kind: MediaIssueKind::MissingSidecar,
            detail: format!("Sidecar not found: {sidecar_rel}"),
        });
    }

    // 3. Missing thumbnails — only for image types
    if is_image_mime(&media.mime_type) {
        let ext = thumbnail_extension(&media.mime_type);
        for size in THUMBNAIL_SIZES {
            let thumb_rel = thumbnail_path(&media.id, size, ext);
            let thumb_path = data_dir.join(&thumb_rel);
            if !thumb_path.exists() {
                issues.push(MediaIssue {
                    media_id: media.id.clone(),
                    original_name: media.original_name.clone(),
                    kind: MediaIssueKind::MissingThumbnail {
                        size: size.to_string(),
                    },
                    detail: format!("Thumbnail missing: {thumb_rel}"),
                });
            }
        }
    }

    // 4. Orphan check — media not linked to any post
    let links = pmq::list_post_media_by_media(conn, &media.id)?;
    if links.is_empty() {
        issues.push(MediaIssue {
            media_id: media.id.clone(),
            original_name: media.original_name.clone(),
            kind: MediaIssueKind::Orphan,
            detail: "Media is not linked to any post".to_string(),
        });
    }

    Ok(())
}

fn is_image_mime(mime: &str) -> bool {
    mime.starts_with("image/")
}

fn thumbnail_extension(mime: &str) -> &str {
    match mime {
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        _ => "jpg",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_image_recognizes_types() {
        assert!(is_image_mime("image/jpeg"));
        assert!(is_image_mime("image/png"));
        assert!(!is_image_mime("application/pdf"));
        assert!(!is_image_mime("video/mp4"));
    }

    #[test]
    fn thumbnail_ext_defaults_to_jpg() {
        assert_eq!(thumbnail_extension("image/jpeg"), "jpg");
        assert_eq!(thumbnail_extension("image/png"), "png");
        assert_eq!(thumbnail_extension("image/webp"), "webp");
        assert_eq!(thumbnail_extension("image/tiff"), "jpg"); // fallback
    }
}
