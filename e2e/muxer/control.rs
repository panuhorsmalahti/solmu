use super::{sessions::Session, *};
use serde_json::{Value, json};

fn command(session: &Session<'_>, args: &[&str]) -> Value {
    let output = session.command(args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["ok"], true);
    value["result"].clone()
}
fn snapshot(session: &Session<'_>) -> Value {
    command(session, &["api", "snapshot"])
}
fn rejected(session: &Session<'_>, args: &[&str], code: i32, message: &str) {
    let output = session.command(args);
    assert_eq!(
        output.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let error: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(error["ok"], false);
    assert!(
        error["error"].as_str().unwrap().contains(message),
        "{error}"
    );
}
async fn idle(session: &Session<'_>, pane: u64) -> Value {
    tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            let record = command(session, &["pane", "get", &pane.to_string()]);
            if record["state"] == "idle" && record["thread"].is_string() {
                break record;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap()
}
async fn screen(session: &Session<'_>, pane: &str, expected: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(25), async {
        loop {
            let read = command(session, &["pane", "read", pane, "--json"]);
            if read["text"].as_str().unwrap().contains(expected) {
                break read;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn headless_crud_has_stable_ids_native_identity_and_persistent_names() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "automation",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let first = idle(&session, 1).await;
    assert_eq!(first["thread"].as_str().unwrap().len(), 36);
    assert!(
        backend.threads().await["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|thread| thread["id"] == first["thread"])
    );
    assert!(first["pid"].as_u64().is_some());
    assert!(first["rows"].as_u64().unwrap() > 0);
    assert_eq!(snapshot(&session)["clients"], json!([]));
    let cwd = backend.directory.path().join("project");
    std::fs::create_dir(&cwd).unwrap();
    let created = command(
        &session,
        &[
            "space",
            "create",
            "--cwd",
            cwd.to_str().unwrap(),
            "--name",
            "Research",
        ],
    );
    assert_eq!(created["space"]["id"], 2);
    assert_eq!(created["tab"]["id"], 2);
    assert_eq!(created["pane"]["id"], 2);
    assert_eq!(snapshot(&session)["active_pane"], 1);
    let tab = command(
        &session,
        &["tab", "create", "--space", "2", "--name", "Plan"],
    );
    assert_eq!(tab["tab"]["id"], 3);
    assert_eq!(snapshot(&session)["active_pane"], 1);
    assert_eq!(
        command(&session, &["tab", "list", "--space", "2"])["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        command(&session, &["space", "get", "2"])["name"],
        "Research"
    );
    command(&session, &["pane", "rename", "3", "Planner"]);
    command(&session, &["tab", "rename", "3", "Planning"]);
    command(&session, &["space", "rename", "2", "Project"]);
    command(&session, &["space", "focus", "2"]);
    assert_eq!(snapshot(&session)["active_pane"], 2);
    command(&session, &["tab", "focus", "3"]);
    assert_eq!(snapshot(&session)["active_pane"], 3);
    command(&session, &["pane", "focus", "3"]);
    assert_eq!(snapshot(&session)["active_pane"], 3);
    idle(&session, 2).await;
    let third = idle(&session, 3).await;
    let before = snapshot(&session);
    assert!(session.command(&["server", "stop"]).status.success());
    assert!(session.command(&["server", "start"]).status.success());
    let restored = idle(&session, 3).await;
    let after = snapshot(&session);
    assert_eq!(after["active_pane"], 3);
    assert_eq!(after["spaces"], before["spaces"]);
    assert_eq!(after["tabs"], before["tabs"]);
    assert_eq!(restored["name"], "Planner");
    assert_eq!(restored["thread"], third["thread"]);
    assert_ne!(restored["instance"], third["instance"]);
    command(&session, &["pane", "rename", "3", "--clear"]);
    assert!(command(&session, &["pane", "get", "3"])["name"].is_null());
    command(&session, &["tab", "close", "2"]);
    assert_eq!(command(&session, &["space", "get", "2"])["id"], 2);
    command(&session, &["space", "close", "2"]);
    assert!(cwd.is_dir());
    assert!(backend.threads().await["items"].as_array().unwrap().len() >= 3);
    let closed = command(&session, &["pane", "close", "1"]);
    assert_eq!(closed["session_stopped"], true);
    tokio::time::timeout(Duration::from_secs(10), async {
        while session.command(&["server", "status"]).status.success() {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn layouts_can_be_split_resized_swapped_and_zoomed_without_a_terminal() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "layouts",
    };
    assert!(session.command(&["server", "start"]).status.success());
    idle(&session, 1).await;
    command(
        &session,
        &["pane", "split", "1", "--ratio", "0.3", "--name", "Right"],
    );
    command(
        &session,
        &[
            "pane",
            "split",
            "2",
            "--direction",
            "down",
            "--ratio",
            "0.7",
        ],
    );
    let state = snapshot(&session);
    assert_eq!(state["active_pane"], 1);
    assert_eq!(state["tabs"][0]["layout"]["Split"]["ratio"], 300);
    assert_eq!(
        state["tabs"][0]["layout"]["Split"]["second"]["Split"]["ratio"],
        700
    );
    assert_eq!(
        command(&session, &["pane", "list", "--tab", "1"])["items"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    command(
        &session,
        &[
            "pane",
            "resize",
            "2",
            "--direction",
            "down",
            "--ratio",
            "0.4",
        ],
    );
    assert_eq!(
        snapshot(&session)["tabs"][0]["layout"]["Split"]["second"]["Split"]["ratio"],
        400
    );
    command(&session, &["pane", "swap", "1", "3"]);
    assert_eq!(
        snapshot(&session)["tabs"][0]["layout"]["Split"]["first"]["Pane"],
        3
    );
    command(&session, &["pane", "zoom", "2", "on"]);
    let state = snapshot(&session);
    assert_eq!(state["active_pane"], 2);
    assert_eq!(state["tabs"][0]["zoomed"], true);
    command(&session, &["pane", "zoom", "2", "toggle"]);
    assert_eq!(snapshot(&session)["tabs"][0]["zoomed"], false);
    command(&session, &["pane", "close", "2"]);
    assert_eq!(snapshot(&session)["tabs"][0]["panes"], json!([3, 1]));
    assert_eq!(
        snapshot(&session)["tabs"][0]["layout"]["Split"]["second"]["Pane"],
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn input_and_reads_use_the_real_solmu_terminal_and_restart_replaces_identity() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "input",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let first = idle(&session, 1).await;
    screen(&session, "1", "Ready").await;
    rejected(
        &session,
        &["pane", "send-text", "1", "Accidental\nsubmission"],
        1,
        "bracketed paste",
    );
    command(&session, &["pane", "send-text", "1", "--", "--help"]);
    screen(&session, "1", "--help").await;
    command(&session, &["pane", "send-keys", "1", "esc"]);
    command(
        &session,
        &["pane", "send-text", "1", "Message sent using the local API"],
    );
    screen(&session, "1", "Message sent using the local API").await;
    assert!(
        backend.messages(first["thread"].as_str().unwrap()).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    command(&session, &["pane", "send-keys", "1", "enter"]);
    screen(&session, "1", "Hello from Solmu").await;
    idle(&session, 1).await;
    assert_eq!(
        backend.messages(first["thread"].as_str().unwrap()).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let plain = session.command(&["pane", "read", "1"]);
    assert!(plain.status.success());
    assert!(String::from_utf8_lossy(&plain.stdout).contains("Hello from Solmu"));
    let limited = command(&session, &["pane", "read", "1", "--lines", "4", "--json"]);
    assert!(limited["text"].as_str().unwrap().lines().count() <= 4);
    let ansi = command(&session, &["pane", "read", "1", "--ansi", "--json"]);
    assert_eq!(ansi["format"], "ansi");
    assert!(ansi["text"].as_str().unwrap().contains('\u{1b}'));
    command(&session, &["pane", "send-text", "1", "/exit"]);
    command(&session, &["pane", "send-keys", "1", "enter"]);
    tokio::time::timeout(Duration::from_secs(10), async {
        while command(&session, &["pane", "get", "1"])["exited"].is_null() {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    rejected(&session, &["pane", "send-keys", "1", "enter"], 1, "stopped");
    command(&session, &["pane", "rename", "1", "Restarted"]);
    command(&session, &["pane", "restart", "1"]);
    let replacement = idle(&session, 1).await;
    assert_ne!(replacement["thread"], first["thread"]);
    assert_ne!(replacement["instance"], first["instance"]);
    assert_eq!(replacement["name"], "Restarted");
    rejected(&session, &["pane", "restart", "1"], 1, "Only stopped");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn automation_preserves_attached_drafts_and_targets_one_client_explicitly() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "views",
    };
    let mut first = Terminal::start_session(&backend, "views");
    first.wait("Ready").await;
    first.send(b"Draft stays here");
    first.wait("Draft stays here").await;
    command(&session, &["pane", "split", "1", "--name", "Planner"]);
    first.wait("Planner #2").await;
    first.wait("Draft stays here").await;
    command(
        &session,
        &["tab", "create", "--space", "1", "--name", "Background"],
    );
    first.wait("Background").await;
    first.wait("Draft stays here").await;
    let mut second = Terminal::start_session(&backend, "views");
    second.wait("Draft stays here").await;
    let state = snapshot(&session);
    let id = state["clients"][1]["id"].as_u64().unwrap().to_string();
    command(&session, &["pane", "focus", "2", "--client", &id]);
    second.wait("\u{203a} Planner #2").await;
    first.wait("\u{203a} Solmu 1").await;
    command(&session, &["pane", "zoom", "2", "on", "--client", &id]);
    second.wait("zoomed").await;
    second.wait_absent("Draft stays here").await;
    first.wait("Draft stays here").await;
    first.wait_absent("zoomed").await;
    command(&session, &["pane", "rename", "2", "Renamed"]);
    first.wait("Renamed #2").await;
    second.wait("Renamed #2").await;
    command(&session, &["pane", "close", "2"]);
    first.wait("Draft stays here").await;
    second.wait("Draft stays here").await;
    first.exit().await;
    second.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_arguments_and_failed_requests_never_mutate_or_start_a_session() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "validation",
    };
    rejected(&session, &["api", "snapshot"], 1, "");
    assert!(!session.command(&["server", "status"]).status.success());
    assert!(session.command(&["server", "start"]).status.success());
    idle(&session, 1).await;
    let before = snapshot(&session);
    for args in [
        vec!["pane", "split", "1", "--ratio", "nan"],
        vec!["pane", "split", "1", "--direction", "up"],
        vec!["tab", "create"],
        vec!["pane", "close", "0"],
        vec!["pane", "send-keys", "1", "enter", "bogus-key"],
        vec!["pane", "get", "1", "--surprise"],
        vec![
            "api",
            "request",
            "{\"method\":\"close\",\"params\":{\"target\":\"pane\",\"id\":1,\"typo\":true}}",
        ],
    ] {
        rejected(&session, &args, 2, "");
    }
    rejected(&session, &["pane", "get", "999"], 1, "does not exist");
    rejected(
        &session,
        &["tab", "list", "--space", "999"],
        1,
        "does not exist",
    );
    rejected(&session, &["space", "create", "--cwd", "missing"], 1, "");
    rejected(&session, &["pane", "rename", "1", &"a".repeat(81)], 1, "80");
    rejected(
        &session,
        &[
            "pane",
            "resize",
            "1",
            "--direction",
            "down",
            "--ratio",
            "0.4",
        ],
        1,
        "No divider",
    );
    rejected(
        &session,
        &["pane", "focus", "1", "--client", "999"],
        1,
        "Client does not exist",
    );
    rejected(
        &session,
        &["pane", "send-text", "1", "\u{1b}/exit"],
        1,
        "control keys",
    );
    assert_eq!(snapshot(&session), before);
    let raw = json!({"method":"rename", "params":{"target":"pane", "id":1, "name":"Raw API"}})
        .to_string();
    assert_eq!(
        command(&session, &["api", "request", &raw])["name"],
        "Raw API"
    );
    assert!(session.command(&["api", "--help"]).status.success());
}

fn wire_write(stream: &mut std::net::TcpStream, value: &Value) {
    let bytes = serde_json::to_vec(value).unwrap();
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&bytes).unwrap();
}
fn wire_read(stream: &mut std::net::TcpStream) -> Value {
    let mut size = [0; 4];
    stream.read_exact(&mut size).unwrap();
    let mut bytes = vec![0; u32::from_be_bytes(size) as usize];
    stream.read_exact(&mut bytes).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authenticated_control_transport_is_bounded_and_does_not_block_attached_clients() {
    let backend = Backend::start().await;
    let _session = Session {
        backend: &backend,
        name: "transport",
    };
    let mut tui = Terminal::start_session(&backend, "transport");
    tui.wait("Ready").await;
    let endpoint: Value = serde_json::from_slice(
        &std::fs::read(
            backend
                .directory
                .path()
                .join("muxer-state/transport.endpoint"),
        )
        .unwrap(),
    )
    .unwrap();
    let connect = || {
        let stream =
            std::net::TcpStream::connect(("127.0.0.1", endpoint["port"].as_u64().unwrap() as u16))
                .unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
    };
    let request = json!({"method":"snapshot"});
    let hello = |token: &str, operation: &str, control: Value| json!({"Hello":{"protocol":1,"token":token,"operation":operation,"width":80,"height":24,"control":control}});
    let mut partial = Vec::new();
    for _ in 0..5 {
        let mut stream = connect();
        stream.write_all(&[0, 0, 1, 0, b'{']).unwrap();
        partial.push(stream);
    }
    let mut invalid = connect();
    wire_write(
        &mut invalid,
        &hello(
            "wrong",
            "control",
            json!({"method":"close","params":{"target":"pane","id":1}}),
        ),
    );
    assert!(
        wire_read(&mut invalid)["Error"]
            .as_str()
            .unwrap()
            .contains("authentication")
    );
    let mut mismatch = connect();
    wire_write(
        &mut mismatch,
        &hello(endpoint["token"].as_str().unwrap(), "stop", request.clone()),
    );
    assert!(
        wire_read(&mut mismatch)["Error"]
            .as_str()
            .unwrap()
            .contains("Unknown")
    );
    let mut valid = connect();
    wire_write(
        &mut valid,
        &hello(endpoint["token"].as_str().unwrap(), "control", request),
    );
    assert_eq!(wire_read(&mut valid)["Ready"]["pid"], endpoint["pid"]);
    let result = wire_read(&mut valid)["Control"].clone();
    assert_eq!(result["panes"].as_array().unwrap().len(), 1);
    assert_eq!(result["clients"].as_array().unwrap().len(), 1);
    assert!(result["panes"][0]["thread"].is_string());
    tui.command("/rename Transport stays responsive");
    tui.wait("Transport stays responsive").await;
    drop(partial);
    tui.exit().await;
}
