DROP INDEX mcp_proposals_entity_idx;

CREATE TABLE mcp_proposals_old (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    kind TEXT NOT NULL CHECK (kind IN (
        'draft_post',
        'propose_script',
        'propose_template',
        'propose_media_translation',
        'propose_media_metadata',
        'propose_post_metadata'
    )),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN (
        'pending', 'executing', 'accepted', 'rejected', 'expired'
    )),
    entity_id TEXT,
    data TEXT NOT NULL,
    result TEXT,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    resolved_at BIGINT
);

INSERT INTO mcp_proposals_old (
    id, project_id, kind, status, entity_id, data, result,
    created_at, expires_at, resolved_at
)
SELECT
    id, project_id, kind, status, entity_id, data, result,
    created_at, expires_at, resolved_at
FROM mcp_proposals;

DROP TABLE mcp_proposals;
ALTER TABLE mcp_proposals_old RENAME TO mcp_proposals;

CREATE INDEX mcp_proposals_project_status_idx
    ON mcp_proposals(project_id, status, created_at);
