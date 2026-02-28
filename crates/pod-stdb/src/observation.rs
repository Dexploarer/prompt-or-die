//! Observation building — generates per-agent perception data each tick.
//!
//! For each entity with a Perception component, this module:
//! 1. Collects self-state (position, velocity, health, team)
//! 2. Finds visible entities (range + FOV check, sorted by distance)
//! 3. Finds audible events (speech within hearing range from previous tick)
//! 4. Serializes to JSON and stores as ObservationEventRow
//!
//! This mirrors the pod-core observation pipeline:
//!   - Vision: range check + FOV dot-product check
//!   - Hearing: min(hearing_range, speech_volume_range) with linear intensity falloff
//!   - Relationship: same non-zero team = Friendly, different non-zero = Hostile, else Neutral
//!   - Health visibility: only within 100 units

use spacetimedb::{Identity, ReducerContext, Table};
use crate::tables::*;
use crate::events::*;
use crate::types::AgentType;

// ============================================================
// CONSTANTS
// ============================================================

/// Max visible entities per observation (matches pod-core Balanced filter).
const MAX_VISIBLE_ENTITIES: usize = 10;

/// Max audible events per observation.
const MAX_AUDIBLE_EVENTS: usize = 10;

/// Health fraction is only visible within this range (units).
const HEALTH_VISIBILITY_RANGE: f32 = 100.0;

// ============================================================
// PUBLIC API
// ============================================================

/// Build and insert observation events for all entities with Perception components.
/// Called during execute_tick Phase 2.
///
/// Observations use the world state at the START of the tick (before actions are processed).
/// Speech events from the PREVIOUS tick are used for hearing (1-tick delay by design).
pub fn build_observations(ctx: &ReducerContext, current_tick: u64) {
    // ── Step 1: Pre-collect all data ──
    // Collect separately to avoid any borrow issues with table handles.

    // Observers: entities with Perception component
    let perceptions: Vec<PerceptionRow> = ctx.db.perception().iter().collect();

    // Pre-collect connected agents for RLS owner_identity lookup
    let connected_agents: Vec<ConnectedAgentRow> = ctx.db.connected_agent().iter().collect();

    // Resolve observer transforms + owner identity
    let observers: Vec<ObserverData> = perceptions
        .into_iter()
        .filter_map(|p| {
            let eid = p.entity_id;
            ctx.db.transform().entity_id().find(eid).map(|t| {
                // Find connected agent for this entity (if any) for RLS
                let owner_identity = connected_agents
                    .iter()
                    .find(|ca| ca.entity_id == eid)
                    .map(|ca| ca.identity);
                ObserverData {
                    entity_id: eid,
                    pos_x: t.pos_x,
                    pos_y: t.pos_y,
                    rotation: t.rotation,
                    vision_range: p.vision_range,
                    vision_fov: p.vision_fov,
                    hearing_range: p.hearing_range,
                    owner_identity,
                }
            })
        })
        .collect();

    if observers.is_empty() {
        return;
    }

    // All alive entities with transforms (potential targets + self-state source)
    let targets: Vec<TargetData> = ctx
        .db
        .entity()
        .iter()
        .filter(|e| e.alive)
        .filter_map(|e| {
            let eid = e.entity_id;
            ctx.db.transform().entity_id().find(eid).map(|t| {
                let label = ctx.db.label().entity_id().find(eid);
                let health = ctx.db.health().entity_id().find(eid);
                let vel = ctx.db.velocity().entity_id().find(eid);
                TargetData {
                    entity_id: eid,
                    agent_type: e.agent_type.clone(),
                    pos_x: t.pos_x,
                    pos_y: t.pos_y,
                    vel_x: vel.as_ref().map(|v| v.linear_x).unwrap_or(0.0),
                    vel_y: vel.as_ref().map(|v| v.linear_y).unwrap_or(0.0),
                    team_id: label.as_ref().map(|l| l.team_id).unwrap_or(0),
                    name: label.map(|l| l.name.clone()).unwrap_or_default(),
                    health_current: health.as_ref().map(|h| h.current),
                    health_max: health.as_ref().map(|h| h.max_hp),
                }
            })
        })
        .collect();

    // Speech events from previous tick (agents hear what was said last tick)
    let prev_tick = current_tick.saturating_sub(1);
    let speeches: Vec<SpeechData> = if current_tick > 0 {
        ctx.db
            .speech_event()
            .iter()
            .filter(|s| s.tick == prev_tick)
            .map(|s| SpeechData {
                speaker_entity_id: s.speaker_entity_id,
                message: s.message.clone(),
                volume_range: s.volume.range(),
                pos_x: s.pos_x,
                pos_y: s.pos_y,
            })
            .collect()
    } else {
        Vec::new() // No speech events on first tick
    };

    // ── Step 2: Build observations for each observer ──
    for obs in &observers {
        // Find self in targets for self-state data
        let self_data = targets.iter().find(|t| t.entity_id == obs.entity_id);
        let observer_team = self_data.map(|d| d.team_id).unwrap_or(0);

        // Build self_state JSON
        let self_json = build_self_state_json(obs, self_data);

        // Find visible entities (range + FOV, sorted by distance, capped)
        let visible_json = build_visible_json(obs, observer_team, &targets);

        // Find audible events (hearing range, capped)
        let audible_json = build_audible_json(obs, &speeches);

        // Assemble full observation JSON
        let obs_json = format!(
            r#"{{"tick":{},"self_state":{},"visible_entities":[{}],"audible_events":[{}]}}"#,
            current_tick, self_json, visible_json, audible_json
        );

        ctx.db.observation_event().insert(ObservationEventRow {
            event_id: 0, // auto_inc
            tick: current_tick,
            observer_entity_id: obs.entity_id,
            owner_identity: obs.owner_identity,
            observation_json: obs_json,
        });
    }

    log::debug!(
        "[pod-stdb] Built {} observations for tick {current_tick}",
        observers.len()
    );
}

