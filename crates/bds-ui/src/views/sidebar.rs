use std::path::{Path, PathBuf};

use iced::widget::{button, column, container, image, row, scrollable, text, text_input, Space};
use iced::widget::text::Shaping;
use iced::{Background, Border, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::{Media, Post, Script, Template};

use crate::app::Message;
use crate::i18n::t;
use crate::state::navigation::SidebarView;
use crate::state::sidebar_filter::{CalendarYear, MediaFilter, PostFilter};
use crate::state::tabs::{Tab, TabType};

/// Sidebar container style — dark background.
fn sidebar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.16, 0.16, 0.20))),
        ..container::Style::default()
    }
}

/// Sidebar item button style.
fn item_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.22, 0.22, 0.27),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.80, 0.80, 0.85),
        border: Border::default(),
        ..button::Style::default()
    }
}

/// Sidebar item button style — active/selected.
fn item_active_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.26, 0.26, 0.32),
        _ => Color::from_rgb(0.22, 0.22, 0.28),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border::default(),
        ..button::Style::default()
    }
}

/// 40×40 thumbnail container: rounded corners, dark background.
fn thumbnail_container_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.14, 0.14, 0.17))),
        border: Border {
            radius: 4.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    }
}

/// Get the appropriate empty-state message key for each sidebar view.
fn placeholder_key(view: SidebarView) -> &'static str {
    match view {
        SidebarView::Posts => "sidebar.noPostsYet",
        SidebarView::Pages => "sidebar.noPagesYet",
        SidebarView::Media => "sidebar.noMediaYet",
        SidebarView::Scripts => "sidebar.noScriptsYet",
        SidebarView::Templates => "sidebar.noTemplatesYet",
        SidebarView::Tags => "sidebar.tagsHeader",
        SidebarView::Chat => "sidebar.chatPlaceholder",
        SidebarView::Import => "sidebar.importPlaceholder",
        SidebarView::Git => "sidebar.gitPlaceholder",
        SidebarView::Settings => "sidebar.settingsHeader",
    }
}

/// sidebar_views.allium media_title_max_length = 60
const MEDIA_TITLE_MAX_LEN: usize = 60;

/// Approximate average character width at font size 12 for proportional fonts.
/// Used to estimate how many characters fit in a given pixel width.
const AVG_CHAR_WIDTH_PX: f32 = 6.8;

/// Sidebar padding on each side (12px) plus item padding (6px each side).
const SIDEBAR_TEXT_OVERHEAD_PX: f32 = 36.0;

/// Truncate a string to fit approximately within `available_px` pixels,
/// appending "…" if truncation occurs.
fn truncate_to_fit(s: &str, available_px: f32) -> String {
    let max_chars = ((available_px / AVG_CHAR_WIDTH_PX) as usize).max(6);
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}\u{2026}")
    }
}

/// Format a file size in bytes to a human-readable string (B / KB / MB).
fn format_file_size(size: i64) -> String {
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    }
}

/// Truncate a media title to the max length, appending "..." if over limit.
/// Per sidebar_views.allium: JS hard limit of 60 chars on title (substring + "...").
fn truncate_media_title(title: &str) -> String {
    if title.chars().count() > MEDIA_TITLE_MAX_LEN {
        let truncated: String = title.chars().take(MEDIA_TITLE_MAX_LEN).collect();
        format!("{truncated}...")
    } else {
        title.to_string()
    }
}

/// Per sidebar_views.allium PostTypeIcon: map first category to emoji.
fn post_type_icon(categories: &[String]) -> &'static str {
    for cat in categories {
        match cat.to_lowercase().as_str() {
            "picture" => return "\u{1F4F7}",       // 📷 camera
            "article" => return "\u{1F5D2}",        // 🗒 notepad
            "aside" | "blogmark" => return "\u{1F517}", // 🔗 link
            "video" => return "\u{1F3AC}",          // 🎬 film
            "podcast" => return "\u{1F4AC}",        // 💬 speech bubble
            _ => {}
        }
    }
    "\u{1F4C4}" // 📄 document (default)
}

