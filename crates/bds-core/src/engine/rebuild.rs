use std::path::Path;

use rusqlite::Connection;

use crate::db::fts;
use crate::engine::media;
use crate::engine::post;
use crate::engine::script_rebuild;
use crate::engine::template_rebuild;
use crate::engine::EngineResult;

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

/// Orchestrate a full rebuild from filesystem into the database.
///
/// Ensures FTS tables exist, then rebuilds posts, media, templates, and scripts
/// from their respective filesystem directories. Returns an aggregated report.
pub fn rebuild_from_filesystem(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<FullRebuildReport> {
    let mut report = FullRebuildReport::default();

    // 1. Ensure FTS tables exist
    fts::ensure_fts_tables(conn)?;

    // 2. Rebuild posts
    let post_report = post::rebuild_posts_from_filesystem(conn, data_dir, project_id)?;
    report.posts_created = post_report.posts_created;
    report.posts_updated = post_report.posts_updated;
    report.translations_created = post_report.translations_created;
    report.translations_updated = post_report.translations_updated;
    report.errors.extend(post_report.errors);

    // 3. Rebuild media
    let media_report = media::rebuild_media_from_filesystem(conn, data_dir, project_id)?;
    report.media_created = media_report.media_created;
    report.media_updated = media_report.media_updated;
    report.media_translations_created = media_report.translations_created;
    report.media_translations_updated = media_report.translations_updated;
    report.errors.extend(media_report.errors);

    // 4. Rebuild templates
    let tpl_report =
        template_rebuild::rebuild_templates_from_filesystem(conn, data_dir, project_id)?;
    report.templates_created = tpl_report.created;
    report.templates_updated = tpl_report.updated;
    report.errors.extend(tpl_report.errors);

    // 5. Rebuild scripts
    let script_report =
        script_rebuild::rebuild_scripts_from_filesystem(conn, data_dir, project_id)?;
    report.scripts_created = script_report.created;
    report.scripts_updated = script_report.updated;
    report.errors.extend(script_report.errors);

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries::media as qm;
    use crate::db::queries::post as qp;
    use crate::db::queries::project::{insert_project, make_test_project};
    use crate::db::queries::script as qs;
    use crate::db::queries::template as qtpl;
    use crate::db::Database;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (Database, TempDir) {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        fts::ensure_fts_tables(db.conn()).unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = TempDir::new().unwrap();
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
---
";
        fs::write(
            media_dir.join("test-media-1.jpg.meta"),
            sidecar_content,
        )
        .unwrap();

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

        // Second rebuild - should update, not create
        let r2 = rebuild_from_filesystem(db.conn(), dir.path(), "p1").unwrap();
        assert_eq!(r2.posts_created, 0);
        assert_eq!(r2.posts_updated, 1);
        assert_eq!(r2.templates_created, 0);
        assert_eq!(r2.templates_updated, 1);
        assert!(r2.errors.is_empty(), "errors: {:?}", r2.errors);
    }
}
