CREATE TABLE ai_endpoint_models (
    kind TEXT NOT NULL,
    model_id TEXT NOT NULL,
    label TEXT NOT NULL,
    context_window INTEGER,
    max_output_tokens INTEGER,
    supports_tools INTEGER NOT NULL DEFAULT 0,
    supports_vision INTEGER NOT NULL DEFAULT 0,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (kind, model_id)
);
