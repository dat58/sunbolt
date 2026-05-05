use std::{
    collections::HashMap,
    env,
    hash::{Hash, Hasher},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Extension, Json, Path, Request, State,
    },
    http::{
        header::{COOKIE, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    middleware::{from_fn_with_state, Next},
    response::IntoResponse,
    response::Response,
    routing::{get, post},
    Router,
};
use futures_util::{SinkExt, StreamExt};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sunbolt_audit::{AuditEvent, AuditEventInput, AuditEventKind, AuditLog};
use sunbolt_auth::{AuthError, AuthService, FactorType, User, SESSION_COOKIE_NAME};
use sunbolt_protocol::{
    AgentTerminalCommand, AgentTerminalEvent, NodeId, TerminalClientMessage,
    TerminalError as ProtocolTerminalError, TerminalErrorCode, TerminalExit,
    TerminalReconnectToken, TerminalServerMessage, TerminalSessionId,
    TerminalSize as ProtocolTerminalSize,
};
use sunbolt_terminal::{
    LocalPtySession, TerminalError, TerminalExitStatus, TerminalSessionState, TerminalSize,
};
use tokio::{
    sync::{broadcast, mpsc, Mutex as AsyncMutex},
    task,
};

const OUTPUT_BUFFER_SIZE: usize = 8192;
const OUTPUT_CHANNEL_CAPACITY: usize = 32;
const READ_SHUTDOWN_GRACE: Duration = Duration::from_millis(100);
const DEFAULT_MAX_TERMINAL_SESSIONS: usize = 16;
const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const DEFAULT_DISCONNECT_GRACE: Duration = Duration::from_secs(30);
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(5);
const NODE_OFFLINE_AFTER: Duration = Duration::from_secs(90);
const DEFAULT_MAX_SESSIONS_PER_USER: usize = 5;
const DEFAULT_MAX_SESSIONS_PER_NODE: usize = 10;
const DEFAULT_MAX_DURATION: Duration = Duration::from_secs(8 * 60 * 60);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60);

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

/// WebSocket path for browser terminal connections.
pub const TERMINAL_WS_PATH: &str = "/terminal/ws";
pub const AUTH_LOGIN_PATH: &str = "/auth/login";
pub const AUTH_LOGOUT_PATH: &str = "/auth/logout";
pub const AUTH_ME_PATH: &str = "/auth/me";
pub const AUTH_MFA_STEP_UP_PATH: &str = "/auth/mfa/step-up";
pub const ACCESS_HISTORY_PATH: &str = "/access/history";
pub const AUDIT_LOGS_PATH: &str = "/audit/logs";
pub const ENROLLMENT_TOKENS_PATH: &str = "/nodes/enrollment-tokens";
pub const AGENT_ENROLL_PATH: &str = "/agent/enroll";
pub const AGENT_HEARTBEAT_PATH: &str = "/agent/heartbeat";
pub const NODES_PATH: &str = "/nodes";

/// Returns a stable name for the control plane component.
#[must_use]
pub fn component_name() -> String {
    format!("{} control plane", sunbolt_common::product_name())
}

/// Builds the control-plane router.
pub fn router() -> Router {
    build_router(AppState::from_env())
}

fn build_router(state: AppState) -> Router {
    spawn_session_cleanup_worker(
        state.sessions.clone(),
        state.terminal_config,
        state.audit.clone(),
    );
    let auth_layer = from_fn_with_state(state.auth.clone(), require_auth_middleware);

    Router::new()
        .route(TERMINAL_WS_PATH, get(terminal_websocket))
        .route(AUTH_LOGIN_PATH, post(auth_login))
        .route(
            AUTH_MFA_STEP_UP_PATH,
            post(auth_mfa_step_up).layer(auth_layer.clone()),
        )
        .route(
            AUTH_LOGOUT_PATH,
            post(auth_logout).layer(auth_layer.clone()),
        )
        .route(AUTH_ME_PATH, get(auth_me).layer(auth_layer.clone()))
        .route(
            ACCESS_HISTORY_PATH,
            get(access_history).layer(auth_layer.clone()),
        )
        .route(AUDIT_LOGS_PATH, get(audit_logs).layer(auth_layer.clone()))
        .route(NODES_PATH, get(list_nodes).layer(auth_layer.clone()))
        .route(
            "/nodes/{node_id}",
            get(node_details).layer(auth_layer.clone()),
        )
        .route(
            "/nodes/{node_id}/revoke",
            post(revoke_node).layer(auth_layer.clone()),
        )
        .route(
            ENROLLMENT_TOKENS_PATH,
            post(create_enrollment_token).layer(auth_layer),
        )
        .route(AGENT_ENROLL_PATH, post(agent_enroll))
        .route(AGENT_HEARTBEAT_PATH, post(agent_heartbeat))
        .with_state(state)
}

#[derive(Clone)]
struct AppState {
    sessions: TerminalSessionRegistry,
    terminal_config: TerminalSessionConfig,
    auth: AuthService,
    audit: AuditLog,
    node_enrollment: NodeEnrollmentRegistry,
    agent_connections: AgentConnectionRegistry,
}

impl AppState {
    fn from_env() -> Self {
        Self {
            sessions: TerminalSessionRegistry::default(),
            terminal_config: TerminalSessionConfig::from_env(),
            auth: AuthService::from_env(),
            audit: AuditLog::default(),
            node_enrollment: NodeEnrollmentRegistry::default(),
            agent_connections: AgentConnectionRegistry::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct TerminalSessionConfig {
    max_sessions: usize,
    max_sessions_per_user: usize,
    max_sessions_per_node: usize,
    idle_timeout: Duration,
    max_duration: Duration,
    disconnect_grace: Duration,
}

impl TerminalSessionConfig {
    fn from_env() -> Self {
        Self {
            max_sessions: env_usize("SUNBOLT_MAX_TERMINAL_SESSIONS")
                .unwrap_or(DEFAULT_MAX_TERMINAL_SESSIONS),
            max_sessions_per_user: env_usize("SUNBOLT_MAX_TERMINAL_SESSIONS_PER_USER")
                .unwrap_or(DEFAULT_MAX_SESSIONS_PER_USER),
            max_sessions_per_node: env_usize("SUNBOLT_MAX_TERMINAL_SESSIONS_PER_NODE")
                .unwrap_or(DEFAULT_MAX_SESSIONS_PER_NODE),
            idle_timeout: env_duration_secs("SUNBOLT_TERMINAL_IDLE_TIMEOUT_SECS")
                .unwrap_or(DEFAULT_IDLE_TIMEOUT),
            max_duration: env_duration_secs("SUNBOLT_TERMINAL_MAX_DURATION_SECS")
                .unwrap_or(DEFAULT_MAX_DURATION),
            disconnect_grace: env_duration_secs("SUNBOLT_TERMINAL_DISCONNECT_GRACE_SECS")
                .unwrap_or(DEFAULT_DISCONNECT_GRACE),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum SessionLimitError {
    GlobalCapacity,
    PerUser,
    PerNode,
}

impl SessionLimitError {
    const fn message(self) -> &'static str {
        match self {
            Self::GlobalCapacity => "maximum terminal session count reached",
            Self::PerUser => "maximum terminal sessions per user reached",
            Self::PerNode => "maximum terminal sessions per node reached",
        }
    }
}

#[derive(Clone, Default)]
struct TerminalSessionRegistry {
    inner: Arc<Mutex<HashMap<TerminalSessionId, TrackedTerminalSession>>>,
}

struct TrackedTerminalSession {
    session: Arc<LocalPtySession>,
    output_tx: broadcast::Sender<TerminalServerMessage>,
    reconnect_token: TerminalReconnectToken,
    state: TerminalSessionState,
    last_activity: Instant,
    created_at: Instant,
    size: ProtocolTerminalSize,
    actor_email: String,
    node_id: Option<String>,
}

impl TerminalSessionRegistry {
    fn insert(
        &self,
        session_id: TerminalSessionId,
        session: Arc<LocalPtySession>,
        size: ProtocolTerminalSize,
        config: TerminalSessionConfig,
        actor_email: String,
        node_id: Option<String>,
    ) -> Result<broadcast::Sender<TerminalServerMessage>, SessionLimitError> {
        let Ok(mut sessions) = self.inner.lock() else {
            return Err(SessionLimitError::GlobalCapacity);
        };
        if sessions.len() >= config.max_sessions {
            return Err(SessionLimitError::GlobalCapacity);
        }
        let user_count = sessions
            .values()
            .filter(|s| {
                s.actor_email == actor_email
                    && !matches!(
                        s.state,
                        TerminalSessionState::Closing | TerminalSessionState::Closed
                    )
            })
            .count();
        if user_count >= config.max_sessions_per_user {
            return Err(SessionLimitError::PerUser);
        }
        let node_count = sessions
            .values()
            .filter(|s| {
                s.node_id.as_deref() == node_id.as_deref()
                    && !matches!(
                        s.state,
                        TerminalSessionState::Closing | TerminalSessionState::Closed
                    )
            })
            .count();
        if node_count >= config.max_sessions_per_node {
            return Err(SessionLimitError::PerNode);
        }
        let (output_tx, _) = broadcast::channel(OUTPUT_CHANNEL_CAPACITY);
        sessions.insert(
            session_id,
            TrackedTerminalSession {
                session,
                output_tx: output_tx.clone(),
                reconnect_token: TerminalReconnectToken(random_token()),
                state: TerminalSessionState::Starting,
                last_activity: Instant::now(),
                created_at: Instant::now(),
                size,
                actor_email,
                node_id,
            },
        );
        Ok(output_tx)
    }

    fn set_state(&self, session_id: &TerminalSessionId, state: TerminalSessionState) {
        if let Ok(mut sessions) = self.inner.lock() {
            if let Some(session) = sessions.get_mut(session_id) {
                if !session.state.can_transition_to(state) && session.state != state {
                    return;
                }
                session.state = state;
            }
        }
    }

    fn detach(&self, session_id: &TerminalSessionId) {
        self.set_state(session_id, TerminalSessionState::Detached);
    }

    fn reattach(
        &self,
        session_id: &TerminalSessionId,
        reconnect_token: &TerminalReconnectToken,
    ) -> Option<(
        Arc<LocalPtySession>,
        broadcast::Receiver<TerminalServerMessage>,
        ProtocolTerminalSize,
        TerminalReconnectToken,
    )> {
        let Ok(mut sessions) = self.inner.lock() else {
            return None;
        };
        let tracked = sessions.get_mut(session_id)?;
        if !matches!(
            tracked.state,
            TerminalSessionState::Detached | TerminalSessionState::Reconnecting
        ) {
            return None;
        }
        if &tracked.reconnect_token != reconnect_token {
            return None;
        }
        tracked.reconnect_token = TerminalReconnectToken(random_token());
        tracked.state = TerminalSessionState::Reconnecting;
        tracked.last_activity = Instant::now();
        Some((
            Arc::clone(&tracked.session),
            tracked.output_tx.subscribe(),
            tracked.size,
            tracked.reconnect_token.clone(),
        ))
    }

    fn reconnect_token(&self, session_id: &TerminalSessionId) -> Option<TerminalReconnectToken> {
        self.inner.lock().ok().and_then(|sessions| {
            sessions
                .get(session_id)
                .map(|session| session.reconnect_token.clone())
        })
    }

    fn touch(&self, session_id: &TerminalSessionId) {
        if let Ok(mut sessions) = self.inner.lock() {
            if let Some(session) = sessions.get_mut(session_id) {
                session.last_activity = Instant::now();
            }
        }
    }

    fn is_idle(&self, session_id: &TerminalSessionId, timeout: Duration) -> bool {
        let Ok(sessions) = self.inner.lock() else {
            return true;
        };
        sessions
            .get(session_id)
            .is_none_or(|session| session.last_activity.elapsed() >= timeout)
    }

    fn set_size(&self, session_id: &TerminalSessionId, size: ProtocolTerminalSize) {
        if let Ok(mut sessions) = self.inner.lock() {
            if let Some(session) = sessions.get_mut(session_id) {
                session.size = size;
            }
        }
    }

    fn remove_if_detached(&self, session_id: &TerminalSessionId) -> bool {
        let Ok(mut sessions) = self.inner.lock() else {
            return false;
        };
        let should_remove = sessions
            .get(session_id)
            .is_some_and(|session| session.state == TerminalSessionState::Detached);
        if !should_remove {
            return false;
        }
        if let Some(session) = sessions.remove(session_id) {
            let _ = session.session.close();
            return true;
        }
        false
    }

    fn remove(&self, session_id: &TerminalSessionId) {
        if let Ok(mut sessions) = self.inner.lock() {
            if let Some(session) = sessions.remove(session_id) {
                let _ = session.session.close();
            }
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner.lock().map_or(0, |sessions| sessions.len())
    }

    fn state(&self, session_id: &TerminalSessionId) -> Option<TerminalSessionState> {
        self.inner
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(session_id).map(|session| session.state))
    }

    fn is_exceeded_max_duration(
        &self,
        session_id: &TerminalSessionId,
        max_duration: Duration,
    ) -> bool {
        let Ok(sessions) = self.inner.lock() else {
            return true;
        };
        sessions
            .get(session_id)
            .is_none_or(|session| session.created_at.elapsed() >= max_duration)
    }

    fn drain_exceeded_max_duration(
        &self,
        max_duration: Duration,
    ) -> Vec<(
        TerminalSessionId,
        String,
        broadcast::Sender<TerminalServerMessage>,
        Arc<LocalPtySession>,
    )> {
        let Ok(mut sessions) = self.inner.lock() else {
            return vec![];
        };
        let expired: Vec<TerminalSessionId> = sessions
            .iter()
            .filter(|(_, session)| {
                session.created_at.elapsed() >= max_duration
                    && !matches!(
                        session.state,
                        TerminalSessionState::Closing | TerminalSessionState::Closed
                    )
            })
            .map(|(id, _)| id.clone())
            .collect();
        expired
            .into_iter()
            .filter_map(|id| {
                sessions
                    .remove(&id)
                    .map(|s| (id, s.actor_email, s.output_tx, s.session))
            })
            .collect()
    }
}

impl Drop for TerminalSessionRegistry {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) != 1 {
            return;
        }

        if let Ok(mut sessions) = self.inner.lock() {
            for (_, session) in sessions.drain() {
                let _ = session.session.close();
            }
        }
    }
}

#[derive(Clone)]
struct NodeEnrollmentRegistry {
    inner: Arc<Mutex<NodeEnrollmentState>>,
    next_token_id: Arc<AtomicU64>,
    next_node_id: Arc<AtomicU64>,
}

impl Default for NodeEnrollmentRegistry {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(NodeEnrollmentState::default())),
            next_token_id: Arc::new(AtomicU64::new(1)),
            next_node_id: Arc::new(AtomicU64::new(1)),
        }
    }
}

