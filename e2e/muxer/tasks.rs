use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn solmu_panes_show_scheduled_tasks() {
    let backend = Backend::start().await;
    let task = backend.client.post(backend.endpoint("/api/v1/tasks"))
        .json(&serde_json::json!({"name":"Morning review","prompt":"Summarize the workspace","schedule_kind":"cron","schedule":"0 9 * * *"}))
        .send().await.unwrap();
    assert!(task.status().is_success());
    let _session = sessions::Session {
        backend: &backend,
        name: "tasks",
    };
    let mut terminal = Terminal::start_session(&backend, "tasks");
    terminal.wait("Ready").await;
    terminal.command("/tasks");
    terminal.wait("Morning review").await;
    terminal.send(b"\r");
    terminal.wait("Summarize the workspace").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(
            terminal.screen.lock().unwrap().screen(),
            "muxer-tasks",
        );
    }
    terminal.send(b"\x1b");
    terminal.wait("Ready").await;
    terminal.exit().await;
}
