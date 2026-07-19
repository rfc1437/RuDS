use std::collections::HashMap;

use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::Tag;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::{t, tw};

const DEFAULT_TAG_COLOR: &str = "#6495ed";
const COLOR_PRESETS: [&str; 17] = [
    "#e63946", "#f77f00", "#fcbf49", "#90be6d", "#43aa8b", "#4d908e", "#577590", "#277da1",
    "#4361ee", "#7209b7", "#b5179e", "#f72585", "#9c6644", "#6c757d", "#adb5bd", "#2a9d8f",
    "#264653",
];

/// Active view within the Tags tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagsSection {
    Cloud,
    Manage,
    Merge,
    Discover,
}

/// State for the tags view.
#[derive(Debug, Clone)]
pub struct TagsViewState {
    pub section: TagsSection,
    pub tags: Vec<Tag>,
    pub tag_post_counts: HashMap<String, usize>,
    pub search_query: String,
    pub selected_tags: Vec<String>,
    pub create_name: String,
    pub create_color: String,
    pub editing_tag: Option<EditingTag>,
    pub merge_target: Option<String>,
    pub template_options: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct EditingTag {
    pub id: String,
    pub original_name: String,
    pub name: String,
    pub color: String,
    pub template_slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagOption {
    pub id: String,
    pub name: String,
}

impl std::fmt::Display for TagOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateOption {
    pub slug: String,
    pub label: String,
}

impl std::fmt::Display for TemplateOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label)
    }
}

impl TagsViewState {
    pub fn new(
        tags: Vec<Tag>,
        tag_post_counts: HashMap<String, usize>,
        template_options: Vec<String>,
    ) -> Self {
        Self {
            section: TagsSection::Cloud,
            tags,
            tag_post_counts,
            search_query: String::new(),
            selected_tags: Vec::new(),
            create_name: String::new(),
            create_color: String::new(),
            editing_tag: None,
            merge_target: None,
            template_options,
        }
    }
}

/// Tags view messages.
#[derive(Debug, Clone)]
pub enum TagsMsg {
    SetSection(TagsSection),
    SearchChanged(String),
    ToggleTagSelection(String),
    ClearSelection,
    CreateNameChanged(String),
    CreateColorChanged(String),
    CreateTag,
    EditTagName(String),
    EditTagColor(String),
    EditTagTemplate(TemplateOption),
    SaveTag,
    DeleteTag(String),
    SetMergeTarget(TagOption),
    MergeTags,
    SyncTags,
}

/// Render the tags management view.
pub fn view<'a>(state: &'a TagsViewState, locale: UiLocale) -> Element<'a, Message> {
    let section_nav = inputs::card(
        row![
            section_tab(
                &t(locale, "tags.nav.cloud"),
                state.section == TagsSection::Cloud,
                TagsSection::Cloud
            ),
            section_tab(
                &t(locale, "tags.nav.manage"),
                state.section == TagsSection::Manage,
                TagsSection::Manage
            ),
            section_tab(
                &t(locale, "tags.nav.merge"),
                state.section == TagsSection::Merge,
                TagsSection::Merge
            ),
            section_tab(
                &t(locale, "tags.nav.discover"),
                state.section == TagsSection::Discover,
                TagsSection::Discover
            ),
        ]
        .spacing(6),
    )
    .padding(6);

    let content: Element<'a, Message> = match state.section {
        TagsSection::Cloud => view_cloud(state, locale),
        TagsSection::Manage => view_manage(state, locale),
        TagsSection::Merge => view_merge(state, locale),
        TagsSection::Discover => view_discover(state, locale),
    };

    column![section_nav, content]
        .spacing(4)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn section_tab<'a>(label: &str, active: bool, section: TagsSection) -> Element<'a, Message> {
    button(text(label.to_string()).size(13))
        .on_press(Message::Tags(TagsMsg::SetSection(section)))
        .padding([6, 12])
        .style(if active {
            inputs::primary_button
        } else {
            inputs::secondary_button
        })
        .into()
}

