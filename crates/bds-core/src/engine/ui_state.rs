use serde::{Deserialize, Serialize};

use crate::db::DbConnection as Connection;
use crate::db::queries::{project, setting};
use crate::engine::{EngineError, EngineResult};
use crate::util::now_unix_ms;

const KEY_SUFFIX: &str = "ui_state";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectUiState {
    pub sidebar_view: String,
    pub sidebar_visible: bool,
    pub sidebar_width: f32,
    pub panel_visible: bool,
    pub panel_tab: String,
    pub tabs: Vec<PersistedTab>,
    pub active_tab: Option<String>,
}

impl Default for ProjectUiState {
    fn default() -> Self {
        Self {
            sidebar_view: "posts".to_string(),
            sidebar_visible: true,
            sidebar_width: 280.0,
            panel_visible: false,
            panel_tab: "tasks".to_string(),
            tabs: Vec::new(),
            active_tab: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PersistedTab {
    pub tab_type: String,
    pub id: String,
    pub title: String,
    pub is_transient: bool,
}

pub fn load(conn: &Connection, project_id: &str) -> EngineResult<Option<ProjectUiState>> {
    match setting::get_setting_by_key(conn, &key(project_id)) {
        Ok(setting) => Ok(Some(serde_json::from_str(&setting.value)?)),
        Err(diesel::result::Error::NotFound) => Ok(None),
        Err(error) => Err(EngineError::Db(error)),
    }
}

pub fn save(conn: &Connection, project_id: &str, state: &ProjectUiState) -> EngineResult<()> {
    match project::get_project_by_id(conn, project_id) {
        Ok(_) => {}
        Err(diesel::result::Error::NotFound) => {
            return Err(EngineError::NotFound(format!("project {project_id}")));
        }
        Err(error) => return Err(EngineError::Db(error)),
    }
    let serialized = serde_json::to_string(state)?;
    setting::set_setting_value(conn, &key(project_id), &serialized, now_unix_ms())?;
    Ok(())
}

fn key(project_id: &str) -> String {
    format!("project:{project_id}:{KEY_SUFFIX}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::db::queries::project::{insert_project, make_test_project};

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        insert_project(db.conn(), &make_test_project("p1", "one")).unwrap();
        insert_project(db.conn(), &make_test_project("p2", "two")).unwrap();
        db
    }

    #[test]
    fn round_trips_independent_project_sessions() {
        let db = setup();
        let one = ProjectUiState {
            sidebar_view: "media".into(),
            sidebar_visible: false,
            sidebar_width: 412.0,
            panel_visible: true,
            panel_tab: "output".into(),
            tabs: vec![PersistedTab {
                tab_type: "post".into(),
                id: "post-1".into(),
                title: "Post one".into(),
                is_transient: false,
            }],
            active_tab: Some("post-1".into()),
        };
        let two = ProjectUiState {
            sidebar_view: "scripts".into(),
            tabs: vec![PersistedTab {
                tab_type: "scripts".into(),
                id: "script-2".into(),
                title: "Script two".into(),
                is_transient: true,
            }],
            active_tab: Some("script-2".into()),
            ..ProjectUiState::default()
        };

        save(db.conn(), "p1", &one).unwrap();
        save(db.conn(), "p2", &two).unwrap();

        assert_eq!(load(db.conn(), "p1").unwrap(), Some(one));
        assert_eq!(load(db.conn(), "p2").unwrap(), Some(two));
        assert_eq!(load(db.conn(), "missing").unwrap(), None);
    }

    #[test]
    fn missing_newer_fields_use_safe_defaults() {
        let db = setup();
        setting::set_setting_value(db.conn(), &key("p1"), r#"{"sidebar_view":"media"}"#, 1)
            .unwrap();

        let state = load(db.conn(), "p1").unwrap().unwrap();
        assert_eq!(state.sidebar_view, "media");
        assert!(state.sidebar_visible);
        assert_eq!(state.sidebar_width, 280.0);
        assert!(state.tabs.is_empty());
    }

    #[test]
    fn deleting_a_project_removes_its_session() {
        let db = setup();
        save(db.conn(), "p1", &ProjectUiState::default()).unwrap();

        crate::engine::project::delete_project(db.conn(), "p1", None).unwrap();

        assert_eq!(load(db.conn(), "p1").unwrap(), None);
        assert!(matches!(
            save(db.conn(), "p1", &ProjectUiState::default()),
            Err(EngineError::NotFound(_))
        ));
    }
}
