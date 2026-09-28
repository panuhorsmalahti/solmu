use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn solmu_panes_show_workspace_plugins() {
    let backend = Backend::start().await;
    let _session = sessions::Session {
        backend: &backend,
        name: "plugins",
    };
    let mut terminal = Terminal::start_session(&backend, "plugins");
    terminal.wait("Ready").await;
    terminal.command("/plugins");
    terminal
        .wait("No plugins installed in this workspace.")
        .await;
    solmu_e2e::support::plugins::install(backend.directory.path(), false);
    terminal.wait("Tools for this project").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(
            terminal.screen.lock().unwrap().screen(),
            "muxer-plugins",
        );
    }
    terminal.send(b"\x1b");
    terminal.wait("Ready").await;
    terminal.exit().await;
}
