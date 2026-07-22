use iced::widget::text::Shaping;
use iced::widget::{button, column, container, row, scrollable, text, text_editor, text_input};
use iced::{Alignment, Color, Element, Length};

use bds_core::engine::ai::AiEndpointKind;
use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::{t, tw};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiModelOption {
    pub id: String,
    pub label: String,
    pub supports_tools: bool,
    pub supports_vision: bool,
}

impl std::fmt::Display for AiModelOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AiModeViewState {
    pub endpoint_url: String,
    pub chat_model: String,
    pub title_model: String,
    pub image_model: String,
    pub api_key_input: String,
    pub api_key_configured: bool,
    pub model_options: Vec<AiModelOption>,
    pub chat_supports_tools: bool,
    pub image_supports_vision: bool,
}

/// Collapsible section identifiers per editor_settings.allium.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SettingsSection {
    Project,
    Editor,
    Content,
    AI,
    Technology,
    Publishing,
    Data,
    MCP,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsCategoryRow {
    pub name: String,
    pub title: String,
    pub render_in_lists: bool,
    pub show_title: bool,
    pub post_template_slug: String,
    pub list_template_slug: String,
    pub is_protected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsMcpAgentRow {
    pub agent: bds_core::engine::mcp::McpAgent,
    pub label: String,
    pub configured: bool,
    pub config_path: String,
}

impl std::fmt::Display for SettingsCategoryRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

const PROTECTED_CATEGORIES: [&str; 4] = ["article", "aside", "page", "picture"];

pub fn default_category_rows() -> Vec<SettingsCategoryRow> {
    PROTECTED_CATEGORIES
        .iter()
        .map(|name| SettingsCategoryRow {
            name: (*name).to_string(),
            title: (*name).to_string(),
            render_in_lists: true,
            show_title: true,
            post_template_slug: String::new(),
            list_template_slug: String::new(),
            is_protected: true,
        })
        .collect()
}

impl SettingsSection {
    pub fn all() -> &'static [SettingsSection] {
        &[
            Self::Project,
            Self::Editor,
            Self::Content,
            Self::AI,
            Self::Technology,
            Self::Publishing,
            Self::Data,
            Self::MCP,
        ]
    }

    pub fn i18n_key(&self) -> &'static str {
        match self {
            Self::Project => "settings.nav.project",
            Self::Editor => "settings.nav.editor",
            Self::Content => "settings.nav.content",
            Self::AI => "settings.nav.ai",
            Self::Technology => "settings.nav.technology",
            Self::Publishing => "settings.nav.publishing",
            Self::Data => "settings.nav.data",
            Self::MCP => "settings.nav.mcp",
        }
    }
}

/// State for the settings view.
pub struct SettingsViewState {
    pub search_query: String,
    pub collapsed: Vec<SettingsSection>,
    pub active_section: Option<SettingsSection>,
    // Project
    pub project_name: String,
    pub project_description: text_editor::Content,
    pub data_path: String,
    pub public_url: String,
    pub main_language: String,
    pub blog_languages: Vec<String>,
    pub available_languages: Vec<String>,
    pub default_author: String,
    pub max_posts_per_page: String,
    pub image_import_concurrency: String,
    pub blogmark_category: String,
    // Editor
    pub default_mode: String,
    pub diff_view_style: String,
    pub wrap_long_lines: bool,
    pub hide_unchanged_regions: bool,
    // Content
    pub categories: Vec<SettingsCategoryRow>,
    pub new_category_name: String,
    pub template_options: Vec<String>,
    // Publishing
    pub ssh_mode: String,
    pub ssh_host: String,
    pub ssh_username: String,
    pub ssh_remote_path: String,
    // AI
    pub online_ai: AiModeViewState,
    pub airplane_ai: AiModeViewState,
    pub system_prompt: text_editor::Content,
    // Technology
    pub semantic_similarity_enabled: bool,
    // MCP
    pub mcp_enabled: bool,
    pub mcp_running: bool,
    pub mcp_endpoint: String,
    pub mcp_proposals: Vec<bds_core::model::McpProposal>,
    pub mcp_agents: Vec<SettingsMcpAgentRow>,
}

impl std::fmt::Debug for SettingsViewState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SettingsViewState")
            .field("project_name", &self.project_name)
            .finish_non_exhaustive()
    }
}

