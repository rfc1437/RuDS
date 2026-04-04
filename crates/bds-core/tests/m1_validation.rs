//! M1 validation integration tests (Steps 19-21).
//!
//! - Step 19: Round-trip tests (create -> publish -> read -> verify)
//! - Step 20: Golden-file comparisons against fixture files
//! - Step 21: Metadata diff coverage matrix (every diffable field covered)

use bds_core::db::fts::ensure_fts_tables;
use bds_core::db::queries::project::insert_project;
use bds_core::db::queries::script as qs;
use bds_core::db::queries::template as qtpl;
use bds_core::db::Database;
use bds_core::engine::media;
use bds_core::engine::meta;
use bds_core::engine::metadata_diff;
use bds_core::engine::post;
use bds_core::engine::tag;
use bds_core::model::{
    PostStatus, Project, Script, ScriptKind, ScriptStatus, Template, TemplateKind,
    TemplateStatus,
};
use bds_core::util::frontmatter::{
    read_post_file, read_template_file, read_translation_file,
    write_script_file, write_template_file, ScriptFrontmatter, TemplateFrontmatter,
};
use bds_core::util::sidecar::{read_sidecar, read_translation_sidecar};
use image::DynamicImage;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// ── Common helpers ──────────────────────────────────────────────────

fn make_test_project(id: &str, slug: &str) -> Project {
    Project {
        id: id.to_string(),
        name: format!("Project {id}"),
        slug: slug.to_string(),
        description: Some("A test project".into()),
        data_path: Some("/data".into()),
        is_active: false,
        created_at: 1000,
        updated_at: 2000,
    }
}

fn setup() -> (Database, TempDir) {
    let db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    ensure_fts_tables(db.conn()).unwrap();
    insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
    let dir = TempDir::new().unwrap();
    // Seed meta directory and tags.json for tag operations
    fs::create_dir_all(dir.path().join("meta")).unwrap();
    fs::write(dir.path().join("meta/tags.json"), "[]").unwrap();
    (db, dir)
}

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/compatibility-projects/rfc1437-sample")
}

fn create_test_image(dir: &Path) -> PathBuf {
    let path = dir.join("test_image.png");
    let img = DynamicImage::new_rgb8(10, 10);
    img.save(&path).unwrap();
    path
}

// ════════════════════════════════════════════════════════════════════
// Step 19: Round-trip Integration Tests
// ════════════════════════════════════════════════════════════════════

#[test]
fn test_post_create_publish_read_roundtrip() {
    let (db, dir) = setup();

    // Create a post with full metadata
    let created = post::create_post(
        db.conn(),
        dir.path(),
        "p1",
        "Round-Trip Post",
        Some("This is the body of the post."),
        vec!["rust".into(), "testing".into()],
        vec!["tech".into()],
        Some("Alice"),
        Some("en"),
        None,
    )
    .unwrap();

    assert_eq!(created.status, PostStatus::Draft);

    // Publish the post
    let published = post::publish_post(db.conn(), dir.path(), &created.id).unwrap();
    assert_eq!(published.status, PostStatus::Published);
    assert!(!published.file_path.is_empty());

    // Read the file from disk
    let abs_path = dir.path().join(&published.file_path);
    assert!(abs_path.exists(), "published file must exist on disk");
    let file_content = fs::read_to_string(&abs_path).unwrap();

    // Parse via read_post_file
    let (fm, body) = read_post_file(&file_content).unwrap();

    // Verify all frontmatter fields match the DB values
    assert_eq!(fm.id, published.id);
    assert_eq!(fm.title, "Round-Trip Post");
    assert_eq!(fm.slug, published.slug);
    assert_eq!(fm.status, "published");
    assert_eq!(fm.tags, vec!["rust", "testing"]);
    assert_eq!(fm.categories, vec!["tech"]);
    assert_eq!(fm.author.as_deref(), Some("Alice"));
    assert_eq!(fm.language.as_deref(), Some("en"));
    assert!(fm.published_at.is_some());
    assert_eq!(fm.published_at, published.published_at);
    assert_eq!(fm.created_at, published.created_at);

    // Verify body content matches
    assert_eq!(body, "This is the body of the post.");
}

