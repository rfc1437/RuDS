use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::time::Duration;

use diesel::prelude::*;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::db::DbConnection as Connection;
use crate::db::queries::chat as queries;
use crate::db::schema::ai_models;
use crate::engine::ai::{self, AiEndpointConfig, TokenUsage};
use crate::engine::{EngineError, EngineResult, chat_tools};
use crate::model::{ChatConversation, ChatMessage, ChatRole, NewChatConversation, NewChatMessage};
use crate::util::now_unix_ms;

const DEFAULT_CONTEXT_TOKENS: usize = 32_768;
const DEFAULT_OUTPUT_TOKENS: u64 = 16_384;
const MAX_TOOL_ROUNDS: usize = 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatModelInfo {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub family: Option<String>,
    pub context_window: u64,
    pub max_output_tokens: u64,
    pub supports_tools: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEvent {
    Started {
        conversation_id: String,
    },
    Content {
        conversation_id: String,
        content: String,
    },
    ToolStarted {
        conversation_id: String,
        name: String,
        surface_id: String,
        arguments: Value,
    },
    ToolFinished {
        conversation_id: String,
        name: String,
    },
    Finished {
        conversation_id: String,
        usage: TokenUsage,
    },
    Failed {
        conversation_id: String,
        message: String,
    },
    Cancelled {
        conversation_id: String,
    },
    Navigate {
        destination: String,
        entity_id: Option<String>,
    },
}

#[derive(Clone)]
pub struct ChatSendOptions {
    pub endpoint: Option<AiEndpointConfig>,
    pub model: Option<String>,
    pub context_tokens: Option<usize>,
    pub max_output_tokens: u64,
    pub max_tool_rounds: usize,
    pub enable_tools: bool,
    pub model_supports_tools: Option<bool>,
    pub event_handler: Option<Arc<dyn Fn(ChatEvent) + Send + Sync>>,
}

impl Default for ChatSendOptions {
    fn default() -> Self {
        Self {
            endpoint: None,
            model: None,
            context_tokens: None,
            max_output_tokens: DEFAULT_OUTPUT_TOKENS,
            max_tool_rounds: MAX_TOOL_ROUNDS,
            enable_tools: true,
            model_supports_tools: None,
            event_handler: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChatTurnResult {
    pub content: String,
    pub usage: TokenUsage,
    pub cancelled: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AssembledResponse {
    pub content: String,
    pub tool_calls: Vec<ChatToolCall>,
    pub usage: TokenUsage,
    pub session_id: Option<String>,
}

#[derive(Default)]
struct PartialToolCall {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
pub struct SseAssembler {
    buffer: Vec<u8>,
    content: String,
    tools: BTreeMap<u64, PartialToolCall>,
    usage: TokenUsage,
    session_id: Option<String>,
    done: bool,
}

impl SseAssembler {
    pub fn feed(&mut self, bytes: &[u8]) -> EngineResult<()> {
        self.buffer.extend_from_slice(bytes);
        while let Some((end, separator_len)) = event_boundary(&self.buffer) {
            let event = self.buffer.drain(..end).collect::<Vec<_>>();
            self.buffer.drain(..separator_len);
            self.process_event(&event)?;
        }
        Ok(())
    }

    pub fn snapshot(&self) -> &str {
        &self.content
    }

    pub fn finish(mut self) -> EngineResult<AssembledResponse> {
        if !self.buffer.is_empty() {
            let trailing = std::mem::take(&mut self.buffer);
            self.process_event(&trailing)?;
        }
        let tool_calls = self
            .tools
            .into_values()
            .map(|tool| {
                let arguments = if tool.arguments.trim().is_empty() {
                    json!({})
                } else {
                    serde_json::from_str(&tool.arguments).map_err(|error| {
                        EngineError::Parse(format!(
                            "invalid arguments for tool {}: {error}",
                            tool.name
                        ))
                    })?
                };
                Ok(ChatToolCall {
                    id: tool.id,
                    name: tool.name,
                    arguments,
                })
            })
            .collect::<EngineResult<Vec<_>>>()?;
        Ok(AssembledResponse {
            content: self.content,
            tool_calls,
            usage: self.usage,
            session_id: self.session_id,
        })
    }

    fn process_event(&mut self, event: &[u8]) -> EngineResult<()> {
        let text = std::str::from_utf8(event)
            .map_err(|error| EngineError::Parse(format!("invalid SSE encoding: {error}")))?;
        let data = text
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            self.done |= data == "[DONE]";
            return Ok(());
        }
        let body: Value = serde_json::from_str(&data)
            .map_err(|error| EngineError::Parse(format!("malformed SSE event: {error}")))?;
        self.apply_json(&body)
    }

    fn apply_json(&mut self, body: &Value) -> EngineResult<()> {
        if let Some(error) = body.get("error") {
            return Err(EngineError::Parse(format!("provider error: {error}")));
        }
        if let Some(session_id) = body
            .get("session_id")
            .or_else(|| body.get("sessionId"))
            .and_then(Value::as_str)
        {
            self.session_id = Some(session_id.to_string());
        }
        merge_usage(&mut self.usage, body);
        for choice in body
            .get("choices")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let delta = choice
                .get("delta")
                .or_else(|| choice.get("message"))
                .unwrap_or(&Value::Null);
            if let Some(content) = delta.get("content").and_then(Value::as_str) {
                self.content.push_str(content);
            }
            for tool in delta
                .get("tool_calls")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let index = tool.get("index").and_then(Value::as_u64).unwrap_or(0);
                let partial = self.tools.entry(index).or_default();
                if let Some(id) = tool.get("id").and_then(Value::as_str) {
                    partial.id.push_str(id);
                }
                let function = tool.get("function").unwrap_or(&Value::Null);
                if let Some(name) = function.get("name").and_then(Value::as_str) {
                    partial.name.push_str(name);
                }
                if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                    partial.arguments.push_str(arguments);
                }
            }
        }
        Ok(())
    }
}

pub fn create_conversation(
    conn: &Connection,
    model: Option<&str>,
) -> EngineResult<ChatConversation> {
    let model = model.filter(|value| !value.trim().is_empty());
    let title = model
        .map(|model| format!("Chat with {model}"))
        .unwrap_or_else(|| "New Chat".to_string());
    create_conversation_titled(conn, model, &title)
}

pub fn create_conversation_titled(
    conn: &Connection,
    model: Option<&str>,
    title: &str,
) -> EngineResult<ChatConversation> {
    let model = model.filter(|value| !value.trim().is_empty());
    let title = title.trim();
    if title.is_empty() {
        return Err(EngineError::Validation(
            "conversation title is required".to_string(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    let now = now_unix_ms();
    Ok(queries::insert_conversation(
        conn,
        &NewChatConversation {
            id: &id,
            title,
            model,
            copilot_session_id: None,
            surface_state: None,
            created_at: now,
            updated_at: now,
        },
    )?)
}

pub fn rename_conversation(
    conn: &Connection,
    id: &str,
    title: &str,
) -> EngineResult<ChatConversation> {
    let title = title.trim();
    if title.is_empty() {
        return Err(EngineError::Validation(
            "conversation title is required".to_string(),
        ));
    }
    Ok(queries::rename_conversation(
        conn,
        id,
        title,
        now_unix_ms(),
    )?)
}

pub fn set_conversation_model(conn: &Connection, id: &str, model: &str) -> EngineResult<()> {
    let model = model.trim();
    if model.is_empty() {
        return Err(EngineError::Validation(
            "chat model is required".to_string(),
        ));
    }
    if queries::set_conversation_model(conn, id, model, now_unix_ms())? == 0 {
        return Err(EngineError::NotFound(format!("conversation {id}")));
    }
    Ok(())
}

pub fn list_conversations(conn: &Connection) -> EngineResult<Vec<ChatConversation>> {
    Ok(queries::list_conversations(conn)?)
}

pub fn list_models(conn: &Connection) -> EngineResult<Vec<ChatModelInfo>> {
    let rows = conn.with(|connection| {
        ai_models::table
            .select((
                ai_models::provider,
                ai_models::model_id,
                ai_models::name,
                ai_models::family,
                ai_models::context_window,
                ai_models::max_output_tokens,
                ai_models::tool_call,
            ))
            .order((ai_models::provider.asc(), ai_models::name.asc()))
            .load::<(String, String, String, Option<String>, i32, i32, i32)>(connection)
    })?;
    Ok(rows
        .into_iter()
        .map(
            |(provider, id, name, family, context_window, max_output_tokens, tool_call)| {
                ChatModelInfo {
                    provider,
                    id,
                    name,
                    family,
                    context_window: context_window.max(0) as u64,
                    max_output_tokens: max_output_tokens.max(0) as u64,
                    supports_tools: tool_call != 0,
                }
            },
        )
        .collect())
}

pub fn get_conversation(conn: &Connection, id: &str) -> EngineResult<ChatConversation> {
    queries::get_conversation(conn, id).map_err(|error| match error {
        diesel::result::Error::NotFound => EngineError::NotFound(format!("conversation {id}")),
        error => error.into(),
    })
}

pub fn delete_conversation(conn: &Connection, id: &str) -> EngineResult<()> {
    cancel_chat(id);
    if queries::delete_conversation(conn, id)? == 0 {
        return Err(EngineError::NotFound(format!("conversation {id}")));
    }
    Ok(())
}

pub fn list_messages(conn: &Connection, id: &str) -> EngineResult<Vec<ChatMessage>> {
    Ok(queries::list_messages(conn, id)?)
}

pub fn get_surface_state(
    conn: &Connection,
    conversation_id: &str,
) -> EngineResult<crate::engine::chat_surfaces::ChatSurfaceState> {
    let conversation = match queries::get_conversation(conn, conversation_id) {
        Ok(conversation) => conversation,
        Err(diesel::result::Error::NotFound) => return Ok(Default::default()),
        Err(error) => return Err(error.into()),
    };
    conversation.surface_state.as_deref().map_or_else(
        || Ok(Default::default()),
        |state| {
            serde_json::from_str(state)
                .map_err(|error| EngineError::Parse(format!("invalid chat surface state: {error}")))
        },
    )
}

pub fn put_surface_state(
    conn: &Connection,
    conversation_id: &str,
    state: &crate::engine::chat_surfaces::ChatSurfaceState,
) -> EngineResult<()> {
    let state = serde_json::to_string(state)?;
    if queries::set_surface_state(conn, conversation_id, &state, now_unix_ms())? == 0 {
        return Err(EngineError::NotFound(format!(
            "chat conversation {conversation_id}"
        )));
    }
    Ok(())
}

pub fn insert_message(
    conn: &Connection,
    conversation_id: &str,
    role: ChatRole,
    content: Option<&str>,
    tool_call_id: Option<&str>,
    tool_calls: Option<&str>,
    usage: TokenUsage,
) -> EngineResult<ChatMessage> {
    let now = now_unix_ms();
    Ok(queries::insert_message(
        conn,
        &NewChatMessage {
            conversation_id,
            role,
            content,
            tool_call_id,
            tool_calls,
            created_at: now,
            cache_read_tokens: token_i32(usage.cache_read_tokens),
            cache_write_tokens: token_i32(usage.cache_write_tokens),
            token_usage_input: token_i32(usage.input_tokens),
            token_usage_output: token_i32(usage.output_tokens),
        },
        now,
    )?)
}

pub fn subscribe_events() -> mpsc::Receiver<ChatEvent> {
    let (sender, receiver) = mpsc::channel();
    listeners()
        .lock()
        .expect("chat listeners lock")
        .push(sender);
    receiver
}

pub fn cancel_chat(conversation_id: &str) -> bool {
    let state = in_flight()
        .lock()
        .expect("chat cancellation lock")
        .get(conversation_id)
        .cloned();
    if let Some(state) = state {
        state.store(true, Ordering::SeqCst);
        true
    } else {
        false
    }
}

#[allow(clippy::too_many_arguments)]
pub fn send_chat_message(
    conn: &Connection,
    data_dir: &std::path::Path,
    project_id: &str,
    offline_mode: bool,
    conversation_id: &str,
    content: &str,
    mut options: ChatSendOptions,
) -> EngineResult<ChatTurnResult> {
    let content = content.trim();
    if content.is_empty() {
        return Err(EngineError::Validation(
            "chat message is required".to_string(),
        ));
    }
    let conversation = get_conversation(conn, conversation_id)?;
    let endpoint = match options.endpoint.clone() {
        Some(endpoint) => validate_runtime_endpoint(endpoint)?,
        None => ai::active_endpoint(conn, offline_mode)?,
    };
    if options.model_supports_tools.is_none() {
        options.model_supports_tools = ai::load_ai_settings(conn, offline_mode)?
            .active()
            .chat_supports_tools;
    }
    let model = options
        .model
        .clone()
        .or(conversation.model.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| endpoint.model.clone());
    if model.trim().is_empty() {
        return Err(EngineError::Validation(
            "AI unavailable - configure a chat model in Settings".to_string(),
        ));
    }

    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut active = in_flight().lock().expect("chat cancellation lock");
        if active.contains_key(conversation_id) {
            return Err(EngineError::Conflict(
                "a response is already streaming for this conversation".to_string(),
            ));
        }
        active.insert(conversation_id.to_string(), Arc::clone(&cancelled));
    }
    let _guard = InFlightGuard(conversation_id.to_string());
    emit(
        &options,
        ChatEvent::Started {
            conversation_id: conversation_id.to_string(),
        },
    );
    set_conversation_model(conn, conversation_id, &model)?;
    insert_message(
        conn,
        conversation_id,
        ChatRole::User,
        Some(content),
        None,
        None,
        TokenUsage::default(),
    )?;

    let result = run_turns(
        conn,
        data_dir,
        project_id,
        conversation_id,
        &endpoint,
        &model,
        &options,
        &cancelled,
    );
    match &result {
        Ok(turn) if turn.cancelled => emit(
            &options,
            ChatEvent::Cancelled {
                conversation_id: conversation_id.to_string(),
            },
        ),
        Ok(turn) => emit(
            &options,
            ChatEvent::Finished {
                conversation_id: conversation_id.to_string(),
                usage: turn.usage,
            },
        ),
        Err(error) => emit(
            &options,
            ChatEvent::Failed {
                conversation_id: conversation_id.to_string(),
                message: error.to_string(),
            },
        ),
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn run_turns(
    conn: &Connection,
    data_dir: &std::path::Path,
    project_id: &str,
    conversation_id: &str,
    endpoint: &AiEndpointConfig,
    model: &str,
    options: &ChatSendOptions,
    cancelled: &AtomicBool,
) -> EngineResult<ChatTurnResult> {
    let mut total_usage = TokenUsage::default();
    let max_rounds = options.max_tool_rounds.min(MAX_TOOL_ROUNDS);
    let supports_tools = options.enable_tools
        && match options.model_supports_tools {
            Some(supports_tools) => supports_tools,
            None => chat_tools::model_supports_tools(conn, model)?,
        };
    let catalog_model = list_models(conn)?
        .into_iter()
        .find(|candidate| candidate.id == model);
    let context_window = options.context_tokens.unwrap_or_else(|| {
        catalog_model
            .as_ref()
            .map(|model| model.context_window as usize)
            .filter(|window| *window > 0)
            .unwrap_or(DEFAULT_CONTEXT_TOKENS)
    });
    let max_output_tokens = catalog_model
        .as_ref()
        .map(|model| model.max_output_tokens)
        .filter(|limit| *limit > 0)
        .map_or(options.max_output_tokens, |limit| {
            limit.min(options.max_output_tokens)
        });
    let tool_specs = supports_tools.then(chat_tools::tool_specs);
    let tool_budget = tool_specs
        .as_ref()
        .map(|tools| approximate_tokens(&Value::Array(tools.clone())))
        .unwrap_or(0);
    let context_budget = context_window
        .saturating_sub(max_output_tokens as usize)
        .saturating_sub(tool_budget)
        .max(1_024.min(context_window));

    for round in 0..=max_rounds {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(ChatTurnResult {
                usage: total_usage,
                cancelled: true,
                ..Default::default()
            });
        }
        let messages = build_context(conn, project_id, conversation_id, context_budget)?;
        let mut payload = json!({
            "model": model,
            "messages": messages,
            "stream": true,
            "stream_options": {"include_usage": true},
            "max_tokens": max_output_tokens,
        });
        if let Some(tools) = tool_specs.as_ref() {
            payload["tools"] = Value::Array(tools.clone());
            payload["tool_choice"] = json!("auto");
        }
        let response = request_completion(endpoint, &payload, cancelled, |content| {
            emit(
                options,
                ChatEvent::Content {
                    conversation_id: conversation_id.to_string(),
                    content: content.to_string(),
                },
            );
        })?;
        add_usage(&mut total_usage, response.usage);
        if let Some(session_id) = response.session_id.as_deref() {
            queries::set_session_id(conn, conversation_id, Some(session_id), now_unix_ms())?;
        }

        if cancelled.load(Ordering::SeqCst) {
            if !response.content.is_empty() {
                insert_message(
                    conn,
                    conversation_id,
                    ChatRole::Assistant,
                    Some(&response.content),
                    None,
                    None,
                    response.usage,
                )?;
            }
            return Ok(ChatTurnResult {
                content: response.content,
                usage: total_usage,
                cancelled: true,
            });
        }

        if !response.tool_calls.is_empty() && round == max_rounds {
            if !response.content.is_empty() {
                insert_message(
                    conn,
                    conversation_id,
                    ChatRole::Assistant,
                    Some(&response.content),
                    None,
                    None,
                    response.usage,
                )?;
            }
            return Err(EngineError::Validation(format!(
                "chat exceeded the {max_rounds}-round tool limit"
            )));
        }
        let serialized_calls = (!response.tool_calls.is_empty())
            .then(|| serialize_tool_calls(&response.tool_calls))
            .transpose()?;
        let assistant_message = insert_message(
            conn,
            conversation_id,
            ChatRole::Assistant,
            (!response.content.is_empty()).then_some(response.content.as_str()),
            None,
            serialized_calls.as_deref(),
            response.usage,
        )?;
        if response.tool_calls.is_empty() {
            return Ok(ChatTurnResult {
                content: response.content,
                usage: total_usage,
                cancelled: false,
            });
        }
        for (index, call) in response.tool_calls.iter().enumerate() {
            if cancelled.load(Ordering::SeqCst) {
                persist_cancelled_tool_results(
                    conn,
                    conversation_id,
                    &response.tool_calls[index..],
                )?;
                return Ok(ChatTurnResult {
                    usage: total_usage,
                    cancelled: true,
                    ..Default::default()
                });
            }
            emit(
                options,
                ChatEvent::ToolStarted {
                    conversation_id: conversation_id.to_string(),
                    name: call.name.clone(),
                    surface_id: format!("{}-surface-{index}", assistant_message.id),
                    arguments: call.arguments.clone(),
                },
            );
            if cancelled.load(Ordering::SeqCst) {
                persist_cancelled_tool_results(
                    conn,
                    conversation_id,
                    &response.tool_calls[index..],
                )?;
                return Ok(ChatTurnResult {
                    usage: total_usage,
                    cancelled: true,
                    ..Default::default()
                });
            }
            let result =
                chat_tools::execute(conn, data_dir, project_id, &call.name, &call.arguments);
            let result = match result {
                Ok(result) => result,
                Err(error) => json!({"success": false, "error": error.to_string()}),
            };
            if call.name == "navigate"
                && let Some(navigation) = result.get("navigation")
                && let Some(destination) = navigation.get("destination").and_then(Value::as_str)
            {
                emit(
                    options,
                    ChatEvent::Navigate {
                        destination: destination.to_string(),
                        entity_id: navigation
                            .get("entity_id")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    },
                );
            }
            let result_text = serde_json::to_string(&result)?;
            insert_message(
                conn,
                conversation_id,
                ChatRole::Tool,
                Some(&result_text),
                Some(&call.id),
                None,
                TokenUsage::default(),
            )?;
            emit(
                options,
                ChatEvent::ToolFinished {
                    conversation_id: conversation_id.to_string(),
                    name: call.name.clone(),
                },
            );
        }
    }
    unreachable!("bounded loop returns on its final iteration")
}

fn persist_cancelled_tool_results(
    conn: &Connection,
    conversation_id: &str,
    calls: &[ChatToolCall],
) -> EngineResult<()> {
    let result = json!({"success": false, "cancelled": true}).to_string();
    for call in calls {
        insert_message(
            conn,
            conversation_id,
            ChatRole::Tool,
            Some(&result),
            Some(&call.id),
            None,
            TokenUsage::default(),
        )?;
    }
    Ok(())
}

pub fn build_context(
    conn: &Connection,
    project_id: &str,
    conversation_id: &str,
    token_budget: usize,
) -> EngineResult<Vec<Value>> {
    let system = chat_tools::system_prompt(conn, project_id)?;
    let messages = list_messages(conn, conversation_id)?;
    let mut groups: Vec<Vec<Value>> = Vec::new();
    let mut current = Vec::new();
    for message in messages {
        let value = message_json(&message)?;
        if message.role == ChatRole::System {
            continue;
        }
        if message.role == ChatRole::User && !current.is_empty() {
            groups.push(std::mem::take(&mut current));
        }
        current.push(value);
    }
    if !current.is_empty() {
        groups.push(current);
    }

    let system_value = json!({"role": "system", "content": system});
    let mut used = approximate_tokens(&system_value);
    let mut selected = Vec::new();
    for group in groups.into_iter().rev() {
        let cost = group.iter().map(approximate_tokens).sum::<usize>();
        if used + cost > token_budget && !selected.is_empty() {
            continue;
        }
        used += cost;
        selected.push(group);
        if used >= token_budget {
            break;
        }
    }
    selected.reverse();
    let mut result = vec![system_value];
    result.extend(selected.into_iter().flatten());
    Ok(result)
}

fn request_completion(
    endpoint: &AiEndpointConfig,
    payload: &Value,
    cancelled: &AtomicBool,
    mut on_content: impl FnMut(&str),
) -> EngineResult<AssembledResponse> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(300))
        .build()?;
    let mut request = client
        .post(chat_completions_url(&endpoint.url))
        .json(payload);
    if let Some(api_key) = endpoint.api_key.as_deref() {
        request = request.bearer_auth(api_key);
    }
    let mut response = request.send()?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        let detail = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/message")
                    .or_else(|| value.get("error"))
                    .or_else(|| value.get("message"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .unwrap_or_else(|| body.trim().to_string());
        let detail = (!detail.is_empty())
            .then_some(format!(": {detail}"))
            .unwrap_or_default();
        return Err(EngineError::Parse(format!(
            "AI provider returned {status}{detail}"
        )));
    }
    let is_stream = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.contains("text/event-stream"));
    if !is_stream {
        let body: Value = response.json()?;
        let mut assembler = SseAssembler::default();
        assembler.apply_json(&body)?;
        return assembler.finish();
    }
    let mut assembler = SseAssembler::default();
    let mut chunk = [0_u8; 4096];
    loop {
        if cancelled.load(Ordering::SeqCst) {
            break;
        }
        let count = response.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        let old_len = assembler.snapshot().len();
        assembler.feed(&chunk[..count])?;
        if assembler.snapshot().len() != old_len {
            on_content(assembler.snapshot());
        }
    }
    assembler.finish()
}

fn message_json(message: &ChatMessage) -> EngineResult<Value> {
    let mut value = json!({"role": message.role.as_str()});
    if let Some(content) = message.content.as_deref() {
        value["content"] = json!(content);
    }
    if let Some(tool_call_id) = message.tool_call_id.as_deref() {
        value["tool_call_id"] = json!(tool_call_id);
    }
    if let Some(tool_calls) = message.tool_calls.as_deref() {
        value["tool_calls"] = serde_json::from_str(tool_calls)?;
    }
    Ok(value)
}

fn serialize_tool_calls(calls: &[ChatToolCall]) -> EngineResult<String> {
    let calls = calls
        .iter()
        .map(|call| {
            json!({
                "id": call.id,
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": call.arguments.to_string(),
                }
            })
        })
        .collect::<Vec<_>>();
    Ok(serde_json::to_string(&calls)?)
}

fn approximate_tokens(value: &Value) -> usize {
    value.to_string().chars().count().div_ceil(4).max(1)
}

fn merge_usage(usage: &mut TokenUsage, body: &Value) {
    let source = body.get("usage").unwrap_or(&Value::Null);
    usage.input_tokens = source
        .get("prompt_tokens")
        .and_then(Value::as_u64)
        .or(usage.input_tokens);
    usage.output_tokens = source
        .get("completion_tokens")
        .and_then(Value::as_u64)
        .or(usage.output_tokens);
    usage.cache_read_tokens = source
        .get("prompt_tokens_details")
        .and_then(|value| value.get("cached_tokens"))
        .and_then(Value::as_u64)
        .or_else(|| source.get("cache_read_tokens").and_then(Value::as_u64))
        .or(usage.cache_read_tokens);
    usage.cache_write_tokens = source
        .get("completion_tokens_details")
        .and_then(|value| value.get("cached_tokens"))
        .and_then(Value::as_u64)
        .or_else(|| source.get("cache_write_tokens").and_then(Value::as_u64))
        .or(usage.cache_write_tokens);
}

fn add_usage(total: &mut TokenUsage, current: TokenUsage) {
    total.input_tokens = add_optional(total.input_tokens, current.input_tokens);
    total.output_tokens = add_optional(total.output_tokens, current.output_tokens);
    total.cache_read_tokens = add_optional(total.cache_read_tokens, current.cache_read_tokens);
    total.cache_write_tokens = add_optional(total.cache_write_tokens, current.cache_write_tokens);
}

fn add_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (None, None) => None,
        (left, right) => Some(left.unwrap_or(0).saturating_add(right.unwrap_or(0))),
    }
}

fn token_i32(value: Option<u64>) -> Option<i32> {
    value.map(|value| i32::try_from(value).unwrap_or(i32::MAX))
}

fn validate_runtime_endpoint(endpoint: AiEndpointConfig) -> EngineResult<AiEndpointConfig> {
    if endpoint.url.trim().is_empty() || endpoint.model.trim().is_empty() {
        return Err(EngineError::Validation(
            "AI unavailable - configure endpoint and model in Settings".to_string(),
        ));
    }
    Ok(endpoint)
}

fn chat_completions_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        base.to_string()
    } else if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

fn event_boundary(buffer: &[u8]) -> Option<(usize, usize)> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|position| (position, 4))
        .or_else(|| {
            buffer
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|position| (position, 2))
        })
}

fn in_flight() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static IN_FLIGHT: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    IN_FLIGHT.get_or_init(|| Mutex::new(HashMap::new()))
}

fn listeners() -> &'static Mutex<Vec<mpsc::Sender<ChatEvent>>> {
    static LISTENERS: OnceLock<Mutex<Vec<mpsc::Sender<ChatEvent>>>> = OnceLock::new();
    LISTENERS.get_or_init(|| Mutex::new(Vec::new()))
}

fn emit(options: &ChatSendOptions, event: ChatEvent) {
    if let Some(handler) = options.event_handler.as_deref() {
        handler(event.clone());
    }
    listeners()
        .lock()
        .expect("chat listeners lock")
        .retain(|sender| sender.send(event.clone()).is_ok());
}

struct InFlightGuard(String);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        in_flight()
            .lock()
            .expect("chat cancellation lock")
            .remove(&self.0);
    }
}