impl Clone for SettingsViewState {
    fn clone(&self) -> Self {
        Self {
            search_query: self.search_query.clone(),
            collapsed: self.collapsed.clone(),
            active_section: self.active_section.clone(),
            project_name: self.project_name.clone(),
            project_description: text_editor::Content::with_text(&self.project_description.text()),
            data_path: self.data_path.clone(),
            public_url: self.public_url.clone(),
            main_language: self.main_language.clone(),
            blog_languages: self.blog_languages.clone(),
            available_languages: self.available_languages.clone(),
            default_author: self.default_author.clone(),
            max_posts_per_page: self.max_posts_per_page.clone(),
            image_import_concurrency: self.image_import_concurrency.clone(),
            blogmark_category: self.blogmark_category.clone(),
            default_mode: self.default_mode.clone(),
            diff_view_style: self.diff_view_style.clone(),
            wrap_long_lines: self.wrap_long_lines,
            hide_unchanged_regions: self.hide_unchanged_regions,
            categories: self.categories.clone(),
            new_category_name: self.new_category_name.clone(),
            template_options: self.template_options.clone(),
            ssh_mode: self.ssh_mode.clone(),
            ssh_host: self.ssh_host.clone(),
            ssh_username: self.ssh_username.clone(),
            ssh_remote_path: self.ssh_remote_path.clone(),
            online_ai: self.online_ai.clone(),
            airplane_ai: self.airplane_ai.clone(),
            system_prompt: text_editor::Content::with_text(&self.system_prompt.text()),
            semantic_similarity_enabled: self.semantic_similarity_enabled,
            mcp_enabled: self.mcp_enabled,
            mcp_running: self.mcp_running,
            mcp_endpoint: self.mcp_endpoint.clone(),
            mcp_proposals: self.mcp_proposals.clone(),
            mcp_agents: self.mcp_agents.clone(),
        }
    }
}

impl Default for SettingsViewState {
    fn default() -> Self {
        Self {
            search_query: String::new(),
            collapsed: Vec::new(),
            active_section: None,
            project_name: String::new(),
            project_description: text_editor::Content::new(),
            data_path: String::new(),
            public_url: String::new(),
            main_language: "en".to_string(),
            blog_languages: vec!["en".to_string()],
            available_languages: vec![
                "en".to_string(),
                "de".to_string(),
                "fr".to_string(),
                "it".to_string(),
                "es".to_string(),
            ],
            default_author: String::new(),
            max_posts_per_page: "50".to_string(),
            image_import_concurrency: "4".to_string(),
            blogmark_category: String::new(),
            default_mode: "markdown".to_string(),
            diff_view_style: "inline".to_string(),
            wrap_long_lines: true,
            hide_unchanged_regions: false,
            categories: default_category_rows(),
            new_category_name: String::new(),
            template_options: Vec::new(),
            ssh_mode: "rsync".to_string(),
            ssh_host: String::new(),
            ssh_username: String::new(),
            ssh_remote_path: String::new(),
            online_ai: AiModeViewState::default(),
            airplane_ai: AiModeViewState::default(),
            system_prompt: text_editor::Content::new(),
            semantic_similarity_enabled: false,
            mcp_enabled: false,
            mcp_running: false,
            mcp_endpoint: String::new(),
            mcp_proposals: Vec::new(),
            mcp_agents: Vec::new(),
        }
    }
}

impl SettingsViewState {
    pub fn focus_section(&mut self, section: SettingsSection) {
        self.collapsed = SettingsSection::all()
            .iter()
            .filter(|candidate| **candidate != section)
            .cloned()
            .collect();
        self.search_query.clear();
        self.active_section = Some(section);
    }

    fn ordered_sections(&self) -> Vec<SettingsSection> {
        let mut sections = SettingsSection::all().to_vec();
        if let Some(active) = &self.active_section
            && let Some(index) = sections.iter().position(|section| section == active)
        {
            let focused = sections.remove(index);
            sections.insert(0, focused);
        }
        sections
    }
}

