use std::error::Error;

use tokio::net::TcpListener;

mod api;
mod config;
mod db;
mod llm;
mod prompt;
mod storage;
mod tools;
mod web;
mod workspace;

fn main() -> Result<(), Box<dyn Error>> {
    if let Err(error) = dotenvy::dotenv()
        && !error.not_found()
    {
        return Err("Could not load .env; check its syntax and permissions".into());
    }
    run()
}

#[tokio::main(worker_threads = 2)]
async fn run() -> Result<(), Box<dyn Error>> {
    let config = config::Config::from_env()?;
    let llm = llm::Llm::from_env()?;
    let pool = db::connect(&config.database_url).await?;
    let listener = TcpListener::bind(config.bind_addr).await?;
    let app = web::router(
        api::router(api::state::AppState::new(pool.clone(), llm)),
        &config.web_dir,
    );

    println!("Solmu listening on http://{}", listener.local_addr()?);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    pool.close().await;

    Ok(())
}

async fn shutdown_signal() {
    let interrupt = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Could not listen for Ctrl+C");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Could not listen for SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = interrupt => {},
        _ = terminate => {},
    }
}
