use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Serialize;
use sunbolt_protocol::{
    AgentTerminalCommand, TerminalReconnectToken, TerminalServerMessage, TerminalSessionId,
    TerminalSize as ProtocolTerminalSize,
};
use sunbolt_terminal::{LocalPtySession, TerminalSessionState};
use tokio::sync::{broadcast, mpsc};

use crate::{config::TerminalSessionConfig, error::SessionLimitError, security::random_token};

const OUTPUT_CHANNEL_CAPACITY: usize = 64;
const OUTPUT_REPLAY_CAPACITY: usize = 128;

#[derive(Clone, Default)]
pub(crate) struct TerminalSessionRegistry {
    inner: Arc<Mutex<HashMap<TerminalSessionId, TrackedTerminalSession>>>,
}

#[derive(Clone)]
pub(crate) enum TerminalBackend {
    Local(Arc<LocalPtySession>),
    Remote {
        command_tx: mpsc::Sender<AgentTerminalCommand>,
    },
}

pub(crate) struct TerminalReattachTarget {
    pub(crate) backend: TerminalBackend,
    pub(crate) output_rx: broadcast::Receiver<TerminalServerMessage>,
    pub(crate) replay: Vec<TerminalServerMessage>,
    pub(crate) size: ProtocolTerminalSize,
    pub(crate) reconnect_token: TerminalReconnectToken,
    pub(crate) node_id: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub(crate) struct TerminalSessionSummary {
    pub(crate) session_id: String,
    pub(crate) node_id: Option<String>,
    pub(crate) state: &'static str,
    pub(crate) cols: u16,
    pub(crate) rows: u16,
    pub(crate) age_secs: u64,
    pub(crate) idle_secs: u64,
}

struct TrackedTerminalSession {
    backend: TerminalBackend,
    output_tx: broadcast::Sender<TerminalServerMessage>,
    replay: VecDeque<TerminalServerMessage>,
    next_output_sequence: u64,
    reconnect_token: TerminalReconnectToken,
    state: TerminalSessionState,
    last_activity: Instant,
    created_at: Instant,
    size: ProtocolTerminalSize,
    actor_email: String,
    node_id: Option<String>,
}

impl TerminalSessionRegistry {
    pub(crate) fn insert(
        &self,
        session_id: TerminalSessionId,
        session: Arc<LocalPtySession>,
        size: ProtocolTerminalSize,
        config: TerminalSessionConfig,
        actor_email: String,
        node_id: Option<String>,
    ) -> Result<broadcast::Sender<TerminalServerMessage>, SessionLimitError> {
        self.insert_backend(
            session_id,
            TerminalBackend::Local(session),
            size,
            config,
            actor_email,
            node_id,
        )
    }

    pub(crate) fn insert_remote(
        &self,
        session_id: TerminalSessionId,
        command_tx: mpsc::Sender<AgentTerminalCommand>,
        size: ProtocolTerminalSize,
        config: TerminalSessionConfig,
        actor_email: String,
        node_id: String,
    ) -> Result<broadcast::Sender<TerminalServerMessage>, SessionLimitError> {
        self.insert_backend(
            session_id,
            TerminalBackend::Remote { command_tx },
            size,
            config,
            actor_email,
            Some(node_id),
        )
    }

