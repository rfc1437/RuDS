use serde::{Deserialize, Serialize};

/// A media item (image, video, etc.).
/// Matches the `media` table schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Media {
    pub id: String,
    pub project_id: String,
    pub filename: String,
    pub original_name: String,
    pub mime_type: String,
    pub size: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub file_path: String,
    pub sidecar_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    /// JSON-serialized string array in DB.
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A translation of media metadata into another language.
/// Matches the `media_translations` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaTranslation {
    pub id: String,
    pub project_id: String,
    pub translation_for: String,
    pub language: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
