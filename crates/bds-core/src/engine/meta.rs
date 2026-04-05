use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::engine::EngineResult;
use crate::model::metadata::{CategorySettings, ProjectMetadata, TagEntry};
use crate::model::PublishingPreferences;
use crate::util::atomic_write_str;

// ── project.json ────────────────────────────────────────────────────

/// Read and parse meta/project.json.
pub fn read_project_json(data_dir: &Path) -> EngineResult<ProjectMetadata> {
    let path = data_dir.join("meta").join("project.json");
    let content = fs::read_to_string(&path)?;
    let meta: ProjectMetadata = serde_json::from_str(&content)?;
    Ok(meta)
}

/// Serialize with pretty JSON, atomic write to meta/project.json.
pub fn write_project_json(data_dir: &Path, meta: &ProjectMetadata) -> EngineResult<()> {
    let path = data_dir.join("meta").join("project.json");
    let json = serde_json::to_string_pretty(meta)?;
    atomic_write_str(&path, &json)?;
    Ok(())
}

// ── categories.json ─────────────────────────────────────────────────

/// Read meta/categories.json as a sorted array of strings.
pub fn read_categories_json(data_dir: &Path) -> EngineResult<Vec<String>> {
    let path = data_dir.join("meta").join("categories.json");
    let content = fs::read_to_string(&path)?;
    let cats: Vec<String> = serde_json::from_str(&content)?;
    Ok(cats)
}

/// Sort categories, then atomic write to meta/categories.json.
pub fn write_categories_json(data_dir: &Path, categories: &[String]) -> EngineResult<()> {
    let mut sorted = categories.to_vec();
    sorted.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    let path = data_dir.join("meta").join("categories.json");
    let json = serde_json::to_string_pretty(&sorted)?;
    atomic_write_str(&path, &json)?;
    Ok(())
}

// ── category-meta.json ──────────────────────────────────────────────

/// Read meta/category-meta.json.
pub fn read_category_meta_json(
    data_dir: &Path,
) -> EngineResult<HashMap<String, CategorySettings>> {
    let path = data_dir.join("meta").join("category-meta.json");
    let content = fs::read_to_string(&path)?;
    let meta: HashMap<String, CategorySettings> = serde_json::from_str(&content)?;
    Ok(meta)
}

/// Atomic write to meta/category-meta.json.
pub fn write_category_meta_json(
    data_dir: &Path,
    meta: &HashMap<String, CategorySettings>,
) -> EngineResult<()> {
    let path = data_dir.join("meta").join("category-meta.json");
    let json = serde_json::to_string_pretty(meta)?;
    atomic_write_str(&path, &json)?;
    Ok(())
}

// ── publishing.json ─────────────────────────────────────────────────

/// Read meta/publishing.json.
pub fn read_publishing_json(data_dir: &Path) -> EngineResult<PublishingPreferences> {
    let path = data_dir.join("meta").join("publishing.json");
    let content = fs::read_to_string(&path)?;
    let prefs: PublishingPreferences = serde_json::from_str(&content)?;
    Ok(prefs)
}

/// Atomic write to meta/publishing.json.
pub fn write_publishing_json(
    data_dir: &Path,
    prefs: &PublishingPreferences,
) -> EngineResult<()> {
    let path = data_dir.join("meta").join("publishing.json");
    let json = serde_json::to_string_pretty(prefs)?;
    atomic_write_str(&path, &json)?;
    Ok(())
}

// ── tags.json ───────────────────────────────────────────────────────

/// Read meta/tags.json.
pub fn read_tags_json(data_dir: &Path) -> EngineResult<Vec<TagEntry>> {
    let path = data_dir.join("meta").join("tags.json");
    let content = fs::read_to_string(&path)?;
    let tags: Vec<TagEntry> = serde_json::from_str(&content)?;
    Ok(tags)
}

/// Sort by name case-insensitive, then atomic write to meta/tags.json.
pub fn write_tags_json(data_dir: &Path, tags: &[TagEntry]) -> EngineResult<()> {
    let mut sorted = tags.to_vec();
    sorted.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let path = data_dir.join("meta").join("tags.json");
    let json = serde_json::to_string_pretty(&sorted)?;
    atomic_write_str(&path, &json)?;
    Ok(())
}

// ── category helpers ────────────────────────────────────────────────

/// Add a category to categories.json and initialize it in category-meta.json.
pub fn add_category(data_dir: &Path, category: &str) -> EngineResult<()> {
    let mut cats = read_categories_json(data_dir)?;
    if !cats.iter().any(|c| c.eq_ignore_ascii_case(category)) {
        cats.push(category.to_string());
        write_categories_json(data_dir, &cats)?;
    }

    let mut meta = read_category_meta_json(data_dir)?;
    if !meta.contains_key(category) {
        meta.insert(
            category.to_string(),
            CategorySettings {
                render_in_lists: true,
                show_title: true,
                post_template_slug: None,
                list_template_slug: None,
            },
        );
        write_category_meta_json(data_dir, &meta)?;
    }

    Ok(())
}

