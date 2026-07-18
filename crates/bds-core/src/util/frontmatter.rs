use crate::model::{Post, PostTranslation};
use crate::util::timestamp::{iso_to_unix_ms, unix_ms_to_iso};

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

/// Split content at Python docstring `"""` delimiters (for legacy .py scripts).
/// The YAML frontmatter is inside: `"""\n---\n{yaml}\n---\n"""`
pub fn split_docstring_frontmatter(input: &str) -> Option<(&str, &str)> {
    let trimmed = input.trim_start_matches('\u{feff}');
    if !trimmed.starts_with("\"\"\"") {
        return None;
    }
    let after_open = &trimmed[3..];
    let after_open = after_open.strip_prefix('\n').unwrap_or(after_open);
    // Find closing """
    let close_pos = after_open.find("\"\"\"")?;
    let inside = &after_open[..close_pos].trim();
    // Inside should be ---\n{yaml}\n---
    let yaml_content = split_frontmatter(inside)?;
    let body_start = close_pos + 3;
    let rest = &after_open[body_start..];
    let body = rest.strip_prefix('\n').unwrap_or(rest);
    Some((yaml_content.0, body))
}

/// Format frontmatter + body into a complete file string.
pub fn format_frontmatter(yaml: &str, body: &str) -> String {
    format!("---\n{yaml}\n---\n{body}")
}

// --- Post Frontmatter ---

/// Parsed post frontmatter fields (camelCase for YAML compatibility).
#[derive(Debug, Clone)]
pub struct PostFrontmatter {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    pub excerpt: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub template_slug: Option<String>,
    pub do_not_translate: bool,
    pub published_at: Option<i64>,
}

