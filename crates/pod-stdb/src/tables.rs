//! SpacetimeDB table definitions mirroring ECS components from pod-core.
//!
//! Design: Each ECS component maps to a separate relational table keyed by entity_id.
//! This preserves the component-per-entity model while enabling SQL queries and
//! fine-grained subscriptions.
//!
//! All tables are `public` — clients can subscribe to any table.
//! Row-level security for observations is handled via filtered subscriptions.

use spacetimedb::{Identity, Timestamp};
use crate::types::*;

// ============================================================
// CORE ENTITY TABLE
// ============================================================

/// Core entity record — every game object has exactly one row here.
/// The entity_id is auto-incremented and serves as the foreign key for all component tables.
#[spacetimedb::table(name = entity, public)]
pub struct EntityRow {
    #[primary_key]
    #[auto_inc]
    pub entity_id: u64,
    /// Agent type (None for non-agent entities like walls, items, projectiles)
    pub agent_type: Option<AgentType>,
    /// SpacetimeDB identity of controlling client (for human agents)
    pub owner_identity: Option<Identity>,
    /// Whether the entity is currently alive/active
    pub alive: bool,
    /// Tick when this entity was created
    pub created_tick: u64,
}

// ============================================================
// SPATIAL COMPONENT TABLES
// ============================================================

/// Position, rotation, scale in 2D world space.
/// Mirrors pod-core Transform component (Vec2 flattened to f32 pairs).
#[spacetimedb::table(name = transform, public)]
pub struct TransformRow {
    #[primary_key]
    pub entity_id: u64,
    pub pos_x: f32,
    pub pos_y: f32,
    pub rotation: f32,
    pub scale_x: f32,
    pub scale_y: f32,
}

/// Linear and angular velocity.
/// Mirrors pod-core Velocity component.
#[spacetimedb::table(name = velocity, public)]
pub struct VelocityRow {
    #[primary_key]
    pub entity_id: u64,
    pub linear_x: f32,
    pub linear_y: f32,
    pub angular: f32,
}

// ============================================================
// PHYSICS COMPONENT TABLES
// ============================================================

/// Physics rigid body properties.
/// Mirrors pod-core RigidBody component.
#[spacetimedb::table(name = rigid_body, public)]
pub struct RigidBodyRow {
    #[primary_key]
    pub entity_id: u64,
    pub body_kind: BodyKind,
    pub mass: f32,
    pub friction: f32,
    pub restitution: f32,
    pub gravity_scale: f32,
}

/// Collision shape definition.
/// Mirrors pod-core Collider component.
/// Shape parameters are flattened — use shape_kind to determine which fields are relevant:
///   Circle: shape_radius
///   Box: shape_half_width, shape_half_height
///   Capsule: shape_half_height, shape_radius
#[spacetimedb::table(name = collider, public)]
pub struct ColliderRow {
    #[primary_key]
    pub entity_id: u64,
    pub shape_kind: ColliderShapeKind,
    pub shape_radius: f32,
    pub shape_half_width: f32,
    pub shape_half_height: f32,
    pub is_trigger: bool,
    pub collision_group: u32,
    pub collision_mask: u32,
}

// ============================================================
// VISUAL COMPONENT TABLES
// ============================================================

/// Sprite rendering component.
/// Mirrors pod-core Sprite component (RGBA color flattened to 4 floats).
#[spacetimedb::table(name = sprite, public)]
pub struct SpriteRow {
    #[primary_key]
    pub entity_id: u64,
    pub texture: String,
    pub frame: u32,
    pub layer: i32,
    pub color_r: f32,
    pub color_g: f32,
    pub color_b: f32,
    pub color_a: f32,
    pub visible: bool,
}

/// Simple colored rectangle for prototyping (no texture needed).
/// Mirrors pod-core ColorRect component.
#[spacetimedb::table(name = color_rect, public)]
pub struct ColorRectRow {
    #[primary_key]
    pub entity_id: u64,
    pub width: f32,
    pub height: f32,
    pub color_r: f32,
    pub color_g: f32,
    pub color_b: f32,
    pub color_a: f32,
    pub layer: i32,
}

// ============================================================
// GAMEPLAY COMPONENT TABLES
// ============================================================

/// Health and damage tracking.
/// Mirrors pod-core Health component.
#[spacetimedb::table(name = health, public)]
pub struct HealthRow {
    #[primary_key]
    pub entity_id: u64,
    pub current: f32,
    pub max_hp: f32,
    pub armor: f32,
    pub invulnerable: bool,
}

/// Entity label and team affiliation.
/// Mirrors pod-core Label component.
/// Team is stored as u8: 0 = no team, 1-255 = team number.
#[spacetimedb::table(name = label, public)]
pub struct LabelRow {
    #[primary_key]
    pub entity_id: u64,
    pub name: String,
    pub team_id: u8,
}

/// Agent perception parameters — defines what an agent can sense.
/// Mirrors pod-core Perception component.
#[spacetimedb::table(name = perception, public)]
pub struct PerceptionRow {
    #[primary_key]
    pub entity_id: u64,
    pub vision_range: f32,
    /// Field of view in radians (2π = full circle)
    pub vision_fov: f32,
    pub hearing_range: f32,
}

/// Movement parameters for agent-controlled entities.
/// Mirrors pod-core Movement component.
#[spacetimedb::table(name = movement, public)]
pub struct MovementRow {
    #[primary_key]
    pub entity_id: u64,
    pub max_speed: f32,
    pub acceleration: f32,
    pub deceleration: f32,
    /// Radians per second
    pub turn_rate: f32,
}

