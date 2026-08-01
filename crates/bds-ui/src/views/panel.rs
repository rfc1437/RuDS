use iced::widget::text::Shaping;
use iced::widget::{Space, button, column, container, progress_bar, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Theme};

use bds_core::engine::git::GitCommit;
use bds_core::engine::task::TaskStatus;
use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::{t, tw};
use crate::state::navigation::{OutputEntry, PanelTab, TaskSnapshot};
use crate::state::tabs::{Tab, TabType};
use crate::views::post_editor::ResolvedPostLink;
use std::collections::HashSet;

fn task_row(
    snapshot: &TaskSnapshot,
    locale: UiLocale,
    is_group_child: bool,
) -> Element<'static, Message> {
    let status = match &snapshot.status {
        TaskStatus::Pending => t(locale, "tasks.statusPending"),
        TaskStatus::Running if snapshot.cancellation_requested => {
            t(locale, "tasks.statusCancelling")
        }
        TaskStatus::Running => t(locale, "tasks.statusRunning"),
        TaskStatus::Completed => t(locale, "tasks.statusCompleted"),
        TaskStatus::Failed(error) => tw(locale, "tasks.statusFailed", &[("error", error)]),
        TaskStatus::Cancelled => t(locale, "tasks.statusCancelled"),
    };

    let header = row![
        text(snapshot.label.clone())
            .size(11)
            .shaping(Shaping::Advanced)
            .font(Font {
                weight: iced::font::Weight::Semibold,
                ..Font::DEFAULT
            }),
        Space::with_width(Length::Fill),
        text(status)
            .size(10)
            .shaping(Shaping::Advanced)
            .color(task_status_color(&snapshot.status)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let mut rows: Vec<Element<'static, Message>> = vec![header.into()];
    if let Some(message) = snapshot
        .message
        .as_deref()
        .filter(|message| !message.is_empty())
    {
        rows.push(
            text(message.to_string())
                .size(11)
                .shaping(Shaping::Advanced)
                .color(Color::from_rgb8(0x9D, 0xA5, 0xB4))
                .into(),
        );
    }
    if let Some(progress) = snapshot.progress {
        rows.push(
            row![
                progress_bar(0.0..=1.0, progress.clamp(0.0, 1.0))
                    .height(Length::Fixed(6.0))
                    .style(task_progress_style),
                text(format!("{:.0}%", progress * 100.0))
                    .size(10)
                    .color(Color::from_rgb8(0x9D, 0xA5, 0xB4)),
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        );
    }
    if snapshot.is_cancellable {
        rows.push(
            row![
                Space::with_width(Length::Fill),
                button(text(t(locale, "tasks.cancelTask")).size(10))
                    .on_press(Message::CancelTask(snapshot.source, snapshot.id))
                    .padding([3, 8])
                    .style(inputs::secondary_button),
            ]
            .into(),
        );
    }

    let card: Element<'static, Message> = container(
        iced::widget::Column::with_children(rows)
            .spacing(6)
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding(8)
    .style(task_entry_style)
    .into();

    if is_group_child {
        row![Space::with_width(Length::Fixed(16.0)), card]
            .width(Length::Fill)
            .into()
    } else {
        card
    }
}

fn task_status_color(status: &TaskStatus) -> Color {
    match status {
        TaskStatus::Running => Color::from_rgb8(0x73, 0xC9, 0x91),
        TaskStatus::Pending => Color::from_rgb8(0xCC, 0xA7, 0x00),
        TaskStatus::Failed(_) => Color::from_rgb8(0xF4, 0x87, 0x71),
        TaskStatus::Completed | TaskStatus::Cancelled => Color::from_rgb8(0x9D, 0xA5, 0xB4),
    }
}

fn task_entry_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb8(0x25, 0x25, 0x26))),
        border: Border {
            color: Color::from_rgb8(0x3C, 0x3C, 0x3C),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    }
}

fn task_progress_style(_theme: &Theme) -> progress_bar::Style {
    progress_bar::Style {
        background: Background::Color(Color::from_rgb8(0x3C, 0x3C, 0x3C)),
        bar: Background::Color(Color::from_rgb8(0x0E, 0x63, 0x9C)),
        border: Border {
            radius: 3.0.into(),
            ..Border::default()
        },
    }
}

