use std::{collections::HashMap, env, future::Future, time::Duration};

use serde::{Deserialize, Serialize};
use sunbolt_protocol::{
    AgentTerminalCommand, AgentTerminalEvent, TerminalExit, TerminalSessionId,
    TerminalSize as ProtocolTerminalSize,
};
use sunbolt_terminal::{
    LocalPtySession, TerminalError as PtyTerminalError, TerminalExitStatus, TerminalSize,
};
use thiserror::Error;
use tokio::time;
use tracing::info;

const DEFAULT_CONTROL_PLANE_URL: &str = "http://127.0.0.1:3000";
const DEFAULT_NODE_NAME: &str = "local-agent";
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(30);

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
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LocalNodeInfo {
    pub hostname: String,
    pub os: String,
    pub architecture: String,
    pub agent_version: String,
}

/// Agent enrollment request sent to the control plane.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentEnrollmentRequest {
    pub token: String,
    pub node_name: String,
    pub hostname: String,
    pub os: String,
    pub architecture: String,
    pub agent_version: String,
}

/// Enrollment response returned by the control plane.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentEnrollmentResponse {
    pub node_id: String,
    pub credential_fingerprint: String,
}

/// Heartbeat status reported by an agent connection.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentHeartbeatStatus {
    Online,
}

/// Agent heartbeat message sent to the control plane.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentHeartbeatMessage {
    pub node_id: String,
    pub credential_fingerprint: String,
    pub hostname: String,
    pub os: String,
    pub architecture: String,
    pub agent_version: String,
    pub status: AgentHeartbeatStatus,
}

impl AgentHeartbeatMessage {
    #[must_use]
    pub fn from_node_identity(
        node_id: impl Into<String>,
        credential_fingerprint: impl Into<String>,
        node_info: &LocalNodeInfo,
    ) -> Self {
        Self {
            node_id: node_id.into(),
            credential_fingerprint: credential_fingerprint.into(),
            hostname: node_info.hostname.clone(),
            os: node_info.os.clone(),
            architecture: node_info.architecture.clone(),
            agent_version: node_info.agent_version.clone(),
            status: AgentHeartbeatStatus::Online,
        }
    }

    #[must_use]
    pub const fn endpoint_path() -> &'static str {
        "/agent/heartbeat"
    }
}

/// Prepared outbound connection state for an enrolled agent.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AgentConnection {
    control_plane_url: String,
    heartbeat: AgentHeartbeatMessage,
}

impl AgentConnection {
    #[must_use]
    pub fn new(
        config: &AgentConfig,
        enrollment: &AgentEnrollmentResponse,
        node_info: &LocalNodeInfo,
    ) -> Self {
        Self {
            control_plane_url: config.control_plane_url.clone(),
            heartbeat: AgentHeartbeatMessage::from_node_identity(
                enrollment.node_id.clone(),
                enrollment.credential_fingerprint.clone(),
                node_info,
            ),
        }
    }

    #[must_use]
    pub fn heartbeat_endpoint(&self) -> String {
        join_url_path(
            &self.control_plane_url,
            AgentHeartbeatMessage::endpoint_path(),
        )
    }

    #[must_use]
    pub const fn heartbeat_message(&self) -> &AgentHeartbeatMessage {
        &self.heartbeat
    }
}

/// Agent-side terminal command handler backed by local PTY sessions.
pub struct AgentTerminalRuntime {
    sessions: HashMap<TerminalSessionId, LocalPtySession>,
}

