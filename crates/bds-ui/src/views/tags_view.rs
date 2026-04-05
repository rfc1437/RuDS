use std::collections::HashMap;

use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Alignment, Background, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::Tag;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

const DEFAULT_TAG_COLOR: &str = "#6495ed";

/// Active view within the Tags tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagsSection {
    Cloud,
    Manage,
    Merge,
}

/// State for the tags view.
#[derive(Debug, Clone)]
pub struct TagsViewState {
    pub section: TagsSection,
    pub tags: Vec<Tag>,
    pub tag_post_counts: HashMap<String, usize>,
    pub search_query: String,
    /// For "Manage": the currently editing tag
    pub editing_tag: Option<EditingTag>,
    /// For "Merge": source and target tag selection
    pub merge_source: Option<String>,
    pub merge_target: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EditingTag {
    pub id: String,
    pub name: String,
    pub color: String,
    pub template_slug: String,
}

impl TagsViewState {
    pub fn new(tags: Vec<Tag>, tag_post_counts: HashMap<String, usize>) -> Self {
        Self {
            section: TagsSection::Cloud,
            tags,
            tag_post_counts,
            search_query: String::new(),
            editing_tag: None,
            merge_source: None,
            merge_target: None,
        }
    }
}

/// Tags view messages.
#[derive(Debug, Clone)]
pub enum TagsMsg {
    SetSection(TagsSection),
    SearchChanged(String),
    SelectTag(String),
    CreateTag(String),
    EditTagName(String),
    EditTagColor(String),
    EditTagTemplate(String),
    SaveTag,
    DeleteTag(String),
    SetMergeSource(String),
    SetMergeTarget(String),
    MergeTags,
}

/// Render the tags management view.
pub fn view<'a>(
    state: &'a TagsViewState,
    locale: UiLocale,
) -> Element<'a, Message> {
    // Section navigation tabs
    let section_nav = row![
        section_tab(&t(locale, "tags.nav.cloud"), state.section == TagsSection::Cloud, TagsSection::Cloud),
        section_tab(&t(locale, "tags.nav.manage"), state.section == TagsSection::Manage, TagsSection::Manage),
        section_tab(&t(locale, "tags.nav.merge"), state.section == TagsSection::Merge, TagsSection::Merge),
    ]
    .spacing(4)
    .padding(8);

    let content: Element<'a, Message> = match state.section {
        TagsSection::Cloud => view_cloud(state, locale),
        TagsSection::Manage => view_manage(state, locale),
        TagsSection::Merge => view_merge(state, locale),
    };

    column![section_nav, content]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn section_tab<'a>(label: &str, active: bool, section: TagsSection) -> Element<'a, Message> {
    let color = if active {
        Color::WHITE
    } else {
        Color::from_rgb(0.55, 0.58, 0.65)
    };
    button(text(label.to_string()).size(13).color(color))
        .on_press(Message::Tags(TagsMsg::SetSection(section)))
        .padding([6, 12])
        .style(move |_theme: &Theme, _status| {
            if active {
                button::Style {
                    background: Some(Background::Color(Color::from_rgb(0.25, 0.27, 0.33))),
                    border: iced::Border {
                        radius: 4.0.into(),
                        ..iced::Border::default()
                    },
                    ..button::Style::default()
                }
            } else {
                button::Style::default()
            }
        })
        .into()
}

