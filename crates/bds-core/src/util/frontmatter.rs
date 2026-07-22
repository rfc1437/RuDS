use crate::model::{Post, PostTranslation};
use crate::util::timestamp::{iso_to_unix_ms, unix_ms_to_iso};
use regex::Regex;
use serde::{Deserialize, Deserializer};
use std::sync::LazyLock;

static SIMPLE_YAML_STRING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[\p{L}\p{N} ._/-]+$").expect("canonical frontmatter regex is valid")
});

fn scalar_string(value: serde_yaml::Value) -> Option<String> {
    match value {
        serde_yaml::Value::String(value) => Some(value),
        serde_yaml::Value::Number(value) => Some(value.to_string()),
        serde_yaml::Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn deserialize_scalar_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    scalar_string(serde_yaml::Value::deserialize(deserializer)?)
        .ok_or_else(|| serde::de::Error::custom("expected a scalar string"))
}

fn deserialize_optional_scalar_string<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<serde_yaml::Value>::deserialize(deserializer)?.and_then(scalar_string))
}

fn deserialize_optional_nonempty_scalar_string<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(deserialize_optional_scalar_string(deserializer)?.filter(|value| !value.is_empty()))
}

fn deserialize_string_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(match serde_yaml::Value::deserialize(deserializer)? {
        serde_yaml::Value::Sequence(values) => values
            .iter()
            .filter_map(|value| value.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    })
}

fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = deserialize_scalar_string(deserializer)?;
    iso_to_unix_ms(&value).map_err(serde::de::Error::custom)
}

fn deserialize_optional_timestamp<'de, D>(deserializer: D) -> Result<Option<i64>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(deserialize_optional_scalar_string(deserializer)?
        .and_then(|value| iso_to_unix_ms(&value).ok()))
}

fn deserialize_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<serde_yaml::Value>::deserialize(deserializer)?
        .and_then(scalar_string)
        .is_some_and(|value| value == "true"))
}

fn deserialize_bool_default_true<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<serde_yaml::Value>::deserialize(deserializer)?
        .and_then(scalar_string)
        .is_none_or(|value| value == "true"))
}

fn default_true() -> bool {
    true
}

fn default_version() -> i32 {
    1
}

fn default_entrypoint() -> String {
    "render".to_owned()
}

fn default_entrypoint_for_kind(kind: &str) -> String {
    if kind == "macro" { "render" } else { "main" }.to_owned()
}

fn deserialize_version<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<serde_yaml::Value>::deserialize(deserializer)?
        .and_then(scalar_string)
        .and_then(|value| value.parse().ok())
        .unwrap_or(1))
}

fn deserialize_entrypoint<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<serde_yaml::Value>::deserialize(deserializer)?
        .and_then(scalar_string)
        .unwrap_or_else(default_entrypoint))
}

/// Split content at `---` delimiters into (yaml, body).
/// Returns `None` if the content does not start with `---`.
pub fn split_frontmatter(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim_start_matches('\u{feff}'); // strip BOM
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_first = &trimmed[3..];
    let after_first = after_first.strip_prefix('\n').unwrap_or(after_first);
    let end = after_first.find("\n---")?;
    let yaml = &after_first[..end];
    let rest = &after_first[end + 4..]; // skip "\n---"
    let body = rest.strip_prefix('\n').unwrap_or(rest);
    Some((yaml, body))
}

/// Format frontmatter + body into a complete file string.
pub fn format_frontmatter(yaml: &str, body: &str) -> String {
    format!("---\n{yaml}\n---\n{}\n", body.trim_end_matches('\n'))
}

// --- Post Frontmatter ---

