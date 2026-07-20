use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use bds_core::db::Database;
use bds_core::engine::ai::{AiEndpointConfig, AiEndpointKind};
use bds_core::engine::{self, domain_events};
use bds_core::i18n::{UiLocale, normalize_language};
use bds_core::model::metadata::ProjectMetadata;
use bds_core::model::{
    DomainEntity, Media, NotificationAction, Post, PostStatus, Project, PublishingPreferences,
    ScriptKind, ScriptStatus, SshMode, Tag, TemplateKind, TemplateStatus,
};
use bds_editor::EditorBuffer;
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Size;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};
use ratatui_image::picker::Picker;
use ratatui_image::protocol::Protocol;
use ratatui_image::{Image as TerminalImage, Resize};

use crate::host::ApplicationHost;

const SIDEBAR_WIDTH: u16 = 30;
const MAX_DIFF_BYTES: usize = 512 * 1024;
const COLORS: &[&str] = &["", "#7aa2f7", "#9ece6a", "#e0af68", "#bb9af7", "#f7768e"];

#[derive(Clone, Copy)]
struct CommandSpec {
    id: &'static str,
    key: &'static str,
}

const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        id: "metadata-diff",
        key: "tui.commandMetadataDiff",
    },
    CommandSpec {
        id: "validate-site",
        key: "tui.commandValidateSite",
    },
    CommandSpec {
        id: "force-render",
        key: "tui.commandForceRender",
    },
    CommandSpec {
        id: "rebuild-database",
        key: "tui.commandRebuildDatabase",
    },
    CommandSpec {
        id: "reindex-search",
        key: "tui.commandReindexSearch",
    },
    CommandSpec {
        id: "validate-translations-gui",
        key: "tui.commandValidateTranslations",
    },
    CommandSpec {
        id: "find-duplicates-gui",
        key: "tui.commandFindDuplicates",
    },
    CommandSpec {
        id: "upload-site",
        key: "tui.commandUploadSite",
    },
    CommandSpec {
        id: "browser-preview-url",
        key: "tui.commandBrowserPreviewUrl",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TuiView {
    Posts,
    Media,
    Templates,
    Scripts,
    Tags,
    Settings,
    Git,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Editor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorMode {
    Source,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiKey {
    Char(char),
    Enter,
    Esc,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Tab,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TuiModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuiInput {
    pub key: TuiKey,
    pub modifiers: TuiModifiers,
}

impl TuiInput {
    pub const fn plain(key: TuiKey) -> Self {
        Self {
            key,
            modifiers: TuiModifiers {
                ctrl: false,
                shift: false,
                alt: false,
            },
        }
    }

    pub const fn ctrl(key: TuiKey) -> Self {
        Self {
            key,
            modifiers: TuiModifiers {
                ctrl: true,
                shift: false,
                alt: false,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SidebarItem {
    Header(String),
    Post(String, String),
    Media(String, String),
    Template(String, String),
    Script(String, String),
    TagSection(TagSection),
    SettingSection(SettingSection),
    GitFile(String, String),
    GitCommit(String, String),
    Empty(String),
}

impl SidebarItem {
    fn selectable(&self) -> bool {
        !matches!(self, Self::Header(_) | Self::Empty(_))
    }

    fn label(&self, locale: UiLocale) -> String {
        match self {
            Self::Header(value) | Self::Empty(value) => value.clone(),
            Self::Post(_, value)
            | Self::Media(_, value)
            | Self::Template(_, value)
            | Self::Script(_, value)
            | Self::GitFile(_, value)
            | Self::GitCommit(_, value) => value.clone(),
            Self::TagSection(value) => bds_core::i18n::translate(locale, value.key()),
            Self::SettingSection(value) => bds_core::i18n::translate(locale, value.key()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TagSection {
    Cloud,
    Manage,
    Merge,
}

impl TagSection {
    const ALL: [Self; 3] = [Self::Cloud, Self::Manage, Self::Merge];
    const fn key(self) -> &'static str {
        match self {
            Self::Cloud => "tags.nav.cloud",
            Self::Manage => "tags.nav.manage",
            Self::Merge => "tags.nav.merge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SettingSection {
    Project,
    Editor,
    Content,
    Ai,
    Technology,
    Publishing,
    Data,
    Mcp,
}

impl SettingSection {
    const ALL: [Self; 8] = [
        Self::Project,
        Self::Editor,
        Self::Content,
        Self::Ai,
        Self::Technology,
        Self::Publishing,
        Self::Data,
        Self::Mcp,
    ];
    const fn key(self) -> &'static str {
        match self {
            Self::Project => "settings.nav.project",
            Self::Editor => "settings.nav.editor",
            Self::Content => "settings.nav.content",
            Self::Ai => "settings.nav.ai",
            Self::Technology => "settings.nav.technology",
            Self::Publishing => "settings.nav.publishing",
            Self::Data => "settings.nav.data",
            Self::Mcp => "settings.nav.mcp",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum FieldKind {
    Text,
    Bool,
    Enum(&'static [&'static str]),
    ReadOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SettingField {
    label: String,
    key: &'static str,
    kind: FieldKind,
    value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EditorEntity {
    Post(String),
    Template(String),
    Script(String),
}

struct Editor {
    entity: EditorEntity,
    title: String,
    syntax: &'static str,
    post_language: Option<String>,
    buffer: EditorBuffer,
    mode: EditorMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PromptKind {
    Search,
    Command,
    ProjectPath,
    Commit,
    Setting(usize),
    TagCreate,
    TagRename(String),
    ConfirmDeleteTag(String),
    EditorTitle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Prompt {
    kind: PromptKind,
    value: String,
    candidates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Panel {
    Welcome,
    MediaPreview {
        title: String,
        path: PathBuf,
    },
    Settings(SettingSection),
    Tags(TagSection),
    Git,
    Report {
        title: String,
        body: String,
        action: ReportAction,
    },
    Help,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReportAction {
    MetadataDiff,
    SiteValidation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Overlay {
    Projects { selected: usize },
    ConfirmDiscard,
}

struct BackgroundResult {
    status: String,
    panel: Option<Panel>,
    reload: bool,
}

pub struct TuiApp {
    host: ApplicationHost,
    remote: bool,
    locale: UiLocale,
    project: Option<Project>,
    data_dir: Option<PathBuf>,
    pub view: TuiView,
    pub focus: Focus,
    pub selected_index: usize,
    items: Vec<SidebarItem>,
    editor: Option<Editor>,
    panel: Panel,
    overlay: Option<Overlay>,
    prompt: Option<Prompt>,
    filters: HashMap<TuiView, String>,
    status: String,
    output: Vec<String>,
    settings_fields: Vec<SettingField>,
    panel_index: usize,
    marked_tags: HashSet<String>,
    scroll: u16,
    quit: bool,
    events: domain_events::EventSubscription,
    started_task_ids: HashSet<u64>,
    completed_task_ids: HashSet<u64>,
    airplane: bool,
    background_tx: mpsc::Sender<BackgroundResult>,
    background_rx: mpsc::Receiver<BackgroundResult>,
    image_source: Option<image::DynamicImage>,
    image_protocol: Option<Protocol>,
    terminal_size: (u16, u16),
}

impl TuiApp {
    pub fn new(host: ApplicationHost, remote: bool) -> Result<Self> {
        let db = host.database()?;
        let locale = engine::settings::ui_language(db.conn())?
            .map(|value| normalize_language(&value))
            .unwrap_or_else(bds_core::i18n::detect_os_locale);
        let project = engine::project::get_active_project(db.conn())?;
        let data_dir = project
            .as_ref()
            .map(|project| host.project_data_dir(project));
        let started_task_ids = host
            .tasks()
            .snapshots()
            .into_iter()
            .map(|task| task.id)
            .collect();
        let (background_tx, background_rx) = mpsc::channel();
        let mut app = Self {
            host,
            remote,
            locale,
            project,
            data_dir,
            view: TuiView::Posts,
            focus: Focus::Sidebar,
            selected_index: 0,
            items: Vec::new(),
            editor: None,
            panel: Panel::Welcome,
            overlay: None,
            prompt: None,
            filters: HashMap::new(),
            status: String::new(),
            output: Vec::new(),
            settings_fields: Vec::new(),
            panel_index: 0,
            marked_tags: HashSet::new(),
            scroll: 0,
            quit: false,
            events: domain_events::subscribe(),
            started_task_ids,
            completed_task_ids: HashSet::new(),
            airplane: std::env::var_os("BDS_AIRPLANE").is_some(),
            background_tx,
            background_rx,
            image_source: None,
            image_protocol: None,
            terminal_size: (80, 24),
        };
        app.reload()?;
        Ok(app)
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }
    pub fn locale(&self) -> UiLocale {
        self.locale
    }
    pub fn editor_text(&self) -> Option<String> {
        self.editor.as_ref().map(|editor| editor.buffer.text())
    }
    pub fn has_unsaved_changes(&self) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|editor| editor.buffer.is_dirty())
    }

    pub fn resize(&mut self, width: u16, height: u16) {
        let size = (width.max(20), height.max(6));
        if size != self.terminal_size {
            self.terminal_size = size;
            self.rebuild_image_protocol();
        }
    }

    fn rebuild_image_protocol(&mut self) {
        let Some(source) = self.image_source.clone() else {
            self.image_protocol = None;
            return;
        };
        let size = Size::new(
            self.terminal_size
                .0
                .saturating_sub(SIDEBAR_WIDTH + 4)
                .max(4),
            self.terminal_size.1.saturating_sub(6).max(2),
        );
        self.image_protocol = Picker::halfblocks()
            .new_protocol(source, size, Resize::Fit(None))
            .ok();
    }

    fn database(&self) -> Result<Database> {
        self.host.database()
    }
    fn project_id(&self) -> Result<&str> {
        self.project
            .as_ref()
            .map(|value| value.id.as_str())
            .ok_or_else(|| anyhow!(self.tr("tui.noActiveProject")))
    }
    fn data_dir(&self) -> Result<&Path> {
        self.data_dir
            .as_deref()
            .ok_or_else(|| anyhow!(self.tr("tui.noActiveProjectDataFolder")))
    }

    pub fn poll(&mut self) -> Result<()> {
        self.poll_tasks();
        while let Ok(result) = self.background_rx.try_recv() {
            self.status = result.status;
            if let Some(panel) = result.panel {
                self.panel = panel;
                self.focus = Focus::Editor;
            }
            if result.reload {
                self.reload()?;
            }
        }
        for event in self.events.drain() {
            if let bds_core::model::DomainEvent::SettingsChanged { key, .. } = &event
                && key == engine::settings::UI_LANGUAGE_KEY
            {
                let db = self.database()?;
                self.locale = engine::settings::ui_language(db.conn())?
                    .map(|v| normalize_language(&v))
                    .unwrap_or(UiLocale::En);
                self.reload()?;
                if let Panel::Settings(section) = self.panel {
                    self.settings_fields = self.load_setting_fields(section)?;
                    self.panel_index = self
                        .panel_index
                        .min(self.settings_fields.len().saturating_sub(1));
                }
                if self
                    .prompt
                    .as_ref()
                    .is_some_and(|prompt| prompt.kind == PromptKind::Command)
                {
                    let query = self
                        .prompt
                        .as_ref()
                        .map(|prompt| prompt.value.clone())
                        .unwrap_or_default();
                    let candidates = self
                        .command_names()
                        .into_iter()
                        .filter(|name| contains_ci(name, &query))
                        .collect();
                    if let Some(prompt) = &mut self.prompt {
                        prompt.candidates = candidates;
                    }
                }
                self.refresh_open_report()?;
            }
            if let bds_core::model::DomainEvent::EntityChanged {
                project_id,
                entity,
                entity_id,
                action,
            } = &event
                && self
                    .project
                    .as_ref()
                    .is_some_and(|project| &project.id == project_id)
            {
                let editing_deleted = *action == NotificationAction::Deleted
                    && match (&self.editor, entity) {
                        (
                            Some(Editor {
                                entity: EditorEntity::Post(id),
                                ..
                            }),
                            DomainEntity::Post,
                        )
                        | (
                            Some(Editor {
                                entity: EditorEntity::Template(id),
                                ..
                            }),
                            DomainEntity::Template,
                        )
                        | (
                            Some(Editor {
                                entity: EditorEntity::Script(id),
                                ..
                            }),
                            DomainEntity::Script,
                        ) => id == entity_id,
                        _ => false,
                    };
                if editing_deleted {
                    self.editor = None;
                    self.focus = Focus::Sidebar;
                    self.status = self.tr("tui.openItemDeleted");
                }
                self.reload()?;
                self.refresh_open_report()?;
            }
        }
        Ok(())
    }

    fn refresh_open_report(&mut self) -> Result<()> {
        let Panel::Report { action, .. } = self.panel else {
            return Ok(());
        };
        let db = self.database()?;
        let data_dir = self.data_dir()?;
        let project_id = self.project_id()?;
        self.panel = match action {
            ReportAction::MetadataDiff => Panel::Report {
                title: self.tr("menu.item.metadataDiff"),
                body: metadata_report_body(db.conn(), data_dir, project_id, self.locale)?,
                action,
            },
            ReportAction::SiteValidation => Panel::Report {
                title: self.tr("menu.item.validateSite"),
                body: site_validation_body(db.conn(), data_dir, project_id, self.locale)?,
                action,
            },
        };
        Ok(())
    }

    fn queue_task(
        &mut self,
        label: &str,
        work: impl FnOnce() -> Result<BackgroundResult> + Send + 'static,
    ) {
        let tasks = self.host.tasks();
        let task_id = tasks.submit(label);
        let sender = self.background_tx.clone();
        self.started_task_ids.insert(task_id);
        self.status = self.tr_with("tui.taskRunning", &[("label", label)]);
        std::thread::spawn(move || {
            if !tasks.wait_until_runnable(task_id) {
                return;
            }
            match work() {
                Ok(result) => {
                    tasks.complete(task_id);
                    let _ = sender.send(result);
                }
                Err(error) => {
                    let message = error.to_string();
                    tasks.fail(task_id, message.clone());
                    let _ = sender.send(BackgroundResult {
                        status: message,
                        panel: None,
                        reload: false,
                    });
                }
            }
        });
    }

    fn poll_tasks(&mut self) {
        for task in self.host.tasks().snapshots() {
            if self.started_task_ids.insert(task.id) {
                self.status = task.label.clone();
            }
            match task.status {
                engine::task::TaskStatus::Pending | engine::task::TaskStatus::Running => {
                    self.status = match task.progress {
                        Some(progress) => self.tr_with(
                            "tui.taskProgress",
                            &[
                                ("label", &task.label),
                                ("percent", &format!("{:.0}", progress * 100.0)),
                            ],
                        ),
                        None => task.label,
                    };
                }
                engine::task::TaskStatus::Completed if self.completed_task_ids.insert(task.id) => {
                    self.status = self.tr_with("tui.taskComplete", &[("label", &task.label)]);
                }
                engine::task::TaskStatus::Failed(error)
                    if self.completed_task_ids.insert(task.id) =>
                {
                    self.status = self.tr_with(
                        "tui.taskFailed",
                        &[("label", &task.label), ("error", &error)],
                    );
                }
                engine::task::TaskStatus::Cancelled if self.completed_task_ids.insert(task.id) => {
                    self.status = self.tr_with("tui.taskCancelled", &[("label", &task.label)]);
                }
                _ => {}
            }
        }
    }

    fn reload(&mut self) -> Result<()> {
        self.items = self.sidebar_items()?;
        self.clamp_selection();
        Ok(())
    }

    fn sidebar_items(&self) -> Result<Vec<SidebarItem>> {
        let Some(project) = &self.project else {
            return Ok(vec![SidebarItem::Empty(self.tr("tui.noProject"))]);
        };
        let db = self.database()?;
        let query = self
            .filters
            .get(&self.view)
            .map(String::as_str)
            .unwrap_or("");
        let mut items = Vec::new();
        match self.view {
            TuiView::Posts => {
                let posts =
                    bds_core::db::queries::post::list_posts_by_project(db.conn(), &project.id)?;
                for (heading_key, status) in [
                    ("sidebar.drafts", PostStatus::Draft),
                    ("sidebar.published", PostStatus::Published),
                    ("sidebar.archived", PostStatus::Archived),
                ] {
                    items.push(SidebarItem::Header(self.tr(heading_key)));
                    items.extend(
                        posts
                            .iter()
                            .filter(|post| {
                                post.status == status && matches_filter_post(post, query)
                            })
                            .map(|post| SidebarItem::Post(post.id.clone(), post.title.clone())),
                    );
                }
            }
            TuiView::Media => {
                items.push(SidebarItem::Header(self.tr("tui.viewMedia")));
                items.extend(
                    bds_core::db::queries::media::list_media_by_project(db.conn(), &project.id)?
                        .into_iter()
                        .filter(|media| matches_filter_media(media, query))
                        .map(|media| {
                            SidebarItem::Media(
                                media.id,
                                media.title.unwrap_or_else(|| media.filename.clone()),
                            )
                        }),
                );
            }
            TuiView::Templates => {
                items.push(SidebarItem::Header(self.tr("tui.viewTemplates")));
                items.extend(
                    bds_core::db::queries::template::list_templates_by_project(
                        db.conn(),
                        &project.id,
                    )?
                    .into_iter()
                    .filter(|item| contains_ci(&format!("{} {}", item.title, item.slug), query))
                    .map(|item| SidebarItem::Template(item.id, item.title)),
                );
            }
            TuiView::Scripts => {
                items.push(SidebarItem::Header(self.tr("tui.viewScripts")));
                items.extend(
                    bds_core::db::queries::script::list_scripts_by_project(db.conn(), &project.id)?
                        .into_iter()
                        .filter(|item| contains_ci(&format!("{} {}", item.title, item.slug), query))
                        .map(|item| SidebarItem::Script(item.id, item.title)),
                );
            }
            TuiView::Tags => {
                items.push(SidebarItem::Header(self.tr("tui.viewTags")));
                items.extend(TagSection::ALL.into_iter().map(SidebarItem::TagSection));
            }
            TuiView::Settings => {
                items.push(SidebarItem::Header(self.tr("tui.viewSettings")));
                items.extend(
                    SettingSection::ALL
                        .into_iter()
                        .map(SidebarItem::SettingSection),
                );
            }
            TuiView::Git => return self.git_items(),
        }
        if !items.iter().any(SidebarItem::selectable) {
            items.push(SidebarItem::Empty(self.tr("tui.noMatchingItems")));
        }
        Ok(items)
    }

    fn clamp_selection(&mut self) {
        if self.items.is_empty() {
            self.selected_index = 0;
            return;
        }
        self.selected_index = self.selected_index.min(self.items.len() - 1);
        if !self.items[self.selected_index].selectable() {
            self.selected_index = self
                .items
                .iter()
                .position(SidebarItem::selectable)
                .unwrap_or(0);
        }
    }
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn matches_filter_post(post: &Post, query: &str) -> bool {
    query.split_whitespace().all(|token| {
        if let Some(tag) = token.strip_prefix("tag:") {
            post.tags
                .iter()
                .any(|value| value.eq_ignore_ascii_case(tag))
        } else if let Some(category) = token.strip_prefix("category:") {
            post.categories
                .iter()
                .any(|value| value.eq_ignore_ascii_case(category))
        } else if token.len() == 10
            && token.as_bytes().get(4) == Some(&b'-')
            && token.as_bytes().get(7) == Some(&b'-')
        {
            chrono::DateTime::from_timestamp_millis(post.created_at)
                .is_some_and(|date| date.format("%Y-%m-%d").to_string() == token)
        } else {
            contains_ci(
                &format!(
                    "{} {} {}",
                    post.title,
                    post.slug,
                    post.excerpt.as_deref().unwrap_or("")
                ),
                token,
            )
        }
    })
}

fn matches_filter_media(media: &Media, query: &str) -> bool {
    query.split_whitespace().all(|token| {
        contains_ci(
            &format!(
                "{} {} {}",
                media.title.as_deref().unwrap_or(""),
                media.filename,
                media.alt.as_deref().unwrap_or("")
            ),
            token,
        )
    })
}

impl TuiApp {
    pub fn handle_input(&mut self, input: TuiInput) -> Result<()> {
        if let Err(error) = self.handle_input_inner(input) {
            self.status = error.to_string();
        }
        Ok(())
    }

    fn handle_input_inner(&mut self, input: TuiInput) -> Result<()> {
        if self.prompt.is_some() {
            return self.handle_prompt(input);
        }
        if self.overlay.is_some() {
            return self.handle_overlay(input);
        }
        if input.modifiers.ctrl {
            return match input.key {
                TuiKey::Char('s') => self.save(false),
                TuiKey::Char('p') => self.save(true),
                TuiKey::Char('e') => {
                    self.toggle_preview();
                    Ok(())
                }
                TuiKey::Char('g') => self.ai_quick_action(),
                TuiKey::Char('u') => self.unpublish(),
                TuiKey::Char('t') => self.edit_title(),
                TuiKey::Char('l') => self.cycle_editor_language(),
                TuiKey::Char('q') => {
                    self.request_quit();
                    Ok(())
                }
                _ => self.handle_editor_input(input),
            };
        }
        if self.focus == Focus::Editor && self.editor.is_none() {
            return self.handle_panel_input(input);
        }
        if self.focus == Focus::Editor {
            return self.handle_editor_input(input);
        }
        match input.key {
            TuiKey::Char('q') => self.request_quit(),
            TuiKey::Char('j') | TuiKey::Down => self.move_selection(1),
            TuiKey::Char('k') | TuiKey::Up => self.move_selection(-1),
            TuiKey::Enter => self.open_selected()?,
            TuiKey::Char('n') => self.new_item()?,
            TuiKey::Char('/') => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::Search,
                    value: self.filters.get(&self.view).cloned().unwrap_or_default(),
                    candidates: Vec::new(),
                })
            }
            TuiKey::Char(':') => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::Command,
                    value: String::new(),
                    candidates: self.command_names(),
                })
            }
            TuiKey::Char('p') => self.overlay = Some(Overlay::Projects { selected: 0 }),
            TuiKey::Char('o') => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::ProjectPath,
                    value: String::new(),
                    candidates: Vec::new(),
                })
            }
            TuiKey::Char('1') => self.set_view(TuiView::Posts)?,
            TuiKey::Char('2') => self.set_view(TuiView::Media)?,
            TuiKey::Char('3') => self.set_view(TuiView::Templates)?,
            TuiKey::Char('4') => self.set_view(TuiView::Scripts)?,
            TuiKey::Char('5') => self.set_view(TuiView::Tags)?,
            TuiKey::Char('6') => self.set_view(TuiView::Settings)?,
            TuiKey::Char('7') => self.set_view(TuiView::Git)?,
            TuiKey::PageUp => self.scroll = self.scroll.saturating_sub(10),
            TuiKey::PageDown => self.scroll = self.scroll.saturating_add(10),
            TuiKey::Char('c') if self.view == TuiView::Git => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::Commit,
                    value: String::new(),
                    candidates: Vec::new(),
                })
            }
            TuiKey::Char('u') if self.view == TuiView::Git => self.git_pull()?,
            TuiKey::Char('s') if self.view == TuiView::Git => self.git_push()?,
            TuiKey::Char(' ') if self.view == TuiView::Tags => self.toggle_tag_mark(),
            TuiKey::Char('m') if self.view == TuiView::Tags => self.merge_marked_tags()?,
            TuiKey::Char('d') if self.view == TuiView::Tags => self.begin_tag_delete()?,
            TuiKey::Char('c') if self.view == TuiView::Tags => self.cycle_tag_color()?,
            TuiKey::Char('t') if self.view == TuiView::Tags => self.cycle_tag_template()?,
            TuiKey::Char('s') if self.view == TuiView::Tags => self.sync_tags()?,
            TuiKey::Esc => {
                self.panel = Panel::Welcome;
                self.scroll = 0;
            }
            _ => {}
        }
        Ok(())
    }

    fn request_quit(&mut self) {
        if self.has_unsaved_changes() {
            self.overlay = Some(Overlay::ConfirmDiscard);
        } else {
            self.quit = true;
        }
    }

    fn set_view(&mut self, view: TuiView) -> Result<()> {
        self.view = view;
        self.focus = Focus::Sidebar;
        self.selected_index = 0;
        self.editor = None;
        self.image_source = None;
        self.image_protocol = None;
        self.panel = match view {
            TuiView::Tags => Panel::Tags(TagSection::Cloud),
            TuiView::Settings => Panel::Settings(SettingSection::Project),
            TuiView::Git => Panel::Git,
            _ => Panel::Welcome,
        };
        self.reload()
    }

    fn move_selection(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let mut index = self.selected_index;
        for _ in 0..self.items.len() {
            index = if delta > 0 {
                (index + 1) % self.items.len()
            } else {
                index.checked_sub(1).unwrap_or(self.items.len() - 1)
            };
            if self.items[index].selectable() {
                self.selected_index = index;
                break;
            }
        }
    }

    fn open_selected(&mut self) -> Result<()> {
        let Some(item) = self.items.get(self.selected_index).cloned() else {
            return Ok(());
        };
        match item {
            SidebarItem::Post(id, _) => self.open_post(&id),
            SidebarItem::Template(id, _) => self.open_template(&id),
            SidebarItem::Script(id, _) => self.open_script(&id),
            SidebarItem::Media(id, _) => self.open_media(&id),
            SidebarItem::TagSection(section) => {
                self.panel = Panel::Tags(section);
                self.panel_index = 0;
                self.focus = Focus::Editor;
                Ok(())
            }
            SidebarItem::SettingSection(section) => self.open_settings(section),
            SidebarItem::GitFile(path, _) => self.open_git_file(&path),
            SidebarItem::GitCommit(hash, _) => self.open_git_commit(&hash),
            _ => Ok(()),
        }
    }

    fn new_item(&mut self) -> Result<()> {
        let db = self.database()?;
        let project_id = self.project_id()?.to_owned();
        let untitled = self.tr("tui.untitled");
        let entity = match self.view {
            TuiView::Posts => EditorEntity::Post(
                engine::post::create_post(
                    db.conn(),
                    self.data_dir()?,
                    &project_id,
                    &untitled,
                    Some(""),
                    Vec::new(),
                    Vec::new(),
                    None,
                    None,
                    None,
                )?
                .id,
            ),
            TuiView::Templates => EditorEntity::Template(
                engine::template::create_template(
                    db.conn(),
                    &project_id,
                    &untitled,
                    TemplateKind::Post,
                    "",
                )?
                .id,
            ),
            TuiView::Scripts => EditorEntity::Script(
                engine::script::create_script(
                    db.conn(),
                    &project_id,
                    &untitled,
                    ScriptKind::Utility,
                    "function main()\nend\n",
                    Some("main"),
                )?
                .id,
            ),
            TuiView::Tags => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::TagCreate,
                    value: String::new(),
                    candidates: Vec::new(),
                });
                return Ok(());
            }
            _ => {
                self.status = self.tr("tui.newItemUnavailable");
                return Ok(());
            }
        };
        match entity {
            EditorEntity::Post(id) => self.open_post(&id),
            EditorEntity::Template(id) => self.open_template(&id),
            EditorEntity::Script(id) => self.open_script(&id),
        }
    }

    fn open_post(&mut self, id: &str) -> Result<()> {
        let db = self.database()?;
        let post = bds_core::db::queries::post::get_post_by_id(db.conn(), id)?;
        let content = if let Some(content) = post.content.clone() {
            content
        } else {
            read_published_body(self.data_dir()?, &post.file_path, PublishedKind::Post)?
        };
        self.editor = Some(editor(
            EditorEntity::Post(post.id),
            post.title,
            "markdown",
            post.language,
            &content,
        ));
        self.focus = Focus::Editor;
        Ok(())
    }

    fn open_template(&mut self, id: &str) -> Result<()> {
        let db = self.database()?;
        let item = bds_core::db::queries::template::get_template_by_id(db.conn(), id)?;
        let content = if let Some(content) = item.content.clone() {
            content
        } else {
            read_published_body(self.data_dir()?, &item.file_path, PublishedKind::Template)?
        };
        self.editor = Some(editor(
            EditorEntity::Template(item.id),
            item.title,
            "html",
            None,
            &content,
        ));
        self.focus = Focus::Editor;
        Ok(())
    }

    fn open_script(&mut self, id: &str) -> Result<()> {
        let db = self.database()?;
        let item = bds_core::db::queries::script::get_script_by_id(db.conn(), id)?;
        let content = if let Some(content) = item.content.clone() {
            content
        } else {
            read_published_body(self.data_dir()?, &item.file_path, PublishedKind::Script)?
        };
        self.editor = Some(editor(
            EditorEntity::Script(item.id),
            item.title,
            "lua",
            None,
            &content,
        ));
        self.focus = Focus::Editor;
        Ok(())
    }

    fn open_media(&mut self, id: &str) -> Result<()> {
        let db = self.database()?;
        let item = bds_core::db::queries::media::get_media_by_id(db.conn(), id)?;
        let path = self.data_dir()?.join(item.file_path);
        if item.mime_type.starts_with("image/") {
            self.image_source = image::ImageReader::open(&path)
                .ok()
                .and_then(|reader| reader.decode().ok());
            self.rebuild_image_protocol();
        } else {
            self.image_source = None;
            self.image_protocol = None;
        }
        self.panel = Panel::MediaPreview {
            title: item.title.unwrap_or(item.original_name),
            path,
        };
        Ok(())
    }

    fn handle_editor_input(&mut self, input: TuiInput) -> Result<()> {
        let Some(editor) = self.editor.as_mut() else {
            if input.key == TuiKey::Esc {
                self.focus = Focus::Sidebar;
            }
            return Ok(());
        };
        if input.key == TuiKey::Esc {
            self.focus = Focus::Sidebar;
            return Ok(());
        }
        if editor.mode == EditorMode::Preview {
            return Ok(());
        }
        let shift = input.modifiers.shift;
        match input.key {
            TuiKey::Char(value) if !input.modifiers.ctrl && !input.modifiers.alt => {
                editor.buffer.insert(&value.to_string())
            }
            TuiKey::Enter => editor.buffer.insert("\n"),
            TuiKey::Tab => editor.buffer.insert("    "),
            TuiKey::Backspace => editor.buffer.backspace(),
            TuiKey::Delete => editor.buffer.delete_forward(),
            TuiKey::Up if shift => editor.buffer.select_up(),
            TuiKey::Up => editor.buffer.move_up(),
            TuiKey::Down if shift => editor.buffer.select_down(),
            TuiKey::Down => editor.buffer.move_down(),
            TuiKey::Left if shift => editor.buffer.select_left(),
            TuiKey::Left => editor.buffer.move_left(),
            TuiKey::Right if shift => editor.buffer.select_right(),
            TuiKey::Right => editor.buffer.move_right(),
            TuiKey::Home if shift => editor.buffer.select_home(),
            TuiKey::Home => editor.buffer.move_home(),
            TuiKey::End if shift => editor.buffer.select_end(),
            TuiKey::End => editor.buffer.move_end(),
            TuiKey::PageUp => editor.buffer.move_page_up(20),
            TuiKey::PageDown => editor.buffer.move_page_down(20),
            TuiKey::Char('z') if input.modifiers.ctrl => editor.buffer.undo(),
            TuiKey::Char('y') if input.modifiers.ctrl => editor.buffer.redo(),
            _ => {}
        }
        let visible_width = self.terminal_size.0.saturating_sub(SIDEBAR_WIDTH).max(1) as usize;
        let visible_height = self.terminal_size.1.saturating_sub(3).max(1) as usize;
        let (cursor_visual_line, total_visual_lines) =
            visual_line_metrics(&editor.buffer, visible_width);
        editor.buffer.ensure_visual_line_visible(
            cursor_visual_line,
            visible_height,
            total_visual_lines.saturating_sub(visible_height),
        );
        Ok(())
    }

    fn toggle_preview(&mut self) {
        if let Some(editor) = &mut self.editor {
            editor.mode = if editor.mode == EditorMode::Source {
                EditorMode::Preview
            } else {
                EditorMode::Source
            };
            self.status = if editor.mode == EditorMode::Preview {
                bds_core::i18n::translate(self.locale, "editor.modePreview")
            } else {
                bds_core::i18n::translate(self.locale, "tui.editorSource")
            };
        }
    }

    fn save(&mut self, publish: bool) -> Result<()> {
        let Some(editor) = &self.editor else {
            return self.save_settings();
        };
        let entity = editor.entity.clone();
        let title = editor.title.clone();
        let post_language = editor.post_language.clone();
        let content = editor.buffer.text();
        let db = self.database()?;
        match entity {
            EditorEntity::Post(id) => {
                engine::post::update_post(
                    db.conn(),
                    self.data_dir()?,
                    &id,
                    Some(&title),
                    None,
                    None,
                    Some(&content),
                    None,
                    None,
                    None,
                    Some(post_language.as_deref()),
                    None,
                    None,
                )?;
                if publish {
                    engine::post::publish_post(db.conn(), self.data_dir()?, &id)?;
                }
            }
            EditorEntity::Template(id) => {
                engine::template::validate_template(&content).map_err(|error| anyhow!(error))?;
                let project_id = self.project_id()?.to_owned();
                engine::template::update_template(
                    db.conn(),
                    &id,
                    &project_id,
                    Some(&title),
                    None,
                    None,
                    None,
                    Some(&content),
                )?;
                if publish {
                    let item = bds_core::db::queries::template::get_template_by_id(db.conn(), &id)?;
                    if item.status == TemplateStatus::Published {
                        engine::template::unpublish_template(db.conn(), self.data_dir()?, &id)?;
                    }
                    engine::template::publish_template(db.conn(), self.data_dir()?, &id)?;
                }
            }
            EditorEntity::Script(id) => {
                engine::script::validate_script_syntax(&content).map_err(|error| anyhow!(error))?;
                let project_id = self.project_id()?.to_owned();
                engine::script::update_script(
                    db.conn(),
                    &id,
                    &project_id,
                    Some(&title),
                    None,
                    None,
                    None,
                    None,
                    Some(&content),
                )?;
                if publish {
                    let item = bds_core::db::queries::script::get_script_by_id(db.conn(), &id)?;
                    if item.status == ScriptStatus::Published {
                        engine::script::unpublish_script(db.conn(), self.data_dir()?, &id)?;
                    }
                    engine::script::publish_script(db.conn(), self.data_dir()?, &id)?;
                }
            }
        }
        if let Some(editor) = &mut self.editor {
            editor.buffer.set_dirty(false);
        }
        self.status = if publish {
            self.tr("tui.savedAndPublished")
        } else {
            self.tr("tui.saved")
        };
        self.reload()
    }

    fn ai_quick_action(&mut self) -> Result<()> {
        let Some(Editor {
            entity: EditorEntity::Post(_),
            title,
            buffer,
            ..
        }) = &self.editor
        else {
            return Ok(());
        };
        let db = self.database()?;
        if let Err(error) = engine::ai::active_endpoint(db.conn(), self.airplane) {
            self.status = if self.airplane {
                self.tr("tui.airplaneAiEndpointRequired")
            } else {
                error.to_string()
            };
            return Ok(());
        }
        let request = engine::ai::OneShotRequest {
            operation: engine::ai::OneShotOperation::AnalyzePost,
            content: serde_json::json!({ "title": title, "content": buffer.text() }),
        };
        let (response, _) = engine::ai::run_one_shot(db.conn(), self.airplane, &request)?;
        let engine::ai::OneShotResponse::PostAnalysis(analysis) = response else {
            bail!(self.tr("tui.aiWrongResponseType"));
        };
        self.output.push(self.tr_with(
            "tui.aiSuggestions",
            &[("title", &analysis.title), ("excerpt", &analysis.excerpt)],
        ));
        self.status = self.tr("tui.aiSuggestionsAdded");
        Ok(())
    }

    fn unpublish(&mut self) -> Result<()> {
        let Some(editor) = &self.editor else {
            return Ok(());
        };
        let db = self.database()?;
        match &editor.entity {
            EditorEntity::Post(id) => engine::post::archive_post(db.conn(), self.data_dir()?, id)?,
            EditorEntity::Template(id) => {
                engine::template::unpublish_template(db.conn(), self.data_dir()?, id)?;
            }
            EditorEntity::Script(id) => {
                engine::script::unpublish_script(db.conn(), self.data_dir()?, id)?;
            }
        }
        self.status = self.tr("tui.unpublished");
        self.reload()
    }

    fn edit_title(&mut self) -> Result<()> {
        if let Some(editor) = &self.editor {
            self.prompt = Some(Prompt {
                kind: PromptKind::EditorTitle,
                value: editor.title.clone(),
                candidates: Vec::new(),
            });
        }
        Ok(())
    }

    fn cycle_editor_language(&mut self) -> Result<()> {
        let Some(editor) = &mut self.editor else {
            return Ok(());
        };
        if !matches!(editor.entity, EditorEntity::Post(_)) {
            return Ok(());
        }
        let mut languages = vec!["en".to_string()];
        if let Some(data_dir) = &self.data_dir
            && let Ok(metadata) = engine::meta::read_project_json(data_dir)
        {
            if let Some(main) = metadata.main_language {
                languages.insert(0, main);
            }
            languages.extend(metadata.blog_languages);
        }
        languages.sort();
        languages.dedup();
        let current = editor
            .post_language
            .as_deref()
            .unwrap_or(languages.first().map(String::as_str).unwrap_or("en"));
        let index = languages
            .iter()
            .position(|value| value == current)
            .unwrap_or(0);
        editor.post_language = Some(languages[(index + 1) % languages.len()].clone());
        editor.buffer.set_dirty(true);
        self.status = bds_core::i18n::translate_with(
            self.locale,
            "tui.editorLanguage",
            &[("language", editor.post_language.as_deref().unwrap_or("en"))],
        );
        Ok(())
    }

    fn handle_panel_input(&mut self, input: TuiInput) -> Result<()> {
        match input.key {
            TuiKey::Esc => {
                self.focus = Focus::Sidebar;
                self.prompt = None;
                self.panel = Panel::Welcome;
                self.settings_fields.clear();
                self.scroll = 0;
            }
            TuiKey::Up | TuiKey::Char('k') => {
                if matches!(self.panel, Panel::Settings(_)) {
                    self.move_setting_selection(-1);
                } else {
                    self.panel_index = self.panel_index.saturating_sub(1);
                }
            }
            TuiKey::Down | TuiKey::Char('j') => {
                if matches!(self.panel, Panel::Settings(_)) {
                    self.move_setting_selection(1);
                } else {
                    self.panel_index = self.panel_index.saturating_add(1);
                }
            }
            TuiKey::PageUp => self.scroll = self.scroll.saturating_sub(10),
            TuiKey::PageDown => self.scroll = self.scroll.saturating_add(10),
            TuiKey::Enter => match self.panel {
                Panel::Settings(_) => self.edit_setting_field()?,
                Panel::Tags(TagSection::Manage) => self.begin_tag_rename()?,
                Panel::Report { action, .. } => self.apply_report(action)?,
                _ => {}
            },
            TuiKey::Char('n') if matches!(self.panel, Panel::Tags(TagSection::Manage)) => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::TagCreate,
                    value: String::new(),
                    candidates: Vec::new(),
                })
            }
            TuiKey::Char(' ') if matches!(self.panel, Panel::Tags(TagSection::Merge)) => {
                self.toggle_tag_mark()
            }
            TuiKey::Char('m') if matches!(self.panel, Panel::Tags(TagSection::Merge)) => {
                self.merge_marked_tags()?
            }
            TuiKey::Char('d') if matches!(self.panel, Panel::Tags(TagSection::Manage)) => {
                self.begin_tag_delete()?
            }
            TuiKey::Char('c') if matches!(self.panel, Panel::Tags(TagSection::Manage)) => {
                self.cycle_tag_color()?
            }
            TuiKey::Char('t') if matches!(self.panel, Panel::Tags(TagSection::Manage)) => {
                self.cycle_tag_template()?
            }
            TuiKey::Char('s') if matches!(self.panel, Panel::Tags(_)) => self.sync_tags()?,
            _ => {}
        }
        let max = match self.panel {
            Panel::Settings(_) => self.settings_fields.len(),
            Panel::Tags(_) => self.tags().map(|tags| tags.len()).unwrap_or(0),
            _ => usize::MAX,
        };
        if max > 0 {
            self.panel_index = self.panel_index.min(max - 1);
        } else {
            self.panel_index = 0;
        }
        Ok(())
    }

    fn move_setting_selection(&mut self, delta: isize) {
        if self.settings_fields.is_empty() {
            return;
        }
        let mut index = self.panel_index;
        for _ in 0..self.settings_fields.len() {
            index = if delta > 0 {
                (index + 1) % self.settings_fields.len()
            } else {
                index
                    .checked_sub(1)
                    .unwrap_or(self.settings_fields.len() - 1)
            };
            if self.settings_fields[index].kind != FieldKind::ReadOnly {
                self.panel_index = index;
                return;
            }
        }
    }

    fn handle_prompt(&mut self, input: TuiInput) -> Result<()> {
        let Some(mut prompt) = self.prompt.take() else {
            return Ok(());
        };
        let refilter = matches!(input.key, TuiKey::Char(_) | TuiKey::Backspace);
        match input.key {
            TuiKey::Esc => {
                if prompt.kind == PromptKind::Search {
                    self.filters.remove(&self.view);
                    self.reload()?;
                } else if prompt.kind == PromptKind::ProjectPath {
                    self.overlay = Some(Overlay::Projects { selected: 0 });
                }
                self.status.clear();
                return Ok(());
            }
            TuiKey::Backspace => {
                prompt.value.pop();
            }
            TuiKey::Char(value) if !input.modifiers.ctrl && !input.modifiers.alt => {
                prompt.value.push(value);
                if prompt.kind == PromptKind::ProjectPath {
                    prompt.candidates.clear();
                }
            }
            TuiKey::Tab if prompt.kind == PromptKind::ProjectPath => complete_path(&mut prompt),
            TuiKey::Up if prompt.kind == PromptKind::Command && !prompt.candidates.is_empty() => {
                prompt.candidates.rotate_right(1)
            }
            TuiKey::Down if prompt.kind == PromptKind::Command && !prompt.candidates.is_empty() => {
                prompt.candidates.rotate_left(1)
            }
            TuiKey::Enter => return self.submit_prompt(prompt),
            _ => {}
        }
        if prompt.kind == PromptKind::Search {
            self.filters.insert(self.view, prompt.value.clone());
            self.reload()?;
        } else if prompt.kind == PromptKind::Command && refilter {
            prompt.candidates = self
                .command_names()
                .iter()
                .filter(|name| contains_ci(name, &prompt.value))
                .cloned()
                .collect();
        }
        self.prompt = Some(prompt);
        Ok(())
    }

    fn submit_prompt(&mut self, prompt: Prompt) -> Result<()> {
        match prompt.kind {
            PromptKind::Search => {
                self.filters.insert(self.view, prompt.value);
                self.reload()?;
            }
            PromptKind::Command => {
                let typed = prompt.value.trim_start_matches(':').trim();
                let selected = if typed == "?" || self.command_id(typed).is_some() {
                    typed.to_owned()
                } else {
                    prompt
                        .candidates
                        .first()
                        .cloned()
                        .unwrap_or_else(|| typed.to_owned())
                };
                self.run_command(&selected)?;
            }
            PromptKind::ProjectPath => self.open_project_path(&prompt.value)?,
            PromptKind::Commit => {
                if !self.require_git()? {
                    return Ok(());
                } else if prompt.value.trim().is_empty() {
                    self.status = self.tr("tui.commitMessageRequired");
                    self.prompt = Some(prompt);
                } else {
                    let data_dir = self.data_dir()?.to_owned();
                    let message = prompt.value;
                    let locale = self.locale;
                    let label = self.tr("tui.taskGitCommit");
                    self.queue_task(&label, move || {
                        let output = engine::git::GitEngine::new(&data_dir)
                            .commit_all(&message)?
                            .output;
                        Ok(BackgroundResult {
                            status: if output.trim().is_empty() {
                                bds_core::i18n::translate(locale, "tui.committed")
                            } else {
                                output
                            },
                            panel: Some(Panel::Git),
                            reload: true,
                        })
                    });
                }
            }
            PromptKind::Setting(index) => {
                if let Some(field) = self.settings_fields.get_mut(index) {
                    field.value = prompt.value;
                }
            }
            PromptKind::TagCreate => {
                if prompt.value.trim().is_empty() {
                    self.status = self.tr("tui.tagNameRequired");
                    self.prompt = Some(prompt);
                } else {
                    let db = self.database()?;
                    engine::tag::create_tag(
                        db.conn(),
                        self.data_dir()?,
                        self.project_id()?,
                        prompt.value.trim(),
                        None,
                    )?;
                    self.status = self.tr("tui.tagCreated");
                }
            }
            PromptKind::TagRename(id) => {
                if prompt.value.trim().is_empty() {
                    self.status = self.tr("tui.tagNameRequired");
                } else {
                    let db = self.database()?;
                    engine::tag::rename_tag(
                        db.conn(),
                        self.data_dir()?,
                        self.project_id()?,
                        &id,
                        prompt.value.trim(),
                    )?;
                    self.status = self.tr("tui.tagRenamed");
                }
            }
            PromptKind::ConfirmDeleteTag(id) => {
                if prompt.value.eq_ignore_ascii_case("y")
                    || prompt
                        .value
                        .eq_ignore_ascii_case(&self.tr("tui.confirmYesInput"))
                {
                    let db = self.database()?;
                    engine::tag::delete_tag(db.conn(), self.data_dir()?, self.project_id()?, &id)?;
                    self.status = self.tr("tui.tagDeleted");
                } else {
                    self.status = self.tr("tui.deleteCancelled");
                }
            }
            PromptKind::EditorTitle => {
                if prompt.value.trim().is_empty() {
                    self.status = self.tr("tui.titleRequired");
                    self.prompt = Some(prompt);
                } else if let Some(editor) = &mut self.editor {
                    editor.title = prompt.value.trim().to_owned();
                    editor.buffer.set_dirty(true);
                }
            }
        }
        Ok(())
    }

    fn handle_overlay(&mut self, input: TuiInput) -> Result<()> {
        let Some(mut overlay) = self.overlay.take() else {
            return Ok(());
        };
        match &mut overlay {
            Overlay::ConfirmDiscard => match input.key {
                TuiKey::Char(value)
                    if value == 'y'
                        || self
                            .tr("tui.confirmYesInput")
                            .starts_with(value.to_ascii_lowercase()) =>
                {
                    self.quit = true
                }
                TuiKey::Char(value)
                    if value == 'n'
                        || self
                            .tr("tui.confirmNoInput")
                            .starts_with(value.to_ascii_lowercase()) => {}
                TuiKey::Esc => {}
                _ => self.overlay = Some(overlay),
            },
            Overlay::Projects { selected } => {
                let projects = engine::project::list_projects(self.database()?.conn())?;
                match input.key {
                    TuiKey::Esc => {}
                    TuiKey::Up | TuiKey::Char('k') => *selected = selected.saturating_sub(1),
                    TuiKey::Down | TuiKey::Char('j') => {
                        *selected = (*selected + 1).min(projects.len().saturating_sub(1))
                    }
                    TuiKey::Enter => {
                        if let Some(project) = projects.get(*selected) {
                            self.activate_project(project.clone())?;
                            return Ok(());
                        }
                    }
                    TuiKey::Char('o') => {
                        self.prompt = Some(Prompt {
                            kind: PromptKind::ProjectPath,
                            value: String::new(),
                            candidates: Vec::new(),
                        });
                        return Ok(());
                    }
                    _ => {}
                }
                if !matches!(input.key, TuiKey::Esc) {
                    self.overlay = Some(overlay);
                }
            }
        }
        Ok(())
    }

    fn activate_project(&mut self, project: Project) -> Result<()> {
        let db = self.database()?;
        engine::project::set_active_project(db.conn(), &project.id)?;
        self.data_dir = Some(self.host.project_data_dir(&project));
        self.project = Some(project);
        self.editor = None;
        self.panel = Panel::Welcome;
        self.status = self.tr("tui.projectSwitched");
        self.reload()
    }

    fn open_project_path(&mut self, value: &str) -> Result<()> {
        let path = expand_home(value);
        if !path.is_dir() {
            self.status =
                self.tr_with("tui.notDirectory", &[("path", &path.display().to_string())]);
            self.prompt = Some(Prompt {
                kind: PromptKind::ProjectPath,
                value: value.into(),
                candidates: Vec::new(),
            });
            return Ok(());
        }
        let canonical = path.canonicalize()?;
        let db = self.database()?;
        if let Some(project) =
            engine::project::list_projects(db.conn())?
                .into_iter()
                .find(|project| {
                    project
                        .data_path
                        .as_deref()
                        .map(Path::new)
                        .is_some_and(|existing| existing == canonical)
                })
        {
            return self.activate_project(project);
        }
        let name = canonical
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| self.tr("tui.defaultProjectName"));
        let project = engine::project::create_project(
            db.conn(),
            &name,
            Some(
                canonical
                    .to_str()
                    .ok_or_else(|| anyhow!(self.tr("tui.projectPathInvalidUtf8")))?,
            ),
        )?;
        engine::project::set_active_project(db.conn(), &project.id)?;
        let report = engine::rebuild::rebuild_from_filesystem(db.conn(), &canonical, &project.id)?;
        self.output.push(self.tr_with(
            "tui.rebuildSummary",
            &[
                (
                    "posts",
                    &(report.posts_created + report.posts_updated).to_string(),
                ),
                (
                    "media",
                    &(report.media_created + report.media_updated).to_string(),
                ),
                (
                    "templates",
                    &(report.templates_created + report.templates_updated).to_string(),
                ),
                (
                    "scripts",
                    &(report.scripts_created + report.scripts_updated).to_string(),
                ),
            ],
        ));
        self.activate_project(project)
    }
}

fn expand_home(value: &str) -> PathBuf {
    if value == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(value));
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(rest);
    }
    PathBuf::from(value)
}

