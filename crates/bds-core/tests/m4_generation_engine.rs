use std::collections::HashMap;

use bds_core::db::queries::project::insert_project;
use bds_core::db::queries::template::insert_template;
use bds_core::db::Database;
use bds_core::engine::generation::{
    PublishedPostSource, apply_validation_sections, generate_starter_site,
    sections_from_validation_report,
};
use bds_core::engine::meta::write_category_meta_json;
use bds_core::engine::validate_site::validate_site;
use bds_core::model::{CategorySettings, Post, PostStatus, Project, ProjectMetadata, Template, TemplateKind, TemplateStatus};
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

fn make_list_template(slug: &str, content: &str) -> Template {
    Template {
        id: format!("template-{slug}"),
        project_id: "p1".into(),
        slug: slug.into(),
        title: slug.into(),
        kind: TemplateKind::List,
        enabled: true,
        version: 1,
        file_path: String::new(),
        status: TemplateStatus::Published,
        content: Some(content.into()),
        created_at: 1,
        updated_at: 1,
    }
}

fn setup() -> (Database, TempDir) {
    let mut db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    insert_project(db.conn(), &make_project()).unwrap();
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("meta")).unwrap();
    bds_core::engine::meta::write_project_json(dir.path(), &make_metadata()).unwrap();
    (db, dir)
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
    assert!(report.written_paths.contains(&"assets/pico.min.css".to_string()));
    assert!(report.written_paths.contains(&"assets/tag-cloud.js".to_string()));
    assert!(report.written_paths.contains(&"2024/03/09/hello/index.html".to_string()));
    assert!(report.written_paths.contains(&"2024/03/10/next/index.html".to_string()));

    assert!(dir.path().join("index.html").exists());
    assert!(dir.path().join("rss.xml").exists());
    assert!(dir.path().join("feed.xml").exists());
    assert!(dir.path().join("atom.xml").exists());
    assert!(dir.path().join("sitemap.xml").exists());
    assert!(dir.path().join("assets/pico.min.css").exists());
    assert!(dir.path().join("assets/tag-cloud.js").exists());
    assert!(dir.path().join("2024/03/09/hello/index.html").exists());

    let rss = std::fs::read_to_string(dir.path().join("rss.xml")).unwrap();
    assert!(rss.contains("<rss version=\"2.0\""));
    assert!(rss.contains("https://example.com/2024/03/09/hello"));

    let sitemap = std::fs::read_to_string(dir.path().join("sitemap.xml")).unwrap();
    assert!(sitemap.contains("https://example.com/2024/03/09/hello"));
    assert!(sitemap.contains("https://example.com/category/article"));
}

#[test]
fn multilingual_generation_writes_language_aware_atom_and_sitemap_routes() {
    let (db, dir) = setup();
    let mut metadata = make_metadata();
    metadata.main_language = Some("de".into());
    metadata.blog_languages = vec!["de".into(), "en".into()];
    let posts = vec![PublishedPostSource {
        post: make_post("hallo", 1_710_000_000_000),
        body_markdown: "Hallo Welt".into(),
    }];

    let report = generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "de").unwrap();

    assert!(report.written_paths.contains(&"en/atom.xml".to_string()));
    assert!(report.written_paths.contains(&"en/sitemap.xml".to_string()));

    let atom = std::fs::read_to_string(dir.path().join("en/atom.xml")).unwrap();
    assert!(atom.contains("<link href=\"https://example.com/en/\" rel=\"alternate\" />") || atom.contains("<link href=\"https://example.com/en\" rel=\"alternate\" />"));
    assert!(atom.contains("<link href=\"https://example.com/en/atom.xml\" rel=\"self\" />"));

    let sitemap = std::fs::read_to_string(dir.path().join("sitemap.xml")).unwrap();
    assert!(sitemap.contains("hreflang=\"de\" href=\"https://example.com/\""));
    assert!(sitemap.contains("hreflang=\"en\" href=\"https://example.com/en\""));
    assert!(sitemap.contains("hreflang=\"x-default\" href=\"https://example.com/\""));
    assert!(sitemap.contains("https://example.com/category/article"));
}

