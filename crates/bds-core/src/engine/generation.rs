use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::db::DbConnection as Connection;
use chrono::{DateTime, TimeZone, Utc};
use pagefind::api::PagefindIndex;
use pagefind::options::PagefindServiceConfig;
use walkdir::WalkDir;

use crate::db::queries;
use crate::engine::site_assets::write_bundled_site_assets;
use crate::engine::validate_site::SiteValidationReport;
use crate::engine::{EngineError, EngineResult};
use crate::model::{CategorySettings, Post, ProjectMetadata};
use crate::render::{
    GeneratedWriteOutcome, build_calendar_json, build_canonical_post_path,
    build_site_render_artifacts, render_markdown_to_html, write_generated_bytes,
    write_generated_file,
};

#[derive(Debug, Clone)]
pub struct PublishedPostSource {
    pub post: Post,
    pub body_markdown: String,
}

/// Whether a post has a published snapshot eligible for site generation.
pub fn has_published_snapshot(post: &Post) -> bool {
    matches!(
        post.status,
        crate::model::PostStatus::Published | crate::model::PostStatus::Draft
    ) && !post.file_path.trim().is_empty()
}

/// Load the last-published body from disk, never from draft database content.
pub fn load_published_post_source(
    data_dir: &Path,
    post: Post,
) -> EngineResult<Option<PublishedPostSource>> {
    if !has_published_snapshot(&post) {
        return Ok(None);
    }
    let raw = std::fs::read_to_string(data_dir.join(&post.file_path))?;
    let (_, body_markdown) =
        crate::util::frontmatter::read_post_file(&raw).map_err(EngineError::Parse)?;
    Ok(Some(PublishedPostSource {
        post,
        body_markdown,
    }))
}

