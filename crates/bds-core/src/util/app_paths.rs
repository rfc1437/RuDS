use std::path::PathBuf;

/// Machine-local application data directory shared by every RuDS surface.
pub fn application_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("bds")
}

/// SQLite cache/registry used by desktop, CLI, TUI, and remote surfaces.
pub fn application_database_path() -> PathBuf {
    application_data_dir().join("bds.db")
}

/// Default portable project folder used on first launch.
pub fn default_project_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("bds")
        .join("my-blog")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_is_inside_application_data_directory() {
        assert_eq!(
            application_database_path(),
            application_data_dir().join("bds.db")
        );
    }
}