fn complete_path(prompt: &mut Prompt) {
    let expanded = expand_home(&prompt.value);
    let (directory, prefix) = if expanded.is_dir() {
        (expanded.as_path(), "")
    } else {
        (
            expanded.parent().unwrap_or(Path::new(".")),
            expanded.file_name().and_then(|v| v.to_str()).unwrap_or(""),
        )
    };
    let show_dot = prefix.starts_with('.');
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut candidates = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| (show_dot || !name.starts_with('.')) && name.starts_with(prefix))
        .collect::<Vec<_>>();
    candidates.sort();
    if candidates.is_empty() {
        return;
    }
    let completion = longest_common_prefix(&candidates);
    let parent = Path::new(&prompt.value).parent().unwrap_or(Path::new(""));
    prompt.value = parent.join(completion).to_string_lossy().into_owned();
    if candidates.len() == 1 {
        prompt.value.push(std::path::MAIN_SEPARATOR);
    }
    prompt.candidates = candidates;
}

fn longest_common_prefix(values: &[String]) -> String {
    let Some(first) = values.first() else {
        return String::new();
    };
    first
        .chars()
        .take_while(|_| true)
        .enumerate()
        .take_while(|(index, value)| {
            values
                .iter()
                .all(|candidate| candidate.chars().nth(*index) == Some(*value))
        })
        .map(|(_, value)| value)
        .collect()
}

