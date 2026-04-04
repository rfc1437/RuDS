use iced::widget::{column, container, text};
use iced::widget::text::Shaping;
use iced::{Background, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::t;

pub fn view(locale: UiLocale) -> Element<'static, Message> {
    let title = t(locale, "welcome.title");
    let subtitle = t(locale, "welcome.subtitle");

    container(
        column![
            text(title)
                .size(28)
                .shaping(Shaping::Advanced)
                .color(Color::from_rgb(0.85, 0.85, 0.90)),
            text(subtitle)
                .size(14)
                .shaping(Shaping::Advanced)
                .color(Color::from_rgb(0.55, 0.55, 0.60)),
        ]
        .spacing(12)
        .align_x(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(|_theme: &Theme| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.11, 0.11, 0.14))),
        ..container::Style::default()
    })
    .into()
}
