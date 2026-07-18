use iced::widget::text::Shaping;
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Color, Element, Length};

use bds_core::engine::validate_translations::{
    TranslationIssue, TranslationIssueKind, TranslationValidationReport,
};
use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

#[derive(Debug, Clone, Default)]
pub struct TranslationValidationState {
    pub is_running: bool,
    pub report: Option<TranslationValidationReport>,
    pub error_message: Option<String>,
}

pub fn view<'a>(state: &'a TranslationValidationState, locale: UiLocale) -> Element<'a, Message> {
    let run = button(text(t(locale, "translationValidation.run")).size(13))
        .on_press_maybe((!state.is_running).then_some(Message::ValidateTranslations))
        .style(inputs::primary_button)
        .padding([6, 16]);
    let mut content = column![
        row![
            text(t(locale, "tabBar.translationValidation"))
                .size(24)
                .shaping(Shaping::Advanced)
                .color(Color::from_rgb(0.88, 0.88, 0.92)),
            Space::with_width(Length::Fill),
            run,
        ]
        .align_y(iced::Alignment::Center),
    ]
    .spacing(16);

    if state.is_running {
        content = content.push(message_card(t(locale, "translationValidation.running")));
    } else if let Some(error) = &state.error_message {
        content = content.push(message_card(error.clone()));
    } else if let Some(report) = &state.report {
        if report.db_issues.is_empty() && report.fs_issues.is_empty() {
            content = content.push(message_card(t(locale, "translationValidation.clean")));
        } else {
            content = content.push(issue_section(
                t(locale, "translationValidation.databaseIssues"),
                &report.db_issues,
                locale,
            ));
            content = content.push(issue_section(
                t(locale, "translationValidation.filesystemIssues"),
                &report.fs_issues,
                locale,
            ));
        }
    } else {
        content = content.push(message_card(t(locale, "translationValidation.idle")));
    }

    container(scrollable(content))
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .into()
}

fn issue_section<'a>(
    title: String,
    issues: &'a [TranslationIssue],
    locale: UiLocale,
) -> Element<'a, Message> {
    if issues.is_empty() {
        return Space::new(0, 0).into();
    }
    let rows = issues.iter().fold(column!().spacing(6), |column, issue| {
        let location = issue.file_path.as_deref().unwrap_or(issue.post_id.as_str());
        column.push(
            text(format!(
                "{} · {} · {}",
                issue.language,
                location,
                issue_kind(locale, &issue.kind)
            ))
            .size(12),
        )
    });
    inputs::card(column![text(title).size(16), rows].spacing(8)).into()
}

fn issue_kind(locale: UiLocale, kind: &TranslationIssueKind) -> String {
    let key = match kind {
        TranslationIssueKind::MissingSourcePost => "translationValidation.missingSourcePost",
        TranslationIssueKind::SameLanguageAsCanonical => {
            "translationValidation.sameLanguageAsCanonical"
        }
        TranslationIssueKind::DoNotTranslateHasTranslations => {
            "translationValidation.doNotTranslateHasTranslations"
        }
        TranslationIssueKind::ContentInDatabase => "translationValidation.contentInDatabase",
        TranslationIssueKind::MissingTranslation => "translationValidation.missingTranslation",
    };
    t(locale, key)
}

fn message_card<'a>(message: String) -> Element<'a, Message> {
    inputs::card(
        text(message)
            .size(13)
            .color(Color::from_rgb(0.72, 0.72, 0.78)),
    )
    .into()
}
