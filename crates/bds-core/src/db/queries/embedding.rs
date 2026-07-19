use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::schema::{dismissed_duplicate_pairs, embedding_keys};
use crate::model::{DismissedDuplicatePair, EmbeddingKey};

pub fn get_key_for_post(
    conn: &DbConnection,
    project_id: &str,
    post_id: &str,
) -> QueryResult<Option<EmbeddingKey>> {
    conn.with(|c| {
        embedding_keys::table
            .filter(embedding_keys::project_id.eq(project_id))
            .filter(embedding_keys::post_id.eq(post_id))
            .select(EmbeddingKey::as_select())
            .first(c)
            .optional()
    })
}

pub fn list_keys(conn: &DbConnection, project_id: &str) -> QueryResult<Vec<EmbeddingKey>> {
    conn.with(|c| {
        embedding_keys::table
            .filter(embedding_keys::project_id.eq(project_id))
            .order(embedding_keys::label.asc())
            .select(EmbeddingKey::as_select())
            .load(c)
    })
}

pub fn max_label(conn: &DbConnection) -> QueryResult<i64> {
    conn.with(|c| {
        embedding_keys::table
            .select(embedding_keys::label)
            .order(embedding_keys::label.desc())
            .first(c)
            .optional()
            .map(|label| label.unwrap_or(0))
    })
}

pub fn upsert_key(conn: &DbConnection, key: &EmbeddingKey) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(embedding_keys::table)
            .values(key)
            .on_conflict((embedding_keys::project_id, embedding_keys::post_id))
            .do_update()
            .set((
                embedding_keys::content_hash.eq(&key.content_hash),
                embedding_keys::vector.eq(&key.vector),
            ))
            .execute(c)
            .map(|_| ())
    })
}

pub fn delete_key_for_post(
    conn: &DbConnection,
    project_id: &str,
    post_id: &str,
) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(
            embedding_keys::table
                .filter(embedding_keys::project_id.eq(project_id))
                .filter(embedding_keys::post_id.eq(post_id)),
        )
        .execute(c)
        .map(|_| ())
    })
}

pub fn delete_stale_keys(
    conn: &DbConnection,
    project_id: &str,
    live_post_ids: &[String],
) -> QueryResult<usize> {
    conn.with(|c| {
        let query = embedding_keys::table.filter(embedding_keys::project_id.eq(project_id));
        if live_post_ids.is_empty() {
            diesel::delete(query).execute(c)
        } else {
            diesel::delete(query.filter(embedding_keys::post_id.ne_all(live_post_ids))).execute(c)
        }
    })
}

pub fn insert_dismissed_pair(
    conn: &DbConnection,
    pair: &DismissedDuplicatePair,
) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(dismissed_duplicate_pairs::table)
            .values(pair)
            .on_conflict((
                dismissed_duplicate_pairs::project_id,
                dismissed_duplicate_pairs::post_id_a,
                dismissed_duplicate_pairs::post_id_b,
            ))
            .do_nothing()
            .execute(c)
            .map(|_| ())
    })
}

pub fn insert_dismissed_pairs(
    conn: &DbConnection,
    pairs: &[DismissedDuplicatePair],
) -> QueryResult<usize> {
    if pairs.is_empty() {
        return Ok(0);
    }
    conn.with(|c| {
        diesel::insert_into(dismissed_duplicate_pairs::table)
            .values(pairs)
            .on_conflict((
                dismissed_duplicate_pairs::project_id,
                dismissed_duplicate_pairs::post_id_a,
                dismissed_duplicate_pairs::post_id_b,
            ))
            .do_nothing()
            .execute(c)
    })
}

pub fn list_dismissed_pairs(
    conn: &DbConnection,
    project_id: &str,
) -> QueryResult<Vec<DismissedDuplicatePair>> {
    conn.with(|c| {
        dismissed_duplicate_pairs::table
            .filter(dismissed_duplicate_pairs::project_id.eq(project_id))
            .select(DismissedDuplicatePair::as_select())
            .load(c)
    })
}

