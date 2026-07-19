use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::db::DbConnection;
use crate::engine::{EngineError, EngineResult};

pub const LOCAL_TIMEOUT: Duration = Duration::from_secs(15);
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);
pub const FILE_HISTORY_LIMIT: usize = 50;

const GITIGNORE_LINES: &[&str] = &[
    "/html/",
    "/thumbnails/",
    "/pagefind/",
    "/.DS_Store",
    "/node_modules/",
    "/deps/",
    "/_build/",
];

const LFS_PATTERNS: &[&str] = &[
    "*.jpg", "*.jpeg", "*.png", "*.gif", "*.webp", "*.svg", "*.tif", "*.tiff", "*.bmp", "*.heic",
    "*.heif",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitProvider {
    GitHub,
    GitLab,
    GiteaForgejo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStatus {
    LocalOnly,
    RemoteOnly,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatusKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

impl FileStatusKind {
    pub fn code(self) -> &'static str {
        match self {
            Self::Added => "A",
            Self::Modified => "M",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Untracked => "U",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFileStatus {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: FileStatusKind,
    pub staged: bool,
    pub unstaged: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitCommit {
    pub hash: String,
    pub subject: Option<String>,
    pub author: Option<String>,
    pub date: Option<String>,
    pub sync_status: SyncStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepository {
    pub is_initialized: bool,
    pub remote_url: Option<String>,
    pub provider: Option<GitProvider>,
    pub current_branch: Option<String>,
    pub has_lfs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRemoteState {
    pub local_branch: Option<String>,
    pub upstream_branch: Option<String>,
    pub has_upstream: bool,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitDiff {
    pub staged: String,
    pub unstaged: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFileDiff {
    pub file_path: String,
    pub original: String,
    pub modified: String,
    pub patch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitOperation {
    Initialize,
    Status,
    Diff,
    History,
    Remote,
    Fetch,
    Pull,
    Push,
    Commit,
    Lfs,
    Reconcile,
}

impl fmt::Display for GitOperation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Initialize => "initialize",
            Self::Status => "status",
            Self::Diff => "diff",
            Self::History => "history",
            Self::Remote => "remote",
            Self::Fetch => "fetch",
            Self::Pull => "pull",
            Self::Push => "push",
            Self::Commit => "commit",
            Self::Lfs => "LFS",
            Self::Reconcile => "reconcile",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitPlatform {
    MacOs,
    Windows,
    Linux,
}

impl fmt::Display for GitPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::MacOs => "macOS",
            Self::Windows => "Windows",
            Self::Linux => "Linux",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitError {
    Io {
        operation: GitOperation,
        message: String,
    },
    Validation(String),
    Failed {
        operation: GitOperation,
        output: String,
    },
    TimedOut {
        operation: GitOperation,
        timeout: Duration,
        output: String,
    },
    Cancelled {
        operation: GitOperation,
        output: String,
    },
    Authentication {
        operation: GitOperation,
        provider: Option<GitProvider>,
        platform: GitPlatform,
        guidance: String,
    },
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { operation, message } => write!(f, "Git {operation} failed: {message}"),
            Self::Validation(message) => f.write_str(message),
            Self::Failed { operation, output } => {
                write!(f, "Git {operation} failed: {output}")
            }
            Self::TimedOut {
                operation, timeout, ..
            } => write!(
                f,
                "Git {operation} timed out after {}ms",
                timeout.as_millis()
            ),
            Self::Cancelled { operation, .. } => write!(f, "Git {operation} was cancelled"),
            Self::Authentication { guidance, .. } => f.write_str(guidance),
        }
    }
}

impl std::error::Error for GitError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitOutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitOutput {
    pub stream: GitOutputStream,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkResult {
    pub output: String,
}

#[derive(Debug, Clone)]
pub struct GitEngine {
    repository_dir: PathBuf,
    executable: PathBuf,
    local_timeout: Duration,
    network_timeout: Duration,
}

impl GitEngine {
    pub fn new(repository_dir: impl Into<PathBuf>) -> Self {
        Self {
            repository_dir: repository_dir.into(),
            executable: PathBuf::from("git"),
            local_timeout: LOCAL_TIMEOUT,
            network_timeout: NETWORK_TIMEOUT,
        }
    }

    pub fn repository(&self) -> Result<GitRepository, GitError> {
        if !self.repository_dir.join(".git").exists() {
            return Ok(GitRepository {
                is_initialized: false,
                remote_url: None,
                provider: None,
                current_branch: None,
                has_lfs: false,
            });
        }

        let remote_url =
            self.optional_output(&["remote", "get-url", "origin"], GitOperation::Remote)?;
        Ok(GitRepository {
            is_initialized: true,
            provider: remote_url.as_deref().map(provider_from_url),
            remote_url,
            current_branch: self.current_branch()?,
            has_lfs: self.has_lfs_tracking(),
        })
    }

    pub fn initialize(&self) -> Result<GitRepository, GitError> {
        fs::create_dir_all(&self.repository_dir)
            .map_err(|error| self.io_error(GitOperation::Initialize, error))?;
        self.run_local(&["init", "-b", "master"], GitOperation::Initialize)?;
        self.run_local(&["lfs", "install", "--local"], GitOperation::Lfs)?;

        let mut args = vec!["lfs", "track"];
        args.extend(LFS_PATTERNS.iter().copied());
        self.run_local(&args, GitOperation::Lfs)?;
        append_missing_lines(&self.repository_dir.join(".gitignore"), GITIGNORE_LINES)
            .map_err(|error| self.io_error(GitOperation::Initialize, error))?;
        append_lfs_lines(&self.repository_dir.join(".gitattributes"))
            .map_err(|error| self.io_error(GitOperation::Lfs, error))?;
        self.repository()
    }

    pub fn status(&self) -> Result<Vec<GitFileStatus>, GitError> {
        let output = self.run_local_bytes(
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
            GitOperation::Status,
        )?;
        Ok(parse_status(&output))
    }

    pub fn diff(&self) -> Result<GitDiff, GitError> {
        Ok(GitDiff {
            staged: self.run_local(
                &["diff", "--cached", "--no-ext-diff", "--no-color"],
                GitOperation::Diff,
            )?,
            unstaged: self
                .run_local(&["diff", "--no-ext-diff", "--no-color"], GitOperation::Diff)?,
        })
    }

    pub fn file_diff(&self, file_path: &str) -> Result<GitFileDiff, GitError> {
        validate_relative_path(file_path)?;
        let revision = format!("HEAD:{file_path}");
        let original = self
            .optional_raw_output(&["show", &revision], GitOperation::Diff)?
            .unwrap_or_default();
        let modified = match fs::read(self.repository_dir.join(file_path)) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(self.io_error(GitOperation::Diff, error)),
        };
        let mut patch = self
            .optional_raw_output(
                &[
                    "diff",
                    "HEAD",
                    "--no-ext-diff",
                    "--no-color",
                    "--",
                    file_path,
                ],
                GitOperation::Diff,
            )?
            .unwrap_or_default();
        if patch.is_empty() && original.is_empty() && !modified.is_empty() {
            patch = added_file_patch(file_path, &modified);
        }
        Ok(GitFileDiff {
            file_path: file_path.to_string(),
            original,
            modified,
            patch,
        })
    }

    pub fn commit_files(&self, hash: &str) -> Result<Vec<ChangedFile>, GitError> {
        if !is_object_name(hash) {
            return Err(GitError::Validation("invalid commit identifier".into()));
        }
        let output = self.run_local_bytes(
            &[
                "diff-tree",
                "--root",
                "--no-commit-id",
                "--name-status",
                "-r",
                "-z",
                "--find-renames",
                hash,
            ],
            GitOperation::Diff,
        )?;
        Ok(parse_changed_files(&output))
    }

    pub fn commit_file_diff(
        &self,
        hash: &str,
        change: &ChangedFile,
    ) -> Result<GitFileDiff, GitError> {
        if !is_object_name(hash) {
            return Err(GitError::Validation("invalid commit identifier".into()));
        }
        validate_relative_path(&change.path)?;
        if let Some(old_path) = &change.old_path {
            validate_relative_path(old_path)?;
        }
        let old_path = change.old_path.as_deref().unwrap_or(&change.path);
        let parent_revision = format!("{hash}^:{old_path}");
        let revision = format!("{hash}:{}", change.path);
        let original = if change.kind == FileStatusKind::Added {
            String::new()
        } else {
            self.run_local(&["show", &parent_revision], GitOperation::Diff)?
        };
        let modified = if change.kind == FileStatusKind::Deleted {
            String::new()
        } else {
            self.run_local(&["show", &revision], GitOperation::Diff)?
        };
        let patch = self.run_local(
            &[
                "show",
                "--format=",
                "--no-ext-diff",
                "--no-color",
                hash,
                "--",
                &change.path,
            ],
            GitOperation::Diff,
        )?;
        Ok(GitFileDiff {
            file_path: change.path.clone(),
            original,
            modified,
            patch,
        })
    }

    pub fn history(&self, branch: &str) -> Result<Vec<GitCommit>, GitError> {
        if branch.trim().is_empty() {
            return Ok(Vec::new());
        }
        let local = self.history_for_revision_optional(branch)?;
        let upstream = self.upstream_branch()?;
        let remote = match upstream {
            Some(upstream) => self.history_for_revision_optional(&upstream)?,
            None => Vec::new(),
        };
        let local_hashes = local
            .iter()
            .map(|commit| commit.hash.clone())
            .collect::<HashSet<_>>();
        let remote_hashes = remote
            .iter()
            .map(|commit| commit.hash.clone())
            .collect::<HashSet<_>>();
        let mut commits = local
            .into_iter()
            .map(|mut commit| {
                commit.sync_status = if remote_hashes.contains(&commit.hash) {
                    SyncStatus::Both
                } else {
                    SyncStatus::LocalOnly
                };
                commit
            })
            .collect::<Vec<_>>();
        commits.extend(remote.into_iter().filter_map(|mut commit| {
            (!local_hashes.contains(&commit.hash)).then(|| {
                commit.sync_status = SyncStatus::RemoteOnly;
                commit
            })
        }));
        Ok(commits)
    }

    pub fn file_history(&self, file_path: &str) -> Result<Vec<GitCommit>, GitError> {
        validate_relative_path(file_path)?;
        let limit = FILE_HISTORY_LIMIT.to_string();
        let output = self.optional_output(
            &[
                "log",
                "--follow",
                "--date=short",
                "--format=%H%x1f%an%x1f%ad%x1f%s%x1e",
                "-n",
                &limit,
                "--",
                file_path,
            ],
            GitOperation::History,
        )?;
        Ok(output.map_or_else(Vec::new, |output| parse_history(&output)))
    }

    pub fn commit_diff(&self, hash: &str) -> Result<String, GitError> {
        if !is_object_name(hash) {
            return Err(GitError::Validation("invalid commit identifier".into()));
        }
        self.run_local(
            &[
                "show",
                "--format=fuller",
                "--no-ext-diff",
                "--no-color",
                hash,
            ],
            GitOperation::Diff,
        )
    }

    pub fn remote_state(&self) -> Result<GitRemoteState, GitError> {
        let local_branch = self.current_branch()?;
        let upstream_branch = self.upstream_branch()?;
        let Some(upstream) = upstream_branch.clone() else {
            return Ok(GitRemoteState {
                local_branch,
                upstream_branch: None,
                has_upstream: false,
                ahead: 0,
                behind: 0,
            });
        };
        Ok(GitRemoteState {
            local_branch,
            upstream_branch,
            has_upstream: true,
            ahead: self.revision_count(&format!("{upstream}..HEAD"))?,
            behind: self.revision_count(&format!("HEAD..{upstream}"))?,
        })
    }

    pub fn set_remote(&self, remote_url: &str) -> Result<(), GitError> {
        let remote_url = remote_url.trim();
        if remote_url.is_empty() {
            return Err(GitError::Validation("remote URL is required".into()));
        }
        let args = if self
            .optional_output(&["remote", "get-url", "origin"], GitOperation::Remote)?
            .is_some()
        {
            ["remote", "set-url", "origin", remote_url]
        } else {
            ["remote", "add", "origin", remote_url]
        };
        self.run_local(&args, GitOperation::Remote).map(|_| ())
    }

    pub fn commit_all(&self, message: &str) -> Result<NetworkResult, GitError> {
        let message = message.trim();
        if message.is_empty() {
            return Err(GitError::Validation("commit message is required".into()));
        }
        self.run_local(&["add", "-A"], GitOperation::Commit)?;
        let output = self.run_local(&["commit", "-m", message], GitOperation::Commit)?;
        Ok(NetworkResult { output })
    }

    pub fn fetch(
        &self,
        is_cancelled: impl Fn() -> bool,
        mut on_output: impl FnMut(GitOutput),
    ) -> Result<NetworkResult, GitError> {
        self.run_network(
            &["fetch", "--all", "--prune", "--progress"],
            GitOperation::Fetch,
            &is_cancelled,
            &mut on_output,
        )
    }

    pub fn pull(
        &self,
        is_cancelled: impl Fn() -> bool,
        mut on_output: impl FnMut(GitOutput),
    ) -> Result<NetworkResult, GitError> {
        self.run_network(
            &["pull", "--ff-only", "--progress"],
            GitOperation::Pull,
            &is_cancelled,
            &mut on_output,
        )
    }

    pub fn push(
        &self,
        is_cancelled: impl Fn() -> bool,
        mut on_output: impl FnMut(GitOutput),
    ) -> Result<NetworkResult, GitError> {
        self.run_network(
            &["push", "--progress"],
            GitOperation::Push,
            &is_cancelled,
            &mut on_output,
        )
    }

    pub fn prune_lfs_cache(&self, retain_recent_days: u32) -> Result<NetworkResult, GitError> {
        let retention = format!("lfs.fetchrecentcommitsdays={retain_recent_days}");
        let output = self.run_local(
            &["-c", &retention, "lfs", "prune", "--recent"],
            GitOperation::Lfs,
        )?;
        Ok(NetworkResult { output })
    }

    pub fn head(&self) -> Result<Option<String>, GitError> {
        self.optional_output(&["rev-parse", "--verify", "HEAD"], GitOperation::History)
    }

    pub fn changed_files(
        &self,
        old_commit: &str,
        new_commit: &str,
    ) -> Result<Vec<ChangedFile>, GitError> {
        if !is_object_name(old_commit) || !is_object_name(new_commit) {
            return Err(GitError::Validation("invalid commit identifier".into()));
        }
        let output = self.run_local_bytes(
            &[
                "diff",
                "--name-status",
                "-z",
                "--find-renames",
                old_commit,
                new_commit,
            ],
            GitOperation::Reconcile,
        )?;
        Ok(parse_changed_files(&output))
    }

    fn current_branch(&self) -> Result<Option<String>, GitError> {
        self.optional_output(
            &["symbolic-ref", "--quiet", "--short", "HEAD"],
            GitOperation::History,
        )
    }

    fn upstream_branch(&self) -> Result<Option<String>, GitError> {
        self.optional_output(
            &[
                "rev-parse",
                "--abbrev-ref",
                "--symbolic-full-name",
                "@{upstream}",
            ],
            GitOperation::Remote,
        )
    }

    fn revision_count(&self, range: &str) -> Result<usize, GitError> {
        Ok(self
            .optional_output(&["rev-list", "--count", range], GitOperation::History)?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0))
    }

    fn history_for_revision_optional(&self, revision: &str) -> Result<Vec<GitCommit>, GitError> {
        Ok(self
            .optional_output(
                &[
                    "log",
                    "--date=short",
                    "--format=%H%x1f%an%x1f%ad%x1f%s%x1e",
                    revision,
                ],
                GitOperation::History,
            )?
            .map_or_else(Vec::new, |output| parse_history(&output)))
    }

    fn has_lfs_tracking(&self) -> bool {
        fs::read_to_string(self.repository_dir.join(".gitattributes")).is_ok_and(|contents| {
            LFS_PATTERNS.iter().all(|pattern| {
                contents
                    .lines()
                    .any(|line| line.starts_with(pattern) && line.contains("filter=lfs"))
            })
        })
    }

    fn optional_output(
        &self,
        args: &[&str],
        operation: GitOperation,
    ) -> Result<Option<String>, GitError> {
        match self.run_command(args, operation, self.local_timeout, &|| false, &mut |_| {}) {
            Ok(result) if result.status == 0 => Ok(nonblank(result.stdout)),
            Ok(_) => Ok(None),
            Err(GitError::Failed { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn optional_raw_output(
        &self,
        args: &[&str],
        operation: GitOperation,
    ) -> Result<Option<String>, GitError> {
        match self.run_command(args, operation, self.local_timeout, &|| false, &mut |_| {}) {
            Ok(result) if result.status == 0 => Ok(Some(result.stdout)),
            Ok(_) | Err(GitError::Failed { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn run_local(&self, args: &[&str], operation: GitOperation) -> Result<String, GitError> {
        let result =
            self.run_command(args, operation, self.local_timeout, &|| false, &mut |_| {})?;
        self.success_or_error(operation, result)
            .map(|result| result.stdout)
    }

    fn run_local_bytes(&self, args: &[&str], operation: GitOperation) -> Result<Vec<u8>, GitError> {
        let result =
            self.run_command(args, operation, self.local_timeout, &|| false, &mut |_| {})?;
        self.success_or_error(operation, result)
            .map(|result| result.stdout_bytes)
    }

    fn run_network(
        &self,
        args: &[&str],
        operation: GitOperation,
        is_cancelled: &dyn Fn() -> bool,
        on_output: &mut dyn FnMut(GitOutput),
    ) -> Result<NetworkResult, GitError> {
        let result = self.run_command(
            args,
            operation,
            self.network_timeout,
            is_cancelled,
            on_output,
        )?;
        match self.success_or_error(operation, result) {
            Ok(result) => Ok(NetworkResult {
                output: combined_output(&result.stdout, &result.stderr),
            }),
            Err(GitError::Failed { output, .. }) if is_auth_error(&output) => {
                let provider = self
                    .optional_output(&["remote", "get-url", "origin"], GitOperation::Remote)?
                    .as_deref()
                    .map(provider_from_url);
                let platform = current_platform();
                Err(GitError::Authentication {
                    operation,
                    provider,
                    platform,
                    guidance: auth_guidance(provider, platform),
                })
            }
            Err(error) => Err(error),
        }
    }

    fn run_command(
        &self,
        args: &[&str],
        operation: GitOperation,
        timeout: Duration,
        is_cancelled: &dyn Fn() -> bool,
        on_output: &mut dyn FnMut(GitOutput),
    ) -> Result<CommandResult, GitError> {
        let mut command = Command::new(&self.executable);
        command
            .args(args)
            .current_dir(&self.repository_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GCM_INTERACTIVE", "never")
            .env("GIT_SSH_COMMAND", "ssh -oBatchMode=yes")
            .env("GIT_LFS_SKIP_SMUDGE", "1")
            .env("LC_ALL", "C")
            .env("LANG", "C");
        configure_process_group(&mut command);
        prepend_tool_paths(&mut command);

        let mut child = command
            .spawn()
            .map_err(|error| self.io_error(operation, error))?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let (sender, receiver) = mpsc::channel();
        let stdout_thread = read_stream(stdout, GitOutputStream::Stdout, sender.clone());
        let stderr_thread = read_stream(stderr, GitOutputStream::Stderr, sender);
        let start = Instant::now();
        let mut stdout_bytes = Vec::new();
        let mut stderr_bytes = Vec::new();

        loop {
            drain_output(&receiver, &mut stdout_bytes, &mut stderr_bytes, on_output);
            if is_cancelled() {
                terminate_child(&mut child);
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                drain_output(&receiver, &mut stdout_bytes, &mut stderr_bytes, on_output);
                return Err(GitError::Cancelled {
                    operation,
                    output: combined_bytes(&stdout_bytes, &stderr_bytes),
                });
            }
            if start.elapsed() >= timeout {
                terminate_child(&mut child);
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                drain_output(&receiver, &mut stdout_bytes, &mut stderr_bytes, on_output);
                return Err(GitError::TimedOut {
                    operation,
                    timeout,
                    output: combined_bytes(&stdout_bytes, &stderr_bytes),
                });
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    let _ = stdout_thread.join();
                    let _ = stderr_thread.join();
                    drain_output(&receiver, &mut stdout_bytes, &mut stderr_bytes, on_output);
                    return Ok(CommandResult {
                        status: status.code().unwrap_or(-1),
                        stdout: String::from_utf8_lossy(&stdout_bytes).into_owned(),
                        stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
                        stdout_bytes,
                    });
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(error) => return Err(self.io_error(operation, error)),
            }
        }
    }

    fn success_or_error(
        &self,
        operation: GitOperation,
        result: CommandResult,
    ) -> Result<CommandResult, GitError> {
        if result.status == 0 {
            Ok(result)
        } else {
            Err(GitError::Failed {
                operation,
                output: combined_output(&result.stdout, &result.stderr),
            })
        }
    }

    fn io_error(&self, operation: GitOperation, error: std::io::Error) -> GitError {
        GitError::Io {
            operation,
            message: error.to_string(),
        }
    }

    #[cfg(test)]
    fn with_executable_and_timeouts(
        repository_dir: impl Into<PathBuf>,
        executable: impl Into<PathBuf>,
        local_timeout: Duration,
        network_timeout: Duration,
    ) -> Self {
        Self {
            repository_dir: repository_dir.into(),
            executable: executable.into(),
            local_timeout,
            network_timeout,
        }
    }
}

struct CommandResult {
    status: i32,
    stdout: String,
    stderr: String,
    stdout_bytes: Vec<u8>,
}

fn read_stream(
    mut stream: impl Read + Send + 'static,
    kind: GitOutputStream,
    sender: mpsc::Sender<(GitOutputStream, Vec<u8>)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            match stream.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    if sender.send((kind, buffer[..read].to_vec())).is_err() {
                        break;
                    }
                }
            }
        }
    })
}

fn drain_output(
    receiver: &mpsc::Receiver<(GitOutputStream, Vec<u8>)>,
    stdout: &mut Vec<u8>,
    stderr: &mut Vec<u8>,
    on_output: &mut dyn FnMut(GitOutput),
) {
    while let Ok((stream, bytes)) = receiver.try_recv() {
        match stream {
            GitOutputStream::Stdout => stdout.extend_from_slice(&bytes),
            GitOutputStream::Stderr => stderr.extend_from_slice(&bytes),
        }
        on_output(GitOutput {
            stream,
            text: String::from_utf8_lossy(&bytes).into_owned(),
        });
    }
}

#[cfg(unix)]
fn configure_process_group(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
}

#[cfg(not(unix))]
fn configure_process_group(_command: &mut Command) {}

#[cfg(unix)]
fn terminate_child(child: &mut std::process::Child) {
    let group = format!("-{}", child.id());
    let _ = Command::new("kill").args(["-TERM", &group]).status();
    thread::sleep(Duration::from_millis(25));
    if child.try_wait().ok().flatten().is_none() {
        let _ = Command::new("kill").args(["-KILL", &group]).status();
    }
}

#[cfg(windows)]
fn terminate_child(child: &mut std::process::Child) {
    let _ = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .status();
    let _ = child.kill();
}

fn prepend_tool_paths(command: &mut Command) {
    #[cfg(target_os = "macos")]
    {
        let mut paths = vec![
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ];
        paths.extend(
            std::env::var_os("PATH")
                .iter()
                .flat_map(std::env::split_paths),
        );
        if let Ok(path) = std::env::join_paths(paths) {
            command.env("PATH", path);
        }
    }
}

fn append_missing_lines(path: &Path, required: &[&str]) -> std::io::Result<()> {
    let existing = fs::read_to_string(path).unwrap_or_default();
    let lines = existing.lines().collect::<HashSet<_>>();
    let missing = required
        .iter()
        .copied()
        .filter(|line| !lines.contains(line))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }
    let mut output = existing;
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str(&missing.join("\n"));
    output.push('\n');
    fs::write(path, output)
}

fn append_lfs_lines(path: &Path) -> std::io::Result<()> {
    let required = LFS_PATTERNS
        .iter()
        .map(|pattern| format!("{pattern} filter=lfs diff=lfs merge=lfs -text"))
        .collect::<Vec<_>>();
    let refs = required.iter().map(String::as_str).collect::<Vec<_>>();
    append_missing_lines(path, &refs)
}

fn parse_status(output: &[u8]) -> Vec<GitFileStatus> {
    let fields = output.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut files = Vec::new();
    let mut index = 0;
    while index < fields.len() {
        let field = fields[index];
        if field.len() < 4 {
            index += 1;
            continue;
        }
        let x = field[0] as char;
        let y = field[1] as char;
        let path = String::from_utf8_lossy(&field[3..]).into_owned();
        let renamed = matches!(x, 'R' | 'C') || matches!(y, 'R' | 'C');
        let old_path = if renamed {
            index += 1;
            fields
                .get(index)
                .filter(|old| !old.is_empty())
                .map(|old| String::from_utf8_lossy(old).into_owned())
        } else {
            None
        };
        let kind = if x == '?' && y == '?' {
            FileStatusKind::Untracked
        } else if renamed {
            FileStatusKind::Renamed
        } else if x == 'A' || y == 'A' {
            FileStatusKind::Added
        } else if x == 'D' || y == 'D' {
            FileStatusKind::Deleted
        } else {
            FileStatusKind::Modified
        };
        files.push(GitFileStatus {
            path,
            old_path,
            kind,
            staged: x != ' ' && x != '?',
            unstaged: (y != ' ' && y != '?') || (x == '?' && y == '?'),
        });
        index += 1;
    }
    files
}

fn parse_history(output: &str) -> Vec<GitCommit> {
    output
        .split('\x1e')
        .filter_map(|record| {
            let fields = record.trim().splitn(4, '\x1f').collect::<Vec<_>>();
            (!fields.first().copied().unwrap_or_default().is_empty()).then(|| GitCommit {
                hash: fields[0].to_string(),
                author: optional_field(fields.get(1).copied()),
                date: optional_field(fields.get(2).copied()),
                subject: optional_field(fields.get(3).copied()),
                sync_status: SyncStatus::LocalOnly,
            })
        })
        .collect()
}

fn optional_field(field: Option<&str>) -> Option<String> {
    field
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn provider_from_url(remote_url: &str) -> GitProvider {
    let url = remote_url.to_ascii_lowercase();
    if url.contains("github.com") {
        GitProvider::GitHub
    } else if url.contains("gitlab") {
        GitProvider::GitLab
    } else {
        GitProvider::GiteaForgejo
    }
}

fn current_platform() -> GitPlatform {
    if cfg!(target_os = "macos") {
        GitPlatform::MacOs
    } else if cfg!(windows) {
        GitPlatform::Windows
    } else {
        GitPlatform::Linux
    }
}

fn auth_guidance(provider: Option<GitProvider>, platform: GitPlatform) -> String {
    let provider = match provider {
        Some(GitProvider::GitHub) => "GitHub",
        Some(GitProvider::GitLab) => "GitLab",
        Some(GitProvider::GiteaForgejo) => "Gitea/Forgejo",
        None => "the Git provider",
    };
    let credential = match platform {
        GitPlatform::MacOs => "Keychain credential helper or an SSH key",
        GitPlatform::Windows => "Git Credential Manager or an SSH key",
        GitPlatform::Linux => "credential helper or an SSH key",
    };
    format!(
        "Authentication failed for {provider} on {platform}. Configure a {credential} that works non-interactively."
    )
}

fn is_auth_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    [
        "authentication failed",
        "permission denied",
        "could not read username",
        "terminal prompts disabled",
        "repository not found",
        "http basic: access denied",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

fn nonblank(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn combined_output(stdout: &str, stderr: &str) -> String {
    match (stdout.trim(), stderr.trim()) {
        ("", stderr) => stderr.to_string(),
        (stdout, "") => stdout.to_string(),
        (stdout, stderr) => format!("{stdout}\n{stderr}"),
    }
}

fn combined_bytes(stdout: &[u8], stderr: &[u8]) -> String {
    combined_output(
        &String::from_utf8_lossy(stdout),
        &String::from_utf8_lossy(stderr),
    )
}

fn added_file_patch(path: &str, contents: &str) -> String {
    let mut patch = format!("diff --git a/{path} b/{path}\n--- /dev/null\n+++ b/{path}\n");
    patch.push_str(
        &contents
            .lines()
            .map(|line| format!("+{line}"))
            .collect::<Vec<_>>()
            .join("\n"),
    );
    if contents.ends_with('\n') {
        patch.push('\n');
    }
    patch
}

fn validate_relative_path(path: &str) -> Result<(), GitError> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(GitError::Validation("invalid repository path".into()));
    }
    Ok(())
}

fn is_object_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.chars().all(|character| character.is_ascii_hexdigit())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedFile {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: FileStatusKind,
}

fn parse_changed_files(output: &[u8]) -> Vec<ChangedFile> {
    let fields = output.split(|byte| *byte == 0).collect::<Vec<_>>();
    let mut changes = Vec::new();
    let mut index = 0;
    while index < fields.len() {
        let status = String::from_utf8_lossy(fields[index]);
        if status.is_empty() {
            break;
        }
        index += 1;
        let Some(path) = fields.get(index).filter(|path| !path.is_empty()) else {
            break;
        };
        let first_path = String::from_utf8_lossy(path).into_owned();
        index += 1;
        let code = status.as_bytes()[0] as char;
        if matches!(code, 'R' | 'C') {
            let Some(new_path) = fields.get(index).filter(|path| !path.is_empty()) else {
                break;
            };
            changes.push(ChangedFile {
                path: String::from_utf8_lossy(new_path).into_owned(),
                old_path: Some(first_path),
                kind: FileStatusKind::Renamed,
            });
            index += 1;
        } else {
            changes.push(ChangedFile {
                path: first_path,
                old_path: None,
                kind: match code {
                    'A' => FileStatusKind::Added,
                    'D' => FileStatusKind::Deleted,
                    _ => FileStatusKind::Modified,
                },
            });
        }
    }
    changes
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReconcileEntityType {
    Post,
    PostTranslation,
    Script,
    Template,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconcileAction {
    Created,
    Updated,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileEvent {
    pub project_id: String,
    pub entity_type: ReconcileEntityType,
    pub entity_id: String,
    pub action: ReconcileAction,
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReconcileReport {
    pub events: Vec<ReconcileEvent>,
}

#[derive(Debug, Clone)]
struct EntityAtPath {
    entity_type: ReconcileEntityType,
    id: String,
}

pub fn reconcile_changed_files(
    conn: &DbConnection,
    data_dir: &Path,
    project_id: &str,
    changes: &[ChangedFile],
    mut emit: impl FnMut(&ReconcileEvent),
) -> EngineResult<ReconcileReport> {
    let before = entities_by_path(conn, project_id)?;
    let posts_changed = changes.iter().any(|change| relevant_path(change, "posts/"));
    let scripts_changed = changes
        .iter()
        .any(|change| relevant_path(change, "scripts/"));
    let templates_changed = changes
        .iter()
        .any(|change| relevant_path(change, "templates/"));

    if posts_changed {
        let report =
            crate::engine::post::rebuild_posts_from_filesystem(conn, data_dir, project_id)?;
        fail_rebuild_errors("posts", report.errors)?;
    }
    if scripts_changed {
        let report = crate::engine::script_rebuild::rebuild_scripts_from_filesystem(
            conn, data_dir, project_id,
        )?;
        fail_rebuild_errors("scripts", report.errors)?;
    }
    if templates_changed {
        let report = crate::engine::template_rebuild::rebuild_templates_from_filesystem(
            conn, data_dir, project_id,
        )?;
        fail_rebuild_errors("templates", report.errors)?;
    }

    let rebuilt = entities_by_path(conn, project_id)?;
    for change in changes {
        match change.kind {
            FileStatusKind::Deleted => {
                delete_entity_at_path(conn, before.get(&change.path))?;
            }
            FileStatusKind::Renamed => {
                if let Some(old_path) = change.old_path.as_deref() {
                    let entity = before.get(old_path).filter(|entity| {
                        !rebuilt.values().any(|current| {
                            current.id == entity.id && current.entity_type == entity.entity_type
                        })
                    });
                    delete_entity_at_path(conn, entity)?;
                }
            }
            _ => {}
        }
    }
    if posts_changed {
        crate::engine::post::rebuild_all_links(conn, data_dir, project_id)?;
    }

    let after = entities_by_path(conn, project_id)?;
    let mut events = Vec::new();
    for change in changes {
        if !is_reconciled_path(&change.path)
            && !change.old_path.as_deref().is_some_and(is_reconciled_path)
        {
            continue;
        }
        let event = match change.kind {
            FileStatusKind::Deleted => before.get(&change.path).map(|entity| ReconcileEvent {
                project_id: project_id.to_string(),
                entity_type: entity.entity_type,
                entity_id: entity.id.clone(),
                action: ReconcileAction::Deleted,
                path: change.path.clone(),
            }),
            FileStatusKind::Renamed => after.get(&change.path).map(|entity| ReconcileEvent {
                project_id: project_id.to_string(),
                entity_type: entity.entity_type,
                entity_id: entity.id.clone(),
                action: ReconcileAction::Renamed,
                path: change.path.clone(),
            }),
            FileStatusKind::Added | FileStatusKind::Modified | FileStatusKind::Untracked => {
                after.get(&change.path).map(|entity| ReconcileEvent {
                    project_id: project_id.to_string(),
                    entity_type: entity.entity_type,
                    entity_id: entity.id.clone(),
                    action: if before.values().any(|old| old.id == entity.id) {
                        ReconcileAction::Updated
                    } else {
                        ReconcileAction::Created
                    },
                    path: change.path.clone(),
                })
            }
        };
        if let Some(event) = event {
            emit(&event);
            events.push(event);
        }
    }
    Ok(ReconcileReport { events })
}

fn relevant_path(change: &ChangedFile, prefix: &str) -> bool {
    change.path.starts_with(prefix)
        || change
            .old_path
            .as_deref()
            .is_some_and(|path| path.starts_with(prefix))
}

fn is_reconciled_path(path: &str) -> bool {
    (path.starts_with("posts/") && path.ends_with(".md"))
        || (path.starts_with("scripts/") && path.ends_with(".lua"))
        || (path.starts_with("templates/") && path.ends_with(".liquid"))
}

fn fail_rebuild_errors(category: &str, errors: Vec<String>) -> EngineResult<()> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(EngineError::Parse(format!(
            "Git reconciliation failed for {category}: {}",
            errors.join("; ")
        )))
    }
}

fn entities_by_path(
    conn: &DbConnection,
    project_id: &str,
) -> EngineResult<HashMap<String, EntityAtPath>> {
    let mut entities = HashMap::new();
    for post in crate::db::queries::post::list_posts_by_project(conn, project_id)? {
        if !post.file_path.is_empty() {
            entities.insert(
                post.file_path.clone(),
                EntityAtPath {
                    entity_type: ReconcileEntityType::Post,
                    id: post.id.clone(),
                },
            );
        }
        for translation in
            crate::db::queries::post_translation::list_post_translations_by_post(conn, &post.id)?
        {
            if !translation.file_path.is_empty() {
                entities.insert(
                    translation.file_path,
                    EntityAtPath {
                        entity_type: ReconcileEntityType::PostTranslation,
                        id: translation.id,
                    },
                );
            }
        }
    }
    for script in crate::db::queries::script::list_scripts_by_project(conn, project_id)? {
        if !script.file_path.is_empty() {
            entities.insert(
                script.file_path,
                EntityAtPath {
                    entity_type: ReconcileEntityType::Script,
                    id: script.id,
                },
            );
        }
    }
    for template in crate::db::queries::template::list_templates_by_project(conn, project_id)? {
        if !template.file_path.is_empty() {
            entities.insert(
                template.file_path,
                EntityAtPath {
                    entity_type: ReconcileEntityType::Template,
                    id: template.id,
                },
            );
        }
    }
    Ok(entities)
}

fn delete_entity_at_path(conn: &DbConnection, entity: Option<&EntityAtPath>) -> EngineResult<()> {
    let Some(entity) = entity else {
        return Ok(());
    };
    match entity.entity_type {
        ReconcileEntityType::Post => crate::db::queries::post::delete_post(conn, &entity.id)?,
        ReconcileEntityType::PostTranslation => {
            crate::db::queries::post_translation::delete_post_translation(conn, &entity.id)?
        }
        ReconcileEntityType::Script => crate::db::queries::script::delete_script(conn, &entity.id)?,
        ReconcileEntityType::Template => {
            crate::db::queries::template::delete_template(conn, &entity.id)?
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn parses_nul_status_with_renames_and_spaces() {
        let files =
            parse_status(b"M  staged name.md\0 M work.md\0?? new file.md\0R  new.md\0old.md\0");
        assert_eq!(files.len(), 4);
        assert!(files[0].staged);
        assert!(files[1].unstaged);
        assert_eq!(files[2].kind, FileStatusKind::Untracked);
        assert_eq!(files[3].path, "new.md");
        assert_eq!(files[3].old_path.as_deref(), Some("old.md"));
    }

    #[test]
    fn parses_name_status_renames() {
        assert_eq!(
            parse_changed_files(b"A\0posts/new.md\0R100\0scripts/old.lua\0scripts/new.lua\0"),
            vec![
                ChangedFile {
                    path: "posts/new.md".into(),
                    old_path: None,
                    kind: FileStatusKind::Added,
                },
                ChangedFile {
                    path: "scripts/new.lua".into(),
                    old_path: Some("scripts/old.lua".into()),
                    kind: FileStatusKind::Renamed,
                },
            ]
        );
    }

    #[test]
    fn rejects_paths_outside_repository() {
        assert!(validate_relative_path("../secret").is_err());
        assert!(validate_relative_path("/secret").is_err());
        assert!(validate_relative_path("posts/ok.md").is_ok());
    }

    #[test]
    fn reconciliation_reuses_rebuild_paths_for_updates_deletes_and_renames() {
        use crate::db::Database;
        use crate::db::queries::project::{insert_project, make_test_project};
        use crate::model::{ScriptKind, TemplateKind};

        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        crate::db::fts::ensure_fts_tables(db.conn()).unwrap();
        insert_project(db.conn(), &make_test_project("p1", "blog")).unwrap();
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("posts")).unwrap();
        fs::create_dir_all(dir.path().join("scripts")).unwrap();
        fs::create_dir_all(dir.path().join("templates")).unwrap();

        let post = crate::engine::post::create_post(
            db.conn(),
            dir.path(),
            "p1",
            "Original title",
            Some("Body"),
            Vec::new(),
            Vec::new(),
            None,
            Some("en"),
            None,
        )
        .unwrap();
        let post = crate::engine::post::publish_post(db.conn(), dir.path(), &post.id).unwrap();
        let script = crate::engine::script::create_script(
            db.conn(),
            "p1",
            "Tool",
            ScriptKind::Utility,
            "function main() return true end",
            Some("main"),
        )
        .unwrap();
        let script =
            crate::engine::script::publish_script(db.conn(), dir.path(), &script.id).unwrap();
        let template = crate::engine::template::create_template(
            db.conn(),
            "p1",
            "Layout",
            TemplateKind::Post,
            "<article>{{ content }}</article>",
        )
        .unwrap();
        let template =
            crate::engine::template::publish_template(db.conn(), dir.path(), &template.id).unwrap();

        let post_path = dir.path().join(&post.file_path);
        let post_contents = fs::read_to_string(&post_path).unwrap();
        fs::write(
            &post_path,
            post_contents.replace("title: Original title", "title: Pulled title"),
        )
        .unwrap();
        fs::remove_file(dir.path().join(&script.file_path)).unwrap();
        let old_template_path = template.file_path.clone();
        let new_template_path = "templates/renamed-layout.liquid".to_string();
        fs::rename(
            dir.path().join(&old_template_path),
            dir.path().join(&new_template_path),
        )
        .unwrap();

        let changes = vec![
            ChangedFile {
                path: post.file_path.clone(),
                old_path: None,
                kind: FileStatusKind::Modified,
            },
            ChangedFile {
                path: script.file_path.clone(),
                old_path: None,
                kind: FileStatusKind::Deleted,
            },
            ChangedFile {
                path: new_template_path.clone(),
                old_path: Some(old_template_path),
                kind: FileStatusKind::Renamed,
            },
        ];
        let mut emitted = Vec::new();
        let report = reconcile_changed_files(db.conn(), dir.path(), "p1", &changes, |event| {
            emitted.push(event.clone());
        })
        .unwrap();

        assert_eq!(report.events, emitted);
        assert_eq!(
            crate::db::queries::post::get_post_by_id(db.conn(), &post.id)
                .unwrap()
                .title,
            "Pulled title"
        );
        assert!(crate::db::queries::script::get_script_by_id(db.conn(), &script.id).is_err());
        assert_eq!(
            crate::db::queries::template::get_template_by_id(db.conn(), &template.id)
                .unwrap()
                .file_path,
            new_template_path
        );
        assert!(report.events.iter().any(|event| {
            event.entity_id == script.id && event.action == ReconcileAction::Deleted
        }));
        assert!(report.events.iter().any(|event| {
            event.entity_id == template.id && event.action == ReconcileAction::Renamed
        }));
        let diff = crate::engine::metadata_diff::compute_metadata_diff(db.conn(), dir.path(), "p1")
            .unwrap();
        assert!(
            diff.diffs.is_empty(),
            "metadata differences: {:?}",
            diff.diffs
        );
    }

    #[cfg(unix)]
    #[test]
    fn initialization_preserves_ignore_entries_and_configures_every_lfs_pattern() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "/custom/\n").unwrap();
        let executable = dir.path().join("fake-git");
        fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = init ]; then mkdir -p .git; exit 0; fi\nif [ \"$1\" = symbolic-ref ]; then echo master; exit 0; fi\nif [ \"$1\" = remote ]; then exit 2; fi\nexit 0\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        let engine = GitEngine::with_executable_and_timeouts(
            dir.path(),
            executable,
            Duration::from_secs(1),
            Duration::from_secs(1),
        );

        let repository = engine.initialize().unwrap();

        assert!(repository.is_initialized);
        assert_eq!(repository.current_branch.as_deref(), Some("master"));
        let gitignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(gitignore.contains("/custom/"));
        assert!(
            GITIGNORE_LINES
                .iter()
                .all(|line| gitignore.lines().any(|found| found == *line))
        );
        let attributes = fs::read_to_string(dir.path().join(".gitattributes")).unwrap();
        assert!(LFS_PATTERNS.iter().all(|pattern| {
            attributes
                .lines()
                .any(|line| line.starts_with(pattern) && line.contains("filter=lfs"))
        }));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_and_cancellation_kill_the_process_and_preserve_output() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("fake-git");
        fs::write(&executable, "#!/bin/sh\nprintf 'started\\n'\nsleep 5\n").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        let engine = GitEngine::with_executable_and_timeouts(
            dir.path(),
            &executable,
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        let error = engine.fetch(|| false, |_| {}).unwrap_err();
        assert!(
            matches!(
                error,
                GitError::TimedOut {
                    operation: GitOperation::Fetch,
                    ref output,
                    ..
                } if output.contains("started")
            ),
            "{error:?}"
        );

        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancelled);
        let output = Arc::new(Mutex::new(String::new()));
        let streamed = Arc::clone(&output);
        let engine = GitEngine::with_executable_and_timeouts(
            dir.path(),
            executable,
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        let error = engine
            .push(
                move || {
                    let seen = !streamed.lock().unwrap().is_empty();
                    if seen {
                        flag.store(true, Ordering::Release);
                    }
                    flag.load(Ordering::Acquire)
                },
                |chunk| output.lock().unwrap().push_str(&chunk.text),
            )
            .unwrap_err();
        assert!(matches!(error, GitError::Cancelled { .. }));
        assert!(output.lock().unwrap().contains("started"));
    }

    #[cfg(unix)]
    #[test]
    fn authentication_errors_are_structured_by_provider_and_platform() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("fake-git");
        fs::write(
            &executable,
            "#!/bin/sh\nif [ \"$1\" = remote ]; then echo git@gitlab.com:owner/repo.git; exit 0; fi\necho 'fatal: Authentication failed' >&2\nexit 128\n",
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        let engine = GitEngine::with_executable_and_timeouts(
            dir.path(),
            executable,
            Duration::from_secs(1),
            Duration::from_secs(1),
        );
        let error = engine.fetch(|| false, |_| {}).unwrap_err();
        assert!(matches!(
            error,
            GitError::Authentication {
                provider: Some(GitProvider::GitLab),
                ..
            }
        ));
    }
}
