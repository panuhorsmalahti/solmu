#[tokio::main(worker_threads = 2)]
async fn main() {
    let backend = solmu_e2e::support::Backend::start().await;
    println!("{}", backend.url);
    // Keep the real backend and local provider alive until the test runner exits.
    let mut input = String::new();
    let _ = tokio::task::spawn_blocking(move || std::io::stdin().read_line(&mut input)).await;
}
