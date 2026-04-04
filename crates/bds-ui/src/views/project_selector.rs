use iced::widget::{button, container, row, svg, text, Column, Space};
use iced::widget::text::Shaping;
use iced::{Background, Border, Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::Project;

use crate::app::Message;
use crate::i18n::t;

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

fn dropdown_bg(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgb(0.18, 0.18, 0.22))),
        border: Border {
            color: Color::from_rgb(0.30, 0.30, 0.35),
            width: 1.0,
            radius: 4.0.into(),
        },
        ..container::Style::default()
    }
}

fn project_item(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.25, 0.25, 0.30),
        _ => Color::from_rgb(0.18, 0.18, 0.22),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 2.0.into(),
        },
        ..button::Style::default()
    }
}

fn project_item_active(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.25, 0.32, 0.45),
        _ => Color::from_rgb(0.20, 0.28, 0.40),
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 2.0.into(),
        },
        ..button::Style::default()
    }
}

fn new_project_btn(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.25, 0.25, 0.30),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::from_rgb(0.55, 0.75, 0.95),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 2.0.into(),
        },
        ..button::Style::default()
    }
}

fn header_style(_theme: &Theme) -> container::Style {
    container::Style {
        border: Border {
            color: Color::from_rgb(0.30, 0.30, 0.35),
            width: 0.0,
            radius: 0.0.into(),
        },
        ..container::Style::default()
    }
}

// ---------------------------------------------------------------------------
// Folder icon SVG (16x16)
// ---------------------------------------------------------------------------

const FOLDER_ICON: &[u8] = include_bytes!("../../assets/icons/folder.svg");

// ---------------------------------------------------------------------------
// View: renders the dropdown content (project list + new project)
// ---------------------------------------------------------------------------

pub fn view(
    projects: &[Project],
    active_project_id: Option<&str>,
    locale: UiLocale,
) -> Element<'static, Message> {
    let header = container(
        text(t(locale, "projectSelector.projectsHeader"))
            .size(11)
            .shaping(Shaping::Advanced)
            .color(Color::from_rgb(0.55, 0.55, 0.60)),
    )
    .padding([4, 8])
    .style(header_style);

    let mut items: Vec<Element<'static, Message>> = Vec::new();
    items.push(header.into());

    for project in projects {
        let is_active = active_project_id == Some(project.id.as_str());
        let name = project.name.clone();
        let id = project.id.clone();

        let label = if is_active {
            row![
                text("\u{2713}").size(12).shaping(Shaping::Advanced).color(Color::from_rgb(0.40, 0.80, 0.40)),
                text(name).size(12).shaping(Shaping::Advanced).color(Color::WHITE),
            ]
            .spacing(6)
        } else {
            row![
                Space::with_width(Length::Fixed(14.0)),
                text(name).size(12).shaping(Shaping::Advanced).color(Color::from_rgb(0.80, 0.80, 0.85)),
            ]
            .spacing(6)
        };

        let style_fn = if is_active { project_item_active } else { project_item };

        items.push(
            button(label)
                .on_press(Message::SwitchProject(id))
                .padding([4, 8])
                .width(Length::Fill)
                .style(style_fn)
                .into(),
        );
    }

    // Separator
    items.push(
        container(Space::new(Length::Fill, Length::Fixed(1.0)))
            .style(|_theme: &Theme| container::Style {
                background: Some(Background::Color(Color::from_rgb(0.30, 0.30, 0.35))),
                ..container::Style::default()
            })
            .width(Length::Fill)
            .into(),
    );

    // New project button
    items.push(
        button(
            row![
                text("+").size(14).shaping(Shaping::Advanced).color(Color::from_rgb(0.55, 0.75, 0.95)),
                text(t(locale, "projectSelector.newProject")).size(12).shaping(Shaping::Advanced),
            ]
            .spacing(6),
        )
        .on_press(Message::RequestCreateProject)
        .padding([4, 8])
        .width(Length::Fill)
        .style(new_project_btn)
        .into(),
    );

    container(
        Column::with_children(items)
            .spacing(2)
            .padding(4)
            .width(Length::Fixed(200.0)),
    )
    .style(dropdown_bg)
    .into()
}

// ---------------------------------------------------------------------------
// Trigger button for status bar (folder icon + project name + chevron)
// ---------------------------------------------------------------------------

pub fn trigger_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.22, 0.22, 0.27),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: Color::WHITE,
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 3.0.into(),
        },
        ..button::Style::default()
    }
}

pub fn trigger_button(project_name: &str) -> Element<'static, Message> {
    let folder = svg::Handle::from_memory(FOLDER_ICON);
    let folder_icon = svg(folder)
        .width(Length::Fixed(14.0))
        .height(Length::Fixed(14.0));

    let name = text(project_name.to_string())
        .size(12)
        .shaping(Shaping::Advanced)
        .color(Color::from_rgb(0.80, 0.80, 0.85));

    let chevron = text("\u{25BE}")
        .size(10)
        .shaping(Shaping::Advanced)
        .color(Color::from_rgb(0.55, 0.55, 0.60));

    button(
        row![folder_icon, name, chevron]
            .spacing(4)
            .align_y(iced::Alignment::Center),
    )
    .on_press(Message::ToggleProjectDropdown)
    .padding([2, 6])
    .style(trigger_style)
    .into()
}
