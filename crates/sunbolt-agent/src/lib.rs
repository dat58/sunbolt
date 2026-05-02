use std::{env, future::Future};

use thiserror::Error;

const DEFAULT_CONTROL_PLANE_URL: &str = "http://127.0.0.1:3000";
const DEFAULT_NODE_NAME: &str = "local-agent";

/// Returns a stable name for the agent component.
#[must_use]
pub fn component_name() -> String {
    format!("{} agent", sunbolt_common::product_name())
}

/// Agent runtime configuration loaded from environment variables.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AgentConfig {
    pub control_plane_url: String,
    pub node_name: String,
    pub enrollment_token: Option<String>,
}

impl AgentConfig {
    /// Loads agent configuration from environment variables.
    ///
    /// Supported variables:
    /// - `SUNBOLT_CONTROL_PLANE_URL`
    /// - `SUNBOLT_AGENT_NODE_NAME`
    /// - `SUNBOLT_AGENT_ENROLLMENT_TOKEN`
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_lookup(|name| env::var(name).ok())
    }

    fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Self {
        let control_plane_url = lookup("SUNBOLT_CONTROL_PLANE_URL")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_CONTROL_PLANE_URL.to_owned());
        let node_name = lookup("SUNBOLT_AGENT_NODE_NAME")
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| default_node_name(&mut lookup));
        let enrollment_token =
            lookup("SUNBOLT_AGENT_ENROLLMENT_TOKEN").filter(|value| !value.trim().is_empty());

        Self {
            control_plane_url,
            node_name,
            enrollment_token,
        }
    }
}

/// Local node information reported by the agent.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LocalNodeInfo {
    pub hostname: String,
    pub os: String,
    pub architecture: String,
    pub agent_version: String,
}

impl LocalNodeInfo {
    /// Collects local node information from the host process environment.
    #[must_use]
    pub fn collect() -> Self {
        Self::from_lookup(|name| env::var(name).ok())
    }

    fn from_lookup(mut lookup: impl FnMut(&str) -> Option<String>) -> Self {
        Self {
            hostname: hostname_from_lookup(&mut lookup),
            os: env::consts::OS.to_owned(),
            architecture: env::consts::ARCH.to_owned(),
            agent_version: env!("CARGO_PKG_VERSION").to_owned(),
        }
    }
}

/// Startup log line emitted by the agent.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StartupLogLine {
    pub level: LogLevel,
    pub message: String,
}

/// Minimal log severity for deterministic startup logs.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum LogLevel {
    Info,
}

/// Agent runtime scaffold.
#[derive(Debug, Clone)]
pub struct AgentRuntime {
    config: AgentConfig,
    node_info: LocalNodeInfo,
}

impl AgentRuntime {
    #[must_use]
    pub fn new(config: AgentConfig, node_info: LocalNodeInfo) -> Self {
        Self { config, node_info }
    }

    #[must_use]
    pub fn from_env() -> Self {
        Self::new(AgentConfig::from_env(), LocalNodeInfo::collect())
    }

    /// Builds deterministic startup log records for the agent process.
    #[must_use]
    pub fn startup_logs(&self) -> Vec<StartupLogLine> {
        vec![
            StartupLogLine {
                level: LogLevel::Info,
                message: format!("starting {}", component_name()),
            },
            StartupLogLine {
                level: LogLevel::Info,
                message: format!("node name: {}", self.config.node_name),
            },
            StartupLogLine {
                level: LogLevel::Info,
                message: format!("control plane: {}", self.config.control_plane_url),
            },
            StartupLogLine {
                level: LogLevel::Info,
                message: format!(
                    "node info: hostname={} os={} architecture={} agent_version={}",
                    self.node_info.hostname,
                    self.node_info.os,
                    self.node_info.architecture,
                    self.node_info.agent_version
                ),
            },
        ]
    }

