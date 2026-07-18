use std::fs;
use std::path::{Path, PathBuf};

const SQL_PREFIXES: &[&str] = &[
    "\"SELECT ",
    "\"INSERT ",
    "\"UPDATE ",
    "\"DELETE ",
    "\"CREATE ",
    "\"DROP ",
    "\"ALTER ",
    "\"SAVEPOINT ",
    "\"RELEASE ",
    "\"ROLLBACK ",
];

#[test]
fn application_queries_use_the_diesel_abstraction() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut rust_files = Vec::new();
    collect_rust_files(&source_root, &mut rust_files);

    let mut violations = Vec::new();
    for path in rust_files {
        // SQLite connection configuration and FTS5 virtual tables/MATCH have no
        // representation in Diesel's typed query builder.
        if path.ends_with("db/connection.rs") || path.ends_with("db/fts.rs") {
            continue;
        }

        let source = fs::read_to_string(&path).unwrap();
        for (line_index, line) in source.lines().enumerate() {
            if SQL_PREFIXES.iter().any(|prefix| line.contains(prefix)) {
                violations.push(format!(
                    "{}:{}: {}",
                    path.display(),
                    line_index + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "literal SQL escaped the backend boundary:\n{}",
        violations.join("\n")
    );
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
