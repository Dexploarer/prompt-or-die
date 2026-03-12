//! Authoritative game server — QUIC-based, server-owned world state.
//!
//! This module only compiles on non-WASM platforms (desktop/cloud).

#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use log::{debug, error, info, warn};
use quinn::Endpoint;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, RwLock};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use pod_core::{
    summarize_focused_entity_debug, Action, AgentTickRollup, AgentToolCallEvent,
    ClientTransportSummary, GameEvent, IdleAgent, ShardTransportSummary, TelemetryArchive,
    TelemetryConfig, TickTelemetryFrame, VersionedTickTelemetry, World,
};

use crate::protocol::{
    ClientId, ClientMessage, ReconnectToken, ServerConfig as ProtoServerConfig, ServerMessage,
};
use crate::snapshot::{SnapshotInterest, StateDelta, WorldSnapshot};

// ============================================================
// CLIENT SESSION STATE
// ============================================================

/// Tracks a connected client and their session
struct ClientSession {
    player_name: Option<String>,
    agent_id: Option<pod_core::AgentId>,
    pending_actions: Vec<(u64, Action)>, // (tick, action)
    reconnect_token: ReconnectToken,
    last_action_tick: Option<u64>,
    last_processed_action_tick: Option<u64>,
    debug_telemetry_enabled: bool,
    debug_focus_entity: Option<u64>,
    last_sent_snapshot: Option<WorldSnapshot>,
    transport: ClientTransportCounters,
}

impl ClientSession {
    #[allow(dead_code)]
    fn new(player_name: String) -> Self {
        Self {
            player_name: Some(player_name),
            agent_id: None,
            pending_actions: Vec::new(),
            reconnect_token: ReconnectToken::new(),
            last_action_tick: None,
            last_processed_action_tick: None,
            debug_telemetry_enabled: false,
            debug_focus_entity: None,
            last_sent_snapshot: None,
            transport: ClientTransportCounters::default(),
        }
    }
}

#[derive(Debug, Clone, Default)]
struct ClientTransportCounters {
    last_seen_tick: u64,
    last_sent_tick: Option<u64>,
    session_resumes: u64,
    recovery_snapshots_sent: u64,
    recovery_delivery_failures: u64,
    recovery_snapshot_bytes_sent: u64,
    inbound_messages: u64,
    outbound_messages: u64,
    inbound_bytes: u64,
    outbound_bytes: u64,
    action_batches_received: u64,
    full_snapshots_sent: u64,
    full_snapshot_bytes: u64,
    max_full_snapshot_bytes: u64,
    full_snapshot_requests: u64,
    ping_requests: u64,
    state_deltas_sent: u64,
    delta_messages_sent: u64,
    delta_bytes_sent: u64,
    max_delta_bytes: u64,
    delta_entities_updated: u64,
    delta_entities_destroyed: u64,
    event_batches_sent: u64,
    debug_documents_sent: u64,
    rejected_messages_sent: u64,
    peak_pending_action_queue_depth: usize,
    queue_pressure_events: u64,
}

#[derive(Clone)]
struct ClientBroadcastTarget {
    client_id: ClientId,
    agent_id: Option<pod_core::AgentId>,
    acknowledged_action_tick: Option<u64>,
    debug_telemetry_enabled: bool,
    debug_focus_entity: Option<u64>,
    last_sent_snapshot: Option<WorldSnapshot>,
}

#[derive(Debug, Clone)]
enum PendingDebugDocument {
    TickTelemetry(VersionedTickTelemetry),
    AgentToolCallEvent(AgentToolCallEvent),
    AgentTickRollup(AgentTickRollup),
    ShardTransportSummary(ShardTransportSummary),
}

impl PendingDebugDocument {
    fn to_toon_document(&self) -> String {
        match self {
            Self::TickTelemetry(document) => document.to_toon_document(),
            Self::AgentToolCallEvent(document) => document.to_toon_document(),
            Self::AgentTickRollup(document) => document.to_toon_document(),
            Self::ShardTransportSummary(document) => document.to_toon_document(),
        }
    }
}

#[derive(Debug)]
enum InboundPacket {
    Message {
        client_id: ClientId,
        encoded_len: usize,
        message: ClientMessage,
    },
    Disconnected {
        client_id: ClientId,
        reason: String,
    },
}

const ACTION_QUEUE_MAX_DEPTH: usize = 256;
const ACTION_WINDOW_BACKWARD_TICKS: u64 = 2;
const ACTION_WINDOW_FORWARD_TICKS: u64 = 2;
const TRANSPORT_SUMMARY_INTERVAL_TICKS: u64 = 60;

// ============================================================
// GAME SERVER
// ============================================================

/// Authoritative game server
pub struct GameServer {
    /// Server configuration
    config: ProtoServerConfig,
    /// Authoritative world state
    world: World,
    /// Connected clients
    clients: Arc<RwLock<HashMap<ClientId, ClientSession>>>,
    /// Message channels per client (for sending updates)
    client_tx: Arc<RwLock<HashMap<ClientId, mpsc::Sender<ServerMessage>>>>,
    /// Inbound client messages aggregated from connection tasks.
    inbound_tx: mpsc::Sender<InboundPacket>,
    inbound_rx: mpsc::Receiver<InboundPacket>,
    /// QUIC endpoint
    endpoint: Option<Endpoint>,
    /// Optional WebSocket fallback listener for browser clients.
    websocket_listener: Option<TcpListener>,
    /// Current tick counter
    tick: u64,
    /// Last tick when snapshot was sent
    last_snapshot_tick: u64,
    /// Last full snapshot for delta computation.
    last_snapshot: Option<WorldSnapshot>,
    /// Latest authoritative telemetry payload for debug/editor clients.
    last_tick_telemetry_json: Option<String>,
    /// Game events emitted during the current authoritative tick.
    pending_events: Vec<GameEvent>,
    /// Recent authoritative telemetry retained for live debug rollups.
    debug_archive: TelemetryArchive,
    /// TOON debug documents collected during the current authoritative tick.
    pending_debug_documents: Vec<PendingDebugDocument>,
    /// Last tick when server transport stats were logged.
    last_transport_log_tick: u64,
    /// Total number of clients disconnected due to inactivity timeout.
    timed_out_clients: u64,
    /// Total number of queue-pressure incidents observed.
    queue_pressure_events: u64,
    /// Total number of session resumes completed through reconnect tokens.
    resumed_sessions: u64,
    /// Total number of recovery full snapshots successfully delivered.
    recovery_snapshots_sent: u64,
    /// Total number of recovery snapshot delivery failures.
    recovery_delivery_failures: u64,
}

impl GameServer {
    /// Create a new game server
    pub fn new(config: ProtoServerConfig, world: World) -> Self {
        let (inbound_tx, inbound_rx) = mpsc::channel(1024);
        Self {
            config,
            world,
            clients: Arc::new(RwLock::new(HashMap::new())),
            client_tx: Arc::new(RwLock::new(HashMap::new())),
            inbound_tx,
            inbound_rx,
            endpoint: None,
            websocket_listener: None,
            tick: 0,
            last_snapshot_tick: 0,
            last_snapshot: None,
            last_tick_telemetry_json: None,
            pending_events: Vec::new(),
            debug_archive: TelemetryArchive::with_capacity(
                TelemetryConfig::default().core_archive_ticks,
            ),
            pending_debug_documents: Vec::new(),
            last_transport_log_tick: 0,
            timed_out_clients: 0,
            queue_pressure_events: 0,
            resumed_sessions: 0,
            recovery_snapshots_sent: 0,
            recovery_delivery_failures: 0,
        }
    }

    /// Initialize the server (bind to network)
    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let addr: SocketAddr =
            format!("{}:{}", self.config.bind_addr, self.config.bind_port).parse()?;

