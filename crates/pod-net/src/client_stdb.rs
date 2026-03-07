//! SpacetimeDB connection mode for pod-net.
//!
//! This module provides [`SpacetimeDBClient`], an adapter that wraps
//! [`pod_stdb::client::StdbClient`] and presents it with a similar interface
//! to [`crate::client_native::NativeClient`]. Instead of direct QUIC/WebSocket
//! connections, it communicates through SpacetimeDB tables and reducers.
//!
//! ## Usage
//!
//! Enable the `spacetimedb` feature flag:
//! ```toml
//! pod-net = { path = "../pod-net", features = ["spacetimedb"] }
//! ```
//!
//! Then use `SpacetimeDBClient` instead of `NativeClient`:
//! ```rust,ignore
//! use pod_net::client_stdb::{SpacetimeDBClient, SpacetimeDBClientConfig};
//! use pod_core::action::Action;
//! use glam::Vec2;
//!
//! let config = SpacetimeDBClientConfig {
//!     host: "http://localhost:3000".into(),
//!     db_name: "prompt-or-die".into(),
//!     player_name: "Agent-1".into(),
//!     ..Default::default()
//! };
//! let mut client = SpacetimeDBClient::new(config);
//! client.connect().unwrap();
//!
//! // Game loop:
//! loop {
//!     let messages = client.poll_updates();
//!     for msg in messages {
//!         // Handle ServerMessage variants...
//!     }
//!     client.queue_action(Action::Move { direction: Vec2::X });
//!     client.send_actions(0).unwrap();
//!     # break;
//! }
//! ```

use std::collections::HashSet;
use std::fmt;

use glam::Vec2;
use uuid::Uuid;

use pod_core::action::{AbilityTarget, Action, SpeakVolume as CoreSpeakVolume};
use pod_core::event::{Event, GameEvent};
use pod_core::id::{AgentId, EntityId};

use pod_stdb::client::{
    CachedEntity, StdbClient, StdbClientConfig, StdbConnectionMode, StdbError, StdbEvent,
    SubmittedAction, Subscriptions,
};
use pod_stdb::types::{
    AbilityTargetKind, ActionKind, AgentType, SpeakVolume as StdbSpeakVolume, WorldEventKind,
};

use crate::protocol::{ClientId, ReconnectToken, ServerMessage};
use crate::snapshot::{EntitySnapshot, StateDelta, WorldSnapshot};

// ============================================================
// SUBSCRIPTION MANAGEMENT
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq)]
enum SubscriptionProfile {
    /// Read-only spectator mode. Receives all public world tables and events.
    Spectator,
    /// Editor/dashboards: all world state without transient events.
    Editor,
    /// Player mode for a single controlled entity + public events.
    Player(u64),
    /// Caller-supplied custom query set.
    Custom(Vec<String>),
}

#[derive(Debug)]
struct SpacetimeSubscriptionManager {
    profile: SubscriptionProfile,
    active_queries: Vec<String>,
    pending: bool,
}

impl SpacetimeSubscriptionManager {
    fn new() -> Self {
        Self {
            profile: SubscriptionProfile::Spectator,
            active_queries: Vec::new(),
            pending: true,
        }
    }

    fn set_spectator(&mut self) -> bool {
        self.set_profile(SubscriptionProfile::Spectator)
    }

    fn set_editor(&mut self) -> bool {
        self.set_profile(SubscriptionProfile::Editor)
    }

    fn set_player(&mut self, entity_id: u64) -> bool {
        self.set_profile(SubscriptionProfile::Player(entity_id))
    }

    fn set_custom(&mut self, queries: Vec<String>) -> bool {
        self.set_profile(SubscriptionProfile::Custom(normalize_queries(queries)))
    }

    fn is_spectator(&self) -> bool {
        matches!(self.profile, SubscriptionProfile::Spectator)
    }

    fn set_profile(&mut self, profile: SubscriptionProfile) -> bool {
        if self.profile == profile {
            return false;
        }

        self.profile = profile;
        self.pending = true;
        true
    }

    fn ensure_subscriptions_applied(
        &mut self,
        client: &mut StdbClient,
    ) -> Result<bool, StdbClientError> {
        if !self.pending {
            return Ok(false);
        }

        if !client.is_connected() {
            return Ok(false);
        }

        let queries = self.queries_for_profile();

        if self.active_queries != queries {
            client
                .subscribe(queries.clone())
                .map_err(StdbClientError::from)?;
            self.active_queries = queries;
        }

        self.pending = false;
        Ok(true)
    }

    fn queries_for_profile(&self) -> Vec<String> {
        match &self.profile {
            SubscriptionProfile::Spectator => Subscriptions::spectator()
                .into_iter()
                .map(ToString::to_string)
                .collect(),
            SubscriptionProfile::Editor => Subscriptions::editor()
                .into_iter()
                .map(ToString::to_string)
                .collect(),
            SubscriptionProfile::Player(entity_id) => Subscriptions::player_agent(*entity_id)
                .into_iter()
                .collect(),
            SubscriptionProfile::Custom(queries) => queries.clone(),
        }
    }

    fn reset_connection(&mut self) {
        self.active_queries.clear();
        self.pending = true;
    }

    #[cfg(test)]
    fn active_queries(&self) -> &[String] {
        &self.active_queries
    }
}

fn normalize_queries(mut queries: Vec<String>) -> Vec<String> {
    let mut dedup = HashSet::<String>::new();
    queries.retain(|query| dedup.insert(query.clone()));
    queries.sort_unstable();
    queries
}

fn build_player_interest_queries(
    entity_id: u64,
    center_x: f32,
    center_y: f32,
    radius: f32,
) -> Result<Vec<String>, StdbClientError> {
    build_player_partitioned_interest_queries(entity_id, center_x, center_y, radius, radius)
}

fn chunk_bounds(min: f32, max: f32, chunk_size: f32) -> impl Iterator<Item = (f32, f32)> {
    let mut chunks = Vec::new();
    let mut cursor = min;

    while cursor <= max {
        let end = (cursor + chunk_size).min(max);
        chunks.push((cursor, end));

        if end >= max {
            break;
        }

        cursor = end;
    }

    chunks.into_iter()
}

