use std::collections::HashMap;

use serde::Serialize;

use bds_core::render::render_liquid_template;

#[derive(Debug, Serialize)]
struct SinglePostContext {
    language: String,
    language_prefix: String,
    page_title: String,
    pico_stylesheet_href: Option<String>,
    html_theme_attribute: Option<String>,
    alternate_links: Vec<serde_json::Value>,
    blog_languages: Vec<serde_json::Value>,
    menu_items: Vec<serde_json::Value>,
    calendar_initial_year: i32,
    calendar_initial_month: i32,
    post: serde_json::Value,
    post_categories: Vec<String>,
    post_tags: Vec<String>,
    tag_color_by_name: HashMap<String, String>,
    backlinks: Vec<serde_json::Value>,
    canonical_post_path_by_slug: HashMap<String, String>,
    canonical_media_path_by_source_path: HashMap<String, String>,
    post_data_json_by_id: HashMap<String, String>,
}

#[test]
fn i18n_filter_renders_partial_content_language() {
    let template = "{% render 'partials/label', label: 'render.archive', language: language %}";
    let partials = HashMap::from([(
        "partials/label".to_string(),
        "{{ label | i18n: language }}".to_string(),
    )]);
    let context = serde_json::json!({ "language": "de" });

    let rendered = render_liquid_template(template, &partials, &context).unwrap();
    assert_eq!(rendered, "Archiv");
}

#[test]
fn markdown_filter_rewrites_post_and_media_urls() {
    let template = "{{ body | markdown: nil, nil, canonical_post_path_by_slug, canonical_media_path_by_source_path, language, language_prefix }}";
    let partials = HashMap::new();
    let context = serde_json::json!({
        "body": "[Post](/posts/hello) ![Img](/media/2024/03/pic.png)",
        "canonical_post_path_by_slug": {"hello": "/2024/03/09/hello"},
        "canonical_media_path_by_source_path": {"media/2024/03/pic.png": "/assets/pic.png"},
        "language": "en",
        "language_prefix": ""
    });

    let rendered = render_liquid_template(template, &partials, &context).unwrap();
    assert!(rendered.contains("href=\"/2024/03/09/hello\""));
    assert!(rendered.contains("src=\"/assets/pic.png\""));
}

#[test]
fn markdown_filter_expands_builtin_macros_from_runtime_context() {
    let template = "{{ body | markdown: post.id, post_data_json_by_id, canonical_post_path_by_slug, canonical_media_path_by_source_path, language, language_prefix }}";
    let partials = HashMap::new();
    let context = serde_json::json!({
        "body": "[[gallery images=post.linked_media columns=2]]\n\n[[youtube id=dQw4w9WgXcQ]]\n\n[[vimeo id=123456]]\n\n[[photo_archive media=post.linked_media]]\n\n[[tag_cloud tags=post_tags]]",
        "post": {
            "id": "post-1",
            "linked_media": [
                {"file_path": "/media/2026/04/one.jpg", "title": "One", "alt": "Image one"},
                {"file_path": "/media/2026/04/two.jpg", "caption": "Two"}
            ]
        },
        "post_data_json_by_id": {
            "post-1": {"id": "post-1", "title": "Post 1"}
        },
        "post_tags": [
            {"name": "Rust", "slug": "rust", "post_count": 4, "color": "#ff6600"},
            {"name": "Iced", "slug": "iced", "post_count": 2}
        ],
        "tag_color_by_name": {"Iced": "#0088cc"},
        "canonical_post_path_by_slug": {},
        "canonical_media_path_by_source_path": {},
        "language": "en",
        "language_prefix": ""
    });

    let rendered = render_liquid_template(template, &partials, &context).unwrap();
    assert!(rendered.contains("macro-gallery gallery-cols-2"));
    assert!(rendered.contains("https://www.youtube.com/embed/dQw4w9WgXcQ"));
    assert!(rendered.contains("https://player.vimeo.com/video/123456"));
    assert!(rendered.contains("macro-photo-archive"));
    assert!(rendered.contains("data-tag-cloud=\"true\""));
}