impl PostFrontmatter {
    /// Build from a Post model.
    pub fn from_post(post: &Post) -> Self {
        Self {
            id: post.id.clone(),
            title: post.title.clone(),
            slug: post.slug.clone(),
            status: serde_json::to_string(&post.status)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string(),
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

    /// Serialize to YAML string (matching TypeScript gray-matter output).
    pub fn to_yaml(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("id: {}", self.id));
        lines.push(format!("title: {}", yaml_string_value(&self.title)));
        lines.push(format!("slug: {}", self.slug));
        lines.push(format!("status: {}", self.status));
        lines.push(format!("createdAt: '{}'", unix_ms_to_iso(self.created_at)));
        lines.push(format!("updatedAt: '{}'", unix_ms_to_iso(self.updated_at)));

        // Tags as YAML list
        if self.tags.is_empty() {
            lines.push("tags: []".to_string());
        } else {
            lines.push("tags:".to_string());
            for tag in &self.tags {
                lines.push(format!("  - {}", yaml_string_value(tag)));
            }
        }

        // Categories as YAML list
        if self.categories.is_empty() {
            lines.push("categories: []".to_string());
        } else {
            lines.push("categories:".to_string());
            for cat in &self.categories {
                lines.push(format!("  - {}", yaml_string_value(cat)));
            }
        }

        // Conditional fields (only when truthy)
        if let Some(ref excerpt) = self.excerpt
            && !excerpt.is_empty()
        {
            lines.push(format!("excerpt: {}", yaml_string_value(excerpt)));
        }
        if let Some(ref author) = self.author
            && !author.is_empty()
        {
            lines.push(format!("author: {}", yaml_string_value(author)));
        }
        if let Some(ref language) = self.language
            && !language.is_empty()
        {
            lines.push(format!("language: {language}"));
        }
        if self.do_not_translate {
            lines.push("doNotTranslate: true".to_string());
        }
        if let Some(ref template_slug) = self.template_slug
            && !template_slug.is_empty()
        {
            lines.push(format!("templateSlug: {template_slug}"));
        }
        if let Some(published_at) = self.published_at {
            lines.push(format!("publishedAt: '{}'", unix_ms_to_iso(published_at)));
        }

        lines.join("\n")
    }

    /// Parse from a YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        let doc: serde_yaml::Value =
            serde_yaml::from_str(yaml).map_err(|e| format!("YAML parse error: {e}"))?;
        let map = doc
            .as_mapping()
            .ok_or("frontmatter is not a YAML mapping")?;

        let get_str = |key: &str| -> Option<String> {
            map.get(serde_yaml::Value::String(key.to_string()))
                .and_then(|v| match v {
                    serde_yaml::Value::String(s) => Some(s.clone()),
                    serde_yaml::Value::Number(n) => Some(n.to_string()),
                    serde_yaml::Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                })
        };

        let get_string_list = |key: &str| -> Vec<String> {
            map.get(serde_yaml::Value::String(key.to_string()))
                .and_then(|v| v.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default()
        };

        let get_timestamp = |key: &str| -> Result<i64, String> {
            let s = get_str(key).ok_or(format!("missing required field '{key}'"))?;
            iso_to_unix_ms(&s)
        };

        Ok(Self {
            id: get_str("id").ok_or("missing 'id'")?,
            title: get_str("title").ok_or("missing 'title'")?,
            slug: get_str("slug").ok_or("missing 'slug'")?,
            status: get_str("status").ok_or("missing 'status'")?,
            created_at: get_timestamp("createdAt")?,
            updated_at: get_timestamp("updatedAt")?,
            tags: get_string_list("tags"),
            categories: get_string_list("categories"),
            excerpt: get_str("excerpt").filter(|s| !s.is_empty()),
            author: get_str("author").filter(|s| !s.is_empty()),
            language: get_str("language").filter(|s| !s.is_empty()),
            template_slug: get_str("templateSlug").filter(|s| !s.is_empty()),
            do_not_translate: get_str("doNotTranslate")
                .map(|s| s == "true")
                .unwrap_or(false),
            published_at: get_str("publishedAt").and_then(|s| iso_to_unix_ms(&s).ok()),
        })
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
    Ok((fm, body.to_string()))
}

// --- Translation Frontmatter ---

/// Parsed translation frontmatter.
#[derive(Debug, Clone)]
pub struct TranslationFrontmatter {
    pub id: Option<String>,
    pub translation_for: String,
    pub language: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub status: Option<String>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
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
            status: Some(
                serde_json::to_string(&t.status)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_string(),
            ),
            created_at: Some(t.created_at),
            updated_at: Some(t.updated_at),
            published_at: t.published_at,
        }
    }

