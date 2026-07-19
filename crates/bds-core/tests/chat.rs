use bds_core::db::Database;
use bds_core::engine::ai::{AiEndpointConfig, AiEndpointKind};
use bds_core::engine::{chat, project};
use bds_core::model::ChatRole;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn setup() -> (tempfile::TempDir, Database, String, std::path::PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let database_path = root.path().join("bds.db");
    let data_dir = root.path().join("project");
    std::fs::create_dir_all(&data_dir).unwrap();
    let db = Database::open(&database_path).unwrap();
    db.migrate().unwrap();
    let project = project::create_project(db.conn(), "Chat Test", data_dir.to_str()).unwrap();
    project::set_active_project(db.conn(), &project.id).unwrap();
    (root, db, project.id, data_dir)
}

#[test]
fn conversation_repository_round_trips_rename_model_messages_and_delete() {
    let (_root, db, _project_id, _data_dir) = setup();
    let conversation = chat::create_conversation(db.conn(), Some("tool-model")).unwrap();
    assert_eq!(conversation.title, "Chat with tool-model");
    assert_eq!(conversation.model.as_deref(), Some("tool-model"));

    let renamed = chat::rename_conversation(db.conn(), &conversation.id, "Rust notes").unwrap();
    assert_eq!(renamed.title, "Rust notes");
    chat::set_conversation_model(db.conn(), &conversation.id, "other-model").unwrap();
    chat::insert_message(
        db.conn(),
        &conversation.id,
        ChatRole::User,
        Some("Hello"),
        None,
        None,
        Default::default(),
    )
    .unwrap();

    assert_eq!(chat::list_conversations(db.conn()).unwrap().len(), 1);
    assert_eq!(
        chat::list_messages(db.conn(), &conversation.id)
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        chat::list_messages(db.conn(), &conversation.id).unwrap()[0].role,
        ChatRole::User
    );
    chat::delete_conversation(db.conn(), &conversation.id).unwrap();
    assert!(chat::list_conversations(db.conn()).unwrap().is_empty());
    assert!(
        chat::list_messages(db.conn(), &conversation.id)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn sse_assembler_handles_split_frames_multiple_tools_and_usage() {
    let mut assembler = chat::SseAssembler::default();
    assembler
        .feed(b"data: {\"choices\":[{\"delta\":{\"content\":\"Hel")
        .unwrap();
    assembler
        .feed(b"lo\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call-1\",\"function\":{\"name\":\"search_posts\",\"arguments\":\"{\\\"query\\\":\"}},{\"index\":1,\"id\":\"call-2\",\"function\":{\"name\":\"get_blog_stats\",\"arguments\":\"{}\"}}]}}]}\n\n")
        .unwrap();
    assembler
        .feed(b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"rust\\\"}\"}}]}}],\"usage\":{\"prompt_tokens\":11,\"completion_tokens\":4,\"prompt_tokens_details\":{\"cached_tokens\":7},\"completion_tokens_details\":{\"cached_tokens\":2}}}\n\ndata: [DONE]\n\n")
        .unwrap();
    let assembled = assembler.finish().unwrap();
    assert_eq!(assembled.content, "Hello");
    assert_eq!(assembled.tool_calls.len(), 2);
    assert_eq!(assembled.tool_calls[0].name, "search_posts");
    assert_eq!(assembled.tool_calls[0].arguments["query"], "rust");
    assert_eq!(assembled.usage.input_tokens, Some(11));
    assert_eq!(assembled.usage.output_tokens, Some(4));
    assert_eq!(assembled.usage.cache_read_tokens, Some(7));
    assert_eq!(assembled.usage.cache_write_tokens, Some(2));
}

#[test]
fn unavailable_endpoint_refuses_chat_without_mutating_transcript() {
    let (_root, db, project_id, data_dir) = setup();
    let conversation = chat::create_conversation(db.conn(), None).unwrap();
    let result = chat::send_chat_message(
        db.conn(),
        &data_dir,
        &project_id,
        false,
        &conversation.id,
        "Hello",
        chat::ChatSendOptions::default(),
    );
    assert!(result.is_err());
    assert!(
        chat::list_messages(db.conn(), &conversation.id)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn streamed_turn_persists_content_session_and_all_token_fields() {
    let (_root, db, project_id, data_dir) = setup();
    let conversation = chat::create_conversation(db.conn(), Some("plain-model")).unwrap();
    let (url, server) = serve(vec![MockResponse::delayed_sse(vec![
        "data: {\"session_id\":\"session-7\",\"choices\":[{\"delta\":{\"content\":\"Hi \"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\"there\"}}],\"usage\":{\"prompt_tokens\":12,\"completion_tokens\":3,\"cache_read_tokens\":4,\"cache_write_tokens\":2}}\n\n",
        "data: [DONE]\n\n",
    ])]);
    let snapshots = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = Arc::clone(&snapshots);
    let result = chat::send_chat_message(
        db.conn(),
        &data_dir,
        &project_id,
        false,
        &conversation.id,
        "Hello",
        options(url, move |event| {
            if let chat::ChatEvent::Content { content, .. } = event {
                captured.lock().unwrap().push(content);
            }
        }),
    )
    .unwrap();
    server.join().unwrap();

    assert_eq!(result.content, "Hi there");
    assert_eq!(&*snapshots.lock().unwrap(), &["Hi ", "Hi there"]);
    let messages = chat::list_messages(db.conn(), &conversation.id).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].content.as_deref(), Some("Hi there"));
    assert_eq!(messages[1].token_usage_input, Some(12));
    assert_eq!(messages[1].token_usage_output, Some(3));
    assert_eq!(messages[1].cache_read_tokens, Some(4));
    assert_eq!(messages[1].cache_write_tokens, Some(2));
    assert_eq!(
        chat::get_conversation(db.conn(), &conversation.id)
            .unwrap()
            .copilot_session_id
            .as_deref(),
        Some("session-7")
    );
}

#[test]
fn tool_loop_persists_valid_assistant_tool_pair_before_final_answer() {
    let (_root, db, project_id, data_dir) = setup();
    let conversation = chat::create_conversation(db.conn(), Some("tool-model")).unwrap();
    let (url, server) = serve(vec![
        MockResponse::sse(vec![
            "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"stats-1\",\"function\":{\"name\":\"get_blog_stats\",\"arguments\":\"{}\"}}]}}]}\n\n",
            "data: [DONE]\n\n",
        ]),
        MockResponse::sse(vec![
            "data: {\"choices\":[{\"delta\":{\"content\":\"There are no posts.\"}}]}\n\n",
            "data: [DONE]\n\n",
        ]),
    ]);
    let result = chat::send_chat_message(
        db.conn(),
        &data_dir,
        &project_id,
        false,
        &conversation.id,
        "How many posts?",
        options(url, |_| {}),
    )
    .unwrap();
    server.join().unwrap();

    assert_eq!(result.content, "There are no posts.");
    let messages = chat::list_messages(db.conn(), &conversation.id).unwrap();
    assert_eq!(
        messages.iter().map(|item| item.role).collect::<Vec<_>>(),
        vec![
            ChatRole::User,
            ChatRole::Assistant,
            ChatRole::Tool,
            ChatRole::Assistant,
        ]
    );
    assert!(
        messages[1]
            .tool_calls
            .as_deref()
            .unwrap()
            .contains("stats-1")
    );
    assert_eq!(messages[2].tool_call_id.as_deref(), Some("stats-1"));
    assert!(
        messages[2]
            .content
            .as_deref()
            .unwrap()
            .contains("\"posts\":0")
    );
}

#[test]
fn malformed_stream_and_provider_error_keep_reopenable_user_turn() {
    for response in [
        MockResponse::sse(vec!["data: {not-json}\n\n"]),
        MockResponse::status(500, "application/json", "{\"error\":\"down\"}"),
    ] {
        let (_root, db, project_id, data_dir) = setup();
        let conversation = chat::create_conversation(db.conn(), Some("plain-model")).unwrap();
        let (url, server) = serve(vec![response]);
        assert!(
            chat::send_chat_message(
                db.conn(),
                &data_dir,
                &project_id,
                false,
                &conversation.id,
                "Remember this",
                options(url, |_| {}),
            )
            .is_err()
        );
        server.join().unwrap();
        let messages = chat::list_messages(db.conn(), &conversation.id).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, ChatRole::User);
        assert_eq!(messages[0].content.as_deref(), Some("Remember this"));
    }
}

#[test]
fn cancellation_persists_only_received_content_and_never_runs_later_work() {
    let (_root, db, project_id, data_dir) = setup();
    let conversation = chat::create_conversation(db.conn(), Some("plain-model")).unwrap();
    let conversation_id = conversation.id.clone();
    let (url, server) = serve(vec![MockResponse::delayed_sse(vec![
        "data: {\"choices\":[{\"delta\":{\"content\":\"Partial\"}}]}\n\n",
        "data: {\"choices\":[{\"delta\":{\"content\":\" ignored\"}}]}\n\n",
        "data: [DONE]\n\n",
    ])]);
    let cancel_id = conversation_id.clone();
    let result = chat::send_chat_message(
        db.conn(),
        &data_dir,
        &project_id,
        false,
        &conversation_id,
        "Start",
        options(url, move |event| {
            if matches!(event, chat::ChatEvent::Content { .. }) {
                chat::cancel_chat(&cancel_id);
            }
        }),
    )
    .unwrap();
    server.join().unwrap();
    assert!(result.cancelled);
    assert_eq!(result.content, "Partial");
    let messages = chat::list_messages(db.conn(), &conversation_id).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].content.as_deref(), Some("Partial"));
}