/// Remove a category from both categories.json and category-meta.json.
pub fn remove_category(data_dir: &Path, category: &str) -> EngineResult<()> {
    let mut cats = read_categories_json(data_dir)?;
    cats.retain(|c| !c.eq_ignore_ascii_case(category));
    write_categories_json(data_dir, &cats)?;

    let mut meta = read_category_meta_json(data_dir)?;
    meta.remove(category);
    write_category_meta_json(data_dir, &meta)?;

    Ok(())
}

// ── update helpers ──────────────────────────────────────────────────

/// Update the blog_languages list in project.json.
/// Per metadata.allium UpdateProjectMetadata: writes ProjectJsonWritten.
pub fn update_blog_languages(data_dir: &Path, languages: Vec<String>) -> EngineResult<()> {
    let mut meta = read_project_json(data_dir)?;
    meta.blog_languages = languages;
    write_project_json(data_dir, &meta)?;
    Ok(())
}

/// Update arbitrary fields of project metadata.
/// Per metadata.allium UpdateProjectMetadata rule.
pub fn update_project_metadata(
    data_dir: &Path,
    changes: &serde_json::Value,
) -> EngineResult<()> {
    let mut meta = read_project_json(data_dir)?;
    if let Some(name) = changes.get("name").and_then(|v| v.as_str()) {
        meta.name = name.to_string();
    }
    if let Some(desc) = changes.get("description") {
        meta.description = desc.as_str().map(|s| s.to_string());
    }
    if let Some(url) = changes.get("publicUrl") {
        meta.public_url = url.as_str().map(|s| s.to_string());
    }
    if let Some(lang) = changes.get("mainLanguage") {
        meta.main_language = lang.as_str().map(|s| s.to_string());
    }
    if let Some(author) = changes.get("defaultAuthor") {
        meta.default_author = author.as_str().map(|s| s.to_string());
    }
    if let Some(max) = changes.get("maxPostsPerPage").and_then(|v| v.as_i64()) {
        meta.max_posts_per_page = max as i32;
    }
    if let Some(cat) = changes.get("blogmarkCategory") {
        meta.blogmark_category = cat.as_str().map(|s| s.to_string());
    }
    if let Some(theme) = changes.get("picoTheme") {
        meta.pico_theme = theme.as_str().map(|s| s.to_string());
    }
    if let Some(enabled) = changes.get("semanticSimilarityEnabled").and_then(|v| v.as_bool()) {
        meta.semantic_similarity_enabled = enabled;
    }
    if let Some(langs) = changes.get("blogLanguages").and_then(|v| v.as_array()) {
        meta.blog_languages = langs
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
    }
    meta.validate().map_err(|e| crate::engine::EngineError::Validation(e))?;
    write_project_json(data_dir, &meta)?;
    Ok(())
}

// ── startup sync ────────────────────────────────────────────────────

