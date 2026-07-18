ALTER TABLE chat_messages ADD COLUMN cache_read_tokens INTEGER;
ALTER TABLE chat_messages ADD COLUMN cache_write_tokens INTEGER;

ALTER TABLE ai_providers RENAME COLUMN npm TO package_ref;
ALTER TABLE ai_models RENAME COLUMN provider_npm TO provider_package_ref;
