use iced::widget::text::Shaping;
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Color, Element, Length};

use bds_core::engine::metadata_diff::{DiffReport, RepairDirection};
use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::{t, tw};

#[derive(Debug, Clone, Default)]
pub struct MetadataDiffState {
    pub is_running: bool,
    pub is_repairing: bool,
    pub report: Option<DiffReport>,
    pub error_message: Option<String>,
}

pub fn view<'a>(state: &'a MetadataDiffState, locale: UiLocale) -> Element<'a, Message> {
    let run = button(text(t(locale, "metadataDiff.run")).size(13))
        .on_press_maybe(
            (!state.is_running && !state.is_repairing).then_some(Message::RunMetadataDiff),
        )
        .style(inputs::primary_button)
        .padding([6, 16]);
    let mut content = column![
        row![
            text(t(locale, "tabBar.metadataDiff"))
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
        content = content.push(message_card(t(locale, "metadataDiff.running")));
    } else if let Some(error) = &state.error_message {
        content = content.push(message_card(error.clone()));
    } else if let Some(report) = &state.report {
        if report.diffs.is_empty() && report.orphans.is_empty() && report.errors.is_empty() {
            content = content.push(message_card(t(locale, "metadataDiff.clean")));
        }
        for (index, item) in report.diffs.iter().enumerate() {
            let fields = item
                .fields
                .iter()
                .fold(column!().spacing(4), |column, field| {
                    column.push(
                        text(tw(
                            locale,
                            "metadataDiff.field",
                            &[
                                ("field", &field.field_name),
                                ("database", &field.db_value),
                                ("file", &field.file_value),
                            ],
                        ))
                        .size(12)
                        .color(Color::from_rgb(0.72, 0.72, 0.78)),
                    )
                });
            let actions = row![
                button(text(t(locale, "metadataDiff.fileToDb")).size(12))
                    .on_press_maybe((!state.is_repairing).then_some(
                        Message::RepairMetadataDiffItem {
                            index,
                            direction: RepairDirection::FileToDatabase,
                        },
                    ))
                    .style(inputs::secondary_button)
                    .padding([5, 10]),
                button(text(t(locale, "metadataDiff.dbToFile")).size(12))
                    .on_press_maybe((!state.is_repairing).then_some(
                        Message::RepairMetadataDiffItem {
                            index,
                            direction: RepairDirection::DatabaseToFile,
                        },
                    ))
                    .style(inputs::secondary_button)
                    .padding([5, 10]),
            ]
            .spacing(8);
            content = content.push(inputs::card(
                column![
                    text(tw(
                        locale,
                        "metadataDiff.entity",
                        &[("entity", &item.entity_type), ("path", &item.file_path)],
                    ))
                    .size(15)
                    .color(Color::from_rgb(0.86, 0.86, 0.92)),
                    fields,
                    actions,
                ]
                .spacing(10),
            ));
        }
        if !report.orphans.is_empty() {
            let rows = report
                .orphans
                .iter()
                .fold(column!().spacing(4), |column, orphan| {
                    column.push(
                        text(tw(
                            locale,
                            "metadataDiff.orphan",
                            &[("reason", &orphan.reason), ("path", &orphan.file_path)],
                        ))
                        .size(12),
                    )
                });
            content = content.push(inputs::card(
                column![text(t(locale, "metadataDiff.orphans")).size(16), rows].spacing(8),
            ));
        }
        if !report.errors.is_empty() {
            content = content.push(inputs::card(
                column![
                    text(t(locale, "metadataDiff.errors")).size(16),
                    report
                        .errors
                        .iter()
                        .fold(column!().spacing(4), |column, error| {
                            column.push(text(error.clone()).size(12))
                        }),
                ]
                .spacing(8),
            ));
        }
    } else {
        content = content.push(message_card(t(locale, "metadataDiff.idle")));
    }

    container(
        scrollable(content)
            .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
            .style(inputs::scrollable_style),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding(24)
    .into()
}

fn message_card(value: String) -> Element<'static, Message> {
    inputs::card(
        text(value)
            .size(14)
            .color(Color::from_rgb(0.66, 0.66, 0.72)),
    )
    .into()
}
