use iced::widget::{button, container, row, text, Space};
use iced::{Element, Length};

use crate::app::Message;
use crate::state::tabs::Tab;

pub fn view(
    tabs: &[Tab],
    active_tab: Option<&str>,
) -> Element<'static, Message> {
    if tabs.is_empty() {
        return Space::with_height(0).into();
    }

    let tab_buttons: Vec<Element<'static, Message>> = tabs
        .iter()
        .map(|tab| {
            let is_active = active_tab == Some(tab.id.as_str());
            let title_text = if tab.is_transient {
                format!("{} (preview)", tab.title)
            } else {
                tab.title.clone()
            };
            let tab_id = tab.id.clone();
            let close_id = tab.id.clone();

            let label = row![
                text(title_text).size(12),
                button(text("×").size(12))
                    .on_press(Message::CloseTab(close_id))
                    .padding(2)
                    .style(button::text),
            ]
            .spacing(4);

            let mut btn = button(label)
                .on_press(Message::SelectTab(tab_id))
                .padding([4, 8]);

            if is_active {
                btn = btn.style(button::primary);
            } else {
                btn = btn.style(button::secondary);
            }

            btn.into()
        })
        .collect();

    container(
        iced::widget::Row::with_children(tab_buttons)
            .spacing(1)
            .height(Length::Fixed(35.0)),
    )
    .width(Length::Fill)
    .height(Length::Fixed(35.0))
    .into()
}
