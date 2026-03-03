//! WebSocket client for web browsers.
//!
//! Provides same interface as NativeClient but communicates
//! over WebSocket instead of QUIC. Messages are JSON-encoded.
//! This module only compiles on WASM targets.

#![cfg(target_arch = "wasm32")]

use std::sync::{Arc, Mutex};

use wasm_bindgen::prelude::*;
use web_sys::WebSocket;

use pod_core::Action;

use crate::protocol::{ClientConfig, ClientId, ClientMessage, ServerMessage};
use crate::snapshot::WorldSnapshot;

#[derive(Debug, Default)]
struct WebRuntimeState {
    closed: bool,
    last_error: Option<String>,
}

/// WebSocket-based client for web browsers
pub struct WebClient {
    config: ClientConfig,
    client_id: Option<ClientId>,
    websocket: Option<WebSocket>,
    connected: bool,
    /// Buffered server updates
    pending_updates: Arc<Mutex<Vec<ServerMessage>>>,
    runtime_state: Arc<Mutex<WebRuntimeState>>,
    /// Local prediction world state
    local_snapshot: Option<WorldSnapshot>,
    /// Actions pending transmission
    pending_actions: Vec<Action>,
    /// Highest server tick applied locally.
    last_server_tick: u64,
    reconnect_attempts: u32,
    next_reconnect_at_ms: f64,
}

impl WebClient {
    /// Connect to a game server via WebSocket
    pub fn connect(config: ClientConfig) -> Result<Self, ClientError> {
        let pending_updates = Arc::new(Mutex::new(Vec::new()));
        let runtime_state = Arc::new(Mutex::new(WebRuntimeState::default()));
        let mut client = Self {
            config,
            client_id: None,
            websocket: None,
            connected: false,
            pending_updates,
            runtime_state,
            local_snapshot: None,
            pending_actions: Vec::new(),
            last_server_tick: 0,
            reconnect_attempts: 0,
            next_reconnect_at_ms: 0.0,
        };
        client.open_socket()?;
        Ok(client)
    }

    fn open_socket(&mut self) -> Result<(), ClientError> {
        let ws_url = build_ws_url(&self.config);
        let websocket = WebSocket::new(&ws_url)
            .map_err(|_| ClientError::Connection("Failed to create WebSocket".into()))?;
        websocket.set_binary_type(web_sys::BinaryType::Arraybuffer);
        self.setup_handlers(&websocket)?;
        self.websocket = Some(websocket);
        self.connected = false;
        Ok(())
    }

    /// Set up WebSocket event handlers
    fn setup_handlers(&self, websocket: &WebSocket) -> Result<(), ClientError> {
        let pending_updates = self.pending_updates.clone();
        let runtime_state = self.runtime_state.clone();

        let onopen = {
            let runtime = runtime_state.clone();
            Closure::wrap(Box::new(move |_: web_sys::Event| {
                if let Ok(mut state) = runtime.lock() {
                    state.closed = false;
                    state.last_error = None;
                }
                web_sys::console::log_1(&"WebSocket connected".into());
            }) as Box<dyn FnMut(web_sys::Event)>)
        };

        let onmessage = {
            let pending = pending_updates.clone();
            Closure::wrap(Box::new(move |event: web_sys::MessageEvent| {
                if let Ok(text) = event.data().dyn_into::<js_sys::JsString>() {
                    let text_str = String::from(text);
                    if let Ok(msg) = ServerMessage::decode_json(&text_str) {
                        if let Ok(mut updates) = pending.lock() {
                            updates.push(msg);
                        }
                    }
                }
            }) as Box<dyn FnMut(web_sys::MessageEvent)>)
        };

        let onerror = {
            let runtime = runtime_state.clone();
            Closure::wrap(Box::new(move |event: web_sys::Event| {
                if let Ok(mut state) = runtime.lock() {
                    state.closed = true;
                    state.last_error = Some("websocket error".to_string());
                }
                web_sys::console::error_1(&format!("WebSocket error: {:?}", event).into());
            }) as Box<dyn FnMut(web_sys::Event)>)
        };

        let onclose = {
            let runtime = runtime_state.clone();
            Closure::wrap(Box::new(move |_: web_sys::CloseEvent| {
                if let Ok(mut state) = runtime.lock() {
                    state.closed = true;
                }
                web_sys::console::log_1(&"WebSocket closed".into());
            }) as Box<dyn FnMut(web_sys::CloseEvent)>)
        };

        websocket.set_onopen(Some(onopen.as_ref().unchecked_ref()));
        websocket.set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        websocket.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        websocket.set_onclose(Some(onclose.as_ref().unchecked_ref()));

        onopen.forget();
        onmessage.forget();
        onerror.forget();
        onclose.forget();

        Ok(())
    }

    /// Send a raw message to the server
    fn send_message(&self, msg: ClientMessage) -> Result<(), ClientError> {
        let websocket = self
            .websocket
            .as_ref()
            .ok_or_else(|| ClientError::NotConnected)?;

        if websocket.ready_state() != WebSocket::OPEN {
            return Err(ClientError::NotConnected);
        }

        let json = serde_json::to_string(&msg)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;

        websocket
            .send_with_str(&json)
            .map_err(|_| ClientError::Connection("Failed to send message".into()))?;

        Ok(())
    }

    /// Send a connect message to the server
    pub fn send_connect(&self) -> Result<(), ClientError> {
        let msg = ClientMessage::Connect {
            player_name: self.config.player_name.clone(),
        };
        self.send_message(msg)
    }

    /// Queue an action to be sent to the server
    pub fn queue_action(&mut self, action: Action) {
        self.pending_actions.push(action);
    }

