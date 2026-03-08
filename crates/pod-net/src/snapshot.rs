//! World state serialization for network transmission.
//!
//! Handles capturing world snapshots, computing efficient deltas, and
//! replaying a deterministic subset of client-side prediction.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Deref;

use pod_core::{
    Action, ActorPresentation, AtmosphereProfile, AtmosphereVolume, CombatLoadout,
    CombatPresentation, CombatStyle, CreatureIdentity, EncounterKind, EncounterProfile,
    EncounterState, FactionAffiliation, FactionDisposition, Health, Label, LootContainer, Movement,
    QuestAnchor, ResourceNode, SkillKind, SpawnProfile, Team, Transform,
    Velocity, WorldPopulationState,
};

const FIXED_TICK_DURATION_SECS: f32 = 1.0 / 60.0;
const DEFAULT_PREDICTION_MOVE_SPEED: f32 = 200.0;
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

/// Default network tick rate used for client-side presentation smoothing.
pub const DEFAULT_NETWORK_TICK_RATE: f32 = 60.0;

/// A serializable snapshot of world state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorldSnapshot {
    /// Tick number at time of capture
    pub tick: u64,
    /// All entity states
    pub entities: Vec<EntitySnapshot>,
    /// Authoritative streamed-world population state derived by the shard.
    pub population: WorldPopulationState,
}

/// A serialized entity state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySnapshot {
    /// Unique entity ID
    pub id: u64,
    /// Entity position
    pub position: Vec2,
    /// Entity velocity
    pub velocity: Vec2,
    /// Entity rotation (radians)
    pub rotation: f32,
    /// Health if applicable
    pub health: Option<f32>,
    /// Max health if applicable
    pub max_health: Option<f32>,
    /// Movement speed if applicable.
    pub movement_speed: Option<f32>,
    /// Entity label/name
    pub label: Option<String>,
    /// Authoritative gameplay metadata used by browser/editor consumers.
    pub metadata: EntityMetadataSnapshot,
}

/// Coarse authoritative classification for world entities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EntityKind {
    #[default]
    Unknown,
    Player,
    Npc,
    WildCreature,
    Companion,
    ResourceNode,
    LootContainer,
    Scenery,
}

/// Action affordances exposed to creator/browser tooling from authoritative state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntityInteractionHints {
    pub can_inspect: bool,
    pub can_interact: bool,
    pub can_attack: bool,
    pub can_gather: bool,
    pub can_loot: bool,
    pub can_capture: bool,
    pub can_command_companion: bool,
    pub can_chat: bool,
}

/// Rich per-entity metadata for browser/editor presentation and creator tooling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EntityMetadataSnapshot {
    pub kind: EntityKind,
    pub chunk_key: Option<String>,
    pub region_id: Option<String>,
    pub region_name: Option<String>,
    pub team_id: Option<u8>,
    pub quest_graph_ids: Vec<String>,
    pub faction_track_id: Option<String>,
    pub encounter_table_id: Option<String>,
    pub combat_style: Option<CombatStyle>,
    pub species_id: Option<String>,
    pub species_name: Option<String>,
    pub resource_skill: Option<SkillKind>,
    pub resource_tier: Option<u8>,
    pub encounter_kind: Option<EncounterKind>,
    pub faction: Option<FactionAffiliation>,
    pub quest_anchor: Option<QuestAnchor>,
    pub encounter_profile: Option<EncounterProfile>,
    pub spawn_profile: Option<SpawnProfile>,
    pub atmosphere: Option<AtmosphereProfile>,
    pub atmosphere_volume: Option<AtmosphereVolume>,
    pub actor_presentation: Option<ActorPresentation>,
    pub combat_presentation: Option<CombatPresentation>,
    pub interaction: EntityInteractionHints,
}

/// Efficient delta — only changed entities
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StateDelta {
    /// Tick number of this delta
    pub tick: u64,
    /// Updated entities
    pub updated: Vec<EntitySnapshot>,
    /// Destroyed entity IDs
    pub destroyed: Vec<u64>,
    /// Authoritative region/chunk population summary for the target tick.
    pub population: WorldPopulationState,
}

/// Tick-aligned locally predicted actions that have been sent to the server but
/// not yet acknowledged by the authoritative simulation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PredictedActionBatch {
    pub tick: u64,
    pub actions: Vec<Action>,
}

/// Result of reconciling local prediction against an authoritative update.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ReconciliationReport {
    pub authoritative_tick: u64,
    pub authoritative_digest: u64,
    pub acknowledged_action_tick: Option<u64>,
    pub pending_action_batches: usize,
    pub replayed_action_count: usize,
    pub predicted_digest: Option<u64>,
    pub used_hard_resync: bool,
}

/// Magnitude of divergence between two snapshots for a specific entity.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityDrift {
    pub entity_id: u64,
    pub position_error: f32,
    pub velocity_error: f32,
    pub rotation_error: f32,
    pub health_error: Option<f32>,
    pub max_health_error: Option<f32>,
    pub movement_speed_error: Option<f32>,
}

/// Preview of the local rollback/replay path that rebuilds prediction from an
/// authoritative rewind point plus unacknowledged input history.
#[derive(Debug, Clone)]
pub struct RollbackPreview {
    pub requested_rewind_tick: u64,
    pub baseline_tick: u64,
    pub replayed_batches: usize,
    pub replayed_action_count: usize,
    pub first_replayed_tick: Option<u64>,
    pub last_replayed_tick: Option<u64>,
    pub authoritative_digest: u64,
    pub predicted_digest: u64,
    pub predicted_snapshot: WorldSnapshot,
    pub controlled_entity_drift: Option<EntityDrift>,
}

/// Presentation/catch-up state useful for debugging prediction recovery.
#[derive(Debug, Clone, PartialEq)]
pub struct CatchUpDiagnostics {
    pub authoritative_tick: Option<u64>,
    pub authoritative_digest: Option<u64>,
    pub predicted_tick: Option<u64>,
    pub predicted_digest: Option<u64>,
    pub presentation_tick: Option<f32>,
    pub desired_presentation_tick: Option<f32>,
    pub presentation_drift_ticks: Option<f32>,
    pub history_snapshots: usize,
    pub oldest_authoritative_tick: Option<u64>,
    pub latest_authoritative_tick: Option<u64>,
    pub pending_action_batches: usize,
    pub replayed_action_count: usize,
    pub controlled_entity_drift: Option<EntityDrift>,
    pub recovery: RecoveryRequestState,
}

/// State for throttled full-snapshot recovery requests after drift or baseline
/// loss.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecoveryRequestState {
    pub awaiting_full_snapshot: bool,
    pub request_attempts: u32,
    pub last_request_server_tick: Option<u64>,
    pub last_request_digest: Option<u64>,
}

impl RecoveryRequestState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn can_request(&self, current_server_tick: u64, retry_interval_ticks: u64) -> bool {
        if !self.awaiting_full_snapshot {
            return true;
        }

        self.last_request_server_tick
            .map(|last_tick| current_server_tick >= last_tick.saturating_add(retry_interval_ticks))
            .unwrap_or(true)
    }

    pub fn record_request(&mut self, current_server_tick: u64, digest: Option<u64>) {
        self.awaiting_full_snapshot = true;
        self.request_attempts = self.request_attempts.saturating_add(1);
        self.last_request_server_tick = Some(current_server_tick);
        self.last_request_digest = digest;
    }

    pub fn next_retry_tick(&self, retry_interval_ticks: u64) -> Option<u64> {
        self.last_request_server_tick
            .map(|tick| tick.saturating_add(retry_interval_ticks))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SnapshotUpdateError {
    MissingBaseline {
        tick: u64,
    },
    DigestMismatch {
        tick: u64,
        expected: u64,
        actual: u64,
    },
}

/// Configuration for rendering/interpolating authoritative snapshots between
/// network updates while recovering smoothly from drift.
#[derive(Debug, Clone, PartialEq)]
pub struct InterpolationConfig {
    /// How many authoritative ticks behind the latest server tick the render
    /// clock should target by default.
    pub interpolation_delay_ticks: f32,
    /// Maximum number of future ticks a client may extrapolate beyond the last
    /// authoritative snapshot before clamping.
    pub max_extrapolation_ticks: f32,
    /// If the render clock drifts farther than this many ticks from the desired
    /// delayed target, snap directly instead of correcting gradually.
    pub snap_threshold_ticks: f32,
    /// Maximum number of ticks per second the render clock may correct by while
    /// catching up or slowing down toward the desired delayed target.
    pub catch_up_rate_ticks_per_second: f32,
    /// Maximum authoritative snapshots retained for interpolation history.
    pub history_limit: usize,
}

impl Default for InterpolationConfig {
    fn default() -> Self {
        Self {
            interpolation_delay_ticks: 2.0,
            max_extrapolation_ticks: 2.0,
            snap_threshold_ticks: 4.0,
            catch_up_rate_ticks_per_second: 8.0,
            history_limit: 32,
        }
    }
}

/// How a presentation snapshot was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotSampleMode {
    Exact,
    Interpolated,
    Extrapolated,
    ClampedPast,
    ClampedFuture,
}

/// A sampled world snapshot suitable for rendering/presentation.
#[derive(Debug, Clone)]
pub struct InterpolatedSnapshot {
    pub target_tick: f32,
    pub mode: SnapshotSampleMode,
    pub snapshot: WorldSnapshot,
}

/// Bounded authoritative history for rendering/interpolating remote state.
#[derive(Debug, Clone)]
pub struct SnapshotInterpolationBuffer {
    config: InterpolationConfig,
    snapshots: VecDeque<WorldSnapshot>,
}

impl Default for SnapshotInterpolationBuffer {
    fn default() -> Self {
        Self::new(InterpolationConfig::default())
    }
}

impl SnapshotInterpolationBuffer {
    pub fn new(config: InterpolationConfig) -> Self {
        Self {
            config,
            snapshots: VecDeque::new(),
        }
    }

    pub fn clear(&mut self) {
        self.snapshots.clear();
    }

    pub fn latest_tick(&self) -> Option<u64> {
        self.snapshots.back().map(|snapshot| snapshot.tick)
    }

    pub fn oldest_tick(&self) -> Option<u64> {
        self.snapshots.front().map(|snapshot| snapshot.tick)
    }

    pub fn snapshot_count(&self) -> usize {
        self.snapshots.len()
    }

