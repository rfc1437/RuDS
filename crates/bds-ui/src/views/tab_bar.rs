use iced::widget::{button, container, row, text, Space};
use iced::{Background, Border, Color, Element, Length, Theme};

use crate::app::Message;
use crate::state::tabs::Tab;

/// Tab bar background.
fn bar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.14, 0.14, 0.18))),
        border: Border {
            color: Color::from_rgb(0.25, 0.25, 0.30),
            width: 0.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}

/// Active tab style.
fn tab_active(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(Color::from_rgb(0.11, 0.11, 0.14))),
        text_color: Color::WHITE,
        border: Border {
            color: Color::from_rgb(0.30, 0.55, 0.90),
            width: 0.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

/// Inactive tab style.
fn tab_inactive(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.18, 0.18, 0.22),
        _ => Color::from_rgb(0.14, 0.14, 0.18),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.60, 0.60, 0.65),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

/// Close button on tab.
fn close_style(_theme: &Theme, status: button::Status) -> button::Style {
    let color = match status {
        button::Status::Hovered => Color::WHITE,
        _ => Color::from_rgb(0.45, 0.45, 0.50),
    };
    button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: color,
        border: Border::default(),
        ..button::Style::default()
    }
}

pub fn view(
    tabs: &[Tab],
    active_tab: Option<&str>,
) -> Element<'static, Message> {
    if tabs.is_empty() {
        return container(Space::new(0, 0))
            .width(Length::Fill)
            .height(Length::Fixed(35.0))
            .style(bar_style)
            .into();
    }

    let tab_buttons: Vec<Element<'static, Message>> = tabs
        .iter()
        .map(|tab| {
            let is_active = active_tab == Some(tab.id.as_str());
            let title_text = if tab.is_transient {
                format!("{} \u{25CB}", tab.title) // hollow circle for transient
            } else {
                tab.title.clone()
            };
            let tab_id = tab.id.clone();
            let close_id = tab.id.clone();

            let label = row![
                text(title_text).size(12),
                button(text("\u{2715}").size(10))
                    .on_press(Message::CloseTab(close_id))
                    .padding(2)
                    .style(close_style),
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center);

            button(label)
                .on_press(Message::SelectTab(tab_id))
                .padding([6, 12])
                .style(if is_active { tab_active } else { tab_inactive })
                .into()
        })
        .collect();

    container(
        iced::widget::Row::with_children(tab_buttons)
            .spacing(1)
            .height(Length::Fixed(35.0)),
    )
    .width(Length::Fill)
    .height(Length::Fixed(35.0))
    .style(bar_style)
    .into()
}
