use std::collections::{BTreeMap, BTreeSet};

use bds_core::db::Database;
use bds_core::engine::chat;
use bds_core::engine::chat_surfaces::{
    ChartType, ChatSurfaceState, FormInputType, SurfaceKind, build_render_surface, merge_form_data,
    resolve_surface_action,
};
use serde_json::{Value, json};

#[test]
fn every_render_tool_builds_the_expected_fixed_surface_type() {
    let state = ChatSurfaceState::default();
    for (index, (tool, expected)) in [
        ("render_card", SurfaceKind::Card),
        ("render_chart", SurfaceKind::Chart),
        ("render_form", SurfaceKind::Form),
        ("render_list", SurfaceKind::List),
        ("render_metric", SurfaceKind::Metric),
        ("render_mindmap", SurfaceKind::Mindmap),
        ("render_table", SurfaceKind::Table),
        ("render_tabs", SurfaceKind::Tabs),
    ]
    .into_iter()
    .enumerate()
    {
        let surface =
            build_render_surface(tool, &json!({}), format!("message-surface-{index}"), &state)
                .unwrap();
        assert_eq!(surface.kind, expected);
    }
    assert!(build_render_surface("read_post", &json!({}), "x".into(), &state).is_none());
}

#[test]
fn charts_accept_all_types_aliases_and_coerce_invalid_numbers_to_zero() {
    let state = ChatSurfaceState::default();
    for chart_type in [
        ChartType::Bar,
        ChartType::StackedBar,
        ChartType::Line,
        ChartType::Area,
        ChartType::Pie,
        ChartType::Donut,
        ChartType::Heatmap,
    ] {
        let surface = build_render_surface(
            "render_chart",
            &json!({
                "chartType": chart_type.as_str(),
                "series": [{
                    "label": "A",
                    "value": "invalid",
                    "segments": [{"label": "one", "value": {}}, {"label": "two", "value": "2.5"}]
                }]
            }),
            "chart".into(),
            &state,
        )
        .unwrap();
        assert_eq!(surface.chart_type, Some(chart_type));
        assert_eq!(surface.series[0].value, 0.0);
        assert_eq!(surface.series[0].segments[0].value, 0.0);
        assert_eq!(surface.series[0].segments[1].value, 2.5);
    }

    let defaulted =
        build_render_surface("render_chart", &json!({}), "default".into(), &state).unwrap();
    assert_eq!(defaulted.chart_type, Some(ChartType::Bar));
    let legacy = build_render_surface(
        "render_chart",
        &json!({"chart_type": "pie"}),
        "legacy".into(),
        &state,
    )
    .unwrap();
    assert_eq!(legacy.chart_type, Some(ChartType::Pie));
}

#[test]
fn forms_support_every_input_type_and_restore_current_values() {
    let mut state = ChatSurfaceState::default();
    state.surface_data.insert(
        "form".into(),
        BTreeMap::from([
            ("title".into(), json!("Restored")),
            ("enabled".into(), json!(true)),
        ]),
    );
    let surface = build_render_surface(
        "render_form",
        &json!({
            "fields": [
                {"key": "title", "label": "Title", "inputType": "text", "defaultValue": "Default"},
                {"key": "body", "label": "Body", "inputType": "textarea"},
                {"key": "kind", "label": "Kind", "inputType": "select", "options": [{"label": "Post", "value": "post"}]},
                {"key": "enabled", "label": "Enabled", "inputType": "checkbox"},
                {"key": "date", "label": "Date", "inputType": "date"},
                {"key": "count", "label": "Count", "inputType": "number"}
            ],
            "submitAction": "openPost"
        }),
        "form".into(),
        &state,
    )
    .unwrap();
    assert_eq!(
        surface
            .fields
            .iter()
            .map(|field| field.input_type)
            .collect::<Vec<_>>(),
        vec![
            FormInputType::Text,
            FormInputType::Textarea,
            FormInputType::Select,
            FormInputType::Checkbox,
            FormInputType::Date,
            FormInputType::Number,
        ]
    );
    assert_eq!(surface.fields[0].value, json!("Restored"));
    assert_eq!(surface.fields[3].value, json!(true));

    let payload = merge_form_data(json!({"postId": "post-1"}), "form", &state);
    assert_eq!(payload["postId"], "post-1");
    assert_eq!(payload["formData"]["title"], "Restored");
    assert_eq!(payload["formData"]["enabled"], true);
}