fn build_player_partitioned_interest_queries(
    entity_id: u64,
    center_x: f32,
    center_y: f32,
    radius: f32,
    partition_size: f32,
) -> Result<Vec<String>, StdbClientError> {
    if !radius.is_finite() || !partition_size.is_finite() || radius < 0.0 || partition_size <= 0.0 {
        return Err(StdbClientError::InvalidState(
            "Player interest radius and partition size must be finite and valid".into(),
        ));
    }

    let min_x = center_x - radius;
    let max_x = center_x + radius;
    let min_y = center_y - radius;
    let max_y = center_y + radius;

    let mut queries = Subscriptions::player_agent(entity_id)
        .into_iter()
        .collect::<Vec<String>>();

    queries.retain(|query| query.as_str() != "SELECT * FROM transform");

    let own_entity_query = format!("SELECT * FROM transform WHERE entity_id = {entity_id}");
    queries.push(own_entity_query);

    for (chunk_min_x, chunk_max_x) in chunk_bounds(min_x, max_x, partition_size) {
        for (chunk_min_y, chunk_max_y) in chunk_bounds(min_y, max_y, partition_size) {
            queries.push(format!(
                "SELECT * FROM transform WHERE pos_x >= {chunk_min_x} AND pos_x <= {chunk_max_x} AND pos_y >= {chunk_min_y} AND pos_y <= {chunk_max_y}"
            ));
        }
    }

    Ok(normalize_queries(queries))
}

// ============================================================
// CONFIGURATION
// ============================================================

/// Configuration for connecting to a SpacetimeDB instance.
///
/// Wraps [`StdbClientConfig`] with no additional fields — kept as a
/// separate type so pod-net consumers don't need to depend on pod-stdb
/// directly.
#[derive(Debug, Clone)]
pub struct SpacetimeDBClientConfig {
    /// SpacetimeDB host URI (e.g., `"http://localhost:3000"`)
    pub host: String,
    /// Database name (e.g., `"prompt-or-die"`)
    pub db_name: String,
    /// Authentication token from a previous session.
    /// If `None`, SpacetimeDB generates a new Identity + token on connect.
    pub auth_token: Option<String>,
    /// Player display name
    pub player_name: String,
    /// Runtime mode for the underlying StdbClient.
    ///
    /// `Generated` is production mode; `Emulated` is explicit local fallback.
    pub connection_mode: StdbConnectionMode,
}

impl Default for SpacetimeDBClientConfig {
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

impl From<SpacetimeDBClientConfig> for StdbClientConfig {
    fn from(cfg: SpacetimeDBClientConfig) -> Self {
        StdbClientConfig {
            host: cfg.host,
            db_name: cfg.db_name,
            auth_token: cfg.auth_token,
            player_name: cfg.player_name,
            connection_mode: cfg.connection_mode,
        }
    }
}

// ============================================================
// ERROR TYPE
// ============================================================

/// Errors from the SpacetimeDB client adapter.
///
/// Mirrors [`crate::client_native::ClientError`] with SpacetimeDB-specific
/// variants.
#[derive(Debug)]
pub enum StdbClientError {
    /// Not connected to SpacetimeDB.
    NotConnected,
    /// Connection attempt failed.
    Connection(String),
    /// Reducer call failed.
    Reducer(String),
    /// Subscription setup failed.
    Subscription(String),
    /// Invalid state for the requested operation.
    InvalidState(String),
}

impl fmt::Display for StdbClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => write!(f, "Not connected to SpacetimeDB"),
            Self::Connection(msg) => write!(f, "SpacetimeDB connection error: {msg}"),
            Self::Reducer(msg) => write!(f, "SpacetimeDB reducer error: {msg}"),
            Self::Subscription(msg) => write!(f, "SpacetimeDB subscription error: {msg}"),
            Self::InvalidState(msg) => write!(f, "SpacetimeDB invalid state: {msg}"),
        }
    }
}

impl std::error::Error for StdbClientError {}

impl From<StdbError> for StdbClientError {
    fn from(err: StdbError) -> Self {
        match err {
            StdbError::NotConnected => Self::NotConnected,
            StdbError::ConnectionFailed(msg) => Self::Connection(msg),
            StdbError::ReducerError(msg) => Self::Reducer(msg),
            StdbError::SubscriptionError(msg) => Self::Subscription(msg),
            StdbError::InvalidState(msg) => Self::InvalidState(msg),
        }
    }
}

// ============================================================
// CLIENT
// ============================================================

/// SpacetimeDB client adapter for pod-net.
///
/// Presents a similar interface to [`crate::client_native::NativeClient`]
/// but communicates through SpacetimeDB tables and reducers instead of
/// direct QUIC connections.
///
/// ## Connection Model
///
/// Unlike `NativeClient` (which uses async I/O over QUIC), `StdbClient` uses
/// an event-driven polling model:
///
/// 1. [`connect()`](Self::connect) initiates connection and subscribes to tables
/// 2. [`poll_updates()`](Self::poll_updates) drives the internal event loop via `frame_tick()`
/// 3. Events from SpacetimeDB are converted to [`ServerMessage`] variants
/// 4. Actions are queued via [`queue_action()`](Self::queue_action) and sent via
///    [`send_actions()`](Self::send_actions), which calls the `submit_actions` reducer
///
/// ## Event Mapping
///
/// | SpacetimeDB Event | ServerMessage |
/// |---|---|
/// | `SubscriptionApplied` | `Welcome` (initial world snapshot) |
/// | `EntityInserted` / `EntityUpdated` | `StateDelta` (updated entities) |
/// | `EntityDeleted` | `StateDelta` (destroyed entities) |
/// | `CombatEventReceived` | `EventBatch` (`Damage` / `Kill`) |
/// | `SpeechEventReceived` | `EventBatch` (`AgentSpoke`) |
/// | `WorldEventReceived` | `EventBatch` (various lifecycle events) |
/// | `ConnectError` | `Rejected` |
pub struct SpacetimeDBClient {
    inner: StdbClient,
    subscriptions: SpacetimeSubscriptionManager,
    client_id: Option<ClientId>,
    reconnect_token: ReconnectToken,
    pending_actions: Vec<Action>,
    local_snapshot: Option<WorldSnapshot>,
    welcome_sent: bool,
    last_emitted_tick: u64,
}

