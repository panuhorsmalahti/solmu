use super::*;
use serde_json::Value;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(binary("boxer")).args(args).output().unwrap()
}

#[test]
fn policy_init_creates_a_valid_scaffold_without_overwriting_existing_files() {
    let temp = tempfile::tempdir().unwrap();
    let output = Command::new(binary("boxer"))
        .args(["policy", "init"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let policy_path = temp.path().join("boxer-policy.json");
    let policy: Value = serde_json::from_slice(&std::fs::read(&policy_path).unwrap()).unwrap();
    assert_eq!(policy["version"], 1);
    assert_eq!(policy["mode"], "workspace");
    assert_eq!(policy["network"], "allow");

    let validated = Command::new(binary("boxer"))
        .args(["policy", "validate", "boxer-policy.json", "--cwd"])
        .arg(temp.path())
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );

    let original = std::fs::read(&policy_path).unwrap();
    let repeated = Command::new(binary("boxer"))
        .args(["policy", "init"])
        .current_dir(temp.path())
        .output()
        .unwrap();
    assert_eq!(repeated.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&repeated.stderr).contains("Refusing to overwrite"));
    assert_eq!(std::fs::read(policy_path).unwrap(), original);
}

#[test]
fn policy_commands_validate_resolve_diff_and_list_builtin_profiles() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(workspace.join("data")).unwrap();
    let before = workspace.join("before.json");
    let after = workspace.join("after.json");
    std::fs::write(workspace.join("data/readme.txt"), "content").unwrap();
    std::fs::write(
        &before,
        r#"{"version":1,"mode":"workspace","network":"allow","read":["data"]}"#,
    )
    .unwrap();
    std::fs::write(
        &after,
        r#"{"version":1,"mode":"workspace","network":"deny","read_only":true,"read":["data"]}"#,
    )
    .unwrap();

    let validated = run(&[
        "policy",
        "validate",
        before.to_str().unwrap(),
        "--cwd",
        workspace.to_str().unwrap(),
    ]);
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );
    let result: Value = serde_json::from_slice(&validated.stdout).unwrap();
    assert_eq!(result["valid"], true);
    assert_eq!(result["policy"]["network"], "allow");
    assert_eq!(result["platform_supported"], !cfg!(windows));

    let shown = run(&[
        "policy",
        "show",
        after.to_str().unwrap(),
        "--cwd",
        workspace.to_str().unwrap(),
    ]);
    assert!(
        shown.status.success(),
        "{}",
        String::from_utf8_lossy(&shown.stderr)
    );
    let result: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(result["policy"]["network"], "deny");
    assert_eq!(result["policy"]["read_only"], true);

    let diff = run(&[
        "policy",
        "diff",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
        "--cwd",
        workspace.to_str().unwrap(),
    ]);
    assert!(
        diff.status.success(),
        "{}",
        String::from_utf8_lossy(&diff.stderr)
    );
    let result: Value = serde_json::from_slice(&diff.stdout).unwrap();
    assert_eq!(result["changes"]["network"]["before"], "allow");
    assert_eq!(result["changes"]["network"]["after"], "deny");
    assert_eq!(result["changes"]["read_only"]["after"], true);

    let profiles = run(&["policy", "profiles"]);
    assert!(profiles.status.success());
    let profiles: Value = serde_json::from_slice(&profiles.stdout).unwrap();
    for name in ["solmu", "codex", "claude-code", "opencode", "pi"] {
        assert!(
            profiles
                .as_array()
                .unwrap()
                .iter()
                .any(|profile| profile["name"] == name)
        );
    }
}

#[test]
fn policy_validate_rejects_duplicate_keys_and_invalid_combinations() {
    let temp = tempfile::tempdir().unwrap();
    let duplicate = temp.path().join("duplicate.json");
    std::fs::write(
        &duplicate,
        r#"{"version":1,"mode":"workspace","network":"allow","network":"deny"}"#,
    )
    .unwrap();
    let output = run(&["policy", "validate", duplicate.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Duplicate policy key"));

    let invalid = temp.path().join("invalid.json");
    std::fs::write(&invalid, r#"{"version":1,"mode":"workspace","cpus":2}"#).unwrap();
    let output = run(&["policy", "validate", invalid.to_str().unwrap()]);
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Resource controls require"));
}

#[test]
fn policy_schema_describes_the_supported_policy_fields() {
    let schema = run(&["policy", "schema"]);
    assert!(
        schema.status.success(),
        "{}",
        String::from_utf8_lossy(&schema.stderr)
    );
    let schema: Value = serde_json::from_slice(&schema.stdout).unwrap();
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["required"], serde_json::json!(["version", "mode"]));
    assert_eq!(schema["additionalProperties"], false);
    for field in [
        "network",
        "network_profile",
        "hosts",
        "local",
        "publish",
        "read_only",
        "read",
        "write",
        "clean_env",
        "pass_env",
        "env_credentials",
        "runtime_groups",
        "credentials",
        "cpus",
        "memory_mib",
        "pids",
        "cgroup_root",
    ] {
        assert!(schema["properties"][field].is_object(), "missing {field}");
    }
    let invalid_arguments = run(&["policy", "schema", "unexpected"]);
    assert_eq!(invalid_arguments.status.code(), Some(125));
}
