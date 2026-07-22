use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use chrono::{SecondsFormat, TimeZone, Utc};
use serde_json::{Map, Value, json};

use crate::db::queries::{media, project, script, tag, template};
use crate::db::{Database, DbConnection};
use crate::engine::task::{TaskId, TaskManager, TaskSnapshot, TaskStatus};
use crate::engine::{self, EngineError};
use crate::model::{Media, Post, Project, Script, Tag, Template};

use super::HostApi;

pub type AppHostHandler = dyn Fn(&str, &[Value]) -> Result<Value, String> + Send + Sync + 'static;

#[derive(Debug, thiserror::Error)]
enum HostError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Engine(#[from] EngineError),
    #[error(transparent)]
    Query(#[from] diesel::result::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<String> for HostError {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

impl From<&str> for HostError {
    fn from(value: &str) -> Self {
        Self::Message(value.to_owned())
    }
}

type HostResult<T> = Result<T, HostError>;

/// Core engine-backed host bound to one active project.
pub struct CoreHost {
    db_path: PathBuf,
    project_id: String,
    data_dir: PathBuf,
    private_cache_dir: PathBuf,
    task_manager: Option<Arc<TaskManager>>,
    task_id: Option<TaskId>,
    offline_mode: bool,
    app_handler: Option<Arc<AppHostHandler>>,
}

impl CoreHost {
    pub fn new(
        db_path: impl Into<PathBuf>,
        project_id: impl Into<String>,
        data_dir: impl Into<PathBuf>,
    ) -> Self {
        let data_dir = data_dir.into();
        Self {
            db_path: db_path.into(),
            project_id: project_id.into(),
            private_cache_dir: data_dir.join(".cache"),
            data_dir,
            task_manager: None,
            task_id: None,
            offline_mode: false,
            app_handler: None,
        }
    }

    pub(crate) fn from_connection(
        conn: &DbConnection,
        project_id: impl Into<String>,
        data_dir: impl Into<PathBuf>,
    ) -> Result<Self, String> {
        conn.database_path()
            .map(|path| Self::new(path, project_id, data_dir))
            .map_err(|error| error.to_string())
    }

    pub fn with_task(mut self, manager: Arc<TaskManager>, task_id: TaskId) -> Self {
        self.task_manager = Some(manager);
        self.task_id = Some(task_id);
        self
    }

    /// Attach the application-wide task service for remote and alternate UI
    /// sessions that are not themselves running inside one managed task.
    pub fn with_task_manager(mut self, manager: Arc<TaskManager>) -> Self {
        self.task_manager = Some(manager);
        self
    }

    pub fn with_offline_mode(mut self, offline_mode: bool) -> Self {
        self.offline_mode = offline_mode;
        self
    }

    pub fn with_app_handler(mut self, handler: Arc<AppHostHandler>) -> Self {
        self.app_handler = Some(handler);
        self
    }

    fn database(&self) -> HostResult<Database> {
        // ponytail: one short-lived SQLite connection per call; pool only if script throughput matters.
        Database::open(&self.db_path).map_err(|error| HostError::Message(error.to_string()))
    }

    fn scoped<T>(
        &self,
        load: impl FnOnce(&DbConnection) -> Result<T, diesel::result::Error>,
        project_of: impl FnOnce(&T) -> &str,
    ) -> HostResult<T> {
        let db = self.database()?;
        let value = load(db.conn()).map_err(|error| error.to_string())?;
        if project_of(&value) != self.project_id {
            return Err("record is outside the active project".into());
        }
        Ok(value)
    }

    fn app(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        match method {
            "get_data_paths" => Ok(json!({
                "database": self.db_path,
                "project": self.data_dir,
            })),
            "get_blogmark_bookmarklet" => {
                Ok(engine::blogmark::bookmarklet(&self.project_id).into())
            }
            "get_system_language" => Ok(crate::i18n::detect_os_locale().code().into()),
            "get_default_project_path" => Ok(self.data_dir.to_string_lossy().into_owned().into()),
            "read_project_metadata" => {
                let path = string_arg(args, 0)?;
                json_value(engine::meta::read_project_json(Path::new(path)))
            }
            method @ ("copy_to_clipboard"
            | "notify_renderer_ready"
            | "open_folder"
            | "select_folder"
            | "set_preview_post_target"
            | "show_item_in_folder"
            | "trigger_menu_action"
            | "get_title_bar_metrics") => self
                .app_handler
                .as_ref()
                .ok_or_else(|| HostError::from("desktop shell capability is unavailable"))?(
                method, args,
            )
            .map_err(HostError::from),
            _ => Err(format!("unknown app capability: {method}").into()),
        }
    }

    fn projects(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        let db = self.database()?;
        match method {
            "create" => {
                let data = object_arg(args, 0)?;
                public_project(engine::project::create_project(
                    db.conn(),
                    string_field(data, "name")?,
                    optional_string_field(data, "data_path"),
                )?)
            }
            "delete" | "delete_with_data" => {
                let id = string_arg(args, 0)?;
                let existing = project::get_project_by_id(db.conn(), id)?;
                let path = existing.data_path.as_deref().map(Path::new);
                engine::project::delete_project(
                    db.conn(),
                    id,
                    (method == "delete_with_data").then_some(path).flatten(),
                )?;
                Ok(Value::Bool(true))
            }
            "get" => public_project(project::get_project_by_id(db.conn(), string_arg(args, 0)?)?),
            "get_all" => public_list(project::list_projects(db.conn())?, public_project),
            "get_active" => engine::project::get_active_project(db.conn())?
                .map(public_project)
                .transpose()
                .map(|value| value.unwrap_or(Value::Null)),
            "set_active" => {
                let id = string_arg(args, 0)?;
                engine::project::set_active_project(db.conn(), id)?;
                public_project(project::get_project_by_id(db.conn(), id)?)
            }
            "update" => {
                let id = string_arg(args, 0)?;
                let data = object_arg(args, 1)?;
                let mut value = project::get_project_by_id(db.conn(), id)?;
                assign_string(data, "name", &mut value.name);
                assign_optional_string(data, "description", &mut value.description);
                value.updated_at = crate::util::now_unix_ms();
                project::update_project(db.conn(), &value)?;
                public_project(value)
            }
            _ => Err(format!("unknown projects capability: {method}").into()),
        }
    }

    fn meta(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        match method {
            "get_project_metadata" => public_metadata(&self.data_dir),
            "update_project_metadata" | "set_project_metadata" => {
                let db = self.database()?;
                let mut metadata = engine::meta::read_project_json(&self.data_dir)?;
                let updates = object_arg(args, 0)?;
                assign_string(updates, "name", &mut metadata.name);
                assign_optional_string(updates, "description", &mut metadata.description);
                assign_optional_string(updates, "public_url", &mut metadata.public_url);
                assign_optional_string(updates, "main_language", &mut metadata.main_language);
                assign_optional_string(updates, "default_author", &mut metadata.default_author);
                if let Some(languages) = string_list(updates, "blog_languages") {
                    metadata.blog_languages = languages;
                }
                metadata.validate().map_err(text)?;
                let project = project::get_project_by_id(db.conn(), &self.project_id)?;
                engine::meta::update_project_metadata(
                    db.conn(),
                    &self.data_dir,
                    &project,
                    &metadata,
                )?;
                public_metadata(&self.data_dir)
            }
            "add_category" => {
                let db = self.database()?;
                engine::meta::add_category(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                    string_arg(args, 0)?,
                )?;
                public_metadata(&self.data_dir)
            }
            "remove_category" => {
                let db = self.database()?;
                engine::meta::remove_category(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                    string_arg(args, 0)?,
                )?;
                public_metadata(&self.data_dir)
            }
            "add_tag" => self.meta_tag(string_arg(args, 0)?, true),
            "remove_tag" => self.meta_tag(string_arg(args, 0)?, false),
            "get_categories" => json_value(engine::meta::read_categories_json(&self.data_dir)),
            "get_tags" => Ok(json!(
                engine::meta::read_tags_json(&self.data_dir)?
                    .into_iter()
                    .map(|tag| tag.name)
                    .collect::<Vec<_>>()
            )),
            "get_publishing_preferences" => {
                json_value(engine::meta::read_publishing_json(&self.data_dir))
            }
            "set_publishing_preferences" => {
                let db = self.database()?;
                let prefs = serde_json::from_value(Value::Object(object_arg(args, 0)?.clone()))
                    .map_err(text)?;
                engine::meta::set_publishing_preferences(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                    &prefs,
                )?;
                json_value(engine::meta::read_publishing_json(&self.data_dir))
            }
            "clear_publishing_preferences" => {
                let db = self.database()?;
                let prefs = Default::default();
                engine::meta::set_publishing_preferences(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                    &prefs,
                )?;
                json_value(Ok::<_, EngineError>(prefs))
            }
            "sync_on_startup" => {
                let db = self.database()?;
                engine::meta::startup_sync(&self.data_dir)?;
                engine::meta::initialize_metadata_snapshots(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                )?;
                Ok(json!({
                    "metadata": public_metadata(&self.data_dir)?,
                    "categories": engine::meta::read_categories_json(&self.data_dir)?,
                    "tags": engine::meta::read_tags_json(&self.data_dir)?,
                }))
            }
            _ => Err(format!("unknown meta capability: {method}").into()),
        }
    }

    fn meta_tag(&self, name: &str, add: bool) -> HostResult<Value> {
        let mut tags = engine::meta::read_tags_json(&self.data_dir)?;
        if add {
            if !tags.iter().any(|tag| tag.name.eq_ignore_ascii_case(name)) {
                tags.push(crate::model::metadata::TagEntry {
                    name: name.to_owned(),
                    color: None,
                    post_template_slug: None,
                });
            }
        } else {
            tags.retain(|tag| !tag.name.eq_ignore_ascii_case(name));
        }
        engine::meta::write_tags_json(&self.data_dir, &tags)?;
        Ok(json!(
            tags.into_iter().map(|tag| tag.name).collect::<Vec<_>>()
        ))
    }

    fn posts(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        let db = self.database()?;
        match method {
            "create" => {
                let data = object_arg(args, 0)?;
                public_post(
                    db.conn(),
                    engine::post::create_post(
                        db.conn(),
                        &self.data_dir,
                        &self.project_id,
                        string_field(data, "title")?,
                        optional_string_field(data, "content"),
                        string_list(data, "tags").unwrap_or_default(),
                        string_list(data, "categories").unwrap_or_default(),
                        optional_string_field(data, "author"),
                        optional_string_field(data, "language"),
                        optional_string_field(data, "template_slug"),
                    )?,
                )
            }
            "update" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| crate::db::queries::post::get_post_by_id(conn, id),
                    |post| &post.project_id,
                )?;
                let data = object_arg(args, 1)?;
                public_post(
                    db.conn(),
                    engine::post::update_post(
                        db.conn(),
                        &self.data_dir,
                        id,
                        optional_string_field(data, "title"),
                        optional_string_field(data, "slug"),
                        optional_nullable_string(data, "excerpt"),
                        optional_string_field(data, "content"),
                        string_list(data, "tags"),
                        string_list(data, "categories"),
                        optional_nullable_string(data, "author"),
                        optional_nullable_string(data, "language"),
                        optional_nullable_string(data, "template_slug"),
                        data.get("do_not_translate").and_then(Value::as_bool),
                    )?,
                )
            }
            "delete" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| crate::db::queries::post::get_post_by_id(conn, id),
                    |post| &post.project_id,
                )?;
                engine::post::delete_post(db.conn(), &self.data_dir, id)?;
                Ok(Value::Bool(true))
            }
            "discard" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| crate::db::queries::post::get_post_by_id(conn, id),
                    |post| &post.project_id,
                )?;
                public_post(
                    db.conn(),
                    engine::post::discard_post_draft(db.conn(), &self.data_dir, id)?,
                )
            }
            "get" => {
                let id = string_arg(args, 0)?;
                public_post(
                    db.conn(),
                    self.scoped(
                        |conn| crate::db::queries::post::get_post_by_id(conn, id),
                        |post| &post.project_id,
                    )?,
                )
            }
            "get_all" => public_posts(
                db.conn(),
                crate::db::queries::post::list_posts_by_project(db.conn(), &self.project_id)?,
            ),
            "get_by_slug" => public_post(
                db.conn(),
                crate::db::queries::post::get_post_by_project_and_slug(
                    db.conn(),
                    &self.project_id,
                    string_arg(args, 0)?,
                )?,
            ),
            "get_by_status" => {
                let filters = crate::db::queries::post::PostFilterParams {
                    status: Some(string_arg(args, 0)?.to_owned()),
                    ..Default::default()
                };
                public_posts(
                    db.conn(),
                    crate::db::queries::post::list_posts_filtered(
                        db.conn(),
                        &self.project_id,
                        &filters,
                        i64::MAX,
                        0,
                    )?,
                )
            }
            "filter" => {
                let filters = post_filters(object_arg(args, 0)?);
                public_posts(
                    db.conn(),
                    crate::db::queries::post::list_posts_filtered(
                        db.conn(),
                        &self.project_id,
                        &filters,
                        i64::MAX,
                        0,
                    )?,
                )
            }
            "search" => {
                let ids = crate::db::fts::search_posts(
                    db.conn(),
                    string_arg(args, 0)?,
                    &self.main_language(),
                )?;
                let posts = ids
                    .into_iter()
                    .filter_map(|id| crate::db::queries::post::get_post_by_id(db.conn(), &id).ok())
                    .filter(|post| post.project_id == self.project_id)
                    .collect();
                public_posts(db.conn(), posts)
            }
            "generate_unique_slug" => {
                let title = string_arg(args, 0)?;
                let exclude = args.get(1).and_then(Value::as_str);
                let base = crate::util::slugify(title);
                Ok(crate::util::ensure_unique(&base, |candidate| {
                    crate::db::queries::post::get_post_by_project_and_slug(
                        db.conn(),
                        &self.project_id,
                        candidate,
                    )
                    .is_ok_and(|post| Some(post.id.as_str()) != exclude)
                })
                .into())
            }
            "get_by_year_month" => Ok(json!(
                crate::db::queries::post::post_calendar_counts(
                    db.conn(),
                    &self.project_id,
                    false,
                    false
                )?
                .into_iter()
                .map(|(year, month, count)| json!({"year":year,"month":month,"count":count}))
                .collect::<Vec<_>>()
            )),
            "get_dashboard_stats" => {
                let posts =
                    crate::db::queries::post::list_posts_by_project(db.conn(), &self.project_id)?;
                Ok(json!({
                    "total": posts.len(),
                    "draft": posts.iter().filter(|post| post.status.as_str() == "draft").count(),
                    "published": posts.iter().filter(|post| post.status.as_str() == "published").count(),
                    "archived": posts.iter().filter(|post| post.status.as_str() == "archived").count(),
                }))
            }
            "get_linked_by" => {
                linked_posts(db.conn(), &self.project_id, string_arg(args, 0)?, true)
            }
            "get_links_to" => {
                linked_posts(db.conn(), &self.project_id, string_arg(args, 0)?, false)
            }
            "get_preview_url" => {
                let post = self.scoped(
                    |conn| {
                        crate::db::queries::post::get_post_by_id(
                            conn,
                            string_arg(args, 0).unwrap_or(""),
                        )
                    },
                    |post| &post.project_id,
                )?;
                Ok(if post.status.as_str() == "published" {
                    engine::post::canonical_url(post.created_at, &post.slug).into()
                } else {
                    format!(
                        "http://{}:{}/__draft/{}",
                        engine::preview::PREVIEW_HOST,
                        engine::preview::PREVIEW_PORT,
                        post.id
                    )
                    .into()
                })
            }
            "get_categories" => json_value(Ok::<_, EngineError>(
                crate::db::queries::post::distinct_post_categories(db.conn(), &self.project_id)?,
            )),
            "get_categories_with_counts" => name_counts(
                crate::db::queries::post::list_posts_by_project(db.conn(), &self.project_id)?
                    .into_iter()
                    .flat_map(|post| post.categories),
            ),
            "get_tags" => json_value(Ok::<_, EngineError>(
                crate::db::queries::post::distinct_post_tags(db.conn(), &self.project_id)?,
            )),
            "get_tags_with_counts" => name_counts(
                crate::db::queries::post::list_posts_by_project(db.conn(), &self.project_id)?
                    .into_iter()
                    .flat_map(|post| post.tags),
            ),
            "get_translation" => {
                let post_id = string_arg(args, 0)?;
                self.scoped(
                    |conn| crate::db::queries::post::get_post_by_id(conn, post_id),
                    |post| &post.project_id,
                )?;
                sanitize_timestamps(json_value(Ok::<_, EngineError>(
                    crate::db::queries::post_translation::get_post_translation_by_post_and_language(
                        db.conn(), post_id, string_arg(args, 1)?,
                    )?,
                )))
            }
            "get_translations" => {
                let post_id = string_arg(args, 0)?;
                self.scoped(
                    |conn| crate::db::queries::post::get_post_by_id(conn, post_id),
                    |post| &post.project_id,
                )?;
                sanitize_timestamps(json_value(Ok::<_, EngineError>(
                    crate::db::queries::post_translation::list_post_translations_by_post(
                        db.conn(),
                        post_id,
                    )?,
                )))
            }
            "has_published_version" => {
                let post = self.scoped(
                    |conn| {
                        crate::db::queries::post::get_post_by_id(
                            conn,
                            string_arg(args, 0).unwrap_or(""),
                        )
                    },
                    |post| &post.project_id,
                )?;
                Ok(Value::Bool(
                    post.published_at.is_some() || !post.file_path.is_empty(),
                ))
            }
            "is_slug_available" => {
                let slug = string_arg(args, 0)?;
                let exclude = args.get(1).and_then(Value::as_str);
                Ok(Value::Bool(
                    match crate::db::queries::post::get_post_by_project_and_slug(
                        db.conn(),
                        &self.project_id,
                        slug,
                    ) {
                        Ok(post) => Some(post.id.as_str()) == exclude,
                        Err(_) => true,
                    },
                ))
            }
            "publish" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| crate::db::queries::post::get_post_by_id(conn, id),
                    |post| &post.project_id,
                )?;
                public_post(
                    db.conn(),
                    engine::post::publish_post(db.conn(), &self.data_dir, id)?,
                )
            }
            "publish_translation" => {
                self.scoped(
                    |conn| {
                        crate::db::queries::post::get_post_by_id(
                            conn,
                            string_arg(args, 0).unwrap_or(""),
                        )
                    },
                    |post| &post.project_id,
                )?;
                let translation = crate::db::queries::post_translation::get_post_translation_by_post_and_language(db.conn(), string_arg(args, 0)?, string_arg(args, 1)?)?;
                sanitize_timestamps(json_value(Ok::<_, EngineError>(
                    engine::post::publish_post_translation(
                        db.conn(),
                        &self.data_dir,
                        &translation.id,
                    )?,
                )))
            }
            "rebuild_from_files" => {
                engine::post::rebuild_posts_from_filesystem(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                )?;
                public_posts(
                    db.conn(),
                    crate::db::queries::post::list_posts_by_project(db.conn(), &self.project_id)?,
                )
            }
            "rebuild_links" => {
                engine::post::rebuild_all_links(db.conn(), &self.data_dir, &self.project_id)?;
                Ok(Value::Bool(true))
            }
            "reindex_text" => {
                engine::search::reindex_project(db.conn(), &self.project_id, None)?;
                Ok(Value::Bool(true))
            }
            _ => Err(format!("unknown posts capability: {method}").into()),
        }
    }

    fn main_language(&self) -> String {
        engine::meta::read_project_json(&self.data_dir)
            .ok()
            .and_then(|meta| meta.main_language)
            .unwrap_or_else(|| "en".into())
    }

    fn media(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        let db = self.database()?;
        match method {
            "import" => {
                let data = object_arg(args, 0)?;
                let source = string_field(data, "source_path")?;
                let original = optional_string_field(data, "original_name").unwrap_or_else(|| {
                    Path::new(source)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("media")
                });
                public_media(engine::media::import_media(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                    Path::new(source),
                    original,
                    optional_string_field(data, "title"),
                    optional_string_field(data, "alt"),
                    optional_string_field(data, "caption"),
                    optional_string_field(data, "author"),
                    optional_string_field(data, "language"),
                    string_list(data, "tags").unwrap_or_default(),
                )?)
            }
            "update" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| media::get_media_by_id(conn, id),
                    |media| &media.project_id,
                )?;
                let data = object_arg(args, 1)?;
                public_media(engine::media::update_media(
                    db.conn(),
                    &self.data_dir,
                    id,
                    optional_nullable_string(data, "title"),
                    optional_nullable_string(data, "alt"),
                    optional_nullable_string(data, "caption"),
                    optional_nullable_string(data, "author"),
                    optional_nullable_string(data, "language"),
                    string_list(data, "tags"),
                )?)
            }
            "delete" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| media::get_media_by_id(conn, id),
                    |media| &media.project_id,
                )?;
                engine::media::delete_media(db.conn(), &self.data_dir, id)?;
                Ok(Value::Bool(true))
            }
            "get" => public_media(self.scoped(
                |conn| media::get_media_by_id(conn, string_arg(args, 0).unwrap_or("")),
                |media| &media.project_id,
            )?),
            "get_all" => public_list(
                media::list_media_by_project(db.conn(), &self.project_id)?,
                public_media,
            ),
            "filter" => {
                let filters = media_filters(object_arg(args, 0)?);
                public_list(
                    media::list_media_filtered(db.conn(), &self.project_id, &filters, i64::MAX, 0)?,
                    public_media,
                )
            }
            "search" => {
                let ids = crate::db::fts::search_media(
                    db.conn(),
                    string_arg(args, 0)?,
                    &self.main_language(),
                )?;
                public_list(
                    ids.into_iter()
                        .filter_map(|id| media::get_media_by_id(db.conn(), &id).ok())
                        .filter(|item| item.project_id == self.project_id)
                        .collect(),
                    public_media,
                )
            }
            "get_by_year_month" => Ok(json!(
                media::media_calendar_counts(db.conn(), &self.project_id)?
                    .into_iter()
                    .map(|(year, month, count)| json!({"year":year,"month":month,"count":count}))
                    .collect::<Vec<_>>()
            )),
            "get_file_path" => {
                let item = self.scoped(
                    |conn| media::get_media_by_id(conn, string_arg(args, 0).unwrap_or("")),
                    |media| &media.project_id,
                )?;
                Ok(self
                    .data_dir
                    .join(item.file_path)
                    .to_string_lossy()
                    .into_owned()
                    .into())
            }
            "get_tags" => json_value(Ok::<_, EngineError>(media::distinct_media_tags(
                db.conn(),
                &self.project_id,
            )?)),
            "get_tags_with_counts" => name_counts(
                media::list_media_by_project(db.conn(), &self.project_id)?
                    .into_iter()
                    .flat_map(|media| media.tags),
            ),
            "get_thumbnail" => {
                let item = self.scoped(
                    |conn| media::get_media_by_id(conn, string_arg(args, 0).unwrap_or("")),
                    |media| &media.project_id,
                )?;
                let size = args.get(1).and_then(Value::as_str).unwrap_or("small");
                let ext = if size == "ai" { "jpg" } else { "webp" };
                let path = self
                    .data_dir
                    .join(crate::util::thumbnail_path(&item.id, size, ext));
                if !path.is_file() {
                    crate::util::thumbnail::generate_all_thumbnails(
                        &self.data_dir.join(&item.file_path),
                        &self.data_dir.join("thumbnails"),
                        &item.id,
                    )?;
                }
                let mime = if ext == "jpg" {
                    "image/jpeg"
                } else {
                    "image/webp"
                };
                Ok(format!(
                    "data:{mime};base64,{}",
                    base64::engine::general_purpose::STANDARD
                        .encode(std::fs::read(path).map_err(text)?)
                )
                .into())
            }
            "get_translation" => {
                let item = self.scoped(
                    |conn| media::get_media_by_id(conn, string_arg(args, 0).unwrap_or("")),
                    |media| &media.project_id,
                )?;
                sanitize_timestamps(json_value(Ok::<_, EngineError>(crate::db::queries::media_translation::get_media_translation_by_media_and_language(db.conn(), &item.id, string_arg(args, 1)?)?)))
            }
            "get_translations" => {
                let item = self.scoped(
                    |conn| media::get_media_by_id(conn, string_arg(args, 0).unwrap_or("")),
                    |media| &media.project_id,
                )?;
                sanitize_timestamps(json_value(Ok::<_, EngineError>(
                    crate::db::queries::media_translation::list_media_translations_by_media(
                        db.conn(),
                        &item.id,
                    )?,
                )))
            }
            "get_url" => {
                let item = self.scoped(
                    |conn| media::get_media_by_id(conn, string_arg(args, 0).unwrap_or("")),
                    |media| &media.project_id,
                )?;
                Ok(format!("/{}", item.file_path.trim_start_matches('/')).into())
            }
            "delete_translation" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| media::get_media_by_id(conn, id),
                    |media| &media.project_id,
                )?;
                engine::media::delete_media_translation(
                    db.conn(),
                    &self.data_dir,
                    id,
                    string_arg(args, 1)?,
                )?;
                Ok(Value::Bool(true))
            }
            "upsert_translation" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| media::get_media_by_id(conn, id),
                    |media| &media.project_id,
                )?;
                let data = object_arg(args, 2)?;
                sanitize_timestamps(json_value(Ok::<_, EngineError>(
                    engine::media::upsert_media_translation(
                        db.conn(),
                        &self.data_dir,
                        id,
                        string_arg(args, 1)?,
                        optional_string_field(data, "title"),
                        optional_string_field(data, "alt"),
                        optional_string_field(data, "caption"),
                    )?,
                )))
            }
            "replace_file" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| media::get_media_by_id(conn, id),
                    |media| &media.project_id,
                )?;
                engine::media::replace_media_file(
                    db.conn(),
                    &self.data_dir,
                    id,
                    Path::new(string_arg(args, 1)?),
                )?
                .map(public_media)
                .transpose()
                .map(|value| value.unwrap_or(Value::Null))
            }
            "rebuild_from_files" => {
                engine::media::rebuild_media_from_filesystem(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                )?;
                public_list(
                    media::list_media_by_project(db.conn(), &self.project_id)?,
                    public_media,
                )
            }
            "regenerate_thumbnails" => {
                let item = self.scoped(
                    |conn| media::get_media_by_id(conn, string_arg(args, 0).unwrap_or("")),
                    |media| &media.project_id,
                )?;
                crate::util::thumbnail::generate_all_thumbnails(
                    &self.data_dir.join(&item.file_path),
                    &self.data_dir.join("thumbnails"),
                    &item.id,
                )?;
                Ok(json!({"generated": true, "media_id": item.id}))
            }
            "regenerate_missing_thumbnails" => {
                let report = engine::media::regenerate_missing_thumbnails(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                )?;
                Ok(json!({
                    "processed": report.media_processed,
                    "generated": report.thumbnails_generated,
                    "failed": report.media_failed,
                }))
            }
            "reindex_text" => {
                engine::search::reindex_project(db.conn(), &self.project_id, None)?;
                Ok(Value::Bool(true))
            }
            _ => Err(format!("unknown media capability: {method}").into()),
        }
    }

    fn scripts(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        let db = self.database()?;
        match method {
            "create" => {
                let data = object_arg(args, 0)?;
                public_script(engine::script::create_script(
                    db.conn(),
                    &self.project_id,
                    string_field(data, "title")?,
                    optional_string_field(data, "kind")
                        .unwrap_or("utility")
                        .parse()
                        .map_err(text)?,
                    optional_string_field(data, "content").unwrap_or(""),
                    optional_string_field(data, "entrypoint"),
                )?)
            }
            "update" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| script::get_script_by_id(conn, id),
                    |script| &script.project_id,
                )?;
                let data = object_arg(args, 1)?;
                public_script(engine::script::update_script(
                    db.conn(),
                    &self.data_dir,
                    id,
                    &self.project_id,
                    optional_string_field(data, "title"),
                    optional_string_field(data, "slug"),
                    optional_string_field(data, "kind")
                        .map(str::parse)
                        .transpose()
                        .map_err(text)?,
                    optional_string_field(data, "entrypoint"),
                    data.get("enabled").and_then(Value::as_bool),
                    optional_string_field(data, "content"),
                )?)
            }
            "delete" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| script::get_script_by_id(conn, id),
                    |script| &script.project_id,
                )?;
                engine::script::delete_script(db.conn(), &self.data_dir, id)?;
                Ok(Value::Bool(true))
            }
            "get" => public_script(self.scoped(
                |conn| script::get_script_by_id(conn, string_arg(args, 0).unwrap_or("")),
                |script| &script.project_id,
            )?),
            "get_all" => public_list(
                script::list_scripts_by_project(db.conn(), &self.project_id)?,
                public_script,
            ),
            "publish" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| script::get_script_by_id(conn, id),
                    |script| &script.project_id,
                )?;
                public_script(engine::script::publish_script(
                    db.conn(),
                    &self.data_dir,
                    id,
                )?)
            }
            "rebuild_from_files" => {
                engine::script_rebuild::rebuild_scripts_from_filesystem(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                )?;
                public_list(
                    script::list_scripts_by_project(db.conn(), &self.project_id)?,
                    public_script,
                )
            }
            _ => Err(format!("unknown scripts capability: {method}").into()),
        }
    }

    fn templates(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        let db = self.database()?;
        match method {
            "create" => {
                let data = object_arg(args, 0)?;
                public_template(engine::template::create_template(
                    db.conn(),
                    &self.project_id,
                    string_field(data, "title")?,
                    optional_string_field(data, "kind")
                        .unwrap_or("post")
                        .parse()
                        .map_err(text)?,
                    optional_string_field(data, "content").unwrap_or(""),
                )?)
            }
            "update" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| template::get_template_by_id(conn, id),
                    |template| &template.project_id,
                )?;
                let data = object_arg(args, 1)?;
                public_template(engine::template::update_template(
                    db.conn(),
                    &self.data_dir,
                    id,
                    &self.project_id,
                    optional_string_field(data, "title"),
                    optional_string_field(data, "slug"),
                    optional_string_field(data, "kind")
                        .map(str::parse)
                        .transpose()
                        .map_err(text)?,
                    data.get("enabled").and_then(Value::as_bool),
                    optional_string_field(data, "content"),
                )?)
            }
            "delete" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| template::get_template_by_id(conn, id),
                    |template| &template.project_id,
                )?;
                engine::template::delete_template(db.conn(), &self.data_dir, id, false)?;
                Ok(Value::Bool(true))
            }
            "get" => public_template(self.scoped(
                |conn| template::get_template_by_id(conn, string_arg(args, 0).unwrap_or("")),
                |template| &template.project_id,
            )?),
            "get_all" => public_list(
                template::list_templates_by_project(db.conn(), &self.project_id)?,
                public_template,
            ),
            "get_enabled_by_kind" => {
                let kind = string_arg(args, 0)?;
                public_list(
                    template::list_templates_by_project(db.conn(), &self.project_id)?
                        .into_iter()
                        .filter(|value| value.enabled && value.kind.as_str() == kind)
                        .collect(),
                    public_template,
                )
            }
            "publish" => {
                let id = string_arg(args, 0)?;
                self.scoped(
                    |conn| template::get_template_by_id(conn, id),
                    |template| &template.project_id,
                )?;
                public_template(engine::template::publish_template(
                    db.conn(),
                    &self.data_dir,
                    id,
                )?)
            }
            "rebuild_from_files" => {
                engine::template_rebuild::rebuild_templates_from_filesystem(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                )?;
                public_list(
                    template::list_templates_by_project(db.conn(), &self.project_id)?,
                    public_template,
                )
            }
            "validate" => Ok(
                match engine::template::validate_template(string_arg(args, 0)?) {
                    Ok(()) => json!({"valid":true,"errors":[]}),
                    Err(error) => json!({"valid":false,"errors":[error]}),
                },
            ),
            _ => Err(format!("unknown templates capability: {method}").into()),
        }
    }

    fn tags(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        let db = self.database()?;
        match method {
            "create" => {
                let data = object_arg(args, 0)?;
                public_tag(engine::tag::create_tag(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                    string_field(data, "name")?,
                    optional_string_field(data, "color"),
                )?)
            }
            "update" => {
                let id = string_arg(args, 0)?;
                self.scoped(|conn| tag::get_tag_by_id(conn, id), |tag| &tag.project_id)?;
                let data = object_arg(args, 1)?;
                engine::tag::update_tag(
                    db.conn(),
                    &self.data_dir,
                    id,
                    optional_string_field(data, "name"),
                    optional_string_field(data, "color"),
                    optional_string_field(data, "post_template_slug"),
                )?;
                public_tag(tag::get_tag_by_id(db.conn(), id)?)
            }
            "delete" => {
                let id = string_arg(args, 0)?;
                self.scoped(|conn| tag::get_tag_by_id(conn, id), |tag| &tag.project_id)?;
                engine::tag::delete_tag(db.conn(), &self.data_dir, &self.project_id, id)?;
                Ok(Value::Bool(true))
            }
            "get" => public_tag(self.scoped(
                |conn| tag::get_tag_by_id(conn, string_arg(args, 0).unwrap_or("")),
                |tag| &tag.project_id,
            )?),
            "get_all" => public_list(
                tag::list_tags_by_project(db.conn(), &self.project_id)?,
                public_tag,
            ),
            "get_by_name" => public_tag(tag::get_tag_by_project_and_name(
                db.conn(),
                &self.project_id,
                string_arg(args, 0)?,
            )?),
            "get_posts_with_tag" => {
                let value = self.scoped(
                    |conn| tag::get_tag_by_id(conn, string_arg(args, 0).unwrap_or("")),
                    |tag| &tag.project_id,
                )?;
                Ok(json!(
                    crate::db::queries::post::list_posts_by_project(db.conn(), &self.project_id)?
                        .into_iter()
                        .filter(|post| post
                            .tags
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(&value.name)))
                        .map(|post| post.id)
                        .collect::<Vec<_>>()
                ))
            }
            "get_with_counts" => {
                let posts =
                    crate::db::queries::post::list_posts_by_project(db.conn(), &self.project_id)?;
                let counts = posts.into_iter().flat_map(|post| post.tags).fold(
                    std::collections::BTreeMap::<String, usize>::new(),
                    |mut counts, name| {
                        *counts.entry(name.to_lowercase()).or_default() += 1;
                        counts
                    },
                );
                Ok(Value::Array(
                    tag::list_tags_by_project(db.conn(), &self.project_id)?
                        .into_iter()
                        .map(|tag| {
                            let count = counts
                                .get(&tag.name.to_lowercase())
                                .copied()
                                .unwrap_or_default();
                            let mut value = public_tag(tag).unwrap();
                            value["count"] = count.into();
                            value
                        })
                        .collect(),
                ))
            }
            "merge" => {
                let sources = args
                    .first()
                    .and_then(Value::as_array)
                    .ok_or("source_tag_ids must be a table")?
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>();
                engine::tag::merge_tags(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                    &sources,
                    string_arg(args, 1)?,
                )?;
                Ok(Value::Bool(true))
            }
            "rename" => {
                let id = string_arg(args, 0)?;
                self.scoped(|conn| tag::get_tag_by_id(conn, id), |tag| &tag.project_id)?;
                engine::tag::rename_tag(
                    db.conn(),
                    &self.data_dir,
                    &self.project_id,
                    id,
                    string_arg(args, 1)?,
                )?;
                public_tag(tag::get_tag_by_id(db.conn(), id)?)
            }
            "sync_from_posts" => public_list(
                engine::tag::sync_tags_from_posts(db.conn(), &self.project_id)?,
                public_tag,
            ),
            _ => Err(format!("unknown tags capability: {method}").into()),
        }
    }

    fn tasks(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        let manager = self
            .task_manager
            .as_ref()
            .ok_or("task manager is unavailable")?;
        match method {
            "get" => {
                let id = task_id_arg(args, 0)?;
                manager
                    .snapshots()
                    .into_iter()
                    .find(|task| task.id == id)
                    .map(public_task)
                    .transpose()
                    .map(|value| value.unwrap_or(Value::Null))
            }
            "get_all" => public_tasks(manager.snapshots()),
            "get_running" => public_tasks(
                manager
                    .snapshots()
                    .into_iter()
                    .filter(|task| task.status == TaskStatus::Running)
                    .collect(),
            ),
            "status_snapshot" => {
                let tasks = manager.snapshots();
                Ok(json!({
                    "active_count": tasks.iter().filter(|task| matches!(task.status, TaskStatus::Pending | TaskStatus::Running)).count(),
                    "running_count": manager.running_count(), "pending_count": manager.pending_count(),
                    "tasks": public_tasks(tasks)?,
                }))
            }
            "cancel" => {
                manager.cancel(task_id_arg(args, 0)?);
                Ok(Value::Bool(true))
            }
            "clear_completed" => {
                manager.clear_completed();
                Ok(Value::Bool(true))
            }
            _ => Err(format!("unknown tasks capability: {method}").into()),
        }
    }

    fn report_progress(&self, args: &[Value]) -> HostResult<Value> {
        let manager = self
            .task_manager
            .as_ref()
            .ok_or("task manager is unavailable")?;
        let task_id = self.task_id.ok_or("current task is unavailable")?;
        let payload = object_arg(args, 0)?;
        let progress = payload
            .get("progress")
            .and_then(Value::as_f64)
            .map(|value| value as f32)
            .or_else(|| {
                payload
                    .get("current")
                    .and_then(Value::as_f64)
                    .zip(payload.get("total").and_then(Value::as_f64))
                    .and_then(|(current, total)| (total > 0.0).then_some((current / total) as f32))
            });
        let message = optional_string_field(payload, "message").map(str::to_owned);
        manager.report_progress(task_id, progress, message);
        Ok(Value::Bool(true))
    }

    fn sync(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        if method == "check_availability" {
            return Ok(std::process::Command::new("git")
                .arg("--version")
                .output()
                .is_ok()
                .into());
        }
        if self.offline_mode && matches!(method, "fetch" | "pull" | "push") {
            return Err(format!("Git {method} is unavailable in airplane mode").into());
        }
        let git = engine::git::GitEngine::new(&self.data_dir);
        let cancelled = || {
            self.task_manager
                .as_ref()
                .zip(self.task_id)
                .is_some_and(|(manager, id)| manager.is_cancelled(id))
        };
        match method {
            "get_repo_state" | "get_remote_state" => {
                Ok(public_git_repository(git.repository().map_err(text)?))
            }
            "get_status" => Ok(json!({
                "files": git.status().map_err(text)?.into_iter().map(public_git_status).collect::<Vec<_>>(),
            })),
            "get_history" => {
                let repository = git.repository().map_err(text)?;
                let commits = match repository.current_branch {
                    Some(branch) => git.history(&branch).map_err(text)?,
                    None => Vec::new(),
                };
                Ok(json!({
                    "commits": commits.into_iter().map(public_git_commit).collect::<Vec<_>>(),
                }))
            }
            "fetch" => {
                let result = git.fetch(cancelled, |_| {}).map_err(text)?;
                Ok(json!({"updated": true, "output": result.output}))
            }
            "pull" => {
                let result = git.pull(cancelled, |_| {}).map_err(text)?;
                let db = self.database()?;
                engine::rebuild::rebuild_incremental(db.conn(), &self.data_dir, &self.project_id)?;
                Ok(json!({"updated": true, "output": result.output}))
            }
            "push" => {
                let result = git.push(cancelled, |_| {}).map_err(text)?;
                Ok(json!({"updated": true, "output": result.output}))
            }
            "commit_all" => {
                let message = string_arg(args, 0)?;
                let result = git.commit_all(message).map_err(text)?;
                Ok(json!({"message": message, "output": result.output}))
            }
            _ => Err(format!("unknown sync capability: {method}").into()),
        }
    }

    fn publish(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        if method != "upload_site" {
            return Err(format!("unknown publish capability: {method}").into());
        }
        let mut preferences = engine::meta::read_publishing_json(&self.data_dir)?;
        let credentials = object_arg(args, 0)?;
        if let Some(value) = optional_string_field(credentials, "ssh_host") {
            preferences.ssh_host = Some(value.to_owned());
        }
        if let Some(value) = optional_string_field(credentials, "ssh_user") {
            preferences.ssh_user = Some(value.to_owned());
        }
        if let Some(value) = optional_string_field(credentials, "ssh_remote_path") {
            preferences.ssh_remote_path = Some(value.to_owned());
        }
        if let Some(value) = optional_string_field(credentials, "ssh_mode") {
            preferences.ssh_mode = match value {
                "rsync" => crate::model::SshMode::Rsync,
                "scp" => crate::model::SshMode::Scp,
                _ => return Err("ssh_mode must be 'scp' or 'rsync'".into()),
            };
        }
        let manager = self.task_manager.clone();
        let task_id = self.task_id;
        engine::publishing::upload_site(
            &self.data_dir,
            &self.private_cache_dir,
            &preferences,
            move |current, total, _| {
                if let (Some(manager), Some(task_id)) = (&manager, task_id) {
                    manager.report_progress(
                        task_id,
                        Some(current as f32 / total.max(1) as f32),
                        Some("uploading site".into()),
                    );
                }
            },
        )?;
        match (&self.task_manager, self.task_id) {
            (Some(manager), Some(id)) => manager
                .snapshots()
                .into_iter()
                .find(|task| task.id == id)
                .map(public_task)
                .transpose()
                .map(|value| value.unwrap_or(Value::Null)),
            _ => Ok(json!({"status":"completed"})),
        }
    }

    fn chat(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        let db = self.database()?;
        let request = match method {
            "detect_post_language" => engine::ai::OneShotRequest {
                operation: engine::ai::OneShotOperation::DetectLanguage,
                content: json!({"title": string_arg(args, 0)?, "content": string_arg(args, 1)?}),
            },
            "detect_media_language" => engine::ai::OneShotRequest {
                operation: engine::ai::OneShotOperation::DetectLanguage,
                content: json!({"title": string_arg(args, 0)?, "alt": args.get(1), "caption": args.get(2)}),
            },
            "analyze_post" => {
                let post = self.scoped(
                    |conn| {
                        crate::db::queries::post::get_post_by_id(
                            conn,
                            string_arg(args, 0).unwrap_or(""),
                        )
                    },
                    |post| &post.project_id,
                )?;
                engine::ai::OneShotRequest {
                    operation: engine::ai::OneShotOperation::AnalyzePost,
                    content: post_ai_content(&self.data_dir, &post)?,
                }
            }
            "translate_post" => {
                let post = self.scoped(
                    |conn| {
                        crate::db::queries::post::get_post_by_id(
                            conn,
                            string_arg(args, 0).unwrap_or(""),
                        )
                    },
                    |post| &post.project_id,
                )?;
                engine::ai::OneShotRequest {
                    operation: engine::ai::OneShotOperation::TranslatePost {
                        target_language: string_arg(args, 1)?.into(),
                    },
                    content: post_ai_content(&self.data_dir, &post)?,
                }
            }
            "analyze_media_image" => {
                let media = self.scoped(
                    |conn| media::get_media_by_id(conn, string_arg(args, 0).unwrap_or("")),
                    |media| &media.project_id,
                )?;
                let bytes = std::fs::read(self.data_dir.join(&media.file_path)).map_err(text)?;
                engine::ai::OneShotRequest {
                    operation: engine::ai::OneShotOperation::AnalyzeImage,
                    content: json!({"media_id":media.id,"title":media.title,"alt":media.alt,"caption":media.caption,
                        "image_data_url":format!("data:{};base64,{}", media.mime_type, base64::engine::general_purpose::STANDARD.encode(bytes))}),
                }
            }
            "translate_media_metadata" => {
                let media = self.scoped(
                    |conn| media::get_media_by_id(conn, string_arg(args, 0).unwrap_or("")),
                    |media| &media.project_id,
                )?;
                engine::ai::OneShotRequest {
                    operation: engine::ai::OneShotOperation::TranslateMedia {
                        target_language: string_arg(args, 1)?.into(),
                    },
                    content: json!({"media_id":media.id,"title":media.title,"alt":media.alt,"caption":media.caption}),
                }
            }
            _ => return Err(format!("unknown chat capability: {method}").into()),
        };
        let (response, _usage) = engine::ai::run_one_shot(db.conn(), self.offline_mode, &request)?;
        match (method, response) {
            ("translate_post", engine::ai::OneShotResponse::Translation(value)) => {
                sanitize_timestamps(json_value(Ok::<_, EngineError>(
                    engine::post::upsert_translation(
                        db.conn(),
                        &self.data_dir,
                        string_arg(args, 0)?,
                        string_arg(args, 1)?,
                        &value.title,
                        Some(&value.excerpt),
                        Some(&value.content),
                    )?,
                )))
            }
            ("translate_media_metadata", engine::ai::OneShotResponse::MediaTranslation(value)) => {
                sanitize_timestamps(json_value(Ok::<_, EngineError>(
                    engine::media::upsert_media_translation(
                        db.conn(),
                        &self.data_dir,
                        string_arg(args, 0)?,
                        string_arg(args, 1)?,
                        Some(&value.title),
                        Some(&value.alt),
                        Some(&value.caption),
                    )?,
                )))
            }
            (_, response) => one_shot_json(response),
        }
    }

    fn embeddings(&self, method: &str, args: &[Value]) -> HostResult<Value> {
        let db = self.database()?;
        let service = engine::embedding::EmbeddingService::production(db.conn(), &self.data_dir);
        match method {
            "get_progress" => {
                let (indexed, total) = service.indexing_progress(&self.project_id)?;
                Ok(json!({"indexed": indexed, "total": total}))
            }
            "find_similar" => {
                let post_id = string_arg(args, 0)?;
                self.scoped(
                    |conn| crate::db::queries::post::get_post_by_id(conn, post_id),
                    |post| post.project_id.as_str(),
                )?;
                let limit = args.get(1).and_then(Value::as_u64).unwrap_or(5) as usize;
                Ok(Value::Array(
                    service
                        .find_similar(post_id, limit)?
                        .into_iter()
                        .map(|post| {
                            json!({
                                "post_id": post.post_id,
                                "title": post.title,
                                "score": post.similarity,
                            })
                        })
                        .collect(),
                ))
            }
            "compute_similarities" => {
                let post_id = string_arg(args, 0)?;
                self.scoped(
                    |conn| crate::db::queries::post::get_post_by_id(conn, post_id),
                    |post| post.project_id.as_str(),
                )?;
                let target_ids = string_array_arg(args, 1)?;
                json_value(service.compute_similarities(post_id, &target_ids))
            }
            "suggest_tags" => {
                let post_id = string_arg(args, 0)?;
                self.scoped(
                    |conn| crate::db::queries::post::get_post_by_id(conn, post_id),
                    |post| post.project_id.as_str(),
                )?;
                json_value(service.suggest_tags(post_id))
            }
            "find_duplicates" => {
                let mut page = 0;
                let pairs = loop {
                    let result = service.find_duplicates(&self.project_id, page)?;
                    if !result.has_more {
                        break result.pairs;
                    }
                    page += 1;
                };
                Ok(Value::Array(
                    pairs
                        .into_iter()
                        .map(|pair| {
                            json!({
                                "post_id_a": pair.post_id_a,
                                "title_a": pair.title_a,
                                "post_id_b": pair.post_id_b,
                                "title_b": pair.title_b,
                                "score": pair.similarity,
                                "similarity": pair.similarity,
                                "exact_match": pair.exact_match,
                            })
                        })
                        .collect(),
                ))
            }
            "dismiss_pair" => {
                let post_id_a = string_arg(args, 0)?;
                let post_id_b = string_arg(args, 1)?;
                for post_id in [post_id_a, post_id_b] {
                    self.scoped(
                        |conn| crate::db::queries::post::get_post_by_id(conn, post_id),
                        |post| post.project_id.as_str(),
                    )?;
                }
                service.dismiss_duplicate_pair(post_id_a, post_id_b)?;
                Ok(Value::Bool(true))
            }
            "index_unindexed_posts" => json_value(service.index_unindexed(&self.project_id)),
            _ => Err(format!("unknown embeddings capability: {method}").into()),
        }
    }
}

