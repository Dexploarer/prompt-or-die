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

use spacetimedb::{ReducerContext, Table};
use crate::tables::*;
use crate::events::*;
use crate::types::*;

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
}

// ============================================================
// WORLD MANAGEMENT REDUCERS
// ============================================================

/// Create or reset the game world with custom parameters.
#[spacetimedb::reducer]
pub fn create_world(
    ctx: &ReducerContext,
    seed: u64,
    width: f32,
    height: f32,
    tps: u32,
) {
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
pub fn spawn_entity(
    ctx: &ReducerContext,
    pos_x: f32,
    pos_y: f32,
    agent_type: Option<AgentType>,
) {
    let ws = ctx.db.world_state().id().find(0)
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
    let ws = ctx.db.world_state().id().find(0)
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
        actions_per_tick: 3,
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
    let entity = ctx.db.entity().entity_id().find(entity_id)
        .expect("Entity not found");
    if !entity.alive {
        panic!("Cannot connect to dead entity {entity_id}");
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
    let ws = ctx.db.world_state().id().find(0)
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
    let mut ws = ctx.db.world_state().id().find(0)
        .expect("World not initialized");

    if ws.paused {
        return;
    }

    let current_tick = ws.tick;
    let dt = 1.0 / ws.ticks_per_second as f32;

    // ── Phase 1: Clear previous tick's events ──
    crate::observation::clear_old_events(ctx, current_tick);

    // ── Phase 2: Build observations ──
    crate::observation::build_observations(ctx, current_tick);

    // ── Phase 3: Process submitted actions ──
    // Collect actions for this tick
    for submission in ctx.db.action_submission().iter() {
        if submission.tick != current_tick {
            continue;
        }

        let eid = submission.entity_id;

        // Check constraints
        if let Some(constraints) = ctx.db.agent_constraints().entity_id().find(eid) {
            if !constraints.can_act {
                continue; // Skip stunned/dead agents
            }
        }

        // Execute action based on kind
        match submission.action_kind {
            ActionKind::Move => {
                if let (Some(dx), Some(dy)) = (submission.direction_x, submission.direction_y) {
                    if let Some(mvmt) = ctx.db.movement().entity_id().find(eid) {
                        if let Some(mut vel) = ctx.db.velocity().entity_id().find(eid) {
                            // Set velocity in direction, clamped to max_speed
                            let sum: f32 = dx * dx + dy * dy;
                            let len = sum.sqrt();
                            if len > 0.001 {
                                let nx = dx / len;
                                let ny = dy / len;
                                vel.linear_x = nx * mvmt.max_speed;
                                vel.linear_y = ny * mvmt.max_speed;
                                ctx.db.velocity().entity_id().update(vel);
                            }
                        }
                    }
                }
            }
            ActionKind::Stop => {
                if let Some(mut vel) = ctx.db.velocity().entity_id().find(eid) {
                    vel.linear_x = 0.0;
                    vel.linear_y = 0.0;
                    vel.angular = 0.0;
                    ctx.db.velocity().entity_id().update(vel);
                }
            }
            ActionKind::Rotate => {
                if let Some(angle) = submission.angle {
                    if let Some(mut tf) = ctx.db.transform().entity_id().find(eid) {
                        tf.rotation = angle;
                        ctx.db.transform().entity_id().update(tf);
                    }
                }
            }
            ActionKind::Speak => {
                let has_msg = submission.message.is_some() && submission.volume.is_some();
                if has_msg {
                    let msg: String = submission.message.clone().unwrap();
                    let vol: SpeakVolume = submission.volume.clone().unwrap();
                    if let Some(tf) = ctx.db.transform().entity_id().find(eid) {
                        ctx.db.speech_event().insert(SpeechEventRow {
                            event_id: 0, // auto_inc
                            tick: current_tick,
                            speaker_entity_id: eid,
                            message: msg,
                            volume: vol,
                            pos_x: tf.pos_x,
                            pos_y: tf.pos_y,
                        });
                    }
                }
            }
            ActionKind::Idle => {
                // Explicit no-op
            }
            // TODO: Implement remaining action kinds
            _ => {
                log::debug!("[pod-stdb] Unhandled action kind {:?} for entity {eid}", submission.action_kind);
            }
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

    // ── Phase 5: Advance tick ──
    ws.tick = current_tick + 1;
    ctx.db.world_state().id().update(ws);

    log::debug!("[pod-stdb] Tick {current_tick} → {}", current_tick + 1);
}