#[derive(Debug, Default, Clone)]
pub struct GenerationReport {
    pub written_paths: Vec<String>,
    pub skipped_paths: Vec<String>,
    pub deleted_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GenerationSection {
    Core,
    Single,
    Category,
    Tag,
    Date,
}

pub fn generate_starter_site(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    _language: &str,
) -> EngineResult<GenerationReport> {
    generate_starter_site_with_progress(
        conn,
        output_dir,
        project_id,
        metadata,
        posts,
        _language,
        |_current, _total, _path| {},
    )
}

pub fn generate_starter_site_with_progress(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    _language: &str,
    mut on_page: impl FnMut(usize, usize, &str),
) -> EngineResult<GenerationReport> {
    let mut report = GenerationReport::default();
    let data_dir = project_data_dir(output_dir);
    let category_settings = load_category_settings(&data_dir);
    let list_posts = filter_posts_for_lists(posts, &category_settings);
    let input_posts = posts
        .iter()
        .map(|source| (source.post.clone(), source.body_markdown.clone()))
        .collect::<Vec<_>>();
    let artifacts =
        build_site_render_artifacts(conn, &data_dir, project_id, metadata, &input_posts)
            .map_err(|error| EngineError::Parse(error.to_string()))?;

    let total_pages = artifacts.pages.len();
    for (index, page) in artifacts.pages.iter().enumerate() {
        write_out(
            conn,
            output_dir,
            project_id,
            &page.relative_path,
            &page.html,
            &mut report,
        )?;
        on_page(index + 1, total_pages, &page.url_path);
    }

    write_bundled_site_assets(conn, output_dir, project_id, &mut report)?;

    write_out(
        conn,
        output_dir,
        project_id,
        "calendar.json",
        &build_calendar_json(
            &list_posts
                .iter()
                .map(|source| source.post.clone())
                .collect::<Vec<_>>(),
        )?,
        &mut report,
    )?;

    for render_language in render_languages(metadata) {
        let localized_posts =
            localized_sources(conn, &data_dir, &list_posts, &render_language, metadata)?;
        let prefix = if render_language
            == metadata
                .main_language
                .clone()
                .unwrap_or_else(|| "en".to_string())
        {
            String::new()
        } else {
            format!("{}/", render_language)
        };
        let rss = build_rss_xml(metadata, &localized_posts, &render_language);
        if prefix.is_empty() {
            write_out(conn, output_dir, project_id, "rss.xml", &rss, &mut report)?;
        }
        write_out(
            conn,
            output_dir,
            project_id,
            &format!("{prefix}feed.xml"),
            &rss,
            &mut report,
        )?;
        write_out(
            conn,
            output_dir,
            project_id,
            &format!("{prefix}atom.xml"),
            &build_atom_xml(metadata, &localized_posts, &render_language),
            &mut report,
        )?;
        write_out(
            conn,
            output_dir,
            project_id,
            &format!("{prefix}sitemap.xml"),
            &build_sitemap_xml(
                metadata,
                &artifacts.pages,
                &localized_posts,
                &render_language,
            ),
            &mut report,
        )?;
    }

    write_pagefind_indexes(
        conn,
        output_dir,
        project_id,
        &artifacts.pagefind_documents,
        &mut report,
    )?;

    Ok(report)
}

pub fn sections_from_validation_report(report: &SiteValidationReport) -> Vec<GenerationSection> {
    let mut sections = HashSet::new();
    let mut saw_unknown = false;

    for path in report
        .missing_pages
        .iter()
        .chain(report.extra_pages.iter())
        .chain(report.stale_pages.iter())
    {
        match classify_generated_path(path) {
            Some(section) => {
                sections.insert(section);
            }
            None => {
                saw_unknown = true;
            }
        }
    }

    if saw_unknown && !report_is_empty(report) {
        return all_sections();
    }

    let mut ordered = sections.into_iter().collect::<Vec<_>>();
    ordered.sort_by_key(section_sort_key);
    ordered
}

pub fn apply_validation_sections(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    sections: &[GenerationSection],
) -> EngineResult<GenerationReport> {
    if sections.is_empty() {
        return Ok(GenerationReport::default());
    }

    let section_set = sections.iter().copied().collect::<HashSet<_>>();
    let data_dir = project_data_dir(output_dir);
    let category_settings = load_category_settings(&data_dir);
    let list_posts = filter_posts_for_lists(posts, &category_settings);
    let input_posts = posts
        .iter()
        .map(|source| (source.post.clone(), source.body_markdown.clone()))
        .collect::<Vec<_>>();
    let artifacts =
        build_site_render_artifacts(conn, &data_dir, project_id, metadata, &input_posts)
            .map_err(|error| EngineError::Parse(error.to_string()))?;
    let mut report = GenerationReport::default();
    let expected_paths = expected_paths_for_sections(metadata, &artifacts.pages, &section_set);

    for page in &artifacts.pages {
        if path_matches_sections(&page.relative_path, &section_set) {
            write_out(
                conn,
                output_dir,
                project_id,
                &page.relative_path,
                &page.html,
                &mut report,
            )?;
        }
    }

    if section_set.contains(&GenerationSection::Core) {
        write_bundled_site_assets(conn, output_dir, project_id, &mut report)?;
        write_out(
            conn,
            output_dir,
            project_id,
            "calendar.json",
            &build_calendar_json(
                &list_posts
                    .iter()
                    .map(|source| source.post.clone())
                    .collect::<Vec<_>>(),
            )?,
            &mut report,
        )?;

        for render_language in render_languages(metadata) {
            let localized_posts =
                localized_sources(conn, &data_dir, &list_posts, &render_language, metadata)?;
            let prefix = if render_language
                == metadata
                    .main_language
                    .clone()
                    .unwrap_or_else(|| "en".to_string())
            {
                String::new()
            } else {
                format!("{}/", render_language)
            };
            let rss = build_rss_xml(metadata, &localized_posts, &render_language);
            if prefix.is_empty() {
                write_out(conn, output_dir, project_id, "rss.xml", &rss, &mut report)?;
            }
            write_out(
                conn,
                output_dir,
                project_id,
                &format!("{prefix}feed.xml"),
                &rss,
                &mut report,
            )?;
            write_out(
                conn,
                output_dir,
                project_id,
                &format!("{prefix}atom.xml"),
                &build_atom_xml(metadata, &localized_posts, &render_language),
                &mut report,
            )?;
            write_out(
                conn,
                output_dir,
                project_id,
                &format!("{prefix}sitemap.xml"),
                &build_sitemap_xml(
                    metadata,
                    &artifacts.pages,
                    &localized_posts,
                    &render_language,
                ),
                &mut report,
            )?;
        }
    }

    remove_extra_section_paths(output_dir, &expected_paths, &section_set, &mut report)?;
    write_pagefind_indexes(
        conn,
        output_dir,
        project_id,
        &artifacts.pagefind_documents,
        &mut report,
    )?;

    Ok(report)
}

fn write_out(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    relative_path: &str,
    content: &str,
    report: &mut GenerationReport,
) -> EngineResult<()> {
    match write_generated_file(conn, output_dir, project_id, relative_path, content)
        .map_err(|error| EngineError::Parse(error.to_string()))?
    {
        GeneratedWriteOutcome::Written => report.written_paths.push(relative_path.to_string()),
        GeneratedWriteOutcome::SkippedUnchanged => {
            report.skipped_paths.push(relative_path.to_string())
        }
    }
    Ok(())
}

fn write_pagefind_indexes(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    documents: &[crate::render::PagefindDocument],
    report: &mut GenerationReport,
) -> EngineResult<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(EngineError::Io)?;

