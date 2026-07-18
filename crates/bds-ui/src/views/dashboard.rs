use iced::widget::{Space, button, column, container, row, scrollable, text, tooltip};
use iced::{Alignment, Background, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::{t, tw};
use crate::state::tabs::{Tab, TabType};

#[derive(Debug, Clone)]
pub struct DashboardStats {
    pub total_posts: usize,
    pub published_count: usize,
    pub draft_count: usize,
    pub archived_count: usize,
    pub media_count: usize,
    pub image_count: usize,
    pub total_media_size: String,
    pub tag_count: usize,
    pub category_count: usize,
}

#[derive(Debug, Clone)]
pub struct DashboardTimelineMonth {
    pub label: String,
    pub year: i32,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct DashboardTag {
    pub name: String,
    pub count: usize,
    pub color: Option<String>,
    /// 11–22 px, scaled relative to the min/max counts of the displayed tags.
    pub font_size: f32,
}

#[derive(Debug, Clone)]
pub struct DashboardCategory {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone)]
pub struct DashboardRecentPost {
    pub post_id: String,
    pub title: String,
    pub status: String,
    pub date: String,
}

/// Dashboard overview state.
#[derive(Debug, Clone)]
pub struct DashboardState {
    pub title: String,
    pub subtitle: String,
    pub stats: DashboardStats,
    pub timeline: Vec<DashboardTimelineMonth>,
    pub tag_cloud: Vec<DashboardTag>,
    pub category_cloud: Vec<DashboardCategory>,
    pub recent_posts: Vec<DashboardRecentPost>,
}

impl DashboardState {
    pub fn new(project_name: String) -> Self {
        Self {
            title: t(UiLocale::En, "dashboard.overview"),
            subtitle: project_name,
            stats: DashboardStats {
                total_posts: 0,
                published_count: 0,
                draft_count: 0,
                archived_count: 0,
                media_count: 0,
                image_count: 0,
                total_media_size: "0 B".to_string(),
                tag_count: 0,
                category_count: 0,
            },
            timeline: Vec::new(),
            tag_cloud: Vec::new(),
            category_cloud: Vec::new(),
            recent_posts: Vec::new(),
        }
    }
}

/// Render the dashboard overview.
pub fn view<'a>(state: &'a DashboardState, locale: UiLocale) -> Element<'a, Message> {
    let header = text(state.title.clone()).size(24).color(Color::WHITE);

    let project_label = text(state.subtitle.clone())
        .size(14)
        .color(Color::from_rgb(0.6, 0.6, 0.7));

    let counts_row = row![
        stat_card(
            &t(locale, "dashboard.posts"),
            state.stats.total_posts.to_string(),
            {
                let mut details = vec![
                    format!(
                        "{} {}",
                        state.stats.published_count,
                        t(locale, "dashboard.published")
                    ),
                    format!(
                        "{} {}",
                        state.stats.draft_count,
                        t(locale, "dashboard.drafts")
                    ),
                ];
                if state.stats.archived_count > 0 {
                    details.push(format!(
                        "{} {}",
                        state.stats.archived_count,
                        t(locale, "dashboard.archived")
                    ));
                }
                details
            },
        ),
        stat_card(
            &t(locale, "dashboard.media"),
            state.stats.media_count.to_string(),
            vec![
                format!(
                    "{} {}",
                    state.stats.image_count,
                    t(locale, "dashboard.images")
                ),
                state.stats.total_media_size.clone(),
            ],
        ),
        stat_card(
            &t(locale, "dashboard.tags"),
            state.stats.tag_count.to_string(),
            vec![format!(
                "{} {}",
                state.stats.category_count,
                t(locale, "dashboard.categories")
            )],
        ),
    ]
    .spacing(16);

    let mut content = column![header, project_label, Space::with_height(16), counts_row].spacing(8);

    if !state.timeline.is_empty() {
        content = content
            .push(Space::with_height(20))
            .push(inputs::section_header(&t(locale, "dashboard.timeline")))
            .push(timeline_chart(&state.timeline, locale));
    }
    if !state.tag_cloud.is_empty() {
        content = content
            .push(Space::with_height(20))
            .push(inputs::section_header(&t(locale, "dashboard.tags")))
            .push(tag_cloud(&state.tag_cloud, locale));
    }
    if !state.category_cloud.is_empty() {
        content = content
            .push(Space::with_height(20))
            .push(inputs::section_header(&t(locale, "dashboard.categories")))
            .push(category_cloud(&state.category_cloud, locale));
    }
    if !state.recent_posts.is_empty() {
        content = content
            .push(Space::with_height(20))
            .push(inputs::section_header(&t(locale, "dashboard.recentPosts")))
            .push(recent_posts(&state.recent_posts, locale));
    }

    scrollable(container(content.padding(24).width(Length::Fill)))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn stat_card<'a>(label: &str, value: String, details: Vec<String>) -> Element<'a, Message> {
    let card_bg = Color::from_rgb(0.15, 0.16, 0.20);
    container(
        column(
            std::iter::once(text(value).size(28).color(Color::WHITE).into())
                .chain(std::iter::once(
                    text(label.to_string())
                        .size(12)
                        .color(Color::from_rgb(0.55, 0.58, 0.65))
                        .into(),
                ))
                .chain(details.into_iter().map(|detail| {
                    text(detail)
                        .size(12)
                        .color(Color::from_rgb(0.72, 0.74, 0.80))
                        .into()
                }))
                .collect::<Vec<Element<'a, Message>>>(),
        )
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

fn timeline_chart<'a>(
    months: &'a [DashboardTimelineMonth],
    _locale: UiLocale,
) -> Element<'a, Message> {
    let max_count = months
        .iter()
        .map(|month| month.count)
        .max()
        .unwrap_or(1)
        .max(1);
    row(months
        .iter()
        .map(|month| {
            let height = if month.count == 0 {
                8.0
            } else {
                24.0 + (month.count as f32 / max_count as f32) * 96.0
            };
            container(
                column![
                    text(month.count.to_string()).size(12).color(Color::WHITE),
                    container(Space::with_height(height))
                        .width(Length::Fill)
                        .style(|_: &Theme| container::Style {
                            background: Some(Background::Color(Color::from_rgb(0.25, 0.48, 0.80))),
                            border: iced::Border {
                                radius: 6.0.into(),
                                ..iced::Border::default()
                            },
                            ..container::Style::default()
                        }),
                    text(format!("{} {}", month.label, month.year))
                        .size(11)
                        .color(Color::from_rgb(0.7, 0.72, 0.78)),
                ]
                .spacing(6)
                .align_x(Alignment::Center),
            )
            .width(Length::FillPortion(1))
            .into()
        })
        .collect::<Vec<_>>())
    .spacing(10)
    .align_y(Alignment::End)
    .into()
}