/// Per metadata.allium StartupSync: loads metadata from filesystem, creating
/// default files if missing.  Called on project activation.
pub fn startup_sync(data_dir: &Path) -> EngineResult<()> {
    let meta_dir = data_dir.join("meta");
    fs::create_dir_all(&meta_dir)?;

    // Ensure project.json exists
    if !meta_dir.join("project.json").exists() {
        let default_meta = ProjectMetadata {
            name: "My Blog".to_string(),
            description: None,
            public_url: None,
            main_language: None,
            default_author: None,
            max_posts_per_page: 50,
            blogmark_category: None,
            pico_theme: None,
            semantic_similarity_enabled: false,
            blog_languages: Vec::new(),
        };
        write_project_json(data_dir, &default_meta)?;
    }

    // Ensure categories.json exists with defaults
    if !meta_dir.join("categories.json").exists() {
        let defaults = vec![
            "article".to_string(),
            "aside".to_string(),
            "page".to_string(),
            "picture".to_string(),
        ];
        write_categories_json(data_dir, &defaults)?;
    }

    // Ensure category-meta.json exists
    if !meta_dir.join("category-meta.json").exists() {
        let empty: HashMap<String, CategorySettings> = HashMap::new();
        write_category_meta_json(data_dir, &empty)?;
    }

    // Ensure publishing.json exists
    if !meta_dir.join("publishing.json").exists() {
        atomic_write_str(&meta_dir.join("publishing.json"), "{}")?;
    }

    // Ensure tags.json exists
    if !meta_dir.join("tags.json").exists() {
        write_tags_json(data_dir, &[])?;
    }

    // Ensure menu.opml exists
    if !meta_dir.join("menu.opml").exists() {
        let opml = crate::engine::menu::default_menu_opml();
        atomic_write_str(&meta_dir.join("menu.opml"), &opml)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SshMode;
    use tempfile::TempDir;

    fn setup() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("meta")).unwrap();
        dir
    }

    // ── project.json ────────────────────────────────────────────────

    #[test]
    fn project_json_roundtrip() {
        let dir = setup();
        let meta = ProjectMetadata {
            name: "Test".into(),
            description: Some("A blog".into()),
            public_url: None,
            main_language: Some("en".into()),
            default_author: None,
            max_posts_per_page: 25,
            blogmark_category: None,
            pico_theme: None,
            semantic_similarity_enabled: false,
            blog_languages: vec!["en".into()],
        };
        write_project_json(dir.path(), &meta).unwrap();
        let read = read_project_json(dir.path()).unwrap();
        assert_eq!(read.name, "Test");
        assert_eq!(read.max_posts_per_page, 25);
        assert_eq!(read.description.as_deref(), Some("A blog"));
    }

    // ── categories.json ─────────────────────────────────────────────

    #[test]
    fn categories_json_sorted() {
        let dir = setup();
        let cats = vec!["picture".into(), "article".into(), "aside".into()];
        write_categories_json(dir.path(), &cats).unwrap();
        let read = read_categories_json(dir.path()).unwrap();
        assert_eq!(read, vec!["article", "aside", "picture"]);
    }

    // ── category-meta.json ──────────────────────────────────────────

    #[test]
    fn category_meta_json_roundtrip() {
        let dir = setup();
        let mut meta = HashMap::new();
        meta.insert(
            "article".to_string(),
            CategorySettings {
                render_in_lists: true,
                show_title: true,
                post_template_slug: None,
                list_template_slug: None,
            },
        );
        write_category_meta_json(dir.path(), &meta).unwrap();
        let read = read_category_meta_json(dir.path()).unwrap();
        assert!(read.contains_key("article"));
        assert!(read["article"].render_in_lists);
    }

    // ── publishing.json ─────────────────────────────────────────────

    #[test]
    fn publishing_json_roundtrip() {
        let dir = setup();
        let prefs = PublishingPreferences {
            ssh_host: Some("example.com".into()),
            ssh_user: Some("deploy".into()),
            ssh_remote_path: Some("/var/www".into()),
            ssh_mode: SshMode::Rsync,
        };
        write_publishing_json(dir.path(), &prefs).unwrap();
        let read = read_publishing_json(dir.path()).unwrap();
        assert_eq!(read.ssh_host.as_deref(), Some("example.com"));
        assert_eq!(read.ssh_mode, SshMode::Rsync);
    }

    // ── tags.json ───────────────────────────────────────────────────

    #[test]
    fn tags_json_sorted_case_insensitive() {
        let dir = setup();
        let tags = vec![
            TagEntry {
                name: "Zebra".into(),
                color: None,
                post_template_slug: None,
            },
            TagEntry {
                name: "alpha".into(),
                color: Some("#00ff00".into()),
                post_template_slug: None,
            },
        ];
        write_tags_json(dir.path(), &tags).unwrap();
        let read = read_tags_json(dir.path()).unwrap();
        assert_eq!(read[0].name, "alpha");
        assert_eq!(read[1].name, "Zebra");
    }

    // ── add / remove category ───────────────────────────────────────

    #[test]
    fn add_category_creates_entries() {
        let dir = setup();
        // Seed files
        write_categories_json(dir.path(), &vec!["article".into()]).unwrap();
        write_category_meta_json(dir.path(), &HashMap::new()).unwrap();

        add_category(dir.path(), "page").unwrap();

        let cats = read_categories_json(dir.path()).unwrap();
        assert!(cats.contains(&"page".to_string()));

        let meta = read_category_meta_json(dir.path()).unwrap();
        assert!(meta.contains_key("page"));
    }

    #[test]
    fn add_category_idempotent() {
        let dir = setup();
        write_categories_json(dir.path(), &vec!["article".into()]).unwrap();
        write_category_meta_json(dir.path(), &HashMap::new()).unwrap();

        add_category(dir.path(), "article").unwrap();
        let cats = read_categories_json(dir.path()).unwrap();
        assert_eq!(cats.iter().filter(|c| *c == "article").count(), 1);
    }

    #[test]
    fn remove_category_deletes_entries() {
        let dir = setup();
        write_categories_json(dir.path(), &vec!["article".into(), "page".into()]).unwrap();
        let mut meta = HashMap::new();
        meta.insert(
            "article".to_string(),
            CategorySettings {
                render_in_lists: true,
                show_title: true,
                post_template_slug: None,
                list_template_slug: None,
            },
        );
        write_category_meta_json(dir.path(), &meta).unwrap();

        remove_category(dir.path(), "article").unwrap();

        let cats = read_categories_json(dir.path()).unwrap();
        assert!(!cats.contains(&"article".to_string()));
        assert!(cats.contains(&"page".to_string()));

        let meta = read_category_meta_json(dir.path()).unwrap();
        assert!(!meta.contains_key("article"));
    }
}
