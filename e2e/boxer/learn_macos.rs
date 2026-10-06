use super::*;
use serde_json::Value;

#[test]
fn macos_learn_traces_filesystem_access_using_fs_usage() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("input.txt");
    std::fs::write(&input, "learn macOS filesystem access").unwrap();
    let output = directory.path().join("learned.txt");
    let result = Command::new(binary("boxer"))
        .args(["learn", "--json", "--timeout", "10", "--", "/bin/cp"])
        .arg(&input)
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    let output = output.canonicalize().unwrap();
    assert!(
        report["filesystem"]["write"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(output))
    );
    assert!(report["network"]["outbound"].as_array().unwrap().is_empty());
}
