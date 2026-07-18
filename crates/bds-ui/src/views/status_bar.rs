use iced::widget::text::Shaping;
use iced::widget::{Space, button, container, row, text};
use iced::{Alignment, Background, Border, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::tw;
use crate::state::navigation::TaskSnapshot;
use crate::views::project_selector;

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
pub fn dropdown_trigger(_theme: &Theme, status: button::Status) -> button::Style {
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
pub fn dropdown_item(_theme: &Theme, status: button::Status) -> button::Style {
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
pub fn dropdown_bg(_theme: &Theme) -> container::Style {
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

#[expect(
    clippy::too_many_arguments,
    reason = "arguments are independent status values"
)]
pub fn view(
    active_project_name: Option<&str>,
    post_count: usize,
    media_count: usize,
    locale: UiLocale,
    offline_mode: bool,
    task_snapshots: &[TaskSnapshot],
    theme_badge: &str,
    active_post_status: Option<&str>,
) -> Element<'static, Message> {
    let label_color = Color::from_rgb(0.60, 0.60, 0.65);

    // ── Left: project selector trigger + task indicator ──
    let project_name = active_project_name.unwrap_or("\u{2014}");
    let project_trigger = project_selector::trigger_button(project_name);

    let running: Vec<&TaskSnapshot> = task_snapshots
        .iter()
        .filter(|t| t.status == "running")
        .collect();
    let task_indicator: Element<'static, Message> = if !running.is_empty() {
        let first = &running[0];
        let progress_str = first
            .progress
            .map(|p| format!(" {:.0}%", p * 100.0))
            .unwrap_or_default();
        let phase_str = first.message.as_deref().unwrap_or("");
        let extra = if running.len() > 1 {
            format!(" (+{})", running.len() - 1)
        } else {
            String::new()
        };
        let display = if phase_str.is_empty() {
            format!("{}{progress_str}{extra}", first.label)
        } else {
            format!("{phase_str}{progress_str}{extra}")
        };
        text(display)
            .size(11)
            .shaping(Shaping::Advanced)
            .color(label_color)
            .into()
    } else {
        Space::with_width(0).into()
    };

    let left = row![project_trigger, task_indicator]
        .spacing(12)
        .align_y(Alignment::Center);

    // ── Right side ──

    // Post status indicator dot (per layout.allium StatusBarRight.post_status)
    let post_status_el: Element<'static, Message> = if let Some(status) = active_post_status {
        let (dot, color) = match status {
            "draft" => ("\u{25CB}", Color::from_rgb(0.60, 0.60, 0.65)), // hollow circle
            "published" => ("\u{25CF}", Color::from_rgb(0.30, 0.75, 0.40)), // green filled
            "archived" => ("\u{25A0}", Color::from_rgb(0.60, 0.60, 0.65)), // square
            _ => ("\u{25CF}", Color::from_rgb(0.60, 0.60, 0.65)),
        };
        text(dot)
            .size(11)
            .shaping(Shaping::Advanced)
            .color(color)
            .into()
    } else {
        Space::with_width(0).into()
    };

    // Post + media counts
    let posts_label = tw(
        locale,
        "statusBar.posts",
        &[("count", &post_count.to_string())],
    );
    let media_label = tw(
        locale,
        "statusBar.media",
        &[("count", &media_count.to_string())],
    );

    // Airplane mode toggle — ✈ icon
    let airplane_btn = button(text("\u{2708}").size(13).shaping(Shaping::Advanced))
        .on_press(Message::SetOfflineMode(!offline_mode))
        .padding([2, 4])
        .style(if offline_mode {
            airplane_active
        } else {
            airplane_inactive
        });

    // Language selector — current flag as trigger
    let trigger_flag = text(locale.flag_emoji())
        .size(14)
        .shaping(Shaping::Advanced);

    let locale_trigger = button(trigger_flag)
        .on_press(Message::ToggleLocaleDropdown)
        .padding([1, 4])
        .style(dropdown_trigger);

    let right = row![
        post_status_el,
        text(posts_label)
            .size(11)
            .shaping(Shaping::Advanced)
            .color(label_color),
        text(media_label)
            .size(11)
            .shaping(Shaping::Advanced)
            .color(label_color),
        text(theme_badge.to_string())
            .size(11)
            .shaping(Shaping::Advanced)
            .color(label_color),
        airplane_btn,
        locale_trigger,
        text("bDS")
            .size(11)
            .shaping(Shaping::Advanced)
            .color(Color::from_rgb(0.45, 0.45, 0.50)),
    ]
    .spacing(8)
    .align_y(Alignment::Center);

    container(
        row![left, Space::with_width(Length::Fill), right,]
            .align_y(Alignment::Center)
            .padding([0, 8]),
    )
    .width(Length::Fill)
    .height(Length::Fixed(24.0))
    .style(bar_style)
    .into()
}