/// Per sidebar_views.allium PostDateFormat: "Feb 10, 2026".
fn format_post_date(unix_ms: i64) -> String {
    let secs = unix_ms / 1000;
    let dt = chrono::DateTime::from_timestamp(secs, 0)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp(0, 0).unwrap());
    dt.format("%b %d, %Y").to_string()
}

/// Search input style.
fn search_input_style(_theme: &Theme, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::from_rgb(0.12, 0.12, 0.15)),
        border: Border {
            color: Color::from_rgb(0.30, 0.30, 0.35),
            width: 1.0,
            radius: 3.0.into(),
        },
        icon: Color::from_rgb(0.50, 0.50, 0.55),
        placeholder: Color::from_rgb(0.45, 0.45, 0.50),
        value: Color::from_rgb(0.85, 0.85, 0.90),
        selection: Color::from_rgba(0.35, 0.55, 0.85, 0.4),
    }
}

/// Style for filter toggle / clear buttons in the header.
fn filter_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.25, 0.25, 0.30),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.60, 0.60, 0.65),
        border: Border::default(),
        ..button::Style::default()
    }
}

/// Active filter toggle button.
fn filter_button_active_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.25, 0.30, 0.40),
        _ => Color::from_rgb(0.20, 0.25, 0.35),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.55, 0.70, 0.95),
        border: Border::default(),
        ..button::Style::default()
    }
}

/// Tag/category chip style (unselected).
fn chip_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.24, 0.24, 0.30),
        _ => Color::from_rgb(0.18, 0.18, 0.22),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.70, 0.70, 0.75),
        border: Border {
            color: Color::from_rgb(0.30, 0.30, 0.35),
            width: 1.0,
            radius: 10.0.into(),
        },
        ..button::Style::default()
    }
}

/// Tag/category chip style (selected).
fn chip_selected_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.25, 0.35, 0.55),
        _ => Color::from_rgb(0.20, 0.30, 0.50),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.85, 0.90, 1.0),
        border: Border {
            color: Color::from_rgb(0.40, 0.55, 0.80),
            width: 1.0,
            radius: 10.0.into(),
        },
        ..button::Style::default()
    }
}

/// Calendar year/month button style.
fn calendar_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.22, 0.22, 0.27),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.70, 0.70, 0.75),
        border: Border::default(),
        ..button::Style::default()
    }
}

/// Calendar year/month button style (selected).
fn calendar_selected_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.25, 0.30, 0.40),
        _ => Color::from_rgb(0.20, 0.25, 0.35),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.55, 0.70, 0.95),
        border: Border::default(),
        ..button::Style::default()
    }
}

/// "Clear All Filters" button style.
fn clear_filters_style(_theme: &Theme, status: button::Status) -> button::Style {
    let color = match status {
        button::Status::Hovered => Color::from_rgb(0.90, 0.50, 0.50),
        _ => Color::from_rgb(0.75, 0.45, 0.45),
    };
    button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: color,
        border: Border::default(),
        ..button::Style::default()
    }
}

/// Month name abbreviation for calendar display.
fn month_abbr(month: u32) -> &'static str {
    match month {
        1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr",
        5 => "May", 6 => "Jun", 7 => "Jul", 8 => "Aug",
        9 => "Sep", 10 => "Oct", 11 => "Nov", 12 => "Dec",
        _ => "???",
    }
}

