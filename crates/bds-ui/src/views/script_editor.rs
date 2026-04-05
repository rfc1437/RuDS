use std::cell::RefCell;

use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::{Script, ScriptKind, ScriptStatus};
use bds_editor::{CodeEditor, EditorBuffer, EditorMessage, highlighter};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

/// State for an open script editor.
pub struct ScriptEditorState {
    pub script_id: String,
    pub title: String,
    pub slug: String,
    pub file_path: String,
    pub kind: ScriptKind,
    pub entrypoint: String,
    pub enabled: bool,
    pub content: String,
    pub editor_buffer: RefCell<EditorBuffer>,
    pub status: ScriptStatus,
    pub version: i32,
    pub created_at: i64,
    pub updated_at: i64,
    pub discovered_entrypoints: Vec<String>,
    pub validation_error: Option<String>,
    pub is_dirty: bool,
}

impl std::fmt::Debug for ScriptEditorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScriptEditorState")
            .field("script_id", &self.script_id)
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

impl Clone for ScriptEditorState {
    fn clone(&self) -> Self {
        Self {
            script_id: self.script_id.clone(),
            title: self.title.clone(),
            slug: self.slug.clone(),
            file_path: self.file_path.clone(),
            kind: self.kind.clone(),
            entrypoint: self.entrypoint.clone(),
            enabled: self.enabled,
            content: self.content.clone(),
            editor_buffer: RefCell::new(EditorBuffer::new(&self.content)),
            status: self.status.clone(),
            version: self.version,
            created_at: self.created_at,
            updated_at: self.updated_at,
            discovered_entrypoints: self.discovered_entrypoints.clone(),
            validation_error: self.validation_error.clone(),
            is_dirty: self.is_dirty,
        }
    }
}

impl ScriptEditorState {
    pub fn from_script(script: &Script) -> Self {
        let content = script.content.clone().unwrap_or_default();
        let discovered = bds_core::engine::script::discover_entrypoints(&content);
        Self {
            script_id: script.id.clone(),
            title: script.title.clone(),
            slug: script.slug.clone(),
            file_path: script.file_path.clone(),
            kind: script.kind.clone(),
            entrypoint: script.entrypoint.clone(),
            enabled: script.enabled,
            content: content.clone(),
            editor_buffer: RefCell::new(EditorBuffer::new(&content)),
            status: script.status.clone(),
            version: script.version,
            created_at: script.created_at,
            updated_at: script.updated_at,
            discovered_entrypoints: discovered,
            validation_error: None,
            is_dirty: false,
        }
    }
}

/// Script kind display helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptKindOption(pub ScriptKind);

impl std::fmt::Display for ScriptKindOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            ScriptKind::Macro => write!(f, "Macro"),
            ScriptKind::Utility => write!(f, "Utility"),
            ScriptKind::Transform => write!(f, "Transform"),
        }
    }
}

/// Script editor messages.
#[derive(Debug, Clone)]
pub enum ScriptEditorMsg {
    TitleChanged(String),
    SlugChanged(String),
    KindChanged(ScriptKindOption),
    EntrypointChanged(String),
    EnabledChanged(bool),
    ContentChanged(String),
    Save,
    CheckSyntax,
    Run,
    Delete,
}

