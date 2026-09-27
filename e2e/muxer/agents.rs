use super::{monitors::Pending, sessions::Session, *};
use serde_json::{Value, json};
use sqlx::{Connection, SqliteConnection};

async fn lock_database(backend: &Backend) -> SqliteConnection {
    let url = format!(
        "sqlite://{}",
        backend.directory.path().join("solmu.db").display()
    );
    let mut connection = SqliteConnection::connect(&url).await.unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut connection)
        .await
        .unwrap();
    connection
}

async fn busy(session: &Session<'_>) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while command(session, &["agent", "get", "1"])["native"]["ready"] != false {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

fn command(session: &Session<'_>, args: &[&str]) -> Value {
    let output = session.command(args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["result"].clone()
}
fn rejected(session: &Session<'_>, args: &[&str]) -> Value {
    let output = session.command(args);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    serde_json::from_slice(&output.stderr).unwrap()
}
async fn ready(session: &Session<'_>, id: &str) -> Value {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let info = command(session, &["agent", "get", id]);
            if info["native"]["ready"] == true {
                break info;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap()
}
async fn queued(session: &Session<'_>, count: u64) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while command(session, &["agent", "get", "1"])["native"]["queued"].as_u64() != Some(count) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}
async fn active(session: &Session<'_>, present: bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while (command(session, &["api", "snapshot"])["active_agents"]
            .as_u64()
            .unwrap()
            > 0)
            != present
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_prompts_queue_in_order_and_wait_for_exact_turns_without_sending_terminal_drafts() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "prompts",
    };
    let mut tui = Terminal::start_session(&backend, "prompts");
    tui.wait("Ready").await;
    let info = ready(&session, "1").await;
    command(&session, &["agent", "rename", "1", "Writer"]);
    tui.send(b"My unsent terminal draft");
    tui.wait("My unsent terminal draft").await;
    let text = "/rename This is message text\nSecond line, not a command";
    let first = command(&session, &["agent", "prompt", "Writer", text]);
    assert_eq!(first["accepted"], true);
    assert_eq!(first["thread"], info["thread"]);
    assert_eq!(first["instance"], info["instance"]);
    command(
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
    let second = Pending::new(
        &session,
        &[
            "agent",
            "prompt",
            "Writer",
            "Second queued task",
            "--wait",
            "--timeout",
            "10000",
        ],
    );
    queued(&session, 1).await;
    let third = command(
        &session,
        &["agent", "prompt", "Writer", "Third queued task"],
    );
    queued(&session, 2).await;
    tui.wait("2 queued").await;
    tui.wait("My unsent terminal draft").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(
            tui.screen.lock().unwrap().screen(),
            "muxer-automation",
        );
    }
    let first_id = first["turn"]["id"].as_str().unwrap();
    let result = command(
        &session,
        &[
            "agent",
            "wait",
            "Writer",
            "--turn",
            first_id,
            "--timeout",
            "10000",
        ],
    );
    assert_eq!(result["turn"]["id"], first_id);
    assert_eq!(result["turn"]["state"], "succeeded");
    assert_eq!(result["turn"]["text"], "Hello from Solmu");
    let second = second.finish(true).await["result"].clone();
    let last = command(&session, &["agent", "wait", "Writer", "--timeout", "10000"]);
    assert_eq!(last["turn"]["id"], third["turn"]["id"]);
    assert_eq!(last["turn"]["state"], "succeeded");
    let cached = command(&session, &["agent", "turn", "Writer", first_id]);
    assert_eq!(cached["turn"], result["turn"]);
    let request =
        json!({"method":"agent_turn","params":{"target":"Writer","turn":first_id}}).to_string();
    assert_eq!(
        command(&session, &["api", "request", &request])["turn"],
        result["turn"]
    );
    assert_ne!(second["turn"]["id"], first["turn"]["id"]);
    let messages = backend.messages(info["thread"].as_str().unwrap()).await;
    let messages = messages["items"].as_array().unwrap();
    assert_eq!(messages.len(), 6);
    let prompts = messages
        .iter()
        .filter(|m| m["role"] == "user")
        .map(|m| m["content"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        prompts,
        vec![text, "Second queued task", "Third queued task"]
    );
    for turn in [&result["turn"], &second["turn"], &last["turn"]] {
        assert!(messages.iter().any(|m| m["id"] == turn["user_message"]));
        assert!(
            messages
                .iter()
                .any(|m| m["id"] == turn["assistant_message"])
        );
    }
    tui.wait("My unsent terminal draft").await;
    tui.wait_absent("1 queued").await;
    tui.wait_absent("2 queued").await;
    tui.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prompts_preserve_open_profile_edits_and_ambiguous_names_never_redirect_work() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "profile",
    };
    let mut tui = Terminal::start_session(&backend, "profile");
    tui.wait("Ready").await;
    ready(&session, "1").await;
    tui.command("/profile");
    tui.wait("You are Solmu, an autonomous agent.").await;
    tui.send(b"\x15My unsaved system prompt");
    tui.wait("My unsaved system prompt").await;
    let reply = Pending::new(
        &session,
        &[
            "agent",
            "prompt",
            "1",
            "Reply while Profile stays open",
            "--wait",
            "--timeout",
            "10000",
        ],
    );
    assert_eq!(
        reply.finish(true).await["result"]["turn"]["state"],
        "succeeded"
    );
    tui.wait("My unsaved system prompt").await;
    let stored: Value = backend
        .client
        .get(backend.endpoint("/api/v1/profile"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(
        stored["system_prompt"]
            .as_str()
            .unwrap()
            .starts_with("You are Solmu,")
    );
    tui.send(b"\x1b");
    tui.wait("Reply while Profile stays open").await;
    command(&session, &["pane", "split", "1"]);
    ready(&session, "2").await;
    command(&session, &["agent", "rename", "1", "Planner"]);
    rejected(&session, &["agent", "rename", "2", "Planner"]);
    command(&session, &["pane", "rename", "2", "Planner"]);
    let error = rejected(
        &session,
        &[
            "agent",
            "prompt",
            "Planner",
            "Do not choose an arbitrary pane",
        ],
    );
    assert!(error["error"].as_str().unwrap().contains("ambiguous"));
    command(&session, &["agent", "rename", "2", "Reviewer"]);
    assert_eq!(
        command(&session, &["agent", "list"])["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(command(&session, &["agent", "get", "Reviewer"])["id"], 2);
    let client = command(&session, &["api", "snapshot"])["clients"][0]["id"]
        .as_u64()
        .unwrap()
        .to_string();
    command(
        &session,
        &["agent", "focus", "Reviewer", "--client", &client],
    );
    tui.wait("\u{203a} Reviewer #2").await;
    command(&session, &["agent", "send-keys", "Reviewer", "slash"]);
    tui.wait("Commands").await;
    command(&session, &["agent", "send-keys", "Reviewer", "esc"]);
    let read = session.command(&["agent", "read", "Reviewer"]);
    assert!(read.status.success());
    assert!(String::from_utf8_lossy(&read.stdout).contains("SOLMU"));
    command(&session, &["agent", "rename", "Reviewer", "--clear"]);
    rejected(&session, &["agent", "get", "Reviewer"]);
    tui.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stopping_cancels_active_and_queued_turns_without_saving_unsubmitted_prompts() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "stop",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let info = ready(&session, "1").await;
    let first = command(
        &session,
        &["agent", "prompt", "1", "Stop this active reply"],
    );
    command(
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
    let second = command(
        &session,
        &["agent", "prompt", "1", "Never submit this queued prompt"],
    );
    queued(&session, 1).await;
    assert_eq!(command(&session, &["agent", "stop", "1"])["stopping"], true);
    for accepted in [&first, &second] {
        let id = accepted["turn"]["id"].as_str().unwrap();
        let error = rejected(
            &session,
            &["agent", "wait", "1", "--turn", id, "--timeout", "10000"],
        );
        assert_eq!(error["accepted"], true);
        assert_eq!(error["code"], "stopped");
        assert_eq!(error["turn"], id);
        assert_eq!(
            command(&session, &["agent", "turn", "1", id])["turn"]["state"],
            "stopped"
        );
    }
    let messages = backend.messages(info["thread"].as_str().unwrap()).await;
    assert_eq!(messages["items"].as_array().unwrap().len(), 1);
    assert_eq!(messages["items"][0]["content"], "Stop this active reply");
    assert!(
        command(
            &session,
            &["agent", "turn", "1", second["turn"]["id"].as_str().unwrap()]
        )["turn"]["user_message"]
            .is_null()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timed_out_and_disconnected_waits_never_duplicate_accepted_prompts() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "timeout",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let info = ready(&session, "1").await;
    let error = Pending::new(
        &session,
        &[
            "agent",
            "prompt",
            "1",
            "Only submit this once",
            "--wait",
            "--timeout",
            "150",
        ],
    )
    .finish(false)
    .await;
    assert_eq!(error["code"], "timeout");
    assert_eq!(error["accepted"], true);
    let id = error["turn"].as_str().unwrap();
    let waiter = Pending::new(&session, &["agent", "wait", "1", "--turn", id]);
    active(&session, true).await;
    waiter.cancel();
    active(&session, false).await;
    let result = command(
        &session,
        &["agent", "wait", "1", "--turn", id, "--timeout", "10000"],
    );
    assert_eq!(result["turn"]["state"], "succeeded");
    let messages = backend.messages(info["thread"].as_str().unwrap()).await;
    assert_eq!(messages["items"].as_array().unwrap().len(), 2);
    assert_eq!(messages["items"][0]["content"], "Only submit this once");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn thread_changes_fail_queued_prompts_instead_of_sending_them_to_another_conversation() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "thread-change",
    };
    let mut tui = Terminal::start_session(&backend, session.name);
    tui.wait("Ready").await;
    let old = ready(&session, "1").await;
    let first = Pending::new(
        &session,
        &[
            "agent",
            "prompt",
            "1",
            "First conversation",
            "--wait",
            "--timeout",
            "10000",
        ],
    );
    assert_eq!(
        first.finish(true).await["result"]["turn"]["state"],
        "succeeded"
    );
    ready(&session, "1").await;
    let mut lock = lock_database(&backend).await;
    tui.command("/new");
    busy(&session).await;
    let pending = Pending::new(
        &session,
        &[
            "agent",
            "prompt",
            "1",
            "Belongs only to the old thread",
            "--wait",
            "--timeout",
            "10000",
        ],
    );
    queued(&session, 1).await;
    sqlx::query("COMMIT").execute(&mut lock).await.unwrap();
    let error = pending.finish(false).await;
    assert_eq!(error["accepted"], true);
    assert_eq!(error["code"], "thread_changed");
    let new = ready(&session, "1").await;
    assert_ne!(old["thread"], new["thread"]);
    assert_eq!(
        backend.messages(old["thread"].as_str().unwrap()).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        backend.messages(new["thread"].as_str().unwrap()).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    tui.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn queue_limit_rejects_additional_work_and_stop_clears_unsent_prompts() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "queue-limit",
    };
    let mut tui = Terminal::start_session(&backend, session.name);
    tui.wait("Ready").await;
    let info = ready(&session, "1").await;
    let mut lock = lock_database(&backend).await;
    tui.command("/rename Hold the queue");
    busy(&session).await;
    let mut turns = Vec::new();
    for index in 0..16 {
        turns.push(command(
            &session,
            &["agent", "prompt", "1", &format!("Queued task {index}")],
        ));
    }
    queued(&session, 16).await;
    let rejected = rejected(&session, &["agent", "prompt", "1", "Queue overflow"]);
    assert_eq!(rejected["code"], "queue_full");
    assert_eq!(rejected["accepted"], false);
    command(&session, &["agent", "stop", "1"]);
    queued(&session, 0).await;
    sqlx::query("COMMIT").execute(&mut lock).await.unwrap();
    ready(&session, "1").await;
    for turn in turns {
        let result = command(
            &session,
            &["agent", "turn", "1", turn["turn"]["id"].as_str().unwrap()],
        );
        assert_eq!(result["turn"]["state"], "stopped");
        assert!(result["turn"]["user_message"].is_null());
    }
    assert!(
        backend.messages(info["thread"].as_str().unwrap()).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    tui.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_replies_are_reported_as_failed_turns_and_never_as_idle_success() {
    let backend = Backend::configured(None, false, false).await;
    let session = Session {
        backend: &backend,
        name: "failure",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let info = ready(&session, "1").await;
    let error = Pending::new(
        &session,
        &[
            "agent",
            "prompt",
            "1",
            "A reply needs credentials",
            "--wait",
            "--timeout",
            "10000",
        ],
    )
    .finish(false)
    .await;
    assert_eq!(error["code"], "failed");
    assert_eq!(error["accepted"], true);
    let id = error["turn"].as_str().unwrap();
    let turn = command(&session, &["agent", "turn", "1", id]);
    assert_eq!(turn["turn"]["state"], "failed");
    assert!(turn["turn"]["user_message"].is_string());
    assert!(turn["turn"]["assistant_message"].is_null());
    assert_eq!(
        rejected(&session, &["agent", "wait", "1", "--turn", id])["code"],
        "failed"
    );
    assert_eq!(
        backend.messages(info["thread"].as_str().unwrap()).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_control_rejects_bad_authentication_and_bounded_frames_without_blocking_the_pane() {
    use std::{
        io::{Read, Write},
        net::TcpStream,
    };
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "authentication",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let info = ready(&session, "1").await;
    let port = info["native"]["port"].as_u64().unwrap();
    let address = format!("127.0.0.1:{port}");
    let mut stream = TcpStream::connect(&address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let bytes = serde_json::to_vec(&json!({"token":"wrong", "instance":info["instance"], "thread":info["thread"], "action":{"type":"prompt", "text":"Unauthenticated prompt"}})).unwrap();
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&bytes).unwrap();
    let mut header = [0; 4];
    stream.read_exact(&mut header).unwrap();
    let mut body = vec![0; u32::from_be_bytes(header) as usize];
    stream.read_exact(&mut body).unwrap();
    let response: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(response["type"], "error");
    assert_eq!(response["failure"]["code"], "authentication");
    assert_eq!(response["failure"]["accepted"], false);
    let mut oversized = TcpStream::connect(&address).unwrap();
    oversized
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    oversized
        .write_all(&(1024u32 * 1024 + 1).to_be_bytes())
        .unwrap();
    assert!(matches!(oversized.read(&mut header), Ok(0) | Err(_)));
    let mut partial = TcpStream::connect(&address).unwrap();
    partial.write_all(&[0, 0]).unwrap();
    assert_eq!(
        command(&session, &["agent", "wait", "1", "--timeout", "1000"])["idle"],
        true
    );
    for args in [
        vec!["agent", "prompt", "1", " "],
        vec!["agent", "wait", "1", "--turn", "invalid"],
        vec!["agent", "prompt", "1", "No submission", "--timeout", "0"],
    ] {
        let output = session.command(&args);
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["ok"], false);
    }
    assert!(
        backend.messages(info["thread"].as_str().unwrap()).await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replacing_a_process_never_completes_the_old_turn_with_a_new_reply() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "process-change",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let old = ready(&session, "1").await;
    command(&session, &["pane", "split", "1"]);
    ready(&session, "2").await;
    command(&session, &["agent", "rename", "1", "Writer"]);
    let pending = Pending::new(
        &session,
        &[
            "agent",
            "prompt",
            "Writer",
            "Old process turn",
            "--wait",
            "--timeout",
            "10000",
        ],
    );
    command(
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
    command(&session, &["pane", "send-text", "1", "/exit"]);
    command(&session, &["agent", "send-keys", "Writer", "enter"]);
    let error = pending.finish(false).await;
    assert_eq!(error["code"], "connection_lost");
    assert_eq!(error["accepted"], true);
    assert_eq!(error["instance"], old["instance"]);
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
    command(&session, &["pane", "restart", "1"]);
    let new = ready(&session, "Writer").await;
    assert_ne!(new["instance"], old["instance"]);
    assert_ne!(new["thread"], old["thread"]);
    let expired = rejected(
        &session,
        &["agent", "turn", "Writer", error["turn"].as_str().unwrap()],
    );
    assert_eq!(expired["code"], "turn_unknown");
    let result = Pending::new(
        &session,
        &[
            "agent",
            "prompt",
            "Writer",
            "New process turn",
            "--wait",
            "--timeout",
            "10000",
        ],
    )
    .finish(true)
    .await;
    assert_ne!(result["result"]["turn"]["id"], error["turn"]);
    assert_eq!(result["result"]["instance"], new["instance"]);
}