        // Generate self-signed certificate for QUIC (rcgen 0.13+ API)
        let cert_key = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])?;

        let key_der = cert_key.key_pair.serialize_der();
        let cert_der = cert_key.cert.der().to_vec();

        let key = rustls::pki_types::PrivateKeyDer::Pkcs8(
            rustls::pki_types::PrivatePkcs8KeyDer::from(key_der),
        );

        let cert = rustls::pki_types::CertificateDer::from(cert_der);

        // Build rustls ServerConfig first (rustls 0.23 API)
        let rustls_server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)?;

        // Create quinn ServerConfig from rustls config (quinn 0.11 + rustls 0.23)
        let quic_server_config =
            quinn::crypto::rustls::QuicServerConfig::try_from(rustls_server_config)?;
        let server_config = quinn::ServerConfig::with_crypto(Arc::new(quic_server_config));

        // Create endpoint and bind to address
        let endpoint = Endpoint::server(server_config, addr)?;

        self.endpoint = Some(endpoint);

        if self.config.enable_websocket {
            let websocket_addr: SocketAddr =
                format!("{}:{}", self.config.bind_addr, self.config.websocket_port).parse()?;
            let listener = TcpListener::bind(websocket_addr).await?;
            self.websocket_listener = Some(listener);
            info!(
                "WebSocket fallback initialized on {}:{}",
                self.config.bind_addr, self.config.websocket_port
            );
        }

        info!(
            "GameServer initialized on {}:{}",
            self.config.bind_addr, self.config.bind_port
        );

        Ok(())
    }

    /// Main server loop — step the world and handle client connections
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let tick_duration = std::time::Duration::from_secs_f32(1.0 / self.config.tick_rate as f32);

        loop {
            let frame_start = std::time::Instant::now();

            // Step the world
            self.step_tick().await?;

            // Handle client connections
            self.handle_connections().await?;

            // Disconnect clients that stopped sending any heartbeat/activity.
            self.prune_stale_clients().await;

            // Send updates to clients
            self.broadcast_updates().await?;

            if (self.tick - self.last_transport_log_tick) >= TRANSPORT_SUMMARY_INTERVAL_TICKS {
                let transport = self.transport_summary().await;
                info!(
                    "[NET] tick={} clients={} resumes={} recoveries={}/fail={} in={} msgs/{} B out={} msgs/{} B snaps={}/{}B/{}Bmax deltas={}/{}B/{}Bmax churn=+{}/-{} queue={}/{} pressure={} events={} batches={} full_resync={} debug_docs={} timeouts={}",
                    transport.latest_tick,
                    transport.client_count,
                    transport.resumed_sessions,
                    transport.recovery_snapshots_sent,
                    transport.recovery_delivery_failures,
                    transport.total_inbound_messages,
                    transport.total_inbound_bytes,
                    transport.total_outbound_messages,
                    transport.total_outbound_bytes,
                    transport.full_snapshots_sent,
                    transport.total_full_snapshot_bytes,
                    transport.max_full_snapshot_bytes,
                    transport.delta_messages_sent,
                    transport.total_delta_bytes,
                    transport.max_delta_bytes,
                    transport.total_delta_entities_updated,
                    transport.total_delta_entities_destroyed,
                    transport.total_pending_action_queue_depth,
                    transport.peak_pending_action_queue_depth,
                    transport.queue_pressure_client_count,
                    transport.queue_pressure_events,
                    transport.action_batches_received,
                    transport.full_snapshot_requests,
                    transport.debug_documents_sent,
                    transport.timed_out_clients,
                );
                self.last_transport_log_tick = self.tick;
            }

            // Sleep to maintain tick rate
            let elapsed = frame_start.elapsed();
            if elapsed < tick_duration {
                tokio::time::sleep(tick_duration - elapsed).await;
            } else {
                warn!(
                    "Tick {} exceeded target duration ({:?})",
                    self.tick, tick_duration
                );
            }

            self.tick += 1;
        }
    }

    /// Process one game tick
    async fn step_tick(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        {
            let mut clients = self.clients.write().await;
            for session in clients.values_mut() {
                let Some(agent_id) = session.agent_id else {
                    session.pending_actions.clear();
                    continue;
                };

                let mut last_processed_action_tick = None;
                for (action_tick, action) in session.pending_actions.drain(..) {
                    let min_tick = self.tick.saturating_sub(ACTION_WINDOW_BACKWARD_TICKS);
                    let max_tick = self.tick + ACTION_WINDOW_FORWARD_TICKS;
                    if action_tick < min_tick || action_tick > max_tick {
                        continue;
                    }
                    self.world.submit_external_action(agent_id, action);
                    last_processed_action_tick = Some(
                        last_processed_action_tick
                            .map(|current: u64| current.max(action_tick))
                            .unwrap_or(action_tick),
                    );
                }

                if last_processed_action_tick.is_some() {
                    session.last_processed_action_tick = last_processed_action_tick;
                }
            }
        }

        // Step the world
        let tick_result = self.world.step();
        self.debug_archive
            .record_tick(tick_result.telemetry.clone());
        self.pending_debug_documents.clear();
        self.pending_events = tick_result.events.clone();

        let telemetry_document = VersionedTickTelemetry::new(tick_result.telemetry.clone());
        self.last_tick_telemetry_json = Some(telemetry_document.to_toon_document());
        self.pending_debug_documents
            .push(PendingDebugDocument::TickTelemetry(telemetry_document));

        for agent in &tick_result.telemetry.agents {
            let Some(entity_id) = agent.entity_id else {
                continue;
            };
            for trace in &agent.tool_calls {
                self.pending_debug_documents
                    .push(PendingDebugDocument::AgentToolCallEvent(
                        AgentToolCallEvent::new(entity_id.0, trace.clone()),
                    ));
            }
        }

        if (tick_result.tick + 1).is_multiple_of(60) {
            for rollup in self.debug_rollups_for_tick(tick_result.tick) {
                self.pending_debug_documents
                    .push(PendingDebugDocument::AgentTickRollup(rollup));
            }
        }

        debug!(
            "Tick {}: {} entities, {} agents",
            self.tick,
            self.world.entity_count(),
            self.world.agent_count()
        );

        Ok(())
    }

    fn queue_pressure_warn_depth(&self) -> usize {
        self.config
            .queue_pressure_warn_depth
            .clamp(1, ACTION_QUEUE_MAX_DEPTH)
    }

    fn is_queue_pressure_depth(&self, depth: usize) -> bool {
        depth >= self.queue_pressure_warn_depth()
    }

    async fn prune_stale_clients(&mut self) {
        let timeout_ticks = self.config.client_inactivity_timeout_ticks;
        if timeout_ticks == 0 {
            return;
        }

        let stale_clients = {
            let clients = self.clients.read().await;
            clients
                .iter()
                .filter_map(|(client_id, session)| {
                    let ticks_since_last_seen = self.tick.saturating_sub(session.transport.last_seen_tick);
                    (ticks_since_last_seen > timeout_ticks).then_some((*client_id, ticks_since_last_seen))
                })
                .collect::<Vec<_>>()
        };

        for (client_id, ticks_since_last_seen) in stale_clients {
            self.timed_out_clients = self.timed_out_clients.saturating_add(1);
            warn!(
                "Client {} timed out after {} ticks without heartbeat/activity",
                client_id.0, ticks_since_last_seen
            );
            self.disconnect_client(
                client_id,
                &format!(
                    "heartbeat timeout after {ticks_since_last_seen} ticks without inbound activity"
                ),
            )
            .await;
        }
    }

    async fn register_pending_client(
        &mut self,
        client_id: ClientId,
    ) -> mpsc::Receiver<ServerMessage> {
        let (outbound_tx, outbound_rx) = mpsc::channel::<ServerMessage>(256);
        self.client_tx.write().await.insert(client_id, outbound_tx);
        let mut session = ClientSession::new("pending".into());
        session.transport.last_seen_tick = self.tick;
        self.clients
            .write()
            .await
            .insert(client_id, session);
        outbound_rx
    }

    /// Accept incoming client connections
    async fn handle_connections(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            let websocket_accepted = {
                let Some(listener) = self.websocket_listener.as_ref() else {
                    break;
                };
                match tokio::time::timeout(Duration::from_millis(1), listener.accept()).await {
                    Ok(Ok(accepted)) => accepted,
                    Ok(Err(err)) => {
                        warn!("Failed to accept websocket client: {err}");
                        break;
                    }
                    Err(_) => break,
                }
            };

            let (stream, _remote_addr) = websocket_accepted;

            let websocket = match accept_async(stream).await {
                Ok(websocket) => websocket,
                Err(err) => {
                    warn!("Failed to complete websocket handshake: {err}");
                    continue;
                }
            };

            let client_id = ClientId::new();
            info!("Accepted new websocket client connection: {}", client_id.0);

            let mut outbound_rx = self.register_pending_client(client_id).await;
            let inbound_tx = self.inbound_tx.clone();
            let (mut ws_write, mut ws_read) = websocket.split();

            tokio::spawn(async move {
                while let Some(message) = ws_read.next().await {
                    match message {
                        Ok(Message::Text(text)) => {
                            let message = match ClientMessage::decode_json(text.as_ref()) {
                                Ok(message) => message,
                                Err(err) => {
                                    warn!(
                                        "WebSocket client {} sent invalid JSON message: {}",
                                        client_id.0, err
                                    );
                                    continue;
                                }
                            };

                            if inbound_tx
                                .send(InboundPacket::Message {
                                    client_id,
                                    encoded_len: text.len(),
                                    message,
                                })
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Ok(Message::Binary(bytes)) => {
                            let text = match String::from_utf8(bytes.to_vec()) {
                                Ok(text) => text,
                                Err(err) => {
                                    warn!(
                                        "WebSocket client {} sent non-UTF8 binary payload: {}",
                                        client_id.0, err
                                    );
                                    continue;
                                }
                            };
                            let message = match ClientMessage::decode_json(&text) {
                                Ok(message) => message,
                                Err(err) => {
                                    warn!(
                                        "WebSocket client {} sent invalid binary JSON message: {}",
                                        client_id.0, err
                                    );
                                    continue;
                                }
                            };

                            if inbound_tx
                                .send(InboundPacket::Message {
                                    client_id,
                                    encoded_len: text.len(),
                                    message,
                                })
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                        Ok(Message::Close(frame)) => {
                            let reason = frame
                                .map(|frame| frame.reason.to_string())
                                .unwrap_or_else(|| "websocket closed".to_string());
                            let _ = inbound_tx
                                .send(InboundPacket::Disconnected { client_id, reason })
                                .await;
                            break;
                        }
                        Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
                        Ok(Message::Frame(_)) => {}
                        Err(err) => {
                            let _ = inbound_tx
                                .send(InboundPacket::Disconnected {
                                    client_id,
                                    reason: format!("websocket receive failed: {err}"),
                                })
                                .await;
                            break;
                        }
                    }
                }
            });

            tokio::spawn(async move {
                while let Some(message) = outbound_rx.recv().await {
                    let payload = match message.encode_json() {
                        Ok(payload) => payload,
                        Err(err) => {
                            error!("Failed to encode websocket outbound message: {}", err);
                            continue;
                        }
                    };

                    if let Err(err) = ws_write.send(Message::Text(payload)).await {
                        error!("Failed to send websocket outbound message: {}", err);
                        break;
                    }
                }

                let _ = ws_write.close().await;
            });
        }

        if self.endpoint.is_some() {
            loop {
                let incoming = {
                    let endpoint = self.endpoint.as_ref().expect("checked above");
                    match tokio::time::timeout(Duration::from_millis(1), endpoint.accept()).await {
                        Ok(Some(incoming)) => incoming,
                        Ok(None) => break,
                        Err(_) => break,
                    }
                };

                let connection = match incoming.await {
                    Ok(conn) => conn,
                    Err(err) => {
                        warn!("Failed to accept client connection: {err}");
                        continue;
                    }
                };

                let client_id = ClientId::new();
                info!("Accepted new client connection: {}", client_id.0);

                let mut outbound_rx = self.register_pending_client(client_id).await;

                let read_conn = connection.clone();
                let write_conn = connection.clone();
                let inbound_tx = self.inbound_tx.clone();

                tokio::spawn(async move {
                    loop {
                        let mut recv = match read_conn.accept_uni().await {
                            Ok(recv) => recv,
                            Err(err) => {
                                let _ = inbound_tx
                                    .send(InboundPacket::Disconnected {
                                        client_id,
                                        reason: format!("receive failed: {err}"),
                                    })
                                    .await;
                                break;
                            }
                        };

                        let bytes = match recv.read_to_end(64 * 1024).await {
                            Ok(bytes) => bytes,
                            Err(err) => {
                                warn!("Client {} stream read failed: {}", client_id.0, err);
                                continue;
                            }
                        };

                        let message = match ClientMessage::decode(&bytes) {
                            Ok(message) => message,
                            Err(err) => {
                                warn!("Client {} sent invalid message: {}", client_id.0, err);
                                continue;
                            }
                        };

                        if inbound_tx
                            .send(InboundPacket::Message {
                                client_id,
                                encoded_len: bytes.len(),
                                message,
                            })
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                });

                tokio::spawn(async move {
                    while let Some(message) = outbound_rx.recv().await {
                        let payload = match message.encode() {
                            Ok(payload) => payload,
                            Err(err) => {
                                error!("Failed to encode outbound message: {}", err);
                                continue;
                            }
                        };

                        let mut send = match write_conn.open_uni().await {
                            Ok(send) => send,
                            Err(err) => {
                                error!("Failed to open outbound stream: {}", err);
                                break;
                            }
                        };

                        if let Err(err) = send.write_all(&payload).await {
                            error!("Failed to write outbound payload: {}", err);
                            break;
                        }
                        if let Err(err) = send.finish() {
                            error!("Failed to finish outbound stream: {}", err);
                            break;
                        }
                    }
                });
            }
        }

        while let Ok(packet) = self.inbound_rx.try_recv() {
            match packet {
                InboundPacket::Message {
                    client_id,
                    encoded_len,
                    message,
                } => {
                    {
                        let mut clients = self.clients.write().await;
                        if let Some(session) = clients.get_mut(&client_id) {
                            session.transport.inbound_messages =
                                session.transport.inbound_messages.saturating_add(1);
                            session.transport.inbound_bytes = session
                                .transport
                                .inbound_bytes
                                .saturating_add(encoded_len as u64);
                            session.transport.last_seen_tick = self.tick;
                        }
                    }
                    match message {
                    ClientMessage::Connect {
                        player_name,
                        reconnect_token,
                    } => {
                        self.attach_remote_agent(client_id, player_name, reconnect_token)
                            .await?;
                    }
                    ClientMessage::ActionBatch { tick, actions } => {
                        let mut overflow = false;
                        let mut unregistered = false;
                        let mut stale_tick = false;
                        let mut out_of_window = false;
                        let mut queue_pressure = false;
                        let mut queue_depth = 0usize;
                        let min_tick = self.tick.saturating_sub(ACTION_WINDOW_BACKWARD_TICKS);
                        let max_tick = self.tick + ACTION_WINDOW_FORWARD_TICKS;
                        let queue_pressure_warn_depth = self.queue_pressure_warn_depth();
                        {
                            let mut clients = self.clients.write().await;
                            if let Some(session) = clients.get_mut(&client_id) {
                                session.transport.action_batches_received = session
                                    .transport
                                    .action_batches_received
                                    .saturating_add(1);
                                if session.agent_id.is_none() {
                                    unregistered = true;
                                }

                                if !unregistered {
                                    if tick < min_tick || tick > max_tick {
                                        out_of_window = true;
                                    } else if session
                                        .last_action_tick
                                        .map(|last| tick < last)
                                        .unwrap_or(false)
                                    {
                                        stale_tick = true;
                    } else {
                        let available = ACTION_QUEUE_MAX_DEPTH
                            .saturating_sub(session.pending_actions.len());
                        if actions.len() > available {
                            overflow = true;
                        }
                        for action in actions.into_iter().take(available) {
                            session.pending_actions.push((tick, action));
                        }
                        queue_depth = session.pending_actions.len();
                        session.transport.peak_pending_action_queue_depth = session
                            .transport
                            .peak_pending_action_queue_depth
                            .max(queue_depth);
                        queue_pressure = queue_depth >= queue_pressure_warn_depth;
                        session.last_action_tick = Some(tick);
                    }
                }
            }
        }
        if queue_pressure || overflow {
            self.queue_pressure_events =
                self.queue_pressure_events.saturating_add(1);
            if let Some(session) = self.clients.write().await.get_mut(&client_id) {
                session.transport.queue_pressure_events =
                    session.transport.queue_pressure_events.saturating_add(1);
            }
            warn!(
                "Client {} pending action queue reached pressure depth {} (threshold {})",
                client_id.0,
                queue_depth,
                queue_pressure_warn_depth
                            );
                        }
                        if unregistered {
                            self.send_to_client(
                                client_id,
                                ServerMessage::Rejected {
                                    reason: "client must send Connect before action batches".into(),
                                },
                            )
                            .await;
                            continue;
                        }
                        if out_of_window {
                            self.send_to_client(
                                    client_id,
                                    ServerMessage::Rejected {
                                        reason: format!(
                                            "action batch tick out of window: tick={tick} accepted=[{min_tick}..={max_tick}]"
                                        ),
                                    },
                                )
                                .await;
                            continue;
                        }
                        if stale_tick {
                            self.send_to_client(
                                    client_id,
                                    ServerMessage::Rejected {
                                        reason: format!(
                                            "stale action batch tick={tick}; requires non-decreasing submission order"
                                        ),
                                    },
                                )
                                .await;
                            continue;
                        }
                        if overflow {
                            self.send_to_client(
                                client_id,
                                ServerMessage::Rejected {
                                    reason: format!(
                                        "action queue full (max depth={ACTION_QUEUE_MAX_DEPTH})"
                                    ),
                                },
                            )
                            .await;
                        }
                    }
                    ClientMessage::RequestFullSnapshot {
                        last_known_tick,
                        last_known_digest,
                    } => {
                        {
                            let mut clients = self.clients.write().await;
                            if let Some(session) = clients.get_mut(&client_id) {
                                session.transport.full_snapshot_requests = session
                                    .transport
                                    .full_snapshot_requests
                                    .saturating_add(1);
                            }
                        }
                        debug!(
                            "Client {} requested full snapshot recovery (tick={:?}, digest={:?})",
                            client_id.0, last_known_tick, last_known_digest
                        );
                        self.send_full_snapshot_to_client(client_id).await;
                    }
                    ClientMessage::SetDebugTelemetry { enabled } => {
                        let mut clients = self.clients.write().await;
                        if let Some(session) = clients.get_mut(&client_id) {
                            session.debug_telemetry_enabled = enabled;
                        }
                    }
                    ClientMessage::SetDebugFocus { entity_id } => {
                        let mut clients = self.clients.write().await;
                        if let Some(session) = clients.get_mut(&client_id) {
                            session.debug_focus_entity = entity_id;
                        }
                    }
                    ClientMessage::Ping { timestamp } => {
                        {
                            let mut clients = self.clients.write().await;
                            if let Some(session) = clients.get_mut(&client_id) {
                                session.transport.ping_requests =
                                    session.transport.ping_requests.saturating_add(1);
                            }
                        }
                        self.send_to_client(
                            client_id,
                            ServerMessage::Pong {
                                client_ts: timestamp,
                                server_ts: self.tick,
                            },
                        )
                        .await;
                    }
                    ClientMessage::Disconnect { reason } => {
                        self.disconnect_client(
                            client_id,
                            reason.as_deref().unwrap_or("client requested disconnect"),
                        )
                        .await;
                    }
                }
                }
                InboundPacket::Disconnected { client_id, reason } => {
                    self.disconnect_client(client_id, &reason).await;
                }
            }
        }

        Ok(())
    }

    /// Broadcast world updates to all connected clients
    async fn broadcast_updates(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let should_snapshot =
            (self.tick - self.last_snapshot_tick) >= self.config.snapshot_interval;
        let authoritative_snapshot = WorldSnapshot::capture(&self.world);
        let has_authoritative_baseline = self.last_snapshot.is_some();
        let has_event_update = !self.pending_events.is_empty();
        let client_targets = {
            let clients = self.clients.read().await;
            clients
                .iter()
                .map(|(client_id, session)| ClientBroadcastTarget {
                    client_id: *client_id,
                    agent_id: session.agent_id,
                    acknowledged_action_tick: session.last_processed_action_tick,
                    debug_telemetry_enabled: session.debug_telemetry_enabled,
                    debug_focus_entity: session.debug_focus_entity,
                    last_sent_snapshot: session.last_sent_snapshot.clone(),
                })
                .collect::<Vec<_>>()
        };
        let has_debug_subscribers = client_targets
            .iter()
            .any(|target| target.debug_telemetry_enabled);

        if !has_authoritative_baseline
            && client_targets.is_empty()
            && !has_debug_subscribers
            && !has_event_update
        {
            self.pending_events.clear();
            self.pending_debug_documents.clear();
            return Ok(());
        }

        let mut updated_client_snapshots = Vec::new();
        let had_missing_client_baseline = client_targets
            .iter()
            .any(|target| target.last_sent_snapshot.is_none());
        let mut any_state_update = !has_authoritative_baseline;

        if has_debug_subscribers && self.tick.is_multiple_of(TRANSPORT_SUMMARY_INTERVAL_TICKS) {
            self.pending_debug_documents
                .push(PendingDebugDocument::ShardTransportSummary(
                    self.transport_summary().await,
                ));
        }

        for target in &client_targets {
            let interest =
                self.snapshot_interest_for_agent(&authoritative_snapshot, target.agent_id);
            let filtered_snapshot = authoritative_snapshot.for_interest(&interest);
            let interested_entities = filtered_snapshot
                .entities
                .iter()
                .map(|entity| entity.id)
                .collect::<std::collections::HashSet<_>>();
            let authoritative_digest = filtered_snapshot.digest();
            let is_full_snapshot = should_snapshot || target.last_sent_snapshot.is_none();
            let delta = if is_full_snapshot {
                StateDelta {
                    tick: self.tick,
                    updated: filtered_snapshot.entities.clone(),
                    destroyed: vec![],
                    population: filtered_snapshot.population.clone(),
                }
            } else {
                StateDelta::diff(
                    target
                        .last_sent_snapshot
                        .as_ref()
                        .expect("client baseline checked above"),
                    &filtered_snapshot,
                )
            };

            let population_changed = target
                .last_sent_snapshot
                .as_ref()
                .map(|snapshot| snapshot.population != filtered_snapshot.population)
                .unwrap_or(true);
            let has_state_update =
                is_full_snapshot || delta.change_count() > 0 || population_changed;

            if has_state_update {
                any_state_update = true;
                let sent = self
                    .send_to_client(
                        target.client_id,
                        ServerMessage::StateDelta {
                            tick: self.tick,
                            acknowledged_action_tick: target.acknowledged_action_tick,
                            authoritative_digest,
                            is_full_snapshot,
                            delta,
                        },
                    )
                    .await;
                if sent {
                    updated_client_snapshots.push((target.client_id, filtered_snapshot));
                }
            }

            if has_event_update {
                let events = self.filter_events_for_interest(&self.pending_events, &interest);
                if !events.is_empty() {
                    let _ = self
                        .send_to_client(
                            target.client_id,
                            ServerMessage::EventBatch {
                                tick: self.tick,
                                events,
                            },
                        )
                        .await;
                }
            }

            if target.debug_telemetry_enabled {
                let mut debug_documents = self.debug_documents_for_interest(
                    &self.pending_debug_documents,
                    &interest,
                    &interested_entities,
                );
                if let Some(focus_entity) = target
                    .debug_focus_entity
                    .filter(|entity_id| {
                        interest.is_unbounded() || interested_entities.contains(entity_id)
                    })
                {
                    if let Some(summary) = summarize_focused_entity_debug(
                        "direct-connect",
                        &self.debug_archive,
                        focus_entity,
                    ) {
                        debug_documents.push(summary.to_toon_document());
                    }
                }
                for document in debug_documents {
                    let sent = self
                        .send_to_client(target.client_id, ServerMessage::DebugDocument { document })
                        .await;
                    if !sent {
                        break;
                    }
                }
            }
        }

        if !updated_client_snapshots.is_empty() {
            let mut clients = self.clients.write().await;
            for (client_id, snapshot) in updated_client_snapshots {
                if let Some(session) = clients.get_mut(&client_id) {
                    session.last_sent_snapshot = Some(snapshot);
                }
            }
        }

        if should_snapshot || had_missing_client_baseline {
            self.last_snapshot_tick = self.tick;
        }
        if any_state_update {
            self.last_snapshot = Some(authoritative_snapshot);
        }
        self.pending_events.clear();
        self.pending_debug_documents.clear();

        Ok(())
    }

    /// Get current tick number
    pub fn current_tick(&self) -> u64 {
        self.tick
    }

    /// Get connected client count
    pub async fn client_count(&self) -> usize {
        self.clients.read().await.len()
    }

    /// Kick a client from the server
    pub async fn disconnect_client(&mut self, client_id: ClientId, reason: &str) {
        if let Some(session) = self.clients.write().await.remove(&client_id) {
            if let Some(agent_id) = session.agent_id {
                if let Some(index) = self
                    .world
                    .agents
                    .iter()
                    .position(|slot| slot.agent.id() == agent_id)
                {
                    self.world.remove_agent(index);
                }
            }
        }
        self.client_tx.write().await.remove(&client_id);
        info!("Client {} disconnected: {}", client_id.0, reason);
    }

    async fn send_to_client(&self, client_id: ClientId, message: ServerMessage) -> bool {
        enum OutboundKind {
            FullSnapshot { recovery: bool },
            Delta {
                updated_entities: usize,
                destroyed_entities: usize,
            },
            EventBatch,
            DebugDocument,
            Rejected,
            Other,
        }

        let outbound_kind = match &message {
            ServerMessage::Welcome { .. } => OutboundKind::FullSnapshot { recovery: false },
            ServerMessage::StateDelta {
                is_full_snapshot,
                delta: _,
                ..
            } => {
                if *is_full_snapshot {
                    OutboundKind::FullSnapshot { recovery: false }
                } else {
                    let ServerMessage::StateDelta { delta, .. } = &message else {
                        unreachable!("matched state delta above");
                    };
                    OutboundKind::Delta {
                        updated_entities: delta.updated.len(),
                        destroyed_entities: delta.destroyed.len(),
                    }
                }
            }
            ServerMessage::EventBatch { .. } => OutboundKind::EventBatch,
            ServerMessage::TickTelemetry { .. } | ServerMessage::DebugDocument { .. } => {
                OutboundKind::DebugDocument
            }
            ServerMessage::Rejected { .. } => OutboundKind::Rejected,
            ServerMessage::Pong { .. } => OutboundKind::Other,
        };
        let encoded_size = message.encode().map(|payload| payload.len()).unwrap_or_default() as u64;

        if let Some(tx) = self.client_tx.read().await.get(&client_id) {
            if let Err(err) = tx.send(message).await {
                warn!("Failed to send message to {}: {}", client_id.0, err);
                return false;
            }
        }
        if let Some(session) = self.clients.write().await.get_mut(&client_id) {
            session.transport.outbound_messages =
                session.transport.outbound_messages.saturating_add(1);
            session.transport.outbound_bytes = session
                .transport
                .outbound_bytes
                .saturating_add(encoded_size);
            session.transport.last_sent_tick = Some(self.tick);
            match outbound_kind {
                OutboundKind::FullSnapshot { recovery } => {
                    session.transport.full_snapshots_sent = session
                        .transport
                        .full_snapshots_sent
                        .saturating_add(1);
                    session.transport.full_snapshot_bytes = session
                        .transport
                        .full_snapshot_bytes
                        .saturating_add(encoded_size);
                    session.transport.max_full_snapshot_bytes = session
                        .transport
                        .max_full_snapshot_bytes
                        .max(encoded_size);
                    session.transport.state_deltas_sent =
                        session.transport.state_deltas_sent.saturating_add(1);
                    if recovery {
                        session.transport.recovery_snapshot_bytes_sent = session
                            .transport
                            .recovery_snapshot_bytes_sent
                            .saturating_add(encoded_size);
                    }
                }
                OutboundKind::Delta {
                    updated_entities,
                    destroyed_entities,
                } => {
                    session.transport.state_deltas_sent = session
                        .transport
                        .state_deltas_sent
                        .saturating_add(1);
                    session.transport.delta_messages_sent = session
                        .transport
                        .delta_messages_sent
                        .saturating_add(1);
                    session.transport.delta_bytes_sent = session
                        .transport
                        .delta_bytes_sent
                        .saturating_add(encoded_size);
                    session.transport.max_delta_bytes = session
                        .transport
                        .max_delta_bytes
                        .max(encoded_size);
                    session.transport.delta_entities_updated = session
                        .transport
                        .delta_entities_updated
                        .saturating_add(updated_entities as u64);
                    session.transport.delta_entities_destroyed = session
                        .transport
                        .delta_entities_destroyed
                        .saturating_add(destroyed_entities as u64);
                }
                OutboundKind::EventBatch => {
                    session.transport.event_batches_sent =
                        session.transport.event_batches_sent.saturating_add(1);
                }
                OutboundKind::DebugDocument => {
                    session.transport.debug_documents_sent =
                        session.transport.debug_documents_sent.saturating_add(1);
                }
                OutboundKind::Rejected => {
                    session.transport.rejected_messages_sent =
                        session.transport.rejected_messages_sent.saturating_add(1);
                }
                OutboundKind::Other => {}
            }
        }
        true
    }

    pub async fn transport_summary(&self) -> ShardTransportSummary {
        let clients = self.clients.read().await;
        let mut client_summaries = clients
            .iter()
            .map(|(client_id, session)| ClientTransportSummary {
                client_id: client_id.0.to_string(),
                player_name: session.player_name.clone(),
                controlled_entity: session
                    .agent_id
                    .and_then(|agent_id| self.controlled_entity_for_agent(agent_id)),
                session_resumes: session.transport.session_resumes,
                recovery_snapshots_sent: session.transport.recovery_snapshots_sent,
                recovery_delivery_failures: session.transport.recovery_delivery_failures,
                last_seen_tick: session.transport.last_seen_tick,
                ticks_since_last_seen: self.tick.saturating_sub(session.transport.last_seen_tick),
                last_sent_tick: session.transport.last_sent_tick,
                pending_action_queue_depth: session.pending_actions.len(),
                peak_pending_action_queue_depth: session
                    .transport
                    .peak_pending_action_queue_depth
                    .max(session.pending_actions.len()),
                queue_pressure: self.is_queue_pressure_depth(session.pending_actions.len()),
                queue_pressure_events: session.transport.queue_pressure_events,
                inbound_messages: session.transport.inbound_messages,
                outbound_messages: session.transport.outbound_messages,
                inbound_bytes: session.transport.inbound_bytes,
                outbound_bytes: session.transport.outbound_bytes,
                action_batches_received: session.transport.action_batches_received,
                full_snapshots_sent: session.transport.full_snapshots_sent,
                full_snapshot_bytes: session.transport.full_snapshot_bytes,
                max_full_snapshot_bytes: session.transport.max_full_snapshot_bytes,
                recovery_snapshot_bytes_sent: session.transport.recovery_snapshot_bytes_sent,
                full_snapshot_requests: session.transport.full_snapshot_requests,
                ping_requests: session.transport.ping_requests,
                state_deltas_sent: session.transport.state_deltas_sent,
                delta_messages_sent: session.transport.delta_messages_sent,
                delta_bytes_sent: session.transport.delta_bytes_sent,
                max_delta_bytes: session.transport.max_delta_bytes,
                delta_entities_updated: session.transport.delta_entities_updated,
                delta_entities_destroyed: session.transport.delta_entities_destroyed,
                event_batches_sent: session.transport.event_batches_sent,
                debug_documents_sent: session.transport.debug_documents_sent,
                rejected_messages_sent: session.transport.rejected_messages_sent,
                debug_telemetry_enabled: session.debug_telemetry_enabled,
            })
            .collect::<Vec<_>>();
        client_summaries.sort_by(|left, right| left.client_id.cmp(&right.client_id));

        ShardTransportSummary {
            shard_id: "direct-connect".to_string(),
            latest_tick: self.tick,
            client_count: client_summaries.len(),
            resumed_sessions: self.resumed_sessions,
            recovery_snapshots_sent: self.recovery_snapshots_sent,
            recovery_delivery_failures: self.recovery_delivery_failures,
            client_inactivity_timeout_ticks: self.config.client_inactivity_timeout_ticks,
            queue_pressure_warn_depth: self.queue_pressure_warn_depth(),
            total_pending_action_queue_depth: client_summaries
                .iter()
                .map(|client| client.pending_action_queue_depth)
                .sum(),
            peak_pending_action_queue_depth: client_summaries
                .iter()
                .map(|client| client.peak_pending_action_queue_depth)
                .max()
                .unwrap_or_default(),
            queue_pressure_client_count: client_summaries
                .iter()
                .filter(|client| client.queue_pressure)
                .count(),
            total_inbound_messages: client_summaries
                .iter()
                .map(|client| client.inbound_messages)
                .sum(),
            total_outbound_messages: client_summaries
                .iter()
                .map(|client| client.outbound_messages)
                .sum(),
            total_inbound_bytes: client_summaries.iter().map(|client| client.inbound_bytes).sum(),
            total_outbound_bytes: client_summaries
                .iter()
                .map(|client| client.outbound_bytes)
                .sum(),
            action_batches_received: client_summaries
                .iter()
                .map(|client| client.action_batches_received)
                .sum(),
            full_snapshots_sent: client_summaries
                .iter()
                .map(|client| client.full_snapshots_sent)
                .sum(),
            total_full_snapshot_bytes: client_summaries
                .iter()
                .map(|client| client.full_snapshot_bytes)
                .sum(),
            max_full_snapshot_bytes: client_summaries
                .iter()
                .map(|client| client.max_full_snapshot_bytes)
                .max()
                .unwrap_or_default(),
            total_recovery_snapshot_bytes: client_summaries
                .iter()
                .map(|client| client.recovery_snapshot_bytes_sent)
                .sum(),
            full_snapshot_requests: client_summaries
                .iter()
                .map(|client| client.full_snapshot_requests)
                .sum(),
            ping_requests: client_summaries.iter().map(|client| client.ping_requests).sum(),
            state_deltas_sent: client_summaries
                .iter()
                .map(|client| client.state_deltas_sent)
                .sum(),
            delta_messages_sent: client_summaries
                .iter()
                .map(|client| client.delta_messages_sent)
                .sum(),
            total_delta_bytes: client_summaries
                .iter()
                .map(|client| client.delta_bytes_sent)
                .sum(),
            max_delta_bytes: client_summaries
                .iter()
                .map(|client| client.max_delta_bytes)
                .max()
                .unwrap_or_default(),
            total_delta_entities_updated: client_summaries
                .iter()
                .map(|client| client.delta_entities_updated)
                .sum(),
            total_delta_entities_destroyed: client_summaries
                .iter()
                .map(|client| client.delta_entities_destroyed)
                .sum(),
            event_batches_sent: client_summaries
                .iter()
                .map(|client| client.event_batches_sent)
                .sum(),
            debug_documents_sent: client_summaries
                .iter()
                .map(|client| client.debug_documents_sent)
                .sum(),
            rejected_messages_sent: client_summaries
                .iter()
                .map(|client| client.rejected_messages_sent)
                .sum(),
            timed_out_clients: self.timed_out_clients,
            queue_pressure_events: self.queue_pressure_events,
            clients: client_summaries,
        }
    }

    fn build_full_snapshot_message(
        &self,
        snapshot: WorldSnapshot,
        acknowledged_action_tick: Option<u64>,
    ) -> ServerMessage {
        let authoritative_digest = snapshot.digest();
        ServerMessage::StateDelta {
            tick: self.tick,
            acknowledged_action_tick,
            authoritative_digest,
            is_full_snapshot: true,
            delta: StateDelta {
                tick: self.tick,
                updated: snapshot.entities,
                destroyed: vec![],
                population: snapshot.population,
            },
        }
    }

    fn controlled_entity_for_agent(&self, agent_id: pod_core::AgentId) -> Option<u64> {
        self.world
            .agents
            .iter()
            .find(|slot| slot.agent.id() == agent_id)
            .and_then(|slot| slot.entity_id)
            .map(|entity| entity.id() as u64)
    }

    fn snapshot_interest_for_controlled_entity(
        &self,
        authoritative_snapshot: &WorldSnapshot,
        controlled_entity: Option<u64>,
    ) -> SnapshotInterest {
        let Some(controlled_entity) = controlled_entity else {
            return SnapshotInterest::default();
        };
        let Some(entity) = authoritative_snapshot
            .entities
            .iter()
            .find(|entity| entity.id == controlled_entity)
        else {
            return SnapshotInterest::default();
        };

        let mut chunk_keys = Vec::new();
        if let Some(chunk_key) = entity.metadata.chunk_key.as_deref() {
            chunk_keys.push(chunk_key.to_string());
            if let Some(chunk) = self
                .world
                .streaming
                .chunks
                .iter()
                .find(|chunk| chunk.chunk_key == chunk_key)
            {
                chunk_keys.extend(chunk.neighbor_chunk_keys.iter().cloned());
            }
        }
        if chunk_keys.is_empty() {
            if let Some(region_id) = entity.metadata.region_id.as_deref() {
                if let Some(region) = self
                    .world
                    .streaming
                    .regions
                    .iter()
                    .find(|region| region.region_id == region_id)
                {
                    chunk_keys.extend(region.chunk_keys.iter().cloned());
                }
            }
        }

        SnapshotInterest::new(
            Some(controlled_entity),
            Some(entity.position),
            Some(self.world.streaming.chunk_size.max(0.001) * 1.75),
            chunk_keys,
        )
    }

    fn snapshot_interest_for_agent(
        &self,
        authoritative_snapshot: &WorldSnapshot,
        agent_id: Option<pod_core::AgentId>,
    ) -> SnapshotInterest {
        let controlled_entity =
            agent_id.and_then(|agent_id| self.controlled_entity_for_agent(agent_id));
        self.snapshot_interest_for_controlled_entity(authoritative_snapshot, controlled_entity)
    }

    fn filter_events_for_interest(
        &self,
        events: &[GameEvent],
        interest: &SnapshotInterest,
    ) -> Vec<GameEvent> {
        let chunk_keys = interest
            .chunk_keys
            .iter()
            .map(String::as_str)
            .collect::<std::collections::HashSet<_>>();
        let center = interest.center;
        let radius = interest.radius.unwrap_or_default().max(0.0);
        let unbounded = interest.controlled_entity.is_none()
            && interest.center.is_none()
            && interest.radius.is_none()
            && interest.chunk_keys.is_empty();

        if unbounded {
            return events.to_vec();
        }

        events
            .iter()
            .filter(|event| {
                if center
                    .map(|center| center.distance(event.origin) <= radius)
                    .unwrap_or(false)
                {
                    return true;
                }

                let event_chunk_key = self
                    .world
                    .resolve_streaming_metadata(event.origin)
                    .chunk_key;
                chunk_keys.contains(event_chunk_key.as_str())
            })
            .cloned()
            .collect()
    }

    fn filter_tick_telemetry_for_entities(
        &self,
        telemetry: &VersionedTickTelemetry,
        interested_entities: &std::collections::HashSet<u64>,
    ) -> Option<VersionedTickTelemetry> {
        let agents = telemetry
            .payload
            .agents
            .iter()
            .filter(|agent| {
                agent
                    .entity_id
                    .map(|entity_id| interested_entities.contains(&entity_id.0))
                    .unwrap_or(false)
            })
            .cloned()
            .collect::<Vec<_>>();

        if agents.is_empty() {
            return None;
        }

        Some(VersionedTickTelemetry::new(TickTelemetryFrame {
            tick: telemetry.payload.tick,
            agents,
        }))
    }

    fn debug_documents_for_interest(
        &self,
        documents: &[PendingDebugDocument],
        interest: &SnapshotInterest,
        interested_entities: &std::collections::HashSet<u64>,
    ) -> Vec<String> {
        if interest.is_unbounded() {
            return documents
                .iter()
                .map(PendingDebugDocument::to_toon_document)
                .collect();
        }

        documents
            .iter()
            .filter_map(|document| match document {
                PendingDebugDocument::TickTelemetry(telemetry) => self
                    .filter_tick_telemetry_for_entities(telemetry, interested_entities)
                    .map(|filtered| filtered.to_toon_document()),
                PendingDebugDocument::AgentToolCallEvent(event) => interested_entities
                    .contains(&event.agent_entity_id)
                    .then(|| event.to_toon_document()),
                PendingDebugDocument::AgentTickRollup(rollup) => interested_entities
                    .contains(&rollup.agent_entity_id)
                    .then(|| rollup.to_toon_document()),
                PendingDebugDocument::ShardTransportSummary(summary) => {
                    Some(summary.to_toon_document())
                }
            })
            .collect()
    }

    fn snapshot_for_client(
        &self,
        authoritative_snapshot: &WorldSnapshot,
        agent_id: Option<pod_core::AgentId>,
    ) -> WorldSnapshot {
        let interest = self.snapshot_interest_for_agent(authoritative_snapshot, agent_id);
        authoritative_snapshot.for_interest(&interest)
    }

    async fn send_full_snapshot_to_client(&mut self, client_id: ClientId) {
        let (acknowledged_action_tick, agent_id) = self
            .clients
            .read()
            .await
            .get(&client_id)
            .map(|session| (session.last_processed_action_tick, session.agent_id))
            .unwrap_or((None, None));
        let authoritative_snapshot = WorldSnapshot::capture(&self.world);
        let snapshot = self.snapshot_for_client(&authoritative_snapshot, agent_id);
        let message = self.build_full_snapshot_message(snapshot.clone(), acknowledged_action_tick);
        let encoded_size = message.encode().map(|payload| payload.len()).unwrap_or_default() as u64;
        let sent = self
            .send_to_client(client_id, message)
            .await;
        if sent {
            self.recovery_snapshots_sent = self.recovery_snapshots_sent.saturating_add(1);
            if let Some(session) = self.clients.write().await.get_mut(&client_id) {
                session.last_sent_snapshot = Some(snapshot);
                session.transport.recovery_snapshots_sent = session
                    .transport
                    .recovery_snapshots_sent
                    .saturating_add(1);
                session.transport.recovery_snapshot_bytes_sent = session
                    .transport
                    .recovery_snapshot_bytes_sent
                    .saturating_add(encoded_size);
            }
        } else {
            self.recovery_delivery_failures = self.recovery_delivery_failures.saturating_add(1);
            if let Some(session) = self.clients.write().await.get_mut(&client_id) {
                session.transport.recovery_delivery_failures = session
                    .transport
                    .recovery_delivery_failures
                    .saturating_add(1);
            }
        }
    }

    async fn attach_remote_agent(
        &mut self,
        client_id: ClientId,
        player_name: String,
        reconnect_token: Option<ReconnectToken>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(token) = reconnect_token {
            let previous_client_id = {
                let mut clients = self.clients.write().await;

                let previous_client_id = clients.iter().find_map(|(id, session)| {
                    if *id != client_id && session.reconnect_token == token {
                        Some(*id)
                    } else {
                        None
                    }
                });

                let previous_session = previous_client_id.and_then(|id| clients.remove(&id));

                if let Some(session) = clients.get_mut(&client_id) {
                    session.reconnect_token = token;
                    if let Some(prev) = previous_session {
                        session.player_name = prev.player_name;
                        session.agent_id = prev.agent_id;
                        session.last_action_tick = prev.last_action_tick;
                        session.last_processed_action_tick = prev.last_processed_action_tick;
                        session.debug_telemetry_enabled = prev.debug_telemetry_enabled;
                        session.debug_focus_entity = prev.debug_focus_entity;
                        session.pending_actions.clear();
                        session.last_sent_snapshot = None;
                        session.transport = prev.transport;
                        session.transport.session_resumes =
                            session.transport.session_resumes.saturating_add(1);
                    } else if session.player_name.is_none() {
                        session.player_name = Some(player_name.clone());
                    }
                }

                previous_client_id
            };

            if let Some(previous_client_id) = previous_client_id {
                self.client_tx.write().await.remove(&previous_client_id);
                self.resumed_sessions = self.resumed_sessions.saturating_add(1);
                info!(
                    "Client {} resumed prior session {}",
                    client_id.0, previous_client_id.0
                );
            }
        }

        let reconnect_token = {
            let clients = self.clients.read().await;
            let session = clients.get(&client_id).ok_or_else(|| {
                Box::new(ServerError::ClientError(format!(
                    "missing session for client {}",
                    client_id.0
                ))) as Box<dyn std::error::Error>
            })?;
            session.reconnect_token
        };

        let already_attached = self
            .clients
            .read()
            .await
            .get(&client_id)
            .and_then(|session| session.agent_id)
            .is_some();

        if already_attached {
            let controlled_entity = self
                .clients
                .read()
                .await
                .get(&client_id)
                .and_then(|session| session.agent_id)
                .and_then(|agent_id| self.controlled_entity_for_agent(agent_id));
            let acknowledged_action_tick = self
                .clients
                .read()
                .await
                .get(&client_id)
                .and_then(|session| session.last_processed_action_tick);
            let authoritative_snapshot = WorldSnapshot::capture(&self.world);
            let snapshot = self.snapshot_for_client(
                &authoritative_snapshot,
                self.clients
                    .read()
                    .await
                    .get(&client_id)
                    .and_then(|session| session.agent_id),
            );
            let authoritative_digest = snapshot.digest();
            let sent = self
                .send_to_client(
                    client_id,
                    ServerMessage::Welcome {
                        client_id,
                        reconnect_token,
                        tick: self.tick,
                        controlled_entity,
                        acknowledged_action_tick,
                        authoritative_digest,
                        snapshot: snapshot.clone(),
                    },
                )
                .await;
            if sent {
                if let Some(session) = self.clients.write().await.get_mut(&client_id) {
                    if session.debug_focus_entity.is_none() {
                        session.debug_focus_entity = controlled_entity;
                    }
                    session.last_sent_snapshot = Some(snapshot);
                }
            }
            return Ok(());
        }

        let (slot_index, entity) = self.world.add_agent(Box::new(IdleAgent::new()));
        if let Ok(mut label) = self.world.ecs.get::<&mut pod_core::Label>(entity) {
            label.name = player_name.clone();
        }
        let agent_id = self.world.agents[slot_index].agent.id();

        {
            let mut clients = self.clients.write().await;
            if let Some(session) = clients.get_mut(&client_id) {
                session.player_name = Some(player_name);
                session.agent_id = Some(agent_id);
                session.last_action_tick = None;
                session.debug_focus_entity = Some(entity.id() as u64);
            }
        }

        let authoritative_snapshot = WorldSnapshot::capture(&self.world);
        let snapshot = self.snapshot_for_client(&authoritative_snapshot, Some(agent_id));
        let authoritative_digest = snapshot.digest();
        let sent = self
            .send_to_client(
                client_id,
                ServerMessage::Welcome {
                    client_id,
                    reconnect_token,
                    tick: self.tick,
                    controlled_entity: Some(entity.id() as u64),
                    acknowledged_action_tick: None,
                    authoritative_digest,
                    snapshot: snapshot.clone(),
                },
            )
            .await;
        if sent {
            if let Some(session) = self.clients.write().await.get_mut(&client_id) {
                session.last_sent_snapshot = Some(snapshot);
            }
        }

        Ok(())
    }

    fn debug_rollups_for_tick(&self, tick_end: u64) -> Vec<AgentTickRollup> {
        let tick_start = tick_end.saturating_sub(59);
        let mut frames_by_agent = HashMap::<u64, Vec<pod_core::AgentTelemetryFrame>>::new();

        for frame in self
            .debug_archive
            .frames()
            .iter()
            .filter(|frame| frame.tick >= tick_start && frame.tick <= tick_end)
        {
            for agent in &frame.agents {
                let Some(entity_id) = agent.entity_id else {
                    continue;
                };
                frames_by_agent
                    .entry(entity_id.0)
                    .or_default()
                    .push(agent.clone());
            }
        }

        let mut entity_ids: Vec<u64> = frames_by_agent.keys().copied().collect();
        entity_ids.sort_unstable();

        entity_ids
            .into_iter()
            .filter_map(|entity_id| {
                frames_by_agent
                    .get(&entity_id)
                    .and_then(|frames| AgentTickRollup::from_agent_frames(entity_id, frames))
            })
            .collect()
    }
}

// ============================================================
// SERVER ERROR TYPES
// ============================================================

#[derive(Debug)]
pub enum ServerError {
    NetworkError(String),
    ConfigError(String),
    ClientError(String),
}

impl std::fmt::Display for ServerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServerError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            ServerError::ConfigError(msg) => write!(f, "Config error: {}", msg),
            ServerError::ClientError(msg) => write!(f, "Client error: {}", msg),
        }
    }
}