impl AgentTerminalRuntime {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
        }
    }

    /// Applies a control-plane terminal command to the agent host.
    ///
    /// # Errors
    ///
    /// Returns an error when the command targets a missing session, starts a
    /// duplicate session, or the local PTY operation fails.
    pub fn handle_command(
        &mut self,
        command: AgentTerminalCommand,
    ) -> Result<Option<AgentTerminalEvent>, AgentError> {
        match command {
            AgentTerminalCommand::StartTerminal { session_id, size } => {
                if self.sessions.contains_key(&session_id) {
                    return Err(AgentError::TerminalSessionExists(session_id.0));
                }
                let session =
                    LocalPtySession::spawn_default_shell(terminal_size_from_protocol(size))?;
                self.sessions.insert(session_id.clone(), session);
                Ok(Some(AgentTerminalEvent::TerminalStarted {
                    session_id,
                    size,
                }))
            }
            AgentTerminalCommand::WriteInput { session_id, data } => {
                self.session(&session_id)?.write_input(data.as_bytes())?;
                Ok(None)
            }
            AgentTerminalCommand::ResizeTerminal { session_id, size } => {
                self.session(&session_id)?
                    .resize(terminal_size_from_protocol(size))?;
                Ok(None)
            }
            AgentTerminalCommand::CloseTerminal { session_id } => {
                let session = self
                    .sessions
                    .remove(&session_id)
                    .ok_or_else(|| AgentError::TerminalSessionMissing(session_id.0.clone()))?;
                session.close()?;
                Ok(Some(AgentTerminalEvent::TerminalExited {
                    session_id,
                    exit: TerminalExit { status: None },
                }))
            }
        }
    }

    /// Reads PTY output for a remote terminal session.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is missing or the PTY read fails.
    pub fn read_output(
        &self,
        session_id: &TerminalSessionId,
        buffer: &mut [u8],
    ) -> Result<Option<AgentTerminalEvent>, AgentError> {
        let read = self.session(session_id)?.read_output(buffer)?;
        if read == 0 {
            return Ok(None);
        }

        Ok(Some(AgentTerminalEvent::TerminalOutput {
            session_id: session_id.clone(),
            data: String::from_utf8_lossy(&buffer[..read]).into_owned(),
        }))
    }

    /// Polls the local PTY process exit status for a remote terminal session.
    ///
    /// # Errors
    ///
    /// Returns an error when the session is missing or PTY process polling
    /// fails.
    pub fn poll_exit(
        &mut self,
        session_id: &TerminalSessionId,
    ) -> Result<Option<AgentTerminalEvent>, AgentError> {
        let Some(exit) = self.session(session_id)?.try_wait_exit()? else {
            return Ok(None);
        };
        self.sessions.remove(session_id);
        Ok(Some(AgentTerminalEvent::TerminalExited {
            session_id: session_id.clone(),
            exit: terminal_exit(exit),
        }))
    }

    #[must_use]
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    fn session(&self, session_id: &TerminalSessionId) -> Result<&LocalPtySession, AgentError> {
        self.sessions
            .get(session_id)
            .ok_or_else(|| AgentError::TerminalSessionMissing(session_id.0.clone()))
    }
}

impl Default for AgentTerminalRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentEnrollmentRequest {
    /// Builds an enrollment request from agent config and local node data.
    ///
    /// # Errors
    ///
    /// Returns an error when the agent has no enrollment token configured.
    pub fn from_config(
        config: &AgentConfig,
        node_info: &LocalNodeInfo,
    ) -> Result<Self, AgentError> {
        let token = config
            .enrollment_token
            .clone()
            .ok_or(AgentError::MissingEnrollmentToken)?;

        Ok(Self {
            token,
            node_name: config.node_name.clone(),
            hostname: node_info.hostname.clone(),
            os: node_info.os.clone(),
            architecture: node_info.architecture.clone(),
            agent_version: node_info.agent_version.clone(),
        })
    }

