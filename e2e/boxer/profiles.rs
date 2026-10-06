use super::*;
use serde_json::{Value, json};

fn profile_plan(profile: &str) -> (tempfile::TempDir, Value) {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("project");
    std::fs::create_dir(&workspace).unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--profile", profile, "--cwd"])
        .arg(&workspace)
        .arg("--print-policy")
        .env("HOME", root.path())
        .env("USERPROFILE", root.path())
        .env("OPENAI_API_KEY", "codex-test-key")
        .env("ANTHROPIC_API_KEY", "claude-test-key")
        .env("BOXER_PRIVATE_VALUE", "keep-private")
        .env("OPENCODE_SERVER_PASSWORD", "never-forward-this")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(!text.contains("test-key"));
    assert!(!text.contains("keep-private"));
    (root, serde_json::from_str(&text).unwrap())
}

#[test]
fn custom_named_profiles_load_from_the_profile_directory_and_are_discoverable() {
    let root = tempfile::tempdir().unwrap();
    let profile_directory = root.path().join("profiles");
    std::fs::create_dir(&profile_directory).unwrap();
    std::fs::write(
        profile_directory.join("reviewer.json"),
        r#"{"version":1,"mode":"workspace","network":"deny","read_only":true}"#,
    )
    .unwrap();
    let workspace = root.path().join("project");
    std::fs::create_dir(&workspace).unwrap();

    let plan = Command::new(binary("boxer"))
        .args(["--profile", "reviewer", "--cwd"])
        .arg(&workspace)
        .arg("--print-policy")
        .env("BOXER_PROFILE_DIR", &profile_directory)
        .env("HOME", root.path())
        .env("USERPROFILE", root.path())
        .output()
        .unwrap();
    assert!(
        plan.status.success(),
        "{}",
        String::from_utf8_lossy(&plan.stderr)
    );
    let plan: Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["profile"], "reviewer");
    assert_eq!(plan["policy"]["mode"], "workspace");
    assert_eq!(plan["policy"]["network"], "deny");
    assert_eq!(plan["policy"]["read_only"], true);

    let listed = Command::new(binary("boxer"))
        .args(["policy", "profiles"])
        .env("BOXER_PROFILE_DIR", &profile_directory)
        .output()
        .unwrap();
    assert!(listed.status.success());
    let listed: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|profile| profile["name"] == "reviewer")
    );

    let invalid = Command::new(binary("boxer"))
        .args(["--profile", "../reviewer", "--print-policy"])
        .env("BOXER_PROFILE_DIR", &profile_directory)
        .output()
        .unwrap();
    assert!(!invalid.status.success());
}

#[test]
fn jsonc_profiles_support_comments_trailing_commas_and_override_json_files() {
    let root = tempfile::tempdir().unwrap();
    let profile_directory = root.path().join("profiles");
    std::fs::create_dir(&profile_directory).unwrap();
    std::fs::write(
        profile_directory.join("reviewer.json"),
        r#"{"version":1,"mode":"workspace","network":"allow"}"#,
    )
    .unwrap();
    std::fs::write(
        profile_directory.join("reviewer.jsonc"),
        r#"{
          // JSONC profile wins if both formats exist.
          "version": 1,
          "mode": "workspace",
          "network": "deny",
          "environment": {"allow_vars": ["PATH",],},
        }"#,
    )
    .unwrap();
    let workspace = root.path().join("project");
    std::fs::create_dir(&workspace).unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--profile", "reviewer", "--cwd"])
        .arg(&workspace)
        .arg("--print-policy")
        .env("BOXER_PROFILE_DIR", &profile_directory)
        .env("HOME", root.path())
        .env("USERPROFILE", root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["policy"]["network"], "deny");
    assert_eq!(plan["policy"]["environment"]["allow_vars"], json!(["PATH"]));

    let listed = Command::new(binary("boxer"))
        .args(["policy", "profiles"])
        .env("BOXER_PROFILE_DIR", &profile_directory)
        .output()
        .unwrap();
    assert!(listed.status.success());
    let profiles: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(
        profiles
            .as_array()
            .unwrap()
            .iter()
            .filter(|profile| profile["name"] == "reviewer")
            .count(),
        1
    );
}

