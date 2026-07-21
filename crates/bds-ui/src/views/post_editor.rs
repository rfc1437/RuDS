use std::cell::RefCell;
use std::collections::HashMap;

use iced::widget::text::{Shaping, Wrapping};
use iced::widget::{Column, Space, button, column, container, row, scrollable, text, text_input};
use iced::{Color, Element, Length, Theme};

use bds_core::i18n::{self, UiLocale};
use bds_core::model::{Post, PostStatus, PostTranslation};
use bds_editor::{CodeEditor, EditorBuffer, EditorMessage, highlighter};

use crate::app::Message;
use crate::components::{inputs, popover};
use crate::i18n::{t, tw};
use crate::views::status_bar;

/// Per editor_post.allium TranslationFlag value.
#[derive(Debug, Clone)]
pub struct TranslationFlag {
    pub language: String,
    pub flag_emoji: String,
    pub status: String,
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

/// Linked media metadata shown in the post editor metadata panel.
#[derive(Debug, Clone)]
pub struct LinkedMediaItem {
    pub media_id: String,
    pub name: String,
    pub file_path: String,
    pub is_image: bool,
    pub sort_order: i32,
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
    pub published_at: Option<i64>,
    pub is_dirty: bool,
    pub last_edit_at_ms: i64,
    pub metadata_expanded: bool,
    pub excerpt_expanded: bool,
    pub editor_mode: String,
    pub quick_actions_open: bool,
    pub tags_input: String,
    pub categories_input: String,
    pub available_tags: Vec<String>,
    pub semantic_tag_suggestions: Vec<String>,
    pub ai_activity: Option<String>,
    pub active_language: String,
    pub canonical_language: String,
    pub blog_languages: Vec<String>,
    pub saved_canonical: Option<TranslationDraft>,
    pub translation_drafts: HashMap<String, TranslationDraft>,
    pub outlinks: Vec<ResolvedPostLink>,
    pub backlinks: Vec<ResolvedPostLink>,
    pub linked_media: Vec<LinkedMediaItem>,
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
            published_at: self.published_at,
            is_dirty: self.is_dirty,
            last_edit_at_ms: self.last_edit_at_ms,
            metadata_expanded: self.metadata_expanded,
            excerpt_expanded: self.excerpt_expanded,
            editor_mode: self.editor_mode.clone(),
            quick_actions_open: self.quick_actions_open,
            tags_input: self.tags_input.clone(),
            categories_input: self.categories_input.clone(),
            available_tags: self.available_tags.clone(),
            semantic_tag_suggestions: self.semantic_tag_suggestions.clone(),
            ai_activity: self.ai_activity.clone(),
            active_language: self.active_language.clone(),
            canonical_language: self.canonical_language.clone(),
            blog_languages: self.blog_languages.clone(),
            saved_canonical: self.saved_canonical.clone(),
            translation_drafts: self.translation_drafts.clone(),
            outlinks: self.outlinks.clone(),
            backlinks: self.backlinks.clone(),
            linked_media: self.linked_media.clone(),
        }
    }
}

impl PostEditorState {
    pub fn from_post(
        post: &Post,
        default_mode: &str,
        blog_languages: &[String],
        translations: &[PostTranslation],
        outlinks: Vec<ResolvedPostLink>,
        backlinks: Vec<ResolvedPostLink>,
        linked_media: Vec<LinkedMediaItem>,
    ) -> Self {
        let title = post.title.clone();
        let excerpt = post.excerpt.clone().unwrap_or_default();
        let content = post.content.clone().unwrap_or_default();
        let canonical_lang = post.language.clone().unwrap_or_else(|| "en".to_string());

        let mut translation_drafts = HashMap::new();
        for tr in translations {
            translation_drafts.insert(
                tr.language.clone(),
                TranslationDraft {
                    title: tr.title.clone(),
                    excerpt: tr.excerpt.clone().unwrap_or_default(),
                    content: tr.content.clone().unwrap_or_default(),
                    status: tr.status.clone(),
                    is_dirty: false,
                },
            );
        }

        Self {
            post_id: post.id.clone(),
            slug: post.slug.clone(),
            excerpt,
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
            published_at: post.published_at,
            is_dirty: false,
            last_edit_at_ms: 0,
            metadata_expanded: title.is_empty(),
            excerpt_expanded: false,
            editor_mode: normalize_editor_mode(default_mode),
            quick_actions_open: false,
            tags_input: String::new(),
            categories_input: String::new(),
            available_tags: Vec::new(),
            semantic_tag_suggestions: Vec::new(),
            ai_activity: None,
            active_language: canonical_lang.clone(),
            canonical_language: canonical_lang,
            blog_languages: blog_languages.to_vec(),
            saved_canonical: None,
            translation_drafts,
            outlinks,
            backlinks,
            linked_media,
            title,
        }
    }