/// Parsed post frontmatter fields (camelCase for YAML compatibility).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostFrontmatter {
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub id: String,
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub title: String,
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub slug: String,
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub status: String,
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub created_at: i64,
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub updated_at: i64,
    #[serde(default, deserialize_with = "deserialize_string_list")]
    pub tags: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_list")]
    pub categories: Vec<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonempty_scalar_string"
    )]
    pub excerpt: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonempty_scalar_string"
    )]
    pub author: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonempty_scalar_string"
    )]
    pub language: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonempty_scalar_string"
    )]
    pub template_slug: Option<String>,
    #[serde(default, deserialize_with = "deserialize_bool")]
    pub do_not_translate: bool,
    #[serde(default, deserialize_with = "deserialize_optional_timestamp")]
    pub published_at: Option<i64>,
}

impl PostFrontmatter {
    /// Build from a Post model.
    pub fn from_post(post: &Post) -> Self {
        Self {
            id: post.id.clone(),
            title: post.title.clone(),
            slug: post.slug.clone(),
            status: post.status.as_str().to_owned(),
            created_at: post.created_at,
            updated_at: post.updated_at,
            tags: post.tags.clone(),
            categories: post.categories.clone(),
            excerpt: post.excerpt.clone(),
            author: post.author.clone(),
            language: post.language.clone(),
            template_slug: post.template_slug.clone(),
            do_not_translate: post.do_not_translate,
            published_at: post.published_at,
        }
    }

    /// Serialize using the canonical bDS2 frontmatter field and scalar rules.
    pub fn to_yaml(&self) -> String {
        let mut lines = Vec::new();
        push_yaml_string(&mut lines, "id", &self.id);
        push_yaml_string(&mut lines, "title", &self.title);
        push_yaml_string(&mut lines, "slug", &self.slug);
        if let Some(ref excerpt) = self.excerpt
            && !excerpt.is_empty()
        {
            push_yaml_string(&mut lines, "excerpt", excerpt);
        }
        push_yaml_string(&mut lines, "status", &self.status);
        if let Some(ref author) = self.author
            && !author.is_empty()
        {
            push_yaml_string(&mut lines, "author", author);
        }
        if let Some(ref language) = self.language
            && !language.is_empty()
        {
            push_yaml_string(&mut lines, "language", language);
        }
        if self.do_not_translate {
            lines.push("doNotTranslate: true".to_string());
        }
        if let Some(ref template_slug) = self.template_slug
            && !template_slug.is_empty()
        {
            push_yaml_string(&mut lines, "templateSlug", template_slug);
        }
        lines.push(format!("createdAt: '{}'", unix_ms_to_iso(self.created_at)));
        lines.push(format!("updatedAt: '{}'", unix_ms_to_iso(self.updated_at)));
        if let Some(published_at) = self.published_at {
            lines.push(format!("publishedAt: '{}'", unix_ms_to_iso(published_at)));
        }
        serialize_yaml_list(&mut lines, "tags", &self.tags);
        serialize_yaml_list(&mut lines, "categories", &self.categories);

        lines.join("\n")
    }

    /// Parse from a YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        serde_yaml::from_str(yaml).map_err(|error| format!("YAML parse error: {error}"))
    }
}

/// Write a complete post file (frontmatter + body).
pub fn write_post_file(post: &Post, body: &str) -> String {
    let fm = PostFrontmatter::from_post(post);
    format_frontmatter(&fm.to_yaml(), body)
}

/// Read a complete post file, returning frontmatter + body.
pub fn read_post_file(content: &str) -> Result<(PostFrontmatter, String), String> {
    let (yaml, body) = split_frontmatter(content).ok_or("no frontmatter delimiters found")?;
    let fm = PostFrontmatter::from_yaml(yaml)?;
    Ok((fm, body.trim_end_matches('\n').to_string()))
}

// --- Translation Frontmatter ---

/// Parsed translation frontmatter.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationFrontmatter {
    #[serde(default, deserialize_with = "deserialize_optional_scalar_string")]
    pub id: Option<String>,
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub translation_for: String,
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub language: String,
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub title: String,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_nonempty_scalar_string"
    )]
    pub excerpt: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_scalar_string")]
    pub status: Option<String>,
    #[serde(default, deserialize_with = "deserialize_optional_timestamp")]
    pub created_at: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_optional_timestamp")]
    pub updated_at: Option<i64>,
    #[serde(default, deserialize_with = "deserialize_optional_timestamp")]
    pub published_at: Option<i64>,
}

