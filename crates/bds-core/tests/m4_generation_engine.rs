use std::collections::{BTreeMap, HashMap};

use bds_core::db::Database;
use bds_core::db::queries::project::insert_project;
use bds_core::db::queries::template::insert_template;
use bds_core::engine::generation::{
    GenerationSection, PublishedPostSource, apply_validation_section_with_progress,
    apply_validation_sections, build_site_search_index, generate_starter_site,
    generate_starter_site_forced, load_published_post_source, render_site_section_with_progress,
    sections_from_validation_report,
};
use bds_core::engine::meta::write_category_meta_json;
use bds_core::engine::validate_site::validate_site;
use bds_core::model::{
    CategorySettings, Post, PostStatus, PostTranslation, Project, ProjectMetadata, Template,
    TemplateKind, TemplateStatus,
};
use bds_core::render::estimate_site_render_pages;
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
        image_import_concurrency: 4,
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
    let db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    insert_project(db.conn(), &make_project()).unwrap();
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("meta")).unwrap();
    bds_core::engine::meta::write_project_json(dir.path(), &make_metadata()).unwrap();
    (db, dir)
}

fn write_published_snapshot(dir: &TempDir, post: &mut Post, body: &str) {
    post.file_path = format!("posts/{}.md", post.slug);
    let frontmatter = bds_core::util::frontmatter::PostFrontmatter::from_post(post).to_yaml();
    let file = bds_core::util::frontmatter::format_frontmatter(&frontmatter, body);
    std::fs::create_dir_all(dir.path().join("posts")).unwrap();
    std::fs::write(dir.path().join(&post.file_path), file).unwrap();
}

#[test]
fn reopened_draft_generation_uses_last_published_file() {
    let (_db, dir) = setup();
    let mut post = make_post("reopened", 1_710_000_000_000);
    post.status = PostStatus::Draft;
    post.content = Some("Unpublished draft body".into());
    post.file_path = "posts/2024/03/reopened.md".into();
    let frontmatter = bds_core::util::frontmatter::PostFrontmatter::from_post(&post).to_yaml();
    let file = bds_core::util::frontmatter::format_frontmatter(&frontmatter, "Published body");
    std::fs::create_dir_all(dir.path().join("posts/2024/03")).unwrap();
    std::fs::write(dir.path().join(&post.file_path), file).unwrap();

    let source = load_published_post_source(dir.path(), post)
        .unwrap()
        .unwrap();
    assert_eq!(source.body_markdown, "Published body");
}

#[test]
fn archived_post_with_published_file_is_not_a_generation_source() {
    let (_db, dir) = setup();
    let mut post = make_post("archived", 1_710_000_000_000);
    write_published_snapshot(&dir, &mut post, "Published body");
    post.status = PostStatus::Archived;
    post.content = None;

    let source = load_published_post_source(dir.path(), post).unwrap();

    assert!(source.is_none());
}

#[test]
fn validation_keeps_reopened_draft_published_snapshots() {
    let (db, dir) = setup();
    let mut post = make_post("reopened", 1_710_000_000_000);
    post.status = PostStatus::Draft;
    post.content = Some("Unpublished draft body".into());
    write_published_snapshot(&dir, &mut post, "Published body");
    bds_core::db::queries::post::insert_post(db.conn(), &post).unwrap();
    let source = load_published_post_source(dir.path(), post)
        .unwrap()
        .unwrap();

    generate_starter_site(
        db.conn(),
        dir.path(),
        "p1",
        &make_metadata(),
        &[source],
        "en",
    )
    .unwrap();
    let validation = validate_site(db.conn(), dir.path(), "p1").unwrap();

    assert!(validation.missing_pages.is_empty());
    assert!(validation.extra_pages.is_empty());
    assert!(validation.stale_pages.is_empty());
}

