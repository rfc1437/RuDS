use serde_json::{Value, json};

use crate::engine::{EngineError, EngineResult};

use super::McpContext;

pub const MCP_PROTOCOL_VERSION: &str = "2025-06-18";

pub fn handle_rpc(context: &McpContext, request: &Value) -> Option<Value> {
    let id = request.get("id").cloned();
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return Some(error(id.unwrap_or(Value::Null), -32600, "Invalid Request"));
    };
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(error(id.unwrap_or(Value::Null), -32600, "Invalid Request"));
    }
    let id = id?;
    let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": negotiated_version(&params),
            "capabilities": {
                "tools": {"listChanged": false},
                "resources": {"subscribe": false, "listChanged": false}
            },
            "serverInfo": {
                "name": "Blogging Desktop Server",
                "version": env!("CARGO_PKG_VERSION")
            }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": context.list_tools()})),
        "tools/call" => call_tool(context, &params),
        "resources/list" => Ok(json!({"resources": context.list_resources()})),
        "resources/templates/list" => Ok(json!({
            "resourceTemplates": context.list_resource_templates()
        })),
        "resources/read" => read_resource(context, &params),
        _ => return Some(error(id, -32601, "Method not found")),
    };
    Some(match result {
        Ok(result) => success(id, result),
        Err(EngineError::NotFound(message)) => error(id, -32004, &message),
        Err(EngineError::Validation(message) | EngineError::Conflict(message)) => {
            error(id, -32602, &message)
        }
        Err(error_value) => error(id, -32000, &error_value.to_string()),
    })
}

fn negotiated_version(params: &Value) -> &str {
    match params.get("protocolVersion").and_then(Value::as_str) {
        Some("2025-03-26") => "2025-03-26",
        Some("2025-06-18") => "2025-06-18",
        _ => MCP_PROTOCOL_VERSION,
    }
}

fn call_tool(context: &McpContext, params: &Value) -> EngineResult<Value> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .filter(|name| !name.is_empty())
        .ok_or_else(|| EngineError::Validation("tool name is required".into()))?;
    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let result = context.call_tool(name, arguments)?;
    Ok(json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string(&result)?
        }],
        "structuredContent": result,
        "isError": false
    }))
}

fn read_resource(context: &McpContext, params: &Value) -> EngineResult<Value> {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .filter(|uri| !uri.is_empty())
        .ok_or_else(|| EngineError::Validation("resource URI is required".into()))?;
    let content = context.read_resource(uri)?;
    let content = if let Some(blob) = content.blob {
        json!({
            "uri": content.uri,
            "mimeType": content.mime_type,
            "blob": blob
        })
    } else {
        json!({
            "uri": content.uri,
            "mimeType": content.mime_type,
            "text": content.text.unwrap_or_default()
        })
    };
    Ok(json!({"contents": [content]}))
}

fn success(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}

pub(crate) fn error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifications_have_no_response_and_invalid_requests_are_rejected() {
        let context = McpContext::new("missing.sqlite".into());
        assert!(
            handle_rpc(
                &context,
                &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
            )
            .is_none()
        );
        assert_eq!(
            handle_rpc(&context, &json!({"jsonrpc":"2.0","id":1})).unwrap()["error"]["code"],
            -32600
        );
    }
}