    pub fn mark_dirty(&mut self) {
        self.is_dirty = true;
        self.last_edit_at_ms = bds_core::util::now_unix_ms();
    }

    pub fn restore_view_state(&mut self, previous: &Self) {
        self.metadata_expanded = previous.metadata_expanded;
        self.excerpt_expanded = previous.excerpt_expanded;
        self.editor_mode.clone_from(&previous.editor_mode);
        self.quick_actions_open = previous.quick_actions_open;
        self.tags_input.clone_from(&previous.tags_input);
        self.categories_input.clone_from(&previous.categories_input);
        self.semantic_tag_suggestions
            .clone_from(&previous.semantic_tag_suggestions);
        self.ai_activity.clone_from(&previous.ai_activity);
        self.switch_language(&previous.active_language);
    }

    pub fn insert_markdown_at_cursor(&mut self, markdown: &str) {
        let new_content = {
            let mut buffer = self.editor_buffer.borrow_mut();
            buffer.insert(markdown);
            buffer.text()
        };
        self.content = new_content;
        self.mark_dirty();
    }

    pub fn insert_dropped_image(&mut self, media_path: &str) {
        self.insert_markdown_at_cursor(&bds_core::engine::post::post_insert_media(
            media_path, true, "",
        ));
    }

    pub fn set_editor_mode(&mut self, mode: &str) {
        self.editor_mode = normalize_editor_mode(mode);
    }

    pub fn switch_language(&mut self, target_lang: &str) {
        if target_lang == self.active_language {
            return;
        }

        if self.active_language == self.canonical_language {
            self.saved_canonical = Some(TranslationDraft {
                title: self.title.clone(),
                excerpt: self.excerpt.clone(),
                content: self.content.clone(),
                status: self.status.clone(),
                is_dirty: self.is_dirty,
            });
        } else {
            self.translation_drafts.insert(
                self.active_language.clone(),
                TranslationDraft {
                    title: self.title.clone(),
                    excerpt: self.excerpt.clone(),
                    content: self.content.clone(),
                    status: PostStatus::Draft,
                    is_dirty: self.is_dirty,
                },
            );
        }

        if target_lang == self.canonical_language {
            if let Some(saved) = self.saved_canonical.take() {
                self.title = saved.title;
                self.excerpt = saved.excerpt;
                self.content = saved.content.clone();
                self.editor_buffer = RefCell::new(EditorBuffer::new(&saved.content));
                self.status = saved.status;
                self.is_dirty = saved.is_dirty;
            }
        } else if let Some(draft) = self.translation_drafts.get(target_lang) {
            self.title = draft.title.clone();
            self.excerpt = draft.excerpt.clone();
            self.content = draft.content.clone();
            self.editor_buffer = RefCell::new(EditorBuffer::new(&draft.content));
            self.is_dirty = draft.is_dirty;
        } else {
            self.title = String::new();
            self.excerpt = String::new();
            self.content = String::new();
            self.editor_buffer = RefCell::new(EditorBuffer::new(""));
            self.is_dirty = false;
        }

        self.active_language = target_lang.to_string();
    }

    pub fn translation_flags(&self) -> Vec<TranslationFlag> {
        let mut flags = Vec::new();

        let canon = &self.canonical_language;
        let canon_locale = i18n::normalize_language(canon);
        flags.push(TranslationFlag {
            language: canon.clone(),
            flag_emoji: canon_locale.flag_emoji().to_string(),
            status: "canonical".to_string(),
            is_active: self.active_language == *canon,
        });

        let mut langs: Vec<String> = self
            .blog_languages
            .iter()
            .filter(|lang| **lang != *canon)
            .cloned()
            .collect();
        for lang in self.translation_drafts.keys() {
            if lang != canon && !langs.contains(lang) {
                langs.push(lang.clone());
            }
        }
        langs.sort();
        for lang in langs {
            let locale = i18n::normalize_language(&lang);
            let status = match self
                .translation_drafts
                .get(&lang)
                .map(|draft| &draft.status)
            {
                Some(PostStatus::Published) => "published",
                Some(_) => "draft",
                None => "missing",
            };
            flags.push(TranslationFlag {
                language: lang.clone(),
                flag_emoji: locale.flag_emoji().to_string(),
                status: status.to_string(),
                is_active: self.active_language == lang,
            });
        }

        flags
    }
}

