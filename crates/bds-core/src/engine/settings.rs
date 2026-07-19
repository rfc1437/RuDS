use crate::db::DbConnection as Connection;
use crate::db::queries::setting;
use crate::engine::{EngineError, EngineResult, domain_events};
use crate::util::now_unix_ms;

pub const UI_LANGUAGE_KEY: &str = "ui.language";

pub fn get(conn: &Connection, key: &str) -> EngineResult<Option<String>> {
    match setting::get_setting_by_key(conn, key) {
        Ok(value) => Ok(Some(value.value)),
        Err(diesel::result::Error::NotFound) => Ok(None),
        Err(error) => Err(EngineError::Db(error)),
    }
}

pub fn set(conn: &Connection, key: &str, value: &str) -> EngineResult<()> {
    set_at(conn, key, value, now_unix_ms())
}

pub fn set_at(conn: &Connection, key: &str, value: &str, updated_at: i64) -> EngineResult<()> {
    setting::set_setting_value(conn, key, value, updated_at)?;
    domain_events::settings_changed(None, key);
    Ok(())
}

pub fn ui_language(conn: &Connection) -> EngineResult<Option<String>> {
    get(conn, UI_LANGUAGE_KEY)
}
