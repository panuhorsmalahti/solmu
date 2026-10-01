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
}