#[derive(Default)]
struct NodeEnrollmentState {
    tokens_by_hash: HashMap<u64, EnrollmentTokenRecord>,
    nodes: Vec<NodeRecord>,
    credentials: Vec<NodeCredentialRecord>,
    heartbeats: Vec<NodeHeartbeatRecord>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct EnrollmentTokenRecord {
    id: u64,
    created_by_user_id: u64,
    expires_at: Instant,
    used_by_node_id: Option<u64>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
struct NodeRecord {
    id: u64,
    node_id: String,
    display_name: String,
    hostname: String,
    os: String,
    architecture: String,
    agent_version: String,
    status: NodeStatus,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum NodeStatus {
    Enrolled,
    Online,
    Offline,
    Revoked,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct NodeCredentialRecord {
    node_id: u64,
    credential_fingerprint: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct NodeHeartbeatRecord {
    node_id: u64,
    status: NodeStatus,
    received_at: Instant,
}

#[derive(Debug, Clone)]
struct RegisteredAgentConnection {
    command_tx: mpsc::Sender<AgentTerminalCommand>,
    event_rx: Arc<AsyncMutex<mpsc::Receiver<AgentTerminalEvent>>>,
}

#[derive(Clone, Default)]
struct AgentConnectionRegistry {
    inner: Arc<Mutex<HashMap<String, RegisteredAgentConnection>>>,
}

impl AgentConnectionRegistry {
    #[cfg(test)]
    fn register(
        &self,
        node_id: impl Into<String>,
        command_tx: mpsc::Sender<AgentTerminalCommand>,
        event_rx: mpsc::Receiver<AgentTerminalEvent>,
    ) {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                node_id.into(),
                RegisteredAgentConnection {
                    command_tx,
                    event_rx: Arc::new(AsyncMutex::new(event_rx)),
                },
            );
    }

    fn connection(&self, node_id: &str) -> Option<RegisteredAgentConnection> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(node_id)
            .cloned()
    }

    fn disconnect(&self, node_id: &str) {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(node_id);
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

impl NodeEnrollmentRegistry {
    fn create_token(&self, user: &User, ttl: Duration) -> EnrollmentTokenResponse {
        let token = random_token();
        let token_hash = token_hash(&token);
        let token_id = self.next_token_id.fetch_add(1, Ordering::Relaxed);
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.tokens_by_hash.insert(
            token_hash,
            EnrollmentTokenRecord {
                id: token_id,
                created_by_user_id: user.id,
                expires_at: Instant::now() + ttl,
                used_by_node_id: None,
            },
        );

        EnrollmentTokenResponse {
            token: token.clone(),
            expires_in_secs: ttl.as_secs(),
            enrollment_command: format!(
                "SUNBOLT_CONTROL_PLANE_URL=http://127.0.0.1:3000 SUNBOLT_AGENT_ENROLLMENT_TOKEN={token} cargo run -p sunbolt-agent"
            ),
        }
    }

    fn enroll(
        &self,
        request: AgentEnrollmentRequest,
    ) -> Result<AgentEnrollmentResponse, EnrollmentError> {
        let token_hash = token_hash(&request.token);
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let token = state
            .tokens_by_hash
            .get_mut(&token_hash)
            .ok_or(EnrollmentError::InvalidToken)?;
        if token.used_by_node_id.is_some() {
            return Err(EnrollmentError::TokenUsed);
        }
        if token.expires_at <= Instant::now() {
            return Err(EnrollmentError::TokenExpired);
        }

        let id = self.next_node_id.fetch_add(1, Ordering::Relaxed);
        let node_id = format!("node-{id}");
        let credential_fingerprint = credential_fingerprint(&request);
        token.used_by_node_id = Some(id);

        state.nodes.push(NodeRecord {
            id,
            node_id: node_id.clone(),
            display_name: request.node_name,
            hostname: request.hostname,
            os: request.os,
            architecture: request.architecture,
            agent_version: request.agent_version,
            status: NodeStatus::Enrolled,
        });
        state.credentials.push(NodeCredentialRecord {
            node_id: id,
            credential_fingerprint: credential_fingerprint.clone(),
        });
        state.heartbeats.push(NodeHeartbeatRecord {
            node_id: id,
            status: NodeStatus::Enrolled,
            received_at: Instant::now(),
        });

        Ok(AgentEnrollmentResponse {
            node_id,
            credential_fingerprint,
        })
    }

    fn heartbeat(&self, request: AgentHeartbeatRequest) -> Result<NodeView, NodeConnectionError> {
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let node_index = state
            .nodes
            .iter()
            .position(|node| node.node_id == request.node_id)
            .ok_or(NodeConnectionError::UnknownNode)?;
        let node = &state.nodes[node_index];
        if node.status == NodeStatus::Revoked {
            return Err(NodeConnectionError::Revoked);
        }
        let credential_matches = state.credentials.iter().any(|credential| {
            credential.node_id == node.id
                && credential.credential_fingerprint == request.credential_fingerprint
        });
        if !credential_matches {
            return Err(NodeConnectionError::InvalidCredential);
        }

        state.nodes[node_index].hostname = request.hostname;
        state.nodes[node_index].os = request.os;
        state.nodes[node_index].architecture = request.architecture;
        state.nodes[node_index].agent_version = request.agent_version;
        state.nodes[node_index].status = NodeStatus::Online;
        let node_id = state.nodes[node_index].id;
        state.heartbeats.push(NodeHeartbeatRecord {
            node_id,
            status: NodeStatus::Online,
            received_at: Instant::now(),
        });

        Ok(node_view(&state.nodes[node_index], Some(Instant::now())))
    }

    fn list_nodes(&self) -> Vec<NodeView> {
        let state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .nodes
            .iter()
            .map(|node| node_view(node, latest_heartbeat_at(&state, node.id)))
            .collect()
    }

    fn node_details(&self, node_id: &str) -> Option<NodeView> {
        let state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .nodes
            .iter()
            .find(|node| node.node_id == node_id)
            .map(|node| node_view(node, latest_heartbeat_at(&state, node.id)))
    }

    fn revoke_node(&self, node_id: &str) -> Result<NodeView, NodeConnectionError> {
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let node_index = state
            .nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .ok_or(NodeConnectionError::UnknownNode)?;
        state.nodes[node_index].status = NodeStatus::Revoked;
        Ok(node_view(
            &state.nodes[node_index],
            latest_heartbeat_at(&state, state.nodes[node_index].id),
        ))
    }

    fn node_is_online(&self, node_id: &str) -> bool {
        self.node_details(node_id)
            .is_some_and(|node| node.status == NodeStatus::Online)
    }
}

fn latest_heartbeat_at(state: &NodeEnrollmentState, node_id: u64) -> Option<Instant> {
    state
        .heartbeats
        .iter()
        .filter(|heartbeat| heartbeat.node_id == node_id && heartbeat.status != NodeStatus::Revoked)
        .max_by_key(|heartbeat| heartbeat.received_at)
        .map(|heartbeat| heartbeat.received_at)
}

fn node_view(node: &NodeRecord, last_heartbeat_at: Option<Instant>) -> NodeView {
    let status = match node.status {
        NodeStatus::Online
            if last_heartbeat_at
                .is_some_and(|received_at| received_at.elapsed() >= NODE_OFFLINE_AFTER) =>
        {
            NodeStatus::Offline
        }
        status => status,
    };

    NodeView {
        node_id: node.node_id.clone(),
        display_name: node.display_name.clone(),
        hostname: node.hostname.clone(),
        os: node.os.clone(),
        architecture: node.architecture.clone(),
        agent_version: node.agent_version.clone(),
        status,
    }
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    user: User,
}

#[derive(Debug, Serialize)]
struct CurrentUserResponse {
    user: User,
}

#[derive(Debug, Deserialize)]
struct StepUpMfaRequest {
    factor_type: FactorType,
}

#[derive(Debug, Serialize)]
struct StepUpMfaResponse {
    accepted: bool,
    factor_type: FactorType,
}

#[derive(Debug, Serialize)]
struct AuditEntriesResponse {
    events: Vec<AuditEvent>,
}

#[derive(Debug, Deserialize)]
struct EnrollmentTokenRequest {
    expires_in_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
struct EnrollmentTokenResponse {
    token: String,
    expires_in_secs: u64,
    enrollment_command: String,
}

#[derive(Debug, Serialize)]
struct NodeListResponse {
    nodes: Vec<NodeView>,
}

#[derive(Debug, Serialize)]
struct NodeDetailsResponse {
    node: NodeView,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
struct NodeView {
    node_id: String,
    display_name: String,
    hostname: String,
    os: String,
    architecture: String,
    agent_version: String,
    status: NodeStatus,
}

#[derive(Debug, Deserialize)]
struct AgentEnrollmentRequest {
    token: String,
    node_name: String,
    hostname: String,
    os: String,
    architecture: String,
    agent_version: String,
}

#[derive(Debug, Serialize)]
struct AgentEnrollmentResponse {
    node_id: String,
    credential_fingerprint: String,
}

#[derive(Debug, Deserialize)]
struct AgentHeartbeatRequest {
    node_id: String,
    credential_fingerprint: String,
    hostname: String,
    os: String,
    architecture: String,
    agent_version: String,
}

#[derive(Debug, Serialize)]
struct AgentHeartbeatResponse {
    accepted: bool,
    node: NodeView,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: &'static str,
}

#[derive(Debug, Clone)]
struct AuthenticatedUser(User);

async fn auth_login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> impl IntoResponse {
    match state.auth.login(&request.email, &request.password) {
        Ok((user, session_token)) => {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::UserLoginSuccess,
                actor_email: Some(user.email.clone()),
                message: "user authenticated".to_owned(),
            });
            let mut response = Json(LoginResponse { user }).into_response();
            match HeaderValue::from_str(&state.auth.session_cookie_header(&session_token)) {
                Ok(cookie) => {
                    response.headers_mut().append(SET_COOKIE, cookie);
                    response
                }
                Err(_) => (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "failed to set auth cookie",
                    }),
                )
                    .into_response(),
            }
        }
        Err(AuthError::InvalidCredentials) => (StatusCode::UNAUTHORIZED, {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::UserLoginFailed,
                actor_email: Some(request.email),
                message: "login rejected".to_owned(),
            });
            Json(ErrorResponse {
                error: "invalid credentials",
            })
        })
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "auth service unavailable",
            }),
        )
            .into_response(),
    }
}

