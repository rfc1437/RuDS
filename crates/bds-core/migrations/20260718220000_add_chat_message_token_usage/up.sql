-- Your SQL goes here
ALTER TABLE chat_messages ADD COLUMN token_usage_input INTEGER;
ALTER TABLE chat_messages ADD COLUMN token_usage_output INTEGER;
