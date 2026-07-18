use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine as _;
use chrono::Datelike;
use iced::{Element, Subscription, Task, window};
use rusqlite::Error as SqlError;
use serde_json::json;
use uuid::Uuid;

use bds_core::db::Database;
use bds_core::engine;
use bds_core::engine::ai::{self, AiEndpointConfig, AiEndpointKind};
use bds_core::engine::task::{TaskId, TaskManager, TaskStatus};
use bds_core::i18n::{UiLocale, detect_os_locale};
use bds_core::model::{
    Media, Post, PostStatus, PostTranslation, Project, PublishingPreferences, Script, SshMode,
    Template,
};

use crate::components::webview::{self, WebViewConfig, WebViewController};
use crate::i18n::{t, tw};
use crate::platform::menu::{self, MenuAction, MenuRegistry};
use crate::state::navigation::{
    OutputEntry, PanelTab, SidebarView, TaskSnapshot, handle_activity_click,
};
use crate::state::sidebar_filter::{CalendarMonth, CalendarYear, MediaFilter, PostFilter};
use crate::state::tabs::{self, Tab, TabType};
use crate::state::toast::{Toast, ToastLevel};
use crate::views::{
    dashboard::{
        DashboardCategory, DashboardRecentPost, DashboardState, DashboardStats, DashboardTag,
        DashboardTimelineMonth,
    },
    media_editor::{LinkedPostItem, MediaEditorMsg, MediaEditorState},
    modal,
    post_editor::{LinkedMediaItem, PostEditorMsg, PostEditorState, ResolvedPostLink},
    script_editor::{ScriptEditorMsg, ScriptEditorState},
    settings_view::{
        AiModelOption, SettingsCategoryRow, SettingsMsg, SettingsViewState, default_category_rows,
    },
    site_validation::SiteValidationState,
    tags_view::{self, TagsMsg, TagsSection, TagsViewState},
    template_editor::{TemplateEditorMsg, TemplateEditorState},
    workspace,
};

// ───────────────────────────────────────────────────────────
// Message
// ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum Message {
    // Menu
    MenuEvent(muda::MenuId),

    // Navigation
    SetActiveView(SidebarView),
    ToggleSidebar,
    TogglePanel,
    OpenSettingsSection(crate::views::settings_view::SettingsSection),

    // Sidebar resize
    SidebarResizeStart,
    SidebarResizeMove(f32),
    SidebarResizeEnd,

    // Tabs
    OpenTab(Tab),
    CloseTab(String),
    SelectTab(String),
    PinTab(String),
    ClearTabs,

    // Project
    ProjectsLoaded(Vec<Project>),
    SwitchProject(String),
    ProjectSwitched(Result<String, String>),
    RequestCreateProject,
    CreateProject {
        name: String,
        data_path: Option<PathBuf>,
    },
    ProjectCreated(Result<String, String>),
    DeleteProject(String),
    ProjectDeleted(Result<String, String>),

    // Dialogs
    FolderPicked(Option<PathBuf>),
    MediaFilesPicked(Option<Vec<PathBuf>>),

    // Tasks
    TaskTick,

    // macOS lifecycle
    FileOpenRequested(PathBuf),
    UrlOpenRequested(String),
    MainWindowLoaded(Option<window::Id>),
    EmbeddedPreviewReady(Result<(), String>),

    // Panel
    SetPanelTab(PanelTab),

    // Settings
    SetOfflineMode(bool),
    SetUiLocale(UiLocale),
    ToggleLocaleDropdown,
    ToggleProjectDropdown,

    // Toast
    ShowToast(ToastLevel, String),
    DismissToast(u64),
    ExpireToasts,

    // Sidebar filters
    PostSearchChanged(String),
    TogglePostFilterPanel,
    SetPostStatusFilter(Option<String>),
    SetPostLanguageFilter(Option<String>),
    SetPostCalendarYear(Option<i32>),
    SetPostCalendarMonth(Option<u32>),
    SetPostFromDate(String),
    SetPostToDate(String),
    TogglePostTagFilter(String),
    TogglePostCategoryFilter(String),
    ClearPostFilters,
    MediaSearchChanged(String),
    ToggleMediaFilterPanel,
    SetMediaCalendarYear(Option<i32>),
    SetMediaCalendarMonth(Option<u32>),
    ToggleMediaTagFilter(String),
    ClearMediaFilters,

    // Modal
    ShowModal(modal::ModalState),
    DismissModal,
    ConfirmModal(modal::ConfirmAction),
    ToggleAiSuggestionField(usize, bool),
    ApplyAiSuggestions(modal::AiEntityTarget, Vec<modal::AiSuggestionField>),

    // Blog actions (dispatched to engine)
    RebuildDatabase,
    ReindexText,
    RegenerateCalendar,
    ValidateTranslations,
    ValidateMedia,
    GenerateSite,
    RunMetadataDiff,
    RunSiteValidation,
    ApplySiteValidation,
    EngineTaskDone {
        task_id: TaskId,
        label: String,
        result: Result<String, String>,
    },
    SiteValidationLoaded(Result<engine::validate_site::SiteValidationReport, String>),
    SiteValidationApplied(Result<String, String>),

    // Editor views
    PostEditor(PostEditorMsg),
    MediaEditor(MediaEditorMsg),
    TemplateEditor(TemplateEditorMsg),
    ScriptEditor(ScriptEditorMsg),
    Tags(TagsMsg),
    Settings(SettingsMsg),

    // Editor data loading
    PostLoaded(Result<Post, String>),
    MediaLoaded(Result<Media, String>),
    TemplateLoaded(Result<Template, String>),
    ScriptLoaded(Result<Script, String>),

    // Async sidebar data
    SidebarPostsLoaded(Vec<Post>),
    SidebarMediaLoaded {
        items: Vec<Media>,
        thumbs: HashMap<String, Option<std::path::PathBuf>>,
    },
    SidebarPostsAppended(Vec<Post>),
    SidebarMediaAppended {
        items: Vec<Media>,
        thumbs: HashMap<String, Option<std::path::PathBuf>>,
    },
    LoadMorePosts,
    LoadMoreMedia,

    CreatePost,
    CreatePage,
    CreateMedia,
    CreateScript,
    CreateTemplate,

    Noop,
    InitMenuBar,
}

enum PersistedPostState {
    Canonical(Box<Post>),
    Translation(Box<bds_core::model::PostTranslation>),
}

const POST_AUTO_SAVE_DELAY_MS: i64 = 3_000;

fn persist_post_editor_state_impl(
    db: &Database,
    data_dir: &Path,
    state: &PostEditorState,
) -> Result<PersistedPostState, String> {
    if state.active_language != state.canonical_language {
        let translation = engine::post::upsert_translation(
            db.conn(),
            data_dir,
            &state.post_id,
            &state.active_language,
            &state.title,
            if state.excerpt.is_empty() {
                None
            } else {
                Some(state.excerpt.as_str())
            },
            Some(&state.content),
        )
        .map_err(|e| e.to_string())?;
        Ok(PersistedPostState::Translation(Box::new(translation)))
    } else {
        let post = engine::post::update_post(
            db.conn(),
            data_dir,
            &state.post_id,
            Some(&state.title),
            if state.published_at.is_some() {
                None
            } else {
                Some(&state.slug)
            },
            Some(if state.excerpt.is_empty() {
                None
            } else {
                Some(state.excerpt.as_str())
            }),
            Some(&state.content),
            Some(state.tags.clone()),
            Some(state.categories.clone()),
            Some(if state.author.is_empty() {
                None
            } else {
                Some(state.author.as_str())
            }),
            Some(if state.language.is_empty() {
                None
            } else {
                Some(state.language.as_str())
            }),
            Some(if state.template_slug.is_empty() {
                None
            } else {
                Some(state.template_slug.as_str())
            }),
            Some(state.do_not_translate),
        )
        .map_err(|e| e.to_string())?;
        Ok(PersistedPostState::Canonical(Box::new(post)))
    }
}

fn persist_post_editor_preview_state_impl(
    db: &Database,
    state: &PostEditorState,
) -> Result<PersistedPostState, String> {
    if state.active_language != state.canonical_language {
        let post = bds_core::db::queries::post::get_post_by_id(db.conn(), &state.post_id)
            .map_err(|e| e.to_string())?;
        if post.do_not_translate {
            return Err("cannot create translation for a do-not-translate post".to_string());
        }

        let now = bds_core::util::now_unix_ms();
        match bds_core::db::queries::post_translation::get_post_translation_by_post_and_language(
            db.conn(),
            &state.post_id,
            &state.active_language,
        ) {
            Ok(mut translation) => {
                translation.title = state.title.clone();
                translation.excerpt = if state.excerpt.is_empty() {
                    None
                } else {
                    Some(state.excerpt.clone())
                };
                translation.content = Some(state.content.clone());
                translation.updated_at = now;
                bds_core::db::queries::post_translation::update_post_translation(
                    db.conn(),
                    &translation,
                )
                .map_err(|e| e.to_string())?;
                Ok(PersistedPostState::Translation(Box::new(translation)))
            }
            Err(SqlError::QueryReturnedNoRows) => {
                let translation = PostTranslation {
                    id: Uuid::new_v4().to_string(),
                    project_id: post.project_id,
                    translation_for: state.post_id.clone(),
                    language: state.active_language.clone(),
                    title: state.title.clone(),
                    excerpt: if state.excerpt.is_empty() {
                        None
                    } else {
                        Some(state.excerpt.clone())
                    },
                    content: Some(state.content.clone()),
                    status: PostStatus::Draft,
                    file_path: String::new(),
                    checksum: None,
                    created_at: now,
                    updated_at: now,
                    published_at: None,
                };
                bds_core::db::queries::post_translation::insert_post_translation(
                    db.conn(),
                    &translation,
                )
                .map_err(|e| e.to_string())?;
                Ok(PersistedPostState::Translation(Box::new(translation)))
            }
            Err(error) => Err(error.to_string()),
        }
    } else {
        let mut post = bds_core::db::queries::post::get_post_by_id(db.conn(), &state.post_id)
            .map_err(|e| e.to_string())?;

        if post.published_at.is_none() && state.slug != post.slug {
            match bds_core::db::queries::post::get_post_by_project_and_slug(
                db.conn(),
                &post.project_id,
                &state.slug,
            ) {
                Ok(existing) if existing.id != post.id => {
                    return Err(format!(
                        "slug '{}' already exists in this project",
                        state.slug
                    ));
                }
                Ok(_) | Err(SqlError::QueryReturnedNoRows) => {}
                Err(error) => return Err(error.to_string()),
            }
        }

        post.title = state.title.clone();
        if post.published_at.is_none() {
            post.slug = state.slug.clone();
        }
        post.excerpt = if state.excerpt.is_empty() {
            None
        } else {
            Some(state.excerpt.clone())
        };
        post.content = Some(state.content.clone());
        post.tags = state.tags.clone();
        post.categories = state.categories.clone();
        post.author = if state.author.is_empty() {
            None
        } else {
            Some(state.author.clone())
        };
        post.language = if state.language.is_empty() {
            None
        } else {
            Some(state.language.clone())
        };
        post.template_slug = if state.template_slug.is_empty() {
            None
        } else {
            Some(state.template_slug.clone())
        };
        post.do_not_translate = state.do_not_translate;
        if matches!(post.status, PostStatus::Published | PostStatus::Archived) {
            post.status = PostStatus::Draft;
        }
        post.updated_at = bds_core::util::now_unix_ms();

        bds_core::db::queries::post::update_post(db.conn(), &post).map_err(|e| e.to_string())?;
        Ok(PersistedPostState::Canonical(Box::new(post)))
    }
}

enum PersistedMediaState {
    Canonical {
        media: Box<Media>,
        tags: Vec<String>,
    },
    Translation,
}

struct PreviewSession {
    project_id: String,
    handle: engine::preview::PreviewServerHandle,
}

struct EmbeddedPreviewState {
    controller: WebViewController,
    current_url: Option<String>,
}

