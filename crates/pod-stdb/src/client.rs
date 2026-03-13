//! SpacetimeDB client wrapper — connection, subscription, and reducer-call abstractions.
//!
//! This module provides a high-level Rust client for connecting to the pod-stdb
//! SpacetimeDB module. It wraps the SpacetimeDB client SDK with typed helpers
//! specific to the Prompt or Die game tables and reducers.
//!
//! # Architecture
//!
//! ```text
//!  ┌──────────────────────────────┐
//!  │        Game Loop             │
//!  │  client.frame_tick()         │
//!  │  for ev in drain_events()   │
//!  └──────────┬───────────────────┘
//!             │
//!  ┌──────────▼───────────────────┐
//!  │       StdbClient             │
//!  │  - Connection lifecycle      │
//!  │  - Subscription management   │
//!  │  - Typed reducer calls       │
//!  │  - Entity/world state cache  │
//!  │  - Event queue               │
//!  └──────────┬───────────────────┘
//!             │
//!  ┌──────────▼───────────────────┐
//!  │   SpacetimeDB SDK            │
//!  │   (DbConnection + bindings)  │
//!  └──────────────────────────────┘
//! ```
//!
//! The client maintains a local cache of subscribed table rows, mirroring
//! SpacetimeDB's client-side cache. Updates arrive via subscription callbacks
//! and are buffered as [`StdbEvent`]s for the game loop to consume via
//! [`StdbClient::drain_events`].
//!
//! # Usage
//!
//! ```rust,ignore
//! use pod_stdb::client::*;
//!
//! let config = StdbClientConfig {
//!     host: "http://localhost:3000".into(),
//!     db_name: "prompt-or-die".into(),
//!     auth_token: None,
//! };
//!
//! let mut client = StdbClient::new(config);
//! client.connect()?;
//!
//! // In game loop:
//! client.frame_tick();
//! for event in client.drain_events() {
//!     match event {
//!         StdbEvent::Connected { .. } => { /* subscribe to tables */ },
//!         StdbEvent::ObservationReceived { .. } => { /* feed to agent */ },
//!         StdbEvent::TickAdvanced { .. } => { /* sync state */ },
//!         _ => {}
//!     }
//! }
//! ```
//!
//! # Feature Gate
//!
//! This module is only available with the `client` feature enabled.
//! It is NOT compiled into the SpacetimeDB WASM module (cdylib target).
//!
//! # Generated Bindings
//!
//! Full SpacetimeDB SDK integration requires generated client bindings:
//! ```bash
//! cargo build -p pod-stdb --target wasm32-unknown-unknown --release --no-default-features --features module
//! spacetime generate --lang rust --out-dir src/module_bindings --bin-path .cargo-target/wasm32-unknown-unknown/release/pod_stdb.wasm
//! ```
//! Until bindings are generated, this module provides the typed API surface
//! with stub implementations. See [`StdbClient::connect`] for details.

use crate::types::*;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use pod_core::{
    decode_toon_document, decode_toon_value, AgentTickRollup, AgentToolCallEvent,
    AppliedWorldStateSummary, FocusedEntityDebugSummary, RemoteTopologyBundle, ToolCallStatus,
    VersionedTickTelemetry, WorldEvaluationSummary, WorldQuestBinding, WorldRealityDefinition,
};
use spacetimedb_sdk::{
    DbContext as _, SubscriptionHandle as _, Table as _, TableWithPrimaryKey as _,
};

use crate::module_bindings::{
    self, remote_topology_document_table::RemoteTopologyDocumentTableAccess as _,
    DbConnection as GeneratedSdkDbConnection,
    RemoteTopologyDocumentRow as GeneratedSdkRemoteTopologyDocumentRow,
    SubscriptionHandle as GeneratedSdkSubscriptionHandle,
};

// ============================================================
// CONFIGURATION
// ============================================================

/// Configuration for connecting to the SpacetimeDB module.
#[derive(Debug, Clone)]
pub struct StdbClientConfig {
    /// SpacetimeDB host URI (e.g., "http://localhost:3000")
    pub host: String,
    /// Database name or Identity (e.g., "prompt-or-die")
    pub db_name: String,
    /// Authentication token from a previous session.
    /// If None, SpacetimeDB generates a new Identity + token on connect.
    pub auth_token: Option<String>,
    /// Player display name for connect_agent reducer
    pub player_name: String,
    /// Connection mode for the client runtime.
    pub connection_mode: StdbConnectionMode,
}

impl Default for StdbClientConfig {
    fn default() -> Self {
        Self {
            host: "http://localhost:3000".into(),
            db_name: "prompt-or-die".into(),
            auth_token: None,
            player_name: "Player".into(),
            connection_mode: StdbConnectionMode::default(),
        }
    }
}

/// Runtime connection mode for the SpacetimeDB client.
///
/// `Generated` is the production mode that requires generated bindings +
/// `DbConnection` runtime wiring. `Emulated` keeps the local deterministic
/// reducer/cache simulation path for offline development and tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StdbConnectionMode {
    /// Production path: generated bindings + real SpacetimeDB transport.
    Generated,
    /// Local fallback path: in-process emulation (no transport).
    Emulated,
}

#[allow(clippy::derivable_impls)]
impl Default for StdbConnectionMode {
    fn default() -> Self {
        #[cfg(debug_assertions)]
        {
            Self::Emulated
        }
        #[cfg(not(debug_assertions))]
        {
            Self::Generated
        }
    }
}

/// Events emitted by a generated SpacetimeDB runtime adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratedRuntimeEvent {
    /// Transport connected and obtained identity + auth token.
    Connected { identity: Vec<u8>, token: String },
    /// Transport connection failed.
    ConnectError { message: String },
    /// Transport disconnected after a prior connection.
    Disconnected { reason: String },
    /// Active SQL subscriptions have been acknowledged/applied.
    SubscriptionApplied,
    /// Authority-published remote topology row delivered through the generated feed.
    RemoteTopologyDocumentRow(GeneratedRemoteTopologyDocumentRow),
}

/// Client-side mirror of the authority-published `remote_topology_document` row.
///
/// This matches the generated callback payload shape more closely than passing
/// loosely related scalar fields around benchmark/test helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedRemoteTopologyDocumentRow {
    pub row_id: u64,
    pub generated_at_unix_ms: u64,
    pub scenario_id: String,
    pub profile_id: String,
    pub world_count: u32,
    pub team_count: u32,
    pub topology_json: String,
}

impl GeneratedRemoteTopologyDocumentRow {
    /// Build a callback row from the shared topology artifact.
    pub fn from_topology_bundle(
        row_id: u64,
        topology: &RemoteTopologyBundle,
    ) -> Result<Self, String> {
        let generated_at_unix_ms = u64::try_from(topology.generated_at_unix_ms)
            .map_err(|_| "topology timestamp exceeds u64".to_string())?;
        let world_count = u32::try_from(topology.worlds.len())
            .map_err(|_| "topology world_count exceeds u32".to_string())?;
        let team_count = u32::try_from(topology.teams.len())
            .map_err(|_| "topology team_count exceeds u32".to_string())?;
        Ok(Self {
            row_id,
            generated_at_unix_ms,
            scenario_id: topology.scenario_id.clone(),
            profile_id: topology.profile_id.clone(),
            world_count,
            team_count,
            topology_json: topology.to_toon_document(),
        })
    }
}

/// Thin runtime seam for generated SpacetimeDB bindings.
///
/// The adapter owns transport-specific state and exposes only the events POD
/// currently needs from generated mode. This keeps the real SDK integration
/// behind a minimal boundary while allowing tests to drive the same path.
pub trait GeneratedRuntimeAdapter {
    /// Begin connecting with the provided client config.
    fn connect(&mut self, config: &StdbClientConfig) -> Result<(), String>;
    /// Tear down any live connection and pending transport state.
    fn disconnect(&mut self);
    /// Apply the active SQL subscription set.
    fn subscribe(&mut self, queries: &[String]) -> Result<(), String>;
    /// Drain any transport/runtime events accumulated since the last frame.
    fn drain_events(&mut self) -> Vec<GeneratedRuntimeEvent>;
}

/// Outbound command issued by [`StdbClient`] to a generated SpacetimeDB binding layer.
#[derive(Debug, Clone)]
pub enum GeneratedBindingCommand {
    /// Begin connecting with the provided client config.
    Connect { config: StdbClientConfig },
    /// Apply the active SQL subscription set.
    Subscribe { queries: Vec<String> },
    /// Tear down the active generated connection.
    Disconnect,
}

/// Thread-safe event handle for a generated SpacetimeDB runtime.
///
/// Real generated bindings should keep a clone of this handle inside their SDK
/// callbacks and push runtime events into it as transport updates arrive.
#[derive(Debug, Clone, Default)]
pub struct GeneratedRuntimeHandle {
    events: Arc<Mutex<VecDeque<GeneratedRuntimeEvent>>>,
}

impl GeneratedRuntimeHandle {
    fn push(&self, event: GeneratedRuntimeEvent) {
        self.events
            .lock()
            .expect("generated runtime event queue poisoned")
            .push_back(event);
    }

    fn drain(&self) -> Vec<GeneratedRuntimeEvent> {
        self.events
            .lock()
            .expect("generated runtime event queue poisoned")
            .drain(..)
            .collect()
    }

    /// Emit a successful transport connection event.
    pub fn connected(&self, identity: Vec<u8>, token: String) {
        self.push(GeneratedRuntimeEvent::Connected { identity, token });
    }

    /// Emit a connection failure event.
    pub fn connect_error(&self, message: impl Into<String>) {
        self.push(GeneratedRuntimeEvent::ConnectError {
            message: message.into(),
        });
    }

    /// Emit a disconnection event.
    pub fn disconnected(&self, reason: impl Into<String>) {
        self.push(GeneratedRuntimeEvent::Disconnected {
            reason: reason.into(),
        });
    }

    /// Emit a subscription-applied event after the generated client confirms queries.
    pub fn subscription_applied(&self) {
        self.push(GeneratedRuntimeEvent::SubscriptionApplied);
    }

    /// Emit an authority-fed remote-topology row from the generated client transport.
    pub fn remote_topology_document_row(
        &self,
        row_id: u64,
        generated_at_unix_ms: u64,
        scenario_id: impl Into<String>,
        profile_id: impl Into<String>,
        topology_json: impl Into<String>,
    ) {
        self.push(GeneratedRuntimeEvent::RemoteTopologyDocumentRow(
            GeneratedRemoteTopologyDocumentRow {
                row_id,
                generated_at_unix_ms,
                scenario_id: scenario_id.into(),
                profile_id: profile_id.into(),
                world_count: 0,
                team_count: 0,
                topology_json: topology_json.into(),
            },
        ));
    }

    /// Emit an authority-fed remote-topology row using the generated binding row shape.
    pub fn remote_topology_document_insert(&self, row: GeneratedRemoteTopologyDocumentRow) {
        self.push(GeneratedRuntimeEvent::RemoteTopologyDocumentRow(row));
    }
}

/// Typed callback surface for live generated SpacetimeDB bindings.
///
/// This is the object a real generated `DbConnection` callback layer should
/// clone into its connect, disconnect, subscribe, and row-insert handlers.
#[derive(Debug, Clone, Default)]
pub struct GeneratedBindingCallbacks {
    handle: GeneratedRuntimeHandle,
}

impl GeneratedBindingCallbacks {
    fn new(handle: GeneratedRuntimeHandle) -> Self {
        Self { handle }
    }

    /// Forward a successful generated-runtime connect callback.
    pub fn connected(&self, identity: Vec<u8>, token: impl Into<String>) {
        self.handle.connected(identity, token.into());
    }

    /// Forward a generated-runtime connect failure callback.
    pub fn connect_error(&self, message: impl Into<String>) {
        self.handle.connect_error(message);
    }

    /// Forward a generated-runtime disconnect callback.
    pub fn disconnected(&self, reason: impl Into<String>) {
        self.handle.disconnected(reason);
    }

    /// Forward a generated-runtime subscription-applied callback.
    pub fn subscription_applied(&self) {
        self.handle.subscription_applied();
    }

    /// Forward an authority-published topology row insert callback.
    pub fn remote_topology_document_insert(&self, row: GeneratedRemoteTopologyDocumentRow) {
        self.handle.remote_topology_document_insert(row);
    }
}

/// External control surface for a command-driven generated runtime.
///
/// A real generated SpacetimeDB binding layer can drain outgoing commands from
/// this endpoint and drive inbound runtime callbacks through the paired
/// [`GeneratedBindingCallbacks`] object.
#[derive(Debug, Clone, Default)]
pub struct GeneratedBindingEndpoint {
    commands: Arc<Mutex<VecDeque<GeneratedBindingCommand>>>,
    callbacks: GeneratedBindingCallbacks,
}

impl GeneratedBindingEndpoint {
    fn new(
        commands: Arc<Mutex<VecDeque<GeneratedBindingCommand>>>,
        callbacks: GeneratedBindingCallbacks,
    ) -> Self {
        Self {
            commands,
            callbacks,
        }
    }

    /// Drain all pending client-to-binding commands.
    pub fn drain_commands(&self) -> Vec<GeneratedBindingCommand> {
        self.commands
            .lock()
            .expect("generated binding command queue poisoned")
            .drain(..)
            .collect()
    }

    /// Clone the callback surface used by the generated binding layer.
    pub fn callbacks(&self) -> GeneratedBindingCallbacks {
        self.callbacks.clone()
    }
}

/// Command-driven runtime adapter for real generated SpacetimeDB bindings.
///
/// Unlike [`GeneratedRuntimeBridge`], this adapter does not synthesize connect
/// or subscription acknowledgements on its own. It only records outbound client
/// commands, leaving an external generated binding layer to decide when to emit
/// the corresponding callbacks through [`GeneratedBindingCallbacks`].
#[derive(Debug, Default)]
pub struct GeneratedBindingRuntime {
    commands: Arc<Mutex<VecDeque<GeneratedBindingCommand>>>,
    handle: GeneratedRuntimeHandle,
}

impl GeneratedBindingRuntime {
    /// Create a new command-driven generated runtime plus its external endpoint.
    pub fn new() -> (Self, GeneratedBindingEndpoint) {
        let commands = Arc::new(Mutex::new(VecDeque::new()));
        let handle = GeneratedRuntimeHandle::default();
        let callbacks = GeneratedBindingCallbacks::new(handle.clone());
        let endpoint = GeneratedBindingEndpoint::new(commands.clone(), callbacks);
        (Self { commands, handle }, endpoint)
    }

    fn push_command(&self, command: GeneratedBindingCommand) {
        self.commands
            .lock()
            .expect("generated binding command queue poisoned")
            .push_back(command);
    }
}

impl GeneratedRuntimeAdapter for GeneratedBindingRuntime {
    fn connect(&mut self, config: &StdbClientConfig) -> Result<(), String> {
        self.push_command(GeneratedBindingCommand::Connect {
            config: config.clone(),
        });
        Ok(())
    }

    fn disconnect(&mut self) {
        self.push_command(GeneratedBindingCommand::Disconnect);
    }

    fn subscribe(&mut self, queries: &[String]) -> Result<(), String> {
        self.push_command(GeneratedBindingCommand::Subscribe {
            queries: queries.to_vec(),
        });
        Ok(())
    }

    fn drain_events(&mut self) -> Vec<GeneratedRuntimeEvent> {
        self.handle.drain()
    }
}

/// Real generated-runtime adapter backed by SpacetimeDB's generated Rust bindings.
///
/// This uses the generated `DbConnection` plus typed table callbacks instead of
/// the command-queue simulation used by [`GeneratedBindingRuntime`].
pub struct GeneratedSdkRuntime {
    handle: GeneratedRuntimeHandle,
    connection: Option<GeneratedSdkDbConnection>,
    subscription: Option<GeneratedSdkSubscriptionHandle>,
}

impl GeneratedSdkRuntime {
    /// Create a real generated-runtime adapter backed by the installed SDK bindings.
    pub fn new() -> Self {
        Self {
            handle: GeneratedRuntimeHandle::default(),
            connection: None,
            subscription: None,
        }
    }

    fn push_remote_topology_row(
        handle: &GeneratedRuntimeHandle,
        row: &GeneratedSdkRemoteTopologyDocumentRow,
    ) {
        handle.remote_topology_document_insert(GeneratedRemoteTopologyDocumentRow {
            row_id: row.row_id,
            generated_at_unix_ms: row.generated_at_unix_ms,
            scenario_id: row.scenario_id.clone(),
            profile_id: row.profile_id.clone(),
            world_count: row.world_count,
            team_count: row.team_count,
            topology_json: row.topology_json.clone(),
        });
    }

    fn register_topology_callbacks(
        connection: &GeneratedSdkDbConnection,
        handle: &GeneratedRuntimeHandle,
    ) {
        let table = connection.db.remote_topology_document();

        let insert_handle = handle.clone();
        table.on_insert(move |_ctx, row| {
            Self::push_remote_topology_row(&insert_handle, row);
        });

        let update_handle = handle.clone();
        table.on_update(move |_ctx, _old, new| {
            Self::push_remote_topology_row(&update_handle, new);
        });
    }
}

impl Default for GeneratedSdkRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl GeneratedRuntimeAdapter for GeneratedSdkRuntime {
    fn connect(&mut self, config: &StdbClientConfig) -> Result<(), String> {
        if self.connection.is_some() {
            return Err("generated SDK runtime already has an active connection".into());
        }

        let on_connect_handle = self.handle.clone();
        let on_error_handle = self.handle.clone();
        let on_disconnect_handle = self.handle.clone();
        let connection = module_bindings::DbConnection::builder()
            .with_uri(config.host.clone())
            .with_database_name(config.db_name.clone())
            .with_token(config.auth_token.clone())
            .on_connect(move |connection, identity, token| {
                Self::register_topology_callbacks(connection, &on_connect_handle);
                on_connect_handle.connected(identity.to_byte_array().to_vec(), token.to_string());
            })
            .on_connect_error(move |_ctx, error| {
                on_error_handle.connect_error(error.to_string());
            })
            .on_disconnect(move |_ctx, error| {
                let reason = error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "generated runtime disconnected".to_string());
                on_disconnect_handle.disconnected(reason);
            })
            .build()
            .map_err(|error| error.to_string())?;

