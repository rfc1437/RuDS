use serde::{Deserialize, Serialize};

/// Tracks content hashes of generated files to skip unchanged writes.
/// Matches the `generated_file_hashes` table.
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
    table_name = crate::db::schema::generated_file_hashes,
    check_for_backend(diesel::sqlite::Sqlite)
)]
pub struct GeneratedFileHash {
    pub project_id: String,
    pub relative_path: String,
    pub content_hash: String,
    pub updated_at: i64,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, diesel::AsExpression, diesel::FromSqlRow,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
#[serde(rename_all = "lowercase")]
pub enum DomainEntity {
    Post,
    Media,
    Tag,
    Script,
    Template,
    Project,
    Setting,
}

pub type NotificationEntity = DomainEntity;

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, diesel::AsExpression, diesel::FromSqlRow,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
#[serde(rename_all = "lowercase")]
pub enum NotificationAction {
    Created,
    Updated,
    Deleted,
}

impl std::str::FromStr for DomainEntity {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "post" => Ok(Self::Post),
            "media" => Ok(Self::Media),
            "tag" => Ok(Self::Tag),
            "script" => Ok(Self::Script),
            "template" => Ok(Self::Template),
            "project" => Ok(Self::Project),
            "setting" => Ok(Self::Setting),
            _ => Err(format!("invalid NotificationEntity: {value}")),
        }
    }
}

impl DomainEntity {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Post => "post",
            Self::Media => "media",
            Self::Tag => "tag",
            Self::Script => "script",
            Self::Template => "template",
            Self::Project => "project",
            Self::Setting => "setting",
        }
    }
}

impl std::str::FromStr for NotificationAction {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "created" => Ok(Self::Created),
            "updated" => Ok(Self::Updated),
            "deleted" => Ok(Self::Deleted),
            _ => Err(format!("invalid NotificationAction: {value}")),
        }
    }
}

impl NotificationAction {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Deleted => "deleted",
        }
    }
}

/// Notification for CLI-to-app synchronization.
/// Matches the `db_notifications` table.
#[derive(Debug, Clone, Serialize, Deserialize, diesel::Queryable, diesel::Selectable)]
#[diesel(
    table_name = crate::db::schema::db_notifications,
    check_for_backend(diesel::sqlite::Sqlite)
)]
pub struct DbNotification {
    pub id: i32,
    pub entity_type: NotificationEntity,
    pub entity_id: String,
    pub action: NotificationAction,
    #[diesel(deserialize_as = crate::db::types::DbBool)]
    pub from_cli: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen_at: Option<i64>,
    pub created_at: i64,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SshMode {
    Scp,
    Rsync,
}

/// Publishing preferences stored in meta/publishing.json.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishingPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_host: Option<String>,
    #[serde(default = "default_ssh_mode")]
    pub ssh_mode: SshMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_remote_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssh_user: Option<String>,
}

impl Default for PublishingPreferences {
    fn default() -> Self {
        Self {
            ssh_host: None,
            ssh_user: None,
            ssh_remote_path: None,
            ssh_mode: default_ssh_mode(),
        }
    }
}

fn default_ssh_mode() -> SshMode {
    SshMode::Scp
}
