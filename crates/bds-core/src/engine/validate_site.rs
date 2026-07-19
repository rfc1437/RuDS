use std::collections::HashSet;
use std::path::Path;

use crate::db::DbConnection as Connection;
use walkdir::WalkDir;

use crate::db::queries;
use crate::engine::{EngineError, EngineResult};
use crate::model::Post;
use crate::render::build_site_render_artifacts;
use crate::util::file_hash;

#[derive(Debug, Clone, Default)]
pub struct SiteValidationReport {
    pub missing_pages: Vec<String>,
    pub extra_pages: Vec<String>,
    pub stale_pages: Vec<String>,
}

pub fn validate_site(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<SiteValidationReport> {
    let metadata = crate::engine::meta::read_project_json(data_dir)?;
    let output_dir = generated_output_dir(data_dir);
    let published_posts = load_published_posts(data_dir, conn, project_id)?;
    let artifacts =
        build_site_render_artifacts(conn, data_dir, project_id, &metadata, &published_posts)
            .map_err(|error| EngineError::Parse(error.to_string()))?;

    let mut expected = artifacts
        .pages
        .iter()
        .map(|page| page.relative_path.clone())
        .collect::<HashSet<_>>();
    expected.insert("calendar.json".to_string());
    for language in render_languages(&metadata) {
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
        expected.insert(format!("{prefix}rss.xml"));
        expected.insert(format!("{prefix}feed.xml"));
        expected.insert(format!("{prefix}atom.xml"));
        expected.insert(format!("{prefix}sitemap.xml"));
    }

    let mut actual = HashSet::new();
    if output_dir.exists() {
        for entry in WalkDir::new(&output_dir).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let rel = entry
                .path()
                .strip_prefix(&output_dir)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            if rel.starts_with("meta/")
                || rel.starts_with("posts/")
                || rel.starts_with("media/")
                || rel.starts_with("assets/")
            {
                continue;
            }
            if rel.starts_with("pagefind") || rel.contains("/pagefind/") {
                continue;
            }
            if rel.ends_with(".html") || rel.ends_with(".xml") || rel.ends_with(".json") {
                actual.insert(rel);
            }
        }
    }

    let mut missing_pages = expected.difference(&actual).cloned().collect::<Vec<_>>();
    let mut extra_pages = actual.difference(&expected).cloned().collect::<Vec<_>>();

    let mut stale_pages = Vec::new();
    for rel in expected.intersection(&actual) {
        if let Ok(stored) =
            queries::generated_file_hash::get_generated_file_hash(conn, project_id, rel)
        {
            let actual_hash = file_hash(&output_dir.join(rel))?;
            if actual_hash != stored.content_hash {
                stale_pages.push(rel.clone());
            }
        }
    }

    missing_pages.sort();
    extra_pages.sort();
    stale_pages.sort();

    Ok(SiteValidationReport {
        missing_pages,
        extra_pages,
        stale_pages,
    })
}

fn generated_output_dir(data_dir: &Path) -> std::path::PathBuf {
    let html_dir = data_dir.join("html");
    if html_dir.exists() {
        html_dir
    } else {
        data_dir.to_path_buf()
    }
}

fn load_published_posts(
    data_dir: &Path,
    conn: &Connection,
    project_id: &str,
) -> EngineResult<Vec<(Post, String)>> {
    let posts = queries::post::list_posts_by_project(conn, project_id)?;
    let mut published = Vec::new();
    for post in posts
        .into_iter()
        .filter(crate::engine::generation::has_published_snapshot)
    {
        if let Some(source) = crate::engine::generation::load_published_post_source(data_dir, post)?
        {
            published.push((source.post, source.body_markdown));
        }
    }
    Ok(published)
}

fn render_languages(metadata: &crate::model::ProjectMetadata) -> Vec<String> {
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