    /// Send queued actions in a batch
    pub fn send_actions(&mut self, tick: u64) -> Result<(), ClientError> {
        if self.pending_actions.is_empty() {
            return Ok(());
        }

        let msg = ClientMessage::ActionBatch {
            tick,
            actions: std::mem::take(&mut self.pending_actions),
        };

        self.send_message(msg)
    }

    /// Check for updates from the server (non-blocking)
    pub fn poll_updates(&mut self) -> Vec<ServerMessage> {
        self.maybe_reconnect();

        let incoming = if let Ok(mut updates) = self.pending_updates.lock() {
            let mut drained = Vec::new();
            std::mem::swap(&mut drained, &mut updates);
            drained
        } else {
            Vec::new()
        };

        let mut outgoing = Vec::new();
        for message in incoming {
            if self.apply_server_message(&message) {
                outgoing.push(message);
            }
        }
        outgoing
    }

    fn apply_server_message(&mut self, message: &ServerMessage) -> bool {
        match message {
            ServerMessage::Welcome {
                client_id,
                tick,
                snapshot,
            } => {
                self.client_id = Some(*client_id);
                self.local_snapshot = Some(snapshot.clone());
                self.connected = true;
                self.last_server_tick = *tick;
                self.reconnect_attempts = 0;
                true
            }
            ServerMessage::StateDelta { tick, delta } => {
                if *tick < self.last_server_tick {
                    return false;
                }

                if self.last_server_tick > 0 && *tick > self.last_server_tick + 1 {
                    // Gap detected; invalidate local prediction and rebuild from incoming stream.
                    self.local_snapshot = None;
                }

                self.local_snapshot = Some(match self.local_snapshot.take() {
                    Some(snapshot) => delta.apply_to(&snapshot),
                    None => WorldSnapshot {
                        tick: *tick,
                        entities: delta.updated.clone(),
                    },
                });

                self.last_server_tick = *tick;
                true
            }
            ServerMessage::Pong { .. } | ServerMessage::EventBatch { .. } => true,
            ServerMessage::Rejected { reason } => {
                web_sys::console::error_1(&format!("Server rejected request: {reason}").into());
                true
            }
        }
    }

    fn maybe_reconnect(&mut self) {
        let should_reconnect = if let Ok(state) = self.runtime_state.lock() {
            state.closed
        } else {
            false
        };

        if !should_reconnect {
            return;
        }

        let now = js_sys::Date::now();
        if now < self.next_reconnect_at_ms {
            return;
        }

        if self.open_socket().is_ok() {
            self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
            let exp = 2u32.pow(self.reconnect_attempts.min(6));
            let delay = (250.0 * exp as f64).min(10_000.0);
            self.next_reconnect_at_ms = now + delay;
        }
    }

    /// Send a ping to measure latency
    pub fn ping(&self) -> Result<(), ClientError> {
        let timestamp = js_sys::Date::now() as u64;
        self.send_message(ClientMessage::Ping { timestamp })
    }

    /// Disconnect from the server
    pub fn disconnect(&mut self, reason: Option<&str>) -> Result<(), ClientError> {
        let msg = ClientMessage::Disconnect {
            reason: reason.map(|s| s.to_string()),
        };
        let _ = self.send_message(msg);

        if let Some(websocket) = self.websocket.as_ref() {
            websocket
                .close()
                .map_err(|_| ClientError::Connection("Failed to close WebSocket".into()))?;
        }

        self.websocket = None;
        self.connected = false;
        Ok(())
    }

    /// Get the local world snapshot (client-side prediction state)
    pub fn local_snapshot(&self) -> Option<&WorldSnapshot> {
        self.local_snapshot.as_ref()
    }

    /// Get the client ID
    pub fn client_id(&self) -> Option<ClientId> {
        self.client_id
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        let ws_open = self
            .websocket
            .as_ref()
            .map(|ws| ws.ready_state() == WebSocket::OPEN)
            .unwrap_or(false);
        self.connected && ws_open
    }

    /// Update connection state (typically called after receiving Welcome)
    pub fn set_connected(&mut self, client_id: ClientId, snapshot: WorldSnapshot) {
        self.client_id = Some(client_id);
        self.local_snapshot = Some(snapshot);
        self.connected = true;
    }
}

fn build_ws_url(config: &ClientConfig) -> String {
    let protocol = web_sys::window()
        .and_then(|window| window.location().protocol().ok())
        .unwrap_or_else(|| "http:".to_string());
    let scheme = if protocol == "https:" { "wss" } else { "ws" };
    format!("{scheme}://{}:{}", config.server_addr, config.server_port)
}

// ============================================================
// ERROR TYPES
// ============================================================

#[derive(Debug)]
pub enum ClientError {
    NotConnected,
    Connection(String),
    Serialization(String),
    Config(String),
    Timeout,
    Rejected(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::NotConnected => write!(f, "Not connected to server"),
            ClientError::Connection(msg) => write!(f, "Connection error: {}", msg),
            ClientError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
            ClientError::Config(msg) => write!(f, "Config error: {}", msg),
            ClientError::Timeout => write!(f, "Connection timeout"),
            ClientError::Rejected(msg) => write!(f, "Connection rejected: {}", msg),
        }
    }
}

impl std::error::Error for ClientError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config() {
        let config = ClientConfig {
            server_addr: "localhost".to_string(),
            server_port: 5001,
            player_name: "WebPlayer".to_string(),
            timeout_ms: 5000,
        };

        assert_eq!(config.player_name, "WebPlayer");
        assert_eq!(config.server_port, 5001);
    }
}
