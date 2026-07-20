use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

use axum::Router;
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use serde::Deserialize;
use tokio::sync::oneshot;

use crate::db::{Database, queries};
use crate::engine::generation::PublishedPostSource;
use crate::engine::{EngineError, EngineResult};
use crate::model::{Post, PostStatus, ProjectMetadata, pico_stylesheet_href};
use crate::render::build_preview_response;
use crate::util::frontmatter::{read_post_file, read_translation_file};

pub const PREVIEW_HOST: &str = "127.0.0.1";
pub const PREVIEW_PORT: u16 = 4123;

#[derive(Debug)]
pub struct PreviewServerHandle {
    shutdown: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl PreviewServerHandle {
    pub fn stop(mut self) -> EngineResult<()> {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        Ok(())
    }
}

impl Drop for PreviewServerHandle {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Debug, Clone)]
struct PreviewServerState {
    db_path: PathBuf,
    data_dir: PathBuf,
    project_id: String,
}

#[derive(Debug, Deserialize, Default)]
struct DraftPreviewQuery {
    language: Option<String>,
    theme: Option<String>,
    mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct StylePreviewQuery {
    theme: Option<String>,
    mode: Option<String>,
}

pub fn start_preview_server(
    db_path: PathBuf,
    data_dir: PathBuf,
    project_id: String,
) -> EngineResult<PreviewServerHandle> {
    let state = PreviewServerState {
        db_path,
        data_dir,
        project_id,
    };
    let listener = std::net::TcpListener::bind((PREVIEW_HOST, PREVIEW_PORT)).map_err(|error| {
        if error.kind() == std::io::ErrorKind::AddrInUse {
            EngineError::Conflict(format!(
                "preview server already running on {PREVIEW_HOST}:{PREVIEW_PORT}"
            ))
        } else {
            EngineError::Io(error)
        }
    })?;
    listener.set_nonblocking(true)?;

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let thread = thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("preview runtime");
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).expect("preview listener");
            let app = Router::new()
                .route("/__draft/{post_id}", get(handle_draft_preview))
                .route("/__style-preview", get(handle_style_preview))
                .route("/", get(handle_preview_request))
                .route("/{*path}", get(handle_preview_request))
                .with_state(state);
            let server = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            });
            let _ = server.await;
        });
    });

    Ok(PreviewServerHandle {
        shutdown: Some(shutdown_tx),
        thread: Some(thread),
    })
}

