use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::engine::{EngineError, EngineResult};
use crate::model::{PublishingPreferences, SshMode};
use crate::util::atomic_write_str;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishJobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadTargetKind {
    Html,
    Thumbnails,
    Media,
}

impl UploadTargetKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::Thumbnails => "thumbnails",
            Self::Media => "media",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadTarget {
    pub kind: UploadTargetKind,
    pub local_dir: PathBuf,
    pub remote_dir: String,
}

#[derive(Debug, Clone)]
pub struct PublishJob {
    pub ssh_host: String,
    pub ssh_user: String,
    pub ssh_remote_path: String,
    pub ssh_mode: SshMode,
    pub status: PublishJobStatus,
    pub completed_targets: Vec<UploadTargetKind>,
    pub error: Option<String>,
}

impl PublishJob {
    fn start(&mut self) -> EngineResult<()> {
        if self.status != PublishJobStatus::Pending {
            return Err(EngineError::Conflict(
                "publish job can only start while pending".into(),
            ));
        }
        self.status = PublishJobStatus::Running;
        Ok(())
    }

    fn complete(&mut self) -> EngineResult<()> {
        if self.status != PublishJobStatus::Running {
            return Err(EngineError::Conflict(
                "publish job can only complete while running".into(),
            ));
        }
        self.status = PublishJobStatus::Completed;
        Ok(())
    }

    fn fail(&mut self, error: impl Into<String>) {
        if self.status == PublishJobStatus::Running {
            self.status = PublishJobStatus::Failed;
            self.error = Some(error.into());
        }
    }
}

#[derive(Debug, Clone)]
struct Credentials {
    host: String,
    user: String,
    remote_path: String,
    mode: SshMode,
}

