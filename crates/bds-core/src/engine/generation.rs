use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use crate::db::DbConnection as Connection;
use chrono::{DateTime, TimeZone, Utc};
use pagefind::api::PagefindIndex;
use pagefind::options::PagefindServiceConfig;
use walkdir::WalkDir;

use crate::db::queries;
use crate::engine::site_assets::bundled_site_assets;
use crate::engine::validate_site::SiteValidationReport;
use crate::engine::{EngineError, EngineResult};
use crate::model::{CategorySettings, Post, ProjectMetadata};
use crate::render::{
    GeneratedFileWriter, GeneratedWriteOutcome, build_calendar_json, build_canonical_post_path,
    build_site_render_artifacts_from_context, prepare_site_render_context, write_generated_file,
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

pub struct PreparedSiteGeneration {
    metadata: ProjectMetadata,
    sources: Vec<PublishedPostSource>,
    render: crate::render::SiteRenderContext,
    generated_hashes: Arc<HashMap<String, String>>,
}

pub fn prepare_site_generation(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    sources: &[PublishedPostSource],
) -> EngineResult<PreparedSiteGeneration> {
    let input_posts = sources
        .iter()
        .map(|source| (source.post.clone(), source.body_markdown.clone()))
        .collect::<Vec<_>>();
    let render =
        prepare_site_render_context(conn, data_dir, project_id, metadata, &input_posts, false)
            .map_err(|error| EngineError::Parse(error.to_string()))?;
    let generated_hashes = Arc::new(
        queries::generated_file_hash::list_generated_file_hashes(conn, project_id)?
            .into_iter()
            .map(|hash| (hash.relative_path, hash.content_hash))
            .collect(),
    );
    Ok(PreparedSiteGeneration {
        metadata: metadata.clone(),
        sources: sources.to_vec(),
        render,
        generated_hashes,
    })
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

pub fn generate_starter_site_forced(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    language: &str,
) -> EngineResult<GenerationReport> {
    generate_starter_site_with_progress_mode(
        conn,
        output_dir,
        project_id,
        metadata,
        posts,
        language,
        true,
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
    on_page: impl FnMut(usize, usize, &str),
) -> EngineResult<GenerationReport> {
    generate_starter_site_with_progress_mode(
        conn, output_dir, project_id, metadata, posts, _language, false, on_page,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "full generation adds write mode to the existing generation context"
)]
fn generate_starter_site_with_progress_mode(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    _language: &str,
    force: bool,
    mut on_page: impl FnMut(usize, usize, &str),
) -> EngineResult<GenerationReport> {
    let data_dir = project_data_dir(output_dir);
    let prepared = prepare_site_generation(conn, &data_dir, project_id, metadata, posts)?;
    let mut report = GenerationReport::default();
    for section in GenerationSection::ALL {
        report.append(render_prepared_site_section_with_progress(
            conn,
            output_dir,
            project_id,
            &prepared,
            section,
            force,
            &|_| {},
            &mut on_page,
            || false,
        )?);
    }
    report.append(build_site_search_index_with_progress_mode(
        conn,
        output_dir,
        project_id,
        metadata,
        force,
        |_current, _total, _path| {},
        || false,
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
    on_page_rendered: impl Fn(&str) + Sync,
    mut is_cancelled: impl FnMut() -> bool,
) -> EngineResult<GenerationReport> {
    render_site_section_with_progress_mode(
        conn,
        output_dir,
        project_id,
        metadata,
        posts,
        section,
        false,
        &on_page_rendered,
        &mut on_page,
        &mut is_cancelled,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "forced section rendering adds write mode to the existing generation context"
)]
pub fn render_site_section_forced_with_progress(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    section: GenerationSection,
    mut on_page: impl FnMut(usize, usize, &str),
    on_page_rendered: impl Fn(&str) + Sync,
    mut is_cancelled: impl FnMut() -> bool,
) -> EngineResult<GenerationReport> {
    render_site_section_with_progress_mode(
        conn,
        output_dir,
        project_id,
        metadata,
        posts,
        section,
        true,
        &on_page_rendered,
        &mut on_page,
        &mut is_cancelled,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "section rendering uses the existing generation context, write mode, and callbacks"
)]
fn render_site_section_with_progress_mode(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    section: GenerationSection,
    force: bool,
    on_page_rendered: &(dyn Fn(&str) + Sync),
    mut on_page: impl FnMut(usize, usize, &str),
    mut is_cancelled: impl FnMut() -> bool,
) -> EngineResult<GenerationReport> {
    if is_cancelled() {
        return Err(EngineError::Validation("cancelled".to_string()));
    }
    let data_dir = project_data_dir(output_dir);
    let prepared = prepare_site_generation(conn, &data_dir, project_id, metadata, posts)?;
    render_prepared_site_section_with_progress(
        conn,
        output_dir,
        project_id,
        &prepared,
        section,
        force,
        on_page_rendered,
        &mut on_page,
        &mut is_cancelled,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "prepared section rendering keeps write mode and callbacks"
)]
pub fn render_prepared_site_section_with_progress(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    prepared: &PreparedSiteGeneration,
    section: GenerationSection,
    force: bool,
    on_page_rendered: &(dyn Fn(&str) + Sync),
    mut on_page: impl FnMut(usize, usize, &str),
    mut is_cancelled: impl FnMut() -> bool,
) -> EngineResult<GenerationReport> {
    if is_cancelled() {
        return Err(EngineError::Validation("cancelled".to_string()));
    }
    let artifacts = build_site_render_artifacts_from_context(
        &prepared.render,
        Some(section),
        None,
        on_page_rendered,
    )
    .map_err(|error| EngineError::Parse(error.to_string()))?;
    let mut report = GenerationReport::default();
    let mut writer = GeneratedFileWriter::with_existing(
        conn,
        output_dir,
        project_id,
        Arc::clone(&prepared.generated_hashes),
        force,
        false,
    )
    .map_err(|error| EngineError::Parse(error.to_string()))?;
    let total_pages = artifacts.pages.len();
    for (index, page) in artifacts.pages.iter().enumerate() {
        if is_cancelled() {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
        write_out(&mut writer, &page.relative_path, &page.html, &mut report)?;
        on_page(index + 1, total_pages, &page.url_path);
    }

    if section == GenerationSection::Core {
        write_core_outputs(
            &mut writer,
            prepared,
            &project_data_dir(output_dir),
            &artifacts.route_manifest,
            None,
            &mut report,
            &mut is_cancelled,
        )?;
    }
    writer
        .finish()
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    Ok(report)
}

pub fn sections_from_validation_report(
    report: &SiteValidationReport,
    metadata: &ProjectMetadata,
) -> Vec<GenerationSection> {
    let mut sections = HashSet::new();
    let mut saw_unknown = false;

    for path in report
        .missing_pages
        .iter()
        .chain(report.extra_pages.iter())
        .chain(report.stale_pages.iter())
    {
        match classify_generated_path(path, metadata) {
            Some(GenerationSection::Single) => {
                sections.extend(GenerationSection::ALL);
            }
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
    validation: &SiteValidationReport,
    sections: &[GenerationSection],
) -> EngineResult<GenerationReport> {
    if sections.is_empty() {
        return Ok(GenerationReport::default());
    }

    let mut report = GenerationReport::default();

    for section in sections {
        report.append(apply_validation_section_with_progress(
            conn,
            output_dir,
            project_id,
            metadata,
            posts,
            validation,
            *section,
            |_current, _total, _url| {},
            |_| {},
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
    on_page_rendered: impl Fn(&str) + Sync,
    mut is_cancelled: impl FnMut() -> bool,
) -> EngineResult<GenerationReport> {
    if is_cancelled() {
        return Err(EngineError::Validation("cancelled".to_string()));
    }
    let data_dir = project_data_dir(output_dir);
    let prepared = prepare_site_generation(conn, &data_dir, project_id, metadata, posts)?;
    apply_validation_prepared_section_with_progress(
        conn,
        output_dir,
        project_id,
        &prepared,
        validation,
        section,
        &mut on_page,
        &on_page_rendered,
        &mut is_cancelled,
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "prepared targeted apply keeps validation and callbacks"
)]
pub fn apply_validation_prepared_section_with_progress(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    prepared: &PreparedSiteGeneration,
    validation: &SiteValidationReport,
    section: GenerationSection,
    mut on_page: impl FnMut(usize, usize, &str),
    on_page_rendered: &(dyn Fn(&str) + Sync),
    mut is_cancelled: impl FnMut() -> bool,
) -> EngineResult<GenerationReport> {
    let metadata = &prepared.metadata;
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
        .any(|path| classify_generated_path(path, metadata).is_none());
    let artifacts = build_site_render_artifacts_from_context(
        &prepared.render,
        Some(section),
        (!fallback).then_some(&requested),
        on_page_rendered,
    )
    .map_err(|error| EngineError::Parse(error.to_string()))?;
    let mut report = GenerationReport::default();
    let mut writer = GeneratedFileWriter::with_existing(
        conn,
        output_dir,
        project_id,
        Arc::clone(&prepared.generated_hashes),
        false,
        true,
    )
    .map_err(|error| EngineError::Parse(error.to_string()))?;
    let total_pages = artifacts.pages.len();
    for (index, page) in artifacts.pages.iter().enumerate() {
        if is_cancelled() {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
        write_out(&mut writer, &page.relative_path, &page.html, &mut report)?;
        on_page(index + 1, total_pages, &page.url_path);
    }

    if section == GenerationSection::Core {
        write_core_outputs(
            &mut writer,
            prepared,
            &project_data_dir(output_dir),
            &artifacts.route_manifest,
            (!fallback).then_some(&requested),
            &mut report,
            &mut is_cancelled,
        )?;
    }
    writer
        .finish()
        .map_err(|error| EngineError::Parse(error.to_string()))?;

    for path in &validation.extra_pages {
        if is_cancelled() {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
        let owned_by_section = classify_generated_path(path, metadata)
            .map_or(section == GenerationSection::Core, |owner| owner == section);
        if owned_by_section && output_dir.join(path).is_file() {
            std::fs::remove_file(output_dir.join(path)).map_err(EngineError::Io)?;
            report.deleted_paths.push(path.clone());
        }
    }
    refresh_route_timestamps(conn, project_id, &report)?;
    Ok(report)
}

fn refresh_route_timestamps(
    conn: &Connection,
    project_id: &str,
    report: &GenerationReport,
) -> EngineResult<()> {
    let paths = report
        .written_paths
        .iter()
        .chain(&report.skipped_paths)
        .filter(|path| *path == "index.html" || path.ends_with("/index.html"))
        .cloned()
        .collect::<Vec<_>>();
    queries::generated_file_hash::touch_generated_file_hashes(
        conn,
        project_id,
        &paths,
        crate::util::now_unix_ms(),
    )?;
    Ok(())
}

fn write_core_outputs(
    writer: &mut GeneratedFileWriter<'_>,
    prepared: &PreparedSiteGeneration,
    data_dir: &Path,
    route_manifest: &[crate::render::SitePage],
    requested: Option<&HashSet<String>>,
    report: &mut GenerationReport,
    is_cancelled: &mut impl FnMut() -> bool,
) -> EngineResult<()> {
    let metadata = &prepared.metadata;
    let published_posts = &prepared.sources;
    if requested.is_none() {
        for asset in bundled_site_assets() {
            let outcome = writer
                .write_bytes(asset.relative_path, asset.bytes)
                .map_err(|error| EngineError::Parse(error.to_string()))?;
            record_write_outcome(report, asset.relative_path, outcome);
        }
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
        let is_main = render_language == metadata.main_language.as_deref().unwrap_or("en");
        let prefix = if is_main {
            String::new()
        } else {
            format!("{render_language}/")
        };
        let mut feed_posts = if is_main {
            published_posts
                .iter()
                .map(|source| &source.post)
                .collect::<Vec<_>>()
        } else {
            prepared
                .render
                .localized_posts(&render_language)
                .filter(|post| {
                    post.language
                        .as_deref()
                        .is_some_and(|language| language.eq_ignore_ascii_case(&render_language))
                })
                .collect::<Vec<_>>()
        };
        feed_posts.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.published_at.cmp(&left.published_at))
                .then_with(|| left.slug.cmp(&right.slug))
        });
        outputs.push((
            format!("{prefix}rss.xml"),
            build_rss_xml(metadata, feed_posts.iter().copied(), &render_language),
        ));
        outputs.push((
            format!("{prefix}atom.xml"),
            build_atom_xml(metadata, feed_posts.iter().copied(), &render_language),
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
            write_out(writer, &path, &content, report)?;
        }
    }
    Ok(())
}

fn write_out(
    writer: &mut GeneratedFileWriter<'_>,
    relative_path: &str,
    content: &str,
    report: &mut GenerationReport,
) -> EngineResult<()> {
    let outcome = writer
        .write_str(relative_path, content)
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    record_write_outcome(report, relative_path, outcome);
    Ok(())
}

fn record_write_outcome(
    report: &mut GenerationReport,
    relative_path: &str,
    outcome: GeneratedWriteOutcome,
) {
    match outcome {
        GeneratedWriteOutcome::Written => report.written_paths.push(relative_path.to_string()),
        GeneratedWriteOutcome::SkippedUnchanged => {
            report.skipped_paths.push(relative_path.to_string())
        }
    }
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

pub fn build_site_search_index_forced_with_progress(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    on_file: impl FnMut(usize, usize, &str),
    is_cancelled: impl FnMut() -> bool,
) -> EngineResult<GenerationReport> {
    build_site_search_index_with_progress_mode(
        conn,
        output_dir,
        project_id,
        metadata,
        true,
        on_file,
        is_cancelled,
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
    build_site_search_index_with_progress_mode(
        conn,
        output_dir,
        project_id,
        metadata,
        false,
        &mut on_file,
        &mut is_cancelled,
    )
}

fn build_site_search_index_with_progress_mode(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    metadata: &ProjectMetadata,
    force: bool,
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
    let mut writer = GeneratedFileWriter::new(conn, output_dir, project_id, force, false)
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    let expected = outputs
        .iter()
        .map(|(relative, _)| relative.clone())
        .collect::<HashSet<_>>();
    for (index, (relative, contents)) in outputs.into_iter().enumerate() {
        if is_cancelled() {
            return Err(EngineError::Validation("cancelled".to_string()));
        }
        let outcome = writer
            .write_bytes(&relative, &contents)
            .map_err(|error| EngineError::Parse(error.to_string()))?;
        record_write_outcome(&mut report, &relative, outcome);
        on_file(index + 1, total, &relative);
    }
    writer
        .finish()
        .map_err(|error| EngineError::Parse(error.to_string()))?;
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

pub(crate) fn classify_generated_path(
    path: &str,
    metadata: &ProjectMetadata,
) -> Option<GenerationSection> {
    if path.ends_with(".xml") || path.ends_with(".json") {
        return Some(GenerationSection::Core);
    }

    let mut parts = path.split('/').collect::<Vec<_>>();
    if parts.is_empty() {
        return None;
    }
    if has_language_prefix(&parts, metadata) {
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

fn has_language_prefix(parts: &[&str], metadata: &ProjectMetadata) -> bool {
    parts.len() > 1
        && render_languages(metadata)
            .iter()
            .any(|language| language.eq_ignore_ascii_case(parts[0]))
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

pub(crate) fn refresh_validation_sitemap(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    data_dir: &Path,
    metadata: &ProjectMetadata,
    posts: &[Post],
    route_manifest: &[crate::render::SitePage],
) -> EngineResult<()> {
    let mut sources = posts
        .iter()
        .cloned()
        .map(|post| PublishedPostSource {
            post,
            body_markdown: String::new(),
        })
        .collect::<Vec<_>>();
    sort_published_sources(&mut sources);
    let list_posts = filter_posts_for_lists(&sources, &load_category_settings(data_dir));
    let content = build_sitemap_xml(
        metadata,
        route_manifest,
        &sources,
        &list_posts,
        metadata.main_language.as_deref().unwrap_or("en"),
    );
    write_generated_file(conn, output_dir, project_id, "sitemap.xml", &content)
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    Ok(())
}

pub(crate) fn build_rss_xml<'a>(
    metadata: &ProjectMetadata,
    posts: impl IntoIterator<Item = &'a Post>,
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

    for post in posts {
        let url = post_absolute_url(base_url, metadata, post, language);
        xml.push_str(&format!(
            "<item><title>{}</title><link>{url}</link></item>",
            escape_xml(&post.title)
        ));
    }
    xml.push_str("</channel></rss>");
    xml
}

pub(crate) fn build_atom_xml<'a>(
    metadata: &ProjectMetadata,
    posts: impl IntoIterator<Item = &'a Post>,
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

    for post in posts {
        let url = post_absolute_url(base_url, metadata, post, language);
        xml.push_str(&format!(
            "<entry><title>{}</title><id>{url}</id></entry>",
            escape_xml(&post.title)
        ));
    }
    xml.push_str("</feed>");
    xml
}

fn post_absolute_url(
    base_url: &str,
    metadata: &ProjectMetadata,
    post: &Post,
    language: &str,
) -> String {
    format!(
        "{base_url}{}/",
        build_canonical_post_path(
            post,
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
