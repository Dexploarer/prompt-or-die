//! WebSocket client for web browsers.
//!
//! Provides same interface as NativeClient but communicates
//! over WebSocket instead of QUIC. Messages are JSON-encoded.
//! This module only compiles on WASM targets.

#![cfg(target_arch = "wasm32")]

use std::sync::{Arc, Mutex};

use wasm_bindgen::prelude::*;
use web_sys::WebSocket;

use pod_core::{decode_toon_value, Action};

use crate::protocol::{ClientConfig, ClientId, ClientMessage, ReconnectToken, ServerMessage};
use crate::snapshot::{
    apply_authoritative_update, build_catch_up_diagnostics, build_rollback_preview,
    compose_presentation_snapshot, CatchUpDiagnostics, InterpolatedSnapshot, PredictedActionBatch,
    ReconciliationReport, RecoveryRequestState, RenderClock, RollbackPreview,
    SnapshotInterpolationBuffer, WorldSnapshot,
};

const RECOVERY_REQUEST_RETRY_TICKS: u64 = 5;

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
    /// Authoritative world state as last confirmed by the server.
    authoritative_snapshot: Option<WorldSnapshot>,
    /// Local prediction world state
    local_snapshot: Option<WorldSnapshot>,
    /// Entity controlled by this client, if assigned by the server.
    controlled_entity: Option<u64>,
    /// Actions pending transmission
    pending_actions: Vec<Action>,
    /// Sent but not yet acknowledged action batches.
    prediction_history: Vec<PredictedActionBatch>,
    /// Highest server tick applied locally.
    last_server_tick: u64,
    /// Highest action tick acknowledged by the server.
    last_acknowledged_action_tick: Option<u64>,
    /// Highest event tick accepted from server.
    last_event_tick: u64,
    /// Reconnect token issued by server and reused across reconnects.
    reconnect_token: Option<ReconnectToken>,
    /// Last authoritative reconciliation outcome.
    last_reconciliation: Option<ReconciliationReport>,
    /// Most recent debug telemetry payload received from the server.
    last_debug_telemetry_json: Option<String>,
    /// Most recent debug document of any supported TOON kind.
    last_debug_document: Option<String>,
    /// Pending TOON debug documents gathered since the last drain.
    pending_debug_documents: Vec<String>,
    /// Recovery request throttle/telemetry for full-snapshot resync.
    recovery_state: RecoveryRequestState,
    /// Authoritative history for presentation smoothing.
    render_buffer: SnapshotInterpolationBuffer,
    /// Presentation clock for interpolation/catch-up recovery.
    render_clock: RenderClock,
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
            authoritative_snapshot: None,
            local_snapshot: None,
            controlled_entity: None,
            pending_actions: Vec::new(),
            prediction_history: Vec::new(),
            last_server_tick: 0,
            last_acknowledged_action_tick: None,
            last_event_tick: 0,
            reconnect_token: None,
            last_reconciliation: None,
            last_debug_telemetry_json: None,
            last_debug_document: None,
            pending_debug_documents: Vec::new(),
            recovery_state: RecoveryRequestState::default(),
            render_buffer: SnapshotInterpolationBuffer::default(),
            render_clock: RenderClock::default(),
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
        let connect_payload = serde_json::to_string(&ClientMessage::Connect {
            player_name: self.config.player_name.clone(),
            reconnect_token: self.reconnect_token,
        })
        .ok();
        self.setup_handlers(&websocket, connect_payload)?;
        self.websocket = Some(websocket);
        self.connected = false;
        Ok(())
    }

    /// Set up WebSocket event handlers
    fn setup_handlers(
        &self,
        websocket: &WebSocket,
        connect_payload: Option<String>,
    ) -> Result<(), ClientError> {
        let pending_updates = self.pending_updates.clone();
        let runtime_state = self.runtime_state.clone();

        let onopen = {
            let runtime = runtime_state.clone();
            let connect_payload = connect_payload.clone();
            Closure::wrap(Box::new(move |event: web_sys::Event| {
                if let Ok(mut state) = runtime.lock() {
                    state.closed = false;
                    state.last_error = None;
                }
                if let Some(payload) = connect_payload.as_ref() {
                    if let Some(target) = event.target() {
                        if let Ok(ws) = target.dyn_into::<WebSocket>() {
                            if let Err(err) = ws.send_with_str(payload) {
                                web_sys::console::error_1(
                                    &format!("Failed to send connect payload: {:?}", err).into(),
                                );
                            }
                        }
                    }
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

        let json =
            serde_json::to_string(&msg).map_err(|e| ClientError::Serialization(e.to_string()))?;

        websocket
            .send_with_str(&json)
            .map_err(|_| ClientError::Connection("Failed to send message".into()))?;

        Ok(())
    }

    /// Send a connect message to the server
    pub fn send_connect(&self) -> Result<(), ClientError> {
        let msg = ClientMessage::Connect {
            player_name: self.config.player_name.clone(),
            reconnect_token: self.reconnect_token,
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

        let actions = std::mem::take(&mut self.pending_actions);
        let msg = ClientMessage::ActionBatch {
            tick,
            actions: actions.clone(),
        };

        if let Err(err) = self.send_message(msg) {
            self.pending_actions = actions;
            return Err(err);
        }

        self.prediction_history
            .push(PredictedActionBatch { tick, actions });
        self.rebuild_predicted_snapshot();
        Ok(())
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
                reconnect_token,
                tick,
                controlled_entity,
                authoritative_digest,
                snapshot,
            } => {
                self.client_id = Some(*client_id);
                self.reconnect_token = Some(*reconnect_token);
                self.controlled_entity = *controlled_entity;
                self.authoritative_snapshot = Some(snapshot.clone());
                self.local_snapshot = Some(snapshot.clone());
                self.ingest_authoritative_snapshot();
                self.prediction_history.clear();
                self.connected = true;
                self.last_server_tick = *tick;
                self.last_acknowledged_action_tick = None;
                self.last_event_tick = *tick;
                self.recovery_state.clear();
                self.reconnect_attempts = 0;
                self.last_reconciliation = Some(ReconciliationReport {
                    authoritative_tick: *tick,
                    authoritative_digest: *authoritative_digest,
                    acknowledged_action_tick: None,
                    pending_action_batches: 0,
                    replayed_action_count: 0,
                    predicted_digest: self.local_snapshot.as_ref().map(WorldSnapshot::digest),
                    used_hard_resync: false,
                });
                true
            }
            ServerMessage::StateDelta {
                tick,
                acknowledged_action_tick,
                authoritative_digest,
                is_full_snapshot,
                delta,
            } => {
                if *tick < self.last_server_tick {
                    return false;
                }

                let gap_detected = self.last_server_tick > 0 && *tick > self.last_server_tick + 1;
                if gap_detected && !*is_full_snapshot {
                    self.request_full_snapshot(*tick);
                    self.authoritative_snapshot = None;
                    self.local_snapshot = None;
                    self.clear_presentation_state();
                    self.record_reconciliation(*tick, *authoritative_digest, true);
                    return false;
                }

                let previous = if *is_full_snapshot || gap_detected {
                    None
                } else {
                    self.authoritative_snapshot.as_ref()
                };

                let authoritative_snapshot = match apply_authoritative_update(
                    previous,
                    *tick,
                    *is_full_snapshot,
                    delta,
                    *authoritative_digest,
                ) {
                    Ok(snapshot) => snapshot,
                    Err(err) => {
                        web_sys::console::error_1(
                            &format!("Authoritative update rejected at tick {}: {:?}", tick, err)
                                .into(),
                        );
                        self.request_full_snapshot(*tick);
                        self.authoritative_snapshot = None;
                        self.local_snapshot = None;
                        self.clear_presentation_state();
                        self.record_reconciliation(*tick, *authoritative_digest, true);
                        return false;
                    }
                };

                self.authoritative_snapshot = Some(authoritative_snapshot);
                if *is_full_snapshot {
                    self.recovery_state.clear();
                }
                self.ingest_authoritative_snapshot();
                self.prune_acknowledged_predictions(*acknowledged_action_tick);
                self.rebuild_predicted_snapshot();
                self.last_server_tick = *tick;
                self.record_reconciliation(*tick, *authoritative_digest, gap_detected);
                true
            }
            ServerMessage::EventBatch { tick, .. } => {
                if *tick < self.last_event_tick {
                    return false;
                }
                self.last_event_tick = *tick;
                true
            }
            ServerMessage::Pong { .. } => true,
            ServerMessage::TickTelemetry { frame_json } => {
                self.record_debug_document(frame_json.clone());
                true
            }
            ServerMessage::DebugDocument { document } => {
                self.record_debug_document(document.clone());
                true
            }
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

    /// Opt-in to raw debug telemetry from the authoritative server.
    pub fn set_debug_telemetry_enabled(&self, enabled: bool) -> Result<(), ClientError> {
        self.send_message(ClientMessage::SetDebugTelemetry { enabled })
    }

    /// Focus debug summaries on a specific entity without widening gameplay
    /// visibility.
    pub fn set_debug_focus_entity(&self, entity_id: Option<u64>) -> Result<(), ClientError> {
        self.send_message(ClientMessage::SetDebugFocus { entity_id })
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
        self.recovery_state.clear();
        self.last_debug_telemetry_json = None;
        self.last_debug_document = None;
        self.pending_debug_documents.clear();
        self.clear_presentation_state();
        Ok(())
    }

    /// Get the local world snapshot (client-side prediction state)
    pub fn local_snapshot(&self) -> Option<&WorldSnapshot> {
        self.local_snapshot.as_ref()
    }

    /// Sample a smoothed presentation snapshot for rendering.
    pub fn presentation_snapshot(
        &mut self,
        frame_delta_seconds: f32,
    ) -> Option<InterpolatedSnapshot> {
        let latest_tick = self.render_buffer.latest_tick()?;
        let target_tick = self.render_clock.advance(latest_tick, frame_delta_seconds);
        let sampled = self.render_buffer.sample(target_tick)?;
        Some(compose_presentation_snapshot(
            sampled,
            self.local_snapshot.as_ref(),
            self.controlled_entity,
        ))
    }

    /// Get the last authoritative world snapshot confirmed by the server.
    pub fn authoritative_snapshot(&self) -> Option<&WorldSnapshot> {
        self.authoritative_snapshot.as_ref()
    }

    /// Current presentation tick after interpolation/catch-up correction.
    pub fn presentation_tick(&self) -> Option<f32> {
        self.render_clock.current_tick()
    }

    /// Get the most recent reconciliation report.
    pub fn last_reconciliation(&self) -> Option<&ReconciliationReport> {
        self.last_reconciliation.as_ref()
    }

    pub fn last_debug_telemetry_json(&self) -> Option<&str> {
        self.last_debug_telemetry_json.as_deref()
    }

    pub fn last_debug_telemetry_document(&self) -> Option<&str> {
        self.last_debug_telemetry_json()
    }

    pub fn last_debug_document(&self) -> Option<&str> {
        self.last_debug_document
            .as_deref()
            .or_else(|| self.last_debug_telemetry_document())
    }

    pub fn drain_debug_documents(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_debug_documents)
    }

    /// Inspect the local rollback/replay path from a chosen authoritative tick.
    pub fn rollback_preview(&self, rewind_tick: Option<u64>) -> Option<RollbackPreview> {
        let rewind_tick = rewind_tick.or_else(|| {
            self.authoritative_snapshot
                .as_ref()
                .map(|snapshot| snapshot.tick)
        })?;
        build_rollback_preview(
            &self.render_buffer,
            rewind_tick,
            self.controlled_entity,
            &self.prediction_history,
        )
    }

    /// Rewind to the newest retained authoritative snapshot at or before `tick`.
    pub fn rewind_authoritative_snapshot(&self, tick: u64) -> Option<WorldSnapshot> {
        self.render_buffer.rewind_to(tick)
    }

    /// Summarize current presentation drift and prediction recovery state.
    pub fn catch_up_diagnostics(&self) -> CatchUpDiagnostics {
        build_catch_up_diagnostics(
            &self.render_buffer,
            self.authoritative_snapshot.as_ref(),
            self.local_snapshot.as_ref(),
            self.controlled_entity,
            &self.prediction_history,
            &self.render_clock,
            &self.recovery_state,
        )
    }

    /// Get the number of unacknowledged predicted action batches.
    pub fn pending_prediction_batches(&self) -> usize {
        self.prediction_history.len()
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
        self.authoritative_snapshot = Some(snapshot.clone());
        self.local_snapshot = Some(snapshot);
        self.ingest_authoritative_snapshot();
        self.connected = true;
        self.recovery_state.clear();
    }

    fn prune_acknowledged_predictions(&mut self, acknowledged_action_tick: Option<u64>) {
        let Some(acknowledged_action_tick) = acknowledged_action_tick else {
            return;
        };

        let acknowledged_action_tick = self
            .last_acknowledged_action_tick
            .map(|last| last.max(acknowledged_action_tick))
            .unwrap_or(acknowledged_action_tick);
        self.last_acknowledged_action_tick = Some(acknowledged_action_tick);
        self.prediction_history
            .retain(|batch| batch.tick > acknowledged_action_tick);
    }

    fn rebuild_predicted_snapshot(&mut self) {
        self.local_snapshot = self.authoritative_snapshot.as_ref().map(|snapshot| {
            snapshot.replay_predicted_actions(self.controlled_entity, &self.prediction_history)
        });
    }

    fn ingest_authoritative_snapshot(&mut self) {
        if let Some(snapshot) = self.authoritative_snapshot.as_ref() {
            self.render_buffer.push(snapshot.clone());
        }
    }

    fn clear_presentation_state(&mut self) {
        self.render_buffer.clear();
        self.render_clock.reset();
    }

    fn request_full_snapshot(&mut self, observed_server_tick: u64) {
        let current_server_tick = observed_server_tick.max(
            self.authoritative_snapshot
                .as_ref()
                .map(|snapshot| snapshot.tick)
                .unwrap_or(self.last_server_tick),
        );
        if !self
            .recovery_state
            .can_request(current_server_tick, RECOVERY_REQUEST_RETRY_TICKS)
        {
            return;
        }

        let last_known_tick = self
            .authoritative_snapshot
            .as_ref()
            .map(|snapshot| snapshot.tick)
            .or(Some(self.last_server_tick).filter(|tick| *tick > 0));
        let last_known_digest = self
            .authoritative_snapshot
            .as_ref()
            .map(WorldSnapshot::digest);

        match self.send_message(ClientMessage::RequestFullSnapshot {
            last_known_tick,
            last_known_digest,
        }) {
            Ok(()) => {
                self.recovery_state
                    .record_request(current_server_tick, last_known_digest);
            }
            Err(err) => {
                web_sys::console::warn_1(
                    &format!("Failed to request full snapshot recovery: {err}").into(),
                );
            }
        }
    }

    fn record_reconciliation(
        &mut self,
        authoritative_tick: u64,
        authoritative_digest: u64,
        used_hard_resync: bool,
    ) {
        let replayed_action_count = self
            .prediction_history
            .iter()
            .map(|batch| batch.actions.len())
            .sum();
        self.last_reconciliation = Some(ReconciliationReport {
            authoritative_tick,
            authoritative_digest,
            acknowledged_action_tick: self.last_acknowledged_action_tick,
            pending_action_batches: self.prediction_history.len(),
            replayed_action_count,
            predicted_digest: self.local_snapshot.as_ref().map(WorldSnapshot::digest),
            used_hard_resync,
        });
    }

    fn record_debug_document(&mut self, document: String) {
        if decode_toon_value(&document)
            .ok()
            .and_then(|value| value["document_type"].as_str().map(str::to_owned))
            .as_deref()
            == Some("versioned_tick_telemetry")
        {
            self.last_debug_telemetry_json = Some(document.clone());
        }

        self.last_debug_document = Some(document.clone());
        self.pending_debug_documents.push(document);
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
