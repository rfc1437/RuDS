use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::collections::HashMap;

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use tokio::sync::oneshot;

use crate::db::{Database, queries};
use crate::engine::generation::PublishedPostSource;
use crate::engine::{EngineError, EngineResult};
use crate::model::{Post, PostStatus, ProjectMetadata};
use crate::render::{build_canonical_post_path, render_starter_list_page_with_media_map, render_starter_single_post_page_with_media_map};
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
            EngineError::Conflict(format!("preview server already running on {PREVIEW_HOST}:{PREVIEW_PORT}"))
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

pub fn render_preview_path(
    path: &str,
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
    canonical_media_path_by_source_path: &HashMap<String, String>,
) -> EngineResult<Option<String>> {
    let normalized = if path.is_empty() { "/" } else { path };
    let main_language = metadata.main_language.as_deref().unwrap_or("en");

    if normalized == "/" {
        let list_posts = posts
            .iter()
            .map(|source| (source.post.clone(), source.body_markdown.clone()))
            .collect::<Vec<_>>();
        return render_starter_list_page_with_media_map(
            &list_posts,
            metadata,
            main_language,
            canonical_media_path_by_source_path.clone(),
        )
            .map(|page| Some(page.html))
            .map_err(|error| EngineError::Parse(error.to_string()));
    }

    let (language, route_path) = split_language_prefix(normalized, metadata);
    if let Some(source) = posts.iter().find(|source| {
        build_canonical_post_path(&source.post, &language, main_language) == route_path
    }) {
        return render_starter_single_post_page_with_media_map(
            &source.post,
            &source.body_markdown,
            metadata,
            &language,
            canonical_media_path_by_source_path.clone(),
        )
            .map(|page| Some(page.html))
            .map_err(|error| EngineError::Parse(error.to_string()));
    }

    Ok(None)
}

