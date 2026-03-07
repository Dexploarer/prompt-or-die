//! Network protocol definitions — shared message types between client and server.
//!
//! All messages are serde + bincode serializable for fast binary transmission.
//! WebSocket clients use JSON encoding instead.

use pod_core::{Action, GameEvent};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique identifier for a connected client session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClientId(pub Uuid);

impl ClientId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ClientId {
    fn default() -> Self {
        Self::new()
    }
}

/// Stable reconnect token for resuming a prior client session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ReconnectToken(pub Uuid);

impl ReconnectToken {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ReconnectToken {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// CLIENT -> SERVER MESSAGES
// ============================================================

/// Messages sent from client to server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Request to connect to the game server
    Connect {
        player_name: String,
        reconnect_token: Option<ReconnectToken>,
    },

    /// Request to disconnect from the server
    Disconnect { reason: Option<String> },

    /// Batch of actions for a specific tick
    ActionBatch { tick: u64, actions: Vec<Action> },

    /// Request an immediate authoritative full snapshot after drift, gap, or
    /// local baseline loss.
    RequestFullSnapshot {
        last_known_tick: Option<u64>,
        last_known_digest: Option<u64>,
    },

    /// Enable or disable debug-only cross-agent telemetry streaming.
    SetDebugTelemetry { enabled: bool },

    /// Ping request — measures round-trip time
    Ping { timestamp: u64 },
}

impl ClientMessage {
    /// Encode to binary via bincode
    pub fn encode(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Decode from binary via bincode
    pub fn decode(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }
}

// ============================================================
// SERVER -> CLIENT MESSAGES
// ============================================================

/// Messages sent from server to client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ServerMessage {
    /// Initial welcome packet — sent on successful connection
    Welcome {
        client_id: ClientId,
        reconnect_token: ReconnectToken,
        tick: u64,
        controlled_entity: Option<u64>,
        authoritative_digest: u64,
        snapshot: super::snapshot::WorldSnapshot,
    },

    /// Delta update — only changed state since last snapshot
    StateDelta {
        tick: u64,
        acknowledged_action_tick: Option<u64>,
        authoritative_digest: u64,
        is_full_snapshot: bool,
        delta: super::snapshot::StateDelta,
    },

    /// Batch of game events
    EventBatch { tick: u64, events: Vec<GameEvent> },

    /// Raw authoritative telemetry payload for debug/editor consumers.
    TickTelemetry { frame_json: String },

    /// Generic TOON debug document for editor/debug consumers.
    DebugDocument { document: String },

    /// Response to ping request
    Pong { client_ts: u64, server_ts: u64 },

    /// Connection rejected
    Rejected { reason: String },
}

impl ServerMessage {
    /// Encode to binary via bincode
    pub fn encode(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Decode from binary via bincode
    pub fn decode(bytes: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(bytes)
    }

    /// Encode to JSON (for WebSocket clients)
    pub fn encode_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Decode from JSON (for WebSocket clients)
    pub fn decode_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

// ============================================================
// NETWORKING CONFIGURATION
// ============================================================

/// Configuration for the game server
#[derive(Debug, Clone)]
pub struct ServerConfig {
    /// Maximum number of simultaneous clients
    pub max_clients: usize,
    /// Server tick rate (ticks per second)
    pub tick_rate: u32,
    /// How often to send full snapshots (in ticks)
    pub snapshot_interval: u64,
    /// QUIC server address to bind to
    pub bind_addr: String,
    /// QUIC server port
    pub bind_port: u16,
    /// Whether to enable WebSocket fallback endpoint
    pub enable_websocket: bool,
    /// WebSocket endpoint port
    pub websocket_port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            max_clients: 64,
            tick_rate: 60,
            snapshot_interval: 10, // Full snapshot every ~167ms
            bind_addr: "0.0.0.0".to_string(),
            bind_port: 5000,
            enable_websocket: true,
            websocket_port: 5001,
        }
    }
}

// ============================================================
// CLIENT CONFIGURATION
// ============================================================

/// Configuration for connecting to a game server
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Server address
    pub server_addr: String,
    /// Server port
    pub server_port: u16,
    /// Player name
    pub player_name: String,
    /// Connection timeout in milliseconds
    pub timeout_ms: u64,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            server_addr: "localhost".to_string(),
            server_port: 5000,
            player_name: "Player".to_string(),
            timeout_ms: 5000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::{decode_toon_value, TickTelemetryFrame, VersionedTickTelemetry};

