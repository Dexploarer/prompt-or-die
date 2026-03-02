//! Event tables — transient per-tick data populated during execute_tick.
//!
//! These tables store events generated during tick execution:
//! - Observations: per-agent perception data (what each agent can see/hear)
//! - Combat: damage dealt, kills
//! - Speech: messages spoken by agents
//! - World: entity lifecycle events (spawn, despawn, death)
//!
//! Events are cleared at the start of each tick before new events are generated.
//! Clients subscribe to these tables to receive real-time game events.

use spacetimedb::Identity;
use crate::types::*;

// ============================================================
// ROW-LEVEL SECURITY — OBSERVATION VISIBILITY
// ============================================================

/// Client visibility filter for observation_event table.
/// Each connected client can only see observations for the entity they control.
///
/// - Connected agents have owner_identity set to their Identity
/// - Server-side agents (ScriptedNpc, local AI) have owner_identity = None,
///   meaning no client can subscribe to their observations (server-only)
/// - Reducers (server-side) can always read all rows regardless of this filter
///
/// NOTE: Requires spacetimedb `unstable` feature. If this feature is not available
/// in the current SDK version, this filter is a no-op and the table remains public.
/// In that case, clients should use filtered subscriptions (WHERE observer_entity_id = ?)
/// to achieve equivalent behavior.
#[cfg(feature = "unstable")]
mod rls {
    use spacetimedb::{client_visibility_filter, Filter};

    #[client_visibility_filter]
    const OBSERVATION_FILTER: Filter = Filter::Sql(
        "SELECT * FROM observation_event WHERE observation_event.owner_identity = :sender"
    );
}

// ============================================================
// OBSERVATION EVENTS
// ============================================================

/// Per-agent observation generated each tick.
/// Contains everything the agent can perceive based on their Perception component.
/// Observation data is JSON-encoded for flexibility during development.
///
/// Row-level security: owner_identity restricts which client can see each observation.
/// Connected agents see only their own observations; server-side agents' observations
/// are invisible to all clients (reducers can still access them).
#[spacetimedb::table(name = observation_event, public)]
pub struct ObservationEventRow {
    #[primary_key]
    #[auto_inc]
    pub event_id: u64,
    pub tick: u64,
    /// The entity this observation is FOR
    pub observer_entity_id: u64,
    /// Identity of the client that should see this observation.
    /// None for server-side agents (no client should see these).
    /// Used by client_visibility_filter for row-level security.
    pub owner_identity: Option<Identity>,
    /// JSON-encoded observation data:
    /// { "visible_entities": [...], "heard_events": [...], "self_state": {...} }
    pub observation_json: String,
}

// ============================================================
// COMBAT EVENTS
// ============================================================

/// Combat event — emitted when damage is dealt.
#[spacetimedb::table(name = combat_event, public)]
pub struct CombatEventRow {
    #[primary_key]
    #[auto_inc]
    pub event_id: u64,
    pub tick: u64,
    pub attacker_entity_id: u64,
    pub defender_entity_id: u64,
    pub damage_dealt: f32,
    pub defender_health_remaining: f32,
    pub killed: bool,
}

// ============================================================
// SPEECH EVENTS
// ============================================================

/// Speech event — emitted when an agent speaks.
/// Other agents within hearing range (based on volume) can perceive this.
#[spacetimedb::table(name = speech_event, public)]
pub struct SpeechEventRow {
    #[primary_key]
    #[auto_inc]
    pub event_id: u64,
    pub tick: u64,
    pub speaker_entity_id: u64,
    pub message: String,
    pub volume: SpeakVolume,
    /// Speaker position (for range checks)
    pub pos_x: f32,
    pub pos_y: f32,
}

// ============================================================
// WORLD EVENTS
// ============================================================

/// General world event — entity lifecycle, interactions, abilities, etc.
#[spacetimedb::table(name = world_event, public)]
pub struct WorldEventRow {
    #[primary_key]
    #[auto_inc]
    pub event_id: u64,
    pub tick: u64,
    pub event_kind: WorldEventKind,
    /// Primary entity involved (e.g., spawned entity, dying entity)
    pub entity_id: u64,
    /// Secondary entity involved (e.g., interactee, killer)
    pub secondary_entity_id: Option<u64>,
    /// JSON-encoded event-specific data for extensibility
    pub data_json: String,
}
