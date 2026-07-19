use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::model::ChatMessage;

pub const RENDER_TOOL_NAMES: [&str; 8] = [
    "render_card",
    "render_chart",
    "render_form",
    "render_list",
    "render_metric",
    "render_mindmap",
    "render_table",
    "render_tabs",
];

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ChatSurfaceState {
    #[serde(default)]
    pub surface_data: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(default)]
    pub surface_tabs: BTreeMap<String, usize>,
    #[serde(default)]
    pub dismissed_surfaces: BTreeSet<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceKind {
    Card,
    Chart,
    Form,
    List,
    Metric,
    Mindmap,
    Table,
    Tabs,
    Text,
    Json,
}

impl SurfaceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Card => "card",
            Self::Chart => "chart",
            Self::Form => "form",
            Self::List => "list",
            Self::Metric => "metric",
            Self::Mindmap => "mindmap",
            Self::Table => "table",
            Self::Tabs => "tabs",
            Self::Text => "text",
            Self::Json => "json",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartType {
    Bar,
    StackedBar,
    Line,
    Area,
    Pie,
    Donut,
    Heatmap,
}

impl ChartType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::StackedBar => "stacked-bar",
            Self::Line => "line",
            Self::Area => "area",
            Self::Pie => "pie",
            Self::Donut => "donut",
            Self::Heatmap => "heatmap",
        }
    }

    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("stacked-bar") => Self::StackedBar,
            Some("line") => Self::Line,
            Some("area") => Self::Area,
            Some("pie") => Self::Pie,
            Some("donut") => Self::Donut,
            Some("heatmap") => Self::Heatmap,
            _ => Self::Bar,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormInputType {
    Text,
    Textarea,
    Select,
    Checkbox,
    Date,
    Number,
}