/// Render the script editor view.
pub fn view<'a>(
    state: &'a ScriptEditorState,
    locale: UiLocale,
) -> Element<'a, Message> {
    let header = inputs::toolbar(
        vec![
            text(state.title.clone()).size(18).into(),
            status_badge(&state.status),
        ],
        vec![
            button(text(t(locale, "common.save")).size(13))
                .on_press(Message::ScriptEditor(ScriptEditorMsg::Save))
                .style(inputs::primary_button)
                .padding([6, 16])
                .into(),
            button(text(t(locale, "editor.run")).size(13))
                .on_press(Message::ScriptEditor(ScriptEditorMsg::Run))
                .padding([6, 16])
                .into(),
            button(text(t(locale, "editor.checkSyntax")).size(13))
                .on_press(Message::ScriptEditor(ScriptEditorMsg::CheckSyntax))
                .padding([6, 16])
                .into(),
            button(text(t(locale, "modal.confirmDelete.delete")).size(13))
                .on_press(Message::ScriptEditor(ScriptEditorMsg::Delete))
                .style(inputs::danger_button)
                .padding([6, 16])
                .into(),
        ],
    );

    // Metadata row 1: title, slug
    let title_input = inputs::labeled_input(
        &t(locale, "editor.title"),
        &t(locale, "editor.titlePlaceholder"),
        &state.title,
        |s| Message::ScriptEditor(ScriptEditorMsg::TitleChanged(s)),
    );
    let slug_input = inputs::labeled_input(
        &t(locale, "editor.slug"),
        &t(locale, "editor.slugPlaceholder"),
        &state.slug,
        |s| Message::ScriptEditor(ScriptEditorMsg::SlugChanged(s)),
    );
    let meta_row1 = row![title_input, slug_input].spacing(16).width(Length::Fill);

    // Metadata row 2: kind, entrypoint, enabled
    let kind_options = vec![
        ScriptKindOption(ScriptKind::Macro),
        ScriptKindOption(ScriptKind::Utility),
        ScriptKindOption(ScriptKind::Transform),
    ];
    let selected_kind = Some(ScriptKindOption(state.kind.clone()));
    let kind_select = inputs::labeled_select(
        &t(locale, "editor.kind"),
        &kind_options,
        selected_kind.as_ref(),
        |k| Message::ScriptEditor(ScriptEditorMsg::KindChanged(k)),
    );

    // Entrypoint: show discovered functions as a select or text input
    let entrypoint_input = inputs::labeled_input(
        &t(locale, "editor.entrypoint"),
        "render",
        &state.entrypoint,
        |s| Message::ScriptEditor(ScriptEditorMsg::EntrypointChanged(s)),
    );

    let enabled_check = inputs::labeled_checkbox(
        &t(locale, "editor.enabled"),
        state.enabled,
        |b| Message::ScriptEditor(ScriptEditorMsg::EnabledChanged(b)),
    );
    let meta_row2 = row![kind_select, entrypoint_input, enabled_check]
        .spacing(16)
        .width(Length::Fill);

    // Content editor (CodeEditor with syntax highlighting based on file extension)
    let syntax_ext = if state.file_path.ends_with(".py") { "py" } else { "lua" };
    let content_section: Element<'a, Message> = column![
        inputs::section_header(&t(locale, "editor.content")),
        Element::from(
            CodeEditor::new(
                &state.editor_buffer,
                highlighter(),
                syntax_ext,
            )
            .on_change(|msg| match msg {
                EditorMessage::ContentChanged(s) => Message::ScriptEditor(ScriptEditorMsg::ContentChanged(s)),
                EditorMessage::SaveRequested => Message::ScriptEditor(ScriptEditorMsg::Save),
            })
        ),
    ]
    .spacing(8)
    .width(Length::Fill)
    .height(Length::Fill)
    .into();

    // Validation error
    let validation: Element<'a, Message> = if let Some(ref err) = state.validation_error {
        container(
            text(err.clone())
                .size(12)
                .color(Color::from_rgb(0.9, 0.3, 0.3)),
        )
        .padding(8)
        .into()
    } else {
        Space::new(0, 0).into()
    };

    // Footer
    let footer = row![
        inputs::date_label(&t(locale, "editor.createdAt"), state.created_at),
        Space::with_width(Length::Fixed(24.0)),
        inputs::date_label(&t(locale, "editor.updatedAt"), state.updated_at),
    ]
    .padding(8);

    // Top pane: header + metadata (scrollable for overflow)
    let top_pane = scrollable(
        column![
            header,
            meta_row1,
            meta_row2,
        ]
        .spacing(12)
        .padding(16)
        .width(Length::Fill)
    )
    .height(Length::Shrink);

    // Full layout: top pane (shrink), content (fill), validation + footer (shrink)
    column![
        top_pane,
        content_section,
        validation,
        footer,
    ]
    .spacing(4)
    .padding([0, 16])
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn status_badge<'a>(status: &ScriptStatus) -> Element<'a, Message> {
    let (label, color) = match status {
        ScriptStatus::Draft => ("Draft", Color::from_rgb(0.8, 0.7, 0.2)),
        ScriptStatus::Published => ("Published", Color::from_rgb(0.2, 0.7, 0.3)),
    };
    container(text(label).size(11).color(color))
        .padding([2, 8])
        .style(move |_: &Theme| container::Style {
            border: iced::Border {
                radius: 4.0.into(),
                width: 1.0,
                color,
            },
            ..container::Style::default()
        })
        .into()
}