/// Settings view messages.
#[derive(Debug, Clone)]
pub enum SettingsMsg {
    SearchChanged(String),
    ToggleSection(SettingsSection),
    // Project
    ProjectNameChanged(String),
    ProjectDescriptionAction(text_editor::Action),
    DataPathChanged(String),
    BrowseDataPath,
    ResetDataPath,
    PublicUrlChanged(String),
    MainLanguageChanged(String),
    ToggleBlogLanguage(String),
    DefaultAuthorChanged(String),
    MaxPostsPerPageChanged(String),
    ImageImportConcurrencyChanged(String),
    BlogmarkCategoryChanged(String),
    CopyBlogmarkBookmarklet,
    SaveProject,
    // Editor
    DefaultModeChanged(String),
    DiffViewStyleChanged(String),
    WrapLongLinesChanged(bool),
    HideUnchangedRegionsChanged(bool),
    SaveEditor,
    // Content
    AddCategoryNameChanged(String),
    AddCategory,
    CategoryTitleChanged(String, String),
    CategoryRenderInListsChanged(String, bool),
    CategoryShowTitleChanged(String, bool),
    CategoryPostTemplateChanged(String, String),
    CategoryListTemplateChanged(String, String),
    SaveCategory(String),
    RemoveCategory(String),
    ResetCategoriesToDefaults,
    // Publishing
    SshModeChanged(String),
    SshHostChanged(String),
    SshUsernameChanged(String),
    SshRemotePathChanged(String),
    SavePublishing,
    ClearPublishing,
    // AI
    AiEndpointUrlChanged(AiEndpointKind, String),
    AiApiKeyChanged(AiEndpointKind, String),
    AiChatModelChanged(AiEndpointKind, String),
    AiTitleModelChanged(AiEndpointKind, String),
    AiImageModelChanged(AiEndpointKind, String),
    AiToolsChanged(AiEndpointKind, bool),
    AiVisionChanged(AiEndpointKind, bool),
    RefreshAiModels(AiEndpointKind),
    TestAi(AiEndpointKind),
    SystemPromptAction(text_editor::Action),
    SaveAi,
    ResetSystemPrompt,
    // Technology
    SemanticSimilarityChanged(bool),
    // Data maintenance
    RebuildPosts,
    RebuildMedia,
    RebuildScripts,
    RebuildTemplates,
    RebuildLinks,
    RebuildSearchIndex,
    RegenerateThumbnails,
    OpenDataFolder,
    InstallCli,
    McpEnabledChanged(bool),
    McpProposalAccepted(String),
    McpProposalRejected(String),
    McpAgentToggled(bds_core::engine::mcp::McpAgent),
    McpRefresh,
    /// Navigate to a specific section from sidebar; expand it, collapse all others.
    FocusSection(SettingsSection),
}

/// Render the settings view.
pub fn view<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    let search = text_input(&t(locale, "sidebar.filter.search"), &state.search_query)
        .on_input(|s| Message::Settings(SettingsMsg::SearchChanged(s)))
        .size(14)
        .padding([8, 10])
        .style(inputs::field_style);

    let query_lower = state.search_query.to_lowercase();

    let mut section_items = Vec::new();
    for section in state.ordered_sections() {
        let label = t(locale, section.i18n_key());
        if !query_lower.is_empty() && !label.to_lowercase().contains(&query_lower) {
            continue;
        }
        let collapsed = state.collapsed.contains(&section);
        let section_el = render_section(state, &section, &label, collapsed, locale);
        section_items.push(section_el);
    }

    let sections = if section_items.is_empty() {
        column![
            text(t(locale, "common.noResults"))
                .size(14)
                .color(Color::from_rgb(0.7, 0.72, 0.78)),
            button(text(t(locale, "common.clear")).size(13))
                .on_press(Message::Settings(SettingsMsg::SearchChanged(String::new())))
                .style(inputs::secondary_button)
                .padding([6, 12]),
        ]
        .spacing(8)
        .width(Length::Fill)
    } else {
        column(section_items).spacing(12).width(Length::Fill)
    };

    let body = scrollable(
        column![search, sections]
            .spacing(16)
            .padding(16)
            .width(Length::Fill),
    )
    .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
    .style(inputs::scrollable_style)
    .width(Length::Fill)
    .height(Length::Fill);

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn render_section<'a>(
    state: &'a SettingsViewState,
    section: &SettingsSection,
    label: &str,
    collapsed: bool,
    locale: UiLocale,
) -> Element<'a, Message> {
    let toggle_char = if collapsed { "\u{25B6}" } else { "\u{25BC}" };
    let header = button(
        row![
            text(toggle_char).size(12),
            text(label.to_string()).size(14).color(Color::WHITE),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .on_press(Message::Settings(SettingsMsg::ToggleSection(
        section.clone(),
    )))
    .padding([6, 8])
    .width(Length::Fill)
    .style(inputs::disclosure_button);

    if collapsed {
        return inputs::card(header).padding([2, 4]).into();
    }

    let content: Element<'a, Message> = match section {
        SettingsSection::Project => section_project(state, locale),
        SettingsSection::Editor => section_editor(state, locale),
        SettingsSection::Content => section_content(state, locale),
        SettingsSection::AI => section_ai(state, locale),
        SettingsSection::Technology => section_technology(state, locale),
        SettingsSection::Publishing => section_publishing(state, locale),
        SettingsSection::Data => section_data(locale),
        SettingsSection::MCP => section_mcp(state, locale),
    };

    inputs::card(column![header, content].spacing(8)).into()
}

