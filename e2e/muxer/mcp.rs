use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn solmu_panes_show_live_mcp_server_status_and_tools() {
    let backend = Backend::start().await;
    let _session = sessions::Session {
        backend: &backend,
        name: "mcp",
    };
    let mut terminal = Terminal::start_session(&backend, "mcp");
    terminal.wait("Ready").await;
    terminal.command("/mcp");
    terminal
        .wait("No MCP servers configured in this workspace.")
        .await;
    solmu_e2e::support::mcp::install(backend.directory.path(), false);
    terminal.wait("Search project notes with MCP.").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(terminal.screen.lock().unwrap().screen(), "muxer-mcp");
    }
    terminal.send(b"\x1b");
    terminal.wait("Ready").await;
    terminal.exit().await;
}
