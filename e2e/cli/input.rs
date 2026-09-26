use super::*;

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
