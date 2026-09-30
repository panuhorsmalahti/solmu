use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_can_compact_and_continue_a_conversation() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    for prompt in [
        "First turn before compaction",
        "Second turn before compaction",
    ] {
        terminal.command(prompt);
        terminal.wait("Hello from Solmu").await;
        terminal.ready().await;
    }
    terminal.command("/compact");
    terminal.wait("Conversation summary (compacted)").await;
    terminal.ready().await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(terminal.screen.lock().unwrap().screen(), "cli");
    }
    assert_eq!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    terminal.command("Continue after compaction");
    terminal.wait("Hello from Solmu").await;
    terminal.ready().await;
    assert!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message["content"] == "Continue after compaction")
    );
    terminal.exit().await;
}
