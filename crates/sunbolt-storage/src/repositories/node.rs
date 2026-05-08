use crate::repositories::{DurableRepository, DurableStateKind, RepositoryFuture};

pub type NodePk = i64;
pub type NodeCredentialId = i64;
pub type NodeHeartbeatId = i64;

/// Durable node status persisted by the control plane.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NodeStatusRecord {
    Enrolling,
    Online,
    Offline,
    Revoked,
}

/// Durable node inventory record.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NodeRecord {
    pub id: NodePk,
    pub node_id: String,
    pub display_name: String,
    pub hostname: String,
    pub os: String,
    pub architecture: String,
    pub agent_version: String,
    pub status: NodeStatusRecord,
    pub enrolled_at_unix_secs: i64,
}

/// Input for creating or updating durable node inventory.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NodeInput {
    pub node_id: String,
    pub display_name: String,
    pub hostname: String,
    pub os: String,
    pub architecture: String,
    pub agent_version: String,
    pub status: NodeStatusRecord,
}

/// Repository boundary for durable node inventory and revocation state.
pub trait NodeRepository: DurableRepository {
    /// Finds a node by its stable external node ID.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot complete the lookup.
    fn find_node_by_node_id<'a>(
        &'a self,
        node_id: &'a str,
    ) -> RepositoryFuture<'a, Option<NodeRecord>>;

    /// Creates or updates a durable node record.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot persist the node.
    fn upsert_node(&self, input: NodeInput) -> RepositoryFuture<'_, NodeRecord>;

    /// Marks a node as revoked.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot persist the revocation.
    fn revoke_node<'a>(&'a self, node_id: &'a str) -> RepositoryFuture<'a, ()>;
}

/// Durable node credential record.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NodeCredentialRecord {
    pub id: NodeCredentialId,
    pub node_pk: NodePk,
    pub credential_fingerprint: String,
    pub credential_kind: String,
    pub created_at_unix_secs: i64,
}

/// Input for appending durable node credential material metadata.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NodeCredentialInput {
    pub node_pk: NodePk,
    pub credential_fingerprint: String,
    pub credential_kind: String,
}

/// Repository boundary for durable node identity credentials.
pub trait NodeCredentialRepository: DurableRepository {
    /// Appends a credential metadata record for a node.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot persist the credential.
    fn add_credential(
        &self,
        input: NodeCredentialInput,
    ) -> RepositoryFuture<'_, NodeCredentialRecord>;

    /// Lists credential metadata records for a node.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot list credentials.
    fn list_credentials_for_node(
        &self,
        node_pk: NodePk,
    ) -> RepositoryFuture<'_, Vec<NodeCredentialRecord>>;
}

/// Durable node heartbeat record.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NodeHeartbeatRecord {
    pub id: NodeHeartbeatId,
    pub node_pk: NodePk,
    pub status: NodeStatusRecord,
    pub received_at_unix_secs: i64,
}

/// Input for appending heartbeat state.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct NodeHeartbeatInput {
    pub node_pk: NodePk,
    pub status: NodeStatusRecord,
    pub received_at_unix_secs: i64,
}

/// Repository boundary for durable node heartbeat history.
pub trait NodeHeartbeatRepository: DurableRepository {
    /// Appends a heartbeat record.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot persist the heartbeat.
    fn record_heartbeat(
        &self,
        input: NodeHeartbeatInput,
    ) -> RepositoryFuture<'_, NodeHeartbeatRecord>;

    /// Returns the latest known heartbeat for a node.
    ///
    /// # Errors
    ///
    /// Returns an error when the storage backend cannot complete the lookup.
    fn latest_heartbeat_for_node(
        &self,
        node_pk: NodePk,
    ) -> RepositoryFuture<'_, Option<NodeHeartbeatRecord>>;
}

/// Marker repository for durable node inventory state.
pub struct NodeRepositoryBoundary;

impl DurableRepository for NodeRepositoryBoundary {
    const STATE_KIND: DurableStateKind = DurableStateKind::Nodes;
}

/// Marker repository for durable node credential state.
pub struct NodeCredentialRepositoryBoundary;

impl DurableRepository for NodeCredentialRepositoryBoundary {
    const STATE_KIND: DurableStateKind = DurableStateKind::NodeCredentials;
}

/// Marker repository for durable node heartbeat state.
pub struct NodeHeartbeatRepositoryBoundary;

impl DurableRepository for NodeHeartbeatRepositoryBoundary {
    const STATE_KIND: DurableStateKind = DurableStateKind::NodeHeartbeats;
}

#[cfg(test)]
mod tests {
    use super::{
        NodeCredentialRepositoryBoundary, NodeHeartbeatRepositoryBoundary, NodeRepositoryBoundary,
    };
    use crate::repositories::{DurableRepository, DurableStateKind};

    #[test]
    fn node_repository_markers_map_to_durable_state() {
        assert_eq!(NodeRepositoryBoundary.state_kind(), DurableStateKind::Nodes);
        assert_eq!(
            NodeCredentialRepositoryBoundary.state_kind(),
            DurableStateKind::NodeCredentials
        );
        assert_eq!(
            NodeHeartbeatRepositoryBoundary.state_kind(),
            DurableStateKind::NodeHeartbeats
        );
    }
}
