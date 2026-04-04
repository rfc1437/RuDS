//! Integration tests that open the real fixture DB extracted from the TypeScript bDS app
//! and verify every table can be read correctly into Rust model structs.
//!
//! This validates the compatibility contract: the Rust app MUST read databases
//! created by the TypeScript app without modification.

use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

fn fixture_db() -> Connection {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/compatibility-projects/rfc1437-sample/bds.db");
    assert!(path.exists(), "fixture DB not found at {}", path.display());
    Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}

const PROJECT_ID: &str = "1979237c-034d-41f6-99a0-f35eb57b3f6c";

// ── Project ─────────────────────────────────────────────────────────

#[test]
fn read_project() {
    let conn = fixture_db();
    let (id, name, slug, data_path, is_active): (String, String, String, String, bool) = conn
        .query_row(
            "SELECT id, name, slug, data_path, is_active FROM projects WHERE id = ?1",
            [PROJECT_ID],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap();

    assert_eq!(id, PROJECT_ID);
    assert_eq!(name, "rfc1437");
    assert_eq!(slug, "rfc1437");
    assert!(data_path.contains("rfc1437.de"));
    assert!(is_active);
}

// ── Posts ────────────────────────────────────────────────────────────

#[test]
fn read_published_post_has_null_content() {
    let conn = fixture_db();
    let (title, slug, status, content): (String, String, String, Option<String>) = conn
        .query_row(
            "SELECT title, slug, status, content FROM posts WHERE slug = 'esmeralda'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();

    assert_eq!(title, "Esmeralda");
    assert_eq!(slug, "esmeralda");
    assert_eq!(status, "published");
    assert!(content.is_none(), "published posts must have NULL content in DB");
}

#[test]
fn read_draft_post_has_content() {
    let conn = fixture_db();
    let (title, slug, status, content): (String, String, String, Option<String>) = conn
        .query_row(
            "SELECT title, slug, status, content FROM posts WHERE slug = 'draft-fixture-post'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();

    assert_eq!(title, "Draft Fixture Post");
    assert_eq!(slug, "draft-fixture-post");
    assert_eq!(status, "draft");
    assert!(content.is_some(), "draft posts must have content in DB");
    assert!(content.unwrap().contains("**body**"));
}

#[test]
fn read_all_posts_count() {
    let conn = fixture_db();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM posts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 4); // 3 published + 1 draft
}

#[test]
fn published_posts_have_file_paths() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare("SELECT slug, file_path FROM posts WHERE status = 'published'")
        .unwrap();
    let rows: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(rows.len(), 3);
    for (slug, path) in &rows {
        assert!(!path.is_empty(), "published post '{slug}' must have a file_path");
        assert!(path.ends_with(&format!("{slug}.md")), "file_path must end with {slug}.md");
    }
}

#[test]
fn post_tags_are_json_arrays() {
    let conn = fixture_db();
    let tags_json: Option<String> = conn
        .query_row(
            "SELECT tags FROM posts WHERE slug = 'esmeralda'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    if let Some(json) = tags_json {
        let parsed: Vec<String> = serde_json::from_str(&json).unwrap();
        assert!(!parsed.is_empty());
    }
    // tags can also be NULL — both are valid
}

#[test]
fn post_timestamps_are_unix_integers() {
    let conn = fixture_db();
    let (created_at, updated_at): (i64, i64) = conn
        .query_row(
            "SELECT created_at, updated_at FROM posts WHERE slug = 'esmeralda'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    // Sanity: timestamps should be in reasonable Unix range (year 2000+)
    assert!(created_at > 946_684_800, "created_at should be after year 2000");
    assert!(updated_at > 946_684_800, "updated_at should be after year 2000");
}

#[test]
fn post_unique_constraint_on_project_slug() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare("SELECT project_id, slug, COUNT(*) FROM posts GROUP BY project_id, slug HAVING COUNT(*) > 1")
        .unwrap();
    let dupes: Vec<(String, String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(dupes.is_empty(), "found duplicate (project_id, slug) pairs: {dupes:?}");
}

// ── Post Translations ───────────────────────────────────────────────

#[test]
fn read_post_translations() {
    let conn = fixture_db();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM post_translations", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 4);
}

#[test]
fn translation_references_valid_post() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare(
            "SELECT pt.id, pt.translation_for FROM post_translations pt \
             LEFT JOIN posts p ON pt.translation_for = p.id \
             WHERE p.id IS NULL",
        )
        .unwrap();
    let orphans: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(orphans.is_empty(), "orphan translations referencing missing posts: {orphans:?}");
}

#[test]
fn published_translations_have_null_content() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare("SELECT id, content FROM post_translations WHERE status = 'published'")
        .unwrap();
    let rows: Vec<(String, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    for (id, content) in &rows {
        assert!(content.is_none(), "published translation {id} must have NULL content");
    }
}

// ── Post Links ──────────────────────────────────────────────────────

#[test]
fn read_post_links() {
    let conn = fixture_db();
    let (source, target, text): (String, String, Option<String>) = conn
        .query_row(
            "SELECT source_post_id, target_post_id, link_text FROM post_links LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    // ghostty links to cmux
    assert_eq!(source, "6745981d-da41-4cfd-80ec-95ad339acf6f");
    assert_eq!(target, "2665bfaa-8251-468d-a710-a4cf34dd81e2");
    assert!(text.is_some());
}

// ── Post Media ──────────────────────────────────────────────────────

#[test]
fn read_post_media() {
    let conn = fixture_db();
    let (post_id, media_id, sort_order): (String, String, i32) = conn
        .query_row(
            "SELECT post_id, media_id, sort_order FROM post_media LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    // esmeralda <-> spider photo
    assert_eq!(post_id, "40a83ab1-423d-4310-aac4-642d84675007");
    assert_eq!(media_id, "eb0cf9d7-6fbd-4b74-9be3-759d6e16f240");
    assert_eq!(sort_order, 0);
}

// ── Media ───────────────────────────────────────────────────────────

#[test]
fn read_media() {
    let conn = fixture_db();
    let (id, filename, original_name, mime_type, title, alt): (
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT id, filename, original_name, mime_type, title, alt FROM media LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();

    assert_eq!(id, "eb0cf9d7-6fbd-4b74-9be3-759d6e16f240");
    assert!(filename.ends_with(".jpg"));
    assert_eq!(original_name, "CRW_1121.jpg");
    assert_eq!(mime_type, "image/jpeg");
    assert!(title.is_some());
    assert!(alt.is_some());
}

// ── Tags ────────────────────────────────────────────────────────────

#[test]
fn read_tags() {
    let conn = fixture_db();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM tags", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 5);
}

#[test]
fn tag_names_are_expected() {
    let conn = fixture_db();
    let mut stmt = conn.prepare("SELECT name FROM tags ORDER BY name").unwrap();
    let names: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert_eq!(names, vec!["fotografie", "mac-os-x", "natur", "programmierung", "sysadmin"]);
}

#[test]
fn tag_unique_constraint_on_project_name() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare("SELECT project_id, name, COUNT(*) FROM tags GROUP BY project_id, name HAVING COUNT(*) > 1")
        .unwrap();
    let dupes: Vec<(String, String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(dupes.is_empty(), "found duplicate (project_id, name) tag pairs: {dupes:?}");
}

// ── Templates ───────────────────────────────────────────────────────

#[test]
fn read_template() {
    let conn = fixture_db();
    let (slug, title, kind, enabled, status, content): (
        String,
        String,
        String,
        bool,
        String,
        Option<String>,
    ) = conn
        .query_row(
            "SELECT slug, title, kind, enabled, status, content FROM templates LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();

    assert_eq!(slug, "testvorlage");
    assert_eq!(title, "Testvorlage");
    assert_eq!(kind, "post");
    assert!(enabled);
    assert_eq!(status, "published");
    assert!(content.is_none(), "published template content should be NULL in DB");
}

// ── Scripts ─────────────────────────────────────────────────────────

#[test]
fn read_scripts() {
    let conn = fixture_db();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM scripts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 2);
}

#[test]
fn read_script_fields() {
    let conn = fixture_db();
    let (slug, title, kind, entrypoint, enabled, status): (
        String,
        String,
        String,
        String,
        bool,
        String,
    ) = conn
        .query_row(
            "SELECT slug, title, kind, entrypoint, enabled, status FROM scripts WHERE slug = 'bgg_link'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();

    assert_eq!(slug, "bgg_link");
    assert_eq!(title, "bgg link");
    assert_eq!(kind, "transform");
    assert_eq!(entrypoint, "normalize_blogmark");
    assert!(enabled);
    assert_eq!(status, "published");
}

// ── Settings ────────────────────────────────────────────────────────

#[test]
fn read_settings() {
    let conn = fixture_db();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM settings", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 5);
}

#[test]
fn settings_are_key_value_pairs() {
    let conn = fixture_db();
    let mut stmt = conn.prepare("SELECT key, value FROM settings").unwrap();
    let pairs: Vec<(String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    for (key, value) in &pairs {
        assert!(!key.is_empty());
        assert!(!value.is_empty());
    }
    // All setting keys should contain the project ID (namespaced)
    let project_keys: Vec<_> = pairs.iter().filter(|(k, _)| k.contains(PROJECT_ID)).collect();
    assert_eq!(project_keys.len(), 5);
}

// ── AI Catalog ──────────────────────────────────────────────────────

#[test]
fn read_ai_tables() {
    let conn = fixture_db();

    let providers: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_providers", [], |row| row.get(0))
        .unwrap();
    let models: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_models", [], |row| row.get(0))
        .unwrap();
    let meta: i64 = conn
        .query_row("SELECT COUNT(*) FROM ai_catalog_meta", [], |row| row.get(0))
        .unwrap();

    assert_eq!(providers, 1);
    assert_eq!(models, 1);
    assert_eq!(meta, 2);
}

// ── Generated File Hashes ───────────────────────────────────────────

#[test]
fn read_generated_file_hashes_via_settings() {
    // The fixture stores generation hashes as settings keys
    let conn = fixture_db();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM settings WHERE key LIKE '%generation-hash%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(count > 0, "expected generation hash entries in settings");
}

// ── Cross-table referential integrity ───────────────────────────────

#[test]
fn all_posts_belong_to_existing_project() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare(
            "SELECT p.id FROM posts p \
             LEFT JOIN projects pr ON p.project_id = pr.id \
             WHERE pr.id IS NULL",
        )
        .unwrap();
    let orphans: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(orphans.is_empty(), "posts referencing missing projects: {orphans:?}");
}

#[test]
fn all_tags_belong_to_existing_project() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare(
            "SELECT t.id FROM tags t \
             LEFT JOIN projects pr ON t.project_id = pr.id \
             WHERE pr.id IS NULL",
        )
        .unwrap();
    let orphans: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(orphans.is_empty(), "tags referencing missing projects: {orphans:?}");
}

#[test]
fn all_media_belong_to_existing_project() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare(
            "SELECT m.id FROM media m \
             LEFT JOIN projects pr ON m.project_id = pr.id \
             WHERE pr.id IS NULL",
        )
        .unwrap();
    let orphans: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(orphans.is_empty(), "media referencing missing projects: {orphans:?}");
}

#[test]
fn post_links_reference_valid_posts() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare(
            "SELECT pl.id, pl.source_post_id, pl.target_post_id FROM post_links pl \
             LEFT JOIN posts s ON pl.source_post_id = s.id \
             LEFT JOIN posts t ON pl.target_post_id = t.id \
             WHERE s.id IS NULL OR t.id IS NULL",
        )
        .unwrap();
    let orphans: Vec<(String, String, String)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(orphans.is_empty(), "post_links with invalid references: {orphans:?}");
}

#[test]
fn post_media_references_valid_entities() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare(
            "SELECT pm.id FROM post_media pm \
             LEFT JOIN posts p ON pm.post_id = p.id \
             LEFT JOIN media m ON pm.media_id = m.id \
             WHERE p.id IS NULL OR m.id IS NULL",
        )
        .unwrap();
    let orphans: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(orphans.is_empty(), "post_media with invalid references: {orphans:?}");
}
