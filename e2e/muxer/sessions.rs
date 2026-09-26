use super::*;

pub(super) struct Session<'a> {
    pub(super) backend: &'a Backend,
    pub(super) name: &'a str,
}
impl Session<'_> {
    pub(super) fn command(&self, args: &[&str]) -> std::process::Output {
        std::process::Command::new(binary("muxer"))
            .args(args)
            .arg("--session")
            .arg(self.name)
            .current_dir(self.backend.directory.path())
            .env("SOLMU_BACKEND_URL", &self.backend.url)
            .env("SOLMU_CLI_PATH", binary("solmu-cli"))
            .env(
                "SOLMU_MUXER_DIR",
                self.backend.directory.path().join("muxer-state"),
            )
            .output()
            .unwrap()
    }
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        let _ = self.command(&["server", "stop"]);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn detach_keeps_stream_running_and_reattaches_the_same_conversation() {
    let backend = Backend::start().await;
    let _session = Session {
        backend: &backend,
        name: "default",
    };
    let mut tui = Terminal::start_session(&backend, "default");
    tui.wait("Solmu 1 · idle").await;
    tui.wait("Ready").await;
    tui.command("/rename Persistent conversation");
    tui.wait("SOLMU    Persistent conversation").await;
    tui.wait("Ready").await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    tui.command("Keep answering while detached");
    tui.wait("Solmu 1 · working").await;
    tui.exit().await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len()
            != 2
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap();
    let mut attached = Terminal::start_session(&backend, "default");
    attached.wait("SOLMU    Persistent conversation").await;
    attached.wait("Keep answering while detached").await;
    attached.wait("Hello from Solmu").await;
    attached.wait("Ready").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    attached.prefix('v');
    attached.wait("Solmu 2 · idle").await;
    attached.wait("Ready").await;
    attached.command("/rename Second conversation");
    attached.wait("SOLMU    Second conversation").await;
    attached.wait("Ready").await;
    attached.wait("Detach").await;
    solmu_e2e::support::capture_terminal(attached.screen.lock().unwrap().screen(), "muxer");
    attached.click("Detach").await;
    attached.wait_exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clients_have_independent_tabs_and_survive_other_client_disconnects() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "shared",
    };
    let mut first = Terminal::start_session(&backend, "shared");
    first.wait("Ready").await;
    first.command("/rename First shared conversation");
    first.wait("SOLMU    First shared conversation").await;
    first.wait("Ready").await;
    first.prefix('n');
    first.wait("Solmu 2 · idle").await;
    first.wait("Ready").await;
    first.command("/rename Second shared conversation");
    first.wait("SOLMU    Second shared conversation").await;
    first.wait("Ready").await;
    let second = Terminal::start_session(&backend, "shared");
    second.wait("SOLMU    Second shared conversation").await;
    second.prefix('1');
    second.wait("SOLMU    First shared conversation").await;
    second
        .wait_absent("SOLMU    Second shared conversation")
        .await;
    first.wait("SOLMU    Second shared conversation").await;
    first.resize(44, 200);
    first.wait_resized(200).await;
    second.resize(18, 60);
    second.wait_resized(60).await;
    second.command("Answer in the first shared pane");
    second.wait("Hello from Solmu").await;
    first.wait_absent("Answer in the first shared pane").await;
    // An abrupt client termination also leaves the server and panes alive.
    drop(second);
    assert!(session.command(&["server", "status"]).status.success());
    first.command("Answer in the second shared pane");
    first.wait("Hello from Solmu").await;
    first.wait("Ready").await;
    first.exit().await;
    let mut restored = Terminal::start_session(&backend, "shared");
    let saved = session.command(&["pane", "read", "2"]);
    assert!(saved.status.success());
    assert!(!session.command(&["pane", "read", "999"]).status.success());
    assert!(
        String::from_utf8_lossy(&saved.stdout).contains("SOLMU    Second shared conversation"),
        "Inner pane: {}",
        String::from_utf8_lossy(&saved.stdout)
    );
    restored.wait("SOLMU    Second shared conversation").await;
    restored.wait("Answer in the second shared pane").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        2
    );
    restored.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn named_headless_sessions_list_stop_and_reject_invalid_names() {
    let backend = Backend::start().await;
    let first = Session {
        backend: &backend,
        name: "work",
    };
    let other = Session {
        backend: &backend,
        name: "side-project",
    };
    for session in [&first, &other] {
        let output = session.command(&["server", "start"]);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(session.command(&["server", "status"]).status.success());
        assert!(session.command(&["server", "start"]).status.success());
    }
    let output = first.command(&["session", "list"]);
    assert!(output.status.success());
    let listed = String::from_utf8_lossy(&output.stdout);
    assert!(listed.contains("work\trunning"));
    assert!(listed.contains("side-project\trunning"));
    let mut first_tui = Terminal::start_session(&backend, "work");
    first_tui.wait("Solmu 1 · idle").await;
    first_tui.wait("Ready").await;
    let mut other_tui = Terminal::start_session(&backend, "side-project");
    other_tui.wait("Solmu 1 · idle").await;
    other_tui.wait("Ready").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        2
    );
    first_tui.command("Do not finish after stopping the server");
    first_tui.wait("Solmu 1 · working").await;
    assert!(first.command(&["server", "stop"]).status.success());
    first_tui.wait_exit().await;
    assert!(!first.command(&["server", "status"]).status.success());
    assert!(other.command(&["server", "status"]).status.success());
    other_tui.exit().await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let threads = backend.threads().await;
    // One stopped session and one idle session: neither persisted an assistant reply.
    for thread in threads["items"].as_array().unwrap() {
        assert!(
            backend.messages(thread["id"].as_str().unwrap()).await["items"]
                .as_array()
                .unwrap()
                .len()
                <= 1
        );
    }
    let invalid = Session {
        backend: &backend,
        name: "../escape",
    };
    assert!(!invalid.command(&["server", "start"]).status.success());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clients_zoom_and_focus_independently_in_one_shared_layout() {
    let backend = Backend::start().await;
    let _session = Session {
        backend: &backend,
        name: "layout",
    };
    let mut first = Terminal::start_session(&backend, "layout");
    first.wait("Ready").await;
    first.command("/rename Shared left");
    first.wait("SOLMU    Shared left").await;
    first.wait("Ready").await;
    first.prefix('v');
    first.wait("Solmu 2 · idle").await;
    first.wait("Ready").await;
    first.command("/rename Shared right");
    first.wait("SOLMU    Shared right").await;
    first.wait("Ready").await;
    first.prefix('h');
    first.wait("› Solmu 1 · idle").await;
    first.prefix('z');
    first.wait("zoomed").await;
    first.wait_absent("Solmu 2 · idle").await;
    let mut second = Terminal::start_session(&backend, "layout");
    second.wait("SOLMU    Shared left").await;
    second.prefix('z');
    second.wait("Solmu 2 · idle").await;
    second.prefix('l');
    second.wait("› Solmu 2 · idle").await;
    second.prefix('z');
    second.wait("zoomed").await;
    second.wait_absent("Solmu 1 · idle").await;
    first.wait("SOLMU    Shared left").await;
    first.wait_absent("Solmu 2 · idle").await;
    second.prefix('x');
    second.wait("Solmu 1 · idle").await;
    first.wait("SOLMU    Shared left").await;
    first.prefix('z');
    first.wait_absent("zoomed").await;
    first.wait_absent("Solmu 2 · idle").await;
    first.exit().await;
    second.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn session_rejects_unauthenticated_and_oversized_connections_without_stalling() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "private",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let path = backend
        .directory
        .path()
        .join("muxer-state/private.endpoint");
    let endpoint: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let port = endpoint["port"].as_u64().unwrap() as u16;
    let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let hello = serde_json::to_vec(&serde_json::json!({"Hello": {"protocol": 1, "token": "invalid", "operation": "stop", "width": 80, "height": 24}})).unwrap();
    stream
        .write_all(&(hello.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&hello).unwrap();
    let mut size = [0; 4];
    stream.read_exact(&mut size).unwrap();
    let mut bytes = vec![0; u32::from_be_bytes(size) as usize];
    stream.read_exact(&mut bytes).unwrap();
    let response: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        response["Error"]
            .as_str()
            .unwrap()
            .contains("authentication")
    );
    let mut oversized = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    oversized
        .write_all(&(5u32 * 1024 * 1024).to_be_bytes())
        .unwrap();
    assert!(session.command(&["server", "status"]).status.success());
    let mut tui = Terminal::start_session(&backend, "private");
    tui.wait("Ready").await;
    tui.exit().await;
}