    /// Runs the agent until the provided shutdown signal completes.
    ///
    /// # Errors
    ///
    /// The initial Phase 3 runtime has no fallible background work. This
    /// method returns a `Result` so later connection and heartbeat loops can
    /// surface structured errors without changing the public runtime shape.
    pub async fn run_until_shutdown<S>(&self, shutdown: S) -> Result<(), AgentError>
    where
        S: Future<Output = ()>,
    {
        shutdown.await;
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum AgentError {}

fn default_node_name(lookup: &mut impl FnMut(&str) -> Option<String>) -> String {
    hostname_from_lookup(lookup)
        .trim()
        .is_empty()
        .then(|| DEFAULT_NODE_NAME.to_owned())
        .unwrap_or_else(|| hostname_from_lookup(lookup))
}

fn hostname_from_lookup(lookup: &mut impl FnMut(&str) -> Option<String>) -> String {
    lookup("HOSTNAME")
        .or_else(|| lookup("COMPUTERNAME"))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_NODE_NAME.to_owned())
}

#[cfg(test)]
mod tests {
    use super::{
        component_name, AgentConfig, AgentRuntime, LocalNodeInfo, LogLevel,
        DEFAULT_CONTROL_PLANE_URL,
    };
    use tokio::sync::oneshot;

    #[test]
    fn component_name_mentions_agent() {
        assert_eq!(component_name(), "Sunbolt agent");
    }

    #[test]
    fn agent_config_uses_defaults() {
        let config = AgentConfig::from_lookup(|_| None);

        assert_eq!(config.control_plane_url, DEFAULT_CONTROL_PLANE_URL);
        assert_eq!(config.node_name, "local-agent");
        assert_eq!(config.enrollment_token, None);
    }

    #[test]
    fn agent_config_reads_values() {
        let config = AgentConfig::from_lookup(|name| match name {
            "SUNBOLT_CONTROL_PLANE_URL" => Some("https://control.example.test".to_owned()),
            "SUNBOLT_AGENT_NODE_NAME" => Some("node-a".to_owned()),
            "SUNBOLT_AGENT_ENROLLMENT_TOKEN" => Some("token-1".to_owned()),
            _ => None,
        });

        assert_eq!(config.control_plane_url, "https://control.example.test");
        assert_eq!(config.node_name, "node-a");
        assert_eq!(config.enrollment_token.as_deref(), Some("token-1"));
    }

    #[test]
    fn local_node_info_collects_host_details() {
        let info = LocalNodeInfo::from_lookup(|name| match name {
            "HOSTNAME" => Some("sunbolt-node".to_owned()),
            _ => None,
        });

        assert_eq!(info.hostname, "sunbolt-node");
        assert!(!info.os.is_empty());
        assert!(!info.architecture.is_empty());
        assert_eq!(info.agent_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn startup_logs_include_config_and_node_info() {
        let runtime = AgentRuntime::new(
            AgentConfig {
                control_plane_url: "https://control.example.test".to_owned(),
                node_name: "node-a".to_owned(),
                enrollment_token: None,
            },
            LocalNodeInfo {
                hostname: "host-a".to_owned(),
                os: "linux".to_owned(),
                architecture: "x86_64".to_owned(),
                agent_version: "0.1.0".to_owned(),
            },
        );

        let logs = runtime.startup_logs();

        assert_eq!(logs[0].level, LogLevel::Info);
        assert!(logs.iter().any(|line| line.message.contains("node-a")));
        assert!(logs.iter().any(|line| line.message.contains("host-a")));
    }

    #[tokio::test]
    async fn runtime_stops_on_shutdown_signal() {
        let runtime = AgentRuntime::new(
            AgentConfig {
                control_plane_url: "https://control.example.test".to_owned(),
                node_name: "node-a".to_owned(),
                enrollment_token: None,
            },
            LocalNodeInfo {
                hostname: "host-a".to_owned(),
                os: "linux".to_owned(),
                architecture: "x86_64".to_owned(),
                agent_version: "0.1.0".to_owned(),
            },
        );
        let (tx, rx) = oneshot::channel();
        tx.send(()).expect("shutdown signal should send");

        runtime
            .run_until_shutdown(async {
                let _ = rx.await;
            })
            .await
            .expect("runtime should stop cleanly");
    }
}