        self.connection = Some(connection);
        Ok(())
    }

    fn disconnect(&mut self) {
        if let Some(subscription) = self.subscription.take() {
            let _ = subscription.unsubscribe();
        }

        if let Some(connection) = self.connection.take() {
            let _ = connection.disconnect();
        }

        let _ = self.handle.drain();
    }

    fn subscribe(&mut self, queries: &[String]) -> Result<(), String> {
        let connection = self
            .connection
            .as_ref()
            .ok_or_else(|| "generated SDK runtime has no active connection".to_string())?;

        if let Some(subscription) = self.subscription.take() {
            let _ = subscription.unsubscribe();
        }

        let applied_handle = self.handle.clone();
        let error_handle = self.handle.clone();
        let subscription = connection
            .subscription_builder()
            .on_applied(move |_ctx| {
                applied_handle.subscription_applied();
            })
            .on_error(move |_ctx, error| {
                error_handle.connect_error(format!("subscription failed: {error}"));
            })
            .subscribe(queries.to_vec());
        self.subscription = Some(subscription);
        Ok(())
    }

    fn drain_events(&mut self) -> Vec<GeneratedRuntimeEvent> {
        let mut disconnected_reason = None;
        if let Some(connection) = self.connection.as_ref() {
            if let Err(error) = connection.frame_tick() {
                disconnected_reason = Some(error.to_string());
            }
        }

        if let Some(reason) = disconnected_reason {
            self.subscription = None;
            self.connection = None;
            self.handle.disconnected(reason);
        }

        self.handle.drain()
    }
}

/// Introspection for the callback-driven generated runtime helper.
#[derive(Debug, Clone, Default)]
pub struct GeneratedRuntimeTrace {
    connect_configs: Arc<Mutex<Vec<StdbClientConfig>>>,
    subscription_queries: Arc<Mutex<Vec<Vec<String>>>>,
    disconnect_count: Arc<Mutex<usize>>,
}

impl GeneratedRuntimeTrace {
    fn record_connect(&self, config: &StdbClientConfig) {
        self.connect_configs
            .lock()
            .expect("generated runtime connect trace poisoned")
            .push(config.clone());
    }

    fn record_subscribe(&self, queries: &[String]) {
        self.subscription_queries
            .lock()
            .expect("generated runtime subscribe trace poisoned")
            .push(queries.to_vec());
    }

    fn record_disconnect(&self) {
        *self
            .disconnect_count
            .lock()
            .expect("generated runtime disconnect trace poisoned") += 1;
    }

    /// All connect configs observed by the helper runtime.
    pub fn connect_configs(&self) -> Vec<StdbClientConfig> {
        self.connect_configs
            .lock()
            .expect("generated runtime connect trace poisoned")
            .clone()
    }

    /// All subscription sets observed by the helper runtime.
    pub fn subscription_queries(&self) -> Vec<Vec<String>> {
        self.subscription_queries
            .lock()
            .expect("generated runtime subscribe trace poisoned")
            .clone()
    }

    /// Number of disconnect callbacks observed by the helper runtime.
    pub fn disconnect_count(&self) -> usize {
        *self
            .disconnect_count
            .lock()
            .expect("generated runtime disconnect trace poisoned")
    }
}

/// Reusable helper that auto-acks connect/subscription callbacks for tests.
///
/// The returned [`GeneratedBindingCallbacks`] are what benchmarks/tests should
/// drive when they want to simulate real callback delivery from generated
/// SpacetimeDB bindings, without re-defining ad hoc bridge closures each time.
///
/// Prefer [`GeneratedBindingRuntime`] for live-like command/callback flows.
pub fn build_generated_runtime_callback_bridge(
    identity: Vec<u8>,
    token: impl Into<String>,
) -> (
    GeneratedRuntimeBridge,
    GeneratedBindingCallbacks,
    GeneratedRuntimeTrace,
) {
    let trace = GeneratedRuntimeTrace::default();
    let trace_for_connect = trace.clone();
    let trace_for_subscribe = trace.clone();
    let trace_for_disconnect = trace.clone();
    let token = token.into();
    let identity_for_connect = identity.clone();
    let (bridge, handle) = GeneratedRuntimeBridge::new(
        move |config, handle| {
            trace_for_connect.record_connect(config);
            handle.connected(identity_for_connect.clone(), token.clone());
            Ok(())
        },
        move |queries, handle| {
            trace_for_subscribe.record_subscribe(queries);
            handle.subscription_applied();
            Ok(())
        },
        move || {
            trace_for_disconnect.record_disconnect();
        },
    );
    (bridge, GeneratedBindingCallbacks::new(handle), trace)
}

type GeneratedConnectHook =
    dyn FnMut(&StdbClientConfig, GeneratedRuntimeHandle) -> Result<(), String>;
type GeneratedSubscribeHook = dyn FnMut(&[String], GeneratedRuntimeHandle) -> Result<(), String>;
type GeneratedDisconnectHook = dyn FnMut();

/// Reusable bridge between `StdbClient` and real generated SpacetimeDB bindings.
///
/// The bridge owns the runtime-event queue and exposes a cloneable
/// [`GeneratedRuntimeHandle`] that SDK callbacks can use to push connection,
/// subscription, and authority-document events back into the client.
pub struct GeneratedRuntimeBridge {
    handle: GeneratedRuntimeHandle,
    on_connect: Box<GeneratedConnectHook>,
    on_subscribe: Box<GeneratedSubscribeHook>,
    on_disconnect: Box<GeneratedDisconnectHook>,
}

impl GeneratedRuntimeBridge {
    /// Build a generated-runtime bridge plus its cloneable callback handle.
    pub fn new(
        on_connect: impl FnMut(&StdbClientConfig, GeneratedRuntimeHandle) -> Result<(), String>
            + 'static,
        on_subscribe: impl FnMut(&[String], GeneratedRuntimeHandle) -> Result<(), String> + 'static,
        on_disconnect: impl FnMut() + 'static,
    ) -> (Self, GeneratedRuntimeHandle) {
        let handle = GeneratedRuntimeHandle::default();
        (
            Self {
                handle: handle.clone(),
                on_connect: Box::new(on_connect),
                on_subscribe: Box::new(on_subscribe),
                on_disconnect: Box::new(on_disconnect),
            },
            handle,
        )
    }
}

impl GeneratedRuntimeAdapter for GeneratedRuntimeBridge {
    fn connect(&mut self, config: &StdbClientConfig) -> Result<(), String> {
        (self.on_connect)(config, self.handle.clone())
    }

    fn disconnect(&mut self) {
        (self.on_disconnect)();
    }

    fn subscribe(&mut self, queries: &[String]) -> Result<(), String> {
        (self.on_subscribe)(queries, self.handle.clone())
    }

    fn drain_events(&mut self) -> Vec<GeneratedRuntimeEvent> {
        self.handle.drain()
    }
}

// ============================================================
// CONNECTION STATE
// ============================================================

/// Current connection state of the client.
#[derive(Debug, Clone)]
pub enum ConnectionState {
    /// Not connected to any SpacetimeDB instance.
    Disconnected,
    /// Connection attempt in progress.
    Connecting,
    /// Connected and authenticated.
    Connected {
        /// Client Identity assigned by SpacetimeDB.
        identity: Vec<u8>,
        /// Authentication token (save for reconnection).
        token: String,
    },
    /// Connection failed or was terminated with an error.
    Error(String),
}

// ============================================================
// EVENTS
// ============================================================

/// Events emitted by the client for the game loop to consume.
///
/// Events are buffered internally and drained via [`StdbClient::drain_events`].
/// The game loop should process all events each frame.
#[derive(Debug, Clone)]
pub enum StdbEvent {
    // ── Connection lifecycle ──
    /// Successfully connected to SpacetimeDB.
    Connected { identity: Vec<u8>, token: String },
    /// Disconnected from SpacetimeDB.
    Disconnected { reason: String },
    /// Connection attempt failed.
    ConnectError { message: String },
    /// Initial subscription data has been received and cached.
    SubscriptionApplied,

    // ── World state ──
    /// World state singleton was updated.
    WorldStateUpdated {
        tick: u64,
        paused: bool,
        world_width: f32,
        world_height: f32,
    },
    /// Tick counter advanced (extracted from world_state updates).
    TickAdvanced { old_tick: u64, new_tick: u64 },

    // ── Entity lifecycle ──
    /// A new entity appeared in the subscription.
    EntityInserted { entity_id: u64 },
    /// An entity was updated (any component changed).
    EntityUpdated { entity_id: u64 },
    /// An entity was removed from the subscription (destroyed or out of range).
    EntityDeleted { entity_id: u64 },

    // ── Agent-specific ──
    /// Observation data received for a controlled entity.
    ObservationReceived {
        tick: u64,
        observer_entity_id: u64,
        observation_json: String,
    },
    /// Combat event occurred.
    CombatEventReceived {
        tick: u64,
        attacker_id: u64,
        defender_id: u64,
        damage: f32,
        killed: bool,
    },
    /// Speech event occurred.
    SpeechEventReceived {
        tick: u64,
        speaker_id: u64,
        message: String,
        volume: SpeakVolume,
    },
    /// World event occurred (spawn, death, etc.).
    WorldEventReceived {
        tick: u64,
        event_kind: WorldEventKind,
        entity_id: u64,
        secondary_entity_id: Option<u64>,
        data_json: String,
    },
    /// Per-agent authoritative telemetry row received for debug/editor consumers.
    AgentTelemetryTickReceived {
        tick: u64,
        agent_entity_id: u64,
        frame_json: String,
    },
    /// Detailed tool/provider side-effect row received for debug/editor consumers.
    AgentToolCallEventReceived {
        tick: u64,
        agent_entity_id: u64,
        tool_name: String,
        provider: String,
        status: String,
        document: String,
    },
    /// Durable aggregate telemetry rollup received for dashboards/analytics.
    AgentTickRollupReceived {
        tick_start: u64,
        tick_end: u64,
        agent_entity_id: u64,
        document: String,
    },
    /// Focused debug summary synthesized from tool-call/rollup streams.
    FocusedEntityDebugSummaryReceived {
        agent_entity_id: u64,
        document: String,
    },
    /// Authority-fed remote topology document received.
    RemoteTopologyDocumentReceived { document: String },
    /// Shared multi-world topology/evaluation bundle applied to the client cache.
    RemoteTopologyUpdated {
        scenario_id: String,
        resolved_world_id: Option<String>,
        world_count: usize,
        team_count: usize,
    },

    // ── Reducer acknowledgments ──
    /// A reducer call was acknowledged by the server.
    ReducerCallSuccess { reducer_name: String },
    /// A reducer call failed.
    ReducerCallError { reducer_name: String, error: String },
}

// ============================================================
// CACHED STATE
// ============================================================

/// Cached world state from the world_state singleton table.
#[derive(Debug, Clone)]
pub struct CachedWorldState {
    pub tick: u64,
    pub rng_seed: u64,
    pub ticks_per_second: u32,
    pub world_width: f32,
    pub world_height: f32,
    pub max_entities: u32,
    pub paused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RemoteTopologyDocumentCursor {
    row_id: u64,
    generated_at_unix_ms: u64,
}

/// Cached entity with all available components.
///
/// Components are `Option` because not every entity has every component.
/// The cache is populated from SpacetimeDB table subscriptions.
#[derive(Debug, Clone)]
pub struct CachedEntity {
    pub entity_id: u64,
    pub agent_type: Option<AgentType>,
    pub alive: bool,

    // Transform
    pub pos_x: Option<f32>,
    pub pos_y: Option<f32>,
    pub rotation: Option<f32>,

    // Velocity
    pub vel_x: Option<f32>,
    pub vel_y: Option<f32>,

    // Health
    pub health: Option<f32>,
    pub max_health: Option<f32>,
    pub armor: Option<f32>,
    pub invulnerable: Option<bool>,

    // Label
    pub name: Option<String>,
    pub team_id: Option<u8>,

    // Perception
    pub vision_range: Option<f32>,
    pub vision_fov: Option<f32>,
    pub hearing_range: Option<f32>,

    // Movement
    pub max_speed: Option<f32>,
}

impl CachedEntity {
    /// Create a minimal cached entity from just the entity row.
    pub fn from_entity(entity_id: u64, agent_type: Option<AgentType>, alive: bool) -> Self {
        Self {
            entity_id,
            agent_type,
            alive,
            pos_x: None,
            pos_y: None,
            rotation: None,
            vel_x: None,
            vel_y: None,
            health: None,
            max_health: None,
            armor: None,
            invulnerable: None,
            name: None,
            team_id: None,
            vision_range: None,
            vision_fov: None,
            hearing_range: None,
            max_speed: None,
        }
    }

    /// Position as (x, y) tuple, if transform is cached.
    pub fn position(&self) -> Option<(f32, f32)> {
        match (self.pos_x, self.pos_y) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        }
    }

    /// Health fraction (0.0 - 1.0), if health is cached.
    pub fn health_fraction(&self) -> Option<f32> {
        match (self.health, self.max_health) {
            (Some(h), Some(m)) if m > 0.0 => Some(h / m),
            _ => None,
        }
    }
}

// ============================================================
// ACTION BUILDER
// ============================================================

/// A typed action to submit to the server via the submit_action reducer.
///
/// This mirrors the flattened parameter set of `submit_action` but provides
/// a builder-pattern API for ergonomic construction.
#[derive(Debug, Clone)]
pub struct SubmittedAction {
    pub entity_id: u64,
    pub action_kind: ActionKind,
    pub direction_x: Option<f32>,
    pub direction_y: Option<f32>,
    pub angle: Option<f32>,
    pub target_x: Option<f32>,
    pub target_y: Option<f32>,
    pub target_entity_id: Option<u64>,
    pub ability_slot: Option<u8>,
    pub ability_target_kind: Option<AbilityTargetKind>,
    pub message: Option<String>,
    pub volume: Option<SpeakVolume>,
    pub signal_type: Option<String>,
    pub signal_data: Option<String>,
    pub prefab: Option<String>,
}

impl SubmittedAction {
    /// Create a Move action in the given direction (normalized).
    pub fn move_dir(entity_id: u64, dx: f32, dy: f32) -> Self {
        Self {
            entity_id,
            action_kind: ActionKind::Move,
            direction_x: Some(dx),
            direction_y: Some(dy),
            angle: None,
            target_x: None,
            target_y: None,
            target_entity_id: None,
            ability_slot: None,
            ability_target_kind: None,
            message: None,
            volume: None,
            signal_type: None,
            signal_data: None,
            prefab: None,
        }
    }

    /// Create a Stop action.
    pub fn stop(entity_id: u64) -> Self {
        Self {
            entity_id,
            action_kind: ActionKind::Stop,
            direction_x: None,
            direction_y: None,
            angle: None,
            target_x: None,
            target_y: None,
            target_entity_id: None,
            ability_slot: None,
            ability_target_kind: None,
            message: None,
            volume: None,
            signal_type: None,
            signal_data: None,
            prefab: None,
        }
    }

    /// Create a Rotate action to the given angle (radians).
    pub fn rotate(entity_id: u64, angle: f32) -> Self {
        Self {
            entity_id,
            action_kind: ActionKind::Rotate,
            direction_x: None,
            direction_y: None,
            angle: Some(angle),
            target_x: None,
            target_y: None,
            target_entity_id: None,
            ability_slot: None,
            ability_target_kind: None,
            message: None,
            volume: None,
            signal_type: None,
            signal_data: None,
            prefab: None,
        }
    }

    /// Create a Speak action.
    pub fn speak(entity_id: u64, message: String, volume: SpeakVolume) -> Self {
        Self {
            entity_id,
            action_kind: ActionKind::Speak,
            direction_x: None,
            direction_y: None,
            angle: None,
            target_x: None,
            target_y: None,
            target_entity_id: None,
            ability_slot: None,
            ability_target_kind: None,
            message: Some(message),
            volume: Some(volume),
            signal_type: None,
            signal_data: None,
            prefab: None,
        }
    }

    /// Create an Attack action toward a direction.
    pub fn attack(entity_id: u64, dx: f32, dy: f32) -> Self {
        Self {
            entity_id,
            action_kind: ActionKind::Attack,
            direction_x: Some(dx),
            direction_y: Some(dy),
            angle: None,
            target_x: None,
            target_y: None,
            target_entity_id: None,
            ability_slot: None,
            ability_target_kind: None,
            message: None,
            volume: None,
            signal_type: None,
            signal_data: None,
            prefab: None,
        }
    }

    /// Create an AttackTarget action toward a specific entity.
    pub fn attack_target(entity_id: u64, target_entity_id: u64) -> Self {
        Self {
            entity_id,
            action_kind: ActionKind::AttackTarget,
            direction_x: None,
            direction_y: None,
            angle: None,
            target_x: None,
            target_y: None,
            target_entity_id: Some(target_entity_id),
            ability_slot: None,
            ability_target_kind: None,
            message: None,
            volume: None,
            signal_type: None,
            signal_data: None,
            prefab: None,
        }
    }

    /// Create an Idle action (explicit no-op).
    pub fn idle(entity_id: u64) -> Self {
        Self {
            entity_id,
            action_kind: ActionKind::Idle,
            direction_x: None,
            direction_y: None,
            angle: None,
            target_x: None,
            target_y: None,
            target_entity_id: None,
            ability_slot: None,
            ability_target_kind: None,
            message: None,
            volume: None,
            signal_type: None,
            signal_data: None,
            prefab: None,
        }
    }
}

// ============================================================
// ERRORS
// ============================================================

/// Errors from the SpacetimeDB client wrapper.
#[derive(Debug, Clone)]
pub enum StdbError {
    /// Not connected to SpacetimeDB.
    NotConnected,
    /// Connection attempt failed.
    ConnectionFailed(String),
    /// Reducer call failed.
    ReducerError(String),
    /// Subscription failed.
    SubscriptionError(String),
    /// Authority document decode or validation failed.
    DocumentError(String),
    /// Invalid state for the requested operation.
    InvalidState(String),
}

impl std::fmt::Display for StdbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotConnected => write!(f, "Not connected to SpacetimeDB"),
            Self::ConnectionFailed(msg) => write!(f, "Connection failed: {msg}"),
            Self::ReducerError(msg) => write!(f, "Reducer error: {msg}"),
            Self::SubscriptionError(msg) => write!(f, "Subscription error: {msg}"),
            Self::DocumentError(msg) => write!(f, "Document error: {msg}"),
            Self::InvalidState(msg) => write!(f, "Invalid state: {msg}"),
        }
    }
}

impl std::error::Error for StdbError {}

// ============================================================
// SUBSCRIPTION QUERIES
// ============================================================

/// Pre-defined subscription queries for common use cases.
///
/// These correspond to SQL subscription queries that the SpacetimeDB client
/// SDK uses to filter which table rows the client receives.
pub struct Subscriptions;

