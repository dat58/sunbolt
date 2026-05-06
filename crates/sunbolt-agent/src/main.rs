use sunbolt_agent::{AgentRuntime, LogLevel};
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> std::process::ExitCode {
    init_tracing();

    let runtime = AgentRuntime::from_env();

    for line in runtime.startup_logs() {
        match line.level {
            LogLevel::Info => info!(message = %line.message, "agent startup"),
        }
    }

    if let Err(error) = runtime.run_until_shutdown(shutdown_signal()).await {
        error!(%error, "sunbolt agent stopped with an error");
        return std::process::ExitCode::FAILURE;
    }

    info!("sunbolt agent stopped");
    std::process::ExitCode::SUCCESS
}

fn init_tracing() {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("sunbolt_agent=info"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        error!(%error, "failed to listen for shutdown signal");
    }
}
