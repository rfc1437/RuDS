-- This file should undo anything in `up.sql`
ALTER TABLE chat_messages DROP COLUMN cache_write_tokens;
ALTER TABLE chat_messages DROP COLUMN cache_read_tokens;
