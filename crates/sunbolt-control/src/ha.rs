use serde::{Deserialize, Serialize};

/// Phase 6 high-availability design for multiple control-plane instances.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ControlPlaneHaPlan {
    pub shared_state: Vec<SharedStateItem>,
    pub routing_backend_options: Vec<RoutingBackendEvaluation>,
    pub agent_connection_strategy: AgentConnectionStrategy,
    pub websocket_sticky_routing: StickyRoutingStrategy,
}

impl ControlPlaneHaPlan {
    #[must_use]
    pub fn phase6() -> Self {
        Self {
            shared_state: vec![
                SharedStateItem::Users,
                SharedStateItem::Sessions,
                SharedStateItem::MfaChallenges,
                SharedStateItem::RbacPolicy,
                SharedStateItem::Nodes,
                SharedStateItem::NodeCredentials,
                SharedStateItem::NodeHeartbeats,
                SharedStateItem::TerminalSessionMetadata,
                SharedStateItem::RouteHealth,
                SharedStateItem::AuditLog,
            ],
            routing_backend_options: vec![
                RoutingBackendEvaluation {
                    backend: RoutingBackend::PostgresNotify,
                    recommendation: BackendRecommendation::Baseline,
                    rationale: "Postgres notifications fit the current storage direction and are sufficient for coarse agent/session invalidation, but not ideal for high-rate terminal stream routing."
                        .to_owned(),
                },
                RoutingBackendEvaluation {
                    backend: RoutingBackend::Redis,
                    recommendation: BackendRecommendation::Prototype,
                    rationale: "Redis is a practical next prototype for ephemeral active-session routing, leases, and presence because it has low-latency pub/sub and expiring keys."
                        .to_owned(),
                },
                RoutingBackendEvaluation {
                    backend: RoutingBackend::Nats,
                    recommendation: BackendRecommendation::EvaluateLater,
                    rationale: "NATS is a strong fit for larger distributed control planes, but it adds an additional operational dependency before Sunbolt needs message-bus scale."
                        .to_owned(),
                },
            ],
            agent_connection_strategy: AgentConnectionStrategy {
                model: AgentConnectionModel::SingleActiveWithFailover,
                lease_owner: "each connected agent is owned by one control-plane instance lease at a time"
                    .to_owned(),
                failover: "agent reconnects with backoff when heartbeat acknowledgements or websocket pings fail; any instance may claim the next lease"
                    .to_owned(),
                identity: "node identity and credential validation remain shared through storage, not instance-local memory"
                    .to_owned(),
            },
            websocket_sticky_routing: StickyRoutingStrategy {
                requirement: StickyRoutingRequirement::StickyForActiveTerminal,
                rationale: "browser terminal WebSockets should stay on the instance that owns the local socket pair and selected agent route for the active session"
                    .to_owned(),
                reconnect: "reattach uses shared terminal session metadata to find the owning instance or fail cleanly when the session lease expired"
                    .to_owned(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharedStateItem {
    Users,
    Sessions,
    MfaChallenges,
    RbacPolicy,
    Nodes,
    NodeCredentials,
    NodeHeartbeats,
    TerminalSessionMetadata,
    RouteHealth,
    AuditLog,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutingBackendEvaluation {
    pub backend: RoutingBackend,
    pub recommendation: BackendRecommendation,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingBackend {
    Redis,
    Nats,
    PostgresNotify,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendRecommendation {
    Baseline,
    Prototype,
    EvaluateLater,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AgentConnectionStrategy {
    pub model: AgentConnectionModel,
    pub lease_owner: String,
    pub failover: String,
    pub identity: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentConnectionModel {
    SingleActiveWithFailover,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct StickyRoutingStrategy {
    pub requirement: StickyRoutingRequirement,
    pub rationale: String,
    pub reconnect: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StickyRoutingRequirement {
    StickyForActiveTerminal,
}

#[cfg(test)]
mod tests {
    use super::{
        AgentConnectionModel, BackendRecommendation, ControlPlaneHaPlan, RoutingBackend,
        SharedStateItem, StickyRoutingRequirement,
    };

    #[test]
    fn phase6_ha_identifies_security_and_routing_shared_state() {
        let plan = ControlPlaneHaPlan::phase6();

        assert!(plan.shared_state.contains(&SharedStateItem::Sessions));
        assert!(plan.shared_state.contains(&SharedStateItem::RbacPolicy));
        assert!(plan
            .shared_state
            .contains(&SharedStateItem::TerminalSessionMetadata));
        assert!(plan.shared_state.contains(&SharedStateItem::RouteHealth));
        assert!(plan.shared_state.contains(&SharedStateItem::AuditLog));
    }

    #[test]
    fn phase6_ha_selects_redis_as_ephemeral_routing_prototype() {
        let plan = ControlPlaneHaPlan::phase6();

        assert!(plan.routing_backend_options.iter().any(|evaluation| {
            evaluation.backend == RoutingBackend::PostgresNotify
                && evaluation.recommendation == BackendRecommendation::Baseline
        }));
        assert!(plan.routing_backend_options.iter().any(|evaluation| {
            evaluation.backend == RoutingBackend::Redis
                && evaluation.recommendation == BackendRecommendation::Prototype
        }));
        assert!(plan.routing_backend_options.iter().any(|evaluation| {
            evaluation.backend == RoutingBackend::Nats
                && evaluation.recommendation == BackendRecommendation::EvaluateLater
        }));
    }

    #[test]
    fn phase6_ha_requires_agent_failover_and_sticky_browser_websockets() {
        let plan = ControlPlaneHaPlan::phase6();

        assert_eq!(
            plan.agent_connection_strategy.model,
            AgentConnectionModel::SingleActiveWithFailover
        );
        assert_eq!(
            plan.websocket_sticky_routing.requirement,
            StickyRoutingRequirement::StickyForActiveTerminal
        );
        assert!(plan
            .agent_connection_strategy
            .identity
            .contains("not instance-local memory"));
    }
}