/// Post editor messages.
#[derive(Debug, Clone)]
pub enum PostEditorMsg {
    ToggleQuickActions,
    AnalyzeWithAi,
    AnalyzeTaxonomy,
    AddGalleryImages,
    DetectLanguage,
    Translate,
    TranslateTo(String),
    SwitchEditorMode(String),
    TitleChanged(String),
    SlugChanged(String),
    ExcerptChanged(String),
    ContentChanged(String),
    AuthorChanged(String),
    LanguageChanged(String),
    TemplateSlugChanged(String),
    ToggleDoNotTranslate(bool),
    ToggleMetadata,
    ToggleExcerpt,
    SwitchLanguage(String),
    TagsInputChanged(String),
    TagsInputSubmit,
    AddSuggestedTag(String),
    RemoveTag(String),
    CategoriesInputChanged(String),
    CategoriesInputSubmit,
    RemoveCategory(String),
    Save,
    Publish,
    Discard,
    Delete,
    InsertLink,
    InsertMedia,
    Gallery,
    LinkExistingMedia,
    OpenLinkedMedia(String),
    UnlinkLinkedMedia(String),
    PostInsertLinkSelected(String),
    PostInsertLinkCreate,
    PostInsertMediaSelected(String),
    PostGalleryImageSelected(usize),
    PostInsertLinkTabSwitch(crate::views::modal::PostInsertLinkTab),
    PostInsertLinkSearch(String),
    PostInsertLinkUrlChanged(String),
    PostInsertLinkTextChanged(String),
    PostInsertLinkExternalInsert,
    PostInsertMediaSearch(String),
    PostGalleryPrevious,
    PostGalleryNext,
    PostGalleryCloseLightbox,
}

