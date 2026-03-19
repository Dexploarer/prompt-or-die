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
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use pod_core::action::{AbilityTarget, Action, CompanionCommand, SpeakVolume as CoreSpeakVolume};
use pod_core::component::{
    CombatLoadout, CombatStyle, CreatureIdentity, EncounterState, Inventory, SkillKind,
    SkillProgress, Team,
};
use pod_core::event::{Event, GameEvent};
use pod_core::id::{AgentId, EntityId};
use pod_core::observation::{
    AgentMessage, AudibleEvent, MessageChannel, Objective, Observation, Relationship, SelfState,
    VisibleEntity,
};
use pod_core::replay::{ReplayFile, ReplayHeader, ReplayRecorder, ReplayTrainingSample};
use pod_core::AgentType as CoreAgentType;
use pod_core::{
    build_rust_sdk_handoff_fixture, decode_toon_document, ActionLifecycleStage, ActionSource,
    AgentRuntimeProfile, AgentTelemetryFrame, AppliedWorldStateSummary, RemoteAgentFallbackReason,
    RemoteAgentRuntimeStatus, RemoteAgentTransportContract, RemoteTopologyBundle,
    RustSdkHandoffArtifact, VersionedObservation, VersionedTickTelemetry, WorldEvaluationSummary,
};

use pod_stdb::client::{
    CachedEntity, ConnectionState, GeneratedBindingCommand, GeneratedBindingEndpoint,
    GeneratedRemoteTopologyDocumentRow, StdbClient, StdbClientConfig, StdbConnectionMode,
    StdbError, StdbEvent, SubmittedAction, Subscriptions,
};
use pod_stdb::module_bindings::{
    self, action_submission_table::ActionSubmissionTableAccess as _,
    connected_agent_table::ConnectedAgentTableAccess as _, entity_table::EntityTableAccess as _,
    publish_remote_topology_document, spawn_entity,
};
use pod_stdb::types::{
    AbilityTargetKind, ActionKind, AgentType, SpeakVolume as StdbSpeakVolume, WorldEventKind,
};
use pod_stdb::{DbContext as _, Table as _};

use crate::protocol::{ClientId, ReconnectToken, ServerMessage};
use crate::snapshot::{
    build_catch_up_diagnostics, build_rollback_preview, compose_presentation_snapshot,
    CatchUpDiagnostics, EntityMetadataSnapshot, EntitySnapshot, InterpolatedSnapshot,
    RecoveryRequestState, RenderClock, RollbackPreview, SnapshotInterpolationBuffer, StateDelta,
    WorldSnapshot,
};

// ============================================================
// SUBSCRIPTION MANAGEMENT
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq)]
enum SubscriptionProfile {
    /// Read-only spectator mode. Receives all public world tables and events.
    Spectator,
    /// Editor/dashboards: all world state without transient events.
    Editor,
    /// Editor/dashboards with debug telemetry streams enabled.
    EditorDebug,
    /// Editor/dashboards with entity-scoped raw debug telemetry.
    EditorDebugEntities(Vec<u64>),
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

    fn set_editor_debug(&mut self) -> bool {
        self.set_profile(SubscriptionProfile::EditorDebug)
    }

    fn set_editor_debug_entities(&mut self, entity_ids: Vec<u64>) -> bool {
        self.set_profile(SubscriptionProfile::EditorDebugEntities(entity_ids))
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
            SubscriptionProfile::EditorDebug => Subscriptions::editor_with_debug_telemetry(),
            SubscriptionProfile::EditorDebugEntities(entity_ids) => {
                Subscriptions::editor_with_debug_telemetry_for_entities(entity_ids)
            }
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

/// Select how a thin Rust SDK adapter host should wire the underlying client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RustSdkAdapterRuntimeMode {
    /// Local deterministic fallback with no generated runtime.
    #[default]
    Emulated,
    /// Generated command/callback seam for deterministic adapter tests.
    GeneratedBinding,
    /// Real generated SpacetimeDB SDK runtime.
    GeneratedSdk,
}

/// Thin host wrapper for future Rust SDK adapters.
///
/// This owns runtime-mode selection and handoff-document decoding while keeping
/// the real observation/topology/telemetry/replay ingest inside
/// [`SpacetimeDBClient`].
pub struct RustSdkAdapterHost {
    runtime_mode: RustSdkAdapterRuntimeMode,
    client: SpacetimeDBClient,
    generated_binding_endpoint: Option<GeneratedBindingEndpoint>,
}

impl RustSdkAdapterHost {
    /// Build a new adapter host with the requested runtime wiring.
    pub fn new(
        mut config: SpacetimeDBClientConfig,
        runtime_mode: RustSdkAdapterRuntimeMode,
    ) -> Self {
        config.connection_mode = match runtime_mode {
            RustSdkAdapterRuntimeMode::Emulated => StdbConnectionMode::Emulated,
            RustSdkAdapterRuntimeMode::GeneratedBinding
            | RustSdkAdapterRuntimeMode::GeneratedSdk => StdbConnectionMode::Generated,
        };

        let mut client = SpacetimeDBClient::new(config);
        let generated_binding_endpoint = match runtime_mode {
            RustSdkAdapterRuntimeMode::Emulated => None,
            RustSdkAdapterRuntimeMode::GeneratedBinding => {
                Some(client.install_generated_binding_runtime())
            }
            RustSdkAdapterRuntimeMode::GeneratedSdk => {
                client.install_generated_sdk_runtime();
                None
            }
        };

        Self {
            runtime_mode,
            client,
            generated_binding_endpoint,
        }
    }

    /// Inspect the selected runtime mode.
    pub fn runtime_mode(&self) -> RustSdkAdapterRuntimeMode {
        self.runtime_mode
    }

    /// Access the wrapped SpacetimeDB client.
    pub fn client(&self) -> &SpacetimeDBClient {
        &self.client
    }

    /// Mutably access the wrapped SpacetimeDB client.
    pub fn client_mut(&mut self) -> &mut SpacetimeDBClient {
        &mut self.client
    }

    /// Clone the command/callback endpoint for generated-binding mode.
    pub fn generated_binding_endpoint(&self) -> Option<GeneratedBindingEndpoint> {
        self.generated_binding_endpoint.clone()
    }

    /// Connect the wrapped client using the configured runtime mode.
    pub fn connect(&mut self) -> Result<(), StdbClientError> {
        self.client.connect()
    }

    /// Drive the wrapped client event loop.
    pub fn poll_updates(&mut self) -> Vec<ServerMessage> {
        self.client.poll_updates()
    }

    /// Apply a repo-owned Rust SDK handoff bundle.
    pub fn apply_handoff_artifact(
        &mut self,
        artifact: RustSdkHandoffArtifact,
    ) -> Result<(), StdbClientError> {
        self.client.apply_rust_sdk_handoff_artifact(artifact)
    }

    /// Apply the canonical deterministic handoff fixture.
    pub fn apply_handoff_fixture(&mut self) -> Result<(), StdbClientError> {
        self.apply_handoff_artifact(build_rust_sdk_handoff_fixture())
    }

    /// Decode and apply a handoff bundle from JSON.
    pub fn apply_handoff_json_document(
        &mut self,
        document: impl AsRef<str>,
    ) -> Result<(), StdbClientError> {
        let artifact =
            serde_json::from_str::<RustSdkHandoffArtifact>(document.as_ref()).map_err(|error| {
                StdbClientError::Document(format!(
                    "failed to decode rust_sdk_handoff_artifact JSON: {error}"
                ))
            })?;
        self.apply_handoff_artifact(artifact)
    }

    /// Decode and apply a handoff bundle from TOON.
    pub fn apply_handoff_toon_document(
        &mut self,
        document: impl AsRef<str>,
    ) -> Result<(), StdbClientError> {
        let artifact = decode_toon_document::<RustSdkHandoffArtifact>(
            document.as_ref(),
            "rust_sdk_handoff_artifact",
        )
        .map_err(|error| {
            StdbClientError::Document(format!(
                "failed to decode rust_sdk_handoff_artifact TOON: {error}"
            ))
        })?;
        self.apply_handoff_artifact(artifact)
    }

    /// Translate and apply a repo-owned Rust SDK state snapshot.
    pub fn apply_state_snapshot(
        &mut self,
        snapshot: &RustSdkStateSnapshot,
    ) -> Result<(), StdbClientError> {
        upsert_rust_sdk_state_snapshot_entities(self.client.inner_mut(), snapshot);
        self.apply_handoff_artifact(snapshot.to_handoff_artifact())
    }

    /// Bind the controlled entity used for repo-owned Rust SDK action execution.
    pub fn bind_action_entity(
        &mut self,
        entity_id: u64,
        agent_type: CoreAgentType,
    ) -> Result<(), StdbClientError> {
        let stdb_agent_type = stdb_agent_type_from_core(agent_type);

        if self.client.inner().entity(entity_id).is_none() {
            self.client
                .inner_mut()
                .upsert_entity(CachedEntity::from_entity(
                    entity_id,
                    Some(stdb_agent_type.clone()),
                    true,
                ));
        }

        self.client.connect_remote_agent(entity_id, stdb_agent_type)
    }

    /// Bind the state snapshot's controlled entity for subsequent action execution.
    pub fn bind_state_snapshot_action_entity(
        &mut self,
        snapshot: &RustSdkStateSnapshot,
    ) -> Result<(), StdbClientError> {
        self.bind_action_entity(
            snapshot.self_state.entity_id.0,
            snapshot.runtime_profile.agent_type,
        )
    }

    /// Execute a repo-owned Rust SDK action plan through the existing action queue.
    pub fn execute_action_plan(
        &mut self,
        plan: &RustSdkActionPlan,
    ) -> Result<Action, RustSdkActionExecutorError> {
        let controlled_entity = self.client.inner().controlled_entity().ok_or_else(|| {
            RustSdkActionExecutorError::Client(
                "cannot execute Rust SDK action plan without a bound remote agent entity".into(),
            )
        })?;

        if self.client.inner().entity(controlled_entity).is_none() {
            return Err(RustSdkActionExecutorError::Client(format!(
                "controlled entity {controlled_entity} is missing from the cached state"
            )));
        }

        let action = rust_sdk_action_from_plan(plan);
        self.client.queue_action(action.clone());
        self.client.send_actions(0)?;
        Ok(action)
    }
}

/// Repo-owned external-state snapshot for a future Rust SDK adapter.
///
/// This keeps SDK-facing capture separate from authoritative [`Observation`]
/// while still translating deterministically into the shared runtime contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustSdkStateSnapshot {
    pub tick: u64,
    pub elapsed_secs: f32,
    pub runtime_profile: AgentRuntimeProfile,
    pub self_state: RustSdkSelfStateSnapshot,
    #[serde(default)]
    pub visible_entities: Vec<RustSdkVisibleEntitySnapshot>,
    #[serde(default)]
    pub audible_events: Vec<AudibleEvent>,
    #[serde(default)]
    pub messages: Vec<AgentMessage>,
    #[serde(default)]
    pub available_actions: Vec<String>,
    #[serde(default)]
    pub objectives: Vec<Objective>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialog: Option<RustSdkDialogState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shop: Option<RustSdkShopState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bank: Option<RustSdkBankState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport_contract: Option<RemoteAgentTransportContract>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_topology: Option<RemoteTopologyBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_tick_telemetry: Option<VersionedTickTelemetry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay: Option<ReplayFile>,
}

impl RustSdkStateSnapshot {
    pub fn to_observation(&self) -> Observation {
        let mut messages = self.messages.clone();
        messages.extend(self.context_messages());

        let mut available_actions = self.available_actions.clone();
        if self.dialog.is_some() {
            push_unique_action_hint(&mut available_actions, "Dialog:Continue");
            push_unique_action_hint(&mut available_actions, "Dialog:SelectOption");
        }
        if let Some(shop) = &self.shop {
            if shop.can_buy {
                push_unique_action_hint(&mut available_actions, "Shop:Buy");
            }
            if shop.can_sell {
                push_unique_action_hint(&mut available_actions, "Shop:Sell");
            }
        }
        if let Some(bank) = &self.bank {
            if bank.can_deposit {
                push_unique_action_hint(&mut available_actions, "Bank:Deposit");
            }
            if bank.can_withdraw {
                push_unique_action_hint(&mut available_actions, "Bank:Withdraw");
            }
        }

        Observation {
            tick: self.tick,
            elapsed_secs: self.elapsed_secs,
            self_state: SelfState {
                agent_id: self.self_state.agent_id,
                entity_id: self.self_state.entity_id,
                runtime_profile: self.runtime_profile,
                position: self.self_state.position,
                rotation: self.self_state.rotation,
                velocity: self.self_state.velocity,
                health: self.self_state.health,
                max_health: self.self_state.max_health,
                team: self.self_state.team,
                cooldowns: Vec::new(),
                combat_loadout: self.self_state.combat_loadout.clone(),
                skills: self.self_state.skills.clone(),
                inventory: self.self_state.inventory.clone(),
                companion_roster: None,
                encounter: self.self_state.encounter.clone(),
            },
            visible_entities: self
                .visible_entities
                .iter()
                .map(|entity| VisibleEntity {
                    entity_id: entity.entity_id,
                    entity_type: entity.entity_type.clone(),
                    position: entity.position,
                    velocity: entity.velocity,
                    rotation: entity.rotation,
                    distance: self.self_state.position.distance(entity.position),
                    relationship: entity.relationship,
                    health_fraction: entity.health_fraction,
                    combat_style: entity.combat_style,
                    creature: entity.creature.clone(),
                })
                .collect(),
            audible_events: self.audible_events.clone(),
            messages,
            available_actions,
            objectives: self.objectives.clone(),
        }
    }

    pub fn to_handoff_artifact(&self) -> RustSdkHandoffArtifact {
        let mut artifact = RustSdkHandoffArtifact::from_versioned_observation(
            VersionedObservation::new(self.runtime_profile, self.to_observation()),
        );
        artifact.transport_contract = self.transport_contract.clone();
        artifact.remote_topology = self.remote_topology.clone();
        artifact.latest_tick_telemetry = self.latest_tick_telemetry.clone();
        artifact.replay = self.replay.clone();
        artifact
    }

    fn context_messages(&self) -> Vec<AgentMessage> {
        let mut messages = Vec::new();
        let from = self.self_state.agent_id;

        if let Some(dialog) = &self.dialog {
            let options = if dialog.options.is_empty() {
                "no options".to_string()
            } else {
                dialog.options.join(", ")
            };
            messages.push(AgentMessage {
                from,
                channel: MessageChannel::Direct,
                content: format!(
                    "dialog:{} prompt=\"{}\" options=[{}]",
                    dialog.speaker, dialog.prompt, options
                ),
            });
        }

        if let Some(shop) = &self.shop {
            messages.push(AgentMessage {
                from,
                channel: MessageChannel::Direct,
                content: format!(
                    "shop:{} offers={} can_buy={} can_sell={}",
                    shop.shop_name, shop.offer_count, shop.can_buy, shop.can_sell
                ),
            });
        }

        if let Some(bank) = &self.bank {
            messages.push(AgentMessage {
                from,
                channel: MessageChannel::Direct,
                content: format!(
                    "bank:{} tabs={} items={} deposit={} withdraw={}",
                    bank.bank_name,
                    bank.tab_count,
                    bank.item_count,
                    bank.can_deposit,
                    bank.can_withdraw
                ),
            });
        }

        messages
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustSdkSelfStateSnapshot {
    pub agent_id: AgentId,
    pub entity_id: EntityId,
    pub position: Vec2,
    pub rotation: f32,
    pub velocity: Vec2,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_health: Option<f32>,
    pub team: Team,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_loadout: Option<CombatLoadout>,
    #[serde(default)]
    pub skills: Vec<SkillProgress>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inventory: Option<Inventory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encounter: Option<EncounterState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustSdkVisibleEntitySnapshot {
    pub entity_id: EntityId,
    pub entity_type: String,
    pub position: Vec2,
    pub velocity: Vec2,
    pub rotation: f32,
    pub relationship: Relationship,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health_fraction: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat_style: Option<CombatStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creature: Option<CreatureIdentity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustSdkDialogState {
    pub speaker: String,
    pub prompt: String,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustSdkShopState {
    pub shop_name: String,
    pub offer_count: u16,
    pub can_buy: bool,
    pub can_sell: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustSdkBankState {
    pub bank_name: String,
    pub tab_count: u8,
    pub item_count: u16,
    pub can_deposit: bool,
    pub can_withdraw: bool,
}

fn push_unique_action_hint(actions: &mut Vec<String>, hint: &str) {
    if !actions.iter().any(|action| action == hint) {
        actions.push(hint.to_string());
    }
}

/// How the SDK adapter should execute a translated action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RustSdkActionExecutionMode {
    /// Immediate low-level call on the SDK runtime.
    #[default]
    Immediate,
    /// Completion-aware helper that may walk, wait, or retry internally.
    CompletionAware,
}

/// Repo-owned action intent produced before a concrete SDK method call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RustSdkActionIntent {
    MoveDirection {
        direction: Vec2,
    },
    Stop,
    Rotate {
        angle: f32,
    },
    LookAtPosition {
        target: Vec2,
    },
    AttackCurrentTarget,
    AttackEntity {
        entity_id: u64,
    },
    UseAbility {
        slot: u8,
        target: Option<AbilityTarget>,
    },
    CaptureCreature {
        entity_id: u64,
        tool_slot: Option<u8>,
    },
    SummonCompanion {
        slot: u8,
    },
    CommandCompanion {
        slot: u8,
        command: CompanionCommand,
        target_entity_id: Option<u64>,
    },
    InteractNearest,
    InteractEntity {
        entity_id: u64,
    },
    PickupEntity {
        entity_id: u64,
    },
    DropInventorySlot {
        slot: u8,
    },
    UseInventorySlot {
        slot: u8,
    },
    GatherEntity {
        entity_id: u64,
        skill: SkillKind,
    },
    LootEntity {
        entity_id: u64,
    },
    Speak {
        message: String,
        volume: CoreSpeakVolume,
    },
    Signal {
        signal_type: String,
        data: String,
    },
    SetAutoRetaliate {
        enabled: bool,
    },
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustSdkActionPlan {
    pub execution_mode: RustSdkActionExecutionMode,
    pub intent: RustSdkActionIntent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustSdkActionAdapterError {
    UnsupportedAction {
        action: &'static str,
        reason: String,
    },
}

impl fmt::Display for RustSdkActionAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedAction { action, reason } => {
                write!(f, "unsupported Rust SDK action {action}: {reason}")
            }
        }
    }
}

impl std::error::Error for RustSdkActionAdapterError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustSdkActionExecutorError {
    Client(String),
}

impl fmt::Display for RustSdkActionExecutorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(message) => write!(f, "failed to execute Rust SDK action plan: {message}"),
        }
    }
}

impl std::error::Error for RustSdkActionExecutorError {}

impl From<StdbClientError> for RustSdkActionExecutorError {
    fn from(error: StdbClientError) -> Self {
        Self::Client(error.to_string())
    }
}

/// Translate a shared POD action into a repo-owned Rust SDK action plan.
pub fn build_rust_sdk_action_plan(
    action: &Action,
) -> Result<RustSdkActionPlan, RustSdkActionAdapterError> {
    let plan = match action {
        Action::Move { direction } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::MoveDirection {
                direction: *direction,
            },
        },
        Action::Stop => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::Stop,
        },
        Action::Rotate { angle } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::Rotate { angle: *angle },
        },
        Action::LookAt { target } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::LookAtPosition { target: *target },
        },
        Action::Attack => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::AttackCurrentTarget,
        },
        Action::AttackTarget { target } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::AttackEntity {
                entity_id: target.0,
            },
        },
        Action::UseAbility { slot, target } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::UseAbility {
                slot: *slot,
                target: target.clone(),
            },
        },
        Action::CaptureCreature { target, tool_slot } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::CaptureCreature {
                entity_id: target.0,
                tool_slot: *tool_slot,
            },
        },
        Action::SummonCompanion { slot } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::SummonCompanion { slot: *slot },
        },
        Action::CommandCompanion {
            slot,
            command,
            target,
        } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::CommandCompanion {
                slot: *slot,
                command: *command,
                target_entity_id: target.map(|entity| entity.0),
            },
        },
        Action::Interact => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::CompletionAware,
            intent: RustSdkActionIntent::InteractNearest,
        },
        Action::InteractWith { target } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::CompletionAware,
            intent: RustSdkActionIntent::InteractEntity {
                entity_id: target.0,
            },
        },
        Action::Pickup { target } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::CompletionAware,
            intent: RustSdkActionIntent::PickupEntity {
                entity_id: target.0,
            },
        },
        Action::Drop { slot } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::DropInventorySlot { slot: *slot },
        },
        Action::UseItem { slot } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::UseInventorySlot { slot: *slot },
        },
        Action::GatherResource { target, skill } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::CompletionAware,
            intent: RustSdkActionIntent::GatherEntity {
                entity_id: target.0,
                skill: *skill,
            },
        },
        Action::Loot { target } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::CompletionAware,
            intent: RustSdkActionIntent::LootEntity {
                entity_id: target.0,
            },
        },
        Action::Speak { message, volume } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::Speak {
                message: message.clone(),
                volume: *volume,
            },
        },
        Action::Signal { signal_type, data } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::Signal {
                signal_type: signal_type.clone(),
                data: data.clone(),
            },
        },
        Action::SetAutoRetaliate { enabled } => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::SetAutoRetaliate { enabled: *enabled },
        },
        Action::Idle => RustSdkActionPlan {
            execution_mode: RustSdkActionExecutionMode::Immediate,
            intent: RustSdkActionIntent::Idle,
        },
        Action::Spawn { .. } => {
            return Err(RustSdkActionAdapterError::UnsupportedAction {
                action: "Spawn",
                reason: "world-authority spawn requests stay outside the SDK adapter".into(),
            });
        }
    };

    Ok(plan)
}

/// One repo-owned record captured from an SDK-backed controller step before it
/// is finalized into the shared replay/training surfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustSdkRolloutRecord {
    pub snapshot: RustSdkStateSnapshot,
    pub prompt_sent: String,
    pub raw_response: String,
    pub action_plans: Vec<RustSdkActionPlan>,
    pub actions_taken: Vec<Action>,
    pub tool_calls: Vec<pod_core::AgentToolCallTrace>,
    pub latency_ms: u32,
}

impl RustSdkRolloutRecord {
    pub fn from_actions(
        snapshot: RustSdkStateSnapshot,
        prompt_sent: impl Into<String>,
        actions_taken: Vec<Action>,
        tool_calls: Vec<pod_core::AgentToolCallTrace>,
        latency_ms: u32,
    ) -> Result<Self, RustSdkRolloutRecorderError> {
        let action_plans = actions_taken
            .iter()
            .map(build_rust_sdk_action_plan)
            .collect::<Result<Vec<_>, _>>()?;
        let raw_response = serde_json::to_string(&action_plans).map_err(|error| {
            RustSdkRolloutRecorderError::SerializeActionPlans(error.to_string())
        })?;

        Ok(Self {
            snapshot,
            prompt_sent: prompt_sent.into(),
            raw_response,
            action_plans,
            actions_taken,
            tool_calls,
            latency_ms,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustSdkRolloutRecorderError {
    DuplicateDecision { tick: u64, agent_id: AgentId },
    UnsupportedAction(RustSdkActionAdapterError),
    ActionExecution(String),
    SerializeActionPlans(String),
}

impl fmt::Display for RustSdkRolloutRecorderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateDecision { tick, agent_id } => {
                write!(
                    f,
                    "duplicate Rust SDK rollout decision for tick {tick} and agent {agent_id}"
                )
            }
            Self::UnsupportedAction(error) => error.fmt(f),
            Self::ActionExecution(message) => {
                write!(f, "failed to execute Rust SDK benchmark action: {message}")
            }
            Self::SerializeActionPlans(message) => {
                write!(f, "failed to serialize Rust SDK action plans: {message}")
            }
        }
    }
}

impl std::error::Error for RustSdkRolloutRecorderError {}

impl From<RustSdkActionAdapterError> for RustSdkRolloutRecorderError {
    fn from(error: RustSdkActionAdapterError) -> Self {
        Self::UnsupportedAction(error)
    }
}

impl From<RustSdkActionExecutorError> for RustSdkRolloutRecorderError {
    fn from(error: RustSdkActionExecutorError) -> Self {
        Self::ActionExecution(error.to_string())
    }
}

/// Thin recorder that keeps external SDK episodes on the shared replay/training
/// surface instead of an adapter-local episode format.
pub struct RustSdkRolloutRecorder {
    replay: ReplayRecorder,
    telemetry_windows: Vec<pod_core::TickTelemetryFrame>,
    seen_decisions: HashSet<(u64, AgentId)>,
}

impl RustSdkRolloutRecorder {
    pub fn new() -> Self {
        Self {
            replay: ReplayRecorder::new(),
            telemetry_windows: Vec::new(),
            seen_decisions: HashSet::new(),
        }
    }

