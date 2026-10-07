use super::*;
use std::process::Stdio;

#[test]
fn windows_rejects_file_deletion_protection_instead_of_ignoring_it() {
    let output = Command::new(binary("boxer"))
        .args(["--protect-unlink", "--", "cmd", "/c", "exit 0"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not supported"));
}

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

#[cfg(windows)]
#[test]
fn windows_rejects_filesystem_deny_rules_before_starting_the_program() {
    let directory = tempfile::tempdir().unwrap();
    let denied = directory.path().join("secret.txt");
    let marker = directory.path().join("must-not-start.txt");
    std::fs::write(&denied, "secret").unwrap();
    let output = Command::new(binary("boxer"))
        .arg("--deny")
        .arg(&denied)
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg(&marker)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not supported"));
    assert!(!marker.exists());
}

#[cfg(windows)]
#[test]
fn windows_boxer_ps_lists_attached_launches_and_finished_history() {
    let sessions = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let mut launch = Command::new(binary("boxer"))
        .args(["--cwd"])
        .arg(workspace.path())
        .args([
            "--",
            "powershell",
            "-NoProfile",
            "-Command",
            "Start-Sleep -Seconds 20",
        ])
        .env("BOXER_SESSIONS_DIR", sessions.path())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    assert!(
        launch.try_wait().unwrap().is_none(),
        "test launch exited too early"
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let launches = loop {
        let listing = Command::new(binary("boxer"))
            .args(["sessions", "list", "--json"])
            .env("BOXER_SESSIONS_DIR", sessions.path())
            .output()
            .unwrap();
        assert!(listing.status.success());
        let launches: serde_json::Value = serde_json::from_slice(&listing.stdout).unwrap();
        if launches.as_array().unwrap().first().is_some_and(|session| {
            session["pid"].as_u64().unwrap_or_default() as u32 != launch.id()
        }) {
            break launches;
        }
        assert!(
            launch.try_wait().unwrap().is_none(),
            "Boxer launcher exited before its record appeared"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "Boxer launch did not appear in the session list; stdout={:?}; stderr={}; files={:?}; dir={}",
            String::from_utf8_lossy(&listing.stdout),
            String::from_utf8_lossy(&listing.stderr),
            std::fs::read_dir(sessions.path())
                .unwrap()
                .map(|entry| entry.unwrap().file_name())
                .collect::<Vec<_>>(),
            sessions.path().display()
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let launch_info = &launches[0];
    assert_eq!(launch_info["status"], "running");
    assert_eq!(launch_info["attached"], true);
    assert_eq!(launch_info["detached"], false);
    assert_ne!(launch_info["pid"].as_u64().unwrap() as u32, launch.id());

    let stopped = Command::new(binary("boxer"))
        .args(["stop", launch_info["id"].as_str().unwrap(), "--force"])
        .env("BOXER_SESSIONS_DIR", sessions.path())
        .output()
        .unwrap();
    assert!(
        stopped.status.success(),
        "boxer stop failed: stdout={}; stderr={}",
        String::from_utf8_lossy(&stopped.stdout),
        String::from_utf8_lossy(&stopped.stderr)
    );
    assert!(!launch.wait().unwrap().success());
    let listing = Command::new(binary("boxer"))
        .args(["ps", "--all", "--json"])
        .env("BOXER_SESSIONS_DIR", sessions.path())
        .output()
        .unwrap();
    assert!(listing.status.success());
    let launches: serde_json::Value = serde_json::from_slice(&listing.stdout).unwrap();
    assert_eq!(launches[0]["status"], "finished");
    assert_eq!(launches[0]["exit_code"], 1);
}