fn section_project<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    let name = inputs::labeled_input(
        &t(locale, "settings.projectName"),
        "",
        &state.project_name,
        |s| Message::Settings(SettingsMsg::ProjectNameChanged(s)),
    );
    let desc = column![
        text(t(locale, "settings.projectDescription"))
            .size(12)
            .color(inputs::LABEL_COLOR)
            .shaping(Shaping::Advanced),
        text_editor(&state.project_description)
            .on_action(|a| Message::Settings(SettingsMsg::ProjectDescriptionAction(a)))
            .height(Length::Fixed(80.0))
            .size(14)
            .style(inputs::text_editor_style),
    ]
    .spacing(4);
    let data_path = row![
        inputs::labeled_input(&t(locale, "settings.dataPath"), "", &state.data_path, |s| {
            Message::Settings(SettingsMsg::DataPathChanged(s))
        },),
        button(text(t(locale, "settings.browse")).size(12))
            .on_press(Message::Settings(SettingsMsg::BrowseDataPath))
            .style(inputs::secondary_button)
            .padding([6, 12]),
        button(text(t(locale, "settings.reset")).size(12))
            .on_press(Message::Settings(SettingsMsg::ResetDataPath))
            .style(inputs::secondary_button)
            .padding([6, 12]),
    ]
    .spacing(8)
    .align_y(Alignment::End);

    let url = inputs::labeled_input(
        &t(locale, "settings.publicUrl"),
        "https://",
        &state.public_url,
        |s| Message::Settings(SettingsMsg::PublicUrlChanged(s)),
    );
    let language_options = state.available_languages.clone();
    let main_language = inputs::labeled_select(
        &t(locale, "settings.mainLanguage"),
        &language_options,
        Some(&state.main_language),
        |s| Message::Settings(SettingsMsg::MainLanguageChanged(s)),
    );
    let blog_languages = column(
        state
            .available_languages
            .iter()
            .map(|language| {
                let label = if *language == state.main_language {
                    format!(
                        "{} ({})",
                        language,
                        t(locale, "settings.mainLanguageRequired")
                    )
                } else {
                    language.clone()
                };
                inputs::labeled_checkbox(
                    &label,
                    state.blog_languages.iter().any(|item| item == language),
                    {
                        let language = language.clone();
                        move |_| {
                            Message::Settings(SettingsMsg::ToggleBlogLanguage(language.clone()))
                        }
                    },
                )
            })
            .collect::<Vec<_>>(),
    )
    .spacing(6);
    let author = inputs::labeled_input(
        &t(locale, "settings.defaultAuthor"),
        "",
        &state.default_author,
        |s| Message::Settings(SettingsMsg::DefaultAuthorChanged(s)),
    );
    let max_posts = inputs::labeled_input(
        &t(locale, "settings.maxPostsPerPage"),
        "50",
        &state.max_posts_per_page,
        |s| Message::Settings(SettingsMsg::MaxPostsPerPageChanged(s)),
    );
    let image_import_concurrency = inputs::labeled_input(
        &t(locale, "settings.imageImportConcurrency"),
        "4",
        &state.image_import_concurrency,
        |s| Message::Settings(SettingsMsg::ImageImportConcurrencyChanged(s)),
    );
    let blogmark_category = inputs::labeled_select(
        &t(locale, "settings.blogmarkCategory"),
        &state.categories,
        state
            .categories
            .iter()
            .find(|row| row.name == state.blogmark_category),
        |row| Message::Settings(SettingsMsg::BlogmarkCategoryChanged(row.name)),
    );
    let copy_blogmark_bookmarklet =
        button(text(t(locale, "settings.copyBlogmarkBookmarklet")).size(13))
            .on_press(Message::Settings(SettingsMsg::CopyBlogmarkBookmarklet))
            .style(inputs::secondary_button)
            .padding([6, 16]);
    let save = button(text(t(locale, "common.save")).size(13))
        .on_press(Message::Settings(SettingsMsg::SaveProject))
        .style(inputs::primary_button)
        .padding([6, 16]);

    column![
        name,
        desc,
        data_path,
        url,
        main_language,
        column![
            text(t(locale, "settings.blogLanguages"))
                .size(12)
                .color(inputs::LABEL_COLOR)
                .shaping(Shaping::Advanced),
            blog_languages,
        ]
        .spacing(4),
        author,
        max_posts,
        image_import_concurrency,
        blogmark_category,
        copy_blogmark_bookmarklet,
        save,
    ]
    .spacing(8)
    .padding([0, 16])
    .into()
}

fn section_editor<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    let mode_options = vec!["markdown".to_string(), "preview".to_string()];
    let diff_options = vec!["inline".to_string(), "side-by-side".to_string()];
    let mode = inputs::labeled_select(
        &t(locale, "settings.defaultMode"),
        &mode_options,
        Some(&state.default_mode),
        |s| Message::Settings(SettingsMsg::DefaultModeChanged(s)),
    );
    let diff = inputs::labeled_select(
        &t(locale, "settings.diffViewStyle"),
        &diff_options,
        Some(&state.diff_view_style),
        |s| Message::Settings(SettingsMsg::DiffViewStyleChanged(s)),
    );
    let wrap = inputs::labeled_checkbox(
        &t(locale, "settings.wrapLongLines"),
        state.wrap_long_lines,
        |b| Message::Settings(SettingsMsg::WrapLongLinesChanged(b)),
    );
    let hide = inputs::labeled_checkbox(
        &t(locale, "settings.hideUnchangedRegions"),
        state.hide_unchanged_regions,
        |b| Message::Settings(SettingsMsg::HideUnchangedRegionsChanged(b)),
    );
    let save = button(text(t(locale, "common.save")).size(13))
        .on_press(Message::Settings(SettingsMsg::SaveEditor))
        .style(inputs::primary_button)
        .padding([6, 16]);

    column![mode, diff, wrap, hide, save]
        .spacing(8)
        .padding([0, 16])
        .into()
}