#[test]
fn generation_engine_writes_core_and_single_post_artifacts() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let hello = make_post("hello", 1_710_000_000_000);
    let next = make_post("next", 1_710_086_400_000);
    let posts = vec![
        PublishedPostSource {
            post: hello,
            body_markdown: "Hello **world**".into(),
        },
        PublishedPostSource {
            post: next,
            body_markdown: "Next post".into(),
        },
    ];

    let report =
        generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();

    assert!(report.written_paths.contains(&"index.html".to_string()));
    assert!(report.written_paths.contains(&"calendar.json".to_string()));
    assert!(report.written_paths.contains(&"rss.xml".to_string()));
    assert!(!report.written_paths.contains(&"feed.xml".to_string()));
    assert!(report.written_paths.contains(&"atom.xml".to_string()));
    assert!(report.written_paths.contains(&"sitemap.xml".to_string()));
    assert!(report.written_paths.contains(&"404.html".to_string()));
    assert!(
        report
            .written_paths
            .contains(&"2024/03/09/index.html".to_string())
    );
    assert!(
        report
            .written_paths
            .contains(&"assets/pico.min.css".to_string())
    );
    assert!(
        report
            .written_paths
            .contains(&"assets/tag-cloud.js".to_string())
    );
    assert!(
        report
            .written_paths
            .contains(&"2024/03/09/hello/index.html".to_string())
    );
    assert!(
        report
            .written_paths
            .contains(&"2024/03/10/next/index.html".to_string())
    );

    assert!(dir.path().join("index.html").exists());
    assert!(dir.path().join("rss.xml").exists());
    assert!(!dir.path().join("feed.xml").exists());
    assert!(dir.path().join("atom.xml").exists());
    assert!(dir.path().join("sitemap.xml").exists());
    assert!(dir.path().join("assets/pico.min.css").exists());
    assert!(dir.path().join("assets/tag-cloud.js").exists());
    assert!(dir.path().join("2024/03/09/hello/index.html").exists());

    let rss = std::fs::read_to_string(dir.path().join("rss.xml")).unwrap();
    assert_eq!(
        rss,
        "<rss><channel><title>Blog (en)</title><item><title>next</title><link>https://example.com/2024/03/10/next/</link></item><item><title>hello</title><link>https://example.com/2024/03/09/hello/</link></item></channel></rss>"
    );
    let atom = std::fs::read_to_string(dir.path().join("atom.xml")).unwrap();
    assert_eq!(
        atom,
        "<feed><title>Blog (en)</title><entry><title>next</title><id>https://example.com/2024/03/10/next/</id></entry><entry><title>hello</title><id>https://example.com/2024/03/09/hello/</id></entry></feed>"
    );

    let sitemap = std::fs::read_to_string(dir.path().join("sitemap.xml")).unwrap();
    assert!(sitemap.contains("https://example.com/2024/03/09/hello"));
    assert!(sitemap.contains("https://example.com/category/article"));
}

