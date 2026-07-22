use std::fmt::{self, Display};

use crate::model::Media;
use crate::util::timestamp::{iso_to_unix_ms, unix_ms_to_iso};

/// Parsed media sidecar fields.
#[derive(Debug, Clone)]
pub struct MediaSidecar {
    pub id: String,
    pub original_name: String,
    pub mime_type: String,
    pub size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub title: Option<String>,
    pub alt: Option<String>,
    pub caption: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub tags: Vec<String>,
    pub linked_post_ids: Vec<String>,
}

/// Parsed media translation sidecar fields.
#[derive(Debug, Clone)]
pub struct MediaTranslationSidecar {
    pub translation_for: String,
    pub language: String,
    pub title: Option<String>,
    pub alt: Option<String>,
    pub caption: Option<String>,
}

impl MediaSidecar {
    pub fn from_media(media: &Media, linked_post_ids: &[String]) -> Self {
        Self {
            id: media.id.clone(),
            original_name: media.original_name.clone(),
            mime_type: media.mime_type.clone(),
            size: media.size,
            width: media.width,
            height: media.height,
            title: media.title.clone(),
            alt: media.alt.clone(),
            caption: media.caption.clone(),
            author: media.author.clone(),
            language: media.language.clone(),
            created_at: media.created_at,
            updated_at: media.updated_at,
            tags: media.tags.clone(),
            linked_post_ids: linked_post_ids.to_vec(),
        }
    }

    /// Serialize to the canonical bDS2 hand-built YAML-like format.
    fn serialize(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push("---".into());
        lines.push(format!("id: {}", self.id));
        lines.push(format!(
            "originalName: \"{}\"",
            escape_double_quotes(&self.original_name)
        ));
        lines.push(format!("mimeType: {}", self.mime_type));
        lines.push(format!("size: {}", self.size));
        if let Some(w) = self.width {
            lines.push(format!("width: {w}"));
        }
        if let Some(h) = self.height {
            lines.push(format!("height: {h}"));
        }
        if let Some(ref title) = self.title
            && !title.is_empty()
        {
            lines.push(format!("title: \"{}\"", escape_double_quotes(title)));
        }
        if let Some(ref alt) = self.alt
            && !alt.is_empty()
        {
            lines.push(format!("alt: \"{}\"", escape_double_quotes(alt)));
        }
        if let Some(ref caption) = self.caption
            && !caption.is_empty()
        {
            lines.push(format!("caption: \"{}\"", escape_double_quotes(caption)));
        }
        if let Some(ref author) = self.author
            && !author.is_empty()
        {
            lines.push(format!("author: \"{}\"", escape_double_quotes(author)));
        }
        if let Some(ref language) = self.language
            && !language.is_empty()
        {
            lines.push(format!("language: {language}"));
        }
        lines.push(format!("createdAt: {}", unix_ms_to_iso(self.created_at)));
        lines.push(format!("updatedAt: {}", unix_ms_to_iso(self.updated_at)));

        lines.push(format!(
            "linkedPostIds: {}",
            serialize_inline_string_array(&self.linked_post_ids)
        ));
        lines.push(format!(
            "tags: {}",
            serialize_inline_string_array(&self.tags)
        ));

        lines.push("---".into());
        lines.join("\n")
    }
}

impl Display for MediaSidecar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.serialize())
    }
}

impl MediaTranslationSidecar {
    /// Serialize to the hand-built format.
    fn serialize(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push("---".into());
        lines.push(format!("translationFor: {}", self.translation_for));
        lines.push(format!("language: {}", self.language));
        if let Some(ref title) = self.title
            && !title.is_empty()
        {
            lines.push(format!("title: \"{}\"", escape_double_quotes(title)));
        }
        if let Some(ref alt) = self.alt
            && !alt.is_empty()
        {
            lines.push(format!("alt: \"{}\"", escape_double_quotes(alt)));
        }
        if let Some(ref caption) = self.caption
            && !caption.is_empty()
        {
            lines.push(format!("caption: \"{}\"", escape_double_quotes(caption)));
        }
        lines.push("---".into());
        lines.join("\n")
    }
}

impl Display for MediaTranslationSidecar {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.serialize())
    }
}