    fn insert_backend(
        &self,
        session_id: TerminalSessionId,
        backend: TerminalBackend,
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
            .filter(|s| s.actor_email == actor_email && !s.state.is_terminal())
            .count();
        if user_count >= config.max_sessions_per_user {
            return Err(SessionLimitError::PerUser);
        }
        let node_count = sessions
            .values()
            .filter(|s| s.node_id.as_deref() == node_id.as_deref() && !s.state.is_terminal())
            .count();
        if node_count >= config.max_sessions_per_node {
            return Err(SessionLimitError::PerNode);
        }
        let (output_tx, _) = broadcast::channel(OUTPUT_CHANNEL_CAPACITY);
        sessions.insert(
            session_id,
            TrackedTerminalSession {
                backend,
                output_tx: output_tx.clone(),
                replay: VecDeque::with_capacity(OUTPUT_REPLAY_CAPACITY),
                next_output_sequence: 1,
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

    pub(crate) fn set_state(&self, session_id: &TerminalSessionId, state: TerminalSessionState) {
        if let Ok(mut sessions) = self.inner.lock() {
            if let Some(session) = sessions.get_mut(session_id) {
                if !session.state.can_transition_to(state) && session.state != state {
                    return;
                }
                session.state = state;
            }
        }
    }

    pub(crate) fn detach(&self, session_id: &TerminalSessionId) {
        self.set_state(session_id, TerminalSessionState::Detached);
    }

    pub(crate) fn reattach(
        &self,
        session_id: &TerminalSessionId,
        reconnect_token: &TerminalReconnectToken,
        actor_email: &str,
    ) -> Option<TerminalReattachTarget> {
        let Ok(mut sessions) = self.inner.lock() else {
            return None;
        };
        let tracked = sessions.get_mut(session_id)?;
        if tracked.actor_email != actor_email || !tracked.state.is_reattachable() {
            return None;
        }
        if &tracked.reconnect_token != reconnect_token {
            return None;
        }
        tracked.reconnect_token = TerminalReconnectToken(random_token());
        tracked.state = TerminalSessionState::Reattaching;
        tracked.last_activity = Instant::now();
        Some(TerminalReattachTarget {
            backend: tracked.backend.clone(),
            output_rx: tracked.output_tx.subscribe(),
            replay: tracked.replay.iter().cloned().collect(),
            size: tracked.size,
            reconnect_token: tracked.reconnect_token.clone(),
            node_id: tracked.node_id.clone(),
        })
    }

    pub(crate) fn reconnect_token(
        &self,
        session_id: &TerminalSessionId,
    ) -> Option<TerminalReconnectToken> {
        self.inner.lock().ok().and_then(|sessions| {
            sessions
                .get(session_id)
                .map(|session| session.reconnect_token.clone())
        })
    }

    pub(crate) fn owner_matches(&self, session_id: &TerminalSessionId, actor_email: &str) -> bool {
        self.inner.lock().ok().is_some_and(|sessions| {
            sessions
                .get(session_id)
                .is_some_and(|session| session.actor_email == actor_email)
        })
    }

    pub(crate) fn node_id(&self, session_id: &TerminalSessionId) -> Option<String> {
        self.inner.lock().ok().and_then(|sessions| {
            sessions
                .get(session_id)
                .and_then(|session| session.node_id.clone())
        })
    }

    pub(crate) fn touch(&self, session_id: &TerminalSessionId) {
        if let Ok(mut sessions) = self.inner.lock() {
            if let Some(session) = sessions.get_mut(session_id) {
                session.last_activity = Instant::now();
            }
        }
    }

    pub(crate) fn is_idle(&self, session_id: &TerminalSessionId, timeout: Duration) -> bool {
        let Ok(sessions) = self.inner.lock() else {
            return true;
        };
        sessions
            .get(session_id)
            .is_none_or(|session| session.last_activity.elapsed() >= timeout)
    }

    pub(crate) fn set_size(&self, session_id: &TerminalSessionId, size: ProtocolTerminalSize) {
        if let Ok(mut sessions) = self.inner.lock() {
            if let Some(session) = sessions.get_mut(session_id) {
                session.size = size;
            }
        }
    }

    pub(crate) fn record_output(
        &self,
        session_id: &TerminalSessionId,
        data: String,
    ) -> Option<TerminalServerMessage> {
        let Ok(mut sessions) = self.inner.lock() else {
            return None;
        };
        let session = sessions.get_mut(session_id)?;
        let message = TerminalServerMessage::Output {
            session_id: session_id.clone(),
            sequence: session.next_output_sequence,
            data,
        };
        session.next_output_sequence = session.next_output_sequence.saturating_add(1);
        push_replay(session, message.clone());
        Some(message)
    }

    pub(crate) fn remember_server_message(&self, message: TerminalServerMessage) {
        let session_id = match &message {
            TerminalServerMessage::Output { session_id, .. }
            | TerminalServerMessage::Exited { session_id, .. }
            | TerminalServerMessage::Detached { session_id }
            | TerminalServerMessage::Reattached { session_id, .. }
            | TerminalServerMessage::Started { session_id, .. } => session_id,
            TerminalServerMessage::Error { session_id, .. } => {
                let Some(session_id) = session_id else {
                    return;
                };
                session_id
            }
            TerminalServerMessage::Pong { .. } => return,
        };
        if let Ok(mut sessions) = self.inner.lock() {
            if let Some(session) = sessions.get_mut(session_id) {
                push_replay(session, message);
            }
        }
    }

    pub(crate) fn terminate(&self, session_id: &TerminalSessionId) -> Option<TerminalBackend> {
        let Ok(mut sessions) = self.inner.lock() else {
            return None;
        };
        if let Some(session) = sessions.get_mut(session_id) {
            session.state = TerminalSessionState::Terminating;
        }
        remove_tracked(&mut sessions, session_id).map(|session| session.backend)
    }

    pub(crate) fn remove(&self, session_id: &TerminalSessionId) {
        let Ok(mut sessions) = self.inner.lock() else {
            return;
        };
        let _ = remove_tracked(&mut sessions, session_id);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.inner.lock().map_or(0, |sessions| sessions.len())
    }

    pub(crate) fn state(&self, session_id: &TerminalSessionId) -> Option<TerminalSessionState> {
        self.inner
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(session_id).map(|session| session.state))
    }

    pub(crate) fn is_exceeded_max_duration(
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

    pub(crate) fn list_sessions_for_actor(
        &self,
        actor_email: &str,
        states: &[TerminalSessionState],
    ) -> Vec<TerminalSessionSummary> {
        let Ok(sessions) = self.inner.lock() else {
            return vec![];
        };
        sessions
            .iter()
            .filter(|(_, session)| {
                session.actor_email == actor_email && states.contains(&session.state)
            })
            .map(|(session_id, session)| TerminalSessionSummary {
                session_id: session_id.0.clone(),
                node_id: session.node_id.clone(),
                state: state_name(session.state),
                cols: session.size.cols,
                rows: session.size.rows,
                age_secs: session.created_at.elapsed().as_secs(),
                idle_secs: session.last_activity.elapsed().as_secs(),
            })
            .collect()
    }

    pub(crate) fn drain_expired(
        &self,
        max_duration: Duration,
        detached_idle_timeout: Duration,
    ) -> Vec<(
        TerminalSessionId,
        String,
        broadcast::Sender<TerminalServerMessage>,
        TerminalBackend,
    )> {
        let Ok(mut sessions) = self.inner.lock() else {
            return vec![];
        };
        let expired: Vec<TerminalSessionId> = sessions
            .iter()
            .filter(|(_, session)| {
                !session.state.is_terminal()
                    && (session.created_at.elapsed() >= max_duration
                        || (session.state == TerminalSessionState::Detached
                            && session.last_activity.elapsed() >= detached_idle_timeout))
            })
            .map(|(id, _)| id.clone())
            .collect();
        expired
            .into_iter()
            .filter_map(|id| {
                sessions.get_mut(&id).map(|session| {
                    session.state = TerminalSessionState::Expired;
                })?;
                remove_tracked(&mut sessions, &id)
                    .map(|s| (id, s.actor_email, s.output_tx, s.backend))
            })
            .collect()
    }

    pub(crate) fn close_sessions_for_node(
        &self,
        node_id: &str,
    ) -> Vec<(
        TerminalSessionId,
        String,
        broadcast::Sender<TerminalServerMessage>,
        TerminalBackend,
    )> {
        let Ok(mut sessions) = self.inner.lock() else {
            return vec![];
        };
        let to_close: Vec<TerminalSessionId> = sessions
            .iter()
            .filter(|(_, s)| s.node_id.as_deref() == Some(node_id) && !s.state.is_terminal())
            .map(|(id, _)| id.clone())
            .collect();
        to_close
            .into_iter()
            .filter_map(|session_id| {
                sessions.get_mut(&session_id).map(|session| {
                    session.state = TerminalSessionState::Terminating;
                })?;
                remove_tracked(&mut sessions, &session_id)
                    .map(|s| (session_id, s.actor_email, s.output_tx, s.backend))
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
                close_backend(&session.backend);
            }
        }
    }
}

fn push_replay(session: &mut TrackedTerminalSession, message: TerminalServerMessage) {
    if session.replay.len() == OUTPUT_REPLAY_CAPACITY {
        session.replay.pop_front();
    }
    session.replay.push_back(message);
}

fn remove_tracked(
    sessions: &mut HashMap<TerminalSessionId, TrackedTerminalSession>,
    session_id: &TerminalSessionId,
) -> Option<TrackedTerminalSession> {
    let session = sessions.remove(session_id)?;
    close_backend(&session.backend);
    Some(session)
}

fn close_backend(backend: &TerminalBackend) {
    match backend {
        TerminalBackend::Local(session) => {
            let _ = session.close();
        }
        TerminalBackend::Remote { .. } => {}
    }
}

fn state_name(state: TerminalSessionState) -> &'static str {
    match state {
        TerminalSessionState::Created => "created",
        TerminalSessionState::Starting => "starting",
        TerminalSessionState::Active => "active",
        TerminalSessionState::Detached => "detached",
        TerminalSessionState::Reattaching => "reattaching",
        TerminalSessionState::Terminating => "terminating",
        TerminalSessionState::Terminated => "terminated",
        TerminalSessionState::Failed => "failed",
        TerminalSessionState::Expired => "expired",
    }
}
