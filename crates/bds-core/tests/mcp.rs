use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Barrier};

use bds_core::db::Database;
use bds_core::engine::{mcp, project};
use bds_core::model::{DomainEntity, DomainEvent, McpProposal, ProposalKind, ProposalStatus};
use serde_json::json;

struct Fixture {
    _root: tempfile::TempDir,
    database_path: std::path::PathBuf,
    data_dir: std::path::PathBuf,
    project_id: String,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let database_path = root.path().join("app/bds.db");
        let data_dir = root.path().join("project");
        std::fs::create_dir_all(database_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&data_dir).unwrap();
        let db = Database::open(&database_path).unwrap();
        db.migrate().unwrap();
        bds_core::engine::search::prepare_search_index(db.conn()).unwrap();
        let project = project::create_project(db.conn(), "MCP Test", data_dir.to_str()).unwrap();
        project::set_active_project(db.conn(), &project.id).unwrap();
        Self {
            _root: root,
            database_path,
            data_dir,
            project_id: project.id,
        }
    }

    fn context(&self) -> mcp::McpContext {
        mcp::McpContext::new(self.database_path.clone())
    }

    fn database(&self) -> Database {
        Database::open(&self.database_path).unwrap()
    }

    fn post(&self, title: &str, content: &str) -> bds_core::model::Post {
        let db = self.database();
        bds_core::engine::post::create_post(
            db.conn(),
            &self.data_dir,
            &self.project_id,
            title,
            Some(content),
            vec!["rust".into()],
            vec!["article".into()],
            Some("Alice"),
            Some("en"),
            None,
        )
        .unwrap()
    }

    fn media(&self) -> bds_core::model::Media {
        let source = self._root.path().join("source.png");
        std::fs::write(
            &source,
            include_bytes!(
                "../../../fixtures/golden-generated-sites/rfc1437-sample/images/close.png"
            ),
        )
        .unwrap();
        let db = self.database();
        bds_core::engine::media::import_media(
            db.conn(),
            &self.data_dir,
            &self.project_id,
            &source,
            "source.png",
            Some("Close"),
            Some("Close icon"),
            None,
            None,
            Some("en"),
            vec!["ui".into()],
        )
        .unwrap()
    }
}

fn resource_json(content: mcp::ResourceContent) -> serde_json::Value {
    serde_json::from_str(content.text.as_deref().unwrap()).unwrap()
}

#[test]
fn protocol_lists_every_resource_and_tool_without_mutating_state() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let post = fixture.post("Read only", "Body");
    let db = fixture.database();
    let before_posts =
        bds_core::db::queries::post::count_posts_by_project(db.conn(), &fixture.project_id)
            .unwrap();
    let before_proposals = mcp::list_proposals(db.conn(), &fixture.project_id)
        .unwrap()
        .len();

    let resources = context.list_resources();
    let templates = context.list_resource_templates();
    let tools = context.list_tools();

    assert_eq!(resources.len(), 6);
    assert_eq!(templates.len(), 4);
    for name in [
        "check_term",
        "search_posts",
        "count_posts",
        "read_post_by_slug",
        "get_post_translations",
        "get_media_translations",
        "upsert_media_translation",
        "draft_post",
        "propose_script",
        "propose_template",
        "propose_media_metadata",
        "propose_post_metadata",
    ] {
        assert!(tools.iter().any(|tool| tool["name"] == name));
    }
    context.read_resource("bds://stats").unwrap();
    context.read_resource("bds://project").unwrap();
    context
        .call_tool("read_post_by_slug", json!({"slug": post.slug}))
        .unwrap();
    assert_eq!(
        bds_core::db::queries::post::count_posts_by_project(db.conn(), &fixture.project_id)
            .unwrap(),
        before_posts
    );
    assert_eq!(
        mcp::list_proposals(db.conn(), &fixture.project_id)
            .unwrap()
            .len(),
        before_proposals
    );
}

