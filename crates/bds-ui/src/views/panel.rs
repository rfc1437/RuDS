use iced::widget::{button, column, container, scrollable, text, Space};
use iced::widget::text::Shaping;
use iced::{Alignment, Background, Border, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::t;
use crate::state::navigation::{OutputEntry, PanelTab, TaskSnapshot};

/// Panel background style.
fn panel_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.13, 0.13, 0.16))),
        ..container::Style::default()
    }
}

/// Panel tab button — active.
fn tab_active(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(Color::from_rgb(0.20, 0.20, 0.25))),
        text_color: Color::WHITE,
        border: Border {
            color: Color::from_rgb(0.30, 0.55, 0.90),
            width: 0.0,
            radius: 3.0.into(),
        },
        ..button::Style::default()
    }
}

/// Panel tab button — inactive.
fn tab_inactive(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.18, 0.18, 0.22),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.60, 0.60, 0.65),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 3.0.into(),
        },
        ..button::Style::default()
    }
}

/// Close button style.
fn close_btn_style(_theme: &Theme, status: button::Status) -> button::Style {
    let color = match status {
        button::Status::Hovered => Color::WHITE,
        _ => Color::from_rgb(0.50, 0.50, 0.55),
    };
    button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: color,
        border: Border::default(),
        ..button::Style::default()
    }
}

pub fn view(
    panel_tab: PanelTab,
    task_snapshots: &[TaskSnapshot],
    output_entries: &[OutputEntry],
    locale: UiLocale,
    active_tab_is_post: bool,
    active_tab_is_post_or_media: bool,
) -> Element<'static, Message> {
    let muted = Color::from_rgb(0.50, 0.50, 0.55);

    // Tab header — per layout.allium: tasks, output, post_links (only when
    // active editor tab is a post), git_log (only when active tab is post or
    // media).
    let tasks_btn = button(text(t(locale, "common.tasks")).size(12).shaping(Shaping::Advanced))
        .on_press(Message::SetPanelTab(PanelTab::Tasks))
        .padding([4, 8])
        .style(if panel_tab == PanelTab::Tasks { tab_active } else { tab_inactive });

    let output_btn = button(text(t(locale, "panel.output")).size(12).shaping(Shaping::Advanced))
        .on_press(Message::SetPanelTab(PanelTab::Output))
        .padding([4, 8])
        .style(if panel_tab == PanelTab::Output { tab_active } else { tab_inactive });

    let close_btn = button(text("\u{2715}").size(12).shaping(Shaping::Advanced))
        .on_press(Message::TogglePanel)
        .padding([4, 6])
        .style(close_btn_style);

    let mut tab_row: Vec<Element<'static, Message>> = vec![
        tasks_btn.into(),
        output_btn.into(),
    ];

    if active_tab_is_post {
        let post_links_btn = button(text(t(locale, "panel.postLinks")).size(12).shaping(Shaping::Advanced))
            .on_press(Message::SetPanelTab(PanelTab::PostLinks))
            .padding([4, 8])
            .style(if panel_tab == PanelTab::PostLinks { tab_active } else { tab_inactive });
        tab_row.push(post_links_btn.into());
    }

    if active_tab_is_post_or_media {
        let git_log_btn = button(text(t(locale, "panel.gitLog")).size(12).shaping(Shaping::Advanced))
            .on_press(Message::SetPanelTab(PanelTab::GitLog))
            .padding([4, 8])
            .style(if panel_tab == PanelTab::GitLog { tab_active } else { tab_inactive });
        tab_row.push(git_log_btn.into());
    }

    tab_row.push(Space::with_width(Length::Fill).into());
    tab_row.push(close_btn.into());

    let tab_header = iced::widget::Row::with_children(tab_row)
        .spacing(4)
        .align_y(Alignment::Center)
        .padding([4, 8]);

    // Tab content
    let content: Element<'static, Message> = match panel_tab {
        PanelTab::Tasks => {
            if task_snapshots.is_empty() {
                container(text(t(locale, "tasks.noActive")).size(12).shaping(Shaping::Advanced).color(muted))
                    .padding(8)
                    .into()
            } else {
                let items: Vec<Element<'static, Message>> = task_snapshots
                    .iter()
                    .map(|snap| {
                        let progress_str = snap.progress
                            .map(|p| format!(" ({:.0}%)", p * 100.0))
                            .unwrap_or_default();
                        let phase_str = snap.message
                            .as_deref()
                            .map(|m| format!(" \u{2014} {m}"))
                            .unwrap_or_default();
                        let status_text = format!(
                            "{} \u{2014} {}{}{}",
                            snap.label,
                            snap.status,
                            progress_str,
                            phase_str,
                        );
                        text(status_text).size(11).shaping(Shaping::Advanced).color(Color::from_rgb(0.70, 0.70, 0.75)).into()
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
                container(text(t(locale, "panel.noOutput")).size(12).shaping(Shaping::Advanced).color(muted))
                    .padding(8)
                    .into()
            } else {
                let items: Vec<Element<'static, Message>> = output_entries
                    .iter()
                    .map(|entry| {
                        text(entry.text.clone())
                            .size(11)
                            .shaping(Shaping::Advanced)
                            .color(Color::from_rgb(0.70, 0.70, 0.75))
                            .into()
                    })
                    .collect();
                scrollable(
                    iced::widget::Column::with_children(items)
                        .spacing(2)
                        .padding(8),
                )
                .into()
            }
        }
        PanelTab::PostLinks => {
            // Post Links content populated in M3 (editor integration)
            container(text(t(locale, "panel.postLinksPlaceholder")).size(12).shaping(Shaping::Advanced).color(muted))
                .padding(8)
                .into()
        }
        PanelTab::GitLog => {
            // Git Log content populated in extension bucket A (git integration)
            container(text(t(locale, "panel.gitLogPlaceholder")).size(12).shaping(Shaping::Advanced).color(muted))
                .padding(8)
                .into()
        }
    };

    container(
        column![tab_header, content].spacing(0),
    )
    .width(Length::Fill)
    .height(Length::Fixed(200.0))
    .style(panel_style)
    .into()
}
