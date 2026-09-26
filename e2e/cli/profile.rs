use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_profile_edits_prompt_and_default_model_and_preserves_dirty_live_edits() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    let thread = backend.threads().await;
    assert_eq!(
        std::path::PathBuf::from(thread["items"][0]["workspace"].as_str().unwrap()),
        backend.directory.path().canonicalize().unwrap()
    );
    terminal.command("/new");
    terminal.ready().await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    terminal.command("/profile");
    terminal.wait("You are Solmu, an autonomous agent.").await;
    terminal.wait("test-model (default)").await;
    terminal.wait("Edited on ").await;
    {
        let mut input = terminal.writer.lock().unwrap();
        input
            .write_all(b"\x15You are Solmu.\rUse Finnish.\tgpt-6-luna\x13")
            .unwrap();
        input.flush().unwrap();
    }
    terminal.wait("Profile saved").await;
    let stored: serde_json::Value = backend
        .client
        .get(backend.endpoint("/api/v1/profile"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(stored["system_prompt"], "You are Solmu.\nUse Finnish.");
    assert_eq!(stored["model"], "gpt-6-luna");
    assert_eq!(stored["backend_default_model"], "test-model");
    terminal.wait(stored["edited_at"].as_str().unwrap()).await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(
            terminal.screen.lock().unwrap().screen(),
            "cli-profile",
        );
    }
    backend
        .client
        .put(backend.endpoint("/api/v1/profile"))
        .json(&serde_json::json!({"system_prompt":"Changed elsewhere", "model":null}))
        .send()
        .await
        .unwrap();
    terminal.wait("Changed elsewhere").await;
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"\t\x15My unsaved draft").unwrap();
        input.flush().unwrap();
    }
    terminal.wait("My unsaved draft").await;
    backend
        .client
        .put(backend.endpoint("/api/v1/profile"))
        .json(&serde_json::json!({"system_prompt":"Another change"}))
        .send()
        .await
        .unwrap();
    terminal.wait("Your draft is unchanged").await;
    terminal.wait("My unsaved draft").await;
    {
        let mut input = terminal.writer.lock().unwrap();
        input.write_all(b"\x1b").unwrap();
        input.flush().unwrap();
    }
    terminal.ready().await;
    terminal.exit().await;
}
