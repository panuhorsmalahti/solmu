use super::*;

#[cfg(target_os = "linux")]
#[test]
fn workspace_landlock_uses_kernel_supported_filesystem_abi() {
    let workspace = tempfile::tempdir().unwrap();
    let output = Command::new(binary("boxer"))
        .args(["--workspace", "--cwd"])
        .arg(workspace.path())
        .arg("--")
        .arg("/bin/true")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let warning = String::from_utf8_lossy(&output.stderr);
    if warning.contains("device ioctl restrictions are unavailable") {
        assert!(warning.contains("Landlock ABI"));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn abstract_unix_socket_scope_blocks_host_sockets_when_landlock_v6_is_available() {
    use std::os::{linux::net::SocketAddrExt, unix::net::UnixDatagram};

    let name = format!("solmu-boxer-e2e-{}", std::process::id());
    let address = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes()).unwrap();
    let _host_socket = UnixDatagram::bind_addr(&address).unwrap();
    let workspace = tempfile::tempdir().unwrap();
    for mode in ["--workspace", "--isolated"] {
        let output = Command::new(binary("boxer"))
            .args([mode, "--scope-abstract-unix-socket", "--cwd"])
            .arg(workspace.path())
            .arg("--")
            .arg(binary("sandbox-probe"))
            .arg("--abstract-connect")
            .arg(&name)
            .output()
            .unwrap();
        if !output.status.success()
            && String::from_utf8_lossy(&output.stderr)
                .to_ascii_lowercase()
                .contains("scope")
        {
            return;
        }
        assert!(
            output.status.success(),
            "{mode}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("isolation verified"));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn signal_scope_blocks_host_processes_when_landlock_v6_is_available() {
    let workspace = tempfile::tempdir().unwrap();
    let mode = "--workspace";
    let mut target = Command::new("sleep").arg("30").spawn().unwrap();
    let output = Command::new(binary("boxer"))
        .args([mode, "--scope-signal", "--cwd"])
        .arg(workspace.path())
        .arg("--")
        .arg(binary("sandbox-probe"))
        .arg("--signal-check")
        .arg(target.id().to_string())
        .output()
        .unwrap();
    if !output.status.success()
        && String::from_utf8_lossy(&output.stderr)
            .to_ascii_lowercase()
            .contains("scope")
    {
        let _ = target.kill();
        let _ = target.wait();
        return;
    }
    let _ = target.kill();
    let _ = target.wait();
    assert!(
        output.status.success(),
        "{mode}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("signal isolation verified"));
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
    let mut child = Command::new(binary("boxer"))
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
    for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        let mut interrupted = Command::new(binary("boxer"))
            .args(["--isolated", "--cwd"])
            .arg(workspace.path())
            .arg("--")
            .arg(binary("sandbox-probe"))
            .arg("--sleep")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let prefix = format!("solmu-{}-", interrupted.id());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let group = loop {
            if let Some(path) = std::fs::read_dir(&root)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| {
                    path.file_name()
                        .unwrap()
                        .to_string_lossy()
                        .starts_with(&prefix)
                })
                && std::fs::read_to_string(path.join("cgroup.procs"))
                    .is_ok_and(|value| !value.trim().is_empty())
            {
                break path;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "signal probe failed to start"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        assert_eq!(unsafe { libc::kill(interrupted.id() as i32, signal) }, 0);
        assert_eq!(interrupted.wait().unwrap().code(), Some(128 + signal));
        assert!(
            !group.exists(),
            "signal cancellation must remove its cgroup"
        );
    }
    let output = Command::new(binary("boxer"))
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
    let output = Command::new(binary("boxer"))
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
    let mut command = Command::new(binary("boxer"));
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
    let output = Command::new(binary("boxer"))
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
    for directory in ["/", "/sys", "/proc", "/dev"] {
        let output = Command::new(binary("boxer"))
            .args(["--isolated", "--cwd", directory, "--", "true"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(125));
    }
}
