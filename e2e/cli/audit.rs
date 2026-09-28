use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn audit_command_pages_through_all_tool_calls_and_expands_details() {
    let backend = Backend::start().await;
    solmu_e2e::support::audit::seed(&backend, 5).await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    terminal.command("/audit");
    terminal.wait("Tool calls across every conversation").await;
    terminal.wait("page 1").await;
    terminal.send(b"\x1b[6~");
    terminal.wait("page 2").await;
    terminal.send(b"\x1b[5~");
    terminal.wait("page 1").await;
    terminal.send(b"\r");
    terminal.wait("Arguments:").await;
    terminal.wait("Result:").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(terminal.screen.lock().unwrap().screen(), "cli-audit");
    }
    terminal.send(b"\x1b");
    terminal.ready().await;
    terminal.exit().await;
}