/// Clear stale events and processed action submissions.
/// Called at the start of each tick before building new observations.
///
/// Policy:
/// - Observation events: cleared every tick (regenerated fresh)
/// - Action submissions: cleared after processing
/// - Speech/combat/world events: kept for 2 ticks, then pruned
pub fn clear_old_events(ctx: &ReducerContext, current_tick: u64) {
    // Clear ALL old observation events (new ones generated this tick)
    let old_obs: Vec<u64> = ctx
        .db
        .observation_event()
        .iter()
        .filter(|e| e.tick < current_tick)
        .map(|e| e.event_id)
        .collect();
    for id in old_obs {
        ctx.db.observation_event().event_id().delete(id);
    }

    // Clear processed action submissions (from previous ticks)
    let old_actions: Vec<u64> = ctx
        .db
        .action_submission()
        .iter()
        .filter(|a| a.tick < current_tick)
        .map(|a| a.submission_id)
        .collect();
    for id in old_actions {
        ctx.db.action_submission().submission_id().delete(id);
    }

    // Prune transient events older than 2 ticks
    if current_tick >= 2 {
        let cutoff = current_tick - 2;

        let old_speech: Vec<u64> = ctx
            .db
            .speech_event()
            .iter()
            .filter(|e| e.tick < cutoff)
            .map(|e| e.event_id)
            .collect();
        for id in old_speech {
            ctx.db.speech_event().event_id().delete(id);
        }

        let old_combat: Vec<u64> = ctx
            .db
            .combat_event()
            .iter()
            .filter(|e| e.tick < cutoff)
            .map(|e| e.event_id)
            .collect();
        for id in old_combat {
            ctx.db.combat_event().event_id().delete(id);
        }

        let old_world: Vec<u64> = ctx
            .db
            .world_event()
            .iter()
            .filter(|e| e.tick < cutoff)
            .map(|e| e.event_id)
            .collect();
        for id in old_world {
            ctx.db.world_event().event_id().delete(id);
        }
    }
}

// ============================================================
// INTERNAL DATA STRUCTURES
// ============================================================

/// Pre-collected observer data (Perception + Transform + optional owner Identity).
struct ObserverData {
    entity_id: u64,
    pos_x: f32,
    pos_y: f32,
    rotation: f32,
    vision_range: f32,
    /// Field of view in radians (2π = full circle)
    vision_fov: f32,
    hearing_range: f32,
    /// Identity of the connected client controlling this entity (for RLS).
    /// None if no client is connected (server-side agent).
    owner_identity: Option<Identity>,
}

/// Pre-collected target entity data for spatial queries.
struct TargetData {
    entity_id: u64,
    agent_type: Option<AgentType>,
    pos_x: f32,
    pos_y: f32,
    vel_x: f32,
    vel_y: f32,
    team_id: u8,
    name: String,
    health_current: Option<f32>,
    health_max: Option<f32>,
}