async fn auth_logout(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Some(token) = session_token_from_headers(&headers) {
        let _ = state.auth.logout(token);
    }
    state.audit.record(AuditEventInput {
        kind: AuditEventKind::UserLogout,
        actor_email: Some(user.0.email.clone()),
        message: "user logged out".to_owned(),
    });

    let mut response = Json(CurrentUserResponse { user: user.0 }).into_response();
    if let Ok(cookie) = HeaderValue::from_str(&state.auth.clear_session_cookie_header()) {
        response.headers_mut().append(SET_COOKIE, cookie);
    }
    response
}

async fn auth_me(Extension(user): Extension<AuthenticatedUser>) -> impl IntoResponse {
    Json(CurrentUserResponse { user: user.0 })
}

async fn auth_mfa_step_up(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    headers: HeaderMap,
    Json(request): Json<StepUpMfaRequest>,
) -> impl IntoResponse {
    state.audit.record(AuditEventInput {
        kind: AuditEventKind::UserMfaChallenge,
        actor_email: Some(user.0.email.clone()),
        message: format!(
            "step-up MFA challenge requested using {:?}",
            request.factor_type
        ),
    });

    let Some(token) = session_token_from_headers(&headers) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "missing auth session",
            }),
        )
            .into_response();
    };
    match state.auth.record_mfa_success(token) {
        Ok(()) => {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::UserMfaSuccess,
                actor_email: Some(user.0.email),
                message: format!("step-up MFA completed using {:?}", request.factor_type),
            });
            Json(StepUpMfaResponse {
                accepted: true,
                factor_type: request.factor_type,
            })
            .into_response()
        }
        Err(AuthError::InvalidSession) => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid auth session",
            }),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "auth service unavailable",
            }),
        )
            .into_response(),
    }
}

async fn access_history(State(state): State<AppState>) -> impl IntoResponse {
    Json(AuditEntriesResponse {
        events: state.audit.access_history(),
    })
}

async fn audit_logs(State(state): State<AppState>) -> impl IntoResponse {
    Json(AuditEntriesResponse {
        events: state.audit.events(),
    })
}

async fn create_enrollment_token(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Json(request): Json<EnrollmentTokenRequest>,
) -> impl IntoResponse {
    let ttl = Duration::from_secs(request.expires_in_secs.unwrap_or(15 * 60).max(60));
    Json(state.node_enrollment.create_token(&user.0, ttl))
}

async fn list_nodes(State(state): State<AppState>) -> impl IntoResponse {
    Json(NodeListResponse {
        nodes: state.node_enrollment.list_nodes(),
    })
}

async fn node_details(
    State(state): State<AppState>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    match state.node_enrollment.node_details(&node_id) {
        Some(node) => Json(NodeDetailsResponse { node }).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "node not found",
            }),
        )
            .into_response(),
    }
}

async fn revoke_node(
    State(state): State<AppState>,
    Extension(user): Extension<AuthenticatedUser>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    match state.node_enrollment.revoke_node(&node_id) {
        Ok(node) => {
            state.agent_connections.disconnect(&node_id);
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::NodeRevoked,
                actor_email: Some(user.0.email),
                message: format!("node {node_id} revoked"),
            });
            Json(NodeDetailsResponse { node }).into_response()
        }
        Err(NodeConnectionError::UnknownNode) => (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "node not found",
            }),
        )
            .into_response(),
        Err(NodeConnectionError::InvalidCredential | NodeConnectionError::Revoked) => (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "node is not allowed",
            }),
        )
            .into_response(),
    }
}

async fn agent_enroll(
    State(state): State<AppState>,
    Json(request): Json<AgentEnrollmentRequest>,
) -> impl IntoResponse {
    match state.node_enrollment.enroll(request) {
        Ok(response) => {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::NodeEnrolled,
                actor_email: None,
                message: format!("node {} enrolled", response.node_id),
            });
            (StatusCode::CREATED, Json(response)).into_response()
        }
        Err(
            EnrollmentError::InvalidToken
            | EnrollmentError::TokenUsed
            | EnrollmentError::TokenExpired,
        ) => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid enrollment token",
            }),
        )
            .into_response(),
    }
}

async fn agent_heartbeat(
    State(state): State<AppState>,
    Json(request): Json<AgentHeartbeatRequest>,
) -> impl IntoResponse {
    match state.node_enrollment.heartbeat(request) {
        Ok(node) => Json(AgentHeartbeatResponse {
            accepted: true,
            node,
        })
        .into_response(),
        Err(NodeConnectionError::UnknownNode | NodeConnectionError::InvalidCredential) => (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "invalid node credential",
            }),
        )
            .into_response(),
        Err(NodeConnectionError::Revoked) => (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse {
                error: "node revoked",
            }),
        )
            .into_response(),
    }
}

async fn require_auth_middleware(
    State(auth): State<AuthService>,
    mut request: Request,
    next: Next,
) -> Response {
    let Some(token) = session_token_from_headers(request.headers()) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                error: "missing auth session",
            }),
        )
            .into_response();
    };

    let user = match auth.current_user(token) {
        Ok(Some(user)) => user,
        Ok(None) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse {
                    error: "invalid auth session",
                }),
            )
                .into_response();
        }
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "auth service unavailable",
                }),
            )
                .into_response();
        }
    };

    request.extensions_mut().insert(AuthenticatedUser(user));
    next.run(request).await
}

async fn terminal_websocket(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let user = match authorize_terminal_request(&state.auth, &headers) {
        Ok(user) => user,
        Err(error) => {
            let status = error.status_code();
            state.audit.record(AuditEventInput {
                kind: if error == TerminalAuthorizationError::StepUpMfaRequired {
                    AuditEventKind::UserMfaChallenge
                } else {
                    AuditEventKind::TerminalFailed
                },
                actor_email: None,
                message: error.message().to_owned(),
            });
            return (
                status,
                Json(ErrorResponse {
                    error: error.message(),
                }),
            )
                .into_response();
        }
    };

    ws.on_upgrade(move |socket| handle_terminal_socket(socket, state, user.email))
        .into_response()
}