/// Build the calendar archive tree widget.
fn calendar_widget(
    years: &[CalendarYear],
    selected_year: Option<i32>,
    selected_month: Option<u32>,
    on_year: impl Fn(Option<i32>) -> Message + 'static + Clone,
    on_month: impl Fn(Option<u32>) -> Message + 'static + Clone,
    locale: UiLocale,
) -> Element<'static, Message> {
    let muted = Color::from_rgb(0.50, 0.50, 0.55);
    let mut items: Vec<Element<'static, Message>> = Vec::new();

    let section_label = text(t(locale, "sidebar.filter.calendar"))
        .size(10)
        .shaping(Shaping::Advanced)
        .color(muted);
    items.push(section_label.into());

    for cy in years {
        let year_selected = selected_year == Some(cy.year);
        let total: usize = cy.months.iter().map(|m| m.count).sum();
        let label = format!("{} ({})", cy.year, total);
        let year_val = cy.year;
        let on_year_clone = on_year.clone();
        let style_fn = if year_selected { calendar_selected_style } else { calendar_style };
        let year_btn = button(
            text(label).size(11).shaping(Shaping::Advanced)
        )
            .on_press(if year_selected {
                on_year_clone(None)
            } else {
                on_year_clone(Some(year_val))
            })
            .padding([2, 4])
            .style(style_fn);
        items.push(year_btn.into());

        // Show months only when this year is selected
        if year_selected {
            for cm in &cy.months {
                let month_selected = selected_month == Some(cm.month);
                let label = format!("  {} ({})", month_abbr(cm.month), cm.count);
                let month_val = cm.month;
                let on_month_clone = on_month.clone();
                let style_fn = if month_selected { calendar_selected_style } else { calendar_style };
                let month_btn = button(
                    text(label).size(10).shaping(Shaping::Advanced)
                )
                    .on_press(if month_selected {
                        on_month_clone(None)
                    } else {
                        on_month_clone(Some(month_val))
                    })
                    .padding([1, 4])
                    .style(style_fn);
                items.push(month_btn.into());
            }
        }
    }

    iced::widget::Column::with_children(items).spacing(1).into()
}

/// Build chip selector for tags or categories.
fn chip_selector(
    label: &str,
    available: &[String],
    selected: &[String],
    on_toggle: impl Fn(String) -> Message + 'static + Clone,
    locale: UiLocale,
) -> Element<'static, Message> {
    let muted = Color::from_rgb(0.50, 0.50, 0.55);
    let mut items: Vec<Element<'static, Message>> = Vec::new();

    let section_label = text(t(locale, label))
        .size(10)
        .shaping(Shaping::Advanced)
        .color(muted);
    items.push(section_label.into());

    // Wrap chips in rows
    let mut chip_row: Vec<Element<'static, Message>> = Vec::new();
    for tag in available {
        let is_selected = selected.contains(tag);
        let tag_clone = tag.clone();
        let on_toggle_clone = on_toggle.clone();
        let style_fn = if is_selected { chip_selected_style } else { chip_style };
        let chip = button(
            text(tag.clone()).size(10).shaping(Shaping::Advanced)
        )
            .on_press(on_toggle_clone(tag_clone))
            .padding([2, 6])
            .style(style_fn);
        chip_row.push(chip.into());
    }

    if !chip_row.is_empty() {
        let mut collected: Vec<Element<'static, Message>> = chip_row.into_iter().collect();
        while !collected.is_empty() {
            let chunk_size = 3.min(collected.len());
            let chunk: Vec<Element<'static, Message>> = collected.drain(..chunk_size).collect();
            items.push(
                iced::widget::Row::with_children(chunk).spacing(4).into()
            );
        }
    }

    iced::widget::Column::with_children(items).spacing(2).into()
}

/// Build the filter panel for Posts/Pages view.
fn post_filter_panel(
    filter: &PostFilter,
    is_pages: bool,
    locale: UiLocale,
) -> Element<'static, Message> {
    let mut sections: Vec<Element<'static, Message>> = Vec::new();

    // Calendar archive
    if !filter.calendar_years.is_empty() {
        sections.push(calendar_widget(
            &filter.calendar_years,
            filter.calendar.selected_year,
            filter.calendar.selected_month,
            |y| Message::SetPostCalendarYear(y),
            |m| Message::SetPostCalendarMonth(m),
            locale,
        ));
        sections.push(Space::with_height(4.0).into());
    }

    // Tag chips
    if !filter.available_tags.is_empty() {
        sections.push(chip_selector(
            "sidebar.filter.tags",
            &filter.available_tags,
            &filter.tag_filter,
            |tag| Message::TogglePostTagFilter(tag),
            locale,
        ));
        sections.push(Space::with_height(4.0).into());
    }

    // Category chips (not shown for Pages since Pages IS a category filter)
    if !is_pages && !filter.available_categories.is_empty() {
        sections.push(chip_selector(
            "sidebar.filter.categories",
            &filter.available_categories,
            &filter.category_filter,
            |cat| Message::TogglePostCategoryFilter(cat),
            locale,
        ));
        sections.push(Space::with_height(4.0).into());
    }

    // Clear all filters button
    if filter.has_active_filters() {
        sections.push(
            button(
                text(t(locale, "sidebar.filter.clearAll"))
                    .size(10)
                    .shaping(Shaping::Advanced)
            )
                .on_press(Message::ClearPostFilters)
                .padding([2, 4])
                .style(clear_filters_style)
                .into()
        );
    }

    iced::widget::Column::with_children(sections).spacing(2).into()
}

