//! SpacetimeDB reducers — atomic game logic operations.
//!
//! Reducers are the ONLY way to mutate game state. They execute as atomic
//! transactions within SpacetimeDB. This mirrors the tick pipeline from pod-core:
//!
//!   1. Build Observations (perception queries)
//!   2. Deliver & Collect Decisions (agents return actions)
//!   3. Validate & Execute Actions (constraints enforced identically for all agents)
//!   4. Physics/Movement (velocity integration)
//!   5. Flush Events (broadcast to listeners)

use crate::events::*;
use crate::tables::*;
use crate::types::*;
use glam::Vec2;
use pod_core::action::{
    AbilityTarget as CoreAbilityTarget, Action as CoreAction,
    CompanionCommand as CoreCompanionCommand, SpeakVolume as CoreSpeakVolume,
};
use pod_core::agent::AgentType as CoreAgentType;
use pod_core::component::SkillKind;
use pod_core::contract::AgentRuntimeProfile;
use pod_core::id::{AgentId as CoreAgentId, EntityId as CoreEntityId};
use pod_core::telemetry::{
    ActionLifecycleStage, ActionSource, AgentTelemetryFrame, AgentTickRollup, AgentToolCallEvent,
    TickTelemetryFrame, TrajectorySample,
};
use pod_core::{
    decode_toon_document, RemoteTopologyBundle, VersionedTickTelemetry,
    REMOTE_AGENT_MAX_ACTIONS_PER_TICK,
};
use serde_json::{json, Value};
use spacetimedb::{Identity, ReducerContext, Table};
use std::collections::HashMap;
use uuid::Uuid;

const TELEMETRY_RETENTION_TICKS: u64 = 600;
const TOOL_EVENT_RETENTION_TICKS: u64 = 36_000;
const ROLLUP_RETENTION_TICKS: u64 = 36_000;
const ROLLUP_WINDOW_TICKS: u64 = 60;

#[derive(Debug, Clone, PartialEq, Eq)]
struct RemoteTopologyDocumentPublishSummary {
    generated_at_unix_ms: u64,
    scenario_id: String,
    profile_id: String,
    world_count: u32,
    team_count: u32,
}

#[derive(Debug, Default, Clone, Copy)]
struct ObservationCounts {
    visible_entities: usize,
    audible_events: usize,
    messages: usize,
}

fn reject_reducer(ctx: &ReducerContext, reason: impl Into<String>) {
    let reason_text = reason.into();
    log::warn!("[pod-stdb][reject] {reason_text}");

    if let Some(ws) = ctx.db.world_state().id().find(0) {
        ctx.db.world_event().insert(WorldEventRow {
            event_id: 0,
            tick: ws.tick,
            event_kind: WorldEventKind::TickAdvanced,
            entity_id: 0,
            secondary_entity_id: None,
            data_json: json!({
                "type": "reducer_reject",
                "reason": reason_text,
            })
            .to_string(),
        });
    }
}

fn summarize_remote_topology_document(
    topology_json: &str,
) -> Result<RemoteTopologyDocumentPublishSummary, String> {
    let topology: RemoteTopologyBundle =
        decode_toon_document(topology_json, "remote_topology_bundle")?;
    let generated_at_unix_ms = u64::try_from(topology.generated_at_unix_ms).map_err(|_| {
        format!(
            "remote_topology_bundle generated_at_unix_ms {} exceeds u64",
            topology.generated_at_unix_ms
        )
    })?;
    let world_count = u32::try_from(topology.worlds.len())
        .map_err(|_| format!("world count {} exceeds u32", topology.worlds.len()))?;
    let team_count = u32::try_from(topology.teams.len())
        .map_err(|_| format!("team count {} exceeds u32", topology.teams.len()))?;

    Ok(RemoteTopologyDocumentPublishSummary {
        generated_at_unix_ms,
        scenario_id: topology.scenario_id,
        profile_id: topology.profile_id,
        world_count,
        team_count,
    })
}

// ============================================================
// LIFECYCLE REDUCERS
// ============================================================

/// Initialize the game world with default configuration.
/// Called once when the SpacetimeDB module is first published.
#[spacetimedb::reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.world_state().insert(WorldStateRow {
        id: 0,
        tick: 0,
        rng_seed: 42,
        ticks_per_second: 60,
        world_width: 2000.0,
        world_height: 2000.0,
        max_entities: 10000,
        paused: true,
    });
    log::info!("[pod-stdb] World initialized (paused, seed=42, 2000x2000)");
}

/// Handle client connection.
#[spacetimedb::reducer(client_connected)]
pub fn client_connected(ctx: &ReducerContext) {
    log::info!("[pod-stdb] Client connected: {:?}", ctx.sender);
}

/// Handle client disconnection — clean up connected agent state.
#[spacetimedb::reducer(client_disconnected)]
pub fn client_disconnected(ctx: &ReducerContext) {
    let identity = ctx.sender;

    // Remove from connected agents table
    if let Some(agent) = ctx.db.connected_agent().identity().find(identity) {
        let eid = agent.entity_id;
        ctx.db.connected_agent().identity().delete(identity);
        log::info!("[pod-stdb] Client disconnected: {identity:?}, was controlling entity {eid}");
    } else {
        log::info!("[pod-stdb] Client disconnected: {identity:?} (no agent)");
    }

    // Remove lobby membership, if any
    for membership in ctx
        .db
        .lobby_member()
        .iter()
        .filter(|member| member.identity == identity)
    {
        ctx.db
            .lobby_member()
            .membership_id()
            .delete(membership.membership_id);
    }
}

// ============================================================
// WORLD MANAGEMENT REDUCERS
// ============================================================

/// Create or reset the game world with custom parameters.
#[spacetimedb::reducer]
pub fn create_world(ctx: &ReducerContext, seed: u64, width: f32, height: f32, tps: u32) {
    if let Some(mut ws) = ctx.db.world_state().id().find(0) {
        ws.tick = 0;
        ws.rng_seed = seed;
        ws.world_width = width;
        ws.world_height = height;
        ws.ticks_per_second = tps;
        ws.paused = true;
        ctx.db.world_state().id().update(ws);
        log::info!("[pod-stdb] World reset: seed={seed}, size={width}x{height}, tps={tps}");
    } else {
        ctx.db.world_state().insert(WorldStateRow {
            id: 0,
            tick: 0,
            rng_seed: seed,
            ticks_per_second: tps,
            world_width: width,
            world_height: height,
            max_entities: 10000,
            paused: true,
        });
        log::info!("[pod-stdb] World created: seed={seed}, size={width}x{height}, tps={tps}");
    }
}

/// Publish the latest shared multi-world topology document for remote clients.
#[spacetimedb::reducer]
pub fn publish_remote_topology_document(ctx: &ReducerContext, topology_json: String) {
    let summary = match summarize_remote_topology_document(&topology_json) {
        Ok(summary) => summary,
        Err(error) => {
            reject_reducer(
                ctx,
                format!("publish_remote_topology_document rejected: {error}"),
            );
            return;
        }
    };

    let stale_row_ids = ctx
        .db
        .remote_topology_document()
        .iter()
        .filter(|row| {
            row.scenario_id == summary.scenario_id && row.profile_id == summary.profile_id
        })
        .map(|row| row.row_id)
        .collect::<Vec<_>>();
    for row_id in stale_row_ids {
        ctx.db.remote_topology_document().row_id().delete(row_id);
    }

    ctx.db
        .remote_topology_document()
        .insert(RemoteTopologyDocumentRow {
            row_id: 0,
            generated_at_unix_ms: summary.generated_at_unix_ms,
            scenario_id: summary.scenario_id,
            profile_id: summary.profile_id,
            world_count: summary.world_count,
            team_count: summary.team_count,
            topology_json,
        });
}

/// Pause or unpause the world simulation.
#[spacetimedb::reducer]
pub fn set_paused(ctx: &ReducerContext, paused: bool) {
    if let Some(mut ws) = ctx.db.world_state().id().find(0) {
        ws.paused = paused;
        ctx.db.world_state().id().update(ws);
        log::info!("[pod-stdb] World paused={paused}");
    }
}

// ============================================================
// ENTITY MANAGEMENT REDUCERS
// ============================================================

