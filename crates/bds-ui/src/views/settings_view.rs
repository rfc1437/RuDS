use iced::widget::text::Shaping;
use iced::widget::{button, column, container, row, scrollable, text, text_editor, text_input};
use iced::{Alignment, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::{t, tw};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiModelOption {
    pub id: String,
    pub label: String,
    pub supports_vision: bool,
}

impl std::fmt::Display for AiModelOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
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
    pub offline_mode: bool,
    pub online_endpoint_url: String,
    pub online_endpoint_model: String,
    pub online_api_key_input: String,
    pub online_api_key_configured: bool,
    pub online_model_options: Vec<AiModelOption>,
    pub airplane_endpoint_url: String,
    pub airplane_endpoint_model: String,
    pub airplane_model_options: Vec<AiModelOption>,
    pub default_model: String,
    pub title_model: String,
    pub image_model: String,
    pub system_prompt: text_editor::Content,
    // Technology
    pub semantic_similarity_enabled: bool,
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
            offline_mode: self.offline_mode,
            online_endpoint_url: self.online_endpoint_url.clone(),
            online_endpoint_model: self.online_endpoint_model.clone(),
            online_api_key_input: self.online_api_key_input.clone(),
            online_api_key_configured: self.online_api_key_configured,
            online_model_options: self.online_model_options.clone(),
            airplane_endpoint_url: self.airplane_endpoint_url.clone(),
            airplane_endpoint_model: self.airplane_endpoint_model.clone(),
            airplane_model_options: self.airplane_model_options.clone(),
            default_model: self.default_model.clone(),
            title_model: self.title_model.clone(),
            image_model: self.image_model.clone(),
            system_prompt: text_editor::Content::with_text(&self.system_prompt.text()),
            semantic_similarity_enabled: self.semantic_similarity_enabled,
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
            offline_mode: false,
            online_endpoint_url: String::new(),
            online_endpoint_model: String::new(),
            online_api_key_input: String::new(),
            online_api_key_configured: false,
            online_model_options: Vec::new(),
            airplane_endpoint_url: String::new(),
            airplane_endpoint_model: String::new(),
            airplane_model_options: Vec::new(),
            default_model: String::new(),
            title_model: String::new(),
            image_model: String::new(),
            system_prompt: text_editor::Content::new(),
            semantic_similarity_enabled: false,
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
    BlogmarkCategoryChanged(String),
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
    OfflineModeChanged(bool),
    OnlineEndpointUrlChanged(String),
    OnlineEndpointModelChanged(String),
    OnlineApiKeyChanged(String),
    RefreshOnlineModels,
    AirplaneEndpointUrlChanged(String),
    AirplaneEndpointModelChanged(String),
    RefreshAirplaneModels,
    DefaultModelChanged(String),
    TitleModelChanged(String),
    ImageModelChanged(String),
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
    /// Navigate to a specific section from sidebar; expand it, collapse all others.
    FocusSection(SettingsSection),
}

/// Render the settings view.
pub fn view<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    let search = text_input(&t(locale, "sidebar.filter.search"), &state.search_query)
        .on_input(|s| Message::Settings(SettingsMsg::SearchChanged(s)))
        .size(14);

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
    .style(|_: &Theme, _| button::Style::default());

    if collapsed {
        return column![header].into();
    }

    let content: Element<'a, Message> = match section {
        SettingsSection::Project => section_project(state, locale),
        SettingsSection::Editor => section_editor(state, locale),
        SettingsSection::Content => section_content(state, locale),
        SettingsSection::AI => section_ai(state, locale),
        SettingsSection::Technology => section_technology(state, locale),
        SettingsSection::Publishing => section_publishing(state, locale),
        SettingsSection::Data => section_data(locale),
        SettingsSection::MCP => section_mcp(locale),
    };

    column![header, content].spacing(4).into()
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
            .size(14),
    ]
    .spacing(4);
    let data_path = row![
        inputs::labeled_input(&t(locale, "settings.dataPath"), "", &state.data_path, |s| {
            Message::Settings(SettingsMsg::DataPathChanged(s))
        },),
        button(text(t(locale, "settings.browse")).size(12))
            .on_press(Message::Settings(SettingsMsg::BrowseDataPath))
            .padding([6, 12]),
        button(text(t(locale, "settings.reset")).size(12))
            .on_press(Message::Settings(SettingsMsg::ResetDataPath))
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
    let blogmark_category = inputs::labeled_select(
        &t(locale, "settings.blogmarkCategory"),
        &state.categories,
        state
            .categories
            .iter()
            .find(|row| row.name == state.blogmark_category),
        |row| Message::Settings(SettingsMsg::BlogmarkCategoryChanged(row.name)),
    );
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
        blogmark_category,
        save,
    ]
    .spacing(8)
    .padding([0, 16])
    .into()
}

