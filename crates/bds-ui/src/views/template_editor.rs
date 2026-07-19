use std::cell::RefCell;

use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::{Template, TemplateKind, TemplateStatus};
use bds_editor::{CodeEditor, EditorBuffer, EditorMessage, highlighter};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

/// State for an open template editor.
pub struct TemplateEditorState {
    pub template_id: String,
    pub title: String,
    pub slug: String,
    pub kind: TemplateKind,
    pub enabled: bool,
    pub content: String,
    pub editor_buffer: RefCell<EditorBuffer>,
    pub status: TemplateStatus,
    pub version: i32,
    pub created_at: i64,
    pub updated_at: i64,
    pub validation_error: Option<String>,
    pub is_dirty: bool,
}

impl std::fmt::Debug for TemplateEditorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TemplateEditorState")
            .field("template_id", &self.template_id)
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

impl Clone for TemplateEditorState {
    fn clone(&self) -> Self {
        Self {
            template_id: self.template_id.clone(),
            title: self.title.clone(),
            slug: self.slug.clone(),
            kind: self.kind.clone(),
            enabled: self.enabled,
            content: self.content.clone(),
            editor_buffer: RefCell::new(EditorBuffer::new(&self.content)),
            status: self.status.clone(),
            version: self.version,
            created_at: self.created_at,
            updated_at: self.updated_at,
            validation_error: self.validation_error.clone(),
            is_dirty: self.is_dirty,
        }
    }
}

impl TemplateEditorState {
    pub fn from_template(tpl: &Template) -> Self {
        let content = tpl.content.clone().unwrap_or_default();
        Self {
            template_id: tpl.id.clone(),
            title: tpl.title.clone(),
            slug: tpl.slug.clone(),
            kind: tpl.kind.clone(),
            enabled: tpl.enabled,
            content: content.clone(),
            editor_buffer: RefCell::new(EditorBuffer::new(&content)),
            status: tpl.status.clone(),
            version: tpl.version,
            created_at: tpl.created_at,
            updated_at: tpl.updated_at,
            validation_error: None,
            is_dirty: false,
        }
    }
}

/// Template kind display helper.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateKindOption(pub TemplateKind);

impl std::fmt::Display for TemplateKindOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            TemplateKind::Post => write!(f, "Post"),
            TemplateKind::List => write!(f, "List"),
            TemplateKind::NotFound => write!(f, "Not Found"),
            TemplateKind::Partial => write!(f, "Partial"),
        }
    }
}

/// Template editor messages.
#[derive(Debug, Clone)]
pub enum TemplateEditorMsg {
    TitleChanged(String),
    SlugChanged(String),
    KindChanged(TemplateKindOption),
    EnabledChanged(bool),
    ContentChanged(String),
    Save,
    Validate,
    Delete,
}

/// Render the template editor view.
pub fn view<'a>(state: &'a TemplateEditorState, locale: UiLocale) -> Element<'a, Message> {
    let header = inputs::toolbar(
        vec![
            text(state.title.clone()).size(18).into(),
            status_badge(&state.status),
        ],
        vec![
            button(text(t(locale, "common.save")).size(13))
                .on_press(Message::TemplateEditor(TemplateEditorMsg::Save))
                .style(inputs::primary_button)
                .padding([6, 16])
                .into(),
            button(text(t(locale, "editor.validate")).size(13))
                .on_press(Message::TemplateEditor(TemplateEditorMsg::Validate))
                .style(inputs::secondary_button)
                .padding([6, 16])
                .into(),
            button(text(t(locale, "modal.confirmDelete.delete")).size(13))
                .on_press(Message::TemplateEditor(TemplateEditorMsg::Delete))
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
        |s| Message::TemplateEditor(TemplateEditorMsg::TitleChanged(s)),
    );
    let slug_input = inputs::labeled_input(
        &t(locale, "editor.slug"),
        &t(locale, "editor.slugPlaceholder"),
        &state.slug,
        |s| Message::TemplateEditor(TemplateEditorMsg::SlugChanged(s)),
    );
    let meta_row1 = row![title_input, slug_input]
        .spacing(16)
        .width(Length::Fill);

    // Metadata row 2: kind (select), enabled (checkbox)
    let kind_options = vec![
        TemplateKindOption(TemplateKind::Post),
        TemplateKindOption(TemplateKind::List),
        TemplateKindOption(TemplateKind::NotFound),
        TemplateKindOption(TemplateKind::Partial),
    ];
    let selected_kind = Some(TemplateKindOption(state.kind.clone()));
    let kind_select = inputs::labeled_select(
        &t(locale, "editor.kind"),
        &kind_options,
        selected_kind.as_ref(),
        |k| Message::TemplateEditor(TemplateEditorMsg::KindChanged(k)),
    );
    let enabled_check =
        inputs::labeled_checkbox(&t(locale, "editor.enabled"), state.enabled, |b| {
            Message::TemplateEditor(TemplateEditorMsg::EnabledChanged(b))
        });
    let meta_row2 = row![kind_select, enabled_check]
        .spacing(16)
        .align_y(iced::Alignment::End)
        .width(Length::Fill);

    // Content editor (CodeEditor with Liquid/HTML syntax highlighting)
    let content_section: Element<'a, Message> = inputs::card(
        column![
            inputs::section_header(&t(locale, "editor.content")),
            Element::from(
                CodeEditor::new(&state.editor_buffer, highlighter(), "liquid",).on_change(|msg| {
                    match msg {
                        EditorMessage::ContentChanged(s) => {
                            Message::TemplateEditor(TemplateEditorMsg::ContentChanged(s))
                        }
                        EditorMessage::SaveRequested => {
                            Message::TemplateEditor(TemplateEditorMsg::Save)
                        }
                    }
                })
            ),
        ]
        .spacing(10)
        .width(Length::Fill)
        .height(Length::Fill),
    )
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
    let metadata = inputs::card(
        column![meta_row1, meta_row2]
            .spacing(12)
            .width(Length::Fill),
    );
    let top_pane = scrollable(column![header, metadata].spacing(12).width(Length::Fill))
        .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
        .style(inputs::scrollable_style)
        .height(Length::Shrink);

    // Full layout: top pane (shrink), content (fill), validation + footer (shrink)
    column![top_pane, content_section, validation, footer,]
        .spacing(8)
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn status_badge<'a>(status: &TemplateStatus) -> Element<'a, Message> {
    let (label, color) = match status {
        TemplateStatus::Draft => ("Draft", Color::from_rgb(0.8, 0.7, 0.2)),
        TemplateStatus::Published => ("Published", Color::from_rgb(0.2, 0.7, 0.3)),
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