#[test]
fn test_post_translation_roundtrip() {
    let (db, dir) = setup();

    // Create post + publish
    let created = post::create_post(
        db.conn(),
        dir.path(),
        "p1",
        "Translated Post",
        Some("English body."),
        vec![],
        vec![],
        None,
        None,
        None,
    )
    .unwrap();

    // Create translation before publishing
    let _translation = post::upsert_translation(
        db.conn(),
        dir.path(),
        &created.id,
        "de",
        "Uebersetzter Beitrag",
        Some("Zusammenfassung"),
        Some("Deutscher Inhalt."),
    )
    .unwrap();

    // Publish (also publishes translations)
    let published = post::publish_post(db.conn(), dir.path(), &created.id).unwrap();

    // Find the translation file on disk
    let trans_rel_path = format!(
        "posts/{}/{}/{}.de.md",
        &published.file_path[6..10],  // YYYY
        &published.file_path[11..13], // MM
        published.slug
    );
    let trans_abs = dir.path().join(&trans_rel_path);
    assert!(trans_abs.exists(), "translation file must exist on disk");

    // Parse the translation file
    let content = fs::read_to_string(&trans_abs).unwrap();
    let (fm, body) = read_translation_file(&content).unwrap();

    // Verify fields
    assert_eq!(fm.translation_for, created.id);
    assert_eq!(fm.language, "de");
    assert_eq!(fm.title, "Uebersetzter Beitrag");
    assert_eq!(fm.excerpt.as_deref(), Some("Zusammenfassung"));
    assert_eq!(body, "Deutscher Inhalt.");
}

#[test]
fn test_media_import_sidecar_roundtrip() {
    let (db, dir) = setup();

    // Create a test PNG
    let source_path = create_test_image(dir.path());

    // Import media
    let imported = media::import_media(
        db.conn(),
        dir.path(),
        "p1",
        &source_path,
        "test_photo.png",
        Some("My Photo Title"),
        Some("A test photo alt text"),
        Some("Caption text"),
        Some("Bob"),
        Some("de"),
        vec!["nature".into(), "macro".into()],
    )
    .unwrap();

    // Read sidecar from disk
    let abs_sidecar = dir.path().join(&imported.sidecar_path);
    assert!(abs_sidecar.exists(), "sidecar file must exist");
    let sidecar_content = fs::read_to_string(&abs_sidecar).unwrap();
    let sc = read_sidecar(&sidecar_content).unwrap();

    // Verify sidecar fields match DB
    assert_eq!(sc.id, imported.id);
    assert_eq!(sc.original_name, "test_photo.png");
    assert_eq!(sc.mime_type, "image/png");
    assert!(sc.size > 0);
    assert_eq!(sc.width, Some(10));
    assert_eq!(sc.height, Some(10));
    assert_eq!(sc.title.as_deref(), Some("My Photo Title"));
    assert_eq!(sc.alt.as_deref(), Some("A test photo alt text"));
    assert_eq!(sc.caption.as_deref(), Some("Caption text"));
    assert_eq!(sc.author.as_deref(), Some("Bob"));
    assert_eq!(sc.language.as_deref(), Some("de"));
    assert_eq!(sc.tags, vec!["nature", "macro"]);
    assert_eq!(sc.created_at, imported.created_at);
    assert_eq!(sc.updated_at, imported.updated_at);
}