impl std::error::Error for ServerError {}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::action::SpeakVolume;
    use pod_core::decode_toon_value;
    use tokio_tungstenite::{connect_async, tungstenite::Message};

    fn next_available_port() -> u16 {
        std::net::TcpListener::bind("127.0.0.1:0")
            .expect("ephemeral port bind")
            .local_addr()
            .expect("local addr")
            .port()
    }

    async fn drive_server_for(mut server: GameServer, iterations: usize) {
        for _ in 0..iterations {
            server.handle_connections().await.unwrap();
            server.step_tick().await.unwrap();
            server.broadcast_updates().await.unwrap();
            tokio::time::sleep(Duration::from_millis(5)).await;
            server.tick += 1;
        }
    }

    #[tokio::test]
    async fn test_server_creation() {
        let config = ProtoServerConfig::default();
        let world = World::new(42);
        let server = GameServer::new(config, world);

        assert_eq!(server.current_tick(), 0);
        assert_eq!(server.client_count().await, 0);
    }

    #[tokio::test]
    async fn test_build_full_snapshot_message_marks_full_resync() {
        let config = ProtoServerConfig::default();
        let world = World::new(42);
        let server = GameServer::new(config, world);
        let snapshot = WorldSnapshot::capture(&server.world);

        let message = server.build_full_snapshot_message(snapshot, Some(12));
        match message {
            ServerMessage::StateDelta {
                tick,
                acknowledged_action_tick,
                authoritative_digest,
                is_full_snapshot,
                delta,
            } => {
                assert_eq!(tick, 0);
                assert_eq!(acknowledged_action_tick, Some(12));
                assert!(is_full_snapshot);
                assert_eq!(delta.destroyed.len(), 0);
                let snapshot = WorldSnapshot {
                    tick,
                    entities: delta.updated,
                    population: delta.population,
                };
                assert_eq!(snapshot.digest(), authoritative_digest);
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_broadcast_updates_emits_debug_documents_for_debug_clients_only() {
        let config = ProtoServerConfig::default();
        let world = World::new(42);
        let mut server = GameServer::new(config, world);
        let client_id = ClientId::new();
        let (tx, mut rx) = mpsc::channel(8);
        server.client_tx.write().await.insert(client_id, tx);
        server.clients.write().await.insert(
            client_id,
            ClientSession {
                player_name: Some("debug".into()),
                agent_id: None,
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: true,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );

        server.step_tick().await.unwrap();
        server.broadcast_updates().await.unwrap();

        let mut saw_tick_telemetry = false;
        while let Ok(message) = rx.try_recv() {
            match message {
                ServerMessage::DebugDocument { document } => {
                    let value =
                        decode_toon_value(&document).expect("tick telemetry TOON should decode");
                    if value["document_type"] == "versioned_tick_telemetry" {
                        saw_tick_telemetry = true;
                    }
                }
                _ => {}
            }
        }

        assert!(saw_tick_telemetry);
    }

    #[tokio::test]
    async fn test_broadcast_updates_filters_debug_documents_per_client_interest() {
        use pod_core::telemetry::{AgentTelemetryFrame, AgentToolCallTrace, ToolCallStatus};
        use pod_core::AgentRuntimeProfile;

        let config = ProtoServerConfig::default();
        let mut world = World::new(42);
        let (alpha_slot, alpha_entity) = world.add_agent(Box::new(IdleAgent::new()));
        let (spire_slot, spire_entity) = world.add_agent(Box::new(IdleAgent::new()));
        world
            .ecs
            .get::<&mut pod_core::Transform>(alpha_entity)
            .expect("alpha transform")
            .position = glam::Vec2::new(1.0, 1.0);
        world
            .ecs
            .get::<&mut pod_core::Transform>(spire_entity)
            .expect("spire transform")
            .position = glam::Vec2::new(30.0, 1.0);

        let mut server = GameServer::new(config, world);
        let client_id = ClientId::new();
        let (tx, mut rx) = mpsc::channel(8);
        server.client_tx.write().await.insert(client_id, tx);
        server.clients.write().await.insert(
            client_id,
            ClientSession {
                player_name: Some("alpha-debug".into()),
                agent_id: Some(server.world.agents[alpha_slot].agent.id()),
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: true,
                debug_focus_entity: Some(alpha_entity.id() as u64),
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );

        let alpha_trace = AgentToolCallTrace::success(7, "llm.complete", "openai", 24, 40, 12);
        let spire_trace = AgentToolCallTrace::new(
            7,
            "llm.complete",
            "openai",
            ToolCallStatus::Failed,
            31,
            50,
            0,
            Some("timeout".into()),
        );
        let telemetry_frame = TickTelemetryFrame {
            tick: 7,
            agents: vec![
                AgentTelemetryFrame {
                    tick: 7,
                    agent_id: server.world.agents[alpha_slot].agent.id(),
                    entity_id: Some(pod_core::EntityId(alpha_entity.id() as u64)),
                    runtime_profile: AgentRuntimeProfile::default(),
                    visible_entity_count: 2,
                    audible_event_count: 1,
                    message_count: 0,
                    available_action_count: 3,
                    objective_count: 1,
                    encounter: None,
                    trajectory: None,
                    action_trace: Vec::new(),
                    tool_calls: vec![alpha_trace.clone()],
                },
                AgentTelemetryFrame {
                    tick: 7,
                    agent_id: server.world.agents[spire_slot].agent.id(),
                    entity_id: Some(pod_core::EntityId(spire_entity.id() as u64)),
                    runtime_profile: AgentRuntimeProfile::default(),
                    visible_entity_count: 3,
                    audible_event_count: 0,
                    message_count: 0,
                    available_action_count: 3,
                    objective_count: 1,
                    encounter: None,
                    trajectory: None,
                    action_trace: Vec::new(),
                    tool_calls: vec![spire_trace.clone()],
                },
            ],
        };
        server.debug_archive.record_tick(telemetry_frame.clone());
        server.pending_debug_documents = vec![
            PendingDebugDocument::TickTelemetry(VersionedTickTelemetry::new(telemetry_frame)),
            PendingDebugDocument::AgentToolCallEvent(AgentToolCallEvent::new(
                alpha_entity.id() as u64,
                alpha_trace,
            )),
            PendingDebugDocument::AgentToolCallEvent(AgentToolCallEvent::new(
                spire_entity.id() as u64,
                spire_trace,
            )),
        ];

        server.broadcast_updates().await.unwrap();

        let mut documents = Vec::new();
        while let Ok(message) = rx.try_recv() {
            if let ServerMessage::DebugDocument { document } = message {
                documents.push(document);
            }
        }

        assert_eq!(documents.len(), 4);
        let tick_doc = documents
            .iter()
            .find(|document| document.contains("versioned_tick_telemetry"))
            .expect("tick telemetry document");
        let tick_value =
            decode_toon_value(tick_doc).expect("filtered tick telemetry TOON should decode");
        let agent_frames = tick_value["payload"]["payload"]["agents"]
            .as_array()
            .expect("agents array");
        assert_eq!(agent_frames.len(), 1);
        assert_eq!(
            agent_frames[0]["entity_id"].as_u64(),
            Some(alpha_entity.id() as u64)
        );

        assert!(documents.iter().any(|document| {
            decode_toon_value(document)
                .map(|value| {
                    value["document_type"] == "agent_tool_call_event"
                        && value["payload"]["agent_entity_id"] == alpha_entity.id() as u64
                })
                .unwrap_or(false)
        }));
        assert!(documents.iter().any(|document| {
            decode_toon_value(document)
                .map(|value| {
                    value["document_type"] == "focused_entity_debug_summary"
                        && value["payload"]["entity_id"] == alpha_entity.id() as u64
                })
                .unwrap_or(false)
        }));
        assert!(documents.iter().any(|document| {
            decode_toon_value(document)
                .map(|value| value["document_type"] == "shard_transport_summary")
                .unwrap_or(false)
        }));
        assert!(!documents.iter().any(|document| {
            decode_toon_value(document)
                .map(|value| {
                    value["document_type"] == "agent_tool_call_event"
                        && value["payload"]["agent_entity_id"] == spire_entity.id() as u64
                })
                .unwrap_or(false)
        }));
    }

    #[tokio::test]
    async fn test_transport_summary_tracks_message_counts() {
        let config = ProtoServerConfig::default();
        let world = World::new(42);
        let mut server = GameServer::new(config, world);
        let client_id = ClientId::new();
        let (tx, _rx) = mpsc::channel(8);
        server.client_tx.write().await.insert(client_id, tx);
        server.clients.write().await.insert(
            client_id,
            ClientSession {
                player_name: Some("debug".into()),
                agent_id: None,
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: true,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );

        server
            .inbound_tx
            .send(InboundPacket::Message {
                client_id,
                encoded_len: 88,
                message: ClientMessage::Ping { timestamp: 7 },
            })
            .await
            .expect("ping message queued");
        server.handle_connections().await.unwrap();

        let summary = server.transport_summary().await;
        assert_eq!(summary.client_count, 1);
        assert_eq!(summary.total_inbound_messages, 1);
        assert!(summary.total_outbound_messages >= 1);
        assert_eq!(summary.ping_requests, 1);
        assert_eq!(summary.full_snapshots_sent, 0);
        assert_eq!(summary.delta_messages_sent, 0);
        assert_eq!(summary.clients[0].client_id, client_id.0.to_string());
    }

    #[tokio::test]
    async fn test_transport_summary_marks_queue_pressure_clients() {
        let config = ProtoServerConfig {
            queue_pressure_warn_depth: 2,
            ..ProtoServerConfig::default()
        };
        let world = World::new(42);
        let mut server = GameServer::new(config, world);
        server.resumed_sessions = 2;
        server.recovery_snapshots_sent = 3;
        server.recovery_delivery_failures = 1;
        let client_id = ClientId::new();
        let (tx, _rx) = mpsc::channel(8);
        server.client_tx.write().await.insert(client_id, tx);
        server.clients.write().await.insert(
            client_id,
            ClientSession {
                player_name: Some("debug".into()),
                agent_id: None,
                pending_actions: vec![(1, Action::Idle), (1, Action::Stop)],
                reconnect_token: ReconnectToken::new(),
                last_action_tick: Some(1),
                last_processed_action_tick: None,
                debug_telemetry_enabled: true,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters {
                    session_resumes: 2,
                    recovery_snapshots_sent: 2,
                    recovery_delivery_failures: 1,
                    ..ClientTransportCounters::default()
                },
            },
        );

        let summary = server.transport_summary().await;
        assert_eq!(summary.resumed_sessions, 2);
        assert_eq!(summary.recovery_snapshots_sent, 3);
        assert_eq!(summary.recovery_delivery_failures, 1);
        assert_eq!(summary.queue_pressure_client_count, 1);
        assert_eq!(summary.peak_pending_action_queue_depth, 2);
        assert_eq!(summary.clients[0].session_resumes, 2);
        assert_eq!(summary.clients[0].recovery_snapshots_sent, 2);
        assert_eq!(summary.clients[0].recovery_delivery_failures, 1);
        assert_eq!(summary.clients[0].pending_action_queue_depth, 2);
        assert_eq!(summary.clients[0].peak_pending_action_queue_depth, 2);
        assert!(summary.clients[0].queue_pressure);
    }

    #[tokio::test]
    async fn test_send_full_snapshot_tracks_recovery_delivery_success() {
        let config = ProtoServerConfig::default();
        let world = World::new(42);
        let mut server = GameServer::new(config, world);
        let client_id = ClientId::new();
        let (tx, mut rx) = mpsc::channel(8);
        server.client_tx.write().await.insert(client_id, tx);
        server.clients.write().await.insert(
            client_id,
            ClientSession {
                player_name: Some("debug".into()),
                agent_id: None,
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: Some(4),
                debug_telemetry_enabled: false,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );

        server.send_full_snapshot_to_client(client_id).await;

        let summary = server.transport_summary().await;
        assert_eq!(summary.recovery_snapshots_sent, 1);
        assert_eq!(summary.recovery_delivery_failures, 0);
        assert_eq!(summary.full_snapshots_sent, 1);
        assert!(summary.total_full_snapshot_bytes > 0);
        assert!(summary.total_recovery_snapshot_bytes > 0);
        assert!(summary.max_full_snapshot_bytes > 0);
        assert_eq!(summary.clients[0].recovery_snapshots_sent, 1);
        assert!(summary.clients[0].full_snapshot_bytes > 0);
        assert!(summary.clients[0].recovery_snapshot_bytes_sent > 0);

        let message = rx.recv().await.expect("recovery snapshot delivered");
        assert!(matches!(message, ServerMessage::StateDelta { is_full_snapshot: true, .. }));
    }

    #[tokio::test]
    async fn test_handle_connections_request_full_snapshot_tracks_recovery_transport_counters() {
        let config = ProtoServerConfig::default();
        let world = World::new(42);
        let mut server = GameServer::new(config, world);
        let client_id = ClientId::new();
        let mut rx = server.register_pending_client(client_id).await;
        {
            let mut clients = server.clients.write().await;
            let session = clients.get_mut(&client_id).expect("registered session");
            session.player_name = Some("recovering".into());
            session.last_processed_action_tick = Some(4);
        }

        let request = ClientMessage::RequestFullSnapshot {
            last_known_tick: Some(3),
            last_known_digest: Some(123),
        };
        let encoded_len = request.encode().unwrap().len();
        server
            .inbound_tx
            .send(InboundPacket::Message {
                client_id,
                encoded_len,
                message: request,
            })
            .await
            .expect("queue full snapshot request");

        server.handle_connections().await.unwrap();

        let summary = server.transport_summary().await;
        assert_eq!(summary.full_snapshot_requests, 1);
        assert_eq!(summary.recovery_snapshots_sent, 1);
        assert_eq!(summary.recovery_delivery_failures, 0);
        assert_eq!(summary.full_snapshots_sent, 1);
        assert!(summary.total_full_snapshot_bytes > 0);
        assert!(summary.total_recovery_snapshot_bytes > 0);
        assert_eq!(summary.clients[0].full_snapshot_requests, 1);
        assert_eq!(summary.clients[0].inbound_messages, 1);
        assert_eq!(summary.clients[0].recovery_snapshots_sent, 1);
        assert_eq!(summary.clients[0].recovery_delivery_failures, 0);
        assert!(summary.clients[0].full_snapshot_bytes > 0);
        assert!(summary.clients[0].recovery_snapshot_bytes_sent > 0);

        let message = rx.recv().await.expect("recovery snapshot delivered");
        match message {
            ServerMessage::StateDelta {
                acknowledged_action_tick,
                is_full_snapshot,
                ..
            } => {
                assert!(is_full_snapshot);
                assert_eq!(acknowledged_action_tick, Some(4));
            }
            other => panic!("unexpected recovery message: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_send_full_snapshot_tracks_recovery_delivery_failure() {
        let config = ProtoServerConfig::default();
        let world = World::new(42);
        let mut server = GameServer::new(config, world);
        let client_id = ClientId::new();
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        server.client_tx.write().await.insert(client_id, tx);
        server.clients.write().await.insert(
            client_id,
            ClientSession {
                player_name: Some("debug".into()),
                agent_id: None,
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: false,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );

        server.send_full_snapshot_to_client(client_id).await;

        let summary = server.transport_summary().await;
        assert_eq!(summary.recovery_snapshots_sent, 0);
        assert_eq!(summary.recovery_delivery_failures, 1);
        assert_eq!(summary.clients[0].recovery_delivery_failures, 1);
        assert_eq!(summary.total_recovery_snapshot_bytes, 0);
    }

    #[tokio::test]
    async fn test_handle_connections_resume_connect_preserves_transport_counters() {
        let config = ProtoServerConfig::default();
        let world = World::new(42);
        let mut server = GameServer::new(config, world);
        let previous_client_id = ClientId::new();
        let mut previous_rx = server.register_pending_client(previous_client_id).await;

        server
            .attach_remote_agent(previous_client_id, "resume-player".into(), None)
            .await
            .unwrap();

        let (reconnect_token, controlled_entity) = match previous_rx
            .recv()
            .await
            .expect("initial welcome")
        {
            ServerMessage::Welcome {
                reconnect_token,
                controlled_entity,
                ..
            } => (reconnect_token, controlled_entity),
            other => panic!("unexpected initial welcome: {other:?}"),
        };

        {
            let mut clients = server.clients.write().await;
            let session = clients
                .get_mut(&previous_client_id)
                .expect("previous session");
            session.last_action_tick = Some(9);
            session.last_processed_action_tick = Some(7);
            session.debug_telemetry_enabled = true;
            session.debug_focus_entity = controlled_entity;
            session.transport.session_resumes = 3;
            session.transport.recovery_snapshots_sent = 2;
            session.transport.recovery_snapshot_bytes_sent = 333;
            session.transport.full_snapshot_requests = 2;
            session.transport.delta_messages_sent = 5;
            session.transport.delta_bytes_sent = 512;
            session.transport.max_delta_bytes = 256;
            session.transport.delta_entities_updated = 11;
            session.transport.delta_entities_destroyed = 2;
            session.transport.queue_pressure_events = 4;
            session.transport.peak_pending_action_queue_depth = 9;
        }

        let resumed_client_id = ClientId::new();
        let mut resumed_rx = server.register_pending_client(resumed_client_id).await;
        let resume_connect = ClientMessage::Connect {
            player_name: "resume-player".into(),
            reconnect_token: Some(reconnect_token),
        };
        let encoded_len = resume_connect.encode().unwrap().len();
        server
            .inbound_tx
            .send(InboundPacket::Message {
                client_id: resumed_client_id,
                encoded_len,
                message: resume_connect,
            })
            .await
            .expect("queue resume connect");

        server.handle_connections().await.unwrap();

        assert!(!server.clients.read().await.contains_key(&previous_client_id));
        assert!(!server.client_tx.read().await.contains_key(&previous_client_id));
        assert_eq!(server.client_count().await, 1);
        assert_eq!(server.resumed_sessions, 1);

        let summary = server.transport_summary().await;
        assert_eq!(summary.resumed_sessions, 1);
        assert_eq!(summary.client_count, 1);
        assert_eq!(summary.clients[0].client_id, resumed_client_id.0.to_string());
        assert_eq!(summary.clients[0].session_resumes, 4);
        assert_eq!(summary.clients[0].recovery_snapshots_sent, 2);
        assert_eq!(summary.clients[0].full_snapshot_requests, 2);
        assert_eq!(summary.clients[0].delta_messages_sent, 5);
        assert_eq!(summary.clients[0].delta_bytes_sent, 512);
        assert_eq!(summary.clients[0].max_delta_bytes, 256);
        assert_eq!(summary.clients[0].delta_entities_updated, 11);
        assert_eq!(summary.clients[0].delta_entities_destroyed, 2);
        assert_eq!(summary.clients[0].queue_pressure_events, 4);
        assert_eq!(summary.clients[0].peak_pending_action_queue_depth, 9);
        assert!(summary.clients[0].debug_telemetry_enabled);

        let message = resumed_rx.recv().await.expect("resume welcome");
        match message {
            ServerMessage::Welcome {
                reconnect_token: resumed_token,
                controlled_entity: resumed_entity,
                acknowledged_action_tick,
                ..
            } => {
                assert_eq!(resumed_token, reconnect_token);
                assert_eq!(resumed_entity, controlled_entity);
                assert_eq!(acknowledged_action_tick, Some(7));
            }
            other => panic!("unexpected resume welcome: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_send_to_client_tracks_delta_bytes_and_entity_churn() {
        let config = ProtoServerConfig::default();
        let world = World::new(42);
        let server = GameServer::new(config, world);
        let client_id = ClientId::new();
        let (tx, _rx) = mpsc::channel(8);
        server.client_tx.write().await.insert(client_id, tx);
        server.clients.write().await.insert(
            client_id,
            ClientSession {
                player_name: Some("debug".into()),
                agent_id: None,
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: false,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );

        let sent = server
            .send_to_client(
                client_id,
                ServerMessage::StateDelta {
                    tick: 7,
                    acknowledged_action_tick: None,
                    authoritative_digest: 44,
                    is_full_snapshot: false,
                    delta: StateDelta {
                        tick: 7,
                        updated: vec![crate::snapshot::EntitySnapshot {
                            id: 9,
                            position: [2.0, 3.0].into(),
                            velocity: [0.0, 0.0].into(),
                            rotation: 0.0,
                            health: None,
                            max_health: None,
                            movement_speed: None,
                            label: Some("Delta".into()),
                            metadata: Default::default(),
                        }],
                        destroyed: vec![3, 4],
                        population: Default::default(),
                    },
                },
            )
            .await;

        assert!(sent);

        let summary = server.transport_summary().await;
        assert_eq!(summary.state_deltas_sent, 1);
        assert_eq!(summary.delta_messages_sent, 1);
        assert!(summary.total_delta_bytes > 0);
        assert!(summary.max_delta_bytes > 0);
        assert_eq!(summary.total_delta_entities_updated, 1);
        assert_eq!(summary.total_delta_entities_destroyed, 2);
        assert_eq!(summary.clients[0].delta_messages_sent, 1);
        assert!(summary.clients[0].delta_bytes_sent > 0);
        assert_eq!(summary.clients[0].delta_entities_updated, 1);
        assert_eq!(summary.clients[0].delta_entities_destroyed, 2);
    }

    #[tokio::test]
    async fn test_prune_stale_clients_disconnects_inactive_sessions() {
        let config = ProtoServerConfig {
            client_inactivity_timeout_ticks: 3,
            ..ProtoServerConfig::default()
        };
        let world = World::new(42);
        let mut server = GameServer::new(config, world);
        let client_id = ClientId::new();
        let (tx, _rx) = mpsc::channel(8);
        server.client_tx.write().await.insert(client_id, tx);
        server.clients.write().await.insert(
            client_id,
            ClientSession {
                player_name: Some("idle".into()),
                agent_id: None,
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: false,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters {
                    last_seen_tick: 1,
                    ..ClientTransportCounters::default()
                },
            },
        );
        server.tick = 5;

        server.prune_stale_clients().await;

        assert_eq!(server.client_count().await, 0);
        let summary = server.transport_summary().await;
        assert_eq!(summary.timed_out_clients, 1);
    }

    #[tokio::test]
    async fn test_broadcast_updates_emits_shard_transport_summary_for_debug_clients() {
        let config = ProtoServerConfig::default();
        let world = World::new(42);
        let mut server = GameServer::new(config, world);
        let client_id = ClientId::new();
        let (tx, mut rx) = mpsc::channel(8);
        server.client_tx.write().await.insert(client_id, tx);
        server.clients.write().await.insert(
            client_id,
            ClientSession {
                player_name: Some("debug".into()),
                agent_id: None,
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: true,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );
        server.tick = TRANSPORT_SUMMARY_INTERVAL_TICKS;

        server.broadcast_updates().await.unwrap();

        let mut saw_transport = false;
        while let Ok(message) = rx.try_recv() {
            if let ServerMessage::DebugDocument { document } = message {
                let value = decode_toon_value(&document).expect("debug document should decode");
                if value["document_type"] == "shard_transport_summary" {
                    saw_transport = true;
                    assert_eq!(value["payload"]["client_count"], 1);
                }
            }
        }

        assert!(saw_transport);
    }

    #[tokio::test]
    async fn test_broadcast_updates_filters_snapshots_per_client_interest() {
        let config = ProtoServerConfig::default();
        let mut world = World::new(42);
        let (alpha_slot, alpha_entity) = world.add_agent(Box::new(IdleAgent::new()));
        let (spire_slot, spire_entity) = world.add_agent(Box::new(IdleAgent::new()));
        world
            .ecs
            .get::<&mut pod_core::Transform>(alpha_entity)
            .expect("alpha transform")
            .position = glam::Vec2::new(1.0, 1.0);
        world
            .ecs
            .get::<&mut pod_core::Transform>(spire_entity)
            .expect("spire transform")
            .position = glam::Vec2::new(30.0, 1.0);
        let near_resource = world
            .spawn_at(2.0, 1.0)
            .with_label("Near Resource", pod_core::Team::None)
            .build();
        let far_resource = world
            .spawn_at(31.0, 1.0)
            .with_label("Far Resource", pod_core::Team::None)
            .build();

        let mut server = GameServer::new(config, world);
        let alpha_client = ClientId::new();
        let spire_client = ClientId::new();
        let (alpha_tx, mut alpha_rx) = mpsc::channel(8);
        let (spire_tx, mut spire_rx) = mpsc::channel(8);
        server
            .client_tx
            .write()
            .await
            .insert(alpha_client, alpha_tx);
        server
            .client_tx
            .write()
            .await
            .insert(spire_client, spire_tx);
        server.clients.write().await.insert(
            alpha_client,
            ClientSession {
                player_name: Some("alpha".into()),
                agent_id: Some(server.world.agents[alpha_slot].agent.id()),
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: false,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );
        server.clients.write().await.insert(
            spire_client,
            ClientSession {
                player_name: Some("spire".into()),
                agent_id: Some(server.world.agents[spire_slot].agent.id()),
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: false,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );

        server.broadcast_updates().await.unwrap();

        let alpha_state = loop {
            match alpha_rx.try_recv() {
                Ok(ServerMessage::StateDelta { delta, .. }) => break delta,
                Ok(_) => continue,
                Err(err) => panic!("missing alpha state delta: {err}"),
            }
        };
        let spire_state = loop {
            match spire_rx.try_recv() {
                Ok(ServerMessage::StateDelta { delta, .. }) => break delta,
                Ok(_) => continue,
                Err(err) => panic!("missing spire state delta: {err}"),
            }
        };

        let alpha_ids = alpha_state
            .updated
            .iter()
            .map(|entity| entity.id)
            .collect::<Vec<_>>();
        let spire_ids = spire_state
            .updated
            .iter()
            .map(|entity| entity.id)
            .collect::<Vec<_>>();

        assert!(alpha_ids.contains(&(alpha_entity.id() as u64)));
        assert!(alpha_ids.contains(&(near_resource.id() as u64)));
        assert!(!alpha_ids.contains(&(spire_entity.id() as u64)));
        assert!(!alpha_ids.contains(&(far_resource.id() as u64)));

        assert!(spire_ids.contains(&(spire_entity.id() as u64)));
        assert!(spire_ids.contains(&(far_resource.id() as u64)));
        assert!(!spire_ids.contains(&(alpha_entity.id() as u64)));
        assert!(!spire_ids.contains(&(near_resource.id() as u64)));
    }

    #[tokio::test]
    async fn test_broadcast_updates_emits_authoritative_event_batches() {
        let config = ProtoServerConfig::default();
        let world = World::new(42);
        let mut server = GameServer::new(config, world);
        let client_id = ClientId::new();
        let (tx, mut rx) = mpsc::channel(8);
        server.client_tx.write().await.insert(client_id, tx);
        server.clients.write().await.insert(
            client_id,
            ClientSession {
                player_name: Some("player".into()),
                agent_id: None,
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: false,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );
        server.tick = 12;
        server.pending_events = vec![pod_core::GameEvent {
            tick: 12,
            origin: glam::Vec2::new(32.0, 48.0),
            event: pod_core::Event::AgentSpoke {
                agent_id: pod_core::AgentId::new(),
                message: "hello shard".into(),
                volume: 200.0,
            },
        }];

        server.broadcast_updates().await.unwrap();

        let mut saw_event_batch = false;
        while let Ok(message) = rx.try_recv() {
            if let ServerMessage::EventBatch { tick, events } = message {
                saw_event_batch = true;
                assert_eq!(tick, 12);
                assert_eq!(events.len(), 1);
                assert!(matches!(
                    events[0].event,
                    pod_core::Event::AgentSpoke { .. }
                ));
            }
        }

        assert!(saw_event_batch);
        assert!(server.pending_events.is_empty());
    }

    #[tokio::test]
    async fn test_broadcast_updates_filters_event_batches_per_client_interest() {
        let config = ProtoServerConfig::default();
        let mut world = World::new(42);
        let (alpha_slot, alpha_entity) = world.add_agent(Box::new(IdleAgent::new()));
        let (spire_slot, spire_entity) = world.add_agent(Box::new(IdleAgent::new()));
        world
            .ecs
            .get::<&mut pod_core::Transform>(alpha_entity)
            .expect("alpha transform")
            .position = glam::Vec2::new(1.0, 1.0);
        world
            .ecs
            .get::<&mut pod_core::Transform>(spire_entity)
            .expect("spire transform")
            .position = glam::Vec2::new(30.0, 1.0);

        let mut server = GameServer::new(config, world);
        let alpha_client = ClientId::new();
        let spire_client = ClientId::new();
        let (alpha_tx, mut alpha_rx) = mpsc::channel(8);
        let (spire_tx, mut spire_rx) = mpsc::channel(8);
        server
            .client_tx
            .write()
            .await
            .insert(alpha_client, alpha_tx);
        server
            .client_tx
            .write()
            .await
            .insert(spire_client, spire_tx);
        server.clients.write().await.insert(
            alpha_client,
            ClientSession {
                player_name: Some("alpha".into()),
                agent_id: Some(server.world.agents[alpha_slot].agent.id()),
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: false,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );
        server.clients.write().await.insert(
            spire_client,
            ClientSession {
                player_name: Some("spire".into()),
                agent_id: Some(server.world.agents[spire_slot].agent.id()),
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: false,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );
        server.tick = 21;
        server.pending_events = vec![
            pod_core::GameEvent {
                tick: 21,
                origin: glam::Vec2::new(2.0, 1.0),
                event: pod_core::Event::AgentSpoke {
                    agent_id: pod_core::AgentId::new(),
                    message: "alpha event".into(),
                    volume: 200.0,
                },
            },
            pod_core::GameEvent {
                tick: 21,
                origin: glam::Vec2::new(31.0, 1.0),
                event: pod_core::Event::AgentSpoke {
                    agent_id: pod_core::AgentId::new(),
                    message: "spire event".into(),
                    volume: 200.0,
                },
            },
        ];

        server.broadcast_updates().await.unwrap();

        let alpha_events = loop {
            match alpha_rx.try_recv() {
                Ok(ServerMessage::EventBatch { events, .. }) => break events,
                Ok(_) => continue,
                Err(err) => panic!("missing alpha event batch: {err}"),
            }
        };
        let spire_events = loop {
            match spire_rx.try_recv() {
                Ok(ServerMessage::EventBatch { events, .. }) => break events,
                Ok(_) => continue,
                Err(err) => panic!("missing spire event batch: {err}"),
            }
        };

        assert_eq!(alpha_events.len(), 1);
        assert_eq!(alpha_events[0].origin, glam::Vec2::new(2.0, 1.0));
        assert_eq!(spire_events.len(), 1);
        assert_eq!(spire_events[0].origin, glam::Vec2::new(31.0, 1.0));
    }

    #[tokio::test]
    async fn test_step_tick_promotes_speak_action_into_event_batch() {
        let config = ProtoServerConfig::default();
        let mut server = GameServer::new(config, World::new(42));
        let client_id = ClientId::new();
        let (tx, mut rx) = mpsc::channel(8);
        let (slot_index, _) = server.world.add_agent(Box::new(IdleAgent::new()));
        let agent_id = server.world.agents[slot_index].agent.id();

        server.client_tx.write().await.insert(client_id, tx);
        server.clients.write().await.insert(
            client_id,
            ClientSession {
                player_name: Some("speaker".into()),
                agent_id: Some(agent_id),
                pending_actions: Vec::new(),
                reconnect_token: ReconnectToken::new(),
                last_action_tick: None,
                last_processed_action_tick: None,
                debug_telemetry_enabled: false,
                debug_focus_entity: None,
                last_sent_snapshot: None,
                transport: ClientTransportCounters::default(),
            },
        );
        server.world.submit_external_action(
            agent_id,
            Action::Speak {
                message: "authoritative hello".into(),
                volume: SpeakVolume::Normal,
            },
        );

        server.step_tick().await.unwrap();
        assert!(server
            .pending_events
            .iter()
            .any(|event| matches!(event.event, pod_core::Event::AgentSpoke { .. })));

        server.broadcast_updates().await.unwrap();

        let mut saw_spoken_event = false;
        while let Ok(message) = rx.try_recv() {
            if let ServerMessage::EventBatch { events, .. } = message {
                saw_spoken_event = events
                    .iter()
                    .any(|event| matches!(event.event, pod_core::Event::AgentSpoke { .. }));
            }
        }

        assert!(saw_spoken_event);
    }

    #[tokio::test]
    async fn test_websocket_connect_receives_welcome() {
        let bind_port = next_available_port();
        let websocket_port = next_available_port();
        let config = ProtoServerConfig {
            bind_addr: "127.0.0.1".into(),
            bind_port,
            enable_websocket: true,
            websocket_port,
            ..ProtoServerConfig::default()
        };

        let mut server = GameServer::new(config, World::new(42));
        server.initialize().await.unwrap();
        let client_task = async move {
            let (mut websocket, _) = connect_async(format!("ws://127.0.0.1:{websocket_port}"))
                .await
                .expect("websocket connection");
            websocket
                .send(Message::Text(
                    ClientMessage::Connect {
                        player_name: "WebPlayer".into(),
                        reconnect_token: None,
                    }
                    .encode_json()
                    .unwrap()
                    .into(),
                ))
                .await
                .expect("send connect");

            let mut welcome = None;
            for _ in 0..40 {
                let next_message =
                    tokio::time::timeout(Duration::from_millis(50), websocket.next()).await;
                if let Ok(Some(message)) = next_message {
                    let message = message.expect("websocket message");
                    if let Message::Text(text) = message {
                        if let Ok(ServerMessage::Welcome { .. }) =
                            ServerMessage::decode_json(text.as_ref())
                        {
                            welcome = Some(text.to_string());
                            break;
                        }
                    }
                }
            }

            welcome.expect("welcome message")
        };

        let (_, welcome) = tokio::join!(drive_server_for(server, 200), client_task);
        match ServerMessage::decode_json(&welcome).unwrap() {
            ServerMessage::Welcome {
                controlled_entity,
                snapshot,
                ..
            } => {
                assert!(controlled_entity.is_some());
                assert!(!snapshot.entities.is_empty());
            }
            other => panic!("unexpected websocket server message: {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_websocket_debug_subscriber_receives_debug_document() {
        let bind_port = next_available_port();
        let websocket_port = next_available_port();
        let config = ProtoServerConfig {
            bind_addr: "127.0.0.1".into(),
            bind_port,
            enable_websocket: true,
            websocket_port,
            ..ProtoServerConfig::default()
        };

        let mut server = GameServer::new(config, World::new(42));
        server.initialize().await.unwrap();
        let client_task = async move {
            let (mut websocket, _) = connect_async(format!("ws://127.0.0.1:{websocket_port}"))
                .await
                .expect("websocket connection");

            websocket
                .send(Message::Text(
                    ClientMessage::Connect {
                        player_name: "DebugPlayer".into(),
                        reconnect_token: None,
                    }
                    .encode_json()
                    .unwrap()
                    .into(),
                ))
                .await
                .expect("send connect");

            websocket
                .send(Message::Text(
                    ClientMessage::SetDebugTelemetry { enabled: true }
                        .encode_json()
                        .unwrap()
                        .into(),
                ))
                .await
                .expect("enable debug telemetry");

            let mut documents = Vec::new();
            for _ in 0..16 {
                let next_message =
                    tokio::time::timeout(Duration::from_millis(50), websocket.next()).await;
                if let Ok(Some(message)) = next_message {
                    let message = message.expect("websocket message");
                    if let Message::Text(text) = message {
                        documents.push(text.to_string());
                    }
                }
            }
            documents
        };

        let (_, documents) = tokio::join!(drive_server_for(server, 200), client_task);

        let decoded: Vec<ServerMessage> = documents
            .iter()
            .filter_map(|document| ServerMessage::decode_json(document).ok())
            .collect();
        assert!(decoded.iter().any(|message| matches!(
            message,
            ServerMessage::DebugDocument { document }
            if decode_toon_value(document)
                .map(|value| value["document_type"] == "versioned_tick_telemetry")
                .unwrap_or(false)
        )));
    }
}
