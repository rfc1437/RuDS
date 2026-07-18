use std::path::PathBuf;

/// Shared context passed to engine operations.
pub struct EngineContext<'a> {
    pub conn: &'a crate::db::DbConnection,
    pub project_id: String,
    pub data_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::TempDir;

    #[test]
    fn context_holds_references() {
        let db = Database::open_in_memory().unwrap();
        let dir = TempDir::new().unwrap();
        let ctx = EngineContext {
            conn: db.conn(),
            project_id: "p1".into(),
            data_dir: dir.path().to_path_buf(),
        };
        assert_eq!(ctx.project_id, "p1");
        assert!(ctx.data_dir.exists());
    }
}
