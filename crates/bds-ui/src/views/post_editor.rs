use std::cell::RefCell;
use std::collections::HashMap;

use iced::widget::{button, column, container, row, scrollable, text, text_input, Column, Space};
use iced::widget::text::{Shaping, Wrapping};
use iced::{Color, Element, Length, Theme};

use bds_core::i18n::{self, UiLocale};
use bds_core::model::{Post, PostStatus, PostTranslation};
use bds_editor::{CodeEditor, EditorBuffer, EditorMessage, highlighter};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

/// Per editor_post.allium TranslationFlag value.
#[derive(Debug, Clone)]
pub struct TranslationFlag {
    pub language: String,
    pub flag_emoji: String,
    pub status: String, // "draft" | "published" | "missing"
    pub is_active: bool,
}

/// Saved draft content for a single translation language.
#[derive(Debug, Clone)]
pub struct TranslationDraft {
    pub title: String,
    pub excerpt: String,
    pub content: String,
    pub status: PostStatus,
    pub is_dirty: bool,
}

/// Resolved post link for display in metadata.
#[derive(Debug, Clone)]
pub struct ResolvedPostLink {
    pub post_id: String,
    pub title: String,
}

/// State for an open post editor.
pub struct PostEditorState {
    pub post_id: String,
    pub title: String,
    pub slug: String,
    pub excerpt: String,
    pub content: String,
    pub editor_buffer: RefCell<EditorBuffer>,
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
    // ── Translation flags ──
    /// Currently-displayed language (canonical or translation).
    pub active_language: String,
    /// The post's own language (canonical).
    pub canonical_language: String,
    /// All blog languages from project metadata.
    pub blog_languages: Vec<String>,
    /// Saved canonical title/excerpt/content when viewing a translation.
    pub saved_canonical: Option<TranslationDraft>,
    /// Translation drafts keyed by language code.
    pub translation_drafts: HashMap<String, TranslationDraft>,
    /// Outgoing links from this post to other posts.
    pub outlinks: Vec<ResolvedPostLink>,
    /// Incoming links from other posts to this post.
    pub backlinks: Vec<ResolvedPostLink>,
}

impl std::fmt::Debug for PostEditorState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostEditorState")
            .field("post_id", &self.post_id)
            .field("title", &self.title)
            .field("active_language", &self.active_language)
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
            editor_buffer: RefCell::new(EditorBuffer::new(&self.content)),
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
            active_language: self.active_language.clone(),
            canonical_language: self.canonical_language.clone(),
            blog_languages: self.blog_languages.clone(),
            saved_canonical: self.saved_canonical.clone(),
            translation_drafts: self.translation_drafts.clone(),
            outlinks: self.outlinks.clone(),
            backlinks: self.backlinks.clone(),
        }
    }
}

impl PostEditorState {
    pub fn from_post(
        post: &Post,
        blog_languages: &[String],
        translations: &[PostTranslation],
        outlinks: Vec<ResolvedPostLink>,
        backlinks: Vec<ResolvedPostLink>,
    ) -> Self {
        let title = post.title.clone();
        let content = post.content.clone().unwrap_or_default();
        let canonical_lang = post.language.clone().unwrap_or_else(|| "en".to_string());

        let mut translation_drafts = HashMap::new();
        for tr in translations {
            translation_drafts.insert(tr.language.clone(), TranslationDraft {
                title: tr.title.clone(),
                excerpt: tr.excerpt.clone().unwrap_or_default(),
                content: tr.content.clone().unwrap_or_default(),
                status: tr.status.clone(),
                is_dirty: false,
            });
        }

        Self {
            post_id: post.id.clone(),
            slug: post.slug.clone(),
            excerpt: post.excerpt.clone().unwrap_or_default(),
            content: content.clone(),
            editor_buffer: RefCell::new(EditorBuffer::new(&content)),
            tags: post.tags.clone(),
            categories: post.categories.clone(),
            author: post.author.clone().unwrap_or_default(),
            language: canonical_lang.clone(),
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
            active_language: canonical_lang.clone(),
            canonical_language: canonical_lang,
            blog_languages: blog_languages.to_vec(),
            saved_canonical: None,
            translation_drafts,
            outlinks,
            backlinks,
            title,
        }
    }

    /// Switch the editor to display a different language.
    /// Saves current fields and loads the target language's draft.
    pub fn switch_language(&mut self, target_lang: &str) {
        if target_lang == self.active_language {
            return;
        }

        // Save current fields
        if self.active_language == self.canonical_language {
            // Switching away from canonical — stash canonical fields
            self.saved_canonical = Some(TranslationDraft {
                title: self.title.clone(),
                excerpt: self.excerpt.clone(),
                content: self.content.clone(),
                status: self.status.clone(),
                is_dirty: self.is_dirty,
            });
        } else {
            // Switching away from a translation — save to drafts
            self.translation_drafts.insert(self.active_language.clone(), TranslationDraft {
                title: self.title.clone(),
                excerpt: self.excerpt.clone(),
                content: self.content.clone(),
                status: PostStatus::Draft,
                is_dirty: self.is_dirty,
            });
        }

        // Load target fields
        if target_lang == self.canonical_language {
            // Restore canonical
            if let Some(saved) = self.saved_canonical.take() {
                self.title = saved.title;
                self.excerpt = saved.excerpt;
                self.content = saved.content.clone();
                self.editor_buffer = RefCell::new(EditorBuffer::new(&saved.content));
                self.status = saved.status;
                self.is_dirty = saved.is_dirty;
            }
        } else if let Some(draft) = self.translation_drafts.get(target_lang) {
            // Load existing translation
            self.title = draft.title.clone();
            self.excerpt = draft.excerpt.clone();
            self.content = draft.content.clone();
            self.editor_buffer = RefCell::new(EditorBuffer::new(&draft.content));
            self.is_dirty = draft.is_dirty;
        } else {
            // No translation yet — blank fields
            self.title = String::new();
            self.excerpt = String::new();
            self.content = String::new();
            self.editor_buffer = RefCell::new(EditorBuffer::new(""));
            self.is_dirty = false;
        }

        self.active_language = target_lang.to_string();
    }

