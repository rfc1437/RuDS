use bds_core::db::Database;
use bds_core::engine::{cli_sync, domain_events};
use bds_core::model::{
    DomainEntity, DomainEvent, NotificationAction, NotificationEntity, Project, ScriptKind,
    TemplateKind,
};
use image::DynamicImage;
use tempfile::TempDir;
use uuid::Uuid;

fn post_event(project_id: &str, id: &str, action: NotificationAction) -> DomainEvent {
    DomainEvent::EntityChanged {
        project_id: project_id.to_string(),
        entity: DomainEntity::Post,
        entity_id: id.to_string(),
        action,
    }
}

fn test_project(id: &str, slug: &str) -> Project {
    Project {
        id: id.to_string(),
        name: format!("Project {id}"),
        slug: slug.to_string(),
        description: None,
        data_path: None,
        is_active: false,
        created_at: 1,
        updated_at: 1,
    }
}

fn assert_one_entity_event(
    subscription: &domain_events::EventSubscription,
    entity: DomainEntity,
    id: &str,
    action: NotificationAction,
) {
    let matching = subscription
        .drain()
        .into_iter()
        .filter(|event| {
            matches!(
                event,
                DomainEvent::EntityChanged {
                    entity: actual_entity,
                    entity_id,
                    action: actual_action,
                    ..
                } if actual_entity == &entity && entity_id == id && actual_action == &action
            )
        })
        .count();
    assert_eq!(matching, 1, "{entity:?} {action:?} must emit once");
}

#[test]
fn subscribers_receive_events_in_order_and_unsubscribe_stops_delivery() {
    let bus = domain_events::EventBus::default();
    let first = bus.subscribe();
    let second = bus.subscribe();
    let created = post_event("project", "one", NotificationAction::Created);
    let updated = post_event("project", "one", NotificationAction::Updated);

    bus.publish(created.clone());
    bus.publish(updated.clone());
    assert_eq!(first.drain(), vec![created.clone(), updated.clone()]);
    assert_eq!(second.drain(), vec![created, updated]);

    first.unsubscribe();
    let deleted = post_event("project", "one", NotificationAction::Deleted);
    bus.publish(deleted.clone());
    assert_eq!(second.drain(), vec![deleted]);
}

#[test]
fn cli_notifications_are_consumed_once_marked_seen_and_pruned_by_age() {
    let db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    let now = 2 * cli_sync::UNPROCESSED_TTL_MS;
    let recent = post_event("project", "recent", NotificationAction::Updated);
    cli_sync::record_cli_event_at(db.conn(), &recent, now).unwrap();
    cli_sync::record_cli_event_at(
        db.conn(),
        &post_event("project", "processed-old", NotificationAction::Updated),
        now - cli_sync::PROCESSED_TTL_MS - 1,
    )
    .unwrap();
    let events = cli_sync::consume_cli_notifications_at(db.conn(), now).unwrap();
    assert_eq!(events.len(), 2);
    assert!(
        cli_sync::consume_cli_notifications_at(db.conn(), now)
            .unwrap()
            .is_empty()
    );

    let notifications =
        bds_core::db::queries::db_notification::list_notifications(db.conn()).unwrap();
    assert!(notifications.iter().all(|item| item.seen_at == Some(now)));
    assert!(notifications.iter().all(|item| item.from_cli));
    assert!(
        notifications
            .iter()
            .all(|item| item.project_id.as_deref() == Some("project"))
    );

    cli_sync::record_cli_event_at(
        db.conn(),
        &post_event("project", "unprocessed-old", NotificationAction::Updated),
        now - cli_sync::UNPROCESSED_TTL_MS - 1,
    )
    .unwrap();

    let pruned = cli_sync::prune_notifications_at(db.conn(), now).unwrap();
    assert_eq!(pruned.processed, 1);
    assert_eq!(pruned.unprocessed, 1);
    let remaining = bds_core::db::queries::db_notification::list_notifications(db.conn()).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].entity_id, "recent");
}

