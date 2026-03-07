use serde::{Deserialize, Serialize};

use crate::action::AgentAction;
use crate::agent::AgentType;
use crate::id::AgentId;
use crate::observation::Observation;
use crate::telemetry::{TickTelemetryFrame, ToolCallStatus};
use crate::toon::encode_toon_document;

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

/// Versioned description of an embedded tool that an agent may call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub read_only: bool,
    pub budget: ToolBudget,
}

/// Fixed limits applied to tool invocations independent of gameplay actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBudget {
    pub max_calls_per_tick: u32,
    pub max_calls_per_minute: u32,
    pub max_request_units_per_tick: u32,
    pub max_response_units_per_tick: u32,
}

impl ToolBudget {
    pub fn disabled() -> Self {
        Self {
            max_calls_per_tick: 0,
            max_calls_per_minute: 0,
            max_request_units_per_tick: 0,
            max_response_units_per_tick: 0,
        }
    }

    pub fn read_only_default() -> Self {
        Self {
            max_calls_per_tick: 1,
            max_calls_per_minute: 30,
            max_request_units_per_tick: 8_000,
            max_response_units_per_tick: 4_000,
        }
    }
}

impl Default for ToolBudget {
    fn default() -> Self {
        Self::read_only_default()
    }
}

/// Runtime policy for agent-side tool access. Tools may gather data, but any
/// gameplay mutation must still become a normal validated `Action`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolPolicy {
    pub enabled: bool,
    pub allow_external_network: bool,
    pub allow_write_tools: bool,
    pub budget: ToolBudget,
}

impl ToolPolicy {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            allow_external_network: false,
            allow_write_tools: false,
            budget: ToolBudget::disabled(),
        }
    }

    pub fn read_only_default() -> Self {
        Self {
            enabled: true,
            allow_external_network: true,
            allow_write_tools: false,
            budget: ToolBudget::read_only_default(),
        }
    }
}

impl Default for ToolPolicy {
    fn default() -> Self {
        Self::disabled()
    }
}

/// Catalog of embedded tools available to a runtime profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCatalog {
    pub version: RuntimeContractVersion,
    pub tools: Vec<ToolDefinition>,
}

impl ToolCatalog {
    pub fn new(tools: Vec<ToolDefinition>) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            tools,
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("tool_catalog", self)
    }
}

/// Request emitted by an embedded tool runtime before a side effect occurs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInvocationRequest {
    pub tick: u64,
    pub agent_id: AgentId,
    pub tool_name: String,
    pub provider: String,
    pub request_units: u32,
    pub arguments_json: String,
}

/// Result emitted by the embedded tool runtime after completion or failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInvocationResult {
    pub tick: u64,
    pub agent_id: AgentId,
    pub tool_name: String,
    pub provider: String,
    pub status: ToolCallStatus,
    pub latency_ms: u32,
    pub request_units: u32,
    pub response_units: u32,
    pub output_json: Option<String>,
    pub error_message: Option<String>,
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

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("versioned_agent_action", self)
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

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("versioned_observation", self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedTickTelemetry {
    pub version: RuntimeContractVersion,
    pub payload: TickTelemetryFrame,
}

impl VersionedTickTelemetry {
    pub fn new(payload: TickTelemetryFrame) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            payload,
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("versioned_tick_telemetry", self)
    }
}

impl ToolDefinition {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("tool_definition", self)
    }
}

impl ToolBudget {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("tool_budget", self)
    }
}

impl ToolPolicy {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("tool_policy", self)
    }
}

impl ToolInvocationRequest {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("tool_invocation_request", self)
    }
}

impl ToolInvocationResult {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("tool_invocation_result", self)
    }
}

#[cfg(test)]
mod tests {
    use crate::action::{Action, AgentAction};
    use crate::agent::AgentType;
    use crate::id::AgentId;
    use crate::observation::Observation;
    use crate::telemetry::{TickTelemetryFrame, ToolCallStatus};
    use crate::toon::decode_toon_value;

    use super::{
        AgentCapabilities, AgentRole, AgentRuntimeProfile, RuntimeContractVersion, ToolBudget,
        ToolCatalog, ToolDefinition, ToolInvocationRequest, ToolInvocationResult, ToolPolicy,
        VersionedAgentAction, VersionedObservation, VersionedTickTelemetry,
        RUNTIME_CONTRACT_VERSION_V1,
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

        let telemetry = TickTelemetryFrame::empty(9);
        let versioned_telemetry = VersionedTickTelemetry::new(telemetry.clone());
        assert_eq!(versioned_telemetry.payload.tick, telemetry.tick);
    }

    #[test]
    fn tool_contract_defaults_are_read_only_and_disabled_by_default() {
        let budget = ToolBudget::default();
        assert_eq!(budget.max_calls_per_tick, 1);
        assert_eq!(budget.max_calls_per_minute, 30);

        let policy = ToolPolicy::default();
        assert!(!policy.enabled);
        assert_eq!(policy.budget, ToolBudget::disabled());
    }

    #[test]
    fn tool_contracts_serialize_runtime_invocations_without_losing_status() {
        let catalog = ToolCatalog::new(vec![ToolDefinition {
            name: "llm.complete".into(),
            description: "Request a provider completion".into(),
            read_only: true,
            budget: ToolBudget::read_only_default(),
        }]);
        assert_eq!(catalog.version, RuntimeContractVersion::V1);
        assert_eq!(catalog.tools.len(), 1);

        let request = ToolInvocationRequest {
            tick: 12,
            agent_id: AgentId::new(),
            tool_name: "llm.complete".into(),
            provider: "openai-compatible".into(),
            request_units: 320,
            arguments_json: "{\"prompt\":\"hi\"}".into(),
        };
        let result = ToolInvocationResult {
            tick: request.tick,
            agent_id: request.agent_id,
            tool_name: request.tool_name.clone(),
            provider: request.provider.clone(),
            status: ToolCallStatus::RateLimited,
            latency_ms: 1500,
            request_units: request.request_units,
            response_units: 0,
            output_json: None,
            error_message: Some("retry after 1000ms".into()),
        };

        let encoded = serde_json::to_string(&result).expect("tool result should serialize");
        let decoded: ToolInvocationResult =
            serde_json::from_str(&encoded).expect("tool result should deserialize");
        assert_eq!(decoded.status, ToolCallStatus::RateLimited);
        assert_eq!(decoded.request_units, 320);
        assert!(decoded.output_json.is_none());
    }

    #[test]
    fn tool_contracts_export_to_toon_documents() {
        let request = ToolInvocationRequest {
            tick: 12,
            agent_id: AgentId::new(),
            tool_name: "llm.complete".into(),
            provider: "qwen".into(),
            request_units: 320,
            arguments_json: "{\"prompt\":\"hi\"}".into(),
        };
        let request_document = request.to_toon_document();
        let request_value =
            decode_toon_value(&request_document).expect("request document should decode");
        assert_eq!(request_value["document_type"], "tool_invocation_request");
        assert_eq!(request_value["payload"]["provider"], "qwen");

        let telemetry = VersionedTickTelemetry::new(TickTelemetryFrame::empty(9));
        let telemetry_document = telemetry.to_toon_document();
        let telemetry_value =
            decode_toon_value(&telemetry_document).expect("telemetry document should decode");
        assert_eq!(telemetry_value["document_type"], "versioned_tick_telemetry");
        assert_eq!(telemetry_value["payload"]["payload"]["tick"], 9);
    }
}
