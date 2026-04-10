use std::collections::HashMap;

use bds_core::model::{Post, PostStatus, Tag, Template, TemplateKind, TemplateStatus};
use bds_core::render::{
    RenderCategorySettings, RenderTemplateLookup, TemplateLookupError,
    render_markdown_to_html, resolve_post_template,
};

fn make_post() -> Post {
    Post {
        id: "post-1".into(),
        project_id: "project-1".into(),
        title: "Post".into(),
        slug: "post".into(),
        excerpt: None,
        content: Some("# Hello".into()),
        status: PostStatus::Published,
        author: None,
        language: Some("en".into()),
        do_not_translate: false,
        template_slug: None,
        file_path: "posts/2026/04/10/post.md".into(),
        checksum: None,
        tags: vec![],
        categories: vec![],
        published_title: None,
        published_content: None,
        published_tags: None,
        published_categories: None,
        published_excerpt: None,
        created_at: 1,
        updated_at: 1,
        published_at: Some(1),
    }
}

fn make_template(slug: &str) -> Template {
    Template {
        id: format!("template-{slug}"),
        project_id: "project-1".into(),
        slug: slug.into(),
        title: slug.into(),
        kind: TemplateKind::Post,
        enabled: true,
        version: 1,
        file_path: format!("templates/{slug}.liquid"),
        status: TemplateStatus::Published,
        content: Some(format!("template:{slug}")),
        created_at: 1,
        updated_at: 1,
    }
}

fn make_tag(name: &str, post_template_slug: Option<&str>) -> Tag {
    Tag {
        id: format!("tag-{name}"),
        project_id: "project-1".into(),
        name: name.into(),
        color: None,
        post_template_slug: post_template_slug.map(ToOwned::to_owned),
        created_at: 1,
        updated_at: 1,
    }
}

#[test]
fn template_lookup_prefers_post_specific_template() {
    let mut post = make_post();
    post.template_slug = Some("custom".into());
    post.tags = vec!["rust".into()];
    post.categories = vec!["article".into()];

    let templates = vec![make_template("post"), make_template("tag-template"), make_template("custom")];
    let tags = vec![make_tag("rust", Some("tag-template"))];
    let mut categories = HashMap::new();
    categories.insert(
        "article".into(),
        RenderCategorySettings {
            post_template_slug: Some("category-template".into()),
        },
    );

    let resolved = resolve_post_template(RenderTemplateLookup {
        post: &post,
        templates: &templates,
        tags: &tags,
        category_settings: &categories,
    })
    .unwrap();

    assert_eq!(resolved.slug, "custom");
}

#[test]
fn template_lookup_falls_back_to_tag_then_category_then_default() {
    let mut post = make_post();
    post.tags = vec!["rust".into()];
    post.categories = vec!["article".into()];

    let templates = vec![
        make_template("post"),
        make_template("tag-template"),
        make_template("category-template"),
    ];
    let tags = vec![make_tag("rust", Some("tag-template"))];
    let mut categories = HashMap::new();
    categories.insert(
        "article".into(),
        RenderCategorySettings {
            post_template_slug: Some("category-template".into()),
        },
    );

    let resolved = resolve_post_template(RenderTemplateLookup {
        post: &post,
        templates: &templates,
        tags: &tags,
        category_settings: &categories,
    })
    .unwrap();
    assert_eq!(resolved.slug, "tag-template");

    let category_post = Post {
        tags: vec![],
        ..post.clone()
    };
    let resolved = resolve_post_template(RenderTemplateLookup {
        post: &category_post,
        templates: &templates,
        tags: &tags,
        category_settings: &categories,
    })
    .unwrap();
    assert_eq!(resolved.slug, "category-template");

    let default_post = Post {
        tags: vec![],
        categories: vec![],
        ..post
    };
    let empty_categories = HashMap::new();
    let resolved = resolve_post_template(RenderTemplateLookup {
        post: &default_post,
        templates: &templates,
        tags: &tags,
        category_settings: &empty_categories,
    })
    .unwrap();
    assert_eq!(resolved.slug, "post");
}

#[test]
fn template_lookup_errors_when_explicit_template_missing() {
    let mut post = make_post();
    post.template_slug = Some("missing".into());
    let templates = vec![make_template("post")];

    let err = resolve_post_template(RenderTemplateLookup {
        post: &post,
        templates: &templates,
        tags: &[],
        category_settings: &HashMap::new(),
    })
    .unwrap_err();

    assert_eq!(err, TemplateLookupError::MissingExplicitTemplate("missing".into()));
}

#[test]
fn markdown_render_produces_html() {
    let html = render_markdown_to_html("# Hello\n\nA paragraph with **bold** text.");
    assert!(html.contains("<h1>Hello</h1>"));
    assert!(html.contains("<strong>bold</strong>"));
}