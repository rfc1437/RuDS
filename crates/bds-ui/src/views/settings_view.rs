use iced::widget::{button, column, container, row, scrollable, text, text_editor, text_input};
use iced::widget::text::Shaping;
use iced::{Alignment, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

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
    pub default_author: String,
    pub max_posts_per_page: String,
    // Editor
    pub default_mode: String,
    pub diff_view_style: String,
    pub wrap_long_lines: bool,
    pub hide_unchanged_regions: bool,
    // Publishing
    pub ssh_mode: String,
    pub ssh_host: String,
    pub ssh_username: String,
    pub ssh_remote_path: String,
    // AI
    pub offline_mode: bool,
    pub system_prompt: text_editor::Content,
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
            default_author: self.default_author.clone(),
            max_posts_per_page: self.max_posts_per_page.clone(),
            default_mode: self.default_mode.clone(),
            diff_view_style: self.diff_view_style.clone(),
            wrap_long_lines: self.wrap_long_lines,
            hide_unchanged_regions: self.hide_unchanged_regions,
            ssh_mode: self.ssh_mode.clone(),
            ssh_host: self.ssh_host.clone(),
            ssh_username: self.ssh_username.clone(),
            ssh_remote_path: self.ssh_remote_path.clone(),
            offline_mode: self.offline_mode,
            system_prompt: text_editor::Content::with_text(&self.system_prompt.text()),
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
            default_author: String::new(),
            max_posts_per_page: "50".to_string(),
            default_mode: "markdown".to_string(),
            diff_view_style: "inline".to_string(),
            wrap_long_lines: true,
            hide_unchanged_regions: false,
            ssh_mode: "rsync".to_string(),
            ssh_host: String::new(),
            ssh_username: String::new(),
            ssh_remote_path: String::new(),
            offline_mode: false,
            system_prompt: text_editor::Content::new(),
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
        if let Some(active) = &self.active_section {
            if let Some(index) = sections.iter().position(|section| section == active) {
                let focused = sections.remove(index);
                sections.insert(0, focused);
            }
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
    DefaultAuthorChanged(String),
    MaxPostsPerPageChanged(String),
    SaveProject,
    // Editor
    DefaultModeChanged(String),
    DiffViewStyleChanged(String),
    WrapLongLinesChanged(bool),
    HideUnchangedRegionsChanged(bool),
    SaveEditor,
    // Publishing
    SshModeChanged(String),
    SshHostChanged(String),
    SshUsernameChanged(String),
    SshRemotePathChanged(String),
    SavePublishing,
    ClearPublishing,
    // AI
    OfflineModeChanged(bool),
    SystemPromptAction(text_editor::Action),
    SaveSystemPrompt,
    ResetSystemPrompt,
    // Data maintenance
    RebuildPosts,
    RebuildMedia,
    RebuildScripts,
    RebuildTemplates,
    RebuildLinks,
    RegenerateThumbnails,
    OpenDataFolder,
    /// Navigate to a specific section from sidebar; expand it, collapse all others.
    FocusSection(SettingsSection),
}

/// Render the settings view.
pub fn view<'a>(
    state: &'a SettingsViewState,
    locale: UiLocale,
) -> Element<'a, Message> {
    let search = text_input(&t(locale, "sidebar.filter.search"), &state.search_query)
        .on_input(|s| Message::Settings(SettingsMsg::SearchChanged(s)))
        .size(14);

    let query_lower = state.search_query.to_lowercase();

    let mut sections = column![].spacing(12).width(Length::Fill);
    for section in state.ordered_sections() {
        let label = t(locale, section.i18n_key());
        if !query_lower.is_empty() && !label.to_lowercase().contains(&query_lower) {
            continue;
        }
        let collapsed = state.collapsed.contains(&section);
        let section_el = render_section(state, &section, &label, collapsed, locale);
        sections = sections.push(section_el);
    }

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
    .on_press(Message::Settings(SettingsMsg::ToggleSection(section.clone())))
    .padding([6, 8])
    .width(Length::Fill)
    .style(|_: &Theme, _| button::Style::default());

    if collapsed {
        return column![header].into();
    }

    let content: Element<'a, Message> = match section {
        SettingsSection::Project => section_project(state, locale),
        SettingsSection::Editor => section_editor(state, locale),
        SettingsSection::Content => section_content(locale),
        SettingsSection::AI => section_ai(state, locale),
        SettingsSection::Technology => section_technology(locale),
        SettingsSection::Publishing => section_publishing(state, locale),
        SettingsSection::Data => section_data(locale),
        SettingsSection::MCP => section_mcp(locale),
    };

    column![header, content]
        .spacing(4)
        .into()
}

