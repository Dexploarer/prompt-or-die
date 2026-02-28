//! WebSocket client for web browsers.
//!
//! Provides same interface as NativeClient but communicates
//! over WebSocket instead of QUIC. Messages are JSON-encoded.
//! This module only compiles on WASM targets.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;
use web_sys::WebSocket;

use pod_core::Action;

use crate::protocol::{ClientConfig, ClientId, ClientMessage, ServerMessage};
use crate::snapshot::WorldSnapshot;

// ============================================================
// WEB CLIENT
// ============================================================

/// WebSocket-based client for web browsers
pub struct WebClient {
    config: ClientConfig,
    client_id: Option<ClientId>,
    websocket: WebSocket,
    connected: bool,
    /// Buffered server updates
    pending_updates: std::sync::Arc<std::sync::Mutex<Vec<ServerMessage>>>,
    /// Local prediction world state
    local_snapshot: Option<WorldSnapshot>,
    /// Actions pending transmission
    pending_actions: Vec<Action>,
}

impl WebClient {
    /// Connect to a game server via WebSocket
    pub fn connect(config: ClientConfig) -> Result<Self, ClientError> {
        let ws_url = format!(
            "ws://{}:{}",
            config.server_addr,
            config.server_port  // In production, use different port for ws
        );

        let websocket = WebSocket::new(&ws_url)
            .map_err(|_| ClientError::Connection("Failed to create WebSocket".into()))?;

        // Set binary type for better performance
        websocket.set_binary_type(web_sys::BinaryType::Arraybuffer);

        let pending_updates = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        let mut client = Self {
            config,
            client_id: None,
            websocket,
            connected: false,
            pending_updates,
            local_snapshot: None,
            pending_actions: Vec::new(),
        };

        // Set up event handlers
        client.setup_handlers()?;

        Ok(client)
    }

    /// Set up WebSocket event handlers
    fn setup_handlers(&mut self) -> Result<(), ClientError> {
        let pending_updates = self.pending_updates.clone();

        let onopen = Closure::wrap(Box::new(move |_: web_sys::Event| {
            web_sys::console::log_1(&"WebSocket connected".into());
        }) as Box<dyn FnMut(web_sys::Event)>);

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

        let onerror = Closure::wrap(Box::new(move |event: web_sys::Event| {
            web_sys::console::error_1(&format!("WebSocket error: {:?}", event).into());
        }) as Box<dyn FnMut(web_sys::Event)>);

        let onclose = Closure::wrap(Box::new(move |_: web_sys::CloseEvent| {
            web_sys::console::log_1(&"WebSocket closed".into());
        }) as Box<dyn FnMut(web_sys::CloseEvent)>);

        self.websocket
            .set_onopen(Some(onopen.as_ref().unchecked_ref()));
        self.websocket
            .set_onmessage(Some(onmessage.as_ref().unchecked_ref()));
        self.websocket
            .set_onerror(Some(onerror.as_ref().unchecked_ref()));
        self.websocket
            .set_onclose(Some(onclose.as_ref().unchecked_ref()));

        // Leak the closures so they stay alive
        onopen.forget();
        onmessage.forget();
        onerror.forget();
        onclose.forget();

        Ok(())
    }

    /// Send a raw message to the server
    fn send_message(&self, msg: ClientMessage) -> Result<(), ClientError> {
        if self.websocket.ready_state() != WebSocket::OPEN {
            return Err(ClientError::NotConnected);
        }

        let json = serde_json::to_string(&msg)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;

        self.websocket
            .send_with_str(&json)
            .map_err(|_| ClientError::Connection("Failed to send message".into()))?;

        Ok(())
    }

    /// Send a connect message to the server
    pub fn send_connect(&self) -> Result<(), ClientError> {
        let msg = ClientMessage::Connect {
            player_name: self.config.player_name.clone(),
        };

        self.send_message(msg)?;

        // Note: In actual use, you'd wait for the Welcome response
        // via poll_updates() since WebSocket is async

        Ok(())
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
        if let Ok(mut updates) = self.pending_updates.lock() {
            let mut result = Vec::new();
            std::mem::swap(&mut result, &mut updates);
            result
        } else {
            Vec::new()
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

        self.send_message(msg)?;

        self.websocket
            .close()
            .map_err(|_| ClientError::Connection("Failed to close WebSocket".into()))?;

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
        self.connected && self.websocket.ready_state() == WebSocket::OPEN
    }

    /// Update connection state (typically called after receiving Welcome)
    pub fn set_connected(&mut self, client_id: ClientId, snapshot: WorldSnapshot) {
        self.client_id = Some(client_id);
        self.local_snapshot = Some(snapshot);
        self.connected = true;
    }
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
