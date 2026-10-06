use super::sessions::Session;
use super::*;

fn rename_workspace_after_exit(source: &std::path::Path, destination: &std::path::Path) {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        match std::fs::rename(source, destination) {
            Ok(()) => return,
            Err(error)
                if cfg!(windows)
                    && error.raw_os_error() == Some(32)
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(error) => panic!("could not move the stopped workspace: {error}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_workspace_stays_unavailable_and_closing_the_last_tab_resets_layout() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "missing",
    };
    let project = backend.directory.path().join("Original project");
    let moved = backend.directory.path().join("Moved project");
    std::fs::create_dir(&project).unwrap();
    assert!(
        session
            .command(&["server", "start", "--cwd", project.to_str().unwrap()])
            .status
            .success()
    );
    let mut first = Terminal::start_session(&backend, "missing");
    first.wait("Ready").await;
    assert!(session.command(&["server", "stop"]).status.success());
    first.wait_exit().await;
    rename_workspace_after_exit(&project, &moved);
    let mut restored = Terminal::start_session(&backend, "missing");
    restored.wait("Solmu 1 · workspace unavailable").await;
    restored.wait("This pane is stopped").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    restored.prefix('w');
    restored.wait("Workspace path:").await;
    restored.command(backend.directory.path().to_str().unwrap());
    restored.wait("Solmu 2 · idle").await;
    restored.wait("Ready").await;
    restored.send(b"\x02\x1b[A");
    restored.wait("Solmu 1 · workspace unavailable").await;
    restored.prefix('X');
    restored.wait("Solmu 2 · idle").await;
    restored.prefix('X');
    restored.wait_exit().await;
    let mut fresh = Terminal::start_session(&backend, "missing");
    fresh.wait("Solmu 1 · idle").await;
    fresh.wait("Ready").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        3
    );
    fresh.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restarting_restores_spaces_nested_layouts_focus_models_and_conversations() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "restore",
    };
    let other = backend.directory.path().join("Second project");
    std::fs::create_dir(&other).unwrap();
    let mut tui = Terminal::start_session(&backend, "restore");
    tui.wait("Ready").await;
    tui.command("/rename Saved left");
    tui.wait("SOLMU    Saved left").await;
    tui.wait("Ready").await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    tui.command("Survive a server restart");
    tui.wait("Hello from Solmu").await;
    tui.wait("Ready").await;
    tui.prefix('v');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready").await;
    tui.command("/rename Model comparison");
    tui.wait("SOLMU    Model comparison").await;
    tui.wait("Ready").await;
    tui.command("/model gpt-6-luna");
    tui.wait("Model: gpt-6-luna").await;
    tui.wait("Ready").await;
    tui.prefix('-');
    tui.wait("Solmu 3 · idle").await;
    tui.wait("Ready").await;
    tui.command("/exit");
    tui.wait("Solmu 3 · exited 0").await;
    tui.prefix('h');
    tui.wait("› Solmu 1 · idle").await;
    tui.prefix('z');
    tui.wait("zoomed").await;
    tui.prefix('n');
    tui.wait("Solmu 4 · idle").await;
    tui.wait("Ready").await;
    tui.command("/rename Saved focused tab");
    tui.wait("SOLMU    Saved focused tab").await;
    tui.wait("Ready").await;
    tui.prefix('w');
    tui.wait("Workspace path:").await;
    tui.command(other.to_str().unwrap());
    tui.wait("Solmu 5 · idle").await;
    tui.wait("Ready").await;
    tui.send(b"\x02\x1b[A");
    tui.wait("SOLMU    Saved focused tab").await;
    assert!(session.command(&["server", "stop"]).status.success());
    tui.wait_exit().await;
    let saved: serde_json::Value = serde_json::from_slice(
        &std::fs::read(backend.directory.path().join("muxer-state/restore.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(saved["active"], 4);
    assert_eq!(saved["spaces"].as_array().unwrap().len(), 2);
    assert_eq!(saved["panes"].as_array().unwrap().len(), 5);
    assert!(
        saved["panes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|pane| pane["thread"].is_string())
    );
    assert_eq!(
        saved["spaces"][0]["tabs"][0]["layout"]["Split"]["axis"],
        "Right"
    );
    assert_eq!(
        saved["spaces"][0]["tabs"][0]["layout"]["Split"]["second"]["Split"]["axis"],
        "Down"
    );
    let mut restored = Terminal::start_session(&backend, "restore");
    restored.wait("SOLMU    Saved focused tab").await;
    restored.wait("Ready").await;
    restored.prefix('1');
    restored.wait("zoomed").await;
    restored.wait("SOLMU    Saved left").await;
    restored.wait("Survive a server restart").await;
    restored.wait("Hello from Solmu").await;
    restored.prefix('z');
    restored.wait("Model: gpt-6-luna").await;
    restored.wait("Solmu 3 · exited 0").await;
    restored.resize(40, 100);
    restored.wait_resized(100).await;
    restored.wait("new conversation.").await;
    restored.resize(40, 180);
    restored.wait_resized(180).await;
    restored
        .wait("Use Restart to start a new conversation.")
        .await;
    for (key, title, name) in [
        ('W', "Rename space", "Solmu workspace"),
        ('T', "Rename tab", "Agents"),
        ('P', "Rename pane", "Planning"),
    ] {
        restored.prefix(key);
        restored.wait(title).await;
        restored.command(name);
        restored.wait_absent(title).await;
    }
    restored.wait("Planning #1 · idle").await;
    solmu_e2e::support::capture_terminal(restored.screen.lock().unwrap().screen(), "muxer");
    restored.send(b"\x02\x1b[B");
    restored.wait("Solmu 5 · idle").await;
    restored.wait("Ready").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        5
    );
    assert_eq!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    restored.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn corrupt_snapshots_are_preserved_and_healthy_restores_do_not_make_backups() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "recovery",
    };
    let directory = backend.directory.path().join("muxer-state");
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("recovery.json");
    let original = b"{broken session";
    std::fs::write(&path, original).unwrap();
    let mut tui = Terminal::start_session(&backend, "recovery");
    tui.wait("Ready").await;
    assert!(session.command(&["server", "stop"]).status.success());
    tui.wait_exit().await;
    let backups = directory.join("backups");
    let files: Vec<_> = std::fs::read_dir(&backups)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(std::fs::read(&files[0]).unwrap(), original);
    let healthy = std::fs::read(&path).unwrap();
    let mut restored = Terminal::start_session(&backend, "recovery");
    restored.wait("Ready").await;
    assert!(session.command(&["server", "stop"]).status.success());
    restored.wait_exit().await;
    assert_eq!(std::fs::read_dir(&backups).unwrap().count(), 1);
    for kind in 0..3 {
        let mut corrupted: serde_json::Value = serde_json::from_slice(&healthy).unwrap();
        match kind {
            0 => corrupted["version"] = 99.into(),
            1 => corrupted["active"] = 999.into(),
            _ => {
                let id = corrupted["panes"][0]["id"].clone();
                corrupted["spaces"][0]["tabs"][0]["layout"] = serde_json::json!({"Split": {"axis": "Right", "ratio": 500, "first": {"Pane": id}, "second": {"Pane": id}}});
            }
        }
        let bytes = serde_json::to_vec(&corrupted).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let mut tui = Terminal::start_session(&backend, "recovery");
        tui.wait("Ready").await;
        assert!(session.command(&["server", "stop"]).status.success());
        tui.wait_exit().await;
        assert!(
            std::fs::read_dir(&backups)
                .unwrap()
                .any(|entry| std::fs::read(entry.unwrap().path()).unwrap() == bytes)
        );
    }
    assert_eq!(std::fs::read_dir(&backups).unwrap().count(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn failed_recovery_backup_leaves_the_original_snapshot_untouched() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "protected",
    };
    let directory = backend.directory.path().join("muxer-state");
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("protected.json");
    let original = b"original broken session";
    std::fs::write(&path, original).unwrap();
    let obstruction = directory.join("backups");
    std::fs::write(&obstruction, b"not a directory").unwrap();
    let mut tui = Terminal::start_session(&backend, "protected");
    tui.wait("Ready").await;
    assert!(session.command(&["server", "stop"]).status.success());
    tui.wait_exit().await;
    assert_eq!(std::fs::read(&path).unwrap(), original);
    std::fs::remove_file(obstruction).unwrap();
    let mut recovered = Terminal::start_session(&backend, "protected");
    recovered.wait("Ready").await;
    assert!(session.command(&["server", "stop"]).status.success());
    recovered.wait_exit().await;
    assert!(serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).is_ok());
    assert!(
        std::fs::read_dir(directory.join("backups"))
            .unwrap()
            .any(|entry| std::fs::read(entry.unwrap().path()).unwrap() == original)
    );
}