#[test]
fn mixed_source_languages_use_the_main_language_at_canonical_urls() {
    let (db, dir) = setup();
    let mut metadata = make_metadata();
    metadata.main_language = Some("de".into());
    metadata.blog_languages = vec!["de".into(), "en".into()];
    bds_core::engine::meta::write_project_json(dir.path(), &metadata).unwrap();
    write_category_meta_json(
        dir.path(),
        &HashMap::from([(
            "article".into(),
            CategorySettings {
                title: Some("Artikel".into()),
                titles: BTreeMap::from([("en".into(), "Articles".into())]),
                render_in_lists: true,
                show_title: true,
                post_template_slug: None,
                list_template_slug: None,
            },
        )]),
    )
    .unwrap();
    insert_template(
        db.conn(),
        &Template {
            id: "template-post".into(),
            project_id: "p1".into(),
            slug: "post".into(),
            title: "Post".into(),
            kind: TemplateKind::Post,
            enabled: true,
            version: 1,
            file_path: String::new(),
            status: TemplateStatus::Published,
            content: Some("ID={{ post.id }} {{ post.content }}".into()),
            created_at: 1,
            updated_at: 1,
        },
    )
    .unwrap();

    let mut english_source = make_post("english-source", 1_710_000_000_000);
    english_source.language = Some("en".into());
    write_published_snapshot(&dir, &mut english_source, "English source body");
    bds_core::db::queries::post::insert_post(db.conn(), &english_source).unwrap();
    let german_translation = PostTranslation {
        id: "translation-english-de".into(),
        project_id: "p1".into(),
        translation_for: english_source.id.clone(),
        language: "de".into(),
        title: "Deutsche Übersetzung".into(),
        excerpt: None,
        content: None,
        status: PostStatus::Published,
        file_path: "posts/english-source.de.md".into(),
        checksum: None,
        created_at: english_source.created_at,
        updated_at: english_source.updated_at,
        published_at: english_source.published_at,
    };
    bds_core::db::queries::post_translation::insert_post_translation(
        db.conn(),
        &german_translation,
    )
    .unwrap();
    std::fs::write(
        dir.path().join(&german_translation.file_path),
        bds_core::util::frontmatter::write_translation_file(
            &german_translation,
            "Deutscher Übersetzungstext",
        ),
    )
    .unwrap();

    let mut german_without_translation = make_post("german-fallback", 1_710_172_800_000);
    german_without_translation.language = Some("de".into());
    german_without_translation.title = "Deutscher Fallback".into();

    let mut german_source = make_post("german-source", 1_710_086_400_000);
    german_source.language = Some("de".into());
    german_source.title = "Deutsche Quelle".into();
    german_source.categories = vec!["aside".into()];
    write_published_snapshot(&dir, &mut german_source, "Deutscher Quelltext");
    bds_core::db::queries::post::insert_post(db.conn(), &german_source).unwrap();
    let english_translation = PostTranslation {
        id: "translation-german-en".into(),
        project_id: "p1".into(),
        translation_for: german_source.id.clone(),
        language: "en".into(),
        title: "English translation".into(),
        excerpt: None,
        content: None,
        status: PostStatus::Published,
        file_path: "posts/german-source.en.md".into(),
        checksum: None,
        created_at: german_source.created_at,
        updated_at: german_source.updated_at,
        published_at: german_source.published_at,
    };
    bds_core::db::queries::post_translation::insert_post_translation(
        db.conn(),
        &english_translation,
    )
    .unwrap();
    std::fs::write(
        dir.path().join(&english_translation.file_path),
        bds_core::util::frontmatter::write_translation_file(
            &english_translation,
            "English translation body",
        ),
    )
    .unwrap();

    let mut german_private = make_post("german-private", 1_710_259_200_000);
    german_private.language = Some("de".into());
    german_private.do_not_translate = true;

    let output = dir.path().join("html");
    let sources = vec![
        PublishedPostSource {
            post: english_source,
            body_markdown: "English source body".into(),
        },
        PublishedPostSource {
            post: german_source,
            body_markdown: "Deutscher Quelltext".into(),
        },
        PublishedPostSource {
            post: german_without_translation,
            body_markdown: "Unübersetzter Quelltext".into(),
        },
        PublishedPostSource {
            post: german_private,
            body_markdown: "Nur auf Deutsch".into(),
        },
    ];
    generate_starter_site(db.conn(), &output, "p1", &metadata, &sources, "de").unwrap();

    let canonical_english_source =
        std::fs::read_to_string(output.join("2024/03/09/english-source/index.html")).unwrap();
    let localized_english_source =
        std::fs::read_to_string(output.join("en/2024/03/09/english-source/index.html")).unwrap();
    let canonical_german_source =
        std::fs::read_to_string(output.join("2024/03/10/german-source/index.html")).unwrap();
    let localized_german_source =
        std::fs::read_to_string(output.join("en/2024/03/10/german-source/index.html")).unwrap();
    let localized_fallback =
        std::fs::read_to_string(output.join("en/2024/03/11/german-fallback/index.html")).unwrap();

    assert!(canonical_english_source.contains("Deutscher Übersetzungstext"));
    assert!(canonical_english_source.contains("ID=translation-english-de"));
    assert!(localized_english_source.contains("English source body"));
    assert!(localized_english_source.contains("ID=post-english-source"));
    assert!(canonical_german_source.contains("Deutscher Quelltext"));
    assert!(localized_german_source.contains("English translation body"));
    assert!(localized_fallback.contains("Unübersetzter Quelltext"));
    let english_index = std::fs::read_to_string(output.join("en/index.html")).unwrap();
    assert!(english_index.contains("English translation body"));
    assert!(!english_index.contains(">English translation</a></h2>"));
    let german_category =
        std::fs::read_to_string(output.join("category/article/index.html")).unwrap();
    let english_category =
        std::fs::read_to_string(output.join("en/category/article/index.html")).unwrap();
    assert!(german_category.contains("<h1 class=\"archive-heading\">Artikel</h1>"));
    assert!(english_category.contains("<h1 class=\"archive-heading\">Articles</h1>"));
    assert!(
        output
            .join("2024/03/12/german-private/index.html")
            .is_file()
    );
    assert!(
        !output
            .join("en/2024/03/12/german-private/index.html")
            .exists()
    );

    let sitemap = std::fs::read_to_string(output.join("sitemap.xml")).unwrap();
    assert!(sitemap.contains("https://example.com/2024/03/09/english-source"));
    assert!(sitemap.contains("https://example.com/en/2024/03/09/english-source"));
    assert!(sitemap.contains("https://example.com/2024/03/10/german-source"));
    assert!(sitemap.contains("https://example.com/en/2024/03/10/german-source"));
    assert!(sitemap.contains("https://example.com/2024/03/12/german-private"));
    assert!(!sitemap.contains("https://example.com/en/2024/03/12/german-private"));
    let main_rss = std::fs::read_to_string(output.join("rss.xml")).unwrap();
    let english_rss = std::fs::read_to_string(output.join("en/rss.xml")).unwrap();
    assert!(main_rss.contains("<title>english-source</title>"));
    assert!(!main_rss.contains("<title>Deutsche Übersetzung</title>"));
    assert!(english_rss.contains("<title>english-source</title>"));
    assert!(english_rss.contains("<title>English translation</title>"));
    assert!(!english_rss.contains("<title>Deutscher Fallback</title>"));
}

#[test]
fn section_generation_reports_its_urls_and_defers_pagefind() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let posts = vec![PublishedPostSource {
        post: make_post("hello", 1_710_000_000_000),
        body_markdown: "Hello **world**".into(),
    }];
    let mut urls = Vec::new();

    let report = render_site_section_with_progress(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &posts,
        GenerationSection::Single,
        |current, total, url| urls.push((current, total, url.to_string())),
        |_| {},
        || false,
    )
    .unwrap();

    assert_eq!(urls, vec![(1, 1, "/2024/03/09/hello".to_string())]);
    assert_eq!(report.written_paths, vec!["2024/03/09/hello/index.html"]);
    assert!(!dir.path().join("pagefind").exists());

    let index_report = build_site_search_index(db.conn(), dir.path(), "p1", &metadata).unwrap();
    assert!(!index_report.written_paths.is_empty());
    assert!(
        dir.path().join("pagefind/pagefind-ui.js").exists(),
        "pagefind outputs: {:?}",
        index_report.written_paths
    );

    let old_fragment = index_report
        .written_paths
        .iter()
        .find(|path| path.contains("/fragment/"))
        .cloned()
        .unwrap();
    let changed_posts = vec![PublishedPostSource {
        post: make_post("hello", 1_710_000_000_000),
        body_markdown: "Changed body".into(),
    }];
    render_site_section_with_progress(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &changed_posts,
        GenerationSection::Single,
        |_current, _total, _url| {},
        |_| {},
        || false,
    )
    .unwrap();
    let rebuilt = build_site_search_index(db.conn(), dir.path(), "p1", &metadata).unwrap();
    assert!(rebuilt.deleted_paths.contains(&old_fragment));
    assert!(!dir.path().join(old_fragment).exists());
}