    /// Rewinds to the newest authoritative snapshot at or before the requested
    /// tick. If the requested tick predates local history, the oldest retained
    /// snapshot is returned instead.
    pub fn rewind_to(&self, tick: u64) -> Option<WorldSnapshot> {
        self.snapshots
            .iter()
            .rev()
            .find(|snapshot| snapshot.tick <= tick)
            .cloned()
            .or_else(|| self.snapshots.front().cloned())
    }

    pub fn push(&mut self, snapshot: WorldSnapshot) {
        if let Some(existing) = self
            .snapshots
            .iter_mut()
            .find(|existing| existing.tick == snapshot.tick)
        {
            *existing = snapshot;
        } else if let Some(index) = self
            .snapshots
            .iter()
            .position(|existing| existing.tick > snapshot.tick)
        {
            self.snapshots.insert(index, snapshot);
        } else {
            self.snapshots.push_back(snapshot);
        }

        while self.snapshots.len() > self.config.history_limit {
            self.snapshots.pop_front();
        }
    }

    pub fn sample(&self, target_tick: f32) -> Option<InterpolatedSnapshot> {
        let first = self.snapshots.front()?;
        let last = self.snapshots.back()?;

        if let Some(exact) = self
            .snapshots
            .iter()
            .find(|snapshot| (snapshot.tick as f32 - target_tick).abs() <= f32::EPSILON)
        {
            return Some(InterpolatedSnapshot {
                target_tick,
                mode: SnapshotSampleMode::Exact,
                snapshot: exact.clone(),
            });
        }

        if target_tick <= first.tick as f32 {
            return Some(InterpolatedSnapshot {
                target_tick,
                mode: SnapshotSampleMode::ClampedPast,
                snapshot: first.clone(),
            });
        }

        let lower_index = self
            .snapshots
            .iter()
            .rposition(|snapshot| (snapshot.tick as f32) < target_tick)?;

        if let Some(upper) = self
            .snapshots
            .iter()
            .skip(lower_index + 1)
            .find(|snapshot| snapshot.tick as f32 > target_tick)
        {
            let lower = &self.snapshots[lower_index];
            let factor =
                (target_tick - lower.tick as f32) / (upper.tick as f32 - lower.tick as f32);

            return Some(InterpolatedSnapshot {
                target_tick,
                mode: SnapshotSampleMode::Interpolated,
                snapshot: interpolate_snapshots(lower, upper, factor, target_tick),
            });
        }

        let clamped_tick =
            target_tick.min(last.tick as f32 + self.config.max_extrapolation_ticks.max(0.0));
        let mode = if clamped_tick < target_tick {
            SnapshotSampleMode::ClampedFuture
        } else {
            SnapshotSampleMode::Extrapolated
        };

        Some(InterpolatedSnapshot {
            target_tick,
            mode,
            snapshot: extrapolate_snapshot(last, clamped_tick),
        })
    }
}

/// Render-time clock that stays a few authoritative ticks behind the server and
/// catches up smoothly as new snapshots arrive.
#[derive(Debug, Clone)]
pub struct RenderClock {
    config: InterpolationConfig,
    render_tick: Option<f32>,
}

impl Default for RenderClock {
    fn default() -> Self {
        Self::new(InterpolationConfig::default())
    }
}

impl RenderClock {
    pub fn new(config: InterpolationConfig) -> Self {
        Self {
            config,
            render_tick: None,
        }
    }

    pub fn reset(&mut self) {
        self.render_tick = None;
    }

    pub fn current_tick(&self) -> Option<f32> {
        self.render_tick
    }

    pub fn desired_tick(&self, latest_authoritative_tick: u64) -> f32 {
        latest_authoritative_tick as f32 - self.config.interpolation_delay_ticks.max(0.0)
    }

    pub fn drift_from_desired(&self, latest_authoritative_tick: u64) -> Option<f32> {
        self.render_tick
            .map(|render_tick| self.desired_tick(latest_authoritative_tick) - render_tick)
    }

    pub fn advance(&mut self, latest_authoritative_tick: u64, frame_delta_seconds: f32) -> f32 {
        let frame_delta_seconds = frame_delta_seconds.max(0.0);
        let desired_tick = self.desired_tick(latest_authoritative_tick);
        let mut render_tick = self.render_tick.unwrap_or(desired_tick);
        let drift = desired_tick - render_tick;

        if drift.abs() > self.config.snap_threshold_ticks {
            render_tick = desired_tick;
        } else {
            let max_correction =
                self.config.catch_up_rate_ticks_per_second.max(0.0) * frame_delta_seconds;
            render_tick += drift.clamp(-max_correction, max_correction);
        }

        render_tick += frame_delta_seconds * DEFAULT_NETWORK_TICK_RATE;
        render_tick = render_tick
            .min(latest_authoritative_tick as f32 + self.config.max_extrapolation_ticks.max(0.0));

        self.render_tick = Some(render_tick);
        render_tick
    }
}

impl WorldSnapshot {
    /// Capture current world state into a snapshot
    pub fn capture(world: &pod_core::World) -> Self {
        let mut entities = Vec::new();
        let population = world.population_state();
        let controlled_entities = world
            .agents
            .iter()
            .filter_map(|slot| slot.entity_id.map(|entity| entity.id() as u64))
            .collect::<HashSet<_>>();

        for (entity, (transform,)) in world.ecs.query::<(&Transform,)>().iter() {
            let id = entity.id();
            let streaming = world.resolve_streaming_metadata(transform.position);
            let velocity = world
                .ecs
                .get::<&Velocity>(entity)
                .map(|velocity| velocity.linear)
                .unwrap_or(Vec2::ZERO);
            let health_opt = world.ecs.get::<&Health>(entity).ok();
            let label_opt = world.ecs.get::<&Label>(entity).ok();
            let combat_loadout = world.ecs.get::<&CombatLoadout>(entity).ok();
            let creature = world.ecs.get::<&CreatureIdentity>(entity).ok();
            let resource = world.ecs.get::<&ResourceNode>(entity).ok();
            let loot = world.ecs.get::<&LootContainer>(entity).ok();
            let encounter = world.ecs.get::<&EncounterState>(entity).ok();
            let faction = world.ecs.get::<&FactionAffiliation>(entity).ok();
            let quest_anchor = world.ecs.get::<&QuestAnchor>(entity).ok();
            let encounter_profile = world.ecs.get::<&EncounterProfile>(entity).ok();
            let spawn_profile = world.ecs.get::<&SpawnProfile>(entity).ok();
            let atmosphere = world.ecs.get::<&AtmosphereProfile>(entity).ok();
            let atmosphere_volume = world.ecs.get::<&AtmosphereVolume>(entity).ok();
            let actor_presentation = world.ecs.get::<&ActorPresentation>(entity).ok();
            let combat_presentation = world.ecs.get::<&CombatPresentation>(entity).ok();

            let health = health_opt.as_ref().map(|h| h.current);
            let max_health = health_opt.as_ref().map(|h| h.max);
            let movement_speed = world.ecs.get::<&Movement>(entity).ok().map(|m| m.max_speed);
            let label = label_opt.as_ref().map(|label| label.name.clone());
            let team_id = label_opt.as_ref().and_then(|label| team_to_id(label.team));
            let kind = classify_entity_kind(
                id as u64,
                &controlled_entities,
                resource.is_some(),
                loot.is_some(),
                creature.as_deref(),
                combat_loadout.as_deref(),
                health_opt.as_deref(),
            );
            let interaction = interaction_hints_for_entity(
                kind,
                resource.as_deref(),
                loot.as_deref(),
                creature.as_deref(),
                combat_loadout.as_deref(),
                health_opt.as_deref(),
            );
            let mut quest_graph_ids = quest_anchor
                .as_ref()
                .map(|value| value.quest_ids.clone())
                .unwrap_or_default();
            for quest_graph_id in &streaming.quest_graph_ids {
                if !quest_graph_ids.contains(quest_graph_id) {
                    quest_graph_ids.push(quest_graph_id.clone());
                }
            }

            entities.push(EntitySnapshot {
                id: id as u64,
                position: transform.position,
                velocity,
                rotation: transform.rotation,
                health,
                max_health,
                movement_speed,
                label,
                metadata: EntityMetadataSnapshot {
                    kind,
                    chunk_key: Some(streaming.chunk_key.clone()),
                    region_id: streaming.region_id.clone(),
                    region_name: streaming.region_name.clone(),
                    team_id,
                    quest_graph_ids,
                    faction_track_id: faction
                        .as_ref()
                        .map(|value| value.faction_id.clone())
                        .or_else(|| streaming.faction_track_id.clone()),
                    encounter_table_id: encounter_profile
                        .as_ref()
                        .map(|value| value.table_id.clone())
                        .or_else(|| streaming.encounter_table_id.clone()),
                    combat_style: combat_loadout.as_ref().map(|loadout| loadout.style),
                    species_id: creature
                        .as_ref()
                        .map(|creature| creature.species_id.clone()),
                    species_name: creature
                        .as_ref()
                        .map(|creature| creature.species_name.clone()),
                    resource_skill: resource.as_ref().map(|resource| resource.skill),
                    resource_tier: resource.as_ref().map(|resource| resource.tier),
                    encounter_kind: encounter.as_ref().map(|encounter| encounter.kind),
                    faction: faction.map(|value| value.deref().clone()),
                    quest_anchor: quest_anchor.map(|value| value.deref().clone()),
                    encounter_profile: encounter_profile.map(|value| value.deref().clone()),
                    spawn_profile: spawn_profile.map(|value| value.deref().clone()),
                    atmosphere: atmosphere.map(|value| value.deref().clone()),
                    atmosphere_volume: atmosphere_volume.map(|value| *value.deref()),
                    actor_presentation: actor_presentation.map(|value| value.deref().clone()),
                    combat_presentation: combat_presentation.map(|value| value.deref().clone()),
                    interaction,
                },
            });
        }

        Self {
            tick: world.tick,
            entities,
            population,
        }
    }

    /// Apply a snapshot to a world (for client-side reconciliation)
    pub fn apply(&self, _world: &mut pod_core::World) {
        // For now, we'll just log this — full reconciliation
        // requires entity mapping and component management
        log::debug!(
            "Applying snapshot at tick {}: {} entities",
            self.tick,
            self.entities.len()
        );
    }

