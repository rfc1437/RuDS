use iced::widget::{button, column, container, text, Column, Space};
use iced::{Alignment, Element, Length};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::state::navigation::SidebarView;

/// Activity-bar short labels used in M2 (no SVG icons yet).
fn short_label(view: SidebarView) -> &'static str {
    match view {
        SidebarView::Posts => "Po",
        SidebarView::Pages => "Pa",
        SidebarView::Media => "Me",
        SidebarView::Scripts => "Sc",
        SidebarView::Templates => "Tp",
        SidebarView::Tags => "Ta",
        SidebarView::Chat => "AI",
        SidebarView::Import => "Im",
        SidebarView::Git => "Gi",
        SidebarView::Settings => "Se",
    }
}

/// Top group of activity items.
const TOP_ACTIVITIES: &[SidebarView] = &[
    SidebarView::Posts,
    SidebarView::Pages,
    SidebarView::Media,
    SidebarView::Scripts,
    SidebarView::Templates,
    SidebarView::Tags,
    SidebarView::Chat,
    SidebarView::Import,
];

/// Bottom group of activity items.
const BOTTOM_ACTIVITIES: &[SidebarView] = &[
    SidebarView::Git,
    SidebarView::Settings,
];

pub fn view(
    active_view: SidebarView,
    _locale: UiLocale,
) -> Element<'static, Message> {
    let make_btn = |view: SidebarView| -> Element<'static, Message> {
        let label = short_label(view);
        let mut btn = button(
            text(label).size(12).center(),
        )
        .width(Length::Fixed(48.0))
        .height(Length::Fixed(48.0))
        .on_press(Message::SetActiveView(view));

        if view == active_view {
            btn = btn.style(button::primary);
        } else {
            btn = btn.style(button::secondary);
        }

        container(btn)
            .center_x(Length::Fixed(48.0))
            .into()
    };

    let top_items: Vec<Element<'static, Message>> = TOP_ACTIVITIES
        .iter()
        .map(|v| make_btn(*v))
        .collect();

    let bottom_items: Vec<Element<'static, Message>> = BOTTOM_ACTIVITIES
        .iter()
        .map(|v| make_btn(*v))
        .collect();

    let top = Column::with_children(top_items).spacing(2);
    let bottom = Column::with_children(bottom_items).spacing(2);

    column![
        top,
        Space::with_height(Length::Fill),
        bottom,
    ]
    .width(Length::Fixed(48.0))
    .height(Length::Fill)
    .align_x(Alignment::Center)
    .into()
}
