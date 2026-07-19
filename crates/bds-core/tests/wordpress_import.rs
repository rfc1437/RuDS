use std::fs;
use std::path::Path;

use bds_core::db::Database;
use bds_core::db::fts::ensure_fts_tables;
use bds_core::engine::{meta, post, project, tag, wordpress_import as import};
use bds_core::model::{
    ImportItemKind, ImportItemStatus, ImportResolution, PostStatus, TaxonomyKind,
};
use bds_core::util::frontmatter::read_post_file;
use bds_core::util::sidecar::read_sidecar;
use image::DynamicImage;
use tempfile::TempDir;

fn setup() -> (Database, TempDir, bds_core::model::Project) {
    let db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    ensure_fts_tables(db.conn()).unwrap();
    let dir = TempDir::new().unwrap();
    let project = project::create_project(
        db.conn(),
        "WordPress Import",
        Some(dir.path().to_str().unwrap()),
    )
    .unwrap();
    (db, dir, project)
}

fn write_image(path: &Path) {
    DynamicImage::new_rgb8(4, 3).save(path).unwrap();
}

#[test]
fn parser_rejects_malformed_or_channel_less_xml_and_ignores_unknown_elements() {
    assert!(import::parse_wxr_xml("<rss><channel>").is_err());
    assert!(import::parse_wxr_xml("<rss></rss>").is_err());

    let parsed = import::parse_wxr_xml(&sample_wxr(
        "<unknown:payload>ignored</unknown:payload><item><title>Menu</title><wp:post_type>nav_menu_item</wp:post_type></item>",
    ))
    .unwrap();
    assert_eq!(parsed.site.title, "Legacy & Blog");
    assert_eq!(parsed.posts.len(), 1);
    assert_eq!(parsed.pages.len(), 1);
    assert_eq!(parsed.media.len(), 1);
    assert_eq!(parsed.categories[0].name, "General");
    assert_eq!(parsed.tags[0].name, "News");
}