#[test]
fn custom_profiles_inherit_permission_and_environment_lists() {
    let root = tempfile::tempdir().unwrap();
    let profile_directory = root.path().join("profiles");
    std::fs::create_dir(&profile_directory).unwrap();
    let shared = root.path().join("shared");
    let local = root.path().join("local");
    std::fs::create_dir(&shared).unwrap();
    std::fs::create_dir(&local).unwrap();
    std::fs::write(
        profile_directory.join("base.json"),
        json!({
            "version": 1,
            "mode": "workspace",
            "network": "deny",
            "read": [shared],
            "environment": {"allow_vars": ["PATH"], "deny_vars": ["*_SECRET"]}
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        profile_directory.join("reviewer.json"),
        json!({
            "version": 1,
            "extends": "base",
            "read": [local],
            "environment": {"allow_vars": ["HOME"], "deny_vars": ["API_KEY"]}
        })
        .to_string(),
    )
    .unwrap();
    let workspace = root.path().join("project");
    std::fs::create_dir(&workspace).unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--profile", "reviewer", "--cwd"])
        .arg(&workspace)
        .arg("--print-policy")
        .env("BOXER_PROFILE_DIR", &profile_directory)
        .env("HOME", root.path())
        .env("USERPROFILE", root.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["policy"]["mode"], "workspace");
    assert_eq!(plan["policy"]["network"], "deny");
    let reads = plan["policy"]["read"].as_array().unwrap();
    assert!(reads.contains(&json!(shared.canonicalize().unwrap())));
    assert!(reads.contains(&json!(local.canonicalize().unwrap())));
    assert_eq!(
        plan["policy"]["environment"]["allow_vars"],
        json!(["PATH", "HOME"])
    );
    assert_eq!(
        plan["policy"]["environment"]["deny_vars"],
        json!(["*_SECRET", "API_KEY"])
    );
}

#[test]
fn custom_profile_inheritance_rejects_cycles() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("first.json"),
        r#"{"version":1,"mode":"workspace","extends":"second"}"#,
    )
    .unwrap();
    std::fs::write(
        root.path().join("second.json"),
        r#"{"version":1,"extends":"first"}"#,
    )
    .unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--policy"])
        .arg(root.path().join("first.json"))
        .arg("--print-policy")
        .env("BOXER_PROFILE_DIR", root.path())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("inheritance cycle"));
}

#[test]
fn agent_profiles_launch_the_expected_program_with_separate_writable_state() {
    for (profile, program, config, allowed, excluded) in [
        (
            "codex",
            "codex",
            "CODEX_HOME",
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
        ),
        (
            "claude-code",
            "claude",
            "CLAUDE_CONFIG_DIR",
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
        ),
    ] {
        let (root, plan) = profile_plan(profile);
        let state = root.path().join(".boxer").join("profiles").join(profile);
        assert_eq!(plan["profile"], profile);
        assert_eq!(plan["program"], program);
        assert_eq!(plan["policy"]["mode"], "workspace");
        assert_eq!(plan["network"], "allowed");
        assert_eq!(plan["platform_supported"], !cfg!(windows));
        assert_eq!(plan["environment"]["inherit"], false);
        assert!(state.join("tmp").is_dir());
        assert!(
            plan["policy"]["write"]
                .as_array()
                .unwrap()
                .contains(&json!(state.canonicalize().unwrap()))
        );
        let names = plan["environment"]["forwarded_names"].as_array().unwrap();
        assert!(names.contains(&json!("HOME")));
        assert!(names.contains(&json!("TMPDIR")));
        assert!(names.contains(&json!(config)));
        assert!(names.contains(&json!(allowed)));
        assert!(!names.contains(&json!(excluded)));
        assert!(!names.contains(&json!("BOXER_PRIVATE_VALUE")));
    }
}

#[test]
fn profile_alias_and_program_override_work() {
    let (root, plan) = profile_plan("claude");
    assert_eq!(plan["profile"], "claude-code");
    assert!(root.path().join(".boxer/profiles/claude-code").is_dir());

    let workspace = tempfile::tempdir().unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--profile", "codex", "--cwd"])
        .arg(workspace.path())
        .args(["--print-policy", "--", "custom-agent", "--flag"])
        .env("HOME", workspace.path())
        .env("USERPROFILE", workspace.path())
        .output()
        .unwrap();
    assert!(output.status.success());
    let plan: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(plan["program"], "custom-agent");
    assert_eq!(plan["arguments"], json!(["--flag"]));
}

#[test]
fn opencode_profile_uses_private_state_and_forwards_provider_auth_without_opencode_secrets() {
    let (root, plan) = profile_plan("opencode");
    let state = root.path().join(".boxer").join("profiles").join("opencode");
    assert_eq!(plan["profile"], "opencode");
    assert_eq!(plan["program"], "opencode");
    assert_eq!(plan["policy"]["mode"], "workspace");
    assert_eq!(plan["environment"]["inherit"], false);
    for directory in ["config", "data", "cache", "log", "state", "tmp"] {
        assert!(state.join(directory).is_dir(), "missing {directory}");
    }
    let names = plan["environment"]["forwarded_names"].as_array().unwrap();
    for name in [
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
        "OPENCODE_CONFIG_DIR",
        "OPENCODE_DATA_DIR",
        "OPENCODE_CACHE_DIR",
        "OPENCODE_LOG_DIR",
        "OPENCODE_STATE_DIR",
    ] {
        assert!(names.contains(&json!(name)), "missing {name}");
    }
    for name in ["BOXER_PRIVATE_VALUE", "OPENCODE_SERVER_PASSWORD"] {
        assert!(!names.contains(&json!(name)), "unexpected {name}");
    }
    let writable = plan["policy"]["write"].as_array().unwrap();
    assert!(writable.contains(&json!(state.canonicalize().unwrap())));
}

