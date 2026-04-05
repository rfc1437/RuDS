use std::collections::HashMap;
use std::path::Path;

use iced::widget::{button, column, container, image, row, scrollable, text, Space};
use iced::widget::text::Shaping;
use iced::{Color, Element, Length};

use bds_core::i18n::{self, UiLocale};
use bds_core::model::{Media, MediaTranslation};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;
use crate::views::post_editor::TranslationFlag;

/// Saved draft content for a single media translation language.
#[derive(Debug, Clone)]
pub struct MediaTranslationDraft {
    pub title: String,
    pub alt: String,
    pub caption: String,
    pub is_dirty: bool,
}

/// State for an open media editor.
#[derive(Debug, Clone)]
pub struct MediaEditorState {
    pub media_id: String,
    pub filename: String,
    pub original_name: String,
    pub mime_type: String,
    pub size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub title: String,
    pub alt: String,
    pub caption: String,
    pub author: String,
    pub language: String,
    pub file_path: String,
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub is_dirty: bool,
    // ── Translation flags ──
    pub active_language: String,
    pub canonical_language: String,
    pub blog_languages: Vec<String>,
    pub saved_canonical: Option<MediaTranslationDraft>,
    pub translation_drafts: HashMap<String, MediaTranslationDraft>,
}

impl MediaEditorState {
    pub fn from_media(media: &Media, blog_languages: &[String], translations: &[MediaTranslation]) -> Self {
        let canonical_lang = media.language.clone().unwrap_or_else(|| "en".to_string());

        let mut translation_drafts = HashMap::new();
        for tr in translations {
            translation_drafts.insert(tr.language.clone(), MediaTranslationDraft {
                title: tr.title.clone().unwrap_or_default(),
                alt: tr.alt.clone().unwrap_or_default(),
                caption: tr.caption.clone().unwrap_or_default(),
                is_dirty: false,
            });
        }

        Self {
            media_id: media.id.clone(),
            filename: media.filename.clone(),
            original_name: media.original_name.clone(),
            mime_type: media.mime_type.clone(),
            size: media.size,
            width: media.width,
            height: media.height,
            title: media.title.clone().unwrap_or_default(),
            alt: media.alt.clone().unwrap_or_default(),
            caption: media.caption.clone().unwrap_or_default(),
            author: media.author.clone().unwrap_or_default(),
            language: canonical_lang.clone(),
            file_path: media.file_path.clone(),
            tags: media.tags.clone(),
            created_at: media.created_at,
            updated_at: media.updated_at,
            is_dirty: false,
            active_language: canonical_lang.clone(),
            canonical_language: canonical_lang,
            blog_languages: blog_languages.to_vec(),
            saved_canonical: None,
            translation_drafts,
        }
    }

    /// Switch to a different language. Saves current fields, loads target.
    pub fn switch_language(&mut self, target_lang: &str) {
        if target_lang == self.active_language {
            return;
        }
        // Save current fields
        if self.active_language == self.canonical_language {
            self.saved_canonical = Some(MediaTranslationDraft {
                title: self.title.clone(),
                alt: self.alt.clone(),
                caption: self.caption.clone(),
                is_dirty: self.is_dirty,
            });
        } else {
            self.translation_drafts.insert(self.active_language.clone(), MediaTranslationDraft {
                title: self.title.clone(),
                alt: self.alt.clone(),
                caption: self.caption.clone(),
                is_dirty: self.is_dirty,
            });
        }
        // Load target fields
        if target_lang == self.canonical_language {
            if let Some(saved) = &self.saved_canonical {
                self.title = saved.title.clone();
                self.alt = saved.alt.clone();
                self.caption = saved.caption.clone();
                self.is_dirty = saved.is_dirty;
            }
        } else if let Some(draft) = self.translation_drafts.get(target_lang) {
            self.title = draft.title.clone();
            self.alt = draft.alt.clone();
            self.caption = draft.caption.clone();
            self.is_dirty = draft.is_dirty;
        } else {
            self.title = String::new();
            self.alt = String::new();
            self.caption = String::new();
            self.is_dirty = false;
        }
        self.active_language = target_lang.to_string();
    }

    /// Build translation flags for the view.
    pub fn translation_flags(&self) -> Vec<TranslationFlag> {
        if self.translation_drafts.is_empty() {
            return Vec::new();
        }
        let mut flags = Vec::new();
        let canon = &self.canonical_language;
        let canon_locale = i18n::normalize_language(canon);
        flags.push(TranslationFlag {
            language: canon.clone(),
            flag_emoji: canon_locale.flag_emoji().to_string(),
            status: "canonical".to_string(),
            is_active: self.active_language == *canon,
        });
        let mut langs: Vec<&String> = self.translation_drafts.keys().collect();
        langs.sort();
        for lang in langs {
            let locale = i18n::normalize_language(lang);
            flags.push(TranslationFlag {
                language: lang.clone(),
                flag_emoji: locale.flag_emoji().to_string(),
                status: "translation".to_string(),
                is_active: self.active_language == **lang,
            });
        }
        flags
    }
}

