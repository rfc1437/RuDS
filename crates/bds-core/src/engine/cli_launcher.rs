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
        "bds-cli.cmd"
    } else {
        "bds-cli"
    });
    let source = executable.canonicalize()?;
    let launcher = launcher_contents(&source);
    #[cfg(unix)]
    if target.is_symlink() && target.canonicalize().ok().as_ref() == Some(&source) {
        std::fs::remove_file(&target)?;
    }
    if target.exists() {
        if std::fs::read(&target).ok().as_deref() != Some(launcher.as_bytes()) {
            return Err(EngineError::Conflict(format!(
                "refusing to overwrite existing launcher at {}",
                target.display()
            )));
        }
        return Ok(target);
    }

    std::fs::write(&target, launcher)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mut permissions = std::fs::metadata(&target)?.permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&target, permissions)?;
    }
    Ok(target)
}

#[cfg(unix)]
fn launcher_contents(executable: &Path) -> String {
    let quoted = executable.to_string_lossy().replace('\'', "'\"'\"'");
    format!("#!/bin/sh\nexec '{quoted}' \"$@\"\n")
}

#[cfg(windows)]
fn launcher_contents(executable: &Path) -> String {
    let escaped = executable.to_string_lossy().replace('%', "%%");
    format!("@echo off\r\n\"{escaped}\" %*\r\n")
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
        assert!(!target.is_symlink());
        let launcher = std::fs::read_to_string(&target).unwrap();
        assert!(launcher.contains(executable.canonicalize().unwrap().to_str().unwrap()));
        assert_eq!(install_launcher(&executable, &home).unwrap(), target);

        std::fs::remove_file(&target).unwrap();
        std::fs::write(&target, b"mine").unwrap();
        assert!(install_launcher(&executable, &home).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"mine");
    }

    #[cfg(unix)]
    #[test]
    fn replaces_the_previous_installer_symlink_with_a_forwarding_launcher() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("packaged-bds-cli");
        std::fs::write(&executable, b"binary").unwrap();
        let home = root.path().join("home");
        let target = home.join(".local/bin/bds-cli");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&executable, &target).unwrap();

        assert_eq!(install_launcher(&executable, &home).unwrap(), target);
        assert!(!target.is_symlink());
        assert!(
            std::fs::read_to_string(target)
                .unwrap()
                .starts_with("#!/bin/sh")
        );
    }
}
