use sunbolt_agent::{AgentRuntime, LogLevel};

#[tokio::main]
async fn main() {
    let runtime = AgentRuntime::from_env();

    for line in runtime.startup_logs() {
        match line.level {
            LogLevel::Info => println!("INFO {}", line.message),
        }
    }

    if let Err(error) = runtime.run_until_shutdown(shutdown_signal()).await {
        eprintln!("ERROR {error}");
        std::process::exit(1);
    }

    println!("INFO sunbolt agent stopped");
}

async fn shutdown_signal() {
    if let Err(error) = tokio::signal::ctrl_c().await {
        eprintln!("ERROR failed to listen for shutdown signal: {error}");
    }
}
