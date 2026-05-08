use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use sunbolt_protocol::{
    TerminalReconnectToken, TerminalServerMessage, TerminalSessionId,
    TerminalSize as ProtocolTerminalSize,
};
use sunbolt_terminal::{LocalPtySession, TerminalSessionState};
use tokio::sync::broadcast;

use crate::{config::TerminalSessionConfig, error::SessionLimitError, security::random_token};

const OUTPUT_CHANNEL_CAPACITY: usize = 32;

#[derive(Clone, Default)]
pub(crate) struct TerminalSessionRegistry {
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
    pub(crate) fn insert(
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

    pub(crate) fn remove_if_detached(&self, session_id: &TerminalSessionId) -> bool {
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

    pub(crate) fn remove(&self, session_id: &TerminalSessionId) {
        if let Ok(mut sessions) = self.inner.lock() {
            if let Some(session) = sessions.remove(session_id) {
                let _ = session.session.close();
            }
        }
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

    pub(crate) fn drain_exceeded_max_duration(
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

    pub(crate) fn close_sessions_for_node(
        &self,
        node_id: &str,
    ) -> Vec<(
        TerminalSessionId,
        String,
        broadcast::Sender<TerminalServerMessage>,
        Arc<LocalPtySession>,
    )> {
        let Ok(mut sessions) = self.inner.lock() else {
            return vec![];
        };
        let to_close: Vec<TerminalSessionId> = sessions
            .iter()
            .filter(|(_, s)| {
                s.node_id.as_deref() == Some(node_id)
                    && !matches!(
                        s.state,
                        TerminalSessionState::Closing | TerminalSessionState::Closed
                    )
            })
            .map(|(id, _)| id.clone())
            .collect();
        to_close
            .into_iter()
            .filter_map(|session_id| {
                let s = sessions.get_mut(&session_id)?;
                s.state = TerminalSessionState::Closing;
                Some((
                    session_id,
                    s.actor_email.clone(),
                    s.output_tx.clone(),
                    Arc::clone(&s.session),
                ))
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