impl TuiApp {
    fn open_settings(&mut self, section: SettingSection) -> Result<()> {
        self.settings_fields = self.load_setting_fields(section)?;
        self.panel = Panel::Settings(section);
        self.panel_index = self
            .settings_fields
            .iter()
            .position(|field| field.kind != FieldKind::ReadOnly)
            .unwrap_or(0);
        self.focus = Focus::Editor;
        Ok(())
    }

    fn load_setting_fields(&self, section: SettingSection) -> Result<Vec<SettingField>> {
        let db = self.database()?;
        let project = self.project.as_ref();
        let metadata = self
            .data_dir()
            .ok()
            .and_then(|path| engine::meta::read_project_json(path).ok());
        let publishing = self
            .data_dir()
            .ok()
            .and_then(|path| engine::meta::read_publishing_json(path).ok())
            .unwrap_or_default();
        let ai = engine::ai::load_ai_settings(db.conn(), self.airplane)?;
        let specs: Vec<(&'static str, FieldKind, String)> = match section {
            SettingSection::Project => vec![
                (
                    "project.name",
                    FieldKind::Text,
                    project.map(|p| p.name.clone()).unwrap_or_default(),
                ),
                (
                    "project.description",
                    FieldKind::Text,
                    project
                        .and_then(|p| p.description.clone())
                        .unwrap_or_default(),
                ),
                (
                    "project.data_path",
                    FieldKind::ReadOnly,
                    self.data_dir
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                ),
                (
                    engine::settings::UI_LANGUAGE_KEY,
                    FieldKind::Enum(&["en", "de", "fr", "it", "es"]),
                    self.locale.code().into(),
                ),
                (
                    "meta.default_author",
                    FieldKind::Text,
                    metadata
                        .as_ref()
                        .and_then(|value| value.default_author.clone())
                        .unwrap_or_default(),
                ),
                (
                    "meta.max_posts_per_page",
                    FieldKind::Text,
                    metadata
                        .as_ref()
                        .map(|value| value.max_posts_per_page.to_string())
                        .unwrap_or_else(|| "50".into()),
                ),
                (
                    "meta.main_language",
                    FieldKind::Text,
                    metadata
                        .as_ref()
                        .and_then(|value| value.main_language.clone())
                        .unwrap_or_default(),
                ),
                (
                    "meta.blog_languages",
                    FieldKind::Text,
                    metadata
                        .as_ref()
                        .map(|value| value.blog_languages.join(", "))
                        .unwrap_or_default(),
                ),
                (
                    "meta.public_url",
                    FieldKind::Text,
                    metadata
                        .as_ref()
                        .and_then(|value| value.public_url.clone())
                        .unwrap_or_default(),
                ),
                (
                    "meta.image_import_concurrency",
                    FieldKind::Text,
                    metadata
                        .as_ref()
                        .map(|value| value.image_import_concurrency.to_string())
                        .unwrap_or_else(|| "4".into()),
                ),
                (
                    "meta.blogmark_category",
                    FieldKind::Text,
                    metadata
                        .as_ref()
                        .and_then(|value| value.blogmark_category.clone())
                        .unwrap_or_default(),
                ),
            ],
            SettingSection::Editor => field_specs(&[
                (
                    "editor.default_mode",
                    FieldKind::Enum(&["markdown", "preview"]),
                    "markdown",
                ),
                (
                    "editor.diff_view_style",
                    FieldKind::Enum(&["inline", "side-by-side"]),
                    "inline",
                ),
                ("editor.wrap_long_lines", FieldKind::Bool, "true"),
                ("editor.hide_unchanged_regions", FieldKind::Bool, "false"),
            ]),
            SettingSection::Content => vec![
                (
                    "meta.categories",
                    FieldKind::ReadOnly,
                    self.data_dir()
                        .ok()
                        .and_then(|path| engine::meta::read_categories_json(path).ok())
                        .map(|values| values.join(", "))
                        .unwrap_or_default(),
                ),
                (
                    "meta.category_editing",
                    FieldKind::ReadOnly,
                    self.tr("tui.categoryEditingGuiOnly"),
                ),
            ],
            SettingSection::Ai => vec![
                (
                    "ai.endpoint.online.url",
                    FieldKind::Text,
                    ai.online.endpoint.url,
                ),
                (
                    "ai.endpoint.online.model",
                    FieldKind::Text,
                    ai.online.endpoint.model,
                ),
                (
                    "ai.endpoint.online.api_key",
                    FieldKind::ReadOnly,
                    if ai.online.endpoint.api_key_configured {
                        self.tr("tui.apiKeyConfigured")
                    } else {
                        self.tr("tui.apiKeyConfigureDesktop")
                    },
                ),
                (
                    "ai.endpoint.online.title_model",
                    FieldKind::Text,
                    ai.online.title_model.unwrap_or_default(),
                ),
                (
                    "ai.endpoint.online.image_model",
                    FieldKind::Text,
                    ai.online.image_model.unwrap_or_default(),
                ),
                (
                    "ai.endpoint.online.chat_supports_tools",
                    FieldKind::Bool,
                    ai.online.chat_supports_tools.unwrap_or(false).to_string(),
                ),
                (
                    "ai.endpoint.online.image_supports_vision",
                    FieldKind::Bool,
                    ai.online.image_supports_vision.unwrap_or(false).to_string(),
                ),
                (
                    "ai.endpoint.airplane.url",
                    FieldKind::Text,
                    ai.airplane.endpoint.url,
                ),
                (
                    "ai.endpoint.airplane.model",
                    FieldKind::Text,
                    ai.airplane.endpoint.model,
                ),
                (
                    "ai.endpoint.airplane.api_key",
                    FieldKind::ReadOnly,
                    if ai.airplane.endpoint.api_key_configured {
                        self.tr("tui.apiKeyConfigured")
                    } else {
                        self.tr("tui.apiKeyConfigureDesktop")
                    },
                ),
                (
                    "ai.endpoint.airplane.title_model",
                    FieldKind::Text,
                    ai.airplane.title_model.unwrap_or_default(),
                ),
                (
                    "ai.endpoint.airplane.image_model",
                    FieldKind::Text,
                    ai.airplane.image_model.unwrap_or_default(),
                ),
                (
                    "ai.endpoint.airplane.chat_supports_tools",
                    FieldKind::Bool,
                    ai.airplane.chat_supports_tools.unwrap_or(false).to_string(),
                ),
                (
                    "ai.endpoint.airplane.image_supports_vision",
                    FieldKind::Bool,
                    ai.airplane
                        .image_supports_vision
                        .unwrap_or(false)
                        .to_string(),
                ),
                ("ai.system_prompt", FieldKind::Text, ai.system_prompt),
            ],
            SettingSection::Technology => vec![
                ("technology.runtime", FieldKind::ReadOnly, "Lua".into()),
                (
                    "meta.semantic_similarity_enabled",
                    FieldKind::Bool,
                    metadata
                        .as_ref()
                        .map(|value| value.semantic_similarity_enabled.to_string())
                        .unwrap_or_else(|| "false".into()),
                ),
            ],
            SettingSection::Publishing => vec![
                (
                    "publishing.ssh_host",
                    FieldKind::Text,
                    publishing.ssh_host.unwrap_or_default(),
                ),
                (
                    "publishing.ssh_username",
                    FieldKind::Text,
                    publishing.ssh_user.unwrap_or_default(),
                ),
                (
                    "publishing.ssh_remote_path",
                    FieldKind::Text,
                    publishing.ssh_remote_path.unwrap_or_default(),
                ),
                (
                    "publishing.ssh_mode",
                    FieldKind::Enum(&["scp", "rsync"]),
                    match publishing.ssh_mode {
                        SshMode::Scp => "scp".into(),
                        SshMode::Rsync => "rsync".into(),
                    },
                ),
            ],
            SettingSection::Data => vec![
                (
                    "data.database",
                    FieldKind::ReadOnly,
                    self.host.database_path().display().to_string(),
                ),
                ("data.automatic_rebuild", FieldKind::Bool, "true".into()),
            ],
            SettingSection::Mcp => {
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                vec![
                    ("mcp.http.enabled", FieldKind::Bool, "false".into()),
                    (
                        "mcp.agent.claude_code",
                        FieldKind::Bool,
                        engine::mcp::is_agent_configured(engine::mcp::McpAgent::ClaudeCode, &home)
                            .to_string(),
                    ),
                    (
                        "mcp.agent.github_copilot",
                        FieldKind::Bool,
                        engine::mcp::is_agent_configured(
                            engine::mcp::McpAgent::GithubCopilot,
                            &home,
                        )
                        .to_string(),
                    ),
                    (
                        "mcp.proposals",
                        FieldKind::ReadOnly,
                        engine::mcp::list_pending_proposals(db.conn(), self.project_id()?)?
                            .len()
                            .to_string(),
                    ),
                ]
            }
        };
        specs
            .into_iter()
            .map(|(key, kind, fallback)| {
                let value = if kind == FieldKind::ReadOnly
                    || key.starts_with("project.")
                    || key.starts_with("meta.")
                    || key.starts_with("publishing.")
                    || key.starts_with("ai.endpoint.")
                    || key.starts_with("mcp.agent.")
                    || key == "ai.system_prompt"
                {
                    fallback
                } else {
                    engine::settings::get_effective(db.conn(), key)?.unwrap_or(fallback)
                };
                Ok(SettingField {
                    label: setting_field_label(self.locale, key),
                    key,
                    kind,
                    value,
                })
            })
            .collect()
    }