    pub fn record(
        &mut self,
        record: RustSdkRolloutRecord,
    ) -> Result<(), RustSdkRolloutRecorderError> {
        let tick = record.snapshot.tick;
        let agent_id = record.snapshot.self_state.agent_id;

        if !self.seen_decisions.insert((tick, agent_id)) {
            return Err(RustSdkRolloutRecorderError::DuplicateDecision { tick, agent_id });
        }

        let observation = record.snapshot.to_observation();
        self.replay.record_decision(
            tick,
            agent_id,
            &observation,
            record.prompt_sent,
            record.raw_response,
            record.actions_taken,
            record.tool_calls,
            record.latency_ms,
        );

        if let Some(telemetry) = &record.snapshot.latest_tick_telemetry {
            upsert_rust_sdk_telemetry_window(&mut self.telemetry_windows, &telemetry.payload);
        }

        Ok(())
    }

    pub fn finalize(mut self, header: ReplayHeader) -> ReplayFile {
        self.telemetry_windows.sort_by_key(|window| window.tick);
        self.replay
            .finalize_with_telemetry(header, self.telemetry_windows)
    }
}

impl Default for RustSdkRolloutRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Repo-owned session facade for rs-sdk integrations that need one surface for
/// snapshot ingest, action execution, and replay recording.
pub struct RustSdkAdapterSession {
    host: RustSdkAdapterHost,
    recorder: RustSdkRolloutRecorder,
}

impl RustSdkAdapterSession {
    pub fn new(config: SpacetimeDBClientConfig, runtime_mode: RustSdkAdapterRuntimeMode) -> Self {
        Self {
            host: RustSdkAdapterHost::new(config, runtime_mode),
            recorder: RustSdkRolloutRecorder::new(),
        }
    }

    pub fn host(&self) -> &RustSdkAdapterHost {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut RustSdkAdapterHost {
        &mut self.host
    }

    pub fn connect(&mut self) -> Result<(), RustSdkAdapterSessionError> {
        self.host.connect().map_err(Into::into)
    }

    pub fn ingest_state_snapshot(
        &mut self,
        snapshot: &RustSdkStateSnapshot,
    ) -> Result<(), RustSdkAdapterSessionError> {
        self.host.bind_state_snapshot_action_entity(snapshot)?;
        self.host.apply_state_snapshot(snapshot)?;
        Ok(())
    }

    pub fn execute_actions(
        &mut self,
        snapshot: RustSdkStateSnapshot,
        prompt_sent: impl Into<String>,
        actions: Vec<Action>,
        tool_calls: Vec<pod_core::AgentToolCallTrace>,
        latency_ms: u32,
    ) -> Result<Vec<Action>, RustSdkAdapterSessionError> {
        self.ingest_state_snapshot(&snapshot)?;

        let mut executed_actions = Vec::with_capacity(actions.len());
        for action in &actions {
            let plan = build_rust_sdk_action_plan(action)?;
            executed_actions.push(self.host.execute_action_plan(&plan)?);
        }

        let record = RustSdkRolloutRecord::from_actions(
            snapshot,
            prompt_sent.into(),
            executed_actions.clone(),
            tool_calls,
            latency_ms,
        )?;
        self.recorder.record(record)?;

        Ok(executed_actions)
    }

    pub fn record_rollout_step(
        &mut self,
        record: RustSdkRolloutRecord,
    ) -> Result<(), RustSdkAdapterSessionError> {
        self.recorder.record(record).map_err(Into::into)
    }

    pub fn into_host_and_recorder(self) -> (RustSdkAdapterHost, RustSdkRolloutRecorder) {
        (self.host, self.recorder)
    }

    pub fn finalize_replay(self, header: ReplayHeader) -> ReplayFile {
        self.recorder.finalize(header)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustSdkAdapterSessionError {
    Client(String),
    UnsupportedAction(String),
    Rollout(String),
}

impl fmt::Display for RustSdkAdapterSessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(message) => write!(f, "Rust SDK session client error: {message}"),
            Self::UnsupportedAction(message) => {
                write!(f, "Rust SDK session action translation error: {message}")
            }
            Self::Rollout(message) => write!(f, "Rust SDK session rollout error: {message}"),
        }
    }
}

impl std::error::Error for RustSdkAdapterSessionError {}

impl From<StdbClientError> for RustSdkAdapterSessionError {
    fn from(error: StdbClientError) -> Self {
        Self::Client(error.to_string())
    }
}

impl From<RustSdkActionAdapterError> for RustSdkAdapterSessionError {
    fn from(error: RustSdkActionAdapterError) -> Self {
        Self::UnsupportedAction(error.to_string())
    }
}

impl From<RustSdkActionExecutorError> for RustSdkAdapterSessionError {
    fn from(error: RustSdkActionExecutorError) -> Self {
        Self::Client(error.to_string())
    }
}

impl From<RustSdkRolloutRecorderError> for RustSdkAdapterSessionError {
    fn from(error: RustSdkRolloutRecorderError) -> Self {
        Self::Rollout(error.to_string())
    }
}

/// Repo-owned configuration for the thin POD rs-sdk facade.
#[derive(Debug, Clone)]
pub struct RustSdkFacadeConfig {
    pub client: SpacetimeDBClientConfig,
    pub runtime_mode: RustSdkAdapterRuntimeMode,
}

impl Default for RustSdkFacadeConfig {
    fn default() -> Self {
        Self {
            client: SpacetimeDBClientConfig::default(),
            runtime_mode: RustSdkAdapterRuntimeMode::default(),
        }
    }
}

/// Thin POD-owned wrapper above [`RustSdkAdapterSession`].
///
/// This is the repo-owned surface a future packaged rs-sdk can depend on
/// directly without rebuilding runtime-mode selection, handoff ingest, action
/// execution, replay finalization, or the live smoke entrypoint in app code.
pub struct RustSdkFacade {
    session: RustSdkAdapterSession,
}

impl RustSdkFacade {
    pub fn new(config: RustSdkFacadeConfig) -> Self {
        Self {
            session: RustSdkAdapterSession::new(config.client, config.runtime_mode),
        }
    }

    pub fn runtime_mode(&self) -> RustSdkAdapterRuntimeMode {
        self.session.host().runtime_mode()
    }

    pub fn generated_binding_endpoint(&self) -> Option<GeneratedBindingEndpoint> {
        self.session.host().generated_binding_endpoint()
    }

    pub fn client(&self) -> &SpacetimeDBClient {
        self.session.host().client()
    }

    pub fn client_mut(&mut self) -> &mut SpacetimeDBClient {
        self.session.host_mut().client_mut()
    }

    pub fn connect(&mut self) -> Result<(), RustSdkFacadeError> {
        self.session.connect().map_err(Into::into)
    }

    pub fn poll_updates(&mut self) -> Vec<ServerMessage> {
        self.session.host_mut().poll_updates()
    }

    pub fn apply_handoff_artifact(
        &mut self,
        artifact: RustSdkHandoffArtifact,
    ) -> Result<(), RustSdkFacadeError> {
        self.session
            .host_mut()
            .apply_handoff_artifact(artifact)
            .map_err(Into::into)
    }

    pub fn apply_handoff_fixture(&mut self) -> Result<(), RustSdkFacadeError> {
        self.session
            .host_mut()
            .apply_handoff_fixture()
            .map_err(Into::into)
    }

    pub fn apply_handoff_json_document(
        &mut self,
        document: impl AsRef<str>,
    ) -> Result<(), RustSdkFacadeError> {
        self.session
            .host_mut()
            .apply_handoff_json_document(document)
            .map_err(Into::into)
    }

    pub fn apply_handoff_toon_document(
        &mut self,
        document: impl AsRef<str>,
    ) -> Result<(), RustSdkFacadeError> {
        self.session
            .host_mut()
            .apply_handoff_toon_document(document)
            .map_err(Into::into)
    }

    pub fn ingest_state_snapshot(
        &mut self,
        snapshot: &RustSdkStateSnapshot,
    ) -> Result<(), RustSdkFacadeError> {
        self.session.ingest_state_snapshot(snapshot).map_err(Into::into)
    }

    pub fn execute_actions(
        &mut self,
        snapshot: RustSdkStateSnapshot,
        prompt_sent: impl Into<String>,
        actions: Vec<Action>,
        tool_calls: Vec<pod_core::AgentToolCallTrace>,
        latency_ms: u32,
    ) -> Result<Vec<Action>, RustSdkFacadeError> {
        self.session
            .execute_actions(snapshot, prompt_sent, actions, tool_calls, latency_ms)
            .map_err(Into::into)
    }

    pub fn record_rollout_step(
        &mut self,
        record: RustSdkRolloutRecord,
    ) -> Result<(), RustSdkFacadeError> {
        self.session.record_rollout_step(record).map_err(Into::into)
    }

    pub fn into_session(self) -> RustSdkAdapterSession {
        self.session
    }

    pub fn finalize_replay(self, header: ReplayHeader) -> ReplayFile {
        self.session.finalize_replay(header)
    }

    pub fn run_live_smoke(
        config: &RustSdkAdapterLiveSmokeConfig,
    ) -> Result<RustSdkAdapterLiveSmokeRun, RustSdkFacadeError> {
        run_rust_sdk_adapter_live_smoke(config).map_err(Into::into)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustSdkFacadeError {
    Host(String),
    Session(String),
}

impl fmt::Display for RustSdkFacadeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Host(message) => write!(f, "Rust SDK facade host error: {message}"),
            Self::Session(message) => write!(f, "Rust SDK facade session error: {message}"),
        }
    }
}

impl std::error::Error for RustSdkFacadeError {}

impl From<StdbClientError> for RustSdkFacadeError {
    fn from(error: StdbClientError) -> Self {
        Self::Host(error.to_string())
    }
}

impl From<RustSdkAdapterSessionError> for RustSdkFacadeError {
    fn from(error: RustSdkAdapterSessionError) -> Self {
        Self::Session(error.to_string())
    }
}

