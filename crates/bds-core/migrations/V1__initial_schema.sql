-- ================================================================
-- CORE ENTITIES
-- ================================================================

CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    description TEXT,
    data_path TEXT,
    is_active INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS posts (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    title TEXT NOT NULL,
    slug TEXT NOT NULL,
    excerpt TEXT,
    content TEXT,
    status TEXT NOT NULL DEFAULT 'draft',
    author TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    published_at INTEGER,
    file_path TEXT NOT NULL DEFAULT '',
    checksum TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    categories TEXT NOT NULL DEFAULT '[]',
    template_slug TEXT,
    language TEXT,
    do_not_translate INTEGER NOT NULL DEFAULT 0,
    published_title TEXT,
    published_content TEXT,
    published_tags TEXT,
    published_categories TEXT,
    published_excerpt TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS posts_project_slug_idx
    ON posts(project_id, slug);

CREATE TABLE IF NOT EXISTS post_translations (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    translation_for TEXT NOT NULL REFERENCES posts(id),
    language TEXT NOT NULL,
    title TEXT NOT NULL,
    excerpt TEXT,
    content TEXT,
    status TEXT NOT NULL DEFAULT 'draft',
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    published_at INTEGER,
    file_path TEXT NOT NULL DEFAULT '',
    checksum TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS post_translations_translation_language_idx
    ON post_translations(translation_for, language);

CREATE TABLE IF NOT EXISTS media (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    filename TEXT NOT NULL,
    original_name TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    title TEXT,
    alt TEXT,
    caption TEXT,
    author TEXT,
    file_path TEXT NOT NULL,
    sidecar_path TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    checksum TEXT,
    tags TEXT NOT NULL DEFAULT '[]',
    language TEXT
);

CREATE TABLE IF NOT EXISTS media_translations (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    translation_for TEXT NOT NULL REFERENCES media(id),
    language TEXT NOT NULL,
    title TEXT,
    alt TEXT,
    caption TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS media_translations_translation_language_idx
    ON media_translations(translation_for, language);

CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    name TEXT NOT NULL,
    color TEXT,
    post_template_slug TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS tags_project_name_idx
    ON tags(project_id, name);

CREATE TABLE IF NOT EXISTS templates (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'post',
    enabled INTEGER NOT NULL DEFAULT 1,
    version INTEGER NOT NULL DEFAULT 1,
    file_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'published',
    content TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS templates_project_slug_idx
    ON templates(project_id, slug);

CREATE TABLE IF NOT EXISTS scripts (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'utility',
    entrypoint TEXT NOT NULL DEFAULT 'render',
    enabled INTEGER NOT NULL DEFAULT 1,
    version INTEGER NOT NULL DEFAULT 1,
    file_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'published',
    content TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS scripts_project_slug_idx
    ON scripts(project_id, slug);

-- ================================================================
-- RELATIONSHIP TABLES
-- ================================================================

CREATE TABLE IF NOT EXISTS post_links (
    id TEXT PRIMARY KEY,
    source_post_id TEXT NOT NULL REFERENCES posts(id),
    target_post_id TEXT NOT NULL REFERENCES posts(id),
    link_text TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS post_media (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    post_id TEXT NOT NULL REFERENCES posts(id),
    media_id TEXT NOT NULL REFERENCES media(id),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS post_media_post_media_idx
    ON post_media(post_id, media_id);

-- ================================================================
-- METADATA TABLES
-- ================================================================

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS generated_file_hashes (
    project_id TEXT NOT NULL REFERENCES projects(id),
    relative_path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS generated_file_hashes_project_path_idx
    ON generated_file_hashes(project_id, relative_path);

-- ================================================================
-- AI / CHAT TABLES (read-only in Rust core, must not error)
-- ================================================================

CREATE TABLE IF NOT EXISTS chat_conversations (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    model TEXT,
    copilot_session_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS chat_messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id TEXT NOT NULL REFERENCES chat_conversations(id),
    role TEXT NOT NULL,
    content TEXT,
    tool_call_id TEXT,
    tool_calls TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS ai_providers (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    env TEXT,
    npm TEXT,
    api TEXT,
    doc TEXT,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS ai_models (
    provider TEXT NOT NULL REFERENCES ai_providers(id),
    model_id TEXT NOT NULL,
    name TEXT NOT NULL,
    family TEXT,
    attachment INTEGER NOT NULL DEFAULT 0,
    reasoning INTEGER NOT NULL DEFAULT 0,
    tool_call INTEGER NOT NULL DEFAULT 0,
    structured_output INTEGER NOT NULL DEFAULT 0,
    temperature INTEGER NOT NULL DEFAULT 1,
    knowledge TEXT,
    release_date TEXT,
    last_updated_date TEXT,
    open_weights INTEGER NOT NULL DEFAULT 0,
    input_price INTEGER,
    output_price INTEGER,
    cache_read_price INTEGER,
    cache_write_price INTEGER,
    context_window INTEGER NOT NULL DEFAULT 0,
    max_input_tokens INTEGER NOT NULL DEFAULT 0,
    max_output_tokens INTEGER NOT NULL DEFAULT 0,
    interleaved TEXT,
    status TEXT,
    provider_npm TEXT,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (provider, model_id)
);

CREATE TABLE IF NOT EXISTS ai_model_modalities (
    provider TEXT NOT NULL,
    model_id TEXT NOT NULL,
    direction TEXT NOT NULL,
    modality TEXT NOT NULL,
    FOREIGN KEY (provider, model_id) REFERENCES ai_models(provider, model_id)
);

CREATE TABLE IF NOT EXISTS ai_catalog_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- ================================================================
-- EMBEDDINGS TABLES (read-only in Rust core, must not error)
-- ================================================================

CREATE TABLE IF NOT EXISTS embedding_keys (
    label INTEGER PRIMARY KEY,
    post_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    vector TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS dismissed_duplicate_pairs (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    post_id_a TEXT NOT NULL,
    post_id_b TEXT NOT NULL,
    dismissed_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS dismissed_pairs_idx
    ON dismissed_duplicate_pairs(project_id, post_id_a, post_id_b);

-- ================================================================
-- IMPORT TABLES (read-only in Rust core, must not error)
-- ================================================================

CREATE TABLE IF NOT EXISTS import_definitions (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    name TEXT NOT NULL,
    wxr_file_path TEXT,
    uploads_folder_path TEXT,
    last_analysis_result TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- ================================================================
-- NOTIFICATION TABLES
-- ================================================================

CREATE TABLE IF NOT EXISTS db_notifications (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    action TEXT NOT NULL,
    from_cli INTEGER NOT NULL DEFAULT 0,
    seen_at INTEGER,
    created_at INTEGER NOT NULL
);
