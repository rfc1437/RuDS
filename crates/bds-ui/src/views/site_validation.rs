use iced::widget::text::Shaping;
use iced::widget::{button, column, container, row, scrollable, text};
use iced::{Background, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::t;

#[derive(Debug, Clone, Default)]
pub struct SiteValidationState {
    pub has_run: bool,
    pub is_running: bool,
    pub is_applying: bool,
    pub missing_files: Vec<String>,
    pub extra_files: Vec<String>,
    pub stale_files: Vec<String>,
    pub error_message: Option<String>,
}

pub fn view<'a>(state: &'a SiteValidationState, locale: UiLocale) -> Element<'a, Message> {
    let run_button = if state.is_running {
        button(text(t(locale, "siteValidation.running")).size(13).shaping(Shaping::Advanced))
    } else {
        button(text(t(locale, "siteValidation.run")).size(13).shaping(Shaping::Advanced))
            .on_press(Message::RunSiteValidation)
    };
    let has_issues = !state.missing_files.is_empty() || !state.extra_files.is_empty() || !state.stale_files.is_empty();
    let apply_button = if state.is_applying {
        button(text(t(locale, "siteValidation.applying")).size(13).shaping(Shaping::Advanced))
    } else if !state.is_running && state.error_message.is_none() && has_issues {
        button(text(t(locale, "siteValidation.apply")).size(13).shaping(Shaping::Advanced))
            .on_press(Message::ApplySiteValidation)
    } else {
        button(text(t(locale, "siteValidation.apply")).size(13).shaping(Shaping::Advanced))
    };

    let mut content = column![
        row![
            text(t(locale, "tabBar.siteValidation"))
                .size(24)
                .shaping(Shaping::Advanced)
                .color(Color::from_rgb(0.88, 0.88, 0.92)),
            row![run_button, apply_button].spacing(12),
        ]
        .spacing(16),
    ]
    .spacing(16);

    if state.is_running {
        content = content.push(help_text(t(locale, "siteValidation.running")));
    } else if let Some(error) = &state.error_message {
        content = content.push(section(
            t(locale, "siteValidation.error"),
            std::slice::from_ref(error),
            Color::from_rgb(0.75, 0.28, 0.28),
        ));
    } else if !state.has_run {
        content = content.push(help_text(t(locale, "siteValidation.idle")));
    } else if state.missing_files.is_empty() && state.extra_files.is_empty() && state.stale_files.is_empty() {
        content = content.push(help_text(t(locale, "siteValidation.clean")));
    } else {
        if !state.missing_files.is_empty() {
            content = content.push(section(
                format!("{} ({})", t(locale, "siteValidation.missing"), state.missing_files.len()),
                &state.missing_files,
                Color::from_rgb(0.70, 0.25, 0.25),
            ));
        }
        if !state.extra_files.is_empty() {
            content = content.push(section(
                format!("{} ({})", t(locale, "siteValidation.extra"), state.extra_files.len()),
                &state.extra_files,
                Color::from_rgb(0.68, 0.48, 0.18),
            ));
        }
        if !state.stale_files.is_empty() {
            content = content.push(section(
                format!("{} ({})", t(locale, "siteValidation.stale"), state.stale_files.len()),
                &state.stale_files,
                Color::from_rgb(0.62, 0.32, 0.18),
            ));
        }
    }

    container(scrollable(content))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(Color::from_rgb(0.11, 0.11, 0.14))),
            ..container::Style::default()
        })
        .into()
}

fn help_text<'a>(value: String) -> Element<'a, Message> {
    container(
        text(value)
            .size(14)
            .shaping(Shaping::Advanced)
            .color(Color::from_rgb(0.66, 0.66, 0.72)),
    )
    .padding(16)
    .style(|_theme: &Theme| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.14, 0.14, 0.18))),
        ..container::Style::default()
    })
    .into()
}

fn section<'a>(title: String, entries: &'a [String], accent: Color) -> Element<'a, Message> {
    let items = entries.iter().fold(column!().spacing(6), |column, entry| {
        column.push(
            text(entry)
                .size(13)
                .shaping(Shaping::Advanced)
                .color(Color::from_rgb(0.82, 0.82, 0.88)),
        )
    });

    container(
        column![
            text(title.to_string())
                .size(16)
                .shaping(Shaping::Advanced)
                .color(accent),
            items,
        ]
        .spacing(10),
    )
    .padding(16)
    .style(|_theme: &Theme| container::Style {
        background: Some(Background::Color(Color::from_rgb(0.14, 0.14, 0.18))),
        ..container::Style::default()
    })
    .into()
}