fn view_cloud<'a>(state: &'a TagsViewState, locale: UiLocale) -> Element<'a, Message> {
    if state.tags.is_empty() {
        return container(
            text(t(locale, "tags.noTags"))
                .size(14)
                .color(Color::from_rgb(0.5, 0.5, 0.5)),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into();
    }

    let counts: Vec<usize> = state
        .tags
        .iter()
        .map(|tag| {
            *state
                .tag_post_counts
                .get(&tag.name.to_lowercase())
                .unwrap_or(&0)
        })
        .collect();
    let min_count = *counts.iter().min().unwrap_or(&0);
    let max_count = *counts.iter().max().unwrap_or(&1);
    let count_range = if max_count > min_count {
        (max_count - min_count) as f32
    } else {
        1.0
    };
    const MIN_FONT: f32 = 12.0;
    const MAX_FONT: f32 = 18.0;

    let chips: Vec<Element<'a, Message>> = state
        .tags
        .iter()
        .map(|tag| {
            let color = parse_tag_color(tag.color.as_deref().unwrap_or(DEFAULT_TAG_COLOR));
            let post_count = *state
                .tag_post_counts
                .get(&tag.name.to_lowercase())
                .unwrap_or(&0);
            let font_size =
                MIN_FONT + ((post_count - min_count) as f32 / count_range) * (MAX_FONT - MIN_FONT);
            let vert_pad = ((MAX_FONT - font_size) / 4.0).max(2.0) as u16;
            let selected = state
                .selected_tags
                .iter()
                .any(|selected_id| selected_id == &tag.id);
            button(
                row![
                    text(&tag.name).size(font_size).color(Color::WHITE),
                    text(post_count.to_string())
                        .size(11)
                        .color(Color::from_rgba(1.0, 1.0, 1.0, 0.85)),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            )
            .on_press(Message::Tags(TagsMsg::ToggleTagSelection(tag.id.clone())))
            .padding([vert_pad, 10])
            .style(move |_: &Theme, status| button::Style {
                background: Some(Background::Color(match (selected, status) {
                    (true, button::Status::Hovered) => Color::from_rgb(0.20, 0.24, 0.30),
                    (true, _) => Color::from_rgb(0.17, 0.20, 0.25),
                    (false, button::Status::Hovered) => Color::from_rgb(0.23, 0.24, 0.26),
                    (false, _) => Color::from_rgb(0.18, 0.18, 0.19),
                })),
                border: iced::Border {
                    radius: 12.0.into(),
                    width: if selected { 2.0 } else { 1.0 },
                    color,
                },
                text_color: Color::WHITE,
                ..button::Style::default()
            })
            .into()
        })
        .collect();

    let selection_summary: Element<'a, Message> = if state.selected_tags.is_empty() {
        text(t(locale, "tags.cloudHelp"))
            .size(12)
            .color(Color::from_rgb(0.60, 0.60, 0.65))
            .into()
    } else {
        row![
            text(tw(
                locale,
                "tags.selectedCount",
                &[("count", &state.selected_tags.len().to_string())]
            ))
            .size(12)
            .color(Color::from_rgb(0.75, 0.77, 0.82)),
            button(text(t(locale, "tags.clearSelection")).size(12))
                .on_press(Message::Tags(TagsMsg::ClearSelection))
                .style(inputs::secondary_button)
                .padding([4, 8]),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
    };

    scrollable(
        column![inputs::card(
            column![
                inputs::section_header(&t(locale, "tags.cloudSection")),
                selection_summary,
                row(chips).spacing(6).wrap(),
            ]
            .spacing(12)
            .width(Length::Fill),
        )]
        .padding(16),
    )
    .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
    .style(inputs::scrollable_style)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_manage<'a>(state: &'a TagsViewState, locale: UiLocale) -> Element<'a, Message> {
    let create_panel = column![
        inputs::section_header(&t(locale, "tags.createSection")),
        inputs::labeled_input(
            &t(locale, "tags.name"),
            &t(locale, "tags.createPlaceholder"),
            &state.create_name,
            |value| Message::Tags(TagsMsg::CreateNameChanged(value)),
        ),
        inputs::labeled_input(
            &t(locale, "tags.color"),
            DEFAULT_TAG_COLOR,
            &state.create_color,
            |value| Message::Tags(TagsMsg::CreateColorChanged(value)),
        ),
        color_swatches(locale, true),
        button(text(t(locale, "tags.createButton")).size(13))
            .on_press_maybe(
                (!state.create_name.trim().is_empty()).then_some(Message::Tags(TagsMsg::CreateTag))
            )
            .style(inputs::primary_button)
            .padding([6, 16]),
    ]
    .spacing(8);

    let search = text_input(&t(locale, "sidebar.filter.search"), &state.search_query)
        .on_input(|value| Message::Tags(TagsMsg::SearchChanged(value)))
        .size(14)
        .padding([8, 10])
        .style(inputs::field_style);

    let filtered: Vec<&Tag> = state
        .tags
        .iter()
        .filter(|tag| {
            state.search_query.is_empty()
                || tag
                    .name
                    .to_lowercase()
                    .contains(&state.search_query.to_lowercase())
        })
        .collect();

    let rows: Vec<Element<'a, Message>> = filtered
        .iter()
        .map(|tag| {
            let color = parse_tag_color(tag.color.as_deref().unwrap_or(DEFAULT_TAG_COLOR));
            let selected = state
                .selected_tags
                .iter()
                .any(|selected_id| selected_id == &tag.id);
            let count = state
                .tag_post_counts
                .get(&tag.name.to_lowercase())
                .copied()
                .unwrap_or(0);
            button(
                row![
                    container(Space::new(12, 12)).style(move |_: &Theme| container::Style {
                        background: Some(Background::Color(color)),
                        border: iced::Border {
                            radius: 6.0.into(),
                            ..iced::Border::default()
                        },
                        ..container::Style::default()
                    }),
                    text(&tag.name).size(14),
                    text(format!("({count})"))
                        .size(12)
                        .color(Color::from_rgb(0.55, 0.58, 0.65)),
                    Space::with_width(Length::Fill),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .on_press(Message::Tags(TagsMsg::ToggleTagSelection(tag.id.clone())))
            .padding([6, 8])
            .style(move |_theme: &Theme, _status| button::Style {
                background: Some(Background::Color(if selected {
                    Color::from_rgb(0.22, 0.24, 0.30)
                } else {
                    Color::TRANSPARENT
                })),
                border: iced::Border {
                    radius: 6.0.into(),
                    width: if selected { 1.0 } else { 0.0 },
                    color: Color::from_rgb(0.35, 0.45, 0.65),
                },
                text_color: Color::WHITE,
                ..button::Style::default()
            })
            .into()
        })
        .collect();

    let tag_list = iced::widget::Column::with_children(rows).spacing(4);

    let edit_panel: Element<'a, Message> = if let Some(ref editing) = state.editing_tag {
        let template_options = template_options(state, locale);
        let selected_template = template_options
            .iter()
            .find(|option| option.slug == editing.template_slug);
        let delete_button: Element<'a, Message> =
            button(text(t(locale, "modal.confirmDelete.delete")).size(13))
                .on_press(Message::Tags(TagsMsg::DeleteTag(editing.id.clone())))
                .style(inputs::danger_button)
                .padding([6, 16])
                .into();

        column![
            inputs::section_header(&t(locale, "tags.editTag")),
            inputs::labeled_input(&t(locale, "tags.name"), "", &editing.name, |value| {
                Message::Tags(TagsMsg::EditTagName(value))
            },),
            inputs::labeled_input(
                &t(locale, "tags.color"),
                DEFAULT_TAG_COLOR,
                &editing.color,
                |value| Message::Tags(TagsMsg::EditTagColor(value)),
            ),
            color_swatches(locale, false),
            inputs::labeled_select(
                &t(locale, "tags.postTemplate"),
                &template_options,
                selected_template,
                |choice| Message::Tags(TagsMsg::EditTagTemplate(choice)),
            ),
            row![
                button(text(t(locale, "common.save")).size(13))
                    .on_press(Message::Tags(TagsMsg::SaveTag))
                    .style(inputs::primary_button)
                    .padding([6, 16]),
                delete_button,
            ]
            .spacing(8),
        ]
        .spacing(8)
        .into()
    } else {
        container(
            text(t(locale, "tags.selectTag"))
                .size(12)
                .color(Color::from_rgb(0.60, 0.60, 0.65)),
        )
        .padding([8, 0])
        .into()
    };

    scrollable(
        column![
            inputs::card(create_panel),
            inputs::card(
                column![
                    inputs::section_header(&t(locale, "tags.manageSection")),
                    search,
                    tag_list,
                    edit_panel,
                ]
                .spacing(12)
            ),
        ]
        .spacing(12)
        .padding(16)
        .width(Length::Fill),
    )
    .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
    .style(inputs::scrollable_style)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_merge<'a>(state: &'a TagsViewState, locale: UiLocale) -> Element<'a, Message> {
    if state.selected_tags.len() < 2 {
        return container(
            text(t(locale, "tags.mergeHelp"))
                .size(12)
                .color(Color::from_rgb(0.60, 0.60, 0.65)),
        )
        .padding(16)
        .into();
    }

    let tag_options = selected_tag_options(state);
    let selected_target = state
        .merge_target
        .as_ref()
        .and_then(|target_id| tag_options.iter().find(|option| &option.id == target_id));

    let merge_preview = state
        .merge_target
        .as_ref()
        .map(|target_id| {
            selected_tag_options(state)
                .into_iter()
                .filter(|option| &option.id != target_id)
                .map(|option| option.name)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();

    container(inputs::card(
        column![
            inputs::section_header(&t(locale, "tags.mergeSection")),
            text(tw(
                locale,
                "tags.selectedCount",
                &[("count", &state.selected_tags.len().to_string())]
            ))
            .size(12)
            .color(Color::from_rgb(0.75, 0.77, 0.82)),
            inputs::labeled_select(
                &t(locale, "tags.mergeTarget"),
                &tag_options,
                selected_target,
                |choice| Message::Tags(TagsMsg::SetMergeTarget(choice)),
            ),
            if merge_preview.is_empty() {
                Element::from(Space::new(0, 0))
            } else {
                container(
                    text(tw(locale, "tags.mergePreview", &[("tags", &merge_preview)])).size(12),
                )
                .padding([4, 0])
                .into()
            },
            button(text(t(locale, "tags.merge")).size(13))
                .on_press_maybe(
                    state
                        .merge_target
                        .is_some()
                        .then_some(Message::Tags(TagsMsg::MergeTags))
                )
                .style(inputs::primary_button)
                .padding([6, 16]),
        ]
        .spacing(12),
    ))
    .padding(16)
    .width(Length::Fill)
    .into()
}

fn view_discover<'a>(_state: &'a TagsViewState, locale: UiLocale) -> Element<'a, Message> {
    container(inputs::card(
        column![
            inputs::section_header(&t(locale, "tags.discoverSection")),
            text(t(locale, "tags.discoverDescription"))
                .size(12)
                .color(Color::from_rgb(0.60, 0.60, 0.65)),
            button(text(t(locale, "tags.discoverButton")).size(13))
                .on_press(Message::Tags(TagsMsg::SyncTags))
                .style(inputs::primary_button)
                .padding([6, 16]),
        ]
        .spacing(12),
    ))
    .padding(16)
    .width(Length::Fill)
    .into()
}

