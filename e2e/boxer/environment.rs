use super::*;
use serde_json::json;

#[test]
fn policy_environment_patterns_allow_deny_and_match_full_names() {
    let directory = tempfile::tempdir().unwrap();
    let policy = directory.path().join("policy.json");
    std::fs::write(
        &policy,
        json!({
            "version": 1,
            "mode": "unrestricted",
            "environment": {
                "allow_vars": ["BOXER_E2E_K*", "BOXER_EXACT", "boxer_e2e_case"],
                "deny_vars": ["BOXER_E2E_SECRET"],
                "case_insensitive_vars": true
            }
        })
        .to_string(),
    )
    .unwrap();
    let output = Command::new(binary("boxer"))
        .arg("--policy")
        .arg(&policy)
        .arg("--")
        .arg(binary("sandbox-probe"))
        .args([
            "--env-check",
            "BOXER_E2E_KEEP=present",
            "BOXER_E2E_SECRET=absent",
            "BOXER_E2E_CASE=present",
            "BOXER_EXACT=present",
            "BOXER_EXACT_SUFFIX=absent",
            "BOXER_E2E_UNRELATED=absent",
        ])
        .env("BOXER_E2E_KEEP", "kept")
        .env("BOXER_E2E_SECRET", "denied")
        .env("BOXER_E2E_CASE", "case-insensitive")
        .env("BOXER_EXACT", "exact")
        .env("BOXER_EXACT_SUFFIX", "not-exact")
        .env("BOXER_E2E_UNRELATED", "not-allowed")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn empty_allow_list_passes_only_explicit_cli_variables() {
    let directory = tempfile::tempdir().unwrap();
    let policy = directory.path().join("policy.json");
    std::fs::write(
        &policy,
        r#"{"version":1,"mode":"unrestricted","environment":{"allow_vars":[]}}"#,
    )
    .unwrap();
    let output = Command::new(binary("boxer"))
        .arg("--policy")
        .arg(&policy)
        .args(["--pass-env", "BOXER_EXPLICIT"])
        .arg("--")
        .arg(binary("sandbox-probe"))
        .args([
            "--env-check",
            "BOXER_EXPLICIT=present",
            "BOXER_OTHER=absent",
        ])
        .env("BOXER_EXPLICIT", "explicit")
        .env("BOXER_OTHER", "filtered")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn policy_rejects_empty_environment_patterns_before_launch() {
    let directory = tempfile::tempdir().unwrap();
    let policy = directory.path().join("policy.json");
    std::fs::write(
        &policy,
        r#"{"version":1,"mode":"unrestricted","environment":{"allow_vars":[""]}}"#,
    )
    .unwrap();
    let output = Command::new(binary("boxer"))
        .arg("--policy")
        .arg(policy)
        .arg("--")
        .arg(binary("program-that-must-not-start"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be empty"));
}

#[test]
fn static_environment_values_expand_paths_override_inherited_values_and_are_redacted() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("project");
    std::fs::create_dir(&workspace).unwrap();
    let policy = root.path().join("policy.json");
    std::fs::write(
        &policy,
        json!({
            "version": 1,
            "mode": "unrestricted",
            "environment": {
                "allow_vars": [],
                "set_vars": {"RUST_LOG": "$HOME|$WORKDIR"}
            }
        })
        .to_string(),
    )
    .unwrap();
    let home = root.path().canonicalize().unwrap();
    let workspace = workspace.canonicalize().unwrap();
    let expected = format!("{}|{}", home.display(), workspace.display());

    let preview = Command::new(binary("boxer"))
        .arg("--policy")
        .arg(&policy)
        .arg("--cwd")
        .arg(&workspace)
        .arg("--print-policy")
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "inherited-secret-value")
        .output()
        .unwrap();
    assert!(preview.status.success());
    let preview = String::from_utf8_lossy(&preview.stdout);
    assert!(preview.contains(r#""RUST_LOG": "[redacted]""#));
    assert!(!preview.contains(&expected));
    assert!(!preview.contains("inherited-secret-value"));

    let launched = Command::new(binary("boxer"))
        .arg("--policy")
        .arg(&policy)
        .arg("--cwd")
        .arg(&workspace)
        .arg("--")
        .arg(binary("sandbox-probe"))
        .args(["--env-check-value", &format!("RUST_LOG={expected}")])
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("RUST_LOG", "inherited-secret-value")
        .output()
        .unwrap();
    assert!(
        launched.status.success(),
        "{}",
        String::from_utf8_lossy(&launched.stderr)
    );
}
