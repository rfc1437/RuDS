use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use bds_core::db::DbQueryError as SqlError;
use iced::{Element, Subscription, Task, window};
use serde_json::json;
use uuid::Uuid;

use bds_core::db::Database;
use bds_core::engine;
use bds_core::engine::ai::{self, AiEndpointConfig, AiEndpointKind};
use bds_core::engine::task::{TaskId, TaskManager, TaskStatus};
use bds_core::i18n::{UiLocale, detect_os_locale, normalize_language};
use bds_core::model::{
    ChatConversation, DomainEntity, DomainEvent, ImportDefinition, ImportReport, Media,
    NotificationAction, Post, PostStatus, PostTranslation, Project, PublishingPreferences, Script,
    SshMode, Template,
};

use crate::components::native_edit;
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
    chat_view::{ChatEditorState, ChatModelChoice},
    dashboard::{
        DashboardCategory, DashboardRecentPost, DashboardState, DashboardStats, DashboardTag,
        DashboardTimelineMonth,
    },
    documentation::{DocumentLoad, DocumentationKind, DocumentationState, current_signature},
    duplicates::DuplicatesState,
    git::{GitDiffLoad, GitDiffState, GitNetworkCompletion, GitSnapshot, GitUiState},
    import_editor::{
        ImportAnalysisEvent, ImportEditorMsg, ImportEditorState, ImportExecutionEvent,
    },
    media_editor::{LinkedPostItem, MediaEditorMsg, MediaEditorState},
    menu_editor::{MenuEditorMsg, MenuEditorState, MenuEditorStatus},
    metadata_diff::MetadataDiffState,
    modal,
    post_editor::{LinkedMediaItem, PostEditorMsg, PostEditorState, ResolvedPostLink},
    script_editor::{ScriptEditorMsg, ScriptEditorState},
    settings_view::{
        AiModeViewState, AiModelOption, SettingsCategoryRow, SettingsMsg, SettingsViewState,
        default_category_rows,
    },
    site_validation::SiteValidationState,
    style_view::{StyleMsg, StyleViewState},
    tags_view::{self, TagsMsg, TagsSection, TagsViewState},
    template_editor::{TemplateEditorMsg, TemplateEditorState},
    workspace,
};

mod editor_handlers;
mod engine_handlers;
mod git_handlers;
mod preview_handlers;
mod search;
mod tasks;

// ───────────────────────────────────────────────────────────
// Message
// ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum OneShotAiAction {
    PostAnalysis,
    PostTaxonomy,
    PostLanguage,
    PostTranslation { target_language: String },
    MediaAnalysis,
    MediaLanguage,
    MediaTranslation { target_language: String },
}

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

    // Project
    ProjectsLoaded(Vec<Project>),
    SwitchProject(String),
    RequestCreateProject,
    RequestOpenProject,
    CreateProject {
        name: String,
        data_path: Option<PathBuf>,
    },
    DeleteProject(String),

    // Dialogs
    FolderPicked(Option<PathBuf>),
    ProjectFolderPicked(Option<PathBuf>),
    MediaFilesPicked(Option<Vec<PathBuf>>),
    GalleryImagesPicked {
        post_id: String,
        result: Result<Option<Vec<PathBuf>>, String>,
    },
    GalleryImportFinished {
        post_id: String,
        report: engine::gallery_import::GalleryImportReport,
    },
    FileDropped(PathBuf),
    ImageDropImported {
        task_id: TaskId,
        post_id: String,
        project_id: String,
        data_dir: PathBuf,
        source_language: String,
        offline_mode: bool,
        path: PathBuf,
        result: Result<Media, String>,
    },
    ImageDropEnriched {
        task_id: TaskId,
        post_id: String,
        path: PathBuf,
        result: Result<String, String>,
    },
    MediaImportFinished {
        imported: Vec<Media>,
        errors: Vec<String>,
    },
    MediaReplacementPicked {
        media_id: String,
        path: Option<PathBuf>,
    },
    MediaReplacementFinished {
        media_id: String,
        result: Result<Option<Media>, String>,
    },
    ImportUploadsPicked {
        definition_id: String,
        path: Option<PathBuf>,
    },
    ImportWxrPicked {
        definition_id: String,
        path: Option<PathBuf>,
    },
    ImportAnalysisEvent {
        definition_id: String,
        event: ImportAnalysisEvent,
    },
    ImportExecutionEvent {
        definition_id: String,
        event: ImportExecutionEvent,
    },
    ImportAutoMapFinished {
        definition_id: String,
        result: Result<(ImportDefinition, ImportReport, usize), String>,
    },

    // Tasks
    TaskTick,
    DomainEventsTick,
    CancelTask(TaskId),
    ToggleTaskGroup(String),

    // macOS lifecycle
    FileOpenRequested(PathBuf),
    UrlOpenRequested(String),
    BlogmarkImported {
        task_id: TaskId,
        result: Result<engine::blogmark::BlogmarkImportResult, String>,
    },
    MainWindowLoaded(Option<window::Id>),
    EmbeddedPreviewReady(Result<(), String>),
    EmbeddedStylePreviewReady(Result<(), String>),

    // Panel
    SetPanelTab(PanelTab),

    // Settings
    SetOfflineMode(bool),
    SetUiLocale(UiLocale),
    ToggleLocaleDropdown,
    ToggleProjectDropdown,

    // Toast
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
    RemoteTargetChanged(String),
    RemoteConnectRequested,
    RemoteConnected(Result<(Arc<bds_server::DesktopClient>, Vec<Project>), String>),
    RemoteProjectSelected(String),
    RemoteOpenProjectRequested,
    RemoteProjectOpened(Result<Project, String>),
    RemoteDisconnectRequested,
    FindQueryChanged(String),
    ReplaceQueryChanged(String),
    FindNext,
    ReplaceCurrent,
    ReplaceAll,
    ToggleAiSuggestionField(usize, bool),
    ApplyAiSuggestions(modal::AiEntityTarget, Vec<modal::AiSuggestionField>),

    OneShotAiFinished {
        entity_id: String,
        action: OneShotAiAction,
        result: Result<ai::OneShotResponse, String>,
    },

    // Blog actions (dispatched to engine)
    RebuildDatabase,
    ReindexText,
    RegenerateCalendar,
    ValidateTranslations,
    TranslationValidationLoaded(
        Result<engine::validate_translations::TranslationValidationReport, String>,
    ),
    ValidateMedia,
    GenerateSite,
    RunMetadataDiff,
    MetadataDiffLoaded(Result<engine::metadata_diff::DiffReport, String>),
    RepairMetadataDiffItem {
        index: usize,
        direction: engine::metadata_diff::RepairDirection,
    },
    MetadataDiffItemRepaired(Result<(), String>),
    RunSiteValidation,
    ApplySiteValidation,
    EngineTaskDone {
        task_id: TaskId,
        operation: &'static str,
        label: String,
        result: Result<String, String>,
    },
    SiteGenerationSectionDone {
        group_id: String,
        task_id: TaskId,
        result: Result<engine::generation::GenerationReport, String>,
    },
    SiteGenerationIndexDone {
        group_id: String,
        task_id: TaskId,
        result: Result<engine::generation::GenerationReport, String>,
    },
    SiteValidationLoaded(Result<engine::validate_site::SiteValidationReport, String>),
    DuplicatesRefresh,
    DuplicatesLoaded(Result<engine::embedding::DuplicateSearchResult, String>),
    DuplicatesToggle(String, String),
    DuplicatesCheckAll,
    DuplicatesUncheckAll,
    DuplicatesDismiss(String, String),
    DuplicatesDismissSelected,
    DuplicatesDismissed(Result<(), String>),
    DuplicatesShowMore,
    DuplicatesOpenPost(String),
    DocumentationRefresh(DocumentationKind),
    DocumentationLoaded(DocumentationKind, u64, DocumentLoad),
    DocumentationLinkClicked(DocumentationKind, String),
    EmbeddingReindex,
    EmbeddingBackfill,
    LoadSemanticTagSuggestions(String),
    SemanticTagSuggestionsLoaded {
        post_id: String,
        result: Result<Vec<String>, String>,
    },

    // Git
    GitRefresh,
    GitLoaded {
        repository_dir: PathBuf,
        result: Result<GitSnapshot, String>,
    },
    GitRemoteInputChanged(String),
    GitCommitMessageChanged(String),
    GitInitialize,
    GitSetRemote,
    GitCommit,
    GitFetch,
    GitPull,
    GitPush,
    GitPruneLfs,
    GitLocalFinished {
        repository_dir: PathBuf,
        operation: engine::git::GitOperation,
        result: Result<GitSnapshot, String>,
    },
    GitNetworkFinished {
        repository_dir: PathBuf,
        task_id: TaskId,
        operation: engine::git::GitOperation,
        result: Result<GitNetworkCompletion, String>,
    },
    OpenGitFileDiff(String),
    OpenGitCommitDiff {
        hash: String,
        subject: String,
    },
    SelectGitCommitFile {
        hash: String,
        change: engine::git::ChangedFile,
    },
    GitDiffLoaded {
        repository_dir: PathBuf,
        tab_id: String,
        result: Result<GitDiffLoad, String>,
    },
    GitFileHistoryLoaded {
        repository_dir: PathBuf,
        path: String,
        result: Result<Vec<engine::git::GitCommit>, String>,
    },

    // Editor views
    PostEditor(PostEditorMsg),
    MediaEditor(MediaEditorMsg),
    TemplateEditor(TemplateEditorMsg),
    ScriptEditor(ScriptEditorMsg),
    Tags(TagsMsg),
    Settings(SettingsMsg),
    Style(StyleMsg),
    ImportEditor(ImportEditorMsg),
    MenuEditor(MenuEditorMsg),

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
    SidebarScrolled(f32),

    CreatePost,
    CreatePage,
    CreateMedia,
    CreateScript,
    CreateTemplate,
    CreateImport,
    /// Per sidebar_views.allium ScriptListItemEntry: row-level delete affordance.
    ScriptDeleteRequested(String),

    // Conversational AI
    ChatCreate,
    ChatRenameInputChanged(String),
    ChatRename,
    ChatDelete(String),
    ChatModelChanged(String),
    ChatInputAction(iced::widget::text_editor::Action),
    ChatSend,
    ChatCancel,
    ChatLinkClicked(String),
    ChatSurfaceFieldChanged {
        surface_id: String,
        field: String,
        value: serde_json::Value,
    },
    ChatSurfaceTextareaAction {
        surface_id: String,
        field: String,
        action: iced::widget::text_editor::Action,
    },
    ChatSurfaceTabSelected {
        surface_id: String,
        index: usize,
    },
    ChatSurfaceDismissed(String),
    ChatSurfaceAction {
        surface_id: String,
        action: String,
        payload: serde_json::Value,
    },
    ChatFinished {
        conversation_id: String,
        result: Result<engine::chat::ChatTurnResult, String>,
    },

    Noop,
    InitMenuBar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SiteGenerationKind {
    Full,
    Validation,
}

#[derive(Debug, Clone)]
struct SiteGenerationWorkflow {
    kind: SiteGenerationKind,
    db_path: PathBuf,
    project_id: String,
    data_dir: PathBuf,
    group_name: String,
    render_task_ids: Vec<TaskId>,
    index_task_id: Option<TaskId>,
    report: engine::generation::GenerationReport,
}

