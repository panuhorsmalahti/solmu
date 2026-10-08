use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn memories_command_lists_saved_facts_and_dates() {
    let backend = Backend::start().await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(backend.directory.path().join("solmu.db")),
        )
        .await
        .unwrap();
    sqlx::query("INSERT INTO memories (id, content) VALUES (?, ?)")
        .bind("memory-cli")
        .bind("My cat is named Miso")
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
    let mut terminal = Terminal::start(&backend);
    terminal.ready().await;
    terminal.command("/memories");
    terminal.wait("SOLMU / MEMORIES").await;
    terminal.wait("My cat is named Miso").await;
    terminal.wait("Saved ").await;
    if std::env::var_os("SOLMU_CAPTURE_SCREENSHOTS").is_some() {
        solmu_e2e::support::capture_terminal(
            terminal.screen.lock().unwrap().screen(),
            "cli-memories",
        );
    }
    terminal.send(b"\x1b");
    terminal.ready().await;
    terminal.exit().await;
}
