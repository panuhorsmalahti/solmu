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
    assert_eq!(profiles.as_array().unwrap().len(), 2);
}
