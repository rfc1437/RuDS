use iced::widget::{button, column, container, row, scrollable, text, text_input, Space};
use iced::{Color, Element, Length, Theme};

use bds_core::i18n::UiLocale;
use bds_core::model::{Post, PostStatus};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

/// State for an open post editor.
#[derive(Debug, Clone)]
pub struct PostEditorState {
    pub post_id: String,
    pub title: String,
    pub slug: String,
    pub excerpt: String,
    pub content: String,
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
}

impl PostEditorState {
    pub fn from_post(post: &Post) -> Self {
        Self {
            post_id: post.id.clone(),
            title: post.title.clone(),
            slug: post.slug.clone(),
            excerpt: post.excerpt.clone().unwrap_or_default(),
            content: post.content.clone().unwrap_or_default(),
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
        }
    }
}

/// Post editor messages.
#[derive(Debug, Clone)]
pub enum PostEditorMsg {
    TitleChanged(String),
    SlugChanged(String),
    ExcerptChanged(String),
    ContentChanged(String),
    AuthorChanged(String),
    TemplateSlugChanged(String),
    ToggleDoNotTranslate(bool),
    Save,
    Publish,
    Delete,
}

/// Render the post editor view.
pub fn view<'a>(
    state: &'a PostEditorState,
    locale: UiLocale,
) -> Element<'a, Message> {
    let header = inputs::toolbar(
        vec![
            text(state.title.clone()).size(18).into(),
            status_badge(&state.status),
        ],
        vec![
            button(text(t(locale, "common.save")).size(13))
                .on_press(Message::PostEditor(PostEditorMsg::Save))
                .style(inputs::primary_button)
                .padding([6, 16])
                .into(),
            button(text(t(locale, "editor.publish")).size(13))
                .on_press(Message::PostEditor(PostEditorMsg::Publish))
                .style(inputs::primary_button)
                .padding([6, 16])
                .into(),
            button(text(t(locale, "modal.confirmDelete.delete")).size(13))
                .on_press(Message::PostEditor(PostEditorMsg::Delete))
                .style(inputs::danger_button)
                .padding([6, 16])
                .into(),
        ],
    );

    // Metadata section
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
    let meta_row1 = row![title_input, slug_input].spacing(16).width(Length::Fill);

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
    let meta_row2 = row![author_input, template_input, dnt].spacing(16).width(Length::Fill);

    // Excerpt
    let excerpt_input = inputs::labeled_input(
        &t(locale, "editor.excerpt"),
        &t(locale, "editor.excerptPlaceholder"),
        &state.excerpt,
        |s| Message::PostEditor(PostEditorMsg::ExcerptChanged(s)),
    );

    // Content (text area placeholder — full editor will use CodeEditor widget)
    let content_section: Element<'a, Message> = column![
        inputs::section_header(&t(locale, "editor.content")),
        container(
            text_input(&t(locale, "editor.contentPlaceholder"), &state.content)
                .on_input(|s| Message::PostEditor(PostEditorMsg::ContentChanged(s)))
                .size(14)
        )
        .width(Length::Fill)
        .height(Length::Fixed(300.0)),
    ]
    .spacing(8)
    .width(Length::Fill)
    .into();

    // Footer
    let footer = row![
        inputs::date_label(&t(locale, "editor.createdAt"), state.created_at),
        Space::with_width(Length::Fixed(24.0)),
        inputs::date_label(&t(locale, "editor.updatedAt"), state.updated_at),
    ]
    .padding(8);

    let body = scrollable(
        column![
            header,
            meta_row1,
            meta_row2,
            excerpt_input,
            content_section,
            footer,
        ]
        .spacing(12)
        .padding(16)
        .width(Length::Fill)
    );

    container(body)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn status_badge<'a>(status: &PostStatus) -> Element<'a, Message> {
    let (label, color) = match status {
        PostStatus::Draft => ("Draft", Color::from_rgb(0.8, 0.7, 0.2)),
        PostStatus::Published => ("Published", Color::from_rgb(0.2, 0.7, 0.3)),
        PostStatus::Archived => ("Archived", Color::from_rgb(0.5, 0.5, 0.5)),
    };
    container(text(label).size(11).color(color))
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
