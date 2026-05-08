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
    use std::future;

    use super::{
        NodeCredentialInput, NodeCredentialRecord, NodeCredentialRepository,
        NodeCredentialRepositoryBoundary, NodeHeartbeatInput, NodeHeartbeatRecord,
        NodeHeartbeatRepository, NodeHeartbeatRepositoryBoundary, NodeInput, NodeRecord,
        NodeRepository, NodeRepositoryBoundary, NodeStatusRecord,
    };
    use crate::repositories::{DurableRepository, DurableStateKind, RepositoryFuture};

    struct MockNodeStore;

    impl DurableRepository for MockNodeStore {
        const STATE_KIND: DurableStateKind = DurableStateKind::Nodes;
    }

    impl NodeRepository for MockNodeStore {
        fn find_node_by_node_id<'a>(
            &'a self,
            node_id: &'a str,
        ) -> RepositoryFuture<'a, Option<NodeRecord>> {
            Box::pin(future::ready(Ok(Some(NodeRecord {
                id: 1,
                node_id: node_id.to_owned(),
                display_name: "Build host".to_owned(),
                hostname: "build-1".to_owned(),
                os: "linux".to_owned(),
                architecture: "x86_64".to_owned(),
                agent_version: "0.1.0".to_owned(),
                status: NodeStatusRecord::Online,
                enrolled_at_unix_secs: 1,
            }))))
        }

        fn upsert_node(&self, input: NodeInput) -> RepositoryFuture<'_, NodeRecord> {
            Box::pin(future::ready(Ok(NodeRecord {
                id: 1,
                node_id: input.node_id,
                display_name: input.display_name,
                hostname: input.hostname,
                os: input.os,
                architecture: input.architecture,
                agent_version: input.agent_version,
                status: input.status,
                enrolled_at_unix_secs: 1,
            })))
        }

        fn revoke_node<'a>(&'a self, _node_id: &'a str) -> RepositoryFuture<'a, ()> {
            Box::pin(future::ready(Ok(())))
        }
    }

    struct MockNodeCredentialStore;

    impl DurableRepository for MockNodeCredentialStore {
        const STATE_KIND: DurableStateKind = DurableStateKind::NodeCredentials;
    }

    impl NodeCredentialRepository for MockNodeCredentialStore {
        fn add_credential(
            &self,
            input: NodeCredentialInput,
        ) -> RepositoryFuture<'_, NodeCredentialRecord> {
            Box::pin(future::ready(Ok(NodeCredentialRecord {
                id: 1,
                node_pk: input.node_pk,
                credential_fingerprint: input.credential_fingerprint,
                credential_kind: input.credential_kind,
                created_at_unix_secs: 1,
            })))
        }

        fn list_credentials_for_node(
            &self,
            node_pk: i64,
        ) -> RepositoryFuture<'_, Vec<NodeCredentialRecord>> {
            Box::pin(future::ready(Ok(vec![NodeCredentialRecord {
                id: 1,
                node_pk,
                credential_fingerprint: "sha256:test".to_owned(),
                credential_kind: "development-fingerprint".to_owned(),
                created_at_unix_secs: 1,
            }])))
        }
    }

    struct MockNodeHeartbeatStore;

    impl DurableRepository for MockNodeHeartbeatStore {
        const STATE_KIND: DurableStateKind = DurableStateKind::NodeHeartbeats;
    }

    impl NodeHeartbeatRepository for MockNodeHeartbeatStore {
        fn record_heartbeat(
            &self,
            input: NodeHeartbeatInput,
        ) -> RepositoryFuture<'_, NodeHeartbeatRecord> {
            Box::pin(future::ready(Ok(NodeHeartbeatRecord {
                id: 1,
                node_pk: input.node_pk,
                status: input.status,
                received_at_unix_secs: input.received_at_unix_secs,
            })))
        }

        fn latest_heartbeat_for_node(
            &self,
            node_pk: i64,
        ) -> RepositoryFuture<'_, Option<NodeHeartbeatRecord>> {
            Box::pin(future::ready(Ok(Some(NodeHeartbeatRecord {
                id: 1,
                node_pk,
                status: NodeStatusRecord::Online,
                received_at_unix_secs: 10,
            }))))
        }
    }

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

    #[tokio::test]
    async fn node_repository_boundaries_can_be_mocked() {
        let nodes = MockNodeStore;
        let credentials = MockNodeCredentialStore;
        let heartbeats = MockNodeHeartbeatStore;

        let node = nodes
            .find_node_by_node_id("node-1")
            .await
            .expect("mock node lookup succeeds")
            .expect("mock returns a node");
        let credential = credentials
            .list_credentials_for_node(node.id)
            .await
            .expect("mock credential list succeeds")
            .remove(0);
        let heartbeat = heartbeats
            .latest_heartbeat_for_node(node.id)
            .await
            .expect("mock heartbeat lookup succeeds")
            .expect("mock returns a heartbeat");

        assert_eq!(node.node_id, "node-1");
        assert_eq!(credential.node_pk, node.id);
        assert_eq!(heartbeat.status, NodeStatusRecord::Online);
    }
}
