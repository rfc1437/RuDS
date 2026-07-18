use iced::widget::text::Shaping;
use iced::widget::{Space, button, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Theme};

use crate::app::Message;
use crate::state::toast::{Toast, ToastLevel};

/// Background color per toast severity.
fn toast_bg(level: ToastLevel) -> Color {
    match level {
        ToastLevel::Info => Color::from_rgb(0.16, 0.22, 0.34),
        ToastLevel::Success => Color::from_rgb(0.12, 0.30, 0.16),
        ToastLevel::Warning => Color::from_rgb(0.38, 0.30, 0.10),
        ToastLevel::Error => Color::from_rgb(0.38, 0.14, 0.14),
    }
}

/// Border color per toast severity.
fn toast_border(level: ToastLevel) -> Color {
    match level {
        ToastLevel::Info => Color::from_rgb(0.25, 0.40, 0.65),
        ToastLevel::Success => Color::from_rgb(0.20, 0.55, 0.25),
        ToastLevel::Warning => Color::from_rgb(0.65, 0.50, 0.15),
        ToastLevel::Error => Color::from_rgb(0.65, 0.20, 0.20),
    }
}

fn toast_style(level: ToastLevel) -> impl Fn(&Theme) -> container::Style {
    move |_theme: &Theme| container::Style {
        background: Some(Background::Color(toast_bg(level))),
        border: Border {
            color: toast_border(level),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    }
}

fn dismiss_btn(_theme: &Theme, status: button::Status) -> button::Style {
    let color = match status {
        button::Status::Hovered => Color::WHITE,
        _ => Color::from_rgb(0.65, 0.65, 0.70),
    };
    button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: color,
        border: Border::default(),
        ..button::Style::default()
    }
}

/// Render the toast stack as an overlay element.
///
/// Returns `None` when no toasts are visible.
pub fn view(toasts: &[Toast]) -> Option<Element<'static, Message>> {
    if toasts.is_empty() {
        return None;
    }

    let items: Vec<Element<'static, Message>> = toasts
        .iter()
        .map(|toast| {
            let level = toast.level;
            let dismiss = button(text("\u{2715}").size(11).shaping(Shaping::Advanced))
                .on_press(Message::DismissToast(toast.id))
                .padding([2, 4])
                .style(dismiss_btn);

            container(
                row![
                    text(toast.message.clone())
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(Color::WHITE),
                    Space::with_width(Length::Fill),
                    dismiss,
                ]
                .spacing(8)
                .align_y(Alignment::Center)
                .padding([6, 12]),
            )
            .width(Length::Fixed(420.0))
            .style(toast_style(level))
            .into()
        })
        .collect();

    Some(
        container(
            container(
                iced::widget::Column::with_children(items)
                    .spacing(4)
                    .align_x(Alignment::Center),
            )
            .width(Length::Fill)
            .align_x(Alignment::Center)
            .padding(Padding {
                top: 8.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0,
            }),
        )
        .width(Length::Fill)
        .height(Length::Shrink)
        .into(),
    )
}
