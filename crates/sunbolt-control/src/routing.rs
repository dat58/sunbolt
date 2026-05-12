use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use sunbolt_protocol::NodeId;
use thiserror::Error;

/// Selects a control-plane route to reach a target node.
pub trait NodeRouter {
    /// Chooses the best currently known route for a target node.
    ///
    /// # Errors
    ///
    /// Returns an error when no healthy direct or relay route is available.
    fn select_route(&self, request: RouteRequest) -> Result<NodeRoute, RouteError>;

    /// Records a successful route operation.
    fn record_success(&self, route: &NodeRoute);

    /// Records a failed route operation.
    fn record_failure(&self, route: &NodeRoute);

    /// Returns the tracked health for a route endpoint.
    fn route_health(&self, endpoint: &RouteEndpoint) -> RouteHealth;
}

/// Inputs available to the router for one terminal-open attempt.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RouteRequest {
    pub target_node_id: NodeId,
    pub direct_agent_connected: bool,
    pub relay_candidates: Vec<NodeId>,
}

/// Selected route from the control plane to a target node.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum NodeRoute {
    DirectAgent {
        target_node_id: NodeId,
    },
    RelayNode {
        target_node_id: NodeId,
        relay_node_id: NodeId,
    },
}

impl NodeRoute {
    #[must_use]
    pub fn endpoint(&self) -> RouteEndpoint {
        match self {
            Self::DirectAgent { target_node_id } => RouteEndpoint::DirectAgent {
                target_node_id: target_node_id.clone(),
            },
            Self::RelayNode { relay_node_id, .. } => RouteEndpoint::RelayNode {
                relay_node_id: relay_node_id.clone(),
            },
        }
    }

    #[must_use]
    pub fn route_id(&self) -> String {
        match self {
            Self::DirectAgent { target_node_id } => {
                format!("direct-agent:{}", target_node_id.0)
            }
            Self::RelayNode {
                relay_node_id,
                target_node_id,
            } => format!("relay:{}:{}", relay_node_id.0, target_node_id.0),
        }
    }
}

/// Health-tracked endpoint for route selection.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum RouteEndpoint {
    DirectAgent { target_node_id: NodeId },
    RelayNode { relay_node_id: NodeId },
}

/// Coarse route health used to avoid repeatedly selecting broken paths.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RouteHealth {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Error, Clone, Copy, Eq, PartialEq)]
pub enum RouteError {
    #[error("no route is available for the target node")]
    NoRoute,
}

#[derive(Clone, Default)]
pub struct InMemoryNodeRouter {
    health: Arc<Mutex<HashMap<RouteEndpoint, RouteHealthRecord>>>,
}

impl NodeRouter for InMemoryNodeRouter {
    fn select_route(&self, request: RouteRequest) -> Result<NodeRoute, RouteError> {
        let direct_endpoint = RouteEndpoint::DirectAgent {
            target_node_id: request.target_node_id.clone(),
        };
        if request.direct_agent_connected
            && self.route_health(&direct_endpoint) != RouteHealth::Unhealthy
        {
            return Ok(NodeRoute::DirectAgent {
                target_node_id: request.target_node_id,
            });
        }

        request
            .relay_candidates
            .into_iter()
            .find(|relay_node_id| {
                self.route_health(&RouteEndpoint::RelayNode {
                    relay_node_id: relay_node_id.clone(),
                }) != RouteHealth::Unhealthy
            })
            .map(|relay_node_id| NodeRoute::RelayNode {
                target_node_id: request.target_node_id,
                relay_node_id,
            })
            .ok_or(RouteError::NoRoute)
    }

    fn record_success(&self, route: &NodeRoute) {
        let endpoint = route.endpoint();
        let mut health = self
            .health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        health.insert(endpoint, RouteHealthRecord::healthy());
    }