    fn edit_setting_field(&mut self) -> Result<()> {
        let Some(field) = self.settings_fields.get_mut(self.panel_index) else {
            return Ok(());
        };
        match &field.kind {
            FieldKind::ReadOnly => {
                self.status = bds_core::i18n::translate(self.locale, "tui.valueReadOnly")
            }
            FieldKind::Bool => {
                field.value = (!field.value.eq_ignore_ascii_case("true")).to_string()
            }
            FieldKind::Enum(values) => {
                let index = values
                    .iter()
                    .position(|value| *value == field.value)
                    .unwrap_or(0);
                field.value = values[(index + 1) % values.len()].to_string();
            }
            FieldKind::Text => {
                self.prompt = Some(Prompt {
                    kind: PromptKind::Setting(self.panel_index),
                    value: field.value.clone(),
                    candidates: Vec::new(),
                })
            }
        }
        Ok(())
    }

    fn save_settings(&mut self) -> Result<()> {
        let Panel::Settings(section) = self.panel else {
            return Ok(());
        };
        let db = self.database()?;
        match section {
            SettingSection::Project => {
                let name = self.setting_value("project.name").trim();
                if name.is_empty() {
                    bail!(self.tr("tui.projectNameRequired"));
                }
                let mut project = self
                    .project
                    .clone()
                    .ok_or_else(|| anyhow!(self.tr("tui.noActiveProject")))?;
                project.name = name.into();
                project.description = optional_string(self.setting_value("project.description"));
                project.updated_at = bds_core::util::now_unix_ms();
                let mut metadata = self.project_metadata()?;
                metadata.name = project.name.clone();
                metadata.description = project.description.clone();
                metadata.default_author =
                    optional_string(self.setting_value("meta.default_author"));
                metadata.main_language = optional_string(self.setting_value("meta.main_language"));
                metadata.public_url = optional_string(self.setting_value("meta.public_url"));
                metadata.blogmark_category =
                    optional_string(self.setting_value("meta.blogmark_category"));
                metadata.blog_languages = self
                    .setting_value("meta.blog_languages")
                    .split(',')
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToOwned::to_owned)
                    .collect();
                metadata.blog_languages.sort();
                metadata.blog_languages.dedup();
                metadata.max_posts_per_page = self
                    .setting_value("meta.max_posts_per_page")
                    .trim()
                    .parse()
                    .map_err(|_| anyhow!(self.tr("tui.postsPerPageInvalid")))?;
                metadata.image_import_concurrency = self
                    .setting_value("meta.image_import_concurrency")
                    .trim()
                    .parse()
                    .map_err(|_| anyhow!(self.tr("tui.imageImportConcurrencyInvalid")))?;
                metadata.validate().map_err(|error| anyhow!(error))?;
                bds_core::db::queries::project::update_project(db.conn(), &project)?;
                engine::meta::write_project_json(self.data_dir()?, &metadata)?;
                self.project = Some(project);
                if let Some(value) = self.setting_field(engine::settings::UI_LANGUAGE_KEY) {
                    engine::settings::set(db.conn(), value.key, &value.value)?;
                    self.locale = normalize_language(&value.value);
                }
            }
            SettingSection::Content => {}
            SettingSection::Ai => {
                self.save_ai_settings(&db)?;
            }
            SettingSection::Technology => {
                let mut metadata = self.project_metadata()?;
                metadata.semantic_similarity_enabled = self
                    .setting_value("meta.semantic_similarity_enabled")
                    .eq_ignore_ascii_case("true");
                engine::meta::write_project_json(self.data_dir()?, &metadata)?;
            }
            SettingSection::Publishing => {
                let preferences = PublishingPreferences {
                    ssh_host: optional_string(self.setting_value("publishing.ssh_host")),
                    ssh_user: optional_string(self.setting_value("publishing.ssh_username")),
                    ssh_remote_path: optional_string(
                        self.setting_value("publishing.ssh_remote_path"),
                    ),
                    ssh_mode: if self.setting_value("publishing.ssh_mode") == "rsync" {
                        SshMode::Rsync
                    } else {
                        SshMode::Scp
                    },
                };
                engine::meta::write_publishing_json(self.data_dir()?, &preferences)?;
            }
            SettingSection::Mcp => {
                engine::settings::set(
                    db.conn(),
                    "mcp.http.enabled",
                    self.setting_value("mcp.http.enabled"),
                )?;
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                for (agent, key) in [
                    (engine::mcp::McpAgent::ClaudeCode, "mcp.agent.claude_code"),
                    (
                        engine::mcp::McpAgent::GithubCopilot,
                        "mcp.agent.github_copilot",
                    ),
                ] {
                    let desired = self.setting_value(key).eq_ignore_ascii_case("true");
                    let configured = engine::mcp::is_agent_configured(agent, &home);
                    if desired && !configured {
                        let executable = engine::mcp::packaged_mcp_executable()?;
                        engine::mcp::install_agent_config(agent, &home, &executable)?;
                    } else if !desired && configured {
                        engine::mcp::remove_agent_config(agent, &home)?;
                    }
                }
            }
            _ => {
                for field in &self.settings_fields {
                    if field.kind != FieldKind::ReadOnly {
                        engine::settings::set(db.conn(), field.key, &field.value)?;
                    }
                }
            }
        }
        if let Some(value) = self
            .settings_fields
            .iter()
            .find(|field| field.key == "ai.airplane_mode")
        {
            self.airplane = value.value == "true";
        }
        if let Some(value) = self
            .settings_fields
            .iter()
            .find(|field| field.key == engine::settings::UI_LANGUAGE_KEY)
        {
            self.locale = normalize_language(&value.value);
        }
        let selected_key = self
            .settings_fields
            .get(self.panel_index)
            .map(|field| field.key);
        self.settings_fields = self.load_setting_fields(section)?;
        self.panel_index = selected_key
            .and_then(|key| {
                self.settings_fields
                    .iter()
                    .position(|field| field.key == key)
            })
            .filter(|index| self.settings_fields[*index].kind != FieldKind::ReadOnly)
            .or_else(|| {
                self.settings_fields
                    .iter()
                    .position(|field| field.kind != FieldKind::ReadOnly)
            })
            .unwrap_or(0);
        self.status = self.tr("tui.settingsSaved");
        Ok(())
    }

    fn setting_field(&self, key: &str) -> Option<&SettingField> {
        self.settings_fields.iter().find(|field| field.key == key)
    }

    fn setting_value(&self, key: &str) -> &str {
        self.setting_field(key)
            .map(|field| field.value.as_str())
            .unwrap_or("")
    }

    fn project_metadata(&self) -> Result<ProjectMetadata> {
        engine::meta::read_project_json(self.data_dir()?).or_else(|_| {
            let project = self
                .project
                .as_ref()
                .ok_or_else(|| anyhow!(self.tr("tui.noActiveProject")))?;
            Ok(ProjectMetadata {
                name: project.name.clone(),
                description: project.description.clone(),
                public_url: None,
                main_language: None,
                default_author: None,
                max_posts_per_page: 50,
                image_import_concurrency: 4,
                blogmark_category: None,
                pico_theme: None,
                semantic_similarity_enabled: false,
                blog_languages: Vec::new(),
            })
        })
    }

    fn save_ai_settings(&mut self, db: &Database) -> Result<()> {
        for (kind, url_key, model_key) in [
            (
                AiEndpointKind::Online,
                "ai.endpoint.online.url",
                "ai.endpoint.online.model",
            ),
            (
                AiEndpointKind::Airplane,
                "ai.endpoint.airplane.url",
                "ai.endpoint.airplane.model",
            ),
        ] {
            let url = self.setting_value(url_key).trim();
            let model = self.setting_value(model_key).trim();
            if !url.is_empty() || !model.is_empty() {
                engine::ai::save_endpoint(
                    db.conn(),
                    &AiEndpointConfig {
                        kind,
                        url: url.into(),
                        model: model.into(),
                        api_key: engine::ai::load_endpoint_api_key(kind)?,
                    },
                )?;
            }
            let prefix = format!("ai.endpoint.{}", kind.as_str());
            engine::ai::save_model_preferences(
                db.conn(),
                kind,
                optional_str(self.setting_value(&format!("{prefix}.title_model"))),
                optional_str(self.setting_value(&format!("{prefix}.image_model"))),
                Some(
                    self.setting_value(&format!("{prefix}.chat_supports_tools"))
                        .eq_ignore_ascii_case("true"),
                ),
                Some(
                    self.setting_value(&format!("{prefix}.image_supports_vision"))
                        .eq_ignore_ascii_case("true"),
                ),
            )?;
        }
        engine::ai::save_system_prompt(db.conn(), self.setting_value("ai.system_prompt"))?;
        Ok(())
    }

    fn tags(&self) -> Result<Vec<Tag>> {
        let db = self.database()?;
        let mut tags =
            bds_core::db::queries::tag::list_tags_by_project(db.conn(), self.project_id()?)?;
        tags.sort_by_key(|tag| tag.name.to_lowercase());
        Ok(tags)
    }

    fn begin_tag_rename(&mut self) -> Result<()> {
        if let Some(tag) = self.tags()?.get(self.panel_index) {
            self.prompt = Some(Prompt {
                kind: PromptKind::TagRename(tag.id.clone()),
                value: tag.name.clone(),
                candidates: Vec::new(),
            });
        }
        Ok(())
    }

    fn begin_tag_delete(&mut self) -> Result<()> {
        if let Some(tag) = self.tags()?.get(self.panel_index) {
            let db = self.database()?;
            let count =
                bds_core::db::queries::post::list_posts_by_project(db.conn(), self.project_id()?)?
                    .into_iter()
                    .filter(|post| {
                        post.tags
                            .iter()
                            .any(|name| name.eq_ignore_ascii_case(&tag.name))
                    })
                    .count();
            self.status = self.tr_with(
                "tui.deleteTagConfirm",
                &[
                    ("name", &tag.name),
                    ("count", &count.to_string()),
                    ("yes", &self.tr("tui.confirmYesInput")),
                ],
            );
            self.prompt = Some(Prompt {
                kind: PromptKind::ConfirmDeleteTag(tag.id.clone()),
                value: String::new(),
                candidates: Vec::new(),
            });
        }
        Ok(())
    }

    fn toggle_tag_mark(&mut self) {
        if let Ok(tags) = self.tags()
            && let Some(tag) = tags.get(self.panel_index)
            && !self.marked_tags.remove(&tag.id)
        {
            self.marked_tags.insert(tag.id.clone());
        }
    }

    fn merge_marked_tags(&mut self) -> Result<()> {
        let tags = self.tags()?;
        let Some(target) = tags.get(self.panel_index) else {
            return Ok(());
        };
        if !self.marked_tags.contains(&target.id) || self.marked_tags.len() < 2 {
            self.status = self.tr("tui.markTagsRequired");
            return Ok(());
        }
        let sources = self
            .marked_tags
            .iter()
            .filter(|id| *id != &target.id)
            .map(String::as_str)
            .collect::<Vec<_>>();
        let db = self.database()?;
        engine::tag::merge_tags(
            db.conn(),
            self.data_dir()?,
            self.project_id()?,
            &sources,
            &target.id,
        )?;
        self.marked_tags.clear();
        self.status = self.tr("tui.tagsMerged");
        Ok(())
    }

    fn cycle_tag_color(&mut self) -> Result<()> {
        let Some(tag) = self.tags()?.get(self.panel_index).cloned() else {
            return Ok(());
        };
        let index = COLORS
            .iter()
            .position(|color| Some(*color) == tag.color.as_deref())
            .unwrap_or(0);
        let db = self.database()?;
        engine::tag::update_tag(
            db.conn(),
            self.data_dir()?,
            &tag.id,
            None,
            Some(COLORS[(index + 1) % COLORS.len()]),
            None,
        )?;
        self.status = self.tr("tui.tagColourUpdated");
        Ok(())
    }

    fn cycle_tag_template(&mut self) -> Result<()> {
        let Some(tag) = self.tags()?.get(self.panel_index).cloned() else {
            return Ok(());
        };
        let db = self.database()?;
        let mut templates = bds_core::db::queries::template::list_templates_by_project(
            db.conn(),
            self.project_id()?,
        )?
        .into_iter()
        .filter(|template| template.kind == TemplateKind::Post)
        .map(|template| template.slug)
        .collect::<Vec<_>>();
        templates.insert(0, String::new());
        let index = templates
            .iter()
            .position(|slug| Some(slug.as_str()) == tag.post_template_slug.as_deref())
            .unwrap_or(0);
        engine::tag::update_tag(
            db.conn(),
            self.data_dir()?,
            &tag.id,
            None,
            None,
            Some(&templates[(index + 1) % templates.len()]),
        )?;
        self.status = self.tr("tui.tagTemplateUpdated");
        Ok(())
    }

    fn sync_tags(&mut self) -> Result<()> {
        let db = self.database()?;
        let tags = engine::tag::sync_tags_from_posts(db.conn(), self.project_id()?)?;
        engine::tag::rewrite_tags_json(db.conn(), self.data_dir()?, self.project_id()?)?;
        self.status = self.tr_with(
            "tui.tagsSynchronized",
            &[("count", &tags.len().to_string())],
        );
        Ok(())
    }
}