/// Pre-collected speech event data for hearing queries.
struct SpeechData {
    speaker_entity_id: u64,
    message: String,
    volume_range: f32,
    pos_x: f32,
    pos_y: f32,
}

/// Intermediate visible entity entry (for sorting before JSON conversion).
struct VisibleEntry {
    entity_id: u64,
    name: String,
    entity_type: &'static str,
    pos_x: f32,
    pos_y: f32,
    distance: f32,
    relationship: &'static str,
    health_fraction: Option<f32>,
}

/// Intermediate audible event entry (for sorting before JSON conversion).
struct AudibleEntry {
    speaker_entity_id: u64,
    message: String,
    distance: f32,
    intensity: f32,
}

// ============================================================
// JSON BUILDERS
// ============================================================

/// Build self_state JSON object.
fn build_self_state_json(obs: &ObserverData, self_data: Option<&TargetData>) -> String {
    let vel_x = self_data.map(|d| d.vel_x).unwrap_or(0.0);
    let vel_y = self_data.map(|d| d.vel_y).unwrap_or(0.0);
    let health = self_data.and_then(|d| d.health_current).unwrap_or(0.0);
    let max_hp = self_data.and_then(|d| d.health_max).unwrap_or(0.0);
    let team = self_data.map(|d| d.team_id).unwrap_or(0);
    let name = self_data
        .map(|d| escape_json_string(&d.name))
        .unwrap_or_default();

    format!(
        r#"{{"entity_id":{},"name":"{}","position":[{:.1},{:.1}],"rotation":{:.3},"velocity":[{:.1},{:.1}],"health":{:.1},"max_health":{:.1},"team":{}}}"#,
        obs.entity_id, name,
        obs.pos_x, obs.pos_y,
        obs.rotation,
        vel_x, vel_y,
        health, max_hp,
        team
    )
}

