use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

/// Write `content` to `path` atomically: write to a temp file in the same
/// directory, then rename. Creates parent directories if missing.
pub fn atomic_write(path: &Path, content: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;

    loop {
        let mut temp_name = OsString::from(".");
        temp_name.push(file_name);
        temp_name.push(format!(
            ".{}.{}.tmp",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let tmp_path = parent.join(temp_name);
        let mut file = match fs::File::create_new(&tmp_path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        };
        let result = (|| {
            file.write_all(content)?;
            file.sync_all()?;
            drop(file);
            fs::rename(&tmp_path, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp_path);
        }
        return result;
    }
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

    #[test]
    fn concurrent_sibling_writes_do_not_share_a_temp_file() {
        let dir = TempDir::new().unwrap();
        let markdown = dir.path().join("a.en.md");
        let metadata = dir.path().join("a.en.meta");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));

        let write = |path: std::path::PathBuf, content: &'static str| {
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                atomic_write_str(&path, content)
            })
        };
        let first = write(markdown.clone(), "markdown");
        let second = write(metadata.clone(), "metadata");

        first.join().unwrap().unwrap();
        second.join().unwrap().unwrap();
        assert_eq!(fs::read_to_string(markdown).unwrap(), "markdown");
        assert_eq!(fs::read_to_string(metadata).unwrap(), "metadata");
    }
}