impl TranslationFrontmatter {
    pub fn from_translation(t: &PostTranslation) -> Self {
        Self {
            id: Some(t.id.clone()),
            translation_for: t.translation_for.clone(),
            language: t.language.clone(),
            title: t.title.clone(),
            excerpt: t.excerpt.clone(),
            status: Some(t.status.as_str().to_owned()),
            created_at: Some(t.created_at),
            updated_at: Some(t.updated_at),
            published_at: t.published_at,
        }
    }

    pub fn to_yaml(&self) -> String {
        let mut lines = Vec::new();
        if let Some(id) = &self.id {
            push_yaml_string(&mut lines, "id", id);
        }
        push_yaml_string(&mut lines, "translationFor", &self.translation_for);
        push_yaml_string(&mut lines, "language", &self.language);
        push_yaml_string(&mut lines, "title", &self.title);
        if let Some(ref excerpt) = self.excerpt
            && !excerpt.is_empty()
        {
            push_yaml_string(&mut lines, "excerpt", excerpt);
        }
        if let Some(status) = &self.status {
            push_yaml_string(&mut lines, "status", status);
        }
        if let Some(created_at) = self.created_at {
            lines.push(format!("createdAt: '{}'", unix_ms_to_iso(created_at)));
        }
        if let Some(updated_at) = self.updated_at {
            lines.push(format!("updatedAt: '{}'", unix_ms_to_iso(updated_at)));
        }
        if let Some(published_at) = self.published_at {
            lines.push(format!("publishedAt: '{}'", unix_ms_to_iso(published_at)));
        }
        lines.join("\n")
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        serde_yaml::from_str(yaml).map_err(|error| format!("YAML parse error: {error}"))
    }
}

/// Write a complete translation file (frontmatter + body).
pub fn write_translation_file(translation: &PostTranslation, body: &str) -> String {
    let fm = TranslationFrontmatter::from_translation(translation);
    format_frontmatter(&fm.to_yaml(), body)
}

/// Read a complete translation file.
pub fn read_translation_file(content: &str) -> Result<(TranslationFrontmatter, String), String> {
    let (yaml, body) = split_frontmatter(content).ok_or("no frontmatter delimiters found")?;
    let fm = TranslationFrontmatter::from_yaml(yaml)?;
    Ok((fm, body.trim_end_matches('\n').to_string()))
}

// --- Template Frontmatter ---

/// Parsed template frontmatter.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateFrontmatter {
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_optional_scalar_string")]
    pub project_id: Option<String>,
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub slug: String,
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub title: String,
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub kind: String,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_bool_default_true"
    )]
    pub enabled: bool,
    #[serde(default = "default_version", deserialize_with = "deserialize_version")]
    pub version: i32,
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub created_at: i64,
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub updated_at: i64,
}

impl TemplateFrontmatter {
    /// Serialize using the canonical bDS2 frontmatter scalar rules.
    pub fn to_yaml(&self) -> String {
        let mut lines = Vec::new();
        push_yaml_string(&mut lines, "id", &self.id);
        if let Some(ref pid) = self.project_id {
            push_yaml_string(&mut lines, "projectId", pid);
        }
        push_yaml_string(&mut lines, "slug", &self.slug);
        push_yaml_string(&mut lines, "title", &self.title);
        push_yaml_string(&mut lines, "kind", &self.kind);
        lines.push(format!("enabled: {}", self.enabled));
        lines.push(format!("version: {}", self.version));
        lines.push(format!("createdAt: '{}'", unix_ms_to_iso(self.created_at)));
        lines.push(format!("updatedAt: '{}'", unix_ms_to_iso(self.updated_at)));
        lines.join("\n")
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        serde_yaml::from_str(yaml).map_err(|error| format!("YAML parse error: {error}"))
    }
}

