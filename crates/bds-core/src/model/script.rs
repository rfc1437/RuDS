use diesel::ExpressionMethods;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, diesel::AsExpression, diesel::FromSqlRow,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
#[serde(rename_all = "lowercase")]
pub enum ScriptKind {
    Macro,
    Utility,
    Transform,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, diesel::AsExpression, diesel::FromSqlRow,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
#[serde(rename_all = "lowercase")]
pub enum ScriptStatus {
    Draft,
    Published,
}

impl ScriptKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Macro => "macro",
            Self::Utility => "utility",
            Self::Transform => "transform",
        }
    }
}

impl std::str::FromStr for ScriptKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "macro" => Ok(Self::Macro),
            "utility" => Ok(Self::Utility),
            "transform" => Ok(Self::Transform),
            _ => Err(format!("invalid ScriptKind: {value}")),
        }
    }
}

impl ScriptStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
        }
    }
}

impl std::str::FromStr for ScriptStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "published" => Ok(Self::Published),
            _ => Err(format!("invalid ScriptStatus: {value}")),
        }
    }
}

/// A user-authored script. Matches the `scripts` table.
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
#[diesel(table_name = crate::db::schema::scripts, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct Script {
    pub id: String,
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub kind: ScriptKind,
    pub entrypoint: String,
    #[diesel(
        deserialize_as = crate::db::types::DbBool,
        serialize_as = crate::db::types::DbBool
    )]
    pub enabled: bool,
    pub version: i32,
    pub file_path: String,
    pub status: ScriptStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
