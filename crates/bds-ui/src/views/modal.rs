use std::path::Path;

use iced::widget::text::Shaping;
use iced::widget::{
    Space, button, checkbox, column, container, image, row, scrollable, text, text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::Media;
use bds_core::util::paths::thumbnail_path;

use crate::app::Message;
use crate::i18n::t;
use crate::views::post_editor::PostEditorMsg;

#[derive(Debug, Clone)]
pub struct InsertLinkResult {
    pub post_id: String,
    pub title: String,
    pub status: String,
    pub canonical_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiEntityTarget {
    Post(String),
    Media(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSuggestionField {
    pub key: String,
    pub label: String,
    pub current_value: String,
    pub suggested_value: String,
    pub accepted: bool,
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageTarget {
    pub code: String,
    pub name: String,
    pub flag_emoji: String,
    pub has_existing_translation: bool,
    pub existing_status: Option<String>,
}

/// Active modal state. Only one modal at a time.
#[derive(Debug, Clone)]
pub enum ModalState {
    /// Per modals.allium ConfirmDeleteModal: warning, entity name, references.
    ConfirmDelete {
        entity_name: String,
        references: Vec<String>,
        on_confirm: ConfirmAction,
    },
    /// Per modals.allium ConfirmDialog: generic confirmation.
    Confirm {
        title: String,
        message: String,
        on_confirm: ConfirmAction,
    },
    /// Per modals.allium InsertPostLinkModal: Ctrl+K link insertion.
    PostInsertLink {
        post_id: String,
        title: String,
        results: Vec<InsertLinkResult>,
        search_query: String,
        active_tab: PostInsertLinkTab,
        external_url: String,
        external_text: String,
    },
    /// Per modals.allium InsertMediaModal: grid for inserting media.
    InsertMedia {
        post_id: String,
        title: String,
        media_list: Vec<bds_core::model::Media>,
        search_query: String,
        link_only: bool,
    },
    /// Per modals.allium GalleryOverlay: full-screen media gallery.
    PostGallery {
        post_id: String,
        title: String,
        media_list: Vec<bds_core::model::Media>,
        selected_index: Option<usize>,
    },
    AISuggestions {
        target: AiEntityTarget,
        fields: Vec<AiSuggestionField>,
    },
    LanguagePicker {
        target: AiEntityTarget,
        source_language: String,
        available_targets: Vec<LanguageTarget>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PostInsertLinkTab {
    Internal,
    External,
}

/// What action to perform when modal is confirmed.
#[derive(Debug, Clone)]
pub enum ConfirmAction {
    DeleteProject(String),
    DeletePost(String),
    DeleteMedia(String),
    DeleteScript(String),
    DeleteTemplate(String),
    ForceDeleteTemplate(String),
    DeleteTag(String),
    MergeTags {
        sources: Vec<String>,
        target: String,
    },
}

// ── Modal backdrop style ──

fn backdrop_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba(0.0, 0.0, 0.0, 0.5))),
        ..container::Style::default()
    }
}

fn modal_box_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.18, 0.18, 0.22))),
        border: Border {
            color: Color::from_rgb(0.30, 0.30, 0.35),
            width: 1.0,
            radius: 8.0.into(),
        },
        ..container::Style::default()
    }
}

fn cancel_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.28, 0.28, 0.33),
        _ => Color::from_rgb(0.22, 0.22, 0.27),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.80, 0.80, 0.85),
        border: Border {
            color: Color::from_rgb(0.35, 0.35, 0.40),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

fn danger_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.85, 0.20, 0.20),
        _ => Color::from_rgb(0.75, 0.15, 0.15),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

fn confirm_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.20, 0.55, 0.85),
        _ => Color::from_rgb(0.15, 0.45, 0.75),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

fn resolve_thumbnail_file(data_dir: Option<&Path>, media: &Media) -> Option<String> {
    let data_dir = data_dir?;
    if !media.mime_type.starts_with("image/") {
        return None;
    }
    let thumb = data_dir.join(thumbnail_path(&media.id, "medium", "webp"));
    if thumb.exists() {
        Some(thumb.to_string_lossy().to_string())
    } else {
        let original = data_dir.join(&media.file_path);
        original
            .exists()
            .then(|| original.to_string_lossy().to_string())
    }
}