/// Build the filter panel for Media view.
fn media_filter_panel(
    filter: &MediaFilter,
    locale: UiLocale,
) -> Element<'static, Message> {
    let mut sections: Vec<Element<'static, Message>> = Vec::new();

    if !filter.calendar_years.is_empty() {
        sections.push(calendar_widget(
            &filter.calendar_years,
            filter.calendar.selected_year,
            filter.calendar.selected_month,
            |y| Message::SetMediaCalendarYear(y),
            |m| Message::SetMediaCalendarMonth(m),
            locale,
        ));
        sections.push(Space::with_height(4.0).into());
    }

    if !filter.available_tags.is_empty() {
        sections.push(chip_selector(
            "sidebar.filter.tags",
            &filter.available_tags,
            &filter.tag_filter,
            |tag| Message::ToggleMediaTagFilter(tag),
            locale,
        ));
        sections.push(Space::with_height(4.0).into());
    }

    if filter.has_active_filters() {
        sections.push(
            button(
                text(t(locale, "sidebar.filter.clearAll"))
                    .size(10)
                    .shaping(Shaping::Advanced)
            )
                .on_press(Message::ClearMediaFilters)
                .padding([2, 4])
                .style(clear_filters_style)
                .into()
        );
    }

    iced::widget::Column::with_children(sections).spacing(2).into()
}

