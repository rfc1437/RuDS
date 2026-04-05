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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationEntity {
    Post,
    Media,
    Script,
    Template,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationAction {
    Created,
    Updated,
    Deleted,
}

/// Notification for CLI-to-app synchronization.
/// Matches the `db_notifications` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbNotification {
    pub id: i64,
    pub entity_type: NotificationEntity,
    pub entity_id: String,
    pub action: NotificationAction,
    pub from_cli: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen_at: Option<i64>,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SshMode {
    Scp,
    Rsync,
}

/// Publishing preferences stored in meta/publishing.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishingPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_remote_path: Option<String>,
    #[serde(default = "default_ssh_mode")]
    pub ssh_mode: SshMode,
}

fn default_ssh_mode() -> SshMode {
    SshMode::Scp
}