fn upsert_rust_sdk_telemetry_window(
    telemetry_windows: &mut Vec<pod_core::TickTelemetryFrame>,
    window: &pod_core::TickTelemetryFrame,
) {
    if let Some(existing) = telemetry_windows
        .iter_mut()
        .find(|existing| existing.tick == window.tick)
    {
        *existing = window.clone();
    } else {
        telemetry_windows.push(window.clone());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RustSdkBenchmarkCheck {
    pub metric: String,
    pub passed: bool,
    pub expected: String,
    pub observed: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustSdkBenchmarkScenarioReport {
    pub scenario_id: String,
    pub description: String,
    pub runtime_mode: RustSdkAdapterRuntimeMode,
    pub expected_action_key: String,
    pub observed_action_key: String,
    pub expected_execution_mode: RustSdkActionExecutionMode,
    pub observed_execution_mode: RustSdkActionExecutionMode,
    pub action_matches: bool,
    pub execution_mode_matches: bool,
    pub available_action_matches: bool,
    pub reducer_submission_count: u64,
    pub reducer_submission_matches: bool,
    pub tool_call_error_count: usize,
    pub training_sample_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustSdkBenchmarkReport {
    pub schema_version: u32,
    pub generated_at_unix_ms: u128,
    pub benchmark_id: String,
    pub replay_tick_count: u64,
    pub replay_training_sample_count: usize,
    pub scenarios: Vec<RustSdkBenchmarkScenarioReport>,
    pub checks: Vec<RustSdkBenchmarkCheck>,
}

impl RustSdkBenchmarkReport {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }
}

#[derive(Debug, Clone)]
pub struct RustSdkBenchmarkRun {
    pub report: RustSdkBenchmarkReport,
    pub replay: ReplayFile,
    pub training_samples: Vec<ReplayTrainingSample>,
}

const RUST_SDK_BENCHMARK_SCHEMA_VERSION: u32 = 1;
const RUST_SDK_LIVE_SMOKE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustSdkAdapterLiveSmokeConfig {
    pub host: String,
    pub db_name: String,
    pub auth_token: Option<String>,
    pub timeout_ms: u64,
    pub poll_interval_ms: u64,
}

impl Default for RustSdkAdapterLiveSmokeConfig {
    fn default() -> Self {
        Self {
            host: "http://localhost:3000".into(),
            db_name: "prompt-or-die".into(),
            auth_token: None,
            timeout_ms: 5_000,
            poll_interval_ms: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RustSdkAdapterLiveSmokeReport {
    pub schema_version: u32,
    pub runtime_mode: RustSdkAdapterRuntimeMode,
    pub host: String,
    pub db_name: String,
    pub spawned_entity_id: u64,
    pub connected_agent_entity_id: Option<u64>,
    pub connected_agent_display_name: Option<String>,
    pub action_submission_entity_id: Option<u64>,
    pub action_submission_kind: Option<String>,
    pub observed_action_key: String,
    pub reducer_submission_count: u64,
    pub replay_training_sample_count: usize,
    pub checks: Vec<RustSdkBenchmarkCheck>,
}

impl RustSdkAdapterLiveSmokeReport {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }
}

#[derive(Debug, Clone)]
pub struct RustSdkAdapterLiveSmokeRun {
    pub report: RustSdkAdapterLiveSmokeReport,
    pub replay: ReplayFile,
    pub training_samples: Vec<ReplayTrainingSample>,
}

#[derive(Debug, Clone)]
struct RustSdkBenchmarkCase {
    scenario_id: &'static str,
    description: &'static str,
    runtime_mode: RustSdkAdapterRuntimeMode,
    snapshot: RustSdkStateSnapshot,
    action: Action,
    tool_calls: Vec<pod_core::AgentToolCallTrace>,
    latency_ms: u32,
    expected_action_key: &'static str,
    expected_execution_mode: RustSdkActionExecutionMode,
}

/// Run deterministic adapter-side benchmark cases against the repo-owned Rust
/// SDK state/action seams.
pub fn run_rust_sdk_adapter_benchmark_suite(
) -> Result<RustSdkBenchmarkRun, RustSdkRolloutRecorderError> {
    let cases = curated_rust_sdk_benchmark_cases();
    let mut recorder = RustSdkRolloutRecorder::new();
    let mut scenario_reports = Vec::new();
    let mut max_tick = 0u64;

    for case in &cases {
        max_tick = max_tick.max(case.snapshot.tick);
        let action_plan = build_rust_sdk_action_plan(&case.action)?;
        let mut host = build_rust_sdk_benchmark_host(case)?;
        let reducers_before = host.client().inner().reducers_called();
        let executed_action = host.execute_action_plan(&action_plan)?;
        let reducer_submission_count = host
            .client()
            .inner()
            .reducers_called()
            .saturating_sub(reducers_before);
        let record = RustSdkRolloutRecord::from_actions(
            case.snapshot.clone(),
            format!(
                "rust-sdk benchmark scenario:{} runtime:{:?}",
                case.scenario_id, case.runtime_mode
            ),
            vec![executed_action.clone()],
            case.tool_calls.clone(),
            case.latency_ms,
        )?;
        recorder.record(record)?;

        let expected_variant = rust_sdk_action_variant_name(&executed_action);
        let training_sample_count = case
            .snapshot
            .latest_tick_telemetry
            .as_ref()
            .map(|telemetry| telemetry.payload.agents.len())
            .unwrap_or(0);
        scenario_reports.push(RustSdkBenchmarkScenarioReport {
            scenario_id: case.scenario_id.to_string(),
            description: case.description.to_string(),
            runtime_mode: case.runtime_mode,
            expected_action_key: case.expected_action_key.to_string(),
            observed_action_key: rust_sdk_action_semantic_key(&executed_action).to_string(),
            expected_execution_mode: case.expected_execution_mode,
            observed_execution_mode: action_plan.execution_mode,
            action_matches: rust_sdk_action_semantic_key(&executed_action)
                == case.expected_action_key,
            execution_mode_matches: action_plan.execution_mode == case.expected_execution_mode,
            available_action_matches: case
                .snapshot
                .available_actions
                .iter()
                .any(|available| available == expected_variant),
            reducer_submission_count,
            reducer_submission_matches: reducer_submission_count == 1,
            tool_call_error_count: case
                .tool_calls
                .iter()
                .filter(|trace| {
                    !matches!(
                        trace.status,
                        pod_core::ToolCallStatus::Requested | pod_core::ToolCallStatus::Succeeded
                    )
                })
                .count(),
            training_sample_count,
        });
    }

    let replay = recorder.finalize(ReplayHeader {
        name: "rust-sdk-adapter-benchmark".into(),
        timestamp: 1_710_849_600,
        world_seed: 13,
        tick_count: max_tick + 1,
        agent_count: 1,
        notes: "Deterministic benchmark for the repo-owned Rust SDK adapter seams".into(),
    });
    let training_samples = replay.training_samples();
    let generated_at_unix_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let checks = vec![
        RustSdkBenchmarkCheck {
            metric: "scenario_count".into(),
            passed: scenario_reports.len() == cases.len(),
            expected: cases.len().to_string(),
            observed: scenario_reports.len().to_string(),
        },
        RustSdkBenchmarkCheck {
            metric: "action_match_count".into(),
            passed: scenario_reports
                .iter()
                .all(|scenario| scenario.action_matches),
            expected: cases.len().to_string(),
            observed: scenario_reports
                .iter()
                .filter(|scenario| scenario.action_matches)
                .count()
                .to_string(),
        },
        RustSdkBenchmarkCheck {
            metric: "execution_mode_match_count".into(),
            passed: scenario_reports
                .iter()
                .all(|scenario| scenario.execution_mode_matches),
            expected: cases.len().to_string(),
            observed: scenario_reports
                .iter()
                .filter(|scenario| scenario.execution_mode_matches)
                .count()
                .to_string(),
        },
        RustSdkBenchmarkCheck {
            metric: "available_action_match_count".into(),
            passed: scenario_reports
                .iter()
                .all(|scenario| scenario.available_action_matches),
            expected: cases.len().to_string(),
            observed: scenario_reports
                .iter()
                .filter(|scenario| scenario.available_action_matches)
                .count()
                .to_string(),
        },
        RustSdkBenchmarkCheck {
            metric: "reducer_submission_match_count".into(),
            passed: scenario_reports
                .iter()
                .all(|scenario| scenario.reducer_submission_matches),
            expected: cases.len().to_string(),
            observed: scenario_reports
                .iter()
                .filter(|scenario| scenario.reducer_submission_matches)
                .count()
                .to_string(),
        },
        RustSdkBenchmarkCheck {
            metric: "training_sample_count".into(),
            passed: training_samples.len() == cases.len(),
            expected: cases.len().to_string(),
            observed: training_samples.len().to_string(),
        },
    ];

    Ok(RustSdkBenchmarkRun {
        report: RustSdkBenchmarkReport {
            schema_version: RUST_SDK_BENCHMARK_SCHEMA_VERSION,
            generated_at_unix_ms,
            benchmark_id: "rust-sdk-adapter-seams".into(),
            replay_tick_count: replay.header.tick_count,
            replay_training_sample_count: training_samples.len(),
            scenarios: scenario_reports,
            checks,
        },
        replay,
        training_samples,
    })
}

/// Run a live generated-SDK smoke harness over the repo-owned
/// [`RustSdkAdapterSession`] facade.
pub fn run_rust_sdk_adapter_live_smoke(
    config: &RustSdkAdapterLiveSmokeConfig,
) -> Result<RustSdkAdapterLiveSmokeRun, StdbClientError> {
    let (observer, observer_state) = connect_live_generated_sdk_smoke_observer(config)?;
    let spawned_entity = spawn_live_generated_sdk_smoke_entity(&observer, &observer_state, config)?;

    let mut session = RustSdkAdapterSession::new(
        SpacetimeDBClientConfig {
            host: config.host.clone(),
            db_name: config.db_name.clone(),
            auth_token: config.auth_token.clone(),
            player_name: "rust-sdk-live-smoke".into(),
            ..Default::default()
        },
        RustSdkAdapterRuntimeMode::GeneratedSdk,
    );
    session.connect().map_err(map_rust_sdk_adapter_session_error)?;
    wait_for_live_generated_sdk_session_ready(&mut session, config)?;

    let snapshot = build_rust_sdk_live_smoke_snapshot(spawned_entity.entity_id);
    let reducers_before = session.host().client().inner().reducers_called();
    let executed_actions = session
        .execute_actions(
            snapshot.clone(),
            "rust-sdk live smoke",
            vec![Action::Idle],
            Vec::new(),
            11,
        )
        .map_err(map_rust_sdk_adapter_session_error)?;
    let reducer_submission_count = session
        .host()
        .client()
        .inner()
        .reducers_called()
        .saturating_sub(reducers_before);
    let (connected_agent, action_submission) = wait_for_live_generated_sdk_smoke_results(
        &observer,
        &observer_state,
        &mut session,
        config,
        spawned_entity.entity_id,
    )?;
    let replay = session.finalize_replay(ReplayHeader {
        name: "rust-sdk-live-smoke".into(),
        timestamp: 1_710_849_600,
        world_seed: 29,
        tick_count: snapshot.tick + 1,
        agent_count: 1,
        notes: "Live generated-SDK smoke for the repo-owned Rust SDK adapter session".into(),
    });
    let training_samples = replay.training_samples();
    let observed_action_key = executed_actions
        .first()
        .map(rust_sdk_action_semantic_key)
        .unwrap_or("none")
        .to_string();

    let checks = vec![
        RustSdkBenchmarkCheck {
            metric: "runtime_mode".into(),
            passed: true,
            expected: "GeneratedSdk".into(),
            observed: format!("{:?}", RustSdkAdapterRuntimeMode::GeneratedSdk),
        },
        RustSdkBenchmarkCheck {
            metric: "connected_agent_entity_match".into(),
            passed: connected_agent.entity_id == spawned_entity.entity_id,
            expected: spawned_entity.entity_id.to_string(),
            observed: connected_agent.entity_id.to_string(),
        },
        RustSdkBenchmarkCheck {
            metric: "connected_agent_display_name_match".into(),
            passed: connected_agent.display_name == "rust-sdk-live-smoke",
            expected: "rust-sdk-live-smoke".into(),
            observed: connected_agent.display_name.clone(),
        },
        RustSdkBenchmarkCheck {
            metric: "connected_agent_type_match".into(),
            passed: matches!(
                connected_agent.agent_type,
                module_bindings::AgentType::LlmAgent
            ),
            expected: "LlmAgent".into(),
            observed: format!("{:?}", connected_agent.agent_type),
        },
        RustSdkBenchmarkCheck {
            metric: "action_submission_entity_match".into(),
            passed: action_submission.entity_id == spawned_entity.entity_id,
            expected: spawned_entity.entity_id.to_string(),
            observed: action_submission.entity_id.to_string(),
        },
        RustSdkBenchmarkCheck {
            metric: "action_submission_kind_match".into(),
            passed: matches!(action_submission.action_kind, module_bindings::ActionKind::Idle),
            expected: "Idle".into(),
            observed: format!("{:?}", action_submission.action_kind),
        },
        RustSdkBenchmarkCheck {
            metric: "reducer_submission_count".into(),
            passed: reducer_submission_count == 2,
            expected: "2".into(),
            observed: reducer_submission_count.to_string(),
        },
        RustSdkBenchmarkCheck {
            metric: "replay_training_sample_count".into(),
            passed: training_samples.len() == 1,
            expected: "1".into(),
            observed: training_samples.len().to_string(),
        },
    ];

    Ok(RustSdkAdapterLiveSmokeRun {
        report: RustSdkAdapterLiveSmokeReport {
            schema_version: RUST_SDK_LIVE_SMOKE_SCHEMA_VERSION,
            runtime_mode: RustSdkAdapterRuntimeMode::GeneratedSdk,
            host: config.host.clone(),
            db_name: config.db_name.clone(),
            spawned_entity_id: spawned_entity.entity_id,
            connected_agent_entity_id: Some(connected_agent.entity_id),
            connected_agent_display_name: Some(connected_agent.display_name.clone()),
            action_submission_entity_id: Some(action_submission.entity_id),
            action_submission_kind: Some(format!("{:?}", action_submission.action_kind)),
            observed_action_key,
            reducer_submission_count,
            replay_training_sample_count: training_samples.len(),
            checks,
        },
        replay,
        training_samples,
    })
}

fn curated_rust_sdk_benchmark_cases() -> Vec<RustSdkBenchmarkCase> {
    let attack_snapshot = build_rust_sdk_attack_snapshot();
    let gather_snapshot = build_rust_sdk_gather_snapshot();

    vec![
        RustSdkBenchmarkCase {
            scenario_id: "frontier_anchor_attack",
            description: "Remote combat state should submit an immediate attack plan over the emulated runtime seam.",
            runtime_mode: RustSdkAdapterRuntimeMode::Emulated,
            snapshot: attack_snapshot,
            action: Action::AttackTarget {
                target: EntityId(9001),
            },
            tool_calls: Vec::new(),
            latency_ms: 7,
            expected_action_key: "attack",
            expected_execution_mode: RustSdkActionExecutionMode::Immediate,
        },
        RustSdkBenchmarkCase {
            scenario_id: "frontier_iron_gather",
            description: "Resource-node state should submit a completion-aware gather plan over the generated-binding seam.",
            runtime_mode: RustSdkAdapterRuntimeMode::GeneratedBinding,
            snapshot: gather_snapshot,
            action: Action::GatherResource {
                target: EntityId(9101),
                skill: SkillKind::Mining,
            },
            tool_calls: vec![pod_core::AgentToolCallTrace::success(
                1,
                "path_probe",
                "rs-sdk-benchmark",
                12,
                4,
                3,
            )],
            latency_ms: 19,
            expected_action_key: "gather_resource",
            expected_execution_mode: RustSdkActionExecutionMode::CompletionAware,
        },
    ]
}

fn build_rust_sdk_benchmark_host(
    case: &RustSdkBenchmarkCase,
) -> Result<RustSdkAdapterHost, RustSdkRolloutRecorderError> {
    let mut host = RustSdkAdapterHost::new(
        SpacetimeDBClientConfig {
            db_name: format!("rust-sdk-benchmark-{}", case.scenario_id),
            ..Default::default()
        },
        case.runtime_mode,
    );

    host.connect().map_err(RustSdkActionExecutorError::from)?;

    match case.runtime_mode {
        RustSdkAdapterRuntimeMode::Emulated => {
            let _ = host.poll_updates();
        }
        RustSdkAdapterRuntimeMode::GeneratedBinding => {
            let endpoint = host.generated_binding_endpoint().ok_or_else(|| {
                RustSdkActionExecutorError::Client(
                    "generated-binding benchmark host is missing its endpoint".into(),
                )
            })?;
            let callbacks = endpoint.callbacks();
            let _ = endpoint.drain_commands();
            callbacks.connected(vec![7; 16], format!("tok-{}", case.scenario_id));
            let _ = host.poll_updates();
            let _ = endpoint.drain_commands();
            callbacks.subscription_applied();
            let _ = host.poll_updates();
        }
        RustSdkAdapterRuntimeMode::GeneratedSdk => {}
    }

    host.bind_state_snapshot_action_entity(&case.snapshot)
        .map_err(RustSdkActionExecutorError::from)?;

    if let Some(endpoint) = host.generated_binding_endpoint() {
        let callbacks = endpoint.callbacks();
        let _ = endpoint.drain_commands();
        callbacks.subscription_applied();
        let _ = host.poll_updates();
    }

    host.apply_state_snapshot(&case.snapshot)
        .map_err(RustSdkActionExecutorError::from)?;
    Ok(host)
}

fn build_rust_sdk_attack_snapshot() -> RustSdkStateSnapshot {
    let fixture = build_rust_sdk_handoff_fixture();
    let mut observation = fixture.observation.payload.clone();
    observation.tick = 0;
    observation.available_actions = vec!["Attack".into(), "Move".into()];
    observation.visible_entities = vec![VisibleEntity {
        entity_id: EntityId(9001),
        entity_type: "rift_hound".into(),
        position: Vec2::new(134.0, 98.0),
        velocity: Vec2::ZERO,
        rotation: 0.0,
        distance: 6.0,
        relationship: Relationship::Hostile,
        health_fraction: Some(0.42),
        combat_style: Some(CombatStyle::Melee),
        creature: None,
    }];
    observation.objectives = vec![Objective {
        id: "hold-anchor".into(),
        description: "Hold the frontier anchor".into(),
        progress: 0.7,
        completed: false,
    }];

    let snapshot = RustSdkStateSnapshot {
        tick: observation.tick,
        elapsed_secs: observation.elapsed_secs,
        runtime_profile: fixture.observation.profile,
        self_state: RustSdkSelfStateSnapshot {
            agent_id: observation.self_state.agent_id,
            entity_id: observation.self_state.entity_id,
            position: observation.self_state.position,
            rotation: observation.self_state.rotation,
            velocity: observation.self_state.velocity,
            health: observation.self_state.health,
            max_health: observation.self_state.max_health,
            team: observation.self_state.team,
            combat_loadout: observation.self_state.combat_loadout.clone(),
            skills: observation.self_state.skills.clone(),
            inventory: observation.self_state.inventory.clone(),
            encounter: Some(EncounterState {
                encounter_id: 71,
                kind: pod_core::EncounterKind::OpenWorld,
                threat_level: 0.9,
                primary_target: Some(EntityId(9001)),
                active_turn_owner: Some(observation.self_state.entity_id),
                capture_allowed: false,
                in_combat: true,
            }),
        },
        visible_entities: observation
            .visible_entities
            .iter()
            .map(rust_sdk_visible_entity_from_observation)
            .collect(),
        audible_events: observation.audible_events.clone(),
        messages: observation.messages.clone(),
        available_actions: observation.available_actions.clone(),
        objectives: observation.objectives.clone(),
        dialog: None,
        shop: None,
        bank: None,
        transport_contract: fixture.transport_contract.clone(),
        remote_topology: fixture.remote_topology.clone(),
        latest_tick_telemetry: Some(VersionedTickTelemetry::new(build_rust_sdk_tick_telemetry(
            &observation,
            &Action::AttackTarget {
                target: EntityId(9001),
            },
            &[],
        ))),
        replay: None,
    };

    snapshot
}

fn build_rust_sdk_gather_snapshot() -> RustSdkStateSnapshot {
    let profile = AgentRuntimeProfile::for_agent_type(CoreAgentType::LlmAgent);
    let agent_id = AgentId(Uuid::from_u128(0x504f445f52555354_53444b0000000011));
    let observation = Observation {
        tick: 1,
        elapsed_secs: 0.2,
        self_state: SelfState {
            agent_id,
            entity_id: EntityId(4001),
            runtime_profile: profile,
            position: Vec2::new(320.0, 640.0),
            rotation: 1.25,
            velocity: Vec2::new(-0.5, 0.75),
            health: Some(67.0),
            max_health: Some(99.0),
            team: Team::Team(2),
            combat_loadout: Some(CombatLoadout {
                style: CombatStyle::Ranged,
                attack_range: 9.0,
                attack_speed_ticks: 4,
                max_hit: 12.0,
                auto_retaliate: true,
                equipped_weapon: Some("maple-shortbow".into()),
                offhand_item: None,
                active_ability_bar: vec!["piercing-shot".into(), "binding-shot".into()],
            }),
            skills: vec![
                SkillProgress::new(SkillKind::Mining, 52, 145_200, 9_800),
                SkillProgress::new(SkillKind::Magic, 41, 49_900, 3_400),
            ],
            inventory: Some(Inventory {
                capacity: 28,
                carried_weight: 7.5,
                coins: 1_250,
                items: vec![pod_core::ItemStack {
                    item_id: "iron-pickaxe".into(),
                    display_name: "Iron Pickaxe".into(),
                    quantity: 1,
                    stackable: false,
                }],
            }),
            companion_roster: None,
            encounter: Some(EncounterState {
                encounter_id: 44,
                kind: pod_core::EncounterKind::OpenWorld,
                threat_level: 0.2,
                primary_target: Some(EntityId(9101)),
                active_turn_owner: None,
                capture_allowed: false,
                in_combat: false,
            }),
            cooldowns: Vec::new(),
        },
        visible_entities: vec![VisibleEntity {
            entity_id: EntityId(9101),
            entity_type: "iron_vein".into(),
            position: Vec2::new(323.0, 644.0),
            velocity: Vec2::ZERO,
            rotation: 0.0,
            distance: 5.0,
            relationship: Relationship::Neutral,
            health_fraction: None,
            combat_style: None,
            creature: None,
        }],
        audible_events: vec![AudibleEvent {
            event_type: "pickaxe_swing".into(),
            direction: Vec2::new(0.7, 0.2),
            distance: 4.5,
            intensity: 0.5,
        }],
        messages: vec![AgentMessage {
            from: agent_id,
            content: "frontier miner ready".into(),
            channel: MessageChannel::Team,
        }],
        available_actions: vec!["Move".into(), "GatherResource".into()],
        objectives: vec![Objective {
            id: "mine-iron".into(),
            description: "Collect iron ore for the squad".into(),
            progress: 0.4,
            completed: false,
        }],
    };

    RustSdkStateSnapshot {
        tick: observation.tick,
        elapsed_secs: observation.elapsed_secs,
        runtime_profile: profile,
        self_state: RustSdkSelfStateSnapshot {
            agent_id,
            entity_id: observation.self_state.entity_id,
            position: observation.self_state.position,
            rotation: observation.self_state.rotation,
            velocity: observation.self_state.velocity,
            health: observation.self_state.health,
            max_health: observation.self_state.max_health,
            team: observation.self_state.team,
            combat_loadout: observation.self_state.combat_loadout.clone(),
            skills: observation.self_state.skills.clone(),
            inventory: observation.self_state.inventory.clone(),
            encounter: observation.self_state.encounter.clone(),
        },
        visible_entities: observation
            .visible_entities
            .iter()
            .map(rust_sdk_visible_entity_from_observation)
            .collect(),
        audible_events: observation.audible_events.clone(),
        messages: observation.messages.clone(),
        available_actions: observation.available_actions.clone(),
        objectives: observation.objectives.clone(),
        dialog: Some(RustSdkDialogState {
            speaker: "Trader".into(),
            prompt: "Interested in tools?".into(),
            options: vec!["Show wares".into(), "Not now".into()],
        }),
        shop: Some(RustSdkShopState {
            shop_name: "Frontier Tools".into(),
            offer_count: 12,
            can_buy: true,
            can_sell: false,
        }),
        bank: Some(RustSdkBankState {
            bank_name: "Anchor Vault".into(),
            tab_count: 3,
            item_count: 86,
            can_deposit: true,
            can_withdraw: true,
        }),
        transport_contract: None,
        remote_topology: None,
        latest_tick_telemetry: Some(VersionedTickTelemetry::new(build_rust_sdk_tick_telemetry(
            &observation,
            &Action::GatherResource {
                target: EntityId(9101),
                skill: SkillKind::Mining,
            },
            &[pod_core::AgentToolCallTrace::success(
                1,
                "path_probe",
                "rs-sdk-benchmark",
                12,
                4,
                3,
            )],
        ))),
        replay: None,
    }
}

fn build_rust_sdk_live_smoke_snapshot(entity_id: u64) -> RustSdkStateSnapshot {
    let mut snapshot = build_rust_sdk_gather_snapshot();
    snapshot.tick = 2;
    snapshot.elapsed_secs = 0.3;
    snapshot.self_state.entity_id = EntityId(entity_id);
    snapshot.self_state.position = Vec2::new(128.0, 192.0);
    snapshot.self_state.velocity = Vec2::ZERO;
    snapshot.self_state.encounter = None;
    snapshot.visible_entities.clear();
    snapshot.audible_events.clear();
    snapshot.messages = vec![AgentMessage {
        from: snapshot.self_state.agent_id,
        content: "live generated sdk smoke ready".into(),
        channel: MessageChannel::Direct,
    }];
    snapshot.available_actions = vec!["Idle".into()];
    snapshot.objectives = vec![Objective {
        id: "smoke-live-generated-sdk".into(),
        description: "Prove the repo-owned Rust SDK session facade reaches live reducers."
            .into(),
        progress: 0.0,
        completed: false,
    }];
    snapshot.dialog = None;
    snapshot.shop = None;
    snapshot.bank = None;
    snapshot.transport_contract = Some(RemoteAgentTransportContract::spacetimedb_default(
        snapshot.runtime_profile,
    ));
    snapshot.remote_topology = None;
    snapshot.replay = None;

    let observation = snapshot.to_observation();
    snapshot.latest_tick_telemetry = Some(VersionedTickTelemetry::new(
        build_rust_sdk_tick_telemetry(&observation, &Action::Idle, &[]),
    ));
    snapshot
}

fn build_rust_sdk_tick_telemetry(
    observation: &Observation,
    action: &Action,
    tool_calls: &[pod_core::AgentToolCallTrace],
) -> pod_core::TickTelemetryFrame {
    let mut agent_frame = AgentTelemetryFrame::new(
        observation.tick,
        observation.self_state.agent_id,
        Some(observation.self_state.entity_id),
        observation.self_state.runtime_profile,
        observation.visible_entities.len(),
        observation.audible_events.len(),
        observation.messages.len(),
        observation.available_actions.len(),
        observation.objectives.len(),
        observation.self_state.encounter.clone(),
        None,
    );
    agent_frame.record_action(
        ActionSource::ExternalSubmission,
        ActionLifecycleStage::Submitted,
        action.clone(),
        None,
    );
    agent_frame.record_action(
        ActionSource::ExternalSubmission,
        ActionLifecycleStage::Executed,
        action.clone(),
        None,
    );
    for tool_call in tool_calls {
        agent_frame.record_tool_call(tool_call.clone());
    }

    pod_core::TickTelemetryFrame {
        tick: observation.tick,
        agents: vec![agent_frame],
    }
}

fn rust_sdk_visible_entity_from_observation(
    entity: &VisibleEntity,
) -> RustSdkVisibleEntitySnapshot {
    RustSdkVisibleEntitySnapshot {
        entity_id: entity.entity_id,
        entity_type: entity.entity_type.clone(),
        position: entity.position,
        velocity: entity.velocity,
        rotation: entity.rotation,
        relationship: entity.relationship,
        health_fraction: entity.health_fraction,
        combat_style: entity.combat_style,
        creature: entity.creature.clone(),
    }
}

fn rust_sdk_action_variant_name(action: &Action) -> &'static str {
    match action {
        Action::Move { .. } => "Move",
        Action::Stop => "Stop",
        Action::Rotate { .. } => "Rotate",
        Action::LookAt { .. } => "LookAt",
        Action::Attack | Action::AttackTarget { .. } => "Attack",
        Action::UseAbility { .. } => "UseAbility",
        Action::CaptureCreature { .. } => "CaptureCreature",
        Action::SummonCompanion { .. } => "SummonCompanion",
        Action::CommandCompanion { .. } => "CommandCompanion",
        Action::Interact | Action::InteractWith { .. } => "Interact",
        Action::Pickup { .. } => "Pickup",
        Action::Drop { .. } => "Drop",
        Action::UseItem { .. } => "UseItem",
        Action::GatherResource { .. } => "GatherResource",
        Action::Loot { .. } => "Loot",
        Action::Speak { .. } => "Speak",
        Action::Signal { .. } => "Signal",
        Action::SetAutoRetaliate { .. } => "SetAutoRetaliate",
        Action::Idle => "Idle",
        Action::Spawn { .. } => "Spawn",
    }
}

fn rust_sdk_action_semantic_key(action: &Action) -> &'static str {
    match action {
        Action::Move { .. } => "move",
        Action::Stop => "stop",
        Action::Rotate { .. } => "rotate",
        Action::LookAt { .. } => "look_at",
        Action::Attack | Action::AttackTarget { .. } => "attack",
        Action::UseAbility { .. } => "use_ability",
        Action::CaptureCreature { .. } => "capture",
        Action::SummonCompanion { .. } => "summon",
        Action::CommandCompanion { .. } => "command_companion",
        Action::Interact | Action::InteractWith { .. } => "interact",
        Action::Pickup { .. } => "pickup",
        Action::Drop { .. } => "drop",
        Action::UseItem { .. } => "use_item",
        Action::GatherResource { .. } => "gather_resource",
        Action::Loot { .. } => "loot",
        Action::Speak { .. } => "speak",
        Action::Signal { .. } => "signal",
        Action::SetAutoRetaliate { .. } => "set_auto_retaliate",
        Action::Idle => "idle",
        Action::Spawn { .. } => "spawn",
    }
}

/// One boolean benchmark check for authority-fed topology ingestion.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyFeedCheck {
    pub metric: String,
    pub passed: bool,
    pub expected: String,
    pub observed: String,
}

/// Per-path topology parity report for one resolved world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyFeedWorldPathReport {
    pub resolved_world_id: Option<String>,
    pub resolved_world_matches: bool,
    pub quest_binding_matches: bool,
    pub applied_world_state_matches: bool,
    pub evaluation_matches: bool,
    pub world_tournament_orchestration_matches: bool,
    pub tournament_control_plane_matches: bool,
    pub tournament_orchestration_matches: bool,
}

/// Combined authority-row and generated-runtime parity report for one world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyFeedWorldReport {
    pub world_id: String,
    pub authority_row: TopologyFeedWorldPathReport,
    pub generated_runtime: TopologyFeedWorldPathReport,
}

/// Benchmark artifact for replaying a topology bundle through pod-net ingestion paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopologyFeedMeasurementsReport {
    pub schema_version: u32,
    pub scenario_id: String,
    pub profile_id: String,
    pub world_count: usize,
    pub worlds: Vec<TopologyFeedWorldReport>,
    pub checks: Vec<TopologyFeedCheck>,
}

impl TopologyFeedMeasurementsReport {
    pub fn all_checks_passed(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }
}

/// Configuration for exercising the generated path through the real SpacetimeDB SDK.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveGeneratedSdkTopologyFeedConfig {
    pub host: String,
    pub auth_token: Option<String>,
    pub timeout_ms: u64,
    pub poll_interval_ms: u64,
}

impl Default for LiveGeneratedSdkTopologyFeedConfig {
    fn default() -> Self {
        Self {
            host: "http://localhost:3000".into(),
            auth_token: None,
            timeout_ms: 5_000,
            poll_interval_ms: 10,
        }
    }
}

/// Select how the generated half of the topology benchmark should run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TopologyFeedGeneratedRuntimeMode {
    /// Deterministic command-driven generated runtime used by CI and moat.
    #[default]
    DeterministicBinding,
    /// Real generated SDK runtime backed by SpacetimeDB's generated Rust bindings.
    LiveSdk(LiveGeneratedSdkTopologyFeedConfig),
}

