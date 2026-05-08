use std::{env, net::SocketAddr};

use thiserror::Error;
use tokio::net::TcpListener;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    init_tracing();

    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            error!(%error, "sunbolt control plane stopped with an error");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), ControlPlaneError> {
    let bind_addr = bind_addr()?;
    let router = sunbolt_control::try_router()
        .await
        .map_err(ControlPlaneError::Startup)?;
    let listener = TcpListener::bind(bind_addr)
        .await
        .map_err(ControlPlaneError::Bind)?;

    info!(%bind_addr, "sunbolt control plane listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(ControlPlaneError::Serve)?;

    info!("sunbolt control plane stopped");
    Ok(())
}

fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("sunbolt_control=info,tower_http=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

fn bind_addr() -> Result<SocketAddr, ControlPlaneError> {
    let raw = env::var("SUNBOLT_BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_owned());
    raw.parse::<SocketAddr>()
        .map_err(|source| ControlPlaneError::InvalidBindAddr { raw, source })
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        error!(%error, "failed to listen for shutdown signal");
    }
}

#[derive(Debug, Error)]
enum ControlPlaneError {
    #[error("invalid SUNBOLT_BIND_ADDR `{raw}`: {source}")]
    InvalidBindAddr {
        raw: String,
        source: std::net::AddrParseError,
    },
    #[error("failed to bind control-plane listener: {0}")]
    Bind(#[source] std::io::Error),
    #[error("control-plane startup validation failed: {0}")]
    Startup(#[source] sunbolt_control::StartupError),
    #[error("control-plane server failed: {0}")]
    Serve(#[source] std::io::Error),
}
