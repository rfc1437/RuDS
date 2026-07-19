-- Your SQL goes here
ALTER TABLE chat_messages ADD COLUMN cache_read_tokens INTEGER;
ALTER TABLE chat_messages ADD COLUMN cache_write_tokens INTEGER;
