use iced::widget::{button, column, container, row, text, Space};
use iced::widget::text::Shaping;
use iced::{Alignment, Background, Border, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::t;

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
}

/// What action to perform when modal is confirmed.
#[derive(Debug, Clone)]
pub enum ConfirmAction {
    DeleteProject(String),
    DeletePost(String),
    DeleteMedia(String),
    DeleteScript(String),
    DeleteTemplate(String),
    MergeTags { source: String, target: String },
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

/// Render the modal overlay.
pub fn view(state: &ModalState, locale: UiLocale) -> Element<'static, Message> {
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
                row![warning_icon, Space::with_width(8.0), title]
                    .align_y(Alignment::Center),
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
