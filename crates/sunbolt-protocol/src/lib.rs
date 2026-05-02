use serde::{Deserialize, Serialize};

/// Initial protocol version for future control-plane and agent messages.
pub const PROTOCOL_VERSION: u16 = 1;

/// Stable identifier for a terminal session.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct TerminalSessionId(pub String);

/// Stable identifier for a managed node.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct NodeId(pub String);

/// Terminal viewport size in character cells.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

/// Browser-to-control-plane terminal messages.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalClientMessage {
    Start {
        node_id: Option<NodeId>,
        initial_size: TerminalSize,
    },
    Input {
        session_id: TerminalSessionId,
        data: String,
    },
    Resize {
        session_id: TerminalSessionId,
        size: TerminalSize,
    },
    Close {
        session_id: TerminalSessionId,
    },
    Ping {
        nonce: String,
    },
}

/// Control-plane-to-browser terminal messages.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalServerMessage {
    Started {
        session_id: TerminalSessionId,
        node_id: Option<NodeId>,
        size: TerminalSize,
    },
    Output {
        session_id: TerminalSessionId,
        data: String,
    },
    Exited {
        session_id: TerminalSessionId,
        exit: TerminalExit,
    },
    Error {
        session_id: Option<TerminalSessionId>,
        error: TerminalError,
    },
    Pong {
        nonce: String,
    },
}

/// Terminal process exit details.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub struct TerminalExit {
    pub status: Option<i32>,
}

/// Terminal protocol error details safe to send to a browser.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TerminalError {
    pub code: TerminalErrorCode,
    pub message: String,
}

/// Stable terminal protocol error codes.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalErrorCode {
    Unauthorized,
    Forbidden,
    SessionNotFound,
    InvalidMessage,
    TerminalUnavailable,
    Internal,
}

#[cfg(test)]
mod tests {
    use super::{
        NodeId, TerminalClientMessage, TerminalError, TerminalErrorCode, TerminalExit,
        TerminalServerMessage, TerminalSessionId, TerminalSize, PROTOCOL_VERSION,
    };
    use serde_json::json;

    #[test]
    fn protocol_version_starts_at_one() {
        assert_eq!(PROTOCOL_VERSION, 1);
    }

    #[test]
    fn serializes_start_message() {
        let message = TerminalClientMessage::Start {
            node_id: Some(NodeId("node-1".to_owned())),
            initial_size: TerminalSize {
                cols: 120,
                rows: 32,
            },
        };

        let value = serde_json::to_value(message).expect("start message should serialize");

        assert_eq!(
            value,
            json!({
                "type": "start",
                "node_id": "node-1",
                "initial_size": {
                    "cols": 120,
                    "rows": 32
                }
            })
        );
    }

    #[test]
    fn deserializes_client_messages() {
        let input: TerminalClientMessage = serde_json::from_value(json!({
            "type": "input",
            "session_id": "session-1",
            "data": "ls -la\n"
        }))
        .expect("input message should deserialize");

        assert_eq!(
            input,
            TerminalClientMessage::Input {
                session_id: TerminalSessionId("session-1".to_owned()),
                data: "ls -la\n".to_owned(),
            }
        );

        let resize: TerminalClientMessage = serde_json::from_value(json!({
            "type": "resize",
            "session_id": "session-1",
            "size": {
                "cols": 100,
                "rows": 40
            }
        }))
        .expect("resize message should deserialize");

        assert_eq!(
            resize,
            TerminalClientMessage::Resize {
                session_id: TerminalSessionId("session-1".to_owned()),
                size: TerminalSize {
                    cols: 100,
                    rows: 40
                },
            }
        );

        let close: TerminalClientMessage = serde_json::from_value(json!({
            "type": "close",
            "session_id": "session-1"
        }))
        .expect("close message should deserialize");

        assert_eq!(
            close,
            TerminalClientMessage::Close {
                session_id: TerminalSessionId("session-1".to_owned()),
            }
        );

        let ping: TerminalClientMessage = serde_json::from_value(json!({
            "type": "ping",
            "nonce": "nonce-1"
        }))
        .expect("ping message should deserialize");

        assert_eq!(
            ping,
            TerminalClientMessage::Ping {
                nonce: "nonce-1".to_owned(),
            }
        );
    }

    #[test]
    fn serializes_server_messages() {
        let started = serde_json::to_value(TerminalServerMessage::Started {
            session_id: TerminalSessionId("session-1".to_owned()),
            node_id: None,
            size: TerminalSize { cols: 80, rows: 24 },
        })
        .expect("started message should serialize");

        assert_eq!(
            started,
            json!({
                "type": "started",
                "session_id": "session-1",
                "node_id": null,
                "size": {
                    "cols": 80,
                    "rows": 24
                }
            })
        );

        let output = serde_json::to_value(TerminalServerMessage::Output {
            session_id: TerminalSessionId("session-1".to_owned()),
            data: "hello\n".to_owned(),
        })
        .expect("output message should serialize");

        assert_eq!(
            output,
            json!({
                "type": "output",
                "session_id": "session-1",
                "data": "hello\n"
            })
        );
    }

    #[test]
    fn deserializes_server_messages() {
        let exited: TerminalServerMessage = serde_json::from_value(json!({
            "type": "exited",
            "session_id": "session-1",
            "exit": {
                "status": 0
            }
        }))
        .expect("exited message should deserialize");

        assert_eq!(
            exited,
            TerminalServerMessage::Exited {
                session_id: TerminalSessionId("session-1".to_owned()),
                exit: TerminalExit { status: Some(0) },
            }
        );

        let error: TerminalServerMessage = serde_json::from_value(json!({
            "type": "error",
            "session_id": "session-1",
            "error": {
                "code": "terminal_unavailable",
                "message": "terminal failed to start"
            }
        }))
        .expect("error message should deserialize");

        assert_eq!(
            error,
            TerminalServerMessage::Error {
                session_id: Some(TerminalSessionId("session-1".to_owned())),
                error: TerminalError {
                    code: TerminalErrorCode::TerminalUnavailable,
                    message: "terminal failed to start".to_owned(),
                },
            }
        );

        let pong: TerminalServerMessage = serde_json::from_value(json!({
            "type": "pong",
            "nonce": "nonce-1"
        }))
        .expect("pong message should deserialize");

        assert_eq!(
            pong,
            TerminalServerMessage::Pong {
                nonce: "nonce-1".to_owned(),
            }
        );
    }
}
