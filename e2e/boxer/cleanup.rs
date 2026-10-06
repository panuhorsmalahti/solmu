use super::*;

fn run(root: &std::path::Path, workspace: &std::path::Path, file: &str) -> std::process::Output {
    Command::new(binary("boxer"))
        .args(["--rollback", "--cwd"])
        .arg(workspace)
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg(file)
        .env("HOME", root)
        .env("USERPROFILE", root)
        .output()
        .unwrap()
}

fn session_id(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .find_map(|line| line.strip_prefix("Rollback session: "))
        .unwrap()
        .to_owned()
}

fn command(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(binary("boxer"))
        .arg("rollback")
        .args(args)
        .env("HOME", root)
        .env("USERPROFILE", root)
        .output()
        .unwrap()
}

fn stored_blob_with(root: &std::path::Path, value: &[u8]) -> bool {
    let directory = root.join(".boxer/rollback/blobs");
    std::fs::read_dir(directory)
        .unwrap()
        .flatten()
        .any(|entry| std::fs::read(entry.path()).is_ok_and(|contents| contents == value))
}

#[test]
fn rollback_cleanup_previews_prunes_and_preserves_retained_snapshot_objects() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("shared.txt"), "shared retained content").unwrap();

    let first = run(root.path(), &workspace, "first.txt");
    assert_eq!(first.status.code(), Some(7));
    let first_id = session_id(&first);
    std::fs::write(
        workspace.join("old-only.txt"),
        "unique old snapshot content",
    )
    .unwrap();
    let second = run(root.path(), &workspace, "second.txt");
    assert_eq!(second.status.code(), Some(7));
    let second_id = session_id(&second);
    std::fs::remove_file(workspace.join("old-only.txt")).unwrap();
    let third = run(root.path(), &workspace, "third.txt");
    assert_eq!(third.status.code(), Some(7));
    let third_id = session_id(&third);
    assert!(stored_blob_with(
        root.path(),
        b"unique old snapshot content"
    ));
    assert!(stored_blob_with(root.path(), b"shared retained content"));

    let session_path = root
        .path()
        .join(".boxer/rollback/sessions")
        .join(format!("{first_id}.json"));
    let mut session: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&session_path).unwrap()).unwrap();
    session["created_unix_ms"] = serde_json::json!(1);
    std::fs::write(&session_path, serde_json::to_vec(&session).unwrap()).unwrap();

    let preview = command(root.path(), &["cleanup", "--older-than", "1", "--dry-run"]);
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview_text = String::from_utf8_lossy(&preview.stdout);
    assert!(preview_text.contains(&first_id));
    assert!(!preview_text.contains(&second_id));
    assert!(stored_blob_with(
        root.path(),
        b"unique old snapshot content"
    ));
    let before = command(root.path(), &["list"]);
    assert_eq!(String::from_utf8_lossy(&before.stdout).lines().count(), 4);

    let pruned = command(root.path(), &["cleanup", "--keep", "1"]);
    assert!(
        pruned.status.success(),
        "{}",
        String::from_utf8_lossy(&pruned.stderr)
    );
    assert!(String::from_utf8_lossy(&pruned.stdout).contains("Cleanup complete"));
    let after = command(root.path(), &["list"]);
    let listing = String::from_utf8_lossy(&after.stdout);
    assert!(listing.contains(&third_id));
    assert!(!listing.contains(&first_id));
    assert!(!listing.contains(&second_id));
    assert_eq!(listing.lines().count(), 2);
    assert!(!stored_blob_with(
        root.path(),
        b"unique old snapshot content"
    ));
    assert!(stored_blob_with(root.path(), b"shared retained content"));
    let audit = command(root.path(), &["audit", "verify", &third_id]);
    assert!(
        audit.status.success(),
        "{}",
        String::from_utf8_lossy(&audit.stderr)
    );
    let restore = command(root.path(), &["restore", &third_id]);
    assert!(
        restore.status.success(),
        "{}",
        String::from_utf8_lossy(&restore.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("shared.txt")).unwrap(),
        "shared retained content"
    );
    assert!(!workspace.join("third.txt").exists());
}

#[test]
fn rollback_cleanup_requires_a_retention_rule_and_validates_it() {
    let root = tempfile::tempdir().unwrap();
    for args in [
        vec!["cleanup"],
        vec!["cleanup", "--older-than", "0"],
        vec!["cleanup", "--keep", "all"],
    ] {
        let output = command(root.path(), &args);
        assert_eq!(output.status.code(), Some(125));
    }
}
