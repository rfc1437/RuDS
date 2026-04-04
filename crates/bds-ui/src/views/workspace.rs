use iced::widget::{button, column, container, row, stack, text, Space};
use iced::widget::text::Shaping;
use iced::{Alignment, Background, Color, Element, Length, Padding, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::state::navigation::{OutputEntry, PanelTab, SidebarView, TaskSnapshot};
use crate::state::tabs::Tab;
use crate::views::{activity_bar, panel, sidebar, status_bar, tab_bar, welcome};

/// Main content area background.
fn content_bg(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.11, 0.11, 0.14))),
        ..container::Style::default()
    }
}

/// Horizontal separator line between regions.
fn separator_v() -> iced::widget::Container<'static, Message> {
    container(Space::new(0, 0))
        .width(Length::Fixed(1.0))
        .height(Length::Fill)
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(Color::from_rgb(0.25, 0.25, 0.30))),
            ..container::Style::default()
        })
}

/// Horizontal line separator (full width).
fn separator_h() -> iced::widget::Container<'static, Message> {
    container(Space::new(0, 0))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(Color::from_rgb(0.25, 0.25, 0.30))),
            ..container::Style::default()
        })
}

/// Compose the full workspace layout.
pub fn view(
    // Navigation
    sidebar_view: SidebarView,
    sidebar_visible: bool,
    // Tabs
    tabs: &[Tab],
    active_tab: Option<&str>,
    // Panel
    panel_visible: bool,
    panel_tab: PanelTab,
    task_snapshots: &[TaskSnapshot],
    output_entries: &[OutputEntry],
    // Status bar
    active_project_name: Option<&str>,
    post_count: usize,
    media_count: usize,
    offline_mode: bool,
    locale_dropdown_open: bool,
    // i18n
    locale: UiLocale,
) -> Element<'static, Message> {
    // Activity bar (leftmost column)
    let activity = activity_bar::view(sidebar_view, locale);

    // Tab bar
    let tabs_el = tab_bar::view(tabs, active_tab);

    // Content area
    let content_area = welcome::view(locale);

    // Right column: tab bar + content + panel
    let mut right_col = column![tabs_el, content_area];
    if panel_visible {
        right_col = right_col.push(separator_h());
        right_col = right_col.push(panel::view(panel_tab, task_snapshots, output_entries, locale));
    }
    let right = container(right_col.width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(content_bg);

    // Main row: activity bar | separator | sidebar? | separator | right column
    let mut main_row = row![activity];

    if sidebar_visible {
        main_row = main_row.push(sidebar::view(sidebar_view, locale));
        main_row = main_row.push(separator_v());
    }

    main_row = main_row.push(right);
    let main_row = main_row.height(Length::Fill);

    // Status bar at bottom
    let status = status_bar::view(
        active_project_name,
        post_count,
        media_count,
        locale,
        offline_mode,
        task_snapshots,
    );

    let base_layout: Element<'static, Message> = column![main_row, separator_h(), status]
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    if locale_dropdown_open {
        // Build the dropdown menu overlay
        let items: Vec<Element<'static, Message>> = UiLocale::all()
            .iter()
            .map(|&l| {
                let flag_text = text(l.flag_emoji())
                    .size(16)
                    .shaping(Shaping::Advanced);

                button(flag_text)
                    .on_press(Message::SetUiLocale(l))
                    .padding([4, 8])
                    .style(status_bar::dropdown_item)
                    .into()
            })
            .collect();

        let dropdown_menu = container(
            iced::widget::Column::with_children(items).spacing(2).padding(4),
        )
        .style(status_bar::dropdown_bg);

        // Position the dropdown at bottom-right, above the status bar (24px)
        let overlay: Element<'static, Message> = container(
            container(
                row![
                    Space::with_width(Length::Fill),
                    dropdown_menu,
                    // Offset from right edge to align near the flag trigger
                    Space::with_width(Length::Fixed(40.0)),
                ]
            )
            .width(Length::Fill)
            .align_y(Alignment::End)
            .padding(Padding { top: 0.0, right: 0.0, bottom: 25.0, left: 0.0 }) // above status bar
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .align_y(Alignment::End)
        .into();

        stack![base_layout, overlay]
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    } else {
        base_layout
    }
}