/// Render the post editor view.
pub fn view<'a>(
    state: &'a PostEditorState,
    locale: UiLocale,
    word_wrap: bool,
    ai_enabled: bool,
    preview_widget: Option<Element<'a, Message>>,
) -> Element<'a, Message> {
    let on_translation = state.active_language != state.canonical_language;

    // ── Header bar ──
    let dirty_indicator = if state.is_dirty { " \u{25CF}" } else { "" };
    let title_display = if state.title.is_empty() {
        t(locale, "editor.untitled")
    } else {
        format!("{}{}", truncate_header_title(&state.title), dirty_indicator)
    };

    let quick_actions_button: Element<'a, Message> = button(
        text(t(locale, "editor.quickActions"))
            .size(13)
            .shaping(Shaping::Advanced),
    )
    .on_press(Message::PostEditor(PostEditorMsg::ToggleQuickActions))
    .padding([6, 16])
    .style(inputs::secondary_button)
    .into();

    let quick_actions_busy = state.ai_activity.as_ref().map(|label| {
        container(
            text(format!("⏳ {label}"))
                .size(12)
                .shaping(Shaping::Advanced),
        )
        .padding([6, 12])
        .width(Length::Fixed(220.0))
        .into()
    });

    let quick_actions_menu: Element<'a, Message> = container(
        column![
            quick_actions_busy.unwrap_or_else(|| quick_action_item(
                locale,
                t(locale, "editor.aiAnalyze"),
                PostEditorMsg::AnalyzeWithAi,
                ai_enabled && state.ai_activity.is_none()
            )),
            quick_action_item(
                locale,
                t(locale, "editor.suggestTaxonomy"),
                PostEditorMsg::AnalyzeTaxonomy,
                ai_enabled && state.ai_activity.is_none()
            ),
            quick_action_item(
                locale,
                t(locale, "editor.translate"),
                PostEditorMsg::Translate,
                ai_enabled && state.ai_activity.is_none()
            ),
            quick_action_item(
                locale,
                t(locale, "editor.detectLanguage"),
                PostEditorMsg::DetectLanguage,
                ai_enabled && state.ai_activity.is_none()
            ),
            quick_action_item(
                locale,
                t(locale, "editor.addGalleryImages"),
                PostEditorMsg::AddGalleryImages,
                true
            ),
        ]
        .spacing(4),
    )
    .padding(8)
    .style(status_bar::dropdown_bg)
    .into();
    let quick_actions: Element<'a, Message> = popover::popover(
        quick_actions_button,
        quick_actions_menu,
        state.quick_actions_open,
        Message::PostEditor(PostEditorMsg::ToggleQuickActions),
    )
    .into();

    let mut header_action_items: Vec<Element<'a, Message>> = vec![
        status_badge(&state.status),
        quick_actions,
        button(
            text(t(locale, "common.save"))
                .size(13)
                .shaping(Shaping::Advanced),
        )
        .on_press(Message::PostEditor(PostEditorMsg::Save))
        .style(inputs::primary_button)
        .padding([6, 16])
        .into(),
    ];
    if state.status == PostStatus::Draft {
        header_action_items.push(
            button(
                text(t(locale, "editor.publish"))
                    .size(13)
                    .shaping(Shaping::Advanced),
            )
            .on_press(Message::PostEditor(PostEditorMsg::Publish))
            .style(inputs::primary_button)
            .padding([6, 16])
            .into(),
        );
    }
    if !on_translation && state.status == PostStatus::Draft && state.published_at.is_some() {
        header_action_items.push(
            button(
                text(t(locale, "editor.discard"))
                    .size(13)
                    .shaping(Shaping::Advanced),
            )
            .on_press(Message::PostEditor(PostEditorMsg::Discard))
            .padding([6, 16])
            .style(inputs::secondary_button)
            .into(),
        );
    }
    header_action_items.push(
        button(
            text(t(locale, "modal.confirmDelete.delete"))
                .size(13)
                .shaping(Shaping::Advanced),
        )
        .on_press(Message::PostEditor(PostEditorMsg::Delete))
        .style(inputs::danger_button)
        .padding([6, 16])
        .into(),
    );
    let header_actions = iced::widget::Row::with_children(header_action_items)
        .spacing(8)
        .align_y(iced::Alignment::Center);

    let header_row = row![
        container(
            text(title_display)
                .size(18)
                .wrapping(Wrapping::None)
                .shaping(Shaping::Advanced)
        )
        .width(Length::Fill)
        .clip(true),
        header_actions,
    ]
    .spacing(8)
    .align_y(iced::Alignment::Center)
    .width(Length::Fill);

    let header = inputs::card(header_row).padding(10);

    // ── Collapsible Metadata Section ──
    let meta_toggle_label = if state.metadata_expanded {
        format!("\u{25BC} {}", t(locale, "editor.metadata"))
    } else {
        format!("\u{25B6} {}", t(locale, "editor.metadata"))
    };
    let meta_toggle = button(
        text(meta_toggle_label)
            .size(12)
            .color(inputs::SECTION_COLOR)
            .shaping(Shaping::Advanced),
    )
    .on_press(Message::PostEditor(PostEditorMsg::ToggleMetadata))
    .padding([8, 10])
    .width(Length::Fill)
    .style(inputs::disclosure_button);

    // ── Translation Flags Bar (inline with metadata toggle) ──
    let flags = state.translation_flags();
    let meta_toggle_content: Element<'a, Message> = if flags.is_empty() {
        meta_toggle.into()
    } else {
        let mut flag_row = row![].spacing(2);
        for flag in &flags {
            let lang = flag.language.clone();
            let label = flag.flag_emoji.to_string();
            let btn = button(text(label).size(14).shaping(Shaping::Advanced))
                .on_press(Message::PostEditor(PostEditorMsg::SwitchLanguage(lang)))
                .padding([2, 4])
                .style(if flag.is_active {
                    flag_active_style
                } else {
                    flag_inactive_style
                });
            flag_row = flag_row.push(btn);
        }
        row![meta_toggle, flag_row]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into()
    };
    let meta_toggle_row: Element<'a, Message> =
        inputs::card(meta_toggle_content).padding([2, 4]).into();

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

        let author_input =
            inputs::labeled_input(&t(locale, "editor.author"), "", &state.author, |s| {
                Message::PostEditor(PostEditorMsg::AuthorChanged(s))
            });
        let language_options = if state.blog_languages.is_empty() {
            vec![state.language.clone()]
        } else {
            state.blog_languages.clone()
        };
        let language_input = inputs::labeled_select(
            &t(locale, "editor.language"),
            &language_options,
            Some(&state.language),
            |lang| Message::PostEditor(PostEditorMsg::LanguageChanged(lang)),
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
        let meta_row2 = row![author_input, language_input, template_input, dnt]
            .spacing(16)
            .align_y(iced::Alignment::End)
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
        let semantic_suggestions = visible_semantic_tag_suggestions(
            &state.semantic_tag_suggestions,
            &state.tags,
            &state.tags_input,
        );
        let semantic_tags: Element<'a, Message> = if semantic_suggestions.is_empty() {
            Space::new(0, 0).into()
        } else {
            let mut chips = row![
                text(t(locale, "editor.semanticTagSuggestions"))
                    .size(11)
                    .color(inputs::LABEL_COLOR)
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center);
            for tag in semantic_suggestions {
                chips = chips.push(
                    button(text(format!("+ {tag}")).size(11))
                        .on_press(Message::PostEditor(PostEditorMsg::AddSuggestedTag(
                            tag.to_string(),
                        )))
                        .padding([4, 8])
                        .style(inputs::secondary_button),
                );
            }
            chips.into()
        };
        let matching_suggestions =
            matching_tag_suggestions(&state.available_tags, &state.tags, &state.tags_input);
        let query_addable =
            tag_query_addable(&state.available_tags, &state.tags, &state.tags_input);
        let matching_tags: Element<'a, Message> = if state.tags_input.trim().is_empty()
            || (matching_suggestions.is_empty() && !query_addable)
        {
            Space::new(0, 0).into()
        } else {
            let mut chips = row![
                text(t(locale, "editor.matchingTags"))
                    .size(11)
                    .color(inputs::LABEL_COLOR)
            ]
            .spacing(6)
            .align_y(iced::Alignment::Center);
            for tag in matching_suggestions {
                chips = chips.push(
                    button(text(tag).size(11))
                        .on_press(Message::PostEditor(PostEditorMsg::AddSuggestedTag(
                            tag.to_string(),
                        )))
                        .padding([4, 8])
                        .style(inputs::secondary_button),
                );
            }
            if query_addable {
                let query = state.tags_input.trim().to_string();
                chips = chips.push(
                    button(text(tw(locale, "editor.createTag", &[("name", &query)])).size(11))
                        .on_press(Message::PostEditor(PostEditorMsg::AddSuggestedTag(query)))
                        .padding([4, 8])
                        .style(inputs::secondary_button),
                );
            }
            chips.wrap().into()
        };

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
                text(t(locale, "editor.outlinks"))
                    .size(12)
                    .color(inputs::LABEL_COLOR)
                    .shaping(Shaping::Advanced)
                    .into(),
            ];
            for link in &state.outlinks {
                items.push(
                    text(format!("\u{2192} {}", link.title))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.55, 0.70, 0.90))
                        .into(),
                );
            }
            Column::with_children(items).spacing(2).into()
        };

        let backlinks_section: Element<'a, Message> = if state.backlinks.is_empty() {
            Space::new(0, 0).into()
        } else {
            let mut items: Vec<Element<'a, Message>> = vec![
                text(t(locale, "editor.backlinks"))
                    .size(12)
                    .color(inputs::LABEL_COLOR)
                    .shaping(Shaping::Advanced)
                    .into(),
            ];
            for link in &state.backlinks {
                items.push(
                    text(format!("\u{2190} {}", link.title))
                        .size(12)
                        .shaping(Shaping::Advanced)
                        .color(Color::from_rgb(0.55, 0.70, 0.90))
                        .into(),
                );
            }
            Column::with_children(items).spacing(2).into()
        };

        let link_existing_button: Element<'a, Message> = button(
            text(t(locale, "editor.linkExistingMedia"))
                .size(11)
                .shaping(Shaping::Advanced),
        )
        .on_press(Message::PostEditor(PostEditorMsg::LinkExistingMedia))
        .padding([4, 10])
        .style(inputs::secondary_button)
        .into();

        let linked_media_header: Element<'a, Message> = row![
            text(t(locale, "editor.linkedMedia"))
                .size(12)
                .color(inputs::LABEL_COLOR)
                .shaping(Shaping::Advanced),
            Space::with_width(Length::Fill),
            link_existing_button,
        ]
        .align_y(iced::Alignment::Center)
        .into();

        let linked_media_section: Element<'a, Message> = if state.linked_media.is_empty() {
            column![
                linked_media_header,
                text(t(locale, "editor.linkedMediaEmpty"))
                    .size(12)
                    .shaping(Shaping::Advanced)
                    .color(Color::from_rgb(0.55, 0.55, 0.60)),
            ]
            .spacing(4)
            .into()
        } else {
            let mut items: Vec<Element<'a, Message>> = vec![linked_media_header];
            for media in &state.linked_media {
                let media_id = media.media_id.clone();
                let open_id = media.media_id.clone();
                let kind_label = if media.is_image {
                    t(locale, "editor.linkedMediaKindImage")
                } else {
                    t(locale, "editor.linkedMediaKindFile")
                };
                items.push(
                    container(
                        row![
                            column![
                                text(media.name.clone())
                                    .size(12)
                                    .shaping(Shaping::Advanced)
                                    .color(Color::WHITE),
                                text(format!("{} {}", kind_label, media.sort_order + 1))
                                    .size(10)
                                    .shaping(Shaping::Advanced)
                                    .color(Color::from_rgb(0.55, 0.55, 0.60)),
                            ]
                            .spacing(2)
                            .width(Length::Fill),
                            button(
                                text(t(locale, "common.open"))
                                    .size(11)
                                    .shaping(Shaping::Advanced)
                            )
                            .on_press(Message::PostEditor(PostEditorMsg::OpenLinkedMedia(open_id)))
                            .padding([4, 10]),
                            button(
                                text(t(locale, "editor.unlinkMedia"))
                                    .size(11)
                                    .shaping(Shaping::Advanced)
                            )
                            .on_press(Message::PostEditor(PostEditorMsg::UnlinkLinkedMedia(
                                media_id
                            )))
                            .padding([4, 10])
                            .style(inputs::danger_button),
                        ]
                        .spacing(8)
                        .align_y(iced::Alignment::Center),
                    )
                    .padding(8)
                    .style(|_: &Theme| container::Style {
                        background: Some(iced::Background::Color(Color::from_rgb(
                            0.16, 0.18, 0.22,
                        ))),
                        border: iced::Border {
                            color: Color::from_rgb(0.28, 0.28, 0.32),
                            width: 1.0,
                            radius: 6.0.into(),
                        },
                        ..container::Style::default()
                    })
                    .into(),
                );
            }
            Column::with_children(items).spacing(6).into()
        };

        inputs::card(
            column![
                meta_row1,
                meta_row2,
                tags_section,
                semantic_tags,
                matching_tags,
                categories_section,
                outlinks_section,
                backlinks_section,
                linked_media_section,
            ]
            .spacing(12)
            .width(Length::Fill),
        )
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
        text(excerpt_toggle_label)
            .size(12)
            .color(inputs::SECTION_COLOR)
            .shaping(Shaping::Advanced),
    )
    .on_press(Message::PostEditor(PostEditorMsg::ToggleExcerpt))
    .padding([8, 10])
    .width(Length::Fill)
    .style(inputs::disclosure_button);
    let excerpt_toggle: Element<'a, Message> = inputs::card(excerpt_toggle).padding([2, 4]).into();

    let excerpt_section: Element<'a, Message> = if state.excerpt_expanded {
        inputs::card(inputs::labeled_input(
            &t(locale, "editor.excerpt"),
            &t(locale, "editor.excerptPlaceholder"),
            &state.excerpt,
            |s| Message::PostEditor(PostEditorMsg::ExcerptChanged(s)),
        ))
        .into()
    } else {
        Space::new(0, 0).into()
    };

    // ── Content section (fills remaining space) ──
    let content_label = inputs::section_header(&t(locale, "editor.content"));
    let mode_toggle = row![
        mode_button(
            locale,
            &state.editor_mode,
            "markdown",
            Message::PostEditor(PostEditorMsg::SwitchEditorMode("markdown".to_string()))
        ),
        mode_button(
            locale,
            &state.editor_mode,
            "preview",
            Message::PostEditor(PostEditorMsg::SwitchEditorMode("preview".to_string()))
        ),
    ]
    .spacing(6)
    .align_y(iced::Alignment::Center);
    let show_content_actions = content_actions_visible(&state.editor_mode);
    let body_toolbar = inputs::toolbar(
        vec![
            row![
                content_label,
                Space::with_width(Length::Fixed(12.0)),
                mode_toggle
            ]
            .align_y(iced::Alignment::Center)
            .into(),
        ],
        vec![
            if show_content_actions {
                button(
                    text(t(locale, "editor.insertLink"))
                        .size(13)
                        .shaping(Shaping::Advanced),
                )
                .on_press(Message::PostEditor(PostEditorMsg::InsertLink))
                .padding([6, 16])
                .style(inputs::secondary_button)
                .into()
            } else {
                Space::new(0, 0).into()
            },
            if show_content_actions {
                button(
                    text(t(locale, "editor.insertMedia"))
                        .size(13)
                        .shaping(Shaping::Advanced),
                )
                .on_press(Message::PostEditor(PostEditorMsg::InsertMedia))
                .padding([6, 16])
                .style(inputs::secondary_button)
                .into()
            } else {
                Space::new(0, 0).into()
            },
            if show_content_actions {
                button(
                    text(t(locale, "editor.gallery"))
                        .size(13)
                        .shaping(Shaping::Advanced),
                )
                .on_press(Message::PostEditor(PostEditorMsg::Gallery))
                .padding([6, 16])
                .style(inputs::secondary_button)
                .into()
            } else {
                Space::new(0, 0).into()
            },
        ],
    );
    let editor_widget: Element<'a, Message> = if state.editor_mode == "preview" {
        preview_widget.unwrap_or_else(|| {
            container(
                text(t(locale, "tabBar.loading"))
                    .size(14)
                    .shaping(Shaping::Advanced),
            )
            .padding(16)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        })
    } else {
        CodeEditor::new(&state.editor_buffer, highlighter(), "md")
            .word_wrap(word_wrap)
            .on_change(|msg| match msg {
                EditorMessage::ContentChanged(s) => {
                    Message::PostEditor(PostEditorMsg::ContentChanged(s))
                }
                EditorMessage::SaveRequested => Message::PostEditor(PostEditorMsg::Save),
            })
            .into()
    };

    // ── Footer ──
    let published_gap: Element<'a, Message> = if state.published_at.is_some() {
        Space::with_width(Length::Fixed(24.0)).into()
    } else {
        Space::new(0, 0).into()
    };
    let published_label: Element<'a, Message> = if let Some(published_at) = state.published_at {
        inputs::date_label(&t(locale, "editor.publishedAt"), published_at)
    } else {
        Space::new(0, 0).into()
    };

    let footer = row![
        inputs::date_label(&t(locale, "editor.createdAt"), state.created_at),
        Space::with_width(Length::Fixed(24.0)),
        inputs::date_label(&t(locale, "editor.updatedAt"), state.updated_at),
        published_gap,
        published_label,
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
        .spacing(8)
        .width(Length::Fill),
    )
    .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
    .style(inputs::scrollable_style)
    .height(Length::Shrink);

    // ── Full layout: top pane (shrink), editor (fill), footer (shrink) ──
    column![top_pane, body_toolbar, editor_widget, footer,]
        .spacing(8)
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn matching_tag_suggestions<'a>(
    available: &'a [String],
    selected: &[String],
    query: &str,
) -> Vec<&'a str> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    available
        .iter()
        .filter(|tag| {
            !selected
                .iter()
                .any(|current| current.eq_ignore_ascii_case(tag))
        })
        .filter(|tag| tag.to_lowercase().contains(&query))
        .take(8)
        .map(String::as_str)
        .collect()
}

