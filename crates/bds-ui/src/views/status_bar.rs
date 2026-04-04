use iced::widget::{button, container, row, text, Space};
use iced::{Alignment, Element, Length};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::{t, tw};
use crate::state::navigation::TaskSnapshot;

pub fn view(
    active_project_name: Option<&str>,
    post_count: usize,
    media_count: usize,
    locale: UiLocale,
    offline_mode: bool,
    task_snapshots: &[TaskSnapshot],
) -> Element<'static, Message> {
    // Left: project name + task indicator
    let project_label = active_project_name.unwrap_or("—").to_string();
    let project_text = text(project_label).size(12);

    let running: Vec<&TaskSnapshot> = task_snapshots
        .iter()
        .filter(|t| t.status == "running")
        .collect();
    let task_indicator: Element<'static, Message> = if !running.is_empty() {
        let first = running[0].label.clone();
        if running.len() > 1 {
            text(format!("{first} (+{})", running.len() - 1)).size(11).into()
        } else {
            text(first).size(11).into()
        }
    } else {
        Space::with_width(0).into()
    };

    let left = row![project_text, task_indicator].spacing(12);

    // Right: post count, media count, airplane mode, locale selector, brand
    let posts_label = tw(locale, "statusBar.posts", &[("count", &post_count.to_string())]);
    let media_label = tw(locale, "statusBar.media", &[("count", &media_count.to_string())]);

    let airplane_label = if offline_mode {
        t(locale, "statusBar.offlineModeActive")
    } else {
        t(locale, "statusBar.offlineMode")
    };

    let airplane_btn = button(text(airplane_label).size(11))
        .on_press(Message::SetOfflineMode(!offline_mode))
        .padding([2, 6])
        .style(if offline_mode { button::primary } else { button::text });

    let ui_label = t(locale, "statusBar.ui");

    let locale_buttons: Vec<Element<'static, Message>> = UiLocale::all()
        .iter()
        .map(|l| {
            let code = l.code().to_string();
            let mut btn = button(text(code).size(11))
                .on_press(Message::SetUiLocale(*l))
                .padding([2, 4]);
            if *l == locale {
                btn = btn.style(button::primary);
            } else {
                btn = btn.style(button::text);
            }
            btn.into()
        })
        .collect();

    let locale_row = iced::widget::Row::with_children(locale_buttons).spacing(2);

    let right = row![
        text(posts_label).size(11),
        text(media_label).size(11),
        airplane_btn,
        text(ui_label).size(11),
        locale_row,
        text("bDS").size(11),
    ]
    .spacing(12)
    .align_y(Alignment::Center);

    container(
        row![
            left,
            Space::with_width(Length::Fill),
            right,
        ]
        .align_y(Alignment::Center)
        .padding([0, 8]),
    )
    .width(Length::Fill)
    .height(Length::Fixed(22.0))
    .into()
}
