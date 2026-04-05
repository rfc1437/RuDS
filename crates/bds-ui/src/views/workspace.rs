use iced::widget::{button, column, container, row, stack, text, Space};
use iced::widget::text::Shaping;
use iced::{Alignment, Background, Color, Element, Length, Padding, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::{Media, Post, Project};

use crate::app::Message;
use crate::state::navigation::{OutputEntry, PanelTab, SidebarView, TaskSnapshot};
use crate::state::tabs::{Tab, TabType};
use crate::state::toast::Toast;
use crate::views::{activity_bar, panel, project_selector, sidebar, status_bar, tab_bar, toast, welcome};

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
    // Sidebar data
    sidebar_posts: &[Post],
    sidebar_media: &[Media],
    // Status bar
    active_project_name: Option<&str>,
    projects: &[Project],
    active_project_id: Option<&str>,
    post_count: usize,
    media_count: usize,
    offline_mode: bool,
    locale_dropdown_open: bool,
    project_dropdown_open: bool,
    theme_badge: &str,
    // i18n
    locale: UiLocale,
    // Toasts
    toasts: &[Toast],
) -> Element<'static, Message> {
    // Activity bar (leftmost column)
    let activity = activity_bar::view(sidebar_view, sidebar_visible, locale);

    // Tab bar
    let tabs_el = tab_bar::view(tabs, active_tab);

    // Content area
    let content_area = welcome::view(locale);

    // Right column: tab bar + content + panel
    let mut right_col = column![tabs_el, content_area];
    if panel_visible {
        // Determine active tab type for panel tab availability (per layout.allium PanelTabAvailability)
        let active_tab_type = active_tab.and_then(|id| tabs.iter().find(|t| t.id == id)).map(|t| &t.tab_type);
        let active_tab_is_post = active_tab_type == Some(&TabType::Post);
        let active_tab_is_post_or_media = active_tab_is_post || active_tab_type == Some(&TabType::Media);

        right_col = right_col.push(separator_h());
        right_col = right_col.push(panel::view(
            panel_tab,
            task_snapshots,
            output_entries,
            locale,
            active_tab_is_post,
            active_tab_is_post_or_media,
        ));
    }
    let right = container(right_col.width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(content_bg);

    // Main row: activity bar | separator | sidebar? | separator | right column
    let mut main_row = row![activity];

    if sidebar_visible {
        main_row = main_row.push(sidebar::view(sidebar_view, sidebar_posts, sidebar_media, locale));
        main_row = main_row.push(separator_v());
    }

    main_row = main_row.push(right);
    let main_row = main_row.height(Length::Fill);

    // Status bar at bottom — determine active post status for status dot
    let active_post_status: Option<String> = active_tab.and_then(|id| {
        tabs.iter().find(|t| t.id == id && t.tab_type == TabType::Post)
    }).and_then(|tab| {
        sidebar_posts.iter().find(|p| p.id == tab.id).map(|p| {
            match p.status {
                bds_core::model::PostStatus::Draft => "draft".to_string(),
                bds_core::model::PostStatus::Published => "published".to_string(),
                bds_core::model::PostStatus::Archived => "archived".to_string(),
            }
        })
    });

    let status = status_bar::view(
        active_project_name,
        post_count,
        media_count,
        locale,
        offline_mode,
        task_snapshots,
        theme_badge,
        active_post_status.as_deref(),
    );

    let base_layout: Element<'static, Message> = column![main_row, separator_h(), status]
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    // Overlay: either locale dropdown or project dropdown (mutually exclusive)
    let overlay: Option<Element<'static, Message>> = if locale_dropdown_open {
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

        // Position at bottom-right, above status bar
        Some(
            container(
                container(
                    row![
                        Space::with_width(Length::Fill),
                        dropdown_menu,
                        Space::with_width(Length::Fixed(40.0)),
                    ]
                )
                .width(Length::Fill)
                .align_y(Alignment::End)
                .padding(Padding { top: 0.0, right: 0.0, bottom: 25.0, left: 0.0 })
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End)
            .into()
        )
    } else if project_dropdown_open {
        let dropdown = project_selector::view(projects, active_project_id, locale);

        // Position at bottom-left, above status bar
        Some(
            container(
                container(
                    row![
                        Space::with_width(Length::Fixed(8.0)),
                        dropdown,
                        Space::with_width(Length::Fill),
                    ]
                )
                .width(Length::Fill)
                .align_y(Alignment::End)
                .padding(Padding { top: 0.0, right: 0.0, bottom: 25.0, left: 0.0 })
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End)
            .into()
        )
    } else {
        None
    };

    // Collect overlays: dropdowns and toasts
    let mut overlays: Vec<Element<'static, Message>> = Vec::new();

    if let Some(toast_overlay) = toast::view(toasts) {
        overlays.push(toast_overlay);
    }

    if let Some(overlay) = overlay {
        overlays.push(overlay);
    }

    if overlays.is_empty() {
        base_layout
    } else {
        let mut layers = vec![base_layout];
        layers.extend(overlays);
        stack(layers)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
}