    pub fn to_yaml(&self) -> String {
        let mut lines = Vec::new();
        if let Some(id) = &self.id {
            lines.push(format!("id: {id}"));
        }
        lines.push(format!("translationFor: {}", self.translation_for));
        lines.push(format!("language: {}", self.language));
        lines.push(format!("title: {}", yaml_string_value(&self.title)));
        if let Some(ref excerpt) = self.excerpt
            && !excerpt.is_empty()
        {
            lines.push(format!("excerpt: {}", yaml_string_value(excerpt)));
        }
        if let Some(status) = &self.status {
            lines.push(format!("status: {status}"));
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
        let doc: serde_yaml::Value =
            serde_yaml::from_str(yaml).map_err(|e| format!("YAML parse error: {e}"))?;
        let map = doc
            .as_mapping()
            .ok_or("frontmatter is not a YAML mapping")?;

        let get_str = |key: &str| -> Option<String> {
            map.get(serde_yaml::Value::String(key.to_string()))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        };

        Ok(Self {
            id: get_str("id"),
            translation_for: get_str("translationFor").ok_or("missing 'translationFor'")?,
            language: get_str("language").ok_or("missing 'language'")?,
            title: get_str("title").ok_or("missing 'title'")?,
            excerpt: get_str("excerpt").filter(|s| !s.is_empty()),
            status: get_str("status"),
            created_at: get_str("createdAt").and_then(|value| iso_to_unix_ms(&value).ok()),
            updated_at: get_str("updatedAt").and_then(|value| iso_to_unix_ms(&value).ok()),
            published_at: get_str("publishedAt").and_then(|value| iso_to_unix_ms(&value).ok()),
        })
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
    Ok((fm, body.to_string()))
}

// --- Template Frontmatter ---

/// Parsed template frontmatter (double-quoted strings, matching TypeScript output).
#[derive(Debug, Clone)]
pub struct TemplateFrontmatter {
    pub id: String,
    pub project_id: Option<String>,
    pub slug: String,
    pub title: String,
    pub kind: String,
    pub enabled: bool,
    pub version: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

impl TemplateFrontmatter {
    /// Serialize to YAML with double-quoted strings (matching TypeScript).
    pub fn to_yaml(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("id: \"{}\"", self.id));
        if let Some(ref pid) = self.project_id {
            lines.push(format!("projectId: \"{pid}\""));
        }
        lines.push(format!("slug: \"{}\"", self.slug));
        lines.push(format!("title: \"{}\"", self.title));
        lines.push(format!("kind: \"{}\"", self.kind));
        lines.push(format!("enabled: {}", self.enabled));
        lines.push(format!("version: {}", self.version));
        lines.push(format!(
            "createdAt: \"{}\"",
            unix_ms_to_iso(self.created_at)
        ));
        lines.push(format!(
            "updatedAt: \"{}\"",
            unix_ms_to_iso(self.updated_at)
        ));
        lines.join("\n")
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        let doc: serde_yaml::Value =
            serde_yaml::from_str(yaml).map_err(|e| format!("YAML parse error: {e}"))?;
        let map = doc.as_mapping().ok_or("not a YAML mapping")?;

        let get_str = |key: &str| -> Option<String> {
            map.get(serde_yaml::Value::String(key.to_string()))
                .and_then(|v| match v {
                    serde_yaml::Value::String(s) => Some(s.clone()),
                    serde_yaml::Value::Number(n) => Some(n.to_string()),
                    serde_yaml::Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                })
        };

        Ok(Self {
            id: get_str("id").ok_or("missing 'id'")?,
            project_id: get_str("projectId"),
            slug: get_str("slug").ok_or("missing 'slug'")?,
            title: get_str("title").ok_or("missing 'title'")?,
            kind: get_str("kind").ok_or("missing 'kind'")?,
            enabled: get_str("enabled").map(|s| s == "true").unwrap_or(true),
            version: get_str("version")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(1),
            created_at: get_str("createdAt")
                .and_then(|s| iso_to_unix_ms(&s).ok())
                .ok_or("missing 'createdAt'")?,
            updated_at: get_str("updatedAt")
                .and_then(|s| iso_to_unix_ms(&s).ok())
                .ok_or("missing 'updatedAt'")?,
        })
    }
}

/// Read a template file (frontmatter + body).
pub fn read_template_file(content: &str) -> Result<(TemplateFrontmatter, String), String> {
    let (yaml, body) = split_frontmatter(content).ok_or("no frontmatter delimiters found")?;
    let fm = TemplateFrontmatter::from_yaml(yaml)?;
    Ok((fm, body.to_string()))
}

/// Write a template file (frontmatter + body).
pub fn write_template_file(fm: &TemplateFrontmatter, body: &str) -> String {
    format_frontmatter(&fm.to_yaml(), body)
}

// --- Script Frontmatter ---

/// Parsed script frontmatter (double-quoted strings like templates, plus entrypoint).
#[derive(Debug, Clone)]
pub struct ScriptFrontmatter {
    pub id: String,
    pub project_id: Option<String>,
    pub slug: String,
    pub title: String,
    pub kind: String,
    pub entrypoint: String,
    pub enabled: bool,
    pub version: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ScriptFrontmatter {
    /// Serialize to YAML with double-quoted strings.
    pub fn to_yaml(&self) -> String {
        let mut lines = Vec::new();
        lines.push(format!("id: \"{}\"", self.id));
        if let Some(ref pid) = self.project_id {
            lines.push(format!("projectId: \"{pid}\""));
        }
        lines.push(format!("slug: \"{}\"", self.slug));
        lines.push(format!("title: \"{}\"", self.title));
        lines.push(format!("kind: \"{}\"", self.kind));
        lines.push(format!("entrypoint: \"{}\"", self.entrypoint));
        lines.push(format!("enabled: {}", self.enabled));
        lines.push(format!("version: {}", self.version));
        lines.push(format!(
            "createdAt: \"{}\"",
            unix_ms_to_iso(self.created_at)
        ));
        lines.push(format!(
            "updatedAt: \"{}\"",
            unix_ms_to_iso(self.updated_at)
        ));
        lines.join("\n")
    }

    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        let doc: serde_yaml::Value =
            serde_yaml::from_str(yaml).map_err(|e| format!("YAML parse error: {e}"))?;
        let map = doc.as_mapping().ok_or("not a YAML mapping")?;

        let get_str = |key: &str| -> Option<String> {
            map.get(serde_yaml::Value::String(key.to_string()))
                .and_then(|v| match v {
                    serde_yaml::Value::String(s) => Some(s.clone()),
                    serde_yaml::Value::Number(n) => Some(n.to_string()),
                    serde_yaml::Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                })
        };

        Ok(Self {
            id: get_str("id").ok_or("missing 'id'")?,
            project_id: get_str("projectId"),
            slug: get_str("slug").ok_or("missing 'slug'")?,
            title: get_str("title").ok_or("missing 'title'")?,
            kind: get_str("kind").ok_or("missing 'kind'")?,
            entrypoint: get_str("entrypoint").unwrap_or_else(|| "render".to_string()),
            enabled: get_str("enabled").map(|s| s == "true").unwrap_or(true),
            version: get_str("version")
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(1),
            created_at: get_str("createdAt")
                .and_then(|s| iso_to_unix_ms(&s).ok())
                .ok_or("missing 'createdAt'")?,
            updated_at: get_str("updatedAt")
                .and_then(|s| iso_to_unix_ms(&s).ok())
                .ok_or("missing 'updatedAt'")?,
        })
    }
}

/// Read a script file. Supports both `---` (Lua) and `"""` (Python) delimiters.
pub fn read_script_file(content: &str) -> Result<(ScriptFrontmatter, String), String> {
    // Try docstring format first (Python scripts)
    if let Some((yaml, body)) = split_docstring_frontmatter(content) {
        let fm = ScriptFrontmatter::from_yaml(yaml)?;
        return Ok((fm, body.to_string()));
    }
    // Fall back to standard --- format (Lua scripts)
    let (yaml, body) = split_frontmatter(content).ok_or("no frontmatter delimiters found")?;
    let fm = ScriptFrontmatter::from_yaml(yaml)?;
    Ok((fm, body.to_string()))
}

/// Write a script file (always Lua format with --- delimiters).
pub fn write_script_file(fm: &ScriptFrontmatter, body: &str) -> String {
    format_frontmatter(&fm.to_yaml(), body)
}

// --- Helpers ---

/// Quote a YAML string value if it contains special characters.
/// Simple values (alphanumeric, hyphens, dots) are left unquoted.
/// Values with colons, quotes, special chars are single-quoted.
fn yaml_string_value(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    // Check if the value needs quoting
    let needs_quoting = s.contains(':')
        || s.contains('#')
        || s.contains('\'')
        || s.contains('"')
        || s.contains('\n')
        || s.contains('{')
        || s.contains('}')
        || s.contains('[')
        || s.contains(']')
        || s.contains(',')
        || s.contains('&')
        || s.contains('*')
        || s.contains('!')
        || s.contains('|')
        || s.contains('>')
        || s.contains('%')
        || s.contains('@')
        || s.contains('`')
        || s.starts_with(' ')
        || s.ends_with(' ')
        || s.starts_with('-')
        || s.starts_with('?')
        || s == "true"
        || s == "false"
        || s == "null"
        || s == "yes"
        || s == "no"
        || s == "on"
        || s == "off";

    if needs_quoting {
        // Use single quotes, escaping internal single quotes by doubling them
        let escaped = s.replace('\'', "''");
        format!("'{escaped}'")
    } else {
        s.to_string()
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
    fn split_docstring() {
        let input = "\"\"\"\n---\nfoo: bar\n---\n\"\"\"\nbody\n";
        let (yaml, body) = split_docstring_frontmatter(input).unwrap();
        assert_eq!(yaml, "foo: bar");
        assert_eq!(body, "body\n");
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
    fn golden_output_esmeralda() {
        let path = fixture_dir().join("posts/2005/11/esmeralda.md");
        let expected = fs::read_to_string(&path).unwrap();
        let (fm, body) = read_post_file(&expected).unwrap();

        let yaml = fm.to_yaml();
        let actual = format_frontmatter(&yaml, &body);
        assert_eq!(actual, expected, "golden output mismatch for esmeralda.md");
    }

    #[test]
    fn golden_output_ghostty() {
        let path = fixture_dir().join("posts/2026/03/ghostty.md");
        let expected = fs::read_to_string(&path).unwrap();
        let (fm, body) = read_post_file(&expected).unwrap();

        let yaml = fm.to_yaml();
        let actual = format_frontmatter(&yaml, &body);
        assert_eq!(actual, expected, "golden output mismatch for ghostty.md");
    }

    #[test]
    fn golden_output_translation() {
        let path = fixture_dir().join("posts/2005/11/esmeralda.en.md");
        let expected = fs::read_to_string(&path).unwrap();
        let (fm, body) = read_translation_file(&expected).unwrap();

        let yaml = fm.to_yaml();
        let actual = format_frontmatter(&yaml, &body);
        assert_eq!(
            actual, expected,
            "golden output mismatch for esmeralda.en.md"
        );
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
    fn yaml_quoting() {
        assert_eq!(yaml_string_value("simple"), "simple");
        assert_eq!(yaml_string_value("has: colon"), "'has: colon'");
        assert_eq!(yaml_string_value("true"), "'true'");
        assert_eq!(yaml_string_value(""), "''");
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
    fn golden_output_template() {
        let path = fixture_dir().join("templates/testvorlage.liquid");
        let expected = fs::read_to_string(&path).unwrap();
        let (fm, body) = read_template_file(&expected).unwrap();
        let actual = write_template_file(&fm, &body);
        assert_eq!(
            actual, expected,
            "golden output mismatch for testvorlage.liquid"
        );
    }

    // --- Script frontmatter tests ---

    #[test]
    fn parse_fixture_script_bgg() {
        let path = fixture_dir().join("scripts/bgg_link.py");
        let content = fs::read_to_string(path).unwrap();
        let (fm, body) = read_script_file(&content).unwrap();
        assert_eq!(fm.id, "2b393cae-84b9-426f-b4cf-4902aea79d7d");
        assert_eq!(fm.slug, "bgg_link");
        assert_eq!(fm.title, "bgg link");
        assert_eq!(fm.kind, "transform");
        assert_eq!(fm.entrypoint, "normalize_blogmark");
        assert!(fm.enabled);
        assert_eq!(fm.version, 12);
        assert!(body.contains("def normalize_blogmark"));
    }

    #[test]
    fn parse_fixture_script_test() {
        let path = fixture_dir().join("scripts/test_script.py");
        let content = fs::read_to_string(path).unwrap();
        let (fm, body) = read_script_file(&content).unwrap();
        assert_eq!(fm.slug, "test_script");
        assert_eq!(fm.kind, "utility");
        assert_eq!(fm.entrypoint, "main");
        assert!(body.contains("print"));
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