fn field_specs(
    values: &[(&'static str, FieldKind, &'static str)],
) -> Vec<(&'static str, FieldKind, String)> {
    values
        .iter()
        .map(|(key, kind, value)| (*key, kind.clone(), (*value).into()))
        .collect()
}

fn setting_field_label(locale: UiLocale, key: &str) -> String {
    let translation_key = match key {
        "project.name" => "settings.projectName",
        "project.description" => "settings.projectDescription",
        "project.data_path" => "settings.dataPath",
        engine::settings::UI_LANGUAGE_KEY => "tui.settingUiLanguage",
        "editor.default_mode" => "settings.defaultMode",
        "editor.diff_view_style" => "settings.diffViewStyle",
        "editor.wrap_long_lines" => "settings.wrapLongLines",
        "editor.hide_unchanged_regions" => "settings.hideUnchangedRegions",
        "meta.default_author" => "settings.defaultAuthor",
        "meta.max_posts_per_page" => "settings.maxPostsPerPage",
        "meta.main_language" => "settings.mainLanguage",
        "meta.blog_languages" => "settings.blogLanguages",
        "meta.public_url" => "settings.publicUrl",
        "meta.image_import_concurrency" => "settings.imageImportConcurrency",
        "meta.blogmark_category" => "settings.blogmarkCategory",
        "meta.categories" => "editor.categories",
        "meta.category_editing" => "tui.categoryEditing",
        "ai.endpoint.online.url" => "settings.onlineEndpointUrl",
        "ai.endpoint.online.model" => "settings.onlineChatModel",
        "ai.endpoint.online.api_key" => "settings.onlineApiKey",
        "ai.endpoint.online.title_model" => "settings.onlineTitleModel",
        "ai.endpoint.online.image_model" => "settings.onlineImageModel",
        "ai.endpoint.online.chat_supports_tools" => "settings.onlineModelSupportsTools",
        "ai.endpoint.online.image_supports_vision" => "settings.onlineModelSupportsVision",
        "ai.endpoint.airplane.url" => "settings.airplaneEndpointUrl",
        "ai.endpoint.airplane.model" => "settings.airplaneChatModel",
        "ai.endpoint.airplane.api_key" => "settings.airplaneApiKey",
        "ai.endpoint.airplane.title_model" => "settings.airplaneTitleModel",
        "ai.endpoint.airplane.image_model" => "settings.airplaneImageModel",
        "ai.endpoint.airplane.chat_supports_tools" => "settings.airplaneModelSupportsTools",
        "ai.endpoint.airplane.image_supports_vision" => "settings.airplaneModelSupportsVision",
        "ai.system_prompt" => "settings.systemPrompt",
        "technology.runtime" => "tui.settingRuntime",
        "meta.semantic_similarity_enabled" => "settings.semanticSimilarityEnabled",
        "publishing.ssh_host" => "settings.sshHost",
        "publishing.ssh_username" => "settings.sshUsername",
        "publishing.ssh_remote_path" => "settings.sshRemotePath",
        "publishing.ssh_mode" => "tui.settingTransferMode",
        "data.database" => "tui.settingDatabase",
        "data.automatic_rebuild" => "tui.settingAutomaticRebuild",
        "mcp.http.enabled" => "settings.mcpEnable",
        "mcp.agent.claude_code" => "tui.settingClaudeCode",
        "mcp.agent.github_copilot" => "tui.settingGithubCopilot",
        "mcp.proposals" => "settings.mcpProposals",
        _ => return key.into(),
    };
    bds_core::i18n::translate(locale, translation_key)
}

fn optional_string(value: &str) -> Option<String> {
    optional_str(value).map(ToOwned::to_owned)
}