impl HostApi for CoreHost {
    fn call(&self, namespace: &str, method: &str, arguments: Vec<Value>) -> Result<Value, String> {
        let result: HostResult<Value> = match namespace {
            "app" => self.app(method, &arguments),
            "projects" => self.projects(method, &arguments),
            "meta" => self.meta(method, &arguments),
            "posts" => self.posts(method, &arguments),
            "media" => self.media(method, &arguments),
            "scripts" => self.scripts(method, &arguments),
            "templates" => self.templates(method, &arguments),
            "tags" => self.tags(method, &arguments),
            "tasks" => self.tasks(method, &arguments),
            "sync" => self.sync(method, &arguments),
            "publish" => self.publish(method, &arguments),
            "chat" => self.chat(method, &arguments),
            "embeddings" => self.embeddings(method, &arguments),
            "bds" if method == "report_progress" => self.report_progress(&arguments),
            _ => Err(format!("unknown host capability: {namespace}.{method}").into()),
        };
        result.map_err(text)
    }
}

fn public_project(value: Project) -> HostResult<Value> {
    Ok(json!({
        "id": value.id, "name": value.name, "slug": value.slug,
        "description": value.description, "data_path": value.data_path,
        "is_active": value.is_active, "created_at": iso(value.created_at),
        "updated_at": iso(value.updated_at),
    }))
}