async fn handle_preview_request(
    State(state): State<PreviewServerState>,
    Query(query): Query<StylePreviewQuery>,
    uri: Uri,
) -> Response {
    match render_preview_response(&state, uri.path(), None, Some(&query)) {
        Ok(response) => response,
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn handle_draft_preview(
    State(state): State<PreviewServerState>,
    AxumPath(post_id): AxumPath<String>,
    Query(query): Query<DraftPreviewQuery>,
) -> Response {
    let style = StylePreviewQuery {
        theme: query.theme.clone(),
        mode: query.mode.clone(),
    };
    match render_preview_response(
        &state,
        &format!("/__draft/{post_id}"),
        query.language.as_deref(),
        Some(&style),
    ) {
        Ok(response) => response,
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn handle_style_preview(
    State(state): State<PreviewServerState>,
    Query(query): Query<StylePreviewQuery>,
) -> Response {
    let metadata = match crate::engine::meta::read_project_json(&state.data_dir) {
        Ok(metadata) => metadata,
        Err(error) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string());
        }
    };
    let theme = query.theme.as_deref().unwrap_or("default");
    let stylesheet = pico_stylesheet_href(Some(theme));
    let mode = match query.mode.as_deref().map(str::trim) {
        Some("light") => Some("light"),
        Some("dark") => Some("dark"),
        _ => None,
    };
    let mode_attributes = mode
        .map(|mode| format!(" data-theme=\"{mode}\" data-mode=\"{mode}\""))
        .unwrap_or_default();
    let project_name = escape_html(&metadata.name);
    let language = escape_html(metadata.main_language.as_deref().unwrap_or("en"));
    let description = escape_html(metadata.description.as_deref().unwrap_or(&metadata.name));
    let html = format!(
        "<!doctype html><html lang=\"{language}\"{mode_attributes}><head><meta charset=\"utf-8\" /><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" /><title>{project_name}</title><link rel=\"stylesheet\" href=\"{stylesheet}\" /><link rel=\"stylesheet\" href=\"/assets/bds.css\" /></head><body><main class=\"container\"><nav><ul><li><strong>{project_name}</strong></li></ul></nav><article><header><h1>{project_name}</h1></header><p>{description}</p><progress value=\"70\" max=\"100\"></progress><input type=\"text\" value=\"{project_name}\" aria-label=\"{project_name}\" /><button type=\"button\">{project_name}</button></article></main></body></html>"
    );
    Html(html).into_response()
}

fn render_preview_response(
    state: &PreviewServerState,
    path: &str,
    requested_language: Option<&str>,
    style: Option<&StylePreviewQuery>,
) -> EngineResult<Response> {
    if let Some(post_id) = path.strip_prefix("/__draft/") {
        let html = apply_preview_style(
            render_draft_preview(state, post_id, requested_language)?,
            style,
        );
        return Ok(Html(html).into_response());
    }

    if let Some(file_response) = serve_project_file(&state.data_dir, path)? {
        return Ok(file_response);
    }

    let metadata = crate::engine::meta::read_project_json(&state.data_dir)?;
    let db = Database::open(&state.db_path)?;
    let published_posts = collect_published_posts(state, &metadata)?;
    let input_posts = published_posts
        .iter()
        .map(|source| (source.post.clone(), source.body_markdown.clone()))
        .collect::<Vec<_>>();
    let response = build_preview_response(
        db.conn(),
        &state.data_dir,
        &state.project_id,
        &metadata,
        &input_posts,
        path,
    )
    .map_err(|error| EngineError::Parse(error.to_string()))?;
    let status = StatusCode::from_u16(response.status_code).unwrap_or(StatusCode::OK);
    Ok((status, Html(apply_preview_style(response.html, style))).into_response())
}

fn apply_preview_style(html: String, style: Option<&StylePreviewQuery>) -> String {
    let Some(style) = style else {
        return html;
    };

    let mut styled = html;
    if let Some(theme) = style.theme.as_deref().filter(|theme| !theme.is_empty()) {
        styled = override_pico_stylesheet(&styled, &pico_stylesheet_href(Some(theme)));
    }
    if let Some(mode) = style.mode.as_deref() {
        let mode = match mode.trim() {
            "light" => Some("light"),
            "dark" => Some("dark"),
            _ => None,
        };
        styled = override_html_attribute(&styled, "data-theme", mode);
        styled = override_html_attribute(&styled, "data-mode", mode);
    }
    styled
}

fn override_pico_stylesheet(html: &str, href: &str) -> String {
    let Some(start) = html.find("/assets/pico") else {
        return html.to_string();
    };
    let Some(end_offset) = html[start..].find(".min.css") else {
        return html.to_string();
    };
    let end = start + end_offset + ".min.css".len();
    format!("{}{}{}", &html[..start], href, &html[end..])
}

fn override_html_attribute(html: &str, attribute: &str, value: Option<&str>) -> String {
    let Some(html_start) = html.find("<html") else {
        return html.to_string();
    };
    let Some(tag_end_offset) = html[html_start..].find('>') else {
        return html.to_string();
    };
    let tag_end = html_start + tag_end_offset;
    let attribute_start_pattern = format!(" {attribute}=\"");
    let existing = html[html_start..tag_end]
        .find(&attribute_start_pattern)
        .and_then(|offset| {
            let start = html_start + offset;
            html[start + attribute_start_pattern.len()..tag_end]
                .find('"')
                .map(|end_offset| {
                    (
                        start,
                        start + attribute_start_pattern.len() + end_offset + 1,
                    )
                })
        });

    match (existing, value) {
        (Some((start, end)), Some(value)) => format!(
            "{} {attribute}=\"{}\"{}",
            &html[..start],
            value,
            &html[end..]
        ),
        (Some((start, end)), None) => format!("{}{}", &html[..start], &html[end..]),
        (None, Some(value)) => format!(
            "{} {attribute}=\"{}\"{}",
            &html[..tag_end],
            value,
            &html[tag_end..]
        ),
        (None, None) => html.to_string(),
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn render_draft_preview(
    state: &PreviewServerState,
    post_id: &str,
    requested_language: Option<&str>,
) -> EngineResult<String> {
    let db = Database::open(&state.db_path)?;
    let metadata = crate::engine::meta::read_project_json(&state.data_dir)?;
    let post = queries::post::get_post_by_id(db.conn(), post_id)?;
    let canonical_language = post
        .language
        .as_deref()
        .unwrap_or_else(|| metadata.main_language.as_deref().unwrap_or("en"));
    let target_language = requested_language.unwrap_or(canonical_language);

    if target_language != canonical_language
        && let Ok(translation) =
            queries::post_translation::get_post_translation_by_post_and_language(
                db.conn(),
                post_id,
                target_language,
            )
    {
        let mut translated_post = post.clone();
        translated_post.title = translation.title.clone();
        translated_post.excerpt = translation.excerpt.clone();
        translated_post.language = Some(translation.language.clone());
        translated_post.status = translation.status.clone();
        translated_post.file_path = translation.file_path.clone();
        translated_post.published_at = translation.published_at.or(post.published_at);
        let body = load_translation_body(&state.data_dir, &translation)?;
        let response = build_preview_response(
            db.conn(),
            &state.data_dir,
            &state.project_id,
            &metadata,
            &[(translated_post, body)],
            &crate::render::build_canonical_post_path(
                &post,
                target_language,
                metadata.main_language.as_deref().unwrap_or("en"),
            ),
        )
        .map_err(|error| EngineError::Parse(error.to_string()))?;
        return Ok(response.html);
    }

    let body = load_post_body(&state.data_dir, &post)?;
    let response = build_preview_response(
        db.conn(),
        &state.data_dir,
        &state.project_id,
        &metadata,
        &[(post.clone(), body)],
        &crate::render::build_canonical_post_path(
            &post,
            canonical_language,
            metadata.main_language.as_deref().unwrap_or("en"),
        ),
    )
    .map_err(|error| EngineError::Parse(error.to_string()))?;
    Ok(response.html)
}

fn collect_published_posts(
    state: &PreviewServerState,
    _metadata: &ProjectMetadata,
) -> EngineResult<Vec<PublishedPostSource>> {
    let db = Database::open(&state.db_path)?;
    let posts = queries::post::list_posts_by_project(db.conn(), &state.project_id)?;
    let mut published = Vec::new();
    for post in posts
        .into_iter()
        .filter(|post| matches!(post.status, PostStatus::Published))
    {
        published.push(PublishedPostSource {
            body_markdown: load_post_body(&state.data_dir, &post)?,
            post,
        });
    }
    published.sort_by_key(|source| source.post.published_at.unwrap_or(source.post.created_at));
    Ok(published)
}

fn load_post_body(data_dir: &Path, post: &Post) -> EngineResult<String> {
    if let Some(content) = &post.content {
        return Ok(content.clone());
    }
    if let Some(content) = &post.published_content {
        return Ok(content.clone());
    }
    load_markdown_body(data_dir, &post.file_path, false)
}

fn load_translation_body(
    data_dir: &Path,
    translation: &crate::model::PostTranslation,
) -> EngineResult<String> {
    if let Some(content) = &translation.content {
        return Ok(content.clone());
    }
    load_markdown_body(data_dir, &translation.file_path, true)
}

fn load_markdown_body(
    data_dir: &Path,
    relative_path: &str,
    translation: bool,
) -> EngineResult<String> {
    let raw = fs::read_to_string(data_dir.join(relative_path.trim_start_matches('/')))?;
    let body = if translation {
        read_translation_file(&raw).map(|(_, body)| body)
    } else {
        read_post_file(&raw).map(|(_, body)| body)
    }
    .map_err(EngineError::Parse)?;
    Ok(body)
}

fn serve_project_file(data_dir: &Path, path: &str) -> EngineResult<Option<Response>> {
    if let Some(response) = serve_scoped_file(data_dir, path, "/media/", "media")? {
        return Ok(Some(response));
    }
    if let Some(response) = serve_scoped_file(data_dir, path, "/assets/", "assets")? {
        return Ok(Some(response));
    }
    Ok(None)
}

fn serve_scoped_file(
    data_dir: &Path,
    path: &str,
    prefix: &str,
    scope_dir: &str,
) -> EngineResult<Option<Response>> {
    let Some(relative) = path.strip_prefix(prefix) else {
        return Ok(None);
    };
    let scope_root = data_dir.join(scope_dir);
    let candidate = scope_root.join(relative);
    if !candidate.exists() || !candidate.is_file() {
        let bundled_path = format!("{scope_dir}/{relative}");
        if let Some(bytes) = crate::engine::site_assets::bundled_site_asset(&bundled_path) {
            let mime = guess_content_type(Path::new(&bundled_path));
            return Ok(Some(
                (StatusCode::OK, [(header::CONTENT_TYPE, mime)], bytes).into_response(),
            ));
        }
        return Ok(Some(error_response(
            StatusCode::NOT_FOUND,
            "preview asset not found",
        )));
    }
    let canonical_candidate = candidate.canonicalize()?;
    let canonical_scope_root = scope_root.canonicalize().unwrap_or(scope_root);
    if !canonical_candidate.starts_with(&canonical_scope_root) {
        return Ok(Some(error_response(
            StatusCode::NOT_FOUND,
            "preview asset not found",
        )));
    }
    let bytes = fs::read(&canonical_candidate)?;
    let mime = guess_content_type(&canonical_candidate);
    Ok(Some(
        (StatusCode::OK, [(header::CONTENT_TYPE, mime)], bytes).into_response(),
    ))
}

fn guess_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
    {
        "css" => "text/css; charset=utf-8",
        "js" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        message.to_string(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries;
    use crate::engine::meta;
    use crate::model::{Post, PostStatus, Project, ProjectMetadata};
    use std::sync::{Mutex, OnceLock};

    fn preview_port_guard() -> &'static Mutex<()> {
        static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
        GUARD.get_or_init(|| Mutex::new(()))
    }

    fn setup_preview_fixture() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("meta")).unwrap();
        std::fs::create_dir_all(dir.path().join("posts/2024/03")).unwrap();
        std::fs::create_dir_all(dir.path().join("media")).unwrap();

        meta::write_project_json(dir.path(), &make_metadata()).unwrap();

        let db_path = dir.path().join("bds.db");
        let db = Database::open(&db_path).unwrap();
        db.migrate().unwrap();
        queries::project::insert_project(
            db.conn(),
            &Project {
                id: "project-1".into(),
                name: "Blog".into(),
                slug: "blog".into(),
                description: None,
                data_path: Some(dir.path().to_string_lossy().to_string()),
                is_active: true,
                created_at: 1_710_000_000_000,
                updated_at: 1_710_000_000_000,
            },
        )
        .unwrap();
        (dir, db)
    }

    fn make_metadata() -> ProjectMetadata {
        ProjectMetadata {
            name: "Blog".into(),
            description: None,
            public_url: Some("https://example.com".into()),
            main_language: Some("en".into()),
            default_author: None,
            max_posts_per_page: 50,
            image_import_concurrency: 4,
            blogmark_category: None,
            pico_theme: None,
            semantic_similarity_enabled: false,
            blog_languages: vec!["en".into(), "de".into()],
        }
    }

    fn make_post() -> PublishedPostSource {
        PublishedPostSource {
            post: Post {
                id: "post-1".into(),
                project_id: "project-1".into(),
                title: "Hello".into(),
                slug: "hello".into(),
                excerpt: None,
                content: Some("Body".into()),
                status: PostStatus::Published,
                author: None,
                language: Some("en".into()),
                do_not_translate: false,
                template_slug: None,
                file_path: String::new(),
                checksum: None,
                tags: vec![],
                categories: vec![],
                published_title: None,
                published_content: None,
                published_tags: None,
                published_categories: None,
                published_excerpt: None,
                created_at: 1_710_000_000_000,
                updated_at: 1_710_000_000_000,
                published_at: Some(1_710_000_000_000),
            },
            body_markdown: "Hello **world**".into(),
        }
    }

    fn make_draft_post() -> Post {
        Post {
            id: "post-1".into(),
            project_id: "project-1".into(),
            title: "Hello".into(),
            slug: "hello".into(),
            excerpt: Some("Excerpt".into()),
            content: Some("Draft **body**".into()),
            status: PostStatus::Draft,
            author: None,
            language: Some("en".into()),
            do_not_translate: false,
            template_slug: None,
            file_path: "posts/2024/03/hello.md".into(),
            checksum: None,
            tags: vec![],
            categories: vec![],
            published_title: None,
            published_content: None,
            published_tags: None,
            published_categories: None,
            published_excerpt: None,
            created_at: 1_710_000_000_000,
            updated_at: 1_710_000_000_000,
            published_at: Some(1_710_000_000_000),
        }
    }

    #[test]
    fn root_preview_renders_index_page() {
        let db = Database::open_in_memory().unwrap();
        let html = build_preview_response(
            db.conn(),
            Path::new("."),
            "project-1",
            &make_metadata(),
            &[(make_post().post, make_post().body_markdown)],
            "/",
        )
        .unwrap()
        .html;
        assert!(html.contains("post-list"));
    }

    #[test]
    fn preview_renders_single_post_for_canonical_path() {
        let db = Database::open_in_memory().unwrap();
        let source = make_post();
        let html = build_preview_response(
            db.conn(),
            Path::new("."),
            "project-1",
            &make_metadata(),
            &[(source.post, source.body_markdown)],
            "/2024/03/09/hello",
        )
        .unwrap()
        .html;
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<strong>world</strong>"));
    }

    #[test]
    fn preview_renders_language_prefixed_single_post() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("meta")).unwrap();
        std::fs::write(
            dir.path().join("posts/2024/03/hello.de.md"),
            "---\ntranslationFor: post-1\nlanguage: de\ntitle: Hallo\n---\nHallo **welt**",
        )
        .ok();
        let db = Database::open_in_memory().unwrap();
        let source = make_post();
        let html = build_preview_response(
            db.conn(),
            dir.path(),
            "project-1",
            &make_metadata(),
            &[(source.post, source.body_markdown)],
            "/de/2024/03/09/hello",
        )
        .unwrap()
        .html;
        assert!(html.contains("lang=\"de\""));
    }

    #[test]
    fn preview_server_serves_draft_post_from_localhost() {
        let _guard = preview_port_guard().lock().unwrap();
        let (dir, db) = setup_preview_fixture();
        queries::post::insert_post(db.conn(), &make_draft_post()).unwrap();

        let server = start_preview_server(
            dir.path().join("bds.db"),
            dir.path().to_path_buf(),
            "project-1".into(),
        )
        .unwrap();

        let client = reqwest::blocking::Client::new();
        let mut body = None;
        for _ in 0..20 {
            if let Ok(response) = client
                .get(format!(
                    "http://{PREVIEW_HOST}:{PREVIEW_PORT}/__draft/post-1"
                ))
                .send()
                && response.status().is_success()
            {
                body = Some(response.text().unwrap());
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        server.stop().unwrap();

        let body = body.expect("draft preview response");
        assert!(body.contains("<h1>Hello</h1>"));
        assert!(body.contains("<strong>body</strong>"));
    }

    #[test]
    fn preview_server_blocks_media_path_traversal() {
        let _guard = preview_port_guard().lock().unwrap();
        let (dir, _db) = setup_preview_fixture();
        std::fs::write(dir.path().join("outside.txt"), "nope").unwrap();
        std::fs::write(dir.path().join("media/ok.txt"), "ok").unwrap();

        let server = start_preview_server(
            dir.path().join("bds.db"),
            dir.path().to_path_buf(),
            "project-1".into(),
        )
        .unwrap();

        let client = reqwest::blocking::Client::new();
        let response = client
            .get(format!(
                "http://{PREVIEW_HOST}:{PREVIEW_PORT}/media/../outside.txt"
            ))
            .send()
            .unwrap();
        server.stop().unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn preview_server_serves_media_and_asset_files() {
        let _guard = preview_port_guard().lock().unwrap();
        let (dir, _db) = setup_preview_fixture();
        std::fs::create_dir_all(dir.path().join("assets")).unwrap();
        std::fs::write(dir.path().join("media/ok.txt"), "ok").unwrap();
        std::fs::write(dir.path().join("assets/site.css"), "body { color: red; }").unwrap();

        let server = start_preview_server(
            dir.path().join("bds.db"),
            dir.path().to_path_buf(),
            "project-1".into(),
        )
        .unwrap();

        let client = reqwest::blocking::Client::new();
        let media = client
            .get(format!("http://{PREVIEW_HOST}:{PREVIEW_PORT}/media/ok.txt"))
            .send()
            .unwrap();
        let asset = client
            .get(format!(
                "http://{PREVIEW_HOST}:{PREVIEW_PORT}/assets/site.css"
            ))
            .send()
            .unwrap();
        let media_body = media.text().unwrap();
        let asset_body = asset.text().unwrap();
        server.stop().unwrap();

        assert_eq!(media_body, "ok");
        assert!(asset_body.contains("color: red"));
    }

    #[test]
    fn preview_server_serves_style_preview() {
        let _guard = preview_port_guard().lock().unwrap();
        let (dir, _db) = setup_preview_fixture();

        let server = start_preview_server(
            dir.path().join("bds.db"),
            dir.path().to_path_buf(),
            "project-1".into(),
        )
        .unwrap();

        let client = reqwest::blocking::Client::new();
        let response = client
            .get(format!(
                "http://{PREVIEW_HOST}:{PREVIEW_PORT}/__style-preview?theme=amber&mode=dark"
            ))
            .send()
            .unwrap();
        let body = response.text().unwrap();
        let stylesheet = client
            .get(format!(
                "http://{PREVIEW_HOST}:{PREVIEW_PORT}/assets/pico.amber.min.css"
            ))
            .send()
            .unwrap();
        assert!(stylesheet.status().is_success());
        let stylesheet = stylesheet.text().unwrap();
        server.stop().unwrap();

        assert!(body.contains("<article>"));
        assert!(body.contains("Blog"));
        assert!(body.contains("href=\"/assets/pico.amber.min.css\""));
        assert!(body.contains("data-theme=\"dark\""));
        assert!(body.contains("data-mode=\"dark\""));
        assert!(!body.contains("data-theme=\"amber\""));
        assert!(stylesheet.contains("--pico-primary-background:#ffbf00"));
    }

    #[test]
    fn preview_server_applies_theme_query_to_rendered_pages() {
        let _guard = preview_port_guard().lock().unwrap();
        let (dir, _db) = setup_preview_fixture();

        let server = start_preview_server(
            dir.path().join("bds.db"),
            dir.path().to_path_buf(),
            "project-1".into(),
        )
        .unwrap();

        let client = reqwest::blocking::Client::new();
        let response = client
            .get(format!(
                "http://{PREVIEW_HOST}:{PREVIEW_PORT}/?theme=amber&mode=light"
            ))
            .send()
            .unwrap();
        let body = response.text().unwrap();
        server.stop().unwrap();

        assert!(body.contains("href=\"/assets/pico.amber.min.css\""));
        assert!(body.contains("data-theme=\"light\""));
        assert!(body.contains("data-mode=\"light\""));
        assert!(!body.contains("data-theme=\"amber\""));
    }

    #[test]
    fn auto_preview_mode_removes_forced_color_scheme_and_invalid_theme_falls_back() {
        let html = "<html data-theme=\"dark\" data-mode=\"dark\"><head><link rel=\"stylesheet\" href=\"/assets/pico.blue.min.css\"></head></html>".to_string();
        let styled = apply_preview_style(
            html,
            Some(&StylePreviewQuery {
                theme: Some("../../secret".into()),
                mode: Some("auto".into()),
            }),
        );

        assert!(styled.contains("href=\"/assets/pico.min.css\""));
        assert!(!styled.contains("data-theme="));
        assert!(!styled.contains("data-mode="));
        assert!(!styled.contains("secret"));
    }

    #[test]
    fn preview_respects_category_list_visibility_and_show_title_rules() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("meta")).unwrap();
        crate::engine::meta::write_category_meta_json(
            dir.path(),
            &std::collections::HashMap::from([
                (
                    "hidden".to_string(),
                    crate::model::CategorySettings {
                        render_in_lists: false,
                        show_title: true,
                        post_template_slug: None,
                        list_template_slug: None,
                    },
                ),
                (
                    "featured".to_string(),
                    crate::model::CategorySettings {
                        render_in_lists: true,
                        show_title: false,
                        post_template_slug: None,
                        list_template_slug: None,
                    },
                ),
            ]),
        )
        .unwrap();

        let db = Database::open_in_memory().unwrap();
        let mut hidden = make_post();
        hidden.post.title = "Hidden Post".into();
        hidden.post.slug = "hidden-post".into();
        hidden.post.categories = vec!["hidden".into()];
        hidden.body_markdown = "Hidden body".into();

        let mut featured = make_post();
        featured.post.title = "Featured Post".into();
        featured.post.slug = "featured-post".into();
        featured.post.categories = vec!["featured".into()];
        featured.body_markdown = "Featured body".into();

        let hidden_response = build_preview_response(
            db.conn(),
            dir.path(),
            "project-1",
            &make_metadata(),
            &[
                (hidden.post.clone(), hidden.body_markdown.clone()),
                (featured.post.clone(), featured.body_markdown.clone()),
            ],
            "/category/hidden",
        )
        .unwrap();
        assert_eq!(hidden_response.status_code, 404);

        let featured_response = build_preview_response(
            db.conn(),
            dir.path(),
            "project-1",
            &make_metadata(),
            &[
                (hidden.post, hidden.body_markdown),
                (featured.post, featured.body_markdown),
            ],
            "/category/featured",
        )
        .unwrap();

        assert_eq!(featured_response.status_code, 200);
        assert!(featured_response.html.contains("Featured body"));
        assert!(!featured_response.html.contains("<h2 class=\"post-title\""));
        assert!(!featured_response.html.contains("Featured Post"));
    }

    #[test]
    fn preview_renders_tag_archive_pagination_and_date_routes() {
        let db = Database::open_in_memory().unwrap();
        let mut metadata = make_metadata();
        metadata.max_posts_per_page = 1;

        let mut oldest = make_post();
        oldest.post.id = "post-1".into();
        oldest.post.slug = "alpha".into();
        oldest.post.title = "Alpha".into();
        oldest.post.tags = vec!["Rust".into()];
        oldest.post.published_at = Some(1_709_568_000_000);
        oldest.post.created_at = 1_709_568_000_000;
        oldest.body_markdown = "Alpha body".into();

        let mut newest = make_post();
        newest.post.id = "post-2".into();
        newest.post.slug = "beta".into();
        newest.post.title = "Beta".into();
        newest.post.tags = vec!["Rust".into()];
        newest.post.published_at = Some(1_710_086_400_000);
        newest.post.created_at = 1_710_086_400_000;
        newest.body_markdown = "Beta body".into();

        let mut april = make_post();
        april.post.id = "post-3".into();
        april.post.slug = "gamma".into();
        april.post.title = "Gamma".into();
        april.post.tags = vec!["Preview".into()];
        april.post.published_at = Some(1_712_016_000_000);
        april.post.created_at = 1_712_016_000_000;
        april.body_markdown = "Gamma body".into();

        let posts = vec![
            (oldest.post.clone(), oldest.body_markdown.clone()),
            (newest.post.clone(), newest.body_markdown.clone()),
            (april.post.clone(), april.body_markdown.clone()),
        ];

        let tag_first = build_preview_response(
            db.conn(),
            Path::new("."),
            "project-1",
            &metadata,
            &posts,
            "/tag/rust",
        )
        .unwrap();
        assert_eq!(tag_first.status_code, 200);
        assert!(tag_first.html.contains("Beta body"));
        assert!(!tag_first.html.contains("Alpha body"));

        let tag_second = build_preview_response(
            db.conn(),
            Path::new("."),
            "project-1",
            &metadata,
            &posts,
            "/tag/rust/page/2",
        )
        .unwrap();
        assert_eq!(tag_second.status_code, 200);
        assert!(tag_second.html.contains("Alpha body"));
        assert!(!tag_second.html.contains("Beta body"));

        let year_archive = build_preview_response(
            db.conn(),
            Path::new("."),
            "project-1",
            &metadata,
            &posts,
            "/2024",
        )
        .unwrap();
        assert_eq!(year_archive.status_code, 200);
        assert!(year_archive.html.contains("Gamma body"));
        assert!(!year_archive.html.contains("Beta body"));

        let year_archive_page_two = build_preview_response(
            db.conn(),
            Path::new("."),
            "project-1",
            &metadata,
            &posts,
            "/2024/page/2",
        )
        .unwrap();
        assert_eq!(year_archive_page_two.status_code, 200);
        assert!(year_archive_page_two.html.contains("Beta body"));

        let month_archive = build_preview_response(
            db.conn(),
            Path::new("."),
            "project-1",
            &metadata,
            &posts,
            "/2024/03",
        )
        .unwrap();
        assert_eq!(month_archive.status_code, 200);
        assert!(month_archive.html.contains("Beta body"));
        assert!(!month_archive.html.contains("Alpha body"));
        assert!(!month_archive.html.contains("Gamma body"));

        let month_archive_page_two = build_preview_response(
            db.conn(),
            Path::new("."),
            "project-1",
            &metadata,
            &posts,
            "/2024/03/page/2",
        )
        .unwrap();
        assert_eq!(month_archive_page_two.status_code, 200);
        assert!(month_archive_page_two.html.contains("Alpha body"));
        assert!(!month_archive_page_two.html.contains("Beta body"));
    }

    #[test]
    fn preview_returns_not_found_template_for_unknown_routes() {
        let db = Database::open_in_memory().unwrap();
        let response = build_preview_response(
            db.conn(),
            Path::new("."),
            "project-1",
            &make_metadata(),
            &[(make_post().post, make_post().body_markdown)],
            "/missing/route",
        )
        .unwrap();

        assert_eq!(response.status_code, 404);
        assert!(
            response
                .html
                .contains("No rendered page exists for /missing/route")
        );
    }
}
