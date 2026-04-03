use rusqlite::Connection;

/// Run all embedded migrations against the given connection.
///
/// For M0, this is a stub. Once we have the full schema from the TypeScript
/// app's migrations, we will embed them via refinery.
pub fn run_migrations(conn: &Connection) -> Result<(), Box<dyn std::error::Error>> {
    // TODO(M1): Embed real migrations from the TypeScript app's schema.
    // For now, create the minimal tables needed for read-access verification.
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS projects (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            slug TEXT NOT NULL,
            description TEXT,
            data_path TEXT,
            is_active INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        ",
    )?;
    Ok(())
}
