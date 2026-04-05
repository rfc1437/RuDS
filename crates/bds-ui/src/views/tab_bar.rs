use iced::widget::{button, container, row, scrollable, text, tooltip, Space};
use iced::widget::scrollable::Direction;
use iced::widget::text::Shaping;
use iced::widget::tooltip::Position;
use iced::{Background, Border, Color, Element, Font, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::t;
use crate::state::tabs::Tab;

/// tabs.allium config: tab_min_width=100, tab_max_width=160
/// In a scrollable tab bar, tabs use the max width since they never need to shrink.
const TAB_WIDTH: f32 = 160.0;
const CHAT_TITLE_MAX_LEN: usize = 18;

/// Maximum characters for tab titles.
/// TAB_WIDTH (160) minus padding (16) minus close button + spacing (~28)
/// leaves ~116px.  At ~6.8px per char (12px proportional font) ≈ 17 chars.
const TAB_TITLE_MAX_LEN: usize = 17;

/// Tab bar background.
fn bar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.14, 0.14, 0.18))),
        border: Border {
            color: Color::from_rgb(0.25, 0.25, 0.30),
            width: 0.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}

/// Active tab style.
fn tab_active(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(Color::from_rgb(0.11, 0.11, 0.14))),
        text_color: Color::WHITE,
        border: Border {
            color: Color::from_rgb(0.30, 0.55, 0.90),
            width: 0.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

/// Inactive tab style.
fn tab_inactive(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.18, 0.18, 0.22),
        _ => Color::from_rgb(0.14, 0.14, 0.18),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.60, 0.60, 0.65),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

/// Close button on tab.
fn close_style(_theme: &Theme, status: button::Status) -> button::Style {
    let color = match status {
        button::Status::Hovered => Color::WHITE,
        _ => Color::from_rgb(0.45, 0.45, 0.50),
    };
    button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: color,
        border: Border::default(),
        ..button::Style::default()
    }
}

/// Tooltip style.
fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.20, 0.20, 0.24))),
        border: Border {
            color: Color::from_rgb(0.35, 0.35, 0.40),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    }
}

/// Truncate chat title per tabs.allium: chat_title_max_length = 18.
fn truncate_chat_title(title: &str) -> String {
    if title.chars().count() > CHAT_TITLE_MAX_LEN {
        let truncated: String = title.chars().take(CHAT_TITLE_MAX_LEN).collect();
        format!("{truncated}...")
    } else {
        title.to_string()
    }
}

/// Truncate a tab title to fit within the tab's available text area.
/// Uses "…" (Unicode ellipsis) as the truncation indicator.
fn truncate_tab_title(title: &str) -> String {
    if title.chars().count() > TAB_TITLE_MAX_LEN {
        let truncated: String = title.chars().take(TAB_TITLE_MAX_LEN.saturating_sub(1)).collect();
        format!("{truncated}\u{2026}")
    } else {
        title.to_string()
    }
}

/// Build tooltip text per tabs.allium:
/// Base: tab title. If transient: append " (Preview)". If dirty: append " * Modified".
fn build_tooltip_text(tab: &Tab, locale: UiLocale) -> String {
    let mut tip = tab.title.clone();
    if tab.is_transient {
        tip.push_str(" (");
        tip.push_str(&t(locale, "tabBar.preview"));
        tip.push(')');
    }
    if tab.is_dirty {
        tip.push_str(" * ");
        tip.push_str(&t(locale, "tabBar.modified"));
    }
    tip
}