fn section_content<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    let template_options = std::iter::once(String::new())
        .chain(state.template_options.iter().cloned())
        .collect::<Vec<_>>();

    let category_rows = state
        .categories
        .iter()
        .fold(column![].spacing(8), |column, category| {
            let title = inputs::labeled_input(
                &tw(
                    locale,
                    "settings.categoryTitle",
                    &[("category", &category.name)],
                ),
                "",
                &category.title,
                {
                    let name = category.name.clone();
                    move |value| {
                        Message::Settings(SettingsMsg::CategoryTitleChanged(name.clone(), value))
                    }
                },
            );
            let post_template = inputs::labeled_select(
                &t(locale, "settings.categoryPostTemplate"),
                &template_options,
                Some(&category.post_template_slug),
                {
                    let name = category.name.clone();
                    move |value| {
                        Message::Settings(SettingsMsg::CategoryPostTemplateChanged(
                            name.clone(),
                            value,
                        ))
                    }
                },
            );
            let list_template = inputs::labeled_select(
                &t(locale, "settings.categoryListTemplate"),
                &template_options,
                Some(&category.list_template_slug),
                {
                    let name = category.name.clone();
                    move |value| {
                        Message::Settings(SettingsMsg::CategoryListTemplateChanged(
                            name.clone(),
                            value,
                        ))
                    }
                },
            );
            let toggles = column![
                inputs::labeled_checkbox(
                    &t(locale, "settings.categoryRenderInLists"),
                    category.render_in_lists,
                    {
                        let name = category.name.clone();
                        move |value| {
                            Message::Settings(SettingsMsg::CategoryRenderInListsChanged(
                                name.clone(),
                                value,
                            ))
                        }
                    },
                ),
                inputs::labeled_checkbox(
                    &t(locale, "settings.categoryShowTitles"),
                    category.show_title,
                    {
                        let name = category.name.clone();
                        move |value| {
                            Message::Settings(SettingsMsg::CategoryShowTitleChanged(
                                name.clone(),
                                value,
                            ))
                        }
                    },
                ),
            ]
            .spacing(6);
            let actions = row![
                button(text(t(locale, "common.save")).size(13))
                    .on_press(Message::Settings(SettingsMsg::SaveCategory(
                        category.name.clone()
                    )))
                    .style(inputs::primary_button)
                    .padding([6, 12]),
                button(text(t(locale, "common.remove")).size(13))
                    .on_press_maybe((!category.is_protected).then(|| Message::Settings(
                        SettingsMsg::RemoveCategory(category.name.clone())
                    )))
                    .style(inputs::danger_button)
                    .padding([6, 12]),
            ]
            .spacing(8);

            column.push(
                inputs::card(
                    column![
                        text(category.name.clone()).size(15).color(Color::WHITE),
                        title,
                        toggles,
                        row![post_template, list_template].spacing(12),
                        actions,
                    ]
                    .spacing(8),
                )
                .padding(12),
            )
        });

    let add_row = row![
        inputs::labeled_input(
            &t(locale, "settings.addCategory"),
            "news",
            &state.new_category_name,
            |value| Message::Settings(SettingsMsg::AddCategoryNameChanged(value)),
        ),
        button(text(t(locale, "common.add")).size(13))
            .on_press(Message::Settings(SettingsMsg::AddCategory))
            .style(inputs::primary_button)
            .padding([6, 12]),
        button(text(t(locale, "settings.resetCategories")).size(13))
            .on_press(Message::Settings(SettingsMsg::ResetCategoriesToDefaults))
            .style(inputs::secondary_button)
            .padding([6, 12]),
    ]
    .spacing(8)
    .align_y(Alignment::End);

    column![category_rows, add_row]
        .spacing(12)
        .padding([0, 16])
        .into()
}

