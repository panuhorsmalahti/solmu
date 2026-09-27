use super::*;
use serde_json::{Value, json};

#[test]
fn readiness_applies_the_policy_without_launching_the_requested_program() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("must-not-run");
    let output = Command::new(binary("boxer"))
        .args(["--check", "--cwd"])
        .arg(directory.path())
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg(&marker)
        .env("OPENAI_API_KEY", "private-readiness-key")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!marker.exists());
    let source = String::from_utf8(output.stdout).unwrap();
    assert!(!source.contains("private-readiness-key"));
    let result: Value = serde_json::from_str(&source).unwrap();
    assert_eq!(result["enforcement"], "checked");
    assert_eq!(result["mode"], "unrestricted");
    assert_eq!(result["network"], "allow");
    assert_eq!(result["program_started"], false);
    // Program availability is deliberately separate from kernel readiness.
    assert!(
        Command::new(binary("boxer"))
            .args(["--check", "--", "missing-program"])
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn readiness_checks_saved_policies_and_rejects_ambiguous_preview_options() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("policy.json");
    std::fs::write(
        &file,
        json!({"version":1,"mode":"workspace","network":"deny","read_only":true}).to_string(),
    )
    .unwrap();
    let output = Command::new(binary("boxer"))
        .arg("--policy")
        .arg(&file)
        .args(["--check", "--cwd"])
        .arg(directory.path())
        .output()
        .unwrap();
    if cfg!(windows) {
        assert_eq!(output.status.code(), Some(125));
        assert!(String::from_utf8_lossy(&output.stderr).contains("Policy readiness check failed"));
    } else {
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["mode"], "workspace");
        assert_eq!(result["network"], "deny");
    }
    let output = Command::new(binary("boxer"))
        .args(["--check", "--print-policy"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Choose either"));
}

#[cfg(target_os = "linux")]
#[test]
fn readiness_verifies_namespaces_and_cgroups_and_reports_missing_delegation() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--check", "--isolated", "--network", "deny", "--cwd"])
        .arg(directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["mode"], "isolated");
    assert_eq!(result["enforcement"], "checked");
    let output = Command::new(binary("boxer"))
        .args(["--check", "--isolated", "--cwd"])
        .arg(directory.path())
        .arg("--cgroup-root")
        .arg(directory.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Policy readiness check failed"));
}
