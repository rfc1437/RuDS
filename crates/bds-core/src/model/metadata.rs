use serde::{Deserialize, Serialize};

fn default_max_posts() -> i32 {
    50
}

fn default_image_import_concurrency() -> i32 {
    4
}

fn deserialize_image_import_concurrency<'de, D>(deserializer: D) -> Result<i32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let parsed = value
        .as_i64()
        .map(|value| value as i32)
        .or_else(|| value.as_str().and_then(|value| value.parse::<i32>().ok()))
        .unwrap_or_else(default_image_import_concurrency);
    Ok(parsed.clamp(1, 8))
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMetadata {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub main_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_author: Option<String>,
    #[serde(default = "default_max_posts")]
    pub max_posts_per_page: i32,
    #[serde(
        default = "default_image_import_concurrency",
        deserialize_with = "deserialize_image_import_concurrency"
    )]
    pub image_import_concurrency: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blogmark_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pico_theme: Option<String>,
    #[serde(default)]
    pub semantic_similarity_enabled: bool,
    #[serde(default)]
    pub blog_languages: Vec<String>,
}

impl ProjectMetadata {
    /// Validate metadata fields per spec constraints.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_posts_per_page < 1 || self.max_posts_per_page > 500 {
            return Err(format!(
                "maxPostsPerPage must be 1..500, got {}",
                self.max_posts_per_page
            ));
        }
        if !(1..=8).contains(&self.image_import_concurrency) {
            return Err(format!(
                "imageImportConcurrency must be 1..8, got {}",
                self.image_import_concurrency
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategorySettings {
    #[serde(default = "default_true")]
    pub render_in_lists: bool,
    #[serde(default = "default_true")]
    pub show_title: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_template_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub list_template_slug: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "postTemplateSlug")]
    pub post_template_slug: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_metadata_roundtrip() {
        let meta = ProjectMetadata {
            name: "Test Blog".into(),
            description: None,
            public_url: Some("https://example.com".into()),
            main_language: Some("en".into()),
            default_author: None,
            max_posts_per_page: 50,
            image_import_concurrency: 4,
            blogmark_category: None,
            pico_theme: None,
            semantic_similarity_enabled: false,
            blog_languages: vec!["en".into(), "de".into()],
        };
        let json = serde_json::to_string_pretty(&meta).unwrap();
        let parsed: ProjectMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "Test Blog");
        assert_eq!(parsed.max_posts_per_page, 50);
        assert_eq!(parsed.blog_languages, vec!["en", "de"]);
        // Verify camelCase
        assert!(json.contains("publicUrl"));
        assert!(json.contains("maxPostsPerPage"));
        assert!(json.contains("blogLanguages"));
    }

    #[test]
    fn project_metadata_defaults() {
        let json = r#"{"name": "Minimal"}"#;
        let meta: ProjectMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.max_posts_per_page, 50);
        assert_eq!(meta.image_import_concurrency, 4);
        assert!(!meta.semantic_similarity_enabled);
        assert!(meta.blog_languages.is_empty());
    }

    #[test]
    fn category_settings_defaults() {
        let json = "{}";
        let settings: CategorySettings = serde_json::from_str(json).unwrap();
        assert!(settings.render_in_lists);
        assert!(settings.show_title);
    }

    #[test]
    fn category_settings_camel_case() {
        let settings = CategorySettings {
            render_in_lists: false,
            show_title: true,
            post_template_slug: Some("article-tpl".into()),
            list_template_slug: None,
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("renderInLists"));
        assert!(json.contains("showTitle"));
        assert!(json.contains("postTemplateSlug"));
    }

    #[test]
    fn tag_entry_roundtrip() {
        let tag = TagEntry {
            name: "rust".into(),
            color: Some("#ff0000".into()),
            post_template_slug: Some("code-tpl".into()),
        };
        let json = serde_json::to_string(&tag).unwrap();
        assert!(json.contains("postTemplateSlug"));
        let parsed: TagEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "rust");
        assert_eq!(parsed.color.as_deref(), Some("#ff0000"));
    }

    #[test]
    fn tag_entry_minimal() {
        let json = r#"{"name": "go"}"#;
        let tag: TagEntry = serde_json::from_str(json).unwrap();
        assert_eq!(tag.name, "go");
        assert!(tag.color.is_none());
        assert!(tag.post_template_slug.is_none());
    }

    #[test]
    fn max_posts_per_page_validation() {
        let mut meta = ProjectMetadata {
            name: "Test".into(),
            description: None,
            public_url: None,
            main_language: None,
            default_author: None,
            max_posts_per_page: 50,
            image_import_concurrency: 4,
            blogmark_category: None,
            pico_theme: None,
            semantic_similarity_enabled: false,
            blog_languages: vec![],
        };
        assert!(meta.validate().is_ok());

        meta.max_posts_per_page = 0;
        assert!(meta.validate().is_err());

        meta.max_posts_per_page = 501;
        assert!(meta.validate().is_err());

        meta.max_posts_per_page = 1;
        assert!(meta.validate().is_ok());

        meta.max_posts_per_page = 500;
        assert!(meta.validate().is_ok());
    }
}