fn authorize_terminal_request(
    auth: &AuthService,
    headers: &HeaderMap,
) -> Result<User, TerminalAuthorizationError> {
    let token =
        session_token_from_headers(headers).ok_or(TerminalAuthorizationError::Unauthorized)?;
    let user = match auth.current_user(token) {
        Ok(Some(user)) => user,
        Ok(None) => return Err(TerminalAuthorizationError::Unauthorized),
        Err(_) => return Err(TerminalAuthorizationError::Internal),
    };

    if !auth.can_open_terminal(&user) {
        return Err(TerminalAuthorizationError::Forbidden);
    }
    match auth.can_open_terminal_with_session(&user, token) {
        Ok(true) => {}
        Ok(false) if auth.terminal_step_up_policy_enabled() => {
            return Err(TerminalAuthorizationError::StepUpMfaRequired);
        }
        Ok(false) => return Err(TerminalAuthorizationError::Forbidden),
        Err(_) => return Err(TerminalAuthorizationError::Internal),
    }

    Ok(user)
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum TerminalAuthorizationError {
    Unauthorized,
    Forbidden,
    StepUpMfaRequired,
    Internal,
}

impl TerminalAuthorizationError {
    const fn status_code(self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden | Self::StepUpMfaRequired => StatusCode::FORBIDDEN,
            Self::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::Unauthorized => "terminal websocket authorization failed",
            Self::Forbidden => "terminal access is forbidden",
            Self::StepUpMfaRequired => "step-up MFA is required before opening a terminal",
            Self::Internal => "terminal authorization service unavailable",
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_terminal_socket(mut socket: WebSocket, state: AppState, actor_email: String) {
    let Some(handshake) = receive_start_message(&mut socket).await else {
        return;
    };

    let start = match handshake {
        TerminalHandshake::Start(start) => start,
        TerminalHandshake::Reattach {
            session_id,
            reconnect_token,
        } => {
            handle_local_terminal_reattach(socket, state, actor_email, session_id, reconnect_token)
                .await;
            return;
        }
    };

    let initial_size = terminal_size_from_protocol(start.initial_size);
    let session_id = next_session_id();

    if let Some(node_id) = start.node_id {
        handle_remote_terminal_socket(
            socket,
            state,
            actor_email,
            node_id,
            session_id,
            start.initial_size,
        )
        .await;
        return;
    }

    let session = match LocalPtySession::spawn_default_shell(initial_size) {
        Ok(session) => Arc::new(session),
        Err(error) => {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::TerminalFailed,
                actor_email: Some(actor_email),
                message: format!("terminal spawn failed: {error}"),
            });
            let _ = send_server_message(
                &mut socket,
                TerminalServerMessage::Error {
                    session_id: Some(session_id),
                    error: protocol_error(TerminalErrorCode::TerminalUnavailable, error),
                },
            )
            .await;
            return;
        }
    };

    let output_tx = match state.sessions.insert(
        session_id.clone(),
        Arc::clone(&session),
        start.initial_size,
        state.terminal_config,
        actor_email.clone(),
        None,
    ) {
        Ok(tx) => tx,
        Err(limit_error) => {
            state.audit.record(AuditEventInput {
                kind: AuditEventKind::TerminalFailed,
                actor_email: Some(actor_email),
                message: limit_error.message().to_owned(),
            });
            let _ = session.close();
            let _ = send_server_message(
                &mut socket,
                TerminalServerMessage::Error {
                    session_id: Some(session_id),
                    error: protocol_error_text(
                        TerminalErrorCode::TerminalUnavailable,
                        limit_error.message(),
                    ),
                },
            )
            .await;
            return;
        }
    };

    if send_server_message(
        &mut socket,
        TerminalServerMessage::Started {
            session_id: session_id.clone(),
            node_id: start.node_id,
            size: start.initial_size,
            reconnect_token: state.sessions.reconnect_token(&session_id),
        },
    )
    .await
    .is_err()
    {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalFailed,
            actor_email: Some(actor_email),
            message: "failed to send terminal started message".to_owned(),
        });
        let _ = session.close();
        return;
    }

    state.audit.record(AuditEventInput {
        kind: AuditEventKind::TerminalOpened,
        actor_email: Some(actor_email.clone()),
        message: format!("terminal session {} opened", session_id.0),
    });

    state
        .sessions
        .set_state(&session_id, TerminalSessionState::Active);

    let (mut sender, mut receiver) = socket.split();
    let mut output_rx = output_tx.subscribe();
    let output_session = Arc::clone(&session);
    let output_session_id = session_id.clone();

    let output_reader = task::spawn_blocking(move || {
        read_pty_output(output_session, output_session_id, output_tx);
    });

    let mut idle_check = tokio::time::interval(IDLE_CHECK_INTERVAL);
    let mut terminal_failed = false;

    loop {
        tokio::select! {
            output = output_rx.recv() => {
                let output = match output {
                    Ok(output) => output,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                state.sessions.touch(&session_id);
                let is_terminal_exit = matches!(output, TerminalServerMessage::Exited { .. });
                if is_terminal_exit {
                    state.sessions.set_state(&session_id, TerminalSessionState::Closed);
                }
                if let TerminalServerMessage::Error { error, .. } = &output {
                    terminal_failed = true;
                    state.audit.record(AuditEventInput {
                        kind: AuditEventKind::TerminalFailed,
                        actor_email: Some(actor_email.clone()),
                        message: format!("terminal stream error: {}", error.message),
                    });
                }
                if send_split_server_message(&mut sender, output).await.is_err() {
                    state.sessions.detach(&session_id);
                    schedule_detached_terminal_cleanup(
                        state.sessions.clone(),
                        session_id.clone(),
                        state.terminal_config.disconnect_grace,
                    );
                    break;
                }
                if is_terminal_exit {
                    break;
                }
            }
            incoming = receiver.next() => {
                if let Some(Ok(message)) = incoming {
                    state.sessions.touch(&session_id);
                    if !handle_client_frame(&state.sessions, &session, &session_id, message, &mut sender).await {
                        schedule_cleanup_if_detached(&state, &session_id);
                        break;
                    }
                } else {
                    state.sessions.detach(&session_id);
                    schedule_cleanup_if_detached(&state, &session_id);
                    break;
                }
            }
            _ = idle_check.tick() => {
                if state.sessions.is_idle(&session_id, state.terminal_config.idle_timeout) {
                    state.sessions.set_state(&session_id, TerminalSessionState::Closing);
                    let _ = send_split_server_message(
                        &mut sender,
                        TerminalServerMessage::Error {
                            session_id: Some(session_id.clone()),
                            error: protocol_error_text(
                                TerminalErrorCode::TerminalUnavailable,
                                "terminal session idle timeout reached",
                            ),
                        },
                    )
                    .await;
                    break;
                } else if state
                    .sessions
                    .is_exceeded_max_duration(&session_id, state.terminal_config.max_duration)
                {
                    state.sessions.set_state(&session_id, TerminalSessionState::Closing);
                    let _ = send_split_server_message(
                        &mut sender,
                        TerminalServerMessage::Error {
                            session_id: Some(session_id.clone()),
                            error: protocol_error_text(
                                TerminalErrorCode::TerminalUnavailable,
                                "terminal session exceeded maximum allowed duration",
                            ),
                        },
                    )
                    .await;
                    break;
                }
            }
        }
    }

    if matches!(
        state.sessions.state(&session_id),
        Some(TerminalSessionState::Detached)
    ) {
        let _ = tokio::time::timeout(READ_SHUTDOWN_GRACE, output_reader).await;
        return;
    }
    state
        .sessions
        .set_state(&session_id, TerminalSessionState::Closing);
    state.sessions.remove(&session_id);
    let _ = tokio::time::timeout(READ_SHUTDOWN_GRACE, output_reader).await;

    if !terminal_failed {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalClosed,
            actor_email: Some(actor_email),
            message: format!("terminal session {} closed", session_id.0),
        });
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_local_terminal_reattach(
    socket: WebSocket,
    state: AppState,
    actor_email: String,
    session_id: TerminalSessionId,
    reconnect_token: TerminalReconnectToken,
) {
    let Some((session, mut output_rx, size, next_reconnect_token)) =
        state.sessions.reattach(&session_id, &reconnect_token)
    else {
        let mut socket = socket;
        let _ = send_server_message(
            &mut socket,
            TerminalServerMessage::Error {
                session_id: Some(session_id),
                error: protocol_error_text(
                    TerminalErrorCode::SessionNotFound,
                    "detached terminal session was not found",
                ),
            },
        )
        .await;
        return;
    };

    let (mut sender, mut receiver) = socket.split();
    state
        .sessions
        .set_state(&session_id, TerminalSessionState::Reattached);
    let _ = send_split_server_message(
        &mut sender,
        TerminalServerMessage::Reattached {
            session_id: session_id.clone(),
            node_id: None,
            size,
            reconnect_token: Some(next_reconnect_token),
        },
    )
    .await;
    state
        .sessions
        .set_state(&session_id, TerminalSessionState::Active);

    let mut idle_check = tokio::time::interval(IDLE_CHECK_INTERVAL);
    let mut terminal_failed = false;

    loop {
        tokio::select! {
            output = output_rx.recv() => {
                let output = match output {
                    Ok(output) => output,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                };
                state.sessions.touch(&session_id);
                let is_terminal_exit = matches!(output, TerminalServerMessage::Exited { .. });
                if is_terminal_exit {
                    state.sessions.set_state(&session_id, TerminalSessionState::Closed);
                }
                if let TerminalServerMessage::Error { error, .. } = &output {
                    terminal_failed = true;
                    state.audit.record(AuditEventInput {
                        kind: AuditEventKind::TerminalFailed,
                        actor_email: Some(actor_email.clone()),
                        message: format!("terminal stream error: {}", error.message),
                    });
                }
                if send_split_server_message(&mut sender, output).await.is_err() {
                    state.sessions.detach(&session_id);
                    schedule_detached_terminal_cleanup(
                        state.sessions.clone(),
                        session_id.clone(),
                        state.terminal_config.disconnect_grace,
                    );
                    break;
                }
                if is_terminal_exit {
                    break;
                }
            }
            incoming = receiver.next() => {
                if let Some(Ok(message)) = incoming {
                    state.sessions.touch(&session_id);
                    if !handle_client_frame(&state.sessions, &session, &session_id, message, &mut sender).await {
                        schedule_cleanup_if_detached(&state, &session_id);
                        break;
                    }
                } else {
                    state.sessions.detach(&session_id);
                    schedule_cleanup_if_detached(&state, &session_id);
                    break;
                }
            }
            _ = idle_check.tick() => {
                if state.sessions.is_idle(&session_id, state.terminal_config.idle_timeout) {
                    state.sessions.set_state(&session_id, TerminalSessionState::Closing);
                    let _ = send_split_server_message(
                        &mut sender,
                        TerminalServerMessage::Error {
                            session_id: Some(session_id.clone()),
                            error: protocol_error_text(
                                TerminalErrorCode::TerminalUnavailable,
                                "terminal session idle timeout reached",
                            ),
                        },
                    )
                    .await;
                    break;
                } else if state
                    .sessions
                    .is_exceeded_max_duration(&session_id, state.terminal_config.max_duration)
                {
                    state.sessions.set_state(&session_id, TerminalSessionState::Closing);
                    let _ = send_split_server_message(
                        &mut sender,
                        TerminalServerMessage::Error {
                            session_id: Some(session_id.clone()),
                            error: protocol_error_text(
                                TerminalErrorCode::TerminalUnavailable,
                                "terminal session exceeded maximum allowed duration",
                            ),
                        },
                    )
                    .await;
                    break;
                }
            }
        }
    }

    if matches!(
        state.sessions.state(&session_id),
        Some(TerminalSessionState::Detached)
    ) {
        return;
    }

    state
        .sessions
        .set_state(&session_id, TerminalSessionState::Closing);
    state.sessions.remove(&session_id);

    if !terminal_failed {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalClosed,
            actor_email: Some(actor_email),
            message: format!("terminal session {} closed", session_id.0),
        });
    }
}

