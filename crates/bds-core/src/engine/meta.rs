use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::db::DbConnection as Connection;
use crate::engine::{EngineError, EngineResult};
use crate::model::metadata::{CategorySettings, ProjectMetadata, TagEntry};
use crate::model::{DomainEntity, NotificationAction, Project, PublishingPreferences};
use crate::util::atomic_write_str;

const PROJECT_SETTING_SUFFIX: &str = "project";
const CATEGORIES_SETTING_SUFFIX: &str = "categories";
const CATEGORY_META_SETTING_SUFFIX: &str = "category_meta";
const PUBLISHING_SETTING_SUFFIX: &str = "publishing";

#[derive(Debug, Serialize, Deserialize)]
struct CategoriesSnapshot {
    categories: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CategoryMetaSnapshot {
    categories: HashMap<String, CategorySettings>,
}

fn metadata_setting_key(project_id: &str, suffix: &str) -> String {
    format!("project:{project_id}:{suffix}")
}

fn persist_snapshot<T: Serialize>(
    conn: &Connection,
    project_id: &str,
    suffix: &str,
    value: &T,
) -> EngineResult<()> {
    crate::db::queries::setting::set_setting_value(
        conn,
        &metadata_setting_key(project_id, suffix),
        &serde_json::to_string(value)?,
        crate::util::now_unix_ms(),
    )?;
    Ok(())
}

fn load_snapshot<T: DeserializeOwned>(
    conn: &Connection,
    project_id: &str,
    suffix: &str,
) -> EngineResult<Option<T>> {
    match crate::db::queries::setting::get_setting_by_key(
        conn,
        &metadata_setting_key(project_id, suffix),
    ) {
        Ok(setting) => Ok(Some(serde_json::from_str(&setting.value)?)),
        Err(diesel::result::Error::NotFound) => Ok(None),
        Err(error) => Err(EngineError::Db(error)),
    }
}

pub fn read_project_metadata_snapshot(
    conn: &Connection,
    project_id: &str,
) -> EngineResult<Option<ProjectMetadata>> {
    load_snapshot(conn, project_id, PROJECT_SETTING_SUFFIX)
}

pub fn read_categories_snapshot(
    conn: &Connection,
    project_id: &str,
) -> EngineResult<Option<Vec<String>>> {
    Ok(
        load_snapshot::<CategoriesSnapshot>(conn, project_id, CATEGORIES_SETTING_SUFFIX)?
            .map(|snapshot| snapshot.categories),
    )
}

pub fn read_category_meta_snapshot(
    conn: &Connection,
    project_id: &str,
) -> EngineResult<Option<HashMap<String, CategorySettings>>> {
    Ok(
        load_snapshot::<CategoryMetaSnapshot>(conn, project_id, CATEGORY_META_SETTING_SUFFIX)?
            .map(|snapshot| snapshot.categories),
    )
}

pub fn read_publishing_snapshot(
    conn: &Connection,
    project_id: &str,
) -> EngineResult<Option<PublishingPreferences>> {
    load_snapshot(conn, project_id, PUBLISHING_SETTING_SUFFIX)
}

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

/// Persist the shared project row and its filesystem metadata, then publish
/// the normal project-updated domain event.
pub fn update_project_metadata(
    conn: &Connection,
    data_dir: &Path,
    project: &Project,
    metadata: &ProjectMetadata,
) -> EngineResult<ProjectMetadata> {
    if project.id.is_empty() {
        return Err(crate::engine::EngineError::Validation(
            "project id is required".to_string(),
        ));
    }
    metadata
        .validate()
        .map_err(crate::engine::EngineError::Validation)?;

    let mut persisted_project = project.clone();
    persisted_project.name = metadata.name.clone();
    persisted_project.description = metadata.description.clone();
    persisted_project.updated_at = crate::util::now_unix_ms();

    let category_metadata = read_category_meta_json(data_dir)?;
    persist_snapshot(conn, &project.id, PROJECT_SETTING_SUFFIX, metadata)?;
    write_project_json(data_dir, metadata)?;
    write_category_meta_json(data_dir, &category_metadata)?;
    crate::db::queries::project::update_project(conn, &persisted_project)?;
    crate::engine::domain_events::entity_changed(
        &project.id,
        DomainEntity::Project,
        &project.id,
        NotificationAction::Updated,
    );
    Ok(metadata.clone())
}

/// Replace all database metadata snapshots with the portable filesystem state.
pub fn sync_metadata_from_filesystem(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<()> {
    let project = read_project_json(data_dir)?;
    project.validate().map_err(EngineError::Validation)?;
    let categories = read_categories_json(data_dir)?;
    let category_meta = read_category_meta_json(data_dir)?;
    let publishing = read_publishing_json(data_dir)?;

    conn.begin_savepoint()?;
    let result = (|| {
        let mut project_row = crate::db::queries::project::get_project_by_id(conn, project_id)?;
        project_row.name = project.name.clone();
        project_row.description = project.description.clone();
        project_row.updated_at = crate::util::now_unix_ms();
        crate::db::queries::project::update_project(conn, &project_row)?;
        persist_snapshot(conn, project_id, PROJECT_SETTING_SUFFIX, &project)?;
        persist_snapshot(
            conn,
            project_id,
            CATEGORIES_SETTING_SUFFIX,
            &CategoriesSnapshot { categories },
        )?;
        persist_snapshot(
            conn,
            project_id,
            CATEGORY_META_SETTING_SUFFIX,
            &CategoryMetaSnapshot {
                categories: category_meta,
            },
        )?;
        persist_snapshot(conn, project_id, PUBLISHING_SETTING_SUFFIX, &publishing)
    })();
    match result {
        Ok(()) => conn.release_savepoint().map_err(Into::into),
        Err(error) => {
            let _ = conn.rollback_savepoint();
            Err(error)
        }
    }
}

/// Seed database snapshots for pre-snapshot projects without overwriting
/// deliberate database/file divergence on later activations.
pub fn initialize_metadata_snapshots(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<()> {
    if read_project_metadata_snapshot(conn, project_id)?.is_none() {
        let value = read_project_json(data_dir)?;
        persist_snapshot(conn, project_id, PROJECT_SETTING_SUFFIX, &value)?;
    }
    if read_categories_snapshot(conn, project_id)?.is_none() {
        let categories = read_categories_json(data_dir)?;
        persist_snapshot(
            conn,
            project_id,
            CATEGORIES_SETTING_SUFFIX,
            &CategoriesSnapshot { categories },
        )?;
    }
    if read_category_meta_snapshot(conn, project_id)?.is_none() {
        let categories = read_category_meta_json(data_dir)?;
        persist_snapshot(
            conn,
            project_id,
            CATEGORY_META_SETTING_SUFFIX,
            &CategoryMetaSnapshot { categories },
        )?;
    }
    if read_publishing_snapshot(conn, project_id)?.is_none() {
        let value = read_publishing_json(data_dir)?;
        persist_snapshot(conn, project_id, PUBLISHING_SETTING_SUFFIX, &value)?;
    }
    Ok(())
}

pub fn flush_metadata_to_filesystem(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
) -> EngineResult<()> {
    let project = read_project_metadata_snapshot(conn, project_id)?.ok_or_else(|| {
        EngineError::NotFound(format!(
            "project metadata snapshot for project {project_id}"
        ))
    })?;
    let categories = read_categories_snapshot(conn, project_id)?.ok_or_else(|| {
        EngineError::NotFound(format!(
            "categories metadata snapshot for project {project_id}"
        ))
    })?;
    let category_meta = read_category_meta_snapshot(conn, project_id)?.ok_or_else(|| {
        EngineError::NotFound(format!(
            "category metadata snapshot for project {project_id}"
        ))
    })?;
    let publishing = read_publishing_snapshot(conn, project_id)?.ok_or_else(|| {
        EngineError::NotFound(format!(
            "publishing metadata snapshot for project {project_id}"
        ))
    })?;

    write_project_json(data_dir, &project)?;
    write_categories_json(data_dir, &categories)?;
    write_category_meta_json(data_dir, &category_meta)?;
    write_publishing_json(data_dir, &publishing)
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
    sorted.sort();
    let path = data_dir.join("meta").join("categories.json");
    let json = serde_json::to_string_pretty(&sorted)?;
    atomic_write_str(&path, &json)?;
    Ok(())
}

pub fn set_categories(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    categories: &[String],
) -> EngineResult<()> {
    let mut sorted = categories.to_vec();
    sorted.sort();
    persist_snapshot(
        conn,
        project_id,
        CATEGORIES_SETTING_SUFFIX,
        &CategoriesSnapshot {
            categories: sorted.clone(),
        },
    )?;
    write_categories_json(data_dir, &sorted)
}

// ── category-meta.json ──────────────────────────────────────────────

/// Read meta/category-meta.json.
pub fn read_category_meta_json(data_dir: &Path) -> EngineResult<HashMap<String, CategorySettings>> {
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
    let sorted = meta.iter().collect::<BTreeMap<_, _>>();
    let json = serde_json::to_string_pretty(&sorted)?;
    atomic_write_str(&path, &json)?;
    Ok(())
}

pub fn set_category_meta(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    meta: &HashMap<String, CategorySettings>,
) -> EngineResult<()> {
    persist_snapshot(
        conn,
        project_id,
        CATEGORY_META_SETTING_SUFFIX,
        &CategoryMetaSnapshot {
            categories: meta.clone(),
        },
    )?;
    write_category_meta_json(data_dir, meta)
}

pub fn set_categories_and_meta(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    categories: &[String],
    meta: &HashMap<String, CategorySettings>,
) -> EngineResult<()> {
    let mut sorted = categories.to_vec();
    sorted.sort();
    conn.begin_savepoint()?;
    let result = (|| {
        persist_snapshot(
            conn,
            project_id,
            CATEGORIES_SETTING_SUFFIX,
            &CategoriesSnapshot {
                categories: sorted.clone(),
            },
        )?;
        persist_snapshot(
            conn,
            project_id,
            CATEGORY_META_SETTING_SUFFIX,
            &CategoryMetaSnapshot {
                categories: meta.clone(),
            },
        )
    })();
    match result {
        Ok(()) => conn.release_savepoint()?,
        Err(error) => {
            let _ = conn.rollback_savepoint();
            return Err(error);
        }
    }
    write_categories_json(data_dir, &sorted)?;
    write_category_meta_json(data_dir, meta)
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
pub fn write_publishing_json(data_dir: &Path, prefs: &PublishingPreferences) -> EngineResult<()> {
    let path = data_dir.join("meta").join("publishing.json");
    let json = serde_json::to_string_pretty(prefs)?;
    atomic_write_str(&path, &json)?;
    Ok(())
}

pub fn set_publishing_preferences(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    prefs: &PublishingPreferences,
) -> EngineResult<()> {
    persist_snapshot(conn, project_id, PUBLISHING_SETTING_SUFFIX, prefs)?;
    write_publishing_json(data_dir, prefs)
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
    sorted.sort_by_key(|a| a.name.to_lowercase());
    let path = data_dir.join("meta").join("tags.json");
    let json = serde_json::to_string_pretty(&sorted)?;
    atomic_write_str(&path, &json)?;
    Ok(())
}

// ── category helpers ────────────────────────────────────────────────

/// Add a category to categories.json and initialize it in category-meta.json.
pub fn add_category(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    category: &str,
) -> EngineResult<()> {
    let mut cats = read_categories_json(data_dir)?;
    if !cats.iter().any(|c| c.eq_ignore_ascii_case(category)) {
        cats.push(category.to_string());
    }

    let mut meta = read_category_meta_json(data_dir)?;
    if !meta.contains_key(category) {
        meta.insert(
            category.to_string(),
            CategorySettings {
                title: None,
                render_in_lists: true,
                show_title: true,
                post_template_slug: None,
                list_template_slug: None,
            },
        );
    }
    set_categories_and_meta(conn, data_dir, project_id, &cats, &meta)
}

/// Remove a category from both categories.json and category-meta.json.
pub fn remove_category(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    category: &str,
) -> EngineResult<()> {
    let mut cats = read_categories_json(data_dir)?;
    cats.retain(|c| !c.eq_ignore_ascii_case(category));
    let mut meta = read_category_meta_json(data_dir)?;
    meta.remove(category);
    set_categories_and_meta(conn, data_dir, project_id, &cats, &meta)
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
            image_import_concurrency: 4,
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
        write_publishing_json(data_dir, &PublishingPreferences::default())?;
    }

    // Ensure tags.json exists
    if !meta_dir.join("tags.json").exists() {
        write_tags_json(data_dir, &[])?;
    }

    // Ensure menu.opml exists
    if !meta_dir.join("menu.opml").exists() {
        let project = read_project_json(data_dir)?;
        let opml =
            crate::engine::menu::default_menu_opml(&project.name, crate::util::now_unix_ms());
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
            image_import_concurrency: 4,
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

    #[test]
    fn project_json_matches_bds2_canonical_key_order() {
        let dir = setup();
        let meta = ProjectMetadata {
            name: "Test".into(),
            description: Some("A blog".into()),
            public_url: Some("https://example.com".into()),
            main_language: Some("en".into()),
            default_author: Some("Writer".into()),
            max_posts_per_page: 25,
            image_import_concurrency: 3,
            blogmark_category: Some("links".into()),
            pico_theme: Some("amber".into()),
            semantic_similarity_enabled: true,
            blog_languages: vec!["en".into(), "de".into()],
        };
        write_project_json(dir.path(), &meta).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("meta/project.json")).unwrap(),
            r#"{
  "blogLanguages": [
    "en",
    "de"
  ],
  "blogmarkCategory": "links",
  "defaultAuthor": "Writer",
  "description": "A blog",
  "imageImportConcurrency": 3,
  "mainLanguage": "en",
  "maxPostsPerPage": 25,
  "name": "Test",
  "picoTheme": "amber",
  "publicUrl": "https://example.com",
  "semanticSimilarityEnabled": true
}"#
        );
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

    #[test]
    fn categories_json_uses_bds2_case_sensitive_sort() {
        let dir = setup();
        write_categories_json(dir.path(), &["alpha".into(), "Zebra".into(), "Beta".into()])
            .unwrap();
        assert_eq!(
            read_categories_json(dir.path()).unwrap(),
            vec!["Beta", "Zebra", "alpha"]
        );
    }

    // ── category-meta.json ──────────────────────────────────────────

    #[test]
    fn category_meta_json_roundtrip() {
        let dir = setup();
        let mut meta = HashMap::new();
        meta.insert(
            "article".to_string(),
            CategorySettings {
                title: None,
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

    #[test]
    fn category_meta_json_preserves_bds2_title() {
        let dir = setup();
        let mut meta = HashMap::new();
        meta.insert(
            "news".to_string(),
            CategorySettings {
                title: Some("News Archive".to_string()),
                render_in_lists: false,
                show_title: true,
                post_template_slug: Some("article".to_string()),
                list_template_slug: Some("listing".to_string()),
            },
        );

        write_category_meta_json(dir.path(), &meta).unwrap();

        let content = std::fs::read_to_string(dir.path().join("meta/category-meta.json")).unwrap();
        let json: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(json["news"]["title"], "News Archive");
        assert_eq!(json["news"]["renderInLists"], false);
        assert_eq!(json["news"]["showTitle"], true);

        let read = read_category_meta_json(dir.path()).unwrap();
        assert_eq!(read["news"].title.as_deref(), Some("News Archive"));
    }

    #[test]
    fn category_meta_json_matches_bds2_deterministic_key_order() {
        let dir = setup();
        let mut meta = HashMap::new();
        meta.insert(
            "zebra".to_string(),
            CategorySettings {
                title: Some("Zebra".into()),
                render_in_lists: true,
                show_title: false,
                post_template_slug: Some("post".into()),
                list_template_slug: Some("list".into()),
            },
        );
        meta.insert(
            "alpha".to_string(),
            CategorySettings {
                title: None,
                render_in_lists: false,
                show_title: true,
                post_template_slug: None,
                list_template_slug: None,
            },
        );
        write_category_meta_json(dir.path(), &meta).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("meta/category-meta.json")).unwrap(),
            r#"{
  "alpha": {
    "renderInLists": false,
    "showTitle": true
  },
  "zebra": {
    "listTemplateSlug": "list",
    "postTemplateSlug": "post",
    "renderInLists": true,
    "showTitle": false,
    "title": "Zebra"
  }
}"#
        );
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

    #[test]
    fn publishing_json_matches_bds2_canonical_key_order() {
        let dir = setup();
        let prefs = PublishingPreferences {
            ssh_host: Some("example.com".into()),
            ssh_user: Some("deploy".into()),
            ssh_remote_path: Some("/var/www".into()),
            ssh_mode: SshMode::Rsync,
        };
        write_publishing_json(dir.path(), &prefs).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("meta/publishing.json")).unwrap(),
            r#"{
  "sshHost": "example.com",
  "sshMode": "rsync",
  "sshRemotePath": "/var/www",
  "sshUser": "deploy"
}"#
        );
    }

