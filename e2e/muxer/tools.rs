use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn muxer_tabs_keep_live_and_saved_tool_activity() {
    let backend = Backend::start().await;
    let mut tui = Terminal::start(&backend, &[]);
    tui.wait("Ready").await;
    tui.command("Inspect this workspace");
    tui.wait("Bash · completed").await;
    tui.wait("Hello from Solmu").await;
    tui.wait("Ready").await;
    tui.prefix('n');
    tui.wait("Solmu 2 · idle").await;
    tui.wait("Ready").await;
    tui.prefix('1');
    tui.wait("Bash · completed").await;
    tui.prefix('s');
    tui.wait("SOLMU    New conversation").await;
    solmu_e2e::support::capture_terminal(tui.screen.lock().unwrap().screen(), "muxer");
    tui.command("Run a long task");
    tui.wait("Bash · running").await;
    tui.command("/stop");
    tui.wait("Bash · cancelled").await;
    tui.wait("Ready").await;
    tui.exit().await;
}
