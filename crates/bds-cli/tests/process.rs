use std::process::Command;

fn cli(home: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_bds-cli"))
        .env("HOME", home)
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn process_exit_codes_help_and_shared_state_roundtrip() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let project = root.path().join("project");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&project).unwrap();

    let help = cli(&home, &["--help"]);
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("rebuild"));

    let invalid = cli(&home, &["not-a-command"]);
    assert_eq!(invalid.status.code(), Some(1));
    assert!(!invalid.stderr.is_empty());

    let added = cli(
        &home,
        &[
            "project",
            "add",
            project.to_str().unwrap(),
            "--name",
            "Process Blog",
        ],
    );
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );
    assert!(
        cli(&home, &["project", "switch", "process-blog"])
            .status
            .success()
    );

    let created = Command::new(env!("CARGO_BIN_EXE_bds-cli"))
        .env("HOME", &home)
        .args(["--json", "post", "--stdin", "--no-translate"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write as _;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(br#"{"title":"Process post","content":"Body","language":"en"}"#)?;
            child.wait_with_output()
        })
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&created.stdout).unwrap();
    assert_eq!(value["ok"], true);
    assert_eq!(value["data"]["slug"], "process-post");

    let get_missing = cli(&home, &["config", "get", "missing"]);
    assert_eq!(get_missing.status.code(), Some(1));
}

#[cfg(unix)]
#[test]
fn process_upload_push_and_pull_dispatch_successfully() {
    use std::os::unix::fs::PermissionsExt as _;

    let root = tempfile::tempdir().unwrap();
    let home = root.path().join("home");
    let project = root.path().join("project");
    let remote = root.path().join("remote.git");
    let peer = root.path().join("peer");
    let fake_bin = root.path().join("bin");
    for directory in [&home, &project, &fake_bin] {
        std::fs::create_dir_all(directory).unwrap();
    }
    assert!(
        cli(
            &home,
            &[
                "project",
                "add",
                project.to_str().unwrap(),
                "--name",
                "External Blog",
            ],
        )
        .status
        .success()
    );
    assert!(
        cli(&home, &["project", "switch", "external-blog"])
            .status
            .success()
    );

    std::fs::write(
        project.join("meta/publishing.json"),
        r#"{"sshHost":"example.test","sshUser":"deploy","sshRemotePath":"/srv/blog","sshMode":"rsync"}"#,
    )
    .unwrap();
    let rsync = fake_bin.join("rsync");
    std::fs::write(&rsync, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = std::fs::metadata(&rsync).unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&rsync, permissions).unwrap();
    let upload = Command::new(env!("CARGO_BIN_EXE_bds-cli"))
        .env("HOME", &home)
        .env("PATH", &fake_bin)
        .env("SSH_AUTH_SOCK", root.path().join("agent.sock"))
        .arg("upload")
        .output()
        .unwrap();
    assert!(
        upload.status.success(),
        "{}",
        String::from_utf8_lossy(&upload.stderr)
    );

    git(&project, &["init", "-b", "main"]);
    git(&project, &["config", "user.email", "cli@example.test"]);
    git(&project, &["config", "user.name", "CLI Test"]);
    git(&project, &["add", "."]);
    git(&project, &["commit", "-m", "Initial"]);
    git(root.path(), &["init", "--bare", remote.to_str().unwrap()]);
    git(
        &project,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    git(&project, &["push", "-u", "origin", "main"]);
    git(&remote, &["symbolic-ref", "HEAD", "refs/heads/main"]);

    let pushed = cli(&home, &["push"]);
    assert!(
        pushed.status.success(),
        "{}",
        String::from_utf8_lossy(&pushed.stderr)
    );

    git(
        root.path(),
        &["clone", remote.to_str().unwrap(), peer.to_str().unwrap()],
    );
    git(&peer, &["config", "user.email", "peer@example.test"]);
    git(&peer, &["config", "user.name", "Peer Test"]);
    std::fs::write(peer.join("remote-change.txt"), "change").unwrap();
    git(&peer, &["add", "remote-change.txt"]);
    git(&peer, &["commit", "-m", "Remote change"]);
    git(&peer, &["push"]);

    let pulled = cli(&home, &["pull"]);
    assert!(
        pulled.status.success(),
        "{}",
        String::from_utf8_lossy(&pulled.stderr)
    );
    assert!(project.join("remote-change.txt").is_file());
}

#[cfg(unix)]
fn git(cwd: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}