#[test]
fn context_truncation_keeps_system_and_complete_tool_pairs() {
    let (_root, db, _project_id, _data_dir) = setup();
    let conversation = chat::create_conversation(db.conn(), None).unwrap();
    chat::insert_message(
        db.conn(),
        &conversation.id,
        ChatRole::User,
        Some(&"old ".repeat(100)),
        None,
        None,
        Default::default(),
    )
    .unwrap();
    chat::insert_message(
        db.conn(),
        &conversation.id,
        ChatRole::Assistant,
        Some("old answer"),
        None,
        None,
        Default::default(),
    )
    .unwrap();
    chat::insert_message(
        db.conn(),
        &conversation.id,
        ChatRole::User,
        Some("new question"),
        None,
        None,
        Default::default(),
    )
    .unwrap();
    chat::insert_message(db.conn(), &conversation.id, ChatRole::Assistant, None, None, Some("[{\"id\":\"c1\",\"type\":\"function\",\"function\":{\"name\":\"get_blog_stats\",\"arguments\":\"{}\"}}]"), Default::default()).unwrap();
    chat::insert_message(
        db.conn(),
        &conversation.id,
        ChatRole::Tool,
        Some("{\"posts\":0}"),
        Some("c1"),
        None,
        Default::default(),
    )
    .unwrap();
    let context = chat::build_context(db.conn(), &_project_id, &conversation.id, 100).unwrap();
    assert_eq!(context[0]["role"], "system");
    assert!(context.iter().any(|item| item["content"] == "new question"));
    let assistant_index = context
        .iter()
        .position(|item| item.get("tool_calls").is_some())
        .unwrap();
    assert_eq!(context[assistant_index + 1]["tool_call_id"], "c1");
    assert!(!context.iter().any(|item| {
        item["content"]
            .as_str()
            .is_some_and(|value| value.starts_with("old "))
    }));
}

