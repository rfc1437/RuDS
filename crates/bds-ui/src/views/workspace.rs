use iced::widget::{column, row};
use iced::{Element, Length};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::state::navigation::{OutputEntry, PanelTab, SidebarView, TaskSnapshot};
use crate::state::tabs::Tab;
use crate::views::{activity_bar, panel, sidebar, status_bar, tab_bar, welcome};

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
    // i18n
    locale: UiLocale,
) -> Element<'static, Message> {
    // Activity bar (leftmost column)
    let activity = activity_bar::view(sidebar_view, locale);

    // Sidebar (conditionally visible)
    let sidebar_el: Option<Element<'static, Message>> = if sidebar_visible {
        Some(sidebar::view(sidebar_view, locale))
    } else {
        None
    };

    // Tab bar
    let tabs_el = tab_bar::view(tabs, active_tab);

    // Content area
    let content_area = welcome::view(locale);

    // Panel (conditionally visible)
    let panel_el: Option<Element<'static, Message>> = if panel_visible {
        Some(panel::view(panel_tab, task_snapshots, output_entries, locale))
    } else {
        None
    };

    // Right column: tab bar + content + panel
    let mut right_col = column![tabs_el, content_area];
    if let Some(p) = panel_el {
        right_col = right_col.push(p);
    }
    let right = right_col.width(Length::Fill).height(Length::Fill);

    // Main row: activity bar + sidebar? + right column
    let mut main_row = row![activity];
    if let Some(sb) = sidebar_el {
        main_row = main_row.push(sb);
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

    column![main_row, status]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
