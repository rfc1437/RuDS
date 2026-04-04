use iced::widget::{button, column, container, row, text, Space};
use iced::widget::text::Shaping;
use iced::{Alignment, Background, Border, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::tw;
use crate::state::navigation::TaskSnapshot;

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

/// Status bar background.
fn bar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.12, 0.12, 0.16))),
        ..container::Style::default()
    }
}

/// Airplane button — inactive (dim).
fn airplane_inactive(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.22, 0.22, 0.27),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgba(0.70, 0.70, 0.75, 0.4),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 3.0.into(),
        },
        ..button::Style::default()
    }
}

/// Airplane button — active (warning color).
fn airplane_active(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.85, 0.65, 0.10),
        _ => Color::from_rgb(0.75, 0.55, 0.05),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 3.0.into(),
        },
        ..button::Style::default()
    }
}

/// Dropdown trigger button style.
fn dropdown_trigger(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.22, 0.22, 0.27),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 3.0.into(),
        },
        ..button::Style::default()
    }
}

/// Dropdown item style.
fn dropdown_item(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.25, 0.25, 0.30),
        _ => Color::from_rgb(0.18, 0.18, 0.22),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 2.0.into(),
        },
        ..button::Style::default()
    }
}

/// Dropdown container background.
fn dropdown_bg(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.18, 0.18, 0.22))),
        border: Border {
            color: Color::from_rgb(0.30, 0.30, 0.35),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    }
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

pub fn view(
    active_project_name: Option<&str>,
    post_count: usize,
    media_count: usize,
    locale: UiLocale,
    offline_mode: bool,
    locale_dropdown_open: bool,
    task_snapshots: &[TaskSnapshot],
) -> Element<'static, Message> {
    let label_color = Color::from_rgb(0.60, 0.60, 0.65);

    // ── Left: project name + task indicator ──
    let project_label = active_project_name.unwrap_or("\u{2014}").to_string();
    let project_text = text(project_label).size(12).color(Color::from_rgb(0.80, 0.80, 0.85));

    let running: Vec<&TaskSnapshot> = task_snapshots
        .iter()
        .filter(|t| t.status == "running")
        .collect();
    let task_indicator: Element<'static, Message> = if !running.is_empty() {
        let first = running[0].label.clone();
        if running.len() > 1 {
            text(format!("{first} (+{})", running.len() - 1)).size(11).color(label_color).into()
        } else {
            text(first).size(11).color(label_color).into()
        }
    } else {
        Space::with_width(0).into()
    };

    let left = row![project_text, task_indicator].spacing(12);

    // ── Right side ──

    // Post + media counts
    let posts_label = tw(locale, "statusBar.posts", &[("count", &post_count.to_string())]);
    let media_label = tw(locale, "statusBar.media", &[("count", &media_count.to_string())]);

    // Airplane mode toggle — ✈ icon
    let airplane_btn = button(
        text("\u{2708}").size(13).shaping(Shaping::Advanced),
    )
        .on_press(Message::SetOfflineMode(!offline_mode))
        .padding([2, 4])
        .style(if offline_mode { airplane_active } else { airplane_inactive });

    // Language selector — current flag as trigger
    let trigger_flag = text(locale.flag_emoji())
        .size(14)
        .shaping(Shaping::Advanced);

    let locale_trigger = button(trigger_flag)
        .on_press(Message::ToggleLocaleDropdown)
        .padding([1, 4])
        .style(dropdown_trigger);

    let right = row![
        text(posts_label).size(11).color(label_color),
        text(media_label).size(11).color(label_color),
        airplane_btn,
        locale_trigger,
        text("bDS").size(11).color(Color::from_rgb(0.45, 0.45, 0.50)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    let status_row = container(
        row![
            left,
            Space::with_width(Length::Fill),
            right,
        ]
        .align_y(Alignment::Center)
        .padding([0, 8]),
    )
    .width(Length::Fill)
    .height(Length::Fixed(24.0))
    .style(bar_style);

    // If dropdown is open, stack it above the status bar
    if locale_dropdown_open {
        let items: Vec<Element<'static, Message>> = UiLocale::all()
            .iter()
            .map(|&l| {
                let flag_text = text(l.flag_emoji())
                    .size(16)
                    .shaping(Shaping::Advanced);

                button(flag_text)
                    .on_press(Message::SetUiLocale(l))
                    .padding([4, 8])
                    .width(Length::Fill)
                    .style(dropdown_item)
                    .into()
            })
            .collect();

        let dropdown_menu = container(
            iced::widget::Column::with_children(items).spacing(2).padding(4),
        )
        .style(dropdown_bg);

        // Align dropdown to the right, above the status bar
        let dropdown_row = row![
            Space::with_width(Length::Fill),
            dropdown_menu,
            // offset from right edge to roughly align with the flag trigger
            Space::with_width(Length::Fixed(40.0)),
        ];

        column![dropdown_row, status_row]
            .width(Length::Fill)
            .into()
    } else {
        status_row.into()
    }
}