#[test]
fn airplane_mode_selects_the_local_endpoint_and_persists_its_model() {
    let (_root, db, project_id, data_dir) = setup();
    let conversation = chat::create_conversation(db.conn(), None).unwrap();
    let (url, server) = serve(vec![MockResponse::sse(vec![
        "data: {\"choices\":[{\"delta\":{\"content\":\"Local answer\"}}]}\n\n",
        "data: [DONE]\n\n",
    ])]);
    bds_core::engine::ai::save_endpoint(
        db.conn(),
        &AiEndpointConfig {
            kind: AiEndpointKind::Airplane,
            url,
            model: "local-airplane-model".to_string(),
            api_key: None,
        },
    )
    .unwrap();

    let result = chat::send_chat_message(
        db.conn(),
        &data_dir,
        &project_id,
        true,
        &conversation.id,
        "Stay offline",
        Default::default(),
    )
    .unwrap();
    server.join().unwrap();
    assert_eq!(result.content, "Local answer");
    assert_eq!(
        chat::get_conversation(db.conn(), &conversation.id)
            .unwrap()
            .model
            .as_deref(),
        Some("local-airplane-model")
    );
}

#[test]
fn tool_round_limit_stops_without_leaving_an_unpaired_tool_call() {
    let (_root, db, project_id, data_dir) = setup();
    let conversation = chat::create_conversation(db.conn(), Some("tool-model")).unwrap();
    let tool_frame = "data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"stats\",\"function\":{\"name\":\"get_blog_stats\",\"arguments\":\"{}\"}}]}}]}\n\ndata: [DONE]\n\n";
    let (url, server) = serve(vec![
        MockResponse::sse(vec![tool_frame]),
        MockResponse::sse(vec![tool_frame]),
    ]);
    let mut send_options = options(url, |_| {});
    send_options.max_tool_rounds = 1;
    let result = chat::send_chat_message(
        db.conn(),
        &data_dir,
        &project_id,
        false,
        &conversation.id,
        "Loop forever",
        send_options,
    );
    assert!(result.is_err());
    server.join().unwrap();

    let messages = chat::list_messages(db.conn(), &conversation.id).unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1].role, ChatRole::Assistant);
    assert_eq!(messages[2].role, ChatRole::Tool);
    assert_eq!(messages[2].tool_call_id.as_deref(), Some("stats"));
    assert!(messages[1].tool_calls.as_deref().unwrap().contains("stats"));
}