fn persist_media_editor_state_impl(
    db: &Database,
    data_dir: &Path,
    state: &MediaEditorState,
) -> Result<PersistedMediaState, String> {
    if state.active_language != state.canonical_language {
        engine::media::upsert_media_translation(
            db.conn(),
            data_dir,
            &state.media_id,
            &state.active_language,
            if state.title.is_empty() {
                None
            } else {
                Some(state.title.as_str())
            },
            if state.alt.is_empty() {
                None
            } else {
                Some(state.alt.as_str())
            },
            if state.caption.is_empty() {
                None
            } else {
                Some(state.caption.as_str())
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(PersistedMediaState::Translation)
    } else {
        let tags = state
            .tags_input
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(|tag| tag.to_string())
            .collect::<Vec<_>>();
        let media = engine::media::update_media(
            db.conn(),
            data_dir,
            &state.media_id,
            Some(if state.title.is_empty() {
                None
            } else {
                Some(state.title.as_str())
            }),
            Some(if state.alt.is_empty() {
                None
            } else {
                Some(state.alt.as_str())
            }),
            Some(if state.caption.is_empty() {
                None
            } else {
                Some(state.caption.as_str())
            }),
            Some(if state.author.is_empty() {
                None
            } else {
                Some(state.author.as_str())
            }),
            Some(if state.language.is_empty() {
                None
            } else {
                Some(state.language.as_str())
            }),
            Some(tags.clone()),
        )
        .map_err(|e| e.to_string())?;
        Ok(PersistedMediaState::Canonical {
            media: Box::new(media),
            tags,
        })
    }
}

fn default_post_editor_mode(settings_state: Option<&SettingsViewState>) -> &str {
    settings_state
        .map(|state| state.default_mode.as_str())
        .unwrap_or("markdown")
}

fn referenced_media_ids(content: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = content;

    while let Some(start) = rest.find("bds-media://") {
        let suffix = &rest[start + "bds-media://".len()..];
        let end = suffix
            .find(|ch: char| ch == ')' || ch == ']' || ch.is_whitespace())
            .unwrap_or(suffix.len());
        let media_id = suffix[..end].trim();
        if !media_id.is_empty() && !ids.iter().any(|existing| existing == media_id) {
            ids.push(media_id.to_string());
        }
        rest = &suffix[end..];
    }

    ids
}

fn draft_preview_url(post_id: &str, language: &str) -> String {
    format!(
        "http://{}:{}/__draft/{}?language={}",
        engine::preview::PREVIEW_HOST,
        engine::preview::PREVIEW_PORT,
        post_id,
        language
    )
}

fn save_template_editor_state_impl(
    db: &Database,
    project_id: &str,
    state: &TemplateEditorState,
) -> Result<Template, String> {
    engine::template::validate_template(&state.content)?;
    engine::template::update_template(
        db.conn(),
        &state.template_id,
        project_id,
        Some(&state.title),
        Some(&state.slug),
        Some(state.kind.clone()),
        Some(state.enabled),
        Some(&state.content),
    )
    .map_err(|e| e.to_string())
}

fn save_script_editor_state_impl(
    db: &Database,
    project_id: &str,
    state: &ScriptEditorState,
) -> Result<Script, String> {
    engine::script::validate_script_syntax(&state.content)?;
    engine::script::update_script(
        db.conn(),
        &state.script_id,
        project_id,
        Some(&state.title),
        Some(&state.slug),
        Some(state.kind.clone()),
        Some(&state.entrypoint),
        Some(state.enabled),
        Some(&state.content),
    )
    .map_err(|e| e.to_string())
}

fn save_editor_settings_state_impl(db: &Database, state: &SettingsViewState) -> Result<(), String> {
    let now = bds_core::util::now_unix_ms();
    [
        bds_core::db::queries::setting::set_setting_value(
            db.conn(),
            "editor.default_mode",
            &state.default_mode,
            now,
        ),
        bds_core::db::queries::setting::set_setting_value(
            db.conn(),
            "editor.diff_view_style",
            &state.diff_view_style,
            now,
        ),
        bds_core::db::queries::setting::set_setting_value(
            db.conn(),
            "editor.wrap_long_lines",
            if state.wrap_long_lines {
                "true"
            } else {
                "false"
            },
            now,
        ),
        bds_core::db::queries::setting::set_setting_value(
            db.conn(),
            "editor.hide_unchanged_regions",
            if state.hide_unchanged_regions {
                "true"
            } else {
                "false"
            },
            now,
        ),
    ]
    .into_iter()
    .collect::<Result<Vec<_>, _>>()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn month_abbreviation(month: u32) -> String {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "?",
    }
    .to_string()
}

fn format_timestamp(timestamp_ms: i64) -> String {
    let secs = timestamp_ms / 1000;
    let (year, month, day) = bds_core::util::timestamp::year_month_day_from_unix_ms(timestamp_ms);
    let hour = ((secs % 86_400) / 3_600) as u32;
    let minute = ((secs % 3_600) / 60) as u32;
    format!("{year}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

fn format_bytes(size: i64) -> String {
    let size = size.max(0) as f64;
    if size < 1024.0 {
        return format!("{} B", size as i64);
    }
    if size < 1024.0 * 1024.0 {
        return format!("{:.1} KB", size / 1024.0);
    }
    if size < 1024.0 * 1024.0 * 1024.0 {
        return format!("{:.1} MB", size / (1024.0 * 1024.0));
    }
    format!("{:.1} GB", size / (1024.0 * 1024.0 * 1024.0))
}

// ───────────────────────────────────────────────────────────
// App State
// ───────────────────────────────────────────────────────────

pub struct BdsApp {
    // Database
    db: Option<Database>,
    db_path: PathBuf,

    // Project
    active_project: Option<Project>,
    projects: Vec<Project>,
    data_dir: Option<PathBuf>,

    // Counts
    post_count: usize,
    media_count: usize,

    // Sidebar data
    sidebar_posts: Vec<Post>,
    sidebar_media: Vec<Media>,
    sidebar_scripts: Vec<Script>,
    sidebar_templates: Vec<Template>,
    sidebar_media_thumbs: HashMap<String, Option<std::path::PathBuf>>,
    sidebar_posts_has_more: bool,
    sidebar_media_has_more: bool,

    // Sidebar filters (per sidebar_views.allium PostsView / MediaView)
    post_filter: PostFilter,
    page_filter: PostFilter,
    media_filter: MediaFilter,

    // Navigation
    sidebar_view: SidebarView,
    sidebar_visible: bool,
    sidebar_width: f32,
    sidebar_dragging: bool,

    // Tabs
    tabs: Vec<Tab>,
    active_tab: Option<String>,

    // Panel
    panel_visible: bool,
    panel_tab: PanelTab,

    // Tasks
    task_manager: Arc<TaskManager>,
    task_snapshots: Vec<TaskSnapshot>,
    output_entries: Vec<OutputEntry>,

    // Platform
    _menu_bar: Option<muda::Menu>,
    menu_registry: MenuRegistry,

    // i18n
    ui_locale: UiLocale,
    /// Content/render language — the blog's main_language from project.json.
    /// Separate from ui_locale per i18n.allium TwoLocaleAxes.
    content_language: String,
    /// All blog languages from project.json (for translation flags).
    blog_languages: Vec<String>,

    // Flags
    offline_mode: bool,
    locale_dropdown_open: bool,
    project_dropdown_open: bool,
    theme_badge: String,

    // Toasts
    toasts: Vec<Toast>,

    // Modal
    active_modal: Option<modal::ModalState>,

    // Local preview
    preview_session: Option<PreviewSession>,
    embedded_preview: Option<EmbeddedPreviewState>,
    main_window_id: Option<window::Id>,

    // Editor states (keyed by entity id)
    post_editors: HashMap<String, PostEditorState>,
    media_editors: HashMap<String, MediaEditorState>,
    template_editors: HashMap<String, TemplateEditorState>,
    script_editors: HashMap<String, ScriptEditorState>,
    tags_view_state: Option<TagsViewState>,
    settings_state: Option<SettingsViewState>,
    dashboard_state: Option<DashboardState>,
    site_validation_state: SiteValidationState,
}

// ───────────────────────────────────────────────────────────
// App Implementation
// ───────────────────────────────────────────────────────────

impl BdsApp {
    pub fn new() -> (Self, Task<Message>) {
        let locale = detect_os_locale();
        let (menu_bar, registry) = menu::build_menu_bar(locale);

        // Open or create the database
        let db_path = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("bds")
            .join("bds.db");

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let mut db = Database::open(&db_path).ok();
        if let Some(ref mut db) = db {
            let _ = db.migrate();
        }

        // Load projects
        let projects = db
            .as_ref()
            .and_then(|d| engine::project::list_projects(d.conn()).ok())
            .unwrap_or_default();

        let active_project = db
            .as_ref()
            .and_then(|d| engine::project::get_active_project(d.conn()).ok())
            .flatten();

        let data_dir = active_project
            .as_ref()
            .and_then(|p| p.data_path.as_ref())
            .map(PathBuf::from);

        // If no projects exist, ensure the default project per spec
        let init_task = if projects.is_empty() {
            if let Some(ref db) = db {
                let default_data = dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("bds")
                    .join("projects")
                    .join("my-blog");
                match engine::project::ensure_default_project(db.conn(), Some(&default_data)) {
                    Ok(project) => Task::done(Message::ProjectsLoaded(vec![project])),
                    Err(_) => Task::none(),
                }
            } else {
                Task::none()
            }
        } else {
            Task::done(Message::ProjectsLoaded(projects.clone()))
        };

        // Chain menu initialization after project loading
        // (must happen after the event loop has started for macOS)
        let init_task = Task::batch([
            init_task,
            Task::done(Message::InitMenuBar),
            window::get_oldest().map(Message::MainWindowLoaded),
        ]);
        registry.set_enabled(MenuAction::Save, false);
        registry.set_enabled(MenuAction::PublishSelected, false);
        registry.set_enabled(MenuAction::PreviewPost, false);
        registry.set_enabled(MenuAction::Find, false);
        registry.set_enabled(MenuAction::Replace, false);
        registry.set_enabled(MenuAction::OpenInBrowser, false);

        (
            Self {
                db,
                db_path,
                active_project: active_project.clone(),
                projects,
                data_dir,
                post_count: 0,
                media_count: 0,
                sidebar_posts: Vec::new(),
                sidebar_media: Vec::new(),
                sidebar_scripts: Vec::new(),
                sidebar_templates: Vec::new(),
                sidebar_media_thumbs: HashMap::new(),
                sidebar_posts_has_more: false,
                sidebar_media_has_more: false,
                post_filter: PostFilter::default(),
                page_filter: PostFilter::default(),
                media_filter: MediaFilter::default(),
                sidebar_view: SidebarView::Posts,
                sidebar_visible: true,
                sidebar_width: 280.0,
                sidebar_dragging: false,
                tabs: Vec::new(),
                active_tab: None,
                panel_visible: false,
                panel_tab: PanelTab::Tasks,
                task_manager: Arc::new(TaskManager::default()),
                task_snapshots: Vec::new(),
                output_entries: Vec::new(),
                _menu_bar: Some(menu_bar),
                menu_registry: registry,
                ui_locale: locale,
                content_language: "en".to_string(),
                blog_languages: Vec::new(),
                offline_mode: false,
                locale_dropdown_open: false,
                project_dropdown_open: false,
                theme_badge: String::from("pico"),
                toasts: Vec::new(),
                active_modal: None,
                preview_session: None,
                embedded_preview: None,
                main_window_id: None,
                post_editors: HashMap::new(),
                media_editors: HashMap::new(),
                template_editors: HashMap::new(),
                script_editors: HashMap::new(),
                tags_view_state: None,
                settings_state: None,
                dashboard_state: None,
                site_validation_state: SiteValidationState::default(),
            },
            init_task,
        )
    }

    #[cfg(test)]
    fn new_for_tests(db: Database, project: Project, data_dir: PathBuf) -> Self {
        Self {
            db: Some(db),
            db_path: data_dir.join("bds.db"),
            active_project: Some(project.clone()),
            projects: vec![project],
            data_dir: Some(data_dir),
            post_count: 0,
            media_count: 0,
            sidebar_posts: Vec::new(),
            sidebar_media: Vec::new(),
            sidebar_scripts: Vec::new(),
            sidebar_templates: Vec::new(),
            sidebar_media_thumbs: HashMap::new(),
            sidebar_posts_has_more: false,
            sidebar_media_has_more: false,
            post_filter: PostFilter::default(),
            page_filter: PostFilter::default(),
            media_filter: MediaFilter::default(),
            sidebar_view: SidebarView::Posts,
            sidebar_visible: true,
            sidebar_width: 280.0,
            sidebar_dragging: false,
            tabs: Vec::new(),
            active_tab: None,
            panel_visible: false,
            panel_tab: PanelTab::Tasks,
            task_manager: Arc::new(TaskManager::default()),
            task_snapshots: Vec::new(),
            output_entries: Vec::new(),
            _menu_bar: None,
            menu_registry: MenuRegistry::empty(),
            ui_locale: UiLocale::En,
            content_language: "en".to_string(),
            blog_languages: Vec::new(),
            offline_mode: false,
            locale_dropdown_open: false,
            project_dropdown_open: false,
            theme_badge: String::from("pico"),
            toasts: Vec::new(),
            active_modal: None,
            preview_session: None,
            embedded_preview: None,
            main_window_id: None,
            post_editors: HashMap::new(),
            media_editors: HashMap::new(),
            template_editors: HashMap::new(),
            script_editors: HashMap::new(),
            tags_view_state: None,
            settings_state: None,
            dashboard_state: None,
            site_validation_state: SiteValidationState::default(),
        }
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            // ── Menu event dispatch ──
            Message::MenuEvent(id) => {
                if let Some(action) = self.menu_registry.lookup(&id) {
                    return self.dispatch_menu_action(action);
                }
                Task::none()
            }

            // ── Navigation ──
            Message::SetActiveView(view) => {
                let (new_view, new_visible) =
                    handle_activity_click(self.sidebar_view, self.sidebar_visible, view);
                let old_view = self.sidebar_view;
                self.sidebar_view = new_view;
                self.sidebar_visible = new_visible;
                // When switching to/from Posts/Pages, re-query with correct filter
                let needs_post_refresh = old_view != new_view
                    && matches!(new_view, SidebarView::Posts | SidebarView::Pages);
                if needs_post_refresh {
                    self.refresh_sidebar_posts()
                } else {
                    Task::none()
                }
            }
            Message::ToggleSidebar => {
                self.sidebar_visible = !self.sidebar_visible;
                Task::none()
            }
            Message::TogglePanel => {
                self.panel_visible = !self.panel_visible;
                Task::none()
            }
            Message::CreatePost => self.create_sidebar_post(false),
            Message::CreatePage => self.create_sidebar_post(true),
            Message::CreateMedia => crate::platform::dialog::pick_media_files(
                t(self.ui_locale, "dialog.importMedia"),
                t(self.ui_locale, "dialog.imageFilter"),
            ),
            Message::CreateScript => self.create_sidebar_script(),
            Message::CreateTemplate => self.create_sidebar_template(),
            Message::OpenSettingsSection(section) => {
                self.sidebar_view = SidebarView::Settings;
                self.sidebar_visible = true;
                self.open_singleton_tab(TabType::Settings, "common.settings");
                if self.settings_state.is_none() {
                    self.settings_state = Some(self.hydrate_settings_state());
                }
                if let Some(state) = self.settings_state.as_mut() {
                    state.focus_section(section);
                }
                self.sync_menu_state();
                Task::none()
            }
            Message::SidebarResizeStart => {
                self.sidebar_dragging = true;
                Task::none()
            }
            Message::SidebarResizeMove(x) => {
                if self.sidebar_dragging {
                    // x is global cursor position; subtract activity bar width (~48px)
                    let effective = x - 48.0;
                    self.sidebar_width = effective.clamp(200.0, 500.0);
                }
                Task::none()
            }
            Message::SidebarResizeEnd => {
                self.sidebar_dragging = false;
                Task::none()
            }

            // ── Tabs ──
            Message::OpenTab(tab) => {
                self.flush_active_post_editor();
                let idx = tabs::open_tab(&mut self.tabs, tab);
                if let Some(t) = self.tabs.get(idx) {
                    self.active_tab = Some(t.id.clone());
                    let tab_clone = t.clone();
                    self.load_editor_for_tab(&tab_clone);
                }
                self.enforce_panel_tab_fallback();
                self.sync_menu_state();
                self.sync_embedded_preview_for_active_post()
            }
            Message::CloseTab(id) => {
                if self.active_tab.as_deref() == Some(id.as_str()) {
                    self.flush_active_post_editor();
                }
                if let Some(next_idx) = tabs::close_tab(&mut self.tabs, &id) {
                    self.active_tab = self.tabs.get(next_idx).map(|t| t.id.clone());
                } else {
                    self.active_tab = None;
                }
                self.enforce_panel_tab_fallback();
                self.sync_menu_state();
                self.sync_embedded_preview_for_active_post()
            }
            Message::SelectTab(id) => {
                if self.tabs.iter().any(|t| t.id == id) {
                    if self.active_tab.as_deref() != Some(id.as_str()) {
                        self.flush_active_post_editor();
                    }
                    self.active_tab = Some(id);
                }
                self.enforce_panel_tab_fallback();
                self.sync_embedded_preview_for_active_post()
            }
            Message::PinTab(id) => {
                tabs::pin_tab(&mut self.tabs, &id);
                Task::none()
            }
            Message::ClearTabs => {
                self.flush_active_post_editor();
                self.tabs.clear();
                self.active_tab = None;
                self.hide_embedded_preview();
                Task::none()
            }

            // ── Project management ──
            Message::ProjectsLoaded(projects) => {
                self.projects = projects;
                if let Some(ref db) = self.db {
                    self.active_project = engine::project::get_active_project(db.conn())
                        .ok()
                        .flatten();
                    self.data_dir = self
                        .active_project
                        .as_ref()
                        .and_then(|p| p.data_path.as_ref())
                        .map(PathBuf::from);
                }
                // Per metadata.allium StartupSync: sync metadata from filesystem
                if let Some(data_dir) = self.data_dir.clone() {
                    if let Err(e) = engine::meta::startup_sync(&data_dir) {
                        let message =
                            self.operation_failed_text("common.metadataSync", e.to_string());
                        self.add_output(&message);
                    }
                    // Extract content language from project metadata
                    if let Ok(meta) = engine::meta::read_project_json(&data_dir) {
                        let main_lang = meta.main_language.unwrap_or_else(|| "en".to_string());
                        self.content_language = main_lang.clone();
                        self.blog_languages = meta.blog_languages;
                        if !self.blog_languages.contains(&main_lang) {
                            self.blog_languages.insert(0, main_lang);
                        }
                    }
                }
                let sidebar_task = self.refresh_counts();
                self.sync_menu_state();
                sidebar_task
            }
            Message::SwitchProject(project_id) => {
                self.project_dropdown_open = false;
                if let Some(ref db) = self.db {
                    match engine::project::set_active_project(db.conn(), &project_id) {
                        Ok(()) => {
                            self.active_project =
                                self.projects.iter().find(|p| p.id == project_id).cloned();
                            self.preview_session = None;
                            self.hide_embedded_preview();
                            self.data_dir = self
                                .active_project
                                .as_ref()
                                .and_then(|p| p.data_path.as_ref())
                                .map(PathBuf::from);
                            // Per metadata.allium StartupSync
                            if let Some(data_dir) = self.data_dir.clone() {
                                let _ = engine::meta::startup_sync(&data_dir);
                                if let Ok(meta) = engine::meta::read_project_json(&data_dir) {
                                    let main_lang =
                                        meta.main_language.unwrap_or_else(|| "en".to_string());
                                    self.content_language = main_lang.clone();
                                    self.blog_languages = meta.blog_languages;
                                    if !self.blog_languages.contains(&main_lang) {
                                        self.blog_languages.insert(0, main_lang);
                                    }
                                }
                            }
                            let name = self
                                .active_project
                                .as_ref()
                                .map(|p| p.name.clone())
                                .unwrap_or_default();
                            self.notify(
                                ToastLevel::Success,
                                &tw(
                                    self.ui_locale,
                                    "projectSelector.toast.switched",
                                    &[("name", &name)],
                                ),
                            );
                        }
                        Err(_) => {
                            self.notify(
                                ToastLevel::Error,
                                &t(self.ui_locale, "projectSelector.toast.switchFailed"),
                            );
                        }
                    }
                }
                self.sync_menu_state();
                Task::none()
            }
            Message::ProjectSwitched(result) => {
                match result {
                    Ok(name) => self.notify(
                        ToastLevel::Success,
                        &tw(
                            self.ui_locale,
                            "projectSelector.toast.switched",
                            &[("name", &name)],
                        ),
                    ),
                    Err(msg) => self.notify(ToastLevel::Error, &msg),
                }
                Task::none()
            }
            Message::RequestCreateProject => {
                crate::platform::dialog::pick_folder(t(self.ui_locale, "dialog.selectFolder"))
            }
            Message::CreateProject { name, data_path } => {
                if let Some(ref db) = self.db {
                    let path_str = data_path.as_ref().map(|p| p.to_string_lossy().to_string());
                    match engine::project::create_project(db.conn(), &name, path_str.as_deref()) {
                        Ok(project) => {
                            let _ = engine::project::set_active_project(db.conn(), &project.id);
                            self.projects =
                                engine::project::list_projects(db.conn()).unwrap_or_default();
                            self.active_project = Some(project.clone());
                            self.preview_session = None;
                            self.data_dir = project.data_path.as_ref().map(PathBuf::from);
                            let msg = tw(
                                self.ui_locale,
                                "projectSelector.toast.created",
                                &[("name", &project.name)],
                            );
                            self.notify(ToastLevel::Success, &msg);
                        }
                        Err(_) => {
                            self.notify(
                                ToastLevel::Error,
                                &t(self.ui_locale, "projectSelector.toast.createFailed"),
                            );
                        }
                    }
                }
                Task::none()
            }
            Message::ProjectCreated(result) => {
                match result {
                    Ok(name) => self.notify(
                        ToastLevel::Success,
                        &tw(
                            self.ui_locale,
                            "projectSelector.toast.created",
                            &[("name", &name)],
                        ),
                    ),
                    Err(msg) => self.notify(ToastLevel::Error, &msg),
                }
                Task::none()
            }
            Message::DeleteProject(project_id) => {
                if let Some(ref db) = self.db {
                    let data_path = self
                        .projects
                        .iter()
                        .find(|p| p.id == project_id)
                        .and_then(|p| p.data_path.as_ref())
                        .map(PathBuf::from);
                    match engine::project::delete_project(
                        db.conn(),
                        &project_id,
                        data_path.as_deref(),
                    ) {
                        Ok(()) => {
                            self.projects.retain(|p| p.id != project_id);
                        }
                        Err(_) => {
                            self.notify(
                                ToastLevel::Error,
                                &t(self.ui_locale, "projectSelector.toast.deleteFailed"),
                            );
                        }
                    }
                }
                Task::none()
            }
            Message::ProjectDeleted(result) => {
                if let Err(msg) = result {
                    self.notify(ToastLevel::Error, &msg);
                }
                Task::none()
            }

            // ── Dialogs ──
            Message::FolderPicked(path) => {
                if let Some(path) = path {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "New Project".to_string());
                    return Task::done(Message::CreateProject {
                        name,
                        data_path: Some(path),
                    });
                }
                Task::none()
            }
            Message::MediaFilesPicked(_paths) => {
                // Media import will be expanded in later milestones
                Task::none()
            }
            Message::MainWindowLoaded(window_id) => {
                self.main_window_id = window_id;
                if self.active_post_uses_embedded_preview() {
                    self.sync_embedded_preview_for_active_post()
                } else {
                    Task::none()
                }
            }
            Message::EmbeddedPreviewReady(result) => {
                match result {
                    Ok(()) => {
                        let visible = self.active_post_uses_embedded_preview();
                        if let Some(preview) = &mut self.embedded_preview {
                            preview.controller.take_staged();
                            if let Some(url) = preview.current_url.as_deref() {
                                preview.controller.navigate(url);
                            }
                            preview.controller.set_visible(visible);
                        }
                    }
                    Err(error) => {
                        self.notify(ToastLevel::Error, &error);
                    }
                }
                Task::none()
            }

            // ── Tasks ──
            Message::TaskTick => {
                self.refresh_task_snapshots();
                self.auto_save_due_post_editors();
                Task::none()
            }

            // ── macOS lifecycle ──
            Message::FileOpenRequested(_path) => {
                // File open handling deferred to later milestones
                Task::none()
            }
            Message::UrlOpenRequested(url) => {
                let _ = open::that(url);
                Task::none()
            }

            // ── Panel ──
            Message::SetPanelTab(tab) => {
                self.panel_tab = tab;
                Task::none()
            }

            // ── Settings ──
            Message::SetOfflineMode(mode) => {
                self.offline_mode = mode;
                self.sync_menu_state();
                Task::none()
            }
            Message::SetUiLocale(locale) => {
                self.ui_locale = locale;
                self.locale_dropdown_open = false;
                menu::update_menu_labels(&self.menu_registry, locale);
                // Re-translate singleton tab titles per tabs.allium
                for tab in &mut self.tabs {
                    if let Some(key) = tab.tab_type.i18n_key() {
                        tab.title = t(locale, key);
                    }
                }
                Task::none()
            }
            Message::ToggleLocaleDropdown => {
                self.locale_dropdown_open = !self.locale_dropdown_open;
                self.project_dropdown_open = false;
                Task::none()
            }
            Message::ToggleProjectDropdown => {
                self.project_dropdown_open = !self.project_dropdown_open;
                self.locale_dropdown_open = false;
                Task::none()
            }

            // ── Blog engine actions (async via TaskManager) ──
            Message::RebuildDatabase => self.spawn_engine_task(
                "engine.rebuildStarted",
                |db_path, project_id, data_dir, tm, tid| {
                    let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                    let on_progress: engine::rebuild::ProgressFn = Arc::new(move |pct, msg| {
                        tm.report_progress(tid, Some(pct), Some(msg.to_string()));
                    });
                    let report = engine::rebuild::rebuild_from_filesystem_with_progress(
                        db.conn(),
                        &data_dir,
                        &project_id,
                        Some(on_progress),
                    )
                    .map_err(|e| e.to_string())?;
                    let posts = report.posts_created + report.posts_updated;
                    let media = report.media_created + report.media_updated;
                    let templates = report.templates_created + report.templates_updated;
                    let scripts = report.scripts_created + report.scripts_updated;
                    Ok(format!(
                        "posts={posts}, media={media}, templates={templates}, scripts={scripts}"
                    ))
                },
            ),
            Message::ReindexText => {
                let locale = self.ui_locale;
                self.spawn_engine_task(
                    "engine.reindexStarted",
                    move |db_path, project_id, data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        tm.report_progress(
                            tid,
                            Some(0.0),
                            Some(t(locale, "engine.readingProjectConfig")),
                        );
                        let main_lang = engine::meta::read_project_json(&data_dir)
                            .ok()
                            .and_then(|m| m.main_language)
                            .unwrap_or_else(|| "en".to_string());
                        let tm2 = Arc::clone(&tm);
                        let on_item: engine::search::ItemProgressFn =
                            Box::new(move |current, total, name| {
                                let pct = if total > 0 {
                                    current as f32 / total as f32
                                } else {
                                    1.0
                                };
                                let msg = tw(
                                    locale,
                                    "engine.indexingItem",
                                    &[
                                        ("current", &current.to_string()),
                                        ("total", &total.to_string()),
                                        ("name", name),
                                    ],
                                );
                                tm2.report_progress(tid, Some(pct), Some(msg));
                            });
                        let report = engine::search::reindex_all_with_progress(
                            db.conn(),
                            &project_id,
                            &main_lang,
                            Some(on_item),
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(format!(
                            "posts={}, media={}",
                            report.posts_indexed, report.media_indexed
                        ))
                    },
                )
            }
            Message::RegenerateCalendar => {
                let locale = self.ui_locale;
                self.spawn_engine_task(
                    "engine.calendarStarted",
                    move |db_path, project_id, data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        tm.report_progress(tid, Some(0.20), Some(t(locale, "engine.loadingPosts")));
                        engine::calendar::regenerate_calendar(db.conn(), &data_dir, &project_id)
                            .map_err(|e| e.to_string())?;
                        tm.report_progress(
                            tid,
                            Some(0.90),
                            Some(t(locale, "engine.writingCalendar")),
                        );
                        Ok("done".to_string())
                    },
                )
            }
            Message::ValidateTranslations => {
                let locale = self.ui_locale;
                self.open_singleton_tab(
                    TabType::TranslationValidation,
                    "tabBar.translationValidation",
                );
                self.spawn_engine_task(
                    "engine.validateTranslationsStarted",
                    move |db_path, project_id, data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let meta = engine::meta::read_project_json(&data_dir)
                            .map_err(|e| e.to_string())?;
                        let main_lang = meta.main_language.as_deref().unwrap_or("en");
                        let blog_langs = meta.blog_languages.clone();
                        let tm2 = Arc::clone(&tm);
                        let on_item: engine::validate_translations::ItemProgressFn =
                            Box::new(move |current, total, name| {
                                let pct = if total > 0 {
                                    current as f32 / total as f32
                                } else {
                                    1.0
                                };
                                let msg = tw(
                                    locale,
                                    "engine.checkingItem",
                                    &[
                                        ("current", &current.to_string()),
                                        ("total", &total.to_string()),
                                        ("name", name),
                                    ],
                                );
                                tm2.report_progress(tid, Some(pct), Some(msg));
                            });
                        let report =
                            engine::validate_translations::validate_translations_with_progress(
                                db.conn(),
                                &data_dir,
                                &project_id,
                                &blog_langs,
                                main_lang,
                                Some(on_item),
                            )
                            .map_err(|e| e.to_string())?;
                        Ok(format!(
                            "db_issues={}, fs_issues={}",
                            report.db_issues.len(),
                            report.fs_issues.len()
                        ))
                    },
                )
            }
            Message::ValidateMedia => {
                let locale = self.ui_locale;
                self.spawn_engine_task(
                    "engine.validateMediaStarted",
                    move |db_path, project_id, _data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let on_item: engine::validate_media::ProgressFn =
                            Box::new(move |current, total, name| {
                                let pct = if total > 0 {
                                    current as f32 / total as f32
                                } else {
                                    1.0
                                };
                                let msg = tw(
                                    locale,
                                    "engine.checkingItem",
                                    &[
                                        ("current", &current.to_string()),
                                        ("total", &total.to_string()),
                                        ("name", name),
                                    ],
                                );
                                tm.report_progress(tid, Some(pct), Some(msg));
                            });
                        let report = engine::validate_media::validate_media(
                            db.conn(),
                            &_data_dir,
                            &project_id,
                            Some(on_item),
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(format!(
                            "checked={}, issues={}",
                            report.total_checked,
                            report.issues.len()
                        ))
                    },
                )
            }
            Message::GenerateSite => {
                let locale = self.ui_locale;
                self.spawn_engine_task(
                    "engine.generateSiteStarted",
                    move |db_path, project_id, data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let metadata = engine::meta::read_project_json(&data_dir)
                            .map_err(|e| e.to_string())?;
                        if metadata
                            .public_url
                            .as_deref()
                            .unwrap_or("")
                            .trim()
                            .is_empty()
                        {
                            return Err(
                                "public URL is required before generating the site".to_string()
                            );
                        }
                        let main_language = metadata
                            .main_language
                            .clone()
                            .unwrap_or_else(|| "en".to_string());
                        let all_posts = bds_core::db::queries::post::list_posts_by_project(
                            db.conn(),
                            &project_id,
                        )
                        .map_err(|e| e.to_string())?;
                        let published_posts = all_posts
                            .into_iter()
                            .filter(engine::generation::has_published_snapshot)
                            .collect::<Vec<_>>();
                        let total = published_posts.len().max(1) as f32;
                        let mut sources = Vec::new();
                        for (index, post) in published_posts.into_iter().enumerate() {
                            tm.report_progress(
                                tid,
                                Some(((index as f32) / total) * 0.7),
                                Some(tw(locale, "engine.renderingPost", &[("post", &post.slug)])),
                            );
                            if let Some(source) =
                                engine::generation::load_published_post_source(&data_dir, post)
                                    .map_err(|error| error.to_string())?
                            {
                                sources.push(source);
                            }
                        }
                        let output_dir = data_dir.join("html");
                        std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
                        tm.report_progress(
                            tid,
                            Some(0.85),
                            Some(t(locale, "engine.writingGeneratedFiles")),
                        );
                        let report = engine::generation::generate_starter_site(
                            db.conn(),
                            &output_dir,
                            &project_id,
                            &metadata,
                            &sources,
                            &main_language,
                        )
                        .map_err(|e| e.to_string())?;
                        tm.report_progress(
                            tid,
                            Some(1.0),
                            Some(t(locale, "engine.generationComplete")),
                        );
                        Ok(format!(
                            "written={}, skipped={}, output={}",
                            report.written_paths.len(),
                            report.skipped_paths.len(),
                            output_dir.display(),
                        ))
                    },
                )
            }
            Message::RunMetadataDiff => {
                self.open_singleton_tab(TabType::MetadataDiff, "tabBar.metadataDiff");
                Task::none()
            }
            Message::RunSiteValidation => self.start_site_validation(),
            Message::ApplySiteValidation => self.apply_site_validation(),
            Message::EngineTaskDone {
                task_id,
                label,
                result,
            } => {
                match &result {
                    Ok(detail) => {
                        self.task_manager.complete(task_id);
                        self.notify(ToastLevel::Success, &format!("{label}: {detail}"));
                    }
                    Err(err) => {
                        self.task_manager.fail(task_id, err.clone());
                        let message = tw(
                            self.ui_locale,
                            "common.operationFailed",
                            &[("operation", &label), ("error", &err)],
                        );
                        self.notify(ToastLevel::Error, &message);
                    }
                }
                let sidebar_task = self.refresh_counts();
                self.refresh_task_snapshots();
                sidebar_task
            }
            Message::SiteValidationLoaded(result) => {
                self.site_validation_state.is_running = false;
                self.site_validation_state.has_run = true;
                match result {
                    Ok(report) => {
                        self.site_validation_state.error_message = None;
                        self.site_validation_state.missing_files = report.missing_pages;
                        self.site_validation_state.extra_files = report.extra_pages;
                        self.site_validation_state.stale_files = report.stale_pages;
                        self.notify(
                            ToastLevel::Success,
                            &tw(
                                self.ui_locale,
                                "siteValidation.summary",
                                &[
                                    ("label", &t(self.ui_locale, "tabBar.siteValidation")),
                                    (
                                        "missing",
                                        &self.site_validation_state.missing_files.len().to_string(),
                                    ),
                                    (
                                        "extra",
                                        &self.site_validation_state.extra_files.len().to_string(),
                                    ),
                                    (
                                        "stale",
                                        &self.site_validation_state.stale_files.len().to_string(),
                                    ),
                                ],
                            ),
                        );
                    }
                    Err(error) => {
                        self.site_validation_state.error_message = Some(error.clone());
                        self.site_validation_state.missing_files.clear();
                        self.site_validation_state.extra_files.clear();
                        self.site_validation_state.stale_files.clear();
                        self.notify(ToastLevel::Error, &error);
                    }
                }
                Task::none()
            }
            Message::SiteValidationApplied(result) => {
                self.site_validation_state.is_applying = false;
                match result {
                    Ok(detail) => {
                        self.site_validation_state.error_message = None;
                        self.notify(ToastLevel::Success, &detail);
                        self.start_site_validation()
                    }
                    Err(error) => {
                        self.site_validation_state.error_message = Some(error.clone());
                        self.notify(ToastLevel::Error, &error);
                        Task::none()
                    }
                }
            }

            // ── Toast ──
            Message::ShowToast(level, msg) => {
                self.toasts.push(Toast::new(level, msg));
                Task::none()
            }
            Message::DismissToast(id) => {
                self.toasts.retain(|t| t.id != id);
                Task::none()
            }
            Message::ExpireToasts => {
                self.toasts.retain(|t| !t.is_expired());
                Task::none()
            }

            // ── Sidebar filters ──
            Message::PostSearchChanged(query) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.search_query = query;
                self.refresh_sidebar_posts()
            }
            Message::TogglePostFilterPanel => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.filter_panel_visible = !filter.filter_panel_visible;
                Task::none()
            }
            Message::SetPostStatusFilter(status) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.status_filter = status;
                self.refresh_sidebar_posts()
            }
            Message::SetPostLanguageFilter(language) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.language_filter = language;
                self.refresh_sidebar_posts()
            }
            Message::SetPostCalendarYear(year) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.calendar.selected_year = year;
                filter.calendar.selected_month = None;
                self.refresh_sidebar_posts()
            }
            Message::SetPostCalendarMonth(month) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.calendar.selected_month = month;
                self.refresh_sidebar_posts()
            }
            Message::SetPostFromDate(value) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.from_date = value;
                self.refresh_sidebar_posts()
            }
            Message::SetPostToDate(value) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.to_date = value;
                self.refresh_sidebar_posts()
            }
            Message::TogglePostTagFilter(tag) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                if let Some(pos) = filter.tag_filter.iter().position(|t| *t == tag) {
                    filter.tag_filter.remove(pos);
                } else {
                    filter.tag_filter.push(tag);
                }
                self.refresh_sidebar_posts()
            }
            Message::TogglePostCategoryFilter(cat) => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                if let Some(pos) = filter.category_filter.iter().position(|c| *c == cat) {
                    filter.category_filter.remove(pos);
                } else {
                    filter.category_filter.push(cat);
                }
                self.refresh_sidebar_posts()
            }
            Message::ClearPostFilters => {
                let filter = match self.sidebar_view {
                    SidebarView::Pages => &mut self.page_filter,
                    _ => &mut self.post_filter,
                };
                filter.clear();
                self.refresh_sidebar_posts()
            }
            Message::MediaSearchChanged(query) => {
                self.media_filter.search_query = query;
                self.refresh_sidebar_media()
            }
            Message::ToggleMediaFilterPanel => {
                self.media_filter.filter_panel_visible = !self.media_filter.filter_panel_visible;
                Task::none()
            }
            Message::SetMediaCalendarYear(year) => {
                self.media_filter.calendar.selected_year = year;
                self.media_filter.calendar.selected_month = None;
                self.refresh_sidebar_media()
            }
            Message::SetMediaCalendarMonth(month) => {
                self.media_filter.calendar.selected_month = month;
                self.refresh_sidebar_media()
            }
            Message::ToggleMediaTagFilter(tag) => {
                if let Some(pos) = self.media_filter.tag_filter.iter().position(|t| *t == tag) {
                    self.media_filter.tag_filter.remove(pos);
                } else {
                    self.media_filter.tag_filter.push(tag);
                }
                self.refresh_sidebar_media()
            }
            Message::ClearMediaFilters => {
                self.media_filter.clear();
                self.refresh_sidebar_media()
            }

            // ── Async sidebar data ──
            Message::SidebarPostsLoaded(mut posts) => {
                self.sidebar_posts_has_more = posts.len() > Self::SIDEBAR_PAGE_SIZE as usize;
                posts.truncate(Self::SIDEBAR_PAGE_SIZE as usize);
                self.sidebar_posts = posts;
                Task::none()
            }
            Message::SidebarMediaLoaded { mut items, thumbs } => {
                self.sidebar_media_has_more = items.len() > Self::SIDEBAR_PAGE_SIZE as usize;
                items.truncate(Self::SIDEBAR_PAGE_SIZE as usize);
                self.sidebar_media = items;
                self.sidebar_media_thumbs = thumbs;
                Task::none()
            }
            Message::SidebarPostsAppended(mut posts) => {
                self.sidebar_posts_has_more = posts.len() > Self::SIDEBAR_PAGE_SIZE as usize;
                posts.truncate(Self::SIDEBAR_PAGE_SIZE as usize);
                self.sidebar_posts.extend(posts);
                Task::none()
            }
            Message::SidebarMediaAppended { mut items, thumbs } => {
                self.sidebar_media_has_more = items.len() > Self::SIDEBAR_PAGE_SIZE as usize;
                items.truncate(Self::SIDEBAR_PAGE_SIZE as usize);
                self.sidebar_media.extend(items);
                self.sidebar_media_thumbs.extend(thumbs);
                Task::none()
            }
            Message::LoadMorePosts => self.load_more_sidebar_posts(),
            Message::LoadMoreMedia => self.load_more_sidebar_media(),

            // ── Modal ──
            Message::ShowModal(state) => {
                self.active_modal = Some(state);
                Task::none()
            }
            Message::DismissModal => {
                self.active_modal = None;
                Task::none()
            }
            Message::ConfirmModal(action) => {
                self.active_modal = None;
                match action {
                    modal::ConfirmAction::DeleteProject(id) => {
                        Task::done(Message::DeleteProject(id))
                    }
                    modal::ConfirmAction::DeletePost(id) => self.delete_post_editor(&id),
                    modal::ConfirmAction::DeleteMedia(id) => self.delete_media_editor(&id),
                    modal::ConfirmAction::DeleteScript(id) => self.delete_script_editor(&id),
                    modal::ConfirmAction::DeleteTemplate(id) => {
                        self.delete_template_editor(&id, false)
                    }
                    modal::ConfirmAction::ForceDeleteTemplate(id) => {
                        self.delete_template_editor(&id, true)
                    }
                    modal::ConfirmAction::DeleteTag(id) => self.delete_tag(&id),
                    modal::ConfirmAction::MergeTags { sources, target } => {
                        self.merge_tags(&sources, &target)
                    }
                }
            }
            Message::ToggleAiSuggestionField(index, accepted) => {
                if let Some(modal::ModalState::AISuggestions { fields, .. }) =
                    self.active_modal.as_mut()
                    && let Some(field) = fields.get_mut(index)
                    && !field.locked
                {
                    field.accepted = accepted;
                }
                Task::none()
            }
            Message::ApplyAiSuggestions(target, fields) => {
                self.active_modal = None;
                self.apply_ai_suggestions(target, &fields)
            }

            // ── Editor view messages ──
            Message::PostEditor(msg) => {
                enum DeferredPostAction {
                    None,
                    SyncEmbeddedPreview,
                    Analyze(String),
                    AnalyzeTaxonomy(String),
                    DetectLanguage(String),
                    OpenTranslate(String),
                    TranslateTo {
                        post_id: String,
                        target_language: String,
                    },
                    Save(String),
                    Publish(String),
                    Duplicate(String),
                    Discard(String),
                    ShowDelete {
                        tab_id: String,
                        name: String,
                    },
                    OpenInsertLink(String),
                    OpenInsertMedia {
                        post_id: String,
                        link_only: bool,
                    },
                    OpenGallery(String),
                    OpenLinkedMedia(String),
                    UnlinkLinkedMedia {
                        post_id: String,
                        media_id: String,
                    },
                    InsertSelectedLink {
                        post_id: String,
                        linked_post_id: String,
                    },
                    CreateLinkedPost(String),
                    InsertSelectedMedia {
                        post_id: String,
                        media_id: String,
                    },
                    SetLinkTab(modal::PostInsertLinkTab),
                    SetLinkSearch(String),
                    SetExternalUrl(String),
                    SetExternalText(String),
                    InsertExternalLink,
                    SetMediaSearch(String),
                    SelectGalleryImage(usize),
                    GalleryPrevious,
                    GalleryNext,
                    GalleryCloseLightbox,
                }

                let mut deferred = DeferredPostAction::None;
                let mut refresh_linked_media: Option<(String, String)> = None;
                if let Some(tab_id) = self.active_tab.clone()
                    && let Some(state) = self.post_editors.get_mut(&tab_id)
                {
                    match msg {
                        PostEditorMsg::ToggleQuickActions => {
                            state.quick_actions_open = !state.quick_actions_open;
                        }
                        PostEditorMsg::AnalyzeWithAi => {
                            state.quick_actions_open = false;
                            deferred = DeferredPostAction::Analyze(tab_id.clone());
                        }
                        PostEditorMsg::AnalyzeTaxonomy => {
                            state.quick_actions_open = false;
                            deferred = DeferredPostAction::AnalyzeTaxonomy(tab_id.clone());
                        }
                        PostEditorMsg::SwitchEditorMode(mode) => {
                            state.set_editor_mode(&mode);
                            deferred = DeferredPostAction::SyncEmbeddedPreview;
                        }
                        PostEditorMsg::DetectLanguage => {
                            state.quick_actions_open = false;
                            deferred = DeferredPostAction::DetectLanguage(tab_id.clone());
                        }
                        PostEditorMsg::Translate => {
                            state.quick_actions_open = false;
                            deferred = DeferredPostAction::OpenTranslate(tab_id.clone());
                        }
                        PostEditorMsg::TranslateTo(target_language) => {
                            deferred = DeferredPostAction::TranslateTo {
                                post_id: tab_id.clone(),
                                target_language,
                            };
                        }
                        PostEditorMsg::TitleChanged(s) => {
                            state.title = s;
                            state.mark_dirty();
                        }
                        PostEditorMsg::SlugChanged(s) => {
                            state.slug = s;
                            state.mark_dirty();
                        }
                        PostEditorMsg::ExcerptChanged(s) => {
                            state.excerpt = s;
                            state.mark_dirty();
                        }
                        PostEditorMsg::ContentChanged(new_text) => {
                            state.content = new_text;
                            state.mark_dirty();
                            refresh_linked_media =
                                Some((state.post_id.clone(), state.content.clone()));
                        }
                        PostEditorMsg::AuthorChanged(s) => {
                            state.author = s;
                            state.mark_dirty();
                        }
                        PostEditorMsg::LanguageChanged(s) => {
                            state.language = s;
                            state.mark_dirty();
                        }
                        PostEditorMsg::TemplateSlugChanged(s) => {
                            state.template_slug = s;
                            state.mark_dirty();
                        }
                        PostEditorMsg::ToggleDoNotTranslate(b) => {
                            state.do_not_translate = b;
                            state.mark_dirty();
                        }
                        PostEditorMsg::ToggleMetadata => {
                            state.metadata_expanded = !state.metadata_expanded;
                        }
                        PostEditorMsg::ToggleExcerpt => {
                            state.excerpt_expanded = !state.excerpt_expanded;
                        }
                        PostEditorMsg::SwitchLanguage(lang) => {
                            state.switch_language(&lang);
                            if state.editor_mode == "preview" {
                                deferred = DeferredPostAction::SyncEmbeddedPreview;
                            }
                        }
                        PostEditorMsg::TagsInputChanged(s) => {
                            state.tags_input = s;
                        }
                        PostEditorMsg::TagsInputSubmit => {
                            let tag = state.tags_input.trim().to_string();
                            if !tag.is_empty() && !state.tags.contains(&tag) {
                                state.tags.push(tag);
                                state.mark_dirty();
                            }
                            state.tags_input.clear();
                        }
                        PostEditorMsg::RemoveTag(tag) => {
                            state.tags.retain(|t| t != &tag);
                            state.mark_dirty();
                        }
                        PostEditorMsg::CategoriesInputChanged(s) => {
                            state.categories_input = s;
                        }
                        PostEditorMsg::CategoriesInputSubmit => {
                            let cat = state.categories_input.trim().to_string();
                            if !cat.is_empty() && !state.categories.contains(&cat) {
                                state.categories.push(cat);
                                state.mark_dirty();
                            }
                            state.categories_input.clear();
                        }
                        PostEditorMsg::RemoveCategory(cat) => {
                            state.categories.retain(|c| c != &cat);
                            state.mark_dirty();
                        }
                        PostEditorMsg::Save => {
                            deferred = DeferredPostAction::Save(tab_id.clone());
                        }
                        PostEditorMsg::Publish => {
                            deferred = DeferredPostAction::Publish(tab_id.clone());
                        }
                        PostEditorMsg::Duplicate => {
                            deferred = DeferredPostAction::Duplicate(tab_id.clone());
                        }
                        PostEditorMsg::Discard => {
                            deferred = DeferredPostAction::Discard(tab_id.clone());
                        }
                        PostEditorMsg::Delete => {
                            deferred = DeferredPostAction::ShowDelete {
                                tab_id: tab_id.clone(),
                                name: state.title.clone(),
                            };
                        }
                        PostEditorMsg::InsertLink => {
                            deferred = DeferredPostAction::OpenInsertLink(state.post_id.clone());
                        }
                        PostEditorMsg::InsertMedia => {
                            deferred = DeferredPostAction::OpenInsertMedia {
                                post_id: state.post_id.clone(),
                                link_only: false,
                            };
                        }
                        PostEditorMsg::Gallery => {
                            deferred = DeferredPostAction::OpenGallery(state.post_id.clone());
                        }
                        PostEditorMsg::LinkExistingMedia => {
                            deferred = DeferredPostAction::OpenInsertMedia {
                                post_id: state.post_id.clone(),
                                link_only: true,
                            };
                        }
                        PostEditorMsg::OpenLinkedMedia(media_id) => {
                            deferred = DeferredPostAction::OpenLinkedMedia(media_id);
                        }
                        PostEditorMsg::UnlinkLinkedMedia(media_id) => {
                            deferred = DeferredPostAction::UnlinkLinkedMedia {
                                post_id: state.post_id.clone(),
                                media_id,
                            };
                        }
                        PostEditorMsg::PostInsertLinkSelected(linked_post_id) => {
                            deferred = DeferredPostAction::InsertSelectedLink {
                                post_id: state.post_id.clone(),
                                linked_post_id,
                            };
                        }
                        PostEditorMsg::PostInsertLinkCreate => {
                            deferred = DeferredPostAction::CreateLinkedPost(state.post_id.clone());
                        }
                        PostEditorMsg::PostInsertMediaSelected(media_id) => {
                            deferred = DeferredPostAction::InsertSelectedMedia {
                                post_id: state.post_id.clone(),
                                media_id,
                            };
                        }
                        PostEditorMsg::PostGalleryImageSelected(index) => {
                            deferred = DeferredPostAction::SelectGalleryImage(index);
                        }
                        PostEditorMsg::PostInsertLinkTabSwitch(tab) => {
                            deferred = DeferredPostAction::SetLinkTab(tab);
                        }
                        PostEditorMsg::PostInsertLinkSearch(query) => {
                            deferred = DeferredPostAction::SetLinkSearch(query);
                        }
                        PostEditorMsg::PostInsertLinkUrlChanged(url) => {
                            deferred = DeferredPostAction::SetExternalUrl(url);
                        }
                        PostEditorMsg::PostInsertLinkTextChanged(text) => {
                            deferred = DeferredPostAction::SetExternalText(text);
                        }
                        PostEditorMsg::PostInsertLinkExternalInsert => {
                            deferred = DeferredPostAction::InsertExternalLink;
                        }
                        PostEditorMsg::PostInsertMediaSearch(query) => {
                            deferred = DeferredPostAction::SetMediaSearch(query);
                        }
                        PostEditorMsg::PostGalleryPrevious => {
                            deferred = DeferredPostAction::GalleryPrevious;
                        }
                        PostEditorMsg::PostGalleryNext => {
                            deferred = DeferredPostAction::GalleryNext;
                        }
                        PostEditorMsg::PostGalleryCloseLightbox => {
                            deferred = DeferredPostAction::GalleryCloseLightbox;
                        }
                    }

                    if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == state.post_id) {
                        tab.is_dirty = state.is_dirty;
                    }
                }

                if let Some((post_id, content)) = refresh_linked_media {
                    let linked_media = self.load_post_media_items(&post_id, Some(&content));
                    if let Some(state) = self.post_editors.get_mut(&post_id) {
                        state.linked_media = linked_media;
                    }
                }

                match deferred {
                    DeferredPostAction::None => Task::none(),
                    DeferredPostAction::SyncEmbeddedPreview => {
                        self.sync_embedded_preview_for_active_post()
                    }
                    DeferredPostAction::Analyze(tab_id) => self.run_post_ai_analysis(&tab_id),
                    DeferredPostAction::AnalyzeTaxonomy(tab_id) => {
                        self.run_post_taxonomy_analysis(&tab_id)
                    }
                    DeferredPostAction::DetectLanguage(tab_id) => {
                        self.detect_post_language(&tab_id)
                    }
                    DeferredPostAction::OpenTranslate(tab_id) => {
                        self.open_post_translation_modal(&tab_id)
                    }
                    DeferredPostAction::TranslateTo {
                        post_id,
                        target_language,
                    } => self.translate_post_to(&post_id, &target_language),
                    DeferredPostAction::Save(tab_id) => self.save_post_editor(&tab_id),
                    DeferredPostAction::Publish(tab_id) => self.publish_post_editor(&tab_id),
                    DeferredPostAction::Duplicate(tab_id) => self.duplicate_post_editor(&tab_id),
                    DeferredPostAction::Discard(tab_id) => self.discard_post_editor(&tab_id),
                    DeferredPostAction::ShowDelete { tab_id, name } => {
                        Task::done(Message::ShowModal(modal::ModalState::ConfirmDelete {
                            entity_name: name,
                            references: Vec::new(),
                            on_confirm: modal::ConfirmAction::DeletePost(tab_id),
                        }))
                    }
                    DeferredPostAction::OpenInsertLink(post_id) => self.insert_link_modal(&post_id),
                    DeferredPostAction::OpenInsertMedia { post_id, link_only } => {
                        self.insert_media_modal(&post_id, link_only)
                    }
                    DeferredPostAction::OpenGallery(post_id) => self.post_gallery(&post_id),
                    DeferredPostAction::OpenLinkedMedia(media_id) => {
                        let title = self
                            .db
                            .as_ref()
                            .and_then(|db| {
                                bds_core::db::queries::media::get_media_by_id(db.conn(), &media_id)
                                    .ok()
                                    .map(|media| media.title.unwrap_or(media.original_name))
                            })
                            .unwrap_or_else(|| media_id.clone());
                        Task::done(Message::OpenTab(Tab {
                            id: media_id,
                            tab_type: TabType::Media,
                            title,
                            is_transient: false,
                            is_dirty: false,
                        }))
                    }
                    DeferredPostAction::UnlinkLinkedMedia { post_id, media_id } => {
                        if let (Some(db), Some(data_dir)) = (&self.db, &self.data_dir) {
                            if let Err(err) = engine::post_media::unlink_media_from_post(
                                db.conn(),
                                data_dir,
                                &post_id,
                                &media_id,
                            ) {
                                self.notify_operation_failed("editor.unlinkMedia", err);
                                return Task::none();
                            }
                            self.refresh_post_relationships(&post_id);
                        }
                        Task::none()
                    }
                    DeferredPostAction::InsertSelectedLink {
                        post_id,
                        linked_post_id,
                    } => self.insert_selected_post_link(&post_id, &linked_post_id),
                    DeferredPostAction::CreateLinkedPost(post_id) => {
                        self.insert_created_post_link(&post_id)
                    }
                    DeferredPostAction::InsertSelectedMedia { post_id, media_id } => {
                        self.insert_selected_media(&post_id, &media_id)
                    }
                    DeferredPostAction::SetLinkTab(tab) => {
                        self.refresh_post_insert_link_modal(Some(tab), None, None, None);
                        Task::none()
                    }
                    DeferredPostAction::SetLinkSearch(query) => {
                        self.refresh_post_insert_link_modal(None, Some(query), None, None);
                        Task::none()
                    }
                    DeferredPostAction::SetExternalUrl(url) => {
                        self.refresh_post_insert_link_modal(None, None, Some(url), None);
                        Task::none()
                    }
                    DeferredPostAction::SetExternalText(text) => {
                        self.refresh_post_insert_link_modal(None, None, None, Some(text));
                        Task::none()
                    }
                    DeferredPostAction::InsertExternalLink => {
                        if let Some(modal::ModalState::PostInsertLink {
                            post_id,
                            external_url,
                            external_text,
                            ..
                        }) = self.active_modal.clone()
                        {
                            if let Some(markdown) =
                                modal::external_link_markdown(&external_url, &external_text)
                            {
                                self.insert_markdown_into_post(&post_id, &markdown)
                            } else {
                                self.notify(
                                    ToastLevel::Error,
                                    &t(self.ui_locale, "modal.postInsertLink.urlRequired"),
                                );
                                Task::none()
                            }
                        } else {
                            Task::none()
                        }
                    }
                    DeferredPostAction::SetMediaSearch(query) => {
                        self.refresh_insert_media_modal(query);
                        Task::none()
                    }
                    DeferredPostAction::SelectGalleryImage(index) => {
                        self.update_gallery_selection(Some(index));
                        Task::none()
                    }
                    DeferredPostAction::GalleryPrevious => {
                        self.step_gallery_selection(-1);
                        Task::none()
                    }
                    DeferredPostAction::GalleryNext => {
                        self.step_gallery_selection(1);
                        Task::none()
                    }
                    DeferredPostAction::GalleryCloseLightbox => {
                        self.update_gallery_selection(None);
                        Task::none()
                    }
                }
            }
            Message::MediaEditor(msg) => {
                enum DeferredMediaAction {
                    None,
                    Analyze(String),
                    DetectLanguage(String),
                    OpenTranslate(String),
                    TranslateTo {
                        media_id: String,
                        target_language: String,
                    },
                    LinkPost {
                        media_id: String,
                        post_id: String,
                    },
                    OpenLinkedPost(String),
                    UnlinkPost {
                        media_id: String,
                        post_id: String,
                    },
                    Save(String),
                    Delete {
                        tab_id: String,
                        name: String,
                    },
                }

                let mut deferred = DeferredMediaAction::None;
                let mut picker_refresh: Option<(String, String)> = None;
                if let Some(tab_id) = self.active_tab.clone()
                    && let Some(state) = self.media_editors.get_mut(&tab_id)
                {
                    match msg {
                        MediaEditorMsg::AnalyzeWithAi => {
                            deferred = DeferredMediaAction::Analyze(tab_id.clone());
                        }
                        MediaEditorMsg::DetectLanguage => {
                            deferred = DeferredMediaAction::DetectLanguage(tab_id.clone());
                        }
                        MediaEditorMsg::TranslateMetadata => {
                            deferred = DeferredMediaAction::OpenTranslate(tab_id.clone());
                        }
                        MediaEditorMsg::TranslateTo(target_language) => {
                            deferred = DeferredMediaAction::TranslateTo {
                                media_id: tab_id.clone(),
                                target_language,
                            };
                        }
                        MediaEditorMsg::TitleChanged(s) => {
                            state.title = s;
                            state.is_dirty = true;
                        }
                        MediaEditorMsg::AltChanged(s) => {
                            state.alt = s;
                            state.is_dirty = true;
                        }
                        MediaEditorMsg::CaptionChanged(s) => {
                            state.caption = s;
                            state.is_dirty = true;
                        }
                        MediaEditorMsg::AuthorChanged(s) => {
                            state.author = s;
                            state.is_dirty = true;
                        }
                        MediaEditorMsg::LanguageChanged(s) => {
                            state.language = s;
                            state.is_dirty = true;
                        }
                        MediaEditorMsg::TagsChanged(s) => {
                            state.tags_input = s;
                            state.is_dirty = true;
                        }
                        MediaEditorMsg::SwitchLanguage(lang) => {
                            state.switch_language(&lang);
                        }
                        MediaEditorMsg::TogglePostPicker => {
                            let next_open = !state.post_picker_open;
                            state.post_picker_open = next_open;
                            if !next_open {
                                state.post_picker_results.clear();
                            } else {
                                picker_refresh = Some((
                                    state.media_id.clone(),
                                    state.post_picker_search.clone(),
                                ));
                            }
                        }
                        MediaEditorMsg::PostPickerSearchChanged(search) => {
                            state.post_picker_search = search;
                            if state.post_picker_open {
                                picker_refresh = Some((
                                    state.media_id.clone(),
                                    state.post_picker_search.clone(),
                                ));
                            }
                        }
                        MediaEditorMsg::LinkPost(post_id) => {
                            deferred = DeferredMediaAction::LinkPost {
                                media_id: state.media_id.clone(),
                                post_id,
                            };
                        }
                        MediaEditorMsg::OpenLinkedPost(post_id) => {
                            deferred = DeferredMediaAction::OpenLinkedPost(post_id);
                        }
                        MediaEditorMsg::UnlinkPost(post_id) => {
                            deferred = DeferredMediaAction::UnlinkPost {
                                media_id: state.media_id.clone(),
                                post_id,
                            };
                        }
                        MediaEditorMsg::Save => {
                            deferred = DeferredMediaAction::Save(tab_id.clone());
                        }
                        MediaEditorMsg::Delete => {
                            deferred = DeferredMediaAction::Delete {
                                tab_id: tab_id.clone(),
                                name: state.title.clone(),
                            };
                        }
                    }
                    if let Some(tab) = self
                        .tabs
                        .iter_mut()
                        .find(|t| t.id == *state.media_id.as_str())
                    {
                        tab.is_dirty = state.is_dirty;
                    }
                }
                if let Some((media_id, search)) = picker_refresh {
                    let results = self.query_media_post_picker_results(&media_id, &search);
                    if let Some(state) = self.media_editors.get_mut(&media_id) {
                        state.post_picker_results = results;
                    }
                }
                match deferred {
                    DeferredMediaAction::None => Task::none(),
                    DeferredMediaAction::Analyze(media_id) => self.run_media_ai_analysis(&media_id),
                    DeferredMediaAction::DetectLanguage(media_id) => {
                        self.detect_media_language(&media_id)
                    }
                    DeferredMediaAction::OpenTranslate(media_id) => {
                        self.open_media_translation_modal(&media_id)
                    }
                    DeferredMediaAction::TranslateTo {
                        media_id,
                        target_language,
                    } => self.translate_media_to(&media_id, &target_language),
                    DeferredMediaAction::LinkPost { media_id, post_id } => {
                        self.link_media_to_post(&media_id, &post_id)
                    }
                    DeferredMediaAction::OpenLinkedPost(post_id) => {
                        let title = self
                            .db
                            .as_ref()
                            .and_then(|db| {
                                bds_core::db::queries::post::get_post_by_id(db.conn(), &post_id)
                                    .ok()
                            })
                            .map(|post| post.title)
                            .unwrap_or_else(|| post_id.clone());
                        Task::done(Message::OpenTab(Tab {
                            id: post_id,
                            tab_type: TabType::Post,
                            title,
                            is_transient: false,
                            is_dirty: false,
                        }))
                    }
                    DeferredMediaAction::UnlinkPost { media_id, post_id } => {
                        self.unlink_media_from_post(&media_id, &post_id)
                    }
                    DeferredMediaAction::Save(tab_id) => self.save_media_editor(&tab_id),
                    DeferredMediaAction::Delete { tab_id, name } => {
                        Task::done(Message::ShowModal(modal::ModalState::ConfirmDelete {
                            entity_name: name,
                            references: Vec::new(),
                            on_confirm: modal::ConfirmAction::DeleteMedia(tab_id),
                        }))
                    }
                }
            }
            Message::TemplateEditor(msg) => {
                if let Some(tab_id) = self.active_tab.clone()
                    && let Some(state) = self.template_editors.get_mut(&tab_id)
                {
                    match msg {
                        TemplateEditorMsg::TitleChanged(s) => {
                            state.title = s;
                            state.is_dirty = true;
                        }
                        TemplateEditorMsg::SlugChanged(s) => {
                            state.slug = s;
                            state.is_dirty = true;
                        }
                        TemplateEditorMsg::KindChanged(k) => {
                            state.kind = k.0;
                            state.is_dirty = true;
                        }
                        TemplateEditorMsg::EnabledChanged(b) => {
                            state.enabled = b;
                            state.is_dirty = true;
                        }
                        TemplateEditorMsg::ContentChanged(new_text) => {
                            state.content = new_text;
                            state.is_dirty = true;
                        }
                        TemplateEditorMsg::Save => {
                            return self.save_template_editor(&tab_id);
                        }
                        TemplateEditorMsg::Validate => {
                            if let Some(st) = self.template_editors.get_mut(&tab_id) {
                                match engine::template::validate_template(&st.content) {
                                    Ok(()) => {
                                        st.validation_error = None;
                                    }
                                    Err(e) => {
                                        st.validation_error = Some(e);
                                    }
                                }
                            }
                        }
                        TemplateEditorMsg::Delete => {
                            return self.show_template_delete_confirmation(&tab_id);
                        }
                    }
                    if let Some(st) = self.template_editors.get(&tab_id)
                        && let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id)
                    {
                        tab.is_dirty = st.is_dirty;
                    }
                }
                Task::none()
            }
            Message::ScriptEditor(msg) => {
                if let Some(tab_id) = self.active_tab.clone()
                    && let Some(state) = self.script_editors.get_mut(&tab_id)
                {
                    match msg {
                        ScriptEditorMsg::TitleChanged(s) => {
                            state.title = s;
                            state.is_dirty = true;
                        }
                        ScriptEditorMsg::SlugChanged(s) => {
                            state.slug = s;
                            state.is_dirty = true;
                        }
                        ScriptEditorMsg::KindChanged(k) => {
                            state.kind = k.0;
                            state.is_dirty = true;
                        }
                        ScriptEditorMsg::EntrypointChanged(s) => {
                            state.entrypoint = s;
                            state.is_dirty = true;
                        }
                        ScriptEditorMsg::EnabledChanged(b) => {
                            state.enabled = b;
                            state.is_dirty = true;
                        }
                        ScriptEditorMsg::ContentChanged(new_text) => {
                            state.discovered_entrypoints =
                                engine::script::discover_entrypoints(&new_text);
                            state.content = new_text;
                            state.is_dirty = true;
                        }
                        ScriptEditorMsg::Save => {
                            return self.save_script_editor(&tab_id);
                        }
                        ScriptEditorMsg::CheckSyntax => {
                            if let Some(st) = self.script_editors.get_mut(&tab_id) {
                                match engine::script::validate_script_syntax(&st.content) {
                                    Ok(()) => {
                                        st.validation_error = None;
                                    }
                                    Err(e) => {
                                        st.validation_error = Some(e);
                                    }
                                }
                            }
                        }
                        ScriptEditorMsg::Run => {
                            self.notify(
                                ToastLevel::Info,
                                &t(self.ui_locale, "editor.scriptRunNotYet"),
                            );
                        }
                        ScriptEditorMsg::Delete => {
                            let name = state.title.clone();
                            return Task::done(Message::ShowModal(
                                modal::ModalState::ConfirmDelete {
                                    entity_name: name,
                                    references: Vec::new(),
                                    on_confirm: modal::ConfirmAction::DeleteScript(tab_id),
                                },
                            ));
                        }
                    }
                    if let Some(st) = self.script_editors.get(&tab_id)
                        && let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id)
                    {
                        tab.is_dirty = st.is_dirty;
                    }
                }
                Task::none()
            }
            Message::Tags(msg) => self.handle_tags_msg(msg),
            Message::Settings(msg) => self.handle_settings_msg(msg),

            // ── Editor data loading ──
            Message::PostLoaded(result) => {
                match result {
                    Ok(post) => {
                        let mut translations = self.db.as_ref()
                            .and_then(|db| bds_core::db::queries::post_translation::list_post_translations_by_post(
                                db.conn(), &post.id,
                            ).ok())
                            .unwrap_or_default();
                        // Published translations don't store body in DB — read from file
                        if let Some(ref data_dir) = self.data_dir {
                            for tr in &mut translations {
                                if tr.content.is_none() {
                                    let rel = bds_core::util::paths::translation_file_path(
                                        post.created_at,
                                        &post.slug,
                                        &tr.language,
                                    );
                                    let path = data_dir.join(&rel);
                                    if let Ok(raw) = std::fs::read_to_string(&path)
                                        && let Ok((_fm, body)) =
                                            bds_core::util::frontmatter::read_translation_file(&raw)
                                    {
                                        tr.content = Some(body);
                                    }
                                }
                            }
                        }
                        let (outlinks, backlinks) = self.load_post_links(&post.id);
                        let linked_media =
                            self.load_post_media_items(&post.id, post.content.as_deref());
                        let state = PostEditorState::from_post(
                            &post,
                            default_post_editor_mode(self.settings_state.as_ref()),
                            &self.blog_languages,
                            &translations,
                            outlinks,
                            backlinks,
                            linked_media,
                        );
                        self.post_editors.insert(post.id.clone(), state);
                    }
                    Err(e) => self.notify(ToastLevel::Error, &e),
                }
                Task::none()
            }
            Message::MediaLoaded(result) => {
                match result {
                    Ok(media) => {
                        let translations = self.db.as_ref()
                            .and_then(|db| bds_core::db::queries::media_translation::list_media_translations_by_media(
                                db.conn(), &media.id,
                            ).ok())
                            .unwrap_or_default();
                        let linked_posts = self.load_media_linked_posts(&media.id);
                        let state = MediaEditorState::from_media(
                            &media,
                            &self.blog_languages,
                            &translations,
                            linked_posts,
                        );
                        self.media_editors.insert(media.id.clone(), state);
                    }
                    Err(e) => self.notify(ToastLevel::Error, &e),
                }
                Task::none()
            }
            Message::TemplateLoaded(result) => {
                match result {
                    Ok(template) => {
                        let state = TemplateEditorState::from_template(&template);
                        self.template_editors.insert(template.id.clone(), state);
                    }
                    Err(e) => self.notify(ToastLevel::Error, &e),
                }
                Task::none()
            }
            Message::ScriptLoaded(result) => {
                match result {
                    Ok(script) => {
                        let state = ScriptEditorState::from_script(&script);
                        self.script_editors.insert(script.id.clone(), state);
                    }
                    Err(e) => self.notify(ToastLevel::Error, &e),
                }
                Task::none()
            }

            Message::Noop => Task::none(),
            Message::InitMenuBar => {
                #[cfg(target_os = "macos")]
                if let Some(menu_bar) = self._menu_bar.as_ref() {
                    menu::init_menu_for_nsapp(menu_bar);
                }
                Task::none()
            }
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let active_name = self.active_project.as_ref().map(|p| p.name.as_str());
        let active_post_filter = match self.sidebar_view {
            SidebarView::Pages => &self.page_filter,
            _ => &self.post_filter,
        };
        let post_preview_widget = if self.active_post_uses_embedded_preview() {
            self.embedded_preview
                .as_ref()
                .map(|preview| webview::webview(&preview.controller).into())
        } else {
            None
        };

        workspace::view(
            self.sidebar_view,
            self.sidebar_visible,
            self.sidebar_width,
            &self.tabs,
            self.active_tab.as_deref(),
            self.panel_visible,
            self.panel_tab,
            &self.task_snapshots,
            &self.output_entries,
            &self.sidebar_posts,
            &self.sidebar_media,
            &self.sidebar_scripts,
            &self.sidebar_templates,
            active_post_filter,
            &self.media_filter,
            &self.sidebar_media_thumbs,
            self.sidebar_posts_has_more,
            self.sidebar_media_has_more,
            active_name,
            &self.projects,
            self.active_project.as_ref().map(|p| p.id.as_str()),
            self.post_count,
            self.media_count,
            self.offline_mode,
            self.locale_dropdown_open,
            self.project_dropdown_open,
            &self.theme_badge,
            self.ui_locale,
            &self.toasts,
            self.active_modal.clone(),
            self.data_dir.as_deref(),
            post_preview_widget,
            &self.post_editors,
            &self.media_editors,
            &self.template_editors,
            &self.script_editors,
            self.tags_view_state.as_ref(),
            self.settings_state.as_ref(),
            self.dashboard_state.as_ref(),
            &self.site_validation_state,
        )
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let menu_sub = menu::menu_subscription();

        let task_tick =
            iced::time::every(std::time::Duration::from_millis(500)).map(|_| Message::TaskTick);

        let toast_tick = if !self.toasts.is_empty() {
            iced::time::every(std::time::Duration::from_millis(250)).map(|_| Message::ExpireToasts)
        } else {
            Subscription::none()
        };

        // Global mouse tracking for sidebar resize dragging.
        // The 4px drag handle mouse_area only fires on_press; move/release
        // are captured here so dragging works even when the cursor leaves
        // the narrow handle strip.
        let drag_sub = if self.sidebar_dragging {
            iced::event::listen_with(|event, _status, _id| match event {
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::SidebarResizeMove(position.x))
                }
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )) => Some(Message::SidebarResizeEnd),
                _ => None,
            })
        } else {
            Subscription::none()
        };

        Subscription::batch([menu_sub, task_tick, toast_tick, drag_sub])
    }

    // ── Private helpers ──

    fn dispatch_menu_action(&mut self, action: MenuAction) -> Task<Message> {
        match action {
            // File
            MenuAction::NewPost => self.create_sidebar_post(false),
            MenuAction::ImportMedia => crate::platform::dialog::pick_media_files(
                t(self.ui_locale, "dialog.importMedia"),
                t(self.ui_locale, "dialog.imageFilter"),
            ),
            MenuAction::Save => Task::none(), // Disabled in M2
            MenuAction::OpenInBrowser => self.preview_active_post(),
            MenuAction::OpenDataFolder => {
                if let Some(ref dir) = self.data_dir {
                    let _ = open::that(dir);
                }
                Task::none()
            }
            // Edit
            MenuAction::Find => Task::none(),
            MenuAction::Replace => Task::none(),
            MenuAction::EditPreferences => {
                self.open_singleton_tab(TabType::Settings, "common.settings");
                Task::none()
            }
            // View
            MenuAction::ViewPosts => Task::done(Message::SetActiveView(SidebarView::Posts)),
            MenuAction::ViewMedia => Task::done(Message::SetActiveView(SidebarView::Media)),
            MenuAction::ToggleSidebar => Task::done(Message::ToggleSidebar),
            MenuAction::TogglePanel => Task::done(Message::TogglePanel),
            // Blog
            MenuAction::PublishSelected => Task::none(), // Disabled in M2
            MenuAction::PreviewPost => self.preview_active_post(),
            MenuAction::EditMenu => {
                self.open_singleton_tab(TabType::MenuEditor, "tabBar.menuEditor");
                Task::none()
            }
            MenuAction::RebuildDatabase => Task::done(Message::RebuildDatabase),
            MenuAction::ReindexText => Task::done(Message::ReindexText),
            MenuAction::MetadataDiff => Task::done(Message::RunMetadataDiff),
            MenuAction::RegenerateCalendar => Task::done(Message::RegenerateCalendar),
            MenuAction::ValidateTranslations => Task::done(Message::ValidateTranslations),
            MenuAction::FillMissingTranslations => {
                if self.offline_mode {
                    self.notify(
                        ToastLevel::Warning,
                        &t(self.ui_locale, "engine.fillMissingTranslationsOffline"),
                    );
                } else {
                    self.notify(
                        ToastLevel::Warning,
                        &t(self.ui_locale, "engine.fillMissingTranslationsNoAi"),
                    );
                }
                Task::none()
            }
            MenuAction::GenerateSitemap => Task::done(Message::GenerateSite),
            MenuAction::ValidateSite => {
                self.open_singleton_tab(TabType::SiteValidation, "tabBar.siteValidation");
                self.start_site_validation()
            }
            MenuAction::UploadSite => {
                if self.offline_mode {
                    self.notify(
                        ToastLevel::Warning,
                        &t(self.ui_locale, "engine.uploadOffline"),
                    );
                } else if let Some(data_dir) = &self.data_dir {
                    let pub_prefs = engine::meta::read_publishing_json(data_dir).ok();
                    let has_creds = pub_prefs
                        .as_ref()
                        .map(|p| {
                            p.ssh_host.as_ref().is_some_and(|h| !h.is_empty())
                                && p.ssh_user.as_ref().is_some_and(|u| !u.is_empty())
                        })
                        .unwrap_or(false);
                    if !has_creds {
                        self.notify(
                            ToastLevel::Warning,
                            &t(self.ui_locale, "engine.uploadMissingCredentials"),
                        );
                    } else {
                        self.notify(ToastLevel::Info, &t(self.ui_locale, "engine.uploadStarted"));
                    }
                }
                Task::none()
            }
            // Help
            MenuAction::About => {
                self.add_output(&t(self.ui_locale, "menu.item.about"));
                Task::none()
            }
            MenuAction::OpenDocumentation => {
                self.open_singleton_tab(TabType::Documentation, "tabBar.documentation");
                Task::none()
            }
            MenuAction::ViewOnGitHub => {
                let _ = open::that("https://github.com/nickarumern/bds");
                Task::none()
            }
            MenuAction::ReportIssue => {
                let _ = open::that("https://github.com/nickarumern/bds/issues");
                Task::none()
            }
        }
    }

    fn open_singleton_tab(&mut self, tab_type: TabType, i18n_key: &str) {
        let title = t(self.ui_locale, i18n_key);
        let tab = Tab {
            id: tab_type.singleton_id().to_string(),
            tab_type,
            title,
            is_transient: false,
            is_dirty: false,
        };
        let idx = tabs::open_tab(&mut self.tabs, tab);
        if let Some(t) = self.tabs.get(idx) {
            self.active_tab = Some(t.id.clone());
        }
        self.enforce_panel_tab_fallback();
    }

    fn create_sidebar_post(&mut self, is_page: bool) -> Task<Message> {
        let (Some(db), Some(project), Some(data_dir)) =
            (&self.db, &self.active_project, &self.data_dir)
        else {
            return Task::none();
        };

        let categories = if is_page {
            vec!["page".to_string()]
        } else {
            Vec::new()
        };

        match engine::post::create_post(
            db.conn(),
            data_dir,
            &project.id,
            "",
            Some(""),
            Vec::new(),
            categories,
            None,
            None,
            None,
        ) {
            Ok(post) => {
                let tab = Tab {
                    id: post.id.clone(),
                    tab_type: TabType::Post,
                    title: t(self.ui_locale, "post.untitled"),
                    is_transient: true,
                    is_dirty: false,
                };
                let idx = tabs::open_tab(&mut self.tabs, tab);
                if let Some(tab) = self.tabs.get(idx).cloned() {
                    self.active_tab = Some(tab.id.clone());
                    self.load_editor_for_tab(&tab);
                }
                self.sync_menu_state();
                self.refresh_counts()
            }
            Err(error) => {
                self.notify_operation_failed("modal.postInsertLink.createPost", error);
                Task::none()
            }
        }
    }

    fn create_sidebar_script(&mut self) -> Task<Message> {
        let (Some(db), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };

        match engine::script::create_script(
            db.conn(),
            &project.id,
            &t(self.ui_locale, "editor.untitled"),
            bds_core::model::ScriptKind::Utility,
            "print(\"new script\")",
            Some("render"),
        ) {
            Ok(script) => {
                let tab = Tab {
                    id: script.id.clone(),
                    tab_type: TabType::Scripts,
                    title: if script.title.is_empty() {
                        t(self.ui_locale, "editor.untitled")
                    } else {
                        script.title.clone()
                    },
                    is_transient: true,
                    is_dirty: false,
                };
                let idx = tabs::open_tab(&mut self.tabs, tab);
                if let Some(tab) = self.tabs.get(idx).cloned() {
                    self.active_tab = Some(tab.id.clone());
                    self.load_editor_for_tab(&tab);
                }
                self.sync_menu_state();
                self.refresh_counts()
            }
            Err(error) => {
                self.notify_operation_failed("common.createScript", error);
                Task::none()
            }
        }
    }

    fn create_sidebar_template(&mut self) -> Task<Message> {
        let (Some(db), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };

        match engine::template::create_template(
            db.conn(),
            &project.id,
            &t(self.ui_locale, "editor.untitled"),
            bds_core::model::TemplateKind::Post,
            "",
        ) {
            Ok(template) => {
                let tab = Tab {
                    id: template.id.clone(),
                    tab_type: TabType::Templates,
                    title: if template.title.is_empty() {
                        t(self.ui_locale, "editor.untitled")
                    } else {
                        template.title.clone()
                    },
                    is_transient: true,
                    is_dirty: false,
                };
                let idx = tabs::open_tab(&mut self.tabs, tab);
                if let Some(tab) = self.tabs.get(idx).cloned() {
                    self.active_tab = Some(tab.id.clone());
                    self.load_editor_for_tab(&tab);
                }
                self.sync_menu_state();
                self.refresh_counts()
            }
            Err(error) => {
                self.notify_operation_failed("common.createTemplate", error);
                Task::none()
            }
        }
    }

    fn refresh_task_snapshots(&mut self) {
        self.task_snapshots = self
            .task_manager
            .snapshots()
            .into_iter()
            .map(|snapshot| {
                let status_str = match &snapshot.status {
                    TaskStatus::Pending => "pending".to_string(),
                    TaskStatus::Running => "running".to_string(),
                    TaskStatus::Completed => "completed".to_string(),
                    TaskStatus::Failed(e) => format!("failed: {e}"),
                    TaskStatus::Cancelled => "cancelled".to_string(),
                };
                TaskSnapshot {
                    id: snapshot.id,
                    label: snapshot.label,
                    status: status_str,
                    progress: snapshot.progress,
                    message: snapshot.message,
                }
            })
            .collect();
    }

    fn flush_active_post_editor(&mut self) {
        let Some(active_id) = self.active_tab.clone() else {
            return;
        };
        let is_dirty_post = self
            .tabs
            .iter()
            .find(|tab| tab.id == active_id)
            .map(|tab| tab.tab_type == TabType::Post)
            .unwrap_or(false)
            && self
                .post_editors
                .get(&active_id)
                .map(|state| state.is_dirty)
                .unwrap_or(false);
        if is_dirty_post {
            let _ = self.persist_post_editor_state(&active_id);
        }
    }

    fn auto_save_due_post_editors(&mut self) {
        let now = bds_core::util::now_unix_ms();
        let due_ids: Vec<String> = self
            .post_editors
            .iter()
            .filter(|&(_post_id, state)| {
                state.is_dirty
                    && state.last_edit_at_ms > 0
                    && now - state.last_edit_at_ms >= POST_AUTO_SAVE_DELAY_MS
            })
            .map(|(post_id, _state)| post_id.clone())
            .collect();

        for post_id in due_ids {
            if let Err(error) = self.persist_post_editor_state(&post_id) {
                self.notify_operation_failed("common.autoSave", error);
            }
        }
    }

    fn operation_failed_text(&self, operation_key: &str, error: impl std::fmt::Display) -> String {
        let operation = t(self.ui_locale, operation_key);
        let error = error.to_string();
        tw(
            self.ui_locale,
            "common.operationFailed",
            &[("operation", &operation), ("error", &error)],
        )
    }

    fn notify_operation_failed(&mut self, operation_key: &str, error: impl std::fmt::Display) {
        let message = self.operation_failed_text(operation_key, error);
        self.notify(ToastLevel::Error, &message);
    }

    fn add_output(&mut self, text: &str) {
        self.output_entries.push(OutputEntry {
            timestamp: chrono::Utc::now().timestamp(),
            text: text.to_string(),
        });
    }

    /// Show a toast notification AND log to output panel.
    fn notify(&mut self, level: ToastLevel, text: &str) {
        self.toasts.push(Toast::new(level, text.to_string()));
        self.add_output(text);
    }

    fn start_site_validation(&mut self) -> Task<Message> {
        self.open_singleton_tab(TabType::SiteValidation, "tabBar.siteValidation");
        let Some(_db) = self.db.as_ref() else {
            self.site_validation_state.is_running = false;
            self.site_validation_state.error_message =
                Some(t(self.ui_locale, "engine.generateSiteNoProject"));
            return Task::none();
        };
        let Some(project_id) = self
            .active_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            self.site_validation_state.is_running = false;
            self.site_validation_state.error_message =
                Some(t(self.ui_locale, "engine.generateSiteNoProject"));
            return Task::none();
        };
        let Some(data_dir) = self.data_dir.clone() else {
            self.site_validation_state.is_running = false;
            self.site_validation_state.error_message =
                Some(t(self.ui_locale, "engine.previewDataDirUnavailable"));
            return Task::none();
        };

        self.site_validation_state.is_running = true;
        self.site_validation_state.error_message = None;
        let db_path = self.db_path.clone();

        Task::perform(
            async move {
                let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                engine::validate_site::validate_site(db.conn(), &data_dir, &project_id)
                    .map_err(|error| error.to_string())
            },
            Message::SiteValidationLoaded,
        )
    }

    fn apply_site_validation(&mut self) -> Task<Message> {
        if self.site_validation_state.is_running || self.site_validation_state.is_applying {
            return Task::none();
        }

        let report = engine::validate_site::SiteValidationReport {
            missing_pages: self.site_validation_state.missing_files.clone(),
            extra_pages: self.site_validation_state.extra_files.clone(),
            stale_pages: self.site_validation_state.stale_files.clone(),
        };
        let sections = engine::generation::sections_from_validation_report(&report);
        if sections.is_empty() {
            return Task::none();
        }

        let Some(project_id) = self
            .active_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            self.site_validation_state.error_message =
                Some(t(self.ui_locale, "engine.generateSiteNoProject"));
            return Task::none();
        };
        let Some(data_dir) = self.data_dir.clone() else {
            self.site_validation_state.error_message =
                Some(t(self.ui_locale, "engine.previewDataDirUnavailable"));
            return Task::none();
        };

        self.site_validation_state.is_applying = true;
        self.site_validation_state.error_message = None;
        let db_path = self.db_path.clone();
        let applied_label = t(self.ui_locale, "siteValidation.apply");

        Task::perform(
            async move {
                let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                let metadata = engine::meta::read_project_json(&data_dir)
                    .map_err(|error| error.to_string())?;
                let all_posts =
                    bds_core::db::queries::post::list_posts_by_project(db.conn(), &project_id)
                        .map_err(|error| error.to_string())?;
                let mut sources = Vec::new();
                for post in all_posts
                    .into_iter()
                    .filter(engine::generation::has_published_snapshot)
                {
                    if let Some(source) =
                        engine::generation::load_published_post_source(&data_dir, post)
                            .map_err(|error| error.to_string())?
                    {
                        sources.push(source);
                    }
                }
                let output_dir = data_dir.join("html");
                std::fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
                let apply_report = engine::generation::apply_validation_sections(
                    db.conn(),
                    &output_dir,
                    &project_id,
                    &metadata,
                    &sources,
                    &sections,
                )
                .map_err(|error| error.to_string())?;
                Ok(format!(
                    "{}: written={}, skipped={}, deleted={}, output={}",
                    applied_label,
                    apply_report.written_paths.len(),
                    apply_report.skipped_paths.len(),
                    apply_report.deleted_paths.len(),
                    output_dir.display(),
                ))
            },
            Message::SiteValidationApplied,
        )
    }

    /// Spawn a blocking engine operation on a background thread via TaskManager.
    ///
    /// Returns `Task::none()` if no active project/db/data_dir.
    /// Otherwise registers the task, logs the start message, and returns an
    /// async `Task` that opens a fresh DB connection on a worker thread.
    ///
    /// The closure receives `(db_path, project_id, data_dir, task_manager, task_id)`.
    /// Use `task_manager.report_progress(task_id, percent, message)` for live updates.
    fn spawn_engine_task<F>(&mut self, label_key: &str, work: F) -> Task<Message>
    where
        F: FnOnce(PathBuf, String, PathBuf, Arc<TaskManager>, TaskId) -> Result<String, String>
            + Send
            + 'static,
    {
        let (Some(project), Some(data_dir)) = (&self.active_project, &self.data_dir) else {
            return Task::none();
        };

        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let data_dir = data_dir.clone();

        let label = t(self.ui_locale, label_key);
        self.add_output(&label);

        let task_id = self.task_manager.submit(&label);
        self.refresh_task_snapshots();

        let label_for_msg = label.clone();
        let tm = Arc::clone(&self.task_manager);

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    work(db_path, project_id, data_dir, tm, task_id)
                })
                .await
                .unwrap_or_else(|e| Err(format!("task panicked: {e}")))
            },
            move |result| Message::EngineTaskDone {
                task_id,
                label: label_for_msg.clone(),
                result,
            },
        )
    }

    fn refresh_counts(&mut self) -> Task<Message> {
        if let (Some(db), Some(project)) = (&self.db, &self.active_project) {
            self.post_count =
                bds_core::db::queries::post::count_posts_by_project(db.conn(), &project.id)
                    .unwrap_or(0) as usize;
            self.media_count =
                bds_core::db::queries::media::count_media_by_project(db.conn(), &project.id)
                    .unwrap_or(0) as usize;

            self.sidebar_scripts =
                bds_core::db::queries::script::list_scripts_by_project(db.conn(), &project.id)
                    .unwrap_or_default();
            self.sidebar_templates =
                bds_core::db::queries::template::list_templates_by_project(db.conn(), &project.id)
                    .unwrap_or_default();

            // Read pico theme from project metadata for status bar badge
            if let Some(ref data_dir) = self.data_dir
                && let Ok(meta) = engine::meta::read_project_json(data_dir)
                && let Some(theme) = meta.pico_theme
            {
                self.theme_badge = theme;
            }

            self.dashboard_state = Some(self.hydrate_dashboard_state());
        }

        // Refresh sidebar data with current filters (async — off main thread)
        let t1 = self.refresh_sidebar_posts();
        let t2 = self.refresh_sidebar_media();
        self.refresh_filter_metadata();
        Task::batch([t1, t2])
    }

    fn hydrate_dashboard_state(&self) -> DashboardState {
        let Some(project) = &self.active_project else {
            return DashboardState::new(String::new());
        };
        let Some(db) = &self.db else {
            return DashboardState::new(project.name.clone());
        };

        let posts = bds_core::db::queries::post::list_posts_by_project(db.conn(), &project.id)
            .unwrap_or_default();
        let media = bds_core::db::queries::media::list_media_by_project(db.conn(), &project.id)
            .unwrap_or_default();
        let tags = bds_core::db::queries::tag::list_tags_by_project(db.conn(), &project.id)
            .unwrap_or_default();

        let mut draft_count = 0usize;
        let mut published_count = 0usize;
        let mut archived_count = 0usize;
        let mut monthly_counts: std::collections::BTreeMap<(i32, u32), usize> =
            std::collections::BTreeMap::new();
        let mut category_counts: HashMap<String, usize> = HashMap::new();
        let mut tag_counts: HashMap<String, usize> = HashMap::new();

        for post in &posts {
            match post.status {
                PostStatus::Draft => draft_count += 1,
                PostStatus::Published => published_count += 1,
                PostStatus::Archived => archived_count += 1,
            }

            let (year, month, _) =
                bds_core::util::timestamp::year_month_day_from_unix_ms(post.created_at);
            let year = year.parse::<i32>().unwrap_or(0);
            let month = month.parse::<u32>().unwrap_or(0);
            *monthly_counts.entry((year, month)).or_insert(0) += 1;

            for category in &post.categories {
                *category_counts.entry(category.clone()).or_insert(0) += 1;
            }
            for tag in &post.tags {
                *tag_counts.entry(tag.to_lowercase()).or_insert(0) += 1;
            }
        }

        let image_count = media
            .iter()
            .filter(|item| item.mime_type.starts_with("image/"))
            .count();
        let total_media_size = media.iter().map(|item| item.size).sum::<i64>();

        let now = chrono::Utc::now();
        let current_year = now.year();
        let current_month = now.month() as i32;
        let timeline = (0..12)
            .rev()
            .map(|offset| {
                let total_month_index = current_year * 12 + (current_month - 1) - offset;
                let year = total_month_index.div_euclid(12);
                let month = total_month_index.rem_euclid(12) + 1;
                let count = monthly_counts
                    .get(&(year, month as u32))
                    .copied()
                    .unwrap_or(0);

                DashboardTimelineMonth {
                    label: month_abbreviation(month as u32),
                    year,
                    count,
                }
            })
            .collect::<Vec<_>>();

        let mut tag_cloud = tags
            .into_iter()
            .map(|tag| DashboardTag {
                count: tag_counts
                    .get(&tag.name.to_lowercase())
                    .copied()
                    .unwrap_or(0),
                name: tag.name,
                color: tag.color,
            })
            .collect::<Vec<_>>();
        tag_cloud.sort_by_key(|left| left.name.to_lowercase());
        let tag_overflow_count = tag_cloud.len().saturating_sub(40);
        tag_cloud.truncate(40);

        let mut category_cloud = category_counts
            .into_iter()
            .map(|(name, count)| DashboardCategory { name, count })
            .collect::<Vec<_>>();
        category_cloud.sort_by_key(|left| left.name.to_lowercase());

        let mut sorted_posts = posts;
        sorted_posts.sort_by_key(|post| std::cmp::Reverse(post.updated_at));
        let mut recent_posts = sorted_posts
            .into_iter()
            .map(|post| DashboardRecentPost {
                post_id: post.id,
                title: if post.title.trim().is_empty() {
                    "Untitled".to_string()
                } else {
                    post.title
                },
                status: match post.status {
                    PostStatus::Published => "published".to_string(),
                    _ => "draft".to_string(),
                },
                date: format_timestamp(post.updated_at),
            })
            .collect::<Vec<_>>();
        recent_posts.truncate(5);

        DashboardState {
            title: t(self.ui_locale, "dashboard.overview"),
            subtitle: project.name.clone(),
            stats: DashboardStats {
                total_posts: draft_count + published_count + archived_count,
                published_count,
                draft_count,
                archived_count,
                media_count: media.len(),
                image_count,
                total_media_size: format_bytes(total_media_size),
                tag_count: tag_counts.len(),
                category_count: category_cloud.len(),
            },
            timeline,
            tag_cloud,
            tag_overflow_count,
            category_cloud,
            recent_posts,
        }
    }

    /// Number of items to load per sidebar page.
    /// Matches the TypeScript app's limit of 500 for initial load.
    const SIDEBAR_PAGE_SIZE: i64 = 500;

    /// Refresh only sidebar posts using current filter state (async).
    fn refresh_sidebar_posts(&mut self) -> Task<Message> {
        let (Some(_), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };

        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let content_language = self.content_language.clone();
        let filter = match self.sidebar_view {
            SidebarView::Pages => self.page_filter.clone(),
            _ => self.post_filter.clone(),
        };
        let is_pages = self.sidebar_view == SidebarView::Pages;

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    Self::query_sidebar_posts_blocking(
                        &db_path,
                        &project_id,
                        &content_language,
                        &filter,
                        is_pages,
                        Self::SIDEBAR_PAGE_SIZE + 1,
                        0,
                    )
                })
                .await
                .unwrap_or_default()
            },
            Message::SidebarPostsLoaded,
        )
    }

    /// Refresh only sidebar media using current filter state (async).
    fn refresh_sidebar_media(&mut self) -> Task<Message> {
        let (Some(_), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };

        use bds_core::db::queries::media::MediaFilterParams;

        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let data_dir = self.data_dir.clone();

        let params = MediaFilterParams {
            search_query: self.media_filter.search_query.clone(),
            year: self.media_filter.calendar.selected_year,
            month: self.media_filter.calendar.selected_month,
            tags: self.media_filter.tag_filter.clone(),
        };

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let db = Database::open(&db_path).ok();
                    let items = db
                        .and_then(|db| {
                            bds_core::db::queries::media::list_media_filtered(
                                db.conn(),
                                &project_id,
                                &params,
                                Self::SIDEBAR_PAGE_SIZE + 1,
                                0,
                            )
                            .ok()
                        })
                        .unwrap_or_default();

                    // Pre-resolve thumbnail paths off the main thread
                    let thumbs: HashMap<String, Option<std::path::PathBuf>> = items
                        .iter()
                        .map(|m| {
                            let thumb = data_dir.as_ref().and_then(|dir| {
                                if !m.mime_type.starts_with("image/") {
                                    return None;
                                }
                                let rel =
                                    bds_core::util::paths::thumbnail_path(&m.id, "small", "webp");
                                let full = dir.join(&rel);
                                if full.exists() { Some(full) } else { None }
                            });
                            (m.id.clone(), thumb)
                        })
                        .collect();

                    (items, thumbs)
                })
                .await
                .unwrap_or_default()
            },
            |(items, thumbs)| Message::SidebarMediaLoaded { items, thumbs },
        )
    }

    /// Load more posts (append to existing sidebar data).
    fn load_more_sidebar_posts(&mut self) -> Task<Message> {
        let (Some(_), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };

        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let content_language = self.content_language.clone();
        let offset = self.sidebar_posts.len() as i64;
        let filter = match self.sidebar_view {
            SidebarView::Pages => self.page_filter.clone(),
            _ => self.post_filter.clone(),
        };
        let is_pages = self.sidebar_view == SidebarView::Pages;

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    Self::query_sidebar_posts_blocking(
                        &db_path,
                        &project_id,
                        &content_language,
                        &filter,
                        is_pages,
                        Self::SIDEBAR_PAGE_SIZE + 1,
                        offset,
                    )
                })
                .await
                .unwrap_or_default()
            },
            Message::SidebarPostsAppended,
        )
    }

    fn parse_filter_date(input: &str, end_of_day: bool) -> Option<i64> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }
        let date = chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").ok()?;
        let time = if end_of_day {
            date.and_hms_opt(23, 59, 59)?
        } else {
            date.and_hms_opt(0, 0, 0)?
        };
        Some(time.and_utc().timestamp_millis())
    }

    fn build_post_filter_params(
        filter: &PostFilter,
        is_pages: bool,
    ) -> bds_core::db::queries::post::PostFilterParams {
        bds_core::db::queries::post::PostFilterParams {
            search_query: filter.search_query.clone(),
            status: filter.status_filter.clone(),
            language: filter.language_filter.clone(),
            year: filter.calendar.selected_year,
            month: filter.calendar.selected_month,
            from: Self::parse_filter_date(&filter.from_date, false),
            to: Self::parse_filter_date(&filter.to_date, true),
            tags: filter.tag_filter.clone(),
            categories: filter.category_filter.clone(),
            exclude_pages: !is_pages,
            pages_only: is_pages,
        }
    }

    fn query_sidebar_posts_blocking(
        db_path: &Path,
        project_id: &str,
        content_language: &str,
        filter: &PostFilter,
        is_pages: bool,
        limit: i64,
        offset: i64,
    ) -> Vec<Post> {
        let Ok(db) = Database::open(db_path) else {
            return Vec::new();
        };

        let params = Self::build_post_filter_params(filter, is_pages);
        if filter.search_query.trim().is_empty() {
            return bds_core::db::queries::post::list_posts_filtered(
                db.conn(),
                project_id,
                &params,
                limit,
                offset,
            )
            .unwrap_or_default();
        }

        let fts_filters = bds_core::db::fts::PostSearchFilters {
            status: params.status.as_deref(),
            tags: (!params.tags.is_empty()).then_some(params.tags.as_slice()),
            categories: (!params.categories.is_empty()).then_some(params.categories.as_slice()),
            language: params.language.as_deref(),
            year: params.year,
            month: params.month,
            from: params.from,
            to: params.to,
            limit: Some(limit as usize),
            offset: Some(offset as usize),
            ..Default::default()
        };

        let ids = bds_core::db::fts::search_posts_filtered(
            db.conn(),
            &params.search_query,
            content_language,
            &fts_filters,
        )
        .map(|results| results.post_ids)
        .unwrap_or_default();

        ids.into_iter()
            .filter_map(|post_id| {
                bds_core::db::queries::post::get_post_by_id(db.conn(), &post_id).ok()
            })
            .filter(|post| post.project_id == project_id)
            .filter(|post| {
                let is_page_post = post
                    .categories
                    .iter()
                    .any(|category| category.eq_ignore_ascii_case("page"));
                if is_pages {
                    is_page_post
                } else {
                    !is_page_post
                }
            })
            .collect()
    }

    fn regenerate_project_thumbnails(
        db: &Database,
        data_dir: &Path,
        project_id: &str,
        mut progress: impl FnMut(usize, usize, &str),
    ) -> Result<usize, String> {
        let media = bds_core::db::queries::media::list_media_by_project(db.conn(), project_id)
            .map_err(|e| e.to_string())?;
        let total = media.len();
        let thumbnails_dir = data_dir.join("thumbnails");
        let mut regenerated = 0usize;

        for (index, item) in media.iter().enumerate() {
            progress(index, total, &item.original_name);
            if !item.mime_type.starts_with("image/") {
                continue;
            }
            let source = data_dir.join(&item.file_path);
            if !source.exists() {
                continue;
            }
            bds_core::util::thumbnail::generate_all_thumbnails(&source, &thumbnails_dir, &item.id)
                .map_err(|e| e.to_string())?;
            regenerated += 1;
        }

        Ok(regenerated)
    }

    /// Load more media (append to existing sidebar data).
    fn load_more_sidebar_media(&mut self) -> Task<Message> {
        let (Some(_), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };

        use bds_core::db::queries::media::MediaFilterParams;

        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let offset = self.sidebar_media.len() as i64;
        let data_dir = self.data_dir.clone();

        let params = MediaFilterParams {
            search_query: self.media_filter.search_query.clone(),
            year: self.media_filter.calendar.selected_year,
            month: self.media_filter.calendar.selected_month,
            tags: self.media_filter.tag_filter.clone(),
        };

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let db = Database::open(&db_path).ok();
                    let items = db
                        .and_then(|db| {
                            bds_core::db::queries::media::list_media_filtered(
                                db.conn(),
                                &project_id,
                                &params,
                                Self::SIDEBAR_PAGE_SIZE + 1,
                                offset,
                            )
                            .ok()
                        })
                        .unwrap_or_default();

                    let thumbs: HashMap<String, Option<std::path::PathBuf>> = items
                        .iter()
                        .map(|m| {
                            let thumb = data_dir.as_ref().and_then(|dir| {
                                if !m.mime_type.starts_with("image/") {
                                    return None;
                                }
                                let rel =
                                    bds_core::util::paths::thumbnail_path(&m.id, "small", "webp");
                                let full = dir.join(&rel);
                                if full.exists() { Some(full) } else { None }
                            });
                            (m.id.clone(), thumb)
                        })
                        .collect();

                    (items, thumbs)
                })
                .await
                .unwrap_or_default()
            },
            |(items, thumbs)| Message::SidebarMediaAppended { items, thumbs },
        )
    }

    /// Refresh available tags, categories, and calendar data for filter widgets.
    fn refresh_filter_metadata(&mut self) {
        if let (Some(db), Some(project)) = (&self.db, &self.active_project) {
            use bds_core::db::queries::media;
            use bds_core::db::queries::post;

            // Post filter metadata
            let all_tags = post::distinct_post_tags(db.conn(), &project.id).unwrap_or_default();
            let all_cats =
                post::distinct_post_categories(db.conn(), &project.id).unwrap_or_default();

            // Calendar counts for posts (excluding pages)
            let post_cal =
                post::post_calendar_counts(db.conn(), &project.id, false, true).unwrap_or_default();
            self.post_filter.available_tags = all_tags.clone();
            self.post_filter.available_categories = all_cats.clone();
            self.post_filter.available_languages = self.blog_languages.clone();
            self.post_filter.calendar_years = Self::build_calendar_tree(&post_cal);

            // Calendar counts for pages only
            let page_cal =
                post::post_calendar_counts(db.conn(), &project.id, true, false).unwrap_or_default();
            self.page_filter.available_tags = all_tags;
            self.page_filter.available_categories = all_cats;
            self.page_filter.available_languages = self.blog_languages.clone();
            self.page_filter.calendar_years = Self::build_calendar_tree(&page_cal);

            // Media filter metadata
            self.media_filter.available_tags =
                media::distinct_media_tags(db.conn(), &project.id).unwrap_or_default();
            let media_cal =
                media::media_calendar_counts(db.conn(), &project.id).unwrap_or_default();
            self.media_filter.calendar_years = Self::build_calendar_tree(&media_cal);
        }
    }

    /// Convert (year, month, count) tuples into CalendarYear/CalendarMonth tree.
    fn build_calendar_tree(data: &[(i32, u32, usize)]) -> Vec<CalendarYear> {
        let mut years: Vec<CalendarYear> = Vec::new();
        for &(y, m, c) in data {
            if let Some(cy) = years.iter_mut().find(|cy| cy.year == y) {
                cy.months.push(CalendarMonth { month: m, count: c });
            } else {
                years.push(CalendarYear {
                    year: y,
                    months: vec![CalendarMonth { month: m, count: c }],
                });
            }
        }
        years
    }

    /// Per layout.allium PanelTabFallback invariant: if the active panel tab
    /// becomes unavailable (post_links when no post tab active, git_log when
    /// neither post nor media tab active), fall back to Tasks.
    fn enforce_panel_tab_fallback(&mut self) {
        let active_tab_type = self
            .active_tab
            .as_ref()
            .and_then(|id| self.tabs.iter().find(|t| t.id == *id).map(|t| &t.tab_type));
        let is_post = active_tab_type == Some(&TabType::Post);
        let is_post_or_media = is_post || active_tab_type == Some(&TabType::Media);

        match self.panel_tab {
            PanelTab::PostLinks if !is_post => self.panel_tab = PanelTab::Tasks,
            PanelTab::GitLog if !is_post_or_media => self.panel_tab = PanelTab::Tasks,
            _ => {}
        }
    }

    /// Synchronise menu enabled/disabled state with current app state.
    ///
    /// Called after state-changing operations (project switch, tab open/close,
    /// offline toggle) so that menu items reflect what's actually possible.
    fn sync_menu_state(&self) {
        let has_project = self.active_project.is_some();
        let active_tab_type = self
            .active_tab
            .as_ref()
            .and_then(|id| self.tabs.iter().find(|t| t.id == *id).map(|t| &t.tab_type));
        let has_tab = active_tab_type.is_some();
        let has_post_tab = active_tab_type == Some(&TabType::Post);

        // File group: need active project for most, need open tab for Save
        self.menu_registry
            .set_enabled(MenuAction::NewPost, has_project);
        self.menu_registry
            .set_enabled(MenuAction::ImportMedia, has_project);
        self.menu_registry.set_enabled(MenuAction::Save, has_tab);
        self.menu_registry
            .set_enabled(MenuAction::OpenInBrowser, has_project && has_post_tab);
        self.menu_registry
            .set_enabled(MenuAction::OpenDataFolder, has_project);

        // Edit: Find/Replace need an open tab
        self.menu_registry.set_enabled(MenuAction::Find, has_tab);
        self.menu_registry.set_enabled(MenuAction::Replace, has_tab);

        // Blog group: need active project
        self.menu_registry
            .set_enabled(MenuAction::PublishSelected, has_project && has_tab);
        self.menu_registry
            .set_enabled(MenuAction::PreviewPost, has_project && has_post_tab);
        self.menu_registry
            .set_enabled(MenuAction::EditMenu, has_project);
        self.menu_registry
            .set_enabled(MenuAction::RebuildDatabase, has_project);
        self.menu_registry
            .set_enabled(MenuAction::ReindexText, has_project);
        self.menu_registry
            .set_enabled(MenuAction::MetadataDiff, has_project);
        self.menu_registry
            .set_enabled(MenuAction::RegenerateCalendar, has_project);
        self.menu_registry
            .set_enabled(MenuAction::ValidateTranslations, has_project);
        self.menu_registry.set_enabled(
            MenuAction::FillMissingTranslations,
            has_project && !self.offline_mode,
        );
        self.menu_registry
            .set_enabled(MenuAction::GenerateSitemap, has_project);
        self.menu_registry
            .set_enabled(MenuAction::ValidateSite, has_project);
        self.menu_registry
            .set_enabled(MenuAction::UploadSite, has_project && !self.offline_mode);
    }

    // ── Editor save/publish helpers ──

    fn save_post_editor(&mut self, post_id: &str) -> Task<Message> {
        match self.persist_post_editor_state(post_id) {
            Ok(()) => self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved")),
            Err(e) => self.notify_operation_failed("common.save", e),
        }
        Task::none()
    }

    fn publish_post_editor(&mut self, post_id: &str) -> Task<Message> {
        if let Err(e) = self.persist_post_editor_state(post_id) {
            self.notify_operation_failed("editor.publish", e);
            return Task::none();
        }
        let Some(ref db) = self.db else {
            return Task::none();
        };
        let Some(ref data_dir) = self.data_dir else {
            return Task::none();
        };
        match engine::post::publish_post(db.conn(), data_dir, post_id) {
            Ok(post) => {
                if let Some(s) = self.post_editors.get_mut(post_id) {
                    s.status = post.status.clone();
                    s.is_dirty = false;
                    s.updated_at = post.updated_at;
                    s.published_at = post.published_at;
                    for draft in s.translation_drafts.values_mut() {
                        draft.status = bds_core::model::PostStatus::Published;
                        draft.is_dirty = false;
                    }
                }
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.published"));
            }
            Err(e) => {
                self.notify_operation_failed("editor.publish", e);
            }
        }
        Task::none()
    }

    fn insert_link_modal(&mut self, post_id: &str) -> Task<Message> {
        let state = match self.post_editors.get(post_id) {
            Some(s) => s.clone(),
            None => return Task::none(),
        };

        self.active_modal = Some(modal::ModalState::PostInsertLink {
            post_id: post_id.to_string(),
            title: state.title,
            results: self.query_post_link_results(post_id, ""),
            search_query: String::new(),
            active_tab: modal::PostInsertLinkTab::Internal,
            external_url: String::new(),
            external_text: String::new(),
        });
        Task::none()
    }

    fn insert_media_modal(&mut self, post_id: &str, link_only: bool) -> Task<Message> {
        let state = match self.post_editors.get(post_id) {
            Some(s) => s.clone(),
            None => return Task::none(),
        };

        self.active_modal = Some(modal::ModalState::InsertMedia {
            post_id: post_id.to_string(),
            title: state.title,
            media_list: self.query_post_insert_media_results(""),
            search_query: String::new(),
            link_only,
        });
        Task::none()
    }

    fn post_gallery(&mut self, post_id: &str) -> Task<Message> {
        let state = match self.post_editors.get(post_id) {
            Some(s) => s.clone(),
            None => return Task::none(),
        };

        let media_list = if let Some(ref db) = self.db {
            engine::post_media::list_media_for_post(db.conn(), post_id).unwrap_or_default()
        } else {
            Vec::new()
        };

        self.active_modal = Some(modal::ModalState::PostGallery {
            post_id: post_id.to_string(),
            title: state.title,
            media_list,
            selected_index: None,
        });
        Task::none()
    }

    fn query_post_link_results(
        &self,
        current_post_id: &str,
        search_query: &str,
    ) -> Vec<modal::InsertLinkResult> {
        let (Some(db), Some(project)) = (&self.db, &self.active_project) else {
            return Vec::new();
        };
        let query = search_query.trim();
        if query.chars().count() < 2 {
            return Vec::new();
        }

        let filters = bds_core::db::fts::PostSearchFilters {
            limit: Some(20),
            ..Default::default()
        };

        let ids = bds_core::db::fts::search_posts_filtered(
            db.conn(),
            query,
            &self.content_language,
            &filters,
        )
        .map(|results| results.post_ids)
        .unwrap_or_default();

        ids.into_iter()
            .filter_map(|post_id| {
                bds_core::db::queries::post::get_post_by_id(db.conn(), &post_id).ok()
            })
            .filter(|post| post.project_id == project.id && post.id != current_post_id)
            .map(|post| modal::InsertLinkResult {
                post_id: post.id,
                title: post.title,
                status: match post.status {
                    bds_core::model::PostStatus::Draft => "draft".to_string(),
                    bds_core::model::PostStatus::Published => "published".to_string(),
                    bds_core::model::PostStatus::Archived => "archived".to_string(),
                },
                canonical_url: bds_core::engine::post::canonical_url(post.created_at, &post.slug),
            })
            .collect()
    }

    fn query_post_insert_media_results(&self, search_query: &str) -> Vec<Media> {
        let (Some(db), Some(project)) = (&self.db, &self.active_project) else {
            return Vec::new();
        };

        let filters = bds_core::db::queries::media::MediaFilterParams {
            search_query: search_query.trim().to_string(),
            ..Default::default()
        };

        bds_core::db::queries::media::list_media_filtered(db.conn(), &project.id, &filters, 24, 0)
            .unwrap_or_default()
    }

    fn query_media_post_picker_results(
        &self,
        media_id: &str,
        search_query: &str,
    ) -> Vec<LinkedPostItem> {
        let (Some(db), Some(project)) = (&self.db, &self.active_project) else {
            return Vec::new();
        };

        let linked_ids: std::collections::HashSet<String> =
            bds_core::engine::post_media::list_posts_for_media(db.conn(), media_id)
                .unwrap_or_default()
                .into_iter()
                .map(|post| post.id)
                .collect();

        let query = search_query.trim().to_lowercase();
        bds_core::db::queries::post::list_posts_by_project(db.conn(), &project.id)
            .unwrap_or_default()
            .into_iter()
            .filter(|post| !linked_ids.contains(&post.id))
            .filter(|post| query.is_empty() || post.title.to_lowercase().contains(&query))
            .take(10)
            .map(|post| LinkedPostItem {
                post_id: post.id,
                title: if post.title.is_empty() {
                    t(self.ui_locale, "editor.untitled")
                } else {
                    post.title
                },
            })
            .collect()
    }

    fn insert_markdown_into_post(&mut self, post_id: &str, markdown: &str) -> Task<Message> {
        let Some(state) = self.post_editors.get_mut(post_id) else {
            return Task::none();
        };
        state.insert_markdown_at_cursor(markdown);
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == post_id) {
            tab.is_dirty = true;
        }
        self.active_modal = None;
        self.save_post_editor(post_id)
    }

    fn insert_selected_post_link(&mut self, post_id: &str, linked_post_id: &str) -> Task<Message> {
        let Some(ref db) = self.db else {
            return Task::none();
        };
        let Ok(linked_post) =
            bds_core::db::queries::post::get_post_by_id(db.conn(), linked_post_id)
        else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "modal.postInsertLink.loadFailed"),
            );
            return Task::none();
        };

        let markdown = bds_core::engine::post::post_insert_link(&linked_post.slug).replacen(
            "title",
            &linked_post.title,
            1,
        );
        self.insert_markdown_into_post(post_id, &markdown)
    }

    fn insert_created_post_link(&mut self, post_id: &str) -> Task<Message> {
        let Some(modal::ModalState::PostInsertLink { search_query, .. }) =
            self.active_modal.clone()
        else {
            return Task::none();
        };
        let title = search_query.trim();
        if title.is_empty() {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "modal.postInsertLink.titleRequired"),
            );
            return Task::none();
        }

        let Some(ref db) = self.db else {
            return Task::none();
        };
        let Some(ref data_dir) = self.data_dir else {
            return Task::none();
        };
        let Some(ref project) = self.active_project else {
            return Task::none();
        };

        match engine::post::create_post(
            db.conn(),
            data_dir,
            &project.id,
            title,
            Some(""),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
        ) {
            Ok(post) => {
                let markdown = bds_core::engine::post::post_insert_link(&post.slug).replacen(
                    "title",
                    &post.title,
                    1,
                );
                self.insert_markdown_into_post(post_id, &markdown)
            }
            Err(_) => {
                self.notify(
                    ToastLevel::Error,
                    &t(self.ui_locale, "modal.postInsertLink.createFailed"),
                );
                Task::none()
            }
        }
    }

    fn insert_selected_media(&mut self, post_id: &str, media_id: &str) -> Task<Message> {
        let Some(ref db) = self.db else {
            return Task::none();
        };
        let Ok(media) = bds_core::db::queries::media::get_media_by_id(db.conn(), media_id) else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "modal.insertMedia.loadFailed"),
            );
            return Task::none();
        };

        let link_only = matches!(
            self.active_modal,
            Some(modal::ModalState::InsertMedia {
                link_only: true,
                ..
            })
        );

        if let (Some(data_dir), Some(project)) = (&self.data_dir, &self.active_project) {
            let already_linked = engine::post_media::list_media_for_post(db.conn(), post_id)
                .map(|items| items.into_iter().any(|item| item.id == media_id))
                .unwrap_or(false);
            if !already_linked {
                let sort_order = engine::post_media::list_media_for_post(db.conn(), post_id)
                    .map(|items| items.len() as i32)
                    .unwrap_or(0);
                let _ = engine::post_media::link_media_to_post(
                    db.conn(),
                    data_dir,
                    &project.id,
                    post_id,
                    media_id,
                    sort_order,
                );
            }
        }

        self.refresh_post_relationships(post_id);

        if link_only {
            self.active_modal = None;
            return Task::none();
        }

        let markdown = bds_core::engine::post::post_insert_media(
            &media.id,
            media.mime_type.starts_with("image/"),
            &media.original_name,
        );
        self.insert_markdown_into_post(post_id, &markdown)
    }

    fn refresh_post_insert_link_modal(
        &mut self,
        active_tab: Option<modal::PostInsertLinkTab>,
        search_query: Option<String>,
        external_url: Option<String>,
        external_text: Option<String>,
    ) {
        let Some(modal::ModalState::PostInsertLink {
            post_id,
            title,
            search_query: current_query,
            active_tab: current_tab,
            external_url: current_url,
            external_text: current_text,
            ..
        }) = self.active_modal.clone()
        else {
            return;
        };

        let next_query = search_query.unwrap_or(current_query);
        let next_tab = active_tab.unwrap_or(current_tab);
        let next_url = external_url.unwrap_or(current_url);
        let next_text = external_text.unwrap_or(current_text);

        self.active_modal = Some(modal::ModalState::PostInsertLink {
            post_id: post_id.clone(),
            title,
            results: self.query_post_link_results(&post_id, &next_query),
            search_query: next_query,
            active_tab: next_tab,
            external_url: next_url,
            external_text: next_text,
        });
    }

    fn refresh_insert_media_modal(&mut self, search_query: String) {
        let Some(modal::ModalState::InsertMedia {
            post_id,
            title,
            link_only,
            ..
        }) = self.active_modal.clone()
        else {
            return;
        };

        self.active_modal = Some(modal::ModalState::InsertMedia {
            post_id,
            title,
            media_list: self.query_post_insert_media_results(&search_query),
            search_query,
            link_only,
        });
    }

    fn update_gallery_selection(&mut self, next_index: Option<usize>) {
        let Some(modal::ModalState::PostGallery {
            post_id,
            title,
            media_list,
            ..
        }) = self.active_modal.clone()
        else {
            return;
        };

        self.active_modal = Some(modal::ModalState::PostGallery {
            post_id,
            title,
            media_list,
            selected_index: next_index,
        });
    }

    fn step_gallery_selection(&mut self, delta: isize) {
        let Some(modal::ModalState::PostGallery {
            selected_index,
            media_list,
            ..
        }) = self.active_modal.clone()
        else {
            return;
        };

        let image_count = media_list
            .iter()
            .filter(|media| media.mime_type.starts_with("image/"))
            .count();
        if image_count == 0 {
            return;
        }

        let current = selected_index.unwrap_or(0);
        let next = if delta < 0 {
            (current + image_count - 1) % image_count
        } else {
            (current + 1) % image_count
        };
        self.update_gallery_selection(Some(next));
    }

    fn save_media_editor(&mut self, media_id: &str) -> Task<Message> {
        match self.persist_media_editor_state(media_id) {
            Ok(()) => self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved")),
            Err(e) => self.notify_operation_failed("common.save", e),
        }
        Task::none()
    }

    fn load_media_linked_posts(&self, media_id: &str) -> Vec<LinkedPostItem> {
        let Some(ref db) = self.db else {
            return Vec::new();
        };

        bds_core::engine::post_media::list_posts_for_media(db.conn(), media_id)
            .unwrap_or_default()
            .into_iter()
            .map(|post| LinkedPostItem {
                post_id: post.id,
                title: if post.title.is_empty() {
                    t(self.ui_locale, "editor.untitled")
                } else {
                    post.title
                },
            })
            .collect()
    }

    fn refresh_media_relationships(&mut self, media_id: &str) {
        let linked_posts = self.load_media_linked_posts(media_id);
        let search = self
            .media_editors
            .get(media_id)
            .map(|state| state.post_picker_search.clone())
            .unwrap_or_default();
        let post_picker_results = self.query_media_post_picker_results(media_id, &search);
        if let Some(state) = self.media_editors.get_mut(media_id) {
            state.linked_posts = linked_posts;
            state.post_picker_results = post_picker_results;
        }
    }

    fn link_media_to_post(&mut self, media_id: &str, post_id: &str) -> Task<Message> {
        if let (Some(db), Some(data_dir), Some(project)) =
            (&self.db, &self.data_dir, &self.active_project)
        {
            let sort_order = bds_core::engine::post_media::list_media_for_post(db.conn(), post_id)
                .map(|items| items.len() as i32)
                .unwrap_or(0);
            match bds_core::engine::post_media::link_media_to_post(
                db.conn(),
                data_dir,
                &project.id,
                post_id,
                media_id,
                sort_order,
            ) {
                Ok(_) => {
                    self.refresh_media_relationships(media_id);
                    self.refresh_post_relationships(post_id);
                }
                Err(error) => {
                    self.notify_operation_failed("editor.linkToPost", error);
                }
            }
        }
        Task::none()
    }

    fn unlink_media_from_post(&mut self, media_id: &str, post_id: &str) -> Task<Message> {
        if let (Some(db), Some(data_dir)) = (&self.db, &self.data_dir) {
            match bds_core::engine::post_media::unlink_media_from_post(
                db.conn(),
                data_dir,
                post_id,
                media_id,
            ) {
                Ok(()) => {
                    self.refresh_media_relationships(media_id);
                    self.refresh_post_relationships(post_id);
                }
                Err(error) => {
                    self.notify_operation_failed("editor.unlinkMedia", error);
                }
            }
        }
        Task::none()
    }

    fn save_template_editor(&mut self, template_id: &str) -> Task<Message> {
        let Some(state) = self.template_editors.get(template_id) else {
            return Task::none();
        };
        let Some(ref db) = self.db else {
            return Task::none();
        };
        let Some(ref project) = self.active_project else {
            return Task::none();
        };

        match save_template_editor_state_impl(db, &project.id, state) {
            Ok(tmpl) => {
                let s = self.template_editors.get_mut(template_id).unwrap();
                s.is_dirty = false;
                s.updated_at = tmpl.updated_at;
                s.validation_error = None;
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tmpl.id) {
                    tab.is_dirty = false;
                    tab.title = tmpl.title.clone();
                }
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
            }
            Err(e) => {
                if let Some(s) = self.template_editors.get_mut(template_id) {
                    s.validation_error = Some(e.clone());
                }
                self.notify_operation_failed("common.save", e);
            }
        }
        Task::none()
    }

    fn save_script_editor(&mut self, script_id: &str) -> Task<Message> {
        let Some(state) = self.script_editors.get(script_id) else {
            return Task::none();
        };
        let Some(ref db) = self.db else {
            return Task::none();
        };
        let Some(ref project) = self.active_project else {
            return Task::none();
        };

        match save_script_editor_state_impl(db, &project.id, state) {
            Ok(script) => {
                let s = self.script_editors.get_mut(script_id).unwrap();
                s.is_dirty = false;
                s.updated_at = script.updated_at;
                s.discovered_entrypoints = engine::script::discover_entrypoints(&s.content);
                s.validation_error = None;
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == script.id) {
                    tab.is_dirty = false;
                    tab.title = script.title.clone();
                }
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
            }
            Err(e) => {
                if let Some(s) = self.script_editors.get_mut(script_id) {
                    s.validation_error = Some(e.clone());
                }
                self.notify_operation_failed("common.save", e);
            }
        }
        Task::none()
    }

    fn apply_persisted_post_state(
        &mut self,
        post_id: &str,
        state: &PostEditorState,
        persisted: PersistedPostState,
    ) {
        match persisted {
            PersistedPostState::Translation(translation) => {
                if let Some(editor) = self.post_editors.get_mut(post_id) {
                    editor.is_dirty = false;
                    editor.last_edit_at_ms = 0;
                    if let Some(draft) = editor.translation_drafts.get_mut(&state.active_language) {
                        draft.title = translation.title.clone();
                        draft.excerpt = translation.excerpt.clone().unwrap_or_default();
                        draft.content = translation.content.clone().unwrap_or_default();
                        draft.status = translation.status.clone();
                        draft.is_dirty = false;
                    }
                }
            }
            PersistedPostState::Canonical(post) => {
                if let Some(editor) = self.post_editors.get_mut(post_id) {
                    editor.title = post.title.clone();
                    editor.slug = post.slug.clone();
                    editor.excerpt = post.excerpt.clone().unwrap_or_default();
                    editor.author = post.author.clone().unwrap_or_default();
                    editor.language = post.language.clone().unwrap_or_default();
                    editor.template_slug = post.template_slug.clone().unwrap_or_default();
                    editor.do_not_translate = post.do_not_translate;
                    editor.tags = post.tags.clone();
                    editor.categories = post.categories.clone();
                    editor.status = post.status.clone();
                    editor.updated_at = post.updated_at;
                    editor.published_at = post.published_at;
                    editor.is_dirty = false;
                    editor.last_edit_at_ms = 0;
                }
                if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == post.id) {
                    tab.is_dirty = false;
                    if !post.title.is_empty() {
                        tab.title = post.title.clone();
                    }
                }
                self.refresh_post_relationships(post_id);
            }
        }

        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == post_id) {
            tab.is_dirty = false;
        }
    }

    fn persist_post_editor_state(&mut self, post_id: &str) -> Result<(), String> {
        let state = self
            .post_editors
            .get(post_id)
            .cloned()
            .ok_or_else(|| "missing post editor".to_string())?;
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| "database unavailable".to_string())?;
        let data_dir = self
            .data_dir
            .as_ref()
            .ok_or_else(|| "project data directory unavailable".to_string())?;

        let persisted = persist_post_editor_state_impl(db, data_dir, &state)?;
        self.apply_persisted_post_state(post_id, &state, persisted);
        Ok(())
    }

    fn persist_post_editor_preview_state(&mut self, post_id: &str) -> Result<(), String> {
        let state = self
            .post_editors
            .get(post_id)
            .cloned()
            .ok_or_else(|| "missing post editor".to_string())?;
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| "database unavailable".to_string())?;

        let persisted = persist_post_editor_preview_state_impl(db, &state)?;
        self.apply_persisted_post_state(post_id, &state, persisted);
        Ok(())
    }

    fn persist_media_editor_state(&mut self, media_id: &str) -> Result<(), String> {
        let state = self
            .media_editors
            .get(media_id)
            .cloned()
            .ok_or_else(|| "missing media editor".to_string())?;
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| "database unavailable".to_string())?;
        let data_dir = self
            .data_dir
            .as_ref()
            .ok_or_else(|| "project data directory unavailable".to_string())?;

        match persist_media_editor_state_impl(db, data_dir, &state)? {
            PersistedMediaState::Translation => {
                if let Some(editor) = self.media_editors.get_mut(media_id) {
                    editor.is_dirty = false;
                }
            }
            PersistedMediaState::Canonical { media, tags } => {
                if let Some(editor) = self.media_editors.get_mut(media_id) {
                    editor.title = media.title.clone().unwrap_or_default();
                    editor.alt = media.alt.clone().unwrap_or_default();
                    editor.caption = media.caption.clone().unwrap_or_default();
                    editor.author = media.author.clone().unwrap_or_default();
                    editor.language = media.language.clone().unwrap_or_default();
                    editor.tags = tags;
                    editor.tags_input = editor.tags.join(", ");
                    editor.updated_at = media.updated_at;
                    editor.is_dirty = false;
                }
            }
        }

        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == media_id) {
            tab.is_dirty = false;
        }
        Ok(())
    }

    fn close_entity_tab(&mut self, entity_id: &str) {
        if let Some(next_idx) = tabs::close_tab(&mut self.tabs, entity_id) {
            self.active_tab = self.tabs.get(next_idx).map(|tab| tab.id.clone());
        } else if self.active_tab.as_deref() == Some(entity_id) {
            self.active_tab = None;
        }
        self.enforce_panel_tab_fallback();
        self.sync_menu_state();
    }

    fn delete_post_editor(&mut self, post_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            return Task::none();
        };
        let Some(data_dir) = &self.data_dir else {
            return Task::none();
        };
        match engine::post::delete_post(db.conn(), data_dir, post_id) {
            Ok(()) => {
                self.post_editors.remove(post_id);
                self.close_entity_tab(post_id);
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.deleted"));
            }
            Err(e) => self.notify_operation_failed("modal.confirmDelete.delete", e),
        }
        Task::none()
    }

    fn discard_post_editor(&mut self, post_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            return Task::none();
        };
        let Some(data_dir) = &self.data_dir else {
            return Task::none();
        };
        match engine::post::discard_post_draft(db.conn(), data_dir, post_id) {
            Ok(post) => {
                if let Some(editor) = self.post_editors.get_mut(post_id) {
                    editor.title = post.title.clone();
                    editor.slug = post.slug.clone();
                    editor.excerpt = post.excerpt.clone().unwrap_or_default();
                    editor.content = post.content.clone().unwrap_or_default();
                    editor.author = post.author.clone().unwrap_or_default();
                    editor.language = post.language.clone().unwrap_or_default();
                    editor.active_language = editor.language.clone();
                    editor.canonical_language = editor.language.clone();
                    editor.template_slug = post.template_slug.clone().unwrap_or_default();
                    editor.tags = post.tags.clone();
                    editor.categories = post.categories.clone();
                    editor.status = post.status.clone();
                    editor.do_not_translate = post.do_not_translate;
                    editor.updated_at = post.updated_at;
                    editor.published_at = post.published_at;
                    editor.is_dirty = false;
                    editor.translation_drafts.clear();
                }
                if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == post.id) {
                    tab.is_dirty = false;
                    tab.title = post.title.clone();
                }
                self.refresh_post_relationships(post_id);
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
            }
            Err(e) => self.notify_operation_failed("editor.discard", e),
        }
        Task::none()
    }

    fn duplicate_post_editor(&mut self, post_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            return Task::none();
        };
        let Some(data_dir) = &self.data_dir else {
            return Task::none();
        };
        match engine::post::duplicate_post(db.conn(), data_dir, post_id) {
            Ok(post) => {
                let tab = Tab {
                    id: post.id.clone(),
                    title: post.title.clone(),
                    tab_type: TabType::Post,
                    is_transient: false,
                    is_dirty: false,
                };
                let idx = tabs::open_tab(&mut self.tabs, tab);
                self.active_tab = self.tabs.get(idx).map(|tab| tab.id.clone());
                if let Some(tab) = self.tabs.get(idx).cloned() {
                    self.load_editor_for_tab(&tab);
                }
                self.enforce_panel_tab_fallback();
                self.sync_menu_state();
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
            }
            Err(e) => self.notify_operation_failed("editor.duplicate", e),
        }
        Task::none()
    }

    fn delete_media_editor(&mut self, media_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            return Task::none();
        };
        let Some(data_dir) = &self.data_dir else {
            return Task::none();
        };
        match engine::media::delete_media(db.conn(), data_dir, media_id) {
            Ok(()) => {
                self.media_editors.remove(media_id);
                self.close_entity_tab(media_id);
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.deleted"));
            }
            Err(e) => self.notify_operation_failed("modal.confirmDelete.delete", e),
        }
        Task::none()
    }

    fn delete_template_editor(&mut self, template_id: &str, force: bool) -> Task<Message> {
        let Some(db) = &self.db else {
            return Task::none();
        };
        let Some(data_dir) = &self.data_dir else {
            return Task::none();
        };
        match engine::template::delete_template(db.conn(), data_dir, template_id, force) {
            Ok(()) => {
                self.template_editors.remove(template_id);
                self.close_entity_tab(template_id);
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.deleted"));
            }
            Err(e) => self.notify_operation_failed("modal.confirmDelete.delete", e),
        }
        Task::none()
    }

    fn show_template_delete_confirmation(&mut self, template_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            return Task::none();
        };
        let Ok(template) =
            bds_core::db::queries::template::get_template_by_id(db.conn(), template_id)
        else {
            return Task::none();
        };

        let referencing_posts =
            bds_core::db::queries::post::list_posts_by_project(db.conn(), &template.project_id)
                .unwrap_or_default()
                .into_iter()
                .filter(|post| post.template_slug.as_deref() == Some(template.slug.as_str()))
                .count();
        let referencing_tags =
            bds_core::db::queries::tag::list_tags_by_project(db.conn(), &template.project_id)
                .unwrap_or_default()
                .into_iter()
                .filter(|tag| tag.post_template_slug.as_deref() == Some(template.slug.as_str()))
                .count();

        if referencing_posts > 0 || referencing_tags > 0 {
            let title = t(self.ui_locale, "template.forceDeleteTitle");
            let message = tw(
                self.ui_locale,
                "template.forceDeleteMessage",
                &[
                    ("posts", &referencing_posts.to_string()),
                    ("tags", &referencing_tags.to_string()),
                ],
            );
            return Task::done(Message::ShowModal(modal::ModalState::Confirm {
                title,
                message,
                on_confirm: modal::ConfirmAction::ForceDeleteTemplate(template_id.to_string()),
            }));
        }

        Task::done(Message::ShowModal(modal::ModalState::ConfirmDelete {
            entity_name: template.title,
            references: Vec::new(),
            on_confirm: modal::ConfirmAction::DeleteTemplate(template_id.to_string()),
        }))
    }

    fn delete_script_editor(&mut self, script_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            return Task::none();
        };
        let Some(data_dir) = &self.data_dir else {
            return Task::none();
        };
        match engine::script::delete_script(db.conn(), data_dir, script_id) {
            Ok(()) => {
                self.script_editors.remove(script_id);
                self.close_entity_tab(script_id);
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.deleted"));
            }
            Err(e) => self.notify_operation_failed("modal.confirmDelete.delete", e),
        }
        Task::none()
    }

    fn reload_tags_state(&mut self) {
        let Some(db) = &self.db else { return };
        let Some(project) = &self.active_project else {
            return;
        };
        let tags = bds_core::db::queries::tag::list_tags_by_project(db.conn(), &project.id)
            .unwrap_or_default();
        let template_options =
            bds_core::db::queries::template::list_templates_by_project(db.conn(), &project.id)
                .unwrap_or_default()
                .into_iter()
                .map(|template| template.slug)
                .collect::<Vec<_>>();
        let posts = bds_core::db::queries::post::list_posts_by_project(db.conn(), &project.id)
            .unwrap_or_default();
        let mut counts = HashMap::new();
        for post in &posts {
            for tag_name in &post.tags {
                *counts.entry(tag_name.to_lowercase()).or_insert(0usize) += 1;
            }
        }
        if let Some(state) = self.tags_view_state.as_mut() {
            state.tags = tags;
            state.tag_post_counts = counts;
            state.template_options = template_options;
            state
                .selected_tags
                .retain(|selected_id| state.tags.iter().any(|tag| &tag.id == selected_id));
            if state.selected_tags.len() == 1 {
                if let Some(tag) = state
                    .tags
                    .iter()
                    .find(|tag| state.selected_tags[0] == tag.id)
                {
                    state.editing_tag = Some(tags_view::EditingTag {
                        id: tag.id.clone(),
                        original_name: tag.name.clone(),
                        name: tag.name.clone(),
                        color: tag.color.clone().unwrap_or_default(),
                        template_slug: tag.post_template_slug.clone().unwrap_or_default(),
                    });
                }
            } else {
                state.editing_tag = None;
            }
            if let Some(target_id) = state.merge_target.as_ref()
                && !state
                    .selected_tags
                    .iter()
                    .any(|selected_id| selected_id == target_id)
            {
                state.merge_target = None;
            }
        } else {
            self.tags_view_state = Some(TagsViewState::new(tags, counts, template_options));
        }
    }

    fn delete_tag(&mut self, tag_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            return Task::none();
        };
        let Some(data_dir) = &self.data_dir else {
            return Task::none();
        };
        let Some(project) = &self.active_project else {
            return Task::none();
        };
        match engine::tag::delete_tag(db.conn(), data_dir, &project.id, tag_id) {
            Ok(()) => {
                self.reload_tags_state();
                if let Some(state) = self.tags_view_state.as_mut()
                    && state.editing_tag.as_ref().map(|tag| tag.id.as_str()) == Some(tag_id)
                {
                    state.editing_tag = None;
                }
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.deleted"));
            }
            Err(e) => self.notify_operation_failed("modal.confirmDelete.delete", e),
        }
        Task::none()
    }

    fn merge_tags(&mut self, sources: &[String], target: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            return Task::none();
        };
        let Some(data_dir) = &self.data_dir else {
            return Task::none();
        };
        let Some(project) = &self.active_project else {
            return Task::none();
        };
        let source_refs = sources.iter().map(String::as_str).collect::<Vec<_>>();
        match engine::tag::merge_tags(db.conn(), data_dir, &project.id, &source_refs, target) {
            Ok(()) => {
                self.reload_tags_state();
                if let Some(state) = self.tags_view_state.as_mut() {
                    state.selected_tags.clear();
                    state.merge_target = None;
                    state.editing_tag = None;
                }
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
            }
            Err(e) => self.notify_operation_failed("tags.merge", e),
        }
        Task::none()
    }

    fn hydrate_settings_state(&self) -> SettingsViewState {
        let mut state = SettingsViewState::default();
        if let Some(project) = &self.active_project {
            state.project_name = project.name.clone();
            state.project_description = iced::widget::text_editor::Content::with_text(
                &project.description.clone().unwrap_or_default(),
            );
            state.data_path = project.data_path.clone().unwrap_or_default();
        }
        if let Some(data_dir) = &self.data_dir {
            if let Ok(meta) = engine::meta::read_project_json(data_dir) {
                state.public_url = meta.public_url.unwrap_or_default();
                state.main_language = meta
                    .main_language
                    .unwrap_or_else(|| self.content_language.clone());
                state.default_author = meta.default_author.unwrap_or_default();
                state.max_posts_per_page = meta.max_posts_per_page.to_string();
                state.blogmark_category = meta.blogmark_category.unwrap_or_default();
                state.blog_languages = if meta.blog_languages.is_empty() {
                    vec![state.main_language.clone()]
                } else {
                    meta.blog_languages
                };
                if !state
                    .blog_languages
                    .iter()
                    .any(|language| language == &state.main_language)
                {
                    state.blog_languages.push(state.main_language.clone());
                }
                state.semantic_similarity_enabled = meta.semantic_similarity_enabled;
            }
            let categories = engine::meta::read_categories_json(data_dir).unwrap_or_else(|_| {
                default_category_rows()
                    .into_iter()
                    .map(|row| row.name)
                    .collect()
            });
            let category_meta = engine::meta::read_category_meta_json(data_dir).unwrap_or_default();
            state.categories = categories
                .into_iter()
                .map(|name| {
                    let meta = category_meta.get(&name);
                    SettingsCategoryRow {
                        title: name.clone(),
                        render_in_lists: meta.map(|value| value.render_in_lists).unwrap_or(true),
                        show_title: meta.map(|value| value.show_title).unwrap_or(true),
                        post_template_slug: meta
                            .and_then(|value| value.post_template_slug.clone())
                            .unwrap_or_default(),
                        list_template_slug: meta
                            .and_then(|value| value.list_template_slug.clone())
                            .unwrap_or_default(),
                        is_protected: ["article", "aside", "page", "picture"]
                            .contains(&name.as_str()),
                        name,
                    }
                })
                .collect();
            if let Ok(pub_prefs) = engine::meta::read_publishing_json(data_dir) {
                state.ssh_host = pub_prefs.ssh_host.unwrap_or_default();
                state.ssh_username = pub_prefs.ssh_user.unwrap_or_default();
                state.ssh_remote_path = pub_prefs.ssh_remote_path.unwrap_or_default();
                state.ssh_mode = match pub_prefs.ssh_mode {
                    SshMode::Scp => "scp".to_string(),
                    SshMode::Rsync => "rsync".to_string(),
                };
            }
        }
        if let Some(db) = &self.db {
            if let Some(project) = &self.active_project {
                state.template_options =
                    bds_core::db::queries::template::list_templates_by_project(
                        db.conn(),
                        &project.id,
                    )
                    .unwrap_or_default()
                    .into_iter()
                    .map(|template| template.slug)
                    .collect();
            }
            if let Ok(setting) =
                bds_core::db::queries::setting::get_setting_by_key(db.conn(), "editor.default_mode")
            {
                state.default_mode = setting.value;
            }
            if let Ok(setting) = bds_core::db::queries::setting::get_setting_by_key(
                db.conn(),
                "editor.diff_view_style",
            ) {
                state.diff_view_style = setting.value;
            }
            if let Ok(setting) = bds_core::db::queries::setting::get_setting_by_key(
                db.conn(),
                "editor.wrap_long_lines",
            ) {
                state.wrap_long_lines = setting.value == "true";
            }
            if let Ok(setting) = bds_core::db::queries::setting::get_setting_by_key(
                db.conn(),
                "editor.hide_unchanged_regions",
            ) {
                state.hide_unchanged_regions = setting.value == "true";
            }
            if let Ok(setting) =
                bds_core::db::queries::setting::get_setting_by_key(db.conn(), "ai.system_prompt")
            {
                state.system_prompt = iced::widget::text_editor::Content::with_text(&setting.value);
            }
            if let Ok(ai_settings) = ai::load_ai_settings(db.conn(), self.offline_mode) {
                state.online_endpoint_url = ai_settings.online_endpoint.url;
                state.online_endpoint_model = ai_settings.online_endpoint.model;
                state.online_api_key_configured = ai_settings.online_endpoint.api_key_configured;
                if !state.online_endpoint_model.is_empty() {
                    state.online_model_options = vec![AiModelOption {
                        id: state.online_endpoint_model.clone(),
                        label: state.online_endpoint_model.clone(),
                        supports_vision: false,
                    }];
                }
                state.airplane_endpoint_url = ai_settings.airplane_endpoint.url;
                state.airplane_endpoint_model = ai_settings.airplane_endpoint.model;
                if !state.airplane_endpoint_model.is_empty() {
                    state.airplane_model_options = vec![AiModelOption {
                        id: state.airplane_endpoint_model.clone(),
                        label: state.airplane_endpoint_model.clone(),
                        supports_vision: false,
                    }];
                }
                state.default_model = ai_settings.default_model.unwrap_or_default();
                state.title_model = ai_settings.title_model.unwrap_or_default();
                state.image_model = ai_settings.image_model.unwrap_or_default();
            }
        }
        state.offline_mode = self.offline_mode;
        state
    }

    fn handle_tags_msg(&mut self, msg: TagsMsg) -> Task<Message> {
        // Ensure tags view state exists
        if self.tags_view_state.is_none() {
            self.reload_tags_state();
            if self.tags_view_state.is_none() {
                self.tags_view_state =
                    Some(TagsViewState::new(Vec::new(), HashMap::new(), Vec::new()));
            }
        }
        let state = self.tags_view_state.as_mut().unwrap();
        match msg {
            TagsMsg::SetSection(s) => {
                state.section = s;
            }
            TagsMsg::SearchChanged(q) => {
                state.search_query = q;
            }
            TagsMsg::ToggleTagSelection(id) => {
                if let Some(pos) = state
                    .selected_tags
                    .iter()
                    .position(|selected_id| selected_id == &id)
                {
                    state.selected_tags.remove(pos);
                } else {
                    state.selected_tags.push(id.clone());
                }

                if state.selected_tags.len() == 1 {
                    let selected_id = state.selected_tags[0].clone();
                    if let Some(tag) = state.tags.iter().find(|tag| tag.id == selected_id) {
                        state.editing_tag = Some(tags_view::EditingTag {
                            id: tag.id.clone(),
                            original_name: tag.name.clone(),
                            name: tag.name.clone(),
                            color: tag.color.clone().unwrap_or_default(),
                            template_slug: tag.post_template_slug.clone().unwrap_or_default(),
                        });
                    }
                    state.section = TagsSection::Manage;
                } else {
                    state.editing_tag = None;
                }

                if let Some(target_id) = state.merge_target.as_ref()
                    && !state
                        .selected_tags
                        .iter()
                        .any(|selected_id| selected_id == target_id)
                {
                    state.merge_target = None;
                }
            }
            TagsMsg::ClearSelection => {
                state.selected_tags.clear();
                state.merge_target = None;
                state.editing_tag = None;
            }
            TagsMsg::CreateNameChanged(name) => {
                state.create_name = name;
            }
            TagsMsg::CreateColorChanged(color) => {
                state.create_color = color;
            }
            TagsMsg::CreateTag => {
                let mut created_editing = None;
                if let (Some(db), Some(data_dir), Some(project)) =
                    (&self.db, &self.data_dir, &self.active_project)
                {
                    match engine::tag::create_tag(
                        db.conn(),
                        data_dir,
                        &project.id,
                        &state.create_name,
                        if state.create_color.trim().is_empty() {
                            None
                        } else {
                            Some(state.create_color.trim())
                        },
                    ) {
                        Ok(tag) => {
                            created_editing = Some(tags_view::EditingTag {
                                id: tag.id,
                                original_name: tag.name.clone(),
                                name: tag.name,
                                color: tag.color.unwrap_or_default(),
                                template_slug: tag.post_template_slug.unwrap_or_default(),
                            });
                        }
                        Err(e) => self.notify_operation_failed("common.save", e),
                    }
                }
                if let Some(editing_tag) = created_editing {
                    self.reload_tags_state();
                    if let Some(state) = self.tags_view_state.as_mut() {
                        state.selected_tags = vec![editing_tag.id.clone()];
                        state.editing_tag = Some(editing_tag);
                        state.create_name.clear();
                        state.create_color.clear();
                        state.section = TagsSection::Manage;
                    }
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                }
            }
            TagsMsg::EditTagName(s) => {
                if let Some(ref mut e) = state.editing_tag {
                    e.name = s;
                }
            }
            TagsMsg::EditTagColor(s) => {
                if let Some(ref mut e) = state.editing_tag {
                    e.color = s;
                }
            }
            TagsMsg::EditTagTemplate(option) => {
                if let Some(ref mut e) = state.editing_tag {
                    e.template_slug = option.slug;
                }
            }
            TagsMsg::SaveTag => {
                if let Some(editing) = state.editing_tag.clone()
                    && let (Some(db), Some(data_dir)) = (&self.db, &self.data_dir)
                {
                    let rename_result = if editing.name != editing.original_name {
                        engine::tag::rename_tag(
                            db.conn(),
                            data_dir,
                            self.active_project
                                .as_ref()
                                .map(|project| project.id.as_str())
                                .unwrap_or_default(),
                            &editing.id,
                            &editing.name,
                        )
                    } else {
                        Ok(())
                    };
                    match rename_result.and_then(|_| {
                        engine::tag::update_tag(
                            db.conn(),
                            data_dir,
                            &editing.id,
                            None,
                            Some(&editing.color),
                            Some(&editing.template_slug),
                        )
                    }) {
                        Ok(()) => {
                            self.reload_tags_state();
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                        }
                        Err(e) => self.notify_operation_failed("common.save", e),
                    }
                }
            }
            TagsMsg::DeleteTag(id) => {
                let name = state
                    .tags
                    .iter()
                    .find(|t| t.id == id)
                    .map(|t| t.name.clone())
                    .unwrap_or_default();
                let count = state
                    .tags
                    .iter()
                    .find(|t| t.id == id)
                    .and_then(|tag| state.tag_post_counts.get(&tag.name.to_lowercase()).copied())
                    .unwrap_or(0);
                return Task::done(Message::ShowModal(modal::ModalState::ConfirmDelete {
                    entity_name: name,
                    references: vec![tw(
                        self.ui_locale,
                        "tags.deleteUsage",
                        &[("count", &count.to_string())],
                    )],
                    on_confirm: modal::ConfirmAction::DeleteTag(id),
                }));
            }
            TagsMsg::SetMergeTarget(option) => {
                state.merge_target = Some(option.id);
            }
            TagsMsg::MergeTags => {
                if let Some(target) = &state.merge_target {
                    let target_name = state
                        .tags
                        .iter()
                        .find(|tag| &tag.id == target)
                        .map(|tag| tag.name.clone())
                        .unwrap_or_default();
                    let sources = state
                        .selected_tags
                        .iter()
                        .filter(|tag_id| *tag_id != target)
                        .cloned()
                        .collect::<Vec<_>>();
                    return Task::done(Message::ShowModal(modal::ModalState::Confirm {
                        title: t(self.ui_locale, "tags.mergeConfirmTitle"),
                        message: tw(
                            self.ui_locale,
                            "tags.mergeConfirmMessage",
                            &[
                                ("count", &sources.len().to_string()),
                                ("target", &target_name),
                            ],
                        ),
                        on_confirm: modal::ConfirmAction::MergeTags {
                            sources,
                            target: target.clone(),
                        },
                    }));
                }
            }
            TagsMsg::SyncTags => {
                if let (Some(db), Some(data_dir), Some(project)) =
                    (&self.db, &self.data_dir, &self.active_project)
                {
                    match engine::tag::discover_tags(db.conn(), data_dir, &project.id) {
                        Ok(_) => {
                            self.reload_tags_state();
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                        }
                        Err(e) => self.notify_operation_failed("tags.discoverButton", e),
                    }
                }
            }
        }
        Task::none()
    }

    fn handle_settings_msg(&mut self, msg: SettingsMsg) -> Task<Message> {
        // Ensure settings state exists
        if self.settings_state.is_none() {
            self.settings_state = Some(self.hydrate_settings_state());
        }
        let state = self.settings_state.as_mut().unwrap();
        match msg {
            SettingsMsg::SearchChanged(q) => {
                state.search_query = q;
            }
            SettingsMsg::ToggleSection(section) => {
                if let Some(pos) = state.collapsed.iter().position(|s| *s == section) {
                    state.collapsed.remove(pos);
                } else {
                    state.collapsed.push(section);
                }
            }
            SettingsMsg::ProjectNameChanged(s) => {
                state.project_name = s;
            }
            SettingsMsg::ProjectDescriptionAction(action) => {
                state.project_description.perform(action);
            }
            SettingsMsg::DataPathChanged(s) => {
                state.data_path = s;
            }
            SettingsMsg::BrowseDataPath => {
                return crate::platform::dialog::pick_folder(t(
                    self.ui_locale,
                    "dialog.selectFolder",
                ));
            }
            SettingsMsg::ResetDataPath => {
                if let Some(ref project) = self.active_project {
                    state.data_path = project.data_path.clone().unwrap_or_default();
                }
            }
            SettingsMsg::PublicUrlChanged(s) => {
                state.public_url = s;
            }
            SettingsMsg::MainLanguageChanged(s) => {
                state.main_language = s.clone();
                if !state.blog_languages.iter().any(|language| language == &s) {
                    state.blog_languages.push(s);
                }
            }
            SettingsMsg::ToggleBlogLanguage(language) => {
                if language == state.main_language {
                    if !state.blog_languages.iter().any(|item| item == &language) {
                        state.blog_languages.push(language);
                    }
                } else if let Some(index) = state
                    .blog_languages
                    .iter()
                    .position(|item| item == &language)
                {
                    state.blog_languages.remove(index);
                } else {
                    state.blog_languages.push(language);
                }
                state.blog_languages.sort();
                state.blog_languages.dedup();
            }
            SettingsMsg::DefaultAuthorChanged(s) => {
                state.default_author = s;
            }
            SettingsMsg::MaxPostsPerPageChanged(s) => {
                state.max_posts_per_page = s;
            }
            SettingsMsg::BlogmarkCategoryChanged(s) => {
                state.blogmark_category = s;
            }
            SettingsMsg::SaveProject => {
                if let (Some(db), Some(data_dir), Some(project)) =
                    (&self.db, &self.data_dir, self.active_project.as_mut())
                {
                    let max_posts = match state.max_posts_per_page.trim().parse::<i32>() {
                        Ok(value) => value,
                        Err(_) => {
                            self.notify(
                                ToastLevel::Error,
                                &t(self.ui_locale, "settings.maxPostsPerPageInvalid"),
                            );
                            return Task::none();
                        }
                    };
                    let mut meta = engine::meta::read_project_json(data_dir).unwrap_or(
                        bds_core::model::metadata::ProjectMetadata {
                            name: state.project_name.clone(),
                            description: None,
                            public_url: None,
                            main_language: None,
                            default_author: None,
                            max_posts_per_page: 50,
                            blogmark_category: None,
                            pico_theme: None,
                            semantic_similarity_enabled: false,
                            blog_languages: Vec::new(),
                        },
                    );
                    meta.name = state.project_name.clone();
                    meta.description = {
                        let value = state.project_description.text();
                        if value.trim().is_empty() {
                            None
                        } else {
                            Some(value)
                        }
                    };
                    meta.public_url = if state.public_url.trim().is_empty() {
                        None
                    } else {
                        Some(state.public_url.clone())
                    };
                    meta.main_language = if state.main_language.trim().is_empty() {
                        None
                    } else {
                        Some(state.main_language.clone())
                    };
                    meta.default_author = if state.default_author.trim().is_empty() {
                        None
                    } else {
                        Some(state.default_author.clone())
                    };
                    meta.max_posts_per_page = max_posts;
                    meta.blogmark_category = if state.blogmark_category.trim().is_empty() {
                        None
                    } else {
                        Some(state.blogmark_category.clone())
                    };
                    meta.blog_languages = state.blog_languages.clone();
                    meta.semantic_similarity_enabled = state.semantic_similarity_enabled;
                    if let Err(e) = meta.validate() {
                        self.notify_operation_failed("common.save", e);
                        return Task::none();
                    }
                    project.name = state.project_name.clone();
                    project.description = meta.description.clone();
                    project.data_path = if state.data_path.trim().is_empty() {
                        None
                    } else {
                        Some(state.data_path.clone())
                    };
                    project.updated_at = bds_core::util::now_unix_ms();
                    let db_result =
                        bds_core::db::queries::project::update_project(db.conn(), project);
                    let file_result = engine::meta::write_project_json(data_dir, &meta);
                    match (db_result, file_result) {
                        (Ok(()), Ok(())) => {
                            if let Some(listing) =
                                self.projects.iter_mut().find(|p| p.id == project.id)
                            {
                                *listing = project.clone();
                            }
                            self.content_language = state.main_language.clone();
                            self.blog_languages = state.blog_languages.clone();
                            self.dashboard_state = Some(self.hydrate_dashboard_state());
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                        }
                        (Err(e), _) => self.notify_operation_failed("common.save", e),
                        (_, Err(e)) => self.notify_operation_failed("common.save", e),
                    }
                }
            }
            SettingsMsg::DefaultModeChanged(s) => {
                state.default_mode = s;
            }
            SettingsMsg::DiffViewStyleChanged(s) => {
                state.diff_view_style = s;
            }
            SettingsMsg::WrapLongLinesChanged(b) => {
                state.wrap_long_lines = b;
            }
            SettingsMsg::HideUnchangedRegionsChanged(b) => {
                state.hide_unchanged_regions = b;
            }
            SettingsMsg::SaveEditor => {
                if let Some(db) = &self.db {
                    match save_editor_settings_state_impl(db, state) {
                        Ok(_) => {
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"))
                        }
                        Err(e) => self.notify_operation_failed("common.save", e),
                    }
                }
            }
            SettingsMsg::AddCategoryNameChanged(value) => {
                state.new_category_name = value;
            }
            SettingsMsg::AddCategory => {
                let category_name = state.new_category_name.trim();
                if category_name.is_empty() {
                    self.notify(
                        ToastLevel::Error,
                        &t(self.ui_locale, "settings.categoryNameRequired"),
                    );
                    return Task::none();
                }
                if state
                    .categories
                    .iter()
                    .any(|row| row.name.eq_ignore_ascii_case(category_name))
                {
                    self.notify(
                        ToastLevel::Error,
                        &t(self.ui_locale, "settings.categoryAlreadyExists"),
                    );
                    return Task::none();
                }
                if let Some(data_dir) = &self.data_dir {
                    match engine::meta::add_category(data_dir, category_name) {
                        Ok(()) => {
                            self.settings_state = Some(self.hydrate_settings_state());
                            if let Some(state) = self.settings_state.as_mut() {
                                state.new_category_name.clear();
                            }
                            self.dashboard_state = Some(self.hydrate_dashboard_state());
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                        }
                        Err(e) => self.notify_operation_failed("common.save", e),
                    }
                }
            }
            SettingsMsg::CategoryTitleChanged(name, value) => {
                if let Some(row) = state.categories.iter_mut().find(|row| row.name == name) {
                    row.title = value;
                }
            }
            SettingsMsg::CategoryRenderInListsChanged(name, value) => {
                if let Some(row) = state.categories.iter_mut().find(|row| row.name == name) {
                    row.render_in_lists = value;
                }
            }
            SettingsMsg::CategoryShowTitleChanged(name, value) => {
                if let Some(row) = state.categories.iter_mut().find(|row| row.name == name) {
                    row.show_title = value;
                }
            }
            SettingsMsg::CategoryPostTemplateChanged(name, value) => {
                if let Some(row) = state.categories.iter_mut().find(|row| row.name == name) {
                    row.post_template_slug = value;
                }
            }
            SettingsMsg::CategoryListTemplateChanged(name, value) => {
                if let Some(row) = state.categories.iter_mut().find(|row| row.name == name) {
                    row.list_template_slug = value;
                }
            }
            SettingsMsg::SaveCategory(name) => {
                if let Some(data_dir) = &self.data_dir
                    && let Some(row) = state.categories.iter().find(|row| row.name == name)
                {
                    let mut category_meta =
                        engine::meta::read_category_meta_json(data_dir).unwrap_or_default();
                    category_meta.insert(
                        row.name.clone(),
                        bds_core::model::metadata::CategorySettings {
                            render_in_lists: row.render_in_lists,
                            show_title: row.show_title,
                            post_template_slug: (!row.post_template_slug.is_empty())
                                .then(|| row.post_template_slug.clone()),
                            list_template_slug: (!row.list_template_slug.is_empty())
                                .then(|| row.list_template_slug.clone()),
                        },
                    );
                    match engine::meta::write_category_meta_json(data_dir, &category_meta) {
                        Ok(()) => {
                            self.dashboard_state = Some(self.hydrate_dashboard_state());
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                        }
                        Err(e) => self.notify_operation_failed("common.save", e),
                    }
                }
            }
            SettingsMsg::RemoveCategory(name) => {
                if let Some(data_dir) = &self.data_dir {
                    match engine::meta::remove_category(data_dir, &name) {
                        Ok(()) => {
                            self.settings_state = Some(self.hydrate_settings_state());
                            self.dashboard_state = Some(self.hydrate_dashboard_state());
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                        }
                        Err(e) => self.notify_operation_failed("common.save", e),
                    }
                }
            }
            SettingsMsg::ResetCategoriesToDefaults => {
                if let Some(data_dir) = &self.data_dir {
                    let default_names = default_category_rows()
                        .into_iter()
                        .map(|row| row.name)
                        .collect::<Vec<_>>();
                    let default_meta = default_category_rows()
                        .into_iter()
                        .map(|row| {
                            (
                                row.name,
                                bds_core::model::metadata::CategorySettings {
                                    render_in_lists: row.render_in_lists,
                                    show_title: row.show_title,
                                    post_template_slug: None,
                                    list_template_slug: None,
                                },
                            )
                        })
                        .collect::<HashMap<_, _>>();
                    match (
                        engine::meta::write_categories_json(data_dir, &default_names),
                        engine::meta::write_category_meta_json(data_dir, &default_meta),
                    ) {
                        (Ok(()), Ok(())) => {
                            self.settings_state = Some(self.hydrate_settings_state());
                            self.dashboard_state = Some(self.hydrate_dashboard_state());
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                        }
                        (Err(e), _) | (_, Err(e)) => self.notify_operation_failed("common.save", e),
                    }
                }
            }
            SettingsMsg::SshModeChanged(s) => {
                state.ssh_mode = s;
            }
            SettingsMsg::SshHostChanged(s) => {
                state.ssh_host = s;
            }
            SettingsMsg::SshUsernameChanged(s) => {
                state.ssh_username = s;
            }
            SettingsMsg::SshRemotePathChanged(s) => {
                state.ssh_remote_path = s;
            }
            SettingsMsg::SavePublishing => {
                if let Some(data_dir) = &self.data_dir {
                    let prefs = PublishingPreferences {
                        ssh_host: if state.ssh_host.trim().is_empty() {
                            None
                        } else {
                            Some(state.ssh_host.clone())
                        },
                        ssh_user: if state.ssh_username.trim().is_empty() {
                            None
                        } else {
                            Some(state.ssh_username.clone())
                        },
                        ssh_remote_path: if state.ssh_remote_path.trim().is_empty() {
                            None
                        } else {
                            Some(state.ssh_remote_path.clone())
                        },
                        ssh_mode: if state.ssh_mode.eq_ignore_ascii_case("scp") {
                            SshMode::Scp
                        } else {
                            SshMode::Rsync
                        },
                    };
                    match engine::meta::write_publishing_json(data_dir, &prefs) {
                        Ok(()) => {
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"))
                        }
                        Err(e) => self.notify_operation_failed("common.save", e),
                    }
                }
            }
            SettingsMsg::ClearPublishing => {
                state.ssh_host.clear();
                state.ssh_username.clear();
                state.ssh_remote_path.clear();
            }
            SettingsMsg::OfflineModeChanged(b) => {
                state.offline_mode = b;
                return Task::done(Message::SetOfflineMode(b));
            }
            SettingsMsg::OnlineEndpointUrlChanged(value) => {
                state.online_endpoint_url = value;
            }
            SettingsMsg::OnlineEndpointModelChanged(value) => {
                state.online_endpoint_model = value;
            }
            SettingsMsg::OnlineApiKeyChanged(value) => {
                state.online_api_key_input = value;
            }
            SettingsMsg::AirplaneEndpointUrlChanged(value) => {
                state.airplane_endpoint_url = value;
            }
            SettingsMsg::AirplaneEndpointModelChanged(value) => {
                state.airplane_endpoint_model = value;
            }
            SettingsMsg::DefaultModelChanged(value) => {
                state.default_model = value;
            }
            SettingsMsg::TitleModelChanged(value) => {
                state.title_model = value;
            }
            SettingsMsg::ImageModelChanged(value) => {
                state.image_model = value;
            }
            SettingsMsg::RefreshOnlineModels => {
                if let Some(db) = &self.db {
                    match Self::refresh_ai_models(db, state, AiEndpointKind::Online) {
                        Ok(()) => {
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"))
                        }
                        Err(error) => self.notify_operation_failed("common.save", error),
                    }
                }
            }
            SettingsMsg::RefreshAirplaneModels => {
                if let Some(db) = &self.db {
                    match Self::refresh_ai_models(db, state, AiEndpointKind::Airplane) {
                        Ok(()) => {
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"))
                        }
                        Err(error) => self.notify_operation_failed("common.save", error),
                    }
                }
            }
            SettingsMsg::SystemPromptAction(action) => {
                state.system_prompt.perform(action);
            }
            SettingsMsg::SaveAi => {
                if let Some(db) = &self.db {
                    match Self::save_ai_settings_state(db, state) {
                        Ok(()) => {
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"))
                        }
                        Err(e) => self.notify_operation_failed("common.save", e),
                    }
                }
            }
            SettingsMsg::ResetSystemPrompt => {
                state.system_prompt = iced::widget::text_editor::Content::new();
            }
            SettingsMsg::SemanticSimilarityChanged(value) => {
                state.semantic_similarity_enabled = value;
            }
            SettingsMsg::RebuildPosts => {
                return self.spawn_engine_task(
                    "settings.rebuildPosts",
                    |db_path, project_id, data_dir, _tm, _tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let report = engine::post::rebuild_posts_from_filesystem(
                            db.conn(),
                            &data_dir,
                            &project_id,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(format!(
                            "created={}, updated={}, translations={}",
                            report.posts_created,
                            report.posts_updated,
                            report.translations_created + report.translations_updated,
                        ))
                    },
                );
            }
            SettingsMsg::RebuildMedia => {
                return self.spawn_engine_task(
                    "settings.rebuildMedia",
                    |db_path, project_id, data_dir, _tm, _tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let report = engine::media::rebuild_media_from_filesystem(
                            db.conn(),
                            &data_dir,
                            &project_id,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(format!(
                            "created={}, updated={}, translations={}",
                            report.media_created,
                            report.media_updated,
                            report.translations_created + report.translations_updated,
                        ))
                    },
                );
            }
            SettingsMsg::RebuildScripts => {
                return self.spawn_engine_task(
                    "settings.rebuildScripts",
                    |db_path, project_id, data_dir, _tm, _tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let report = engine::script_rebuild::rebuild_scripts_from_filesystem(
                            db.conn(),
                            &data_dir,
                            &project_id,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(format!(
                            "created={}, updated={}, errors={}",
                            report.created,
                            report.updated,
                            report.errors.len(),
                        ))
                    },
                );
            }
            SettingsMsg::RebuildTemplates => {
                return self.spawn_engine_task(
                    "settings.rebuildTemplates",
                    |db_path, project_id, data_dir, _tm, _tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let report = engine::template_rebuild::rebuild_templates_from_filesystem(
                            db.conn(),
                            &data_dir,
                            &project_id,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(format!(
                            "created={}, updated={}, errors={}",
                            report.created,
                            report.updated,
                            report.errors.len(),
                        ))
                    },
                );
            }
            SettingsMsg::RebuildLinks => {
                return self.spawn_engine_task(
                    "settings.rebuildLinks",
                    |db_path, project_id, data_dir, _tm, _tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let rebuilt = bds_core::engine::post::rebuild_all_links(
                            db.conn(),
                            &data_dir,
                            &project_id,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(format!("rebuilt={rebuilt}"))
                    },
                );
            }
            SettingsMsg::RegenerateThumbnails => {
                let locale = self.ui_locale;
                return self.spawn_engine_task(
                    "settings.regenerateThumbnails",
                    move |db_path, project_id, data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let regenerated = Self::regenerate_project_thumbnails(
                            &db,
                            &data_dir,
                            &project_id,
                            |index, total, name| {
                                let progress = if total > 0 {
                                    index as f32 / total as f32
                                } else {
                                    1.0
                                };
                                tm.report_progress(
                                    tid,
                                    Some(progress),
                                    Some(tw(locale, "engine.regeneratingItem", &[("name", name)])),
                                );
                            },
                        )?;
                        Ok(format!("regenerated={regenerated}"))
                    },
                );
            }
            SettingsMsg::OpenDataFolder => {
                if let Some(ref dir) = self.data_dir {
                    let _ = open::that(dir);
                }
            }
            SettingsMsg::FocusSection(section) => {
                state.focus_section(section);
            }
        }
        Task::none()
    }

    /// Load outlinks and backlinks for a post, resolving target post titles.
    fn load_post_links(&self, post_id: &str) -> (Vec<ResolvedPostLink>, Vec<ResolvedPostLink>) {
        let Some(ref db) = self.db else {
            return (Vec::new(), Vec::new());
        };
        let outlinks = bds_core::db::queries::post_link::list_links_by_source(db.conn(), post_id)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|link| {
                bds_core::db::queries::post::get_post_by_id(db.conn(), &link.target_post_id)
                    .ok()
                    .map(|p| ResolvedPostLink {
                        post_id: p.id,
                        title: p.title,
                    })
            })
            .collect();
        let backlinks = bds_core::db::queries::post_link::list_links_by_target(db.conn(), post_id)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|link| {
                bds_core::db::queries::post::get_post_by_id(db.conn(), &link.source_post_id)
                    .ok()
                    .map(|p| ResolvedPostLink {
                        post_id: p.id,
                        title: p.title,
                    })
            })
            .collect();
        (outlinks, backlinks)
    }

    fn load_post_media_items(&self, post_id: &str, content: Option<&str>) -> Vec<LinkedMediaItem> {
        let Some(ref db) = self.db else {
            return Vec::new();
        };

        let mut media_by_id =
            bds_core::db::queries::post_media::list_post_media_by_post(db.conn(), post_id)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|link| {
                    bds_core::db::queries::media::get_media_by_id(db.conn(), &link.media_id)
                        .ok()
                        .map(|media| LinkedMediaItem {
                            media_id: media.id,
                            name: media.title.unwrap_or(media.original_name),
                            file_path: media.file_path,
                            is_image: media.mime_type.starts_with("image/"),
                            sort_order: link.sort_order,
                        })
                })
                .map(|media| (media.media_id.clone(), media))
                .collect::<HashMap<_, _>>();

        for media_id in referenced_media_ids(content.unwrap_or_default()) {
            if media_by_id.contains_key(&media_id) {
                continue;
            }

            if let Ok(media) = bds_core::db::queries::media::get_media_by_id(db.conn(), &media_id) {
                media_by_id.insert(
                    media_id,
                    LinkedMediaItem {
                        media_id: media.id,
                        name: media.title.unwrap_or(media.original_name),
                        file_path: media.file_path,
                        is_image: media.mime_type.starts_with("image/"),
                        sort_order: i32::MAX,
                    },
                );
            }
        }

        let mut items = media_by_id.into_values().collect::<Vec<_>>();
        items.sort_by_key(|media| media.sort_order);
        items
    }

    fn refresh_post_relationships(&mut self, post_id: &str) {
        let (outlinks, backlinks) = self.load_post_links(post_id);
        let current_content = self
            .post_editors
            .get(post_id)
            .map(|state| state.content.clone());
        let linked_media = self.load_post_media_items(post_id, current_content.as_deref());
        if let Some(state) = self.post_editors.get_mut(post_id) {
            state.outlinks = outlinks;
            state.backlinks = backlinks;
            state.linked_media = linked_media;
        }
    }

    /// Load editor state when a tab is opened for an entity.
    fn load_editor_for_tab(&mut self, tab: &Tab) {
        let Some(ref db) = self.db else { return };
        match tab.tab_type {
            TabType::Post => {
                if !self.post_editors.contains_key(&tab.id) {
                    match bds_core::db::queries::post::get_post_by_id(db.conn(), &tab.id) {
                        Ok(mut post) => {
                            // Published posts don't store body in DB — read from file
                            if post.content.is_none()
                                && let Some(ref data_dir) = self.data_dir
                            {
                                let rel = bds_core::util::paths::post_file_path(
                                    post.created_at,
                                    &post.slug,
                                );
                                let path = data_dir.join(&rel);
                                if let Ok(raw) = std::fs::read_to_string(&path)
                                    && let Ok((_fm, body)) =
                                        bds_core::util::frontmatter::read_post_file(&raw)
                                {
                                    post.content = Some(body);
                                }
                            }
                            // Load translations for translation flags bar
                            let mut translations = bds_core::db::queries::post_translation::list_post_translations_by_post(
                                db.conn(), &post.id,
                            ).unwrap_or_default();
                            // Published translations don't store body in DB — read from file
                            if let Some(ref data_dir) = self.data_dir {
                                for tr in &mut translations {
                                    if tr.content.is_none() {
                                        let rel = bds_core::util::paths::translation_file_path(
                                            post.created_at,
                                            &post.slug,
                                            &tr.language,
                                        );
                                        let path = data_dir.join(&rel);
                                        if let Ok(raw) = std::fs::read_to_string(&path)
                                            && let Ok((_fm, body)) =
                                                bds_core::util::frontmatter::read_translation_file(
                                                    &raw,
                                                )
                                        {
                                            tr.content = Some(body);
                                        }
                                    }
                                }
                            }
                            let (outlinks, backlinks) = self.load_post_links(&post.id);
                            let linked_media =
                                self.load_post_media_items(&post.id, post.content.as_deref());
                            self.post_editors.insert(
                                post.id.clone(),
                                PostEditorState::from_post(
                                    &post,
                                    default_post_editor_mode(self.settings_state.as_ref()),
                                    &self.blog_languages,
                                    &translations,
                                    outlinks,
                                    backlinks,
                                    linked_media,
                                ),
                            );
                        }
                        Err(e) => {
                            self.notify_operation_failed("activity.posts", e);
                        }
                    }
                }
            }
            TabType::Media => {
                if !self.media_editors.contains_key(&tab.id) {
                    match bds_core::db::queries::media::get_media_by_id(db.conn(), &tab.id) {
                        Ok(media) => {
                            let translations = bds_core::db::queries::media_translation::list_media_translations_by_media(
                                db.conn(), &media.id,
                            ).unwrap_or_default();
                            let linked_posts = self.load_media_linked_posts(&media.id);
                            self.media_editors.insert(
                                media.id.clone(),
                                MediaEditorState::from_media(
                                    &media,
                                    &self.blog_languages,
                                    &translations,
                                    linked_posts,
                                ),
                            );
                        }
                        Err(e) => {
                            self.notify_operation_failed("activity.media", e);
                        }
                    }
                }
            }
            TabType::Templates => {
                if !self.template_editors.contains_key(&tab.id) {
                    match bds_core::db::queries::template::get_template_by_id(db.conn(), &tab.id) {
                        Ok(mut template) => {
                            // Published templates: read content from file, strip frontmatter
                            if template.content.is_none()
                                && let Some(ref data_dir) = self.data_dir
                            {
                                let rel = bds_core::util::paths::template_file_path(&template.slug);
                                let path = data_dir.join(&rel);
                                if let Ok(raw) = std::fs::read_to_string(&path)
                                    && let Ok((_fm, body)) =
                                        bds_core::util::frontmatter::read_template_file(&raw)
                                {
                                    template.content = Some(body);
                                }
                            }
                            self.template_editors.insert(
                                template.id.clone(),
                                TemplateEditorState::from_template(&template),
                            );
                        }
                        Err(e) => {
                            self.notify_operation_failed("activity.templates", e);
                        }
                    }
                }
            }
            TabType::Scripts => {
                if !self.script_editors.contains_key(&tab.id) {
                    match bds_core::db::queries::script::get_script_by_id(db.conn(), &tab.id) {
                        Ok(mut script) => {
                            // Published scripts: read content from file using actual file_path
                            if script.content.is_none()
                                && let Some(ref data_dir) = self.data_dir
                            {
                                let path = data_dir.join(&script.file_path);
                                if let Ok(raw) = std::fs::read_to_string(&path)
                                    && let Ok((_fm, body)) =
                                        bds_core::util::frontmatter::read_script_file(&raw)
                                {
                                    script.content = Some(body);
                                }
                            }
                            self.script_editors
                                .insert(script.id.clone(), ScriptEditorState::from_script(&script));
                        }
                        Err(e) => {
                            self.notify_operation_failed("activity.scripts", e);
                        }
                    }
                }
            }
            TabType::Tags => {
                if self.tags_view_state.is_none() {
                    let project_id = self
                        .active_project
                        .as_ref()
                        .map(|p| p.id.as_str())
                        .unwrap_or("");
                    // Import tags from file first, then sync from posts (additive only)
                    if let Some(ref data_dir) = self.data_dir {
                        let _ = bds_core::engine::tag::import_tags_from_file(
                            db.conn(),
                            data_dir,
                            project_id,
                        );
                        let _ = bds_core::engine::tag::sync_tags_from_posts(db.conn(), project_id);
                    }
                    let tags =
                        bds_core::db::queries::tag::list_tags_by_project(db.conn(), project_id)
                            .unwrap_or_default();
                    let template_options =
                        bds_core::db::queries::template::list_templates_by_project(
                            db.conn(),
                            project_id,
                        )
                        .unwrap_or_default()
                        .into_iter()
                        .map(|template| template.slug)
                        .collect::<Vec<_>>();
                    // Compute post counts per tag for cloud sizing
                    let posts =
                        bds_core::db::queries::post::list_posts_by_project(db.conn(), project_id)
                            .unwrap_or_default();
                    let mut tag_post_counts = std::collections::HashMap::new();
                    for post in &posts {
                        for tag_name in &post.tags {
                            *tag_post_counts
                                .entry(tag_name.to_lowercase())
                                .or_insert(0usize) += 1;
                        }
                    }
                    self.tags_view_state =
                        Some(TagsViewState::new(tags, tag_post_counts, template_options));
                }
            }
            TabType::Settings if self.settings_state.is_none() => {
                let mut state = SettingsViewState::default();
                if let Some(ref project) = self.active_project {
                    state.project_name = project.name.clone();
                    state.project_description = iced::widget::text_editor::Content::with_text(
                        &project.description.clone().unwrap_or_default(),
                    );
                    state.data_path = project.data_path.clone().unwrap_or_default();
                }
                if let Some(ref data_dir) = self.data_dir {
                    if let Ok(meta) = engine::meta::read_project_json(data_dir) {
                        state.public_url = meta.public_url.unwrap_or_default();
                        state.default_author = meta.default_author.unwrap_or_default();
                        state.max_posts_per_page = meta.max_posts_per_page.to_string();
                    }
                    if let Ok(pub_prefs) = engine::meta::read_publishing_json(data_dir) {
                        state.ssh_host = pub_prefs.ssh_host.unwrap_or_default();
                        state.ssh_username = pub_prefs.ssh_user.unwrap_or_default();
                        state.ssh_remote_path = pub_prefs.ssh_remote_path.unwrap_or_default();
                        state.ssh_mode = format!("{:?}", pub_prefs.ssh_mode).to_lowercase();
                    }
                }
                state.offline_mode = self.offline_mode;
                self.settings_state = Some(state);
            }
            _ => {}
        }
    }

    fn refresh_ai_models(
        db: &Database,
        state: &mut SettingsViewState,
        kind: AiEndpointKind,
    ) -> Result<(), String> {
        let endpoint = Self::compose_ai_endpoint(state, kind)?;
        let models = ai::refresh_model_catalog(&endpoint).map_err(|error| error.to_string())?;
        let options = models
            .into_iter()
            .map(|model| AiModelOption {
                id: model.id,
                label: model.name,
                supports_vision: model.supports_vision,
            })
            .collect::<Vec<_>>();
        match kind {
            AiEndpointKind::Online => state.online_model_options = options,
            AiEndpointKind::Airplane => state.airplane_model_options = options,
        }
        let _ = db;
        Ok(())
    }

    fn save_ai_settings_state(db: &Database, state: &mut SettingsViewState) -> Result<(), String> {
        if Self::endpoint_has_configuration(state, AiEndpointKind::Online) {
            let online_endpoint = Self::compose_ai_endpoint(state, AiEndpointKind::Online)?;
            ai::test_endpoint(&online_endpoint).map_err(|error| error.to_string())?;
            ai::save_endpoint(db.conn(), &online_endpoint).map_err(|error| error.to_string())?;
            state.online_api_key_input.clear();
            state.online_api_key_configured = true;
        }
        if Self::endpoint_has_configuration(state, AiEndpointKind::Airplane) {
            let airplane_endpoint = Self::compose_ai_endpoint(state, AiEndpointKind::Airplane)?;
            ai::test_endpoint(&airplane_endpoint).map_err(|error| error.to_string())?;
            ai::save_endpoint(db.conn(), &airplane_endpoint).map_err(|error| error.to_string())?;
        }
        ai::save_model_preferences(
            db.conn(),
            (!state.default_model.trim().is_empty()).then_some(state.default_model.as_str()),
            (!state.title_model.trim().is_empty()).then_some(state.title_model.as_str()),
            (!state.image_model.trim().is_empty()).then_some(state.image_model.as_str()),
            &state.system_prompt.text(),
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn endpoint_has_configuration(state: &SettingsViewState, kind: AiEndpointKind) -> bool {
        match kind {
            AiEndpointKind::Online => {
                !state.online_endpoint_url.trim().is_empty()
                    || !state.online_endpoint_model.trim().is_empty()
                    || !state.online_api_key_input.trim().is_empty()
                    || state.online_api_key_configured
            }
            AiEndpointKind::Airplane => {
                !state.airplane_endpoint_url.trim().is_empty()
                    || !state.airplane_endpoint_model.trim().is_empty()
            }
        }
    }

    fn compose_ai_endpoint(
        state: &SettingsViewState,
        kind: AiEndpointKind,
    ) -> Result<AiEndpointConfig, String> {
        let (url, model, configured) = match kind {
            AiEndpointKind::Online => (
                state.online_endpoint_url.trim().to_string(),
                state.online_endpoint_model.trim().to_string(),
                state.online_api_key_configured,
            ),
            AiEndpointKind::Airplane => (
                state.airplane_endpoint_url.trim().to_string(),
                state.airplane_endpoint_model.trim().to_string(),
                false,
            ),
        };
        let api_key = if kind == AiEndpointKind::Online {
            let input = state.online_api_key_input.trim();
            if !input.is_empty() {
                Some(input.to_string())
            } else if configured {
                ai::load_endpoint_api_key(kind).map_err(|error| error.to_string())?
            } else {
                None
            }
        } else {
            None
        };
        Ok(AiEndpointConfig {
            kind,
            url,
            model,
            api_key,
        })
    }

    fn ensure_preview_server(&mut self) -> Result<(), String> {
        let Some(project) = self.active_project.as_ref() else {
            return Err(t(self.ui_locale, "engine.generateSiteNoProject"));
        };
        let Some(data_dir) = self.data_dir.clone() else {
            return Err(t(self.ui_locale, "engine.previewDataDirUnavailable"));
        };

        let should_restart = self
            .preview_session
            .as_ref()
            .map(|session| session.project_id != project.id)
            .unwrap_or(true);
        if should_restart {
            self.preview_session = None;
            match engine::preview::start_preview_server(
                self.db_path.clone(),
                data_dir,
                project.id.clone(),
            ) {
                Ok(handle) => {
                    self.preview_session = Some(PreviewSession {
                        project_id: project.id.clone(),
                        handle,
                    });
                }
                Err(engine::EngineError::Conflict(_)) if self.preview_session.is_none() => {
                    return Err(t(self.ui_locale, "engine.previewPortInUse"));
                }
                Err(error) => {
                    return Err(error.to_string());
                }
            }
        }

        if let Some(session) = &self.preview_session {
            let _ = &session.handle;
        }

        Ok(())
    }

    fn preview_url_for_post(&mut self, post_id: &str) -> Result<String, String> {
        self.persist_post_editor_preview_state(post_id)?;
        self.ensure_preview_server()?;

        let language = self
            .post_editors
            .get(post_id)
            .map(|editor| editor.active_language.clone())
            .filter(|language| !language.is_empty())
            .unwrap_or_else(|| self.content_language.clone());

        Ok(draft_preview_url(post_id, &language))
    }

    fn active_post_uses_embedded_preview(&self) -> bool {
        self.active_tab
            .as_ref()
            .and_then(|tab_id| self.post_editors.get(tab_id))
            .map(|editor| editor.editor_mode == "preview")
            .unwrap_or(false)
    }

    fn hide_embedded_preview(&self) {
        if let Some(preview) = &self.embedded_preview {
            preview.controller.set_visible(false);
        }
    }

    fn sync_embedded_preview_for_active_post(&mut self) -> Task<Message> {
        let Some(active_id) = self.active_tab.clone() else {
            self.hide_embedded_preview();
            return Task::none();
        };
        let Some(editor) = self.post_editors.get(&active_id) else {
            self.hide_embedded_preview();
            return Task::none();
        };
        if editor.editor_mode != "preview" {
            self.hide_embedded_preview();
            return Task::none();
        }

        let url = match self.preview_url_for_post(&active_id) {
            Ok(url) => url,
            Err(error) => {
                self.notify(ToastLevel::Error, &error);
                return Task::none();
            }
        };

        if let Some(preview) = &mut self.embedded_preview {
            preview.current_url = Some(url.clone());
            if preview.controller.is_active() {
                preview.controller.navigate(&url);
                preview.controller.set_visible(true);
                return Task::none();
            }
        } else {
            self.embedded_preview = Some(EmbeddedPreviewState {
                controller: WebViewController::new(WebViewConfig::default().url(url.clone())),
                current_url: Some(url.clone()),
            });
        }

        let Some(window_id) = self.main_window_id else {
            return window::get_oldest().map(Message::MainWindowLoaded);
        };

        if let Some(preview) = &mut self.embedded_preview
            && !preview.controller.is_active()
        {
            preview.controller = WebViewController::new(WebViewConfig::default().url(url));
            return preview
                .controller
                .create_task(window_id, Message::EmbeddedPreviewReady);
        }

        Task::none()
    }

    fn preview_active_post(&mut self) -> Task<Message> {
        let Some(active_id) = self.active_tab.clone() else {
            return Task::none();
        };
        if !self
            .tabs
            .iter()
            .any(|tab| tab.id == active_id && tab.tab_type == TabType::Post)
        {
            return Task::none();
        }

        let url = match self.preview_url_for_post(&active_id) {
            Ok(url) => url,
            Err(error) => {
                self.notify(ToastLevel::Error, &error);
                return Task::none();
            }
        };

        if let Err(error) = open::that(&url) {
            self.notify(ToastLevel::Error, &error.to_string());
        }
        Task::none()
    }

    fn run_post_ai_analysis(&mut self, post_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "common.databaseUnavailable"),
            );
            return Task::none();
        };
        let Some(state) = self.post_editors.get(post_id).cloned() else {
            return Task::none();
        };
        let request = ai::OneShotRequest {
            operation: ai::OneShotOperation::AnalyzePost,
            content: json!({
                "title": state.title,
                "excerpt": state.excerpt,
                "content": content_sample(&state.content, 2000),
            }),
        };
        match ai::run_one_shot(db.conn(), self.offline_mode, &request) {
            Ok(ai::OneShotResponse::PostAnalysis(result)) => {
                self.active_modal = Some(modal::ModalState::AISuggestions {
                    target: modal::AiEntityTarget::Post(post_id.to_string()),
                    fields: vec![
                        modal::AiSuggestionField {
                            key: "title".to_string(),
                            label: t(self.ui_locale, "editor.title"),
                            current_value: state.title,
                            suggested_value: result.title,
                            accepted: true,
                            locked: false,
                        },
                        modal::AiSuggestionField {
                            key: "excerpt".to_string(),
                            label: t(self.ui_locale, "editor.excerpt"),
                            current_value: state.excerpt,
                            suggested_value: result.excerpt,
                            accepted: true,
                            locked: false,
                        },
                        modal::AiSuggestionField {
                            key: "slug".to_string(),
                            label: t(self.ui_locale, "editor.slug"),
                            current_value: state.slug,
                            suggested_value: result.slug,
                            accepted: state.published_at.is_none(),
                            locked: state.published_at.is_some(),
                        },
                    ],
                });
            }
            Ok(_) => {}
            Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
        }
        Task::none()
    }

    fn run_post_taxonomy_analysis(&mut self, post_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "common.databaseUnavailable"),
            );
            return Task::none();
        };
        let Some(state) = self.post_editors.get(post_id).cloned() else {
            return Task::none();
        };
        let request = ai::OneShotRequest {
            operation: ai::OneShotOperation::AnalyzeTaxonomy,
            content: json!({
                "title": state.title,
                "excerpt": state.excerpt,
                "content": content_sample(&state.content, 2000),
                "tags": state.tags,
                "categories": state.categories,
            }),
        };
        match ai::run_one_shot(db.conn(), self.offline_mode, &request) {
            Ok(ai::OneShotResponse::Taxonomy(result)) => {
                self.active_modal = Some(modal::ModalState::AISuggestions {
                    target: modal::AiEntityTarget::Post(post_id.to_string()),
                    fields: vec![
                        modal::AiSuggestionField {
                            key: "tags".to_string(),
                            label: t(self.ui_locale, "sidebar.filter.tags"),
                            current_value: state.tags.join(", "),
                            suggested_value: result.tags.join(", "),
                            accepted: true,
                            locked: false,
                        },
                        modal::AiSuggestionField {
                            key: "categories".to_string(),
                            label: t(self.ui_locale, "sidebar.filter.categories"),
                            current_value: state.categories.join(", "),
                            suggested_value: result.categories.join(", "),
                            accepted: true,
                            locked: false,
                        },
                    ],
                });
            }
            Ok(_) => {}
            Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
        }
        Task::none()
    }

    fn detect_post_language(&mut self, post_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "common.databaseUnavailable"),
            );
            return Task::none();
        };
        let Some(state) = self.post_editors.get(post_id).cloned() else {
            return Task::none();
        };
        let request = ai::OneShotRequest {
            operation: ai::OneShotOperation::DetectLanguage,
            content: json!({
                "text": format!("{}\n\n{}\n\n{}", state.title, state.excerpt, content_sample(&state.content, 2000)),
            }),
        };
        match ai::run_one_shot(db.conn(), self.offline_mode, &request) {
            Ok(ai::OneShotResponse::LanguageDetection(result)) => {
                if let Some(editor) = self.post_editors.get_mut(post_id) {
                    editor.language = result.language_code;
                    editor.mark_dirty();
                }
                if let Err(error) = self.persist_post_editor_state(post_id) {
                    self.notify(ToastLevel::Error, &error);
                } else {
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                }
            }
            Ok(_) => {}
            Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
        }
        Task::none()
    }

    fn open_post_translation_modal(&mut self, post_id: &str) -> Task<Message> {
        let Some(state) = self.post_editors.get(post_id) else {
            return Task::none();
        };
        let targets = self
            .blog_languages
            .iter()
            .filter(|language| **language != state.active_language)
            .map(|language| modal::LanguageTarget {
                code: language.clone(),
                name: language_label(self.ui_locale, language),
                flag_emoji: bds_core::i18n::normalize_language(language)
                    .flag_emoji()
                    .to_string(),
                has_existing_translation: state.translation_drafts.contains_key(language),
                existing_status: state.translation_drafts.get(language).map(|draft| {
                    match draft.status {
                        PostStatus::Draft => "draft".to_string(),
                        PostStatus::Published => "published".to_string(),
                        PostStatus::Archived => "archived".to_string(),
                    }
                }),
            })
            .collect::<Vec<_>>();
        self.active_modal = Some(modal::ModalState::LanguagePicker {
            target: modal::AiEntityTarget::Post(post_id.to_string()),
            source_language: state.active_language.clone(),
            available_targets: targets,
        });
        Task::none()
    }

    fn translate_post_to(&mut self, post_id: &str, target_language: &str) -> Task<Message> {
        self.active_modal = None;
        let Some(db) = &self.db else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "common.databaseUnavailable"),
            );
            return Task::none();
        };
        let Some(data_dir) = &self.data_dir else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "engine.previewDataDirUnavailable"),
            );
            return Task::none();
        };
        let Some(state) = self.post_editors.get(post_id).cloned() else {
            return Task::none();
        };

        if state.active_language == state.canonical_language
            && state.status == PostStatus::Published
        {
            match engine::post::update_post(
                db.conn(),
                data_dir,
                post_id,
                Some(&state.title),
                None,
                Some(Some(state.excerpt.as_str())),
                Some(&state.content),
                Some(state.tags.clone()),
                Some(state.categories.clone()),
                Some(Some(state.author.as_str())),
                Some(Some(state.language.as_str())),
                Some(Some(state.template_slug.as_str())),
                Some(state.do_not_translate),
            ) {
                Ok(updated) => {
                    if let Some(editor) = self.post_editors.get_mut(post_id) {
                        editor.status = updated.status;
                    }
                }
                Err(error) => {
                    self.notify(ToastLevel::Error, &error.to_string());
                    return Task::none();
                }
            }
        }

        let request = ai::OneShotRequest {
            operation: ai::OneShotOperation::TranslatePost {
                target_language: target_language.to_string(),
            },
            content: json!({
                "title": state.title,
                "excerpt": state.excerpt,
                "content": state.content,
            }),
        };
        match ai::run_one_shot(db.conn(), self.offline_mode, &request) {
            Ok(ai::OneShotResponse::Translation(result)) => {
                if let Some(editor) = self.post_editors.get_mut(post_id) {
                    editor.switch_language(target_language);
                    editor.title = result.title.clone();
                    editor.excerpt = result.excerpt.clone();
                    editor.content = result.content.clone();
                    editor.editor_buffer =
                        std::cell::RefCell::new(bds_editor::EditorBuffer::new(&result.content));
                    editor.mark_dirty();
                }
                if let Err(error) = self.persist_post_editor_state(post_id) {
                    self.notify(ToastLevel::Error, &error);
                } else {
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                }
            }
            Ok(_) => {}
            Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
        }
        Task::none()
    }

    fn run_media_ai_analysis(&mut self, media_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "common.databaseUnavailable"),
            );
            return Task::none();
        };
        let Some(data_dir) = &self.data_dir else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "engine.previewDataDirUnavailable"),
            );
            return Task::none();
        };
        let Some(state) = self.media_editors.get(media_id).cloned() else {
            return Task::none();
        };
        let image_data_url = match build_ai_image_data_url(
            data_dir,
            &state.media_id,
            &state.file_path,
            &state.mime_type,
        ) {
            Ok(value) => value,
            Err(error) => {
                self.notify(ToastLevel::Error, &error);
                return Task::none();
            }
        };
        let request = ai::OneShotRequest {
            operation: ai::OneShotOperation::AnalyzeImage,
            content: json!({
                "title": state.title,
                "alt": state.alt,
                "caption": state.caption,
                "filename": state.original_name,
                "mime_type": state.mime_type,
                "image_data_url": image_data_url,
            }),
        };
        match ai::run_one_shot(db.conn(), self.offline_mode, &request) {
            Ok(ai::OneShotResponse::ImageAnalysis(result)) => {
                self.active_modal = Some(modal::ModalState::AISuggestions {
                    target: modal::AiEntityTarget::Media(media_id.to_string()),
                    fields: vec![
                        modal::AiSuggestionField {
                            key: "title".to_string(),
                            label: t(self.ui_locale, "editor.title"),
                            current_value: state.title,
                            suggested_value: result.title,
                            accepted: true,
                            locked: false,
                        },
                        modal::AiSuggestionField {
                            key: "alt".to_string(),
                            label: t(self.ui_locale, "editor.alt"),
                            current_value: state.alt,
                            suggested_value: result.alt,
                            accepted: true,
                            locked: false,
                        },
                        modal::AiSuggestionField {
                            key: "caption".to_string(),
                            label: t(self.ui_locale, "editor.caption"),
                            current_value: state.caption,
                            suggested_value: result.caption,
                            accepted: true,
                            locked: false,
                        },
                    ],
                });
            }
            Ok(_) => {}
            Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
        }
        Task::none()
    }

    fn detect_media_language(&mut self, media_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "common.databaseUnavailable"),
            );
            return Task::none();
        };
        let Some(state) = self.media_editors.get(media_id).cloned() else {
            return Task::none();
        };
        let request = ai::OneShotRequest {
            operation: ai::OneShotOperation::DetectLanguage,
            content: json!({
                "text": format!("{}\n{}\n{}", state.title, state.alt, state.caption),
            }),
        };
        match ai::run_one_shot(db.conn(), self.offline_mode, &request) {
            Ok(ai::OneShotResponse::LanguageDetection(result)) => {
                if let Some(editor) = self.media_editors.get_mut(media_id) {
                    editor.language = result.language_code;
                    editor.is_dirty = true;
                }
                if let Err(error) = self.persist_media_editor_state(media_id) {
                    self.notify(ToastLevel::Error, &error);
                } else {
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                }
            }
            Ok(_) => {}
            Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
        }
        Task::none()
    }

    fn open_media_translation_modal(&mut self, media_id: &str) -> Task<Message> {
        let Some(state) = self.media_editors.get(media_id) else {
            return Task::none();
        };
        let targets = self
            .blog_languages
            .iter()
            .filter(|language| **language != state.active_language)
            .map(|language| modal::LanguageTarget {
                code: language.clone(),
                name: language_label(self.ui_locale, language),
                flag_emoji: bds_core::i18n::normalize_language(language)
                    .flag_emoji()
                    .to_string(),
                has_existing_translation: state.translation_drafts.contains_key(language),
                existing_status: state
                    .translation_drafts
                    .get(language)
                    .map(|_| "draft".to_string()),
            })
            .collect::<Vec<_>>();
        self.active_modal = Some(modal::ModalState::LanguagePicker {
            target: modal::AiEntityTarget::Media(media_id.to_string()),
            source_language: state.active_language.clone(),
            available_targets: targets,
        });
        Task::none()
    }

    fn translate_media_to(&mut self, media_id: &str, target_language: &str) -> Task<Message> {
        self.active_modal = None;
        let Some(db) = &self.db else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "common.databaseUnavailable"),
            );
            return Task::none();
        };
        let Some(state) = self.media_editors.get(media_id).cloned() else {
            return Task::none();
        };
        let source_language = if state.language.is_empty() {
            match ai::run_one_shot(
                db.conn(),
                self.offline_mode,
                &ai::OneShotRequest {
                    operation: ai::OneShotOperation::DetectLanguage,
                    content: json!({ "text": format!("{}\n{}\n{}", state.title, state.alt, state.caption) }),
                },
            ) {
                Ok(ai::OneShotResponse::LanguageDetection(result)) => result.language_code,
                Ok(_) => String::new(),
                Err(error) => {
                    self.notify(ToastLevel::Error, &error.to_string());
                    return Task::none();
                }
            }
        } else {
            state.language.clone()
        };
        if let Some(editor) = self.media_editors.get_mut(media_id)
            && editor.language.is_empty()
        {
            editor.language = source_language;
        }
        let request = ai::OneShotRequest {
            operation: ai::OneShotOperation::TranslateMedia {
                target_language: target_language.to_string(),
            },
            content: json!({
                "title": state.title,
                "alt": state.alt,
                "caption": state.caption,
            }),
        };
        match ai::run_one_shot(db.conn(), self.offline_mode, &request) {
            Ok(ai::OneShotResponse::MediaTranslation(result)) => {
                if let Some(editor) = self.media_editors.get_mut(media_id) {
                    editor.switch_language(target_language);
                    editor.title = result.title.clone();
                    editor.alt = result.alt.clone();
                    editor.caption = result.caption.clone();
                    editor.is_dirty = true;
                }
                if let Err(error) = self.persist_media_editor_state(media_id) {
                    self.notify(ToastLevel::Error, &error);
                } else {
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                }
            }
            Ok(_) => {}
            Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
        }
        Task::none()
    }

    fn apply_ai_suggestions(
        &mut self,
        target: modal::AiEntityTarget,
        fields: &[modal::AiSuggestionField],
    ) -> Task<Message> {
        match target {
            modal::AiEntityTarget::Post(post_id) => {
                if let Some(editor) = self.post_editors.get_mut(&post_id) {
                    for field in fields
                        .iter()
                        .filter(|field| field.accepted && !field.locked)
                    {
                        match field.key.as_str() {
                            "title" => editor.title = field.suggested_value.clone(),
                            "excerpt" => editor.excerpt = field.suggested_value.clone(),
                            "slug" => editor.slug = field.suggested_value.clone(),
                            "tags" => {
                                editor.tags = split_csv_values(&field.suggested_value);
                                editor.tags_input.clear();
                            }
                            "categories" => {
                                editor.categories = split_csv_values(&field.suggested_value);
                                editor.categories_input.clear();
                            }
                            _ => {}
                        }
                    }
                    editor.mark_dirty();
                }
                if let Err(error) = self.persist_post_editor_state(&post_id) {
                    self.notify(ToastLevel::Error, &error);
                } else {
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                }
            }
            modal::AiEntityTarget::Media(media_id) => {
                if let Some(editor) = self.media_editors.get_mut(&media_id) {
                    for field in fields
                        .iter()
                        .filter(|field| field.accepted && !field.locked)
                    {
                        match field.key.as_str() {
                            "title" => editor.title = field.suggested_value.clone(),
                            "alt" => editor.alt = field.suggested_value.clone(),
                            "caption" => editor.caption = field.suggested_value.clone(),
                            _ => {}
                        }
                    }
                    editor.is_dirty = true;
                }
                if let Err(error) = self.persist_media_editor_state(&media_id) {
                    self.notify(ToastLevel::Error, &error);
                } else {
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                }
            }
        }
        Task::none()
    }
}

