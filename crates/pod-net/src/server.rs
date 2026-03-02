//! Authoritative game server — QUIC-based, server-owned world state.
//!
//! This module only compiles on non-WASM platforms (desktop/cloud).

#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use log::{debug, error, info, warn};
use quinn::Endpoint;
use tokio::sync::{mpsc, RwLock};

use pod_core::{Action, World};

use crate::protocol::{ClientId, ServerConfig as ProtoServerConfig, ServerMessage};
use crate::snapshot::{StateDelta, WorldSnapshot};

// ============================================================
// CLIENT SESSION STATE
// ============================================================

/// Tracks a connected client and their session
struct ClientSession {
    _id: ClientId,
    _player_name: String,
    _tick_joined: u64,
    _last_snapshot_tick: u64,
    pending_actions: Vec<(u64, Action)>, // (tick, action)
}

impl ClientSession {
    #[allow(dead_code)]
    fn new(id: ClientId, player_name: String, tick: u64) -> Self {
        Self {
            _id: id,
            _player_name: player_name,
            _tick_joined: tick,
            _last_snapshot_tick: tick,
            pending_actions: Vec::new(),
        }
    }
}

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
    /// QUIC endpoint
    endpoint: Option<Endpoint>,
    /// Current tick counter
    tick: u64,
    /// Last tick when snapshot was sent
    last_snapshot_tick: u64,
}

impl GameServer {
    /// Create a new game server
    pub fn new(config: ProtoServerConfig, world: World) -> Self {
        Self {
            config,
            world,
            clients: Arc::new(RwLock::new(HashMap::new())),
            client_tx: Arc::new(RwLock::new(HashMap::new())),
            endpoint: None,
            tick: 0,
            last_snapshot_tick: 0,
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
        // Collect pending actions from all clients
        let mut all_actions = Vec::new();

        {
            let clients = self.clients.read().await;
            for session in clients.values() {
                for (_, action) in &session.pending_actions {
                    all_actions.push(action.clone());
                }
            }
        }

        // Apply actions to world
        // TODO: Validate actions against constraints
        // for action in all_actions {
        //     // apply_action(&mut self.world, action);
        // }

        // Clear pending actions
        {
            let mut clients = self.clients.write().await;
            for session in clients.values_mut() {
                session.pending_actions.clear();
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
        // For now, we'll implement this with proper QUIC handling
        // This is a placeholder for the actual connection handling
        Ok(())
    }

    /// Broadcast world updates to all connected clients
    async fn broadcast_updates(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Decide whether to send full snapshot or delta
        let should_snapshot = (self.tick - self.last_snapshot_tick)
            >= self.config.snapshot_interval;

        let clients = self.clients.read().await;

        for client_id in clients.keys() {
            let msg = if should_snapshot {
                let snapshot = WorldSnapshot::capture(&self.world);
                ServerMessage::StateDelta {
                    tick: self.tick,
                    delta: StateDelta {
                        tick: self.tick,
                        updated: snapshot.entities,
                        destroyed: vec![],
                    },
                }
            } else {
                // Send delta (requires previous snapshot state)
                // For now, simplified
                ServerMessage::StateDelta {
                    tick: self.tick,
                    delta: StateDelta::default(),
                }
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
        self.clients.write().await.remove(&client_id);
        self.client_tx.write().await.remove(&client_id);
        info!("Client {} disconnected: {}", client_id.0, reason);
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
