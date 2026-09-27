use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn solmu_panes_list_workspace_skills_and_receive_live_catalog_updates() {
    let backend = Backend::start().await;
    let _session = sessions::Session {
        backend: &backend,
        name: "skills",
    };
    let mut terminal = Terminal::start_session(&backend, "skills");
    terminal.wait("Ready").await;
    terminal.command("/skills");
    terminal
        .wait("No skills installed in this workspace.")
        .await;
    solmu_e2e::support::skills::install(
        backend.directory.path(),
        "writing",
        "Clear practical writing for project documentation.",
    );
    terminal
        .wait("Clear practical writing for project documentation.")
        .await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(
            terminal.screen.lock().unwrap().screen(),
            "muxer-skills",
        );
    }
    terminal.send(b"\x1b");
    terminal.wait("Ready").await;
    terminal.exit().await;
}