impl Subscriptions {
    /// Subscribe to ALL public tables (development/debugging).
    pub fn all_tables() -> Vec<&'static str> {
        vec![
            "SELECT * FROM entity",
            "SELECT * FROM transform",
            "SELECT * FROM velocity",
            "SELECT * FROM health",
            "SELECT * FROM label",
            "SELECT * FROM perception",
            "SELECT * FROM movement",
            "SELECT * FROM agent_constraints",
            "SELECT * FROM color_rect",
            "SELECT * FROM sprite",
            "SELECT * FROM collider",
            "SELECT * FROM rigid_body",
            "SELECT * FROM script",
            "SELECT * FROM world_state",
            "SELECT * FROM connected_agent",
            "SELECT * FROM action_submission",
            "SELECT * FROM observation_event",
            "SELECT * FROM combat_event",
            "SELECT * FROM speech_event",
            "SELECT * FROM world_event",
            "SELECT * FROM agent_telemetry_tick",
            "SELECT * FROM agent_tool_call_event",
            "SELECT * FROM agent_tick_rollup",
            "SELECT * FROM remote_topology_document",
            "SELECT * FROM match_queue",
            "SELECT * FROM game_match",
            "SELECT * FROM match_participant",
            "SELECT * FROM lobby",
            "SELECT * FROM lobby_member",
        ]
    }

    /// Subscribe to the minimal set needed for a player agent.
    /// Includes world state, own entity components, and events.
    pub fn player_agent(entity_id: u64) -> Vec<String> {
        vec![
            "SELECT * FROM world_state".to_string(),
            "SELECT * FROM entity".to_string(),
            "SELECT * FROM transform".to_string(),
            "SELECT * FROM velocity".to_string(),
            "SELECT * FROM health".to_string(),
            "SELECT * FROM label".to_string(),
            "SELECT * FROM color_rect".to_string(),
            // Events (RLS filters observation_event automatically)
            "SELECT * FROM observation_event".to_string(),
            "SELECT * FROM combat_event".to_string(),
            "SELECT * FROM speech_event".to_string(),
            "SELECT * FROM world_event".to_string(),
            "SELECT * FROM remote_topology_document".to_string(),
            // Own agent connection
            format!("SELECT * FROM connected_agent WHERE entity_id = {entity_id}"),
        ]
    }

    /// Subscribe to read-only spectator view (all entities, all events).
    pub fn spectator() -> Vec<&'static str> {
        vec![
            "SELECT * FROM world_state",
            "SELECT * FROM entity",
            "SELECT * FROM transform",
            "SELECT * FROM velocity",
            "SELECT * FROM health",
            "SELECT * FROM label",
            "SELECT * FROM color_rect",
            "SELECT * FROM sprite",
            "SELECT * FROM connected_agent",
            "SELECT * FROM combat_event",
            "SELECT * FROM speech_event",
            "SELECT * FROM world_event",
            "SELECT * FROM remote_topology_document",
        ]
    }

    /// Subscribe to world state and entity tables only (no events).
    /// Useful for the editor/dashboard.
    pub fn editor() -> Vec<&'static str> {
        vec![
            "SELECT * FROM world_state",
            "SELECT * FROM entity",
            "SELECT * FROM transform",
            "SELECT * FROM velocity",
            "SELECT * FROM health",
            "SELECT * FROM label",
            "SELECT * FROM perception",
            "SELECT * FROM movement",
            "SELECT * FROM agent_constraints",
            "SELECT * FROM color_rect",
            "SELECT * FROM sprite",
            "SELECT * FROM collider",
            "SELECT * FROM rigid_body",
            "SELECT * FROM script",
            "SELECT * FROM connected_agent",
            "SELECT * FROM remote_topology_document",
        ]
    }

    /// Subscribe to debug-only telemetry tables.
    pub fn debug_telemetry() -> Vec<&'static str> {
        vec![
            "SELECT * FROM agent_telemetry_tick",
            "SELECT * FROM agent_tool_call_event",
            "SELECT * FROM agent_tick_rollup",
        ]
    }

    /// Subscribe to debug-only telemetry rows for a specific set of agent entities.
    ///
    /// This is the safer default for editor/debug tooling that is focused on a
    /// known selection, because it avoids subscribing to full-shard raw telemetry.
    pub fn debug_telemetry_for_entities(entity_ids: &[u64]) -> Vec<String> {
        let mut queries = Vec::new();

        for &entity_id in entity_ids {
            queries.push(format!(
                "SELECT * FROM agent_telemetry_tick WHERE agent_entity_id = {entity_id}"
            ));
            queries.push(format!(
                "SELECT * FROM agent_tool_call_event WHERE agent_entity_id = {entity_id}"
            ));
            queries.push(format!(
                "SELECT * FROM agent_tick_rollup WHERE agent_entity_id = {entity_id}"
            ));
        }

        queries.sort_unstable();
        queries.dedup();
        queries
    }

    /// Subscribe to editor world state plus debug telemetry streams.
    pub fn editor_with_debug_telemetry() -> Vec<String> {
        let mut queries = Self::editor()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<String>>();
        queries.extend(Self::debug_telemetry().into_iter().map(str::to_string));
        queries.sort_unstable();
        queries.dedup();
        queries
    }

    /// Subscribe to editor world state plus entity-scoped debug telemetry streams.
    pub fn editor_with_debug_telemetry_for_entities(entity_ids: &[u64]) -> Vec<String> {
        let mut queries = Self::editor()
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<String>>();
        queries.extend(Self::debug_telemetry_for_entities(entity_ids));
        queries.sort_unstable();
        queries.dedup();
        queries
    }

    /// Subscribe to lobby tables only.
    pub fn lobby_system() -> Vec<&'static str> {
        vec!["SELECT * FROM lobby", "SELECT * FROM lobby_member"]
    }

    /// Subscribe to matchmaking tables only.
    pub fn matchmaking_system() -> Vec<&'static str> {
        vec![
            "SELECT * FROM match_queue",
            "SELECT * FROM game_match",
            "SELECT * FROM match_participant",
        ]
    }
}

// ============================================================
// CLIENT
// ============================================================

/// High-level SpacetimeDB client for the Prompt or Die module.
///
/// Wraps the SpacetimeDB client SDK with typed methods for all game reducers
/// and a local entity cache populated from table subscriptions.
///
/// # Game Loop Integration
///
/// Call [`frame_tick`](Self::frame_tick) once per frame to process pending
/// SpacetimeDB messages, then drain events with [`drain_events`](Self::drain_events).
///
/// # Generated Bindings
///
/// The actual SpacetimeDB SDK integration requires generated client bindings.
/// Run `spacetime generate --lang rust` against the published module, then
/// wire the generated `DbConnection` into this client. Until then, methods
/// that require the SDK return [`StdbError::NotConnected`] stubs.
pub struct StdbClient {
    config: StdbClientConfig,
    state: ConnectionState,
    events: VecDeque<StdbEvent>,
    generated_runtime: Option<Box<dyn GeneratedRuntimeAdapter>>,

    // ── Local cache ──
    world_state: Option<CachedWorldState>,
    entities: HashMap<u64, CachedEntity>,
    /// Latest observation JSON per observer entity_id.
    observations: HashMap<u64, String>,
    /// Latest observation tick per observer entity_id.
    observation_ticks: HashMap<u64, u64>,
    /// Controlled entity ID (set after connect_agent).
    controlled_entity: Option<u64>,
    /// Active SQL subscription query set.
    active_queries: Vec<String>,
    /// Monotonic entity id source for local reducer emulation.
    next_entity_id: u64,
    /// Retained focused debug summaries synthesized from tool-call and rollup docs.
    focused_debug_summaries: HashMap<u64, FocusedEntityDebugSummary>,
    /// Latest shared multi-world topology artifact applied to this client.
    remote_topology: Option<RemoteTopologyBundle>,
    /// Latest authority-published topology row accepted by the client.
    latest_remote_topology_document_cursor: Option<RemoteTopologyDocumentCursor>,

    // ── Metrics ──
    frames_processed: u64,
    reducers_called: u64,
    events_received: u64,
}

impl StdbClient {
    /// Create a new client with the given configuration.
    pub fn new(config: StdbClientConfig) -> Self {
        Self {
            config,
            state: ConnectionState::Disconnected,
            events: VecDeque::new(),
            generated_runtime: None,
            world_state: None,
            entities: HashMap::new(),
            observations: HashMap::new(),
            observation_ticks: HashMap::new(),
            controlled_entity: None,
            active_queries: Vec::new(),
            next_entity_id: 1,
            focused_debug_summaries: HashMap::new(),
            remote_topology: None,
            latest_remote_topology_document_cursor: None,
            frames_processed: 0,
            reducers_called: 0,
            events_received: 0,
        }
    }

    /// Attach a generated-runtime adapter.
    ///
    /// Call this before [`connect`](Self::connect) when running in
    /// [`StdbConnectionMode::Generated`].
    pub fn set_generated_runtime(&mut self, runtime: Box<dyn GeneratedRuntimeAdapter>) {
        self.generated_runtime = Some(runtime);
    }

    /// Install the command-driven generated binding runtime and return its endpoint.
    ///
    /// Call this before [`connect`](Self::connect) when running in
    /// [`StdbConnectionMode::Generated`] and an external binding host needs to
    /// observe outbound commands and deliver inbound callbacks separately.
    pub fn install_generated_binding_runtime(&mut self) -> GeneratedBindingEndpoint {
        let (runtime, endpoint) = GeneratedBindingRuntime::new();
        self.set_generated_runtime(Box::new(runtime));
        endpoint
    }

    /// Install the real generated SpacetimeDB SDK runtime.
    ///
    /// This uses the generated `DbConnection` and typed table callbacks from
    /// [`crate::module_bindings`] instead of the synthetic command-queue test seam.
    pub fn install_generated_sdk_runtime(&mut self) {
        self.set_generated_runtime(Box::new(GeneratedSdkRuntime::new()));
    }

    /// Wire a generated-runtime bridge backed by callback hooks.
    ///
    /// This is kept as a lightweight hook seam for focused tests and helpers.
    /// Real generated binding integrations should prefer
    /// [`GeneratedBindingRuntime`] plus [`GeneratedBindingEndpoint`], which
    /// preserve the connect/subscribe command flow instead of auto-acking it.
    pub fn set_generated_runtime_bridge(&mut self, runtime: GeneratedRuntimeBridge) {
        self.generated_runtime = Some(Box::new(runtime));
    }

    // ── Connection lifecycle ──

    /// Initiate connection to the SpacetimeDB module.
    ///
    /// This begins an async connection attempt. The actual connection is
    /// established during subsequent [`frame_tick`](Self::frame_tick) calls.
    /// A [`StdbEvent::Connected`] or [`StdbEvent::ConnectError`] event will
    /// be emitted when the connection resolves.
    ///
    /// # Generated Bindings Required
    ///
    /// Full implementation requires generated bindings from `spacetime generate`.
    /// Use [`StdbConnectionMode::Generated`] for that production path.
    ///
    /// Local emulation mode is available only when explicitly selected through
    /// [`StdbConnectionMode::Emulated`].
    ///
    /// When bindings are available, this will use:
    /// ```rust,ignore
    /// DbConnection::builder()
    ///     .with_uri(&self.config.host)
    ///     .with_database_name(&self.config.db_name)
    ///     .with_token(self.config.auth_token.clone())
    ///     .on_connect(|conn, identity, token| { ... })
    ///     .on_connect_error(|err| { ... })
    ///     .on_disconnect(|conn, err| { ... })
    ///     .build()
    ///     .expect("Failed to connect");
    /// ```
    pub fn connect(&mut self) -> Result<(), StdbError> {
        match &self.state {
            ConnectionState::Connected { .. } => {
                return Err(StdbError::InvalidState("Already connected".into()));
            }
            ConnectionState::Connecting => {
                return Err(StdbError::InvalidState(
                    "Connection already in progress".into(),
                ));
            }
            _ => {}
        }

        if matches!(self.config.connection_mode, StdbConnectionMode::Generated) {
            let runtime = self.generated_runtime.as_mut().ok_or_else(|| {
                self.state = ConnectionState::Error("generated SpacetimeDB runtime is not installed".into());
                StdbError::ConnectionFailed(
                    "generated SpacetimeDB runtime is not installed; call install_generated_sdk_runtime() for the live bindings path or use StdbConnectionMode::Emulated for local fallback".into(),
                )
            })?;

            runtime.connect(&self.config).map_err(|message| {
                self.state = ConnectionState::Error(message.clone());
                StdbError::ConnectionFailed(message)
            })?;
            self.state = ConnectionState::Connecting;
            log::info!(
                "[pod-stdb client] Connecting to {}:{} via generated runtime ...",
                self.config.host,
                self.config.db_name
            );
            return Ok(());
        }

        self.state = ConnectionState::Connecting;
        log::info!(
            "[pod-stdb client] Connecting to {}:{} ...",
            self.config.host,
            self.config.db_name
        );

        Ok(())
    }

    /// Disconnect from SpacetimeDB.
    pub fn disconnect(&mut self) {
        if matches!(
            self.state,
            ConnectionState::Connected { .. } | ConnectionState::Connecting
        ) {
            if let Some(runtime) = self.generated_runtime.as_mut() {
                runtime.disconnect();
            }
            log::info!("[pod-stdb client] Disconnecting...");
            self.state = ConnectionState::Disconnected;
            self.active_queries.clear();
            self.events.push_back(StdbEvent::Disconnected {
                reason: "Client requested disconnect".into(),
            });
        }
    }

    /// Process pending SpacetimeDB messages.
    ///
    /// Call this once per game loop frame. In the real implementation, this
    /// calls `DbConnection::frame_tick()` which processes all pending messages
    /// from the SpacetimeDB server and triggers table callbacks.
    ///
    /// Messages are buffered as [`StdbEvent`]s — drain them with
    /// [`drain_events`](Self::drain_events).
    pub fn frame_tick(&mut self) {
        self.frames_processed += 1;
        if matches!(self.config.connection_mode, StdbConnectionMode::Generated) {
            let runtime_events = self
                .generated_runtime
                .as_mut()
                .map(|runtime| runtime.drain_events())
                .unwrap_or_default();
            for event in runtime_events {
                self.apply_generated_runtime_event(event);
            }
            return;
        }

        if let ConnectionState::Connecting = &self.state {
            let token = self.config.auth_token.clone().unwrap_or_else(|| {
                format!(
                    "pod-local-{}-{}",
                    self.config.db_name, self.frames_processed
                )
            });
            let mut identity = vec![0u8; 16];
            for (idx, byte) in self
                .config
                .player_name
                .as_bytes()
                .iter()
                .chain(self.config.db_name.as_bytes().iter())
                .enumerate()
            {
                identity[idx % 16] ^= *byte;
            }
            self.state = ConnectionState::Connected {
                identity: identity.clone(),
                token: token.clone(),
            };
            self.events
                .push_back(StdbEvent::Connected { identity, token });

            if self.world_state.is_none() {
                self.update_world_state(CachedWorldState {
                    tick: 0,
                    rng_seed: 42,
                    ticks_per_second: 60,
                    world_width: 2000.0,
                    world_height: 2000.0,
                    max_entities: 10000,
                    paused: true,
                });
            }
        }
    }