#[test]
fn write_tools_create_inert_proposals_and_desktop_approval_executes_once() {
    let fixture = Fixture::new();
    let context = fixture.context();
    let proposed = context
        .call_tool(
            "draft_post",
            json!({"title":"Proposed post","content":"Body","tags":["rust"]}),
        )
        .unwrap();
    let proposal_id = proposed["proposalId"].as_str().unwrap();

    let db = Database::open(&fixture.database_path).unwrap();
    assert!(
        bds_core::db::queries::post::list_posts_by_project(db.conn(), &fixture.project_id)
            .unwrap()
            .is_empty()
    );
    let proposal = mcp::get_proposal(db.conn(), proposal_id).unwrap();
    assert_eq!(proposal.status, mcp::ProposalStatus::Pending);

    let events = bds_core::engine::domain_events::subscribe();
    let accepted = mcp::accept_proposal(db.conn(), &fixture.data_dir, proposal_id).unwrap();
    assert_eq!(accepted.status, mcp::ProposalStatus::Accepted);
    let posts =
        bds_core::db::queries::post::list_posts_by_project(db.conn(), &fixture.project_id).unwrap();
    assert_eq!(posts.len(), 1);
    assert_eq!(posts[0].title, "Proposed post");
    assert_eq!(posts[0].status, bds_core::model::PostStatus::Published);
    assert!(fixture.data_dir.join(&posts[0].file_path).is_file());
    assert!(events.drain().into_iter().any(|event| matches!(
        event,
        DomainEvent::EntityChanged {
            project_id,
            entity: DomainEntity::Post,
            ..
        } if project_id == fixture.project_id
    )));
    assert!(mcp::accept_proposal(db.conn(), &fixture.data_dir, proposal_id).is_err());
    assert_eq!(
        bds_core::db::queries::post::list_posts_by_project(db.conn(), &fixture.project_id)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn every_resource_supports_pagination_links_images_and_not_found() {
    let fixture = Fixture::new();
    let first = fixture.post("First post", "First body");
    for index in 0..50 {
        fixture.post(&format!("Post {index}"), "Body");
    }
    let media = fixture.media();
    let db = fixture.database();
    bds_core::engine::post_media::link_media_to_post(
        db.conn(),
        &fixture.data_dir,
        &fixture.project_id,
        &first.id,
        &media.id,
        0,
    )
    .unwrap();
    bds_core::engine::tag::sync_tags_from_posts(db.conn(), &fixture.project_id).unwrap();
    bds_core::engine::meta::write_categories_json(&fixture.data_dir, &["article".into()]).unwrap();

    let context = fixture.context();
    let project = resource_json(context.read_resource("bds://project").unwrap());
    assert_eq!(project["id"], fixture.project_id);
    assert_eq!(project["name"], "MCP Test");
    let posts = resource_json(context.read_resource("bds://posts").unwrap());
    assert_eq!(posts["items"].as_array().unwrap().len(), 50);
    assert_eq!(posts["total"], 51);
    let cursor = posts["nextCursor"].as_str().unwrap();
    let next = resource_json(
        context
            .read_resource(&format!("bds://posts?cursor={cursor}"))
            .unwrap(),
    );
    assert_eq!(next["items"].as_array().unwrap().len(), 1);
    assert!(
        context
            .read_resource("bds://posts?cursor=not-base64")
            .is_err()
    );

    assert_eq!(
        resource_json(context.read_resource("bds://media").unwrap())["total"],
        1
    );
    assert_eq!(
        resource_json(context.read_resource("bds://tags").unwrap())["items"][0]["name"],
        "rust"
    );
    assert_eq!(
        resource_json(context.read_resource("bds://categories").unwrap())["items"][0]["name"],
        "article"
    );
    let stats = resource_json(context.read_resource("bds://stats").unwrap());
    assert_eq!(stats["post_count"], 51);
    assert_eq!(stats["media_count"], 1);

    let linked = resource_json(
        context
            .read_resource(&format!("bds://posts/{}/media", first.id))
            .unwrap(),
    );
    assert_eq!(linked["items"][0]["id"], media.id);
    let image = context
        .read_resource(&format!("bds://media/{}/image", media.id))
        .unwrap();
    assert_eq!(image.mime_type, "image/png");
    assert!(!image.blob.unwrap().is_empty());
    assert!(context.read_resource("bds://posts/missing/media").is_err());
    assert!(context.read_resource("bds://unknown").is_err());
}

#[test]
fn every_read_tool_uses_shared_queries_filters_translations_and_grouping() {
    let fixture = Fixture::new();
    let post = fixture.post("Rust search", "Semantic body");
    let media = fixture.media();
    let db = fixture.database();
    bds_core::engine::post::upsert_translation(
        db.conn(),
        &fixture.data_dir,
        &post.id,
        "de",
        "Rust Suche",
        Some("Kurz"),
        Some("Deutscher Inhalt"),
    )
    .unwrap();
    bds_core::engine::media::upsert_media_translation(
        db.conn(),
        &fixture.data_dir,
        &media.id,
        "de",
        Some("Schließen"),
        Some("Schließen-Symbol"),
        None,
    )
    .unwrap();

    let context = fixture.context();
    let term = context
        .call_tool("check_term", json!({"term":"rust"}))
        .unwrap();
    assert_eq!(term["is_tag"], true);
    assert_eq!(term["tag_post_count"], 1);
    let search = context
        .call_tool(
            "search_posts",
            json!({"query":"semantic","category":"article","tags":["rust"],"language":"en","offset":0,"limit":10}),
        )
        .unwrap();
    assert_eq!(search["total"], 1);
    assert_eq!(search["posts"][0]["id"], post.id);
    let missing = context
        .call_tool("search_posts", json!({"missingTranslationLanguage":"fr"}))
        .unwrap();
    assert_eq!(missing["total"], 1);
    let count = context
        .call_tool("count_posts", json!({"groupBy":["status","tag"]}))
        .unwrap();
    assert_eq!(count["totalPosts"], 1);
    assert_eq!(count["groups"][0]["count"], 1);
    let translated = context
        .call_tool(
            "read_post_by_slug",
            json!({"slug":post.slug,"language":"de"}),
        )
        .unwrap();
    assert_eq!(translated["post"]["title"], "Rust Suche");
    assert_eq!(translated["post"]["content"], "Deutscher Inhalt");
    assert_eq!(
        context
            .call_tool("get_post_translations", json!({"postId":post.id}))
            .unwrap()["translations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        context
            .call_tool("get_media_translations", json!({"mediaId":media.id}))
            .unwrap()["translations"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        context
            .call_tool("search_posts", json!({"month":2}))
            .is_err()
    );
    assert!(context.call_tool("missing", json!({})).is_err());
}

#[test]
fn every_write_tool_is_inert_then_approves_or_rejects_through_shared_engines() {
    let fixture = Fixture::new();
    let post = fixture.post("Existing", "Body");
    let media = fixture.media();
    let context = fixture.context();
    let db = fixture.database();

    let requests = [
        (
            "upsert_media_translation",
            json!({"mediaId":media.id,"language":"fr","title":"Fermer"}),
        ),
        (
            "propose_script",
            json!({"title":"Task","kind":"utility","content":"function main()\n return true\nend"}),
        ),
        (
            "propose_template",
            json!({"title":"Card","kind":"partial","content":"<p>{{ post.title }}</p>"}),
        ),
        (
            "propose_media_metadata",
            json!({"mediaId":media.id,"title":"Updated media","tags":["updated"]}),
        ),
        (
            "propose_post_metadata",
            json!({"postId":post.id,"title":"Updated post","categories":["news"]}),
        ),
    ];
    let mut proposal_ids = Vec::new();
    for (tool, params) in requests {
        let response = context.call_tool(tool, params).unwrap();
        proposal_ids.push(response["proposalId"].as_str().unwrap().to_string());
    }
    assert_eq!(
        bds_core::db::queries::script::list_scripts_by_project(db.conn(), &fixture.project_id)
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        bds_core::db::queries::template::list_templates_by_project(db.conn(), &fixture.project_id)
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        bds_core::db::queries::post::get_post_by_id(db.conn(), &post.id)
            .unwrap()
            .title,
        "Existing"
    );
    assert_eq!(
        bds_core::db::queries::media::get_media_by_id(db.conn(), &media.id)
            .unwrap()
            .title
            .as_deref(),
        Some("Close")
    );

    let events = bds_core::engine::domain_events::subscribe();
    for proposal_id in proposal_ids.iter().take(4) {
        mcp::accept_proposal(db.conn(), &fixture.data_dir, proposal_id).unwrap();
    }
    mcp::reject_proposal(db.conn(), &fixture.data_dir, &proposal_ids[4]).unwrap();
    assert_eq!(
        bds_core::db::queries::media_translation::list_media_translations_by_media(
            db.conn(),
            &media.id
        )
        .unwrap()
        .len(),
        1
    );
    assert_eq!(
        bds_core::db::queries::script::list_scripts_by_project(db.conn(), &fixture.project_id)
            .unwrap()[0]
            .status,
        bds_core::model::ScriptStatus::Published
    );
    assert_eq!(
        bds_core::db::queries::template::list_templates_by_project(db.conn(), &fixture.project_id)
            .unwrap()[0]
            .status,
        bds_core::model::TemplateStatus::Published
    );
    assert_eq!(
        bds_core::db::queries::media::get_media_by_id(db.conn(), &media.id)
            .unwrap()
            .title
            .as_deref(),
        Some("Updated media")
    );
    assert_eq!(
        bds_core::db::queries::post::get_post_by_id(db.conn(), &post.id)
            .unwrap()
            .title,
        "Existing"
    );
    assert_eq!(
        mcp::get_proposal(db.conn(), &proposal_ids[4])
            .unwrap()
            .status,
        ProposalStatus::Rejected
    );
    let changed_entities = events
        .drain()
        .into_iter()
        .filter_map(|event| match event {
            DomainEvent::EntityChanged {
                project_id, entity, ..
            } if project_id == fixture.project_id => Some(entity),
            _ => None,
        })
        .collect::<Vec<_>>();
    for entity in [
        DomainEntity::Media,
        DomainEntity::Script,
        DomainEntity::Template,
    ] {
        assert!(
            changed_entities.contains(&entity),
            "approved {entity:?} proposal did not emit a domain event"
        );
    }
    assert!(
        context
            .call_tool(
                "propose_script",
                json!({"title":"Bad","kind":"utility","content":"function ("}),
            )
            .is_err()
    );
    assert!(
        context
            .call_tool(
                "propose_template",
                json!({"title":"Bad","kind":"post","content":"{% if x %}"}),
            )
            .is_err()
    );
}

#[test]
fn propose_template_rejects_unsupported_liquid_without_creating_a_proposal() {
    let fixture = Fixture::new();
    let context = fixture.context();

    let error = context
        .call_tool(
            "propose_template",
            json!({
                "title": "Unsafe template",
                "kind": "partial",
                "content": "{{ title | upcase }}"
            }),
        )
        .unwrap_err();

    assert!(error.to_string().contains("unsupported filter: upcase"));
    let db = fixture.database();
    assert!(
        mcp::list_proposals(db.conn(), &fixture.project_id)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn expiry_invalid_ids_unavailable_projects_and_concurrent_acceptance_are_safe() {
    let fixture = Fixture::new();
    let db = fixture.database();
    let expired = McpProposal {
        id: "expired".into(),
        project_id: fixture.project_id.clone(),
        kind: ProposalKind::DraftPost,
        status: ProposalStatus::Pending,
        entity_id: None,
        data: json!({"title":"Expired","content":"Body"}).to_string(),
        result: None,
        created_at: 1,
        expires_at: 2,
        resolved_at: None,
    };
    bds_core::db::queries::mcp_proposal::insert_proposal(db.conn(), &expired).unwrap();
    assert!(
        mcp::list_pending_proposals(db.conn(), &fixture.project_id)
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        mcp::get_proposal(db.conn(), "expired").unwrap().status,
        ProposalStatus::Expired
    );
    assert!(mcp::accept_proposal(db.conn(), &fixture.data_dir, "missing").is_err());

    let response = fixture
        .context()
        .call_tool("draft_post", json!({"title":"Once","content":"Body"}))
        .unwrap();
    let proposal_id = response["proposalId"].as_str().unwrap().to_string();
    let barrier = Arc::new(Barrier::new(2));
    let threads = (0..2)
        .map(|_| {
            let barrier = Arc::clone(&barrier);
            let database_path = fixture.database_path.clone();
            let data_dir = fixture.data_dir.clone();
            let proposal_id = proposal_id.clone();
            std::thread::spawn(move || {
                let db = Database::open(&database_path).unwrap();
                barrier.wait();
                mcp::accept_proposal(db.conn(), &data_dir, &proposal_id).is_ok()
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .filter(|accepted| *accepted)
            .count(),
        1
    );
    assert_eq!(
        bds_core::db::queries::post::list_posts_by_project(db.conn(), &fixture.project_id)
            .unwrap()
            .len(),
        1
    );

    let empty_root = tempfile::tempdir().unwrap();
    let empty_path = empty_root.path().join("empty.db");
    let empty_db = Database::open(&empty_path).unwrap();
    empty_db.migrate().unwrap();
    assert!(
        mcp::McpContext::new(empty_path)
            .call_tool("check_term", json!({"term":"rust"}))
            .is_err()
    );
}

#[test]
fn protocol_routes_every_family_and_http_enforces_loopback_origin_and_cors() {
    let fixture = Fixture::new();
    let post = fixture.post("Protocol post", "Protocol body");
    let media = fixture.media();
    let db = fixture.database();
    bds_core::engine::post_media::link_media_to_post(
        db.conn(),
        &fixture.data_dir,
        &fixture.project_id,
        &post.id,
        &media.id,
        0,
    )
    .unwrap();
    let context = fixture.context();
    for (id, method, params, result_key) in [
        (
            1,
            "initialize",
            json!({"protocolVersion":"2025-06-18"}),
            "serverInfo",
        ),
        (2, "tools/list", json!({}), "tools"),
        (3, "resources/list", json!({}), "resources"),
        (
            4,
            "resources/templates/list",
            json!({}),
            "resourceTemplates",
        ),
    ] {
        let response = mcp::handle_rpc(
            &context,
            &json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}),
        )
        .unwrap();
        assert!(response["result"].get(result_key).is_some(), "{method}");
    }
    for (id, uri) in [
        "bds://project".to_string(),
        "bds://posts".to_string(),
        "bds://media".to_string(),
        "bds://tags".to_string(),
        "bds://categories".to_string(),
        "bds://stats".to_string(),
        format!("bds://posts/{}/media", post.id),
        format!("bds://media/{}/image", media.id),
    ]
    .into_iter()
    .enumerate()
    {
        let response = mcp::handle_rpc(
            &context,
            &json!({"jsonrpc":"2.0","id":10+id,"method":"resources/read","params":{"uri":uri}}),
        )
        .unwrap();
        assert!(response["result"]["contents"].is_array(), "{uri}");
    }
    let tool_calls = [
        ("check_term", json!({"term":"rust"})),
        ("search_posts", json!({"query":"protocol"})),
        ("count_posts", json!({"groupBy":["status"]})),
        ("read_post_by_slug", json!({"slug":post.slug})),
        ("get_post_translations", json!({"postId":post.id})),
        ("get_media_translations", json!({"mediaId":media.id})),
        (
            "upsert_media_translation",
            json!({"mediaId":media.id,"language":"de","title":"Bild"}),
        ),
        (
            "draft_post",
            json!({"title":"Protocol proposal","content":"Body"}),
        ),
        (
            "propose_script",
            json!({"title":"Protocol script","kind":"utility","content":"function main() end"}),
        ),
        (
            "propose_template",
            json!({"title":"Protocol template","kind":"partial","content":"{{ post.title }}"}),
        ),
        (
            "propose_media_metadata",
            json!({"mediaId":media.id,"title":"Changed"}),
        ),
        (
            "propose_post_metadata",
            json!({"postId":post.id,"title":"Changed"}),
        ),
    ];
    for (id, (name, arguments)) in tool_calls.into_iter().enumerate() {
        let call = mcp::handle_rpc(
            &context,
            &json!({"jsonrpc":"2.0","id":30+id,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
        )
        .unwrap();
        assert_eq!(call["result"]["isError"], false, "{name}: {call}");
    }
    assert_eq!(
        mcp::handle_rpc(
            &context,
            &json!({"jsonrpc":"2.0","id":7,"method":"missing"}),
        )
        .unwrap()["error"]["code"],
        -32601
    );

    let server = mcp::McpHttpServer::start(fixture.database_path.clone(), 0).unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .build()
        .unwrap();
    let response = client
        .post(server.endpoint())
        .header("accept", "application/json, text/event-stream")
        .header("origin", "http://localhost:3000")
        .json(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}))
        .send()
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("access-control-allow-origin")
            .unwrap(),
        "http://localhost:3000"
    );
    assert!(response.headers().get("mcp-session-id").is_none());
    let forbidden = client
        .post(server.endpoint())
        .header("accept", "application/json, text/event-stream")
        .header("origin", "https://attacker.example")
        .json(&json!({"jsonrpc":"2.0","id":2,"method":"ping"}))
        .send()
        .unwrap();
    assert_eq!(forbidden.status(), reqwest::StatusCode::FORBIDDEN);
    let get = client.get(server.endpoint()).send().unwrap();
    assert_eq!(get.status(), reqwest::StatusCode::METHOD_NOT_ALLOWED);
    server.stop().unwrap();
}

#[test]
fn http_server_binds_loopback_and_rejects_remote_origins() {
    let fixture = Fixture::new();
    let server = mcp::McpHttpServer::start(fixture.database_path.clone(), 0).unwrap();
    assert_eq!(server.address().ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_eq!(
        server.endpoint(),
        format!("http://{}/mcp", server.address())
    );
    server.stop().unwrap();
}
