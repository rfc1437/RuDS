use iced::widget::text::Shaping;
use iced::widget::{
    Container, button, checkbox, column, container, pick_list, row, text, text_editor, text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Shadow, Theme, Vector};

/// Standard form field label color.
pub const LABEL_COLOR: Color = rgb8(0xB5, 0xBA, 0xC4);
pub const SECTION_COLOR: Color = rgb8(0x9D, 0xA5, 0xB4);
const FIELD_BG: Color = rgb8(0x24, 0x24, 0x26);
const SURFACE_BG: Color = rgb8(0x25, 0x25, 0x26);
const BORDER_COLOR: Color = rgb8(0x3C, 0x3C, 0x3C);
const FOCUS_COLOR: Color = rgb8(0x00, 0x7F, 0xD4);
const DANGER_BG: Color = rgb8(0xB6, 0x23, 0x24);
const DANGER_HOVER: Color = rgb8(0xCF, 0x2F, 0x30);
const PRIMARY_BG: Color = rgb8(0x0E, 0x63, 0x9C);
const PRIMARY_HOVER: Color = rgb8(0x11, 0x77, 0xBB);
const SECONDARY_BG: Color = rgb8(0x2D, 0x2D, 0x30);
const SECONDARY_HOVER: Color = rgb8(0x3A, 0x3D, 0x41);

const fn rgb8(red: u8, green: u8, blue: u8) -> Color {
    Color {
        r: red as f32 / 255.0,
        g: green as f32 / 255.0,
        b: blue as f32 / 255.0,
        a: 1.0,
    }
}

/// Application-wide VS Code-like palette for native Iced controls.
pub fn app_theme() -> Theme {
    Theme::custom(
        "bDS".to_string(),
        iced::theme::Palette {
            background: Color::from_rgb8(0x1E, 0x1E, 0x1E),
            text: Color::from_rgb8(0xCC, 0xCC, 0xCC),
            primary: PRIMARY_BG,
            success: Color::from_rgb8(0x2E, 0x7D, 0x32),
            danger: DANGER_BG,
        },
    )
}

/// Opaque tooltip surface that remains legible over editor and sidebar content.
pub fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.20, 0.20, 0.24))),
        border: Border {
            color: Color::from_rgb(0.35, 0.35, 0.40),
            width: 1.0,
            radius: 6.0.into(),
        },
        text_color: Some(Color::WHITE),
        ..container::Style::default()
    }
}

pub fn field_style(_theme: &Theme, status: text_input::Status) -> text_input::Style {
    let border_color = match status {
        text_input::Status::Focused => FOCUS_COLOR,
        text_input::Status::Hovered => Color::from_rgb8(0x5A, 0x5A, 0x5A),
        _ => BORDER_COLOR,
    };
    text_input::Style {
        background: Background::Color(FIELD_BG),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 6.0.into(),
        },
        icon: SECTION_COLOR,
        placeholder: Color::from_rgb8(0x7D, 0x7D, 0x7D),
        value: Color::from_rgb8(0xE4, 0xE4, 0xE4),
        selection: Color::from_rgba8(0x26, 0x4F, 0x78, 0.8),
    }
}

/// Multi-line editor style matching the standard form fields.
pub fn text_editor_style(_theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let border_color = match status {
        text_editor::Status::Focused => FOCUS_COLOR,
        text_editor::Status::Hovered => Color::from_rgb8(0x5A, 0x5A, 0x5A),
        _ => BORDER_COLOR,
    };
    text_editor::Style {
        background: Background::Color(FIELD_BG),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 6.0.into(),
        },
        icon: SECTION_COLOR,
        placeholder: Color::from_rgb8(0x7D, 0x7D, 0x7D),
        value: Color::from_rgb8(0xE4, 0xE4, 0xE4),
        selection: Color::from_rgba8(0x26, 0x4F, 0x78, 0.8),
    }
}

