-- Your SQL goes here
CREATE TABLE embedding_keys_new (
    label INTEGER NOT NULL PRIMARY KEY,
    post_id TEXT NOT NULL,
    project_id TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    vector BLOB NOT NULL
);

INSERT INTO embedding_keys_new (label, post_id, project_id, content_hash, vector)
SELECT label, post_id, project_id, content_hash, CAST(vector AS BLOB)
FROM embedding_keys;

DROP TABLE embedding_keys;
ALTER TABLE embedding_keys_new RENAME TO embedding_keys;

CREATE UNIQUE INDEX embedding_keys_project_post_idx
    ON embedding_keys(project_id, post_id);