#[derive(Debug, Clone)]
struct ImageDropRequest {
    post_id: String,
    project_id: String,
    data_dir: PathBuf,
    source_language: String,
    offline_mode: bool,
    path: PathBuf,
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
            Err(SqlError::NotFound) => {
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
                Ok(_) | Err(SqlError::NotFound) => {}
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
        if post.status == PostStatus::Published {
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
    creation_pending: bool,
}

fn should_start_embedded_preview_creation(active: bool, pending: bool) -> bool {
    !active && !pending
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

fn replace_current_in_buffer(
    buffer: &mut bds_editor::EditorBuffer,
    query: &str,
    replacement: &str,
) -> bool {
    if query.is_empty() {
        return false;
    }
    if buffer.selected_text() != query && !buffer.find_next(query) {
        return false;
    }
    buffer.insert(replacement);
    let _ = buffer.find_next(query);
    true
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

fn preview_base_url() -> String {
    format!(
        "http://{}:{}",
        engine::preview::PREVIEW_HOST,
        engine::preview::PREVIEW_PORT,
    )
}

fn post_preview_url(post: &Post, language: &str, main_language: &str) -> String {
    let path = bds_core::render::build_canonical_post_path(post, language, main_language);
    format!(
        "{}{path}?draft=true&post_id={}",
        preview_base_url(),
        post.id
    )
}

fn save_template_editor_state_impl(
    db: &Database,
    data_dir: &Path,
    project_id: &str,
    state: &TemplateEditorState,
) -> Result<Template, String> {
    engine::template::validate_template(&state.content)?;
    engine::template::update_template(
        db.conn(),
        data_dir,
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
    data_dir: &Path,
    project_id: &str,
    state: &ScriptEditorState,
) -> Result<Script, String> {
    engine::script::validate_script_syntax(&state.content)?;
    engine::script::update_script(
        db.conn(),
        data_dir,
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
    [
        engine::settings::set(db.conn(), "editor.default_mode", &state.default_mode),
        engine::settings::set(db.conn(), "editor.diff_view_style", &state.diff_view_style),
        engine::settings::set(
            db.conn(),
            "editor.wrap_long_lines",
            if state.wrap_long_lines {
                "true"
            } else {
                "false"
            },
        ),
        engine::settings::set(
            db.conn(),
            "editor.hide_unchanged_regions",
            if state.hide_unchanged_regions {
                "true"
            } else {
                "false"
            },
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
    sidebar_imports: Vec<ImportDefinition>,
    chat_conversations: Vec<ChatConversation>,
    sidebar_media_thumbs: HashMap<String, Option<std::path::PathBuf>>,
    sidebar_posts_has_more: bool,
    sidebar_media_has_more: bool,
    sidebar_posts_loading_more: bool,
    sidebar_media_loading_more: bool,

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
    domain_events: engine::domain_events::EventSubscription,
    chat_events: std::sync::mpsc::Receiver<engine::chat::ChatEvent>,
    script_menu_actions: Arc<Mutex<Vec<MenuAction>>>,
    task_snapshots: Vec<TaskSnapshot>,
    collapsed_task_groups: HashSet<String>,
    output_entries: Vec<OutputEntry>,
    search_index_rebuild_required: bool,
    search_index_rebuild_running: bool,
    search_index_rebuild_task_id: Option<TaskId>,
    site_generation_workflows: HashMap<String, SiteGenerationWorkflow>,
    pending_image_drops: VecDeque<ImageDropRequest>,
    image_drop_import_running: bool,

    // Platform
    _menu_bar: Option<muda::Menu>,
    menu_registry: MenuRegistry,
    native_edit_commands: native_edit::EditCommandQueue,
    #[cfg(target_os = "macos")]
    _lifecycle_handler: Option<objc2::rc::Retained<crate::platform::macos::BdsAppleEventHandler>>,
    #[cfg(target_os = "macos")]
    lifecycle_receiver: Option<std::sync::mpsc::Receiver<crate::platform::macos::LifecycleEvent>>,

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
    remote_client: Option<Arc<bds_server::DesktopClient>>,
    remote_projects: Vec<Project>,
    remote_project: Option<Project>,
    remote_display_name: Option<String>,
    remote_previous_locale: Option<UiLocale>,

    // Local preview
    preview_session: Option<PreviewSession>,
    mcp_server: Option<engine::mcp::McpHttpServer>,
    embedded_preview: Option<EmbeddedPreviewState>,
    embedded_style_preview: Option<EmbeddedPreviewState>,
    main_window_id: Option<window::Id>,

    // Editor states (keyed by entity id)
    post_editors: HashMap<String, PostEditorState>,
    media_editors: HashMap<String, MediaEditorState>,
    template_editors: HashMap<String, TemplateEditorState>,
    script_editors: HashMap<String, ScriptEditorState>,
    import_editors: HashMap<String, ImportEditorState>,
    chat_editors: HashMap<String, ChatEditorState>,
    tags_view_state: Option<TagsViewState>,
    settings_state: Option<SettingsViewState>,
    style_view_state: Option<StyleViewState>,
    dashboard_state: Option<DashboardState>,
    site_validation_state: SiteValidationState,
    duplicates_state: DuplicatesState,
    guide_documentation: DocumentationState,
    api_documentation: DocumentationState,
    cli_documentation: DocumentationState,
    mcp_documentation: DocumentationState,
    metadata_diff_state: MetadataDiffState,
    menu_editor_state: MenuEditorState,
    translation_validation_state: crate::views::translation_validation::TranslationValidationState,
    git_state: GitUiState,
    git_diffs: HashMap<String, GitDiffState>,
    git_file_history: Vec<engine::git::GitCommit>,
    git_file_history_target: Option<String>,
}

// ───────────────────────────────────────────────────────────
// App Implementation
// ───────────────────────────────────────────────────────────

impl BdsApp {
    pub fn new() -> (Self, Task<Message>) {
        let os_locale = detect_os_locale();

        // Open or create the database
        let db_path = bds_core::util::application_database_path();

        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let db = Database::open(&db_path).ok();
        if let Some(ref db) = db {
            let _ = db.migrate();
        }
        let locale = db
            .as_ref()
            .and_then(|db| engine::settings::ui_language(db.conn()).ok().flatten())
            .map(|language| normalize_language(&language))
            .unwrap_or(os_locale);
        let offline_mode = db
            .as_ref()
            .is_none_or(|db| engine::settings::airplane_mode(db.conn()).unwrap_or(true));
        let (menu_bar, registry) = menu::build_menu_bar(locale);
        let search_index_rebuild_required = db
            .as_ref()
            .is_some_and(|db| engine::search::prepare_search_index(db.conn()).unwrap_or(true));
        let mcp_enabled = db.as_ref().is_some_and(|db| {
            engine::settings::get(db.conn(), "mcp.http.enabled")
                .ok()
                .flatten()
                .is_some_and(|value| value == "true")
        });
        let mcp_server = mcp_enabled
            .then(|| {
                engine::mcp::McpHttpServer::start(db_path.clone(), engine::mcp::DEFAULT_HTTP_PORT)
            })
            .and_then(Result::ok);

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
                let default_data = bds_core::util::default_project_data_dir();
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
        registry.set_enabled(MenuAction::DisconnectServer, false);
        let chat_conversations = db
            .as_ref()
            .and_then(|db| engine::chat::list_conversations(db.conn()).ok())
            .unwrap_or_default();
        #[cfg(target_os = "macos")]
        let (lifecycle_handler, lifecycle_receiver) =
            crate::platform::macos::install_lifecycle_handler()
                .map(|(handler, receiver)| (Some(handler), Some(receiver)))
                .unwrap_or((None, None));

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
                sidebar_imports: Vec::new(),
                chat_conversations,
                sidebar_media_thumbs: HashMap::new(),
                sidebar_posts_has_more: false,
                sidebar_media_has_more: false,
                sidebar_posts_loading_more: false,
                sidebar_media_loading_more: false,
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
                domain_events: engine::domain_events::subscribe(),
                chat_events: engine::chat::subscribe_events(),
                script_menu_actions: Arc::new(Mutex::new(Vec::new())),
                task_snapshots: Vec::new(),
                collapsed_task_groups: HashSet::new(),
                output_entries: Vec::new(),
                search_index_rebuild_required,
                search_index_rebuild_running: false,
                search_index_rebuild_task_id: None,
                site_generation_workflows: HashMap::new(),
                pending_image_drops: VecDeque::new(),
                image_drop_import_running: false,
                _menu_bar: Some(menu_bar),
                menu_registry: registry,
                native_edit_commands: native_edit::command_queue(),
                #[cfg(target_os = "macos")]
                _lifecycle_handler: lifecycle_handler,
                #[cfg(target_os = "macos")]
                lifecycle_receiver,
                ui_locale: locale,
                content_language: "en".to_string(),
                blog_languages: Vec::new(),
                offline_mode,
                locale_dropdown_open: false,
                project_dropdown_open: false,
                theme_badge: String::from("pico"),
                toasts: Vec::new(),
                active_modal: search_index_rebuild_required
                    .then_some(modal::ModalState::SearchIndexRepair),
                remote_client: None,
                remote_projects: Vec::new(),
                remote_project: None,
                remote_display_name: None,
                remote_previous_locale: None,
                preview_session: None,
                mcp_server,
                embedded_preview: None,
                embedded_style_preview: None,
                main_window_id: None,
                post_editors: HashMap::new(),
                media_editors: HashMap::new(),
                template_editors: HashMap::new(),
                script_editors: HashMap::new(),
                import_editors: HashMap::new(),
                chat_editors: HashMap::new(),
                tags_view_state: None,
                settings_state: None,
                style_view_state: None,
                dashboard_state: None,
                site_validation_state: SiteValidationState::default(),
                duplicates_state: DuplicatesState::default(),
                guide_documentation: DocumentationState::new(DocumentationKind::Guide),
                api_documentation: DocumentationState::new(DocumentationKind::Api),
                cli_documentation: DocumentationState::new(DocumentationKind::Cli),
                mcp_documentation: DocumentationState::new(DocumentationKind::Mcp),
                metadata_diff_state: MetadataDiffState::default(),
                menu_editor_state: MenuEditorState::default(),
                translation_validation_state: Default::default(),
                git_state: GitUiState::default(),
                git_diffs: HashMap::new(),
                git_file_history: Vec::new(),
                git_file_history_target: None,
            },
            init_task,
        )
    }

    #[cfg(test)]
    fn new_for_tests(db: Database, project: Project, data_dir: PathBuf) -> Self {
        let chat_conversations = engine::chat::list_conversations(db.conn()).unwrap_or_default();
        let offline_mode = engine::settings::airplane_mode(db.conn()).unwrap_or(true);
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
            sidebar_imports: Vec::new(),
            chat_conversations,
            sidebar_media_thumbs: HashMap::new(),
            sidebar_posts_has_more: false,
            sidebar_media_has_more: false,
            sidebar_posts_loading_more: false,
            sidebar_media_loading_more: false,
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
            domain_events: engine::domain_events::subscribe(),
            chat_events: engine::chat::subscribe_events(),
            script_menu_actions: Arc::new(Mutex::new(Vec::new())),
            task_snapshots: Vec::new(),
            collapsed_task_groups: HashSet::new(),
            output_entries: Vec::new(),
            search_index_rebuild_required: false,
            search_index_rebuild_running: false,
            search_index_rebuild_task_id: None,
            site_generation_workflows: HashMap::new(),
            pending_image_drops: VecDeque::new(),
            image_drop_import_running: false,
            _menu_bar: None,
            menu_registry: MenuRegistry::empty(),
            native_edit_commands: native_edit::command_queue(),
            #[cfg(target_os = "macos")]
            _lifecycle_handler: None,
            #[cfg(target_os = "macos")]
            lifecycle_receiver: None,
            ui_locale: UiLocale::En,
            content_language: "en".to_string(),
            blog_languages: Vec::new(),
            offline_mode,
            locale_dropdown_open: false,
            project_dropdown_open: false,
            theme_badge: String::from("pico"),
            toasts: Vec::new(),
            active_modal: None,
            remote_client: None,
            remote_projects: Vec::new(),
            remote_project: None,
            remote_display_name: None,
            remote_previous_locale: None,
            preview_session: None,
            mcp_server: None,
            embedded_preview: None,
            embedded_style_preview: None,
            main_window_id: None,
            post_editors: HashMap::new(),
            media_editors: HashMap::new(),
            template_editors: HashMap::new(),
            script_editors: HashMap::new(),
            import_editors: HashMap::new(),
            chat_editors: HashMap::new(),
            tags_view_state: None,
            settings_state: None,
            style_view_state: None,
            dashboard_state: None,
            site_validation_state: SiteValidationState::default(),
            duplicates_state: DuplicatesState::default(),
            guide_documentation: DocumentationState::new(DocumentationKind::Guide),
            api_documentation: DocumentationState::new(DocumentationKind::Api),
            cli_documentation: DocumentationState::new(DocumentationKind::Cli),
            mcp_documentation: DocumentationState::new(DocumentationKind::Mcp),
            metadata_diff_state: MetadataDiffState::default(),
            menu_editor_state: MenuEditorState::default(),
            translation_validation_state: Default::default(),
            git_state: GitUiState::default(),
            git_diffs: HashMap::new(),
            git_file_history: Vec::new(),
            git_file_history_target: None,
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
                } else if new_view == SidebarView::Git && new_visible {
                    self.refresh_git()
                } else if new_view == SidebarView::Chat && new_visible {
                    self.refresh_chat_conversations();
                    Task::none()
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
            Message::CreateImport => self.create_sidebar_import(),
            Message::ScriptDeleteRequested(script_id) => {
                self.show_script_delete_confirmation(&script_id)
            }
            Message::ChatCreate => self.create_chat_conversation(),
            Message::ChatRenameInputChanged(value) => {
                if let Some(state) = self.active_chat_state_mut() {
                    state.rename_input = value;
                }
                Task::none()
            }
            Message::ChatRename => {
                let Some(id) = self.active_chat_id().map(str::to_string) else {
                    return Task::none();
                };
                let Some(title) = self
                    .chat_editors
                    .get(&id)
                    .map(|state| state.rename_input.clone())
                else {
                    return Task::none();
                };
                let result = self
                    .db
                    .as_ref()
                    .ok_or_else(|| "database unavailable".to_string())
                    .and_then(|db| {
                        engine::chat::rename_conversation(db.conn(), &id, &title)
                            .map_err(|error| error.to_string())
                    });
                match result {
                    Ok(conversation) => {
                        if let Some(state) = self.chat_editors.get_mut(&id) {
                            state.conversation = conversation.clone();
                            state.rename_input = conversation.title.clone();
                        }
                        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
                            tab.title = conversation.title.clone();
                        }
                        self.refresh_chat_conversations();
                    }
                    Err(error) => self.notify(ToastLevel::Error, &error),
                }
                Task::none()
            }
            Message::ChatDelete(id) => {
                let result = self
                    .db
                    .as_ref()
                    .ok_or_else(|| "database unavailable".to_string())
                    .and_then(|db| {
                        engine::chat::delete_conversation(db.conn(), &id)
                            .map_err(|error| error.to_string())
                    });
                match result {
                    Ok(()) => {
                        self.chat_editors.remove(&id);
                        if let Some(next) = tabs::close_tab(&mut self.tabs, &id) {
                            self.active_tab = self.tabs.get(next).map(|tab| tab.id.clone());
                        } else {
                            self.active_tab = None;
                        }
                        self.refresh_chat_conversations();
                        self.notify(ToastLevel::Success, &t(self.ui_locale, "chat.deleted"));
                    }
                    Err(error) => self.notify(ToastLevel::Error, &error),
                }
                Task::none()
            }
            Message::ChatModelChanged(model) => {
                let Some(id) = self.active_chat_id().map(str::to_string) else {
                    return Task::none();
                };
                let result = self
                    .db
                    .as_ref()
                    .ok_or_else(|| "database unavailable".to_string())
                    .and_then(|db| {
                        engine::chat::set_conversation_model(db.conn(), &id, &model)
                            .map_err(|error| error.to_string())
                    });
                match result {
                    Ok(()) => {
                        if let Some(state) = self.chat_editors.get_mut(&id) {
                            state.conversation.model = Some(model);
                        }
                        self.refresh_chat_conversations();
                    }
                    Err(error) => self.notify(ToastLevel::Error, &error),
                }
                Task::none()
            }
            Message::ChatInputAction(action) => {
                if let Some(state) = self.active_chat_state_mut() {
                    state.input.perform(action);
                }
                Task::none()
            }
            Message::ChatSend => self.send_active_chat_message(),
            Message::ChatCancel => {
                if let Some(id) = self.active_chat_id() {
                    engine::chat::cancel_chat(id);
                }
                Task::none()
            }
            Message::ChatLinkClicked(url) => {
                if url.starts_with("https://") || url.starts_with("http://") {
                    if let Err(error) = open::that_detached(&url) {
                        self.notify(ToastLevel::Error, &error.to_string());
                    }
                } else {
                    self.notify(ToastLevel::Error, &t(self.ui_locale, "chat.link.refused"));
                }
                Task::none()
            }
            Message::ChatSurfaceFieldChanged {
                surface_id,
                field,
                value,
            } => {
                if let Some(state) = self.active_chat_state_mut() {
                    state
                        .surface_state
                        .surface_data
                        .entry(surface_id)
                        .or_default()
                        .insert(field, value);
                    state.surface_state_dirty_since = Some(std::time::Instant::now());
                    state.rebuild_surfaces();
                }
                Task::none()
            }
            Message::ChatSurfaceTextareaAction {
                surface_id,
                field,
                action,
            } => {
                if let Some(state) = self.active_chat_state_mut()
                    && let Some(content) = state
                        .surface_textareas
                        .get_mut(&crate::views::chat_view::textarea_key(&surface_id, &field))
                {
                    content.perform(action);
                    let value = content.text();
                    state
                        .surface_state
                        .surface_data
                        .entry(surface_id)
                        .or_default()
                        .insert(field, value.into());
                    state.surface_state_dirty_since = Some(std::time::Instant::now());
                }
                Task::none()
            }
            Message::ChatSurfaceTabSelected { surface_id, index } => {
                let Some(conversation_id) = self.active_chat_id().map(str::to_string) else {
                    return Task::none();
                };
                if let Some(state) = self.chat_editors.get_mut(&conversation_id) {
                    state.surface_state.surface_tabs.insert(surface_id, index);
                    state.rebuild_surfaces();
                }
                self.persist_chat_surface_state(&conversation_id);
                Task::none()
            }
            Message::ChatSurfaceDismissed(surface_id) => {
                let Some(conversation_id) = self.active_chat_id().map(str::to_string) else {
                    return Task::none();
                };
                if let Some(state) = self.chat_editors.get_mut(&conversation_id) {
                    state.surface_state.dismissed_surfaces.insert(surface_id);
                    state.rebuild_surfaces();
                }
                self.persist_chat_surface_state(&conversation_id);
                Task::none()
            }
            Message::ChatSurfaceAction {
                surface_id,
                action,
                payload,
            } => {
                let result = self
                    .active_chat_id()
                    .and_then(|id| self.chat_editors.get(id))
                    .map(|state| {
                        let payload = engine::chat_surfaces::merge_form_data(
                            payload,
                            &surface_id,
                            &state.surface_state,
                        );
                        engine::chat_surfaces::resolve_surface_action(&action, &payload)
                    });
                match result {
                    Some(Ok(navigation)) => self.dispatch_chat_navigation(
                        &navigation.destination,
                        navigation.entity_id.as_deref(),
                    ),
                    Some(Err(reason)) => {
                        let message = tw(
                            self.ui_locale,
                            "chat.surface.actionRefused",
                            &[("reason", &reason)],
                        );
                        if let Some(state) = self.active_chat_state_mut() {
                            state.error = Some(message.clone());
                        }
                        self.notify(ToastLevel::Error, &message);
                    }
                    None => {}
                }
                Task::none()
            }
            Message::ChatFinished {
                conversation_id,
                result,
            } => {
                let locale = self.ui_locale;
                if let Some(state) = self.chat_editors.get_mut(&conversation_id) {
                    state.streaming = false;
                    state.clear_streaming();
                    state.active_tool = None;
                    match result {
                        Ok(_) => state.error = None,
                        Err(error) => state.error = Some(localize_chat_error(locale, &error)),
                    }
                    if let Some(db) = &self.db {
                        state.set_messages(
                            engine::chat::list_messages(db.conn(), &conversation_id)
                                .unwrap_or_default(),
                        );
                        if let Ok(conversation) =
                            engine::chat::get_conversation(db.conn(), &conversation_id)
                        {
                            state.conversation = conversation;
                        }
                    }
                }
                self.refresh_chat_conversations();
                self.refresh_counts()
            }
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
            Message::OpenTab(tab) => self.open_tab(tab),
            Message::CloseTab(id) => {
                if self.active_tab.as_deref() == Some(id.as_str()) {
                    self.flush_active_post_editor();
                }
                if let Some(next_idx) = tabs::close_tab(&mut self.tabs, &id) {
                    self.active_tab = self.tabs.get(next_idx).map(|t| t.id.clone());
                } else {
                    self.active_tab = None;
                }
                self.git_diffs.remove(&id);
                self.enforce_panel_tab_fallback();
                self.sync_menu_state();
                self.sync_embedded_previews()
            }
            Message::SelectTab(id) => {
                if self.tabs.iter().any(|t| t.id == id) {
                    if self.active_tab.as_deref() != Some(id.as_str()) {
                        self.flush_active_post_editor();
                    }
                    self.active_tab = Some(id.clone());
                }
                self.enforce_panel_tab_fallback();
                let semantic_task = self
                    .tabs
                    .iter()
                    .find(|tab| tab.id == id && tab.tab_type == TabType::Post)
                    .map_or_else(Task::none, |_| {
                        Task::done(Message::LoadSemanticTagSuggestions(id))
                    });
                Task::batch([self.sync_embedded_previews(), semantic_task])
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
                    if let (Some(db), Some(project)) = (&self.db, &self.active_project)
                        && let Err(e) = engine::meta::initialize_metadata_snapshots(
                            db.conn(),
                            &data_dir,
                            &project.id,
                        )
                    {
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
                let documentation_task = self.reload_changed_documentation();
                let menu_editor_task = if self
                    .tabs
                    .iter()
                    .any(|tab| tab.tab_type == TabType::MenuEditor)
                {
                    Task::done(Message::MenuEditor(MenuEditorMsg::Reload))
                } else {
                    self.menu_editor_state = MenuEditorState::default();
                    Task::none()
                };
                self.sync_menu_state();
                Task::batch([
                    sidebar_task,
                    Task::done(Message::EmbeddingBackfill),
                    documentation_task,
                    menu_editor_task,
                ])
            }
            Message::SwitchProject(project_id) => {
                self.project_dropdown_open = false;
                if let Some(ref db) = self.db {
                    if let (Some(outgoing), Some(data_dir)) = (&self.active_project, &self.data_dir)
                    {
                        let _ =
                            engine::embedding::EmbeddingService::production(db.conn(), data_dir)
                                .flush_project(&outgoing.id);
                    }
                    match engine::project::set_active_project(db.conn(), &project_id) {
                        Ok(()) => {
                            if let Some(project) = self
                                .projects
                                .iter()
                                .find(|project| project.id == project_id)
                                && let Some(data_dir) =
                                    project.data_path.as_deref().map(PathBuf::from)
                            {
                                let _ = engine::meta::startup_sync(&data_dir);
                                let _ = engine::meta::initialize_metadata_snapshots(
                                    db.conn(),
                                    &data_dir,
                                    &project.id,
                                );
                            }
                            self.reset_git_for_project_change();
                            self.active_project =
                                self.projects.iter().find(|p| p.id == project_id).cloned();
                            self.preview_session = None;
                            self.duplicates_state = DuplicatesState::default();
                            self.hide_embedded_preview();
                            self.hide_embedded_style_preview();
                            self.style_view_state = None;
                            self.data_dir = self
                                .active_project
                                .as_ref()
                                .and_then(|p| p.data_path.as_ref())
                                .map(PathBuf::from);
                            // Per metadata.allium StartupSync
                            if let Some(data_dir) = self.data_dir.clone() {
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
                            if self.tabs.iter().any(|tab| tab.tab_type == TabType::Style) {
                                self.style_view_state = self.hydrate_style_state();
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
                Task::batch([
                    self.refresh_counts(),
                    self.refresh_git_if_visible(),
                    Task::done(Message::EmbeddingBackfill),
                    self.sync_embedded_previews(),
                ])
            }
            Message::RequestCreateProject => {
                crate::platform::dialog::pick_folder(t(self.ui_locale, "dialog.selectFolder"))
            }
            Message::RequestOpenProject => crate::platform::dialog::pick_project_folder(t(
                self.ui_locale,
                "dialog.openProject",
            )),
            Message::CreateProject { name, data_path } => {
                if let Some(ref db) = self.db {
                    let path_str = data_path.as_ref().map(|p| p.to_string_lossy().to_string());
                    match engine::project::create_project(db.conn(), &name, path_str.as_deref()) {
                        Ok(project) => {
                            let _ = engine::project::set_active_project(db.conn(), &project.id);
                            self.projects =
                                engine::project::list_projects(db.conn()).unwrap_or_default();
                            self.reset_git_for_project_change();
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
                self.refresh_git_if_visible()
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
            Message::ProjectFolderPicked(path) => {
                if let (Some(path), Some(db)) = (path, self.db.as_ref()) {
                    match engine::project::open_project(db.conn(), &path) {
                        Ok(project) => {
                            let _ = engine::project::set_active_project(db.conn(), &project.id);
                            self.projects =
                                engine::project::list_projects(db.conn()).unwrap_or_default();
                            self.reset_git_for_project_change();
                            self.active_project = Some(project.clone());
                            self.data_dir = project.data_path.as_ref().map(PathBuf::from);
                            self.preview_session = None;
                            self.hide_embedded_preview();
                            self.hide_embedded_style_preview();
                            self.style_view_state = None;
                            self.notify(
                                ToastLevel::Success,
                                &tw(
                                    self.ui_locale,
                                    "projectSelector.toast.opened",
                                    &[("name", &project.name)],
                                ),
                            );
                            self.sync_menu_state();
                            return Task::batch([
                                self.refresh_counts(),
                                self.refresh_git_if_visible(),
                            ]);
                        }
                        Err(error) => self.notify(
                            ToastLevel::Error,
                            &tw(
                                self.ui_locale,
                                "projectSelector.toast.openFailed",
                                &[("error", &error.to_string())],
                            ),
                        ),
                    }
                }
                Task::none()
            }
            Message::FileDropped(path) => self.enqueue_image_drop(path),
            Message::ImageDropImported {
                task_id,
                post_id,
                project_id,
                data_dir,
                source_language,
                offline_mode,
                path,
                result,
            } => self.finish_image_drop_import(
                task_id,
                ImageDropRequest {
                    post_id,
                    project_id,
                    data_dir,
                    source_language,
                    offline_mode,
                    path,
                },
                result,
            ),
            Message::ImageDropEnriched {
                task_id,
                post_id,
                path,
                result,
            } => self.finish_image_drop_enrichment(task_id, &post_id, &path, result),
            Message::MediaFilesPicked(paths) => {
                let (Some(paths), Some(project), Some(data_dir)) =
                    (paths, self.active_project.as_ref(), self.data_dir.as_ref())
                else {
                    return Task::none();
                };
                let db_path = self.db_path.clone();
                let project_id = project.id.clone();
                let data_dir = data_dir.clone();
                let language = self.content_language.clone();
                let concurrency = engine::meta::read_project_json(&data_dir)
                    .map(|metadata| metadata.image_import_concurrency.clamp(1, 8) as usize)
                    .unwrap_or(4);
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let results = match engine::gallery_import::process_paths_concurrently(
                                paths,
                                concurrency,
                                |_index, path| {
                                    let original_name = path
                                        .file_name()
                                        .map(|name| name.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "image".to_string());
                                    let db = Database::open(&db_path)
                                        .map_err(|error| format!("{}: {error}", path.display()))?;
                                    engine::media::import_media(
                                        db.conn(),
                                        &data_dir,
                                        &project_id,
                                        &path,
                                        &original_name,
                                        None,
                                        None,
                                        None,
                                        None,
                                        Some(&language),
                                        Vec::new(),
                                    )
                                    .map_err(|error| format!("{}: {error}", path.display()))
                                },
                            ) {
                                Ok(results) => results,
                                Err(error) => return (Vec::new(), vec![error]),
                            };
                            let mut imported = Vec::new();
                            let mut errors = Vec::new();
                            for result in results {
                                match result {
                                    Ok(media) => imported.push(media),
                                    Err(error) => errors.push(error),
                                }
                            }
                            (imported, errors)
                        })
                        .await
                        .unwrap_or_else(|error| (Vec::new(), vec![error.to_string()]))
                    },
                    |(imported, errors)| Message::MediaImportFinished { imported, errors },
                )
            }
            Message::GalleryImagesPicked { post_id, result } => match result {
                Ok(Some(paths)) if !paths.is_empty() => {
                    let (Some(project), Some(data_dir)) =
                        (self.active_project.as_ref(), self.data_dir.as_ref())
                    else {
                        return Task::none();
                    };
                    let db_path = self.db_path.clone();
                    let data_dir = data_dir.clone();
                    let project_id = project.id.clone();
                    let source_language = self.content_language.clone();
                    let offline_mode = self.offline_mode;
                    let result_post_id = post_id.clone();
                    let failed_paths = paths.clone();
                    let selected_count = paths.len();
                    Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                engine::gallery_import::import_gallery_images(
                                    &db_path,
                                    &data_dir,
                                    &project_id,
                                    &post_id,
                                    paths,
                                    &source_language,
                                    offline_mode,
                                )
                            })
                            .await
                            .unwrap_or_else(|error| {
                                engine::gallery_import::GalleryImportReport {
                                    selected_count,
                                    outcomes: failed_paths
                                        .into_iter()
                                        .map(|path| engine::gallery_import::GalleryImportOutcome {
                                            path,
                                            result: Err(error.to_string()),
                                        })
                                        .collect(),
                                }
                            })
                        },
                        move |report| Message::GalleryImportFinished {
                            post_id: result_post_id.clone(),
                            report,
                        },
                    )
                }
                Ok(_) => Task::none(),
                Err(error) => {
                    self.add_output(&tw(
                        self.ui_locale,
                        "editor.galleryPickerFailed",
                        &[("error", &error)],
                    ));
                    Task::none()
                }
            },
            Message::GalleryImportFinished { post_id, report } => {
                for outcome in report.outcomes {
                    match outcome.result {
                        Ok(image) => self.add_output(&tw(
                            self.ui_locale,
                            "editor.galleryImageAdded",
                            &[("title", &image.title)],
                        )),
                        Err(error) => {
                            let path = outcome
                                .path
                                .file_name()
                                .map(|name| name.to_string_lossy().to_string())
                                .unwrap_or_else(|| outcome.path.display().to_string());
                            self.add_output(&tw(
                                self.ui_locale,
                                "editor.galleryImageFailed",
                                &[("path", &path), ("error", &error)],
                            ));
                        }
                    }
                }

                if let Some(state) = self.post_editors.get_mut(&post_id) {
                    state.insert_markdown_at_cursor("\n[[gallery]]\n");
                    if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == post_id) {
                        tab.is_dirty = true;
                    }
                    if let Err(error) = self.persist_post_editor_state(&post_id) {
                        self.notify_operation_failed("common.save", error);
                    }
                    self.refresh_post_relationships(&post_id);
                }
                self.add_output(&tw(
                    self.ui_locale,
                    "editor.galleryImportComplete",
                    &[("count", &report.selected_count.to_string())],
                ));
                self.refresh_sidebar_media()
            }
            Message::MediaImportFinished { imported, errors } => {
                if !imported.is_empty() {
                    self.sidebar_view = SidebarView::Media;
                    self.notify(
                        ToastLevel::Success,
                        &tw(
                            self.ui_locale,
                            "media.toast.imported",
                            &[("count", &imported.len().to_string())],
                        ),
                    );
                }
                if !errors.is_empty() {
                    self.notify(
                        ToastLevel::Error,
                        &tw(
                            self.ui_locale,
                            "media.toast.importFailed",
                            &[("error", &errors.join("; "))],
                        ),
                    );
                }
                self.refresh_sidebar_media()
            }
            Message::MediaReplacementPicked { media_id, path } => {
                let (Some(path), Some(data_dir)) = (path, self.data_dir.as_ref()) else {
                    return Task::none();
                };
                let db_path = self.db_path.clone();
                let data_dir = data_dir.clone();
                let result_media_id = media_id.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                            engine::media::replace_media_file(
                                db.conn(),
                                &data_dir,
                                &media_id,
                                &path,
                            )
                            .map_err(|error| error.to_string())
                        })
                        .await
                        .map_err(|error| error.to_string())?
                    },
                    move |result| Message::MediaReplacementFinished {
                        media_id: result_media_id.clone(),
                        result,
                    },
                )
            }
            Message::MediaReplacementFinished { media_id, result } => {
                match result {
                    Ok(Some(media)) => {
                        if let Some(state) = self.media_editors.get_mut(&media_id) {
                            state.size = media.size;
                            state.width = media.width;
                            state.height = media.height;
                            state.updated_at = media.updated_at;
                        }
                        self.notify(
                            ToastLevel::Success,
                            &t(self.ui_locale, "media.toast.replaced"),
                        );
                    }
                    Ok(None) => self.notify(
                        ToastLevel::Info,
                        &t(self.ui_locale, "media.toast.unchanged"),
                    ),
                    Err(error) => self.notify(
                        ToastLevel::Error,
                        &tw(
                            self.ui_locale,
                            "media.toast.replaceFailed",
                            &[("error", &error)],
                        ),
                    ),
                }
                self.refresh_sidebar_media()
            }
            Message::ImportUploadsPicked {
                definition_id,
                path,
            } => {
                if let Some(path) = path {
                    self.update_import_paths(&definition_id, None, Some(path.as_path()));
                }
                Task::none()
            }
            Message::ImportWxrPicked {
                definition_id,
                path,
            } => {
                if let Some(path) = path {
                    self.update_import_paths(&definition_id, Some(path.as_path()), None);
                    return self.start_import_analysis(&definition_id);
                }
                Task::none()
            }
            Message::ImportAnalysisEvent {
                definition_id,
                event,
            } => {
                if let Some(state) = self.import_editors.get_mut(&definition_id) {
                    match event {
                        ImportAnalysisEvent::Progress(progress) => state.progress = Some(progress),
                        ImportAnalysisEvent::Finished(result) => {
                            state.is_analyzing = false;
                            match *result {
                                Ok((definition, report)) => {
                                    state.definition = definition.clone();
                                    state.report = Some(report);
                                    state.error = None;
                                    if let Some(sidebar) = self
                                        .sidebar_imports
                                        .iter_mut()
                                        .find(|item| item.id == definition_id)
                                    {
                                        *sidebar = definition;
                                    }
                                }
                                Err(error) => state.error = Some(error),
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::ImportExecutionEvent {
                definition_id,
                event,
            } => {
                if let Some(state) = self.import_editors.get_mut(&definition_id) {
                    match event {
                        ImportExecutionEvent::Progress(progress) => state.progress = Some(progress),
                        ImportExecutionEvent::Finished(result) => {
                            state.is_executing = false;
                            match result {
                                Ok(result) => {
                                    state.result = Some(result);
                                    state.error = None;
                                    self.notify(
                                        ToastLevel::Success,
                                        &t(self.ui_locale, "import.toast.complete"),
                                    );
                                    return self.refresh_counts();
                                }
                                Err(error) => state.error = Some(error),
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::ImportAutoMapFinished {
                definition_id,
                result,
            } => {
                if let Some(state) = self.import_editors.get_mut(&definition_id) {
                    state.is_analyzing = false;
                    match result {
                        Ok((definition, report, count)) => {
                            state.definition = definition;
                            state.report = Some(report);
                            state.error = None;
                            self.notify(
                                ToastLevel::Success,
                                &tw(
                                    self.ui_locale,
                                    "import.toast.mapped",
                                    &[("count", &count.to_string())],
                                ),
                            );
                        }
                        Err(error) => {
                            state.error = Some(error.clone());
                            self.notify(ToastLevel::Error, &error);
                        }
                    }
                }
                Task::none()
            }
            Message::DuplicatesRefresh => self.start_duplicate_search(0),
            Message::DuplicatesShowMore => {
                let next = self.duplicates_state.page.saturating_add(1);
                self.start_duplicate_search(next)
            }
            Message::DuplicatesLoaded(result) => {
                self.duplicates_state.is_loading = false;
                self.duplicates_state.has_run = true;
                match result {
                    Ok(result) => {
                        self.duplicates_state.result = result;
                        self.duplicates_state.error = None;
                        self.duplicates_state.selected.retain(|pair| {
                            self.duplicates_state.result.pairs.iter().any(|candidate| {
                                candidate.post_id_a == pair.0 && candidate.post_id_b == pair.1
                            })
                        });
                    }
                    Err(error) => self.duplicates_state.error = Some(error),
                }
                Task::none()
            }
            Message::DuplicatesToggle(a, b) => {
                if !self
                    .duplicates_state
                    .selected
                    .remove(&(a.clone(), b.clone()))
                {
                    self.duplicates_state.selected.insert((a, b));
                }
                Task::none()
            }
            Message::DuplicatesCheckAll => {
                self.duplicates_state.selected = self
                    .duplicates_state
                    .result
                    .pairs
                    .iter()
                    .map(|pair| (pair.post_id_a.clone(), pair.post_id_b.clone()))
                    .collect();
                Task::none()
            }
            Message::DuplicatesUncheckAll => {
                self.duplicates_state.selected.clear();
                Task::none()
            }
            Message::DuplicatesDismiss(a, b) => self.dismiss_duplicate_pairs(vec![(a, b)]),
            Message::DuplicatesDismissSelected => self
                .dismiss_duplicate_pairs(self.duplicates_state.selected.iter().cloned().collect()),
            Message::DuplicatesDismissed(result) => {
                match result {
                    Ok(()) => {
                        self.duplicates_state.selected.clear();
                        self.notify(
                            ToastLevel::Success,
                            &t(self.ui_locale, "duplicates.dismissed"),
                        );
                        return self.start_duplicate_search(self.duplicates_state.page);
                    }
                    Err(error) => self.notify(ToastLevel::Error, &error),
                }
                Task::none()
            }
            Message::DuplicatesOpenPost(post_id) => {
                let Some(db) = &self.db else {
                    return Task::none();
                };
                let Ok(post) = bds_core::db::queries::post::get_post_by_id(db.conn(), &post_id)
                else {
                    return Task::none();
                };
                Task::done(Message::OpenTab(Tab {
                    id: post.id,
                    tab_type: TabType::Post,
                    title: post.title,
                    is_transient: false,
                    is_dirty: false,
                }))
            }
            Message::DocumentationRefresh(kind) => self.start_documentation_load(kind),
            Message::DocumentationLoaded(kind, generation, load) => {
                let state = self.documentation_state_mut(kind);
                if state.load_generation == generation {
                    state.apply(load);
                }
                Task::none()
            }
            Message::DocumentationLinkClicked(kind, url) => {
                if url == crate::views::documentation::API_DOCUMENTATION_URL {
                    self.open_singleton_tab(TabType::ApiDocumentation, "tabBar.apiDocumentation");
                    return self.start_documentation_load(DocumentationKind::Api);
                } else if let Some(anchor) = url.strip_prefix("https://ruds.invalid/document#") {
                    if let Some(offset) = self.documentation_state(kind).parsed.anchors.get(anchor)
                    {
                        return iced::widget::scrollable::snap_to(
                            crate::views::documentation::scroll_id(kind),
                            iced::widget::scrollable::RelativeOffset { x: 0.0, y: *offset },
                        );
                    }
                    self.notify(
                        ToastLevel::Error,
                        &t(self.ui_locale, "documentation.anchorMissing"),
                    );
                } else if !self.confirm_external_link(url) {
                    self.notify(
                        ToastLevel::Error,
                        &t(self.ui_locale, "documentation.linkRefused"),
                    );
                }
                Task::none()
            }
            Message::EmbeddingReindex => self.start_embedding_reindex(),
            Message::EmbeddingBackfill => self.start_embedding_backfill(),
            Message::LoadSemanticTagSuggestions(post_id) => {
                let Some(data_dir) = self.data_dir.clone() else {
                    return Task::none();
                };
                let db_path = self.db_path.clone();
                let returned_post_id = post_id.clone();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                            engine::embedding::EmbeddingService::production(db.conn(), &data_dir)
                                .suggest_tags(&post_id)
                                .map_err(|error| error.to_string())
                        })
                        .await
                        .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
                    },
                    move |result| Message::SemanticTagSuggestionsLoaded {
                        post_id: returned_post_id.clone(),
                        result,
                    },
                )
            }
            Message::SemanticTagSuggestionsLoaded { post_id, result } => {
                if let (Some(state), Ok(suggestions)) =
                    (self.post_editors.get_mut(&post_id), result)
                {
                    state.semantic_tag_suggestions = suggestions;
                }
                Task::none()
            }
            message @ (Message::MainWindowLoaded(_)
            | Message::EmbeddedPreviewReady(_)
            | Message::EmbeddedStylePreviewReady(_)) => self.handle_preview_message(message),

            // ── Tasks ──
            Message::TaskTick => {
                self.task_manager.evict_expired();
                self.refresh_task_snapshots();
                self.process_chat_events();
                self.persist_due_chat_surface_state();
                let _ = engine::embedding::EmbeddingService::flush_due();
                if !self.search_index_rebuild_running {
                    self.auto_save_due_post_editors();
                }
                let actions = std::mem::take(&mut *self.script_menu_actions.lock().unwrap());
                let mut tasks = actions
                    .into_iter()
                    .map(|action| self.dispatch_menu_action(action))
                    .collect::<Vec<_>>();
                #[cfg(target_os = "macos")]
                if let Some(receiver) = &self.lifecycle_receiver {
                    tasks.push(
                        crate::platform::macos::drain_lifecycle(receiver)
                            .into_iter()
                            .map(Task::done)
                            .fold(Task::none(), Task::chain),
                    );
                }
                tasks.push(self.reload_changed_documentation());
                Task::batch(tasks)
            }
            Message::DomainEventsTick => self.process_domain_events(),
            Message::CancelTask(task_id) => {
                if self.cancel_site_generation_task(task_id) {
                    return Task::none();
                }
                self.task_manager.cancel(task_id);
                self.refresh_task_snapshots();
                Task::none()
            }
            Message::ToggleTaskGroup(group_id) => {
                if !self.collapsed_task_groups.remove(&group_id) {
                    self.collapsed_task_groups.insert(group_id);
                }
                Task::none()
            }

            // ── macOS lifecycle ──
            Message::FileOpenRequested(path) => {
                let folder = if path.is_dir() {
                    Some(path)
                } else if path.file_name().is_some_and(|name| name == "project.json")
                    && path.parent().is_some_and(|parent| parent.ends_with("meta"))
                {
                    path.parent().and_then(Path::parent).map(PathBuf::from)
                } else {
                    None
                };
                folder.map_or_else(Task::none, |path| {
                    Task::done(Message::ProjectFolderPicked(Some(path)))
                })
            }
            Message::UrlOpenRequested(url) => {
                let candidate = match engine::blogmark::parse_deep_link(&url) {
                    Ok(candidate) => candidate,
                    Err(error) => {
                        self.notify(
                            ToastLevel::Error,
                            &tw(
                                self.ui_locale,
                                "blogmark.invalid",
                                &[("error", &error.to_string())],
                            ),
                        );
                        return Task::none();
                    }
                };
                let target = if let Some(project_id) = candidate.project_id.as_deref() {
                    let Some(project) = self
                        .projects
                        .iter()
                        .find(|project| project.id == project_id)
                        .cloned()
                    else {
                        self.notify(
                            ToastLevel::Error,
                            &t(self.ui_locale, "blogmark.unknownProject"),
                        );
                        return Task::none();
                    };
                    project
                } else if let Some(project) = self.active_project.clone() {
                    project
                } else {
                    self.notify(
                        ToastLevel::Warning,
                        &t(self.ui_locale, "blogmark.noProject"),
                    );
                    return Task::none();
                };
                let Some(data_dir) = target.data_path.as_ref().map(PathBuf::from) else {
                    self.notify(ToastLevel::Error, &t(self.ui_locale, "blogmark.noProject"));
                    return Task::none();
                };
                if self.active_project.as_ref().map(|project| &project.id) != Some(&target.id) {
                    if let Some(db) = self.db.as_ref()
                        && let Err(error) =
                            engine::project::set_active_project(db.conn(), &target.id)
                    {
                        self.notify(ToastLevel::Error, &error.to_string());
                        return Task::none();
                    }
                    self.active_project = Some(target.clone());
                    self.data_dir = Some(data_dir.clone());
                    self.preview_session = None;
                    self.hide_embedded_preview();
                    self.hide_embedded_style_preview();
                    self.style_view_state = None;
                    if let Ok(meta) = engine::meta::read_project_json(&data_dir) {
                        let main_language = meta.main_language.unwrap_or_else(|| "en".into());
                        self.content_language = main_language.clone();
                        self.blog_languages = meta.blog_languages;
                        if !self.blog_languages.contains(&main_language) {
                            self.blog_languages.insert(0, main_language);
                        }
                    }
                    self.sync_menu_state();
                }
                let db_path = self.db_path.clone();
                let project_id = target.id;
                let task_manager = Arc::clone(&self.task_manager);
                let label = t(self.ui_locale, "blogmark.importing");
                let task_id = task_manager.submit(&label);
                let offline_mode = self.offline_mode;
                let app_handler = crate::platform::script_host::handler(
                    Arc::clone(&self.script_menu_actions),
                    t(self.ui_locale, "dialog.selectFolder"),
                );
                self.refresh_task_snapshots();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            if !task_manager.wait_until_runnable(task_id) {
                                return Err("cancelled".to_string());
                            }
                            let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                            let host =
                                bds_core::scripting::CoreHost::new(db_path, &project_id, &data_dir)
                                    .with_task(Arc::clone(&task_manager), task_id)
                                    .with_offline_mode(offline_mode)
                                    .with_app_handler(app_handler);
                            let control = task_manager
                                .cancellation_flag(task_id)
                                .map(bds_core::scripting::ExecutionControl::from_cancelled)
                                .unwrap_or_default();
                            engine::blogmark::receive_deep_link_with_host(
                                db.conn(),
                                &data_dir,
                                &project_id,
                                &url,
                                &control,
                                Arc::new(host),
                            )
                            .map_err(|error| error.to_string())
                        })
                        .await
                        .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
                    },
                    move |result| Message::BlogmarkImported { task_id, result },
                )
            }
            Message::BlogmarkImported { task_id, result } => match result {
                Ok(result) => {
                    self.task_manager.complete(task_id);
                    self.refresh_task_snapshots();
                    for message in result.toasts {
                        self.notify(ToastLevel::Info, &message);
                    }
                    for error in result.transform_errors {
                        self.add_output(&error);
                    }
                    self.sidebar_view = SidebarView::Posts;
                    self.sidebar_visible = true;
                    let tab = Tab {
                        id: result.post.id.clone(),
                        tab_type: TabType::Post,
                        title: result.post.title.clone(),
                        is_transient: false,
                        is_dirty: false,
                    };
                    let open_editor = self.open_tab(tab);
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "blogmark.imported"));
                    Task::batch([open_editor, self.refresh_counts()])
                }
                Err(error) => {
                    if self.task_manager.status(task_id) != Some(TaskStatus::Cancelled) {
                        self.task_manager.fail(task_id, error.clone());
                    }
                    self.refresh_task_snapshots();
                    self.notify(
                        ToastLevel::Error,
                        &tw(self.ui_locale, "blogmark.failed", &[("error", &error)]),
                    );
                    Task::none()
                }
            },

            // ── Panel ──
            Message::SetPanelTab(tab) => {
                self.panel_tab = tab;
                if tab == PanelTab::GitLog {
                    self.refresh_git_file_history()
                } else {
                    Task::none()
                }
            }

            // ── Settings ──
            Message::SetOfflineMode(mode) => {
                if let Some(db) = &self.db
                    && let Err(error) = engine::settings::set_airplane_mode(db.conn(), mode)
                {
                    let operation = t(self.ui_locale, "settings.offlineMode");
                    let error = error.to_string();
                    self.notify(
                        ToastLevel::Error,
                        &tw(
                            self.ui_locale,
                            "common.operationFailed",
                            &[("operation", &operation), ("error", &error)],
                        ),
                    );
                    return Task::none();
                }
                self.offline_mode = mode;
                let models = self.chat_model_options();
                let selected = self.db.as_ref().and_then(|db| {
                    ai::load_ai_settings(db.conn(), mode)
                        .ok()
                        .map(|settings| settings.active().endpoint.model.clone())
                        .filter(|model| !model.trim().is_empty())
                });
                for (id, state) in &mut self.chat_editors {
                    state.model_options = models.clone();
                    if let Some(model) = selected.as_ref() {
                        state.conversation.model = Some(model.clone());
                        if let Some(db) = &self.db {
                            let _ = engine::chat::set_conversation_model(db.conn(), id, model);
                        }
                    }
                }
                self.sync_menu_state();
                Task::none()
            }
            Message::SetUiLocale(locale) => {
                if let Some(db) = &self.db {
                    match engine::settings::set(
                        db.conn(),
                        engine::settings::UI_LANGUAGE_KEY,
                        locale.code(),
                    ) {
                        Ok(()) => self.apply_ui_locale(locale),
                        Err(error) => {
                            self.add_output(&error.to_string());
                        }
                    }
                } else {
                    self.apply_ui_locale(locale);
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
            message @ (Message::RebuildDatabase
            | Message::ReindexText
            | Message::RegenerateCalendar
            | Message::ValidateTranslations
            | Message::TranslationValidationLoaded(_)
            | Message::ValidateMedia
            | Message::GenerateSite
            | Message::RunMetadataDiff
            | Message::MetadataDiffLoaded(_)
            | Message::RepairMetadataDiffItem { .. }
            | Message::MetadataDiffItemRepaired(_)
            | Message::RunSiteValidation
            | Message::ApplySiteValidation
            | Message::EngineTaskDone { .. }
            | Message::SiteGenerationSectionDone { .. }
            | Message::SiteGenerationIndexDone { .. }
            | Message::SiteValidationLoaded(_)) => self.handle_engine_message(message),

            // ── Git ──
            message @ (Message::GitRefresh
            | Message::GitLoaded { .. }
            | Message::GitRemoteInputChanged(_)
            | Message::GitCommitMessageChanged(_)
            | Message::GitInitialize
            | Message::GitSetRemote
            | Message::GitCommit
            | Message::GitFetch
            | Message::GitPull
            | Message::GitPush
            | Message::GitPruneLfs
            | Message::GitLocalFinished { .. }
            | Message::GitNetworkFinished { .. }
            | Message::OpenGitFileDiff(_)
            | Message::OpenGitCommitDiff { .. }
            | Message::SelectGitCommitFile { .. }
            | Message::GitDiffLoaded { .. }
            | Message::GitFileHistoryLoaded { .. }) => self.handle_git_message(message),

            // ── Toasts ──
            Message::DismissToast(id) => {
                self.toasts.retain(|toast| toast.id != id);
                Task::none()
            }
            Message::ExpireToasts => {
                self.toasts.retain(|t| !t.is_expired());
                Task::none()
            }

            // ── Sidebar filters ──
            message @ (Message::PostSearchChanged(_)
            | Message::TogglePostFilterPanel
            | Message::SetPostStatusFilter(_)
            | Message::SetPostLanguageFilter(_)
            | Message::SetPostCalendarYear(_)
            | Message::SetPostCalendarMonth(_)
            | Message::SetPostFromDate(_)
            | Message::SetPostToDate(_)
            | Message::TogglePostTagFilter(_)
            | Message::TogglePostCategoryFilter(_)
            | Message::ClearPostFilters
            | Message::MediaSearchChanged(_)
            | Message::ToggleMediaFilterPanel
            | Message::SetMediaCalendarYear(_)
            | Message::SetMediaCalendarMonth(_)
            | Message::ToggleMediaTagFilter(_)
            | Message::ClearMediaFilters
            | Message::SidebarPostsLoaded(_)
            | Message::SidebarMediaLoaded { .. }
            | Message::SidebarPostsAppended(_)
            | Message::SidebarMediaAppended { .. }
            | Message::SidebarScrolled(_)) => self.handle_sidebar_message(message),

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
                    modal::ConfirmAction::RebuildSearchIndex => self.start_search_index_rebuild(),
                    modal::ConfirmAction::OpenExternalUrl(url) => {
                        if (url.starts_with("https://") || url.starts_with("http://"))
                            && let Err(error) = open::that_detached(&url)
                        {
                            self.notify(ToastLevel::Error, &error.to_string());
                        }
                        Task::none()
                    }
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
                    modal::ConfirmAction::DeleteImport(id) => self.delete_import_definition(&id),
                    modal::ConfirmAction::MergeTags { sources, target } => {
                        self.merge_tags(&sources, &target)
                    }
                }
            }
            Message::RemoteTargetChanged(value) => {
                if let Some(modal::ModalState::RemoteConnection { target, error, .. }) =
                    self.active_modal.as_mut()
                {
                    *target = value;
                    *error = None;
                }
                Task::none()
            }
            Message::RemoteConnectRequested => {
                let target = match self.active_modal.as_mut() {
                    Some(modal::ModalState::RemoteConnection {
                        target,
                        connecting,
                        error,
                        ..
                    }) => {
                        *connecting = true;
                        *error = None;
                        target.clone()
                    }
                    _ => return Task::none(),
                };
                let parsed = match bds_server::RemoteTarget::parse(&target) {
                    Ok(target) => target,
                    Err(error) => {
                        if let Some(modal::ModalState::RemoteConnection {
                            connecting,
                            error: shown,
                            ..
                        }) = self.active_modal.as_mut()
                        {
                            *connecting = false;
                            let _ = error;
                            *shown = Some(t(self.ui_locale, "remoteConnection.invalidTarget"));
                        }
                        return Task::none();
                    }
                };
                let data_root = bds_core::util::application_data_dir();
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            let client = Arc::new(
                                bds_server::DesktopClient::connect(parsed, &data_root)
                                    .map_err(|error| error.to_string())?,
                            );
                            let projects =
                                client.list_projects().map_err(|error| error.to_string())?;
                            Ok((client, projects))
                        })
                        .await
                        .unwrap_or_else(|error| {
                            Err(format!("remote connection task failed: {error}"))
                        })
                    },
                    Message::RemoteConnected,
                )
            }
            Message::RemoteConnected(result) => {
                match result {
                    Ok((client, projects)) => {
                        let server_locale =
                            bds_core::i18n::normalize_language(client.server_locale());
                        self.remote_projects.clone_from(&projects);
                        self.remote_client = Some(client);
                        if let Some(modal::ModalState::RemoteConnection {
                            connecting,
                            connected,
                            error,
                            projects: shown_projects,
                            selected_project_id,
                            ..
                        }) = self.active_modal.as_mut()
                        {
                            *connecting = false;
                            *connected = true;
                            *error = None;
                            shown_projects.clone_from(&projects);
                            *selected_project_id =
                                projects.first().map(|project| project.id.clone());
                        }
                        self.menu_registry
                            .set_enabled(MenuAction::DisconnectServer, true);
                        if self.remote_previous_locale.is_none() {
                            self.remote_previous_locale = Some(self.ui_locale);
                        }
                        self.apply_ui_locale(server_locale);
                    }
                    Err(connection_error) => {
                        let remains_connected = self.remote_client.is_some();
                        if let Some(modal::ModalState::RemoteConnection {
                            connecting,
                            connected,
                            error,
                            ..
                        }) = self.active_modal.as_mut()
                        {
                            *connecting = false;
                            *connected = remains_connected;
                            *error = Some(tw(
                                self.ui_locale,
                                "remoteConnection.failed",
                                &[("reason", &connection_error)],
                            ));
                        }
                    }
                }
                Task::none()
            }
            Message::RemoteProjectSelected(project_id) => {
                if let Some(modal::ModalState::RemoteConnection {
                    selected_project_id,
                    error,
                    ..
                }) = self.active_modal.as_mut()
                {
                    *selected_project_id = Some(project_id);
                    *error = None;
                }
                Task::none()
            }
            Message::RemoteOpenProjectRequested => {
                let project_id = match self.active_modal.as_mut() {
                    Some(modal::ModalState::RemoteConnection {
                        selected_project_id: Some(project_id),
                        connecting,
                        error,
                        ..
                    }) => {
                        *connecting = true;
                        *error = None;
                        project_id.clone()
                    }
                    _ => return Task::none(),
                };
                let Some(client) = self.remote_client.clone() else {
                    return Task::done(Message::RemoteProjectOpened(Err(t(
                        self.ui_locale,
                        "remoteConnection.connectionLost",
                    ))));
                };
                Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || {
                            client
                                .open_project(&project_id)
                                .map_err(|error| error.to_string())
                        })
                        .await
                        .unwrap_or_else(|error| Err(format!("remote project task failed: {error}")))
                    },
                    Message::RemoteProjectOpened,
                )
            }
            Message::RemoteProjectOpened(result) => {
                match result {
                    Ok(project) => {
                        let target = self
                            .remote_client
                            .as_ref()
                            .map(|client| client.target().label())
                            .unwrap_or_default();
                        self.remote_display_name = Some(format!("{} — {target}", project.name));
                        self.remote_project = Some(project);
                        self.active_modal = None;
                        self.notify(
                            ToastLevel::Success,
                            &t(self.ui_locale, "remoteConnection.opened"),
                        );
                    }
                    Err(open_error) => {
                        if let Some(modal::ModalState::RemoteConnection {
                            connecting, error, ..
                        }) = self.active_modal.as_mut()
                        {
                            *connecting = false;
                            *error = Some(tw(
                                self.ui_locale,
                                "remoteConnection.openFailed",
                                &[("reason", &open_error)],
                            ));
                        }
                    }
                }
                Task::none()
            }
            Message::RemoteDisconnectRequested => {
                if let Some(client) = self.remote_client.take() {
                    client.close();
                }
                self.remote_projects.clear();
                self.remote_project = None;
                self.remote_display_name = None;
                self.active_modal = None;
                self.menu_registry
                    .set_enabled(MenuAction::DisconnectServer, false);
                if let Some(locale) = self.remote_previous_locale.take() {
                    self.apply_ui_locale(locale);
                }
                self.notify(
                    ToastLevel::Info,
                    &t(self.ui_locale, "remoteConnection.disconnected"),
                );
                Task::none()
            }
            Message::FindQueryChanged(value) => {
                if let Some(modal::ModalState::FindReplace { query, .. }) =
                    self.active_modal.as_mut()
                {
                    *query = value;
                }
                Task::none()
            }
            Message::ReplaceQueryChanged(value) => {
                if let Some(modal::ModalState::FindReplace { replacement, .. }) =
                    self.active_modal.as_mut()
                {
                    *replacement = value;
                }
                Task::none()
            }
            Message::FindNext => {
                let query = match &self.active_modal {
                    Some(modal::ModalState::FindReplace { query, .. }) => query.clone(),
                    _ => return Task::none(),
                };
                if !self.find_next_in_active_editor(&query) && !query.is_empty() {
                    self.notify(ToastLevel::Info, &t(self.ui_locale, "find.noMatches"));
                }
                Task::none()
            }
            Message::ReplaceCurrent => {
                let (query, replacement) = match &self.active_modal {
                    Some(modal::ModalState::FindReplace {
                        query, replacement, ..
                    }) => (query.clone(), replacement.clone()),
                    _ => return Task::none(),
                };
                self.replace_current_in_active_editor(&query, &replacement);
                Task::none()
            }
            Message::ReplaceAll => {
                let (query, replacement) = match &self.active_modal {
                    Some(modal::ModalState::FindReplace {
                        query, replacement, ..
                    }) => (query.clone(), replacement.clone()),
                    _ => return Task::none(),
                };
                let count = self.replace_all_in_active_editor(&query, &replacement);
                self.notify(
                    ToastLevel::Info,
                    &tw(
                        self.ui_locale,
                        "find.replacedCount",
                        &[("count", &count.to_string())],
                    ),
                );
                Task::none()
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
            Message::OneShotAiFinished {
                entity_id,
                action,
                result,
            } => self.finish_one_shot_ai(&entity_id, action, result),

            // ── Editor view messages ──
            Message::PostEditor(msg) => self.handle_post_editor_msg(msg),
            Message::MediaEditor(msg) => self.handle_media_editor_msg(msg),
            Message::TemplateEditor(msg) => self.handle_template_editor_msg(msg),
            Message::ScriptEditor(msg) => self.handle_script_editor_msg(msg),
            Message::Tags(msg) => self.handle_tags_msg(msg),
            Message::Settings(msg) => self.handle_settings_msg(msg),
            Message::Style(msg) => self.handle_style_msg(msg),
            Message::ImportEditor(msg) => self.handle_import_editor_msg(msg),
            Message::MenuEditor(msg) => self.handle_menu_editor_msg(msg),

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

    fn documentation_state(&self, kind: DocumentationKind) -> &DocumentationState {
        match kind {
            DocumentationKind::Guide => &self.guide_documentation,
            DocumentationKind::Api => &self.api_documentation,
            DocumentationKind::Cli => &self.cli_documentation,
            DocumentationKind::Mcp => &self.mcp_documentation,
        }
    }

    fn documentation_state_mut(&mut self, kind: DocumentationKind) -> &mut DocumentationState {
        match kind {
            DocumentationKind::Guide => &mut self.guide_documentation,
            DocumentationKind::Api => &mut self.api_documentation,
            DocumentationKind::Cli => &mut self.cli_documentation,
            DocumentationKind::Mcp => &mut self.mcp_documentation,
        }
    }

    fn start_documentation_load(&mut self, kind: DocumentationKind) -> Task<Message> {
        let generation = self.documentation_state_mut(kind).start_loading();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || match kind {
                    DocumentationKind::Guide => crate::views::documentation::load_user_guide(),
                    DocumentationKind::Api => crate::views::documentation::load_api_document(),
                    DocumentationKind::Cli => crate::views::documentation::load_cli_guide(),
                    DocumentationKind::Mcp => crate::views::documentation::load_mcp_guide(),
                })
                .await
                .unwrap_or_else(|error| DocumentLoad::Malformed {
                    signature: 0,
                    error: format!("documentation task panicked: {error}"),
                })
            },
            move |load| Message::DocumentationLoaded(kind, generation, load),
        )
    }

    fn reload_changed_documentation(&mut self) -> Task<Message> {
        let mut changed = Vec::new();
        for (tab_type, kind) in [
            (TabType::Documentation, DocumentationKind::Guide),
            (TabType::ApiDocumentation, DocumentationKind::Api),
            (TabType::CliDocumentation, DocumentationKind::Cli),
            (TabType::McpDocumentation, DocumentationKind::Mcp),
        ] {
            if !self.tabs.iter().any(|tab| tab.tab_type == tab_type) {
                continue;
            }
            let should_check = self.documentation_state(kind).should_check();
            if !should_check {
                continue;
            }
            let signature = current_signature(kind);
            let state = self.documentation_state_mut(kind);
            state.mark_checked();
            if state.status == crate::views::documentation::DocumentStatus::NotLoaded
                || signature != state.signature
            {
                changed.push(kind);
            }
        }
        Task::batch(
            changed
                .into_iter()
                .map(|kind| self.start_documentation_load(kind)),
        )
    }

    fn confirm_external_link(&mut self, url: String) -> bool {
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return false;
        }
        self.active_modal = Some(modal::ModalState::Confirm {
            title: t(self.ui_locale, "documentation.externalLinkTitle"),
            message: tw(
                self.ui_locale,
                "documentation.externalLinkMessage",
                &[("url", &url)],
            ),
            on_confirm: modal::ConfirmAction::OpenExternalUrl(url),
        });
        true
    }

    fn start_duplicate_search(&mut self, page: usize) -> Task<Message> {
        let (Some(project), Some(data_dir)) = (&self.active_project, &self.data_dir) else {
            return Task::none();
        };
        self.duplicates_state.enabled = engine::meta::read_project_json(data_dir)
            .is_ok_and(|metadata| metadata.semantic_similarity_enabled);
        self.duplicates_state.page = page;
        self.duplicates_state.error = None;
        if !self.duplicates_state.enabled {
            self.duplicates_state.is_loading = false;
            self.duplicates_state.has_run = false;
            self.duplicates_state.result = Default::default();
            return Task::none();
        }
        self.duplicates_state.is_loading = true;
        let db_path = self.db_path.clone();
        let data_dir = data_dir.clone();
        let project_id = project.id.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                    engine::embedding::EmbeddingService::production(db.conn(), &data_dir)
                        .find_duplicates(&project_id, page)
                        .map_err(|error| error.to_string())
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            Message::DuplicatesLoaded,
        )
    }

    fn dismiss_duplicate_pairs(&mut self, pairs: Vec<(String, String)>) -> Task<Message> {
        if pairs.is_empty() {
            return Task::none();
        }
        let (Some(_), Some(data_dir)) = (&self.db, &self.data_dir) else {
            return Task::none();
        };
        let db_path = self.db_path.clone();
        let data_dir = data_dir.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                    engine::embedding::EmbeddingService::production(db.conn(), &data_dir)
                        .dismiss_duplicate_pairs(&pairs)
                        .map_err(|error| error.to_string())
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            Message::DuplicatesDismissed,
        )
    }

    fn start_embedding_reindex(&mut self) -> Task<Message> {
        let Some(data_dir) = &self.data_dir else {
            return Task::none();
        };
        if !engine::meta::read_project_json(data_dir)
            .is_ok_and(|metadata| metadata.semantic_similarity_enabled)
        {
            self.notify(
                ToastLevel::Warning,
                &t(self.ui_locale, "duplicates.disabled"),
            );
            return Task::none();
        }
        let locale = self.ui_locale;
        self.spawn_engine_task(
            "menu.item.rebuildEmbeddingIndex",
            move |db_path, project_id, data_dir, tm, tid| {
                let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                let service = engine::embedding::EmbeddingService::production(db.conn(), &data_dir);
                let indexed = service
                    .reindex_all_with_progress(&project_id, |current, total| {
                        tm.report_progress(
                            tid,
                            Some(current as f32 / total.max(1) as f32),
                            Some(tw(
                                locale,
                                "embeddings.indexingProgress",
                                &[
                                    ("current", &current.to_string()),
                                    ("total", &total.to_string()),
                                ],
                            )),
                        );
                        !tm.is_cancelled(tid)
                    })
                    .map_err(|error| error.to_string())?;
                service
                    .flush_project(&project_id)
                    .map_err(|error| error.to_string())?;
                Ok(tw(
                    locale,
                    "embeddings.reindexed",
                    &[("count", &indexed.len().to_string())],
                ))
            },
        )
    }

    fn start_embedding_backfill(&mut self) -> Task<Message> {
        let Some(data_dir) = &self.data_dir else {
            return Task::none();
        };
        if !engine::meta::read_project_json(data_dir)
            .is_ok_and(|metadata| metadata.semantic_similarity_enabled)
        {
            return Task::none();
        }
        let locale = self.ui_locale;
        self.spawn_engine_task(
            "embeddings.indexing",
            move |db_path, project_id, data_dir, tm, tid| {
                let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                let indexed = engine::embedding::EmbeddingService::production(db.conn(), &data_dir)
                    .index_unindexed_with_progress(&project_id, |current, total| {
                        tm.report_progress(
                            tid,
                            Some(current as f32 / total.max(1) as f32),
                            Some(tw(
                                locale,
                                "embeddings.indexingProgress",
                                &[
                                    ("current", &current.to_string()),
                                    ("total", &total.to_string()),
                                ],
                            )),
                        );
                        !tm.is_cancelled(tid)
                    })
                    .map_err(|error| error.to_string())?;
                Ok(tw(
                    locale,
                    "embeddings.indexed",
                    &[("count", &indexed.len().to_string())],
                ))
            },
        )
    }

    pub fn view(&self) -> Element<'_, Message> {
        let active_name = self.remote_display_name.as_deref().or_else(|| {
            self.active_project
                .as_ref()
                .map(|project| project.name.as_str())
        });
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
        let style_preview_widget = if self.active_style_uses_embedded_preview() {
            self.embedded_style_preview
                .as_ref()
                .map(|preview| webview::webview(&preview.controller).into())
        } else {
            None
        };

        native_edit::native_edit(
            workspace::view(
                self.sidebar_view,
                self.sidebar_visible,
                self.sidebar_width,
                &self.tabs,
                self.active_tab.as_deref(),
                self.panel_visible,
                self.panel_tab,
                &self.task_snapshots,
                &self.collapsed_task_groups,
                &self.output_entries,
                &self.sidebar_posts,
                &self.sidebar_media,
                &self.sidebar_scripts,
                &self.sidebar_templates,
                &self.sidebar_imports,
                &self.chat_conversations,
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
                style_preview_widget,
                &self.post_editors,
                &self.media_editors,
                &self.template_editors,
                &self.script_editors,
                &self.import_editors,
                &self.chat_editors,
                self.tags_view_state.as_ref(),
                self.settings_state.as_ref(),
                self.style_view_state.as_ref(),
                self.dashboard_state.as_ref(),
                &self.site_validation_state,
                &self.duplicates_state,
                &self.guide_documentation,
                &self.api_documentation,
                &self.cli_documentation,
                &self.mcp_documentation,
                &self.metadata_diff_state,
                &self.menu_editor_state,
                &self.translation_validation_state,
                &self.git_state,
                &self.git_diffs,
                &self.git_file_history,
            ),
            Arc::clone(&self.native_edit_commands),
        )
        .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        let menu_sub = menu::menu_subscription();

        let task_tick =
            iced::time::every(std::time::Duration::from_millis(500)).map(|_| Message::TaskTick);
        let domain_event_tick = iced::time::every(std::time::Duration::from_millis(100))
            .map(|_| Message::DomainEventsTick);

        let toast_tick = if !self.toasts.is_empty() {
            iced::time::every(std::time::Duration::from_millis(250)).map(|_| Message::ExpireToasts)
        } else {
            Subscription::none()
        };

        let file_drop_sub = iced::event::listen_with(|event, _status, _id| match event {
            iced::Event::Window(window::Event::FileDropped(path)) => {
                Some(Message::FileDropped(path))
            }
            _ => None,
        });

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

        let menu_interaction_sub = if self.menu_editor_state.draft.is_some() {
            iced::event::listen_with(|event, _status, _id| match event {
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }) => Some(Message::MenuEditor(MenuEditorMsg::CancelDraft)),
                _ => None,
            })
        } else if self.menu_editor_state.dragging_id.is_some() {
            iced::event::listen_with(|event, _status, _id| match event {
                iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                    iced::mouse::Button::Left,
                )) => Some(Message::MenuEditor(MenuEditorMsg::Drop)),
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }) => Some(Message::MenuEditor(MenuEditorMsg::DragCancel)),
                _ => None,
            })
        } else {
            Subscription::none()
        };
        let menu_expand_tick = if self.menu_editor_state.hover_expand.is_some() {
            iced::time::every(std::time::Duration::from_millis(50))
                .map(|_| Message::MenuEditor(MenuEditorMsg::ExpandTick))
        } else {
            Subscription::none()
        };

        Subscription::batch([
            menu_sub,
            task_tick,
            domain_event_tick,
            toast_tick,
            file_drop_sub,
            drag_sub,
            menu_interaction_sub,
            menu_expand_tick,
        ])
    }

    // ── Private helpers ──

    fn active_chat_id(&self) -> Option<&str> {
        let id = self.active_tab.as_deref()?;
        self.tabs
            .iter()
            .any(|tab| tab.id == id && tab.tab_type == TabType::Chat)
            .then_some(id)
    }

    fn active_chat_state_mut(&mut self) -> Option<&mut ChatEditorState> {
        let id = self.active_chat_id()?.to_string();
        self.chat_editors.get_mut(&id)
    }

    fn refresh_chat_conversations(&mut self) {
        self.chat_conversations = self
            .db
            .as_ref()
            .and_then(|db| engine::chat::list_conversations(db.conn()).ok())
            .unwrap_or_default();
    }

    fn persist_due_chat_surface_state(&mut self) {
        let due = self
            .chat_editors
            .iter()
            .filter_map(|(id, state)| {
                state.surface_state_dirty_since.and_then(|since| {
                    (since.elapsed() >= std::time::Duration::from_millis(500)).then(|| id.clone())
                })
            })
            .collect::<Vec<_>>();
        for id in due {
            self.persist_chat_surface_state(&id);
        }
    }

    fn persist_chat_surface_state(&mut self, conversation_id: &str) {
        let Some(surface_state) = self
            .chat_editors
            .get(conversation_id)
            .map(|state| state.surface_state.clone())
        else {
            return;
        };
        let result = self
            .db
            .as_ref()
            .ok_or_else(|| "database unavailable".to_string())
            .and_then(|db| {
                engine::chat::put_surface_state(db.conn(), conversation_id, &surface_state)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(()) => {
                if let Some(state) = self.chat_editors.get_mut(conversation_id) {
                    state.conversation.surface_state = serde_json::to_string(&surface_state).ok();
                    state.surface_state_dirty_since = None;
                }
            }
            Err(error) => self.notify(ToastLevel::Error, &error),
        }
    }

    fn chat_model_options(&self) -> Vec<ChatModelChoice> {
        let Some(db) = &self.db else {
            return Vec::new();
        };
        let Ok(settings) = ai::load_ai_settings(db.conn(), self.offline_mode) else {
            return Vec::new();
        };
        let active = settings.active();
        let mut models = active
            .models
            .iter()
            .map(|model| ChatModelChoice {
                id: model.id.clone(),
                label: model.name.clone(),
            })
            .collect::<Vec<_>>();
        let model = active.endpoint.model.clone();
        if !model.trim().is_empty() && !models.iter().any(|choice| choice.id == model) {
            models.push(ChatModelChoice {
                id: model.clone(),
                label: model,
            });
        }
        models.sort_by(|left, right| left.label.cmp(&right.label));
        models.dedup_by(|left, right| left.id == right.id);
        models
    }

    fn create_chat_conversation(&mut self) -> Task<Message> {
        let model = self
            .db
            .as_ref()
            .and_then(|db| ai::load_ai_settings(db.conn(), self.offline_mode).ok())
            .map(|settings| settings.active().endpoint.model.clone())
            .filter(|model| !model.trim().is_empty())
            .or_else(|| {
                self.chat_model_options()
                    .into_iter()
                    .next()
                    .map(|choice| choice.id)
            });
        let title = model.as_deref().map_or_else(
            || t(self.ui_locale, "chat.new"),
            |model| tw(self.ui_locale, "chat.newWithModel", &[("model", model)]),
        );
        let result = self
            .db
            .as_ref()
            .ok_or_else(|| "database unavailable".to_string())
            .and_then(|db| {
                engine::chat::create_conversation_titled(db.conn(), model.as_deref(), &title)
                    .map_err(|error| error.to_string())
            });
        match result {
            Ok(conversation) => {
                self.refresh_chat_conversations();
                Task::done(Message::OpenTab(Tab {
                    id: conversation.id,
                    tab_type: TabType::Chat,
                    title: conversation.title,
                    is_transient: false,
                    is_dirty: false,
                }))
            }
            Err(error) => {
                self.notify(ToastLevel::Error, &error);
                Task::none()
            }
        }
    }

    fn send_active_chat_message(&mut self) -> Task<Message> {
        let Some(conversation_id) = self.active_chat_id().map(str::to_string) else {
            return Task::none();
        };
        let Some(project_id) = self
            .active_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            return Task::none();
        };
        let Some(data_dir) = self.data_dir.clone() else {
            return Task::none();
        };
        let Some(state) = self.chat_editors.get_mut(&conversation_id) else {
            return Task::none();
        };
        if state.streaming {
            return Task::none();
        }
        let content = state.input.text().trim().to_string();
        if content.is_empty() {
            return Task::none();
        }
        state.input = iced::widget::text_editor::Content::new();
        state.streaming = true;
        state.clear_streaming();
        state.active_tool = None;
        state.error = None;
        let db_path = self.db_path.clone();
        let offline_mode = self.offline_mode;
        let result_id = conversation_id.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                    engine::chat::send_chat_message(
                        db.conn(),
                        &data_dir,
                        &project_id,
                        offline_mode,
                        &conversation_id,
                        &content,
                        engine::chat::ChatSendOptions::default(),
                    )
                    .map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| error.to_string())?
            },
            move |result| Message::ChatFinished {
                conversation_id: result_id.clone(),
                result,
            },
        )
    }

    fn process_chat_events(&mut self) {
        let events = self.chat_events.try_iter().collect::<Vec<_>>();
        for event in events {
            match event {
                engine::chat::ChatEvent::Content {
                    conversation_id,
                    content,
                } => {
                    if let Some(state) = self.chat_editors.get_mut(&conversation_id) {
                        state.set_streaming_content(content);
                    }
                }
                engine::chat::ChatEvent::ToolStarted {
                    conversation_id,
                    name,
                    surface_id,
                    arguments,
                } => {
                    if let Some(state) = self.chat_editors.get_mut(&conversation_id) {
                        state.active_tool = Some(name.clone());
                        state.add_streaming_surface(&name, &arguments, surface_id);
                    }
                }
                engine::chat::ChatEvent::ToolFinished {
                    conversation_id, ..
                } => {
                    if let Some(state) = self.chat_editors.get_mut(&conversation_id) {
                        state.active_tool = None;
                    }
                }
                engine::chat::ChatEvent::Failed {
                    conversation_id,
                    message,
                } => {
                    let message = localize_chat_error(self.ui_locale, &message);
                    if let Some(state) = self.chat_editors.get_mut(&conversation_id) {
                        state.error = Some(message);
                    }
                }
                engine::chat::ChatEvent::Navigate {
                    destination,
                    entity_id,
                } => self.dispatch_chat_navigation(&destination, entity_id.as_deref()),
                engine::chat::ChatEvent::Started { .. }
                | engine::chat::ChatEvent::Finished { .. }
                | engine::chat::ChatEvent::Cancelled { .. } => {}
            }
        }
    }

    fn dispatch_chat_navigation(&mut self, destination: &str, entity_id: Option<&str>) {
        match destination {
            "toggle_sidebar" => {
                self.sidebar_visible = !self.sidebar_visible;
                return;
            }
            "toggle_panel" => {
                self.panel_visible = !self.panel_visible;
                return;
            }
            "toggle_assistant_sidebar" => {
                if self.sidebar_view == SidebarView::Chat {
                    self.sidebar_visible = !self.sidebar_visible;
                } else {
                    self.sidebar_view = SidebarView::Chat;
                    self.sidebar_visible = true;
                }
                return;
            }
            _ => {}
        }

        self.sidebar_view = match destination {
            "posts" => SidebarView::Posts,
            "pages" => SidebarView::Pages,
            "media" => SidebarView::Media,
            "templates" => SidebarView::Templates,
            "scripts" => SidebarView::Scripts,
            "tags" => SidebarView::Tags,
            "chat" => SidebarView::Chat,
            "import" => SidebarView::Import,
            "git" => SidebarView::Git,
            "settings" => SidebarView::Settings,
            _ => return,
        };
        self.sidebar_visible = true;
        if destination == "settings" && entity_id.is_none() {
            self.open_singleton_tab(TabType::Settings, "common.settings");
            if self.settings_state.is_none() {
                self.settings_state = Some(self.hydrate_settings_state());
            }
            return;
        }
        if destination == "tags" && entity_id.is_none() {
            self.open_singleton_tab(TabType::Tags, "tabBar.tags");
            return;
        }
        let Some(id) = entity_id else { return };
        let tab = self.db.as_ref().and_then(|db| match destination {
            "posts" => bds_core::db::queries::post::get_post_by_id(db.conn(), id)
                .ok()
                .map(|item| (TabType::Post, item.title)),
            "media" => bds_core::db::queries::media::get_media_by_id(db.conn(), id)
                .ok()
                .map(|item| {
                    (
                        TabType::Media,
                        item.title.unwrap_or_else(|| item.original_name.clone()),
                    )
                }),
            "templates" => bds_core::db::queries::template::get_template_by_id(db.conn(), id)
                .ok()
                .map(|item| (TabType::Templates, item.title)),
            "scripts" => bds_core::db::queries::script::get_script_by_id(db.conn(), id)
                .ok()
                .map(|item| (TabType::Scripts, item.title)),
            "chat" => engine::chat::get_conversation(db.conn(), id)
                .ok()
                .map(|item| (TabType::Chat, item.title)),
            _ => None,
        });
        let Some((tab_type, title)) = tab else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "chat.navigation.invalid"),
            );
            return;
        };
        let index = tabs::open_tab(
            &mut self.tabs,
            Tab {
                id: id.to_string(),
                tab_type,
                title,
                is_transient: false,
                is_dirty: false,
            },
        );
        if let Some(tab) = self.tabs.get(index).cloned() {
            self.active_tab = Some(tab.id.clone());
            self.load_editor_for_tab(&tab);
        }
    }

    fn apply_ui_locale(&mut self, locale: UiLocale) {
        self.ui_locale = locale;
        self.locale_dropdown_open = false;
        menu::update_menu_labels(&self.menu_registry, locale);
        for tab in &mut self.tabs {
            if let Some(key) = tab.tab_type.i18n_key() {
                tab.title = t(locale, key);
            }
        }
    }

    fn process_domain_events(&mut self) -> Task<Message> {
        let remote_messages = self
            .remote_client
            .as_ref()
            .map(|client| client.drain_events())
            .unwrap_or_default();
        for message in remote_messages {
            match message {
                bds_server::protocol::ServerMessage::Event { sequence, event } => {
                    self.add_output(&tw(
                        self.ui_locale,
                        "remoteConnection.eventReceived",
                        &[
                            ("sequence", &sequence.to_string()),
                            ("event", &format!("{event:?}")),
                        ],
                    ));
                }
                bds_server::protocol::ServerMessage::Tasks { tasks, .. } => {
                    self.add_output(&tw(
                        self.ui_locale,
                        "remoteConnection.taskUpdate",
                        &[("count", &tasks.len().to_string())],
                    ));
                }
                bds_server::protocol::ServerMessage::Error { code, message, .. } => {
                    self.notify(ToastLevel::Error, &message);
                    if remote_error_closes_connection(&code) {
                        self.remote_client = None;
                        self.remote_project = None;
                        self.remote_projects.clear();
                        self.remote_display_name = None;
                        self.menu_registry
                            .set_enabled(MenuAction::DisconnectServer, false);
                        if let Some(locale) = self.remote_previous_locale.take() {
                            self.apply_ui_locale(locale);
                        }
                    }
                }
                bds_server::protocol::ServerMessage::Response { .. } => {}
            }
        }
        if let Some(db) = &self.db {
            let _ = engine::cli_sync::poll_notifications(db.conn());
        }
        let events = self.domain_events.drain();
        if events.is_empty() {
            return Task::none();
        }

        let mut refresh_content = false;
        for event in events {
            refresh_content |= self.handle_domain_event(event);
        }
        if refresh_content {
            self.refresh_counts()
        } else {
            Task::none()
        }
    }

    fn handle_domain_event(&mut self, event: DomainEvent) -> bool {
        match event {
            DomainEvent::SettingsChanged { project_id, key } => {
                let relevant = project_id.is_none()
                    || project_id.as_deref()
                        == self
                            .active_project
                            .as_ref()
                            .map(|project| project.id.as_str());
                if !relevant {
                    return false;
                }
                if key == engine::settings::UI_LANGUAGE_KEY
                    && let Some(db) = &self.db
                    && let Ok(Some(language)) = engine::settings::ui_language(db.conn())
                {
                    self.apply_ui_locale(normalize_language(&language));
                }
                if self
                    .tabs
                    .iter()
                    .any(|tab| tab.tab_type == TabType::Settings)
                {
                    let active_section = self
                        .settings_state
                        .as_ref()
                        .and_then(|state| state.active_section.clone());
                    let mut state = self.hydrate_settings_state();
                    if let Some(section) = active_section {
                        state.focus_section(section);
                    }
                    self.settings_state = Some(state);
                }
                false
            }
            DomainEvent::EntityChanged {
                project_id,
                entity: DomainEntity::Project,
                ..
            } => {
                let previous_active = self
                    .active_project
                    .as_ref()
                    .map(|project| project.id.clone());
                if let Some(db) = &self.db {
                    self.projects = engine::project::list_projects(db.conn()).unwrap_or_default();
                    self.active_project = engine::project::get_active_project(db.conn())
                        .ok()
                        .flatten();
                    self.data_dir = self
                        .active_project
                        .as_ref()
                        .and_then(|project| project.data_path.as_deref())
                        .map(PathBuf::from);
                }
                if let Some(data_dir) = self.data_dir.as_deref()
                    && let Ok(metadata) = engine::meta::read_project_json(data_dir)
                {
                    let applied = StyleViewState::new(metadata.pico_theme.as_deref());
                    self.theme_badge = applied.applied_theme.clone();
                    if let Some(state) = self.style_view_state.as_mut() {
                        state.refresh_applied_theme(metadata.pico_theme.as_deref());
                    }
                }
                previous_active.as_deref() == Some(project_id.as_str())
                    || self
                        .active_project
                        .as_ref()
                        .map(|project| project.id.as_str())
                        == Some(project_id.as_str())
            }
            DomainEvent::EntityChanged {
                project_id,
                entity,
                entity_id,
                action,
            } => {
                if self
                    .active_project
                    .as_ref()
                    .map(|project| project.id.as_str())
                    != Some(project_id.as_str())
                {
                    return false;
                }
                self.refresh_entity_editor(entity, &entity_id, action);
                true
            }
        }
    }

    fn refresh_entity_editor(
        &mut self,
        entity: DomainEntity,
        entity_id: &str,
        action: NotificationAction,
    ) {
        if action == NotificationAction::Deleted {
            match entity {
                DomainEntity::Post => {
                    self.post_editors.remove(entity_id);
                }
                DomainEntity::Media => {
                    self.media_editors.remove(entity_id);
                }
                DomainEntity::Template => {
                    self.template_editors.remove(entity_id);
                }
                DomainEntity::Script => {
                    self.script_editors.remove(entity_id);
                }
                DomainEntity::Tag => {
                    self.tags_view_state = None;
                }
                DomainEntity::Project | DomainEntity::Setting => {}
            }
            if !matches!(entity, DomainEntity::Tag) {
                self.close_entity_tab(entity_id);
            }
            return;
        }

        if entity == DomainEntity::Tag {
            self.tags_view_state = None;
            self.refresh_post_editor_tag_options();
            if let Some(tab) = self
                .tabs
                .iter()
                .find(|tab| tab.tab_type == TabType::Tags)
                .cloned()
            {
                self.load_editor_for_tab(&tab);
            }
            return;
        }

        let tab = self.tabs.iter().find(|tab| tab.id == entity_id).cloned();
        let Some(tab) = tab else { return };
        let previous_post_state = (entity == DomainEntity::Post)
            .then(|| self.post_editors.get(entity_id).cloned())
            .flatten();
        let dirty = match entity {
            DomainEntity::Post => self
                .post_editors
                .get(entity_id)
                .is_some_and(|state| state.is_dirty),
            DomainEntity::Media => self
                .media_editors
                .get(entity_id)
                .is_some_and(|state| state.is_dirty),
            DomainEntity::Template => self
                .template_editors
                .get(entity_id)
                .is_some_and(|state| state.is_dirty),
            DomainEntity::Script => self
                .script_editors
                .get(entity_id)
                .is_some_and(|state| state.is_dirty),
            DomainEntity::Tag | DomainEntity::Project | DomainEntity::Setting => false,
        };
        if dirty {
            return;
        }
        match entity {
            DomainEntity::Post => {
                self.post_editors.remove(entity_id);
            }
            DomainEntity::Media => {
                self.media_editors.remove(entity_id);
            }
            DomainEntity::Template => {
                self.template_editors.remove(entity_id);
            }
            DomainEntity::Script => {
                self.script_editors.remove(entity_id);
            }
            DomainEntity::Tag | DomainEntity::Project | DomainEntity::Setting => {}
        }
        self.load_editor_for_tab(&tab);
        if let (Some(previous), Some(current)) =
            (previous_post_state, self.post_editors.get_mut(entity_id))
        {
            current.restore_view_state(&previous);
        }
        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == entity_id) {
            tab.title = match entity {
                DomainEntity::Post => self
                    .post_editors
                    .get(entity_id)
                    .map(|state| state.title.clone()),
                DomainEntity::Media => self
                    .media_editors
                    .get(entity_id)
                    .map(|state| state.title.clone()),
                DomainEntity::Template => self
                    .template_editors
                    .get(entity_id)
                    .map(|state| state.title.clone()),
                DomainEntity::Script => self
                    .script_editors
                    .get(entity_id)
                    .map(|state| state.title.clone()),
                DomainEntity::Tag | DomainEntity::Project | DomainEntity::Setting => None,
            }
            .unwrap_or_else(|| tab.title.clone());
        }
    }

    fn dispatch_menu_action(&mut self, action: MenuAction) -> Task<Message> {
        match action {
            // File
            MenuAction::NewPost => self.create_sidebar_post(false),
            MenuAction::ImportMedia => crate::platform::dialog::pick_media_files(
                t(self.ui_locale, "dialog.importMedia"),
                t(self.ui_locale, "dialog.imageFilter"),
            ),
            MenuAction::Save => match self.active_tab_type() {
                Some(TabType::Post) => Task::done(Message::PostEditor(PostEditorMsg::Save)),
                Some(TabType::Media) => Task::done(Message::MediaEditor(MediaEditorMsg::Save)),
                Some(TabType::Templates) => {
                    Task::done(Message::TemplateEditor(TemplateEditorMsg::Save))
                }
                Some(TabType::Scripts) => Task::done(Message::ScriptEditor(ScriptEditorMsg::Save)),
                Some(TabType::MenuEditor) => Task::done(Message::MenuEditor(MenuEditorMsg::Save)),
                _ => Task::none(),
            },
            MenuAction::OpenInBrowser => self.open_preview_in_browser(),
            MenuAction::OpenDataFolder => {
                if let Some(ref dir) = self.data_dir {
                    let _ = open::that(dir);
                }
                Task::none()
            }
            MenuAction::ConnectServer => {
                let target = self
                    .remote_client
                    .as_ref()
                    .map(|client| client.target().label())
                    .unwrap_or_default();
                self.active_modal = Some(modal::ModalState::RemoteConnection {
                    target,
                    connecting: false,
                    connected: self.remote_client.is_some(),
                    error: None,
                    projects: self.remote_projects.clone(),
                    selected_project_id: self
                        .remote_project
                        .as_ref()
                        .map(|project| project.id.clone()),
                });
                Task::none()
            }
            MenuAction::DisconnectServer => Task::done(Message::RemoteDisconnectRequested),
            // Edit
            MenuAction::Undo => {
                native_edit::queue_command(
                    &self.native_edit_commands,
                    native_edit::EditCommand::Undo,
                );
                Task::none()
            }
            MenuAction::Redo => {
                native_edit::queue_command(
                    &self.native_edit_commands,
                    native_edit::EditCommand::Redo,
                );
                Task::none()
            }
            MenuAction::Cut => {
                native_edit::queue_command(
                    &self.native_edit_commands,
                    native_edit::EditCommand::Cut,
                );
                Task::none()
            }
            MenuAction::Copy => {
                native_edit::queue_command(
                    &self.native_edit_commands,
                    native_edit::EditCommand::Copy,
                );
                Task::none()
            }
            MenuAction::Paste => {
                native_edit::queue_command(
                    &self.native_edit_commands,
                    native_edit::EditCommand::Paste,
                );
                Task::none()
            }
            MenuAction::SelectAll => {
                native_edit::queue_command(
                    &self.native_edit_commands,
                    native_edit::EditCommand::SelectAll,
                );
                Task::none()
            }
            MenuAction::Find => {
                self.active_modal = Some(modal::ModalState::FindReplace {
                    query: String::new(),
                    replacement: String::new(),
                    show_replace: false,
                });
                Task::none()
            }
            MenuAction::Replace => {
                self.active_modal = Some(modal::ModalState::FindReplace {
                    query: String::new(),
                    replacement: String::new(),
                    show_replace: true,
                });
                Task::none()
            }
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
            MenuAction::PublishSelected => match self.active_tab_type() {
                Some(TabType::Post) => Task::done(Message::PostEditor(PostEditorMsg::Publish)),
                _ => Task::none(),
            },
            MenuAction::PreviewPost => self.preview_active_post(),
            MenuAction::EditMenu => {
                self.open_singleton_tab(TabType::MenuEditor, "tabBar.menuEditor");
                Task::done(Message::MenuEditor(MenuEditorMsg::Reload))
            }
            MenuAction::RebuildDatabase => Task::done(Message::RebuildDatabase),
            MenuAction::ReindexText => Task::done(Message::ReindexText),
            MenuAction::RebuildEmbeddingIndex => Task::done(Message::EmbeddingReindex),
            MenuAction::FindDuplicates => {
                self.open_singleton_tab(TabType::FindDuplicates, "tabBar.findDuplicates");
                Task::done(Message::DuplicatesRefresh)
            }
            MenuAction::MetadataDiff => Task::done(Message::RunMetadataDiff),
            MenuAction::RegenerateCalendar => Task::done(Message::RegenerateCalendar),
            MenuAction::ValidateTranslations => Task::done(Message::ValidateTranslations),
            MenuAction::FillMissingTranslations => {
                let offline_mode = self.offline_mode;
                let locale = self.ui_locale;
                self.spawn_grouped_engine_task(
                    "engine.fillMissingTranslationsStarted",
                    "AI",
                    move |db_path, project_id, data_dir, task_manager, task_id| {
                        let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                        let meta = engine::meta::read_project_json(&data_dir)
                            .map_err(|error| error.to_string())?;
                        let main_language = meta.main_language.as_deref().unwrap_or("en");
                        let task_manager_for_progress = Arc::clone(&task_manager);
                        let report = engine::auto_translation::fill_missing_translations(
                            db.conn(),
                            &data_dir,
                            &project_id,
                            main_language,
                            &meta.blog_languages,
                            offline_mode,
                            move |progress, _message| {
                                task_manager_for_progress.report_progress(
                                    task_id,
                                    Some(progress),
                                    Some(t(locale, "engine.translatingContent")),
                                );
                                !task_manager_for_progress.is_cancelled(task_id)
                            },
                        )
                        .map_err(|error| error.to_string())?;
                        Ok(tw(
                            locale,
                            "engine.fillMissingTranslationsComplete",
                            &[
                                ("posts", &report.translated_posts.to_string()),
                                ("media", &report.translated_media.to_string()),
                                ("failed", &report.failed_count.to_string()),
                            ],
                        ))
                    },
                )
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
                    return Task::none();
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
                        return Task::none();
                    }
                }
                let locale = self.ui_locale;
                self.spawn_engine_task(
                    "engine.uploadStarted",
                    move |db_path, _project_id, data_dir, task_manager, task_id| {
                        let preferences = engine::meta::read_publishing_json(&data_dir)
                            .map_err(|error| error.to_string())?;
                        let cache_dir = db_path.parent().map(PathBuf::from).ok_or_else(|| {
                            "private application directory unavailable".to_string()
                        })?;
                        let job = engine::publishing::upload_site(
                            &data_dir,
                            &cache_dir,
                            &preferences,
                            |current, total, kind| {
                                let target = match kind {
                                    engine::publishing::UploadTargetKind::Html => "html",
                                    engine::publishing::UploadTargetKind::Thumbnails => {
                                        "thumbnails"
                                    }
                                    engine::publishing::UploadTargetKind::Media => "media",
                                };
                                task_manager.report_progress(
                                    task_id,
                                    Some(current as f32 / total.max(1) as f32),
                                    Some(tw(
                                        locale,
                                        "engine.uploadingTarget",
                                        &[("target", target)],
                                    )),
                                );
                            },
                        )
                        .map_err(|error| error.to_string())?;
                        Ok(format!("{} targets uploaded", job.completed_targets.len()))
                    },
                )
            }
            // Help
            MenuAction::About => {
                self.add_output(&t(self.ui_locale, "menu.item.about"));
                Task::none()
            }
            MenuAction::OpenDocumentation => {
                self.open_singleton_tab(TabType::Documentation, "tabBar.documentation");
                self.start_documentation_load(DocumentationKind::Guide)
            }
            MenuAction::OpenApiDocumentation => {
                self.open_singleton_tab(TabType::ApiDocumentation, "tabBar.apiDocumentation");
                self.start_documentation_load(DocumentationKind::Api)
            }
            MenuAction::OpenCliDocumentation => {
                self.open_singleton_tab(TabType::CliDocumentation, "tabBar.cliDocumentation");
                self.start_documentation_load(DocumentationKind::Cli)
            }
            MenuAction::OpenMcpDocumentation => {
                self.open_singleton_tab(TabType::McpDocumentation, "tabBar.mcpDocumentation");
                self.start_documentation_load(DocumentationKind::Mcp)
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

    fn create_sidebar_import(&mut self) -> Task<Message> {
        let (Some(db), Some(project)) = (&self.db, &self.active_project) else {
            return Task::none();
        };
        match engine::wordpress_import::create_definition(
            db.conn(),
            &project.id,
            &t(self.ui_locale, "import.untitled"),
        ) {
            Ok(definition) => {
                self.sidebar_imports.push(definition.clone());
                let tab = Tab {
                    id: definition.id.clone(),
                    tab_type: TabType::Import,
                    title: definition.name.clone(),
                    is_transient: true,
                    is_dirty: false,
                };
                let index = tabs::open_tab(&mut self.tabs, tab);
                if let Some(tab) = self.tabs.get(index).cloned() {
                    self.active_tab = Some(tab.id.clone());
                    self.load_editor_for_tab(&tab);
                }
                self.sync_menu_state();
            }
            Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
        }
        Task::none()
    }

    fn handle_import_editor_msg(&mut self, message: ImportEditorMsg) -> Task<Message> {
        let Some(definition_id) = self.active_tab.clone().filter(|id| {
            self.tabs
                .iter()
                .any(|tab| tab.id == *id && tab.tab_type == TabType::Import)
        }) else {
            return Task::none();
        };
        match message {
            ImportEditorMsg::NameChanged(name) => {
                let Some(db) = &self.db else {
                    return Task::none();
                };
                match engine::wordpress_import::update_definition(
                    db.conn(),
                    &definition_id,
                    Some(&name),
                    None,
                    None,
                    None,
                ) {
                    Ok(definition) => {
                        if let Some(state) = self.import_editors.get_mut(&definition_id) {
                            state.definition = definition.clone();
                        }
                        if let Some(item) = self
                            .sidebar_imports
                            .iter_mut()
                            .find(|item| item.id == definition_id)
                        {
                            *item = definition;
                        }
                        if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == definition_id)
                        {
                            tab.title = name;
                        }
                    }
                    Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
                }
                Task::none()
            }
            ImportEditorMsg::PickUploads => crate::platform::dialog::pick_import_uploads_folder(
                definition_id,
                t(self.ui_locale, "import.uploadsFolder"),
            ),
            ImportEditorMsg::PickWxr => crate::platform::dialog::pick_import_wxr_file(
                definition_id,
                t(self.ui_locale, "import.wxrFile"),
                t(self.ui_locale, "import.wxrFilter"),
            ),
            ImportEditorMsg::Analyze => self.start_import_analysis(&definition_id),
            ImportEditorMsg::Execute => self.start_import_execution(&definition_id),
            ImportEditorMsg::AutoMapTaxonomy => self.start_import_auto_mapping(&definition_id),
            ImportEditorMsg::DeleteRequested => {
                let name = self
                    .import_editors
                    .get(&definition_id)
                    .map(|state| state.definition.name.clone())
                    .unwrap_or_default();
                self.active_modal = Some(modal::ModalState::Confirm {
                    title: t(self.ui_locale, "import.deleteTitle"),
                    message: tw(self.ui_locale, "import.deleteMessage", &[("name", &name)]),
                    on_confirm: modal::ConfirmAction::DeleteImport(definition_id),
                });
                Task::none()
            }
            ImportEditorMsg::SetResolution {
                kind,
                identity,
                resolution,
            } => {
                let mut report = self
                    .import_editors
                    .get(&definition_id)
                    .and_then(|state| state.report.clone());
                if let Some(report) = report.as_mut() {
                    match engine::wordpress_import::set_conflict_resolution(
                        report, kind, &identity, resolution,
                    )
                    .and_then(|_| self.persist_import_report(&definition_id, report))
                    {
                        Ok(definition) => {
                            if let Some(state) = self.import_editors.get_mut(&definition_id) {
                                state.definition = definition;
                                state.report = Some(report.clone());
                            }
                        }
                        Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
                    }
                }
                Task::none()
            }
            ImportEditorMsg::SetTaxonomyMapping {
                kind,
                source,
                target,
            } => {
                let mut report = self
                    .import_editors
                    .get(&definition_id)
                    .and_then(|state| state.report.clone());
                if let Some(report) = report.as_mut() {
                    match engine::wordpress_import::set_taxonomy_mapping(
                        report,
                        kind,
                        &source,
                        target.as_deref(),
                    )
                    .and_then(|_| self.persist_import_report(&definition_id, report))
                    {
                        Ok(definition) => {
                            if let Some(state) = self.import_editors.get_mut(&definition_id) {
                                state.definition = definition;
                                state.report = Some(report.clone());
                            }
                        }
                        Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
                    }
                }
                Task::none()
            }
            ImportEditorMsg::ToggleSection(section) => {
                if let Some(state) = self.import_editors.get_mut(&definition_id)
                    && !state.expanded.remove(&section)
                {
                    state.expanded.insert(section);
                }
                Task::none()
            }
        }
    }

    fn reload_menu_editor(&mut self) {
        let result = match (&self.db, &self.active_project, &self.data_dir) {
            (Some(db), Some(project), Some(data_dir)) => {
                crate::views::menu_editor::load(db, &project.id, data_dir)
            }
            _ => Err(t(self.ui_locale, "menuEditor.noProject")),
        };
        match result {
            Ok(state) => self.menu_editor_state = state,
            Err(error) => {
                self.menu_editor_state.status = MenuEditorStatus::LoadFailed;
                self.menu_editor_state.error = Some(error.clone());
                self.notify(
                    ToastLevel::Error,
                    &tw(
                        self.ui_locale,
                        "menuEditor.loadFailedDetail",
                        &[("error", &error)],
                    ),
                );
            }
        }
    }

    fn handle_menu_editor_msg(&mut self, message: MenuEditorMsg) -> Task<Message> {
        match message {
            MenuEditorMsg::Reload => self.reload_menu_editor(),
            MenuEditorMsg::Select(id) => self.menu_editor_state.selected_id = Some(id),
            MenuEditorMsg::StartDraft(kind) => {
                self.menu_editor_state.start_draft(kind);
            }
            MenuEditorMsg::DraftChanged(value) => self.menu_editor_state.draft_changed(value),
            MenuEditorMsg::ChoosePage(id) => {
                let _ = self.menu_editor_state.choose_page(&id);
            }
            MenuEditorMsg::ChooseCategory(name) => {
                let _ = self.menu_editor_state.choose_category(&name);
            }
            MenuEditorMsg::SubmitDraft => {
                let kind = self
                    .menu_editor_state
                    .draft
                    .as_ref()
                    .map(|draft| draft.kind);
                match kind {
                    Some(crate::views::menu_editor::DraftKind::Page) => {
                        let label = t(self.ui_locale, "menuEditor.newSubmenu");
                        let _ = self.menu_editor_state.submit_submenu(&label);
                    }
                    Some(crate::views::menu_editor::DraftKind::Category) => {
                        let previous = self.menu_editor_state.clone();
                        if let Ok((name, is_new)) = self.menu_editor_state.submit_category()
                            && is_new
                            && let (Some(db), Some(project)) = (&self.db, &self.active_project)
                            && let Some(data_dir) = &self.data_dir
                            && let Err(error) =
                                engine::meta::add_category(db.conn(), data_dir, &project.id, &name)
                        {
                            self.menu_editor_state = previous;
                            self.notify(
                                ToastLevel::Error,
                                &tw(
                                    self.ui_locale,
                                    "menuEditor.categoryCreateFailed",
                                    &[("error", &error.to_string())],
                                ),
                            );
                        }
                    }
                    None => {}
                }
            }
            MenuEditorMsg::CancelDraft => {
                self.menu_editor_state.cancel_draft();
            }
            MenuEditorMsg::Move(direction) => {
                self.menu_editor_state.move_selected(direction);
            }
            MenuEditorMsg::Indent => {
                self.menu_editor_state.indent_selected();
            }
            MenuEditorMsg::Unindent => {
                self.menu_editor_state.unindent_selected();
            }
            MenuEditorMsg::Delete => {
                self.menu_editor_state.delete_selected();
            }
            MenuEditorMsg::Save => {
                if self.menu_editor_state.draft.is_some() {
                    self.notify(
                        ToastLevel::Warning,
                        &t(self.ui_locale, "menuEditor.finishDraft"),
                    );
                    return Task::none();
                }
                let Some(data_dir) = self.data_dir.clone() else {
                    return Task::none();
                };
                self.menu_editor_state.status = MenuEditorStatus::Saving;
                let result =
                    engine::menu::write_menu(&data_dir, &self.menu_editor_state.persisted_items())
                        .and_then(|()| engine::menu::read_menu(&data_dir));
                match result {
                    Ok(items) => {
                        let project_id = self
                            .menu_editor_state
                            .project_id
                            .clone()
                            .unwrap_or_default();
                        let pages = self.menu_editor_state.pages.clone();
                        let categories = self.menu_editor_state.categories.clone();
                        self.menu_editor_state =
                            MenuEditorState::from_persisted(project_id, items, pages, categories);
                        self.notify(ToastLevel::Success, &t(self.ui_locale, "menuEditor.saved"));
                    }
                    Err(error) => {
                        self.menu_editor_state.status = MenuEditorStatus::Ready;
                        self.menu_editor_state.error = Some(error.to_string());
                        self.notify(
                            ToastLevel::Error,
                            &tw(
                                self.ui_locale,
                                "menuEditor.saveFailed",
                                &[("error", &error.to_string())],
                            ),
                        );
                    }
                }
            }
            MenuEditorMsg::ToggleExpanded(id) => {
                if !self.menu_editor_state.collapsed.insert(id.clone()) {
                    self.menu_editor_state.collapsed.remove(&id);
                }
            }
            MenuEditorMsg::DragStart(id) => {
                if id != crate::views::menu_editor::HOME_ID {
                    self.menu_editor_state.selected_id = Some(id.clone());
                    self.menu_editor_state.dragging_id = Some(id);
                }
            }
            MenuEditorMsg::DragOver(id, position) => {
                self.menu_editor_state
                    .drag_over(id, position, std::time::Instant::now());
            }
            MenuEditorMsg::DragLeave(id) => {
                if self
                    .menu_editor_state
                    .drop_target
                    .as_ref()
                    .is_some_and(|(target, _)| target == &id)
                {
                    self.menu_editor_state.drop_target = None;
                    self.menu_editor_state.hover_expand = None;
                }
            }
            MenuEditorMsg::Drop => {
                if let (Some(dragged), Some((target, position))) = (
                    self.menu_editor_state.dragging_id.clone(),
                    self.menu_editor_state.drop_target.clone(),
                ) {
                    self.menu_editor_state
                        .drop_item(&dragged, &target, position);
                }
                self.menu_editor_state.dragging_id = None;
                self.menu_editor_state.drop_target = None;
                self.menu_editor_state.hover_expand = None;
            }
            MenuEditorMsg::DragCancel => {
                self.menu_editor_state.dragging_id = None;
                self.menu_editor_state.drop_target = None;
                self.menu_editor_state.hover_expand = None;
            }
            MenuEditorMsg::ExpandTick => {
                self.menu_editor_state
                    .expand_hovered(std::time::Instant::now());
            }
        }
        Task::none()
    }

    fn update_import_paths(
        &mut self,
        definition_id: &str,
        wxr_path: Option<&Path>,
        uploads_path: Option<&Path>,
    ) {
        let Some(db) = &self.db else { return };
        match engine::wordpress_import::update_definition(
            db.conn(),
            definition_id,
            None,
            wxr_path.map(Some),
            uploads_path.map(Some),
            wxr_path.map(|_| None),
        ) {
            Ok(definition) => {
                if let Some(state) = self.import_editors.get_mut(definition_id) {
                    state.definition = definition.clone();
                    if wxr_path.is_some() {
                        state.report = None;
                        state.result = None;
                    }
                }
                if let Some(item) = self
                    .sidebar_imports
                    .iter_mut()
                    .find(|item| item.id == definition_id)
                {
                    *item = definition;
                }
            }
            Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
        }
    }

    fn delete_import_definition(&mut self, definition_id: &str) -> Task<Message> {
        self.active_modal = None;
        let Some(db) = &self.db else {
            return Task::none();
        };
        match engine::wordpress_import::delete_definition(db.conn(), definition_id) {
            Ok(()) => {
                self.import_editors.remove(definition_id);
                self.sidebar_imports
                    .retain(|definition| definition.id != definition_id);
                self.close_entity_tab(definition_id);
                self.notify(
                    ToastLevel::Success,
                    &t(self.ui_locale, "import.toast.deleted"),
                );
            }
            Err(error) => self.notify(ToastLevel::Error, &error.to_string()),
        }
        Task::none()
    }

    fn persist_import_report(
        &self,
        definition_id: &str,
        report: &ImportReport,
    ) -> engine::EngineResult<ImportDefinition> {
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| engine::EngineError::Validation("database unavailable".to_string()))?;
        engine::wordpress_import::update_definition(
            db.conn(),
            definition_id,
            None,
            None,
            None,
            Some(Some(report)),
        )
    }

    fn start_import_analysis(&mut self, definition_id: &str) -> Task<Message> {
        let Some(definition) = self
            .import_editors
            .get(definition_id)
            .map(|state| state.definition.clone())
        else {
            return Task::none();
        };
        let Some(wxr_path) = definition.wxr_file_path.clone() else {
            if let Some(state) = self.import_editors.get_mut(definition_id) {
                state.error = Some(t(self.ui_locale, "import.error.wxrRequired"));
            }
            return Task::none();
        };
        let Some(project) = self
            .projects
            .iter()
            .find(|project| project.id == definition.project_id)
        else {
            return Task::none();
        };
        let Some(data_dir) = project.data_path.as_deref().map(PathBuf::from) else {
            return Task::none();
        };
        let state = self.import_editors.get_mut(definition_id).unwrap();
        state.is_analyzing = true;
        state.progress = None;
        state.error = None;
        state.result = None;
        let uploads = definition.uploads_folder_path.clone();
        let db_path = self.db_path.clone();
        let data_dir = data_dir.clone();
        let project_id = project.id.clone();
        let definition_id_owned = definition_id.to_string();
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        let worker_definition_id = definition_id_owned.clone();
        tokio::task::spawn_blocking(move || {
            let progress_sender = sender.clone();
            let result = (|| {
                let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                let report = engine::wordpress_import::analyze_wxr(
                    db.conn(),
                    &data_dir,
                    &project_id,
                    Path::new(&wxr_path),
                    uploads.as_deref().map(Path::new),
                    Some(&mut |progress| {
                        let _ =
                            progress_sender.unbounded_send(ImportAnalysisEvent::Progress(progress));
                    }),
                )
                .map_err(|error| error.to_string())?;
                let definition = engine::wordpress_import::update_definition(
                    db.conn(),
                    &worker_definition_id,
                    None,
                    None,
                    None,
                    Some(Some(&report)),
                )
                .map_err(|error| error.to_string())?;
                Ok((definition, report))
            })();
            let _ = sender.unbounded_send(ImportAnalysisEvent::Finished(Box::new(result)));
        });
        Task::run(receiver, move |event| Message::ImportAnalysisEvent {
            definition_id: definition_id_owned.clone(),
            event,
        })
    }

    fn start_import_execution(&mut self, definition_id: &str) -> Task<Message> {
        let Some(definition) = self
            .import_editors
            .get(definition_id)
            .map(|state| state.definition.clone())
        else {
            return Task::none();
        };
        let Some(report) = self
            .import_editors
            .get(definition_id)
            .and_then(|state| state.report.clone())
        else {
            return Task::none();
        };
        let Some(project) = self
            .projects
            .iter()
            .find(|project| project.id == definition.project_id)
        else {
            return Task::none();
        };
        let Some(data_dir) = project.data_path.as_deref().map(PathBuf::from) else {
            return Task::none();
        };
        let state = self.import_editors.get_mut(definition_id).unwrap();
        state.is_executing = true;
        state.progress = None;
        state.error = None;
        state.result = None;
        let db_path = self.db_path.clone();
        let data_dir = data_dir.clone();
        let project_id = project.id.clone();
        let default_author = engine::meta::read_project_json(&data_dir)
            .ok()
            .and_then(|metadata| metadata.default_author);
        let definition_id_owned = definition_id.to_string();
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        tokio::task::spawn_blocking(move || {
            let progress_sender = sender.clone();
            let result = (|| {
                let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                engine::wordpress_import::execute_import(
                    db.conn(),
                    &data_dir,
                    &project_id,
                    &report,
                    default_author.as_deref(),
                    Some(&mut |progress| {
                        let _ = progress_sender
                            .unbounded_send(ImportExecutionEvent::Progress(progress));
                    }),
                )
                .map_err(|error| error.to_string())
            })();
            let _ = sender.unbounded_send(ImportExecutionEvent::Finished(result));
        });
        Task::run(receiver, move |event| Message::ImportExecutionEvent {
            definition_id: definition_id_owned.clone(),
            event,
        })
    }

    fn start_import_auto_mapping(&mut self, definition_id: &str) -> Task<Message> {
        let Some(definition) = self
            .import_editors
            .get(definition_id)
            .map(|state| state.definition.clone())
        else {
            return Task::none();
        };
        let Some(mut report) = self
            .import_editors
            .get(definition_id)
            .and_then(|state| state.report.clone())
        else {
            return Task::none();
        };
        let Some(project) = self
            .projects
            .iter()
            .find(|project| project.id == definition.project_id)
        else {
            return Task::none();
        };
        let Some(data_dir) = project.data_path.as_deref().map(PathBuf::from) else {
            return Task::none();
        };
        let state = self.import_editors.get_mut(definition_id).unwrap();
        state.is_analyzing = true;
        state.error = None;
        let db_path = self.db_path.clone();
        let data_dir = data_dir.clone();
        let project_id = project.id.clone();
        let offline_mode = self.offline_mode;
        let definition_id_owned = definition_id.to_string();
        let result_definition_id = definition_id_owned.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                    let count = engine::wordpress_import::auto_map_taxonomy(
                        db.conn(),
                        &data_dir,
                        &project_id,
                        offline_mode,
                        &mut report,
                    )
                    .map_err(|error| error.to_string())?;
                    let definition = engine::wordpress_import::update_definition(
                        db.conn(),
                        &definition_id_owned,
                        None,
                        None,
                        None,
                        Some(Some(&report)),
                    )
                    .map_err(|error| error.to_string())?;
                    Ok((definition, report, count))
                })
                .await
                .map_err(|error| error.to_string())?
            },
            move |result| Message::ImportAutoMapFinished {
                definition_id: result_definition_id.clone(),
                result,
            },
        )
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

    fn start_metadata_diff(&mut self) -> Task<Message> {
        let (Some(project), Some(data_dir)) =
            (self.active_project.as_ref(), self.data_dir.as_ref())
        else {
            self.metadata_diff_state.error_message =
                Some(t(self.ui_locale, "engine.generateSiteNoProject"));
            return Task::none();
        };
        self.metadata_diff_state.is_running = true;
        self.metadata_diff_state.error_message = None;
        let db_path = self.db_path.clone();
        let project_id = project.id.clone();
        let data_dir = data_dir.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                    engine::metadata_diff::compute_metadata_diff(db.conn(), &data_dir, &project_id)
                        .map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| error.to_string())?
            },
            Message::MetadataDiffLoaded,
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

        self.site_validation_state.is_applying = true;
        self.site_validation_state.error_message = None;
        self.queue_site_generation(Some(report))
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
            self.sidebar_imports =
                engine::wordpress_import::list_definitions(db.conn(), &project.id)
                    .unwrap_or_default();

            // Read pico theme from project metadata for status bar badge
            if let Some(ref data_dir) = self.data_dir
                && let Ok(meta) = engine::meta::read_project_json(data_dir)
            {
                self.theme_badge = StyleViewState::new(meta.pico_theme.as_deref()).applied_theme;
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
                let trimmed = category.trim();
                if !trimmed.is_empty() {
                    *category_counts.entry(trimmed.to_string()).or_insert(0) += 1;
                }
            }
            for tag in &post.tags {
                let trimmed = tag.trim();
                if !trimmed.is_empty() {
                    *tag_counts.entry(trimmed.to_string()).or_insert(0) += 1;
                }
            }
        }

        let image_count = media
            .iter()
            .filter(|item| item.mime_type.starts_with("image/"))
            .count();
        let total_media_size = media.iter().map(|item| item.size).sum::<i64>();

        // Per bDS2: only the most recent 12 months that actually have posts.
        let timeline = monthly_counts
            .iter()
            .rev()
            .take(12)
            .map(|(&(year, month), &count)| DashboardTimelineMonth {
                label: month_abbreviation(month),
                year,
                count,
            })
            .rev()
            .collect::<Vec<_>>();

        let tag_colors = tags
            .into_iter()
            .filter_map(|tag| match tag.color {
                Some(color) if !color.is_empty() => Some((tag.name, color)),
                _ => None,
            })
            .collect::<HashMap<_, _>>();
        let distinct_tag_count = tag_counts.len();

        // Per bDS2: top 40 tags by count, font scaled 11-22px relative to the
        // min/max counts of the visible set, then sorted alphabetically.
        let mut tag_items = tag_counts.into_iter().collect::<Vec<_>>();
        tag_items.sort_by(|left, right| {
            right
                .1
                .cmp(&left.1)
                .then_with(|| left.0.to_lowercase().cmp(&right.0.to_lowercase()))
        });
        tag_items.truncate(40);
        let max_count = tag_items
            .iter()
            .map(|item| item.1)
            .max()
            .unwrap_or(1)
            .max(1);
        let min_count = tag_items
            .iter()
            .map(|item| item.1)
            .min()
            .unwrap_or(max_count);
        let range = (max_count - min_count).max(1) as f32;
        let mut tag_cloud = tag_items
            .into_iter()
            .map(|(name, count)| DashboardTag {
                font_size: 11.0 + (count - min_count) as f32 / range * 11.0,
                color: tag_colors.get(&name).cloned(),
                name,
                count,
            })
            .collect::<Vec<_>>();
        tag_cloud.sort_by_key(|tag| tag.name.to_lowercase());

        let mut category_cloud = category_counts
            .into_iter()
            .map(|(name, count)| DashboardCategory { name, count })
            .collect::<Vec<_>>();
        category_cloud.sort_by(|left, right| {
            right
                .count
                .cmp(&left.count)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });

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
                tag_count: distinct_tag_count,
                category_count: category_cloud.len(),
            },
            timeline,
            tag_cloud,
            category_cloud,
            recent_posts,
        }
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

    fn active_tab_type(&self) -> Option<TabType> {
        self.active_tab.as_ref().and_then(|id| {
            self.tabs
                .iter()
                .find(|tab| tab.id == *id)
                .map(|tab| tab.tab_type.clone())
        })
    }

    fn open_tab(&mut self, tab: Tab) -> Task<Message> {
        self.flush_active_post_editor();
        let index = tabs::open_tab(&mut self.tabs, tab);
        let mut semantic_post_id = None;
        if let Some(tab) = self.tabs.get(index).cloned() {
            self.active_tab = Some(tab.id.clone());
            if tab.tab_type == TabType::Post {
                semantic_post_id = Some(tab.id.clone());
            }
            self.load_editor_for_tab(&tab);
        }
        self.enforce_panel_tab_fallback();
        self.sync_menu_state();
        let mut tasks = vec![self.sync_embedded_previews()];
        if let Some(post_id) = semantic_post_id {
            tasks.push(Task::done(Message::LoadSemanticTagSuggestions(post_id)));
        }
        Task::batch(tasks)
    }

    fn find_next_in_active_editor(&mut self, query: &str) -> bool {
        let Some(id) = self.active_tab.clone() else {
            return false;
        };
        match self.active_tab_type() {
            Some(TabType::Post) => self
                .post_editors
                .get_mut(&id)
                .is_some_and(|state| state.editor_buffer.borrow_mut().find_next(query)),
            Some(TabType::Templates) => self
                .template_editors
                .get_mut(&id)
                .is_some_and(|state| state.editor_buffer.borrow_mut().find_next(query)),
            Some(TabType::Scripts) => self
                .script_editors
                .get_mut(&id)
                .is_some_and(|state| state.editor_buffer.borrow_mut().find_next(query)),
            _ => false,
        }
    }

    fn replace_current_in_active_editor(&mut self, query: &str, replacement: &str) -> bool {
        let Some(id) = self.active_tab.clone() else {
            return false;
        };
        let replaced = match self.active_tab_type() {
            Some(TabType::Post) => self.post_editors.get_mut(&id).is_some_and(|state| {
                let mut buffer = state.editor_buffer.borrow_mut();
                let replaced = replace_current_in_buffer(&mut buffer, query, replacement);
                state.content = buffer.text();
                state.is_dirty |= replaced;
                replaced
            }),
            Some(TabType::Templates) => self.template_editors.get_mut(&id).is_some_and(|state| {
                let mut buffer = state.editor_buffer.borrow_mut();
                let replaced = replace_current_in_buffer(&mut buffer, query, replacement);
                state.content = buffer.text();
                state.is_dirty |= replaced;
                replaced
            }),
            Some(TabType::Scripts) => self.script_editors.get_mut(&id).is_some_and(|state| {
                let mut buffer = state.editor_buffer.borrow_mut();
                let replaced = replace_current_in_buffer(&mut buffer, query, replacement);
                state.content = buffer.text();
                state.is_dirty |= replaced;
                replaced
            }),
            _ => false,
        };
        if replaced && let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) {
            tab.is_dirty = true;
        }
        replaced
    }

    fn replace_all_in_active_editor(&mut self, query: &str, replacement: &str) -> usize {
        let Some(id) = self.active_tab.clone() else {
            return 0;
        };
        let count = match self.active_tab_type() {
            Some(TabType::Post) => self.post_editors.get_mut(&id).map_or(0, |state| {
                let mut buffer = state.editor_buffer.borrow_mut();
                let count = buffer.replace_all(query, replacement);
                state.content = buffer.text();
                state.is_dirty |= count > 0;
                count
            }),
            Some(TabType::Templates) => self.template_editors.get_mut(&id).map_or(0, |state| {
                let mut buffer = state.editor_buffer.borrow_mut();
                let count = buffer.replace_all(query, replacement);
                state.content = buffer.text();
                state.is_dirty |= count > 0;
                count
            }),
            Some(TabType::Scripts) => self.script_editors.get_mut(&id).map_or(0, |state| {
                let mut buffer = state.editor_buffer.borrow_mut();
                let count = buffer.replace_all(query, replacement);
                state.content = buffer.text();
                state.is_dirty |= count > 0;
                count
            }),
            _ => 0,
        };
        if count > 0
            && let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id)
        {
            tab.is_dirty = true;
        }
        count
    }

    /// Synchronise menu enabled/disabled state with current app state.
    ///
    /// Called after state-changing operations (project switch, tab open/close,
    /// offline toggle) so that menu items reflect what's actually possible.
    fn sync_menu_state(&self) {
        let interactions_enabled = !self.search_index_rebuild_running;
        let active_tab_type = self
            .active_tab
            .as_ref()
            .and_then(|id| self.tabs.iter().find(|t| t.id == *id).map(|t| &t.tab_type));
        for &action in MenuAction::ALL {
            self.menu_registry.set_enabled(
                action,
                menu::action_enabled(
                    action,
                    self.active_project.is_some(),
                    active_tab_type,
                    self.offline_mode,
                    interactions_enabled,
                ),
            );
        }
        self.menu_registry
            .set_enabled(MenuAction::DisconnectServer, self.remote_client.is_some());
    }

    // ── Editor save/publish helpers ──

    fn save_post_editor(
        &mut self,
        post_id: &str,
        schedule_auto_translation: bool,
    ) -> Task<Message> {
        let saves_canonical_post = self
            .post_editors
            .get(post_id)
            .is_some_and(|editor| editor.active_language == editor.canonical_language);
        match self.persist_post_editor_state(post_id) {
            Ok(()) => {
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                if schedule_auto_translation && saves_canonical_post {
                    return self.schedule_post_auto_translation(post_id);
                }
            }
            Err(e) => self.notify_operation_failed("common.save", e),
        }
        Task::none()
    }

    fn enqueue_image_drop(&mut self, path: PathBuf) -> Task<Message> {
        let Some(post_id) = dropped_image_target(self.active_tab.as_deref(), &self.tabs, &path)
        else {
            return Task::none();
        };
        let (Some(project), Some(data_dir)) = (&self.active_project, &self.data_dir) else {
            return Task::none();
        };
        self.pending_image_drops.push_back(ImageDropRequest {
            post_id,
            project_id: project.id.clone(),
            data_dir: data_dir.clone(),
            source_language: self.content_language.clone(),
            offline_mode: self.offline_mode,
            path,
        });
        self.start_next_image_drop_import()
    }

    fn start_next_image_drop_import(&mut self) -> Task<Message> {
        if self.image_drop_import_running {
            return Task::none();
        }
        let Some(request) = self.pending_image_drops.pop_front() else {
            return Task::none();
        };
        self.image_drop_import_running = true;
        let name = request
            .path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| request.path.display().to_string());
        let label = tw(self.ui_locale, "editor.imageDropImport", &[("name", &name)]);
        let task_id = self.task_manager.submit(&label);
        self.refresh_task_snapshots();

        let db_path = self.db_path.clone();
        let task_manager = Arc::clone(&self.task_manager);
        let locale = self.ui_locale;
        let message_request = request.clone();
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    if !task_manager.wait_until_runnable(task_id) {
                        return Err("cancelled".to_string());
                    }
                    task_manager.report_progress(
                        task_id,
                        Some(0.0),
                        Some(tw(locale, "editor.imageDropProgress", &[("name", &name)])),
                    );
                    let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                    let sort_order =
                        engine::post_media::list_media_for_post(db.conn(), &request.post_id)
                            .map(|items| items.len() as i32)
                            .unwrap_or(0);
                    engine::gallery_import::import_and_link_image(
                        db.conn(),
                        &request.data_dir,
                        &request.project_id,
                        &request.post_id,
                        &request.path,
                        &request.source_language,
                        sort_order,
                    )
                    .map_err(|error| error.to_string())
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()))
            },
            move |result| Message::ImageDropImported {
                task_id,
                post_id: message_request.post_id.clone(),
                project_id: message_request.project_id.clone(),
                data_dir: message_request.data_dir.clone(),
                source_language: message_request.source_language.clone(),
                offline_mode: message_request.offline_mode,
                path: message_request.path.clone(),
                result,
            },
        )
    }

    fn finish_image_drop_import(
        &mut self,
        task_id: TaskId,
        request: ImageDropRequest,
        result: Result<Media, String>,
    ) -> Task<Message> {
        self.image_drop_import_running = false;
        let cancelled = self.task_manager.status(task_id) == Some(TaskStatus::Cancelled);
        let mut tasks = vec![self.start_next_image_drop_import()];
        match result {
            Ok(media) => {
                if !cancelled {
                    self.task_manager.complete(task_id);
                }
                let inserted = self
                    .post_editors
                    .get_mut(&request.post_id)
                    .map(|state| state.insert_dropped_image(&media.file_path))
                    .is_some();
                if inserted {
                    if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == request.post_id) {
                        tab.is_dirty = true;
                    }
                    if let Err(error) = self.persist_post_editor_state(&request.post_id) {
                        self.notify_operation_failed("common.save", error);
                    }
                }
                self.refresh_post_relationships(&request.post_id);
                let name = request
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| request.path.display().to_string());
                self.add_output(&tw(
                    self.ui_locale,
                    "editor.imageDropAdded",
                    &[("name", &name)],
                ));
                tasks.push(self.start_image_drop_enrichment(&request, media));
            }
            Err(error) if !cancelled => {
                self.task_manager.fail(task_id, error.clone());
                let path = request
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| request.path.display().to_string());
                self.add_output(&tw(
                    self.ui_locale,
                    "editor.imageDropFailed",
                    &[("path", &path), ("error", &error)],
                ));
            }
            Err(_) => {}
        }
        self.refresh_task_snapshots();
        tasks.push(self.refresh_sidebar_media());
        Task::batch(tasks)
    }

    fn start_image_drop_enrichment(
        &mut self,
        request: &ImageDropRequest,
        media: Media,
    ) -> Task<Message> {
        let Some(db) = self.db.as_ref() else {
            return Task::none();
        };
        if !engine::gallery_import::active_ai_endpoint_configured(db.conn(), request.offline_mode) {
            let key = if request.offline_mode {
                "editor.galleryAirplaneGated"
            } else {
                "chat.unavailable.guidance"
            };
            self.notify(ToastLevel::Warning, &t(self.ui_locale, key));
            return Task::none();
        }
        let metadata = engine::meta::read_project_json(&request.data_dir).ok();
        let translate = bds_core::db::queries::post::get_post_by_id(db.conn(), &request.post_id)
            .is_ok_and(|post| !post.do_not_translate);
        let targets = if translate {
            engine::gallery_import::translation_targets(
                metadata
                    .as_ref()
                    .and_then(|metadata| metadata.main_language.as_deref()),
                metadata
                    .as_ref()
                    .map(|metadata| metadata.blog_languages.as_slice())
                    .unwrap_or_default(),
                &request.source_language,
            )
        } else {
            Vec::new()
        };
        let name = request
            .path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| request.path.display().to_string());
        let label = tw(
            self.ui_locale,
            "editor.imageDropEnrichment",
            &[("name", &name)],
        );
        let task_id = self.task_manager.submit(&label);
        self.refresh_task_snapshots();
        let db_path = self.db_path.clone();
        let data_dir = request.data_dir.clone();
        let offline_mode = request.offline_mode;
        let post_id = request.post_id.clone();
        let path = request.path.clone();
        let task_manager = Arc::clone(&self.task_manager);
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    if !task_manager.wait_until_runnable(task_id) {
                        return Err("cancelled".to_string());
                    }
                    let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                    engine::gallery_import::enrich_imported_image(
                        db.conn(),
                        &data_dir,
                        &media,
                        offline_mode,
                        &targets,
                    )
                })
                .await
                .unwrap_or_else(|error| Err(error.to_string()))
            },
            move |result| Message::ImageDropEnriched {
                task_id,
                post_id: post_id.clone(),
                path: path.clone(),
                result,
            },
        )
    }

    fn finish_image_drop_enrichment(
        &mut self,
        task_id: TaskId,
        post_id: &str,
        path: &Path,
        result: Result<String, String>,
    ) -> Task<Message> {
        let cancelled = self.task_manager.status(task_id) == Some(TaskStatus::Cancelled);
        match result {
            Ok(_) if !cancelled => self.task_manager.complete(task_id),
            Err(error) if !cancelled => {
                self.task_manager.fail(task_id, error.clone());
                let path = path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string());
                self.add_output(&tw(
                    self.ui_locale,
                    "editor.imageDropEnrichmentFailed",
                    &[("path", &path), ("error", &error)],
                ));
            }
            _ => {}
        }
        self.refresh_task_snapshots();
        self.refresh_post_relationships(post_id);
        self.refresh_sidebar_media()
    }

    fn add_gallery_images(&mut self, post_id: &str) -> Task<Message> {
        let Some(db) = self.db.as_ref() else {
            self.add_output(&t(self.ui_locale, "common.databaseUnavailable"));
            return Task::none();
        };
        if self.offline_mode
            && !engine::gallery_import::active_ai_endpoint_configured(db.conn(), true)
        {
            self.notify(
                ToastLevel::Warning,
                &t(self.ui_locale, "editor.galleryAirplaneGated"),
            );
        }
        crate::platform::dialog::pick_gallery_images(
            post_id.to_string(),
            t(self.ui_locale, "editor.addGalleryImages"),
            t(self.ui_locale, "dialog.imageFilter"),
        )
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
                return self.schedule_post_auto_translation(post_id);
            }
            Err(e) => {
                self.notify_operation_failed("editor.publish", e);
            }
        }
        Task::none()
    }

    fn schedule_post_auto_translation(&mut self, post_id: &str) -> Task<Message> {
        let (Some(db), Some(data_dir)) = (self.db.as_ref(), self.data_dir.as_ref()) else {
            return Task::none();
        };
        let Ok(meta) = engine::meta::read_project_json(data_dir) else {
            return Task::none();
        };
        let Ok(post) = bds_core::db::queries::post::get_post_by_id(db.conn(), post_id) else {
            return Task::none();
        };
        let main_language = meta.main_language.unwrap_or_else(|| "en".to_string());
        let configured =
            engine::auto_translation::configured_languages(&main_language, &meta.blog_languages);
        let Ok(targets) =
            engine::auto_translation::missing_languages(db.conn(), &post, &configured)
        else {
            return Task::none();
        };
        if targets.is_empty() {
            return Task::none();
        }
        if engine::ai::active_endpoint(db.conn(), self.offline_mode).is_err() {
            return Task::none();
        }
        let post_id = post_id.to_string();
        let offline_mode = self.offline_mode;
        let locale = self.ui_locale;
        Task::batch(targets.into_iter().map(|language| {
            let post_id = post_id.clone();
            let configured = configured.clone();
            self.spawn_grouped_engine_task(
                "engine.autoTranslationStarted",
                "AI",
                move |db_path, _project_id, data_dir, task_manager, task_id| {
                    let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                    let report = engine::auto_translation::translate_missing_language_for_post(
                        db.conn(),
                        &data_dir,
                        &post_id,
                        &configured,
                        &language,
                        offline_mode,
                        move || task_manager.is_cancelled(task_id),
                    )
                    .map_err(|error| error.to_string())?;
                    Ok(tw(
                        locale,
                        "engine.autoTranslationComplete",
                        &[("count", &report.translated_posts.to_string())],
                    ))
                },
            )
        }))
    }

    fn ensure_post_editor_tag(&mut self, post_id: &str, name: &str) -> Task<Message> {
        let (Some(db), Some(data_dir)) = (self.db.as_ref(), self.data_dir.as_ref()) else {
            return Task::none();
        };
        let Ok(post) = bds_core::db::queries::post::get_post_by_id(db.conn(), post_id) else {
            return Task::none();
        };
        if bds_core::db::queries::tag::get_tag_by_project_and_name(
            db.conn(),
            &post.project_id,
            name,
        )
        .is_err()
            && let Err(error) =
                engine::tag::create_tag(db.conn(), data_dir, &post.project_id, name, None)
        {
            self.notify_operation_failed("editor.tags", error);
            return Task::none();
        }
        self.refresh_post_editor_tag_options();
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

        let mut ids = bds_core::db::fts::search_posts_filtered(
            db.conn(),
            query,
            &self.content_language,
            &filters,
        )
        .map(|results| results.post_ids)
        .unwrap_or_default();

        if let Some(data_dir) = &self.data_dir
            && let Ok(scores) = engine::embedding::EmbeddingService::production(db.conn(), data_dir)
                .compute_similarities(current_post_id, &ids)
        {
            ids.sort_by(|a, b| {
                scores
                    .get(b)
                    .copied()
                    .unwrap_or_default()
                    .total_cmp(&scores.get(a).copied().unwrap_or_default())
            });
        }

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

        let query = search_query.trim();
        if query.is_empty() {
            return bds_core::db::queries::media::list_media_filtered(
                db.conn(),
                &project.id,
                &Default::default(),
                24,
                0,
            )
            .unwrap_or_default();
        }

        let filters = bds_core::db::fts::MediaSearchFilters {
            project_id: Some(&project.id),
            limit: Some(24),
            ..Default::default()
        };
        bds_core::db::fts::search_media_filtered(db.conn(), query, &self.content_language, &filters)
            .map(|results| results.media_ids)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|media_id| {
                bds_core::db::queries::media::get_media_by_id(db.conn(), &media_id).ok()
            })
            .collect()
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
        self.save_post_editor(post_id, false)
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
            &media.file_path,
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

        let Some(ref data_dir) = self.data_dir else {
            return Task::none();
        };

        match save_template_editor_state_impl(db, data_dir, &project.id, state) {
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

        let Some(ref data_dir) = self.data_dir else {
            return Task::none();
        };

        match save_script_editor_state_impl(db, data_dir, &project.id, state) {
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

    fn unarchive_post_editor(&mut self, post_id: &str) -> Task<Message> {
        if let Err(error) = self.persist_post_editor_state(post_id) {
            self.notify_operation_failed("editor.unarchive", error);
            return Task::none();
        }
        let (Some(db), Some(data_dir)) = (self.db.as_ref(), self.data_dir.as_ref()) else {
            return Task::none();
        };
        match engine::post::unarchive_post(db.conn(), data_dir, post_id) {
            Ok(post) => {
                if let Some(editor) = self.post_editors.get_mut(post_id) {
                    editor.content = post.content.clone().unwrap_or_default();
                    editor.status = post.status;
                    editor.updated_at = post.updated_at;
                    editor.is_dirty = false;
                    editor.last_edit_at_ms = 0;
                }
                if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == post_id) {
                    tab.is_dirty = false;
                }
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.unarchived"));
                return self.refresh_sidebar_posts();
            }
            Err(error) => self.notify_operation_failed("editor.unarchive", error),
        }
        Task::none()
    }

    fn archive_post_editor(&mut self, post_id: &str) -> Task<Message> {
        if let Err(error) = self.persist_post_editor_state(post_id) {
            self.notify_operation_failed("editor.archive", error);
            return Task::none();
        }
        let (Some(db), Some(data_dir)) = (self.db.as_ref(), self.data_dir.as_ref()) else {
            return Task::none();
        };
        if let Err(error) = engine::post::archive_post(db.conn(), data_dir, post_id) {
            self.notify_operation_failed("editor.archive", error);
            return Task::none();
        }
        match bds_core::db::queries::post::get_post_by_id(db.conn(), post_id) {
            Ok(post) => {
                if let Some(editor) = self.post_editors.get_mut(post_id) {
                    editor.status = post.status;
                    editor.updated_at = post.updated_at;
                    editor.is_dirty = false;
                    editor.last_edit_at_ms = 0;
                }
                if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == post_id) {
                    tab.is_dirty = false;
                }
                self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.archived"));
                return self.refresh_sidebar_posts();
            }
            Err(error) => self.notify_operation_failed("editor.archive", error),
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

    /// Per editor_script.allium ScriptDeleteAction / action_patterns.allium
    /// confirmation_assignments: script_delete uses the confirm-delete modal
    /// showing the script title with no reference list.
    fn show_script_delete_confirmation(&mut self, script_id: &str) -> Task<Message> {
        let Some(db) = &self.db else {
            return Task::none();
        };
        let Ok(script) = bds_core::db::queries::script::get_script_by_id(db.conn(), script_id)
        else {
            return Task::none();
        };
        self.active_modal = Some(modal::ModalState::ConfirmDelete {
            entity_name: script.title,
            references: Vec::new(),
            on_confirm: modal::ConfirmAction::DeleteScript(script_id.to_string()),
        });
        Task::none()
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
                state.image_import_concurrency = meta.image_import_concurrency.to_string();
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
                        title: meta
                            .and_then(|value| value.title.clone())
                            .unwrap_or_else(|| name.clone()),
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
            if let Ok(Some(value)) =
                engine::settings::get_effective(db.conn(), "editor.default_mode")
            {
                state.default_mode = value;
            }
            if let Ok(Some(value)) =
                engine::settings::get_effective(db.conn(), "editor.diff_view_style")
            {
                state.diff_view_style = value;
            }
            if let Ok(Some(value)) =
                engine::settings::get_effective(db.conn(), "editor.wrap_long_lines")
            {
                state.wrap_long_lines = value == "true";
            }
            if let Ok(Some(value)) =
                engine::settings::get_effective(db.conn(), "editor.hide_unchanged_regions")
            {
                state.hide_unchanged_regions = value == "true";
            }
            if let Ok(setting) =
                bds_core::db::queries::setting::get_setting_by_key(db.conn(), "ai.system_prompt")
            {
                state.system_prompt = iced::widget::text_editor::Content::with_text(&setting.value);
            }
            if let Ok(ai_settings) = ai::load_ai_settings(db.conn(), self.offline_mode) {
                state.online_ai = Self::ai_mode_view_state(ai_settings.online);
                state.airplane_ai = Self::ai_mode_view_state(ai_settings.airplane);
            }
            state.mcp_enabled = engine::settings::get_effective(db.conn(), "mcp.http.enabled")
                .ok()
                .flatten()
                .is_some_and(|value| value == "true");
            state.mcp_running = self.mcp_server.is_some();
            state.mcp_endpoint = self
                .mcp_server
                .as_ref()
                .map(engine::mcp::McpHttpServer::endpoint)
                .unwrap_or_else(|| {
                    format!("http://127.0.0.1:{}/mcp", engine::mcp::DEFAULT_HTTP_PORT)
                });
            if let Some(project) = &self.active_project {
                state.mcp_proposals =
                    engine::mcp::list_pending_proposals(db.conn(), &project.id).unwrap_or_default();
            }
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            state.mcp_agents = engine::mcp::McpAgent::all()
                .into_iter()
                .map(|agent| crate::views::settings_view::SettingsMcpAgentRow {
                    agent,
                    label: agent.label().to_string(),
                    configured: engine::mcp::is_agent_configured(agent, &home),
                    config_path: engine::mcp::agent_config_path(agent, &home)
                        .to_string_lossy()
                        .into_owned(),
                })
                .collect();
        }
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

    fn hydrate_style_state(&self) -> Option<StyleViewState> {
        self.active_project.as_ref()?;
        let data_dir = self.data_dir.as_deref()?;
        let metadata = engine::meta::read_project_json(data_dir).ok()?;
        Some(StyleViewState::new(metadata.pico_theme.as_deref()))
    }

    fn handle_style_msg(&mut self, message: StyleMsg) -> Task<Message> {
        match message {
            StyleMsg::SelectTheme(theme) => {
                if let Some(state) = self.style_view_state.as_mut() {
                    state.select_theme(&theme);
                }
                self.sync_embedded_preview_for_style()
            }
            StyleMsg::PreviewModeChanged(mode) => {
                if let Some(state) = self.style_view_state.as_mut() {
                    state.set_preview_mode(mode);
                }
                self.sync_embedded_preview_for_style()
            }
            StyleMsg::Apply => {
                let Some(theme) = self
                    .style_view_state
                    .as_ref()
                    .filter(|state| state.can_apply())
                    .map(|state| state.selected_theme.clone())
                else {
                    return Task::none();
                };
                let result = (|| {
                    let db = self.db.as_ref().ok_or("database unavailable")?;
                    let data_dir = self
                        .data_dir
                        .as_deref()
                        .ok_or("project data directory unavailable")?;
                    let project = self.active_project.as_ref().ok_or("project unavailable")?;
                    let mut metadata = engine::meta::read_project_json(data_dir)
                        .map_err(|error| error.to_string())?;
                    metadata.pico_theme = Some(theme.clone());
                    engine::meta::update_project_metadata(db.conn(), data_dir, project, &metadata)
                        .map_err(|error| error.to_string())
                })();
                match result {
                    Ok(_) => {
                        if let Some(state) = self.style_view_state.as_mut() {
                            state.mark_applied();
                        }
                        self.theme_badge = theme;
                        self.notify(ToastLevel::Success, &t(self.ui_locale, "style.applied"));
                    }
                    Err(error) => self.notify_operation_failed("style.apply", error),
                }
                Task::none()
            }
        }
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
            SettingsMsg::ImageImportConcurrencyChanged(s) => {
                state.image_import_concurrency = s;
            }
            SettingsMsg::BlogmarkCategoryChanged(s) => {
                state.blogmark_category = s;
            }
            SettingsMsg::CopyBlogmarkBookmarklet => {
                if let Some(project) = &self.active_project {
                    let bookmarklet = engine::blogmark::bookmarklet(&project.id);
                    self.notify(
                        ToastLevel::Success,
                        &t(self.ui_locale, "settings.blogmarkBookmarkletCopied"),
                    );
                    return iced::clipboard::write(bookmarklet);
                }
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
                    let image_import_concurrency =
                        match state.image_import_concurrency.trim().parse::<i32>() {
                            Ok(value) => value.clamp(1, 8),
                            Err(_) => {
                                self.notify(
                                    ToastLevel::Error,
                                    &t(self.ui_locale, "settings.imageImportConcurrencyInvalid"),
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
                            image_import_concurrency: 4,
                            blogmark_category: None,
                            pico_theme: None,
                            semantic_similarity_enabled: false,
                            blog_languages: Vec::new(),
                        },
                    );
                    let semantic_was_enabled = meta.semantic_similarity_enabled;
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
                    meta.image_import_concurrency = image_import_concurrency;
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
                    match engine::meta::update_project_metadata(db.conn(), data_dir, project, &meta)
                    {
                        Ok(_) => {
                            let semantic_should_backfill =
                                state.semantic_similarity_enabled && !semantic_was_enabled;
                            if let Some(listing) =
                                self.projects.iter_mut().find(|p| p.id == project.id)
                            {
                                *listing = project.clone();
                            }
                            self.content_language = state.main_language.clone();
                            self.blog_languages = state.blog_languages.clone();
                            self.dashboard_state = Some(self.hydrate_dashboard_state());
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                            if semantic_should_backfill {
                                return Task::done(Message::EmbeddingBackfill);
                            }
                        }
                        Err(e) => self.notify_operation_failed("common.save", e),
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
                if let (Some(db), Some(data_dir), Some(project)) =
                    (&self.db, &self.data_dir, &self.active_project)
                {
                    match engine::meta::add_category(
                        db.conn(),
                        data_dir,
                        &project.id,
                        category_name,
                    ) {
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
                if let (Some(db), Some(data_dir), Some(project)) =
                    (&self.db, &self.data_dir, &self.active_project)
                    && let Some(row) = state.categories.iter().find(|row| row.name == name)
                {
                    let mut category_meta =
                        engine::meta::read_category_meta_json(data_dir).unwrap_or_default();
                    category_meta.insert(
                        row.name.clone(),
                        bds_core::model::metadata::CategorySettings {
                            title: Some(row.title.clone()).filter(|title| !title.trim().is_empty()),
                            render_in_lists: row.render_in_lists,
                            show_title: row.show_title,
                            post_template_slug: (!row.post_template_slug.is_empty())
                                .then(|| row.post_template_slug.clone()),
                            list_template_slug: (!row.list_template_slug.is_empty())
                                .then(|| row.list_template_slug.clone()),
                        },
                    );
                    match engine::meta::set_category_meta(
                        db.conn(),
                        data_dir,
                        &project.id,
                        &category_meta,
                    ) {
                        Ok(()) => {
                            self.dashboard_state = Some(self.hydrate_dashboard_state());
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                        }
                        Err(e) => self.notify_operation_failed("common.save", e),
                    }
                }
            }
            SettingsMsg::RemoveCategory(name) => {
                if let (Some(db), Some(data_dir), Some(project)) =
                    (&self.db, &self.data_dir, &self.active_project)
                {
                    match engine::meta::remove_category(db.conn(), data_dir, &project.id, &name) {
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
                if let (Some(db), Some(data_dir), Some(project)) =
                    (&self.db, &self.data_dir, &self.active_project)
                {
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
                                    title: Some(row.title.clone())
                                        .filter(|title| !title.trim().is_empty()),
                                    render_in_lists: row.render_in_lists,
                                    show_title: row.show_title,
                                    post_template_slug: None,
                                    list_template_slug: None,
                                },
                            )
                        })
                        .collect::<HashMap<_, _>>();
                    match engine::meta::set_categories_and_meta(
                        db.conn(),
                        data_dir,
                        &project.id,
                        &default_names,
                        &default_meta,
                    ) {
                        Ok(()) => {
                            self.settings_state = Some(self.hydrate_settings_state());
                            self.dashboard_state = Some(self.hydrate_dashboard_state());
                            self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                        }
                        Err(e) => self.notify_operation_failed("common.save", e),
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
                if let (Some(db), Some(data_dir), Some(project)) =
                    (&self.db, &self.data_dir, &self.active_project)
                {
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
                    match engine::meta::set_publishing_preferences(
                        db.conn(),
                        data_dir,
                        &project.id,
                        &prefs,
                    ) {
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
            SettingsMsg::AiEndpointUrlChanged(kind, value) => {
                Self::ai_mode_state_mut(state, kind).endpoint_url = value;
            }
            SettingsMsg::AiApiKeyChanged(kind, value) => {
                Self::ai_mode_state_mut(state, kind).api_key_input = value;
            }
            SettingsMsg::AiChatModelChanged(kind, value) => {
                let mode = Self::ai_mode_state_mut(state, kind);
                mode.chat_supports_tools = mode
                    .model_options
                    .iter()
                    .find(|option| option.id == value)
                    .is_some_and(|option| option.supports_tools);
                mode.chat_model = value;
            }
            SettingsMsg::AiTitleModelChanged(kind, value) => {
                Self::ai_mode_state_mut(state, kind).title_model = value;
            }
            SettingsMsg::AiImageModelChanged(kind, value) => {
                let mode = Self::ai_mode_state_mut(state, kind);
                mode.image_supports_vision = mode
                    .model_options
                    .iter()
                    .find(|option| option.id == value)
                    .is_some_and(|option| option.supports_vision);
                mode.image_model = value;
            }
            SettingsMsg::AiToolsChanged(kind, value) => {
                Self::ai_mode_state_mut(state, kind).chat_supports_tools = value;
            }
            SettingsMsg::AiVisionChanged(kind, value) => {
                Self::ai_mode_state_mut(state, kind).image_supports_vision = value;
            }
            SettingsMsg::RefreshAiModels(kind) => {
                if let Some(db) = &self.db {
                    match Self::refresh_ai_models(db, state, kind) {
                        Ok(()) => self.notify(
                            ToastLevel::Success,
                            &t(self.ui_locale, "settings.modelsLoaded"),
                        ),
                        Err(error) => self.notify_operation_failed("settings.refreshModels", error),
                    }
                }
            }
            SettingsMsg::TestAi(kind) => {
                if let Some(db) = &self.db {
                    match Self::test_ai_settings(db, state, kind) {
                        Ok(()) => self.notify(
                            ToastLevel::Success,
                            &t(self.ui_locale, "settings.testChatSuccess"),
                        ),
                        Err(error) => self.notify_operation_failed("settings.testChat", error),
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
            SettingsMsg::RebuildSearchIndex => return Task::done(Message::ReindexText),
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
                            || tm.is_cancelled(tid),
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
            SettingsMsg::InstallCli => {
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                match engine::cli_launcher::install_packaged_launcher(&home) {
                    Ok(path) => self.notify(
                        ToastLevel::Success,
                        &tw(
                            self.ui_locale,
                            "settings.cliInstalled",
                            &[("path", &path.to_string_lossy())],
                        ),
                    ),
                    Err(error) => {
                        self.notify_operation_failed("settings.installCli", error.to_string())
                    }
                }
            }
            SettingsMsg::McpEnabledChanged(enabled) => {
                let Some(db) = &self.db else {
                    return Task::none();
                };
                if enabled {
                    if self.mcp_server.is_none() {
                        match engine::mcp::McpHttpServer::start(
                            self.db_path.clone(),
                            engine::mcp::DEFAULT_HTTP_PORT,
                        ) {
                            Ok(server) => self.mcp_server = Some(server),
                            Err(error) => {
                                self.notify_operation_failed(
                                    "settings.mcpEnable",
                                    error.to_string(),
                                );
                                return Task::none();
                            }
                        }
                    }
                } else if let Some(server) = self.mcp_server.take()
                    && let Err(error) = server.stop()
                {
                    self.notify_operation_failed("settings.mcpEnable", error.to_string());
                    return Task::none();
                }
                match engine::settings::set(
                    db.conn(),
                    "mcp.http.enabled",
                    if enabled { "true" } else { "false" },
                ) {
                    Ok(()) => {
                        self.settings_state = Some(self.hydrate_settings_state());
                        self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                    }
                    Err(error) => {
                        self.notify_operation_failed("settings.mcpEnable", error.to_string())
                    }
                }
            }
            SettingsMsg::McpProposalAccepted(proposal_id) => {
                if let (Some(db), Some(data_dir)) = (&self.db, &self.data_dir) {
                    match engine::mcp::accept_proposal(db.conn(), data_dir, &proposal_id) {
                        Ok(_) => {
                            self.settings_state = Some(self.hydrate_settings_state());
                            self.notify(
                                ToastLevel::Success,
                                &t(self.ui_locale, "settings.mcpProposalApproved"),
                            );
                        }
                        Err(error) => {
                            self.notify_operation_failed("settings.mcpApprove", error.to_string())
                        }
                    }
                }
            }
            SettingsMsg::McpProposalRejected(proposal_id) => {
                if let (Some(db), Some(data_dir)) = (&self.db, &self.data_dir) {
                    match engine::mcp::reject_proposal(db.conn(), data_dir, &proposal_id) {
                        Ok(_) => {
                            self.settings_state = Some(self.hydrate_settings_state());
                            self.notify(
                                ToastLevel::Success,
                                &t(self.ui_locale, "settings.mcpProposalRejected"),
                            );
                        }
                        Err(error) => {
                            self.notify_operation_failed("settings.mcpReject", error.to_string())
                        }
                    }
                }
            }
            SettingsMsg::McpAgentToggled(agent) => {
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                let result = if engine::mcp::is_agent_configured(agent, &home) {
                    engine::mcp::remove_agent_config(agent, &home)
                } else {
                    engine::mcp::packaged_mcp_executable().and_then(|executable| {
                        engine::mcp::install_agent_config(agent, &home, &executable)
                    })
                };
                match result {
                    Ok(_) => {
                        self.settings_state = Some(self.hydrate_settings_state());
                        self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                    }
                    Err(error) => {
                        self.notify_operation_failed("settings.mcpAgents", error.to_string())
                    }
                }
            }
            SettingsMsg::McpRefresh => {
                self.settings_state = Some(self.hydrate_settings_state());
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

    fn project_tag_names(&self, project_id: &str) -> Vec<String> {
        let Some(db) = self.db.as_ref() else {
            return Vec::new();
        };
        let mut names = bds_core::db::queries::tag::list_tags_by_project(db.conn(), project_id)
            .unwrap_or_default()
            .into_iter()
            .map(|tag| tag.name)
            .collect::<Vec<_>>();
        for tag in bds_core::db::queries::post::list_posts_by_project(db.conn(), project_id)
            .unwrap_or_default()
            .into_iter()
            .flat_map(|post| post.tags)
        {
            if !names
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&tag))
            {
                names.push(tag);
            }
        }
        names.sort_by_key(|name| name.to_lowercase());
        names
    }

    fn refresh_post_editor_tag_options(&mut self) {
        let Some(project_id) = self
            .active_project
            .as_ref()
            .map(|project| project.id.clone())
        else {
            return;
        };
        let names = self.project_tag_names(&project_id);
        for editor in self.post_editors.values_mut() {
            editor.available_tags.clone_from(&names);
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
                            let mut editor = PostEditorState::from_post(
                                &post,
                                default_post_editor_mode(self.settings_state.as_ref()),
                                &self.blog_languages,
                                &translations,
                                outlinks,
                                backlinks,
                                linked_media,
                            );
                            editor.available_tags = self.project_tag_names(&post.project_id);
                            self.post_editors.insert(post.id.clone(), editor);
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
            TabType::Import => {
                if !self.import_editors.contains_key(&tab.id) {
                    match engine::wordpress_import::get_definition(db.conn(), &tab.id) {
                        Ok(definition) => {
                            let project_data_dir = self
                                .projects
                                .iter()
                                .find(|project| project.id == definition.project_id)
                                .and_then(|project| project.data_path.as_deref())
                                .map(PathBuf::from)
                                .or_else(|| self.data_dir.clone());
                            let categories = project_data_dir
                                .as_deref()
                                .and_then(|path| engine::meta::read_categories_json(path).ok())
                                .unwrap_or_default();
                            let tags = bds_core::db::queries::tag::list_tags_by_project(
                                db.conn(),
                                &definition.project_id,
                            )
                            .unwrap_or_default()
                            .into_iter()
                            .map(|tag| tag.name)
                            .collect();
                            self.import_editors.insert(
                                definition.id.clone(),
                                ImportEditorState::new(definition, categories, tags),
                            );
                        }
                        Err(error) => self.notify_operation_failed("activity.import", error),
                    }
                }
            }
            TabType::Chat => {
                if !self.chat_editors.contains_key(&tab.id) {
                    if self.settings_state.is_none() {
                        self.settings_state = Some(self.hydrate_settings_state());
                    }
                    match (
                        engine::chat::get_conversation(db.conn(), &tab.id),
                        engine::chat::list_messages(db.conn(), &tab.id),
                    ) {
                        (Ok(conversation), Ok(messages)) => {
                            let models = self.chat_model_options();
                            self.chat_editors.insert(
                                tab.id.clone(),
                                ChatEditorState::new(conversation, messages, models),
                            );
                        }
                        (Err(error), _) | (_, Err(error)) => {
                            self.notify(ToastLevel::Error, &error.to_string());
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
                self.settings_state = Some(self.hydrate_settings_state());
            }
            TabType::Style if self.style_view_state.is_none() => {
                self.style_view_state = self.hydrate_style_state();
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
        ai::save_endpoint_models(db.conn(), kind, &models).map_err(|error| error.to_string())?;
        let options = models
            .into_iter()
            .map(|model| AiModelOption {
                id: model.id,
                label: model.name,
                supports_tools: model.supports_tools,
                supports_vision: model.supports_vision,
            })
            .collect::<Vec<_>>();
        Self::ai_mode_state_mut(state, kind).model_options = options;
        Ok(())
    }

    fn test_ai_settings(
        _db: &Database,
        state: &SettingsViewState,
        kind: AiEndpointKind,
    ) -> Result<(), String> {
        let endpoint = Self::compose_ai_endpoint(state, kind)?;
        let mode = Self::ai_mode_state(state, kind);
        let models = [&mode.chat_model, &mode.title_model, &mode.image_model]
            .into_iter()
            .filter(|model| !model.trim().is_empty())
            .collect::<std::collections::BTreeSet<_>>();
        if models.is_empty() {
            return Err("select at least one model".to_string());
        }
        for model in models {
            ai::test_chat(&endpoint, model).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn save_ai_settings_state(db: &Database, state: &mut SettingsViewState) -> Result<(), String> {
        for kind in [AiEndpointKind::Online, AiEndpointKind::Airplane] {
            if Self::endpoint_has_configuration(state, kind) {
                let endpoint = Self::compose_ai_endpoint(state, kind)?;
                ai::save_endpoint(db.conn(), &endpoint).map_err(|error| error.to_string())?;
            }
            let mode = Self::ai_mode_state(state, kind);
            ai::save_model_preferences(
                db.conn(),
                kind,
                (!mode.title_model.trim().is_empty()).then_some(mode.title_model.as_str()),
                (!mode.image_model.trim().is_empty()).then_some(mode.image_model.as_str()),
                Some(mode.chat_supports_tools),
                Some(mode.image_supports_vision),
            )
            .map_err(|error| error.to_string())?;
        }
        ai::save_system_prompt(db.conn(), &state.system_prompt.text())
            .map_err(|error| error.to_string())?;
        for kind in [AiEndpointKind::Online, AiEndpointKind::Airplane] {
            let mode = Self::ai_mode_state_mut(state, kind);
            if !mode.api_key_input.trim().is_empty() {
                mode.api_key_configured = true;
            }
            mode.api_key_input.clear();
        }
        Ok(())
    }

    fn endpoint_has_configuration(state: &SettingsViewState, kind: AiEndpointKind) -> bool {
        let mode = Self::ai_mode_state(state, kind);
        !mode.endpoint_url.trim().is_empty()
            || !mode.chat_model.trim().is_empty()
            || !mode.api_key_input.trim().is_empty()
            || mode.api_key_configured
    }

    fn compose_ai_endpoint(
        state: &SettingsViewState,
        kind: AiEndpointKind,
    ) -> Result<AiEndpointConfig, String> {
        let mode = Self::ai_mode_state(state, kind);
        let input = mode.api_key_input.trim();
        let api_key = if !input.is_empty() {
            Some(input.to_string())
        } else if mode.api_key_configured {
            ai::load_endpoint_api_key(kind).map_err(|error| error.to_string())?
        } else {
            None
        };
        Ok(AiEndpointConfig {
            kind,
            url: mode.endpoint_url.trim().to_string(),
            model: mode.chat_model.trim().to_string(),
            api_key,
        })
    }

    fn ai_mode_state(state: &SettingsViewState, kind: AiEndpointKind) -> &AiModeViewState {
        match kind {
            AiEndpointKind::Online => &state.online_ai,
            AiEndpointKind::Airplane => &state.airplane_ai,
        }
    }

    fn ai_mode_state_mut(
        state: &mut SettingsViewState,
        kind: AiEndpointKind,
    ) -> &mut AiModeViewState {
        match kind {
            AiEndpointKind::Online => &mut state.online_ai,
            AiEndpointKind::Airplane => &mut state.airplane_ai,
        }
    }

    fn ai_mode_view_state(settings: ai::AiModeSettings) -> AiModeViewState {
        AiModeViewState {
            endpoint_url: settings.endpoint.url,
            chat_model: settings.endpoint.model,
            title_model: settings.title_model.unwrap_or_default(),
            image_model: settings.image_model.unwrap_or_default(),
            api_key_configured: settings.endpoint.api_key_configured,
            chat_supports_tools: settings.chat_supports_tools.unwrap_or(false),
            image_supports_vision: settings.image_supports_vision.unwrap_or(false),
            model_options: settings
                .models
                .into_iter()
                .map(|model| AiModelOption {
                    id: model.id,
                    label: model.name,
                    supports_tools: model.supports_tools,
                    supports_vision: model.supports_vision,
                })
                .collect(),
            ..Default::default()
        }
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
        let db = self
            .db
            .as_ref()
            .ok_or_else(|| t(self.ui_locale, "common.databaseUnavailable"))?;
        let post = bds_core::db::queries::post::get_post_by_id(db.conn(), post_id)
            .map_err(|error| error.to_string())?;
        let main_language = self
            .data_dir
            .as_deref()
            .and_then(|data_dir| engine::meta::read_project_json(data_dir).ok())
            .and_then(|metadata| metadata.main_language)
            .unwrap_or_else(|| language.clone());

        Ok(post_preview_url(&post, &language, &main_language))
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

    fn active_style_uses_embedded_preview(&self) -> bool {
        let active_is_style = self.active_tab.as_ref().is_some_and(|tab_id| {
            self.tabs
                .iter()
                .any(|tab| tab.id == *tab_id && tab.tab_type == TabType::Style)
        });
        active_is_style && self.active_project.is_some() && self.style_view_state.is_some()
    }

    fn hide_embedded_style_preview(&self) {
        if let Some(preview) = &self.embedded_style_preview {
            preview.controller.set_visible(false);
        }
    }

    fn sync_embedded_previews(&mut self) -> Task<Message> {
        let post = self.sync_embedded_preview_for_active_post();
        let style = self.sync_embedded_preview_for_style();
        Task::batch([post, style])
    }

    fn sync_embedded_preview_for_style(&mut self) -> Task<Message> {
        if !self.active_style_uses_embedded_preview() {
            self.hide_embedded_style_preview();
            return Task::none();
        }
        if let Err(error) = self.ensure_preview_server() {
            self.notify(ToastLevel::Error, &error);
            return Task::none();
        }
        let Some(url) = self
            .style_view_state
            .as_ref()
            .map(StyleViewState::preview_url)
        else {
            return Task::none();
        };

        if let Some(preview) = &mut self.embedded_style_preview {
            preview.current_url = Some(url.clone());
            if preview.controller.is_active() {
                preview.controller.navigate(&url);
                preview.controller.set_visible(true);
                return Task::none();
            }
        } else {
            self.embedded_style_preview = Some(EmbeddedPreviewState {
                controller: WebViewController::new(WebViewConfig::default().url(url.clone())),
                current_url: Some(url.clone()),
                creation_pending: false,
            });
        }

        let Some(window_id) = self.main_window_id else {
            return window::get_oldest().map(Message::MainWindowLoaded);
        };
        if let Some(preview) = &mut self.embedded_style_preview
            && should_start_embedded_preview_creation(
                preview.controller.is_active(),
                preview.creation_pending,
            )
        {
            preview.controller = WebViewController::new(WebViewConfig::default().url(url));
            preview.creation_pending = true;
            return preview
                .controller
                .create_task(window_id, Message::EmbeddedStylePreviewReady);
        }
        Task::none()
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
                creation_pending: false,
            });
        }

        let Some(window_id) = self.main_window_id else {
            return window::get_oldest().map(Message::MainWindowLoaded);
        };

        if let Some(preview) = &mut self.embedded_preview
            && should_start_embedded_preview_creation(
                preview.controller.is_active(),
                preview.creation_pending,
            )
        {
            preview.controller = WebViewController::new(WebViewConfig::default().url(url));
            preview.creation_pending = true;
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

    fn open_preview_in_browser(&mut self) -> Task<Message> {
        let active_post = active_post_tab_id(self.active_tab.as_deref(), &self.tabs);
        let url = match active_post {
            Some(post_id) => self.preview_url_for_post(&post_id),
            None => self
                .ensure_preview_server()
                .map(|()| format!("{}/", preview_base_url())),
        };
        match url {
            Ok(url) => {
                if let Err(error) = open::that(url) {
                    self.notify(ToastLevel::Error, &error.to_string());
                }
            }
            Err(error) => self.notify(ToastLevel::Error, &error),
        }
        Task::none()
    }

    fn start_one_shot_ai(
        &self,
        entity_id: String,
        action: OneShotAiAction,
        request: ai::OneShotRequest,
    ) -> Task<Message> {
        let db_path = self.db_path.clone();
        let offline_mode = self.offline_mode;
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    let db = Database::open(&db_path).map_err(|error| error.to_string())?;
                    ai::run_one_shot(db.conn(), offline_mode, &request)
                        .map(|(response, _usage)| response)
                        .map_err(|error| error.to_string())
                })
                .await
                .unwrap_or_else(|error| Err(format!("task panicked: {error}")))
            },
            move |result| Message::OneShotAiFinished {
                entity_id: entity_id.clone(),
                action: action.clone(),
                result,
            },
        )
    }

    fn run_post_ai_analysis(&mut self, post_id: &str) -> Task<Message> {
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
        if let Some(editor) = self.post_editors.get_mut(post_id) {
            editor.ai_activity = Some(t(self.ui_locale, "editor.aiAnalyze"));
        }
        self.start_one_shot_ai(post_id.to_string(), OneShotAiAction::PostAnalysis, request)
    }

    fn run_post_taxonomy_analysis(&mut self, post_id: &str) -> Task<Message> {
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
        if let Some(editor) = self.post_editors.get_mut(post_id) {
            editor.ai_activity = Some(t(self.ui_locale, "editor.suggestTaxonomy"));
        }
        self.start_one_shot_ai(post_id.to_string(), OneShotAiAction::PostTaxonomy, request)
    }

    fn detect_post_language(&mut self, post_id: &str) -> Task<Message> {
        let Some(state) = self.post_editors.get(post_id).cloned() else {
            return Task::none();
        };
        let request = ai::OneShotRequest {
            operation: ai::OneShotOperation::DetectLanguage,
            content: json!({
                "text": format!("{}\n\n{}\n\n{}", state.title, state.excerpt, content_sample(&state.content, 2000)),
            }),
        };
        if let Some(editor) = self.post_editors.get_mut(post_id) {
            editor.ai_activity = Some(t(self.ui_locale, "editor.detectLanguage"));
        }
        self.start_one_shot_ai(post_id.to_string(), OneShotAiAction::PostLanguage, request)
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
        if let Some(editor) = self.post_editors.get_mut(post_id) {
            editor.ai_activity = Some(t(self.ui_locale, "editor.translate"));
        }
        self.start_one_shot_ai(
            post_id.to_string(),
            OneShotAiAction::PostTranslation {
                target_language: target_language.to_string(),
            },
            request,
        )
    }

    fn run_media_ai_analysis(&mut self, media_id: &str) -> Task<Message> {
        let Some(_db) = &self.db else {
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
        let image_data_url = match engine::gallery_import::build_ai_image_data_url(
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
        if let Some(editor) = self.media_editors.get_mut(media_id) {
            editor.ai_activity = Some(t(self.ui_locale, "editor.aiAnalyze"));
        }
        self.start_one_shot_ai(
            media_id.to_string(),
            OneShotAiAction::MediaAnalysis,
            request,
        )
    }

    fn detect_media_language(&mut self, media_id: &str) -> Task<Message> {
        let Some(_db) = &self.db else {
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
        if let Some(editor) = self.media_editors.get_mut(media_id) {
            editor.ai_activity = Some(t(self.ui_locale, "editor.detectLanguage"));
        }
        self.start_one_shot_ai(
            media_id.to_string(),
            OneShotAiAction::MediaLanguage,
            request,
        )
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
        let Some(_db) = &self.db else {
            self.notify(
                ToastLevel::Error,
                &t(self.ui_locale, "common.databaseUnavailable"),
            );
            return Task::none();
        };
        let Some(state) = self.media_editors.get(media_id).cloned() else {
            return Task::none();
        };
        if let Some(editor) = self.media_editors.get_mut(media_id)
            && editor.language.is_empty()
        {
            editor.language = state.canonical_language.clone();
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
        if let Some(editor) = self.media_editors.get_mut(media_id) {
            editor.ai_activity = Some(t(self.ui_locale, "editor.translate"));
        }
        self.start_one_shot_ai(
            media_id.to_string(),
            OneShotAiAction::MediaTranslation {
                target_language: target_language.to_string(),
            },
            request,
        )
    }

    fn finish_one_shot_ai(
        &mut self,
        entity_id: &str,
        action: OneShotAiAction,
        result: Result<ai::OneShotResponse, String>,
    ) -> Task<Message> {
        if matches!(
            action,
            OneShotAiAction::PostAnalysis
                | OneShotAiAction::PostTaxonomy
                | OneShotAiAction::PostLanguage
                | OneShotAiAction::PostTranslation { .. }
        ) {
            if let Some(editor) = self.post_editors.get_mut(entity_id) {
                editor.ai_activity = None;
            }
        } else if let Some(editor) = self.media_editors.get_mut(entity_id) {
            editor.ai_activity = None;
        }

        let response = match result {
            Ok(response) => response,
            Err(error) => {
                self.notify(ToastLevel::Error, &error);
                return Task::none();
            }
        };

        match (action, response) {
            (OneShotAiAction::PostAnalysis, ai::OneShotResponse::PostAnalysis(result)) => {
                if let Some(state) = self.post_editors.get(entity_id).cloned() {
                    self.active_modal = Some(modal::ModalState::AISuggestions {
                        target: modal::AiEntityTarget::Post(entity_id.to_string()),
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
            }
            (OneShotAiAction::PostTaxonomy, ai::OneShotResponse::Taxonomy(result)) => {
                if let Some(state) = self.post_editors.get(entity_id).cloned() {
                    self.active_modal = Some(modal::ModalState::AISuggestions {
                        target: modal::AiEntityTarget::Post(entity_id.to_string()),
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
            }
            (OneShotAiAction::PostLanguage, ai::OneShotResponse::LanguageDetection(result)) => {
                if let Some(editor) = self.post_editors.get_mut(entity_id) {
                    editor.language = result.language_code;
                    editor.mark_dirty();
                }
                if let Err(error) = self.persist_post_editor_state(entity_id) {
                    self.notify(ToastLevel::Error, &error);
                } else {
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                }
            }
            (
                OneShotAiAction::PostTranslation { target_language },
                ai::OneShotResponse::Translation(result),
            ) => {
                if let Some(editor) = self.post_editors.get_mut(entity_id) {
                    editor.switch_language(&target_language);
                    editor.title = result.title.clone();
                    editor.excerpt = result.excerpt.clone();
                    editor.content = result.content.clone();
                    editor.editor_buffer =
                        std::cell::RefCell::new(bds_editor::EditorBuffer::new(&result.content));
                    editor.mark_dirty();
                }
                if let Err(error) = self.persist_post_editor_state(entity_id) {
                    self.notify(ToastLevel::Error, &error);
                } else {
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                }
            }
            (OneShotAiAction::MediaAnalysis, ai::OneShotResponse::ImageAnalysis(result)) => {
                if let Some(state) = self.media_editors.get(entity_id).cloned() {
                    self.active_modal = Some(modal::ModalState::AISuggestions {
                        target: modal::AiEntityTarget::Media(entity_id.to_string()),
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
            }
            (OneShotAiAction::MediaLanguage, ai::OneShotResponse::LanguageDetection(result)) => {
                if let Some(editor) = self.media_editors.get_mut(entity_id) {
                    editor.language = result.language_code;
                    editor.is_dirty = true;
                }
                if let Err(error) = self.persist_media_editor_state(entity_id) {
                    self.notify(ToastLevel::Error, &error);
                } else {
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                }
            }
            (
                OneShotAiAction::MediaTranslation { target_language },
                ai::OneShotResponse::MediaTranslation(result),
            ) => {
                if let Some(editor) = self.media_editors.get_mut(entity_id) {
                    editor.switch_language(&target_language);
                    editor.title = result.title.clone();
                    editor.alt = result.alt.clone();
                    editor.caption = result.caption.clone();
                    editor.is_dirty = true;
                }
                if let Err(error) = self.persist_media_editor_state(entity_id) {
                    self.notify(ToastLevel::Error, &error);
                } else {
                    self.notify(ToastLevel::Success, &t(self.ui_locale, "editor.saved"));
                }
            }
            _ => {}
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

impl Drop for BdsApp {
    fn drop(&mut self) {
        let _ = engine::embedding::EmbeddingService::flush_all();
    }
}

fn content_sample(content: &str, max_len: usize) -> String {
    content.chars().take(max_len).collect()
}

fn dropped_image_target(active_tab: Option<&str>, tabs: &[Tab], path: &Path) -> Option<String> {
    if !engine::media::is_supported_image_path(path) {
        return None;
    }
    let active_tab = active_tab?;
    tabs.iter()
        .find(|tab| tab.id == active_tab && tab.tab_type == TabType::Post)
        .map(|tab| tab.id.clone())
}

fn active_post_tab_id(active_tab: Option<&str>, tabs: &[Tab]) -> Option<String> {
    let active_tab = active_tab?;
    tabs.iter()
        .find(|tab| tab.id == active_tab && tab.tab_type == TabType::Post)
        .map(|tab| tab.id.clone())
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

fn localize_chat_error(locale: UiLocale, error: &str) -> String {
    if error.contains("AI unavailable - configure") {
        t(locale, "chat.unavailable.guidance")
    } else if let Some(detail) = error.strip_prefix("parse error: AI provider returned ") {
        format!("{} {detail}", t(locale, "chat.providerError"))
    } else {
        error.to_string()
    }
}

fn remote_error_closes_connection(code: &str) -> bool {
    code == "connection_lost"
}

#[cfg(test)]
mod tests {
    use super::{
        BdsApp, Message, POST_AUTO_SAVE_DELAY_MS, PersistedMediaState, PersistedPostState,
        PostStatus, SettingsMsg, active_post_tab_id, dropped_image_target, localize_chat_error,
        month_abbreviation, persist_media_editor_state_impl,
        persist_post_editor_preview_state_impl, persist_post_editor_state_impl,
        remote_error_closes_connection, save_editor_settings_state_impl,
        save_script_editor_state_impl, save_template_editor_state_impl,
        should_start_embedded_preview_creation,
    };
    use crate::i18n::t;
    use crate::platform::menu::MenuAction;
    use crate::state::ToastLevel;
    use crate::state::navigation::SidebarView;
    use crate::state::sidebar_filter::{MediaFilter, PostFilter};
    use crate::state::tabs::{Tab, TabType};
    use crate::views::chat_view::ChatEditorState;
    use crate::views::documentation::{DocumentLoad, DocumentationKind};
    use crate::views::media_editor::{MediaEditorMsg, MediaEditorState};
    use crate::views::menu_editor::{MenuEditorMsg, MenuEditorState, MenuEditorStatus};
    use crate::views::modal;
    use crate::views::post_editor::{PostEditorMsg, PostEditorState};
    use crate::views::script_editor::{ScriptEditorMsg, ScriptEditorState};
    use crate::views::settings_view::{AiModeViewState, SettingsSection, SettingsViewState};
    use crate::views::style_view::{PreviewMode, StyleMsg, StyleViewState};
    use crate::views::template_editor::TemplateEditorState;
    use bds_core::db::Database;
    use bds_core::db::fts::ensure_fts_tables;
    use bds_core::db::queries::project::insert_project;
    use bds_core::engine::generation::GenerationReport;
    use bds_core::engine::task::{TaskStatus, TaskStatus::*};
    use bds_core::engine::{
        ai, blogmark, chat, media, menu, meta, post, script, tag, template, wordpress_import,
    };
    use bds_core::i18n::UiLocale;
    use bds_core::model::{
        ChatRole, DomainEntity, DomainEvent, NotificationAction, Project, ScriptKind, TemplateKind,
    };
    use chrono::{Datelike, TimeZone};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::{Path, PathBuf};
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
        let db = Database::open_in_memory().unwrap();
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

    #[test]
    fn remote_connection_menu_opens_localized_selection_and_failure_states() {
        let (db, project, temp) = setup();
        let mut app = BdsApp::new_for_tests(db, project, temp.path().to_path_buf());
        let _ = app.dispatch_menu_action(MenuAction::ConnectServer);
        assert!(matches!(
            app.active_modal,
            Some(modal::ModalState::RemoteConnection {
                connected: false,
                ref projects,
                ..
            }) if projects.is_empty()
        ));

        let _ = app.update(Message::RemoteTargetChanged("not-a-target".into()));
        let _ = app.update(Message::RemoteConnectRequested);
        assert!(matches!(
            app.active_modal,
            Some(modal::ModalState::RemoteConnection {
                connecting: false,
                error: Some(ref error),
                ..
            }) if error == &t(UiLocale::En, "remoteConnection.invalidTarget")
        ));

        let _ = app.update(Message::RemoteConnected(Err("refused".into())));
        assert!(matches!(
            app.active_modal,
            Some(modal::ModalState::RemoteConnection {
                connected: false,
                error: Some(ref error),
                ..
            }) if error.contains("refused") && error != "refused"
        ));
    }

    #[test]
    fn only_transport_loss_closes_an_open_remote_session() {
        assert!(remote_error_closes_connection("connection_lost"));
        assert!(!remote_error_closes_connection("sync_watcher_error"));
        assert!(!remote_error_closes_connection("engine_error"));
    }

    #[test]
    fn imported_blogmark_activates_posts_and_opens_its_editor() {
        let (db, project, temp) = setup();
        ai::save_endpoint(
            db.conn(),
            &ai::AiEndpointConfig {
                kind: ai::AiEndpointKind::Airplane,
                url: "http://127.0.0.1:9".to_string(),
                model: "test".to_string(),
                api_key: None,
            },
        )
        .unwrap();
        let created = post::create_post(
            db.conn(),
            temp.path(),
            &project.id,
            "Saved From Browser",
            Some("[Saved From Browser](https://example.com/)"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let mut app = BdsApp::new_for_tests(db, project, temp.path().to_path_buf());
        let settings = SettingsViewState {
            default_mode: "preview".to_string(),
            ..Default::default()
        };
        app.settings_state = Some(settings);
        app.offline_mode = true;
        app.sidebar_view = SidebarView::Settings;
        app.sidebar_visible = false;
        let task_id = app.task_manager.submit("Importing blogmark");

        let _ = app.update(Message::BlogmarkImported {
            task_id,
            result: Ok(blogmark::BlogmarkImportResult {
                post: created.clone(),
                toasts: Vec::new(),
                transform_errors: Vec::new(),
            }),
        });

        assert_eq!(app.sidebar_view, SidebarView::Posts);
        assert!(app.sidebar_visible);
        assert_eq!(app.active_tab.as_deref(), Some(created.id.as_str()));
        assert!(app.post_editors.contains_key(&created.id));
        assert!(app.embedded_preview.is_some());
        assert!(
            app.task_manager
                .snapshots()
                .iter()
                .all(|task| task.group_name.as_deref() != Some("AI"))
        );
    }

    #[test]
    fn documentation_external_links_require_confirmation_and_api_help_opens_real_tab() {
        let (db, project, temp) = setup();
        let mut app = BdsApp::new_for_tests(db, project, temp.path().to_path_buf());

        let _ = app.update(Message::DocumentationLinkClicked(
            DocumentationKind::Guide,
            "https://example.com/guide".to_string(),
        ));
        assert!(matches!(
            app.active_modal,
            Some(modal::ModalState::Confirm {
                on_confirm: modal::ConfirmAction::OpenExternalUrl(ref url),
                ..
            }) if url == "https://example.com/guide"
        ));

        app.active_modal = None;
        let _ = app.update(Message::DocumentationLinkClicked(
            DocumentationKind::Guide,
            crate::views::documentation::API_DOCUMENTATION_URL.to_string(),
        ));
        assert!(
            app.tabs
                .iter()
                .any(|tab| tab.tab_type == TabType::ApiDocumentation)
        );
        assert_eq!(
            app.api_documentation.status,
            crate::views::documentation::DocumentStatus::Loading
        );
    }

    #[test]
    fn help_menu_opens_cli_and_mcp_documentation_tabs() {
        let (db, project, temp) = setup();
        let mut app = BdsApp::new_for_tests(db, project, temp.path().to_path_buf());

        let _ = app.dispatch_menu_action(MenuAction::OpenCliDocumentation);
        assert!(
            app.tabs
                .iter()
                .any(|tab| tab.tab_type == TabType::CliDocumentation)
        );
        assert_eq!(
            app.cli_documentation.status,
            crate::views::documentation::DocumentStatus::Loading
        );

        let _ = app.dispatch_menu_action(MenuAction::OpenMcpDocumentation);
        assert!(
            app.tabs
                .iter()
                .any(|tab| tab.tab_type == TabType::McpDocumentation)
        );
        assert_eq!(
            app.mcp_documentation.status,
            crate::views::documentation::DocumentStatus::Loading
        );
    }

    #[test]
    fn project_switch_keeps_global_documentation_loaded() {
        let (db, project, temp) = setup();
        let second_dir = temp.path().join("second-project");
        std::fs::create_dir_all(second_dir.join("meta")).unwrap();
        for name in [
            "project.json",
            "publishing.json",
            "categories.json",
            "category-meta.json",
        ] {
            std::fs::copy(
                temp.path().join("meta").join(name),
                second_dir.join("meta").join(name),
            )
            .unwrap();
        }
        let second = Project {
            id: "p2".to_string(),
            name: "Second Project".to_string(),
            slug: "second-project".to_string(),
            data_path: Some(second_dir.to_string_lossy().into_owned()),
            is_active: false,
            ..project.clone()
        };
        insert_project(db.conn(), &second).unwrap();
        let mut app = BdsApp::new_for_tests(db, project, temp.path().to_path_buf());
        app.projects.push(second.clone());
        app.tabs.push(Tab {
            id: "documentation".to_string(),
            tab_type: TabType::Documentation,
            title: "Documentation".to_string(),
            is_transient: false,
            is_dirty: false,
        });
        app.guide_documentation.apply(DocumentLoad::Ready {
            source: "# Global guide".to_string(),
            signature: 42,
        });

        let _ = app.update(Message::SwitchProject(second.id.clone()));

        assert_eq!(
            app.active_project.as_ref().map(|item| item.id.as_str()),
            Some("p2")
        );
        assert_eq!(app.data_dir.as_deref(), Some(second_dir.as_path()));
        assert_eq!(
            app.guide_documentation.status,
            crate::views::documentation::DocumentStatus::Ready
        );
        assert_eq!(app.guide_documentation.signature, 42);
    }

    #[test]
    fn menu_editor_persists_pages_new_categories_and_reload_roundtrip() {
        let (db, project, temp) = setup();
        let page = post::create_post(
            db.conn(),
            temp.path(),
            &project.id,
            "About",
            Some("About body"),
            Vec::new(),
            vec!["page".to_string()],
            None,
            None,
            None,
        )
        .unwrap();
        let mut app = BdsApp::new_for_tests(db, project, temp.path().to_path_buf());

        let _ = app.update(Message::MenuEditor(MenuEditorMsg::Reload));
        assert_eq!(app.menu_editor_state.status, MenuEditorStatus::Ready);
        assert!(
            app.menu_editor_state
                .pages
                .iter()
                .any(|item| item.id == page.id)
        );

        let _ = app.update(Message::MenuEditor(MenuEditorMsg::StartDraft(
            crate::views::menu_editor::DraftKind::Page,
        )));
        let _ = app.update(Message::MenuEditor(MenuEditorMsg::ChoosePage(page.id)));
        let _ = app.update(Message::MenuEditor(MenuEditorMsg::StartDraft(
            crate::views::menu_editor::DraftKind::Category,
        )));
        let _ = app.update(Message::MenuEditor(MenuEditorMsg::DraftChanged(
            "Long Form".to_string(),
        )));
        let _ = app.update(Message::MenuEditor(MenuEditorMsg::SubmitDraft));
        assert!(
            meta::read_categories_json(temp.path())
                .unwrap()
                .contains(&"Long Form".to_string())
        );
        assert!(
            meta::read_category_meta_json(temp.path())
                .unwrap()
                .contains_key("Long Form")
        );

        let _ = app.update(Message::MenuEditor(MenuEditorMsg::Save));
        let saved = menu::read_menu(temp.path()).unwrap();
        assert_eq!(saved[0].kind, menu::MenuItemKind::Home);
        assert!(
            saved
                .iter()
                .any(|item| item.kind == menu::MenuItemKind::Page
                    && item.slug.as_deref() == Some("about"))
        );
        assert!(
            saved
                .iter()
                .any(|item| item.kind == menu::MenuItemKind::CategoryArchive
                    && item.slug.as_deref() == Some("Long Form"))
        );
        assert!(!app.menu_editor_state.dirty);

        app.menu_editor_state = MenuEditorState::default();
        let _ = app.update(Message::MenuEditor(MenuEditorMsg::Reload));
        assert_eq!(app.menu_editor_state.items.len(), 3);
    }

    #[test]
    fn stale_documentation_load_cannot_overwrite_a_newer_guide_read() {
        let (db, project, temp) = setup();
        let mut app = BdsApp::new_for_tests(db, project, temp.path().to_path_buf());
        let stale = app.guide_documentation.start_loading();
        let current = app.guide_documentation.start_loading();

        let _ = app.update(Message::DocumentationLoaded(
            DocumentationKind::Guide,
            stale,
            DocumentLoad::Ready {
                source: "# Old guide".to_string(),
                signature: 1,
            },
        ));
        assert_eq!(
            app.guide_documentation.status,
            crate::views::documentation::DocumentStatus::Loading
        );

        let _ = app.update(Message::DocumentationLoaded(
            DocumentationKind::Guide,
            current,
            DocumentLoad::Ready {
                source: "# New guide".to_string(),
                signature: 2,
            },
        ));
        assert_eq!(
            app.guide_documentation.status,
            crate::views::documentation::DocumentStatus::Ready
        );
        assert_eq!(app.guide_documentation.signature, 2);
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

    #[test]
    fn import_definition_loads_saved_analysis_into_real_editor_state() {
        let (db, project, tempdir) = setup();
        let definition =
            wordpress_import::create_definition(db.conn(), &project.id, "Legacy").unwrap();
        let mut report = wordpress_import::empty_report();
        report.site.title = "Legacy Blog".to_string();
        report.source_file = tempdir.path().join("legacy.xml").display().to_string();
        wordpress_import::update_definition(
            db.conn(),
            &definition.id,
            None,
            None,
            None,
            Some(Some(&report)),
        )
        .unwrap();

        let mut app = BdsApp::new_for_tests(db, project, tempdir.path().to_path_buf());
        let _ = app.refresh_counts();
        assert_eq!(app.sidebar_imports.len(), 1);
        let tab = Tab {
            id: definition.id.clone(),
            tab_type: TabType::Import,
            title: "Legacy".to_string(),
            is_transient: true,
            is_dirty: false,
        };
        app.load_editor_for_tab(&tab);

        let editor = app.import_editors.get(&definition.id).unwrap();
        assert_eq!(editor.definition.name, "Legacy");
        assert_eq!(editor.report.as_ref().unwrap().site.title, "Legacy Blog");
        assert!(editor.category_options.iter().any(|name| name == "article"));
    }

    fn make_app(db: Database, project: Project, tmp: &TempDir) -> BdsApp {
        BdsApp::new_for_tests(db, project, tmp.path().to_path_buf())
    }

    #[test]
    fn airplane_mode_is_restored_and_status_bar_changes_are_persisted() {
        let (db, project, tmp) = setup();
        bds_core::engine::settings::set(
            db.conn(),
            bds_core::engine::settings::AIRPLANE_MODE_KEY,
            "true",
        )
        .unwrap();

        let mut app = make_app(db, project, &tmp);
        assert!(app.offline_mode);

        let _ = app.update(Message::SetOfflineMode(false));

        assert!(!app.offline_mode);
        assert_eq!(
            bds_core::engine::settings::get(
                app.db.as_ref().unwrap().conn(),
                bds_core::engine::settings::AIRPLANE_MODE_KEY
            )
            .unwrap()
            .as_deref(),
            Some("false")
        );
    }

    #[test]
    fn native_paste_menu_action_is_queued_for_the_focused_widget() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);

        let _ = app.dispatch_menu_action(MenuAction::Paste);

        assert_eq!(
            app.native_edit_commands.lock().unwrap().pop_front(),
            Some(crate::components::native_edit::EditCommand::Paste)
        );
    }

    fn enable_generation(tmp: &TempDir) {
        let mut metadata = bds_core::engine::meta::read_project_json(tmp.path()).unwrap();
        metadata.public_url = Some("https://example.com".to_string());
        bds_core::engine::meta::write_project_json(tmp.path(), &metadata).unwrap();
    }

    fn open_post_editor(app: &mut BdsApp, post: &bds_core::model::Post) {
        let tab = crate::state::tabs::Tab {
            id: post.id.clone(),
            tab_type: crate::state::tabs::TabType::Post,
            title: post.title.clone(),
            is_transient: false,
            is_dirty: false,
        };
        app.tabs.push(tab.clone());
        app.active_tab = Some(post.id.clone());
        app.load_editor_for_tab(&tab);
    }

    #[test]
    fn file_drop_routes_only_supported_images_to_the_active_post_tab() {
        let tabs = vec![
            Tab {
                id: "post-1".to_string(),
                tab_type: TabType::Post,
                title: "Post".to_string(),
                is_transient: false,
                is_dirty: false,
            },
            Tab {
                id: "settings".to_string(),
                tab_type: TabType::Settings,
                title: "Settings".to_string(),
                is_transient: false,
                is_dirty: false,
            },
        ];

        assert_eq!(
            dropped_image_target(Some("post-1"), &tabs, Path::new("photo.PNG")),
            Some("post-1".to_string())
        );
        assert_eq!(
            dropped_image_target(Some("post-1"), &tabs, Path::new("notes.txt")),
            None
        );
        assert_eq!(
            dropped_image_target(Some("settings"), &tabs, Path::new("photo.png")),
            None
        );
        assert_eq!(
            dropped_image_target(None, &tabs, Path::new("photo.png")),
            None
        );
    }

    #[test]
    fn browser_preview_targets_the_active_post_or_the_site_root() {
        let tabs = vec![
            Tab {
                id: "post-1".to_string(),
                tab_type: TabType::Post,
                title: "Post".to_string(),
                is_transient: false,
                is_dirty: false,
            },
            Tab {
                id: "settings".to_string(),
                tab_type: TabType::Settings,
                title: "Settings".to_string(),
                is_transient: false,
                is_dirty: false,
            },
        ];

        assert_eq!(
            active_post_tab_id(Some("post-1"), &tabs),
            Some("post-1".to_string())
        );
        assert_eq!(active_post_tab_id(Some("settings"), &tabs), None);
        assert_eq!(active_post_tab_id(None, &tabs), None);
    }

    #[test]
    fn multiple_file_drop_events_queue_one_managed_import_at_a_time() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Drops",
            Some("Body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let mut app = make_app(db, project, &tmp);
        open_post_editor(&mut app, &created);

        let _ = app.update(Message::FileDropped(tmp.path().join("first.png")));
        let _ = app.update(Message::FileDropped(tmp.path().join("second.png")));

        assert!(app.image_drop_import_running);
        assert_eq!(app.pending_image_drops.len(), 1);
        assert_eq!(app.task_manager.snapshots().len(), 1);
        assert!(app.task_manager.snapshots()[0].label.contains("first.png"));
    }

    #[test]
    fn completed_drop_is_linked_persisted_and_skips_unavailable_airplane_ai() {
        let (db, project, tmp) = setup();
        let project_id = project.id.clone();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project_id,
            "Drop",
            Some("Body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let source = tmp.path().join("dropped.png");
        std::fs::write(&source, tiny_png_bytes()).unwrap();
        let imported = bds_core::engine::gallery_import::import_and_link_image(
            db.conn(),
            tmp.path(),
            &project_id,
            &created.id,
            &source,
            "en",
            0,
        )
        .unwrap();
        let expected_url = format!("/{}", imported.file_path);
        let mut app = make_app(db, project, &tmp);
        open_post_editor(&mut app, &created);
        app.offline_mode = true;
        app.post_editors[&created.id]
            .editor_buffer
            .borrow_mut()
            .set_cursor(0, 2);
        let task_id = app.task_manager.submit("drop");

        let _ = app.update(Message::ImageDropImported {
            task_id,
            post_id: created.id.clone(),
            project_id,
            data_dir: tmp.path().to_path_buf(),
            source_language: "en".to_string(),
            offline_mode: true,
            path: source,
            result: Ok(imported.clone()),
        });

        let content = &app.post_editors[&created.id].content;
        assert!(content.contains(&format!("![]({expected_url})")));
        assert!(!content.contains("bds-media://"));
        let saved = bds_core::db::queries::post::get_post_by_id(
            app.db.as_ref().unwrap().conn(),
            &created.id,
        )
        .unwrap();
        assert_eq!(saved.content.as_deref(), Some(content.as_str()));
        assert_eq!(
            bds_core::engine::post_media::list_media_for_post(
                app.db.as_ref().unwrap().conn(),
                &created.id,
            )
            .unwrap()[0]
                .id,
            imported.id
        );
        assert_eq!(
            app.task_manager.status(task_id),
            Some(TaskStatus::Completed)
        );
        assert!(app.toasts.iter().any(|toast| {
            toast.level == ToastLevel::Warning
                && toast.message == "Automatic AI actions stay gated by airplane mode."
        }));
    }

    #[test]
    fn gallery_action_warns_but_keeps_import_available_without_a_local_endpoint() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Gallery",
            Some("Body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let mut app = make_app(db, project, &tmp);
        open_post_editor(&mut app, &created);
        app.offline_mode = true;
        app.post_editors
            .get_mut(&created.id)
            .unwrap()
            .quick_actions_open = true;

        let _ = app.handle_post_editor_msg(PostEditorMsg::AddGalleryImages);

        assert!(!app.post_editors[&created.id].quick_actions_open);
        assert!(app.toasts.iter().any(|toast| {
            toast.level == ToastLevel::Warning
                && toast.message == "Automatic AI actions stay gated by airplane mode."
        }));
    }

    #[test]
    fn git_network_actions_are_gated_in_airplane_mode() {
        let (db, project, tempdir) = setup();
        let mut app = BdsApp::new_for_tests(db, project, tempdir.path().to_path_buf());
        app.offline_mode = true;

        let _ = app.update(Message::GitFetch);

        assert!(app.git_state.network_run.is_none());
        assert!(
            app.toasts
                .iter()
                .any(|toast| toast.message == t(UiLocale::En, "git.airplaneBlocked"))
        );
    }

    #[test]
    fn gallery_completion_reports_every_path_inserts_macro_and_refreshes_editor() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Gallery",
            Some("Body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let mut app = make_app(db, project, &tmp);
        open_post_editor(&mut app, &created);

        let _ = app.update(Message::GalleryImportFinished {
            post_id: created.id.clone(),
            report: bds_core::engine::gallery_import::GalleryImportReport {
                selected_count: 2,
                outcomes: vec![
                    bds_core::engine::gallery_import::GalleryImportOutcome {
                        path: PathBuf::from("first.jpg"),
                        result: Ok(bds_core::engine::gallery_import::ImportedGalleryImage {
                            media_id: "m1".to_string(),
                            title: "First".to_string(),
                        }),
                    },
                    bds_core::engine::gallery_import::GalleryImportOutcome {
                        path: PathBuf::from("broken.jpg"),
                        result: Err("bad image".to_string()),
                    },
                ],
            },
        });

        assert!(
            app.post_editors[&created.id]
                .content
                .contains("\n[[gallery]]\n")
        );
        assert!(!app.post_editors[&created.id].is_dirty);
        let saved = bds_core::db::queries::post::get_post_by_id(
            app.db.as_ref().unwrap().conn(),
            &created.id,
        )
        .unwrap();
        assert!(saved.content.unwrap().contains("\n[[gallery]]\n"));
        assert_eq!(app.output_entries.len(), 3);
        assert_eq!(app.output_entries[0].text, "Added First");
        assert!(app.output_entries[1].text.contains("broken.jpg"));
        assert_eq!(app.output_entries[2].text, "Added 2 images to post");
    }

    #[test]
    fn cancelling_gallery_picker_is_a_no_op() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);

        let _ = app.update(Message::GalleryImagesPicked {
            post_id: "post1".to_string(),
            result: Ok(None),
        });

        assert!(app.output_entries.is_empty());
    }

    #[test]
    fn search_index_rebuild_requires_confirmation_and_blocks_editing() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);
        app.search_index_rebuild_required = true;

        let _ = app.update(Message::PostSearchChanged("query".to_string()));
        assert!(app.post_filter.search_query.is_empty());
        assert!(matches!(
            app.active_modal,
            Some(modal::ModalState::SearchIndexRepair)
        ));

        let _ = app.update(Message::ReindexText);
        assert!(matches!(
            app.active_modal,
            Some(modal::ModalState::SearchIndexRepair)
        ));

        let _ = app.update(Message::ConfirmModal(
            modal::ConfirmAction::RebuildSearchIndex,
        ));
        assert!(app.search_index_rebuild_running);
        assert!(matches!(
            app.active_modal,
            Some(modal::ModalState::SearchIndexRebuilding)
        ));
    }

    #[test]
    fn full_generation_queues_five_ordered_section_tasks_before_indexing() {
        let (db, project, tmp) = setup();
        enable_generation(&tmp);
        let mut app = make_app(db, project, &tmp);

        let _task = app.queue_site_generation(None);
        let snapshots = app.task_manager.snapshots();

        assert_eq!(
            snapshots
                .iter()
                .map(|task| task.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Render Site Core",
                "Render Single Posts",
                "Render Category Archives",
                "Render Tag Archives",
                "Render Date Archives",
            ]
        );
        assert!(
            snapshots
                .iter()
                .all(|task| task.group_name.as_deref() == Some("Render Site"))
        );
        assert_eq!(
            snapshots
                .iter()
                .map(|task| task.status.clone())
                .collect::<Vec<_>>(),
            vec![Running, Running, Running, Pending, Pending]
        );
    }

    #[test]
    fn search_index_is_queued_only_after_every_render_task_succeeds() {
        let (db, project, tmp) = setup();
        enable_generation(&tmp);
        let mut app = make_app(db, project, &tmp);
        let _task = app.queue_site_generation(None);
        let group_id = app.task_manager.snapshots()[0].group_id.clone().unwrap();
        let render_ids = app.site_generation_workflows[&group_id]
            .render_task_ids
            .clone();

        for (index, task_id) in render_ids.into_iter().enumerate() {
            let _task = app.handle_engine_message(Message::SiteGenerationSectionDone {
                group_id: group_id.clone(),
                task_id,
                result: Ok(GenerationReport::default()),
            });
            let has_index = app
                .task_manager
                .snapshots()
                .iter()
                .any(|task| task.label == "Build Search Index");
            assert_eq!(has_index, index == 4);
        }
    }

    #[test]
    fn failed_render_cancels_its_group_and_never_queues_index() {
        let (db, project, tmp) = setup();
        enable_generation(&tmp);
        let mut app = make_app(db, project, &tmp);
        let _task = app.queue_site_generation(None);
        let snapshots = app.task_manager.snapshots();
        let group_id = snapshots[0].group_id.clone().unwrap();
        let first_id = snapshots[0].id;

        let _task = app.handle_engine_message(Message::SiteGenerationSectionDone {
            group_id: group_id.clone(),
            task_id: first_id,
            result: Err("render failed".to_string()),
        });

        let snapshots = app.task_manager.snapshots();
        assert_eq!(
            app.task_manager.status(first_id),
            Some(TaskStatus::Failed("render failed".to_string()))
        );
        assert!(
            snapshots
                .iter()
                .filter(|task| task.group_id.as_deref() == Some(&group_id))
                .all(|task| matches!(task.status, TaskStatus::Failed(_) | TaskStatus::Cancelled))
        );
        assert!(
            !snapshots
                .iter()
                .any(|task| task.label == "Build Search Index")
        );
        assert!(!app.site_generation_workflows.contains_key(&group_id));
    }

    #[test]
    fn cancelling_a_render_task_cancels_the_generation_group() {
        let (db, project, tmp) = setup();
        enable_generation(&tmp);
        let mut app = make_app(db, project, &tmp);
        let _task = app.queue_site_generation(None);
        let snapshots = app.task_manager.snapshots();
        let group_id = snapshots[0].group_id.clone().unwrap();

        let _task = app.update(Message::CancelTask(snapshots[0].id));

        let snapshots = app.task_manager.snapshots();
        assert!(
            snapshots
                .iter()
                .filter(|task| task.group_id.as_deref() == Some(&group_id))
                .all(|task| task.status == TaskStatus::Cancelled)
        );
        assert!(
            !snapshots
                .iter()
                .any(|task| task.label == "Build Search Index")
        );
        assert!(!app.site_generation_workflows.contains_key(&group_id));
    }

    #[test]
    fn validation_apply_queues_only_affected_sections_then_index() {
        let (db, project, tmp) = setup();
        enable_generation(&tmp);
        let mut app = make_app(db, project, &tmp);
        let validation = bds_core::engine::validate_site::SiteValidationReport {
            stale_pages: vec!["2024/03/09/hello/index.html".to_string()],
            ..Default::default()
        };

        let _task = app.queue_site_generation(Some(validation));
        let snapshots = app.task_manager.snapshots();
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].label, "Render Single Posts");
        assert_eq!(
            snapshots[0].group_name.as_deref(),
            Some("Apply Site Validation")
        );
        let group_id = snapshots[0].group_id.clone().unwrap();

        let _task = app.handle_engine_message(Message::SiteGenerationSectionDone {
            group_id,
            task_id: snapshots[0].id,
            result: Ok(GenerationReport::default()),
        });
        assert!(
            app.task_manager
                .snapshots()
                .iter()
                .any(|task| task.label == "Build Search Index")
        );
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
    fn draft_editor_save_refreshes_live_outlinks_without_publishing() {
        let (db, project, tmp) = setup();
        let target = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Link Target",
            Some("Target body"),
            Vec::new(),
            Vec::new(),
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let source = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Link Source",
            Some("No links"),
            Vec::new(),
            Vec::new(),
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let mut app = make_app(db, project, &tmp);
        open_post_editor(&mut app, &source);
        let editor = app.post_editors.get_mut(&source.id).unwrap();
        editor.content = "[target](/2024/01/01/link-target)".to_string();
        editor.is_dirty = true;

        app.persist_post_editor_state(&source.id).unwrap();

        let editor = &app.post_editors[&source.id];
        assert_eq!(editor.status, PostStatus::Draft);
        assert_eq!(editor.outlinks.len(), 1);
        assert_eq!(editor.outlinks[0].post_id, target.id);
        assert_eq!(editor.outlinks[0].title, "Link Target");
    }

    #[test]
    fn manual_translation_save_reopens_and_refreshes_published_canonical_editor() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Canonical",
            Some("Canonical body"),
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
            "Kanonisch",
            None,
            Some("Deutscher Inhalt"),
        )
        .unwrap();
        let published = post::publish_post(db.conn(), tmp.path(), &created.id).unwrap();
        let mut app = make_app(db, project, &tmp);
        open_post_editor(&mut app, &published);
        let editor = app.post_editors.get_mut(&created.id).unwrap();
        editor.switch_language("de");
        editor.title = "Neu formuliert".to_string();
        editor.content = "Neuer Entwurf".to_string();
        editor.is_dirty = true;

        app.persist_post_editor_state(&created.id).unwrap();

        let reopened = bds_core::db::queries::post::get_post_by_id(
            app.db.as_ref().unwrap().conn(),
            &created.id,
        )
        .unwrap();
        assert_eq!(reopened.status, PostStatus::Draft);
        assert_eq!(reopened.content.as_deref(), Some("Canonical body"));

        let _ = app.update(Message::DomainEventsTick);
        let refreshed = &app.post_editors[&created.id];
        assert_eq!(refreshed.status, PostStatus::Draft);
        assert_eq!(refreshed.active_language, "de");
        assert_eq!(refreshed.title, "Neu formuliert");
        assert_eq!(refreshed.content, "Neuer Entwurf");
    }

    #[test]
    fn post_editor_publish_removes_a_divergent_old_post_file() {
        let (db, project, tmp) = setup();
        let mut created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Moved Through Editor",
            Some("Body"),
            Vec::new(),
            Vec::new(),
            None,
            Some("en"),
            None,
        )
        .unwrap();
        created.file_path = "posts/legacy/editor-old.md".to_string();
        bds_core::db::queries::post::update_post(db.conn(), &created).unwrap();
        let old_path = tmp.path().join(&created.file_path);
        std::fs::create_dir_all(old_path.parent().unwrap()).unwrap();
        std::fs::write(&old_path, "old file").unwrap();
        let mut app = make_app(db, project, &tmp);
        open_post_editor(&mut app, &created);

        let _ = app.publish_post_editor(&created.id);

        let published = bds_core::db::queries::post::get_post_by_id(
            app.db.as_ref().unwrap().conn(),
            &created.id,
        )
        .unwrap();
        assert_eq!(published.status, PostStatus::Published);
        assert_ne!(published.file_path, created.file_path);
        assert!(!old_path.exists());
        assert!(tmp.path().join(&published.file_path).is_file());
        assert_eq!(app.post_editors[&created.id].status, PostStatus::Published);
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
        bds_core::db::fts::drop_post_index(db.conn()).unwrap();

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
    fn preview_persist_keeps_archived_canonical_post_archived() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Archived",
            Some("Body"),
            Vec::new(),
            vec!["article".to_string()],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        post::archive_post(db.conn(), tmp.path(), &created.id).unwrap();
        let archived = bds_core::db::queries::post::get_post_by_id(db.conn(), &created.id).unwrap();
        let mut editor = PostEditorState::from_post(
            &archived,
            "preview",
            &["en".to_string()],
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        editor.title = "Changed while archived".to_string();

        let result = persist_post_editor_preview_state_impl(&db, &editor).unwrap();

        match result {
            PersistedPostState::Canonical(post) => {
                assert_eq!(post.status, PostStatus::Archived);
                assert_eq!(post.title, "Changed while archived");
            }
            PersistedPostState::Translation(_) => panic!("expected canonical post save"),
        }
    }

    #[test]
    fn post_editor_unarchive_action_restores_draft_and_body() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Archived",
            Some("File body"),
            Vec::new(),
            Vec::new(),
            None,
            Some("en"),
            None,
        )
        .unwrap();
        post::publish_post(db.conn(), tmp.path(), &created.id).unwrap();
        post::archive_post(db.conn(), tmp.path(), &created.id).unwrap();
        let archived = bds_core::db::queries::post::get_post_by_id(db.conn(), &created.id).unwrap();
        assert_eq!(archived.content, None);

        let mut app = make_app(db, project, &tmp);
        open_post_editor(&mut app, &archived);
        let _ = app.handle_post_editor_msg(PostEditorMsg::Unarchive);

        let from_db = bds_core::db::queries::post::get_post_by_id(
            app.db.as_ref().unwrap().conn(),
            &created.id,
        )
        .unwrap();
        assert_eq!(from_db.status, PostStatus::Draft);
        assert_eq!(from_db.content.as_deref(), Some("File body"));
        assert_eq!(app.post_editors[&created.id].status, PostStatus::Draft);
        assert_eq!(app.post_editors[&created.id].content, "File body");
        assert!(app.toasts.iter().any(|toast| {
            toast.level == ToastLevel::Success
                && toast.message == t(UiLocale::En, "editor.unarchived")
        }));
    }

    #[test]
    fn post_editor_archive_action_keeps_published_body_file_only() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Published",
            Some("File body"),
            Vec::new(),
            Vec::new(),
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let published = post::publish_post(db.conn(), tmp.path(), &created.id).unwrap();
        let path = tmp.path().join(&published.file_path);
        let original_file = std::fs::read(&path).unwrap();
        let mut app = make_app(db, project, &tmp);
        open_post_editor(&mut app, &published);
        app.post_editors
            .get_mut(&created.id)
            .unwrap()
            .quick_actions_open = true;

        let _ = app.handle_post_editor_msg(PostEditorMsg::Archive);

        let from_db = bds_core::db::queries::post::get_post_by_id(
            app.db.as_ref().unwrap().conn(),
            &created.id,
        )
        .unwrap();
        assert_eq!(from_db.status, PostStatus::Archived);
        assert_eq!(from_db.content, None);
        assert_eq!(std::fs::read(path).unwrap(), original_file);
        assert_eq!(app.post_editors[&created.id].status, PostStatus::Archived);
        assert!(!app.post_editors[&created.id].quick_actions_open);
        assert!(app.toasts.iter().any(|toast| {
            toast.level == ToastLevel::Success
                && toast.message == t(UiLocale::En, "editor.archived")
        }));
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
        bds_core::db::fts::drop_post_index(db.conn()).unwrap();

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
            airplane_ai: AiModeViewState {
                endpoint_url: spawn_models_server(),
                chat_model: "llama3.2".to_string(),
                ..Default::default()
            },
            system_prompt: iced::widget::text_editor::Content::with_text("Use JSON only."),
            ..SettingsViewState::default()
        };

        BdsApp::save_ai_settings_state(&db, &mut state).unwrap();

        let settings = ai::load_ai_settings(db.conn(), false).unwrap();
        assert!(settings.online.endpoint.url.is_empty());
        assert!(settings.online.endpoint.model.is_empty());
        assert_eq!(
            settings.airplane.endpoint.url,
            state.airplane_ai.endpoint_url
        );
        assert_eq!(settings.airplane.endpoint.model, "llama3.2");
        assert_eq!(settings.system_prompt.trim_end(), "Use JSON only.");
    }

    #[test]
    fn settings_change_rehydrates_ai_configuration() {
        let (db, project, tmp) = setup();
        bds_core::engine::settings::set(
            db.conn(),
            "ai.endpoint.online.url",
            "http://127.0.0.1:9000/v1",
        )
        .unwrap();
        bds_core::engine::settings::set(
            db.conn(),
            "ai.endpoint.online.model",
            "mlx-community--gemma-4-12B-8bit",
        )
        .unwrap();
        bds_core::engine::settings::set(db.conn(), "ai.endpoint.online.api_key_configured", "true")
            .unwrap();
        let mut app = make_app(db, project, &tmp);
        app.tabs.push(Tab {
            id: "settings".to_string(),
            tab_type: TabType::Settings,
            title: "Settings".to_string(),
            is_transient: false,
            is_dirty: false,
        });
        let mut settings_state = SettingsViewState::default();
        settings_state.focus_section(SettingsSection::AI);
        app.settings_state = Some(settings_state);

        app.handle_domain_event(DomainEvent::SettingsChanged {
            project_id: None,
            key: "ai.endpoint.online.url".to_string(),
        });

        let state = app.settings_state.as_ref().unwrap();
        assert_eq!(state.online_ai.endpoint_url, "http://127.0.0.1:9000/v1");
        assert_eq!(
            state.online_ai.chat_model,
            "mlx-community--gemma-4-12B-8bit"
        );
        assert!(state.online_ai.api_key_configured);
        assert_eq!(state.active_section, Some(SettingsSection::AI));
    }

    #[test]
    fn ai_unavailable_chat_errors_are_localized() {
        let raw = "validation error: AI unavailable - configure online endpoint in Settings";
        for locale in UiLocale::all() {
            assert_eq!(
                localize_chat_error(*locale, raw),
                t(*locale, "chat.unavailable.guidance")
            );
        }
        let provider =
            "parse error: AI provider returned 400 Bad Request: tokenizer.chat_template is not set";
        for locale in UiLocale::all() {
            assert_eq!(
                localize_chat_error(*locale, provider),
                format!(
                    "{} 400 Bad Request: tokenizer.chat_template is not set",
                    t(*locale, "chat.providerError")
                )
            );
        }
        assert_eq!(
            localize_chat_error(UiLocale::En, "connection refused"),
            "connection refused"
        );
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
        let (db, project, tmp) = setup();
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
        editor.slug = " Updated Template! ".to_string();
        editor.content = "<main>{{ title }}</main>".to_string();

        let saved_template =
            save_template_editor_state_impl(&db, tmp.path(), &project.id, &editor).unwrap();
        assert_eq!(saved_template.title, "Updated Template");
        assert_eq!(saved_template.slug, "updated-template");

        let saved =
            bds_core::db::queries::template::get_template_by_id(db.conn(), &created.id).unwrap();
        assert_eq!(saved.title, "Updated Template");
        assert_eq!(saved.slug, "updated-template");
        assert_eq!(saved.content.as_deref(), Some("<main>{{ title }}</main>"));
    }

    #[test]
    fn template_editor_save_rejects_invalid_content() {
        let (db, project, tmp) = setup();
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

        let error =
            save_template_editor_state_impl(&db, tmp.path(), &project.id, &editor).unwrap_err();
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
        let (db, project, tmp) = setup();
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
        editor.slug = " Updated Script! ".to_string();
        editor.content = "function main()\n  return 'lua'\nend".to_string();
        editor.entrypoint = "main".to_string();

        let saved_script =
            save_script_editor_state_impl(&db, tmp.path(), &project.id, &editor).unwrap();
        assert_eq!(saved_script.title, "Updated Script");
        assert_eq!(saved_script.slug, "updated-script");

        let saved =
            bds_core::db::queries::script::get_script_by_id(db.conn(), &created.id).unwrap();
        assert_eq!(saved.title, "Updated Script");
        assert_eq!(saved.slug, "updated-script");
        assert_eq!(
            saved.content.as_deref(),
            Some("function main()\n  return 'lua'\nend")
        );
    }

    #[test]
    fn script_editor_save_rejects_invalid_content() {
        let (db, project, tmp) = setup();
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

        let error =
            save_script_editor_state_impl(&db, tmp.path(), &project.id, &editor).unwrap_err();
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
    fn post_preview_url_uses_the_generated_site_route() {
        let (db, project, tmp) = setup();
        let created = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Preview Route",
            Some("Body"),
            Vec::new(),
            Vec::new(),
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let path = bds_core::render::build_canonical_post_path(&created, "de", "en");

        assert_eq!(
            super::post_preview_url(&created, "de", "en"),
            format!(
                "http://127.0.0.1:4123{path}?draft=true&post_id={}",
                created.id
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
    fn style_selection_and_preview_mode_stay_local_until_apply() {
        let mut state = StyleViewState::new(Some("blue"));

        assert_eq!(state.selected_theme, "blue");
        assert_eq!(state.applied_theme, "blue");
        assert!(!state.can_apply());

        state.select_theme("green");
        state.set_preview_mode(PreviewMode::Dark);

        assert_eq!(state.selected_theme, "green");
        assert_eq!(state.applied_theme, "blue");
        assert_eq!(state.preview_mode, PreviewMode::Dark);
        assert!(state.can_apply());

        state.mark_applied();
        assert!(!state.can_apply());
    }

    #[test]
    fn embedded_preview_creation_starts_only_when_idle() {
        assert!(should_start_embedded_preview_creation(false, false));
        assert!(!should_start_embedded_preview_creation(false, true));
        assert!(!should_start_embedded_preview_creation(true, false));
    }

    #[test]
    fn applying_style_persists_project_metadata_emits_event_and_updates_badge() {
        let (db, project, tmp) = setup();
        let events = bds_core::engine::domain_events::subscribe();
        let mut app = make_app(db, project, &tmp);
        app.style_view_state = Some(StyleViewState::new(None));

        let _ = app.update(Message::Style(StyleMsg::SelectTheme("purple".to_string())));
        let _ = app.update(Message::Style(StyleMsg::Apply));

        let metadata = bds_core::engine::meta::read_project_json(tmp.path()).unwrap();
        assert_eq!(metadata.pico_theme.as_deref(), Some("purple"));
        let project_json = std::fs::read_to_string(tmp.path().join("meta/project.json")).unwrap();
        assert!(project_json.contains("\"picoTheme\": \"purple\""));
        assert!(!project_json.contains("previewMode"));
        assert_eq!(app.theme_badge, "purple");
        assert_eq!(
            app.style_view_state.as_ref().unwrap().applied_theme,
            "purple"
        );
        assert!(events.drain().iter().any(|event| matches!(
            event,
            DomainEvent::EntityChanged {
                project_id,
                entity: bds_core::model::DomainEntity::Project,
                entity_id,
                action: bds_core::model::NotificationAction::Updated,
            } if project_id == "p1" && entity_id == "p1"
        )));
    }

    #[test]
    fn task_tick_autosaves_dirty_post_editor() {
        let (db, project, tmp) = setup();
        ai::save_endpoint(
            db.conn(),
            &ai::AiEndpointConfig {
                kind: ai::AiEndpointKind::Airplane,
                url: "http://127.0.0.1:9".to_string(),
                model: "test".to_string(),
                api_key: None,
            },
        )
        .unwrap();
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
        app.offline_mode = true;
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
        assert!(
            app.task_manager
                .snapshots()
                .iter()
                .all(|task| task.group_name.as_deref() != Some("AI"))
        );
    }

    fn auto_translation_test_app(translated_languages: &[&str]) -> (BdsApp, String, TempDir) {
        let (db, project, tmp) = setup();
        let mut metadata = bds_core::engine::meta::read_project_json(tmp.path()).unwrap();
        metadata.main_language = Some("en".to_string());
        metadata.blog_languages = vec!["en".to_string(), "de".to_string(), "fr".to_string()];
        bds_core::engine::meta::write_project_json(tmp.path(), &metadata).unwrap();
        ai::save_endpoint(
            db.conn(),
            &ai::AiEndpointConfig {
                kind: ai::AiEndpointKind::Airplane,
                url: "http://127.0.0.1:9".to_string(),
                model: "test".to_string(),
                api_key: None,
            },
        )
        .unwrap();
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
        for language in translated_languages {
            post::upsert_translation(
                db.conn(),
                tmp.path(),
                &created.id,
                language,
                "Translated",
                None,
                Some("Body"),
            )
            .unwrap();
        }
        let editor_post =
            bds_core::db::queries::post::get_post_by_id(db.conn(), &created.id).unwrap();
        let translations = bds_core::db::queries::post_translation::list_post_translations_by_post(
            db.conn(),
            &created.id,
        )
        .unwrap();
        let editor = PostEditorState::from_post(
            &editor_post,
            "markdown",
            &["en".to_string(), "de".to_string(), "fr".to_string()],
            &translations,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let mut app = make_app(db, project, &tmp);
        app.offline_mode = true;
        app.post_editors.insert(created.id.clone(), editor);
        app.tabs.push(crate::state::tabs::Tab {
            id: created.id.clone(),
            tab_type: crate::state::tabs::TabType::Post,
            title: created.title,
            is_transient: false,
            is_dirty: false,
        });
        (app, created.id, tmp)
    }

    #[test]
    fn first_manual_save_enqueues_one_task_per_missing_translation() {
        let (mut app, post_id, _tmp) = auto_translation_test_app(&[]);

        let _ = app.save_post_editor(&post_id, true);

        assert_eq!(
            app.task_manager
                .snapshots()
                .iter()
                .filter(|task| task.group_name.as_deref() == Some("AI"))
                .count(),
            2
        );
    }

    #[test]
    fn later_manual_save_enqueues_no_translation_task_when_languages_exist() {
        let (mut app, post_id, _tmp) = auto_translation_test_app(&["de", "fr"]);

        let _ = app.save_post_editor(&post_id, true);

        assert!(
            app.task_manager
                .snapshots()
                .iter()
                .all(|task| task.group_name.as_deref() != Some("AI"))
        );
    }

    #[test]
    fn saving_a_translation_draft_does_not_enqueue_other_languages() {
        let (mut app, post_id, _tmp) = auto_translation_test_app(&["de"]);
        app.post_editors
            .get_mut(&post_id)
            .unwrap()
            .switch_language("de");

        let _ = app.save_post_editor(&post_id, true);

        assert!(
            app.task_manager
                .snapshots()
                .iter()
                .all(|task| task.group_name.as_deref() != Some("AI"))
        );
    }

    #[test]
    fn inserting_editor_markdown_persists_without_enqueuing_translations() {
        let (mut app, post_id, _tmp) = auto_translation_test_app(&[]);

        let _ = app.insert_markdown_into_post(&post_id, "[Link](/target)");

        assert!(
            app.task_manager
                .snapshots()
                .iter()
                .all(|task| task.group_name.as_deref() != Some("AI"))
        );
    }

    #[test]
    fn post_editor_loads_existing_project_tags_for_partial_suggestions() {
        let (db, project, tmp) = setup();
        tag::create_tag(db.conn(), tmp.path(), &project.id, "Photography", None).unwrap();
        let tagged = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Existing",
            Some("Body"),
            vec!["Photo Essay".to_string()],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let edited = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Edited",
            Some("Body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let mut app = make_app(db, project, &tmp);

        let _ = app.open_tab(Tab {
            id: edited.id.clone(),
            tab_type: TabType::Post,
            title: edited.title,
            is_transient: false,
            is_dirty: false,
        });

        assert_eq!(
            app.post_editors[&edited.id].available_tags,
            vec!["Photo Essay", "Photography"]
        );
        assert_eq!(tagged.tags, vec!["Photo Essay"]);
    }

    #[test]
    fn adding_new_tag_from_post_editor_creates_portable_tag_metadata() {
        let (db, project, tmp) = setup();
        let edited = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Edited",
            Some("Body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let mut app = make_app(db, project.clone(), &tmp);
        let _ = app.open_tab(Tab {
            id: edited.id.clone(),
            tab_type: TabType::Post,
            title: edited.title,
            is_transient: false,
            is_dirty: false,
        });

        let _ = app.handle_post_editor_msg(PostEditorMsg::AddSuggestedTag("New Tag".into()));

        assert!(
            bds_core::db::queries::tag::get_tag_by_project_and_name(
                app.db.as_ref().unwrap().conn(),
                &project.id,
                "new tag",
            )
            .is_ok()
        );
        assert!(
            app.post_editors[&edited.id]
                .tags
                .contains(&"New Tag".to_string())
        );
        let portable = std::fs::read_to_string(tmp.path().join("meta/tags.json")).unwrap();
        assert!(portable.contains("New Tag"));
    }

    #[test]
    fn post_refresh_does_not_drop_loaded_semantic_tag_suggestions() {
        let (db, project, tmp) = setup();
        let edited = post::create_post(
            db.conn(),
            tmp.path(),
            &project.id,
            "Edited",
            Some("Body"),
            vec![],
            vec![],
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let mut app = make_app(db, project.clone(), &tmp);
        let _ = app.open_tab(Tab {
            id: edited.id.clone(),
            tab_type: TabType::Post,
            title: edited.title,
            is_transient: false,
            is_dirty: false,
        });
        app.post_editors
            .get_mut(&edited.id)
            .unwrap()
            .semantic_tag_suggestions = vec!["science".to_string()];

        app.handle_domain_event(DomainEvent::EntityChanged {
            project_id: project.id,
            entity: DomainEntity::Post,
            entity_id: edited.id.clone(),
            action: NotificationAction::Updated,
        });

        assert_eq!(
            app.post_editors[&edited.id].semantic_tag_suggestions,
            vec!["science"]
        );
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
        second_post.updated_at = first.updated_at + 1;
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

        let dash = app
            .dashboard_state
            .clone()
            .expect("dashboard state should be set");
        let now = chrono::Utc::now();
        assert_eq!(dash.stats.total_posts, 2);
        assert_eq!(dash.stats.published_count, 1);
        assert_eq!(dash.stats.media_count, 1);
        assert_eq!(dash.stats.tag_count, 2);
        assert_eq!(dash.timeline.len(), 1);
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

        // TagCloud guarantee: alphabetical display order, font size relative
        // to the min/max counts of the visible set (equal counts -> 11px).
        let names = dash
            .tag_cloud
            .iter()
            .map(|tag| tag.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["lua", "rust"]);
        assert!(dash.tag_cloud.iter().all(|tag| tag.font_size == 11.0));
    }

    #[test]
    fn dashboard_tag_cloud_takes_most_used_tags_and_scales_fonts() {
        let (db, project, tmp) = setup();
        // 42 distinct tags; "big" appears on 3 posts, "mid" on 2, the rest once.
        for index in 0..3 {
            post::create_post(
                db.conn(),
                tmp.path(),
                &project.id,
                &format!("Post {index}"),
                Some("Body"),
                vec![
                    "big".to_string(),
                    format!("solo-{index:02}"),
                    format!("solo-x{index:02}"),
                ],
                vec![],
                None,
                Some("en"),
                None,
            )
            .unwrap();
        }
        for index in 3..20 {
            post::create_post(
                db.conn(),
                tmp.path(),
                &project.id,
                &format!("Post {index}"),
                Some("Body"),
                vec![
                    format!("solo-{index:02}"),
                    format!("solo-x{index:02}"),
                    if index < 5 { "mid" } else { "big" }.to_string(),
                ],
                vec![],
                None,
                Some("en"),
                None,
            )
            .unwrap();
        }

        let app = make_app(db, project, &tmp);
        let dash = app.hydrate_dashboard_state();

        // 42 distinct tags overall, but only the 40 most-used are displayed.
        assert_eq!(dash.stats.tag_count, 42);
        assert_eq!(dash.tag_cloud.len(), 40);
        let big = dash.tag_cloud.iter().find(|tag| tag.name == "big").unwrap();
        let mid = dash.tag_cloud.iter().find(|tag| tag.name == "mid").unwrap();
        assert_eq!(big.count, 18);
        assert_eq!(mid.count, 2);
        // Relative scaling over the visible set: max count -> 22px, min -> 11px.
        assert_eq!(big.font_size, 22.0);
        assert!(dash.tag_cloud.iter().any(|tag| tag.font_size == 11.0));
        // Display order is alphabetical.
        let mut sorted = dash
            .tag_cloud
            .iter()
            .map(|tag| tag.name.to_lowercase())
            .collect::<Vec<_>>();
        let displayed = sorted.clone();
        sorted.sort();
        assert_eq!(displayed, sorted);
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
        // Only months with posts appear in the timeline: the created_at month
        // (March), not the updated_at month (April).
        assert_eq!(dash.timeline.len(), 1);
        let march = &dash.timeline[0];
        assert_eq!(march.year, 2026);
        assert_eq!(march.label, "Mar");
        assert_eq!(march.count, 1);
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
    fn script_syntax_check_reports_success_and_failure() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);
        let _ = app.update(Message::CreateScript);

        let _ = app.update(Message::ScriptEditor(ScriptEditorMsg::CheckSyntax));
        assert_eq!(
            app.toasts.last().map(|toast| toast.message.as_str()),
            Some(t(UiLocale::En, "editor.syntaxValid").as_str())
        );

        let _ = app.update(Message::ScriptEditor(ScriptEditorMsg::ContentChanged(
            "function render(".to_string(),
        )));
        let _ = app.update(Message::ScriptEditor(ScriptEditorMsg::CheckSyntax));
        assert_eq!(
            app.toasts.last().map(|toast| toast.level),
            Some(ToastLevel::Error)
        );
    }

    #[test]
    fn script_delete_requires_confirmation_before_removal() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);
        let _ = app.update(Message::CreateScript);
        let script_id = app.sidebar_scripts[0].id.clone();

        let _ = app.update(Message::ScriptEditor(ScriptEditorMsg::Delete));
        assert!(
            bds_core::db::queries::script::get_script_by_id(
                app.db.as_ref().unwrap().conn(),
                &script_id
            )
            .is_ok()
        );
        assert!(app.tabs.iter().any(|tab| tab.id == script_id));

        let _ = app.update(Message::ConfirmModal(modal::ConfirmAction::DeleteScript(
            script_id.clone(),
        )));
        assert!(
            bds_core::db::queries::script::get_script_by_id(
                app.db.as_ref().unwrap().conn(),
                &script_id
            )
            .is_err()
        );
        assert!(!app.tabs.iter().any(|tab| tab.id == script_id));
        assert!(!app.script_editors.contains_key(&script_id));
    }

    /// sidebar_views.allium ScriptListItemEntry provides
    /// ScriptDeleteRequested(item.script_id): deleting from the sidebar row
    /// routes through the same confirm modal as the editor delete button and
    /// works without an open editor tab.
    #[test]
    fn sidebar_script_delete_requires_confirmation_and_works_without_open_tab() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);
        let _ = app.update(Message::CreateScript);
        let script_id = app.sidebar_scripts[0].id.clone();

        // Close the editor tab: the sidebar row must not depend on one.
        let _ = app.update(Message::CloseTab(script_id.clone()));
        assert!(!app.tabs.iter().any(|tab| tab.id == script_id));

        // Requesting deletion only opens the confirm modal; nothing is deleted.
        let _ = app.update(Message::ScriptDeleteRequested(script_id.clone()));
        assert!(matches!(
            app.active_modal,
            Some(modal::ModalState::ConfirmDelete { .. })
        ));
        assert!(
            bds_core::db::queries::script::get_script_by_id(
                app.db.as_ref().unwrap().conn(),
                &script_id
            )
            .is_ok()
        );

        let _ = app.update(Message::ConfirmModal(modal::ConfirmAction::DeleteScript(
            script_id.clone(),
        )));
        assert!(
            bds_core::db::queries::script::get_script_by_id(
                app.db.as_ref().unwrap().conn(),
                &script_id
            )
            .is_err()
        );
        assert!(!app.script_editors.contains_key(&script_id));
    }

    /// editor_script.allium ScriptDeleteAction: the confirm modal shows the
    /// script title with no reference list.
    #[test]
    fn sidebar_script_delete_confirmation_shows_script_title() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);
        let _ = app.update(Message::CreateScript);
        let script = app.sidebar_scripts[0].clone();

        let _ = app.update(Message::ScriptDeleteRequested(script.id.clone()));
        match app.active_modal {
            Some(modal::ModalState::ConfirmDelete {
                ref entity_name,
                ref references,
                on_confirm: modal::ConfirmAction::DeleteScript(ref id),
            }) => {
                assert_eq!(entity_name, &script.title);
                assert!(references.is_empty());
                assert_eq!(id, &script.id);
            }
            ref other => panic!("expected ConfirmDelete modal, got {other:?}"),
        }
    }

    #[test]
    fn rebuild_completion_refreshes_project_metadata_in_settings() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);
        app.settings_state = Some(app.hydrate_settings_state());

        let mut rebuilt = bds_core::db::queries::project::get_project_by_id(
            app.db.as_ref().unwrap().conn(),
            &app.active_project.as_ref().unwrap().id,
        )
        .unwrap();
        rebuilt.description = Some("Description from project.json".to_string());
        bds_core::db::queries::project::update_project(app.db.as_ref().unwrap().conn(), &rebuilt)
            .unwrap();

        let label = t(UiLocale::En, "engine.rebuildStarted");
        let task_id = app.task_manager.submit(&label);
        let _ = app.update(Message::EngineTaskDone {
            task_id,
            operation: "engine.rebuildStarted",
            label,
            result: Ok("done".to_string()),
        });

        assert_eq!(
            app.active_project
                .as_ref()
                .and_then(|project| project.description.as_deref()),
            Some("Description from project.json")
        );
        assert_eq!(
            app.settings_state
                .as_ref()
                .map(|state| state.project_description.text()),
            Some("Description from project.json\n".to_string())
        );
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
        let db = Database::open(&db_path).unwrap();
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
            Some(tmp.path()),
            &filter,
            false,
            (50, 0),
        );

        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].id, published.id);
    }

    #[test]
    fn query_sidebar_media_uses_full_text_metadata() {
        let (_db, project, tmp) = setup();
        let db_path = tmp.path().join("media-sidebar.db");
        let db = Database::open(&db_path).unwrap();
        db.migrate().unwrap();
        ensure_fts_tables(db.conn()).unwrap();
        insert_project(db.conn(), &project).unwrap();
        let source = tmp.path().join("searchable.png");
        std::fs::write(&source, tiny_png_bytes()).unwrap();
        let imported = media::import_media(
            db.conn(),
            tmp.path(),
            &project.id,
            &source,
            "searchable.png",
            Some("Sunset"),
            Some("Mountains under a red sky"),
            None,
            None,
            Some("en"),
            vec!["nature".to_string()],
        )
        .unwrap();

        let filter = MediaFilter {
            search_query: "mountains".to_string(),
            tag_filter: vec!["nature".to_string()],
            ..MediaFilter::default()
        };
        let media =
            BdsApp::query_sidebar_media_blocking(&db_path, &project.id, "en", &filter, 50, 0);

        assert_eq!(media.len(), 1);
        assert_eq!(media[0].id, imported.id);
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
            || false,
            |_index, _total, _name| {},
        )
        .unwrap();

        assert_eq!(regenerated, 1);
        assert!(small_thumb.exists());
    }

    #[test]
    fn desktop_watcher_consumes_cli_event_refreshes_sidebar_and_marks_seen() {
        let (db, project, tmp) = setup();
        let created = script::create_script(
            db.conn(),
            &project.id,
            "From CLI",
            ScriptKind::Utility,
            "function main() end",
            None,
        )
        .unwrap();
        let mut app = make_app(db, project.clone(), &tmp);
        bds_core::engine::cli_sync::record_cli_event(
            app.db.as_ref().unwrap().conn(),
            &bds_core::model::DomainEvent::EntityChanged {
                project_id: project.id,
                entity: bds_core::model::DomainEntity::Script,
                entity_id: created.id.clone(),
                action: bds_core::model::NotificationAction::Created,
            },
        )
        .unwrap();

        let _ = app.update(Message::DomainEventsTick);

        assert!(
            app.sidebar_scripts
                .iter()
                .any(|script| script.id == created.id)
        );
        let rows = bds_core::db::queries::db_notification::list_notifications(
            app.db.as_ref().unwrap().conn(),
        )
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].seen_at.is_some());
        assert!(app.toasts.is_empty());
    }

    #[test]
    fn locale_setting_is_persisted_and_external_event_reloads_without_feedback() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);

        let _ = app.update(Message::SetUiLocale(UiLocale::De));
        assert_eq!(app.ui_locale, UiLocale::De);
        assert_eq!(
            bds_core::engine::settings::ui_language(app.db.as_ref().unwrap().conn())
                .unwrap()
                .as_deref(),
            Some("de")
        );

        bds_core::engine::settings::set(
            app.db.as_ref().unwrap().conn(),
            bds_core::engine::settings::UI_LANGUAGE_KEY,
            "fr-FR",
        )
        .unwrap();
        let _ = app.update(Message::DomainEventsTick);

        assert_eq!(app.ui_locale, UiLocale::Fr);
        assert!(app.toasts.is_empty());
        assert!(
            bds_core::db::queries::db_notification::list_notifications(
                app.db.as_ref().unwrap().conn()
            )
            .unwrap()
            .is_empty()
        );
    }

    #[test]
    fn deleted_entity_event_closes_its_open_editor_without_notification() {
        let (db, project, tmp) = setup();
        let created = script::create_script(
            db.conn(),
            &project.id,
            "Delete externally",
            ScriptKind::Utility,
            "function main() end",
            None,
        )
        .unwrap();
        let mut app = make_app(db, project, &tmp);
        let tab = Tab {
            id: created.id.clone(),
            tab_type: TabType::Scripts,
            title: created.title,
            is_transient: false,
            is_dirty: false,
        };
        app.tabs.push(tab.clone());
        app.load_editor_for_tab(&tab);

        script::delete_script(app.db.as_ref().unwrap().conn(), tmp.path(), &created.id).unwrap();
        let _ = app.update(Message::DomainEventsTick);

        assert!(!app.tabs.iter().any(|tab| tab.id == created.id));
        assert!(!app.script_editors.contains_key(&created.id));
        assert!(app.toasts.is_empty());
    }

    #[test]
    fn external_update_event_preserves_unsaved_editor_buffer() {
        let (db, project, tmp) = setup();
        let created = script::create_script(
            db.conn(),
            &project.id,
            "Edit externally",
            ScriptKind::Utility,
            "function main() end",
            None,
        )
        .unwrap();
        let mut app = make_app(db, project.clone(), &tmp);
        let tab = Tab {
            id: created.id.clone(),
            tab_type: TabType::Scripts,
            title: created.title,
            is_transient: false,
            is_dirty: true,
        };
        app.tabs.push(tab.clone());
        app.load_editor_for_tab(&tab);
        let editor = app.script_editors.get_mut(&created.id).unwrap();
        editor.content = "local unsaved buffer".to_string();
        editor.is_dirty = true;

        script::update_script(
            app.db.as_ref().unwrap().conn(),
            tmp.path(),
            &created.id,
            &project.id,
            Some("Remote title"),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let _ = app.update(Message::DomainEventsTick);

        assert_eq!(
            app.script_editors[&created.id].content,
            "local unsaved buffer"
        );
        assert!(app.script_editors[&created.id].is_dirty);
        assert!(app.toasts.is_empty());
    }

    #[test]
    fn mcp_settings_review_applies_or_rejects_inert_proposals() {
        let (db, mut project, tmp) = setup();
        project.data_path = Some(tmp.path().to_string_lossy().into_owned());
        bds_core::db::queries::project::update_project(db.conn(), &project).unwrap();
        let now = bds_core::util::now_unix_ms();
        let accepted = bds_core::model::McpProposal {
            id: "accept-me".into(),
            project_id: project.id.clone(),
            kind: bds_core::model::ProposalKind::DraftPost,
            status: bds_core::model::ProposalStatus::Pending,
            entity_id: None,
            data: serde_json::json!({"title":"Approved","content":"Body"}).to_string(),
            result: None,
            created_at: now,
            expires_at: now + 60_000,
            resolved_at: None,
        };
        let mut rejected = accepted.clone();
        rejected.id = "reject-me".into();
        rejected.data = serde_json::json!({"title":"Rejected","content":"Body"}).to_string();
        bds_core::db::queries::mcp_proposal::insert_proposal(db.conn(), &accepted).unwrap();
        bds_core::db::queries::mcp_proposal::insert_proposal(db.conn(), &rejected).unwrap();

        let mut app = make_app(db, project.clone(), &tmp);
        app.settings_state = Some(app.hydrate_settings_state());
        assert_eq!(app.settings_state.as_ref().unwrap().mcp_proposals.len(), 2);
        let _ = app.handle_settings_msg(SettingsMsg::McpProposalAccepted("accept-me".into()));
        let _ = app.handle_settings_msg(SettingsMsg::McpProposalRejected("reject-me".into()));

        let posts = bds_core::db::queries::post::list_posts_by_project(
            app.db.as_ref().unwrap().conn(),
            &project.id,
        )
        .unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].title, "Approved");
        assert_eq!(posts[0].status, PostStatus::Published);
        assert_eq!(
            bds_core::engine::mcp::get_proposal(app.db.as_ref().unwrap().conn(), "accept-me")
                .unwrap()
                .status,
            bds_core::model::ProposalStatus::Accepted
        );
        assert_eq!(
            bds_core::engine::mcp::get_proposal(app.db.as_ref().unwrap().conn(), "reject-me")
                .unwrap()
                .status,
            bds_core::model::ProposalStatus::Rejected
        );
        assert!(
            app.settings_state
                .as_ref()
                .unwrap()
                .mcp_proposals
                .is_empty()
        );
    }

    #[test]
    fn chat_ui_creates_reopens_renames_and_deletes_persistent_conversations() {
        let (db, project, tmp) = setup();
        let mut app = make_app(db, project, &tmp);

        let _ = app.update(Message::ChatCreate);
        assert_eq!(app.chat_conversations.len(), 1);
        let conversation = app.chat_conversations[0].clone();
        let tab = Tab {
            id: conversation.id.clone(),
            tab_type: TabType::Chat,
            title: conversation.title,
            is_transient: false,
            is_dirty: false,
        };
        let _ = app.update(Message::OpenTab(tab));
        assert!(app.chat_editors.contains_key(&conversation.id));

        let _ = app.update(Message::ChatRenameInputChanged("Research".to_string()));
        let _ = app.update(Message::ChatRename);
        assert_eq!(app.chat_conversations[0].title, "Research");
        assert_eq!(
            bds_core::engine::chat::get_conversation(
                app.db.as_ref().unwrap().conn(),
                &conversation.id,
            )
            .unwrap()
            .title,
            "Research"
        );

        let _ = app.update(Message::ChatDelete(conversation.id.clone()));
        assert!(app.chat_conversations.is_empty());
        assert!(!app.chat_editors.contains_key(&conversation.id));
        assert!(
            bds_core::engine::chat::list_conversations(app.db.as_ref().unwrap().conn())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn chat_surfaces_stream_persist_reopen_and_refuse_unknown_actions() {
        let (db, project, tmp) = setup();
        let conversation =
            chat::create_conversation_titled(db.conn(), Some("test-model"), "Surfaces").unwrap();
        let tool_calls = serde_json::json!([
            {
                "id": "form-call",
                "type": "function",
                "function": {
                    "name": "render_form",
                    "arguments": serde_json::json!({
                        "title": "Preferences",
                        "fields": [{"key": "topic", "label": "Topic", "inputType": "text"}],
                        "submitAction": "switchView"
                    }).to_string()
                }
            },
            {
                "id": "tabs-call",
                "type": "function",
                "function": {
                    "name": "render_tabs",
                    "arguments": serde_json::json!({
                        "tabs": [
                            {"label": "One", "content": [{"type": "text", "text": "First"}]},
                            {"label": "Two", "content": [{"type": "text", "text": "Second"}]}
                        ]
                    }).to_string()
                }
            }
        ])
        .to_string();
        let message = chat::insert_message(
            db.conn(),
            &conversation.id,
            ChatRole::Assistant,
            Some("Structured answer"),
            None,
            Some(&tool_calls),
            bds_core::engine::ai::TokenUsage::default(),
        )
        .unwrap();
        let mut app = make_app(db, project, &tmp);
        let tab = Tab {
            id: conversation.id.clone(),
            tab_type: TabType::Chat,
            title: conversation.title.clone(),
            is_transient: false,
            is_dirty: false,
        };
        let _ = app.update(Message::OpenTab(tab));
        let form_id = format!("{}-surface-0", message.id);
        let tabs_id = format!("{}-surface-1", message.id);
        {
            let state = app.chat_editors.get_mut(&conversation.id).unwrap();
            assert_eq!(state.message_surfaces[&message.id].len(), 2);
            state.streaming = true;
            state.add_streaming_surface(
                "render_card",
                &serde_json::json!({"title": "Later"}),
                "later-message-surface-0".into(),
            );
            assert_eq!(state.message_surfaces[&message.id].len(), 2);
            assert_eq!(state.streaming_surfaces.len(), 1);
        }

        let _ = app.update(Message::ChatSurfaceFieldChanged {
            surface_id: form_id.clone(),
            field: "topic".into(),
            value: "Rust".into(),
        });
        app.chat_editors
            .get_mut(&conversation.id)
            .unwrap()
            .surface_state_dirty_since =
            Some(std::time::Instant::now() - std::time::Duration::from_millis(501));
        let _ = app.update(Message::TaskTick);
        let _ = app.update(Message::ChatSurfaceTabSelected {
            surface_id: tabs_id.clone(),
            index: 1,
        });
        let _ = app.update(Message::ChatSurfaceDismissed(form_id.clone()));

        let reopened_conversation =
            chat::get_conversation(app.db.as_ref().unwrap().conn(), &conversation.id).unwrap();
        let reopened = ChatEditorState::new(
            reopened_conversation,
            chat::list_messages(app.db.as_ref().unwrap().conn(), &conversation.id).unwrap(),
            vec![],
        );
        assert_eq!(
            reopened.surface_state.surface_data[&form_id]["topic"],
            "Rust"
        );
        assert_eq!(reopened.surface_state.surface_tabs[&tabs_id], 1);
        assert!(reopened.surface_state.dismissed_surfaces.contains(&form_id));
        assert_eq!(reopened.message_surfaces[&message.id].len(), 1);

        let _ = app.update(Message::ChatSurfaceAction {
            surface_id: form_id,
            action: "runJavaScript".into(),
            payload: serde_json::json!({"script": "alert(1)"}),
        });
        assert!(
            app.chat_editors[&conversation.id]
                .error
                .as_deref()
                .is_some_and(|error| error.contains("runJavaScript"))
        );
    }
}
