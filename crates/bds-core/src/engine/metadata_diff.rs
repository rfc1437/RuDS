use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::db::DbConnection as Connection;
use walkdir::WalkDir;

use crate::db::from_row::{script_kind_to_str, template_kind_to_str};
use crate::db::queries::media as qm;
use crate::db::queries::media_translation as qmt;
use crate::db::queries::post as qp;
use crate::db::queries::post_translation as qt;
use crate::db::queries::project as qproject;
use crate::db::queries::script as qs;
use crate::db::queries::template as qtpl;
use crate::engine::{EngineError, EngineResult};
use crate::model::{Media, MediaTranslation, Post, PostStatus, PostTranslation, Script, Template};
use crate::util::frontmatter::{
    ScriptFrontmatter, TemplateFrontmatter, read_post_file, read_script_file, read_template_file,
    read_translation_file, write_post_file, write_script_file, write_template_file,
    write_translation_file,
};
use crate::util::sidecar::{
    MediaSidecar, MediaTranslationSidecar, read_sidecar, read_translation_sidecar,
};
use crate::util::{atomic_write_str, media_translation_sidecar_path};

/// A single field difference.
#[derive(Debug, Clone)]
pub struct DiffField {
    pub field_name: String,
    pub db_value: String,
    pub file_value: String,
}

/// Diff for a single entity.
#[derive(Debug, Clone)]
pub struct EntityDiff {
    pub entity_type: String,
    pub entity_id: String,
    pub file_path: String,
    pub fields: Vec<DiffField>,
}

/// An orphan file (exists on disk but not in DB, or vice versa).
#[derive(Debug, Clone)]
pub struct OrphanFile {
    pub file_path: String,
    pub reason: String,
}