    let grouped = documents.iter().fold(
        HashMap::<String, Vec<&crate::render::PagefindDocument>>::new(),
        |mut acc, doc| {
            acc.entry(doc.language.clone()).or_default().push(doc);
            acc
        },
    );

    for (language, docs) in grouped {
        let config = PagefindServiceConfig::builder()
            .keep_index_url(true)
            .force_language(language.clone())
            .build();
        let mut index = PagefindIndex::new(Some(config))
            .map_err(|error| EngineError::Parse(error.to_string()))?;
        runtime.block_on(async {
            for doc in docs {
                index
                    .add_html_file(Some(doc.relative_path.clone()), None, doc.html.clone())
                    .await
                    .map_err(|error| EngineError::Parse(error.to_string()))?;
            }
            let files = index
                .get_files()
                .await
                .map_err(|error| EngineError::Parse(error.to_string()))?;
            for file in files {
                let relative = file
                    .filename
                    .to_string_lossy()
                    .trim_start_matches('/')
                    .to_string();
                match write_generated_bytes(conn, output_dir, project_id, &relative, &file.contents)
                    .map_err(|error| EngineError::Parse(error.to_string()))?
                {
                    GeneratedWriteOutcome::Written => report.written_paths.push(relative),
                    GeneratedWriteOutcome::SkippedUnchanged => report.skipped_paths.push(relative),
                }
            }
            Ok::<(), EngineError>(())
        })?;
    }

    Ok(())
}

fn project_data_dir(output_dir: &Path) -> std::path::PathBuf {
    if output_dir.join("meta").exists() {
        output_dir.to_path_buf()
    } else {
        output_dir.parent().unwrap_or(output_dir).to_path_buf()
    }
}

fn expected_paths_for_sections(
    metadata: &ProjectMetadata,
    pages: &[crate::render::SitePage],
    sections: &HashSet<GenerationSection>,
) -> HashSet<String> {
    let mut expected = pages
        .iter()
        .filter(|page| path_matches_sections(&page.relative_path, sections))
        .map(|page| page.relative_path.clone())
        .collect::<HashSet<_>>();

    if sections.contains(&GenerationSection::Core) {
        expected.insert("calendar.json".to_string());
        expected.insert("rss.xml".to_string());
        for language in render_languages(metadata) {
            let prefix = if language
                == metadata
                    .main_language
                    .clone()
                    .unwrap_or_else(|| "en".to_string())
            {
                String::new()
            } else {
                format!("{language}/")
            };
            expected.insert(format!("{prefix}feed.xml"));
            expected.insert(format!("{prefix}atom.xml"));
            expected.insert(format!("{prefix}sitemap.xml"));
        }
    }

    expected
}