fn template_options(state: &TagsViewState, locale: UiLocale) -> Vec<TemplateOption> {
    let mut options = vec![TemplateOption {
        slug: String::new(),
        label: t(locale, "tags.noTemplate"),
    }];
    options.extend(state.template_options.iter().map(|slug| TemplateOption {
        slug: slug.clone(),
        label: slug.clone(),
    }));
    options
}

fn selected_tag_options(state: &TagsViewState) -> Vec<TagOption> {
    state
        .selected_tags
        .iter()
        .filter_map(|tag_id| {
            state
                .tags
                .iter()
                .find(|tag| &tag.id == tag_id)
                .map(|tag| TagOption {
                    id: tag.id.clone(),
                    name: tag.name.clone(),
                })
        })
        .collect()
}

fn color_swatches<'a>(locale: UiLocale, create_mode: bool) -> Element<'a, Message> {
    let buttons: Vec<Element<'a, Message>> = COLOR_PRESETS
        .iter()
        .map(|hex| {
            let color = parse_tag_color(hex);
            let msg = if create_mode {
                TagsMsg::CreateColorChanged((*hex).to_string())
            } else {
                TagsMsg::EditTagColor((*hex).to_string())
            };
            button(Space::new(18, 18))
                .on_press(Message::Tags(msg))
                .padding(0)
                .style(move |_theme: &Theme, _status| button::Style {
                    background: Some(Background::Color(color)),
                    border: iced::Border {
                        radius: 9.0.into(),
                        ..iced::Border::default()
                    },
                    ..button::Style::default()
                })
                .into()
        })
        .collect();

    column![
        text(t(locale, "tags.colorPresets"))
            .size(12)
            .color(Color::from_rgb(0.55, 0.58, 0.65)),
        row(buttons).spacing(6).wrap(),
    ]
    .spacing(6)
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
