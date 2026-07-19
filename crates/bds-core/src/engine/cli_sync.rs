use crate::db::DbConnection as Connection;
use crate::db::queries::db_notification as qn;
use crate::engine::{EngineError, EngineResult, domain_events};
use crate::model::{DomainEntity, DomainEvent, NotificationAction};
use crate::util::now_unix_ms;

pub const PROCESSED_TTL_MS: i64 = 60 * 60 * 1_000;
pub const UNPROCESSED_TTL_MS: i64 = 24 * 60 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PruneResult {
    pub processed: usize,
    pub unprocessed: usize,
}

/// Run a future CLI mutation through the same in-process event path, then
/// persist only the successfully published events for the desktop watcher.
pub fn run_cli_mutation<T>(
    conn: &Connection,
    operation: impl FnOnce() -> EngineResult<T>,
) -> EngineResult<T> {
    let (result, events) = domain_events::capture_current_thread(operation)
        .map_err(|message| EngineError::Validation(message.to_string()))?;
    for event in &events {
        record_cli_event(conn, event)?;
    }
    result
}

pub fn record_cli_event(conn: &Connection, event: &DomainEvent) -> EngineResult<()> {
    record_cli_event_at(conn, event, now_unix_ms())
}

#[doc(hidden)]
pub fn record_cli_event_at(
    conn: &Connection,
    event: &DomainEvent,
    created_at: i64,
) -> EngineResult<()> {
    let (entity, entity_id, action, project_id) = match event {
        DomainEvent::EntityChanged {
            project_id,
            entity,
            entity_id,
            action,
        } => (
            entity.clone(),
            entity_id.as_str(),
            action.clone(),
            Some(project_id.as_str()),
        ),
        DomainEvent::SettingsChanged { project_id, key } => (
            DomainEntity::Setting,
            key.as_str(),
            NotificationAction::Updated,
            project_id.as_deref(),
        ),
    };
    qn::insert_notification(
        conn, &entity, entity_id, &action, true, None, created_at, project_id,
    )?;
    Ok(())
}

pub fn consume_cli_notifications(conn: &Connection) -> EngineResult<Vec<DomainEvent>> {
    consume_cli_notifications_at(conn, now_unix_ms())
}

#[doc(hidden)]
pub fn consume_cli_notifications_at(
    conn: &Connection,
    seen_at: i64,
) -> EngineResult<Vec<DomainEvent>> {
    conn.begin_savepoint()?;
    let result = (|| {
        let notifications = qn::list_unseen_cli_notifications(conn)?;
        let ids = notifications.iter().map(|item| item.id).collect::<Vec<_>>();
        qn::mark_notifications_seen(conn, &ids, seen_at)?;
        let events = notifications
            .into_iter()
            .filter_map(|notification| match notification.entity_type {
                DomainEntity::Setting => Some(DomainEvent::SettingsChanged {
                    project_id: notification.project_id,
                    key: notification.entity_id,
                }),
                entity => notification
                    .project_id
                    .or_else(|| legacy_project_id(conn, &entity, &notification.entity_id))
                    .map(|project_id| DomainEvent::EntityChanged {
                        project_id,
                        entity,
                        entity_id: notification.entity_id,
                        action: notification.action,
                    }),
            })
            .collect();
        Ok(events)
    })();
    match result {
        Ok(events) => {
            conn.release_savepoint()?;
            Ok(events)
        }
        Err(error) => {
            let _ = conn.rollback_savepoint();
            Err(error)
        }
    }
}

fn legacy_project_id(conn: &Connection, entity: &DomainEntity, entity_id: &str) -> Option<String> {
    let resolved = match entity {
        DomainEntity::Post => crate::db::queries::post::get_post_by_id(conn, entity_id)
            .ok()
            .map(|item| item.project_id),
        DomainEntity::Media => crate::db::queries::media::get_media_by_id(conn, entity_id)
            .ok()
            .map(|item| item.project_id),
        DomainEntity::Tag => crate::db::queries::tag::get_tag_by_id(conn, entity_id)
            .ok()
            .map(|item| item.project_id),
        DomainEntity::Template => crate::db::queries::template::get_template_by_id(conn, entity_id)
            .ok()
            .map(|item| item.project_id),
        DomainEntity::Script => crate::db::queries::script::get_script_by_id(conn, entity_id)
            .ok()
            .map(|item| item.project_id),
        DomainEntity::Project => Some(entity_id.to_string()),
        DomainEntity::Setting => None,
    };
    resolved.or_else(|| {
        crate::db::queries::project::get_active_project(conn)
            .ok()
            .map(|project| project.id)
    })
}

pub fn prune_notifications(conn: &Connection) -> EngineResult<PruneResult> {
    prune_notifications_at(conn, now_unix_ms())
}

#[doc(hidden)]
pub fn prune_notifications_at(conn: &Connection, now: i64) -> EngineResult<PruneResult> {
    Ok(PruneResult {
        processed: qn::prune_processed(conn, now - PROCESSED_TTL_MS)?,
        unprocessed: qn::prune_unprocessed(conn, now - UNPROCESSED_TTL_MS)?,
    })
}

/// Desktop watcher poll: consume once, publish through the shared bus, then
/// apply both retention windows.
pub fn poll_notifications(conn: &Connection) -> EngineResult<usize> {
    let events = consume_cli_notifications(conn)?;
    let count = events.len();
    for event in events {
        domain_events::publish(event);
    }
    prune_notifications(conn)?;
    Ok(count)
}
