-- This file should undo anything in `up.sql`
ALTER TABLE chat_messages DROP COLUMN token_usage_output;
ALTER TABLE chat_messages DROP COLUMN token_usage_input;