fn content_sample(content: &str, max_len: usize) -> String {
    content.chars().take(max_len).collect()
}

fn build_ai_image_data_url(
    data_dir: &Path,
    media_id: &str,
    file_path: &str,
    mime_type: &str,
) -> Result<String, String> {
    if !mime_type.starts_with("image/") {
        return Err("AI image analysis requires an image".to_string());
    }

    let source_path = data_dir.join(file_path.trim_start_matches('/'));
    let thumbnail_relative = bds_core::util::thumbnail_path(media_id, "ai", "jpg");
    let thumbnail_path = data_dir.join(&thumbnail_relative);

    if !thumbnail_path.exists() {
        bds_core::util::thumbnail::generate_all_thumbnails(
            &source_path,
            &data_dir.join("thumbnails"),
            media_id,
        )
        .map_err(|error| format!("failed to generate AI thumbnail: {error}"))?;
    }

    let bytes = std::fs::read(&thumbnail_path)
        .map_err(|error| format!("failed to read AI thumbnail: {error}"))?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:image/jpeg;base64,{encoded}"))
}

fn split_csv_values(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(|item| item.to_string())
        .collect()
}

fn language_label(locale: UiLocale, code: &str) -> String {
    let key = format!("language.{code}");
    let value = t(locale, &key);
    if value == key {
        code.to_string()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BdsApp, Message, POST_AUTO_SAVE_DELAY_MS, PersistedMediaState, PersistedPostState,
        PostStatus, SettingsMsg, month_abbreviation, persist_media_editor_state_impl,
        persist_post_editor_preview_state_impl, persist_post_editor_state_impl,
        save_editor_settings_state_impl, save_script_editor_state_impl,
        save_template_editor_state_impl,
    };
    use crate::state::sidebar_filter::PostFilter;
    use crate::views::media_editor::{MediaEditorMsg, MediaEditorState};
    use crate::views::post_editor::PostEditorState;
    use crate::views::script_editor::ScriptEditorState;
    use crate::views::settings_view::SettingsViewState;
    use crate::views::template_editor::TemplateEditorState;
    use bds_core::db::Database;
    use bds_core::db::fts::ensure_fts_tables;
    use bds_core::db::queries::project::insert_project;
    use bds_core::engine::{ai, media, post, script, template};
    use bds_core::model::{Project, ScriptKind, TemplateKind};
    use chrono::{Datelike, TimeZone};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use tempfile::TempDir;

    fn make_project() -> Project {
        Project {
            id: "p1".to_string(),
            name: "Test Project".to_string(),
            slug: "test-project".to_string(),
            description: Some("desc".to_string()),
            data_path: None,
            is_active: true,
            created_at: 1000,
            updated_at: 1000,
        }
    }

    fn tiny_png_bytes() -> &'static [u8] {
        &[
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, b'I', b'H',
            b'D', b'R', 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00,
            0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A', b'T', 0x78,
            0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99,
            0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
        ]
    }

    fn setup() -> (Database, Project, TempDir) {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        ensure_fts_tables(db.conn()).unwrap();
        let project = make_project();
        insert_project(db.conn(), &project).unwrap();
        let tempdir = TempDir::new().unwrap();
        std::fs::create_dir_all(tempdir.path().join("meta")).unwrap();
        std::fs::write(
            tempdir.path().join("meta/project.json"),
            r#"{"name":"Test Project","maxPostsPerPage":50,"blogLanguages":["en"],"semanticSimilarityEnabled":false}"#,
        )
        .unwrap();
        std::fs::write(tempdir.path().join("meta/publishing.json"), "{}\n").unwrap();
        std::fs::write(
            tempdir.path().join("meta/categories.json"),
            r#"["article","aside","page","picture"]"#,
        )
        .unwrap();
        std::fs::write(tempdir.path().join("meta/category-meta.json"), "{}\n").unwrap();
        (db, project, tempdir)
    }

    fn spawn_models_server() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            if let Some(stream) = listener.incoming().next() {
                let mut stream = stream.unwrap();
                let mut buffer = [0_u8; 8192];
                let size = stream.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer[..size]).to_string();
                assert!(request.starts_with("GET /v1/models HTTP/1.1"));
                let body = r#"{"data":[{"id":"llama3.2","name":"Llama 3.2"}]}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body,
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        format!("http://{}", addr)
    }

    fn make_app(db: Database, project: Project, tmp: &TempDir) -> BdsApp {
        BdsApp::new_for_tests(db, project, tmp.path().to_path_buf())
    }

    #[test]
    fn post_editor_save_flow_persists_changes() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Original",
            Some("Body"),
            vec!["rust".to_string()],
            vec!["article".to_string()],
            Some("Alice"),
            Some("en"),
            None,
        )
        .unwrap();

        let editor_post =
            bds_core::db::queries::post::get_post_by_id(db.conn(), &created.id).unwrap();
        let mut editor = PostEditorState::from_post(
            &editor_post,
            "markdown",
            &["en".to_string(), "de".to_string()],
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        editor.title = "Updated Post".to_string();
        editor.content = "Updated body".to_string();
        editor.tags = vec!["rust".to_string(), "lua".to_string()];

        let result = persist_post_editor_state_impl(&db, tmp.path(), &editor).unwrap();
        match result {
            PersistedPostState::Canonical(post) => assert_eq!(post.title, "Updated Post"),
            PersistedPostState::Translation(_) => panic!("expected canonical post save"),
        }

        let saved = bds_core::db::queries::post::get_post_by_id(db.conn(), &created.id).unwrap();
        assert_eq!(saved.title, "Updated Post");
        assert_eq!(saved.content.as_deref(), Some("Updated body"));
        assert_eq!(saved.tags, vec!["rust".to_string(), "lua".to_string()]);
    }

    #[test]
    fn persist_post_editor_state_allows_published_posts_without_slug_conflict() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Published",
            Some("Body"),
            Vec::new(),
            vec!["article".to_string()],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let published = post::publish_post(db.conn(), tmp.path(), &created.id).unwrap();

        let mut editor_post =
            bds_core::db::queries::post::get_post_by_id(db.conn(), &published.id).unwrap();
        let raw = std::fs::read_to_string(tmp.path().join(&editor_post.file_path)).unwrap();
        let (_frontmatter, body) = bds_core::util::frontmatter::read_post_file(&raw).unwrap();
        editor_post.content = Some(body);

        let editor = PostEditorState::from_post(
            &editor_post,
            "preview",
            &["en".to_string()],
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        let result = persist_post_editor_state_impl(&db, tmp.path(), &editor).unwrap();

        match result {
            PersistedPostState::Canonical(post) => {
                assert_eq!(post.slug, created.slug);
                assert_eq!(post.title, created.title);
            }
            PersistedPostState::Translation(_) => panic!("expected canonical post save"),
        }
    }

    #[test]
    fn preview_persist_bypasses_fts_for_published_canonical_post() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Published",
            Some("Body"),
            Vec::new(),
            vec!["article".to_string()],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let published = post::publish_post(db.conn(), tmp.path(), &created.id).unwrap();
        db.conn().execute("DROP TABLE posts_fts", []).unwrap();

        let mut editor_post =
            bds_core::db::queries::post::get_post_by_id(db.conn(), &published.id).unwrap();
        let raw = std::fs::read_to_string(tmp.path().join(&editor_post.file_path)).unwrap();
        let (_frontmatter, body) = bds_core::util::frontmatter::read_post_file(&raw).unwrap();
        editor_post.content = Some(body);

        let mut editor = PostEditorState::from_post(
            &editor_post,
            "preview",
            &["en".to_string()],
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        editor.title = "Preview Draft".to_string();
        editor.slug = "changed-slug".to_string();
        editor.content = "Preview-only body".to_string();

        let result = persist_post_editor_preview_state_impl(&db, &editor).unwrap();

        match result {
            PersistedPostState::Canonical(post) => {
                assert_eq!(post.slug, created.slug);
                assert_eq!(post.title, "Preview Draft");
                assert_eq!(post.content.as_deref(), Some("Preview-only body"));
                assert_eq!(post.status, PostStatus::Draft);
            }
            PersistedPostState::Translation(_) => panic!("expected canonical post save"),
        }
    }

    #[test]
    fn preview_persist_bypasses_fts_for_translation_updates() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Canonical",
            Some("Body"),
            Vec::new(),
            vec!["article".to_string()],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        post::upsert_translation(
            db.conn(),
            tmp.path(),
            &created.id,
            "de",
            "Alt",
            Some("Auszug"),
            Some("Inhalt"),
        )
        .unwrap();
        db.conn().execute("DROP TABLE posts_fts", []).unwrap();

        let editor_post =
            bds_core::db::queries::post::get_post_by_id(db.conn(), &created.id).unwrap();
        let translations = bds_core::db::queries::post_translation::list_post_translations_by_post(
            db.conn(),
            &created.id,
        )
        .unwrap();
        let mut editor = PostEditorState::from_post(
            &editor_post,
            "preview",
            &["en".to_string(), "de".to_string()],
            &translations,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        editor.switch_language("de");
        editor.title = "Vorschau".to_string();
        editor.excerpt = "Kurz".to_string();
        editor.content = "Nur Vorschau".to_string();

        let result = persist_post_editor_preview_state_impl(&db, &editor).unwrap();

        match result {
            PersistedPostState::Translation(translation) => {
                assert_eq!(translation.language, "de");
                assert_eq!(translation.title, "Vorschau");
                assert_eq!(translation.content.as_deref(), Some("Nur Vorschau"));
            }
            PersistedPostState::Canonical(_) => panic!("expected translation save"),
        }
    }

    #[test]
    fn save_ai_settings_allows_airplane_only_configuration() {
        let (db, _project, _tmp) = setup();
        let mut state = SettingsViewState {
            airplane_endpoint_url: spawn_models_server(),
            airplane_endpoint_model: "llama3.2".to_string(),
            system_prompt: iced::widget::text_editor::Content::with_text("Use JSON only."),
            ..SettingsViewState::default()
        };

        BdsApp::save_ai_settings_state(&db, &mut state).unwrap();

        let settings = ai::load_ai_settings(db.conn(), false).unwrap();
        assert!(settings.online_endpoint.url.is_empty());
        assert!(settings.online_endpoint.model.is_empty());
        assert_eq!(settings.airplane_endpoint.url, state.airplane_endpoint_url);
        assert_eq!(settings.airplane_endpoint.model, "llama3.2");
        assert_eq!(settings.system_prompt.trim_end(), "Use JSON only.");
    }

    #[test]
    fn media_editor_save_flow_persists_changes() {
        let (db, project, tmp) = setup();
        let source = tmp.path().join("tiny.png");
        std::fs::write(&source, tiny_png_bytes()).unwrap();
        let imported = media::import_media(
            db.conn(),
            tmp.path(),
            &project.id,
            &source,
            "tiny.png",
            Some("Tiny"),
            Some("Alt"),
            None,
            None,
            Some("en"),
            vec!["photo".to_string()],
        )
        .unwrap();

        let media_record =
            bds_core::db::queries::media::get_media_by_id(db.conn(), &imported.id).unwrap();
        let mut editor =
            MediaEditorState::from_media(&media_record, &["en".to_string()], &[], Vec::new());
        editor.title = "Tiny Updated".to_string();
        editor.tags_input = "photo, lua".to_string();

        let result = persist_media_editor_state_impl(&db, tmp.path(), &editor).unwrap();
        match result {
            PersistedMediaState::Canonical { media, tags } => {
                assert_eq!(media.title.as_deref(), Some("Tiny Updated"));
                assert_eq!(tags, vec!["photo".to_string(), "lua".to_string()]);
            }
            PersistedMediaState::Translation => panic!("expected canonical media save"),
        }

        let saved = bds_core::db::queries::media::get_media_by_id(db.conn(), &imported.id).unwrap();
        assert_eq!(saved.title.as_deref(), Some("Tiny Updated"));
        assert_eq!(saved.tags, vec!["photo".to_string(), "lua".to_string()]);
        assert!(tmp.path().join(saved.sidecar_path).exists());
    }

    #[test]
    fn template_editor_save_flow_persists_changes() {
        let (db, project, _tmp) = setup();
        let created = template::create_template(
            db.conn(),
            &project.id,
            "Post Template",
            TemplateKind::Post,
            "<article>{{ title }}</article>",
        )
        .unwrap();

        let template_record =
            bds_core::db::queries::template::get_template_by_id(db.conn(), &created.id).unwrap();
        let mut editor = TemplateEditorState::from_template(&template_record);
        editor.title = "Updated Template".to_string();
        editor.content = "<main>{{ title }}</main>".to_string();

        let saved_template = save_template_editor_state_impl(&db, &project.id, &editor).unwrap();
        assert_eq!(saved_template.title, "Updated Template");

        let saved =
            bds_core::db::queries::template::get_template_by_id(db.conn(), &created.id).unwrap();
        assert_eq!(saved.title, "Updated Template");
        assert_eq!(saved.content.as_deref(), Some("<main>{{ title }}</main>"));
    }

    #[test]
    fn template_editor_save_rejects_invalid_content() {
        let (db, project, _tmp) = setup();
        let created = template::create_template(
            db.conn(),
            &project.id,
            "Broken Template",
            TemplateKind::Post,
            "<article>{{ title }}</article>",
        )
        .unwrap();

        let template_record =
            bds_core::db::queries::template::get_template_by_id(db.conn(), &created.id).unwrap();
        let mut editor = TemplateEditorState::from_template(&template_record);
        editor.content = "{% if title %}".to_string();

        let error = save_template_editor_state_impl(&db, &project.id, &editor).unwrap_err();
        assert!(error.contains("endif") || error.contains("unclosed") || error.contains("missing"));

        let saved =
            bds_core::db::queries::template::get_template_by_id(db.conn(), &created.id).unwrap();
        assert_eq!(
            saved.content.as_deref(),
            Some("<article>{{ title }}</article>")
        );
    }

    #[test]
    fn script_editor_save_flow_persists_changes() {
        let (db, project, _tmp) = setup();
        let created = script::create_script(
            db.conn(),
            &project.id,
            "Utility Script",
            ScriptKind::Utility,
            "function main()\n  return 'ok'\nend",
            Some("main"),
        )
        .unwrap();

        let script_record =
            bds_core::db::queries::script::get_script_by_id(db.conn(), &created.id).unwrap();
        let mut editor = ScriptEditorState::from_script(&script_record);
        editor.title = "Updated Script".to_string();
        editor.content = "function main()\n  return 'lua'\nend".to_string();
        editor.entrypoint = "main".to_string();

        let saved_script = save_script_editor_state_impl(&db, &project.id, &editor).unwrap();
        assert_eq!(saved_script.title, "Updated Script");

        let saved =
            bds_core::db::queries::script::get_script_by_id(db.conn(), &created.id).unwrap();
        assert_eq!(saved.title, "Updated Script");
        assert_eq!(
            saved.content.as_deref(),
            Some("function main()\n  return 'lua'\nend")
        );
    }

    #[test]
    fn script_editor_save_rejects_invalid_content() {
        let (db, project, _tmp) = setup();
        let created = script::create_script(
            db.conn(),
            &project.id,
            "Broken Script",
            ScriptKind::Utility,
            "function main()\n  return 'ok'\nend",
            Some("main"),
        )
        .unwrap();

        let script_record =
            bds_core::db::queries::script::get_script_by_id(db.conn(), &created.id).unwrap();
        let mut editor = ScriptEditorState::from_script(&script_record);
        editor.content = "function main()\n  return 'oops'".to_string();

        let error = save_script_editor_state_impl(&db, &project.id, &editor).unwrap_err();
        assert!(error.contains("end") || error.contains("unclosed") || error.contains("missing"));

        let saved =
            bds_core::db::queries::script::get_script_by_id(db.conn(), &created.id).unwrap();
        assert_eq!(
            saved.content.as_deref(),
            Some("function main()\n  return 'ok'\nend")
        );
    }

    #[test]
    fn settings_editor_save_flow_persists_values() {
        let (db, _project, _tmp) = setup();
        let settings = SettingsViewState {
            default_mode: "markdown".to_string(),
            diff_view_style: "side-by-side".to_string(),
            wrap_long_lines: false,
            hide_unchanged_regions: true,
            ..SettingsViewState::default()
        };

        save_editor_settings_state_impl(&db, &settings).unwrap();

        let wrap =
            bds_core::db::queries::setting::get_setting_by_key(db.conn(), "editor.wrap_long_lines")
                .unwrap();
        let hide = bds_core::db::queries::setting::get_setting_by_key(
            db.conn(),
            "editor.hide_unchanged_regions",
        )
        .unwrap();
        let diff =
            bds_core::db::queries::setting::get_setting_by_key(db.conn(), "editor.diff_view_style")
                .unwrap();

        assert_eq!(wrap.value, "false");
        assert_eq!(hide.value, "true");
        assert_eq!(diff.value, "side-by-side");
    }

    #[test]
    fn load_post_media_items_includes_media_referenced_only_in_markdown() {
        let (db, project, tmp) = setup();
        let source = tmp.path().join("tiny.png");
        std::fs::write(&source, tiny_png_bytes()).unwrap();
        let imported = media::import_media(
            db.conn(),
            tmp.path(),
            &project.id,
            &source,
            "tiny.png",
            Some("Tiny"),
            Some("Alt"),
            None,
            None,
            Some("en"),
            vec!["photo".to_string()],
        )
        .unwrap();
        let post = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Referenced",
            Some(&format!("![](bds-media://{})", imported.id)),
            Vec::new(),
            vec!["article".to_string()],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let app = make_app(db, project, &tmp);

        let linked_media = app.load_post_media_items(&post.id, post.content.as_deref());

        assert_eq!(linked_media.len(), 1);
        assert_eq!(linked_media[0].media_id, imported.id);
        assert!(linked_media[0].is_image);
    }

    #[test]
    fn draft_preview_url_points_at_local_preview_server() {
        let url = super::draft_preview_url("post-42", "de");

        assert_eq!(
            url,
            format!(
                "http://{}:{}/__draft/post-42?language=de",
                bds_core::engine::preview::PREVIEW_HOST,
                bds_core::engine::preview::PREVIEW_PORT,
            )
        );
    }

    #[test]
    fn active_post_uses_embedded_preview_only_for_preview_mode_posts() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Previewed",
            Some("Body"),
            Vec::new(),
            vec!["article".to_string()],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let editor_post =
            bds_core::db::queries::post::get_post_by_id(db.conn(), &created.id).unwrap();
        let mut editor = PostEditorState::from_post(
            &editor_post,
            "markdown",
            &["en".to_string()],
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let mut app = make_app(db, project, &tmp);
        app.tabs.push(crate::state::tabs::Tab {
            id: created.id.clone(),
            tab_type: crate::state::tabs::TabType::Post,
            title: created.title.clone(),
            is_transient: false,
            is_dirty: false,
        });
        app.active_tab = Some(created.id.clone());
        app.post_editors.insert(created.id.clone(), editor.clone());

        assert!(!app.active_post_uses_embedded_preview());

        editor.set_editor_mode("preview");
        app.post_editors.insert(created.id.clone(), editor);

        assert!(app.active_post_uses_embedded_preview());
    }

    #[test]
    fn task_tick_autosaves_dirty_post_editor() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Original",
            Some("Body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();

        let editor_post =
            bds_core::db::queries::post::get_post_by_id(db.conn(), &created.id).unwrap();
        let mut editor = PostEditorState::from_post(
            &editor_post,
            "markdown",
            &["en".to_string()],
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        editor.title = "Autosaved".to_string();
        editor.mark_dirty();
        editor.last_edit_at_ms = bds_core::util::now_unix_ms() - POST_AUTO_SAVE_DELAY_MS - 100;

        let mut app = make_app(db, project, &tmp);
        app.post_editors.insert(created.id.clone(), editor);
        app.tabs.push(crate::state::tabs::Tab {
            id: created.id.clone(),
            tab_type: crate::state::tabs::TabType::Post,
            title: "Original".to_string(),
            is_transient: false,
            is_dirty: true,
        });
        app.active_tab = Some(created.id.clone());

        let _ = app.update(Message::TaskTick);

        let saved = bds_core::db::queries::post::get_post_by_id(
            app.db.as_ref().unwrap().conn(),
            &created.id,
        )
        .unwrap();
        assert_eq!(saved.title, "Autosaved");
        assert!(!app.post_editors.get(&created.id).unwrap().is_dirty);
    }

    #[test]
    fn switching_tabs_flushes_active_post_editor() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Original",
            Some("Body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();

        let editor_post =
            bds_core::db::queries::post::get_post_by_id(db.conn(), &created.id).unwrap();
        let mut editor = PostEditorState::from_post(
            &editor_post,
            "markdown",
            &["en".to_string()],
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        editor.title = "Switched".to_string();
        editor.mark_dirty();

        let mut app = make_app(db, project, &tmp);
        app.post_editors.insert(created.id.clone(), editor);
        app.tabs.push(crate::state::tabs::Tab {
            id: created.id.clone(),
            tab_type: crate::state::tabs::TabType::Post,
            title: "Original".to_string(),
            is_transient: false,
            is_dirty: true,
        });
        app.tabs.push(crate::state::tabs::Tab {
            id: "settings".to_string(),
            tab_type: crate::state::tabs::TabType::Settings,
            title: "Settings".to_string(),
            is_transient: false,
            is_dirty: false,
        });
        app.active_tab = Some(created.id.clone());

        let _ = app.update(Message::SelectTab("settings".to_string()));

        let saved = bds_core::db::queries::post::get_post_by_id(
            app.db.as_ref().unwrap().conn(),
            &created.id,
        )
        .unwrap();
        assert_eq!(saved.title, "Switched");
        assert_eq!(app.active_tab.as_deref(), Some("settings"));
    }

    #[test]
    fn media_editor_link_and_unlink_post_refreshes_relationships() {
        let (db, project, tmp) = setup();
        let created_post = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Linked Post",
            Some("Body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let source = tmp.path().join("tiny.png");
        std::fs::write(&source, tiny_png_bytes()).unwrap();
        let imported = media::import_media(
            db.conn(),
            tmp.path(),
            &project.id,
            &source,
            "tiny.png",
            Some("Tiny"),
            Some("Alt"),
            None,
            None,
            Some("en"),
            vec![],
        )
        .unwrap();

        let media_record =
            bds_core::db::queries::media::get_media_by_id(db.conn(), &imported.id).unwrap();
        let mut app = make_app(db, project, &tmp);
        app.media_editors.insert(
            imported.id.clone(),
            MediaEditorState::from_media(&media_record, &["en".to_string()], &[], Vec::new()),
        );
        app.tabs.push(crate::state::tabs::Tab {
            id: imported.id.clone(),
            tab_type: crate::state::tabs::TabType::Media,
            title: "Tiny".to_string(),
            is_transient: false,
            is_dirty: false,
        });
        app.active_tab = Some(imported.id.clone());

        let _ = app.update(Message::MediaEditor(MediaEditorMsg::LinkPost(
            created_post.id.clone(),
        )));

        let linked = bds_core::engine::post_media::list_posts_for_media(
            app.db.as_ref().unwrap().conn(),
            &imported.id,
        )
        .unwrap();
        assert_eq!(linked.len(), 1);
        assert_eq!(linked[0].id, created_post.id);
        assert_eq!(
            app.media_editors
                .get(&imported.id)
                .unwrap()
                .linked_posts
                .len(),
            1
        );

        let _ = app.update(Message::MediaEditor(MediaEditorMsg::UnlinkPost(
            created_post.id.clone(),
        )));

        let linked = bds_core::engine::post_media::list_posts_for_media(
            app.db.as_ref().unwrap().conn(),
            &imported.id,
        )
        .unwrap();
        assert!(linked.is_empty());
        assert!(
            app.media_editors
                .get(&imported.id)
                .unwrap()
                .linked_posts
                .is_empty()
        );
    }

    #[test]
    fn refresh_counts_populates_dashboard_state() {
        let (db, project, tmp) = setup();
        let first = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "First",
            Some("Body"),
            vec!["rust".to_string()],
            vec!["article".to_string()],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let second = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Second",
            Some("Body"),
            vec!["lua".to_string()],
            vec!["aside".to_string()],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let mut second_post =
            bds_core::db::queries::post::get_post_by_id(db.conn(), &second.id).unwrap();
        second_post.status = PostStatus::Published;
        bds_core::db::queries::post::update_post(db.conn(), &second_post).unwrap();
        bds_core::engine::tag::discover_tags(db.conn(), tmp.path(), &project.id).unwrap();

        let source = tmp.path().join("tiny.png");
        std::fs::write(&source, tiny_png_bytes()).unwrap();
        media::import_media(
            db.conn(),
            tmp.path(),
            &project.id,
            &source,
            "tiny.png",
            Some("Tiny"),
            Some("Alt"),
            None,
            None,
            Some("en"),
            vec!["cover".to_string()],
        )
        .unwrap();

        let mut app = make_app(db, project, &tmp);
        let _ = app.refresh_counts();

        let dash = app.dashboard_state.expect("dashboard state should be set");
        let now = chrono::Utc::now();
        assert_eq!(dash.stats.total_posts, 2);
        assert_eq!(dash.stats.published_count, 1);
        assert_eq!(dash.stats.media_count, 1);
        assert_eq!(dash.stats.tag_count, 2);
        assert_eq!(dash.timeline.len(), 12);
        assert_eq!(
            dash.timeline.last().map(|month| month.year),
            Some(now.year())
        );
        assert_eq!(
            dash.timeline.last().map(|month| month.label.clone()),
            Some(month_abbreviation(now.month()))
        );
        assert_eq!(
            dash.timeline.iter().map(|month| month.count).sum::<usize>(),
            2
        );
        assert_eq!(dash.recent_posts.len(), 2);
        assert!(!dash.category_cloud.is_empty());
        assert_eq!(dash.recent_posts[0].title, "Second");
        assert_eq!(first.title, "First");
    }

    #[test]
    fn dashboard_timeline_uses_created_month_not_updated_month() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "March Post",
            Some("Body"),
            vec![],
            vec!["article".to_string()],
            None,
            Some("en"),
            None,
        )
        .unwrap();

        let march_created_at = chrono::Utc
            .with_ymd_and_hms(2026, 3, 15, 12, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        let april_updated_at = chrono::Utc
            .with_ymd_and_hms(2026, 4, 2, 12, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();

        let mut saved =
            bds_core::db::queries::post::get_post_by_id(db.conn(), &created.id).unwrap();
        saved.created_at = march_created_at;
        saved.updated_at = april_updated_at;
        bds_core::db::queries::post::update_post(db.conn(), &saved).unwrap();

        let app = make_app(db, project, &tmp);
        let dash = app.hydrate_dashboard_state();
        let march = dash
            .timeline
            .iter()
            .find(|month| month.year == 2026 && month.label == "Mar")
            .expect("march bucket should exist");
        let april = dash
            .timeline
            .iter()
            .find(|month| month.year == 2026 && month.label == "Apr")
            .expect("april bucket should exist");

        assert_eq!(march.count, 1);
        assert_eq!(april.count, 0);
    }

    #[test]
    fn save_project_persists_languages_and_blogmark_category() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);
        let mut state = app.hydrate_settings_state();
        state.project_name = "Localized Project".to_string();
        state.main_language = "de".to_string();
        state.blog_languages = vec!["de".to_string(), "en".to_string()];
        state.blogmark_category = "article".to_string();
        state.semantic_similarity_enabled = true;
        app.settings_state = Some(state);

        let _ = app.handle_settings_msg(SettingsMsg::SaveProject);

        let meta = bds_core::engine::meta::read_project_json(tmp.path()).unwrap();
        assert_eq!(meta.name, "Localized Project");
        assert_eq!(meta.main_language.as_deref(), Some("de"));
        assert_eq!(
            meta.blog_languages,
            vec!["de".to_string(), "en".to_string()]
        );
        assert_eq!(meta.blogmark_category.as_deref(), Some("article"));
        assert!(meta.semantic_similarity_enabled);
    }

    #[test]
    fn add_category_and_reset_defaults_updates_metadata_files() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);
        app.settings_state = Some(app.hydrate_settings_state());

        let _ = app.handle_settings_msg(SettingsMsg::AddCategoryNameChanged("news".to_string()));
        let _ = app.handle_settings_msg(SettingsMsg::AddCategory);

        let categories = bds_core::engine::meta::read_categories_json(tmp.path()).unwrap();
        assert!(categories.iter().any(|category| category == "news"));

        let _ = app.handle_settings_msg(SettingsMsg::ResetCategoriesToDefaults);

        let categories = bds_core::engine::meta::read_categories_json(tmp.path()).unwrap();
        assert_eq!(
            categories,
            vec![
                "article".to_string(),
                "aside".to_string(),
                "page".to_string(),
                "picture".to_string(),
            ]
        );
    }

    #[test]
    fn create_script_opens_editor_and_refreshes_sidebar() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);

        let _ = app.update(Message::CreateScript);

        assert_eq!(app.sidebar_scripts.len(), 1);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs[0].tab_type, crate::state::tabs::TabType::Scripts);
        assert_eq!(
            app.active_tab.as_deref(),
            Some(app.sidebar_scripts[0].id.as_str())
        );
        let editor = app
            .script_editors
            .get(&app.sidebar_scripts[0].id)
            .expect("script editor should be hydrated");
        assert_eq!(editor.entrypoint, "render");
        assert!(editor.content.contains("new script"));
    }

    #[test]
    fn create_template_opens_editor_and_refreshes_sidebar() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);

        let _ = app.update(Message::CreateTemplate);

        assert_eq!(app.sidebar_templates.len(), 1);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs[0].tab_type, crate::state::tabs::TabType::Templates);
        assert_eq!(
            app.active_tab.as_deref(),
            Some(app.sidebar_templates[0].id.as_str())
        );
        let editor = app
            .template_editors
            .get(&app.sidebar_templates[0].id)
            .expect("template editor should be hydrated");
        assert!(editor.content.is_empty());
    }

    #[test]
    fn query_sidebar_posts_filters_status_language_and_date_range() {
        let (_db, project, tmp) = setup();
        let db_path = tmp.path().join("sidebar.db");
        let mut db = Database::open(&db_path).unwrap();
        db.migrate().unwrap();
        ensure_fts_tables(db.conn()).unwrap();
        insert_project(db.conn(), &project).unwrap();

        let draft = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Draft German",
            Some("Body"),
            vec![],
            vec!["article".to_string()],
            None,
            Some("de"),
            None,
        )
        .unwrap();
        let published = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Published English",
            Some("Body"),
            vec![],
            vec!["article".to_string()],
            None,
            Some("en"),
            None,
        )
        .unwrap();

        let mut published_saved =
            bds_core::db::queries::post::get_post_by_id(db.conn(), &published.id).unwrap();
        published_saved.status = PostStatus::Published;
        published_saved.created_at = chrono::Utc
            .with_ymd_and_hms(2026, 4, 12, 8, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        bds_core::db::queries::post::update_post(db.conn(), &published_saved).unwrap();

        let mut draft_saved =
            bds_core::db::queries::post::get_post_by_id(db.conn(), &draft.id).unwrap();
        draft_saved.created_at = chrono::Utc
            .with_ymd_and_hms(2025, 1, 5, 8, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        bds_core::db::queries::post::update_post(db.conn(), &draft_saved).unwrap();

        let filter = PostFilter {
            status_filter: Some("published".to_string()),
            language_filter: Some("en".to_string()),
            from_date: "2026-04-01".to_string(),
            to_date: "2026-04-30".to_string(),
            ..PostFilter::default()
        };

        let posts = BdsApp::query_sidebar_posts_blocking(
            db_path.as_path(),
            &project.id,
            "en",
            &filter,
            false,
            50,
            0,
        );

        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].id, published.id);
    }

    #[test]
    fn regenerate_project_thumbnails_recreates_missing_files() {
        let (db, project, tmp) = setup();
        let source = tmp.path().join("tiny.png");
        std::fs::write(&source, tiny_png_bytes()).unwrap();
        let media = media::import_media(
            db.conn(),
            tmp.path(),
            &project.id,
            &source,
            "tiny.png",
            Some("Tiny"),
            Some("Alt"),
            None,
            None,
            Some("en"),
            vec![],
        )
        .unwrap();

        let small_thumb = tmp.path().join(bds_core::util::paths::thumbnail_path(
            &media.id, "small", "webp",
        ));
        std::fs::remove_file(&small_thumb).unwrap();
        assert!(!small_thumb.exists());

        let regenerated = BdsApp::regenerate_project_thumbnails(
            &db,
            tmp.path(),
            &project.id,
            |_index, _total, _name| {},
        )
        .unwrap();

        assert_eq!(regenerated, 1);
        assert!(small_thumb.exists());
    }
}
