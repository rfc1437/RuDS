use rusqlite::{params, Connection};

use crate::db::from_row::{project_from_row, PROJECT_COLUMNS};
use crate::model::Project;

pub fn insert_project(conn: &Connection, project: &Project) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO projects (id, name, slug, description, data_path, is_active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            project.id,
            project.name,
            project.slug,
            project.description,
            project.data_path,
            project.is_active as i64,
            project.created_at,
            project.updated_at,
        ],
    )?;
    Ok(())
}

pub fn get_project_by_id(conn: &Connection, id: &str) -> rusqlite::Result<Project> {
    conn.query_row(
        &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE id = ?1"),
        params![id],
        project_from_row,
    )
}

pub fn get_project_by_slug(conn: &Connection, slug: &str) -> rusqlite::Result<Project> {
    conn.query_row(
        &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE slug = ?1"),
        params![slug],
        project_from_row,
    )
}

pub fn get_active_project(conn: &Connection) -> rusqlite::Result<Project> {
    conn.query_row(
        &format!("SELECT {PROJECT_COLUMNS} FROM projects WHERE is_active = 1 LIMIT 1"),
        [],
        project_from_row,
    )
}

pub fn set_active_project(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE projects SET is_active = 0 WHERE is_active = 1", [])?;
    conn.execute(
        "UPDATE projects SET is_active = 1 WHERE id = ?1",
        params![id],
    )?;
    Ok(())
}

pub fn list_projects(conn: &Connection) -> rusqlite::Result<Vec<Project>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PROJECT_COLUMNS} FROM projects ORDER BY name"
    ))?;
    let rows = stmt.query_map([], project_from_row)?;
    rows.collect()
}

pub fn update_project(conn: &Connection, project: &Project) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE projects SET name = ?1, slug = ?2, description = ?3, data_path = ?4,
         is_active = ?5, updated_at = ?6
         WHERE id = ?7",
        params![
            project.name,
            project.slug,
            project.description,
            project.data_path,
            project.is_active as i64,
            project.updated_at,
            project.id,
        ],
    )?;
    Ok(())
}

pub fn delete_project(conn: &Connection, id: &str) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM projects WHERE id = ?1", params![id])?;
    Ok(())
}

/// Test helper: create a minimal Project value (available to sibling test modules).
#[cfg(test)]
pub fn make_test_project(id: &str, slug: &str) -> Project {
    Project {
        id: id.to_string(),
        name: format!("Project {id}"),
        slug: slug.to_string(),
        description: Some("A test project".into()),
        data_path: Some("/data".into()),
        is_active: false,
        created_at: 1000,
        updated_at: 2000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    fn setup() -> Database {
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        db
    }

    fn make_project(id: &str, slug: &str) -> Project {
        Project {
            id: id.to_string(),
            name: format!("Project {id}"),
            slug: slug.to_string(),
            description: Some("A test project".into()),
            data_path: Some("/data".into()),
            is_active: false,
            created_at: 1000,
            updated_at: 2000,
        }
    }

    #[test]
    fn insert_and_get_by_id() {
        let db = setup();
        let p = make_project("p1", "blog");
        insert_project(db.conn(), &p).unwrap();
        let fetched = get_project_by_id(db.conn(), "p1").unwrap();
        assert_eq!(fetched.id, "p1");
        assert_eq!(fetched.name, "Project p1");
        assert_eq!(fetched.slug, "blog");
        assert_eq!(fetched.description.as_deref(), Some("A test project"));
        assert_eq!(fetched.data_path.as_deref(), Some("/data"));
        assert!(!fetched.is_active);
        assert_eq!(fetched.created_at, 1000);
        assert_eq!(fetched.updated_at, 2000);
    }

    #[test]
    fn get_by_slug() {
        let db = setup();
        insert_project(db.conn(), &make_project("p1", "my-blog")).unwrap();
        let fetched = get_project_by_slug(db.conn(), "my-blog").unwrap();
        assert_eq!(fetched.id, "p1");
    }

    #[test]
    fn active_project_flow() {
        let db = setup();
        insert_project(db.conn(), &make_project("p1", "blog1")).unwrap();
        insert_project(db.conn(), &make_project("p2", "blog2")).unwrap();

        set_active_project(db.conn(), "p1").unwrap();
        let active = get_active_project(db.conn()).unwrap();
        assert_eq!(active.id, "p1");

        set_active_project(db.conn(), "p2").unwrap();
        let active = get_active_project(db.conn()).unwrap();
        assert_eq!(active.id, "p2");

        let p1 = get_project_by_id(db.conn(), "p1").unwrap();
        assert!(!p1.is_active);
    }

    #[test]
    fn list_projects_ordered() {
        let db = setup();
        let mut p2 = make_project("p2", "zebra");
        p2.name = "Zebra".into();
        let mut p1 = make_project("p1", "alpha");
        p1.name = "Alpha".into();
        insert_project(db.conn(), &p2).unwrap();
        insert_project(db.conn(), &p1).unwrap();
        let list = list_projects(db.conn()).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "Alpha");
        assert_eq!(list[1].name, "Zebra");
    }

    #[test]
    fn update_project_fields() {
        let db = setup();
        let mut p = make_project("p1", "blog");
        insert_project(db.conn(), &p).unwrap();
        p.name = "Updated".into();
        p.description = None;
        p.updated_at = 9999;
        update_project(db.conn(), &p).unwrap();
        let fetched = get_project_by_id(db.conn(), "p1").unwrap();
        assert_eq!(fetched.name, "Updated");
        assert!(fetched.description.is_none());
        assert_eq!(fetched.updated_at, 9999);
    }

    #[test]
    fn delete_project_removes_row() {
        let db = setup();
        insert_project(db.conn(), &make_project("p1", "blog")).unwrap();
        delete_project(db.conn(), "p1").unwrap();
        let result = get_project_by_id(db.conn(), "p1");
        assert!(result.is_err());
    }

    #[test]
    fn duplicate_slug_rejected() {
        let db = setup();
        insert_project(db.conn(), &make_project("p1", "blog")).unwrap();
        let result = insert_project(db.conn(), &make_project("p2", "blog"));
        assert!(result.is_err());
    }
}