#[test]
fn test_media_translation_roundtrip() {
    let (db, dir) = setup();

    let source_path = create_test_image(dir.path());
    let imported = media::import_media(
        db.conn(),
        dir.path(),
        "p1",
        &source_path,
        "photo.png",
        Some("Title"),
        Some("Alt"),
        None,
        None,
        None,
        vec![],
    )
    .unwrap();

    // Create translation
    let _t = media::upsert_media_translation(
        db.conn(),
        dir.path(),
        &imported.id,
        "fr",
        Some("Titre francais"),
        Some("Texte alternatif"),
        Some("Legende"),
    )
    .unwrap();

    // Read the translation sidecar from disk
    let trans_sidecar_rel =
        bds_core::util::media_translation_sidecar_path(&imported.file_path, "fr");
    let abs_trans_sidecar = dir.path().join(&trans_sidecar_rel);
    assert!(abs_trans_sidecar.exists(), "translation sidecar must exist");

    let content = fs::read_to_string(&abs_trans_sidecar).unwrap();
    let sc = read_translation_sidecar(&content).unwrap();

    assert_eq!(sc.translation_for, imported.id);
    assert_eq!(sc.language, "fr");
    assert_eq!(sc.title.as_deref(), Some("Titre francais"));
    assert_eq!(sc.alt.as_deref(), Some("Texte alternatif"));
    assert_eq!(sc.caption.as_deref(), Some("Legende"));
}

#[test]
fn test_tag_sync_roundtrip() {
    let (db, dir) = setup();

    // Create tags via the engine
    tag::create_tag(db.conn(), dir.path(), "p1", "rust", Some("#ff0000")).unwrap();
    tag::create_tag(db.conn(), dir.path(), "p1", "go", None).unwrap();
    tag::create_tag(db.conn(), dir.path(), "p1", "python", Some("#3776ab")).unwrap();

    // Read tags.json from disk
    let entries = meta::read_tags_json(dir.path()).unwrap();

    // Tags should be sorted case-insensitively
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].name, "go");
    assert_eq!(entries[1].name, "python");
    assert_eq!(entries[2].name, "rust");

    // Verify colors
    assert!(entries[0].color.is_none());
    assert_eq!(entries[1].color.as_deref(), Some("#3776ab"));
    assert_eq!(entries[2].color.as_deref(), Some("#ff0000"));
}

// ════════════════════════════════════════════════════════════════════
// Step 20: Golden-file Comparisons
// ════════════════════════════════════════════════════════════════════

#[test]
fn test_golden_post_esmeralda() {
    let path = fixture_dir().join("posts/2005/11/esmeralda.md");
    let expected = fs::read_to_string(&path).unwrap();
    let (fm, body) = read_post_file(&expected).unwrap();

    // Verify parsed fixture data
    assert_eq!(fm.id, "40a83ab1-423d-4310-aac4-642d84675007");
    assert_eq!(fm.title, "Esmeralda");
    assert_eq!(fm.slug, "esmeralda");
    assert_eq!(fm.status, "published");
    assert_eq!(fm.language.as_deref(), Some("es"));
    assert_eq!(fm.created_at, 1131883200000);
    assert_eq!(fm.published_at, Some(1131883200000));
    assert_eq!(
        fm.tags,
        vec!["fotografie", "makro", "natur", "spinne", "tiere"]
    );
    assert_eq!(fm.categories, vec!["picture"]);

    // Write back via write_post_file, then reconstruct using from_post-style
    let yaml = fm.to_yaml();
    let actual = bds_core::util::frontmatter::format_frontmatter(&yaml, &body);

    // Compare byte-for-byte with fixture
    assert_eq!(
        actual, expected,
        "golden output mismatch for esmeralda.md"
    );
}