/// Read a template file (frontmatter + body).
pub fn read_template_file(content: &str) -> Result<(TemplateFrontmatter, String), String> {
    let (yaml, body) = split_frontmatter(content).ok_or("no frontmatter delimiters found")?;
    let fm = TemplateFrontmatter::from_yaml(yaml)?;
    Ok((fm, body.trim_end_matches('\n').to_string()))
}

/// Write a template file (frontmatter + body).
pub fn write_template_file(fm: &TemplateFrontmatter, body: &str) -> String {
    format_frontmatter(&fm.to_yaml(), body)
}

// --- Script Frontmatter ---

/// Parsed script frontmatter (template fields plus entrypoint).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptFrontmatter {
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub id: String,
    #[serde(default, deserialize_with = "deserialize_optional_scalar_string")]
    pub project_id: Option<String>,
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub slug: String,
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub title: String,
    #[serde(deserialize_with = "deserialize_scalar_string")]
    pub kind: String,
    #[serde(
        default = "default_entrypoint",
        deserialize_with = "deserialize_entrypoint"
    )]
    pub entrypoint: String,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_bool_default_true"
    )]
    pub enabled: bool,
    #[serde(default = "default_version", deserialize_with = "deserialize_version")]
    pub version: i32,
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub created_at: i64,
    #[serde(deserialize_with = "deserialize_timestamp")]
    pub updated_at: i64,
}

impl ScriptFrontmatter {
    /// Serialize using the canonical bDS2 frontmatter scalar rules.
    pub fn to_yaml(&self) -> String {
        let mut lines = Vec::new();
        push_yaml_string(&mut lines, "id", &self.id);
        if let Some(ref pid) = self.project_id {
            push_yaml_string(&mut lines, "projectId", pid);
        }
        push_yaml_string(&mut lines, "slug", &self.slug);
        push_yaml_string(&mut lines, "title", &self.title);
        push_yaml_string(&mut lines, "kind", &self.kind);
        push_yaml_string(&mut lines, "entrypoint", &self.entrypoint);
        lines.push(format!("enabled: {}", self.enabled));
        lines.push(format!("version: {}", self.version));
        lines.push(format!("createdAt: '{}'", unix_ms_to_iso(self.created_at)));
        lines.push(format!("updatedAt: '{}'", unix_ms_to_iso(self.updated_at)));
        lines.join("\n")
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        let value: serde_yaml::Value =
            serde_yaml::from_str(yaml).map_err(|error| format!("YAML parse error: {error}"))?;
        let entrypoint_key = serde_yaml::Value::String("entrypoint".to_owned());
        let entrypoint_is_missing = value
            .as_mapping()
            .is_none_or(|mapping| !mapping.contains_key(&entrypoint_key));
        let mut frontmatter: Self =
            serde_yaml::from_value(value).map_err(|error| format!("YAML parse error: {error}"))?;
        if entrypoint_is_missing {
            frontmatter.entrypoint = default_entrypoint_for_kind(&frontmatter.kind);
        }
        Ok(frontmatter)
    }
}

/// Read a Lua script file with standard YAML frontmatter.
pub fn read_script_file(content: &str) -> Result<(ScriptFrontmatter, String), String> {
    let (yaml, body) = split_frontmatter(content).ok_or("no frontmatter delimiters found")?;
    let fm = ScriptFrontmatter::from_yaml(yaml)?;
    Ok((fm, body.trim_end_matches('\n').to_string()))
}

/// Write a script file (always Lua format with --- delimiters).
pub fn write_script_file(fm: &ScriptFrontmatter, body: &str) -> String {
    format_frontmatter(&fm.to_yaml(), body)
}

// --- Helpers ---

fn yaml_string_value(s: &str) -> String {
    if SIMPLE_YAML_STRING.is_match(s) {
        s.to_string()
    } else {
        format!(
            "\"{}\"",
            s.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
        )
    }
}

