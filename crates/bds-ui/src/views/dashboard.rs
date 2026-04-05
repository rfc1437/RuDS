use iced::widget::{column, container, row, text, Space};
use iced::{Alignment, Background, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

/// Dashboard overview state.
#[derive(Debug, Clone)]
pub struct DashboardState {
    pub post_count: usize,
    pub media_count: usize,
    pub template_count: usize,
    pub script_count: usize,
    pub draft_count: usize,
    pub published_count: usize,
    pub project_name: String,
}

impl DashboardState {
    pub fn new(project_name: String) -> Self {
        Self {
            post_count: 0,
            media_count: 0,
            template_count: 0,
            script_count: 0,
            draft_count: 0,
            published_count: 0,
            project_name,
        }
    }
}

/// Render the dashboard overview.
pub fn view<'a>(
    state: &'a DashboardState,
    locale: UiLocale,
) -> Element<'a, Message> {
    let header = text(t(locale, "dashboard.overview"))
        .size(20)
        .color(Color::WHITE);

    let project_label = text(state.project_name.clone())
        .size(14)
        .color(Color::from_rgb(0.6, 0.6, 0.7));

    let counts_row = row![
        stat_card(&t(locale, "dashboard.posts"), state.post_count),
        stat_card(&t(locale, "dashboard.media"), state.media_count),
        stat_card(&t(locale, "dashboard.templates"), state.template_count),
        stat_card(&t(locale, "dashboard.scripts"), state.script_count),
    ]
    .spacing(16);

    let status_row = row![
        stat_card(&t(locale, "dashboard.drafts"), state.draft_count),
        stat_card(&t(locale, "dashboard.published"), state.published_count),
    ]
    .spacing(16);

    container(
        column![
            header,
            project_label,
            Space::with_height(16),
            inputs::section_header(&t(locale, "dashboard.entityCounts")),
            counts_row,
            Space::with_height(12),
            inputs::section_header(&t(locale, "dashboard.statusCounts")),
            status_row,
        ]
        .spacing(8)
        .padding(24)
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn stat_card<'a>(label: &str, count: usize) -> Element<'a, Message> {
    let card_bg = Color::from_rgb(0.15, 0.16, 0.20);
    container(
        column![
            text(count.to_string()).size(28).color(Color::WHITE),
            text(label.to_string()).size(12).color(Color::from_rgb(0.55, 0.58, 0.65)),
        ]
        .spacing(4)
        .align_x(Alignment::Center),
    )
    .padding(16)
    .width(Length::FillPortion(1))
    .style(move |_: &Theme| container::Style {
        background: Some(Background::Color(card_bg)),
        border: iced::Border {
            radius: 8.0.into(),
            ..iced::Border::default()
        },
        ..container::Style::default()
    })
    .into()
}
