use std::collections::HashSet;

use bds_core::engine::embedding::DuplicateSearchResult;
use bds_core::i18n::UiLocale;
use iced::widget::text::Shaping;
use iced::widget::{Space, button, checkbox, column, container, row, scrollable, text};
use iced::{Color, Element, Length};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::{t, tw};

#[derive(Debug, Clone, Default)]
pub struct DuplicatesState {
    pub enabled: bool,
    pub is_loading: bool,
    pub has_run: bool,
    pub page: usize,
    pub result: DuplicateSearchResult,
    pub selected: HashSet<(String, String)>,
    pub error: Option<String>,
}

pub fn view(state: &DuplicatesState, locale: UiLocale) -> Element<'_, Message> {
    let refresh = if state.is_loading {
        button(text(t(locale, "duplicates.searching")).size(13)).style(inputs::secondary_button)
    } else {
        button(text(t(locale, "common.refresh")).size(13))
            .on_press(Message::DuplicatesRefresh)
            .style(inputs::secondary_button)
    }
    .padding([6, 16]);

    let dismiss_checked = if state.selected.is_empty() || state.is_loading {
        button(text(t(locale, "duplicates.dismissChecked")).size(13))
            .style(inputs::secondary_button)
    } else {
        button(
            text(tw(
                locale,
                "duplicates.dismissCheckedCount",
                &[("count", &state.selected.len().to_string())],
            ))
            .size(13),
        )
        .on_press(Message::DuplicatesDismissSelected)
        .style(inputs::primary_button)
    }
    .padding([6, 16]);

    let toolbar = inputs::toolbar(
        vec![
            text(t(locale, "duplicates.title"))
                .size(20)
                .shaping(Shaping::Advanced)
                .into(),
            text(tw(
                locale,
                "duplicates.count",
                &[("count", &state.result.pairs.len().to_string())],
            ))
            .size(12)
            .color(inputs::LABEL_COLOR)
            .into(),
        ],
        vec![
            button(text(t(locale, "duplicates.checkAll")).size(13))
                .on_press(Message::DuplicatesCheckAll)
                .padding([6, 12])
                .style(inputs::secondary_button)
                .into(),
            button(text(t(locale, "duplicates.uncheckAll")).size(13))
                .on_press(Message::DuplicatesUncheckAll)
                .padding([6, 12])
                .style(inputs::secondary_button)
                .into(),
            dismiss_checked.into(),
            refresh.into(),
        ],
    );

    let body: Element<'_, Message> = if !state.enabled {
        inputs::card(
            text(t(locale, "duplicates.disabled"))
                .size(14)
                .color(inputs::LABEL_COLOR),
        )
        .into()
    } else if state.is_loading && !state.has_run {
        inputs::card(
            text(t(locale, "duplicates.searching"))
                .size(14)
                .color(inputs::LABEL_COLOR),
        )
        .into()
    } else if let Some(error) = &state.error {
        inputs::card(
            text(error.clone())
                .size(14)
                .color(Color::from_rgb(0.90, 0.38, 0.38)),
        )
        .into()
    } else if state.has_run && state.result.pairs.is_empty() {
        inputs::card(
            text(t(locale, "duplicates.empty"))
                .size(14)
                .color(inputs::LABEL_COLOR),
        )
        .into()
    } else {
        let mut pairs = column!().spacing(8);
        for pair in &state.result.pairs {
            let key = (pair.post_id_a.clone(), pair.post_id_b.clone());
            let checked = state.selected.contains(&key);
            let badge = if pair.exact_match {
                t(locale, "duplicates.exactMatch")
            } else {
                format!("{:.1}%", pair.similarity * 100.0)
            };
            pairs = pairs.push(inputs::card(
                row![
                    checkbox("", checked)
                        .on_toggle({
                            let a = pair.post_id_a.clone();
                            let b = pair.post_id_b.clone();
                            move |_| Message::DuplicatesToggle(a.clone(), b.clone())
                        })
                        .size(16),
                    button(
                        text(pair.title_a.clone())
                            .size(13)
                            .shaping(Shaping::Advanced)
                    )
                    .on_press(Message::DuplicatesOpenPost(pair.post_id_a.clone()))
                    .padding([5, 8])
                    .style(inputs::disclosure_button),
                    text("→").size(14).color(inputs::LABEL_COLOR),
                    button(
                        text(pair.title_b.clone())
                            .size(13)
                            .shaping(Shaping::Advanced)
                    )
                    .on_press(Message::DuplicatesOpenPost(pair.post_id_b.clone()))
                    .padding([5, 8])
                    .style(inputs::disclosure_button),
                    Space::with_width(Length::Fill),
                    text(badge).size(12).color(if pair.exact_match {
                        Color::from_rgb(0.96, 0.68, 0.28)
                    } else {
                        Color::from_rgb(0.55, 0.76, 0.92)
                    }),
                    button(text(t(locale, "duplicates.dismiss")).size(12))
                        .on_press(Message::DuplicatesDismiss(
                            pair.post_id_a.clone(),
                            pair.post_id_b.clone()
                        ))
                        .padding([5, 12])
                        .style(inputs::secondary_button),
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            ));
        }
        if state.result.has_more {
            pairs = pairs.push(
                button(text(t(locale, "duplicates.showMore")).size(13))
                    .on_press(Message::DuplicatesShowMore)
                    .padding([7, 16])
                    .style(inputs::secondary_button),
            );
        }
        scrollable(container(pairs).padding(2))
            .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
            .style(inputs::scrollable_style)
            .height(Length::Fill)
            .into()
    };

    container(column![toolbar, body].spacing(12))
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