fn tag_query_addable(available: &[String], selected: &[String], query: &str) -> bool {
    let query = query.trim();
    !query.is_empty()
        && !available.iter().any(|tag| tag.eq_ignore_ascii_case(query))
        && !selected.iter().any(|tag| tag.eq_ignore_ascii_case(query))
}

fn visible_semantic_tag_suggestions<'a>(
    semantic: &'a [String],
    selected: &[String],
    query: &str,
) -> Vec<&'a str> {
    if !query.trim().is_empty() {
        return Vec::new();
    }
    semantic
        .iter()
        .filter(|tag| {
            !selected
                .iter()
                .any(|current| current.eq_ignore_ascii_case(tag))
        })
        .map(String::as_str)
        .collect()
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
        text(label.to_string())
            .size(12)
            .color(inputs::LABEL_COLOR)
            .shaping(Shaping::Advanced),
        chip_row.wrap(),
        text_input(placeholder, input_value)
            .on_input(on_input)
            .on_submit(on_submit)
            .size(13)
            .padding([8, 10])
            .style(inputs::field_style),
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

fn mode_button<'a>(
    locale: UiLocale,
    active_mode: &str,
    mode: &str,
    message: Message,
) -> Element<'a, Message> {
    let label = match mode {
        "preview" => t(locale, "editor.modePreview"),
        _ => t(locale, "editor.modeMarkdown"),
    };
    button(text(label).size(12).shaping(Shaping::Advanced))
        .on_press(message)
        .padding([4, 10])
        .style(if active_mode == mode {
            flag_active_style
        } else {
            flag_inactive_style
        })
        .into()
}

