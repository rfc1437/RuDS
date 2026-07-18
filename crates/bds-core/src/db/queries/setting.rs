use diesel::prelude::*;

use crate::db::DbConnection;
use crate::db::from_row::SettingRecord;
use crate::db::schema::settings;
use crate::model::Setting;

pub fn get_setting_by_key(conn: &DbConnection, key: &str) -> QueryResult<Setting> {
    conn.with(|c| {
        settings::table
            .filter(settings::key.eq(key))
            .select(SettingRecord::as_select())
            .first(c)
            .map(Into::into)
    })
}

pub fn set_setting_value(
    conn: &DbConnection,
    key: &str,
    value: &str,
    updated_at: i64,
) -> QueryResult<()> {
    conn.with(|c| {
        diesel::insert_into(settings::table)
            .values((
                settings::key.eq(key),
                settings::value.eq(value),
                settings::updated_at.eq(updated_at),
            ))
            .on_conflict(settings::key)
            .do_update()
            .set((
                settings::value.eq(value),
                settings::updated_at.eq(updated_at),
            ))
            .execute(c)
            .map(|_| ())
    })
}

pub fn list_all_settings(conn: &DbConnection) -> QueryResult<Vec<Setting>> {
    conn.with(|c| {
        settings::table
            .order(settings::key)
            .select(SettingRecord::as_select())
            .load(c)
            .map(|rows: Vec<SettingRecord>| rows.into_iter().map(Into::into).collect())
    })
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