#[test]
fn generation_respects_category_list_settings_and_writes_bundled_images() {
    let (db, dir) = setup();
    insert_template(
        db.conn(),
        &make_list_template(
            "featured-list",
            "FEATURED:{% for day_block in day_blocks %}{% for post in day_block.posts %}[{{ post.title }}|{{ post.show_title }}]{% endfor %}{% endfor %}",
        ),
    )
    .unwrap();
    write_category_meta_json(
        dir.path(),
        &HashMap::from([
            (
                "hidden".to_string(),
                CategorySettings {
                    render_in_lists: false,
                    show_title: true,
                    post_template_slug: None,
                    list_template_slug: None,
                },
            ),
            (
                "featured".to_string(),
                CategorySettings {
                    render_in_lists: true,
                    show_title: false,
                    post_template_slug: None,
                    list_template_slug: Some("featured-list".to_string()),
                },
            ),
        ]),
    )
    .unwrap();

    let mut hidden_post = make_post("hidden-post", 1_710_000_000_000);
    hidden_post.title = "Hidden Post".into();
    hidden_post.categories = vec!["hidden".into()];

    let mut featured_post = make_post("featured-post", 1_710_086_400_000);
    featured_post.title = "Featured Post".into();
    featured_post.categories = vec!["featured".into()];

    let posts = vec![
        PublishedPostSource {
            post: hidden_post,
            body_markdown: "Hidden body".into(),
        },
        PublishedPostSource {
            post: featured_post,
            body_markdown: "Featured body".into(),
        },
    ];

    let report = generate_starter_site(db.conn(), dir.path(), "p1", &make_metadata(), &posts, "en").unwrap();

    for asset in [
        "images/close.png",
        "images/loading.gif",
        "images/next.png",
        "images/prev.png",
    ] {
        assert!(report.written_paths.contains(&asset.to_string()));
        assert!(dir.path().join(asset).exists(), "missing bundled image {asset}");
    }

    assert!(!report.written_paths.contains(&"category/hidden/index.html".to_string()));
    assert!(report.written_paths.contains(&"category/featured/index.html".to_string()));

    let index_html = std::fs::read_to_string(dir.path().join("index.html")).unwrap();
    assert!(!index_html.contains("Hidden Post"));
    assert!(index_html.contains("[Featured Post|false]"));

    let featured_html = std::fs::read_to_string(dir.path().join("category/featured/index.html")).unwrap();
    assert!(featured_html.contains("FEATURED:[Featured Post|false]"));

    let feed = std::fs::read_to_string(dir.path().join("feed.xml")).unwrap();
    assert!(!feed.contains("hidden-post"));
    assert!(feed.contains("featured-post"));

    let calendar = std::fs::read_to_string(dir.path().join("calendar.json")).unwrap();
    assert!(!calendar.contains("2024-03-09"));
    assert!(calendar.contains("2024-03-10"));
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
    assert!(second.skipped_paths.contains(&"assets/pico.min.css".to_string()));
}

#[test]
fn site_validation_detects_stale_and_missing_outputs() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let post = make_post("hello", 1_710_000_000_000);
    bds_core::db::queries::post::insert_post(db.conn(), &post).unwrap();
    let posts = vec![PublishedPostSource {
        post,
        body_markdown: "Hello **world**".into(),
    }];

    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    std::fs::write(dir.path().join("index.html"), "tampered").unwrap();
    std::fs::remove_file(dir.path().join("feed.xml")).unwrap();

    let report = validate_site(db.conn(), dir.path(), "p1").unwrap();

    assert!(report.stale_pages.contains(&"index.html".to_string()));
    assert!(report.missing_pages.contains(&"feed.xml".to_string()));
    assert!(report.extra_pages.is_empty());
}

#[test]
fn apply_validation_repairs_core_section_outputs() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let post = make_post("hello", 1_710_000_000_000);
    bds_core::db::queries::post::insert_post(db.conn(), &post).unwrap();
    let posts = vec![PublishedPostSource {
        post,
        body_markdown: "Hello **world**".into(),
    }];

    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    std::fs::write(dir.path().join("index.html"), "tampered").unwrap();
    std::fs::remove_file(dir.path().join("feed.xml")).unwrap();

    let report = validate_site(db.conn(), dir.path(), "p1").unwrap();
    let sections = sections_from_validation_report(&report);
    let apply_report = apply_validation_sections(db.conn(), dir.path(), "p1", &metadata, &posts, &sections).unwrap();
    let repaired = validate_site(db.conn(), dir.path(), "p1").unwrap();

    assert!(!apply_report.written_paths.is_empty() || !apply_report.skipped_paths.is_empty());
    assert!(repaired.missing_pages.is_empty());
    assert!(repaired.extra_pages.is_empty());
    assert!(repaired.stale_pages.is_empty());
}

#[test]
fn apply_validation_removes_extra_section_outputs() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let post = make_post("hello", 1_710_000_000_000);
    bds_core::db::queries::post::insert_post(db.conn(), &post).unwrap();
    let posts = vec![PublishedPostSource {
        post,
        body_markdown: "Hello **world**".into(),
    }];

    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    let extra_dir = dir.path().join("tag/ghost");
    std::fs::create_dir_all(&extra_dir).unwrap();
    std::fs::write(extra_dir.join("index.html"), "ghost").unwrap();

    let report = validate_site(db.conn(), dir.path(), "p1").unwrap();
    assert!(report.extra_pages.contains(&"tag/ghost/index.html".to_string()));
    let sections = sections_from_validation_report(&report);
    let apply_report = apply_validation_sections(db.conn(), dir.path(), "p1", &metadata, &posts, &sections).unwrap();
    let repaired = validate_site(db.conn(), dir.path(), "p1").unwrap();

    assert!(apply_report.deleted_paths.contains(&"tag/ghost/index.html".to_string()));
    assert!(repaired.extra_pages.is_empty());
}

#[test]
fn site_validation_uses_html_output_directory_when_present() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let post = make_post("hello", 1_710_000_000_000);
    bds_core::db::queries::post::insert_post(db.conn(), &post).unwrap();
    let posts = vec![PublishedPostSource {
        post,
        body_markdown: "Hello **world**".into(),
    }];

    let output_dir = dir.path().join("html");
    std::fs::create_dir_all(&output_dir).unwrap();
    generate_starter_site(db.conn(), &output_dir, "p1", &metadata, &posts, "en").unwrap();

    let report = validate_site(db.conn(), dir.path(), "p1").unwrap();

    assert!(report.missing_pages.is_empty());
    assert!(report.extra_pages.is_empty());
    assert!(report.stale_pages.is_empty());
}