/// Optional knobs for topology feed benchmarking.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TopologyFeedMeasurementsOptions {
    pub generated_runtime_mode: TopologyFeedGeneratedRuntimeMode,
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
    /// TOON document ingest failed.
    Document(String),
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
            Self::Document(msg) => write!(f, "SpacetimeDB document error: {msg}"),
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
            StdbError::DocumentError(msg) => Self::Document(msg),
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
    remote_agent_contract: Option<RemoteAgentTransportContract>,
    remote_agent_status: RemoteAgentRuntimeStatus,
    local_snapshot: Option<WorldSnapshot>,
    last_debug_telemetry_json: Option<String>,
    pending_debug_documents: Vec<String>,
    render_buffer: SnapshotInterpolationBuffer,
    render_clock: RenderClock,
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
            remote_agent_contract: None,
            remote_agent_status: RemoteAgentRuntimeStatus::default(),
            local_snapshot: None,
            last_debug_telemetry_json: None,
            pending_debug_documents: Vec::new(),
            render_buffer: SnapshotInterpolationBuffer::default(),
            render_clock: RenderClock::default(),
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
        self.connect_remote_agent(entity_id, AgentType::LlmAgent)
    }

    /// Claim a remote agent slot on an entity and install the gameplay contract
    /// used by the SpacetimeDB observation/action path.
    pub fn connect_remote_agent(
        &mut self,
        entity_id: u64,
        agent_type: AgentType,
    ) -> Result<(), StdbClientError> {
        let display_name = self.inner.config().player_name.clone();
        self.inner
            .call_connect_agent(entity_id, agent_type.clone(), display_name)
            .map_err(StdbClientError::from)?;
        let profile = AgentRuntimeProfile::for_agent_type(core_agent_type_from_stdb(&agent_type));
        self.remote_agent_contract =
            Some(RemoteAgentTransportContract::spacetimedb_default(profile));
        self.remote_agent_status = RemoteAgentRuntimeStatus {
            last_authoritative_tick: self.inner.current_tick(),
            last_observation_tick: self.inner.latest_observation_tick(entity_id),
            stale_observation_ticks: self
                .inner
                .latest_observation_tick(entity_id)
                .map(|tick| self.current_tick_or_zero().saturating_sub(tick))
                .unwrap_or(0),
            ..RemoteAgentRuntimeStatus::default()
        };
        self.subscribe_for_player(entity_id)?;
        Ok(())
    }

    /// Queue an action for the next [`send_actions`](Self::send_actions) call.
    pub fn queue_action(&mut self, action: Action) {
        self.pending_actions.push(action);
        self.remote_agent_status.pending_action_count = self.pending_actions.len() as u32;
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

        let entity_id = self.inner.controlled_entity().ok_or_else(|| {
            StdbClientError::InvalidState("No controlled entity is connected".into())
        })?;
        self.remote_agent_status.pending_action_count = self.pending_actions.len() as u32;
        self.remote_agent_status.last_authoritative_tick = Some(self.current_tick_or_zero());

        if let Some(contract) = self.remote_agent_contract.as_ref() {
            let action_count = self.pending_actions.len() as u32;
            if action_count > contract.action_budget.max_actions_per_tick {
                return self.reject_remote_actions(
                    RemoteAgentFallbackReason::ActionBudgetExceeded,
                    format!(
                        "Remote agent queued {action_count} actions but contract allows {} per tick",
                        contract.action_budget.max_actions_per_tick
                    ),
                );
            }

            let current_tick = self.current_tick_or_zero();
            let Some(last_observation_tick) = self.inner.latest_observation_tick(entity_id) else {
                return self.reject_remote_actions(
                    RemoteAgentFallbackReason::ObservationMissing,
                    "Remote agent has no authoritative observation yet".into(),
                );
            };

            let stale_ticks = current_tick.saturating_sub(last_observation_tick);
            self.remote_agent_status.last_observation_tick = Some(last_observation_tick);
            self.remote_agent_status.stale_observation_ticks = stale_ticks;

            if stale_ticks > contract.heartbeat.timeout_after_ticks {
                return self.reject_remote_actions(
                    RemoteAgentFallbackReason::HeartbeatTimedOut,
                    format!(
                        "Remote agent observation timed out after {stale_ticks} ticks (limit {})",
                        contract.heartbeat.timeout_after_ticks
                    ),
                );
            }

            if stale_ticks > contract.observation_budget.stale_after_ticks {
                return self.reject_remote_actions(
                    RemoteAgentFallbackReason::ObservationStale,
                    format!(
                        "Remote agent observation is {stale_ticks} ticks old (stale after {})",
                        contract.observation_budget.stale_after_ticks
                    ),
                );
            }
        }

        let actions: Vec<Action> = self.pending_actions.drain(..).collect();

        for action in &actions {
            let submitted = convert_action(entity_id, action);
            self.inner
                .call_submit_action(&submitted)
                .map_err(StdbClientError::from)?;
        }

        self.remote_agent_status.pending_action_count = 0;
        self.remote_agent_status.clear_fallback();

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
                        self.ingest_local_snapshot();
                        self.welcome_sent = true;

                        if let Some(client_id) = self.client_id {
                            messages.push(ServerMessage::Welcome {
                                client_id,
                                reconnect_token: self.reconnect_token,
                                tick,
                                controlled_entity: None,
                                acknowledged_action_tick: None,
                                authoritative_digest: snapshot.digest(),
                                snapshot,
                            });
                        }
                    }
                }
                StdbEvent::RemoteTopologyUpdated { .. } => {
                    if !self.welcome_sent {
                        continue;
                    }
                    let tick = self.current_tick_or_zero();
                    let snapshot = self.build_world_snapshot();
                    let updated_entities = snapshot.entities.clone();
                    let population = snapshot.population.clone();
                    let authoritative_digest = snapshot.digest();
                    self.local_snapshot = Some(snapshot);
                    self.ingest_local_snapshot();

                    messages.push(ServerMessage::StateDelta {
                        tick,
                        acknowledged_action_tick: None,
                        authoritative_digest,
                        is_full_snapshot: true,
                        delta: StateDelta {
                            tick,
                            updated: updated_entities,
                            destroyed: Vec::new(),
                            population,
                        },
                    });
                }

                StdbEvent::ConnectError { message } => {
                    log::error!("[pod-net stdb] Connection failed: {message}");
                    messages.push(ServerMessage::Rejected { reason: message });
                }

                StdbEvent::Disconnected { reason } => {
                    log::info!("[pod-net stdb] Disconnected: {reason}");
                    self.client_id = None;
                    self.welcome_sent = false;
                    self.clear_presentation_state();
                }

                // ── Entity lifecycle → StateDelta ──
                StdbEvent::EntityInserted { entity_id }
                | StdbEvent::EntityUpdated { entity_id } => {
                    if let Some(cached) = self.inner.entity(entity_id) {
                        let snap = self.entity_to_snapshot(cached);
                        let tick = self.current_tick_or_zero();
                        if tick < self.last_emitted_tick {
                            continue;
                        }
                        self.upsert_local_snapshot(tick, &snap);
                        self.ingest_local_snapshot();
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
                                population: pod_core::WorldPopulationState {
                                    tick,
                                    ..Default::default()
                                },
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
                    self.ingest_local_snapshot();
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
                            population: pod_core::WorldPopulationState {
                                tick,
                                ..Default::default()
                            },
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
                StdbEvent::AgentTelemetryTickReceived { frame_json, .. } => {
                    self.last_debug_telemetry_json = Some(frame_json.clone());
                    self.pending_debug_documents.push(frame_json.clone());
                    messages.push(ServerMessage::DebugDocument {
                        document: frame_json,
                    });
                }
                StdbEvent::AgentToolCallEventReceived { document, .. } => {
                    self.pending_debug_documents.push(document.clone());
                    messages.push(ServerMessage::DebugDocument { document });
                }
                StdbEvent::AgentTickRollupReceived { document, .. } => {
                    self.pending_debug_documents.push(document.clone());
                    messages.push(ServerMessage::DebugDocument { document });
                }
                StdbEvent::FocusedEntityDebugSummaryReceived { document, .. } => {
                    self.pending_debug_documents.push(document.clone());
                    messages.push(ServerMessage::DebugDocument { document });
                }
                StdbEvent::RemoteTopologyDocumentReceived { document } => {
                    self.pending_debug_documents.push(document.clone());
                    messages.push(ServerMessage::DebugDocument { document });
                }

                // ── Tick advancement ──
                StdbEvent::TickAdvanced { new_tick, .. } => {
                    self.update_remote_agent_tick(new_tick);
                    self.last_emitted_tick = self.last_emitted_tick.max(new_tick);
                    if let Some(ref mut local) = self.local_snapshot {
                        local.tick = new_tick;
                    }
                    self.ingest_local_snapshot();
                }

                StdbEvent::WorldStateUpdated { tick, .. } => {
                    self.update_remote_agent_tick(tick);
                    self.last_emitted_tick = self.last_emitted_tick.max(tick);
                    if let Some(ref mut local) = self.local_snapshot {
                        local.tick = tick;
                    }
                    self.ingest_local_snapshot();
                }

                // ── Internal events (no ServerMessage equivalent) ──
                StdbEvent::ObservationReceived {
                    tick,
                    observer_entity_id,
                    ..
                } => {
                    if Some(observer_entity_id) == self.inner.controlled_entity() {
                        self.remote_agent_status.last_observation_tick = Some(tick);
                        self.remote_agent_status.last_authoritative_tick = Some(
                            self.remote_agent_status
                                .last_authoritative_tick
                                .unwrap_or(tick)
                                .max(tick),
                        );
                        self.remote_agent_status.stale_observation_ticks = self
                            .remote_agent_status
                            .last_authoritative_tick
                            .unwrap_or(tick)
                            .saturating_sub(tick);
                        self.remote_agent_status.clear_fallback();
                    }
                }
                StdbEvent::ReducerCallSuccess { .. } | StdbEvent::ReducerCallError { .. } => {}
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
        self.remote_agent_contract = None;
        self.remote_agent_status = RemoteAgentRuntimeStatus::default();
        self.pending_actions.clear();
        self.last_debug_telemetry_json = None;
        self.pending_debug_documents.clear();
        self.subscriptions.reset_connection();
        self.clear_presentation_state();
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

    /// Configure subscriptions for editor dashboards with raw debug telemetry.
    pub fn subscribe_as_editor_with_debug_telemetry(&mut self) -> Result<bool, StdbClientError> {
        self.subscriptions.set_editor_debug();
        self.subscriptions
            .ensure_subscriptions_applied(&mut self.inner)
    }

    /// Configure subscriptions for editor dashboards with raw debug telemetry
    /// scoped to a selected set of agent entities.
    pub fn subscribe_as_editor_with_debug_telemetry_for_entities(
        &mut self,
        entity_ids: impl IntoIterator<Item = u64>,
    ) -> Result<bool, StdbClientError> {
        let entity_ids = entity_ids.into_iter().collect::<Vec<_>>();
        self.subscriptions.set_editor_debug_entities(entity_ids);
        self.subscriptions
            .ensure_subscriptions_applied(&mut self.inner)
    }

    /// Mirror editor selection into the active debug telemetry subscription.
    ///
    /// `Some(entity_id)` narrows raw telemetry/tool/rollup streams to that
    /// entity while retaining editor world-state tables. `None` drops back to
    /// world-state-only editor mode.
    pub fn sync_selected_entity_debug_focus(
        &mut self,
        entity_id: Option<u64>,
    ) -> Result<bool, StdbClientError> {
        match entity_id {
            Some(entity_id) => {
                self.subscribe_as_editor_with_debug_telemetry_for_entities([entity_id])
            }
            None => self.subscribe_as_editor(),
        }
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

    /// Sample a smoothed presentation snapshot for rendering.
    pub fn presentation_snapshot(
        &mut self,
        frame_delta_seconds: f32,
    ) -> Option<InterpolatedSnapshot> {
        let latest_tick = self.render_buffer.latest_tick()?;
        let target_tick = self.render_clock.advance(latest_tick, frame_delta_seconds);
        let sampled = self.render_buffer.sample(target_tick)?;
        Some(compose_presentation_snapshot(
            sampled,
            self.local_snapshot.as_ref(),
            self.inner.controlled_entity(),
        ))
    }

    /// Get the assigned client ID (available after `Connected` event).
    pub fn client_id(&self) -> Option<ClientId> {
        self.client_id
    }

    /// Whether the client is connected to SpacetimeDB.
    pub fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    /// Current presentation tick after interpolation/catch-up correction.
    pub fn presentation_tick(&self) -> Option<f32> {
        self.render_clock.current_tick()
    }

    pub fn last_debug_telemetry_json(&self) -> Option<&str> {
        self.last_debug_telemetry_json.as_deref()
    }

    pub fn last_debug_telemetry_document(&self) -> Option<&str> {
        self.last_debug_telemetry_json()
    }

    /// Inspect the newest retained TOON document across live debug telemetry,
    /// tool-call events, rollups, replays, and shard incidents.
    pub fn last_debug_document(&self) -> Option<&str> {
        self.pending_debug_documents
            .last()
            .map(String::as_str)
            .or_else(|| self.last_debug_telemetry_document())
    }

    /// Drain all pending TOON debug documents gathered since the last call.
    pub fn drain_debug_documents(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_debug_documents)
    }

    /// Inject a TOON debug document into the live stream surface. This lets
    /// shard tooling forward replay files and incident summaries through the
    /// same browser/editor consumer path as tick telemetry.
    pub fn push_debug_document(&mut self, document: String) {
        self.pending_debug_documents.push(document);
    }

    /// Inspect the local rollback/replay path from a chosen retained tick.
    ///
    /// SpacetimeDB clients currently replay zero local prediction batches here;
    /// this still exposes retained authoritative history for rewind tooling and
    /// keeps the surface aligned with direct-connect clients.
    pub fn rollback_preview(&self, rewind_tick: Option<u64>) -> Option<RollbackPreview> {
        let rewind_tick =
            rewind_tick.or_else(|| self.local_snapshot.as_ref().map(|snapshot| snapshot.tick))?;
        build_rollback_preview(
            &self.render_buffer,
            rewind_tick,
            self.inner.controlled_entity(),
            &[],
        )
    }

    /// Rewind to the newest retained authoritative snapshot at or before `tick`.
    pub fn rewind_authoritative_snapshot(&self, tick: u64) -> Option<WorldSnapshot> {
        self.render_buffer.rewind_to(tick)
    }

    /// Summarize current presentation drift and retained-history state.
    pub fn catch_up_diagnostics(&self) -> CatchUpDiagnostics {
        build_catch_up_diagnostics(
            &self.render_buffer,
            self.local_snapshot.as_ref(),
            self.local_snapshot.as_ref(),
            self.inner.controlled_entity(),
            &[],
            &self.render_clock,
            &RecoveryRequestState::default(),
        )
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

    /// Inspect the active remote-agent transport contract, if one is installed.
    pub fn remote_agent_contract(&self) -> Option<&RemoteAgentTransportContract> {
        self.remote_agent_contract.as_ref()
    }

    /// Inspect the live remote-agent runtime status.
    pub fn remote_agent_status(&self) -> &RemoteAgentRuntimeStatus {
        &self.remote_agent_status
    }

    /// Install the command-driven generated binding runtime on the underlying
    /// [`StdbClient`] and return the public binding endpoint.
    pub fn install_generated_binding_runtime(&mut self) -> GeneratedBindingEndpoint {
        self.inner.install_generated_binding_runtime()
    }

    /// Install the real generated SpacetimeDB SDK runtime on the wrapped client.
    pub fn install_generated_sdk_runtime(&mut self) {
        self.inner.install_generated_sdk_runtime();
    }

    /// Apply a shared multi-world topology artifact to the underlying SpacetimeDB client.
    pub fn apply_remote_topology(
        &mut self,
        topology: RemoteTopologyBundle,
    ) -> Result<(), StdbClientError> {
        self.inner.apply_remote_topology(topology);
        Ok(())
    }

    /// Apply the repo-owned Rust SDK handoff bundle to the wrapped client.
    ///
    /// This installs any published transport contract and forwards replay
    /// documents through the existing debug-document surface while delegating
    /// observation/topology/telemetry ingestion to `pod-stdb`.
    pub fn apply_rust_sdk_handoff_artifact(
        &mut self,
        artifact: RustSdkHandoffArtifact,
    ) -> Result<(), StdbClientError> {
        let observation_tick = artifact.observation.payload.tick;
        let transport_contract = artifact.transport_contract.clone();
        let replay_document = artifact
            .replay
            .as_ref()
            .map(|replay| replay.to_toon_document());

        self.inner.apply_rust_sdk_handoff_artifact(artifact)?;

        if let Some(contract) = transport_contract {
            self.remote_agent_contract = Some(contract);
        }

        let authoritative_tick = self
            .remote_agent_status
            .last_authoritative_tick
            .unwrap_or(observation_tick)
            .max(observation_tick);
        self.remote_agent_status.last_authoritative_tick = Some(authoritative_tick);
        self.remote_agent_status.last_observation_tick = Some(observation_tick);
        self.remote_agent_status.stale_observation_ticks =
            authoritative_tick.saturating_sub(observation_tick);
        if self.remote_agent_status.stale_observation_ticks == 0 {
            self.remote_agent_status.clear_fallback();
        }

        if let Some(document) = replay_document {
            self.push_debug_document(document);
        }

        Ok(())
    }

    /// Apply an authority-published remote-topology feed row from SpacetimeDB.
    pub fn receive_remote_topology_document_row(
        &mut self,
        row_id: u64,
        generated_at_unix_ms: u64,
        scenario_id: impl Into<String>,
        profile_id: impl Into<String>,
        topology_json: impl Into<String>,
    ) -> Result<(), StdbClientError> {
        self.inner.receive_remote_topology_document_row(
            row_id,
            generated_at_unix_ms,
            scenario_id.into(),
            profile_id.into(),
            topology_json.into(),
        )?;
        Ok(())
    }

    /// Apply an authority TOON document through the underlying SpacetimeDB client.
    pub fn apply_debug_document(
        &mut self,
        document: impl Into<String>,
    ) -> Result<(), StdbClientError> {
        self.inner.receive_debug_document(document.into())?;
        Ok(())
    }

    /// Apply a shared multi-world topology artifact received as an authority TOON document.
    pub fn apply_remote_topology_document(
        &mut self,
        document: impl Into<String>,
    ) -> Result<(), StdbClientError> {
        self.apply_debug_document(document)
    }

    /// Access the last applied multi-world topology artifact, if any.
    pub fn remote_topology(&self) -> Option<&RemoteTopologyBundle> {
        self.inner.remote_topology()
    }

    /// Resolve the active remote world id for this client.
    pub fn remote_world_id(&self) -> Option<&str> {
        self.inner.resolved_remote_world_id()
    }

    /// Resolve the applied cross-world state summary for this client's active world.
    pub fn remote_applied_world_state(&self) -> Option<&AppliedWorldStateSummary> {
        self.inner.resolved_remote_applied_world_state()
    }

    /// Resolve the admitted roster summary for this client's active world.
    pub fn remote_world_admissions(&self) -> Option<&pod_core::WorldAdmissionSummary> {
        self.inner.resolved_remote_world_admissions()
    }

    /// Resolve the admitted roster/controller mix for this client's active world.
    pub fn remote_world_control_plane(&self) -> Option<&pod_core::WorldControlPlaneSummary> {
        self.inner.resolved_remote_world_control_plane()
    }

    /// Resolve the tournament-wide standings/control summary from the active topology.
    pub fn remote_tournament_control_plane(
        &self,
    ) -> Option<&pod_core::TournamentControlPlaneSummary> {
        self.inner.resolved_remote_tournament_control_plane()
    }

    /// Resolve the tournament-wide orchestration summary from the active topology.
    pub fn remote_tournament_orchestration(
        &self,
    ) -> Option<&pod_core::TournamentOrchestrationSummary> {
        self.inner.resolved_remote_tournament_orchestration()
    }

    /// Resolve the active world's tournament-orchestration summary.
    pub fn remote_world_tournament_orchestration(
        &self,
    ) -> Option<&pod_core::WorldTournamentOrchestrationSummary> {
        self.inner.resolved_remote_world_tournament_orchestration()
    }

    /// Resolve the replay/evaluation summary for this client's active world.
    pub fn remote_world_evaluation(&self) -> Option<&WorldEvaluationSummary> {
        self.inner.resolved_remote_world_evaluation()
    }

    // ── Internal helpers ──

    fn current_tick_or_zero(&self) -> u64 {
        self.inner.current_tick().unwrap_or(0)
    }

    fn reject_remote_actions(
        &mut self,
        reason: RemoteAgentFallbackReason,
        message: String,
    ) -> Result<(), StdbClientError> {
        match reason {
            RemoteAgentFallbackReason::ActionBudgetExceeded => {
                self.remote_agent_status.budget_overflow_rejections += 1;
            }
            RemoteAgentFallbackReason::HeartbeatTimedOut => {
                self.remote_agent_status.timeout_rejections += 1;
            }
            RemoteAgentFallbackReason::ObservationMissing
            | RemoteAgentFallbackReason::ObservationStale => {
                self.remote_agent_status.stale_action_rejections += 1;
            }
        }
        self.remote_agent_status.activate_fallback(reason);
        self.pending_actions.clear();
        self.remote_agent_status.pending_action_count = 0;
        Err(StdbClientError::InvalidState(message))
    }

    fn update_remote_agent_tick(&mut self, tick: u64) {
        self.remote_agent_status.last_authoritative_tick = Some(
            self.remote_agent_status
                .last_authoritative_tick
                .unwrap_or(tick)
                .max(tick),
        );
        if let Some(last_observation_tick) = self.remote_agent_status.last_observation_tick {
            self.remote_agent_status.stale_observation_ticks =
                tick.saturating_sub(last_observation_tick);
        }
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
            .map(|entity| self.entity_to_snapshot(entity))
            .collect();

        WorldSnapshot {
            tick: self.current_tick_or_zero(),
            entities,
            population: pod_core::WorldPopulationState {
                tick: self.current_tick_or_zero(),
                ..Default::default()
            },
        }
    }

    fn entity_to_snapshot(&self, cached: &CachedEntity) -> EntitySnapshot {
        entity_to_snapshot(
            cached,
            self.inner.resolved_remote_world_id(),
            self.inner.resolved_remote_world().map(|world| world.role),
            self.inner.resolved_remote_team_key(cached.team_id),
            self.inner
                .resolved_remote_world_quest_binding()
                .map(|binding| binding.quest_graph_ids.as_slice()),
        )
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

    fn ingest_local_snapshot(&mut self) {
        if let Some(snapshot) = self.local_snapshot.as_ref() {
            self.render_buffer.push(snapshot.clone());
        }
    }

    fn clear_presentation_state(&mut self) {
        self.local_snapshot = None;
        self.render_buffer.clear();
        self.render_clock.reset();
    }
}

// ============================================================
// TYPE CONVERSIONS
// ============================================================

/// Convert a cached SpacetimeDB entity into a pod-net [`EntitySnapshot`].
fn entity_to_snapshot(
    cached: &CachedEntity,
    world_id: Option<&str>,
    world_role: Option<pod_core::WorldRealityRole>,
    team_key: Option<String>,
    world_active_quest_graph_ids: Option<&[String]>,
) -> EntitySnapshot {
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
        metadata: EntityMetadataSnapshot {
            team_id: cached.team_id,
            team_key,
            world_id: world_id.map(ToOwned::to_owned),
            world_role,
            world_active_quest_graph_ids: world_active_quest_graph_ids
                .map(|quest_ids| quest_ids.to_vec())
                .unwrap_or_default(),
            ..EntityMetadataSnapshot::default()
        },
    }
}

fn core_agent_type_from_stdb(agent_type: &AgentType) -> CoreAgentType {
    match agent_type {
        AgentType::Human => CoreAgentType::Human,
        AgentType::LlmAgent => CoreAgentType::LlmAgent,
        AgentType::NeuralAgent => CoreAgentType::NeuralAgent,
        AgentType::ScriptedNpc => CoreAgentType::ScriptedNpc,
        AgentType::System => CoreAgentType::System,
    }
}

fn stdb_agent_type_from_core(agent_type: CoreAgentType) -> AgentType {
    match agent_type {
        CoreAgentType::Human => AgentType::Human,
        CoreAgentType::LlmAgent => AgentType::LlmAgent,
        CoreAgentType::NeuralAgent => AgentType::NeuralAgent,
        CoreAgentType::ScriptedNpc => AgentType::ScriptedNpc,
        CoreAgentType::System => AgentType::System,
    }
}

fn rust_sdk_action_from_plan(plan: &RustSdkActionPlan) -> Action {
    match &plan.intent {
        RustSdkActionIntent::MoveDirection { direction } => Action::Move {
            direction: *direction,
        },
        RustSdkActionIntent::Stop => Action::Stop,
        RustSdkActionIntent::Rotate { angle } => Action::Rotate { angle: *angle },
        RustSdkActionIntent::LookAtPosition { target } => Action::LookAt { target: *target },
        RustSdkActionIntent::AttackCurrentTarget => Action::Attack,
        RustSdkActionIntent::AttackEntity { entity_id } => Action::AttackTarget {
            target: EntityId(*entity_id),
        },
        RustSdkActionIntent::UseAbility { slot, target } => Action::UseAbility {
            slot: *slot,
            target: target.clone(),
        },
        RustSdkActionIntent::CaptureCreature {
            entity_id,
            tool_slot,
        } => Action::CaptureCreature {
            target: EntityId(*entity_id),
            tool_slot: *tool_slot,
        },
        RustSdkActionIntent::SummonCompanion { slot } => Action::SummonCompanion { slot: *slot },
        RustSdkActionIntent::CommandCompanion {
            slot,
            command,
            target_entity_id,
        } => Action::CommandCompanion {
            slot: *slot,
            command: *command,
            target: target_entity_id.map(EntityId),
        },
        RustSdkActionIntent::InteractNearest => Action::Interact,
        RustSdkActionIntent::InteractEntity { entity_id } => Action::InteractWith {
            target: EntityId(*entity_id),
        },
        RustSdkActionIntent::PickupEntity { entity_id } => Action::Pickup {
            target: EntityId(*entity_id),
        },
        RustSdkActionIntent::DropInventorySlot { slot } => Action::Drop { slot: *slot },
        RustSdkActionIntent::UseInventorySlot { slot } => Action::UseItem { slot: *slot },
        RustSdkActionIntent::GatherEntity { entity_id, skill } => Action::GatherResource {
            target: EntityId(*entity_id),
            skill: *skill,
        },
        RustSdkActionIntent::LootEntity { entity_id } => Action::Loot {
            target: EntityId(*entity_id),
        },
        RustSdkActionIntent::Speak { message, volume } => Action::Speak {
            message: message.clone(),
            volume: *volume,
        },
        RustSdkActionIntent::Signal { signal_type, data } => Action::Signal {
            signal_type: signal_type.clone(),
            data: data.clone(),
        },
        RustSdkActionIntent::SetAutoRetaliate { enabled } => {
            Action::SetAutoRetaliate { enabled: *enabled }
        }
        RustSdkActionIntent::Idle => Action::Idle,
    }
}

fn upsert_rust_sdk_state_snapshot_entities(
    client: &mut StdbClient,
    snapshot: &RustSdkStateSnapshot,
) {
    let mut self_entity = CachedEntity::from_entity(
        snapshot.self_state.entity_id.0,
        Some(stdb_agent_type_from_core(
            snapshot.runtime_profile.agent_type,
        )),
        true,
    );
    self_entity.pos_x = Some(snapshot.self_state.position.x);
    self_entity.pos_y = Some(snapshot.self_state.position.y);
    self_entity.rotation = Some(snapshot.self_state.rotation);
    self_entity.vel_x = Some(snapshot.self_state.velocity.x);
    self_entity.vel_y = Some(snapshot.self_state.velocity.y);
    self_entity.health = snapshot.self_state.health;
    self_entity.max_health = snapshot.self_state.max_health;
    self_entity.team_id = team_id_from_core(snapshot.self_state.team);
    self_entity.max_speed = Some(200.0);
    client.upsert_entity(self_entity);

    for entity in &snapshot.visible_entities {
        let mut cached = CachedEntity::from_entity(entity.entity_id.0, None, true);
        cached.pos_x = Some(entity.position.x);
        cached.pos_y = Some(entity.position.y);
        cached.rotation = Some(entity.rotation);
        cached.vel_x = Some(entity.velocity.x);
        cached.vel_y = Some(entity.velocity.y);
        cached.name = Some(entity.entity_type.clone());
        if let Some(fraction) = entity.health_fraction {
            cached.max_health = Some(100.0);
            cached.health = Some((fraction.clamp(0.0, 1.0)) * 100.0);
        }
        client.upsert_entity(cached);
    }
}

fn team_id_from_core(team: Team) -> Option<u8> {
    match team {
        Team::None => None,
        Team::Team(team_id) => Some(team_id),
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

fn build_topology_feed_check(
    metric: impl Into<String>,
    passed: bool,
    expected: impl Into<String>,
    observed: impl Into<String>,
) -> TopologyFeedCheck {
    TopologyFeedCheck {
        metric: metric.into(),
        passed,
        expected: expected.into(),
        observed: observed.into(),
    }
}

fn build_topology_feed_world_path_report(
    client: &SpacetimeDBClient,
    topology: &RemoteTopologyBundle,
    world_id: &str,
) -> TopologyFeedWorldPathReport {
    let expected_binding = topology
        .world_quest_bindings
        .iter()
        .find(|binding| binding.world_id == world_id);
    let expected_applied_state = topology
        .applied_world_states
        .iter()
        .find(|state| state.world_id == world_id);
    let expected_evaluation = topology
        .evaluation
        .worlds
        .iter()
        .find(|state| state.world_id == world_id);
    let expected_world_orchestration = topology
        .tournament_orchestration
        .worlds
        .iter()
        .find(|state| state.world_id == world_id);

    TopologyFeedWorldPathReport {
        resolved_world_id: client.remote_world_id().map(str::to_owned),
        resolved_world_matches: client.remote_world_id() == Some(world_id),
        quest_binding_matches: client.inner().resolved_remote_world_quest_binding()
            == expected_binding,
        applied_world_state_matches: client.remote_applied_world_state() == expected_applied_state,
        evaluation_matches: client.remote_world_evaluation() == expected_evaluation,
        world_tournament_orchestration_matches: client.remote_world_tournament_orchestration()
            == expected_world_orchestration,
        tournament_control_plane_matches: client.remote_tournament_control_plane()
            == Some(&topology.tournament_control_plane),
        tournament_orchestration_matches: client.remote_tournament_orchestration()
            == Some(&topology.tournament_orchestration),
    }
}

fn collect_topology_feed_checks(
    world_id: &str,
    path_name: &str,
    path: &TopologyFeedWorldPathReport,
) -> Vec<TopologyFeedCheck> {
    let expected_world_id = world_id.to_string();
    vec![
        build_topology_feed_check(
            format!("{path_name}.{world_id}.resolved_world_matches"),
            path.resolved_world_matches,
            "true",
            serde_json::to_string(&path.resolved_world_id)
                .unwrap_or_else(|_| format!("{:?}", path.resolved_world_id)),
        ),
        build_topology_feed_check(
            format!("{path_name}.{world_id}.quest_binding_matches"),
            path.quest_binding_matches,
            "true",
            path.quest_binding_matches.to_string(),
        ),
        build_topology_feed_check(
            format!("{path_name}.{world_id}.applied_world_state_matches"),
            path.applied_world_state_matches,
            "true",
            path.applied_world_state_matches.to_string(),
        ),
        build_topology_feed_check(
            format!("{path_name}.{world_id}.evaluation_matches"),
            path.evaluation_matches,
            "true",
            path.evaluation_matches.to_string(),
        ),
        build_topology_feed_check(
            format!("{path_name}.{world_id}.world_tournament_orchestration_matches"),
            path.world_tournament_orchestration_matches,
            "true",
            path.world_tournament_orchestration_matches.to_string(),
        ),
        build_topology_feed_check(
            format!("{path_name}.{world_id}.tournament_control_plane_matches"),
            path.tournament_control_plane_matches,
            "true",
            path.tournament_control_plane_matches.to_string(),
        ),
        build_topology_feed_check(
            format!("{path_name}.{world_id}.tournament_orchestration_matches"),
            path.tournament_orchestration_matches,
            "true",
            path.tournament_orchestration_matches.to_string(),
        ),
        build_topology_feed_check(
            format!("{path_name}.{world_id}.resolved_world_id"),
            path.resolved_world_id.as_deref() == Some(world_id),
            expected_world_id,
            path.resolved_world_id
                .clone()
                .unwrap_or_else(|| "null".into()),
        ),
    ]
}

fn build_deterministic_generated_topology_feed_world_path_report(
    topology: &RemoteTopologyBundle,
    topology_json: &str,
    world_id: &str,
    row_id: u64,
) -> Result<TopologyFeedWorldPathReport, StdbClientError> {
    let mut generated_client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
        db_name: world_id.to_string(),
        connection_mode: StdbConnectionMode::Generated,
        ..Default::default()
    });
    let endpoint = generated_client.install_generated_binding_runtime();
    let callbacks = endpoint.callbacks();
    generated_client.connect()?;
    let commands = endpoint.drain_commands();
    assert!(
        matches!(
            commands.as_slice(),
            [GeneratedBindingCommand::Connect { config }]
                if config.db_name == world_id
                    && matches!(config.connection_mode, StdbConnectionMode::Generated)
        ),
        "generated topology feed benchmark should request one connect command for {world_id}"
    );
    callbacks.connected(vec![7; 16], "tok-generated");
    generated_client.inner_mut().frame_tick();
    generated_client.subscribe_custom(vec!["SELECT * FROM remote_topology_document".into()])?;
    let commands = endpoint.drain_commands();
    assert!(
        matches!(
            commands.as_slice(),
            [GeneratedBindingCommand::Subscribe { queries }]
                if queries == &vec!["SELECT * FROM remote_topology_document".to_string()]
        ),
        "generated topology feed benchmark should request the topology subscription for {world_id}"
    );
    callbacks.subscription_applied();
    generated_client.inner_mut().frame_tick();
    callbacks.remote_topology_document_insert(GeneratedRemoteTopologyDocumentRow {
        row_id,
        generated_at_unix_ms: u64::try_from(topology.generated_at_unix_ms)
            .map_err(|_| StdbClientError::Subscription("topology timestamp exceeds u64".into()))?,
        scenario_id: topology.scenario_id.clone(),
        profile_id: topology.profile_id.clone(),
        world_count: topology.worlds.len() as u32,
        team_count: topology.teams.len() as u32,
        topology_json: topology_json.to_string(),
    });
    generated_client.inner_mut().frame_tick();
    Ok(build_topology_feed_world_path_report(
        &generated_client,
        topology,
        world_id,
    ))
}

fn wait_for_live_generated_client_ready(
    client: &mut SpacetimeDBClient,
    config: &LiveGeneratedSdkTopologyFeedConfig,
) -> Result<(), StdbClientError> {
    let timeout = Duration::from_millis(config.timeout_ms);
    let poll_interval = Duration::from_millis(config.poll_interval_ms);
    let deadline = Instant::now() + timeout;

    loop {
        let _ = client.poll_updates();
        if client.is_connected() && client.local_snapshot().is_some() {
            return Ok(());
        }

        match client.inner().connection_state() {
            ConnectionState::Error(message) => {
                return Err(StdbClientError::Connection(message.clone()));
            }
            ConnectionState::Disconnected => {
                return Err(StdbClientError::Connection(
                    "generated SDK runtime disconnected before subscriptions applied".into(),
                ));
            }
            _ => {}
        }

        if Instant::now() >= deadline {
            return Err(StdbClientError::Connection(
                "timed out waiting for generated SDK runtime to connect and apply subscriptions"
                    .into(),
            ));
        }

        sleep(poll_interval);
    }
}

fn connect_live_generated_sdk_publisher(
    world_id: &str,
    config: &LiveGeneratedSdkTopologyFeedConfig,
) -> Result<module_bindings::DbConnection, StdbClientError> {
    let connected = Arc::new(Mutex::new(false));
    let connect_error = Arc::new(Mutex::new(None::<String>));
    let disconnect_reason = Arc::new(Mutex::new(None::<String>));

    let connection = module_bindings::DbConnection::builder()
        .with_uri(config.host.clone())
        .with_database_name(world_id.to_string())
        .with_token(config.auth_token.clone())
        .on_connect({
            let connected = Arc::clone(&connected);
            move |_connection, _identity, _token| {
                *connected.lock().expect("publisher connect flag poisoned") = true;
            }
        })
        .on_connect_error({
            let connect_error = Arc::clone(&connect_error);
            move |_ctx, error| {
                *connect_error
                    .lock()
                    .expect("publisher connect error poisoned") = Some(error.to_string());
            }
        })
        .on_disconnect({
            let disconnect_reason = Arc::clone(&disconnect_reason);
            move |_ctx, error| {
                *disconnect_reason
                    .lock()
                    .expect("publisher disconnect poisoned") = Some(
                    error
                        .map(|error| error.to_string())
                        .unwrap_or_else(|| "publisher disconnected".to_string()),
                );
            }
        })
        .build()
        .map_err(|error| StdbClientError::Connection(error.to_string()))?;

    let timeout = Duration::from_millis(config.timeout_ms);
    let poll_interval = Duration::from_millis(config.poll_interval_ms);
    let deadline = Instant::now() + timeout;

    while !*connected.lock().expect("publisher connect flag poisoned") {
        connection
            .frame_tick()
            .map_err(|error| StdbClientError::Connection(error.to_string()))?;

        if let Some(error) = connect_error
            .lock()
            .expect("publisher connect error poisoned")
            .clone()
        {
            return Err(StdbClientError::Connection(error));
        }

        if let Some(reason) = disconnect_reason
            .lock()
            .expect("publisher disconnect poisoned")
            .clone()
        {
            return Err(StdbClientError::Connection(reason));
        }

        if Instant::now() >= deadline {
            return Err(StdbClientError::Connection(
                "timed out waiting for live topology publisher connection".into(),
            ));
        }

        sleep(poll_interval);
    }

    Ok(connection)
}

fn build_live_generated_topology_feed_world_path_report(
    topology: &RemoteTopologyBundle,
    topology_json: &str,
    world_id: &str,
    config: &LiveGeneratedSdkTopologyFeedConfig,
) -> Result<TopologyFeedWorldPathReport, StdbClientError> {
    let mut generated_client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
        host: config.host.clone(),
        db_name: world_id.to_string(),
        auth_token: config.auth_token.clone(),
        connection_mode: StdbConnectionMode::Generated,
        ..Default::default()
    });
    generated_client.install_generated_sdk_runtime();
    generated_client.subscribe_custom(vec!["SELECT * FROM remote_topology_document".into()])?;
    generated_client.connect()?;
    wait_for_live_generated_client_ready(&mut generated_client, config)?;

    let publisher = connect_live_generated_sdk_publisher(world_id, config)?;
    publisher
        .reducers
        .publish_remote_topology_document(topology_json.to_string())
        .map_err(|error| StdbClientError::Reducer(error.to_string()))?;

    let timeout = Duration::from_millis(config.timeout_ms);
    let poll_interval = Duration::from_millis(config.poll_interval_ms);
    let deadline = Instant::now() + timeout;
    loop {
        publisher
            .frame_tick()
            .map_err(|error| StdbClientError::Reducer(error.to_string()))?;
        let _ = generated_client.poll_updates();

        if generated_client.remote_world_id() == Some(world_id)
            && generated_client.remote_applied_world_state().is_some()
            && generated_client.remote_world_evaluation().is_some()
        {
            return Ok(build_topology_feed_world_path_report(
                &generated_client,
                topology,
                world_id,
            ));
        }

        match generated_client.inner().connection_state() {
            ConnectionState::Error(message) => {
                return Err(StdbClientError::Connection(message.clone()));
            }
            ConnectionState::Disconnected => {
                return Err(StdbClientError::Connection(
                    "generated SDK runtime disconnected before topology row arrived".into(),
                ));
            }
            _ => {}
        }

        if Instant::now() >= deadline {
            return Err(StdbClientError::Connection(
                "timed out waiting for generated SDK runtime topology row".into(),
            ));
        }

        sleep(poll_interval);
    }
}

