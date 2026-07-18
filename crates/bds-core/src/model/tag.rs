use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Serialize,
    Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Insertable,
    diesel::AsChangeset,
)]
#[diesel(table_name = crate::db::schema::tags, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct Tag {
    pub id: String,
    pub project_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_template_slug: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
