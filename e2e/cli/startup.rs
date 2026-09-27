use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_startup_thread_reports_error_without_creating_a_new_thread() {
    let backend = Backend::start().await;
    let mut terminal =
        Terminal::start_with_thread(&backend, "00000000-0000-0000-0000-000000000001");
    terminal.wait("not found").await;
    assert!(
        backend.threads().await["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    terminal.command("/new");
    terminal.wait("New conversation").await;
    terminal.ready().await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    terminal.exit().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn thread_option_resumes_saved_history_without_creating_a_conversation() {
    let backend = Backend::start().await;
    let mut first = Terminal::start(&backend);
    first.ready().await;
    first.command("/rename Resume me");
    first.wait("SOLMU    Resume me").await;
    first.ready().await;
    let id = backend.threads().await["items"][0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    first.command("Remember this message");
    first.wait("Hello from Solmu").await;
    first.ready().await;
    first.exit().await;
    let mut resumed = Terminal::start_with_thread(&backend, &id);
    resumed.wait("SOLMU    Resume me").await;
    resumed.wait("Remember this message").await;
    resumed.wait("Hello from Solmu").await;
    resumed.ready().await;
    assert_eq!(
        backend.threads().await["items"].as_array().unwrap().len(),
        1
    );
    resumed.exit().await;
}

#[test]
fn startup_options_show_usage_version_and_reject_invalid_arguments() {
    let directory = tempfile::tempdir().unwrap();
    for arg in ["--help", "--version"] {
        let output = std::process::Command::new(binary("solmu"))
            .arg(arg)
            .current_dir(directory.path())
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("Solmu CLI"));
    }
    for args in [
        vec!["--thread"],
        vec!["--thread", "invalid"],
        vec!["--unknown"],
    ] {
        assert!(
            !std::process::Command::new(binary("solmu"))
                .args(args)
                .current_dir(directory.path())
                .output()
                .unwrap()
                .status
                .success()
        );
    }
}