/// Agent constraints — action budget, cooldowns.
/// Mirrors pod-core AgentConstraints.
/// Applied IDENTICALLY to human and AI agents.
#[spacetimedb::table(name = agent_constraints, public)]
pub struct AgentConstraintsRow {
    #[primary_key]
    pub entity_id: u64,
    pub actions_per_tick: u8,
    pub attack_cooldown: u32,
    /// JSON-encoded Vec<u32> of per-ability cooldown durations
    pub ability_cooldowns_json: String,
    pub can_act: bool,
    /// Remaining ticks until attack is available
    pub attack_cooldown_remaining: u32,
}

/// Attached script component.
/// Mirrors pod-core Script component.
#[spacetimedb::table(name = script, public)]
pub struct ScriptRow {
    #[primary_key]
    pub entity_id: u64,
    pub source: String,
    pub enabled: bool,
}

// ============================================================
// WORLD STATE (SINGLETON)
// ============================================================

/// Global world state — singleton row (id always = 0).
/// Contains tick counter, RNG seed, world dimensions, and configuration.
#[spacetimedb::table(name = world_state, public)]
pub struct WorldStateRow {
    #[primary_key]
    pub id: u32,
    pub tick: u64,
    pub rng_seed: u64,
    pub ticks_per_second: u32,
    pub world_width: f32,
    pub world_height: f32,
    pub max_entities: u32,
    pub paused: bool,
}

// ============================================================
// ACTION SUBMISSION TABLE
// ============================================================

/// Pending action submitted by an agent for the current tick.
/// Action parameters are flattened — use action_kind to determine which fields are relevant.
/// Cleared after each tick's execution phase.
#[spacetimedb::table(name = action_submission, public)]
pub struct ActionSubmissionRow {
    #[primary_key]
    #[auto_inc]
    pub submission_id: u64,
    pub entity_id: u64,
    pub tick: u64,
    pub action_kind: ActionKind,
    // Movement params
    pub direction_x: Option<f32>,
    pub direction_y: Option<f32>,
    pub angle: Option<f32>,
    pub target_x: Option<f32>,
    pub target_y: Option<f32>,
    // Target entity
    pub target_entity_id: Option<u64>,
    // Ability
    pub ability_slot: Option<u8>,
    pub ability_target_kind: Option<AbilityTargetKind>,
    // Communication
    pub message: Option<String>,
    pub volume: Option<SpeakVolume>,
    pub signal_type: Option<String>,
    pub signal_data: Option<String>,
    // Spawn
    pub prefab: Option<String>,
}

// ============================================================
// CONNECTED AGENTS TABLE
// ============================================================

/// Currently connected agents (human or remote AI).
/// Updated on client_connected / client_disconnected lifecycle events.
#[spacetimedb::table(name = connected_agent, public)]
pub struct ConnectedAgentRow {
    #[primary_key]
    pub identity: Identity,
    pub entity_id: u64,
    pub agent_type: AgentType,
    pub display_name: String,
    pub connected_at: Timestamp,
}

// ============================================================
// LOBBY TABLES
// ============================================================

/// Lobby metadata for pre-game grouping and matchmaking.
#[spacetimedb::table(name = lobby, public)]
pub struct LobbyRow {
    #[primary_key]
    #[auto_inc]
    pub lobby_id: u64,
    pub name: String,
    pub host_identity: Identity,
    pub host_entity_id: u64,
    pub max_players: u32,
    pub is_private: bool,
    pub created_at: Timestamp,
    pub started: bool,
}

/// Membership links between identities and lobby slots.
#[spacetimedb::table(name = lobby_member, public)]
pub struct LobbyMemberRow {
    #[primary_key]
    #[auto_inc]
    pub membership_id: u64,
    pub lobby_id: u64,
    pub identity: Identity,
    pub entity_id: u64,
    pub joined_at_tick: u64,
    pub is_ready: bool,
}

// ============================================================
// MATCHMAKING TABLES
// ============================================================

/// Global matchmaking queue entries.
///
/// Players enqueue one or more times for a desired party size.
/// Matchmaking reducers consume rows from this table in queue order and create
/// active matches in `game_match`.
#[spacetimedb::table(name = match_queue, public)]
pub struct MatchQueueRow {
    #[primary_key]
    #[auto_inc]
    pub queue_id: u64,
    pub identity: Identity,
    pub entity_id: u64,
    /// Desired party size for this queue entry (e.g., 2v2, 1v1, squads).
    pub desired_party_size: u32,
    /// World tick when the entity was queued.
    pub queued_at_tick: u64,
}

/// Match metadata for active/completed matches.
#[spacetimedb::table(name = game_match, public)]
pub struct GameMatchRow {
    #[primary_key]
    #[auto_inc]
    pub match_id: u64,
    pub created_tick: u64,
    pub max_players: u32,
    pub state: MatchState,
    /// Null until the match starts; stored for compatibility.
    pub started_tick: Option<u64>,
}

/// Participant membership in a specific match.
#[spacetimedb::table(name = match_participant, public)]
pub struct MatchParticipantRow {
    #[primary_key]
    #[auto_inc]
    pub participant_id: u64,
    pub match_id: u64,
    pub identity: Identity,
    pub entity_id: u64,
    pub team_id: u8,
    pub joined_at_tick: u64,
}
