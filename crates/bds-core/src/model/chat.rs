use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    diesel::AsExpression,
    diesel::FromSqlRow,
)]
#[diesel(sql_type = diesel::sql_types::Text)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

impl ChatRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::Tool => "tool",
        }
    }
}

impl std::str::FromStr for ChatRole {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "system" => Ok(Self::System),
            "user" => Ok(Self::User),
            "assistant" => Ok(Self::Assistant),
            "tool" => Ok(Self::Tool),
            _ => Err(format!("invalid chat role: {value}")),
        }
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, diesel::Queryable, diesel::Selectable,
)]
#[diesel(
    table_name = crate::db::schema::chat_conversations,
    check_for_backend(diesel::sqlite::Sqlite)
)]
pub struct ChatConversation {
    pub id: String,
    pub title: String,
    pub model: Option<String>,
    pub copilot_session_id: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, diesel::Insertable)]
#[diesel(table_name = crate::db::schema::chat_conversations)]
pub struct NewChatConversation<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub model: Option<&'a str>,
    pub copilot_session_id: Option<&'a str>,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, diesel::Queryable, diesel::Selectable,
)]
#[diesel(
    table_name = crate::db::schema::chat_messages,
    check_for_backend(diesel::sqlite::Sqlite)
)]
pub struct ChatMessage {
    pub id: i32,
    pub conversation_id: String,
    pub role: ChatRole,
    pub content: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_calls: Option<String>,
    pub created_at: i64,
    pub cache_read_tokens: Option<i32>,
    pub cache_write_tokens: Option<i32>,
    pub token_usage_input: Option<i32>,
    pub token_usage_output: Option<i32>,
}

#[derive(Debug, diesel::Insertable)]
#[diesel(table_name = crate::db::schema::chat_messages)]
pub struct NewChatMessage<'a> {
    pub conversation_id: &'a str,
    pub role: ChatRole,
    pub content: Option<&'a str>,
    pub tool_call_id: Option<&'a str>,
    pub tool_calls: Option<&'a str>,
    pub created_at: i64,
    pub cache_read_tokens: Option<i32>,
    pub cache_write_tokens: Option<i32>,
    pub token_usage_input: Option<i32>,
    pub token_usage_output: Option<i32>,
}
