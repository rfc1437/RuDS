use std::collections::{BTreeMap, HashMap, HashSet};
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
    GeneratedWriteOutcome, PostLanguageVariant, build_calendar_json, build_canonical_post_path,
    build_site_render_artifacts, build_site_section_render_artifacts,
    build_targeted_site_section_render_artifacts, select_post_language_variant,
    write_generated_bytes, write_generated_file,
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

impl GenerationSection {
    pub const ALL: [Self; 5] = [
        Self::Core,
        Self::Single,
        Self::Category,
        Self::Tag,
        Self::Date,
    ];
}

impl GenerationReport {
    pub fn append(&mut self, mut other: Self) {
        self.written_paths.append(&mut other.written_paths);
        self.skipped_paths.append(&mut other.skipped_paths);
        self.deleted_paths.append(&mut other.deleted_paths);
    }
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

/// Forget stored generated-file hashes so the next render writes every
/// artifact while repopulating the cache with its current content hash.
pub fn clear_generation_cache(conn: &Connection, project_id: &str) -> EngineResult<usize> {
    use crate::db::schema::generated_file_hashes;
    use diesel::prelude::*;

    Ok(conn.with(|connection| {
        diesel::delete(
            generated_file_hashes::table.filter(generated_file_hashes::project_id.eq(project_id)),
        )
        .execute(connection)
    })?)
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
    for section in GenerationSection::ALL {
        report.append(render_site_section_with_progress(
            conn,
            output_dir,
            project_id,
            metadata,
            posts,
            section,
            &mut on_page,
            || false,
        )?);
    }
    report.append(build_site_search_index(
        conn, output_dir, project_id, metadata,
    )?);
    Ok(report)
}

#[expect(
    clippy::too_many_arguments,
    reason = "section rendering uses the existing generation context and two callbacks"
)]
pub fn render_site_section_with_progress(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    section: GenerationSection,
    mut on_page: impl FnMut(usize, usize, &str),
    mut is_cancelled: impl FnMut() -> bool,
) -> EngineResult<GenerationReport> {
    if is_cancelled() {
        return Err(EngineError::Validation("cancelled".to_string()));
    }
    let data_dir = project_data_dir(output_dir);
    let input_posts = posts
        .iter()
        .map(|source| (source.post.clone(), source.body_markdown.clone()))
        .collect::<Vec<_>>();
    let artifacts = build_site_section_render_artifacts(
        conn,
        &data_dir,
        project_id,
        metadata,
        &input_posts,
        section,
    )
    .map_err(|error| EngineError::Parse(error.to_string()))?;
    let mut report = GenerationReport::default();
    let total_pages = artifacts.pages.len();
    for (index, page) in artifacts.pages.iter().enumerate() {
        if is_cancelled() {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
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

    if section == GenerationSection::Core {
        write_core_outputs(
            conn,
            output_dir,
            project_id,
            metadata,
            &data_dir,
            posts,
            &artifacts.route_manifest,
            None,
            &mut report,
            &mut is_cancelled,
        )?;
    }
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
    let input_posts = posts
        .iter()
        .map(|source| (source.post.clone(), source.body_markdown.clone()))
        .collect::<Vec<_>>();
    let artifacts =
        build_site_render_artifacts(conn, &data_dir, project_id, metadata, &input_posts)
            .map_err(|error| EngineError::Parse(error.to_string()))?;
    let mut report = GenerationReport::default();
    let expected_paths = expected_paths_for_sections(metadata, &artifacts.pages, &section_set);

    for section in sections {
        report.append(render_site_section_with_progress(
            conn,
            output_dir,
            project_id,
            metadata,
            posts,
            *section,
            |_current, _total, _url| {},
            || false,
        )?);
    }

    remove_extra_section_paths(output_dir, &expected_paths, &section_set, &mut report)?;
    report.append(build_site_search_index(
        conn, output_dir, project_id, metadata,
    )?);

    Ok(report)
}

#[expect(
    clippy::too_many_arguments,
    reason = "targeted apply adds its validation report to the existing generation context"
)]
pub fn apply_validation_section_with_progress(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    validation: &SiteValidationReport,
    section: GenerationSection,
    mut on_page: impl FnMut(usize, usize, &str),
    mut is_cancelled: impl FnMut() -> bool,
) -> EngineResult<GenerationReport> {
    if is_cancelled() {
        return Err(EngineError::Validation("cancelled".to_string()));
    }
    let data_dir = project_data_dir(output_dir);
    let input_posts = posts
        .iter()
        .map(|source| (source.post.clone(), source.body_markdown.clone()))
        .collect::<Vec<_>>();
    let requested = validation
        .missing_pages
        .iter()
        .chain(validation.stale_pages.iter())
        .cloned()
        .collect::<HashSet<_>>();
    let fallback = validation
        .missing_pages
        .iter()
        .chain(validation.extra_pages.iter())
        .chain(validation.stale_pages.iter())
        .any(|path| classify_generated_path(path).is_none());
    let artifacts = if fallback {
        build_site_section_render_artifacts(
            conn,
            &data_dir,
            project_id,
            metadata,
            &input_posts,
            section,
        )
    } else {
        build_targeted_site_section_render_artifacts(
            conn,
            &data_dir,
            project_id,
            metadata,
            &input_posts,
            section,
            &requested,
        )
    }
    .map_err(|error| EngineError::Parse(error.to_string()))?;
    let mut report = GenerationReport::default();
    let total_pages = artifacts.pages.len();
    for (index, page) in artifacts.pages.iter().enumerate() {
        if is_cancelled() {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
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

    if section == GenerationSection::Core {
        write_core_outputs(
            conn,
            output_dir,
            project_id,
            metadata,
            &data_dir,
            posts,
            &artifacts.route_manifest,
            (!fallback).then_some(&requested),
            &mut report,
            &mut is_cancelled,
        )?;
    }

    for path in &validation.extra_pages {
        if is_cancelled() {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
        let owned_by_section = classify_generated_path(path)
            .map_or(section == GenerationSection::Core, |owner| owner == section);
        if owned_by_section && output_dir.join(path).is_file() {
            std::fs::remove_file(output_dir.join(path)).map_err(EngineError::Io)?;
            report.deleted_paths.push(path.clone());
        }
    }
    Ok(report)
}

#[expect(
    clippy::too_many_arguments,
    reason = "generation context is existing domain data"
)]
fn write_core_outputs(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    data_dir: &Path,
    published_posts: &[PublishedPostSource],
    route_manifest: &[crate::render::SitePage],
    requested: Option<&HashSet<String>>,
    report: &mut GenerationReport,
    is_cancelled: &mut impl FnMut() -> bool,
) -> EngineResult<()> {
    if requested.is_none() {
        write_bundled_site_assets(conn, output_dir, project_id, report)?;
    }
    let mut outputs = vec![(
        "calendar.json".to_string(),
        build_calendar_json(
            &published_posts
                .iter()
                .map(|source| source.post.clone())
                .collect::<Vec<_>>(),
        )?,
    )];
    for render_language in render_languages(metadata) {
        let localized_posts =
            localized_sources(conn, data_dir, published_posts, &render_language, metadata)?;
        let is_main = render_language == metadata.main_language.as_deref().unwrap_or("en");
        let prefix = if is_main {
            String::new()
        } else {
            format!("{render_language}/")
        };
        let mut feed_posts = if is_main {
            published_posts.to_vec()
        } else {
            localized_posts
                .iter()
                .filter(|source| {
                    source
                        .post
                        .language
                        .as_deref()
                        .is_some_and(|language| language.eq_ignore_ascii_case(&render_language))
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        sort_published_sources(&mut feed_posts);
        outputs.push((
            format!("{prefix}rss.xml"),
            build_rss_xml(metadata, &feed_posts, &render_language),
        ));
        outputs.push((
            format!("{prefix}atom.xml"),
            build_atom_xml(metadata, &feed_posts, &render_language),
        ));
        if is_main {
            let category_settings = load_category_settings(data_dir);
            let mut sitemap_posts = published_posts.to_vec();
            sort_published_sources(&mut sitemap_posts);
            let sitemap_list_posts = filter_posts_for_lists(&sitemap_posts, &category_settings);
            outputs.push((
                "sitemap.xml".to_string(),
                build_sitemap_xml(
                    metadata,
                    route_manifest,
                    &sitemap_posts,
                    &sitemap_list_posts,
                    &render_language,
                ),
            ));
        }
    }
    for (path, content) in outputs {
        if is_cancelled() {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
        if requested.is_none_or(|requested| requested.contains(&path)) {
            write_out(conn, output_dir, project_id, &path, &content, report)?;
        }
    }
    Ok(())
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

pub fn build_site_search_index(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
) -> EngineResult<GenerationReport> {
    build_site_search_index_with_progress(
        conn,
        output_dir,
        project_id,
        metadata,
        |_current, _total, _path| {},
        || false,
    )
}

pub fn build_site_search_index_with_progress(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    mut on_file: impl FnMut(usize, usize, &str),
    mut is_cancelled: impl FnMut() -> bool,
) -> EngineResult<GenerationReport> {
    let mut documents = Vec::new();
    if output_dir.exists() {
        for entry in WalkDir::new(output_dir).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() {
                continue;
            }
            let relative_path = entry
                .path()
                .strip_prefix(output_dir)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            if !relative_path.ends_with(".html")
                || relative_path.starts_with("pagefind/")
                || relative_path.contains("/pagefind/")
            {
                continue;
            }
            let language = render_languages(metadata)
                .into_iter()
                .find(|language| relative_path.starts_with(&format!("{language}/")))
                .unwrap_or_else(|| {
                    metadata
                        .main_language
                        .clone()
                        .unwrap_or_else(|| "en".into())
                });
            documents.push(crate::render::PagefindDocument {
                language,
                url_path: String::new(),
                html: std::fs::read_to_string(entry.path()).map_err(EngineError::Io)?,
                relative_path,
            });
        }
    }
    documents.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(EngineError::Io)?;

    let grouped = documents.iter().fold(
        BTreeMap::<String, Vec<&crate::render::PagefindDocument>>::new(),
        |mut acc, doc| {
            acc.entry(doc.language.clone()).or_default().push(doc);
            acc
        },
    );
    let mut outputs = Vec::new();
    for (language, docs) in grouped {
        if is_cancelled() {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
        let config = PagefindServiceConfig::builder()
            .keep_index_url(true)
            .force_language(language.clone())
            .build();
        let mut index = PagefindIndex::new(Some(config))
            .map_err(|error| EngineError::Parse(error.to_string()))?;
        let output_prefix = if language == metadata.main_language.as_deref().unwrap_or("en") {
            "pagefind".to_string()
        } else {
            format!("{language}/pagefind")
        };
        runtime.block_on(async {
            for doc in docs {
                if is_cancelled() {
                    return Err(EngineError::Validation("cancelled".to_string()));
                }
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
                outputs.push((
                    format!(
                        "{output_prefix}/{}",
                        file.filename.to_string_lossy().trim_start_matches('/')
                    ),
                    file.contents,
                ));
            }
            Ok::<(), EngineError>(())
        })?;
    }
    outputs.sort_by(|left, right| {
        left.0
            .ends_with("pagefind-entry.json")
            .cmp(&right.0.ends_with("pagefind-entry.json"))
            .then_with(|| left.0.cmp(&right.0))
    });
    let total = outputs.len();
    let mut report = GenerationReport::default();
    let expected = outputs
        .iter()
        .map(|(relative, _)| relative.clone())
        .collect::<HashSet<_>>();
    for (index, (relative, contents)) in outputs.into_iter().enumerate() {
        if is_cancelled() {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
        match write_generated_bytes(conn, output_dir, project_id, &relative, &contents)
            .map_err(|error| EngineError::Parse(error.to_string()))?
        {
            GeneratedWriteOutcome::Written => report.written_paths.push(relative.clone()),
            GeneratedWriteOutcome::SkippedUnchanged => report.skipped_paths.push(relative.clone()),
        }
        on_file(index + 1, total, &relative);
    }
    for language in render_languages(metadata) {
        let prefix = if language == metadata.main_language.as_deref().unwrap_or("en") {
            "pagefind".to_string()
        } else {
            format!("{language}/pagefind")
        };
        let index_dir = output_dir.join(&prefix);
        if !index_dir.exists() {
            continue;
        }
        for entry in WalkDir::new(&index_dir).into_iter().filter_map(Result::ok) {
            if is_cancelled() {
                return Err(EngineError::Validation("cancelled".to_string()));
            }
            if !entry.file_type().is_file() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(output_dir)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            if !expected.contains(&relative) {
                std::fs::remove_file(entry.path()).map_err(EngineError::Io)?;
                report.deleted_paths.push(relative);
            }
        }
    }
    Ok(report)
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
            expected.insert(format!("{prefix}rss.xml"));
            expected.insert(format!("{prefix}atom.xml"));
        }
        expected.insert("sitemap.xml".to_string());
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

pub(crate) fn classify_generated_path(path: &str) -> Option<GenerationSection> {
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
        ["index.html"] | ["404.html"] | ["page", _, "index.html"] => Some(GenerationSection::Core),
        ["category", ..] => Some(GenerationSection::Category),
        ["tag", ..] => Some(GenerationSection::Tag),
        [year, "index.html"] if is_year_segment(year) => Some(GenerationSection::Date),
        [year, "page", _, "index.html"] if is_year_segment(year) => Some(GenerationSection::Date),
        [year, month, "index.html"] if is_year_segment(year) && is_month_segment(month) => {
            Some(GenerationSection::Date)
        }
        [year, month, "page", _, "index.html"]
            if is_year_segment(year) && is_month_segment(month) =>
        {
            Some(GenerationSection::Date)
        }
        [year, month, day, "index.html"]
            if is_year_segment(year) && is_month_segment(month) && is_day_segment(day) =>
        {
            Some(GenerationSection::Date)
        }
        [year, month, day, "page", _, "index.html"]
            if is_year_segment(year) && is_month_segment(month) && is_day_segment(day) =>
        {
            Some(GenerationSection::Date)
        }
        [year, month, day, _slug, "index.html"]
            if is_year_segment(year) && is_month_segment(month) && is_day_segment(day) =>
        {
            Some(GenerationSection::Single)
        }
        [_slug, "index.html"] => Some(GenerationSection::Core),
        _ => None,
    }
}

fn has_language_prefix(parts: &[&str]) -> bool {
    match parts {
        [first, second, ..] => {
            matches!(*first, "de" | "en" | "es" | "fr" | "it")
                && (*second == "index.html"
                    || *second == "404.html"
                    || *second == "page"
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
    GenerationSection::ALL.to_vec()
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
        let translation = queries::post_translation::get_post_translation_by_post_and_language(
            conn,
            &source.post.id,
            language,
        )
        .ok()
        .filter(|translation| {
            !translation.file_path.trim().is_empty()
                && data_dir
                    .join(translation.file_path.trim_start_matches('/'))
                    .is_file()
        });
        match select_post_language_variant(
            &source.post,
            language,
            main_language,
            translation.is_some(),
        ) {
            Some(PostLanguageVariant::Base) => localized.push(source.clone()),
            Some(PostLanguageVariant::Translation) => {
                let Some(translation) = translation else {
                    continue;
                };
                let raw = std::fs::read_to_string(
                    data_dir.join(translation.file_path.trim_start_matches('/')),
                )
                .map_err(EngineError::Io)?;
                let (_, body) = crate::util::frontmatter::read_translation_file(&raw)
                    .map_err(EngineError::Parse)?;
                let mut translated_post = source.post.clone();
                translated_post.id = translation.id.clone();
                translated_post.title = translation.title.clone();
                translated_post.excerpt = translation.excerpt.clone();
                translated_post.language = Some(translation.language.clone());
                translated_post.status = translation.status.clone();
                translated_post.file_path = translation.file_path.clone();
                translated_post.updated_at = translation.updated_at;
                translated_post.published_at =
                    translation.published_at.or(source.post.published_at);
                localized.push(PublishedPostSource {
                    post: translated_post,
                    body_markdown: body,
                });
            }
            None => {}
        }
    }
    sort_published_sources(&mut localized);
    Ok(localized)
}

fn sort_published_sources(posts: &mut [PublishedPostSource]) {
    posts.sort_by(|left, right| {
        right
            .post
            .created_at
            .cmp(&left.post.created_at)
            .then_with(|| right.post.published_at.cmp(&left.post.published_at))
            .then_with(|| left.post.slug.cmp(&right.post.slug))
    });
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
                    .is_some_and(|settings| !settings.render_in_lists)
            })
        })
        .cloned()
        .collect()
}

pub(crate) fn build_rss_xml(
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    language: &str,
) -> String {
    let base_url = metadata
        .public_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let mut xml = format!(
        "<rss><channel><title>{} ({})</title>",
        escape_xml(&metadata.name),
        escape_xml(language)
    );

    for source in posts {
        let url = post_absolute_url(base_url, metadata, source, language);
        xml.push_str(&format!(
            "<item><title>{}</title><link>{url}</link></item>",
            escape_xml(&source.post.title)
        ));
    }
    xml.push_str("</channel></rss>");
    xml
}

pub(crate) fn build_atom_xml(
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    language: &str,
) -> String {
    let base_url = metadata
        .public_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let mut xml = format!(
        "<feed><title>{} ({})</title>",
        escape_xml(&metadata.name),
        escape_xml(language)
    );

    for source in posts {
        let url = post_absolute_url(base_url, metadata, source, language);
        xml.push_str(&format!(
            "<entry><title>{}</title><id>{url}</id></entry>",
            escape_xml(&source.post.title)
        ));
    }
    xml.push_str("</feed>");
    xml
}

fn post_absolute_url(
    base_url: &str,
    metadata: &ProjectMetadata,
    source: &PublishedPostSource,
    language: &str,
) -> String {
    format!(
        "{base_url}{}/",
        build_canonical_post_path(
            &source.post,
            language,
            metadata.main_language.as_deref().unwrap_or("en")
        )
        .trim_end_matches('/')
    )
}

fn build_sitemap_xml(
    metadata: &ProjectMetadata,
    pages: &[crate::render::SitePage],
    posts: &[PublishedPostSource],
    list_posts: &[PublishedPostSource],
    language: &str,
) -> String {
    let base_url = metadata
        .public_url
        .as_deref()
        .unwrap_or("")
        .trim_end_matches('/');
    let main_language = metadata.main_language.as_deref().unwrap_or("en");
    let languages = render_languages(metadata);
    let index_lastmod = list_posts
        .first()
        .and_then(|post| timestamp(post.post.updated_at))
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let mut post_lastmod_by_path = HashMap::new();
    for source in posts {
        let Some(lastmod) = timestamp(source.post.updated_at) else {
            continue;
        };
        let lastmod = lastmod.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        post_lastmod_by_path.insert(
            build_canonical_post_path(&source.post, language, main_language),
            lastmod.clone(),
        );
        if source
            .post
            .categories
            .iter()
            .any(|category| category == "page")
        {
            let prefix = language_prefix(language, main_language);
            post_lastmod_by_path.insert(format!("{prefix}/{}", source.post.slug), lastmod.clone());
        }
    }
    let page_groups = group_pages_by_logical_path(pages, &languages, main_language);

    let mut xml = vec![
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>".to_string(),
        "<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\" xmlns:xhtml=\"http://www.w3.org/1999/xhtml\">".to_string(),
    ];

    let mut language_pages = pages
        .iter()
        .filter(|page| page.language == language)
        .collect::<Vec<_>>();
    language_pages.sort_by(|left, right| {
        let left_key = logical_page_key(&left.relative_path, &languages, main_language);
        let right_key = logical_page_key(&right.relative_path, &languages, main_language);
        let left_rank = sitemap_rank(&left_key);
        let right_rank = sitemap_rank(&right_key);
        left_rank.cmp(&right_rank).then_with(|| {
            if (4..=6).contains(&left_rank) {
                right_key.cmp(&left_key)
            } else {
                std::cmp::Ordering::Equal
            }
        })
    });

    for page in language_pages {
        let logical_key = logical_page_key(&page.relative_path, &languages, main_language);
        if logical_key.contains("/page/") && !logical_key.starts_with("page/") {
            continue;
        }
        let url_path = sitemap_url_path(&page.url_path);
        let url = format!("{base_url}{url_path}");
        let alternates = page_groups.get(&logical_key);
        let lastmod = post_lastmod_by_path
            .get(&page.url_path)
            .cloned()
            .unwrap_or_else(|| index_lastmod.clone());
        let is_home = page.url_path == language_root_url_path(language, main_language);
        let (changefreq, priority) = sitemap_metadata(&logical_key, is_home);
        let rank = sitemap_rank(&logical_key);
        xml.push("  <url>".to_string());
        xml.push(format!("    <loc>{}</loc>", escape_xml(&url)));
        xml.push(format!("    <lastmod>{lastmod}</lastmod>"));
        xml.push(format!("    <changefreq>{}</changefreq>", changefreq));
        xml.push(format!("    <priority>{}</priority>", priority));
        if !matches!(rank, 2 | 3) {
            for alternate_language in &languages {
                let alternate_path = if alternate_language == main_language {
                    page.url_path.clone()
                } else if page.url_path == "/" {
                    format!("/{alternate_language}")
                } else {
                    format!("/{alternate_language}{}", page.url_path)
                };
                let href = format!("{base_url}{}", sitemap_url_path(&alternate_path));
                xml.push(format!(
                    "    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{}\" />",
                    escape_xml(alternate_language),
                    escape_xml(&href),
                ));
            }
            let href = format!("{base_url}{}", sitemap_url_path(&page.url_path));
            xml.push(format!(
                "    <xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"{}\" />",
                escape_xml(&href),
            ));
        } else if let Some(alternates) = alternates {
            for alternate in alternates {
                let href = format!("{base_url}{}", sitemap_url_path(&alternate.url_path));
                xml.push(format!(
                    "    <xhtml:link rel=\"alternate\" hreflang=\"{}\" href=\"{}\" />",
                    escape_xml(&alternate.language),
                    escape_xml(&href),
                ));
            }
            if let Some(default_page) = alternates
                .iter()
                .find(|alternate| alternate.language == main_language)
            {
                let href = format!("{base_url}{}", sitemap_url_path(&default_page.url_path));
                xml.push(format!(
                    "    <xhtml:link rel=\"alternate\" hreflang=\"x-default\" href=\"{}\" />",
                    escape_xml(&href),
                ));
            }
        }
        xml.push("  </url>".to_string());
    }

    xml.push("</urlset>".to_string());
    format!("{}\n", xml.join("\n"))
}

fn sitemap_url_path(path: &str) -> String {
    if path == "/" {
        path.to_string()
    } else {
        format!("{}/", path.trim_end_matches('/'))
    }
}

fn sitemap_metadata(logical_path: &str, is_home: bool) -> (&'static str, &'static str) {
    if is_home {
        return ("daily", "1.0");
    }
    if logical_path.starts_with("page/") {
        return ("daily", "0.9");
    }
    let parts = logical_path.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        [year, month, day, _slug, "index.html"]
            if is_year_segment(year) && is_month_segment(month) && is_day_segment(day) =>
        {
            ("monthly", "0.8")
        }
        ["category" | "tag", ..] => ("weekly", "0.6"),
        [year, month, day, "index.html"]
            if is_year_segment(year) && is_month_segment(month) && is_day_segment(day) =>
        {
            ("monthly", "0.4")
        }
        [year, ..] if is_year_segment(year) => ("monthly", "0.5"),
        [_slug, "index.html"] => ("weekly", "0.7"),
        _ => ("weekly", "0.6"),
    }
}

fn sitemap_rank(logical_path: &str) -> u8 {
    let parts = logical_path.split('/').collect::<Vec<_>>();
    match parts.as_slice() {
        ["index.html"] => 0,
        ["page", _, "index.html"] => 1,
        [year, month, day, _slug, "index.html"]
            if is_year_segment(year) && is_month_segment(month) && is_day_segment(day) =>
        {
            2
        }
        [year, "index.html"] if is_year_segment(year) => 4,
        [year, month, "index.html"] if is_year_segment(year) && is_month_segment(month) => 5,
        [year, month, day, "index.html"]
            if is_year_segment(year) && is_month_segment(month) && is_day_segment(day) =>
        {
            6
        }
        [_slug, "index.html"] => 3,
        ["category", ..] => 7,
        ["tag", ..] => 8,
        _ => 9,
    }
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