fn section_ai<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    let online = ai_mode_block(
        &state.online_ai,
        AiEndpointKind::Online,
        locale,
        "https://api.example.com/v1",
    );
    let airplane = ai_mode_block(
        &state.airplane_ai,
        AiEndpointKind::Airplane,
        locale,
        "http://localhost:11434/v1",
    );
    let prompt = column![
        text(t(locale, "settings.systemPrompt"))
            .size(12)
            .color(inputs::LABEL_COLOR)
            .shaping(Shaping::Advanced),
        text_editor(&state.system_prompt)
            .on_action(|a| Message::Settings(SettingsMsg::SystemPromptAction(a)))
            .height(Length::Fixed(200.0))
            .size(14)
            .style(inputs::text_editor_style),
    ]
    .spacing(4);
    let btns = row![
        button(text(t(locale, "common.save")).size(13))
            .on_press(Message::Settings(SettingsMsg::SaveAi))
            .style(inputs::primary_button)
            .padding([6, 16]),
        button(text(t(locale, "settings.resetToDefault")).size(13))
            .on_press(Message::Settings(SettingsMsg::ResetSystemPrompt))
            .style(inputs::secondary_button)
            .padding([6, 16]),
    ]
    .spacing(8);

    column![online, airplane, prompt, btns,]
        .spacing(8)
        .padding([0, 16])
        .into()
}

fn ai_mode_block<'a>(
    state: &'a AiModeViewState,
    kind: AiEndpointKind,
    locale: UiLocale,
    url_placeholder: &'a str,
) -> Element<'a, Message> {
    let mut options = state.model_options.clone();
    for model in [&state.chat_model, &state.title_model, &state.image_model] {
        if !model.is_empty() && !options.iter().any(|option| option.id == *model) {
            options.push(AiModelOption {
                id: model.clone(),
                label: model.clone(),
                supports_tools: false,
                supports_vision: false,
            });
        }
    }
    options.sort_by(|left, right| left.label.cmp(&right.label));
    options.insert(
        0,
        AiModelOption {
            id: String::new(),
            label: t(locale, "tags.noTemplate"),
            supports_tools: false,
            supports_vision: false,
        },
    );
    let selected = |id: &str| options.iter().find(|option| option.id == id);
    let section_key = match kind {
        AiEndpointKind::Online => "settings.onlineEndpointSection",
        AiEndpointKind::Airplane => "settings.airplaneEndpointSection",
    };
    let url_key = match kind {
        AiEndpointKind::Online => "settings.onlineEndpointUrl",
        AiEndpointKind::Airplane => "settings.airplaneEndpointUrl",
    };
    let api_key = match kind {
        AiEndpointKind::Online => "settings.onlineApiKey",
        AiEndpointKind::Airplane => "settings.airplaneApiKey",
    };
    let keychain_placeholder = t(locale, "settings.keychainConfigured");

    inputs::card(
        column![
            text(t(locale, section_key)).size(15),
            row![
                inputs::labeled_input(
                    &t(locale, url_key),
                    url_placeholder,
                    &state.endpoint_url,
                    move |value| {
                        Message::Settings(SettingsMsg::AiEndpointUrlChanged(kind, value))
                    }
                ),
                button(text(t(locale, "settings.refreshModels")).size(13))
                    .on_press(Message::Settings(SettingsMsg::RefreshAiModels(kind)))
                    .style(inputs::secondary_button)
                    .padding([6, 16]),
            ]
            .spacing(12)
            .align_y(Alignment::End),
            inputs::labeled_secure_input(
                &t(locale, api_key),
                if state.api_key_configured {
                    &keychain_placeholder
                } else {
                    "sk-..."
                },
                &state.api_key_input,
                move |value| Message::Settings(SettingsMsg::AiApiKeyChanged(kind, value)),
            ),
            inputs::labeled_select(
                &t(locale, "settings.chatModel"),
                &options,
                selected(&state.chat_model),
                move |option| Message::Settings(SettingsMsg::AiChatModelChanged(kind, option.id)),
            ),
            inputs::labeled_checkbox(
                &t(locale, "settings.modelSupportsTools"),
                state.chat_supports_tools,
                move |value| Message::Settings(SettingsMsg::AiToolsChanged(kind, value)),
            ),
            inputs::labeled_select(
                &t(locale, "settings.titleModel"),
                &options,
                selected(&state.title_model),
                move |option| Message::Settings(SettingsMsg::AiTitleModelChanged(kind, option.id)),
            ),
            inputs::labeled_select(
                &t(locale, "settings.imageAnalysisModel"),
                &options,
                selected(&state.image_model),
                move |option| Message::Settings(SettingsMsg::AiImageModelChanged(kind, option.id)),
            ),
            inputs::labeled_checkbox(
                &t(locale, "settings.modelSupportsVision"),
                state.image_supports_vision,
                move |value| Message::Settings(SettingsMsg::AiVisionChanged(kind, value)),
            ),
            button(text(t(locale, "settings.testChat")).size(13))
                .on_press(Message::Settings(SettingsMsg::TestAi(kind)))
                .style(inputs::secondary_button)
                .padding([6, 16]),
        ]
        .spacing(8),
    )
    .into()
}

