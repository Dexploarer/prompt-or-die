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
    Action, AgentTickRollup, AgentToolCallEvent, IdleAgent, TelemetryArchive, TelemetryConfig,
    VersionedTickTelemetry, World, GameEvent,
};

use crate::protocol::{
    ClientId, ClientMessage, ReconnectToken, ServerConfig as ProtoServerConfig, ServerMessage,
};
use crate::snapshot::{StateDelta, WorldSnapshot};

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
        }
    }
}

#[derive(Debug)]
enum InboundPacket {
    Message {
        client_id: ClientId,
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
    pending_debug_documents: Vec<String>,
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

            // Send updates to clients
            self.broadcast_updates().await?;

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

        let telemetry_document =
            VersionedTickTelemetry::new(tick_result.telemetry.clone()).to_toon_document();
        self.last_tick_telemetry_json = Some(telemetry_document.clone());
        self.pending_debug_documents.push(telemetry_document);

        for agent in &tick_result.telemetry.agents {
            let Some(entity_id) = agent.entity_id else {
                continue;
            };
            for trace in &agent.tool_calls {
                self.pending_debug_documents
                    .push(AgentToolCallEvent::new(entity_id.0, trace.clone()).to_toon_document());
            }
        }

        if (tick_result.tick + 1) % 60 == 0 {
            for rollup in self.debug_rollups_for_tick(tick_result.tick) {
                self.pending_debug_documents.push(rollup.to_toon_document());
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

    async fn register_pending_client(
        &mut self,
        client_id: ClientId,
    ) -> mpsc::Receiver<ServerMessage> {
        let (outbound_tx, outbound_rx) = mpsc::channel::<ServerMessage>(256);
        self.client_tx.write().await.insert(client_id, outbound_tx);
        self.clients
            .write()
            .await
            .insert(client_id, ClientSession::new("pending".into()));
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
                                .send(InboundPacket::Message { client_id, message })
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
                                .send(InboundPacket::Message { client_id, message })
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

                    if let Err(err) = ws_write.send(Message::Text(payload.into())).await {
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
                            .send(InboundPacket::Message { client_id, message })
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
                InboundPacket::Message { client_id, message } => match message {
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
                        let min_tick = self.tick.saturating_sub(ACTION_WINDOW_BACKWARD_TICKS);
                        let max_tick = self.tick + ACTION_WINDOW_FORWARD_TICKS;
                        {
                            let mut clients = self.clients.write().await;
                            if let Some(session) = clients.get_mut(&client_id) {
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
                                        session.last_action_tick = Some(tick);
                                    }
                                }
                            }
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
                    ClientMessage::Ping { timestamp } => {
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
                },
                InboundPacket::Disconnected { client_id, reason } => {
                    self.disconnect_client(client_id, &reason).await;
                }
            }
        }

        Ok(())
    }

    /// Broadcast world updates to all connected clients
    async fn broadcast_updates(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Decide whether to send full snapshot or delta
        let should_snapshot =
            (self.tick - self.last_snapshot_tick) >= self.config.snapshot_interval;
        let current_snapshot = WorldSnapshot::capture(&self.world);
        let authoritative_digest = current_snapshot.digest();

        let is_full_snapshot = should_snapshot || self.last_snapshot.is_none();
        let delta = if is_full_snapshot {
            StateDelta {
                tick: self.tick,
                updated: current_snapshot.entities.clone(),
                destroyed: vec![],
            }
        } else {
            StateDelta::diff(
                self.last_snapshot
                    .as_ref()
                    .expect("last_snapshot checked above"),
                &current_snapshot,
            )
        };

        let has_state_update = should_snapshot || delta.change_count() > 0;
        let has_event_update = !self.pending_events.is_empty();
        let clients = self.clients.read().await;
        let has_debug_subscribers = clients
            .values()
            .any(|session| session.debug_telemetry_enabled);

        if !has_state_update && !has_debug_subscribers && !has_event_update {
            self.pending_events.clear();
            self.pending_debug_documents.clear();
            return Ok(());
        }

        for (client_id, session) in clients.iter() {
            if has_state_update {
                let msg = ServerMessage::StateDelta {
                    tick: self.tick,
                    acknowledged_action_tick: session.last_processed_action_tick,
                    authoritative_digest,
                    is_full_snapshot,
                    delta: delta.clone(),
                };
                if let Some(tx) = self.client_tx.read().await.get(client_id) {
                    if let Err(e) = tx.send(msg).await {
                        error!("Failed to send update to client {}: {}", client_id.0, e);
                    }
                }
            }

            if has_event_update {
                let msg = ServerMessage::EventBatch {
                    tick: self.tick,
                    events: self.pending_events.clone(),
                };
                if let Some(tx) = self.client_tx.read().await.get(client_id) {
                    if let Err(e) = tx.send(msg).await {
                        error!("Failed to send event batch to client {}: {}", client_id.0, e);
                    }
                }
            }

            if session.debug_telemetry_enabled {
                if let Some(tx) = self.client_tx.read().await.get(client_id) {
                    for document in &self.pending_debug_documents {
                        if let Err(e) = tx
                            .send(ServerMessage::DebugDocument {
                                document: document.clone(),
                            })
                            .await
                        {
                            error!(
                                "Failed to send debug document to client {}: {}",
                                client_id.0, e
                            );
                            break;
                        }
                    }
                }
            }
        }

        if has_state_update && is_full_snapshot {
            self.last_snapshot_tick = self.tick;
        }
        if has_state_update {
            self.last_snapshot = Some(current_snapshot);
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

    async fn send_to_client(&self, client_id: ClientId, message: ServerMessage) {
        if let Some(tx) = self.client_tx.read().await.get(&client_id) {
            if let Err(err) = tx.send(message).await {
                warn!("Failed to send message to {}: {}", client_id.0, err);
            }
        }
    }

    fn build_full_snapshot_message(&self, acknowledged_action_tick: Option<u64>) -> ServerMessage {
        let snapshot = WorldSnapshot::capture(&self.world);
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
            },
        }
    }

    async fn send_full_snapshot_to_client(&self, client_id: ClientId) {
        let acknowledged_action_tick = self
            .clients
            .read()
            .await
            .get(&client_id)
            .and_then(|session| session.last_processed_action_tick);
        self.send_to_client(
            client_id,
            self.build_full_snapshot_message(acknowledged_action_tick),
        )
        .await;
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
                        session.pending_actions.clear();
                    } else if session.player_name.is_none() {
                        session.player_name = Some(player_name.clone());
                    }
                }

                previous_client_id
            };