pub fn view(
    tabs: &[Tab],
    active_tab: Option<&str>,
    locale: UiLocale,
) -> Element<'static, Message> {
    // Per tabs.allium: "Hidden when no tabs exist."
    if tabs.is_empty() {
        return Space::with_height(0).into();
    }

    let tab_buttons: Vec<Element<'static, Message>> = tabs
        .iter()
        .map(|tab| {
            let is_active = active_tab == Some(tab.id.as_str());
            let tab_id = tab.id.clone();
            let close_id = tab.id.clone();

            // Per tabs.allium: chat titles are JS-truncated to 18 chars + "..."
            // All other titles are truncated to fit the tab's text area.
            let display_title = if tab.tab_type == crate::state::tabs::TabType::Chat {
                truncate_chat_title(&tab.title)
            } else {
                truncate_tab_title(&tab.title)
            };

            // Per tabs.allium: transient tabs show italic title
            let title_label = if tab.is_transient {
                text(display_title)
                    .size(12)
                    .shaping(Shaping::Advanced)
                    .font(Font { style: iced::font::Style::Italic, ..Font::DEFAULT })
                    .wrapping(iced::widget::text::Wrapping::None)
            } else {
                text(display_title)
                    .size(12)
                    .shaping(Shaping::Advanced)
                    .wrapping(iced::widget::text::Wrapping::None)
            };

            // Per tabs.allium DirtyIndicator: dot for dirty post tabs
            let dirty_indicator = if tab.is_dirty {
                text(" \u{25CF}").size(10).shaping(Shaping::Advanced)
                    .color(Color::from_rgb(0.90, 0.70, 0.30))
            } else {
                text("").size(10)
            };

            // Title + dirty indicator clipped to enforce ellipsis-like truncation
            let title_area = container(
                row![title_label, dirty_indicator]
                    .spacing(2)
                    .align_y(iced::Alignment::Center)
            )
            .width(Length::Fill)
            .clip(true);

            let label = row![
                title_area,
                button(text("\u{2715}").size(10).shaping(Shaping::Advanced))
                    .on_press(Message::CloseTab(close_id))
                    .padding(2)
                    .style(close_style),
            ]
            .spacing(4)
            .align_y(iced::Alignment::Center);

            // tabs.allium: tab_min_width=100, tab_max_width=160
            let tab_btn = button(label)
                .on_press(Message::SelectTab(tab_id))
                .padding([6, 8])
                .width(Length::Fixed(TAB_WIDTH))
                .style(if is_active { tab_active } else { tab_inactive });

            // tabs.allium tooltip: title + "(Preview)" if transient + "* Modified" if dirty
            let tooltip_text = build_tooltip_text(tab, locale);
            let tip: Element<'static, Message> = tooltip(
                tab_btn,
                text(tooltip_text).size(11).shaping(Shaping::Advanced),
                Position::Bottom,
            )
            .gap(4)
            .style(tooltip_style)
            .into();

            tip
        })
        .collect();

    // tabs.allium: horizontal strip with overflow scroll
    let tab_row = iced::widget::Row::with_children(tab_buttons)
        .spacing(1)
        .height(Length::Fixed(35.0));

    let scrollable_tabs = scrollable(tab_row)
        .direction(Direction::Horizontal(
            scrollable::Scrollbar::new().width(0).scroller_width(0),
        ))
        .width(Length::Fill)
        .height(Length::Fixed(35.0));

    container(scrollable_tabs)
        .width(Length::Fill)
        .height(Length::Fixed(35.0))
        .style(bar_style)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_chat_title_short() {
        assert_eq!(truncate_chat_title("Hello"), "Hello");
    }

    #[test]
    fn truncate_chat_title_exact_18() {
        let title: String = "a".repeat(18);
        assert_eq!(truncate_chat_title(&title), title);
    }

    #[test]
    fn truncate_chat_title_over_18() {
        let title: String = "a".repeat(25);
        let expected = format!("{}...", "a".repeat(18));
        assert_eq!(truncate_chat_title(&title), expected);
    }

    #[test]
    fn truncate_tab_title_short() {
        assert_eq!(truncate_tab_title("Settings"), "Settings");
    }

    #[test]
    fn truncate_tab_title_exact_limit() {
        let title: String = "a".repeat(TAB_TITLE_MAX_LEN);
        assert_eq!(truncate_tab_title(&title), title);
    }

    #[test]
    fn truncate_tab_title_over_limit() {
        let title: String = "a".repeat(30);
        let expected = format!("{}\u{2026}", "a".repeat(TAB_TITLE_MAX_LEN - 1));
        assert_eq!(truncate_tab_title(&title), expected);
    }
}