#[test]
fn list_pages_receive_only_their_page_post_data_like_bds2() {
    let (db, dir) = setup();
    insert_template(
        db.conn(),
        &make_list_template("list", "POST_DATA={{ post_data_json_by_id | size }}"),
    )
    .unwrap();
    let mut metadata = make_metadata();
    metadata.max_posts_per_page = 1;
    let posts = vec![
        PublishedPostSource {
            post: make_post("hello", 1_710_000_000_000),
            body_markdown: "Hello".into(),
        },
        PublishedPostSource {
            post: make_post("next", 1_710_086_400_000),
            body_markdown: "Next".into(),
        },
    ];

    render_site_section_with_progress(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &posts,
        GenerationSection::Core,
        |_, _, _| {},
        |_| {},
        || false,
    )
    .unwrap();

    assert_eq!(
        std::fs::read_to_string(dir.path().join("index.html")).unwrap(),
        "POST_DATA=1"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("page/2/index.html")).unwrap(),
        "POST_DATA=1"
    );
}

#[test]
fn full_generation_uses_the_project_description_for_every_home_page_title() {
    let (db, dir) = setup();
    let mut metadata = make_metadata();
    metadata.description = Some("My Wonderful Blog".into());
    metadata.blog_languages = vec!["en".into(), "de".into()];
    let posts = vec![PublishedPostSource {
        post: make_post("hello", 1_710_000_000_000),
        body_markdown: "Hello".into(),
    }];

    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();

    for path in ["index.html", "de/index.html"] {
        let html = std::fs::read_to_string(dir.path().join(path)).unwrap();
        assert!(
            html.contains("<title>My Wonderful Blog</title>"),
            "unexpected document title in {path}: {html}"
        );
        assert!(
            html.contains("<h1 class=\"archive-heading\">My Wonderful Blog</h1>"),
            "missing homepage title in {path}: {html}"
        );
    }
}

#[test]
fn section_generation_reports_while_html_is_rendered_before_files_are_written() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let posts = vec![PublishedPostSource {
        post: make_post("hello", 1_710_000_000_000),
        body_markdown: "Hello **world**".into(),
    }];
    let output = dir.path().join("2024/03/09/hello/index.html");
    let rendered_before_write = std::sync::atomic::AtomicBool::new(false);
    let rendered = std::sync::atomic::AtomicUsize::new(0);
    let estimated = estimate_site_render_pages(dir.path(), &metadata, &[posts[0].post.clone()])
        .unwrap()[&GenerationSection::Single];

    render_site_section_with_progress(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &posts,
        GenerationSection::Single,
        |_current, _total, _url| {},
        |_| {
            rendered.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            rendered_before_write.store(!output.exists(), std::sync::atomic::Ordering::Relaxed);
        },
        || false,
    )
    .unwrap();

    assert!(rendered_before_write.load(std::sync::atomic::Ordering::Relaxed));
    assert_eq!(
        rendered.load(std::sync::atomic::Ordering::Relaxed),
        estimated
    );
}

#[test]
fn validation_apply_rewrites_only_the_reported_urls() {
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
    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    std::fs::write(dir.path().join("2024/03/09/hello/index.html"), "tampered").unwrap();
    let validation = bds_core::engine::validate_site::SiteValidationReport {
        stale_pages: vec!["2024/03/09/hello/index.html".into()],
        ..Default::default()
    };
    let mut urls = Vec::new();

    let report = apply_validation_section_with_progress(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &posts,
        &validation,
        GenerationSection::Single,
        |current, total, url| urls.push((current, total, url.to_string())),
        |_| {},
        || false,
    )
    .unwrap();

    assert_eq!(urls, vec![(1, 1, "/2024/03/09/hello".to_string())]);
    assert_eq!(report.written_paths, vec!["2024/03/09/hello/index.html"]);
    assert!(
        !report
            .skipped_paths
            .contains(&"2024/03/10/next/index.html".to_string())
    );
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

    let report =
        generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "de").unwrap();

    assert!(report.written_paths.contains(&"en/atom.xml".to_string()));
    assert!(report.written_paths.contains(&"en/rss.xml".to_string()));
    assert!(!report.written_paths.contains(&"en/sitemap.xml".to_string()));
    assert!(!report.written_paths.contains(&"feed.xml".to_string()));
    assert!(report.written_paths.contains(&"en/404.html".to_string()));
    assert!(
        report
            .written_paths
            .contains(&"en/pagefind/pagefind-ui.js".to_string())
    );

    let atom = std::fs::read_to_string(dir.path().join("en/atom.xml")).unwrap();
    assert!(atom.starts_with("<feed><title>Blog (en)</title>"));

    let sitemap = std::fs::read_to_string(dir.path().join("sitemap.xml")).unwrap();
    assert!(sitemap.contains("hreflang=\"de\" href=\"https://example.com/\""));
    assert!(sitemap.contains("hreflang=\"en\" href=\"https://example.com/en/\""));
    assert!(sitemap.contains("hreflang=\"x-default\" href=\"https://example.com/\""));
    assert!(sitemap.contains("https://example.com/category/article"));
}

