use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::engine::{EngineError, EngineResult};

const SERVER_NAME: &str = "bDS";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpAgent {
    ClaudeCode,
    GithubCopilot,
}

impl McpAgent {
    pub const fn all() -> [Self; 2] {
        [Self::ClaudeCode, Self::GithubCopilot]
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::GithubCopilot => "GitHub Copilot",
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::GithubCopilot => "github_copilot",
        }
    }
}

pub fn agent_config_path(agent: McpAgent, home_dir: &Path) -> PathBuf {
    match agent {
        McpAgent::ClaudeCode => home_dir.join(".claude.json"),
        McpAgent::GithubCopilot => {
            #[cfg(target_os = "macos")]
            let path = home_dir.join("Library/Application Support/Code/User/mcp.json");
            #[cfg(target_os = "windows")]
            let path = home_dir.join("AppData/Roaming/Code/User/mcp.json");
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let path = home_dir.join(".config/Code/User/mcp.json");
            path
        }
    }
}

pub fn packaged_mcp_executable() -> EngineResult<PathBuf> {
    let executable = std::env::current_exe()?;
    let sibling = executable.with_file_name(if cfg!(windows) {
        "bds-mcp.exe"
    } else {
        "bds-mcp"
    });
    if sibling.is_file() {
        Ok(sibling)
    } else {
        Err(EngineError::NotFound(format!(
            "packaged MCP executable {}",
            sibling.display()
        )))
    }
}

pub fn is_agent_configured(agent: McpAgent, home_dir: &Path) -> bool {
    read_config(&agent_config_path(agent, home_dir))
        .ok()
        .is_some_and(|config| {
            server_map(&config, agent).is_some_and(|servers| servers.contains_key(SERVER_NAME))
        })
}

pub fn install_agent_config(
    agent: McpAgent,
    home_dir: &Path,
    executable: &Path,
) -> EngineResult<PathBuf> {
    if !executable.is_file() {
        return Err(EngineError::NotFound(format!(
            "MCP executable {}",
            executable.display()
        )));
    }
    let path = agent_config_path(agent, home_dir);
    let mut config = read_config(&path)?;
    let key = server_key(agent);
    let servers = config
        .as_object_mut()
        .ok_or_else(|| {
            EngineError::Validation(format!("{} must contain a JSON object", path.display()))
        })?
        .entry(key)
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| {
            EngineError::Validation(format!("{key} in {} must be an object", path.display()))
        })?;
    let executable = executable.to_string_lossy();
    let server = match agent {
        McpAgent::ClaudeCode => json!({"command": executable, "args": []}),
        McpAgent::GithubCopilot => {
            json!({"type": "stdio", "command": executable, "args": []})
        }
    };
    servers.insert(SERVER_NAME.into(), server);
    write_config(&path, &config)?;
    Ok(path)
}

pub fn remove_agent_config(agent: McpAgent, home_dir: &Path) -> EngineResult<PathBuf> {
    let path = agent_config_path(agent, home_dir);
    let mut config = read_config(&path)?;
    if let Some(servers) = config
        .as_object_mut()
        .and_then(|object| object.get_mut(server_key(agent)))
        .and_then(Value::as_object_mut)
    {
        servers.remove(SERVER_NAME);
    }
    write_config(&path, &config)?;
    Ok(path)
}

fn server_key(agent: McpAgent) -> &'static str {
    match agent {
        McpAgent::ClaudeCode => "mcpServers",
        McpAgent::GithubCopilot => "servers",
    }
}

fn server_map(config: &Value, agent: McpAgent) -> Option<&Map<String, Value>> {
    config.get(server_key(agent)).and_then(Value::as_object)
}

fn read_config(path: &Path) -> EngineResult<Value> {
    match std::fs::read_to_string(path) {
        Ok(source) => serde_json::from_str(&source)
            .map_err(|error| EngineError::Parse(format!("{}: {error}", path.display()))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(error) => Err(error.into()),
    }
}

fn write_config(path: &Path, config: &Value) -> EngineResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let source = serde_json::to_string_pretty(config)?;
    crate::util::atomic_write_str(path, &source)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_and_remove_preserve_unrelated_config_without_secrets() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("bds-mcp");
        std::fs::write(&executable, "binary").unwrap();
        for agent in McpAgent::all() {
            let path = agent_config_path(agent, root.path());
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, r#"{"unrelated":{"token":"kept"}}"#).unwrap();
            install_agent_config(agent, root.path(), &executable).unwrap();
            assert!(is_agent_configured(agent, root.path()));
            let source = std::fs::read_to_string(&path).unwrap();
            assert!(source.contains("kept"));
            assert!(!source.contains("api_key"));
            remove_agent_config(agent, root.path()).unwrap();
            assert!(!is_agent_configured(agent, root.path()));
            assert!(std::fs::read_to_string(path).unwrap().contains("kept"));
        }
    }
}
