use iced::widget::{button, column, container, svg, text, tooltip, Column, Space};
use iced::widget::text::Shaping;
use iced::{Background, Border, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::i18n::t;
use crate::state::navigation::SidebarView;

// ---------------------------------------------------------------------------
// SVG icon data — ported from bDS ActivityBar.tsx inline SVGs
// ---------------------------------------------------------------------------

fn icon_svg(view: SidebarView) -> &'static [u8] {
    match view {
        SidebarView::Posts => include_bytes!("../../assets/icons/posts.svg"),
        SidebarView::Pages => include_bytes!("../../assets/icons/pages.svg"),
        SidebarView::Media => include_bytes!("../../assets/icons/media.svg"),
        SidebarView::Scripts => include_bytes!("../../assets/icons/scripts.svg"),
        SidebarView::Templates => include_bytes!("../../assets/icons/templates.svg"),
        SidebarView::Tags => include_bytes!("../../assets/icons/tags.svg"),
        SidebarView::Chat => include_bytes!("../../assets/icons/chat.svg"),
        SidebarView::Import => include_bytes!("../../assets/icons/import.svg"),
        SidebarView::Git => include_bytes!("../../assets/icons/git.svg"),
        SidebarView::Settings => include_bytes!("../../assets/icons/settings.svg"),
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

// ---------------------------------------------------------------------------
// Styles — matching bDS ActivityBar.css
// ---------------------------------------------------------------------------

/// Active button: transparent bg.
fn active_button_style(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(Color::TRANSPARENT)),
        text_color: Color::WHITE,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        ..button::Style::default()
    }
}

/// Inactive button: hover brightens background.
fn inactive_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.22, 0.22, 0.27),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.55, 0.55, 0.60),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        ..button::Style::default()
    }
}

/// Activity bar container: dark background.
fn bar_background_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.14, 0.14, 0.18))),
        ..container::Style::default()
    }
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

pub fn view(
    active_view: SidebarView,
    sidebar_visible: bool,
    locale: UiLocale,
) -> Element<'static, Message> {
    let make_btn = |view: SidebarView| -> Element<'static, Message> {
        let handle = svg::Handle::from_memory(icon_svg(view));
        // Per layout.allium ActivityActiveHighlight invariant:
        // button shows active iff its view == active_view AND sidebar is visible
        let is_active = view == active_view && sidebar_visible;

        // Render SVG: active = full opacity, inactive = 0.4 opacity (like bDS 60% opacity)
        let icon = svg(handle)
            .width(Length::Fixed(24.0))
            .height(Length::Fixed(24.0))
            .opacity(if is_active { 1.0 } else { 0.4 });

        let btn = button(
            container(icon)
                .center_x(Length::Fixed(48.0))
                .center_y(Length::Fixed(48.0)),
        )
        .width(Length::Fixed(48.0))
        .height(Length::Fixed(48.0))
        .padding(0)
        .on_press(Message::SetActiveView(view))
        .style(if is_active { active_button_style } else { inactive_button_style });

        // Active indicator: 2px left border (like bDS/VS Code)
        let btn_row: Element<'static, Message> = if is_active {
            let indicator = container(Space::new(0, 0))
                .width(Length::Fixed(2.0))
                .height(Length::Fixed(48.0))
                .style(|_theme: &Theme| container::Style {
                    background: Some(Background::Color(Color::WHITE)),
                    ..container::Style::default()
                });
            iced::widget::row![indicator, btn].into()
        } else {
            let spacer = Space::with_width(2.0);
            iced::widget::row![spacer, btn].into()
        };

        // Wrap in tooltip per layout.allium ActivityButton.label_key
        let tip_text = t(locale, view.i18n_key());
        tooltip(btn_row, text(tip_text).size(12).shaping(Shaping::Advanced), tooltip::Position::Right)
            .gap(4)
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

    let top = Column::with_children(top_items).spacing(0);
    let bottom = Column::with_children(bottom_items).spacing(0);

    container(
        column![
            top,
            Space::with_height(Length::Fill),
            bottom,
        ]
        .width(Length::Fixed(50.0))
        .height(Length::Fill)
        .padding([4, 0]),
    )
    .width(Length::Fixed(50.0))
    .height(Length::Fill)
    .style(bar_background_style)
    .into()
}
