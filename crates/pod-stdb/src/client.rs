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
//! spacetime generate --lang rust --out-dir src/module_bindings --project-path .
//! ```
//! Until bindings are generated, this module provides the typed API surface
//! with stub implementations. See [`StdbClient::connect`] for details.

use std::collections::{HashMap, VecDeque};
use crate::types::*;

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
}

impl Default for StdbClientConfig {
    fn default() -> Self {
        Self {
            host: "http://localhost:3000".into(),
            db_name: "prompt-or-die".into(),
            auth_token: None,
            player_name: "Player".into(),
        }
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
    Connected {
        identity: Vec<u8>,
        token: String,
    },
    /// Disconnected from SpacetimeDB.
    Disconnected {
        reason: String,
    },
    /// Connection attempt failed.
    ConnectError {
        message: String,
    },
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
    TickAdvanced {
        old_tick: u64,
        new_tick: u64,
    },

    // ── Entity lifecycle ──
    /// A new entity appeared in the subscription.
    EntityInserted {
        entity_id: u64,
    },
    /// An entity was updated (any component changed).
    EntityUpdated {
        entity_id: u64,
    },
    /// An entity was removed from the subscription (destroyed or out of range).
    EntityDeleted {
        entity_id: u64,
    },

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

    // ── Reducer acknowledgments ──
    /// A reducer call was acknowledged by the server.
    ReducerCallSuccess {
        reducer_name: String,
    },
    /// A reducer call failed.
    ReducerCallError {
        reducer_name: String,
        error: String,
    },
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
        ]
    }

    /// Subscribe to lobby tables only.
    pub fn lobby_system() -> Vec<&'static str> {
        vec![
            "SELECT * FROM lobby",
            "SELECT * FROM lobby_member",
        ]
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

    // ── Local cache ──
    world_state: Option<CachedWorldState>,
    entities: HashMap<u64, CachedEntity>,
    /// Latest observation JSON per observer entity_id.
    observations: HashMap<u64, String>,
    /// Controlled entity ID (set after connect_agent).
    controlled_entity: Option<u64>,

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
            world_state: None,
            entities: HashMap::new(),
            observations: HashMap::new(),
            controlled_entity: None,
            frames_processed: 0,
            reducers_called: 0,
            events_received: 0,
        }
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
    /// Without them, this transitions to `ConnectionState::Connecting` and
    /// the connection will not actually establish.
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
                return Err(StdbError::InvalidState("Connection already in progress".into()));
            }
            _ => {}
        }

        self.state = ConnectionState::Connecting;
        log::info!(
            "[pod-stdb client] Connecting to {}:{} ...",
            self.config.host,
            self.config.db_name
        );

        // TODO: Replace with actual SpacetimeDB SDK connection when bindings are generated.
        // For now, emit a ConnectError event on next frame_tick to signal
        // that real SDK integration is needed.

        Ok(())
    }

    /// Disconnect from SpacetimeDB.
    pub fn disconnect(&mut self) {
        if let ConnectionState::Connected { .. } = &self.state {
            log::info!("[pod-stdb client] Disconnecting...");
            // TODO: Call actual DbConnection disconnect when SDK is wired.
            self.state = ConnectionState::Disconnected;
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

        // Handle connecting state → emit stub event
        if let ConnectionState::Connecting = &self.state {
            // TODO: Replace with actual DbConnection::frame_tick() when SDK is wired.
            // For now, emit an error to let callers know SDK integration is pending.
            if self.frames_processed % 60 == 0 {
                log::warn!(
                    "[pod-stdb client] Connection pending — SpacetimeDB SDK bindings not yet generated. \
                     Run: spacetime generate --lang rust --out-dir src/module_bindings --project-path ."
                );
            }
        }

        // TODO: In real implementation:
        // self.db_connection.frame_tick();
        // The frame_tick processes pending messages and triggers row callbacks
        // which populate self.events via the registered handlers.
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

        log::info!(
            "[pod-stdb client] Subscribing with {} queries",
            queries.len()
        );

        // TODO: Replace with actual SDK subscription when bindings are generated:
        // self.db_connection.subscription_builder()
        //     .on_applied(|ctx| { ... })
        //     .on_error(|ctx, err| { ... })
        //     .subscribe(queries);

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
        self.reducers_called += 1;
        log::debug!("[pod-stdb client] Calling create_world(seed={seed}, {width}x{height}, tps={tps})");
        // TODO: create_world_reducer::call(seed, width, height, tps);
        Ok(())
    }

    /// Call the `set_paused` reducer.
    pub fn call_set_paused(&mut self, paused: bool) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::debug!("[pod-stdb client] Calling set_paused({paused})");
        // TODO: set_paused_reducer::call(paused);
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
        self.reducers_called += 1;
        log::debug!("[pod-stdb client] Calling spawn_entity({pos_x}, {pos_y}, {agent_type:?})");
        // TODO: spawn_entity_reducer::call(pos_x, pos_y, agent_type);
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
        self.controlled_entity = Some(entity_id);
        self.reducers_called += 1;
        log::info!(
            "[pod-stdb client] Calling connect_agent(entity={entity_id}, type={agent_type:?}, name='{display_name}')"
        );
        // TODO: connect_agent_reducer::call(entity_id, agent_type, display_name);
        Ok(())
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
        self.reducers_called += 1;
        log::info!(
            "[pod-stdb client] Calling create_lobby(name='{name}', host_entity_id={host_entity_id}, max_players={max_players}, is_private={is_private})"
        );
        // TODO: create_lobby_reducer::call(name, host_entity_id, max_players, is_private);
        Ok(())
    }

    /// Join a lobby by ID.
    pub fn call_join_lobby(&mut self, lobby_id: u64, entity_id: u64) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::info!(
            "[pod-stdb client] Calling join_lobby(lobby_id={lobby_id}, entity_id={entity_id})"
        );
        // TODO: join_lobby_reducer::call(lobby_id, entity_id);
        Ok(())
    }

    /// Leave the lobby the caller is currently joined to.
    pub fn call_leave_lobby(&mut self) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::info!("[pod-stdb client] Calling leave_lobby");
        // TODO: leave_lobby_reducer::call();
        Ok(())
    }

    /// Mark this player as ready or not ready in a lobby.
    pub fn call_set_lobby_ready(
        &mut self,
        lobby_id: u64,
        is_ready: bool,
    ) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::info!("[pod-stdb client] Calling set_lobby_ready(lobby_id={lobby_id}, is_ready={is_ready})");
        // TODO: set_lobby_ready_reducer::call(lobby_id, is_ready);
        Ok(())
    }

    /// Start a lobby (host-only action).
    pub fn call_start_lobby(&mut self, lobby_id: u64) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::info!("[pod-stdb client] Calling start_lobby({lobby_id})");
        // TODO: start_lobby_reducer::call(lobby_id);
        Ok(())
    }

    /// Join the matchmaking queue with desired party size.
    pub fn call_join_match_queue(
        &mut self,
        entity_id: u64,
        desired_party_size: u32,
    ) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::info!(
            "[pod-stdb client] Calling join_match_queue(entity_id={entity_id}, desired_party_size={desired_party_size})"
        );
        // TODO: join_match_queue_reducer::call(entity_id, desired_party_size);
        Ok(())
    }

    /// Leave the caller from matchmaking queue.
    pub fn call_leave_match_queue(&mut self) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::info!("[pod-stdb client] Calling leave_match_queue()");
        // TODO: leave_match_queue_reducer::call();
        Ok(())
    }

    /// Create a match from the matchmaking queue.
    pub fn call_create_match_from_queue(&mut self, desired_party_size: u32) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::info!(
            "[pod-stdb client] Calling create_match_from_queue(desired_party_size={desired_party_size})"
        );
        // TODO: create_match_from_queue_reducer::call(desired_party_size);
        Ok(())
    }

    /// Call the `submit_action` reducer with a typed action.
    pub fn call_submit_action(&mut self, action: &SubmittedAction) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::debug!(
            "[pod-stdb client] Calling submit_action(entity={}, kind={:?})",
            action.entity_id,
            action.action_kind
        );
        // TODO: submit_action_reducer::call(
        //     action.entity_id,
        //     action.action_kind,
        //     action.direction_x, action.direction_y,
        //     action.angle,
        //     action.target_x, action.target_y,
        //     action.target_entity_id,
        //     action.ability_slot, action.ability_target_kind,
        //     action.message.clone(), action.volume,
        //     action.signal_type.clone(), action.signal_data.clone(),
        //     action.prefab.clone(),
        // );
        Ok(())
    }

    /// Call the `execute_tick` reducer (server/admin only).
    pub fn call_execute_tick(&mut self) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::debug!("[pod-stdb client] Calling execute_tick()");
        // TODO: execute_tick_reducer::call();
        Ok(())
    }

    /// Call the `destroy_entity` reducer (admin only).
    pub fn call_destroy_entity(&mut self, entity_id: u64) -> Result<(), StdbError> {
        self.require_connected()?;
        self.reducers_called += 1;
        log::debug!("[pod-stdb client] Calling destroy_entity({entity_id})");
        // TODO: destroy_entity_reducer::call(entity_id);
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

    /// Get the entity ID this client controls (set after connect_agent).
    pub fn controlled_entity(&self) -> Option<u64> {
        self.controlled_entity
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
            self.events.push_back(StdbEvent::EntityInserted { entity_id: eid });
        } else {
            self.events.push_back(StdbEvent::EntityUpdated { entity_id: eid });
        }
        self.events_received += 1;
    }

    /// Remove a cached entity. Called from subscription callbacks.
    pub fn remove_entity(&mut self, entity_id: u64) {
        self.entities.remove(&entity_id);
        self.observations.remove(&entity_id);
        self.events.push_back(StdbEvent::EntityDeleted { entity_id });
        self.events_received += 1;
    }

    /// Store a received observation. Called from subscription callbacks.
    pub fn receive_observation(
        &mut self,
        tick: u64,
        observer_entity_id: u64,
        observation_json: String,
    ) {
        self.observations.insert(observer_entity_id, observation_json.clone());
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
}

impl std::fmt::Debug for StdbClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StdbClient")
            .field("host", &self.config.host)
            .field("db_name", &self.config.db_name)
            .field("state", &self.state)
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
        let mut client = StdbClient::new(StdbClientConfig::default());
        assert!(matches!(client.connection_state(), ConnectionState::Disconnected));

        client.connect().unwrap();
        assert!(matches!(client.connection_state(), ConnectionState::Connecting));

        // Double connect should error
        assert!(client.connect().is_err());
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
        assert_eq!(
            client.latest_observation(42),
            Some("{\"test\":true}")
        );
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

        let player = Subscriptions::player_agent(42);
        assert!(player.iter().any(|q| q.contains("observation_event")));
        assert!(player.iter().any(|q| q.contains("42")));

        let spectator = Subscriptions::spectator();
        assert!(spectator.iter().any(|q| q.contains("combat_event")));

        let editor = Subscriptions::editor();
        assert!(editor.iter().any(|q| q.contains("agent_constraints")));
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
