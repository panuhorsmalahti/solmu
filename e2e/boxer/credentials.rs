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
        r#"{"version":1,"mode":"workspace","network":"allow","env_credentials":["OPENAI_API_KEY"],"env_credential_map":{"op://Development/OpenAI API Key/credential":"OPENAI_API_KEY_ALT"}}"#,
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
    assert_eq!(
        output["policy"]["env_credential_map"]["op://Development/OpenAI API Key/credential"],
        "OPENAI_API_KEY_ALT"
    );
    assert!(!String::from_utf8_lossy(&process.stdout).contains("test-secret"));
}

#[test]
fn environment_credential_map_accepts_secret_references_and_validates_target_names() {
    let temp = tempfile::tempdir().unwrap();
    let valid = Command::new(binary("boxer"))
        .args([
            "--env-credential-map",
            "op://Development/OpenAI API Key/credential",
            "OPENAI_API_KEY",
            "--cwd",
        ])
        .arg(temp.path())
        .args(["--print-policy", "--", "unused-program"])
        .output()
        .unwrap();
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    let policy: serde_json::Value = serde_json::from_slice(&valid.stdout).unwrap();
    assert_eq!(
        policy["policy"]["env_credential_map"]["op://Development/OpenAI API Key/credential"],
        "OPENAI_API_KEY"
    );

    let apple_reference = Command::new(binary("boxer"))
        .args([
            "--env-credential-map",
            "apple-password://github.com/alice%40example.com",
            "GITHUB_PASSWORD",
            "--cwd",
        ])
        .arg(temp.path())
        .args(["--print-policy", "--", "unused-program"])
        .output()
        .unwrap();
    assert!(
        apple_reference.status.success(),
        "{}",
        String::from_utf8_lossy(&apple_reference.stderr)
    );
    let policy: serde_json::Value = serde_json::from_slice(&apple_reference.stdout).unwrap();
    assert_eq!(
        policy["policy"]["env_credential_map"]["apple-password://github.com/alice%40example.com"],
        "GITHUB_PASSWORD"
    );

    let invalid = run(&[
        "--env-credential-map",
        "op://Development/OpenAI API Key/credential",
        "SOLMU_WORKSPACE",
        "--",
        "must-not-run",
    ]);
    assert_eq!(invalid.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("Invalid target environment"));
}

#[test]
fn credential_proxy_requires_isolated_routed_networking() {
    let output = run(&["--credential", "openai", "--", "unused-program"]);
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Credential proxying"));
}

#[test]
fn fixed_network_proxy_port_requires_isolated_proxy_networking() {
    let invalid = run(&["--proxy-port", "47891", "--", "must-not-run"]);
    assert_eq!(invalid.status.code(), Some(125));
    assert!(
        String::from_utf8_lossy(&invalid.stderr)
            .contains("requires Linux --isolated --network proxy")
    );

    let workspace = tempfile::tempdir().unwrap();
    let valid = Command::new(binary("boxer"))
        .args([
            "--isolated",
            "--network",
            "proxy",
            "--proxy-port",
            "47891",
            "--cwd",
        ])
        .arg(workspace.path())
        .args(["--print-policy", "--", "unused-program"])
        .output()
        .unwrap();
    assert!(
        valid.status.success(),
        "{}",
        String::from_utf8_lossy(&valid.stderr)
    );
    let policy: serde_json::Value = serde_json::from_slice(&valid.stdout).unwrap();
    assert_eq!(policy["policy"]["proxy_port"], 47891);
}

#[test]
fn endpoint_allowlists_require_a_brokered_credential() {
    let output = run(&[
        "--allow-endpoint",
        "openai:POST:/v1/chat/completions",
        "--",
        "unused-program",
    ]);
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("require at least one --credential"));

    let invalid = run(&["--allow-endpoint", "openai:POST:/v1/models?verbose=true"]);
    assert_eq!(invalid.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("Endpoint paths"));
}