/// Parse a canonical media sidecar.
pub fn read_sidecar(content: &str) -> Result<MediaSidecar, String> {
    // Strip --- delimiters
    let inner = content.trim();
    let inner = inner.strip_prefix("---").ok_or("missing opening ---")?;
    let inner = inner.trim_start_matches('\n');
    let inner = inner.strip_suffix("---").ok_or("missing closing ---")?;
    let inner = inner.trim_end_matches('\n');

    let mut id = None;
    let mut original_name = None;
    let mut mime_type = None;
    let mut size = None;
    let mut width = None;
    let mut height = None;
    let mut title = None;
    let mut alt = None;
    let mut caption = None;
    let mut author = None;
    let mut language = None;
    let mut created_at = None;
    let mut updated_at = None;
    let mut tags = Vec::new();
    let mut linked_post_ids = Vec::new();

    for line in inner.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(": ") {
            let value = value.trim();
            match key.trim() {
                "id" => id = Some(value.to_string()),
                "originalName" => original_name = Some(unquote_double(value)),
                "mimeType" => mime_type = Some(value.to_string()),
                "size" => size = value.parse::<i64>().ok(),
                "width" => width = value.parse::<i32>().ok(),
                "height" => height = value.parse::<i32>().ok(),
                "title" => title = Some(unquote_double(value)),
                "alt" => alt = Some(unquote_double(value)),
                "caption" => caption = Some(unquote_double(value)),
                "author" => author = Some(unquote_double(value)),
                "language" => language = Some(value.to_string()),
                "createdAt" => created_at = iso_to_unix_ms(value).ok(),
                "updatedAt" => updated_at = iso_to_unix_ms(value).ok(),
                "tags" => tags = parse_inline_json_array(value),
                "linkedPostIds" => linked_post_ids = parse_inline_json_array(value),
                _ => {} // ignore unknown fields
            }
        }
    }

    Ok(MediaSidecar {
        id: id.ok_or("missing 'id'")?,
        original_name: original_name.ok_or("missing 'originalName'")?,
        mime_type: mime_type.ok_or("missing 'mimeType'")?,
        size: size.ok_or("missing 'size'")?,
        width,
        height,
        title,
        alt,
        caption,
        author,
        language,
        created_at: created_at.ok_or("missing 'createdAt'")?,
        updated_at: updated_at.ok_or("missing 'updatedAt'")?,
        tags,
        linked_post_ids,
    })
}

/// Parse a media translation sidecar.
pub fn read_translation_sidecar(content: &str) -> Result<MediaTranslationSidecar, String> {
    let inner = content.trim();
    let inner = inner.strip_prefix("---").ok_or("missing opening ---")?;
    let inner = inner.trim_start_matches('\n');
    let inner = inner.strip_suffix("---").ok_or("missing closing ---")?;
    let inner = inner.trim_end_matches('\n');

    let mut translation_for = None;
    let mut language = None;
    let mut title = None;
    let mut alt = None;
    let mut caption = None;

    for line in inner.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((key, value)) = line.split_once(": ") {
            let value = value.trim();
            match key.trim() {
                "translationFor" => translation_for = Some(value.to_string()),
                "language" => language = Some(value.to_string()),
                "title" => title = Some(unquote_double(value)),
                "alt" => alt = Some(unquote_double(value)),
                "caption" => caption = Some(unquote_double(value)),
                _ => {}
            }
        }
    }

    Ok(MediaTranslationSidecar {
        translation_for: translation_for.ok_or("missing 'translationFor'")?,
        language: language.ok_or("missing 'language'")?,
        title,
        alt,
        caption,
    })
}

// --- Helpers ---

fn escape_double_quotes(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn unquote_double(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        inner
            .replace("\\n", "\n")
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        s.to_string()
    }
}

fn serialize_inline_string_array(values: &[String]) -> String {
    let quoted = values
        .iter()
        .map(|value| format!("\"{}\"", escape_double_quotes(value)))
        .collect::<Vec<_>>();
    format!("[{}]", quoted.join(", "))
}

