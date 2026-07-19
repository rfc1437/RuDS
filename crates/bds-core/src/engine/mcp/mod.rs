mod agent_config;
mod http;
mod protocol;
mod resources;
mod tools;

use std::path::{Path, PathBuf};

use serde_json::Value;
use uuid::Uuid;

use crate::db::queries::{mcp_proposal as proposal_q, project as project_q};
use crate::db::{Database, DbConnection};
use crate::engine::{EngineError, EngineResult, cli_sync, domain_events};
use crate::model::DomainEvent;
use crate::util::now_unix_ms;

pub use crate::model::{McpProposal, ProposalKind, ProposalStatus};
pub use agent_config::{
    McpAgent, agent_config_path, install_agent_config, is_agent_configured,
    packaged_mcp_executable, remove_agent_config,
};
pub use http::McpHttpServer;
pub use protocol::{MCP_PROTOCOL_VERSION, handle_rpc};
pub use resources::ResourceContent;

pub const DEFAULT_HTTP_PORT: u16 = 4124;
pub const PROPOSAL_TTL_MS: i64 = 30 * 60 * 1_000;
pub const PROPOSALS_EVENT_KEY: &str = "mcp.proposals";

#[derive(Debug, Clone)]
pub struct McpContext {
    database_path: PathBuf,
}

