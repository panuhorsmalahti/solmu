#[tokio::main(worker_threads = 2)]
async fn main() {
    let backend = solmu_e2e::support::Backend::start().await;
    if std::env::var_os("SOLMU_FIXTURE_WEB").is_some() {
        let source = solmu_e2e::support::root().join("clients/web/dist");
        let destination = backend.directory.path().join("web");
        std::fs::create_dir_all(destination.join("assets")).unwrap();
        std::fs::copy(source.join("index.html"), destination.join("index.html")).unwrap();
        for entry in std::fs::read_dir(source.join("assets")).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(
                entry.path(),
                destination.join("assets").join(entry.file_name()),
            )
            .unwrap();
        }
    }
    println!("{}", backend.url);
    // Keep the real backend and local provider alive until the test runner exits.
    let mut input = String::new();
    let _ = tokio::task::spawn_blocking(move || std::io::stdin().read_line(&mut input)).await;
}
