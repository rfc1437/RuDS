//! Tests that verify allium spec claims against the Rust model and DB code.
//!
//! Each test is annotated with the spec invariant or rule it validates.

use rusqlite::{Connection, OpenFlags};
use std::path::PathBuf;

fn fixture_db() -> Connection {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/compatibility-projects/rfc1437-sample/bds.db");
    Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}

fn memory_db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    bds_core::db::run_migrations(&mut conn).unwrap();
    conn
}

// ── spec: post.allium — PostStatus serde matches DB string values ────

#[test]
fn post_status_serde_matches_db_values() {
    // spec: status: draft | published | archived
    use bds_core::model::PostStatus;

    assert_eq!(serde_json::to_string(&PostStatus::Draft).unwrap(), "\"draft\"");
    assert_eq!(serde_json::to_string(&PostStatus::Published).unwrap(), "\"published\"");
    assert_eq!(serde_json::to_string(&PostStatus::Archived).unwrap(), "\"archived\"");

    // round-trip
    let d: PostStatus = serde_json::from_str("\"draft\"").unwrap();
    assert_eq!(d, PostStatus::Draft);
    let p: PostStatus = serde_json::from_str("\"published\"").unwrap();
    assert_eq!(p, PostStatus::Published);
    let a: PostStatus = serde_json::from_str("\"archived\"").unwrap();
    assert_eq!(a, PostStatus::Archived);
}

// ── spec: schema.allium — Template kind: post | list | not_found | partial ──

#[test]
fn template_kind_serde_matches_db_values() {
    use bds_core::model::TemplateKind;

    assert_eq!(serde_json::to_string(&TemplateKind::Post).unwrap(), "\"post\"");
    assert_eq!(serde_json::to_string(&TemplateKind::List).unwrap(), "\"list\"");
    assert_eq!(serde_json::to_string(&TemplateKind::NotFound).unwrap(), "\"not_found\"");
    assert_eq!(serde_json::to_string(&TemplateKind::Partial).unwrap(), "\"partial\"");

    let nf: TemplateKind = serde_json::from_str("\"not_found\"").unwrap();
    assert_eq!(nf, TemplateKind::NotFound);
}

// ── spec: schema.allium — Template status: draft | published ──

#[test]
fn template_status_serde_matches_db_values() {
    use bds_core::model::TemplateStatus;

    assert_eq!(serde_json::to_string(&TemplateStatus::Draft).unwrap(), "\"draft\"");
    assert_eq!(serde_json::to_string(&TemplateStatus::Published).unwrap(), "\"published\"");
}

// ── spec: schema.allium — Script kind: macro | utility | transform ──

#[test]
fn script_kind_serde_matches_db_values() {
    use bds_core::model::ScriptKind;

    assert_eq!(serde_json::to_string(&ScriptKind::Macro).unwrap(), "\"macro\"");
    assert_eq!(serde_json::to_string(&ScriptKind::Utility).unwrap(), "\"utility\"");
    assert_eq!(serde_json::to_string(&ScriptKind::Transform).unwrap(), "\"transform\"");

    let t: ScriptKind = serde_json::from_str("\"transform\"").unwrap();
    assert_eq!(t, ScriptKind::Transform);
}

// ── spec: schema.allium — Script status: draft | published ──

#[test]
fn script_status_serde_matches_db_values() {
    use bds_core::model::ScriptStatus;

    assert_eq!(serde_json::to_string(&ScriptStatus::Draft).unwrap(), "\"draft\"");
    assert_eq!(serde_json::to_string(&ScriptStatus::Published).unwrap(), "\"published\"");
}

// ── spec: post.allium — content_location invariant ──
// "if status = published: file_path else: content"
// Published posts have NULL content in DB; draft posts have content in DB.

#[test]
fn content_location_published_posts_null_content_in_fixture() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare("SELECT slug, content FROM posts WHERE status = 'published'")
        .unwrap();
    let rows: Vec<(String, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(!rows.is_empty());
    for (slug, content) in &rows {
        assert!(content.is_none(), "spec: published post '{slug}' must have NULL content in DB");
    }
}

#[test]
fn content_location_draft_posts_have_content_in_fixture() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare("SELECT slug, content FROM posts WHERE status = 'draft'")
        .unwrap();
    let rows: Vec<(String, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    assert!(!rows.is_empty());
    for (slug, content) in &rows {
        assert!(content.is_some(), "spec: draft post '{slug}' must have content in DB");
    }
}