/// Parse inline JSON-like array: `["a", "b"]` or `[]`.
fn parse_inline_json_array(s: &str) -> Vec<String> {
    let s = s.trim();
    if s == "[]" {
        return Vec::new();
    }
    // Try serde_json first
    if let Ok(arr) = serde_json::from_str::<Vec<String>>(s) {
        return arr;
    }
    // Fallback: strip brackets and split by comma
    let inner = s.trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|item| unquote_double(item.trim()))
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/compatibility-projects/rfc1437-sample")
    }

    #[test]
    fn parse_fixture_sidecar() {
        let path =
            fixture_dir().join("media/2005/11/eb0cf9d7-6fbd-4b74-9be3-759d6e16f240.jpg.meta");
        let content = fs::read_to_string(path).unwrap();
        let sc = read_sidecar(&content).unwrap();
        assert_eq!(sc.id, "eb0cf9d7-6fbd-4b74-9be3-759d6e16f240");
        assert_eq!(sc.original_name, "CRW_1121.jpg");
        assert_eq!(sc.mime_type, "image/jpeg");
        assert_eq!(sc.size, 706358);
        assert_eq!(sc.width, Some(1800));
        assert_eq!(sc.height, Some(1200));
        assert_eq!(sc.title.as_deref(), Some("Esmeralda"));
        assert!(sc.alt.as_ref().unwrap().contains("Spinnenfrau"));
        assert!(sc.caption.as_ref().unwrap().contains("Handwerkskunst"));
        assert!(sc.tags.is_empty());
        assert_eq!(
            sc.linked_post_ids,
            vec!["40a83ab1-423d-4310-aac4-642d84675007"]
        );
    }

    #[test]
    fn roundtrip_sidecar() {
        let sc = MediaSidecar {
            id: "test-uuid".into(),
            original_name: "photo.jpg".into(),
            mime_type: "image/jpeg".into(),
            size: 12345,
            width: Some(800),
            height: Some(600),
            title: Some("My Photo".into()),
            alt: Some("A nice photo".into()),
            caption: None,
            author: Some("Hugo".into()),
            language: None,
            created_at: 1131883200000,
            updated_at: 1131883200000,
            tags: vec!["nature".into(), "photo".into()],
            linked_post_ids: vec![],
        };
        let output = sc.to_string();
        let parsed = read_sidecar(&output).unwrap();
        assert_eq!(parsed.id, sc.id);
        assert_eq!(parsed.original_name, sc.original_name);
        assert_eq!(parsed.size, sc.size);
        assert_eq!(parsed.title, sc.title);
        assert_eq!(parsed.alt, sc.alt);
        assert!(parsed.caption.is_none());
        assert_eq!(parsed.tags, sc.tags);
    }

    #[test]
    fn translation_sidecar_roundtrip() {
        let ts = MediaTranslationSidecar {
            translation_for: "uuid-123".into(),
            language: "fr".into(),
            title: Some("Mon titre".into()),
            alt: Some("Description".into()),
            caption: None,
        };
        let output = ts.to_string();
        let parsed = read_translation_sidecar(&output).unwrap();
        assert_eq!(parsed.translation_for, "uuid-123");
        assert_eq!(parsed.language, "fr");
        assert_eq!(parsed.title.as_deref(), Some("Mon titre"));
        assert_eq!(parsed.alt.as_deref(), Some("Description"));
        assert!(parsed.caption.is_none());
    }

    #[test]
    fn inline_json_array_parsing() {
        assert_eq!(parse_inline_json_array("[]"), Vec::<String>::new());
        assert_eq!(parse_inline_json_array("[\"a\", \"b\"]"), vec!["a", "b"]);
    }

    #[test]
    fn escape_and_unquote() {
        assert_eq!(unquote_double("\"hello\""), "hello");
        assert_eq!(unquote_double("plain"), "plain");
        assert_eq!(escape_double_quotes("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(
            escape_double_quotes("line one\nline two"),
            "line one\\nline two"
        );
    }

    #[test]
    fn output_matches_bds2_order_empty_links_and_escaping() {
        let sidecar = MediaSidecar {
            id: "media-1".into(),
            original_name: "photo.jpg".into(),
            mime_type: "image/jpeg".into(),
            size: 123,
            width: None,
            height: None,
            title: Some("Photo".into()),
            alt: None,
            caption: Some("First line\nSecond line".into()),
            author: None,
            language: Some("en".into()),
            created_at: 1_711_833_600_000,
            updated_at: 1_711_920_000_000,
            tags: vec!["alpha".into()],
            linked_post_ids: Vec::new(),
        };
        assert_eq!(
            sidecar.to_string(),
            concat!(
                "---\n",
                "id: media-1\n",
                "originalName: \"photo.jpg\"\n",
                "mimeType: image/jpeg\n",
                "size: 123\n",
                "title: \"Photo\"\n",
                "caption: \"First line\\nSecond line\"\n",
                "language: en\n",
                "createdAt: 2024-03-30T21:20:00.000Z\n",
                "updatedAt: 2024-03-31T21:20:00.000Z\n",
                "linkedPostIds: []\n",
                "tags: [\"alpha\"]\n",
                "---",
            )
        );

        let translation = MediaTranslationSidecar {
            translation_for: "media-1".into(),
            language: "de".into(),
            title: None,
            alt: None,
            caption: Some("Erste Zeile\nZweite Zeile".into()),
        };
        assert!(
            translation
                .to_string()
                .contains("caption: \"Erste Zeile\\nZweite Zeile\"")
        );
    }
}
