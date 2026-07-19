ALTER TABLE db_notifications ADD COLUMN project_id TEXT;

CREATE INDEX db_notifications_unseen_cli_idx
    ON db_notifications (from_cli, seen_at, created_at);

CREATE INDEX db_notifications_prune_idx
    ON db_notifications (seen_at, created_at);
