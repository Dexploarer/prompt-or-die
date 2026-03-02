use glam::Vec2;
use serde::{Deserialize, Serialize};
use crate::id::{AgentId, EntityId};

/// Every action an agent can take in the game.
/// This is the SAME action space for human and AI agents.
/// The engine validates these identically regardless of source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentAction {
    pub agent_id: AgentId,
    pub tick: u64,
    pub action: Action,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    // === MOVEMENT ===
    /// Move in a direction (normalized input vector)
    Move { direction: Vec2 },
    /// Stop all movement
    Stop,
    /// Face a specific direction (radians)
    Rotate { angle: f32 },
    /// Look at a world position
    LookAt { target: Vec2 },

    // === COMBAT ===
    /// Attack in facing direction
    Attack,
    /// Attack a specific entity
    AttackTarget { target: EntityId },
    /// Use ability by slot index
    UseAbility { slot: u8, target: Option<AbilityTarget> },

    // === INTERACTION ===
    /// Interact with nearest interactable
    Interact,
    /// Interact with a specific entity
    InteractWith { target: EntityId },
    /// Pick up an item
    Pickup { target: EntityId },
    /// Drop an item from inventory
    Drop { slot: u8 },
    /// Use an item from inventory
    UseItem { slot: u8 },

    // === COMMUNICATION ===
    /// Send a message (visible to agents in hearing range)
    Speak { message: String, volume: SpeakVolume },
    /// Send a signal (game-specific, e.g. ping, emote)
    Signal { signal_type: String, data: String },

    // === META ===
    /// Do nothing this tick (explicit no-op)
    Idle,
    /// Spawn request (only valid for system agents)
    Spawn { prefab: String, position: Vec2 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AbilityTarget {
    Position(Vec2),
    Entity(EntityId),
    Direction(Vec2),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SpeakVolume {
    Whisper,  // short range
    Normal,   // medium range
    Shout,    // long range
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

/// Constraints that limit how fast/often an agent can act.
/// Applied IDENTICALLY to human and AI agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConstraints {
    /// Max actions per tick
    pub actions_per_tick: u8,
    /// Cooldown ticks between attacks
    pub attack_cooldown: u32,
    /// Cooldown ticks between ability uses (per slot)
    pub ability_cooldowns: Vec<u32>,
    /// Whether agent can act this tick (stunned, dead, etc.)
    pub can_act: bool,
}

impl Default for AgentConstraints {
    fn default() -> Self {
        Self {
            actions_per_tick: 3,
            attack_cooldown: 30, // half second at 60 tps
            ability_cooldowns: vec![60, 120, 300],
            can_act: true,
        }
    }
}

/// Result of action validation
#[derive(Debug)]
pub enum ActionResult {
    /// Action is valid and will be executed
    Valid,
    /// Action rejected — reason provided
    Rejected(String),
    /// Action queued for next available tick
    Queued,
}

/// Validates an action against constraints and world state
pub fn validate_action(
    _action: &AgentAction,
    _constraints: &AgentConstraints,
    _tick: u64,
) -> ActionResult {
    if !_constraints.can_act {
        return ActionResult::Rejected("Agent cannot act (stunned/dead)".into());
    }

    // TODO: Check cooldowns, range, line of sight, etc.
    // These checks are IDENTICAL for human and AI agents

    ActionResult::Valid
}
