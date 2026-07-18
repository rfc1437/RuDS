use diesel::ExpressionMethods;
use serde::{Deserialize, Serialize};

/// A bDS project. Matches the `projects` table schema.
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
#[diesel(table_name = crate::db::schema::projects, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub slug: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_path: Option<String>,
    #[diesel(
        deserialize_as = crate::db::types::DbBool,
        serialize_as = crate::db::types::DbBool
    )]
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Key-value settings. Matches the `settings` table.
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
#[diesel(table_name = crate::db::schema::settings, check_for_backend(diesel::sqlite::Sqlite))]
pub struct Setting {
    pub key: String,
    pub value: String,
    pub updated_at: i64,
}
