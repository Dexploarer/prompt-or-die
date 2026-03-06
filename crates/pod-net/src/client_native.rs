//! Native QUIC client — for desktop platforms.
//!
//! Uses quinn for QUIC protocol (UDP-based, faster than TCP).
//! This module only compiles on non-WASM platforms.

#![cfg(not(target_arch = "wasm32"))]

use std::sync::Arc;

use log::{debug, error, info};
use quinn::Endpoint;
use tokio::sync::mpsc;

use pod_core::Action;

use crate::protocol::{
    ClientConfig as ProtoClientConfig, ClientId, ClientMessage, ReconnectToken, ServerMessage,
};
use crate::snapshot::WorldSnapshot;

// ============================================================
// NATIVE CLIENT
// ============================================================

/// QUIC-based client for native platforms
pub struct NativeClient {
    config: ProtoClientConfig,
    client_id: Option<ClientId>,
    connection: Option<quinn::Connection>,
    endpoint: Endpoint,
    /// Channel for receiving server messages
    rx: mpsc::Receiver<ServerMessage>,
    /// Channel for sending server messages (internal)
    tx: mpsc::Sender<ServerMessage>,
    reader_task: Option<tokio::task::JoinHandle<()>>,
    /// Buffered server updates
    pending_updates: Vec<ServerMessage>,
    /// Local prediction world state
    local_snapshot: Option<WorldSnapshot>,
    /// Actions pending transmission
    pending_actions: Vec<Action>,
    /// Highest server tick applied locally.
    last_server_tick: u64,
    /// Highest event tick accepted from server.
    last_event_tick: u64,
    /// Reconnect token issued by server on first successful connect.
    reconnect_token: Option<ReconnectToken>,
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
                let cert_der = std::fs::read(&path)
                    .map_err(|e| ClientError::Config(format!("Failed reading POD_TRUSTED_CERT_DER: {e}")))?;
                roots
                    .add(rustls::pki_types::CertificateDer::from(cert_der))
                    .map_err(|e| ClientError::Config(format!("Invalid trusted DER certificate: {e}")))?;
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

        let mut client = Self {
            config,
            client_id: None,
            connection: Some(connection),
            endpoint,
            rx,
            tx,
            reader_task: None,
            pending_updates: Vec::new(),
            local_snapshot: None,
            pending_actions: Vec::new(),
            last_server_tick: 0,
            last_event_tick: 0,
            reconnect_token: None,
        };

        // Send connect message
        client.send_connect().await?;
        client.start_reader_loop();

        Ok(client)
    }

    /// Send a connect message to the server
    async fn send_connect(&mut self) -> Result<(), ClientError> {
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
                snapshot,
                ..
            } = msg {
                self.client_id = Some(client_id);
                self.local_snapshot = Some(snapshot);
                self.reconnect_token = Some(reconnect_token);
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
        self.reader_task = Some(tokio::spawn(async move {
            loop {
                let mut recv = match connection.accept_uni().await {
                    Ok(recv) => recv,
                    Err(err) => {
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

        let bytes = msg.encode()
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

        let msg = ServerMessage::decode(&buf)
            .map_err(|e| ClientError::Serialization(e.to_string()))?;

        Ok(msg)
    }

    /// Queue an action to be sent to the server
    pub fn queue_action(&mut self, action: Action) {
        self.pending_actions.push(action);
    }

    /// Send queued actions in a batch
    pub async fn send_actions(&mut self, tick: u64) -> Result<(), ClientError> {
        if self.pending_actions.is_empty() {
            return Ok(());
        }

        let msg = ClientMessage::ActionBatch {
            tick,
            actions: std::mem::take(&mut self.pending_actions),
        };

        self.send_message(msg).await
    }

    /// Check for updates from the server (non-blocking)
    pub fn poll_updates(&mut self) -> Vec<ServerMessage> {
        while let Ok(msg) = self.rx.try_recv() {
            if self.apply_server_message(&msg) {
                self.pending_updates.push(msg);
            }
        }

        std::mem::take(&mut self.pending_updates)
    }

    fn apply_server_message(&mut self, message: &ServerMessage) -> bool {
        match message {
            ServerMessage::Welcome {
                client_id,
                reconnect_token,
                tick,
                snapshot,
            } => {
                self.client_id = Some(*client_id);
                self.local_snapshot = Some(snapshot.clone());
                self.last_server_tick = *tick;
                self.last_event_tick = *tick;
                self.reconnect_token = Some(*reconnect_token);
                true
            }
            ServerMessage::StateDelta { tick, delta } => {
                if *tick < self.last_server_tick {
                    return false;
                }

                if self.last_server_tick > 0 && *tick > self.last_server_tick + 1 {
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
            ServerMessage::EventBatch { tick, .. } => {
                if *tick < self.last_event_tick {
                    return false;
                }
                self.last_event_tick = *tick;
                true
            }
            ServerMessage::Pong { .. } => true,
            ServerMessage::Rejected { reason } => {
                error!("Server rejected request: {}", reason);
                true
            }
        }
    }

    /// Send a ping to measure latency
    pub async fn ping(&mut self) -> Result<(), ClientError> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        self.send_message(ClientMessage::Ping { timestamp }).await
    }

    /// Disconnect from the server
    pub async fn disconnect(&mut self, reason: Option<&str>) -> Result<(), ClientError> {
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

        info!("Disconnected from server");
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
        self.connection.is_some() && self.client_id.is_some()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_creation() {
        let config = ProtoClientConfig {
            server_addr: "localhost".to_string(),
            server_port: 5000,
            player_name: "TestPlayer".to_string(),
            timeout_ms: 5000,
        };

        // Note: actual connection would require a running server
        // This just tests configuration
        assert_eq!(config.player_name, "TestPlayer");
        assert_eq!(config.server_addr, "localhost");
    }
}