    /// Drain all buffered events.
    ///
    /// Returns an iterator over all events that have accumulated since the
    /// last drain. Call this after [`frame_tick`](Self::frame_tick).
    pub fn drain_events(&mut self) -> impl Iterator<Item = StdbEvent> + '_ {
        self.events.drain(..)
    }

    /// Peek at buffered events without consuming them.
    pub fn peek_events(&self) -> &VecDeque<StdbEvent> {
        &self.events
    }

    /// Check if any events are pending.
    pub fn has_events(&self) -> bool {
        !self.events.is_empty()
    }

    // ── Subscription management ──

    /// Subscribe to tables using pre-defined query sets.
    ///
    /// Call this after receiving [`StdbEvent::Connected`].
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Subscribe as a player agent
    /// client.subscribe(Subscriptions::player_agent(my_entity_id));
    ///
    /// // Or subscribe to everything (development)
    /// client.subscribe(
    ///     Subscriptions::all_tables().into_iter().map(|s| s.to_string()).collect()
    /// );
    /// ```
    pub fn subscribe(&mut self, queries: Vec<String>) -> Result<(), StdbError> {
        if !self.is_connected() {
            return Err(StdbError::NotConnected);
        }
        if queries.is_empty() {
            return Err(StdbError::SubscriptionError(
                "Subscription query set cannot be empty".into(),
            ));
        }

        log::info!(
            "[pod-stdb client] Subscribing with {} queries",
            queries.len()
        );
        if matches!(self.config.connection_mode, StdbConnectionMode::Generated) {
            let runtime = self.generated_runtime.as_mut().ok_or_else(|| {
                StdbError::SubscriptionError(
                    "generated SpacetimeDB runtime is unavailable for subscription".into(),
                )
            })?;
            runtime
                .subscribe(&queries)
                .map_err(StdbError::SubscriptionError)?;
            self.active_queries = queries;
            return Ok(());
        }

        self.active_queries = queries;
        self.events.push_back(StdbEvent::SubscriptionApplied);

        Ok(())
    }

    // ── Reducer calls (typed wrappers) ──

    /// Call the `create_world` reducer to initialize or reset the game world.
    pub fn call_create_world(
        &mut self,
        seed: u64,
        width: f32,
        height: f32,
        tps: u32,
    ) -> Result<(), StdbError> {
        self.require_connected()?;
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err(StdbError::ReducerError(
                "World dimensions must be finite and positive".into(),
            ));
        }
        if tps == 0 {
            return Err(StdbError::ReducerError(
                "ticks_per_second must be greater than zero".into(),
            ));
        }
        self.reducers_called += 1;
        log::debug!(
            "[pod-stdb client] Calling create_world(seed={seed}, {width}x{height}, tps={tps})"
        );
        self.update_world_state(CachedWorldState {
            tick: 0,
            rng_seed: seed,
            ticks_per_second: tps,
            world_width: width,
            world_height: height,
            max_entities: 10000,
            paused: true,
        });
        self.record_reducer_success("create_world");
        Ok(())
    }

    /// Call the `set_paused` reducer.
    pub fn call_set_paused(&mut self, paused: bool) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::debug!("[pod-stdb client] Calling set_paused({paused})");
        if let Some(mut world_state) = self.world_state.clone() {
            world_state.paused = paused;
            self.update_world_state(world_state);
        }
        self.record_reducer_success("set_paused");
        Ok(())
    }

    /// Call the `spawn_entity` reducer.
    pub fn call_spawn_entity(
        &mut self,
        pos_x: f32,
        pos_y: f32,
        agent_type: Option<AgentType>,
    ) -> Result<(), StdbError> {
        self.require_connected()?;
        if !pos_x.is_finite() || !pos_y.is_finite() {
            return Err(StdbError::ReducerError(
                "Spawn position must be finite".into(),
            ));
        }
        self.reducers_called += 1;
        log::debug!("[pod-stdb client] Calling spawn_entity({pos_x}, {pos_y}, {agent_type:?})");
        let entity_id = self.next_entity_id;
        self.next_entity_id += 1;

        let mut entity = CachedEntity::from_entity(entity_id, agent_type, true);
        entity.pos_x = Some(pos_x);
        entity.pos_y = Some(pos_y);
        entity.rotation = Some(0.0);
        entity.vel_x = Some(0.0);
        entity.vel_y = Some(0.0);
        entity.max_speed = Some(200.0);
        entity.health = Some(100.0);
        entity.max_health = Some(100.0);
        self.upsert_entity(entity);
        self.record_reducer_success("spawn_entity");
        Ok(())
    }

    /// Call the `connect_agent` reducer to claim control of an entity.
    ///
    /// On success, the client will begin receiving observations for this entity
    /// (filtered by RLS if the `unstable` feature is enabled on the module).
    pub fn call_connect_agent(
        &mut self,
        entity_id: u64,
        agent_type: AgentType,
        display_name: String,
    ) -> Result<(), StdbError> {
        self.require_connected()?;
        if self.entity(entity_id).is_none() {
            return Err(StdbError::ReducerError(format!(
                "Cannot connect agent to unknown entity {entity_id}"
            )));
        }
        self.controlled_entity = Some(entity_id);
        self.reducers_called += 1;
        log::info!(
            "[pod-stdb client] Calling connect_agent(entity={entity_id}, type={agent_type:?}, name='{display_name}')"
        );
        self.record_reducer_success("connect_agent");
        Ok(())
    }

    /// Call the `connect_agent` reducer as a remote LLM player.
    ///
    /// This is a convenience wrapper that forces `agent_type` to
    /// [`AgentType::LlmAgent`], so callers do not need to pass the enum
    /// value directly.
    pub fn call_connect_llm_agent(
        &mut self,
        entity_id: u64,
        display_name: String,
    ) -> Result<(), StdbError> {
        self.call_connect_agent(entity_id, AgentType::LlmAgent, display_name)
    }

    /// Create a new lobby owned by this client identity.
    pub fn call_create_lobby(
        &mut self,
        name: String,
        host_entity_id: u64,
        max_players: u32,
        is_private: bool,
    ) -> Result<(), StdbError> {
        self.require_connected()?;
        if name.trim().is_empty() {
            return Err(StdbError::ReducerError("Lobby name cannot be empty".into()));
        }
        if max_players == 0 {
            return Err(StdbError::ReducerError(
                "Lobby max_players must be > 0".into(),
            ));
        }
        if self.entity(host_entity_id).is_none() {
            return Err(StdbError::ReducerError(format!(
                "Host entity {host_entity_id} not found"
            )));
        }
        self.reducers_called += 1;
        log::info!(
            "[pod-stdb client] Calling create_lobby(name='{name}', host_entity_id={host_entity_id}, max_players={max_players}, is_private={is_private})"
        );
        let _ = is_private;
        self.record_reducer_success("create_lobby");
        Ok(())
    }

    /// Join a lobby by ID.
    pub fn call_join_lobby(&mut self, lobby_id: u64, entity_id: u64) -> Result<(), StdbError> {
        self.require_connected()?;
        if lobby_id == 0 {
            return Err(StdbError::ReducerError("Lobby id must be non-zero".into()));
        }
        if self.entity(entity_id).is_none() {
            return Err(StdbError::ReducerError(format!(
                "Entity {entity_id} not found"
            )));
        }
        self.reducers_called += 1;
        log::info!(
            "[pod-stdb client] Calling join_lobby(lobby_id={lobby_id}, entity_id={entity_id})"
        );
        self.record_reducer_success("join_lobby");
        Ok(())
    }

    /// Leave the lobby the caller is currently joined to.
    pub fn call_leave_lobby(&mut self) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::info!("[pod-stdb client] Calling leave_lobby");
        self.record_reducer_success("leave_lobby");
        Ok(())
    }

    /// Mark this player as ready or not ready in a lobby.
    pub fn call_set_lobby_ready(&mut self, lobby_id: u64, is_ready: bool) -> Result<(), StdbError> {
        self.require_connected()?;
        if lobby_id == 0 {
            return Err(StdbError::ReducerError("Lobby id must be non-zero".into()));
        }
        self.reducers_called += 1;
        log::info!(
            "[pod-stdb client] Calling set_lobby_ready(lobby_id={lobby_id}, is_ready={is_ready})"
        );
        let _ = is_ready;
        self.record_reducer_success("set_lobby_ready");
        Ok(())
    }

    /// Start a lobby (host-only action).
    pub fn call_start_lobby(&mut self, lobby_id: u64) -> Result<(), StdbError> {
        self.require_connected()?;
        if lobby_id == 0 {
            return Err(StdbError::ReducerError("Lobby id must be non-zero".into()));
        }
        self.reducers_called += 1;
        log::info!("[pod-stdb client] Calling start_lobby({lobby_id})");
        self.record_reducer_success("start_lobby");
        Ok(())
    }

    /// Join the matchmaking queue with desired party size.
    pub fn call_join_match_queue(
        &mut self,
        entity_id: u64,
        desired_party_size: u32,
    ) -> Result<(), StdbError> {
        self.require_connected()?;
        if desired_party_size == 0 {
            return Err(StdbError::ReducerError(
                "desired_party_size must be > 0".into(),
            ));
        }
        if self.entity(entity_id).is_none() {
            return Err(StdbError::ReducerError(format!(
                "Entity {entity_id} not found"
            )));
        }
        self.reducers_called += 1;
        log::info!(
            "[pod-stdb client] Calling join_match_queue(entity_id={entity_id}, desired_party_size={desired_party_size})"
        );
        self.record_reducer_success("join_match_queue");
        Ok(())
    }

    /// Leave the caller from matchmaking queue.
    pub fn call_leave_match_queue(&mut self) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::info!("[pod-stdb client] Calling leave_match_queue()");
        self.record_reducer_success("leave_match_queue");
        Ok(())
    }

    /// Create a match from the matchmaking queue.
    pub fn call_create_match_from_queue(
        &mut self,
        desired_party_size: u32,
    ) -> Result<(), StdbError> {
        self.require_connected()?;
        if desired_party_size == 0 {
            return Err(StdbError::ReducerError(
                "desired_party_size must be > 0".into(),
            ));
        }
        self.reducers_called += 1;
        log::info!(
            "[pod-stdb client] Calling create_match_from_queue(desired_party_size={desired_party_size})"
        );
        self.record_reducer_success("create_match_from_queue");
        Ok(())
    }

    /// Call the `submit_action` reducer with a typed action.
    pub fn call_submit_action(&mut self, action: &SubmittedAction) -> Result<(), StdbError> {
        self.require_connected()?;
        if self.entity(action.entity_id).is_none() {
            return Err(StdbError::ReducerError(format!(
                "Action source entity {} not found",
                action.entity_id
            )));
        }
        self.reducers_called += 1;
        log::debug!(
            "[pod-stdb client] Calling submit_action(entity={}, kind={:?})",
            action.entity_id,
            action.action_kind
        );
        self.apply_submitted_action(action)?;
        self.record_reducer_success("submit_action");
        Ok(())
    }

    /// Call the `execute_tick` reducer (server/admin only).
    pub fn call_execute_tick(&mut self) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::debug!("[pod-stdb client] Calling execute_tick()");
        let mut ws = self
            .world_state
            .clone()
            .ok_or_else(|| StdbError::InvalidState("World state missing".into()))?;

        if !ws.paused {
            let dt = 1.0 / ws.ticks_per_second as f32;
            for entity in self.entities.values_mut() {
                if let (Some(x), Some(y), Some(vx), Some(vy)) =
                    (entity.pos_x, entity.pos_y, entity.vel_x, entity.vel_y)
                {
                    let nx = (x + vx * dt).clamp(0.0, ws.world_width);
                    let ny = (y + vy * dt).clamp(0.0, ws.world_height);
                    entity.pos_x = Some(nx);
                    entity.pos_y = Some(ny);
                }
            }
            ws.tick += 1;
            self.update_world_state(ws);
        }
        self.record_reducer_success("execute_tick");
        Ok(())
    }

    /// Call the `destroy_entity` reducer (admin only).
    pub fn call_destroy_entity(&mut self, entity_id: u64) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::debug!("[pod-stdb client] Calling destroy_entity({entity_id})");
        if self.entities.remove(&entity_id).is_none() {
            return Err(StdbError::ReducerError(format!(
                "Entity {entity_id} not found"
            )));
        }
        self.remove_entity(entity_id);
        self.record_reducer_success("destroy_entity");
        Ok(())
    }

    // ── Cache accessors ──

    /// Get the cached world state, if subscribed and received.
    pub fn world_state(&self) -> Option<&CachedWorldState> {
        self.world_state.as_ref()
    }

    /// Get the current tick from cached world state.
    pub fn current_tick(&self) -> Option<u64> {
        self.world_state.as_ref().map(|ws| ws.tick)
    }

    /// Check if the world is paused.
    pub fn is_paused(&self) -> Option<bool> {
        self.world_state.as_ref().map(|ws| ws.paused)
    }

    /// Get a cached entity by ID.
    pub fn entity(&self, entity_id: u64) -> Option<&CachedEntity> {
        self.entities.get(&entity_id)
    }

    /// Get all cached entities.
    pub fn entities(&self) -> &HashMap<u64, CachedEntity> {
        &self.entities
    }

    /// Get the number of cached entities.
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Get the latest observation JSON for an entity.
    pub fn latest_observation(&self, entity_id: u64) -> Option<&str> {
        self.observations.get(&entity_id).map(|s| s.as_str())
    }

    /// Get the latest observation tick for an entity.
    pub fn latest_observation_tick(&self, entity_id: u64) -> Option<u64> {
        self.observation_ticks.get(&entity_id).copied()
    }

    /// Get the entity ID this client controls (set after connect_agent).
    pub fn controlled_entity(&self) -> Option<u64> {
        self.controlled_entity
    }

    /// Apply a shared remote topology bundle to the client-side cache.
    pub fn apply_remote_topology(&mut self, topology: RemoteTopologyBundle) {
        self.apply_remote_topology_resolved(topology, None);
    }

    /// Store a received authority-published remote-topology row.
    ///
    /// Rows are monotonic by `(generated_at_unix_ms, row_id)`. Older rows are
    /// ignored so out-of-order delivery cannot roll back the active topology.
    pub fn receive_remote_topology_document_row(
        &mut self,
        row_id: u64,
        generated_at_unix_ms: u64,
        _scenario_id: String,
        _profile_id: String,
        topology_json: String,
    ) -> Result<(), StdbError> {
        if let Some(cursor) = self.latest_remote_topology_document_cursor {
            let stale = generated_at_unix_ms < cursor.generated_at_unix_ms
                || (generated_at_unix_ms == cursor.generated_at_unix_ms && row_id <= cursor.row_id);
            if stale {
                return Ok(());
            }
        }

        self.receive_debug_document(topology_json)?;
        self.latest_remote_topology_document_cursor = Some(RemoteTopologyDocumentCursor {
            row_id,
            generated_at_unix_ms,
        });
        Ok(())
    }

    /// Apply a shared remote topology bundle received as an authority TOON document.
    pub fn receive_remote_topology_document(&mut self, document: String) -> Result<(), StdbError> {
        let topology: RemoteTopologyBundle =
            decode_toon_document(&document, "remote_topology_bundle").map_err(|error| {
                StdbError::DocumentError(format!(
                    "failed to decode remote_topology_bundle: {error}"
                ))
            })?;
        self.apply_remote_topology_resolved(topology, Some(document));
        Ok(())
    }

    /// Store an authority-fed TOON document and dispatch it to the matching
    /// debug/topology ingress path.
    pub fn receive_debug_document(&mut self, document: String) -> Result<(), StdbError> {
        let document_type = decode_toon_value(&document)
            .map_err(StdbError::DocumentError)?
            .get("document_type")
            .and_then(|value| value.as_str())
            .map(str::to_owned)
            .ok_or_else(|| {
                StdbError::DocumentError(
                    "decoded TOON document missing string `document_type` field".into(),
                )
            })?;

        match document_type.as_str() {
            "remote_topology_bundle" => self.receive_remote_topology_document(document),
            "versioned_tick_telemetry" => {
                let telemetry: VersionedTickTelemetry =
                    decode_toon_document(&document, "versioned_tick_telemetry").map_err(
                        |error| {
                            StdbError::DocumentError(format!(
                                "failed to decode versioned_tick_telemetry: {error}"
                            ))
                        },
                    )?;
                for frame in telemetry.payload.agents {
                    if let Some(entity_id) = frame.entity_id {
                        self.receive_agent_telemetry_tick(
                            frame.tick,
                            entity_id.0,
                            document.clone(),
                        );
                    }
                }
                Ok(())
            }
            "agent_tool_call_event" => {
                let event: AgentToolCallEvent =
                    decode_toon_document(&document, "agent_tool_call_event").map_err(|error| {
                        StdbError::DocumentError(format!(
                            "failed to decode agent_tool_call_event: {error}"
                        ))
                    })?;
                self.receive_agent_tool_call_event(
                    event.tick,
                    event.agent_entity_id,
                    event.trace.tool_name.clone(),
                    event.trace.provider.clone(),
                    format!("{:?}", event.trace.status),
                    document,
                );
                Ok(())
            }
            "agent_tick_rollup" => {
                let rollup: AgentTickRollup = decode_toon_document(&document, "agent_tick_rollup")
                    .map_err(|error| {
                        StdbError::DocumentError(format!(
                            "failed to decode agent_tick_rollup: {error}"
                        ))
                    })?;
                self.receive_agent_tick_rollup(
                    rollup.tick_start,
                    rollup.tick_end,
                    rollup.agent_entity_id,
                    document,
                );
                Ok(())
            }
            "focused_entity_debug_summary" => {
                let summary: FocusedEntityDebugSummary =
                    decode_toon_document(&document, "focused_entity_debug_summary").map_err(
                        |error| {
                            StdbError::DocumentError(format!(
                                "failed to decode focused_entity_debug_summary: {error}"
                            ))
                        },
                    )?;
                self.events
                    .push_back(StdbEvent::FocusedEntityDebugSummaryReceived {
                        agent_entity_id: summary.entity_id,
                        document,
                    });
                self.events_received += 1;
                Ok(())
            }
            other => Err(StdbError::DocumentError(format!(
                "unsupported debug document type `{other}`"
            ))),
        }
    }

    fn apply_remote_topology_resolved(
        &mut self,
        topology: RemoteTopologyBundle,
        document: Option<String>,
    ) {
        if let Some(document) = document {
            self.events
                .push_back(StdbEvent::RemoteTopologyDocumentReceived { document });
            self.events_received += 1;
        }
        let resolved_world_id = resolve_topology_world_id(&self.config.db_name, &topology);
        let scenario_id = topology.scenario_id.clone();
        let world_count = topology.worlds.len();
        let team_count = topology.teams.len();
        self.remote_topology = Some(topology);
        self.events.push_back(StdbEvent::RemoteTopologyUpdated {
            scenario_id,
            resolved_world_id,
            world_count,
            team_count,
        });
        self.events_received += 1;
    }

    /// Return the last applied shared topology bundle, if any.
    pub fn remote_topology(&self) -> Option<&RemoteTopologyBundle> {
        self.remote_topology.as_ref()
    }

    /// Resolve the active remote world definition for this client.
    pub fn resolved_remote_world(&self) -> Option<&WorldRealityDefinition> {
        let topology = self.remote_topology.as_ref()?;
        let world_id = resolve_topology_world_id(&self.config.db_name, topology)?;
        topology
            .worlds
            .iter()
            .find(|world| world.world_id == world_id)
    }

    /// Resolve the active remote world id for this client.
    pub fn resolved_remote_world_id(&self) -> Option<&str> {
        self.resolved_remote_world()
            .map(|world| world.world_id.as_str())
    }

    /// Resolve the authored quest binding for the active remote world.
    pub fn resolved_remote_world_quest_binding(&self) -> Option<&WorldQuestBinding> {
        let topology = self.remote_topology.as_ref()?;
        let world_id = self.resolved_remote_world_id()?;
        topology
            .world_quest_bindings
            .iter()
            .find(|binding| binding.world_id == world_id)
    }

    /// Resolve the deterministic admitted roster for the active remote world.
    pub fn resolved_remote_world_admissions(
        &self,
    ) -> Option<&pod_core::WorldAdmissionSummary> {
        let topology = self.remote_topology.as_ref()?;
        let world_id = self.resolved_remote_world_id()?;
        topology
            .world_admissions
            .iter()
            .find(|summary| summary.world_id == world_id)
    }

    /// Resolve the applied world-state summary for the active remote world.
    pub fn resolved_remote_applied_world_state(&self) -> Option<&AppliedWorldStateSummary> {
        let topology = self.remote_topology.as_ref()?;
        let world_id = self.resolved_remote_world_id()?;
        topology
            .applied_world_states
            .iter()
            .find(|state| state.world_id == world_id)
    }

    /// Resolve the evaluation summary for the active remote world.
    pub fn resolved_remote_world_evaluation(&self) -> Option<&WorldEvaluationSummary> {
        let topology = self.remote_topology.as_ref()?;
        let world_id = self.resolved_remote_world_id()?;
        topology
            .evaluation
            .worlds
            .iter()
            .find(|world| world.world_id == world_id)
    }

    /// Resolve a symbolic team id from the numeric entity team slot.
    pub fn resolved_remote_team_key(&self, numeric_team_id: Option<u8>) -> Option<String> {
        let team_index = usize::from(numeric_team_id?.checked_sub(1)?);
        self.resolved_remote_world()
            .and_then(|world| world.active_team_ids.get(team_index).cloned())
    }

    // ── Connection state ──

    /// Check if the client is currently connected.
    pub fn is_connected(&self) -> bool {
        matches!(self.state, ConnectionState::Connected { .. })
    }

    /// Get the current connection state.
    pub fn connection_state(&self) -> &ConnectionState {
        &self.state
    }

    /// Get client configuration (read-only).
    pub fn config(&self) -> &StdbClientConfig {
        &self.config
    }

    // ── Metrics ──

    /// Number of frame_tick calls processed.
    pub fn frames_processed(&self) -> u64 {
        self.frames_processed
    }

    /// Number of reducer calls made.
    pub fn reducers_called(&self) -> u64 {
        self.reducers_called
    }

    /// Number of events received from SpacetimeDB.
    pub fn events_received(&self) -> u64 {
        self.events_received
    }

    fn apply_generated_runtime_event(&mut self, event: GeneratedRuntimeEvent) {
        match event {
            GeneratedRuntimeEvent::Connected { identity, token } => {
                self.state = ConnectionState::Connected {
                    identity: identity.clone(),
                    token: token.clone(),
                };
                self.events
                    .push_back(StdbEvent::Connected { identity, token });
            }
            GeneratedRuntimeEvent::ConnectError { message } => {
                self.state = ConnectionState::Error(message.clone());
                self.events.push_back(StdbEvent::ConnectError { message });
            }
            GeneratedRuntimeEvent::Disconnected { reason } => {
                self.state = ConnectionState::Disconnected;
                self.active_queries.clear();
                self.events.push_back(StdbEvent::Disconnected { reason });
            }
            GeneratedRuntimeEvent::SubscriptionApplied => {
                self.events.push_back(StdbEvent::SubscriptionApplied);
            }
            GeneratedRuntimeEvent::RemoteTopologyDocumentRow(row) => {
                if let Err(error) = self.receive_remote_topology_document_row(
                    row.row_id,
                    row.generated_at_unix_ms,
                    row.scenario_id,
                    row.profile_id,
                    row.topology_json,
                ) {
                    log::warn!(
                        "[pod-stdb client] Failed to apply generated remote_topology_document row: {error}"
                    );
                }
            }
        }
    }

    fn record_reducer_success(&mut self, reducer_name: &str) {
        self.events.push_back(StdbEvent::ReducerCallSuccess {
            reducer_name: reducer_name.to_string(),
        });
    }

    fn current_tick_value(&self) -> u64 {
        self.world_state.as_ref().map(|ws| ws.tick).unwrap_or(0)
    }

    fn apply_submitted_action(&mut self, action: &SubmittedAction) -> Result<(), StdbError> {
        const ATTACK_RANGE: f32 = 80.0;
        const INTERACT_RANGE: f32 = 50.0;
        const BASE_DAMAGE: f32 = 10.0;

        let source_id = action.entity_id;
        let source = self
            .entities
            .get(&source_id)
            .ok_or_else(|| StdbError::ReducerError(format!("Entity {source_id} missing")))?;
        let source_pos = source.position().ok_or_else(|| {
            StdbError::ReducerError(format!("Entity {source_id} has no transform"))
        })?;
        let source_team = source.team_id.unwrap_or(0);

        match action.action_kind {
            ActionKind::Move => {
                if let (Some(dx), Some(dy)) = (action.direction_x, action.direction_y) {
                    if let Some(entity) = self.entities.get_mut(&source_id) {
                        let len = (dx * dx + dy * dy).sqrt();
                        if len > 0.001 {
                            let speed = entity.max_speed.unwrap_or(200.0);
                            entity.vel_x = Some((dx / len) * speed);
                            entity.vel_y = Some((dy / len) * speed);
                            self.events.push_back(StdbEvent::EntityUpdated {
                                entity_id: source_id,
                            });
                        }
                    }
                }
            }
            ActionKind::Stop => {
                if let Some(entity) = self.entities.get_mut(&source_id) {
                    entity.vel_x = Some(0.0);
                    entity.vel_y = Some(0.0);
                    self.events.push_back(StdbEvent::EntityUpdated {
                        entity_id: source_id,
                    });
                }
            }
            ActionKind::Rotate => {
                if let (Some(entity), Some(angle)) =
                    (self.entities.get_mut(&source_id), action.angle)
                {
                    entity.rotation = Some(angle);
                    self.events.push_back(StdbEvent::EntityUpdated {
                        entity_id: source_id,
                    });
                }
            }
            ActionKind::LookAt => {
                if let (Some(entity), Some(tx), Some(ty)) = (
                    self.entities.get_mut(&source_id),
                    action.target_x,
                    action.target_y,
                ) {
                    if let Some((sx, sy)) = entity.position() {
                        entity.rotation = Some((ty - sy).atan2(tx - sx));
                        self.events.push_back(StdbEvent::EntityUpdated {
                            entity_id: source_id,
                        });
                    }
                }
            }
            ActionKind::Attack => {
                let mut target: Option<u64> = None;
                let mut best_distance = f32::MAX;
                for (candidate_id, candidate) in &self.entities {
                    if *candidate_id == source_id || !candidate.alive {
                        continue;
                    }
                    let Some((tx, ty)) = candidate.position() else {
                        continue;
                    };
                    let dx = tx - source_pos.0;
                    let dy = ty - source_pos.1;
                    let distance = (dx * dx + dy * dy).sqrt();
                    let candidate_team = candidate.team_id.unwrap_or(0);
                    let hostile =
                        source_team == 0 || candidate_team == 0 || source_team != candidate_team;
                    if hostile && distance <= ATTACK_RANGE && distance < best_distance {
                        target = Some(*candidate_id);
                        best_distance = distance;
                    }
                }
                if let Some(target_id) = target {
                    self.apply_damage(source_id, target_id, BASE_DAMAGE)?;
                }
            }
            ActionKind::AttackTarget => {
                let Some(target_id) = action.target_entity_id else {
                    return Err(StdbError::ReducerError(
                        "AttackTarget requires target_entity_id".into(),
                    ));
                };
                if target_id == source_id {
                    return Ok(());
                }
                let Some(target) = self.entities.get(&target_id) else {
                    return Err(StdbError::ReducerError(format!(
                        "Attack target entity {target_id} missing"
                    )));
                };
                let Some((tx, ty)) = target.position() else {
                    return Ok(());
                };
                let dx = tx - source_pos.0;
                let dy = ty - source_pos.1;
                let distance = (dx * dx + dy * dy).sqrt();
                let target_team = target.team_id.unwrap_or(0);
                let hostile = source_team == 0 || target_team == 0 || source_team != target_team;
                if hostile && distance <= ATTACK_RANGE {
                    self.apply_damage(source_id, target_id, BASE_DAMAGE)?;
                }
            }
            ActionKind::CaptureCreature => {
                let Some(target_id) = action.target_entity_id else {
                    return Err(StdbError::ReducerError(
                        "CaptureCreature requires target_entity_id".into(),
                    ));
                };
                self.receive_world_event(
                    self.current_tick_value(),
                    WorldEventKind::AbilityUsed,
                    source_id,
                    Some(target_id),
                    format!(
                        r#"{{"type":"capture_creature","tool_slot":{}}}"#,
                        action
                            .ability_slot
                            .map(|slot| slot.to_string())
                            .unwrap_or_else(|| "null".to_string())
                    ),
                );
            }
            ActionKind::SummonCompanion => {
                self.receive_world_event(
                    self.current_tick_value(),
                    WorldEventKind::AbilityUsed,
                    source_id,
                    None,
                    format!(
                        r#"{{"type":"summon_companion","slot":{}}}"#,
                        action.ability_slot.unwrap_or(0)
                    ),
                );
            }
            ActionKind::CommandCompanion => {
                let command = action
                    .signal_type
                    .clone()
                    .unwrap_or_else(|| "follow".to_string())
                    .replace('"', "\\\"");
                let target_id = action
                    .target_entity_id
                    .map(|target| target.to_string())
                    .unwrap_or_else(|| "null".to_string());
                self.receive_world_event(
                    self.current_tick_value(),
                    WorldEventKind::AbilityUsed,
                    source_id,
                    action.target_entity_id,
                    format!(
                        r#"{{"type":"command_companion","slot":{},"command":"{}","target":{}}}"#,
                        action.ability_slot.unwrap_or(0),
                        command,
                        target_id
                    ),
                );
            }
            ActionKind::GatherResource => {
                let Some(target_id) = action.target_entity_id else {
                    return Err(StdbError::ReducerError(
                        "GatherResource requires target_entity_id".into(),
                    ));
                };
                let skill = action
                    .signal_type
                    .clone()
                    .unwrap_or_else(|| "gather".to_string())
                    .replace('"', "\\\"");
                self.receive_world_event(
                    self.current_tick_value(),
                    WorldEventKind::AbilityUsed,
                    source_id,
                    Some(target_id),
                    format!(r#"{{"type":"gather_resource","skill":"{}"}}"#, skill),
                );
            }
            ActionKind::Loot => {
                let Some(target_id) = action.target_entity_id else {
                    return Err(StdbError::ReducerError(
                        "Loot requires target_entity_id".into(),
                    ));
                };
                self.receive_world_event(
                    self.current_tick_value(),
                    WorldEventKind::ItemPickedUp,
                    source_id,
                    Some(target_id),
                    r#"{"type":"loot"}"#.to_string(),
                );
            }
            ActionKind::SetAutoRetaliate => {
                let enabled = action.signal_data.as_deref() == Some("true");
                self.receive_world_event(
                    self.current_tick_value(),
                    WorldEventKind::AbilityUsed,
                    source_id,
                    None,
                    format!(r#"{{"type":"set_auto_retaliate","enabled":{enabled}}}"#),
                );
            }
            ActionKind::Speak => {
                if let (Some(message), Some(volume)) =
                    (action.message.clone(), action.volume.clone())
                {
                    self.receive_speech_event(
                        self.current_tick_value(),
                        source_id,
                        message,
                        volume,
                    );
                }
            }
            ActionKind::Interact => {
                let mut target: Option<u64> = None;
                let mut best_distance = f32::MAX;
                for (candidate_id, candidate) in &self.entities {
                    if *candidate_id == source_id || !candidate.alive {
                        continue;
                    }
                    let Some((tx, ty)) = candidate.position() else {
                        continue;
                    };
                    let dx = tx - source_pos.0;
                    let dy = ty - source_pos.1;
                    let distance = (dx * dx + dy * dy).sqrt();
                    if distance <= INTERACT_RANGE && distance < best_distance {
                        best_distance = distance;
                        target = Some(*candidate_id);
                    }
                }
                if let Some(target_id) = target {
                    self.receive_world_event(
                        self.current_tick_value(),
                        WorldEventKind::InteractionTriggered,
                        source_id,
                        Some(target_id),
                        "{}".to_string(),
                    );
                }
            }
            ActionKind::InteractWith => {
                if let Some(target_id) = action.target_entity_id {
                    if let Some(target) = self.entities.get(&target_id) {
                        if let Some((tx, ty)) = target.position() {
                            let dx = tx - source_pos.0;
                            let dy = ty - source_pos.1;
                            if (dx * dx + dy * dy).sqrt() <= INTERACT_RANGE {
                                self.receive_world_event(
                                    self.current_tick_value(),
                                    WorldEventKind::InteractionTriggered,
                                    source_id,
                                    Some(target_id),
                                    "{}".to_string(),
                                );
                            }
                        }
                    }
                }
            }
            ActionKind::Pickup => {
                if let Some(target_id) = action.target_entity_id {
                    self.receive_world_event(
                        self.current_tick_value(),
                        WorldEventKind::ItemPickedUp,
                        source_id,
                        Some(target_id),
                        "{}".to_string(),
                    );
                }
            }
            ActionKind::Drop => {
                self.receive_world_event(
                    self.current_tick_value(),
                    WorldEventKind::ItemDropped,
                    source_id,
                    None,
                    format!(r#"{{"slot":{}}}"#, action.ability_slot.unwrap_or(0)),
                );
            }
            ActionKind::UseItem | ActionKind::UseAbility | ActionKind::Signal => {
                self.receive_world_event(
                    self.current_tick_value(),
                    WorldEventKind::AbilityUsed,
                    source_id,
                    action.target_entity_id,
                    action
                        .signal_data
                        .clone()
                        .unwrap_or_else(|| "{}".to_string()),
                );
            }
            ActionKind::Spawn => {
                if let (Some(x), Some(y)) = (action.target_x, action.target_y) {
                    self.call_spawn_entity(x, y, None)?;
                    self.receive_world_event(
                        self.current_tick_value(),
                        WorldEventKind::EntitySpawned,
                        source_id,
                        None,
                        action.prefab.clone().unwrap_or_default(),
                    );
                }
            }
            ActionKind::Idle => {}
        }

        Ok(())
    }

    fn apply_damage(
        &mut self,
        attacker_id: u64,
        target_id: u64,
        base_damage: f32,
    ) -> Result<(), StdbError> {
        let tick = self.current_tick_value();
        let Some(target) = self.entities.get_mut(&target_id) else {
            return Err(StdbError::ReducerError(format!(
                "Target entity {target_id} missing"
            )));
        };

        let current = target.health.unwrap_or(0.0);
        let armor = target.armor.unwrap_or(0.0);
        if target.invulnerable.unwrap_or(false) {
            return Ok(());
        }

        let damage = (base_damage - armor).max(0.0);
        if damage <= 0.0 {
            return Ok(());
        }

        let remaining = (current - damage).max(0.0);
        target.health = Some(remaining);
        target.alive = remaining > 0.0;

        self.receive_combat_event(tick, attacker_id, target_id, damage, remaining <= 0.0);
        self.events.push_back(StdbEvent::EntityUpdated {
            entity_id: target_id,
        });
        if remaining <= 0.0 {
            self.receive_world_event(
                tick,
                WorldEventKind::EntityDied,
                target_id,
                Some(attacker_id),
                "{}".to_string(),
            );
        }
        Ok(())
    }

    // ── Internal helpers ──

    fn require_connected(&self) -> Result<(), StdbError> {
        if !self.is_connected() {
            return Err(StdbError::NotConnected);
        }
        Ok(())
    }

    // ── Cache mutation (called from SpacetimeDB callbacks) ──

    /// Update the cached world state. Called from subscription callbacks.
    pub fn update_world_state(&mut self, ws: CachedWorldState) {
        let old_tick = self.world_state.as_ref().map(|w| w.tick);
        let new_tick = ws.tick;

        self.events.push_back(StdbEvent::WorldStateUpdated {
            tick: ws.tick,
            paused: ws.paused,
            world_width: ws.world_width,
            world_height: ws.world_height,
        });

        if let Some(old) = old_tick {
            if new_tick != old {
                self.events.push_back(StdbEvent::TickAdvanced {
                    old_tick: old,
                    new_tick,
                });
            }
        }

        self.world_state = Some(ws);
        self.events_received += 1;
    }

    /// Insert or update a cached entity. Called from subscription callbacks.
    pub fn upsert_entity(&mut self, entity: CachedEntity) {
        let eid = entity.entity_id;
        let is_new = !self.entities.contains_key(&eid);
        self.entities.insert(eid, entity);

        if is_new {
            self.events
                .push_back(StdbEvent::EntityInserted { entity_id: eid });
        } else {
            self.events
                .push_back(StdbEvent::EntityUpdated { entity_id: eid });
        }
        self.events_received += 1;
    }

    /// Remove a cached entity. Called from subscription callbacks.
    pub fn remove_entity(&mut self, entity_id: u64) {
        self.entities.remove(&entity_id);
        self.observations.remove(&entity_id);
        self.observation_ticks.remove(&entity_id);
        self.events
            .push_back(StdbEvent::EntityDeleted { entity_id });
        self.events_received += 1;
    }

    /// Store a received observation. Called from subscription callbacks.
    pub fn receive_observation(
        &mut self,
        tick: u64,
        observer_entity_id: u64,
        observation_json: String,
    ) {
        self.observations
            .insert(observer_entity_id, observation_json.clone());
        self.observation_ticks.insert(observer_entity_id, tick);
        self.events.push_back(StdbEvent::ObservationReceived {
            tick,
            observer_entity_id,
            observation_json,
        });
        self.events_received += 1;
    }

    /// Store a received combat event. Called from subscription callbacks.
    pub fn receive_combat_event(
        &mut self,
        tick: u64,
        attacker_id: u64,
        defender_id: u64,
        damage: f32,
        killed: bool,
    ) {
        self.events.push_back(StdbEvent::CombatEventReceived {
            tick,
            attacker_id,
            defender_id,
            damage,
            killed,
        });
        self.events_received += 1;
    }

    /// Store a received speech event. Called from subscription callbacks.
    pub fn receive_speech_event(
        &mut self,
        tick: u64,
        speaker_id: u64,
        message: String,
        volume: SpeakVolume,
    ) {
        self.events.push_back(StdbEvent::SpeechEventReceived {
            tick,
            speaker_id,
            message,
            volume,
        });
        self.events_received += 1;
    }

    /// Store a received world event. Called from subscription callbacks.
    pub fn receive_world_event(
        &mut self,
        tick: u64,
        event_kind: WorldEventKind,
        entity_id: u64,
        secondary_entity_id: Option<u64>,
        data_json: String,
    ) {
        self.events.push_back(StdbEvent::WorldEventReceived {
            tick,
            event_kind,
            entity_id,
            secondary_entity_id,
            data_json,
        });
        self.events_received += 1;
    }

    /// Store a received per-agent telemetry row for debug/editor subscriptions.
    pub fn receive_agent_telemetry_tick(
        &mut self,
        tick: u64,
        agent_entity_id: u64,
        frame_json: String,
    ) {
        self.events
            .push_back(StdbEvent::AgentTelemetryTickReceived {
                tick,
                agent_entity_id,
                frame_json,
            });
        self.events_received += 1;
    }

    /// Store a received tool/provider telemetry row for debug/editor subscriptions.
    pub fn receive_agent_tool_call_event(
        &mut self,
        tick: u64,
        agent_entity_id: u64,
        tool_name: String,
        provider: String,
        status: String,
        document: String,
    ) {
        self.events
            .push_back(StdbEvent::AgentToolCallEventReceived {
                tick,
                agent_entity_id,
                tool_name,
                provider,
                status,
                document,
            });
        self.events_received += 1;

        if let Some(summary_document) =
            self.update_focused_summary_from_tool_call_document(agent_entity_id)
        {
            self.events
                .push_back(StdbEvent::FocusedEntityDebugSummaryReceived {
                    agent_entity_id,
                    document: summary_document,
                });
            self.events_received += 1;
        }
    }

    /// Store a received aggregate telemetry rollup for debug/editor subscriptions.
    pub fn receive_agent_tick_rollup(
        &mut self,
        tick_start: u64,
        tick_end: u64,
        agent_entity_id: u64,
        document: String,
    ) {
        self.events.push_back(StdbEvent::AgentTickRollupReceived {
            tick_start,
            tick_end,
            agent_entity_id,
            document,
        });
        self.events_received += 1;

        if let Some(summary_document) =
            self.update_focused_summary_from_rollup_document(agent_entity_id)
        {
            self.events
                .push_back(StdbEvent::FocusedEntityDebugSummaryReceived {
                    agent_entity_id,
                    document: summary_document,
                });
            self.events_received += 1;
        }
    }

    fn focused_debug_summary_entry(&mut self, entity_id: u64) -> &mut FocusedEntityDebugSummary {
        let shard_id = self.config.db_name.clone();
        self.focused_debug_summaries
            .entry(entity_id)
            .or_insert_with(|| FocusedEntityDebugSummary {
                shard_id,
                entity_id,
                ..Default::default()
            })
    }

    fn refresh_focused_summary_notes(summary: &mut FocusedEntityDebugSummary) {
        summary.notes.clear();
        if summary.tool_error_count > 0 {
            summary.notes.push(format!(
                "{} tool-call errors retained",
                summary.tool_error_count
            ));
        }
        if summary.rejected_action_count > 0 {
            summary.notes.push(format!(
                "{} rejected actions retained",
                summary.rejected_action_count
            ));
        }
    }

    fn update_focused_summary_from_tool_call_document(
        &mut self,
        agent_entity_id: u64,
    ) -> Option<String> {
        let tool_document = self.events.iter().rev().find_map(|event| match event {
            StdbEvent::AgentToolCallEventReceived {
                agent_entity_id: current_entity_id,
                document,
                ..
            } if *current_entity_id == agent_entity_id => Some(document.as_str()),
            _ => None,
        })?;
        let event: AgentToolCallEvent = match decode_toon_document(
            tool_document,
            "agent_tool_call_event",
        ) {
            Ok(event) => event,
            Err(error) => {
                log::warn!(
                        "[pod-stdb client] Failed to decode agent_tool_call_event for focused summary synthesis: {error}"
                    );
                return None;
            }
        };

        let summary = self.focused_debug_summary_entry(event.agent_entity_id);
        let prior_tool_count = summary.tool_call_count;
        summary.latest_tick = summary.latest_tick.max(event.tick);
        summary.tool_call_count += 1;
        summary.average_tool_latency_ms = if prior_tool_count == 0 {
            event.trace.latency_ms as f32
        } else {
            ((summary.average_tool_latency_ms * prior_tool_count as f32)
                + event.trace.latency_ms as f32)
                / summary.tool_call_count as f32
        };
        summary.latest_tool_name = Some(event.trace.tool_name.clone());
        summary.latest_tool_status = Some(format!("{:?}", event.trace.status));
        summary.latest_tool_error = event.trace.error_message.clone();
        if !matches!(
            event.trace.status,
            ToolCallStatus::Requested | ToolCallStatus::Succeeded
        ) {
            summary.tool_error_count += 1;
        }
        Self::refresh_focused_summary_notes(summary);
        Some(summary.to_toon_document())
    }

    fn update_focused_summary_from_rollup_document(
        &mut self,
        agent_entity_id: u64,
    ) -> Option<String> {
        let rollup_document = self.events.iter().rev().find_map(|event| match event {
            StdbEvent::AgentTickRollupReceived {
                agent_entity_id: current_entity_id,
                document,
                ..
            } if *current_entity_id == agent_entity_id => Some(document.as_str()),
            _ => None,
        })?;
        let rollup: AgentTickRollup = match decode_toon_document(
            rollup_document,
            "agent_tick_rollup",
        ) {
            Ok(rollup) => rollup,
            Err(error) => {
                log::warn!(
                    "[pod-stdb client] Failed to decode agent_tick_rollup for focused summary synthesis: {error}"
                );
                return None;
            }
        };

        let summary = self.focused_debug_summary_entry(rollup.agent_entity_id);
        summary.latest_tick = summary.latest_tick.max(rollup.tick_end);
        summary.tool_call_count = rollup.tool_call_count as usize;
        summary.tool_error_count = rollup.tool_error_count as usize;
        summary.rejected_action_count = rollup.rejected_action_count as usize;
        summary.total_distance = rollup.total_distance;
        summary.average_tool_latency_ms = rollup.average_tool_latency_ms;
        summary.visible_entity_count = rollup.visible_entity_count as usize;
        summary.audible_event_count = rollup.audible_event_count as usize;
        summary.message_count = rollup.message_count as usize;
        Self::refresh_focused_summary_notes(summary);
        Some(summary.to_toon_document())
    }
}

fn resolve_topology_world_id(db_name: &str, topology: &RemoteTopologyBundle) -> Option<String> {
    if topology
        .worlds
        .iter()
        .any(|world| world.world_id == db_name)
    {
        return Some(db_name.to_string());
    }

    if topology.worlds.len() == 1 {
        return topology.worlds.first().map(|world| world.world_id.clone());
    }

    None
}

impl std::fmt::Debug for StdbClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdbClient")
            .field("host", &self.config.host)
            .field("db_name", &self.config.db_name)
            .field("state", &self.state)
            .field("generated_runtime_wired", &self.generated_runtime.is_some())
            .field("entities_cached", &self.entities.len())
            .field("observations_cached", &self.observations.len())
            .field("events_pending", &self.events.len())
            .field("frames_processed", &self.frames_processed)
            .finish()
    }
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_new() {
        let client = StdbClient::new(StdbClientConfig::default());
        assert!(!client.is_connected());
        assert!(client.world_state().is_none());
        assert_eq!(client.entity_count(), 0);
        assert_eq!(client.frames_processed(), 0);
    }

    #[test]
    fn test_connection_state_transitions() {
        let mut client = StdbClient::new(StdbClientConfig {
            connection_mode: StdbConnectionMode::Emulated,
            ..StdbClientConfig::default()
        });
        assert!(matches!(
            client.connection_state(),
            ConnectionState::Disconnected
        ));

        client.connect().unwrap();
        assert!(matches!(
            client.connection_state(),
            ConnectionState::Connecting
        ));

        // Double connect should error
        assert!(client.connect().is_err());
    }

    #[test]
    fn test_generated_mode_connect_requires_runtime() {
        let mut client = StdbClient::new(StdbClientConfig {
            connection_mode: StdbConnectionMode::Generated,
            ..StdbClientConfig::default()
        });
        let err = client
            .connect()
            .expect_err("generated mode requires runtime wiring");
        assert!(matches!(err, StdbError::ConnectionFailed(_)));
        assert!(matches!(
            client.connection_state(),
            ConnectionState::Error(_)
        ));
    }

    #[test]
    fn test_generated_sdk_runtime_uses_live_bindings_path() {
        let mut client = StdbClient::new(StdbClientConfig {
            host: "http://127.0.0.1:1".into(),
            connection_mode: StdbConnectionMode::Generated,
            ..StdbClientConfig::default()
        });
        client.install_generated_sdk_runtime();

        let err = client
            .connect()
            .expect_err("closed localhost port should fail the real generated SDK connection");
        assert!(matches!(err, StdbError::ConnectionFailed(_)));
    }

    #[test]
    fn test_generated_mode_connect_and_subscription_use_runtime_adapter() {
        let mut client = StdbClient::new(StdbClientConfig {
            connection_mode: StdbConnectionMode::Generated,
            ..StdbClientConfig::default()
        });
        let (bridge, callbacks, trace) =
            build_generated_runtime_callback_bridge(vec![1, 2, 3, 4], "tok-generated");
        client.set_generated_runtime_bridge(bridge);

        client.connect().expect("generated runtime should connect");
        assert!(matches!(
            client.connection_state(),
            ConnectionState::Connecting
        ));
        assert_eq!(trace.connect_configs().len(), 1);
        client.frame_tick();
        assert!(client.is_connected());

        client
            .subscribe(vec!["SELECT * FROM remote_topology_document".into()])
            .expect("generated runtime should accept subscriptions");
        assert_eq!(
            trace.subscription_queries().as_slice(),
            vec![vec!["SELECT * FROM remote_topology_document".to_string()]]
        );
        client.frame_tick();

        callbacks.disconnected("test teardown");
        client.frame_tick();

        let events = client.drain_events().collect::<Vec<_>>();
        assert!(events.iter().any(|event| matches!(
            event,
            StdbEvent::Connected { token, .. } if token == "tok-generated"
        )));
        assert!(events
            .iter()
            .any(|event| matches!(event, StdbEvent::SubscriptionApplied)));
    }

    #[test]
    fn test_generated_runtime_remote_topology_row_updates_client_state() {
        let mut client = StdbClient::new(StdbClientConfig {
            connection_mode: StdbConnectionMode::Generated,
            db_name: "deadman-shadow".into(),
            ..StdbClientConfig::default()
        });
        let (bridge, callbacks, _trace) =
            build_generated_runtime_callback_bridge(vec![9; 16], "tok-generated");
        client.set_generated_runtime_bridge(bridge);
        client.connect().expect("generated runtime should connect");
        client.frame_tick();
        client
            .subscribe(vec!["SELECT * FROM remote_topology_document".into()])
            .expect("generated runtime should accept subscriptions");

        let topology = pod_core::RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "generated-test".into(),
            generated_at_unix_ms: 42,
            tournament: pod_core::WorldTournamentDefinition::new(
                "deadman-neural-cup",
                "Deadman Neural Cup",
            ),
            teams: vec![pod_core::AgentTeamDefinition::new(
                "gloam-mesh",
                "Gloam Mesh",
                "deadman-shadow",
            )],
            worlds: vec![{
                let mut world = pod_core::WorldRealityDefinition::new(
                    "deadman-shadow",
                    "Deadman Shadow",
                    "shadow",
                );
                world.role = pod_core::WorldRealityRole::Shadow;
                world.active_team_ids = vec!["gloam-mesh".into()];
                world
            }],
            links: vec![],
            world_quest_bindings: vec![pod_core::WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-hunt".into()],
            }],
            world_admissions: vec![],
            quest_graphs: vec![],
            applied_world_states: vec![],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![pod_core::WorldEvaluationSummary {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: pod_core::WorldRealityRole::Shadow,
                    average_reward_per_row: 4.5,
                    controller_mix: vec![pod_core::ControllerEvaluationSummary {
                        agent_type: "neural_agent".into(),
                        row_count: 3,
                        reward_total: 13.5,
                        average_reward_per_row: 4.5,
                    }],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 6666,
                    applied_score_delta_total: 0,
                    applied_death_mark_count: 0,
                    applied_death_mark_ticks: 0,
                    applied_objective_shift_count: 0,
                    applied_reputation_delta_total: 0,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        };
        let document = topology.to_toon_document();

        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(7, &topology)
                .expect("callback row should build"),
        );
        client.frame_tick();

        assert_eq!(client.resolved_remote_world_id(), Some("deadman-shadow"));
        assert_eq!(
            client
                .resolved_remote_world_evaluation()
                .and_then(|world| world.controller_mix.first())
                .map(|controller| controller.agent_type.as_str()),
            Some("neural_agent")
        );

        let events = client.drain_events().collect::<Vec<_>>();
        assert!(events.iter().any(|event| matches!(
            event,
            StdbEvent::RemoteTopologyDocumentReceived { document: current }
                if current == &document
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            StdbEvent::RemoteTopologyUpdated {
                resolved_world_id,
                ..
            } if resolved_world_id.as_deref() == Some("deadman-shadow")
        )));
    }

    #[test]
    fn test_generated_runtime_bridge_records_connect_and_subscription_callbacks() {
        let (bridge, callbacks, trace) =
            build_generated_runtime_callback_bridge(vec![4; 16], "bridge-token");

        let mut client = StdbClient::new(StdbClientConfig {
            connection_mode: StdbConnectionMode::Generated,
            ..StdbClientConfig::default()
        });
        client.set_generated_runtime_bridge(bridge);
        client.connect().expect("generated bridge should connect");
        client.frame_tick();
        client
            .subscribe(vec!["SELECT * FROM remote_topology_document".into()])
            .expect("bridge subscriptions should apply");
        client.frame_tick();

        assert_eq!(trace.connect_configs().len(), 1);
        assert_eq!(
            trace.subscription_queries(),
            vec![vec!["SELECT * FROM remote_topology_document".to_string()]]
        );
        client.disconnect();
        assert_eq!(trace.disconnect_count(), 1);
        callbacks.disconnected("bridge closed");
        client.frame_tick();

        let events = client.drain_events().collect::<Vec<_>>();
        assert!(events.iter().any(|event| matches!(
            event,
            StdbEvent::Connected { token, .. } if token == "bridge-token"
        )));
        assert!(events
            .iter()
            .any(|event| matches!(event, StdbEvent::SubscriptionApplied)));
        assert!(events.iter().any(|event| matches!(
            event,
            StdbEvent::Disconnected { reason } if reason == "bridge closed"
        )));
    }

    #[test]
    fn test_generated_runtime_bridge_updates_same_world_quest_and_effect_state() {
        let (bridge, callbacks, _trace) =
            build_generated_runtime_callback_bridge(vec![8; 16], "bridge-generated");
        let mut client = StdbClient::new(StdbClientConfig {
            connection_mode: StdbConnectionMode::Generated,
            db_name: "deadman-shadow".into(),
            ..StdbClientConfig::default()
        });
        client.set_generated_runtime_bridge(bridge);
        client
            .connect()
            .expect("bridge-generated runtime should connect");
        client.frame_tick();
        client
            .subscribe(vec!["SELECT * FROM remote_topology_document".into()])
            .expect("bridge-generated subscriptions should apply");

        let initial = pod_core::RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "generated-test".into(),
            generated_at_unix_ms: 200,
            tournament: pod_core::WorldTournamentDefinition::new(
                "deadman-neural-cup",
                "Deadman Neural Cup",
            ),
            teams: vec![pod_core::AgentTeamDefinition::new(
                "gloam-mesh",
                "Gloam Mesh",
                "deadman-shadow",
            )],
            worlds: vec![{
                let mut world = pod_core::WorldRealityDefinition::new(
                    "deadman-shadow",
                    "Deadman Shadow",
                    "shadow",
                );
                world.role = pod_core::WorldRealityRole::Shadow;
                world.active_team_ids = vec!["gloam-mesh".into()];
                world
            }],
            links: vec![],
            world_quest_bindings: vec![pod_core::WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-hunt".into()],
            }],
            world_admissions: vec![],
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: pod_core::WorldRealityRole::Shadow,
                team_scores: vec![pod_core::TeamDeltaSummary {
                    team_id: "gloam-mesh".into(),
                    total_delta: 3,
                }],
                death_marks: vec![],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![pod_core::ObjectiveShiftSummary {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    stage_tag: "marked-by-kills".into(),
                    applications: 1,
                }],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![pod_core::QuestLineStateSummary {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    display_name: "Deadman Shadow: Mirror Hunt".into(),
                    current_stage_ids: vec!["marked-by-kills".into()],
                    completed_stage_ids: vec!["shadow-observe".into()],
                    pending_stage_ids: vec!["rift-collapse".into()],
                    next_stage_ids: vec!["rift-collapse".into()],
                    progress_basis_points: 5000,
                    terminal: false,
                    stage_applications: vec![pod_core::QuestStageApplicationSummary {
                        stage_id: "marked-by-kills".into(),
                        title: "Marked by Kills".into(),
                        applications: 1,
                    }],
                }],
            }],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![pod_core::WorldEvaluationSummary {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: pod_core::WorldRealityRole::Shadow,
                    average_reward_per_row: 4.5,
                    controller_mix: vec![],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 5000,
                    applied_score_delta_total: 3,
                    applied_death_mark_count: 0,
                    applied_death_mark_ticks: 0,
                    applied_objective_shift_count: 1,
                    applied_reputation_delta_total: 0,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        };
        let updated = pod_core::RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "generated-test".into(),
            generated_at_unix_ms: 260,
            tournament: pod_core::WorldTournamentDefinition::new(
                "deadman-neural-cup",
                "Deadman Neural Cup",
            ),
            teams: vec![pod_core::AgentTeamDefinition::new(
                "gloam-mesh",
                "Gloam Mesh",
                "deadman-shadow",
            )],
            worlds: vec![{
                let mut world = pod_core::WorldRealityDefinition::new(
                    "deadman-shadow",
                    "Deadman Shadow",
                    "shadow",
                );
                world.role = pod_core::WorldRealityRole::Shadow;
                world.active_team_ids = vec!["gloam-mesh".into()];
                world
            }],
            links: vec![],
            world_quest_bindings: vec![pod_core::WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-collapse".into()],
            }],
            world_admissions: vec![],
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: pod_core::WorldRealityRole::Shadow,
                team_scores: vec![pod_core::TeamDeltaSummary {
                    team_id: "gloam-mesh".into(),
                    total_delta: 9,
                }],
                death_marks: vec![],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![pod_core::ObjectiveShiftSummary {
                    quest_graph_id: "deadman-shadow-collapse".into(),
                    stage_tag: "rift-collapse".into(),
                    applications: 4,
                }],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![pod_core::QuestLineStateSummary {
                    quest_graph_id: "deadman-shadow-collapse".into(),
                    display_name: "Deadman Shadow: Collapse".into(),
                    current_stage_ids: vec!["rift-collapse".into()],
                    completed_stage_ids: vec!["shadow-observe".into()],
                    pending_stage_ids: vec!["echo-resolve".into()],
                    next_stage_ids: vec!["echo-resolve".into()],
                    progress_basis_points: 7500,
                    terminal: false,
                    stage_applications: vec![pod_core::QuestStageApplicationSummary {
                        stage_id: "rift-collapse".into(),
                        title: "Rift Collapse".into(),
                        applications: 4,
                    }],
                }],
            }],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![pod_core::WorldEvaluationSummary {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: pod_core::WorldRealityRole::Shadow,
                    average_reward_per_row: 6.25,
                    controller_mix: vec![],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 7500,
                    applied_score_delta_total: 9,
                    applied_death_mark_count: 0,
                    applied_death_mark_ticks: 0,
                    applied_objective_shift_count: 4,
                    applied_reputation_delta_total: 0,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        };

        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(21, &initial)
                .expect("initial callback row should build"),
        );
        client.frame_tick();

        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(22, &updated)
                .expect("updated callback row should build"),
        );
        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(20, &initial)
                .expect("stale callback row should build"),
        );
        client.frame_tick();

        assert_eq!(client.resolved_remote_world_id(), Some("deadman-shadow"));
        assert_eq!(
            client
                .resolved_remote_world_quest_binding()
                .map(|binding| binding.quest_graph_ids.as_slice()),
            Some(["deadman-shadow-collapse".to_string()].as_slice())
        );
        assert_eq!(
            client
                .resolved_remote_applied_world_state()
                .and_then(|state| state.quest_lines.first())
                .map(|quest| quest.quest_graph_id.as_str()),
            Some("deadman-shadow-collapse")
        );
        assert_eq!(
            client
                .resolved_remote_world_evaluation()
                .map(|world| world.average_reward_per_row),
            Some(6.25)
        );
        assert_eq!(
            client
                .resolved_remote_world_evaluation()
                .map(|world| world.applied_objective_shift_count),
            Some(4)
        );
    }

    #[test]
    fn test_generated_runtime_bridge_updates_linked_world_quest_and_effect_state() {
        let (bridge, callbacks, _trace) =
            build_generated_runtime_callback_bridge(vec![10; 16], "bridge-generated");
        let mut client = StdbClient::new(StdbClientConfig {
            connection_mode: StdbConnectionMode::Generated,
            db_name: "deadman-shadow".into(),
            ..StdbClientConfig::default()
        });
        client.set_generated_runtime_bridge(bridge);
        client
            .connect()
            .expect("bridge-generated runtime should connect");
        client.frame_tick();
        client
            .subscribe(vec!["SELECT * FROM remote_topology_document".into()])
            .expect("bridge-generated subscriptions should apply");

        let initial = pod_core::RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "generated-test".into(),
            generated_at_unix_ms: 300,
            tournament: pod_core::WorldTournamentDefinition::new(
                "deadman-neural-cup",
                "Deadman Neural Cup",
            ),
            teams: vec![
                pod_core::AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime"),
                pod_core::AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow"),
            ],
            worlds: vec![
                {
                    let mut world = pod_core::WorldRealityDefinition::new(
                        "deadman-prime",
                        "Deadman Prime",
                        "tournament",
                    );
                    world.role = pod_core::WorldRealityRole::Tournament;
                    world.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];
                    world
                },
                {
                    let mut world = pod_core::WorldRealityDefinition::new(
                        "deadman-shadow",
                        "Deadman Shadow",
                        "shadow",
                    );
                    world.role = pod_core::WorldRealityRole::Shadow;
                    world.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];
                    world.linked_world_ids = vec!["deadman-prime".into()];
                    world
                },
            ],
            links: vec![],
            world_quest_bindings: vec![
                pod_core::WorldQuestBinding {
                    world_id: "deadman-prime".into(),
                    quest_graph_ids: vec!["deadman-prime-season".into()],
                },
                pod_core::WorldQuestBinding {
                    world_id: "deadman-shadow".into(),
                    quest_graph_ids: vec!["deadman-shadow-hunt".into()],
                },
            ],
            world_admissions: vec![],
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: pod_core::WorldRealityRole::Shadow,
                team_scores: vec![pod_core::TeamDeltaSummary {
                    team_id: "iron-sigil".into(),
                    total_delta: 10,
                }],
                death_marks: vec![pod_core::TeamDeathMarkSummary {
                    team_id: "gloam-mesh".into(),
                    applications: 2,
                    total_duration_ticks: 1200,
                }],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![pod_core::ObjectiveShiftSummary {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    stage_tag: "marked-by-kills".into(),
                    applications: 2,
                }],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![pod_core::QuestLineStateSummary {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    display_name: "Deadman Shadow: Mirror Hunt".into(),
                    current_stage_ids: vec!["marked-by-kills".into()],
                    completed_stage_ids: vec!["shadow-observe".into()],
                    pending_stage_ids: vec!["rift-collapse".into()],
                    next_stage_ids: vec!["rift-collapse".into()],
                    progress_basis_points: 6666,
                    terminal: false,
                    stage_applications: vec![pod_core::QuestStageApplicationSummary {
                        stage_id: "marked-by-kills".into(),
                        title: "Marked by Kills".into(),
                        applications: 2,
                    }],
                }],
            }],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![pod_core::WorldEvaluationSummary {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: pod_core::WorldRealityRole::Shadow,
                    average_reward_per_row: 4.5,
                    controller_mix: vec![pod_core::ControllerEvaluationSummary {
                        agent_type: "neural_agent".into(),
                        row_count: 3,
                        reward_total: 13.5,
                        average_reward_per_row: 4.5,
                    }],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 6666,
                    applied_score_delta_total: 10,
                    applied_death_mark_count: 2,
                    applied_death_mark_ticks: 1200,
                    applied_objective_shift_count: 2,
                    applied_reputation_delta_total: 0,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        };
        let updated = pod_core::RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "generated-test".into(),
            generated_at_unix_ms: 360,
            tournament: pod_core::WorldTournamentDefinition::new(
                "deadman-neural-cup",
                "Deadman Neural Cup",
            ),
            teams: vec![
                pod_core::AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime"),
                pod_core::AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow"),
            ],
            worlds: vec![
                {
                    let mut world = pod_core::WorldRealityDefinition::new(
                        "deadman-prime",
                        "Deadman Prime",
                        "tournament",
                    );
                    world.role = pod_core::WorldRealityRole::Tournament;
                    world.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];
                    world
                },
                {
                    let mut world = pod_core::WorldRealityDefinition::new(
                        "deadman-shadow",
                        "Deadman Shadow",
                        "shadow",
                    );
                    world.role = pod_core::WorldRealityRole::Shadow;
                    world.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];
                    world.linked_world_ids = vec!["deadman-prime".into()];
                    world
                },
            ],
            links: vec![],
            world_quest_bindings: vec![
                pod_core::WorldQuestBinding {
                    world_id: "deadman-prime".into(),
                    quest_graph_ids: vec!["deadman-prime-season".into()],
                },
                pod_core::WorldQuestBinding {
                    world_id: "deadman-shadow".into(),
                    quest_graph_ids: vec!["deadman-shadow-collapse".into()],
                },
            ],
            world_admissions: vec![],
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: pod_core::WorldRealityRole::Shadow,
                team_scores: vec![pod_core::TeamDeltaSummary {
                    team_id: "iron-sigil".into(),
                    total_delta: 14,
                }],
                death_marks: vec![pod_core::TeamDeathMarkSummary {
                    team_id: "gloam-mesh".into(),
                    applications: 3,
                    total_duration_ticks: 1800,
                }],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![pod_core::ObjectiveShiftSummary {
                    quest_graph_id: "deadman-shadow-collapse".into(),
                    stage_tag: "rift-collapse".into(),
                    applications: 5,
                }],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![pod_core::QuestLineStateSummary {
                    quest_graph_id: "deadman-shadow-collapse".into(),
                    display_name: "Deadman Shadow: Collapse".into(),
                    current_stage_ids: vec!["rift-collapse".into()],
                    completed_stage_ids: vec!["shadow-observe".into()],
                    pending_stage_ids: vec!["echo-resolve".into()],
                    next_stage_ids: vec!["echo-resolve".into()],
                    progress_basis_points: 8333,
                    terminal: false,
                    stage_applications: vec![pod_core::QuestStageApplicationSummary {
                        stage_id: "rift-collapse".into(),
                        title: "Rift Collapse".into(),
                        applications: 5,
                    }],
                }],
            }],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![pod_core::WorldEvaluationSummary {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: pod_core::WorldRealityRole::Shadow,
                    average_reward_per_row: 6.75,
                    controller_mix: vec![pod_core::ControllerEvaluationSummary {
                        agent_type: "neural_agent".into(),
                        row_count: 4,
                        reward_total: 27.0,
                        average_reward_per_row: 6.75,
                    }],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 8333,
                    applied_score_delta_total: 14,
                    applied_death_mark_count: 3,
                    applied_death_mark_ticks: 1800,
                    applied_objective_shift_count: 5,
                    applied_reputation_delta_total: 0,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        };

        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(41, &initial)
                .expect("initial linked callback row should build"),
        );
        client.frame_tick();

        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(42, &updated)
                .expect("updated linked callback row should build"),
        );
        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(40, &initial)
                .expect("stale linked callback row should build"),
        );
        client.frame_tick();

        assert_eq!(client.resolved_remote_world_id(), Some("deadman-shadow"));
        assert_eq!(
            client
                .resolved_remote_world_quest_binding()
                .map(|binding| binding.quest_graph_ids.as_slice()),
            Some(["deadman-shadow-collapse".to_string()].as_slice())
        );
        assert_eq!(
            client
                .resolved_remote_applied_world_state()
                .and_then(|state| state.quest_lines.first())
                .map(|quest| quest.quest_graph_id.as_str()),
            Some("deadman-shadow-collapse")
        );
        assert_eq!(
            client
                .resolved_remote_applied_world_state()
                .and_then(|state| state.death_marks.first())
                .map(|mark| mark.total_duration_ticks),
            Some(1800)
        );
        assert_eq!(
            client
                .resolved_remote_world_evaluation()
                .map(|world| world.average_reward_per_row),
            Some(6.75)
        );
        assert_eq!(
            client
                .resolved_remote_world_evaluation()
                .map(|world| world.applied_death_mark_count),
            Some(3)
        );
        assert_eq!(
            client
                .resolved_remote_world_evaluation()
                .and_then(|world| world.controller_mix.first())
                .map(|controller| controller.row_count),
            Some(4)
        );
    }

    #[test]
    fn test_entity_cache() {
        let mut client = StdbClient::new(StdbClientConfig::default());

        // Insert entity
        let entity = CachedEntity::from_entity(1, Some(AgentType::Human), true);
        client.upsert_entity(entity);
        assert_eq!(client.entity_count(), 1);
        assert!(client.entity(1).is_some());

        // Update entity
        let mut updated = CachedEntity::from_entity(1, Some(AgentType::Human), true);
        updated.pos_x = Some(100.0);
        updated.pos_y = Some(200.0);
        client.upsert_entity(updated);
        assert_eq!(client.entity_count(), 1);
        assert_eq!(client.entity(1).unwrap().position(), Some((100.0, 200.0)));

        // Remove entity
        client.remove_entity(1);
        assert_eq!(client.entity_count(), 0);
        assert!(client.entity(1).is_none());
    }

    #[test]
    fn test_event_queue() {
        let mut client = StdbClient::new(StdbClientConfig::default());

        client.receive_observation(1, 42, "{\"test\":true}".to_string());
        assert!(client.has_events());
        assert_eq!(client.peek_events().len(), 1);

        let events: Vec<StdbEvent> = client.drain_events().collect();
        assert_eq!(events.len(), 1);
        assert!(!client.has_events());

        // Check observation is cached
        assert_eq!(client.latest_observation(42), Some("{\"test\":true}"));
        assert_eq!(client.latest_observation_tick(42), Some(1));
    }

    #[test]
    fn test_apply_remote_topology_resolves_world_and_team_metadata() {
        let mut client = StdbClient::new(StdbClientConfig {
            db_name: "deadman-prime".into(),
            ..StdbClientConfig::default()
        });
        let mut world = pod_core::WorldRealityDefinition::new(
            "deadman-prime",
            "Deadman Prime",
            "deadman-seasonal",
        );
        world.role = pod_core::WorldRealityRole::Tournament;
        world.active_team_ids = vec!["iron-sigil".into(), "ember-veil".into()];

        client.apply_remote_topology(pod_core::RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 42,
            tournament: pod_core::WorldTournamentDefinition::new(
                "deadman-neural-cup",
                "Deadman Neural Cup",
            ),
            teams: vec![
                pod_core::AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime"),
                pod_core::AgentTeamDefinition::new("ember-veil", "Ember Veil", "deadman-prime"),
            ],
            worlds: vec![world],
            links: vec![],
            world_quest_bindings: vec![pod_core::WorldQuestBinding {
                world_id: "deadman-prime".into(),
                quest_graph_ids: vec!["deadman-prime-season".into()],
            }],
            world_admissions: vec![pod_core::WorldAdmissionSummary {
                world_id: "deadman-prime".into(),
                assignments: vec![pod_core::WorldAdmissionAssignment {
                    agent_id: "agent-a".into(),
                    team_id: "iron-sigil".into(),
                    slot_index: 0,
                }],
            }],
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                role: pod_core::WorldRealityRole::Tournament,
                team_scores: vec![pod_core::TeamDeltaSummary {
                    team_id: "iron-sigil".into(),
                    total_delta: 4,
                }],
                death_marks: vec![],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![],
            }],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![pod_core::WorldEvaluationSummary {
                    world_id: "deadman-prime".into(),
                    display_name: "Deadman Prime".into(),
                    role: pod_core::WorldRealityRole::Tournament,
                    average_reward_per_row: 1.5,
                    controller_mix: vec![],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 6500,
                    applied_score_delta_total: 4,
                    applied_death_mark_count: 0,
                    applied_death_mark_ticks: 0,
                    applied_objective_shift_count: 0,
                    applied_reputation_delta_total: 0,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        });

        assert_eq!(client.resolved_remote_world_id(), Some("deadman-prime"));
        assert_eq!(
            client.resolved_remote_world().map(|world| world.role),
            Some(pod_core::WorldRealityRole::Tournament)
        );
        assert_eq!(
            client.resolved_remote_team_key(Some(1)).as_deref(),
            Some("iron-sigil")
        );
        assert_eq!(
            client.resolved_remote_team_key(Some(2)).as_deref(),
            Some("ember-veil")
        );
        assert_eq!(
            client
                .resolved_remote_world_quest_binding()
                .map(|binding| binding.quest_graph_ids.as_slice()),
            Some(["deadman-prime-season".to_string()].as_slice())
        );
        assert_eq!(
            client
                .resolved_remote_world_admissions()
                .map(|summary| summary.assignments[0].team_id.as_str()),
            Some("iron-sigil")
        );
        assert_eq!(
            client
                .resolved_remote_applied_world_state()
                .and_then(|state| state.team_scores.first())
                .map(|score| (score.team_id.as_str(), score.total_delta)),
            Some(("iron-sigil", 4))
        );
        assert_eq!(
            client
                .resolved_remote_world_evaluation()
                .map(|world| world.average_reward_per_row),
            Some(1.5)
        );

        let event = client
            .drain_events()
            .find(|event| matches!(event, StdbEvent::RemoteTopologyUpdated { .. }))
            .expect("remote topology event emitted");
        match event {
            StdbEvent::RemoteTopologyUpdated {
                scenario_id,
                resolved_world_id,
                world_count,
                team_count,
            } => {
                assert_eq!(scenario_id, "deadman-neural-cup");
                assert_eq!(resolved_world_id.as_deref(), Some("deadman-prime"));
                assert_eq!(world_count, 1);
                assert_eq!(team_count, 2);
            }
            _ => panic!("unexpected event variant"),
        }
    }

    #[test]
    fn test_receive_debug_document_decodes_remote_topology_and_emits_document_event() {
        let mut client = StdbClient::new(StdbClientConfig {
            db_name: "deadman-shadow".into(),
            ..StdbClientConfig::default()
        });
        let mut world =
            pod_core::WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow");
        world.role = pod_core::WorldRealityRole::Shadow;
        world.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];

        let document = pod_core::RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 42,
            tournament: pod_core::WorldTournamentDefinition::new(
                "deadman-neural-cup",
                "Deadman Neural Cup",
            ),
            teams: vec![
                pod_core::AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime"),
                pod_core::AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow"),
            ],
            worlds: vec![world],
            links: vec![],
            world_quest_bindings: vec![pod_core::WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-hunt".into()],
            }],
            world_admissions: vec![],
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: pod_core::WorldRealityRole::Shadow,
                team_scores: vec![],
                death_marks: vec![],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![pod_core::QuestLineStateSummary {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    display_name: "Deadman Shadow: Mirror Hunt".into(),
                    current_stage_ids: vec!["marked-by-kills".into()],
                    completed_stage_ids: vec!["shadow-observe".into()],
                    pending_stage_ids: vec!["rift-collapse".into()],
                    next_stage_ids: vec!["rift-collapse".into()],
                    progress_basis_points: 6666,
                    terminal: false,
                    stage_applications: vec![pod_core::QuestStageApplicationSummary {
                        stage_id: "marked-by-kills".into(),
                        title: "Marked by Kills".into(),
                        applications: 2,
                    }],
                }],
            }],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![pod_core::WorldEvaluationSummary {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: pod_core::WorldRealityRole::Shadow,
                    average_reward_per_row: 4.5,
                    controller_mix: vec![pod_core::ControllerEvaluationSummary {
                        agent_type: "neural_agent".into(),
                        row_count: 3,
                        reward_total: 13.5,
                        average_reward_per_row: 4.5,
                    }],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 6666,
                    applied_score_delta_total: 0,
                    applied_death_mark_count: 0,
                    applied_death_mark_ticks: 0,
                    applied_objective_shift_count: 0,
                    applied_reputation_delta_total: 0,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        }
        .to_toon_document();

        client
            .receive_debug_document(document.clone())
            .expect("document should decode");

        assert_eq!(client.resolved_remote_world_id(), Some("deadman-shadow"));
        assert_eq!(
            client
                .resolved_remote_world_evaluation()
                .and_then(|world| world.controller_mix.first())
                .map(|controller| controller.agent_type.as_str()),
            Some("neural_agent")
        );

        let events = client.drain_events().collect::<Vec<_>>();
        assert!(events.iter().any(|event| matches!(
            event,
            StdbEvent::RemoteTopologyDocumentReceived { document: current }
                if current == &document
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            StdbEvent::RemoteTopologyUpdated {
                resolved_world_id,
                ..
            } if resolved_world_id.as_deref() == Some("deadman-shadow")
        )));
    }

    #[test]
    fn test_receive_remote_topology_document_row_ignores_stale_rows() {
        let mut client = StdbClient::new(StdbClientConfig {
            db_name: "deadman-shadow".into(),
            ..StdbClientConfig::default()
        });
        let mut current_world =
            pod_core::WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow");
        current_world.role = pod_core::WorldRealityRole::Shadow;
        let current = pod_core::RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 200,
            tournament: pod_core::WorldTournamentDefinition::new(
                "deadman-neural-cup",
                "Deadman Neural Cup",
            ),
            teams: vec![],
            worlds: vec![current_world],
            links: vec![],
            world_quest_bindings: vec![],
            world_admissions: vec![],
            quest_graphs: vec![],
            applied_world_states: vec![],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![],
            },
        }
        .to_toon_document();
        let mut stale_world =
            pod_core::WorldRealityDefinition::new("deadman-prime", "Deadman Prime", "tournament");
        stale_world.role = pod_core::WorldRealityRole::Tournament;
        let stale = pod_core::RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 150,
            tournament: pod_core::WorldTournamentDefinition::new(
                "deadman-neural-cup",
                "Deadman Neural Cup",
            ),
            teams: vec![],
            worlds: vec![stale_world],
            links: vec![],
            world_quest_bindings: vec![],
            world_admissions: vec![],
            quest_graphs: vec![],
            applied_world_states: vec![],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![],
            },
        }
        .to_toon_document();

        client
            .receive_remote_topology_document_row(
                10,
                200,
                "deadman-neural-cup".into(),
                "ci-smoke".into(),
                current.clone(),
            )
            .expect("current row should apply");
        client
            .receive_remote_topology_document_row(
                9,
                150,
                "deadman-neural-cup".into(),
                "ci-smoke".into(),
                stale,
            )
            .expect("stale row should be ignored");

        assert_eq!(client.resolved_remote_world_id(), Some("deadman-shadow"));
        let events = client.drain_events().collect::<Vec<_>>();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, StdbEvent::RemoteTopologyUpdated { .. }))
                .count(),
            1
        );
        assert!(events.iter().any(|event| matches!(
            event,
            StdbEvent::RemoteTopologyDocumentReceived { document }
                if document == &current
        )));
    }

    #[test]
    fn test_receive_remote_topology_document_row_updates_quest_and_effect_state_within_same_world()
    {
        let mut client = StdbClient::new(StdbClientConfig {
            db_name: "deadman-shadow".into(),
            ..StdbClientConfig::default()
        });
        let mut world =
            pod_core::WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow");
        world.role = pod_core::WorldRealityRole::Shadow;
        world.active_team_ids = vec!["iron-sigil".into()];

        let initial = pod_core::RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 200,
            tournament: pod_core::WorldTournamentDefinition::new(
                "deadman-neural-cup",
                "Deadman Neural Cup",
            ),
            teams: vec![],
            worlds: vec![world.clone()],
            links: vec![],
            world_quest_bindings: vec![pod_core::WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-hunt".into()],
            }],
            world_admissions: vec![],
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: pod_core::WorldRealityRole::Shadow,
                team_scores: vec![pod_core::TeamDeltaSummary {
                    team_id: "iron-sigil".into(),
                    total_delta: 3,
                }],
                death_marks: vec![],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![pod_core::ObjectiveShiftSummary {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    stage_tag: "marked-by-kills".into(),
                    applications: 1,
                }],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![pod_core::QuestLineStateSummary {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    display_name: "Deadman Shadow: Mirror Hunt".into(),
                    current_stage_ids: vec!["marked-by-kills".into()],
                    completed_stage_ids: vec!["shadow-observe".into()],
                    pending_stage_ids: vec!["rift-collapse".into()],
                    next_stage_ids: vec!["rift-collapse".into()],
                    progress_basis_points: 5000,
                    terminal: false,
                    stage_applications: vec![pod_core::QuestStageApplicationSummary {
                        stage_id: "marked-by-kills".into(),
                        title: "Marked by Kills".into(),
                        applications: 1,
                    }],
                }],
            }],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![pod_core::WorldEvaluationSummary {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: pod_core::WorldRealityRole::Shadow,
                    average_reward_per_row: 4.5,
                    controller_mix: vec![],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 5000,
                    applied_score_delta_total: 3,
                    applied_death_mark_count: 0,
                    applied_death_mark_ticks: 0,
                    applied_objective_shift_count: 1,
                    applied_reputation_delta_total: 0,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        }
        .to_toon_document();

        let updated = pod_core::RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 260,
            tournament: pod_core::WorldTournamentDefinition::new(
                "deadman-neural-cup",
                "Deadman Neural Cup",
            ),
            teams: vec![],
            worlds: vec![world],
            links: vec![],
            world_quest_bindings: vec![pod_core::WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-collapse".into()],
            }],
            world_admissions: vec![],
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: pod_core::WorldRealityRole::Shadow,
                team_scores: vec![pod_core::TeamDeltaSummary {
                    team_id: "iron-sigil".into(),
                    total_delta: 9,
                }],
                death_marks: vec![],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![pod_core::ObjectiveShiftSummary {
                    quest_graph_id: "deadman-shadow-collapse".into(),
                    stage_tag: "rift-collapse".into(),
                    applications: 4,
                }],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![pod_core::QuestLineStateSummary {
                    quest_graph_id: "deadman-shadow-collapse".into(),
                    display_name: "Deadman Shadow: Collapse".into(),
                    current_stage_ids: vec!["rift-collapse".into()],
                    completed_stage_ids: vec!["shadow-observe".into()],
                    pending_stage_ids: vec!["echo-resolve".into()],
                    next_stage_ids: vec!["echo-resolve".into()],
                    progress_basis_points: 7500,
                    terminal: false,
                    stage_applications: vec![pod_core::QuestStageApplicationSummary {
                        stage_id: "rift-collapse".into(),
                        title: "Rift Collapse".into(),
                        applications: 4,
                    }],
                }],
            }],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![pod_core::WorldEvaluationSummary {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: pod_core::WorldRealityRole::Shadow,
                    average_reward_per_row: 6.25,
                    controller_mix: vec![],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 7500,
                    applied_score_delta_total: 9,
                    applied_death_mark_count: 0,
                    applied_death_mark_ticks: 0,
                    applied_objective_shift_count: 4,
                    applied_reputation_delta_total: 0,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        }
        .to_toon_document();

        client
            .receive_remote_topology_document_row(
                10,
                200,
                "deadman-neural-cup".into(),
                "ci-smoke".into(),
                initial.clone(),
            )
            .expect("initial row should apply");
        client
            .receive_remote_topology_document_row(
                11,
                260,
                "deadman-neural-cup".into(),
                "ci-smoke".into(),
                updated.clone(),
            )
            .expect("updated row should apply");
        client
            .receive_remote_topology_document_row(
                9,
                240,
                "deadman-neural-cup".into(),
                "ci-smoke".into(),
                initial,
            )
            .expect("stale row should be ignored");

        assert_eq!(client.resolved_remote_world_id(), Some("deadman-shadow"));
        assert_eq!(
            client
                .resolved_remote_world_quest_binding()
                .map(|binding| binding.quest_graph_ids.as_slice()),
            Some(["deadman-shadow-collapse".to_string()].as_slice())
        );
        assert_eq!(
            client
                .resolved_remote_applied_world_state()
                .and_then(|state| state.team_scores.first())
                .map(|score| score.total_delta),
            Some(9)
        );
        assert_eq!(
            client
                .resolved_remote_applied_world_state()
                .and_then(|state| state.quest_lines.first())
                .map(|quest| quest.quest_graph_id.as_str()),
            Some("deadman-shadow-collapse")
        );
        assert_eq!(
            client
                .resolved_remote_world_evaluation()
                .map(|world| world.average_reward_per_row),
            Some(6.25)
        );
        assert_eq!(
            client
                .resolved_remote_world_evaluation()
                .map(|world| world.applied_objective_shift_count),
            Some(4)
        );

        let events = client.drain_events().collect::<Vec<_>>();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event, StdbEvent::RemoteTopologyUpdated { .. }))
                .count(),
            2
        );
        assert!(events.iter().any(|event| matches!(
            event,
            StdbEvent::RemoteTopologyDocumentReceived { document }
                if document == &updated
        )));
    }

    #[test]
    fn test_receive_debug_document_dispatches_tool_call_event() {
        use pod_core::{AgentToolCallEvent, AgentToolCallTrace, FocusedEntityDebugSummary};

        let mut client = StdbClient::new(StdbClientConfig::default());
        let document = AgentToolCallEvent::new(
            41,
            AgentToolCallTrace::success(8, "llm.complete", "qwen", 12, 24, 8),
        )
        .to_toon_document();

        client
            .receive_debug_document(document.clone())
            .expect("tool document should decode");

        let events = client.drain_events().collect::<Vec<_>>();
        assert!(matches!(
            &events[0],
            StdbEvent::AgentToolCallEventReceived {
                tick: 8,
                agent_entity_id: 41,
                tool_name,
                provider,
                status,
                document: current,
            } if tool_name == "llm.complete"
                && provider == "qwen"
                && status == "Succeeded"
                && current == &document
        ));
        let summary = match &events[1] {
            StdbEvent::FocusedEntityDebugSummaryReceived {
                agent_entity_id,
                document,
            } => {
                assert_eq!(*agent_entity_id, 41);
                decode_toon_document::<FocusedEntityDebugSummary>(
                    document,
                    "focused_entity_debug_summary",
                )
                .expect("focused summary document should decode")
            }
            other => panic!("expected focused summary event, got {other:?}"),
        };
        assert_eq!(summary.entity_id, 41);
        assert_eq!(summary.tool_call_count, 1);
        assert_eq!(summary.latest_tool_name.as_deref(), Some("llm.complete"));
    }

    #[test]
    fn test_receive_debug_document_rejects_unknown_document_type() {
        let mut client = StdbClient::new(StdbClientConfig::default());
        let document = pod_core::encode_toon_document("unsupported_debug_document", &vec![1_u8]);

        let error = client
            .receive_debug_document(document)
            .expect_err("unknown document type should fail");
        assert!(matches!(
            error,
            StdbError::DocumentError(message)
                if message.contains("unsupported debug document type `unsupported_debug_document`")
        ));
    }

    #[test]
    fn test_world_state_cache() {
        let mut client = StdbClient::new(StdbClientConfig::default());

        client.update_world_state(CachedWorldState {
            tick: 42,
            rng_seed: 123,
            ticks_per_second: 60,
            world_width: 2000.0,
            world_height: 2000.0,
            max_entities: 10000,
            paused: false,
        });

        assert_eq!(client.current_tick(), Some(42));
        assert_eq!(client.is_paused(), Some(false));

        // Tick advance should emit event
        client.update_world_state(CachedWorldState {
            tick: 43,
            rng_seed: 123,
            ticks_per_second: 60,
            world_width: 2000.0,
            world_height: 2000.0,
            max_entities: 10000,
            paused: false,
        });

        let events: Vec<StdbEvent> = client.drain_events().collect();
        // Should have WorldStateUpdated + TickAdvanced for each update (4 total)
        assert!(events.len() >= 3);
    }

    #[test]
    fn test_submitted_action_builders() {
        let move_action = SubmittedAction::move_dir(1, 0.707, 0.707);
        assert_eq!(move_action.entity_id, 1);
        assert!(matches!(move_action.action_kind, ActionKind::Move));
        assert!(move_action.direction_x.is_some());

        let stop = SubmittedAction::stop(2);
        assert!(matches!(stop.action_kind, ActionKind::Stop));

        let speak = SubmittedAction::speak(3, "Hello!".into(), SpeakVolume::Normal);
        assert!(matches!(speak.action_kind, ActionKind::Speak));
        assert_eq!(speak.message.as_deref(), Some("Hello!"));

        let attack = SubmittedAction::attack_target(4, 5);
        assert!(matches!(attack.action_kind, ActionKind::AttackTarget));
        assert_eq!(attack.target_entity_id, Some(5));
    }

    #[test]
    fn test_subscription_queries() {
        let all = Subscriptions::all_tables();
        assert!(all.len() >= 15);
        assert!(all.iter().any(|q| q.contains("agent_telemetry_tick")));

        let player = Subscriptions::player_agent(42);
        assert!(player.iter().any(|q| q.contains("observation_event")));
        assert!(player.iter().any(|q| q.contains("42")));
        assert!(player
            .iter()
            .any(|q| q.contains("remote_topology_document")));

        let spectator = Subscriptions::spectator();
        assert!(spectator.iter().any(|q| q.contains("combat_event")));
        assert!(spectator
            .iter()
            .any(|q| q.contains("remote_topology_document")));

        let editor = Subscriptions::editor();
        assert!(editor.iter().any(|q| q.contains("agent_constraints")));
        assert!(editor
            .iter()
            .any(|q| q.contains("remote_topology_document")));

        let editor_debug = Subscriptions::editor_with_debug_telemetry();
        assert!(editor_debug
            .iter()
            .any(|q| q.contains("agent_telemetry_tick")));
        assert!(editor_debug
            .iter()
            .any(|q| q.contains("agent_tool_call_event")));
        assert!(editor_debug.iter().any(|q| q.contains("agent_tick_rollup")));
        assert!(editor_debug
            .iter()
            .any(|q| q.contains("remote_topology_document")));

        let entity_scoped_debug = Subscriptions::debug_telemetry_for_entities(&[44, 77, 44]);
        assert_eq!(entity_scoped_debug.len(), 6);
        assert!(entity_scoped_debug
            .iter()
            .all(|query| query.contains("WHERE agent_entity_id = ")));
        assert!(entity_scoped_debug
            .iter()
            .any(|query| query == "SELECT * FROM agent_telemetry_tick WHERE agent_entity_id = 44"));
        assert!(
            entity_scoped_debug
                .iter()
                .any(|query| query
                    == "SELECT * FROM agent_tool_call_event WHERE agent_entity_id = 77")
        );
        assert!(entity_scoped_debug
            .iter()
            .any(|query| query == "SELECT * FROM agent_tick_rollup WHERE agent_entity_id = 44"));

        let editor_debug_scoped = Subscriptions::editor_with_debug_telemetry_for_entities(&[44]);
        assert!(editor_debug_scoped
            .iter()
            .any(|query| query.contains("FROM world_state")));
        assert!(editor_debug_scoped
            .iter()
            .any(|query| query == "SELECT * FROM agent_telemetry_tick WHERE agent_entity_id = 44"));
        assert!(!editor_debug_scoped
            .iter()
            .any(|query| query == "SELECT * FROM agent_telemetry_tick"));
        assert!(!editor_debug_scoped
            .iter()
            .any(|query| query == "SELECT * FROM agent_tool_call_event"));
        assert!(!editor_debug_scoped
            .iter()
            .any(|query| query == "SELECT * FROM agent_tick_rollup"));
    }

    #[test]
    fn test_receive_debug_telemetry_events() {
        use pod_core::{
            decode_toon_document, AgentTickRollup, AgentToolCallEvent, AgentToolCallTrace,
            FocusedEntityDebugSummary, TickTelemetryFrame, VersionedTickTelemetry,
        };

        let mut client = StdbClient::new(StdbClientConfig::default());
        client.receive_agent_telemetry_tick(
            8,
            41,
            VersionedTickTelemetry::new(TickTelemetryFrame::empty(8)).to_toon_document(),
        );
        client.receive_agent_tool_call_event(
            8,
            41,
            "llm.complete".into(),
            "qwen".into(),
            "Succeeded".into(),
            AgentToolCallEvent::new(
                41,
                AgentToolCallTrace::success(8, "llm.complete", "qwen", 12, 24, 8),
            )
            .to_toon_document(),
        );
        client.receive_agent_tick_rollup(
            1,
            60,
            41,
            AgentTickRollup {
                tick_start: 1,
                tick_end: 60,
                agent_entity_id: 41,
                total_distance: 42.0,
                submitted_action_count: 4,
                executed_action_count: 3,
                rejected_action_count: 1,
                tool_call_count: 2,
                tool_error_count: 1,
                visible_entity_count: 21,
                audible_event_count: 4,
                message_count: 2,
                average_tool_latency_ms: 24.0,
            }
            .to_toon_document(),
        );

        let events: Vec<StdbEvent> = client.drain_events().collect();
        assert!(matches!(
            &events[0],
            StdbEvent::AgentTelemetryTickReceived {
                tick: 8,
                agent_entity_id: 41,
                ..
            }
        ));
        assert!(matches!(
            &events[1],
            StdbEvent::AgentToolCallEventReceived {
                tick: 8,
                agent_entity_id: 41,
                document,
                ..
            } if document.contains("agent_tool_call_event")
        ));
        assert!(matches!(
            &events[2],
            StdbEvent::FocusedEntityDebugSummaryReceived {
                agent_entity_id: 41,
                document,
            } if document.contains("focused_entity_debug_summary")
        ));
        assert!(matches!(
            &events[3],
            StdbEvent::AgentTickRollupReceived {
                tick_start: 1,
                tick_end: 60,
                agent_entity_id: 41,
                document,
            } if document.contains("agent_tick_rollup")
        ));
        let focused_summary = match &events[4] {
            StdbEvent::FocusedEntityDebugSummaryReceived { document, .. } => {
                decode_toon_document::<FocusedEntityDebugSummary>(
                    document,
                    "focused_entity_debug_summary",
                )
                .expect("focused summary document should decode")
            }
            other => panic!("expected focused summary event, got {other:?}"),
        };
        assert_eq!(focused_summary.latest_tick, 60);
        assert_eq!(focused_summary.tool_call_count, 2);
        assert_eq!(
            focused_summary.latest_tool_name.as_deref(),
            Some("llm.complete")
        );
        assert_eq!(
            focused_summary.latest_tool_status.as_deref(),
            Some("Succeeded")
        );
    }

    #[test]
    fn test_cached_entity_helpers() {
        let mut entity = CachedEntity::from_entity(1, Some(AgentType::LlmAgent), true);
        assert_eq!(entity.position(), None);
        assert_eq!(entity.health_fraction(), None);

        entity.pos_x = Some(10.0);
        entity.pos_y = Some(20.0);
        assert_eq!(entity.position(), Some((10.0, 20.0)));

        entity.health = Some(75.0);
        entity.max_health = Some(100.0);
        assert!((entity.health_fraction().unwrap() - 0.75).abs() < 0.001);
    }

    #[test]
    fn test_reducer_calls_require_connection() {
        let mut client = StdbClient::new(StdbClientConfig::default());
        assert!(client.call_create_world(42, 1000.0, 1000.0, 60).is_err());
        assert!(client.call_set_paused(true).is_err());
        assert!(client.call_spawn_entity(0.0, 0.0, None).is_err());
        assert!(client.call_connect_llm_agent(1, "Bot".into()).is_err());
        assert!(client.call_execute_tick().is_err());
        assert!(client
            .call_create_lobby("arena".into(), 1, 4, false)
            .is_err());
        assert!(client.call_join_lobby(1, 1).is_err());
        assert!(client.call_leave_lobby().is_err());
        assert!(client.call_set_lobby_ready(1, true).is_err());
        assert!(client.call_start_lobby(1).is_err());
        assert!(client.call_join_match_queue(1, 2).is_err());
        assert!(client.call_leave_match_queue().is_err());
        assert!(client.call_create_match_from_queue(2).is_err());
    }
}
