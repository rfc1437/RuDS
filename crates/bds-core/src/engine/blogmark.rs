use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use url::Url;

use crate::db::DbConnection as Connection;
use crate::db::queries::script as script_queries;
use crate::engine::{EngineError, EngineResult};
use crate::model::{Post, Script, ScriptKind};
use crate::scripting::{self, ExecutionControl, ExecutionKind};

const MAX_TITLE_LENGTH: usize = 200;
const MAX_URL_LENGTH: usize = 2_048;
const MAX_TOASTS_TOTAL: usize = 20;
const MAX_TOAST_LENGTH: usize = 300;
const BLOGMARK_SCHEME: &str = "ruds";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlogmarkCandidate {
    pub title: String,
    pub url: Option<String>,
    pub content: Option<String>,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BlogmarkImportResult {
    pub post: Post,
    pub toasts: Vec<String>,
    pub transform_errors: Vec<String>,
}

pub fn parse_deep_link(raw: &str) -> EngineResult<BlogmarkCandidate> {
    let parsed =
        Url::parse(raw).map_err(|_| EngineError::Validation("invalid blogmark URL".into()))?;
    if parsed.scheme() != BLOGMARK_SCHEME {
        return Err(EngineError::Validation(
            "unsupported blogmark scheme".into(),
        ));
    }
    if parsed.host_str() != Some("new-post") {
        return Err(EngineError::Validation(
            "unsupported blogmark action".into(),
        ));
    }
    let params = parsed
        .query_pairs()
        .into_owned()
        .collect::<std::collections::HashMap<_, _>>();
    let url = params.get("url").and_then(|value| sanitize_http_url(value));
    let fallback = url
        .as_ref()
        .and_then(|value| Url::parse(value).ok())
        .and_then(|value| value.host_str().map(str::to_string));
    let title = sanitize_title(params.get("title").map(String::as_str), fallback.as_deref());
    Ok(BlogmarkCandidate {
        title,
        url,
        content: nonempty(params.get("content")),
        tags: list_param(params.get("tags")),
        categories: list_param(params.get("categories")),
        project_id: nonempty(params.get("project_id")),
    })
}

pub fn bookmarklet(project_id: &str) -> String {
    let project_id =
        url::form_urlencoded::byte_serialize(project_id.as_bytes()).collect::<String>();
    format!(
        "javascript:(()=>{{const t=encodeURIComponent(document.title||'');const u=encodeURIComponent(location.href||'');location.href='ruds://new-post?title='+t+'&url='+u+'&project_id={project_id}';}})();"
    )
}

pub fn receive_deep_link(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    raw: &str,
) -> EngineResult<BlogmarkImportResult> {
    let mut candidate = parse_deep_link(raw)?;
    if let Some(target) = &candidate.project_id
        && target != project_id
    {
        return Err(EngineError::Validation(
            "blogmark targets a different project".into(),
        ));
    }
    if candidate.content.is_none()
        && let Some(url) = &candidate.url
    {
        candidate.content = Some(format!(
            "[{}]({url})",
            escape_markdown_link_text(&candidate.title)
        ));
    }
    let (mut candidate, toasts, transform_errors) =
        run_transforms(conn, data_dir, project_id, candidate)?;
    if candidate.categories.is_empty() {
        let metadata = crate::engine::meta::read_project_json(data_dir)?;
        if let Some(category) = metadata
            .blogmark_category
            .filter(|value| !value.trim().is_empty())
        {
            candidate.categories.push(category);
        }
    }
    let metadata = crate::engine::meta::read_project_json(data_dir)?;
    let post = crate::engine::post::create_post(
        conn,
        data_dir,
        project_id,
        &candidate.title,
        candidate.content.as_deref(),
        candidate.tags,
        candidate.categories,
        metadata.default_author.as_deref(),
        metadata.main_language.as_deref(),
        None,
    )?;
    Ok(BlogmarkImportResult {
        post,
        toasts,
        transform_errors,
    })
}

fn run_transforms(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    candidate: BlogmarkCandidate,
) -> EngineResult<(BlogmarkCandidate, Vec<String>, Vec<String>)> {
    let mut transforms = script_queries::list_scripts_by_project(conn, project_id)?
        .into_iter()
        .filter(|script| script.kind == ScriptKind::Transform && script.enabled)
        .collect::<Vec<_>>();
    transforms.sort_by(|left, right| {
        left.updated_at
            .cmp(&right.updated_at)
            .then_with(|| left.slug.cmp(&right.slug))
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut current =
        serde_json::to_value(candidate).map_err(|error| EngineError::Parse(error.to_string()))?;
    let mut toasts = Vec::new();
    let mut errors = Vec::new();
    for script in transforms {
        if script.entrypoint.trim().is_empty() {
            continue;
        }
        let source = resolved_script_content(data_dir, &script)?;
        let context = json!({
            "source": "blogmark",
            "url": current.get("url").cloned().unwrap_or(Value::Null),
        });
        match scripting::execute_many(
            &source,
            &script.entrypoint,
            &[current.clone(), context],
            ExecutionKind::Transform,
            &ExecutionControl::default(),
        ) {
            Ok(execution) => {
                let (next, returned_toasts) = split_transform_result(execution.value, &current);
                current = next;
                accept_toasts(
                    &mut toasts,
                    execution.toasts.into_iter().chain(returned_toasts),
                );
            }
            Err(error) => errors.push(format!("{}: {error}", script.slug)),
        }
    }
    let candidate = serde_json::from_value(current).map_err(|error| {
        EngineError::Validation(format!("transform returned invalid post data: {error}"))
    })?;
    Ok((candidate, toasts, errors))
}

fn split_transform_result(value: Value, previous: &Value) -> (Value, Vec<String>) {
    if let Some(object) = value.as_object() {
        if let Some(data) = object.get("data").filter(|value| value.is_object()) {
            let toasts = object
                .get("toasts")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            return (data.clone(), toasts);
        }
        return (value, Vec::new());
    }
    (previous.clone(), Vec::new())
}

fn accept_toasts(target: &mut Vec<String>, source: impl IntoIterator<Item = String>) {
    for message in source.into_iter().take(5) {
        if target.len() >= MAX_TOASTS_TOTAL {
            break;
        }
        target.push(message.chars().take(MAX_TOAST_LENGTH).collect());
    }
}

fn resolved_script_content(data_dir: &Path, script: &Script) -> EngineResult<String> {
    if let Some(content) = &script.content {
        return Ok(content.clone());
    }
    let raw = fs::read_to_string(data_dir.join(&script.file_path))?;
    crate::util::frontmatter::read_script_file(&raw)
        .map(|(_, body)| body)
        .map_err(EngineError::Parse)
}

fn sanitize_title(value: Option<&str>, fallback: Option<&str>) -> String {
    let cleaned = value
        .unwrap_or_default()
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>();
    let cleaned = cleaned.trim();
    let selected = if cleaned.is_empty() {
        fallback.unwrap_or_default()
    } else {
        cleaned
    };
    selected.chars().take(MAX_TITLE_LENGTH).collect()
}

fn sanitize_http_url(value: &str) -> Option<String> {
    let cleaned = value
        .chars()
        .filter(|ch| !ch.is_control())
        .collect::<String>();
    let mut parsed = Url::parse(cleaned.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return None;
    }
    parsed.set_fragment(None);
    parsed.set_username("").ok()?;
    parsed.set_password(None).ok()?;
    let normalized = parsed.to_string();
    (normalized.chars().count() <= MAX_URL_LENGTH).then_some(normalized)
}

fn nonempty(value: Option<&String>) -> Option<String> {
    value
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn list_param(value: Option<&String>) -> Vec<String> {
    value
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn escape_markdown_link_text(value: &str) -> String {
    value
        .trim()
        .replace('\\', "\\\\")
        .replace('[', "\\[")
        .replace(']', "\\]")
        .replace('(', "\\(")
        .replace(')', "\\)")
        .replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::model::ScriptKind;
    use tempfile::TempDir;

    #[test]
    fn parses_and_hardens_blogmark_links() {
        let candidate = parse_deep_link(
            "ruds://new-post?title=%00Hello&url=https%3A%2F%2Fuser%3Apass%40example.com%2Fa%23frag&tags=one%2Ctwo",
        )
        .unwrap();
        assert_eq!(candidate.title, "Hello");
        assert_eq!(candidate.url.as_deref(), Some("https://example.com/a"));
        assert_eq!(candidate.tags, vec!["one", "two"]);
    }

    #[test]
    fn rejects_other_schemes_and_actions() {
        assert!(parse_deep_link("bds://new-post?title=x").is_err());
        assert!(parse_deep_link("bds2://new-post?title=x").is_err());
        assert!(parse_deep_link("ruds://other?title=x").is_err());
    }

    #[test]
    fn bookmarklet_targets_only_ruds_and_the_selected_project() {
        let value = bookmarklet("project & seven");
        assert!(value.contains("ruds://new-post?"));
        assert!(value.contains("project_id=project+%26+seven"));
        assert!(!value.contains("bds2://"));
    }

    #[test]
    fn invalid_source_url_never_reaches_candidate() {
        let candidate =
            parse_deep_link("ruds://new-post?title=x&url=javascript%3Aalert%281%29").unwrap();
        assert!(candidate.url.is_none());
    }

    #[test]
    fn imports_draft_after_ordered_transform_pipeline() {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        crate::db::fts::ensure_fts_tables(db.conn()).unwrap();
        let directory = TempDir::new().unwrap();
        let project = crate::engine::project::create_project(
            db.conn(),
            "Blog",
            Some(directory.path().to_str().unwrap()),
        )
        .unwrap();
        crate::engine::script::create_script(
            db.conn(),
            &project.id,
            "Add tag",
            ScriptKind::Transform,
            "function main(data, context) data.tags = {context.source}; bds.app.toast('done'); return data end",
            None,
        )
        .unwrap();

        let result = receive_deep_link(
            db.conn(),
            directory.path(),
            &project.id,
            "ruds://new-post?title=Example&url=https%3A%2F%2Fexample.com",
        )
        .unwrap();

        assert_eq!(result.post.tags, vec!["blogmark"]);
        assert_eq!(
            result.post.content.as_deref(),
            Some("[Example](https://example.com/)")
        );
        assert_eq!(result.toasts, vec!["done"]);
        assert!(result.transform_errors.is_empty());
    }
}
