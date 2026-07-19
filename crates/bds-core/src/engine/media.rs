use std::fs;
use std::path::Path;

use crate::db::DbConnection as Connection;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::db::fts;
use crate::db::queries::media as qm;
use crate::db::queries::media_translation as qmt;
use crate::db::queries::post as qp;
use crate::db::queries::post_media as qpm;
use crate::engine::{EngineError, EngineResult, domain_events};
use crate::model::{DomainEntity, Media, MediaTranslation, NotificationAction, PostMedia};
use crate::util::sidecar::{
    MediaSidecar, MediaTranslationSidecar, read_sidecar, read_translation_sidecar,
};
use crate::util::thumbnail::{
    THUMBNAIL_SIZES, ThumbnailFormat, generate_all_thumbnails, image_dimensions,
    mime_from_extension,
};
use crate::util::{
    atomic_write_str, content_hash, media_dir_path, media_sidecar_path,
    media_translation_sidecar_path, now_unix_ms,
};

/// Report returned by `rebuild_media_from_filesystem`.
#[derive(Debug, Default)]
pub struct MediaRebuildReport {
    pub media_created: usize,
    pub media_updated: usize,
    pub translations_created: usize,
    pub translations_updated: usize,
    pub errors: Vec<String>,
}

/// Supported image MIME types for import (per media_processing.allium).
const SUPPORTED_IMAGE_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/tiff",
    "image/bmp",
    "image/heic",
    "image/heif",
];

/// Import a media file (image, etc.) into the project.
#[expect(
    clippy::too_many_arguments,
    reason = "arguments are the user-supplied media metadata fields"
)]
pub fn import_media(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    source_path: &Path,
    original_name: &str,
    title: Option<&str>,
    alt: Option<&str>,
    caption: Option<&str>,
    author: Option<&str>,
    language: Option<&str>,
    tags: Vec<String>,
) -> EngineResult<Media> {
    import_media_at(
        conn,
        data_dir,
        project_id,
        source_path,
        original_name,
        title,
        alt,
        caption,
        author,
        language,
        tags,
        now_unix_ms(),
    )
}

/// Import media while preserving a trusted source timestamp. Importers use
/// this path so the canonical date-based location and sidecar metadata match
/// the source system; interactive imports continue to use the current time.
#[expect(
    clippy::too_many_arguments,
    reason = "arguments are the user-supplied media metadata fields plus source timestamp"
)]
pub(crate) fn import_media_at(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    source_path: &Path,
    original_name: &str,
    title: Option<&str>,
    alt: Option<&str>,
    caption: Option<&str>,
    author: Option<&str>,
    language: Option<&str>,
    tags: Vec<String>,
    created_at: i64,
) -> EngineResult<Media> {
    // Validate file type per spec
    let ext = Path::new(original_name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin");
    let mime_type = mime_from_extension(ext).to_string();
    if !SUPPORTED_IMAGE_TYPES.contains(&mime_type.as_str()) {
        return Err(EngineError::Validation(format!(
            "unsupported file type: {mime_type} (file: {original_name})"
        )));
    }

    let id = Uuid::new_v4().to_string();
    let filename = format!("{id}.{ext}");

    // Compute target directory and copy file
    let dir_path = media_dir_path(created_at);
    let rel_file_path = format!("{dir_path}{filename}");
    let abs_file_path = data_dir.join(&rel_file_path);

    if let Some(parent) = abs_file_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source_path, &abs_file_path)?;

    // Get image dimensions
    let (width, height) = image_dimensions(&abs_file_path)
        .map(|(w, h)| (Some(w as i32), Some(h as i32)))
        .unwrap_or((None, None));

    // Get file size
    let file_size = fs::metadata(&abs_file_path)?.len() as i64;

    // Compute sidecar path
    let sidecar_rel = media_sidecar_path(&rel_file_path);

    // Compute checksum of the copied file
    let file_bytes = fs::read(&abs_file_path)?;
    let checksum = content_hash(&file_bytes);

    // Generate thumbnails (silently ignore errors for non-image files)
    let thumbnails_dir = data_dir.join("thumbnails");
    let _ = generate_all_thumbnails(&abs_file_path, &thumbnails_dir, &id);

    let media = Media {
        id: id.clone(),
        project_id: project_id.to_string(),
        filename,
        original_name: original_name.to_string(),
        mime_type,
        size: file_size,
        width,
        height,
        title: title.map(|s| s.to_string()),
        alt: alt.map(|s| s.to_string()),
        caption: caption.map(|s| s.to_string()),
        author: author.map(|s| s.to_string()),
        language: language.map(|s| s.to_string()),
        file_path: rel_file_path,
        sidecar_path: sidecar_rel.clone(),
        checksum: Some(checksum),
        tags,
        created_at,
        updated_at: created_at,
    };

    // Write sidecar
    let sidecar = MediaSidecar::from_media(&media, &[]);
    let abs_sidecar = data_dir.join(&sidecar_rel);
    atomic_write_str(&abs_sidecar, &sidecar.to_string())?;

    // Insert into DB
    qm::insert_media(conn, &media)?;

    // Index in FTS
    fts_index_media(conn, &media)?;

    emit_media(&media, NotificationAction::Created);

    Ok(media)
}

