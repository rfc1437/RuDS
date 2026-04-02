use serde::{Deserialize, Serialize};

/// A user-authored script. Matches the `scripts` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Script {
    pub id: String,
    pub project_id: String,
    pub slug: String,
    pub title: String,
    /// "macro", "transform", or "utility"
    pub kind: String,
    pub entrypoint: String,
    pub enabled: bool,
    pub version: i32,
    pub file_path: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
