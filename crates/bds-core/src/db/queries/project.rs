use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::from_row::ProjectRecord;
use crate::db::schema::projects;
use crate::model::Project;

pub fn insert_project(conn: &DbConnection, project: &Project) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(projects::table)
            .values(ProjectRecord::from(project))
            .execute(c)
            .map(|_| ())
    })
}

pub fn get_project_by_id(conn: &DbConnection, id: &str) -> QueryResult<Project> {
    conn.with(|c| {
        projects::table
            .filter(projects::id.eq(id))
            .select(ProjectRecord::as_select())
            .first(c)
            .map(Into::into)
    })
}

pub fn get_project_by_slug(conn: &DbConnection, slug: &str) -> QueryResult<Project> {
    conn.with(|c| {
        projects::table
            .filter(projects::slug.eq(slug))
            .select(ProjectRecord::as_select())
            .first(c)
            .map(Into::into)
    })
}

pub fn get_active_project(conn: &DbConnection) -> QueryResult<Project> {
    conn.with(|c| {
        projects::table
            .filter(projects::is_active.eq(1))
            .select(ProjectRecord::as_select())
            .first(c)
            .map(Into::into)
    })
}

pub fn set_active_project(conn: &DbConnection, id: &str) -> QueryResult<()> {
    conn.with(|c| {
        c.transaction(|c| {
            diesel::update(projects::table.filter(projects::is_active.eq(1)))
                .set(projects::is_active.eq(0))
                .execute(c)?;
            diesel::update(projects::table.filter(projects::id.eq(id)))
                .set(projects::is_active.eq(1))
                .execute(c)?;
            Ok(())
        })
    })
}

pub fn list_projects(conn: &DbConnection) -> QueryResult<Vec<Project>> {
    conn.with(|c| {
        projects::table
            .order(projects::name)
            .select(ProjectRecord::as_select())
            .load(c)
            .map(|rows: Vec<ProjectRecord>| rows.into_iter().map(Into::into).collect())
    })
}

pub fn update_project(conn: &DbConnection, project: &Project) -> QueryResult<()> {
    conn.with(|c| {
        diesel::update(projects::table.filter(projects::id.eq(&project.id)))
            .set(ProjectRecord::from(project))
            .execute(c)
            .map(|_| ())
    })
}

pub fn delete_project(conn: &DbConnection, id: &str) -> QueryResult<()> {
    conn.with(|c| {
        diesel::delete(projects::table.filter(projects::id.eq(id)))
            .execute(c)
            .map(|_| ())
    })
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
        let db = Database::open_in_memory().unwrap();
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
