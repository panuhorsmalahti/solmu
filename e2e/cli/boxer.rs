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