#[test]
fn page_category_posts_also_generate_flat_routes() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let mut page = make_post("about", 1_710_000_000_000);
    page.categories = vec!["page".into()];
    let posts = vec![PublishedPostSource {
        post: page,
        body_markdown: "About this site".into(),
    }];

    let report =
        generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();

    assert!(
        report
            .written_paths
            .contains(&"about/index.html".to_string())
    );
    assert!(dir.path().join("about/index.html").is_file());
    let sitemap = std::fs::read_to_string(dir.path().join("sitemap.xml")).unwrap();
    assert!(sitemap.contains("<loc>https://example.com/about/</loc>"));
}

#[test]
fn page_aliases_belong_to_core_while_dated_posts_belong_to_single() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let mut page = make_post("about", 1_710_000_000_000);
    page.categories = vec!["page".into()];
    let posts = vec![PublishedPostSource {
        post: page,
        body_markdown: "About this site".into(),
    }];

    let single = render_site_section_with_progress(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &posts,
        GenerationSection::Single,
        |_current, _total, _url| {},
        |_| {},
        || false,
    )
    .unwrap();
    assert_eq!(single.written_paths, vec!["2024/03/09/about/index.html"]);
    assert!(!dir.path().join("about/index.html").exists());

    let core = render_site_section_with_progress(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &posts,
        GenerationSection::Core,
        |_current, _total, _url| {},
        |_| {},
        || false,
    )
    .unwrap();
    assert!(core.written_paths.contains(&"about/index.html".to_string()));
    assert!(dir.path().join("about/index.html").is_file());
}

