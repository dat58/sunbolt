use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime},
};

use serde::Serialize;
use sunbolt_auth::User;

use crate::{
    agent::{
        credential_expiration_unix_secs, credential_proof, generate_node_credential,
        AgentEnrollmentRequest, AgentEnrollmentResponse, AgentHeartbeatRequest,
        NODE_CREDENTIAL_TTL,
    },
    config::NODE_OFFLINE_AFTER,
    error::{EnrollmentError, NodeConnectionError},
    security::{random_token, token_hash},
};

#[derive(Clone)]
pub(crate) struct NodeEnrollmentRegistry {
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
pub(crate) enum NodeStatus {
    Enrolled,
    Online,
    Offline,
    Revoked,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct NodeCredentialRecord {
    node_id: u64,
    credential_fingerprint: String,
    credential_proof: String,
    created_at: Instant,
    created_at_unix_secs: i64,
    expires_at: Instant,
    expires_at_unix_secs: i64,
    rotated_from_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct NodeHeartbeatRecord {
    node_id: u64,
    status: NodeStatus,
    received_at: Instant,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub(crate) struct NodeView {
    pub(crate) node_id: String,
    pub(crate) display_name: String,
    pub(crate) hostname: String,
    pub(crate) os: String,
    pub(crate) architecture: String,
    pub(crate) agent_version: String,
    pub(crate) status: NodeStatus,
    pub(crate) credential_expires_at_unix_secs: Option<i64>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub(crate) struct RotatedNodeCredential {
    pub(crate) credential_fingerprint: String,
    pub(crate) credential_secret: String,
    pub(crate) credential_created_at_unix_secs: i64,
    pub(crate) credential_expires_at_unix_secs: i64,
    pub(crate) previous_credential_fingerprint: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub(crate) struct NodeCredentialRotationResponse {
    pub(crate) node: NodeView,
    pub(crate) credential: RotatedNodeCredential,
}

#[derive(Debug, Serialize)]
pub(crate) struct EnrollmentTokenResponse {
    pub(crate) token: String,
    pub(crate) expires_in_secs: u64,
    pub(crate) enrollment_command: String,
}

impl NodeEnrollmentRegistry {
    pub(crate) fn create_token(&self, user: &User, ttl: Duration) -> EnrollmentTokenResponse {
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

    pub(crate) fn enroll(
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
        let (credential_secret, credential_fingerprint) = generate_node_credential();
        let credential_created_at_unix_secs = unix_secs(SystemTime::now());
        let credential_expires_at_unix_secs = credential_expiration_unix_secs(SystemTime::now());
        let credential_expires_at = Instant::now() + NODE_CREDENTIAL_TTL;
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
            credential_proof: credential_proof(&node_id, &credential_secret),
            created_at: Instant::now(),
            created_at_unix_secs: credential_created_at_unix_secs,
            expires_at: credential_expires_at,
            expires_at_unix_secs: credential_expires_at_unix_secs,
            rotated_from_fingerprint: None,
        });
        state.heartbeats.push(NodeHeartbeatRecord {
            node_id: id,
            status: NodeStatus::Enrolled,
            received_at: Instant::now(),
        });

        Ok(AgentEnrollmentResponse {
            node_id,
            credential_fingerprint,
            credential_secret,
            credential_expires_at_unix_secs,
        })
    }

    pub(crate) fn heartbeat(
        &self,
        request: AgentHeartbeatRequest,
    ) -> Result<NodeView, NodeConnectionError> {
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
        let Some(credential) = authenticated_credential(
            &state,
            node.id,
            &request.credential_proof,
            &request.credential_fingerprint,
        ) else {
            return Err(NodeConnectionError::InvalidCredential);
        };
        if credential.expires_at <= Instant::now() {
            return Err(NodeConnectionError::CredentialExpired);
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

        Ok(node_view(
            &state.nodes[node_index],
            Some(Instant::now()),
            credential_expiration_for_node(&state, node_id),
        ))
    }

    pub(crate) fn authenticate_transport(
        &self,
        node_id: &str,
        credential_fingerprint: &str,
        credential_proof: &str,
        agent_version: &str,
    ) -> Result<NodeView, NodeConnectionError> {
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let node_index = state
            .nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .ok_or(NodeConnectionError::UnknownNode)?;
        let node = &state.nodes[node_index];
        if node.status == NodeStatus::Revoked {
            return Err(NodeConnectionError::Revoked);
        }
        let Some(credential) =
            authenticated_credential(&state, node.id, credential_proof, credential_fingerprint)
        else {
            return Err(NodeConnectionError::InvalidCredential);
        };
        if credential.expires_at <= Instant::now() {
            return Err(NodeConnectionError::CredentialExpired);
        }

        agent_version.clone_into(&mut state.nodes[node_index].agent_version);
        state.nodes[node_index].status = NodeStatus::Online;
        let internal_node_id = state.nodes[node_index].id;
        state.heartbeats.push(NodeHeartbeatRecord {
            node_id: internal_node_id,
            status: NodeStatus::Online,
            received_at: Instant::now(),
        });

        Ok(node_view(
            &state.nodes[node_index],
            Some(Instant::now()),
            credential_expiration_for_node(&state, internal_node_id),
        ))
    }

    pub(crate) fn list_nodes(&self) -> Vec<NodeView> {
        let state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .nodes
            .iter()
            .map(|node| {
                node_view(
                    node,
                    latest_heartbeat_at(&state, node.id),
                    credential_expiration_for_node(&state, node.id),
                )
            })
            .collect()
    }

    pub(crate) fn node_details(&self, node_id: &str) -> Option<NodeView> {
        let state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .nodes
            .iter()
            .find(|node| node.node_id == node_id)
            .map(|node| {
                node_view(
                    node,
                    latest_heartbeat_at(&state, node.id),
                    credential_expiration_for_node(&state, node.id),
                )
            })
    }

    pub(crate) fn revoke_node(&self, node_id: &str) -> Result<NodeView, NodeConnectionError> {
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
        let node_pk = state.nodes[node_index].id;
        Ok(node_view(
            &state.nodes[node_index],
            latest_heartbeat_at(&state, node_pk),
            credential_expiration_for_node(&state, node_pk),
        ))
    }

    pub(crate) fn rotate_credential(
        &self,
        node_id: &str,
    ) -> Result<NodeCredentialRotationResponse, NodeConnectionError> {
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let node_index = state
            .nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .ok_or(NodeConnectionError::UnknownNode)?;
        let node = &state.nodes[node_index];
        if node.status == NodeStatus::Revoked {
            return Err(NodeConnectionError::Revoked);
        }

        let previous_credential_fingerprint = latest_credential_for_node(&state, node.id)
            .map(|credential| credential.credential_fingerprint.clone());
        let (credential_secret, credential_fingerprint) = generate_node_credential();
        let credential_created_at_unix_secs = unix_secs(SystemTime::now());
        let credential_expires_at_unix_secs = credential_expiration_unix_secs(SystemTime::now());
        let credential_expires_at = Instant::now() + NODE_CREDENTIAL_TTL;
        let node_pk = node.id;
        state.credentials.push(NodeCredentialRecord {
            node_id: node_pk,
            credential_fingerprint: credential_fingerprint.clone(),
            credential_proof: credential_proof(node_id, &credential_secret),
            created_at: Instant::now(),
            created_at_unix_secs: credential_created_at_unix_secs,
            expires_at: credential_expires_at,
            expires_at_unix_secs: credential_expires_at_unix_secs,
            rotated_from_fingerprint: previous_credential_fingerprint.clone(),
        });

        Ok(NodeCredentialRotationResponse {
            node: node_view(
                &state.nodes[node_index],
                latest_heartbeat_at(&state, node_pk),
                credential_expiration_for_node(&state, node_pk),
            ),
            credential: RotatedNodeCredential {
                credential_fingerprint,
                credential_secret,
                credential_created_at_unix_secs,
                credential_expires_at_unix_secs,
                previous_credential_fingerprint,
            },
        })
    }

    pub(crate) fn node_is_online(&self, node_id: &str) -> bool {
        self.node_details(node_id)
            .is_some_and(|node| node.status == NodeStatus::Online)
    }

    #[cfg(test)]
    pub(crate) fn expire_credentials_for_node(&self, node_id: &str) -> bool {
        let mut state = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(node_pk) = state
            .nodes
            .iter()
            .find(|node| node.node_id == node_id)
            .map(|node| node.id)
        else {
            return false;
        };
        let mut expired = false;
        let expired_at = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
        for credential in state
            .credentials
            .iter_mut()
            .filter(|credential| credential.node_id == node_pk)
        {
            credential.expires_at = expired_at;
            expired = true;
        }
        expired
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

fn authenticated_credential<'a>(
    state: &'a NodeEnrollmentState,
    node_id: u64,
    presented_proof: &str,
    claimed_fingerprint: &str,
) -> Option<&'a NodeCredentialRecord> {
    state.credentials.iter().find(|credential| {
        credential.node_id == node_id
            && credential.credential_fingerprint == claimed_fingerprint
            && credential.credential_proof == presented_proof
    })
}

fn credential_expiration_for_node(state: &NodeEnrollmentState, node_id: u64) -> Option<i64> {
    state
        .credentials
        .iter()
        .filter(|credential| credential.node_id == node_id)
        .map(|credential| credential.expires_at_unix_secs)
        .max()
}

fn latest_credential_for_node(
    state: &NodeEnrollmentState,
    node_id: u64,
) -> Option<&NodeCredentialRecord> {
    state
        .credentials
        .iter()
        .filter(|credential| credential.node_id == node_id)
        .max_by_key(|credential| {
            (
                credential.created_at,
                credential.created_at_unix_secs,
                credential.rotated_from_fingerprint.is_some(),
            )
        })
}

fn unix_secs(now: SystemTime) -> i64 {
    i64::try_from(
        now.duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    )
    .unwrap_or(i64::MAX)
}

fn node_view(
    node: &NodeRecord,
    last_heartbeat_at: Option<Instant>,
    credential_expires_at_unix_secs: Option<i64>,
) -> NodeView {
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
        credential_expires_at_unix_secs,
    }
}