fn remove_extra_section_paths(
    output_dir: &Path,
    expected: &HashSet<String>,
    sections: &HashSet<GenerationSection>,
    report: &mut GenerationReport,
) -> EngineResult<()> {
    if !output_dir.exists() {
        return Ok(());
    }

    let mut deleted = Vec::new();
    for entry in WalkDir::new(output_dir).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(output_dir)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        if rel.starts_with("meta/")
            || rel.starts_with("posts/")
            || rel.starts_with("media/")
            || rel.starts_with("assets/")
            || rel.starts_with("pagefind")
            || rel.contains("/pagefind/")
        {
            continue;
        }
        if !matches_generated_extension(&rel)
            || !path_matches_sections(&rel, sections)
            || expected.contains(&rel)
        {
            continue;
        }
        std::fs::remove_file(entry.path()).map_err(EngineError::Io)?;
        deleted.push(rel);
    }

    deleted.sort();
    report.deleted_paths.extend(deleted);
    Ok(())
}

fn path_matches_sections(path: &str, sections: &HashSet<GenerationSection>) -> bool {
    classify_generated_path(path)
        .map(|section| sections.contains(&section))
        .unwrap_or(false)
}

fn classify_generated_path(path: &str) -> Option<GenerationSection> {
    if path.ends_with(".xml") || path.ends_with(".json") {
        return Some(GenerationSection::Core);
    }

    let mut parts = path.split('/').collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    if has_language_prefix(&parts) {
        parts.remove(0);
    }

    match parts.as_slice() {
        ["index.html"] => Some(GenerationSection::Core),
        ["category", ..] => Some(GenerationSection::Category),
        ["tag", ..] => Some(GenerationSection::Tag),
        [year, "index.html"] if is_year_segment(year) => Some(GenerationSection::Date),
        [year, month, "index.html"] if is_year_segment(year) && is_month_segment(month) => {
            Some(GenerationSection::Date)
        }
        [year, month, day, _slug, "index.html"]
            if is_year_segment(year) && is_month_segment(month) && is_day_segment(day) =>
        {
            Some(GenerationSection::Single)
        }
        _ => None,
    }
}

fn has_language_prefix(parts: &[&str]) -> bool {
    match parts {
        [first, second, ..] => {
            !is_year_segment(first)
                && *first != "category"
                && *first != "tag"
                && (*second == "index.html"
                    || is_year_segment(second)
                    || *second == "category"
                    || *second == "tag")
        }
        _ => false,
    }
}

fn is_year_segment(value: &str) -> bool {
    value.len() == 4 && value.chars().all(|ch| ch.is_ascii_digit())
}

fn is_month_segment(value: &str) -> bool {
    value.len() == 2 && value.chars().all(|ch| ch.is_ascii_digit())
}

fn is_day_segment(value: &str) -> bool {
    is_month_segment(value)
}

fn matches_generated_extension(path: &str) -> bool {
    path.ends_with(".html") || path.ends_with(".xml") || path.ends_with(".json")
}

fn all_sections() -> Vec<GenerationSection> {
    vec![
        GenerationSection::Core,
        GenerationSection::Single,
        GenerationSection::Category,
        GenerationSection::Tag,
        GenerationSection::Date,
    ]
}

fn section_sort_key(section: &GenerationSection) -> u8 {
    match section {
        GenerationSection::Core => 0,
        GenerationSection::Single => 1,
        GenerationSection::Category => 2,
        GenerationSection::Tag => 3,
        GenerationSection::Date => 4,
    }
}

fn report_is_empty(report: &SiteValidationReport) -> bool {
    report.missing_pages.is_empty()
        && report.extra_pages.is_empty()
        && report.stale_pages.is_empty()
}