fn public_post(conn: &DbConnection, value: Post) -> HostResult<Value> {
    Ok(json!({
        "id": value.id, "project_id": value.project_id, "title": value.title,
        "slug": value.slug, "status": value.status.as_str(), "language": value.language,
        "tags": value.tags, "categories": value.categories,
        "backlinks": linked_post_records(conn, &value.id, true, Some(&value.project_id))?,
        "links_to": linked_post_records(conn, &value.id, false, Some(&value.project_id))?,
        "created_at": iso(value.created_at), "updated_at": iso(value.updated_at),
    }))
}

fn public_posts(conn: &DbConnection, values: Vec<Post>) -> HostResult<Value> {
    values
        .into_iter()
        .map(|value| public_post(conn, value))
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn public_media(value: Media) -> HostResult<Value> {
    Ok(json!({
        "id": value.id, "project_id": value.project_id, "original_name": value.original_name,
        "mime_type": value.mime_type, "file_path": value.file_path, "title": value.title,
        "alt": value.alt, "caption": value.caption, "tags": value.tags,
        "created_at": iso(value.created_at), "updated_at": iso(value.updated_at),
    }))
}

fn public_script(value: Script) -> HostResult<Value> {
    Ok(json!({
        "id": value.id, "project_id": value.project_id, "slug": value.slug,
        "title": value.title, "kind": value.kind.as_str(), "entrypoint": value.entrypoint,
        "enabled": value.enabled, "status": value.status.as_str(),
        "created_at": iso(value.created_at), "updated_at": iso(value.updated_at),
    }))
}

fn public_template(value: Template) -> HostResult<Value> {
    Ok(json!({
        "id": value.id, "project_id": value.project_id, "slug": value.slug,
        "title": value.title, "kind": value.kind.as_str(), "enabled": value.enabled,
        "status": value.status.as_str(), "created_at": iso(value.created_at),
        "updated_at": iso(value.updated_at),
    }))
}

fn public_tag(value: Tag) -> HostResult<Value> {
    Ok(json!({
        "id": value.id, "project_id": value.project_id, "name": value.name,
        "color": value.color, "post_template_slug": value.post_template_slug,
        "created_at": iso(value.created_at), "updated_at": iso(value.updated_at),
    }))
}

fn public_task(value: TaskSnapshot) -> HostResult<Value> {
    let (status, error) = match value.status {
        TaskStatus::Pending => ("pending", None),
        TaskStatus::Running => ("running", None),
        TaskStatus::Completed => ("completed", None),
        TaskStatus::Cancelled => ("cancelled", None),
        TaskStatus::Failed(error) => ("failed", Some(error)),
    };
    Ok(
        json!({"id":value.id.to_string(),"name":value.label,"status":status,
        "progress":value.progress,"message":value.message.or(error)}),
    )
}

fn public_tasks(values: Vec<TaskSnapshot>) -> HostResult<Value> {
    values
        .into_iter()
        .map(public_task)
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn public_git_repository(value: engine::git::GitRepository) -> Value {
    json!({
        "is_initialized": value.is_initialized,
        "remote_url": value.remote_url,
        "provider": value.provider.map(|provider| json!({"kind": match provider {
            engine::git::GitProvider::GitHub => "github",
            engine::git::GitProvider::GitLab => "gitlab",
            engine::git::GitProvider::GiteaForgejo => "gitea_forgejo",
        }})),
        "current_branch": value.current_branch,
        "has_lfs": value.has_lfs,
    })
}

fn public_git_status(value: engine::git::GitFileStatus) -> Value {
    let mut result = Map::new();
    result.insert("path".into(), value.path.into());
    if let Some(old_path) = value.old_path {
        result.insert("old_path".into(), old_path.into());
    }
    result.insert(
        "status".into(),
        match value.kind {
            engine::git::FileStatusKind::Added => "added",
            engine::git::FileStatusKind::Modified => "modified",
            engine::git::FileStatusKind::Deleted => "deleted",
            engine::git::FileStatusKind::Renamed => "renamed",
            engine::git::FileStatusKind::Untracked => "untracked",
        }
        .into(),
    );
    Value::Object(result)
}

fn public_git_commit(value: engine::git::GitCommit) -> Value {
    json!({
        "hash": value.hash,
        "subject": value.subject,
        "author": value.author,
        "date": value.date,
        "sync_status": {"kind": match value.sync_status {
            engine::git::SyncStatus::LocalOnly => "local_only",
            engine::git::SyncStatus::RemoteOnly => "remote_only",
            engine::git::SyncStatus::Both => "both",
        }},
    })
}

fn public_metadata(data_dir: &Path) -> HostResult<Value> {
    let metadata = engine::meta::read_project_json(data_dir)?;
    Ok(json!({
        "name": metadata.name, "description": metadata.description,
        "public_url": metadata.public_url, "main_language": metadata.main_language,
        "default_author": metadata.default_author,
        "categories": engine::meta::read_categories_json(data_dir)?,
        "blog_languages": metadata.blog_languages,
        "publishing_preferences": engine::meta::read_publishing_json(data_dir)?,
    }))
}

fn public_list<T>(values: Vec<T>, convert: fn(T) -> HostResult<Value>) -> HostResult<Value> {
    values
        .into_iter()
        .map(convert)
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

fn json_value<T: serde::Serialize>(value: Result<T, EngineError>) -> HostResult<Value> {
    serde_json::to_value(value?).map_err(|error| HostError::Message(error.to_string()))
}

fn iso(timestamp: i64) -> String {
    Utc.timestamp_millis_opt(timestamp)
        .single()
        .map(|value| value.to_rfc3339_opts(SecondsFormat::Millis, true))
        .unwrap_or_default()
}

fn text(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn object_arg(args: &[Value], index: usize) -> HostResult<&Map<String, Value>> {
    args.get(index)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("argument {} must be a table", index + 1).into())
}

fn string_arg(args: &[Value], index: usize) -> HostResult<&str> {
    args.get(index)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("argument {} must be a string", index + 1).into())
}

fn string_array_arg(args: &[Value], index: usize) -> HostResult<Vec<String>> {
    args.get(index)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .ok_or_else(|| format!("argument {} must be a table", index + 1).into())
}

fn string_field<'a>(value: &'a Map<String, Value>, field: &str) -> HostResult<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{field} must be a string").into())
}

