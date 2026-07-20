use iced::widget::text::Shaping;
use iced::widget::{Column, Space, button, column, container, row, scrollable, text};
use iced::{Background, Border, Color, Element, Length, Radians, Theme, gradient};

use bds_core::i18n::UiLocale;

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

const DEFAULT_THEME: &str = "default";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewMode {
    Auto,
    Light,
    Dark,
}

impl PreviewMode {
    pub fn query_value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreviewModeOption {
    mode: PreviewMode,
    label: String,
}

impl std::fmt::Display for PreviewModeOption {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleTheme {
    pub name: &'static str,
    pub accent_color: &'static str,
    pub light_bg_color: &'static str,
    pub dark_bg_color: &'static str,
}

const fn theme(name: &'static str, accent_color: &'static str) -> StyleTheme {
    StyleTheme {
        name,
        accent_color,
        light_bg_color: "#ffffff",
        dark_bg_color: "#13171f",
    }
}

pub const THEMES: [StyleTheme; 20] = [
    theme("default", "#0172ad"),
    theme("amber", "#ffbf00"),
    theme("blue", "#2060df"),
    theme("cyan", "#047878"),
    theme("fuchsia", "#c1208b"),
    theme("green", "#398712"),
    theme("grey", "#ababab"),
    theme("indigo", "#524ed2"),
    theme("jade", "#007a50"),
    theme("lime", "#a5d601"),
    theme("orange", "#d24317"),
    theme("pink", "#d92662"),
    theme("pumpkin", "#ff9500"),
    theme("purple", "#9236a4"),
    theme("red", "#c52f21"),
    theme("sand", "#ccc6b4"),
    theme("slate", "#525f7a"),
    theme("violet", "#7540bf"),
    theme("yellow", "#f2df0d"),
    theme("zinc", "#646b79"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyleViewState {
    pub selected_theme: String,
    pub applied_theme: String,
    pub preview_mode: PreviewMode,
}

impl StyleViewState {
    pub fn new(applied_theme: Option<&str>) -> Self {
        let applied_theme = normalize_theme(applied_theme);
        Self {
            selected_theme: applied_theme.clone(),
            applied_theme,
            preview_mode: PreviewMode::Auto,
        }
    }

    pub fn select_theme(&mut self, theme: &str) {
        if is_supported_theme(theme) {
            self.selected_theme = theme.to_string();
        }
    }

    pub fn set_preview_mode(&mut self, mode: PreviewMode) {
        self.preview_mode = mode;
    }

    pub fn can_apply(&self) -> bool {
        self.selected_theme != self.applied_theme
    }

    pub fn mark_applied(&mut self) {
        self.applied_theme.clone_from(&self.selected_theme);
    }

    pub fn refresh_applied_theme(&mut self, applied_theme: Option<&str>) {
        let preserve_pending_selection = self.can_apply();
        self.applied_theme = normalize_theme(applied_theme);
        if !preserve_pending_selection {
            self.selected_theme.clone_from(&self.applied_theme);
        }
    }

    pub fn preview_url(&self) -> String {
        format!(
            "http://{}:{}/__style-preview?theme={}&mode={}",
            bds_core::engine::preview::PREVIEW_HOST,
            bds_core::engine::preview::PREVIEW_PORT,
            self.selected_theme,
            self.preview_mode.query_value(),
        )
    }
}

#[derive(Debug, Clone)]
pub enum StyleMsg {
    SelectTheme(String),
    PreviewModeChanged(PreviewMode),
    Apply,
}

pub fn is_supported_theme(theme: &str) -> bool {
    THEMES.iter().any(|candidate| candidate.name == theme)
}

pub fn display_theme_name(theme: &str) -> String {
    let replaced = theme.replace('-', " ");
    let mut characters = replaced.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

fn normalize_theme(theme: Option<&str>) -> String {
    theme
        .filter(|theme| is_supported_theme(theme))
        .unwrap_or(DEFAULT_THEME)
        .to_string()
}

fn parse_hex_color(hex: &str) -> Color {
    let bytes = hex.as_bytes();
    if bytes.len() == 7 && bytes[0] == b'#' {
        let component = |start| u8::from_str_radix(&hex[start..start + 2], 16).unwrap_or_default();
        Color::from_rgb8(component(1), component(3), component(5))
    } else {
        Color::TRANSPARENT
    }
}

fn theme_button_style(selected: bool, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    button::Style {
        background: Some(Background::Color(if selected {
            Color::from_rgb8(0x1F, 0x45, 0x63)
        } else if hovered {
            Color::from_rgb8(0x32, 0x32, 0x35)
        } else {
            Color::from_rgb8(0x24, 0x24, 0x26)
        })),
        text_color: Color::from_rgb8(0xE4, 0xE4, 0xE4),
        border: Border {
            color: if selected {
                Color::from_rgb8(0x00, 0x7F, 0xD4)
            } else {
                Color::from_rgb8(0x3C, 0x3C, 0x3C)
            },
            width: if selected { 2.0 } else { 1.0 },
            radius: 6.0.into(),
        },
        ..button::Style::default()
    }
}

fn theme_accent_background(theme: &StyleTheme) -> Background {
    gradient::Linear::new(Radians(3.0 * std::f32::consts::FRAC_PI_4))
        .add_stop(0.0, parse_hex_color(theme.accent_color))
        .add_stop(1.0, parse_hex_color(theme.dark_bg_color))
        .into()
}

fn swatch(background: Background, width_portion: u16) -> Element<'static, Message> {
    container(Space::new(Length::Fill, Length::Fill))
        .width(Length::FillPortion(width_portion))
        .height(Length::Fixed(30.0))
        .style(move |_: &Theme| container::Style {
            background: Some(background),
            border: Border {
                color: Color::from_rgba8(0xff, 0xff, 0xff, 0.08),
                width: 1.0,
                radius: 5.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

fn theme_button<'a>(theme: &StyleTheme, selected_theme: &str) -> Element<'a, Message> {
    let selected = theme.name == selected_theme;
    let theme_name = theme.name.to_string();
    button(
        column![
            row![
                swatch(theme_accent_background(theme), 2),
                swatch(Background::Color(parse_hex_color(theme.light_bg_color)), 1),
                swatch(Background::Color(parse_hex_color(theme.dark_bg_color)), 1),
            ]
            .spacing(4),
            text(display_theme_name(theme.name))
                .size(12)
                .shaping(Shaping::Advanced),
        ]
        .spacing(7)
        .width(Length::Fill),
    )
    .on_press(Message::Style(StyleMsg::SelectTheme(theme_name)))
    .padding(8)
    .width(Length::FillPortion(1))
    .style(move |_: &Theme, status| theme_button_style(selected, status))
    .into()
}

pub fn view<'a>(
    state: &'a StyleViewState,
    locale: UiLocale,
    preview_widget: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let header = inputs::card(
        column![
            text(t(locale, "style.title"))
                .size(18)
                .shaping(Shaping::Advanced),
            text(t(locale, "style.subtitle"))
                .size(12)
                .color(inputs::LABEL_COLOR)
                .shaping(Shaping::Advanced),
        ]
        .spacing(4),
    );

    let theme_rows = THEMES
        .chunks(4)
        .map(|themes| {
            let mut children = themes
                .iter()
                .map(|theme| theme_button(theme, &state.selected_theme))
                .collect::<Vec<_>>();
            while children.len() < 4 {
                children.push(Space::with_width(Length::FillPortion(1)).into());
            }
            iced::widget::Row::with_children(children)
                .spacing(8)
                .width(Length::Fill)
                .into()
        })
        .collect::<Vec<Element<'a, Message>>>();
    let picker = inputs::card(
        column![
            text(t(locale, "style.themes"))
                .size(12)
                .color(inputs::SECTION_COLOR)
                .shaping(Shaping::Advanced),
            scrollable(Column::with_children(theme_rows).spacing(8))
                .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
                .style(inputs::scrollable_style)
                .height(Length::Fixed(250.0)),
        ]
        .spacing(8),
    );

    let preview_modes = [
        PreviewModeOption {
            mode: PreviewMode::Auto,
            label: t(locale, "style.previewMode.auto"),
        },
        PreviewModeOption {
            mode: PreviewMode::Light,
            label: t(locale, "style.previewMode.light"),
        },
        PreviewModeOption {
            mode: PreviewMode::Dark,
            label: t(locale, "style.previewMode.dark"),
        },
    ];
    let selected_mode = preview_modes
        .iter()
        .find(|option| option.mode == state.preview_mode);
    let mode_select = container(inputs::labeled_select(
        &t(locale, "style.previewMode"),
        &preview_modes,
        selected_mode,
        |option| Message::Style(StyleMsg::PreviewModeChanged(option.mode)),
    ))
    .width(Length::Fixed(220.0));
    let apply_label = text(t(locale, "style.apply"))
        .size(13)
        .shaping(Shaping::Advanced);
    let apply_button: Element<'a, Message> = if state.can_apply() {
        button(apply_label)
            .on_press(Message::Style(StyleMsg::Apply))
            .padding([8, 16])
            .style(inputs::primary_button)
            .into()
    } else {
        button(apply_label)
            .padding([8, 16])
            .style(inputs::primary_button)
            .into()
    };
    let controls = inputs::toolbar(vec![mode_select.into()], vec![apply_button]);

    let preview = preview_widget.unwrap_or_else(|| {
        container(
            text(t(locale, "tabBar.loading"))
                .size(14)
                .shaping(Shaping::Advanced),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    });
    let preview_card = inputs::card(
        column![
            text(t(locale, "style.preview"))
                .size(12)
                .color(inputs::SECTION_COLOR)
                .shaping(Shaping::Advanced),
            preview,
        ]
        .spacing(8)
        .height(Length::Fill),
    )
    .height(Length::Fill);

    container(
        column![header, picker, controls, preview_card]
            .spacing(10)
            .height(Length::Fill),
    )
    .padding(16)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_match_the_allium_and_bds2_catalog() {
        assert_eq!(THEMES.len(), 20);
        assert_eq!(THEMES.first().unwrap().name, "default");
        assert_eq!(THEMES.last().unwrap().name, "zinc");
        assert_eq!(
            THEMES.iter().map(|theme| theme.name).collect::<Vec<_>>(),
            bds_core::model::SUPPORTED_PICO_THEMES.to_vec()
        );
        assert_eq!(THEMES[1].accent_color, "#ffbf00");
        assert_eq!(THEMES[2].accent_color, "#2060df");
        assert_eq!(THEMES[18].accent_color, "#f2df0d");
        assert_eq!(THEMES[19].accent_color, "#646b79");
        assert_eq!(
            THEMES
                .iter()
                .map(|theme| theme.accent_color)
                .collect::<std::collections::HashSet<_>>()
                .len(),
            THEMES.len()
        );
        assert!(THEMES.iter().all(|theme| {
            theme.light_bg_color == "#ffffff" && theme.dark_bg_color == "#13171f"
        }));
    }

    #[test]
    fn theme_swatches_use_distinct_accent_gradients() {
        let backgrounds = THEMES
            .iter()
            .map(theme_accent_background)
            .collect::<Vec<_>>();

        assert!(
            backgrounds
                .iter()
                .all(|background| matches!(background, Background::Gradient(_)))
        );
        for (index, background) in backgrounds.iter().enumerate() {
            assert!(!backgrounds[..index].contains(background));
        }
    }

    #[test]
    fn preview_url_tracks_selection_and_local_mode() {
        let mut state = StyleViewState::new(Some("blue"));
        state.select_theme("pumpkin");

        assert_eq!(
            state.preview_url(),
            "http://127.0.0.1:4123/__style-preview?theme=pumpkin&mode=auto"
        );
        state.set_preview_mode(PreviewMode::Light);
        assert_eq!(
            state.preview_url(),
            "http://127.0.0.1:4123/__style-preview?theme=pumpkin&mode=light"
        );
        state.set_preview_mode(PreviewMode::Dark);
        assert_eq!(
            state.preview_url(),
            "http://127.0.0.1:4123/__style-preview?theme=pumpkin&mode=dark"
        );
        assert_eq!(state.applied_theme, "blue");
    }

    #[test]
    fn invalid_or_empty_applied_theme_uses_default() {
        assert_eq!(StyleViewState::new(None).applied_theme, "default");
        assert_eq!(StyleViewState::new(Some("")).applied_theme, "default");
        assert_eq!(
            StyleViewState::new(Some("unknown")).applied_theme,
            "default"
        );
    }
}
