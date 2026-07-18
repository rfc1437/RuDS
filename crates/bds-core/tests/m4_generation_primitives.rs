use std::fs;

use bds_core::db::Database;
use bds_core::db::queries::generated_file_hash::get_generated_file_hash;
use bds_core::db::queries::project::insert_project;
use bds_core::model::{Post, PostStatus, Project};
use bds_core::render::{
    GeneratedWriteOutcome, build_calendar_json, build_core_generation_paths, write_generated_file,
};
use tempfile::TempDir;

fn make_project() -> Project {
    Project {
        id: "p1".into(),
        name: "Blog".into(),
        slug: "blog".into(),
        description: None,
        data_path: None,
        is_active: false,
        created_at: 1,
        updated_at: 1,
    }
}

fn make_post(slug: &str, published_at: i64) -> Post {
    Post {
        id: format!("post-{slug}"),
        project_id: "p1".into(),
        title: slug.into(),
        slug: slug.into(),
        excerpt: None,
        content: Some("Body".into()),
        status: PostStatus::Published,
        author: None,
        language: Some("en".into()),
        do_not_translate: false,
        template_slug: None,
        file_path: String::new(),
        checksum: None,
        tags: vec![],
        categories: vec![],
        published_title: None,
        published_content: None,
        published_tags: None,
        published_categories: None,
        published_excerpt: None,
        created_at: published_at,
        updated_at: published_at,
        published_at: Some(published_at),
    }
}

fn setup() -> (Database, TempDir) {
    let db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    insert_project(db.conn(), &make_project()).unwrap();
    (db, TempDir::new().unwrap())
}

#[test]
fn generated_write_skips_unchanged_content() {
    let (db, dir) = setup();

    let first = write_generated_file(db.conn(), dir.path(), "p1", "index.html", "hello").unwrap();
    let second = write_generated_file(db.conn(), dir.path(), "p1", "index.html", "hello").unwrap();
    let third = write_generated_file(db.conn(), dir.path(), "p1", "index.html", "changed").unwrap();

    assert_eq!(first, GeneratedWriteOutcome::Written);
    assert_eq!(second, GeneratedWriteOutcome::SkippedUnchanged);
    assert_eq!(third, GeneratedWriteOutcome::Written);

    let stored = get_generated_file_hash(db.conn(), "p1", "index.html").unwrap();
    assert_eq!(stored.relative_path, "index.html");
    assert!(
        fs::read_to_string(dir.path().join("index.html"))
            .unwrap()
            .contains("changed")
    );
}

#[test]
fn generated_write_rewrites_missing_file_even_when_hash_matches() {
    let (db, dir) = setup();

    let first = write_generated_file(db.conn(), dir.path(), "p1", "index.html", "hello").unwrap();
    fs::remove_file(dir.path().join("index.html")).unwrap();
    let second = write_generated_file(db.conn(), dir.path(), "p1", "index.html", "hello").unwrap();

    assert_eq!(first, GeneratedWriteOutcome::Written);
    assert_eq!(second, GeneratedWriteOutcome::Written);
    assert_eq!(
        fs::read_to_string(dir.path().join("index.html")).unwrap(),
        "hello"
    );
}

#[test]
fn generated_write_rewrites_stale_file_even_when_db_hash_matches() {
    let (db, dir) = setup();

    let first = write_generated_file(db.conn(), dir.path(), "p1", "index.html", "hello").unwrap();
    fs::write(dir.path().join("index.html"), "tampered").unwrap();
    let second = write_generated_file(db.conn(), dir.path(), "p1", "index.html", "hello").unwrap();

    assert_eq!(first, GeneratedWriteOutcome::Written);
    assert_eq!(second, GeneratedWriteOutcome::Written);
    assert_eq!(
        fs::read_to_string(dir.path().join("index.html")).unwrap(),
        "hello"
    );
}

#[test]
fn core_generation_paths_include_language_prefixed_variants() {
    let paths = build_core_generation_paths("en", &["en".into(), "de".into(), "fr".into()]);
    assert!(paths.contains(&"index.html".to_string()));
    assert!(paths.contains(&"sitemap.xml".to_string()));
    assert!(paths.contains(&"feed.xml".to_string()));
    assert!(paths.contains(&"atom.xml".to_string()));
    assert!(paths.contains(&"calendar.json".to_string()));
    assert!(paths.contains(&"de/index.html".to_string()));
    assert!(paths.contains(&"de/feed.xml".to_string()));
    assert!(paths.contains(&"de/atom.xml".to_string()));
    assert!(paths.contains(&"fr/index.html".to_string()));
}

#[test]
fn calendar_json_groups_posts_by_year_month_day() {
    let posts = vec![
        make_post("a", 1_710_000_000_000),
        make_post("b", 1_710_000_000_000),
        make_post("c", 1_712_678_400_000),
    ];

    let json = build_calendar_json(&posts).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed["years"]["2024"], 3);
    assert_eq!(parsed["months"]["2024-03"], 2);
    assert_eq!(parsed["months"]["2024-04"], 1);
    assert_eq!(parsed["days"]["2024-03-09"], 2);
    assert_eq!(parsed["days"]["2024-04-09"], 1);
}