fn optional_str(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn visual_line_metrics(buffer: &EditorBuffer, width: usize) -> (usize, usize) {
    let width = width.max(1);
    let (cursor_line, cursor_col) = buffer.cursor();
    let mut visual_cursor = 0;
    let mut visual_total = 0;
    for index in 0..buffer.line_count() {
        let line_len = buffer
            .line(index)
            .map(|line| line.chars().filter(|character| *character != '\n').count())
            .unwrap_or(0);
        if index < cursor_line {
            visual_cursor += line_len.max(1).div_ceil(width);
        } else if index == cursor_line {
            visual_cursor += cursor_col / width;
        }
        visual_total += line_len.max(1).div_ceil(width);
    }
    (visual_cursor, visual_total.max(1))
}

impl TuiApp {
    fn git_items(&self) -> Result<Vec<SidebarItem>> {
        let Some(data_dir) = &self.data_dir else {
            return Ok(vec![SidebarItem::Empty(self.tr("tui.noActiveProject"))]);
        };
        let git = engine::git::GitEngine::new(data_dir);
        let repository = git.repository()?;
        if !repository.is_initialized {
            return Ok(vec![SidebarItem::Empty(self.tr("git.notRepository"))]);
        }
        let remote = git.remote_state()?;
        let branch = remote
            .local_branch
            .clone()
            .unwrap_or_else(|| self.tr("tui.detachedHead"));
        let mut items = vec![SidebarItem::Header(format!(
            "{branch} ↑{} ↓{}",
            remote.ahead, remote.behind
        ))];
        items.extend(git.status()?.into_iter().map(|file| {
            SidebarItem::GitFile(
                file.path.clone(),
                format!("{} {}", file.kind.code(), file.path),
            )
        }));
        items.push(SidebarItem::Header(self.tr("git.history")));
        items.extend(git.history(&branch)?.into_iter().map(|commit| {
            let marker = match commit.sync_status {
                engine::git::SyncStatus::LocalOnly => "↑",
                engine::git::SyncStatus::RemoteOnly => "↓",
                engine::git::SyncStatus::Both => " ",
            };
            let short = commit.hash.chars().take(8).collect::<String>();
            SidebarItem::GitCommit(
                commit.hash,
                format!("{marker} {short} {}", commit.subject.unwrap_or_default()),
            )
        }));
        if !items.iter().any(SidebarItem::selectable) {
            items.push(SidebarItem::Empty(self.tr("tui.workingTreeClean")));
        }
        Ok(items)
    }

    fn open_git_file(&mut self, path: &str) -> Result<()> {
        let patch = engine::git::GitEngine::new(self.data_dir()?)
            .file_diff(path)?
            .patch;
        self.output = vec![truncate_bytes(&patch, MAX_DIFF_BYTES, self.locale)];
        self.panel = Panel::Git;
        self.scroll = 0;
        Ok(())
    }

    fn open_git_commit(&mut self, hash: &str) -> Result<()> {
        let diff = engine::git::GitEngine::new(self.data_dir()?).commit_diff(hash)?;
        self.output = vec![truncate_bytes(&diff, MAX_DIFF_BYTES, self.locale)];
        self.panel = Panel::Git;
        self.scroll = 0;
        Ok(())
    }

    fn git_pull(&mut self) -> Result<()> {
        if !self.require_git()? {
            return Ok(());
        }
        if self.airplane {
            self.status = self.tr("tui.airplaneGitPullBlocked");
            return Ok(());
        }
        let data_dir = self.data_dir()?.to_owned();
        let locale = self.locale;
        let label = self.tr("tui.taskGitPull");
        self.queue_task(&label, move || {
            let mut output = Vec::new();
            let result = engine::git::GitEngine::new(&data_dir)
                .pull(|| false, |line| output.push(line.text))?;
            output.push(result.output);
            Ok(BackgroundResult {
                status: bds_core::i18n::translate_with(
                    locale,
                    "tui.gitPullComplete",
                    &[("output", &output.join("\n"))],
                ),
                panel: Some(Panel::Git),
                reload: true,
            })
        });
        Ok(())
    }

    fn git_push(&mut self) -> Result<()> {
        if !self.require_git()? {
            return Ok(());
        }
        if self.airplane {
            self.status = self.tr("tui.airplaneGitPushBlocked");
            return Ok(());
        }
        let data_dir = self.data_dir()?.to_owned();
        let locale = self.locale;
        let label = self.tr("tui.taskGitPush");
        self.queue_task(&label, move || {
            let mut output = Vec::new();
            let result = engine::git::GitEngine::new(&data_dir)
                .push(|| false, |line| output.push(line.text))?;
            output.push(result.output);
            Ok(BackgroundResult {
                status: bds_core::i18n::translate_with(
                    locale,
                    "tui.gitPushComplete",
                    &[("output", &output.join("\n"))],
                ),
                panel: Some(Panel::Git),
                reload: true,
            })
        });
        Ok(())
    }

    fn require_git(&mut self) -> Result<bool> {
        let ready = engine::git::GitEngine::new(self.data_dir()?)
            .repository()?
            .is_initialized;
        if !ready {
            self.status = self.tr("git.notRepository");
        }
        Ok(ready)
    }

    fn run_command(&mut self, command: &str) -> Result<()> {
        let command = if command.is_empty() {
            COMMANDS.first().map(|command| command.id).unwrap_or("")
        } else if command == "?" {
            command
        } else {
            self.command_id(command).unwrap_or(command)
        };
        if command == "?" {
            self.panel = Panel::Help;
            self.focus = Focus::Editor;
            return Ok(());
        }
        let project_id = self.project_id()?.to_owned();
        let data_dir = self.data_dir()?.to_owned();
        let database_path = self.host.database_path().to_owned();
        match command {
            "metadata-diff" => {
                let locale = self.locale;
                let label = self.tr("tui.commandMetadataDiff");
                self.queue_task(&label, move || {
                    let db = Database::open(&database_path)?;
                    Ok(BackgroundResult {
                        status: bds_core::i18n::translate(locale, "tui.metadataDiffComplete"),
                        panel: Some(Panel::Report {
                            title: bds_core::i18n::translate(locale, "menu.item.metadataDiff"),
                            body: metadata_report_body(db.conn(), &data_dir, &project_id, locale)?,
                            action: ReportAction::MetadataDiff,
                        }),
                        reload: false,
                    })
                });
            }
            "validate-site" => {
                let locale = self.locale;
                let label = self.tr("tui.commandValidateSite");
                self.queue_task(&label, move || {
                    let db = Database::open(&database_path)?;
                    Ok(BackgroundResult {
                        status: bds_core::i18n::translate(locale, "tui.siteValidationComplete"),
                        panel: Some(Panel::Report {
                            title: bds_core::i18n::translate(locale, "menu.item.validateSite"),
                            body: site_validation_body(db.conn(), &data_dir, &project_id, locale)?,
                            action: ReportAction::SiteValidation,
                        }),
                        reload: false,
                    })
                });
            }
            "force-render" => {
                let locale = self.locale;
                let label = self.tr("tui.commandForceRender");
                self.queue_task(&label, move || {
                    let db = Database::open(&database_path)?;
                    let metadata = engine::meta::read_project_json(&data_dir)?;
                    let posts = published_sources(db.conn(), &data_dir, &project_id)?;
                    let report = engine::generation::generate_starter_site(
                        db.conn(),
                        &data_dir.join("html"),
                        &project_id,
                        &metadata,
                        &posts,
                        metadata.main_language.as_deref().unwrap_or("en"),
                    )?;
                    Ok(BackgroundResult {
                        status: bds_core::i18n::translate_with(
                            locale,
                            "tui.filesRendered",
                            &[("count", &report.written_paths.len().to_string())],
                        ),
                        panel: None,
                        reload: false,
                    })
                });
            }
            "rebuild-database" => {
                let locale = self.locale;
                let label = self.tr("tui.commandRebuildDatabase");
                self.queue_task(&label, move || {
                    let db = Database::open(&database_path)?;
                    let report = engine::rebuild::rebuild_from_filesystem(
                        db.conn(),
                        &data_dir,
                        &project_id,
                    )?;
                    let count = report.posts_created
                        + report.posts_updated
                        + report.media_created
                        + report.media_updated
                        + report.templates_created
                        + report.templates_updated
                        + report.scripts_created
                        + report.scripts_updated;
                    Ok(BackgroundResult {
                        status: bds_core::i18n::translate_with(
                            locale,
                            "tui.itemsRebuilt",
                            &[("count", &count.to_string())],
                        ),
                        panel: None,
                        reload: true,
                    })
                });
            }
            "reindex-search" => {
                let locale = self.locale;
                let label = self.tr("tui.commandReindexSearch");
                self.queue_task(&label, move || {
                    let db = Database::open(&database_path)?;
                    let report = engine::search::reindex_project(db.conn(), &project_id, None)?;
                    Ok(BackgroundResult {
                        status: bds_core::i18n::translate_with(
                            locale,
                            "tui.postsReindexed",
                            &[("count", &report.posts_indexed.to_string())],
                        ),
                        panel: None,
                        reload: false,
                    })
                });
            }
            "validate-translations-gui" | "find-duplicates-gui" => {
                self.status = self.tr("tui.commandDesktopOnly")
            }
            "upload-site" if self.airplane => self.status = self.tr("tui.airplaneUploadBlocked"),
            "upload-site" => self.status = self.tr("tui.uploadRequiresPublishingSettings"),
            "browser-preview-url" => {
                self.status = format!("file://{}", data_dir.join("html/index.html").display())
            }
            _ => self.status = self.tr_with("tui.unknownCommand", &[("command", command)]),
        }
        Ok(())
    }

    fn apply_report(&mut self, action: ReportAction) -> Result<()> {
        let db = self.database()?;
        let project_id = self.project_id()?.to_owned();
        let data_dir = self.data_dir()?.to_owned();
        match action {
            ReportAction::MetadataDiff => {
                let report = engine::metadata_diff::compute_metadata_diff(
                    db.conn(),
                    &data_dir,
                    &project_id,
                )?;
                for item in &report.diffs {
                    engine::metadata_diff::repair_metadata_diff_item(
                        db.conn(),
                        &data_dir,
                        &project_id,
                        engine::metadata_diff::RepairDirection::FileToDatabase,
                        item,
                    )?;
                }
                for orphan in &report.orphans {
                    if orphan.reason == "file_without_db_entry" {
                        engine::metadata_diff::import_orphan_file(
                            db.conn(),
                            &data_dir,
                            &project_id,
                            orphan,
                        )?;
                    }
                }
                self.status = self.tr_with(
                    "tui.metadataChangesApplied",
                    &[(
                        "count",
                        &(report.diffs.len() + report.orphans.len()).to_string(),
                    )],
                );
                self.reload()?;
            }
            ReportAction::SiteValidation => {
                let validation =
                    engine::validate_site::validate_site(db.conn(), &data_dir, &project_id)?;
                let metadata = engine::meta::read_project_json(&data_dir)?;
                let posts = published_sources(db.conn(), &data_dir, &project_id)?;
                let sections = engine::generation::sections_from_validation_report(&validation);
                let report = engine::generation::apply_validation_sections(
                    db.conn(),
                    &data_dir.join("html"),
                    &project_id,
                    &metadata,
                    &posts,
                    &sections,
                )?;
                self.status = self.tr_with(
                    "tui.validationApplied",
                    &[
                        ("written", &report.written_paths.len().to_string()),
                        ("removed", &report.deleted_paths.len().to_string()),
                    ],
                );
            }
        }
        self.panel = Panel::Welcome;
        self.focus = Focus::Sidebar;
        Ok(())
    }
}

fn truncate_bytes(value: &str, limit: usize, locale: UiLocale) -> String {
    if value.len() <= limit {
        return value.into();
    }
    let mut end = limit;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!(
        "{}\n\n{}",
        &value[..end],
        bds_core::i18n::translate(locale, "tui.diffTruncated")
    )
}

fn published_sources(
    conn: &bds_core::db::DbConnection,
    data_dir: &Path,
    project_id: &str,
) -> Result<Vec<engine::generation::PublishedPostSource>> {
    let mut sources = Vec::new();
    for post in bds_core::db::queries::post::list_posts_by_project(conn, project_id)? {
        if let Some(source) = engine::generation::load_published_post_source(data_dir, post)? {
            sources.push(source);
        }
    }
    Ok(sources)
}

fn metadata_report_body(
    conn: &bds_core::db::DbConnection,
    data_dir: &Path,
    project_id: &str,
    locale: UiLocale,
) -> Result<String> {
    let report = engine::metadata_diff::compute_metadata_diff(conn, data_dir, project_id)?;
    let mut lines = report
        .diffs
        .iter()
        .map(|item| {
            bds_core::i18n::translate_with(
                locale,
                "tui.metadataDiffFields",
                &[
                    ("entity", &item.entity_type),
                    ("path", &item.file_path),
                    ("count", &item.fields.len().to_string()),
                ],
            )
        })
        .collect::<Vec<_>>();
    lines.extend(report.orphans.iter().map(|item| {
        bds_core::i18n::translate_with(
            locale,
            "tui.metadataOrphan",
            &[("path", &item.file_path), ("reason", &item.reason)],
        )
    }));
    lines.extend(report.errors.iter().map(|error| {
        bds_core::i18n::translate_with(locale, "tui.metadataError", &[("error", error)])
    }));
    if lines.is_empty() {
        lines.push(bds_core::i18n::translate(
            locale,
            "tui.noMetadataDifferences",
        ));
    }
    Ok(lines.join("\n"))
}

fn site_validation_body(
    conn: &bds_core::db::DbConnection,
    data_dir: &Path,
    project_id: &str,
    locale: UiLocale,
) -> Result<String> {
    let report = engine::validate_site::validate_site(conn, data_dir, project_id)?;
    Ok(bds_core::i18n::translate_with(
        locale,
        "tui.siteValidationReport",
        &[
            ("missing", &report.missing_pages.join("\n")),
            ("extra", &report.extra_pages.join("\n")),
            ("stale", &report.stale_pages.join("\n")),
        ],
    ))
}

impl TuiApp {
    pub fn render(&mut self, frame: &mut Frame<'_>) {
        let area = frame.area();
        self.resize(area.width, area.height);
        self.render_buffer(area, frame.buffer_mut());
    }

    pub fn render_buffer(&self, area: Rect, buffer: &mut Buffer) {
        let background = Style::default()
            .bg(Color::Rgb(24, 26, 32))
            .fg(Color::Rgb(210, 214, 224));
        buffer.set_style(area, background);
        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(4),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(area);
        let body = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(SIDEBAR_WIDTH.min(area.width.saturating_sub(8))),
                Constraint::Min(8),
            ])
            .split(vertical[0]);
        self.render_sidebar(body[0], buffer);
        self.render_main(body[1], buffer);
        let output = self
            .output
            .last()
            .map(|value| value.lines().last().unwrap_or(""))
            .unwrap_or("");
        Paragraph::new(output)
            .style(
                Style::default()
                    .bg(Color::Rgb(35, 38, 46))
                    .fg(Color::Rgb(153, 162, 184)),
            )
            .render(vertical[1], buffer);
        self.render_status(vertical[2], buffer);
        if let Some(prompt) = &self.prompt {
            self.render_prompt_overlay(area, prompt, buffer);
        }
        if let Some(overlay) = &self.overlay {
            self.render_overlay(area, overlay, buffer);
        }
    }

    fn render_sidebar(&self, area: Rect, buffer: &mut Buffer) {
        let filter = self
            .filters
            .get(&self.view)
            .filter(|value| !value.is_empty())
            .map(|value| format!(" /{value}"))
            .unwrap_or_default();
        let title = format!(" {}{} ", self.view_title(), filter);
        let inner = Block::default()
            .title(title)
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(Color::Rgb(65, 70, 83)))
            .inner(area);
        Block::default()
            .title(format!(" {}{} ", self.view_title(), filter))
            .borders(Borders::RIGHT)
            .border_style(Style::default().fg(Color::Rgb(65, 70, 83)))
            .render(area, buffer);
        let rows = self
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                let style = if index == self.selected_index && self.focus == Focus::Sidebar {
                    Style::default()
                        .bg(Color::Rgb(54, 61, 78))
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else if matches!(item, SidebarItem::Header(_)) {
                    Style::default()
                        .fg(Color::Rgb(126, 137, 160))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(196, 201, 214))
                };
                let prefix = if matches!(item, SidebarItem::Header(_)) {
                    ""
                } else {
                    "  "
                };
                Line::styled(format!("{prefix}{}", item.label(self.locale)), style)
            })
            .collect::<Vec<_>>();
        Paragraph::new(rows).render(inner, buffer);
    }

    fn render_main(&self, area: Rect, buffer: &mut Buffer) {
        if let Some(editor) = &self.editor {
            self.render_editor(area, editor, buffer);
            return;
        }
        if let Panel::MediaPreview { title, path } = &self.panel
            && let Some(protocol) = &self.image_protocol
        {
            let block = Block::default()
                .title(format!(" {title} — {} ", path.display()))
                .padding(ratatui::widgets::Padding::new(1, 1, 1, 1));
            let inner = block.inner(area);
            block.render(area, buffer);
            TerminalImage::new(protocol).render(inner, buffer);
            return;
        }
        let (title, text) = match &self.panel {
            Panel::Welcome => (
                self.project
                    .as_ref()
                    .map(|project| project.name.clone())
                    .unwrap_or_else(|| {
                        self.tr(if self.remote {
                            "remoteTerminal.serverTitle"
                        } else {
                            "remoteTerminal.localTitle"
                        })
                    }),
                self.tr("tui.help"),
            ),
            Panel::MediaPreview { title, path } => (
                title.clone(),
                self.tr_with(
                    "tui.imagePreview",
                    &[
                        ("path", &path.display().to_string()),
                        (
                            "dimensions",
                            &image_dimensions(path, self.locale)
                                .unwrap_or_else(|| self.tr("modal.postGallery.unavailable")),
                        ),
                    ],
                ),
            ),
            Panel::Settings(section) => (self.tr(section.key()), self.settings_text()),
            Panel::Tags(section) => (self.tr(section.key()), self.tags_text(*section)),
            Panel::Git => (
                self.tr("git.diff"),
                self.output
                    .last()
                    .cloned()
                    .unwrap_or_else(|| self.git_diff_text()),
            ),
            Panel::Report { title, body, .. } => (
                title.clone(),
                format!("{body}\n\n{}", self.tr("tui.applyCancel")),
            ),
            Panel::Help => (
                self.tr("tui.commandsTitle"),
                self.command_names().join("\n"),
            ),
        };
        Paragraph::new(text)
            .block(
                Block::default()
                    .title(format!(" {title} "))
                    .borders(Borders::NONE)
                    .padding(ratatui::widgets::Padding::new(2, 2, 1, 1)),
            )
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0))
            .render(area, buffer);
    }

    fn render_editor(&self, area: Rect, editor: &Editor, buffer: &mut Buffer) {
        let dirty = if editor.buffer.is_dirty() { " ●" } else { "" };
        let language = editor.post_language.as_deref().unwrap_or(editor.syntax);
        let mode = if editor.mode == EditorMode::Preview {
            self.tr("editor.modePreview")
        } else {
            language.to_owned()
        };
        let title = format!(" {} — {}{} ", editor.title, mode, dirty);
        let inner = Block::default()
            .title(title.clone())
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::Rgb(65, 70, 83)))
            .inner(area);
        Block::default()
            .title(title)
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(Color::Rgb(65, 70, 83)))
            .render(area, buffer);
        if editor.mode == EditorMode::Preview {
            let source = editor.buffer.text();
            Paragraph::new(tui_markdown::from_str(&source))
                .wrap(Wrap { trim: false })
                .scroll((editor.buffer.scroll_offset() as u16, 0))
                .render(inner, buffer);
            return;
        }
        let source = editor.buffer.text();
        let highlighter = bds_editor::highlighter();
        let extension = match editor.syntax {
            "markdown" => "md",
            other => other,
        };
        let syntax = highlighter.syntax_for_extension(extension);
        let cursor_line = editor.buffer.cursor().0;
        let lines = highlighter
            .highlight_lines(&source, syntax)
            .into_iter()
            .enumerate()
            .map(|(index, spans)| {
                let row_style = if index == cursor_line {
                    Style::default().bg(Color::Rgb(39, 43, 53))
                } else {
                    Style::default()
                };
                Line::from(
                    spans
                        .into_iter()
                        .map(|(style, value)| {
                            Span::styled(
                                value.trim_end_matches('\n').to_owned(),
                                Style::default()
                                    .fg(Color::Rgb(
                                        style.foreground.r,
                                        style.foreground.g,
                                        style.foreground.b,
                                    ))
                                    .bg(row_style.bg.unwrap_or(Color::Reset)),
                            )
                        })
                        .collect::<Vec<_>>(),
                )
                .style(row_style)
            })
            .collect::<Vec<_>>();
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .scroll((editor.buffer.scroll_offset() as u16, 0))
            .render(inner, buffer);
    }

    fn render_status(&self, area: Rect, buffer: &mut Buffer) {
        let transport = bds_core::i18n::translate_with(
            self.locale,
            "tui.status",
            &[
                (
                    "transport",
                    &self.tr(if self.remote {
                        "tui.remote"
                    } else {
                        "tui.local"
                    }),
                ),
                ("locale", self.locale.code()),
            ],
        );
        let text = if let Some(prompt) = &self.prompt {
            let marker = match prompt.kind {
                PromptKind::Search => "/".into(),
                PromptKind::Command => ":".into(),
                PromptKind::ProjectPath => self.tr("tui.promptOpen"),
                PromptKind::Commit => self.tr("tui.promptCommit"),
                PromptKind::ConfirmDeleteTag(_) => self.tr("tui.promptConfirm"),
                _ => self.tr("tui.promptValue"),
            };
            if matches!(prompt.kind, PromptKind::Search | PromptKind::Command) {
                format!("{marker}{}", prompt.value)
            } else {
                format!("{marker} {}", prompt.value)
            }
        } else if !self.status.is_empty() {
            format!("{transport} · {}", self.status)
        } else {
            transport
        };
        Paragraph::new(text)
            .style(
                Style::default()
                    .bg(Color::Rgb(47, 52, 64))
                    .fg(Color::Rgb(230, 233, 240)),
            )
            .render(area, buffer);
    }

    fn render_overlay(&self, area: Rect, overlay: &Overlay, buffer: &mut Buffer) {
        let popup = centered(
            area,
            64.min(area.width.saturating_sub(2)),
            16.min(area.height.saturating_sub(2)),
        );
        Clear.render(popup, buffer);
        let (title, lines) = match overlay {
            Overlay::ConfirmDiscard => (
                self.tr("tui.unsavedTitle"),
                vec![Line::from(self.tr("tui.unsavedPrompt"))],
            ),
            Overlay::Projects { selected } => {
                let projects = self
                    .database()
                    .and_then(|db| engine::project::list_projects(db.conn()).map_err(Into::into))
                    .unwrap_or_default();
                (
                    self.tr("remoteTerminal.availableProjects"),
                    projects
                        .into_iter()
                        .enumerate()
                        .map(|(index, project)| {
                            Line::styled(
                                format!(
                                    "{} {}",
                                    if project.is_active { "●" } else { " " },
                                    project.name
                                ),
                                if index == *selected {
                                    Style::default()
                                        .bg(Color::Rgb(54, 61, 78))
                                        .add_modifier(Modifier::BOLD)
                                } else {
                                    Style::default()
                                },
                            )
                        })
                        .collect(),
                )
            }
        };
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .style(Style::default().bg(Color::Rgb(31, 34, 42))),
            )
            .render(popup, buffer);
    }

    fn render_prompt_overlay(&self, area: Rect, prompt: &Prompt, buffer: &mut Buffer) {
        if !matches!(prompt.kind, PromptKind::Command | PromptKind::ProjectPath) {
            return;
        }
        let candidates = if prompt.kind == PromptKind::Command && prompt.value == "?" {
            self.command_names()
        } else {
            prompt.candidates.clone()
        };
        if candidates.is_empty() && prompt.kind == PromptKind::ProjectPath {
            return;
        }
        let height = (candidates.len() as u16 + 2).clamp(3, area.height.saturating_sub(3).max(3));
        let popup = Rect::new(
            area.x + 2,
            area.y + area.height.saturating_sub(height + 2),
            area.width.saturating_sub(4),
            height,
        );
        Clear.render(popup, buffer);
        let rows = candidates
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                Line::styled(
                    value,
                    if index == 0 {
                        Style::default()
                            .bg(Color::Rgb(54, 61, 78))
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                )
            })
            .collect::<Vec<_>>();
        Paragraph::new(rows)
            .block(
                Block::default()
                    .title(if prompt.kind == PromptKind::Command {
                        self.tr("tui.commandsTitle")
                    } else {
                        self.tr("tui.foldersTitle")
                    })
                    .borders(Borders::ALL)
                    .style(Style::default().bg(Color::Rgb(31, 34, 42))),
            )
            .render(popup, buffer);
    }

    fn view_title(&self) -> String {
        self.tr(match self.view {
            TuiView::Posts => "tui.viewPosts",
            TuiView::Media => "tui.viewMedia",
            TuiView::Templates => "tui.viewTemplates",
            TuiView::Scripts => "tui.viewScripts",
            TuiView::Tags => "tui.viewTags",
            TuiView::Settings => "tui.viewSettings",
            TuiView::Git => "tui.viewGit",
        })
    }

    fn settings_text(&self) -> String {
        let fields = self
            .settings_fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                format!(
                    "{} {:<22} {}{}",
                    if index == self.panel_index {
                        "›"
                    } else {
                        " "
                    },
                    field.label,
                    field.value,
                    if field.kind == FieldKind::ReadOnly {
                        self.tr("tui.readOnlyMarker")
                    } else {
                        String::new()
                    }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("{fields}\n\n{}", self.tr("tui.settingsHint"))
    }

    fn tags_text(&self, section: TagSection) -> String {
        let Ok(mut tags) = self.tags() else {
            return self.tr("tui.tagsLoadFailed");
        };
        let usage = self.tag_usage();
        if section == TagSection::Cloud {
            tags.sort_by_key(|tag| {
                std::cmp::Reverse(*usage.get(&tag.name.to_lowercase()).unwrap_or(&0))
            });
        }
        let mut rows = tags
            .iter()
            .enumerate()
            .map(|(index, tag)| {
                format!(
                    "{}{} {:<24} {:>4}  {}  {}",
                    if index == self.panel_index {
                        "›"
                    } else {
                        " "
                    },
                    if self.marked_tags.contains(&tag.id) {
                        "●"
                    } else {
                        " "
                    },
                    tag.name,
                    usage.get(&tag.name.to_lowercase()).unwrap_or(&0),
                    tag.color.as_deref().unwrap_or("—"),
                    tag.post_template_slug.as_deref().unwrap_or("—")
                )
            })
            .collect::<Vec<_>>();
        if rows.is_empty() {
            rows.push(self.tr("tui.noTags"));
        }
        rows.push(format!(
            "\n{}",
            self.tr(match section {
                TagSection::Cloud => "tui.tagCloudHint",
                TagSection::Manage => "tui.tagManageHint",
                TagSection::Merge => "tui.tagMergeHint",
            })
        ));
        rows.join("\n")
    }

    fn tag_usage(&self) -> HashMap<String, usize> {
        let Ok(db) = self.database() else {
            return HashMap::new();
        };
        let Ok(project_id) = self.project_id() else {
            return HashMap::new();
        };
        let Ok(posts) = bds_core::db::queries::post::list_posts_by_project(db.conn(), project_id)
        else {
            return HashMap::new();
        };
        let mut usage = HashMap::new();
        for tag in posts.into_iter().flat_map(|post| post.tags) {
            *usage.entry(tag.to_lowercase()).or_default() += 1;
        }
        usage
    }

    fn git_diff_text(&self) -> String {
        let Ok(data_dir) = self.data_dir() else {
            return self.tr("tui.noActiveProject");
        };
        match engine::git::GitEngine::new(data_dir).diff() {
            Ok(diff) => truncate_bytes(
                &self.tr_with(
                    "tui.gitDiffBody",
                    &[("staged", &diff.staged), ("unstaged", &diff.unstaged)],
                ),
                MAX_DIFF_BYTES,
                self.locale,
            ),
            Err(error) => error.to_string(),
        }
    }

    fn tr(&self, key: &str) -> String {
        bds_core::i18n::translate(self.locale, key)
    }

    fn tr_with(&self, key: &str, params: &[(&str, &str)]) -> String {
        bds_core::i18n::translate_with(self.locale, key, params)
    }

    fn command_names(&self) -> Vec<String> {
        COMMANDS
            .iter()
            .map(|command| self.tr(command.key))
            .collect()
    }

    fn command_id(&self, name: &str) -> Option<&'static str> {
        COMMANDS
            .iter()
            .find(|command| command.id == name || self.tr(command.key) == name)
            .map(|command| command.id)
    }
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn image_dimensions(path: &Path, locale: UiLocale) -> Option<String> {
    let data = fs::read(path).ok()?;
    if data.starts_with(b"\x89PNG") && data.len() >= 24 {
        return Some(bds_core::i18n::translate_with(
            locale,
            "tui.imageDimensions",
            &[
                ("format", "PNG"),
                (
                    "width",
                    &u32::from_be_bytes(data[16..20].try_into().ok()?).to_string(),
                ),
                (
                    "height",
                    &u32::from_be_bytes(data[20..24].try_into().ok()?).to_string(),
                ),
            ],
        ));
    }
    if data.starts_with(b"GIF8") && data.len() >= 10 {
        return Some(bds_core::i18n::translate_with(
            locale,
            "tui.imageDimensions",
            &[
                ("format", "GIF"),
                (
                    "width",
                    &u16::from_le_bytes(data[6..8].try_into().ok()?).to_string(),
                ),
                (
                    "height",
                    &u16::from_le_bytes(data[8..10].try_into().ok()?).to_string(),
                ),
            ],
        ));
    }
    Some(bds_core::i18n::translate_with(
        locale,
        "tui.fileBytes",
        &[("count", &data.len().to_string())],
    ))
}

