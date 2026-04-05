use iced::widget::{button, column, container, row, scrollable, text, text_input, text_editor, Space};
use iced::widget::text::{Shaping, Wrapping};
use iced::{Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::{Post, PostStatus};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

/// State for an open post editor.
pub struct PostEditorState {
    pub post_id: String,
    pub title: String,
    pub slug: String,
    pub excerpt: String,
    pub content: String,
    pub editor_content: text_editor::Content,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    pub author: String,
    pub language: String,
    pub template_slug: String,
    pub do_not_translate: bool,
    pub status: PostStatus,
    pub created_at: i64,
    pub updated_at: i64,
    pub is_dirty: bool,
    pub metadata_expanded: bool,
    pub excerpt_expanded: bool,
    pub tags_input: String,
    pub categories_input: String,
}

impl std::fmt::Debug for PostEditorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostEditorState")
            .field("post_id", &self.post_id)
            .field("title", &self.title)
            .finish_non_exhaustive()
    }
}

impl Clone for PostEditorState {
    fn clone(&self) -> Self {
        Self {
            post_id: self.post_id.clone(),
            title: self.title.clone(),
            slug: self.slug.clone(),
            excerpt: self.excerpt.clone(),
            content: self.content.clone(),
            editor_content: text_editor::Content::with_text(&self.content),
            tags: self.tags.clone(),
            categories: self.categories.clone(),
            author: self.author.clone(),
            language: self.language.clone(),
            template_slug: self.template_slug.clone(),
            do_not_translate: self.do_not_translate,
            status: self.status.clone(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            is_dirty: self.is_dirty,
            metadata_expanded: self.metadata_expanded,
            excerpt_expanded: self.excerpt_expanded,
            tags_input: self.tags_input.clone(),
            categories_input: self.categories_input.clone(),
        }
    }
}

impl PostEditorState {
    pub fn from_post(post: &Post) -> Self {
        let title = post.title.clone();
        let content = post.content.clone().unwrap_or_default();
        Self {
            post_id: post.id.clone(),
            slug: post.slug.clone(),
            excerpt: post.excerpt.clone().unwrap_or_default(),
            content: content.clone(),
            editor_content: text_editor::Content::with_text(&content),
            tags: post.tags.clone(),
            categories: post.categories.clone(),
            author: post.author.clone().unwrap_or_default(),
            language: post.language.clone().unwrap_or_default(),
            template_slug: post.template_slug.clone().unwrap_or_default(),
            do_not_translate: post.do_not_translate,
            status: post.status.clone(),
            created_at: post.created_at,
            updated_at: post.updated_at,
            is_dirty: false,
            metadata_expanded: title.is_empty(),
            excerpt_expanded: false,
            tags_input: String::new(),
            categories_input: String::new(),
            title,
        }
    }
}

/// Post editor messages.
#[derive(Debug, Clone)]
pub enum PostEditorMsg {
    TitleChanged(String),
    SlugChanged(String),
    ExcerptChanged(String),
    ContentAction(text_editor::Action),
    AuthorChanged(String),
    TemplateSlugChanged(String),
    ToggleDoNotTranslate(bool),
    ToggleMetadata,
    ToggleExcerpt,
    TagsInputChanged(String),
    TagsInputSubmit,
    RemoveTag(String),
    CategoriesInputChanged(String),
    CategoriesInputSubmit,
    RemoveCategory(String),
    Save,
    Publish,
    Delete,
}

/// Render the post editor view.
pub fn view<'a>(
    state: &'a PostEditorState,
    locale: UiLocale,
) -> Element<'a, Message> {
    // ── Header bar ──
    let dirty_indicator = if state.is_dirty { " \u{25CF}" } else { "" };
    let title_display = if state.title.is_empty() {
        t(locale, "editor.untitled")
    } else {
        format!("{}{}", state.title, dirty_indicator)
    };

    let header = inputs::toolbar(
        vec![
            text(title_display)
                .size(18)
                .wrapping(Wrapping::None)
                .shaping(Shaping::Advanced)
                .into(),
        ],
        vec![
            status_badge(&state.status),
            button(text(t(locale, "common.save")).size(13).shaping(Shaping::Advanced))
                .on_press(Message::PostEditor(PostEditorMsg::Save))
                .style(inputs::primary_button)
                .padding([6, 16])
                .into(),
            if state.status == PostStatus::Draft {
                button(text(t(locale, "editor.publish")).size(13).shaping(Shaping::Advanced))
                    .on_press(Message::PostEditor(PostEditorMsg::Publish))
                    .style(inputs::primary_button)
                    .padding([6, 16])
                    .into()
            } else {
                Space::new(0, 0).into()
            },
            button(text(t(locale, "modal.confirmDelete.delete")).size(13).shaping(Shaping::Advanced))
                .on_press(Message::PostEditor(PostEditorMsg::Delete))
                .style(inputs::danger_button)
                .padding([6, 16])
                .into(),
        ],
    );

    // ── Collapsible Metadata Section ──
    let meta_toggle_label = if state.metadata_expanded {
        format!("\u{25BC} {}", t(locale, "editor.metadata"))
    } else {
        format!("\u{25B6} {}", t(locale, "editor.metadata"))
    };
    let meta_toggle = button(
        text(meta_toggle_label).size(12).color(inputs::SECTION_COLOR).shaping(Shaping::Advanced),
    )
    .on_press(Message::PostEditor(PostEditorMsg::ToggleMetadata))
    .padding([4, 0])
    .style(|_, _| button::Style {
        background: None,
        ..button::Style::default()
    });

    let metadata_section: Element<'a, Message> = if state.metadata_expanded {
        let title_input = inputs::labeled_input(
            &t(locale, "editor.title"),
            &t(locale, "editor.titlePlaceholder"),
            &state.title,
            |s| Message::PostEditor(PostEditorMsg::TitleChanged(s)),
        );
        let slug_input = inputs::labeled_input(
            &t(locale, "editor.slug"),
            &t(locale, "editor.slugPlaceholder"),
            &state.slug,
            |s| Message::PostEditor(PostEditorMsg::SlugChanged(s)),
        );
        let meta_row1 = row![title_input, slug_input]
            .spacing(16)
            .width(Length::Fill);

        let author_input = inputs::labeled_input(
            &t(locale, "editor.author"),
            "",
            &state.author,
            |s| Message::PostEditor(PostEditorMsg::AuthorChanged(s)),
        );
        let template_input = inputs::labeled_input(
            &t(locale, "editor.templateSlug"),
            "",
            &state.template_slug,
            |s| Message::PostEditor(PostEditorMsg::TemplateSlugChanged(s)),
        );
        let dnt = inputs::labeled_checkbox(
            &t(locale, "editor.doNotTranslate"),
            state.do_not_translate,
            |b| Message::PostEditor(PostEditorMsg::ToggleDoNotTranslate(b)),
        );
        let meta_row2 = row![author_input, template_input, dnt]
            .spacing(16)
            .width(Length::Fill);

        // Tags chip input
        let tags_section = chip_input_field(
            &t(locale, "editor.tags"),
            &t(locale, "editor.tagsPlaceholder"),
            &state.tags,
            &state.tags_input,
            |s| Message::PostEditor(PostEditorMsg::TagsInputChanged(s)),
            Message::PostEditor(PostEditorMsg::TagsInputSubmit),
            |tag| Message::PostEditor(PostEditorMsg::RemoveTag(tag)),
        );

        // Categories chip input
        let categories_section = chip_input_field(
            &t(locale, "editor.categories"),
            &t(locale, "editor.categoriesPlaceholder"),
            &state.categories,
            &state.categories_input,
            |s| Message::PostEditor(PostEditorMsg::CategoriesInputChanged(s)),
            Message::PostEditor(PostEditorMsg::CategoriesInputSubmit),
            |cat| Message::PostEditor(PostEditorMsg::RemoveCategory(cat)),
        );

        column![meta_row1, meta_row2, tags_section, categories_section]
            .spacing(8)
            .width(Length::Fill)
            .into()
    } else {
        Space::new(0, 0).into()
    };

    // ── Collapsible Excerpt Section ──
    let excerpt_toggle_label = if state.excerpt_expanded {
        format!("\u{25BC} {}", t(locale, "editor.excerpt"))
    } else {
        format!("\u{25B6} {}", t(locale, "editor.excerpt"))
    };
    let excerpt_toggle = button(
        text(excerpt_toggle_label).size(12).color(inputs::SECTION_COLOR).shaping(Shaping::Advanced),
    )
    .on_press(Message::PostEditor(PostEditorMsg::ToggleExcerpt))
    .padding([4, 0])
    .style(|_, _| button::Style {
        background: None,
        ..button::Style::default()
    });

    let excerpt_section: Element<'a, Message> = if state.excerpt_expanded {
        inputs::labeled_input(
            &t(locale, "editor.excerpt"),
            &t(locale, "editor.excerptPlaceholder"),
            &state.excerpt,
            |s| Message::PostEditor(PostEditorMsg::ExcerptChanged(s)),
        )
    } else {
        Space::new(0, 0).into()
    };

    // ── Content section (fills remaining space) ──
    let content_placeholder = t(locale, "editor.contentPlaceholder");
    let content_label = inputs::section_header(&t(locale, "editor.content"));
    let editor_widget = text_editor(&state.editor_content)
        .placeholder(content_placeholder)
        .on_action(|action| Message::PostEditor(PostEditorMsg::ContentAction(action)))
        .height(Length::Fill)
        .style(editor_style);

    // ── Footer ──
    let footer = row![
        inputs::date_label(&t(locale, "editor.createdAt"), state.created_at),
        Space::with_width(Length::Fixed(24.0)),
        inputs::date_label(&t(locale, "editor.updatedAt"), state.updated_at),
    ]
    .padding(8);

    // ── Top pane: header + collapsible sections (scrollable for overflow) ──
    let top_pane = scrollable(
        column![
            header,
            meta_toggle,
            metadata_section,
            excerpt_toggle,
            excerpt_section,
        ]
        .spacing(4)
        .width(Length::Fill)
    )
    .height(Length::Shrink);

    // ── Full layout: top pane (shrink), editor (fill), footer (shrink) ──
    column![
        top_pane,
        content_label,
        editor_widget,
        footer,
    ]
    .spacing(4)
    .padding(16)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Dark editor style for the text_editor widget.