fn optional_string_field<'a>(value: &'a Map<String, Value>, field: &str) -> Option<&'a str> {
    value.get(field).and_then(Value::as_str)
}

fn string_list(value: &Map<String, Value>, field: &str) -> Option<Vec<String>> {
    value.get(field).and_then(Value::as_array).map(|items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    })
}

fn assign_string(value: &Map<String, Value>, field: &str, target: &mut String) {
    if let Some(value) = optional_string_field(value, field) {
        *target = value.to_owned();
    }
}

fn assign_optional_string(value: &Map<String, Value>, field: &str, target: &mut Option<String>) {
    if let Some(value) = value.get(field) {
        *target = value.as_str().map(str::to_owned);
    }
}

fn optional_nullable_string<'a>(
    value: &'a Map<String, Value>,
    field: &str,
) -> Option<Option<&'a str>> {
    value.get(field).map(Value::as_str)
}

fn post_filters(value: &Map<String, Value>) -> crate::db::queries::post::PostFilterParams {
    crate::db::queries::post::PostFilterParams {
        search_query: optional_string_field(value, "query")
            .unwrap_or("")
            .to_owned(),
        status: optional_string_field(value, "status").map(str::to_owned),
        language: optional_string_field(value, "language").map(str::to_owned),
        year: value
            .get("year")
            .and_then(Value::as_i64)
            .map(|value| value as i32),
        month: value
            .get("month")
            .and_then(Value::as_u64)
            .map(|value| value as u32),
        from: value.get("from").and_then(timestamp_value),
        to: value.get("to").and_then(timestamp_value),
        tags: string_list(value, "tags").unwrap_or_default(),
        categories: string_list(value, "categories").unwrap_or_default(),
        ..Default::default()
    }
}