#[test]
fn generation_respects_category_list_settings_and_writes_bundled_images() {
    let (db, dir) = setup();
    insert_template(
        db.conn(),
        &make_list_template(
            "featured-list",
            "FEATURED:{% if archive_context %}{% if archive_context.kind == 'category' %}{{ archive_context.name }}{% endif %}{% endif %}:{% for day_block in day_blocks %}{% for post in day_block.posts %}[{{ post.title }}|{{ post.show_title }}]{% endfor %}{% endfor %}",
        ),
    )
    .unwrap();
    write_category_meta_json(
        dir.path(),
        &HashMap::from([
            (
                "hidden".to_string(),
                CategorySettings {
                    title: None,
                    titles: BTreeMap::new(),
                    render_in_lists: false,
                    show_title: true,
                    post_template_slug: None,
                    list_template_slug: None,
                },
            ),
            (
                "featured".to_string(),
                CategorySettings {
                    title: Some("Featured Archive".to_string()),
                    titles: BTreeMap::from([("de".into(), "Ausgewählt".into())]),
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

    let report =
        generate_starter_site(db.conn(), dir.path(), "p1", &make_metadata(), &posts, "en").unwrap();

    for asset in [
        "images/close.png",
        "images/loading.gif",
        "images/next.png",
        "images/prev.png",
    ] {
        assert!(report.written_paths.contains(&asset.to_string()));
        assert!(
            dir.path().join(asset).exists(),
            "missing bundled image {asset}"
        );
    }

    assert!(
        !report
            .written_paths
            .contains(&"category/hidden/index.html".to_string())
    );
    assert!(
        report
            .written_paths
            .contains(&"category/featured/index.html".to_string())
    );

    let index_html = std::fs::read_to_string(dir.path().join("index.html")).unwrap();
    assert!(!index_html.contains("Hidden Post"));
    assert!(index_html.contains("[Featured Post|false]"));

    let featured_html =
        std::fs::read_to_string(dir.path().join("category/featured/index.html")).unwrap();
    assert!(featured_html.contains("FEATURED:Featured Archive:[Featured Post|false]"));

    let rss = std::fs::read_to_string(dir.path().join("rss.xml")).unwrap();
    assert!(rss.contains("hidden-post"));
    assert!(rss.contains("featured-post"));

    let calendar = std::fs::read_to_string(dir.path().join("calendar.json")).unwrap();
    assert!(calendar.contains("2024-03-09"));
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

    let first =
        generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    let second =
        generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();

    assert!(!first.written_paths.is_empty());
    assert!(second.skipped_paths.contains(&"index.html".to_string()));
    assert!(second.skipped_paths.contains(&"calendar.json".to_string()));
    assert!(second.skipped_paths.contains(&"rss.xml".to_string()));
    assert!(!second.skipped_paths.contains(&"feed.xml".to_string()));
    assert!(
        second
            .skipped_paths
            .contains(&"assets/pico.min.css".to_string())
    );
}

#[test]
fn forced_generation_rewrites_unchanged_outputs_without_erasing_unrelated_cache_records() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let posts = vec![PublishedPostSource {
        post: make_post("hello", 1_710_000_000_000),
        body_markdown: "Hello **world**".into(),
    }];

    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    let unchanged =
        generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    assert!(unchanged.skipped_paths.contains(&"index.html".to_string()));
    std::fs::write(dir.path().join("index.html"), "tampered").unwrap();
    let normal_after_drift =
        generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    assert!(
        normal_after_drift
            .skipped_paths
            .contains(&"index.html".to_string())
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("index.html")).unwrap(),
        "tampered"
    );
    bds_core::db::queries::generated_file_hash::upsert_generated_file_hash(
        db.conn(),
        &bds_core::model::GeneratedFileHash {
            project_id: "p1".into(),
            relative_path: "legacy-output.txt".into(),
            content_hash: "legacy".into(),
            updated_at: 42,
        },
    )
    .unwrap();

    let forced =
        generate_starter_site_forced(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();

    assert!(forced.written_paths.contains(&"index.html".to_string()));
    assert!(forced.skipped_paths.is_empty());
    assert_ne!(
        std::fs::read_to_string(dir.path().join("index.html")).unwrap(),
        "tampered"
    );
    assert!(
        bds_core::db::queries::generated_file_hash::get_generated_file_hash(
            db.conn(),
            "p1",
            "legacy-output.txt",
        )
        .is_ok()
    );
}

#[test]
fn site_validation_matches_bds2_index_route_comparison() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let mut post = make_post("hello", 1_710_000_000_000);
    write_published_snapshot(&dir, &mut post, "Hello **world**");
    bds_core::db::queries::post::insert_post(db.conn(), &post).unwrap();
    let posts = vec![PublishedPostSource {
        post,
        body_markdown: "Hello **world**".into(),
    }];

    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    std::fs::write(dir.path().join("index.html"), "tampered but non-empty").unwrap();
    std::fs::remove_file(dir.path().join("atom.xml")).unwrap();
    std::fs::write(dir.path().join("ghost.xml"), "not a route").unwrap();

    let report = validate_site(db.conn(), dir.path(), "p1").unwrap();

    assert!(report.stale_pages.is_empty());
    assert!(report.missing_pages.is_empty());
    assert!(report.extra_pages.is_empty());
}

#[test]
fn site_validation_detects_missing_zero_byte_and_extra_index_routes() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let mut post = make_post("hello", 1_710_000_000_000);
    write_published_snapshot(&dir, &mut post, "Hello **world**");
    bds_core::db::queries::post::insert_post(db.conn(), &post).unwrap();
    let posts = vec![PublishedPostSource {
        post,
        body_markdown: "Hello **world**".into(),
    }];

    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    std::fs::write(dir.path().join("index.html"), "").unwrap();
    let extra = dir.path().join("ghost/index.html");
    std::fs::create_dir_all(extra.parent().unwrap()).unwrap();
    std::fs::write(&extra, "ghost").unwrap();
    let zero_byte_extra = dir.path().join("empty/index.html");
    std::fs::create_dir_all(zero_byte_extra.parent().unwrap()).unwrap();
    std::fs::write(&zero_byte_extra, "").unwrap();

    let report = validate_site(db.conn(), dir.path(), "p1").unwrap();

    assert!(report.missing_pages.contains(&"index.html".to_string()));
    assert!(report.extra_pages.contains(&"ghost/index.html".to_string()));
    assert!(report.extra_pages.contains(&"empty/index.html".to_string()));
}

#[test]
fn site_validation_detects_post_sources_newer_than_generated_routes() {
    use std::fs::{File, FileTimes};
    use std::time::{Duration, SystemTime};

    let (db, dir) = setup();
    let metadata = make_metadata();
    let mut post = make_post("hello", 1_710_000_000_000);
    write_published_snapshot(&dir, &mut post, "Hello **world**");
    let source_path = dir.path().join(&post.file_path);
    bds_core::db::queries::post::insert_post(db.conn(), &post).unwrap();
    let posts = vec![PublishedPostSource {
        post,
        body_markdown: "Hello **world**".into(),
    }];

    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    File::options()
        .write(true)
        .open(source_path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(SystemTime::now() + Duration::from_secs(5)))
        .unwrap();

    let report = validate_site(db.conn(), dir.path(), "p1").unwrap();

    assert!(
        report
            .stale_pages
            .contains(&"2024/03/09/hello/index.html".to_string())
    );
}

#[test]
fn site_validation_refreshes_sitemap_without_rendering_pages() {
    let (db, dir) = setup();
    let mut post = make_post("hello", 1_710_000_000_000);
    write_published_snapshot(&dir, &mut post, "Hello **world**");
    bds_core::db::queries::post::insert_post(db.conn(), &post).unwrap();

    let report = validate_site(db.conn(), dir.path(), "p1").unwrap();
    let sitemap = std::fs::read_to_string(dir.path().join("sitemap.xml")).unwrap();

    assert!(
        report
            .missing_pages
            .contains(&"2024/03/09/hello/index.html".to_string())
    );
    assert!(sitemap.contains("https://example.com/2024/03/09/hello/"));
    assert!(!dir.path().join("2024/03/09/hello/index.html").exists());
}

#[test]
fn apply_validation_repairs_core_section_outputs() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let mut post = make_post("hello", 1_710_000_000_000);
    write_published_snapshot(&dir, &mut post, "Hello **world**");
    bds_core::db::queries::post::insert_post(db.conn(), &post).unwrap();
    let posts = vec![PublishedPostSource {
        post,
        body_markdown: "Hello **world**".into(),
    }];

    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    std::fs::remove_file(dir.path().join("index.html")).unwrap();

    let report = validate_site(db.conn(), dir.path(), "p1").unwrap();
    let sections = sections_from_validation_report(&report, &metadata);
    let apply_report = apply_validation_sections(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &posts,
        &report,
        &sections,
    )
    .unwrap();
    let repaired = validate_site(db.conn(), dir.path(), "p1").unwrap();

    assert!(!apply_report.written_paths.is_empty() || !apply_report.skipped_paths.is_empty());
    assert!(repaired.missing_pages.is_empty());
    assert!(repaired.extra_pages.is_empty());
    assert!(repaired.stale_pages.is_empty());
}

#[test]
fn validation_apply_clears_unchanged_touched_post_routes() {
    use std::fs::{File, FileTimes};
    use std::time::{Duration, UNIX_EPOCH};

    let (db, dir) = setup();
    let metadata = make_metadata();
    let mut post = make_post("hello", 1_710_000_000_000);
    write_published_snapshot(&dir, &mut post, "Hello **world**");
    let source_path = dir.path().join(&post.file_path);
    bds_core::db::queries::post::insert_post(db.conn(), &post).unwrap();
    let posts = vec![PublishedPostSource {
        post,
        body_markdown: "Hello **world**".into(),
    }];

    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    let relative_path = "2024/03/09/hello/index.html";
    let output_path = dir.path().join(relative_path);
    File::options()
        .write(true)
        .open(&output_path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(10)))
        .unwrap();
    File::options()
        .write(true)
        .open(source_path)
        .unwrap()
        .set_times(FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(20)))
        .unwrap();
    let mut hash = bds_core::db::queries::generated_file_hash::get_generated_file_hash(
        db.conn(),
        "p1",
        relative_path,
    )
    .unwrap();
    hash.updated_at = 10_000;
    bds_core::db::queries::generated_file_hash::upsert_generated_file_hash(db.conn(), &hash)
        .unwrap();

    let validation = validate_site(db.conn(), dir.path(), "p1").unwrap();
    assert_eq!(validation.stale_pages, vec![relative_path.to_string()]);
    let sections = sections_from_validation_report(&validation, &metadata);
    let applied = apply_validation_sections(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &posts,
        &validation,
        &sections,
    )
    .unwrap();
    let repaired = validate_site(db.conn(), dir.path(), "p1").unwrap();

    assert!(applied.skipped_paths.contains(&relative_path.to_string()));
    assert!(repaired.stale_pages.is_empty());
}

