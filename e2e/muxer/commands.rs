use super::{sessions::Session, *};
use serde_json::{Value, json};

fn command(session: &Session<'_>, args: &[&str]) -> Value {
    let output = session.command(args);
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice::<Value>(&output.stdout).unwrap()["result"].clone()
}
async fn pane(session: &Session<'_>, id: &str, predicate: impl Fn(&Value) -> bool) -> Value {
    tokio::time::timeout(Duration::from_secs(15), async {
        loop {
            let value = command(session, &["pane", "get", id]);
            if predicate(&value) {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("Timed out waiting for pane {id}"))
}
fn config(backend: &Backend, text: &str) {
    let path = backend.directory.path().join("muxer-state/config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}
fn probe_config(default: &str) -> String {
    format!(
        "[terminal]\nnew_pane = {default:?}\nshell = {}\nshell_mode = \"non_login\"\n",
        serde_json::to_string(&binary("terminal-probe").to_string_lossy()).unwrap()
    )
}
async fn file(path: &std::path::Path) -> String {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(value) = std::fs::read_to_string(path)
                && !value.is_empty()
            {
                return value;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("Timed out reading {}", path.display()))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_argv_preserves_arguments_cwd_identity_and_generic_terminal_attachment() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "argv",
    };
    assert!(session.command(&["server", "start"]).status.success());
    pane(&session, "1", |v| v["native"]["ready"] == true).await;
    let cwd = backend.directory.path().join("project with spaces");
    std::fs::create_dir(&cwd).unwrap();
    let report = backend.directory.path().join("argv.jsonl");
    let argv = json!([
        binary("terminal-probe"),
        "--report",
        report,
        "two words",
        "$HOME",
        "--session",
        "literal",
        "& echo not-run"
    ])
    .to_string();
    let created = command(
        &session,
        &[
            "pane",
            "split",
            "1",
            "--argv",
            &argv,
            "--cwd",
            cwd.to_str().unwrap(),
            "--name",
            "Build",
        ],
    );
    let id = created["pane"]["id"].as_u64().unwrap().to_string();
    let record: Value = serde_json::from_str(file(&report).await.trim()).unwrap();
    assert_eq!(
        record["argv"],
        json!(
            serde_json::from_str::<Value>(&argv)
                .unwrap()
                .as_array()
                .unwrap()[1..]
        )
    );
    assert_eq!(
        record["cwd"],
        serde_json::to_value(cwd.canonicalize().unwrap()).unwrap()
    );
    assert_eq!(record["pane"], id);
    assert!(record["agent_token"].is_null());
    let info = pane(&session, &id, |v| v["state"] == "running").await;
    assert_eq!(info["launch"]["kind"], "command");
    assert_eq!(
        command(
            &session,
            &[
                "pane",
                "wait",
                &id,
                "--until",
                "running",
                "--timeout",
                "1000"
            ]
        )["matched_state"],
        "running"
    );
    assert!(info["thread"].is_null());
    assert_eq!(
        command(&session, &["agent", "list"])["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        !session
            .command(&["agent", "prompt", &id, "Must not become terminal input"])
            .status
            .success()
    );
    let mut direct = Terminal::start_direct(&backend, "argv", &id);
    direct.wait("LOCAL TERMINAL READY").await;
    direct.send(b"Attached command\r");
    direct.wait("INPUT Attached command").await;
    direct.prefix('q');
    direct.wait_exit().await;
    assert!(command(&session, &["pane", "get", &id])["exited"].is_null());
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shell_defaults_hot_reload_apply_to_new_layouts_and_explicit_solmu_overrides_them() {
    let backend = Backend::start().await;
    config(&backend, &probe_config("solmu"));
    let session = Session {
        backend: &backend,
        name: "shell-defaults",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let initial = pane(&session, "1", |v| v["native"]["ready"] == true).await;
    config(&backend, &probe_config("shell"));
    tokio::time::sleep(Duration::from_millis(700)).await;
    let space = command(&session, &["space", "create", "--name", "Terminal work"]);
    assert_eq!(space["pane"]["launch"]["kind"], "shell");
    let space_id = space["space"]["id"].as_u64().unwrap().to_string();
    let tab = command(&session, &["tab", "create", "--space", &space_id]);
    assert_eq!(tab["pane"]["launch"]["kind"], "shell");
    let explicit = command(
        &session,
        &["tab", "create", "--space", &space_id, "--solmu"],
    );
    let id = explicit["pane"]["id"].as_u64().unwrap().to_string();
    pane(&session, &id, |v| v["native"]["ready"] == true).await;
    assert_eq!(
        command(&session, &["pane", "get", "1"])["instance"],
        initial["instance"]
    );
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        2
    );
    assert!(session.command(&["server", "stop"]).status.success());
    config(
        &backend,
        "[terminal]\nnew_pane = \"solmu\"\nshell = \"missing-shell\"\n",
    );
    assert!(session.command(&["server", "start"]).status.success());
    let shell_id = space["pane"]["id"].as_u64().unwrap().to_string();
    let restored = pane(&session, &shell_id, |v| v["exited"].is_null()).await;
    assert_eq!(restored["launch"], space["pane"]["launch"]);
    assert_ne!(restored["instance"], space["pane"]["instance"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn saved_commands_wait_for_explicit_restart_and_exited_output_stays_readable() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "command-restore",
    };
    assert!(session.command(&["server", "start"]).status.success());
    let report = backend.directory.path().join("executions.jsonl");
    let argv = json!([binary("terminal-probe"), "--report", report]).to_string();
    let created = command(&session, &["pane", "split", "1", "--argv", &argv]);
    let id = created["pane"]["id"].as_u64().unwrap().to_string();
    assert_eq!(file(&report).await.lines().count(), 1);
    assert!(session.command(&["server", "stop"]).status.success());
    let saved: Value = serde_json::from_slice(
        &std::fs::read(
            backend
                .directory
                .path()
                .join("muxer-state/command-restore.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(saved["version"], 2);
    assert!(session.command(&["server", "start"]).status.success());
    let stopped = command(&session, &["pane", "get", &id]);
    assert_eq!(stopped["exited"], "command awaits explicit restart");
    assert_eq!(file(&report).await.lines().count(), 1);
    command(&session, &["pane", "restart", &id]);
    tokio::time::timeout(Duration::from_secs(10), async {
        while file(&report).await.lines().count() != 2 {
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
    })
    .await
    .unwrap();
    command(&session, &["pane", "send-text", &id, "quit"]);
    command(&session, &["pane", "send-keys", &id, "Enter"]);
    pane(&session, &id, |v| !v["exited"].is_null()).await;
    assert!(
        command(&session, &["pane", "read", &id, "--json"])["text"]
            .as_str()
            .unwrap()
            .contains("LOCAL TERMINAL READY")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_launches_leave_topology_focus_and_processes_unchanged() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "invalid-launch",
    };
    assert!(session.command(&["server", "start"]).status.success());
    pane(&session, "1", |v| v["native"]["ready"] == true).await;
    let before = command(&session, &["api", "snapshot"]);
    for options in [
        vec!["--argv", "[]"],
        vec!["--argv", "[3]"],
        vec!["--argv", "[\"missing-solmu-program\"]"],
        vec!["--shell", "--solmu"],
        vec!["--command", ""],
        vec!["--argv", "[\"bad\\u0000program\"]"],
    ] {
        let mut args = vec!["pane", "split", "1", "--focus"];
        args.extend(options);
        assert!(!session.command(&args).status.success());
    }
    let after = command(&session, &["api", "snapshot"]);
    assert_eq!(after["active_pane"], before["active_pane"]);
    assert_eq!(after["panes"].as_array().unwrap().len(), 1);
    assert_eq!(
        after["panes"][0]["instance"],
        before["panes"][0]["instance"]
    );
    assert_eq!(after["tabs"], before["tabs"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tui_shell_and_command_controls_run_in_the_workspace_and_preserve_solmu_drafts() {
    let backend = Backend::start().await;
    let shell = if cfg!(windows) { "cmd.exe" } else { "/bin/sh" };
    config(
        &backend,
        &format!("[terminal]\nshell = {shell:?}\nshell_mode = \"non_login\"\n"),
    );
    let mut tui = Terminal::start(&backend, &[]);
    tui.wait("Ready").await;
    tui.send(b"Unsent Solmu draft");
    tui.wait("Unsent Solmu draft").await;
    tui.prefix('t');
    tui.wait("Shell 2").await;
    if cfg!(windows) {
        tui.wait("Microsoft Windows").await;
    } else {
        tui.wait("$ ").await;
    }
    tui.send(b"echo Project shell is ready\r");
    tui.send(b"echo shell-finished> shell-output.txt\r");
    assert!(
        file(&backend.directory.path().join("shell-output.txt"))
            .await
            .contains("shell-finished")
    );
    tui.prefix('k');
    tui.wait("Unsent Solmu draft").await;
    tui.prefix('j');
    tui.prefix('!');
    tui.wait("Run command").await;
    tui.send(b"\r");
    tui.wait("Enter a command").await;
    tui.send(
        b"\x1b[200~echo Command finished && echo command-finished> command-output.txt\x1b[201~",
    );
    // Wait for the paste to reach the dialog before clicking its Run button.
    tui.wait("command-output.txt").await;
    tui.click_at(26, 17);
    assert!(
        file(&backend.directory.path().join("command-output.txt"))
            .await
            .contains("command-finished")
    );
    tui.wait("exited 0").await;
    tui.wait("Command finished").await;
    tui.wait("Unsent Solmu draft").await;
    solmu_e2e::support::capture_terminal(tui.screen.lock().unwrap().screen(), "muxer-commands");
    assert!(
        backend
            .messages(backend.threads().await["items"][0]["id"].as_str().unwrap())
            .await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    tui.exit().await;
}