    /// Get entity count in snapshot
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Stable digest for client/server divergence detection.
    pub fn digest(&self) -> u64 {
        let mut hash = FNV_OFFSET_BASIS;
        hash_u64(&mut hash, self.tick);

        let mut entities = self.entities.iter().collect::<Vec<_>>();
        entities.sort_by_key(|entity| entity.id);

        for entity in entities {
            hash_u64(&mut hash, entity.id);
            hash_f32(&mut hash, entity.position.x);
            hash_f32(&mut hash, entity.position.y);
            hash_f32(&mut hash, entity.velocity.x);
            hash_f32(&mut hash, entity.velocity.y);
            hash_f32(&mut hash, entity.rotation);
            hash_option_f32(&mut hash, entity.health);
            hash_option_f32(&mut hash, entity.max_health);
            hash_option_f32(&mut hash, entity.movement_speed);
            hash_option_str(&mut hash, entity.label.as_deref());
            hash_entity_metadata(&mut hash, &entity.metadata);
        }

        hash_population_state(&mut hash, &self.population);

        hash
    }

    /// Replays locally predicted action batches over an authoritative snapshot.
    ///
    /// This intentionally models a narrow shared-sim subset: kinematic movement,
    /// stop, and facing updates for the locally controlled entity while advancing
    /// all entities by their current velocities between ticks.
    pub fn replay_predicted_actions(
        &self,
        controlled_entity: Option<u64>,
        batches: &[PredictedActionBatch],
    ) -> Self {
        let mut predicted = self.clone();

        for batch in batches {
            if let Some(controlled_entity) = controlled_entity {
                if let Some(entity) = predicted
                    .entities
                    .iter_mut()
                    .find(|entity| entity.id == controlled_entity)
                {
                    for action in &batch.actions {
                        apply_predicted_action(entity, action);
                    }
                }
            }

            for entity in &mut predicted.entities {
                entity.position += entity.velocity * FIXED_TICK_DURATION_SECS;
            }

            predicted.tick = batch.tick;
        }

        predicted
    }
}

impl StateDelta {
    /// Create a delta by comparing two snapshots
    pub fn diff(old: &WorldSnapshot, new: &WorldSnapshot) -> Self {
        let old_map: HashMap<u64, &EntitySnapshot> =
            old.entities.iter().map(|e| (e.id, e)).collect();

        let new_map: HashMap<u64, &EntitySnapshot> =
            new.entities.iter().map(|e| (e.id, e)).collect();

        let mut updated = Vec::new();
        let mut destroyed = Vec::new();

        // Find new and updated entities
        for (id, new_entity) in &new_map {
            if let Some(old_entity) = old_map.get(id) {
                // Check if anything changed
                if entity_changed(old_entity, new_entity) {
                    updated.push((*new_entity).clone());
                }
            } else {
                // New entity
                updated.push((*new_entity).clone());
            }
        }

        // Find destroyed entities
        for id in old_map.keys() {
            if !new_map.contains_key(id) {
                destroyed.push(*id);
            }
        }

        Self {
            tick: new.tick,
            updated,
            destroyed,
            population: new.population.clone(),
        }
    }

    /// Apply delta to a snapshot to create new snapshot
    pub fn apply_to(&self, snapshot: &WorldSnapshot) -> WorldSnapshot {
        let mut entities = snapshot.entities.clone();

        // Remove destroyed entities
        entities.retain(|e| !self.destroyed.contains(&e.id));

        // Update or add new entities
        for updated in &self.updated {
            if let Some(pos) = entities.iter_mut().find(|e| e.id == updated.id) {
                *pos = updated.clone();
            } else {
                entities.push(updated.clone());
            }
        }

        WorldSnapshot {
            tick: self.tick,
            entities,
            population: self.population.clone(),
        }
    }

    /// Get change count (for bandwidth estimation)
    pub fn change_count(&self) -> usize {
        self.updated.len() + self.destroyed.len()
    }
}

/// Check if an entity changed between snapshots
fn entity_changed(old: &EntitySnapshot, new: &EntitySnapshot) -> bool {
    // Allow small position/velocity deltas (floating point noise)
    const EPSILON: f32 = 0.01;

    old.position.distance(new.position) > EPSILON
        || old.velocity.distance(new.velocity) > EPSILON
        || (old.rotation - new.rotation).abs() > EPSILON
        || old.health != new.health
        || old.max_health != new.max_health
        || old.movement_speed != new.movement_speed
        || old.label != new.label
        || old.metadata != new.metadata
}

/// Applies an authoritative state update and validates its digest.
pub fn apply_authoritative_update(
    previous: Option<&WorldSnapshot>,
    tick: u64,
    is_full_snapshot: bool,
    delta: &StateDelta,
    authoritative_digest: u64,
) -> Result<WorldSnapshot, SnapshotUpdateError> {
    let snapshot = if is_full_snapshot {
        WorldSnapshot {
            tick,
            entities: delta.updated.clone(),
            population: delta.population.clone(),
        }
    } else {
        let previous = previous.ok_or(SnapshotUpdateError::MissingBaseline { tick })?;
        delta.apply_to(previous)
    };

    let actual_digest = snapshot.digest();
    if actual_digest != authoritative_digest {
        return Err(SnapshotUpdateError::DigestMismatch {
            tick,
            expected: authoritative_digest,
            actual: actual_digest,
        });
    }

    Ok(snapshot)
}

/// Overlays the locally predicted controlled entity onto an interpolated
/// authoritative snapshot, preserving responsive local control while still
/// smoothing remote entities from authoritative history.
pub fn compose_presentation_snapshot(
    mut sampled: InterpolatedSnapshot,
    predicted_local: Option<&WorldSnapshot>,
    controlled_entity: Option<u64>,
) -> InterpolatedSnapshot {
    let Some(controlled_entity) = controlled_entity else {
        return sampled;
    };

    let Some(predicted_entity) = predicted_local.and_then(|snapshot| {
        snapshot
            .entities
            .iter()
            .find(|entity| entity.id == controlled_entity)
    }) else {
        return sampled;
    };

    if let Some(existing) = sampled
        .snapshot
        .entities
        .iter_mut()
        .find(|entity| entity.id == controlled_entity)
    {
        *existing = predicted_entity.clone();
    } else {
        sampled.snapshot.entities.push(predicted_entity.clone());
    }

    sampled
}

/// Build a rollback preview by rewinding authoritative history to a requested
/// tick and replaying every locally predicted batch after that point.
pub fn build_rollback_preview(
    authoritative_history: &SnapshotInterpolationBuffer,
    rewind_tick: u64,
    controlled_entity: Option<u64>,
    prediction_history: &[PredictedActionBatch],
) -> Option<RollbackPreview> {
    let baseline = authoritative_history.rewind_to(rewind_tick)?;
    let replay_batches = prediction_history
        .iter()
        .filter(|batch| batch.tick > baseline.tick)
        .cloned()
        .collect::<Vec<_>>();
    let predicted_snapshot =
        baseline.replay_predicted_actions(controlled_entity, replay_batches.as_slice());

    Some(RollbackPreview {
        requested_rewind_tick: rewind_tick,
        baseline_tick: baseline.tick,
        replayed_batches: replay_batches.len(),
        replayed_action_count: replay_batches.iter().map(|batch| batch.actions.len()).sum(),
        first_replayed_tick: replay_batches.first().map(|batch| batch.tick),
        last_replayed_tick: replay_batches.last().map(|batch| batch.tick),
        authoritative_digest: baseline.digest(),
        predicted_digest: predicted_snapshot.digest(),
        controlled_entity_drift: controlled_entity.and_then(|entity_id| {
            entity_drift_between_snapshots(&baseline, &predicted_snapshot, entity_id)
        }),
        predicted_snapshot,
    })
}

/// Build a client-facing summary of prediction, presentation, and catch-up
/// state from the currently retained authoritative history.
pub fn build_catch_up_diagnostics(
    authoritative_history: &SnapshotInterpolationBuffer,
    authoritative_snapshot: Option<&WorldSnapshot>,
    predicted_snapshot: Option<&WorldSnapshot>,
    controlled_entity: Option<u64>,
    prediction_history: &[PredictedActionBatch],
    render_clock: &RenderClock,
    recovery: &RecoveryRequestState,
) -> CatchUpDiagnostics {
    let latest_authoritative_tick = authoritative_history
        .latest_tick()
        .or_else(|| authoritative_snapshot.map(|snapshot| snapshot.tick));
    let presentation_tick = render_clock.current_tick();
    let desired_presentation_tick =
        latest_authoritative_tick.map(|tick| render_clock.desired_tick(tick));
    let presentation_drift_ticks =
        latest_authoritative_tick.and_then(|tick| render_clock.drift_from_desired(tick));

    CatchUpDiagnostics {
        authoritative_tick: authoritative_snapshot
            .map(|snapshot| snapshot.tick)
            .or(latest_authoritative_tick),
        authoritative_digest: authoritative_snapshot.map(WorldSnapshot::digest),
        predicted_tick: predicted_snapshot.map(|snapshot| snapshot.tick),
        predicted_digest: predicted_snapshot.map(WorldSnapshot::digest),
        presentation_tick,
        desired_presentation_tick,
        presentation_drift_ticks,
        history_snapshots: authoritative_history.snapshot_count(),
        oldest_authoritative_tick: authoritative_history.oldest_tick(),
        latest_authoritative_tick,
        pending_action_batches: prediction_history.len(),
        replayed_action_count: prediction_history
            .iter()
            .map(|batch| batch.actions.len())
            .sum(),
        controlled_entity_drift: controlled_entity.and_then(|entity_id| {
            entity_drift_between_snapshots(authoritative_snapshot?, predicted_snapshot?, entity_id)
        }),
        recovery: recovery.clone(),
    }
}

fn interpolate_snapshots(
    lower: &WorldSnapshot,
    upper: &WorldSnapshot,
    factor: f32,
    target_tick: f32,
) -> WorldSnapshot {
    let factor = factor.clamp(0.0, 1.0);
    let mut upper_entities = upper
        .entities
        .iter()
        .map(|entity| (entity.id, entity))
        .collect::<HashMap<_, _>>();
    let mut entities = Vec::with_capacity(lower.entities.len());

    for lower_entity in &lower.entities {
        if let Some(upper_entity) = upper_entities.remove(&lower_entity.id) {
            entities.push(interpolate_entity(lower_entity, upper_entity, factor));
        } else {
            // An entity that disappears in the upper snapshot remains visible
            // until that authoritative tick is fully reached.
            entities.push(lower_entity.clone());
        }
    }

    WorldSnapshot {
        tick: target_tick.floor().max(0.0) as u64,
        entities,
        population: if factor < 0.5 {
            lower.population.clone()
        } else {
            upper.population.clone()
        },
    }
}