#[test]
fn cancellation_before_mutating_tool_execution_records_pairs_without_mutation() {
    let (_root, db, project_id, data_dir) = setup();
    bds_core::db::fts::ensure_fts_tables(db.conn()).unwrap();
    let post = bds_core::engine::post::create_post(
        db.conn(),
        &data_dir,
        &project_id,
        "Original",
        Some("Body"),
        Vec::new(),
        Vec::new(),
        None,
        Some("en"),
        None,
    )
    .unwrap();
    let conversation = chat::create_conversation(db.conn(), Some("tool-model")).unwrap();
    let tool_frame = format!(
        "data: {{\"choices\":[{{\"delta\":{{\"tool_calls\":[{{\"index\":0,\"id\":\"update-1\",\"function\":{{\"name\":\"update_post_metadata\",\"arguments\":\"{{\\\"post_id\\\":\\\"{}\\\",\\\"title\\\":\\\"Changed\\\"}}\"}}}},{{\"index\":1,\"id\":\"stats-2\",\"function\":{{\"name\":\"get_blog_stats\",\"arguments\":\"{{}}\"}}}}]}}}}]}}\n\ndata: [DONE]\n\n",
        post.id
    );
    let (url, server) = serve(vec![MockResponse::sse(vec![tool_frame.as_str()])]);
    let cancel_id = conversation.id.clone();
    let result = chat::send_chat_message(
        db.conn(),
        &data_dir,
        &project_id,
        false,
        &conversation.id,
        "Change it",
        options(url, move |event| {
            if matches!(event, chat::ChatEvent::ToolStarted { .. }) {
                chat::cancel_chat(&cancel_id);
            }
        }),
    )
    .unwrap();
    server.join().unwrap();

    assert!(result.cancelled);
    assert_eq!(
        bds_core::db::queries::post::get_post_by_id(db.conn(), &post.id)
            .unwrap()
            .title,
        "Original"
    );
    let messages = chat::list_messages(db.conn(), &conversation.id).unwrap();
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[2].tool_call_id.as_deref(), Some("update-1"));
    assert_eq!(messages[3].tool_call_id.as_deref(), Some("stats-2"));
    assert!(
        messages[2]
            .content
            .as_deref()
            .unwrap()
            .contains("cancelled")
    );
}

fn options(
    url: String,
    handler: impl Fn(chat::ChatEvent) + Send + Sync + 'static,
) -> chat::ChatSendOptions {
    chat::ChatSendOptions {
        endpoint: Some(AiEndpointConfig {
            kind: AiEndpointKind::Online,
            url,
            model: "plain-model".to_string(),
            api_key: Some("test-key".to_string()),
        }),
        event_handler: Some(Arc::new(handler)),
        ..Default::default()
    }
}

struct MockResponse {
    status: u16,
    content_type: &'static str,
    chunks: Vec<String>,
    delay: bool,
}

impl MockResponse {
    fn sse(chunks: Vec<&str>) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream",
            chunks: chunks.into_iter().map(str::to_string).collect(),
            delay: false,
        }
    }

    fn delayed_sse(chunks: Vec<&str>) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream",
            chunks: chunks.into_iter().map(str::to_string).collect(),
            delay: true,
        }
    }

    fn status(status: u16, content_type: &'static str, body: &str) -> Self {
        Self {
            status,
            content_type,
            chunks: vec![body.to_string()],
            delay: false,
        }
    }
}

fn serve(responses: Vec<MockResponse>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for response in responses {
            let (mut socket, _) = listener.accept().unwrap();
            read_request(&mut socket);
            let reason = if response.status == 200 {
                "OK"
            } else {
                "Error"
            };
            write!(
                socket,
                "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nConnection: close\r\n\r\n",
                response.status, reason, response.content_type
            )
            .unwrap();
            for chunk in response.chunks {
                if socket.write_all(chunk.as_bytes()).is_err() || socket.flush().is_err() {
                    break;
                }
                if response.delay {
                    thread::sleep(Duration::from_millis(60));
                }
            }
        }
    });
    (format!("http://{address}"), server)
}

fn read_request(socket: &mut std::net::TcpStream) {
    socket
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut received = Vec::new();
    let mut chunk = [0_u8; 2048];
    loop {
        let count = socket.read(&mut chunk).unwrap();
        received.extend_from_slice(&chunk[..count]);
        let Some(headers_end) = received.windows(4).position(|value| value == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&received[..headers_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(str::trim)
                    .map(str::to_string)
            })
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        if received.len() >= headers_end + 4 + content_length {
            break;
        }
    }
}
