use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PostStatus {
    Draft,
    Published,
    Archived,
}

/// A blog post. Matches the `posts` table schema.
///
/// NOTE: `content` is null for published posts — body lives in the filesystem
/// `.md` file only. Draft content is stored in DB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    /// Draft body text. Null/empty when published (content is in the file).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub status: PostStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default)]
    pub do_not_translate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_slug: Option<String>,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    /// JSON-serialized string array in DB.
    #[serde(default)]
    pub tags: Vec<String>,
    /// JSON-serialized string array in DB.
    #[serde(default)]
    pub categories: Vec<String>,
    // Published snapshot fields (used for diff detection)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_tags: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_categories: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_excerpt: Option<String>,
    /// Unix timestamp (integer in DB).
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<i64>,
}

/// A translation of a post into another language.
/// Matches the `post_translations` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostTranslation {
    pub id: String,
    pub project_id: String,
    pub translation_for: String,
    pub language: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub status: PostStatus,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<i64>,
}

/// A link between two posts (tracked for backlinks).
/// Matches the `post_links` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostLink {
    pub id: String,
    pub source_post_id: String,
    pub target_post_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_text: Option<String>,
    pub created_at: i64,
}

/// Association between a post and media item.
/// Matches the `post_media` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostMedia {
    pub id: String,
    pub project_id: String,
    pub post_id: String,
    pub media_id: String,
    pub sort_order: i32,
    pub created_at: i64,
}
