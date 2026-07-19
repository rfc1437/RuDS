use bds_core::model::DomainEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SUBSYSTEM: &str = "ruds";
pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    #[serde(flatten)]
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    Hello {
        protocol_version: u16,
    },
    ListProjects,
    OpenProject {
        project_id: String,
    },
    Call {
        namespace: String,
        method: String,
        #[serde(default)]
        arguments: Vec<Value>,
    },
    Ping,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Response {
        id: String,
        result: Value,
    },
    Error {
        id: String,
        code: String,
        message: String,
    },
    Event {
        sequence: u64,
        event: DomainEvent,
    },
    Tasks {
        sequence: u64,
        tasks: Vec<RemoteTask>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteTask {
    pub id: u64,
    pub label: String,
    pub status: String,
    pub progress: Option<f32>,
    pub message: Option<String>,
}
