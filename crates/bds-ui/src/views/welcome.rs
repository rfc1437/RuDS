use iced::widget::{column, container, text};
use iced::{Element, Length};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::t;

pub fn view(locale: UiLocale) -> Element<'static, Message> {
    let title = t(locale, "welcome.title");
    let subtitle = t(locale, "welcome.subtitle");

    container(
        column![
            text(title).size(28),
            text(subtitle).size(14),
        ]
        .spacing(12)
        .align_x(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}
