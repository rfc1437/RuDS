use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use iced::{Element, Subscription, Task};

use bds_core::db::Database;
use bds_core::engine::task::{TaskId, TaskManager, TaskStatus};
use bds_core::engine;
use bds_core::i18n::{detect_os_locale, UiLocale};
use bds_core::model::{Media, Post, Project, PublishingPreferences, Script, SshMode, Template};

use crate::i18n::{t, tw};
use crate::platform::menu::{self, MenuAction, MenuRegistry};
use crate::state::navigation::{
    handle_activity_click, OutputEntry, PanelTab, SidebarView, TaskSnapshot,
};
use crate::state::sidebar_filter::{
    CalendarMonth, CalendarYear, MediaFilter, PostFilter,
};
use crate::state::tabs::{self, Tab, TabType};
use crate::state::toast::{Toast, ToastLevel};
use crate::views::{
    modal, workspace,
    post_editor::{LinkedMediaItem, PostEditorMsg, PostEditorState, ResolvedPostLink},
    media_editor::{MediaEditorState, MediaEditorMsg},
    template_editor::{TemplateEditorState, TemplateEditorMsg},
    script_editor::{ScriptEditorState, ScriptEditorMsg},
    tags_view::{self, TagsViewState, TagsMsg},
    settings_view::{SettingsViewState, SettingsMsg},
    dashboard::DashboardState,
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
    CreateProject { name: String, data_path: Option<PathBuf> },
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
    SetPostCalendarYear(Option<i32>),
    SetPostCalendarMonth(Option<u32>),
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

    // Blog actions (dispatched to engine)
    RebuildDatabase,
    ReindexText,
    RegenerateCalendar,
    ValidateTranslations,
    ValidateMedia,
    GenerateSite,
    RunMetadataDiff,
    EngineTaskDone { task_id: TaskId, label: String, result: Result<String, String> },

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

    Noop,
    InitMenuBar,
}

enum PersistedPostState {
    Canonical(Post),
    Translation(bds_core::model::PostTranslation),
}

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
            if state.excerpt.is_empty() { None } else { Some(state.excerpt.as_str()) },
            Some(&state.content),
        )
        .map_err(|e| e.to_string())?;
        Ok(PersistedPostState::Translation(translation))
    } else {
        let post = engine::post::update_post(
            db.conn(),
            data_dir,
            &state.post_id,
            Some(&state.title),
            Some(&state.slug),
            Some(if state.excerpt.is_empty() { None } else { Some(state.excerpt.as_str()) }),
            Some(&state.content),
            Some(state.tags.clone()),
            Some(state.categories.clone()),
            Some(if state.author.is_empty() { None } else { Some(state.author.as_str()) }),
            Some(if state.language.is_empty() { None } else { Some(state.language.as_str()) }),
            Some(if state.template_slug.is_empty() { None } else { Some(state.template_slug.as_str()) }),
            Some(state.do_not_translate),
        )
        .map_err(|e| e.to_string())?;
        Ok(PersistedPostState::Canonical(post))
    }
}

enum PersistedMediaState {
    Canonical { media: Media, tags: Vec<String> },
    Translation,
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
            if state.title.is_empty() { None } else { Some(state.title.as_str()) },
            if state.alt.is_empty() { None } else { Some(state.alt.as_str()) },
            if state.caption.is_empty() { None } else { Some(state.caption.as_str()) },
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
            Some(if state.title.is_empty() { None } else { Some(state.title.as_str()) }),
            Some(if state.alt.is_empty() { None } else { Some(state.alt.as_str()) }),
            Some(if state.caption.is_empty() { None } else { Some(state.caption.as_str()) }),
            Some(if state.author.is_empty() { None } else { Some(state.author.as_str()) }),
            Some(if state.language.is_empty() { None } else { Some(state.language.as_str()) }),
            Some(tags.clone()),
        )
        .map_err(|e| e.to_string())?;
        Ok(PersistedMediaState::Canonical { media, tags })
    }
}