    #[test]
    fn canonical_empty_metadata_collections_and_default_publishing_are_compact() {
        let dir = setup();
        write_project_json(
            dir.path(),
            &ProjectMetadata {
                name: "Minimal".into(),
                description: None,
                public_url: None,
                main_language: None,
                default_author: None,
                max_posts_per_page: 50,
                image_import_concurrency: 4,
                blogmark_category: None,
                pico_theme: None,
                semantic_similarity_enabled: false,
                blog_languages: Vec::new(),
            },
        )
        .unwrap();
        write_category_meta_json(dir.path(), &HashMap::new()).unwrap();
        write_tags_json(dir.path(), &[]).unwrap();
        write_publishing_json(dir.path(), &PublishingPreferences::default()).unwrap();

        assert_eq!(
            std::fs::read_to_string(dir.path().join("meta/project.json")).unwrap(),
            concat!(
                "{\n",
                "  \"blogLanguages\": [],\n",
                "  \"imageImportConcurrency\": 4,\n",
                "  \"maxPostsPerPage\": 50,\n",
                "  \"name\": \"Minimal\",\n",
                "  \"semanticSimilarityEnabled\": false\n",
                "}"
            )
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("meta/category-meta.json")).unwrap(),
            "{}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("meta/tags.json")).unwrap(),
            "[]"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("meta/publishing.json")).unwrap(),
            "{\n  \"sshMode\": \"scp\"\n}"
        );
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

    #[test]
    fn tags_json_matches_bds2_entry_order_and_omits_blank_optional_values() {
        let dir = setup();
        write_tags_json(
            dir.path(),
            &[TagEntry {
                name: "rust".into(),
                color: Some("".into()),
                post_template_slug: Some("article".into()),
            }],
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("meta/tags.json")).unwrap(),
            r#"[
  {
    "name": "rust",
    "postTemplateSlug": "article"
  }
]"#
        );
    }

    // ── add / remove category ───────────────────────────────────────

    #[test]
    fn add_category_creates_entries() {
        let dir = setup();
        let db = crate::db::Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        // Seed files
        write_categories_json(dir.path(), &["article".into()]).unwrap();
        write_category_meta_json(dir.path(), &HashMap::new()).unwrap();

        add_category(db.conn(), dir.path(), "p1", "page").unwrap();

        let cats = read_categories_json(dir.path()).unwrap();
        assert!(cats.contains(&"page".to_string()));

        let meta = read_category_meta_json(dir.path()).unwrap();
        assert!(meta.contains_key("page"));
    }

    #[test]
    fn add_category_idempotent() {
        let dir = setup();
        let db = crate::db::Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        write_categories_json(dir.path(), &["article".into()]).unwrap();
        write_category_meta_json(dir.path(), &HashMap::new()).unwrap();

        add_category(db.conn(), dir.path(), "p1", "article").unwrap();
        let cats = read_categories_json(dir.path()).unwrap();
        assert_eq!(cats.iter().filter(|c| *c == "article").count(), 1);
    }

    #[test]
    fn remove_category_deletes_entries() {
        let dir = setup();
        let db = crate::db::Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        write_categories_json(dir.path(), &["article".into(), "page".into()]).unwrap();
        let mut meta = HashMap::new();
        meta.insert(
            "article".to_string(),
            CategorySettings {
                title: None,
                render_in_lists: true,
                show_title: true,
                post_template_slug: None,
                list_template_slug: None,
            },
        );
        write_category_meta_json(dir.path(), &meta).unwrap();

        remove_category(db.conn(), dir.path(), "p1", "article").unwrap();

        let cats = read_categories_json(dir.path()).unwrap();
        assert!(!cats.contains(&"article".to_string()));
        assert!(cats.contains(&"page".to_string()));

        let meta = read_category_meta_json(dir.path()).unwrap();
        assert!(!meta.contains_key("article"));
    }

    #[test]
    fn snapshot_initialization_does_not_hide_later_filesystem_drift() {
        let dir = setup();
        let db = crate::db::Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        crate::db::queries::project::insert_project(
            db.conn(),
            &crate::db::queries::project::make_test_project("p1", "blog"),
        )
        .unwrap();
        startup_sync(dir.path()).unwrap();
        initialize_metadata_snapshots(db.conn(), dir.path(), "p1").unwrap();

        write_categories_json(dir.path(), &["filesystem-only".into()]).unwrap();
        initialize_metadata_snapshots(db.conn(), dir.path(), "p1").unwrap();

        assert_eq!(
            read_categories_snapshot(db.conn(), "p1").unwrap().unwrap(),
            vec!["article", "aside", "page", "picture"]
        );
    }
}
