use std::{env, error::Error, net::SocketAddr};

use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let bind_addr = bind_addr()?;
    let listener = TcpListener::bind(bind_addr).await?;

    println!("INFO sunbolt control plane listening on {bind_addr}");

    axum::serve(listener, sunbolt_control::router())
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    println!("INFO sunbolt control plane stopped");
    Ok(())
}

fn bind_addr() -> Result<SocketAddr, Box<dyn Error>> {
    let raw = env::var("SUNBOLT_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
    raw.parse::<SocketAddr>()
        .map_err(|error| format!("invalid SUNBOLT_BIND_ADDR `{raw}`: {error}").into())
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("ERROR failed to listen for shutdown signal: {error}");
    }
}
