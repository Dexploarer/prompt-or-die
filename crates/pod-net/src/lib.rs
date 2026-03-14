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
//!     client_inactivity_timeout_ticks: 600,
//!     queue_pressure_warn_depth: 192,
//! };
//! ```

#![allow(clippy::unnecessary_lazy_evaluations)]

pub mod authority;
pub mod protocol;
pub mod snapshot;

#[cfg(not(target_arch = "wasm32"))]
pub mod server;

#[cfg(not(target_arch = "wasm32"))]
pub mod client_native;

#[cfg(target_arch = "wasm32")]
pub mod client_web;

#[cfg(feature = "spacetimedb")]
pub mod client_stdb;

// Re-export common types
pub use authority::{parse_bind_target, DirectConnectTransportConfig, TransportPolicy};
pub use pod_core::{build_authoritative_world, AuthorityWorldConfig, WorldBootstrapPlan};
pub use protocol::{
    ClientConfig, ClientId, ClientMessage, ReconnectToken, ServerConfig, ServerMessage,
};
pub use snapshot::{
    apply_authoritative_update, build_catch_up_diagnostics, build_rollback_preview,
    compose_presentation_snapshot, CatchUpDiagnostics, EntityDrift, EntityInteractionHints,
    EntityKind, EntityMetadataSnapshot, EntitySnapshot, InterpolatedSnapshot, InterpolationConfig,
    PredictedActionBatch, ReconciliationReport, RecoveryRequestState, RenderClock, RollbackPreview,
    SnapshotInterest, SnapshotInterpolationBuffer, SnapshotSampleMode, SnapshotUpdateError,
    StateDelta, WorldSnapshot,
};

#[cfg(not(target_arch = "wasm32"))]
pub use server::{
    run_transport_benchmark_suite, GameServer, OpsDocumentArchiveSnapshot, OpsDocumentStream,
    ServerError, ServerLifecycleCommand, ServerLifecyclePhase, ServerLifecycleState,
    TransportBenchmarkAggregate, TransportBenchmarkCheck, TransportBenchmarkProfile,
    TransportBenchmarkReport, TransportBenchmarkScenarioReport,
};

#[cfg(not(target_arch = "wasm32"))]
pub use client_native::{ClientError, NativeClient};

#[cfg(target_arch = "wasm32")]
pub use client_web::{ClientError, WebClient};

#[cfg(feature = "spacetimedb")]
pub use client_stdb::{
    build_topology_feed_measurements, build_topology_feed_measurements_with_options,
    LiveGeneratedSdkTopologyFeedConfig, SpacetimeDBClient, SpacetimeDBClientConfig,
    StdbClientError, TopologyFeedCheck, TopologyFeedGeneratedRuntimeMode,
    TopologyFeedMeasurementsOptions, TopologyFeedMeasurementsReport, TopologyFeedWorldPathReport,
    TopologyFeedWorldReport,
};

/// Default web runtime client type.
///
/// On web builds with the `spacetimedb` feature enabled, this resolves to
/// `SpacetimeDBClient`; otherwise it falls back to `WebClient`.
#[cfg(all(target_arch = "wasm32", feature = "spacetimedb"))]
pub type WebDefaultClient = SpacetimeDBClient;
#[cfg(all(target_arch = "wasm32", feature = "spacetimedb"))]
pub type WebDefaultClientConfig = SpacetimeDBClientConfig;

#[cfg(all(target_arch = "wasm32", not(feature = "spacetimedb")))]
pub type WebDefaultClient = WebClient;
#[cfg(all(target_arch = "wasm32", not(feature = "spacetimedb")))]
pub type WebDefaultClientConfig = ClientConfig;

pub fn init() {
    log::info!("{} initialized", env!("CARGO_PKG_NAME"));
}
