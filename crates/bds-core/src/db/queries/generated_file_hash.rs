use rusqlite::{Connection, params};

use crate::db::from_row::{GENERATED_FILE_HASH_COLUMNS, generated_file_hash_from_row};
use crate::model::GeneratedFileHash;

pub fn get_generated_file_hash(
    conn: &Connection,
    project_id: &str,
    relative_path: &str,
) -> rusqlite::Result<GeneratedFileHash> {
    conn.query_row(
        &format!(
            "SELECT {GENERATED_FILE_HASH_COLUMNS} FROM generated_file_hashes WHERE project_id = ?1 AND relative_path = ?2"
        ),
        params![project_id, relative_path],
        generated_file_hash_from_row,
    )
}

pub fn upsert_generated_file_hash(
    conn: &Connection,
    hash: &GeneratedFileHash,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO generated_file_hashes (project_id, relative_path, content_hash, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(project_id, relative_path)
         DO UPDATE SET content_hash = excluded.content_hash, updated_at = excluded.updated_at",
        params![
            hash.project_id,
            hash.relative_path,
            hash.content_hash,
            hash.updated_at
        ],
    )?;
    Ok(())
}

pub fn delete_generated_file_hash(
    conn: &Connection,
    project_id: &str,
    relative_path: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "DELETE FROM generated_file_hashes WHERE project_id = ?1 AND relative_path = ?2",
        params![project_id, relative_path],
    )?;
    Ok(())
}

pub fn list_generated_file_hashes_by_project(
    conn: &Connection,
    project_id: &str,
) -> rusqlite::Result<Vec<GeneratedFileHash>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {GENERATED_FILE_HASH_COLUMNS} FROM generated_file_hashes WHERE project_id = ?1 ORDER BY relative_path"
    ))?;
    let rows = stmt.query_map(params![project_id], generated_file_hash_from_row)?;
    rows.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::project::{insert_project, make_test_project};

    fn setup() -> Database {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        db
    }

    #[test]
    fn upsert_and_get_generated_hash() {
        let db = setup();
        let hash = GeneratedFileHash {
            project_id: "p1".into(),
            relative_path: "index.html".into(),
            content_hash: "abc".into(),
            updated_at: 42,
        };

        upsert_generated_file_hash(db.conn(), &hash).unwrap();
        let stored = get_generated_file_hash(db.conn(), "p1", "index.html").unwrap();
        assert_eq!(stored.content_hash, "abc");

        upsert_generated_file_hash(
            db.conn(),
            &GeneratedFileHash {
                content_hash: "def".into(),
                updated_at: 99,
                ..hash
            },
        )
        .unwrap();
        let stored = get_generated_file_hash(db.conn(), "p1", "index.html").unwrap();
        assert_eq!(stored.content_hash, "def");
        assert_eq!(stored.updated_at, 99);
    }
}
