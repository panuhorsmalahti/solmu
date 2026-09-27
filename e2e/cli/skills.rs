use super::*;
use solmu_e2e::support::skills::install;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn skills_command_completes_lists_live_workspace_skills_and_never_sends_a_message() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"/ski\t").unwrap();
        input.flush().unwrap();
    }
    terminal.wait("> /skills ").await;
    terminal.command("");
    terminal
        .wait("No skills installed in this workspace.")
        .await;
    let file = install(
        backend.directory.path(),
        "writing",
        "Write clear explanations with practical examples.",
    );
    terminal
        .wait("Write clear explanations with practical examples.")
        .await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(
            terminal.screen.lock().unwrap().screen(),
            "cli-skills",
        );
    }
    std::fs::write(
        file,
        "---\nname: writing\ndescription: Updated explanations.\n---\nUpdated instructions.\n",
    )
    .unwrap();
    terminal.wait("Updated explanations.").await;
    let threads = backend.threads().await;
    let id = threads["items"][0]["id"].as_str().unwrap();
    assert!(
        backend.messages(id).await["items"]
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
