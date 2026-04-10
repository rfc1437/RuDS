use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::Connection;

use crate::engine::{EngineError, EngineResult};
use crate::model::{Post, ProjectMetadata};
use crate::render::{
    GeneratedWriteOutcome, build_calendar_json, build_canonical_post_path,
    render_markdown_to_html, render_starter_list_page, render_starter_single_post_page,
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

    let list_input = posts
        .iter()
        .map(|source| (source.post.clone(), source.body_markdown.clone()))
        .collect::<Vec<_>>();
    let index_page = render_starter_list_page(&list_input, metadata, language)
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    write_out(conn, output_dir, project_id, &index_page.relative_path, &index_page.html, &mut report)?;

    for source in posts {
        let rendered = render_starter_single_post_page(&source.post, &source.body_markdown, metadata, language)
            .map_err(|error| EngineError::Parse(error.to_string()))?;
        write_out(conn, output_dir, project_id, &rendered.relative_path, &rendered.html, &mut report)?;
    }

    write_out(
        conn,
        output_dir,
        project_id,
        "calendar.json",
        &build_calendar_json(&posts.iter().map(|source| source.post.clone()).collect::<Vec<_>>())?,
        &mut report,
    )?;

    let rss = build_rss_xml(metadata, posts, language);
    write_out(conn, output_dir, project_id, "rss.xml", &rss, &mut report)?;
    write_out(conn, output_dir, project_id, "feed.xml", &rss, &mut report)?;
    write_out(conn, output_dir, project_id, "atom.xml", &build_atom_xml(metadata, posts, language), &mut report)?;
    write_out(conn, output_dir, project_id, "sitemap.xml", &build_sitemap_xml(metadata, posts, language), &mut report)?;

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
        GeneratedWriteOutcome::SkippedUnchanged => report.skipped_paths.push(relative_path.to_string()),
    }
    Ok(())
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