use super::*;

#[tokio::test]
async fn workspace_profile_keeps_terminal_input_streaming_and_exit_available() {
    let backend = Backend::start().await;
    let mut cli = Terminal::start_boxed(&backend);
    cli.ready().await;
    cli.command("Hello from a Boxer workspace");
    cli.wait("Hello from Solmu").await;
    cli.ready().await;
    let threads = backend.threads().await;
    let thread = &threads["items"][0];
    assert_eq!(
        std::path::PathBuf::from(thread["workspace"].as_str().unwrap()),
        backend.directory.path().canonicalize().unwrap()
    );
    assert_eq!(
        backend.messages(thread["id"].as_str().unwrap()).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    cli.exit().await;
}

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn routed_private_network_keeps_http_streams_and_websocket_updates_available() {
    let backend = Backend::start().await;
    let mut cli = Terminal::start_routed(&backend);
    cli.ready().await;
    cli.command("Hello through the allowed local backend");
    cli.wait("Hello from Solmu").await;
    cli.ready().await;
    let threads = backend.threads().await;
    let id = threads["items"][0]["id"].as_str().unwrap();
    assert_eq!(
        backend.messages(id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let response = backend
        .client
        .patch(backend.endpoint(&format!("/api/v1/threads/{id}")))
        .json(&serde_json::json!({"title":"Routed live rename"}))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    cli.wait("Routed live rename").await;
    cli.exit().await;
}
