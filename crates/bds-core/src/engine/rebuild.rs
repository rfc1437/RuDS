use std::path::Path;
use std::sync::Arc;

use crate::db::DbConnection as Connection;
use diesel::prelude::*;

use crate::db::fts;
use crate::engine::EngineResult;
use crate::engine::media;
use crate::engine::post;
use crate::engine::script_rebuild;
use crate::engine::template_rebuild;

/// Report from a full rebuild operation.
#[derive(Debug, Default)]
pub struct FullRebuildReport {
    pub posts_created: usize,
    pub posts_updated: usize,
    pub translations_created: usize,
    pub translations_updated: usize,
    pub media_created: usize,
    pub media_updated: usize,
    pub media_translations_created: usize,
    pub media_translations_updated: usize,
    pub templates_created: usize,
    pub templates_updated: usize,
    pub scripts_created: usize,
    pub scripts_updated: usize,
    pub errors: Vec<String>,
}

/// Progress callback: (percent 0.0..1.0, phase description).
pub type ProgressFn = Arc<dyn Fn(f32, &str) + Send + Sync>;

/// Orchestrate a full rebuild from filesystem into the database.
///
/// Replaces the project-scoped database state with project metadata, content,
/// and relationships reconstructed from the filesystem.
pub fn rebuild_from_filesystem(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<FullRebuildReport> {
    rebuild_from_filesystem_with_progress(conn, data_dir, project_id, None)
}

/// Like `rebuild_from_filesystem` but accepts an optional progress callback.
pub fn rebuild_from_filesystem_with_progress(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    on_progress: Option<ProgressFn>,
) -> EngineResult<FullRebuildReport> {
    conn.begin_savepoint()?;
    match rebuild_from_filesystem_inner(conn, data_dir, project_id, on_progress) {
        Ok(report) => {
            conn.release_savepoint()?;
            Ok(report)
        }
        Err(error) => {
            let _ = conn.rollback_savepoint();
            Err(error)
        }
    }
}

fn rebuild_from_filesystem_inner(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    on_progress: Option<ProgressFn>,
) -> EngineResult<FullRebuildReport> {
    let mut report = FullRebuildReport::default();
    let progress = |pct: f32, msg: &str| {
        if let Some(ref f) = on_progress {
            f(pct, msg);
        }
    };

    // Phase weights: posts 0.0..0.35, media 0.35..0.70, templates 0.70..0.85, scripts 0.85..1.0

    // 1. Load portable project metadata and clear all reconstructible rows.
    progress(0.0, "Loading project metadata...");
    fts::ensure_fts_tables(conn)?;
    crate::engine::meta::sync_project_from_file(conn, data_dir, project_id)?;
    clear_project_rows(conn, project_id)?;

    // 2. Rebuild posts  (0.00 .. 0.35)
    progress(0.01, "Scanning posts...");
    let post_item_cb: Option<post::ItemProgressFn> = on_progress.as_ref().map(|cb| {
        let cb = Arc::clone(cb);
        let f: post::ItemProgressFn = Box::new(move |current, total, name| {
            let phase_pct = if total > 0 {
                current as f32 / total as f32
            } else {
                1.0
            };
            let global_pct = 0.01 + phase_pct * 0.34;
            let msg = format!("Posts: {current}/{total} \u{2014} {name}");
            cb(global_pct, &msg);
        });
        f
    });
    let post_report = post::rebuild_posts_from_filesystem_with_progress(
        conn,
        data_dir,
        project_id,
        post_item_cb,
    )?;
    report.posts_created = post_report.posts_created;
    report.posts_updated = post_report.posts_updated;
    report.translations_created = post_report.translations_created;
    report.translations_updated = post_report.translations_updated;
    report.errors.extend(post_report.errors);

    // 3. Rebuild media  (0.35 .. 0.70)
    progress(0.35, "Scanning media...");
    let media_item_cb: Option<media::ItemProgressFn> = on_progress.as_ref().map(|cb| {
        let cb = Arc::clone(cb);
        let f: media::ItemProgressFn = Box::new(move |current, total, name| {
            let phase_pct = if total > 0 {
                current as f32 / total as f32
            } else {
                1.0
            };
            let global_pct = 0.35 + phase_pct * 0.35;
            let msg = format!("Media: {current}/{total} \u{2014} {name}");
            cb(global_pct, &msg);
        });
        f
    });
    let media_report = media::rebuild_media_from_filesystem_with_progress(
        conn,
        data_dir,
        project_id,
        media_item_cb,
    )?;
    report.media_created = media_report.media_created;
    report.media_updated = media_report.media_updated;
    report.media_translations_created = media_report.translations_created;
    report.media_translations_updated = media_report.translations_updated;
    report.errors.extend(media_report.errors);

    // 4. Rebuild templates  (0.70 .. 0.85)
    progress(0.70, "Rebuilding templates...");
    let tpl_report =
        template_rebuild::rebuild_templates_from_filesystem(conn, data_dir, project_id)?;
    report.templates_created = tpl_report.created;
    report.templates_updated = tpl_report.updated;
    report.errors.extend(tpl_report.errors);

    // 5. Rebuild scripts  (0.85 .. 0.95)
    progress(0.85, "Rebuilding scripts...");
    let script_report =
        script_rebuild::rebuild_scripts_from_filesystem(conn, data_dir, project_id)?;
    report.scripts_created = script_report.created;
    report.scripts_updated = script_report.updated;
    report.errors.extend(script_report.errors);

    // 6. Restore relationships and tags (0.95 .. 1.0)
    progress(0.95, "Importing tags...");
    super::tag::import_tags_from_file(conn, data_dir, project_id)?;
    super::tag::sync_tags_from_posts(conn, project_id)?;
    post::rebuild_all_links(conn, data_dir, project_id)?;

    if !report.errors.is_empty() {
        return Err(crate::engine::EngineError::Validation(format!(
            "rebuild failed: {}",
            report.errors.join("; ")
        )));
    }

    progress(1.0, "Rebuild complete");
    Ok(report)
}

fn clear_project_rows(conn: &Connection, project_id: &str) -> EngineResult<()> {
    use crate::db::schema::{
        dismissed_duplicate_pairs, embedding_keys, generated_file_hashes, import_definitions,
        media, media_translations, post_links, post_media, post_translations, posts, scripts, tags,
        templates,
    };

    let post_ids = crate::db::queries::post::list_posts_by_project(conn, project_id)?
        .into_iter()
        .map(|post| post.id)
        .collect::<Vec<_>>();
    let media_ids = crate::db::queries::media::list_media_by_project(conn, project_id)?
        .into_iter()
        .map(|media| media.id)
        .collect::<Vec<_>>();
    for id in &post_ids {
        fts::remove_post_from_index(conn, id)?;
    }
    for id in &media_ids {
        fts::remove_media_from_index(conn, id)?;
    }

    conn.with(|connection| {
        diesel::delete(
            post_links::table.filter(
                post_links::source_post_id
                    .eq_any(&post_ids)
                    .or(post_links::target_post_id.eq_any(&post_ids)),
            ),
        )
        .execute(connection)?;
        diesel::delete(post_media::table.filter(post_media::project_id.eq(project_id)))
            .execute(connection)?;
        diesel::delete(
            post_translations::table.filter(post_translations::project_id.eq(project_id)),
        )
        .execute(connection)?;
        diesel::delete(
            media_translations::table.filter(media_translations::project_id.eq(project_id)),
        )
        .execute(connection)?;
        diesel::delete(
            generated_file_hashes::table.filter(generated_file_hashes::project_id.eq(project_id)),
        )
        .execute(connection)?;
        diesel::delete(embedding_keys::table.filter(embedding_keys::project_id.eq(project_id)))
            .execute(connection)?;
        diesel::delete(
            dismissed_duplicate_pairs::table
                .filter(dismissed_duplicate_pairs::project_id.eq(project_id)),
        )
        .execute(connection)?;
        diesel::delete(
            import_definitions::table.filter(import_definitions::project_id.eq(project_id)),
        )
        .execute(connection)?;
        diesel::delete(tags::table.filter(tags::project_id.eq(project_id))).execute(connection)?;
        diesel::delete(scripts::table.filter(scripts::project_id.eq(project_id)))
            .execute(connection)?;
        diesel::delete(templates::table.filter(templates::project_id.eq(project_id)))
            .execute(connection)?;
        diesel::delete(media::table.filter(media::project_id.eq(project_id)))
            .execute(connection)?;
        diesel::delete(posts::table.filter(posts::project_id.eq(project_id)))
            .execute(connection)?;
        Ok(())
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::media as qm;
    use crate::db::queries::post as qp;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::db::queries::script as qs;
    use crate::db::queries::tag as qtag;
    use crate::db::queries::template as qtpl;
    use crate::model::metadata::ProjectMetadata;
    use crate::model::{
        Script, ScriptKind, ScriptStatus, Tag, Template, TemplateKind, TemplateStatus,
    };
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (Database, TempDir) {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        fts::ensure_fts_tables(db.conn()).unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = TempDir::new().unwrap();
        crate::engine::meta::write_project_json(
            dir.path(),
            &ProjectMetadata {
                name: "Project p1".into(),
                description: Some("A test project".into()),
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
        (db, dir)
    }

    #[test]
    fn rebuild_empty_dir_returns_zeros() {
        let (db, dir) = setup();

        let report = rebuild_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(report.posts_created, 0);
        assert_eq!(report.posts_updated, 0);
        assert_eq!(report.translations_created, 0);
        assert_eq!(report.translations_updated, 0);
        assert_eq!(report.media_created, 0);
        assert_eq!(report.media_updated, 0);
        assert_eq!(report.media_translations_created, 0);
        assert_eq!(report.media_translations_updated, 0);
        assert_eq!(report.templates_created, 0);
        assert_eq!(report.templates_updated, 0);
        assert_eq!(report.scripts_created, 0);
        assert_eq!(report.scripts_updated, 0);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn rebuild_creates_posts_and_media() {
        let (db, dir) = setup();

        // Write a post fixture
        let posts_dir = dir.path().join("posts").join("2024").join("01");
        fs::create_dir_all(&posts_dir).unwrap();

        let post_content = "\
---
id: test-post-1
title: Test Post
slug: test-post
status: published
createdAt: '2024-01-15T12:00:00.000Z'
updatedAt: '2024-01-15T12:00:00.000Z'
tags:
  - test
categories: []
publishedAt: '2024-01-15T12:00:00.000Z'
---
Hello from rebuild test!
";
        fs::write(posts_dir.join("test-post.md"), post_content).unwrap();

        // Write a media fixture: sidecar + dummy binary
        let media_dir = dir.path().join("media").join("2024").join("01");
        fs::create_dir_all(&media_dir).unwrap();

        let media_file = media_dir.join("test-media-1.jpg");
        fs::write(&media_file, b"fake-jpeg-data").unwrap();

        let sidecar_content = "\
---
id: test-media-1
originalName: \"photo.jpg\"
mimeType: image/jpeg
size: 12345
width: 800
height: 600
createdAt: 2024-01-15T12:00:00.000Z
updatedAt: 2024-01-15T12:00:00.000Z
tags: []
---
";
        fs::write(media_dir.join("test-media-1.jpg.meta"), sidecar_content).unwrap();

        let report = rebuild_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(report.posts_created, 1);
        assert_eq!(report.media_created, 1);
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);

        // Verify post in DB
        let post = qp::get_post_by_id(db.conn(), "test-post-1").unwrap();
        assert_eq!(post.title, "Test Post");
        assert_eq!(post.slug, "test-post");

        // Verify media in DB
        let m = qm::get_media_by_id(db.conn(), "test-media-1").unwrap();
        assert_eq!(m.original_name, "photo.jpg");
    }

    #[test]
    fn rebuild_all_entity_types() {
        let (db, dir) = setup();

        crate::engine::meta::write_project_json(
            dir.path(),
            &ProjectMetadata {
                name: "Rebuilt Project".into(),
                description: None,
                public_url: None,
                main_language: Some("en".into()),
                default_author: None,
                max_posts_per_page: 50,
                image_import_concurrency: 4,
                blogmark_category: None,
                pico_theme: None,
                semantic_similarity_enabled: false,
                blog_languages: vec!["en".into()],
            },
        )
        .unwrap();

        qs::insert_script(
            db.conn(),
            &Script {
                id: "stale-script".into(),
                project_id: "p1".into(),
                slug: "stale-script".into(),
                title: "Stale Script".into(),
                kind: ScriptKind::Utility,
                entrypoint: "main".into(),
                enabled: true,
                version: 1,
                file_path: "scripts/stale.lua".into(),
                status: ScriptStatus::Published,
                content: None,
                created_at: 1,
                updated_at: 1,
            },
        )
        .unwrap();
        qtpl::insert_template(
            db.conn(),
            &Template {
                id: "stale-template".into(),
                project_id: "p1".into(),
                slug: "stale-template".into(),
                title: "Stale Template".into(),
                kind: TemplateKind::Post,
                enabled: true,
                version: 1,
                file_path: "templates/stale.liquid".into(),
                status: TemplateStatus::Published,
                content: None,
                created_at: 1,
                updated_at: 1,
            },
        )
        .unwrap();
        qp::insert_post(
            db.conn(),
            &qp::make_test_post("stale-post", "p1", "stale-post"),
        )
        .unwrap();
        qm::insert_media(db.conn(), &qm::make_test_media("stale-media", "p1")).unwrap();
        qtag::insert_tag(
            db.conn(),
            &Tag {
                id: "stale-tag".into(),
                project_id: "p1".into(),
                name: "stale-tag".into(),
                color: None,
                post_template_slug: None,
                created_at: 1,
                updated_at: 1,
            },
        )
        .unwrap();
        db.conn()
            .with(|connection| {
                use crate::db::schema::import_definitions;
                diesel::insert_into(import_definitions::table)
                    .values((
                        import_definitions::id.eq("stale-import"),
                        import_definitions::project_id.eq("p1"),
                        import_definitions::name.eq("Stale Import"),
                        import_definitions::created_at.eq(1_i64),
                        import_definitions::updated_at.eq(1_i64),
                    ))
                    .execute(connection)?;
                Ok(())
            })
            .unwrap();

        // Post
        let posts_dir = dir.path().join("posts").join("2024").join("01");
        fs::create_dir_all(&posts_dir).unwrap();
        let post_content = "\
---
id: test-post-1
title: Test Post
slug: test-post
status: published
createdAt: '2024-01-15T12:00:00.000Z'
updatedAt: '2024-01-15T12:00:00.000Z'
tags:
  - test
categories: []
publishedAt: '2024-01-15T12:00:00.000Z'
---
Hello from rebuild test!
";
        fs::write(posts_dir.join("test-post.md"), post_content).unwrap();

        // Media
        let media_dir = dir.path().join("media").join("2024").join("01");
        fs::create_dir_all(&media_dir).unwrap();
        fs::write(media_dir.join("test-media-1.jpg"), b"fake-jpeg").unwrap();
        let sidecar_content = "\
---
id: test-media-1
originalName: \"photo.jpg\"
mimeType: image/jpeg
size: 12345
width: 800
height: 600
createdAt: 2024-01-15T12:00:00.000Z
updatedAt: 2024-01-15T12:00:00.000Z
tags: []
linkedPostIds: [\"test-post-1\"]
---
";
        fs::write(media_dir.join("test-media-1.jpg.meta"), sidecar_content).unwrap();

        // Template
        let tpl_dir = dir.path().join("templates");
        fs::create_dir_all(&tpl_dir).unwrap();
        let tpl_content = "\
---
id: \"test-tpl-1\"
slug: \"test-template\"
title: \"Test Template\"
kind: \"post\"
enabled: true
version: 1
createdAt: \"2024-01-15T12:00:00.000Z\"
updatedAt: \"2024-01-15T12:00:00.000Z\"
---
<html>{{ content }}</html>
";
        fs::write(tpl_dir.join("test-template.liquid"), tpl_content).unwrap();

        // Script
        let scripts_dir = dir.path().join("scripts");
        fs::create_dir_all(&scripts_dir).unwrap();
        let script_content = "\
---
id: \"test-script-1\"
slug: \"test-script\"
title: \"Test Script\"
kind: \"macro\"
entrypoint: \"render\"
enabled: true
version: 1
createdAt: \"2024-01-15T12:00:00.000Z\"
updatedAt: \"2024-01-15T12:00:00.000Z\"
---
function render() end
";
        fs::write(scripts_dir.join("test-script.lua"), script_content).unwrap();
        fs::write(scripts_dir.join("legacy.py"), "print('ignore me')").unwrap();

        let report = rebuild_from_filesystem(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(report.posts_created, 1);
        assert_eq!(report.media_created, 1);
        assert_eq!(report.templates_created, 1);
        assert_eq!(report.scripts_created, 1);
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);

        // Verify all entities in DB
        let post = qp::get_post_by_id(db.conn(), "test-post-1").unwrap();
        assert_eq!(post.title, "Test Post");

        let m = qm::get_media_by_id(db.conn(), "test-media-1").unwrap();
        assert_eq!(m.original_name, "photo.jpg");

        let tpl = qtpl::get_template_by_id(db.conn(), "test-tpl-1").unwrap();
        assert_eq!(tpl.title, "Test Template");

        let script = qs::get_script_by_id(db.conn(), "test-script-1").unwrap();
        assert_eq!(script.title, "Test Script");
        assert!(qs::get_script_by_id(db.conn(), "stale-script").is_err());
        assert!(qtpl::get_template_by_id(db.conn(), "stale-template").is_err());
        assert!(qp::get_post_by_id(db.conn(), "stale-post").is_err());
        assert!(qm::get_media_by_id(db.conn(), "stale-media").is_err());
        assert!(qtag::get_tag_by_id(db.conn(), "stale-tag").is_err());
        let import_count = db
            .conn()
            .with(|connection| {
                use crate::db::schema::import_definitions::dsl::*;
                import_definitions
                    .filter(project_id.eq("p1"))
                    .count()
                    .get_result::<i64>(connection)
            })
            .unwrap();
        assert_eq!(import_count, 0);
        assert_eq!(
            qs::list_scripts_by_project(db.conn(), "p1").unwrap().len(),
            1
        );

        let project = crate::db::queries::project::get_project_by_id(db.conn(), "p1").unwrap();
        assert_eq!(project.name, "Rebuilt Project");
        assert!(project.description.is_none());

        let links =
            crate::db::queries::post_media::list_post_media_by_media(db.conn(), "test-media-1")
                .unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].post_id, "test-post-1");
    }

    #[test]
    fn rebuild_idempotent() {
        let (db, dir) = setup();

        // Write one of each entity type
        let posts_dir = dir.path().join("posts").join("2024").join("01");
        fs::create_dir_all(&posts_dir).unwrap();
        let post_content = "\
---
id: idem-post
title: Idempotent Post
slug: idem-post
status: draft
createdAt: '2024-01-15T12:00:00.000Z'
updatedAt: '2024-01-15T12:00:00.000Z'
tags: []
categories: []
---
Body text.
";
        fs::write(posts_dir.join("idem-post.md"), post_content).unwrap();

        let tpl_dir = dir.path().join("templates");
        fs::create_dir_all(&tpl_dir).unwrap();
        let tpl_content = "\
---
id: \"idem-tpl\"
slug: \"idem-tpl\"
title: \"Idempotent Template\"
kind: \"post\"
enabled: true
version: 1
createdAt: \"2024-01-15T12:00:00.000Z\"
updatedAt: \"2024-01-15T12:00:00.000Z\"
---
<html>{{ content }}</html>
";
        fs::write(tpl_dir.join("idem-tpl.liquid"), tpl_content).unwrap();

        // First rebuild
        let r1 = rebuild_from_filesystem(db.conn(), dir.path(), "p1").unwrap();
        assert_eq!(r1.posts_created, 1);
        assert_eq!(r1.posts_updated, 0);
        assert_eq!(r1.templates_created, 1);
        assert_eq!(r1.templates_updated, 0);
        assert!(r1.errors.is_empty(), "errors: {:?}", r1.errors);

        // A full rebuild clears the project rows before importing them again.
        let r2 = rebuild_from_filesystem(db.conn(), dir.path(), "p1").unwrap();
        assert_eq!(r2.posts_created, 1);
        assert_eq!(r2.posts_updated, 0);
        assert_eq!(r2.templates_created, 1);
        assert_eq!(r2.templates_updated, 0);
        assert!(r2.errors.is_empty(), "errors: {:?}", r2.errors);
    }

    #[test]
    fn failed_rebuild_rolls_back_truncation() {
        let (db, dir) = setup();
        qp::insert_post(db.conn(), &qp::make_test_post("keep-me", "p1", "keep-me")).unwrap();
        let posts_dir = dir.path().join("posts");
        fs::create_dir_all(&posts_dir).unwrap();
        fs::write(posts_dir.join("broken.md"), "not frontmatter").unwrap();

        assert!(rebuild_from_filesystem(db.conn(), dir.path(), "p1").is_err());
        assert!(qp::get_post_by_id(db.conn(), "keep-me").is_ok());
    }
}