#[test]
fn desktop_events_do_not_create_cli_notification_rows() {
    let db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    domain_events::publish(post_event(
        "project",
        "desktop",
        NotificationAction::Created,
    ));

    assert!(
        bds_core::db::queries::db_notification::list_notifications(db.conn())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn legacy_cli_rows_without_scope_resolve_the_entity_project() {
    let db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    bds_core::db::fts::ensure_fts_tables(db.conn()).unwrap();
    let project_id = Uuid::new_v4().to_string();
    bds_core::db::queries::project::insert_project(db.conn(), &test_project(&project_id, "legacy"))
        .unwrap();
    let dir = TempDir::new().unwrap();
    let post = bds_core::engine::post::create_post(
        db.conn(),
        dir.path(),
        &project_id,
        "Legacy",
        None,
        vec![],
        vec![],
        None,
        None,
        None,
    )
    .unwrap();
    bds_core::db::queries::db_notification::insert_notification(
        db.conn(),
        &DomainEntity::Post,
        &post.id,
        &NotificationAction::Updated,
        true,
        None,
        1,
        None,
    )
    .unwrap();

    assert_eq!(
        cli_sync::consume_cli_notifications_at(db.conn(), 2).unwrap(),
        vec![post_event(
            &project_id,
            &post.id,
            NotificationAction::Updated
        )]
    );
}

#[test]
fn notification_entity_types_cover_every_shared_mutation_family() {
    assert_eq!(NotificationEntity::Post.as_str(), "post");
    assert_eq!(NotificationEntity::Media.as_str(), "media");
    assert_eq!(NotificationEntity::Tag.as_str(), "tag");
    assert_eq!(NotificationEntity::Template.as_str(), "template");
    assert_eq!(NotificationEntity::Script.as_str(), "script");
    assert_eq!(NotificationEntity::Project.as_str(), "project");
    assert_eq!(NotificationEntity::Setting.as_str(), "setting");
}

#[test]
fn representative_shared_mutations_emit_exactly_one_typed_event_each() {
    let db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    bds_core::db::fts::ensure_fts_tables(db.conn()).unwrap();
    let project_id = Uuid::new_v4().to_string();
    bds_core::db::queries::project::insert_project(db.conn(), &test_project(&project_id, "events"))
        .unwrap();
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("event.png");
    DynamicImage::new_rgb8(2, 2).save(&source).unwrap();
    let setting_key = format!("test.event.{}", Uuid::new_v4());
    let subscription = domain_events::subscribe();

    let post = bds_core::engine::post::create_post(
        db.conn(),
        dir.path(),
        &project_id,
        "Post",
        Some("Body"),
        vec![],
        vec![],
        None,
        None,
        None,
    )
    .unwrap();
    let media = bds_core::engine::media::import_media(
        db.conn(),
        dir.path(),
        &project_id,
        &source,
        "event.png",
        None,
        None,
        None,
        None,
        None,
        vec![],
    )
    .unwrap();
    let tag = bds_core::engine::tag::create_tag(db.conn(), dir.path(), &project_id, "Event", None)
        .unwrap();
    let template = bds_core::engine::template::create_template(
        db.conn(),
        &project_id,
        "Event",
        TemplateKind::Post,
        "{{ content }}",
    )
    .unwrap();
    let script = bds_core::engine::script::create_script(
        db.conn(),
        &project_id,
        "Event",
        ScriptKind::Utility,
        "function main() end",
        None,
    )
    .unwrap();
    let created_project = bds_core::engine::project::create_project(
        db.conn(),
        "Event Project",
        Some(dir.path().join("project").to_string_lossy().as_ref()),
    )
    .unwrap();
    bds_core::engine::settings::set(db.conn(), &setting_key, "value").unwrap();

    let relevant = subscription
        .drain()
        .into_iter()
        .filter(|event| match event {
            DomainEvent::EntityChanged {
                project_id: scope, ..
            } => scope == &project_id || scope == &created_project.id,
            DomainEvent::SettingsChanged { key, .. } => key == &setting_key,
        })
        .collect::<Vec<_>>();
    assert_eq!(relevant.len(), 7);
    for (entity, id) in [
        (DomainEntity::Post, post.id.clone()),
        (DomainEntity::Media, media.id.clone()),
        (DomainEntity::Tag, tag.id.clone()),
        (DomainEntity::Template, template.id.clone()),
        (DomainEntity::Script, script.id.clone()),
        (DomainEntity::Project, created_project.id.clone()),
    ] {
        assert_eq!(
            relevant
                .iter()
                .filter(|event| matches!(
                    event,
                    DomainEvent::EntityChanged {
                        entity: actual_entity,
                        entity_id,
                        action: NotificationAction::Created,
                        ..
                    } if actual_entity == &entity && entity_id == &id
                ))
                .count(),
            1,
            "{entity:?} mutation must emit once"
        );
    }
    assert_eq!(
        relevant
            .iter()
            .filter(|event| matches!(
                event,
                DomainEvent::SettingsChanged { key, project_id: None } if key == &setting_key
            ))
            .count(),
        1
    );

    bds_core::engine::post::update_post(
        db.conn(),
        dir.path(),
        &post.id,
        Some("Updated Post"),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Post,
        &post.id,
        NotificationAction::Updated,
    );
    bds_core::engine::post::publish_post(db.conn(), dir.path(), &post.id).unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Post,
        &post.id,
        NotificationAction::Updated,
    );
    bds_core::engine::post::archive_post(db.conn(), dir.path(), &post.id).unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Post,
        &post.id,
        NotificationAction::Updated,
    );
    bds_core::engine::post::unarchive_post(db.conn(), dir.path(), &post.id).unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Post,
        &post.id,
        NotificationAction::Updated,
    );

    bds_core::engine::media::update_media(
        db.conn(),
        dir.path(),
        &media.id,
        Some(Some("Updated Media")),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Media,
        &media.id,
        NotificationAction::Updated,
    );
    bds_core::engine::tag::update_tag(
        db.conn(),
        dir.path(),
        &tag.id,
        Some("Updated Tag"),
        None,
        None,
    )
    .unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Tag,
        &tag.id,
        NotificationAction::Updated,
    );
    bds_core::engine::template::publish_template(db.conn(), dir.path(), &template.id).unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Template,
        &template.id,
        NotificationAction::Updated,
    );
    bds_core::engine::script::publish_script(db.conn(), dir.path(), &script.id).unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Script,
        &script.id,
        NotificationAction::Updated,
    );

    bds_core::engine::post::delete_post(db.conn(), dir.path(), &post.id).unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Post,
        &post.id,
        NotificationAction::Deleted,
    );
    bds_core::engine::media::delete_media(db.conn(), dir.path(), &media.id).unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Media,
        &media.id,
        NotificationAction::Deleted,
    );
    bds_core::engine::tag::delete_tag(db.conn(), dir.path(), &project_id, &tag.id).unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Tag,
        &tag.id,
        NotificationAction::Deleted,
    );
    bds_core::engine::template::delete_template(db.conn(), dir.path(), &template.id, false)
        .unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Template,
        &template.id,
        NotificationAction::Deleted,
    );
    bds_core::engine::script::delete_script(db.conn(), dir.path(), &script.id).unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Script,
        &script.id,
        NotificationAction::Deleted,
    );
    bds_core::engine::project::delete_project(
        db.conn(),
        &created_project.id,
        Some(&dir.path().join("project")),
    )
    .unwrap();
    assert_one_entity_event(
        &subscription,
        DomainEntity::Project,
        &created_project.id,
        NotificationAction::Deleted,
    );
}

