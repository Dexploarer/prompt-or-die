//! Shared type definitions for SpacetimeDB tables and reducers.
//!
//! These types are shared between the WASM module and native client code.
//! When the `module` feature is enabled, they derive `SpacetimeType` for
//! BSATN serialization in SpacetimeDB tables. Without the feature, they
//! are plain Rust enums usable in any context (client, tests, etc.).

#[cfg(feature = "module")]
use spacetimedb::SpacetimeType;

// ============================================================
// AGENT CLASSIFICATION
// ============================================================

/// Agent type classification — determines how decisions are made.
/// Every agent type goes through the SAME pipeline with IDENTICAL constraints.
#[cfg_attr(feature = "module", derive(SpacetimeType))]
#[derive(Clone, Debug, PartialEq)]
pub enum AgentType {
    /// Human player connected via client
    Human,
    /// LLM-powered agent (GPT, Claude, etc.)
    LlmAgent,
    /// Neural network agent (ONNX policy)
    NeuralAgent,
    /// Scripted NPC (behavior tree / FSM / Lua)
    ScriptedNpc,
    /// System agent (world manager, spawner)
    System,
}

// ============================================================
// PHYSICS TYPES
// ============================================================

/// Physics body type — mirrors pod-core BodyType
#[cfg_attr(feature = "module", derive(SpacetimeType))]
#[derive(Clone, Debug, PartialEq)]
pub enum BodyKind {
    /// Doesn't move, infinite mass (walls, ground)
    Static,
    /// Moved by forces (players, projectiles)
    Dynamic,
    /// Moved by code, pushes dynamic bodies (platforms)
    Kinematic,
}

/// Collider shape discriminant.
/// Shape parameters (radius, half_width, half_height) stored as separate columns.
#[cfg_attr(feature = "module", derive(SpacetimeType))]
#[derive(Clone, Debug, PartialEq)]
pub enum ColliderShapeKind {
    Circle,
    Box,
    Capsule,
}

// ============================================================
// ACTION TYPES
// ============================================================

/// Action type discriminant for the action submission table.
/// Mirrors pod-core Action enum variants.
#[cfg_attr(feature = "module", derive(SpacetimeType))]
#[derive(Clone, Debug, PartialEq)]
pub enum ActionKind {
    // Movement
    Move,
    Stop,
    Rotate,
    LookAt,
    // Combat
    Attack,
    AttackTarget,
    UseAbility,
    CaptureCreature,
    SummonCompanion,
    CommandCompanion,
    // Interaction
    Interact,
    InteractWith,
    Pickup,
    Drop,
    UseItem,
    GatherResource,
    Loot,
    // Communication
    Speak,
    Signal,
    SetAutoRetaliate,
    // Meta
    Idle,
    Spawn,
}

/// Ability target type discriminant
#[cfg_attr(feature = "module", derive(SpacetimeType))]
#[derive(Clone, Debug, PartialEq)]
pub enum AbilityTargetKind {
    Position,
    Entity,
    Direction,
    None,
}

/// Speech volume level — mirrors pod-core SpeakVolume
#[cfg_attr(feature = "module", derive(SpacetimeType))]
#[derive(Clone, Debug, PartialEq)]
pub enum SpeakVolume {
    /// Short range (~50 units)
    Whisper,
    /// Medium range (~200 units)
    Normal,
    /// Long range (~500 units)
    Shout,
}

impl SpeakVolume {
    pub fn range(&self) -> f32 {
        match self {
            SpeakVolume::Whisper => 50.0,
            SpeakVolume::Normal => 200.0,
            SpeakVolume::Shout => 500.0,
        }
    }
}

// ============================================================
// EVENT TYPES
// ============================================================

/// World event type classification
#[cfg_attr(feature = "module", derive(SpacetimeType))]
#[derive(Clone, Debug, PartialEq)]
pub enum WorldEventKind {
    EntitySpawned,
    EntityDespawned,
    EntityDied,
    InteractionTriggered,
    ItemPickedUp,
    ItemDropped,
    AbilityUsed,
    WorldCreated,
    WorldReset,
    TickAdvanced,
}

/// Match lifecycle state.
#[cfg_attr(feature = "module", derive(SpacetimeType))]
#[derive(Clone, Debug, PartialEq)]
pub enum MatchState {
    /// Match is queued and waiting for full party assignment.
    Queued,
    /// Match has been created and is currently running.
    InProgress,
    /// Match completed normally.
    Completed,
    /// Match was cancelled before completion.
    Cancelled,
}