impl SpacetimeDBClient {
    /// Create a new SpacetimeDB client (not yet connected).
    pub fn new(config: SpacetimeDBClientConfig) -> Self {
        Self {
            inner: StdbClient::new(config.into()),
            subscriptions: SpacetimeSubscriptionManager::new(),
            client_id: None,
            reconnect_token: ReconnectToken::new(),
            pending_actions: Vec::new(),
            local_snapshot: None,
            welcome_sent: false,
            last_emitted_tick: 0,
        }
    }

    /// Connect to the SpacetimeDB instance and subscribe to game tables.
    ///
    /// After calling `connect()`, poll for updates with [`poll_updates()`](Self::poll_updates)
    /// to receive the `Welcome` message once the subscription is applied.
    pub fn connect(&mut self) -> Result<(), StdbClientError> {
        self.inner.connect().map_err(StdbClientError::from)?;
        self.subscriptions.set_spectator();
        self.subscriptions
            .ensure_subscriptions_applied(&mut self.inner)?;

        Ok(())
    }

    /// Claim a remote LLM slot on an entity and subscribe to player-scoped
    /// tables for that entity.
    ///
    /// This uses the configured `player_name` as the display name for the
    /// `connect_agent` reducer with `AgentType::LlmAgent`.
    pub fn connect_llm_agent(&mut self, entity_id: u64) -> Result<(), StdbClientError> {
        let display_name = self.inner.config().player_name.clone();
        self.inner
            .call_connect_agent(entity_id, AgentType::LlmAgent, display_name)
            .map_err(StdbClientError::from)?;
        self.subscribe_for_player(entity_id)?;
        Ok(())
    }

    /// Queue an action for the next [`send_actions`](Self::send_actions) call.
    pub fn queue_action(&mut self, action: Action) {
        self.pending_actions.push(action);
    }

    /// Send all queued actions to SpacetimeDB via the `submit_actions` reducer.
    ///
    /// The `_tick` parameter exists for interface compatibility with
    /// `NativeClient` — SpacetimeDB determines the authoritative tick
    /// on the server side.
    ///
    /// Returns an error when running in spectator mode, since spectators
    /// must remain read-only.
    pub fn send_actions(&mut self, _tick: u64) -> Result<(), StdbClientError> {
        if !self.inner.is_connected() {
            return Err(StdbClientError::NotConnected);
        }

        if self.subscriptions.is_spectator() {
            return Err(StdbClientError::InvalidState(
                "Spectator mode is read-only".into(),
            ));
        }

        let entity_id = self.inner.controlled_entity().unwrap_or(0);
        let actions: Vec<Action> = self.pending_actions.drain(..).collect();

        for action in &actions {
            let submitted = convert_action(entity_id, action);
            self.inner
                .call_submit_action(&submitted)
                .map_err(StdbClientError::from)?;
        }

        Ok(())
    }

