use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mlua::{
    Function, HookTriggers, Lua, LuaOptions, LuaSerdeExt, MultiValue, StdLib, Value, VmState,
};
use serde_json::Value as JsonValue;

mod core_host;
mod manifest;

pub use core_host::{AppHostHandler, CoreHost};
pub use manifest::{ApiManifest, api_manifest};

const MACRO_TIMEOUT: Duration = Duration::from_secs(10);
const MEMORY_LIMIT_BYTES: usize = 32 * 1024 * 1024;
const MAX_TOASTS_PER_SCRIPT: usize = 5;
const MAX_TOAST_LENGTH: usize = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionKind {
    Macro,
    Utility,
    Transform,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptProgress {
    pub current: f64,
    pub total: Option<f64>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScriptExecution {
    pub value: JsonValue,
    pub output: Vec<String>,
    pub progress: Vec<ScriptProgress>,
    pub toasts: Vec<String>,
}

/// Project/application operations explicitly made available to sandboxed Lua.
pub trait HostApi: Send + Sync {
    fn call(
        &self,
        namespace: &str,
        method: &str,
        arguments: Vec<JsonValue>,
    ) -> Result<JsonValue, String>;
}

pub(crate) struct UnavailableHost;

impl HostApi for UnavailableHost {
    fn call(
        &self,
        _namespace: &str,
        _method: &str,
        _arguments: Vec<JsonValue>,
    ) -> Result<JsonValue, String> {
        Err("host capability is unavailable in this execution context".into())
    }
}

#[derive(Clone)]
pub struct ExecutionControl {
    cancelled: Arc<AtomicBool>,
}

impl Default for ExecutionControl {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl ExecutionControl {
    pub fn from_cancelled(cancelled: Arc<AtomicBool>) -> Self {
        Self { cancelled }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// Parse Lua source with the same runtime used for execution.
pub fn validate(source: &str) -> Result<(), String> {
    let lua = sandboxed_lua()?;
    lua.load(source)
        .set_name("script")
        .into_function()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Execute a named Lua entrypoint with one JSON-compatible argument.
pub fn execute(
    source: &str,
    entrypoint: &str,
    args: &JsonValue,
    kind: ExecutionKind,
    control: &ExecutionControl,
) -> Result<ScriptExecution, String> {
    execute_many_with_host(
        source,
        entrypoint,
        std::slice::from_ref(args),
        kind,
        control,
        Arc::new(UnavailableHost),
    )
}

/// Execute with project-scoped host capabilities.
pub fn execute_with_host(
    source: &str,
    entrypoint: &str,
    args: &JsonValue,
    kind: ExecutionKind,
    control: &ExecutionControl,
    host: Arc<dyn HostApi>,
) -> Result<ScriptExecution, String> {
    execute_many_with_host(
        source,
        entrypoint,
        std::slice::from_ref(args),
        kind,
        control,
        host,
    )
}

/// Execute a named Lua entrypoint with positional JSON-compatible arguments.
pub fn execute_many(
    source: &str,
    entrypoint: &str,
    args: &[JsonValue],
    kind: ExecutionKind,
    control: &ExecutionControl,
) -> Result<ScriptExecution, String> {
    execute_many_with_host(
        source,
        entrypoint,
        args,
        kind,
        control,
        Arc::new(UnavailableHost),
    )
}

/// Execute with positional arguments and project-scoped host capabilities.
pub fn execute_many_with_host(
    source: &str,
    entrypoint: &str,
    args: &[JsonValue],
    kind: ExecutionKind,
    control: &ExecutionControl,
    host: Arc<dyn HostApi>,
) -> Result<ScriptExecution, String> {
    if entrypoint.trim().is_empty() {
        return Err("script entrypoint is empty".into());
    }

    let lua = sandboxed_lua()?;
    let output = Arc::new(Mutex::new(Vec::<String>::new()));
    let progress = Arc::new(Mutex::new(Vec::<ScriptProgress>::new()));
    let toasts = Arc::new(Mutex::new(Vec::<String>::new()));
    install_host_api(&lua, &output, &progress, &toasts, host)?;

    let started = Instant::now();
    let cancelled = Arc::clone(&control.cancelled);
    lua.set_hook(
        HookTriggers::new().every_nth_instruction(5_000),
        move |_lua, _debug| {
            if cancelled.load(Ordering::Acquire) {
                return Err(mlua::Error::RuntimeError("script cancelled".into()));
            }
            if kind == ExecutionKind::Macro && started.elapsed() >= MACRO_TIMEOUT {
                return Err(mlua::Error::RuntimeError(
                    "macro exceeded 10 second timeout".into(),
                ));
            }
            Ok(VmState::Continue)
        },
    )
    .map_err(|error| error.to_string())?;

    lua.load(source)
        .set_name("script")
        .exec()
        .map_err(|error| error.to_string())?;
    let function: Function = lua
        .globals()
        .get(entrypoint)
        .map_err(|_| format!("entrypoint '{entrypoint}' was not found"))?;
    let arguments = args
        .iter()
        .map(|argument| lua.to_value(argument))
        .collect::<Result<MultiValue, _>>()
        .map_err(|error| error.to_string())?;
    let value: Value = function
        .call(arguments)
        .map_err(|error| error.to_string())?;
    let value = lua.from_value(value).map_err(|error| error.to_string())?;

    Ok(ScriptExecution {
        value,
        output: output.lock().map_err(|_| "output sink poisoned")?.clone(),
        progress: progress
            .lock()
            .map_err(|_| "progress sink poisoned")?
            .clone(),
        toasts: toasts.lock().map_err(|_| "toast sink poisoned")?.clone(),
    })
}

fn sandboxed_lua() -> Result<Lua, String> {
    let libraries = StdLib::TABLE | StdLib::STRING | StdLib::MATH | StdLib::UTF8;
    let lua = Lua::new_with(libraries, LuaOptions::default()).map_err(|error| error.to_string())?;
    lua.set_memory_limit(MEMORY_LIMIT_BYTES)
        .map_err(|error| error.to_string())?;
    let globals = lua.globals();
    for dangerous in [
        "collectgarbage",
        "debug",
        "dofile",
        "io",
        "load",
        "loadfile",
        "os",
        "package",
        "require",
    ] {
        globals
            .set(dangerous, Value::Nil)
            .map_err(|error| error.to_string())?;
    }
    drop(globals);
    Ok(lua)
}

fn install_host_api(
    lua: &Lua,
    output: &Arc<Mutex<Vec<String>>>,
    progress: &Arc<Mutex<Vec<ScriptProgress>>>,
    toasts: &Arc<Mutex<Vec<String>>>,
    host: Arc<dyn HostApi>,
) -> Result<(), String> {
    let globals = lua.globals();

    let print_output = Arc::clone(output);
    globals
        .set(
            "print",
            lua.create_function(move |_lua, values: MultiValue| {
                let line = values
                    .iter()
                    .map(lua_value_text)
                    .collect::<Vec<_>>()
                    .join("\t");
                print_output.lock().unwrap().push(line);
                Ok(())
            })
            .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;

    let bds = lua.create_table().map_err(|error| error.to_string())?;
    let app = lua.create_table().map_err(|error| error.to_string())?;

    let log_output = Arc::clone(output);
    app.set(
        "log",
        lua.create_function(move |_lua, values: MultiValue| {
            let line = values
                .iter()
                .map(lua_value_text)
                .collect::<Vec<_>>()
                .join("\t");
            log_output.lock().unwrap().push(line);
            Ok(true)
        })
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let progress_sink = Arc::clone(progress);
    let progress_host = Arc::clone(&host);
    app.set(
        "progress",
        lua.create_function(
            move |_lua, (current, total, message): (f64, Option<f64>, Option<String>)| {
                progress_sink.lock().unwrap().push(ScriptProgress {
                    current,
                    total,
                    message: message.clone(),
                });
                let _ = progress_host.call(
                    "bds",
                    "report_progress",
                    vec![serde_json::json!({
                        "current": current,
                        "total": total,
                        "message": message,
                    })],
                );
                Ok(true)
            },
        )
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let toast_sink = Arc::clone(toasts);
    app.set(
        "toast",
        lua.create_function(move |_lua, message: String| {
            let mut sink = toast_sink.lock().unwrap();
            if sink.len() < MAX_TOASTS_PER_SCRIPT {
                sink.push(message.chars().take(MAX_TOAST_LENGTH).collect());
            }
            Ok(true)
        })
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    bds.set("app", app).map_err(|error| error.to_string())?;

    for namespace in api_manifest().namespaces() {
        let table = if namespace == "app" {
            bds.get(namespace).map_err(|error| error.to_string())?
        } else {
            let table = lua.create_table().map_err(|error| error.to_string())?;
            bds.set(namespace, table.clone())
                .map_err(|error| error.to_string())?;
            table
        };
        for method in api_manifest()
            .methods
            .iter()
            .filter(|method| method.namespace == namespace)
        {
            if namespace == "app" && matches!(method.name.as_str(), "log" | "progress" | "toast") {
                continue;
            }
            let host = Arc::clone(&host);
            let namespace = method.namespace.clone();
            let method_name = method.name.clone();
            let returns = method.returns.clone();
            table
                .set(
                    method.name.as_str(),
                    lua.create_function(move |lua, values: MultiValue| {
                        let arguments = values
                            .into_iter()
                            .map(|value| lua.from_value(value))
                            .collect::<mlua::Result<Vec<JsonValue>>>()?;
                        let value = host
                            .call(&namespace, &method_name, arguments)
                            .unwrap_or_else(|_| failure_value(&returns));
                        lua.to_value(&value)
                    })
                    .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
        }
    }

    let report_sink = Arc::clone(progress);
    let report_host = Arc::clone(&host);
    bds.set(
        "report_progress",
        lua.create_function(move |lua, payload: Value| {
            let payload: JsonValue = lua.from_value(payload)?;
            let current = payload
                .get("current")
                .or_else(|| payload.get("progress"))
                .and_then(JsonValue::as_f64)
                .unwrap_or_default();
            let total = payload.get("total").and_then(JsonValue::as_f64);
            let message = payload
                .get("message")
                .and_then(JsonValue::as_str)
                .map(str::to_owned);
            report_sink.lock().unwrap().push(ScriptProgress {
                current,
                total,
                message,
            });
            let _ = report_host.call("bds", "report_progress", vec![payload]);
            Ok(true)
        })
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    globals.set("bds", bds).map_err(|error| error.to_string())?;
    Ok(())
}

fn failure_value(returns: &str) -> JsonValue {
    if returns == "boolean" {
        JsonValue::Bool(false)
    } else if returns.contains("[]") {
        JsonValue::Array(Vec::new())
    } else if returns == "table" {
        JsonValue::Object(Default::default())
    } else {
        JsonValue::Null
    }
}

fn lua_value_text(value: &Value) -> String {
    match value {
        Value::Nil => "nil".into(),
        Value::Boolean(value) => value.to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::String(value) => value.to_string_lossy(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    struct RecordingHost(Mutex<Vec<String>>);

    impl HostApi for RecordingHost {
        fn call(
            &self,
            namespace: &str,
            method: &str,
            _arguments: Vec<JsonValue>,
        ) -> Result<JsonValue, String> {
            self.0.lock().unwrap().push(format!("{namespace}.{method}"));
            Ok(json!(format!("{namespace}.{method}")))
        }
    }

    #[test]
    fn manifest_and_runtime_expose_exact_core_namespaces() {
        let manifest = api_manifest();
        assert_eq!(
            manifest.namespaces(),
            [
                "app",
                "chat",
                "media",
                "meta",
                "posts",
                "projects",
                "publish",
                "scripts",
                "tags",
                "tasks",
                "templates",
            ]
        );

        let execution = execute(
            r#"
                function main()
                    return {
                        sync = bds.sync,
                        embeddings = bds.embeddings,
                        report_progress = type(bds.report_progress),
                        post_search = type(bds.posts.search),
                        app_toast = type(bds.app.toast),
                    }
                end
            "#,
            "main",
            &JsonValue::Null,
            ExecutionKind::Utility,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(
            execution.value,
            json!({
                "report_progress": "function",
                "post_search": "function",
                "app_toast": "function",
            })
        );
    }

    #[test]
    fn representative_calls_from_every_namespace_reach_one_host_dispatcher() {
        let host = Arc::new(RecordingHost(Mutex::new(Vec::new())));
        let execution = execute_with_host(
            r#"
                function main()
                    return {
                        bds.app.get_data_paths(),
                        bds.projects.get_active(),
                        bds.meta.get_project_metadata(),
                        bds.posts.get_all(),
                        bds.media.get_all(),
                        bds.scripts.get_all(),
                        bds.templates.get_all(),
                        bds.tags.get_all(),
                        bds.tasks.status_snapshot(),
                        bds.publish.upload_site({}),
                        bds.chat.detect_post_language("title", "body"),
                    }
                end
            "#,
            "main",
            &JsonValue::Null,
            ExecutionKind::Utility,
            &ExecutionControl::default(),
            host.clone(),
        )
        .unwrap();

        assert_eq!(execution.value.as_array().unwrap().len(), 11);
        assert_eq!(host.0.lock().unwrap().len(), 11);
    }

    #[test]
    fn spec_manifest_runtime_and_generated_docs_stay_in_sync() {
        let manifest = api_manifest();
        let spec = include_str!("../../../../specs/script.allium");
        for namespace in manifest.namespaces() {
            let marker = format!("bds.{namespace}:");
            let start = spec
                .find(&marker)
                .unwrap_or_else(|| panic!("missing {marker} in spec"));
            let list = &spec[start + marker.len()..];
            let list = &list[..list.find('.').expect("method list ends with a period")];
            let expected = list
                .replace("--", "")
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect::<std::collections::BTreeSet<_>>();
            let actual = manifest
                .methods
                .iter()
                .filter(|method| method.namespace == namespace && method.compatibility == "bds2")
                .map(|method| method.name.clone())
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(
                actual, expected,
                "bds.{namespace} differs from the Allium contract"
            );
        }

        let checks = manifest
            .methods
            .iter()
            .map(|method| {
                format!(
                    "assert(type(bds.{}.{}) == 'function')",
                    method.namespace, method.name
                )
            })
            .chain(
                manifest
                    .root_methods
                    .iter()
                    .map(|method| format!("assert(type(bds.{}) == 'function')", method.name)),
            )
            .collect::<Vec<_>>()
            .join("\n");
        validate(&checks).unwrap();
        execute(
            &format!("{checks}\nfunction main() return true end"),
            "main",
            &JsonValue::Null,
            ExecutionKind::Utility,
            &ExecutionControl::default(),
        )
        .unwrap();

        assert_eq!(
            manifest.render_reference(),
            include_str!("../../../../docs/scripting/API_REFERENCE.md")
        );
        assert_eq!(
            manifest.render_types(),
            include_str!("../../../../docs/scripting/TYPES.md")
        );
        assert_eq!(
            manifest.render_completions(),
            include_str!("../../../../docs/scripting/completions.json")
        );
    }

    #[test]
    fn generated_reference_documents_every_manifest_entry() {
        let manifest = api_manifest();
        let undocumented = manifest
            .methods
            .iter()
            .filter(|method| method.description.trim().is_empty())
            .map(|method| format!("bds.{}.{}", method.namespace, method.name))
            .collect::<Vec<_>>();
        assert!(
            undocumented.is_empty(),
            "methods without documentation: {}",
            undocumented.join(", ")
        );
        let undocumented_types = manifest
            .types
            .iter()
            .filter(|api_type| api_type.description.trim().is_empty())
            .map(|api_type| api_type.name.as_str())
            .collect::<Vec<_>>();
        assert!(
            undocumented_types.is_empty(),
            "types without documentation: {}",
            undocumented_types.join(", ")
        );

        let reference = manifest.render_reference();
        for section in [
            "## Usage",
            "**Parameters**",
            "**Returns**",
            "**Example call**",
            "**Example response**",
        ] {
            assert!(reference.contains(section), "missing {section}");
        }
        let types = manifest.render_types();
        for section in ["## Value conventions", "**Lua shape**", "| Field | Type |"] {
            assert!(types.contains(section), "missing {section}");
        }
    }

    #[test]
    fn portable_method_signatures_are_identical_to_bds2() {
        let manifest = api_manifest();
        let baseline: JsonValue = serde_json::from_str(include_str!(
            "../../../../docs/scripting/bds2-core-signatures.json"
        ))
        .unwrap();
        assert_eq!(baseline["version"], manifest.version);

        let expected = baseline["methods"]
            .as_array()
            .unwrap()
            .iter()
            .map(|method| {
                let path = format!(
                    "{}.{}",
                    method["namespace"].as_str().unwrap(),
                    method["name"].as_str().unwrap()
                );
                (
                    path,
                    json!({"params": method["params"], "returns": method["returns"]}),
                )
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        let actual = manifest
            .methods
            .iter()
            .filter(|method| method.compatibility == "bds2")
            .map(|method| {
                let path = format!("{}.{}", method.namespace, method.name);
                let params = method
                    .params
                    .iter()
                    .map(|param| {
                        json!({
                            "name": param.name,
                            "type": param.type_name,
                            "required": param.required,
                        })
                    })
                    .collect::<Vec<_>>();
                (path, json!({"params": params, "returns": method.returns}))
            })
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(actual, expected);
    }

    #[test]
    fn bundled_examples_execute() {
        execute_many(
            include_str!("../../../../docs/scripting/examples/macro.lua"),
            "render",
            &[json!({"text":"Hello"}), json!({"mainLanguage":"en"})],
            ExecutionKind::Macro,
            &ExecutionControl::default(),
        )
        .unwrap();
        execute_many(
            include_str!("../../../../docs/scripting/examples/transform.lua"),
            "main",
            &[json!({}), json!({"source":"blogmark"})],
            ExecutionKind::Transform,
            &ExecutionControl::default(),
        )
        .unwrap();
        execute_with_host(
            include_str!("../../../../docs/scripting/examples/utility.lua"),
            "main",
            &JsonValue::Null,
            ExecutionKind::Utility,
            &ExecutionControl::default(),
            Arc::new(RecordingHost(Mutex::new(Vec::new()))),
        )
        .unwrap();
    }

    #[test]
    fn executes_entrypoint_and_routes_output_and_progress() {
        let result = execute(
            r#"
                function main(input)
                    print("starting", input.name)
                    bds.app.log("working")
                    bds.app.progress(1, 2, "half")
                    return { title = input.name, complete = true }
                end
            "#,
            "main",
            &json!({"name": "Example"}),
            ExecutionKind::Utility,
            &ExecutionControl::default(),
        )
        .unwrap();

        assert_eq!(result.value["title"], "Example");
        assert_eq!(result.value["complete"], true);
        assert_eq!(result.output, vec!["starting\tExample", "working"]);
        assert_eq!(result.progress[0].message.as_deref(), Some("half"));
    }

    #[test]
    fn app_progress_reaches_the_live_host_once() {
        let host = Arc::new(RecordingHost(Mutex::new(Vec::new())));
        let result = execute_with_host(
            "function main() bds.app.progress(1, 2, 'half') end",
            "main",
            &JsonValue::Null,
            ExecutionKind::Utility,
            &ExecutionControl::default(),
            Arc::clone(&host) as Arc<dyn HostApi>,
        )
        .unwrap();

        assert_eq!(result.progress.len(), 1);
        assert_eq!(host.0.lock().unwrap().as_slice(), &["bds.report_progress"]);
    }

    #[test]
    fn sandbox_hides_ambient_host_capabilities() {
        let result = execute(
            "function main(_) return { io = io, os = os, package = package, load = load } end",
            "main",
            &JsonValue::Null,
            ExecutionKind::Utility,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(result.value, serde_json::json!({}));
    }

    #[test]
    fn cancellation_stops_managed_execution() {
        let control = ExecutionControl::default();
        control.cancel();
        let error = execute(
            "function main(_) while true do end end",
            "main",
            &JsonValue::Null,
            ExecutionKind::Utility,
            &control,
        )
        .unwrap_err();
        assert!(error.contains("cancelled"));
    }

    #[test]
    fn validates_with_real_lua_parser() {
        assert!(validate("function main() return 1 end").is_ok());
        assert!(validate("function main( return 1 end").is_err());
    }

    #[test]
    fn enforces_per_script_toast_budget_and_length() {
        let result = execute(
            r#"
                function main(_)
                    for i = 1, 8 do bds.app.toast(string.rep("x", 400)) end
                end
            "#,
            "main",
            &JsonValue::Null,
            ExecutionKind::Transform,
            &ExecutionControl::default(),
        )
        .unwrap();
        assert_eq!(result.toasts.len(), 5);
        assert!(
            result
                .toasts
                .iter()
                .all(|toast| toast.chars().count() == 300)
        );
    }
}