pub fn run_local(host: ApplicationHost) -> Result<()> {
    ratatui::run(|terminal| -> io::Result<()> {
        let mut app = TuiApp::new(host, false).map_err(io::Error::other)?;
        loop {
            app.poll().map_err(io::Error::other)?;
            terminal.draw(|frame| app.render(frame))?;
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        if let Some(input) = crossterm_input(key) {
                            app.handle_input(input).map_err(io::Error::other)?;
                        }
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
            if app.should_quit() {
                break Ok(());
            }
        }
    })
    .map_err(Into::into)
}

fn crossterm_input(key: KeyEvent) -> Option<TuiInput> {
    let modifiers = key.modifiers;
    let key = match key.code {
        KeyCode::Char(value) => TuiKey::Char(value),
        KeyCode::Enter => TuiKey::Enter,
        KeyCode::Esc => TuiKey::Esc,
        KeyCode::Backspace => TuiKey::Backspace,
        KeyCode::Delete => TuiKey::Delete,
        KeyCode::Up => TuiKey::Up,
        KeyCode::Down => TuiKey::Down,
        KeyCode::Left => TuiKey::Left,
        KeyCode::Right => TuiKey::Right,
        KeyCode::Home => TuiKey::Home,
        KeyCode::End => TuiKey::End,
        KeyCode::PageUp => TuiKey::PageUp,
        KeyCode::PageDown => TuiKey::PageDown,
        KeyCode::Tab | KeyCode::BackTab => TuiKey::Tab,
        _ => return None,
    };
    Some(TuiInput {
        key,
        modifiers: TuiModifiers {
            ctrl: modifiers.contains(KeyModifiers::CONTROL),
            shift: modifiers.contains(KeyModifiers::SHIFT),
            alt: modifiers.contains(KeyModifiers::ALT),
        },
    })
}

#[derive(Default)]
pub(crate) struct InputDecoder {
    pending: Vec<u8>,
}

impl InputDecoder {
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Vec<TuiInput> {
        self.pending.extend_from_slice(bytes);
        let mut inputs = Vec::new();
        while let Some(first) = self.pending.first().copied() {
            if first == 0x1b {
                if self.pending.len() == 1 {
                    break;
                }
                let sequences: &[(&[u8], TuiKey)] = &[
                    (b"\x1b[A", TuiKey::Up),
                    (b"\x1b[B", TuiKey::Down),
                    (b"\x1b[C", TuiKey::Right),
                    (b"\x1b[D", TuiKey::Left),
                    (b"\x1b[H", TuiKey::Home),
                    (b"\x1b[F", TuiKey::End),
                    (b"\x1b[5~", TuiKey::PageUp),
                    (b"\x1b[6~", TuiKey::PageDown),
                    (b"\x1b[3~", TuiKey::Delete),
                    (b"\x1b[Z", TuiKey::Tab),
                ];
                if let Some((sequence, key)) = sequences
                    .iter()
                    .find(|(sequence, _)| self.pending.starts_with(sequence))
                {
                    self.pending.drain(..sequence.len());
                    inputs.push(TuiInput::plain(*key));
                    continue;
                }
                if sequences
                    .iter()
                    .any(|(sequence, _)| sequence.starts_with(&self.pending))
                {
                    break;
                }
                self.pending.remove(0);
                inputs.push(TuiInput::plain(TuiKey::Esc));
                continue;
            }
            let (input, consumed) = match first {
                b'\r' | b'\n' => (Some(TuiInput::plain(TuiKey::Enter)), 1),
                b'\t' => (Some(TuiInput::plain(TuiKey::Tab)), 1),
                0x7f | 0x08 => (Some(TuiInput::plain(TuiKey::Backspace)), 1),
                1..=26 => (
                    Some(TuiInput::ctrl(TuiKey::Char((b'a' + first - 1) as char))),
                    1,
                ),
                _ => match std::str::from_utf8(&self.pending) {
                    Ok(value) => {
                        let value = value.chars().next().expect("non-empty input");
                        (Some(TuiInput::plain(TuiKey::Char(value))), value.len_utf8())
                    }
                    Err(error) if error.error_len().is_none() => break,
                    Err(_) => (None, 1),
                },
            };
            self.pending.drain(..consumed);
            if let Some(input) = input {
                inputs.push(input);
            }
        }
        inputs
    }

    pub(crate) fn flush(&mut self) -> Vec<TuiInput> {
        if self.pending.as_slice() == [0x1b] {
            self.pending.clear();
            vec![TuiInput::plain(TuiKey::Esc)]
        } else {
            Vec::new()
        }
    }
}

pub(crate) fn render_ansi(app: &mut TuiApp, width: u16, height: u16) -> Vec<u8> {
    let width = width.max(20);
    let height = height.max(6);
    let area = Rect::new(0, 0, width, height);
    app.resize(width, height);
    let mut buffer = Buffer::empty(area);
    app.render_buffer(area, &mut buffer);
    let mut output = String::from("\x1b[?25l\x1b[2J\x1b[H");
    let mut last_style: Option<Style> = None;
    for y in 0..height {
        if y > 0 {
            output.push_str("\r\n");
        }
        for x in 0..width {
            let cell = &buffer[(x, y)];
            if last_style != Some(cell.style()) {
                output.push_str(&ansi_style(cell.style()));
                last_style = Some(cell.style());
            }
            output.push_str(cell.symbol());
        }
    }
    output.push_str("\x1b[0m");
    output.into_bytes()
}

fn ansi_style(style: Style) -> String {
    let mut codes = vec!["0".to_string()];
    if style.add_modifier.contains(Modifier::BOLD) {
        codes.push("1".into());
    }
    if style.add_modifier.contains(Modifier::ITALIC) {
        codes.push("3".into());
    }
    if style.add_modifier.contains(Modifier::UNDERLINED) {
        codes.push("4".into());
    }
    if let Some(color) = style.fg {
        codes.push(ansi_color(color, false));
    }
    if let Some(color) = style.bg {
        codes.push(ansi_color(color, true));
    }
    format!("\x1b[{}m", codes.join(";"))
}

fn ansi_color(color: Color, background: bool) -> String {
    let prefix = if background { 48 } else { 38 };
    match color {
        Color::Rgb(r, g, b) => format!("{prefix};2;{r};{g};{b}"),
        Color::Black => if background { "40" } else { "30" }.into(),
        Color::Red => if background { "41" } else { "31" }.into(),
        Color::Green => if background { "42" } else { "32" }.into(),
        Color::Yellow => if background { "43" } else { "33" }.into(),
        Color::Blue => if background { "44" } else { "34" }.into(),
        Color::Magenta => if background { "45" } else { "35" }.into(),
        Color::Cyan => if background { "46" } else { "36" }.into(),
        Color::Gray | Color::White => if background { "47" } else { "37" }.into(),
        _ => if background { "49" } else { "39" }.into(),
    }
}

fn editor(
    entity: EditorEntity,
    title: String,
    syntax: &'static str,
    post_language: Option<String>,
    content: &str,
) -> Editor {
    let mut buffer = EditorBuffer::new(content);
    buffer.set_soft_wrap(true);
    Editor {
        entity,
        title,
        syntax,
        post_language,
        buffer,
        mode: EditorMode::Source,
    }
}

enum PublishedKind {
    Post,
    Template,
    Script,
}

