use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plugins_command_lists_live_plugins_and_completes_from_slash() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"/").unwrap();
        input.flush().unwrap();
    }
    terminal.wait("/plugins").await;
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"plugins\r").unwrap();
        input.flush().unwrap();
    }
    terminal
        .wait("No plugins installed in this workspace.")
        .await;
    let root = solmu_e2e::support::plugins::install(backend.directory.path(), false);
    terminal.wait("Tools for this project").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(
            terminal.screen.lock().unwrap().screen(),
            "cli-plugins",
        );
    }
    std::fs::write(root.join("plugin.json"), "invalid JSON").unwrap();
    terminal.wait("Not loaded:").await;
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"\x1b").unwrap();
        input.flush().unwrap();
    }
    terminal.ready().await;
    terminal.exit().await;
}
