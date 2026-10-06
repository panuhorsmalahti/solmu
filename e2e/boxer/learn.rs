use super::*;
use serde_json::{Value, json};

#[test]
fn learn_traces_filesystem_access_and_emits_json_discovery() {
    let workspace = tempfile::tempdir().unwrap();
    let input = workspace.path().join("input.txt");
    let output = workspace.path().join("output.txt");
    std::fs::write(&input, "fixture input").unwrap();

    let result = Command::new(binary("boxer"))
        .args(["learn", "--json", "--"])
        .arg(binary("sandbox-probe"))
        .arg("--learn-fixture")
        .arg(&input)
        .arg(&output)
        .current_dir(workspace.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(report.get("policy_gaps").is_none());
    assert!(
        report["filesystem"]["read"]
            .as_array()
            .unwrap()
            .contains(&json!(input))
    );
    assert!(
        report["filesystem"]["write"]
            .as_array()
            .unwrap()
            .contains(&json!(output))
    );
    assert_eq!(report["command_exit_code"], 0);
    assert_eq!(std::fs::read(&output).unwrap(), b"learned write");
    assert!(String::from_utf8_lossy(&result.stderr).contains("learn fixture stdout"));
    assert!(String::from_utf8_lossy(&result.stderr).contains("learn fixture stderr"));
}

#[test]
fn learn_compares_discovered_access_with_a_resolved_boxer_policy() {
    let root = tempfile::tempdir().unwrap();
    let workspace = root.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let input = root.path().join("external-input.txt");
    let output = root.path().join("external-output.txt");
    std::fs::write(&input, "outside the granted workspace").unwrap();
    let policy = root.path().join("policy.json");
    std::fs::write(
        &policy,
        r#"{"version":1,"mode":"workspace","network":"deny"}"#,
    )
    .unwrap();

    let result = Command::new(binary("boxer"))
        .args(["learn", "--json", "--policy"])
        .arg(&policy)
        .args(["--"])
        .arg(binary("sandbox-probe"))
        .arg("--learn-fixture")
        .arg(&input)
        .arg(&output)
        .current_dir(&workspace)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(
        report["policy_gaps"]["filesystem"]["read"]
            .as_array()
            .unwrap()
            .contains(&json!(input))
    );
    assert!(
        report["policy_gaps"]["filesystem"]["write"]
            .as_array()
            .unwrap()
            .contains(&json!(output))
    );
    assert!(
        report["policy_gaps"]["network"]["outbound_denied"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        report["policy_gaps"]["network"]["listening_denied"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn learn_counts_repeated_network_access_to_each_endpoint() {
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let result = Command::new(binary("boxer"))
        .args(["learn", "--json", "--"])
        .arg(binary("sandbox-probe"))
        .arg("--learn-network-fixture")
        .arg(address.to_string())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    for _ in 0..2 {
        let (stream, _) = listener.accept().unwrap();
        drop(stream);
    }
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    let outbound = report["network"]["outbound"].as_array().unwrap();
    assert_eq!(outbound.len(), 1);
    assert_eq!(outbound[0]["address"], "127.0.0.1");
    assert_eq!(outbound[0]["port"], address.port());
    assert_eq!(outbound[0]["count"], 2);
}

#[test]
fn learn_requires_a_command_and_rejects_invalid_timeouts() {
    for arguments in [
        vec!["learn"],
        vec!["learn", "--", "sandbox-probe"],
        vec!["learn", "--timeout", "0", "--", "sandbox-probe"],
    ] {
        let result = Command::new(binary("boxer"))
            .args(arguments)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(125));
    }
}

#[test]
fn learn_timeout_stops_the_traced_process_group() {
    let result = Command::new(binary("boxer"))
        .args(["learn", "--json", "--timeout", "1", "--"])
        .arg(binary("sandbox-probe"))
        .arg("--sleep")
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(124));
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["timed_out"], true);
}
