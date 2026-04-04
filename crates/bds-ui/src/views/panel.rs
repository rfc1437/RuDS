use iced::widget::{button, column, container, row, scrollable, text, Space};
use iced::{Alignment, Element, Length};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::t;
use crate::state::navigation::{OutputEntry, PanelTab, TaskSnapshot};

pub fn view(
    panel_tab: PanelTab,
    task_snapshots: &[TaskSnapshot],
    output_entries: &[OutputEntry],
    locale: UiLocale,
) -> Element<'static, Message> {
    // Tab header
    let tasks_btn = {
        let mut btn = button(text(t(locale, "common.tasks")).size(12))
            .on_press(Message::SetPanelTab(PanelTab::Tasks))
            .padding([4, 8]);
        if panel_tab == PanelTab::Tasks {
            btn = btn.style(button::primary);
        } else {
            btn = btn.style(button::secondary);
        }
        btn
    };

    let output_btn = {
        let mut btn = button(text(t(locale, "panel.output")).size(12))
            .on_press(Message::SetPanelTab(PanelTab::Output))
            .padding([4, 8]);
        if panel_tab == PanelTab::Output {
            btn = btn.style(button::primary);
        } else {
            btn = btn.style(button::secondary);
        }
        btn
    };

    let close_btn = button(text("×").size(12))
        .on_press(Message::TogglePanel)
        .padding([4, 6])
        .style(button::text);

    let tab_header = row![
        tasks_btn,
        output_btn,
        Space::with_width(Length::Fill),
        close_btn,
    ]
    .spacing(4)
    .align_y(Alignment::Center);

    // Tab content
    let content: Element<'static, Message> = match panel_tab {
        PanelTab::Tasks => {
            if task_snapshots.is_empty() {
                container(text(t(locale, "tasks.noActive")).size(12))
                    .padding(8)
                    .into()
            } else {
                let items: Vec<Element<'static, Message>> = task_snapshots
                    .iter()
                    .map(|snap| {
                        let status_text = format!(
                            "{} — {}{}",
                            snap.label,
                            snap.status,
                            snap.progress
                                .map(|p| format!(" ({:.0}%)", p * 100.0))
                                .unwrap_or_default(),
                        );
                        text(status_text).size(11).into()
                    })
                    .collect();
                scrollable(
                    iced::widget::Column::with_children(items)
                        .spacing(4)
                        .padding(8),
                )
                .into()
            }
        }
        PanelTab::Output => {
            if output_entries.is_empty() {
                container(text(t(locale, "panel.noOutput")).size(12))
                    .padding(8)
                    .into()
            } else {
                let items: Vec<Element<'static, Message>> = output_entries
                    .iter()
                    .map(|entry| text(entry.text.clone()).size(11).into())
                    .collect();
                scrollable(
                    iced::widget::Column::with_children(items)
                        .spacing(2)
                        .padding(8),
                )
                .into()
            }
        }
    };

    container(
        column![tab_header, content].spacing(4),
    )
    .width(Length::Fill)
    .height(Length::Fixed(200.0))
    .into()
}