/// Update a media item's metadata fields.
#[expect(
    clippy::too_many_arguments,
    reason = "optional arguments represent independent media field changes"
)]
pub fn update_media(
    conn: &Connection,
    data_dir: &Path,
    media_id: &str,
    title: Option<Option<&str>>,
    alt: Option<Option<&str>>,
    caption: Option<Option<&str>>,
    author: Option<Option<&str>>,
    language: Option<Option<&str>>,
    tags: Option<Vec<String>>,
) -> EngineResult<Media> {
    let mut media = qm::get_media_by_id(conn, media_id)?;

    if let Some(t) = title {
        media.title = t.map(|s| s.to_string());
    }
    if let Some(a) = alt {
        media.alt = a.map(|s| s.to_string());
    }
    if let Some(c) = caption {
        media.caption = c.map(|s| s.to_string());
    }
    if let Some(a) = author {
        media.author = a.map(|s| s.to_string());
    }
    if let Some(l) = language {
        media.language = l.map(|s| s.to_string());
    }
    if let Some(t) = tags {
        media.tags = t;
    }

    media.updated_at = now_unix_ms();
    qm::update_media(conn, &media)?;

    // Rewrite sidecar (need linked_post_ids from post_media table)
    let linked = qpm::list_post_media_by_media(conn, media_id).unwrap_or_default();
    let linked_post_ids: Vec<String> = linked.iter().map(|pm| pm.post_id.clone()).collect();
    let sidecar = MediaSidecar::from_media(&media, &linked_post_ids);
    let abs_sidecar = data_dir.join(&media.sidecar_path);
    atomic_write_str(&abs_sidecar, &sidecar.to_string())?;

    // Re-index FTS
    fts_index_media(conn, &media)?;

    emit_media(&media, NotificationAction::Updated);

    Ok(media)
}

