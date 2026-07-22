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
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    DraftPost,
    ProposeScript,
    ProposeTemplate,
    ProposeMediaTranslation,
    ProposeMediaMetadata,
    ProposePostMetadata,
}

impl ProposalKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DraftPost => "draft_post",
            Self::ProposeScript => "propose_script",
            Self::ProposeTemplate => "propose_template",
            Self::ProposeMediaTranslation => "propose_media_translation",
            Self::ProposeMediaMetadata => "propose_media_metadata",
            Self::ProposePostMetadata => "propose_post_metadata",
        }
    }
}

impl std::str::FromStr for ProposalKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "draft_post" => Ok(Self::DraftPost),
            "propose_script" => Ok(Self::ProposeScript),
            "propose_template" => Ok(Self::ProposeTemplate),
            "propose_media_translation" => Ok(Self::ProposeMediaTranslation),
            "propose_media_metadata" => Ok(Self::ProposeMediaMetadata),
            "propose_post_metadata" => Ok(Self::ProposePostMetadata),
            _ => Err(format!("invalid proposal kind: {value}")),
        }
    }
}

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
pub enum ProposalStatus {
    Pending,
    Executing,
    Accepted,
    Rejected,
    Expired,
}

impl ProposalStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Executing => "executing",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Expired => "expired",
        }
    }
}

impl std::str::FromStr for ProposalStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "pending" => Ok(Self::Pending),
            "executing" => Ok(Self::Executing),
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            "expired" => Ok(Self::Expired),
            _ => Err(format!("invalid proposal status: {value}")),
        }
    }
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
)]
#[diesel(
    table_name = crate::db::schema::mcp_proposals,
    check_for_backend(diesel::sqlite::Sqlite)
)]
pub struct McpProposal {
    pub id: String,
    pub project_id: String,
    pub kind: ProposalKind,
    pub status: ProposalStatus,
    pub entity_id: String,
    pub data: String,
    pub result: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
    pub resolved_at: Option<i64>,
}