fn editor_style(_theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let bg = Color::from_rgb(0.10, 0.11, 0.14);
    let border_color = match status {
        text_editor::Status::Focused => Color::from_rgb(0.30, 0.50, 0.80),
        text_editor::Status::Hovered => Color::from_rgb(0.30, 0.32, 0.40),
        _ => Color::from_rgb(0.22, 0.24, 0.30),
    };
    text_editor::Style {
        background: iced::Background::Color(bg),
        border: iced::Border {
            radius: 4.0.into(),
            width: 1.0,
            color: border_color,
        },
        icon: Color::from_rgb(0.50, 0.52, 0.58),
        placeholder: Color::from_rgb(0.40, 0.42, 0.48),
        value: Color::from_rgb(0.85, 0.87, 0.92),
        selection: Color::from_rgba(0.30, 0.50, 0.80, 0.40),
    }
}

/// Chip input: shows existing chips as removable buttons + a text input to add new ones.
fn chip_input_field<'a>(
    label: &str,
    placeholder: &str,
    chips: &[String],
    input_value: &str,
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Message,
    on_remove: impl Fn(String) -> Message + 'a,
) -> Element<'a, Message> {
    let chip_elements: Vec<Element<'a, Message>> = chips
        .iter()
        .map(|chip| {
            let label = format!("{} \u{2715}", chip);
            let chip_val = chip.clone();
            button(text(label).size(11).shaping(Shaping::Advanced))
                .on_press(on_remove(chip_val))
                .padding([2, 6])
                .style(chip_button_style)
                .into()
        })
        .collect();

    let mut chip_row = row![].spacing(4);
    for el in chip_elements {
        chip_row = chip_row.push(el);
    }

    column![
        text(label.to_string()).size(12).color(inputs::LABEL_COLOR).shaping(Shaping::Advanced),
        chip_row.wrap(),
        text_input(placeholder, input_value)
            .on_input(on_input)
            .on_submit(on_submit)
            .size(13)
            .padding([4, 6]),
    ]
    .spacing(4)
    .width(Length::Fill)
    .into()
}

fn chip_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.30, 0.35, 0.45),
        _ => Color::from_rgb(0.22, 0.25, 0.32),
    };
    button::Style {
        background: Some(iced::Background::Color(bg)),
        text_color: Color::from_rgb(0.80, 0.82, 0.88),
        border: iced::Border {
            radius: 3.0.into(),
            ..iced::Border::default()
        },
        ..button::Style::default()
    }
}

fn status_badge<'a>(status: &PostStatus) -> Element<'a, Message> {
    let (label, color) = match status {
        PostStatus::Draft => ("Draft", Color::from_rgb(0.8, 0.7, 0.2)),
        PostStatus::Published => ("Published", Color::from_rgb(0.2, 0.7, 0.3)),
        PostStatus::Archived => ("Archived", Color::from_rgb(0.5, 0.5, 0.5)),
    };
    container(
        text(label)
            .size(11)
            .color(color)
            .wrapping(Wrapping::None)
            .shaping(Shaping::Advanced),
    )
    .padding([2, 8])
    .style(move |_: &Theme| container::Style {
        border: iced::Border {
            radius: 4.0.into(),
            width: 1.0,
            color,
        },
        ..container::Style::default()
    })
    .into()
}
