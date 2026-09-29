use std::error::Error;

use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

mod api;
mod config;
mod db;
mod llm;
mod mcp;
mod plugins;
mod prompt;
mod scheduler;
mod skills;
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
    let webhook_listener = TcpListener::bind(config.webhook_bind_addr).await?;
    let webhook_port = webhook_listener.local_addr()?.port();
    let state = api::state::AppState::new(pool.clone(), llm, webhook_port);
    let scheduler = tokio::spawn(scheduler::run(state.clone()));
    let app = web::router(api::router(state.clone()), &config.web_dir);
    let webhook_app = api::webhooks::receiver(state);
    let webhook_shutdown = CancellationToken::new();
    let cancel_webhook_shutdown = webhook_shutdown.clone();
    let webhook_server = tokio::spawn(async move {
        axum::serve(webhook_listener, webhook_app)
            .with_graceful_shutdown(async move { cancel_webhook_shutdown.cancelled().await })
            .await
    });

    println!("Solmu listening on http://{}", listener.local_addr()?);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    webhook_shutdown.cancel();
    webhook_server.await??;
    scheduler.abort();
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