fn render_languages(metadata: &ProjectMetadata) -> Vec<String> {
    let main = metadata
        .main_language
        .clone()
        .unwrap_or_else(|| "en".to_string());
    let mut languages = vec![main.clone()];
    for language in &metadata.blog_languages {
        if !languages
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(language))
        {
            languages.push(language.clone());
        }
    }
    languages
}

fn localized_sources(
    conn: &Connection,
    data_dir: &Path,
    posts: &[PublishedPostSource],
    language: &str,
    metadata: &ProjectMetadata,
) -> EngineResult<Vec<PublishedPostSource>> {
    let main_language = metadata.main_language.as_deref().unwrap_or("en");
    let mut localized = Vec::new();
    for source in posts {
        if language.eq_ignore_ascii_case(main_language) {
            localized.push(source.clone());
            continue;
        }
        if let Ok(translation) =
            queries::post_translation::get_post_translation_by_post_and_language(
                conn,
                &source.post.id,
                language,
            )
        {
            let raw = std::fs::read_to_string(
                data_dir.join(translation.file_path.trim_start_matches('/')),
            )
            .map_err(EngineError::Io)?;
            let (_, body) = crate::util::frontmatter::read_translation_file(&raw)
                .map_err(EngineError::Parse)?;
            let mut translated_post = source.post.clone();
            translated_post.title = translation.title.clone();
            translated_post.excerpt = translation.excerpt.clone();
            translated_post.language = Some(translation.language.clone());
            translated_post.file_path = translation.file_path.clone();
            translated_post.published_at = translation.published_at.or(source.post.published_at);
            localized.push(PublishedPostSource {
                post: translated_post,
                body_markdown: body,
            });
        }
    }
    Ok(localized)
}

fn load_category_settings(data_dir: &Path) -> HashMap<String, CategorySettings> {
    crate::engine::meta::read_category_meta_json(data_dir).unwrap_or_default()
}

fn filter_posts_for_lists(
    posts: &[PublishedPostSource],
    category_settings: &HashMap<String, CategorySettings>,
) -> Vec<PublishedPostSource> {
    posts
        .iter()
        .filter(|source| {
            !source.post.categories.iter().any(|category| {
                category_settings
                    .get(category)
                    .map(|settings| !settings.render_in_lists)
                    .unwrap_or(false)
            })
        })
        .cloned()
        .collect()
}

fn build_rss_xml(
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    language: &str,
) -> String {
    let base_url = metadata
        .public_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let last_build = posts
        .iter()
        .filter_map(|post| timestamp(post.post.published_at.unwrap_or(post.post.created_at)))
        .max()
        .unwrap_or_else(Utc::now);

    let mut xml = vec![
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_string(),
        "<rss version=\"2.0\" xmlns:content=\"http://purl.org/rss/1.0/modules/content/\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\">".to_string(),
        "  <channel>".to_string(),
        format!("    <title>{}</title>", escape_xml(&metadata.name)),
        format!("    <link>{base_url}/</link>"),
        format!("    <description>{}</description>", escape_xml(metadata.description.as_deref().unwrap_or(""))),
        format!("    <lastBuildDate>{}</lastBuildDate>", last_build.format("%a, %d %b %Y %H:%M:%S GMT")),
        "    <generator>bDS</generator>".to_string(),
    ];

    for source in posts {
        let url = format!(
            "{base_url}{}",
            build_canonical_post_path(
                &source.post,
                language,
                metadata.main_language.as_deref().unwrap_or("en")
            )
        );
        let published = timestamp(source.post.published_at.unwrap_or(source.post.created_at))
            .unwrap_or_else(Utc::now);
        xml.push("    <item>".to_string());
        xml.push(format!(
            "      <title>{}</title>",
            escape_xml(&source.post.title)
        ));
        xml.push(format!("      <link>{url}</link>"));
        xml.push(format!("      <guid isPermaLink=\"true\">{url}</guid>"));
        xml.push(format!(
            "      <pubDate>{}</pubDate>",
            published.format("%a, %d %b %Y %H:%M:%S GMT")
        ));
        if let Some(author) = &source.post.author {
            xml.push(format!("      <author>{}</author>", escape_xml(author)));
        }
        xml.push(format!(
            "      <dc:language>{}</dc:language>",
            escape_xml(language)
        ));
        let html = render_markdown_to_html(&source.body_markdown);
        xml.push(format!(
            "      <description><![CDATA[{html}]]></description>"
        ));
        xml.push(format!(
            "      <content:encoded><![CDATA[{html}]]></content:encoded>"
        ));
        for category in &source.post.categories {
            xml.push(format!(
                "      <category>{}</category>",
                escape_xml(category)
            ));
        }
        for tag in &source.post.tags {
            xml.push(format!("      <category>{}</category>", escape_xml(tag)));
        }
        xml.push("    </item>".to_string());
    }

    xml.push("  </channel>".to_string());
    xml.push("</rss>".to_string());
    xml.join("\n")
}

