use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_creates_and_browses_tasks() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    terminal.command("/task cron 0 9 * * * | Morning review | Summarize the workspace");
    terminal.wait("Task created: Morning review").await;
    terminal.command("/tasks");
    terminal.wait("Morning review").await;
    terminal.send(b"\r");
    terminal.wait("Summarize the workspace").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(terminal.screen.lock().unwrap().screen(), "cli-tasks");
    }
    terminal.send(b"\x1b");
    let tasks: serde_json::Value = backend
        .client
        .get(backend.endpoint("/api/v1/tasks"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = tasks["items"][0]["id"].as_str().unwrap();
    terminal.command(&format!("/task pause {id}"));
    terminal.wait("Task paused").await;
    terminal.command(&format!("/task resume {id}"));
    terminal.wait("Task resumed").await;
    terminal.command(&format!("/task runs {id}"));
    terminal.wait("No runs yet").await;
    terminal.command(&format!("/task delete {id}"));
    terminal.wait("Task deleted").await;
    terminal.exit().await;
}