#[test]
fn validation_apply_expands_a_missing_post_to_its_aggregates() {
    let (db, dir) = setup();
    let mut metadata = make_metadata();
    metadata.max_posts_per_page = 1;
    let first = make_post("first", 1_710_000_000_000);
    let second = make_post("second", 1_710_000_001_000);

    generate_starter_site(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &[PublishedPostSource {
            post: first.clone(),
            body_markdown: "First body".into(),
        }],
        "en",
    )
    .unwrap();

    let posts = vec![
        PublishedPostSource {
            post: first,
            body_markdown: "First body".into(),
        },
        PublishedPostSource {
            post: second,
            body_markdown: "Second body".into(),
        },
    ];
    let validation = bds_core::engine::validate_site::SiteValidationReport {
        missing_pages: vec!["2024/03/09/second/index.html".into()],
        extra_pages: Vec::new(),
        stale_pages: Vec::new(),
    };
    let sections = sections_from_validation_report(&validation, &metadata);

    apply_validation_sections(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &posts,
        &validation,
        &sections,
    )
    .unwrap();

    for path in [
        "index.html",
        "category/article/index.html",
        "tag/rust/index.html",
        "2024/index.html",
        "2024/03/index.html",
        "2024/03/09/index.html",
    ] {
        assert!(
            std::fs::read_to_string(dir.path().join(path))
                .unwrap()
                .contains("second"),
            "aggregate {path} was not refreshed"
        );
    }
    for path in [
        "page/2/index.html",
        "category/article/page/2/index.html",
        "tag/rust/page/2/index.html",
        "2024/page/2/index.html",
        "2024/03/page/2/index.html",
        "2024/03/09/page/2/index.html",
    ] {
        assert!(
            std::fs::read_to_string(dir.path().join(path))
                .unwrap()
                .contains("first"),
            "aggregate pagination {path} was not refreshed"
        );
    }
}