fn serialize_yaml_list(lines: &mut Vec<String>, key: &str, values: &[String]) {
    lines.push(format!("{key}:"));
    lines.extend(
        values
            .iter()
            .map(|value| format!("  - {}", yaml_string_value(value))),
    );
}

fn push_yaml_string(lines: &mut Vec<String>, key: &str, value: &str) {
    if !value.is_empty() {
        lines.push(format!("{key}: {}", yaml_string_value(value)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::PostStatus;
    use std::fs;
    use std::path::PathBuf;

    fn fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/compatibility-projects/rfc1437-sample")
    }

    #[test]
    fn split_basic() {
        let input = "---\nfoo: bar\n---\nbody text\n";
        let (yaml, body) = split_frontmatter(input).unwrap();
        assert_eq!(yaml, "foo: bar");
        assert_eq!(body, "body text\n");
    }

    #[test]
    fn split_no_frontmatter() {
        assert!(split_frontmatter("no frontmatter here").is_none());
    }

    #[test]
    fn derived_deserialization_preserves_lenient_scalar_coercion() {
        let yaml = "id: 42\ntitle: true\nslug: 7\nstatus: false\ncreatedAt: '2026-01-02T03:04:05.000Z'\nupdatedAt: '2026-01-02T03:04:05.000Z'\ntags: [rust, 2]\ncategories: invalid\nauthor: 9\ndoNotTranslate: true";
        let parsed = PostFrontmatter::from_yaml(yaml).unwrap();

        assert_eq!(parsed.id, "42");
        assert_eq!(parsed.title, "true");
        assert_eq!(parsed.slug, "7");
        assert_eq!(parsed.status, "false");
        assert_eq!(parsed.tags, vec!["rust"]);
        assert!(parsed.categories.is_empty());
        assert_eq!(parsed.author.as_deref(), Some("9"));
        assert!(parsed.do_not_translate);
    }

    #[test]
    fn translation_frontmatter_accepts_canonical_unquoted_scalars() {
        let yaml = "translationFor: 42\nlanguage: en\ntitle: true";
        let parsed = TranslationFrontmatter::from_yaml(yaml).unwrap();
        assert_eq!(parsed.translation_for, "42");
        assert_eq!(parsed.language, "en");
        assert_eq!(parsed.title, "true");
    }

    #[test]
    fn parse_esmeralda() {
        let path = fixture_dir().join("posts/2005/11/esmeralda.md");
        let content = fs::read_to_string(path).unwrap();
        let (fm, body) = read_post_file(&content).unwrap();
        assert_eq!(fm.id, "40a83ab1-423d-4310-aac4-642d84675007");
        assert_eq!(fm.title, "Esmeralda");
        assert_eq!(fm.slug, "esmeralda");
        assert_eq!(fm.status, "published");
        assert_eq!(
            fm.tags,
            vec!["fotografie", "makro", "natur", "spinne", "tiere"]
        );
        assert_eq!(fm.categories, vec!["picture"]);
        assert_eq!(fm.language.as_deref(), Some("es"));
        assert_eq!(fm.published_at, Some(1131883200000));
        assert_eq!(fm.created_at, 1131883200000);
        assert!(body.contains("Esmeralda"));
    }

    #[test]
    fn parse_ghostty() {
        let path = fixture_dir().join("posts/2026/03/ghostty.md");
        let content = fs::read_to_string(path).unwrap();
        let (fm, _body) = read_post_file(&content).unwrap();
        assert_eq!(fm.id, "6745981d-da41-4cfd-80ec-95ad339acf6f");
        assert_eq!(fm.title, "Ghostty");
        assert_eq!(fm.slug, "ghostty");
        assert_eq!(fm.tags, vec!["programmierung", "sysadmin", "mac-os-x"]);
        assert_eq!(fm.categories, vec!["aside"]);
        assert!(fm.language.is_none());
    }

    #[test]
    fn parse_cmux() {
        let path = fixture_dir().join("posts/2026/03/cmux-das-terminal-fur-multitasking.md");
        let content = fs::read_to_string(path).unwrap();
        let (fm, _body) = read_post_file(&content).unwrap();
        assert_eq!(fm.slug, "cmux-das-terminal-fur-multitasking");
        assert!(fm.title.contains("cmux"));
    }

    #[test]
    fn parse_esmeralda_en_translation() {
        let path = fixture_dir().join("posts/2005/11/esmeralda.en.md");
        let content = fs::read_to_string(path).unwrap();
        let (fm, body) = read_translation_file(&content).unwrap();
        assert_eq!(fm.translation_for, "40a83ab1-423d-4310-aac4-642d84675007");
        assert_eq!(fm.language, "en");
        assert_eq!(fm.title, "Esmeralda");
        assert!(body.contains("Esmeralda"));
    }

    #[test]
    fn parse_esmeralda_de_translation() {
        let path = fixture_dir().join("posts/2005/11/esmeralda.de.md");
        let content = fs::read_to_string(path).unwrap();
        let (fm, _body) = read_translation_file(&content).unwrap();
        assert_eq!(fm.language, "de");
    }

    #[test]
    fn roundtrip_post_frontmatter() {
        let path = fixture_dir().join("posts/2005/11/esmeralda.md");
        let content = fs::read_to_string(path).unwrap();
        let (fm, body) = read_post_file(&content).unwrap();

        // Write back out
        let yaml = fm.to_yaml();
        let output = format_frontmatter(&yaml, &body);

        // Parse again
        let (fm2, body2) = read_post_file(&output).unwrap();
        assert_eq!(fm.id, fm2.id);
        assert_eq!(fm.title, fm2.title);
        assert_eq!(fm.slug, fm2.slug);
        assert_eq!(fm.status, fm2.status);
        assert_eq!(fm.created_at, fm2.created_at);
        assert_eq!(fm.updated_at, fm2.updated_at);
        assert_eq!(fm.tags, fm2.tags);
        assert_eq!(fm.categories, fm2.categories);
        assert_eq!(fm.language, fm2.language);
        assert_eq!(fm.published_at, fm2.published_at);
        assert_eq!(body, body2);
    }

    #[test]
    fn current_translation_output_carries_full_metadata() {
        let translation = PostTranslation {
            id: "translation-1".into(),
            project_id: "project-1".into(),
            translation_for: "post-1".into(),
            language: "de".into(),
            title: "Titel".into(),
            excerpt: None,
            content: None,
            status: PostStatus::Published,
            file_path: "posts/2026/07/post.de.md".into(),
            checksum: None,
            created_at: 1_751_328_000_000,
            updated_at: 1_751_414_400_000,
            published_at: Some(1_751_414_400_000),
        };

        let output = write_translation_file(&translation, "Inhalt");
        assert!(output.contains("id: translation-1"));
        assert!(output.contains("status: published"));
        assert!(output.contains("createdAt:"));
        assert!(output.contains("updatedAt:"));
        assert!(output.contains("publishedAt:"));
    }

    #[test]
    fn translation_output_matches_bds2_quoting_and_trailing_newline() {
        let fm = TranslationFrontmatter {
            id: Some("translation-1".into()),
            translation_for: "post-1".into(),
            language: "de".into(),
            title: "true".into(),
            excerpt: Some("Summary: translated".into()),
            status: Some("published".into()),
            created_at: Some(1_711_833_600_000),
            updated_at: Some(1_711_920_000_000),
            published_at: Some(1_712_006_400_000),
        };
        assert_eq!(
            format_frontmatter(&fm.to_yaml(), "body\n\n"),
            concat!(
                "---\n",
                "id: translation-1\n",
                "translationFor: post-1\n",
                "language: de\n",
                "title: true\n",
                "excerpt: \"Summary: translated\"\n",
                "status: published\n",
                "createdAt: '2024-03-30T21:20:00.000Z'\n",
                "updatedAt: '2024-03-31T21:20:00.000Z'\n",
                "publishedAt: '2024-04-01T21:20:00.000Z'\n",
                "---\n",
                "body\n",
            )
        );
        let parsed = TranslationFrontmatter::from_yaml(&fm.to_yaml()).unwrap();
        assert_eq!(parsed.title, "true");
    }

    #[test]
    fn yaml_quoting() {
        assert_eq!(yaml_string_value("simple"), "simple");
        assert_eq!(yaml_string_value("Über Öl/123"), "Über Öl/123");
        assert_eq!(yaml_string_value("has: colon"), "\"has: colon\"");
        assert_eq!(yaml_string_value("true"), "true");
        assert_eq!(
            yaml_string_value("say \"hi\"\nnext"),
            "\"say \\\"hi\\\"\\nnext\""
        );
        assert_eq!(yaml_string_value(""), "\"\"");
    }

    #[test]
    fn post_output_matches_bds2_canonical_bytes() {
        let fm = PostFrontmatter {
            id: "post-1".into(),
            title: "Published: \"Post\"".into(),
            slug: "published-post".into(),
            excerpt: Some("Summary".into()),
            status: "published".into(),
            author: Some("Writer".into()),
            language: Some("en".into()),
            do_not_translate: true,
            template_slug: Some("article".into()),
            created_at: 1_711_833_600_000,
            updated_at: 1_711_920_000_000,
            published_at: Some(1_712_006_400_000),
            tags: Vec::new(),
            categories: vec!["notes".into()],
        };

        assert_eq!(
            format_frontmatter(&fm.to_yaml(), "Hello from markdown\n\n"),
            concat!(
                "---\n",
                "id: post-1\n",
                "title: \"Published: \\\"Post\\\"\"\n",
                "slug: published-post\n",
                "excerpt: Summary\n",
                "status: published\n",
                "author: Writer\n",
                "language: en\n",
                "doNotTranslate: true\n",
                "templateSlug: article\n",
                "createdAt: '2024-03-30T21:20:00.000Z'\n",
                "updatedAt: '2024-03-31T21:20:00.000Z'\n",
                "publishedAt: '2024-04-01T21:20:00.000Z'\n",
                "tags:\n",
                "categories:\n",
                "  - notes\n",
                "---\n",
                "Hello from markdown\n",
            )
        );
    }

    #[test]
    fn template_and_script_output_match_bds2_scalar_and_timestamp_rules() {
        let template = TemplateFrontmatter {
            id: "template-1".into(),
            project_id: Some("project-1".into()),
            slug: "article".into(),
            title: "Article: Wide".into(),
            kind: "post".into(),
            enabled: true,
            version: 2,
            created_at: 1_711_833_600_000,
            updated_at: 1_711_920_000_000,
        };
        assert_eq!(
            write_template_file(&template, "body\n\n"),
            concat!(
                "---\n",
                "id: template-1\n",
                "projectId: project-1\n",
                "slug: article\n",
                "title: \"Article: Wide\"\n",
                "kind: post\n",
                "enabled: true\n",
                "version: 2\n",
                "createdAt: '2024-03-30T21:20:00.000Z'\n",
                "updatedAt: '2024-03-31T21:20:00.000Z'\n",
                "---\n",
                "body\n",
            )
        );

        let script = ScriptFrontmatter {
            id: "script-1".into(),
            project_id: Some("project-1".into()),
            slug: "render-card".into(),
            title: "Render Card".into(),
            kind: "macro".into(),
            entrypoint: "render".into(),
            enabled: true,
            version: 3,
            created_at: 1_711_833_600_000,
            updated_at: 1_711_920_000_000,
        };
        assert_eq!(
            script.to_yaml(),
            concat!(
                "id: script-1\n",
                "projectId: project-1\n",
                "slug: render-card\n",
                "title: Render Card\n",
                "kind: macro\n",
                "entrypoint: render\n",
                "enabled: true\n",
                "version: 3\n",
                "createdAt: '2024-03-30T21:20:00.000Z'\n",
                "updatedAt: '2024-03-31T21:20:00.000Z'",
            )
        );
    }

    #[test]
    fn conditional_fields_omitted_when_empty() {
        let fm = PostFrontmatter {
            id: "test-id".into(),
            title: "Test".into(),
            slug: "test".into(),
            status: "draft".into(),
            created_at: 1131883200000,
            updated_at: 1131883200000,
            tags: vec![],
            categories: vec![],
            excerpt: None,
            author: None,
            language: None,
            template_slug: None,
            do_not_translate: false,
            published_at: None,
        };
        let yaml = fm.to_yaml();
        assert!(!yaml.contains("excerpt"));
        assert!(!yaml.contains("author"));
        assert!(!yaml.contains("language"));
        assert!(!yaml.contains("doNotTranslate"));
        assert!(!yaml.contains("templateSlug"));
        assert!(!yaml.contains("publishedAt"));
    }

    #[test]
    fn do_not_translate_written_when_true() {
        let fm = PostFrontmatter {
            id: "test-id".into(),
            title: "Test".into(),
            slug: "test".into(),
            status: "draft".into(),
            created_at: 1131883200000,
            updated_at: 1131883200000,
            tags: vec![],
            categories: vec![],
            excerpt: None,
            author: None,
            language: None,
            template_slug: None,
            do_not_translate: true,
            published_at: None,
        };
        let yaml = fm.to_yaml();
        assert!(yaml.contains("doNotTranslate: true"));
    }

    // --- Template frontmatter tests ---

    #[test]
    fn parse_fixture_template() {
        let path = fixture_dir().join("templates/testvorlage.liquid");
        let content = fs::read_to_string(path).unwrap();
        let (fm, body) = read_template_file(&content).unwrap();
        assert_eq!(fm.id, "38704737-b7e7-4dd4-b010-9208bcf80ef6");
        assert_eq!(
            fm.project_id.as_deref(),
            Some("1979237c-034d-41f6-99a0-f35eb57b3f6c")
        );
        assert_eq!(fm.slug, "testvorlage");
        assert_eq!(fm.title, "Testvorlage");
        assert_eq!(fm.kind, "post");
        assert!(fm.enabled);
        assert_eq!(fm.version, 3);
        assert!(body.contains("<div>"));
    }

    #[test]
    fn template_frontmatter_roundtrip() {
        let fm = TemplateFrontmatter {
            id: "test-uuid".into(),
            project_id: Some("proj-uuid".into()),
            slug: "my-template".into(),
            title: "My Template".into(),
            kind: "post".into(),
            enabled: true,
            version: 1,
            created_at: 1131883200000,
            updated_at: 1131883200000,
        };
        let yaml = fm.to_yaml();
        let parsed = TemplateFrontmatter::from_yaml(&yaml).unwrap();
        assert_eq!(parsed.id, fm.id);
        assert_eq!(parsed.slug, fm.slug);
        assert_eq!(parsed.kind, fm.kind);
        assert_eq!(parsed.version, fm.version);
    }

    #[test]
    fn script_frontmatter_roundtrip() {
        let fm = ScriptFrontmatter {
            id: "test-uuid".into(),
            project_id: None,
            slug: "my-script".into(),
            title: "My Script".into(),
            kind: "utility".into(),
            entrypoint: "main".into(),
            enabled: true,
            version: 2,
            created_at: 1131883200000,
            updated_at: 1131883200000,
        };
        let yaml = fm.to_yaml();
        let parsed = ScriptFrontmatter::from_yaml(&yaml).unwrap();
        assert_eq!(parsed.id, fm.id);
        assert_eq!(parsed.entrypoint, "main");
        assert_eq!(parsed.version, 2);
    }
}
