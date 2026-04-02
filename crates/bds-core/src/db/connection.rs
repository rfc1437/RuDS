use rusqlite::Connection;
use std::path::Path;
use crate::db::migrations;

/// Database wrapper managing a SQLite connection.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open an existing bDS project database.
    pub fn open(path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(Self { conn })
    }

    /// Open an in-memory database (for tests).
    pub fn open_in_memory() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")?;
        Ok(Self { conn })
    }

    /// Get a reference to the underlying connection.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Run all pending migrations.
    pub fn migrate(&self) -> Result<(), Box<dyn std::error::Error>> {
        migrations::run_migrations(&self.conn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_in_memory() {
        let db = Database::open_in_memory().expect("should open in-memory db");
        let result: i64 = db
            .conn()
            .query_row("SELECT 1", [], |row| row.get(0))
            .unwrap();
        assert_eq!(result, 1);
    }
}