    /// Drive the SpacetimeDB event loop and return any pending messages.
    ///
    /// This is the main polling method — call it once per frame/tick.
    /// It internally calls `StdbClient::frame_tick()` to process pending
    /// SpacetimeDB messages, then converts buffered events into
    /// [`ServerMessage`] variants.
    pub fn poll_updates(&mut self) -> Vec<ServerMessage> {
        self.inner.frame_tick();

        let events: Vec<StdbEvent> = self.inner.drain_events().collect();
        let mut messages = Vec::new();

        for event in events {
            match event {
                // ── Connection lifecycle ──
                StdbEvent::Connected { .. } => {
                    self.client_id = Some(ClientId::new());
                    log::info!("[pod-net stdb] Connected to SpacetimeDB");
                    if let Err(err) = self
                        .subscriptions
                        .ensure_subscriptions_applied(&mut self.inner)
                    {
                        log::error!(
                            "[pod-net stdb] Failed to apply subscriptions after connect: {err}"
                        );
                        messages.push(ServerMessage::Rejected {
                            reason: err.to_string(),
                        });
                    }
                }

                StdbEvent::SubscriptionApplied => {
                    if !self.welcome_sent {
                        let snapshot = self.build_world_snapshot();
                        let tick = self.current_tick_or_zero();
                        self.local_snapshot = Some(snapshot.clone());
                        self.welcome_sent = true;

                        if let Some(client_id) = self.client_id {
                            messages.push(ServerMessage::Welcome {
                                client_id,
                                reconnect_token: self.reconnect_token,
                                tick,
                                controlled_entity: None,
                                authoritative_digest: snapshot.digest(),
                                snapshot,
                            });
                        }
                    }
                }

                StdbEvent::ConnectError { message } => {
                    log::error!("[pod-net stdb] Connection failed: {message}");
                    messages.push(ServerMessage::Rejected { reason: message });
                }

                StdbEvent::Disconnected { reason } => {
                    log::info!("[pod-net stdb] Disconnected: {reason}");
                    self.client_id = None;
                    self.welcome_sent = false;
                }

                // ── Entity lifecycle → StateDelta ──
                StdbEvent::EntityInserted { entity_id }
                | StdbEvent::EntityUpdated { entity_id } => {
                    if let Some(cached) = self.inner.entity(entity_id) {
                        let snap = entity_to_snapshot(cached);
                        let tick = self.current_tick_or_zero();
                        if tick < self.last_emitted_tick {
                            continue;
                        }
                        self.upsert_local_snapshot(tick, &snap);
                        let authoritative_digest = self
                            .local_snapshot
                            .as_ref()
                            .map(WorldSnapshot::digest)
                            .unwrap_or_default();

                        messages.push(ServerMessage::StateDelta {
                            tick,
                            acknowledged_action_tick: None,
                            authoritative_digest,
                            is_full_snapshot: false,
                            delta: StateDelta {
                                tick,
                                updated: vec![snap],
                                destroyed: vec![],
                            },
                        });
                    }
                }

                StdbEvent::EntityDeleted { entity_id } => {
                    let tick = self.current_tick_or_zero();
                    if tick < self.last_emitted_tick {
                        continue;
                    }
                    if let Some(ref mut local) = self.local_snapshot {
                        local.tick = tick;
                        local.entities.retain(|e| e.id != entity_id);
                    }
                    let authoritative_digest = self
                        .local_snapshot
                        .as_ref()
                        .map(WorldSnapshot::digest)
                        .unwrap_or_default();

                    messages.push(ServerMessage::StateDelta {
                        tick,
                        acknowledged_action_tick: None,
                        authoritative_digest,
                        is_full_snapshot: false,
                        delta: StateDelta {
                            tick,
                            updated: vec![],
                            destroyed: vec![entity_id],
                        },
                    });
                }

                // ── Combat events → EventBatch ──
                StdbEvent::CombatEventReceived {
                    tick,
                    attacker_id,
                    defender_id,
                    damage,
                    killed,
                } => {
                    let origin = self.entity_position(defender_id);
                    let mut game_events = vec![GameEvent {
                        tick,
                        origin,
                        event: Event::Damage {
                            source: Some(EntityId(attacker_id)),
                            target: EntityId(defender_id),
                            amount: damage,
                        },
                    }];

                    if killed {
                        game_events.push(GameEvent {
                            tick,
                            origin,
                            event: Event::Kill {
                                killer: Some(EntityId(attacker_id)),
                                victim: EntityId(defender_id),
                            },
                        });
                    }

                    messages.push(ServerMessage::EventBatch {
                        tick,
                        events: game_events,
                    });
                }

                // ── Speech events → EventBatch ──
                StdbEvent::SpeechEventReceived {
                    tick,
                    speaker_id,
                    message,
                    volume,
                } => {
                    let origin = self.entity_position(speaker_id);
                    let agent_id = agent_id_from_entity(speaker_id);

                    messages.push(ServerMessage::EventBatch {
                        tick,
                        events: vec![GameEvent {
                            tick,
                            origin,
                            event: Event::AgentSpoke {
                                agent_id,
                                message,
                                volume: volume.range(),
                            },
                        }],
                    });
                }

                // ── World events → EventBatch ──
                StdbEvent::WorldEventReceived {
                    tick,
                    event_kind,
                    entity_id,
                    secondary_entity_id,
                    data_json,
                } => {
                    let origin = self.entity_position(entity_id);
                    if let Some(game_event) = convert_world_event(
                        tick,
                        origin,
                        &event_kind,
                        entity_id,
                        secondary_entity_id,
                        &data_json,
                    ) {
                        messages.push(ServerMessage::EventBatch {
                            tick,
                            events: vec![game_event],
                        });
                    }
                }

                // ── Tick advancement ──
                StdbEvent::TickAdvanced { new_tick, .. } => {
                    self.last_emitted_tick = self.last_emitted_tick.max(new_tick);
                    if let Some(ref mut local) = self.local_snapshot {
                        local.tick = new_tick;
                    }
                }

                StdbEvent::WorldStateUpdated { tick, .. } => {
                    self.last_emitted_tick = self.last_emitted_tick.max(tick);
                    if let Some(ref mut local) = self.local_snapshot {
                        local.tick = tick;
                    }
                }

                // ── Internal events (no ServerMessage equivalent) ──
                StdbEvent::ObservationReceived { .. }
                | StdbEvent::ReducerCallSuccess { .. }
                | StdbEvent::ReducerCallError { .. } => {}
            }
        }

        messages
    }

    /// Disconnect from SpacetimeDB.
    pub fn disconnect(&mut self) {
        self.inner.disconnect();
        self.client_id = None;
        self.welcome_sent = false;
        self.last_emitted_tick = 0;
        self.subscriptions.reset_connection();
    }

    /// Configure subscriptions for spectator mode (all public tables + events).
    ///
    /// Calling this while connected applies immediately; otherwise, it is staged and
    /// applied once a connection is established.
    pub fn subscribe_as_spectator(&mut self) -> Result<bool, StdbClientError> {
        self.subscriptions.set_spectator();
        self.subscriptions
            .ensure_subscriptions_applied(&mut self.inner)
    }

    /// Configure subscriptions for editor dashboards (world-state tables, no transient events).
    ///
    /// Calling this while connected applies immediately; otherwise, it is staged and
    /// applied once a connection is established.
    pub fn subscribe_as_editor(&mut self) -> Result<bool, StdbClientError> {
        self.subscriptions.set_editor();
        self.subscriptions
            .ensure_subscriptions_applied(&mut self.inner)
    }

    /// Configure subscriptions for a specific player entity.
    ///
    /// Includes shared world tables and the connected entity row + filtered observations.
    /// Calling this while connected applies immediately; otherwise, it is staged and
    /// applied once a connection is established.
    pub fn subscribe_for_player(&mut self, entity_id: u64) -> Result<bool, StdbClientError> {
        self.subscriptions.set_player(entity_id);
        self.subscriptions
            .ensure_subscriptions_applied(&mut self.inner)
    }

    /// Configure spatially filtered subscriptions around a controlled player.
    ///
    /// This keeps world metadata and your player row subscribed while filtering
    /// transform updates by an axis-aligned interest box derived from
    /// `(center_x, center_y, radius)`.
    pub fn subscribe_for_player_with_interest(
        &mut self,
        entity_id: u64,
        center_x: f32,
        center_y: f32,
        radius: f32,
    ) -> Result<bool, StdbClientError> {
        let queries = build_player_interest_queries(entity_id, center_x, center_y, radius)?;
        self.subscriptions.set_custom(queries);
        self.subscriptions
            .ensure_subscriptions_applied(&mut self.inner)
    }