#[test]
fn analysis_converts_html_and_shortcodes_and_classifies_every_status() {
    let (db, dir, project) = setup();
    meta::add_category(dir.path(), "GENERAL").unwrap();
    tag::create_tag(db.conn(), dir.path(), &project.id, "nEWs", None).unwrap();

    let update = post::create_post(
        db.conn(),
        dir.path(),
        &project.id,
        "Update",
        Some("Update body"),
        vec![],
        vec![],
        None,
        None,
        None,
    )
    .unwrap();
    post::update_post(
        db.conn(),
        dir.path(),
        &update.id,
        None,
        Some("hello-world"),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    let duplicate = post::create_post(
        db.conn(),
        dir.path(),
        &project.id,
        "Duplicate",
        Some("Duplicate body"),
        vec![],
        vec![],
        None,
        None,
        None,
    )
    .unwrap();
    assert_ne!(update.id, duplicate.id);

    let uploads = dir.path().join("uploads/2024/05");
    fs::create_dir_all(&uploads).unwrap();
    write_image(&uploads.join("photo.png"));
    let wxr = dir.path().join("legacy.xml");
    fs::write(&wxr, sample_wxr("")).unwrap();

    let mut progress = Vec::new();
    let report = import::analyze_wxr(
        db.conn(),
        dir.path(),
        &project.id,
        &wxr,
        Some(dir.path().join("uploads").as_path()),
        Some(&mut |value| progress.push(value)),
    )
    .unwrap();

    assert!(
        report.posts[0]
            .content
            .as_deref()
            .unwrap()
            .contains("**world**")
    );
    assert!(
        report.posts[0]
            .content
            .as_deref()
            .unwrap()
            .contains("[[gallery ids=\"1,2\"]]"),
        "{}",
        report.posts[0].content.as_deref().unwrap()
    );
    assert_eq!(report.posts[0].status, ImportItemStatus::Conflict);
    assert_eq!(report.posts[0].resolution, Some(ImportResolution::Ignore));
    assert_eq!(report.pages[0].status, ImportItemStatus::New);
    assert_eq!(report.media[0].status, ImportItemStatus::New);
    assert!(
        report
            .taxonomies
            .iter()
            .any(|item| { item.kind == TaxonomyKind::Category && item.exists_in_project })
    );
    assert!(
        report
            .taxonomies
            .iter()
            .any(|item| { item.kind == TaxonomyKind::Tag && item.exists_in_project })
    );
    assert_eq!(report.date_distribution[0].year, 2024);
    assert_eq!(report.macros[0].name, "gallery");
    assert!(!progress.is_empty());
}

#[test]
fn analysis_distinguishes_new_updates_conflicts_duplicates_and_missing_media() {
    let (db, dir, project) = setup();
    let uploads = dir.path().join("uploads/2024/05");
    fs::create_dir_all(&uploads).unwrap();
    write_image(&uploads.join("same.png"));
    fs::copy(uploads.join("same.png"), uploads.join("copy.png")).unwrap();
    DynamicImage::new_rgb8(5, 3)
        .save(uploads.join("different.png"))
        .unwrap();
    let old_different = dir.path().join("old-different.png");
    DynamicImage::new_rgb8(2, 2).save(&old_different).unwrap();
    bds_core::engine::media::import_media(
        db.conn(),
        dir.path(),
        &project.id,
        &uploads.join("same.png"),
        "same.png",
        None,
        None,
        None,
        None,
        None,
        Vec::new(),
    )
    .unwrap();
    bds_core::engine::media::import_media(
        db.conn(),
        dir.path(),
        &project.id,
        &old_different,
        "different.png",
        None,
        None,
        None,
        None,
        None,
        Vec::new(),
    )
    .unwrap();

    let same_body = "Hello **world**.\n\n[[gallery ids=\"1,2\"]]";
    let update = post::create_post(
        db.conn(),
        dir.path(),
        &project.id,
        "Hello World",
        Some(same_body),
        vec![],
        vec![],
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(update.slug, "hello-world");
    let collision = post::create_post(
        db.conn(),
        dir.path(),
        &project.id,
        "Collision",
        Some("Local body"),
        vec![],
        vec![],
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(collision.slug, "collision");
    post::create_post(
        db.conn(),
        dir.path(),
        &project.id,
        "Local Duplicate",
        Some("Shared duplicate body"),
        vec![],
        vec![],
        None,
        None,
        None,
    )
    .unwrap();

    let extras = format!(
        r#"
<item><title>Collision</title><content:encoded><![CDATA[Incoming body]]></content:encoded><wp:post_id>102</wp:post_id><wp:post_name>collision</wp:post_name><wp:status>draft</wp:status><wp:post_type>post</wp:post_type></item>
<item><title>Duplicate Content</title><content:encoded><![CDATA[<p>Shared duplicate body</p>]]></content:encoded><wp:post_id>103</wp:post_id><wp:post_name>duplicate-content</wp:post_name><wp:status>draft</wp:status><wp:post_type>post</wp:post_type></item>
{}
{}
{}
{}
{}"#,
        attachment_item(302, "Same", "same.png"),
        attachment_item(303, "Copy", "copy.png"),
        attachment_item(304, "Different", "different.png"),
        attachment_item(305, "Missing", "missing.png"),
        attachment_item(306, "Escape", "../escape.png"),
    );
    let wxr = dir.path().join("classifications.xml");
    fs::write(&wxr, sample_wxr(&extras)).unwrap();
    let posts_before = serde_json::to_value(
        bds_core::db::queries::post::list_posts_by_project(db.conn(), &project.id).unwrap(),
    )
    .unwrap();
    let media_before = serde_json::to_value(
        bds_core::db::queries::media::list_media_by_project(db.conn(), &project.id).unwrap(),
    )
    .unwrap();
    let tags_before = serde_json::to_value(
        bds_core::db::queries::tag::list_tags_by_project(db.conn(), &project.id).unwrap(),
    )
    .unwrap();
    let categories_before = fs::read(dir.path().join("meta/categories.json")).unwrap();
    let report = import::analyze_wxr(
        db.conn(),
        dir.path(),
        &project.id,
        &wxr,
        Some(dir.path().join("uploads").as_path()),
        None,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(
            bds_core::db::queries::post::list_posts_by_project(db.conn(), &project.id).unwrap()
        )
        .unwrap(),
        posts_before
    );
    assert_eq!(
        serde_json::to_value(
            bds_core::db::queries::media::list_media_by_project(db.conn(), &project.id).unwrap()
        )
        .unwrap(),
        media_before
    );
    assert_eq!(
        serde_json::to_value(
            bds_core::db::queries::tag::list_tags_by_project(db.conn(), &project.id).unwrap()
        )
        .unwrap(),
        tags_before
    );
    assert_eq!(
        fs::read(dir.path().join("meta/categories.json")).unwrap(),
        categories_before
    );

    let post_statuses = report
        .posts
        .iter()
        .map(|item| item.status)
        .collect::<Vec<_>>();
    assert!(post_statuses.contains(&ImportItemStatus::Update));
    assert!(post_statuses.contains(&ImportItemStatus::Conflict));
    assert!(post_statuses.contains(&ImportItemStatus::ContentDuplicate));
    let media_status = |name: &str| {
        report
            .media
            .iter()
            .find(|item| item.filename.as_deref() == Some(name))
            .unwrap()
            .status
    };
    assert_eq!(media_status("same.png"), ImportItemStatus::Update);
    assert_eq!(media_status("copy.png"), ImportItemStatus::ContentDuplicate);
    assert_eq!(media_status("different.png"), ImportItemStatus::Conflict);
    assert_eq!(media_status("missing.png"), ImportItemStatus::Missing);
    assert_eq!(media_status("escape.png"), ImportItemStatus::Missing);
    assert!(
        report
            .media
            .iter()
            .find(|item| item.filename.as_deref() == Some("escape.png"))
            .unwrap()
            .source_path
            .is_none()
    );
}

#[test]
fn definitions_round_trip_saved_analysis_and_are_project_scoped() {
    let (db, dir, project) = setup();
    let definition = import::create_definition(db.conn(), &project.id, "Legacy").unwrap();
    let wxr = dir.path().join("legacy.xml");
    fs::write(&wxr, sample_wxr("")).unwrap();
    let report = import::analyze_wxr(db.conn(), dir.path(), &project.id, &wxr, None, None).unwrap();

    let updated = import::update_definition(
        db.conn(),
        &definition.id,
        Some("Renamed"),
        Some(Some(wxr.as_path())),
        Some(Some(dir.path())),
        Some(Some(&report)),
    )
    .unwrap();
    assert_eq!(updated.name, "Renamed");
    assert_eq!(updated.analysis().unwrap().unwrap(), report);
    assert_eq!(
        import::list_definitions(db.conn(), &project.id)
            .unwrap()
            .len(),
        1
    );
    import::delete_definition(db.conn(), &definition.id).unwrap();
    assert!(import::get_definition(db.conn(), &definition.id).is_err());
}

#[test]
fn execution_uses_core_engines_preserves_metadata_and_links_media_parent() {
    let (db, dir, project) = setup();
    let uploads = dir.path().join("uploads/2024/05");
    fs::create_dir_all(&uploads).unwrap();
    write_image(&uploads.join("photo.png"));
    let wxr = dir.path().join("legacy.xml");
    fs::write(&wxr, sample_wxr("")).unwrap();
    let report = import::analyze_wxr(
        db.conn(),
        dir.path(),
        &project.id,
        &wxr,
        Some(dir.path().join("uploads").as_path()),
        None,
    )
    .unwrap();

    let mut progress = Vec::new();
    let result = import::execute_import(
        db.conn(),
        dir.path(),
        &project.id,
        &report,
        Some("Fallback Author"),
        Some(&mut |value| progress.push(value)),
    )
    .unwrap();
    assert_eq!(result.posts.imported, 1);
    assert_eq!(result.pages.imported, 1);
    assert_eq!(result.media.imported, 1);

    let posts = bds_core::db::queries::post::list_posts_by_project(db.conn(), &project.id).unwrap();
    let imported = posts
        .iter()
        .find(|post| post.slug == "hello-world")
        .unwrap();
    assert_eq!(imported.status, PostStatus::Published);
    assert_eq!(imported.author.as_deref(), Some("Importer"));
    assert_eq!(imported.created_at, 1_714_564_800_000);
    assert_eq!(imported.updated_at, 1_714_653_000_000);
    assert_eq!(imported.published_at, Some(1_714_564_800_000));
    assert_eq!(imported.file_path, "posts/2024/05/hello-world.md");
    assert!(dir.path().join(&imported.file_path).is_file());
    let raw = fs::read_to_string(dir.path().join(&imported.file_path)).unwrap();
    let (frontmatter, _) = read_post_file(&raw).unwrap();
    assert_eq!(frontmatter.created_at, imported.created_at);
    assert_eq!(frontmatter.updated_at, imported.updated_at);
    assert_eq!(frontmatter.published_at, imported.published_at);
    let page = posts.iter().find(|post| post.slug == "about").unwrap();
    assert!(page.categories.iter().any(|category| category == "page"));

    let media =
        bds_core::db::queries::media::list_media_by_project(db.conn(), &project.id).unwrap();
    assert_eq!(media[0].alt.as_deref(), Some("Photo description"));
    assert_eq!(media[0].author.as_deref(), Some("Fallback Author"));
    assert_eq!(media[0].created_at, 1_714_737_600_000);
    assert!(media[0].file_path.starts_with("media/2024/05/"));
    let sidecar =
        read_sidecar(&fs::read_to_string(dir.path().join(&media[0].sidecar_path)).unwrap())
            .unwrap();
    assert_eq!(sidecar.created_at, media[0].created_at);
    let links =
        bds_core::db::queries::post_media::list_post_media_by_media(db.conn(), &media[0].id)
            .unwrap();
    assert_eq!(links[0].post_id, imported.id);
    let phases = progress.iter().map(|item| item.phase).collect::<Vec<_>>();
    assert!(phases.windows(2).all(|window| window[0] <= window[1]));
    assert_eq!(phases.last(), Some(&bds_core::model::ImportPhase::Complete));
    for phase in [
        bds_core::model::ImportPhase::Taxonomy,
        bds_core::model::ImportPhase::Posts,
        bds_core::model::ImportPhase::Media,
        bds_core::model::ImportPhase::Pages,
    ] {
        assert!(
            progress
                .iter()
                .any(|item| item.phase == phase && item.current == 0),
            "missing start event for {phase:?}"
        );
    }

    let diff =
        bds_core::engine::metadata_diff::compute_metadata_diff(db.conn(), dir.path(), &project.id)
            .unwrap();
    assert!(diff.errors.is_empty(), "{:?}", diff.errors);
    assert!(
        diff.diffs
            .iter()
            .all(|item| item.entity_type != "post" && item.entity_type != "media"),
        "{:?}",
        diff.diffs
    );
    assert!(
        bds_core::engine::post::rebuild_posts_from_filesystem(db.conn(), dir.path(), &project.id,)
            .unwrap()
            .errors
            .is_empty()
    );
    assert!(
        bds_core::engine::media::rebuild_media_from_filesystem(db.conn(), dir.path(), &project.id,)
            .unwrap()
            .errors
            .is_empty()
    );
}

#[test]
fn conflict_import_uses_unique_slug_and_progress_panics_do_not_abort() {
    let (db, dir, project) = setup();
    let existing = post::create_post(
        db.conn(),
        dir.path(),
        &project.id,
        "Hello World",
        Some("local"),
        vec![],
        vec![],
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(existing.slug, "hello-world");
    let wxr = dir.path().join("legacy.xml");
    fs::write(&wxr, sample_wxr("")).unwrap();
    let mut report = import::analyze_wxr(
        db.conn(),
        dir.path(),
        &project.id,
        &wxr,
        None,
        Some(&mut |_| panic!("observer failure")),
    )
    .unwrap();
    import::set_conflict_resolution(
        &mut report,
        ImportItemKind::Post,
        "hello-world",
        ImportResolution::Import,
    )
    .unwrap();
    import::execute_import(
        db.conn(),
        dir.path(),
        &project.id,
        &report,
        None,
        Some(&mut |_| panic!("observer failure")),
    )
    .unwrap();
    let posts = bds_core::db::queries::post::list_posts_by_project(db.conn(), &project.id).unwrap();
    assert_eq!(posts.len(), 3);
    assert!(
        posts
            .iter()
            .any(|post| post.slug.starts_with("hello-world-") && post.slug != "hello-world")
    );
}

#[test]
fn default_ignore_resolution_skips_conflict_without_mutating_existing_post() {
    let (db, dir, project) = setup();
    let existing = post::create_post(
        db.conn(),
        dir.path(),
        &project.id,
        "Hello World",
        Some("Local body"),
        vec![],
        vec![],
        None,
        None,
        None,
    )
    .unwrap();
    let wxr = dir.path().join("ignore.xml");
    fs::write(&wxr, sample_wxr("")).unwrap();
    let mut report =
        import::analyze_wxr(db.conn(), dir.path(), &project.id, &wxr, None, None).unwrap();
    report.taxonomies.clear();
    report.pages.clear();
    report.media.clear();
    assert_eq!(report.posts[0].resolution, Some(ImportResolution::Ignore));

    let result =
        import::execute_import(db.conn(), dir.path(), &project.id, &report, None, None).unwrap();
    assert_eq!(result.posts.imported, 0);
    assert_eq!(result.posts.skipped, 1);
    let unchanged = bds_core::db::queries::post::get_post_by_id(db.conn(), &existing.id).unwrap();
    assert_eq!(unchanged.content.as_deref(), Some("Local body"));
}

#[test]
fn overwrite_resolution_preserves_existing_post_and_media_identity() {
    let (db, dir, project) = setup();
    let existing_post = post::create_post(
        db.conn(),
        dir.path(),
        &project.id,
        "Hello World",
        Some("Local post body"),
        vec![],
        vec![],
        None,
        None,
        None,
    )
    .unwrap();
    let old_image = dir.path().join("old-photo.png");
    DynamicImage::new_rgb8(2, 2).save(&old_image).unwrap();
    let existing_media = bds_core::engine::media::import_media(
        db.conn(),
        dir.path(),
        &project.id,
        &old_image,
        "photo.png",
        Some("Old photo"),
        Some("Old alt"),
        None,
        None,
        None,
        Vec::new(),
    )
    .unwrap();
    let uploads = dir.path().join("uploads/2024/05");
    fs::create_dir_all(&uploads).unwrap();
    write_image(&uploads.join("photo.png"));
    let wxr = dir.path().join("overwrite.xml");
    fs::write(&wxr, sample_wxr("")).unwrap();
    let mut report = import::analyze_wxr(
        db.conn(),
        dir.path(),
        &project.id,
        &wxr,
        Some(dir.path().join("uploads").as_path()),
        None,
    )
    .unwrap();
    import::set_conflict_resolution(
        &mut report,
        ImportItemKind::Post,
        "hello-world",
        ImportResolution::Overwrite,
    )
    .unwrap();
    import::set_conflict_resolution(
        &mut report,
        ImportItemKind::Media,
        "photo.png",
        ImportResolution::Overwrite,
    )
    .unwrap();

    import::execute_import(db.conn(), dir.path(), &project.id, &report, None, None).unwrap();
    let overwritten_post =
        bds_core::db::queries::post::get_post_by_id(db.conn(), &existing_post.id).unwrap();
    assert_eq!(overwritten_post.id, existing_post.id);
    assert_eq!(overwritten_post.status, PostStatus::Published);
    assert_eq!(
        overwritten_post.published_content.as_deref(),
        report.posts[0].content.as_deref()
    );
    let overwritten_media =
        bds_core::db::queries::media::get_media_by_id(db.conn(), &existing_media.id).unwrap();
    assert_eq!(overwritten_media.id, existing_media.id);
    assert_eq!(overwritten_media.alt.as_deref(), Some("Photo description"));
    assert_ne!(overwritten_media.checksum, existing_media.checksum);
}

#[test]
fn airplane_ai_mapping_requires_a_local_endpoint() {
    let (db, _dir, _project) = setup();
    let mut report = import::empty_report();
    report.taxonomies.push(import::taxonomy_candidate(
        TaxonomyKind::Tag,
        "Legacy News",
        false,
    ));
    let error = import::auto_map_taxonomy(db.conn(), _dir.path(), &_project.id, true, &mut report)
        .unwrap_err();
    assert!(error.to_string().contains("airplane"));
    assert!(report.taxonomies[0].mapped_to.is_none());
}

#[test]
fn execution_rejects_a_saved_taxonomy_mapping_to_a_nonexistent_term() {
    let (db, dir, project) = setup();
    let mut report = import::empty_report();
    let mut candidate = import::taxonomy_candidate(TaxonomyKind::Category, "Old", false);
    candidate.mapped_to = Some("Does Not Exist".to_string());
    report.taxonomies.push(candidate);

    let error = import::execute_import(db.conn(), dir.path(), &project.id, &report, None, None)
        .unwrap_err();
    assert!(error.to_string().contains("mapping target does not exist"));
    assert!(
        !meta::read_categories_json(dir.path())
            .unwrap()
            .iter()
            .any(|category| category == "Old")
    );
}

#[test]
fn execution_commits_complete_500_item_batches_and_rolls_back_database_and_files_for_failure() {
    let (db, dir, project) = setup();
    let wxr = dir.path().join("legacy.xml");
    fs::write(&wxr, sample_wxr("")).unwrap();
    let mut report =
        import::analyze_wxr(db.conn(), dir.path(), &project.id, &wxr, None, None).unwrap();
    let template = report.posts[0].clone();
    report.taxonomies.clear();
    report.media.clear();
    report.pages.clear();
    report.posts = (0..=500)
        .map(|index| {
            let mut item = template.clone();
            item.source_id = Some(10_000 + index);
            item.title = format!("Batch {index}");
            item.slug = Some(format!("batch-{index}"));
            item.status = ImportItemStatus::New;
            item.resolution = None;
            item.existing_id = None;
            item
        })
        .collect();
    let mut failing = template;
    failing.title = "Fail after file write".to_string();
    failing.slug = Some("batch-500".to_string());
    failing.status = ImportItemStatus::New;
    failing.resolution = None;
    failing.existing_id = None;
    report.posts.push(failing);

    assert!(
        import::execute_import(db.conn(), dir.path(), &project.id, &report, None, None).is_err()
    );
    let posts = bds_core::db::queries::post::list_posts_by_project(db.conn(), &project.id).unwrap();
    assert_eq!(posts.len(), 500);
    assert!(posts.iter().any(|post| post.slug == "batch-499"));
    assert!(!posts.iter().any(|post| post.slug == "batch-500"));
    assert!(!dir.path().join("posts/2024/05/batch-500.md").exists());
    assert!(dir.path().join("posts/2024/05/batch-499.md").is_file());
}

#[test]
fn failed_media_batch_restores_an_overwritten_binary_sidecar_and_database_row() {
    let (db, dir, project) = setup();
    let old_image = dir.path().join("old.png");
    DynamicImage::new_rgb8(2, 2).save(&old_image).unwrap();
    let existing = bds_core::engine::media::import_media(
        db.conn(),
        dir.path(),
        &project.id,
        &old_image,
        "photo.png",
        Some("Old title"),
        Some("Old alt"),
        None,
        None,
        None,
        Vec::new(),
    )
    .unwrap();
    let original_binary = fs::read(dir.path().join(&existing.file_path)).unwrap();
    let original_sidecar = fs::read(dir.path().join(&existing.sidecar_path)).unwrap();

    let uploads = dir.path().join("uploads/2024/05");
    fs::create_dir_all(&uploads).unwrap();
    write_image(&uploads.join("photo.png"));
    let wxr = dir.path().join("rollback-overwrite.xml");
    fs::write(&wxr, sample_wxr("")).unwrap();
    let mut report = import::analyze_wxr(
        db.conn(),
        dir.path(),
        &project.id,
        &wxr,
        Some(dir.path().join("uploads").as_path()),
        None,
    )
    .unwrap();
    report.taxonomies.clear();
    report.posts.clear();
    report.pages.clear();
    import::set_conflict_resolution(
        &mut report,
        ImportItemKind::Media,
        "photo.png",
        ImportResolution::Overwrite,
    )
    .unwrap();
    let mut missing = report.media[0].clone();
    missing.title = "Missing".to_string();
    missing.filename = Some("missing.png".to_string());
    missing.source_path = Some(
        dir.path()
            .join("missing.png")
            .to_string_lossy()
            .into_owned(),
    );
    missing.status = ImportItemStatus::New;
    missing.resolution = None;
    missing.existing_id = None;
    report.media.push(missing);

    assert!(
        import::execute_import(db.conn(), dir.path(), &project.id, &report, None, None).is_err()
    );
    let restored = bds_core::db::queries::media::get_media_by_id(db.conn(), &existing.id).unwrap();
    assert_eq!(
        serde_json::to_value(&restored).unwrap(),
        serde_json::to_value(&existing).unwrap()
    );
    assert_eq!(
        fs::read(dir.path().join(&existing.file_path)).unwrap(),
        original_binary
    );
    assert_eq!(
        fs::read(dir.path().join(&existing.sidecar_path)).unwrap(),
        original_sidecar
    );
}

fn sample_wxr(extra: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0" xmlns:excerpt="http://wordpress.org/export/1.2/excerpt/" xmlns:content="http://purl.org/rss/1.0/modules/content/" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:wp="http://wordpress.org/export/1.2/" xmlns:unknown="urn:unknown">
<channel><title>Legacy &amp; Blog</title><link>https://legacy.example</link><language>en</language>{extra}
<wp:category><wp:cat_name><![CDATA[General]]></wp:cat_name><wp:category_nicename>general</wp:category_nicename><wp:category_parent></wp:category_parent></wp:category>
<wp:tag><wp:tag_slug>news</wp:tag_slug><wp:tag_name><![CDATA[News]]></wp:tag_name></wp:tag>
<item><title>Hello World</title><pubDate>Wed, 01 May 2024 12:00:00 +0000</pubDate><dc:creator><![CDATA[Importer]]></dc:creator><content:encoded><![CDATA[<p>Hello <strong>world</strong>.</p><p>[gallery ids="1,2"]</p>]]></content:encoded><excerpt:encoded>Legacy hello</excerpt:encoded><wp:post_id>101</wp:post_id><wp:post_date>2024-05-01 12:00:00</wp:post_date><wp:post_modified>2024-05-02 12:30:00</wp:post_modified><wp:post_name>hello-world</wp:post_name><wp:status>publish</wp:status><wp:post_type>post</wp:post_type><category domain="category"><![CDATA[General]]></category><category domain="post_tag"><![CDATA[News]]></category></item>
<item><title>About</title><pubDate>Thu, 02 May 2024 12:00:00 +0000</pubDate><dc:creator>Importer</dc:creator><content:encoded><![CDATA[<p>About page</p>]]></content:encoded><wp:post_id>201</wp:post_id><wp:post_date>2024-05-02 12:00:00</wp:post_date><wp:post_modified>2024-05-02 12:30:00</wp:post_modified><wp:post_name>about</wp:post_name><wp:status>draft</wp:status><wp:post_type>page</wp:post_type><category domain="category">General</category></item>
<item><title>Photo</title><pubDate>Fri, 03 May 2024 12:00:00 +0000</pubDate><content:encoded><![CDATA[Photo description]]></content:encoded><wp:post_id>301</wp:post_id><wp:post_parent>101</wp:post_parent><wp:post_name>photo</wp:post_name><wp:status>inherit</wp:status><wp:post_type>attachment</wp:post_type><wp:attachment_url>https://legacy.example/wp-content/uploads/2024/05/photo.png</wp:attachment_url></item>
</channel></rss>"#
    )
}

fn attachment_item(id: i64, title: &str, filename: &str) -> String {
    format!(
        r#"<item><title>{title}</title><pubDate>Fri, 03 May 2024 12:00:00 +0000</pubDate><content:encoded>{title}</content:encoded><wp:post_id>{id}</wp:post_id><wp:post_parent>101</wp:post_parent><wp:post_name>{title}</wp:post_name><wp:status>inherit</wp:status><wp:post_type>attachment</wp:post_type><wp:attachment_url>https://legacy.example/wp-content/uploads/2024/05/{filename}</wp:attachment_url></item>"#
    )
}