impl FormInputType {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("textarea") => Self::Textarea,
            Some("select") => Self::Select,
            Some("checkbox") => Self::Checkbox,
            Some("date") => Self::Date,
            Some("number") => Self::Number,
            _ => Self::Text,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InlineSurface {
    pub id: String,
    pub kind: SurfaceKind,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub body: Option<String>,
    pub actions: Vec<SurfaceAction>,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub chart_type: Option<ChartType>,
    pub series: Vec<ChartSeries>,
    pub max_value: Option<f64>,
    pub label: Option<String>,
    pub value: Option<String>,
    pub items: Vec<String>,
    pub nodes: Vec<MindmapNode>,
    pub fields: Vec<FormField>,
    pub submit_label: Option<String>,
    pub submit_action: Option<String>,
    pub tabs: Vec<TabPanel>,
    pub selected_index: Option<usize>,
    pub raw: Option<Value>,
}

impl InlineSurface {
    fn empty(id: String, kind: SurfaceKind) -> Self {
        Self {
            id,
            kind,
            title: None,
            subtitle: None,
            body: None,
            actions: vec![],
            columns: vec![],
            rows: vec![],
            chart_type: None,
            series: vec![],
            max_value: None,
            label: None,
            value: None,
            items: vec![],
            nodes: vec![],
            fields: vec![],
            submit_label: None,
            submit_action: None,
            tabs: vec![],
            selected_index: None,
            raw: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceAction {
    pub label: String,
    pub action: String,
    pub payload: Value,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSeries {
    pub label: String,
    pub value: f64,
    pub segments: Vec<ChartSegment>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSegment {
    pub label: String,
    pub value: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct MindmapNode {
    pub id: Option<String>,
    pub label: String,
    pub children: Vec<String>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    pub key: String,
    pub label: String,
    pub input_type: FormInputType,
    pub placeholder: Option<String>,
    pub value: Value,
    pub options: Vec<FieldOption>,
    pub required: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldOption {
    pub label: String,
    pub value: String,
}
impl std::fmt::Display for FieldOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct TabPanel {
    pub label: String,
    pub content: Vec<InlineSurface>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceNavigation {
    pub destination: String,
    pub entity_id: Option<String>,
}

pub fn build_message_surfaces(
    message: &ChatMessage,
    state: &ChatSurfaceState,
) -> Vec<InlineSurface> {
    let Some(raw) = message.tool_calls.as_deref() else {
        return vec![];
    };
    let Ok(calls) = serde_json::from_str::<Vec<Value>>(raw) else {
        return vec![];
    };
    calls
        .iter()
        .enumerate()
        .filter_map(|(index, call)| {
            let function = call.get("function").unwrap_or(call);
            let name = function.get("name")?.as_str()?;
            let raw = function
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            let arguments = raw
                .as_str()
                .and_then(|s| serde_json::from_str(s).ok())
                .unwrap_or(raw);
            build_render_surface(
                name,
                &arguments,
                format!("{}-surface-{index}", message.id),
                state,
            )
        })
        .filter(|surface| !state.dismissed_surfaces.contains(&surface.id))
        .collect()
}

pub fn build_render_surface(
    name: &str,
    arguments: &Value,
    id: String,
    state: &ChatSurfaceState,
) -> Option<InlineSurface> {
    match name {
        "render_card" => Some(card(arguments, id)),
        "render_chart" => Some(chart(arguments, id)),
        "render_form" => Some(form(arguments, id, state)),
        "render_list" => Some(list(arguments, id)),
        "render_metric" => Some(metric(arguments, id)),
        "render_mindmap" => Some(mindmap(arguments, id)),
        "render_table" => Some(table(arguments, id)),
        "render_tabs" => Some(tabs(arguments, id, state)),
        unknown if unknown.starts_with("render_") => {
            let mut surface = InlineSurface::empty(id, SurfaceKind::Json);
            surface.raw = Some(arguments.clone());
            Some(surface)
        }
        _ => None,
    }
}

pub fn merge_form_data(payload: Value, surface_id: &str, state: &ChatSurfaceState) -> Value {
    let mut result = payload.as_object().cloned().unwrap_or_default();
    if let Some(values) = state.surface_data.get(surface_id) {
        result.insert(
            "formData".into(),
            Value::Object(values.clone().into_iter().collect()),
        );
    }
    Value::Object(result)
}

pub fn resolve_surface_action(action: &str, payload: &Value) -> Result<SurfaceNavigation, String> {
    let map = payload
        .as_object()
        .ok_or_else(|| "surface action payload must be an object".to_string())?;
    let normalized = action.replace('_', "").to_ascii_lowercase();
    let open = |destination: &str, keys: &[&str]| {
        let id = keys
            .iter()
            .find_map(|key| map.get(*key).and_then(Value::as_str))
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| format!("surface action {action} requires an identifier"))?;
        Ok(SurfaceNavigation {
            destination: destination.into(),
            entity_id: Some(id.into()),
        })
    };
    match normalized.as_str() {
        "openpost" => open("posts", &["postId", "post_id", "id"]),
        "openmedia" => open("media", &["mediaId", "media_id", "id"]),
        "openchat" => open(
            "chat",
            &[
                "conversationId",
                "conversation_id",
                "chatId",
                "chat_id",
                "id",
            ],
        ),
        "opensettings" => Ok(navigation("settings")),
        "switchview" | "setview" | "setactiveview" => {
            let view = map
                .get("view")
                .or_else(|| map.get("destination"))
                .and_then(Value::as_str)
                .filter(|view| {
                    [
                        "posts",
                        "pages",
                        "media",
                        "templates",
                        "scripts",
                        "tags",
                        "chat",
                        "import",
                        "git",
                        "settings",
                    ]
                    .contains(view)
                })
                .ok_or_else(|| format!("surface action {action} requires a valid view"))?;
            Ok(navigation(view))
        }
        "togglesidebar" => Ok(navigation("toggle_sidebar")),
        "togglepanel" | "openpanel" => Ok(navigation("toggle_panel")),
        "toggleassistantsidebar" => Ok(navigation("toggle_assistant_sidebar")),
        _ => Err(format!("unsupported surface action: {action}")),
    }
}

fn navigation(destination: &str) -> SurfaceNavigation {
    SurfaceNavigation {
        destination: destination.into(),
        entity_id: None,
    }
}

fn card(a: &Value, id: String) -> InlineSurface {
    let mut s = InlineSurface::empty(id, SurfaceKind::Card);
    s.title = string(a, "title");
    s.subtitle = string(a, "subtitle");
    s.body = string(a, "body");
    s.actions = array(a, "actions")
        .iter()
        .map(|v| SurfaceAction {
            label: string(v, "label").unwrap_or_default(),
            action: string(v, "action").unwrap_or_default(),
            payload: v.get("payload").cloned().unwrap_or_else(|| json!({})),
        })
        .collect();
    s
}

fn chart(a: &Value, id: String) -> InlineSurface {
    let mut s = InlineSurface::empty(id, SurfaceKind::Chart);
    s.title = string(a, "title");
    s.chart_type = Some(ChartType::parse(
        a.get("chartType")
            .or_else(|| a.get("chart_type"))
            .and_then(Value::as_str),
    ));
    s.series = array(a, "series")
        .iter()
        .map(|v| ChartSeries {
            label: string(v, "label").unwrap_or_default(),
            value: numeric(v.get("value")),
            segments: array(v, "segments")
                .iter()
                .map(|seg| ChartSegment {
                    label: string(seg, "label").unwrap_or_default(),
                    value: numeric(seg.get("value")),
                })
                .collect(),
        })
        .collect();
    s.max_value = Some(s.series.iter().map(|v| v.value).fold(0.0_f64, f64::max));
    s
}

fn form(a: &Value, id: String, state: &ChatSurfaceState) -> InlineSurface {
    let mut s = InlineSurface::empty(id.clone(), SurfaceKind::Form);
    s.title = string(a, "title");
    s.fields = array(a, "fields")
        .iter()
        .map(|v| {
            let key = string(v, "key").unwrap_or_else(|| "field".into());
            let default = v
                .get("defaultValue")
                .or_else(|| v.get("default_value"))
                .cloned()
                .unwrap_or(Value::Null);
            FormField {
                label: string(v, "label").unwrap_or_else(|| key.clone()),
                input_type: FormInputType::parse(
                    v.get("inputType")
                        .or_else(|| v.get("input_type"))
                        .and_then(Value::as_str),
                ),
                placeholder: string(v, "placeholder"),
                value: state
                    .surface_data
                    .get(&id)
                    .and_then(|m| m.get(&key))
                    .cloned()
                    .unwrap_or(default),
                options: array(v, "options")
                    .iter()
                    .map(|o| FieldOption {
                        label: string(o, "label").unwrap_or_default(),
                        value: string(o, "value").unwrap_or_default(),
                    })
                    .collect(),
                required: v.get("required").and_then(Value::as_bool).unwrap_or(false),
                key,
            }
        })
        .collect();
    s.submit_label = a
        .get("submitLabel")
        .or_else(|| a.get("submit_label"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    s.submit_action = a
        .get("submitAction")
        .or_else(|| a.get("submit_action"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    s
}

fn list(a: &Value, id: String) -> InlineSurface {
    let mut s = InlineSurface::empty(id, SurfaceKind::List);
    s.title = string(a, "title");
    s.items = strings(a.get("items"));
    s
}
fn metric(a: &Value, id: String) -> InlineSurface {
    let mut s = InlineSurface::empty(id, SurfaceKind::Metric);
    s.label = string(a, "label");
    s.value = scalar(a.get("value"));
    s
}
fn mindmap(a: &Value, id: String) -> InlineSurface {
    let mut s = InlineSurface::empty(id, SurfaceKind::Mindmap);
    s.title = string(a, "title");
    s.nodes = array(a, "nodes")
        .iter()
        .map(|v| MindmapNode {
            id: string(v, "id"),
            label: string(v, "label").unwrap_or_default(),
            children: strings(v.get("children")),
        })
        .collect();
    s
}
fn table(a: &Value, id: String) -> InlineSurface {
    let mut s = InlineSurface::empty(id, SurfaceKind::Table);
    s.title = string(a, "title");
    s.columns = strings(a.get("columns"));
    s.rows = array(a, "rows").iter().map(|v| strings(Some(v))).collect();
    s
}
fn tabs(a: &Value, id: String, state: &ChatSurfaceState) -> InlineSurface {
    let mut s = InlineSurface::empty(id.clone(), SurfaceKind::Tabs);
    s.title = string(a, "title");
    s.tabs = array(a, "tabs")
        .iter()
        .enumerate()
        .map(|(ti, tab)| TabPanel {
            label: string(tab, "label").unwrap_or_default(),
            content: array(tab, "content")
                .iter()
                .enumerate()
                .map(|(ci, v)| nested(v, format!("{id}-tab-{ti}-{ci}"), state))
                .collect(),
        })
        .collect();
    s.selected_index = Some(
        state
            .surface_tabs
            .get(&id)
            .copied()
            .unwrap_or(0)
            .min(s.tabs.len().saturating_sub(1)),
    );
    s
}
fn nested(v: &Value, id: String, state: &ChatSurfaceState) -> InlineSurface {
    let Some(object) = v.as_object() else {
        let mut s = InlineSurface::empty(id, SurfaceKind::Text);
        s.body = Some(scalar(Some(v)).unwrap_or_default());
        return s;
    };
    match object.get("type").and_then(Value::as_str).unwrap_or("text") {
        "card" => card(v, id),
        "chart" => chart(v, id),
        "form" => form(v, id, state),
        "list" => list(v, id),
        "metric" => metric(v, id),
        "mindmap" => mindmap(v, id),
        "table" => table(v, id),
        "tabs" => tabs(v, id, state),
        "text" => {
            let mut s = InlineSurface::empty(id, SurfaceKind::Text);
            s.body = string(v, "body").or_else(|| string(v, "text"));
            s
        }
        _ => {
            let mut s = InlineSurface::empty(id, SurfaceKind::Json);
            s.raw = Some(Value::Object(object.clone()));
            s
        }
    }
}

fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v.get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}
fn string(v: &Value, key: &str) -> Option<String> {
    scalar(v.get(key))
}
fn strings(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| scalar(Some(v)))
        .collect()
}
fn scalar(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::Null => None,
        Value::String(v) => Some(v.clone()),
        Value::Bool(v) => Some(v.to_string()),
        Value::Number(v) => Some(v.to_string()),
        v => serde_json::to_string(v).ok(),
    }
}
fn numeric(v: Option<&Value>) -> f64 {
    v.and_then(|v| {
        v.as_f64()
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
    .filter(|v| v.is_finite())
    .unwrap_or(0.0)
}

pub fn render_tool_result(name: &str, arguments: &Value) -> Option<Value> {
    let kind = name.strip_prefix("render_")?;
    RENDER_TOOL_NAMES.contains(&name).then(|| {
        let mut values = arguments.as_object().cloned().unwrap_or_default();
        values.insert("type".into(), Value::String(kind.into()));
        Value::Object(values)
    })
}