fn section_editor<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    let mode_options = vec![
        "wysiwyg".to_string(),
        "markdown".to_string(),
        "preview".to_string(),
    ];
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
                container(
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
    let active_model_options = if state.offline_mode {
        &state.airplane_model_options
    } else {
        &state.online_model_options
    };
    let model_options = std::iter::once(AiModelOption {
        id: String::new(),
        label: t(locale, "tags.noTemplate"),
        supports_vision: false,
    })
    .chain(active_model_options.iter().cloned())
    .collect::<Vec<_>>();
    let image_model_options = std::iter::once(AiModelOption {
        id: String::new(),
        label: t(locale, "tags.noTemplate"),
        supports_vision: false,
    })
    .chain(
        active_model_options
            .iter()
            .filter(|option| option.supports_vision)
            .cloned(),
    )
    .collect::<Vec<_>>();

    let offline = inputs::labeled_checkbox(
        &t(locale, "settings.offlineMode"),
        state.offline_mode,
        |b| Message::Settings(SettingsMsg::OfflineModeChanged(b)),
    );
    let online_url = inputs::labeled_input(
        &t(locale, "settings.onlineEndpointUrl"),
        "https://api.example.com/v1",
        &state.online_endpoint_url,
        |value| Message::Settings(SettingsMsg::OnlineEndpointUrlChanged(value)),
    );
    let online_model = inputs::labeled_input(
        &t(locale, "settings.onlineEndpointModel"),
        "gpt-4.1-mini",
        &state.online_endpoint_model,
        |value| Message::Settings(SettingsMsg::OnlineEndpointModelChanged(value)),
    );
    let keychain_placeholder = t(locale, "settings.keychainConfigured");
    let online_api_key = inputs::labeled_input(
        &t(locale, "settings.onlineApiKey"),
        if state.online_api_key_configured {
            &keychain_placeholder
        } else {
            "sk-..."
        },
        &state.online_api_key_input,
        |value| Message::Settings(SettingsMsg::OnlineApiKeyChanged(value)),
    );
    let online_refresh = button(text(t(locale, "settings.refreshModels")).size(13))
        .on_press(Message::Settings(SettingsMsg::RefreshOnlineModels))
        .padding([6, 16]);

    let airplane_url = inputs::labeled_input(
        &t(locale, "settings.airplaneEndpointUrl"),
        "http://localhost:11434/v1",
        &state.airplane_endpoint_url,
        |value| Message::Settings(SettingsMsg::AirplaneEndpointUrlChanged(value)),
    );
    let airplane_model = inputs::labeled_input(
        &t(locale, "settings.airplaneEndpointModel"),
        "llama3.2",
        &state.airplane_endpoint_model,
        |value| Message::Settings(SettingsMsg::AirplaneEndpointModelChanged(value)),
    );
    let airplane_refresh = button(text(t(locale, "settings.refreshModels")).size(13))
        .on_press(Message::Settings(SettingsMsg::RefreshAirplaneModels))
        .padding([6, 16]);

    let default_model = inputs::labeled_select(
        &t(locale, "settings.defaultModel"),
        &model_options,
        model_options
            .iter()
            .find(|option| option.id == state.default_model),
        |option| Message::Settings(SettingsMsg::DefaultModelChanged(option.id)),
    );
    let title_model = inputs::labeled_select(
        &t(locale, "settings.titleModel"),
        &model_options,
        model_options
            .iter()
            .find(|option| option.id == state.title_model),
        |option| Message::Settings(SettingsMsg::TitleModelChanged(option.id)),
    );
    let image_model = inputs::labeled_select(
        &t(locale, "settings.imageAnalysisModel"),
        &image_model_options,
        image_model_options
            .iter()
            .find(|option| option.id == state.image_model),
        |option| Message::Settings(SettingsMsg::ImageModelChanged(option.id)),
    );
    let prompt = column![
        text(t(locale, "settings.systemPrompt"))
            .size(12)
            .color(inputs::LABEL_COLOR)
            .shaping(Shaping::Advanced),
        text_editor(&state.system_prompt)
            .on_action(|a| Message::Settings(SettingsMsg::SystemPromptAction(a)))
            .height(Length::Fixed(200.0))
            .size(14),
    ]
    .spacing(4);
    let btns = row![
        button(text(t(locale, "common.save")).size(13))
            .on_press(Message::Settings(SettingsMsg::SaveAi))
            .style(inputs::primary_button)
            .padding([6, 16]),
        button(text(t(locale, "settings.resetToDefault")).size(13))
            .on_press(Message::Settings(SettingsMsg::ResetSystemPrompt))
            .padding([6, 16]),
    ]
    .spacing(8);

    column![
        offline,
        text(t(locale, "settings.onlineEndpointSection"))
            .size(12)
            .color(inputs::LABEL_COLOR),
        online_url,
        row![online_model, online_api_key].spacing(12),
        online_refresh,
        text(t(locale, "settings.airplaneEndpointSection"))
            .size(12)
            .color(inputs::LABEL_COLOR),
        airplane_url,
        row![airplane_model, airplane_refresh].spacing(12),
        default_model,
        title_model,
        image_model,
        prompt,
        btns,
    ]
    .spacing(8)
    .padding([0, 16])
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
            .padding([6, 16])
            .width(Length::Fill),
        button(text(t(locale, "settings.rebuildMedia")).size(13))
            .on_press(Message::Settings(SettingsMsg::RebuildMedia))
            .padding([6, 16])
            .width(Length::Fill),
        button(text(t(locale, "settings.rebuildScripts")).size(13))
            .on_press(Message::Settings(SettingsMsg::RebuildScripts))
            .padding([6, 16])
            .width(Length::Fill),
        button(text(t(locale, "settings.rebuildTemplates")).size(13))
            .on_press(Message::Settings(SettingsMsg::RebuildTemplates))
            .padding([6, 16])
            .width(Length::Fill),
        button(text(t(locale, "settings.rebuildLinks")).size(13))
            .on_press(Message::Settings(SettingsMsg::RebuildLinks))
            .padding([6, 16])
            .width(Length::Fill),
        button(text(t(locale, "settings.rebuildSearchIndex")).size(13))
            .on_press(Message::Settings(SettingsMsg::RebuildSearchIndex))
            .padding([6, 16])
            .width(Length::Fill),
        button(text(t(locale, "settings.regenerateThumbnails")).size(13))
            .on_press(Message::Settings(SettingsMsg::RegenerateThumbnails))
            .padding([6, 16])
            .width(Length::Fill),
    ]
    .spacing(4);

    let open = button(text(t(locale, "settings.openDataFolder")).size(13))
        .on_press(Message::Settings(SettingsMsg::OpenDataFolder))
        .padding([6, 16]);

    column![rebuild_btns, open]
        .spacing(12)
        .padding([0, 16])
        .into()
}

fn section_mcp<'a>(locale: UiLocale) -> Element<'a, Message> {
    // MCP status and agent toggles — placeholder for M3
    text(t(locale, "settings.mcpPlaceholder"))
        .size(13)
        .color(Color::from_rgb(0.5, 0.5, 0.5))
        .into()
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