#[test]
fn pi_profile_uses_private_agent_directory_and_provider_credentials() {
    let (root, plan) = profile_plan("pi");
    let state = root.path().join(".boxer").join("profiles").join("pi");
    assert_eq!(plan["profile"], "pi");
    assert_eq!(plan["program"], "pi");
    assert_eq!(plan["policy"]["mode"], "workspace");
    assert!(state.join("agent").is_dir());
    let names = plan["environment"]["forwarded_names"].as_array().unwrap();
    for name in ["PI_CODING_AGENT_DIR", "OPENAI_API_KEY", "ANTHROPIC_API_KEY"] {
        assert!(names.contains(&json!(name)), "missing {name}");
    }
    for name in ["BOXER_PRIVATE_VALUE", "OPENCODE_SERVER_PASSWORD"] {
        assert!(!names.contains(&json!(name)), "unexpected {name}");
    }
    assert!(
        plan["policy"]["write"]
            .as_array()
            .unwrap()
            .contains(&json!(state.canonicalize().unwrap()))
    );
}

#[test]
fn unsupported_profiles_and_conflicting_policy_are_rejected() {
    let workspace = tempfile::tempdir().unwrap();
    let policy = workspace.path().join("boxer.json");
    std::fs::write(&policy, r#"{"version":1,"mode":"workspace"}"#).unwrap();
    for arguments in [
        vec!["--profile", "unknown"],
        vec!["--profile", "codex", "--profile", "claude-code"],
        vec!["--profile", "codex", "--policy", policy.to_str().unwrap()],
    ] {
        let output = Command::new(binary("boxer"))
            .args(arguments)
            .arg("--print-policy")
            .env("HOME", workspace.path())
            .env("USERPROFILE", workspace.path())
            .output()
            .unwrap();
        assert!(!output.status.success());
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn agent_profiles_run_with_their_private_home_and_writable_project() {
    for (profile, config_directory) in [("codex", ".codex"), ("claude-code", ".claude")] {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("project");
        std::fs::create_dir(&project).unwrap();
        let output = Command::new(binary("boxer"))
            .args(["--profile", profile, "--cwd"])
            .arg(&project)
            .args([
                "--", "/bin/sh", "-c",
                "printf '%s' \"$HOME\" > agent-home; printf '%s' \"${CODEX_HOME}${CLAUDE_CONFIG_DIR}\" > agent-config; printf '%s' \"${BOXER_PRIVATE_VALUE:-missing}\" > filtered",
            ])
            .env("HOME", root.path())
            .env("BOXER_PRIVATE_VALUE", "secret")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let home = root
            .path()
            .join(".boxer")
            .join("profiles")
            .join(profile)
            .canonicalize()
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(project.join("agent-home")).unwrap(),
            home.to_string_lossy()
        );
        assert_eq!(
            std::fs::read_to_string(project.join("agent-config")).unwrap(),
            home.join(config_directory).to_string_lossy()
        );
        assert_eq!(
            std::fs::read_to_string(project.join("filtered")).unwrap(),
            "missing"
        );
    }

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--profile", "opencode", "--cwd"])
        .arg(&project)
        .args([
            "--", "/bin/sh", "-c",
            "printf '%s' \"$OPENCODE_CONFIG_DIR|$OPENCODE_DATA_DIR|$OPENCODE_CACHE_DIR|$OPENCODE_LOG_DIR|$OPENCODE_STATE_DIR\" > agent-paths",
        ])
        .env("HOME", root.path())
        .env("OPENCODE_SERVER_PASSWORD", "secret")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let home = root
        .path()
        .join(".boxer/profiles/opencode")
        .canonicalize()
        .unwrap();
    let paths = ["config", "data", "cache", "log", "state"]
        .map(|name| home.join(name).to_string_lossy().into_owned())
        .join("|");
    assert_eq!(
        std::fs::read_to_string(project.join("agent-paths")).unwrap(),
        paths
    );

    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--profile", "pi", "--cwd"])
        .arg(&project)
        .args([
            "--",
            "/bin/sh",
            "-c",
            "printf '%s' \"$PI_CODING_AGENT_DIR\" > agent-path",
        ])
        .env("HOME", root.path())
        .env("PI_CODING_AGENT_DIR", root.path().join("host-pi"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let home = root
        .path()
        .join(".boxer/profiles/pi/agent")
        .canonicalize()
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(project.join("agent-path")).unwrap(),
        home.to_string_lossy()
    );
}
