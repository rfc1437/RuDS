use iced::widget::text::Shaping;
use iced::widget::{Space, button, checkbox, column, container, pick_list, row, text, text_input};
use iced::{Alignment, Background, Color, Element, Length, Theme};

/// Standard form field label color.
pub const LABEL_COLOR: Color = Color::from_rgb(0.65, 0.68, 0.75);
const _FIELD_BG: Color = Color::from_rgb(0.15, 0.16, 0.20);
pub const SECTION_COLOR: Color = Color::from_rgb(0.50, 0.52, 0.58);
const DANGER_BG: Color = Color::from_rgb(0.60, 0.15, 0.15);
const DANGER_HOVER: Color = Color::from_rgb(0.70, 0.20, 0.20);
const PRIMARY_BG: Color = Color::from_rgb(0.20, 0.40, 0.70);
const PRIMARY_HOVER: Color = Color::from_rgb(0.25, 0.48, 0.80);

/// A labeled text input field.
pub fn labeled_input<'a, Message: Clone + 'a>(
    label: &str,
    placeholder: &str,
    value: &str,
    on_change: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    column![
        text(label.to_string())
            .size(12)
            .color(LABEL_COLOR)
            .shaping(Shaping::Advanced),
        text_input(placeholder, value).on_input(on_change).size(14),
    ]
    .spacing(4)
    .into()
}

/// A labeled select/dropdown field.
pub fn labeled_select<'a, T, Message>(
    label: &str,
    options: &[T],
    selected: Option<&T>,
    on_select: impl Fn(T) -> Message + 'a,
) -> Element<'a, Message>
where
    T: ToString + PartialEq + Clone + 'a,
    Message: Clone + 'a,
{
    let list: Vec<T> = options.to_vec();
    column![
        text(label.to_string())
            .size(12)
            .color(LABEL_COLOR)
            .shaping(Shaping::Advanced),
        pick_list(list, selected.cloned(), on_select),
    ]
    .spacing(4)
    .into()
}

/// A labeled checkbox.
pub fn labeled_checkbox<'a, Message: Clone + 'a>(
    label: &str,
    is_checked: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
) -> Element<'a, Message> {
    checkbox(label, is_checked)
        .on_toggle(on_toggle)
        .size(16)
        .text_size(14)
        .into()
}

/// A section header with optional separator line.
pub fn section_header<'a, Message: 'a>(label: &str) -> Element<'a, Message> {
    column![
        text(label.to_string())
            .size(11)
            .color(SECTION_COLOR)
            .shaping(Shaping::Advanced),
        container(Space::new(0, 0))
            .width(Length::Fill)
            .height(Length::Fixed(1.0))
            .style(|_: &Theme| container::Style {
                background: Some(Background::Color(Color::from_rgb(0.25, 0.25, 0.30))),
                ..container::Style::default()
            }),
    ]
    .spacing(4)
    .into()
}

/// Primary action button style.
pub fn primary_button(theme: &Theme, status: button::Status) -> button::Style {
    let _ = theme;
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(PRIMARY_HOVER)),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 4.0.into(),
                ..iced::Border::default()
            },
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(Background::Color(PRIMARY_BG)),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 4.0.into(),
                ..iced::Border::default()
            },
            ..button::Style::default()
        },
    }
}

/// Danger action button style (delete, destructive).
pub fn danger_button(theme: &Theme, status: button::Status) -> button::Style {
    let _ = theme;
    match status {
        button::Status::Hovered => button::Style {
            background: Some(Background::Color(DANGER_HOVER)),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 4.0.into(),
                ..iced::Border::default()
            },
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(Background::Color(DANGER_BG)),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 4.0.into(),
                ..iced::Border::default()
            },
            ..button::Style::default()
        },
    }
}

/// Toolbar-style row with right-aligned actions.
pub fn toolbar<'a, Message: 'a>(
    left: Vec<Element<'a, Message>>,
    right: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let left_row = container(
        iced::widget::Row::with_children(left)
            .spacing(8)
            .align_y(Alignment::Center),
    )
    .clip(true)
    .width(Length::Fill);
    let right_row = iced::widget::Row::with_children(right)
        .spacing(8)
        .align_y(Alignment::Center)
        .wrap();

    row![left_row, right_row]
        .padding(8)
        .spacing(8)
        .align_y(Alignment::Center)
        .width(Length::Fill)
        .into()
}

/// Date display (read-only, locale-formatted).
pub fn date_label<'a, Message: 'a>(label: &str, timestamp_ms: i64) -> Element<'a, Message> {
    let date_str = format_timestamp(timestamp_ms);
    row![
        text(label.to_string())
            .size(12)
            .color(LABEL_COLOR)
            .shaping(Shaping::Advanced),
        text(date_str)
            .size(12)
            .color(Color::from_rgb(0.55, 0.58, 0.65))
            .shaping(Shaping::Advanced),
    ]
    .spacing(8)
    .into()
}

/// Format a Unix timestamp (ms) to a readable date string.
fn format_timestamp(ms: i64) -> String {
    let secs = ms / 1000;
    let (y, m, d) = bds_core::util::timestamp::year_month_day_from_unix_ms(ms);
    let h = ((secs % 86400) / 3600) as u32;
    let min = ((secs % 3600) / 60) as u32;
    format!("{y}-{m:02}-{d:02} {h:02}:{min:02}")
}
