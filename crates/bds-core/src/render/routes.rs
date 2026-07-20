use std::collections::HashMap;

use chrono::{Datelike, Local, TimeZone};
use serde::Serialize;

use crate::i18n::normalize_language;
use crate::model::{Post, ProjectMetadata};
use crate::render::{RenderError, render_liquid_template};

const STARTER_SINGLE_POST_TEMPLATE: &str =
    include_str!("../../../../assets/starter-templates/single-post.liquid");
const STARTER_POST_LIST_TEMPLATE: &str =
    include_str!("../../../../assets/starter-templates/post-list.liquid");
const STARTER_HEAD_PARTIAL: &str =
    include_str!("../../../../assets/starter-templates/partials/head.liquid");
const STARTER_MENU_PARTIAL: &str =
    include_str!("../../../../assets/starter-templates/partials/menu.liquid");
const STARTER_LANGUAGE_SWITCHER_PARTIAL: &str =
    include_str!("../../../../assets/starter-templates/partials/language-switcher.liquid");
const STARTER_MENU_ITEMS_PARTIAL: &str =
    include_str!("../../../../assets/starter-templates/partials/menu-items.liquid");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PostLanguageVariant {
    Base,
    Translation,
}

pub(crate) fn select_post_language_variant(
    post: &Post,
    target_language: &str,
    main_language: &str,
    has_translation: bool,
) -> Option<PostLanguageVariant> {
    let source_language = post.language.as_deref().unwrap_or(main_language);
    if target_language.eq_ignore_ascii_case(main_language) {
        return Some(if has_translation {
            PostLanguageVariant::Translation
        } else {
            PostLanguageVariant::Base
        });
    }
    if post.do_not_translate {
        return None;
    }
    if source_language.eq_ignore_ascii_case(target_language) {
        Some(PostLanguageVariant::Base)
    } else if has_translation {
        Some(PostLanguageVariant::Translation)
    } else {
        Some(PostLanguageVariant::Base)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPage {
    pub relative_path: String,
    pub html: String,
}

#[derive(Debug, Clone, Serialize)]
struct AlternateLinkContext {
    href: String,
    hreflang: String,
}

#[derive(Debug, Clone, Serialize)]
struct BlogLanguageContext {
    is_current: bool,
    code: String,
    flag: String,
    href: String,
    href_prefix: String,
}

#[derive(Debug, Clone, Serialize)]
struct DayBlockContext {
    show_date_marker: bool,
    date_label: String,
    posts: Vec<serde_json::Value>,
    show_separator: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ListTemplateContext {
    language: String,
    language_prefix: String,
    page_title: String,
    pico_stylesheet_href: Option<String>,
    html_theme_attribute: Option<String>,
    alternate_links: Vec<AlternateLinkContext>,
    blog_languages: Vec<BlogLanguageContext>,
    menu_items: Vec<serde_json::Value>,
    calendar_initial_year: i32,
    calendar_initial_month: u32,
    archive_context: Option<serde_json::Value>,
    show_archive_range_heading: bool,
    min_date: Option<serde_json::Value>,
    max_date: Option<serde_json::Value>,
    day_blocks: Vec<DayBlockContext>,
    is_list_page: bool,
    is_first_page: bool,
    is_last_page: bool,
    has_prev_page: bool,
    has_next_page: bool,
    prev_page_href: Option<String>,
    next_page_href: Option<String>,
    canonical_post_path_by_slug: HashMap<String, String>,
    canonical_media_path_by_source_path: HashMap<String, String>,
    post_data_json_by_id: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
struct PostTemplateContext<'a> {
    language: &'a str,
    language_prefix: String,
    page_title: &'a str,
    pico_stylesheet_href: Option<String>,
    html_theme_attribute: Option<String>,
    alternate_links: Vec<AlternateLinkContext>,
    blog_languages: Vec<BlogLanguageContext>,
    menu_items: Vec<serde_json::Value>,
    calendar_initial_year: i32,
    calendar_initial_month: u32,
    post: serde_json::Value,
    post_categories: Vec<String>,
    post_tags: Vec<String>,
    tag_color_by_name: HashMap<String, String>,
    backlinks: Vec<serde_json::Value>,
    canonical_post_path_by_slug: HashMap<String, String>,
    canonical_media_path_by_source_path: HashMap<String, String>,
    post_data_json_by_id: HashMap<String, String>,
}

pub fn build_canonical_post_path(post: &Post, language: &str, main_language: &str) -> String {
    let Some(timestamp) = Local.timestamp_millis_opt(post.created_at).single() else {
        return fallback_language_path(post, language, main_language);
    };

    let base = format!(
        "/{:04}/{:02}/{:02}/{}",
        timestamp.year(),
        timestamp.month(),
        timestamp.day(),
        post.slug
    );

    if language.eq_ignore_ascii_case(main_language) {
        base
    } else {
        format!("/{language}{base}")
    }
}

pub fn render_starter_single_post_page(
    post: &Post,
    body_markdown: &str,
    metadata: &ProjectMetadata,
    language: &str,
) -> Result<RenderedPage, RenderError> {
    render_starter_single_post_page_with_media_map(
        post,
        body_markdown,
        metadata,
        language,
        HashMap::new(),
    )
}

pub fn render_starter_single_post_page_with_media_map(
    post: &Post,
    body_markdown: &str,
    metadata: &ProjectMetadata,
    language: &str,
    canonical_media_path_by_source_path: HashMap<String, String>,
) -> Result<RenderedPage, RenderError> {
    let relative_path = format!(
        "{}/index.html",
        build_canonical_post_path(post, language, main_language(metadata)).trim_start_matches('/')
    );
    let canonical_path = build_canonical_post_path(post, language, main_language(metadata));
    let (calendar_initial_year, calendar_initial_month) = calendar_initial_parts(post);
    let context = PostTemplateContext {
        language,
        language_prefix: language_prefix(language, main_language(metadata)),
        page_title: &post.title,
        pico_stylesheet_href: Some(crate::model::pico_stylesheet_href(
            metadata.pico_theme.as_deref(),
        )),
        html_theme_attribute: None,
        alternate_links: build_alternate_links(post, metadata, language),
        blog_languages: build_blog_languages(post, metadata, language),
        menu_items: vec![],
        calendar_initial_year,
        calendar_initial_month,
        post: serde_json::json!({
            "id": post.id,
            "title": post.title,
            "content": body_markdown,
        }),
        post_categories: post.categories.clone(),
        post_tags: post.tags.clone(),
        tag_color_by_name: post
            .tags
            .iter()
            .map(|tag| (tag.clone(), String::new()))
            .collect(),
        backlinks: vec![],
        canonical_post_path_by_slug: HashMap::from([(post.slug.clone(), canonical_path)]),
        canonical_media_path_by_source_path,
        post_data_json_by_id: HashMap::new(),
    };

    let html = render_liquid_template(STARTER_SINGLE_POST_TEMPLATE, &starter_partials(), &context)?;

    Ok(RenderedPage {
        relative_path,
        html,
    })
}

pub fn render_starter_list_page(
    posts: &[(Post, String)],
    metadata: &ProjectMetadata,
    language: &str,
) -> Result<RenderedPage, RenderError> {
    render_starter_list_page_with_media_map(posts, metadata, language, HashMap::new())
}

pub fn render_starter_list_page_with_media_map(
    posts: &[(Post, String)],
    metadata: &ProjectMetadata,
    language: &str,
    canonical_media_path_by_source_path: HashMap<String, String>,
) -> Result<RenderedPage, RenderError> {
    let relative_path = if language.eq_ignore_ascii_case(main_language(metadata)) {
        "index.html".to_string()
    } else {
        format!("{language}/index.html")
    };

    let canonical_paths = posts
        .iter()
        .map(|(post, _)| {
            (
                post.slug.clone(),
                build_canonical_post_path(post, language, main_language(metadata)),
            )
        })
        .collect::<HashMap<_, _>>();

    let (calendar_initial_year, calendar_initial_month) = posts
        .first()
        .map(|(post, _)| calendar_initial_parts(post))
        .unwrap_or((1970, 1));

    let context = ListTemplateContext {
        language: language.to_string(),
        language_prefix: language_prefix(language, main_language(metadata)),
        page_title: metadata.name.clone(),
        pico_stylesheet_href: Some(crate::model::pico_stylesheet_href(
            metadata.pico_theme.as_deref(),
        )),
        html_theme_attribute: None,
        alternate_links: vec![],
        blog_languages: build_blog_languages_for_index(metadata, language),
        menu_items: vec![],
        calendar_initial_year,
        calendar_initial_month,
        archive_context: None,
        show_archive_range_heading: false,
        min_date: None,
        max_date: None,
        day_blocks: build_day_blocks(posts),
        is_list_page: false,
        is_first_page: true,
        is_last_page: true,
        has_prev_page: false,
        has_next_page: false,
        prev_page_href: None,
        next_page_href: None,
        canonical_post_path_by_slug: canonical_paths,
        canonical_media_path_by_source_path,
        post_data_json_by_id: HashMap::new(),
    };

    let html =
        render_liquid_template(&starter_post_list_template(), &starter_partials(), &context)?;

    Ok(RenderedPage {
        relative_path,
        html,
    })
}

fn starter_partials() -> HashMap<String, String> {
    HashMap::from([
        (
            "partials/head".to_string(),
            STARTER_HEAD_PARTIAL.to_string(),
        ),
        (
            "partials/menu".to_string(),
            STARTER_MENU_PARTIAL.to_string(),
        ),
        (
            "partials/language-switcher".to_string(),
            STARTER_LANGUAGE_SWITCHER_PARTIAL.to_string(),
        ),
        (
            "partials/menu-items".to_string(),
            STARTER_MENU_ITEMS_PARTIAL.to_string(),
        ),
    ])
}

fn starter_post_list_template() -> String {
    STARTER_POST_LIST_TEMPLATE.replace(
        "{% render 'partials/head', page_title: page_title, pico_stylesheet_href: pico_stylesheet_href, language_prefix: language_prefix %}",
        "{% render 'partials/head', page_title: page_title, pico_stylesheet_href: pico_stylesheet_href, language_prefix: language_prefix, alternate_links: alternate_links %}",
    )
}

fn main_language(metadata: &ProjectMetadata) -> &str {
    metadata.main_language.as_deref().unwrap_or("en")
}

fn language_prefix(language: &str, main_language: &str) -> String {
    if language.eq_ignore_ascii_case(main_language) {
        String::new()
    } else {
        format!("/{language}")
    }
}

fn fallback_language_path(post: &Post, language: &str, main_language: &str) -> String {
    if language.eq_ignore_ascii_case(main_language) {
        format!("/posts/{}", post.slug)
    } else {
        format!("/{language}/posts/{}", post.slug)
    }
}

fn calendar_initial_parts(post: &Post) -> (i32, u32) {
    Local
        .timestamp_millis_opt(post.created_at)
        .single()
        .map(|timestamp| (timestamp.year(), timestamp.month()))
        .unwrap_or((1970, 1))
}

fn build_alternate_links(
    post: &Post,
    metadata: &ProjectMetadata,
    current_language: &str,
) -> Vec<AlternateLinkContext> {
    metadata
        .blog_languages
        .iter()
        .map(|language| AlternateLinkContext {
            href: build_absolute_post_url(post, metadata, language),
            hreflang: language.clone(),
        })
        .chain(std::iter::once(AlternateLinkContext {
            href: build_absolute_post_url(post, metadata, current_language),
            hreflang: "x-default".to_string(),
        }))
        .collect()
}

fn build_blog_languages(
    post: &Post,
    metadata: &ProjectMetadata,
    current_language: &str,
) -> Vec<BlogLanguageContext> {
    metadata
        .blog_languages
        .iter()
        .map(|language| BlogLanguageContext {
            is_current: language.eq_ignore_ascii_case(current_language),
            code: language.clone(),
            flag: render_flag(language),
            href: build_absolute_post_url(post, metadata, language),
            href_prefix: language_prefix(language, main_language(metadata)),
        })
        .collect()
}

fn build_blog_languages_for_index(
    metadata: &ProjectMetadata,
    current_language: &str,
) -> Vec<BlogLanguageContext> {
    metadata
        .blog_languages
        .iter()
        .map(|language| BlogLanguageContext {
            is_current: language.eq_ignore_ascii_case(current_language),
            code: language.clone(),
            flag: render_flag(language),
            href: build_absolute_index_url(metadata, language),
            href_prefix: language_prefix(language, main_language(metadata)),
        })
        .collect()
}

fn build_absolute_post_url(post: &Post, metadata: &ProjectMetadata, language: &str) -> String {
    let base_url = metadata
        .public_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    format!(
        "{base_url}{}",
        build_canonical_post_path(post, language, main_language(metadata))
    )
}

fn build_absolute_index_url(metadata: &ProjectMetadata, language: &str) -> String {
    let base_url = metadata
        .public_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let suffix = if language.eq_ignore_ascii_case(main_language(metadata)) {
        "/".to_string()
    } else {
        format!("/{language}/")
    };
    format!("{base_url}{suffix}")
}

fn render_flag(language: &str) -> String {
    normalize_language(language).flag_emoji().to_string()
}

fn build_day_blocks(posts: &[(Post, String)]) -> Vec<DayBlockContext> {
    let mut blocks: Vec<DayBlockContext> = Vec::new();
    let mut current_key: Option<String> = None;

    for (post, body) in posts {
        let Some(timestamp) = Local.timestamp_millis_opt(post.created_at).single() else {
            continue;
        };

        let key = format!(
            "{:04}-{:02}-{:02}",
            timestamp.year(),
            timestamp.month(),
            timestamp.day()
        );
        if current_key.as_deref() != Some(key.as_str()) {
            if let Some(last) = blocks.last_mut() {
                last.show_separator = true;
            }
            current_key = Some(key);
            blocks.push(DayBlockContext {
                show_date_marker: true,
                date_label: format!(
                    "{:04}-{:02}-{:02}",
                    timestamp.year(),
                    timestamp.month(),
                    timestamp.day()
                ),
                posts: Vec::new(),
                show_separator: false,
            });
        }

        if let Some(block) = blocks.last_mut() {
            block.posts.push(serde_json::json!({
                "id": post.id,
                "slug": post.slug,
                "title": post.title,
                "content": post.excerpt.clone().unwrap_or_else(|| body.clone()),
                "show_title": true,
            }));
        }
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PostStatus;

    fn make_post() -> Post {
        Post {
            id: "post-1".into(),
            project_id: "project-1".into(),
            title: "Hello".into(),
            slug: "hello".into(),
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
            created_at: 1_710_000_000_000,
            updated_at: 1_710_000_000_000,
            published_at: Some(1_710_000_000_000),
        }
    }

    fn make_metadata() -> ProjectMetadata {
        ProjectMetadata {
            name: "Blog".into(),
            description: None,
            public_url: Some("https://example.com".into()),
            main_language: Some("en".into()),
            default_author: None,
            max_posts_per_page: 50,
            image_import_concurrency: 4,
            blogmark_category: None,
            pico_theme: None,
            semantic_similarity_enabled: false,
            blog_languages: vec!["en".into(), "de".into()],
        }
    }

    #[test]
    fn canonical_post_paths_follow_language_prefix_rule() {
        let mut post = make_post();
        post.published_at = Some(1_712_678_400_000);
        assert_eq!(
            build_canonical_post_path(&post, "en", "en"),
            "/2024/03/09/hello"
        );
        assert_eq!(
            build_canonical_post_path(&post, "de", "en"),
            "/de/2024/03/09/hello"
        );
    }

    #[test]
    fn canonical_post_paths_use_the_bds2_local_created_date() {
        let mut post = make_post();
        post.created_at = 1_711_927_800_000;
        let local = Local
            .timestamp_millis_opt(post.created_at)
            .single()
            .unwrap();

        assert_eq!(
            build_canonical_post_path(&post, "en", "en"),
            format!(
                "/{:04}/{:02}/{:02}/hello",
                local.year(),
                local.month(),
                local.day()
            )
        );
    }

    #[test]
    fn language_variants_match_bds2_canonical_and_fallback_rules() {
        let mut post = make_post();
        post.language = Some("en".into());

        assert_eq!(
            select_post_language_variant(&post, "de", "de", true),
            Some(PostLanguageVariant::Translation)
        );
        assert_eq!(
            select_post_language_variant(&post, "en", "de", false),
            Some(PostLanguageVariant::Base)
        );
        assert_eq!(
            select_post_language_variant(&post, "fr", "de", true),
            Some(PostLanguageVariant::Translation)
        );
        assert_eq!(
            select_post_language_variant(&post, "fr", "de", false),
            Some(PostLanguageVariant::Base)
        );

        post.do_not_translate = true;
        assert_eq!(select_post_language_variant(&post, "fr", "de", true), None);
    }

    #[test]
    fn starter_single_post_renderer_uses_canonical_route_and_language_links() {
        let post = make_post();
        let metadata = make_metadata();
        let rendered = render_starter_single_post_page(
            &post,
            "Body with [link](/posts/hello)",
            &metadata,
            "en",
        )
        .unwrap();

        assert_eq!(rendered.relative_path, "2024/03/09/hello/index.html");
        assert!(
            rendered
                .html
                .contains("https://example.com/2024/03/09/hello")
        );
        assert!(
            rendered
                .html
                .contains("https://example.com/de/2024/03/09/hello")
        );
        assert!(rendered.html.contains("href=\"/2024/03/09/hello\""));
    }

    #[test]
    fn starter_renderer_uses_the_selected_pico_theme_stylesheet() {
        let post = make_post();
        let mut metadata = make_metadata();
        metadata.pico_theme = Some("amber".into());

        let rendered = render_starter_single_post_page(&post, "Body", &metadata, "en").unwrap();

        assert!(
            rendered
                .html
                .contains("href=\"/assets/pico.amber.min.css\"")
        );
    }

    #[test]
    fn starter_single_post_renderer_rewrites_bds_media_image_links() {
        let post = make_post();
        let metadata = make_metadata();
        let rendered = render_starter_single_post_page_with_media_map(
            &post,
            "![](bds-media://media-1)",
            &metadata,
            "en",
            HashMap::from([(
                "bds-media://media-1".to_string(),
                "/media/2026/04/media-1.png".to_string(),
            )]),
        )
        .unwrap();

        assert!(rendered.html.contains("src=\"/media/2026/04/media-1.png\""));
    }

    #[test]
    fn starter_list_renderer_groups_posts_and_uses_language_specific_index_path() {
        let metadata = make_metadata();
        let first = make_post();
        let mut second = make_post();
        second.id = "post-2".into();
        second.slug = "next".into();
        second.title = "Next".into();
        second.published_at = Some(1_710_086_400_000);
        second.created_at = 1_710_086_400_000;

        let rendered = render_starter_list_page(
            &[(first, "First body".into()), (second, "Second body".into())],
            &metadata,
            "de",
        )
        .unwrap();

        assert_eq!(rendered.relative_path, "de/index.html");
        assert!(rendered.html.contains("archive-day-group"));
        assert!(rendered.html.contains("2024-03-09"));
        assert!(rendered.html.contains("2024-03-10"));
        assert!(rendered.html.contains("href=\"/de/2024/03/10/next\""));
    }
}