fn save_template_editor_state_impl(
    db: &Database,
    project_id: &str,
    state: &TemplateEditorState,
) -> Result<Template, String> {
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

fn save_editor_settings_state_impl(
    db: &Database,
    state: &SettingsViewState,
) -> Result<(), String> {
    let now = bds_core::util::now_unix_ms();
    [
        bds_core::db::queries::setting::set_setting_value(db.conn(), "editor.default_mode", &state.default_mode, now),
        bds_core::db::queries::setting::set_setting_value(db.conn(), "editor.diff_view_style", &state.diff_view_style, now),
        bds_core::db::queries::setting::set_setting_value(db.conn(), "editor.wrap_long_lines", if state.wrap_long_lines { "true" } else { "false" }, now),
        bds_core::db::queries::setting::set_setting_value(db.conn(), "editor.hide_unchanged_regions", if state.hide_unchanged_regions { "true" } else { "false" }, now),
    ]
    .into_iter()
    .collect::<Result<Vec<_>, _>>()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::{persist_media_editor_state_impl, persist_post_editor_state_impl, save_editor_settings_state_impl, save_script_editor_state_impl, save_template_editor_state_impl, PersistedMediaState, PersistedPostState};
    use crate::views::media_editor::MediaEditorState;
    use crate::views::post_editor::PostEditorState;
    use crate::views::script_editor::ScriptEditorState;
    use crate::views::settings_view::SettingsViewState;
    use crate::views::template_editor::TemplateEditorState;
    use bds_core::db::fts::ensure_fts_tables;
    use bds_core::db::queries::project::insert_project;
    use bds_core::db::Database;
    use bds_core::engine::{media, post, script, template};
    use bds_core::model::{Project, ScriptKind, TemplateKind};
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
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A,
            0x00, 0x00, 0x00, 0x0D, b'I', b'H', b'D', b'R',
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
            0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
            0x89, 0x00, 0x00, 0x00, 0x0D, b'I', b'D', b'A',
            b'T', 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
            0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99,
            0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, b'I', b'E',
            b'N', b'D', 0xAE, 0x42, 0x60, 0x82,
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
        std::fs::write(tempdir.path().join("meta/project.json"), "{}\n").unwrap();
        std::fs::write(tempdir.path().join("meta/publishing.json"), "{}\n").unwrap();
        (db, project, tempdir)
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
        ).unwrap();

        let editor_post = bds_core::db::queries::post::get_post_by_id(db.conn(), &created.id).unwrap();
        let mut editor = PostEditorState::from_post(&editor_post, &["en".to_string(), "de".to_string()], &[], Vec::new(), Vec::new(), Vec::new());
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
        ).unwrap();

        let media_record = bds_core::db::queries::media::get_media_by_id(db.conn(), &imported.id).unwrap();
        let mut editor = MediaEditorState::from_media(&media_record, &["en".to_string()], &[]);
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
        ).unwrap();

        let template_record = bds_core::db::queries::template::get_template_by_id(db.conn(), &created.id).unwrap();
        let mut editor = TemplateEditorState::from_template(&template_record);
        editor.title = "Updated Template".to_string();
        editor.content = "<main>{{ title }}</main>".to_string();

        let saved_template = save_template_editor_state_impl(&db, &project.id, &editor).unwrap();
        assert_eq!(saved_template.title, "Updated Template");

        let saved = bds_core::db::queries::template::get_template_by_id(db.conn(), &created.id).unwrap();
        assert_eq!(saved.title, "Updated Template");
        assert_eq!(saved.content.as_deref(), Some("<main>{{ title }}</main>"));
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
        ).unwrap();

        let script_record = bds_core::db::queries::script::get_script_by_id(db.conn(), &created.id).unwrap();
        let mut editor = ScriptEditorState::from_script(&script_record);
        editor.title = "Updated Script".to_string();
        editor.content = "function main()\n  return 'lua'\nend".to_string();
        editor.entrypoint = "main".to_string();

        let saved_script = save_script_editor_state_impl(&db, &project.id, &editor).unwrap();
        assert_eq!(saved_script.title, "Updated Script");

        let saved = bds_core::db::queries::script::get_script_by_id(db.conn(), &created.id).unwrap();
        assert_eq!(saved.title, "Updated Script");
        assert_eq!(saved.content.as_deref(), Some("function main()\n  return 'lua'\nend"));
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

        let wrap = bds_core::db::queries::setting::get_setting_by_key(db.conn(), "editor.wrap_long_lines").unwrap();
        let hide = bds_core::db::queries::setting::get_setting_by_key(db.conn(), "editor.hide_unchanged_regions").unwrap();
        let diff = bds_core::db::queries::setting::get_setting_by_key(db.conn(), "editor.diff_view_style").unwrap();

        assert_eq!(wrap.value, "false");
        assert_eq!(hide.value, "true");
        assert_eq!(diff.value, "side-by-side");
    }
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
    _menu_bar: muda::Menu,
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

    // Editor states (keyed by entity id)
    post_editors: HashMap<String, PostEditorState>,
    media_editors: HashMap<String, MediaEditorState>,
    template_editors: HashMap<String, TemplateEditorState>,
    script_editors: HashMap<String, ScriptEditorState>,
    tags_view_state: Option<TagsViewState>,
    settings_state: Option<SettingsViewState>,
    dashboard_state: Option<DashboardState>,
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
                match engine::project::ensure_default_project(
                    db.conn(),
                    Some(&default_data),
                ) {
                    Ok(project) => {
                        Task::done(Message::ProjectsLoaded(vec![project]))
                    }
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
        let init_task = Task::batch([init_task, Task::done(Message::InitMenuBar)]);
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
                _menu_bar: menu_bar,
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
                post_editors: HashMap::new(),
                media_editors: HashMap::new(),
                template_editors: HashMap::new(),
                script_editors: HashMap::new(),
                tags_view_state: None,
                settings_state: None,
                dashboard_state: None,
            },
            init_task,
        )
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
                    && matches!(
                        new_view,
                        SidebarView::Posts | SidebarView::Pages
                    );
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
                let idx = tabs::open_tab(&mut self.tabs, tab);
                if let Some(t) = self.tabs.get(idx) {
                    self.active_tab = Some(t.id.clone());
                    let tab_clone = t.clone();
                    self.load_editor_for_tab(&tab_clone);
                }
                self.enforce_panel_tab_fallback();
                self.sync_menu_state();
                Task::none()
            }
            Message::CloseTab(id) => {
                if let Some(next_idx) = tabs::close_tab(&mut self.tabs, &id) {
                    self.active_tab = self.tabs.get(next_idx).map(|t| t.id.clone());
                } else {
                    self.active_tab = None;
                }
                self.enforce_panel_tab_fallback();
                self.sync_menu_state();
                Task::none()
            }
            Message::SelectTab(id) => {
                if self.tabs.iter().any(|t| t.id == id) {
                    self.active_tab = Some(id);
                }
                self.enforce_panel_tab_fallback();
                Task::none()
            }
            Message::PinTab(id) => {
                tabs::pin_tab(&mut self.tabs, &id);
                Task::none()
            }
            Message::ClearTabs => {
                self.tabs.clear();
                self.active_tab = None;
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
                        self.add_output(&format!("Metadata sync failed: {e}"));
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
                            self.active_project = self.projects.iter().find(|p| p.id == project_id).cloned();
                            self.data_dir = self
                                .active_project
                                .as_ref()
                                .and_then(|p| p.data_path.as_ref())
                                .map(PathBuf::from);
                            // Per metadata.allium StartupSync
                            if let Some(data_dir) = self.data_dir.clone() {
                                let _ = engine::meta::startup_sync(&data_dir);
                                if let Ok(meta) = engine::meta::read_project_json(&data_dir) {
                                    let main_lang = meta.main_language.unwrap_or_else(|| "en".to_string());
                                    self.content_language = main_lang.clone();
                                    self.blog_languages = meta.blog_languages;
                                    if !self.blog_languages.contains(&main_lang) {
                                        self.blog_languages.insert(0, main_lang);
                                    }
                                }
                            }
                            let name = self.active_project.as_ref().map(|p| p.name.clone()).unwrap_or_default();
                            self.notify(ToastLevel::Success, &tw(self.ui_locale, "projectSelector.toast.switched", &[("name", &name)]));
                        }
                        Err(_) => {
                            self.notify(ToastLevel::Error, &t(self.ui_locale, "projectSelector.toast.switchFailed"));
                        }
                    }
                }
                self.sync_menu_state();
                Task::none()
            }
            Message::ProjectSwitched(result) => {
                match result {
                    Ok(name) => self.notify(ToastLevel::Success, &tw(self.ui_locale, "projectSelector.toast.switched", &[("name", &name)])),
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
                    match engine::project::create_project(
                        db.conn(),
                        &name,
                        path_str.as_deref(),
                    ) {
                        Ok(project) => {
                            let _ = engine::project::set_active_project(db.conn(), &project.id);
                            self.projects = engine::project::list_projects(db.conn()).unwrap_or_default();
                            self.active_project = Some(project.clone());
                            self.data_dir = project.data_path.as_ref().map(PathBuf::from);
                            let msg = tw(self.ui_locale, "projectSelector.toast.created", &[("name", &project.name)]);
                            self.notify(ToastLevel::Success, &msg);
                        }
                        Err(_) => {
                            self.notify(ToastLevel::Error, &t(self.ui_locale, "projectSelector.toast.createFailed"));
                        }
                    }
                }
                Task::none()
            }
            Message::ProjectCreated(result) => {
                match result {
                    Ok(name) => self.notify(ToastLevel::Success, &tw(self.ui_locale, "projectSelector.toast.created", &[("name", &name)])),
                    Err(msg) => self.notify(ToastLevel::Error, &msg),
                }
                Task::none()
            }
            Message::DeleteProject(project_id) => {
                if let Some(ref db) = self.db {
                    let data_path = self.projects.iter()
                        .find(|p| p.id == project_id)
                        .and_then(|p| p.data_path.as_ref())
                        .map(PathBuf::from);
                    match engine::project::delete_project(db.conn(), &project_id, data_path.as_deref()) {
                        Ok(()) => {
                            self.projects.retain(|p| p.id != project_id);
                        }
                        Err(_) => {
                            self.notify(ToastLevel::Error, &t(self.ui_locale, "projectSelector.toast.deleteFailed"));
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

            // ── Tasks ──
            Message::TaskTick => {
                self.refresh_task_snapshots();
                Task::none()
            }

            // ── macOS lifecycle ──
            Message::FileOpenRequested(_path) => {
                // File open handling deferred to later milestones
                Task::none()
            }
            Message::UrlOpenRequested(_url) => {
                // URL open handling deferred to later milestones
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
            Message::RebuildDatabase => {
                self.spawn_engine_task(
                    "engine.rebuildStarted",
                    |db_path, project_id, data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let on_progress: engine::rebuild::ProgressFn = Arc::new(move |pct, msg| {
                            tm.report_progress(tid, Some(pct), Some(msg.to_string()));
                        });
                        let report = engine::rebuild::rebuild_from_filesystem_with_progress(
                            db.conn(), &data_dir, &project_id, Some(on_progress),
                        ).map_err(|e| e.to_string())?;
                        let posts = report.posts_created + report.posts_updated;
                        let media = report.media_created + report.media_updated;
                        let templates = report.templates_created + report.templates_updated;
                        let scripts = report.scripts_created + report.scripts_updated;
                        Ok(format!("posts={posts}, media={media}, templates={templates}, scripts={scripts}"))
                    },
                )
            }
            Message::ReindexText => {
                self.spawn_engine_task(
                    "engine.reindexStarted",
                    |db_path, project_id, data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        tm.report_progress(tid, Some(0.0), Some("Reading project config...".into()));
                        let main_lang = engine::meta::read_project_json(&data_dir)
                            .ok()
                            .and_then(|m| m.main_language)
                            .unwrap_or_else(|| "en".to_string());
                        let tm2 = Arc::clone(&tm);
                        let on_item: engine::search::ItemProgressFn = Box::new(move |current, total, name| {
                            let pct = if total > 0 { current as f32 / total as f32 } else { 1.0 };
                            let msg = format!("Indexing: {current}/{total} \u{2014} {name}");
                            tm2.report_progress(tid, Some(pct), Some(msg));
                        });
                        let report = engine::search::reindex_all_with_progress(
                            db.conn(), &project_id, &main_lang, Some(on_item),
                        ).map_err(|e| e.to_string())?;
                        Ok(format!("posts={}, media={}", report.posts_indexed, report.media_indexed))
                    },
                )
            }
            Message::RegenerateCalendar => {
                self.spawn_engine_task(
                    "engine.calendarStarted",
                    |db_path, project_id, data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        tm.report_progress(tid, Some(0.20), Some("Loading posts...".into()));
                        engine::calendar::regenerate_calendar(db.conn(), &data_dir, &project_id)
                            .map_err(|e| e.to_string())?;
                        tm.report_progress(tid, Some(0.90), Some("Writing calendar JSON...".into()));
                        Ok("done".to_string())
                    },
                )
            }
            Message::ValidateTranslations => {
                self.open_singleton_tab(TabType::TranslationValidation, "tabBar.translationValidation");
                self.spawn_engine_task(
                    "engine.validateTranslationsStarted",
                    |db_path, project_id, data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let meta = engine::meta::read_project_json(&data_dir)
                            .map_err(|e| e.to_string())?;
                        let main_lang = meta.main_language.as_deref().unwrap_or("en");
                        let blog_langs = meta.blog_languages.clone();
                        let tm2 = Arc::clone(&tm);
                        let on_item: engine::validate_translations::ItemProgressFn = Box::new(move |current, total, name| {
                            let pct = if total > 0 { current as f32 / total as f32 } else { 1.0 };
                            let msg = format!("Checking: {current}/{total} \u{2014} {name}");
                            tm2.report_progress(tid, Some(pct), Some(msg));
                        });
                        let report = engine::validate_translations::validate_translations_with_progress(
                            db.conn(), &data_dir, &project_id, &blog_langs, main_lang, Some(on_item),
                        ).map_err(|e| e.to_string())?;
                        Ok(format!("db_issues={}, fs_issues={}", report.db_issues.len(), report.fs_issues.len()))
                    },
                )
            }
            Message::ValidateMedia => {
                self.spawn_engine_task(
                    "engine.validateMediaStarted",
                    |db_path, project_id, _data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        let on_item: engine::validate_media::ProgressFn = Box::new(move |current, total, name| {
                            let pct = if total > 0 { current as f32 / total as f32 } else { 1.0 };
                            let msg = format!("Checking: {current}/{total} \u{2014} {name}");
                            tm.report_progress(tid, Some(pct), Some(msg));
                        });
                        let report = engine::validate_media::validate_media(
                            db.conn(), &_data_dir, &project_id, Some(on_item),
                        ).map_err(|e| e.to_string())?;
                        Ok(format!("checked={}, issues={}", report.total_checked, report.issues.len()))
                    },
                )
            }
            Message::GenerateSite => {
                self.spawn_engine_task(
                    "engine.generateSiteStarted",
                    |db_path, project_id, data_dir, tm, tid| {
                        let db = Database::open(&db_path).map_err(|e| e.to_string())?;
                        tm.report_progress(tid, Some(0.20), Some("Generating calendar...".into()));
                        engine::calendar::regenerate_calendar(db.conn(), &data_dir, &project_id)
                            .map_err(|e| e.to_string())?;
                        tm.report_progress(tid, Some(0.90), Some("Calendar written".into()));
                        Ok("done".to_string())
                    },
                )
            }
            Message::RunMetadataDiff => {
                self.open_singleton_tab(TabType::MetadataDiff, "tabBar.metadataDiff");
                Task::none()
            }
            Message::EngineTaskDone { task_id, label, result } => {
                match &result {
                    Ok(detail) => {
                        self.task_manager.complete(task_id);
                        self.notify(ToastLevel::Success, &format!("{label}: {detail}"));
                    }
                    Err(err) => {
                        self.task_manager.fail(task_id, err.clone());
                        self.notify(ToastLevel::Error, &format!("{label} failed: {err}"));
                    }
                }
                let sidebar_task = self.refresh_counts();
                self.refresh_task_snapshots();
                sidebar_task
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
            Message::LoadMorePosts => {
                self.load_more_sidebar_posts()
            }
            Message::LoadMoreMedia => {
                self.load_more_sidebar_media()
            }

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
                    modal::ConfirmAction::DeleteTemplate(id) => self.delete_template_editor(&id),
                    modal::ConfirmAction::DeleteTag(id) => self.delete_tag(&id),
                    modal::ConfirmAction::MergeTags { source, target } => {
                        self.merge_tags(&source, &target)
                    }
                }
            }

            // ── Editor view messages ──
            Message::PostEditor(msg) => {
                enum DeferredPostAction {
                    None,
                    Save(String),
                    Publish(String),
                    Duplicate(String),
                    Discard(String),
                    ShowDelete { tab_id: String, name: String },
                    OpenInsertLink(String),
                    OpenInsertMedia { post_id: String, link_only: bool },
                    OpenGallery(String),
                    OpenLinkedMedia(String),
                    UnlinkLinkedMedia { post_id: String, media_id: String },
                    InsertSelectedLink { post_id: String, linked_post_id: String },
                    CreateLinkedPost(String),
                    InsertSelectedMedia { post_id: String, media_id: String },
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
                if let Some(tab_id) = self.active_tab.clone() {
                    if let Some(state) = self.post_editors.get_mut(&tab_id) {
                        match msg {
                            PostEditorMsg::TitleChanged(s) => { state.title = s; state.is_dirty = true; }
                            PostEditorMsg::SlugChanged(s) => { state.slug = s; state.is_dirty = true; }
                            PostEditorMsg::ExcerptChanged(s) => { state.excerpt = s; state.is_dirty = true; }
                            PostEditorMsg::ContentChanged(new_text) => {
                                state.content = new_text;
                                state.is_dirty = true;
                            }
                            PostEditorMsg::AuthorChanged(s) => { state.author = s; state.is_dirty = true; }
                            PostEditorMsg::LanguageChanged(s) => { state.language = s; state.is_dirty = true; }
                            PostEditorMsg::TemplateSlugChanged(s) => { state.template_slug = s; state.is_dirty = true; }
                            PostEditorMsg::ToggleDoNotTranslate(b) => { state.do_not_translate = b; state.is_dirty = true; }
                            PostEditorMsg::ToggleMetadata => { state.metadata_expanded = !state.metadata_expanded; }
                            PostEditorMsg::ToggleExcerpt => { state.excerpt_expanded = !state.excerpt_expanded; }
                            PostEditorMsg::SwitchLanguage(lang) => { state.switch_language(&lang); }
                            PostEditorMsg::TagsInputChanged(s) => { state.tags_input = s; }
                            PostEditorMsg::TagsInputSubmit => {
                                let tag = state.tags_input.trim().to_string();
                                if !tag.is_empty() && !state.tags.contains(&tag) {
                                    state.tags.push(tag);
                                    state.is_dirty = true;
                                }
                                state.tags_input.clear();
                            }
                            PostEditorMsg::RemoveTag(tag) => {
                                state.tags.retain(|t| t != &tag);
                                state.is_dirty = true;
                            }
                            PostEditorMsg::CategoriesInputChanged(s) => { state.categories_input = s; }
                            PostEditorMsg::CategoriesInputSubmit => {
                                let cat = state.categories_input.trim().to_string();
                                if !cat.is_empty() && !state.categories.contains(&cat) {
                                    state.categories.push(cat);
                                    state.is_dirty = true;
                                }
                                state.categories_input.clear();
                            }
                            PostEditorMsg::RemoveCategory(cat) => {
                                state.categories.retain(|c| c != &cat);
                                state.is_dirty = true;
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
                }

                match deferred {
                    DeferredPostAction::None => Task::none(),
                    DeferredPostAction::Save(tab_id) => self.save_post_editor(&tab_id),
                    DeferredPostAction::Publish(tab_id) => self.publish_post_editor(&tab_id),
                    DeferredPostAction::Duplicate(tab_id) => self.duplicate_post_editor(&tab_id),
                    DeferredPostAction::Discard(tab_id) => self.discard_post_editor(&tab_id),
                    DeferredPostAction::ShowDelete { tab_id, name } => Task::done(Message::ShowModal(
                        modal::ModalState::ConfirmDelete {
                            entity_name: name,
                            references: Vec::new(),
                            on_confirm: modal::ConfirmAction::DeletePost(tab_id),
                        },
                    )),
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
                                self.notify(ToastLevel::Error, &format!("Failed to unlink media: {err}"));
                                return Task::none();
                            }
                            self.refresh_post_relationships(&post_id);
                        }
                        Task::none()
                    }
                    DeferredPostAction::InsertSelectedLink { post_id, linked_post_id } => {
                        self.insert_selected_post_link(&post_id, &linked_post_id)
                    }
                    DeferredPostAction::CreateLinkedPost(post_id) => self.insert_created_post_link(&post_id),
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
                            if let Some(markdown) = modal::external_link_markdown(&external_url, &external_text) {
                                self.insert_markdown_into_post(&post_id, &markdown)
                            } else {
                                self.notify(ToastLevel::Error, &t(self.ui_locale, "modal.postInsertLink.urlRequired"));
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
                if let Some(tab_id) = self.active_tab.clone() {
                    if let Some(state) = self.media_editors.get_mut(&tab_id) {
                        match msg {
                            MediaEditorMsg::TitleChanged(s) => { state.title = s; state.is_dirty = true; }
                            MediaEditorMsg::AltChanged(s) => { state.alt = s; state.is_dirty = true; }
                            MediaEditorMsg::CaptionChanged(s) => { state.caption = s; state.is_dirty = true; }
                            MediaEditorMsg::AuthorChanged(s) => { state.author = s; state.is_dirty = true; }
                            MediaEditorMsg::LanguageChanged(s) => { state.language = s; state.is_dirty = true; }
                            MediaEditorMsg::TagsChanged(s) => { state.tags_input = s; state.is_dirty = true; }
                            MediaEditorMsg::SwitchLanguage(lang) => { state.switch_language(&lang); }
                            MediaEditorMsg::Save => {
                                return self.save_media_editor(&tab_id);
                            }
                            MediaEditorMsg::Delete => {
                                let name = state.title.clone();
                                return Task::done(Message::ShowModal(modal::ModalState::ConfirmDelete {
                                    entity_name: name,
                                    references: Vec::new(),
                                    on_confirm: modal::ConfirmAction::DeleteMedia(tab_id),
                                }));
                            }
                        }
                        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == *state.media_id.as_str()) {
                            tab.is_dirty = state.is_dirty;
                        }
                    }
                }
                Task::none()
            }
            Message::TemplateEditor(msg) => {
                if let Some(tab_id) = self.active_tab.clone() {
                    if let Some(state) = self.template_editors.get_mut(&tab_id) {
                        match msg {
                            TemplateEditorMsg::TitleChanged(s) => { state.title = s; state.is_dirty = true; }
                            TemplateEditorMsg::SlugChanged(s) => { state.slug = s; state.is_dirty = true; }
                            TemplateEditorMsg::KindChanged(k) => { state.kind = k.0; state.is_dirty = true; }
                            TemplateEditorMsg::EnabledChanged(b) => { state.enabled = b; state.is_dirty = true; }
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
                                        Ok(()) => { st.validation_error = None; }
                                        Err(e) => { st.validation_error = Some(e); }
                                    }
                                }
                            }
                            TemplateEditorMsg::Delete => {
                                let name = state.title.clone();
                                return Task::done(Message::ShowModal(modal::ModalState::ConfirmDelete {
                                    entity_name: name,
                                    references: Vec::new(),
                                    on_confirm: modal::ConfirmAction::DeleteTemplate(tab_id),
                                }));
                            }
                        }
                        if let Some(st) = self.template_editors.get(&tab_id) {
                            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                                tab.is_dirty = st.is_dirty;
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::ScriptEditor(msg) => {
                if let Some(tab_id) = self.active_tab.clone() {
                    if let Some(state) = self.script_editors.get_mut(&tab_id) {
                        match msg {
                            ScriptEditorMsg::TitleChanged(s) => { state.title = s; state.is_dirty = true; }
                            ScriptEditorMsg::SlugChanged(s) => { state.slug = s; state.is_dirty = true; }
                            ScriptEditorMsg::KindChanged(k) => { state.kind = k.0; state.is_dirty = true; }
                            ScriptEditorMsg::EntrypointChanged(s) => { state.entrypoint = s; state.is_dirty = true; }
                            ScriptEditorMsg::EnabledChanged(b) => { state.enabled = b; state.is_dirty = true; }
                            ScriptEditorMsg::ContentChanged(new_text) => {
                                state.discovered_entrypoints = engine::script::discover_entrypoints(&new_text);
                                state.content = new_text;
                                state.is_dirty = true;
                            }
                            ScriptEditorMsg::Save => {
                                return self.save_script_editor(&tab_id);
                            }
                            ScriptEditorMsg::CheckSyntax => {
                                if let Some(st) = self.script_editors.get_mut(&tab_id) {
                                    match engine::script::validate_script_syntax(&st.content) {
                                        Ok(()) => { st.validation_error = None; }
                                        Err(e) => { st.validation_error = Some(e); }
                                    }
                                }
                            }
                            ScriptEditorMsg::Run => {
                                self.notify(ToastLevel::Info, &t(self.ui_locale, "editor.scriptRunNotYet"));
                            }
                            ScriptEditorMsg::Delete => {
                                let name = state.title.clone();
                                return Task::done(Message::ShowModal(modal::ModalState::ConfirmDelete {
                                    entity_name: name,
                                    references: Vec::new(),
                                    on_confirm: modal::ConfirmAction::DeleteScript(tab_id),
                                }));
                            }
                        }
                        if let Some(st) = self.script_editors.get(&tab_id) {
                            if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tab_id) {
                                tab.is_dirty = st.is_dirty;
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::Tags(msg) => {
                self.handle_tags_msg(msg)
            }
            Message::Settings(msg) => {
                self.handle_settings_msg(msg)
            }

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
                                    if let Ok(raw) = std::fs::read_to_string(&path) {
                                        if let Ok((_fm, body)) =
                                            bds_core::util::frontmatter::read_translation_file(&raw)
                                        {
                                            tr.content = Some(body);
                                        }
                                    }
                                }
                            }
                        }
                        let (outlinks, backlinks) = self.load_post_links(&post.id);
                        let linked_media = self.load_post_media_items(&post.id);
                        let state = PostEditorState::from_post(
                            &post,
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
                        let state = MediaEditorState::from_media(&media, &self.blog_languages, &translations);
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
                menu::init_menu_for_nsapp(&self._menu_bar);
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
            &self.post_editors,
            &self.media_editors,
            &self.template_editors,
            &self.script_editors,
            self.tags_view_state.as_ref(),
            self.settings_state.as_ref(),
            self.dashboard_state.as_ref(),
        )
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let menu_sub = menu::menu_subscription();

        let task_tick = iced::time::every(std::time::Duration::from_millis(500))
            .map(|_| Message::TaskTick);

        let toast_tick = if !self.toasts.is_empty() {
            iced::time::every(std::time::Duration::from_millis(250))
                .map(|_| Message::ExpireToasts)
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
            MenuAction::NewPost => {
                if let (Some(db), Some(project), Some(data_dir)) =
                    (&self.db, &self.active_project, &self.data_dir)
                {
                    let display_title = t(self.ui_locale, "post.untitled");
                    match engine::post::create_post(
                        db.conn(),
                        data_dir,
                        &project.id,
                        "",
                        Some(""),
                        Vec::new(),
                        Vec::new(),
                        None,
                        None,
                        None,
                    ) {
                        Ok(post) => {
                            let tab = Tab {
                                id: post.id.clone(),
                                tab_type: TabType::Post,
                                title: display_title.to_string(),
                                is_transient: true,
                                is_dirty: false,
                            };
                            let idx = tabs::open_tab(&mut self.tabs, tab);
                            if let Some(t) = self.tabs.get(idx) {
                                self.active_tab = Some(t.id.clone());
                            }
                        }
                        Err(e) => {
                            self.add_output(&format!("Failed to create post: {e}"));
                        }
                    }
                }
                Task::none()
            }
            MenuAction::ImportMedia => crate::platform::dialog::pick_media_files(
                t(self.ui_locale, "dialog.importMedia"),
                t(self.ui_locale, "dialog.imageFilter"),
            ),
            MenuAction::Save => Task::none(), // Disabled in M2
            MenuAction::OpenInBrowser => Task::none(),
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
            MenuAction::ViewPosts => {
                Task::done(Message::SetActiveView(SidebarView::Posts))
            }
            MenuAction::ViewMedia => {
                Task::done(Message::SetActiveView(SidebarView::Media))
            }
            MenuAction::ToggleSidebar => {
                Task::done(Message::ToggleSidebar)
            }
            MenuAction::TogglePanel => {
                Task::done(Message::TogglePanel)
            }
            // Blog
            MenuAction::PublishSelected => Task::none(), // Disabled in M2
            MenuAction::PreviewPost => Task::none(),     // Disabled in M2
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
                    self.notify(ToastLevel::Warning, &t(self.ui_locale, "engine.fillMissingTranslationsOffline"));
                } else {
                    self.notify(ToastLevel::Warning, &t(self.ui_locale, "engine.fillMissingTranslationsNoAi"));
                }
                Task::none()
            }
            MenuAction::GenerateSitemap => Task::done(Message::GenerateSite),
            MenuAction::ValidateSite => {
                self.open_singleton_tab(TabType::SiteValidation, "tabBar.siteValidation");
                Task::none()
            }
            MenuAction::UploadSite => {
                if self.offline_mode {
                    self.notify(ToastLevel::Warning, &t(self.ui_locale, "engine.uploadOffline"));
                } else if let Some(data_dir) = &self.data_dir {
                    let pub_prefs = engine::meta::read_publishing_json(data_dir).ok();
                    let has_creds = pub_prefs
                        .as_ref()
                        .map(|p| {
                            p.ssh_host.as_ref().map_or(false, |h| !h.is_empty())
                                && p.ssh_user.as_ref().map_or(false, |u| !u.is_empty())
                        })
                        .unwrap_or(false);
                    if !has_creds {
                        self.notify(ToastLevel::Warning, &t(self.ui_locale, "engine.uploadMissingCredentials"));
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

    fn refresh_task_snapshots(&mut self) {
        self.task_snapshots = self
            .task_manager
            .snapshots()
            .into_iter()
            .map(|(id, label, status, progress, message)| {
                let status_str = match &status {
                    TaskStatus::Pending => "pending".to_string(),
                    TaskStatus::Running => "running".to_string(),
                    TaskStatus::Completed => "completed".to_string(),
                    TaskStatus::Failed(e) => format!("failed: {e}"),
                    TaskStatus::Cancelled => "cancelled".to_string(),
                };
                TaskSnapshot {
                    id,
                    label,
                    status: status_str,
                    progress,
                    message,
                }
            })
            .collect();
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

    /// Spawn a blocking engine operation on a background thread via TaskManager.
    ///
    /// Returns `Task::none()` if no active project/db/data_dir.
    /// Otherwise registers the task, logs the start message, and returns an
    /// async `Task` that opens a fresh DB connection on a worker thread.
    ///
    /// The closure receives `(db_path, project_id, data_dir, task_manager, task_id)`.
    /// Use `task_manager.report_progress(task_id, percent, message)` for live updates.
    fn spawn_engine_task<F>(
        &mut self,
        label_key: &str,
        work: F,
    ) -> Task<Message>
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
                tokio::task::spawn_blocking(move || work(db_path, project_id, data_dir, tm, task_id))
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
            self.post_count = bds_core::db::queries::post::count_posts_by_project(
                db.conn(),
                &project.id,
            )
            .unwrap_or(0) as usize;
            self.media_count = bds_core::db::queries::media::count_media_by_project(
                db.conn(),
                &project.id,
            )
            .unwrap_or(0) as usize;

            self.sidebar_scripts = bds_core::db::queries::script::list_scripts_by_project(
                db.conn(),
                &project.id,
            )
            .unwrap_or_default();
            self.sidebar_templates = bds_core::db::queries::template::list_templates_by_project(
                db.conn(),
                &project.id,
            )
            .unwrap_or_default();

            // Read pico theme from project metadata for status bar badge
            if let Some(ref data_dir) = self.data_dir {
                if let Ok(meta) = engine::meta::read_project_json(data_dir) {
                    if let Some(theme) = meta.pico_theme {
                        self.theme_badge = theme;
                    }
                }
            }
        }

        // Refresh sidebar data with current filters (async — off main thread)
        let t1 = self.refresh_sidebar_posts();
        let t2 = self.refresh_sidebar_media();
        self.refresh_filter_metadata();
        Task::batch([t1, t2])
    }

    /// Number of items to load per sidebar page.
    /// Matches the TypeScript app's limit of 500 for initial load.
    const SIDEBAR_PAGE_SIZE: i64 = 500;

    /// Refresh only sidebar posts using current filter state (async).
    fn refresh_sidebar_posts(&mut self) -> Task<Message> {
        let (Some(_), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };

        use bds_core::db::queries::post::PostFilterParams;

        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let filter = match self.sidebar_view {
            SidebarView::Pages => &self.page_filter,
            _ => &self.post_filter,
        };
        let is_pages = self.sidebar_view == SidebarView::Pages;

        let params = PostFilterParams {
            search_query: filter.search_query.clone(),
            year: filter.calendar.selected_year,
            month: filter.calendar.selected_month,
            tags: filter.tag_filter.clone(),
            categories: filter.category_filter.clone(),
            exclude_pages: !is_pages,
            pages_only: is_pages,
        };

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let db = Database::open(&db_path).ok();
                    db.and_then(|db| {
                        bds_core::db::queries::post::list_posts_filtered(
                            db.conn(), &project_id, &params,
                            Self::SIDEBAR_PAGE_SIZE + 1, 0,
                        ).ok()
                    }).unwrap_or_default()
                }).await.unwrap_or_default()
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
                    let items = db.and_then(|db| {
                        bds_core::db::queries::media::list_media_filtered(
                            db.conn(), &project_id, &params,
                            Self::SIDEBAR_PAGE_SIZE + 1, 0,
                        ).ok()
                    }).unwrap_or_default();

                    // Pre-resolve thumbnail paths off the main thread
                    let thumbs: HashMap<String, Option<std::path::PathBuf>> = items.iter().map(|m| {
                        let thumb = data_dir.as_ref().and_then(|dir| {
                            if !m.mime_type.starts_with("image/") { return None; }
                            let rel = bds_core::util::paths::thumbnail_path(&m.id, "small", "webp");
                            let full = dir.join(&rel);
                            if full.exists() { Some(full) } else { None }
                        });
                        (m.id.clone(), thumb)
                    }).collect();

                    (items, thumbs)
                }).await.unwrap_or_default()
            },
            |(items, thumbs)| Message::SidebarMediaLoaded { items, thumbs },
        )
    }

    /// Load more posts (append to existing sidebar data).
    fn load_more_sidebar_posts(&mut self) -> Task<Message> {
        let (Some(_), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };

        use bds_core::db::queries::post::PostFilterParams;

        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let offset = self.sidebar_posts.len() as i64;
        let filter = match self.sidebar_view {
            SidebarView::Pages => &self.page_filter,
            _ => &self.post_filter,
        };
        let is_pages = self.sidebar_view == SidebarView::Pages;

        let params = PostFilterParams {
            search_query: filter.search_query.clone(),
            year: filter.calendar.selected_year,
            month: filter.calendar.selected_month,
            tags: filter.tag_filter.clone(),
            categories: filter.category_filter.clone(),
            exclude_pages: !is_pages,
            pages_only: is_pages,
        };

        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let db = Database::open(&db_path).ok();
                    db.and_then(|db| {
                        bds_core::db::queries::post::list_posts_filtered(
                            db.conn(), &project_id, &params,
                            Self::SIDEBAR_PAGE_SIZE + 1, offset,
                        ).ok()
                    }).unwrap_or_default()
                }).await.unwrap_or_default()
            },
            Message::SidebarPostsAppended,
        )
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
                    let items = db.and_then(|db| {
                        bds_core::db::queries::media::list_media_filtered(
                            db.conn(), &project_id, &params,
                            Self::SIDEBAR_PAGE_SIZE + 1, offset,
                        ).ok()
                    }).unwrap_or_default();

                    let thumbs: HashMap<String, Option<std::path::PathBuf>> = items.iter().map(|m| {
                        let thumb = data_dir.as_ref().and_then(|dir| {
                            if !m.mime_type.starts_with("image/") { return None; }
                            let rel = bds_core::util::paths::thumbnail_path(&m.id, "small", "webp");
                            let full = dir.join(&rel);
                            if full.exists() { Some(full) } else { None }
                        });
                        (m.id.clone(), thumb)
                    }).collect();

                    (items, thumbs)
                }).await.unwrap_or_default()
            },
            |(items, thumbs)| Message::SidebarMediaAppended { items, thumbs },
        )
    }

    /// Refresh available tags, categories, and calendar data for filter widgets.
    fn refresh_filter_metadata(&mut self) {
        if let (Some(db), Some(project)) = (&self.db, &self.active_project) {
            use bds_core::db::queries::post;
            use bds_core::db::queries::media;

            // Post filter metadata
            let all_tags = post::distinct_post_tags(db.conn(), &project.id)
                .unwrap_or_default();
            let all_cats = post::distinct_post_categories(db.conn(), &project.id)
                .unwrap_or_default();

            // Calendar counts for posts (excluding pages)
            let post_cal = post::post_calendar_counts(
                db.conn(), &project.id, false, true,
            ).unwrap_or_default();
            self.post_filter.available_tags = all_tags.clone();
            self.post_filter.available_categories = all_cats.clone();
            self.post_filter.calendar_years = Self::build_calendar_tree(&post_cal);

            // Calendar counts for pages only
            let page_cal = post::post_calendar_counts(
                db.conn(), &project.id, true, false,
            ).unwrap_or_default();
            self.page_filter.available_tags = all_tags;
            self.page_filter.available_categories = all_cats;
            self.page_filter.calendar_years = Self::build_calendar_tree(&page_cal);

            // Media filter metadata
            self.media_filter.available_tags = media::distinct_media_tags(
                db.conn(), &project.id,
            ).unwrap_or_default();
            let media_cal = media::media_calendar_counts(
                db.conn(), &project.id,
            ).unwrap_or_default();
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
        let active_tab_type = self.active_tab.as_ref().and_then(|id| {
            self.tabs.iter().find(|t| t.id == *id).map(|t| &t.tab_type)
        });
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
        let has_tab = self.active_tab.is_some();

        // File group: need active project for most, need open tab for Save
        self.menu_registry.set_enabled(MenuAction::NewPost, has_project);
        self.menu_registry.set_enabled(MenuAction::ImportMedia, has_project);
        self.menu_registry.set_enabled(MenuAction::Save, has_tab);
        self.menu_registry.set_enabled(MenuAction::OpenInBrowser, has_tab);
        self.menu_registry.set_enabled(MenuAction::OpenDataFolder, has_project);

        // Edit: Find/Replace need an open tab
        self.menu_registry.set_enabled(MenuAction::Find, has_tab);
        self.menu_registry.set_enabled(MenuAction::Replace, has_tab);

        // Blog group: need active project
        self.menu_registry.set_enabled(MenuAction::PublishSelected, has_project && has_tab);
        self.menu_registry.set_enabled(MenuAction::PreviewPost, has_project && has_tab);
        self.menu_registry.set_enabled(MenuAction::EditMenu, has_project);
        self.menu_registry.set_enabled(MenuAction::RebuildDatabase, has_project);
        self.menu_registry.set_enabled(MenuAction::ReindexText, has_project);
        self.menu_registry.set_enabled(MenuAction::MetadataDiff, has_project);
        self.menu_registry.set_enabled(MenuAction::RegenerateCalendar, has_project);
        self.menu_registry.set_enabled(MenuAction::ValidateTranslations, has_project);
        self.menu_registry.set_enabled(MenuAction::FillMissingTranslations, has_project && !self.offline_mode);
        self.menu_registry.set_enabled(MenuAction::GenerateSitemap, has_project);
        self.menu_registry.set_enabled(MenuAction::ValidateSite, has_project);
        self.menu_registry.set_enabled(MenuAction::UploadSite, has_project && !self.offline_mode);
    }

    // ── Editor save/publish helpers ──

    fn save_post_editor(&mut self, post_id: &str) -> Task<Message> {
        match self.persist_post_editor_state(post_id) {
            Ok(()) => self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved")),
            Err(e) => self.notify(ToastLevel::Error, &format!("Save failed: {e}")),
        }
        Task::none()
    }

    fn publish_post_editor(&mut self, post_id: &str) -> Task<Message> {
        if let Err(e) = self.persist_post_editor_state(post_id) {
            self.notify(ToastLevel::Error, &format!("Publish failed: {e}"));
            return Task::none();
        }
        let Some(ref db) = self.db else { return Task::none() };
        let Some(ref data_dir) = self.data_dir else { return Task::none() };
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
                self.notify(ToastLevel::Error, &format!("Publish failed: {e}"));
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

    fn query_post_link_results(&self, current_post_id: &str, search_query: &str) -> Vec<modal::InsertLinkResult> {
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
            .filter_map(|post_id| bds_core::db::queries::post::get_post_by_id(db.conn(), &post_id).ok())
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

    fn insert_markdown_into_post(&mut self, post_id: &str, markdown: &str) -> Task<Message> {
        let Some(state) = self.post_editors.get_mut(post_id) else { return Task::none() };
        state.insert_markdown_at_cursor(markdown);
        if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == post_id) {
            tab.is_dirty = true;
        }
        self.active_modal = None;
        self.save_post_editor(post_id)
    }

    fn insert_selected_post_link(&mut self, post_id: &str, linked_post_id: &str) -> Task<Message> {
        let Some(ref db) = self.db else { return Task::none() };
        let Ok(linked_post) = bds_core::db::queries::post::get_post_by_id(db.conn(), linked_post_id) else {
            self.notify(ToastLevel::Error, &t(self.ui_locale, "modal.postInsertLink.loadFailed"));
            return Task::none();
        };

        let markdown = bds_core::engine::post::post_insert_link(&linked_post.slug)
            .replacen("title", &linked_post.title, 1);
        self.insert_markdown_into_post(post_id, &markdown)
    }

    fn insert_created_post_link(&mut self, post_id: &str) -> Task<Message> {
        let Some(modal::ModalState::PostInsertLink { search_query, .. }) = self.active_modal.clone() else {
            return Task::none();
        };
        let title = search_query.trim();
        if title.is_empty() {
            self.notify(ToastLevel::Error, &t(self.ui_locale, "modal.postInsertLink.titleRequired"));
            return Task::none();
        }

        let Some(ref db) = self.db else { return Task::none() };
        let Some(ref data_dir) = self.data_dir else { return Task::none() };
        let Some(ref project) = self.active_project else { return Task::none() };

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
                let markdown = bds_core::engine::post::post_insert_link(&post.slug)
                    .replacen("title", &post.title, 1);
                self.insert_markdown_into_post(post_id, &markdown)
            }
            Err(_) => {
                self.notify(ToastLevel::Error, &t(self.ui_locale, "modal.postInsertLink.createFailed"));
                Task::none()
            }
        }
    }

    fn insert_selected_media(&mut self, post_id: &str, media_id: &str) -> Task<Message> {
        let Some(ref db) = self.db else { return Task::none() };
        let Ok(media) = bds_core::db::queries::media::get_media_by_id(db.conn(), media_id) else {
            self.notify(ToastLevel::Error, &t(self.ui_locale, "modal.insertMedia.loadFailed"));
            return Task::none();
        };

        let link_only = matches!(
            self.active_modal,
            Some(modal::ModalState::InsertMedia { link_only: true, .. })
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
        }) = self.active_modal.clone() else {
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
        let Some(modal::ModalState::InsertMedia { post_id, title, link_only, .. }) = self.active_modal.clone() else {
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
        }) = self.active_modal.clone() else {
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
        }) = self.active_modal.clone() else {
            return;
        };

        let image_count = media_list.iter().filter(|media| media.mime_type.starts_with("image/")).count();
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
            Err(e) => self.notify(ToastLevel::Error, &format!("Save failed: {e}")),
        }
        Task::none()
    }

    fn save_template_editor(&mut self, template_id: &str) -> Task<Message> {
        let Some(state) = self.template_editors.get(template_id) else { return Task::none() };
        let Some(ref db) = self.db else { return Task::none() };
        let Some(ref project) = self.active_project else { return Task::none() };

        match save_template_editor_state_impl(db, &project.id, state) {
            Ok(tmpl) => {
                let s = self.template_editors.get_mut(template_id).unwrap();
                s.is_dirty = false;
                s.updated_at = tmpl.updated_at;
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == tmpl.id) {
                    tab.is_dirty = false;
                    tab.title = tmpl.title.clone();
                }
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
            }
            Err(e) => {
                self.notify(ToastLevel::Error, &format!("Save failed: {e}"));
            }
        }
        Task::none()
    }

    fn save_script_editor(&mut self, script_id: &str) -> Task<Message> {
        let Some(state) = self.script_editors.get(script_id) else { return Task::none() };
        let Some(ref db) = self.db else { return Task::none() };
        let Some(ref project) = self.active_project else { return Task::none() };

        match save_script_editor_state_impl(db, &project.id, state) {
            Ok(script) => {
                let s = self.script_editors.get_mut(script_id).unwrap();
                s.is_dirty = false;
                s.updated_at = script.updated_at;
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.id == script.id) {
                    tab.is_dirty = false;
                    tab.title = script.title.clone();
                }
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
            }
            Err(e) => {
                self.notify(ToastLevel::Error, &format!("Save failed: {e}"));
            }
        }
        Task::none()
    }

    fn persist_post_editor_state(&mut self, post_id: &str) -> Result<(), String> {
        let state = self
            .post_editors
            .get(post_id)
            .cloned()
            .ok_or_else(|| "missing post editor".to_string())?;
        let db = self.db.as_ref().ok_or_else(|| "database unavailable".to_string())?;
        let data_dir = self
            .data_dir
            .as_ref()
            .ok_or_else(|| "project data directory unavailable".to_string())?;

        match persist_post_editor_state_impl(db, data_dir, &state)? {
            PersistedPostState::Translation(translation) => {
                if let Some(editor) = self.post_editors.get_mut(post_id) {
                    editor.is_dirty = false;
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
        Ok(())
    }

    fn persist_media_editor_state(&mut self, media_id: &str) -> Result<(), String> {
        let state = self
            .media_editors
            .get(media_id)
            .cloned()
            .ok_or_else(|| "missing media editor".to_string())?;
        let db = self.db.as_ref().ok_or_else(|| "database unavailable".to_string())?;
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
        let Some(db) = &self.db else { return Task::none() };
        let Some(data_dir) = &self.data_dir else { return Task::none() };
        match engine::post::delete_post(db.conn(), data_dir, post_id) {
            Ok(()) => {
                self.post_editors.remove(post_id);
                self.close_entity_tab(post_id);
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.deleted"));
            }
            Err(e) => self.notify(ToastLevel::Error, &format!("Delete failed: {e}")),
        }
        Task::none()
    }

    fn discard_post_editor(&mut self, post_id: &str) -> Task<Message> {
        let Some(db) = &self.db else { return Task::none() };
        let Some(data_dir) = &self.data_dir else { return Task::none() };
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
            Err(e) => self.notify(ToastLevel::Error, &format!("Discard failed: {e}")),
        }
        Task::none()
    }

    fn duplicate_post_editor(&mut self, post_id: &str) -> Task<Message> {
        let Some(db) = &self.db else { return Task::none() };
        let Some(data_dir) = &self.data_dir else { return Task::none() };
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
            Err(e) => self.notify(ToastLevel::Error, &format!("Duplicate failed: {e}")),
        }
        Task::none()
    }

    fn delete_media_editor(&mut self, media_id: &str) -> Task<Message> {
        let Some(db) = &self.db else { return Task::none() };
        let Some(data_dir) = &self.data_dir else { return Task::none() };
        match engine::media::delete_media(db.conn(), data_dir, media_id) {
            Ok(()) => {
                self.media_editors.remove(media_id);
                self.close_entity_tab(media_id);
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.deleted"));
            }
            Err(e) => self.notify(ToastLevel::Error, &format!("Delete failed: {e}")),
        }
        Task::none()
    }

    fn delete_template_editor(&mut self, template_id: &str) -> Task<Message> {
        let Some(db) = &self.db else { return Task::none() };
        let Some(data_dir) = &self.data_dir else { return Task::none() };
        match engine::template::delete_template(db.conn(), data_dir, template_id, false) {
            Ok(()) => {
                self.template_editors.remove(template_id);
                self.close_entity_tab(template_id);
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.deleted"));
            }
            Err(e) => self.notify(ToastLevel::Error, &format!("Delete failed: {e}")),
        }
        Task::none()
    }

    fn delete_script_editor(&mut self, script_id: &str) -> Task<Message> {
        let Some(db) = &self.db else { return Task::none() };
        let Some(data_dir) = &self.data_dir else { return Task::none() };
        match engine::script::delete_script(db.conn(), data_dir, script_id) {
            Ok(()) => {
                self.script_editors.remove(script_id);
                self.close_entity_tab(script_id);
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.deleted"));
            }
            Err(e) => self.notify(ToastLevel::Error, &format!("Delete failed: {e}")),
        }
        Task::none()
    }

    fn reload_tags_state(&mut self) {
        let Some(db) = &self.db else { return };
        let Some(project) = &self.active_project else { return };
        let tags = bds_core::db::queries::tag::list_tags_by_project(db.conn(), &project.id)
            .unwrap_or_default();
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
        } else {
            self.tags_view_state = Some(TagsViewState::new(tags, counts));
        }
    }

    fn delete_tag(&mut self, tag_id: &str) -> Task<Message> {
        let Some(db) = &self.db else { return Task::none() };
        let Some(data_dir) = &self.data_dir else { return Task::none() };
        let Some(project) = &self.active_project else { return Task::none() };
        match engine::tag::delete_tag(db.conn(), data_dir, &project.id, tag_id) {
            Ok(()) => {
                self.reload_tags_state();
                if let Some(state) = self.tags_view_state.as_mut() {
                    if state.editing_tag.as_ref().map(|tag| tag.id.as_str()) == Some(tag_id) {
                        state.editing_tag = None;
                    }
                }
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.deleted"));
            }
            Err(e) => self.notify(ToastLevel::Error, &format!("Delete failed: {e}")),
        }
        Task::none()
    }

    fn merge_tags(&mut self, source: &str, target: &str) -> Task<Message> {
        let Some(db) = &self.db else { return Task::none() };
        let Some(data_dir) = &self.data_dir else { return Task::none() };
        let Some(project) = &self.active_project else { return Task::none() };
        match engine::tag::merge_tags(db.conn(), data_dir, &project.id, &[source], target) {
            Ok(()) => {
                self.reload_tags_state();
                if let Some(state) = self.tags_view_state.as_mut() {
                    state.merge_source = None;
                    state.merge_target = None;
                }
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
            }
            Err(e) => self.notify(ToastLevel::Error, &format!("Merge failed: {e}")),
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
                state.default_author = meta.default_author.unwrap_or_default();
                state.max_posts_per_page = meta.max_posts_per_page.to_string();
            }
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
            if let Ok(setting) = bds_core::db::queries::setting::get_setting_by_key(db.conn(), "editor.default_mode") {
                state.default_mode = setting.value;
            }
            if let Ok(setting) = bds_core::db::queries::setting::get_setting_by_key(db.conn(), "editor.diff_view_style") {
                state.diff_view_style = setting.value;
            }
            if let Ok(setting) = bds_core::db::queries::setting::get_setting_by_key(db.conn(), "editor.wrap_long_lines") {
                state.wrap_long_lines = setting.value == "true";
            }
            if let Ok(setting) = bds_core::db::queries::setting::get_setting_by_key(db.conn(), "editor.hide_unchanged_regions") {
                state.hide_unchanged_regions = setting.value == "true";
            }
            if let Ok(setting) = bds_core::db::queries::setting::get_setting_by_key(db.conn(), "ai.system_prompt") {
                state.system_prompt = iced::widget::text_editor::Content::with_text(&setting.value);
            }
        }
        state.offline_mode = self.offline_mode;
        state
    }

    fn handle_tags_msg(&mut self, msg: TagsMsg) -> Task<Message> {
        // Ensure tags view state exists
        if self.tags_view_state.is_none() {
            let (tags, counts) = if let (Some(db), Some(project)) = (&self.db, &self.active_project) {
                let tags = bds_core::db::queries::tag::list_tags_by_project(db.conn(), &project.id)
                    .unwrap_or_default();
                let posts = bds_core::db::queries::post::list_posts_by_project(db.conn(), &project.id)
                    .unwrap_or_default();
                let mut counts = std::collections::HashMap::new();
                for post in &posts {
                    for tag_name in &post.tags {
                        *counts.entry(tag_name.to_lowercase()).or_insert(0usize) += 1;
                    }
                }
                (tags, counts)
            } else {
                (Vec::new(), std::collections::HashMap::new())
            };
            self.tags_view_state = Some(TagsViewState::new(tags, counts));
        }
        let state = self.tags_view_state.as_mut().unwrap();
        match msg {
            TagsMsg::SetSection(s) => { state.section = s; }
            TagsMsg::SearchChanged(q) => { state.search_query = q; }
            TagsMsg::SelectTag(id) => {
                if let Some(tag) = state.tags.iter().find(|t| t.id == id) {
                    state.editing_tag = Some(tags_view::EditingTag {
                        id: tag.id.clone(),
                        name: tag.name.clone(),
                        color: tag.color.clone().unwrap_or_default(),
                        template_slug: tag.post_template_slug.clone().unwrap_or_default(),
                    });
                }
            }
            TagsMsg::CreateTag(name) => {
                let mut created_editing = None;
                if let (Some(db), Some(data_dir), Some(project)) = (&self.db, &self.data_dir, &self.active_project) {
                    match engine::tag::create_tag(db.conn(), data_dir, &project.id, &name, None) {
                        Ok(tag) => {
                            created_editing = Some(tags_view::EditingTag {
                                id: tag.id,
                                name: tag.name,
                                color: tag.color.unwrap_or_default(),
                                template_slug: tag.post_template_slug.unwrap_or_default(),
                            });
                        }
                        Err(e) => self.notify(ToastLevel::Error, &format!("Save failed: {e}")),
                    }
                }
                if let Some(editing_tag) = created_editing {
                    self.reload_tags_state();
                    if let Some(state) = self.tags_view_state.as_mut() {
                        state.editing_tag = Some(editing_tag);
                    }
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                }
            }
            TagsMsg::EditTagName(s) => { if let Some(ref mut e) = state.editing_tag { e.name = s; } }
            TagsMsg::EditTagColor(s) => { if let Some(ref mut e) = state.editing_tag { e.color = s; } }
            TagsMsg::EditTagTemplate(s) => { if let Some(ref mut e) = state.editing_tag { e.template_slug = s; } }
            TagsMsg::SaveTag => {
                if let Some(editing) = state.editing_tag.clone() {
                    if let (Some(db), Some(data_dir)) = (&self.db, &self.data_dir) {
                        match engine::tag::update_tag(
                            db.conn(),
                            data_dir,
                            &editing.id,
                            Some(&editing.name),
                            Some(&editing.color),
                            Some(&editing.template_slug),
                        ) {
                            Ok(()) => {
                                self.reload_tags_state();
                                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                            }
                            Err(e) => self.notify(ToastLevel::Error, &format!("Save failed: {e}")),
                        }
                    }
                }
            }
            TagsMsg::DeleteTag(id) => {
                let name = state.tags.iter().find(|t| t.id == id).map(|t| t.name.clone()).unwrap_or_default();
                return Task::done(Message::ShowModal(modal::ModalState::ConfirmDelete {
                    entity_name: name,
                    references: Vec::new(),
                    on_confirm: modal::ConfirmAction::DeleteTag(id),
                }));
            }
            TagsMsg::SetMergeSource(s) => { state.merge_source = Some(s); }
            TagsMsg::SetMergeTarget(s) => { state.merge_target = Some(s); }
            TagsMsg::MergeTags => {
                if let (Some(source), Some(target)) = (&state.merge_source, &state.merge_target) {
                    return Task::done(Message::ShowModal(modal::ModalState::Confirm {
                        title: t(self.ui_locale, "tags.mergeTags"),
                        message: t(self.ui_locale, "tags.mergeConfirm"),
                        on_confirm: modal::ConfirmAction::MergeTags {
                            source: source.clone(),
                            target: target.clone(),
                        },
                    }));
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
            SettingsMsg::SearchChanged(q) => { state.search_query = q; }
            SettingsMsg::ToggleSection(section) => {
                if let Some(pos) = state.collapsed.iter().position(|s| *s == section) {
                    state.collapsed.remove(pos);
                } else {
                    state.collapsed.push(section);
                }
            }
            SettingsMsg::ProjectNameChanged(s) => { state.project_name = s; }
            SettingsMsg::ProjectDescriptionAction(action) => {
                state.project_description.perform(action);
            }
            SettingsMsg::DataPathChanged(s) => { state.data_path = s; }
            SettingsMsg::BrowseDataPath => {
                return crate::platform::dialog::pick_folder(t(self.ui_locale, "dialog.selectFolder"));
            }
            SettingsMsg::ResetDataPath => {
                if let Some(ref project) = self.active_project {
                    state.data_path = project.data_path.clone().unwrap_or_default();
                }
            }
            SettingsMsg::PublicUrlChanged(s) => { state.public_url = s; }
            SettingsMsg::DefaultAuthorChanged(s) => { state.default_author = s; }
            SettingsMsg::MaxPostsPerPageChanged(s) => { state.max_posts_per_page = s; }
            SettingsMsg::SaveProject => {
                if let (Some(db), Some(data_dir), Some(project)) = (&self.db, &self.data_dir, self.active_project.as_mut()) {
                    let max_posts = match state.max_posts_per_page.trim().parse::<i32>() {
                        Ok(value) => value,
                        Err(_) => {
                            self.notify(ToastLevel::Error, "Invalid max posts per page");
                            return Task::none();
                        }
                    };
                    let mut meta = engine::meta::read_project_json(data_dir).unwrap_or(bds_core::model::metadata::ProjectMetadata {
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
                    });
                    meta.name = state.project_name.clone();
                    meta.description = {
                        let value = state.project_description.text();
                        if value.trim().is_empty() { None } else { Some(value) }
                    };
                    meta.public_url = if state.public_url.trim().is_empty() { None } else { Some(state.public_url.clone()) };
                    meta.default_author = if state.default_author.trim().is_empty() { None } else { Some(state.default_author.clone()) };
                    meta.max_posts_per_page = max_posts;
                    if let Err(e) = meta.validate() {
                        self.notify(ToastLevel::Error, &format!("Save failed: {e}"));
                        return Task::none();
                    }
                    project.name = state.project_name.clone();
                    project.description = meta.description.clone();
                    project.data_path = if state.data_path.trim().is_empty() { None } else { Some(state.data_path.clone()) };
                    project.updated_at = bds_core::util::now_unix_ms();
                    let db_result = bds_core::db::queries::project::update_project(db.conn(), project);
                    let file_result = engine::meta::write_project_json(data_dir, &meta);
                    match (db_result, file_result) {
                        (Ok(()), Ok(())) => {
                            if let Some(listing) = self.projects.iter_mut().find(|p| p.id == project.id) {
                                *listing = project.clone();
                            }
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                        }
                        (Err(e), _) => self.notify(ToastLevel::Error, &format!("Save failed: {e}")),
                        (_, Err(e)) => self.notify(ToastLevel::Error, &format!("Save failed: {e}")),
                    }
                }
            }
            SettingsMsg::DefaultModeChanged(s) => { state.default_mode = s; }
            SettingsMsg::DiffViewStyleChanged(s) => { state.diff_view_style = s; }
            SettingsMsg::WrapLongLinesChanged(b) => { state.wrap_long_lines = b; }
            SettingsMsg::HideUnchangedRegionsChanged(b) => { state.hide_unchanged_regions = b; }
            SettingsMsg::SaveEditor => {
                if let Some(db) = &self.db {
                    match save_editor_settings_state_impl(db, state) {
                        Ok(_) => self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved")),
                        Err(e) => self.notify(ToastLevel::Error, &format!("Save failed: {e}")),
                    }
                }
            }
            SettingsMsg::SshModeChanged(s) => { state.ssh_mode = s; }
            SettingsMsg::SshHostChanged(s) => { state.ssh_host = s; }
            SettingsMsg::SshUsernameChanged(s) => { state.ssh_username = s; }
            SettingsMsg::SshRemotePathChanged(s) => { state.ssh_remote_path = s; }
            SettingsMsg::SavePublishing => {
                if let Some(data_dir) = &self.data_dir {
                    let prefs = PublishingPreferences {
                        ssh_host: if state.ssh_host.trim().is_empty() { None } else { Some(state.ssh_host.clone()) },
                        ssh_user: if state.ssh_username.trim().is_empty() { None } else { Some(state.ssh_username.clone()) },
                        ssh_remote_path: if state.ssh_remote_path.trim().is_empty() { None } else { Some(state.ssh_remote_path.clone()) },
                        ssh_mode: if state.ssh_mode.eq_ignore_ascii_case("scp") { SshMode::Scp } else { SshMode::Rsync },
                    };
                    match engine::meta::write_publishing_json(data_dir, &prefs) {
                        Ok(()) => self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved")),
                        Err(e) => self.notify(ToastLevel::Error, &format!("Save failed: {e}")),
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
            SettingsMsg::SystemPromptAction(action) => {
                state.system_prompt.perform(action);
            }
            SettingsMsg::SaveSystemPrompt => {
                if let Some(db) = &self.db {
                    match bds_core::db::queries::setting::set_setting_value(
                        db.conn(),
                        "ai.system_prompt",
                        &state.system_prompt.text(),
                        bds_core::util::now_unix_ms(),
                    ) {
                        Ok(()) => self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved")),
                        Err(e) => self.notify(ToastLevel::Error, &format!("Save failed: {e}")),
                    }
                }
            }
            SettingsMsg::ResetSystemPrompt => {
                state.system_prompt = iced::widget::text_editor::Content::new();
            }
            SettingsMsg::RebuildPosts => { return Task::done(Message::RebuildDatabase); }
            SettingsMsg::RebuildMedia => { return Task::done(Message::RebuildDatabase); }
            SettingsMsg::RebuildScripts => { return Task::done(Message::RebuildDatabase); }
            SettingsMsg::RebuildTemplates => { return Task::done(Message::RebuildDatabase); }
            SettingsMsg::RebuildLinks => {
                if let (Some(db), Some(data_dir), Some(project)) =
                    (&self.db, &self.data_dir, &self.active_project)
                {
                    match bds_core::engine::post::rebuild_all_links(
                        db.conn(), data_dir, &project.id,
                    ) {
                        Ok(count) => {
                            self.notify(
                                ToastLevel::Info,
                                &format!("Rebuilt {} links", count),
                            );
                        }
                        Err(e) => {
                            self.notify(
                                ToastLevel::Error,
                                &format!("Failed to rebuild links: {e}"),
                            );
                        }
                    }
                }
            }
            SettingsMsg::RegenerateThumbnails => {
                self.notify(ToastLevel::Info, &t(self.ui_locale, "settings.regeneratingThumbnails"));
            }
            SettingsMsg::OpenDataFolder => {
                if let Some(ref dir) = self.data_dir {
                    let _ = open::that(dir);
                }
            }
            SettingsMsg::FocusSection(section) => {
                // Expand the target section, collapse all others
                use crate::views::settings_view::SettingsSection;
                let all_others: Vec<SettingsSection> = SettingsSection::all()
                    .iter()
                    .filter(|s| **s != section)
                    .cloned()
                    .collect();
                state.collapsed = all_others;
                // Clear search filter to ensure section is visible
                state.search_query.clear();
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

    fn load_post_media_items(&self, post_id: &str) -> Vec<LinkedMediaItem> {
        let Some(ref db) = self.db else {
            return Vec::new();
        };

        bds_core::db::queries::post_media::list_post_media_by_post(db.conn(), post_id)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|link| {
                bds_core::db::queries::media::get_media_by_id(db.conn(), &link.media_id)
                    .ok()
                    .map(|media| LinkedMediaItem {
                        media_id: media.id,
                        name: media.title.unwrap_or(media.original_name),
                        is_image: media.mime_type.starts_with("image/"),
                        sort_order: link.sort_order,
                    })
            })
            .collect()
    }

    fn refresh_post_relationships(&mut self, post_id: &str) {
        let (outlinks, backlinks) = self.load_post_links(post_id);
        let linked_media = self.load_post_media_items(post_id);
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
                            if post.content.is_none() {
                                if let Some(ref data_dir) = self.data_dir {
                                    let rel = bds_core::util::paths::post_file_path(
                                        post.created_at,
                                        &post.slug,
                                    );
                                    let path = data_dir.join(&rel);
                                    if let Ok(raw) = std::fs::read_to_string(&path) {
                                        if let Ok((_fm, body)) =
                                            bds_core::util::frontmatter::read_post_file(&raw)
                                        {
                                            post.content = Some(body);
                                        }
                                    }
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
                                        if let Ok(raw) = std::fs::read_to_string(&path) {
                                            if let Ok((_fm, body)) =
                                                bds_core::util::frontmatter::read_translation_file(&raw)
                                            {
                                                tr.content = Some(body);
                                            }
                                        }
                                    }
                                }
                            }
                            let (outlinks, backlinks) = self.load_post_links(&post.id);
                            let linked_media = self.load_post_media_items(&post.id);
                            self.post_editors.insert(
                                post.id.clone(),
                                PostEditorState::from_post(
                                    &post,
                                    &self.blog_languages,
                                    &translations,
                                    outlinks,
                                    backlinks,
                                    linked_media,
                                ),
                            );
                        }
                        Err(e) => {
                            self.notify(ToastLevel::Error, &format!("Failed to load post: {e}"));
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
                            self.media_editors.insert(media.id.clone(), MediaEditorState::from_media(&media, &self.blog_languages, &translations));
                        }
                        Err(e) => {
                            self.notify(ToastLevel::Error, &format!("Failed to load media: {e}"));
                        }
                    }
                }
            }
            TabType::Templates => {
                if !self.template_editors.contains_key(&tab.id) {
                    match bds_core::db::queries::template::get_template_by_id(db.conn(), &tab.id) {
                        Ok(mut template) => {
                            // Published templates: read content from file, strip frontmatter
                            if template.content.is_none() {
                                if let Some(ref data_dir) = self.data_dir {
                                    let rel = bds_core::util::paths::template_file_path(&template.slug);
                                    let path = data_dir.join(&rel);
                                    if let Ok(raw) = std::fs::read_to_string(&path) {
                                        if let Ok((_fm, body)) =
                                            bds_core::util::frontmatter::read_template_file(&raw)
                                        {
                                            template.content = Some(body);
                                        }
                                    }
                                }
                            }
                            self.template_editors.insert(template.id.clone(), TemplateEditorState::from_template(&template));
                        }
                        Err(e) => {
                            self.notify(ToastLevel::Error, &format!("Failed to load template: {e}"));
                        }
                    }
                }
            }
            TabType::Scripts => {
                if !self.script_editors.contains_key(&tab.id) {
                    match bds_core::db::queries::script::get_script_by_id(db.conn(), &tab.id) {
                        Ok(mut script) => {
                            // Published scripts: read content from file using actual file_path
                            if script.content.is_none() {
                                if let Some(ref data_dir) = self.data_dir {
                                    let path = data_dir.join(&script.file_path);
                                    if let Ok(raw) = std::fs::read_to_string(&path) {
                                        if let Ok((_fm, body)) =
                                            bds_core::util::frontmatter::read_script_file(&raw)
                                        {
                                            script.content = Some(body);
                                        }
                                    }
                                }
                            }
                            self.script_editors.insert(script.id.clone(), ScriptEditorState::from_script(&script));
                        }
                        Err(e) => {
                            self.notify(ToastLevel::Error, &format!("Failed to load script: {e}"));
                        }
                    }
                }
            }
            TabType::Tags => {
                if self.tags_view_state.is_none() {
                    let project_id = self.active_project.as_ref().map(|p| p.id.as_str()).unwrap_or("");
                    // Import tags from file first, then sync from posts (additive only)
                    if let Some(ref data_dir) = self.data_dir {
                        let _ = bds_core::engine::tag::import_tags_from_file(
                            db.conn(), data_dir, project_id,
                        );
                        let _ = bds_core::engine::tag::sync_tags_from_posts(
                            db.conn(), project_id,
                        );
                    }
                    let tags = bds_core::db::queries::tag::list_tags_by_project(
                        db.conn(),
                        project_id,
                    ).unwrap_or_default();
                    // Compute post counts per tag for cloud sizing
                    let posts = bds_core::db::queries::post::list_posts_by_project(
                        db.conn(), project_id,
                    ).unwrap_or_default();
                    let mut tag_post_counts = std::collections::HashMap::new();
                    for post in &posts {
                        for tag_name in &post.tags {
                            *tag_post_counts.entry(tag_name.to_lowercase()).or_insert(0usize) += 1;
                        }
                    }
                    self.tags_view_state = Some(TagsViewState::new(tags, tag_post_counts));
                }
            }
            TabType::Settings => {
                if self.settings_state.is_none() {
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
            }
            _ => {}
        }
    }
}