    /// Build the translation flags list for the view.
    /// Flags are driven by the post's actual translations, not blog-level languages.
    pub fn translation_flags(&self) -> Vec<TranslationFlag> {
        if self.translation_drafts.is_empty() {
            return Vec::new();
        }
        let mut flags = Vec::new();

        // Canonical language first (always shown when translations exist)
        let canon = &self.canonical_language;
        let canon_locale = i18n::normalize_language(canon);
        flags.push(TranslationFlag {
            language: canon.clone(),
            flag_emoji: canon_locale.flag_emoji().to_string(),
            status: "canonical".to_string(),
            is_active: self.active_language == *canon,
        });

        // Each existing translation for this post
        let mut langs: Vec<&String> = self.translation_drafts.keys().collect();
        langs.sort();
        for lang in langs {
            let locale = i18n::normalize_language(lang);
            let status = match self.translation_drafts[lang].status {
                PostStatus::Published => "published",
                _ => "draft",
            };
            flags.push(TranslationFlag {
                language: lang.clone(),
                flag_emoji: locale.flag_emoji().to_string(),
                status: status.to_string(),
                is_active: self.active_language == **lang,
            });
        }

        flags
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
    ToggleMetadata,
    ToggleExcerpt,
    SwitchLanguage(String),
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

    // ── Translation Flags Bar (inline with metadata toggle) ──
    let flags = state.translation_flags();
    let on_translation = state.active_language != state.canonical_language;
    let meta_toggle_row: Element<'a, Message> = if flags.is_empty() {
        meta_toggle.into()
    } else {
        let mut flag_row = row![].spacing(2);
        for flag in &flags {
            let lang = flag.language.clone();
            let label = format!("{}", flag.flag_emoji);
            let btn = button(
                text(label).size(14).shaping(Shaping::Advanced),
            )
            .on_press(Message::PostEditor(PostEditorMsg::SwitchLanguage(lang)))
            .padding([2, 4])
            .style(if flag.is_active { flag_active_style } else { flag_inactive_style });
            flag_row = flag_row.push(btn);
        }
        row![meta_toggle, Space::with_width(Length::Fill), flag_row]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into()
    };

    let metadata_section: Element<'a, Message> = if state.metadata_expanded && !on_translation {
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

        // Post links sections
        let outlinks_section: Element<'a, Message> = if state.outlinks.is_empty() {
            Space::new(0, 0).into()
        } else {
            let mut items: Vec<Element<'a, Message>> = vec![
                text(t(locale, "editor.outlinks")).size(12).color(inputs::LABEL_COLOR).shaping(Shaping::Advanced).into(),
            ];
            for link in &state.outlinks {
                items.push(
                    text(format!("\u{2192} {}", link.title))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.55, 0.70, 0.90))
                        .into()
                );
            }
            Column::with_children(items).spacing(2).into()
        };

        let backlinks_section: Element<'a, Message> = if state.backlinks.is_empty() {
            Space::new(0, 0).into()
        } else {
            let mut items: Vec<Element<'a, Message>> = vec![
                text(t(locale, "editor.backlinks")).size(12).color(inputs::LABEL_COLOR).shaping(Shaping::Advanced).into(),
            ];
            for link in &state.backlinks {
                items.push(
                    text(format!("\u{2190} {}", link.title))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.55, 0.70, 0.90))
                        .into()
                );
            }
            Column::with_children(items).spacing(2).into()
        };

        column![meta_row1, meta_row2, tags_section, categories_section, outlinks_section, backlinks_section]
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
    let content_label = inputs::section_header(&t(locale, "editor.content"));
    let editor_widget: Element<'a, Message> = CodeEditor::new(
            &state.editor_buffer,
            highlighter(),
            "md",
        )
        .on_change(|msg| match msg {
            EditorMessage::ContentChanged(s) => Message::PostEditor(PostEditorMsg::ContentChanged(s)),
            EditorMessage::SaveRequested => Message::PostEditor(PostEditorMsg::Save),
        })
        .into();

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
            meta_toggle_row,
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

/// Active translation flag button style (highlighted).
fn flag_active_style(_theme: &Theme, _status: button::Status) -> button::Style {
    button::Style {
        background: Some(iced::Background::Color(Color::from_rgb(0.20, 0.35, 0.60))),
        text_color: Color::WHITE,
        border: iced::Border {
            radius: 4.0.into(),
            width: 1.0,
            color: Color::from_rgb(0.30, 0.50, 0.80),
        },
        ..button::Style::default()
    }
}

/// Inactive translation flag button style.
fn flag_inactive_style(_theme: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered => Color::from_rgb(0.22, 0.24, 0.30),
        _ => Color::TRANSPARENT,
    };
    button::Style {
        background: Some(iced::Background::Color(bg)),
        text_color: Color::from_rgb(0.70, 0.72, 0.78),
        border: iced::Border {
            radius: 4.0.into(),
            ..iced::Border::default()
        },
        ..button::Style::default()
    }
}