fn task_group_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered | button::Status::Pressed => Color::from_rgb8(0x2D, 0x2D, 0x30),
        _ => Color::from_rgb8(0x25, 0x25, 0x26),
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: Color::from_rgb8(0xCC, 0xCC, 0xCC),
        border: Border {
            color: Color::from_rgb8(0x3C, 0x3C, 0x3C),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

fn group_progress(members: &[&TaskSnapshot]) -> Option<f32> {
    (!members.is_empty()).then(|| {
        members
            .iter()
            .map(|member| match member.status {
                TaskStatus::Completed | TaskStatus::Failed(_) | TaskStatus::Cancelled => 1.0,
                TaskStatus::Pending | TaskStatus::Running => member.progress.unwrap_or(0.0),
            })
            .sum::<f32>()
            / members.len() as f32
    })
}

fn task_group_meta(members: &[&TaskSnapshot], locale: UiLocale) -> String {
    let mut parts = vec![format!(
        "{:.0}%",
        group_progress(members).unwrap_or_default() * 100.0
    )];
    let running = members
        .iter()
        .filter(|member| matches!(member.status, TaskStatus::Running))
        .count();
    let pending = members
        .iter()
        .filter(|member| matches!(member.status, TaskStatus::Pending))
        .count();
    if running > 0 {
        parts.push(format!("{running} {}", t(locale, "tasks.statusRunning")));
    }
    if pending > 0 {
        parts.push(format!("{pending} {}", t(locale, "tasks.statusPending")));
    }
    parts.join(" · ")
}

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
            radius: 6.0.into(),
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
            radius: 6.0.into(),
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

#[expect(
    clippy::too_many_arguments,
    reason = "arguments are independent panel state slices"
)]
pub fn view(
    panel_tab: PanelTab,
    task_snapshots: &[TaskSnapshot],
    collapsed_task_groups: &HashSet<String>,
    output_entries: &[OutputEntry],
    post_outlinks: &[ResolvedPostLink],
    post_backlinks: &[ResolvedPostLink],
    locale: UiLocale,
    active_tab_is_post: bool,
    active_tab_is_post_or_media: bool,
    git_file_history: &[GitCommit],
) -> Element<'static, Message> {
    let muted = Color::from_rgb(0.50, 0.50, 0.55);

    // Tab header — per layout.allium: tasks, output, post_links (only when
    // active editor tab is a post), git_log (only when active tab is post or
    // media).
    let tasks_btn = button(
        text(t(locale, "common.tasks"))
            .size(12)
            .shaping(Shaping::Advanced),
    )
    .on_press(Message::SetPanelTab(PanelTab::Tasks))
    .padding([4, 8])
    .style(if panel_tab == PanelTab::Tasks {
        tab_active
    } else {
        tab_inactive
    });

    let output_btn = button(
        text(t(locale, "panel.output"))
            .size(12)
            .shaping(Shaping::Advanced),
    )
    .on_press(Message::SetPanelTab(PanelTab::Output))
    .padding([4, 8])
    .style(if panel_tab == PanelTab::Output {
        tab_active
    } else {
        tab_inactive
    });

    let close_btn = button(text("\u{2715}").size(12).shaping(Shaping::Advanced))
        .on_press(Message::TogglePanel)
        .padding([4, 6])
        .style(close_btn_style);

    let mut tab_row: Vec<Element<'static, Message>> = vec![tasks_btn.into(), output_btn.into()];

    if active_tab_is_post {
        let post_links_btn = button(
            text(t(locale, "panel.postLinks"))
                .size(12)
                .shaping(Shaping::Advanced),
        )
        .on_press(Message::SetPanelTab(PanelTab::PostLinks))
        .padding([4, 8])
        .style(if panel_tab == PanelTab::PostLinks {
            tab_active
        } else {
            tab_inactive
        });
        tab_row.push(post_links_btn.into());
    }

    if active_tab_is_post_or_media {
        let git_log_btn = button(
            text(t(locale, "panel.gitLog"))
                .size(12)
                .shaping(Shaping::Advanced),
        )
        .on_press(Message::SetPanelTab(PanelTab::GitLog))
        .padding([4, 8])
        .style(if panel_tab == PanelTab::GitLog {
            tab_active
        } else {
            tab_inactive
        });
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
                container(
                    text(t(locale, "tasks.noActive"))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(muted),
                )
                .padding(8)
                .into()
            } else {
                let visible = task_snapshots.iter().collect::<Vec<_>>();
                let mut rendered_groups = HashSet::new();
                let mut items: Vec<Element<'static, Message>> = Vec::new();
                for snapshot in &visible {
                    let Some(group_id) = snapshot.group_id.as_ref() else {
                        items.push(task_row(snapshot, locale, false));
                        continue;
                    };
                    if !rendered_groups.insert(group_id.clone()) {
                        continue;
                    }
                    let members = visible
                        .iter()
                        .copied()
                        .filter(|member| member.group_id.as_ref() == Some(group_id))
                        .collect::<Vec<_>>();
                    let collapsed = collapsed_task_groups.contains(group_id);
                    let group_name = snapshot.group_name.as_deref().unwrap_or(group_id);
                    items.push(
                        button(
                            row![
                                text(if collapsed { "\u{25b8}" } else { "\u{25be}" }).size(11),
                                text(format!("{} ({})", group_name, members.len()))
                                    .size(11)
                                    .font(Font {
                                        weight: iced::font::Weight::Semibold,
                                        ..Font::DEFAULT
                                    }),
                                text(task_group_meta(&members, locale))
                                    .size(10)
                                    .color(Color::from_rgb8(0x9D, 0xA5, 0xB4)),
                            ]
                            .spacing(8)
                            .align_y(Alignment::Center),
                        )
                        .on_press(Message::ToggleTaskGroup(group_id.clone()))
                        .width(Length::Fill)
                        .padding(8)
                        .style(task_group_button_style)
                        .into(),
                    );
                    if !collapsed {
                        items.extend(
                            members
                                .into_iter()
                                .map(|member| task_row(member, locale, true)),
                        );
                    }
                }
                scrollable(
                    iced::widget::Column::with_children(items)
                        .spacing(4)
                        .padding(8),
                )
                .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
                .style(inputs::scrollable_style)
                .into()
            }
        }
        PanelTab::Output => {
            if output_entries.is_empty() {
                container(
                    text(t(locale, "panel.noOutput"))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(muted),
                )
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
                .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
                .style(inputs::scrollable_style)
                .into()
            }
        }
        PanelTab::PostLinks => {
            if post_outlinks.is_empty() && post_backlinks.is_empty() {
                container(
                    text(t(locale, "panel.postLinksPlaceholder"))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(muted),
                )
                .padding(8)
                .into()
            } else {
                let mut items: Vec<Element<'static, Message>> = vec![
                    text(t(locale, "editor.outlinks"))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.75, 0.77, 0.82))
                        .into(),
                ];

                if post_outlinks.is_empty() {
                    items.push(
                        text(t(locale, "panel.postLinksPlaceholder"))
                            .size(11)
                            .color(muted)
                            .into(),
                    );
                } else {
                    for link in post_outlinks {
                        items.push(post_link_button(locale, link));
                    }
                }

                items.push(Space::with_height(8.0).into());
                items.push(
                    text(t(locale, "editor.backlinks"))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.75, 0.77, 0.82))
                        .into(),
                );

                if post_backlinks.is_empty() {
                    items.push(
                        text(t(locale, "panel.postLinksPlaceholder"))
                            .size(11)
                            .color(muted)
                            .into(),
                    );
                } else {
                    for link in post_backlinks {
                        items.push(post_link_button(locale, link));
                    }
                }

                scrollable(
                    iced::widget::Column::with_children(items)
                        .spacing(4)
                        .padding(8),
                )
                .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
                .style(inputs::scrollable_style)
                .into()
            }
        }
        PanelTab::GitLog => {
            if git_file_history.is_empty() {
                container(
                    text(t(locale, "git.noFileHistory"))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(muted),
                )
                .padding(8)
                .into()
            } else {
                let items = git_file_history
                    .iter()
                    .map(|commit| {
                        let hash = commit.hash.clone();
                        let subject = commit.subject.clone().unwrap_or_else(|| hash.clone());
                        let short = hash.chars().take(7).collect::<String>();
                        button(
                            row![
                                text(short).size(11).font(iced::Font::MONOSPACE),
                                text(subject.clone()).size(11),
                                Space::with_width(Length::Fill),
                                text(commit.date.clone().unwrap_or_default()).size(10),
                            ]
                            .spacing(8),
                        )
                        .on_press(Message::OpenGitCommitDiff { hash, subject })
                        .padding([4, 8])
                        .width(Length::Fill)
                        .style(inputs::disclosure_button)
                        .into()
                    })
                    .collect::<Vec<Element<'static, Message>>>();
                scrollable(
                    iced::widget::Column::with_children(items)
                        .spacing(2)
                        .padding(8),
                )
                .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
                .style(inputs::scrollable_style)
                .into()
            }
        }
    };

    container(column![tab_header, content].spacing(0))
        .width(Length::Fill)
        .height(Length::Fixed(200.0))
        .style(panel_style)
        .into()
}

