use std::collections::HashMap;
use std::path::{Path, PathBuf};

use iced::widget::text::Shaping;
use iced::widget::{button, column, container, mouse_area, row, stack, text, Space};
use iced::{Alignment, Background, Color, Element, Length, Padding, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::{Media, Post, Project, Script, Template};

use crate::app::Message;
use crate::state::navigation::{OutputEntry, PanelTab, SidebarView, TaskSnapshot};
use crate::state::sidebar_filter::{MediaFilter, PostFilter};
use crate::state::tabs::{Tab, TabType};
use crate::state::toast::Toast;
use crate::views::{
    activity_bar,
    dashboard::DashboardState,
    media_editor::{self, MediaEditorState},
    modal, panel,
    post_editor::{self, PostEditorState},
    project_selector,
    script_editor::{self, ScriptEditorState},
    settings_view::{self, SettingsViewState},
    sidebar, status_bar, tab_bar,
    tags_view::{self, TagsViewState},
    template_editor::{self, TemplateEditorState},
    toast, welcome,
};

/// Main content area background.
fn content_bg(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.11, 0.11, 0.14))),
        ..container::Style::default()
    }
}

/// Sidebar resize drag handle style.
fn drag_handle_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.25, 0.25, 0.30))),
        ..container::Style::default()
    }
}

/// Horizontal line separator (full width).
fn separator_h<'a>() -> Element<'a, Message> {
    container(Space::new(0, 0))
        .width(Length::Fill)
        .height(Length::Fixed(1.0))
        .style(|_theme: &Theme| container::Style {
            background: Some(Background::Color(Color::from_rgb(0.25, 0.25, 0.30))),
            ..container::Style::default()
        })
        .into()
}