fn media_filters(value: &Map<String, Value>) -> crate::db::queries::media::MediaFilterParams {
    crate::db::queries::media::MediaFilterParams {
        search_query: optional_string_field(value, "query")
            .unwrap_or("")
            .to_owned(),
        year: value
            .get("year")
            .and_then(Value::as_i64)
            .map(|value| value as i32),
        month: value
            .get("month")
            .and_then(Value::as_u64)
            .map(|value| value as u32),
        tags: string_list(value, "tags").unwrap_or_default(),
    }
}

fn timestamp_value(value: &Value) -> Option<i64> {
    value.as_i64().or_else(|| {
        value
            .as_str()
            .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.timestamp_millis())
    })
}

fn name_counts(values: impl Iterator<Item = String>) -> HostResult<Value> {
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    for value in values {
        *counts.entry(value).or_default() += 1;
    }
    Ok(Value::Array(
        counts
            .into_iter()
            .map(|(name, count)| json!({"name":name,"count":count}))
            .collect(),
    ))
}

fn linked_post_records(
    conn: &DbConnection,
    post_id: &str,
    incoming: bool,
    project_id: Option<&str>,
) -> HostResult<Vec<Value>> {
    let links = if incoming {
        crate::db::queries::post_link::list_links_by_target(conn, post_id)?
    } else {
        crate::db::queries::post_link::list_links_by_source(conn, post_id)?
    };
    Ok(links
        .into_iter()
        .filter_map(|link| {
            let id = if incoming {
                link.source_post_id
            } else {
                link.target_post_id
            };
            crate::db::queries::post::get_post_by_id(conn, &id)
                .ok()
                .filter(|post| project_id.is_none_or(|project_id| post.project_id == project_id))
                .map(|post| json!({"id":post.id,"title":post.title,"slug":post.slug}))
        })
        .collect())
}

