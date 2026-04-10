use bds_core::db::queries::project::insert_project;
use bds_core::db::Database;
use bds_core::engine::generation::{PublishedPostSource, generate_starter_site};
use bds_core::model::{Post, PostStatus, Project, ProjectMetadata};
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

fn make_metadata() -> ProjectMetadata {
    ProjectMetadata {
        name: "Blog".into(),
        description: Some("desc".into()),
        public_url: Some("https://example.com".into()),
        main_language: Some("en".into()),
        default_author: None,
        max_posts_per_page: 50,
        blogmark_category: None,
        pico_theme: None,
        semantic_similarity_enabled: false,
        blog_languages: vec!["en".into()],
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
        author: Some("alice".into()),
        language: Some("en".into()),
        do_not_translate: false,
        template_slug: None,
        file_path: String::new(),
        checksum: None,
        tags: vec!["rust".into()],
        categories: vec!["article".into()],
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
    let mut db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    insert_project(db.conn(), &make_project()).unwrap();
    (db, TempDir::new().unwrap())
}

#[test]
fn generation_engine_writes_core_and_single_post_artifacts() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let posts = vec![
        PublishedPostSource {
            post: make_post("hello", 1_710_000_000_000),
            body_markdown: "Hello **world**".into(),
        },
        PublishedPostSource {
            post: make_post("next", 1_710_086_400_000),
            body_markdown: "Next post".into(),
        },
    ];

    let report = generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();

    assert!(report.written_paths.contains(&"index.html".to_string()));
    assert!(report.written_paths.contains(&"calendar.json".to_string()));
    assert!(report.written_paths.contains(&"rss.xml".to_string()));
    assert!(report.written_paths.contains(&"feed.xml".to_string()));
    assert!(report.written_paths.contains(&"atom.xml".to_string()));
    assert!(report.written_paths.contains(&"sitemap.xml".to_string()));
    assert!(report.written_paths.contains(&"2024/03/09/hello/index.html".to_string()));
    assert!(report.written_paths.contains(&"2024/03/10/next/index.html".to_string()));

    assert!(dir.path().join("index.html").exists());
    assert!(dir.path().join("rss.xml").exists());
    assert!(dir.path().join("feed.xml").exists());
    assert!(dir.path().join("atom.xml").exists());
    assert!(dir.path().join("sitemap.xml").exists());
    assert!(dir.path().join("2024/03/09/hello/index.html").exists());

    let rss = std::fs::read_to_string(dir.path().join("rss.xml")).unwrap();
    assert!(rss.contains("<rss version=\"2.0\""));
    assert!(rss.contains("https://example.com/2024/03/09/hello"));

    let sitemap = std::fs::read_to_string(dir.path().join("sitemap.xml")).unwrap();
    assert!(sitemap.contains("https://example.com/2024/03/09/hello"));
}

#[test]
fn generation_engine_skips_unchanged_outputs_on_second_run() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let posts = vec![PublishedPostSource {
        post: make_post("hello", 1_710_000_000_000),
        body_markdown: "Hello **world**".into(),
    }];

    let first = generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    let second = generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();

    assert!(!first.written_paths.is_empty());
    assert!(second.skipped_paths.contains(&"index.html".to_string()));
    assert!(second.skipped_paths.contains(&"calendar.json".to_string()));
    assert!(second.skipped_paths.contains(&"rss.xml".to_string()));
    assert!(second.skipped_paths.contains(&"feed.xml".to_string()));
}