#[test]
fn test_golden_translation_esmeralda_en() {
    let path = fixture_dir().join("posts/2005/11/esmeralda.en.md");
    let expected = fs::read_to_string(&path).unwrap();
    let (fm, body) = read_translation_file(&expected).unwrap();

    // Verify parsed data
    assert_eq!(fm.translation_for, "40a83ab1-423d-4310-aac4-642d84675007");
    assert_eq!(fm.language, "en");
    assert_eq!(fm.title, "Esmeralda");

    // Write back and compare byte-for-byte
    let yaml = fm.to_yaml();
    let actual = bds_core::util::frontmatter::format_frontmatter(&yaml, &body);
    assert_eq!(
        actual, expected,
        "golden output mismatch for esmeralda.en.md"
    );
}

#[test]
fn test_golden_sidecar() {
    let path =
        fixture_dir().join("media/2005/11/eb0cf9d7-6fbd-4b74-9be3-759d6e16f240.jpg.meta");
    let expected = fs::read_to_string(&path).unwrap();
    let sc = read_sidecar(&expected).unwrap();

    // Verify parsed data
    assert_eq!(sc.id, "eb0cf9d7-6fbd-4b74-9be3-759d6e16f240");
    assert_eq!(sc.original_name, "CRW_1121.jpg");
    assert_eq!(sc.mime_type, "image/jpeg");
    assert_eq!(sc.size, 706358);
    assert_eq!(sc.width, Some(1800));
    assert_eq!(sc.height, Some(1200));
    assert_eq!(sc.title.as_deref(), Some("Esmeralda"));

    // Write back via to_string() and compare
    let actual = sc.to_string();
    assert_eq!(
        actual.trim(),
        expected.trim(),
        "golden output mismatch for media sidecar"
    );
}

#[test]
fn test_golden_meta_files() {
    let fd = fixture_dir();

    // project.json: parse and verify key fields
    let project_json = fs::read_to_string(fd.join("meta/project.json")).unwrap();
    let project_meta: bds_core::model::ProjectMetadata =
        serde_json::from_str(&project_json).unwrap();
    assert_eq!(project_meta.name, "rfc1437");
    assert_eq!(project_meta.main_language.as_deref(), Some("de"));
    assert_eq!(project_meta.default_author.as_deref(), Some("hugo"));
    assert_eq!(project_meta.max_posts_per_page, 50);
    assert_eq!(
        project_meta.public_url.as_deref(),
        Some("https://www.rfc1437.de")
    );
    assert!(project_meta.semantic_similarity_enabled);
    assert_eq!(project_meta.blog_languages, vec!["en"]);

    // categories.json: parse and verify sorted order
    let categories_json = fs::read_to_string(fd.join("meta/categories.json")).unwrap();
    let categories: Vec<String> = serde_json::from_str(&categories_json).unwrap();
    assert_eq!(
        categories,
        vec![
            "article", "aside", "kochbuch", "metaowl", "page", "picture", "spielelog", "wiki"
        ]
    );
    // Verify they are sorted
    let mut sorted = categories.clone();
    sorted.sort();
    assert_eq!(categories, sorted, "categories should be sorted");

    // tags.json: parse and verify structure
    let tags_json = fs::read_to_string(fd.join("meta/tags.json")).unwrap();
    let tags: Vec<bds_core::model::TagEntry> = serde_json::from_str(&tags_json).unwrap();
    assert!(!tags.is_empty());
    // Verify sorted by name
    let names: Vec<&str> = tags.iter().map(|t| t.name.as_str()).collect();
    let mut sorted_names = names.clone();
    sorted_names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    assert_eq!(names, sorted_names, "tags should be sorted by name");
    // Spot-check a known tag
    assert!(tags.iter().any(|t| t.name == "rust"));
    assert!(tags.iter().any(|t| t.name == "fotografie"));
}