fn linked_posts(
    conn: &DbConnection,
    project_id: &str,
    post_id: &str,
    incoming: bool,
) -> HostResult<Value> {
    let post = crate::db::queries::post::get_post_by_id(conn, post_id)?;
    if post.project_id != project_id {
        return Err("record is outside the active project".into());
    }
    Ok(Value::Array(linked_post_records(
        conn,
        post_id,
        incoming,
        Some(project_id),
    )?))
}

fn task_id_arg(args: &[Value], index: usize) -> HostResult<TaskId> {
    args.get(index)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        })
        .ok_or_else(|| format!("argument {} must be a task id", index + 1).into())
}

fn sanitize_timestamps(value: HostResult<Value>) -> HostResult<Value> {
    fn visit(value: &mut Value) {
        match value {
            Value::Array(values) => values.iter_mut().for_each(visit),
            Value::Object(values) => {
                for (key, value) in values {
                    if key.ends_with("_at")
                        && let Some(timestamp) = value.as_i64()
                    {
                        *value = iso(timestamp).into();
                    } else {
                        visit(value);
                    }
                }
            }
            _ => {}
        }
    }
    let mut value = value?;
    visit(&mut value);
    Ok(value)
}

fn post_ai_content(data_dir: &Path, post: &Post) -> HostResult<Value> {
    let content = match (&post.content, post.file_path.is_empty()) {
        (Some(content), _) => content.clone(),
        (None, false) => std::fs::read_to_string(data_dir.join(&post.file_path))
            .map_err(text)
            .and_then(|raw| {
                crate::util::frontmatter::read_post_file(&raw)
                    .map(|(_, body)| body)
                    .map_err(text)
            })?,
        _ => String::new(),
    };
    Ok(
        json!({"id":post.id,"title":post.title,"excerpt":post.excerpt,"content":content,
        "tags":post.tags,"categories":post.categories,"language":post.language}),
    )
}