fn schedule_detached_terminal_cleanup(
    registry: TerminalSessionRegistry,
    session_id: TerminalSessionId,
    grace: Duration,
) {
    tokio::spawn(async move {
        tokio::time::sleep(grace).await;
        let _ = registry.remove_if_detached(&session_id);
    });
}

fn schedule_cleanup_if_detached(state: &AppState, session_id: &TerminalSessionId) {
    if !matches!(
        state.sessions.state(session_id),
        Some(TerminalSessionState::Detached)
    ) {
        return;
    }
    schedule_detached_terminal_cleanup(
        state.sessions.clone(),
        session_id.clone(),
        state.terminal_config.disconnect_grace,
    );
}

fn spawn_session_cleanup_worker(
    sessions: TerminalSessionRegistry,
    config: TerminalSessionConfig,
    audit: AuditLog,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CLEANUP_INTERVAL);
        interval.tick().await;
        loop {
            interval.tick().await;
            let expired = sessions.drain_exceeded_max_duration(config.max_duration);
            for (session_id, actor_email, output_tx, pty_session) in expired {
                let _ = output_tx.send(TerminalServerMessage::Error {
                    session_id: Some(session_id.clone()),
                    error: protocol_error_text(
                        TerminalErrorCode::TerminalUnavailable,
                        "terminal session exceeded maximum allowed duration",
                    ),
                });
                let _ = pty_session.close();
                audit.record(AuditEventInput {
                    kind: AuditEventKind::TerminalClosed,
                    actor_email: Some(actor_email),
                    message: format!(
                        "terminal session {} forcibly closed: exceeded max duration",
                        session_id.0
                    ),
                });
            }
        }
    });
}

struct StartTerminal {
    node_id: Option<sunbolt_protocol::NodeId>,
    initial_size: ProtocolTerminalSize,
}

enum TerminalHandshake {
    Start(StartTerminal),
    Reattach {
        session_id: TerminalSessionId,
        reconnect_token: TerminalReconnectToken,
    },
}

async fn receive_start_message(socket: &mut WebSocket) -> Option<TerminalHandshake> {
    match socket.recv().await {
        Some(Ok(message)) => match parse_client_message(message) {
            Ok(TerminalClientMessage::Start {
                node_id,
                initial_size,
            }) => Some(TerminalHandshake::Start(StartTerminal {
                node_id,
                initial_size,
            })),
            Ok(TerminalClientMessage::Reattach {
                session_id,
                reconnect_token,
            }) => Some(TerminalHandshake::Reattach {
                session_id,
                reconnect_token,
            }),
            Ok(_) => {
                let _ = send_server_message(
                    socket,
                    TerminalServerMessage::Error {
                        session_id: None,
                        error: protocol_error_text(
                            TerminalErrorCode::InvalidMessage,
                            "first terminal message must be start or reattach",
                        ),
                    },
                )
                .await;
                None
            }
            Err(error) => {
                let _ = send_server_message(
                    socket,
                    TerminalServerMessage::Error {
                        session_id: None,
                        error,
                    },
                )
                .await;
                None
            }
        },
        Some(Err(_)) | None => None,
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_remote_terminal_socket(
    mut socket: WebSocket,
    state: AppState,
    actor_email: String,
    node_id: NodeId,
    session_id: TerminalSessionId,
    initial_size: ProtocolTerminalSize,
) {
    let node_id_text = node_id.0.clone();
    if !state.node_enrollment.node_is_online(&node_id_text) {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalFailed,
            actor_email: Some(actor_email),
            message: format!("remote terminal requested for unavailable node {node_id_text}"),
        });
        let _ = send_server_message(
            &mut socket,
            TerminalServerMessage::Error {
                session_id: Some(session_id),
                error: protocol_error_text(
                    TerminalErrorCode::TerminalUnavailable,
                    "agent node is not online",
                ),
            },
        )
        .await;
        return;
    }

    let Some(connection) = state.agent_connections.connection(&node_id_text) else {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalFailed,
            actor_email: Some(actor_email),
            message: format!("remote terminal requested without agent channel {node_id_text}"),
        });
        let _ = send_server_message(
            &mut socket,
            TerminalServerMessage::Error {
                session_id: Some(session_id),
                error: protocol_error_text(
                    TerminalErrorCode::TerminalUnavailable,
                    "agent connection is not active",
                ),
            },
        )
        .await;
        return;
    };

    if connection
        .command_tx
        .send(AgentTerminalCommand::StartTerminal {
            session_id: session_id.clone(),
            size: initial_size,
        })
        .await
        .is_err()
    {
        state.agent_connections.disconnect(&node_id_text);
        let _ = send_server_message(
            &mut socket,
            TerminalServerMessage::Error {
                session_id: Some(session_id),
                error: protocol_error_text(
                    TerminalErrorCode::TerminalUnavailable,
                    "agent connection dropped",
                ),
            },
        )
        .await;
        return;
    }

    let (mut sender, mut receiver) = socket.split();
    let mut event_rx = connection.event_rx.lock().await;
    let mut terminal_opened = false;
    let mut terminal_failed = false;

    loop {
        tokio::select! {
            event = event_rx.recv() => {
                let Some(event) = event else {
                    terminal_failed = true;
                    let _ = send_split_server_message(
                        &mut sender,
                        TerminalServerMessage::Error {
                            session_id: Some(session_id.clone()),
                            error: protocol_error_text(
                                TerminalErrorCode::TerminalUnavailable,
                                "agent disconnected during terminal session",
                            ),
                        },
                    )
                    .await;
                    break;
                };
                let message = agent_event_to_browser_message(event, &node_id);
                if matches!(message, TerminalServerMessage::Started { .. }) {
                    terminal_opened = true;
                    state.audit.record(AuditEventInput {
                        kind: AuditEventKind::TerminalOpened,
                        actor_email: Some(actor_email.clone()),
                        message: format!("remote terminal session {} opened on {node_id_text}", session_id.0),
                    });
                }
                if matches!(message, TerminalServerMessage::Error { .. }) {
                    terminal_failed = true;
                }
                let is_terminal_exit = matches!(message, TerminalServerMessage::Exited { .. });
                if send_split_server_message(&mut sender, message).await.is_err() {
                    break;
                }
                if is_terminal_exit {
                    break;
                }
            }
            incoming = receiver.next() => {
                match incoming {
                    Some(Ok(message)) => {
                        if !handle_remote_client_frame(&connection.command_tx, &session_id, message, &mut sender).await {
                            break;
                        }
                    }
                    Some(Err(_)) | None => break,
                }
            }
        }
    }

    let _ = connection
        .command_tx
        .send(AgentTerminalCommand::CloseTerminal {
            session_id: session_id.clone(),
        })
        .await;
    if terminal_failed {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalFailed,
            actor_email: Some(actor_email),
            message: format!("remote terminal session {} failed", session_id.0),
        });
    } else if terminal_opened {
        state.audit.record(AuditEventInput {
            kind: AuditEventKind::TerminalClosed,
            actor_email: Some(actor_email),
            message: format!("remote terminal session {} closed", session_id.0),
        });
    }
}

async fn handle_remote_client_frame(
    command_tx: &mpsc::Sender<AgentTerminalCommand>,
    active_session_id: &TerminalSessionId,
    message: Message,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> bool {
    let message = match parse_client_message(message) {
        Ok(message) => message,
        Err(error) => {
            let _ = send_split_server_message(
                sender,
                TerminalServerMessage::Error {
                    session_id: Some(active_session_id.clone()),
                    error,
                },
            )
            .await;
            return true;
        }
    };

    let command = match message {
        TerminalClientMessage::Input { session_id, data } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            AgentTerminalCommand::WriteInput { session_id, data }
        }
        TerminalClientMessage::Resize { session_id, size } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            AgentTerminalCommand::ResizeTerminal { session_id, size }
        }
        TerminalClientMessage::Close { session_id } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            let _ = command_tx
                .send(AgentTerminalCommand::CloseTerminal { session_id })
                .await;
            return false;
        }
        TerminalClientMessage::Detach { session_id } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            let _ =
                send_split_server_message(sender, TerminalServerMessage::Detached { session_id })
                    .await;
            return false;
        }
        TerminalClientMessage::Reattach { session_id, .. } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            let _ = send_split_server_message(
                sender,
                TerminalServerMessage::Error {
                    session_id: Some(session_id),
                    error: protocol_error_text(
                        TerminalErrorCode::InvalidMessage,
                        "remote terminal reattach is not available yet",
                    ),
                },
            )
            .await;
            return true;
        }
        TerminalClientMessage::Ping { nonce } => {
            let _ = send_split_server_message(sender, TerminalServerMessage::Pong { nonce }).await;
            return true;
        }
        TerminalClientMessage::Start { .. } => {
            let _ = send_split_server_message(
                sender,
                TerminalServerMessage::Error {
                    session_id: Some(active_session_id.clone()),
                    error: protocol_error_text(
                        TerminalErrorCode::InvalidMessage,
                        "terminal session is already started",
                    ),
                },
            )
            .await;
            return true;
        }
    };

    if command_tx.send(command).await.is_err() {
        let _ = send_split_server_message(
            sender,
            TerminalServerMessage::Error {
                session_id: Some(active_session_id.clone()),
                error: protocol_error_text(
                    TerminalErrorCode::TerminalUnavailable,
                    "agent connection dropped",
                ),
            },
        )
        .await;
        return false;
    }

    true
}