/// Find visible entities and build JSON array string.
/// Applies range check + FOV check, sorts by distance, caps at MAX_VISIBLE_ENTITIES.
fn build_visible_json(
    obs: &ObserverData,
    observer_team: u8,
    targets: &[TargetData],
) -> String {
    let full_circle = obs.vision_fov >= std::f32::consts::TAU - 0.01;

    // Collect visible entities with distance
    let mut visible: Vec<VisibleEntry> = Vec::new();

    for target in targets {
        if target.entity_id == obs.entity_id {
            continue; // Skip self
        }

        let dx = target.pos_x - obs.pos_x;
        let dy = target.pos_y - obs.pos_y;
        let dist_sq: f32 = dx * dx + dy * dy;
        let dist: f32 = dist_sq.sqrt();

        // Range check
        if dist > obs.vision_range {
            continue;
        }

        // FOV check (skip if entity has 360° vision)
        if !full_circle && !is_in_fov(obs.pos_x, obs.pos_y, obs.rotation, obs.vision_fov, target.pos_x, target.pos_y) {
            continue;
        }

        // Relationship
        let relationship = determine_relationship(observer_team, target.team_id);

        // Health fraction (only visible at close range)
        let health_fraction = if dist < HEALTH_VISIBILITY_RANGE {
            match (target.health_current, target.health_max) {
                (Some(cur), Some(max)) if max > 0.0 => Some(cur / max),
                _ => None,
            }
        } else {
            None
        };

        visible.push(VisibleEntry {
            entity_id: target.entity_id,
            name: target.name.clone(),
            entity_type: agent_type_str(&target.agent_type),
            pos_x: target.pos_x,
            pos_y: target.pos_y,
            distance: dist,
            relationship,
            health_fraction,
        });
    }

    // Sort by distance (closest first) — matches pod-core salience ordering
    visible.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));

    // Cap at max
    visible.truncate(MAX_VISIBLE_ENTITIES);

    // Convert to JSON strings
    let parts: Vec<String> = visible
        .iter()
        .map(|v| {
            let name_esc = escape_json_string(&v.name);
            let mut json = format!(
                r#"{{"entity_id":{},"name":"{}","type":"{}","position":[{:.1},{:.1}],"distance":{:.1},"relationship":"{}""#,
                v.entity_id, name_esc, v.entity_type,
                v.pos_x, v.pos_y,
                v.distance, v.relationship
            );
            if let Some(hf) = v.health_fraction {
                json.push_str(&format!(r#","health_fraction":{:.2}"#, hf));
            }
            json.push('}');
            json
        })
        .collect();

    parts.join(",")
}

/// Find audible speech events and build JSON array string.
/// Applies hearing range + volume range check, calculates intensity with linear falloff.
fn build_audible_json(obs: &ObserverData, speeches: &[SpeechData]) -> String {
    let mut audible: Vec<AudibleEntry> = Vec::new();

    for speech in speeches {
        if speech.speaker_entity_id == obs.entity_id {
            continue; // Skip own speech
        }

        let dx = speech.pos_x - obs.pos_x;
        let dy = speech.pos_y - obs.pos_y;
        let dist_sq: f32 = dx * dx + dy * dy;
        let dist: f32 = dist_sq.sqrt();

        // Effective range = min(observer hearing, speech volume range)
        let effective_range = obs.hearing_range.min(speech.volume_range);
        if dist > effective_range {
            continue;
        }

        // Linear intensity falloff: 1.0 at origin, 0.0 at max range
        let intensity: f32 = if effective_range > 0.0 {
            (1.0 - dist / effective_range).max(0.0)
        } else {
            0.0
        };

        audible.push(AudibleEntry {
            speaker_entity_id: speech.speaker_entity_id,
            message: speech.message.clone(),
            distance: dist,
            intensity,
        });
    }

    // Sort by intensity (loudest first)
    audible.sort_by(|a, b| b.intensity.partial_cmp(&a.intensity).unwrap_or(std::cmp::Ordering::Equal));

    // Cap at max
    audible.truncate(MAX_AUDIBLE_EVENTS);

    // Convert to JSON strings
    let parts: Vec<String> = audible
        .iter()
        .map(|a| {
            let msg_esc = escape_json_string(&a.message);
            format!(
                r#"{{"speaker_entity_id":{},"message":"{}","distance":{:.1},"intensity":{:.2}}}"#,
                a.speaker_entity_id, msg_esc, a.distance, a.intensity
            )
        })
        .collect();

    parts.join(",")
}

// ============================================================
// SPATIAL HELPERS
// ============================================================

/// Check if a target position falls within the observer's field of view.
///
/// Uses dot-product of forward vector and direction-to-target:
///   forward = (cos(rotation), sin(rotation))
///   to_target = normalize(target - observer)
///   visible if dot(forward, to_target) >= cos(fov / 2)
fn is_in_fov(
    obs_x: f32,
    obs_y: f32,
    obs_rotation: f32,
    fov: f32,
    target_x: f32,
    target_y: f32,
) -> bool {
    // Forward direction from rotation angle
    let fwd_x = obs_rotation.cos();
    let fwd_y = obs_rotation.sin();

    // Direction to target (unnormalized)
    let dx = target_x - obs_x;
    let dy = target_y - obs_y;
    let dist_sq: f32 = dx * dx + dy * dy;

    // Target is on top of observer — always visible
    if dist_sq < 0.001 {
        return true;
    }

    // Normalize direction to target
    let inv_dist: f32 = 1.0 / dist_sq.sqrt();
    let dir_x = dx * inv_dist;
    let dir_y = dy * inv_dist;

    // cos(angle) between forward and direction
    let dot: f32 = fwd_x * dir_x + fwd_y * dir_y;

    // Target is within FOV if angle < fov/2, i.e. cos(angle) > cos(fov/2)
    let half_fov = fov * 0.5;
    dot >= half_fov.cos()
}

/// Determine relationship between observer and target based on team IDs.
///   - Both have same non-zero team → Friendly
///   - Both have different non-zero teams → Hostile
///   - Either has team 0 (no team) → Neutral
fn determine_relationship(observer_team: u8, target_team: u8) -> &'static str {
    if observer_team == 0 || target_team == 0 {
        "Neutral"
    } else if observer_team == target_team {
        "Friendly"
    } else {
        "Hostile"
    }
}

/// Convert AgentType to display string for observations.
fn agent_type_str(agent_type: &Option<AgentType>) -> &'static str {
    match agent_type {
        Some(AgentType::Human) => "Human",
        Some(AgentType::LlmAgent) => "LlmAgent",
        Some(AgentType::NeuralAgent) => "NeuralAgent",
        Some(AgentType::ScriptedNpc) => "ScriptedNpc",
        Some(AgentType::System) => "System",
        None => "Object",
    }
}

/// Escape a string for safe JSON embedding.
/// Handles quotes, backslashes, and control characters.
fn escape_json_string(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                // Control characters as unicode escapes
                escaped.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => escaped.push(c),
        }
    }
    escaped
}
