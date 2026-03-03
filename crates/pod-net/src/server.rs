//! Authoritative game server — QUIC-based, server-owned world state.
//!
//! This module only compiles on non-WASM platforms (desktop/cloud).

#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use log::{debug, error, info, warn};
use quinn::Endpoint;
use tokio::sync::{mpsc, RwLock};

use pod_core::{Action, IdleAgent, World};

use crate::protocol::{
    ClientId, ClientMessage, ServerConfig as ProtoServerConfig, ServerMessage,
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
}

impl ClientSession {
    #[allow(dead_code)]
    fn new(player_name: String) -> Self {
        Self {
            player_name: Some(player_name),
            agent_id: None,
            pending_actions: Vec::new(),
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
    /// Current tick counter
    tick: u64,
    /// Last tick when snapshot was sent
    last_snapshot_tick: u64,
    /// Last full snapshot for delta computation.
    last_snapshot: Option<WorldSnapshot>,
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
            tick: 0,
            last_snapshot_tick: 0,
            last_snapshot: None,
        }
    }

    /// Initialize the server (bind to network)
    pub async fn initialize(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let addr: SocketAddr = format!("{}:{}", self.config.bind_addr, self.config.bind_port)
            .parse()?;

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

        info!(
            "GameServer initialized on {}:{}",
            self.config.bind_addr, self.config.bind_port
        );

        Ok(())
    }

    /// Main server loop — step the world and handle client connections
    pub async fn run(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let tick_duration =
            std::time::Duration::from_secs_f32(1.0 / self.config.tick_rate as f32);

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

                for (action_tick, action) in session.pending_actions.drain(..) {
                    let min_tick = self.tick.saturating_sub(ACTION_WINDOW_BACKWARD_TICKS);
                    let max_tick = self.tick + ACTION_WINDOW_FORWARD_TICKS;
                    if action_tick < min_tick || action_tick > max_tick {
                        continue;
                    }
                    self.world.submit_external_action(agent_id, action);
                }
            }
        }

        // Step the world
        self.world.step();

        debug!("Tick {}: {} entities, {} agents", self.tick,
               self.world.entity_count(), self.world.agent_count());

        Ok(())
    }

    /// Accept incoming client connections
    async fn handle_connections(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(endpoint) = &self.endpoint {
            loop {
                let incoming = match tokio::time::timeout(Duration::from_millis(1), endpoint.accept()).await {
                    Ok(Some(incoming)) => incoming,
                    Ok(None) => break,
                    Err(_) => break,
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

                let (outbound_tx, mut outbound_rx) = mpsc::channel::<ServerMessage>(256);
                self.client_tx.write().await.insert(client_id, outbound_tx);
                self.clients
                    .write()
                    .await
                    .insert(client_id, ClientSession::new("pending".into()));

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
                InboundPacket::Message { client_id, message } => {
                    match message {
                        ClientMessage::Connect { player_name } => {
                            self.attach_remote_agent(client_id, player_name).await?;
                        }
                        ClientMessage::ActionBatch { tick, actions } => {
                            let mut overflow = false;
                            let mut unregistered = false;
                            {
                                let mut clients = self.clients.write().await;
                                if let Some(session) = clients.get_mut(&client_id) {
                                    if session.agent_id.is_none() {
                                        unregistered = true;
                                    }
                                    let available = ACTION_QUEUE_MAX_DEPTH
                                        .saturating_sub(session.pending_actions.len());
                                    if actions.len() > available {
                                        overflow = true;
                                    }
                                    if !unregistered {
                                        for action in actions.into_iter().take(available) {
                                            session.pending_actions.push((tick, action));
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
        // Decide whether to send full snapshot or delta
        let should_snapshot = (self.tick - self.last_snapshot_tick)
            >= self.config.snapshot_interval;
        let current_snapshot = WorldSnapshot::capture(&self.world);

        let delta = if should_snapshot || self.last_snapshot.is_none() {
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

        if !should_snapshot && delta.change_count() == 0 {
            return Ok(());
        }

        let clients = self.clients.read().await;

        for client_id in clients.keys() {
            let msg = ServerMessage::StateDelta {
                tick: self.tick,
                delta: delta.clone(),
            };
            if let Some(tx) = self.client_tx.read().await.get(client_id) {
                if let Err(e) = tx.send(msg).await {
                    error!("Failed to send update to client {}: {}", client_id.0, e);
                }
            }
        }

        if should_snapshot {
            self.last_snapshot_tick = self.tick;
        }
        self.last_snapshot = Some(current_snapshot);

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

    async fn attach_remote_agent(
        &mut self,
        client_id: ClientId,
        player_name: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let already_attached = self
            .clients
            .read()
            .await
            .get(&client_id)
            .and_then(|session| session.agent_id)
            .is_some();

        if already_attached {
            let snapshot = WorldSnapshot::capture(&self.world);
            self.send_to_client(
                client_id,
                ServerMessage::Welcome {
                    client_id,
                    tick: self.tick,
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
            }
        }

        let snapshot = WorldSnapshot::capture(&self.world);
        self.send_to_client(
            client_id,
            ServerMessage::Welcome {
                client_id,
                tick: self.tick,
                snapshot,
            },
        )
        .await;

        Ok(())
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

    #[tokio::test]
    async fn test_server_creation() {
        let config = ProtoServerConfig::default();
        let world = World::new(42);
        let server = GameServer::new(config, world);

        assert_eq!(server.current_tick(), 0);
        assert_eq!(server.client_count().await, 0);
    }
}
