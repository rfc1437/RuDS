use iced::widget::{column, container, scrollable, text};
use iced::{Element, Length};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::t;
use crate::state::navigation::SidebarView;

pub fn view(
    sidebar_view: SidebarView,
    locale: UiLocale,
) -> Element<'static, Message> {
    let header_key = sidebar_view.i18n_key();
    let header_text = t(locale, header_key);

    let placeholder = match sidebar_view {
        SidebarView::Posts => t(locale, "sidebar.noPostsYet"),
        SidebarView::Pages => t(locale, "sidebar.noPagesYet"),
        SidebarView::Media => t(locale, "sidebar.noMediaYet"),
        SidebarView::Settings => t(locale, "sidebar.settingsHeader"),
        SidebarView::Tags => t(locale, "sidebar.tagsHeader"),
        _ => t(locale, "sidebar.loading"),
    };

    let content = column![
        text(header_text).size(14),
        text(placeholder).size(12),
    ]
    .spacing(8)
    .padding(12);

    container(scrollable(content))
        .width(Length::Fixed(280.0))
        .height(Length::Fill)
        .into()
}
