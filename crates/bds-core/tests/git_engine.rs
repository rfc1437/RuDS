use std::fs;

use bds_core::engine::git::{FileStatusKind, GitEngine};

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git_text(dir: &std::path::Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

#[test]
fn status_diff_history_and_missing_diff_sides_follow_the_spec() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "master"]);
    git(dir.path(), &["config", "user.name", "RuDS Test"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    fs::write(dir.path().join("kept.txt"), "before\n").unwrap();
    fs::write(dir.path().join("deleted.txt"), "gone\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-m", "initial"]);

    fs::write(dir.path().join("kept.txt"), "after\n").unwrap();
    fs::remove_file(dir.path().join("deleted.txt")).unwrap();
    fs::write(dir.path().join("added.txt"), "new\n").unwrap();

    let engine = GitEngine::new(dir.path());
    let status = engine.status().unwrap();
    assert!(
        status
            .iter()
            .any(|file| { file.path == "kept.txt" && file.kind == FileStatusKind::Modified })
    );
    assert!(
        status
            .iter()
            .any(|file| { file.path == "deleted.txt" && file.kind == FileStatusKind::Deleted })
    );
    assert!(
        status
            .iter()
            .any(|file| { file.path == "added.txt" && file.kind == FileStatusKind::Untracked })
    );

    let diff = engine.diff().unwrap();
    assert!(diff.unstaged.contains("-before"));
    assert!(diff.unstaged.contains("+after"));

    let added = engine.file_diff("added.txt").unwrap();
    assert_eq!(added.original, "");
    assert_eq!(added.modified, "new\n");
    let deleted = engine.file_diff("deleted.txt").unwrap();
    assert_eq!(deleted.original, "gone\n");
    assert_eq!(deleted.modified, "");

    assert_eq!(engine.file_history("kept.txt").unwrap().len(), 1);

    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-m", "working tree changes"]);
    let hash = git_text(dir.path(), &["rev-parse", "HEAD"]);
    let changes = engine.commit_files(&hash).unwrap();
    let added_change = changes
        .iter()
        .find(|change| change.path == "added.txt")
        .unwrap();
    let deleted_change = changes
        .iter()
        .find(|change| change.path == "deleted.txt")
        .unwrap();
    let added = engine.commit_file_diff(&hash, added_change).unwrap();
    let deleted = engine.commit_file_diff(&hash, deleted_change).unwrap();
    assert_eq!(
        (added.original.as_str(), added.modified.as_str()),
        ("", "new\n")
    );
    assert_eq!(
        (deleted.original.as_str(), deleted.modified.as_str()),
        ("gone\n", "")
    );
}

#[test]
fn commit_rejects_blank_messages_before_staging() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "master"]);
    fs::write(dir.path().join("untracked.txt"), "content\n").unwrap();

    let error = GitEngine::new(dir.path()).commit_all(" \n\t ").unwrap_err();
    assert!(error.to_string().contains("commit message"));

    let output = std::process::Command::new("git")
        .args(["diff", "--cached", "--name-only"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(output.stdout.is_empty());
}

#[test]
fn remote_state_history_fetch_and_fast_forward_pull_use_real_refs() {
    let root = tempfile::tempdir().unwrap();
    let remote = root.path().join("remote.git");
    let local = root.path().join("local");
    let peer = root.path().join("peer");
    fs::create_dir(&local).unwrap();
    git(
        root.path(),
        &["init", "--bare", "-b", "master", remote.to_str().unwrap()],
    );
    git(&local, &["init", "-b", "master"]);
    git(&local, &["config", "user.name", "Local"]);
    git(&local, &["config", "user.email", "local@example.com"]);
    fs::write(local.join("post.md"), "one\n").unwrap();
    git(&local, &["add", "-A"]);
    git(&local, &["commit", "-m", "local one"]);
    let engine = GitEngine::new(&local);
    engine.set_remote(remote.to_str().unwrap()).unwrap();
    git(&local, &["push", "-u", "origin", "master"]);

    git(
        root.path(),
        &["clone", remote.to_str().unwrap(), peer.to_str().unwrap()],
    );
    git(&peer, &["config", "user.name", "Peer"]);
    git(&peer, &["config", "user.email", "peer@example.com"]);
    fs::write(peer.join("post.md"), "two\n").unwrap();
    git(&peer, &["add", "-A"]);
    git(&peer, &["commit", "-m", "remote two"]);
    git(&peer, &["push"]);

    let mut streamed = String::new();
    engine
        .fetch(|| false, |chunk| streamed.push_str(&chunk.text))
        .unwrap();
    let state = engine.remote_state().unwrap();
    assert_eq!(state.behind, 1);
    assert_eq!(state.ahead, 0);
    assert!(
        engine
            .history("master")
            .unwrap()
            .iter()
            .any(|commit| commit.subject.as_deref() == Some("remote two")
                && commit.sync_status == bds_core::engine::git::SyncStatus::RemoteOnly)
    );

    engine.pull(|| false, |_| {}).unwrap();
    let state = engine.remote_state().unwrap();
    assert_eq!((state.ahead, state.behind), (0, 0));
    assert_eq!(fs::read_to_string(local.join("post.md")).unwrap(), "two\n");
}

#[test]
fn file_history_follows_renames_and_is_limited_to_fifty_commits() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "master"]);
    git(dir.path(), &["config", "user.name", "RuDS Test"]);
    git(dir.path(), &["config", "user.email", "test@example.com"]);
    fs::write(dir.path().join("old.txt"), "0\n").unwrap();
    git(dir.path(), &["add", "-A"]);
    git(dir.path(), &["commit", "-m", "old name"]);
    git(dir.path(), &["mv", "old.txt", "new.txt"]);
    git(dir.path(), &["commit", "-m", "renamed"]);
    for index in 1..=49 {
        fs::write(dir.path().join("new.txt"), format!("{index}\n")).unwrap();
        git(dir.path(), &["add", "new.txt"]);
        git(dir.path(), &["commit", "-m", &format!("change {index}")]);
    }

    let history = GitEngine::new(dir.path()).file_history("new.txt").unwrap();

    assert_eq!(history.len(), 50);
    assert_eq!(history[0].subject.as_deref(), Some("change 49"));
    assert!(
        history
            .iter()
            .any(|commit| commit.subject.as_deref() == Some("renamed"))
    );
}
