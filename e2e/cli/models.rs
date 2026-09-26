use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_model_picker_and_command_support_known_custom_and_default_models() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    terminal.command("/model");
    terminal.wait("GPT 6 Luna").await;
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"\x1b[B\x1b[B\x1b[B\r").unwrap();
        input.flush().unwrap();
    }
    terminal.ready().await;
    let thread: serde_json::Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(thread["model"], "gpt-6-luna");
    terminal.command("/model test-model");
    terminal.wait("Model: test-model").await;
    terminal.ready().await;
    terminal.command("/model default");
    terminal.wait("Model: default").await;
    terminal.ready().await;
    let thread: serde_json::Value = backend
        .client
        .get(backend.endpoint(&format!("/api/v1/threads/{id}")))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(thread["model"].is_null());
    terminal.exit().await;
}