#[test]
fn markdown_filter_uses_project_macro_template_overrides() {
    let template = "{{ body | markdown: nil, nil, canonical_post_path_by_slug, canonical_media_path_by_source_path, language, language_prefix }}";
    let context = serde_json::json!({
        "body": "[[youtube id=custom-id]]",
        "macro_templates": {
            "youtube": "<figure data-video=\"{{ id | escape }}\">custom</figure>"
        },
        "canonical_post_path_by_slug": {},
        "canonical_media_path_by_source_path": {},
        "language": "en",
        "language_prefix": ""
    });

    let rendered = render_liquid_template(template, &HashMap::new(), &context).unwrap();

    assert!(rendered.contains("<figure data-video=\"custom-id\">custom</figure>"));
    assert!(!rendered.contains("macro-youtube"));
}

#[test]
fn markdown_filter_leaves_unknown_macros_verbatim() {
    let template = "{{ body | markdown: nil, nil, canonical_post_path_by_slug, canonical_media_path_by_source_path, language, language_prefix }}";
    let partials = HashMap::new();
    let context = serde_json::json!({
        "body": "[[custom_block foo=bar]]",
        "canonical_post_path_by_slug": {},
        "canonical_media_path_by_source_path": {},
        "language": "en",
        "language_prefix": ""
    });

    let rendered = render_liquid_template(template, &partials, &context).unwrap();
    assert!(rendered.contains("[[custom_block foo=bar]]"));
    assert!(!rendered.contains("macro-unsupported"));
}

#[test]
fn starter_single_post_template_renders_with_partials() {
    let template = include_str!("../../../assets/starter-templates/single-post.liquid");
    let partials = HashMap::from([
        (
            "partials/head".to_string(),
            include_str!("../../../assets/starter-templates/partials/head.liquid").to_string(),
        ),
        (
            "partials/menu".to_string(),
            include_str!("../../../assets/starter-templates/partials/menu.liquid").to_string(),
        ),
        (
            "partials/language-switcher".to_string(),
            include_str!("../../../assets/starter-templates/partials/language-switcher.liquid")
                .to_string(),
        ),
        (
            "partials/menu-items".to_string(),
            "{% for item in items %}<a href=\"{{ item.href }}\">{{ item.title }}</a>{% endfor %}"
                .to_string(),
        ),
    ]);

    let context = SinglePostContext {
        language: "en".into(),
        language_prefix: String::new(),
        page_title: "Hello".into(),
        pico_stylesheet_href: None,
        html_theme_attribute: None,
        alternate_links: vec![],
        blog_languages: vec![serde_json::json!({
            "is_current": true,
            "code": "en",
            "flag": "GB",
            "href": "/",
            "href_prefix": ""
        })],
        menu_items: vec![],
        calendar_initial_year: 2024,
        calendar_initial_month: 3,
        post: serde_json::json!({
            "id": "post-1",
            "title": "Hello",
            "content": "A **world** post with [link](/posts/hello).",
        }),
        post_categories: vec![],
        post_tags: vec![],
        tag_color_by_name: HashMap::new(),
        backlinks: vec![],
        canonical_post_path_by_slug: HashMap::from([("hello".into(), "/2024/03/09/hello".into())]),
        canonical_media_path_by_source_path: HashMap::new(),
        post_data_json_by_id: HashMap::new(),
    };

    let rendered = render_liquid_template(template, &partials, &context).unwrap();
    assert!(rendered.contains("<h1>Hello</h1>"));
    assert!(rendered.contains("<strong>world</strong>"));
    assert!(rendered.contains("href=\"/2024/03/09/hello\""));
    assert!(rendered.contains("data-pagefind-body"));
}
