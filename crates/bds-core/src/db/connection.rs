use std::cell::RefCell;
use std::path::Path;

use diesel::connection::SimpleConnection;
use diesel::prelude::*;
use diesel::sql_types::Text;

use crate::db::migrations;

#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("{0}")]
    Connection(#[from] diesel::ConnectionError),
    #[error("{0}")]
    Query(#[from] diesel::result::Error),
}

/// Shared synchronous Diesel connection used by the engine query API.
pub struct DbConnection(RefCell<SqliteConnection>);

#[derive(QueryableByName)]
struct DatabasePathRow {
    #[diesel(sql_type = Text)]
    file: String,
}

impl DbConnection {
    pub fn with<T>(
        &self,
        operation: impl FnOnce(&mut SqliteConnection) -> diesel::QueryResult<T>,
    ) -> diesel::QueryResult<T> {
        operation(&mut self.0.borrow_mut())
    }

    pub(crate) fn with_migrations<T>(
        &self,
        operation: impl FnOnce(&mut SqliteConnection) -> T,
    ) -> T {
        operation(&mut self.0.borrow_mut())
    }

    pub(crate) fn begin_savepoint(&self) -> diesel::QueryResult<()> {
        self.0.borrow_mut().batch_execute("SAVEPOINT bds_operation")
    }

    pub(crate) fn release_savepoint(&self) -> diesel::QueryResult<()> {
        self.0.borrow_mut().batch_execute("RELEASE bds_operation")
    }

    pub(crate) fn rollback_savepoint(&self) -> diesel::QueryResult<()> {
        self.0
            .borrow_mut()
            .batch_execute("ROLLBACK TO bds_operation; RELEASE bds_operation")
    }

    #[cfg(test)]
    pub(crate) fn reject_tag_template_updates_for_test(&self) -> diesel::QueryResult<()> {
        self.0.borrow_mut().batch_execute(
            "CREATE TRIGGER reject_template_tag_cascade \
             BEFORE UPDATE OF post_template_slug ON tags \
             BEGIN SELECT RAISE(ABORT, 'reject cascade'); END",
        )
    }

    /// Filesystem database path for sibling surfaces that must open their own
    /// short-lived connection (gallery workers, preview servers, Lua hosts).
    pub fn database_path(&self) -> diesel::QueryResult<std::path::PathBuf> {
        self.with(|conn| {
            diesel::sql_query("SELECT file FROM pragma_database_list WHERE name = 'main'")
                .get_result::<DatabasePathRow>(conn)
                .and_then(|row| {
                    if row.file.is_empty() {
                        Err(diesel::result::Error::NotFound)
                    } else {
                        Ok(row.file.into())
                    }
                })
        })
    }
}

/// Database wrapper managing a SQLite connection.
pub struct Database {
    conn: DbConnection,
}

impl Database {
    /// Open an existing bDS project database.
    pub fn open(path: &Path) -> Result<Self, DatabaseError> {
        Self::establish(path.to_string_lossy().as_ref(), true)
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self, DatabaseError> {
        Self::establish(":memory:", false)
    }

    fn establish(database_url: &str, wal: bool) -> Result<Self, DatabaseError> {
        let mut conn = SqliteConnection::establish(database_url)?;
        // SQLite connection configuration is backend-specific and not expressible in Diesel's DSL.
        conn.batch_execute(if wal {
            "PRAGMA busy_timeout=5000; PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;"
        } else {
            "PRAGMA foreign_keys=ON;"
        })?;
        Ok(Self {
            conn: DbConnection(RefCell::new(conn)),
        })
    }

    pub fn conn(&self) -> &DbConnection {
        &self.conn
    }

    /// Run all pending embedded Diesel migrations.
    pub fn migrate(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        migrations::run_migrations(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory() {
        let db = Database::open_in_memory().expect("should open in-memory db");
        let result = db
            .conn()
            .with(|conn| {
                diesel::select(1.into_sql::<diesel::sql_types::Integer>()).get_result::<i32>(conn)
            })
            .unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn reports_the_disk_database_path() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("project.sqlite3");
        let db = Database::open(&path).unwrap();

        assert_eq!(
            db.conn().database_path().unwrap().canonicalize().unwrap(),
            path.canonicalize().unwrap()
        );
    }
}
