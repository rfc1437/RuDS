use diesel::ExpressionMethods;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, diesel::AsExpression, diesel::FromSqlRow,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
#[serde(rename_all = "snake_case")]
pub enum TemplateKind {
    Post,
    List,
    NotFound,
    Partial,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, diesel::AsExpression, diesel::FromSqlRow,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
#[serde(rename_all = "lowercase")]
pub enum TemplateStatus {
    Draft,
    Published,
}

impl TemplateKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Post => "post",
            Self::List => "list",
            Self::NotFound => "not_found",
            Self::Partial => "partial",
        }
    }
}

impl std::str::FromStr for TemplateKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "post" => Ok(Self::Post),
            "list" => Ok(Self::List),
            "not_found" | "notFound" | "not-found" => Ok(Self::NotFound),
            "partial" => Ok(Self::Partial),
            _ => Err(format!("invalid TemplateKind: {value}")),
        }
    }
}

impl TemplateStatus {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Published => "published",
        }
    }
}

impl std::str::FromStr for TemplateStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft" => Ok(Self::Draft),
            "published" => Ok(Self::Published),
            _ => Err(format!("invalid TemplateStatus: {value}")),
        }
    }
}

/// A Liquid template. Matches the `templates` table.
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
#[diesel(table_name = crate::db::schema::templates, check_for_backend(diesel::sqlite::Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct Template {
    pub id: String,
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub kind: TemplateKind,
    #[diesel(
        deserialize_as = crate::db::types::DbBool,
        serialize_as = crate::db::types::DbBool
    )]
    pub enabled: bool,
    pub version: i32,
    pub file_path: String,
    pub status: TemplateStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