fn resolve_media_file(data_dir: Option<&Path>, media: &Media) -> Option<String> {
    let data_dir = data_dir?;
    let path = data_dir.join(&media.file_path);
    path.exists().then(|| path.to_string_lossy().to_string())
}

pub(crate) fn external_link_markdown(url: &str, display_text: &str) -> Option<String> {
    let trimmed_url = url.trim();
    if trimmed_url.is_empty() {
        return None;
    }
    let trimmed_text = display_text.trim();
    if trimmed_text.is_empty() {
        Some(trimmed_url.to_string())
    } else {
        Some(format!("[{trimmed_text}]({trimmed_url})"))
    }
}

#[cfg(test)]
fn gallery_step(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    ((current as isize + delta).rem_euclid(len as isize)) as usize
}

/// Render the modal overlay.
pub fn view(
    state: ModalState,
    locale: UiLocale,
    data_dir: Option<&Path>,
) -> Element<'static, Message> {
    let modal_content: Element<'static, Message> = match state {
        ModalState::ConfirmDelete {
            entity_name,
            references,
            on_confirm,
        } => {
            let title = text(t(locale, "modal.confirmDelete.title"))
                .size(16)
                .shaping(Shaping::Advanced)
                .color(Color::WHITE);

            let warning_icon = text("\u{26A0}")
                .size(20)
                .shaping(Shaping::Advanced)
                .color(Color::from_rgb(1.0, 0.80, 0.0));

            let entity_label = text(entity_name.clone())
                .size(14)
                .shaping(Shaping::Advanced)
                .color(Color::from_rgb(0.90, 0.90, 0.95));

            let warning_text = text(t(locale, "modal.confirmDelete.warning"))
                .size(12)
                .shaping(Shaping::Advanced)
                .color(Color::from_rgb(0.70, 0.70, 0.75));

            let mut content_col = column![
                row![warning_icon, Space::with_width(8.0), title].align_y(Alignment::Center),
                Space::with_height(12.0),
                entity_label,
                warning_text,
            ]
            .spacing(4);

            // Reference section
            if !references.is_empty() {
                content_col = content_col.push(Space::with_height(8.0));
                for r in references {
                    content_col = content_col.push(
                        text(format!("\u{2022} {r}"))
                            .size(11)
                            .shaping(Shaping::Advanced)
                            .color(Color::from_rgb(0.60, 0.60, 0.65)),
                    );
                }
            }

            let on_confirm_clone = on_confirm.clone();
            let buttons = row![
                button(
                    text(t(locale, "modal.confirmDelete.cancel"))
                        .size(13)
                        .shaping(Shaping::Advanced)
                )
                .on_press(Message::DismissModal)
                .padding([6, 16])
                .style(cancel_button_style),
                Space::with_width(Length::Fill),
                button(
                    text(t(locale, "modal.confirmDelete.delete"))
                        .size(13)
                        .shaping(Shaping::Advanced)
                )
                .on_press(Message::ConfirmModal(on_confirm_clone))
                .padding([6, 16])
                .style(danger_button_style),
            ];

            content_col = content_col.push(Space::with_height(16.0));
            content_col = content_col.push(buttons);

            container(content_col.padding(20))
                .width(Length::Fixed(380.0))
                .style(modal_box_style)
                .into()
        }

        ModalState::Confirm {
            title: dialog_title,
            message: dialog_message,
            on_confirm,
        } => {
            let title = text(dialog_title.clone())
                .size(16)
                .shaping(Shaping::Advanced)
                .color(Color::WHITE);

            let msg = text(dialog_message.clone())
                .size(13)
                .shaping(Shaping::Advanced)
                .color(Color::from_rgb(0.80, 0.80, 0.85));

            let on_confirm_clone = on_confirm.clone();
            let buttons = row![
                button(
                    text(t(locale, "modal.confirm.cancel"))
                        .size(13)
                        .shaping(Shaping::Advanced)
                )
                .on_press(Message::DismissModal)
                .padding([6, 16])
                .style(cancel_button_style),
                Space::with_width(Length::Fill),
                button(
                    text(t(locale, "modal.confirm.confirm"))
                        .size(13)
                        .shaping(Shaping::Advanced)
                )
                .on_press(Message::ConfirmModal(on_confirm_clone))
                .padding([6, 16])
                .style(confirm_button_style),
            ];

            let content_col = column![
                title,
                Space::with_height(12.0),
                msg,
                Space::with_height(16.0),
                buttons,
            ]
            .spacing(4);

            container(content_col.padding(20))
                .width(Length::Fixed(380.0))
                .style(modal_box_style)
                .into()
        }

        ModalState::PostInsertLink {
            post_id: _post_id,
            title: _title,
            results,
            search_query,
            active_tab,
            external_url,
            external_text,
        } => {
            let internal_tab = button(
                text(t(locale, "modal.postInsertLink.tabInternal"))
                    .size(13)
                    .shaping(Shaping::Advanced)
                    .color(if active_tab == PostInsertLinkTab::Internal {
                        Color::WHITE
                    } else {
                        Color::from_rgb(0.60, 0.60, 0.65)
                    }),
            )
            .on_press(Message::PostEditor(PostEditorMsg::PostInsertLinkTabSwitch(
                PostInsertLinkTab::Internal,
            )))
            .padding([8, 16])
            .style(cancel_button_style);

            let external_tab = button(
                text(t(locale, "modal.postInsertLink.tabExternal"))
                    .size(13)
                    .shaping(Shaping::Advanced)
                    .color(if active_tab == PostInsertLinkTab::External {
                        Color::WHITE
                    } else {
                        Color::from_rgb(0.60, 0.60, 0.65)
                    }),
            )
            .on_press(Message::PostEditor(PostEditorMsg::PostInsertLinkTabSwitch(
                PostInsertLinkTab::External,
            )))
            .padding([8, 16])
            .style(cancel_button_style);

            let tabs = row![internal_tab, external_tab].spacing(4);

            let search_input = text_input(
                t(locale, "modal.postInsertLink.searchPlaceholder").as_str(),
                &search_query,
            )
            .size(13)
            .on_input(|s| Message::PostEditor(PostEditorMsg::PostInsertLinkSearch(s)))
            .padding([6, 10])
            .width(Length::Fill);

            let internal_content: Element<'static, Message> = if results.is_empty() {
                column![
                    search_input,
                    Space::with_height(12.0),
                    text(t(locale, "modal.postInsertLink.emptyState"))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.55, 0.55, 0.60)),
                ]
                .spacing(4)
                .into()
            } else {
                let mut column = column![search_input, Space::with_height(12.0)];
                for link in results {
                    column = column.push(
                        button(
                            row![
                                column![
                                    text(link.title.clone())
                                        .size(13)
                                        .shaping(Shaping::Advanced)
                                        .color(Color::WHITE),
                                    text(link.canonical_url.clone())
                                        .size(11)
                                        .shaping(Shaping::Advanced)
                                        .color(Color::from_rgb(0.58, 0.58, 0.64)),
                                ]
                                .spacing(2)
                                .width(Length::Fill),
                                container(
                                    text(link.status.clone())
                                        .size(10)
                                        .shaping(Shaping::Advanced)
                                        .color(match link.status.as_str() {
                                            "published" => Color::from_rgb(0.52, 0.82, 0.60),
                                            "archived" => Color::from_rgb(0.70, 0.70, 0.74),
                                            _ => Color::from_rgb(0.90, 0.78, 0.35),
                                        }),
                                )
                                .padding([3, 8])
                                .style(|_: &Theme| {
                                    container::Style {
                                        background: Some(Background::Color(Color::from_rgb(
                                            0.18, 0.20, 0.25,
                                        ))),
                                        border: Border {
                                            color: Color::from_rgb(0.30, 0.30, 0.35),
                                            width: 1.0,
                                            radius: 999.0.into(),
                                        },
                                        ..container::Style::default()
                                    }
                                }),
                            ]
                            .spacing(12)
                            .align_y(Alignment::Center),
                        )
                        .on_press(Message::PostEditor(PostEditorMsg::PostInsertLinkSelected(
                            link.post_id.clone(),
                        )))
                        .padding([8, 12])
                        .style(|_: &Theme, _| button::Style {
                            background: Some(Background::Color(Color::from_rgb(0.22, 0.24, 0.30))),
                            text_color: Color::from_rgb(0.85, 0.85, 0.90),
                            border: Border {
                                color: Color::from_rgb(0.30, 0.30, 0.35),
                                width: 1.0,
                                radius: 4.0.into(),
                            },
                            ..button::Style::default()
                        }),
                    );
                }
                column.spacing(4).into()
            };

            let external_content: Element<'static, Message> = column![
                text_input(
                    t(locale, "modal.postInsertLink.externalUrlPlaceholder").as_str(),
                    &external_url,
                )
                .size(13)
                .on_input(|s| Message::PostEditor(PostEditorMsg::PostInsertLinkUrlChanged(s)))
                .padding([6, 10])
                .width(Length::Fill),
                text_input(
                    t(locale, "modal.postInsertLink.externalTextPlaceholder").as_str(),
                    &external_text,
                )
                .size(13)
                .on_input(|s| Message::PostEditor(PostEditorMsg::PostInsertLinkTextChanged(s)))
                .padding([6, 10])
                .width(Length::Fill),
                Space::with_height(12.0),
                row![
                    Space::with_width(Length::Fill),
                    button(text(t(locale, "modal.postInsertLink.insert")))
                        .on_press(Message::PostEditor(
                            PostEditorMsg::PostInsertLinkExternalInsert
                        ))
                        .padding([6, 16])
                        .style(confirm_button_style),
                ]
            ]
            .spacing(8)
            .into();

            let create_post_btn: Element<'static, Message> = button(
                text(t(locale, "modal.postInsertLink.createPost"))
                    .size(12)
                    .shaping(Shaping::Advanced)
                    .color(Color::from_rgb(0.60, 0.60, 0.65)),
            )
            .on_press(Message::PostEditor(PostEditorMsg::PostInsertLinkCreate))
            .padding([6, 12])
            .style(|_: &Theme, _| button::Style {
                background: Some(Background::Color(Color::from_rgb(0.18, 0.20, 0.25))),
                text_color: Color::from_rgb(0.60, 0.60, 0.65),
                border: Border {
                    color: Color::from_rgb(0.30, 0.30, 0.35),
                    width: 1.0,
                    radius: 4.0.into(),
                },
                ..button::Style::default()
            })
            .into();

            let cancel_text = text(t(locale, "modal.postInsertLink.cancel"))
                .size(13)
                .shaping(Shaping::Advanced);

            let trailing_button: Element<'static, Message> =
                if active_tab == PostInsertLinkTab::Internal {
                    create_post_btn
                } else {
                    Space::with_width(0.0).into()
                };

            let buttons = row![
                button(cancel_text)
                    .on_press(Message::DismissModal)
                    .padding([6, 16])
                    .style(cancel_button_style),
                Space::with_width(Length::Fill),
                trailing_button,
            ]
            .spacing(8);

            let content = column![
                tabs,
                Space::with_height(12.0),
                if active_tab == PostInsertLinkTab::Internal {
                    internal_content
                } else {
                    external_content
                },
                Space::with_height(16.0),
                buttons,
            ]
            .spacing(4);

            container(content.padding(20))
                .width(Length::Fixed(480.0))
                .style(modal_box_style)
                .into()
        }

        ModalState::InsertMedia {
            post_id: _post_id,
            title: _title,
            media_list,
            search_query,
            link_only: _,
        } => {
            let title = text(t(locale, "modal.insertMedia.title"))
                .size(16)
                .shaping(Shaping::Advanced)
                .color(Color::WHITE);

            let search_input = text_input(
                t(locale, "modal.insertMedia.searchPlaceholder").as_str(),
                &search_query,
            )
            .size(13)
            .on_input(|s| Message::PostEditor(PostEditorMsg::PostInsertMediaSearch(s)))
            .padding([6, 10])
            .width(Length::Fill);

            let media_items: Vec<Element<'static, Message>> = media_list
                .iter()
                .map(|m| {
                    let is_image = m.mime_type.starts_with("image/");
                    let title = m.title.as_deref().unwrap_or("").to_string();
                    let original_name = m.original_name.clone();
                    let thumb: Element<'static, Message> =
                        if let Some(path) = resolve_thumbnail_file(data_dir, m) {
                            image(path)
                                .width(Length::Fixed(120.0))
                                .height(Length::Fixed(90.0))
                                .into()
                        } else if is_image {
                            text("\u{1F5BC}")
                                .size(32)
                                .shaping(Shaping::Advanced)
                                .color(Color::from_rgb(0.40, 0.40, 0.45))
                                .into()
                        } else {
                            text("\u{1F4C4}")
                                .size(32)
                                .shaping(Shaping::Advanced)
                                .color(Color::from_rgb(0.40, 0.40, 0.45))
                                .into()
                        };

                    let media_title = text(title.clone())
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.85, 0.85, 0.90));

                    let media_name = text(original_name)
                        .size(10)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.55, 0.55, 0.60));

                    let media_col = column![
                        container(thumb)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .center_x(Length::Fill)
                            .center_y(Length::Fill)
                            .style(|_| container::Style {
                                background: Some(Background::Color(Color::from_rgb(
                                    0.18, 0.20, 0.25
                                ))),
                                border: Border {
                                    color: Color::from_rgb(0.30, 0.30, 0.35),
                                    width: 1.0,
                                    radius: 6.0.into(),
                                },
                                ..container::Style::default()
                            }),
                        Space::with_height(8.0),
                        media_title,
                        media_name,
                    ]
                    .spacing(4)
                    .align_x(Alignment::Center);

                    let btn = button(media_col)
                        .on_press(Message::PostEditor(PostEditorMsg::PostInsertMediaSelected(
                            m.id.clone(),
                        )))
                        .padding(8)
                        .style(|_: &Theme, _| button::Style {
                            background: Some(Background::Color(Color::from_rgb(0.20, 0.22, 0.27))),
                            border: Border {
                                color: Color::from_rgb(0.30, 0.30, 0.35),
                                width: 1.0,
                                radius: 8.0.into(),
                            },
                            ..button::Style::default()
                        });
                    btn.into()
                })
                .collect();

            let mut grid = column![].spacing(12).width(Length::Fill);
            let mut media_items = media_items.into_iter();
            loop {
                let mut grid_row = row![].spacing(12).width(Length::Fill);
                let mut pushed_any = false;
                for item in media_items.by_ref().take(3) {
                    grid_row = grid_row.push(item);
                    pushed_any = true;
                }
                if !pushed_any {
                    break;
                }
                grid = grid.push(grid_row);
            }

            let media_list_col: Element<'static, Message> = if media_list.is_empty() {
                column![
                    search_input,
                    Space::with_height(12.0),
                    text(t(locale, "modal.postInsertLink.emptyState"))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.55, 0.55, 0.60)),
                ]
                .spacing(4)
                .into()
            } else {
                column![
                    search_input,
                    Space::with_height(16.0),
                    container(grid).width(Length::Fill).center_x(Length::Fill),
                ]
                .spacing(8)
                .width(Length::Fill)
                .into()
            };

            let cancel_text = text(t(locale, "modal.insertMedia.cancel"))
                .size(13)
                .shaping(Shaping::Advanced);

            let buttons = row![
                button(cancel_text)
                    .on_press(Message::DismissModal)
                    .padding([6, 16])
                    .style(cancel_button_style),
            ];

            let content = column![
                title,
                Space::with_height(12.0),
                media_list_col,
                Space::with_height(16.0),
                buttons,
            ]
            .spacing(4)
            .width(Length::Fill);

            container(content.padding(20))
                .width(Length::Fixed(580.0))
                .height(Length::Fixed(480.0))
                .style(modal_box_style)
                .into()
        }

        ModalState::PostGallery {
            post_id: _post_id,
            title: _title,
            media_list,
            selected_index,
        } => {
            let image_media: Vec<Media> = media_list
                .iter()
                .filter(|m| m.mime_type.starts_with("image/"))
                .cloned()
                .collect();

            let media_items: Vec<Element<'static, Message>> = image_media
                .iter()
                .enumerate()
                .map(|(index, m)| {
                    let title = m.title.as_deref().unwrap_or("").to_string();
                    let thumb: Element<'static, Message> =
                        if let Some(path) = resolve_thumbnail_file(data_dir, m) {
                            image(path)
                                .width(Length::Fixed(180.0))
                                .height(Length::Fixed(120.0))
                                .into()
                        } else {
                            text("\u{1F5BC}")
                                .size(32)
                                .shaping(Shaping::Advanced)
                                .color(Color::from_rgb(0.40, 0.40, 0.45))
                                .into()
                        };

                    let media_title = text(title)
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.85, 0.85, 0.90));

                    let media_col = column![
                        container(thumb)
                            .width(Length::Fill)
                            .height(Length::Fill)
                            .center_x(Length::Fill)
                            .center_y(Length::Fill)
                            .style(|_| container::Style {
                                background: Some(Background::Color(Color::from_rgb(
                                    0.18, 0.20, 0.25
                                ))),
                                border: Border {
                                    color: Color::from_rgb(0.30, 0.30, 0.35),
                                    width: 1.0,
                                    radius: 6.0.into(),
                                },
                                ..container::Style::default()
                            }),
                        Space::with_height(8.0),
                        media_title,
                    ]
                    .spacing(4)
                    .align_x(Alignment::Center);

                    let btn = button(media_col)
                        .on_press(Message::PostEditor(
                            PostEditorMsg::PostGalleryImageSelected(index),
                        ))
                        .padding(8)
                        .style(|_: &Theme, _| button::Style {
                            background: Some(Background::Color(Color::from_rgb(0.20, 0.22, 0.27))),
                            border: Border {
                                color: Color::from_rgb(0.30, 0.30, 0.35),
                                width: 1.0,
                                radius: 8.0.into(),
                            },
                            ..button::Style::default()
                        });
                    btn.into()
                })
                .collect();

            let mut grid = column![].spacing(12).width(Length::Fill);
            let mut media_items = media_items.into_iter();
            loop {
                let mut grid_row = row![].spacing(12).width(Length::Fill);
                let mut pushed_any = false;
                for item in media_items.by_ref().take(3) {
                    grid_row = grid_row.push(item);
                    pushed_any = true;
                }
                if !pushed_any {
                    break;
                }
                grid = grid.push(grid_row);
            }

            let close_text = text(t(locale, "modal.postGallery.close"))
                .size(13)
                .shaping(Shaping::Advanced);

            let close_button = button(close_text)
                .on_press(Message::DismissModal)
                .padding([6, 16])
                .style(cancel_button_style);

            let lightbox: Element<'static, Message> = if let Some(index) = selected_index {
                if let Some(selected) = image_media.get(index) {
                    let preview: Element<'static, Message> =
                        if let Some(path) = resolve_media_file(data_dir, selected) {
                            image(path)
                                .width(Length::Fill)
                                .height(Length::Fixed(420.0))
                                .into()
                        } else {
                            text(t(locale, "modal.postGallery.unavailable"))
                                .size(13)
                                .shaping(Shaping::Advanced)
                                .into()
                        };
                    column![
                        container(preview)
                            .width(Length::Fill)
                            .center_x(Length::Fill),
                        row![
                            button(text("<"))
                                .on_press(Message::PostEditor(PostEditorMsg::PostGalleryPrevious))
                                .padding([6, 12])
                                .style(cancel_button_style),
                            Space::with_width(Length::Fill),
                            text(format!("{} / {}", index + 1, image_media.len()))
                                .size(12)
                                .shaping(Shaping::Advanced),
                            Space::with_width(Length::Fill),
                            button(text(">"))
                                .on_press(Message::PostEditor(PostEditorMsg::PostGalleryNext))
                                .padding([6, 12])
                                .style(cancel_button_style),
                            Space::with_width(12.0),
                            button(text(t(locale, "modal.postGallery.backToGrid")))
                                .on_press(Message::PostEditor(
                                    PostEditorMsg::PostGalleryCloseLightbox
                                ))
                                .padding([6, 12])
                                .style(cancel_button_style),
                        ]
                        .align_y(Alignment::Center)
                    ]
                    .spacing(12)
                    .into()
                } else {
                    Space::with_height(0.0).into()
                }
            } else {
                Space::with_height(0.0).into()
            };

            let content = column![
                if selected_index.is_some() {
                    lightbox
                } else {
                    container(grid).center_x(Length::Fill).into()
                },
                Space::with_height(16.0),
                close_button,
            ]
            .spacing(4);

            container(content.padding(20))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(modal_box_style)
                .into()
        }

        ModalState::AISuggestions { target, fields } => {
            let title = text(t(locale, "modal.aiSuggestions.title"))
                .size(16)
                .shaping(Shaping::Advanced)
                .color(Color::WHITE);

            let rows = fields
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let toggle =
                        checkbox(field.label.clone(), field.accepted)
                            .on_toggle_maybe((!field.locked).then_some(move |value| {
                                Message::ToggleAiSuggestionField(index, value)
                            }))
                            .size(16)
                            .text_size(13);
                    container(
                        column![
                            toggle,
                            row![
                                container(
                                    text(field.current_value.clone())
                                        .size(12)
                                        .color(Color::from_rgb(0.58, 0.58, 0.64))
                                )
                                .width(Length::FillPortion(1)),
                                text("→").size(14).color(Color::from_rgb(0.62, 0.66, 0.74)),
                                container(
                                    text(field.suggested_value.clone())
                                        .size(12)
                                        .color(Color::WHITE)
                                )
                                .width(Length::FillPortion(1)),
                            ]
                            .spacing(10)
                            .align_y(Alignment::Center),
                        ]
                        .spacing(8),
                    )
                    .padding(10)
                    .style(|_: &Theme| container::Style {
                        background: Some(Background::Color(Color::from_rgb(0.18, 0.20, 0.25))),
                        border: Border {
                            color: Color::from_rgb(0.30, 0.30, 0.35),
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..container::Style::default()
                    })
                    .into()
                })
                .collect::<Vec<Element<'static, Message>>>();

            let buttons = row![
                button(text(t(locale, "common.cancel")).size(13))
                    .on_press(Message::DismissModal)
                    .padding([6, 16])
                    .style(cancel_button_style),
                Space::with_width(Length::Fill),
                button(text(t(locale, "modal.aiSuggestions.applySelected")).size(13))
                    .on_press(Message::ApplyAiSuggestions(target, fields))
                    .padding([6, 16])
                    .style(confirm_button_style),
            ];

            let content = column![
                title,
                Space::with_height(12.0),
                scrollable(column(rows).spacing(8)).height(Length::Fixed(320.0)),
                Space::with_height(16.0),
                buttons,
            ]
            .spacing(4);

            container(content.padding(20))
                .width(Length::Fixed(640.0))
                .style(modal_box_style)
                .into()
        }

        ModalState::LanguagePicker {
            target,
            source_language,
            available_targets,
        } => {
            let title = text(t(locale, "modal.languagePicker.title"))
                .size(16)
                .shaping(Shaping::Advanced)
                .color(Color::WHITE);
            let subtitle = text(format!(
                "{}: {}",
                t(locale, "editor.language"),
                source_language
            ))
            .size(12)
            .shaping(Shaping::Advanced)
            .color(Color::from_rgb(0.60, 0.60, 0.68));

            let rows = if available_targets.is_empty() {
                vec![text(t(locale, "common.noResults")).size(12).into()]
            } else {
                available_targets
                    .into_iter()
                    .map(|language| {
                        let message = match &target {
                            AiEntityTarget::Post(_) => Message::PostEditor(
                                PostEditorMsg::TranslateTo(language.code.clone()),
                            ),
                            AiEntityTarget::Media(_) => Message::MediaEditor(
                                crate::views::media_editor::MediaEditorMsg::TranslateTo(
                                    language.code.clone(),
                                ),
                            ),
                        };
                        let status = language.existing_status.clone().unwrap_or_default();
                        button(
                            row![
                                text(format!("{} {}", language.flag_emoji, language.name))
                                    .size(13)
                                    .color(Color::WHITE),
                                Space::with_width(Length::Fill),
                                text(status)
                                    .size(11)
                                    .color(Color::from_rgb(0.60, 0.60, 0.68)),
                            ]
                            .align_y(Alignment::Center),
                        )
                        .on_press(message)
                        .padding([8, 12])
                        .style(cancel_button_style)
                        .into()
                    })
                    .collect::<Vec<Element<'static, Message>>>()
            };

            let content = column![
                title,
                subtitle,
                Space::with_height(12.0),
                column(rows).spacing(6),
                Space::with_height(16.0),
                button(text(t(locale, "common.cancel")).size(13))
                    .on_press(Message::DismissModal)
                    .padding([6, 16])
                    .style(cancel_button_style),
            ]
            .spacing(4);

            container(content.padding(20))
                .width(Length::Fixed(420.0))
                .style(modal_box_style)
                .into()
        }
    };

    // Center the modal in a full-screen backdrop
    container(
        container(modal_content)
            .center_x(Length::Fill)
            .center_y(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(backdrop_style)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_link_markdown_requires_url() {
        assert_eq!(external_link_markdown("", "text"), None);
        assert_eq!(
            external_link_markdown("https://example.com", ""),
            Some("https://example.com".to_string())
        );
        assert_eq!(
            external_link_markdown("https://example.com", "Example"),
            Some("[Example](https://example.com)".to_string())
        );
    }

    #[test]
    fn gallery_step_wraps_in_both_directions() {
        assert_eq!(gallery_step(0, 3, -1), 2);
        assert_eq!(gallery_step(2, 3, 1), 0);
        assert_eq!(gallery_step(1, 3, 1), 2);
    }
}
