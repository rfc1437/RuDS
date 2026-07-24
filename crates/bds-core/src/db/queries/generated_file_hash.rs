use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::schema::generated_file_hashes;
use crate::model::GeneratedFileHash;

pub fn get_generated_file_hash(
    conn: &DbConnection,
    project_id: &str,
    relative_path: &str,
) -> QueryResult<GeneratedFileHash> {
    conn.with(|c| {
        generated_file_hashes::table
            .filter(generated_file_hashes::project_id.eq(project_id))
            .filter(generated_file_hashes::relative_path.eq(relative_path))
            .select(GeneratedFileHash::as_select())
            .first(c)
    })
}

pub fn list_generated_file_hashes(
    conn: &DbConnection,
    project_id: &str,
) -> QueryResult<Vec<GeneratedFileHash>> {
    conn.with(|c| {
        generated_file_hashes::table
            .filter(generated_file_hashes::project_id.eq(project_id))
            .select(GeneratedFileHash::as_select())
            .load(c)
    })
}

pub fn upsert_generated_file_hash(
    conn: &DbConnection,
    hash: &GeneratedFileHash,
) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(generated_file_hashes::table)
            .values(hash.clone())
            .on_conflict((
                generated_file_hashes::project_id,
                generated_file_hashes::relative_path,
            ))
            .do_update()
            .set((
                generated_file_hashes::content_hash.eq(&hash.content_hash),
                generated_file_hashes::updated_at.eq(hash.updated_at),
            ))
            .execute(c)
            .map(|_| ())
    })
}

pub fn upsert_generated_file_hashes(
    conn: &DbConnection,
    hashes: &[GeneratedFileHash],
) -> QueryResult<()> {
    conn.with(|c| {
        hashes.chunks(200).try_for_each(|chunk| {
            diesel::insert_into(generated_file_hashes::table)
                .values(chunk)
                .on_conflict((
                    generated_file_hashes::project_id,
                    generated_file_hashes::relative_path,
                ))
                .do_update()
                .set((
                    generated_file_hashes::content_hash.eq(diesel::upsert::excluded(
                        generated_file_hashes::content_hash,
                    )),
                    generated_file_hashes::updated_at
                        .eq(diesel::upsert::excluded(generated_file_hashes::updated_at)),
                ))
                .execute(c)
                .map(|_| ())
        })
    })
}

pub fn touch_generated_file_hashes(
    conn: &DbConnection,
    project_id: &str,
    relative_paths: &[String],
    updated_at: i64,
) -> QueryResult<usize> {
    conn.with(|c| {
        relative_paths.chunks(500).try_fold(0, |updated, paths| {
            diesel::update(
                generated_file_hashes::table
                    .filter(generated_file_hashes::project_id.eq(project_id))
                    .filter(generated_file_hashes::relative_path.eq_any(paths)),
            )
            .set(generated_file_hashes::updated_at.eq(updated_at))
            .execute(c)
            .map(|count| updated + count)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::project::{insert_project, make_test_project};

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
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

    #[test]
    fn batch_upsert_inserts_and_updates_hashes() {
        let db = setup();
        let mut hashes = vec![
            GeneratedFileHash {
                project_id: "p1".into(),
                relative_path: "index.html".into(),
                content_hash: "one".into(),
                updated_at: 1,
            },
            GeneratedFileHash {
                project_id: "p1".into(),
                relative_path: "rss.xml".into(),
                content_hash: "two".into(),
                updated_at: 1,
            },
        ];
        upsert_generated_file_hashes(db.conn(), &hashes).unwrap();
        hashes[0].content_hash = "changed".into();
        hashes[0].updated_at = 2;
        upsert_generated_file_hashes(db.conn(), &hashes[..1]).unwrap();

        let stored = list_generated_file_hashes(db.conn(), "p1").unwrap();
        assert_eq!(stored.len(), 2);
        assert_eq!(
            get_generated_file_hash(db.conn(), "p1", "index.html")
                .unwrap()
                .content_hash,
            "changed"
        );
    }
}
