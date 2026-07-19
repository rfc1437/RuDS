use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::schema::db_notifications;
use crate::model::{DbNotification, NotificationAction, NotificationEntity};

#[expect(
    clippy::too_many_arguments,
    reason = "fields mirror the persisted notification record"
)]
pub fn insert_notification(
    conn: &DbConnection,
    entity_type: &NotificationEntity,
    entity_id: &str,
    action: &NotificationAction,
    from_cli: bool,
    seen_at: Option<i64>,
    created_at: i64,
    project_id: Option<&str>,
) -> QueryResult<()> {
    conn.with(|connection| {
        diesel::insert_into(db_notifications::table)
            .values((
                db_notifications::entity_type.eq(entity_type),
                db_notifications::entity_id.eq(entity_id),
                db_notifications::action.eq(action),
                db_notifications::from_cli.eq(i32::from(from_cli)),
                db_notifications::seen_at.eq(seen_at),
                db_notifications::created_at.eq(created_at),
                db_notifications::project_id.eq(project_id),
            ))
            .execute(connection)
            .map(|_| ())
    })
}

pub fn list_notifications(conn: &DbConnection) -> QueryResult<Vec<DbNotification>> {
    conn.with(|connection| {
        db_notifications::table
            .order((
                db_notifications::created_at.asc(),
                db_notifications::id.asc(),
            ))
            .select(DbNotification::as_select())
            .load(connection)
    })
}

pub fn list_unseen_cli_notifications(conn: &DbConnection) -> QueryResult<Vec<DbNotification>> {
    conn.with(|connection| {
        db_notifications::table
            .filter(db_notifications::from_cli.eq(1))
            .filter(db_notifications::seen_at.is_null())
            .order((
                db_notifications::created_at.asc(),
                db_notifications::id.asc(),
            ))
            .select(DbNotification::as_select())
            .load(connection)
    })
}

pub fn mark_notifications_seen(
    conn: &DbConnection,
    ids: &[i32],
    seen_at: i64,
) -> QueryResult<usize> {
    if ids.is_empty() {
        return Ok(0);
    }
    conn.with(|connection| {
        diesel::update(db_notifications::table.filter(db_notifications::id.eq_any(ids)))
            .set(db_notifications::seen_at.eq(seen_at))
            .execute(connection)
    })
}

pub fn prune_processed(conn: &DbConnection, cutoff: i64) -> QueryResult<usize> {
    conn.with(|connection| {
        diesel::delete(
            db_notifications::table
                .filter(db_notifications::seen_at.is_not_null())
                .filter(db_notifications::created_at.le(cutoff)),
        )
        .execute(connection)
    })
}

pub fn prune_unprocessed(conn: &DbConnection, cutoff: i64) -> QueryResult<usize> {
    conn.with(|connection| {
        diesel::delete(
            db_notifications::table
                .filter(db_notifications::seen_at.is_null())
                .filter(db_notifications::created_at.le(cutoff)),
        )
        .execute(connection)
    })
}
