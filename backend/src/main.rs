use std::{env, error::Error, net::SocketAddr};

use axum::Router;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let bind_addr: SocketAddr = match env::var("SOLMU_BIND_ADDR") {
        Ok(value) => value.parse()?,
        Err(env::VarError::NotPresent) => "127.0.0.1:3000".parse()?,
        Err(error) => return Err(error.into()),
    };

    let listener = TcpListener::bind(bind_addr).await?;
    let app = Router::new();

    println!("Solmu listening on http://{}", listener.local_addr()?);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

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
