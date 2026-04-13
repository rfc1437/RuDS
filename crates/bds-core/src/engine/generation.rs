use std::path::Path;
use std::collections::HashMap;

use chrono::{DateTime, TimeZone, Utc};
use pagefind::api::PagefindIndex;
use pagefind::options::PagefindServiceConfig;
use rusqlite::Connection;

use crate::db::queries;
use crate::engine::{EngineError, EngineResult};
use crate::model::{Post, ProjectMetadata};
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

#[derive(Debug, Default, Clone)]
pub struct GenerationReport {
    pub written_paths: Vec<String>,
    pub skipped_paths: Vec<String>,
}

pub fn generate_starter_site(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    language: &str,
) -> EngineResult<GenerationReport> {
    let mut report = GenerationReport::default();
    let input_posts = posts
        .iter()
        .map(|source| (source.post.clone(), source.body_markdown.clone()))
        .collect::<Vec<_>>();
    let artifacts = build_site_render_artifacts(conn, output_dir.parent().unwrap_or(output_dir), project_id, metadata, &input_posts)
        .map_err(|error| EngineError::Parse(error.to_string()))?;

    for page in &artifacts.pages {
        write_out(conn, output_dir, project_id, &page.relative_path, &page.html, &mut report)?;
    }

    write_out(
        conn,
        output_dir,
        project_id,
        "calendar.json",
        &build_calendar_json(&posts.iter().map(|source| source.post.clone()).collect::<Vec<_>>())?,
        &mut report,
    )?;

    for render_language in render_languages(metadata) {
        let localized_posts = localized_sources(conn, output_dir.parent().unwrap_or(output_dir), posts, &render_language, metadata)?;
        let prefix = if render_language == metadata.main_language.clone().unwrap_or_else(|| "en".to_string()) {
            String::new()
        } else {
            format!("{}/", render_language)
        };
        let rss = build_rss_xml(metadata, &localized_posts, &render_language);
        if prefix.is_empty() {
            write_out(conn, output_dir, project_id, "rss.xml", &rss, &mut report)?;
        }
        write_out(conn, output_dir, project_id, &format!("{prefix}feed.xml"), &rss, &mut report)?;
        write_out(conn, output_dir, project_id, &format!("{prefix}atom.xml"), &build_atom_xml(metadata, &localized_posts, &render_language), &mut report)?;
        write_out(conn, output_dir, project_id, &format!("{prefix}sitemap.xml"), &build_sitemap_xml(metadata, &localized_posts, &render_language), &mut report)?;
    }

    write_pagefind_indexes(conn, output_dir, project_id, &artifacts.pagefind_documents, &mut report)?;

    Ok(report)
}