fn view_cloud<'a>(state: &'a TagsViewState, locale: UiLocale) -> Element<'a, Message> {
    if state.tags.is_empty() {
        return container(
            text(t(locale, "tags.noTags")).size(14).color(Color::from_rgb(0.5, 0.5, 0.5)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into();
    }

    // Calculate min/max post counts for font scaling
    let counts: Vec<usize> = state.tags.iter()
        .map(|t| *state.tag_post_counts.get(&t.name.to_lowercase()).unwrap_or(&0))
        .collect();
    let min_count = *counts.iter().min().unwrap_or(&0);
    let max_count = *counts.iter().max().unwrap_or(&1);
    let count_range = if max_count > min_count { (max_count - min_count) as f32 } else { 1.0 };
    const MIN_FONT: f32 = 11.0;
    const MAX_FONT: f32 = 24.0;

    let chips: Vec<Element<'a, Message>> = state
        .tags
        .iter()
        .map(|tag| {
            let color = parse_tag_color(tag.color.as_deref().unwrap_or(DEFAULT_TAG_COLOR));
            let post_count = *state.tag_post_counts.get(&tag.name.to_lowercase()).unwrap_or(&0);
            let font_size = MIN_FONT + ((post_count - min_count) as f32 / count_range) * (MAX_FONT - MIN_FONT);
            let vert_pad = ((MAX_FONT - font_size) / 4.0).max(2.0) as u16;
            button(text(&tag.name).size(font_size).color(Color::WHITE))
                .on_press(Message::Tags(TagsMsg::SelectTag(tag.id.clone())))
                .padding([vert_pad, 10])
                .style(move |_: &Theme, _| button::Style {
                    background: Some(Background::Color(color)),
                    border: iced::Border {
                        radius: 12.0.into(),
                        ..iced::Border::default()
                    },
                    text_color: Color::WHITE,
                    ..button::Style::default()
                })
                .into()
        })
        .collect();

    let cloud = row(chips).spacing(6).wrap();

    scrollable(
        container(cloud)
            .width(Length::Fill)
            .padding(16),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_manage<'a>(state: &'a TagsViewState, locale: UiLocale) -> Element<'a, Message> {
    let search = text_input(&t(locale, "sidebar.filter.search"), &state.search_query)
        .on_input(|s| Message::Tags(TagsMsg::SearchChanged(s)))
        .size(14);

    let filtered: Vec<&Tag> = state
        .tags
        .iter()
        .filter(|t| {
            state.search_query.is_empty()
                || t.name.to_lowercase().contains(&state.search_query.to_lowercase())
        })
        .collect();

    let rows: Vec<Element<'a, Message>> = filtered
        .iter()
        .map(|tag| {
            let color = parse_tag_color(tag.color.as_deref().unwrap_or(DEFAULT_TAG_COLOR));
            row![
                container(Space::new(12, 12))
                    .style(move |_: &Theme| container::Style {
                        background: Some(Background::Color(color)),
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..iced::Border::default()
                        },
                        ..container::Style::default()
                    }),
                text(&tag.name).size(14),
                Space::with_width(Length::Fill),
                button(text(t(locale, "modal.confirmDelete.delete")).size(12))
                    .on_press(Message::Tags(TagsMsg::DeleteTag(tag.id.clone())))
                    .style(inputs::danger_button)
                    .padding([3, 8])
            ]
            .spacing(8)
            .align_y(Alignment::Center)
            .padding(4)
            .into()
        })
        .collect();

    let tag_list = iced::widget::Column::with_children(rows).spacing(2);

    // Edit panel for selected tag
    let edit_panel: Element<'a, Message> = if let Some(ref editing) = state.editing_tag {
        column![
            inputs::section_header(&t(locale, "tags.editTag")),
            inputs::labeled_input(
                &t(locale, "tags.name"),
                "",
                &editing.name,
                |s| Message::Tags(TagsMsg::EditTagName(s)),
            ),
            inputs::labeled_input(
                &t(locale, "tags.color"),
                "#3498db",
                &editing.color,
                |s| Message::Tags(TagsMsg::EditTagColor(s)),
            ),
            inputs::labeled_input(
                &t(locale, "tags.postTemplate"),
                "",
                &editing.template_slug,
                |s| Message::Tags(TagsMsg::EditTagTemplate(s)),
            ),
            button(text(t(locale, "common.save")).size(13))
                .on_press(Message::Tags(TagsMsg::SaveTag))
                .style(inputs::primary_button)
                .padding([6, 16]),
        ]
        .spacing(8)
        .padding(12)
        .into()
    } else {
        Space::new(0, 0).into()
    };

    scrollable(
        column![
            search,
            tag_list,
            edit_panel,
        ]
        .spacing(12)
        .padding(16)
        .width(Length::Fill)
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_merge<'a>(state: &'a TagsViewState, locale: UiLocale) -> Element<'a, Message> {
    let source_input = inputs::labeled_input(
        &t(locale, "tags.mergeSource"),
        &t(locale, "tags.selectTag"),
        state.merge_source.as_deref().unwrap_or(""),
        |s| Message::Tags(TagsMsg::SetMergeSource(s)),
    );
    let target_input = inputs::labeled_input(
        &t(locale, "tags.mergeTarget"),
        &t(locale, "tags.selectTag"),
        state.merge_target.as_deref().unwrap_or(""),
        |s| Message::Tags(TagsMsg::SetMergeTarget(s)),
    );

    let can_merge = state.merge_source.is_some() && state.merge_target.is_some();

    let merge_btn = if can_merge {
        button(text(t(locale, "tags.merge")).size(13))
            .on_press(Message::Tags(TagsMsg::MergeTags))
            .style(inputs::primary_button)
            .padding([6, 16])
    } else {
        button(text(t(locale, "tags.merge")).size(13))
            .padding([6, 16])
    };

    column![
        source_input,
        target_input,
        merge_btn,
    ]
    .spacing(12)
    .padding(16)
    .width(Length::Fill)
    .into()
}

fn parse_tag_color(hex: &str) -> Color {
    let hex = hex.trim_start_matches('#');
    if hex.len() == 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(100);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(100);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(100);
        Color::from_rgb(r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0)
    } else {
        Color::from_rgb(0.4, 0.5, 0.7)
    }
}
