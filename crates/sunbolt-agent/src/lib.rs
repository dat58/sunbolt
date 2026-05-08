use std::{
    collections::HashMap,
    env, fs,
    fs::OpenOptions,
    future::Future,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sunbolt_protocol::{
    transport::{
        AgentTransportClientHello, AgentTransportEnvelope, AgentTransportHeartbeat,
        AgentTransportId, AgentTransportKind, AgentTransportMessageId, AgentTransportPayload,
        AgentTransportReconnectPolicy,
    },
    AgentTerminalCommand, AgentTerminalEvent, NodeId, TerminalExit, TerminalSessionId,
    TerminalSize as ProtocolTerminalSize, PROTOCOL_VERSION,
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
const AGENT_TRANSPORT_WS_PATH: &str = "/agent/transport/ws";
const AGENT_TRANSPORT_QUIC_PATH: &str = "/agent/transport/quic";
const AGENT_TRANSPORT_LONG_POLL_PATH: &str = "/agent/transport/long-poll";
const AGENT_IDENTITY_FILE_NAME: &str = "identity.json";

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
    pub identity_path: PathBuf,
}

impl AgentConfig {
    /// Loads agent configuration from environment variables.
    ///
    /// Supported variables:
    /// - `SUNBOLT_CONTROL_PLANE_URL`
    /// - `SUNBOLT_AGENT_NODE_NAME`
    /// - `SUNBOLT_AGENT_ENROLLMENT_TOKEN`
    /// - `SUNBOLT_AGENT_IDENTITY_PATH`
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
        let identity_path = lookup("SUNBOLT_AGENT_IDENTITY_PATH")
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_identity_path(&mut lookup));

        Self {
            control_plane_url,
            node_name,
            enrollment_token,
            identity_path,
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
    pub credential_secret: String,
    pub credential_expires_at_unix_secs: i64,
}

/// Durable node identity material persisted by the agent host.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentNodeIdentity {
    pub node_id: String,
    pub credential_fingerprint: String,
    pub credential_secret: String,
    pub credential_expires_at_unix_secs: i64,
    pub agent_version: String,
}

impl AgentNodeIdentity {
    #[must_use]
    pub fn from_enrollment(
        enrollment: &AgentEnrollmentResponse,
        node_info: &LocalNodeInfo,
    ) -> Self {
        Self {
            node_id: enrollment.node_id.clone(),
            credential_fingerprint: enrollment.credential_fingerprint.clone(),
            credential_secret: enrollment.credential_secret.clone(),
            credential_expires_at_unix_secs: enrollment.credential_expires_at_unix_secs,
            agent_version: node_info.agent_version.clone(),
        }
    }