fn post_link_button(locale: UiLocale, link: &ResolvedPostLink) -> Element<'static, Message> {
    button(text(link.title.clone()).size(11).shaping(Shaping::Advanced))
        .on_press(Message::OpenTab(Tab {
            id: link.post_id.clone(),
            title: if link.title.is_empty() {
                t(locale, "editor.untitled")
            } else {
                link.title.clone()
            },
            tab_type: TabType::Post,
            is_transient: false,
            is_dirty: false,
        }))
        .padding([4, 8])
        .style(tab_inactive)
        .into()
}

#[cfg(test)]
mod progress_tests {
    use super::*;

    #[test]
    fn group_progress_includes_pending_tasks_as_zero_like_bds2() {
        let complete = TaskSnapshot {
            id: 1,
            source: crate::state::navigation::TaskSource::Local,
            label: String::new(),
            group_id: None,
            group_name: None,
            status: TaskStatus::Completed,
            progress: Some(1.0),
            message: None,
            cancellation_requested: false,
            is_cancellable: false,
        };
        let pending = TaskSnapshot {
            id: 2,
            status: TaskStatus::Pending,
            progress: None,
            is_cancellable: true,
            ..complete.clone()
        };

        assert_eq!(group_progress(&[&complete, &pending]), Some(0.5));
    }