fn interpolate_entity(
    lower: &EntitySnapshot,
    upper: &EntitySnapshot,
    factor: f32,
) -> EntitySnapshot {
    EntitySnapshot {
        id: lower.id,
        position: lower.position.lerp(upper.position, factor),
        velocity: lower.velocity.lerp(upper.velocity, factor),
        rotation: lerp_f32(lower.rotation, upper.rotation, factor),
        health: lerp_option_f32(lower.health, upper.health, factor),
        max_health: lerp_option_f32(lower.max_health, upper.max_health, factor),
        movement_speed: lerp_option_f32(lower.movement_speed, upper.movement_speed, factor),
        label: if factor < 1.0 {
            lower.label.clone()
        } else {
            upper.label.clone()
        },
        metadata: if factor < 1.0 && lower.metadata == upper.metadata {
            lower.metadata.clone()
        } else {
            upper.metadata.clone()
        },
    }
}

fn extrapolate_snapshot(snapshot: &WorldSnapshot, target_tick: f32) -> WorldSnapshot {
    let dt_ticks = (target_tick - snapshot.tick as f32).max(0.0);
    let dt_seconds = dt_ticks * FIXED_TICK_DURATION_SECS;
    let mut extrapolated = snapshot.clone();
    extrapolated.tick = target_tick.floor().max(0.0) as u64;

    for entity in &mut extrapolated.entities {
        entity.position += entity.velocity * dt_seconds;
    }

    extrapolated
}

fn lerp_f32(lower: f32, upper: f32, factor: f32) -> f32 {
    lower + (upper - lower) * factor
}

fn lerp_option_f32(lower: Option<f32>, upper: Option<f32>, factor: f32) -> Option<f32> {
    match (lower, upper) {
        (Some(lower), Some(upper)) => Some(lerp_f32(lower, upper, factor)),
        (Some(lower), None) => Some(lower),
        (None, Some(upper)) if factor >= 1.0 => Some(upper),
        (None, Some(_)) => None,
        (None, None) => None,
    }
}

fn entity_drift_between_snapshots(
    authoritative: &WorldSnapshot,
    predicted: &WorldSnapshot,
    entity_id: u64,
) -> Option<EntityDrift> {
    let authoritative_entity = authoritative
        .entities
        .iter()
        .find(|entity| entity.id == entity_id)?;
    let predicted_entity = predicted
        .entities
        .iter()
        .find(|entity| entity.id == entity_id)?;

    Some(EntityDrift {
        entity_id,
        position_error: authoritative_entity
            .position
            .distance(predicted_entity.position),
        velocity_error: authoritative_entity
            .velocity
            .distance(predicted_entity.velocity),
        rotation_error: (authoritative_entity.rotation - predicted_entity.rotation).abs(),
        health_error: match (authoritative_entity.health, predicted_entity.health) {
            (Some(authoritative), Some(predicted)) => Some((authoritative - predicted).abs()),
            _ => None,
        },
        max_health_error: match (authoritative_entity.max_health, predicted_entity.max_health) {
            (Some(authoritative), Some(predicted)) => Some((authoritative - predicted).abs()),
            _ => None,
        },
        movement_speed_error: match (
            authoritative_entity.movement_speed,
            predicted_entity.movement_speed,
        ) {
            (Some(authoritative), Some(predicted)) => Some((authoritative - predicted).abs()),
            _ => None,
        },
    })
}

fn apply_predicted_action(entity: &mut EntitySnapshot, action: &Action) {
    match action {
        Action::Move { direction } => {
            let speed = entity
                .movement_speed
                .unwrap_or(DEFAULT_PREDICTION_MOVE_SPEED);
            entity.velocity = direction.normalize_or_zero() * speed;
        }
        Action::Stop => {
            entity.velocity = Vec2::ZERO;
        }
        Action::Rotate { angle } => {
            entity.rotation = *angle;
        }
        Action::LookAt { target } => {
            let delta = *target - entity.position;
            if delta.length_squared() > f32::EPSILON {
                entity.rotation = delta.y.atan2(delta.x);
            }
        }
        _ => {}
    }
}

fn hash_u64(hash: &mut u64, value: u64) {
    *hash ^= value;
    *hash = hash.wrapping_mul(FNV_PRIME);
}

fn hash_f32(hash: &mut u64, value: f32) {
    hash_u64(hash, value.to_bits() as u64);
}

