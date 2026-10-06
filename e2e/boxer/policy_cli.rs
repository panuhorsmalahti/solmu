use super::*;
use serde_json::{Value, json};

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
fn policy_init_scaffolds_a_named_profile_with_optional_inheritance() {
    let root = tempfile::tempdir().unwrap();
    let profiles = root.path().join("profiles");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&profiles).unwrap();
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(
        profiles.join("base.json"),
        r#"{"version":1,"mode":"workspace","network":"deny"}"#,
    )
    .unwrap();

    let created = Command::new(binary("boxer"))
        .args(["policy", "init", "reviewer", "--extends", "base"])
        .env("BOXER_PROFILE_DIR", &profiles)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let scaffold: Value =
        serde_json::from_slice(&std::fs::read(profiles.join("reviewer.json")).unwrap()).unwrap();
    assert_eq!(scaffold["extends"], "base");
    assert!(scaffold.get("mode").is_none());

    let validated = Command::new(binary("boxer"))
        .args(["policy", "validate", "reviewer", "--cwd"])
        .arg(&workspace)
        .env("BOXER_PROFILE_DIR", &profiles)
        .output()
        .unwrap();
    assert!(
        validated.status.success(),
        "{}",
        String::from_utf8_lossy(&validated.stderr)
    );
    let resolved: Value = serde_json::from_slice(&validated.stdout).unwrap();
    assert_eq!(resolved["policy"]["network"], "deny");

    let shown = Command::new(binary("boxer"))
        .args(["policy", "show", "reviewer", "--cwd"])
        .arg(&workspace)
        .env("BOXER_PROFILE_DIR", &profiles)
        .output()
        .unwrap();
    assert!(
        shown.status.success(),
        "{}",
        String::from_utf8_lossy(&shown.stderr)
    );
    let shown: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(shown["policy"]["network"], "deny");

    let repeated = Command::new(binary("boxer"))
        .args(["policy", "init", "reviewer"])
        .env("BOXER_PROFILE_DIR", &profiles)
        .output()
        .unwrap();
    assert_eq!(repeated.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&repeated.stderr).contains("Refusing to overwrite"));
}

#[test]
fn policy_init_rejects_a_missing_parent_without_creating_the_child() {
    let root = tempfile::tempdir().unwrap();
    let profiles = root.path().join("profiles");
    std::fs::create_dir(&profiles).unwrap();
    let child = root.path().join("child.json");

    let output = Command::new(binary("boxer"))
        .args(["policy", "init", "--output"])
        .arg(&child)
        .args(["--extends", "missing"])
        .env("BOXER_PROFILE_DIR", &profiles)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Cannot extend policy"));
    assert!(!child.exists());
}

#[test]
fn policy_init_supports_ordered_multiple_parents() {
    let root = tempfile::tempdir().unwrap();
    let profiles = root.path().join("profiles");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(profiles.join("first")).unwrap();
    std::fs::create_dir_all(profiles.join("second")).unwrap();
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(
        profiles.join("base-a.json"),
        r#"{"version":1,"mode":"workspace","network":"allow","read":["first"]}"#,
    )
    .unwrap();
    std::fs::write(
        profiles.join("base-b.json"),
        r#"{"version":1,"mode":"workspace","network":"deny","read":["second"]}"#,
    )
    .unwrap();

    let created = Command::new(binary("boxer"))
        .args([
            "policy",
            "init",
            "merged",
            "--extends",
            "base-a",
            "--extends",
            "base-b",
        ])
        .env("BOXER_PROFILE_DIR", &profiles)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let scaffold: Value =
        serde_json::from_slice(&std::fs::read(profiles.join("merged.json")).unwrap()).unwrap();
    assert_eq!(scaffold["extends"], json!(["base-a", "base-b"]));

    let resolved = Command::new(binary("boxer"))
        .args(["policy", "validate", "merged", "--cwd"])
        .arg(&workspace)
        .env("BOXER_PROFILE_DIR", &profiles)
        .output()
        .unwrap();
    assert!(
        resolved.status.success(),
        "{}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    let resolved: Value = serde_json::from_slice(&resolved.stdout).unwrap();
    assert_eq!(resolved["policy"]["network"], "deny");
    assert_eq!(resolved["policy"]["read"].as_array().unwrap().len(), 2);
}

#[test]
fn full_policy_scaffold_preserves_inherited_scalar_security_settings() {
    let root = tempfile::tempdir().unwrap();
    let profiles = root.path().join("profiles");
    let workspace = root.path().join("workspace");
    std::fs::create_dir_all(&profiles).unwrap();
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(
        profiles.join("base.json"),
        r#"{"version":1,"mode":"workspace","network":"deny","read_only":true,"clean_env":true,"environment":{"case_insensitive_vars":true,"deny_vars":["SECRET_*"]}}"#,
    )
    .unwrap();

    let created = Command::new(binary("boxer"))
        .args(["policy", "init", "reviewer", "--extends", "base", "--full"])
        .env("BOXER_PROFILE_DIR", &profiles)
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "{}",
        String::from_utf8_lossy(&created.stderr)
    );
    let scaffold: Value =
        serde_json::from_slice(&std::fs::read(profiles.join("reviewer.json")).unwrap()).unwrap();
    assert!(scaffold.get("read_only").is_none());
    assert!(scaffold.get("clean_env").is_none());
    assert!(
        scaffold["environment"]
            .get("case_insensitive_vars")
            .is_none()
    );
    assert_eq!(scaffold["endpoint_rules"], json!([]));
    assert_eq!(scaffold["custom_credentials"], json!({}));

    let resolved = Command::new(binary("boxer"))
        .args(["policy", "validate", "reviewer", "--cwd"])
        .arg(&workspace)
        .env("BOXER_PROFILE_DIR", &profiles)
        .output()
        .unwrap();
    assert!(
        resolved.status.success(),
        "{}",
        String::from_utf8_lossy(&resolved.stderr)
    );
    let resolved: Value = serde_json::from_slice(&resolved.stdout).unwrap();
    assert_eq!(resolved["policy"]["network"], "deny");
    assert_eq!(resolved["policy"]["read_only"], true);
    assert_eq!(resolved["policy"]["clean_env"], true);
    assert_eq!(
        resolved["policy"]["environment"]["case_insensitive_vars"],
        true
    );
    assert_eq!(
        resolved["policy"]["environment"]["deny_vars"][0],
        "SECRET_*"
    );
}

#[test]
fn policy_show_raw_preserves_declared_paths_before_resolution() {
    let root = tempfile::tempdir().unwrap();
    let policy_path = root.path().join("policy.json");
    std::fs::write(
        &policy_path,
        r#"{"version":1,"mode":"workspace","network":"allow","read":["$HOME/.ssh","relative/data"]}"#,
    )
    .unwrap();

    let output = Command::new(binary("boxer"))
        .args(["policy", "show"])
        .arg(&policy_path)
        .arg("--raw")
        .current_dir(root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let shown: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(shown["resolved"], false);
    assert_eq!(shown["policy"]["read"][0], "$HOME/.ssh");
    assert_eq!(shown["policy"]["read"][1], "relative/data");
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
        "upstream_proxy",
        "upstream_bypass",
        "hosts",
        "deny_hosts",
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
        "custom_credentials",
        "endpoint_rules",
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
