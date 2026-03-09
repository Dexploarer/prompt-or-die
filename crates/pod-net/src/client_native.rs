//! Native QUIC client — for desktop platforms.
//!
//! Uses quinn for QUIC protocol (UDP-based, faster than TCP).
//! This module only compiles on non-WASM platforms.

#![cfg(not(target_arch = "wasm32"))]

use std::sync::{Arc, Mutex};

use log::{debug, error, info, warn};
use quinn::Endpoint;
use tokio::sync::mpsc;

use pod_core::{decode_toon_value, Action};

use crate::protocol::{
    ClientConfig as ProtoClientConfig, ClientId, ClientMessage, ReconnectToken, ServerMessage,
};
use crate::snapshot::{
    apply_authoritative_update, build_catch_up_diagnostics, build_rollback_preview,
    compose_presentation_snapshot, CatchUpDiagnostics, InterpolatedSnapshot, PredictedActionBatch,
    ReconciliationReport, RecoveryRequestState, RenderClock, RollbackPreview,
    SnapshotInterpolationBuffer, WorldSnapshot,
};

const RECOVERY_REQUEST_RETRY_TICKS: u64 = 5;

#[derive(Debug, Default)]
struct NativeRuntimeState {
    closed: bool,
    last_error: Option<String>,
    closed_explicitly: bool,
}

// ============================================================
// NATIVE CLIENT
// ============================================================

/// QUIC-based client for native platforms
pub struct NativeClient {
    config: ProtoClientConfig,
    client_id: Option<ClientId>,
    connection: Option<quinn::Connection>,
    endpoint: Endpoint,
    runtime_state: Arc<Mutex<NativeRuntimeState>>,
    /// Channel for receiving server messages
    rx: mpsc::Receiver<ServerMessage>,
    /// Channel for sending server messages (internal)
    tx: mpsc::Sender<ServerMessage>,
    reader_task: Option<tokio::task::JoinHandle<()>>,
    /// Buffered server updates
    pending_updates: Vec<ServerMessage>,
    /// Authoritative world state as last confirmed by the server.
    authoritative_snapshot: Option<WorldSnapshot>,
    /// Local prediction world state
    local_snapshot: Option<WorldSnapshot>,
    /// Entity controlled by this client, if one has been assigned by the server.
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
    /// Reconnect token issued by server on first successful connect.
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
    /// Last time any authoritative server traffic was observed.
    last_server_activity_at: std::time::Instant,
    /// Latest round-trip latency sample derived from ping/pong.
    last_ping_rtt_ms: Option<u64>,
    /// Smoothed latency jitter estimate derived from recent RTT deltas.
    ping_jitter_ms: Option<f32>,
    reconnect_attempts: u32,
    next_reconnect_at: Option<std::time::Instant>,
}

impl NativeClient {
    /// Connect to a game server
    pub async fn connect(config: ProtoClientConfig) -> Result<Self, ClientError> {
        // Create QUIC endpoint (quinn 0.11 API)
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())
            .map_err(|e| ClientError::Connection(e.to_string()))?;

