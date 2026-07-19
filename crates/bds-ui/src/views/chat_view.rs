use std::collections::HashMap;
use std::time::Instant;

use bds_core::engine::chat_surfaces::{
    ChatSurfaceState, InlineSurface, build_message_surfaces, build_render_surface,
};
use bds_core::i18n::UiLocale;
use bds_core::model::{ChatConversation, ChatMessage, ChatRole};
use iced::widget::text::Shaping;
use iced::widget::{
    Space, button, column, container, markdown, row, scrollable, text, text_editor, text_input,
};
use iced::{Alignment, Color, Element, Length};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::{t, tw};

pub struct ChatEditorState {
    pub conversation: ChatConversation,
    pub messages: Vec<ChatMessage>,
    rendered_messages: std::collections::HashMap<i32, Vec<markdown::Item>>,
    pub input: text_editor::Content,
    pub rename_input: String,
    pub model_options: Vec<ChatModelChoice>,
    pub streaming: bool,
    pub streaming_content: String,
    streaming_markdown: Vec<markdown::Item>,
    pub active_tool: Option<String>,
    pub error: Option<String>,
    pub surface_state: ChatSurfaceState,
    pub message_surfaces: HashMap<i32, Vec<InlineSurface>>,
    pub streaming_surfaces: Vec<InlineSurface>,
    pub surface_textareas: HashMap<String, text_editor::Content>,
    pub surface_state_dirty_since: Option<Instant>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatModelChoice {
    pub id: String,
    pub label: String,
}

impl std::fmt::Display for ChatModelChoice {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

impl ChatEditorState {
    pub fn new(
        conversation: ChatConversation,
        messages: Vec<ChatMessage>,
        mut model_options: Vec<ChatModelChoice>,
    ) -> Self {
        if let Some(model) = conversation.model.as_ref()
            && !model_options.iter().any(|choice| choice.id == *model)
        {
            model_options.push(ChatModelChoice {
                id: model.clone(),
                label: model.clone(),
            });
        }
        model_options.sort_by(|left, right| left.label.cmp(&right.label));
        model_options.dedup_by(|left, right| left.id == right.id);
        let rendered_messages = messages
            .iter()
            .filter(|message| message.role == ChatRole::Assistant)
            .map(|message| {
                (
                    message.id,
                    parse_safe_markdown(message.content.as_deref().unwrap_or_default()),
                )
            })
            .collect();
        let surface_state = conversation
            .surface_state
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok())
            .unwrap_or_default();
        let message_surfaces = build_surfaces(&messages, &surface_state);
        let surface_textareas = build_textareas(&message_surfaces);
        Self {
            rename_input: conversation.title.clone(),
            conversation,
            messages,
            rendered_messages,
            input: text_editor::Content::new(),
            model_options,
            streaming: false,
            streaming_content: String::new(),
            streaming_markdown: Vec::new(),
            active_tool: None,
            error: None,
            surface_state,
            message_surfaces,
            streaming_surfaces: Vec::new(),
            surface_textareas,
            surface_state_dirty_since: None,
        }
    }

    pub fn set_messages(&mut self, messages: Vec<ChatMessage>) {
        self.rendered_messages = messages
            .iter()
            .filter(|message| message.role == ChatRole::Assistant)
            .map(|message| {
                (
                    message.id,
                    parse_safe_markdown(message.content.as_deref().unwrap_or_default()),
                )
            })
            .collect();
        self.messages = messages;
        self.rebuild_surfaces();
    }

    pub fn set_streaming_content(&mut self, content: String) {
        self.streaming_markdown = parse_safe_markdown(&content);
        self.streaming_content = content;
    }

    pub fn clear_streaming(&mut self) {
        self.streaming_content.clear();
        self.streaming_markdown.clear();
        self.streaming_surfaces.clear();
    }

    pub fn add_streaming_surface(&mut self, name: &str, arguments: &serde_json::Value, id: String) {
        if let Some(surface) = build_render_surface(name, arguments, id, &self.surface_state)
            && !self.surface_state.dismissed_surfaces.contains(&surface.id)
        {
            add_textareas(&mut self.surface_textareas, &surface);
            self.streaming_surfaces.push(surface);
        }
    }

