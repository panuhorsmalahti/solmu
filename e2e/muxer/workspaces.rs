use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn workspace_selection_validation_error_state_and_stopping_replies() {
    let backend = Backend::start().await;
    let other = backend.directory.path().join("project with spaces");
    std::fs::create_dir(&other).unwrap();
    let mut tui = Terminal::start(&backend, &[backend.directory.path(), &other]);
    tui.wait("Solmu 1 · idle").await;
    tui.prefix(']');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready ·").await;
    tui.prefix('w');
    tui.wait("Workspace path:").await;
    tui.command("solmu-missing-workspace");
    tui.wait("Workspace path: solmu-missing-workspace").await;
    assert!(tui.contents().contains("Workspace path:"));
    tui.send(b"\x1b");
    // Unix terminals encode Alt using an Escape prefix. Confirm cancellation
    // before sending Ctrl+b, so separate user actions cannot become Alt+Ctrl+b.
    tui.wait_absent("New workspace").await;
    tui.prefix('w');
    tui.command(other.to_str().unwrap());
    tui.wait("Solmu 3 · idle").await;
    tui.wait("Ready ·").await;
    tui.command("FAIL_STREAM");
    tui.wait("Solmu 3 · error").await;
    tui.command("/new Stop conversation");
    tui.wait("SOLMU    Stop conversation").await;
    tui.wait("Ready ·").await;
    tui.command("Hello to stop");
    tui.wait("SOLMU · streaming").await;
    tui.command("/stop");
    tui.wait("Ready ·").await;
    let threads = backend.threads().await;
    let id = threads["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["title"] == "Stop conversation")
        .unwrap()["id"]
        .as_str()
        .unwrap();
    assert_eq!(
        backend.messages(id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    tui.prefix('[');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready ·").await;
    tui.command("/rename Quit cleanup");
    tui.wait("SOLMU    Quit cleanup").await;
    tui.wait("Ready ·").await;
    let threads = backend.threads().await;
    let cleanup_id = threads["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["title"] == "Quit cleanup")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    tui.command("Reply interrupted by muxer exit");
    tui.wait("SOLMU · streaming").await;
    tui.exit().await;
    // Owned CLIs must have exited: no further new conversations appear.
    let count = backend.threads().await["items"].as_array().unwrap().len();
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        count
    );
    assert_eq!(
        backend.messages(&cleanup_id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "Muxer exit must terminate its CLI and abandon the pending assistant reply"
    );
}