/// Complete diff report.
#[derive(Debug, Clone, Default)]
pub struct DiffReport {
    pub diffs: Vec<EntityDiff>,
    pub orphans: Vec<OrphanFile>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairDirection {
    FileToDatabase,
    DatabaseToFile,
}

/// Compare DB state vs filesystem files and report all differences.
///
/// This function does NOT modify anything -- it only reports differences.
pub fn compute_metadata_diff(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<DiffReport> {
    let mut report = DiffReport::default();

    if let Ok(project) = qproject::get_project_by_id(conn, project_id) {
        match diff_project(data_dir, &project) {
            Ok(Some(diff)) => report.diffs.push(diff),
            Ok(None) => {}
            Err(error) => report.errors.push(format!("project {project_id}: {error}")),
        }
    }

    // 1. Diff posts
    let posts = qp::list_posts_by_project(conn, project_id)?;
    for post in &posts {
        if post.file_path.is_empty() {
            continue;
        }
        match diff_post(data_dir, post) {
            Ok(Some(d)) => report.diffs.push(d),
            Ok(None) => {}
            Err(e) => report.errors.push(format!("post {}: {e}", post.id)),
        }
    }

    // 2. Diff translations
    for post in &posts {
        let translations = qt::list_post_translations_by_post(conn, &post.id)?;
        for t in &translations {
            if t.file_path.is_empty() {
                continue;
            }
            match diff_translation(data_dir, t) {
                Ok(Some(d)) => report.diffs.push(d),
                Ok(None) => {}
                Err(e) => report.errors.push(format!("translation {}: {e}", t.id)),
            }
        }
    }

    // 3. Diff media
    let media_items = qm::list_media_by_project(conn, project_id)?;
    for m in &media_items {
        if m.sidecar_path.is_empty() {
            continue;
        }

        let translations = qmt::list_media_translations_by_media(conn, &m.id)?;
        for translation in &translations {
            match diff_media_translation(data_dir, m, translation) {
                Ok(Some(diff)) => report.diffs.push(diff),
                Ok(None) => {}
                Err(error) => report
                    .errors
                    .push(format!("media translation {}: {error}", translation.id)),
            }
        }
        match diff_media(data_dir, m) {
            Ok(Some(d)) => report.diffs.push(d),
            Ok(None) => {}
            Err(e) => report.errors.push(format!("media {}: {e}", m.id)),
        }
    }

    // 4. Diff templates
    let templates = qtpl::list_templates_by_project(conn, project_id)?;
    for t in &templates {
        if t.file_path.is_empty() {
            continue;
        }
        match diff_template(data_dir, t) {
            Ok(Some(d)) => report.diffs.push(d),
            Ok(None) => {}
            Err(e) => report.errors.push(format!("template {}: {e}", t.id)),
        }
    }

    // 5. Diff scripts
    let scripts = qs::list_scripts_by_project(conn, project_id)?;
    for s in &scripts {
        if s.file_path.is_empty() {
            continue;
        }
        match diff_script(data_dir, s) {
            Ok(Some(d)) => report.diffs.push(d),
            Ok(None) => {}
            Err(e) => report.errors.push(format!("script {}: {e}", s.id)),
        }
    }

    // 6. Detect orphans
    let orphans = detect_orphan_files(conn, data_dir, project_id)?;
    report.orphans = orphans;

    Ok(report)
}

/// Resolve one reported difference in the selected direction.
pub fn repair_metadata_diff_item(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    direction: RepairDirection,
    item: &EntityDiff,
) -> EngineResult<()> {
    match direction {
        RepairDirection::FileToDatabase => {
            let path = data_dir.join(&item.file_path);
            match item.entity_type.as_str() {
                "project" => sync_project_from_file(conn, data_dir, project_id)?,
                "post" => {
                    crate::engine::post::rebuild_canonical_post(conn, data_dir, project_id, &path)?;
                }
                "post_translation" => {
                    crate::engine::post::rebuild_translation(conn, data_dir, project_id, &path)?;
                }
                "media" => {
                    crate::engine::media::rebuild_canonical_media(
                        conn, data_dir, project_id, &path,
                    )?;
                }
                "media_translation" => {
                    crate::engine::media::rebuild_translation_sidecar(
                        conn, data_dir, project_id, &path,
                    )?;
                }
                "script" => {
                    crate::engine::script_rebuild::rebuild_single_script(
                        conn, data_dir, project_id, &path,
                    )?;
                }
                "template" => {
                    crate::engine::template_rebuild::rebuild_single_template(
                        conn, data_dir, project_id, &path,
                    )?;
                }
                other => return unsupported_repair(other),
            }
        }
        RepairDirection::DatabaseToFile => match item.entity_type.as_str() {
            "project" => rewrite_project_from_database(conn, data_dir, project_id)?,
            "post" => rewrite_post_from_database(conn, data_dir, &item.entity_id)?,
            "post_translation" => {
                rewrite_post_translation_from_database(conn, data_dir, &item.entity_id)?
            }
            "media" => rewrite_media_from_database(conn, data_dir, &item.entity_id)?,
            "media_translation" => rewrite_media_translation_from_database(
                conn,
                data_dir,
                project_id,
                &item.entity_id,
            )?,
            "script" => rewrite_script_from_database(conn, data_dir, &item.entity_id)?,
            "template" => rewrite_template_from_database(conn, data_dir, &item.entity_id)?,
            other => return unsupported_repair(other),
        },
    }
    Ok(())
}

fn diff_project(
    data_dir: &Path,
    project: &crate::model::Project,
) -> EngineResult<Option<EntityDiff>> {
    let metadata = crate::engine::meta::read_project_json(data_dir)?;
    let mut fields = Vec::new();
    compare_field(&mut fields, "name", &project.name, &metadata.name);
    compare_field(
        &mut fields,
        "description",
        project.description.as_deref().unwrap_or(""),
        metadata.description.as_deref().unwrap_or(""),
    );
    Ok((!fields.is_empty()).then_some(EntityDiff {
        entity_type: "project".into(),
        entity_id: project.id.clone(),
        file_path: "meta/project.json".into(),
        fields,
    }))
}

fn sync_project_from_file(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<()> {
    crate::engine::meta::sync_project_from_file(conn, data_dir, project_id)
}

fn rewrite_project_from_database(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<()> {
    let project = qproject::get_project_by_id(conn, project_id)?;
    let mut metadata = crate::engine::meta::read_project_json(data_dir)?;
    metadata.name = project.name;
    metadata.description = project.description;
    crate::engine::meta::write_project_json(data_dir, &metadata)
}

fn unsupported_repair<T>(entity_type: &str) -> EngineResult<T> {
    Err(EngineError::Validation(format!(
        "unsupported metadata diff entity type: {entity_type}"
    )))
}

fn rewrite_post_from_database(
    conn: &Connection,
    data_dir: &Path,
    entity_id: &str,
) -> EngineResult<()> {
    let post = qp::get_post_by_id(conn, entity_id)?;
    let path = data_dir.join(&post.file_path);
    let body = fs::read_to_string(&path)
        .ok()
        .and_then(|content| read_post_file(&content).ok().map(|(_, body)| body))
        .unwrap_or_default();
    atomic_write_str(&path, &write_post_file(&post, &body))?;
    Ok(())
}

fn rewrite_post_translation_from_database(
    conn: &Connection,
    data_dir: &Path,
    entity_id: &str,
) -> EngineResult<()> {
    let translation = qt::get_post_translation_by_id(conn, entity_id)?;
    let path = data_dir.join(&translation.file_path);
    let body = fs::read_to_string(&path)
        .ok()
        .and_then(|content| read_translation_file(&content).ok().map(|(_, body)| body))
        .unwrap_or_default();
    atomic_write_str(&path, &write_translation_file(&translation, &body))?;
    Ok(())
}

fn rewrite_media_from_database(
    conn: &Connection,
    data_dir: &Path,
    entity_id: &str,
) -> EngineResult<()> {
    let media = qm::get_media_by_id(conn, entity_id)?;
    let linked = crate::db::queries::post_media::list_post_media_by_media(conn, entity_id)
        .unwrap_or_default();
    let post_ids = linked
        .into_iter()
        .map(|link| link.post_id)
        .collect::<Vec<_>>();
    atomic_write_str(
        &data_dir.join(&media.sidecar_path),
        &MediaSidecar::from_media(&media, &post_ids).to_string(),
    )?;
    Ok(())
}

fn rewrite_media_translation_from_database(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    entity_id: &str,
) -> EngineResult<()> {
    let media_items = qm::list_media_by_project(conn, project_id)?;
    for media in media_items {
        if let Some(translation) = qmt::list_media_translations_by_media(conn, &media.id)?
            .into_iter()
            .find(|translation| translation.id == entity_id)
        {
            let sidecar = MediaTranslationSidecar {
                translation_for: translation.translation_for,
                language: translation.language.clone(),
                title: translation.title,
                alt: translation.alt,
                caption: translation.caption,
            };
            let path = media_translation_sidecar_path(&media.file_path, &translation.language);
            atomic_write_str(&data_dir.join(path), &sidecar.to_string())?;
            return Ok(());
        }
    }
    Err(EngineError::NotFound(format!(
        "media translation {entity_id}"
    )))
}

fn rewrite_script_from_database(
    conn: &Connection,
    data_dir: &Path,
    entity_id: &str,
) -> EngineResult<()> {
    let script = qs::get_script_by_id(conn, entity_id)?;
    let path = data_dir.join(&script.file_path);
    let body = fs::read_to_string(&path)
        .ok()
        .and_then(|content| read_script_file(&content).ok().map(|(_, body)| body))
        .or(script.content.clone())
        .unwrap_or_default();
    let frontmatter = ScriptFrontmatter {
        id: script.id,
        project_id: Some(script.project_id),
        slug: script.slug,
        title: script.title,
        kind: script_kind_to_str(&script.kind).to_owned(),
        entrypoint: script.entrypoint,
        enabled: script.enabled,
        version: script.version,
        created_at: script.created_at,
        updated_at: script.updated_at,
    };
    atomic_write_str(&path, &write_script_file(&frontmatter, &body))?;
    Ok(())
}

fn rewrite_template_from_database(
    conn: &Connection,
    data_dir: &Path,
    entity_id: &str,
) -> EngineResult<()> {
    let template = qtpl::get_template_by_id(conn, entity_id)?;
    let path = data_dir.join(&template.file_path);
    let body = fs::read_to_string(&path)
        .ok()
        .and_then(|content| read_template_file(&content).ok().map(|(_, body)| body))
        .or(template.content.clone())
        .unwrap_or_default();
    let frontmatter = TemplateFrontmatter {
        id: template.id,
        project_id: Some(template.project_id),
        slug: template.slug,
        title: template.title,
        kind: template_kind_to_str(&template.kind).to_owned(),
        enabled: template.enabled,
        version: template.version,
        created_at: template.created_at,
        updated_at: template.updated_at,
    };
    atomic_write_str(&path, &write_template_file(&frontmatter, &body))?;
    Ok(())
}

// --- Internal helpers ---

fn opt_to_str(opt: &Option<String>) -> String {
    opt.clone().unwrap_or_default()
}

fn bool_to_str(b: bool) -> String {
    if b {
        "true".to_string()
    } else {
        "false".to_string()
    }
}

fn tags_to_json(tags: &[String]) -> String {
    serde_json::to_string(tags).unwrap_or_else(|_| "[]".to_string())
}

fn status_to_str(s: &PostStatus) -> &'static str {
    match s {
        PostStatus::Draft => "draft",
        PostStatus::Published => "published",
        PostStatus::Archived => "archived",
    }
}

fn compare_field(fields: &mut Vec<DiffField>, name: &str, db_val: &str, file_val: &str) {
    if db_val != file_val {
        fields.push(DiffField {
            field_name: name.to_string(),
            db_value: db_val.to_string(),
            file_value: file_val.to_string(),
        });
    }
}

fn diff_post(data_dir: &Path, post: &Post) -> EngineResult<Option<EntityDiff>> {
    let abs_path = data_dir.join(&post.file_path);
    if !abs_path.exists() {
        // Will be caught by orphan detection
        return Ok(None);
    }

    let content = fs::read_to_string(&abs_path)?;
    let (fm, _body) = read_post_file(&content).map_err(EngineError::Parse)?;

    let mut fields = Vec::new();

    compare_field(&mut fields, "title", &post.title, &fm.title);
    compare_field(&mut fields, "slug", &post.slug, &fm.slug);
    compare_field(
        &mut fields,
        "status",
        status_to_str(&post.status),
        &fm.status,
    );
    compare_field(
        &mut fields,
        "tags",
        &tags_to_json(&post.tags),
        &tags_to_json(&fm.tags),
    );
    compare_field(
        &mut fields,
        "categories",
        &tags_to_json(&post.categories),
        &tags_to_json(&fm.categories),
    );
    compare_field(
        &mut fields,
        "excerpt",
        &opt_to_str(&post.excerpt),
        &opt_to_str(&fm.excerpt),
    );
    compare_field(
        &mut fields,
        "author",
        &opt_to_str(&post.author),
        &opt_to_str(&fm.author),
    );
    compare_field(
        &mut fields,
        "language",
        &opt_to_str(&post.language),
        &opt_to_str(&fm.language),
    );
    compare_field(
        &mut fields,
        "doNotTranslate",
        &bool_to_str(post.do_not_translate),
        &bool_to_str(fm.do_not_translate),
    );
    compare_field(
        &mut fields,
        "templateSlug",
        &opt_to_str(&post.template_slug),
        &opt_to_str(&fm.template_slug),
    );
    compare_field(
        &mut fields,
        "createdAt",
        &post.created_at.to_string(),
        &fm.created_at.to_string(),
    );
    compare_field(
        &mut fields,
        "updatedAt",
        &post.updated_at.to_string(),
        &fm.updated_at.to_string(),
    );
    compare_field(
        &mut fields,
        "publishedAt",
        &post.published_at.map(|v| v.to_string()).unwrap_or_default(),
        &fm.published_at.map(|v| v.to_string()).unwrap_or_default(),
    );

    if fields.is_empty() {
        Ok(None)
    } else {
        Ok(Some(EntityDiff {
            entity_type: "post".to_string(),
            entity_id: post.id.clone(),
            file_path: post.file_path.clone(),
            fields,
        }))
    }
}

fn diff_translation(data_dir: &Path, t: &PostTranslation) -> EngineResult<Option<EntityDiff>> {
    let abs_path = data_dir.join(&t.file_path);
    if !abs_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&abs_path)?;
    let (fm, _body) = read_translation_file(&content).map_err(EngineError::Parse)?;

    let mut fields = Vec::new();

    compare_field(
        &mut fields,
        "translationFor",
        &t.translation_for,
        &fm.translation_for,
    );
    compare_field(&mut fields, "language", &t.language, &fm.language);
    compare_field(&mut fields, "title", &t.title, &fm.title);
    compare_field(
        &mut fields,
        "excerpt",
        &opt_to_str(&t.excerpt),
        &opt_to_str(&fm.excerpt),
    );

    if fields.is_empty() {
        Ok(None)
    } else {
        Ok(Some(EntityDiff {
            entity_type: "post_translation".to_string(),
            entity_id: t.id.clone(),
            file_path: t.file_path.clone(),
            fields,
        }))
    }
}

fn diff_media(data_dir: &Path, media: &Media) -> EngineResult<Option<EntityDiff>> {
    let abs_path = data_dir.join(&media.sidecar_path);
    if !abs_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&abs_path)?;
    let sc = read_sidecar(&content).map_err(EngineError::Parse)?;

    let mut fields = Vec::new();

    compare_field(
        &mut fields,
        "title",
        &opt_to_str(&media.title),
        &opt_to_str(&sc.title),
    );
    compare_field(
        &mut fields,
        "alt",
        &opt_to_str(&media.alt),
        &opt_to_str(&sc.alt),
    );
    compare_field(
        &mut fields,
        "caption",
        &opt_to_str(&media.caption),
        &opt_to_str(&sc.caption),
    );
    compare_field(
        &mut fields,
        "author",
        &opt_to_str(&media.author),
        &opt_to_str(&sc.author),
    );
    compare_field(
        &mut fields,
        "tags",
        &tags_to_json(&media.tags),
        &tags_to_json(&sc.tags),
    );
    compare_field(
        &mut fields,
        "language",
        &opt_to_str(&media.language),
        &opt_to_str(&sc.language),
    );

    if fields.is_empty() {
        Ok(None)
    } else {
        Ok(Some(EntityDiff {
            entity_type: "media".to_string(),
            entity_id: media.id.clone(),
            file_path: media.sidecar_path.clone(),
            fields,
        }))
    }
}

fn diff_media_translation(
    data_dir: &Path,
    media: &Media,
    translation: &MediaTranslation,
) -> EngineResult<Option<EntityDiff>> {
    let relative_path = media_translation_sidecar_path(&media.file_path, &translation.language);
    let absolute_path = data_dir.join(&relative_path);
    if !absolute_path.is_file() {
        return Ok(None);
    }
    let sidecar = read_translation_sidecar(&fs::read_to_string(&absolute_path)?)
        .map_err(EngineError::Parse)?;
    let mut fields = Vec::new();
    compare_field(
        &mut fields,
        "translationFor",
        &translation.translation_for,
        &sidecar.translation_for,
    );
    compare_field(
        &mut fields,
        "language",
        &translation.language,
        &sidecar.language,
    );
    compare_field(
        &mut fields,
        "title",
        &opt_to_str(&translation.title),
        &opt_to_str(&sidecar.title),
    );
    compare_field(
        &mut fields,
        "alt",
        &opt_to_str(&translation.alt),
        &opt_to_str(&sidecar.alt),
    );
    compare_field(
        &mut fields,
        "caption",
        &opt_to_str(&translation.caption),
        &opt_to_str(&sidecar.caption),
    );

    Ok((!fields.is_empty()).then_some(EntityDiff {
        entity_type: "media_translation".into(),
        entity_id: translation.id.clone(),
        file_path: relative_path,
        fields,
    }))
}

fn diff_template(data_dir: &Path, tpl: &Template) -> EngineResult<Option<EntityDiff>> {
    let abs_path = data_dir.join(&tpl.file_path);
    if !abs_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&abs_path)?;
    let (fm, _body) = read_template_file(&content).map_err(EngineError::Parse)?;

