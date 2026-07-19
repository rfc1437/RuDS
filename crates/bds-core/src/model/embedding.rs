use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Insertable,
    diesel::AsChangeset,
)]
#[diesel(
    table_name = crate::db::schema::embedding_keys,
    check_for_backend(diesel::sqlite::Sqlite),
    treat_none_as_default_value = false
)]
pub struct EmbeddingKey {
    pub label: i64,
    pub post_id: String,
    pub project_id: String,
    pub content_hash: String,
    pub vector: Vec<u8>,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Insertable,
    diesel::AsChangeset,
)]
#[diesel(
    table_name = crate::db::schema::dismissed_duplicate_pairs,
    check_for_backend(diesel::sqlite::Sqlite),
    treat_none_as_default_value = false
)]
pub struct DismissedDuplicatePair {
    pub id: String,
    pub project_id: String,
    pub post_id_a: String,
    pub post_id_b: String,
    pub dismissed_at: i64,
}
