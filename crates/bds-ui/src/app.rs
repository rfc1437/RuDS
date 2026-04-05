use std::path::PathBuf;
use std::sync::Arc;

use iced::{Element, Subscription, Task};

use bds_core::db::Database;
use bds_core::engine::task::{TaskId, TaskManager, TaskStatus};
use bds_core::engine;
use bds_core::i18n::{detect_os_locale, UiLocale};
use bds_core::model::{Media, Post, Project};

use crate::i18n::{t, tw};
use crate::platform::menu::{self, MenuAction, MenuRegistry};
use crate::state::navigation::{
    handle_activity_click, OutputEntry, PanelTab, SidebarView, TaskSnapshot,
};
use crate::state::tabs::{self, Tab, TabType};
use crate::state::toast::{Toast, ToastLevel};
use crate::views::workspace;

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

    // Tabs
    OpenTab(Tab),
    CloseTab(String),
    SelectTab(String),
    PinTab(String),

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

    // Blog actions (dispatched to engine)
    RebuildDatabase,
    ReindexText,
    RegenerateCalendar,
    ValidateTranslations,
    GenerateSite,
    RunMetadataDiff,
    EngineTaskDone { task_id: TaskId, label: String, result: Result<String, String> },

    Noop,
    InitMenuBar,
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

    // Navigation
    sidebar_view: SidebarView,
    sidebar_visible: bool,

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

    // Flags
    offline_mode: bool,
    locale_dropdown_open: bool,
    project_dropdown_open: bool,

    // Toasts
    toasts: Vec<Toast>,

    // macOS lifecycle receiver and retained delegate
    #[cfg(target_os = "macos")]
    _lifecycle_rx: std::sync::mpsc::Receiver<crate::platform::macos::LifecycleEvent>,
    #[cfg(target_os = "macos")]
    _lifecycle_delegate: Option<objc2::rc::Retained<crate::platform::macos::BdsAppDelegate>>,
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

        #[cfg(target_os = "macos")]
        let (_lifecycle_tx, _lifecycle_rx) = crate::platform::macos::lifecycle_channel();
        #[cfg(target_os = "macos")]
        let _lifecycle_delegate = crate::platform::macos::install_delegate(_lifecycle_tx);

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
                sidebar_view: SidebarView::Posts,
                sidebar_visible: true,
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
                offline_mode: false,
                locale_dropdown_open: false,
                project_dropdown_open: false,
                toasts: Vec::new(),
                #[cfg(target_os = "macos")]
                _lifecycle_rx,
                #[cfg(target_os = "macos")]
                _lifecycle_delegate,
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
                self.sidebar_view = new_view;
                self.sidebar_visible = new_visible;
                Task::none()
            }
            Message::ToggleSidebar => {
                self.sidebar_visible = !self.sidebar_visible;
                Task::none()
            }
            Message::TogglePanel => {
                self.panel_visible = !self.panel_visible;
                Task::none()
            }

            // ── Tabs ──
            Message::OpenTab(tab) => {
                let idx = tabs::open_tab(&mut self.tabs, tab);
                if let Some(t) = self.tabs.get(idx) {
                    self.active_tab = Some(t.id.clone());
                }
                self.sync_menu_state();
                Task::none()
            }
            Message::CloseTab(id) => {
                if let Some(next_idx) = tabs::close_tab(&mut self.tabs, &id) {
                    self.active_tab = self.tabs.get(next_idx).map(|t| t.id.clone());
                } else {
                    self.active_tab = None;
                }
                self.sync_menu_state();
                Task::none()
            }
            Message::SelectTab(id) => {
                if self.tabs.iter().any(|t| t.id == id) {
                    self.active_tab = Some(id);
                }
                Task::none()
            }
            Message::PinTab(id) => {
                tabs::pin_tab(&mut self.tabs, &id);
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
                self.refresh_counts();
                self.sync_menu_state();
                Task::none()
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
                self.open_singleton_tab(TabType::TranslationValidation, "Translation Validation");
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
                self.open_singleton_tab(TabType::MetadataDiff, "Metadata Diff");
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
                self.refresh_counts();
                self.refresh_task_snapshots();
                Task::none()
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

        workspace::view(
            self.sidebar_view,
            self.sidebar_visible,
            &self.tabs,
            self.active_tab.as_deref(),
            self.panel_visible,
            self.panel_tab,
            &self.task_snapshots,
            &self.output_entries,
            &self.sidebar_posts,
            &self.sidebar_media,
            active_name,
            &self.projects,
            self.active_project.as_ref().map(|p| p.id.as_str()),
            self.post_count,
            self.media_count,
            self.offline_mode,
            self.locale_dropdown_open,
            self.project_dropdown_open,
            self.ui_locale,
            &self.toasts,
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

        Subscription::batch([menu_sub, task_tick, toast_tick])
    }

    // ── Private helpers ──

    fn dispatch_menu_action(&mut self, action: MenuAction) -> Task<Message> {
        match action {
            // File
            MenuAction::NewPost => {
                if let (Some(db), Some(project), Some(data_dir)) =
                    (&self.db, &self.active_project, &self.data_dir)
                {
                    let title = t(self.ui_locale, "post.untitled");
                    match engine::post::create_post(
                        db.conn(),
                        data_dir,
                        &project.id,
                        &title,
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
                                title: post.title.clone(),
                                is_transient: true,
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
                self.open_singleton_tab(TabType::Settings, "Settings");
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
                self.open_singleton_tab(TabType::MenuEditor, "Menu Editor");
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
                self.open_singleton_tab(TabType::SiteValidation, "Site Validation");
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
                self.open_singleton_tab(TabType::Documentation, "Documentation");
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

    fn open_singleton_tab(&mut self, tab_type: TabType, title: &str) {
        let tab = Tab {
            id: format!("singleton-{title}"),
            tab_type,
            title: title.to_string(),
            is_transient: false,
        };
        let idx = tabs::open_tab(&mut self.tabs, tab);
        if let Some(t) = self.tabs.get(idx) {
            self.active_tab = Some(t.id.clone());
        }
    }

    fn refresh_task_snapshots(&mut self) {
        self.task_snapshots = self
            .task_manager
            .snapshots()
            .into_iter()
            .map(|(id, label, status, progress, message)| {
                let status_str = match &status {
                    TaskStatus::Queued => "queued".to_string(),
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

    fn refresh_counts(&mut self) {
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
            self.sidebar_posts = bds_core::db::queries::post::list_posts_by_project_limited(
                db.conn(),
                &project.id,
                500,
                0,
            )
            .unwrap_or_default();
            self.sidebar_media = bds_core::db::queries::media::list_media_by_project_limited(
                db.conn(),
                &project.id,
                500,
                0,
            )
            .unwrap_or_default();
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
}
