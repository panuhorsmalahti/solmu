use super::{sessions::Session, *};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    process::{Child, ChildStdin, Stdio},
};

fn command(session: &Session<'_>, args: &[&str]) -> Value {
    let mut values = args.to_vec();
    if values.starts_with(&["pane", "read"]) {
        values.push("--json");
    }
    let output = session.command(&values);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["result"].clone()
}
async fn ready(session: &Session<'_>) -> Value {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let value = command(session, &["pane", "get", "1"]);
            if value["state"] == "idle" && value["native"]["ready"] == true {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap()
}
struct Stream {
    child: Child,
    input: ChildStdin,
    frames: tokio::sync::mpsc::UnboundedReceiver<Value>,
}
impl Stream {
    fn new(session: &Session<'_>, args: &[&str]) -> Self {
        let mut child = session
            .process(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (sender, frames) = tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                let Ok(value) = serde_json::from_str(&line) else {
                    break;
                };
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input,
            frames,
        }
    }
    fn send(&mut self, value: Value) {
        writeln!(self.input, "{value}").unwrap();
        self.input.flush().unwrap();
    }
    fn text(&mut self, text: &str) {
        self.send(json!({"type":"input","data":STANDARD.encode(text)}));
    }
    async fn next(&mut self) -> Value {
        tokio::time::timeout(Duration::from_secs(15), self.frames.recv())
            .await
            .unwrap()
            .expect("Terminal stream ended")
    }
    async fn matching(&mut self, predicate: impl Fn(&Value) -> bool) -> Value {
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let frame = self.next().await;
                if predicate(&frame) {
                    return frame;
                }
            }
        })
        .await
        .unwrap()
    }
    async fn text_frame(&mut self, text: &str) -> Value {
        self.matching(|f| f["type"] == "frame" && screen(f).contains(text))
            .await
    }
    async fn closed(&mut self, reason: &str) {
        assert_eq!(
            self.matching(|f| f["type"] == "closed").await["reason"],
            reason
        );
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
fn screen(frame: &Value) -> String {
    let mut parser = vt100::Parser::new(
        frame["rows"].as_u64().unwrap() as u16,
        frame["cols"].as_u64().unwrap() as u16,
        0,
    );
    parser.process(&STANDARD.decode(frame["data"].as_str().unwrap()).unwrap());
    parser.screen().contents()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streams_share_live_output_control_input_and_size_while_observers_preserve_the_pane() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "direct",
    };
    let mut tui = Terminal::start_session(&backend, "direct");
    tui.wait("Ready").await;
    let before = ready(&session).await;
    let mut observer = Stream::new(
        &session,
        &[
            "terminal", "session", "observe", "1", "--cols", "120", "--rows", "30",
        ],
    );
    let initial = observer.text_frame("Ready").await;
    assert_eq!(initial["cols"], 120);
    assert_eq!(initial["instance"], before["instance"]);
    let after = command(&session, &["pane", "get", "1"]);
    assert_eq!(after["cols"], before["cols"]);
    assert_eq!(after["rows"], before["rows"]);
    command(&session, &["agent", "rename", "1", "Planner"]);
    let mut controller = Stream::new(
        &session,
        &[
            "terminal", "session", "control", "Planner", "--cols", "100", "--rows", "28",
        ],
    );
    controller.text_frame("Ready").await;
    assert_eq!(command(&session, &["pane", "get", "1"])["cols"], 100);
    controller.text("Shared draft");
    controller.text_frame("Shared draft").await;
    observer.text_frame("Shared draft").await;
    tui.send(b"Must not steal input");
    tui.wait("Pane controlled by a direct terminal").await;
    solmu_e2e::support::capture_terminal(tui.screen.lock().unwrap().screen(), "muxer-terminal");
    assert!(
        !command(&session, &["pane", "read", "1"])["text"]
            .as_str()
            .unwrap()
            .contains("Must not steal")
    );
    let forbidden = session.command(&["pane", "send-keys", "1", "Enter"]);
    assert!(!forbidden.status.success());
    assert!(String::from_utf8_lossy(&forbidden.stderr).contains("controlled"));
    controller.send(json!({"type":"resize","cols":110,"rows":29}));
    controller
        .matching(|f| f["type"] == "frame" && f["cols"] == 110 && f["rows"] == 29)
        .await;
    assert_eq!(command(&session, &["pane", "get", "1"])["cols"], 110);
    controller.text("\r");
    controller.text_frame("Hello from Solmu").await;
    observer.text_frame("Hello from Solmu").await;
    let thread = before["thread"].as_str().unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while backend.messages(thread).await["items"]
            .as_array()
            .unwrap()
            .len()
            != 2
        {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        backend.messages(thread).await["items"][0]["content"],
        "Shared draft"
    );
    controller.send(json!({"type":"release"}));
    controller.closed("released").await;
    tui.command("/rename Back in Muxer");
    tui.wait("SOLMU    Back in Muxer").await;
    observer.text_frame("Back in Muxer").await;
    tui.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn controllers_are_exclusive_takeover_preserves_observers_and_disconnect_releases_authority()
{
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "leases",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let original = ready(&session).await;
    let mut first = Stream::new(
        &session,
        &[
            "terminal", "session", "control", "1", "--cols", "97", "--rows", "27",
        ],
    );
    first.text_frame("Ready").await;
    let rejected = session.command(&["terminal", "session", "control", "1"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("already has a controller"));
    let mut observer = Stream::new(
        &session,
        &[
            "terminal", "session", "observe", "1", "--cols", "120", "--rows", "40",
        ],
    );
    observer.text_frame("Ready").await;
    let mut second = Stream::new(
        &session,
        &[
            "terminal",
            "session",
            "control",
            "1",
            "--takeover",
            "--cols",
            "105",
            "--rows",
            "31",
        ],
    );
    second.text_frame("Ready").await;
    first.closed("taken_over").await;
    second.text("Preserved draft");
    observer.text_frame("Preserved draft").await;
    second.child.kill().unwrap();
    second.child.wait().unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !command(&session, &["pane", "get", "1"])["controller"].is_null() {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    let restored = command(&session, &["pane", "get", "1"]);
    assert_eq!(restored["cols"], original["cols"]);
    assert_eq!(restored["rows"], original["rows"]);
    for _ in 0.."Preserved draft".len() {
        command(&session, &["pane", "send-keys", "1", "Backspace"]);
    }
    command(&session, &["pane", "send-text", "1", "After disconnect"]);
    observer.text_frame("After disconnect").await;
    assert!(session.command(&["server", "stop"]).status.success());
    observer.closed("server_stopped").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_prompts_preserve_controller_drafts_and_quiet_streams_stay_connected() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "terminal-prompts",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let info = ready(&session).await;
    let mut controller = Stream::new(&session, &["terminal", "session", "control", "1"]);
    controller.text_frame("Ready").await;
    controller.text("My private draft");
    controller.text_frame("My private draft").await;
    // Quiet streams must outlive the five-second connection handshake timeout.
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert!(controller.child.try_wait().unwrap().is_none());
    let result = command(
        &session,
        &[
            "agent",
            "prompt",
            "1",
            "A separate queued task",
            "--wait",
            "--timeout",
            "10000",
        ],
    );
    assert_eq!(result["turn"]["state"], "succeeded");
    controller.text_frame("Hello from Solmu").await;
    assert!(
        command(&session, &["pane", "read", "1"])["text"]
            .as_str()
            .unwrap()
            .contains("My private draft")
    );
    let messages = backend.messages(info["thread"].as_str().unwrap()).await;
    assert_eq!(messages["items"].as_array().unwrap().len(), 2);
    assert_eq!(messages["items"][0]["content"], "A separate queued task");
    controller.send(json!({"type":"release"}));
    controller.closed("released").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn streams_validate_input_and_close_on_restart_without_redirecting_to_a_new_process() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "pinned",
    };
    assert!(session.command(&["server", "start"]).status.success());
    ready(&session).await;
    for args in [
        vec!["terminal", "session", "observe", "1", "--takeover"],
        vec!["terminal", "session", "control", "1", "--cols", "0"],
        vec!["terminal", "attach", "1"],
    ] {
        assert!(!session.command(&args).status.success());
    }
    let mut stream = Stream::new(&session, &["terminal", "session", "control", "1"]);
    let first = stream.text_frame("Ready").await;
    for invalid in [
        json!({"type":"input","data":"not base64"}),
        json!({"type":"resize","cols":0,"rows":30}),
        json!({"type":"scroll","lines":2001}),
    ] {
        stream.send(invalid);
        assert_eq!(
            stream.matching(|f| f["type"] == "error").await["type"],
            "error"
        );
    }
    stream.text("Draft stays in the old process");
    stream.text_frame("Draft stays").await;
    stream.text("\u{3}");
    stream.closed("exited").await;
    command(&session, &["pane", "restart", "1"]);
    let next = ready(&session).await;
    assert_ne!(next["instance"], first["instance"]);
    assert!(
        !command(&session, &["pane", "read", "1"])["text"]
            .as_str()
            .unwrap()
            .contains("Draft stays")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interactive_attachment_uses_the_existing_thread_and_detaches_without_stopping_it() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "interactive",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let before = ready(&session).await;
    let mut direct = Terminal::start_direct(&backend, "interactive", "1");
    direct.wait("Ready").await;
    direct.send(b"\x1b[200~Pasted draft\x1b[201~");
    direct.wait("Pasted draft").await;
    direct.send(&[127; 12]);
    direct.command("/rename Direct conversation");
    direct.wait("SOLMU    Direct conversation").await;
    direct.command("Message from direct attach");
    direct.wait("Hello from Solmu").await;
    direct.wait("Ready").await;
    direct.send(b"Unsent draft");
    direct.wait("Unsent draft").await;
    direct.exit().await;
    assert!(session.command(&["server", "status"]).status.success());
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        command(&session, &["pane", "get", "1"])["thread"],
        before["thread"]
    );
    let mut tui = Terminal::start_session(&backend, "interactive");
    tui.wait("Unsent draft").await;
    tui.wait("Direct conversation").await;
    tui.exit().await;
}

fn raw_packet(stream: &mut std::net::TcpStream, value: Value) {
    let bytes = serde_json::to_vec(&value).unwrap();
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&bytes).unwrap();
}
fn raw_read(stream: &mut std::net::TcpStream) -> Value {
    let mut size = [0; 4];
    stream.read_exact(&mut size).unwrap();
    let mut bytes = vec![0; u32::from_be_bytes(size) as usize];
    stream.read_exact(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authenticated_observers_cannot_send_input_resize_or_scroll_even_through_the_raw_api() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "observer-rights",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let before = ready(&session).await;
    let endpoint: Value = serde_json::from_slice(
        &std::fs::read(
            backend
                .directory
                .path()
                .join("muxer-state/observer-rights.endpoint"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut raw =
        std::net::TcpStream::connect(("127.0.0.1", endpoint["port"].as_u64().unwrap() as u16))
            .unwrap();
    raw.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    raw_packet(
        &mut raw,
        json!({"Hello":{"protocol":1,"token":endpoint["token"],"operation":"control","width":80,"height":24,"control":{"method":"terminal_open","params":{"target":1,"observe":true}}}}),
    );
    assert!(raw_read(&mut raw).get("Ready").is_some());
    assert_eq!(raw_read(&mut raw)["Terminal"]["type"], "frame");
    for request in [
        json!({"type":"input","data":STANDARD.encode("Must not type")}),
        json!({"type":"resize","cols":25,"rows":10}),
        json!({"type":"scroll","lines":10}),
    ] {
        raw_packet(&mut raw, json!({"Terminal":request}));
        loop {
            let response = raw_read(&mut raw);
            if response["Terminal"]["type"] == "error" {
                assert!(
                    response["Terminal"]["message"]
                        .as_str()
                        .unwrap()
                        .contains("Observer")
                );
                break;
            }
        }
    }
    let after = command(&session, &["pane", "get", "1"]);
    assert_eq!(after["cols"], before["cols"]);
    assert_eq!(after["rows"], before["rows"]);
    assert!(after["controller"].is_null());
    assert!(
        !command(&session, &["pane", "read", "1"])["text"]
            .as_str()
            .unwrap()
            .contains("Must not type")
    );
    raw_packet(&mut raw, json!({"Terminal":{"type":"release"}}));
    loop {
        let response = raw_read(&mut raw);
        if response["Terminal"]["type"] == "closed" {
            assert_eq!(response["Terminal"]["reason"], "released");
            break;
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_script_input_releases_its_controller_and_connection_limits_leave_the_session_usable()
 {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "terminal-limits",
    };
    assert!(session.command(&["server", "start"]).status.success());
    ready(&session).await;
    let mut bad = Stream::new(&session, &["terminal", "session", "control", "1"]);
    bad.text_frame("Ready").await;
    bad.input.write_all(b"{broken JSON}\n").unwrap();
    bad.input.flush().unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while bad.child.try_wait().unwrap().is_none() {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    assert!(!bad.child.try_wait().unwrap().unwrap().success());
    tokio::time::timeout(Duration::from_secs(10), async {
        while !command(&session, &["pane", "get", "1"])["controller"].is_null() {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    let mut streams = Vec::new();
    for _ in 0..16 {
        let mut stream = Stream::new(&session, &["terminal", "session", "observe", "1"]);
        stream.text_frame("Ready").await;
        streams.push(stream);
    }
    let rejected = session.command(&["terminal", "session", "observe", "1"]);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("limit"));
    command(
        &session,
        &["pane", "send-text", "1", "Session stays usable"],
    );
    streams[0].text_frame("Session stays usable").await;
    assert!(session.command(&["server", "status"]).status.success());
}