fn one_shot_json(value: engine::ai::OneShotResponse) -> HostResult<Value> {
    let value = match value {
        engine::ai::OneShotResponse::Taxonomy(value) => serde_json::to_value(value),
        engine::ai::OneShotResponse::ImportTaxonomyMapping(value) => serde_json::to_value(value),
        engine::ai::OneShotResponse::PostAnalysis(value) => serde_json::to_value(value),
        engine::ai::OneShotResponse::LanguageDetection(value) => serde_json::to_value(value),
        engine::ai::OneShotResponse::Translation(value) => serde_json::to_value(value),
        engine::ai::OneShotResponse::ImageAnalysis(value) => serde_json::to_value(value),
        engine::ai::OneShotResponse::MediaTranslation(value) => serde_json::to_value(value),
    };
    value.map_err(|error| HostError::Message(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scripting::{ExecutionControl, ExecutionKind, execute_with_host};
    use std::fs;
    use std::process::Command;

    struct SyncFixture {
        _temp: tempfile::TempDir,
        db_path: PathBuf,
        data_dir: PathBuf,
        remote_dir: PathBuf,
        project_id: String,
    }

    impl SyncFixture {
        fn new() -> Self {
            let temp = tempfile::tempdir().unwrap();
            let db_path = temp.path().join("ruds.db");
            let data_dir = temp.path().join("project");
            let remote_dir = temp.path().join("remote.git");
            let upstream_dir = temp.path().join("upstream");
            let db = Database::open(&db_path).unwrap();
            db.migrate().unwrap();
            crate::db::fts::ensure_fts_tables(db.conn()).unwrap();
            let project = engine::project::create_project(
                db.conn(),
                "Active",
                Some(data_dir.to_str().unwrap()),
            )
            .unwrap();
            engine::project::set_active_project(db.conn(), &project.id).unwrap();
            drop(db);

            git(&data_dir, &["init", "-b", "master"]);
            configure_git(&data_dir);
            git(&data_dir, &["add", "-A"]);
            git(&data_dir, &["commit", "-m", "Initial project"]);
            git(
                temp.path(),
                &[
                    "init",
                    "--bare",
                    "-b",
                    "master",
                    remote_dir.to_str().unwrap(),
                ],
            );
            git(
                &data_dir,
                &["remote", "add", "origin", remote_dir.to_str().unwrap()],
            );
            git(&data_dir, &["push", "-u", "origin", "master"]);
            git(
                temp.path(),
                &[
                    "clone",
                    remote_dir.to_str().unwrap(),
                    upstream_dir.to_str().unwrap(),
                ],
            );
            configure_git(&upstream_dir);
            let mut metadata = engine::meta::read_project_json(&upstream_dir).unwrap();
            metadata.name = "Pulled Name".into();
            engine::meta::write_project_json(&upstream_dir, &metadata).unwrap();
            git(&upstream_dir, &["add", "-A"]);
            git(&upstream_dir, &["commit", "-m", "Update project name"]);
            git(&upstream_dir, &["push", "origin", "master"]);
            fs::write(data_dir.join("pending.txt"), "pending").unwrap();

            Self {
                _temp: temp,
                db_path,
                data_dir,
                remote_dir,
                project_id: project.id,
            }
        }

        fn host(&self, offline: bool) -> CoreHost {
            CoreHost::new(&self.db_path, &self.project_id, &self.data_dir)
                .with_offline_mode(offline)
        }
    }

    fn configure_git(dir: &Path) {
        git(dir, &["config", "user.name", "Lua Test"]);
        git(dir, &["config", "user.email", "lua@example.invalid"]);
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    #[test]
    fn sync_namespace_runs_every_bds2_method_and_reconciles_pull() {
        let fixture = SyncFixture::new();
        let result = execute_with_host(
            r#"
                function main()
                    local status = bds.sync.get_status()
                    local history = bds.sync.get_history()
                    return {
                        available = bds.sync.check_availability(),
                        repo = bds.sync.get_repo_state(),
                        remote = bds.sync.get_remote_state(),
                        status = status,
                        history = history,
                        fetched = bds.sync.fetch(),
                        pulled = bds.sync.pull(),
                        committed = bds.sync.commit_all("Lua commit"),
                        pushed = bds.sync.push(),
                    }
                end
            "#,
            "main",
            &Value::Null,
            ExecutionKind::Utility,
            &ExecutionControl::default(),
            Arc::new(fixture.host(false)),
        )
        .unwrap();

        assert_eq!(result.value["available"], true);
        assert_eq!(result.value["repo"]["is_initialized"], true);
        assert_eq!(result.value["remote"], result.value["repo"]);
        assert!(
            result.value["status"]["files"]
                .as_array()
                .unwrap()
                .iter()
                .any(|file| file == &json!({"path":"pending.txt","status":"untracked"}))
        );
        let commit = &result.value["history"]["commits"][0];
        assert!(commit["date"].as_str().is_some_and(|date| date.len() == 10));
        assert!(commit["sync_status"]["kind"].is_string());
        assert_eq!(result.value["fetched"]["updated"], true);
        assert_eq!(result.value["pulled"]["updated"], true);
        assert_eq!(result.value["committed"]["message"], "Lua commit");
        assert_eq!(result.value["pushed"]["updated"], true);

        let db = Database::open(&fixture.db_path).unwrap();
        assert_eq!(
            project::get_project_by_id(db.conn(), &fixture.project_id)
                .unwrap()
                .name,
            "Pulled Name"
        );
        assert_eq!(
            git(&fixture.remote_dir, &["log", "-1", "--format=%s"]),
            "Lua commit"
        );
    }

    #[test]
    fn sync_namespace_uses_nil_for_non_repository_failures() {
        let temp = tempfile::tempdir().unwrap();
        let data_dir = temp.path().join("not-a-repository");
        fs::create_dir(&data_dir).unwrap();
        let result = execute_with_host(
            r#"
                function main()
                    return {
                        repo = bds.sync.get_repo_state(),
                        remote = bds.sync.get_remote_state(),
                        status = bds.sync.get_status(),
                        history = bds.sync.get_history(),
                        fetched = bds.sync.fetch(),
                        pulled = bds.sync.pull(),
                        pushed = bds.sync.push(),
                        committed = bds.sync.commit_all("no repository"),
                    }
                end
            "#,
            "main",
            &Value::Null,
            ExecutionKind::Utility,
            &ExecutionControl::default(),
            Arc::new(CoreHost::new(
                temp.path().join("missing.db"),
                "p1",
                &data_dir,
            )),
        )
        .unwrap();

        assert_eq!(result.value["repo"]["is_initialized"], false);
        assert_eq!(result.value["remote"], result.value["repo"]);
        assert_eq!(result.value["history"], json!({"commits": []}));
        for key in ["status", "fetched", "pulled", "pushed", "committed"] {
            assert!(result.value[key].is_null(), "{key} was not nil");
        }
    }

    #[test]
    fn sync_namespace_airplane_gate_prevents_network_commands_and_rejects_blank_commit() {
        let fixture = SyncFixture::new();
        git(&fixture.data_dir, &["add", "pending.txt"]);
        git(&fixture.data_dir, &["commit", "-m", "Local only"]);
        let local_head = git(&fixture.data_dir, &["rev-parse", "HEAD"]);
        let tracking_head = git(
            &fixture.data_dir,
            &["rev-parse", "refs/remotes/origin/master"],
        );
        let remote_head = git(&fixture.remote_dir, &["rev-parse", "refs/heads/master"]);

        let result = execute_with_host(
            r#"
                function main()
                    return {
                        fetched = bds.sync.fetch(),
                        pulled = bds.sync.pull(),
                        pushed = bds.sync.push(),
                        committed = bds.sync.commit_all("   "),
                    }
                end
            "#,
            "main",
            &Value::Null,
            ExecutionKind::Utility,
            &ExecutionControl::default(),
            Arc::new(fixture.host(true)),
        )
        .unwrap();

        for key in ["fetched", "pulled", "pushed", "committed"] {
            assert!(result.value[key].is_null(), "{key} was not nil");
        }
        assert_eq!(git(&fixture.data_dir, &["rev-parse", "HEAD"]), local_head);
        assert_eq!(
            git(
                &fixture.data_dir,
                &["rev-parse", "refs/remotes/origin/master"],
            ),
            tracking_head
        );
        assert_eq!(
            git(&fixture.remote_dir, &["rev-parse", "refs/heads/master"]),
            remote_head
        );
    }

    #[test]
    fn project_host_round_trips_engines_and_enforces_scope_and_failure_values() {
        let temp = tempfile::tempdir().unwrap();
        let db_path = temp.path().join("ruds.db");
        let data_dir = temp.path().join("project");
        let foreign_dir = temp.path().join("foreign");
        let db = Database::open(&db_path).unwrap();
        db.migrate().unwrap();
        crate::db::fts::ensure_fts_tables(db.conn()).unwrap();
        let active =
            engine::project::create_project(db.conn(), "Active", Some(data_dir.to_str().unwrap()))
                .unwrap();
        let foreign = engine::project::create_project(
            db.conn(),
            "Foreign",
            Some(foreign_dir.to_str().unwrap()),
        )
        .unwrap();
        engine::project::set_active_project(db.conn(), &active.id).unwrap();
        let foreign_post = engine::post::create_post(
            db.conn(),
            &foreign_dir,
            &foreign.id,
            "Foreign post",
            Some("secret"),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
        )
        .unwrap();
        drop(db);

        let manager = Arc::new(TaskManager::default());
        let task_id = manager.submit("Lua utility");
        let media_source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/golden-generated-sites/rfc1437-sample/images/close.png");
        let host = CoreHost::new(&db_path, &active.id, &data_dir)
            .with_task(Arc::clone(&manager), task_id)
            .with_offline_mode(true);
        let result = execute_with_host(
            r#"
                function main(input)
                    local project = bds.projects.get_active()
                    local post = bds.posts.create({title = "Lua post", content = "Hello"})
                    local script = bds.scripts.create({title = "Lua utility", kind = "utility", content = "function main() end"})
                    local template = bds.templates.create({title = "Lua post", kind = "post", content = "{{ content }}"})
                    local tag = bds.tags.create({name = "lua"})
                    local media = bds.media.import({source_path = input.media_source, original_name = "close.png", title = "Close"})
                    local metadata = bds.meta.add_category("Lua")
                    local tasks = bds.tasks.status_snapshot()
                    local upload = bds.publish.upload_site({})
                    bds.report_progress({current = 1, total = 2, message = "half"})
                    return {
                        project = project.name,
                        post = bds.posts.get(post.id).title,
                        script = script.kind,
                        template = template.kind,
                        template_validation = bds.templates.validate("{{ title | upcase }}"),
                        tag = tag.name,
                        media = media.title,
                        category = metadata.categories[1],
                        tasks = tasks.running_count,
                        foreign = bds.posts.get(input.foreign_post),
                        ai = bds.chat.detect_post_language("Title", "Body"),
                        upload = upload,
                        timestamp = post.created_at,
                        project_path = bds.app.get_default_project_path(),
                        embedding_progress = bds.embeddings.get_progress(),
                        embedding_backfill = bds.embeddings.index_unindexed_posts(),
                        foreign_embedding = bds.embeddings.find_similar(input.foreign_post, 5),
                    }
                end
            "#,
            "main",
            &json!({"foreign_post":foreign_post.id,"media_source":media_source}),
            ExecutionKind::Utility,
            &ExecutionControl::default(),
            Arc::new(host),
        ).unwrap();

        assert_eq!(result.value["project"], "Active");
        assert_eq!(result.value["post"], "Lua post");
        assert_eq!(result.value["script"], "utility");
        assert_eq!(result.value["template"], "post");
        assert_eq!(result.value["template_validation"]["valid"], false);
        assert_eq!(
            result.value["template_validation"]["errors"][0],
            "unsupported filter: upcase"
        );
        assert_eq!(result.value["tag"], "lua");
        assert_eq!(result.value["media"], "Close");
        assert!(result.value["foreign"].is_null());
        assert_eq!(result.value["ai"], json!({}));
        assert!(result.value["upload"].is_null());
        assert!(result.value["timestamp"].as_str().unwrap().contains('T'));
        assert_eq!(manager.progress(task_id), Some(0.5));
        assert_eq!(
            result.value["embedding_progress"],
            json!({"indexed": 0, "total": 1})
        );
        assert_eq!(result.value["embedding_backfill"], json!([]));
        assert!(result.value["foreign_embedding"].is_null());
    }
}
