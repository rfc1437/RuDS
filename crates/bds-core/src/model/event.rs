use serde::{Deserialize, Serialize};

use super::{DomainEntity, NotificationAction};

/// The single event shape shared by desktop, CLI synchronization, and future
/// remote clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomainEvent {
    EntityChanged {
        project_id: String,
        entity: DomainEntity,
        entity_id: String,
        action: NotificationAction,
    },
    SettingsChanged {
        project_id: Option<String>,
        key: String,
    },
}

impl DomainEvent {
    pub fn project_id(&self) -> Option<&str> {
        match self {
            Self::EntityChanged { project_id, .. } => Some(project_id),
            Self::SettingsChanged { project_id, .. } => project_id.as_deref(),
        }
    }
}