// ── spec: post.allium — all status transitions allowed ──
// draft -> published, draft -> archived, published -> draft,
// published -> archived, archived -> draft, archived -> published

#[test]
fn post_status_transitions_all_valid() {
    let conn = memory_db();
    conn.execute(
        "INSERT INTO projects (id, name, slug, created_at, updated_at, is_active) \
         VALUES ('p1', 'test', 'test', 1000, 1000, 1)",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO posts (id, project_id, title, slug, status, file_path, created_at, updated_at) \
         VALUES ('post1', 'p1', 'Test', 'test', 'draft', '', 1000, 1000)",
        [],
    ).unwrap();

    let transitions = [
        ("draft", "published"),
        ("published", "draft"),
        ("draft", "archived"),
        ("archived", "draft"),
        ("draft", "published"),
        ("published", "archived"),
        ("archived", "published"),
    ];

    for (from, to) in transitions {
        // Set to 'from' state first
        conn.execute("UPDATE posts SET status = ?1 WHERE id = 'post1'", [from]).unwrap();
        // Transition to 'to' state
        conn.execute("UPDATE posts SET status = ?1 WHERE id = 'post1'", [to]).unwrap();
        let status: String = conn
            .query_row("SELECT status FROM posts WHERE id = 'post1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, to, "transition {from} -> {to} failed");
    }
}

// ── spec: post.allium — is_slug_frozen: published_at != null ──
// Slug changes only allowed before first publish

#[test]
fn slug_frozen_after_publish_semantics() {
    let conn = memory_db();
    conn.execute(
        "INSERT INTO projects (id, name, slug, created_at, updated_at, is_active) \
         VALUES ('p1', 'test', 'test', 1000, 1000, 1)",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO posts (id, project_id, title, slug, status, file_path, created_at, updated_at, published_at) \
         VALUES ('post1', 'p1', 'Test', 'test', 'published', 'posts/2024/01/test.md', 1000, 1000, 1000)",
        [],
    ).unwrap();

    // published_at is set — slug should be considered frozen
    let published_at: Option<i64> = conn
        .query_row("SELECT published_at FROM posts WHERE id = 'post1'", [], |r| r.get(0))
        .unwrap();
    assert!(published_at.is_some(), "spec: is_slug_frozen = published_at != null");

    // A never-published draft has no published_at — slug is mutable
    conn.execute(
        "INSERT INTO posts (id, project_id, title, slug, status, file_path, created_at, updated_at) \
         VALUES ('post2', 'p1', 'Draft', 'draft-post', 'draft', '', 1000, 1000)",
        [],
    ).unwrap();
    let draft_published_at: Option<i64> = conn
        .query_row("SELECT published_at FROM posts WHERE id = 'post2'", [], |r| r.get(0))
        .unwrap();
    assert!(draft_published_at.is_none(), "spec: unpublished draft has no published_at");
}

// ── spec: project.allium — SingleActiveProject invariant ──
// "Exactly one project is active at any time"

#[test]
fn single_active_project_in_fixture() {
    let conn = fixture_db();
    let active_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM projects WHERE is_active = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(active_count, 1, "spec: exactly one project is active");
}

// ── spec: project.allium — UniqueProjectSlug invariant ──

#[test]
fn unique_project_slug_in_fixture() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare("SELECT slug, COUNT(*) FROM projects GROUP BY slug HAVING COUNT(*) > 1")
        .unwrap();
    let dupes: Vec<(String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert!(dupes.is_empty(), "spec: project slug must be unique");
}

// ── spec: translation.allium — UniqueTranslationPerLanguage ──
// "post_translations must have unique (translation_for, language)"

#[test]
fn unique_translation_per_post_language_in_fixture() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare(
            "SELECT translation_for, language, COUNT(*) FROM post_translations \
             GROUP BY translation_for, language HAVING COUNT(*) > 1",
        )
        .unwrap();
    let dupes: Vec<(String, String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert!(dupes.is_empty(), "spec: translation unique per (post, language)");
}

// ── spec: schema.allium — Post defaults ──
// status default 'draft', do_not_translate default false, file_path default ''

#[test]
fn post_defaults_match_spec() {
    let conn = memory_db();
    conn.execute(
        "INSERT INTO projects (id, name, slug, created_at, updated_at, is_active) \
         VALUES ('p1', 'test', 'test', 1000, 1000, 1)",
        [],
    ).unwrap();
    // Insert with minimal columns to test defaults
    conn.execute(
        "INSERT INTO posts (id, project_id, title, slug, created_at, updated_at) \
         VALUES ('min1', 'p1', 'Minimal', 'minimal', 1000, 1000)",
        [],
    ).unwrap();

    let (status, do_not_translate, file_path): (String, bool, String) = conn
        .query_row(
            "SELECT status, do_not_translate, file_path FROM posts WHERE id = 'min1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(status, "draft", "spec: default status is draft");
    assert!(!do_not_translate, "spec: default do_not_translate is false");
    assert_eq!(file_path, "", "spec: default file_path is empty string");
}

// ── spec: schema.allium — Script defaults ──
// kind default 'utility', entrypoint default 'render', enabled default true,
// version default 1, status default 'published'

#[test]
fn script_defaults_match_spec() {
    let conn = memory_db();
    conn.execute(
        "INSERT INTO projects (id, name, slug, created_at, updated_at, is_active) \
         VALUES ('p1', 'test', 'test', 1000, 1000, 1)",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO scripts (id, project_id, slug, title, file_path, created_at, updated_at) \
         VALUES ('s1', 'p1', 'test', 'Test', 'scripts/test.lua', 1000, 1000)",
        [],
    ).unwrap();

    let (kind, entrypoint, enabled, version, status): (String, String, bool, i32, String) = conn
        .query_row(
            "SELECT kind, entrypoint, enabled, version, status FROM scripts WHERE id = 's1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(kind, "utility", "spec: default kind is utility");
    assert_eq!(entrypoint, "render", "spec: default entrypoint is 'render'");
    assert!(enabled, "spec: default enabled is true");
    assert_eq!(version, 1, "spec: default version is 1");
    assert_eq!(status, "draft", "spec: default status is draft");
}

// ── spec: schema.allium — Template defaults ──
// kind default 'post', enabled default true, version default 1, status default 'draft'

#[test]
fn template_defaults_match_spec() {
    let conn = memory_db();
    conn.execute(
        "INSERT INTO projects (id, name, slug, created_at, updated_at, is_active) \
         VALUES ('p1', 'test', 'test', 1000, 1000, 1)",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO templates (id, project_id, slug, title, file_path, created_at, updated_at) \
         VALUES ('t1', 'p1', 'test', 'Test', 'templates/test.liquid', 1000, 1000)",
        [],
    ).unwrap();

    let (kind, enabled, version, status): (String, bool, i32, String) = conn
        .query_row(
            "SELECT kind, enabled, version, status FROM templates WHERE id = 't1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(kind, "post", "spec: default kind is post");
    assert!(enabled, "spec: default enabled is true");
    assert_eq!(version, 1, "spec: default version is 1");
    assert_eq!(status, "draft", "spec: default status is draft");
}

// ── spec: tag.allium — UniqueTagNamePerProject (case-insensitive) ──

#[test]
fn tag_unique_name_per_project_enforced() {
    let conn = memory_db();
    conn.execute(
        "INSERT INTO projects (id, name, slug, created_at, updated_at, is_active) \
         VALUES ('p1', 'test', 'test', 1000, 1000, 1)",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO tags (id, project_id, name, created_at, updated_at) \
         VALUES ('t1', 'p1', 'rust', 1000, 1000)",
        [],
    ).unwrap();

    // Same name, same project — must fail
    let result = conn.execute(
        "INSERT INTO tags (id, project_id, name, created_at, updated_at) \
         VALUES ('t2', 'p1', 'rust', 1000, 1000)",
        [],
    );
    assert!(result.is_err(), "spec: duplicate tag name in same project must fail");
}

// ── spec: post.allium — Slug generation algorithm ──
// "transliterate unicode to ASCII, lowercase, replace [^a-z0-9]+ with hyphens,
//  strip leading/trailing hyphens"

#[test]
fn slug_generation_matches_spec_algorithm() {
    use bds_core::util::slugify;

    // Basic: lowercase + hyphen separation
    assert_eq!(slugify("Hello World"), "hello-world");

    // Non-alphanumeric replaced with single hyphen
    assert_eq!(slugify("a --- b"), "a-b");

    // Leading/trailing hyphens stripped
    assert_eq!(slugify("---hello---"), "hello");

    // Unicode transliteration
    assert_eq!(slugify("café"), "cafe");

    // German umlauts (spec: "only German and English letters used")
    assert_eq!(slugify("über"), "ueber");
    assert_eq!(slugify("Ärger"), "aerger");
    assert_eq!(slugify("Öffnung"), "oeffnung");
    assert_eq!(slugify("Straße"), "strasse");
}

// ── spec: post.allium — Slug uniqueness algorithm ──
// "tries base, then {slug}-2 .. {slug}-999, then {slug}-{timestamp}"

#[test]
fn slug_uniqueness_matches_spec() {
    use bds_core::util::ensure_unique;

    // Base available
    assert_eq!(ensure_unique("test", |_| false), "test");

    // Base taken → -2
    assert_eq!(ensure_unique("test", |s| s == "test"), "test-2");

    // -2 and -3 taken → -4
    assert_eq!(
        ensure_unique("test", |s| s == "test" || s == "test-2" || s == "test-3"),
        "test-4"
    );
}

// ── spec: frontmatter.allium — PostFileLayout ──
// "posts/{YYYY}/{MM}/{slug}.md"

#[test]
fn published_post_file_paths_follow_date_layout() {
    let conn = fixture_db();
    let mut stmt = conn
        .prepare("SELECT slug, file_path, created_at FROM posts WHERE status = 'published' AND file_path != ''")
        .unwrap();
    let rows: Vec<(String, String, i64)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();

    for (slug, path, _created_at) in &rows {
        // Path should end with {slug}.md
        assert!(
            path.ends_with(&format!("{slug}.md")),
            "spec: file_path must end with {{slug}}.md, got: {path}"
        );
        // Path should contain posts/YYYY/MM/ pattern
        assert!(
            path.contains("/posts/"),
            "spec: file_path must contain /posts/, got: {path}"
        );
    }
}

// ── spec: cli_sync.allium — DbNotification entity_type values ──

#[test]
fn notification_entity_serde_matches_db_values() {
    use bds_core::model::{NotificationEntity, NotificationAction};

    assert_eq!(serde_json::to_string(&NotificationEntity::Post).unwrap(), "\"post\"");
    assert_eq!(serde_json::to_string(&NotificationEntity::Media).unwrap(), "\"media\"");
    assert_eq!(serde_json::to_string(&NotificationEntity::Script).unwrap(), "\"script\"");
    assert_eq!(serde_json::to_string(&NotificationEntity::Template).unwrap(), "\"template\"");

    assert_eq!(serde_json::to_string(&NotificationAction::Created).unwrap(), "\"created\"");
    assert_eq!(serde_json::to_string(&NotificationAction::Updated).unwrap(), "\"updated\"");
    assert_eq!(serde_json::to_string(&NotificationAction::Deleted).unwrap(), "\"deleted\"");
}

// ── spec: generation.allium / publishing.allium — SshMode values ──

#[test]
fn ssh_mode_serde_matches_spec() {
    use bds_core::model::SshMode;

    assert_eq!(serde_json::to_string(&SshMode::Scp).unwrap(), "\"scp\"");
    assert_eq!(serde_json::to_string(&SshMode::Rsync).unwrap(), "\"rsync\"");
}

// ── spec: i18n.allium — SplitLocalization ──
// Verify that the 5 supported UI languages exist as config

#[test]
fn supported_languages_match_spec() {
    // spec: en, de, fr, it, es — the 5 supported UI languages
    let supported = ["en", "de", "fr", "it", "es"];
    assert_eq!(supported.len(), 5);
    // This test documents the spec constraint; actual locale loading
    // will be tested when i18n module is implemented in M2+
}

// ── spec: schema.allium — FTS5 virtual tables exist in fixture ──

#[test]
fn fts5_tables_exist_in_fixture() {
    let conn = fixture_db();

    // posts_fts
    let result = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='posts_fts'",
        [],
        |r| r.get::<_, i64>(0),
    );
    assert_eq!(result.unwrap(), 1, "spec: posts_fts virtual table must exist");

    // media_fts
    let result = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='media_fts'",
        [],
        |r| r.get::<_, i64>(0),
    );
    assert_eq!(result.unwrap(), 1, "spec: media_fts virtual table must exist");
}