        let insecure_tls = std::env::var("POD_INSECURE_SKIP_TLS_VERIFY")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);

        let tls_config = if insecure_tls {
            warn_insecure_tls_mode();
            rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoServerVerification))
                .with_no_client_auth()
        } else {
            let mut roots = rustls::RootCertStore::empty();
            if let Ok(path) = std::env::var("POD_TRUSTED_CERT_DER") {
                let cert_der = std::fs::read(&path).map_err(|e| {
                    ClientError::Config(format!("Failed reading POD_TRUSTED_CERT_DER: {e}"))
                })?;
                roots
                    .add(rustls::pki_types::CertificateDer::from(cert_der))
                    .map_err(|e| {
                        ClientError::Config(format!("Invalid trusted DER certificate: {e}"))
                    })?;
            } else {
                return Err(ClientError::Config(
                    "TLS verification requires POD_TRUSTED_CERT_DER to point to a trusted server certificate (DER).".into(),
                ));
            }
            rustls::ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth()
        };

        let quic_client_config = quinn::crypto::rustls::QuicClientConfig::try_from(tls_config)
            .map_err(|e| ClientError::Config(format!("QUIC client config: {}", e)))?;
        let mut client_config = quinn::ClientConfig::new(Arc::new(quic_client_config));
        client_config.transport_config(Arc::new(quinn::TransportConfig::default()));

        let mut endpoint = endpoint;
        endpoint.set_default_client_config(client_config);

        let addr = format!("{}:{}", config.server_addr, config.server_port)
            .parse()
            .map_err(|e| ClientError::Config(format!("Invalid address: {}", e)))?;

        info!("Connecting to server at {}", addr);

        let connecting = endpoint
            .connect(addr, "localhost")
            .map_err(|e| ClientError::Connection(e.to_string()))?;

        let connection = connecting
            .await
            .map_err(|e| ClientError::Connection(e.to_string()))?;

        info!("Connected to server");

        let (tx, rx) = mpsc::channel(128);
        let runtime_state = Arc::new(Mutex::new(NativeRuntimeState::default()));

        let mut client = Self {
            config,
            client_id: None,
            connection: Some(connection),
            endpoint,
            runtime_state,
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
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
            last_server_activity_at: std::time::Instant::now(),
            last_ping_rtt_ms: None,
            ping_jitter_ms: None,
            reconnect_attempts: 0,
            next_reconnect_at: None,
        };

        // Send connect message
        client.send_connect().await?;
        client.start_reader_loop();

        Ok(client)
    }

    /// Send a connect message to the server
    async fn send_connect(&mut self) -> Result<(), ClientError> {
        let resuming_session = self.reconnect_token.is_some();
        let msg = ClientMessage::Connect {
            player_name: self.config.player_name.clone(),
            reconnect_token: self.reconnect_token,
        };

        self.send_message(msg).await?;

        // Wait for welcome response (with timeout)
        tokio::time::timeout(
            std::time::Duration::from_millis(self.config.timeout_ms),
            self.receive_message(),
        )
        .await
        .map_err(|_| ClientError::Timeout)?
        .and_then(|msg| {
            if let ServerMessage::Welcome {
                client_id,
                reconnect_token,
                controlled_entity,
                acknowledged_action_tick,
                authoritative_digest,
                snapshot,
                ..
            } = msg
            {
                self.client_id = Some(client_id);
                self.controlled_entity = controlled_entity;
                self.authoritative_snapshot = Some(snapshot.clone());
                self.local_snapshot = Some(snapshot);
                self.ingest_authoritative_snapshot();
                self.reconnect_token = Some(reconnect_token);
                if resuming_session {
                    self.prune_acknowledged_predictions(acknowledged_action_tick);
                    self.rebuild_predicted_snapshot();
                } else {
                    self.prediction_history.clear();
                    self.last_acknowledged_action_tick = None;
                }
                self.recovery_state.clear();
                let replayed_action_count = self
                    .prediction_history
                    .iter()
                    .map(|batch| batch.actions.len())
                    .sum();
                self.last_reconciliation = Some(ReconciliationReport {
                    authoritative_tick: self
                        .local_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.tick)
                        .unwrap_or_default(),
                    authoritative_digest,
                    acknowledged_action_tick,
                    pending_action_batches: self.prediction_history.len(),
                    replayed_action_count,
                    predicted_digest: self.local_snapshot.as_ref().map(WorldSnapshot::digest),
                    used_hard_resync: false,
                });
                self.last_server_activity_at = std::time::Instant::now();
                if let Ok(mut runtime) = self.runtime_state.lock() {
                    runtime.closed = false;
                    runtime.closed_explicitly = false;
                    runtime.last_error = None;
                }
                debug!("Received welcome from server, client_id: {}", client_id.0);
                Ok(())
            } else if let ServerMessage::Rejected { reason } = msg {
                Err(ClientError::Rejected(reason))
            } else {
                error!("Unexpected response to connect message");
                Err(ClientError::Connection(
                    "Unexpected response to connect message".into(),
                ))
            }
        })?;

        Ok(())
    }

    fn start_reader_loop(&mut self) {
        let Some(connection) = self.connection.as_ref().cloned() else {
            return;
        };
        let tx = self.tx.clone();
        let runtime_state = self.runtime_state.clone();
        self.reader_task = Some(tokio::spawn(async move {
            loop {
                let mut recv = match connection.accept_uni().await {
                    Ok(recv) => recv,
                    Err(err) => {
                        if let Ok(mut runtime) = runtime_state.lock() {
                            runtime.closed = true;
                            if !runtime.closed_explicitly {
                                runtime.last_error = Some(err.to_string());
                            }
                        }
                        error!("Server stream accept failed: {}", err);
                        break;
                    }
                };

                let payload = match recv.read_to_end(64 * 1024).await {
                    Ok(payload) => payload,
                    Err(err) => {
                        error!("Server stream read failed: {}", err);
                        continue;
                    }
                };

                let message = match ServerMessage::decode(&payload) {
                    Ok(message) => message,
                    Err(err) => {
                        error!("Server message decode failed: {}", err);
                        continue;
                    }
                };

                if tx.send(message).await.is_err() {
                    if let Ok(mut runtime) = runtime_state.lock() {
                        runtime.closed = true;
                        if !runtime.closed_explicitly {
                            runtime.last_error = Some("reader channel closed".to_string());
                        }
                    }
                    break;
                }
            }
        }));
    }

    /// Send a raw message to the server
    async fn send_message(&mut self, msg: ClientMessage) -> Result<(), ClientError> {
        let connection = self
            .connection
            .as_ref()
            .ok_or_else(|| ClientError::NotConnected)?;

        let bytes = msg
            .encode()
            .map_err(|e| ClientError::Serialization(e.to_string()))?;

        let mut send = connection
            .open_uni()
            .await
            .map_err(|e| ClientError::Connection(e.to_string()))?;

        send.write_all(&bytes)
            .await
            .map_err(|e| ClientError::Connection(e.to_string()))?;

        send.finish()
            .map_err(|e| ClientError::Connection(e.to_string()))?;

        Ok(())
    }

    /// Receive a message from the server
    async fn receive_message(&mut self) -> Result<ServerMessage, ClientError> {
        let connection = self
            .connection
            .as_ref()
            .ok_or_else(|| ClientError::NotConnected)?;

        let mut recv = connection
            .accept_uni()
            .await
            .map_err(|e| ClientError::Connection(e.to_string()))?;

        let mut buf = vec![0u8; 8192]; // Read up to 8KB per message
        let n = recv
            .read(&mut buf)
            .await
            .map_err(|e| ClientError::Connection(e.to_string()))?
            .ok_or_else(|| ClientError::Connection("Stream closed".into()))?;

        buf.truncate(n);

        let msg =
            ServerMessage::decode(&buf).map_err(|e| ClientError::Serialization(e.to_string()))?;

        Ok(msg)
    }

    /// Queue an action to be sent to the server
    pub fn queue_action(&mut self, action: Action) {
        if self.pending_actions.len() >= self.config.max_pending_actions {
            warn!(
                "Dropping queued client action because pending action buffer is saturated at {}",
                self.config.max_pending_actions
            );
            return;
        }
        self.pending_actions.push(action);
    }

    /// Send queued actions in a batch
    pub async fn send_actions(&mut self, tick: u64) -> Result<(), ClientError> {
        if self.pending_actions.is_empty() {
            return Ok(());
        }

        if self.prediction_history.len() >= self.config.max_pending_actions {
            let reason = format!(
                "Pending action backlog saturated ({})",
                self.prediction_history.len()
            );
            warn!("{reason}");
            return Err(ClientError::Rejected(reason));
        }

        let actions = std::mem::take(&mut self.pending_actions);
        let msg = ClientMessage::ActionBatch {
            tick,
            actions: actions.clone(),
        };

        if let Err(err) = self.send_message(msg).await {
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
        self.enforce_heartbeat_timeout_at(std::time::Instant::now());

        while let Ok(msg) = self.rx.try_recv() {
            if self.apply_server_message(&msg) {
                self.pending_updates.push(msg);
            }
        }

        std::mem::take(&mut self.pending_updates)
    }

    fn apply_server_message(&mut self, message: &ServerMessage) -> bool {
        self.last_server_activity_at = std::time::Instant::now();
        match message {
            ServerMessage::Welcome {
                client_id,
                reconnect_token,
                tick,
                controlled_entity,
                acknowledged_action_tick,
                authoritative_digest,
                snapshot,
            } => {
                let resuming_session = self.reconnect_token.is_some();
                self.client_id = Some(*client_id);
                self.controlled_entity = *controlled_entity;
                self.authoritative_snapshot = Some(snapshot.clone());
                self.local_snapshot = Some(snapshot.clone());
                self.ingest_authoritative_snapshot();
                if resuming_session {
                    self.prune_acknowledged_predictions(*acknowledged_action_tick);
                    self.rebuild_predicted_snapshot();
                } else {
                    self.prediction_history.clear();
                    self.last_acknowledged_action_tick = None;
                }
                self.recovery_state.clear();
                self.last_server_tick = *tick;
                self.last_event_tick = *tick;
                self.reconnect_token = Some(*reconnect_token);
                let replayed_action_count = self
                    .prediction_history
                    .iter()
                    .map(|batch| batch.actions.len())
                    .sum();
                self.last_reconciliation = Some(ReconciliationReport {
                    authoritative_tick: *tick,
                    authoritative_digest: *authoritative_digest,
                    acknowledged_action_tick: *acknowledged_action_tick,
                    pending_action_batches: self.prediction_history.len(),
                    replayed_action_count,
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
                        error!("Authoritative update rejected at tick {}: {:?}", tick, err);
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
            ServerMessage::Pong { client_ts, .. } => {
                self.record_pong_latency(system_time_millis(), *client_ts);
                true
            }
            ServerMessage::TickTelemetry { frame_json } => {
                self.record_debug_document(frame_json.clone());
                true
            }
            ServerMessage::DebugDocument { document } => {
                self.record_debug_document(document.clone());
                true
            }
            ServerMessage::Rejected { reason } => {
                error!("Server rejected request: {}", reason);
                true
            }
        }
    }

    /// Send a ping to measure latency
    pub async fn ping(&mut self) -> Result<(), ClientError> {
        self.send_message(ClientMessage::Ping {
            timestamp: system_time_millis(),
        })
        .await
    }

    /// Opt-in to raw debug telemetry from the authoritative server.
    pub async fn set_debug_telemetry_enabled(&mut self, enabled: bool) -> Result<(), ClientError> {
        self.send_message(ClientMessage::SetDebugTelemetry { enabled })
            .await
    }

    /// Focus debug summaries on a specific entity without widening gameplay
    /// visibility.
    pub async fn set_debug_focus_entity(
        &mut self,
        entity_id: Option<u64>,
    ) -> Result<(), ClientError> {
        self.send_message(ClientMessage::SetDebugFocus { entity_id })
            .await
    }

    /// Disconnect from the server
    pub async fn disconnect(&mut self, reason: Option<&str>) -> Result<(), ClientError> {
        if let Ok(mut runtime) = self.runtime_state.lock() {
            runtime.closed = true;
            runtime.closed_explicitly = true;
            runtime.last_error = reason.map(str::to_string);
        }
        let msg = ClientMessage::Disconnect {
            reason: reason.map(|s| s.to_string()),
        };

        self.send_message(msg).await?;

        if let Some(conn) = self.connection.take() {
            conn.close(
                quinn::VarInt::from_u32(0),
                reason.unwrap_or("Disconnect").as_bytes(),
            );
        }
        if let Some(reader_task) = self.reader_task.take() {
            reader_task.abort();
        }
        self.endpoint.close(
            quinn::VarInt::from_u32(0),
            reason.unwrap_or("Disconnect").as_bytes(),
        );
        self.recovery_state.clear();
        self.last_debug_telemetry_json = None;
        self.last_debug_document = None;
        self.pending_debug_documents.clear();
        self.clear_presentation_state();
        self.reconnect_attempts = 0;
        self.next_reconnect_at = None;

        info!("Disconnected from server");
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

    /// Most recent measured round-trip time, if any.
    pub fn latency_ms(&self) -> Option<u64> {
        self.last_ping_rtt_ms
    }

    /// Smoothed RTT jitter estimate in milliseconds, if any.
    pub fn jitter_ms(&self) -> Option<f32> {
        self.ping_jitter_ms
    }

    /// Latest fatal transport error observed from the reader/runtime state.
    pub fn last_connection_error(&self) -> Option<String> {
        self.runtime_state
            .lock()
            .ok()
            .and_then(|runtime| runtime.last_error.clone())
    }

    /// Returns true when the native transport needs a reconnect attempt.
    pub fn reconnect_needed(&self) -> bool {
        let Ok(runtime) = self.runtime_state.lock() else {
            return false;
        };

        !runtime.closed_explicitly
            && (runtime.closed
                || ((self.client_id.is_some() || self.connection.is_some())
                    && self.heartbeat_timed_out_at(std::time::Instant::now())))
    }

    /// Attempt to recover the native transport using the retained reconnect token.
    pub async fn recover_connection(&mut self) -> Result<bool, ClientError> {
        if !self.reconnect_needed() {
            return Ok(false);
        }

        let now = std::time::Instant::now();
        if let Some(next_attempt_at) = self.next_reconnect_at {
            if now < next_attempt_at {
                return Ok(false);
            }
        }

        if let Some(reader_task) = self.reader_task.take() {
            reader_task.abort();
        }
        if let Some(conn) = self.connection.take() {
            conn.close(quinn::VarInt::from_u32(2), b"reconnect");
        }
        self.client_id = None;

        let addr = format!("{}:{}", self.config.server_addr, self.config.server_port)
            .parse()
            .map_err(|e| ClientError::Config(format!("Invalid address: {e}")))?;

        let connecting = match self.endpoint.connect(addr, "localhost") {
            Ok(connecting) => connecting,
            Err(err) => {
                self.schedule_reconnect_backoff(now, err.to_string());
                return Err(ClientError::Connection(err.to_string()));
            }
        };

        let connection = match connecting.await {
            Ok(connection) => connection,
            Err(err) => {
                self.schedule_reconnect_backoff(now, err.to_string());
                return Err(ClientError::Connection(err.to_string()));
            }
        };

        self.connection = Some(connection);
        if let Err(err) = self.send_connect().await {
            self.connection = None;
            self.schedule_reconnect_backoff(now, err.to_string());
            return Err(err);
        }

        self.start_reader_loop();
        self.reconnect_attempts = 0;
        self.next_reconnect_at = None;
        if let Ok(mut runtime) = self.runtime_state.lock() {
            runtime.closed = false;
            runtime.closed_explicitly = false;
            runtime.last_error = None;
        }
        Ok(true)
    }

    /// Get the client ID
    pub fn client_id(&self) -> Option<ClientId> {
        self.client_id
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.connection.is_some() && self.client_id.is_some()
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

    fn reconnect_delay_ms(&self) -> u64 {
        (250u64.saturating_mul(2u64.pow(self.reconnect_attempts.min(6)))).min(10_000)
    }

    fn schedule_reconnect_backoff(
        &mut self,
        now: std::time::Instant,
        reason: impl Into<String>,
    ) {
        self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
        self.next_reconnect_at = Some(
            now + std::time::Duration::from_millis(self.reconnect_delay_ms()),
        );
        if let Ok(mut runtime) = self.runtime_state.lock() {
            runtime.closed = true;
            runtime.closed_explicitly = false;
            runtime.last_error = Some(reason.into());
        }
    }

    fn record_pong_latency(&mut self, now_ms: u64, client_ts: u64) {
        let Some(rtt_ms) = now_ms.checked_sub(client_ts) else {
            return;
        };
        let jitter = self
            .last_ping_rtt_ms
            .map(|last_rtt_ms| last_rtt_ms.abs_diff(rtt_ms) as f32);
        self.last_ping_rtt_ms = Some(rtt_ms);
        self.ping_jitter_ms = Some(match (self.ping_jitter_ms, jitter) {
            (Some(previous), Some(delta)) => (previous * 0.75) + (delta * 0.25),
            (_, Some(delta)) => delta,
            (Some(previous), None) => previous,
            (None, None) => 0.0,
        });
    }

    fn heartbeat_timed_out_at(&self, now: std::time::Instant) -> bool {
        now.saturating_duration_since(self.last_server_activity_at)
            > std::time::Duration::from_millis(self.config.heartbeat_timeout_ms)
    }

    fn enforce_heartbeat_timeout_at(&mut self, now: std::time::Instant) {
        if (self.client_id.is_none() && self.connection.is_none())
            || !self.heartbeat_timed_out_at(now)
        {
            return;
        }

        warn!(
            "Disconnecting native client after {}ms without server traffic",
            self.config.heartbeat_timeout_ms
        );

        if let Some(reader_task) = self.reader_task.take() {
            reader_task.abort();
        }
        if let Some(conn) = self.connection.take() {
            conn.close(quinn::VarInt::from_u32(1), b"heartbeat timeout");
        }
        self.client_id = None;
        self.recovery_state.clear();
        self.schedule_reconnect_backoff(now, "heartbeat timeout");
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

        let message = ClientMessage::RequestFullSnapshot {
            last_known_tick,
            last_known_digest,
        };

        let payload = match message.encode() {
            Ok(payload) => payload,
            Err(err) => {
                warn!("Failed to encode full snapshot recovery request: {}", err);
                return;
            }
        };

        let Some(connection) = self.connection.as_ref().cloned() else {
            warn!("Failed to request full snapshot recovery: not connected");
            return;
        };

        self.recovery_state
            .record_request(current_server_tick, last_known_digest);
        tokio::spawn(async move {
            let mut send = match connection.open_uni().await {
                Ok(send) => send,
                Err(err) => {
                    warn!("Failed to open recovery request stream: {}", err);
                    return;
                }
            };

            if let Err(err) = send.write_all(&payload).await {
                warn!("Failed to write recovery request payload: {}", err);
                return;
            }

            if let Err(err) = send.finish() {
                warn!("Failed to finish recovery request stream: {}", err);
            }
        });
    }

    #[cfg(test)]
    fn request_full_snapshot_without_connection_is_safe(&mut self) {
        self.request_full_snapshot(self.last_server_tick);
        if self.connection.is_none() {
            self.recovery_state.clear();
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

// ============================================================
// TLS VERIFICATION STUB
// ============================================================

/// Stub certificate verifier that accepts any certificate
/// (appropriate for self-signed dev certs)
#[derive(Debug)]
struct NoServerVerification;

impl rustls::client::danger::ServerCertVerifier for NoServerVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

fn warn_insecure_tls_mode() {
    error!(
        "POD_INSECURE_SKIP_TLS_VERIFY is enabled. TLS certificate verification is disabled and this mode is unsafe for production."
    );
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

fn system_time_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::{TickTelemetryFrame, VersionedTickTelemetry};

    #[test]
    fn test_client_creation() {
        let config = ProtoClientConfig {
            server_addr: "localhost".to_string(),
            server_port: 5000,
            player_name: "TestPlayer".to_string(),
            timeout_ms: 5000,
            heartbeat_timeout_ms: 6500,
            max_pending_actions: 32,
        };

        // Note: actual connection would require a running server
        // This just tests configuration
        assert_eq!(config.player_name, "TestPlayer");
        assert_eq!(config.server_addr, "localhost");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_catch_up_diagnostics_uses_replay_history() {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let authoritative = WorldSnapshot {
            tick: 20,
            entities: vec![crate::snapshot::EntitySnapshot {
                id: 9,
                position: glam::Vec2::new(10.0, 0.0),
                velocity: glam::Vec2::ZERO,
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: Some(120.0),
                label: Some("player".into()),
                metadata: crate::snapshot::EntityMetadataSnapshot::default(),
            }],
            population: pod_core::WorldPopulationState {
                tick: 20,
                ..Default::default()
            },
        };
        let predicted = WorldSnapshot {
            tick: 21,
            entities: vec![crate::snapshot::EntitySnapshot {
                id: 9,
                position: glam::Vec2::new(12.0, 0.0),
                velocity: glam::Vec2::new(120.0, 0.0),
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: Some(120.0),
                label: Some("player".into()),
                metadata: crate::snapshot::EntityMetadataSnapshot::default(),
            }],
            population: pod_core::WorldPopulationState {
                tick: 21,
                ..Default::default()
            },
        };
        let mut render_buffer = SnapshotInterpolationBuffer::default();
        render_buffer.push(authoritative.clone());
        let mut render_clock = RenderClock::default();
        render_clock.advance(20, 1.0 / 60.0);

        let client = NativeClient {
            config: ProtoClientConfig {
                server_addr: "localhost".into(),
                server_port: 5000,
                player_name: "Tester".into(),
                timeout_ms: 1000,
                heartbeat_timeout_ms: 6500,
                max_pending_actions: 32,
            },
            client_id: None,
            connection: None,
            endpoint,
            runtime_state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
            authoritative_snapshot: Some(authoritative),
            local_snapshot: Some(predicted),
            controlled_entity: Some(9),
            pending_actions: Vec::new(),
            prediction_history: vec![PredictedActionBatch {
                tick: 21,
                actions: vec![Action::Move {
                    direction: glam::Vec2::X,
                }],
            }],
            last_server_tick: 20,
            last_acknowledged_action_tick: None,
            last_event_tick: 20,
            reconnect_token: None,
            last_reconciliation: None,
            last_debug_telemetry_json: None,
            last_debug_document: None,
            pending_debug_documents: Vec::new(),
            recovery_state: RecoveryRequestState::default(),
            render_buffer,
            render_clock,
            last_server_activity_at: std::time::Instant::now(),
            last_ping_rtt_ms: None,
            ping_jitter_ms: None,
            reconnect_attempts: 0,
            next_reconnect_at: None,
        };

        let diagnostics = client.catch_up_diagnostics();
        assert_eq!(diagnostics.authoritative_tick, Some(20));
        assert_eq!(diagnostics.predicted_tick, Some(21));
        assert_eq!(diagnostics.pending_action_batches, 1);
        assert!(diagnostics.controlled_entity_drift.is_some());
        assert_eq!(client.rewind_authoritative_snapshot(18).unwrap().tick, 20);
        assert_eq!(
            client.rollback_preview(Some(20)).unwrap().replayed_batches,
            1
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_resumed_welcome_replays_unacknowledged_predictions() {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let authoritative = WorldSnapshot {
            tick: 7,
            entities: vec![crate::snapshot::EntitySnapshot {
                id: 9,
                position: glam::Vec2::ZERO,
                velocity: glam::Vec2::ZERO,
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: Some(120.0),
                label: Some("player".into()),
                metadata: crate::snapshot::EntityMetadataSnapshot::default(),
            }],
            population: pod_core::WorldPopulationState {
                tick: 7,
                ..Default::default()
            },
        };
        let reconnect_token = ReconnectToken::new();
        let mut client = NativeClient {
            config: ProtoClientConfig {
                server_addr: "localhost".into(),
                server_port: 5000,
                player_name: "Tester".into(),
                timeout_ms: 1000,
                heartbeat_timeout_ms: 6500,
                max_pending_actions: 32,
            },
            client_id: None,
            connection: None,
            endpoint,
            runtime_state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
            authoritative_snapshot: None,
            local_snapshot: None,
            controlled_entity: None,
            pending_actions: Vec::new(),
            prediction_history: vec![PredictedActionBatch {
                tick: 7,
                actions: vec![Action::Move {
                    direction: glam::Vec2::X,
                }],
            }],
            last_server_tick: 0,
            last_acknowledged_action_tick: Some(5),
            last_event_tick: 0,
            reconnect_token: Some(reconnect_token),
            last_reconciliation: None,
            last_debug_telemetry_json: None,
            last_debug_document: None,
            pending_debug_documents: Vec::new(),
            recovery_state: RecoveryRequestState::default(),
            render_buffer: SnapshotInterpolationBuffer::default(),
            render_clock: RenderClock::default(),
            last_server_activity_at: std::time::Instant::now(),
            last_ping_rtt_ms: None,
            ping_jitter_ms: None,
            reconnect_attempts: 0,
            next_reconnect_at: None,
        };

        assert!(client.apply_server_message(&ServerMessage::Welcome {
            client_id: ClientId::new(),
            reconnect_token: ReconnectToken::new(),
            tick: 7,
            controlled_entity: Some(9),
            acknowledged_action_tick: Some(6),
            authoritative_digest: authoritative.digest(),
            snapshot: authoritative.clone(),
        }));

        assert_eq!(client.prediction_history.len(), 1);
        assert_eq!(client.last_acknowledged_action_tick, Some(6));
        assert_eq!(
            client
                .last_reconciliation
                .as_ref()
                .expect("reconciliation recorded")
                .pending_action_batches,
            1
        );
        assert_eq!(
            client
                .last_reconciliation
                .as_ref()
                .expect("reconciliation recorded")
                .acknowledged_action_tick,
            Some(6)
        );
        assert!(
            client
                .local_snapshot
                .as_ref()
                .expect("predicted snapshot rebuilt")
                .entities[0]
                .position
                .x
                > authoritative.entities[0].position.x
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_request_full_snapshot_without_connection_does_not_arm_recovery_flag() {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let mut client = NativeClient {
            config: ProtoClientConfig {
                server_addr: "localhost".into(),
                server_port: 5000,
                player_name: "Tester".into(),
                timeout_ms: 1000,
                heartbeat_timeout_ms: 6500,
                max_pending_actions: 32,
            },
            client_id: None,
            connection: None,
            endpoint,
            runtime_state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
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
            last_server_activity_at: std::time::Instant::now(),
            last_ping_rtt_ms: None,
            ping_jitter_ms: None,
            reconnect_attempts: 0,
            next_reconnect_at: None,
        };

        client.request_full_snapshot_without_connection_is_safe();
        assert_eq!(client.recovery_state, RecoveryRequestState::default());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_tick_telemetry_updates_debug_cache() {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let mut client = NativeClient {
            config: ProtoClientConfig {
                server_addr: "localhost".into(),
                server_port: 5000,
                player_name: "Tester".into(),
                timeout_ms: 1000,
                heartbeat_timeout_ms: 6500,
                max_pending_actions: 32,
            },
            client_id: None,
            connection: None,
            endpoint,
            runtime_state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
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
            last_server_activity_at: std::time::Instant::now(),
            last_ping_rtt_ms: None,
            ping_jitter_ms: None,
            reconnect_attempts: 0,
            next_reconnect_at: None,
        };

        let message = ServerMessage::TickTelemetry {
            frame_json: VersionedTickTelemetry::new(TickTelemetryFrame::empty(4))
                .to_toon_document(),
        };
        assert!(client.apply_server_message(&message));
        assert!(client
            .last_debug_telemetry_document()
            .expect("debug telemetry stored")
            .contains("versioned_tick_telemetry"));
        assert_eq!(client.drain_debug_documents().len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_debug_document_updates_generic_cache() {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let mut client = NativeClient {
            config: ProtoClientConfig {
                server_addr: "localhost".into(),
                server_port: 5000,
                player_name: "Tester".into(),
                timeout_ms: 1000,
                heartbeat_timeout_ms: 6500,
                max_pending_actions: 32,
            },
            client_id: None,
            connection: None,
            endpoint,
            runtime_state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
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
            last_server_activity_at: std::time::Instant::now(),
            last_ping_rtt_ms: None,
            ping_jitter_ms: None,
            reconnect_attempts: 0,
            next_reconnect_at: None,
        };

        let message = ServerMessage::DebugDocument {
            document: pod_core::AgentToolCallEvent::new(
                44,
                pod_core::AgentToolCallTrace::success(4, "llm.complete", "qwen", 18, 128, 64),
            )
            .to_toon_document(),
        };
        assert!(client.apply_server_message(&message));
        assert!(client
            .last_debug_document()
            .expect("latest debug document stored")
            .contains("agent_tool_call_event"));
        assert!(client.last_debug_telemetry_document().is_none());
        assert_eq!(client.drain_debug_documents().len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_pong_updates_latency_and_jitter() {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let mut client = NativeClient {
            config: ProtoClientConfig {
                server_addr: "localhost".into(),
                server_port: 5000,
                player_name: "Tester".into(),
                timeout_ms: 1000,
                heartbeat_timeout_ms: 6500,
                max_pending_actions: 32,
            },
            client_id: None,
            connection: None,
            endpoint,
            runtime_state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
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
            last_server_activity_at: std::time::Instant::now(),
            last_ping_rtt_ms: None,
            ping_jitter_ms: None,
            reconnect_attempts: 0,
            next_reconnect_at: None,
        };

        client.record_pong_latency(1_100, 1_000);
        client.record_pong_latency(1_240, 1_100);

        assert_eq!(client.latency_ms(), Some(140));
        assert_eq!(client.jitter_ms(), Some(10.0));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_queue_action_respects_pending_limit() {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let mut client = NativeClient {
            config: ProtoClientConfig {
                server_addr: "localhost".into(),
                server_port: 5000,
                player_name: "Tester".into(),
                timeout_ms: 1000,
                heartbeat_timeout_ms: 6500,
                max_pending_actions: 1,
            },
            client_id: None,
            connection: None,
            endpoint,
            runtime_state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
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
            last_server_activity_at: std::time::Instant::now(),
            last_ping_rtt_ms: None,
            ping_jitter_ms: None,
            reconnect_attempts: 0,
            next_reconnect_at: None,
        };

        client.queue_action(Action::Idle);
        client.queue_action(Action::Stop);

        assert_eq!(client.pending_actions.len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_send_actions_rejects_when_prediction_backlog_is_saturated() {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let mut client = NativeClient {
            config: ProtoClientConfig {
                server_addr: "localhost".into(),
                server_port: 5000,
                player_name: "Tester".into(),
                timeout_ms: 1000,
                heartbeat_timeout_ms: 6500,
                max_pending_actions: 1,
            },
            client_id: Some(ClientId::new()),
            connection: None,
            endpoint,
            runtime_state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
            authoritative_snapshot: None,
            local_snapshot: None,
            controlled_entity: None,
            pending_actions: vec![Action::Idle],
            prediction_history: vec![PredictedActionBatch {
                tick: 4,
                actions: vec![Action::Stop],
            }],
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
            last_server_activity_at: std::time::Instant::now(),
            last_ping_rtt_ms: None,
            ping_jitter_ms: None,
            reconnect_attempts: 0,
            next_reconnect_at: None,
        };

        let error = client
            .send_actions(5)
            .await
            .expect_err("backlog should reject");
        assert!(matches!(error, ClientError::Rejected(_)));
        assert_eq!(client.pending_actions.len(), 1);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_enforce_heartbeat_timeout_clears_stale_session() {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let now = std::time::Instant::now();
        let mut client = NativeClient {
            config: ProtoClientConfig {
                server_addr: "localhost".into(),
                server_port: 5000,
                player_name: "Tester".into(),
                timeout_ms: 1000,
                heartbeat_timeout_ms: 10,
                max_pending_actions: 32,
            },
            client_id: Some(ClientId::new()),
            connection: None,
            endpoint,
            runtime_state: Arc::new(Mutex::new(NativeRuntimeState::default())),
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
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
            last_server_activity_at: now,
            last_ping_rtt_ms: None,
            ping_jitter_ms: None,
            reconnect_attempts: 0,
            next_reconnect_at: None,
        };

        assert!(client.heartbeat_timed_out_at(now + std::time::Duration::from_millis(11)));
        client.enforce_heartbeat_timeout_at(now + std::time::Duration::from_millis(11));
        assert!(client.client_id().is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_reconnect_needed_tracks_runtime_closed_state() {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let runtime_state = Arc::new(Mutex::new(NativeRuntimeState::default()));
        let client = NativeClient {
            config: ProtoClientConfig {
                server_addr: "localhost".into(),
                server_port: 5000,
                player_name: "Tester".into(),
                timeout_ms: 1000,
                heartbeat_timeout_ms: 6500,
                max_pending_actions: 32,
            },
            client_id: Some(ClientId::new()),
            connection: None,
            endpoint,
            runtime_state: runtime_state.clone(),
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
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
            last_server_activity_at: std::time::Instant::now(),
            last_ping_rtt_ms: None,
            ping_jitter_ms: None,
            reconnect_attempts: 0,
            next_reconnect_at: None,
        };

        assert!(!client.reconnect_needed());
        runtime_state.lock().unwrap().closed = true;
        assert!(client.reconnect_needed());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_recover_connection_respects_backoff_window() {
        let endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap()).unwrap();
        let (tx, rx) = mpsc::channel(1);
        let runtime_state = Arc::new(Mutex::new(NativeRuntimeState {
            closed: true,
            last_error: Some("connection lost".into()),
            closed_explicitly: false,
        }));
        let now = std::time::Instant::now();
        let mut client = NativeClient {
            config: ProtoClientConfig {
                server_addr: "localhost".into(),
                server_port: 5000,
                player_name: "Tester".into(),
                timeout_ms: 1000,
                heartbeat_timeout_ms: 6500,
                max_pending_actions: 32,
            },
            client_id: None,
            connection: None,
            endpoint,
            runtime_state,
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
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
            last_server_activity_at: now,
            last_ping_rtt_ms: None,
            ping_jitter_ms: None,
            reconnect_attempts: 2,
            next_reconnect_at: Some(now + std::time::Duration::from_secs(1)),
        };

        assert!(!client.recover_connection().await.expect("backoff should defer"));
    }
}