    pub fn rebuild_surfaces(&mut self) {
        self.message_surfaces = build_surfaces(&self.messages, &self.surface_state);
        self.surface_textareas = build_textareas(&self.message_surfaces);
        for surface in &self.streaming_surfaces {
            add_textareas(&mut self.surface_textareas, surface);
        }
    }

    pub fn token_totals(&self) -> (u64, u64, u64, u64) {
        self.messages
            .iter()
            .fold((0, 0, 0, 0), |mut total, message| {
                total.0 += message.token_usage_input.unwrap_or(0).max(0) as u64;
                total.1 += message.token_usage_output.unwrap_or(0).max(0) as u64;
                total.2 += message.cache_read_tokens.unwrap_or(0).max(0) as u64;
                total.3 += message.cache_write_tokens.unwrap_or(0).max(0) as u64;
                total
            })
    }
}

fn build_surfaces(
    messages: &[ChatMessage],
    state: &ChatSurfaceState,
) -> HashMap<i32, Vec<InlineSurface>> {
    messages
        .iter()
        .filter_map(|message| {
            let surfaces = build_message_surfaces(message, state);
            (!surfaces.is_empty()).then_some((message.id, surfaces))
        })
        .collect()
}

fn build_textareas(
    surfaces: &HashMap<i32, Vec<InlineSurface>>,
) -> HashMap<String, text_editor::Content> {
    let mut result = HashMap::new();
    for surface in surfaces.values().flatten() {
        add_textareas(&mut result, surface);
    }
    result
}

fn add_textareas(result: &mut HashMap<String, text_editor::Content>, surface: &InlineSurface) {
    for field in &surface.fields {
        if field.input_type == bds_core::engine::chat_surfaces::FormInputType::Textarea {
            result
                .entry(textarea_key(&surface.id, &field.key))
                .or_insert_with(|| {
                    text_editor::Content::with_text(field.value.as_str().unwrap_or_default())
                });
        }
    }
    for tab in &surface.tabs {
        for child in &tab.content {
            add_textareas(result, child);
        }
    }
}

pub fn textarea_key(surface_id: &str, field: &str) -> String {
    format!("{surface_id}\0{field}")
}

pub fn view<'a>(
    state: &'a ChatEditorState,
    locale: UiLocale,
    ai_available: bool,
) -> Element<'a, Message> {
    if !ai_available {
        return container(inputs::card(
            column![
                text(t(locale, "chat.unavailable.title")).size(20),
                text(t(locale, "chat.unavailable.guidance"))
                    .size(13)
                    .color(inputs::SECTION_COLOR),
                button(text(t(locale, "chat.unavailable.openSettings")))
                    .on_press(Message::OpenSettingsSection(
                        crate::views::settings_view::SettingsSection::AI,
                    ))
                    .style(inputs::primary_button),
            ]
            .spacing(12),
        ))
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
    }

    let selected_model = state.conversation.model.as_ref().and_then(|selected| {
        state
            .model_options
            .iter()
            .find(|model| model.id == *selected)
    });
    let model_control: Element<'a, Message> = if state.model_options.is_empty() {
        text(t(locale, "chat.model.none"))
            .size(12)
            .color(inputs::SECTION_COLOR)
            .into()
    } else {
        inputs::labeled_select(
            &t(locale, "chat.model.label"),
            &state.model_options,
            selected_model,
            |choice| Message::ChatModelChanged(choice.id),
        )
    };
    let header = inputs::card(
        column![
            row![
                text_input(&t(locale, "chat.rename.placeholder"), &state.rename_input)
                    .on_input(Message::ChatRenameInputChanged)
                    .on_submit(Message::ChatRename)
                    .size(18)
                    .padding([7, 9])
                    .style(inputs::field_style),
                button(text(t(locale, "chat.rename.action")))
                    .on_press(Message::ChatRename)
                    .padding([8, 12])
                    .style(inputs::secondary_button),
                button(text(t(locale, "common.delete")))
                    .on_press(Message::ChatDelete(state.conversation.id.clone()))
                    .padding([8, 12])
                    .style(inputs::danger_button),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            model_control,
        ]
        .spacing(10),
    );

    let mut message_elements: Vec<Element<'a, Message>> = Vec::new();
    if state.messages.is_empty() {
        message_elements.push(welcome(locale));
    } else {
        for message in &state.messages {
            if message.role == ChatRole::Tool {
                continue;
            }
            message_elements.push(message_view(
                message,
                state.rendered_messages.get(&message.id),
                locale,
            ));
            if let Some(surfaces) = state.message_surfaces.get(&message.id) {
                for surface in surfaces {
                    message_elements.push(crate::views::chat_surfaces::view(
                        surface,
                        &state.surface_state,
                        &state.surface_textareas,
                        locale,
                    ));
                }
            }
            if let Some(markers) = tool_markers_view(message, locale) {
                message_elements.push(markers);
            }
        }
    }
    if state.streaming && !state.streaming_content.is_empty() {
        message_elements.push(assistant_card(
            &state.streaming_markdown,
            t(locale, "chat.streaming"),
        ));
    }
    for surface in &state.streaming_surfaces {
        message_elements.push(crate::views::chat_surfaces::view(
            surface,
            &state.surface_state,
            &state.surface_textareas,
            locale,
        ));
    }
    if let Some(tool) = state.active_tool.as_deref() {
        message_elements.push(
            inputs::card(
                row![
                    text("⚙").size(13),
                    text(tw(locale, "chat.tool.running", &[("name", tool)]))
                        .size(12)
                        .color(inputs::SECTION_COLOR),
                ]
                .spacing(8),
            )
            .into(),
        );
    }
    if let Some(error) = state.error.as_deref() {
        message_elements.push(
            text(error.to_string())
                .size(12)
                .color(Color::from_rgb8(0xE0, 0x6C, 0x75))
                .into(),
        );
    }
    let transcript = scrollable(
        iced::widget::Column::with_children(message_elements)
            .spacing(10)
            .width(Length::Fill),
    )
    .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
    .style(inputs::scrollable_style)
    .anchor_bottom()
    .height(Length::Fill);

    let send_label = if state.streaming {
        t(locale, "chat.stop")
    } else {
        t(locale, "chat.send")
    };
    let mut send_button = button(text(send_label))
        .padding([8, 16])
        .style(if state.streaming {
            inputs::danger_button
        } else {
            inputs::primary_button
        });
    if state.streaming {
        send_button = send_button.on_press(Message::ChatCancel);
    } else if !state.input.text().trim().is_empty() {
        send_button = send_button.on_press(Message::ChatSend);
    }
    let input_height =
        (state.input.text().lines().count().max(1) as f32 * 22.0 + 28.0).clamp(72.0, 200.0);
    let input_placeholder = t(locale, "chat.input.placeholder");
    let composer = inputs::card(
        column![
            text_editor(&state.input)
                .placeholder(input_placeholder)
                .on_action(Message::ChatInputAction)
                .key_binding(|key_press| {
                    if matches!(
                        key_press.key,
                        iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter)
                    ) && !key_press.modifiers.shift()
                    {
                        Some(iced::widget::text_editor::Binding::Custom(
                            Message::ChatSend,
                        ))
                    } else {
                        iced::widget::text_editor::Binding::from_key_press(key_press)
                    }
                })
                .height(Length::Fixed(input_height))
                .style(inputs::text_editor_style),
            row![
                text(t(locale, "chat.input.hint"))
                    .size(11)
                    .color(inputs::SECTION_COLOR),
                Space::with_width(Length::Fill),
                send_button,
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(8),
    );

    column![header, transcript, composer]
        .spacing(12)
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn welcome(locale: UiLocale) -> Element<'static, Message> {
    let mut items = vec![
        text(t(locale, "chat.welcome.title")).size(20).into(),
        text(t(locale, "chat.welcome.subtitle"))
            .size(13)
            .color(inputs::SECTION_COLOR)
            .into(),
    ];
    for index in 1..=5 {
        items.push(
            text(format!(
                "• {}",
                t(locale, &format!("chat.welcome.tip{index}"))
            ))
            .size(13)
            .into(),
        );
    }
    inputs::card(iced::widget::Column::with_children(items).spacing(7)).into()
}

fn message_view<'a>(
    message: &'a ChatMessage,
    rendered_markdown: Option<&'a Vec<markdown::Item>>,
    locale: UiLocale,
) -> Element<'a, Message> {
    let label = match message.role {
        ChatRole::User => t(locale, "chat.role.user"),
        ChatRole::Assistant => t(locale, "chat.role.assistant"),
        ChatRole::System => t(locale, "chat.role.system"),
        ChatRole::Tool => t(locale, "chat.role.tool"),
    };
    let content = message.content.as_deref().unwrap_or_default();
    let body: Element<'a, Message> = if let Some(items) = rendered_markdown {
        markdown::view(
            items,
            markdown::Settings::default(),
            markdown::Style::from_palette(crate::components::inputs::app_theme().palette()),
        )
        .map(|url| Message::ChatLinkClicked(url.to_string()))
    } else {
        text(content.to_string())
            .size(14)
            .shaping(Shaping::Advanced)
            .into()
    };
    let children: Vec<Element<'a, Message>> = vec![
        text(label).size(11).color(inputs::SECTION_COLOR).into(),
        body,
    ];
    inputs::card(iced::widget::Column::with_children(children).spacing(6)).into()
}

