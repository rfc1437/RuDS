use rusqlite::{params, Connection};

use crate::db::from_row::{setting_from_row, SETTING_COLUMNS};
use crate::model::Setting;

pub fn get_setting_by_key(conn: &Connection, key: &str) -> rusqlite::Result<Setting> {
    conn.query_row(
        &format!("SELECT {SETTING_COLUMNS} FROM settings WHERE key = ?1"),
        params![key],
        setting_from_row,
    )
}

pub fn set_setting_value(
    conn: &Connection,
    key: &str,
    value: &str,
    updated_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, updated_at],
    )?;
    Ok(())
}

pub fn list_all_settings(conn: &Connection) -> rusqlite::Result<Vec<Setting>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SETTING_COLUMNS} FROM settings ORDER BY key"
    ))?;
    let rows = stmt.query_map([], setting_from_row)?;
    rows.collect()
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

    #[test]
    fn set_and_get() {
        let db = setup();
        set_setting_value(db.conn(), "theme", "dark", 1000).unwrap();
        let s = get_setting_by_key(db.conn(), "theme").unwrap();
        assert_eq!(s.key, "theme");
        assert_eq!(s.value, "dark");
        assert_eq!(s.updated_at, 1000);
    }

    #[test]
    fn upsert_updates_existing() {
        let db = setup();
        set_setting_value(db.conn(), "theme", "dark", 1000).unwrap();
        set_setting_value(db.conn(), "theme", "light", 2000).unwrap();
        let s = get_setting_by_key(db.conn(), "theme").unwrap();
        assert_eq!(s.value, "light");
        assert_eq!(s.updated_at, 2000);
    }

    #[test]
    fn list_all() {
        let db = setup();
        set_setting_value(db.conn(), "b_key", "val_b", 1000).unwrap();
        set_setting_value(db.conn(), "a_key", "val_a", 1000).unwrap();
        let list = list_all_settings(db.conn()).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].key, "a_key");
        assert_eq!(list[1].key, "b_key");
    }

    #[test]
    fn get_nonexistent_returns_error() {
        let db = setup();
        assert!(get_setting_by_key(db.conn(), "nope").is_err());
    }
}
