use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_tool_activity_is_live_saved_and_stoppable() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    terminal.command("Inspect this workspace");
    terminal.wait("Bash · completed").await;
    terminal.wait("Hello from Solmu").await;
    terminal.ready().await;
    terminal.wait("Workspace ready").await;
    assert_eq!(
        std::fs::read_to_string(backend.directory.path().join("hello.txt")).unwrap(),
        "Welcome tools\n"
    );
    terminal.screenshot_source();
    terminal.command("/new Another thread");
    terminal.wait("SOLMU    Another thread").await;
    terminal.ready().await;
    terminal.command(&format!("/open {id}"));
    terminal.wait("Bash · completed").await;
    terminal.ready().await;
    terminal.command("Run a long task");
    terminal.wait("Bash · running").await;
    terminal.command("/stop");
    terminal.ready().await;
    terminal.wait("Bash · cancelled").await;
    terminal.exit().await;
}
