//! Verify that the Rust Diesel models read the compatibility fixture produced by bDS.

use std::collections::HashSet;
use std::path::PathBuf;

use bds_core::db::Database;
use bds_core::db::queries::{
    media, post, post_link, post_media, post_translation, project, script, setting, tag, template,
};
use bds_core::db::schema::{ai_catalog_meta, ai_models, ai_providers};
use bds_core::model::{ScriptKind, ScriptStatus, TemplateKind, TemplateStatus};
use diesel::prelude::*;

const PROJECT_ID: &str = "1979237c-034d-41f6-99a0-f35eb57b3f6c";
const ESMERALDA_ID: &str = "40a83ab1-423d-4310-aac4-642d84675007";
const GHOSTTY_ID: &str = "6745981d-da41-4cfd-80ec-95ad339acf6f";
const CMUX_ID: &str = "2665bfaa-8251-468d-a710-a4cf34dd81e2";
const SPIDER_ID: &str = "eb0cf9d7-6fbd-4b74-9be3-759d6e16f240";

fn fixture_db() -> Database {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/compatibility-projects/rfc1437-sample/bds.db");
    assert!(path.exists(), "fixture DB not found at {}", path.display());
    Database::open(&path).unwrap()
}

#[test]
fn read_project() {
    let db = fixture_db();
    let value = project::get_project_by_id(db.conn(), PROJECT_ID).unwrap();
    assert_eq!(value.name, "rfc1437");
    assert_eq!(value.slug, "rfc1437");
    assert!(value.data_path.unwrap().contains("rfc1437.de"));
    assert!(value.is_active);
}

#[test]
fn posts_are_compatible() {
    let db = fixture_db();
    let posts = post::list_posts_by_project(db.conn(), PROJECT_ID).unwrap();
    assert_eq!(posts.len(), 4);
    let published = posts.iter().find(|post| post.slug == "esmeralda").unwrap();
    assert_eq!(published.title, "Esmeralda");
    assert_eq!(published.status, bds_core::model::PostStatus::Published);
    assert!(published.content.is_none());
    assert!(!published.tags.is_empty());
    assert!(published.created_at > 946_684_800);
    assert!(published.updated_at > 946_684_800);
    let draft = posts
        .iter()
        .find(|post| post.slug == "draft-fixture-post")
        .unwrap();
    assert!(draft.content.as_deref().unwrap().contains("**body**"));
    assert!(
        posts
            .iter()
            .filter(|post| post.status == bds_core::model::PostStatus::Published)
            .all(|post| !post.file_path.is_empty()
                && post.file_path.ends_with(&format!("{}.md", post.slug)))
    );
    let unique: HashSet<_> = posts
        .iter()
        .map(|post| (&post.project_id, &post.slug))
        .collect();
    assert_eq!(unique.len(), posts.len());
}

#[test]
fn post_translations_are_compatible() {
    let db = fixture_db();
    let posts = post::list_posts_by_project(db.conn(), PROJECT_ID).unwrap();
    let translations: Vec<_> = posts
        .iter()
        .flat_map(|post| {
            post_translation::list_post_translations_by_post(db.conn(), &post.id).unwrap()
        })
        .collect();
    assert_eq!(translations.len(), 4);
    assert!(
        translations
            .iter()
            .filter(|translation| translation.status == bds_core::model::PostStatus::Published)
            .all(|translation| translation.content.is_none())
    );
    let post_ids: HashSet<_> = posts.iter().map(|post| post.id.as_str()).collect();
    assert!(
        translations
            .iter()
            .all(|translation| post_ids.contains(translation.translation_for.as_str()))
    );
}

#[test]
fn relationships_are_compatible() {
    let db = fixture_db();
    let links = post_link::list_links_by_source(db.conn(), GHOSTTY_ID).unwrap();
    assert!(
        links
            .iter()
            .any(|link| link.target_post_id == CMUX_ID && link.link_text.is_some())
    );
    let media_links = post_media::list_post_media_by_post(db.conn(), ESMERALDA_ID).unwrap();
    assert!(
        media_links
            .iter()
            .any(|link| link.media_id == SPIDER_ID && link.sort_order == 0)
    );
}