fn build_atom_xml(
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    language: &str,
) -> String {
    let base_url = metadata
        .public_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let main_language = metadata.main_language.as_deref().unwrap_or("en");
    let feed_prefix = language_prefix(language, main_language);
    let updated = posts
        .iter()
        .filter_map(|post| timestamp(post.post.published_at.unwrap_or(post.post.created_at)))
        .max()
        .unwrap_or_else(Utc::now);
    let mut xml = vec![
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_string(),
        "<feed xmlns=\"http://www.w3.org/2005/Atom\">".to_string(),
        format!("  <title>{}</title>", escape_xml(&metadata.name)),
        format!(
            "  <subtitle>{}</subtitle>",
            escape_xml(metadata.description.as_deref().unwrap_or(""))
        ),
        format!("  <id>{base_url}/</id>"),
        format!("  <link href=\"{base_url}{feed_prefix}/\" rel=\"alternate\" />"),
        format!("  <link href=\"{base_url}{feed_prefix}/atom.xml\" rel=\"self\" />"),
        format!(
            "  <updated>{}</updated>",
            updated.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ),
    ];

    for source in posts {
        let url = format!(
            "{base_url}{}",
            build_canonical_post_path(
                &source.post,
                language,
                metadata.main_language.as_deref().unwrap_or("en")
            )
        );
        let published = timestamp(source.post.published_at.unwrap_or(source.post.created_at))
            .unwrap_or_else(Utc::now);
        let html = render_markdown_to_html(&source.body_markdown);
        xml.push(format!("  <entry xml:lang=\"{}\">", escape_xml(language)));
        xml.push(format!(
            "    <title>{}</title>",
            escape_xml(&source.post.title)
        ));
        xml.push(format!("    <id>{url}</id>"));
        xml.push(format!("    <link href=\"{url}\" />"));
        xml.push(format!(
            "    <updated>{}</updated>",
            published.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ));
        xml.push(format!(
            "    <published>{}</published>",
            published.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ));
        if let Some(author) = &source.post.author {
            xml.push(format!(
                "    <author><name>{}</name></author>",
                escape_xml(author)
            ));
        }
        xml.push(format!("    <summary type=\"xhtml\"><div xmlns=\"http://www.w3.org/1999/xhtml\">{html}</div></summary>"));
        xml.push(format!("    <content type=\"xhtml\"><div xmlns=\"http://www.w3.org/1999/xhtml\">{html}</div></content>"));
        for category in &source.post.categories {
            xml.push(format!(
                "    <category term=\"{}\" />",
                escape_xml(category)
            ));
        }
        for tag in &source.post.tags {
            xml.push(format!("    <category term=\"{}\" />", escape_xml(tag)));
        }
        xml.push("  </entry>".to_string());
    }

    xml.push("</feed>".to_string());
    xml.join("\n")
}

