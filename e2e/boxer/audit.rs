use super::*;
use serde_json::Value;

fn invoke(root: &std::path::Path, args: &[&str]) -> std::process::Output {
    Command::new(binary("boxer"))
        .arg("rollback")
        .args(args)
        .env("HOME", root)
        .env("USERPROFILE", root)
        .output()
        .unwrap()
}

#[test]
fn audit_records_lifecycle_and_detects_session_tampering() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--rollback", "--cwd"])
        .arg(&workspace)
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg("audit-created.txt")
        .env("HOME", root.path())
        .env("USERPROFILE", root.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(7));
    let session = String::from_utf8_lossy(&output.stderr)
        .lines()
        .find_map(|line| line.strip_prefix("Rollback session: "))
        .unwrap()
        .to_owned();

    let verified = invoke(root.path(), &["audit", "verify", &session]);
    assert!(
        verified.status.success(),
        "{}",
        String::from_utf8_lossy(&verified.stderr)
    );
    let shown = invoke(root.path(), &["audit", "show", &session]);
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(shown.status.success());
    assert!(text.contains("session_started"));
    assert!(text.contains("session_completed"));
    let listed = invoke(root.path(), &["audit", "list"]);
    assert!(String::from_utf8_lossy(&listed.stdout).contains("verified"));

    let session_file = root
        .path()
        .join(".boxer/rollback/sessions")
        .join(format!("{session}.json"));
    let mut record: Value = serde_json::from_slice(&std::fs::read(&session_file).unwrap()).unwrap();
    record["audit"][0]["payload"]["event"] = Value::String("forged".into());
    std::fs::write(&session_file, serde_json::to_vec(&record).unwrap()).unwrap();
    let rejected = invoke(root.path(), &["audit", "verify", &session]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("authentication failed"));
}
