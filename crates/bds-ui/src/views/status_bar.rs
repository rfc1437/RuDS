use iced::widget::{button, container, pick_list, row, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::{t, tw};
use crate::state::navigation::TaskSnapshot;

// ---------------------------------------------------------------------------
// Locale option wrapper for pick_list (needs ToString + PartialEq + Clone)
// ---------------------------------------------------------------------------

/// Wraps a UiLocale with a display label for the pick_list dropdown.
#[derive(Debug, Clone, PartialEq)]
struct LocaleOption {
    locale: UiLocale,
    label: String,
}

impl ToString for LocaleOption {
    fn to_string(&self) -> String {
        self.label.clone()
    }
}

impl LocaleOption {
    fn new(locale: UiLocale, display_locale: UiLocale) -> Self {
        let key = match locale {
            UiLocale::En => "language.en",
            UiLocale::De => "language.de",
            UiLocale::Fr => "language.fr",
            UiLocale::It => "language.it",
            UiLocale::Es => "language.es",
        };
        Self {
            locale,
            label: t(display_locale, key),
        }
    }
}

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

/// Pick list style matching status bar.
fn locale_pick_list_style(_theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let bg = match status {
        pick_list::Status::Hovered => Color::from_rgb(0.22, 0.22, 0.27),
        _ => Color::TRANSPARENT,
    };
    pick_list::Style {
        text_color: Color::from_rgb(0.70, 0.70, 0.75),
        placeholder_color: Color::from_rgb(0.50, 0.50, 0.55),
        handle_color: Color::from_rgb(0.50, 0.50, 0.55),
        background: Background::Color(bg),
        border: Border {
            color: Color::from_rgb(0.25, 0.25, 0.30),
            width: 1.0,
            radius: 3.0.into(),
        },
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

    // Airplane mode toggle — just the ✈ icon, like bDS
    let airplane_btn = button(text("\u{2708}").size(13))
        .on_press(Message::SetOfflineMode(!offline_mode))
        .padding([2, 4])
        .style(if offline_mode { airplane_active } else { airplane_inactive });

    // Language selector — "UI" label + pick_list dropdown, like bDS
    let options: Vec<LocaleOption> = UiLocale::all()
        .iter()
        .map(|l| LocaleOption::new(*l, locale))
        .collect();
    let selected = Some(LocaleOption::new(locale, locale));

    let locale_picker = pick_list(
        options,
        selected,
        |opt: LocaleOption| Message::SetUiLocale(opt.locale),
    )
    .text_size(11)
    .padding([2, 4])
    .width(Length::Shrink)
    .style(locale_pick_list_style);

    let right = row![
        text(posts_label).size(11).color(label_color),
        text(media_label).size(11).color(label_color),
        airplane_btn,
        text(t(locale, "statusBar.ui")).size(11).color(label_color),
        locale_picker,
        text("bDS").size(11).color(Color::from_rgb(0.45, 0.45, 0.50)),
    ]
    .spacing(8)
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
    .height(Length::Fixed(24.0))
    .style(bar_style)
    .into()
}