#[test]
fn cli_mutation_persists_the_shared_event_for_the_desktop_process() {
    let db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    bds_core::db::fts::ensure_fts_tables(db.conn()).unwrap();
    let project_id = Uuid::new_v4().to_string();
    bds_core::db::queries::project::insert_project(db.conn(), &test_project(&project_id, "cli"))
        .unwrap();
    let dir = TempDir::new().unwrap();

    let post = cli_sync::run_cli_mutation(db.conn(), || {
        bds_core::engine::post::create_post(
            db.conn(),
            dir.path(),
            &project_id,
            "CLI",
            None,
            vec![],
            vec![],
            None,
            None,
            None,
        )
    })
    .unwrap();

    let notifications =
        bds_core::db::queries::db_notification::list_notifications(db.conn()).unwrap();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].entity_id, post.id);
    assert_eq!(
        notifications[0].project_id.as_deref(),
        Some(project_id.as_str())
    );
    assert!(notifications[0].from_cli);
}

#[test]
fn cli_mutation_deduplicates_composite_events_and_keeps_the_final_state() {
    let db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();

    let result: bds_core::engine::EngineResult<()> = cli_sync::run_cli_mutation(db.conn(), || {
        bds_core::engine::domain_events::entity_changed(
            "project",
            DomainEntity::Post,
            "created-then-updated",
            NotificationAction::Created,
        );
        bds_core::engine::domain_events::entity_changed(
            "project",
            DomainEntity::Post,
            "created-then-updated",
            NotificationAction::Updated,
        );
        bds_core::engine::domain_events::entity_changed(
            "project",
            DomainEntity::Media,
            "updated-then-deleted",
            NotificationAction::Updated,
        );
        bds_core::engine::domain_events::entity_changed(
            "project",
            DomainEntity::Media,
            "updated-then-deleted",
            NotificationAction::Deleted,
        );
        Err(bds_core::engine::EngineError::Validation(
            "later composite step failed".into(),
        ))
    });
    assert!(result.is_err());

    let notifications =
        bds_core::db::queries::db_notification::list_notifications(db.conn()).unwrap();
    assert_eq!(notifications.len(), 2);
    assert_eq!(notifications[0].entity_id, "created-then-updated");
    assert_eq!(notifications[0].action, NotificationAction::Created);
    assert_eq!(notifications[1].entity_id, "updated-then-deleted");
    assert_eq!(notifications[1].action, NotificationAction::Deleted);
}
