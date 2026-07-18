use diesel::ExpressionMethods;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, diesel::AsExpression, diesel::FromSqlRow,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
#[serde(rename_all = "lowercase")]
pub enum PostStatus {
    Draft,
    Published,
    Archived,
}

impl PostStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }

    /// Returns true if this status is valid for a translation (draft or published only).
    pub fn is_valid_for_translation(&self) -> bool {
        matches!(self, PostStatus::Draft | PostStatus::Published)
    }
}

impl std::str::FromStr for PostStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "published" => Ok(Self::Published),
            "archived" => Ok(Self::Archived),
            _ => Err(format!("invalid PostStatus: {value}")),
        }
    }
}

/// A blog post. Matches the `posts` table schema.
///
/// NOTE: `content` is null for published posts — body lives in the filesystem
/// `.md` file only. Draft content is stored in DB.
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Insertable,
    diesel::AsChangeset,
)]
#[diesel(table_name = crate::db::schema::posts, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
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
    #[diesel(
        deserialize_as = crate::db::types::DbBool,
        serialize_as = crate::db::types::DbBool
    )]
    pub do_not_translate: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_slug: Option<String>,
    pub file_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checksum: Option<String>,
    /// JSON-serialized string array in DB.
    #[serde(default)]
    #[diesel(
        deserialize_as = crate::db::types::DbStringList,
        serialize_as = crate::db::types::DbStringList
    )]
    pub tags: Vec<String>,
    /// JSON-serialized string array in DB.
    #[serde(default)]
    #[diesel(
        deserialize_as = crate::db::types::DbStringList,
        serialize_as = crate::db::types::DbStringList
    )]
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
#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Insertable,
    diesel::AsChangeset,
)]
#[diesel(
    table_name = crate::db::schema::post_translations,
    check_for_backend(diesel::sqlite::Sqlite)
)]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
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
#[derive(
    Debug, Clone, Serialize, Deserialize, diesel::Queryable, diesel::Selectable, diesel::Insertable,
)]
#[diesel(table_name = crate::db::schema::post_links, check_for_backend(diesel::sqlite::Sqlite))]
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
#[derive(
    Debug, Clone, Serialize, Deserialize, diesel::Queryable, diesel::Selectable, diesel::Insertable,
)]
#[diesel(table_name = crate::db::schema::post_media, check_for_backend(diesel::sqlite::Sqlite))]
pub struct PostMedia {
    pub id: String,
    pub project_id: String,
    pub post_id: String,
    pub media_id: String,
    pub sort_order: i32,
    pub created_at: i64,
}