    /// Configure spatially filtered subscriptions with explicit partition size.
    ///
    /// This lets callers balance spatial selectivity and query count for very
    /// large worlds by splitting the radius square into smaller query windows.
    pub fn subscribe_for_player_with_interest_partitioned(
        &mut self,
        entity_id: u64,
        center_x: f32,
        center_y: f32,
        radius: f32,
        partition_size: f32,
    ) -> Result<bool, StdbClientError> {
        let queries = build_player_partitioned_interest_queries(
            entity_id,
            center_x,
            center_y,
            radius,
            partition_size,
        )?;
        self.subscriptions.set_custom(queries);
        self.subscriptions
            .ensure_subscriptions_applied(&mut self.inner)
    }

    /// Configure subscriptions using custom SQL queries.
    /// Duplicate queries are deduplicated and ordering is normalized.
    ///
    /// Calling this while connected applies immediately; otherwise, it is staged and
    /// applied once a connection is established.
    pub fn subscribe_custom(&mut self, queries: Vec<String>) -> Result<bool, StdbClientError> {
        self.subscriptions.set_custom(queries);
        self.subscriptions
            .ensure_subscriptions_applied(&mut self.inner)
    }

    /// Get the current local world snapshot (populated after `Welcome`).
    pub fn local_snapshot(&self) -> Option<&WorldSnapshot> {
        self.local_snapshot.as_ref()
    }

    /// Get the assigned client ID (available after `Connected` event).
    pub fn client_id(&self) -> Option<ClientId> {
        self.client_id
    }

    /// Whether the client is connected to SpacetimeDB.
    pub fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    /// Access the underlying [`StdbClient`] for advanced operations
    /// (e.g., calling reducers directly, inspecting cached state).
    pub fn inner(&self) -> &StdbClient {
        &self.inner
    }

    /// Mutably access the underlying [`StdbClient`].
    pub fn inner_mut(&mut self) -> &mut StdbClient {
        &mut self.inner
    }

    // ── Internal helpers ──

    fn current_tick_or_zero(&self) -> u64 {
        self.inner.current_tick().unwrap_or(0)
    }

    fn entity_position(&self, entity_id: u64) -> Vec2 {
        self.inner
            .entity(entity_id)
            .and_then(|e| e.position())
            .map(|(x, y)| Vec2::new(x, y))
            .unwrap_or(Vec2::ZERO)
    }

    fn build_world_snapshot(&self) -> WorldSnapshot {
        let entities: Vec<EntitySnapshot> = self
            .inner
            .entities()
            .values()
            .map(entity_to_snapshot)
            .collect();

        WorldSnapshot {
            tick: self.current_tick_or_zero(),
            entities,
        }
    }

    fn upsert_local_snapshot(&mut self, tick: u64, snap: &EntitySnapshot) {
        if let Some(ref mut local) = self.local_snapshot {
            local.tick = tick;
            if let Some(existing) = local.entities.iter_mut().find(|e| e.id == snap.id) {
                *existing = snap.clone();
            } else {
                local.entities.push(snap.clone());
            }
        }
    }
}

// ============================================================
// TYPE CONVERSIONS
// ============================================================

/// Convert a cached SpacetimeDB entity into a pod-net [`EntitySnapshot`].
fn entity_to_snapshot(cached: &CachedEntity) -> EntitySnapshot {
    let (px, py) = cached.position().unwrap_or((0.0, 0.0));
    EntitySnapshot {
        id: cached.entity_id,
        position: Vec2::new(px, py),
        velocity: Vec2::new(cached.vel_x.unwrap_or(0.0), cached.vel_y.unwrap_or(0.0)),
        rotation: cached.rotation.unwrap_or(0.0),
        health: cached.health,
        max_health: cached.max_health,
        movement_speed: cached.max_speed,
        label: cached.name.clone(),
    }
}

/// Convert a pod-core [`Action`] into a SpacetimeDB [`SubmittedAction`].
fn convert_action(entity_id: u64, action: &Action) -> SubmittedAction {
    match action {
        Action::Move { direction } => {
            SubmittedAction::move_dir(entity_id, direction.x, direction.y)
        }
        Action::Stop => SubmittedAction::stop(entity_id),
        Action::Rotate { angle } => SubmittedAction::rotate(entity_id, *angle),
        Action::LookAt { target } => SubmittedAction {
            action_kind: ActionKind::LookAt,
            target_x: Some(target.x),
            target_y: Some(target.y),
            ..default_submitted(entity_id)
        },
        Action::Attack => SubmittedAction::attack(entity_id, 0.0, 0.0),
        Action::AttackTarget { target } => SubmittedAction::attack_target(entity_id, target.0),
        Action::UseAbility { slot, target } => {
            let (target_kind, tx, ty, te) = match target {
                Some(AbilityTarget::Position(p)) => (
                    Some(AbilityTargetKind::Position),
                    Some(p.x),
                    Some(p.y),
                    None,
                ),
                Some(AbilityTarget::Entity(eid)) => {
                    (Some(AbilityTargetKind::Entity), None, None, Some(eid.0))
                }
                Some(AbilityTarget::Direction(d)) => (
                    Some(AbilityTargetKind::Direction),
                    Some(d.x),
                    Some(d.y),
                    None,
                ),
                None => (Some(AbilityTargetKind::None), None, None, None),
            };
            SubmittedAction {
                action_kind: ActionKind::UseAbility,
                ability_slot: Some(*slot),
                ability_target_kind: target_kind,
                target_x: tx,
                target_y: ty,
                target_entity_id: te,
                ..default_submitted(entity_id)
            }
        }
        Action::CaptureCreature { target, tool_slot } => SubmittedAction {
            action_kind: ActionKind::CaptureCreature,
            target_entity_id: Some(target.0),
            ability_slot: *tool_slot,
            ..default_submitted(entity_id)
        },
        Action::SummonCompanion { slot } => SubmittedAction {
            action_kind: ActionKind::SummonCompanion,
            ability_slot: Some(*slot),
            ..default_submitted(entity_id)
        },
        Action::CommandCompanion {
            slot,
            command,
            target,
        } => SubmittedAction {
            action_kind: ActionKind::CommandCompanion,
            ability_slot: Some(*slot),
            target_entity_id: target.map(|target| target.0),
            signal_type: Some(format!("{command:?}")),
            ..default_submitted(entity_id)
        },
        Action::Interact => SubmittedAction {
            action_kind: ActionKind::Interact,
            ..default_submitted(entity_id)
        },
        Action::InteractWith { target } => SubmittedAction {
            action_kind: ActionKind::InteractWith,
            target_entity_id: Some(target.0),
            ..default_submitted(entity_id)
        },
        Action::Pickup { target } => SubmittedAction {
            action_kind: ActionKind::Pickup,
            target_entity_id: Some(target.0),
            ..default_submitted(entity_id)
        },
        Action::Drop { slot } => SubmittedAction {
            action_kind: ActionKind::Drop,
            ability_slot: Some(*slot),
            ..default_submitted(entity_id)
        },
        Action::UseItem { slot } => SubmittedAction {
            action_kind: ActionKind::UseItem,
            ability_slot: Some(*slot),
            ..default_submitted(entity_id)
        },
        Action::GatherResource { target, skill } => SubmittedAction {
            action_kind: ActionKind::GatherResource,
            target_entity_id: Some(target.0),
            signal_type: Some(format!("{skill:?}")),
            ..default_submitted(entity_id)
        },
        Action::Loot { target } => SubmittedAction {
            action_kind: ActionKind::Loot,
            target_entity_id: Some(target.0),
            ..default_submitted(entity_id)
        },
        Action::Speak { message, volume } => {
            SubmittedAction::speak(entity_id, message.clone(), convert_speak_volume(volume))
        }
        Action::Signal { signal_type, data } => SubmittedAction {
            action_kind: ActionKind::Signal,
            signal_type: Some(signal_type.clone()),
            signal_data: Some(data.clone()),
            ..default_submitted(entity_id)
        },
        Action::SetAutoRetaliate { enabled } => SubmittedAction {
            action_kind: ActionKind::SetAutoRetaliate,
            signal_data: Some(enabled.to_string()),
            ..default_submitted(entity_id)
        },
        Action::Idle => SubmittedAction::idle(entity_id),
        Action::Spawn { prefab, position } => SubmittedAction {
            action_kind: ActionKind::Spawn,
            prefab: Some(prefab.clone()),
            target_x: Some(position.x),
            target_y: Some(position.y),
            ..default_submitted(entity_id)
        },
    }
}