fn section_technology<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    column![
        text(t(locale, "settings.luaRuntimeOnly"))
            .size(13)
            .color(Color::from_rgb(0.7, 0.72, 0.78)),
        inputs::labeled_checkbox(
            &t(locale, "settings.semanticSimilarityEnabled"),
            state.semantic_similarity_enabled,
            |value| Message::Settings(SettingsMsg::SemanticSimilarityChanged(value)),
        ),
    ]
    .spacing(8)
    .padding([0, 16])
    .into()
}

fn section_publishing<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    let host = inputs::labeled_input(&t(locale, "settings.sshHost"), "", &state.ssh_host, |s| {
        Message::Settings(SettingsMsg::SshHostChanged(s))
    });
    let user = inputs::labeled_input(
        &t(locale, "settings.sshUsername"),
        "",
        &state.ssh_username,
        |s| Message::Settings(SettingsMsg::SshUsernameChanged(s)),
    );
    let path = inputs::labeled_input(
        &t(locale, "settings.sshRemotePath"),
        "",
        &state.ssh_remote_path,
        |s| Message::Settings(SettingsMsg::SshRemotePathChanged(s)),
    );
    let btns = row![
        button(text(t(locale, "common.save")).size(13))
            .on_press(Message::Settings(SettingsMsg::SavePublishing))
            .style(inputs::primary_button)
            .padding([6, 16]),
        button(text(t(locale, "settings.clear")).size(13))
            .on_press(Message::Settings(SettingsMsg::ClearPublishing))
            .style(inputs::danger_button)
            .padding([6, 16]),
    ]
    .spacing(8);

    column![host, user, path, btns]
        .spacing(8)
        .padding([0, 16])
        .into()
}

fn section_data<'a>(locale: UiLocale) -> Element<'a, Message> {
    let rebuild_btns = column![
        button(text(t(locale, "settings.rebuildPosts")).size(13))
            .on_press(Message::Settings(SettingsMsg::RebuildPosts))
            .style(inputs::secondary_button)
            .padding([6, 16])
            .width(Length::Fill),
        button(text(t(locale, "settings.rebuildMedia")).size(13))
            .on_press(Message::Settings(SettingsMsg::RebuildMedia))
            .style(inputs::secondary_button)
            .padding([6, 16])
            .width(Length::Fill),
        button(text(t(locale, "settings.rebuildScripts")).size(13))
            .on_press(Message::Settings(SettingsMsg::RebuildScripts))
            .style(inputs::secondary_button)
            .padding([6, 16])
            .width(Length::Fill),
        button(text(t(locale, "settings.rebuildTemplates")).size(13))
            .on_press(Message::Settings(SettingsMsg::RebuildTemplates))
            .style(inputs::secondary_button)
            .padding([6, 16])
            .width(Length::Fill),
        button(text(t(locale, "settings.rebuildLinks")).size(13))
            .on_press(Message::Settings(SettingsMsg::RebuildLinks))
            .style(inputs::secondary_button)
            .padding([6, 16])
            .width(Length::Fill),
        button(text(t(locale, "settings.rebuildSearchIndex")).size(13))
            .on_press(Message::Settings(SettingsMsg::RebuildSearchIndex))
            .style(inputs::secondary_button)
            .padding([6, 16])
            .width(Length::Fill),
        button(text(t(locale, "settings.regenerateThumbnails")).size(13))
            .on_press(Message::Settings(SettingsMsg::RegenerateThumbnails))
            .style(inputs::secondary_button)
            .padding([6, 16])
            .width(Length::Fill),
    ]
    .spacing(4);

    let open = button(text(t(locale, "settings.openDataFolder")).size(13))
        .on_press(Message::Settings(SettingsMsg::OpenDataFolder))
        .style(inputs::secondary_button)
        .padding([6, 16]);

    let install_cli = button(text(t(locale, "settings.installCli")).size(13))
        .on_press(Message::Settings(SettingsMsg::InstallCli))
        .style(inputs::secondary_button)
        .padding([6, 16]);

    column![rebuild_btns, row![open, install_cli].spacing(8)]
        .spacing(12)
        .padding([0, 16])
        .into()
}