#[derive(Debug, Clone, Default)]
struct LiveGeneratedSdkSmokeObserverState {
    connected: bool,
    subscription_applied: bool,
    connect_error: Option<String>,
    disconnect_reason: Option<String>,
    subscription_error: Option<String>,
    entity_rows: Vec<module_bindings::EntityRow>,
    connected_agent_rows: Vec<module_bindings::ConnectedAgentRow>,
    action_submission_rows: Vec<module_bindings::ActionSubmissionRow>,
}

fn map_rust_sdk_adapter_session_error(error: RustSdkAdapterSessionError) -> StdbClientError {
    match error {
        RustSdkAdapterSessionError::Client(message) => StdbClientError::Connection(message),
        RustSdkAdapterSessionError::UnsupportedAction(message) => {
            StdbClientError::InvalidState(message)
        }
        RustSdkAdapterSessionError::Rollout(message) => StdbClientError::InvalidState(message),
    }
}

fn connect_live_generated_sdk_smoke_observer(
    config: &RustSdkAdapterLiveSmokeConfig,
) -> Result<
    (
        module_bindings::DbConnection,
        Arc<Mutex<LiveGeneratedSdkSmokeObserverState>>,
    ),
    StdbClientError,
> {
    let state = Arc::new(Mutex::new(LiveGeneratedSdkSmokeObserverState::default()));
    let connection = module_bindings::DbConnection::builder()
        .with_uri(config.host.clone())
        .with_database_name(config.db_name.clone())
        .with_token(config.auth_token.clone())
        .on_connect({
            let state = Arc::clone(&state);
            move |_connection, _identity, _token| {
                state
                    .lock()
                    .expect("live rust sdk smoke observer state poisoned")
                    .connected = true;
            }
        })
        .on_connect_error({
            let state = Arc::clone(&state);
            move |_ctx, error| {
                state
                    .lock()
                    .expect("live rust sdk smoke observer state poisoned")
                    .connect_error = Some(error.to_string());
            }
        })
        .on_disconnect({
            let state = Arc::clone(&state);
            move |_ctx, error| {
                state
                    .lock()
                    .expect("live rust sdk smoke observer state poisoned")
                    .disconnect_reason = Some(
                    error
                        .map(|error| error.to_string())
                        .unwrap_or_else(|| "live rust sdk smoke observer disconnected".into()),
                );
            }
        })
        .build()
        .map_err(|error| StdbClientError::Connection(error.to_string()))?;

    let timeout = Duration::from_millis(config.timeout_ms);
    let poll_interval = Duration::from_millis(config.poll_interval_ms);
    let deadline = Instant::now() + timeout;

    loop {
        connection
            .frame_tick()
            .map_err(|error| StdbClientError::Connection(error.to_string()))?;
        {
            let snapshot = state
                .lock()
                .expect("live rust sdk smoke observer state poisoned")
                .clone();
            if snapshot.connected {
                break;
            }
            if let Some(error) = snapshot.connect_error {
                return Err(StdbClientError::Connection(error));
            }
            if let Some(reason) = snapshot.disconnect_reason {
                return Err(StdbClientError::Connection(reason));
            }
        }

        if Instant::now() >= deadline {
            return Err(StdbClientError::Connection(
                "timed out waiting for live rust sdk smoke observer connection".into(),
            ));
        }

        sleep(poll_interval);
    }

    connection.db.entity().on_insert({
        let state = Arc::clone(&state);
        move |_ctx, row| {
            state
                .lock()
                .expect("live rust sdk smoke observer state poisoned")
                .entity_rows
                .push(row.clone());
        }
    });
    connection.db.connected_agent().on_insert({
        let state = Arc::clone(&state);
        move |_ctx, row| {
            state
                .lock()
                .expect("live rust sdk smoke observer state poisoned")
                .connected_agent_rows
                .push(row.clone());
        }
    });
    connection.db.action_submission().on_insert({
        let state = Arc::clone(&state);
        move |_ctx, row| {
            state
                .lock()
                .expect("live rust sdk smoke observer state poisoned")
                .action_submission_rows
                .push(row.clone());
        }
    });

    connection
        .subscription_builder()
        .on_applied({
            let state = Arc::clone(&state);
            move |_ctx| {
                state
                    .lock()
                    .expect("live rust sdk smoke observer state poisoned")
                    .subscription_applied = true;
            }
        })
        .on_error({
            let state = Arc::clone(&state);
            move |_ctx, error| {
                state
                    .lock()
                    .expect("live rust sdk smoke observer state poisoned")
                    .subscription_error = Some(error.to_string());
            }
        })
        .subscribe(vec![
            String::from("SELECT * FROM entity"),
            String::from("SELECT * FROM connected_agent"),
            String::from("SELECT * FROM action_submission"),
        ]);

    let deadline = Instant::now() + timeout;
    loop {
        connection
            .frame_tick()
            .map_err(|error| StdbClientError::Connection(error.to_string()))?;
        {
            let snapshot = state
                .lock()
                .expect("live rust sdk smoke observer state poisoned")
                .clone();
            if snapshot.subscription_applied {
                break;
            }
            if let Some(error) = snapshot.subscription_error {
                return Err(StdbClientError::Subscription(error));
            }
            if let Some(error) = snapshot.connect_error {
                return Err(StdbClientError::Connection(error));
            }
            if let Some(reason) = snapshot.disconnect_reason {
                return Err(StdbClientError::Connection(reason));
            }
        }

        if Instant::now() >= deadline {
            return Err(StdbClientError::Subscription(
                "timed out waiting for live rust sdk smoke observer subscription".into(),
            ));
        }

        sleep(poll_interval);
    }

    Ok((connection, state))
}

fn spawn_live_generated_sdk_smoke_entity(
    observer: &module_bindings::DbConnection,
    observer_state: &Arc<Mutex<LiveGeneratedSdkSmokeObserverState>>,
    config: &RustSdkAdapterLiveSmokeConfig,
) -> Result<module_bindings::EntityRow, StdbClientError> {
    let baseline_ids = observer_state
        .lock()
        .expect("live rust sdk smoke observer state poisoned")
        .entity_rows
        .iter()
        .map(|row| row.entity_id)
        .collect::<HashSet<_>>();

    observer
        .reducers
        .spawn_entity(128.0, 192.0, Some(module_bindings::AgentType::LlmAgent))
        .map_err(|error| StdbClientError::Reducer(error.to_string()))?;

    let timeout = Duration::from_millis(config.timeout_ms);
    let poll_interval = Duration::from_millis(config.poll_interval_ms);
    let deadline = Instant::now() + timeout;

    loop {
        observer
            .frame_tick()
            .map_err(|error| StdbClientError::Reducer(error.to_string()))?;
        {
            let snapshot = observer_state
                .lock()
                .expect("live rust sdk smoke observer state poisoned")
                .clone();
            if let Some(row) = snapshot.entity_rows.iter().find(|row| {
                !baseline_ids.contains(&row.entity_id)
                    && row.alive
                    && matches!(row.agent_type, Some(module_bindings::AgentType::LlmAgent))
            }) {
                return Ok(row.clone());
            }
            if let Some(error) = snapshot.subscription_error {
                return Err(StdbClientError::Subscription(error));
            }
            if let Some(reason) = snapshot.disconnect_reason {
                return Err(StdbClientError::Connection(reason));
            }
        }

        if Instant::now() >= deadline {
            return Err(StdbClientError::Reducer(
                "timed out waiting for live rust sdk smoke spawn_entity row".into(),
            ));
        }

        sleep(poll_interval);
    }
}

fn wait_for_live_generated_sdk_session_ready(
    session: &mut RustSdkAdapterSession,
    config: &RustSdkAdapterLiveSmokeConfig,
) -> Result<(), StdbClientError> {
    let timeout = Duration::from_millis(config.timeout_ms);
    let poll_interval = Duration::from_millis(config.poll_interval_ms);
    let deadline = Instant::now() + timeout;

    loop {
        let _ = session.host_mut().poll_updates();
        if session.host().client().is_connected() {
            return Ok(());
        }

        match session.host().client().inner().connection_state() {
            ConnectionState::Error(message) => {
                return Err(StdbClientError::Connection(message.clone()));
            }
            ConnectionState::Disconnected => {
                return Err(StdbClientError::Connection(
                    "rust sdk live smoke session disconnected before ready".into(),
                ));
            }
            _ => {}
        }

        if Instant::now() >= deadline {
            return Err(StdbClientError::Connection(
                "timed out waiting for rust sdk live smoke session connection".into(),
            ));
        }

        sleep(poll_interval);
    }
}

fn wait_for_live_generated_sdk_smoke_results(
    observer: &module_bindings::DbConnection,
    observer_state: &Arc<Mutex<LiveGeneratedSdkSmokeObserverState>>,
    session: &mut RustSdkAdapterSession,
    config: &RustSdkAdapterLiveSmokeConfig,
    entity_id: u64,
) -> Result<
    (
        module_bindings::ConnectedAgentRow,
        module_bindings::ActionSubmissionRow,
    ),
    StdbClientError,
> {
    let timeout = Duration::from_millis(config.timeout_ms);
    let poll_interval = Duration::from_millis(config.poll_interval_ms);
    let deadline = Instant::now() + timeout;

    loop {
        observer
            .frame_tick()
            .map_err(|error| StdbClientError::Reducer(error.to_string()))?;
        let _ = session.host_mut().poll_updates();

        {
            let snapshot = observer_state
                .lock()
                .expect("live rust sdk smoke observer state poisoned")
                .clone();
            let connected_agent = snapshot
                .connected_agent_rows
                .iter()
                .find(|row| row.entity_id == entity_id)
                .cloned();
            let action_submission = snapshot
                .action_submission_rows
                .iter()
                .find(|row| {
                    row.entity_id == entity_id
                        && matches!(row.action_kind, module_bindings::ActionKind::Idle)
                })
                .cloned();

            if let (Some(connected_agent), Some(action_submission)) =
                (connected_agent, action_submission)
            {
                return Ok((connected_agent, action_submission));
            }

            if let Some(error) = snapshot.subscription_error {
                return Err(StdbClientError::Subscription(error));
            }
            if let Some(error) = snapshot.connect_error {
                return Err(StdbClientError::Connection(error));
            }
            if let Some(reason) = snapshot.disconnect_reason {
                return Err(StdbClientError::Connection(reason));
            }
        }

        match session.host().client().inner().connection_state() {
            ConnectionState::Error(message) => {
                return Err(StdbClientError::Connection(message.clone()));
            }
            ConnectionState::Disconnected => {
                return Err(StdbClientError::Connection(
                    "rust sdk live smoke session disconnected before rows arrived".into(),
                ));
            }
            _ => {}
        }

        if Instant::now() >= deadline {
            return Err(StdbClientError::Reducer(
                "timed out waiting for rust sdk live smoke connect/action rows".into(),
            ));
        }

        sleep(poll_interval);
    }
}

/// Replay a shared topology bundle through both authority-row and generated-mode ingestion paths.
pub fn build_topology_feed_measurements(
    topology: &RemoteTopologyBundle,
) -> Result<TopologyFeedMeasurementsReport, StdbClientError> {
    build_topology_feed_measurements_with_options(
        topology,
        &TopologyFeedMeasurementsOptions::default(),
    )
}

