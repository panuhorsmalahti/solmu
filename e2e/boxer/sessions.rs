#![cfg(unix)]

use super::*;

#[test]
fn detached_sessions_can_be_inspected_logged_stopped_and_pruned() {
    let directory = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let start = Command::new(binary("boxer"))
        .args(["--detached", "--cwd"])
        .arg(workspace.path())
        .args([
            "--",
            "/bin/sh",
            "-c",
            "echo boxer-session-ready; exec sleep 60",
        ])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(
        start.status.success(),
        "{}",
        String::from_utf8_lossy(&start.stderr)
    );
    let text = String::from_utf8_lossy(&start.stdout);
    let id = text
        .lines()
        .find_map(|line| line.strip_prefix("Detached Boxer session: "))
        .unwrap();

    let inspect = Command::new(binary("boxer"))
        .args(["sessions", "inspect", id])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(inspect.status.success());
    assert!(String::from_utf8_lossy(&inspect.stdout).contains("Status: running"));
    assert!(String::from_utf8_lossy(&inspect.stdout).contains(workspace.path().to_str().unwrap()));

    let mut log_found = false;
    for _ in 0..40 {
        let output = Command::new(binary("boxer"))
            .args(["sessions", "logs", id])
            .env("BOXER_SESSIONS_DIR", directory.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        if String::from_utf8_lossy(&output.stdout).contains("boxer-session-ready") {
            log_found = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(log_found, "detached session did not write its log");

    let stop = Command::new(binary("boxer"))
        .args(["sessions", "stop", id])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(
        stop.status.success(),
        "{}",
        String::from_utf8_lossy(&stop.stderr)
    );
    assert!(String::from_utf8_lossy(&stop.stdout).contains("stopped"));

    let prune = Command::new(binary("boxer"))
        .args(["sessions", "prune"])
        .env("BOXER_SESSIONS_DIR", directory.path())
        .output()
        .unwrap();
    assert!(prune.status.success());
    assert!(String::from_utf8_lossy(&prune.stdout).contains("Removed 1 finished session"));
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}