/// Media editor messages.
#[derive(Debug, Clone)]
pub enum MediaEditorMsg {
    TitleChanged(String),
    AltChanged(String),
    CaptionChanged(String),
    AuthorChanged(String),
    SwitchLanguage(String),
    Save,
    Delete,
}

/// Render the media editor view.
pub fn view<'a>(
    state: &'a MediaEditorState,
    locale: UiLocale,
    data_dir: Option<&Path>,
) -> Element<'a, Message> {
    let header = inputs::toolbar(
        vec![
            text(state.original_name.clone()).size(18).into(),
        ],
        vec![
            button(text(t(locale, "common.save")).size(13))
                .on_press(Message::MediaEditor(MediaEditorMsg::Save))
                .style(inputs::primary_button)
                .padding([6, 16])
                .into(),
            button(text(t(locale, "modal.confirmDelete.delete")).size(13))
                .on_press(Message::MediaEditor(MediaEditorMsg::Delete))
                .style(inputs::danger_button)
                .padding([6, 16])
                .into(),
        ],
    );

    // Translation flags bar
    let flags = state.translation_flags();
    let flags_bar: Element<'a, Message> = if flags.is_empty() {
        Space::new(0, 0).into()
    } else {
        let flag_buttons: Vec<Element<'a, Message>> = flags
            .iter()
            .map(|flag| {
                let label = format!("{} {}", flag.flag_emoji, flag.language);
                let color = if flag.is_active {
                    Color::WHITE
                } else {
                    Color::from_rgb(0.55, 0.58, 0.65)
                };
                button(text(label).size(12).shaping(Shaping::Advanced).color(color))
                    .on_press(Message::MediaEditor(MediaEditorMsg::SwitchLanguage(flag.language.clone())))
                    .padding([4, 8])
                    .style(|_: &iced::Theme, _| button::Style::default())
                    .into()
            })
            .collect();
        row(flag_buttons).spacing(4).into()
    };

    // Preview section
    let preview: Element<'a, Message> = if state.mime_type.starts_with("image/") {
        if let Some(dir) = data_dir {
            let img_path = dir.join(&state.file_path);
            if img_path.exists() {
                container(
                    image(img_path.to_string_lossy().to_string())
                        .width(Length::Fill)
                        .height(Length::Fixed(300.0)),
                )
                .width(Length::Fill)
                .into()
            } else {
                no_preview()
            }
        } else {
            no_preview()
        }
    } else {
        no_preview()
    };

    // File info
    let dimensions = match (state.width, state.height) {
        (Some(w), Some(h)) => format!("{w} × {h}"),
        _ => String::new(),
    };
    let size_str = format_file_size(state.size);
    let info = row![
        text(format!("{} • {} • {}", state.mime_type, size_str, dimensions))
            .size(12)
            .color(Color::from_rgb(0.55, 0.58, 0.65)),
    ]
    .padding(8);

    // Metadata fields
    let title_input = inputs::labeled_input(
        &t(locale, "editor.title"),
        &t(locale, "editor.titlePlaceholder"),
        &state.title,
        |s| Message::MediaEditor(MediaEditorMsg::TitleChanged(s)),
    );
    let alt_input = inputs::labeled_input(
        &t(locale, "editor.alt"),
        &t(locale, "editor.altPlaceholder"),
        &state.alt,
        |s| Message::MediaEditor(MediaEditorMsg::AltChanged(s)),
    );
    let meta_row1 = row![title_input, alt_input].spacing(16).width(Length::Fill);

    let caption_input = inputs::labeled_input(
        &t(locale, "editor.caption"),
        "",
        &state.caption,
        |s| Message::MediaEditor(MediaEditorMsg::CaptionChanged(s)),
    );
    let author_input = inputs::labeled_input(
        &t(locale, "editor.author"),
        "",
        &state.author,
        |s| Message::MediaEditor(MediaEditorMsg::AuthorChanged(s)),
    );
    let meta_row2 = row![caption_input, author_input].spacing(16).width(Length::Fill);

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
            flags_bar,
            preview,
            info,
            inputs::section_header(&t(locale, "editor.metadata")),
            meta_row1,
            meta_row2,
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

fn no_preview<'a>() -> Element<'a, Message> {
    container(
        text("No preview available")
            .size(14)
            .color(Color::from_rgb(0.5, 0.5, 0.5)),
    )
    .width(Length::Fill)
    .height(Length::Fixed(200.0))
    .center_x(Length::Fill)
    .center_y(Length::Fixed(200.0))
    .into()
}

fn format_file_size(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}
