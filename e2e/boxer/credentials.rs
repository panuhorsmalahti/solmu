use super::*;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(binary("boxer")).args(args).output().unwrap()
}

#[test]
fn credential_commands_validate_names_without_exposing_values() {
    let invalid = run(&["credential", "status", "PATH"]);
    assert_eq!(invalid.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("environment variable name"));
    assert!(!String::from_utf8_lossy(&invalid.stdout).contains("test-secret"));

    let usage = run(&["credential"]);
    assert_eq!(usage.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&usage.stderr).contains("credential set|status|delete"));
}

#[test]
fn environment_credentials_reject_reserved_names_before_store_access() {
    let output = run(&[
        "--env-credential",
        "SOLMU_WORKSPACE",
        "--",
        "program-that-must-not-run",
    ]);
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Invalid --env-credential"));
}

#[test]
fn environment_credentials_can_be_declared_in_a_policy_file() {
    let temp = tempfile::tempdir().unwrap();
    let policy = temp.path().join("policy.json");
    std::fs::write(
        &policy,
        r#"{"version":1,"mode":"workspace","network":"allow","env_credentials":["OPENAI_API_KEY"]}"#,
    )
    .unwrap();
    let process = Command::new(binary("boxer"))
        .args(["--policy"])
        .arg(&policy)
        .args(["--cwd"])
        .arg(temp.path())
        .args(["--print-policy", "--", "unused-program"])
        .output()
        .unwrap();
    assert!(
        process.status.success(),
        "{}",
        String::from_utf8_lossy(&process.stderr)
    );
    let output: serde_json::Value = serde_json::from_slice(&process.stdout).unwrap();
    assert_eq!(output["policy"]["env_credentials"][0], "OPENAI_API_KEY");
    assert!(!String::from_utf8_lossy(&process.stdout).contains("test-secret"));
}

#[test]
fn credential_proxy_requires_isolated_routed_networking() {
    let output = run(&["--credential", "openai", "--", "unused-program"]);
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Credential proxying"));
}

#[cfg(target_os = "linux")]
#[test]
fn credential_proxy_policy_adds_provider_route_without_forwarding_real_key() {
    let workspace = tempfile::tempdir().unwrap();
    let output = Command::new(binary("boxer"))
        .args([
            "--isolated",
            "--network",
            "proxy",
            "--credential",
            "openai",
            "--cwd",
        ])
        .arg(workspace.path())
        .args(["--print-policy", "--", "unused-program"])
        .env("OPENAI_API_KEY", "real-secret-fixture")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["policy"]["credentials"][0], "openai");
    assert!(
        result["policy"]["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|host| { host == "api.openai.com:443" })
    );
    assert!(
        !result["environment"]["forwarded_names"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "OPENAI_API_KEY")
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("real-secret-fixture"));
}
