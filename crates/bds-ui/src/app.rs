use std::path::PathBuf;
use std::sync::Arc;

use iced::{Element, Subscription, Task};

use bds_core::db::Database;
use bds_core::engine::task::{TaskManager, TaskStatus};
use bds_core::engine;
use bds_core::i18n::{detect_os_locale, UiLocale};
use bds_core::model::Project;

use crate::i18n::{t, tw};
use crate::platform::menu::{self, MenuAction, MenuRegistry};
use crate::state::navigation::{
    handle_activity_click, OutputEntry, PanelTab, SidebarView, TaskSnapshot,
};
use crate::state::tabs::{self, Tab, TabType};
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

    // Blog actions (dispatched to engine)
    RebuildDatabase,
    ReindexText,
    RunMetadataDiff,
    BlogTaskFinished { label: String, result: Result<(), String> },

    Noop,
}

// ───────────────────────────────────────────────────────────
// App State
// ───────────────────────────────────────────────────────────

pub struct BdsApp {
    // Database
    db: Option<Database>,

    // Project
    active_project: Option<Project>,
    projects: Vec<Project>,
    data_dir: Option<PathBuf>,

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

    // macOS lifecycle receiver
    #[cfg(target_os = "macos")]
    _lifecycle_rx: std::sync::mpsc::Receiver<crate::platform::macos::LifecycleEvent>,
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

        let db = Database::open(&db_path).ok();
        if let Some(ref db) = db {
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

        // If no projects exist, create a default one
        let init_task = if projects.is_empty() {
            if let Some(ref db) = db {
                let default_data = dirs::data_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("bds")
                    .join("projects")
                    .join("my-blog");
                match engine::project::create_project(
                    db.conn(),
                    "My Blog",
                    Some(default_data.to_str().unwrap_or("my-blog")),
                ) {
                    Ok(project) => {
                        let _ = engine::project::set_active_project(db.conn(), &project.id);
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

        // Disable items that need selection
        registry.set_enabled(MenuAction::Save, false);
        registry.set_enabled(MenuAction::PublishSelected, false);
        registry.set_enabled(MenuAction::PreviewPost, false);

        #[cfg(target_os = "macos")]
        let (_lifecycle_tx, _lifecycle_rx) = crate::platform::macos::lifecycle_channel();

        (
            Self {
                db,
                active_project: active_project.clone(),
                projects,
                data_dir,
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
                #[cfg(target_os = "macos")]
                _lifecycle_rx,
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
                Task::none()
            }
            Message::CloseTab(id) => {
                if let Some(next_idx) = tabs::close_tab(&mut self.tabs, &id) {
                    self.active_tab = self.tabs.get(next_idx).map(|t| t.id.clone());
                } else {
                    self.active_tab = None;
                }
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
                // Re-resolve active
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
                Task::none()
            }
            Message::SwitchProject(project_id) => {
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
                            self.add_output(&tw(self.ui_locale, "projectSelector.toast.switched", &[("name", &name)]));
                        }
                        Err(_) => {
                            self.add_output(&t(self.ui_locale, "projectSelector.toast.switchFailed"));
                        }
                    }
                }
                Task::none()
            }
            Message::ProjectSwitched(result) => {
                match result {
                    Ok(name) => self.add_output(&tw(self.ui_locale, "projectSelector.toast.switched", &[("name", &name)])),
                    Err(msg) => self.add_output(&msg),
                }
                Task::none()
            }
            Message::RequestCreateProject => {
                crate::platform::dialog::pick_folder()
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
                            self.add_output(&msg);
                        }
                        Err(_) => {
                            self.add_output(&t(self.ui_locale, "projectSelector.toast.createFailed"));
                        }
                    }
                }
                Task::none()
            }
            Message::ProjectCreated(result) => {
                match result {
                    Ok(name) => self.add_output(&tw(self.ui_locale, "projectSelector.toast.created", &[("name", &name)])),
                    Err(msg) => self.add_output(&msg),
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
                            self.add_output(&t(self.ui_locale, "projectSelector.toast.deleteFailed"));
                        }
                    }
                }
                Task::none()
            }
            Message::ProjectDeleted(result) => {
                if let Err(msg) = result {
                    self.add_output(&msg);
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
                Task::none()
            }

            // ── Blog engine actions ──
            Message::RebuildDatabase => {
                self.add_output("Rebuilding database...");
                // Actual rebuild dispatch deferred to later milestones
                Task::none()
            }
            Message::ReindexText => {
                self.add_output("Reindexing search text...");
                Task::none()
            }
            Message::RunMetadataDiff => {
                self.open_singleton_tab(TabType::MetadataDiff, "Metadata Diff");
                Task::none()
            }
            Message::BlogTaskFinished { label, result } => {
                match result {
                    Ok(()) => {
                        let msg = tw(self.ui_locale, "app.taskCompleted", &[("message", &label)]);
                        self.add_output(&msg);
                    }
                    Err(err) => {
                        let msg = tw(self.ui_locale, "app.taskFailed", &[("message", &err)]);
                        self.add_output(&msg);
                    }
                }
                Task::none()
            }

            Message::Noop => Task::none(),
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
            active_name,
            0, // post_count — populated in later milestones
            0, // media_count — populated in later milestones
            self.offline_mode,
            self.locale_dropdown_open,
            self.ui_locale,
        )
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let menu_sub = menu::menu_subscription();

        let task_tick = iced::time::every(std::time::Duration::from_millis(500))
            .map(|_| Message::TaskTick);

        Subscription::batch([menu_sub, task_tick])
    }

    // ── Private helpers ──

    fn dispatch_menu_action(&mut self, action: MenuAction) -> Task<Message> {
        match action {
            // File
            MenuAction::NewPost => {
                // Will create post + open tab in later milestones
                Task::none()
            }
            MenuAction::ImportMedia => crate::platform::dialog::pick_media_files(),
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
            MenuAction::RegenerateCalendar => Task::none(),
            MenuAction::ValidateTranslations => {
                self.open_singleton_tab(TabType::TranslationValidation, "Translation Validation");
                Task::none()
            }
            MenuAction::FillMissingTranslations => Task::none(),
            MenuAction::GenerateSitemap => Task::none(),
            MenuAction::ValidateSite => {
                self.open_singleton_tab(TabType::SiteValidation, "Site Validation");
                Task::none()
            }
            MenuAction::UploadSite => Task::none(),
            // Help
            MenuAction::About => Task::none(),
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
}