    #[test]
    fn test_client_message_roundtrip() {
        let reconnect_token = ReconnectToken::new();
        let msg = ClientMessage::Connect {
            player_name: "TestPlayer".into(),
            reconnect_token: Some(reconnect_token),
        };

        let encoded = msg.encode().unwrap();
        let decoded = ClientMessage::decode(&encoded).unwrap();

        if let ClientMessage::Connect {
            player_name,
            reconnect_token: decoded_token,
        } = decoded
        {
            assert_eq!(player_name, "TestPlayer");
            assert_eq!(decoded_token, Some(reconnect_token));
        } else {
            panic!("Wrong message type");
        }
    }

    #[test]
    fn test_full_snapshot_request_roundtrip() {
        let msg = ClientMessage::RequestFullSnapshot {
            last_known_tick: Some(42),
            last_known_digest: Some(99),
        };

        let encoded = msg.encode().unwrap();
        let decoded = ClientMessage::decode(&encoded).unwrap();

        match decoded {
            ClientMessage::RequestFullSnapshot {
                last_known_tick,
                last_known_digest,
            } => {
                assert_eq!(last_known_tick, Some(42));
                assert_eq!(last_known_digest, Some(99));
            }
            other => panic!("Wrong message type: {other:?}"),
        }
    }

    #[test]
    fn test_set_debug_telemetry_roundtrip() {
        let msg = ClientMessage::SetDebugTelemetry { enabled: true };
        let encoded = msg.encode().unwrap();
        let decoded = ClientMessage::decode(&encoded).unwrap();

        match decoded {
            ClientMessage::SetDebugTelemetry { enabled } => assert!(enabled),
            other => panic!("Wrong message type: {other:?}"),
        }
    }

    #[test]
    fn test_server_message_json() {
        let _config = ServerConfig::default();
        let snapshot = crate::WorldSnapshot::default();

        let msg = ServerMessage::Welcome {
            client_id: ClientId::new(),
            reconnect_token: ReconnectToken::new(),
            tick: 100,
            controlled_entity: Some(42),
            authoritative_digest: snapshot.digest(),
            snapshot,
        };

        let json = msg.encode_json().unwrap();
        let decoded = ServerMessage::decode_json(&json).unwrap();

        match decoded {
            ServerMessage::Welcome {
                tick,
                controlled_entity,
                ..
            } => {
                assert_eq!(tick, 100);
                assert_eq!(controlled_entity, Some(42));
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_tick_telemetry_json_roundtrip() {
        let msg = ServerMessage::TickTelemetry {
            frame_json: VersionedTickTelemetry::new(TickTelemetryFrame::empty(7))
                .to_toon_document(),
        };
        let json = msg.encode_json().unwrap();
        let decoded = ServerMessage::decode_json(&json).unwrap();

        match decoded {
            ServerMessage::TickTelemetry { frame_json } => {
                let value =
                    decode_toon_value(&frame_json).expect("tick telemetry TOON should decode");
                assert_eq!(value["document_type"], "versioned_tick_telemetry");
                assert_eq!(value["payload"]["payload"]["tick"], 7);
            }
            other => panic!("Wrong message type: {other:?}"),
        }
    }

    #[test]
    fn test_debug_document_json_roundtrip() {
        let msg = ServerMessage::DebugDocument {
            document: VersionedTickTelemetry::new(TickTelemetryFrame::empty(9)).to_toon_document(),
        };
        let json = msg.encode_json().unwrap();
        let decoded = ServerMessage::decode_json(&json).unwrap();

        match decoded {
            ServerMessage::DebugDocument { document } => {
                let value =
                    decode_toon_value(&document).expect("debug document TOON should decode");
                assert_eq!(value["document_type"], "versioned_tick_telemetry");
                assert_eq!(value["payload"]["payload"]["tick"], 9);
            }
            other => panic!("Wrong message type: {other:?}"),
        }
    }
}
