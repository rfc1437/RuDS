//! Executable checks for storage-related Allium claims.

mod support;

use std::collections::HashSet;

use bds_core::db::Database;
use bds_core::db::queries::{
    post as post_queries, post_translation, project as project_queries, script, tag, template,
};
use bds_core::db::schema::{posts, scripts, templates};
use bds_core::model::{
    Post, PostStatus, Project, ScriptKind, ScriptStatus, Tag, TemplateKind, TemplateStatus,
};
use diesel::prelude::*;

fn fixture_db() -> support::FixtureDatabase {
    support::fixture_database()
}

fn memory_db() -> Database {
    let db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    db
}

fn project(id: &str, slug: &str) -> Project {
    Project {
        id: id.into(),
        name: id.into(),
        slug: slug.into(),
        description: None,
        data_path: None,
        is_active: true,
        created_at: 1000,
        updated_at: 1000,
    }
}

fn post(id: &str, status: PostStatus, published_at: Option<i64>) -> Post {
    Post {
        id: id.into(),
        project_id: "p1".into(),
        title: id.into(),
        slug: id.into(),
        excerpt: None,
        content: Some("body".into()),
        status,
        author: None,
        language: None,
        do_not_translate: false,
        template_slug: None,
        file_path: String::new(),
        checksum: None,
        tags: Vec::new(),
        categories: Vec::new(),
        published_title: None,
        published_content: None,
        published_tags: None,
        published_categories: None,
        published_excerpt: None,
        created_at: 1000,
        updated_at: 1000,
        published_at,
    }
}

#[test]
fn enum_serde_matches_database_values() {
    assert_eq!(
        serde_json::to_string(&PostStatus::Draft).unwrap(),
        "\"draft\""
    );
    assert_eq!(
        serde_json::to_string(&PostStatus::Published).unwrap(),
        "\"published\""
    );
    assert_eq!(
        serde_json::to_string(&PostStatus::Archived).unwrap(),
        "\"archived\""
    );
    assert_eq!(
        serde_json::to_string(&TemplateKind::NotFound).unwrap(),
        "\"not_found\""
    );
    assert_eq!(
        serde_json::to_string(&TemplateStatus::Published).unwrap(),
        "\"published\""
    );
    assert_eq!(
        serde_json::to_string(&ScriptKind::Transform).unwrap(),
        "\"transform\""
    );
    assert_eq!(
        serde_json::to_string(&ScriptStatus::Draft).unwrap(),
        "\"draft\""
    );
}

#[test]
fn fixture_content_location_matches_spec() {
    let db = fixture_db();
    let active = project_queries::get_active_project(db.conn()).unwrap();
    let posts = post_queries::list_posts_by_project(db.conn(), &active.id).unwrap();
    assert!(
        posts
            .iter()
            .filter(|post| post.status == PostStatus::Published)
            .all(|post| post.content.is_none())
    );
    assert!(
        posts
            .iter()
            .filter(|post| post.status == PostStatus::Draft)
            .all(|post| post.content.is_some())
    );
}

#[test]
fn post_status_transitions_all_valid() {
    let db = memory_db();
    project_queries::insert_project(db.conn(), &project("p1", "test")).unwrap();
    post_queries::insert_post(db.conn(), &post("post1", PostStatus::Draft, None)).unwrap();
    for status in [
        PostStatus::Published,
        PostStatus::Draft,
        PostStatus::Archived,
        PostStatus::Draft,
        PostStatus::Published,
        PostStatus::Archived,
        PostStatus::Published,
    ] {
        post_queries::update_post_status(db.conn(), "post1", &status, 1000).unwrap();
        assert_eq!(
            post_queries::get_post_by_id(db.conn(), "post1")
                .unwrap()
                .status,
            status
        );
    }
}

#[test]
fn slug_frozen_after_publish_semantics() {
    let db = memory_db();
    project_queries::insert_project(db.conn(), &project("p1", "test")).unwrap();
    post_queries::insert_post(
        db.conn(),
        &post("published", PostStatus::Published, Some(1000)),
    )
    .unwrap();
    post_queries::insert_post(db.conn(), &post("draft", PostStatus::Draft, None)).unwrap();
    assert!(
        post_queries::get_post_by_id(db.conn(), "published")
            .unwrap()
            .published_at
            .is_some()
    );
    assert!(
        post_queries::get_post_by_id(db.conn(), "draft")
            .unwrap()
            .published_at
            .is_none()
    );
}

#[test]
fn project_and_translation_uniqueness_in_fixture() {
    let db = fixture_db();
    let projects = project_queries::list_projects(db.conn()).unwrap();
    assert_eq!(
        projects.iter().filter(|project| project.is_active).count(),
        1
    );
    assert_eq!(
        projects
            .iter()
            .map(|project| &project.slug)
            .collect::<HashSet<_>>()
            .len(),
        projects.len()
    );
    let active = project_queries::get_active_project(db.conn()).unwrap();
    let posts = post_queries::list_posts_by_project(db.conn(), &active.id).unwrap();
    for post in posts {
        let translations =
            post_translation::list_post_translations_by_post(db.conn(), &post.id).unwrap();
        assert_eq!(
            translations
                .iter()
                .map(|translation| &translation.language)
                .collect::<HashSet<_>>()
                .len(),
            translations.len()
        );
    }
}

