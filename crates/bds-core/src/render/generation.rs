use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::{Datelike, TimeZone, Utc};
use rusqlite::Connection;
use serde::Serialize;

use crate::db::queries::generated_file_hash as qhash;
use crate::model::{GeneratedFileHash, Post};
use crate::util::{atomic_write_str, content_hash, file_hash, now_unix_ms};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratedWriteOutcome {
    Written,
    SkippedUnchanged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CalendarArchiveData {
    pub years: BTreeMap<String, usize>,
    pub months: BTreeMap<String, usize>,
    pub days: BTreeMap<String, usize>,
}

pub fn write_generated_file(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    relative_path: &str,
    content: &str,
) -> Result<GeneratedWriteOutcome, Box<dyn std::error::Error + Send + Sync>> {
    let hash = content_hash(content.as_bytes());
    let target_path = output_dir.join(relative_path);
    if let Ok(existing) = qhash::get_generated_file_hash(conn, project_id, relative_path)
        && existing.content_hash == hash
        && target_path.exists()
        && file_hash(&target_path)? == hash
    {
        return Ok(GeneratedWriteOutcome::SkippedUnchanged);
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    atomic_write_str(&target_path, content)?;

    qhash::upsert_generated_file_hash(
        conn,
        &GeneratedFileHash {
            project_id: project_id.to_string(),
            relative_path: relative_path.to_string(),
            content_hash: hash,
            updated_at: now_unix_ms(),
        },
    )?;

    Ok(GeneratedWriteOutcome::Written)
}

pub fn write_generated_bytes(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    relative_path: &str,
    content: &[u8],
) -> Result<GeneratedWriteOutcome, Box<dyn std::error::Error + Send + Sync>> {
    let hash = content_hash(content);
    let target_path = output_dir.join(relative_path);
    if let Ok(existing) = qhash::get_generated_file_hash(conn, project_id, relative_path)
        && existing.content_hash == hash
        && target_path.exists()
        && file_hash(&target_path)? == hash
    {
        return Ok(GeneratedWriteOutcome::SkippedUnchanged);
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    crate::util::atomic_write(&target_path, content)?;

    qhash::upsert_generated_file_hash(
        conn,
        &GeneratedFileHash {
            project_id: project_id.to_string(),
            relative_path: relative_path.to_string(),
            content_hash: hash,
            updated_at: now_unix_ms(),
        },
    )?;

    Ok(GeneratedWriteOutcome::Written)
}

pub fn build_core_generation_paths(main_language: &str, blog_languages: &[String]) -> Vec<String> {
    let mut paths = vec![
        "index.html".to_string(),
        "sitemap.xml".to_string(),
        "feed.xml".to_string(),
        "atom.xml".to_string(),
        "calendar.json".to_string(),
    ];

    for language in blog_languages {
        if language != main_language {
            paths.push(format!("{language}/index.html"));
            paths.push(format!("{language}/feed.xml"));
            paths.push(format!("{language}/atom.xml"));
        }
    }

    paths
}

pub fn build_calendar_archive_data(posts: &[Post]) -> CalendarArchiveData {
    let mut years = BTreeMap::new();
    let mut months = BTreeMap::new();
    let mut days = BTreeMap::new();

    for post in posts {
        let timestamp_ms = post.published_at.unwrap_or(post.created_at);
        let Some(created_at) = Utc.timestamp_millis_opt(timestamp_ms).single() else {
            continue;
        };

        let year = created_at.year().to_string();
        let month = format!("{year}-{:02}", created_at.month());
        let day = format!("{month}-{:02}", created_at.day());

        *years.entry(year).or_insert(0) += 1;
        *months.entry(month).or_insert(0) += 1;
        *days.entry(day).or_insert(0) += 1;
    }

    CalendarArchiveData {
        years,
        months,
        days,
    }
}

pub fn build_calendar_json(posts: &[Post]) -> serde_json::Result<String> {
    serde_json::to_string_pretty(&build_calendar_archive_data(posts))
}
