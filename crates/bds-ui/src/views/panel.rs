use iced::widget::text::Shaping;
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Theme};

use bds_core::engine::git::GitCommit;
use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;
use crate::state::navigation::{OutputEntry, PanelTab, TaskSnapshot};
use crate::state::tabs::{Tab, TabType};
use crate::views::post_editor::ResolvedPostLink;
use std::collections::HashSet;

fn task_row(snapshot: &TaskSnapshot, locale: UiLocale) -> Element<'static, Message> {
    let progress = snapshot
        .progress
        .map(|value| format!(" ({:.0}%)", value * 100.0))
        .unwrap_or_default();
    let phase = snapshot
        .message
        .as_deref()
        .map(|message| format!(" — {message}"))
        .unwrap_or_default();
    let label = text(format!(
        "{} — {}{}{}",
        snapshot.label, snapshot.status, progress, phase
    ))
    .size(11)
    .shaping(Shaping::Advanced)
    .color(Color::from_rgb(0.70, 0.70, 0.75));
    let mut content = row![label].align_y(Alignment::Center).spacing(8);
    if snapshot.is_cancellable {
        content = content.push(Space::with_width(Length::Fill)).push(
            button(text(t(locale, "tasks.cancelTask")).size(11))
                .on_press(Message::CancelTask(snapshot.id))
                .padding([3, 8])
                .style(inputs::secondary_button),
        );
    }
    content.into()
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
                let visible = task_snapshots.iter().rev().take(10).collect::<Vec<_>>();
                let mut rendered_groups = HashSet::new();
                let mut items: Vec<Element<'static, Message>> = Vec::new();
                for snapshot in &visible {
                    let Some(group_id) = snapshot.group_id.as_ref() else {
                        items.push(task_row(snapshot, locale));
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
                    let progress_values = members
                        .iter()
                        .filter_map(|member| member.progress)
                        .collect::<Vec<_>>();
                    let progress = (!progress_values.is_empty()).then(|| {
                        format!(
                            " ({:.0}%)",
                            progress_values.iter().sum::<f32>() / progress_values.len() as f32
                                * 100.0
                        )
                    });
                    let collapsed = collapsed_task_groups.contains(group_id);
                    let group_name = snapshot.group_name.as_deref().unwrap_or(group_id);
                    items.push(
                        button(
                            row![
                                text(if collapsed { "\u{25b8}" } else { "\u{25be}" }).size(11),
                                text(format!("{}{}", group_name, progress.unwrap_or_default()))
                                    .size(11)
                            ]
                            .spacing(6),
                        )
                        .on_press(Message::ToggleTaskGroup(group_id.clone()))
                        .width(Length::Fill)
                        .padding([3, 6])
                        .style(inputs::disclosure_button)
                        .into(),
                    );
                    if !collapsed {
                        items.extend(members.into_iter().map(|member| task_row(member, locale)));
                    }
                }
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