    /// Loads durable agent identity material from disk when present.
    ///
    /// # Errors
    ///
    /// Returns an error when the identity file exists but cannot be read or parsed.
    pub fn load_from_path(path: &Path) -> Result<Option<Self>, AgentError> {
        match fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents)
                .map(Some)
                .map_err(AgentError::IdentityFormat),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(AgentError::IdentityIo(error)),
        }
    }

    /// Persists durable agent identity material with restrictive file permissions.
    ///
    /// # Errors
    ///
    /// Returns an error when the identity directory or file cannot be written.
    pub fn save_to_path(&self, path: &Path) -> Result<(), AgentError> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            let parent_already_exists = parent.exists();
            fs::create_dir_all(parent).map_err(AgentError::IdentityIo)?;
            if !parent_already_exists {
                restrict_directory_permissions(parent)?;
            }
        }

        let mut options = OpenOptions::new();
        options.create(true).truncate(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let file = options.open(path).map_err(AgentError::IdentityIo)?;
        serde_json::to_writer_pretty(file, self).map_err(AgentError::IdentityFormat)?;
        restrict_file_permissions(path)?;
        Ok(())
    }

    #[must_use]
    pub fn credential_secret_matches_fingerprint(&self) -> bool {
        credential_fingerprint(&self.credential_secret) == self.credential_fingerprint
    }
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
        identity: &AgentNodeIdentity,
        node_info: &LocalNodeInfo,
    ) -> Self {
        Self {
            control_plane_url: config.control_plane_url.clone(),
            heartbeat: AgentHeartbeatMessage::from_node_identity(
                identity.node_id.clone(),
                identity.credential_fingerprint.clone(),
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

    #[must_use]
    pub fn transport_endpoint(&self) -> String {
        websocket_url_for_path(&self.control_plane_url, AGENT_TRANSPORT_WS_PATH)
    }

    #[must_use]
    pub fn quic_transport_endpoint(&self) -> String {
        quic_url_for_path(&self.control_plane_url, AGENT_TRANSPORT_QUIC_PATH)
    }

    #[must_use]
    pub fn long_poll_transport_endpoint(&self) -> String {
        join_url_path(&self.control_plane_url, AGENT_TRANSPORT_LONG_POLL_PATH)
    }

    #[must_use]
    pub fn transport_id(&self) -> AgentTransportId {
        AgentTransportId(format!("{}-outbound", self.heartbeat.node_id))
    }

    #[must_use]
    pub fn transport_client_hello(&self) -> AgentTransportClientHello {
        AgentTransportClientHello {
            supported_protocol_versions: vec![PROTOCOL_VERSION],
            supported_transports: vec![
                AgentTransportKind::QuicUdp443,
                AgentTransportKind::WebSocketTlsTcp443,
                AgentTransportKind::Http2TlsTcp443,
                AgentTransportKind::LongPollHttps,
            ],
            preferred_transport: AgentTransportKind::QuicUdp443,
            agent_version: self.heartbeat.agent_version.clone(),
            credential_fingerprint: self.heartbeat.credential_fingerprint.clone(),
            resume: None,
        }
    }

    #[must_use]
    pub fn transport_client_hello_envelope(&self) -> AgentTransportEnvelope {
        AgentTransportEnvelope::current(
            AgentTransportMessageId::first(),
            NodeId(self.heartbeat.node_id.clone()),
            self.transport_id(),
            AgentTransportPayload::ClientHello {
                hello: self.transport_client_hello(),
            },
        )
    }

    #[must_use]
    pub fn heartbeat_envelope(
        &self,
        message_id: AgentTransportMessageId,
    ) -> AgentTransportEnvelope {
        AgentTransportEnvelope::current(
            message_id,
            NodeId(self.heartbeat.node_id.clone()),
            self.transport_id(),
            AgentTransportPayload::Heartbeat {
                heartbeat: AgentTransportHeartbeat::Ping { sent_at_unix_ms: 0 },
            },
        )
    }

    #[must_use]
    pub fn reconnect_policy(&self) -> AgentTransportReconnectPolicy {
        AgentTransportReconnectPolicy::production_default()
    }
}

/// Agent-side baseline outbound transport settings.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct AgentOutboundTransportPlan {
    pub endpoint: String,
    pub quic_fast_path_endpoint: String,
    pub long_poll_fallback_endpoint: String,
    pub transport_attempt_order: Vec<AgentTransportKind>,
    pub client_hello: AgentTransportClientHello,
    pub reconnect_policy: AgentTransportReconnectPolicy,
    pub heartbeat_interval: Duration,
}

impl AgentOutboundTransportPlan {
    #[must_use]
    pub fn from_connection(connection: &AgentConnection) -> Self {
        Self {
            endpoint: connection.transport_endpoint(),
            quic_fast_path_endpoint: connection.quic_transport_endpoint(),
            long_poll_fallback_endpoint: connection.long_poll_transport_endpoint(),
            transport_attempt_order: vec![
                AgentTransportKind::QuicUdp443,
                AgentTransportKind::WebSocketTlsTcp443,
                AgentTransportKind::Http2TlsTcp443,
                AgentTransportKind::LongPollHttps,
            ],
            client_hello: connection.transport_client_hello(),
            reconnect_policy: connection.reconnect_policy(),
            heartbeat_interval: HEARTBEAT_INTERVAL,
        }
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
        let identity = self.load_or_enroll_identity(&client).await?;
        info!(
            node_id = %identity.node_id,
            "agent identity loaded"
        );

        let connection = AgentConnection::new(&self.config, &identity, &self.node_info);
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

    async fn load_or_enroll_identity(
        &self,
        client: &reqwest::Client,
    ) -> Result<AgentNodeIdentity, AgentError> {
        if let Some(identity) = AgentNodeIdentity::load_from_path(&self.config.identity_path)? {
            return Ok(identity);
        }

        let enrollment = self.enroll(client).await?;
        let identity = AgentNodeIdentity::from_enrollment(&enrollment, &self.node_info);
        identity.save_to_path(&self.config.identity_path)?;
        Ok(identity)
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
    #[error("agent identity file I/O failed: {0}")]
    IdentityIo(#[source] std::io::Error),
    #[error("agent identity file format is invalid: {0}")]
    IdentityFormat(#[source] serde_json::Error),
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

fn websocket_url_for_path(base: &str, path: &str) -> String {
    let endpoint = join_url_path(base, path);
    if let Some(rest) = endpoint.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = endpoint.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        endpoint
    }
}

fn quic_url_for_path(base: &str, path: &str) -> String {
    let endpoint = join_url_path(base, path);
    if let Some(rest) = endpoint.strip_prefix("https://") {
        format!("quic://{rest}")
    } else if let Some(rest) = endpoint.strip_prefix("http://") {
        format!("quic://{rest}")
    } else {
        endpoint
    }
}

fn default_identity_path(lookup: &mut impl FnMut(&str) -> Option<String>) -> PathBuf {
    if let Some(config_home) = lookup("XDG_CONFIG_HOME").filter(|value| !value.trim().is_empty()) {
        return PathBuf::from(config_home)
            .join("sunbolt")
            .join(AGENT_IDENTITY_FILE_NAME);
    }
    if let Some(home) = lookup("HOME").filter(|value| !value.trim().is_empty()) {
        return PathBuf::from(home)
            .join(".config")
            .join("sunbolt")
            .join(AGENT_IDENTITY_FILE_NAME);
    }
    if let Some(profile) = lookup("USERPROFILE").filter(|value| !value.trim().is_empty()) {
        return PathBuf::from(profile)
            .join("AppData")
            .join("Roaming")
            .join("Sunbolt")
            .join(AGENT_IDENTITY_FILE_NAME);
    }
    PathBuf::from(".sunbolt").join(AGENT_IDENTITY_FILE_NAME)
}

fn credential_fingerprint(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    let mut fingerprint = String::with_capacity("sha256:".len() + digest.len() * 2);
    fingerprint.push_str("sha256:");
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(fingerprint, "{byte:02x}");
    }
    fingerprint
}

fn restrict_directory_permissions(path: &Path) -> Result<(), AgentError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(AgentError::IdentityIo)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

fn restrict_file_permissions(path: &Path) -> Result<(), AgentError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))
            .map_err(AgentError::IdentityIo)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
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
        component_name, credential_fingerprint, quic_url_for_path, websocket_url_for_path,
        AgentConfig, AgentConnection, AgentEnrollmentRequest, AgentEnrollmentResponse,
        AgentHeartbeatMessage, AgentHeartbeatStatus, AgentNodeIdentity, AgentOutboundTransportPlan,
        AgentRuntime, AgentTerminalRuntime, LocalNodeInfo, LogLevel, DEFAULT_CONTROL_PLANE_URL,
    };
    use std::path::PathBuf;
    use sunbolt_protocol::{
        transport::{
            AgentTransportHeartbeat, AgentTransportKind, AgentTransportMessageId,
            AgentTransportPayload,
        },
        PROTOCOL_VERSION,
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
        assert_eq!(
            config.identity_path,
            PathBuf::from(".sunbolt").join("identity.json")
        );
    }

    #[test]
    fn agent_config_reads_values() {
        let config = AgentConfig::from_lookup(|name| match name {
            "SUNBOLT_CONTROL_PLANE_URL" => Some("https://control.example.test".to_owned()),
            "SUNBOLT_AGENT_NODE_NAME" => Some("node-a".to_owned()),
            "SUNBOLT_AGENT_ENROLLMENT_TOKEN" => Some("token-1".to_owned()),
            "SUNBOLT_AGENT_IDENTITY_PATH" => Some("/tmp/sunbolt-agent.json".to_owned()),
            _ => None,
        });

        assert_eq!(config.control_plane_url, "https://control.example.test");
        assert_eq!(config.node_name, "node-a");
        assert_eq!(config.enrollment_token.as_deref(), Some("token-1"));
        assert_eq!(
            config.identity_path,
            PathBuf::from("/tmp/sunbolt-agent.json")
        );
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
            identity_path: test_identity_path(),
        };
        let identity = test_identity();
        let info = LocalNodeInfo {
            hostname: "host-a".to_owned(),
            os: "linux".to_owned(),
            architecture: "x86_64".to_owned(),
            agent_version: "0.1.0".to_owned(),
        };

        let connection = AgentConnection::new(&config, &identity, &info);

        assert_eq!(
            connection.heartbeat_endpoint(),
            "https://control.example.test/agent/heartbeat"
        );
        assert_eq!(connection.heartbeat_message().node_id, "node-1");
    }

    #[test]
    fn agent_connection_builds_outbound_websocket_transport_endpoint() {
        assert_eq!(
            websocket_url_for_path("https://control.example.test/", "/agent/transport/ws"),
            "wss://control.example.test/agent/transport/ws"
        );
        assert_eq!(
            websocket_url_for_path("http://127.0.0.1:3000", "/agent/transport/ws"),
            "ws://127.0.0.1:3000/agent/transport/ws"
        );
    }

    #[test]
    fn agent_connection_builds_quic_fast_path_endpoint() {
        assert_eq!(
            quic_url_for_path("https://control.example.test/", "/agent/transport/quic"),
            "quic://control.example.test/agent/transport/quic"
        );
        assert_eq!(
            quic_url_for_path("http://127.0.0.1:3000", "/agent/transport/quic"),
            "quic://127.0.0.1:3000/agent/transport/quic"
        );
    }

    #[test]
    fn agent_connection_builds_restrictive_network_fallback_endpoint() {
        let config = AgentConfig {
            control_plane_url: "https://control.example.test/".to_owned(),
            node_name: "node-a".to_owned(),
            enrollment_token: None,
            identity_path: test_identity_path(),
        };
        let identity = test_identity();
        let info = LocalNodeInfo {
            hostname: "host-a".to_owned(),
            os: "linux".to_owned(),
            architecture: "x86_64".to_owned(),
            agent_version: "0.1.0".to_owned(),
        };

        let connection = AgentConnection::new(&config, &identity, &info);

        assert_eq!(
            connection.long_poll_transport_endpoint(),
            "https://control.example.test/agent/transport/long-poll"
        );
        assert!(connection
            .transport_client_hello()
            .supported_transports
            .contains(&AgentTransportKind::QuicUdp443));
        assert!(connection
            .transport_client_hello()
            .supported_transports
            .contains(&AgentTransportKind::LongPollHttps));
    }

    #[test]
    fn outbound_transport_plan_uses_enrolled_node_identity() {
        let config = AgentConfig {
            control_plane_url: "https://control.example.test".to_owned(),
            node_name: "node-a".to_owned(),
            enrollment_token: None,
            identity_path: test_identity_path(),
        };
        let identity = test_identity();
        let info = LocalNodeInfo {
            hostname: "host-a".to_owned(),
            os: "linux".to_owned(),
            architecture: "x86_64".to_owned(),
            agent_version: "0.1.0".to_owned(),
        };
        let connection = AgentConnection::new(&config, &identity, &info);

        let plan = AgentOutboundTransportPlan::from_connection(&connection);
        let hello_envelope = connection.transport_client_hello_envelope();
        let heartbeat = connection.heartbeat_envelope(AgentTransportMessageId(2));

        assert_eq!(
            plan.endpoint,
            "wss://control.example.test/agent/transport/ws"
        );
        assert_eq!(
            plan.quic_fast_path_endpoint,
            "quic://control.example.test/agent/transport/quic"
        );
        assert_eq!(
            plan.long_poll_fallback_endpoint,
            "https://control.example.test/agent/transport/long-poll"
        );
        assert_eq!(
            plan.transport_attempt_order,
            vec![
                AgentTransportKind::QuicUdp443,
                AgentTransportKind::WebSocketTlsTcp443,
                AgentTransportKind::Http2TlsTcp443,
                AgentTransportKind::LongPollHttps,
            ]
        );
        assert_eq!(
            plan.client_hello.supported_protocol_versions,
            vec![PROTOCOL_VERSION]
        );
        assert_eq!(
            plan.client_hello.preferred_transport,
            AgentTransportKind::QuicUdp443
        );
        assert_eq!(plan.reconnect_policy.delay_for_attempt(0), 1_000);
        assert_eq!(plan.reconnect_policy.delay_for_attempt(1), 2_000);
        assert_eq!(plan.reconnect_policy.delay_for_attempt(8), 30_000);
        assert_eq!(plan.reconnect_policy.resume_window_ms, 120_000);
        assert_eq!(
            hello_envelope.payload,
            AgentTransportPayload::ClientHello {
                hello: plan.client_hello
            }
        );
        assert_eq!(
            heartbeat.payload,
            AgentTransportPayload::Heartbeat {
                heartbeat: AgentTransportHeartbeat::Ping { sent_at_unix_ms: 0 },
            }
        );
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
                identity_path: test_identity_path(),
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
                identity_path: test_identity_path(),
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
                identity_path: test_identity_path(),
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
                identity_path: test_identity_path(),
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

    #[test]
    fn node_identity_persists_to_restricted_file() {
        let path = std::env::temp_dir()
            .join(format!(
                "sunbolt-agent-identity-test-{}-{}",
                std::process::id(),
                "node_identity_persists"
            ))
            .join("identity.json");
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
        let enrollment = AgentEnrollmentResponse {
            node_id: "node-1".to_owned(),
            credential_fingerprint: credential_fingerprint("secret-1"),
            credential_secret: "secret-1".to_owned(),
            credential_expires_at_unix_secs: 4_102_444_800,
        };
        let identity = AgentNodeIdentity::from_enrollment(
            &enrollment,
            &LocalNodeInfo {
                hostname: "host-a".to_owned(),
                os: "linux".to_owned(),
                architecture: "x86_64".to_owned(),
                agent_version: "0.1.0".to_owned(),
            },
        );

        identity
            .save_to_path(&path)
            .expect("identity file should save");
        let loaded = AgentNodeIdentity::load_from_path(&path)
            .expect("identity file should load")
            .expect("identity should exist");

        assert_eq!(loaded, identity);
        assert!(loaded.credential_secret_matches_fingerprint());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("identity metadata should load")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
            let dir_mode = std::fs::metadata(path.parent().expect("identity has parent"))
                .expect("identity directory metadata should load")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(dir_mode, 0o700);
        }
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }
    }

    fn test_identity_path() -> PathBuf {
        PathBuf::from("/tmp/sunbolt-agent-test-identity.json")
    }

    fn test_identity() -> AgentNodeIdentity {
        AgentNodeIdentity {
            node_id: "node-1".to_owned(),
            credential_fingerprint: credential_fingerprint("secret-1"),
            credential_secret: "secret-1".to_owned(),
            credential_expires_at_unix_secs: 4_102_444_800,
            agent_version: "0.1.0".to_owned(),
        }
    }
}