#[test]
fn media_is_compatible() {
    let db = fixture_db();
    let item = media::get_media_by_id(db.conn(), SPIDER_ID).unwrap();
    assert!(item.filename.ends_with(".jpg"));
    assert_eq!(item.original_name, "CRW_1121.jpg");
    assert_eq!(item.mime_type, "image/jpeg");
    assert!(item.title.is_some());
    assert!(item.alt.is_some());
}

#[test]
fn tags_are_compatible() {
    let db = fixture_db();
    let tags = tag::list_tags_by_project(db.conn(), PROJECT_ID).unwrap();
    assert_eq!(
        tags.iter().map(|tag| tag.name.as_str()).collect::<Vec<_>>(),
        [
            "fotografie",
            "mac-os-x",
            "natur",
            "programmierung",
            "sysadmin"
        ]
    );
    let unique: HashSet<_> = tags.iter().map(|tag| tag.name.to_lowercase()).collect();
    assert_eq!(unique.len(), tags.len());
}

#[test]
fn templates_are_compatible() {
    let db = fixture_db();
    let templates = template::list_templates_by_project(db.conn(), PROJECT_ID).unwrap();
    let value = templates.first().unwrap();
    assert_eq!(value.slug, "testvorlage");
    assert_eq!(value.title, "Testvorlage");
    assert_eq!(value.kind, TemplateKind::Post);
    assert!(value.enabled);
    assert_eq!(value.status, TemplateStatus::Published);
    assert!(value.content.is_none());
}

#[test]
fn scripts_are_compatible() {
    let db = fixture_db();
    let scripts = script::list_scripts_by_project(db.conn(), PROJECT_ID).unwrap();
    assert_eq!(scripts.len(), 2);
    let value = scripts
        .iter()
        .find(|script| script.slug == "bgg_link")
        .unwrap();
    assert_eq!(value.title, "bgg link");
    assert_eq!(value.kind, ScriptKind::Transform);
    assert_eq!(value.entrypoint, "normalize_blogmark");
    assert!(value.enabled);
    assert_eq!(value.status, ScriptStatus::Published);
}

#[test]
fn settings_are_compatible() {
    let db = fixture_db();
    let settings = setting::list_all_settings(db.conn()).unwrap();
    assert_eq!(settings.len(), 5);
    assert!(settings.iter().all(|setting| !setting.key.is_empty()
        && !setting.value.is_empty()
        && setting.key.contains(PROJECT_ID)));
    assert!(
        settings
            .iter()
            .any(|setting| setting.key.contains("generation-hash"))
    );
}

#[test]
fn ai_catalog_is_compatible() {
    let db = fixture_db();
    let (providers, models, meta) = db
        .conn()
        .with(|conn| {
            Ok((
                ai_providers::table.count().get_result::<i64>(conn)?,
                ai_models::table.count().get_result::<i64>(conn)?,
                ai_catalog_meta::table.count().get_result::<i64>(conn)?,
            ))
        })
        .unwrap();
    assert_eq!((providers, models, meta), (1, 1, 2));
}

#[test]
fn relationships_reference_existing_entities() {
    let db = fixture_db();
    let projects = project::list_projects(db.conn()).unwrap();
    let project_ids: HashSet<_> = projects.iter().map(|project| project.id.as_str()).collect();
    let posts = post::list_posts_by_project(db.conn(), PROJECT_ID).unwrap();
    let post_ids: HashSet<_> = posts.iter().map(|post| post.id.as_str()).collect();
    let media = media::list_media_by_project(db.conn(), PROJECT_ID).unwrap();
    let media_ids: HashSet<_> = media.iter().map(|media| media.id.as_str()).collect();
    let tags = tag::list_tags_by_project(db.conn(), PROJECT_ID).unwrap();
    assert!(
        posts
            .iter()
            .all(|post| project_ids.contains(post.project_id.as_str()))
    );
    assert!(
        media
            .iter()
            .all(|media| project_ids.contains(media.project_id.as_str()))
    );
    assert!(
        tags.iter()
            .all(|tag| project_ids.contains(tag.project_id.as_str()))
    );
    for post in &posts {
        assert!(
            post_link::list_links_by_source(db.conn(), &post.id)
                .unwrap()
                .iter()
                .all(|link| post_ids.contains(link.target_post_id.as_str()))
        );
        assert!(
            post_media::list_post_media_by_post(db.conn(), &post.id)
                .unwrap()
                .iter()
                .all(|link| media_ids.contains(link.media_id.as_str()))
        );
    }
}
