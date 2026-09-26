use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn redraw_preserves_unsent_input_and_conversation() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    {
        let mut writer = terminal.writer.lock().unwrap();
        writer.write_all(b"Keep my draft\x0c").unwrap();
        writer.flush().unwrap();
    }
    terminal.wait("Keep my draft").await;
    terminal.ready().await;
    terminal.command("");
    terminal.wait("Hello from Solmu").await;
    terminal.ready().await;
    let messages = backend.messages(&id).await;
    assert_eq!(messages["items"][0]["content"], "Keep my draft");
    assert_eq!(messages["items"].as_array().unwrap().len(), 2);
    terminal.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slash_command_list_filters_navigates_and_completes_without_persistent_hints() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    for hint in [
        "Enter send",
        "Tab complete",
        "/help commands",
        "/exit quit",
        "PageUp/PageDown scroll",
    ] {
        assert!(
            !terminal
                .screen
                .lock()
                .unwrap()
                .screen()
                .contents()
                .contains(hint)
        );
    }
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"/").unwrap();
        input.flush().unwrap();
    }
    terminal.wait("Start a new conversation").await;
    terminal.wait("Edit Solmu's system prompt").await;
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"prof\t").unwrap();
        input.flush().unwrap();
    }
    terminal.wait("> /profile ").await;
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"\r").unwrap();
        input.flush().unwrap();
    }
    terminal.wait("You are Solmu, an autonomous agent.").await;
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"\x1b").unwrap();
        input.flush().unwrap();
    }
    terminal.ready().await;
    terminal.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_keeps_enter_while_a_conversation_operation_finishes() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    terminal.command("/new Pending operation\r/rename Preserved submission");
    terminal.wait("SOLMU    Preserved submission").await;
    terminal.ready().await;
    assert!(
        backend.threads().await["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|thread| thread["title"] == "Preserved submission")
    );
    terminal.exit().await;
}
