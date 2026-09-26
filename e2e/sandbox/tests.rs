use solmu_e2e::support::binary;
use std::process::Command;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn permissive_kernel_sandbox_allows_files_and_network_and_preserves_exit_code() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        assert!(socket.read(&mut request).await.unwrap() > 0);
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\nnetwork allowed").await.unwrap();
    });
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().to_owned();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(binary("sandbox"))
            .args(["--cwd"])
            .arg(cwd)
            .arg("--")
            .arg(binary("sandbox-probe"))
            .arg("saved idea.txt")
            .env("SOLMU_TEST_NETWORK", address.to_string())
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(
        output.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("network allowed"));
    assert_eq!(
        std::fs::read(directory.path().join("saved idea.txt")).unwrap(),
        b"Solmu sandbox write allowed"
    );
    server.await.unwrap();
}

#[test]
fn kernel_policy_denies_writes_or_reports_unsupported_and_invalid_commands_fail() {
    for args in [
        vec!["--cpus", "0"],
        vec!["--memory-mib", "no"],
        vec!["--pids"],
        vec!["--cpus", "1"],
    ] {
        assert_eq!(
            Command::new(binary("sandbox"))
                .args(args)
                .output()
                .unwrap()
                .status
                .code(),
            Some(125)
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("blocked.txt");
    let output = Command::new(binary("sandbox"))
        .arg("--read-only")
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg(&path)
        .output()
        .unwrap();
    #[cfg(windows)]
    {
        assert_eq!(output.status.code(), Some(125));
        assert!(String::from_utf8_lossy(&output.stderr).contains("not supported"));
    }
    #[cfg(not(windows))]
    assert_eq!(
        output.status.code(),
        Some(12),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!path.exists());
    assert!(
        Command::new(binary("sandbox"))
            .arg("--help")
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !Command::new(binary("sandbox"))
            .arg("--unknown")
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !Command::new(binary("sandbox"))
            .args(["--", "solmu-nonexistent-program"])
            .output()
            .unwrap()
            .status
            .success()
    );
    #[cfg(not(target_os = "linux"))]
    assert_eq!(
        Command::new(binary("sandbox"))
            .args(["--isolated", "--", "solmu-nonexistent-program"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(125)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn isolated_limits_are_enforced_and_invalid_cgroup_roots_fail_closed() {
    let workspace = tempfile::tempdir().unwrap();
    let launcher_group = std::fs::read_to_string("/proc/self/cgroup").unwrap();
    let leaf = launcher_group
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .unwrap();
    let root = std::path::Path::new("/sys/fs/cgroup")
        .join(leaf.trim_start_matches('/'))
        .parent()
        .unwrap()
        .to_owned();
    let mut child = Command::new(binary("sandbox"))
        .args([
            "--isolated",
            "--cpus",
            "1",
            "--memory-mib",
            "64",
            "--pids",
            "8",
            "--cwd",
        ])
        .arg(workspace.path())
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg("--limits")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while !workspace.path().join("limits-ready").exists() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!(
                "limit probe exited: {status}: {:?}",
                child.wait_with_output().unwrap()
            );
        }
        assert!(
            std::time::Instant::now() < deadline,
            "limit probe did not start"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let prefix = format!("solmu-{}-", child.id());
    let group = std::fs::read_dir(&root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&prefix)
        })
        .unwrap();
    assert_eq!(
        std::fs::read_to_string(group.join("cpu.max"))
            .unwrap()
            .trim(),
        "100000 100000"
    );
    assert_eq!(
        std::fs::read_to_string(group.join("memory.max"))
            .unwrap()
            .trim(),
        "67108864"
    );
    assert_eq!(
        std::fs::read_to_string(group.join("memory.swap.max"))
            .unwrap()
            .trim(),
        "0"
    );
    assert_eq!(
        std::fs::read_to_string(group.join("pids.max"))
            .unwrap()
            .trim(),
        "8"
    );
    let events = std::fs::read_to_string(group.join("pids.events")).unwrap();
    assert_ne!(
        events.trim(),
        "max 0",
        "kernel must report an enforced process limit"
    );
    std::fs::write(workspace.path().join("limits-release"), "release").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!group.exists(), "owned cgroup must be removed on exit");
    let output = Command::new(binary("sandbox"))
        .args(["--isolated", "--memory-mib", "32", "--cwd"])
        .arg(workspace.path())
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg("--memory-limit")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(137),
        "Memory exhaustion must kill the process tree: {:?}",
        output
    );
    let output = Command::new(binary("sandbox"))
        .args(["--isolated", "--cgroup-root"])
        .arg(workspace.path())
        .arg("--cwd")
        .arg(workspace.path())
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg("unexpected-write")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(!workspace.path().join("unexpected-write").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("real cgroup v2"));
}

#[cfg(windows)]
#[test]
fn windows_job_terminates_descendants_after_the_agent_exits() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("escaped.txt");
    let output = Command::new(binary("sandbox"))
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

#[cfg(unix)]
#[tokio::test]
async fn read_only_policy_still_allows_network_requests() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        assert!(socket.read(&mut request).await.unwrap() > 0);
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\nnetwork allowed").await.unwrap();
    });
    let output = tokio::task::spawn_blocking(move || {
        Command::new(binary("sandbox"))
            .args(["--read-only", "--"])
            .arg(binary("sandbox-probe"))
            .arg("--no-write")
            .env("SOLMU_TEST_NETWORK", address.to_string())
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(
        output.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("network allowed"));
    server.await.unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn isolated_linux_workspace_network_and_host_process_boundary() {
    let workspace = tempfile::tempdir().unwrap();
    let private = tempfile::NamedTempFile::new().unwrap();
    let probe = workspace.path().join("sandbox-probe");
    std::fs::copy(binary("sandbox-probe"), &probe).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        assert!(socket.read(&mut request).await.unwrap() > 0);
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 15\r\nConnection: close\r\n\r\nnetwork allowed").await.unwrap();
    });
    let mut command = Command::new(binary("sandbox"));
    command
        .arg("--isolated")
        .arg("--cwd")
        .arg(workspace.path())
        .arg("--")
        .arg(&probe)
        .arg("workspace.txt")
        .env("SOLMU_TEST_ISOLATION", "1")
        .env("SOLMU_TEST_HIDDEN_FILE", private.path())
        .env("SOLMU_TEST_HOST_PID", std::process::id().to_string())
        .env("SOLMU_TEST_NETWORK", address.to_string())
        .env("OPENAI_API_KEY", "sandbox-fixture-key")
        .env("AWS_REGION", "sandbox-fixture-region")
        .env("SSH_AUTH_SOCK", "/host/agent.sock");
    let output = tokio::task::spawn_blocking(move || command.output().unwrap())
        .await
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(7),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(workspace.path().join("workspace.txt").exists());
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("processes, filesystem and privileges isolated")
    );
    server.await.unwrap();
    let output = Command::new(binary("sandbox"))
        .args(["--isolated", "--read-only", "--cwd"])
        .arg(workspace.path())
        .arg("--")
        .arg(&probe)
        .arg("blocked.txt")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(12),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!workspace.path().join("blocked.txt").exists());
    let output = Command::new(binary("sandbox"))
        .args(["--isolated", "--cwd", "/", "--", "true"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
}
