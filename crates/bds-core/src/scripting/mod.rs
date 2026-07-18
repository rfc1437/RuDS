use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mlua::{
    Function, HookTriggers, Lua, LuaOptions, LuaSerdeExt, MultiValue, StdLib, Value, VmState,
};
use serde_json::Value as JsonValue;

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
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
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
    execute_many(
        source,
        entrypoint,
        std::slice::from_ref(args),
        kind,
        control,
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
    if entrypoint.trim().is_empty() {
        return Err("script entrypoint is empty".into());
    }

    let lua = sandboxed_lua()?;
    let output = Arc::new(Mutex::new(Vec::<String>::new()));
    let progress = Arc::new(Mutex::new(Vec::<ScriptProgress>::new()));
    let toasts = Arc::new(Mutex::new(Vec::<String>::new()));
    install_host_api(&lua, &output, &progress, &toasts)?;

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
            Ok(())
        })
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    let progress_sink = Arc::clone(progress);
    app.set(
        "progress",
        lua.create_function(
            move |_lua, (current, total, message): (f64, Option<f64>, Option<String>)| {
                progress_sink.lock().unwrap().push(ScriptProgress {
                    current,
                    total,
                    message,
                });
                Ok(())
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
            Ok(())
        })
        .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;

    bds.set("app", app).map_err(|error| error.to_string())?;
    globals.set("bds", bds).map_err(|error| error.to_string())?;
    Ok(())
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