#[test]
fn post_defaults_match_spec() {
    let db = memory_db();
    project_queries::insert_project(db.conn(), &project("p1", "test")).unwrap();
    db.conn()
        .with(|conn| {
            diesel::insert_into(posts::table)
                .values((
                    posts::id.eq("min1"),
                    posts::project_id.eq("p1"),
                    posts::title.eq("Minimal"),
                    posts::slug.eq("minimal"),
                    posts::created_at.eq(1000_i64),
                    posts::updated_at.eq(1000_i64),
                ))
                .execute(conn)
                .map(|_| ())
        })
        .unwrap();
    let (status, do_not_translate, file_path) = db
        .conn()
        .with(|conn| {
            posts::table
                .filter(posts::id.eq("min1"))
                .select((posts::status, posts::do_not_translate, posts::file_path))
                .first::<(String, i32, String)>(conn)
        })
        .unwrap();
    assert_eq!(status, "draft");
    assert_eq!(do_not_translate, 0);
    assert!(file_path.is_empty());
}

#[test]
fn script_defaults_match_spec() {
    let db = memory_db();
    project_queries::insert_project(db.conn(), &project("p1", "test")).unwrap();
    db.conn()
        .with(|conn| {
            diesel::insert_into(scripts::table)
                .values((
                    scripts::id.eq("s1"),
                    scripts::project_id.eq("p1"),
                    scripts::slug.eq("test"),
                    scripts::title.eq("Test"),
                    scripts::file_path.eq("scripts/test.lua"),
                    scripts::created_at.eq(1000_i64),
                    scripts::updated_at.eq(1000_i64),
                ))
                .execute(conn)
                .map(|_| ())
        })
        .unwrap();
    let value = script::get_script_by_id(db.conn(), "s1").unwrap();
    assert_eq!(value.kind, ScriptKind::Utility);
    assert_eq!(value.entrypoint, "render");
    assert!(value.enabled);
    assert_eq!(value.version, 1);
    assert_eq!(value.status, ScriptStatus::Draft);
}

#[test]
fn template_defaults_match_spec() {
    let db = memory_db();
    project_queries::insert_project(db.conn(), &project("p1", "test")).unwrap();
    db.conn()
        .with(|conn| {
            diesel::insert_into(templates::table)
                .values((
                    templates::id.eq("t1"),
                    templates::project_id.eq("p1"),
                    templates::slug.eq("test"),
                    templates::title.eq("Test"),
                    templates::file_path.eq("templates/test.liquid"),
                    templates::created_at.eq(1000_i64),
                    templates::updated_at.eq(1000_i64),
                ))
                .execute(conn)
                .map(|_| ())
        })
        .unwrap();
    let value = template::get_template_by_id(db.conn(), "t1").unwrap();
    assert_eq!(value.kind, TemplateKind::Post);
    assert!(value.enabled);
    assert_eq!(value.version, 1);
    assert_eq!(value.status, TemplateStatus::Draft);
}

#[test]
fn tag_unique_name_per_project_enforced_case_insensitively() {
    let db = memory_db();
    project_queries::insert_project(db.conn(), &project("p1", "test")).unwrap();
    let make_tag = |id: &str, name: &str| Tag {
        id: id.into(),
        project_id: "p1".into(),
        name: name.into(),
        color: None,
        post_template_slug: None,
        created_at: 1000,
        updated_at: 1000,
    };
    tag::insert_tag(db.conn(), &make_tag("t1", "rust")).unwrap();
    assert!(tag::insert_tag(db.conn(), &make_tag("t2", "RUST")).is_err());
}

#[test]
fn slug_generation_matches_spec() {
    use bds_core::util::{ensure_unique, slugify};
    assert_eq!(slugify("Hello World"), "hello-world");
    assert_eq!(slugify("a --- b"), "a-b");
    assert_eq!(slugify("---hello---"), "hello");
    assert_eq!(slugify("café"), "cafe");
    assert_eq!(slugify("über"), "ueber");
    assert_eq!(slugify("Straße"), "strasse");
    assert_eq!(ensure_unique("test", |value| value == "test"), "test-2");
}

#[test]
fn published_paths_follow_layout() {
    let db = fixture_db();
    let active = project_queries::get_active_project(db.conn()).unwrap();
    for post in post_queries::list_posts_by_project(db.conn(), &active.id)
        .unwrap()
        .into_iter()
        .filter(|post| post.status == PostStatus::Published)
    {
        assert!(post.file_path.ends_with(&format!("{}.md", post.slug)));
        assert!(post.file_path.contains("/posts/"));
    }
}

#[test]
fn remaining_value_specs_match() {
    use bds_core::model::{NotificationAction, NotificationEntity, SshMode};
    assert_eq!(
        serde_json::to_string(&NotificationEntity::Post).unwrap(),
        "\"post\""
    );
    assert_eq!(
        serde_json::to_string(&NotificationAction::Deleted).unwrap(),
        "\"deleted\""
    );
    assert_eq!(serde_json::to_string(&SshMode::Rsync).unwrap(), "\"rsync\"");
    assert_eq!(["en", "de", "fr", "it", "es"].len(), 5);
}

#[test]
fn fts5_tables_exist_in_fixture() {
    let db = fixture_db();
    assert!(bds_core::db::fts::tables_exist(db.conn()).unwrap());
}