#[test]
fn apply_validation_removes_extra_section_outputs() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let mut post = make_post("hello", 1_710_000_000_000);
    write_published_snapshot(&dir, &mut post, "Hello **world**");
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
    assert!(
        report
            .extra_pages
            .contains(&"tag/ghost/index.html".to_string())
    );
    let sections = sections_from_validation_report(&report, &metadata);
    let apply_report = apply_validation_sections(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &posts,
        &report,
        &sections,
    )
    .unwrap();
    let repaired = validate_site(db.conn(), dir.path(), "p1").unwrap();

    assert!(
        apply_report
            .deleted_paths
            .contains(&"tag/ghost/index.html".to_string())
    );
    assert!(repaired.extra_pages.is_empty());
}

#[test]
fn configured_language_prefixes_select_their_actual_validation_section() {
    let mut metadata = make_metadata();
    metadata.blog_languages = vec!["en".into(), "pt".into(), "ja".into()];
    let report = bds_core::engine::validate_site::SiteValidationReport {
        missing_pages: vec![
            "pt/category/ghost/index.html".into(),
            "ja/category/ghost/index.html".into(),
        ],
        extra_pages: Vec::new(),
        stale_pages: Vec::new(),
    };

    assert_eq!(
        sections_from_validation_report(&report, &metadata),
        vec![GenerationSection::Category]
    );
}

#[test]
fn localized_page_paths_select_the_core_validation_section() {
    let mut metadata = make_metadata();
    metadata.blog_languages.push("de".into());
    let report = bds_core::engine::validate_site::SiteValidationReport {
        missing_pages: vec!["de/about/index.html".into()],
        extra_pages: Vec::new(),
        stale_pages: Vec::new(),
    };

    assert_eq!(
        sections_from_validation_report(&report, &metadata),
        vec![GenerationSection::Core]
    );
}

#[test]
fn validation_apply_repairs_and_cleans_localized_page_outputs() {
    let (db, dir) = setup();
    let mut metadata = make_metadata();
    metadata.blog_languages = vec!["en".into(), "de".into()];
    bds_core::engine::meta::write_project_json(dir.path(), &metadata).unwrap();
    let mut page = make_post("about", 1_710_000_000_000);
    page.categories = vec!["page".into()];
    write_published_snapshot(&dir, &mut page, "About");
    bds_core::db::queries::post::insert_post(db.conn(), &page).unwrap();
    let posts = vec![PublishedPostSource {
        post: page,
        body_markdown: "About".into(),
    }];

    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    let localized_page = dir.path().join("de/about/index.html");
    let localized_extra = dir.path().join("de/ghost/index.html");
    std::fs::write(&localized_page, "").unwrap();
    std::fs::create_dir_all(localized_extra.parent().unwrap()).unwrap();
    std::fs::write(&localized_extra, "ghost").unwrap();

    let validation = validate_site(db.conn(), dir.path(), "p1").unwrap();
    let sections = sections_from_validation_report(&validation, &metadata);
    let applied = apply_validation_sections(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &posts,
        &validation,
        &sections,
    )
    .unwrap();
    let repaired = validate_site(db.conn(), dir.path(), "p1").unwrap();

    assert_eq!(sections, vec![GenerationSection::Core]);
    assert!(
        applied
            .written_paths
            .contains(&"de/about/index.html".into())
    );
    assert!(
        applied
            .deleted_paths
            .contains(&"de/ghost/index.html".into())
    );
    assert!(!localized_extra.exists());
    assert!(repaired.missing_pages.is_empty());
    assert!(repaired.extra_pages.is_empty());
    assert!(repaired.stale_pages.is_empty());
}

#[test]
fn validation_apply_removes_extra_output_under_configured_language() {
    let (db, dir) = setup();
    let mut metadata = make_metadata();
    metadata.blog_languages = vec!["en".into(), "pt".into(), "ja".into()];
    bds_core::engine::meta::write_project_json(dir.path(), &metadata).unwrap();
    let mut post = make_post("hello", 1_710_000_000_000);
    write_published_snapshot(&dir, &mut post, "Hello **world**");
    bds_core::db::queries::post::insert_post(db.conn(), &post).unwrap();
    let posts = vec![PublishedPostSource {
        post,
        body_markdown: "Hello **world**".into(),
    }];

    generate_starter_site(db.conn(), dir.path(), "p1", &metadata, &posts, "en").unwrap();
    let extra = dir.path().join("pt/category/ghost/index.html");
    std::fs::create_dir_all(extra.parent().unwrap()).unwrap();
    std::fs::write(&extra, "ghost").unwrap();

    let validation = validate_site(db.conn(), dir.path(), "p1").unwrap();
    let sections = sections_from_validation_report(&validation, &metadata);
    let applied = apply_validation_sections(
        db.conn(),
        dir.path(),
        "p1",
        &metadata,
        &posts,
        &validation,
        &sections,
    )
    .unwrap();

    assert_eq!(sections, vec![GenerationSection::Category]);
    assert!(
        applied
            .deleted_paths
            .contains(&"pt/category/ghost/index.html".to_string())
    );
    assert!(!extra.exists());
}

#[test]
fn site_validation_uses_html_output_directory_when_present() {
    let (db, dir) = setup();
    let metadata = make_metadata();
    let mut post = make_post("hello", 1_710_000_000_000);
    write_published_snapshot(&dir, &mut post, "Hello **world**");
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