fn tag_cloud<'a>(tags: &'a [DashboardTag], locale: UiLocale) -> Element<'a, Message> {
    let words = tags
        .iter()
        .map(|tag| {
            let bg = parse_color(tag.color.as_deref()).unwrap_or(Color::from_rgb(0.18, 0.21, 0.28));
            let fg = contrast_color(bg);
            tooltip(
                container(text(tag.name.clone()).size(tag.font_size).color(fg))
                    .padding([6, 10])
                    .style(move |_: &Theme| container::Style {
                        background: Some(Background::Color(bg)),
                        border: iced::Border {
                            radius: 999.0.into(),
                            ..iced::Border::default()
                        },
                        ..container::Style::default()
                    }),
                text(post_count_label(locale, tag.count)).size(12),
                tooltip::Position::Top,
            )
            .into()
        })
        .collect::<Vec<_>>();
    row(words).spacing(8).wrap().into()
}

fn category_cloud<'a>(
    categories: &'a [DashboardCategory],
    locale: UiLocale,
) -> Element<'a, Message> {
    let badges = categories
        .iter()
        .map(|category| {
            tooltip(
                container(
                    row![
                        text(category.name.clone()).size(13).color(Color::WHITE),
                        text(category.count.to_string())
                            .size(12)
                            .color(Color::from_rgb(0.72, 0.74, 0.80)),
                    ]
                    .spacing(8),
                )
                .padding([6, 10])
                .style(|_: &Theme| container::Style {
                    background: Some(Background::Color(Color::from_rgb(0.16, 0.18, 0.22))),
                    border: iced::Border {
                        radius: 999.0.into(),
                        ..iced::Border::default()
                    },
                    ..container::Style::default()
                }),
                text(post_count_label(locale, category.count)).size(12),
                tooltip::Position::Top,
            )
            .into()
        })
        .collect::<Vec<_>>();
    row(badges).spacing(8).wrap().into()
}

fn post_count_label(locale: UiLocale, count: usize) -> String {
    tw(
        locale,
        "dashboard.postCount",
        &[("count", &count.to_string())],
    )
}

fn recent_posts<'a>(posts: &'a [DashboardRecentPost], locale: UiLocale) -> Element<'a, Message> {
    column(
        posts
            .iter()
            .map(|post| {
                container(
                    row![
                        button(
                            column![
                                text(post.title.clone()).size(14).color(Color::WHITE),
                                text(post.date.clone())
                                    .size(12)
                                    .color(Color::from_rgb(0.6, 0.62, 0.68)),
                            ]
                            .spacing(4),
                        )
                        .on_press(Message::OpenTab(Tab {
                            id: post.post_id.clone(),
                            tab_type: TabType::Post,
                            title: post.title.clone(),
                            is_transient: true,
                            is_dirty: false,
                        }))
                        .width(Length::Fill),
                        status_badge(&post.status),
                        button(text(t(locale, "dashboard.pin")).size(12))
                            .on_press(Message::OpenTab(Tab {
                                id: post.post_id.clone(),
                                tab_type: TabType::Post,
                                title: post.title.clone(),
                                is_transient: false,
                                is_dirty: false,
                            }))
                            .padding([6, 10]),
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center),
                )
                .padding(12)
                .style(|_: &Theme| container::Style {
                    background: Some(Background::Color(Color::from_rgb(0.13, 0.14, 0.18))),
                    border: iced::Border {
                        radius: 8.0.into(),
                        ..iced::Border::default()
                    },
                    ..container::Style::default()
                })
                .into()
            })
            .collect::<Vec<_>>(),
    )
    .spacing(8)
    .into()
}

fn status_badge<'a>(status: &str) -> Element<'a, Message> {
    let bg = if status.eq_ignore_ascii_case("published") {
        Color::from_rgb(0.16, 0.42, 0.24)
    } else {
        Color::from_rgb(0.45, 0.33, 0.14)
    };
    container(text(status.to_string()).size(11).color(Color::WHITE))
        .padding([4, 8])
        .style(move |_: &Theme| container::Style {
            background: Some(Background::Color(bg)),
            border: iced::Border {
                radius: 999.0.into(),
                ..iced::Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn parse_color(value: Option<&str>) -> Option<Color> {
    let value = value?.trim_start_matches('#');
    if value.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&value[0..2], 16).ok()?;
    let green = u8::from_str_radix(&value[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&value[4..6], 16).ok()?;
    Some(Color::from_rgb8(red, green, blue))
}

fn contrast_color(background: Color) -> Color {
    let luma = 0.299 * background.r + 0.587 * background.g + 0.114 * background.b;
    if luma > 0.55 {
        Color::BLACK
    } else {
        Color::WHITE
    }
}
