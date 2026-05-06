use serde::{Deserialize, Serialize};

/// Phase 6 mesh-routing research captured as versioned protocol planning data.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct MeshResearch {
    pub transport_options: Vec<TransportEvaluation>,
    pub relay_mode: RelayModeEvaluation,
    pub trust_model: NodeTrustModel,
    pub audit_implications: Vec<RelayAuditRequirement>,
}

impl MeshResearch {
    #[must_use]
    pub fn phase6() -> Self {
        Self {
            transport_options: vec![
                TransportEvaluation {
                    option: MeshTransportOption::Quic,
                    recommendation: TransportRecommendation::Prototype,
                    rationale: "QUIC is a good candidate for future agent streams because it supports multiplexed streams, transport security, connection migration, and backpressure-friendly flow control without requiring a kernel overlay."
                        .to_owned(),
                },
                TransportEvaluation {
                    option: MeshTransportOption::WireGuardOverlay,
                    recommendation: TransportRecommendation::Defer,
                    rationale: "WireGuard is attractive for operator-managed private networks, but Sunbolt should not require host-level tunnel configuration for the first distributed expansion."
                        .to_owned(),
                },
            ],
            relay_mode: RelayModeEvaluation {
                recommendation: RelayRecommendation::ControlPlaneAuthorizedRelay,
                rationale: "Node-to-node relay should remain authorized by the control plane. Relay nodes may forward encrypted streams, but identity, policy, route grants, and audit records stay centralized."
                    .to_owned(),
            },
            trust_model: NodeTrustModel {
                enrollment: "one-time enrollment token creates node identity and bootstrap credential"
                    .to_owned(),
                long_term_identity: "node key material or certificate, rotated and revocable by the control plane"
                    .to_owned(),
                relay_authorization: "short-lived route grant scoped to target node, relay node, actor, session, and expiration"
                    .to_owned(),
                peer_trust_boundary: "nodes do not trust each other by default; peer traffic requires a control-plane-issued route grant"
                    .to_owned(),
            },
            audit_implications: vec![
                RelayAuditRequirement::RouteSelected,
                RelayAuditRequirement::RelayGrantIssued,
                RelayAuditRequirement::RelayStarted,
                RelayAuditRequirement::RelayEnded,
                RelayAuditRequirement::RelayFailed,
            ],
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransportEvaluation {
    pub option: MeshTransportOption,
    pub recommendation: TransportRecommendation,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeshTransportOption {
    Quic,
    WireGuardOverlay,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportRecommendation {
    Prototype,
    Defer,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelayModeEvaluation {
    pub recommendation: RelayRecommendation,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayRecommendation {
    ControlPlaneAuthorizedRelay,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeTrustModel {
    pub enrollment: String,
    pub long_term_identity: String,
    pub relay_authorization: String,
    pub peer_trust_boundary: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayAuditRequirement {
    RouteSelected,
    RelayGrantIssued,
    RelayStarted,
    RelayEnded,
    RelayFailed,
}

#[cfg(test)]
mod tests {
    use super::{
        MeshResearch, MeshTransportOption, RelayAuditRequirement, RelayRecommendation,
        TransportRecommendation,
    };

    #[test]
    fn phase6_research_prefers_quic_prototype_and_defers_wireguard() {
        let research = MeshResearch::phase6();

        assert!(research.transport_options.iter().any(|evaluation| {
            evaluation.option == MeshTransportOption::Quic
                && evaluation.recommendation == TransportRecommendation::Prototype
        }));
        assert!(research.transport_options.iter().any(|evaluation| {
            evaluation.option == MeshTransportOption::WireGuardOverlay
                && evaluation.recommendation == TransportRecommendation::Defer
        }));
    }

    #[test]
    fn phase6_research_keeps_relay_authorized_by_control_plane() {
        let research = MeshResearch::phase6();

        assert_eq!(
            research.relay_mode.recommendation,
            RelayRecommendation::ControlPlaneAuthorizedRelay
        );
        assert!(research
            .trust_model
            .peer_trust_boundary
            .contains("do not trust each other by default"));
    }

    #[test]
    fn phase6_research_requires_relay_audit_events() {
        let research = MeshResearch::phase6();

        assert_eq!(research.audit_implications.len(), 5);
        assert!(research
            .audit_implications
            .contains(&RelayAuditRequirement::RelayGrantIssued));
        assert!(research
            .audit_implications
            .contains(&RelayAuditRequirement::RelayFailed));
    }
}