pub fn view(
    sidebar_view: SidebarView,
    posts: &[Post],
    media: &[Media],
    scripts: &[Script],
    templates: &[Template],
    post_filter: &PostFilter,
    media_filter: &MediaFilter,
    media_thumbs: &std::collections::HashMap<String, Option<PathBuf>>,
    posts_has_more: bool,
    media_has_more: bool,
    width: f32,
    active_tab: Option<&str>,
    locale: UiLocale,
    _data_dir: Option<&Path>,
) -> Element<'static, Message> {
    let header_text = t(locale, sidebar_view.i18n_key());
    let muted = Color::from_rgb(0.50, 0.50, 0.55);

    let header = text(header_text)
        .size(13)
        .shaping(Shaping::Advanced)
        .color(Color::from_rgb(0.85, 0.85, 0.90));

    let body: Element<'static, Message> = match sidebar_view {
        SidebarView::Posts | SidebarView::Pages => {
            let is_pages = sidebar_view == SidebarView::Pages;
            let filter = post_filter;

            let mut top_items: Vec<Element<'static, Message>> = Vec::new();

            // Search input + filter toggle row
            let search = text_input(
                &t(locale, "sidebar.filter.search"),
                &filter.search_query,
            )
                .on_input(Message::PostSearchChanged)
                .size(11)
                .padding([4, 6])
                .width(Length::Fill)
                .style(search_input_style);

            let has_filters = filter.has_active_filters();
            let toggle_style = if filter.filter_panel_visible || has_filters {
                filter_button_active_style
            } else {
                filter_button_style
            };
            let toggle_label = if has_filters {
                "\u{2B50}" // ⭐ indicates active filters
            } else {
                "\u{25BC}" // ▼ toggle icon
            };
            let filter_toggle = button(
                text(toggle_label).size(11).shaping(Shaping::Advanced)
            )
                .on_press(Message::TogglePostFilterPanel)
                .padding([4, 6])
                .style(toggle_style);

            top_items.push(
                row![search, filter_toggle].spacing(4).into()
            );

            // Filter panel (collapsible)
            if filter.filter_panel_visible {
                top_items.push(Space::with_height(4.0).into());
                top_items.push(post_filter_panel(filter, is_pages, locale));
                top_items.push(Space::with_height(4.0).into());
            }

            // Post list
            if posts.is_empty() {
                let key = if has_filters {
                    "sidebar.filter.noResults"
                } else {
                    placeholder_key(sidebar_view)
                };
                top_items.push(
                    text(t(locale, key))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(muted)
                        .into()
                );
            } else {
                let section_header = |label: &str| -> Element<'static, Message> {
                    text(label.to_string())
                        .size(11)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.55, 0.55, 0.60))
                        .into()
                };

                let make_post_item = |p: &Post| -> Element<'static, Message> {
                    let is_active = active_tab == Some(p.id.as_str());
                    let icon = post_type_icon(&p.categories);
                    let status_indicator = match p.status {
                        bds_core::model::PostStatus::Draft => "\u{25CB}",
                        bds_core::model::PostStatus::Published => "\u{25CF}",
                        bds_core::model::PostStatus::Archived => "\u{25A1}",
                    };
                    let date = format_post_date(p.created_at);
                    let prefix_chars: usize = 5;
                    let text_px = width - SIDEBAR_TEXT_OVERHEAD_PX
                        - (prefix_chars as f32 * AVG_CHAR_WIDTH_PX);
                    let display_title = truncate_to_fit(&p.title, text_px);
                    let label = format!("{icon} {status_indicator} {display_title}");
                    let label_text = text(label)
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .wrapping(iced::widget::text::Wrapping::None);
                    let date_text = text(date)
                        .size(10)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.50, 0.50, 0.55));
                    let style_fn = if is_active { item_active_style } else { item_style };
                    button(
                        container(column![label_text, date_text].spacing(1))
                            .width(Length::Fill)
                            .clip(true)
                    )
                        .on_press(Message::OpenTab(Tab {
                            id: p.id.clone(),
                            tab_type: TabType::Post,
                            title: p.title.clone(),
                            is_transient: true,
                            is_dirty: false,
                        }))
                        .padding([3, 6])
                        .width(Length::Fill)
                        .style(style_fn)
                        .into()
                };

                // Draft section
                let drafts: Vec<&Post> = posts.iter().filter(|p| p.status == bds_core::model::PostStatus::Draft).collect();
                if !drafts.is_empty() {
                    top_items.push(section_header(&t(locale, "sidebar.drafts")));
                    for p in &drafts {
                        top_items.push(make_post_item(p));
                    }
                    top_items.push(Space::with_height(6.0).into());
                }

                // Published section
                let published: Vec<&Post> = posts.iter().filter(|p| p.status == bds_core::model::PostStatus::Published).collect();
                if !published.is_empty() {
                    top_items.push(section_header(&t(locale, "sidebar.published")));
                    for p in &published {
                        top_items.push(make_post_item(p));
                    }
                    top_items.push(Space::with_height(6.0).into());
                }

                // Archived section
                let archived: Vec<&Post> = posts.iter().filter(|p| p.status == bds_core::model::PostStatus::Archived).collect();
                if !archived.is_empty() {
                    top_items.push(section_header(&t(locale, "sidebar.archived")));
                    for p in &archived {
                        top_items.push(make_post_item(p));
                    }
                }

                // "Load More" button
                if posts_has_more {
                    top_items.push(Space::with_height(4.0).into());
                    top_items.push(
                        button(
                            text(t(locale, "sidebar.loadMore"))
                                .size(11)
                                .shaping(Shaping::Advanced)
                        )
                            .on_press(Message::LoadMorePosts)
                            .padding([4, 8])
                            .width(Length::Fill)
                            .style(filter_button_style)
                            .into()
                    );
                }
            }

            iced::widget::Column::with_children(top_items)
                .spacing(1)
                .into()
        }
        SidebarView::Media => {
            let filter = media_filter;
            let mut top_items: Vec<Element<'static, Message>> = Vec::new();

            // Search input + filter toggle row
            let search = text_input(
                &t(locale, "sidebar.filter.search"),
                &filter.search_query,
            )
                .on_input(Message::MediaSearchChanged)
                .size(11)
                .padding([4, 6])
                .width(Length::Fill)
                .style(search_input_style);

            let has_filters = filter.has_active_filters();
            let toggle_style = if filter.filter_panel_visible || has_filters {
                filter_button_active_style
            } else {
                filter_button_style
            };
            let toggle_label = if has_filters {
                "\u{2B50}" // ⭐ indicates active filters
            } else {
                "\u{25BC}" // ▼ toggle icon
            };
            let filter_toggle = button(
                text(toggle_label).size(11).shaping(Shaping::Advanced)
            )
                .on_press(Message::ToggleMediaFilterPanel)
                .padding([4, 6])
                .style(toggle_style);

            top_items.push(
                row![search, filter_toggle].spacing(4).into()
            );

            // Filter panel (collapsible)
            if filter.filter_panel_visible {
                top_items.push(Space::with_height(4.0).into());
                top_items.push(media_filter_panel(filter, locale));
                top_items.push(Space::with_height(4.0).into());
            }

            if media.is_empty() {
                let key = if has_filters {
                    "sidebar.filter.noResults"
                } else {
                    placeholder_key(sidebar_view)
                };
                top_items.push(
                    text(t(locale, key))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(muted)
                        .into()
                );
            } else {
                let items: Vec<Element<'static, Message>> = media
                    .iter()
                    .map(|m| {
                        let is_active = active_tab == Some(m.id.as_str());
                        let display_name = match m.title.as_deref() {
                            Some(title) => truncate_media_title(title),
                            None => m.original_name.clone(),
                        };
                        let text_px = width - SIDEBAR_TEXT_OVERHEAD_PX - 48.0;
                        let display_name = truncate_to_fit(&display_name, text_px);
                        let style_fn = if is_active { item_active_style } else { item_style };

                        // Left: 40x40 thumbnail or file icon (pre-resolved, no filesystem I/O)
                        let thumb_path = media_thumbs
                            .get(&m.id)
                            .and_then(|opt| opt.as_ref());
                        let left: Element<'static, Message> = if let Some(path) = thumb_path {
                            container(
                                image(path.to_string_lossy().to_string())
                                    .width(Length::Fixed(40.0))
                                    .height(Length::Fixed(40.0))
                                    .content_fit(iced::ContentFit::Cover)
                            )
                                .width(Length::Fixed(40.0))
                                .height(Length::Fixed(40.0))
                                .clip(true)
                                .style(thumbnail_container_style)
                                .into()
                        } else {
                            container(
                                text("\u{1F4C4}") // 📄 generic file icon
                                    .size(20)
                                    .shaping(Shaping::Advanced)
                            )
                                .width(Length::Fixed(40.0))
                                .height(Length::Fixed(40.0))
                                .align_x(iced::alignment::Horizontal::Center)
                                .align_y(iced::alignment::Vertical::Center)
                                .style(thumbnail_container_style)
                                .into()
                        };

                        // Right column: name + metadata line
                        let name_text = text(display_name.clone())
                            .size(12)
                            .shaping(Shaping::Advanced)
                            .wrapping(iced::widget::text::Wrapping::None);

                        let file_size = format_file_size(m.size);
                        let meta_label = match (m.width, m.height) {
                            (Some(w), Some(h)) => format!("{file_size} · {w}×{h}"),
                            _ => file_size,
                        };
                        let meta_text = text(meta_label)
                            .size(10)
                            .shaping(Shaping::Advanced)
                            .color(Color::from_rgb(0.50, 0.50, 0.55));

                        let right = column![name_text, meta_text].spacing(1);

                        let content = row![left, right].spacing(8).align_y(iced::Alignment::Center);

                        button(
                            container(content)
                                .width(Length::Fill)
                                .clip(true)
                        )
                            .on_press(Message::OpenTab(Tab {
                                id: m.id.clone(),
                                tab_type: TabType::Media,
                                title: display_name.clone(),
                                is_transient: true,
                                is_dirty: false,
                            }))
                            .padding([4, 6])
                            .width(Length::Fill)
                            .style(style_fn)
                            .into()
                    })
                    .collect();
                top_items.extend(items);

                // "Load More" button
                if media_has_more {
                    top_items.push(Space::with_height(4.0).into());
                    top_items.push(
                        button(
                            text(t(locale, "sidebar.loadMore"))
                                .size(11)
                                .shaping(Shaping::Advanced)
                        )
                            .on_press(Message::LoadMoreMedia)
                            .padding([4, 8])
                            .width(Length::Fill)
                            .style(filter_button_style)
                            .into()
                    );
                }
            }

            iced::widget::Column::with_children(top_items)
                .spacing(1)
                .into()
        }
        SidebarView::Scripts => {
            if scripts.is_empty() {
                text(t(locale, placeholder_key(sidebar_view)))
                    .size(12)
                    .shaping(Shaping::Advanced)
                    .color(muted)
                    .into()
            } else {
                let items: Vec<Element<'static, Message>> = scripts
                    .iter()
                    .map(|s| {
                        let is_active = active_tab == Some(s.id.as_str());
                        let text_px = width - SIDEBAR_TEXT_OVERHEAD_PX;
                        let display_title = truncate_to_fit(&s.title, text_px);
                        let label_text = text(display_title.clone())
                            .size(12)
                            .shaping(Shaping::Advanced)
                            .wrapping(iced::widget::text::Wrapping::None);
                        let style_fn = if is_active { item_active_style } else { item_style };
                        button(
                            container(label_text)
                                .width(Length::Fill)
                                .clip(true)
                        )
                            .on_press(Message::OpenTab(Tab {
                                id: s.id.clone(),
                                tab_type: TabType::Scripts,
                                title: display_title,
                                is_transient: true,
                                is_dirty: false,
                            }))
                            .padding([3, 6])
                            .width(Length::Fill)
                            .style(style_fn)
                            .into()
                    })
                    .collect();
                iced::widget::Column::with_children(items)
                    .spacing(1)
                    .into()
            }
        }
        SidebarView::Templates => {
            if templates.is_empty() {
                text(t(locale, placeholder_key(sidebar_view)))
                    .size(12)
                    .shaping(Shaping::Advanced)
                    .color(muted)
                    .into()
            } else {
                let items: Vec<Element<'static, Message>> = templates
                    .iter()
                    .map(|tmpl| {
                        let is_active = active_tab == Some(tmpl.id.as_str());
                        let text_px = width - SIDEBAR_TEXT_OVERHEAD_PX;
                        let display_title = truncate_to_fit(&tmpl.title, text_px);
                        let label_text = text(display_title.clone())
                            .size(12)
                            .shaping(Shaping::Advanced)
                            .wrapping(iced::widget::text::Wrapping::None);
                        let style_fn = if is_active { item_active_style } else { item_style };
                        button(
                            container(label_text)
                                .width(Length::Fill)
                                .clip(true)
                        )
                            .on_press(Message::OpenTab(Tab {
                                id: tmpl.id.clone(),
                                tab_type: TabType::Templates,
                                title: display_title,
                                is_transient: true,
                                is_dirty: false,
                            }))
                            .padding([3, 6])
                            .width(Length::Fill)
                            .style(style_fn)
                            .into()
                    })
                    .collect();
                iced::widget::Column::with_children(items)
                    .spacing(1)
                    .into()
            }
        }
        SidebarView::Settings => {
            // Per sidebar_views.allium SettingsNav: 9 fixed-order sections
            use crate::views::settings_view::SettingsSection;
            let sections: &[(&str, Option<SettingsSection>)] = &[
                ("settings.nav.project", Some(SettingsSection::Project)),
                ("settings.nav.editor", Some(SettingsSection::Editor)),
                ("settings.nav.content", Some(SettingsSection::Content)),
                ("settings.nav.ai", Some(SettingsSection::AI)),
                ("settings.nav.technology", Some(SettingsSection::Technology)),
                ("settings.nav.publishing", Some(SettingsSection::Publishing)),
                ("settings.nav.data", Some(SettingsSection::Data)),
                ("settings.nav.mcp", Some(SettingsSection::MCP)),
                ("settings.nav.style", None),
            ];
            let items: Vec<Element<'static, Message>> = sections
                .iter()
                .map(|(key, section_opt)| {
                    let label = t(locale, key);
                    let label_text = text(label)
                        .size(12)
                        .shaping(Shaping::Advanced);
                    let msg = if let Some(section) = section_opt {
                        Message::OpenSettingsSection(section.clone())
                    } else {
                        Message::OpenTab(Tab {
                            id: "style".to_string(),
                            tab_type: TabType::Style,
                            title: t(locale, "tabBar.style"),
                            is_transient: false,
                            is_dirty: false,
                        })
                    };
                    button(
                        container(label_text)
                            .width(Length::Fill)
                    )
                        .on_press(msg)
                        .padding([3, 6])
                        .width(Length::Fill)
                        .style(item_style)
                        .into()
                })
                .collect();
            iced::widget::Column::with_children(items)
                .spacing(1)
                .into()
        }
        SidebarView::Tags => {
            // Per sidebar_views.allium TagsNav: 3 fixed-order sections
            let sections = [
                "tags.nav.cloud",
                "tags.nav.manage",
                "tags.nav.merge",
            ];
            let items: Vec<Element<'static, Message>> = sections
                .iter()
                .map(|key| {
                    let label = t(locale, key);
                    let label_text = text(label)
                        .size(12)
                        .shaping(Shaping::Advanced);
                    button(
                        container(label_text)
                            .width(Length::Fill)
                    )
                        .on_press(Message::OpenTab(Tab {
                            id: "tags".to_string(),
                            tab_type: TabType::Tags,
                            title: t(locale, "tabBar.tags"),
                            is_transient: false,
                            is_dirty: false,
                        }))
                        .padding([3, 6])
                        .width(Length::Fill)
                        .style(item_style)
                        .into()
                })
                .collect();
            iced::widget::Column::with_children(items)
                .spacing(1)
                .into()
        }
        _ => {
            text(t(locale, placeholder_key(sidebar_view)))
                .size(12)
                .shaping(Shaping::Advanced)
                .color(muted)
                .into()
        }
    };

    let content = column![
        header,
        Space::with_height(8.0),
        body,
    ]
    .spacing(4)
    .padding(12);

    // layout.allium: sidebar width is resizable, passed as parameter
    container(scrollable(content))
        .width(Length::Fixed(width))
        .height(Length::Fill)
        .style(sidebar_style)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_media_title_short() {
        assert_eq!(truncate_media_title("short title"), "short title");
    }

    #[test]
    fn truncate_media_title_exact_60() {
        let title: String = "a".repeat(60);
        assert_eq!(truncate_media_title(&title), title);
    }

    #[test]
    fn truncate_media_title_over_60() {
        let title: String = "a".repeat(65);
        let expected = format!("{}...", "a".repeat(60));
        assert_eq!(truncate_media_title(&title), expected);
    }

    #[test]
    fn truncate_media_title_unicode() {
        // 61 Unicode chars should trigger truncation
        let title: String = "\u{00FC}".repeat(61); // ü × 61
        let expected = format!("{}...", "\u{00FC}".repeat(60));
        assert_eq!(truncate_media_title(&title), expected);
    }

    #[test]
    fn truncate_to_fit_short() {
        // 100px at ~6.8px/char ≈ 14 chars; "Hello" fits.
        assert_eq!(truncate_to_fit("Hello", 100.0), "Hello");
    }

    #[test]
    fn truncate_to_fit_long() {
        // 50px at ~6.8px/char ≈ 7 chars; 20-char string truncated.
        let result = truncate_to_fit(&"a".repeat(20), 50.0);
        assert!(result.ends_with('\u{2026}'));
        assert!(result.chars().count() <= 8);
    }

    #[test]
    fn truncate_to_fit_narrow() {
        // Very narrow (10px): minimum 6 chars enforced.
        let result = truncate_to_fit(&"a".repeat(20), 10.0);
        assert!(result.ends_with('\u{2026}'));
        assert!(result.chars().count() >= 2);
    }

    #[test]
    fn format_file_size_bytes() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(1023), "1023 B");
    }

    #[test]
    fn format_file_size_kilobytes() {
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(1024 * 1024 - 1), "1024.0 KB");
    }

    #[test]
    fn format_file_size_megabytes() {
        assert_eq!(format_file_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_file_size(5 * 1024 * 1024), "5.0 MB");
    }
}
