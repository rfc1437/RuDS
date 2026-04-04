use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Write `content` to `path` atomically: write to a temp file in the same
/// directory, then rename. Creates parent directories if missing.
pub fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp_path = path.with_extension("tmp");
    let mut file = fs::File::create(&tmp_path)?;
    file.write_all(content)?;
    file.sync_all()?;
    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Convenience wrapper for UTF-8 string content.
pub fn atomic_write_str(path: &Path, content: &str) -> io::Result<()> {
    atomic_write(path, content.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_and_read_back() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        atomic_write_str(&path, "hello world").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello world");
    }

    #[test]
    fn creates_parent_directories() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a").join("b").join("c.txt");
        atomic_write_str(&path, "nested").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "nested");
    }

    #[test]
    fn overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        atomic_write_str(&path, "v1").unwrap();
        atomic_write_str(&path, "v2").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "v2");
    }
}