#[test]
fn test_golden_template() {
    let path = fixture_dir().join("templates/testvorlage.liquid");
    let expected = fs::read_to_string(&path).unwrap();
    let (fm, body) = read_template_file(&expected).unwrap();

    // Verify parsed data
    assert_eq!(fm.id, "38704737-b7e7-4dd4-b010-9208bcf80ef6");
    assert_eq!(
        fm.project_id.as_deref(),
        Some("1979237c-034d-41f6-99a0-f35eb57b3f6c")
    );
    assert_eq!(fm.slug, "testvorlage");
    assert_eq!(fm.title, "Testvorlage");
    assert_eq!(fm.kind, "post");
    assert!(fm.enabled);
    assert_eq!(fm.version, 3);

    // Write back and compare byte-for-byte
    let actual = write_template_file(&fm, &body);
    assert_eq!(
        actual, expected,
        "golden output mismatch for testvorlage.liquid"
    );
}

// ════════════════════════════════════════════════════════════════════
// Step 21: Metadata Diff Coverage Matrix
// ════════════════════════════════════════════════════════════════════

// ── Post diff: 13 fields ────────────────────────────────────────────

/// Helper: create a post, publish it, then modify a specific field in the
/// file on disk. Run the diff and return all detected field names.
fn post_diff_for_field(
    modify_fn: impl FnOnce(&str) -> String,
) -> Vec<String> {
    let (db, dir) = setup();

    let created = post::create_post(
        db.conn(),
        dir.path(),
        "p1",
        "Diff Test Post",
        Some("body text"),
        vec!["tag1".into()],
        vec!["cat1".into()],
        Some("Author"),
        Some("en"),
        Some("tpl-slug"),
    )
    .unwrap();

    let published = post::publish_post(db.conn(), dir.path(), &created.id).unwrap();
    let abs_path = dir.path().join(&published.file_path);
    let content = fs::read_to_string(&abs_path).unwrap();

    // Apply modification to file
    let modified = modify_fn(&content);
    fs::write(&abs_path, modified).unwrap();

    let report = metadata_diff::compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();

    report
        .diffs
        .iter()
        .filter(|d| d.entity_type == "post")
        .flat_map(|d| d.fields.iter())
        .map(|f| f.field_name.clone())
        .collect()
}

