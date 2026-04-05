use std::collections::HashMap;
use std::fs;
use std::path::Path;

use rusqlite::Connection;
use uuid::Uuid;

use crate::db::queries::project as q;
use crate::engine::{EngineError, EngineResult};
use crate::model::Project;
use crate::model::metadata::ProjectMetadata;
use crate::util::{atomic_write_str, now_unix_ms, slugify, ensure_unique};

/// Create a new project: insert into DB, create directory structure, write default meta files.
pub fn create_project(
    conn: &Connection,
    name: &str,
    data_path: Option<&str>,
) -> EngineResult<Project> {
    let id = Uuid::new_v4().to_string();
    let base_slug = slugify(name);
    let slug = ensure_unique(&base_slug, |candidate| {
        q::get_project_by_slug(conn, candidate).is_ok()
    });

    let now = now_unix_ms();
    let project = Project {
        id: id.clone(),
        name: name.to_string(),
        slug: slug.clone(),
        description: None,
        data_path: data_path.map(|s| s.to_string()),
        is_active: false,
        created_at: now,
        updated_at: now,
    };
    q::insert_project(conn, &project)?;

    // Determine data directory
    let data_dir = match data_path {
        Some(p) => std::path::PathBuf::from(p),
        None => std::path::PathBuf::from("projects").join(&id),
    };

    // Create directory structure
    create_directory_structure(&data_dir)?;

    // Write default meta files
    write_default_meta_files(&data_dir, name)?;

    Ok(project)
}

