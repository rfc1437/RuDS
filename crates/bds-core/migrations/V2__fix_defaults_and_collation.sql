-- Fix script and template default status from 'published' to 'draft'
-- and make tag name uniqueness case-insensitive.
--
-- SQLite cannot ALTER COLUMN defaults, so we recreate the affected tables.

-- ── Scripts: default status 'published' → 'draft' ──

CREATE TABLE scripts_new (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'utility',
    entrypoint TEXT NOT NULL DEFAULT 'render',
    enabled INTEGER NOT NULL DEFAULT 1,
    version INTEGER NOT NULL DEFAULT 1,
    file_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    content TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

INSERT INTO scripts_new SELECT * FROM scripts;
DROP TABLE scripts;
ALTER TABLE scripts_new RENAME TO scripts;

CREATE UNIQUE INDEX IF NOT EXISTS scripts_project_slug_idx
    ON scripts(project_id, slug);

-- ── Templates: default status 'published' → 'draft' ──

CREATE TABLE templates_new (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    slug TEXT NOT NULL,
    title TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'post',
    enabled INTEGER NOT NULL DEFAULT 1,
    version INTEGER NOT NULL DEFAULT 1,
    file_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    content TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

INSERT INTO templates_new SELECT * FROM templates;
DROP TABLE templates;
ALTER TABLE templates_new RENAME TO templates;

CREATE UNIQUE INDEX IF NOT EXISTS templates_project_slug_idx
    ON templates(project_id, slug);

-- ── Tags: case-insensitive unique index ──

DROP INDEX IF EXISTS tags_project_name_idx;
CREATE UNIQUE INDEX tags_project_name_idx ON tags(project_id, name COLLATE NOCASE);