fn section_mcp<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    let status = if state.mcp_running {
        t(locale, "settings.mcpRunning")
    } else {
        t(locale, "settings.mcpStopped")
    };
    let status_color = if state.mcp_running {
        Color::from_rgb(0.35, 0.78, 0.48)
    } else {
        inputs::LABEL_COLOR
    };
    let server = column![
        iced::widget::checkbox(t(locale, "settings.mcpEnable"), state.mcp_enabled)
            .on_toggle(|value| Message::Settings(SettingsMsg::McpEnabledChanged(value))),
        row![
            text(status).size(13).color(status_color),
            text(state.mcp_endpoint.clone())
                .size(12)
                .color(inputs::LABEL_COLOR),
            button(text(t(locale, "settings.mcpRefresh")).size(12))
                .on_press(Message::Settings(SettingsMsg::McpRefresh))
                .style(inputs::secondary_button)
                .padding([5, 10]),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    ]
    .spacing(8);

    let proposal_rows = if state.mcp_proposals.is_empty() {
        column![
            text(t(locale, "settings.mcpNoProposals"))
                .size(13)
                .color(inputs::LABEL_COLOR)
        ]
    } else {
        column(state.mcp_proposals.iter().map(|proposal| {
            let kind = t(locale, proposal_kind_i18n_key(proposal.kind));
            let summary = format!("{kind} · {}", proposal.entity_id);
            inputs::card(
                row![
                    column![
                        text(summary).size(13),
                        text(t(locale, proposal_status_i18n_key(proposal.status)))
                            .size(11)
                            .color(inputs::LABEL_COLOR),
                        text(proposal.data.clone())
                            .size(11)
                            .color(inputs::LABEL_COLOR),
                    ]
                    .spacing(2)
                    .width(Length::Fill),
                    button(text(t(locale, "settings.mcpApprove")).size(12))
                        .on_press(Message::Settings(SettingsMsg::McpProposalAccepted(
                            proposal.id.clone()
                        )))
                        .style(inputs::primary_button)
                        .padding([5, 10]),
                    button(text(t(locale, "settings.mcpReject")).size(12))
                        .on_press(Message::Settings(SettingsMsg::McpProposalRejected(
                            proposal.id.clone()
                        )))
                        .style(inputs::danger_button)
                        .padding([5, 10]),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .into()
        }))
        .spacing(6)
    };

    let agents = column(state.mcp_agents.iter().map(|agent| {
        let configured = agent.configured;
        column![
            iced::widget::checkbox(agent.label.clone(), configured).on_toggle({
                let agent = agent.agent;
                move |_| Message::Settings(SettingsMsg::McpAgentToggled(agent))
            }),
            text(agent.config_path.clone())
                .size(11)
                .color(inputs::LABEL_COLOR),
        ]
        .spacing(2)
        .into()
    }))
    .spacing(8);

    column![
        server,
        text(t(locale, "settings.mcpProposals")).size(14),
        proposal_rows,
        text(t(locale, "settings.mcpAgents")).size(14),
        agents,
    ]
    .spacing(12)
    .padding([0, 16])
    .into()
}

fn proposal_kind_i18n_key(kind: bds_core::model::ProposalKind) -> &'static str {
    match kind {
        bds_core::model::ProposalKind::DraftPost => "settings.mcpKindDraftPost",
        bds_core::model::ProposalKind::ProposeScript => "settings.mcpKindScript",
        bds_core::model::ProposalKind::ProposeTemplate => "settings.mcpKindTemplate",
        bds_core::model::ProposalKind::ProposeMediaTranslation => {
            "settings.mcpKindMediaTranslation"
        }
        bds_core::model::ProposalKind::ProposeMediaMetadata => "settings.mcpKindMediaMetadata",
        bds_core::model::ProposalKind::ProposePostMetadata => "settings.mcpKindPostMetadata",
    }
}

fn proposal_status_i18n_key(status: bds_core::model::ProposalStatus) -> &'static str {
    match status {
        bds_core::model::ProposalStatus::Pending => "settings.mcpStatusPending",
        bds_core::model::ProposalStatus::Executing => "settings.mcpStatusExecuting",
        bds_core::model::ProposalStatus::Accepted => "settings.mcpStatusAccepted",
        bds_core::model::ProposalStatus::Rejected => "settings.mcpStatusRejected",
        bds_core::model::ProposalStatus::Expired => "settings.mcpStatusExpired",
    }
}

#[cfg(test)]
mod tests {
    use super::{SettingsSection, SettingsViewState};

    #[test]
    fn focus_section_collapses_other_sections_and_clears_search() {
        let mut state = SettingsViewState {
            search_query: "publish".to_string(),
            ..SettingsViewState::default()
        };

        state.focus_section(SettingsSection::Publishing);

        assert_eq!(state.active_section, Some(SettingsSection::Publishing));
        assert!(state.search_query.is_empty());
        assert!(!state.collapsed.contains(&SettingsSection::Publishing));
        assert!(state.collapsed.contains(&SettingsSection::Project));
        assert!(state.collapsed.contains(&SettingsSection::MCP));
    }

    #[test]
    fn focused_section_is_rendered_first() {
        let mut state = SettingsViewState::default();
        state.focus_section(SettingsSection::Technology);

        let ordered = state.ordered_sections();

        assert_eq!(ordered.first(), Some(&SettingsSection::Technology));
        assert_eq!(ordered.len(), SettingsSection::all().len());
    }
}
