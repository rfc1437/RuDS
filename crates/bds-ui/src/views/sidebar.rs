use iced::widget::{button, column, container, scrollable, text, Space};
use iced::widget::text::Shaping;
use iced::{Background, Border, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::{Media, Post};

use crate::app::Message;
use crate::i18n::t;
use crate::state::navigation::SidebarView;
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

/// Extra space used by the post status indicator prefix ("○ " etc.).
const STATUS_INDICATOR_CHARS: usize = 2;

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

pub fn view(
    sidebar_view: SidebarView,
    posts: &[Post],
    media: &[Media],
    width: f32,
    active_tab: Option<&str>,
    locale: UiLocale,
) -> Element<'static, Message> {
    let header_text = t(locale, sidebar_view.i18n_key());
    let muted = Color::from_rgb(0.50, 0.50, 0.55);

    let header = text(header_text)
        .size(13)
        .shaping(Shaping::Advanced)
        .color(Color::from_rgb(0.85, 0.85, 0.90));

    let body: Element<'static, Message> = match sidebar_view {
        SidebarView::Posts => {
            if posts.is_empty() {
                text(t(locale, placeholder_key(sidebar_view)))
                    .size(12)
                    .shaping(Shaping::Advanced)
                    .color(muted)
                    .into()
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
                    let status_indicator = match p.status {
                        bds_core::model::PostStatus::Draft => "\u{25CB} ",
                        bds_core::model::PostStatus::Published => "\u{25CF} ",
                        bds_core::model::PostStatus::Archived => "\u{25A1} ",
                    };
                    // Truncate title to fit sidebar width, accounting for
                    // padding and the status indicator prefix.
                    let text_px = width - SIDEBAR_TEXT_OVERHEAD_PX
                        - (STATUS_INDICATOR_CHARS as f32 * AVG_CHAR_WIDTH_PX);
                    let display_title = truncate_to_fit(&p.title, text_px);
                    let label = format!("{status_indicator}{display_title}");
                    let label_text = text(label)
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

                let mut sections: Vec<Element<'static, Message>> = Vec::new();

                // Draft section
                let drafts: Vec<&Post> = posts.iter().filter(|p| p.status == bds_core::model::PostStatus::Draft).collect();
                if !drafts.is_empty() {
                    sections.push(section_header(&t(locale, "sidebar.drafts")));
                    for p in &drafts {
                        sections.push(make_post_item(p));
                    }
                    sections.push(Space::with_height(6.0).into());
                }

                // Published section
                let published: Vec<&Post> = posts.iter().filter(|p| p.status == bds_core::model::PostStatus::Published).collect();
                if !published.is_empty() {
                    sections.push(section_header(&t(locale, "sidebar.published")));
                    for p in &published {
                        sections.push(make_post_item(p));
                    }
                    sections.push(Space::with_height(6.0).into());
                }

                // Archived section
                let archived: Vec<&Post> = posts.iter().filter(|p| p.status == bds_core::model::PostStatus::Archived).collect();
                if !archived.is_empty() {
                    sections.push(section_header(&t(locale, "sidebar.archived")));
                    for p in &archived {
                        sections.push(make_post_item(p));
                    }
                }

                iced::widget::Column::with_children(sections)
                    .spacing(1)
                    .into()
            }
        }
        SidebarView::Media => {
            if media.is_empty() {
                text(t(locale, placeholder_key(sidebar_view)))
                    .size(12)
                    .shaping(Shaping::Advanced)
                    .color(muted)
                    .into()
            } else {
                let items: Vec<Element<'static, Message>> = media
                    .iter()
                    .map(|m| {
                        let is_active = active_tab == Some(m.id.as_str());
                        // Per sidebar_views.allium MediaGridItem: title truncated to 60 chars + "..."
                        // if over limit; fallback originalName (no truncation).
                        // Additionally truncate to fit sidebar width.
                        let display_name = match m.title.as_deref() {
                            Some(title) => truncate_media_title(title),
                            None => m.original_name.clone(),
                        };
                        // Emoji prefix "🖼 " is ~2 chars wide
                        let text_px = width - SIDEBAR_TEXT_OVERHEAD_PX - 16.0;
                        let display_name = truncate_to_fit(&display_name, text_px);
                        let label = format!("\u{1F5BC} {display_name}");
                        let label_text = text(label)
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
                                id: m.id.clone(),
                                tab_type: TabType::Media,
                                title: display_name.clone(),
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
}