/// Create a [`SubmittedAction`] with all optional fields set to `None`.
/// Used as a base for struct update syntax (`..default_submitted(id)`).
fn default_submitted(entity_id: u64) -> SubmittedAction {
    SubmittedAction {
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

/// Convert pod-core [`CoreSpeakVolume`] to pod-stdb [`StdbSpeakVolume`].
fn convert_speak_volume(volume: &CoreSpeakVolume) -> StdbSpeakVolume {
    match volume {
        CoreSpeakVolume::Whisper => StdbSpeakVolume::Whisper,
        CoreSpeakVolume::Normal => StdbSpeakVolume::Normal,
        CoreSpeakVolume::Shout => StdbSpeakVolume::Shout,
    }
}

/// Create a deterministic [`AgentId`] from a SpacetimeDB entity_id.
///
/// SpacetimeDB uses `u64` entity IDs; pod-core uses UUID-based `AgentId`s.
/// We create a deterministic UUID so the same entity always maps to the
/// same `AgentId`.
fn agent_id_from_entity(entity_id: u64) -> AgentId {
    AgentId(Uuid::from_u128(entity_id as u128))
}

/// Convert a SpacetimeDB [`WorldEventKind`] into a pod-core [`GameEvent`].
///
/// Returns `None` for events that don't have a meaningful `GameEvent`
/// equivalent (e.g., `TickAdvanced` is handled separately).
fn convert_world_event(
    tick: u64,
    origin: Vec2,
    event_kind: &WorldEventKind,
    entity_id: u64,
    secondary_entity_id: Option<u64>,
    data_json: &str,
) -> Option<GameEvent> {
    let event = match event_kind {
        WorldEventKind::EntitySpawned => Event::EntitySpawned {
            entity: EntityId(entity_id),
            entity_type: data_json.to_string(),
        },
        WorldEventKind::EntityDespawned => Event::EntityDestroyed {
            entity: EntityId(entity_id),
        },
        WorldEventKind::EntityDied => Event::Kill {
            killer: secondary_entity_id.map(EntityId),
            victim: EntityId(entity_id),
        },
        WorldEventKind::InteractionTriggered => Event::Custom {
            name: "interaction".into(),
            data: data_json.to_string(),
        },
        WorldEventKind::ItemPickedUp => Event::ItemPickedUp {
            entity: EntityId(entity_id),
            item: EntityId(secondary_entity_id.unwrap_or(0)),
        },
        WorldEventKind::ItemDropped => Event::ItemDropped {
            entity: EntityId(entity_id),
            item: EntityId(secondary_entity_id.unwrap_or(0)),
        },
        WorldEventKind::AbilityUsed => Event::Custom {
            name: "ability_used".into(),
            data: data_json.to_string(),
        },
        WorldEventKind::WorldCreated => Event::GameStateChanged {
            new_state: "world_created".into(),
        },
        WorldEventKind::WorldReset => Event::GameStateChanged {
            new_state: "world_reset".into(),
        },
        WorldEventKind::TickAdvanced => {
            // Handled separately via StdbEvent::TickAdvanced
            return None;
        }
    };

    Some(GameEvent {
        tick,
        origin,
        event,
    })
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::action::SpeakVolume as CoreSpeakVolume;

    #[test]
    fn test_entity_to_snapshot_defaults() {
        let cached = CachedEntity::from_entity(42, None, true);
        let snap = entity_to_snapshot(&cached);

        assert_eq!(snap.id, 42);
        assert_eq!(snap.position, Vec2::ZERO);
        assert_eq!(snap.velocity, Vec2::ZERO);
        assert_eq!(snap.rotation, 0.0);
        assert!(snap.health.is_none());
        assert!(snap.max_health.is_none());
        assert!(snap.movement_speed.is_none());
        assert!(snap.label.is_none());
    }

    #[test]
    fn test_entity_to_snapshot_with_components() {
        let mut cached = CachedEntity::from_entity(7, None, true);
        cached.pos_x = Some(100.0);
        cached.pos_y = Some(200.0);
        cached.vel_x = Some(1.0);
        cached.vel_y = Some(-1.0);
        cached.rotation = Some(1.57);
        cached.health = Some(80.0);
        cached.max_health = Some(100.0);
        cached.max_speed = Some(240.0);
        cached.name = Some("Hero".into());

        let snap = entity_to_snapshot(&cached);

        assert_eq!(snap.id, 7);
        assert_eq!(snap.position, Vec2::new(100.0, 200.0));
        assert_eq!(snap.velocity, Vec2::new(1.0, -1.0));
        assert!((snap.rotation - 1.57).abs() < f32::EPSILON);
        assert_eq!(snap.health, Some(80.0));
        assert_eq!(snap.max_health, Some(100.0));
        assert_eq!(snap.movement_speed, Some(240.0));
        assert_eq!(snap.label.as_deref(), Some("Hero"));
    }

    #[test]
    fn test_convert_action_move() {
        let action = Action::Move {
            direction: Vec2::new(1.0, 0.0),
        };
        let submitted = convert_action(5, &action);
        assert_eq!(submitted.entity_id, 5);
        assert_eq!(submitted.action_kind, ActionKind::Move);
        assert_eq!(submitted.direction_x, Some(1.0));
        assert_eq!(submitted.direction_y, Some(0.0));
    }

    #[test]
    fn test_convert_action_stop() {
        let submitted = convert_action(10, &Action::Stop);
        assert_eq!(submitted.entity_id, 10);
        assert_eq!(submitted.action_kind, ActionKind::Stop);
    }

    #[test]
    fn test_convert_action_speak() {
        let action = Action::Speak {
            message: "hello".into(),
            volume: CoreSpeakVolume::Shout,
        };
        let submitted = convert_action(1, &action);
        assert_eq!(submitted.action_kind, ActionKind::Speak);
        assert_eq!(submitted.message.as_deref(), Some("hello"));
        assert_eq!(submitted.volume, Some(StdbSpeakVolume::Shout));
    }

    #[test]
    fn test_convert_action_attack_target() {
        let action = Action::AttackTarget {
            target: EntityId(99),
        };
        let submitted = convert_action(1, &action);
        assert_eq!(submitted.action_kind, ActionKind::AttackTarget);
        assert_eq!(submitted.target_entity_id, Some(99));
    }

    #[test]
    fn test_convert_action_capture_creature() {
        let action = Action::CaptureCreature {
            target: EntityId(12),
            tool_slot: Some(3),
        };
        let submitted = convert_action(1, &action);
        assert_eq!(submitted.action_kind, ActionKind::CaptureCreature);
        assert_eq!(submitted.target_entity_id, Some(12));
        assert_eq!(submitted.ability_slot, Some(3));
    }

    #[test]
    fn test_convert_action_command_companion() {
        let action = Action::CommandCompanion {
            slot: 2,
            command: pod_core::action::CompanionCommand::Attack,
            target: Some(EntityId(44)),
        };
        let submitted = convert_action(1, &action);
        assert_eq!(submitted.action_kind, ActionKind::CommandCompanion);
        assert_eq!(submitted.ability_slot, Some(2));
        assert_eq!(submitted.target_entity_id, Some(44));
        assert_eq!(submitted.signal_type.as_deref(), Some("Attack"));
    }

    #[test]
    fn test_convert_action_idle() {
        let submitted = convert_action(1, &Action::Idle);
        assert_eq!(submitted.action_kind, ActionKind::Idle);
    }

    #[test]
    fn test_convert_action_rotate() {
        let action = Action::Rotate { angle: 3.14 };
        let submitted = convert_action(1, &action);
        assert_eq!(submitted.action_kind, ActionKind::Rotate);
        assert_eq!(submitted.angle, Some(3.14));
    }

    #[test]
    fn test_convert_speak_volume() {
        assert_eq!(
            convert_speak_volume(&CoreSpeakVolume::Whisper),
            StdbSpeakVolume::Whisper
        );
        assert_eq!(
            convert_speak_volume(&CoreSpeakVolume::Normal),
            StdbSpeakVolume::Normal
        );
        assert_eq!(
            convert_speak_volume(&CoreSpeakVolume::Shout),
            StdbSpeakVolume::Shout
        );
    }

    #[test]
    fn test_agent_id_from_entity_deterministic() {
        let a = agent_id_from_entity(42);
        let b = agent_id_from_entity(42);
        assert_eq!(a, b);

        let c = agent_id_from_entity(43);
        assert_ne!(a, c);
    }

    #[test]
    fn test_convert_world_event_entity_spawned() {
        let event = convert_world_event(
            10,
            Vec2::ZERO,
            &WorldEventKind::EntitySpawned,
            5,
            None,
            "npc",
        );
        assert!(event.is_some());
        let ge = event.unwrap();
        assert_eq!(ge.tick, 10);
        match ge.event {
            Event::EntitySpawned {
                entity,
                entity_type,
            } => {
                assert_eq!(entity, EntityId(5));
                assert_eq!(entity_type, "npc");
            }
            _ => panic!("Expected EntitySpawned"),
        }
    }

    #[test]
    fn test_convert_world_event_entity_died() {
        let event = convert_world_event(
            20,
            Vec2::new(1.0, 2.0),
            &WorldEventKind::EntityDied,
            10,
            Some(3),
            "",
        );
        assert!(event.is_some());
        match event.unwrap().event {
            Event::Kill { killer, victim } => {
                assert_eq!(killer, Some(EntityId(3)));
                assert_eq!(victim, EntityId(10));
            }
            _ => panic!("Expected Kill"),
        }
    }

    #[test]
    fn test_convert_world_event_tick_advanced_returns_none() {
        let event = convert_world_event(30, Vec2::ZERO, &WorldEventKind::TickAdvanced, 0, None, "");
        assert!(event.is_none());
    }

    #[test]
    fn test_config_default() {
        let cfg = SpacetimeDBClientConfig::default();
        assert_eq!(cfg.host, "http://localhost:3000");
        assert_eq!(cfg.db_name, "prompt-or-die");
        assert!(cfg.auth_token.is_none());
        assert_eq!(cfg.player_name, "Player");
        #[cfg(debug_assertions)]
        assert!(matches!(cfg.connection_mode, StdbConnectionMode::Emulated));
        #[cfg(not(debug_assertions))]
        assert!(matches!(cfg.connection_mode, StdbConnectionMode::Generated));
    }

    #[test]
    fn test_config_into_stdb_config() {
        let cfg = SpacetimeDBClientConfig {
            host: "http://example.com".into(),
            db_name: "test-db".into(),
            auth_token: Some("tok123".into()),
            player_name: "Bot".into(),
            connection_mode: StdbConnectionMode::Emulated,
        };
        let stdb: StdbClientConfig = cfg.into();
        assert_eq!(stdb.host, "http://example.com");
        assert_eq!(stdb.db_name, "test-db");
        assert_eq!(stdb.auth_token, Some("tok123".into()));
        assert_eq!(stdb.player_name, "Bot");
        assert!(matches!(stdb.connection_mode, StdbConnectionMode::Emulated));
    }

    #[test]
    fn test_error_display() {
        assert_eq!(
            format!("{}", StdbClientError::NotConnected),
            "Not connected to SpacetimeDB"
        );
        assert_eq!(
            format!("{}", StdbClientError::Connection("timeout".into())),
            "SpacetimeDB connection error: timeout"
        );
    }

    #[test]
    fn test_error_from_stdb_error() {
        let err: StdbClientError = StdbError::NotConnected.into();
        assert!(matches!(err, StdbClientError::NotConnected));

        let err: StdbClientError = StdbError::ReducerError("fail".into()).into();
        assert!(matches!(err, StdbClientError::Reducer(msg) if msg == "fail"));
    }

    #[test]
    fn test_new_client_not_connected() {
        let client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        assert!(!client.is_connected());
        assert!(client.client_id().is_none());
        assert!(client.local_snapshot().is_none());
    }

    #[test]
    fn test_queue_action() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        client.queue_action(Action::Idle);
        client.queue_action(Action::Stop);
        assert_eq!(client.pending_actions.len(), 2);
    }

    #[test]
    fn test_subscriptions_default_profile() {
        let client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        let queries = client.subscriptions.queries_for_profile();

        assert!(queries
            .iter()
            .any(|query| query.contains("FROM world_state")));
        assert!(queries.iter().any(|query| query.contains("FROM entity")));
        assert!(queries.iter().any(|query| query.contains("combat_event")));
        assert!(client.subscriptions.active_queries().is_empty());
    }

    #[test]
    fn test_subscriptions_dedup_custom() {
        let mut manager = SpacetimeSubscriptionManager::new();
        let input = vec![
            "SELECT * FROM world_state".to_string(),
            "SELECT * FROM world_state".to_string(),
            "SELECT * FROM entity".to_string(),
            "SELECT * FROM entity".to_string(),
        ];
        manager.set_custom(input);
        manager.pending = true;

        let queries = manager
            .queries_for_profile()
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(queries, normalize_queries(queries.clone()));
    }

    #[test]
    fn test_subscriptions_player_interest_query() {
        let queries = build_player_interest_queries(7, 10.0, 20.0, 5.0).unwrap();
        let own_transform_query = queries
            .iter()
            .find(|query| query.starts_with("SELECT * FROM transform WHERE entity_id = 7"))
            .expect("own transform query should exist");

        assert!(own_transform_query.contains("entity_id = 7"));

        let chunk_transform_queries = queries
            .iter()
            .filter(|query| query.contains("pos_x >= ") && query.contains("pos_y >= "))
            .collect::<Vec<_>>();

        assert_eq!(chunk_transform_queries.len(), 4);

        for query in chunk_transform_queries {
            assert!(query.contains("pos_x >= "));
            assert!(query.contains("pos_x <= "));
            assert!(query.contains("pos_y >= "));
            assert!(query.contains("pos_y <= "));
        }
    }

    #[test]
    fn test_subscriptions_player_interest_radius_validation() {
        assert!(build_player_interest_queries(1, 0.0, 0.0, -1.0).is_err());
        assert!(build_player_interest_queries(1, 0.0, 0.0, f32::INFINITY).is_err());
        assert!(build_player_interest_queries(1, 0.0, 0.0, f32::NEG_INFINITY).is_err());
        assert!(build_player_interest_queries(1, 0.0, 0.0, f32::NAN).is_err());
    }

    #[test]
    fn test_subscriptions_player_interest_partitioned_query() {
        let queries = build_player_partitioned_interest_queries(7, 10.0, 20.0, 10.0, 6.0).unwrap();
        let own_transform_queries = queries
            .iter()
            .filter(|query| query == &"SELECT * FROM transform WHERE entity_id = 7")
            .count();
        let chunk_queries = queries
            .iter()
            .filter(|query| query.contains("pos_x >= "))
            .count();

        assert_eq!(own_transform_queries, 1);
        assert_eq!(chunk_queries, 16);
    }

    #[test]
    fn test_subscriptions_player_interest_partitioned_validation() {
        assert!(build_player_partitioned_interest_queries(1, 0.0, 0.0, 5.0, 0.0).is_err());
        assert!(build_player_partitioned_interest_queries(1, 0.0, 0.0, 5.0, -1.0).is_err());
        assert!(build_player_partitioned_interest_queries(1, 0.0, 0.0, f32::NAN, 1.0).is_err());
        assert!(build_player_partitioned_interest_queries(1, 0.0, 0.0, 5.0, f32::NAN).is_err());
    }

    #[test]
    fn test_send_actions_not_connected() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        client.queue_action(Action::Idle);
        let result = client.send_actions(0);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StdbClientError::NotConnected));
    }

    #[test]
    fn connect_llm_agent_fails_when_disconnected() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        let result = client.connect_llm_agent(1);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), StdbClientError::NotConnected));
    }
}
