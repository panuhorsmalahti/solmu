use super::*;

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
        Command::new(binary("boxer"))
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
            Command::new(binary("boxer"))
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
    let output = Command::new(binary("boxer"))
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
        Command::new(binary("boxer"))
            .arg("--help")
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !Command::new(binary("boxer"))
            .arg("--unknown")
            .output()
            .unwrap()
            .status
            .success()
    );
    assert!(
        !Command::new(binary("boxer"))
            .args(["--", "solmu-nonexistent-program"])
            .output()
            .unwrap()
            .status
            .success()
    );
    #[cfg(not(target_os = "linux"))]
    assert_eq!(
        Command::new(binary("boxer"))
            .args(["--isolated", "--", "solmu-nonexistent-program"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(125)
    );
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
        Command::new(binary("boxer"))
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