#[test]
fn tabs_restore_selection_and_nested_unknown_content_is_safe_data() {
    let state = ChatSurfaceState {
        surface_tabs: BTreeMap::from([("tabs".into(), 1)]),
        ..Default::default()
    };
    let surface = build_render_surface(
        "render_tabs",
        &json!({"tabs": [
            {"label": "Known", "content": [{"type": "text", "body": "<script>bad()</script>"}]},
            {"label": "Fallback", "content": [{"type": "future", "html": "<b>not markup</b>"}, 42]}
        ]}),
        "tabs".into(),
        &state,
    )
    .unwrap();
    assert_eq!(surface.selected_index, Some(1));
    assert_eq!(surface.tabs[0].content[0].kind, SurfaceKind::Text);
    assert_eq!(
        surface.tabs[0].content[0].body.as_deref(),
        Some("<script>bad()</script>")
    );
    assert_eq!(surface.tabs[1].content[0].kind, SurfaceKind::Json);
    assert_eq!(surface.tabs[1].content[1].kind, SurfaceKind::Text);
}

#[test]
fn unknown_render_tool_is_inspectable_json_but_non_render_tools_are_not_surfaces() {
    let state = ChatSurfaceState::default();
    let raw = json!({"html": "<script>alert(1)</script>", "answer": 42});
    let surface = build_render_surface("render_future", &raw, "future-0".into(), &state).unwrap();
    assert_eq!(surface.kind, SurfaceKind::Json);
    assert_eq!(surface.raw.as_ref(), Some(&raw));
    assert!(build_render_surface("run_script", &raw, "bad-0".into(), &state).is_none());
}

#[test]
fn surface_state_persists_and_reopens_against_stable_ids() {
    let db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    let conversation = chat::create_conversation(db.conn(), Some("model")).unwrap();
    let expected = ChatSurfaceState {
        surface_data: BTreeMap::from([(
            "17-surface-0".into(),
            BTreeMap::from([("query".into(), json!("hello"))]),
        )]),
        surface_tabs: BTreeMap::from([("17-surface-1".into(), 2)]),
        dismissed_surfaces: BTreeSet::from(["17-surface-2".into()]),
    };
    chat::put_surface_state(db.conn(), &conversation.id, &expected).unwrap();
    assert_eq!(
        chat::get_surface_state(db.conn(), &conversation.id).unwrap(),
        expected
    );
    assert_eq!(
        chat::get_surface_state(db.conn(), "missing").unwrap(),
        ChatSurfaceState::default()
    );
}

#[test]
fn assistant_action_allow_list_accepts_aliases_and_refuses_bad_payloads() {
    for (action, payload, destination, entity_id) in [
        (
            "openPost",
            json!({"postId": "post-1"}),
            "posts",
            Some("post-1"),
        ),
        (
            "open_media",
            json!({"media_id": "media-1"}),
            "media",
            Some("media-1"),
        ),
        ("openSettings", json!({}), "settings", None),
        (
            "open_chat",
            json!({"conversationId": "chat-1"}),
            "chat",
            Some("chat-1"),
        ),
        (
            "switchView",
            json!({"view": "templates"}),
            "templates",
            None,
        ),
        ("set_view", json!({"view": "scripts"}), "scripts", None),
        ("toggleSidebar", json!({}), "toggle_sidebar", None),
        ("toggle_panel", json!({}), "toggle_panel", None),
        (
            "toggleAssistantSidebar",
            json!({}),
            "toggle_assistant_sidebar",
            None,
        ),
    ] {
        let navigation = resolve_surface_action(action, &payload).unwrap();
        assert_eq!(navigation.destination, destination);
        assert_eq!(navigation.entity_id.as_deref(), entity_id);
    }
    for (action, payload) in [
        ("runJavaScript", json!({"code": "alert(1)"})),
        ("openPost", json!({"postId": ""})),
        ("switchView", json!({"view": "root-shell"})),
        ("openMedia", Value::String("media-1".into())),
    ] {
        assert!(resolve_surface_action(action, &payload).is_err());
    }
}
