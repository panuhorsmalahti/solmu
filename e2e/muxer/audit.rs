use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn solmu_panes_can_browse_audit_tool_calls() {
    let backend = Backend::start().await;
    solmu_e2e::support::audit::seed(&backend, 1).await;
    let _session = sessions::Session {
        backend: &backend,
        name: "audit",
    };
    let mut terminal = Terminal::start_session(&backend, "audit");
    terminal.wait("Ready").await;
    terminal.command("/audit");
    terminal.wait("Tool calls across every conversation").await;
    terminal.send(b"\r");
    terminal.wait("Arguments:").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(
            terminal.screen.lock().unwrap().screen(),
            "muxer-audit",
        );
    }
    terminal.send(b"\x1b");
    terminal.wait("Ready").await;
    terminal.exit().await;
}