fn agent_event_to_browser_message(
    event: AgentTerminalEvent,
    node_id: &NodeId,
) -> TerminalServerMessage {
    match event {
        AgentTerminalEvent::TerminalStarted { session_id, size } => {
            TerminalServerMessage::Started {
                session_id,
                node_id: Some(node_id.clone()),
                size,
                reconnect_token: None,
            }
        }
        AgentTerminalEvent::TerminalOutput { session_id, data } => {
            TerminalServerMessage::Output { session_id, data }
        }
        AgentTerminalEvent::TerminalExited { session_id, exit } => {
            TerminalServerMessage::Exited { session_id, exit }
        }
        AgentTerminalEvent::TerminalError { session_id, error } => TerminalServerMessage::Error {
            session_id: Some(session_id),
            error,
        },
    }
}

async fn handle_client_frame(
    registry: &TerminalSessionRegistry,
    session: &LocalPtySession,
    active_session_id: &TerminalSessionId,
    message: Message,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> bool {
    let message = match parse_client_message(message) {
        Ok(message) => message,
        Err(error) => {
            let _ = send_split_server_message(
                sender,
                TerminalServerMessage::Error {
                    session_id: Some(active_session_id.clone()),
                    error,
                },
            )
            .await;
            return true;
        }
    };

    match message {
        TerminalClientMessage::Input { session_id, data } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            if let Err(error) = session.write_input(data.as_bytes()) {
                let _ = send_split_server_message(
                    sender,
                    TerminalServerMessage::Error {
                        session_id: Some(active_session_id.clone()),
                        error: protocol_error(TerminalErrorCode::TerminalUnavailable, error),
                    },
                )
                .await;
            }
            true
        }
        TerminalClientMessage::Resize { session_id, size } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            registry.set_size(active_session_id, size);
            if let Err(error) = session.resize(terminal_size_from_protocol(size)) {
                let _ = send_split_server_message(
                    sender,
                    TerminalServerMessage::Error {
                        session_id: Some(active_session_id.clone()),
                        error: protocol_error(TerminalErrorCode::TerminalUnavailable, error),
                    },
                )
                .await;
            }
            true
        }
        TerminalClientMessage::Close { session_id } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            registry.set_state(active_session_id, TerminalSessionState::Closing);
            let _ = session.close();
            let _ = send_split_server_message(
                sender,
                TerminalServerMessage::Exited {
                    session_id: active_session_id.clone(),
                    exit: TerminalExit { status: None },
                },
            )
            .await;
            false
        }
        TerminalClientMessage::Detach { session_id } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            detach_local_terminal(registry, active_session_id, sender).await
        }
        TerminalClientMessage::Reattach { session_id, .. } => {
            if !session_id_matches(&session_id, active_session_id, sender).await {
                return true;
            }
            reattach_local_terminal(registry, active_session_id, sender).await
        }
        TerminalClientMessage::Ping { nonce } => {
            let _ = send_split_server_message(sender, TerminalServerMessage::Pong { nonce }).await;
            true
        }
        TerminalClientMessage::Start { .. } => {
            let _ = send_split_server_message(
                sender,
                TerminalServerMessage::Error {
                    session_id: Some(active_session_id.clone()),
                    error: protocol_error_text(
                        TerminalErrorCode::InvalidMessage,
                        "terminal session is already started",
                    ),
                },
            )
            .await;
            true
        }
    }
}

