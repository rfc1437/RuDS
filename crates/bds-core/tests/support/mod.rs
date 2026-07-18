use std::fs;
use std::ops::Deref;
use std::path::PathBuf;

use bds_core::db::Database;
use tempfile::TempDir;

pub struct FixtureDatabase {
    database: Database,
    _directory: TempDir,
}

impl Deref for FixtureDatabase {
    type Target = Database;

    fn deref(&self) -> &Self::Target {
        &self.database
    }
}

pub fn fixture_database() -> FixtureDatabase {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/compatibility-projects/rfc1437-sample");
    let directory = TempDir::new().unwrap();

    for name in ["bds.db", "bds.db-shm", "bds.db-wal"] {
        fs::copy(source.join(name), directory.path().join(name)).unwrap();
    }

    FixtureDatabase {
        database: Database::open(&directory.path().join("bds.db")).unwrap(),
        _directory: directory,
    }
}
