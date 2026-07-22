CREATE TABLE mcp_proposals_new (
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
    entity_id TEXT NOT NULL,
    data TEXT NOT NULL,
    result TEXT,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    resolved_at BIGINT
);

INSERT INTO mcp_proposals_new (
    id, project_id, kind, status, entity_id, data, result,
    created_at, expires_at, resolved_at
)
SELECT
    id, project_id, kind, status, COALESCE(entity_id, id), data, result,
    created_at, expires_at, resolved_at
FROM mcp_proposals;

DROP TABLE mcp_proposals;
ALTER TABLE mcp_proposals_new RENAME TO mcp_proposals;

-- Keep the newest record if a pre-migration database already contains an
-- invalid duplicate. NULL-backed proposals were assigned their own IDs above
-- and therefore remain distinct.
DELETE FROM mcp_proposals AS duplicate
WHERE EXISTS (
    SELECT 1
    FROM mcp_proposals AS keeper
    WHERE keeper.kind = duplicate.kind
      AND keeper.entity_id = duplicate.entity_id
      AND keeper.status = duplicate.status
      AND (
          keeper.created_at > duplicate.created_at
          OR (keeper.created_at = duplicate.created_at AND keeper.id > duplicate.id)
      )
);

CREATE INDEX mcp_proposals_project_status_idx
    ON mcp_proposals(project_id, status, created_at);

CREATE UNIQUE INDEX mcp_proposals_entity_idx
    ON mcp_proposals(kind, entity_id, status);