fn select_style(_theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let border_color = match status {
        pick_list::Status::Hovered | pick_list::Status::Opened => FOCUS_COLOR,
        pick_list::Status::Active => BORDER_COLOR,
    };
    pick_list::Style {
        text_color: Color::from_rgb8(0xE4, 0xE4, 0xE4),
        placeholder_color: Color::from_rgb8(0x7D, 0x7D, 0x7D),
        handle_color: SECTION_COLOR,
        background: Background::Color(FIELD_BG),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 6.0.into(),
        },
    }
}

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
        text_input(placeholder, value)
            .on_input(on_change)
            .size(14)
            .padding([8, 10])
            .width(Length::Fill)
            .style(field_style),
    ]
    .spacing(6)
    .width(Length::Fill)
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
        pick_list(list, selected.cloned(), on_select)
            .padding([8, 10])
            .width(Length::Fill)
            .style(select_style),
    ]
    .spacing(6)
    .width(Length::Fill)
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
    text(label.to_string())
        .size(12)
        .color(SECTION_COLOR)
        .shaping(Shaping::Advanced)
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
                radius: 6.0.into(),
                ..iced::Border::default()
            },
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(Background::Color(PRIMARY_BG)),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 6.0.into(),
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
                radius: 6.0.into(),
                ..iced::Border::default()
            },
            ..button::Style::default()
        },
        _ => button::Style {
            background: Some(Background::Color(DANGER_BG)),
            text_color: Color::WHITE,
            border: iced::Border {
                radius: 6.0.into(),
                ..iced::Border::default()
            },
            ..button::Style::default()
        },
    }
}

/// Secondary editor action button style.
pub fn secondary_button(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => SECONDARY_HOVER,
        button::Status::Disabled => SECONDARY_BG.scale_alpha(0.45),
        _ => SECONDARY_BG,
    };
    button::Style {
        background: Some(Background::Color(background)),
        text_color: if matches!(status, button::Status::Disabled) {
            LABEL_COLOR.scale_alpha(0.55)
        } else {
            Color::from_rgb8(0xE4, 0xE4, 0xE4)
        },
        border: Border {
            color: BORDER_COLOR,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..button::Style::default()
    }
}

/// Hoverable disclosure row style for collapsible editor sections.
pub fn disclosure_button(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered => Some(Background::Color(SECONDARY_BG)),
            _ => None,
        },
        text_color: SECTION_COLOR,
        border: Border {
            radius: 6.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

/// Raised surface used to group editor metadata and controls.
pub fn card<'a, Message: 'a>(content: impl Into<Element<'a, Message>>) -> Container<'a, Message> {
    container(content)
        .padding(16)
        .width(Length::Fill)
        .style(|_: &Theme| container::Style {
            background: Some(Background::Color(SURFACE_BG)),
            border: Border {
                color: Color::from_rgb8(0x31, 0x31, 0x33),
                width: 1.0,
                radius: 10.0.into(),
            },
            shadow: Shadow {
                color: Color::from_rgba(0.0, 0.0, 0.0, 0.28),
                offset: Vector::new(0.0, 4.0),
                blur_radius: 16.0,
            },
            ..container::Style::default()
        })
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

    container(
        row![left_row, right_row]
            .spacing(10)
            .align_y(Alignment::Center)
            .width(Length::Fill),
    )
    .padding(10)
    .width(Length::Fill)
    .style(|_: &Theme| container::Style {
        background: Some(Background::Color(SURFACE_BG)),
        border: Border {
            color: Color::from_rgb8(0x31, 0x31, 0x33),
            width: 1.0,
            radius: 10.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.2),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 10.0,
        },
        ..container::Style::default()
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_controls_have_distinct_hover_and_focus_states() {
        let theme = app_theme();
        assert_ne!(
            primary_button(&theme, button::Status::Active).background,
            primary_button(&theme, button::Status::Hovered).background
        );
        assert_ne!(
            field_style(&theme, text_input::Status::Active).border.color,
            field_style(&theme, text_input::Status::Focused)
                .border
                .color
        );
        assert_ne!(
            text_editor_style(&theme, text_editor::Status::Active)
                .border
                .color,
            text_editor_style(&theme, text_editor::Status::Focused)
                .border
                .color
        );
    }

    #[test]
    fn tooltip_surface_is_opaque_rounded_and_bordered() {
        let style = tooltip_style(&app_theme());
        assert!(matches!(
            style.background,
            Some(Background::Color(color)) if color.a == 1.0
        ));
        assert_eq!(style.border.width, 1.0);
        assert_eq!(style.border.radius.top_left, 6.0);
    }
}