    let mut fields = Vec::new();

    compare_field(&mut fields, "title", &tpl.title, &fm.title);
    compare_field(
        &mut fields,
        "kind",
        template_kind_to_str(&tpl.kind),
        &fm.kind,
    );
    compare_field(
        &mut fields,
        "enabled",
        &bool_to_str(tpl.enabled),
        &bool_to_str(fm.enabled),
    );
    compare_field(
        &mut fields,
        "version",
        &tpl.version.to_string(),
        &fm.version.to_string(),
    );

    if fields.is_empty() {
        Ok(None)
    } else {
        Ok(Some(EntityDiff {
            entity_type: "template".to_string(),
            entity_id: tpl.id.clone(),
            file_path: tpl.file_path.clone(),
            fields,
        }))
    }
}

fn diff_script(data_dir: &Path, script: &Script) -> EngineResult<Option<EntityDiff>> {
    let abs_path = data_dir.join(&script.file_path);
    if !abs_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&abs_path)?;
    let (fm, _body) = read_script_file(&content).map_err(EngineError::Parse)?;

    let mut fields = Vec::new();

    compare_field(&mut fields, "title", &script.title, &fm.title);
    compare_field(
        &mut fields,
        "kind",
        script_kind_to_str(&script.kind),
        &fm.kind,
    );
    compare_field(
        &mut fields,
        "entrypoint",
        &script.entrypoint,
        &fm.entrypoint,
    );
    compare_field(
        &mut fields,
        "enabled",
        &bool_to_str(script.enabled),
        &bool_to_str(fm.enabled),
    );
    compare_field(
        &mut fields,
        "version",
        &script.version.to_string(),
        &fm.version.to_string(),
    );

    if fields.is_empty() {
        Ok(None)
    } else {
        Ok(Some(EntityDiff {
            entity_type: "script".to_string(),
            entity_id: script.id.clone(),
            file_path: script.file_path.clone(),
            fields,
        }))
    }
}

