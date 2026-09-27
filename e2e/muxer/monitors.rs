use super::{sessions::Session, *};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};
use std::process::{Child, Stdio};

fn command(session: &Session<'_>, args: &[&str]) -> Value {
    let output = session.command(args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["result"].clone()
}
pub(super) struct Pending(Option<Child>);
impl Pending {
    pub(super) fn new(session: &Session<'_>, args: &[&str]) -> Self {
        Self(Some(
            session
                .process(args)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        ))
    }
    pub(super) async fn finish(mut self, success: bool) -> Value {
        tokio::time::timeout(Duration::from_secs(15), async {
            while self.0.as_mut().unwrap().try_wait().unwrap().is_none() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let output = self.0.take().unwrap().wait_with_output().unwrap();
        assert_eq!(
            output.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(if success {
            &output.stdout
        } else {
            &output.stderr
        })
        .unwrap()
    }
    pub(super) fn cancel(mut self) {
        let child = self.0.as_mut().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
struct Stream {
    child: Child,
    events: tokio::sync::mpsc::UnboundedReceiver<Value>,
}
impl Stream {
    fn new(session: &Session<'_>, args: &[&str]) -> Self {
        let mut child = session
            .process(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let output = child.stdout.take().unwrap();
        let (sender, events) = tokio::sync::mpsc::unbounded_channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let value: Value = serde_json::from_str(&line.unwrap()).unwrap();
                if sender.send(value["result"].clone()).is_err() {
                    break;
                }
            }
        });
        Self { child, events }
    }
    async fn next(&mut self) -> Value {
        tokio::time::timeout(Duration::from_secs(10), self.events.recv())
            .await
            .unwrap()
            .expect("Event stream ended early")
    }
    async fn finish(&mut self) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while self.child.try_wait().unwrap().is_none() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let status = self.child.wait().unwrap();
        assert!(status.success());
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
async fn ready(session: &Session<'_>) -> Value {
    let reply = command(
        session,
        &["pane", "wait", "1", "--until", "idle", "--timeout", "10000"],
    );
    assert_eq!(reply["matched_state"], "idle");
    command(
        session,
        &[
            "pane",
            "wait-output",
            "1",
            "--match",
            "Ready",
            "--timeout",
            "10000",
        ],
    );
    reply["pane"].clone()
}
async fn registered(session: &Session<'_>) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while command(session, &["api", "snapshot"])["active_monitors"]
            .as_u64()
            .unwrap()
            == 0
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn state_and_output_waits_match_existing_and_future_real_solmu_activity() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "waits",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let pane = ready(&session).await;
    let immediate = command(
        &session,
        &[
            "pane",
            "wait-output",
            "1",
            "--regex",
            "(?m)^.*Ready.*$",
            "--timeout",
            "1000",
        ],
    );
    assert!(
        immediate["matched_text"]
            .as_str()
            .unwrap()
            .contains("Ready")
    );
    let future = Pending::new(
        &session,
        &[
            "pane",
            "wait-output",
            "1",
            "--regex",
            "Hello\\s+from\\s+Solmu",
            "--timeout",
            "10000",
        ],
    );
    command(
        &session,
        &["pane", "send-text", "1", "A response to wait for"],
    );
    command(&session, &["pane", "send-keys", "1", "enter"]);
    let working = command(
        &session,
        &[
            "pane",
            "wait",
            "1",
            "--until",
            "working",
            "--timeout",
            "10000",
        ],
    );
    assert_eq!(working["matched_state"], "working");
    assert_eq!(working["pane"]["instance"], pane["instance"]);
    let idle = Pending::new(
        &session,
        &["pane", "wait", "1", "--until", "idle", "--timeout", "10000"],
    );
    let result = future.finish(true).await;
    assert_eq!(result["result"]["matched_text"], "Hello from Solmu");
    assert_eq!(result["result"]["pane"]["thread"], pane["thread"]);
    assert_eq!(idle.finish(true).await["result"]["matched_state"], "idle");
    assert_eq!(
        backend.messages(pane["thread"].as_str().unwrap()).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let exited = Pending::new(
        &session,
        &[
            "pane",
            "wait",
            "1",
            "--until",
            "exited",
            "--timeout",
            "10000",
        ],
    );
    command(&session, &["pane", "send-text", "1", "/exit"]);
    command(&session, &["pane", "send-keys", "1", "enter"]);
    assert_eq!(
        exited.finish(true).await["result"]["matched_state"],
        "exited"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn waits_timeout_and_cannot_be_satisfied_by_a_replacement_or_closed_pane() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "pins",
    };
    assert!(session.command(&["server", "start"]).status.success());
    ready(&session).await;
    let timeout = Pending::new(
        &session,
        &[
            "pane",
            "wait-output",
            "1",
            "--match",
            "Never appears",
            "--timeout",
            "100",
        ],
    );
    assert!(
        timeout.finish(false).await["error"]
            .as_str()
            .unwrap()
            .contains("timed out")
    );
    command(&session, &["pane", "split", "1"]);
    command(&session, &["pane", "send-text", "1", "/exit"]);
    command(&session, &["pane", "send-keys", "1", "enter"]);
    command(
        &session,
        &[
            "pane",
            "wait",
            "1",
            "--until",
            "exited",
            "--timeout",
            "10000",
        ],
    );
    let replacement = Pending::new(
        &session,
        &[
            "pane",
            "wait",
            "1",
            "--until",
            "working",
            "--timeout",
            "10000",
        ],
    );
    registered(&session).await;
    command(&session, &["pane", "restart", "1"]);
    assert!(
        replacement.finish(false).await["error"]
            .as_str()
            .unwrap()
            .contains("occupant changed")
    );
    ready(&session).await;
    let closing = Pending::new(
        &session,
        &[
            "pane",
            "wait-output",
            "1",
            "--match",
            "Never appears",
            "--timeout",
            "10000",
        ],
    );
    registered(&session).await;
    command(&session, &["pane", "close", "1"]);
    assert!(
        closing.finish(false).await["error"]
            .as_str()
            .unwrap()
            .contains("closed")
    );
    for args in [
        vec!["pane", "wait", "2", "--until", "done"],
        vec!["pane", "wait-output", "2", "--regex", "["],
        vec!["events", "--timeout", "0"],
    ] {
        let output = session.command(&args);
        assert_eq!(output.status.code(), Some(2));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn subscriptions_stream_bootstrap_and_live_changes_then_end_by_count_or_timeout() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "events",
    };
    assert!(session.command(&["server", "start"]).status.success());
    ready(&session).await;
    let mut global = Stream::new(&session, &["events", "--timeout", "10000"]);
    let first = global.next().await;
    assert_eq!(first["type"], "snapshot");
    assert_eq!(first["sequence"], 1);
    assert_eq!(first["snapshot"]["panes"].as_array().unwrap().len(), 1);
    let mut pane = Stream::new(
        &session,
        &[
            "events",
            "--pane",
            "1",
            "--count",
            "2",
            "--timeout",
            "10000",
        ],
    );
    assert_eq!(pane.next().await["snapshot"]["id"], 1);
    command(&session, &["pane", "rename", "1", "Observed"]);
    let changed = pane.next().await;
    assert_eq!(changed["type"], "changed");
    assert_eq!(changed["sequence"], 2);
    assert_eq!(changed["snapshot"]["name"], "Observed");
    pane.finish().await;
    loop {
        let event = global.next().await;
        if event["snapshot"]["panes"][0]["name"] == "Observed" {
            break;
        }
    }
    let timed = session.command(&["events", "--timeout", "100"]);
    assert!(timed.status.success());
    let lines = String::from_utf8_lossy(&timed.stdout);
    assert!(lines.lines().count() >= 1);
    assert_eq!(
        serde_json::from_str::<Value>(lines.lines().next().unwrap()).unwrap()["result"]["type"],
        "snapshot"
    );
    let single = session.command(&[
        "api",
        "request",
        "{\"method\":\"subscribe\",\"params\":{\"count\":1}}",
    ]);
    assert!(single.status.success());
    assert_eq!(String::from_utf8_lossy(&single.stdout).lines().count(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn monitor_limits_and_disconnected_waiters_leave_the_session_responsive() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "limits",
    };
    let mut tui = Terminal::start_session(&backend, "limits");
    tui.wait("Ready").await;
    let endpoint: Value = serde_json::from_slice(
        &std::fs::read(backend.directory.path().join("muxer-state/limits.endpoint")).unwrap(),
    )
    .unwrap();
    let mut streams = Vec::new();
    for _ in 0..16 {
        let mut stream =
            std::net::TcpStream::connect(("127.0.0.1", endpoint["port"].as_u64().unwrap() as u16))
                .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        super::control::wire_write(
            &mut stream,
            &json!({"Hello":{"protocol":1,"token":endpoint["token"],"operation":"control","width":80,"height":24,"control":{"method":"wait_output","params":{"pane":1,"pattern":"Never appears"}}}}),
        );
        assert!(super::control::wire_read(&mut stream)["Ready"].is_object());
        streams.push(stream);
    }
    let rejected = session.command(&["events", "--count", "1"]);
    assert_eq!(rejected.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("Sixteen"));
    command(&session, &["pane", "rename", "1", "Still responsive"]);
    tui.wait("Still responsive #1").await;
    drop(streams);
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if session
                .command(&["events", "--count", "1"])
                .status
                .success()
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    tui.exit().await;
}
