use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::{Template, TemplateKind, TemplateStatus};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

/// State for an open template editor.
#[derive(Debug, Clone)]
pub struct TemplateEditorState {
    pub template_id: String,
    pub title: String,
    pub slug: String,
    pub kind: TemplateKind,
    pub enabled: bool,
    pub content: String,
    pub status: TemplateStatus,
    pub version: i32,
    pub created_at: i64,
    pub updated_at: i64,
    pub validation_error: Option<String>,
    pub is_dirty: bool,
}

impl TemplateEditorState {
    pub fn from_template(tpl: &Template) -> Self {
        Self {
            template_id: tpl.id.clone(),
            title: tpl.title.clone(),
            slug: tpl.slug.clone(),
            kind: tpl.kind.clone(),
            enabled: tpl.enabled,
            content: tpl.content.clone().unwrap_or_default(),
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
pub fn view<'a>(
    state: &'a TemplateEditorState,
    locale: UiLocale,
) -> Element<'a, Message> {
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
    let meta_row1 = row![title_input, slug_input].spacing(16).width(Length::Fill);

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
    let enabled_check = inputs::labeled_checkbox(
        &t(locale, "editor.enabled"),
        state.enabled,
        |b| Message::TemplateEditor(TemplateEditorMsg::EnabledChanged(b)),
    );
    let meta_row2 = row![kind_select, enabled_check].spacing(16).width(Length::Fill);

    // Content editor (text area placeholder — will use CodeEditor widget for liquid)
    let content_section: Element<'a, Message> = column![
        inputs::section_header(&t(locale, "editor.content")),
        container(
            text_input("", &state.content)
                .on_input(|s| Message::TemplateEditor(TemplateEditorMsg::ContentChanged(s)))
                .size(14)
        )
        .width(Length::Fill)
        .height(Length::Fixed(300.0)),
    ]
    .spacing(8)
    .width(Length::Fill)
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

    let body = scrollable(
        column![
            header,
            meta_row1,
            meta_row2,
            content_section,
            validation,
            footer,
        ]
        .spacing(12)
        .padding(16)
        .width(Length::Fill)
    );

    container(body)
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