/// Compose the full workspace layout.
pub fn view<'a>(
    // Navigation
    sidebar_view: SidebarView,
    sidebar_visible: bool,
    sidebar_width: f32,
    // Tabs
    tabs: &'a [Tab],
    active_tab: Option<&'a str>,
    // Panel
    panel_visible: bool,
    panel_tab: PanelTab,
    task_snapshots: &'a [TaskSnapshot],
    output_entries: &'a [OutputEntry],
    // Sidebar data
    sidebar_posts: &'a [Post],
    sidebar_media: &'a [Media],
    sidebar_scripts: &'a [Script],
    sidebar_templates: &'a [Template],
    // Sidebar filters
    post_filter: &'a PostFilter,
    media_filter: &'a MediaFilter,
    // Pre-resolved media thumbnail paths
    sidebar_media_thumbs: &'a HashMap<String, Option<PathBuf>>,
    // Pagination
    sidebar_posts_has_more: bool,
    sidebar_media_has_more: bool,
    // Status bar
    active_project_name: Option<&'a str>,
    projects: &'a [Project],
    active_project_id: Option<&'a str>,
    post_count: usize,
    media_count: usize,
    offline_mode: bool,
    locale_dropdown_open: bool,
    project_dropdown_open: bool,
    theme_badge: &'a str,
    // i18n
    locale: UiLocale,
    // Toasts
    toasts: &'a [Toast],
    // Modal
    active_modal: Option<modal::ModalState>,
    // Data directory (for thumbnail paths)
    data_dir: Option<&'a Path>,
    // Editor states
    post_editors: &'a HashMap<String, PostEditorState>,
    media_editors: &'a HashMap<String, MediaEditorState>,
    template_editors: &'a HashMap<String, TemplateEditorState>,
    script_editors: &'a HashMap<String, ScriptEditorState>,
    tags_view_state: Option<&'a TagsViewState>,
    settings_state: Option<&'a SettingsViewState>,
    dashboard_state: Option<&'a DashboardState>,
) -> Element<'a, Message> {
    // Activity bar (leftmost column)
    let activity = activity_bar::view(sidebar_view, sidebar_visible, locale);

    // Tab bar
    let tabs_el = tab_bar::view(tabs, active_tab, locale);

    // Content area — route based on active tab type
    let content_area = route_content_area(
        tabs,
        active_tab,
        locale,
        data_dir,
        post_editors,
        media_editors,
        template_editors,
        script_editors,
        tags_view_state,
        settings_state,
        dashboard_state,
    );

    // Right column: tab bar + content + panel
    let mut right_col = column![tabs_el, content_area];
    if panel_visible {
        // Determine active tab type for panel tab availability (per layout.allium PanelTabAvailability)
        let active_tab_type = active_tab
            .and_then(|id| tabs.iter().find(|t| t.id == id))
            .map(|t| &t.tab_type);
        let active_tab_is_post = active_tab_type == Some(&TabType::Post);
        let active_tab_is_post_or_media =
            active_tab_is_post || active_tab_type == Some(&TabType::Media);

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

    // Main row: activity bar | sidebar? | drag handle | right column
    let mut main_row = row![activity];

    if sidebar_visible {
        main_row = main_row.push(sidebar::view(
            sidebar_view,
            sidebar_posts,
            sidebar_media,
            sidebar_scripts,
            sidebar_templates,
            post_filter,
            media_filter,
            sidebar_media_thumbs,
            sidebar_posts_has_more,
            sidebar_media_has_more,
            sidebar_width,
            active_tab,
            locale,
            data_dir,
        ));

        // Resize drag handle: 4px wide strip between sidebar and content
        let handle = container(Space::new(0, 0))
            .width(Length::Fixed(4.0))
            .height(Length::Fill)
            .style(drag_handle_style);

        // Only on_press here; move/release are captured by a global
        // subscription so dragging works even when the cursor leaves
        // the narrow 4px handle strip.
        let handle_hover = mouse_area(handle)
            .on_press(Message::SidebarResizeStart)
            .interaction(iced::mouse::Interaction::ResizingHorizontally);

        main_row = main_row.push(handle_hover);
    }

    main_row = main_row.push(right);
    let main_row = main_row.height(Length::Fill);

    // Status bar at bottom — determine active post status for status dot
    let active_post_status: Option<String> = active_tab
        .and_then(|id| {
            tabs.iter()
                .find(|t| t.id == id && t.tab_type == TabType::Post)
        })
        .and_then(|tab| {
            sidebar_posts
                .iter()
                .find(|p| p.id == tab.id)
                .map(|p| match p.status {
                    bds_core::model::PostStatus::Draft => "draft".to_string(),
                    bds_core::model::PostStatus::Published => "published".to_string(),
                    bds_core::model::PostStatus::Archived => "archived".to_string(),
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

    let base_layout: Element<'a, Message> = column![main_row, separator_h(), status]
        .width(Length::Fill)
        .height(Length::Fill)
        .into();

    // Overlay: either locale dropdown or project dropdown (mutually exclusive)
    let overlay: Option<Element<'a, Message>> = if locale_dropdown_open {
        let items: Vec<Element<'a, Message>> = UiLocale::all()
            .iter()
            .map(|&l| {
                let flag_text = text(l.flag_emoji()).size(16).shaping(Shaping::Advanced);

                button(flag_text)
                    .on_press(Message::SetUiLocale(l))
                    .padding([4, 8])
                    .style(status_bar::dropdown_item)
                    .into()
            })
            .collect();

        let dropdown_menu = container(
            iced::widget::Column::with_children(items)
                .spacing(2)
                .padding(4),
        )
        .style(status_bar::dropdown_bg);

        // Position at bottom-right, above status bar
        Some(
            container(
                container(row![
                    Space::with_width(Length::Fill),
                    dropdown_menu,
                    Space::with_width(Length::Fixed(40.0)),
                ])
                .width(Length::Fill)
                .align_y(Alignment::End)
                .padding(Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 25.0,
                    left: 0.0,
                }),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End)
            .into(),
        )
    } else if project_dropdown_open {
        let dropdown = project_selector::view(projects, active_project_id, locale);

        // Position at bottom-left, above status bar
        Some(
            container(
                container(row![
                    Space::with_width(Length::Fixed(8.0)),
                    dropdown,
                    Space::with_width(Length::Fill),
                ])
                .width(Length::Fill)
                .align_y(Alignment::End)
                .padding(Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 25.0,
                    left: 0.0,
                }),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .align_y(Alignment::End)
            .into(),
        )
    } else {
        None
    };

    // Collect overlays: dropdowns and toasts
    let mut overlays: Vec<Element<'a, Message>> = Vec::new();

    if let Some(toast_overlay) = toast::view(toasts) {
        overlays.push(toast_overlay);
    }

    if let Some(overlay) = overlay {
        overlays.push(overlay);
    }

    // Modal overlay (highest z-index)
    if let Some(modal_state) = active_modal {
        overlays.push(modal::view(modal_state, locale, data_dir));
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

/// Route the content area based on the active tab type.
fn route_content_area<'a>(
    tabs: &'a [Tab],
    active_tab: Option<&'a str>,
    locale: UiLocale,
    data_dir: Option<&'a Path>,
    post_editors: &'a HashMap<String, PostEditorState>,
    media_editors: &'a HashMap<String, MediaEditorState>,
    template_editors: &'a HashMap<String, TemplateEditorState>,
    script_editors: &'a HashMap<String, ScriptEditorState>,
    tags_view_state: Option<&'a TagsViewState>,
    settings_state: Option<&'a SettingsViewState>,
    _dashboard_state: Option<&'a DashboardState>,
) -> Element<'a, Message> {
    let Some(tab_id) = active_tab else {
        return welcome::view(locale);
    };
    let Some(tab) = tabs.iter().find(|t| t.id == tab_id) else {
        return welcome::view(locale);
    };

    match tab.tab_type {
        TabType::Post => {
            if let Some(state) = post_editors.get(tab_id) {
                let wrap = settings_state.map(|s| s.wrap_long_lines).unwrap_or(true);
                post_editor::view(state, locale, wrap)
            } else {
                loading_view(locale)
            }
        }
        TabType::Media => {
            if let Some(state) = media_editors.get(tab_id) {
                media_editor::view(state, locale, data_dir)
            } else {
                loading_view(locale)
            }
        }
        TabType::Templates => {
            if let Some(state) = template_editors.get(tab_id) {
                template_editor::view(state, locale)
            } else {
                loading_view(locale)
            }
        }
        TabType::Scripts => {
            if let Some(state) = script_editors.get(tab_id) {
                script_editor::view(state, locale)
            } else {
                loading_view(locale)
            }
        }
        TabType::Tags => {
            if let Some(state) = tags_view_state {
                tags_view::view(state, locale)
            } else {
                loading_view(locale)
            }
        }
        TabType::Settings => {
            if let Some(state) = settings_state {
                settings_view::view(state, locale)
            } else {
                loading_view(locale)
            }
        }
        _ => welcome::view(locale),
    }
}

fn loading_view<'a>(locale: UiLocale) -> Element<'a, Message> {
    use crate::i18n::t;
    container(
        text(t(locale, "tabBar.loading"))
            .size(14)
            .color(Color::from_rgb(0.5, 0.5, 0.5)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}
