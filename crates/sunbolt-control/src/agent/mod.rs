use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use sunbolt_protocol::{AgentTerminalCommand, AgentTerminalEvent, NodeId};
use tokio::sync::{mpsc, Mutex as AsyncMutex};

use crate::node::NodeView;

#[derive(Debug, Clone)]
pub(crate) struct RegisteredAgentConnection {
    pub(crate) command_tx: mpsc::Sender<AgentTerminalCommand>,
    pub(crate) event_rx: Arc<AsyncMutex<mpsc::Receiver<AgentTerminalEvent>>>,
}

#[derive(Clone, Default)]
pub(crate) struct AgentConnectionRegistry {
    inner: Arc<Mutex<HashMap<String, RegisteredAgentConnection>>>,
}

impl AgentConnectionRegistry {
    #[cfg(test)]
    pub(crate) fn register(
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

    pub(crate) fn connection(&self, node_id: &str) -> Option<RegisteredAgentConnection> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(node_id)
            .cloned()
    }

    pub(crate) fn disconnect(&self, node_id: &str) {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(node_id);
    }

    pub(crate) fn connected_node_ids_except(&self, node_id: &str) -> Vec<NodeId> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .filter(|candidate| candidate.as_str() != node_id)
            .cloned()
            .map(NodeId)
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct AgentEnrollmentRequest {
    pub(crate) token: String,
    pub(crate) node_name: String,
    pub(crate) hostname: String,
    pub(crate) os: String,
    pub(crate) architecture: String,
    pub(crate) agent_version: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentEnrollmentResponse {
    pub(crate) node_id: String,
    pub(crate) credential_fingerprint: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct AgentHeartbeatRequest {
    pub(crate) node_id: String,
    pub(crate) credential_fingerprint: String,
    pub(crate) hostname: String,
    pub(crate) os: String,
    pub(crate) architecture: String,
    pub(crate) agent_version: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct AgentHeartbeatResponse {
    pub(crate) accepted: bool,
    pub(crate) node: NodeView,
}

pub(crate) fn credential_fingerprint(request: &AgentEnrollmentRequest) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    request.node_name.hash(&mut hasher);
    request.hostname.hash(&mut hasher);
    request.os.hash(&mut hasher);
    request.architecture.hash(&mut hasher);
    request.agent_version.hash(&mut hasher);
    format!("dev-{:016x}", hasher.finish())
}