/// Replay a shared topology bundle through both authority-row and generated-mode ingestion paths,
/// using explicit benchmark options for the generated half.
pub fn build_topology_feed_measurements_with_options(
    topology: &RemoteTopologyBundle,
    options: &TopologyFeedMeasurementsOptions,
) -> Result<TopologyFeedMeasurementsReport, StdbClientError> {
    let topology_json = topology.to_toon_document();
    let generated_at_unix_ms = u64::try_from(topology.generated_at_unix_ms)
        .map_err(|_| StdbClientError::Subscription("topology timestamp exceeds u64".into()))?;
    let mut worlds = Vec::with_capacity(topology.worlds.len());
    let mut checks = Vec::new();

    for (index, world) in topology.worlds.iter().enumerate() {
        let world_id = world.world_id.clone();

        let mut authority_client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
            db_name: world_id.clone(),
            connection_mode: StdbConnectionMode::Emulated,
            ..Default::default()
        });
        authority_client.receive_remote_topology_document_row(
            index as u64 + 1,
            generated_at_unix_ms,
            topology.scenario_id.clone(),
            topology.profile_id.clone(),
            topology_json.clone(),
        )?;
        let authority_row =
            build_topology_feed_world_path_report(&authority_client, topology, &world_id);

        let generated_runtime = match &options.generated_runtime_mode {
            TopologyFeedGeneratedRuntimeMode::DeterministicBinding => {
                build_deterministic_generated_topology_feed_world_path_report(
                    topology,
                    &topology_json,
                    &world_id,
                    index as u64 + 1,
                )?
            }
            TopologyFeedGeneratedRuntimeMode::LiveSdk(config) => {
                build_live_generated_topology_feed_world_path_report(
                    topology,
                    &topology_json,
                    &world_id,
                    config,
                )?
            }
        };

        checks.extend(collect_topology_feed_checks(
            &world_id,
            "authority_row",
            &authority_row,
        ));
        checks.extend(collect_topology_feed_checks(
            &world_id,
            "generated_runtime",
            &generated_runtime,
        ));

        worlds.push(TopologyFeedWorldReport {
            world_id,
            authority_row,
            generated_runtime,
        });
    }

    Ok(TopologyFeedMeasurementsReport {
        schema_version: 1,
        scenario_id: topology.scenario_id.clone(),
        profile_id: topology.profile_id.clone(),
        world_count: topology.worlds.len(),
        worlds,
        checks,
    })
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::{
        action::SpeakVolume as CoreSpeakVolume, build_rust_sdk_handoff_fixture,
        decode_toon_document, AgentTelemetryFrame, AgentTickRollup, AgentToolCallEvent,
        AgentToolCallTrace, CombatStyle, EncounterKind, EncounterState, FocusedEntityDebugSummary,
        Inventory, ItemStack, Objective, RemoteTopologyBundle, ReplayFile, ReplayHeader,
        ShardIncidentSummary, SkillKind, SkillProgress, Team, TickTelemetryFrame,
        VersionedTickTelemetry, WorldQuestBinding, WorldRealityDefinition, WorldRealityRole,
        WorldTournamentDefinition,
    };
    use pod_stdb::client::CachedWorldState;

    fn build_connected_remote_agent_client() -> SpacetimeDBClient {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
            connection_mode: StdbConnectionMode::Emulated,
            ..Default::default()
        });
        client.connect().expect("client connects in emulated mode");
        client.inner_mut().frame_tick();
        client
            .inner_mut()
            .call_spawn_entity(0.0, 0.0, Some(AgentType::LlmAgent))
            .expect("entity spawns");
        client
            .connect_remote_agent(1, AgentType::LlmAgent)
            .expect("remote llm agent connects");
        client
    }

    fn build_rust_sdk_state_snapshot_fixture() -> RustSdkStateSnapshot {
        let fixture = build_rust_sdk_handoff_fixture();
        let profile = fixture.observation.profile;
        let self_agent = fixture.observation.payload.self_state.agent_id;

        RustSdkStateSnapshot {
            tick: 41,
            elapsed_secs: 0.683,
            runtime_profile: profile,
            self_state: RustSdkSelfStateSnapshot {
                agent_id: self_agent,
                entity_id: EntityId(4001),
                position: Vec2::new(320.0, 640.0),
                rotation: 1.25,
                velocity: Vec2::new(-0.5, 0.75),
                health: Some(67.0),
                max_health: Some(99.0),
                team: Team::Team(2),
                combat_loadout: Some(pod_core::CombatLoadout {
                    style: CombatStyle::Ranged,
                    attack_range: 9.0,
                    attack_speed_ticks: 4,
                    max_hit: 12.0,
                    auto_retaliate: true,
                    equipped_weapon: Some("maple-shortbow".into()),
                    offhand_item: None,
                    active_ability_bar: vec!["piercing-shot".into(), "binding-shot".into()],
                }),
                skills: vec![
                    SkillProgress::new(SkillKind::Mining, 52, 145_200, 9_800),
                    SkillProgress::new(SkillKind::Magic, 41, 49_900, 3_400),
                ],
                inventory: Some(Inventory {
                    capacity: 28,
                    carried_weight: 7.5,
                    coins: 1_250,
                    items: vec![ItemStack {
                        item_id: "iron-pickaxe".into(),
                        display_name: "Iron Pickaxe".into(),
                        quantity: 1,
                        stackable: false,
                    }],
                }),
                encounter: Some(EncounterState {
                    encounter_id: 44,
                    kind: EncounterKind::OpenWorld,
                    threat_level: 0.6,
                    primary_target: Some(EntityId(5001)),
                    active_turn_owner: None,
                    capture_allowed: false,
                    in_combat: true,
                }),
            },
            visible_entities: vec![RustSdkVisibleEntitySnapshot {
                entity_id: EntityId(5001),
                entity_type: "iron_vein".into(),
                position: Vec2::new(323.0, 644.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                relationship: Relationship::Neutral,
                health_fraction: None,
                combat_style: None,
                creature: None,
            }],
            audible_events: vec![AudibleEvent {
                event_type: "pickaxe_swing".into(),
                direction: Vec2::new(0.7, 0.2),
                distance: 4.5,
                intensity: 0.5,
            }],
            messages: vec![AgentMessage {
                from: self_agent,
                content: "frontier miner ready".into(),
                channel: MessageChannel::Team,
            }],
            available_actions: vec!["Move".into(), "GatherResource".into()],
            objectives: vec![Objective {
                id: "mine-iron".into(),
                description: "Collect iron ore for the squad".into(),
                progress: 0.4,
                completed: false,
            }],
            dialog: Some(RustSdkDialogState {
                speaker: "Trader".into(),
                prompt: "Interested in tools?".into(),
                options: vec!["Show wares".into(), "Not now".into()],
            }),
            shop: Some(RustSdkShopState {
                shop_name: "Frontier Tools".into(),
                offer_count: 12,
                can_buy: true,
                can_sell: false,
            }),
            bank: Some(RustSdkBankState {
                bank_name: "Anchor Vault".into(),
                tab_count: 3,
                item_count: 86,
                can_deposit: true,
                can_withdraw: true,
            }),
            transport_contract: fixture.transport_contract.clone(),
            remote_topology: fixture.remote_topology.clone(),
            latest_tick_telemetry: fixture.latest_tick_telemetry.clone(),
            replay: fixture.replay.clone(),
        }
    }

    #[test]
    fn test_rust_sdk_adapter_host_applies_handoff_artifact_json_and_toon() {
        let fixture = build_rust_sdk_handoff_fixture();
        let observation = fixture.observation.payload.clone();

        for input in ["artifact", "json", "toon"] {
            let mut host = RustSdkAdapterHost::new(
                SpacetimeDBClientConfig {
                    db_name: "world-frontier-1".into(),
                    ..Default::default()
                },
                RustSdkAdapterRuntimeMode::Emulated,
            );

            match input {
                "artifact" => host
                    .apply_handoff_artifact(fixture.clone())
                    .expect("artifact handoff should apply"),
                "json" => host
                    .apply_handoff_json_document(
                        serde_json::to_string(&fixture).expect("fixture should serialize to JSON"),
                    )
                    .expect("json handoff should apply"),
                "toon" => host
                    .apply_handoff_toon_document(fixture.to_toon_document())
                    .expect("toon handoff should apply"),
                _ => unreachable!("covered inputs only"),
            }

            assert_eq!(host.runtime_mode(), RustSdkAdapterRuntimeMode::Emulated);
            assert_eq!(host.client().remote_world_id(), Some("world-frontier-1"));
            assert_eq!(
                host.client()
                    .remote_agent_contract()
                    .map(|contract| contract.profile.agent_type),
                Some(CoreAgentType::LlmAgent)
            );
            assert_eq!(
                host.client()
                    .inner()
                    .latest_observation_tick(observation.self_state.entity_id.0),
                Some(observation.tick)
            );

            let messages = host.poll_updates();
            assert!(messages.iter().any(|message| matches!(
                message,
                ServerMessage::DebugDocument { document }
                    if document.contains("remote_topology_bundle")
            )));
            let documents = host.client_mut().drain_debug_documents();
            let replay = documents
                .iter()
                .find_map(|document| {
                    decode_toon_document::<ReplayFile>(document, "replay_file").ok()
                })
                .expect("replay debug document should be retained");
            assert_eq!(replay.header.name, "rust-sdk-fixture");
        }
    }

    #[test]
    fn test_rust_sdk_adapter_host_generated_binding_mode_exposes_runtime_flow() {
        let mut host = RustSdkAdapterHost::new(
            SpacetimeDBClientConfig {
                db_name: "world-frontier-1".into(),
                ..Default::default()
            },
            RustSdkAdapterRuntimeMode::GeneratedBinding,
        );
        let endpoint = host
            .generated_binding_endpoint()
            .expect("generated binding mode should expose an endpoint");

        assert_eq!(
            host.client().inner().config().connection_mode,
            StdbConnectionMode::Generated
        );

        host.connect()
            .expect("connect should stage generated runtime work");
        let commands = endpoint.drain_commands();
        assert!(matches!(
            commands.as_slice(),
            [GeneratedBindingCommand::Connect { config }]
                if config.db_name == "world-frontier-1"
                    && matches!(config.connection_mode, StdbConnectionMode::Generated)
        ));

        let callbacks = endpoint.callbacks();
        callbacks.connected(vec![3; 16], "tok-rs-sdk");
        let _ = host.poll_updates();
        let commands = endpoint.drain_commands();
        assert!(matches!(
            commands.as_slice(),
            [GeneratedBindingCommand::Subscribe { queries }] if !queries.is_empty()
        ));
        callbacks.subscription_applied();
        let messages = host.poll_updates();
        assert!(messages
            .iter()
            .any(|message| matches!(message, ServerMessage::Welcome { .. })));

        host.apply_handoff_fixture()
            .expect("fixture should apply after generated binding connect flow");
        let messages = host.poll_updates();
        assert!(messages.iter().any(|message| matches!(
            message,
            ServerMessage::DebugDocument { document }
                if document.contains("remote_topology_bundle")
        )));
        let replay = host
            .client_mut()
            .drain_debug_documents()
            .into_iter()
            .find_map(|document| decode_toon_document::<ReplayFile>(&document, "replay_file").ok())
            .expect("replay document should be retained");
        assert_eq!(replay.header.name, "rust-sdk-fixture");
        assert_eq!(host.client().remote_world_id(), Some("world-frontier-1"));
    }

    #[test]
    fn test_rust_sdk_adapter_host_generated_sdk_mode_maps_connect_failures() {
        let mut host = RustSdkAdapterHost::new(
            SpacetimeDBClientConfig {
                host: "http://127.0.0.1:1".into(),
                ..Default::default()
            },
            RustSdkAdapterRuntimeMode::GeneratedSdk,
        );

        assert_eq!(host.runtime_mode(), RustSdkAdapterRuntimeMode::GeneratedSdk);
        assert!(host.generated_binding_endpoint().is_none());
        assert_eq!(
            host.client().inner().config().connection_mode,
            StdbConnectionMode::Generated
        );

        let err = host
            .connect()
            .expect_err("closed localhost port should fail generated SDK connect");
        assert!(matches!(err, StdbClientError::Connection(_)));
    }

    #[test]
    fn test_rust_sdk_state_snapshot_translates_context_into_observation_and_handoff() {
        let snapshot = build_rust_sdk_state_snapshot_fixture();

        let observation = snapshot.to_observation();
        assert_eq!(observation.tick, 41);
        assert_eq!(observation.self_state.entity_id, EntityId(4001));
        assert_eq!(observation.self_state.team, Team::Team(2));
        assert_eq!(
            observation
                .self_state
                .inventory
                .as_ref()
                .map(|inventory| inventory.coins),
            Some(1_250)
        );
        assert_eq!(observation.visible_entities.len(), 1);
        assert!((observation.visible_entities[0].distance - 5.0).abs() < 0.001);
        assert!(observation
            .messages
            .iter()
            .any(|message| message.content.contains("dialog:Trader")));
        assert!(observation
            .messages
            .iter()
            .any(|message| message.content.contains("shop:Frontier Tools")));
        assert!(observation
            .messages
            .iter()
            .any(|message| message.content.contains("bank:Anchor Vault")));
        assert!(observation
            .available_actions
            .iter()
            .any(|action| action == "Dialog:SelectOption"));
        assert!(observation
            .available_actions
            .iter()
            .any(|action| action == "Shop:Buy"));
        assert!(observation
            .available_actions
            .iter()
            .any(|action| action == "Bank:Withdraw"));

        let artifact = snapshot.to_handoff_artifact();
        assert_eq!(artifact.observation.payload.tick, 41);
        assert_eq!(artifact.profile().agent_type, CoreAgentType::LlmAgent);
        assert_eq!(
            artifact
                .remote_topology
                .as_ref()
                .map(|topology| topology.scenario_id.as_str()),
            Some("rust-sdk-fixture")
        );
        assert_eq!(
            artifact
                .replay
                .as_ref()
                .map(|replay| replay.header.name.as_str()),
            Some("rust-sdk-fixture")
        );
    }

    #[test]
    fn test_rust_sdk_adapter_host_applies_state_snapshot_through_handoff_ingest() {
        let snapshot = build_rust_sdk_state_snapshot_fixture();
        let mut host = RustSdkAdapterHost::new(
            SpacetimeDBClientConfig {
                db_name: "world-frontier-1".into(),
                ..Default::default()
            },
            RustSdkAdapterRuntimeMode::Emulated,
        );

        host.apply_state_snapshot(&snapshot)
            .expect("state snapshot should apply through adapter host");

        assert_eq!(host.client().remote_world_id(), Some("world-frontier-1"));
        assert_eq!(
            host.client()
                .inner()
                .latest_observation_tick(snapshot.self_state.entity_id.0),
            Some(snapshot.tick)
        );
        assert!(host
            .client()
            .inner()
            .entity(snapshot.visible_entities[0].entity_id.0)
            .is_some());
        let messages = host.poll_updates();
        assert!(messages.iter().any(|message| matches!(
            message,
            ServerMessage::DebugDocument { document }
                if document.contains("remote_topology_bundle")
        )));
        let replay = host
            .client_mut()
            .drain_debug_documents()
            .into_iter()
            .find_map(|document| decode_toon_document::<ReplayFile>(&document, "replay_file").ok())
            .expect("replay debug document should be retained");
        assert_eq!(replay.header.name, "rust-sdk-fixture");
    }

    #[test]
    fn test_rust_sdk_adapter_host_executes_action_plan_through_emulated_submission() {
        let snapshot = build_rust_sdk_attack_snapshot();
        let plan = build_rust_sdk_action_plan(&Action::AttackTarget {
            target: EntityId(9001),
        })
        .expect("attack action should lower into a Rust SDK plan");
        let mut host = RustSdkAdapterHost::new(
            SpacetimeDBClientConfig {
                db_name: "world-frontier-1".into(),
                ..Default::default()
            },
            RustSdkAdapterRuntimeMode::Emulated,
        );

        host.connect().expect("emulated host should connect");
        let _ = host.poll_updates();
        host.bind_state_snapshot_action_entity(&snapshot)
            .expect("snapshot entity should bind");
        host.apply_state_snapshot(&snapshot)
            .expect("snapshot should hydrate state and handoff");

        let reducers_before = host.client().inner().reducers_called();
        let executed = host
            .execute_action_plan(&plan)
            .expect("action plan should execute over the shared queue/send path");

        assert!(matches!(
            executed,
            Action::AttackTarget {
                target: EntityId(9001)
            }
        ));
        assert_eq!(host.client().inner().reducers_called(), reducers_before + 1);
        assert_eq!(
            host.client().inner().controlled_entity(),
            Some(snapshot.self_state.entity_id.0)
        );
        assert_eq!(host.client().remote_agent_status().pending_action_count, 0);
    }

    #[test]
    fn test_rust_sdk_adapter_host_executes_action_plan_through_generated_binding_submission() {
        let snapshot = build_rust_sdk_gather_snapshot();
        let plan = build_rust_sdk_action_plan(&Action::GatherResource {
            target: EntityId(9101),
            skill: SkillKind::Mining,
        })
        .expect("gather action should lower into a Rust SDK plan");
        let mut host = RustSdkAdapterHost::new(
            SpacetimeDBClientConfig {
                db_name: "world-frontier-1".into(),
                ..Default::default()
            },
            RustSdkAdapterRuntimeMode::GeneratedBinding,
        );
        let endpoint = host
            .generated_binding_endpoint()
            .expect("generated-binding host should expose its endpoint");

        host.connect()
            .expect("generated-binding host should stage connection work");
        let callbacks = endpoint.callbacks();
        let _ = endpoint.drain_commands();
        callbacks.connected(vec![9; 16], "tok-rust-sdk-action");
        let _ = host.poll_updates();
        let _ = endpoint.drain_commands();
        callbacks.subscription_applied();
        let _ = host.poll_updates();

        host.bind_state_snapshot_action_entity(&snapshot)
            .expect("snapshot entity should bind");
        let commands = endpoint.drain_commands();
        assert!(matches!(
            commands.as_slice(),
            [GeneratedBindingCommand::Subscribe { queries }] if !queries.is_empty()
        ));
        callbacks.subscription_applied();
        let _ = host.poll_updates();
        host.apply_state_snapshot(&snapshot)
            .expect("snapshot should hydrate state and handoff");

        let reducers_before = host.client().inner().reducers_called();
        let executed = host
            .execute_action_plan(&plan)
            .expect("action plan should execute over generated-binding transport");

        assert!(matches!(
            executed,
            Action::GatherResource {
                target: EntityId(9101),
                skill: SkillKind::Mining
            }
        ));
        assert_eq!(host.client().inner().reducers_called(), reducers_before + 1);
        assert_eq!(host.client().remote_agent_status().pending_action_count, 0);
    }

    #[test]
    fn test_rust_sdk_adapter_session_executes_and_records_emulated_step() {
        let snapshot = build_rust_sdk_gather_snapshot();
        let mut session = RustSdkAdapterSession::new(
            SpacetimeDBClientConfig {
                db_name: "world-frontier-1".into(),
                ..Default::default()
            },
            RustSdkAdapterRuntimeMode::Emulated,
        );

        session.connect().expect("session should connect");
        let _ = session.host_mut().poll_updates();
        let executed = session
            .execute_actions(
                snapshot,
                "session gather prompt",
                vec![Action::GatherResource {
                    target: EntityId(9101),
                    skill: SkillKind::Mining,
                }],
                vec![AgentToolCallTrace::success(
                    1,
                    "path_probe",
                    "rs-sdk-session",
                    14,
                    5,
                    2,
                )],
                21,
            )
            .expect("session should execute and record the rollout step");

        assert!(matches!(
            executed.as_slice(),
            [Action::GatherResource {
                target: EntityId(9101),
                skill: SkillKind::Mining
            }]
        ));

        let replay = session.finalize_replay(ReplayHeader {
            name: "rust-sdk-session".into(),
            timestamp: 1_710_849_600,
            world_seed: 17,
            tick_count: 2,
            agent_count: 1,
            notes: "deterministic session test".into(),
        });
        let training_samples = replay.training_samples();

        assert_eq!(replay.header.name, "rust-sdk-session");
        assert_eq!(training_samples.len(), 1);
        assert_eq!(training_samples[0].tick, 1);
        assert_eq!(training_samples[0].tool_call_latency_ms, 12);
        assert_eq!(training_samples[0].action_outcomes.executed, 1);
    }

    #[test]
    fn test_rust_sdk_facade_executes_fixture_backed_emulated_step() {
        let snapshot = build_rust_sdk_gather_snapshot();
        let mut facade = RustSdkFacade::new(RustSdkFacadeConfig {
            client: SpacetimeDBClientConfig {
                db_name: "world-frontier-1".into(),
                ..Default::default()
            },
            runtime_mode: RustSdkAdapterRuntimeMode::Emulated,
        });

        facade.connect().expect("facade should connect");
        let _ = facade.poll_updates();
        facade
            .apply_handoff_fixture()
            .expect("facade should ingest the canonical handoff fixture");
        assert!(facade.client().last_debug_document().is_some());

        let executed = facade
            .execute_actions(
                snapshot,
                "facade gather prompt",
                vec![Action::GatherResource {
                    target: EntityId(9101),
                    skill: SkillKind::Mining,
                }],
                vec![AgentToolCallTrace::success(
                    1,
                    "facade_path_probe",
                    "rs-sdk-facade",
                    9,
                    3,
                    1,
                )],
                18,
            )
            .expect("facade should execute and record the shared rollout step");

        assert!(matches!(
            executed.as_slice(),
            [Action::GatherResource {
                target: EntityId(9101),
                skill: SkillKind::Mining
            }]
        ));
        assert_eq!(facade.runtime_mode(), RustSdkAdapterRuntimeMode::Emulated);

        let replay = facade.finalize_replay(ReplayHeader {
            name: "rust-sdk-facade".into(),
            timestamp: 1_710_849_600,
            world_seed: 23,
            tick_count: 2,
            agent_count: 1,
            notes: "deterministic facade test".into(),
        });
        let training_samples = replay.training_samples();

        assert_eq!(replay.header.name, "rust-sdk-facade");
        assert_eq!(training_samples.len(), 1);
        assert_eq!(training_samples[0].tick, 1);
        assert_eq!(training_samples[0].tool_call_latency_ms, 12);
    }

    #[test]
    fn test_rust_sdk_facade_exposes_generated_binding_endpoint_and_handoff_decode() {
        let fixture = build_rust_sdk_handoff_fixture();
        let expected_tick = fixture.observation.payload.tick;
        let mut generated_binding_facade = RustSdkFacade::new(RustSdkFacadeConfig {
            client: SpacetimeDBClientConfig {
                db_name: "world-frontier-1".into(),
                ..Default::default()
            },
            runtime_mode: RustSdkAdapterRuntimeMode::GeneratedBinding,
        });
        let mut toon_facade = RustSdkFacade::new(RustSdkFacadeConfig {
            client: SpacetimeDBClientConfig {
                db_name: "world-frontier-1".into(),
                ..Default::default()
            },
            runtime_mode: RustSdkAdapterRuntimeMode::Emulated,
        });

        assert_eq!(
            generated_binding_facade.runtime_mode(),
            RustSdkAdapterRuntimeMode::GeneratedBinding
        );
        assert!(generated_binding_facade.generated_binding_endpoint().is_some());
        generated_binding_facade
            .apply_handoff_json_document(
                serde_json::to_string(&fixture).expect("handoff fixture should encode to JSON"),
            )
            .expect("facade should decode handoff JSON");
        toon_facade
            .apply_handoff_toon_document(fixture.to_toon_document())
            .expect("facade should decode handoff TOON");

        assert_eq!(
            generated_binding_facade
                .client()
                .remote_agent_status()
                .last_observation_tick,
            Some(expected_tick)
        );
        assert_eq!(
            toon_facade.client().remote_agent_status().last_observation_tick,
            Some(expected_tick)
        );
    }

    #[test]
    fn test_rust_sdk_facade_run_live_smoke_maps_host_failures() {
        let err = RustSdkFacade::run_live_smoke(&RustSdkAdapterLiveSmokeConfig {
            host: "http://127.0.0.1:1".into(),
            db_name: "missing-world".into(),
            timeout_ms: 25,
            poll_interval_ms: 5,
            ..Default::default()
        })
        .expect_err("closed localhost port should fail facade live smoke");

        assert!(matches!(err, RustSdkFacadeError::Host(_)));
    }

    #[test]
    fn test_build_rust_sdk_action_plan_routes_execution_modes() {
        let move_plan = build_rust_sdk_action_plan(&Action::Move {
            direction: Vec2::new(1.0, 0.0),
        })
        .expect("move should translate");
        assert_eq!(
            move_plan.execution_mode,
            RustSdkActionExecutionMode::Immediate
        );
        assert!(matches!(
            move_plan.intent,
            RustSdkActionIntent::MoveDirection { direction }
                if direction == Vec2::new(1.0, 0.0)
        ));

        let gather_plan = build_rust_sdk_action_plan(&Action::GatherResource {
            target: EntityId(77),
            skill: SkillKind::Mining,
        })
        .expect("gather should translate");
        assert_eq!(
            gather_plan.execution_mode,
            RustSdkActionExecutionMode::CompletionAware
        );
        assert!(matches!(
            gather_plan.intent,
            RustSdkActionIntent::GatherEntity {
                entity_id: 77,
                skill: SkillKind::Mining
            }
        ));

        let speak_plan = build_rust_sdk_action_plan(&Action::Speak {
            message: "hold west".into(),
            volume: CoreSpeakVolume::Normal,
        })
        .expect("speak should translate");
        assert_eq!(
            speak_plan.execution_mode,
            RustSdkActionExecutionMode::Immediate
        );
        assert!(matches!(
            speak_plan.intent,
            RustSdkActionIntent::Speak { ref message, .. } if message == "hold west"
        ));
    }

    #[test]
    fn test_build_rust_sdk_action_plan_rejects_spawn() {
        let err = build_rust_sdk_action_plan(&Action::Spawn {
            prefab: "ore-vein".into(),
            position: Vec2::new(10.0, 20.0),
        })
        .expect_err("spawn should stay outside the SDK adapter");

        assert!(matches!(
            err,
            RustSdkActionAdapterError::UnsupportedAction {
                action: "Spawn",
                ..
            }
        ));
    }

    #[test]
    fn test_rust_sdk_rollout_recorder_finalizes_shared_replay_and_training_samples() {
        let mut recorder = RustSdkRolloutRecorder::new();
        let attack_record = RustSdkRolloutRecord::from_actions(
            build_rust_sdk_attack_snapshot(),
            "attack benchmark",
            vec![Action::AttackTarget {
                target: EntityId(9001),
            }],
            Vec::new(),
            7,
        )
        .expect("attack record should build");
        recorder
            .record(attack_record)
            .expect("attack record should store");

        let gather_record = RustSdkRolloutRecord::from_actions(
            build_rust_sdk_gather_snapshot(),
            "gather benchmark",
            vec![Action::GatherResource {
                target: EntityId(9101),
                skill: SkillKind::Mining,
            }],
            vec![pod_core::AgentToolCallTrace::success(
                1,
                "path_probe",
                "rs-sdk-benchmark",
                12,
                4,
                3,
            )],
            19,
        )
        .expect("gather record should build");
        recorder
            .record(gather_record)
            .expect("gather record should store");

        let replay = recorder.finalize(ReplayHeader {
            name: "rust-sdk-rollout".into(),
            timestamp: 1_710_849_600,
            world_seed: 5,
            tick_count: 2,
            agent_count: 1,
            notes: "deterministic test rollout".into(),
        });
        let training_samples = replay.training_samples();

        assert_eq!(replay.header.name, "rust-sdk-rollout");
        assert_eq!(replay.header.tick_count, 2);
        assert_eq!(replay.traces.len(), 2);
        assert_eq!(replay.telemetry_windows.len(), 2);
        assert_eq!(training_samples.len(), 2);
        assert_eq!(training_samples[0].tick, 0);
        assert_eq!(training_samples[1].tick, 1);
        assert_eq!(training_samples[1].tool_call_latency_ms, 12);
        assert_eq!(training_samples[1].action_outcomes.executed, 1);
    }

    #[test]
    fn test_run_rust_sdk_adapter_benchmark_suite_reports_passing_checks() {
        let run = run_rust_sdk_adapter_benchmark_suite()
            .expect("benchmark suite should run on execution-backed fixtures");

        assert!(run.report.passed(), "benchmark checks should pass");
        assert_eq!(run.report.benchmark_id, "rust-sdk-adapter-seams");
        assert_eq!(run.report.scenarios.len(), 2);
        assert_eq!(run.report.replay_training_sample_count, 2);
        assert_eq!(run.training_samples.len(), 2);
        assert_eq!(run.replay.header.name, "rust-sdk-adapter-benchmark");
        assert!(run.report.scenarios.iter().all(|scenario| {
            scenario.action_matches
                && scenario.execution_mode_matches
                && scenario.reducer_submission_matches
        }));
        assert!(run.report.scenarios.iter().any(|scenario| {
            scenario.runtime_mode == RustSdkAdapterRuntimeMode::GeneratedBinding
        }));
    }

    #[test]
    fn test_build_topology_feed_measurements_matches_authority_and_generated_paths() {
        let mut world = WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow");
        world.role = WorldRealityRole::Shadow;
        world.active_team_ids = vec!["gloam-mesh".into()];

        let topology = RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 42,
            tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            teams: vec![pod_core::AgentTeamDefinition::new(
                "gloam-mesh",
                "Gloam Mesh",
                "deadman-shadow",
            )],
            worlds: vec![world],
            links: vec![],
            world_quest_bindings: vec![WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-collapse".into()],
            }],
            world_admissions: vec![],
            world_control_planes: vec![],
            tournament_control_plane: pod_core::TournamentControlPlaneSummary::default(),
            tournament_orchestration: pod_core::TournamentOrchestrationSummary::default(),
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
                team_scores: vec![pod_core::TeamDeltaSummary {
                    team_id: "gloam-mesh".into(),
                    total_delta: 5,
                }],
                death_marks: vec![],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![pod_core::ObjectiveShiftSummary {
                    quest_graph_id: "deadman-shadow-collapse".into(),
                    stage_tag: "collapse-signaled".into(),
                    applications: 2,
                }],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![pod_core::QuestLineStateSummary {
                    quest_graph_id: "deadman-shadow-collapse".into(),
                    display_name: "Shadow Collapse".into(),
                    current_stage_ids: vec!["collapse-signaled".into()],
                    completed_stage_ids: vec![],
                    pending_stage_ids: vec!["collapse-resolved".into()],
                    next_stage_ids: vec!["collapse-resolved".into()],
                    progress_basis_points: 5000,
                    terminal: false,
                    stage_applications: vec![pod_core::QuestStageApplicationSummary {
                        stage_id: "collapse-signaled".into(),
                        title: "Collapse Signaled".into(),
                        applications: 2,
                    }],
                }],
            }],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![pod_core::WorldEvaluationSummary {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: WorldRealityRole::Shadow,
                    average_reward_per_row: 6.25,
                    controller_mix: vec![pod_core::ControllerEvaluationSummary {
                        agent_type: "neural_agent".into(),
                        row_count: 4,
                        reward_total: 25.0,
                        average_reward_per_row: 6.25,
                    }],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 5000,
                    applied_score_delta_total: 5,
                    applied_death_mark_count: 0,
                    applied_death_mark_ticks: 0,
                    applied_objective_shift_count: 2,
                    applied_reputation_delta_total: 0,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        };

        let report =
            build_topology_feed_measurements(&topology).expect("topology feed benchmark builds");

        assert_eq!(report.schema_version, 1);
        assert_eq!(report.world_count, 1);
        assert!(report.all_checks_passed());
        assert_eq!(report.worlds[0].world_id, "deadman-shadow");
        assert!(report.worlds[0].authority_row.quest_binding_matches);
        assert!(report.worlds[0].authority_row.applied_world_state_matches);
        assert!(
            report.worlds[0]
                .authority_row
                .tournament_control_plane_matches
        );
        assert!(
            report.worlds[0]
                .authority_row
                .world_tournament_orchestration_matches
        );
        assert!(
            report.worlds[0]
                .authority_row
                .tournament_orchestration_matches
        );
        assert!(report.worlds[0].generated_runtime.quest_binding_matches);
        assert!(report.worlds[0].generated_runtime.evaluation_matches);
        assert!(
            report.worlds[0]
                .generated_runtime
                .tournament_control_plane_matches
        );
        assert!(
            report.worlds[0]
                .generated_runtime
                .world_tournament_orchestration_matches
        );
        assert!(
            report.worlds[0]
                .generated_runtime
                .tournament_orchestration_matches
        );
    }

    #[test]
    fn test_build_topology_feed_measurements_live_sdk_propagates_connect_failures() {
        let topology = RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 42,
            tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            teams: vec![pod_core::AgentTeamDefinition::new(
                "gloam-mesh",
                "Gloam Mesh",
                "deadman-shadow",
            )],
            worlds: vec![WorldRealityDefinition::new(
                "deadman-shadow",
                "Deadman Shadow",
                "shadow",
            )],
            links: vec![],
            world_quest_bindings: vec![],
            world_admissions: vec![],
            world_control_planes: vec![],
            tournament_control_plane: pod_core::TournamentControlPlaneSummary::default(),
            tournament_orchestration: pod_core::TournamentOrchestrationSummary::default(),
            quest_graphs: vec![],
            applied_world_states: vec![],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![],
            },
        };

        let err = build_topology_feed_measurements_with_options(
            &topology,
            &TopologyFeedMeasurementsOptions {
                generated_runtime_mode: TopologyFeedGeneratedRuntimeMode::LiveSdk(
                    LiveGeneratedSdkTopologyFeedConfig {
                        host: "http://127.0.0.1:1".into(),
                        auth_token: None,
                        timeout_ms: 100,
                        poll_interval_ms: 1,
                    },
                ),
            },
        )
        .expect_err("closed localhost port should fail the live generated SDK path");

        assert!(matches!(err, StdbClientError::Connection(_)));
    }

    #[test]
    fn test_entity_to_snapshot_defaults() {
        let cached = CachedEntity::from_entity(42, None, true);
        let snap = entity_to_snapshot(&cached, None, None, None, None);

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
        let quest_graph_ids = vec!["deadman-prime-season".to_string()];
        cached.pos_x = Some(100.0);
        cached.pos_y = Some(200.0);
        cached.vel_x = Some(1.0);
        cached.vel_y = Some(-1.0);
        cached.rotation = Some(1.57);
        cached.health = Some(80.0);
        cached.max_health = Some(100.0);
        cached.max_speed = Some(240.0);
        cached.name = Some("Hero".into());
        cached.team_id = Some(1);

        let snap = entity_to_snapshot(
            &cached,
            Some("deadman-prime"),
            Some(WorldRealityRole::Tournament),
            Some("iron-sigil".into()),
            Some(quest_graph_ids.as_slice()),
        );

        assert_eq!(snap.id, 7);
        assert_eq!(snap.position, Vec2::new(100.0, 200.0));
        assert_eq!(snap.velocity, Vec2::new(1.0, -1.0));
        assert!((snap.rotation - 1.57).abs() < f32::EPSILON);
        assert_eq!(snap.health, Some(80.0));
        assert_eq!(snap.max_health, Some(100.0));
        assert_eq!(snap.movement_speed, Some(240.0));
        assert_eq!(snap.label.as_deref(), Some("Hero"));
        assert_eq!(snap.metadata.team_id, Some(1));
        assert_eq!(snap.metadata.team_key.as_deref(), Some("iron-sigil"));
        assert_eq!(snap.metadata.world_id.as_deref(), Some("deadman-prime"));
        assert_eq!(snap.metadata.world_role, Some(WorldRealityRole::Tournament));
        assert_eq!(
            snap.metadata.world_active_quest_graph_ids,
            vec!["deadman-prime-season".to_string()]
        );
    }

    #[test]
    fn test_apply_remote_topology_rebuilds_snapshot_metadata() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
            db_name: "deadman-prime".into(),
            connection_mode: StdbConnectionMode::Emulated,
            ..Default::default()
        });
        client.connect().expect("client connects in emulated mode");
        client.inner.frame_tick();
        client
            .subscriptions
            .ensure_subscriptions_applied(&mut client.inner)
            .expect("subscriptions apply");

        let mut cached = CachedEntity::from_entity(7, None, true);
        cached.name = Some("Hero".into());
        cached.team_id = Some(1);
        client.inner.upsert_entity(cached);

        let mut world =
            WorldRealityDefinition::new("deadman-prime", "Deadman Prime", "deadman-seasonal");
        world.role = WorldRealityRole::Tournament;
        world.active_team_ids = vec!["iron-sigil".into()];
        let topology = RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 42,
            tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            teams: vec![pod_core::AgentTeamDefinition::new(
                "iron-sigil",
                "Iron Sigil",
                "deadman-prime",
            )],
            worlds: vec![world],
            links: vec![],
            world_quest_bindings: vec![WorldQuestBinding {
                world_id: "deadman-prime".into(),
                quest_graph_ids: vec!["deadman-prime-season".into()],
            }],
            world_admissions: vec![pod_core::WorldAdmissionSummary {
                world_id: "deadman-prime".into(),
                assignments: vec![pod_core::WorldAdmissionAssignment {
                    agent_id: agent_id_from_entity(7).to_string(),
                    team_id: "iron-sigil".into(),
                    slot_index: 0,
                }],
            }],
            world_control_planes: vec![pod_core::WorldControlPlaneSummary {
                world_id: "deadman-prime".into(),
                teams: vec![pod_core::WorldTeamControlSummary {
                    team_id: "iron-sigil".into(),
                    assignments: vec![pod_core::WorldControlAssignmentSummary {
                        agent_id: agent_id_from_entity(7).to_string(),
                        slot_index: 0,
                        runtime_profile: pod_core::AgentRuntimeProfile::for_agent_type(
                            pod_core::AgentType::NeuralAgent,
                        ),
                    }],
                    controller_mix: vec![pod_core::AgentTypeCountSummary {
                        agent_type: "neural_agent".into(),
                        count: 1,
                    }],
                }],
            }],
            tournament_control_plane: pod_core::TournamentControlPlaneSummary {
                tournament_id: "deadman-neural-cup".into(),
                standings: vec![pod_core::TournamentTeamStandingSummary {
                    team_id: "iron-sigil".into(),
                    display_name: "Iron Sigil".into(),
                    control_mode: pod_core::TeamControlMode::DeveloperCaptain,
                    home_world_id: "deadman-prime".into(),
                    participating_world_ids: vec!["deadman-prime".into()],
                    assigned_agent_count: 1,
                    controller_mix: vec![pod_core::AgentTypeCountSummary {
                        agent_type: "neural_agent".into(),
                        count: 1,
                    }],
                    dataset_row_count: 0,
                    world_reward_total: 0.0,
                    applied_score_delta: 0,
                    active_death_marks: 0,
                    active_death_mark_ticks: 0,
                }],
            },
            tournament_orchestration: pod_core::TournamentOrchestrationSummary {
                tournament_id: "deadman-neural-cup".into(),
                phase: pod_core::TournamentOrchestrationPhase::Active,
                active_world_ids: vec!["deadman-prime".into()],
                contested_world_ids: vec!["deadman-prime".into()],
                active_link_ids: vec![],
                leading_team_ids: vec!["iron-sigil".into()],
                at_risk_team_ids: vec![],
                worlds: vec![pod_core::WorldTournamentOrchestrationSummary {
                    world_id: "deadman-prime".into(),
                    display_name: "Deadman Prime".into(),
                    role: WorldRealityRole::Tournament,
                    active_team_ids: vec!["iron-sigil".into()],
                    linked_world_ids: vec![],
                    active_link_ids: vec![],
                    assigned_agent_count: 1,
                    controller_mix: vec![pod_core::AgentTypeCountSummary {
                        agent_type: "neural_agent".into(),
                        count: 1,
                    }],
                    applied_score_delta_total: 0,
                    applied_death_mark_count: 0,
                    applied_death_mark_ticks: 0,
                    objective_shift_count: 0,
                    unresolved_objective_shift_count: 0,
                    progressed_quest_line_count: 0,
                    terminal_quest_line_count: 0,
                    leading_team_ids: vec!["iron-sigil".into()],
                    at_risk_team_ids: vec![],
                }],
            },
            quest_graphs: vec![],
            applied_world_states: vec![],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![],
            },
        };

        client
            .apply_remote_topology(topology)
            .expect("topology applies");
        let messages = client.poll_updates();
        let delta = messages
            .into_iter()
            .find_map(|message| match message {
                ServerMessage::StateDelta { delta, .. } => Some(delta),
                _ => None,
            })
            .expect("state delta emitted after topology update");
        let hero = delta
            .updated
            .into_iter()
            .find(|entity| entity.id == 7)
            .expect("hero snapshot updated");
        assert_eq!(hero.metadata.team_key.as_deref(), Some("iron-sigil"));
        assert_eq!(hero.metadata.world_id.as_deref(), Some("deadman-prime"));
        assert_eq!(hero.metadata.world_role, Some(WorldRealityRole::Tournament));
        assert_eq!(
            hero.metadata.world_active_quest_graph_ids,
            vec!["deadman-prime-season".to_string()]
        );
        let admissions = client
            .remote_world_admissions()
            .expect("resolved world admissions");
        assert_eq!(admissions.world_id, "deadman-prime");
        assert_eq!(admissions.assignments.len(), 1);
        assert_eq!(
            admissions.assignments[0].agent_id,
            agent_id_from_entity(7).to_string()
        );
        assert_eq!(admissions.assignments[0].team_id, "iron-sigil");
        assert_eq!(admissions.assignments[0].slot_index, 0);
        let control_plane = client
            .remote_world_control_plane()
            .expect("resolved world control plane");
        assert_eq!(control_plane.world_id, "deadman-prime");
        assert_eq!(control_plane.teams.len(), 1);
        assert_eq!(control_plane.teams[0].team_id, "iron-sigil");
        assert_eq!(
            control_plane.teams[0].controller_mix[0].agent_type,
            "neural_agent"
        );
        assert_eq!(control_plane.teams[0].controller_mix[0].count, 1);
        let tournament_control_plane = client
            .remote_tournament_control_plane()
            .expect("resolved tournament control plane");
        assert_eq!(tournament_control_plane.tournament_id, "deadman-neural-cup");
        assert_eq!(tournament_control_plane.standings.len(), 1);
        assert_eq!(tournament_control_plane.standings[0].team_id, "iron-sigil");
        assert_eq!(
            tournament_control_plane.standings[0].assigned_agent_count,
            1
        );
        let orchestration = client
            .remote_tournament_orchestration()
            .expect("resolved tournament orchestration");
        assert_eq!(
            orchestration.phase,
            pod_core::TournamentOrchestrationPhase::Active
        );
        assert_eq!(
            orchestration.leading_team_ids,
            vec!["iron-sigil".to_string()]
        );
        assert_eq!(
            client
                .remote_world_tournament_orchestration()
                .map(|summary| summary.world_id.as_str()),
            Some("deadman-prime")
        );
    }

    #[test]
    fn test_remote_topology_exposes_linked_world_evaluation_for_neural_swarm_world() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
            db_name: "deadman-shadow".into(),
            connection_mode: StdbConnectionMode::Emulated,
            ..Default::default()
        });
        client.connect().expect("client connects in emulated mode");
        client
            .subscribe_as_spectator()
            .expect("spectator subscriptions stage or apply");
        client.inner.frame_tick();

        let mut cached = CachedEntity::from_entity(9, None, true);
        cached.name = Some("Swarm Vanguard".into());
        cached.team_id = Some(2);
        client.inner.upsert_entity(cached);

        let mut prime =
            WorldRealityDefinition::new("deadman-prime", "Deadman Prime", "deadman-seasonal");
        prime.role = WorldRealityRole::Tournament;
        prime.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];

        let mut shadow =
            WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow-seasonal");
        shadow.role = WorldRealityRole::Shadow;
        shadow.linked_world_ids = vec!["deadman-prime".into()];
        shadow.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];

        let topology = RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 42,
            tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            teams: vec![
                pod_core::AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime"),
                pod_core::AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow"),
            ],
            worlds: vec![prime, shadow],
            links: vec![],
            world_quest_bindings: vec![
                WorldQuestBinding {
                    world_id: "deadman-prime".into(),
                    quest_graph_ids: vec!["deadman-prime-season".into()],
                },
                WorldQuestBinding {
                    world_id: "deadman-shadow".into(),
                    quest_graph_ids: vec!["deadman-shadow-hunt".into()],
                },
            ],
            world_admissions: vec![],
            world_control_planes: vec![],
            tournament_control_plane: pod_core::TournamentControlPlaneSummary::default(),
            tournament_orchestration: pod_core::TournamentOrchestrationSummary::default(),
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
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
                    role: WorldRealityRole::Shadow,
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

        client
            .apply_remote_topology(topology)
            .expect("topology applies");

        assert_eq!(client.remote_world_id(), Some("deadman-shadow"));
        let applied = client
            .remote_applied_world_state()
            .expect("applied world state should resolve");
        assert_eq!(
            applied.quest_lines[0].current_stage_ids,
            vec!["marked-by-kills"]
        );
        assert_eq!(applied.death_marks[0].applications, 2);

        let evaluation = client
            .remote_world_evaluation()
            .expect("world evaluation should resolve");
        assert_eq!(evaluation.controller_mix[0].agent_type, "neural_agent");
        assert_eq!(evaluation.controller_mix[0].row_count, 3);
        assert_eq!(evaluation.average_reward_per_row, 4.5);
        assert_eq!(evaluation.applied_objective_shift_count, 2);

        let messages = client.poll_updates();
        let delta = messages
            .into_iter()
            .find_map(|message| match message {
                ServerMessage::StateDelta { delta, .. } => Some(delta),
                _ => None,
            })
            .expect("state delta emitted after topology update");
        let entity = delta
            .updated
            .into_iter()
            .find(|entity| entity.id == 9)
            .expect("swarm vanguard snapshot updated");
        assert_eq!(entity.metadata.team_key.as_deref(), Some("gloam-mesh"));
        assert_eq!(entity.metadata.world_id.as_deref(), Some("deadman-shadow"));
        assert_eq!(entity.metadata.world_role, Some(WorldRealityRole::Shadow));
        assert_eq!(
            entity.metadata.world_active_quest_graph_ids,
            vec!["deadman-shadow-hunt".to_string()]
        );
    }

    #[test]
    fn test_receive_remote_topology_document_row_emits_debug_document_and_updates_state() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
            db_name: "deadman-shadow".into(),
            connection_mode: StdbConnectionMode::Emulated,
            ..Default::default()
        });
        client.connect().expect("client connects in emulated mode");
        client
            .subscribe_as_spectator()
            .expect("spectator subscriptions stage or apply");

        let mut cached = CachedEntity::from_entity(11, None, true);
        cached.team_id = Some(2);
        client.inner.upsert_entity(cached);

        let mut shadow =
            WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow-seasonal");
        shadow.role = WorldRealityRole::Shadow;
        shadow.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];

        let document = RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 42,
            tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            teams: vec![
                pod_core::AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime"),
                pod_core::AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow"),
            ],
            worlds: vec![shadow],
            links: vec![],
            world_quest_bindings: vec![WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-hunt".into()],
            }],
            world_admissions: vec![],
            world_control_planes: vec![],
            tournament_control_plane: pod_core::TournamentControlPlaneSummary::default(),
            tournament_orchestration: pod_core::TournamentOrchestrationSummary::default(),
            quest_graphs: vec![],
            applied_world_states: vec![],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![pod_core::WorldEvaluationSummary {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: WorldRealityRole::Shadow,
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
            .receive_remote_topology_document_row(
                7,
                42,
                "deadman-neural-cup",
                "ci-smoke",
                document.clone(),
            )
            .expect("document row applies");

        let messages = client.poll_updates();
        assert!(messages.iter().any(|message| matches!(
            message,
            ServerMessage::DebugDocument { document: current }
                if current == &document
        )));
        assert_eq!(client.last_debug_document(), Some(document.as_str()));
        assert_eq!(client.remote_world_id(), Some("deadman-shadow"));
        assert_eq!(
            client
                .remote_world_evaluation()
                .and_then(|world| world.controller_mix.first())
                .map(|controller| controller.agent_type.as_str()),
            Some("neural_agent")
        );
    }

    #[test]
    fn test_generated_runtime_topology_rows_flow_through_poll_updates() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
            db_name: "deadman-shadow".into(),
            connection_mode: StdbConnectionMode::Generated,
            ..Default::default()
        });
        let endpoint = client.install_generated_binding_runtime();
        let callbacks = endpoint.callbacks();
        client.connect().expect("generated runtime should connect");
        let commands = endpoint.drain_commands();
        assert!(matches!(
            commands.as_slice(),
            [GeneratedBindingCommand::Connect { config }]
                if config.db_name == "deadman-shadow"
                    && matches!(config.connection_mode, StdbConnectionMode::Generated)
        ));
        callbacks.connected(vec![7; 16], "tok-generated");

        let topology = RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "generated-test".into(),
            generated_at_unix_ms: 77,
            tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            teams: vec![pod_core::AgentTeamDefinition::new(
                "gloam-mesh",
                "Gloam Mesh",
                "deadman-shadow",
            )],
            worlds: vec![{
                let mut world =
                    WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow");
                world.role = WorldRealityRole::Shadow;
                world.active_team_ids = vec!["gloam-mesh".into()];
                world
            }],
            links: vec![],
            world_quest_bindings: vec![WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-hunt".into()],
            }],
            world_admissions: vec![],
            world_control_planes: vec![],
            tournament_control_plane: pod_core::TournamentControlPlaneSummary::default(),
            tournament_orchestration: pod_core::TournamentOrchestrationSummary::default(),
            quest_graphs: vec![],
            applied_world_states: vec![],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![pod_core::WorldEvaluationSummary {
                    world_id: "deadman-shadow".into(),
                    display_name: "Deadman Shadow".into(),
                    role: WorldRealityRole::Shadow,
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
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(13, &topology)
                .expect("callback row should build"),
        );

        let messages = client.poll_updates();
        let commands = endpoint.drain_commands();
        assert!(matches!(
            commands.as_slice(),
            [GeneratedBindingCommand::Subscribe { queries }]
                if queries.iter().any(|query| query == "SELECT * FROM remote_topology_document")
        ));
        assert!(messages.iter().any(|message| matches!(
            message,
            ServerMessage::DebugDocument { document: current }
                if current == &document
        )));
        assert_eq!(client.remote_world_id(), Some("deadman-shadow"));
        assert_eq!(
            client
                .remote_world_evaluation()
                .and_then(|world| world.controller_mix.first())
                .map(|controller| controller.agent_type.as_str()),
            Some("neural_agent")
        );
    }

    #[test]
    fn test_generated_runtime_topology_rows_update_same_world_quest_and_effect_state() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
            db_name: "deadman-shadow".into(),
            connection_mode: StdbConnectionMode::Generated,
            ..Default::default()
        });
        let endpoint = client.install_generated_binding_runtime();
        let callbacks = endpoint.callbacks();
        client.connect().expect("generated runtime should connect");
        let commands = endpoint.drain_commands();
        assert!(matches!(
            commands.as_slice(),
            [GeneratedBindingCommand::Connect { config }]
                if config.db_name == "deadman-shadow"
                    && matches!(config.connection_mode, StdbConnectionMode::Generated)
        ));
        callbacks.connected(vec![6; 16], "tok-generated");
        client.inner_mut().frame_tick();
        client
            .subscribe_as_spectator()
            .expect("spectator subscription should be requested");

        let mut cached = CachedEntity::from_entity(29, None, true);
        cached.team_id = Some(1);
        cached.name = Some("Bridge Relay".into());
        client.inner.upsert_entity(cached);

        let initial = RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "generated-test".into(),
            generated_at_unix_ms: 200,
            tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            teams: vec![pod_core::AgentTeamDefinition::new(
                "gloam-mesh",
                "Gloam Mesh",
                "deadman-shadow",
            )],
            worlds: vec![{
                let mut world =
                    WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow");
                world.role = WorldRealityRole::Shadow;
                world.active_team_ids = vec!["gloam-mesh".into()];
                world
            }],
            links: vec![],
            world_quest_bindings: vec![WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-hunt".into()],
            }],
            world_admissions: vec![],
            world_control_planes: vec![],
            tournament_control_plane: pod_core::TournamentControlPlaneSummary::default(),
            tournament_orchestration: pod_core::TournamentOrchestrationSummary::default(),
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
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
                    role: WorldRealityRole::Shadow,
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
        let updated = RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "generated-test".into(),
            generated_at_unix_ms: 260,
            tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            teams: vec![pod_core::AgentTeamDefinition::new(
                "gloam-mesh",
                "Gloam Mesh",
                "deadman-shadow",
            )],
            worlds: vec![{
                let mut world =
                    WorldRealityDefinition::new("deadman-shadow", "Deadman Shadow", "shadow");
                world.role = WorldRealityRole::Shadow;
                world.active_team_ids = vec!["gloam-mesh".into()];
                world
            }],
            links: vec![],
            world_quest_bindings: vec![WorldQuestBinding {
                world_id: "deadman-shadow".into(),
                quest_graph_ids: vec!["deadman-shadow-collapse".into()],
            }],
            world_admissions: vec![],
            world_control_planes: vec![],
            tournament_control_plane: pod_core::TournamentControlPlaneSummary::default(),
            tournament_orchestration: pod_core::TournamentOrchestrationSummary::default(),
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
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
                    role: WorldRealityRole::Shadow,
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

        let commands = endpoint.drain_commands();
        assert!(matches!(
            commands.as_slice(),
            [GeneratedBindingCommand::Subscribe { queries }]
                if queries.iter().any(|query| query == "SELECT * FROM remote_topology_document")
        ));
        callbacks.subscription_applied();
        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(31, &initial)
                .expect("initial callback row should build"),
        );
        let _ = client.poll_updates();

        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(32, &updated)
                .expect("updated callback row should build"),
        );
        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(30, &initial)
                .expect("stale callback row should build"),
        );

        let messages = client.poll_updates();
        let delta = messages
            .iter()
            .find_map(|message| match message {
                ServerMessage::StateDelta { delta, .. } => Some(delta),
                _ => None,
            })
            .expect("generated quest churn should rebuild snapshot metadata");
        let relay = delta
            .updated
            .iter()
            .find(|entity| entity.id == 29)
            .expect("tracked entity should be updated");
        assert_eq!(
            relay.metadata.world_active_quest_graph_ids,
            vec!["deadman-shadow-collapse".to_string()]
        );
        assert_eq!(
            client
                .remote_applied_world_state()
                .and_then(|state| state.quest_lines.first())
                .map(|quest| quest.quest_graph_id.as_str()),
            Some("deadman-shadow-collapse")
        );
        assert_eq!(
            client
                .remote_world_evaluation()
                .map(|world| world.average_reward_per_row),
            Some(6.25)
        );
        assert_eq!(
            client
                .remote_world_evaluation()
                .map(|world| world.applied_objective_shift_count),
            Some(4)
        );
    }

    #[test]
    fn test_generated_runtime_topology_rows_update_linked_world_quest_and_effect_state() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
            db_name: "deadman-shadow".into(),
            connection_mode: StdbConnectionMode::Generated,
            ..Default::default()
        });
        let endpoint = client.install_generated_binding_runtime();
        let callbacks = endpoint.callbacks();
        client.connect().expect("generated runtime should connect");
        let commands = endpoint.drain_commands();
        assert!(matches!(
            commands.as_slice(),
            [GeneratedBindingCommand::Connect { config }]
                if config.db_name == "deadman-shadow"
                    && matches!(config.connection_mode, StdbConnectionMode::Generated)
        ));
        callbacks.connected(vec![5; 16], "tok-generated");
        client.inner_mut().frame_tick();
        client
            .subscribe_as_spectator()
            .expect("spectator subscription should be requested");

        let mut cached = CachedEntity::from_entity(33, None, true);
        cached.team_id = Some(2);
        cached.name = Some("Swarm Vanguard".into());
        client.inner.upsert_entity(cached);

        let initial = RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "generated-test".into(),
            generated_at_unix_ms: 300,
            tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            teams: vec![
                pod_core::AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime"),
                pod_core::AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow"),
            ],
            worlds: vec![
                {
                    let mut world = WorldRealityDefinition::new(
                        "deadman-prime",
                        "Deadman Prime",
                        "deadman-seasonal",
                    );
                    world.role = WorldRealityRole::Tournament;
                    world.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];
                    world
                },
                {
                    let mut world = WorldRealityDefinition::new(
                        "deadman-shadow",
                        "Deadman Shadow",
                        "shadow-seasonal",
                    );
                    world.role = WorldRealityRole::Shadow;
                    world.linked_world_ids = vec!["deadman-prime".into()];
                    world.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];
                    world
                },
            ],
            links: vec![],
            world_quest_bindings: vec![
                WorldQuestBinding {
                    world_id: "deadman-prime".into(),
                    quest_graph_ids: vec!["deadman-prime-season".into()],
                },
                WorldQuestBinding {
                    world_id: "deadman-shadow".into(),
                    quest_graph_ids: vec!["deadman-shadow-hunt".into()],
                },
            ],
            world_admissions: vec![],
            world_control_planes: vec![],
            tournament_control_plane: pod_core::TournamentControlPlaneSummary::default(),
            tournament_orchestration: pod_core::TournamentOrchestrationSummary::default(),
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
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
                    role: WorldRealityRole::Shadow,
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
        let updated = RemoteTopologyBundle {
            version: pod_core::RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "generated-test".into(),
            generated_at_unix_ms: 360,
            tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            teams: vec![
                pod_core::AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime"),
                pod_core::AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow"),
            ],
            worlds: vec![
                {
                    let mut world = WorldRealityDefinition::new(
                        "deadman-prime",
                        "Deadman Prime",
                        "deadman-seasonal",
                    );
                    world.role = WorldRealityRole::Tournament;
                    world.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];
                    world
                },
                {
                    let mut world = WorldRealityDefinition::new(
                        "deadman-shadow",
                        "Deadman Shadow",
                        "shadow-seasonal",
                    );
                    world.role = WorldRealityRole::Shadow;
                    world.linked_world_ids = vec!["deadman-prime".into()];
                    world.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];
                    world
                },
            ],
            links: vec![],
            world_quest_bindings: vec![
                WorldQuestBinding {
                    world_id: "deadman-prime".into(),
                    quest_graph_ids: vec!["deadman-prime-season".into()],
                },
                WorldQuestBinding {
                    world_id: "deadman-shadow".into(),
                    quest_graph_ids: vec!["deadman-shadow-collapse".into()],
                },
            ],
            world_admissions: vec![],
            world_control_planes: vec![],
            tournament_control_plane: pod_core::TournamentControlPlaneSummary::default(),
            tournament_orchestration: pod_core::TournamentOrchestrationSummary::default(),
            quest_graphs: vec![],
            applied_world_states: vec![pod_core::AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
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
                    role: WorldRealityRole::Shadow,
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

        let commands = endpoint.drain_commands();
        assert!(matches!(
            commands.as_slice(),
            [GeneratedBindingCommand::Subscribe { queries }]
                if queries.iter().any(|query| query == "SELECT * FROM remote_topology_document")
        ));
        callbacks.subscription_applied();
        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(41, &initial)
                .expect("initial linked callback row should build"),
        );
        let _ = client.poll_updates();

        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(42, &updated)
                .expect("updated linked callback row should build"),
        );
        callbacks.remote_topology_document_insert(
            GeneratedRemoteTopologyDocumentRow::from_topology_bundle(40, &initial)
                .expect("stale linked callback row should build"),
        );

        let messages = client.poll_updates();
        let delta = messages
            .iter()
            .find_map(|message| match message {
                ServerMessage::StateDelta { delta, .. } => Some(delta),
                _ => None,
            })
            .expect("generated linked-world churn should rebuild snapshot metadata");
        let relay = delta
            .updated
            .iter()
            .find(|entity| entity.id == 33)
            .expect("tracked entity should be updated");
        assert_eq!(client.remote_world_id(), Some("deadman-shadow"));
        assert_eq!(relay.metadata.team_key.as_deref(), Some("gloam-mesh"));
        assert_eq!(
            relay.metadata.world_active_quest_graph_ids,
            vec!["deadman-shadow-collapse".to_string()]
        );
        assert_eq!(
            client
                .remote_applied_world_state()
                .and_then(|state| state.quest_lines.first())
                .map(|quest| quest.quest_graph_id.as_str()),
            Some("deadman-shadow-collapse")
        );
        assert_eq!(
            client
                .remote_applied_world_state()
                .and_then(|state| state.death_marks.first())
                .map(|mark| mark.total_duration_ticks),
            Some(1800)
        );
        assert_eq!(
            client
                .remote_world_evaluation()
                .map(|world| world.average_reward_per_row),
            Some(6.75)
        );
        assert_eq!(
            client
                .remote_world_evaluation()
                .map(|world| world.applied_score_delta_total),
            Some(14)
        );
        assert_eq!(
            client
                .remote_world_evaluation()
                .and_then(|world| world.controller_mix.first())
                .map(|controller| controller.row_count),
            Some(4)
        );
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
        assert_eq!(
            format!("{}", StdbClientError::Document("bad toon".into())),
            "SpacetimeDB document error: bad toon"
        );
    }

    #[test]
    fn test_error_from_stdb_error() {
        let err: StdbClientError = StdbError::NotConnected.into();
        assert!(matches!(err, StdbClientError::NotConnected));

        let err: StdbClientError = StdbError::ReducerError("fail".into()).into();
        assert!(matches!(err, StdbClientError::Reducer(msg) if msg == "fail"));

        let err: StdbClientError = StdbError::DocumentError("bad toon".into()).into();
        assert!(matches!(err, StdbClientError::Document(msg) if msg == "bad toon"));
    }

    #[test]
    fn test_new_client_not_connected() {
        let client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        assert!(!client.is_connected());
        assert!(client.client_id().is_none());
        assert!(client.local_snapshot().is_none());
    }

    #[test]
    fn test_install_generated_sdk_runtime_maps_live_connect_failure() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
            host: "http://127.0.0.1:1".into(),
            connection_mode: StdbConnectionMode::Generated,
            ..Default::default()
        });
        client.install_generated_sdk_runtime();

        let err = client
            .connect()
            .expect_err("closed localhost port should fail the live generated runtime");
        assert!(matches!(err, StdbClientError::Connection(_)));
    }

    #[test]
    fn test_run_rust_sdk_adapter_live_smoke_propagates_connect_failures() {
        let err = run_rust_sdk_adapter_live_smoke(&RustSdkAdapterLiveSmokeConfig {
            host: "http://127.0.0.1:1".into(),
            db_name: "prompt-or-die".into(),
            auth_token: None,
            timeout_ms: 100,
            poll_interval_ms: 1,
        })
        .expect_err("closed localhost port should fail the live rust sdk smoke");

        assert!(matches!(err, StdbClientError::Connection(_)));
    }

    #[test]
    fn test_presentation_snapshot_uses_local_snapshot_history() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        client.local_snapshot = Some(WorldSnapshot {
            tick: 10,
            entities: vec![EntitySnapshot {
                id: 7,
                position: Vec2::new(4.0, 2.0),
                velocity: Vec2::new(60.0, 0.0),
                rotation: 0.0,
                health: Some(90.0),
                max_health: Some(100.0),
                movement_speed: Some(120.0),
                label: Some("player".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
            population: pod_core::WorldPopulationState {
                tick: 10,
                ..Default::default()
            },
        });
        client.ingest_local_snapshot();

        let sampled = client.presentation_snapshot(1.0 / 60.0).unwrap();

        assert_eq!(sampled.snapshot.entities.len(), 1);
        assert!(sampled.snapshot.entities[0].position.x >= 4.0);
        assert!(client.presentation_tick().is_some());
    }

    #[test]
    fn test_diagnostics_and_rewind_surface_local_history() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        client.local_snapshot = Some(WorldSnapshot {
            tick: 10,
            entities: vec![EntitySnapshot {
                id: 3,
                position: Vec2::new(1.0, 2.0),
                velocity: Vec2::new(30.0, 0.0),
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: Some(90.0),
                label: Some("npc".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
            population: pod_core::WorldPopulationState {
                tick: 10,
                ..Default::default()
            },
        });
        client.ingest_local_snapshot();
        client.render_clock.advance(10, 1.0 / 60.0);

        let diagnostics = client.catch_up_diagnostics();
        assert_eq!(diagnostics.authoritative_tick, Some(10));
        assert_eq!(diagnostics.history_snapshots, 1);
        assert_eq!(client.rewind_authoritative_snapshot(5).unwrap().tick, 10);
        assert_eq!(client.rollback_preview(None).unwrap().baseline_tick, 10);
    }

    #[test]
    fn test_queue_action() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        client.queue_action(Action::Idle);
        client.queue_action(Action::Stop);
        assert_eq!(client.pending_actions.len(), 2);
    }

    #[test]
    fn test_connect_remote_agent_installs_transport_contract() {
        let client = build_connected_remote_agent_client();

        let contract = client
            .remote_agent_contract()
            .expect("remote agent contract should be installed");
        assert_eq!(contract.profile.agent_type, CoreAgentType::LlmAgent);
        assert_eq!(contract.action_budget.max_actions_per_tick, 3);
        assert_eq!(
            client.remote_agent_status().last_authoritative_tick,
            Some(0)
        );
    }

    #[test]
    fn test_send_actions_rejects_budget_overflow_for_remote_agent() {
        let mut client = build_connected_remote_agent_client();

        for _ in 0..4 {
            client.queue_action(Action::Idle);
        }

        let err = client
            .send_actions(0)
            .expect_err("too many queued actions should be rejected");
        assert!(matches!(err, StdbClientError::InvalidState(_)));
        assert_eq!(client.remote_agent_status().budget_overflow_rejections, 1);
        assert_eq!(
            client.remote_agent_status().fallback_reason,
            Some(RemoteAgentFallbackReason::ActionBudgetExceeded)
        );
        assert_eq!(client.remote_agent_status().pending_action_count, 0);
    }

    #[test]
    fn test_send_actions_rejects_stale_remote_observation() {
        let mut client = build_connected_remote_agent_client();
        client
            .inner_mut()
            .receive_observation(0, 1, "{\"tick\":0}".into());
        client.inner_mut().update_world_state(CachedWorldState {
            tick: 4,
            rng_seed: 42,
            ticks_per_second: 60,
            world_width: 2000.0,
            world_height: 2000.0,
            max_entities: 10000,
            paused: true,
        });
        client.queue_action(Action::Idle);

        let err = client
            .send_actions(4)
            .expect_err("stale observation should be rejected");
        assert!(matches!(err, StdbClientError::InvalidState(_)));
        assert_eq!(client.remote_agent_status().stale_action_rejections, 1);
        assert_eq!(client.remote_agent_status().stale_observation_ticks, 4);
        assert_eq!(
            client.remote_agent_status().fallback_reason,
            Some(RemoteAgentFallbackReason::ObservationStale)
        );
    }

    #[test]
    fn test_send_actions_rejects_timed_out_remote_observation() {
        let mut client = build_connected_remote_agent_client();
        client
            .inner_mut()
            .receive_observation(0, 1, "{\"tick\":0}".into());
        client.inner_mut().update_world_state(CachedWorldState {
            tick: 7,
            rng_seed: 42,
            ticks_per_second: 60,
            world_width: 2000.0,
            world_height: 2000.0,
            max_entities: 10000,
            paused: true,
        });
        client.queue_action(Action::Idle);

        let err = client
            .send_actions(7)
            .expect_err("timed out observation should be rejected");
        assert!(matches!(err, StdbClientError::InvalidState(_)));
        assert_eq!(client.remote_agent_status().timeout_rejections, 1);
        assert_eq!(
            client.remote_agent_status().fallback_reason,
            Some(RemoteAgentFallbackReason::HeartbeatTimedOut)
        );
    }

    #[test]
    fn test_fresh_observation_clears_remote_agent_fallback() {
        let mut client = build_connected_remote_agent_client();
        client
            .inner_mut()
            .receive_observation(0, 1, "{\"tick\":0}".into());
        client.inner_mut().update_world_state(CachedWorldState {
            tick: 4,
            rng_seed: 42,
            ticks_per_second: 60,
            world_width: 2000.0,
            world_height: 2000.0,
            max_entities: 10000,
            paused: true,
        });
        client.queue_action(Action::Idle);
        let _ = client.send_actions(4);
        assert!(client.remote_agent_status().fallback_active);

        client
            .inner_mut()
            .receive_observation(4, 1, "{\"tick\":4}".into());
        let _ = client.poll_updates();

        assert!(!client.remote_agent_status().fallback_active);
        assert_eq!(client.remote_agent_status().fallback_reason, None);
        assert_eq!(client.remote_agent_status().last_observation_tick, Some(4));
    }

    #[test]
    fn test_apply_rust_sdk_handoff_artifact_updates_remote_contract_and_forwards_replay() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig {
            db_name: "world-frontier-1".into(),
            connection_mode: StdbConnectionMode::Emulated,
            ..Default::default()
        });
        let mut artifact = build_rust_sdk_handoff_fixture();
        let observation = artifact.observation.payload.clone();
        artifact.latest_tick_telemetry = Some(VersionedTickTelemetry::new(TickTelemetryFrame {
            tick: observation.tick,
            agents: vec![AgentTelemetryFrame::new(
                observation.tick,
                observation.self_state.agent_id,
                Some(observation.self_state.entity_id),
                artifact.profile(),
                observation.visible_entities.len(),
                observation.audible_events.len(),
                observation.messages.len(),
                observation.available_actions.len(),
                observation.objectives.len(),
                None,
                None,
            )],
        }));

        client
            .apply_rust_sdk_handoff_artifact(artifact)
            .expect("handoff artifact should apply");

        assert_eq!(
            client
                .remote_agent_contract()
                .map(|contract| contract.profile.agent_type),
            Some(CoreAgentType::LlmAgent)
        );
        assert_eq!(
            client.remote_agent_status().last_observation_tick,
            Some(observation.tick)
        );
        assert_eq!(
            client.remote_agent_status().last_authoritative_tick,
            Some(observation.tick)
        );
        assert_eq!(client.remote_agent_status().stale_observation_ticks, 0);
        assert_eq!(client.remote_world_id(), Some("world-frontier-1"));
        assert_eq!(
            client
                .inner()
                .latest_observation_tick(observation.self_state.entity_id.0),
            Some(observation.tick)
        );

        let messages = client.poll_updates();
        assert!(messages.iter().any(|message| matches!(
            message,
            ServerMessage::DebugDocument { document }
                if document.contains("remote_topology_bundle")
        )));
        assert!(messages.iter().any(|message| matches!(
            message,
            ServerMessage::DebugDocument { document }
                if document.contains("versioned_tick_telemetry")
        )));
        let documents = client.drain_debug_documents();
        let replay = documents
            .iter()
            .find_map(|document| {
                if document.contains("\"document_type\":\"replay_file\"")
                    || document.contains("replay_file")
                {
                    decode_toon_document::<ReplayFile>(document, "replay_file").ok()
                } else {
                    None
                }
            })
            .expect("replay document should be retained");
        assert_eq!(replay.header.name, "rust-sdk-fixture");
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
    fn test_editor_debug_profile_includes_telemetry_queries() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        client.subscriptions.set_editor_debug();
        let queries = client.subscriptions.queries_for_profile();

        assert!(queries
            .iter()
            .any(|query| query.contains("agent_telemetry_tick")));
        assert!(queries
            .iter()
            .any(|query| query.contains("agent_tool_call_event")));
        assert!(queries
            .iter()
            .any(|query| query.contains("agent_tick_rollup")));
        assert!(queries
            .iter()
            .any(|query| query.contains("remote_topology_document")));
    }

    #[test]
    fn test_editor_debug_entity_profile_filters_telemetry_queries() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        client
            .subscriptions
            .set_editor_debug_entities(vec![44, 77, 44]);
        let queries = client.subscriptions.queries_for_profile();

        assert!(queries
            .iter()
            .any(|query| query == "SELECT * FROM agent_telemetry_tick WHERE agent_entity_id = 44"));
        assert!(
            queries
                .iter()
                .any(|query| query
                    == "SELECT * FROM agent_tool_call_event WHERE agent_entity_id = 77")
        );
        assert!(queries
            .iter()
            .any(|query| query == "SELECT * FROM agent_tick_rollup WHERE agent_entity_id = 44"));
        assert!(queries
            .iter()
            .any(|query| query == "SELECT * FROM remote_topology_document"));
        assert!(!queries
            .iter()
            .any(|query| query == "SELECT * FROM agent_telemetry_tick"));
        assert!(!queries
            .iter()
            .any(|query| query == "SELECT * FROM agent_tool_call_event"));
        assert!(!queries
            .iter()
            .any(|query| query == "SELECT * FROM agent_tick_rollup"));
    }

    #[test]
    fn test_sync_selected_entity_debug_focus_switches_between_scoped_and_editor_queries() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());

        client
            .sync_selected_entity_debug_focus(Some(44))
            .expect("selection sync should stage");
        let queries = client.subscriptions.queries_for_profile();
        assert!(queries
            .iter()
            .any(|query| query == "SELECT * FROM agent_telemetry_tick WHERE agent_entity_id = 44"));
        assert!(!queries
            .iter()
            .any(|query| query == "SELECT * FROM agent_telemetry_tick"));

        client
            .sync_selected_entity_debug_focus(None)
            .expect("clearing selection should stage editor profile");
        let queries = client.subscriptions.queries_for_profile();
        assert!(queries
            .iter()
            .any(|query| query.contains("FROM world_state")));
        assert!(!queries
            .iter()
            .any(|query| query.contains("FROM agent_telemetry_tick")));
    }

    #[test]
    fn test_poll_updates_emits_debug_documents_from_stdb_events() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        client.inner_mut().receive_agent_telemetry_tick(
            12,
            44,
            VersionedTickTelemetry::new(TickTelemetryFrame::empty(12)).to_toon_document(),
        );

        let messages = client.poll_updates();
        assert!(messages.iter().any(|message| matches!(
            message,
            ServerMessage::DebugDocument { document }
                if document.contains("versioned_tick_telemetry")
        )));
        assert!(client
            .last_debug_telemetry_document()
            .expect("debug telemetry stored")
            .contains("versioned_tick_telemetry"));
    }

    #[test]
    fn test_debug_documents_include_live_tool_rollup_replay_and_incident_docs() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        client.inner_mut().receive_agent_telemetry_tick(
            12,
            44,
            VersionedTickTelemetry::new(TickTelemetryFrame::empty(12)).to_toon_document(),
        );
        client.inner_mut().receive_agent_tool_call_event(
            12,
            44,
            "llm.complete".into(),
            "qwen".into(),
            "TimedOut".into(),
            AgentToolCallEvent::new(
                44,
                AgentToolCallTrace::failure(
                    12,
                    "llm.complete",
                    "qwen",
                    pod_core::ToolCallStatus::TimedOut,
                    48,
                    "timeout",
                ),
            )
            .to_toon_document(),
        );
        client.inner_mut().receive_agent_tick_rollup(
            1,
            60,
            44,
            AgentTickRollup {
                tick_start: 1,
                tick_end: 60,
                agent_entity_id: 44,
                total_distance: 18.5,
                submitted_action_count: 4,
                executed_action_count: 3,
                rejected_action_count: 1,
                tool_call_count: 1,
                tool_error_count: 1,
                visible_entity_count: 12,
                audible_event_count: 3,
                message_count: 2,
                average_tool_latency_ms: 48.0,
            }
            .to_toon_document(),
        );

        client.push_debug_document(
            ReplayFile {
                header: ReplayHeader {
                    name: "ops-replay".into(),
                    timestamp: 1_741_315_200,
                    world_seed: 42,
                    tick_count: 1,
                    agent_count: 0,
                    notes: "debug".into(),
                },
                traces: Vec::new(),
                telemetry_windows: vec![TickTelemetryFrame::empty(12)],
            }
            .to_toon_document(),
        );
        client.push_debug_document(
            ShardIncidentSummary {
                shard_id: "alpha-1".into(),
                latest_tick: 12,
                severity: pod_core::IncidentSeverity::Warning,
                summary: "Shard alpha-1 requires attention".into(),
                tick_budget_overrun_rate: 0.08,
                action_rejection_rate: 0.02,
                tool_call_error_rate: 0.11,
                average_tool_latency_ms: 820.0,
                average_trajectory_distance: 3.2,
                peak_entity_count: 512,
                peak_agent_count: 128,
                capture_actions: 4,
                summon_actions: 2,
                gather_actions: 7,
                loot_actions: 9,
                notes: vec!["tool-call error rate exceeds 10%".into()],
            }
            .to_toon_document(),
        );

        let _ = client.poll_updates();
        let documents = client.drain_debug_documents();
        assert!(documents
            .iter()
            .any(|document| document.contains("versioned_tick_telemetry")));
        assert!(documents
            .iter()
            .any(|document| document.contains("agent_tool_call_event")));
        assert!(documents
            .iter()
            .any(|document| document.contains("agent_tick_rollup")));
        assert!(documents
            .iter()
            .any(|document| document.contains("focused_entity_debug_summary")));
        assert!(documents
            .iter()
            .any(|document| document.contains("replay_file")));
        assert!(documents
            .iter()
            .any(|document| document.contains("shard_incident_summary")));
        assert!(client
            .last_debug_document()
            .expect("latest telemetry document retained")
            .contains("versioned_tick_telemetry"));
    }

    #[test]
    fn test_stdb_tool_and_rollup_documents_synthesize_focused_summary() {
        let mut client = SpacetimeDBClient::new(SpacetimeDBClientConfig::default());
        client.inner_mut().receive_agent_tool_call_event(
            18,
            44,
            "llm.complete".into(),
            "qwen".into(),
            "TimedOut".into(),
            AgentToolCallEvent::new(
                44,
                AgentToolCallTrace::failure(
                    18,
                    "llm.complete",
                    "qwen",
                    pod_core::ToolCallStatus::TimedOut,
                    96,
                    "timeout",
                ),
            )
            .to_toon_document(),
        );
        client.inner_mut().receive_agent_tick_rollup(
            1,
            60,
            44,
            AgentTickRollup {
                tick_start: 1,
                tick_end: 60,
                agent_entity_id: 44,
                total_distance: 24.5,
                submitted_action_count: 6,
                executed_action_count: 5,
                rejected_action_count: 1,
                tool_call_count: 2,
                tool_error_count: 1,
                visible_entity_count: 9,
                audible_event_count: 3,
                message_count: 4,
                average_tool_latency_ms: 72.0,
            }
            .to_toon_document(),
        );

        let _ = client.poll_updates();
        let documents = client.drain_debug_documents();
        let focused_summary = documents
            .iter()
            .rev()
            .filter_map(|document| {
                decode_toon_document::<FocusedEntityDebugSummary>(
                    document,
                    "focused_entity_debug_summary",
                )
                .ok()
            })
            .find(|summary| summary.entity_id == 44)
            .expect("focused summary should be synthesized");

        assert_eq!(focused_summary.latest_tick, 60);
        assert_eq!(focused_summary.tool_call_count, 2);
        assert_eq!(focused_summary.tool_error_count, 1);
        assert_eq!(focused_summary.rejected_action_count, 1);
        assert_eq!(focused_summary.visible_entity_count, 9);
        assert_eq!(
            focused_summary.latest_tool_name.as_deref(),
            Some("llm.complete")
        );
        assert_eq!(
            focused_summary.latest_tool_status.as_deref(),
            Some("TimedOut")
        );
        assert_eq!(
            focused_summary.latest_tool_error.as_deref(),
            Some("timeout")
        );
        assert!((focused_summary.total_distance - 24.5).abs() < f32::EPSILON);
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