impl Credentials {
    fn from_preferences(preferences: &PublishingPreferences) -> EngineResult<Self> {
        let required = |value: &Option<String>, field: &str| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .ok_or_else(|| EngineError::Validation(format!("missing {field}")))
        };
        Ok(Self {
            host: required(&preferences.ssh_host, "SSH host")?,
            user: required(&preferences.ssh_user, "SSH user")?,
            remote_path: required(&preferences.ssh_remote_path, "SSH remote path")?,
            mode: preferences.ssh_mode.clone(),
        })
    }

    fn remote_base(&self) -> String {
        format!("{}@{}", self.user, self.host)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ScpMtimeCache(BTreeMap<String, u64>);

type CommandRunner<'a> = dyn FnMut(&str, &[String]) -> Result<(), String> + 'a;

/// Upload the generated site using non-interactive SSH-agent authentication.
pub fn upload_site(
    data_dir: &Path,
    private_cache_dir: &Path,
    preferences: &PublishingPreferences,
    mut on_progress: impl FnMut(usize, usize, UploadTargetKind),
) -> EngineResult<PublishJob> {
    if std::env::var_os("SSH_AUTH_SOCK").is_none() {
        return Err(EngineError::Validation(
            "SSH agent is unavailable (SSH_AUTH_SOCK is not set)".into(),
        ));
    }
    upload_site_with_runner(
        data_dir,
        private_cache_dir,
        preferences,
        &mut |program, args| run_command(program, args),
        &mut on_progress,
    )
}

fn upload_site_with_runner(
    data_dir: &Path,
    private_cache_dir: &Path,
    preferences: &PublishingPreferences,
    runner: &mut CommandRunner<'_>,
    on_progress: &mut dyn FnMut(usize, usize, UploadTargetKind),
) -> EngineResult<PublishJob> {
    let credentials = Credentials::from_preferences(preferences)?;
    let targets = build_upload_targets(data_dir, &credentials);
    let mut job = PublishJob {
        ssh_host: credentials.host.clone(),
        ssh_user: credentials.user.clone(),
        ssh_remote_path: credentials.remote_path.clone(),
        ssh_mode: credentials.mode.clone(),
        status: PublishJobStatus::Pending,
        completed_targets: Vec::new(),
        error: None,
    };
    job.start()?;

    let cache_path = private_cache_dir.join("publishing-scp-mtimes.json");
    let mut cache = read_cache(&cache_path);
    for (index, target) in targets.iter().enumerate() {
        on_progress(index + 1, targets.len(), target.kind);
        let result = match credentials.mode {
            SshMode::Rsync => upload_rsync(target, &credentials, runner),
            SshMode::Scp => upload_scp(target, &credentials, &mut cache, runner),
        };
        if let Err(error) = result {
            job.fail(error.clone());
            return Err(EngineError::Parse(error));
        }
        job.completed_targets.push(target.kind);
        if matches!(credentials.mode, SshMode::Scp) {
            write_cache(&cache_path, &cache)?;
        }
    }
    job.complete()?;
    Ok(job)
}

fn build_upload_targets(data_dir: &Path, credentials: &Credentials) -> Vec<UploadTarget> {
    let root = credentials.remote_path.trim_end_matches('/');
    vec![
        UploadTarget {
            kind: UploadTargetKind::Html,
            local_dir: data_dir.join("html"),
            remote_dir: root.to_owned(),
        },
        UploadTarget {
            kind: UploadTargetKind::Thumbnails,
            local_dir: data_dir.join("thumbnails"),
            remote_dir: format!("{root}/thumbnails"),
        },
        UploadTarget {
            kind: UploadTargetKind::Media,
            local_dir: data_dir.join("media"),
            remote_dir: format!("{root}/media"),
        },
    ]
}

fn upload_rsync(
    target: &UploadTarget,
    credentials: &Credentials,
    runner: &mut CommandRunner<'_>,
) -> Result<(), String> {
    if !target.local_dir.is_dir() {
        return Ok(());
    }
    let mut args = vec![
        "--update".into(),
        "--compress".into(),
        "--verbose".into(),
        "--recursive".into(),
        "--times".into(),
    ];
    if target.kind == UploadTargetKind::Media {
        args.push("--exclude=*.meta".into());
    }
    args.push(format!("{}/", target.local_dir.display()));
    args.push(format!(
        "{}:{}/",
        credentials.remote_base(),
        target.remote_dir.trim_end_matches('/')
    ));
    runner("rsync", &args)
}

fn upload_scp(
    target: &UploadTarget,
    credentials: &Credentials,
    cache: &mut ScpMtimeCache,
    runner: &mut CommandRunner<'_>,
) -> Result<(), String> {
    let files = list_target_files(target).map_err(|error| error.to_string())?;
    let mut remote_dirs = BTreeSet::new();
    let mut pending = Vec::new();
    for relative in files {
        let local = target.local_dir.join(&relative);
        let mtime = modified_seconds(&local).map_err(|error| error.to_string())?;
        let key = cache_key(credentials, target, &relative);
        if cache.0.get(&key).is_some_and(|recorded| *recorded >= mtime) {
            continue;
        }
        let remote_file = format!("{}/{}", target.remote_dir, relative.to_string_lossy());
        if let Some(parent) = Path::new(&remote_file).parent() {
            remote_dirs.insert(parent.to_string_lossy().to_string());
        }
        pending.push((local, remote_file, key, mtime));
    }
    if pending.is_empty() {
        return Ok(());
    }

    let mut mkdir_args = vec![credentials.remote_base(), "mkdir".into(), "-p".into()];
    mkdir_args.extend(remote_dirs);
    runner("ssh", &mkdir_args)?;

    for (local, remote_file, key, mtime) in pending {
        runner(
            "scp",
            &[
                "-q".into(),
                "-p".into(),
                local.to_string_lossy().to_string(),
                format!("{}:{remote_file}", credentials.remote_base()),
            ],
        )?;
        cache.0.insert(key, mtime);
    }
    Ok(())
}

fn list_target_files(target: &UploadTarget) -> EngineResult<Vec<PathBuf>> {
    if !target.local_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = WalkDir::new(&target.local_dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| {
            entry
                .path()
                .strip_prefix(&target.local_dir)
                .ok()
                .map(PathBuf::from)
        })
        .filter(|relative| {
            target.kind != UploadTargetKind::Media || !relative.to_string_lossy().ends_with(".meta")
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

fn cache_key(credentials: &Credentials, target: &UploadTarget, relative: &Path) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}",
        credentials.host,
        credentials.user,
        credentials.remote_path,
        target.kind.as_str(),
        target.remote_dir,
        relative.to_string_lossy()
    )
}

fn modified_seconds(path: &Path) -> std::io::Result<u64> {
    Ok(fs::metadata(path)?
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs())
}

fn read_cache(path: &Path) -> ScpMtimeCache {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
}

fn write_cache(path: &Path, cache: &ScpMtimeCache) -> EngineResult<()> {
    atomic_write_str(path, &serde_json::to_string(cache)?)?;
    Ok(())
}

fn run_command(program: &str, args: &[String]) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("failed to start {program}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            format!("{program} exited with {}", output.status)
        } else {
            stderr
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn preferences(mode: SshMode) -> PublishingPreferences {
        PublishingPreferences {
            ssh_host: Some("example.test".into()),
            ssh_user: Some("alice".into()),
            ssh_remote_path: Some("/srv/blog".into()),
            ssh_mode: mode,
        }
    }

    #[test]
    fn rsync_uses_required_flags_and_excludes_media_sidecars() {
        let dir = TempDir::new().unwrap();
        let cache = TempDir::new().unwrap();
        for subdir in ["html", "thumbnails", "media"] {
            fs::create_dir_all(dir.path().join(subdir)).unwrap();
            fs::write(dir.path().join(subdir).join("file.txt"), "x").unwrap();
        }
        let mut commands = Vec::<(String, Vec<String>)>::new();
        let job = upload_site_with_runner(
            dir.path(),
            cache.path(),
            &preferences(SshMode::Rsync),
            &mut |program, args| {
                commands.push((program.to_owned(), args.to_vec()));
                Ok(())
            },
            &mut |_, _, _| {},
        )
        .unwrap();

        assert_eq!(job.status, PublishJobStatus::Completed);
        assert_eq!(job.completed_targets.len(), 3);
        assert!(commands.iter().all(|(program, _)| program == "rsync"));
        assert!(commands.iter().all(|(_, args)| {
            args.contains(&"--update".into())
                && args.contains(&"--compress".into())
                && args.contains(&"--verbose".into())
        }));
        assert_eq!(
            commands
                .iter()
                .filter(|(_, args)| args.contains(&"--exclude=*.meta".into()))
                .count(),
            1
        );
    }

    #[test]
    fn scp_excludes_sidecars_and_skips_unchanged_files() {
        let dir = TempDir::new().unwrap();
        let cache = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("media/nested")).unwrap();
        fs::write(dir.path().join("media/nested/photo.jpg"), "image").unwrap();
        fs::write(dir.path().join("media/nested/photo.jpg.meta"), "metadata").unwrap();
        let prefs = preferences(SshMode::Scp);

        let mut first = Vec::<(String, Vec<String>)>::new();
        upload_site_with_runner(
            dir.path(),
            cache.path(),
            &prefs,
            &mut |program, args| {
                first.push((program.to_owned(), args.to_vec()));
                Ok(())
            },
            &mut |_, _, _| {},
        )
        .unwrap();
        assert_eq!(
            first.iter().filter(|(program, _)| program == "scp").count(),
            1
        );
        assert!(
            first
                .iter()
                .all(|(_, args)| !args.iter().any(|arg| arg.ends_with(".meta")))
        );

        let mut second = Vec::<String>::new();
        upload_site_with_runner(
            dir.path(),
            cache.path(),
            &prefs,
            &mut |program, _| {
                second.push(program.to_owned());
                Ok(())
            },
            &mut |_, _, _| {},
        )
        .unwrap();
        assert!(second.is_empty());
    }

    #[test]
    fn failed_target_does_not_complete_job() {
        let dir = TempDir::new().unwrap();
        let cache = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("html")).unwrap();
        fs::write(dir.path().join("html/index.html"), "x").unwrap();
        let error = upload_site_with_runner(
            dir.path(),
            cache.path(),
            &preferences(SshMode::Rsync),
            &mut |_, _| Err("network down".into()),
            &mut |_, _, _| {},
        )
        .unwrap_err();
        assert!(error.to_string().contains("network down"));
    }
}
