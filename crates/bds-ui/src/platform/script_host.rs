use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

use bds_core::scripting::AppHostHandler;
use serde_json::Value;

use super::menu::MenuAction;

pub fn handler(
    menu_actions: Arc<Mutex<Vec<MenuAction>>>,
    select_folder_title: String,
) -> Arc<AppHostHandler> {
    Arc::new(move |method, args| match method {
        "copy_to_clipboard" => Ok(copy_to_clipboard(string_arg(args, 0)?).into()),
        "get_title_bar_metrics" => Ok(title_bar_metrics()),
        "notify_renderer_ready" => Ok(Value::Bool(true)),
        "open_folder" => Ok(open::that(string_arg(args, 0)?)
            .map(|()| Value::String(String::new()))
            .unwrap_or_else(|error| Value::String(error.to_string()))),
        "select_folder" => Ok(rfd::FileDialog::new()
            .set_title(optional_string_arg(args, 0).unwrap_or(select_folder_title.as_str()))
            .pick_folder()
            .map(|path| Value::String(path.to_string_lossy().into_owned()))
            .unwrap_or(Value::Null)),
        "set_preview_post_target" => {
            *preview_post_target().lock().unwrap() =
                optional_string_arg(args, 0).map(str::to_owned);
            Ok(Value::Bool(true))
        }
        "show_item_in_folder" => {
            let _ = show_item_in_folder(Path::new(string_arg(args, 0)?));
            Ok(Value::Null)
        }
        "trigger_menu_action" => {
            if let Some(action) = MenuAction::from_script_name(string_arg(args, 0)?) {
                menu_actions.lock().unwrap().push(action);
            }
            Ok(Value::Null)
        }
        _ => Err(format!("unknown desktop shell capability: {method}")),
    })
}

fn string_arg(args: &[Value], index: usize) -> Result<&str, String> {
    args.get(index)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("argument {} must be a string", index + 1))
}

fn optional_string_arg(args: &[Value], index: usize) -> Option<&str> {
    args.get(index).and_then(Value::as_str)
}

fn preview_post_target() -> &'static Mutex<Option<String>> {
    static TARGET: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    TARGET.get_or_init(|| Mutex::new(None))
}

#[cfg(target_os = "macos")]
fn title_bar_metrics() -> Value {
    json!({"macos_left_inset": 72})
}

#[cfg(not(target_os = "macos"))]
fn title_bar_metrics() -> Value {
    Value::Null
}

fn copy_to_clipboard(text: &str) -> bool {
    #[cfg(target_os = "macos")]
    let mut child = Command::new("pbcopy");
    #[cfg(target_os = "windows")]
    let mut child = {
        let mut command = Command::new("cmd");
        command.args(["/c", "clip"]);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut child = {
        let mut command = Command::new("xclip");
        command.args(["-selection", "clipboard"]);
        command
    };

    child
        .stdin(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            child.stdin.take().unwrap().write_all(text.as_bytes())?;
            child.wait().map(|status| status.success())
        })
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn show_item_in_folder(path: &Path) -> std::io::Result<()> {
    Command::new("open")
        .arg("-R")
        .arg(path)
        .status()
        .map(|_| ())
}

#[cfg(target_os = "windows")]
fn show_item_in_folder(path: &Path) -> std::io::Result<()> {
    Command::new("explorer")
        .arg(format!("/select,{}", path.display()))
        .status()
        .map(|_| ())
}

#[cfg(all(unix, not(target_os = "macos")))]
fn show_item_in_folder(path: &Path) -> std::io::Result<()> {
    open::that(path.parent().unwrap_or(path)).map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_handler_queues_supported_menu_actions() {
        let queued = Arc::new(Mutex::new(Vec::new()));
        handler(Arc::clone(&queued), String::new())("trigger_menu_action", &[serde_json::json!("new_post")])
            .unwrap();
        assert_eq!(queued.lock().unwrap().as_slice(), &[MenuAction::NewPost]);
    }
}
