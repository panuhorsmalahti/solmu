use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mcp_command_completes_shows_live_servers_tools_and_config_errors_without_sending_messages()
{
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"/mc\t").unwrap();
        input.flush().unwrap();
    }
    terminal.wait("> /mcp ").await;
    terminal.command("");
    terminal
        .wait("No MCP servers configured in this workspace.")
        .await;
    let config = solmu_e2e::support::mcp::install(backend.directory.path(), false);
    terminal.wait("Search project notes with MCP.").await;
    terminal.wait("notes · connected · stdio").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(terminal.screen.lock().unwrap().screen(), "cli-mcp");
    }
    std::fs::write(
        backend.directory.path().join("mcp-description.txt"),
        "Updated MCP notes.",
    )
    .unwrap();
    terminal.wait("Updated MCP notes.").await;
    std::fs::write(config, "invalid JSON").unwrap();
    terminal.wait("MCP config must be valid JSON").await;
    let threads = backend.threads().await;
    assert!(
        backend
            .messages(threads["items"][0]["id"].as_str().unwrap())
            .await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"\x1b").unwrap();
        input.flush().unwrap();
    }
    terminal.ready().await;
    terminal.exit().await;
}
