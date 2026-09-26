use super::*;

#[cfg(target_os = "linux")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_works_in_the_isolated_linux_workspace() {
    let backend = Backend::start().await;
    let mut terminal = Terminal::start_in(&backend, true);
    terminal.ready().await;
    terminal.command("Hello inside the Linux sandbox");
    terminal.wait("SOLMU · streaming").await;
    terminal.wait("Hello from Solmu").await;
    terminal.ready().await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(
        backend.messages(&id).await["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    terminal.exit().await;
}
