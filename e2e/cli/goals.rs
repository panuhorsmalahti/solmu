use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_creates_and_lists_persistent_goals() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    terminal.command("/goal Finish the first release");
    terminal
        .wait("Goal started: Finish the first release")
        .await;
    terminal.command("/goal");
    terminal.wait("Finish the first release").await;
    terminal.exit().await;
}