async fn handle_preview_request(
    State(state): State<PreviewServerState>,
    uri: Uri,
) -> Response {
    match render_preview_response(&state, uri.path(), None) {
        Ok(response) => response,
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

async fn handle_draft_preview(
    State(state): State<PreviewServerState>,
    AxumPath(post_id): AxumPath<String>,
    Query(query): Query<DraftPreviewQuery>,
) -> Response {
    match render_preview_response(&state, &format!("/__draft/{post_id}"), query.language.as_deref()) {
        Ok(response) => response,
        Err(error) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string()),
    }
}

fn render_preview_response(
    state: &PreviewServerState,
    path: &str,
    requested_language: Option<&str>,
) -> EngineResult<Response> {
    if let Some(post_id) = path.strip_prefix("/__draft/") {
        let html = render_draft_preview(state, post_id, requested_language)?;
        return Ok(Html(html).into_response());
    }

    if let Some(file_response) = serve_project_file(&state.data_dir, path)? {
        return Ok(file_response);
    }

    let metadata = crate::engine::meta::read_project_json(&state.data_dir)?;
    let db = Database::open(&state.db_path)?;
    let media_rewrite_map = build_media_rewrite_map(db.conn(), &state.project_id)?;
    let published_posts = collect_published_posts(state, &metadata)?;
    match render_preview_path(path, &metadata, &published_posts, &media_rewrite_map)? {
        Some(html) => Ok(Html(html).into_response()),
        None => Ok(error_response(StatusCode::NOT_FOUND, "preview not found")),
    }
}

fn render_draft_preview(
    state: &PreviewServerState,
    post_id: &str,
    requested_language: Option<&str>,
) -> EngineResult<String> {
    let db = Database::open(&state.db_path)?;
    let metadata = crate::engine::meta::read_project_json(&state.data_dir)?;
    let post = queries::post::get_post_by_id(db.conn(), post_id)?;
    let canonical_language = post.language.as_deref().unwrap_or_else(|| metadata.main_language.as_deref().unwrap_or("en"));
    let target_language = requested_language.unwrap_or(canonical_language);

    if target_language != canonical_language {
        if let Ok(translation) = queries::post_translation::get_post_translation_by_post_and_language(
            db.conn(),
            post_id,
            target_language,
        ) {
            let media_rewrite_map = build_media_rewrite_map(db.conn(), &post.project_id)?;
            let mut translated_post = post.clone();
            translated_post.title = translation.title.clone();
            translated_post.excerpt = translation.excerpt.clone();
            translated_post.language = Some(translation.language.clone());
            translated_post.status = translation.status.clone();
            translated_post.file_path = translation.file_path.clone();
            translated_post.published_at = translation.published_at.or(post.published_at);
            let body = load_translation_body(&state.data_dir, &translation)?;
            return render_starter_single_post_page_with_media_map(
                &translated_post,
                &body,
                &metadata,
                target_language,
                media_rewrite_map,
            )
                .map(|page| page.html)
                .map_err(|error| EngineError::Parse(error.to_string()));
        }
    }

    let media_rewrite_map = build_media_rewrite_map(db.conn(), &post.project_id)?;
    let body = load_post_body(&state.data_dir, &post)?;
    render_starter_single_post_page_with_media_map(
        &post,
        &body,
        &metadata,
        canonical_language,
        media_rewrite_map,
    )
        .map(|page| page.html)
        .map_err(|error| EngineError::Parse(error.to_string()))
}

fn build_media_rewrite_map(
    conn: &rusqlite::Connection,
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

fn collect_published_posts(
    state: &PreviewServerState,
    metadata: &ProjectMetadata,
) -> EngineResult<Vec<PublishedPostSource>> {
    let db = Database::open(&state.db_path)?;
    let posts = queries::post::list_posts_by_project(db.conn(), &state.project_id)?;
    let mut published = Vec::new();
    for post in posts.into_iter().filter(|post| matches!(post.status, PostStatus::Published)) {
        published.push(PublishedPostSource {
            body_markdown: load_post_body(&state.data_dir, &post)?,
            post,
        });
    }
    let main_language = metadata.main_language.as_deref().unwrap_or("en");
    published.sort_by_key(|source| build_canonical_post_path(&source.post, main_language, main_language));
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

fn load_markdown_body(data_dir: &Path, relative_path: &str, translation: bool) -> EngineResult<String> {
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
        return Ok(Some(error_response(StatusCode::NOT_FOUND, "preview asset not found")));
    }
    let canonical_candidate = candidate.canonicalize()?;
    let canonical_scope_root = scope_root.canonicalize().unwrap_or(scope_root);
    if !canonical_candidate.starts_with(&canonical_scope_root) {
        return Ok(Some(error_response(StatusCode::NOT_FOUND, "preview asset not found")));
    }
    let bytes = fs::read(&canonical_candidate)?;
    let mime = guess_content_type(&canonical_candidate);
    Ok(Some((
        StatusCode::OK,
        [(header::CONTENT_TYPE, mime)],
        bytes,
    )
        .into_response()))
}

fn guess_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or_default() {
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
    (status, [(header::CONTENT_TYPE, "text/plain; charset=utf-8")], message.to_string()).into_response()
}

fn split_language_prefix(path: &str, metadata: &ProjectMetadata) -> (String, String) {
    let trimmed = path.trim_start_matches('/');
    let mut segments = trimmed.split('/');
    let first = segments.next().unwrap_or_default();
    if metadata.blog_languages.iter().any(|language| language == first) {
        let remainder = segments.collect::<Vec<_>>().join("/");
        return (first.to_string(), format!("/{first}/{}", remainder.trim_start_matches('/')));
    }

    (
        metadata.main_language.as_deref().unwrap_or("en").to_string(),
        path.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::queries;
    use crate::engine::meta;
    use crate::model::{Post, Project, ProjectMetadata, PostStatus};
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
        let mut db = Database::open(&db_path).unwrap();
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
        let html = render_preview_path("/", &make_metadata(), &[make_post()], &HashMap::new())
            .unwrap()
            .unwrap();
        assert!(html.contains("post-list"));
    }

    #[test]
    fn preview_renders_single_post_for_canonical_path() {
        let html = render_preview_path(
            "/2024/03/09/hello",
            &make_metadata(),
            &[make_post()],
            &HashMap::new(),
        )
            .unwrap()
            .unwrap();
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<strong>world</strong>"));
    }

    #[test]
    fn preview_renders_language_prefixed_single_post() {
        let html = render_preview_path(
            "/de/2024/03/09/hello",
            &make_metadata(),
            &[make_post()],
            &HashMap::new(),
        )
            .unwrap()
            .unwrap();
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
            if let Ok(response) = client.get(format!("http://{PREVIEW_HOST}:{PREVIEW_PORT}/__draft/post-1")).send() {
                if response.status().is_success() {
                    body = Some(response.text().unwrap());
                    break;
                }
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
            .get(format!("http://{PREVIEW_HOST}:{PREVIEW_PORT}/media/../outside.txt"))
            .send()
            .unwrap();
        server.stop().unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}