fn detect_orphan_files(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<Vec<OrphanFile>> {
    let mut orphans = Vec::new();

    // Collect all known file paths from DB
    let mut db_file_paths: HashSet<String> = HashSet::new();

    let posts = qp::list_posts_by_project(conn, project_id)?;
    for post in &posts {
        if !post.file_path.is_empty() {
            db_file_paths.insert(post.file_path.clone());
        }
        let translations = qt::list_post_translations_by_post(conn, &post.id)?;
        for t in &translations {
            if !t.file_path.is_empty() {
                db_file_paths.insert(t.file_path.clone());
            }
        }
    }

    let media_items = qm::list_media_by_project(conn, project_id)?;
    for m in &media_items {
        if !m.file_path.is_empty() {
            db_file_paths.insert(m.file_path.clone());
        }
        if !m.sidecar_path.is_empty() {
            db_file_paths.insert(m.sidecar_path.clone());
        }
        for translation in qmt::list_media_translations_by_media(conn, &m.id)? {
            db_file_paths.insert(media_translation_sidecar_path(
                &m.file_path,
                &translation.language,
            ));
        }
    }

    let templates = qtpl::list_templates_by_project(conn, project_id)?;
    for t in &templates {
        if !t.file_path.is_empty() {
            db_file_paths.insert(t.file_path.clone());
        }
    }

    let scripts = qs::list_scripts_by_project(conn, project_id)?;
    for s in &scripts {
        if !s.file_path.is_empty() {
            db_file_paths.insert(s.file_path.clone());
        }
    }

    // Walk filesystem directories and find orphans
    let dirs_to_walk = ["posts", "media", "templates", "scripts"];
    for dir_name in &dirs_to_walk {
        let dir_path = data_dir.join(dir_name);
        if !dir_path.exists() {
            continue;
        }
        for entry in WalkDir::new(&dir_path).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }

            // Compute relative path from data_dir
            let rel_path = path
                .strip_prefix(data_dir)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();

            // Skip non-content files (thumbnails, etc.)
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let is_content_file = match *dir_name {
                "posts" => ext == "md",
                "media" => ext == "meta",
                "templates" => ext == "liquid",
                "scripts" => ext == "lua",
                _ => false,
            };

            if !is_content_file {
                continue;
            }

            if !db_file_paths.contains(&rel_path) {
                orphans.push(OrphanFile {
                    file_path: rel_path,
                    reason: "file_without_db_entry".to_string(),
                });
            }
        }
    }

    // Check DB entries whose file_path doesn't exist on disk
    for post in &posts {
        if !post.file_path.is_empty() {
            let abs = data_dir.join(&post.file_path);
            if !abs.exists() {
                orphans.push(OrphanFile {
                    file_path: post.file_path.clone(),
                    reason: "db_entry_without_file".to_string(),
                });
            }
        }
        let translations = qt::list_post_translations_by_post(conn, &post.id)?;
        for t in &translations {
            if !t.file_path.is_empty() {
                let abs = data_dir.join(&t.file_path);
                if !abs.exists() {
                    orphans.push(OrphanFile {
                        file_path: t.file_path.clone(),
                        reason: "db_entry_without_file".to_string(),
                    });
                }
            }
        }
    }

    for m in &media_items {
        if !m.sidecar_path.is_empty() {
            let abs = data_dir.join(&m.sidecar_path);
            if !abs.exists() {
                orphans.push(OrphanFile {
                    file_path: m.sidecar_path.clone(),
                    reason: "db_entry_without_file".to_string(),
                });
            }
        }
        for translation in qmt::list_media_translations_by_media(conn, &m.id)? {
            let path = media_translation_sidecar_path(&m.file_path, &translation.language);
            if !data_dir.join(&path).is_file() {
                orphans.push(OrphanFile {
                    file_path: path,
                    reason: "db_entry_without_file".to_string(),
                });
            }
        }
    }

    for t in &templates {
        if !t.file_path.is_empty() {
            let abs = data_dir.join(&t.file_path);
            if !abs.exists() {
                orphans.push(OrphanFile {
                    file_path: t.file_path.clone(),
                    reason: "db_entry_without_file".to_string(),
                });
            }
        }
    }

    for s in &scripts {
        if !s.file_path.is_empty() {
            let abs = data_dir.join(&s.file_path);
            if !abs.exists() {
                orphans.push(OrphanFile {
                    file_path: s.file_path.clone(),
                    reason: "db_entry_without_file".to_string(),
                });
            }
        }
    }

    Ok(orphans)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::fts::ensure_fts_tables;
    use crate::db::queries::media::insert_media;
    use crate::db::queries::post::insert_post;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::db::queries::script::insert_script;
    use crate::db::queries::template::insert_template;
    use crate::engine::post::{create_post, publish_post};
    use crate::model::{
        Media, Post, PostStatus, ProjectMetadata, Script, ScriptKind, ScriptStatus, Template,
        TemplateKind, TemplateStatus,
    };
    use crate::util::frontmatter::{
        ScriptFrontmatter, TemplateFrontmatter, write_script_file, write_template_file,
    };
    use crate::util::sidecar::MediaSidecar;
    use tempfile::TempDir;

    fn setup() -> (Database, TempDir) {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        ensure_fts_tables(db.conn()).unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = TempDir::new().unwrap();
        (db, dir)
    }

    #[test]
    fn no_diffs_for_clean_state() {
        let (db, dir) = setup();

        // Create and publish a post via the engine
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Clean Post",
            Some("body text"),
            vec!["rust".into()],
            vec!["tech".into()],
            Some("Alice"),
            Some("en"),
            None,
        )
        .unwrap();

        publish_post(db.conn(), dir.path(), &post.id).unwrap();

        let report = compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();

        // Published post: file was just written by publish, DB and file should match.
        // The only expected diff is updatedAt because publish sets it to now() in the DB
        // but the file was written with that same now(). They should match.
        // However, publish_post updates updatedAt multiple times, so the DB value may
        // differ from the file. Filter to only non-updatedAt diffs.
        let non_time_diffs: Vec<_> = report
            .diffs
            .iter()
            .filter(|d| d.fields.iter().any(|f| f.field_name != "updatedAt"))
            .collect();
        assert!(
            non_time_diffs.is_empty(),
            "expected no non-timestamp diffs, got: {non_time_diffs:?}"
        );
        assert!(report.orphans.is_empty(), "orphans: {:?}", report.orphans);
    }

    #[test]
    fn detects_title_drift_in_post() {
        let (db, dir) = setup();

        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Original Title",
            Some("body"),
            vec!["rust".into()],
            vec!["tech".into()],
            None,
            None,
            None,
        )
        .unwrap();

        let published = publish_post(db.conn(), dir.path(), &post.id).unwrap();

        // Manually edit the .md file to change the title
        let abs_path = dir.path().join(&published.file_path);
        let content = fs::read_to_string(&abs_path).unwrap();
        let modified = content.replace("Original Title", "Tampered Title");
        fs::write(&abs_path, modified).unwrap();

        let report = compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();

        // Should detect title drift
        let title_diffs: Vec<_> = report
            .diffs
            .iter()
            .filter(|d| d.entity_type == "post")
            .flat_map(|d| d.fields.iter())
            .filter(|f| f.field_name == "title")
            .collect();

        assert_eq!(title_diffs.len(), 1);
        assert_eq!(title_diffs[0].db_value, "Original Title");
        assert_eq!(title_diffs[0].file_value, "Tampered Title");
    }

    #[test]
    fn detects_missing_project_description() {
        let (db, dir) = setup();
        crate::engine::meta::write_project_json(
            dir.path(),
            &ProjectMetadata {
                name: "Project p1".into(),
                description: None,
                public_url: None,
                main_language: None,
                default_author: None,
                max_posts_per_page: 50,
                image_import_concurrency: 4,
                blogmark_category: None,
                pico_theme: None,
                semantic_similarity_enabled: false,
                blog_languages: vec![],
            },
        )
        .unwrap();

        let report = compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();
        let project = report
            .diffs
            .iter()
            .find(|item| item.entity_type == "project")
            .unwrap();
        assert!(project.fields.iter().any(|field| {
            field.field_name == "description"
                && field.db_value == "A test project"
                && field.file_value.is_empty()
        }));
    }

    #[test]
    fn ignores_python_scripts_during_orphan_scan() {
        let (db, dir) = setup();
        let scripts = dir.path().join("scripts");
        fs::create_dir_all(&scripts).unwrap();
        fs::write(scripts.join("legacy.py"), "print('legacy')").unwrap();

        let report = compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();

        assert!(
            report
                .orphans
                .iter()
                .all(|orphan| !orphan.file_path.ends_with(".py"))
        );
    }

    #[test]
    fn repairs_one_post_diff_from_file_to_database() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Database Title",
            Some("body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let published = publish_post(db.conn(), dir.path(), &post.id).unwrap();
        let path = dir.path().join(&published.file_path);
        let content = fs::read_to_string(&path).unwrap();
        fs::write(&path, content.replace("Database Title", "Filesystem Title")).unwrap();
        let report = compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();
        let item = report
            .diffs
            .iter()
            .find(|item| item.entity_type == "post")
            .unwrap();

        repair_metadata_diff_item(
            db.conn(),
            dir.path(),
            "p1",
            RepairDirection::FileToDatabase,
            item,
        )
        .unwrap();

        assert_eq!(
            qp::get_post_by_id(db.conn(), &post.id).unwrap().title,
            "Filesystem Title"
        );
    }

    #[test]
    fn repairs_one_post_diff_from_database_to_file_without_losing_body() {
        let (db, dir) = setup();
        let post = create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Database Title",
            Some("body that must survive"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let published = publish_post(db.conn(), dir.path(), &post.id).unwrap();
        let path = dir.path().join(&published.file_path);
        let content = fs::read_to_string(&path).unwrap();
        fs::write(&path, content.replace("Database Title", "Filesystem Title")).unwrap();
        let report = compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();
        let item = report
            .diffs
            .iter()
            .find(|item| item.entity_type == "post")
            .unwrap();

        repair_metadata_diff_item(
            db.conn(),
            dir.path(),
            "p1",
            RepairDirection::DatabaseToFile,
            item,
        )
        .unwrap();

        let repaired = fs::read_to_string(path).unwrap();
        assert!(repaired.contains("title: Database Title"));
        assert!(repaired.contains("body that must survive"));
    }

    #[test]
    fn detects_media_sidecar_drift() {
        let (db, dir) = setup();

        // Create media in DB with sidecar
        let media = Media {
            id: "m1".to_string(),
            project_id: "p1".to_string(),
            filename: "m1.jpg".to_string(),
            original_name: "photo.jpg".to_string(),
            mime_type: "image/jpeg".to_string(),
            size: 50000,
            width: Some(800),
            height: Some(600),
            title: Some("My Photo".to_string()),
            alt: Some("A nice photo".to_string()),
            caption: None,
            author: None,
            language: None,
            file_path: "media/2024/01/m1.jpg".to_string(),
            sidecar_path: "media/2024/01/m1.jpg.meta".to_string(),
            checksum: None,
            tags: vec![],
            created_at: 1000,
            updated_at: 2000,
        };
        insert_media(db.conn(), &media).unwrap();

        // Write sidecar with matching data initially, then change alt
        let sidecar = MediaSidecar {
            id: "m1".to_string(),
            original_name: "photo.jpg".to_string(),
            mime_type: "image/jpeg".to_string(),
            size: 50000,
            width: Some(800),
            height: Some(600),
            title: Some("My Photo".to_string()),
            alt: Some("TAMPERED ALT".to_string()),
            caption: None,
            author: None,
            language: None,
            created_at: 1000,
            updated_at: 2000,
            tags: vec![],
            linked_post_ids: vec![],
        };

        let sidecar_dir = dir.path().join("media/2024/01");
        fs::create_dir_all(&sidecar_dir).unwrap();
        fs::write(sidecar_dir.join("m1.jpg.meta"), sidecar.to_string()).unwrap();

        let report = compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();

        let alt_diffs: Vec<_> = report
            .diffs
            .iter()
            .filter(|d| d.entity_type == "media")
            .flat_map(|d| d.fields.iter())
            .filter(|f| f.field_name == "alt")
            .collect();

        assert_eq!(alt_diffs.len(), 1);
        assert_eq!(alt_diffs[0].db_value, "A nice photo");
        assert_eq!(alt_diffs[0].file_value, "TAMPERED ALT");
    }

    #[test]
    fn detects_orphan_file() {
        let (db, dir) = setup();

        // Create a .md file in posts/ that is not in the DB
        let posts_dir = dir.path().join("posts/2024/01");
        fs::create_dir_all(&posts_dir).unwrap();
        fs::write(
            posts_dir.join("orphan.md"),
            "---\ntitle: Orphan\n---\nBody\n",
        )
        .unwrap();

        let report = compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();

        let orphan_files: Vec<_> = report
            .orphans
            .iter()
            .filter(|o| o.reason == "file_without_db_entry")
            .collect();

        assert!(
            !orphan_files.is_empty(),
            "expected at least one orphan file"
        );
        assert!(
            orphan_files
                .iter()
                .any(|o| o.file_path.contains("orphan.md")),
            "expected orphan.md in orphans, got: {orphan_files:?}"
        );
    }

    #[test]
    fn detects_db_without_file() {
        let (db, dir) = setup();

        // Insert post in DB with file_path pointing to a non-existent file
        let post = Post {
            id: "ghost-post".to_string(),
            project_id: "p1".to_string(),
            title: "Ghost".to_string(),
            slug: "ghost".to_string(),
            excerpt: None,
            content: None,
            status: PostStatus::Published,
            author: None,
            language: None,
            do_not_translate: false,
            template_slug: None,
            file_path: "posts/2024/01/ghost.md".to_string(),
            checksum: None,
            tags: vec![],
            categories: vec![],
            published_title: None,
            published_content: None,
            published_tags: None,
            published_categories: None,
            published_excerpt: None,
            created_at: 1000,
            updated_at: 2000,
            published_at: Some(3000),
        };
        insert_post(db.conn(), &post).unwrap();

        let report = compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();

        let db_orphans: Vec<_> = report
            .orphans
            .iter()
            .filter(|o| o.reason == "db_entry_without_file")
            .collect();

        assert!(
            !db_orphans.is_empty(),
            "expected at least one db_entry_without_file orphan"
        );
        assert!(
            db_orphans.iter().any(|o| o.file_path.contains("ghost.md")),
            "expected ghost.md in orphans, got: {db_orphans:?}"
        );
    }

    #[test]
    fn detects_template_drift() {
        let (db, dir) = setup();

        // Insert template in DB
        let tpl = Template {
            id: "tpl1".to_string(),
            project_id: "p1".to_string(),
            slug: "my-template".to_string(),
            title: "My Template".to_string(),
            kind: TemplateKind::Post,
            enabled: true,
            version: 1,
            file_path: "templates/my-template.liquid".to_string(),
            status: TemplateStatus::Published,
            content: None,
            created_at: 1000,
            updated_at: 2000,
        };
        insert_template(db.conn(), &tpl).unwrap();

        // Write template file with different title
        let fm = TemplateFrontmatter {
            id: "tpl1".to_string(),
            project_id: Some("p1".to_string()),
            slug: "my-template".to_string(),
            title: "CHANGED Template Title".to_string(),
            kind: "post".to_string(),
            enabled: true,
            version: 1,
            created_at: 1000,
            updated_at: 2000,
        };
        let file_content = write_template_file(&fm, "<div>body</div>");
        let tpl_dir = dir.path().join("templates");
        fs::create_dir_all(&tpl_dir).unwrap();
        fs::write(tpl_dir.join("my-template.liquid"), file_content).unwrap();

        let report = compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();

        let title_diffs: Vec<_> = report
            .diffs
            .iter()
            .filter(|d| d.entity_type == "template")
            .flat_map(|d| d.fields.iter())
            .filter(|f| f.field_name == "title")
            .collect();

        assert_eq!(title_diffs.len(), 1);
        assert_eq!(title_diffs[0].db_value, "My Template");
        assert_eq!(title_diffs[0].file_value, "CHANGED Template Title");
    }

    #[test]
    fn detects_script_drift() {
        let (db, dir) = setup();

        // Insert script in DB
        let script = Script {
            id: "s1".to_string(),
            project_id: "p1".to_string(),
            slug: "my-script".to_string(),
            title: "My Script".to_string(),
            kind: ScriptKind::Utility,
            entrypoint: "main".to_string(),
            enabled: true,
            version: 1,
            file_path: "scripts/my-script.lua".to_string(),
            status: ScriptStatus::Published,
            content: None,
            created_at: 1000,
            updated_at: 2000,
        };
        insert_script(db.conn(), &script).unwrap();

        // Write script file with different title and version
        let fm = ScriptFrontmatter {
            id: "s1".to_string(),
            project_id: Some("p1".to_string()),
            slug: "my-script".to_string(),
            title: "CHANGED Script Title".to_string(),
            kind: "utility".to_string(),
            entrypoint: "main".to_string(),
            enabled: true,
            version: 5,
            created_at: 1000,
            updated_at: 2000,
        };
        let file_content = write_script_file(&fm, "-- lua code\nreturn 1");
        let scripts_dir = dir.path().join("scripts");
        fs::create_dir_all(&scripts_dir).unwrap();
        fs::write(scripts_dir.join("my-script.lua"), file_content).unwrap();

        let report = compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();

        let script_diffs: Vec<_> = report
            .diffs
            .iter()
            .filter(|d| d.entity_type == "script")
            .collect();

        assert!(!script_diffs.is_empty(), "expected script diffs");

        let title_diffs: Vec<_> = script_diffs
            .iter()
            .flat_map(|d| d.fields.iter())
            .filter(|f| f.field_name == "title")
            .collect();
        assert_eq!(title_diffs.len(), 1);
        assert_eq!(title_diffs[0].db_value, "My Script");
        assert_eq!(title_diffs[0].file_value, "CHANGED Script Title");

        let version_diffs: Vec<_> = script_diffs
            .iter()
            .flat_map(|d| d.fields.iter())
            .filter(|f| f.field_name == "version")
            .collect();
        assert_eq!(version_diffs.len(), 1);
        assert_eq!(version_diffs[0].db_value, "1");
        assert_eq!(version_diffs[0].file_value, "5");
    }
}