fn section_project<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    let name = inputs::labeled_input(
        &t(locale, "settings.projectName"),
        "",
        &state.project_name,
        |s| Message::Settings(SettingsMsg::ProjectNameChanged(s)),
    );
    let desc = column![
        text(t(locale, "settings.projectDescription")).size(12).color(inputs::LABEL_COLOR).shaping(Shaping::Advanced),
        text_editor(&state.project_description)
            .on_action(|a| Message::Settings(SettingsMsg::ProjectDescriptionAction(a)))
            .height(Length::Fixed(80.0))
            .size(14),
    ]
    .spacing(4);
    let data_path = row![
        inputs::labeled_input(
            &t(locale, "settings.dataPath"),
            "",
            &state.data_path,
            |s| Message::Settings(SettingsMsg::DataPathChanged(s)),
        ),
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
    let save = button(text(t(locale, "common.save")).size(13))
        .on_press(Message::Settings(SettingsMsg::SaveProject))
        .style(inputs::primary_button)
        .padding([6, 16]);

    column![name, desc, data_path, url, author, max_posts, save]
        .spacing(8)
        .padding([0, 16])
        .into()
}

fn section_editor<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
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

    column![wrap, hide, save]
        .spacing(8)
        .padding([0, 16])
        .into()
}

fn section_content<'a>(locale: UiLocale) -> Element<'a, Message> {
    // Categories table placeholder — full implementation in future iteration
    text(t(locale, "settings.contentPlaceholder"))
        .size(13)
        .color(Color::from_rgb(0.5, 0.5, 0.5))
        .into()
}

fn section_ai<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    let offline = inputs::labeled_checkbox(
        &t(locale, "settings.offlineMode"),
        state.offline_mode,
        |b| Message::Settings(SettingsMsg::OfflineModeChanged(b)),
    );
    let prompt = column![
        text(t(locale, "settings.systemPrompt")).size(12).color(inputs::LABEL_COLOR).shaping(Shaping::Advanced),
        text_editor(&state.system_prompt)
            .on_action(|a| Message::Settings(SettingsMsg::SystemPromptAction(a)))
            .height(Length::Fixed(200.0))
            .size(14),
    ]
    .spacing(4);
    let btns = row![
        button(text(t(locale, "common.save")).size(13))
            .on_press(Message::Settings(SettingsMsg::SaveSystemPrompt))
            .style(inputs::primary_button)
            .padding([6, 16]),
        button(text(t(locale, "settings.resetToDefault")).size(13))
            .on_press(Message::Settings(SettingsMsg::ResetSystemPrompt))
            .padding([6, 16]),
    ]
    .spacing(8);

    column![offline, prompt, btns]
        .spacing(8)
        .padding([0, 16])
        .into()
}

fn section_technology<'a>(locale: UiLocale) -> Element<'a, Message> {
    text(t(locale, "settings.technologyPlaceholder"))
        .size(13)
        .color(Color::from_rgb(0.5, 0.5, 0.5))
        .into()
}

fn section_publishing<'a>(state: &'a SettingsViewState, locale: UiLocale) -> Element<'a, Message> {
    let host = inputs::labeled_input(
        &t(locale, "settings.sshHost"),
        "",
        &state.ssh_host,
        |s| Message::Settings(SettingsMsg::SshHostChanged(s)),
    );
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
