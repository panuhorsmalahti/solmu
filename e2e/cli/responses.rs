use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_shows_provider_and_membership_errors_and_can_exit_while_streaming() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    terminal.command("FAIL");
    terminal.wait("LLM request failed").await;
    terminal.command("/open missing");
    terminal.wait("not found").await;
    terminal.command("/new Recovery");
    terminal.wait("SOLMU    Recovery").await;
    terminal.ready().await;
    terminal.command("Try again");
    terminal.wait("SOLMU · streaming").await;
    terminal.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_stop_command_and_escape_cancel_reply_and_allow_continuing() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    for (index, stop) in ["/stop", "\x1b"].iter().enumerate() {
        terminal.command("A response to stop");
        terminal.wait("SOLMU · streaming").await;
        terminal.wait("Hello").await;
        if *stop == "\x1b" {
            let mut writer = terminal.writer.lock().unwrap();
            writer.write_all(stop.as_bytes()).unwrap();
            writer.flush().unwrap();
        } else {
            terminal.command(stop);
        }
        terminal.ready().await;
        assert_eq!(
            backend.messages(&id).await["items"]
                .as_array()
                .unwrap()
                .len(),
            index + 1
        );
        assert!(
            !terminal
                .screen
                .lock()
                .unwrap()
                .screen()
                .contents()
                .contains("SOLMU · streaming")
        );
    }
    terminal.command("Continue after stopping");
    terminal.wait("Hello from Solmu").await;
    terminal.ready().await;
    assert_eq!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    terminal.exit().await;
}
