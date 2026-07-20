use std::{
    env,
    error::Error,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() -> Result<(), Box<dyn Error>> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if env::args_os().nth(1).as_deref() == Some(OsStr::new("sign-app-for-dmg")) {
        return sign_app_for_dmg(&workspace);
    }
    run(
        Command::new(env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
            .current_dir(&workspace)
            .args([
                "build",
                "--release",
                "-p",
                "bds-ui",
                "-p",
                "bds-cli",
                "-p",
                "bds-mcp",
            ]),
        "release build",
    )?;

    let target = env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));
    let release = target.join("release");
    let staging = release.join("bds-runtime");
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    let shared_libraries = shared_library_names().map(|name| release.join(name));
    for library in &shared_libraries {
        copy_runtime(library, &staging)?;
    }

    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let output = Command::new(rustc)
        .args(["--print", "target-libdir"])
        .output()?;
    if !output.status.success() {
        return Err("rustc --print target-libdir failed".into());
    }
    let rust_lib_dir = PathBuf::from(String::from_utf8(output.stdout)?.trim());
    let mut std_libraries = fs::read_dir(&rust_lib_dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_dynamic_std(path))
        .collect::<Vec<_>>();
    std_libraries.sort();
    if std_libraries.is_empty() {
        return Err(format!(
            "no dynamic Rust standard library found in {}",
            rust_lib_dir.display()
        )
        .into());
    }
    for library in &std_libraries {
        copy_runtime(&library, &staging)?;
    }
    #[cfg(target_os = "macos")]
    rewrite_macos_install_names(&release, &staging, &shared_libraries, &std_libraries)?;
    Ok(())
}

fn sign_app_for_dmg(workspace: &Path) -> Result<(), Box<dyn Error>> {
    #[cfg(target_os = "macos")]
    if env::var_os("CARGO_PACKAGER_FORMAT").as_deref() == Some(OsStr::new("dmg")) {
        let target = env::var_os("CARGO_TARGET_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace.join("target"));
        run(
            Command::new("codesign")
                .args(["--force", "--sign", "-"])
                .arg(target.join("release/Blogging Desktop Server.app")),
            "macOS app ad-hoc signing",
        )?;
    }
    Ok(())
}

fn run(command: &mut Command, description: &str) -> Result<(), Box<dyn Error>> {
    if command.status()?.success() {
        Ok(())
    } else {
        Err(format!("{description} failed").into())
    }
}

fn copy_runtime(source: &Path, staging: &Path) -> Result<(), Box<dyn Error>> {
    let name = source
        .file_name()
        .ok_or_else(|| format!("runtime path has no filename: {}", source.display()))?;
    let destination = staging.join(name);
    fs::copy(source, &destination)?;
    let mut permissions = fs::metadata(&destination)?.permissions();
    permissions.set_readonly(false);
    fs::set_permissions(&destination, permissions)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&destination, fs::Permissions::from_mode(0o755))?;
    }
    Ok(())
}

fn shared_library_names() -> [&'static str; 2] {
    if cfg!(target_os = "windows") {
        ["bds_core.dll", "bds_server.dll"]
    } else if cfg!(target_os = "macos") {
        ["libbds_core.dylib", "libbds_server.dylib"]
    } else {
        ["libbds_core.so", "libbds_server.so"]
    }
}

fn is_dynamic_std(path: &Path) -> bool {
    let name = path.file_name().and_then(OsStr::to_str).unwrap_or_default();
    if cfg!(target_os = "windows") {
        name.starts_with("std-") && name.ends_with(".dll")
    } else if cfg!(target_os = "macos") {
        name.starts_with("libstd-") && name.ends_with(".dylib")
    } else {
        name.starts_with("libstd-") && name.ends_with(".so")
    }
}

#[cfg(target_os = "macos")]
fn rewrite_macos_install_names(
    release: &Path,
    staging: &Path,
    shared_libraries: &[PathBuf],
    std_libraries: &[PathBuf],
) -> Result<(), Box<dyn Error>> {
    let mut replacements = Vec::new();
    for source in shared_libraries.iter().chain(std_libraries) {
        let old_name = macho_install_name(source)?;
        let file_name = source
            .file_name()
            .and_then(OsStr::to_str)
            .ok_or_else(|| format!("invalid dylib filename: {}", source.display()))?;
        replacements.push((old_name, format!("@rpath/{file_name}")));
    }

    let mut targets = ["bds-ui", "bds-cli", "bds-mcp"]
        .map(|name| release.join(name))
        .into_iter()
        .collect::<Vec<_>>();
    targets.extend(
        shared_libraries
            .iter()
            .filter_map(|source| source.file_name().map(|name| staging.join(name))),
    );
    targets.extend(
        std_libraries
            .iter()
            .filter_map(|source| source.file_name().map(|name| staging.join(name))),
    );

    for target in &targets {
        for (old_name, new_name) in &replacements {
            run(
                Command::new("install_name_tool")
                    .args(["-change", old_name, new_name])
                    .arg(target),
                "Mach-O dependency rewrite",
            )?;
        }
    }
    for (_, new_name) in &replacements {
        let file_name = new_name.trim_start_matches("@rpath/");
        run(
            Command::new("install_name_tool")
                .args(["-id", new_name])
                .arg(staging.join(file_name)),
            "Mach-O install-name rewrite",
        )?;
    }
    for target in targets {
        run(
            Command::new("codesign")
                .args(["--force", "--sign", "-"])
                .arg(target),
            "Mach-O ad-hoc signing",
        )?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn macho_install_name(library: &Path) -> Result<String, Box<dyn Error>> {
    let output = Command::new("otool").arg("-D").arg(library).output()?;
    if !output.status.success() {
        return Err(format!("otool -D failed for {}", library.display()).into());
    }
    String::from_utf8(output.stdout)?
        .lines()
        .nth(1)
        .map(str::to_owned)
        .ok_or_else(|| format!("no Mach-O install name in {}", library.display()).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_only_the_platform_dynamic_standard_library() {
        let valid = if cfg!(target_os = "windows") {
            "std-123.dll"
        } else if cfg!(target_os = "macos") {
            "libstd-123.dylib"
        } else {
            "libstd-123.so"
        };
        assert!(is_dynamic_std(Path::new(valid)));
        assert!(!is_dynamic_std(Path::new("libstd-123.rlib")));
        for library in shared_library_names() {
            assert!(!is_dynamic_std(Path::new(library)));
        }
    }
}