#[test]
fn test_diff_detects_post_title() {
    let fields = post_diff_for_field(|content| {
        content.replace("title: Diff Test Post", "title: CHANGED Title")
    });
    assert!(
        fields.contains(&"title".to_string()),
        "expected 'title' in diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_post_slug() {
    let fields = post_diff_for_field(|content| {
        content.replace("slug: diff-test-post", "slug: changed-slug")
    });
    assert!(
        fields.contains(&"slug".to_string()),
        "expected 'slug' in diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_post_status() {
    let fields = post_diff_for_field(|content| {
        content.replace("status: published", "status: draft")
    });
    assert!(
        fields.contains(&"status".to_string()),
        "expected 'status' in diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_post_tags() {
    let fields = post_diff_for_field(|content| {
        content.replace("  - tag1", "  - changed-tag")
    });
    assert!(
        fields.contains(&"tags".to_string()),
        "expected 'tags' in diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_post_categories() {
    let fields = post_diff_for_field(|content| {
        content.replace("  - cat1", "  - changed-cat")
    });
    assert!(
        fields.contains(&"categories".to_string()),
        "expected 'categories' in diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_post_excerpt() {
    // The post was created without excerpt. Add one in the file.
    let fields = post_diff_for_field(|content| {
        content.replace(
            "templateSlug: tpl-slug",
            "templateSlug: tpl-slug\nexcerpt: Added excerpt",
        )
    });
    assert!(
        fields.contains(&"excerpt".to_string()),
        "expected 'excerpt' in diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_post_author() {
    let fields = post_diff_for_field(|content| {
        content.replace("author: Author", "author: Different Author")
    });
    assert!(
        fields.contains(&"author".to_string()),
        "expected 'author' in diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_post_language() {
    let fields = post_diff_for_field(|content| {
        content.replace("language: en", "language: de")
    });
    assert!(
        fields.contains(&"language".to_string()),
        "expected 'language' in diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_post_do_not_translate() {
    // Post was created with do_not_translate=false (omitted from file).
    // Add it to the file.
    let fields = post_diff_for_field(|content| {
        content.replace(
            "templateSlug: tpl-slug",
            "doNotTranslate: true\ntemplateSlug: tpl-slug",
        )
    });
    assert!(
        fields.contains(&"doNotTranslate".to_string()),
        "expected 'doNotTranslate' in diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_post_template_slug() {
    let fields = post_diff_for_field(|content| {
        content.replace("templateSlug: tpl-slug", "templateSlug: different-tpl")
    });
    assert!(
        fields.contains(&"templateSlug".to_string()),
        "expected 'templateSlug' in diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_post_created_at() {
    let fields = post_diff_for_field(|content| {
        // Replace the createdAt ISO timestamp with a different one
        // The fixture will have something like createdAt: '2026-...'
        // Replace with a very different date
        let mut result = content.to_string();
        if let Some(start) = result.find("createdAt: '") {
            let val_start = start + "createdAt: '".len();
            if let Some(end) = result[val_start..].find('\'') {
                let old_val = &result[val_start..val_start + end].to_string();
                result = result.replace(
                    &format!("createdAt: '{old_val}'"),
                    "createdAt: '2000-01-01T00:00:00.000Z'",
                );
            }
        }
        result
    });
    assert!(
        fields.contains(&"createdAt".to_string()),
        "expected 'createdAt' in diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_post_updated_at() {
    let fields = post_diff_for_field(|content| {
        let mut result = content.to_string();
        if let Some(start) = result.find("updatedAt: '") {
            let val_start = start + "updatedAt: '".len();
            if let Some(end) = result[val_start..].find('\'') {
                let old_val = &result[val_start..val_start + end].to_string();
                result = result.replace(
                    &format!("updatedAt: '{old_val}'"),
                    "updatedAt: '2000-01-01T00:00:00.000Z'",
                );
            }
        }
        result
    });
    assert!(
        fields.contains(&"updatedAt".to_string()),
        "expected 'updatedAt' in diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_post_published_at() {
    let fields = post_diff_for_field(|content| {
        let mut result = content.to_string();
        if let Some(start) = result.find("publishedAt: '") {
            let val_start = start + "publishedAt: '".len();
            if let Some(end) = result[val_start..].find('\'') {
                let old_val = &result[val_start..val_start + end].to_string();
                result = result.replace(
                    &format!("publishedAt: '{old_val}'"),
                    "publishedAt: '2000-01-01T00:00:00.000Z'",
                );
            }
        }
        result
    });
    assert!(
        fields.contains(&"publishedAt".to_string()),
        "expected 'publishedAt' in diff fields, got: {fields:?}"
    );
}

// ── Media diff: 6 fields ────────────────────────────────────────────

/// Helper: import media, then modify a field in the sidecar file.
/// Returns the list of diff field names detected.
fn media_diff_for_field(
    modify_fn: impl FnOnce(&str) -> String,
) -> Vec<String> {
    let (db, dir) = setup();

    let source_path = create_test_image(dir.path());
    let imported = media::import_media(
        db.conn(),
        dir.path(),
        "p1",
        &source_path,
        "photo.png",
        Some("Original Title"),
        Some("Original Alt"),
        Some("Original Caption"),
        Some("Original Author"),
        Some("en"),
        vec!["original-tag".into()],
    )
    .unwrap();

    let abs_sidecar = dir.path().join(&imported.sidecar_path);
    let content = fs::read_to_string(&abs_sidecar).unwrap();

    let modified = modify_fn(&content);
    fs::write(&abs_sidecar, modified).unwrap();

    let report = metadata_diff::compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();

    report
        .diffs
        .iter()
        .filter(|d| d.entity_type == "media")
        .flat_map(|d| d.fields.iter())
        .map(|f| f.field_name.clone())
        .collect()
}

#[test]
fn test_diff_detects_media_title() {
    let fields = media_diff_for_field(|content| {
        content.replace("title: \"Original Title\"", "title: \"Changed Title\"")
    });
    assert!(
        fields.contains(&"title".to_string()),
        "expected 'title' in media diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_media_alt() {
    let fields = media_diff_for_field(|content| {
        content.replace("alt: \"Original Alt\"", "alt: \"Changed Alt\"")
    });
    assert!(
        fields.contains(&"alt".to_string()),
        "expected 'alt' in media diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_media_caption() {
    let fields = media_diff_for_field(|content| {
        content.replace(
            "caption: \"Original Caption\"",
            "caption: \"Changed Caption\"",
        )
    });
    assert!(
        fields.contains(&"caption".to_string()),
        "expected 'caption' in media diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_media_author() {
    let fields = media_diff_for_field(|content| {
        content.replace(
            "author: \"Original Author\"",
            "author: \"Changed Author\"",
        )
    });
    assert!(
        fields.contains(&"author".to_string()),
        "expected 'author' in media diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_media_tags() {
    let fields = media_diff_for_field(|content| {
        content.replace(
            "tags: [\"original-tag\"]",
            "tags: [\"changed-tag\"]",
        )
    });
    assert!(
        fields.contains(&"tags".to_string()),
        "expected 'tags' in media diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_media_language() {
    let fields = media_diff_for_field(|content| {
        content.replace("language: en", "language: de")
    });
    assert!(
        fields.contains(&"language".to_string()),
        "expected 'language' in media diff fields, got: {fields:?}"
    );
}

// ── Template diff: 4 fields ─────────────────────────────────────────

/// Helper: insert a template in DB + write file, then modify a field in
/// the file. Returns the list of diff field names.
fn template_diff_for_field(
    modify_fn: impl FnOnce(&str) -> String,
) -> Vec<String> {
    let (db, dir) = setup();

    let tpl = Template {
        id: "tpl-diff-1".to_string(),
        project_id: "p1".to_string(),
        slug: "diff-template".to_string(),
        title: "Diff Template".to_string(),
        kind: TemplateKind::Post,
        enabled: true,
        version: 1,
        file_path: "templates/diff-template.liquid".to_string(),
        status: TemplateStatus::Published,
        content: None,
        created_at: 1000,
        updated_at: 2000,
    };
    qtpl::insert_template(db.conn(), &tpl).unwrap();

    // Write matching template file
    let fm = TemplateFrontmatter {
        id: "tpl-diff-1".to_string(),
        project_id: Some("p1".to_string()),
        slug: "diff-template".to_string(),
        title: "Diff Template".to_string(),
        kind: "post".to_string(),
        enabled: true,
        version: 1,
        created_at: 1000,
        updated_at: 2000,
    };
    let file_content = write_template_file(&fm, "<div>template body</div>");
    let tpl_dir = dir.path().join("templates");
    fs::create_dir_all(&tpl_dir).unwrap();
    fs::write(tpl_dir.join("diff-template.liquid"), &file_content).unwrap();

    // Apply modification
    let modified = modify_fn(&file_content);
    fs::write(tpl_dir.join("diff-template.liquid"), modified).unwrap();

    let report = metadata_diff::compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();

    report
        .diffs
        .iter()
        .filter(|d| d.entity_type == "template")
        .flat_map(|d| d.fields.iter())
        .map(|f| f.field_name.clone())
        .collect()
}

#[test]
fn test_diff_detects_template_title() {
    let fields = template_diff_for_field(|content| {
        content.replace("title: \"Diff Template\"", "title: \"Changed Template\"")
    });
    assert!(
        fields.contains(&"title".to_string()),
        "expected 'title' in template diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_template_kind() {
    let fields = template_diff_for_field(|content| {
        content.replace("kind: \"post\"", "kind: \"list\"")
    });
    assert!(
        fields.contains(&"kind".to_string()),
        "expected 'kind' in template diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_template_enabled() {
    let fields = template_diff_for_field(|content| {
        content.replace("enabled: true", "enabled: false")
    });
    assert!(
        fields.contains(&"enabled".to_string()),
        "expected 'enabled' in template diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_template_version() {
    let fields = template_diff_for_field(|content| {
        content.replace("version: 1", "version: 99")
    });
    assert!(
        fields.contains(&"version".to_string()),
        "expected 'version' in template diff fields, got: {fields:?}"
    );
}

// ── Script diff: 5 fields ───────────────────────────────────────────

/// Helper: insert a script in DB + write file, then modify a field in the
/// file. Returns the list of diff field names.
fn script_diff_for_field(
    modify_fn: impl FnOnce(&str) -> String,
) -> Vec<String> {
    let (db, dir) = setup();

    let script = Script {
        id: "scr-diff-1".to_string(),
        project_id: "p1".to_string(),
        slug: "diff-script".to_string(),
        title: "Diff Script".to_string(),
        kind: ScriptKind::Utility,
        entrypoint: "main".to_string(),
        enabled: true,
        version: 1,
        file_path: "scripts/diff-script.lua".to_string(),
        status: ScriptStatus::Published,
        content: None,
        created_at: 1000,
        updated_at: 2000,
    };
    qs::insert_script(db.conn(), &script).unwrap();

    // Write matching script file
    let fm = ScriptFrontmatter {
        id: "scr-diff-1".to_string(),
        project_id: Some("p1".to_string()),
        slug: "diff-script".to_string(),
        title: "Diff Script".to_string(),
        kind: "utility".to_string(),
        entrypoint: "main".to_string(),
        enabled: true,
        version: 1,
        created_at: 1000,
        updated_at: 2000,
    };
    let file_content = write_script_file(&fm, "-- lua script\nreturn 1");
    let scripts_dir = dir.path().join("scripts");
    fs::create_dir_all(&scripts_dir).unwrap();
    fs::write(scripts_dir.join("diff-script.lua"), &file_content).unwrap();

    // Apply modification
    let modified = modify_fn(&file_content);
    fs::write(scripts_dir.join("diff-script.lua"), modified).unwrap();

    let report = metadata_diff::compute_metadata_diff(db.conn(), dir.path(), "p1").unwrap();

    report
        .diffs
        .iter()
        .filter(|d| d.entity_type == "script")
        .flat_map(|d| d.fields.iter())
        .map(|f| f.field_name.clone())
        .collect()
}

#[test]
fn test_diff_detects_script_title() {
    let fields = script_diff_for_field(|content| {
        content.replace("title: \"Diff Script\"", "title: \"Changed Script\"")
    });
    assert!(
        fields.contains(&"title".to_string()),
        "expected 'title' in script diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_script_kind() {
    let fields = script_diff_for_field(|content| {
        content.replace("kind: \"utility\"", "kind: \"transform\"")
    });
    assert!(
        fields.contains(&"kind".to_string()),
        "expected 'kind' in script diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_script_entrypoint() {
    let fields = script_diff_for_field(|content| {
        content.replace("entrypoint: \"main\"", "entrypoint: \"run\"")
    });
    assert!(
        fields.contains(&"entrypoint".to_string()),
        "expected 'entrypoint' in script diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_script_enabled() {
    let fields = script_diff_for_field(|content| {
        content.replace("enabled: true", "enabled: false")
    });
    assert!(
        fields.contains(&"enabled".to_string()),
        "expected 'enabled' in script diff fields, got: {fields:?}"
    );
}

#[test]
fn test_diff_detects_script_version() {
    let fields = script_diff_for_field(|content| {
        content.replace("version: 1", "version: 42")
    });
    assert!(
        fields.contains(&"version".to_string()),
        "expected 'version' in script diff fields, got: {fields:?}"
    );
}