    #[must_use]
    pub const fn endpoint_path() -> &'static str {
        "/agent/enroll"
    }
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
    /// Returns an error when enrollment or a heartbeat request to the control
    /// plane fails.
    pub async fn run_until_shutdown<S>(&self, shutdown: S) -> Result<(), AgentError>
    where
        S: Future<Output = ()>,
    {
        let client = reqwest::Client::new();
        let enrollment = self.enroll(&client).await?;
        info!(
            node_id = %enrollment.node_id,
            "agent enrolled with control plane"
        );

        let connection = AgentConnection::new(&self.config, &enrollment, &self.node_info);
        self.send_heartbeat(&client, &connection).await?;
        info!(
            node_id = %connection.heartbeat_message().node_id,
            "agent heartbeat accepted"
        );

        let mut heartbeat_interval = time::interval(HEARTBEAT_INTERVAL);
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                () = &mut shutdown => break,
                _ = heartbeat_interval.tick() => {
                    self.send_heartbeat(&client, &connection).await?;
                    info!(
                        node_id = %connection.heartbeat_message().node_id,
                        "agent heartbeat accepted"
                    );
                }
            }
        }
        Ok(())
    }

    async fn enroll(
        &self,
        client: &reqwest::Client,
    ) -> Result<AgentEnrollmentResponse, AgentError> {
        let request = AgentEnrollmentRequest::from_config(&self.config, &self.node_info)?;
        let response = client
            .post(join_url_path(
                &self.config.control_plane_url,
                AgentEnrollmentRequest::endpoint_path(),
            ))
            .json(&request)
            .send()
            .await?
            .error_for_status()?;

        Ok(response.json::<AgentEnrollmentResponse>().await?)
    }

    async fn send_heartbeat(
        &self,
        client: &reqwest::Client,
        connection: &AgentConnection,
    ) -> Result<(), AgentError> {
        client
            .post(connection.heartbeat_endpoint())
            .json(connection.heartbeat_message())
            .send()
            .await?
            .error_for_status()?;

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum AgentError {
    #[error("agent enrollment token is not configured")]
    MissingEnrollmentToken,
    #[error("agent control-plane request failed: {0}")]
    ControlPlaneRequest(#[from] reqwest::Error),
    #[error("terminal session {0} already exists")]
    TerminalSessionExists(String),
    #[error("terminal session {0} does not exist")]
    TerminalSessionMissing(String),
    #[error("agent terminal PTY operation failed: {0}")]
    Terminal(#[from] PtyTerminalError),
}

fn join_url_path(base: &str, path: &str) -> String {
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn terminal_size_from_protocol(size: ProtocolTerminalSize) -> TerminalSize {
    TerminalSize::new(size.cols.max(1), size.rows.max(1))
}

fn terminal_exit(exit: TerminalExitStatus) -> TerminalExit {
    TerminalExit { status: exit.code }
}

fn default_node_name(lookup: &mut impl FnMut(&str) -> Option<String>) -> String {
    let hostname = hostname_from_lookup(lookup);
    if hostname.trim().is_empty() {
        DEFAULT_NODE_NAME.to_owned()
    } else {
        hostname
    }
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
        component_name, AgentConfig, AgentConnection, AgentEnrollmentRequest,
        AgentEnrollmentResponse, AgentHeartbeatMessage, AgentHeartbeatStatus, AgentRuntime,
        AgentTerminalRuntime, LocalNodeInfo, LogLevel, DEFAULT_CONTROL_PLANE_URL,
    };
    use sunbolt_protocol::{
        AgentTerminalCommand, AgentTerminalEvent, TerminalSessionId, TerminalSize,
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
    fn heartbeat_message_uses_enrolled_node_identity() {
        let info = LocalNodeInfo {
            hostname: "host-a".to_owned(),
            os: "linux".to_owned(),
            architecture: "x86_64".to_owned(),
            agent_version: "0.1.0".to_owned(),
        };

        let heartbeat =
            AgentHeartbeatMessage::from_node_identity("node-1", "dev-fingerprint", &info);

        assert_eq!(heartbeat.node_id, "node-1");
        assert_eq!(heartbeat.credential_fingerprint, "dev-fingerprint");
        assert_eq!(heartbeat.hostname, "host-a");
        assert_eq!(heartbeat.status, AgentHeartbeatStatus::Online);
        assert_eq!(AgentHeartbeatMessage::endpoint_path(), "/agent/heartbeat");
    }

    #[test]
    fn agent_connection_builds_heartbeat_endpoint() {
        let config = AgentConfig {
            control_plane_url: "https://control.example.test/".to_owned(),
            node_name: "node-a".to_owned(),
            enrollment_token: None,
        };
        let enrollment = AgentEnrollmentResponse {
            node_id: "node-1".to_owned(),
            credential_fingerprint: "dev-fingerprint".to_owned(),
        };
        let info = LocalNodeInfo {
            hostname: "host-a".to_owned(),
            os: "linux".to_owned(),
            architecture: "x86_64".to_owned(),
            agent_version: "0.1.0".to_owned(),
        };

        let connection = AgentConnection::new(&config, &enrollment, &info);

        assert_eq!(
            connection.heartbeat_endpoint(),
            "https://control.example.test/agent/heartbeat"
        );
        assert_eq!(connection.heartbeat_message().node_id, "node-1");
    }

    #[test]
    fn agent_terminal_runtime_reports_missing_session() {
        let mut runtime = AgentTerminalRuntime::new();
        let result = runtime.handle_command(AgentTerminalCommand::CloseTerminal {
            session_id: TerminalSessionId("remote-1".to_owned()),
        });

        assert!(result.is_err());
    }

    #[test]
    fn agent_terminal_runtime_starts_and_closes_session_when_shell_is_available() {
        let mut runtime = AgentTerminalRuntime::new();
        let session_id = TerminalSessionId("remote-1".to_owned());
        let started = runtime.handle_command(AgentTerminalCommand::StartTerminal {
            session_id: session_id.clone(),
            size: TerminalSize { cols: 80, rows: 24 },
        });
        let Ok(Some(AgentTerminalEvent::TerminalStarted { .. })) = started else {
            return;
        };
        assert_eq!(runtime.session_count(), 1);

        let closed = runtime
            .handle_command(AgentTerminalCommand::CloseTerminal { session_id })
            .expect("close command should succeed");

        assert!(matches!(
            closed,
            Some(AgentTerminalEvent::TerminalExited { .. })
        ));
        assert_eq!(runtime.session_count(), 0);
    }

    #[test]
    fn enrollment_request_uses_config_token_and_node_info() {
        let request = AgentEnrollmentRequest::from_config(
            &AgentConfig {
                control_plane_url: "https://control.example.test".to_owned(),
                node_name: "node-a".to_owned(),
                enrollment_token: Some("token-1".to_owned()),
            },
            &LocalNodeInfo {
                hostname: "host-a".to_owned(),
                os: "linux".to_owned(),
                architecture: "x86_64".to_owned(),
                agent_version: "0.1.0".to_owned(),
            },
        )
        .expect("request should build");

        assert_eq!(request.token, "token-1");
        assert_eq!(request.node_name, "node-a");
        assert_eq!(request.hostname, "host-a");
        assert_eq!(AgentEnrollmentRequest::endpoint_path(), "/agent/enroll");
    }

    #[test]
    fn enrollment_request_requires_token() {
        let error = AgentEnrollmentRequest::from_config(
            &AgentConfig {
                control_plane_url: "https://control.example.test".to_owned(),
                node_name: "node-a".to_owned(),
                enrollment_token: None,
            },
            &LocalNodeInfo {
                hostname: "host-a".to_owned(),
                os: "linux".to_owned(),
                architecture: "x86_64".to_owned(),
                agent_version: "0.1.0".to_owned(),
            },
        )
        .expect_err("missing token should fail");

        assert_eq!(
            error.to_string(),
            "agent enrollment token is not configured"
        );
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
    async fn runtime_requires_enrollment_token() {
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

        let error = runtime
            .run_until_shutdown(async {
                let _ = rx.await;
            })
            .await
            .expect_err("missing enrollment token should stop startup");

        assert_eq!(
            error.to_string(),
            "agent enrollment token is not configured"
        );
    }
}