/// Replace a media item's binary while preserving its identity, public path,
/// metadata, translations, and post links. Returns `None` when the new file is
/// byte-for-byte identical to the stored binary.
pub fn replace_media_file(
    conn: &Connection,
    data_dir: &Path,
    media_id: &str,
    new_source_path: &Path,
) -> EngineResult<Option<Media>> {
    let mut media = qm::get_media_by_id(conn, media_id)
        .map_err(|_| EngineError::NotFound(format!("media {media_id}")))?;
    if !new_source_path.is_file() {
        return Err(EngineError::Validation(format!(
            "replacement file does not exist: {}",
            new_source_path.display()
        )));
    }

    let replacement_checksum = crate::util::file_hash(new_source_path)?;
    if media.checksum.as_deref() == Some(replacement_checksum.as_str()) {
        return Ok(None);
    }

    let destination = data_dir.join(&media.file_path);
    if !destination.is_file() {
        return Err(EngineError::NotFound(format!(
            "stored media file {}",
            destination.display()
        )));
    }

    let (width, height) = image_dimensions(new_source_path)
        .map(|(w, h)| (Some(w as i32), Some(h as i32)))
        .map_err(|error| {
            EngineError::Validation(format!(
                "replacement is not a readable image '{}': {error}",
                new_source_path.display(),
            ))
        })?;
    let replacement_size = fs::metadata(new_source_path)?.len() as i64;

    // Generate all derived files before touching the canonical binary.
    let staged_thumbnails = data_dir
        .join("thumbnails")
        .join(format!(".replace-{media_id}"));
    if staged_thumbnails.exists() {
        fs::remove_dir_all(&staged_thumbnails)?;
    }
    let staged_paths = generate_all_thumbnails(new_source_path, &staged_thumbnails, media_id)
        .map_err(EngineError::Parse)?;

    let backup = destination.with_extension(format!(
        "{}.bak",
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("media")
    ));
    if backup.exists() {
        fs::remove_file(&backup)?;
    }
    fs::rename(&destination, &backup)?;
    if let Err(error) = fs::copy(new_source_path, &destination) {
        let _ = fs::rename(&backup, &destination);
        let _ = fs::remove_dir_all(&staged_thumbnails);
        return Err(error.into());
    }

    let previous_media = media.clone();
    let previous_sidecar = fs::read_to_string(data_dir.join(&media.sidecar_path)).ok();
    media.size = replacement_size;
    media.width = width;
    media.height = height;
    media.checksum = Some(replacement_checksum);
    media.updated_at = now_unix_ms();

    let apply_result = (|| -> EngineResult<()> {
        conn.begin_savepoint()?;
        qm::update_media(conn, &media)?;
        let linked = qpm::list_post_media_by_media(conn, media_id).unwrap_or_default();
        let linked_post_ids: Vec<String> = linked.iter().map(|item| item.post_id.clone()).collect();
        let sidecar = MediaSidecar::from_media(&media, &linked_post_ids);
        atomic_write_str(&data_dir.join(&media.sidecar_path), &sidecar.to_string())?;
        fts_index_media(conn, &media)?;

        for staged in &staged_paths {
            let staged = Path::new(staged);
            let relative = staged.strip_prefix(&staged_thumbnails).map_err(|error| {
                EngineError::Validation(format!("invalid staged thumbnail path: {error}"))
            })?;
            let target = data_dir.join("thumbnails").join(relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(staged, target)?;
        }
        conn.release_savepoint()?;
        Ok(())
    })();

    let _ = fs::remove_dir_all(&staged_thumbnails);
    match apply_result {
        Ok(()) => {
            fs::remove_file(backup)?;
            emit_media(&media, NotificationAction::Updated);
            Ok(Some(media))
        }
        Err(error) => {
            let _ = conn.rollback_savepoint();
            let _ = fs::remove_file(&destination);
            let _ = fs::rename(&backup, &destination);
            let _ = qm::update_media(conn, &previous_media);
            if let Some(previous_sidecar) = previous_sidecar {
                let _ = atomic_write_str(
                    &data_dir.join(&previous_media.sidecar_path),
                    &previous_sidecar,
                );
            }
            Err(error)
        }
    }
}

/// Delete a media item and all related artifacts.
pub fn delete_media(conn: &Connection, data_dir: &Path, media_id: &str) -> EngineResult<()> {
    let media = qm::get_media_by_id(conn, media_id)?;

    // Delete binary file
    let abs_file = data_dir.join(&media.file_path);
    if abs_file.exists() {
        fs::remove_file(&abs_file)?;
    }

    // Delete sidecar file
    let abs_sidecar = data_dir.join(&media.sidecar_path);
    if abs_sidecar.exists() {
        fs::remove_file(&abs_sidecar)?;
    }

    // Delete all translation sidecar files
    let translations = qmt::list_media_translations_by_media(conn, media_id).unwrap_or_default();
    for t in &translations {
        let trans_sidecar = media_translation_sidecar_path(&media.file_path, &t.language);
        let abs_trans = data_dir.join(&trans_sidecar);
        if abs_trans.exists() {
            fs::remove_file(&abs_trans)?;
        }
    }

    // Delete all thumbnails
    let ext_map = |fmt: &ThumbnailFormat| -> &str {
        match fmt {
            ThumbnailFormat::Webp => "webp",
            ThumbnailFormat::Jpeg => "jpg",
        }
    };
    let prefix = &media_id[..2.min(media_id.len())];
    for size in THUMBNAIL_SIZES {
        let ext = ext_map(&size.format);
        let thumb_rel = format!("thumbnails/{prefix}/{media_id}-{}.{ext}", size.name);
        let abs_thumb = data_dir.join(&thumb_rel);
        if abs_thumb.exists() {
            let _ = fs::remove_file(&abs_thumb);
        }
    }

    // Delete media translations from DB
    for t in &translations {
        qmt::delete_media_translation(conn, media_id, &t.language)?;
    }

    // Delete post_media links from DB
    let links = qpm::list_post_media_by_media(conn, media_id).unwrap_or_default();
    for link in &links {
        qpm::unlink_media(conn, &link.post_id, media_id)?;
    }

    // Remove from FTS index
    fts::remove_media_from_index(conn, media_id)?;

    // Delete from media table
    qm::delete_media(conn, media_id)?;

    emit_media(&media, NotificationAction::Deleted);

    Ok(())
}

fn emit_media(media: &Media, action: NotificationAction) {
    domain_events::entity_changed(&media.project_id, DomainEntity::Media, &media.id, action);
}

/// Create or update a translation for a media item.
pub fn upsert_media_translation(
    conn: &Connection,
    data_dir: &Path,
    media_id: &str,
    language: &str,
    title: Option<&str>,
    alt: Option<&str>,
    caption: Option<&str>,
) -> EngineResult<MediaTranslation> {
    let media = qm::get_media_by_id(conn, media_id)?;
    let now = now_unix_ms();

    // Check if translation already exists
    let existing = qmt::get_media_translation_by_media_and_language(conn, media_id, language);
    let translation = match existing {
        Ok(mut t) => {
            t.title = title.map(|s| s.to_string());
            t.alt = alt.map(|s| s.to_string());
            t.caption = caption.map(|s| s.to_string());
            t.updated_at = now;
            qmt::upsert_media_translation(conn, &t)?;
            t
        }
        Err(_) => {
            let t = MediaTranslation {
                id: Uuid::new_v4().to_string(),
                project_id: media.project_id.clone(),
                translation_for: media_id.to_string(),
                language: language.to_string(),
                title: title.map(|s| s.to_string()),
                alt: alt.map(|s| s.to_string()),
                caption: caption.map(|s| s.to_string()),
                created_at: now,
                updated_at: now,
            };
            qmt::upsert_media_translation(conn, &t)?;
            t
        }
    };

    // Write translation sidecar file
    let trans_sidecar = MediaTranslationSidecar {
        translation_for: media_id.to_string(),
        language: language.to_string(),
        title: title.map(|s| s.to_string()),
        alt: alt.map(|s| s.to_string()),
        caption: caption.map(|s| s.to_string()),
    };
    let sidecar_rel = media_translation_sidecar_path(&media.file_path, language);
    let abs_sidecar = data_dir.join(&sidecar_rel);
    atomic_write_str(&abs_sidecar, &trans_sidecar.to_string())?;

    // Re-index FTS for parent media
    fts_index_media(conn, &media)?;

    Ok(translation)
}

/// Delete a media translation and its sidecar file.
pub fn delete_media_translation(
    conn: &Connection,
    data_dir: &Path,
    media_id: &str,
    language: &str,
) -> EngineResult<()> {
    let media = qm::get_media_by_id(conn, media_id)?;

    // Delete translation sidecar file
    let sidecar_rel = media_translation_sidecar_path(&media.file_path, language);
    let abs_sidecar = data_dir.join(&sidecar_rel);
    if abs_sidecar.exists() {
        fs::remove_file(&abs_sidecar)?;
    }

    // Delete from DB
    qmt::delete_media_translation(conn, media_id, language)?;

    // Re-index FTS for parent media
    fts_index_media(conn, &media)?;

    Ok(())
}

/// Rebuild media entries from filesystem. Walk `media/` directory, parse sidecars, upsert into DB.
pub fn rebuild_media_from_filesystem(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<MediaRebuildReport> {
    rebuild_media_from_filesystem_with_progress(conn, data_dir, project_id, None)
}

/// Per-item progress callback: (current_item, total_items, item_description).
pub type ItemProgressFn = Box<dyn Fn(usize, usize, &str) + Send>;

/// Like `rebuild_media_from_filesystem` but with optional per-item progress.
pub fn rebuild_media_from_filesystem_with_progress(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    on_item: Option<ItemProgressFn>,
) -> EngineResult<MediaRebuildReport> {
    let mut report = MediaRebuildReport::default();
    let media_dir = data_dir.join("media");

    if !media_dir.exists() {
        return Ok(report);
    }

    let mut canonical_sidecars = Vec::new();
    let mut translation_sidecars = Vec::new();

    for entry in WalkDir::new(&media_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if !file_name.ends_with(".meta") {
            continue;
        }

        if is_translation_sidecar(file_name) {
            translation_sidecars.push(path.to_path_buf());
        } else {
            canonical_sidecars.push(path.to_path_buf());
        }
    }

    let total = canonical_sidecars.len() + translation_sidecars.len();

    // Process canonical sidecars first
    for (i, path) in canonical_sidecars.iter().enumerate() {
        if let Some(ref cb) = on_item {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
            cb(i + 1, total, name);
        }
        match rebuild_canonical_media(conn, data_dir, project_id, path) {
            Ok(created) => {
                if created {
                    report.media_created += 1;
                } else {
                    report.media_updated += 1;
                }
            }
            Err(e) => {
                report.errors.push(format!("{}: {e}", path.display()));
            }
        }
    }

    // Process translation sidecars
    let offset = canonical_sidecars.len();
    for (i, path) in translation_sidecars.iter().enumerate() {
        if let Some(ref cb) = on_item {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("?");
            cb(offset + i + 1, total, name);
        }
        match rebuild_translation_sidecar(conn, data_dir, project_id, path) {
            Ok(created) => {
                if created {
                    report.translations_created += 1;
                } else {
                    report.translations_updated += 1;
                }
            }
            Err(e) => {
                report.errors.push(format!("{}: {e}", path.display()));
            }
        }
    }

    // Re-index FTS for all media in this project
    let all_media = qm::list_media_by_project(conn, project_id)?;
    for m in &all_media {
        fts_index_media(conn, m)?;
    }

    Ok(report)
}

// --- Internal helpers ---

/// Index a media item in FTS, gathering translation texts.
fn fts_index_media(conn: &Connection, media: &Media) -> EngineResult<()> {
    let translations = qmt::list_media_translations_by_media(conn, &media.id).unwrap_or_default();
    let translation_data: Vec<fts::MediaTranslationFts> = translations
        .iter()
        .map(|t| fts::MediaTranslationFts {
            title: t.title.clone(),
            alt: t.alt.clone(),
            caption: t.caption.clone(),
            language: t.language.clone(),
        })
        .collect();

    let lang = media.language.as_deref().unwrap_or("en");
    fts::index_media(
        conn,
        &media.id,
        media.title.as_deref(),
        media.alt.as_deref(),
        media.caption.as_deref(),
        &media.original_name,
        &media.tags,
        &translation_data,
        lang,
    )?;
    Ok(())
}

/// Check if a .meta filename is a translation sidecar: `{name}.{lang}.meta`
/// where lang is exactly 2 lowercase ASCII letters.
fn is_translation_sidecar(file_name: &str) -> bool {
    // file_name ends with .meta
    // Strip .meta suffix, then check if remaining ends with .{2-letter-lang}
    let without_meta = match file_name.strip_suffix(".meta") {
        Some(s) => s,
        None => return false,
    };
    // Find the last dot in what remains
    if let Some(dot_pos) = without_meta.rfind('.') {
        let suffix = &without_meta[dot_pos + 1..];
        // Must also have another dot before this (the extension of the media file)
        // e.g. "foo.jpg.de" -> dot_pos points to the dot before "de"
        // "foo.jpg" would be the canonical sidecar (no extra dot+lang)
        suffix.len() == 2 && suffix.chars().all(|c| c.is_ascii_lowercase()) && dot_pos > 0
    } else {
        false
    }
}

/// Rebuild a canonical media from its sidecar file. Returns true if created, false if updated.
pub(crate) fn rebuild_canonical_media(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    sidecar_path: &Path,
) -> EngineResult<bool> {
    let content = fs::read_to_string(sidecar_path)?;
    let sc = read_sidecar(&content).map_err(EngineError::Parse)?;

    // Derive file_path: the sidecar path minus ".meta" suffix, relative to data_dir
    let sidecar_rel = sidecar_path
        .strip_prefix(data_dir)
        .unwrap_or(sidecar_path)
        .to_string_lossy()
        .to_string();
    let file_path = sidecar_rel
        .strip_suffix(".meta")
        .unwrap_or(&sidecar_rel)
        .to_string();

    // Derive filename from file_path
    let filename = Path::new(&file_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_string();

    let linked_post_ids = sc.linked_post_ids.clone();

    let existing = qm::get_media_by_id(conn, &sc.id);
    let created = match existing {
        Ok(mut media) => {
            // Update existing media
            media.original_name = sc.original_name;
            media.mime_type = sc.mime_type;
            media.size = sc.size;
            media.width = sc.width;
            media.height = sc.height;
            media.title = sc.title;
            media.alt = sc.alt;
            media.caption = sc.caption;
            media.author = sc.author;
            media.language = sc.language;
            media.file_path = file_path;
            media.sidecar_path = sidecar_rel;
            media.checksum = None;
            media.tags = sc.tags;
            media.updated_at = sc.updated_at;
            qm::update_media(conn, &media)?;
            false
        }
        Err(_) => {
            let media = Media {
                id: sc.id.clone(),
                project_id: project_id.to_string(),
                filename,
                original_name: sc.original_name,
                mime_type: sc.mime_type,
                size: sc.size,
                width: sc.width,
                height: sc.height,
                title: sc.title,
                alt: sc.alt,
                caption: sc.caption,
                author: sc.author,
                language: sc.language,
                file_path,
                sidecar_path: sidecar_rel,
                checksum: None,
                tags: sc.tags,
                created_at: sc.created_at,
                updated_at: sc.updated_at,
            };
            qm::insert_media(conn, &media)?;
            true
        }
    };

    let existing_post_ids: std::collections::HashSet<_> =
        qpm::list_post_media_by_media(conn, &sc.id)?
            .into_iter()
            .map(|link| link.post_id)
            .collect();
    for post_id in linked_post_ids {
        if !existing_post_ids.contains(&post_id) && qp::get_post_by_id(conn, &post_id).is_ok() {
            qpm::link_media(
                conn,
                &PostMedia {
                    id: Uuid::new_v4().to_string(),
                    project_id: project_id.to_string(),
                    post_id,
                    media_id: sc.id.clone(),
                    sort_order: 0,
                    created_at: now_unix_ms(),
                },
            )?;
        }
    }

    Ok(created)
}

/// Rebuild a translation from a `*.{lang}.meta` sidecar. Returns true if created, false if updated.
pub(crate) fn rebuild_translation_sidecar(
    conn: &Connection,
    _data_dir: &Path,
    project_id: &str,
    sidecar_path: &Path,
) -> EngineResult<bool> {
    let content = fs::read_to_string(sidecar_path)?;
    let sc = read_translation_sidecar(&content).map_err(EngineError::Parse)?;

    // Check parent media exists
    let _media = qm::get_media_by_id(conn, &sc.translation_for).map_err(|_| {
        EngineError::NotFound(format!(
            "parent media '{}' not found for translation",
            sc.translation_for
        ))
    })?;

    let now = now_unix_ms();

    let existing =
        qmt::get_media_translation_by_media_and_language(conn, &sc.translation_for, &sc.language);
    match existing {
        Ok(mut t) => {
            t.title = sc.title;
            t.alt = sc.alt;
            t.caption = sc.caption;
            t.updated_at = now;
            qmt::upsert_media_translation(conn, &t)?;
            Ok(false)
        }
        Err(_) => {
            let t = MediaTranslation {
                id: Uuid::new_v4().to_string(),
                project_id: project_id.to_string(),
                translation_for: sc.translation_for,
                language: sc.language,
                title: sc.title,
                alt: sc.alt,
                caption: sc.caption,
                created_at: now,
                updated_at: now,
            };
            qmt::upsert_media_translation(conn, &t)?;
            Ok(true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::fts::ensure_fts_tables;
    use crate::db::queries::project::{insert_project, make_test_project};
    use image::DynamicImage;
    use tempfile::TempDir;

    fn setup() -> (Database, TempDir) {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        ensure_fts_tables(db.conn()).unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = TempDir::new().unwrap();
        (db, dir)
    }

    /// Create a simple 100x80 PNG in the given directory.
    fn create_test_png(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("test-source.png");
        let img = DynamicImage::new_rgb8(100, 80);
        img.save(&path).unwrap();
        path
    }

    #[test]
    fn import_media_creates_artifacts() {
        let (db, dir) = setup();
        let source = create_test_png(dir.path());

        let media = import_media(
            db.conn(),
            dir.path(),
            "p1",
            &source,
            "photo.png",
            Some("My Photo"),
            Some("A photo"),
            None,
            Some("Alice"),
            Some("en"),
            vec!["nature".into()],
        )
        .unwrap();

        // Verify DB entry
        let from_db = qm::get_media_by_id(db.conn(), &media.id).unwrap();
        assert_eq!(from_db.original_name, "photo.png");
        assert_eq!(from_db.title.as_deref(), Some("My Photo"));
        assert_eq!(from_db.alt.as_deref(), Some("A photo"));
        assert_eq!(from_db.author.as_deref(), Some("Alice"));
        assert_eq!(from_db.language.as_deref(), Some("en"));
        assert_eq!(from_db.tags, vec!["nature"]);
        assert_eq!(from_db.mime_type, "image/png");
        assert_eq!(from_db.width, Some(100));
        assert_eq!(from_db.height, Some(80));
        assert!(from_db.checksum.is_some());
        assert!(from_db.size > 0);

        // Verify binary file exists
        let abs_file = dir.path().join(&from_db.file_path);
        assert!(abs_file.exists(), "binary file should exist");

        // Verify sidecar exists
        let abs_sidecar = dir.path().join(&from_db.sidecar_path);
        assert!(abs_sidecar.exists(), "sidecar should exist");

        // Verify sidecar content is parseable
        let sidecar_content = fs::read_to_string(&abs_sidecar).unwrap();
        let sc = read_sidecar(&sidecar_content).unwrap();
        assert_eq!(sc.id, media.id);
        assert_eq!(sc.original_name, "photo.png");

        // Verify thumbnails exist
        let prefix = &media.id[..2];
        for size in THUMBNAIL_SIZES {
            let ext = match size.format {
                ThumbnailFormat::Webp => "webp",
                ThumbnailFormat::Jpeg => "jpg",
            };
            let thumb = dir
                .path()
                .join("thumbnails")
                .join(prefix)
                .join(format!("{}-{}.{ext}", media.id, size.name));
            assert!(thumb.exists(), "thumbnail {} should exist", size.name);
        }
    }

    #[test]
    fn update_media_rewrites_sidecar() {
        let (db, dir) = setup();
        let source = create_test_png(dir.path());

        let media = import_media(
            db.conn(),
            dir.path(),
            "p1",
            &source,
            "photo.png",
            Some("Original Title"),
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();

        // Read original sidecar
        let abs_sidecar = dir.path().join(&media.sidecar_path);
        let original_content = fs::read_to_string(&abs_sidecar).unwrap();

        // Update
        let updated = update_media(
            db.conn(),
            dir.path(),
            &media.id,
            Some(Some("New Title")),
            Some(Some("New alt")),
            None,
            None,
            None,
            Some(vec!["updated-tag".into()]),
        )
        .unwrap();

        assert_eq!(updated.title.as_deref(), Some("New Title"));
        assert_eq!(updated.alt.as_deref(), Some("New alt"));
        assert_eq!(updated.tags, vec!["updated-tag"]);

        // Verify sidecar was rewritten
        let new_content = fs::read_to_string(&abs_sidecar).unwrap();
        assert_ne!(original_content, new_content);
        let sc = read_sidecar(&new_content).unwrap();
        assert_eq!(sc.title.as_deref(), Some("New Title"));
        assert_eq!(sc.tags, vec!["updated-tag"]);
    }

    #[test]
    fn replace_media_file_preserves_identity_and_regenerates_artifacts() {
        let (db, dir) = setup();
        let source = create_test_png(dir.path());
        let media = import_media(
            db.conn(),
            dir.path(),
            "p1",
            &source,
            "photo.png",
            Some("Kept title"),
            None,
            None,
            None,
            None,
            vec!["kept".into()],
        )
        .unwrap();
        let replacement = dir.path().join("replacement.png");
        DynamicImage::new_rgb8(320, 180).save(&replacement).unwrap();
        let old_checksum = media.checksum.clone();

        let updated = replace_media_file(db.conn(), dir.path(), &media.id, &replacement)
            .unwrap()
            .unwrap();

        assert_eq!(updated.id, media.id);
        assert_eq!(updated.file_path, media.file_path);
        assert_eq!(updated.original_name, media.original_name);
        assert_eq!(updated.title, media.title);
        assert_eq!(updated.tags, media.tags);
        assert_eq!((updated.width, updated.height), (Some(320), Some(180)));
        assert_ne!(updated.checksum, old_checksum);
        assert!(!dir.path().join(format!("{}.bak", media.file_path)).exists());

        let sidecar =
            read_sidecar(&fs::read_to_string(dir.path().join(&updated.sidecar_path)).unwrap())
                .unwrap();
        assert_eq!((sidecar.width, sidecar.height), (Some(320), Some(180)));

        let prefix = &media.id[..2];
        let ai_thumb = dir
            .path()
            .join("thumbnails")
            .join(prefix)
            .join(format!("{}-ai.jpg", media.id));
        assert!(ai_thumb.is_file());
    }

    #[test]
    fn replace_media_file_is_noop_for_identical_content() {
        let (db, dir) = setup();
        let source = create_test_png(dir.path());
        let media = import_media(
            db.conn(),
            dir.path(),
            "p1",
            &source,
            "photo.png",
            None,
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();
        let stored = dir.path().join(&media.file_path);
        let updated_at = media.updated_at;

        assert!(
            replace_media_file(db.conn(), dir.path(), &media.id, &stored)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            qm::get_media_by_id(db.conn(), &media.id)
                .unwrap()
                .updated_at,
            updated_at
        );
    }

    #[test]
    fn delete_media_removes_everything() {
        let (db, dir) = setup();
        let source = create_test_png(dir.path());

        let media = import_media(
            db.conn(),
            dir.path(),
            "p1",
            &source,
            "photo.png",
            None,
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();

        let abs_file = dir.path().join(&media.file_path);
        let abs_sidecar = dir.path().join(&media.sidecar_path);
        assert!(abs_file.exists());
        assert!(abs_sidecar.exists());

        // Delete
        delete_media(db.conn(), dir.path(), &media.id).unwrap();

        // Verify file gone
        assert!(!abs_file.exists(), "binary file should be removed");

        // Verify sidecar gone
        assert!(!abs_sidecar.exists(), "sidecar should be removed");

        // Verify DB entry gone
        assert!(qm::get_media_by_id(db.conn(), &media.id).is_err());

        // Verify thumbnails gone
        let prefix = &media.id[..2];
        for size in THUMBNAIL_SIZES {
            let ext = match size.format {
                ThumbnailFormat::Webp => "webp",
                ThumbnailFormat::Jpeg => "jpg",
            };
            let thumb = dir
                .path()
                .join("thumbnails")
                .join(prefix)
                .join(format!("{}-{}.{ext}", media.id, size.name));
            assert!(!thumb.exists(), "thumbnail {} should be removed", size.name);
        }
    }

    #[test]
    fn upsert_media_translation_writes_sidecar() {
        let (db, dir) = setup();
        let source = create_test_png(dir.path());

        let media = import_media(
            db.conn(),
            dir.path(),
            "p1",
            &source,
            "photo.png",
            None,
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();

        // Create translation
        let t = upsert_media_translation(
            db.conn(),
            dir.path(),
            &media.id,
            "de",
            Some("Deutsches Foto"),
            Some("Ein Foto"),
            None,
        )
        .unwrap();

        assert_eq!(t.language, "de");
        assert_eq!(t.title.as_deref(), Some("Deutsches Foto"));

        // Verify translation sidecar file
        let sidecar_rel = media_translation_sidecar_path(&media.file_path, "de");
        let abs_sidecar = dir.path().join(&sidecar_rel);
        assert!(abs_sidecar.exists(), "translation sidecar should exist");

        let content = fs::read_to_string(&abs_sidecar).unwrap();
        let sc = read_translation_sidecar(&content).unwrap();
        assert_eq!(sc.language, "de");
        assert_eq!(sc.title.as_deref(), Some("Deutsches Foto"));
    }

    #[test]
    fn delete_media_translation_removes_sidecar() {
        let (db, dir) = setup();
        let source = create_test_png(dir.path());

        let media = import_media(
            db.conn(),
            dir.path(),
            "p1",
            &source,
            "photo.png",
            None,
            None,
            None,
            None,
            None,
            vec![],
        )
        .unwrap();

        // Create translation
        upsert_media_translation(
            db.conn(),
            dir.path(),
            &media.id,
            "de",
            Some("Titel"),
            None,
            None,
        )
        .unwrap();

        let sidecar_rel = media_translation_sidecar_path(&media.file_path, "de");
        let abs_sidecar = dir.path().join(&sidecar_rel);
        assert!(abs_sidecar.exists());

        // Delete translation
        delete_media_translation(db.conn(), dir.path(), &media.id, "de").unwrap();

        // Sidecar should be gone
        assert!(
            !abs_sidecar.exists(),
            "translation sidecar should be removed"
        );

        // DB entry should be gone
        assert!(
            qmt::get_media_translation_by_media_and_language(db.conn(), &media.id, "de").is_err()
        );
    }

    #[test]
    fn rebuild_media_from_filesystem_test() {
        let (db, dir) = setup();

        // Create a fake media file and its sidecar manually
        let media_subdir = dir.path().join("media").join("2024").join("01");
        fs::create_dir_all(&media_subdir).unwrap();

        // Write a dummy binary file
        let media_file = media_subdir.join("abcdef12-test-uuid.png");
        fs::write(&media_file, b"fake-png-data").unwrap();

        // Write a canonical sidecar
        let sidecar_content = "\
---
id: abcdef12-test-uuid
originalName: \"photo.png\"
mimeType: image/png
size: 13
title: \"Rebuild Test\"
alt: \"An image\"
createdAt: 2024-01-15T12:00:00.000Z
updatedAt: 2024-01-15T12:00:00.000Z
tags: [\"test\"]
---";
        fs::write(
            media_subdir.join("abcdef12-test-uuid.png.meta"),
            sidecar_content,
        )
        .unwrap();

        // Write a translation sidecar
        let trans_sidecar_content = "\
---
translationFor: abcdef12-test-uuid
language: de
title: \"Wiederherstellungstest\"
alt: \"Ein Bild\"
---";
        fs::write(
            media_subdir.join("abcdef12-test-uuid.png.de.meta"),
            trans_sidecar_content,
        )
        .unwrap();

        // Run rebuild
        let report = rebuild_media_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(report.media_created, 1);
        assert_eq!(report.translations_created, 1);
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);

        // Verify media in DB
        let media = qm::get_media_by_id(db.conn(), "abcdef12-test-uuid").unwrap();
        assert_eq!(media.title.as_deref(), Some("Rebuild Test"));
        assert_eq!(media.tags, vec!["test"]);
        assert_eq!(media.original_name, "photo.png");

        // Verify translation in DB
        let trans =
            qmt::get_media_translation_by_media_and_language(db.conn(), "abcdef12-test-uuid", "de")
                .unwrap();
        assert_eq!(trans.title.as_deref(), Some("Wiederherstellungstest"));

        // Run rebuild again - should update, not create
        let report2 = rebuild_media_from_filesystem(db.conn(), dir.path(), "p1").unwrap();
        assert_eq!(report2.media_created, 0);
        assert_eq!(report2.media_updated, 1);
        assert_eq!(report2.translations_created, 0);
        assert_eq!(report2.translations_updated, 1);
    }

    #[test]
    fn rebuild_media_trusts_sidecars_without_generating_thumbnails() {
        let (db, dir) = setup();
        let media_subdir = dir.path().join("media/2024/01");
        fs::create_dir_all(&media_subdir).unwrap();
        DynamicImage::new_rgb8(100, 80)
            .save(media_subdir.join("sidecar-only.png"))
            .unwrap();
        fs::write(
            media_subdir.join("sidecar-only.png.meta"),
            "---\nid: sidecar-only\noriginalName: \"photo.png\"\nmimeType: image/png\nsize: 123\nwidth: 640\nheight: 480\ncreatedAt: 2024-01-15T12:00:00.000Z\nupdatedAt: 2024-01-15T12:00:00.000Z\ntags: []\n---",
        )
        .unwrap();

        rebuild_media_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        let media = qm::get_media_by_id(db.conn(), "sidecar-only").unwrap();
        assert_eq!(
            (media.size, media.width, media.height),
            (123, Some(640), Some(480))
        );
        assert!(!dir.path().join("thumbnails").exists());
    }

    #[test]
    fn is_translation_sidecar_detection() {
        assert!(is_translation_sidecar("photo.jpg.de.meta"));
        assert!(is_translation_sidecar("photo.jpg.fr.meta"));
        assert!(is_translation_sidecar("uuid.png.en.meta"));
        assert!(!is_translation_sidecar("photo.jpg.meta"));
        assert!(!is_translation_sidecar("photo.meta"));
        assert!(!is_translation_sidecar("photo.jpg.123.meta"));
        assert!(!is_translation_sidecar("photo.jpg.DEU.meta"));
    }
}