fn tool_markers_view<'a>(
    message: &'a ChatMessage,
    locale: UiLocale,
) -> Option<Element<'a, Message>> {
    let mut markers = Vec::new();
    if let Some(tool_calls) = message.tool_calls.as_deref()
        && let Ok(calls) = serde_json::from_str::<Vec<serde_json::Value>>(tool_calls)
    {
        for call in calls {
            let raw_name = call
                .get("function")
                .and_then(|function| function.get("name"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| t(locale, "chat.role.tool"));
            let name = raw_name.chars().take(30).collect::<String>();
            let arguments = call
                .get("function")
                .and_then(|function| function.get("arguments"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let args_preview = arguments.chars().take(30).collect::<String>();
            let marker = if args_preview.is_empty() {
                format!("✓ {}", tw(locale, "chat.tool.used", &[("name", &name)]))
            } else {
                format!(
                    "✓ {} · {}",
                    tw(locale, "chat.tool.used", &[("name", &name)]),
                    args_preview
                )
            };
            markers.push(text(marker).size(11).color(inputs::SECTION_COLOR).into());
        }
    }
    (!markers.is_empty())
        .then(|| inputs::card(iced::widget::Column::with_children(markers).spacing(4)).into())
}

fn assistant_card<'a>(items: &'a [markdown::Item], label: String) -> Element<'a, Message> {
    let body = markdown::view(
        items,
        markdown::Settings::default(),
        markdown::Style::from_palette(crate::components::inputs::app_theme().palette()),
    )
    .map(|url| Message::ChatLinkClicked(url.to_string()));
    inputs::card(column![text(label).size(11).color(inputs::SECTION_COLOR), body,].spacing(6))
        .into()
}

