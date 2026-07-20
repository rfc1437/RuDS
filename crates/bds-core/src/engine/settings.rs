use std::collections::BTreeMap;

use crate::db::DbConnection as Connection;
use crate::db::queries::setting;
use crate::engine::{EngineError, EngineResult, domain_events};
use crate::util::now_unix_ms;

pub const UI_LANGUAGE_KEY: &str = "ui.language";
pub const ONLINE_API_KEY: &str = "ai.endpoint.online.api_key";
const ONLINE_API_KEY_CONFIGURED: &str = "ai.endpoint.online.api_key_configured";
const DEFAULTS: &[(&str, &str)] = &[
    ("editor.default_mode", "markdown"),
    ("editor.diff_view_style", "inline"),
    ("editor.wrap_long_lines", "true"),
    ("editor.hide_unchanged_regions", "false"),
    ("ai.endpoint.online.url", ""),
    ("ai.endpoint.online.model", ""),
    (ONLINE_API_KEY, ""),
    ("ai.endpoint.airplane.url", ""),
    ("ai.endpoint.airplane.model", ""),
    ("ai.default_model", ""),
    ("ai.title_model", ""),
    ("ai.image_model", ""),
    ("ai.system_prompt", ""),
    ("mcp.http.enabled", "false"),
    ("data.automatic_rebuild", "true"),
    ("style.theme", "system"),
    ("style.content_width", "72"),
];

pub fn get(conn: &Connection, key: &str) -> EngineResult<Option<String>> {
    match setting::get_setting_by_key(conn, key) {
        Ok(value) => Ok(Some(value.value)),
        Err(diesel::result::Error::NotFound) => Ok(None),
        Err(error) => Err(EngineError::Db(error)),
    }
}

pub fn get_effective(conn: &Connection, key: &str) -> EngineResult<Option<String>> {
    if key == ONLINE_API_KEY {
        return Ok(Some(
            get(conn, ONLINE_API_KEY_CONFIGURED)?
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
        {
            values.insert(value.key, value.value);
        }
    }
    if get(conn, ONLINE_API_KEY_CONFIGURED)?.as_deref() == Some("true") {
        values.insert(ONLINE_API_KEY.to_string(), "configured".to_string());
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
