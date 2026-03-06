use crate::component::SkillKind;
use crate::id::{AgentId, EntityId};
use glam::Vec2;
use serde::{Deserialize, Serialize};

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
    UseAbility {
        slot: u8,
        target: Option<AbilityTarget>,
    },
    /// Capture a weakened wild creature into the companion roster.
    CaptureCreature {
        target: EntityId,
        tool_slot: Option<u8>,
    },
    /// Summon a companion from the roster into the active slot.
    SummonCompanion { slot: u8 },
    /// Issue a direct order to a companion in the roster.
    CommandCompanion {
        slot: u8,
        command: CompanionCommand,
        target: Option<EntityId>,
    },

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
    /// Gather from a resource node using the specified skill.
    GatherResource { target: EntityId, skill: SkillKind },
    /// Claim a loot container or dropped bundle.
    Loot { target: EntityId },

    // === COMMUNICATION ===
    /// Send a message (visible to agents in hearing range)
    Speak {
        message: String,
        volume: SpeakVolume,
    },
    /// Send a signal (game-specific, e.g. ping, emote)
    Signal { signal_type: String, data: String },
    /// Toggle RuneScape-style auto retaliation on or off.
    SetAutoRetaliate { enabled: bool },

    // === META ===
    /// Do nothing this tick (explicit no-op)
    Idle,
    /// Spawn request (only valid for system agents)
    Spawn { prefab: String, position: Vec2 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompanionCommand {
    Attack,
    Follow,
    Guard,
    Recall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AbilityTarget {
    Position(Vec2),
    Entity(EntityId),
    Direction(Vec2),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SpeakVolume {
    Whisper, // short range
    Normal,  // medium range
    Shout,   // long range
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
    agent_action: &AgentAction,
    constraints: &AgentConstraints,
    _tick: u64,
) -> ActionResult {
    if !constraints.can_act {
        return ActionResult::Rejected("Agent cannot act (stunned/dead)".into());
    }

    if constraints.actions_per_tick == 0 {
        return ActionResult::Rejected("Agent cannot act this tick".into());
    }

    let rejection = validate_action_payload(&agent_action.action);
    if let Some(reason) = rejection {
        return ActionResult::Rejected(reason);
    }

    ActionResult::Valid
}

fn validate_action_payload(action: &Action) -> Option<String> {
    match action {
        Action::Move { direction } => {
            if !direction.is_finite() {
                return Some("Move direction must be finite".into());
            }
            if direction.length_squared() > 2.25 {
                return Some("Move direction magnitude too large".into());
            }
            None
        }
        Action::Rotate { angle } => {
            if !angle.is_finite() {
                return Some("Rotation angle must be finite".into());
            }
            None
        }
        Action::LookAt { target } => {
            if !target.is_finite() {
                return Some("LookAt target must be finite".into());
            }
            None
        }
        Action::Speak { message, .. } => {
            if message.len() > 512 {
                return Some("Speak message too long".into());
            }
            None
        }
        Action::Signal { signal_type, data } => {
            if signal_type.is_empty() {
                return Some("Signal type cannot be empty".into());
            }
            if signal_type.len() > 64 || data.len() > 256 {
                return Some("Signal payload too long".into());
            }
            None
        }
        Action::Spawn { prefab, position } => {
            if prefab.is_empty() {
                return Some("Spawn prefab name cannot be empty".into());
            }
            if prefab.len() > 128 {
                return Some("Spawn prefab name too long".into());
            }
            if !position.is_finite() {
                return Some("Spawn position must be finite".into());
            }
            None
        }
        Action::UseAbility { slot, .. } => {
            if *slot >= 5 {
                return Some("Ability slot out of range".into());
            }
            None
        }
        Action::SummonCompanion { slot } => {
            if *slot >= 6 {
                return Some("Companion slot out of range".into());
            }
            None
        }
        Action::CommandCompanion { slot, target, .. } => {
            if *slot >= 6 {
                return Some("Companion slot out of range".into());
            }
            if let Some(target) = target {
                if target.0 == 0 {
                    return Some("Companion target entity id must be non-zero".into());
                }
            }
            None
        }
        Action::Drop { slot } | Action::UseItem { slot } => {
            if *slot >= 8 {
                return Some("Inventory slot out of range".into());
            }
            None
        }
        Action::CaptureCreature { target, tool_slot } => {
            if target.0 == 0 {
                return Some("Capture target entity id must be non-zero".into());
            }
            if let Some(slot) = tool_slot {
                if *slot >= 28 {
                    return Some("Capture tool slot out of range".into());
                }
            }
            None
        }
        Action::GatherResource { target, .. } | Action::Loot { target } => {
            if target.0 == 0 {
                return Some("Target entity id must be non-zero".into());
            }
            None
        }
        Action::AttackTarget { target }
        | Action::InteractWith { target }
        | Action::Pickup { target } => {
            if target.0 == 0 {
                return Some("Target entity id must be non-zero".into());
            }
            None
        }
        Action::Stop
        | Action::Attack
        | Action::Interact
        | Action::Idle
        | Action::SetAutoRetaliate { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{validate_action, Action, AgentAction, AgentConstraints};
    use crate::id::AgentId;

    fn action(agent_id: u64, action: Action) -> AgentAction {
        AgentAction {
            agent_id: AgentId(uuid::Uuid::from_u128(agent_id as u128)),
            tick: 1,
            action,
        }
    }

    #[test]
    fn validate_action_rejects_invalid_move_vector() {
        let constraints = AgentConstraints::default();
        let invalid = action(
            1,
            Action::Move {
                direction: glam::Vec2::new(f32::NAN, 0.0),
            },
        );
        assert!(matches!(
            validate_action(&invalid, &constraints, 1),
            super::ActionResult::Rejected(_)
        ));
    }

    #[test]
    fn validate_action_rejects_too_fast_speak() {
        let constraints = AgentConstraints::default();
        let invalid = action(
            1,
            Action::Speak {
                message: "x".repeat(1024),
                volume: crate::action::SpeakVolume::Normal,
            },
        );
        assert!(matches!(
            validate_action(&invalid, &constraints, 1),
            super::ActionResult::Rejected(_)
        ));
    }

    #[test]
    fn validate_action_rejects_empty_signal_type() {
        let constraints = AgentConstraints::default();
        let invalid = action(
            1,
            Action::Signal {
                signal_type: String::new(),
                data: String::from("ok"),
            },
        );
        assert!(matches!(
            validate_action(&invalid, &constraints, 1),
            super::ActionResult::Rejected(_)
        ));
    }

    #[test]
    fn validate_action_rejects_invalid_inventory_slot() {
        let constraints = AgentConstraints::default();
        let invalid = action(1, Action::Drop { slot: 9 });
        assert!(matches!(
            validate_action(&invalid, &constraints, 1),
            super::ActionResult::Rejected(_)
        ));
    }

    #[test]
    fn validate_action_rejects_zero_entity_target() {
        let constraints = AgentConstraints::default();
        let invalid = action(
            1,
            Action::AttackTarget {
                target: crate::id::EntityId(0),
            },
        );
        assert!(matches!(
            validate_action(&invalid, &constraints, 1),
            super::ActionResult::Rejected(_)
        ));
    }

    #[test]
    fn validate_action_rejects_invalid_companion_slot() {
        let constraints = AgentConstraints::default();
        let invalid = action(1, Action::SummonCompanion { slot: 9 });
        assert!(matches!(
            validate_action(&invalid, &constraints, 1),
            super::ActionResult::Rejected(_)
        ));
    }

    #[test]
    fn validate_action_rejects_invalid_capture_tool_slot() {
        let constraints = AgentConstraints::default();
        let invalid = action(
            1,
            Action::CaptureCreature {
                target: crate::id::EntityId(4),
                tool_slot: Some(30),
            },
        );
        assert!(matches!(
            validate_action(&invalid, &constraints, 1),
            super::ActionResult::Rejected(_)
        ));
    }

    #[test]
    fn validate_action_rejects_stunned_agent() {
        let mut constraints = AgentConstraints::default();
        constraints.can_act = false;
        let action = action(1, Action::Attack);
        assert!(matches!(
            validate_action(&action, &constraints, 1),
            super::ActionResult::Rejected(_)
        ));
    }

    #[test]
    fn validate_action_accepts_valid_inputs() {
        let constraints = AgentConstraints::default();
        let valid = action(
            1,
            Action::Move {
                direction: glam::Vec2::new(0.5, 0.5),
            },
        );
        assert!(matches!(
            validate_action(&valid, &constraints, 1),
            super::ActionResult::Valid
        ));
    }
}
