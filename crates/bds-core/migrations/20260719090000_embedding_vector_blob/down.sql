-- This file should undo anything in `up.sql`
CREATE TABLE embedding_keys_old (
    label INTEGER NOT NULL PRIMARY KEY,
    post_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    vector TEXT NOT NULL
);

INSERT INTO embedding_keys_old (label, post_id, project_id, content_hash, vector)
SELECT label, post_id, project_id, content_hash, CAST(vector AS TEXT)
FROM embedding_keys;

DROP TABLE embedding_keys;
ALTER TABLE embedding_keys_old RENAME TO embedding_keys;