/// Spawn a new entity with a transform at the given position.
/// Optionally assign an agent type. Returns the entity via auto_inc.
#[spacetimedb::reducer]
pub fn spawn_entity(ctx: &ReducerContext, pos_x: f32, pos_y: f32, agent_type: Option<AgentType>) {
    let ws = ctx
        .db
        .world_state()
        .id()
        .find(0)
        .expect("World not initialized — call create_world first");

    // Insert entity row (entity_id = 0 triggers auto_inc)
    let entity = ctx.db.entity().insert(EntityRow {
        entity_id: 0,
        agent_type,
        owner_identity: None,
        alive: true,
        created_tick: ws.tick,
    });
    let eid = entity.entity_id;

    // Insert transform component
    ctx.db.transform().insert(TransformRow {
        entity_id: eid,
        pos_x,
        pos_y,
        rotation: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
    });

    // Insert velocity component (default zero)
    ctx.db.velocity().insert(VelocityRow {
        entity_id: eid,
        linear_x: 0.0,
        linear_y: 0.0,
        angular: 0.0,
    });

    log::info!("[pod-stdb] Spawned entity {eid} at ({pos_x}, {pos_y})");
}

/// Spawn an entity with full component configuration.
#[spacetimedb::reducer]
pub fn spawn_entity_full(
    ctx: &ReducerContext,
    pos_x: f32,
    pos_y: f32,
    agent_type: Option<AgentType>,
    // Visual
    rect_width: f32,
    rect_height: f32,
    color_r: f32,
    color_g: f32,
    color_b: f32,
    color_a: f32,
    // Gameplay
    label_name: String,
    team_id: u8,
    max_hp: f32,
    // Perception
    vision_range: f32,
    vision_fov: f32,
    hearing_range: f32,
) {
    let ws = ctx
        .db
        .world_state()
        .id()
        .find(0)
        .expect("World not initialized");

    let entity = ctx.db.entity().insert(EntityRow {
        entity_id: 0,
        agent_type,
        owner_identity: None,
        alive: true,
        created_tick: ws.tick,
    });
    let eid = entity.entity_id;

    // Spatial
    ctx.db.transform().insert(TransformRow {
        entity_id: eid,
        pos_x,
        pos_y,
        rotation: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
    });
    ctx.db.velocity().insert(VelocityRow {
        entity_id: eid,
        linear_x: 0.0,
        linear_y: 0.0,
        angular: 0.0,
    });

    // Visual
    ctx.db.color_rect().insert(ColorRectRow {
        entity_id: eid,
        width: rect_width,
        height: rect_height,
        color_r,
        color_g,
        color_b,
        color_a,
        layer: 0,
    });

    // Gameplay
    ctx.db.label().insert(LabelRow {
        entity_id: eid,
        name: label_name.clone(),
        team_id,
    });
    ctx.db.health().insert(HealthRow {
        entity_id: eid,
        current: max_hp,
        max_hp,
        armor: 0.0,
        invulnerable: false,
    });

    // Perception
    ctx.db.perception().insert(PerceptionRow {
        entity_id: eid,
        vision_range,
        vision_fov,
        hearing_range,
    });

    // Movement (defaults)
    ctx.db.movement().insert(MovementRow {
        entity_id: eid,
        max_speed: 200.0,
        acceleration: 800.0,
        deceleration: 600.0,
        turn_rate: std::f32::consts::TAU,
    });

    // Agent constraints (defaults)
    ctx.db.agent_constraints().insert(AgentConstraintsRow {
        entity_id: eid,
        actions_per_tick: REMOTE_AGENT_MAX_ACTIONS_PER_TICK,
        attack_cooldown: 30,
        ability_cooldowns_json: "[60,120,300]".to_string(),
        can_act: true,
        attack_cooldown_remaining: 0,
    });

    log::info!("[pod-stdb] Spawned full entity {eid} '{label_name}' at ({pos_x}, {pos_y})");
}

/// Destroy an entity and all its component rows.
#[spacetimedb::reducer]
pub fn destroy_entity(ctx: &ReducerContext, entity_id: u64) {
    // Mark entity as dead
    if let Some(mut entity) = ctx.db.entity().entity_id().find(entity_id) {
        entity.alive = false;
        ctx.db.entity().entity_id().update(entity);
    }

    // Remove component rows
    ctx.db.transform().entity_id().delete(entity_id);
    ctx.db.velocity().entity_id().delete(entity_id);
    ctx.db.rigid_body().entity_id().delete(entity_id);
    ctx.db.collider().entity_id().delete(entity_id);
    ctx.db.sprite().entity_id().delete(entity_id);
    ctx.db.color_rect().entity_id().delete(entity_id);
    ctx.db.health().entity_id().delete(entity_id);
    ctx.db.label().entity_id().delete(entity_id);
    ctx.db.perception().entity_id().delete(entity_id);
    ctx.db.movement().entity_id().delete(entity_id);
    ctx.db.agent_constraints().entity_id().delete(entity_id);
    ctx.db.script().entity_id().delete(entity_id);

    log::info!("[pod-stdb] Destroyed entity {entity_id}");
}

// ============================================================
// AGENT CONNECTION REDUCERS
// ============================================================

/// Register the calling client as an agent controlling an entity.
#[spacetimedb::reducer]
pub fn connect_agent(
    ctx: &ReducerContext,
    entity_id: u64,
    agent_type: AgentType,
    display_name: String,
) {
    let identity = ctx.sender;

    // Verify entity exists and is alive
    let entity = ctx
        .db
        .entity()
        .entity_id()
        .find(entity_id)
        .expect("Entity not found");
    if !entity.alive {
        reject_reducer(
            ctx,
            format!("connect_agent rejected: entity {entity_id} is dead"),
        );
        return;
    }

    // Register connection
    ctx.db.connected_agent().insert(ConnectedAgentRow {
        identity,
        entity_id,
        agent_type,
        display_name: display_name.clone(),
        connected_at: ctx.timestamp,
    });

    // Update entity owner
    if let Some(mut e) = ctx.db.entity().entity_id().find(entity_id) {
        e.owner_identity = Some(identity);
        ctx.db.entity().entity_id().update(e);
    }

    log::info!("[pod-stdb] Agent '{display_name}' connected to entity {entity_id}");
}

/// Create a new lobby owned by the calling player.
#[spacetimedb::reducer]
pub fn create_lobby(
    ctx: &ReducerContext,
    name: String,
    host_entity_id: u64,
    max_players: u32,
    is_private: bool,
) {
    let identity = ctx.sender;

    let ws = ctx
        .db
        .world_state()
        .id()
        .find(0)
        .expect("World not initialized");
    if max_players == 0 {
        reject_reducer(ctx, "create_lobby rejected: max_players must be at least 1");
        return;
    }

    let lobby = ctx.db.lobby().insert(LobbyRow {
        lobby_id: 0, // auto_inc
        name,
        host_identity: identity,
        host_entity_id,
        max_players,
        is_private,
        created_at: ctx.timestamp,
        started: false,
    });

    ctx.db.lobby_member().insert(LobbyMemberRow {
        membership_id: 0, // auto_inc
        lobby_id: lobby.lobby_id,
        identity,
        entity_id: host_entity_id,
        joined_at_tick: ws.tick,
        is_ready: true,
    });

    log::info!(
        "[pod-stdb] Lobby {} created by {identity:?} as entity {host_entity_id}",
        lobby.lobby_id
    );
}

/// Join a lobby by ID.
#[spacetimedb::reducer]
pub fn join_lobby(ctx: &ReducerContext, lobby_id: u64, entity_id: u64) {
    let identity = ctx.sender;
    let ws = ctx
        .db
        .world_state()
        .id()
        .find(0)
        .expect("World not initialized");

    let lobby = match ctx.db.lobby().lobby_id().find(lobby_id) {
        Some(row) => row,
        None => {
            reject_reducer(
                ctx,
                format!("join_lobby rejected: lobby {lobby_id} does not exist"),
            );
            return;
        }
    };

    if lobby.started {
        reject_reducer(
            ctx,
            format!("join_lobby rejected: lobby {lobby_id} already started"),
        );
        return;
    }

    if !ctx
        .db
        .lobby_member()
        .iter()
        .any(|member| member.identity == identity && member.lobby_id == lobby_id)
    {
        let current_members: Vec<LobbyMemberRow> = ctx
            .db
            .lobby_member()
            .iter()
            .filter(|member| member.lobby_id == lobby_id)
            .collect();

        if current_members.len() as u32 >= lobby.max_players {
            reject_reducer(
                ctx,
                format!("join_lobby rejected: lobby {lobby_id} is full"),
            );
            return;
        }

        ctx.db.lobby_member().insert(LobbyMemberRow {
            membership_id: 0, // auto_inc
            lobby_id,
            identity,
            entity_id,
            joined_at_tick: ws.tick,
            is_ready: false,
        });
    }
}

