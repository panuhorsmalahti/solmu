use super::*;
use serde_json::{Value, json};

#[test]
fn routed_policies_validate_exact_destinations_and_show_platform_support() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("routes.json");
    std::fs::write(&source, json!({"version":1,"mode":"isolated","network":"proxy","hosts":["API.OPENAI.COM","api.openai.com:443"],"local":["127.0.0.1:3000","[::1]:3001"],"publish":[4000]}).to_string()).unwrap();
    let output = Command::new(binary("boxer"))
        .arg("--policy")
        .arg(&source)
        .args(["--print-policy", "--", "missing-program"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let preview: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(preview["network"], "routed");
    assert_eq!(preview["policy"]["hosts"], json!(["api.openai.com:443"]));
    assert_eq!(preview["policy"]["publish"], json!([4000]));
    assert_eq!(preview["platform_supported"], cfg!(target_os = "linux"));
    for args in [
        vec!["--network", "proxy"],
        vec!["--isolated", "--allow-host", "example.com"],
        vec![
            "--isolated",
            "--network",
            "proxy",
            "--allow-host",
            "*.example.com",
        ],
        vec![
            "--isolated",
            "--network",
            "proxy",
            "--allow-host",
            "https://example.com",
        ],
        vec![
            "--isolated",
            "--network",
            "proxy",
            "--allow-host",
            "127.0.0.1:3000",
        ],
        vec![
            "--isolated",
            "--network",
            "proxy",
            "--allow-host",
            "example.com:0",
        ],
        vec![
            "--isolated",
            "--network",
            "proxy",
            "--allow-local",
            "localhost:3000",
        ],
        vec![
            "--isolated",
            "--network",
            "proxy",
            "--allow-local",
            "192.168.1.1:3000",
        ],
        vec!["--isolated", "--network", "proxy", "--publish", "0"],
        vec![
            "--isolated",
            "--network",
            "proxy",
            "--allow-local",
            "127.0.0.1:3000",
            "--publish",
            "3000",
        ],
    ] {
        let output = Command::new(binary("boxer"))
            .args(&args)
            .arg("--print-policy")
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(125), "{args:?}");
    }
}

#[cfg(not(target_os = "linux"))]
#[test]
fn unsupported_routed_network_never_starts_the_program() {
    let directory = tempfile::tempdir().unwrap();
    let marker = directory.path().join("must-not-run");
    let output = Command::new(binary("boxer"))
        .args(["--isolated", "--network", "proxy", "--"])
        .arg(binary("sandbox-probe"))
        .arg(&marker)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(125));
    assert!(!marker.exists());
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_routes_proxy_acl_dns_rebinding_and_direct_socket_bypasses_are_enforced() {
    use std::os::unix::net::UnixListener;
    let allowed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let allowed_address = allowed.local_addr().unwrap().to_string();
    let forbidden = std::net::TcpListener::bind("127.0.0.2:0").unwrap();
    forbidden.set_nonblocking(true).unwrap();
    let forbidden_address = forbidden.local_addr().unwrap().to_string();
    let directory = tempfile::tempdir().unwrap();
    let socket_path = directory.path().join("host.sock");
    let unix = UnixListener::bind(&socket_path).unwrap();
    unix.set_nonblocking(true).unwrap();
    let server = tokio::spawn(async move {
        for _ in 0..4 {
            let (mut client, _) = allowed.accept().await.unwrap();
            tokio::spawn(async move {
                let mut request = Vec::new();
                while !request.ends_with(b"\r\n\r\n") {
                    let mut byte = [0];
                    client.read_exact(&mut byte).await.unwrap();
                    request.push(byte[0]);
                }
                client.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 13\r\nConnection: close\r\n\r\nallowed route").await.unwrap();
            });
        }
    });
    let cwd = directory.path().to_owned();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(binary("boxer"))
            .args([
                "--isolated",
                "--network",
                "proxy",
                "--allow-local",
                &allowed_address,
                "--allow-host",
                &format!("localhost:{}", allowed_address.rsplit_once(':').unwrap().1),
                "--deny-host",
                "localhost",
                "--cwd",
            ])
            .arg(cwd)
            .arg("--")
            .arg(binary("sandbox-probe"))
            .arg("--proxy-check")
            .env("SOLMU_TEST_ALLOWED", allowed_address)
            .env("SOLMU_TEST_FORBIDDEN", forbidden_address)
            .env("SOLMU_TEST_UNIX", socket_path)
            .env("HTTP_PROXY", "http://127.0.0.1:1")
            .env("NO_PROXY", "*")
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "{:?}: {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("routed network enforced"));
    assert_eq!(
        forbidden.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        unix.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), server)
        .await
        .unwrap()
        .unwrap();
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn explicitly_allowed_public_hostname_supports_https_with_certificate_verification() {
    let directory = tempfile::tempdir().unwrap();
    let cwd = directory.path().to_owned();
    let output = tokio::task::spawn_blocking(move || {
        Command::new(binary("boxer"))
            .args([
                "--isolated",
                "--network",
                "proxy",
                "--allow-host",
                "example.com",
                "--cwd",
            ])
            .arg(cwd)
            .args([
                "--",
                "/usr/bin/curl",
                "--disable",
                "--fail",
                "--silent",
                "--show-error",
                "--max-time",
                "15",
                "https://example.com/",
            ])
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Example Domain"));
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn published_solmu_backend_streams_provider_replies_and_blocks_unlisted_tool_routes() {
    use eventsource_stream::Eventsource;
    use futures_util::StreamExt;
    let backend = solmu_e2e::support::Backend::routed().await;
    let listener = std::net::TcpListener::bind("127.0.0.2:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let thread = backend.create_thread("Routed backend").await;
    let id = thread["id"].as_str().unwrap();
    let command = format!(
        "/usr/bin/curl --noproxy '*' --silent --show-error --max-time 2 http://{}/",
        listener.local_addr().unwrap()
    );
    let message = backend.send_message(id, &format!("TOOLS {}", json!([
        {"name":"Write","arguments":{"path":"routed.txt","content":"workspace allowed"}},
        {"name":"Bash","arguments":{"command":command}}
    ]))).await;
    let response = backend
        .client
        .post(backend.endpoint(&format!("/api/v1/threads/{id}/responses")))
        .json(&json!({"message_id":message["id"]}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let mut events = response.bytes_stream().eventsource();
    let mut done = false;
    while let Some(event) = events.next().await {
        if event.unwrap().event == "done" {
            done = true;
        }
    }
    assert!(done);
    assert_eq!(
        backend.messages(id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(!backend.requests.lock().unwrap().is_empty());
    assert_eq!(
        std::fs::read_to_string(backend.directory.path().join("routed.txt")).unwrap(),
        "workspace allowed"
    );
    let tools: Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}/tools")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(tools["items"][0]["status"], "completed");
    assert_eq!(tools["items"][1]["status"], "failed");
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