            if let Some(previous_client_id) = previous_client_id {
                self.client_tx.write().await.remove(&previous_client_id);
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
            let snapshot = WorldSnapshot::capture(&self.world);
            let authoritative_digest = snapshot.digest();
            let controlled_entity = self
                .clients
                .read()
                .await
                .get(&client_id)
                .and_then(|session| session.agent_id)
                .and_then(|agent_id| {
                    self.world
                        .agents
                        .iter()
                        .find(|slot| slot.agent.id() == agent_id)
                        .and_then(|slot| slot.entity_id)
                        .map(|entity| entity.id() as u64)
                });
            self.send_to_client(
                client_id,
                ServerMessage::Welcome {
                    client_id,
                    reconnect_token,
                    tick: self.tick,
                    controlled_entity,
                    authoritative_digest,
                    snapshot,
                },
            )
            .await;
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
            }
        }

        let snapshot = WorldSnapshot::capture(&self.world);
        let authoritative_digest = snapshot.digest();
        self.send_to_client(
            client_id,
            ServerMessage::Welcome {
                client_id,
                reconnect_token,
                tick: self.tick,
                controlled_entity: Some(entity.id() as u64),
                authoritative_digest,
                snapshot,
            },
        )
        .await;

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
        use pod_core::decode_toon_value;
        use tokio_tungstenite::{connect_async, tungstenite::Message};
        use pod_core::action::SpeakVolume;

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

        let message = server.build_full_snapshot_message(Some(12));
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
            },
        );

        server.step_tick().await.unwrap();
        server.broadcast_updates().await.unwrap();

        let mut saw_tick_telemetry = false;
        while let Ok(message) = rx.try_recv() {
            match message {
                ServerMessage::DebugDocument { document } => {
                    saw_tick_telemetry = true;
                    let value =
                        decode_toon_value(&document).expect("tick telemetry TOON should decode");
                    assert_eq!(value["document_type"], "versioned_tick_telemetry");
                }
                _ => {}
            }
        }

        assert!(saw_tick_telemetry);
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
            },
        );
        server
            .world
            .submit_external_action(
                agent_id,
                Action::Speak {
                    message: "authoritative hello".into(),
                    volume: SpeakVolume::Normal,
                },
            );

        server.step_tick().await.unwrap();
        assert!(server.pending_events.iter().any(|event| matches!(
            event.event,
            pod_core::Event::AgentSpoke { .. }
        )));

        server.broadcast_updates().await.unwrap();

        let mut saw_spoken_event = false;
        while let Ok(message) = rx.try_recv() {
            if let ServerMessage::EventBatch { events, .. } = message {
                saw_spoken_event = events.iter().any(|event| matches!(
                    event.event,
                    pod_core::Event::AgentSpoke { .. }
                ));
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