    #[test]
    fn task_group_header_matches_bds2_progress_and_status_summary() {
        let complete = TaskSnapshot {
            id: 1,
            source: crate::state::navigation::TaskSource::Local,
            label: "Complete".into(),
            group_id: Some("group".into()),
            group_name: Some("Build".into()),
            status: TaskStatus::Completed,
            progress: Some(1.0),
            message: None,
            cancellation_requested: false,
            is_cancellable: false,
        };
        let running = TaskSnapshot {
            id: 2,
            label: "Running".into(),
            status: TaskStatus::Running,
            progress: Some(0.25),
            is_cancellable: true,
            ..complete.clone()
        };
        let pending = TaskSnapshot {
            id: 3,
            label: "Pending".into(),
            status: TaskStatus::Pending,
            progress: None,
            is_cancellable: true,
            ..complete.clone()
        };

        assert_eq!(
            task_group_meta(&[&complete, &running, &pending], UiLocale::En),
            "42% · 1 Running · 1 Pending"
        );
    }

    #[test]
    fn task_cards_use_bds2_surface_and_status_palette() {
        let style = task_entry_style(&Theme::Dark);
        assert_eq!(
            style.background,
            Some(Background::Color(Color::from_rgb8(0x25, 0x25, 0x26)))
        );
        assert_eq!(style.border.color, Color::from_rgb8(0x3C, 0x3C, 0x3C));
        assert_eq!(style.border.width, 1.0);
        let group_style = task_group_button_style(&Theme::Dark, button::Status::Active);
        assert_eq!(group_style.background, style.background);
        assert_eq!(group_style.border, style.border);
        assert_eq!(
            task_status_color(&TaskStatus::Running),
            Color::from_rgb8(0x73, 0xC9, 0x91)
        );
        assert_eq!(
            task_status_color(&TaskStatus::Pending),
            Color::from_rgb8(0xCC, 0xA7, 0x00)
        );
    }
}