    fn record_failure(&self, route: &NodeRoute) {
        let endpoint = route.endpoint();
        let mut health = self
            .health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        health
            .entry(endpoint)
            .and_modify(RouteHealthRecord::record_failure)
            .or_insert_with(RouteHealthRecord::degraded);
    }

    fn route_health(&self, endpoint: &RouteEndpoint) -> RouteHealth {
        self.health
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(endpoint)
            .copied()
            .map_or(RouteHealth::Healthy, RouteHealthRecord::health)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct RouteHealthRecord {
    consecutive_failures: u8,
}

impl RouteHealthRecord {
    const UNHEALTHY_AFTER: u8 = 3;

    const fn healthy() -> Self {
        Self {
            consecutive_failures: 0,
        }
    }

    const fn degraded() -> Self {
        Self {
            consecutive_failures: 1,
        }
    }

    fn record_failure(&mut self) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
    }

    const fn health(self) -> RouteHealth {
        if self.consecutive_failures == 0 {
            RouteHealth::Healthy
        } else if self.consecutive_failures >= Self::UNHEALTHY_AFTER {
            RouteHealth::Unhealthy
        } else {
            RouteHealth::Degraded
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        InMemoryNodeRouter, NodeRoute, NodeRouter, RouteEndpoint, RouteError, RouteHealth,
        RouteRequest,
    };
    use sunbolt_protocol::NodeId;

    #[test]
    fn selects_direct_agent_when_connected() {
        let router = InMemoryNodeRouter::default();

        let route = router
            .select_route(RouteRequest {
                target_node_id: NodeId("node-1".to_owned()),
                direct_agent_connected: true,
                relay_candidates: vec![NodeId("relay-1".to_owned())],
            })
            .expect("direct route should be selected");

        assert_eq!(
            route,
            NodeRoute::DirectAgent {
                target_node_id: NodeId("node-1".to_owned())
            }
        );
    }

    #[test]
    fn selects_relay_when_direct_agent_is_not_connected() {
        let router = InMemoryNodeRouter::default();

        let route = router
            .select_route(RouteRequest {
                target_node_id: NodeId("node-1".to_owned()),
                direct_agent_connected: false,
                relay_candidates: vec![NodeId("relay-1".to_owned())],
            })
            .expect("relay route should be selected");

        assert_eq!(
            route,
            NodeRoute::RelayNode {
                target_node_id: NodeId("node-1".to_owned()),
                relay_node_id: NodeId("relay-1".to_owned())
            }
        );
    }

    #[test]
    fn tracks_route_health_and_avoids_unhealthy_routes() {
        let router = InMemoryNodeRouter::default();
        let route = NodeRoute::DirectAgent {
            target_node_id: NodeId("node-1".to_owned()),
        };
        let endpoint = RouteEndpoint::DirectAgent {
            target_node_id: NodeId("node-1".to_owned()),
        };

        router.record_failure(&route);
        assert_eq!(router.route_health(&endpoint), RouteHealth::Degraded);
        router.record_failure(&route);
        router.record_failure(&route);
        assert_eq!(router.route_health(&endpoint), RouteHealth::Unhealthy);

        let error = router
            .select_route(RouteRequest {
                target_node_id: NodeId("node-1".to_owned()),
                direct_agent_connected: true,
                relay_candidates: vec![],
            })
            .expect_err("unhealthy direct route should not be selected");
        assert_eq!(error, RouteError::NoRoute);

        router.record_success(&route);
        assert_eq!(router.route_health(&endpoint), RouteHealth::Healthy);
    }

    #[test]
    fn route_ids_are_stable_for_tracing() {
        assert_eq!(
            NodeRoute::DirectAgent {
                target_node_id: NodeId("node-1".to_owned())
            }
            .route_id(),
            "direct-agent:node-1"
        );
        assert_eq!(
            NodeRoute::RelayNode {
                target_node_id: NodeId("node-1".to_owned()),
                relay_node_id: NodeId("relay-1".to_owned())
            }
            .route_id(),
            "relay:relay-1:node-1"
        );
    }
}
