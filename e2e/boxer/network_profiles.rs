use super::*;
use serde_json::Value;

#[test]
fn network_profiles_expand_to_exact_hosts_and_require_the_proxy() {
    let dir = tempfile::tempdir().unwrap();
    let output = Command::new(binary("boxer"))
        .args([
            "--isolated",
            "--network",
            "proxy",
            "--network-profile",
            "minimal",
            "--print-policy",
            "--cwd",
        ])
        .arg(dir.path())
        .arg("--")
        .arg("missing-program")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let policy: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(policy["policy"]["network_profile"], "minimal");
    assert_eq!(
        policy["policy"]["hosts"],
        serde_json::json!([
            "api.anthropic.com:443",
            "api.openai.com:443",
            "generativelanguage.googleapis.com:443"
        ])
    );

    let output = Command::new(binary("boxer"))
        .args(["--network-profile", "minimal", "--print-policy"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("require --network proxy"));

    let output = Command::new(binary("boxer"))
        .args([
            "--isolated",
            "--network",
            "proxy",
            "--network-profile",
            "unknown",
            "--print-policy",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Unknown network profile"));

    let profiles = Command::new(binary("boxer"))
        .args(["network", "profiles"])
        .output()
        .unwrap();
    assert!(profiles.status.success());
    let profiles: Value = serde_json::from_slice(&profiles.stdout).unwrap();
    assert_eq!(profiles.as_array().unwrap().len(), 6);
    let codex = Command::new(binary("boxer"))
        .args([
            "--isolated",
            "--network",
            "proxy",
            "--network-profile",
            "codex",
            "--print-policy",
        ])
        .output()
        .unwrap();
    assert!(codex.status.success());
    let codex: Value = serde_json::from_slice(&codex.stdout).unwrap();
    assert_eq!(codex["policy"]["network_profile"], "codex");
    assert!(
        codex["policy"]["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|host| { host == "fulcio.sigstore.dev:443" || host == "doc.rust-lang.org:443" })
    );
    let enterprise = Command::new(binary("boxer"))
        .args([
            "--isolated",
            "--network",
            "proxy",
            "--network-profile",
            "enterprise",
            "--print-policy",
        ])
        .output()
        .unwrap();
    assert!(enterprise.status.success());
    let enterprise: Value = serde_json::from_slice(&enterprise.stdout).unwrap();
    assert!(
        enterprise["policy"]["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|host| {
                host == "*.googleapis.com:443"
                    || host == "*.openai.azure.com:443"
                    || host == "*.bedrock-runtime.amazonaws.com:443"
            })
    );
}

#[test]
fn upstream_proxy_requires_routed_networking_and_valid_http_url() {
    let unsupported = Command::new(binary("boxer"))
        .args([
            "--upstream-proxy",
            "http://127.0.0.1:3128",
            "--print-policy",
        ])
        .output()
        .unwrap();
    assert_eq!(unsupported.status.code(), Some(125));
    assert!(
        String::from_utf8_lossy(&unsupported.stderr)
            .contains("requires Linux --isolated --network proxy")
    );

    let invalid = Command::new(binary("boxer"))
        .args([
            "--upstream-proxy",
            "https://proxy.example.com:3128",
            "--isolated",
            "--network",
            "proxy",
            "--print-policy",
        ])
        .output()
        .unwrap();
    assert_eq!(invalid.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("must use http://"));
}

#[test]
fn denied_domains_override_allowlisted_hosts_and_normalize_wildcards() {
    let directory = tempfile::tempdir().unwrap();
    let output = Command::new(binary("boxer"))
        .args([
            "--isolated",
            "--network",
            "proxy",
            "--allow-host",
            "api.example.com",
            "--deny-host",
            "*.tracking.example",
            "--deny-host",
            "build.*.ci.example.com",
            "--deny-host",
            "ADS.EXAMPLE.COM",
            "--print-policy",
            "--cwd",
        ])
        .arg(directory.path())
        .arg("--")
        .arg("unused-program")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result["policy"]["hosts"],
        serde_json::json!(["api.example.com:443"])
    );
    assert_eq!(
        result["policy"]["deny_hosts"],
        serde_json::json!([
            "*.tracking.example",
            "ads.example.com",
            "build.*.ci.example.com"
        ])
    );

    let unsupported = Command::new(binary("boxer"))
        .args(["--deny-host", "*.example.com", "--print-policy"])
        .output()
        .unwrap();
    assert_eq!(unsupported.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&unsupported.stderr).contains("require --network proxy"));

    let malformed = Command::new(binary("boxer"))
        .args([
            "--isolated",
            "--network",
            "proxy",
            "--deny-host",
            "api*bad.example.com",
            "--print-policy",
        ])
        .output()
        .unwrap();
    assert_eq!(malformed.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&malformed.stderr).contains("complete hostname label"));
}

#[cfg(target_os = "linux")]
#[test]
fn upstream_proxy_configuration_is_redacted_and_not_forwarded_to_the_agent() {
    let workspace = tempfile::tempdir().unwrap();
    let output = Command::new(binary("boxer"))
        .args([
            "--isolated",
            "--network",
            "proxy",
            "--network-profile",
            "minimal",
            "--cwd",
        ])
        .arg(workspace.path())
        .args(["--print-policy", "--", "unused-program"])
        .env(
            "BOXER_UPSTREAM_PROXY",
            "http://agent:secret@proxy.example.com:3128",
        )
        .env("BOXER_UPSTREAM_BYPASS", "git.internal.example,*.dev.local")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        result["policy"]["upstream_proxy"],
        "http://proxy.example.com:3128/"
    );
    assert_eq!(
        result["policy"]["upstream_bypass"],
        serde_json::json!(["*.dev.local", "git.internal.example"])
    );
    assert!(
        !result["environment"]["forwarded_names"]
            .as_array()
            .unwrap()
            .iter()
            .any(|name| name == "BOXER_UPSTREAM_PROXY" || name == "BOXER_UPSTREAM_BYPASS")
    );
    assert!(!String::from_utf8_lossy(&output.stdout).contains("secret"));
}