/// Leave the calling player's current lobby.
#[spacetimedb::reducer]
pub fn leave_lobby(ctx: &ReducerContext) {
    let identity = ctx.sender;
    for membership in ctx
        .db
        .lobby_member()
        .iter()
        .filter(|member| member.identity == identity)
    {
        ctx.db
            .lobby_member()
            .membership_id()
            .delete(membership.membership_id);
    }
}

/// Update readiness for the calling player in a lobby.
#[spacetimedb::reducer]
pub fn set_lobby_ready(ctx: &ReducerContext, lobby_id: u64, is_ready: bool) {
    let identity = ctx.sender;
    let mut updated = false;

    if let Some(mut member) = ctx
        .db
        .lobby_member()
        .iter()
        .find(|member| member.identity == identity && member.lobby_id == lobby_id)
    {
        member.is_ready = is_ready;
        ctx.db.lobby_member().membership_id().update(member);
        updated = true;
    }

    if !updated {
        reject_reducer(
            ctx,
            format!("set_lobby_ready rejected: caller is not a member of lobby {lobby_id}"),
        );
    }
}

/// Start a lobby (host-only).
#[spacetimedb::reducer]
pub fn start_lobby(ctx: &ReducerContext, lobby_id: u64) {
    let identity = ctx.sender;

    let mut lobby = match ctx.db.lobby().lobby_id().find(lobby_id) {
        Some(row) => row,
        None => {
            reject_reducer(
                ctx,
                format!("start_lobby rejected: lobby {lobby_id} does not exist"),
            );
            return;
        }
    };

    if lobby.host_identity != identity {
        reject_reducer(
            ctx,
            format!("start_lobby rejected: caller is not host for lobby {lobby_id}"),
        );
        return;
    }

    lobby.started = true;
    ctx.db.lobby().lobby_id().update(lobby);
}

// ============================================================
// MATCHMAKING REDUCERS
// ============================================================

/// Add an entity to the global matchmaking queue.
#[spacetimedb::reducer]
pub fn join_match_queue(ctx: &ReducerContext, entity_id: u64, desired_party_size: u32) {
    if desired_party_size == 0 {
        reject_reducer(
            ctx,
            "join_match_queue rejected: desired_party_size must be at least 1",
        );
        return;
    }

    let identity = ctx.sender;
    let ws = ctx
        .db
        .world_state()
        .id()
        .find(0)
        .expect("World not initialized");

    // Validate entity exists and is alive.
    let entity = ctx
        .db
        .entity()
        .entity_id()
        .find(entity_id)
        .expect("Entity not found");
    if !entity.alive {
        reject_reducer(
            ctx,
            format!("join_match_queue rejected: entity {entity_id} is not alive"),
        );
        return;
    }

    // Prevent duplicate queue entries for same identity/entity pair.
    if ctx
        .db
        .match_queue()
        .iter()
        .any(|row| row.identity == identity && row.entity_id == entity_id)
    {
        reject_reducer(
            ctx,
            format!(
                "join_match_queue rejected: identity {identity:?} already queued for entity {entity_id}"
            ),
        );
        return;
    }

    ctx.db.match_queue().insert(MatchQueueRow {
        queue_id: 0, // auto_inc
        identity,
        entity_id,
        desired_party_size,
        queued_at_tick: ws.tick,
    });

    log::info!(
        "[pod-stdb] {identity:?} queued entity {entity_id} for party size {desired_party_size}"
    );
}

/// Remove all queue entries for the calling identity.
#[spacetimedb::reducer]
pub fn leave_match_queue(ctx: &ReducerContext) {
    let identity = ctx.sender;

    let mut removed = false;
    let queue_ids: Vec<u64> = ctx
        .db
        .match_queue()
        .iter()
        .filter(|row| row.identity == identity)
        .map(|row| row.queue_id)
        .collect();

    for queue_id in queue_ids {
        ctx.db.match_queue().queue_id().delete(queue_id);
        removed = true;
    }

    if !removed {
        reject_reducer(
            ctx,
            format!("leave_match_queue rejected: no queue entries for {identity:?}"),
        );
        return;
    }

    log::info!("[pod-stdb] {identity:?} left matchmaking queue");
}

/// Consume queued players and create an active match.
///
/// This reducer creates a `game_match` row and `match_participant` rows for the
/// first `desired_party_size` entries that requested that party size.
#[spacetimedb::reducer]
pub fn create_match_from_queue(ctx: &ReducerContext, desired_party_size: u32) {
    if desired_party_size == 0 {
        reject_reducer(
            ctx,
            "create_match_from_queue rejected: desired_party_size must be at least 1",
        );
        return;
    }

    let ws = ctx
        .db
        .world_state()
        .id()
        .find(0)
        .expect("World not initialized");

    let mut candidates: Vec<(u64, Identity, u64)> = Vec::new();
    for row in ctx.db.match_queue().iter() {
        if row.desired_party_size == desired_party_size {
            candidates.push((row.queue_id, row.identity, row.entity_id));
            if candidates.len() as u32 >= desired_party_size {
                break;
            }
        }
    }

    if candidates.len() < desired_party_size as usize {
        reject_reducer(
            ctx,
            format!(
                "create_match_from_queue rejected: insufficient players for party size {desired_party_size} (found {})",
                candidates.len()
            ),
        );
        return;
    }

    let game_match = ctx.db.game_match().insert(GameMatchRow {
        match_id: 0, // auto_inc
        created_tick: ws.tick,
        max_players: desired_party_size,
        state: MatchState::InProgress,
        started_tick: Some(ws.tick),
    });

    for (team_id, (queue_id, identity, entity_id)) in candidates
        .into_iter()
        .take(desired_party_size as usize)
        .enumerate()
    {
        ctx.db.match_participant().insert(MatchParticipantRow {
            participant_id: 0, // auto_inc
            match_id: game_match.match_id,
            identity,
            entity_id,
            team_id: team_id as u8 % 2,
            joined_at_tick: ws.tick,
        });
        ctx.db.match_queue().queue_id().delete(queue_id);
    }

    log::info!(
        "[pod-stdb] Created match {} for {} players (party_size={})",
        game_match.match_id,
        desired_party_size,
        desired_party_size
    );
}

// ============================================================
// ACTION SUBMISSION REDUCER
// ============================================================

/// Submit an action for an entity during the current tick.
/// Actions are validated and executed during execute_tick.
#[spacetimedb::reducer]
pub fn submit_action(
    ctx: &ReducerContext,
    entity_id: u64,
    action_kind: ActionKind,
    // Flattened optional parameters
    direction_x: Option<f32>,
    direction_y: Option<f32>,
    angle: Option<f32>,
    target_x: Option<f32>,
    target_y: Option<f32>,
    target_entity_id: Option<u64>,
    ability_slot: Option<u8>,
    ability_target_kind: Option<AbilityTargetKind>,
    message: Option<String>,
    volume: Option<SpeakVolume>,
    signal_type: Option<String>,
    signal_data: Option<String>,
    prefab: Option<String>,
) {
    let ws = ctx
        .db
        .world_state()
        .id()
        .find(0)
        .expect("World not initialized");

    ctx.db.action_submission().insert(ActionSubmissionRow {
        submission_id: 0, // auto_inc
        entity_id,
        tick: ws.tick,
        action_kind,
        direction_x,
        direction_y,
        angle,
        target_x,
        target_y,
        target_entity_id,
        ability_slot,
        ability_target_kind,
        message,
        volume,
        signal_type,
        signal_data,
        prefab,
    });
}

// ============================================================
// TICK EXECUTION REDUCER
// ============================================================