fn read_published_body(root: &Path, relative: &str, kind: PublishedKind) -> Result<String> {
    if relative.is_empty() {
        return Ok(String::new());
    }
    let source = fs::read_to_string(root.join(relative))?;
    let body = match kind {
        PublishedKind::Post => {
            bds_core::util::frontmatter::read_post_file(&source)
                .map_err(|error| anyhow!(error))?
                .1
        }
        PublishedKind::Template => {
            bds_core::util::frontmatter::read_template_file(&source)
                .map_err(|error| anyhow!(error))?
                .1
        }
        PublishedKind::Script => {
            bds_core::util::frontmatter::read_script_file(&source)
                .map_err(|error| anyhow!(error))?
                .1
        }
    };
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;
    use std::time::Instant;

    struct Fixture {
        _root: tempfile::TempDir,
        host: ApplicationHost,
        project: Project,
        data_dir: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let database_path = root.path().join("bds.db");
            let data_root = root.path().join("app");
            let data_dir = root.path().join("blog");
            let host = ApplicationHost::start(database_path.clone(), data_root).unwrap();
            let db = Database::open(&database_path).unwrap();
            let project = engine::project::create_project(
                db.conn(),
                "Terminal Blog",
                Some(data_dir.to_str().unwrap()),
            )
            .unwrap();
            engine::project::set_active_project(db.conn(), &project.id).unwrap();
            engine::settings::set(
                db.conn(),
                engine::settings::UI_LANGUAGE_KEY,
                UiLocale::En.code(),
            )
            .unwrap();
            Self {
                _root: root,
                host,
                project,
                data_dir,
            }
        }

        fn app(&self, remote: bool) -> TuiApp {
            TuiApp::new(self.host.clone(), remote).unwrap()
        }
        fn db(&self) -> Database {
            self.host.database().unwrap()
        }
    }

    fn type_text(app: &mut TuiApp, value: &str) {
        for character in value.chars() {
            app.handle_input(TuiInput::plain(TuiKey::Char(character)))
                .unwrap();
        }
    }

    fn wait_for(app: &mut TuiApp, predicate: impl Fn(&TuiApp) -> bool) {
        let started = Instant::now();
        while !predicate(app) && started.elapsed() < Duration::from_secs(2) {
            app.poll().unwrap();
            thread::sleep(Duration::from_millis(10));
        }
        assert!(predicate(app));
    }

    fn rendered_text(app: &TuiApp, width: u16, height: u16) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buffer = Buffer::empty(area);
        app.render_buffer(area, &mut buffer);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn git(directory: &Path, arguments: &[&str]) {
        let status = std::process::Command::new("git")
            .args(arguments)
            .current_dir(directory)
            .status()
            .unwrap();
        assert!(status.success(), "git {arguments:?} failed");
    }

    #[test]
    fn sidebar_navigation_skips_headers_wraps_and_views_are_numbered() {
        let fixture = Fixture::new();
        let db = fixture.db();
        engine::post::create_post(
            db.conn(),
            &fixture.data_dir,
            &fixture.project.id,
            "One",
            Some("body"),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
        )
        .unwrap();
        let mut app = fixture.app(false);
        assert!(app.items[app.selected_index].selectable());
        app.handle_input(TuiInput::plain(TuiKey::Down)).unwrap();
        assert!(app.items[app.selected_index].selectable());
        app.handle_input(TuiInput::plain(TuiKey::Char('4')))
            .unwrap();
        assert_eq!(app.view, TuiView::Scripts);
        app.handle_input(TuiInput::plain(TuiKey::Char('6')))
            .unwrap();
        assert_eq!(app.view, TuiView::Settings);
    }

    #[test]
    fn posts_templates_and_scripts_edit_validate_save_and_publish_through_shared_engines() {
        let fixture = Fixture::new();
        let mut app = fixture.app(false);
        app.handle_input(TuiInput::plain(TuiKey::Char('n')))
            .unwrap();
        type_text(&mut app, "Hello **terminal**");
        app.handle_input(TuiInput::ctrl(TuiKey::Char('t'))).unwrap();
        for _ in 0..app.tr("tui.untitled").chars().count() {
            app.handle_input(TuiInput::plain(TuiKey::Backspace))
                .unwrap();
        }
        type_text(&mut app, "Terminal Post");
        app.handle_input(TuiInput::plain(TuiKey::Enter)).unwrap();
        app.handle_input(TuiInput::ctrl(TuiKey::Char('l'))).unwrap();
        app.handle_input(TuiInput::ctrl(TuiKey::Char('s'))).unwrap();
        assert!(!app.has_unsaved_changes());
        app.handle_input(TuiInput::ctrl(TuiKey::Char('p'))).unwrap();
        let db = fixture.db();
        let post =
            bds_core::db::queries::post::list_posts_by_project(db.conn(), &fixture.project.id)
                .unwrap()
                .pop()
                .unwrap();
        assert_eq!(post.status, PostStatus::Published);
        assert!(post.content.is_none());
        assert!(fixture.data_dir.join(post.file_path).is_file());
        assert_eq!(post.title, "Terminal Post");
        app.handle_input(TuiInput::ctrl(TuiKey::Char('u'))).unwrap();
        let post = bds_core::db::queries::post::get_post_by_id(db.conn(), &post.id).unwrap();
        assert_eq!(post.status, PostStatus::Archived);

        app.handle_input(TuiInput::plain(TuiKey::Esc)).unwrap();
        app.handle_input(TuiInput::plain(TuiKey::Char('3')))
            .unwrap();
        app.handle_input(TuiInput::plain(TuiKey::Char('n')))
            .unwrap();
        type_text(&mut app, "<h1>{{ post.title }}</h1>");
        app.handle_input(TuiInput::ctrl(TuiKey::Char('p'))).unwrap();
        let template = bds_core::db::queries::template::list_templates_by_project(
            db.conn(),
            &fixture.project.id,
        )
        .unwrap()
        .pop()
        .unwrap();
        assert_eq!(template.status, TemplateStatus::Published);
        app.handle_input(TuiInput::ctrl(TuiKey::Char('u'))).unwrap();
        assert_eq!(
            bds_core::db::queries::template::get_template_by_id(db.conn(), &template.id)
                .unwrap()
                .status,
            TemplateStatus::Draft
        );

        app.handle_input(TuiInput::plain(TuiKey::Esc)).unwrap();
        app.handle_input(TuiInput::plain(TuiKey::Char('4')))
            .unwrap();
        app.handle_input(TuiInput::plain(TuiKey::Char('n')))
            .unwrap();
        app.handle_input(TuiInput::ctrl(TuiKey::Char('p'))).unwrap();
        let script =
            bds_core::db::queries::script::list_scripts_by_project(db.conn(), &fixture.project.id)
                .unwrap()
                .pop()
                .unwrap();
        assert_eq!(script.status, ScriptStatus::Published);
        app.handle_input(TuiInput::ctrl(TuiKey::Char('u'))).unwrap();
        assert_eq!(
            bds_core::db::queries::script::get_script_by_id(db.conn(), &script.id)
                .unwrap()
                .status,
            ScriptStatus::Draft
        );
    }

    #[test]
    fn preview_is_read_only_and_source_editor_soft_wraps_without_a_gutter() {
        let fixture = Fixture::new();
        let mut app = fixture.app(false);
        app.handle_input(TuiInput::plain(TuiKey::Char('n')))
            .unwrap();
        type_text(&mut app, "# Heading\nA very long paragraph");
        let before = app.editor_text().unwrap();
        assert!(app.editor.as_ref().unwrap().buffer.soft_wrap());
        app.handle_input(TuiInput::ctrl(TuiKey::Char('e'))).unwrap();
        type_text(&mut app, "ignored");
        assert_eq!(app.editor_text().unwrap(), before);
        let rendered = rendered_text(&app, 70, 18);
        assert!(rendered.contains("Heading"));
        assert!(!rendered.contains(" 1 │"));

        app.handle_input(TuiInput::ctrl(TuiKey::Char('e'))).unwrap();
        type_text(&mut app, &"x".repeat(1_000));
        assert!(app.editor.as_ref().unwrap().buffer.scroll_offset() > 0);
    }

    #[test]
    fn live_filters_are_per_view_and_combine_text_tag_category_and_date_tokens() {
        let fixture = Fixture::new();
        let db = fixture.db();
        engine::post::create_post(
            db.conn(),
            &fixture.data_dir,
            &fixture.project.id,
            "Rust Notes",
            Some("body"),
            vec!["rust".into()],
            vec!["dev".into()],
            None,
            None,
            None,
        )
        .unwrap();
        engine::post::create_post(
            db.conn(),
            &fixture.data_dir,
            &fixture.project.id,
            "Other",
            Some("body"),
            vec!["misc".into()],
            Vec::new(),
            None,
            None,
            None,
        )
        .unwrap();
        let mut app = fixture.app(false);
        app.handle_input(TuiInput::plain(TuiKey::Char('/')))
            .unwrap();
        type_text(&mut app, "Rust tag:rust category:dev");
        assert_eq!(
            app.items
                .iter()
                .filter(|item| matches!(item, SidebarItem::Post(_, _)))
                .count(),
            1
        );
        app.handle_input(TuiInput::plain(TuiKey::Enter)).unwrap();
        app.handle_input(TuiInput::plain(TuiKey::Char('2')))
            .unwrap();
        assert!(!app.filters.contains_key(&TuiView::Media));
        assert_eq!(
            app.filters.get(&TuiView::Posts).map(String::as_str),
            Some("Rust tag:rust category:dev")
        );
    }

    #[test]
    fn external_events_preserve_dirty_buffers_and_external_delete_closes_the_editor() {
        let fixture = Fixture::new();
        let db = fixture.db();
        let post = engine::post::create_post(
            db.conn(),
            &fixture.data_dir,
            &fixture.project.id,
            "Open",
            Some("base"),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
        )
        .unwrap();
        let mut app = fixture.app(true);
        app.open_post(&post.id).unwrap();
        type_text(&mut app, " dirty");
        let dirty = app.editor_text().unwrap();
        engine::post::create_post(
            db.conn(),
            &fixture.data_dir,
            &fixture.project.id,
            "Elsewhere",
            Some("body"),
            Vec::new(),
            Vec::new(),
            None,
            None,
            None,
        )
        .unwrap();
        app.poll().unwrap();
        assert_eq!(app.editor_text().unwrap(), dirty);
        engine::post::delete_post(db.conn(), &fixture.data_dir, &post.id).unwrap();
        app.poll().unwrap();
        assert!(app.editor.is_none());
        assert_eq!(app.focus, Focus::Sidebar);

        let mut metadata = engine::meta::read_project_json(&fixture.data_dir).unwrap();
        metadata.name = "Changed on disk".into();
        engine::meta::write_project_json(&fixture.data_dir, &metadata).unwrap();
        app.panel = Panel::Report {
            title: "Metadata Diff".into(),
            body: "stale report".into(),
            action: ReportAction::MetadataDiff,
        };
        domain_events::entity_changed(
            &fixture.project.id,
            DomainEntity::Project,
            &fixture.project.id,
            NotificationAction::Updated,
        );
        app.poll().unwrap();
        assert!(matches!(app.panel, Panel::Report { .. }));
        assert!(!matches!(&app.panel, Panel::Report { body, .. } if body == "stale report"));
    }

    #[test]
    fn settings_are_typed_saved_and_server_locale_relocalizes_every_session() {
        let fixture = Fixture::new();
        let mut app = fixture.app(false);
        assert_eq!(
            SettingSection::ALL,
            [
                SettingSection::Project,
                SettingSection::Editor,
                SettingSection::Content,
                SettingSection::Ai,
                SettingSection::Technology,
                SettingSection::Publishing,
                SettingSection::Data,
                SettingSection::Mcp,
            ]
        );
        let removed_prefix = ["style", "."].concat();
        for section in SettingSection::ALL {
            let fields = app.load_setting_fields(section).unwrap();
            assert!(
                fields
                    .iter()
                    .all(|field| !field.key.starts_with(&removed_prefix))
            );
            assert!(fields.iter().all(|field| field.label != field.key));
        }
        app.set_view(TuiView::Settings).unwrap();
        app.selected_index = 2;
        app.open_selected().unwrap();
        app.panel_index = 2;
        app.handle_input(TuiInput::plain(TuiKey::Enter)).unwrap();
        app.handle_input(TuiInput::ctrl(TuiKey::Char('s'))).unwrap();
        assert_eq!(
            engine::settings::get(fixture.db().conn(), "editor.wrap_long_lines")
                .unwrap()
                .as_deref(),
            Some("false")
        );
        app.open_settings(SettingSection::Project).unwrap();
        assert_eq!(
            app.settings_fields
                .iter()
                .find(|field| field.key == "project.name")
                .unwrap()
                .label,
            bds_core::i18n::translate(UiLocale::En, "settings.projectName")
        );
        engine::settings::set(fixture.db().conn(), engine::settings::UI_LANGUAGE_KEY, "de")
            .unwrap();
        app.poll().unwrap();
        assert_eq!(app.locale(), UiLocale::De);
        assert!(rendered_text(&app, 70, 18).contains("Einstellungen"));
        assert_eq!(
            app.settings_fields
                .iter()
                .find(|field| field.key == "project.name")
                .unwrap()
                .label,
            bds_core::i18n::translate(UiLocale::De, "settings.projectName")
        );
        app.save_settings().unwrap();
        assert_eq!(
            app.status,
            bds_core::i18n::translate(UiLocale::De, "tui.settingsSaved")
        );
        assert_eq!(fixture.app(true).locale(), UiLocale::De);
    }

    #[test]
    fn project_content_ai_and_publishing_settings_use_shared_backends() {
        let fixture = Fixture::new();
        let mut app = fixture.app(false);

        app.open_settings(SettingSection::Project).unwrap();
        app.settings_fields
            .iter_mut()
            .find(|field| field.key == "project.name")
            .unwrap()
            .value = "Renamed Terminal Blog".into();
        app.save_settings().unwrap();
        assert_eq!(
            bds_core::db::queries::project::get_project_by_id(
                fixture.db().conn(),
                &fixture.project.id
            )
            .unwrap()
            .name,
            "Renamed Terminal Blog"
        );
        assert_eq!(
            engine::meta::read_project_json(&fixture.data_dir)
                .unwrap()
                .name,
            "Renamed Terminal Blog"
        );

        app.open_settings(SettingSection::Project).unwrap();
        app.settings_fields
            .iter_mut()
            .find(|field| field.key == "meta.max_posts_per_page")
            .unwrap()
            .value = "75".into();
        app.settings_fields
            .iter_mut()
            .find(|field| field.key == "meta.blog_languages")
            .unwrap()
            .value = "de, en, de".into();
        app.save_settings().unwrap();
        let metadata = engine::meta::read_project_json(&fixture.data_dir).unwrap();
        assert_eq!(metadata.max_posts_per_page, 75);
        assert_eq!(metadata.blog_languages, ["de", "en"]);

        let mut remote = fixture.app(true);
        remote.open_settings(SettingSection::Publishing).unwrap();
        remote
            .settings_fields
            .iter_mut()
            .find(|field| field.key == "publishing.ssh_host")
            .unwrap()
            .value = "example.net".into();
        remote
            .settings_fields
            .iter_mut()
            .find(|field| field.key == "publishing.ssh_mode")
            .unwrap()
            .value = "rsync".into();
        remote.save_settings().unwrap();
        let publishing = engine::meta::read_publishing_json(&fixture.data_dir).unwrap();
        assert_eq!(publishing.ssh_host.as_deref(), Some("example.net"));
        assert_eq!(publishing.ssh_mode, SshMode::Rsync);

        remote.open_settings(SettingSection::Ai).unwrap();
        remote
            .settings_fields
            .iter_mut()
            .find(|field| field.key == "ai.endpoint.airplane.url")
            .unwrap()
            .value = "http://127.0.0.1:11434/v1".into();
        remote
            .settings_fields
            .iter_mut()
            .find(|field| field.key == "ai.endpoint.airplane.model")
            .unwrap()
            .value = "local-model".into();
        remote.save_settings().unwrap();
        let ai = engine::ai::load_ai_settings(fixture.db().conn(), false).unwrap();
        assert_eq!(ai.airplane.endpoint.model, "local-model");
    }

    #[test]
    fn tags_manage_colour_template_delete_sync_and_merge_use_tag_engine() {
        let fixture = Fixture::new();
        let db = fixture.db();
        let one = engine::tag::create_tag(
            db.conn(),
            &fixture.data_dir,
            &fixture.project.id,
            "one",
            None,
        )
        .unwrap();
        let two = engine::tag::create_tag(
            db.conn(),
            &fixture.data_dir,
            &fixture.project.id,
            "two",
            None,
        )
        .unwrap();
        let mut app = fixture.app(false);
        app.panel = Panel::Tags(TagSection::Merge);
        app.focus = Focus::Editor;
        app.panel_index = 0;
        app.toggle_tag_mark();
        app.panel_index = 1;
        app.toggle_tag_mark();
        app.merge_marked_tags().unwrap();
        assert_eq!(app.tags().unwrap().len(), 1);
        app.panel_index = 0;
        app.cycle_tag_color().unwrap();
        assert!(app.tags().unwrap()[0].color.is_some());
        app.sync_tags().unwrap();
        assert!(fixture.data_dir.join("meta/tags.json").is_file());
        assert!(
            app.tags()
                .unwrap()
                .iter()
                .any(|tag| tag.id == one.id || tag.id == two.id)
        );
    }

    #[test]
    fn command_reports_run_as_tasks_and_only_open_after_completion() {
        let fixture = Fixture::new();
        let mut app = fixture.app(false);
        app.run_command("metadata-diff").unwrap();
        assert!(fixture.host.tasks().running_count() + fixture.host.tasks().pending_count() >= 1);
        wait_for(&mut app, |app| {
            matches!(
                app.panel,
                Panel::Report {
                    action: ReportAction::MetadataDiff,
                    ..
                }
            )
        });
        assert_eq!(app.status, app.tr("tui.metadataDiffComplete"));
        let report = engine::metadata_diff::compute_metadata_diff(
            fixture.db().conn(),
            &fixture.data_dir,
            &fixture.project.id,
        )
        .unwrap();
        let applied = (report.diffs.len() + report.orphans.len()).to_string();
        app.handle_input(TuiInput::plain(TuiKey::Enter)).unwrap();
        assert_eq!(
            app.status,
            app.tr_with("tui.metadataChangesApplied", &[("count", &applied)])
        );
        assert!(matches!(app.panel, Panel::Welcome));
    }

    #[test]
    fn command_palette_lists_filters_selects_and_marks_gui_only_commands() {
        let fixture = Fixture::new();
        let mut app = fixture.app(false);
        app.handle_input(TuiInput::plain(TuiKey::Char(':')))
            .unwrap();
        let listed = rendered_text(&app, 90, 24);
        assert!(listed.contains(&app.tr("tui.commandMetadataDiff")));
        assert!(listed.contains("[GUI]"));
        let first = app.prompt.as_ref().unwrap().candidates[0].clone();
        app.handle_input(TuiInput::plain(TuiKey::Down)).unwrap();
        assert_ne!(app.prompt.as_ref().unwrap().candidates[0], first);
        app.handle_input(TuiInput::plain(TuiKey::Esc)).unwrap();
        app.handle_input(TuiInput::plain(TuiKey::Char(':')))
            .unwrap();
        let query = app
            .tr("tui.commandMetadataDiff")
            .chars()
            .take(4)
            .collect::<String>();
        type_text(&mut app, &query);
        app.handle_input(TuiInput::plain(TuiKey::Enter)).unwrap();
        wait_for(&mut app, |app| {
            matches!(
                app.panel,
                Panel::Report {
                    action: ReportAction::MetadataDiff,
                    ..
                }
            )
        });
    }

    #[test]
    fn projects_overlay_path_validation_and_completion_are_directory_only() {
        let fixture = Fixture::new();
        let folder = fixture._root.path().join("second-blog");
        fs::create_dir_all(&folder).unwrap();
        fs::write(fixture._root.path().join("second-file"), "x").unwrap();
        let mut prompt = Prompt {
            kind: PromptKind::ProjectPath,
            value: fixture._root.path().join("sec").display().to_string(),
            candidates: Vec::new(),
        };
        complete_path(&mut prompt);
        assert!(prompt.value.ends_with("second-blog/"));
        assert!(!prompt.candidates.iter().any(|value| value == "second-file"));
        let mut app = fixture.app(false);
        app.handle_input(TuiInput::plain(TuiKey::Char('p')))
            .unwrap();
        app.handle_input(TuiInput::plain(TuiKey::Char('o')))
            .unwrap();
        assert!(matches!(
            app.prompt,
            Some(Prompt {
                kind: PromptKind::ProjectPath,
                ..
            })
        ));
        app.handle_input(TuiInput::plain(TuiKey::Esc)).unwrap();
        assert!(matches!(app.overlay, Some(Overlay::Projects { .. })));
        app.open_project_path(folder.to_str().unwrap()).unwrap();
        assert_eq!(app.project.as_ref().unwrap().name, "second-blog");
    }

    #[test]
    fn local_and_remote_apps_share_persistence_but_identify_their_transport() {
        let fixture = Fixture::new();
        let mut local = fixture.app(false);
        let mut remote = fixture.app(true);
        local
            .handle_input(TuiInput::plain(TuiKey::Char('n')))
            .unwrap();
        type_text(&mut local, "shared");
        local
            .handle_input(TuiInput::ctrl(TuiKey::Char('s')))
            .unwrap();
        remote.poll().unwrap();
        assert!(
            remote
                .items
                .iter()
                .any(|item| matches!(item, SidebarItem::Post(_, title) if title == &remote.tr("tui.untitled")))
        );
        let local_render = rendered_text(&local, 60, 15);
        let remote_render = rendered_text(&remote, 60, 15);
        assert!(
            local_render.contains(&local.tr("tui.local")),
            "{local_render}"
        );
        assert!(
            remote_render.contains(&remote.tr("tui.remote")),
            "{remote_render}"
        );
    }

    #[test]
    fn pty_decoder_handles_split_escape_utf8_control_and_resize_renderer() {
        let mut decoder = InputDecoder::default();
        assert!(decoder.push(b"\x1b").is_empty());
        assert_eq!(decoder.push(b"[A"), vec![TuiInput::plain(TuiKey::Up)]);
        assert_eq!(
            decoder.push("é".as_bytes()),
            vec![TuiInput::plain(TuiKey::Char('é'))]
        );
        assert_eq!(decoder.push(&[19]), vec![TuiInput::ctrl(TuiKey::Char('s'))]);
        assert!(decoder.push(b"\x1b").is_empty());
        assert_eq!(decoder.flush(), vec![TuiInput::plain(TuiKey::Esc)]);
        let fixture = Fixture::new();
        let mut app = fixture.app(true);
        let small = render_ansi(&mut app, 40, 10);
        let large = render_ansi(&mut app, 100, 30);
        assert!(small.starts_with(b"\x1b[?25l\x1b[2J\x1b[H"));
        assert!(large.len() > small.len());
        assert!(large.ends_with(b"\x1b[0m"));
    }

    #[test]
    fn unsaved_quit_requires_confirmation_and_clean_quit_does_not() {
        let fixture = Fixture::new();
        let mut app = fixture.app(false);
        app.handle_input(TuiInput::plain(TuiKey::Char('n')))
            .unwrap();
        type_text(&mut app, "dirty");
        app.handle_input(TuiInput::ctrl(TuiKey::Char('q'))).unwrap();
        assert!(matches!(app.overlay, Some(Overlay::ConfirmDiscard)));
        app.handle_input(TuiInput::plain(TuiKey::Char('n')))
            .unwrap();
        assert!(!app.should_quit());
        app.handle_input(TuiInput::ctrl(TuiKey::Char('s'))).unwrap();
        app.handle_input(TuiInput::ctrl(TuiKey::Char('q'))).unwrap();
        assert!(app.should_quit());
    }

    #[test]
    fn validation_and_ai_gating_are_reported_without_ending_the_session() {
        let fixture = Fixture::new();
        let mut app = fixture.app(false);
        app.open_settings(SettingSection::Project).unwrap();
        app.settings_fields
            .iter_mut()
            .find(|field| field.key == "meta.max_posts_per_page")
            .unwrap()
            .value = "0".into();
        app.handle_input(TuiInput::ctrl(TuiKey::Char('s'))).unwrap();
        assert!(app.status.contains("1..500"));
        assert!(!app.should_quit());

        app.set_view(TuiView::Posts).unwrap();
        app.handle_input(TuiInput::plain(TuiKey::Char('n')))
            .unwrap();
        app.handle_input(TuiInput::ctrl(TuiKey::Char('g'))).unwrap();
        assert!(!app.status.is_empty());
        assert!(app.output.is_empty());
        assert!(!app.should_quit());
    }

    #[test]
    fn ai_quick_action_uses_the_active_airplane_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 8192];
            let size = stream.read(&mut request).unwrap();
            assert!(
                String::from_utf8_lossy(&request[..size])
                    .starts_with("POST /v1/chat/completions HTTP/1.1")
            );
            let body = r#"{"choices":[{"message":{"content":"{\"title\":\"Sharper title\",\"excerpt\":\"Brief\",\"slug\":\"sharper-title\"}"}}]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        });
        let fixture = Fixture::new();
        engine::ai::save_endpoint(
            fixture.db().conn(),
            &AiEndpointConfig {
                kind: AiEndpointKind::Airplane,
                url: endpoint,
                model: "local-test".into(),
                api_key: None,
            },
        )
        .unwrap();
        let mut app = fixture.app(true);
        app.airplane = true;
        app.handle_input(TuiInput::plain(TuiKey::Char('n')))
            .unwrap();
        type_text(&mut app, "draft body");
        app.handle_input(TuiInput::ctrl(TuiKey::Char('g'))).unwrap();
        server.join().unwrap();
        assert!(app.output.last().unwrap().contains("Sharper title"));
        assert_eq!(app.status, app.tr("tui.aiSuggestionsAdded"));
    }

    #[test]
    fn image_media_opens_as_a_colour_halfblock_preview_that_survives_resize() {
        let fixture = Fixture::new();
        let source = fixture._root.path().join("preview.png");
        image::RgbImage::from_pixel(12, 8, image::Rgb([220, 40, 80]))
            .save(&source)
            .unwrap();
        let db = fixture.db();
        let media = engine::media::import_media(
            db.conn(),
            &fixture.data_dir,
            &fixture.project.id,
            &source,
            "preview.png",
            Some("Preview"),
            None,
            None,
            None,
            None,
            Vec::new(),
        )
        .unwrap();
        let mut app = fixture.app(true);
        app.open_media(&media.id).unwrap();
        assert!(app.image_protocol.is_some());
        let before = rendered_text(&app, 60, 18);
        app.resize(100, 30);
        let after = rendered_text(&app, 100, 30);
        assert!(before.contains('▀'));
        assert!(after.contains('▀'));
        assert!(after.len() > before.len());
    }

    #[test]
    fn git_actions_in_a_non_repository_report_instead_of_starting_work() {
        let fixture = Fixture::new();
        let mut app = fixture.app(false);
        app.set_view(TuiView::Git).unwrap();
        app.git_pull().unwrap();
        assert_eq!(app.status, app.tr("git.notRepository"));
        app.git_push().unwrap();
        assert_eq!(fixture.host.tasks().running_count(), 0);
    }

    #[test]
    fn local_git_commit_is_visible_in_the_remote_terminal_session() {
        let fixture = Fixture::new();
        git(&fixture.data_dir, &["init", "-b", "master"]);
        git(&fixture.data_dir, &["config", "user.name", "TUI Test"]);
        git(
            &fixture.data_dir,
            &["config", "user.email", "tui@example.invalid"],
        );
        fs::write(fixture.data_dir.join("terminal-change.txt"), "shared").unwrap();

        let mut local = fixture.app(false);
        local.set_view(TuiView::Git).unwrap();
        assert!(local.items.iter().any(
            |item| matches!(item, SidebarItem::GitFile(path, _) if path == "terminal-change.txt")
        ));
        local
            .handle_input(TuiInput::plain(TuiKey::Char('c')))
            .unwrap();
        type_text(&mut local, "Commit from terminal");
        local.handle_input(TuiInput::plain(TuiKey::Enter)).unwrap();
        wait_for(&mut local, |app| {
            app.items.iter().any(
                |item| matches!(item, SidebarItem::GitCommit(_, subject) if subject.contains("Commit from terminal")),
            )
        });

        let mut remote = fixture.app(true);
        remote.set_view(TuiView::Git).unwrap();
        assert!(remote.items.iter().any(
            |item| matches!(item, SidebarItem::GitCommit(_, subject) if subject.contains("Commit from terminal"))
        ));
    }
}