fn hash_option_f32(hash: &mut u64, value: Option<f32>) {
    match value {
        Some(value) => {
            hash_u64(hash, 1);
            hash_f32(hash, value);
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_option_str(hash: &mut u64, value: Option<&str>) {
    match value {
        Some(value) => {
            hash_u64(hash, 1);
            for byte in value.as_bytes() {
                hash_u64(hash, *byte as u64);
            }
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_bool(hash: &mut u64, value: bool) {
    hash_u64(hash, value as u64);
}

fn hash_rgba(hash: &mut u64, value: [f32; 4]) {
    for channel in value {
        hash_f32(hash, channel);
    }
}

fn hash_rgb(hash: &mut u64, value: [f32; 3]) {
    for channel in value {
        hash_f32(hash, channel);
    }
}

fn hash_option_atmosphere(hash: &mut u64, atmosphere: Option<&AtmosphereProfile>) {
    match atmosphere {
        Some(atmosphere) => {
            hash_u64(hash, 1);
            hash_option_str(hash, Some(atmosphere.biome_id.as_str()));
            hash_rgba(hash, atmosphere.sky_color);
            hash_rgba(hash, atmosphere.fog_color);
            hash_f32(hash, atmosphere.fog_near);
            hash_f32(hash, atmosphere.fog_far);
            hash_rgb(hash, atmosphere.ambient_color);
            hash_f32(hash, atmosphere.ambient_intensity);
            hash_rgb(hash, atmosphere.sun_color);
            hash_f32(hash, atmosphere.sun_intensity);
            hash_rgb(hash, atmosphere.sun_direction);
            hash_rgb(hash, atmosphere.fill_color);
            hash_f32(hash, atmosphere.fill_intensity);
            hash_rgb(hash, atmosphere.fill_direction);
            hash_rgb(hash, atmosphere.rim_color);
            hash_f32(hash, atmosphere.rim_intensity);
            hash_rgba(hash, atmosphere.ground_color);
            hash_f32(hash, atmosphere.starfield_intensity);
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_string_list(hash: &mut u64, values: &[String]) {
    hash_u64(hash, values.len() as u64);
    for value in values {
        hash_option_str(hash, Some(value.as_str()));
    }
}

fn hash_option_faction(hash: &mut u64, faction: Option<&FactionAffiliation>) {
    match faction {
        Some(faction) => {
            hash_u64(hash, 1);
            hash_option_str(hash, Some(faction.faction_id.as_str()));
            hash_option_str(hash, Some(faction.role_id.as_str()));
            hash_option_str(
                hash,
                Some(match faction.disposition {
                    FactionDisposition::Friendly => "Friendly",
                    FactionDisposition::Neutral => "Neutral",
                    FactionDisposition::Hostile => "Hostile",
                }),
            );
            hash_f32(hash, faction.influence_radius);
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_option_quest_anchor(hash: &mut u64, quest_anchor: Option<&QuestAnchor>) {
    match quest_anchor {
        Some(quest_anchor) => {
            hash_u64(hash, 1);
            hash_string_list(hash, &quest_anchor.quest_ids);
            hash_option_str(hash, Some(quest_anchor.primary_prompt.as_str()));
            hash_string_list(hash, &quest_anchor.stage_tags);
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_option_encounter_profile(hash: &mut u64, encounter: Option<&EncounterProfile>) {
    match encounter {
        Some(encounter) => {
            hash_u64(hash, 1);
            hash_option_str(hash, Some(encounter.table_id.as_str()));
            hash_u64(hash, encounter.difficulty_tier as u64);
            hash_u64(hash, encounter.recommended_party_size as u64);
            hash_u64(hash, encounter.respawn_ticks as u64);
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_option_spawn_profile(hash: &mut u64, spawn: Option<&SpawnProfile>) {
    match spawn {
        Some(spawn) => {
            hash_u64(hash, 1);
            hash_option_str(hash, Some(spawn.profile_id.as_str()));
            hash_option_str(hash, Some(spawn.biome_id.as_str()));
            hash_option_str(hash, Some(spawn.spawn_group.as_str()));
            hash_u64(hash, spawn.respawn_ticks as u64);
            hash_f32(hash, spawn.leash_radius);
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_option_atmosphere_volume(hash: &mut u64, volume: Option<&AtmosphereVolume>) {
    match volume {
        Some(volume) => {
            hash_u64(hash, 1);
            hash_f32(hash, volume.radius);
            hash_u64(hash, volume.priority as u64);
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_option_actor_presentation(hash: &mut u64, presentation: Option<&ActorPresentation>) {
    match presentation {
        Some(presentation) => {
            hash_u64(hash, 1);
            hash_option_str(hash, Some(presentation.profile_id.as_str()));
            hash_option_str(hash, presentation.mesh_asset_id.as_deref());
            hash_option_str(hash, Some(presentation.material_palette_id.as_str()));
            hash_option_str(hash, Some(presentation.animation_set_id.as_str()));
            hash_f32(hash, presentation.scale_multiplier);
            hash_f32(hash, presentation.footprint_radius);
            hash_f32(hash, presentation.selection_ring_scale);
            hash_rgba(hash, presentation.aura_color);
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_option_combat_presentation(hash: &mut u64, presentation: Option<&CombatPresentation>) {
    match presentation {
        Some(presentation) => {
            hash_u64(hash, 1);
            hash_option_str(hash, Some(presentation.profile_id.as_str()));
            hash_rgba(hash, presentation.hit_flash_color);
            hash_rgba(hash, presentation.critical_ring_color);
            hash_rgba(hash, presentation.selection_ring_color);
            hash_rgb(hash, presentation.emissive_boost);
            hash_f32(hash, presentation.impact_scale);
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_option_u8(hash: &mut u64, value: Option<u8>) {
    match value {
        Some(value) => {
            hash_u64(hash, 1);
            hash_u64(hash, value as u64);
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_option_u64(hash: &mut u64, value: Option<u64>) {
    match value {
        Some(value) => {
            hash_u64(hash, 1);
            hash_u64(hash, value);
        }
        None => hash_u64(hash, 0),
    }
}

fn hash_population_breakdown(hash: &mut u64, counts: &pod_core::PopulationBreakdown) {
    hash_u64(hash, counts.players as u64);
    hash_u64(hash, counts.npcs as u64);
    hash_u64(hash, counts.wild_creatures as u64);
    hash_u64(hash, counts.companions as u64);
    hash_u64(hash, counts.resource_nodes as u64);
    hash_u64(hash, counts.loot_containers as u64);
    hash_u64(hash, counts.scenery as u64);
}

fn hash_population_state(hash: &mut u64, population: &WorldPopulationState) {
    hash_u64(hash, population.tick);
    hash_u64(hash, population.chunks.len() as u64);
    for chunk in &population.chunks {
        hash_option_str(hash, Some(chunk.chunk_key.as_str()));
        hash_option_str(hash, chunk.region_id.as_deref());
        hash_option_str(hash, chunk.region_name.as_deref());
        hash_option_str(hash, chunk.biome_id.as_deref());
        hash_string_list(hash, &chunk.quest_graph_ids);
        hash_option_str(hash, chunk.faction_track_id.as_deref());
        hash_string_list(hash, &chunk.encounter_table_ids);
        hash_population_breakdown(hash, &chunk.counts);
        hash_u64(hash, chunk.active_entity_count as u64);
        hash_u64(hash, chunk.ambient_population_cap as u64);
        hash_u64(hash, chunk.spawn_budget_remaining as u64);
        hash_u64(hash, chunk.pending_respawns as u64);
        hash_option_u64(hash, chunk.next_respawn_tick);
        hash_f32(hash, chunk.population_pressure);
    }
    hash_u64(hash, population.regions.len() as u64);
    for region in &population.regions {
        hash_option_str(hash, Some(region.region_id.as_str()));
        hash_option_str(hash, Some(region.region_name.as_str()));
        hash_option_str(hash, Some(region.primary_biome_id.as_str()));
        hash_string_list(hash, &region.chunk_keys);
        hash_string_list(hash, &region.active_quest_graph_ids);
        hash_option_str(hash, region.dominant_faction_track_id.as_deref());
        hash_string_list(hash, &region.encounter_table_ids);
        hash_u64(hash, region.active_chunk_count as u64);
        hash_population_breakdown(hash, &region.counts);
        hash_u64(hash, region.active_entity_count as u64);
        hash_u64(hash, region.ambient_population_cap as u64);
        hash_u64(hash, region.spawn_budget_remaining as u64);
        hash_u64(hash, region.pending_respawns as u64);
        hash_option_u64(hash, region.next_respawn_tick);
        hash_f32(hash, region.population_pressure);
    }
}

fn hash_entity_metadata(hash: &mut u64, metadata: &EntityMetadataSnapshot) {
    hash_option_str(hash, Some(entity_kind_name(metadata.kind)));
    hash_option_str(hash, metadata.chunk_key.as_deref());
    hash_option_str(hash, metadata.region_id.as_deref());
    hash_option_str(hash, metadata.region_name.as_deref());
    hash_option_u8(hash, metadata.team_id);
    hash_u64(hash, metadata.quest_graph_ids.len() as u64);
    for quest_graph_id in &metadata.quest_graph_ids {
        hash_option_str(hash, Some(quest_graph_id.as_str()));
    }
    hash_option_str(hash, metadata.faction_track_id.as_deref());
    hash_option_str(hash, metadata.encounter_table_id.as_deref());
    hash_option_str(hash, metadata.combat_style.map(combat_style_name));
    hash_option_str(hash, metadata.species_id.as_deref());
    hash_option_str(hash, metadata.species_name.as_deref());
    hash_option_str(hash, metadata.resource_skill.map(skill_kind_name));
    hash_option_u8(hash, metadata.resource_tier);
    hash_option_str(hash, metadata.encounter_kind.map(encounter_kind_name));
    hash_option_faction(hash, metadata.faction.as_ref());
    hash_option_quest_anchor(hash, metadata.quest_anchor.as_ref());
    hash_option_encounter_profile(hash, metadata.encounter_profile.as_ref());
    hash_option_spawn_profile(hash, metadata.spawn_profile.as_ref());
    hash_option_atmosphere(hash, metadata.atmosphere.as_ref());
    hash_option_atmosphere_volume(hash, metadata.atmosphere_volume.as_ref());
    hash_option_actor_presentation(hash, metadata.actor_presentation.as_ref());
    hash_option_combat_presentation(hash, metadata.combat_presentation.as_ref());
    hash_bool(hash, metadata.interaction.can_inspect);
    hash_bool(hash, metadata.interaction.can_interact);
    hash_bool(hash, metadata.interaction.can_attack);
    hash_bool(hash, metadata.interaction.can_gather);
    hash_bool(hash, metadata.interaction.can_loot);
    hash_bool(hash, metadata.interaction.can_capture);
    hash_bool(hash, metadata.interaction.can_command_companion);
    hash_bool(hash, metadata.interaction.can_chat);
}

fn team_to_id(team: Team) -> Option<u8> {
    match team {
        Team::None => None,
        Team::Team(id) => Some(id),
    }
}

fn classify_entity_kind(
    entity_id: u64,
    controlled_entities: &HashSet<u64>,
    has_resource: bool,
    has_loot: bool,
    creature: Option<&CreatureIdentity>,
    combat_loadout: Option<&CombatLoadout>,
    health: Option<&Health>,
) -> EntityKind {
    if has_resource {
        EntityKind::ResourceNode
    } else if has_loot {
        EntityKind::LootContainer
    } else if let Some(creature) = creature {
        if creature.is_wild {
            EntityKind::WildCreature
        } else {
            EntityKind::Companion
        }
    } else if controlled_entities.contains(&entity_id) {
        EntityKind::Player
    } else if combat_loadout.is_some() || health.is_some() {
        EntityKind::Npc
    } else {
        EntityKind::Scenery
    }
}

fn interaction_hints_for_entity(
    kind: EntityKind,
    resource: Option<&ResourceNode>,
    loot: Option<&LootContainer>,
    creature: Option<&CreatureIdentity>,
    combat_loadout: Option<&CombatLoadout>,
    health: Option<&Health>,
) -> EntityInteractionHints {
    let has_health = health.map(|health| health.current > 0.0).unwrap_or(false);
    let can_attack = matches!(
        kind,
        EntityKind::Player | EntityKind::Npc | EntityKind::WildCreature | EntityKind::Companion
    ) && (combat_loadout.is_some() || has_health);

    EntityInteractionHints {
        can_inspect: !matches!(kind, EntityKind::Unknown),
        can_interact: matches!(kind, EntityKind::Player | EntityKind::Npc),
        can_attack,
        can_gather: resource
            .map(|resource| resource.remaining_uses > 0)
            .unwrap_or(false),
        can_loot: loot
            .map(|loot| !loot.claimed && (loot.coins > 0 || !loot.items.is_empty()))
            .unwrap_or(false),
        can_capture: creature.map(|creature| creature.is_wild).unwrap_or(false),
        can_command_companion: matches!(kind, EntityKind::Companion),
        can_chat: matches!(kind, EntityKind::Player | EntityKind::Npc),
    }
}

fn entity_kind_name(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Unknown => "unknown",
        EntityKind::Player => "player",
        EntityKind::Npc => "npc",
        EntityKind::WildCreature => "wild_creature",
        EntityKind::Companion => "companion",
        EntityKind::ResourceNode => "resource_node",
        EntityKind::LootContainer => "loot_container",
        EntityKind::Scenery => "scenery",
    }
}

fn combat_style_name(style: CombatStyle) -> &'static str {
    match style {
        CombatStyle::Melee => "melee",
        CombatStyle::Ranged => "ranged",
        CombatStyle::Magic => "magic",
        CombatStyle::Summoning => "summoning",
    }
}

fn skill_kind_name(skill: SkillKind) -> &'static str {
    match skill {
        SkillKind::Attack => "attack",
        SkillKind::Strength => "strength",
        SkillKind::Defence => "defence",
        SkillKind::Ranged => "ranged",
        SkillKind::Magic => "magic",
        SkillKind::Constitution => "constitution",
        SkillKind::Mining => "mining",
        SkillKind::Woodcutting => "woodcutting",
        SkillKind::Fishing => "fishing",
        SkillKind::Cooking => "cooking",
        SkillKind::Smithing => "smithing",
        SkillKind::Crafting => "crafting",
        SkillKind::Slayer => "slayer",
        SkillKind::Taming => "taming",
        SkillKind::Bonding => "bonding",
    }
}

fn encounter_kind_name(kind: EncounterKind) -> &'static str {
    match kind {
        EncounterKind::OpenWorld => "open_world",
        EncounterKind::Duel => "duel",
        EncounterKind::WildCreature => "wild_creature",
        EncounterKind::Boss => "boss",
        EncounterKind::Raid => "raid",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::{
        ActorPresentation, AtmosphereProfile, AtmosphereVolume, CombatLoadout, CombatPresentation,
        CreatureIdentity, EncounterProfile, FactionAffiliation, FactionDisposition, IdleAgent,
        LootContainer, QuestAnchor, RegionEncounterTable, ResourceNode, SpawnProfile,
        WorldChunkDefinition, WorldRegionDefinition,
    };

    fn empty_population(tick: u64) -> WorldPopulationState {
        WorldPopulationState {
            tick,
            ..Default::default()
        }
    }

    fn snapshot_with_entities(tick: u64, entities: Vec<EntitySnapshot>) -> WorldSnapshot {
        WorldSnapshot {
            tick,
            entities,
            population: empty_population(tick),
        }
    }

    fn delta_with_updates(
        tick: u64,
        updated: Vec<EntitySnapshot>,
        destroyed: Vec<u64>,
    ) -> StateDelta {
        StateDelta {
            tick,
            updated,
            destroyed,
            population: empty_population(tick),
        }
    }

    #[test]
    fn test_snapshot_default() {
        let snap = WorldSnapshot::default();
        assert_eq!(snap.tick, 0);
        assert_eq!(snap.entity_count(), 0);
    }

    #[test]
    fn test_capture_includes_static_entities_and_authoritative_metadata() {
        let mut world = pod_core::World::new(7);
        let _ = world.add_agent(Box::new(IdleAgent::new()));
        world
            .spawn_at(32.0, 12.0)
            .with_label("Copper Vein", Team::None)
            .with_resource_node(ResourceNode::default())
            .build();
        world
            .spawn_at(36.0, 16.0)
            .with_label("Bronze Chest", Team::None)
            .with_loot_container(LootContainer {
                coins: 24,
                ..LootContainer::default()
            })
            .build();
        world
            .spawn_at(40.0, 20.0)
            .with_label("Wild Embercub", Team::None)
            .with_health(24.0)
            .with_combat_loadout(CombatLoadout::default())
            .with_actor_presentation(ActorPresentation {
                profile_id: "wild-creature".into(),
                mesh_asset_id: Some("rift-beast".into()),
                material_palette_id: "ember".into(),
                animation_set_id: "beast-stalker".into(),
                scale_multiplier: 1.15,
                footprint_radius: 1.45,
                selection_ring_scale: 2.8,
                aura_color: [0.92, 0.46, 0.22, 0.2],
            })
            .with_combat_presentation(CombatPresentation {
                profile_id: "ember-crit".into(),
                critical_ring_color: [0.95, 0.32, 0.18, 0.28],
                ..CombatPresentation::default()
            })
            .with_creature_identity(CreatureIdentity {
                species_id: "embercub".into(),
                species_name: "Wild Embercub".into(),
                is_wild: true,
                ..CreatureIdentity::default()
            })
            .with_faction_affiliation(FactionAffiliation {
                faction_id: "verdant-wilds".into(),
                role_id: "stalker".into(),
                disposition: FactionDisposition::Hostile,
                influence_radius: 18.0,
            })
            .with_encounter_profile(EncounterProfile {
                table_id: "verdant-predators".into(),
                difficulty_tier: 2,
                recommended_party_size: 1,
                respawn_ticks: 1_800,
            })
            .with_spawn_profile(SpawnProfile {
                profile_id: "predator-grove".into(),
                biome_id: "verdant-hollow".into(),
                spawn_group: "predators".into(),
                respawn_ticks: 900,
                leash_radius: 14.0,
            })
            .build();
        world
            .spawn_at(4.0, 4.0)
            .with_label("Verdant Atmosphere Anchor", Team::None)
            .with_atmosphere_profile(AtmosphereProfile {
                biome_id: "verdant-hollow".into(),
                fog_color: [0.05, 0.11, 0.09, 1.0],
                ground_color: [0.08, 0.15, 0.1, 1.0],
                ..AtmosphereProfile::default()
            })
            .with_atmosphere_volume(AtmosphereVolume {
                radius: 128.0,
                priority: 3,
            })
            .with_quest_anchor(QuestAnchor {
                quest_ids: vec!["discover-verdant-hollow".into()],
                primary_prompt: "Inspect the spire".into(),
                stage_tags: vec!["exploration".into(), "intro".into()],
            })
            .build();

        let snapshot = WorldSnapshot::capture(&world);

        assert_eq!(snapshot.entities.len(), 5);

        let player = snapshot
            .entities
            .iter()
            .find(|entity| entity.metadata.kind == EntityKind::Player)
            .expect("player entity present");
        assert!(player.metadata.interaction.can_chat);
        assert!(player.metadata.interaction.can_attack);
        assert_eq!(player.metadata.faction_track_id, None);

        let resource = snapshot
            .entities
            .iter()
            .find(|entity| entity.label.as_deref() == Some("Copper Vein"))
            .expect("resource present");
        assert_eq!(resource.metadata.kind, EntityKind::ResourceNode);
        assert_eq!(resource.metadata.resource_skill, Some(SkillKind::Mining));
        assert!(resource.metadata.interaction.can_gather);
        assert_eq!(resource.metadata.faction_track_id, None);

        let loot = snapshot
            .entities
            .iter()
            .find(|entity| entity.label.as_deref() == Some("Bronze Chest"))
            .expect("loot present");
        assert_eq!(loot.metadata.kind, EntityKind::LootContainer);
        assert!(loot.metadata.interaction.can_loot);

        let creature = snapshot
            .entities
            .iter()
            .find(|entity| entity.metadata.kind == EntityKind::WildCreature)
            .expect("wild creature present");
        assert_eq!(
            creature.metadata.faction_track_id.as_deref(),
            Some("verdant-wilds")
        );
        assert_eq!(
            creature.metadata.encounter_table_id.as_deref(),
            Some("verdant-predators")
        );
        assert_eq!(creature.metadata.quest_graph_ids, Vec::<String>::new());
        assert_eq!(creature.metadata.species_id.as_deref(), Some("embercub"));
        assert_eq!(
            creature.metadata.species_name.as_deref(),
            Some("Wild Embercub")
        );
        assert!(creature.metadata.interaction.can_capture);
        assert_eq!(
            creature
                .metadata
                .actor_presentation
                .as_ref()
                .and_then(|presentation| presentation.mesh_asset_id.as_deref()),
            Some("rift-beast")
        );
        assert_eq!(
            creature
                .metadata
                .combat_presentation
                .as_ref()
                .map(|presentation| presentation.profile_id.as_str()),
            Some("ember-crit")
        );
        assert_eq!(
            creature
                .metadata
                .faction
                .as_ref()
                .map(|faction| faction.faction_id.as_str()),
            Some("verdant-wilds")
        );
        assert_eq!(
            creature
                .metadata
                .encounter_profile
                .as_ref()
                .map(|encounter| encounter.table_id.as_str()),
            Some("verdant-predators")
        );
        assert_eq!(
            creature
                .metadata
                .spawn_profile
                .as_ref()
                .map(|spawn| spawn.profile_id.as_str()),
            Some("predator-grove")
        );

        let atmosphere = snapshot
            .entities
            .iter()
            .find(|entity| entity.label.as_deref() == Some("Verdant Atmosphere Anchor"))
            .expect("atmosphere anchor present");
        assert_eq!(
            atmosphere
                .metadata
                .atmosphere
                .as_ref()
                .map(|atmosphere| atmosphere.biome_id.as_str()),
            Some("verdant-hollow")
        );
        assert_eq!(
            atmosphere
                .metadata
                .atmosphere_volume
                .as_ref()
                .map(|volume| volume.priority),
            Some(3)
        );
        assert_eq!(
            atmosphere
                .metadata
                .quest_anchor
                .as_ref()
                .map(|quest| quest.primary_prompt.as_str()),
            Some("Inspect the spire")
        );
    }

    #[test]
    fn test_capture_derives_region_and_chunk_metadata_from_world_catalog() {
        let mut world = pod_core::World::new(11);

        let mut heart_chunk = WorldChunkDefinition::new("0:0", "verdant-heart", "verdant-hollow");
        heart_chunk.quest_graph_ids.push("verdant-intro".into());
        heart_chunk.faction_track_ids.push("verdant-wardens".into());
        heart_chunk
            .encounter_table_ids
            .push("verdant-heart-wildlife".into());

        let mut heart_region =
            WorldRegionDefinition::new("verdant-heart", "Verdant Heart", "verdant-hollow");
        heart_region.chunk_keys.push("0:0".into());
        heart_region
            .active_quest_graph_ids
            .push("verdant-intro".into());
        heart_region.dominant_faction_track_id = "verdant-wardens".into();
        heart_region
            .encounter_table_ids
            .push("verdant-heart-wildlife".into());

        world.set_streaming_metadata(
            8.0,
            vec![heart_chunk],
            vec![heart_region],
            vec![RegionEncounterTable::new(
                "verdant-heart-wildlife",
                "verdant-hollow",
                "wildlife",
                vec![],
            )],
        );

        world
            .spawn_at(2.0, 2.0)
            .with_label("Quest Stela", Team::None)
            .build();
        world
            .spawn_at(3.0, 3.0)
            .with_label("Wild Lynx", Team::None)
            .with_faction_affiliation(FactionAffiliation {
                faction_id: "verdant-wilds".into(),
                role_id: "hunter".into(),
                disposition: FactionDisposition::Hostile,
                influence_radius: 16.0,
            })
            .with_encounter_profile(EncounterProfile {
                table_id: "lynx-pack".into(),
                difficulty_tier: 2,
                recommended_party_size: 1,
                respawn_ticks: 600,
            })
            .with_quest_anchor(QuestAnchor {
                quest_ids: vec!["lynx-patrol".into()],
                primary_prompt: "Track the wild lynx".into(),
                stage_tags: vec!["hunt".into()],
            })
            .build();

        let snapshot = WorldSnapshot::capture(&world);

        let stela = snapshot
            .entities
            .iter()
            .find(|entity| entity.label.as_deref() == Some("Quest Stela"))
            .expect("stela present");
        assert_eq!(stela.metadata.chunk_key.as_deref(), Some("0:0"));
        assert_eq!(stela.metadata.region_id.as_deref(), Some("verdant-heart"));
        assert_eq!(stela.metadata.region_name.as_deref(), Some("Verdant Heart"));
        assert_eq!(
            stela.metadata.quest_graph_ids,
            vec!["verdant-intro".to_string()]
        );
        assert_eq!(
            stela.metadata.faction_track_id.as_deref(),
            Some("verdant-wardens")
        );
        assert_eq!(
            stela.metadata.encounter_table_id.as_deref(),
            Some("verdant-heart-wildlife")
        );

        let lynx = snapshot
            .entities
            .iter()
            .find(|entity| entity.label.as_deref() == Some("Wild Lynx"))
            .expect("lynx present");
        assert_eq!(lynx.metadata.chunk_key.as_deref(), Some("0:0"));
        assert_eq!(
            lynx.metadata.quest_graph_ids,
            vec!["lynx-patrol".to_string(), "verdant-intro".to_string()]
        );
        assert_eq!(
            lynx.metadata.faction_track_id.as_deref(),
            Some("verdant-wilds")
        );
        assert_eq!(
            lynx.metadata.encounter_table_id.as_deref(),
            Some("lynx-pack")
        );
    }

    #[test]
    fn test_delta_diff() {
        let mut snap1 = WorldSnapshot::default();
        snap1.entities.push(EntitySnapshot {
            id: 1,
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::new(1.0, 0.0),
            rotation: 0.0,
            health: Some(100.0),
            max_health: Some(100.0),
            movement_speed: Some(200.0),
            label: Some("Player".into()),
            metadata: EntityMetadataSnapshot::default(),
        });

        let mut snap2 = WorldSnapshot::default();
        snap2.entities.push(EntitySnapshot {
            id: 1,
            position: Vec2::new(1.0, 0.0), // Changed
            velocity: Vec2::new(1.0, 0.0),
            rotation: 0.0,
            health: Some(100.0),
            max_health: Some(100.0),
            movement_speed: Some(200.0),
            label: Some("Player".into()),
            metadata: EntityMetadataSnapshot::default(),
        });

        let delta = StateDelta::diff(&snap1, &snap2);
        assert_eq!(delta.updated.len(), 1);
        assert_eq!(delta.destroyed.len(), 0);
    }

    #[test]
    fn test_delta_apply() {
        let mut snap1 = WorldSnapshot::default();
        snap1.entities.push(EntitySnapshot {
            id: 1,
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::ZERO,
            rotation: 0.0,
            health: None,
            max_health: None,
            movement_speed: None,
            label: None,
            metadata: EntityMetadataSnapshot::default(),
        });

        let delta = delta_with_updates(
            1,
            vec![EntitySnapshot {
                id: 2,
                position: Vec2::new(5.0, 5.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: None,
                label: None,
                metadata: EntityMetadataSnapshot::default(),
            }],
            vec![],
        );

        let snap2 = delta.apply_to(&snap1);
        assert_eq!(snap2.entities.len(), 2);
    }

    #[test]
    fn test_delta_ignores_small_jitter_as_unchanged() {
        let mut snap1 = WorldSnapshot::default();
        snap1.entities.push(EntitySnapshot {
            id: 1,
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::new(0.0, 0.0),
            rotation: 0.0,
            health: None,
            max_health: None,
            movement_speed: Some(200.0),
            label: Some("sprite2d".into()),
            metadata: EntityMetadataSnapshot::default(),
        });

        let mut snap2 = WorldSnapshot::default();
        snap2.entities.push(EntitySnapshot {
            id: 1,
            position: Vec2::new(0.005, -0.005),
            velocity: Vec2::new(0.002, 0.003),
            rotation: 0.003,
            health: None,
            max_health: None,
            movement_speed: Some(200.0),
            label: Some("sprite2d".into()),
            metadata: EntityMetadataSnapshot::default(),
        });

        let delta = StateDelta::diff(&snap1, &snap2);
        assert_eq!(delta.updated.len(), 0);
        assert_eq!(delta.destroyed.len(), 0);
    }

    #[test]
    fn test_delta_prefers_updated_mode_like_label_change_over_destroy() {
        let mut base = WorldSnapshot::default();
        base.entities.push(EntitySnapshot {
            id: 12,
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::ZERO,
            rotation: 0.0,
            health: Some(100.0),
            max_health: Some(100.0),
            movement_speed: Some(200.0),
            label: Some("sprite2d".into()),
            metadata: EntityMetadataSnapshot::default(),
        });

        let mut next = WorldSnapshot::default();
        next.entities.push(EntitySnapshot {
            id: 12,
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::ZERO,
            rotation: 0.0,
            health: Some(100.0),
            max_health: Some(100.0),
            movement_speed: Some(200.0),
            label: Some("sprite3d".into()),
            metadata: EntityMetadataSnapshot::default(),
        });

        let delta = StateDelta::diff(&base, &next);
        assert_eq!(delta.updated.len(), 1);
        assert_eq!(delta.updated[0].label.as_deref(), Some("sprite3d"));

        let delta_with_destroy = delta_with_updates(2, delta.updated.clone(), vec![12]);

        let applied = delta_with_destroy.apply_to(&base);
        assert_eq!(applied.entities.len(), 1);
        let updated = &applied.entities[0];
        assert_eq!(updated.id, 12);
        assert_eq!(updated.label.as_deref(), Some("sprite3d"));
    }

    #[test]
    fn test_delta_apply_keeps_existing_updates_when_destroy_contains_extra_ids() {
        let mut base = WorldSnapshot::default();
        base.entities.push(EntitySnapshot {
            id: 10,
            position: Vec2::new(0.0, 0.0),
            velocity: Vec2::new(0.0, 0.0),
            rotation: 0.0,
            health: Some(100.0),
            max_health: Some(100.0),
            movement_speed: None,
            label: Some("keep".into()),
            metadata: EntityMetadataSnapshot::default(),
        });
        base.entities.push(EntitySnapshot {
            id: 11,
            position: Vec2::new(1.0, 1.0),
            velocity: Vec2::new(0.0, 0.0),
            rotation: 0.0,
            health: Some(50.0),
            max_health: Some(50.0),
            movement_speed: None,
            label: Some("to_remove".into()),
            metadata: EntityMetadataSnapshot::default(),
        });

        let delta = delta_with_updates(
            10,
            vec![EntitySnapshot {
                id: 10,
                position: Vec2::new(5.0, 5.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                health: Some(100.0),
                max_health: Some(100.0),
                movement_speed: None,
                label: Some("updated".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
            vec![11, 99_999],
        );

        let applied = delta.apply_to(&base);
        assert_eq!(applied.entities.len(), 1);
        assert_eq!(applied.entities[0].id, 10);
        assert_eq!(
            applied
                .entities
                .iter()
                .find(|e| e.id == 10)
                .unwrap()
                .label
                .as_deref(),
            Some("updated")
        );
        assert!(applied.entities.iter().all(|entity| entity.id != 99_999));
    }

    #[test]
    fn test_snapshot_digest_is_stable_for_entity_order() {
        let entity_a = EntitySnapshot {
            id: 1,
            position: Vec2::new(1.0, 2.0),
            velocity: Vec2::new(3.0, 4.0),
            rotation: 0.5,
            health: Some(10.0),
            max_health: Some(20.0),
            movement_speed: Some(200.0),
            label: Some("a".into()),
            metadata: EntityMetadataSnapshot::default(),
        };
        let entity_b = EntitySnapshot {
            id: 2,
            position: Vec2::new(5.0, 6.0),
            velocity: Vec2::new(7.0, 8.0),
            rotation: 1.5,
            health: None,
            max_health: None,
            movement_speed: None,
            label: Some("b".into()),
            metadata: EntityMetadataSnapshot::default(),
        };

        let snapshot_a = snapshot_with_entities(4, vec![entity_a.clone(), entity_b.clone()]);
        let snapshot_b = snapshot_with_entities(4, vec![entity_b, entity_a]);

        assert_eq!(snapshot_a.digest(), snapshot_b.digest());
    }

    #[test]
    fn test_replay_predicted_actions_replays_move_and_stop() {
        let snapshot = snapshot_with_entities(
            10,
            vec![EntitySnapshot {
                id: 7,
                position: Vec2::ZERO,
                velocity: Vec2::ZERO,
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: Some(120.0),
                label: Some("player".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
        );

        let predicted = snapshot.replay_predicted_actions(
            Some(7),
            &[
                PredictedActionBatch {
                    tick: 11,
                    actions: vec![Action::Move { direction: Vec2::X }],
                },
                PredictedActionBatch {
                    tick: 12,
                    actions: vec![Action::Stop],
                },
            ],
        );

        let entity = &predicted.entities[0];
        assert_eq!(predicted.tick, 12);
        assert!((entity.position.x - 2.0).abs() < 0.001);
        assert_eq!(entity.velocity, Vec2::ZERO);
    }

    #[test]
    fn test_interpolation_buffer_replaces_same_tick_snapshot() {
        let mut buffer = SnapshotInterpolationBuffer::default();

        buffer.push(snapshot_with_entities(
            10,
            vec![EntitySnapshot {
                id: 1,
                position: Vec2::ZERO,
                velocity: Vec2::ZERO,
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: None,
                label: Some("old".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
        ));
        buffer.push(snapshot_with_entities(
            10,
            vec![EntitySnapshot {
                id: 1,
                position: Vec2::new(5.0, 0.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: None,
                label: Some("new".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
        ));

        let sampled = buffer.sample(10.0).unwrap();
        assert_eq!(sampled.mode, SnapshotSampleMode::Exact);
        assert_eq!(sampled.snapshot.entities.len(), 1);
        assert_eq!(sampled.snapshot.entities[0].label.as_deref(), Some("new"));
        assert_eq!(sampled.snapshot.entities[0].position, Vec2::new(5.0, 0.0));
    }

    #[test]
    fn test_interpolation_buffer_lerps_shared_entities() {
        let mut buffer = SnapshotInterpolationBuffer::default();
        buffer.push(snapshot_with_entities(
            10,
            vec![EntitySnapshot {
                id: 1,
                position: Vec2::new(0.0, 0.0),
                velocity: Vec2::new(10.0, 0.0),
                rotation: 0.0,
                health: Some(80.0),
                max_health: Some(100.0),
                movement_speed: Some(120.0),
                label: Some("player".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
        ));
        buffer.push(snapshot_with_entities(
            12,
            vec![EntitySnapshot {
                id: 1,
                position: Vec2::new(12.0, 0.0),
                velocity: Vec2::new(14.0, 0.0),
                rotation: 1.0,
                health: Some(60.0),
                max_health: Some(100.0),
                movement_speed: Some(160.0),
                label: Some("player".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
        ));

        let sampled = buffer.sample(11.0).unwrap();
        let entity = &sampled.snapshot.entities[0];

        assert_eq!(sampled.mode, SnapshotSampleMode::Interpolated);
        assert!((entity.position.x - 6.0).abs() < 0.001);
        assert!((entity.velocity.x - 12.0).abs() < 0.001);
        assert!((entity.rotation - 0.5).abs() < 0.001);
        assert!((entity.health.unwrap() - 70.0).abs() < 0.001);
        assert!((entity.movement_speed.unwrap() - 140.0).abs() < 0.001);
    }

    #[test]
    fn test_interpolation_buffer_extrapolates_latest_snapshot() {
        let mut buffer = SnapshotInterpolationBuffer::default();
        buffer.push(snapshot_with_entities(
            5,
            vec![EntitySnapshot {
                id: 9,
                position: Vec2::new(1.0, 2.0),
                velocity: Vec2::new(60.0, 0.0),
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: Some(60.0),
                label: Some("npc".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
        ));

        let sampled = buffer.sample(6.0).unwrap();
        let entity = &sampled.snapshot.entities[0];

        assert_eq!(sampled.mode, SnapshotSampleMode::Extrapolated);
        assert!((entity.position.x - 2.0).abs() < 0.001);
        assert!((entity.position.y - 2.0).abs() < 0.001);
    }

    #[test]
    fn test_interpolation_buffer_clamps_past_and_future() {
        let mut buffer = SnapshotInterpolationBuffer::new(InterpolationConfig {
            max_extrapolation_ticks: 1.0,
            ..InterpolationConfig::default()
        });
        buffer.push(snapshot_with_entities(
            10,
            vec![EntitySnapshot {
                id: 1,
                position: Vec2::new(5.0, 0.0),
                velocity: Vec2::new(30.0, 0.0),
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: None,
                label: None,
                metadata: EntityMetadataSnapshot::default(),
            }],
        ));

        let past = buffer.sample(9.0).unwrap();
        assert_eq!(past.mode, SnapshotSampleMode::ClampedPast);
        assert_eq!(past.snapshot.tick, 10);

        let future = buffer.sample(13.0).unwrap();
        let entity = &future.snapshot.entities[0];
        assert_eq!(future.mode, SnapshotSampleMode::ClampedFuture);
        assert!((entity.position.x - 5.5).abs() < 0.001);
    }

    #[test]
    fn test_render_clock_advances_and_snaps_on_large_drift() {
        let mut clock = RenderClock::default();

        let first = clock.advance(20, 1.0 / 60.0);
        assert!((first - 19.0).abs() < 0.001);

        let second = clock.advance(40, 0.0);
        assert!((second - 38.0).abs() < 0.001);
    }

    #[test]
    fn test_interpolation_buffer_rewind_returns_latest_snapshot_before_tick() {
        let mut buffer = SnapshotInterpolationBuffer::default();
        buffer.push(snapshot_with_entities(10, vec![]));
        buffer.push(snapshot_with_entities(15, vec![]));
        buffer.push(snapshot_with_entities(20, vec![]));

        assert_eq!(buffer.rewind_to(16).unwrap().tick, 15);
        assert_eq!(buffer.rewind_to(8).unwrap().tick, 10);
    }

    #[test]
    fn test_build_rollback_preview_replays_batches_after_rewind_tick() {
        let mut buffer = SnapshotInterpolationBuffer::default();
        buffer.push(snapshot_with_entities(
            10,
            vec![EntitySnapshot {
                id: 7,
                position: Vec2::ZERO,
                velocity: Vec2::ZERO,
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: Some(120.0),
                label: Some("player".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
        ));
        buffer.push(snapshot_with_entities(
            12,
            vec![EntitySnapshot {
                id: 7,
                position: Vec2::new(2.0, 0.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                health: None,
                max_health: None,
                movement_speed: Some(120.0),
                label: Some("player".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
        ));

        let preview = build_rollback_preview(
            &buffer,
            11,
            Some(7),
            &[
                PredictedActionBatch {
                    tick: 11,
                    actions: vec![Action::Move { direction: Vec2::X }],
                },
                PredictedActionBatch {
                    tick: 12,
                    actions: vec![Action::Stop],
                },
                PredictedActionBatch {
                    tick: 13,
                    actions: vec![Action::Move { direction: Vec2::Y }],
                },
            ],
        )
        .unwrap();

        assert_eq!(preview.requested_rewind_tick, 11);
        assert_eq!(preview.baseline_tick, 10);
        assert_eq!(preview.replayed_batches, 3);
        assert_eq!(preview.replayed_action_count, 3);
        assert_eq!(preview.first_replayed_tick, Some(11));
        assert_eq!(preview.last_replayed_tick, Some(13));
        assert_eq!(preview.predicted_snapshot.tick, 13);
        let controlled = preview
            .predicted_snapshot
            .entities
            .iter()
            .find(|entity| entity.id == 7)
            .unwrap();
        assert!((controlled.position.x - 2.0).abs() < 0.001);
        assert!((controlled.position.y - 2.0).abs() < 0.001);
        assert_eq!(
            preview
                .controlled_entity_drift
                .as_ref()
                .map(|drift| drift.entity_id),
            Some(7)
        );
    }

    #[test]
    fn test_build_catch_up_diagnostics_reports_drift_and_history_window() {
        let authoritative = snapshot_with_entities(
            12,
            vec![EntitySnapshot {
                id: 1,
                position: Vec2::new(6.0, 0.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                health: Some(10.0),
                max_health: Some(10.0),
                movement_speed: Some(120.0),
                label: Some("player".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
        );
        let predicted = snapshot_with_entities(
            13,
            vec![EntitySnapshot {
                id: 1,
                position: Vec2::new(8.0, 0.0),
                velocity: Vec2::new(120.0, 0.0),
                rotation: 0.5,
                health: Some(10.0),
                max_health: Some(10.0),
                movement_speed: Some(120.0),
                label: Some("player".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
        );
        let mut buffer = SnapshotInterpolationBuffer::default();
        buffer.push(snapshot_with_entities(10, authoritative.entities.clone()));
        buffer.push(snapshot_with_entities(11, authoritative.entities.clone()));
        buffer.push(authoritative.clone());

        let mut clock = RenderClock::default();
        let presentation_tick = clock.advance(12, 1.0 / 60.0);
        assert!((presentation_tick - 11.0).abs() < 0.001);

        let mut recovery = RecoveryRequestState::default();
        recovery.record_request(12, Some(authoritative.digest()));

        let diagnostics = build_catch_up_diagnostics(
            &buffer,
            Some(&authoritative),
            Some(&predicted),
            Some(1),
            &[PredictedActionBatch {
                tick: 13,
                actions: vec![Action::Move { direction: Vec2::X }],
            }],
            &clock,
            &recovery,
        );

        assert_eq!(diagnostics.authoritative_tick, Some(12));
        assert_eq!(diagnostics.predicted_tick, Some(13));
        assert_eq!(diagnostics.history_snapshots, 3);
        assert_eq!(diagnostics.oldest_authoritative_tick, Some(10));
        assert_eq!(diagnostics.latest_authoritative_tick, Some(12));
        assert_eq!(diagnostics.pending_action_batches, 1);
        assert_eq!(diagnostics.replayed_action_count, 1);
        assert_eq!(diagnostics.presentation_tick, Some(11.0));
        assert_eq!(diagnostics.desired_presentation_tick, Some(10.0));
        assert_eq!(diagnostics.presentation_drift_ticks, Some(-1.0));
        assert!(
            diagnostics
                .controlled_entity_drift
                .as_ref()
                .unwrap()
                .position_error
                > 1.9
        );
        assert!(diagnostics.recovery.awaiting_full_snapshot);
        assert_eq!(diagnostics.recovery.request_attempts, 1);
        assert_eq!(diagnostics.recovery.last_request_server_tick, Some(12));
    }

    #[test]
    fn test_recovery_request_state_throttles_retries() {
        let mut recovery = RecoveryRequestState::default();
        assert!(recovery.can_request(10, 5));

        recovery.record_request(10, Some(99));
        assert!(!recovery.can_request(12, 5));
        assert!(recovery.can_request(15, 5));
        assert_eq!(recovery.next_retry_tick(5), Some(15));

        recovery.clear();
        assert_eq!(recovery, RecoveryRequestState::default());
    }

    #[test]
    fn test_compose_presentation_snapshot_overlays_predicted_controlled_entity() {
        let sampled = InterpolatedSnapshot {
            target_tick: 12.0,
            mode: SnapshotSampleMode::Interpolated,
            snapshot: snapshot_with_entities(
                12,
                vec![
                    EntitySnapshot {
                        id: 1,
                        position: Vec2::new(5.0, 0.0),
                        velocity: Vec2::new(1.0, 0.0),
                        rotation: 0.0,
                        health: None,
                        max_health: None,
                        movement_speed: Some(120.0),
                        label: Some("player".into()),
                        metadata: EntityMetadataSnapshot::default(),
                    },
                    EntitySnapshot {
                        id: 2,
                        position: Vec2::new(10.0, 0.0),
                        velocity: Vec2::ZERO,
                        rotation: 0.0,
                        health: None,
                        max_health: None,
                        movement_speed: None,
                        label: Some("npc".into()),
                        metadata: EntityMetadataSnapshot::default(),
                    },
                ],
            ),
        };
        let predicted_local = snapshot_with_entities(
            13,
            vec![EntitySnapshot {
                id: 1,
                position: Vec2::new(8.0, 1.0),
                velocity: Vec2::new(3.0, 0.0),
                rotation: 1.0,
                health: None,
                max_health: None,
                movement_speed: Some(120.0),
                label: Some("player".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
        );

        let composed = compose_presentation_snapshot(sampled, Some(&predicted_local), Some(1));

        assert_eq!(composed.snapshot.entities.len(), 2);
        let controlled = composed
            .snapshot
            .entities
            .iter()
            .find(|entity| entity.id == 1)
            .unwrap();
        assert_eq!(controlled.position, Vec2::new(8.0, 1.0));
        assert_eq!(controlled.velocity, Vec2::new(3.0, 0.0));
    }

    #[test]
    fn test_apply_authoritative_update_requires_baseline_for_delta() {
        let delta = delta_with_updates(5, vec![], vec![]);

        let error = apply_authoritative_update(None, 5, false, &delta, 0).unwrap_err();
        assert_eq!(error, SnapshotUpdateError::MissingBaseline { tick: 5 });
    }

    #[test]
    fn test_apply_authoritative_update_accepts_full_snapshot() {
        let full = delta_with_updates(
            5,
            vec![EntitySnapshot {
                id: 1,
                position: Vec2::new(2.0, 3.0),
                velocity: Vec2::X,
                rotation: 0.25,
                health: Some(10.0),
                max_health: Some(10.0),
                movement_speed: Some(200.0),
                label: Some("player".into()),
                metadata: EntityMetadataSnapshot::default(),
            }],
            vec![],
        );

        let expected_snapshot = WorldSnapshot {
            tick: 5,
            entities: full.updated.clone(),
            population: full.population.clone(),
        };

        let applied =
            apply_authoritative_update(None, 5, true, &full, expected_snapshot.digest()).unwrap();

        assert_eq!(applied.digest(), expected_snapshot.digest());
    }
}
