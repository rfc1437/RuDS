use serde::{Deserialize, Serialize};

/// A bDS project. Matches the `projects` table schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_path: Option<String>,
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Key-value settings. Matches the `settings` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub updated_at: i64,
}