async fn detach_local_terminal(
    registry: &TerminalSessionRegistry,
    active_session_id: &TerminalSessionId,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> bool {
    registry.set_state(active_session_id, TerminalSessionState::Detached);
    let _ = send_split_server_message(
        sender,
        TerminalServerMessage::Detached {
            session_id: active_session_id.clone(),
        },
    )
    .await;
    false
}

async fn reattach_local_terminal(
    registry: &TerminalSessionRegistry,
    active_session_id: &TerminalSessionId,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> bool {
    registry.set_state(active_session_id, TerminalSessionState::Reconnecting);
    registry.set_state(active_session_id, TerminalSessionState::Reattached);
    let _ = send_split_server_message(
        sender,
        TerminalServerMessage::Reattached {
            session_id: active_session_id.clone(),
            node_id: None,
            size: ProtocolTerminalSize { cols: 80, rows: 24 },
            reconnect_token: registry.reconnect_token(active_session_id),
        },
    )
    .await;
    registry.set_state(active_session_id, TerminalSessionState::Active);
    true
}

async fn session_id_matches(
    received: &TerminalSessionId,
    active: &TerminalSessionId,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> bool {
    if received == active {
        return true;
    }

    let _ = send_split_server_message(
        sender,
        TerminalServerMessage::Error {
            session_id: Some(active.clone()),
            error: protocol_error_text(
                TerminalErrorCode::SessionNotFound,
                "terminal session id does not match this connection",
            ),
        },
    )
    .await;

    false
}

#[allow(clippy::needless_pass_by_value)]
fn read_pty_output(
    session: Arc<LocalPtySession>,
    session_id: TerminalSessionId,
    output_tx: broadcast::Sender<TerminalServerMessage>,
) {
    let mut buffer = [0_u8; OUTPUT_BUFFER_SIZE];

    loop {
        if session.is_closed() {
            break;
        }

        match session.read_output(&mut buffer) {
            Ok(0) | Err(TerminalError::Closed) => {
                break;
            }
            Ok(read) => {
                let data = String::from_utf8_lossy(&buffer[..read]).into_owned();
                // The bounded channel is the temporary backpressure strategy:
                // this blocking send slows PTY reads when the WebSocket writer
                // cannot keep up, instead of buffering terminal output without
                // limit.
                let _ = output_tx.send(TerminalServerMessage::Output {
                    session_id: session_id.clone(),
                    data,
                });
            }
            Err(error) => {
                if let Ok(Some(exit)) = session.try_wait_exit() {
                    let _ = output_tx.send(exit_message(session_id.clone(), exit));
                } else {
                    let _ = output_tx.send(TerminalServerMessage::Error {
                        session_id: Some(session_id.clone()),
                        error: protocol_error(TerminalErrorCode::TerminalUnavailable, error),
                    });
                }
                break;
            }
        }
    }

    if let Ok(Some(exit)) = session.wait_exit() {
        let _ = output_tx.send(exit_message(session_id, exit));
    }
}

fn exit_message(session_id: TerminalSessionId, exit: TerminalExitStatus) -> TerminalServerMessage {
    TerminalServerMessage::Exited {
        session_id,
        exit: TerminalExit { status: exit.code },
    }
}

fn parse_client_message(message: Message) -> Result<TerminalClientMessage, ProtocolTerminalError> {
    match message {
        Message::Text(text) => serde_json::from_str(&text).map_err(|error| {
            protocol_error_text(
                TerminalErrorCode::InvalidMessage,
                format!("invalid terminal message JSON: {error}"),
            )
        }),
        Message::Binary(_) => Err(protocol_error_text(
            TerminalErrorCode::InvalidMessage,
            "binary terminal messages are not supported",
        )),
        Message::Close(_) => Err(protocol_error_text(
            TerminalErrorCode::InvalidMessage,
            "terminal websocket closed",
        )),
        Message::Ping(_) | Message::Pong(_) => Err(protocol_error_text(
            TerminalErrorCode::InvalidMessage,
            "websocket control frames are not terminal protocol messages",
        )),
    }
}

async fn send_server_message(
    socket: &mut WebSocket,
    message: TerminalServerMessage,
) -> Result<(), axum::Error> {
    socket
        .send(Message::Text(serialize_server_message(&message).into()))
        .await
}

async fn send_split_server_message(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: TerminalServerMessage,
) -> Result<(), axum::Error> {
    sender
        .send(Message::Text(serialize_server_message(&message).into()))
        .await
}

fn serialize_server_message(message: &TerminalServerMessage) -> String {
    serde_json::to_string(message).expect("terminal server messages should serialize")
}

fn protocol_error(code: TerminalErrorCode, error: impl std::error::Error) -> ProtocolTerminalError {
    protocol_error_text(code, error.to_string())
}

fn protocol_error_text(
    code: TerminalErrorCode,
    message: impl Into<String>,
) -> ProtocolTerminalError {
    ProtocolTerminalError {
        code,
        message: message.into(),
    }
}

fn terminal_size_from_protocol(size: ProtocolTerminalSize) -> TerminalSize {
    let cols = size.cols.max(1);
    let rows = size.rows.max(1);
    TerminalSize { cols, rows }
}

fn next_session_id() -> TerminalSessionId {
    let id = NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed);
    TerminalSessionId(format!("local-{id}"))
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum EnrollmentError {
    InvalidToken,
    TokenUsed,
    TokenExpired,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum NodeConnectionError {
    UnknownNode,
    InvalidCredential,
    Revoked,
}

fn random_token() -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut bytes = [0_u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);

    let mut token = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        token.push(char::from(HEX[usize::from(byte >> 4)]));
        token.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    token
}

fn token_hash(token: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    token.hash(&mut hasher);
    hasher.finish()
}

fn credential_fingerprint(request: &AgentEnrollmentRequest) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    request.node_name.hash(&mut hasher);
    request.hostname.hash(&mut hasher);
    request.os.hash(&mut hasher);
    request.architecture.hash(&mut hasher);
    request.agent_version.hash(&mut hasher);
    format!("dev-{:016x}", hasher.finish())
}

fn session_token_from_headers(headers: &HeaderMap) -> Option<&str> {
    let cookie_header = headers.get(COOKIE)?.to_str().ok()?;

    for cookie in cookie_header.split(';') {
        let cookie = cookie.trim();
        let (name, value) = cookie.split_once('=')?;
        if name == SESSION_COOKIE_NAME {
            return Some(value);
        }
    }

    None
}

fn env_usize(name: &str) -> Option<usize> {
    env::var(name).ok()?.parse().ok()
}

fn env_duration_secs(name: &str) -> Option<Duration> {
    env_usize(name).and_then(|seconds| u64::try_from(seconds).ok().map(Duration::from_secs))
}

#[cfg(test)]
mod tests {
    use super::{
        authorize_terminal_request, build_router, component_name, exit_message,
        parse_client_message, terminal_size_from_protocol, AgentConnectionRegistry, AppState,
        NodeEnrollmentRegistry, SessionLimitError, TerminalAuthorizationError,
        TerminalSessionConfig, TerminalSessionRegistry, ACCESS_HISTORY_PATH, AGENT_ENROLL_PATH,
        AGENT_HEARTBEAT_PATH, AUDIT_LOGS_PATH, AUTH_LOGIN_PATH, AUTH_LOGOUT_PATH, AUTH_ME_PATH,
        AUTH_MFA_STEP_UP_PATH, ENROLLMENT_TOKENS_PATH, NODES_PATH, SESSION_COOKIE_NAME,
        TERMINAL_WS_PATH,
    };
    use axum::{
        body::Body,
        extract::ws::Message,
        http::{header, HeaderMap, Method, Request, StatusCode},
    };
    use serde_json::{json, Value};
    use std::{process::Command, sync::Arc, time::Duration};
    use sunbolt_auth::{AuthConfig, AuthService};
    use sunbolt_protocol::{
        AgentTerminalCommand, AgentTerminalEvent, TerminalClientMessage, TerminalError,
        TerminalErrorCode, TerminalReconnectToken, TerminalServerMessage, TerminalSessionId,
        TerminalSize,
    };
    use sunbolt_terminal::{
        LocalPtySession, TerminalExitStatus, TerminalSessionState, TerminalSize as PtyTerminalSize,
    };
    use tokio::sync::mpsc;
    use tower::ServiceExt;

    #[test]
    fn component_name_mentions_control_plane() {
        assert_eq!(component_name(), "Sunbolt control plane");
    }

    #[test]
    fn terminal_size_from_protocol_clamps_zero_dimensions() {
        let size = terminal_size_from_protocol(TerminalSize { cols: 0, rows: 0 });

        assert_eq!(size.cols, 1);
        assert_eq!(size.rows, 1);
    }

    #[test]
    fn default_terminal_session_config_is_bounded() {
        let config = TerminalSessionConfig::from_env();

        assert!(config.max_sessions > 0);
        assert!(config.max_sessions_per_user > 0);
        assert!(config.max_sessions_per_node > 0);
        assert!(config.idle_timeout >= Duration::from_secs(60));
        assert!(config.max_duration >= Duration::from_secs(60 * 60));
    }

    #[test]
    fn terminal_session_registry_tracks_state_and_cleanup() {
        let Some(shell) = test_shell() else {
            return;
        };

        let registry = TerminalSessionRegistry::default();
        let session_id = TerminalSessionId("session-1".to_owned());
        let session = Arc::new(
            LocalPtySession::spawn_shell(shell, PtyTerminalSize::new(80, 24))
                .expect("test shell should spawn"),
        );

        let config = TerminalSessionConfig {
            max_sessions: 1,
            max_sessions_per_user: 5,
            max_sessions_per_node: 10,
            idle_timeout: Duration::from_secs(30 * 60),
            max_duration: Duration::from_secs(8 * 60 * 60),
            disconnect_grace: Duration::from_secs(30),
        };
        assert!(registry
            .insert(
                session_id.clone(),
                Arc::clone(&session),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "test@example.com".to_owned(),
                None,
            )
            .is_ok());
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.state(&session_id),
            Some(TerminalSessionState::Starting)
        );

        registry.set_state(&session_id, TerminalSessionState::Active);
        assert_eq!(
            registry.state(&session_id),
            Some(TerminalSessionState::Active)
        );
        registry.set_state(&session_id, TerminalSessionState::Detached);
        assert_eq!(
            registry.state(&session_id),
            Some(TerminalSessionState::Detached)
        );
        registry.set_state(&session_id, TerminalSessionState::Active);
        assert_eq!(
            registry.state(&session_id),
            Some(TerminalSessionState::Detached)
        );
        let reconnect_token = registry
            .reconnect_token(&session_id)
            .expect("reconnect token should be issued");
        assert!(registry
            .reattach(
                &session_id,
                &TerminalReconnectToken("wrong-token".to_owned())
            )
            .is_none());
        assert!(registry.reattach(&session_id, &reconnect_token).is_some());
        registry.set_state(&session_id, TerminalSessionState::Reattached);
        registry.set_state(&session_id, TerminalSessionState::Active);
        assert_eq!(
            registry.state(&session_id),
            Some(TerminalSessionState::Active)
        );

        assert!(registry
            .insert(
                TerminalSessionId("session-2".to_owned()),
                Arc::clone(&session),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "test@example.com".to_owned(),
                None,
            )
            .is_err());

        registry.remove(&session_id);
        assert_eq!(registry.len(), 0);
        assert!(session.is_closed());
    }

    #[test]
    fn per_user_session_limit_is_enforced() {
        let Some(shell) = test_shell() else {
            return;
        };

        let registry = TerminalSessionRegistry::default();
        let config = TerminalSessionConfig {
            max_sessions: 10,
            max_sessions_per_user: 2,
            max_sessions_per_node: 10,
            idle_timeout: Duration::from_secs(30 * 60),
            max_duration: Duration::from_secs(8 * 60 * 60),
            disconnect_grace: Duration::from_secs(30),
        };
        let spawn_session = || {
            Arc::new(
                LocalPtySession::spawn_shell(shell.clone(), PtyTerminalSize::new(80, 24))
                    .expect("test shell should spawn"),
            )
        };

        assert!(registry
            .insert(
                TerminalSessionId("s1".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "alice@example.com".to_owned(),
                None,
            )
            .is_ok());
        assert!(registry
            .insert(
                TerminalSessionId("s2".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "alice@example.com".to_owned(),
                None,
            )
            .is_ok());

        let err = registry
            .insert(
                TerminalSessionId("s3".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "alice@example.com".to_owned(),
                None,
            )
            .expect_err("user limit should be enforced");
        assert_eq!(err, SessionLimitError::PerUser);

        assert!(registry
            .insert(
                TerminalSessionId("s4".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "bob@example.com".to_owned(),
                None,
            )
            .is_ok());
    }

    #[test]
    fn per_node_session_limit_is_enforced() {
        let Some(shell) = test_shell() else {
            return;
        };

        let registry = TerminalSessionRegistry::default();
        let config = TerminalSessionConfig {
            max_sessions: 10,
            max_sessions_per_user: 10,
            max_sessions_per_node: 2,
            idle_timeout: Duration::from_secs(30 * 60),
            max_duration: Duration::from_secs(8 * 60 * 60),
            disconnect_grace: Duration::from_secs(30),
        };
        let spawn_session = || {
            Arc::new(
                LocalPtySession::spawn_shell(shell.clone(), PtyTerminalSize::new(80, 24))
                    .expect("test shell should spawn"),
            )
        };

        assert!(registry
            .insert(
                TerminalSessionId("s1".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "user1@example.com".to_owned(),
                Some("node-1".to_owned()),
            )
            .is_ok());
        assert!(registry
            .insert(
                TerminalSessionId("s2".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "user2@example.com".to_owned(),
                Some("node-1".to_owned()),
            )
            .is_ok());

        let err = registry
            .insert(
                TerminalSessionId("s3".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "user3@example.com".to_owned(),
                Some("node-1".to_owned()),
            )
            .expect_err("node limit should be enforced");
        assert_eq!(err, SessionLimitError::PerNode);

        assert!(registry
            .insert(
                TerminalSessionId("s4".to_owned()),
                spawn_session(),
                TerminalSize { cols: 80, rows: 24 },
                config,
                "user3@example.com".to_owned(),
                Some("node-2".to_owned()),
            )
            .is_ok());
    }

    #[test]
    fn cleanup_removes_sessions_exceeding_max_duration() {
        let Some(shell) = test_shell() else {
            return;
        };

        let registry = TerminalSessionRegistry::default();
        let config = TerminalSessionConfig {
            max_sessions: 10,
            max_sessions_per_user: 5,
            max_sessions_per_node: 10,
            idle_timeout: Duration::from_secs(30 * 60),
            max_duration: Duration::from_secs(8 * 60 * 60),
            disconnect_grace: Duration::from_secs(30),
        };
        let session = Arc::new(
            LocalPtySession::spawn_shell(shell, PtyTerminalSize::new(80, 24))
                .expect("test shell should spawn"),
        );
        assert!(registry
            .insert(
                TerminalSessionId("session-1".to_owned()),
                session,
                TerminalSize { cols: 80, rows: 24 },
                config,
                "test@example.com".to_owned(),
                None,
            )
            .is_ok());
        assert_eq!(registry.len(), 1);

        // Duration::ZERO means every session has exceeded max duration immediately
        let expired = registry.drain_exceeded_max_duration(Duration::ZERO);
        assert_eq!(expired.len(), 1);
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn exit_status_maps_to_protocol_message() {
        let message = exit_message(
            TerminalSessionId("session-1".to_owned()),
            TerminalExitStatus { code: Some(3) },
        );

        assert!(matches!(
            message,
            TerminalServerMessage::Exited {
                exit: sunbolt_protocol::TerminalExit { status: Some(3) },
                ..
            }
        ));
    }

    #[test]
    fn parse_client_message_rejects_invalid_json() {
        let error = parse_client_message(Message::Text("{".to_owned().into()))
            .expect_err("invalid JSON should be rejected");

        assert_eq!(
            error.code,
            sunbolt_protocol::TerminalErrorCode::InvalidMessage
        );
    }

    #[test]
    fn parse_client_message_accepts_start_message() {
        let message = parse_client_message(Message::Text(
            r#"{"type":"start","node_id":null,"initial_size":{"cols":80,"rows":24}}"#
                .to_owned()
                .into(),
        ))
        .expect("start message should parse");

        assert!(matches!(message, TerminalClientMessage::Start { .. }));
    }

    #[tokio::test]
    async fn terminal_route_requires_websocket_upgrade() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri(TERMINAL_WS_PATH)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn unknown_route_returns_not_found() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .uri("/missing")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn login_sets_session_cookie_and_returns_user() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_LOGIN_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": "admin@example.com",
                            "password": "admin-password"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::OK);
        let set_cookie = response
            .headers()
            .get(header::SET_COOKIE)
            .expect("set-cookie should be present")
            .to_str()
            .expect("cookie header should be utf-8");
        assert!(set_cookie.contains("sunbolt_session="));

        let body = axum::body::to_bytes(response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        assert_eq!(payload["user"]["email"], "admin@example.com");
    }

    #[tokio::test]
    async fn auth_me_requires_authentication() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(AUTH_ME_PATH)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn access_history_requires_authentication() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(ACCESS_HISTORY_PATH)
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn audit_logs_capture_login_and_logout_events() {
        let router = test_router();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;

        let _failed_login_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_LOGIN_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": "admin@example.com",
                            "password": "wrong-password"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        let _logout_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_LOGOUT_PATH)
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        let fresh_cookie =
            login_and_get_cookie(&router, "admin@example.com", "admin-password").await;
        let logs_response = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(AUDIT_LOGS_PATH)
                    .header(header::COOKIE, fresh_cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(logs_response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(logs_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        let events = payload["events"]
            .as_array()
            .expect("events should be a list");
        assert!(
            events
                .iter()
                .any(|event| event["kind"] == json!("UserLoginSuccess")),
            "expected login success event"
        );
        assert!(
            events
                .iter()
                .any(|event| event["kind"] == json!("UserLoginFailed")),
            "expected login failed event"
        );
        assert!(
            events
                .iter()
                .any(|event| event["kind"] == json!("UserLogout")),
            "expected logout event"
        );
    }

    #[tokio::test]
    async fn step_up_mfa_endpoint_records_recent_mfa_and_audit_events() {
        let router = test_router();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_MFA_STEP_UP_PATH)
                    .header(header::COOKIE, cookie.as_str())
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "factor_type": "totp"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::OK);

        let logs_response = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(AUDIT_LOGS_PATH)
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        let body = axum::body::to_bytes(logs_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        let events = payload["events"]
            .as_array()
            .expect("events should be a list");
        assert!(events
            .iter()
            .any(|event| event["kind"] == json!("UserMfaChallenge")));
        assert!(events
            .iter()
            .any(|event| event["kind"] == json!("UserMfaSuccess")));
    }

    #[tokio::test]
    async fn enrollment_token_requires_authentication() {
        let response = test_router()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(ENROLLMENT_TOKENS_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from("{}"))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn enrollment_token_registers_agent_once() {
        let router = test_router();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;
        let token_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(ENROLLMENT_TOKENS_PATH)
                    .header(header::COOKIE, cookie.as_str())
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"expires_in_secs": 300}).to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(token_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(token_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        let token = payload["token"].as_str().expect("token should be present");
        assert!(payload["enrollment_command"]
            .as_str()
            .expect("command should be present")
            .contains("SUNBOLT_AGENT_ENROLLMENT_TOKEN"));

        let enroll_body = json!({
            "token": token,
            "node_name": "node-a",
            "hostname": "host-a",
            "os": "linux",
            "architecture": "x86_64",
            "agent_version": "0.1.0"
        })
        .to_string();
        let enroll_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AGENT_ENROLL_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(enroll_body.clone()))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(enroll_response.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(enroll_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        assert_eq!(payload["node_id"], "node-1");

        let reused_response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AGENT_ENROLL_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(enroll_body))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(reused_response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn agent_heartbeat_marks_node_online_and_nodes_are_listed() {
        let router = test_router();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;
        let enrollment = enroll_test_agent(&router, &cookie).await;

        let heartbeat_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AGENT_HEARTBEAT_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "node_id": enrollment.node_id,
                            "credential_fingerprint": enrollment.credential_fingerprint,
                            "hostname": "host-a",
                            "os": "linux",
                            "architecture": "x86_64",
                            "agent_version": "0.1.0"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(heartbeat_response.status(), StatusCode::OK);

        let nodes_response = router
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(NODES_PATH)
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(nodes_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(nodes_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        assert_eq!(payload["nodes"][0]["status"], "online");
    }

    #[tokio::test]
    async fn node_details_and_revoke_are_authenticated() {
        let router = test_router();
        let cookie = login_and_get_cookie(&router, "admin@example.com", "admin-password").await;
        let enrollment = enroll_test_agent(&router, &cookie).await;

        let details_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(format!("{NODES_PATH}/{}", enrollment.node_id))
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(details_response.status(), StatusCode::OK);

        let revoke_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(format!("{NODES_PATH}/{}/revoke", enrollment.node_id))
                    .header(header::COOKIE, cookie.as_str())
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(revoke_response.status(), StatusCode::OK);

        let heartbeat_response = router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AGENT_HEARTBEAT_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "node_id": enrollment.node_id,
                            "credential_fingerprint": enrollment.credential_fingerprint,
                            "hostname": "host-a",
                            "os": "linux",
                            "architecture": "x86_64",
                            "agent_version": "0.1.0"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(heartbeat_response.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn agent_terminal_events_map_to_browser_messages() {
        let session_id = TerminalSessionId("remote-1".to_owned());
        let output = super::agent_event_to_browser_message(
            AgentTerminalEvent::TerminalOutput {
                session_id: session_id.clone(),
                data: "hello\n".to_owned(),
            },
            &sunbolt_protocol::NodeId("node-1".to_owned()),
        );

        assert_eq!(
            output,
            TerminalServerMessage::Output {
                session_id: session_id.clone(),
                data: "hello\n".to_owned(),
            }
        );

        let error = super::agent_event_to_browser_message(
            AgentTerminalEvent::TerminalError {
                session_id: session_id.clone(),
                error: TerminalError {
                    code: TerminalErrorCode::TerminalUnavailable,
                    message: "agent disconnected".to_owned(),
                },
            },
            &sunbolt_protocol::NodeId("node-1".to_owned()),
        );

        assert!(matches!(
            error,
            TerminalServerMessage::Error {
                session_id: Some(_),
                ..
            }
        ));
    }

    #[tokio::test]
    async fn agent_connection_registry_tracks_active_channel() {
        let registry = AgentConnectionRegistry::default();
        let (command_tx, mut command_rx) = mpsc::channel(1);
        let (_event_tx, event_rx) = mpsc::channel(1);
        registry.register("node-1", command_tx, event_rx);

        let connection = registry
            .connection("node-1")
            .expect("agent connection should be registered");
        connection
            .command_tx
            .send(AgentTerminalCommand::CloseTerminal {
                session_id: TerminalSessionId("remote-1".to_owned()),
            })
            .await
            .expect("command should send");

        assert_eq!(registry.len(), 1);
        assert!(matches!(
            command_rx.recv().await,
            Some(AgentTerminalCommand::CloseTerminal { .. })
        ));

        registry.disconnect("node-1");
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn viewer_cannot_open_terminal() {
        let auth = test_auth_service();
        let (_, token) = auth
            .login("viewer@example.com", "viewer-password")
            .expect("viewer should log in");

        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{SESSION_COOKIE_NAME}={token}")
                .parse()
                .expect("cookie header should parse"),
        );

        let error =
            authorize_terminal_request(&auth, &headers).expect_err("viewer should be forbidden");
        assert_eq!(error, TerminalAuthorizationError::Forbidden);
    }

    #[test]
    fn terminal_authorization_requires_session_cookie() {
        let headers = HeaderMap::new();
        let auth = test_auth_service();

        let error = authorize_terminal_request(&auth, &headers)
            .expect_err("missing auth should be rejected");
        assert_eq!(error, TerminalAuthorizationError::Unauthorized);
    }

    #[test]
    fn terminal_authorization_requires_recent_step_up_mfa() {
        let auth = test_auth_service();
        let (_, token) = auth
            .login("admin@example.com", "admin-password")
            .expect("admin should log in");
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("{SESSION_COOKIE_NAME}={token}")
                .parse()
                .expect("cookie header should parse"),
        );

        let error = authorize_terminal_request(&auth, &headers)
            .expect_err("missing step-up MFA should be rejected");
        assert_eq!(error, TerminalAuthorizationError::StepUpMfaRequired);

        auth.record_mfa_success(&token)
            .expect("MFA success should record");
        let user = authorize_terminal_request(&auth, &headers)
            .expect("recent step-up MFA should allow terminal");
        assert_eq!(user.email, "admin@example.com");
    }

    fn test_shell() -> Option<String> {
        for candidate in ["/bin/sh", "/usr/bin/sh"] {
            if Command::new(candidate)
                .arg("-c")
                .arg("exit 0")
                .status()
                .is_ok()
            {
                return Some(candidate.to_owned());
            }
        }

        None
    }

    fn test_router() -> axum::Router {
        let auth = test_auth_service();

        build_router(AppState {
            sessions: TerminalSessionRegistry::default(),
            terminal_config: TerminalSessionConfig::from_env(),
            auth,
            audit: sunbolt_audit::AuditLog::default(),
            node_enrollment: NodeEnrollmentRegistry::default(),
            agent_connections: AgentConnectionRegistry::default(),
        })
    }

    fn test_auth_service() -> AuthService {
        let auth = AuthService::new(AuthConfig {
            session_ttl: Duration::from_secs(60 * 60),
            recent_mfa_ttl: Duration::from_secs(10 * 60),
            secure_cookie: false,
            require_step_up_mfa_for_terminal: true,
            bootstrap_admin: false,
            admin_email: "unused@example.com".to_owned(),
            admin_password: "unused".to_owned(),
        });
        auth.upsert_user(
            "admin@example.com",
            "admin-password",
            sunbolt_auth::UserRole::Admin,
        )
        .expect("admin should be created");
        auth.upsert_user(
            "viewer@example.com",
            "viewer-password",
            sunbolt_auth::UserRole::Viewer,
        )
        .expect("viewer should be created");
        auth
    }

    struct TestEnrollment {
        node_id: String,
        credential_fingerprint: String,
    }

    async fn enroll_test_agent(router: &axum::Router, cookie: &str) -> TestEnrollment {
        let token_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(ENROLLMENT_TOKENS_PATH)
                    .header(header::COOKIE, cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({"expires_in_secs": 300}).to_string()))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(token_response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(token_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");
        let token = payload["token"].as_str().expect("token should be present");

        let enroll_response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AGENT_ENROLL_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "token": token,
                            "node_name": "node-a",
                            "hostname": "host-a",
                            "os": "linux",
                            "architecture": "x86_64",
                            "agent_version": "0.1.0"
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(enroll_response.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(enroll_response.into_body(), 1024 * 64)
            .await
            .expect("response body should be readable");
        let payload: Value = serde_json::from_slice(&body).expect("body should parse");

        TestEnrollment {
            node_id: payload["node_id"]
                .as_str()
                .expect("node id should be present")
                .to_owned(),
            credential_fingerprint: payload["credential_fingerprint"]
                .as_str()
                .expect("credential fingerprint should be present")
                .to_owned(),
        }
    }

    async fn login_and_get_cookie(router: &axum::Router, email: &str, password: &str) -> String {
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(AUTH_LOGIN_PATH)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "email": email,
                            "password": password
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("router should respond");
        assert_eq!(response.status(), StatusCode::OK);

        response
            .headers()
            .get(header::SET_COOKIE)
            .expect("set-cookie should be present")
            .to_str()
            .expect("cookie should be utf-8")
            .split(';')
            .next()
            .expect("cookie should contain a token")
            .to_owned()
    }
}
