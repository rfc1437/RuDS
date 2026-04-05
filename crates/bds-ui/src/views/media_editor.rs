use std::path::Path;

use iced::widget::{button, column, container, image, row, scrollable, text, Space};
use iced::{Color, Element, Length};

use bds_core::i18n::UiLocale;
use bds_core::model::Media;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

/// State for an open media editor.
#[derive(Debug, Clone)]
pub struct MediaEditorState {
    pub media_id: String,
    pub filename: String,
    pub original_name: String,
    pub mime_type: String,
    pub size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub title: String,
    pub alt: String,
    pub caption: String,
    pub author: String,
    pub language: String,
    pub file_path: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub is_dirty: bool,
}

impl MediaEditorState {
    pub fn from_media(media: &Media) -> Self {
        Self {
            media_id: media.id.clone(),
            filename: media.filename.clone(),
            original_name: media.original_name.clone(),
            mime_type: media.mime_type.clone(),
            size: media.size,
            width: media.width,
            height: media.height,
            title: media.title.clone().unwrap_or_default(),
            alt: media.alt.clone().unwrap_or_default(),
            caption: media.caption.clone().unwrap_or_default(),
            author: media.author.clone().unwrap_or_default(),
            language: media.language.clone().unwrap_or_default(),
            file_path: media.file_path.clone(),
            tags: media.tags.clone(),
            created_at: media.created_at,
            updated_at: media.updated_at,
            is_dirty: false,
        }
    }
}

/// Media editor messages.
#[derive(Debug, Clone)]
pub enum MediaEditorMsg {
    TitleChanged(String),
    AltChanged(String),
    CaptionChanged(String),
    AuthorChanged(String),
    Save,
    Delete,
}

/// Render the media editor view.
pub fn view<'a>(
    state: &'a MediaEditorState,
    locale: UiLocale,
    data_dir: Option<&Path>,
) -> Element<'a, Message> {
    let header = inputs::toolbar(
        vec![
            text(state.original_name.clone()).size(18).into(),
        ],
        vec![
            button(text(t(locale, "common.save")).size(13))
                .on_press(Message::MediaEditor(MediaEditorMsg::Save))
                .style(inputs::primary_button)
                .padding([6, 16])
                .into(),
            button(text(t(locale, "modal.confirmDelete.delete")).size(13))
                .on_press(Message::MediaEditor(MediaEditorMsg::Delete))
                .style(inputs::danger_button)
                .padding([6, 16])
                .into(),
        ],
    );

    // Preview section
    let preview: Element<'a, Message> = if state.mime_type.starts_with("image/") {
        if let Some(dir) = data_dir {
            let img_path = dir.join(&state.file_path);
            if img_path.exists() {
                container(
                    image(img_path.to_string_lossy().to_string())
                        .width(Length::Fill)
                        .height(Length::Fixed(300.0)),
                )
                .width(Length::Fill)
                .into()
            } else {
                no_preview()
            }
        } else {
            no_preview()
        }
    } else {
        no_preview()
    };

    // File info
    let dimensions = match (state.width, state.height) {
        (Some(w), Some(h)) => format!("{w} × {h}"),
        _ => String::new(),
    };
    let size_str = format_file_size(state.size);
    let info = row![
        text(format!("{} • {} • {}", state.mime_type, size_str, dimensions))
            .size(12)
            .color(Color::from_rgb(0.55, 0.58, 0.65)),
    ]
    .padding(8);

    // Metadata fields
    let title_input = inputs::labeled_input(
        &t(locale, "editor.title"),
        &t(locale, "editor.titlePlaceholder"),
        &state.title,
        |s| Message::MediaEditor(MediaEditorMsg::TitleChanged(s)),
    );
    let alt_input = inputs::labeled_input(
        &t(locale, "editor.alt"),
        &t(locale, "editor.altPlaceholder"),
        &state.alt,
        |s| Message::MediaEditor(MediaEditorMsg::AltChanged(s)),
    );
    let meta_row1 = row![title_input, alt_input].spacing(16).width(Length::Fill);

    let caption_input = inputs::labeled_input(
        &t(locale, "editor.caption"),
        "",
        &state.caption,
        |s| Message::MediaEditor(MediaEditorMsg::CaptionChanged(s)),
    );
    let author_input = inputs::labeled_input(
        &t(locale, "editor.author"),
        "",
        &state.author,
        |s| Message::MediaEditor(MediaEditorMsg::AuthorChanged(s)),
    );
    let meta_row2 = row![caption_input, author_input].spacing(16).width(Length::Fill);

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
            preview,
            info,
            inputs::section_header(&t(locale, "editor.metadata")),
            meta_row1,
            meta_row2,
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

fn no_preview<'a>() -> Element<'a, Message> {
    container(
        text("No preview available")
            .size(14)
            .color(Color::from_rgb(0.5, 0.5, 0.5)),
    )
    .width(Length::Fill)
    .height(Length::Fixed(200.0))
    .center_x(Length::Fill)
    .center_y(Length::Fixed(200.0))
    .into()
}

fn format_file_size(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
