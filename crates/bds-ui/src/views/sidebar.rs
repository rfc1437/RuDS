use iced::widget::{column, container, scrollable, text, Space};
use iced::{Background, Border, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::t;
use crate::state::navigation::SidebarView;

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
    locale: UiLocale,
) -> Element<'static, Message> {
    let header_text = t(locale, sidebar_view.i18n_key());
    let placeholder = t(locale, placeholder_key(sidebar_view));

    let content = column![
        text(header_text)
            .size(13)
            .color(Color::from_rgb(0.85, 0.85, 0.90)),
        Space::with_height(8.0),
        text(placeholder)
            .size(12)
            .color(Color::from_rgb(0.50, 0.50, 0.55)),
    ]
    .spacing(4)
    .padding(12);

    container(scrollable(content))
        .width(Length::Fixed(240.0))
        .height(Length::Fill)
        .style(sidebar_style)
        .into()
}
