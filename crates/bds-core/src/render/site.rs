use std::collections::{BTreeMap, HashMap};
use std::error::Error;
use std::fs;
use std::path::Path;

use chrono::{Datelike, TimeZone, Utc};
use rayon::prelude::*;
use rusqlite::Connection;
use serde_json::{Value, json};

use crate::db::queries;
use crate::engine::menu::{self, MenuItemKind};
use crate::model::{CategorySettings, Media, Post, ProjectMetadata, Tag, Template, TemplateKind, TemplateStatus};
use crate::render::{RenderCategorySettings, RenderTemplateLookup, build_canonical_post_path, render_liquid_template, resolve_post_template};
use crate::util::frontmatter::{read_template_file, read_translation_file};
use crate::util::slugify;

const STARTER_SINGLE_POST_TEMPLATE: &str = include_str!("../../../../assets/starter-templates/single-post.liquid");
const STARTER_POST_LIST_TEMPLATE: &str = include_str!("../../../../assets/starter-templates/post-list.liquid");
const STARTER_NOT_FOUND_TEMPLATE: &str = include_str!("../../../../assets/starter-templates/not-found.liquid");
const STARTER_HEAD_PARTIAL: &str = include_str!("../../../../assets/starter-templates/partials/head.liquid");
const STARTER_MENU_PARTIAL: &str = include_str!("../../../../assets/starter-templates/partials/menu.liquid");
const STARTER_LANGUAGE_SWITCHER_PARTIAL: &str = include_str!("../../../../assets/starter-templates/partials/language-switcher.liquid");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitePage {
    pub language: String,
    pub relative_path: String,
    pub url_path: String,
    pub html: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagefindDocument {
    pub language: String,
    pub relative_path: String,
    pub url_path: String,
    pub html: String,
}

#[derive(Debug, Clone, Default)]
pub struct SiteRenderArtifacts {
    pub pages: Vec<SitePage>,
    pub pagefind_documents: Vec<PagefindDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRenderResult {
    pub status_code: u16,
    pub html: String,
}

#[derive(Debug, Clone)]
struct TemplateBundle {
    post_templates: Vec<Template>,
    template_source_by_slug: HashMap<String, String>,
    list_template: String,
    not_found_template: String,
    partials: HashMap<String, String>,
}

#[derive(Debug, Clone)]
struct RenderPostRecord {
    post: Post,
    body_markdown: String,
}

#[derive(Debug, Clone)]
struct RouteSpec {
    relative_path: String,
    url_path: String,
    page_title: String,
    archive_context: Option<Value>,
    posts: Vec<RenderPostRecord>,
    current_page: usize,
    total_pages: usize,
    total_items: usize,
    items_per_page: usize,
}

pub fn build_site_render_artifacts(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    published_posts: &[(Post, String)],
) -> Result<SiteRenderArtifacts, Box<dyn Error + Send + Sync>> {
    let bundle = load_template_bundle(conn, data_dir, project_id)?;
    let main_language = main_language(metadata).to_string();
    let languages = render_languages(metadata);
    let tags = queries::tag::list_tags_by_project(conn, project_id).unwrap_or_default();
    let category_settings = queries_category_settings(data_dir)?;
    let media_items = queries::media::list_media_by_project(conn, project_id).unwrap_or_default();
    let media_by_id = media_items
        .into_iter()
        .map(|media| (media.id.clone(), media))
        .collect::<HashMap<_, _>>();

    let mut artifacts = SiteRenderArtifacts::default();
    for language in languages {
        let localized_posts = load_language_posts(conn, data_dir, published_posts, &language, &main_language)?;
        let routes = build_language_routes(&localized_posts, metadata, &language, &tags);
        let post_data_json_by_id = build_post_data_json_by_id(&localized_posts);
        let menu_items = build_menu_items(data_dir, &language, &main_language)?;
        let rendered_list_pages = routes
            .par_iter()
            .map(|route| {
                render_list_route(
                    route,
                    metadata,
                    &language,
                    &localized_posts,
                    &tags,
                    &menu_items,
                    &post_data_json_by_id,
                    &bundle,
                )
                .map(|html| SitePage {
                    language: language.clone(),
                    relative_path: route.relative_path.clone(),
                    url_path: route.url_path.clone(),
                    html,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        for page in rendered_list_pages {
            artifacts.pagefind_documents.push(PagefindDocument {
                language: page.language.clone(),
                relative_path: page.relative_path.clone(),
                url_path: page.url_path.clone(),
                html: page.html.clone(),
            });
            artifacts.pages.push(page);
        }

        let canonical_map = canonical_post_path_by_slug(&localized_posts, &language, &main_language);
        for record in &localized_posts {
            let html = render_post_route(
                conn,
                metadata,
                &language,
                &main_language,
                record,
                &localized_posts,
                &tags,
                &category_settings,
                &media_by_id,
                &canonical_map,
                &menu_items,
                &post_data_json_by_id,
                &bundle,
            )?;
            let canonical_path = build_canonical_post_path(&record.post, &language, &main_language);
            let relative_path = format!("{}/index.html", canonical_path.trim_start_matches('/'));
            artifacts.pagefind_documents.push(PagefindDocument {
                language: language.clone(),
                relative_path: relative_path.clone(),
                url_path: canonical_path.clone(),
                html: html.clone(),
            });
            artifacts.pages.push(SitePage {
                language: language.clone(),
                relative_path,
                url_path: canonical_path,
                html,
            });
        }
    }

    Ok(artifacts)
}

pub fn build_preview_response(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    published_posts: &[(Post, String)],
    requested_path: &str,
) -> Result<PreviewRenderResult, Box<dyn Error + Send + Sync>> {
    let artifacts = build_site_render_artifacts(conn, data_dir, project_id, metadata, published_posts)?;
    let normalized = normalize_request_path(requested_path);
    if let Some(page) = artifacts.pages.iter().find(|page| page.url_path == normalized) {
        return Ok(PreviewRenderResult {
            status_code: 200,
            html: page.html.clone(),
        });
    }

    let bundle = load_template_bundle(conn, data_dir, project_id)?;
    let language = language_from_path(&normalized, metadata);
    let menu_items = build_menu_items(data_dir, &language, main_language(metadata))?;
    let html = render_not_found_route(&bundle, metadata, &language, &normalized, &menu_items)?;
    Ok(PreviewRenderResult {
        status_code: 404,
        html,
    })
}

fn load_template_bundle(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> Result<TemplateBundle, Box<dyn Error + Send + Sync>> {
    let templates = queries::template::list_templates_by_project(conn, project_id).unwrap_or_default();
    let mut template_source_by_slug = HashMap::new();
    let mut post_templates = Vec::new();
    let mut partials = starter_partials();
    let mut list_template = STARTER_POST_LIST_TEMPLATE.to_string();
    let mut not_found_template = STARTER_NOT_FOUND_TEMPLATE.to_string();

    for template in templates {
        if !template.enabled {
            continue;
        }
        let source = load_template_source(data_dir, &template)?;
        match template.kind {
            TemplateKind::Post => {
                template_source_by_slug.insert(template.slug.clone(), source.clone());
                let mut hydrated = template.clone();
                hydrated.content = Some(source);
                post_templates.push(hydrated);
            }
            TemplateKind::List => {
                if template.slug == "list" || list_template == STARTER_POST_LIST_TEMPLATE {
                    list_template = source;
                }
            }
            TemplateKind::NotFound => {
                if template.slug == "not-found" || template.slug == "not_found" || not_found_template == STARTER_NOT_FOUND_TEMPLATE {
                    not_found_template = source;
                }
            }
            TemplateKind::Partial => {
                let key = normalize_partial_slug(&template.slug);
                partials.insert(key.clone(), source.clone());
                if !key.starts_with("partials/") {
                    partials.insert(format!("partials/{key}"), source);
                }
            }
        }
    }

    if !post_templates.iter().any(|template| template.slug == "post") {
        post_templates.push(Template {
            id: "starter-post-template".to_string(),
            project_id: project_id.to_string(),
            slug: "post".to_string(),
            title: "post".to_string(),
            kind: TemplateKind::Post,
            enabled: true,
            version: 1,
            file_path: String::new(),
            status: TemplateStatus::Published,
            content: Some(STARTER_SINGLE_POST_TEMPLATE.to_string()),
            created_at: 0,
            updated_at: 0,
        });
        template_source_by_slug.insert("post".to_string(), STARTER_SINGLE_POST_TEMPLATE.to_string());
    }

    Ok(TemplateBundle {
        post_templates,
        template_source_by_slug,
        list_template,
        not_found_template,
        partials,
    })
}

fn load_template_source(
    data_dir: &Path,
    template: &Template,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    if let Some(content) = &template.content {
        return Ok(content.clone());
    }
    if template.file_path.is_empty() {
        return Ok(String::new());
    }
    let content = fs::read_to_string(data_dir.join(&template.file_path))?;
    let (_, body) = read_template_file(&content)?;
    Ok(body)
}

fn load_language_posts(
    conn: &Connection,
    data_dir: &Path,
    published_posts: &[(Post, String)],
    language: &str,
    main_language: &str,
) -> Result<Vec<RenderPostRecord>, Box<dyn Error + Send + Sync>> {
    let mut posts = Vec::new();
    for (post, body) in published_posts {
        if language.eq_ignore_ascii_case(main_language) {
            posts.push(RenderPostRecord {
                post: post.clone(),
                body_markdown: body.clone(),
            });
            continue;
        }

        if let Ok(translation) = queries::post_translation::get_post_translation_by_post_and_language(conn, &post.id, language) {
            let raw = fs::read_to_string(data_dir.join(&translation.file_path))?;
            let (_, translated_body) = read_translation_file(&raw)?;
            let mut translated_post = post.clone();
            translated_post.title = translation.title.clone();
            translated_post.excerpt = translation.excerpt.clone();
            translated_post.language = Some(translation.language.clone());
            translated_post.status = translation.status.clone();
            translated_post.file_path = translation.file_path.clone();
            translated_post.published_at = translation.published_at.or(post.published_at);
            posts.push(RenderPostRecord {
                post: translated_post,
                body_markdown: translated_body,
            });
        }
    }

    posts.sort_by(|left, right| {
        right.post.published_at.unwrap_or(right.post.created_at)
            .cmp(&left.post.published_at.unwrap_or(left.post.created_at))
    });
    Ok(posts)
}

fn build_language_routes(
    posts: &[RenderPostRecord],
    metadata: &ProjectMetadata,
    language: &str,
    tags: &[Tag],
) -> Vec<RouteSpec> {
    let per_page = metadata.max_posts_per_page.max(1) as usize;
    let mut routes = Vec::new();
    routes.extend(paginated_route_specs(posts, per_page, language_root_prefix(language, metadata), metadata.name.clone(), None));

    let mut category_posts: BTreeMap<String, Vec<RenderPostRecord>> = BTreeMap::new();
    let mut tag_posts: BTreeMap<String, Vec<RenderPostRecord>> = BTreeMap::new();
    let mut year_posts: BTreeMap<i32, Vec<RenderPostRecord>> = BTreeMap::new();
    let mut month_posts: BTreeMap<(i32, u32), Vec<RenderPostRecord>> = BTreeMap::new();

    for record in posts {
        for category in &record.post.categories {
            category_posts.entry(category.clone()).or_default().push(record.clone());
        }
        for tag in &record.post.tags {
            tag_posts.entry(tag.clone()).or_default().push(record.clone());
        }
        if let Some(timestamp) = Utc.timestamp_millis_opt(record.post.published_at.unwrap_or(record.post.created_at)).single() {
            year_posts.entry(timestamp.year()).or_default().push(record.clone());
            month_posts.entry((timestamp.year(), timestamp.month())).or_default().push(record.clone());
        }
    }

    for (category, records) in category_posts {
        let slug = slugify(&category);
        routes.extend(paginated_route_specs(
            &records,
            per_page,
            format!("{}/category/{slug}", language_root_prefix(language, metadata)),
            category.clone(),
            Some(json!({"kind": "category", "name": category})),
        ));
    }

    for (tag_name, records) in tag_posts {
        let slug = slugify(&tag_name);
        let display_name = tags
            .iter()
            .find(|tag| tag.name.eq_ignore_ascii_case(&tag_name))
            .map(|tag| tag.name.clone())
            .unwrap_or(tag_name.clone());
        routes.extend(paginated_route_specs(
            &records,
            per_page,
            format!("{}/tag/{slug}", language_root_prefix(language, metadata)),
            display_name.clone(),
            Some(json!({"kind": "tag", "name": display_name})),
        ));
    }

    for (year, records) in year_posts {
        routes.extend(paginated_route_specs(
            &records,
            per_page,
            format!("{}/{year}", language_root_prefix(language, metadata)),
            format!("{} {year}", metadata.name),
            Some(json!({"kind": "year", "year": year})),
        ));
    }

    for ((year, month), records) in month_posts {
        routes.extend(paginated_route_specs(
            &records,
            per_page,
            format!("{}/{year}/{month:02}", language_root_prefix(language, metadata)),
            format!("{} {year}-{month:02}", metadata.name),
            Some(json!({"kind": "month", "year": year, "month": month})),
        ));
    }

    routes
}

fn paginated_route_specs(
    posts: &[RenderPostRecord],
    per_page: usize,
    base_path: String,
    page_title: String,
    archive_context: Option<Value>,
) -> Vec<RouteSpec> {
    let total_items = posts.len();
    let total_pages = total_items.max(1).div_ceil(per_page.max(1));
    let mut pages = Vec::new();
    for page_index in 0..total_pages.max(1) {
        let current_page = page_index + 1;
        let start = page_index * per_page;
        let end = (start + per_page).min(total_items);
        let slice = if start < end { posts[start..end].to_vec() } else { Vec::new() };
        let relative_base = base_path.trim_matches('/');
        let relative_path = if current_page == 1 {
            if relative_base.is_empty() {
                "index.html".to_string()
            } else {
                format!("{relative_base}/index.html")
            }
        } else if relative_base.is_empty() {
            format!("page/{current_page}/index.html")
        } else {
            format!("{relative_base}/page/{current_page}/index.html")
        };
        let url_path = relative_to_url_path(&relative_path);
        pages.push(RouteSpec {
            relative_path,
            url_path,
            page_title: page_title.clone(),
            archive_context: archive_context.clone(),
            posts: slice,
            current_page,
            total_pages: total_pages.max(1),
            total_items,
            items_per_page: per_page,
        });
    }
    pages
}

fn render_list_route(
    route: &RouteSpec,
    metadata: &ProjectMetadata,
    language: &str,
    posts: &[RenderPostRecord],
    tags: &[Tag],
    menu_items: &[Value],
    post_data_json_by_id: &HashMap<String, Value>,
    bundle: &TemplateBundle,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let main_language = main_language(metadata);
    let canonical_map = canonical_post_path_by_slug(posts, language, main_language);
    let taxonomy_counts = build_taxonomy_counts(posts, tags);
    let context = json!({
        "language": language,
        "language_prefix": language_prefix(language, main_language),
        "html_theme_attribute": serde_json::Value::Null,
        "page_title": route.page_title,
        "pico_stylesheet_href": pico_stylesheet_href(metadata),
        "blog_languages": build_list_blog_languages(metadata, language, &route.url_path),
        "alternate_links": build_alternate_list_links(metadata, &route.url_path),
        "menu_items": menu_items,
        "calendar_initial_year": route.posts.first().map(|post| calendar_initial_parts(&post.post).0).unwrap_or(1970),
        "calendar_initial_month": route.posts.first().map(|post| calendar_initial_parts(&post.post).1).unwrap_or(1),
        "archive_context": route.archive_context,
        "show_archive_range_heading": false,
        "min_date": route.posts.last().map(|record| timestamp_parts(record.post.published_at.unwrap_or(record.post.created_at))),
        "max_date": route.posts.first().map(|record| timestamp_parts(record.post.published_at.unwrap_or(record.post.created_at))),
        "day_blocks": build_day_blocks(&route.posts),
        "is_list_page": route.current_page > 1,
        "is_first_page": route.current_page == 1,
        "is_last_page": route.current_page >= route.total_pages,
        "has_prev_page": route.current_page > 1,
        "has_next_page": route.current_page < route.total_pages,
        "prev_page_href": route.current_page.checked_sub(1).map(|page| route_href(route, page)),
        "next_page_href": if route.current_page < route.total_pages { Some(route_href(route, route.current_page + 1)) } else { None::<String> },
        "current_page": route.current_page,
        "total_pages": route.total_pages,
        "total_items": route.total_items,
        "items_per_page": route.items_per_page,
        "canonical_post_path_by_slug": canonical_map,
        "canonical_media_path_by_source_path": canonical_media_paths(posts),
        "post_data_json_by_id": post_data_json_by_id,
        "post_categories": taxonomy_counts.0,
        "post_tags": taxonomy_counts.1,
        "tag_color_by_name": taxonomy_counts.2,
        "backlinks": Vec::<Value>::new(),
        "not_found_message": serde_json::Value::Null,
        "not_found_back_label": serde_json::Value::Null,
    });

    Ok(render_liquid_template(&bundle.list_template, &bundle.partials, &context)?)
}

#[allow(clippy::too_many_arguments)]
fn render_post_route(
    conn: &Connection,
    metadata: &ProjectMetadata,
    language: &str,
    main_language: &str,
    record: &RenderPostRecord,
    all_posts: &[RenderPostRecord],
    tags: &[Tag],
    category_settings: &HashMap<String, CategorySettings>,
    media_by_id: &HashMap<String, Media>,
    canonical_post_path_by_slug: &HashMap<String, String>,
    menu_items: &[Value],
    post_data_json_by_id: &HashMap<String, Value>,
    bundle: &TemplateBundle,
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let render_categories = category_settings
        .iter()
        .map(|(name, settings)| {
            (name.clone(), RenderCategorySettings {
                post_template_slug: settings.post_template_slug.clone(),
            })
        })
        .collect::<HashMap<_, _>>();
    let resolved = resolve_post_template(RenderTemplateLookup {
        post: &record.post,
        templates: &bundle.post_templates,
        tags,
        category_settings: &render_categories,
    })
    .map_err(|error| format!("template lookup failed: {error:?}"))?;
    let template_source = bundle
        .template_source_by_slug
        .get(&resolved.slug)
        .cloned()
        .or_else(|| resolved.content.clone())
        .unwrap_or_else(|| STARTER_SINGLE_POST_TEMPLATE.to_string());

    let linked_media = queries::post_media::list_post_media_by_post(conn, &record.post.id)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|link| media_by_id.get(&link.media_id))
        .map(media_context)
        .collect::<Vec<_>>();
    let outgoing_links = queries::post_link::list_links_by_source(conn, &record.post.id).unwrap_or_default();
    let incoming_links = queries::post_link::list_links_by_target(conn, &record.post.id).unwrap_or_default();
    let post_by_id = all_posts.iter().map(|item| (item.post.id.clone(), item)).collect::<HashMap<_, _>>();
    let outgoing_link_context = outgoing_links.iter().map(|link| link_context(link, &post_by_id, language, main_language)).collect::<Vec<_>>();
    let incoming_link_context = incoming_links.iter().map(|link| link_context(link, &post_by_id, language, main_language)).collect::<Vec<_>>();
    let backlinks = incoming_links.iter().map(|link| backlink_context(link, &post_by_id, language, main_language)).collect::<Vec<_>>();
    let taxonomy_counts = build_taxonomy_counts(all_posts, tags);

    let context = json!({
        "language": language,
        "language_prefix": language_prefix(language, main_language),
        "page_title": record.post.title,
        "pico_stylesheet_href": pico_stylesheet_href(metadata),
        "html_theme_attribute": serde_json::Value::Null,
        "alternate_links": build_alternate_post_links(&record.post, metadata),
        "blog_languages": build_post_blog_languages(&record.post, metadata, language),
        "menu_items": menu_items,
        "calendar_initial_year": calendar_initial_parts(&record.post).0,
        "calendar_initial_month": calendar_initial_parts(&record.post).1,
        "post": post_context(&record.post, &record.body_markdown, linked_media, outgoing_link_context, incoming_link_context),
        "post_categories": taxonomy_items_for_categories(&record.post.categories, all_posts),
        "post_tags": taxonomy_items_for_tags(&record.post.tags, all_posts, tags),
        "tag_color_by_name": taxonomy_counts.2,
        "backlinks": backlinks,
        "canonical_post_path_by_slug": canonical_post_path_by_slug,
        "canonical_media_path_by_source_path": canonical_media_paths(all_posts),
        "post_data_json_by_id": post_data_json_by_id,
        "day_blocks": Vec::<Value>::new(),
        "archive_context": serde_json::Value::Null,
        "show_archive_range_heading": false,
        "min_date": serde_json::Value::Null,
        "max_date": serde_json::Value::Null,
        "is_list_page": false,
        "is_first_page": true,
        "is_last_page": true,
        "has_prev_page": false,
        "has_next_page": false,
        "prev_page_href": serde_json::Value::Null,
        "next_page_href": serde_json::Value::Null,
        "not_found_message": serde_json::Value::Null,
        "not_found_back_label": serde_json::Value::Null,
    });

    Ok(render_liquid_template(&template_source, &bundle.partials, &context)?)
}

fn render_not_found_route(
    bundle: &TemplateBundle,
    metadata: &ProjectMetadata,
    language: &str,
    requested_path: &str,
    menu_items: &[Value],
) -> Result<String, Box<dyn Error + Send + Sync>> {
    let context = json!({
        "language": language,
        "language_prefix": language_prefix(language, main_language(metadata)),
        "html_theme_attribute": serde_json::Value::Null,
        "page_title": format!("404 | {}", metadata.name),
        "pico_stylesheet_href": pico_stylesheet_href(metadata),
        "blog_languages": build_list_blog_languages(metadata, language, requested_path),
        "alternate_links": build_alternate_list_links(metadata, requested_path),
        "menu_items": menu_items,
        "calendar_initial_year": 1970,
        "calendar_initial_month": 1,
        "post": serde_json::Value::Null,
        "post_categories": Vec::<Value>::new(),
        "post_tags": Vec::<Value>::new(),
        "tag_color_by_name": HashMap::<String, String>::new(),
        "backlinks": Vec::<Value>::new(),
        "day_blocks": Vec::<Value>::new(),
        "archive_context": serde_json::Value::Null,
        "show_archive_range_heading": false,
        "min_date": serde_json::Value::Null,
        "max_date": serde_json::Value::Null,
        "is_list_page": false,
        "is_first_page": true,
        "is_last_page": true,
        "has_prev_page": false,
        "has_next_page": false,
        "prev_page_href": serde_json::Value::Null,
        "next_page_href": serde_json::Value::Null,
        "not_found_message": format!("No rendered page exists for {}", requested_path),
        "not_found_back_label": serde_json::Value::Null,
        "canonical_post_path_by_slug": HashMap::<String, String>::new(),
        "canonical_media_path_by_source_path": HashMap::<String, String>::new(),
        "post_data_json_by_id": HashMap::<String, Value>::new(),
    });
    Ok(render_liquid_template(&bundle.not_found_template, &bundle.partials, &context)?)
}

fn build_menu_items(
    data_dir: &Path,
    language: &str,
    main_language: &str,
) -> Result<Vec<Value>, Box<dyn Error + Send + Sync>> {
    let items = menu::read_menu(data_dir)?;
    Ok(items
        .iter()
        .map(|item| menu_item_context(item, language, main_language))
        .collect())
}

fn menu_item_context(item: &menu::MenuItem, language: &str, main_language: &str) -> Value {
    let children = item
        .children
        .iter()
        .map(|child| menu_item_context(child, language, main_language))
        .collect::<Vec<_>>();
    json!({
        "title": item.label,
        "href": menu_item_href(item, language, main_language),
        "has_children": !children.is_empty(),
        "children": children,
    })
}

fn menu_item_href(item: &menu::MenuItem, language: &str, main_language: &str) -> String {
    let prefix = language_prefix(language, main_language);
    match item.kind {
        MenuItemKind::Home => {
            if prefix.is_empty() {
                "/".to_string()
            } else {
                format!("{prefix}/")
            }
        }
        MenuItemKind::Submenu => "#".to_string(),
        MenuItemKind::Page => item
            .slug
            .as_deref()
            .map(|slug| prefixed_slug_path(&prefix, slug))
            .unwrap_or_else(|| "#".to_string()),
        MenuItemKind::CategoryArchive => item
            .slug
            .as_deref()
            .map(|slug| format!("{}/category/{}/", prefix_or_root(&prefix), slugify(slug)))
            .unwrap_or_else(|| "#".to_string()),
    }
}

fn prefixed_slug_path(prefix: &str, slug: &str) -> String {
    format!("{}{}/", prefix_or_root(prefix), slug.trim_matches('/'))
}

fn prefix_or_root(prefix: &str) -> &str {
    if prefix.is_empty() {
        "/"
    } else {
        prefix
    }
}

fn route_href(route: &RouteSpec, page: usize) -> String {
    let base = route.url_path.trim_end_matches('/');
    if page <= 1 {
        if base.is_empty() { "/".to_string() } else { base.to_string() }
    } else if base.is_empty() || base == "/" {
        format!("/page/{page}")
    } else {
        format!("{base}/page/{page}")
    }
}

fn build_day_blocks(posts: &[RenderPostRecord]) -> Vec<Value> {
    let mut blocks = Vec::new();
    let mut current_key = String::new();
    let mut current_posts = Vec::new();
    let mut current_label = String::new();

    for record in posts {
        let timestamp_ms = record.post.published_at.unwrap_or(record.post.created_at);
        let Some(timestamp) = Utc.timestamp_millis_opt(timestamp_ms).single() else {
            continue;
        };
        let key = format!("{:04}-{:02}-{:02}", timestamp.year(), timestamp.month(), timestamp.day());
        if !current_key.is_empty() && current_key != key {
            blocks.push(json!({
                "show_date_marker": true,
                "date_label": current_label,
                "posts": current_posts,
                "show_separator": true,
            }));
            current_posts = Vec::new();
        }
        current_key = key;
        current_label = format!("{:02}.{:02}.{:04}", timestamp.day(), timestamp.month(), timestamp.year());
        current_posts.push(json!({
            "id": record.post.id,
            "title": record.post.title,
            "slug": record.post.slug,
            "content": record.post.excerpt.clone().unwrap_or_else(|| record.body_markdown.clone()),
            "show_title": true,
        }));
    }

    if !current_key.is_empty() {
        blocks.push(json!({
            "show_date_marker": true,
            "date_label": current_label,
            "posts": current_posts,
            "show_separator": false,
        }));
    }
    blocks
}

fn canonical_post_path_by_slug(
    posts: &[RenderPostRecord],
    language: &str,
    main_language: &str,
) -> HashMap<String, String> {
    posts.iter()
        .map(|record| (
            record.post.slug.clone(),
            build_canonical_post_path(&record.post, language, main_language),
        ))
        .collect()
}

fn canonical_media_paths(posts: &[RenderPostRecord]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for record in posts {
        for media_reference in extract_media_refs(&record.body_markdown) {
            map.entry(media_reference.clone())
                .or_insert_with(|| media_reference.clone());
        }
    }
    map
}

fn extract_media_refs(markdown: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut remainder = markdown;
    while let Some(index) = remainder.find("bds-media://") {
        let rest = &remainder[index..];
        let end = rest.find(|ch: char| ch == ')' || ch == ']' || ch.is_whitespace()).unwrap_or(rest.len());
        refs.push(rest[..end].to_string());
        remainder = &rest[end..];
    }
    refs
}

fn build_post_data_json_by_id(posts: &[RenderPostRecord]) -> HashMap<String, Value> {
    posts.iter()
        .map(|record| {
            (
                record.post.id.clone(),
                json!({
                    "id": record.post.id,
                    "title": record.post.title,
                    "slug": record.post.slug,
                    "excerpt": record.post.excerpt,
                    "author": record.post.author,
                    "language": record.post.language,
                    "published_at": timestamp_parts(record.post.published_at.unwrap_or(record.post.created_at)),
                    "created_at": timestamp_parts(record.post.created_at),
                    "updated_at": timestamp_parts(record.post.updated_at),
                    "tags": record.post.tags,
                    "categories": record.post.categories,
                }),
            )
        })
        .collect()
}

fn build_taxonomy_counts(
    posts: &[RenderPostRecord],
    tags: &[Tag],
) -> (Vec<Value>, Vec<Value>, HashMap<String, String>) {
    let mut category_counts: HashMap<String, usize> = HashMap::new();
    let mut tag_counts: HashMap<String, usize> = HashMap::new();
    let mut tag_colors = HashMap::new();

    for record in posts {
        for category in &record.post.categories {
            *category_counts.entry(category.clone()).or_default() += 1;
        }
        for tag_name in &record.post.tags {
            *tag_counts.entry(tag_name.clone()).or_default() += 1;
        }
    }

    for tag in tags {
        if let Some(color) = &tag.color {
            tag_colors.insert(tag.name.clone(), color.clone());
        }
    }

    let categories = category_counts.into_iter().map(|(name, count)| json!({
        "name": name,
        "slug": slugify(&name),
        "post_count": count,
    })).collect::<Vec<_>>();

    let tag_items = tag_counts.into_iter().map(|(name, count)| {
        let color = tags.iter().find(|tag| tag.name.eq_ignore_ascii_case(&name)).and_then(|tag| tag.color.clone());
        json!({
            "name": name,
            "slug": slugify(&name),
            "color": color,
            "post_count": count,
        })
    }).collect::<Vec<_>>();

    (categories, tag_items, tag_colors)
}

fn taxonomy_items_for_categories(names: &[String], posts: &[RenderPostRecord]) -> Vec<Value> {
    names.iter()
        .map(|name| {
            let count = posts.iter().filter(|record| record.post.categories.iter().any(|category| category.eq_ignore_ascii_case(name))).count();
            json!({
                "name": name,
                "slug": slugify(name),
                "post_count": count,
            })
        })
        .collect()
}

fn taxonomy_items_for_tags(names: &[String], posts: &[RenderPostRecord], tags: &[Tag]) -> Vec<Value> {
    names.iter()
        .map(|name| {
            let count = posts.iter().filter(|record| record.post.tags.iter().any(|tag| tag.eq_ignore_ascii_case(name))).count();
            let color = tags.iter().find(|tag| tag.name.eq_ignore_ascii_case(name)).and_then(|tag| tag.color.clone());
            json!({
                "name": name,
                "slug": slugify(name),
                "color": color,
                "post_count": count,
            })
        })
        .collect()
}

fn post_context(
    post: &Post,
    body_markdown: &str,
    linked_media: Vec<Value>,
    outgoing_links: Vec<Value>,
    incoming_links: Vec<Value>,
) -> Value {
    json!({
        "id": post.id,
        "title": post.title,
        "content": body_markdown,
        "slug": post.slug,
        "excerpt": post.excerpt,
        "author": post.author,
        "language": post.language,
        "show_title": true,
        "published_at": timestamp_parts(post.published_at.unwrap_or(post.created_at)),
        "created_at": timestamp_parts(post.created_at),
        "updated_at": timestamp_parts(post.updated_at),
        "tags": post.tags,
        "categories": post.categories,
        "template_slug": post.template_slug,
        "do_not_translate": post.do_not_translate,
        "linked_media": linked_media,
        "outgoing_links": outgoing_links,
        "incoming_links": incoming_links,
    })
}

fn media_context(media: &Media) -> Value {
    json!({
        "id": media.id,
        "filename": media.filename,
        "original_name": media.original_name,
        "mime_type": media.mime_type,
        "title": media.title,
        "alt": media.alt,
        "caption": media.caption,
        "author": media.author,
        "width": media.width,
        "height": media.height,
        "file_path": canonical_media_path(media),
    })
}

fn link_context(
    link: &crate::model::PostLink,
    post_by_id: &HashMap<String, &RenderPostRecord>,
    language: &str,
    main_language: &str,
) -> Value {
    let target = post_by_id.get(&link.target_post_id).copied();
    json!({
        "path": target.map(|record| build_canonical_post_path(&record.post, language, main_language)).unwrap_or_default(),
        "display_slug": target.map(|record| record.post.slug.clone()).unwrap_or_default(),
        "title": target.map(|record| record.post.title.clone()).unwrap_or_default(),
        "link_text": link.link_text,
    })
}

fn backlink_context(
    link: &crate::model::PostLink,
    post_by_id: &HashMap<String, &RenderPostRecord>,
    language: &str,
    main_language: &str,
) -> Value {
    let source = post_by_id.get(&link.source_post_id).copied();
    json!({
        "path": source.map(|record| build_canonical_post_path(&record.post, language, main_language)).unwrap_or_default(),
        "display_slug": source.map(|record| record.post.slug.clone()).unwrap_or_default(),
        "title": source.map(|record| record.post.title.clone()).unwrap_or_default(),
    })
}

fn timestamp_parts(timestamp_ms: i64) -> Value {
    if let Some(timestamp) = Utc.timestamp_millis_opt(timestamp_ms).single() {
        json!({
            "year": timestamp.year(),
            "month": timestamp.month(),
            "day": timestamp.day(),
        })
    } else {
        json!(null)
    }
}

fn build_alternate_post_links(post: &Post, metadata: &ProjectMetadata) -> Vec<Value> {
    let main_language = main_language(metadata);
    let mut links = render_languages(metadata)
        .into_iter()
        .map(|language| json!({
            "href": format!("{}{}", metadata.public_url.as_deref().unwrap_or("").trim_end_matches('/'), build_canonical_post_path(post, &language, main_language)),
            "hreflang": language,
        }))
        .collect::<Vec<_>>();
    links.push(json!({
        "href": format!("{}{}", metadata.public_url.as_deref().unwrap_or("").trim_end_matches('/'), build_canonical_post_path(post, post.language.as_deref().unwrap_or(main_language), main_language)),
        "hreflang": "x-default",
    }));
    links
}

fn build_post_blog_languages(post: &Post, metadata: &ProjectMetadata, current_language: &str) -> Vec<Value> {
    let main_language = main_language(metadata);
    render_languages(metadata)
        .into_iter()
        .map(|language| json!({
            "is_current": language.eq_ignore_ascii_case(current_language),
            "code": language,
            "flag": crate::i18n::normalize_language(&language).flag_emoji().to_string(),
            "href": format!("{}{}", metadata.public_url.as_deref().unwrap_or("").trim_end_matches('/'), build_canonical_post_path(post, &language, main_language)),
            "href_prefix": language_prefix(&language, main_language),
        }))
        .collect()
}

fn build_list_blog_languages(metadata: &ProjectMetadata, current_language: &str, current_url_path: &str) -> Vec<Value> {
    let main_language = main_language(metadata);
    render_languages(metadata)
        .into_iter()
        .map(|language| {
            let path = relocalize_url_path(current_url_path, &language, main_language);
            json!({
                "is_current": language.eq_ignore_ascii_case(current_language),
                "code": language,
                "flag": crate::i18n::normalize_language(&language).flag_emoji().to_string(),
                "href": format!("{}{}", metadata.public_url.as_deref().unwrap_or("").trim_end_matches('/'), path),
                "href_prefix": language_prefix(&language, main_language),
            })
        })
        .collect()
}

fn build_alternate_list_links(metadata: &ProjectMetadata, current_url_path: &str) -> Vec<Value> {
    let main_language = main_language(metadata);
    let mut links = render_languages(metadata)
        .into_iter()
        .map(|language| {
            let path = relocalize_url_path(current_url_path, &language, main_language);
            json!({
                "href": format!("{}{}", metadata.public_url.as_deref().unwrap_or("").trim_end_matches('/'), path),
                "hreflang": language,
            })
        })
        .collect::<Vec<_>>();
    links.push(json!({
        "href": format!("{}{}", metadata.public_url.as_deref().unwrap_or("").trim_end_matches('/'), relocalize_url_path(current_url_path, main_language, main_language)),
        "hreflang": "x-default",
    }));
    links
}

fn relocalize_url_path(current_url_path: &str, target_language: &str, main_language: &str) -> String {
    let stripped = strip_language_prefix(current_url_path, main_language);
    if target_language.eq_ignore_ascii_case(main_language) {
        stripped
    } else if stripped == "/" {
        format!("/{target_language}")
    } else {
        format!("/{target_language}{}", stripped)
    }
}

fn strip_language_prefix(path: &str, main_language: &str) -> String {
    let normalized = normalize_request_path(path);
    let trimmed = normalized.trim_start_matches('/');
    if let Some((first, remainder)) = trimmed.split_once('/') {
        if first.len() == 2 && !first.eq_ignore_ascii_case(main_language) {
            return format!("/{}", remainder.trim_start_matches('/'));
        }
    }
    normalized
}

fn normalize_request_path(path: &str) -> String {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "/" {
        "/".to_string()
    } else {
        format!("/{}", trimmed.trim_matches('/'))
    }
}

fn relative_to_url_path(relative_path: &str) -> String {
    if relative_path == "index.html" {
        return "/".to_string();
    }
    let trimmed = relative_path.trim_end_matches("index.html").trim_end_matches('/');
    format!("/{}", trimmed.trim_start_matches('/'))
}

fn language_from_path(path: &str, metadata: &ProjectMetadata) -> String {
    let trimmed = path.trim_start_matches('/');
    if let Some((candidate, _)) = trimmed.split_once('/') {
        if render_languages(metadata).iter().any(|language| language == candidate) {
            return candidate.to_string();
        }
    }
    main_language(metadata).to_string()
}

fn render_languages(metadata: &ProjectMetadata) -> Vec<String> {
    let main = main_language(metadata).to_string();
    let mut languages = vec![main.clone()];
    for language in &metadata.blog_languages {
        if !languages.iter().any(|existing| existing.eq_ignore_ascii_case(language)) {
            languages.push(language.clone());
        }
    }
    languages
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

fn language_root_prefix(language: &str, metadata: &ProjectMetadata) -> String {
    language_prefix(language, main_language(metadata))
}

fn normalize_partial_slug(slug: &str) -> String {
    let trimmed = slug.trim().trim_matches('/');
    if trimmed.starts_with("partials/") {
        trimmed.to_string()
    } else {
        format!("partials/{trimmed}")
    }
}

fn starter_partials() -> HashMap<String, String> {
    HashMap::from([
        ("partials/head".to_string(), STARTER_HEAD_PARTIAL.to_string()),
        ("partials/menu".to_string(), STARTER_MENU_PARTIAL.to_string()),
        ("partials/language-switcher".to_string(), STARTER_LANGUAGE_SWITCHER_PARTIAL.to_string()),
        (
            "partials/menu-items".to_string(),
            "{% for item in items %}<a href=\"{{ item.href }}\">{{ item.title }}</a>{% endfor %}".to_string(),
        ),
    ])
}

fn pico_stylesheet_href(metadata: &ProjectMetadata) -> Option<String> {
    metadata.pico_theme.as_ref().map(|_| "/assets/pico.min.css".to_string())
}

fn calendar_initial_parts(post: &Post) -> (i32, u32) {
    let timestamp_ms = post.published_at.unwrap_or(post.created_at);
    Utc.timestamp_millis_opt(timestamp_ms)
        .single()
        .map(|timestamp| (timestamp.year(), timestamp.month()))
        .unwrap_or((1970, 1))
}

fn canonical_media_path(media: &Media) -> String {
    if media.file_path.starts_with('/') {
        media.file_path.clone()
    } else {
        format!("/{}", media.file_path.trim_start_matches('/'))
    }
}

fn queries_category_settings(
    data_dir: &Path,
) -> Result<HashMap<String, CategorySettings>, Box<dyn Error + Send + Sync>> {
    Ok(crate::engine::meta::read_category_meta_json(data_dir).unwrap_or_default())
}