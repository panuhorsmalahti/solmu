use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn memories_command_lists_saved_facts_and_dates() {
    let backend = Backend::start().await;
    backend
        .client
        .post(backend.endpoint("/api/v1/memories"))
        .json(&serde_json::json!({"content":"My cat is named Miso"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    terminal.command("/memories");
    terminal.wait("SOLMU / MEMORIES").await;
    terminal.wait("My cat is named Miso").await;
    terminal.wait("Saved ").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(
            terminal.screen.lock().unwrap().screen(),
            "cli-memories",
        );
    }
    terminal.send(b"\x1b");
    terminal.ready().await;
    terminal.exit().await;
}
