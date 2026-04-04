use iced::widget::{button, column, container, scrollable, text, Space};
use iced::widget::text::Shaping;
use iced::{Background, Border, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::Post;

use crate::app::Message;
use crate::i18n::t;
use crate::state::navigation::SidebarView;
use crate::state::tabs::{Tab, TabType};

/// Sidebar container style — dark background with right border separator.
fn sidebar_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.16, 0.16, 0.20))),
        border: Border {
            color: Color::from_rgb(0.25, 0.25, 0.30),
            width: 1.0,
            radius: 0.0.into(),
        },
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

pub fn view(
    sidebar_view: SidebarView,
    posts: &[Post],
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
                let items: Vec<Element<'static, Message>> = posts
                    .iter()
                    .map(|p| {
                        let status_indicator = match p.status {
                            bds_core::model::PostStatus::Draft => "\u{25CB} ",      // ○
                            bds_core::model::PostStatus::Published => "\u{25CF} ",   // ●
                            bds_core::model::PostStatus::Archived => "\u{25A1} ",    // □
                        };
                        let label = format!("{status_indicator}{}", p.title);
                        button(text(label).size(12).shaping(Shaping::Advanced))
                            .on_press(Message::OpenTab(Tab {
                                id: p.id.clone(),
                                tab_type: TabType::Post,
                                title: p.title.clone(),
                                is_transient: true,
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

    container(scrollable(content))
        .width(Length::Fixed(240.0))
        .height(Length::Fill)
        .style(sidebar_style)
        .into()
}
