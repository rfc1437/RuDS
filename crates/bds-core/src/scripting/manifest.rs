use std::collections::BTreeSet;
use std::sync::LazyLock;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ApiManifest {
    pub version: String,
    pub root_methods: Vec<ApiMethod>,
    pub methods: Vec<ApiMethod>,
    pub types: Vec<ApiType>,
}

#[derive(Debug, Deserialize)]
pub struct ApiMethod {
    #[serde(default = "bds2_compatibility")]
    pub compatibility: String,
    #[serde(default)]
    pub namespace: String,
    pub name: String,
    pub params: Vec<ApiParameter>,
    pub returns: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct ApiParameter {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    pub required: bool,
}

#[derive(Debug, Deserialize)]
pub struct ApiType {
    pub name: String,
    pub description: String,
    pub fields: serde_json::Map<String, serde_json::Value>,
}

impl ApiManifest {
    pub fn namespaces(&self) -> Vec<&str> {
        self.methods
            .iter()
            .map(|method| method.namespace.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub fn render_reference(&self) -> String {
        let mut output = format!(
            "# RuDS Lua API Reference\n\nContract version: `{}`\n\nThe `bds` global is the supported bridge from sandboxed Lua scripts to RuDS. The unmarked method signatures are identical to bDS2 so the same published script file can run in either application. Calls are synchronous, JSON-compatible values cross the bridge as Lua tables, and project-scoped methods always use the active project.\n\n## Usage\n\nA utility script exposes `main(input)` and calls the API through `bds`:\n\n```lua\nfunction main(input)\n  local posts = bds.posts.get_all()\n  bds.app.log(\"Found \" .. #posts .. \" posts\")\n  return posts\nend\n```\n\nMacro scripts expose `render(input, context)` and transform scripts expose `main(input, context)`. Complete runnable files are in [`examples/`](examples/). Scripts cannot access the network, filesystem, processes, environment variables, or native Lua modules directly; use the documented host methods instead. Host failures return `nil` or `false` where the signature permits it.\n\n## Conventions\n\n- A parameter ending in `?` is optional.\n- `T | nil` means the call can return no value. Check for `nil` before using it.\n- `T[]` is a one-based Lua array.\n- Public records are documented in [Lua API Types](TYPES.md).\n- Dates and timestamps returned by RuDS records are ISO-8601 strings.\n- Methods marked **RuDS extension** are not available to scripts running under bDS2.\n\n## Contents\n",
            self.version
        );

        if !self.root_methods.is_empty() {
            output.push_str("\n- [Root helpers](#root-helpers)");
        }
        for namespace in self.namespaces() {
            output.push_str(&format!("\n- [`bds.{namespace}`](#bds{namespace})"));
        }
        output.push_str("\n- [Public data types](TYPES.md)\n");

        if !self.root_methods.is_empty() {
            output.push_str("\n## Root helpers\n");
            for method in &self.root_methods {
                self.render_method(&mut output, method);
            }
        }

        for namespace in self.namespaces() {
            output.push_str(&format!("\n## `bds.{namespace}`\n"));
            for method in self
                .methods
                .iter()
                .filter(|method| method.namespace == namespace)
            {
                self.render_method(&mut output, method);
            }
        }
        output
    }

    pub fn render_types(&self) -> String {
        let mut output = format!(
            "# Lua API Types\n\nContract version: `{}`\n\nThese are the public, JSON-compatible records returned by the Lua host API. They contain no database handles or private implementation fields.\n\n## Value conventions\n\n- `T | nil` marks an optional field.\n- `T[]` is a one-based Lua array.\n- `table` is a JSON-compatible Lua table whose shape depends on the operation.\n- ISO-8601 values are strings such as `2026-07-19T08:00:00Z`.\n\n## Contents\n",
            self.version
        );
        for api_type in &self.types {
            output.push_str(&format!(
                "\n- [`{}`](#{})",
                api_type.name,
                api_type.name.to_lowercase()
            ));
        }
        output.push('\n');

        for api_type in &self.types {
            output.push_str(&format!(
                "\n## `{}`\n\n{}\n\n**Lua shape**\n\n```lua\n{}\n```\n\n| Field | Type | Required | Meaning |\n| --- | --- | --- | --- |\n",
                api_type.name,
                api_type.description,
                self.example_for_type(api_type)
            ));
            for (field, type_name) in &api_type.fields {
                let type_name = type_name.as_str().unwrap_or("any");
                output.push_str(&format!(
                    "| `{field}` | `{}` | {} | {} |\n",
                    type_name.replace('|', "\\|"),
                    if type_name.contains("nil") {
                        "No"
                    } else {
                        "Yes"
                    },
                    field_description(field)
                ));
            }
        }
        output
    }

    pub fn render_completions(&self) -> String {
        let completions = self
            .methods
            .iter()
            .map(|method| {
                serde_json::json!({
                    "label": format!("bds.{}.{}", method.namespace, method.name),
                    "insert_text": self.example_call(method),
                    "detail": self.signature(method),
                    "description": method.description,
                    "parameters": method.params.iter().map(|param| serde_json::json!({
                        "name": param.name,
                        "type": param.type_name,
                        "required": param.required,
                    })).collect::<Vec<_>>(),
                    "returns": method.returns,
                })
            })
            .chain(self.root_methods.iter().map(|method| {
                serde_json::json!({
                    "label": format!("bds.{}", method.name),
                    "insert_text": self.example_call(method),
                    "detail": self.signature(method),
                    "description": method.description,
                    "parameters": method.params.iter().map(|param| serde_json::json!({
                        "name": param.name,
                        "type": param.type_name,
                        "required": param.required,
                    })).collect::<Vec<_>>(),
                    "returns": method.returns,
                })
            }))
            .collect::<Vec<_>>();
        serde_json::to_string_pretty(&completions).expect("completion data is serializable") + "\n"
    }

    fn render_method(&self, output: &mut String, method: &ApiMethod) {
        let path = method_path(method);
        output.push_str(&format!("\n### `{path}`\n\n{}\n\n", method.description));
        if method.compatibility == "ruds" {
            output.push_str(
                "> **RuDS extension:** this helper is not part of the portable bDS2 API.\n\n",
            );
        }
        output.push_str(&format!(
            "**Signature**\n\n```text\n{}\n```\n\n**Parameters**\n\n",
            self.signature(method)
        ));
        if method.params.is_empty() {
            output.push_str("None.\n");
        } else {
            output.push_str("| Name | Type | Required | Example |\n| --- | --- | --- | --- |\n");
            for param in &method.params {
                output.push_str(&format!(
                    "| `{}` | `{}` | {} | `{}` |\n",
                    param.name,
                    param.type_name.replace('|', "\\|"),
                    if param.required { "Yes" } else { "No" },
                    example_argument(param).replace('|', "\\|")
                ));
            }
        }

        output.push_str(&format!(
            "\n**Returns**\n\n{}",
            self.render_return_type(&method.returns)
        ));
        if method.returns.contains("nil") {
            output.push_str(" `nil` means no value was available or the host operation failed.");
        } else if method.returns == "boolean" {
            output.push_str(" `false` means the operation was rejected or failed.");
        }
        output.push_str(&format!(
            "\n\n**Example call**\n\n```lua\nlocal result = {}\n```\n\n**Example response**\n\n```lua\n{}\n```\n",
            self.example_call(method),
            self.example_response(&method.returns)
        ));
    }

    fn signature(&self, method: &ApiMethod) -> String {
        let params = method
            .params
            .iter()
            .map(|param| {
                format!(
                    "{}{}: {}",
                    param.name,
                    if param.required { "" } else { "?" },
                    param.type_name
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}({params}) -> {}", method_path(method), method.returns)
    }

    fn example_call(&self, method: &ApiMethod) -> String {
        let arguments = method
            .params
            .iter()
            .map(example_argument)
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}({arguments})", method_path(method))
    }

    fn render_return_type(&self, returns: &str) -> String {
        let references = self
            .types
            .iter()
            .filter(|api_type| returns.contains(&api_type.name))
            .map(|api_type| {
                format!(
                    "[`{}`](TYPES.md#{})",
                    api_type.name,
                    api_type.name.to_lowercase()
                )
            })
            .collect::<Vec<_>>();
        if references.is_empty() {
            format!("`{returns}`.")
        } else {
            format!("`{returns}`. See {}.", references.join(", "))
        }
    }

    fn example_response(&self, returns: &str) -> String {
        self.example_response_named("result", returns)
    }

    fn example_response_named(&self, name: &str, returns: &str) -> String {
        let base = returns.split('|').next().unwrap_or(returns).trim();
        if let Some(element) = base.strip_suffix("[]") {
            if let Some(api_type) = self.types.iter().find(|item| item.name == element) {
                return format!("{{\n{}\n}}", indent(&self.example_for_type(api_type), 2));
            }
            return match element {
                "string" => "{ \"example\" }".to_owned(),
                _ => "{ { key = \"value\" } }".to_owned(),
            };
        }
        if let Some(api_type) = self.types.iter().find(|item| item.name == base) {
            return self.example_for_type(api_type);
        }
        example_value(name, base)
    }

    fn example_for_type(&self, api_type: &ApiType) -> String {
        let fields = api_type
            .fields
            .iter()
            .map(|(field, type_name)| {
                format!(
                    "  {field} = {},",
                    self.example_response_named(field, type_name.as_str().unwrap_or("any"))
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("{{\n{fields}\n}}")
    }
}

fn method_path(method: &ApiMethod) -> String {
    if method.namespace.is_empty() {
        format!("bds.{}", method.name)
    } else {
        format!("bds.{}.{}", method.namespace, method.name)
    }
}

fn bds2_compatibility() -> String {
    "bds2".to_owned()
}

fn example_argument(param: &ApiParameter) -> String {
    match param.type_name.as_str() {
        "table" => match param.name.as_str() {
            "credentials" => "{ host = \"example.com\", username = \"author\" }".to_owned(),
            "filters" => "{ status = \"draft\" }".to_owned(),
            "options" => "{ language = \"en\" }".to_owned(),
            "prefs" => "{ publish_drafts = false }".to_owned(),
            "source_tag_ids" => "{ \"tag-1\", \"tag-2\" }".to_owned(),
            "updates" => "{ description = \"Updated description\" }".to_owned(),
            "payload" => "{ current = 1, total = 10, message = \"Working\" }".to_owned(),
            _ => "{ title = \"Example\" }".to_owned(),
        },
        "number" => match param.name.as_str() {
            "total" => "100".to_owned(),
            _ => "1".to_owned(),
        },
        "boolean" => "true".to_owned(),
        type_name if type_name.contains("string") => example_string(&param.name),
        _ => "nil".to_owned(),
    }
}

fn example_value(name: &str, type_name: &str) -> String {
    let base = type_name.split('|').next().unwrap_or(type_name).trim();
    if base.ends_with("[]") {
        return if base == "string[]" {
            "{ \"example\" }".to_owned()
        } else {
            "{ { key = \"value\" } }".to_owned()
        };
    }
    match base {
        "nil" => "nil".to_owned(),
        "boolean" => "true".to_owned(),
        "integer" => "1".to_owned(),
        "number" => "0.5".to_owned(),
        "table" => "{ key = \"value\" }".to_owned(),
        value if value.contains("ISO-8601") => "\"2026-07-19T08:00:00Z\"".to_owned(),
        "string" => example_string(name),
        _ => "{ key = \"value\" }".to_owned(),
    }
}

fn example_string(name: &str) -> String {
    let value = match name {
        "action" => "new-post",
        "alt" => "A descriptive alternative",
        "caption" => "Example caption",
        "color" => "#336699",
        "content" | "text" => "Example content",
        "data_path" | "folder_path" | "item_path" | "source_path" | "file_path" => "/path/to/item",
        "default_author" => "Ada Author",
        "entrypoint" => "main",
        "kind" => "utility",
        "language" | "main_language" => "en",
        "message" => "Working",
        "mime_type" => "image/png",
        "name" | "new_name" => "Example",
        "original_name" => "image.png",
        "public_url" => "https://example.com",
        "query" => "rust",
        "size" => "small",
        "slug" | "post_template_slug" => "example-post",
        "status" => "draft",
        "title" => "Example post",
        value if value == "id" || value.ends_with("_id") => "example-id",
        _ => "example",
    };
    format!("\"{value}\"")
}

fn field_description(field: &str) -> &'static str {
    match field {
        "id" => "Stable record identifier.",
        "project_id" => "Identifier of the owning project.",
        "name" => "Human-readable name.",
        "title" => "Human-readable title.",
        "slug" => "URL-safe record identifier.",
        "description" => "Human-readable description.",
        "status" => "Current lifecycle state.",
        "progress" => "Completion value reported by the task.",
        "message" => "Latest user-facing task message.",
        "created_at" => "Creation timestamp.",
        "updated_at" => "Last-update timestamp.",
        "language" | "main_language" => "BCP 47 language code.",
        "blog_languages" => "Languages configured for the blog.",
        "categories" => "Assigned category names.",
        "tags" => "Assigned tag names.",
        "active_count" => "Number of active tasks.",
        "running_count" => "Number of currently running tasks.",
        "pending_count" => "Number of queued tasks.",
        "tasks" => "Tasks included in this status snapshot.",
        "errors" => "Validation error messages.",
        "valid" => "Whether validation succeeded.",
        "is_active" => "Whether this is the active project.",
        "enabled" => "Whether the record is enabled.",
        "data_path" => "Filesystem path containing project data.",
        "public_url" => "Published site base URL.",
        "default_author" => "Default post author name.",
        "publishing_preferences" => "Project publishing configuration.",
        "backlinks" => "Links from other posts to this post.",
        "links_to" => "Links from this post to other posts.",
        "original_name" => "Original imported filename.",
        "mime_type" => "Media MIME type.",
        "file_path" => "Stored media file path.",
        "alt" => "Alternative text for the media.",
        "caption" => "Media caption.",
        "kind" => "Script or template kind.",
        "entrypoint" => "Lua function invoked by the runtime.",
        "color" => "Optional display color.",
        "post_template_slug" => "Template selected for tagged posts.",
        _ => "Public value returned by the host API.",
    }
}

fn indent(value: &str, spaces: usize) -> String {
    let padding = " ".repeat(spaces);
    value
        .lines()
        .map(|line| format!("{padding}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

static API_MANIFEST: LazyLock<ApiManifest> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../../docs/scripting/api.json"))
        .expect("docs/scripting/api.json must be valid")
});

pub fn api_manifest() -> &'static ApiManifest {
    &API_MANIFEST
}
