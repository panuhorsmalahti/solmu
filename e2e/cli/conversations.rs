use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_streaming_thread_commands_history_and_exit() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    let threads = backend.threads().await;
    assert_eq!(threads["items"].as_array().unwrap().len(), 1);
    let id = threads["items"][0]["id"].as_str().unwrap().to_owned();
    terminal.command("Hello from the terminal");
    terminal.wait("SOLMU · streaming").await;
    terminal.wait("Hello").await;
    assert!(
        ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
            .iter()
            .any(|frame| terminal
                .screen
                .lock()
                .unwrap()
                .screen()
                .contents()
                .contains(frame)),
        "Busy state must show a spinner"
    );
    assert_eq!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    terminal.wait("Hello from Solmu").await;
    terminal.ready().await;
    assert_eq!(
        backend.messages(&id).await["items"][1]["content"],
        "Hello from Solmu"
    );
    terminal.command("/ren\tTerminal planning");
    terminal.wait("SOLMU    Terminal planning").await;
    terminal.ready().await;
    backend
        .client
        .patch(format!("{}/api/v1/threads/{id}", backend.url))
        .json(&serde_json::json!({"title": "Across clients"}))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    terminal.wait("SOLMU    Across clients").await;
    terminal.ready().await;
    terminal.command("/rename Terminal planning");
    terminal.wait("SOLMU    Terminal planning").await;
    terminal.ready().await;
    terminal.screenshot_source();
    terminal.command("/threads");
    terminal.wait("Conversations ·").await;
    terminal.wait(&id).await;
    terminal.ready().await;
    terminal.command("/new Another idea");
    terminal.wait("SOLMU    Another idea").await;
    terminal.ready().await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        2
    );
    terminal.command(&format!("/open {id}"));
    terminal.wait("Hello from the terminal").await;
    terminal.ready().await;
    terminal.command("/delete");
    terminal.wait("No conversation").await;
    terminal.ready().await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    terminal.command("/help");
    terminal.wait("/rename <title>").await;
    terminal.command("/unknown");
    terminal.wait("Unknown command").await;
    terminal.exit().await;
    let mut again = Terminal::start(&backend);
    again.ready().await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        2
    );
    again.exit().await;
}