pub fn delete_orphan_dismissals(
    conn: &DbConnection,
    project_id: &str,
    live_post_ids: &[String],
) -> QueryResult<usize> {
    conn.with(|c| {
        let query = dismissed_duplicate_pairs::table
            .filter(dismissed_duplicate_pairs::project_id.eq(project_id));
        if live_post_ids.is_empty() {
            diesel::delete(query).execute(c)
        } else {
            diesel::delete(
                query.filter(
                    dismissed_duplicate_pairs::post_id_a
                        .ne_all(live_post_ids)
                        .or(dismissed_duplicate_pairs::post_id_b.ne_all(live_post_ids)),
                ),
            )
            .execute(c)
        }
    })
}

pub fn delete_dismissals_for_post(
    conn: &DbConnection,
    project_id: &str,
    post_id: &str,
) -> QueryResult<usize> {
    conn.with(|c| {
        diesel::delete(
            dismissed_duplicate_pairs::table
                .filter(dismissed_duplicate_pairs::project_id.eq(project_id))
                .filter(
                    dismissed_duplicate_pairs::post_id_a
                        .eq(post_id)
                        .or(dismissed_duplicate_pairs::post_id_b.eq(post_id)),
                ),
        )
        .execute(c)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::model::{Post, PostStatus, Project};

    fn seeded() -> (Database, String, String) {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        let project_id = "embedding-project".to_string();
        crate::db::queries::project::insert_project(
            db.conn(),
            &Project {
                id: project_id.clone(),
                name: "Embedding".into(),
                slug: "embedding".into(),
                description: None,
                data_path: Some("/tmp/embedding".into()),
                is_active: true,
                created_at: 1,
                updated_at: 1,
            },
        )
        .unwrap();
        let post_id = "embedding-post".to_string();
        crate::db::queries::post::insert_post(
            db.conn(),
            &Post {
                id: post_id.clone(),
                project_id: project_id.clone(),
                title: "Post".into(),
                slug: "post".into(),
                excerpt: None,
                content: Some("Body".into()),
                status: PostStatus::Draft,
                author: None,
                language: Some("en".into()),
                do_not_translate: false,
                template_slug: None,
                file_path: "posts/post.md".into(),
                checksum: None,
                tags: vec![],
                categories: vec![],
                published_title: None,
                published_content: None,
                published_tags: None,
                published_categories: None,
                published_excerpt: None,
                created_at: 1,
                updated_at: 1,
                published_at: None,
            },
        )
        .unwrap();
        (db, project_id, post_id)
    }

    #[test]
    fn embedding_vector_round_trips_as_blob_and_dismissals_are_canonical() {
        let (db, project_id, post_id) = seeded();
        let key = EmbeddingKey {
            label: 1,
            post_id: post_id.clone(),
            project_id: project_id.clone(),
            content_hash: "hash".into(),
            vector: vec![0, 1, 255],
        };
        upsert_key(db.conn(), &key).unwrap();
        assert_eq!(
            get_key_for_post(db.conn(), &project_id, &post_id).unwrap(),
            Some(key)
        );

        let replacement = EmbeddingKey {
            label: 2,
            post_id: post_id.clone(),
            project_id: project_id.clone(),
            content_hash: "new-hash".into(),
            vector: vec![3, 2, 1],
        };
        upsert_key(db.conn(), &replacement).unwrap();
        let updated = get_key_for_post(db.conn(), &project_id, &post_id)
            .unwrap()
            .unwrap();
        assert_eq!(updated.label, 1, "a post keeps its stable HNSW label");
        assert_eq!(updated.content_hash, "new-hash");
        assert_eq!(updated.vector, vec![3, 2, 1]);

        let pair = DismissedDuplicatePair {
            id: "dismissal".into(),
            project_id: project_id.clone(),
            post_id_a: "a".into(),
            post_id_b: "b".into(),
            dismissed_at: 1,
        };
        insert_dismissed_pair(db.conn(), &pair).unwrap();
        insert_dismissed_pair(db.conn(), &pair).unwrap();
        assert_eq!(
            list_dismissed_pairs(db.conn(), &project_id).unwrap(),
            vec![pair]
        );
    }
}