/// Get the currently active project, if any.
pub fn get_active_project(conn: &Connection) -> EngineResult<Option<Project>> {
    match q::get_active_project(conn) {
        Ok(p) => Ok(Some(p)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(EngineError::Db(e)),
    }
}

/// Deactivate all projects, then activate the given one.
pub fn set_active_project(conn: &Connection, project_id: &str) -> EngineResult<()> {
    q::set_active_project(conn, project_id)?;
    Ok(())
}

/// List all projects ordered by name.
pub fn list_projects(conn: &Connection) -> EngineResult<Vec<Project>> {
    Ok(q::list_projects(conn)?)
}

/// Delete a project row (cascading handled by queries).
/// Rejects deletion of the currently active project.
/// Optionally cleans up the project data directory.
pub fn delete_project(conn: &Connection, project_id: &str, data_dir: Option<&Path>) -> EngineResult<()> {
    // Check if this is the active project (don't delete active)
    if let Ok(active) = q::get_active_project(conn) {
        if active.id == project_id {
            return Err(EngineError::Validation(
                "cannot delete the active project".to_string(),
            ));
        }
    }

    q::delete_project(conn, project_id)?;

    // Clean up filesystem if path provided
    if let Some(dir) = data_dir {
        if dir.exists() {
            let _ = fs::remove_dir_all(dir);
        }
    }

    Ok(())
}

// ── helpers ──────────────────────────────────────────────────────────

fn create_directory_structure(data_dir: &Path) -> EngineResult<()> {
    let subdirs = ["posts", "media", "meta", "thumbnails", "templates", "scripts"];
    for sub in &subdirs {
        fs::create_dir_all(data_dir.join(sub))?;
    }
    Ok(())
}

fn write_default_meta_files(data_dir: &Path, project_name: &str) -> EngineResult<()> {
    let meta_dir = data_dir.join("meta");

    // project.json
    let project_meta = ProjectMetadata {
        name: project_name.to_string(),
        description: None,
        public_url: None,
        main_language: None,
        default_author: None,
        max_posts_per_page: 50,
        blogmark_category: None,
        pico_theme: None,
        python_runtime_mode: None,
        semantic_similarity_enabled: false,
        blog_languages: Vec::new(),
    };
    let json = serde_json::to_string_pretty(&project_meta)?;
    atomic_write_str(&meta_dir.join("project.json"), &json)?;

    // categories.json — default categories
    let categories = vec!["article", "aside", "page", "picture"];
    let json = serde_json::to_string_pretty(&categories)?;
    atomic_write_str(&meta_dir.join("categories.json"), &json)?;

    // category-meta.json — empty object
    let empty_map: HashMap<String, serde_json::Value> = HashMap::new();
    let json = serde_json::to_string_pretty(&empty_map)?;
    atomic_write_str(&meta_dir.join("category-meta.json"), &json)?;

    // publishing.json — empty object
    atomic_write_str(&meta_dir.join("publishing.json"), "{}")?;

    // tags.json — empty array
    atomic_write_str(&meta_dir.join("tags.json"), "[]")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::TempDir;

    fn setup() -> (Database, TempDir) {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        let dir = TempDir::new().unwrap();
        (db, dir)
    }

    #[test]
    fn create_project_inserts_and_creates_dirs() {
        let (db, dir) = setup();
        let data_path = dir.path().join("my-blog");
        let project = create_project(
            db.conn(),
            "My Blog",
            Some(data_path.to_str().unwrap()),
        )
        .unwrap();

        assert_eq!(project.name, "My Blog");
        assert_eq!(project.slug, "my-blog");
        assert!(!project.is_active);

        // Verify directories
        assert!(data_path.join("posts").is_dir());
        assert!(data_path.join("media").is_dir());
        assert!(data_path.join("meta").is_dir());
        assert!(data_path.join("thumbnails").is_dir());
        assert!(data_path.join("templates").is_dir());
        assert!(data_path.join("scripts").is_dir());

        // Verify meta files
        let project_json: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(data_path.join("meta/project.json")).unwrap())
                .unwrap();
        assert_eq!(project_json["name"], "My Blog");
        assert_eq!(project_json["maxPostsPerPage"], 50);

        let cats: Vec<String> =
            serde_json::from_str(&fs::read_to_string(data_path.join("meta/categories.json")).unwrap())
                .unwrap();
        assert_eq!(cats, vec!["article", "aside", "page", "picture"]);

        let cat_meta: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(data_path.join("meta/category-meta.json")).unwrap())
                .unwrap();
        assert!(cat_meta.as_object().unwrap().is_empty());

        let tags: Vec<String> =
            serde_json::from_str(&fs::read_to_string(data_path.join("meta/tags.json")).unwrap())
                .unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn create_project_unique_slug() {
        let (db, dir) = setup();
        let p1_path = dir.path().join("blog1");
        create_project(db.conn(), "Blog", Some(p1_path.to_str().unwrap())).unwrap();
        let p2_path = dir.path().join("blog2");
        let p2 = create_project(db.conn(), "Blog", Some(p2_path.to_str().unwrap())).unwrap();
        assert_eq!(p2.slug, "blog-2");
    }

    #[test]
    fn get_active_project_none() {
        let (db, _dir) = setup();
        assert!(get_active_project(db.conn()).unwrap().is_none());
    }

    #[test]
    fn set_and_get_active_project() {
        let (db, dir) = setup();
        let p1_path = dir.path().join("p1");
        let p1 = create_project(db.conn(), "P1", Some(p1_path.to_str().unwrap())).unwrap();
        set_active_project(db.conn(), &p1.id).unwrap();
        let active = get_active_project(db.conn()).unwrap().unwrap();
        assert_eq!(active.id, p1.id);
    }

    #[test]
    fn list_projects_returns_all() {
        let (db, dir) = setup();
        let p1_path = dir.path().join("alpha");
        let p2_path = dir.path().join("beta");
        create_project(db.conn(), "Beta", Some(p2_path.to_str().unwrap())).unwrap();
        create_project(db.conn(), "Alpha", Some(p1_path.to_str().unwrap())).unwrap();
        let list = list_projects(db.conn()).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Alpha");
        assert_eq!(list[1].name, "Beta");
    }

    #[test]
    fn delete_project_removes_row() {
        let (db, dir) = setup();
        let p_path = dir.path().join("p");
        let p = create_project(db.conn(), "P", Some(p_path.to_str().unwrap())).unwrap();
        delete_project(db.conn(), &p.id, Some(&p_path)).unwrap();
        assert!(list_projects(db.conn()).unwrap().is_empty());
        assert!(!p_path.exists(), "project directory should be cleaned up");
    }

    #[test]
    fn delete_active_project_rejected() {
        let (db, dir) = setup();
        let p_path = dir.path().join("p");
        let p = create_project(db.conn(), "P", Some(p_path.to_str().unwrap())).unwrap();
        set_active_project(db.conn(), &p.id).unwrap();
        let result = delete_project(db.conn(), &p.id, None);
        assert!(result.is_err());
    }
}
