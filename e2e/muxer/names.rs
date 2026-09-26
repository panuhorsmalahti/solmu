use super::sessions::Session;
use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn names_survive_restart_and_remain_separate_from_conversation_titles() {
    let backend = Backend::start().await;
    let session = Session {
        backend: &backend,
        name: "names",
    };
    let mut tui = Terminal::start_session(&backend, "names");
    tui.wait("Ready").await;
    for (key, title, name) in [
        ('W', "Rename space", "Project work"),
        ('T', "Rename tab", "Agents"),
        ('P', "Rename pane", "Planner"),
    ] {
        tui.prefix(key);
        tui.wait(title).await;
        tui.command(name);
        tui.wait_absent(title).await;
        tui.wait(name).await;
    }
    tui.command("/rename Backend title");
    tui.wait("SOLMU    Backend title").await;
    tui.wait("Planner #1 · idle").await;
    assert!(session.command(&["server", "stop"]).status.success());
    tui.wait_exit().await;
    let mut restored = Terminal::start_session(&backend, "names");
    restored.wait("Project work").await;
    restored.wait("Agents").await;
    restored.wait("Planner #1 · idle").await;
    restored.wait("SOLMU    Backend title").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    restored.command("/exit");
    restored.wait("Planner #1 · exited 0").await;
    restored.click("Restart").await;
    restored.wait("Planner #1 · idle").await;
    restored.wait("Ready").await;
    restored.prefix('P');
    restored.wait("Rename pane").await;
    restored.send(b"\x15\r");
    restored.wait_absent("Rename pane").await;
    restored.wait("Solmu 1 · idle").await;
    restored.prefix('P');
    restored.wait("Rename pane").await;
    restored.command(&"x".repeat(81));
    restored.wait("Use at most 80 characters").await;
    restored.send(b"\x1b");
    restored.wait_absent("Rename pane").await;
    restored.wait("Solmu 1 · idle").await;
    restored.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rename_fields_edit_graphemes_words_paste_and_preserve_other_client_drafts() {
    let backend = Backend::start().await;
    let _session = Session {
        backend: &backend,
        name: "editing",
    };
    let mut first = Terminal::start_session(&backend, "editing");
    first.wait("Ready").await;
    first.prefix('P');
    first.wait("Rename pane").await;
    first.send("one e\u{301}X".as_bytes());
    first.send(b"\x1b[D\x7f\x1b[F\x7f\x17\x19");
    first.send("\x1b[200~🌱\r\n\x1b[201~".as_bytes());
    first.send(b"\r");
    first.wait_absent("Rename pane").await;
    first.wait("one 🌱 #1 · idle").await;
    let mut second = Terminal::start_session(&backend, "editing");
    second.wait("one 🌱 #1 · idle").await;
    first.prefix('P');
    first.wait("Rename pane").await;
    first.send(b"\x15Local draft");
    first.wait("Local draft").await;
    second.prefix('P');
    second.wait("Rename pane").await;
    second.send(b"\x15Shared name\r");
    second.wait_absent("Rename pane").await;
    second.wait("Shared name #1 · idle").await;
    first.wait("Shared name #1 · idle").await;
    first.wait("Local draft").await;
    first.send(b"\x1b");
    first.wait_absent("Rename pane").await;
    first.wait_absent("Local draft").await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    first.exit().await;
    second.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mouse_menus_rename_and_close_spaces_tabs_and_panes() {
    let backend = Backend::start().await;
    let mut tui = Terminal::start(&backend, &[]);
    tui.wait("Ready").await;
    tui.send(b"\x1b[<2;8;4M\x1b[<2;8;4m");
    tui.click("Rename space").await;
    tui.wait("Rename space").await;
    tui.send(b"Mouse project");
    tui.click("Save").await;
    tui.wait_absent("Rename space").await;
    tui.wait("Mouse project").await;
    tui.send(b"\x1b[<2;35;3M\x1b[<2;35;3m");
    tui.click("Rename tab").await;
    tui.wait("Rename tab").await;
    tui.command("Mouse tab");
    tui.wait_absent("Rename tab").await;
    tui.wait("Mouse tab").await;
    tui.send(b"\x1b[<2;50;8M\x1b[<2;50;8m");
    tui.click("Rename pane").await;
    tui.wait("Rename pane").await;
    tui.command("Mouse pane");
    tui.wait_absent("Rename pane").await;
    tui.wait("Mouse pane #1 · idle").await;
    tui.prefix('w');
    tui.wait("Workspace path:").await;
    tui.command(backend.directory.path().to_str().unwrap());
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready").await;
    tui.prefix('n');
    tui.wait("Solmu 3 · idle").await;
    tui.wait("Ready").await;
    tui.send(b"\x1b[<2;8;7M\x1b[<2;8;7m");
    tui.click("Close space").await;
    tui.wait_absent("Solmu 3 · idle").await;
    tui.wait("Mouse pane #1 · idle").await;
    tui.send(b"\x1b[<2;35;3M\x1b[<2;35;3m");
    tui.click("Close tab").await;
    tui.wait_exit().await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        3
    );
}