fn build_media_rewrite_map(
    conn: &Connection,
    project_id: &str,
) -> EngineResult<HashMap<String, String>> {
    let media_items = queries::media::list_media_by_project(conn, project_id)?;
    let mut map = HashMap::new();

    for media in media_items {
        let canonical_path = if media.file_path.starts_with('/') {
            media.file_path.clone()
        } else {
            format!("/{}", media.file_path.trim_start_matches('/'))
        };
        map.insert(format!("bds-media://{}", media.id), canonical_path.clone());

        let relative_key = media.file_path.trim_start_matches('/').to_lowercase();
        map.insert(relative_key, canonical_path);
    }

    Ok(map)
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
        GeneratedWriteOutcome::SkippedUnchanged => report.skipped_paths.push(relative_path.to_string()),
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

    let grouped = documents.iter().fold(HashMap::<String, Vec<&crate::render::PagefindDocument>>::new(), |mut acc, doc| {
        acc.entry(doc.language.clone()).or_default().push(doc);
        acc
    });

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
            let files = index.get_files().await.map_err(|error| EngineError::Parse(error.to_string()))?;
            for file in files {
                let relative = file.filename.to_string_lossy().trim_start_matches('/').to_string();
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

fn render_languages(metadata: &ProjectMetadata) -> Vec<String> {
    let main = metadata.main_language.clone().unwrap_or_else(|| "en".to_string());
    let mut languages = vec![main.clone()];
    for language in &metadata.blog_languages {
        if !languages.iter().any(|existing| existing.eq_ignore_ascii_case(language)) {
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
        if let Ok(translation) = queries::post_translation::get_post_translation_by_post_and_language(conn, &source.post.id, language) {
            let raw = std::fs::read_to_string(data_dir.join(translation.file_path.trim_start_matches('/')))
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

fn build_rss_xml(metadata: &ProjectMetadata, posts: &[PublishedPostSource], language: &str) -> String {
    let base_url = metadata.public_url.as_deref().unwrap_or("").trim_end_matches('/');
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
        let url = format!("{base_url}{}", build_canonical_post_path(&source.post, language, metadata.main_language.as_deref().unwrap_or("en")));
        let published = timestamp(source.post.published_at.unwrap_or(source.post.created_at)).unwrap_or_else(Utc::now);
        xml.push("    <item>".to_string());
        xml.push(format!("      <title>{}</title>", escape_xml(&source.post.title)));
        xml.push(format!("      <link>{url}</link>"));
        xml.push(format!("      <guid isPermaLink=\"true\">{url}</guid>"));
        xml.push(format!("      <pubDate>{}</pubDate>", published.format("%a, %d %b %Y %H:%M:%S GMT")));
        if let Some(author) = &source.post.author {
            xml.push(format!("      <author>{}</author>", escape_xml(author)));
        }
        xml.push(format!("      <dc:language>{}</dc:language>", escape_xml(language)));
        let html = render_markdown_to_html(&source.body_markdown);
        xml.push(format!("      <description><![CDATA[{html}]]></description>"));
        xml.push(format!("      <content:encoded><![CDATA[{html}]]></content:encoded>"));
        for category in &source.post.categories {
            xml.push(format!("      <category>{}</category>", escape_xml(category)));
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

fn build_atom_xml(metadata: &ProjectMetadata, posts: &[PublishedPostSource], language: &str) -> String {
    let base_url = metadata.public_url.as_deref().unwrap_or("").trim_end_matches('/');
    let updated = posts
        .iter()
        .filter_map(|post| timestamp(post.post.published_at.unwrap_or(post.post.created_at)))
        .max()
        .unwrap_or_else(Utc::now);
    let mut xml = vec![
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_string(),
        "<feed xmlns=\"http://www.w3.org/2005/Atom\">".to_string(),
        format!("  <title>{}</title>", escape_xml(&metadata.name)),
        format!("  <subtitle>{}</subtitle>", escape_xml(metadata.description.as_deref().unwrap_or(""))),
        format!("  <id>{base_url}/</id>"),
        format!("  <link href=\"{base_url}/\" rel=\"alternate\" />"),
        format!("  <link href=\"{base_url}/atom.xml\" rel=\"self\" />"),
        format!("  <updated>{}</updated>", updated.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
    ];

    for source in posts {
        let url = format!("{base_url}{}", build_canonical_post_path(&source.post, language, metadata.main_language.as_deref().unwrap_or("en")));
        let published = timestamp(source.post.published_at.unwrap_or(source.post.created_at)).unwrap_or_else(Utc::now);
        let html = render_markdown_to_html(&source.body_markdown);
        xml.push(format!("  <entry xml:lang=\"{}\">", escape_xml(language)));
        xml.push(format!("    <title>{}</title>", escape_xml(&source.post.title)));
        xml.push(format!("    <id>{url}</id>"));
        xml.push(format!("    <link href=\"{url}\" />"));
        xml.push(format!("    <updated>{}</updated>", published.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)));
        xml.push(format!("    <published>{}</published>", published.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)));
        if let Some(author) = &source.post.author {
            xml.push(format!("    <author><name>{}</name></author>", escape_xml(author)));
        }
        xml.push(format!("    <summary type=\"xhtml\"><div xmlns=\"http://www.w3.org/1999/xhtml\">{html}</div></summary>"));
        xml.push(format!("    <content type=\"xhtml\"><div xmlns=\"http://www.w3.org/1999/xhtml\">{html}</div></content>"));
        for category in &source.post.categories {
            xml.push(format!("    <category term=\"{}\" />", escape_xml(category)));
        }
        for tag in &source.post.tags {
            xml.push(format!("    <category term=\"{}\" />", escape_xml(tag)));
        }
        xml.push("  </entry>".to_string());
    }

    xml.push("</feed>".to_string());
    xml.join("\n")
}

fn build_sitemap_xml(metadata: &ProjectMetadata, posts: &[PublishedPostSource], language: &str) -> String {
    let base_url = metadata.public_url.as_deref().unwrap_or("").trim_end_matches('/');
    let index_lastmod = posts
        .iter()
        .filter_map(|post| timestamp(post.post.published_at.unwrap_or(post.post.created_at)))
        .max()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let mut xml = vec![
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_string(),
        "<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">".to_string(),
        "  <url>".to_string(),
        format!("    <loc>{base_url}/</loc>"),
        format!("    <lastmod>{index_lastmod}</lastmod>"),
        "    <changefreq>daily</changefreq>".to_string(),
        "    <priority>1.0</priority>".to_string(),
        "  </url>".to_string(),
    ];

    for source in posts {
        let url = format!("{base_url}{}", build_canonical_post_path(&source.post, language, metadata.main_language.as_deref().unwrap_or("en")));
        let lastmod = timestamp(source.post.published_at.unwrap_or(source.post.created_at))
            .unwrap_or_else(Utc::now)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        xml.push("  <url>".to_string());
        xml.push(format!("    <loc>{url}</loc>"));
        xml.push(format!("    <lastmod>{lastmod}</lastmod>"));
        xml.push("    <changefreq>weekly</changefreq>".to_string());
        xml.push("    <priority>0.8</priority>".to_string());
        xml.push("  </url>".to_string());
    }

    xml.push("</urlset>".to_string());
    xml.join("\n")
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