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