fn quick_action_item<'a>(
    locale: UiLocale,
    label: String,
    msg: PostEditorMsg,
    enabled: bool,
) -> Element<'a, Message> {
    let _ = locale;
    button(text(label).size(12).shaping(Shaping::Advanced))
        .on_press_maybe(enabled.then_some(Message::PostEditor(msg)))
        .padding([6, 12])
        .style(status_bar::dropdown_item)
        .width(Length::Fixed(220.0))
        .into()
}

fn truncate_header_title(title: &str) -> String {
    const HEADER_TITLE_MAX_LEN: usize = 56;
    if title.chars().count() > HEADER_TITLE_MAX_LEN {
        let truncated: String = title
            .chars()
            .take(HEADER_TITLE_MAX_LEN.saturating_sub(3))
            .collect();
        format!("{truncated}...")
    } else {
        title.to_string()
    }
}

fn normalize_editor_mode(mode: &str) -> String {
    match mode {
        "preview" => "preview".to_string(),
        _ => "markdown".to_string(),
    }
}

fn content_actions_visible(mode: &str) -> bool {
    mode == "markdown"
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

#[cfg(test)]
mod tests {
    use super::*;
    fn sample_state() -> PostEditorState {
        let post = Post {
            id: "post-1".to_string(),
            project_id: "project-1".to_string(),
            title: "Sample".to_string(),
            slug: "sample".to_string(),
            excerpt: None,
            content: Some("Hello world".to_string()),
            status: PostStatus::Draft,
            file_path: String::new(),
            checksum: None,
            created_at: 1,
            updated_at: 1,
            published_at: None,
            published_title: None,
            published_content: None,
            published_excerpt: None,
            published_tags: None,
            published_categories: None,
            tags: Vec::new(),
            categories: Vec::new(),
            author: None,
            language: Some("en".to_string()),
            template_slug: None,
            do_not_translate: false,
        };
        PostEditorState::from_post(
            &post,
            "markdown",
            &["en".to_string()],
            &[],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )
    }

    #[test]
    fn partial_tag_query_matches_existing_tags_case_insensitively() {
        let available = vec![
            "Photography".to_string(),
            "Photo Essay".to_string(),
            "Rust".to_string(),
        ];
        let selected = vec!["Photography".to_string()];

        assert_eq!(
            matching_tag_suggestions(&available, &selected, "PHO"),
            vec!["Photo Essay"]
        );
    }

    #[test]
    fn partial_tag_query_limits_matches_and_exact_names_are_not_addable() {
        let available = (0..10)
            .map(|index| format!("tag-{index}"))
            .collect::<Vec<_>>();

        assert_eq!(matching_tag_suggestions(&available, &[], "tag").len(), 8);
        assert!(!tag_query_addable(&available, &[], " TAG-3 "));
        assert!(tag_query_addable(&available, &[], "new tag"));
    }

    #[test]
    fn semantic_tag_suggestions_are_hidden_while_filtering_and_exclude_selected_tags() {
        let selected = vec!["science".to_string()];
        let semantic = vec![
            "science".to_string(),
            "space".to_string(),
            "history".to_string(),
        ];

        assert_eq!(
            visible_semantic_tag_suggestions(&semantic, &selected, ""),
            vec!["space", "history"]
        );
        assert!(visible_semantic_tag_suggestions(&semantic, &selected, "spa").is_empty());
    }

    #[test]
    fn insert_markdown_at_cursor_uses_buffer_cursor() {
        let mut state = sample_state();
        state.editor_buffer.borrow_mut().set_cursor(0, 5);

        state.insert_markdown_at_cursor(" brave");

        assert_eq!(state.content, "Hello brave world");
        assert!(state.is_dirty);
    }

    #[test]
    fn insert_markdown_at_cursor_replaces_selection() {
        let mut state = sample_state();
        state.editor_buffer.borrow_mut().set_selection(0, 6, 0, 11);

        state.insert_markdown_at_cursor("Rust");

        assert_eq!(state.content, "Hello Rust");
        assert!(state.is_dirty);
    }

    #[test]
    fn dropped_image_markdown_is_inserted_at_the_buffer_cursor() {
        let mut state = sample_state();
        state.editor_buffer.borrow_mut().set_cursor(0, 5);

        state.insert_dropped_image("media/2026/07/media-1.png");

        assert_eq!(state.content, "Hello![](/media/2026/07/media-1.png) world");
        assert!(state.is_dirty);
    }

    #[test]
    fn unsupported_default_mode_falls_back_to_markdown() {
        let mut state = sample_state();

        state.set_editor_mode("visual");
        assert_eq!(state.editor_mode, "markdown");

        state.set_editor_mode("preview");
        assert_eq!(state.editor_mode, "preview");
    }

    #[test]
    fn content_actions_are_only_visible_in_markdown_mode() {
        assert!(content_actions_visible("markdown"));
        assert!(!content_actions_visible("preview"));
    }
}
