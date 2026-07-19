use std::path::{Path, PathBuf};

use crate::engine::{EngineError, EngineResult};

/// Install a recoverable launcher pointing at a packaged `bds-cli` binary.
/// Existing unrelated files are never overwritten.
pub fn install_launcher(executable: &Path, home_dir: &Path) -> EngineResult<PathBuf> {
    if !executable.is_file() {
        return Err(EngineError::Validation(format!(
            "installing the CLI requires the packaged bds-cli executable (not found at {})",
            executable.display()
        )));
    }
    let bin_dir = home_dir.join(".local/bin");
    std::fs::create_dir_all(&bin_dir)?;
    let target = bin_dir.join(if cfg!(windows) {
        "bds-cli.exe"
    } else {
        "bds-cli"
    });
    if target.exists() {
        let existing = target.canonicalize().ok();
        let source = executable.canonicalize()?;
        if existing.as_ref() != Some(&source) {
            return Err(EngineError::Conflict(format!(
                "refusing to overwrite existing launcher at {}",
                target.display()
            )));
        }
        return Ok(target);
    }

    #[cfg(unix)]
    std::os::unix::fs::symlink(executable.canonicalize()?, &target)?;
    #[cfg(windows)]
    std::fs::copy(executable, &target)?;
    Ok(target)
}

/// Resolve the CLI shipped beside the desktop executable and install it.
pub fn install_packaged_launcher(home_dir: &Path) -> EngineResult<PathBuf> {
    let app = std::env::current_exe()?;
    let cli = app.with_file_name(if cfg!(windows) {
        "bds-cli.exe"
    } else {
        "bds-cli"
    });
    install_launcher(&cli, home_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_is_idempotent_and_never_overwrites_an_unrelated_file() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("packaged-bds-cli");
        std::fs::write(&executable, b"binary").unwrap();
        let home = root.path().join("home");
        let target = install_launcher(&executable, &home).unwrap();
        assert_eq!(
            target.canonicalize().unwrap(),
            executable.canonicalize().unwrap()
        );
        assert_eq!(install_launcher(&executable, &home).unwrap(), target);

        std::fs::remove_file(&target).unwrap();
        std::fs::write(&target, b"mine").unwrap();
        assert!(install_launcher(&executable, &home).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"mine");
    }
}
