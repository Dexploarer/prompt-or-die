//! # pod-net — Networking Layer for Prompt or Die
//!
//! Server-authoritative networking system using QUIC (native) and WebSocket (web).
//!
//! ## Architecture
//!
//! - **Server** (desktop/cloud only): Owns the authoritative world state.
//!   Accepts client connections, validates actions, broadcasts state deltas.
//!
//! - **NativeClient** (desktop): Connects via QUIC (UDP-based, low-latency).
//!   Performs client-side prediction for responsive input.
//!
//! - **WebClient** (browser): Connects via WebSocket (TCP-based, broad compatibility).
//!   Same interface as NativeClient for easy integration.
//!
//! ## Message Flow
//!
//! 1. Client connects, sends `ClientMessage::Connect`
//! 2. Server validates, sends `ServerMessage::Welcome` with initial snapshot
//! 3. Each tick:
//!    - Client sends `ClientMessage::ActionBatch`
//!    - Server applies actions, steps world
//!    - Server broadcasts `ServerMessage::StateDelta` or snapshot
//!    - Client applies delta to local prediction world
//! 4. Client disconnects via `ClientMessage::Disconnect`
//!
//! ## Serialization
//!
//! - **Native (QUIC)**: bincode (fast binary format)
//! - **Web (WebSocket)**: JSON (for JavaScript interop)
//!
//! ## Configuration
//!
//! Server configuration (pod-net::protocol::ServerConfig):
//! ```rust,no_run
//! # use pod_net::protocol::ServerConfig;
//! let config = ServerConfig {
//!     max_clients: 64,
//!     tick_rate: 60,
//!     snapshot_interval: 10,
//!     bind_addr: "0.0.0.0".to_string(),
//!     bind_port: 5000,
//!     enable_websocket: true,
//!     websocket_port: 5001,
//! };
//! ```

pub mod protocol;
pub mod snapshot;

#[cfg(not(target_arch = "wasm32"))]
pub mod server;

#[cfg(not(target_arch = "wasm32"))]
pub mod client_native;

#[cfg(target_arch = "wasm32")]
pub mod client_web;

// Re-export common types
pub use protocol::{ClientConfig, ClientId, ClientMessage, ServerConfig, ServerMessage};
pub use snapshot::{EntitySnapshot, StateDelta, WorldSnapshot};

#[cfg(not(target_arch = "wasm32"))]
pub use server::{GameServer, ServerError};

#[cfg(not(target_arch = "wasm32"))]
pub use client_native::{ClientError, NativeClient};

#[cfg(target_arch = "wasm32")]
pub use client_web::{ClientError, WebClient};

pub fn init() {
    log::info!("{} initialized", env!("CARGO_PKG_NAME"));
}