fn parse_safe_markdown(markdown_source: &str) -> Vec<markdown::Item> {
    let safe = external_images_as_links(markdown_source);
    markdown::parse(&safe).collect()
}

fn external_images_as_links(markdown_source: &str) -> String {
    static EXTERNAL_IMAGE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let pattern = EXTERNAL_IMAGE.get_or_init(|| {
        regex::Regex::new(r"!\[([^\]]*)\]\((https?://[^)\s]+)\)")
            .expect("external image Markdown regex")
    });
    pattern
        .replace_all(markdown_source, "[🖼 $1]($2)")
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_never_embeds_external_images_or_raw_html() {
        let rendered = parse_safe_markdown(
            "# Hello\n![secret](https://example.com/a.png)<script>unsafe</script>",
        );
        let debug = format!("{rendered:?}");
        assert!(debug.contains("Hello"));
        assert!(debug.contains("secret"));
        assert!(!debug.contains("script"));
        assert!(debug.contains("unsafe"));
    }

    #[test]
    fn external_images_become_safe_links_before_gfm_rendering() {
        let safe = external_images_as_links("![diagram](https://example.com/a.png)");
        assert_eq!(safe, "[🖼 diagram](https://example.com/a.png)");
        assert_eq!(parse_safe_markdown(&safe).len(), 1);
    }
}
