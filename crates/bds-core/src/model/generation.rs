use serde::{Deserialize, Serialize};

/// Tracks content hashes of generated files to skip unchanged writes.
/// Matches the `generated_file_hashes` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratedFileHash {
    pub project_id: String,
    pub relative_path: String,
    pub content_hash: String,
    pub updated_at: i64,
}

/// Notification for CLI-to-app synchronization.
/// Matches the `db_notifications` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbNotification {
    pub id: i64,
    pub entity: String,
    pub entity_id: String,
    pub action: String,
    pub from_cli: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen_at: Option<i64>,
    pub created_at: i64,
}

/// Publishing preferences stored in meta/publishing.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishingPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_remote_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_mode: Option<String>,
}
