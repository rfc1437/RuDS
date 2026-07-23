use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::db::DbConnection as Connection;
use walkdir::WalkDir;

use crate::db::queries;
use crate::engine::EngineResult;
use crate::engine::generation::has_published_snapshot;
use crate::model::Post;
use crate::render::{build_canonical_post_path, build_site_route_manifest};

const MTIME_GRANULARITY_TOLERANCE_MS: i64 = 1_000;

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
    let published_posts = load_published_posts(conn, project_id)?;
    let route_manifest = build_site_route_manifest(data_dir, &metadata, &published_posts)
        .map_err(|error| crate::engine::EngineError::Parse(error.to_string()))?;
    crate::engine::generation::refresh_validation_sitemap(
        conn,
        &output_dir,
        project_id,
        data_dir,
        &metadata,
        &published_posts,
        &route_manifest,
    )?;
    let expected = route_manifest
        .into_iter()
        .map(|page| page.relative_path)
        .collect::<HashSet<_>>();

    let mut actual = HashSet::new();
    let mut zero_byte = HashSet::new();
    if output_dir.exists() {
        for entry in WalkDir::new(&output_dir).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() || entry.file_name() != "index.html" {
                continue;
            }
            let path = relative_path(&output_dir, entry.path());
            if entry.metadata().is_ok_and(|metadata| metadata.len() > 0) {
                actual.insert(path);
            } else {
                zero_byte.insert(path);
            }
        }
    }

    let mut missing_pages = expected.difference(&actual).cloned().collect::<Vec<_>>();
    let mut extra_pages = actual.difference(&expected).cloned().collect::<Vec<_>>();
    extra_pages.extend(zero_byte.difference(&expected).cloned());
    let generated_at = queries::generated_file_hash::list_generated_file_hashes(conn, project_id)?
        .into_iter()
        .map(|file| (file.relative_path, file.updated_at))
        .collect::<HashMap<_, _>>();
    let mut stale_pages = stale_post_paths(
        data_dir,
        &output_dir,
        &metadata,
        &published_posts,
        &expected,
        &actual,
        &generated_at,
    );

    missing_pages.sort();
    extra_pages.sort();
    extra_pages.dedup();
    stale_pages.sort();
    stale_pages.dedup();

    Ok(SiteValidationReport {
        missing_pages,
        extra_pages,
        stale_pages,
    })
}

fn stale_post_paths(
    data_dir: &Path,
    output_dir: &Path,
    metadata: &crate::model::ProjectMetadata,
    published_posts: &[Post],
    expected: &HashSet<String>,
    actual: &HashSet<String>,
    generated_at: &HashMap<String, i64>,
) -> Vec<String> {
    let main_language = metadata.main_language.as_deref().unwrap_or("en");
    let mut languages = vec![main_language.to_string()];
    for language in &metadata.blog_languages {
        if !languages
            .iter()
            .any(|known| known.eq_ignore_ascii_case(language))
        {
            languages.push(language.clone());
        }
    }
    let mut stale = Vec::new();

    for post in published_posts {
        let Some(source_modified) = modified_ms(&data_dir.join(&post.file_path)) else {
            continue;
        };
        for language in &languages {
            if language != main_language && post.do_not_translate {
                continue;
            }
            let relative_path = format!(
                "{}/index.html",
                build_canonical_post_path(post, language, main_language).trim_start_matches('/')
            );
            if !expected.contains(&relative_path) || !actual.contains(&relative_path) {
                continue;
            }
            let Some(output_modified) = modified_ms(&output_dir.join(&relative_path)) else {
                continue;
            };
            let effective_generated = output_modified.max(
                generated_at
                    .get(&relative_path)
                    .copied()
                    .unwrap_or_default(),
            );
            if source_modified > effective_generated + MTIME_GRANULARITY_TOLERANCE_MS {
                stale.push(relative_path);
            }
        }
    }
    stale
}

fn modified_ms(path: &Path) -> Option<i64> {
    let modified = path.metadata().ok()?.modified().ok()?;
    Some(system_time_ms(modified))
}

fn system_time_ms(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or_default()
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn generated_output_dir(data_dir: &Path) -> std::path::PathBuf {
    let html_dir = data_dir.join("html");
    if html_dir.exists() {
        html_dir
    } else {
        data_dir.to_path_buf()
    }
}

fn load_published_posts(conn: &Connection, project_id: &str) -> EngineResult<Vec<Post>> {
    Ok(queries::post::list_posts_by_project(conn, project_id)?
        .into_iter()
        .filter(has_published_snapshot)
        .collect())
}
