use serde::{Deserialize, Serialize};

use crate::action::AgentAction;
use crate::agent::AgentType;
use crate::observation::Observation;

pub const RUNTIME_CONTRACT_VERSION_V1: u16 = 1;

/// Semantic version identifier for runtime contracts exchanged between
/// human clients, local AI, and remote AI connectors.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RuntimeContractVersion {
    #[default]
    V1,
}

impl RuntimeContractVersion {
    pub fn as_u16(self) -> u16 {
        match self {
            Self::V1 => RUNTIME_CONTRACT_VERSION_V1,
        }
    }
}

/// Role is explicit and auditable. Permissions flow from capabilities,
/// not from hidden engine-side branches.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentRole {
    #[default]
    Player,
    Npc,
    Companion,
    WorldSystem,
}

/// Capability gates sit above the shared action schema so humans and AI still
/// speak the same language even when role permissions differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCapabilities {
    pub can_combat: bool,
    pub can_trade: bool,
    pub can_join_party: bool,
    pub can_capture_creatures: bool,
    pub can_command_companions: bool,
    pub can_spawn_world_entities: bool,
}

impl AgentCapabilities {
    pub fn player_default() -> Self {
        Self {
            can_combat: true,
            can_trade: true,
            can_join_party: true,
            can_capture_creatures: true,
            can_command_companions: true,
            can_spawn_world_entities: false,
        }
    }

    pub fn npc_default() -> Self {
        Self {
            can_combat: true,
            can_trade: false,
            can_join_party: false,
            can_capture_creatures: false,
            can_command_companions: false,
            can_spawn_world_entities: false,
        }
    }

    pub fn companion_default() -> Self {
        Self {
            can_combat: true,
            can_trade: false,
            can_join_party: false,
            can_capture_creatures: false,
            can_command_companions: false,
            can_spawn_world_entities: false,
        }
    }

    pub fn system_default() -> Self {
        Self {
            can_combat: false,
            can_trade: false,
            can_join_party: false,
            can_capture_creatures: false,
            can_command_companions: true,
            can_spawn_world_entities: true,
        }
    }
}

impl Default for AgentCapabilities {
    fn default() -> Self {
        Self::player_default()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRuntimeProfile {
    pub role: AgentRole,
    pub agent_type: AgentType,
    pub capabilities: AgentCapabilities,
}

impl AgentRuntimeProfile {
    pub fn for_agent_type(agent_type: AgentType) -> Self {
        match agent_type {
            AgentType::Human | AgentType::LlmAgent | AgentType::NeuralAgent => Self {
                role: AgentRole::Player,
                agent_type,
                capabilities: AgentCapabilities::player_default(),
            },
            AgentType::ScriptedNpc => Self {
                role: AgentRole::Npc,
                agent_type,
                capabilities: AgentCapabilities::npc_default(),
            },
            AgentType::System => Self {
                role: AgentRole::WorldSystem,
                agent_type,
                capabilities: AgentCapabilities::system_default(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedAgentAction {
    pub version: RuntimeContractVersion,
    pub profile: AgentRuntimeProfile,
    pub payload: AgentAction,
}

impl VersionedAgentAction {
    pub fn new(profile: AgentRuntimeProfile, payload: AgentAction) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            profile,
            payload,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedObservation {
    pub version: RuntimeContractVersion,
    pub profile: AgentRuntimeProfile,
    pub payload: Observation,
}

impl VersionedObservation {
    pub fn new(profile: AgentRuntimeProfile, payload: Observation) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            profile,
            payload,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::action::{Action, AgentAction};
    use crate::agent::AgentType;
    use crate::id::AgentId;
    use crate::observation::Observation;

    use super::{
        AgentCapabilities, AgentRole, AgentRuntimeProfile, RuntimeContractVersion,
        VersionedAgentAction, VersionedObservation, RUNTIME_CONTRACT_VERSION_V1,
    };

    #[test]
    fn runtime_version_maps_to_wire_number() {
        assert_eq!(
            RuntimeContractVersion::V1.as_u16(),
            RUNTIME_CONTRACT_VERSION_V1
        );
    }

    #[test]
    fn runtime_profile_defaults_match_agent_type() {
        let player = AgentRuntimeProfile::for_agent_type(AgentType::Human);
        assert_eq!(player.role, AgentRole::Player);
        assert!(player.capabilities.can_capture_creatures);

        let npc = AgentRuntimeProfile::for_agent_type(AgentType::ScriptedNpc);
        assert_eq!(npc.role, AgentRole::Npc);
        assert!(!npc.capabilities.can_trade);

        let system = AgentRuntimeProfile::for_agent_type(AgentType::System);
        assert_eq!(system.role, AgentRole::WorldSystem);
        assert!(system.capabilities.can_spawn_world_entities);
    }

    #[test]
    fn versioned_contracts_wrap_payloads_without_mutating_them() {
        let profile = AgentRuntimeProfile {
            role: AgentRole::Player,
            agent_type: AgentType::Human,
            capabilities: AgentCapabilities::player_default(),
        };
        let action = AgentAction {
            agent_id: AgentId::new(),
            tick: 7,
            action: Action::Idle,
        };
        let versioned_action = VersionedAgentAction::new(profile, action.clone());
        assert_eq!(versioned_action.payload.tick, 7);

        let observation = Observation::default();
        let versioned_observation = VersionedObservation::new(profile, observation.clone());
        assert_eq!(versioned_observation.payload.tick, observation.tick);
    }
}
