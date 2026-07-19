use std::io::{BufRead as _, Write as _};
use std::process::ExitCode;

use bds_core::engine::mcp::{McpContext, handle_rpc};
use serde_json::{Value, json};

fn main() -> ExitCode {
    if let Err(error) = run_stdio() {
        eprintln!("MCP server error: {error}");
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn run_stdio() -> Result<(), Box<dyn std::error::Error>> {
    let database_path = bds_core::util::application_database_path();
    if let Some(parent) = database_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let context = McpContext::new(database_path);
    context.prepare()?;
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<Value>(&line) {
            Ok(request) => handle_rpc(&context, &request),
            Err(_) => Some(json!({
                "jsonrpc":"2.0",
                "id":null,
                "error":{"code":-32700,"message":"Parse error"}
            })),
        };
        if let Some(response) = response {
            serde_json::to_writer(&mut stdout, &response)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
        }
    }
    Ok(())
}
