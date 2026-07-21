use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::BufReader;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

use chrono::{DateTime, Datelike, NaiveDateTime, Utc};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use regex::Regex;
use serde_json::json;
use uuid::Uuid;

use crate::db::DbConnection as Connection;
use crate::db::queries::{
    import_definition as qd, media as qm, media_translation as qmt, post as qp,
    post_translation as qpt, tag as qt,
};
use crate::engine::{EngineError, EngineResult, ai, media, meta, post, post_media, tag};
use crate::model::{
    ImportCandidate, ImportCounts, ImportDateBucket, ImportDefinition, ImportExecutionCounts,
    ImportExecutionResult, ImportItemKind, ImportItemStatus, ImportMacroUsage, ImportPhase,
    ImportProgress, ImportReport, ImportResolution, ImportedSite, Media, Post, TaxonomyCandidate,
    TaxonomyKind,
};
use crate::util::{
    atomic_write_str, content_hash, media_file_hash, media_sidecar_path,
    media_translation_sidecar_path, now_unix_ms, post_file_path, slugify,
};

const TRANSACTION_BATCH_SIZE: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WxrSite {
    pub title: String,
    pub link: String,
    pub description: String,
    pub language: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WxrTaxonomy {
    pub name: String,
    pub slug: String,
    pub parent: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WxrPost {
    pub source_id: Option<i64>,
    pub title: String,
    pub slug: String,
    pub content: String,
    pub excerpt: String,
    pub published_at: Option<i64>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub creator: String,
    pub status: String,
    pub post_type: String,
    pub categories: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WxrMedia {
    pub source_id: Option<i64>,
    pub title: String,
    pub url: String,
    pub filename: String,
    pub relative_path: String,
    pub published_at: Option<i64>,
    pub parent_source_id: Option<i64>,
    pub mime_type: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WxrExport {
    pub site: WxrSite,
    pub posts: Vec<WxrPost>,
    pub pages: Vec<WxrPost>,
    pub media: Vec<WxrMedia>,
    pub categories: Vec<WxrTaxonomy>,
    pub tags: Vec<WxrTaxonomy>,
}

#[derive(Debug, Default)]
struct RawItem {
    post_id: String,
    title: String,
    content: String,
    excerpt: String,
    pub_date: String,
    post_date: String,
    post_modified: String,
    creator: String,
    status: String,
    post_type: String,
    post_name: String,
    post_parent: String,
    attachment_url: String,
    categories: Vec<String>,
    tags: Vec<String>,
}

#[derive(Debug, Default)]
struct ParserState {
    stack: Vec<String>,
    channel_seen: bool,
    site: WxrSite,
    categories: Vec<WxrTaxonomy>,
    tags: Vec<WxrTaxonomy>,
    items: Vec<RawItem>,
    current_category: Option<WxrTaxonomy>,
    current_tag: Option<WxrTaxonomy>,
    current_item: Option<RawItem>,
    current_taxonomy_domain: Option<String>,
    text: String,
}

pub fn parse_wxr_file(path: &Path) -> EngineResult<WxrExport> {
    let file = fs::File::open(path)?;
    parse_wxr_reader(Reader::from_reader(BufReader::new(file)))
}

pub fn parse_wxr_xml(xml: &str) -> EngineResult<WxrExport> {
    parse_wxr_reader(Reader::from_reader(xml.as_bytes()))
}

fn parse_wxr_reader<R: std::io::BufRead>(mut reader: Reader<R>) -> EngineResult<WxrExport> {
    reader.config_mut().expand_empty_elements = true;
    reader.config_mut().check_end_names = true;
    let mut state = ParserState::default();
    let mut buffer = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(start)) => parser_start(&mut state, &start, reader.decoder())?,
            Ok(Event::End(end)) => {
                let name = reader
                    .decoder()
                    .decode(end.name().as_ref())
                    .map_err(|error| EngineError::Parse(error.to_string()))?
                    .into_owned();
                parser_end(&mut state, &name);
            }
            Ok(Event::Text(text)) => {
                state.text.push_str(
                    &text
                        .decode()
                        .map_err(|error| EngineError::Parse(error.to_string()))?,
                );
            }
            Ok(Event::CData(text)) => {
                state.text.push_str(
                    &text
                        .decode()
                        .map_err(|error| EngineError::Parse(error.to_string()))?,
                );
            }
            Ok(Event::GeneralRef(reference)) => {
                let name = reference
                    .decode()
                    .map_err(|error| EngineError::Parse(error.to_string()))?;
                state.text.push_str(match name.as_ref() {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    _ => "",
                });
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(EngineError::Parse(format!(
                    "invalid WXR XML at byte {}: {error}",
                    reader.error_position()
                )));
            }
        }
        buffer.clear();
    }

    if !state.channel_seen {
        return Err(EngineError::Validation(
            "invalid WXR file: no RSS channel element found".to_string(),
        ));
    }
    if !state.stack.is_empty() {
        return Err(EngineError::Parse(
            "invalid WXR XML: document ended before all elements were closed".to_string(),
        ));
    }
    Ok(build_wxr_export(state))
}

fn parser_start(
    state: &mut ParserState,
    start: &BytesStart<'_>,
    decoder: quick_xml::encoding::Decoder,
) -> EngineResult<()> {
    let name = decoder
        .decode(start.name().as_ref())
        .map_err(|error| EngineError::Parse(error.to_string()))?
        .into_owned();
    let parent = state.stack.last().map(String::as_str);

    match (parent, name.as_str()) {
        (Some("rss"), "channel") => state.channel_seen = true,
        (Some("channel"), "wp:category") => state.current_category = Some(WxrTaxonomy::default()),
        (Some("channel"), "wp:tag") => state.current_tag = Some(WxrTaxonomy::default()),
        (Some("channel"), "item") => state.current_item = Some(RawItem::default()),
        (Some("item"), "category") => {
            state.current_taxonomy_domain = attribute(start, b"domain", decoder)?;
        }
        _ => {}
    }
    state.stack.push(name);
    state.text.clear();
    Ok(())
}

fn attribute(
    start: &BytesStart<'_>,
    expected: &[u8],
    decoder: quick_xml::encoding::Decoder,
) -> EngineResult<Option<String>> {
    for attribute in start.attributes() {
        let attribute = attribute.map_err(|error| EngineError::Parse(error.to_string()))?;
        if attribute.key.as_ref() == expected {
            return Ok(Some(
                attribute
                    .decoded_and_normalized_value(quick_xml::XmlVersion::Implicit1_0, decoder)
                    .map_err(|error| EngineError::Parse(error.to_string()))?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn parser_end(state: &mut ParserState, name: &str) {
    let parent = state
        .stack
        .iter()
        .rev()
        .nth(1)
        .map(String::as_str)
        .unwrap_or("");
    let text = state.text.trim().to_string();

    if state.current_category.is_none()
        && state.current_tag.is_none()
        && state.current_item.is_none()
        && parent == "channel"
    {
        match name {
            "title" => state.site.title = text.clone(),
            "link" => state.site.link = text.clone(),
            "description" => state.site.description = text.clone(),
            "language" => state.site.language = text.clone(),
            _ => {}
        }
    }

    if let Some(category) = state.current_category.as_mut()
        && parent == "wp:category"
    {
        match name {
            "wp:cat_name" => category.name = text.clone(),
            "wp:category_nicename" => category.slug = text.clone(),
            "wp:category_parent" => category.parent = text.clone(),
            _ => {}
        }
    }
    if parent == "channel"
        && name == "wp:category"
        && let Some(category) = state.current_category.take()
    {
        state.categories.push(category);
    }

    if let Some(tag) = state.current_tag.as_mut()
        && parent == "wp:tag"
    {
        match name {
            "wp:tag_name" => tag.name = text.clone(),
            "wp:tag_slug" => tag.slug = text.clone(),
            _ => {}
        }
    }
    if parent == "channel"
        && name == "wp:tag"
        && let Some(tag) = state.current_tag.take()
    {
        state.tags.push(tag);
    }

    if let Some(item) = state.current_item.as_mut()
        && parent == "item"
    {
        match name {
            "title" => item.title = text.clone(),
            "pubDate" => item.pub_date = text.clone(),
            "dc:creator" => item.creator = text.clone(),
            "content:encoded" => item.content = text.clone(),
            "excerpt:encoded" => item.excerpt = text.clone(),
            "wp:post_id" => item.post_id = text.clone(),
            "wp:post_date" => item.post_date = text.clone(),
            "wp:post_modified" => item.post_modified = text.clone(),
            "wp:post_name" => item.post_name = text.clone(),
            "wp:status" => item.status = text.clone(),
            "wp:post_type" => item.post_type = text.clone(),
            "wp:post_parent" => item.post_parent = text.clone(),
            "wp:attachment_url" => item.attachment_url = text.clone(),
            "category" => match state.current_taxonomy_domain.as_deref() {
                Some("category") if !text.is_empty() => item.categories.push(text.clone()),
                Some("post_tag") if !text.is_empty() => item.tags.push(text.clone()),
                _ => {}
            },
            _ => {}
        }
    }
    if parent == "item" && name == "category" {
        state.current_taxonomy_domain = None;
    }
    if parent == "channel"
        && name == "item"
        && let Some(item) = state.current_item.take()
    {
        state.items.push(item);
    }

    state.stack.pop();
    state.text.clear();
}

fn build_wxr_export(state: ParserState) -> WxrExport {
    let mut export = WxrExport {
        site: state.site,
        categories: state.categories,
        tags: state.tags,
        ..WxrExport::default()
    };
    for item in state.items {
        if item.post_type == "attachment" {
            let filename = Path::new(&item.attachment_url)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string();
            let relative_path = item
                .attachment_url
                .split_once("/wp-content/uploads/")
                .map(|(_, suffix)| suffix.to_string())
                .unwrap_or_else(|| filename.clone());
            let extension = Path::new(&filename)
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or("")
                .to_string();
            export.media.push(WxrMedia {
                source_id: parse_id(&item.post_id),
                title: item.title,
                url: item.attachment_url,
                filename,
                relative_path,
                published_at: parse_timestamp(&item.pub_date),
                parent_source_id: parse_id(&item.post_parent),
                mime_type: crate::util::thumbnail::mime_from_extension(&extension).to_string(),
                description: item.content,
            });
        } else {
            let post = WxrPost {
                source_id: parse_id(&item.post_id),
                title: item.title,
                slug: item.post_name,
                content: item.content,
                excerpt: item.excerpt,
                published_at: parse_timestamp(&item.pub_date),
                created_at: parse_timestamp(&item.post_date)
                    .or_else(|| parse_timestamp(&item.pub_date)),
                updated_at: parse_timestamp(&item.post_modified)
                    .or_else(|| parse_timestamp(&item.post_date))
                    .or_else(|| parse_timestamp(&item.pub_date)),
                creator: item.creator,
                status: item.status,
                post_type: item.post_type.clone(),
                categories: item.categories,
                tags: item.tags,
            };
            if item.post_type == "page" {
                export.pages.push(post);
            } else if item.post_type == "post" {
                export.posts.push(post);
            }
        }
    }
    export
}

fn parse_id(value: &str) -> Option<i64> {
    value.parse().ok().filter(|value| *value != 0)
}

fn parse_timestamp(value: &str) -> Option<i64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc2822(value)
        .map(|date| date.timestamp_millis())
        .or_else(|_| DateTime::parse_from_rfc3339(value).map(|date| date.timestamp_millis()))
        .or_else(|_| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .map(|date| date.and_utc().timestamp_millis())
        })
        .ok()
}

pub fn analyze_wxr(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    wxr_file: &Path,
    uploads_folder: Option<&Path>,
    mut progress: Option<&mut dyn FnMut(ImportProgress)>,
) -> EngineResult<ImportReport> {
    let export = parse_wxr_file(wxr_file)?;
    let existing_posts = qp::list_posts_by_project(conn, project_id)?;
    let existing_media = qm::list_media_by_project(conn, project_id)?;
    let existing_tags = qt::list_tags_by_project(conn, project_id)?;
    let existing_categories = meta::read_categories_json(data_dir).unwrap_or_default();

    let posts_by_slug: HashMap<String, &Post> = existing_posts
        .iter()
        .map(|post| (post.slug.clone(), post))
        .collect();
    let posts_by_checksum: HashMap<String, &Post> = existing_posts
        .iter()
        .filter_map(|post| {
            existing_post_body(post, data_dir).map(|body| (content_hash(body.as_bytes()), post))
        })
        .collect();
    let post_checksums = existing_posts
        .iter()
        .filter_map(|post| {
            existing_post_body(post, data_dir)
                .map(|body| (post.id.clone(), content_hash(body.as_bytes())))
        })
        .collect::<HashMap<_, _>>();
    let media_by_name: HashMap<String, &Media> = existing_media
        .iter()
        .map(|media| (media.original_name.to_lowercase(), media))
        .collect();
    let media_by_checksum: HashMap<String, &Media> = existing_media
        .iter()
        .filter_map(|media| media.checksum.clone().map(|checksum| (checksum, media)))
        .collect();

    notify(
        &mut progress,
        ImportPhase::Posts,
        0,
        export.posts.len(),
        "analyzing_posts",
        None,
    );
    let posts = export
        .posts
        .iter()
        .map(|item| {
            analyze_post(
                item,
                ImportItemKind::Post,
                &posts_by_slug,
                &posts_by_checksum,
                &post_checksums,
            )
        })
        .collect::<EngineResult<Vec<_>>>()?;
    notify(
        &mut progress,
        ImportPhase::Posts,
        posts.len(),
        posts.len(),
        "analyzed_posts",
        None,
    );
    notify(
        &mut progress,
        ImportPhase::Pages,
        0,
        export.pages.len(),
        "analyzing_pages",
        None,
    );
    let pages = export
        .pages
        .iter()
        .map(|item| {
            analyze_post(
                item,
                ImportItemKind::Page,
                &posts_by_slug,
                &posts_by_checksum,
                &post_checksums,
            )
        })
        .collect::<EngineResult<Vec<_>>>()?;
    notify(
        &mut progress,
        ImportPhase::Pages,
        pages.len(),
        pages.len(),
        "analyzed_pages",
        None,
    );
    notify(
        &mut progress,
        ImportPhase::Media,
        0,
        export.media.len(),
        "analyzing_media",
        None,
    );
    let media = export
        .media
        .iter()
        .map(|item| analyze_media(item, uploads_folder, &media_by_name, &media_by_checksum))
        .collect::<Vec<_>>();
    notify(
        &mut progress,
        ImportPhase::Media,
        media.len(),
        media.len(),
        "analyzed_media",
        None,
    );

    let category_names = existing_categories
        .iter()
        .map(|name| name.to_lowercase())
        .collect::<HashSet<_>>();
    let tag_names = existing_tags
        .iter()
        .map(|tag| tag.name.to_lowercase())
        .collect::<HashSet<_>>();
    let taxonomies = export
        .categories
        .iter()
        .map(|item| TaxonomyCandidate {
            kind: TaxonomyKind::Category,
            name: item.name.clone(),
            slug: nonempty(&item.slug),
            exists_in_project: category_names.contains(&item.name.to_lowercase()),
            mapped_to: None,
        })
        .chain(export.tags.iter().map(|item| TaxonomyCandidate {
            kind: TaxonomyKind::Tag,
            name: item.name.clone(),
            slug: nonempty(&item.slug),
            exists_in_project: tag_names.contains(&item.name.to_lowercase()),
            mapped_to: None,
        }))
        .collect::<Vec<_>>();
    notify(
        &mut progress,
        ImportPhase::Taxonomy,
        taxonomies.len(),
        taxonomies.len(),
        "analyzed_taxonomy",
        None,
    );

    Ok(ImportReport {
        source_file: wxr_file.to_string_lossy().to_string(),
        uploads_folder: uploads_folder.map(|path| path.to_string_lossy().to_string()),
        site: ImportedSite {
            title: export.site.title,
            url: nonempty(&export.site.link),
            language: nonempty(&export.site.language),
        },
        post_counts: counts(&posts),
        page_counts: counts(&pages),
        media_counts: counts(&media),
        date_distribution: date_distribution(&posts, &pages, &media),
        macros: analyze_macros(export.posts.iter().chain(&export.pages)),
        posts,
        pages,
        media,
        taxonomies,
    })
}

fn existing_post_body(post: &Post, data_dir: &Path) -> Option<String> {
    post.content.clone().or_else(|| {
        (!post.file_path.is_empty())
            .then(|| fs::read_to_string(data_dir.join(&post.file_path)).ok())
            .flatten()
            .and_then(|raw| crate::util::frontmatter::read_post_file(&raw).ok())
            .map(|(_, body)| body)
    })
}

fn analyze_post(
    item: &WxrPost,
    kind: ImportItemKind,
    posts_by_slug: &HashMap<String, &Post>,
    posts_by_checksum: &HashMap<String, &Post>,
    post_checksums: &HashMap<String, String>,
) -> EngineResult<ImportCandidate> {
    let content = html_to_markdown(&item.content)?;
    let checksum = content_hash(content.as_bytes());
    let slug = if item.slug.trim().is_empty() {
        slugify(&item.title)
    } else {
        item.slug.clone()
    };
    let existing_by_slug = posts_by_slug.get(&slug).copied();
    let existing_by_checksum = posts_by_checksum.get(&checksum).copied();
    let (status, existing_id) = if let Some(existing) = existing_by_slug {
        if post_checksums.get(&existing.id) == Some(&checksum) {
            (ImportItemStatus::Update, Some(existing.id.clone()))
        } else {
            (ImportItemStatus::Conflict, Some(existing.id.clone()))
        }
    } else if let Some(existing) = existing_by_checksum {
        (
            ImportItemStatus::ContentDuplicate,
            Some(existing.id.clone()),
        )
    } else {
        (ImportItemStatus::New, None)
    };
    Ok(ImportCandidate {
        kind,
        source_id: item.source_id,
        title: item.title.clone(),
        slug: Some(slug),
        filename: None,
        relative_path: None,
        status,
        resolution: (status == ImportItemStatus::Conflict).then_some(ImportResolution::Ignore),
        existing_id,
        author: nonempty(&item.creator),
        excerpt: nonempty(&item.excerpt),
        content: Some(content),
        source_status: nonempty(&item.status),
        categories: item.categories.clone(),
        tags: item.tags.clone(),
        source_path: None,
        parent_source_id: None,
        created_at: item.created_at,
        updated_at: item.updated_at,
        published_at: item.published_at,
        checksum: Some(checksum),
        mime_type: None,
        description: None,
    })
}

fn analyze_media(
    item: &WxrMedia,
    uploads_folder: Option<&Path>,
    media_by_name: &HashMap<String, &Media>,
    media_by_checksum: &HashMap<String, &Media>,
) -> ImportCandidate {
    let source_path =
        uploads_folder.and_then(|folder| safe_upload_path(folder, &item.relative_path));
    let checksum = source_path
        .as_ref()
        .filter(|path| path.is_file())
        .and_then(|path| media_file_hash(path).ok());
    let existing_by_name = media_by_name.get(&item.filename.to_lowercase()).copied();
    let existing_by_checksum = checksum
        .as_ref()
        .and_then(|checksum| media_by_checksum.get(checksum).copied());
    let (status, existing_id) = if checksum.is_none() {
        (ImportItemStatus::Missing, None)
    } else if let Some(existing) = existing_by_name {
        if existing.checksum == checksum {
            (ImportItemStatus::Update, Some(existing.id.clone()))
        } else {
            (ImportItemStatus::Conflict, Some(existing.id.clone()))
        }
    } else if let Some(existing) = existing_by_checksum {
        (
            ImportItemStatus::ContentDuplicate,
            Some(existing.id.clone()),
        )
    } else {
        (ImportItemStatus::New, None)
    };
    ImportCandidate {
        kind: ImportItemKind::Media,
        source_id: item.source_id,
        title: item.title.clone(),
        slug: None,
        filename: Some(item.filename.clone()),
        relative_path: Some(item.relative_path.clone()),
        status,
        resolution: (status == ImportItemStatus::Conflict).then_some(ImportResolution::Ignore),
        existing_id,
        author: None,
        excerpt: None,
        content: None,
        source_status: None,
        categories: Vec::new(),
        tags: Vec::new(),
        source_path: source_path.map(|path| path.to_string_lossy().to_string()),
        parent_source_id: item.parent_source_id,
        created_at: item.published_at,
        updated_at: item.published_at,
        published_at: item.published_at,
        checksum,
        mime_type: nonempty(&item.mime_type),
        description: nonempty(&item.description),
    }
}

fn safe_upload_path(folder: &Path, relative: &str) -> Option<PathBuf> {
    let relative = Path::new(relative);
    if relative
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
    {
        Some(folder.join(relative))
    } else {
        None
    }
}

fn counts(items: &[ImportCandidate]) -> ImportCounts {
    let mut result = ImportCounts::default();
    for item in items {
        match item.status {
            ImportItemStatus::New => result.new_count += 1,
            ImportItemStatus::Update => result.update_count += 1,
            ImportItemStatus::Conflict => result.conflict_count += 1,
            ImportItemStatus::ContentDuplicate => result.duplicate_count += 1,
            ImportItemStatus::Missing => result.missing_count += 1,
        }
    }
    result
}

fn date_distribution(
    posts: &[ImportCandidate],
    pages: &[ImportCandidate],
    media: &[ImportCandidate],
) -> Vec<ImportDateBucket> {
    let mut years = BTreeMap::<i32, (usize, usize)>::new();
    for item in posts.iter().chain(pages) {
        if let Some(timestamp) = item.created_at.or(item.published_at)
            && let Some(date) = DateTime::<Utc>::from_timestamp_millis(timestamp)
        {
            years.entry(date.year()).or_default().0 += 1;
        }
    }
    for item in media {
        if let Some(timestamp) = item.created_at
            && let Some(date) = DateTime::<Utc>::from_timestamp_millis(timestamp)
        {
            years.entry(date.year()).or_default().1 += 1;
        }
    }
    years
        .into_iter()
        .map(|(year, (post_count, media_count))| ImportDateBucket {
            year,
            post_count,
            media_count,
        })
        .collect()
}

fn html_to_markdown(html: &str) -> EngineResult<String> {
    let converted = htmd::convert(&transform_shortcodes(html)).map_err(|error| {
        EngineError::Parse(format!("HTML to Markdown conversion failed: {error}"))
    })?;
    let converted = converted
        .replace('\u{e000}', "[[")
        .replace('\u{e001}', "]]");
    Ok(Regex::new(r"\n{3,}")
        .unwrap()
        .replace_all(converted.trim(), "\n\n")
        .into_owned())
}

fn transform_shortcodes(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'['
            && bytes.get(index.wrapping_sub(1)) != Some(&b'[')
            && bytes
                .get(index + 1)
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            && let Some(end_offset) = value[index + 1..].find(']')
        {
            let end = index + 1 + end_offset;
            if bytes.get(end + 1) != Some(&b']') {
                let inner = value[index + 1..end].trim().trim_end_matches('/').trim();
                output.push('\u{e000}');
                output.push_str(inner);
                output.push('\u{e001}');
                index = end + 1;
                continue;
            }
        }
        let character = value[index..].chars().next().unwrap();
        output.push(character);
        index += character.len_utf8();
    }
    output
}

fn analyze_macros<'a>(items: impl Iterator<Item = &'a WxrPost>) -> Vec<ImportMacroUsage> {
    let shortcode = Regex::new(r#"\[(\w+)([^\]]*?)(?:\s*/)?\]"#).unwrap();
    let parameter = Regex::new(r#"(\w+)=(?:"([^"]*)"|'([^']*)'|([^\s\]"']+))"#).unwrap();
    let mut macros = BTreeMap::<String, ImportMacroUsage>::new();
    for item in items {
        for capture in shortcode.captures_iter(&item.content) {
            let name = capture[1].to_lowercase();
            let parameters = parameter
                .captures_iter(capture.get(2).map(|value| value.as_str()).unwrap_or(""))
                .map(|capture| {
                    let value = capture
                        .get(2)
                        .or_else(|| capture.get(3))
                        .or_else(|| capture.get(4))
                        .map(|value| value.as_str())
                        .unwrap_or("");
                    (capture[1].to_string(), value.to_string())
                })
                .collect::<BTreeMap<_, _>>();
            let entry = macros
                .entry(name.clone())
                .or_insert_with(|| ImportMacroUsage {
                    name,
                    total_count: 0,
                    post_slugs: Vec::new(),
                    parameters: Vec::new(),
                });
            entry.total_count += 1;
            if !item.slug.is_empty() && !entry.post_slugs.contains(&item.slug) {
                entry.post_slugs.push(item.slug.clone());
            }
            if !entry.parameters.contains(&parameters) {
                entry.parameters.push(parameters);
            }
        }
    }
    macros.into_values().collect()
}

fn nonempty(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

pub fn create_definition(
    conn: &Connection,
    project_id: &str,
    name: &str,
) -> EngineResult<ImportDefinition> {
    let now = now_unix_ms();
    let definition = ImportDefinition {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        name: name.to_string(),
        wxr_file_path: None,
        uploads_folder_path: None,
        last_analysis_result: None,
        created_at: now,
        updated_at: now,
    };
    qd::insert_import_definition(conn, &definition)?;
    Ok(definition)
}

pub fn get_definition(conn: &Connection, id: &str) -> EngineResult<ImportDefinition> {
    qd::get_import_definition(conn, id).map_err(Into::into)
}

pub fn list_definitions(
    conn: &Connection,
    project_id: &str,
) -> EngineResult<Vec<ImportDefinition>> {
    qd::list_import_definitions(conn, project_id).map_err(Into::into)
}

pub fn update_definition(
    conn: &Connection,
    id: &str,
    name: Option<&str>,
    wxr_file_path: Option<Option<&Path>>,
    uploads_folder_path: Option<Option<&Path>>,
    analysis: Option<Option<&ImportReport>>,
) -> EngineResult<ImportDefinition> {
    let mut definition = get_definition(conn, id)?;
    if let Some(name) = name {
        definition.name = name.to_string();
    }
    if let Some(path) = wxr_file_path {
        definition.wxr_file_path = path.map(|path| path.to_string_lossy().to_string());
    }
    if let Some(path) = uploads_folder_path {
        definition.uploads_folder_path = path.map(|path| path.to_string_lossy().to_string());
    }
    if let Some(report) = analysis {
        definition.last_analysis_result = report.map(serde_json::to_string).transpose()?;
    }
    definition.updated_at = now_unix_ms();
    qd::update_import_definition(conn, &definition)?;
    Ok(definition)
}

pub fn delete_definition(conn: &Connection, id: &str) -> EngineResult<()> {
    qd::delete_import_definition(conn, id).map_err(Into::into)
}

pub fn set_conflict_resolution(
    report: &mut ImportReport,
    kind: ImportItemKind,
    identity: &str,
    resolution: ImportResolution,
) -> EngineResult<()> {
    let candidates = match kind {
        ImportItemKind::Post => &mut report.posts,
        ImportItemKind::Page => &mut report.pages,
        ImportItemKind::Media => &mut report.media,
    };
    let candidate = candidates
        .iter_mut()
        .find(|candidate| {
            candidate.status == ImportItemStatus::Conflict
                && candidate.slug.as_deref().or(candidate.filename.as_deref()) == Some(identity)
        })
        .ok_or_else(|| EngineError::NotFound(format!("import conflict {identity}")))?;
    candidate.resolution = Some(resolution);
    Ok(())
}

pub fn set_taxonomy_mapping(
    report: &mut ImportReport,
    kind: TaxonomyKind,
    source_name: &str,
    target_name: Option<&str>,
) -> EngineResult<()> {
    let candidate = report
        .taxonomies
        .iter_mut()
        .find(|candidate| candidate.kind == kind && candidate.name == source_name)
        .ok_or_else(|| EngineError::NotFound(format!("import taxonomy {source_name}")))?;
    candidate.mapped_to = target_name
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .map(str::to_string);
    Ok(())
}

pub fn auto_map_taxonomy(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    offline_mode: bool,
    report: &mut ImportReport,
) -> EngineResult<usize> {
    let existing_categories = meta::read_categories_json(data_dir).unwrap_or_default();
    let existing_tags = qt::list_tags_by_project(conn, project_id)?
        .into_iter()
        .map(|tag| tag.name)
        .collect::<Vec<_>>();
    let imported_categories = report
        .taxonomies
        .iter()
        .filter(|item| item.kind == TaxonomyKind::Category && !item.exists_in_project)
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    let imported_tags = report
        .taxonomies
        .iter()
        .filter(|item| item.kind == TaxonomyKind::Tag && !item.exists_in_project)
        .map(|item| item.name.clone())
        .collect::<Vec<_>>();
    let request = ai::OneShotRequest {
        operation: ai::OneShotOperation::MapImportTaxonomy,
        content: json!({
            "imported_categories": imported_categories,
            "imported_tags": imported_tags,
            "existing_categories": existing_categories,
            "existing_tags": existing_tags,
        }),
    };
    let (response, _) = ai::run_one_shot(conn, offline_mode, &request)?;
    let ai::OneShotResponse::ImportTaxonomyMapping(mapping) = response else {
        return Err(EngineError::Parse(
            "AI returned the wrong response for import taxonomy mapping".to_string(),
        ));
    };
    Ok(apply_ai_taxonomy_mapping(
        report,
        &mapping,
        &existing_categories,
        &existing_tags,
    ))
}

fn apply_ai_taxonomy_mapping(
    report: &mut ImportReport,
    mapping: &ai::ImportTaxonomyMapping,
    existing_categories: &[String],
    existing_tags: &[String],
) -> usize {
    let mut count = 0;
    for item in &mut report.taxonomies {
        let (mapped, existing) = match item.kind {
            TaxonomyKind::Category => (
                mapping.category_mappings.get(&item.name),
                existing_categories,
            ),
            TaxonomyKind::Tag => (mapping.tag_mappings.get(&item.name), existing_tags),
        };
        let canonical = mapped.and_then(|mapped| {
            existing
                .iter()
                .find(|candidate| candidate.eq_ignore_ascii_case(mapped.trim()))
        });
        if let Some(canonical) = canonical {
            item.mapped_to = Some(canonical.clone());
            count += 1;
        }
    }
    count
}

pub fn empty_report() -> ImportReport {
    ImportReport::default()
}

pub fn taxonomy_candidate(
    kind: TaxonomyKind,
    name: &str,
    exists_in_project: bool,
) -> TaxonomyCandidate {
    TaxonomyCandidate {
        kind,
        name: name.to_string(),
        slug: None,
        exists_in_project,
        mapped_to: None,
    }
}

pub fn execute_import(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    report: &ImportReport,
    default_author: Option<&str>,
    mut progress: Option<&mut dyn FnMut(ImportProgress)>,
) -> EngineResult<ImportExecutionResult> {
    let started = Instant::now();
    let taxonomy_map = taxonomy_mapping(conn, data_dir, project_id, report)?;
    let mut result = ImportExecutionResult::default();
    let mut post_ids = HashMap::new();

    notify(
        &mut progress,
        ImportPhase::Taxonomy,
        0,
        report.taxonomies.len(),
        "creating_taxonomy",
        Some(started),
    );
    run_batches(
        conn,
        data_dir,
        &report.taxonomies,
        |_| {
            Ok(vec![
                PathBuf::from("meta/categories.json"),
                PathBuf::from("meta/category-meta.json"),
                PathBuf::from("meta/tags.json"),
            ])
        },
        |item| {
            let imported = if item.exists_in_project || item.mapped_to.is_some() {
                false
            } else {
                match item.kind {
                    TaxonomyKind::Category => meta::add_category(data_dir, &item.name)?,
                    TaxonomyKind::Tag => {
                        tag::create_tag(conn, data_dir, project_id, &item.name, None)?;
                    }
                }
                true
            };
            if imported {
                result.taxonomy.imported += 1;
            } else {
                result.taxonomy.skipped += 1;
            }
            let current = result.taxonomy.imported + result.taxonomy.skipped;
            notify(
                &mut progress,
                ImportPhase::Taxonomy,
                current,
                report.taxonomies.len(),
                &item.name,
                Some(started),
            );
            Ok(())
        },
    )?;

    execute_post_phase(
        conn,
        data_dir,
        project_id,
        &report.posts,
        default_author,
        &taxonomy_map,
        &mut result.posts,
        &mut post_ids,
        ImportPhase::Posts,
        &mut progress,
        started,
    )?;
    execute_media_phase(
        conn,
        data_dir,
        project_id,
        &report.media,
        default_author,
        &post_ids,
        &mut result.media,
        &mut progress,
        started,
    )?;
    execute_post_phase(
        conn,
        data_dir,
        project_id,
        &report.pages,
        default_author,
        &taxonomy_map,
        &mut result.pages,
        &mut post_ids,
        ImportPhase::Pages,
        &mut progress,
        started,
    )?;
    notify(
        &mut progress,
        ImportPhase::Complete,
        1,
        1,
        "import_complete",
        Some(started),
    );
    Ok(result)
}

fn taxonomy_mapping(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    report: &ImportReport,
) -> EngineResult<HashMap<(TaxonomyKind, String), String>> {
    let categories = meta::read_categories_json(data_dir).unwrap_or_default();
    let tags = qt::list_tags_by_project(conn, project_id)?;
    let mut mapping = HashMap::new();
    for item in &report.taxonomies {
        let requested = item.mapped_to.as_deref().unwrap_or(&item.name);
        let canonical = match item.kind {
            TaxonomyKind::Category => categories
                .iter()
                .find(|name| name.eq_ignore_ascii_case(requested))
                .cloned(),
            TaxonomyKind::Tag => tags
                .iter()
                .find(|tag| tag.name.eq_ignore_ascii_case(requested))
                .map(|tag| tag.name.clone()),
        };
        if item.mapped_to.is_some() && canonical.is_none() {
            return Err(EngineError::Validation(format!(
                "import taxonomy mapping target does not exist: {requested}"
            )));
        }
        mapping.insert(
            (item.kind, item.name.to_lowercase()),
            canonical.unwrap_or_else(|| item.name.clone()),
        );
    }
    Ok(mapping)
}

#[expect(clippy::too_many_arguments, reason = "phase orchestration inputs")]
fn execute_post_phase(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    items: &[ImportCandidate],
    default_author: Option<&str>,
    taxonomy: &HashMap<(TaxonomyKind, String), String>,
    counts: &mut ImportExecutionCounts,
    post_ids: &mut HashMap<i64, String>,
    phase: ImportPhase,
    progress: &mut Option<&mut dyn FnMut(ImportProgress)>,
    started: Instant,
) -> EngineResult<()> {
    notify(
        progress,
        phase,
        0,
        items.len(),
        match phase {
            ImportPhase::Posts => "importing_posts",
            ImportPhase::Pages => "importing_pages",
            _ => "importing_items",
        },
        Some(started),
    );
    run_batches(
        conn,
        data_dir,
        items,
        |item| touched_post_paths(conn, item),
        |item| {
            if !should_import(item) {
                counts.skipped += 1;
            } else {
                let post = import_post_item(
                    conn,
                    data_dir,
                    project_id,
                    item,
                    default_author,
                    taxonomy,
                    phase == ImportPhase::Pages,
                )?;
                counts.imported += 1;
                if let Some(source_id) = item.source_id {
                    post_ids.insert(source_id, post.id);
                }
            }
            notify(
                progress,
                phase,
                counts.imported + counts.skipped,
                items.len(),
                &item.title,
                Some(started),
            );
            Ok(())
        },
    )
}

fn import_post_item(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    item: &ImportCandidate,
    default_author: Option<&str>,
    taxonomy: &HashMap<(TaxonomyKind, String), String>,
    page: bool,
) -> EngineResult<Post> {
    let author = item.author.as_deref().or(default_author);
    let tags = resolve_terms(&item.tags, TaxonomyKind::Tag, taxonomy);
    let mut categories = resolve_terms(&item.categories, TaxonomyKind::Category, taxonomy);
    if page
        && !categories
            .iter()
            .any(|category| category.eq_ignore_ascii_case("page"))
    {
        categories.push("page".to_string());
    }
    let content = item.content.as_deref().unwrap_or("");
    let mut imported = if item.status == ImportItemStatus::Conflict
        && item.resolution == Some(ImportResolution::Overwrite)
    {
        let id = item
            .existing_id
            .as_deref()
            .ok_or_else(|| EngineError::NotFound("conflict target".to_string()))?;
        post::update_post(
            conn,
            data_dir,
            id,
            Some(&item.title),
            None,
            Some(item.excerpt.as_deref()),
            Some(content),
            Some(tags),
            Some(categories),
            Some(author),
            None,
            None,
            None,
        )?
    } else {
        let created = post::create_post(
            conn,
            data_dir,
            project_id,
            &item.title,
            Some(content),
            tags,
            categories,
            author,
            None,
            None,
        )?;
        let desired_slug = item.slug.as_deref().unwrap_or(&created.slug);
        if item.status != ImportItemStatus::Conflict
            || item.resolution != Some(ImportResolution::Import)
        {
            post::update_post(
                conn,
                data_dir,
                &created.id,
                None,
                Some(desired_slug),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )?
        } else {
            created
        }
    };

    let previous_file_path = imported.file_path.clone();
    let previous_translation_paths = qpt::list_post_translations_by_post(conn, &imported.id)?
        .into_iter()
        .filter(|translation| !translation.file_path.is_empty())
        .map(|translation| translation.file_path)
        .collect::<Vec<_>>();
    imported.created_at = item.created_at.unwrap_or(imported.created_at);
    imported.updated_at = item.updated_at.unwrap_or(imported.created_at);
    imported.published_at = if item.source_status.as_deref() == Some("publish") {
        item.published_at.or(Some(imported.created_at))
    } else {
        None
    };
    imported.checksum = item.checksum.clone();
    qp::update_post(conn, &imported)?;
    if item.source_status.as_deref() == Some("publish") {
        imported = post::publish_post(conn, data_dir, &imported.id)?;
        let body = content.to_string();
        imported.created_at = item.created_at.unwrap_or(imported.created_at);
        imported.updated_at = item.updated_at.unwrap_or(imported.created_at);
        imported.published_at = item.published_at.or(Some(imported.created_at));
        imported.file_path = post_file_path(imported.created_at, &imported.slug);
        let serialized = crate::util::frontmatter::write_post_file(&imported, &body);
        atomic_write_str(&data_dir.join(&imported.file_path), &serialized)?;
        if !previous_file_path.is_empty() && previous_file_path != imported.file_path {
            remove_file_if_present(&data_dir.join(previous_file_path))?;
        }
        let current_translation_paths = qpt::list_post_translations_by_post(conn, &imported.id)?
            .into_iter()
            .map(|translation| translation.file_path)
            .collect::<HashSet<_>>();
        for previous_path in previous_translation_paths {
            if !current_translation_paths.contains(&previous_path) {
                remove_file_if_present(&data_dir.join(previous_path))?;
            }
        }
        qp::update_post(conn, &imported)?;
    }
    Ok(imported)
}

fn resolve_terms(
    terms: &[String],
    kind: TaxonomyKind,
    taxonomy: &HashMap<(TaxonomyKind, String), String>,
) -> Vec<String> {
    let mut result = Vec::new();
    for term in terms {
        let resolved = taxonomy
            .get(&(kind, term.to_lowercase()))
            .cloned()
            .unwrap_or_else(|| term.clone());
        if !result
            .iter()
            .any(|item: &String| item.eq_ignore_ascii_case(&resolved))
        {
            result.push(resolved);
        }
    }
    result
}

#[expect(clippy::too_many_arguments, reason = "phase orchestration inputs")]
fn execute_media_phase(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    items: &[ImportCandidate],
    default_author: Option<&str>,
    post_ids: &HashMap<i64, String>,
    counts: &mut ImportExecutionCounts,
    progress: &mut Option<&mut dyn FnMut(ImportProgress)>,
    started: Instant,
) -> EngineResult<()> {
    notify(
        progress,
        ImportPhase::Media,
        0,
        items.len(),
        "importing_media",
        Some(started),
    );
    run_batches(
        conn,
        data_dir,
        items,
        |item| touched_media_paths(conn, data_dir, item),
        |item| {
            if !should_import(item) {
                counts.skipped += 1;
            } else {
                let source = item
                    .source_path
                    .as_deref()
                    .map(Path::new)
                    .filter(|path| path.is_file())
                    .ok_or_else(|| EngineError::NotFound(format!("media source {}", item.title)))?;
                let filename = item.filename.as_deref().unwrap_or("imported-media");
                let imported = if item.status == ImportItemStatus::Conflict
                    && item.resolution == Some(ImportResolution::Overwrite)
                {
                    let id = item.existing_id.as_deref().ok_or_else(|| {
                        EngineError::NotFound("media conflict target".to_string())
                    })?;
                    media::replace_media_file(conn, data_dir, id, source)?;
                    media::update_media(
                        conn,
                        data_dir,
                        id,
                        Some(Some(&item.title)),
                        Some(item.description.as_deref()),
                        None,
                        Some(default_author),
                        None,
                        None,
                    )?
                } else {
                    media::import_media_at(
                        conn,
                        data_dir,
                        project_id,
                        source,
                        filename,
                        nonempty(&item.title).as_deref(),
                        item.description.as_deref(),
                        None,
                        default_author,
                        None,
                        Vec::new(),
                        item.checksum.as_deref(),
                        item.created_at.unwrap_or_else(now_unix_ms),
                    )?
                };
                if let Some(parent_id) = item.parent_source_id.and_then(|id| post_ids.get(&id)) {
                    post_media::link_media_to_post(
                        conn,
                        data_dir,
                        project_id,
                        parent_id,
                        &imported.id,
                        0,
                    )?;
                }
                counts.imported += 1;
            }
            notify(
                progress,
                ImportPhase::Media,
                counts.imported + counts.skipped,
                items.len(),
                item.filename.as_deref().unwrap_or(&item.title),
                Some(started),
            );
            Ok(())
        },
    )
}

fn should_import(item: &ImportCandidate) -> bool {
    item.status == ImportItemStatus::New
        || (item.status == ImportItemStatus::Conflict
            && matches!(
                item.resolution,
                Some(ImportResolution::Overwrite | ImportResolution::Import)
            ))
}

fn touched_post_paths(conn: &Connection, item: &ImportCandidate) -> EngineResult<Vec<PathBuf>> {
    if item.status != ImportItemStatus::Conflict
        || item.resolution != Some(ImportResolution::Overwrite)
    {
        return Ok(Vec::new());
    }
    let id = item
        .existing_id
        .as_deref()
        .ok_or_else(|| EngineError::NotFound("post conflict target".to_string()))?;
    let post = qp::get_post_by_id(conn, id)?;
    let mut paths = Vec::new();
    if !post.file_path.is_empty() {
        paths.push(PathBuf::from(post.file_path));
    }
    paths.extend(
        qpt::list_post_translations_by_post(conn, id)?
            .into_iter()
            .filter(|translation| !translation.file_path.is_empty())
            .map(|translation| PathBuf::from(translation.file_path)),
    );
    Ok(paths)
}

fn touched_media_paths(
    conn: &Connection,
    data_dir: &Path,
    item: &ImportCandidate,
) -> EngineResult<Vec<PathBuf>> {
    if item.status != ImportItemStatus::Conflict
        || item.resolution != Some(ImportResolution::Overwrite)
    {
        return Ok(Vec::new());
    }
    let id = item
        .existing_id
        .as_deref()
        .ok_or_else(|| EngineError::NotFound("media conflict target".to_string()))?;
    let media = qm::get_media_by_id(conn, id)?;
    let mut paths = vec![PathBuf::from(&media.file_path)];
    paths.push(PathBuf::from(if media.sidecar_path.is_empty() {
        media_sidecar_path(&media.file_path)
    } else {
        media.sidecar_path.clone()
    }));
    paths.extend(
        qmt::list_media_translations_by_media(conn, id)?
            .into_iter()
            .map(|translation| {
                PathBuf::from(media_translation_sidecar_path(
                    &media.file_path,
                    &translation.language,
                ))
            }),
    );

    let thumbnail_dir = data_dir.join("thumbnails").join(&id[..2.min(id.len())]);
    if thumbnail_dir.is_dir() {
        for entry in fs::read_dir(&thumbnail_dir)? {
            let entry = entry?;
            let filename = entry.file_name();
            if entry.file_type()?.is_file()
                && filename
                    .to_str()
                    .is_some_and(|filename| filename.starts_with(&format!("{id}-")))
            {
                paths.push(
                    entry
                        .path()
                        .strip_prefix(data_dir)
                        .map_err(|error| EngineError::Validation(error.to_string()))?
                        .to_path_buf(),
                );
            }
        }
    }
    Ok(paths)
}

fn run_batches<T>(
    conn: &Connection,
    data_dir: &Path,
    items: &[T],
    mut touched_paths: impl FnMut(&T) -> EngineResult<Vec<PathBuf>>,
    mut operation: impl FnMut(&T) -> EngineResult<()>,
) -> EngineResult<()> {
    for batch in items.chunks(TRANSACTION_BATCH_SIZE) {
        let touched = batch
            .iter()
            .map(&mut touched_paths)
            .collect::<EngineResult<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<HashSet<_>>();
        let filesystem = FilesystemSnapshot::capture(data_dir, &touched)?;
        conn.begin_savepoint()?;
        for item in batch {
            if let Err(error) = operation(item) {
                let _ = conn.rollback_savepoint();
                filesystem.restore(data_dir)?;
                return Err(error);
            }
        }
        if let Err(error) = conn.release_savepoint() {
            let _ = conn.rollback_savepoint();
            filesystem.restore(data_dir)?;
            return Err(error.into());
        }
    }
    Ok(())
}

#[derive(Debug)]
struct FilesystemSnapshot {
    existing_files: HashSet<PathBuf>,
    directories: HashSet<PathBuf>,
    backups: HashMap<PathBuf, Vec<u8>>,
}

impl FilesystemSnapshot {
    fn capture(data_dir: &Path, touched_paths: &HashSet<PathBuf>) -> EngineResult<Self> {
        let mut existing_files = HashSet::new();
        let mut directories = HashSet::new();
        if data_dir.exists() {
            for entry in walkdir::WalkDir::new(data_dir) {
                let entry = entry.map_err(|error| EngineError::Io(error.into()))?;
                let relative = entry
                    .path()
                    .strip_prefix(data_dir)
                    .map_err(|error| EngineError::Parse(error.to_string()))?
                    .to_path_buf();
                if relative.as_os_str().is_empty() {
                    continue;
                }
                if entry.file_type().is_dir() {
                    directories.insert(relative);
                } else if entry.file_type().is_file() {
                    existing_files.insert(relative);
                }
            }
        }
        let backups = touched_paths
            .iter()
            .map(|path| validated_relative_path(data_dir, path))
            .collect::<EngineResult<HashSet<_>>>()?
            .into_iter()
            .filter(|path| existing_files.contains(path))
            .map(|path| fs::read(data_dir.join(&path)).map(|content| (path, content)))
            .collect::<Result<HashMap<_, _>, _>>()?;
        Ok(Self {
            existing_files,
            directories,
            backups,
        })
    }

    fn restore(self, data_dir: &Path) -> EngineResult<()> {
        if data_dir.exists() {
            let mut current = walkdir::WalkDir::new(data_dir)
                .min_depth(1)
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| EngineError::Io(error.into()))?;
            current.sort_by_key(|entry| std::cmp::Reverse(entry.depth()));
            for entry in current {
                let relative = entry
                    .path()
                    .strip_prefix(data_dir)
                    .map_err(|error| EngineError::Parse(error.to_string()))?;
                if entry.file_type().is_file() && !self.existing_files.contains(relative) {
                    fs::remove_file(entry.path())?;
                } else if entry.file_type().is_dir() && !self.directories.contains(relative) {
                    fs::remove_dir(entry.path())?;
                }
            }
        }
        for (relative, content) in self.backups {
            let path = data_dir.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, content)?;
        }
        Ok(())
    }
}

fn validated_relative_path(data_dir: &Path, path: &Path) -> EngineResult<PathBuf> {
    let relative = if path.is_absolute() {
        path.strip_prefix(data_dir).map_err(|_| {
            EngineError::Validation(format!("path escapes data folder: {}", path.display()))
        })?
    } else {
        path
    };
    if relative.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(EngineError::Validation(format!(
            "path escapes data folder: {}",
            path.display()
        )));
    }
    Ok(relative.to_path_buf())
}

fn remove_file_if_present(path: &Path) -> EngineResult<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn notify(
    callback: &mut Option<&mut dyn FnMut(ImportProgress)>,
    phase: ImportPhase,
    current: usize,
    total: usize,
    detail: &str,
    started: Option<Instant>,
) {
    let Some(callback) = callback.as_deref_mut() else {
        return;
    };
    let eta_ms = started.and_then(|started| {
        (current > 0 && current < total).then(|| {
            (started.elapsed().as_millis() as u64 / current as u64)
                .saturating_mul((total - current) as u64)
        })
    });
    let _ = catch_unwind(AssertUnwindSafe(|| {
        callback(ImportProgress {
            phase,
            current,
            total,
            detail: detail.to_string(),
            eta_ms,
        })
    }));
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn ai_mapping_only_accepts_canonical_existing_terms() {
        let mut report = ImportReport {
            taxonomies: vec![
                taxonomy_candidate(TaxonomyKind::Category, "Old Engineering", false),
                taxonomy_candidate(TaxonomyKind::Tag, "Old Rust", false),
                taxonomy_candidate(TaxonomyKind::Tag, "Unknown", false),
            ],
            ..ImportReport::default()
        };
        let mapping = ai::ImportTaxonomyMapping {
            category_mappings: BTreeMap::from([(
                "Old Engineering".to_string(),
                "engineering".to_string(),
            )]),
            tag_mappings: BTreeMap::from([
                ("Old Rust".to_string(), "RUST".to_string()),
                ("Unknown".to_string(), "Invented".to_string()),
            ]),
        };

        let count = apply_ai_taxonomy_mapping(
            &mut report,
            &mapping,
            &["Engineering".to_string()],
            &["Rust".to_string()],
        );

        assert_eq!(count, 2);
        assert_eq!(
            report.taxonomies[0].mapped_to.as_deref(),
            Some("Engineering")
        );
        assert_eq!(report.taxonomies[1].mapped_to.as_deref(), Some("Rust"));
        assert!(report.taxonomies[2].mapped_to.is_none());
    }

    #[test]
    fn progress_reports_eta_without_propagating_callback_panics() {
        let started = Instant::now();
        let mut observed = None;
        notify(
            &mut Some(&mut |progress| observed = Some(progress)),
            ImportPhase::Posts,
            1,
            2,
            "first",
            Some(started),
        );
        assert!(observed.unwrap().eta_ms.is_some());

        notify(
            &mut Some(&mut |_| panic!("observer failed")),
            ImportPhase::Posts,
            1,
            2,
            "first",
            Some(started),
        );
    }
}
