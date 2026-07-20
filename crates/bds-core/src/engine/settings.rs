use std::collections::BTreeMap;

use crate::db::DbConnection as Connection;
use crate::db::queries::setting;
use crate::engine::{EngineError, EngineResult, domain_events};
use crate::util::now_unix_ms;

pub const UI_LANGUAGE_KEY: &str = "ui.language";
pub const ONLINE_API_KEY: &str = "ai.endpoint.online.api_key";
pub const AIRPLANE_API_KEY: &str = "ai.endpoint.airplane.api_key";
const ONLINE_API_KEY_CONFIGURED: &str = "ai.endpoint.online.api_key_configured";
const AIRPLANE_API_KEY_CONFIGURED: &str = "ai.endpoint.airplane.api_key_configured";
const DEFAULTS: &[(&str, &str)] = &[
    ("editor.default_mode", "markdown"),
    ("editor.diff_view_style", "inline"),
    ("editor.wrap_long_lines", "true"),
    ("editor.hide_unchanged_regions", "false"),
    ("ai.endpoint.online.url", ""),
    ("ai.endpoint.online.model", ""),
    ("ai.endpoint.online.title_model", ""),
    ("ai.endpoint.online.image_model", ""),
    ("ai.endpoint.online.chat_supports_tools", ""),
    ("ai.endpoint.online.image_supports_vision", ""),
    (ONLINE_API_KEY, ""),
    ("ai.endpoint.airplane.url", ""),
    ("ai.endpoint.airplane.model", ""),
    ("ai.endpoint.airplane.title_model", ""),
    ("ai.endpoint.airplane.image_model", ""),
    ("ai.endpoint.airplane.chat_supports_tools", ""),
    ("ai.endpoint.airplane.image_supports_vision", ""),
    (AIRPLANE_API_KEY, ""),
    ("ai.default_model", ""),
    ("ai.title_model", ""),
    ("ai.image_model", ""),
    ("ai.system_prompt", ""),
    ("mcp.http.enabled", "false"),
    ("data.automatic_rebuild", "true"),
];

pub fn get(conn: &Connection, key: &str) -> EngineResult<Option<String>> {
    match setting::get_setting_by_key(conn, key) {
        Ok(value) => Ok(Some(value.value)),
        Err(diesel::result::Error::NotFound) => Ok(None),
        Err(error) => Err(EngineError::Db(error)),
    }
}

pub fn get_effective(conn: &Connection, key: &str) -> EngineResult<Option<String>> {
    if key == ONLINE_API_KEY || key == AIRPLANE_API_KEY {
        let configured_key = if key == ONLINE_API_KEY {
            ONLINE_API_KEY_CONFIGURED
        } else {
            AIRPLANE_API_KEY_CONFIGURED
        };
        return Ok(Some(
            get(conn, configured_key)?
                .filter(|value| value == "true")
                .map_or_else(String::new, |_| "configured".to_string()),
        ));
    }
    if let Some(value) = get(conn, key)? {
        return Ok(Some(value));
    }
    if key == UI_LANGUAGE_KEY {
        return Ok(Some(crate::i18n::detect_os_locale().code().to_string()));
    }
    Ok(DEFAULTS
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, value)| (*value).to_string()))
}

pub fn list_effective(conn: &Connection) -> EngineResult<BTreeMap<String, String>> {
    let mut values = DEFAULTS
        .iter()
        .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
        .collect::<BTreeMap<_, _>>();
    values.insert(
        UI_LANGUAGE_KEY.to_string(),
        crate::i18n::detect_os_locale().code().to_string(),
    );
    for value in setting::list_all_settings(conn)? {
        if !value.key.starts_with("app.")
            && value.key != ONLINE_API_KEY
            && value.key != ONLINE_API_KEY_CONFIGURED
            && value.key != AIRPLANE_API_KEY
            && value.key != AIRPLANE_API_KEY_CONFIGURED
        {
            values.insert(value.key, value.value);
        }
    }
    if get(conn, ONLINE_API_KEY_CONFIGURED)?.as_deref() == Some("true") {
        values.insert(ONLINE_API_KEY.to_string(), "configured".to_string());
    }
    if get(conn, AIRPLANE_API_KEY_CONFIGURED)?.as_deref() == Some("true") {
        values.insert(AIRPLANE_API_KEY.to_string(), "configured".to_string());
    }
    Ok(values)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;

    #[test]
    fn effective_defaults_exclude_removed_style_settings() {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();

        let values = list_effective(db.conn()).unwrap();

        let removed_prefix = ["style", "."].concat();
        assert!(!values.keys().any(|key| key.starts_with(&removed_prefix)));
    }
}