/// Execute one game tick — the core simulation step.
///
/// Tick pipeline (mirrors pod-core tick.rs):
///   1. Clear previous tick's events
///   2. Build observations per agent
///   3. Process submitted actions (validate + execute)
///   4. Physics/movement (velocity integration)
///   5. Generate events
///   6. Advance tick counter
#[spacetimedb::reducer]
pub fn execute_tick(ctx: &ReducerContext) {
    let mut ws = ctx
        .db
        .world_state()
        .id()
        .find(0)
        .expect("World not initialized");

    if ws.paused {
        return;
    }

    let current_tick = ws.tick;
    let dt = 1.0 / ws.ticks_per_second as f32;
    let elapsed = current_tick as f32 * dt;

    // ── Phase 1: Clear previous tick's events ──
    crate::observation::clear_old_events(ctx, current_tick);
    prune_debug_telemetry_rows(ctx, current_tick);

    // ── Phase 2: Build observations ──
    crate::observation::build_observations(ctx, current_tick);
    let observation_counts = collect_observation_counts(ctx, current_tick);
    let mut telemetry_frames =
        seed_agent_telemetry_frames(ctx, current_tick, elapsed, &observation_counts);

    // ── Phase 3: Process submitted actions ──
    let constraints_rows: Vec<AgentConstraintsRow> = ctx.db.agent_constraints().iter().collect();
    for mut constraints in constraints_rows {
        if constraints.attack_cooldown_remaining > 0 {
            constraints.attack_cooldown_remaining -= 1;
            ctx.db.agent_constraints().entity_id().update(constraints);
        }
    }

    let mut submissions: Vec<ActionSubmissionRow> = ctx
        .db
        .action_submission()
        .iter()
        .filter(|submission| submission.tick == current_tick)
        .collect();
    submissions.sort_by_key(|submission| submission.submission_id);

    let mut action_counts: HashMap<u64, u8> = HashMap::new();

    for submission in submissions {
        let eid = submission.entity_id;
        let telemetry_action = submission_to_core_action(&submission);
        record_submitted_action(&mut telemetry_frames, eid, telemetry_action.clone());

        let Some(entity) = ctx.db.entity().entity_id().find(eid) else {
            record_rejected_action(
                &mut telemetry_frames,
                eid,
                telemetry_action,
                "Submission source entity missing",
            );
            continue;
        };
        if !entity.alive {
            record_rejected_action(
                &mut telemetry_frames,
                eid,
                telemetry_action,
                "Submission source entity is not alive",
            );
            continue;
        }

        let mut constraints = ctx.db.agent_constraints().entity_id().find(eid);
        if let Some(ref c) = constraints {
            if !c.can_act {
                record_rejected_action(
                    &mut telemetry_frames,
                    eid,
                    telemetry_action,
                    "Agent cannot act this tick",
                );
                continue;
            }

            let actions_this_tick = action_counts.entry(eid).or_insert(0);
            if *actions_this_tick >= c.actions_per_tick {
                record_rejected_action(
                    &mut telemetry_frames,
                    eid,
                    telemetry_action,
                    "Agent exceeded per-tick action budget",
                );
                continue;
            }
            *actions_this_tick += 1;
        }

        let mut executed = false;
        let mut rejection_reason: Option<&'static str> = None;

        match submission.action_kind {
            ActionKind::Move => {
                if let (Some(dx), Some(dy)) = (submission.direction_x, submission.direction_y) {
                    if let Some(mvmt) = ctx.db.movement().entity_id().find(eid) {
                        if let Some(mut vel) = ctx.db.velocity().entity_id().find(eid) {
                            let len = (dx * dx + dy * dy).sqrt();
                            if len > 0.001 {
                                let nx = dx / len;
                                let ny = dy / len;
                                vel.linear_x = nx * mvmt.max_speed;
                                vel.linear_y = ny * mvmt.max_speed;
                                ctx.db.velocity().entity_id().update(vel);
                                executed = true;
                            } else {
                                rejection_reason = Some("Move direction magnitude too small");
                            }
                        } else {
                            rejection_reason = Some("Move source velocity missing");
                        }
                    } else {
                        rejection_reason = Some("Move source movement settings missing");
                    }
                } else {
                    rejection_reason = Some("Move action missing direction");
                }
            }
            ActionKind::Stop => {
                if let Some(mut vel) = ctx.db.velocity().entity_id().find(eid) {
                    vel.linear_x = 0.0;
                    vel.linear_y = 0.0;
                    vel.angular = 0.0;
                    ctx.db.velocity().entity_id().update(vel);
                    executed = true;
                } else {
                    rejection_reason = Some("Stop source velocity missing");
                }
            }
            ActionKind::Rotate => {
                if let Some(angle) = submission.angle {
                    if let Some(mut tf) = ctx.db.transform().entity_id().find(eid) {
                        tf.rotation = angle;
                        ctx.db.transform().entity_id().update(tf);
                        executed = true;
                    } else {
                        rejection_reason = Some("Rotate source transform missing");
                    }
                } else {
                    rejection_reason = Some("Rotate action missing angle");
                }
            }
            ActionKind::LookAt => {
                if let (Some(tx), Some(ty)) = (submission.target_x, submission.target_y) {
                    if let Some(mut tf) = ctx.db.transform().entity_id().find(eid) {
                        let dx = tx - tf.pos_x;
                        let dy = ty - tf.pos_y;
                        if (dx * dx + dy * dy) > 0.0001 {
                            tf.rotation = dy.atan2(dx);
                            ctx.db.transform().entity_id().update(tf);
                            executed = true;
                        } else {
                            rejection_reason = Some("LookAt target coincides with source");
                        }
                    } else {
                        rejection_reason = Some("LookAt source transform missing");
                    }
                } else {
                    rejection_reason = Some("LookAt action missing target");
                }
            }
            ActionKind::Attack => {
                let cooling_down = constraints
                    .as_ref()
                    .map(|c| c.attack_cooldown_remaining > 0)
                    .unwrap_or(false);
                if cooling_down {
                    rejection_reason = Some("Attack is on cooldown");
                } else if let Some(target_id) = find_attack_target(ctx, eid) {
                    if apply_attack(ctx, current_tick, eid, target_id) {
                        if let Some(mut c) = constraints.take() {
                            c.attack_cooldown_remaining = c.attack_cooldown;
                            ctx.db.agent_constraints().entity_id().update(c);
                        }
                        executed = true;
                    } else {
                        rejection_reason = Some("Attack target invalid or out of range");
                    }
                } else {
                    rejection_reason = Some("No hostile target in range");
                }
            }
            ActionKind::AttackTarget => {
                let cooling_down = constraints
                    .as_ref()
                    .map(|c| c.attack_cooldown_remaining > 0)
                    .unwrap_or(false);
                if cooling_down {
                    rejection_reason = Some("Attack is on cooldown");
                } else if let Some(target_id) = submission.target_entity_id {
                    if target_id == eid {
                        rejection_reason = Some("Cannot attack self");
                    } else if apply_attack(ctx, current_tick, eid, target_id) {
                        if let Some(mut c) = constraints.take() {
                            c.attack_cooldown_remaining = c.attack_cooldown;
                            ctx.db.agent_constraints().entity_id().update(c);
                        }
                        executed = true;
                    } else {
                        rejection_reason = Some("AttackTarget target invalid or out of range");
                    }
                } else {
                    rejection_reason = Some("AttackTarget missing target entity");
                }
            }
            ActionKind::CaptureCreature => {
                if let Some(target_id) = submission.target_entity_id {
                    let tool_slot = submission
                        .ability_slot
                        .map(|slot| slot.to_string())
                        .unwrap_or_else(|| "null".to_string());
                    ctx.db.world_event().insert(WorldEventRow {
                        event_id: 0,
                        tick: current_tick,
                        event_kind: WorldEventKind::AbilityUsed,
                        entity_id: eid,
                        secondary_entity_id: Some(target_id),
                        data_json: format!(
                            r#"{{"type":"capture_creature","tool_slot":{tool_slot}}}"#
                        ),
                    });
                    executed = true;
                } else {
                    rejection_reason = Some("CaptureCreature missing target entity");
                }
            }
            ActionKind::SummonCompanion => {
                let slot = submission.ability_slot.unwrap_or(0);
                ctx.db.world_event().insert(WorldEventRow {
                    event_id: 0,
                    tick: current_tick,
                    event_kind: WorldEventKind::AbilityUsed,
                    entity_id: eid,
                    secondary_entity_id: None,
                    data_json: format!(r#"{{"type":"summon_companion","slot":{slot}}}"#),
                });
                executed = true;
            }
            ActionKind::CommandCompanion => {
                let slot = submission.ability_slot.unwrap_or(0);
                let command = submission
                    .signal_type
                    .clone()
                    .unwrap_or_else(|| "follow".to_string())
                    .replace('"', "\\\"");
                let target_id = submission
                    .target_entity_id
                    .map(|target| target.to_string())
                    .unwrap_or_else(|| "null".to_string());
                ctx.db.world_event().insert(WorldEventRow {
                    event_id: 0,
                    tick: current_tick,
                    event_kind: WorldEventKind::AbilityUsed,
                    entity_id: eid,
                    secondary_entity_id: submission.target_entity_id,
                    data_json: format!(
                        r#"{{"type":"command_companion","slot":{slot},"command":"{command}","target":{target_id}}}"#
                    ),
                });
                executed = true;
            }
            ActionKind::GatherResource => {
                if let Some(target_id) = submission.target_entity_id {
                    let skill = submission
                        .signal_type
                        .clone()
                        .unwrap_or_else(|| "gather".to_string())
                        .replace('"', "\\\"");
                    ctx.db.world_event().insert(WorldEventRow {
                        event_id: 0,
                        tick: current_tick,
                        event_kind: WorldEventKind::AbilityUsed,
                        entity_id: eid,
                        secondary_entity_id: Some(target_id),
                        data_json: format!(r#"{{"type":"gather_resource","skill":"{skill}"}}"#),
                    });
                    executed = true;
                } else {
                    rejection_reason = Some("GatherResource missing target entity");
                }
            }
            ActionKind::Loot => {
                if let Some(target_id) = submission.target_entity_id {
                    ctx.db.world_event().insert(WorldEventRow {
                        event_id: 0,
                        tick: current_tick,
                        event_kind: WorldEventKind::ItemPickedUp,
                        entity_id: eid,
                        secondary_entity_id: Some(target_id),
                        data_json: r#"{"type":"loot"}"#.to_string(),
                    });
                    executed = true;
                } else {
                    rejection_reason = Some("Loot missing target entity");
                }
            }
            ActionKind::SetAutoRetaliate => {
                let enabled = submission.signal_data.as_deref() == Some("true");
                ctx.db.world_event().insert(WorldEventRow {
                    event_id: 0,
                    tick: current_tick,
                    event_kind: WorldEventKind::AbilityUsed,
                    entity_id: eid,
                    secondary_entity_id: None,
                    data_json: format!(r#"{{"type":"set_auto_retaliate","enabled":{enabled}}}"#),
                });
                executed = true;
            }
            ActionKind::Interact => {
                if let Some(target_id) = find_interaction_target(ctx, eid) {
                    ctx.db.world_event().insert(WorldEventRow {
                        event_id: 0,
                        tick: current_tick,
                        event_kind: WorldEventKind::InteractionTriggered,
                        entity_id: eid,
                        secondary_entity_id: Some(target_id),
                        data_json: "{}".to_string(),
                    });
                    executed = true;
                } else {
                    rejection_reason = Some("No interaction target in range");
                }
            }
            ActionKind::InteractWith => {
                if let Some(target_id) = submission.target_entity_id {
                    if in_range(ctx, eid, target_id, INTERACT_RANGE) {
                        ctx.db.world_event().insert(WorldEventRow {
                            event_id: 0,
                            tick: current_tick,
                            event_kind: WorldEventKind::InteractionTriggered,
                            entity_id: eid,
                            secondary_entity_id: Some(target_id),
                            data_json: "{}".to_string(),
                        });
                        executed = true;
                    } else {
                        rejection_reason = Some("InteractWith target out of range");
                    }
                } else {
                    rejection_reason = Some("InteractWith missing target entity");
                }
            }
            ActionKind::Pickup => {
                if let Some(target_id) = submission.target_entity_id {
                    ctx.db.world_event().insert(WorldEventRow {
                        event_id: 0,
                        tick: current_tick,
                        event_kind: WorldEventKind::ItemPickedUp,
                        entity_id: eid,
                        secondary_entity_id: Some(target_id),
                        data_json: "{}".to_string(),
                    });
                    executed = true;
                } else {
                    rejection_reason = Some("Pickup missing target entity");
                }
            }
            ActionKind::Drop => {
                let slot = submission.ability_slot.unwrap_or(0);
                ctx.db.world_event().insert(WorldEventRow {
                    event_id: 0,
                    tick: current_tick,
                    event_kind: WorldEventKind::ItemDropped,
                    entity_id: eid,
                    secondary_entity_id: None,
                    data_json: format!(r#"{{"slot":{slot}}}"#),
                });
                executed = true;
            }
            ActionKind::UseItem => {
                let slot = submission.ability_slot.unwrap_or(0);
                ctx.db.world_event().insert(WorldEventRow {
                    event_id: 0,
                    tick: current_tick,
                    event_kind: WorldEventKind::AbilityUsed,
                    entity_id: eid,
                    secondary_entity_id: None,
                    data_json: format!(r#"{{"item_slot":{slot}}}"#),
                });
                executed = true;
            }
            ActionKind::UseAbility => {
                let slot = submission.ability_slot.unwrap_or(0);
                ctx.db.world_event().insert(WorldEventRow {
                    event_id: 0,
                    tick: current_tick,
                    event_kind: WorldEventKind::AbilityUsed,
                    entity_id: eid,
                    secondary_entity_id: submission.target_entity_id,
                    data_json: format!(r#"{{"slot":{slot}}}"#),
                });
                executed = true;
            }
            ActionKind::Speak => {
                if let (Some(msg), Some(vol)) =
                    (submission.message.clone(), submission.volume.clone())
                {
                    if let Some(tf) = ctx.db.transform().entity_id().find(eid) {
                        ctx.db.speech_event().insert(SpeechEventRow {
                            event_id: 0,
                            tick: current_tick,
                            speaker_entity_id: eid,
                            message: msg,
                            volume: vol,
                            pos_x: tf.pos_x,
                            pos_y: tf.pos_y,
                        });
                        executed = true;
                    } else {
                        rejection_reason = Some("Speak source transform missing");
                    }
                } else {
                    rejection_reason = Some("Speak action missing message or volume");
                }
            }
            ActionKind::Signal => {
                let signal_type = submission
                    .signal_type
                    .clone()
                    .unwrap_or_else(|| "signal".to_string());
                let signal_data = submission.signal_data.clone().unwrap_or_default();
                let escaped_type = signal_type.replace('"', "\\\"");
                let escaped_data = signal_data.replace('"', "\\\"");
                ctx.db.world_event().insert(WorldEventRow {
                    event_id: 0,
                    tick: current_tick,
                    event_kind: WorldEventKind::AbilityUsed,
                    entity_id: eid,
                    secondary_entity_id: None,
                    data_json: format!(
                        r#"{{"signal_type":"{}","signal_data":"{}"}}"#,
                        escaped_type, escaped_data
                    ),
                });
                executed = true;
            }
            ActionKind::Spawn => {
                if let (Some(prefab), Some(x), Some(y)) = (
                    submission.prefab.clone(),
                    submission.target_x,
                    submission.target_y,
                ) {
                    let created = ctx.db.entity().insert(EntityRow {
                        entity_id: 0,
                        agent_type: None,
                        owner_identity: None,
                        alive: true,
                        created_tick: current_tick,
                    });
                    ctx.db.transform().insert(TransformRow {
                        entity_id: created.entity_id,
                        pos_x: x,
                        pos_y: y,
                        rotation: 0.0,
                        scale_x: 1.0,
                        scale_y: 1.0,
                    });
                    ctx.db.velocity().insert(VelocityRow {
                        entity_id: created.entity_id,
                        linear_x: 0.0,
                        linear_y: 0.0,
                        angular: 0.0,
                    });
                    ctx.db.world_event().insert(WorldEventRow {
                        event_id: 0,
                        tick: current_tick,
                        event_kind: WorldEventKind::EntitySpawned,
                        entity_id: created.entity_id,
                        secondary_entity_id: None,
                        data_json: prefab,
                    });
                    executed = true;
                } else {
                    rejection_reason = Some("Spawn action missing prefab or position");
                }
            }
            ActionKind::Idle => {
                executed = true;
            }
        }

        if executed {
            record_executed_action(&mut telemetry_frames, eid, telemetry_action);
        } else if let Some(reason) = rejection_reason {
            record_rejected_action(&mut telemetry_frames, eid, telemetry_action, reason);
        }
    }

    // ── Phase 4: Physics — velocity integration ──
    for vel in ctx.db.velocity().iter() {
        if vel.linear_x.abs() > 0.001 || vel.linear_y.abs() > 0.001 || vel.angular.abs() > 0.001 {
            if let Some(mut tf) = ctx.db.transform().entity_id().find(vel.entity_id) {
                tf.pos_x += vel.linear_x * dt;
                tf.pos_y += vel.linear_y * dt;
                tf.rotation += vel.angular * dt;

                // Clamp to world bounds
                tf.pos_x = tf.pos_x.clamp(0.0, ws.world_width);
                tf.pos_y = tf.pos_y.clamp(0.0, ws.world_height);

                ctx.db.transform().entity_id().update(tf);
            }
        }
    }

    update_telemetry_trajectory_ends(ctx, current_tick, elapsed + dt, &mut telemetry_frames);
    emit_agent_telemetry_rows(ctx, current_tick, &telemetry_frames);
    emit_agent_tool_call_events(ctx, &telemetry_frames);
    emit_agent_tick_rollups(ctx, current_tick);

    // ── Phase 5: Advance tick ──
    ws.tick = current_tick + 1;
    ctx.db.world_state().id().update(ws);

    log::debug!("[pod-stdb] Tick {current_tick} → {}", current_tick + 1);
}

fn prune_debug_telemetry_rows(ctx: &ReducerContext, current_tick: u64) {
    let telemetry_floor = current_tick.saturating_sub(TELEMETRY_RETENTION_TICKS.saturating_sub(1));
    let old_telemetry_rows: Vec<u64> = ctx
        .db
        .agent_telemetry_tick()
        .iter()
        .filter(|row| row.tick < telemetry_floor)
        .map(|row| row.row_id)
        .collect();
    for row_id in old_telemetry_rows {
        ctx.db.agent_telemetry_tick().row_id().delete(row_id);
    }

    let tool_event_floor =
        current_tick.saturating_sub(TOOL_EVENT_RETENTION_TICKS.saturating_sub(1));
    let old_tool_rows: Vec<u64> = ctx
        .db
        .agent_tool_call_event()
        .iter()
        .filter(|row| row.tick < tool_event_floor)
        .map(|row| row.row_id)
        .collect();
    for row_id in old_tool_rows {
        ctx.db.agent_tool_call_event().row_id().delete(row_id);
    }

    let rollup_floor = current_tick.saturating_sub(ROLLUP_RETENTION_TICKS.saturating_sub(1));
    let old_rollup_rows: Vec<u64> = ctx
        .db
        .agent_tick_rollup()
        .iter()
        .filter(|row| row.tick_end < rollup_floor)
        .map(|row| row.row_id)
        .collect();
    for row_id in old_rollup_rows {
        ctx.db.agent_tick_rollup().row_id().delete(row_id);
    }
}

fn collect_observation_counts(
    ctx: &ReducerContext,
    current_tick: u64,
) -> HashMap<u64, ObservationCounts> {
    ctx.db
        .observation_event()
        .iter()
        .filter(|row| row.tick == current_tick)
        .map(|row| {
            (
                row.observer_entity_id,
                decode_observation_counts(&row.observation_json),
            )
        })
        .collect()
}

fn decode_observation_counts(document: &str) -> ObservationCounts {
    let Ok(value) = serde_json::from_str::<Value>(document) else {
        return ObservationCounts::default();
    };

    ObservationCounts {
        visible_entities: value["visible_entities"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
        audible_events: value["audible_events"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
        messages: value["messages"]
            .as_array()
            .map(Vec::len)
            .unwrap_or_default(),
    }
}

fn seed_agent_telemetry_frames(
    ctx: &ReducerContext,
    current_tick: u64,
    elapsed: f32,
    observation_counts: &HashMap<u64, ObservationCounts>,
) -> HashMap<u64, AgentTelemetryFrame> {
    let mut frames = HashMap::new();

    for entity in ctx.db.entity().iter().filter(|entity| entity.alive) {
        let Some(agent_type) = entity.agent_type.clone() else {
            continue;
        };
        let Some(transform) = ctx.db.transform().entity_id().find(entity.entity_id) else {
            continue;
        };
        let velocity = ctx.db.velocity().entity_id().find(entity.entity_id);
        let counts = observation_counts
            .get(&entity.entity_id)
            .copied()
            .unwrap_or_default();

        let trajectory_start = TrajectorySample::new(
            current_tick,
            elapsed,
            Vec2::new(transform.pos_x, transform.pos_y),
            velocity
                .as_ref()
                .map(|value| Vec2::new(value.linear_x, value.linear_y))
                .unwrap_or(Vec2::ZERO),
            transform.rotation,
        );

        frames.insert(
            entity.entity_id,
            AgentTelemetryFrame::new(
                current_tick,
                stable_agent_id(entity.entity_id),
                Some(CoreEntityId(entity.entity_id)),
                AgentRuntimeProfile::for_agent_type(core_agent_type(&agent_type)),
                counts.visible_entities,
                counts.audible_events,
                counts.messages,
                0,
                0,
                None,
                Some(trajectory_start),
            ),
        );
    }

    frames
}

fn record_submitted_action(
    telemetry_frames: &mut HashMap<u64, AgentTelemetryFrame>,
    entity_id: u64,
    action: CoreAction,
) {
    if let Some(frame) = telemetry_frames.get_mut(&entity_id) {
        frame.record_action(
            ActionSource::ExternalSubmission,
            ActionLifecycleStage::Submitted,
            action,
            None,
        );
    }
}

fn record_executed_action(
    telemetry_frames: &mut HashMap<u64, AgentTelemetryFrame>,
    entity_id: u64,
    action: CoreAction,
) {
    if let Some(frame) = telemetry_frames.get_mut(&entity_id) {
        frame.record_action(
            ActionSource::ExternalSubmission,
            ActionLifecycleStage::Executed,
            action,
            None,
        );
    }
}

fn record_rejected_action(
    telemetry_frames: &mut HashMap<u64, AgentTelemetryFrame>,
    entity_id: u64,
    action: CoreAction,
    reason: impl Into<String>,
) {
    if let Some(frame) = telemetry_frames.get_mut(&entity_id) {
        frame.record_action(
            ActionSource::ExternalSubmission,
            ActionLifecycleStage::Rejected,
            action,
            Some(reason.into()),
        );
    }
}

fn update_telemetry_trajectory_ends(
    ctx: &ReducerContext,
    current_tick: u64,
    elapsed: f32,
    telemetry_frames: &mut HashMap<u64, AgentTelemetryFrame>,
) {
    for (entity_id, frame) in telemetry_frames.iter_mut() {
        let Some(transform) = ctx.db.transform().entity_id().find(*entity_id) else {
            continue;
        };
        let velocity = ctx.db.velocity().entity_id().find(*entity_id);
        frame.update_trajectory_end(TrajectorySample::new(
            current_tick,
            elapsed,
            Vec2::new(transform.pos_x, transform.pos_y),
            velocity
                .as_ref()
                .map(|value| Vec2::new(value.linear_x, value.linear_y))
                .unwrap_or(Vec2::ZERO),
            transform.rotation,
        ));
    }
}

fn emit_agent_telemetry_rows(
    ctx: &ReducerContext,
    current_tick: u64,
    telemetry_frames: &HashMap<u64, AgentTelemetryFrame>,
) {
    let mut entity_ids: Vec<u64> = telemetry_frames.keys().copied().collect();
    entity_ids.sort_unstable();

    for entity_id in entity_ids {
        let Some(frame) = telemetry_frames.get(&entity_id) else {
            continue;
        };
        let document = VersionedTickTelemetry::new(TickTelemetryFrame {
            tick: current_tick,
            agents: vec![frame.clone()],
        })
        .to_toon_document();
        let trajectory = frame.trajectory.as_ref();
        ctx.db.agent_telemetry_tick().insert(AgentTelemetryTickRow {
            row_id: 0,
            tick: current_tick,
            agent_entity_id: entity_id,
            visible_entity_count: frame.visible_entity_count as u32,
            audible_event_count: frame.audible_event_count as u32,
            message_count: frame.message_count as u32,
            submitted_action_count: action_count(frame, ActionLifecycleStage::Submitted),
            executed_action_count: action_count(frame, ActionLifecycleStage::Executed),
            rejected_action_count: action_count(frame, ActionLifecycleStage::Rejected),
            trajectory_start_x: trajectory
                .as_ref()
                .map(|value| value.start.position.x)
                .unwrap_or_default(),
            trajectory_start_y: trajectory
                .as_ref()
                .map(|value| value.start.position.y)
                .unwrap_or_default(),
            trajectory_end_x: trajectory
                .as_ref()
                .map(|value| value.end.position.x)
                .unwrap_or_default(),
            trajectory_end_y: trajectory
                .as_ref()
                .map(|value| value.end.position.y)
                .unwrap_or_default(),
            frame_json: document,
        });
    }
}

fn emit_agent_tool_call_events(
    ctx: &ReducerContext,
    telemetry_frames: &HashMap<u64, AgentTelemetryFrame>,
) {
    for event in collect_agent_tool_call_events(telemetry_frames) {
        ctx.db
            .agent_tool_call_event()
            .insert(AgentToolCallEventRow {
                row_id: 0,
                tick: event.tick,
                agent_entity_id: event.agent_entity_id,
                tool_name: event.trace.tool_name.clone(),
                provider: event.trace.provider.clone(),
                status: format!("{:?}", event.trace.status),
                latency_ms: event.trace.latency_ms,
                request_units: event.trace.request_units,
                response_units: event.trace.response_units,
                data_json: event.to_toon_document(),
            });
    }
}

fn collect_agent_tool_call_events(
    telemetry_frames: &HashMap<u64, AgentTelemetryFrame>,
) -> Vec<AgentToolCallEvent> {
    let mut entity_ids: Vec<u64> = telemetry_frames.keys().copied().collect();
    entity_ids.sort_unstable();

    let mut events = Vec::new();
    for entity_id in entity_ids {
        let Some(frame) = telemetry_frames.get(&entity_id) else {
            continue;
        };

        for trace in &frame.tool_calls {
            events.push(AgentToolCallEvent::new(entity_id, trace.clone()));
        }
    }

    events
}

fn emit_agent_tick_rollups(ctx: &ReducerContext, current_tick: u64) {
    if (current_tick + 1) % ROLLUP_WINDOW_TICKS != 0 {
        return;
    }

    let window_start = current_tick.saturating_sub(ROLLUP_WINDOW_TICKS - 1);
    let mut frames_by_agent: HashMap<u64, Vec<AgentTelemetryFrame>> = HashMap::new();

    for row in ctx
        .db
        .agent_telemetry_tick()
        .iter()
        .filter(|row| row.tick >= window_start && row.tick <= current_tick)
    {
        let Ok(versioned) = decode_toon_document::<VersionedTickTelemetry>(
            &row.frame_json,
            "versioned_tick_telemetry",
        ) else {
            continue;
        };
        for frame in versioned.payload.agents {
            let Some(entity_id) = frame.entity_id else {
                continue;
            };
            frames_by_agent.entry(entity_id.0).or_default().push(frame);
        }
    }

    let mut agent_ids: Vec<u64> = frames_by_agent.keys().copied().collect();
    agent_ids.sort_unstable();

    for agent_entity_id in agent_ids {
        let Some(frames) = frames_by_agent.get_mut(&agent_entity_id) else {
            continue;
        };
        frames.sort_by_key(|frame| frame.tick);
        let Some(rollup) = AgentTickRollup::from_agent_frames(agent_entity_id, frames) else {
            continue;
        };
        ctx.db.agent_tick_rollup().insert(AgentTickRollupRow {
            row_id: 0,
            tick_start: rollup.tick_start,
            tick_end: rollup.tick_end,
            agent_entity_id,
            total_distance: rollup.total_distance,
            submitted_action_count: rollup.submitted_action_count,
            executed_action_count: rollup.executed_action_count,
            rejected_action_count: rollup.rejected_action_count,
            tool_call_count: rollup.tool_call_count,
            tool_error_count: rollup.tool_error_count,
            rollup_json: rollup.to_toon_document(),
        });
    }
}

fn action_count(frame: &AgentTelemetryFrame, stage: ActionLifecycleStage) -> u32 {
    frame
        .action_trace
        .iter()
        .filter(|trace| trace.stage == stage)
        .count() as u32
}

fn stable_agent_id(entity_id: u64) -> CoreAgentId {
    CoreAgentId(Uuid::from_u128(
        0x504f4453544442000000000000000000u128 | entity_id as u128,
    ))
}

fn core_agent_type(agent_type: &AgentType) -> CoreAgentType {
    match agent_type {
        AgentType::Human => CoreAgentType::Human,
        AgentType::LlmAgent => CoreAgentType::LlmAgent,
        AgentType::NeuralAgent => CoreAgentType::NeuralAgent,
        AgentType::ScriptedNpc => CoreAgentType::ScriptedNpc,
        AgentType::System => CoreAgentType::System,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::telemetry::{AgentToolCallTrace, ToolCallStatus};
    use pod_core::{decode_toon_document, AgentType, RuntimeContractVersion};

    #[test]
    fn collect_agent_tool_call_events_exports_toon_ready_rows() {
        let mut frame = AgentTelemetryFrame::new(
            12,
            stable_agent_id(44),
            Some(CoreEntityId(44)),
            AgentRuntimeProfile::for_agent_type(CoreAgentType::LlmAgent),
            3,
            1,
            0,
            2,
            1,
            None,
            None,
        );
        frame.record_tool_call(AgentToolCallTrace::new(
            12,
            "llm.complete",
            "qwen",
            ToolCallStatus::TimedOut,
            48,
            128,
            0,
            Some("timeout".into()),
        ));
        frame.record_tool_call(AgentToolCallTrace::success(
            12,
            "tool.lookup",
            "catalog",
            8,
            16,
            12,
        ));

        let mut telemetry_frames = HashMap::new();
        telemetry_frames.insert(44, frame);

        let events = collect_agent_tool_call_events(&telemetry_frames);
        assert_eq!(events.len(), 2);
        assert!(events
            .iter()
            .all(|event| event.agent_entity_id == 44 && event.tick == 12));

        let exported: AgentToolCallEvent =
            decode_toon_document(&events[0].to_toon_document(), "agent_tool_call_event")
                .expect("tool-call event should decode");
        assert_eq!(exported.agent_entity_id, 44);
        assert_eq!(exported.trace.tool_name, "llm.complete");
        assert_eq!(exported.trace.provider, "qwen");
        assert_eq!(exported.trace.status, ToolCallStatus::TimedOut);
    }

    #[test]
    fn summarize_remote_topology_document_extracts_publish_metadata() {
        let document = RemoteTopologyBundle {
            version: RuntimeContractVersion::V1,
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
            worlds: vec![pod_core::WorldRealityDefinition::new(
                "deadman-shadow",
                "Deadman Shadow",
                "shadow",
            )],
            links: vec![],
            world_quest_bindings: vec![],
            quest_graphs: vec![],
            applied_world_states: vec![],
            evaluation: pod_core::ScenarioEvaluationSummary {
                controller_mix: vec![],
                worlds: vec![],
            },
        }
        .to_toon_document();

        let summary =
            summarize_remote_topology_document(&document).expect("topology document should decode");
        assert_eq!(summary.generated_at_unix_ms, 42);
        assert_eq!(summary.scenario_id, "deadman-neural-cup");
        assert_eq!(summary.profile_id, "ci-smoke");
        assert_eq!(summary.world_count, 1);
        assert_eq!(summary.team_count, 2);
    }

    #[test]
    fn summarize_remote_topology_document_rejects_wrong_document_type() {
        let document = VersionedTickTelemetry::new(TickTelemetryFrame::empty(7)).to_toon_document();

        let error =
            summarize_remote_topology_document(&document).expect_err("wrong document should fail");
        assert!(error.contains("Expected TOON document type `remote_topology_bundle`"));
    }
}

fn submission_to_core_action(submission: &ActionSubmissionRow) -> CoreAction {
    match submission.action_kind {
        ActionKind::Move => CoreAction::Move {
            direction: Vec2::new(
                submission.direction_x.unwrap_or_default(),
                submission.direction_y.unwrap_or_default(),
            ),
        },
        ActionKind::Stop => CoreAction::Stop,
        ActionKind::Rotate => CoreAction::Rotate {
            angle: submission.angle.unwrap_or_default(),
        },
        ActionKind::LookAt => CoreAction::LookAt {
            target: Vec2::new(
                submission.target_x.unwrap_or_default(),
                submission.target_y.unwrap_or_default(),
            ),
        },
        ActionKind::Attack => CoreAction::Attack,
        ActionKind::AttackTarget => CoreAction::AttackTarget {
            target: CoreEntityId(submission.target_entity_id.unwrap_or_default()),
        },
        ActionKind::UseAbility => CoreAction::UseAbility {
            slot: submission.ability_slot.unwrap_or(0),
            target: core_ability_target(submission),
        },
        ActionKind::CaptureCreature => CoreAction::CaptureCreature {
            target: CoreEntityId(submission.target_entity_id.unwrap_or_default()),
            tool_slot: submission.ability_slot,
        },
        ActionKind::SummonCompanion => CoreAction::SummonCompanion {
            slot: submission.ability_slot.unwrap_or(0),
        },
        ActionKind::CommandCompanion => CoreAction::CommandCompanion {
            slot: submission.ability_slot.unwrap_or(0),
            command: parse_companion_command(submission.signal_type.as_deref()),
            target: submission.target_entity_id.map(CoreEntityId),
        },
        ActionKind::Interact => CoreAction::Interact,
        ActionKind::InteractWith => CoreAction::InteractWith {
            target: CoreEntityId(submission.target_entity_id.unwrap_or_default()),
        },
        ActionKind::Pickup => CoreAction::Pickup {
            target: CoreEntityId(submission.target_entity_id.unwrap_or_default()),
        },
        ActionKind::Drop => CoreAction::Drop {
            slot: submission.ability_slot.unwrap_or(0),
        },
        ActionKind::UseItem => CoreAction::UseItem {
            slot: submission.ability_slot.unwrap_or(0),
        },
        ActionKind::GatherResource => CoreAction::GatherResource {
            target: CoreEntityId(submission.target_entity_id.unwrap_or_default()),
            skill: parse_skill_kind(submission.signal_type.as_deref()),
        },
        ActionKind::Loot => CoreAction::Loot {
            target: CoreEntityId(submission.target_entity_id.unwrap_or_default()),
        },
        ActionKind::Speak => CoreAction::Speak {
            message: submission.message.clone().unwrap_or_default(),
            volume: match submission.volume.as_ref().unwrap_or(&SpeakVolume::Normal) {
                SpeakVolume::Whisper => CoreSpeakVolume::Whisper,
                SpeakVolume::Normal => CoreSpeakVolume::Normal,
                SpeakVolume::Shout => CoreSpeakVolume::Shout,
            },
        },
        ActionKind::Signal => CoreAction::Signal {
            signal_type: submission.signal_type.clone().unwrap_or_default(),
            data: submission.signal_data.clone().unwrap_or_default(),
        },
        ActionKind::SetAutoRetaliate => CoreAction::SetAutoRetaliate {
            enabled: submission.signal_data.as_deref() == Some("true"),
        },
        ActionKind::Idle => CoreAction::Idle,
        ActionKind::Spawn => CoreAction::Spawn {
            prefab: submission.prefab.clone().unwrap_or_default(),
            position: Vec2::new(
                submission.target_x.unwrap_or_default(),
                submission.target_y.unwrap_or_default(),
            ),
        },
    }
}

fn core_ability_target(submission: &ActionSubmissionRow) -> Option<CoreAbilityTarget> {
    match submission.ability_target_kind {
        Some(AbilityTargetKind::Position) => Some(CoreAbilityTarget::Position(Vec2::new(
            submission.target_x.unwrap_or_default(),
            submission.target_y.unwrap_or_default(),
        ))),
        Some(AbilityTargetKind::Entity) => Some(CoreAbilityTarget::Entity(CoreEntityId(
            submission.target_entity_id.unwrap_or_default(),
        ))),
        Some(AbilityTargetKind::Direction) => Some(CoreAbilityTarget::Direction(Vec2::new(
            submission.direction_x.unwrap_or_default(),
            submission.direction_y.unwrap_or_default(),
        ))),
        Some(AbilityTargetKind::None) | None => None,
    }
}

fn parse_companion_command(command: Option<&str>) -> CoreCompanionCommand {
    match command.unwrap_or("follow").to_ascii_lowercase().as_str() {
        "attack" => CoreCompanionCommand::Attack,
        "guard" => CoreCompanionCommand::Guard,
        "recall" => CoreCompanionCommand::Recall,
        _ => CoreCompanionCommand::Follow,
    }
}

fn parse_skill_kind(skill: Option<&str>) -> SkillKind {
    match skill.unwrap_or("gather").to_ascii_lowercase().as_str() {
        "attack" => SkillKind::Attack,
        "strength" => SkillKind::Strength,
        "defence" | "defense" => SkillKind::Defence,
        "ranged" => SkillKind::Ranged,
        "magic" => SkillKind::Magic,
        "constitution" => SkillKind::Constitution,
        "mining" => SkillKind::Mining,
        "woodcutting" => SkillKind::Woodcutting,
        "fishing" => SkillKind::Fishing,
        "cooking" => SkillKind::Cooking,
        "smithing" => SkillKind::Smithing,
        "crafting" => SkillKind::Crafting,
        "slayer" => SkillKind::Slayer,
        "taming" => SkillKind::Taming,
        "bonding" => SkillKind::Bonding,
        _ => SkillKind::Mining,
    }
}

const ATTACK_RANGE: f32 = 80.0;
const INTERACT_RANGE: f32 = 50.0;
const BASE_ATTACK_DAMAGE: f32 = 10.0;

fn in_range(ctx: &ReducerContext, source: u64, target: u64, range: f32) -> bool {
    let Some(source_tf) = ctx.db.transform().entity_id().find(source) else {
        return false;
    };
    let Some(target_tf) = ctx.db.transform().entity_id().find(target) else {
        return false;
    };
    let dx = source_tf.pos_x - target_tf.pos_x;
    let dy = source_tf.pos_y - target_tf.pos_y;
    (dx * dx + dy * dy).sqrt() <= range
}

fn is_hostile(ctx: &ReducerContext, source: u64, target: u64) -> bool {
    let source_team = ctx
        .db
        .label()
        .entity_id()
        .find(source)
        .map(|label| label.team_id)
        .unwrap_or(0);
    let target_team = ctx
        .db
        .label()
        .entity_id()
        .find(target)
        .map(|label| label.team_id)
        .unwrap_or(0);
    source_team == 0 || target_team == 0 || source_team != target_team
}

fn find_attack_target(ctx: &ReducerContext, attacker_id: u64) -> Option<u64> {
    let attacker_tf = ctx.db.transform().entity_id().find(attacker_id)?;
    let mut candidates: Vec<(u64, f32)> = ctx
        .db
        .entity()
        .iter()
        .filter(|entity| entity.alive && entity.entity_id != attacker_id)
        .filter_map(|entity| {
            let target_id = entity.entity_id;
            let target_tf = ctx.db.transform().entity_id().find(target_id)?;
            let dx = attacker_tf.pos_x - target_tf.pos_x;
            let dy = attacker_tf.pos_y - target_tf.pos_y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist > ATTACK_RANGE || !is_hostile(ctx, attacker_id, target_id) {
                return None;
            }
            Some((target_id, dist))
        })
        .collect();
    candidates.sort_by(|(a_id, a_dist), (b_id, b_dist)| {
        a_dist
            .partial_cmp(b_dist)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a_id.cmp(b_id))
    });
    candidates.first().map(|(target_id, _)| *target_id)
}

fn find_interaction_target(ctx: &ReducerContext, entity_id: u64) -> Option<u64> {
    let source_tf = ctx.db.transform().entity_id().find(entity_id)?;
    let mut candidates: Vec<(u64, f32)> = ctx
        .db
        .entity()
        .iter()
        .filter(|entity| entity.alive && entity.entity_id != entity_id)
        .filter_map(|entity| {
            let target_tf = ctx.db.transform().entity_id().find(entity.entity_id)?;
            let dx = source_tf.pos_x - target_tf.pos_x;
            let dy = source_tf.pos_y - target_tf.pos_y;
            let dist = (dx * dx + dy * dy).sqrt();
            (dist <= INTERACT_RANGE).then_some((entity.entity_id, dist))
        })
        .collect();
    candidates.sort_by(|(a_id, a_dist), (b_id, b_dist)| {
        a_dist
            .partial_cmp(b_dist)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a_id.cmp(b_id))
    });
    candidates.first().map(|(target_id, _)| *target_id)
}

fn apply_attack(ctx: &ReducerContext, tick: u64, attacker_id: u64, defender_id: u64) -> bool {
    if attacker_id == defender_id || !in_range(ctx, attacker_id, defender_id, ATTACK_RANGE) {
        return false;
    }
    if !is_hostile(ctx, attacker_id, defender_id) {
        return false;
    }

    let Some(mut defender_health) = ctx.db.health().entity_id().find(defender_id) else {
        return false;
    };

    if defender_health.invulnerable {
        return false;
    }

    let damage = (BASE_ATTACK_DAMAGE - defender_health.armor).max(0.0);
    if damage <= 0.0 {
        return false;
    }

    defender_health.current = (defender_health.current - damage).max(0.0);
    let remaining = defender_health.current;
    let killed = remaining <= 0.0;
    ctx.db.health().entity_id().update(defender_health);

    ctx.db.combat_event().insert(CombatEventRow {
        event_id: 0,
        tick,
        attacker_entity_id: attacker_id,
        defender_entity_id: defender_id,
        damage_dealt: damage,
        defender_health_remaining: remaining,
        killed,
    });

    if killed {
        ctx.db.world_event().insert(WorldEventRow {
            event_id: 0,
            tick,
            event_kind: WorldEventKind::EntityDied,
            entity_id: defender_id,
            secondary_entity_id: Some(attacker_id),
            data_json: "{}".to_string(),
        });
    }

    true
}