fn build_sitemap_xml(
    metadata: &ProjectMetadata,
    pages: &[crate::render::SitePage],
    posts: &[PublishedPostSource],
    language: &str,
) -> String {
    let base_url = metadata
        .public_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let main_language = metadata.main_language.as_deref().unwrap_or("en");
    let languages = render_languages(metadata);
    let index_lastmod = posts
        .iter()
        .filter_map(|post| timestamp(post.post.published_at.unwrap_or(post.post.created_at)))
        .max()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let post_lastmod_by_path = posts
        .iter()
        .filter_map(|source| {
            timestamp(source.post.published_at.unwrap_or(source.post.created_at)).map(|lastmod| {
                (
                    build_canonical_post_path(&source.post, language, main_language),
                    lastmod.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
                )
            })
        })
        .collect::<HashMap<_, _>>();
    let page_groups = group_pages_by_logical_path(pages, &languages, main_language);

    let mut xml = vec![
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_string(),
        "<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">".to_string(),
    ];

    for page in pages.iter().filter(|page| page.language == language) {
        let url = format!("{base_url}{}", page.url_path);
        let logical_key = logical_page_key(&page.relative_path, &languages, main_language);
        let alternates = page_groups.get(&logical_key);
        let lastmod = post_lastmod_by_path
            .get(&page.url_path)
            .cloned()
            .unwrap_or_else(|| index_lastmod.clone());
        let is_home = page.url_path == language_root_url_path(language, main_language);
        xml.push("  <url>".to_string());
        xml.push(format!("    <loc>{url}</loc>"));
        xml.push(format!("    <lastmod>{lastmod}</lastmod>"));
        xml.push(format!(
            "    <changefreq>{}</changefreq>",
            if is_home { "daily" } else { "weekly" }
        ));
        xml.push(format!(
            "    <priority>{}</priority>",
            if is_home { "1.0" } else { "0.8" }
        ));
        if let Some(alternates) = alternates {
            for alternate in alternates {
                xml.push(format!(
                    "    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{base_url}{}\" />",
                    escape_xml(&alternate.language),
                    alternate.url_path,
                ));
            }
            if let Some(default_page) = alternates
                .iter()
                .find(|alternate| alternate.language == main_language)
            {
                xml.push(format!(
                    "    <xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"{base_url}{}\" />",
                    default_page.url_path,
                ));
            }
        }
        xml.push("  </url>".to_string());
    }

    xml.push("</urlset>".to_string());
    xml.join("\n")
}

fn language_prefix(language: &str, main_language: &str) -> String {
    if language.eq_ignore_ascii_case(main_language) {
        String::new()
    } else {
        format!("/{language}")
    }
}

fn language_root_url_path(language: &str, main_language: &str) -> String {
    let prefix = language_prefix(language, main_language);
    if prefix.is_empty() {
        "/".to_string()
    } else {
        format!("{prefix}/")
    }
}

fn group_pages_by_logical_path<'a>(
    pages: &'a [crate::render::SitePage],
    languages: &[String],
    main_language: &str,
) -> HashMap<String, Vec<&'a crate::render::SitePage>> {
    let mut grouped = HashMap::<String, Vec<&crate::render::SitePage>>::new();
    for page in pages {
        let key = logical_page_key(&page.relative_path, languages, main_language);
        grouped.entry(key).or_default().push(page);
    }
    grouped
}

fn logical_page_key(relative_path: &str, languages: &[String], main_language: &str) -> String {
    let mut parts = relative_path.split('/');
    let Some(first) = parts.next() else {
        return relative_path.to_string();
    };
    if first.eq_ignore_ascii_case(main_language) {
        return parts.collect::<Vec<_>>().join("/");
    }
    if languages.iter().any(|language| {
        language.eq_ignore_ascii_case(first) && !language.eq_ignore_ascii_case(main_language)
    }) {
        return parts.collect::<Vec<_>>().join("/");
    }
    relative_path.to_string()
}

fn timestamp(timestamp_ms: i64) -> Option<DateTime<Utc>> {
    chrono::Utc.timestamp_millis_opt(timestamp_ms).single()
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
