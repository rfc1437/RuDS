use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

/// State for the translation editor (post translation).
#[derive(Debug, Clone)]
pub struct TranslationEditorState {
    pub post_id: String,
    pub post_title: String,
    pub language: String,
    pub title: String,
    pub excerpt: String,
    pub content: String,
    pub status: String, // draft | published
    pub created_at: i64,
    pub updated_at: i64,
    pub is_dirty: bool,
}

impl TranslationEditorState {
    pub fn new(post_id: String, post_title: String, language: String) -> Self {
        Self {
            post_id,
            post_title,
            language,
            title: String::new(),
            excerpt: String::new(),
            content: String::new(),
            status: "draft".to_string(),
            created_at: 0,
            updated_at: 0,
            is_dirty: false,
        }
    }
}

/// Translation editor messages.
#[derive(Debug, Clone)]
pub enum TranslationEditorMsg {
    TitleChanged(String),
    ExcerptChanged(String),
    ContentChanged(String),
    Save,
    Publish,
    Delete,
}

/// Render the translation editor view.
pub fn view<'a>(
    state: &'a TranslationEditorState,
    locale: UiLocale,
) -> Element<'a, Message> {
    let header = inputs::toolbar(
        vec![
            text(format!("{} [{}]", &state.post_title, &state.language))
                .size(18)
                .into(),
            status_badge(&state.status),
        ],
        vec![
            button(text(t(locale, "common.save")).size(13))
                .on_press(Message::TranslationEditor(TranslationEditorMsg::Save))
                .style(inputs::primary_button)
                .padding([6, 16])
                .into(),
            button(text(t(locale, "editor.publish")).size(13))
                .on_press(Message::TranslationEditor(TranslationEditorMsg::Publish))
                .style(inputs::primary_button)
                .padding([6, 16])
                .into(),
            button(text(t(locale, "modal.confirmDelete.delete")).size(13))
                .on_press(Message::TranslationEditor(TranslationEditorMsg::Delete))
                .style(inputs::danger_button)
                .padding([6, 16])
                .into(),
        ],
    );

    let title_input = inputs::labeled_input(
        &t(locale, "editor.title"),
        &t(locale, "editor.titlePlaceholder"),
        &state.title,
        |s| Message::TranslationEditor(TranslationEditorMsg::TitleChanged(s)),
    );

    let excerpt_input = inputs::labeled_input(
        &t(locale, "editor.excerpt"),
        &t(locale, "editor.excerptPlaceholder"),
        &state.excerpt,
        |s| Message::TranslationEditor(TranslationEditorMsg::ExcerptChanged(s)),
    );

    let content_section: Element<'a, Message> = column![
        inputs::section_header(&t(locale, "editor.content")),
        container(
            text_input(&t(locale, "editor.contentPlaceholder"), &state.content)
                .on_input(|s| Message::TranslationEditor(TranslationEditorMsg::ContentChanged(s)))
                .size(14)
        )
        .width(Length::Fill)
        .height(Length::Fixed(300.0)),
    ]
    .spacing(8)
    .width(Length::Fill)
    .into();

    let footer = row![
        inputs::date_label(&t(locale, "editor.createdAt"), state.created_at),
        Space::with_width(Length::Fixed(24.0)),
        inputs::date_label(&t(locale, "editor.updatedAt"), state.updated_at),
    ]
    .padding(8);

    let body = scrollable(
        column![
            header,
            title_input,
            excerpt_input,
            content_section,
            footer,
        ]
        .spacing(12)
        .padding(16)
        .width(Length::Fill),
    );

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn status_badge<'a>(status: &str) -> Element<'a, Message> {
    let (label, color) = match status {
        "published" => ("Published", Color::from_rgb(0.2, 0.7, 0.3)),
        _ => ("Draft", Color::from_rgb(0.8, 0.7, 0.2)),
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