impl McpContext {
    pub fn new(database_path: PathBuf) -> Self {
        Self { database_path }
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    /// Prepare shared storage once when a transport starts. Individual
    /// stateless read requests never run migrations or repair derived state.
    pub fn prepare(&self) -> EngineResult<()> {
        let db = Database::open(&self.database_path)?;
        db.migrate()
            .map_err(|error| EngineError::Parse(error.to_string()))?;
        crate::engine::search::prepare_search_index(db.conn())?;
        Ok(())
    }

    pub fn list_resources(&self) -> Vec<Value> {
        resources::list()
    }

    pub fn list_resource_templates(&self) -> Vec<Value> {
        resources::templates()
    }

    pub fn read_resource(&self, uri: &str) -> EngineResult<resources::ResourceContent> {
        let db = self.open_database()?;
        resources::read(db.conn(), uri)
    }

    pub fn list_tools(&self) -> Vec<Value> {
        tools::list()
    }

    pub fn call_tool(&self, name: &str, params: Value) -> EngineResult<Value> {
        let db = self.open_database()?;
        tools::call(db.conn(), name, params)
    }

    pub(crate) fn open_database(&self) -> EngineResult<Database> {
        Ok(Database::open(&self.database_path)?)
    }
}

pub fn get_proposal(conn: &DbConnection, proposal_id: &str) -> EngineResult<McpProposal> {
    proposal_q::get_proposal(conn, proposal_id)
        .map_err(|_| EngineError::NotFound(format!("MCP proposal {proposal_id}")))
}

pub fn list_proposals(conn: &DbConnection, project_id: &str) -> EngineResult<Vec<McpProposal>> {
    expire_proposals(conn)?;
    Ok(proposal_q::list_proposals(conn, project_id)?)
}

pub fn list_pending_proposals(
    conn: &DbConnection,
    project_id: &str,
) -> EngineResult<Vec<McpProposal>> {
    expire_proposals(conn)?;
    Ok(proposal_q::list_pending_proposals(conn, project_id)?)
}

pub fn expire_proposals(conn: &DbConnection) -> EngineResult<usize> {
    let expired = proposal_q::expire_pending(conn, now_unix_ms())?;
    if expired > 0 {
        notify_proposals_changed();
    }
    Ok(expired)
}

pub(crate) fn create_proposal(
    conn: &DbConnection,
    kind: ProposalKind,
    project_id: &str,
    entity_id: Option<&str>,
    data: &Value,
) -> EngineResult<McpProposal> {
    expire_proposals(conn)?;
    let now = now_unix_ms();
    let proposal = McpProposal {
        id: Uuid::new_v4().to_string(),
        project_id: project_id.to_string(),
        kind,
        status: ProposalStatus::Pending,
        entity_id: entity_id.map(str::to_string),
        data: serde_json::to_string(data)?,
        result: None,
        created_at: now,
        expires_at: now + PROPOSAL_TTL_MS,
        resolved_at: None,
    };
    proposal_q::insert_proposal(conn, &proposal)?;
    cli_sync::record_cli_event(
        conn,
        &DomainEvent::SettingsChanged {
            project_id: None,
            key: PROPOSALS_EVENT_KEY.to_string(),
        },
    )?;
    Ok(proposal)
}

pub fn accept_proposal(
    conn: &DbConnection,
    data_dir: &Path,
    proposal_id: &str,
) -> EngineResult<McpProposal> {
    resolve_proposal(conn, data_dir, proposal_id, true)
}

pub fn reject_proposal(
    conn: &DbConnection,
    data_dir: &Path,
    proposal_id: &str,
) -> EngineResult<McpProposal> {
    resolve_proposal(conn, data_dir, proposal_id, false)
}

fn resolve_proposal(
    conn: &DbConnection,
    data_dir: &Path,
    proposal_id: &str,
    accept: bool,
) -> EngineResult<McpProposal> {
    expire_proposals(conn)?;
    conn.begin_savepoint()?;
    let outcome = (|| {
        let now = now_unix_ms();
        if !proposal_q::claim_pending(conn, proposal_id, now)? {
            let current = get_proposal(conn, proposal_id)?;
            return Err(EngineError::Conflict(format!(
                "MCP proposal {} is {}",
                current.id,
                current.status.as_str()
            )));
        }
        let proposal = get_proposal(conn, proposal_id)?;
        let result = if accept {
            execute_proposal(conn, data_dir, &proposal)?
        } else {
            serde_json::json!({"message": "rejected"})
        };
        let status = if accept {
            ProposalStatus::Accepted
        } else {
            ProposalStatus::Rejected
        };
        if !proposal_q::resolve_claimed(
            conn,
            proposal_id,
            status,
            &serde_json::to_string(&result)?,
            now_unix_ms(),
        )? {
            return Err(EngineError::Conflict(format!(
                "MCP proposal {proposal_id} was resolved concurrently"
            )));
        }
        get_proposal(conn, proposal_id)
    })();
    match outcome {
        Ok(proposal) => {
            conn.release_savepoint()?;
            notify_proposals_changed();
            Ok(proposal)
        }
        Err(error) => {
            let _ = conn.rollback_savepoint();
            Err(error)
        }
    }
}

fn execute_proposal(
    conn: &DbConnection,
    data_dir: &Path,
    proposal: &McpProposal,
) -> EngineResult<Value> {
    let data: Value = serde_json::from_str(&proposal.data)?;
    match proposal.kind {
        ProposalKind::DraftPost => {
            let post = crate::engine::post::create_post(
                conn,
                data_dir,
                &proposal.project_id,
                required_string(&data, "title")?,
                Some(required_string(&data, "content")?),
                string_array(&data, "tags"),
                string_array(&data, "categories"),
                optional_string(&data, "author"),
                optional_string(&data, "language"),
                None,
            )?;
            let post = if let Some(excerpt) = optional_string(&data, "excerpt") {
                crate::engine::post::update_post(
                    conn,
                    data_dir,
                    &post.id,
                    None,
                    None,
                    Some(Some(excerpt)),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                )?
            } else {
                post
            };
            let post = crate::engine::post::publish_post(conn, data_dir, &post.id)?;
            Ok(serde_json::to_value(post)?)
        }
        ProposalKind::ProposeScript => {
            let kind = required_string(&data, "kind")?
                .parse()
                .map_err(EngineError::Validation)?;
            let script = crate::engine::script::create_script(
                conn,
                &proposal.project_id,
                required_string(&data, "title")?,
                kind,
                required_string(&data, "content")?,
                optional_string(&data, "entrypoint"),
            )?;
            let script = crate::engine::script::publish_script(conn, data_dir, &script.id)?;
            Ok(serde_json::to_value(script)?)
        }
        ProposalKind::ProposeTemplate => {
            let kind = required_string(&data, "kind")?
                .parse()
                .map_err(EngineError::Validation)?;
            let template = crate::engine::template::create_template(
                conn,
                &proposal.project_id,
                required_string(&data, "title")?,
                kind,
                required_string(&data, "content")?,
            )?;
            let template = crate::engine::template::publish_template(conn, data_dir, &template.id)?;
            Ok(serde_json::to_value(template)?)
        }
        ProposalKind::ProposeMediaTranslation => {
            let translation = crate::engine::media::upsert_media_translation(
                conn,
                data_dir,
                required_string(&data, "mediaId")?,
                required_string(&data, "language")?,
                optional_string(&data, "title"),
                optional_string(&data, "alt"),
                optional_string(&data, "caption"),
            )?;
            Ok(serde_json::to_value(translation)?)
        }
        ProposalKind::ProposeMediaMetadata => {
            let media = crate::engine::media::update_media(
                conn,
                data_dir,
                required_string(&data, "mediaId")?,
                optional_optional_string(&data, "title"),
                optional_optional_string(&data, "alt"),
                optional_optional_string(&data, "caption"),
                None,
                None,
                data.get("tags")
                    .is_some()
                    .then(|| string_array(&data, "tags")),
            )?;
            Ok(serde_json::to_value(media)?)
        }
        ProposalKind::ProposePostMetadata => {
            let post = crate::engine::post::update_post(
                conn,
                data_dir,
                required_string(&data, "postId")?,
                optional_string(&data, "title"),
                None,
                optional_optional_string(&data, "excerpt"),
                None,
                data.get("tags")
                    .is_some()
                    .then(|| string_array(&data, "tags")),
                data.get("categories")
                    .is_some()
                    .then(|| string_array(&data, "categories")),
                None,
                None,
                None,
                None,
            )?;
            Ok(serde_json::to_value(post)?)
        }
    }
}

pub(crate) fn active_project(
    conn: &DbConnection,
) -> EngineResult<(crate::model::Project, PathBuf)> {
    let project = project_q::get_active_project(conn)
        .map_err(|_| EngineError::NotFound("active project".into()))?;
    let data_dir = project
        .data_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| EngineError::Validation("active project has no data path".into()))?;
    Ok((project, data_dir))
}

pub(crate) fn required_string<'a>(value: &'a Value, key: &str) -> EngineResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| EngineError::Validation(format!("{key} is required")))
}

pub(crate) fn optional_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn optional_optional_string<'a>(value: &'a Value, key: &str) -> Option<Option<&'a str>> {
    value.get(key).map(|value| value.as_str())
}

pub(crate) fn string_array(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn notify_proposals_changed() {
    domain_events::settings_changed(None, PROPOSALS_EVENT_KEY);
}
