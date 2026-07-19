DROP INDEX IF EXISTS db_notifications_prune_idx;
DROP INDEX IF EXISTS db_notifications_unseen_cli_idx;
ALTER TABLE db_notifications DROP COLUMN project_id;
