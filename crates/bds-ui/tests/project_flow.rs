//! Project flow integration tests.
//!
//! Tests the project lifecycle: create, list, set active, switch, delete.
//! Uses in-memory DB — no Iced window required.

use bds_core::db::Database;
use bds_core::engine::project;
use tempfile::TempDir;

fn setup() -> (Database, TempDir) {
    let mut db = Database::open_in_memory().unwrap();
    db.migrate().unwrap();
    let dir = TempDir::new().unwrap();
    (db, dir)
}

#[test]
fn create_and_activate_default_project() {
    let (db, dir) = setup();
    let data_path = dir.path().join("my-blog");

    let p =
        project::create_project(db.conn(), "My Blog", Some(data_path.to_str().unwrap())).unwrap();

    assert_eq!(p.name, "My Blog");
    assert_eq!(p.slug, "my-blog");
    assert!(!p.is_active);

    // Activate it
    project::set_active_project(db.conn(), &p.id).unwrap();
    let active = project::get_active_project(db.conn()).unwrap().unwrap();
    assert_eq!(active.id, p.id);
}

#[test]
fn switch_between_projects() {
    let (db, dir) = setup();
    let p1_path = dir.path().join("blog-a");
    let p2_path = dir.path().join("blog-b");

    let p1 = project::create_project(db.conn(), "Blog A", Some(p1_path.to_str().unwrap())).unwrap();
    let p2 = project::create_project(db.conn(), "Blog B", Some(p2_path.to_str().unwrap())).unwrap();

    project::set_active_project(db.conn(), &p1.id).unwrap();
    let active = project::get_active_project(db.conn()).unwrap().unwrap();
    assert_eq!(active.id, p1.id);

    // Switch
    project::set_active_project(db.conn(), &p2.id).unwrap();
    let active = project::get_active_project(db.conn()).unwrap().unwrap();
    assert_eq!(active.id, p2.id);
}

#[test]
fn delete_inactive_project() {
    let (db, dir) = setup();
    let p1_path = dir.path().join("keep");
    let p2_path = dir.path().join("remove");

    let p1 = project::create_project(db.conn(), "Keep", Some(p1_path.to_str().unwrap())).unwrap();
    let p2 = project::create_project(db.conn(), "Remove", Some(p2_path.to_str().unwrap())).unwrap();

    project::set_active_project(db.conn(), &p1.id).unwrap();
    project::delete_project(db.conn(), &p2.id, Some(&p2_path)).unwrap();

    let list = project::list_projects(db.conn()).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "Keep");
}

#[test]
fn cannot_delete_active_project() {
    let (db, dir) = setup();
    let p_path = dir.path().join("active");
    let p = project::create_project(db.conn(), "Active", Some(p_path.to_str().unwrap())).unwrap();

    project::set_active_project(db.conn(), &p.id).unwrap();
    let result = project::delete_project(db.conn(), &p.id, None);
    assert!(result.is_err());
}

#[test]
fn project_directory_structure_created() {
    let (db, dir) = setup();
    let p_path = dir.path().join("structured");
    project::create_project(db.conn(), "Structured", Some(p_path.to_str().unwrap())).unwrap();

    assert!(p_path.join("posts").is_dir());
    assert!(p_path.join("media").is_dir());
    assert!(p_path.join("meta").is_dir());
    assert!(p_path.join("thumbnails").is_dir());
    assert!(p_path.join("templates").is_dir());
    assert!(p_path.join("scripts").is_dir());
}

#[test]
fn default_meta_files_written() {
    let (db, dir) = setup();
    let p_path = dir.path().join("meta-test");
    project::create_project(db.conn(), "Meta Test", Some(p_path.to_str().unwrap())).unwrap();

    let project_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(p_path.join("meta/project.json")).unwrap())
            .unwrap();
    assert_eq!(project_json["name"], "Meta Test");
    assert_eq!(project_json["maxPostsPerPage"], 50);

    let categories: Vec<String> = serde_json::from_str(
        &std::fs::read_to_string(p_path.join("meta/categories.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(categories, vec!["article", "aside", "page", "picture"]);

    let tags: Vec<String> =
        serde_json::from_str(&std::fs::read_to_string(p_path.join("meta/tags.json")).unwrap())
            .unwrap();
    assert!(tags.is_empty());
}
