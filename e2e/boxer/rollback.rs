use super::*;
use serde_json::Value;

fn run_probe(
    root: &std::path::Path,
    workspace: &std::path::Path,
    path: &str,
) -> std::process::Output {
    Command::new(binary("boxer"))
        .args(["--rollback", "--cwd"])
        .arg(workspace)
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg(path)
        .env("HOME", root)
        .env("USERPROFILE", root)
        .output()
        .unwrap()
}

fn session_id(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    stderr
        .lines()
        .find_map(|line| line.strip_prefix("Rollback session: "))
        .unwrap_or_else(|| panic!("missing session id in {stderr}"))
        .to_owned()
}

fn rollback_command(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(binary("boxer"))
        .arg("rollback")
        .args(args)
        .env("HOME", root)
        .env("USERPROFILE", root)
        .output()
        .unwrap()
}

#[test]
fn rollback_snapshots_sessions_lists_diffs_and_restores_workspace() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("modified.txt"), "before session").unwrap();
    std::fs::write(
        workspace.join("newline.txt"),
        "Solmu sandbox write allowed\n",
    )
    .unwrap();
    std::fs::write(workspace.join("deleted.txt"), "keep until rollback").unwrap();

    let output = Command::new(binary("boxer"))
        .args(["--rollback", "--cwd"])
        .arg(&workspace)
        .arg("--")
        .arg(binary("sandbox-probe"))
        .args([
            "--rollback-set",
            "modified.txt",
            "newline.txt",
            "created.txt",
            "deleted.txt",
        ])
        .env("HOME", root.path())
        .env("USERPROFILE", root.path())
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("modified.txt")).unwrap(),
        "Solmu sandbox write allowed"
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("created.txt")).unwrap(),
        "created during session"
    );
    assert!(!workspace.join("deleted.txt").exists());
    let id = session_id(&output);

    let listed = rollback_command(root.path(), &["list"]);
    assert!(listed.status.success());
    assert!(String::from_utf8_lossy(&listed.stdout).contains(&id));
    let shown = rollback_command(root.path(), &["show", &id, "--diff"]);
    assert!(shown.status.success());
    let diff = String::from_utf8_lossy(&shown.stdout);
    assert!(diff.contains("modified\tmodified.txt"));
    assert!(diff.contains("added\tcreated.txt"));
    assert!(diff.contains("deleted\tdeleted.txt"));
    assert!(diff.contains("-before session"));
    assert!(diff.contains("+Solmu sandbox write allowed"));
    assert!(diff.contains("-Solmu sandbox write allowed"));
    assert!(diff.contains("\\ No newline at end of file"));

    let preview = rollback_command(root.path(), &["restore", &id, "--dry-run"]);
    assert!(preview.status.success());
    assert!(String::from_utf8_lossy(&preview.stdout).contains("modified.txt"));
    assert_eq!(
        std::fs::read_to_string(workspace.join("modified.txt")).unwrap(),
        "Solmu sandbox write allowed",
        "preview must not modify the workspace"
    );

    let restored = rollback_command(root.path(), &["restore", &id]);
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("modified.txt")).unwrap(),
        "before session"
    );
    assert!(!workspace.join("created.txt").exists());
    assert_eq!(
        std::fs::read_to_string(workspace.join("deleted.txt")).unwrap(),
        "keep until rollback"
    );
}

#[test]
fn rollback_restores_deleted_workspaces_without_following_tampered_paths() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("file.txt"), "before").unwrap();
    let output = run_probe(root.path(), &workspace, "file.txt");
    let id = session_id(&output);
    std::fs::remove_dir_all(&workspace).unwrap();
    let restored = rollback_command(root.path(), &["restore", &id]);
    assert!(
        restored.status.success(),
        "{}",
        String::from_utf8_lossy(&restored.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("file.txt")).unwrap(),
        "before"
    );

    let manifest_path = root
        .path()
        .join(".boxer/rollback/sessions")
        .join(format!("{id}.json"));
    std::fs::write(workspace.join("file.txt"), "after interrupted session").unwrap();
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["after"] = Value::Null;
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let interrupted_restore = rollback_command(root.path(), &["restore", &id]);
    assert!(
        interrupted_restore.status.success(),
        "{}",
        String::from_utf8_lossy(&interrupted_restore.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(workspace.join("file.txt")).unwrap(),
        "before"
    );

    let outside = root.path().join("outside.txt");
    std::fs::write(&outside, "keep safe").unwrap();
    let mut manifest: Value =
        serde_json::from_slice(&std::fs::read(&manifest_path).unwrap()).unwrap();
    manifest["before"]["entries"] = serde_json::json!({
        "2e2e2f6f7574736964652e747874": {
            "kind": "file",
            "hash": "0000000000000000000000000000000000000000000000000000000000000000",
            "mode": 420
        }
    });
    std::fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    let rejected = rollback_command(root.path(), &["restore", &id]);
    assert!(!rejected.status.success());
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "keep safe");
}

#[test]
fn rollback_rejects_storage_inside_workspace_before_launching_or_creating_it() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--rollback", "--cwd"])
        .arg(&workspace)
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg("must-not-exist.txt")
        .env("HOME", root.path())
        .env("USERPROFILE", root.path())
        .env("BOXER_ROLLBACK_DIR", workspace.join("rollback-store"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(!workspace.join("must-not-exist.txt").exists());
    assert!(!workspace.join("rollback-store").exists());
}