#[cfg(target_os = "linux")]
#[test]
fn custom_credential_routes_are_configurable_and_add_only_the_upstream_host() {
    let workspace = tempfile::tempdir().unwrap();
    let policy = workspace.path().join("policy.json");
    std::fs::write(
        &policy,
        r#"{
            "version": 1,
            "mode": "isolated",
            "network": "proxy",
            "proxy_port": 47892,
            "credentials": ["example_api", "telegram", "maps", "private_api", "openai"],
            "custom_credentials": {
                "example_api": {
                    "upstream": "https://api.example.com/v1",
                    "credential_key": "EXAMPLE_API_KEY",
                    "env_var": "EXAMPLE_API_KEY",
                    "inject_header": "X-API-Key",
                    "credential_format": "Key {}"
                },
                "telegram": {
                    "upstream": "https://api.example.com",
                    "credential_key": "TELEGRAM_TOKEN",
                    "inject_mode": "url_path",
                    "path_pattern": "/bot{}/",
                    "path_replacement": "/bot{}/"
                },
                "maps": {
                    "upstream": "https://maps.example.com",
                    "credential_key": "MAPS_TOKEN",
                    "inject_mode": "query_param",
                    "query_param_name": "key"
                },
                "private_api": {
                    "upstream": "https://private.example.com",
                    "credential_key": "PRIVATE_API_TOKEN",
                    "inject_mode": "basic_auth"
                },
                "openai": {
                    "upstream": "https://openai-proxy.example.com/v1",
                    "credential_key": "op://Development/OpenAI API Key/credential",
                    "env_var": "OPENAI_API_KEY",
                    "inject_header": "Authorization",
                    "credential_format": "Bearer {}"
                }
            },
            "endpoint_rules": [
                {"provider": "example_api", "method": "GET", "path": "/v1/**"},
                {"provider": "telegram", "method": "POST", "path": "/**"},
                {"provider": "maps", "method": "GET", "path": "/places/**"},
                {"provider": "private_api", "method": "GET", "path": "/resource"},
                {"provider": "openai", "method": "POST", "path": "/v1/**"}
            ]
        }"#,
    )
    .unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--policy"])
        .arg(&policy)
        .args(["--cwd"])
        .arg(workspace.path())
        .args(["--print-policy", "--", "unused-program"])
        .env("EXAMPLE_API_KEY", "real-secret-fixture")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["policy"]["credentials"][0], "example_api");
    assert_eq!(result["policy"]["credentials"].as_array().unwrap().len(), 5);
    assert_eq!(result["policy"]["proxy_port"], 47892);
    assert_eq!(
        result["policy"]["custom_credentials"]["example_api"]["inject_header"],
        "X-API-Key"
    );
    assert_eq!(
        result["policy"]["custom_credentials"]["openai"]["credential_key"],
        "op://Development/OpenAI API Key/credential"
    );
    assert_eq!(
        result["policy"]["endpoint_rules"][0]["provider"],
        "example_api"
    );
    assert!(
        result["policy"]["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|host| host == "api.example.com:443")
    );
    assert!(
        result["policy"]["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|host| host == "openai-proxy.example.com:443")
    );
    assert!(
        !result["policy"]["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|host| host == "api.openai.com:443")
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("real-secret-fixture"));
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
            "--credential",
            "gemini",
            "--credential",
            "github",
            "--credential",
            "gitlab",
            "--allow-endpoint",
            "openai:POST:/v1/chat/completions",
            "--cwd",
        ])
        .arg(workspace.path())
        .args(["--print-policy", "--", "unused-program"])
        .env("OPENAI_API_KEY", "real-secret-fixture")
        .env("GEMINI_API_KEY", "gemini-secret-fixture")
        .env("GITHUB_TOKEN", "github-secret-fixture")
        .env("GITLAB_TOKEN", "gitlab-secret-fixture")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["policy"]["credentials"][0], "openai");
    assert_eq!(result["policy"]["credentials"][1], "gemini");
    assert_eq!(result["policy"]["credentials"][2], "github");
    assert_eq!(result["policy"]["credentials"][3], "gitlab");
    assert_eq!(
        result["policy"]["endpoint_rules"][0]["path"],
        "/v1/chat/completions"
    );
    assert!(
        result["policy"]["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|host| { host == "api.openai.com:443" })
    );
    assert!(
        result["policy"]["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|host| { host == "generativelanguage.googleapis.com:443" })
    );
    for host in ["api.github.com:443", "gitlab.com:443"] {
        assert!(
            result["policy"]["hosts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|found| found == host)
        );
    }
    assert!(
        !result["environment"]["forwarded_names"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "OPENAI_API_KEY")
    );
    assert!(
        !result["environment"]["forwarded_names"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "GEMINI_API_KEY")
    );
    for name in ["GITHUB_TOKEN", "GITLAB_TOKEN"] {
        assert!(
            !result["environment"]["forwarded_names"]
                .as_array()
                .unwrap()
                .iter()
                .any(|found| found == name)
        );
    }
    assert!(!String::from_utf8_lossy(&output.stdout).contains("real-secret-fixture"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("gemini-secret-fixture"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("github-secret-fixture"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("gitlab-secret-fixture"));
}
