use super::*;

#[cfg(windows)]
#[test]
fn windows_job_terminates_descendants_after_the_agent_exits() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("escaped.txt");
    let output = Command::new(binary("boxer"))
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg("--spawn-descendant")
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("spawned descendant"));
    std::thread::sleep(std::time::Duration::from_millis(1500));
    assert!(!path.exists(), "Descendant outlived the sandbox");
}
