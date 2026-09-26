use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn picker_finds_names_paths_states_and_focuses_real_panes_with_keys_or_mouse() {
    let backend = Backend::start().await;
    let project = backend.directory.path().join("Navigation project");
    std::fs::create_dir(&project).unwrap();
    let mut tui = Terminal::start(&backend, &[backend.directory.path(), &project]);
    tui.wait("Ready").await;
    tui.click("Navigation project").await;
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready").await;
    tui.prefix('P');
    tui.wait("Rename pane").await;
    tui.command("Research");
    tui.wait_absent("Rename pane").await;
    tui.prefix('v');
    tui.wait("Solmu 3 · idle").await;
    tui.wait("Ready").await;
    tui.click("Find").await;
    tui.wait("Find spaces, tabs and panes").await;
    tui.send(b"research idle");
    tui.wait("Pane · Research #2 · idle").await;
    tui.send(b"\r");
    tui.wait_absent("Find spaces, tabs and panes").await;
    tui.wait("› Research #2 · idle").await;
    tui.command("/rename Focused research");
    tui.wait("SOLMU    Focused research").await;
    tui.wait("Ready").await;
    tui.prefix('g');
    tui.wait("Find spaces, tabs and panes").await;
    tui.send(b"nothing-will-match");
    tui.wait("No matches").await;
    tui.send(b"\x15Navigation project");
    tui.wait("> Navigation project").await;
    tui.wait("Space · Navigation project").await;
    tui.click("Pane · Solmu 3 · idle").await;
    tui.wait_absent("Find spaces, tabs and panes").await;
    tui.wait("› Solmu 3 · idle").await;
    tui.command("/rename Mouse target");
    tui.wait("SOLMU    Mouse target").await;
    tui.wait("Ready").await;
    tui.prefix('g');
    tui.wait("Find spaces, tabs and panes").await;
    tui.send(b"Solmu 1 idle\r");
    tui.wait_absent("Find spaces, tabs and panes").await;
    tui.wait("Solmu 1 · idle").await;
    assert!(!tui.contents().contains("Mouse target"));
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        3
    );
    solmu_e2e::support::capture_terminal(tui.screen.lock().unwrap().screen(), "muxer");
    tui.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn navigation_and_searchable_help_keep_keystrokes_out_of_conversations() {
    let backend = Backend::start().await;
    let mut tui = Terminal::start(&backend, &[]);
    tui.wait("Ready").await;
    tui.prefix('v');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready").await;
    tui.click("Navigate").await;
    tui.wait("Navigate:").await;
    tui.send(b"h");
    tui.wait("› Solmu 1 · idle").await;
    tui.send(b"l");
    tui.wait("› Solmu 2 · idle").await;
    tui.send(b"?rename");
    tui.wait("Keyboard help").await;
    tui.wait("Rename space").await;
    tui.wait("Rename tab").await;
    tui.wait("Rename pane").await;
    tui.wait_absent("Send literal").await;
    tui.send(b"\x15unmatched-filter");
    tui.wait("No matches").await;
    tui.send(b"\x1b");
    tui.wait_absent("Keyboard help").await;
    tui.wait("Navigate:").await;
    tui.send(b"q");
    tui.wait_absent("Navigate:").await;
    tui.command("A real user message");
    tui.wait("Solmu 2 · working").await;
    tui.wait("Solmu 2 · idle").await;
    let threads = backend.threads().await;
    let mut user_messages = vec![];
    for thread in threads["items"].as_array().unwrap() {
        let messages = backend.messages(thread["id"].as_str().unwrap()).await;
        user_messages.extend(
            messages["items"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|message| message["role"] == "user")
                .cloned(),
        );
    }
    assert_eq!(user_messages.len(), 1);
    assert_eq!(user_messages[0]["content"], "A real user message");
    tui.click("Help").await;
    tui.wait("Keyboard help").await;
    tui.click_at(156, 7);
    tui.wait_absent("Keyboard help").await;
    tui.exit().await;
}
