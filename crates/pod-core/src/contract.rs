use std::collections::{BTreeMap, BTreeSet};

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

pub const REMOTE_AGENT_MAX_VISIBLE_ENTITIES: usize = 10;
pub const REMOTE_AGENT_MAX_AUDIBLE_EVENTS: usize = 10;
pub const REMOTE_AGENT_MAX_ACTIONS_PER_TICK: u8 = 3;
pub const REMOTE_AGENT_OBSERVATION_STALE_AFTER_TICKS: u64 = 2;
pub const REMOTE_AGENT_TIMEOUT_AFTER_TICKS: u64 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAgentObservationBudget {
    pub max_visible_entities: u32,
    pub max_audible_events: u32,
    pub stale_after_ticks: u64,
    pub timeout_after_ticks: u64,
}

impl RemoteAgentObservationBudget {
    pub fn spacetimedb_default() -> Self {
        Self {
            max_visible_entities: REMOTE_AGENT_MAX_VISIBLE_ENTITIES as u32,
            max_audible_events: REMOTE_AGENT_MAX_AUDIBLE_EVENTS as u32,
            stale_after_ticks: REMOTE_AGENT_OBSERVATION_STALE_AFTER_TICKS,
            timeout_after_ticks: REMOTE_AGENT_TIMEOUT_AFTER_TICKS,
        }
    }
}

impl Default for RemoteAgentObservationBudget {
    fn default() -> Self {
        Self::spacetimedb_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAgentActionBudget {
    pub max_actions_per_tick: u32,
}

impl RemoteAgentActionBudget {
    pub fn spacetimedb_default() -> Self {
        Self {
            max_actions_per_tick: u32::from(REMOTE_AGENT_MAX_ACTIONS_PER_TICK),
        }
    }
}

impl Default for RemoteAgentActionBudget {
    fn default() -> Self {
        Self::spacetimedb_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAgentHeartbeatPolicy {
    pub stale_after_ticks: u64,
    pub timeout_after_ticks: u64,
}

impl RemoteAgentHeartbeatPolicy {
    pub fn spacetimedb_default() -> Self {
        Self {
            stale_after_ticks: REMOTE_AGENT_OBSERVATION_STALE_AFTER_TICKS,
            timeout_after_ticks: REMOTE_AGENT_TIMEOUT_AFTER_TICKS,
        }
    }
}

impl Default for RemoteAgentHeartbeatPolicy {
    fn default() -> Self {
        Self::spacetimedb_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteAgentFallbackMode {
    RejectActionsUntilFreshObservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteAgentFallbackReason {
    ObservationMissing,
    ObservationStale,
    HeartbeatTimedOut,
    ActionBudgetExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAgentTransportContract {
    pub version: RuntimeContractVersion,
    pub profile: AgentRuntimeProfile,
    pub observation_budget: RemoteAgentObservationBudget,
    pub action_budget: RemoteAgentActionBudget,
    pub heartbeat: RemoteAgentHeartbeatPolicy,
    pub fallback_mode: RemoteAgentFallbackMode,
}

impl RemoteAgentTransportContract {
    pub fn spacetimedb_default(profile: AgentRuntimeProfile) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            profile,
            observation_budget: RemoteAgentObservationBudget::spacetimedb_default(),
            action_budget: RemoteAgentActionBudget::spacetimedb_default(),
            heartbeat: RemoteAgentHeartbeatPolicy::spacetimedb_default(),
            fallback_mode: RemoteAgentFallbackMode::RejectActionsUntilFreshObservation,
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("remote_agent_transport_contract", self)
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteAgentRuntimeStatus {
    pub fallback_active: bool,
    pub fallback_reason: Option<RemoteAgentFallbackReason>,
    pub last_observation_tick: Option<u64>,
    pub last_authoritative_tick: Option<u64>,
    pub stale_observation_ticks: u64,
    pub pending_action_count: u32,
    pub stale_action_rejections: u64,
    pub budget_overflow_rejections: u64,
    pub timeout_rejections: u64,
}

impl RemoteAgentRuntimeStatus {
    pub fn clear_fallback(&mut self) {
        self.fallback_active = false;
        self.fallback_reason = None;
    }

    pub fn activate_fallback(&mut self, reason: RemoteAgentFallbackReason) {
        self.fallback_active = true;
        self.fallback_reason = Some(reason);
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("remote_agent_runtime_status", self)
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

/// One authored stage inside a quest-state graph for creator tooling, AI
/// planning, and editor/runtime exports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestStageDefinition {
    pub stage_id: String,
    pub title: String,
    pub objectives: Vec<String>,
    pub next_stage_ids: Vec<String>,
    pub reward_tags: Vec<String>,
}

/// Native quest graph contract shared by editor, runtime exports, and agent
/// tooling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestStateGraph {
    pub version: RuntimeContractVersion,
    pub quest_id: String,
    pub display_name: String,
    pub start_stage_id: String,
    pub repeatable: bool,
    pub stages: Vec<QuestStageDefinition>,
}

impl QuestStateGraph {
    pub fn new(
        quest_id: impl Into<String>,
        display_name: impl Into<String>,
        start_stage_id: impl Into<String>,
        stages: Vec<QuestStageDefinition>,
    ) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            quest_id: quest_id.into(),
            display_name: display_name.into(),
            start_stage_id: start_stage_id.into(),
            repeatable: false,
            stages,
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("quest_state_graph", self)
    }
}

/// One score threshold in a faction reputation track.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionReputationTier {
    pub tier_id: String,
    pub label: String,
    pub minimum_score: i32,
    pub perk_tags: Vec<String>,
}

/// Native faction reputation progression definition for authored worlds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactionReputationTrack {
    pub version: RuntimeContractVersion,
    pub faction_id: String,
    pub display_name: String,
    pub starting_score: i32,
    pub hostile_below: i32,
    pub allied_at: i32,
    pub tiers: Vec<FactionReputationTier>,
}

impl FactionReputationTrack {
    pub fn new(
        faction_id: impl Into<String>,
        display_name: impl Into<String>,
        tiers: Vec<FactionReputationTier>,
    ) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            faction_id: faction_id.into(),
            display_name: display_name.into(),
            starting_score: 0,
            hostile_below: -25,
            allied_at: 50,
            tiers,
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("faction_reputation_track", self)
    }
}

/// One weighted spawn entry inside a regional encounter table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncounterSpawnEntry {
    pub archetype_id: String,
    pub weight: u16,
    pub min_count: u8,
    pub max_count: u8,
    pub required_stage_tags: Vec<String>,
    pub required_reputation_tiers: Vec<String>,
}

/// Native encounter-table contract for authored streamed regions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionEncounterTable {
    pub version: RuntimeContractVersion,
    pub table_id: String,
    pub biome_id: String,
    pub spawn_group: String,
    pub ambient_cap: u16,
    pub entries: Vec<EncounterSpawnEntry>,
}

impl RegionEncounterTable {
    pub fn new(
        table_id: impl Into<String>,
        biome_id: impl Into<String>,
        spawn_group: impl Into<String>,
        entries: Vec<EncounterSpawnEntry>,
    ) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            table_id: table_id.into(),
            biome_id: biome_id.into(),
            spawn_group: spawn_group.into(),
            ambient_cap: 12,
            entries,
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("region_encounter_table", self)
    }
}

/// Streamed chunk definition that links authored content, quest graphs, and
/// encounter tables together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldChunkDefinition {
    pub version: RuntimeContractVersion,
    pub chunk_key: String,
    pub region_id: String,
    pub biome_id: String,
    pub quest_graph_ids: Vec<String>,
    pub faction_track_ids: Vec<String>,
    pub encounter_table_ids: Vec<String>,
    pub neighbor_chunk_keys: Vec<String>,
}

impl WorldChunkDefinition {
    pub fn new(
        chunk_key: impl Into<String>,
        region_id: impl Into<String>,
        biome_id: impl Into<String>,
    ) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            chunk_key: chunk_key.into(),
            region_id: region_id.into(),
            biome_id: biome_id.into(),
            quest_graph_ids: Vec::new(),
            faction_track_ids: Vec::new(),
            encounter_table_ids: Vec::new(),
            neighbor_chunk_keys: Vec::new(),
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("world_chunk_definition", self)
    }
}

/// Region-level grouping for multiple streamed chunks and their dominant
/// authored progression hooks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRegionDefinition {
    pub version: RuntimeContractVersion,
    pub region_id: String,
    pub display_name: String,
    pub primary_biome_id: String,
    pub chunk_keys: Vec<String>,
    pub active_quest_graph_ids: Vec<String>,
    pub dominant_faction_track_id: String,
    pub encounter_table_ids: Vec<String>,
}

impl WorldRegionDefinition {
    pub fn new(
        region_id: impl Into<String>,
        display_name: impl Into<String>,
        primary_biome_id: impl Into<String>,
    ) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            region_id: region_id.into(),
            display_name: display_name.into(),
            primary_biome_id: primary_biome_id.into(),
            chunk_keys: Vec::new(),
            active_quest_graph_ids: Vec::new(),
            dominant_faction_track_id: String::new(),
            encounter_table_ids: Vec::new(),
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("world_region_definition", self)
    }
}

/// How a team of agents is commanded across one or more worlds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeamControlMode {
    DeveloperCaptain,
    SharedOperators,
    AutonomousSwarm,
    HybridCommand,
}

/// Team definition for developer-controlled squads, guilds, or swarms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTeamDefinition {
    pub version: RuntimeContractVersion,
    pub team_id: String,
    pub display_name: String,
    pub control_mode: TeamControlMode,
    pub max_agents: u16,
    pub home_world_id: String,
    pub allowed_world_ids: Vec<String>,
    pub objective_tags: Vec<String>,
}

impl AgentTeamDefinition {
    pub fn new(
        team_id: impl Into<String>,
        display_name: impl Into<String>,
        home_world_id: impl Into<String>,
    ) -> Self {
        let home_world_id = home_world_id.into();
        Self {
            version: RuntimeContractVersion::V1,
            team_id: team_id.into(),
            display_name: display_name.into(),
            control_mode: TeamControlMode::DeveloperCaptain,
            max_agents: 10,
            home_world_id: home_world_id.clone(),
            allowed_world_ids: vec![home_world_id],
            objective_tags: Vec::new(),
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("agent_team_definition", self)
    }
}

/// The role a world plays inside a connected world matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WorldRealityRole {
    Primary,
    Mirror,
    Tournament,
    Sanctuary,
    Shadow,
}

/// A single world or shard that can participate in cross-world influence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldRealityDefinition {
    pub version: RuntimeContractVersion,
    pub world_id: String,
    pub display_name: String,
    pub ruleset_id: String,
    pub role: WorldRealityRole,
    pub linked_world_ids: Vec<String>,
    pub active_team_ids: Vec<String>,
}

impl WorldRealityDefinition {
    pub fn new(
        world_id: impl Into<String>,
        display_name: impl Into<String>,
        ruleset_id: impl Into<String>,
    ) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            world_id: world_id.into(),
            display_name: display_name.into(),
            ruleset_id: ruleset_id.into(),
            role: WorldRealityRole::Primary,
            linked_world_ids: Vec::new(),
            active_team_ids: Vec::new(),
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("world_reality_definition", self)
    }
}

/// How effects propagate from one world to another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrossWorldPropagation {
    Immediate,
    Delayed { ticks: u32 },
    Threshold { required_triggers: u16 },
    Scaled { basis_points: u16 },
}

/// Canonical cross-world consequences. These effects are applied by authority
/// in the target world instead of bypassing the action pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CrossWorldEffect {
    FactionReputationDelta {
        faction_id: String,
        delta: i32,
    },
    EncounterWeightDelta {
        table_id: String,
        delta: i16,
    },
    ResourceScarcityDelta {
        biome_id: String,
        delta: i16,
    },
    TeamScoreDelta {
        team_id: String,
        delta: i32,
    },
    DeathMark {
        team_id: String,
        duration_ticks: u32,
    },
    ObjectiveStateShift {
        quest_graph_id: String,
        stage_tag: String,
    },
}

/// A link between two worlds that turns one world's decisions into authored
/// consequences in another reality.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossWorldLinkDefinition {
    pub version: RuntimeContractVersion,
    pub link_id: String,
    pub source_world_id: String,
    pub target_world_id: String,
    pub trigger_tags: Vec<String>,
    pub propagation: CrossWorldPropagation,
    pub effects: Vec<CrossWorldEffect>,
    pub cooldown_ticks: u32,
    pub max_applications_per_window: u16,
}

impl CrossWorldLinkDefinition {
    pub fn new(
        link_id: impl Into<String>,
        source_world_id: impl Into<String>,
        target_world_id: impl Into<String>,
    ) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            link_id: link_id.into(),
            source_world_id: source_world_id.into(),
            target_world_id: target_world_id.into(),
            trigger_tags: Vec::new(),
            propagation: CrossWorldPropagation::Immediate,
            effects: Vec::new(),
            cooldown_ticks: 0,
            max_applications_per_window: 1,
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("cross_world_link_definition", self)
    }
}

/// Tournament rules for multi-world elimination or score-attack formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TournamentEliminationMode {
    Permadeath,
    Seasonal,
    ScoreAttack,
    Extraction,
}

/// Top-level tournament contract for developer-run teams across multiple
/// linked worlds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldTournamentDefinition {
    pub version: RuntimeContractVersion,
    pub tournament_id: String,
    pub display_name: String,
    pub world_ids: Vec<String>,
    pub team_ids: Vec<String>,
    pub cross_world_link_ids: Vec<String>,
    pub max_agents_per_team: u16,
    pub elimination_mode: TournamentEliminationMode,
    pub reward_tags: Vec<String>,
}

impl WorldTournamentDefinition {
    pub fn new(tournament_id: impl Into<String>, display_name: impl Into<String>) -> Self {
        Self {
            version: RuntimeContractVersion::V1,
            tournament_id: tournament_id.into(),
            display_name: display_name.into(),
            world_ids: Vec::new(),
            team_ids: Vec::new(),
            cross_world_link_ids: Vec::new(),
            max_agents_per_team: 10,
            elimination_mode: TournamentEliminationMode::Permadeath,
            reward_tags: Vec::new(),
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("world_tournament_definition", self)
    }
}

/// Explicit world-to-quest attachment used by remote topology consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldQuestBinding {
    pub world_id: String,
    pub quest_graph_ids: Vec<String>,
}

impl WorldQuestBinding {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("world_quest_binding", self)
    }
}

pub fn build_world_quest_bindings(
    world_quest_graph_ids: &BTreeMap<String, Vec<String>>,
) -> Vec<WorldQuestBinding> {
    world_quest_graph_ids
        .iter()
        .map(|(world_id, quest_graph_ids)| WorldQuestBinding {
            world_id: world_id.clone(),
            quest_graph_ids: quest_graph_ids.clone(),
        })
        .collect()
}

/// Admission of one agent into one world-scoped team slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldAdmissionAssignment {
    pub agent_id: String,
    pub team_id: String,
    pub slot_index: u16,
}

impl WorldAdmissionAssignment {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("world_admission_assignment", self)
    }
}

/// Deterministic roster admission for one world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldAdmissionSummary {
    pub world_id: String,
    pub assignments: Vec<WorldAdmissionAssignment>,
}

impl WorldAdmissionSummary {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("world_admission_summary", self)
    }
}

/// Deterministic count rollup keyed by admitted controller type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTypeCountSummary {
    pub agent_type: String,
    pub count: usize,
}

/// Runtime-visible slot assignment for one admitted agent in one world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldControlAssignmentSummary {
    pub agent_id: String,
    pub slot_index: u16,
    pub runtime_profile: AgentRuntimeProfile,
}

/// Team-scoped control plane snapshot for one world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldTeamControlSummary {
    pub team_id: String,
    pub assignments: Vec<WorldControlAssignmentSummary>,
    pub controller_mix: Vec<AgentTypeCountSummary>,
}

/// Explicit world-scoped roster/control snapshot derived from admissions plus
/// the actual runtime profiles present in the authoritative run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldControlPlaneSummary {
    pub world_id: String,
    pub teams: Vec<WorldTeamControlSummary>,
}

impl WorldControlPlaneSummary {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("world_control_plane_summary", self)
    }
}

fn agent_type_key(agent_type: AgentType) -> &'static str {
    match agent_type {
        AgentType::Human => "human",
        AgentType::LlmAgent => "llm_agent",
        AgentType::NeuralAgent => "neural_agent",
        AgentType::ScriptedNpc => "scripted_npc",
        AgentType::System => "system",
    }
}

pub fn build_world_control_plane_summary(
    admissions: &WorldAdmissionSummary,
    runtime_profiles: &BTreeMap<String, AgentRuntimeProfile>,
) -> WorldControlPlaneSummary {
    let mut teams: Vec<WorldTeamControlSummary> = admissions
        .assignments
        .iter()
        .fold(
            BTreeMap::<String, Vec<WorldControlAssignmentSummary>>::new(),
            |mut by_team, assignment| {
                by_team.entry(assignment.team_id.clone()).or_default().push(
                    WorldControlAssignmentSummary {
                        agent_id: assignment.agent_id.clone(),
                        slot_index: assignment.slot_index,
                        runtime_profile: runtime_profiles
                            .get(&assignment.agent_id)
                            .copied()
                            .unwrap_or_default(),
                    },
                );
                by_team
            },
        )
        .into_iter()
        .map(|(team_id, mut assignments)| {
            assignments.sort_by(|left, right| {
                left.slot_index
                    .cmp(&right.slot_index)
                    .then_with(|| left.agent_id.cmp(&right.agent_id))
            });
            let mut controller_mix = assignments.iter().fold(
                BTreeMap::<String, usize>::new(),
                |mut by_type, assignment| {
                    *by_type
                        .entry(agent_type_key(assignment.runtime_profile.agent_type).to_string())
                        .or_default() += 1;
                    by_type
                },
            );
            let controller_mix = controller_mix
                .iter_mut()
                .map(|(agent_type, count)| AgentTypeCountSummary {
                    agent_type: agent_type.clone(),
                    count: *count,
                })
                .collect();
            WorldTeamControlSummary {
                team_id,
                assignments,
                controller_mix,
            }
        })
        .collect();

    teams.sort_by(
        |left: &WorldTeamControlSummary, right: &WorldTeamControlSummary| {
            left.team_id.cmp(&right.team_id)
        },
    );

    WorldControlPlaneSummary {
        world_id: admissions.world_id.clone(),
        teams,
    }
}

/// Shared reward ledger for one team across replay-derived dataset rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TeamRewardLedgerSummary {
    pub team_id: String,
    pub dataset_row_count: usize,
    pub world_reward_total: f32,
}

/// Shared tournament-facing standing summary for one team.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TournamentTeamStandingSummary {
    pub team_id: String,
    pub display_name: String,
    pub control_mode: TeamControlMode,
    pub home_world_id: String,
    pub participating_world_ids: Vec<String>,
    pub assigned_agent_count: usize,
    pub controller_mix: Vec<AgentTypeCountSummary>,
    pub dataset_row_count: usize,
    pub world_reward_total: f32,
    pub applied_score_delta: i32,
    pub active_death_marks: usize,
    pub active_death_mark_ticks: u64,
}

/// Tournament-scoped control-plane rollup derived from shared admissions,
/// per-world control planes, reward ledgers, and applied world state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TournamentControlPlaneSummary {
    pub tournament_id: String,
    pub standings: Vec<TournamentTeamStandingSummary>,
}

impl TournamentControlPlaneSummary {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("tournament_control_plane_summary", self)
    }
}

impl Default for TournamentControlPlaneSummary {
    fn default() -> Self {
        Self {
            tournament_id: String::new(),
            standings: Vec::new(),
        }
    }
}

/// Shared tournament phase derived from world pressure and elimination state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TournamentOrchestrationPhase {
    Muster,
    Active,
    SuddenDeath,
    Resolved,
}

/// World-scoped tournament orchestration state derived from control-plane,
/// link, and applied-effect summaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldTournamentOrchestrationSummary {
    pub world_id: String,
    pub display_name: String,
    pub role: WorldRealityRole,
    pub active_team_ids: Vec<String>,
    pub linked_world_ids: Vec<String>,
    pub active_link_ids: Vec<String>,
    pub assigned_agent_count: usize,
    pub controller_mix: Vec<AgentTypeCountSummary>,
    pub applied_score_delta_total: i32,
    pub applied_death_mark_count: usize,
    pub applied_death_mark_ticks: u64,
    pub objective_shift_count: usize,
    pub unresolved_objective_shift_count: usize,
    pub progressed_quest_line_count: usize,
    pub terminal_quest_line_count: usize,
    pub leading_team_ids: Vec<String>,
    pub at_risk_team_ids: Vec<String>,
}

/// Shared tournament orchestration rollup derived from the authoritative
/// control plane, world links, and applied world-state summaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TournamentOrchestrationSummary {
    pub tournament_id: String,
    pub phase: TournamentOrchestrationPhase,
    pub active_world_ids: Vec<String>,
    pub contested_world_ids: Vec<String>,
    pub active_link_ids: Vec<String>,
    pub leading_team_ids: Vec<String>,
    pub at_risk_team_ids: Vec<String>,
    pub worlds: Vec<WorldTournamentOrchestrationSummary>,
}

impl TournamentOrchestrationSummary {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("tournament_orchestration_summary", self)
    }
}

impl Default for TournamentOrchestrationSummary {
    fn default() -> Self {
        Self {
            tournament_id: String::new(),
            phase: TournamentOrchestrationPhase::Muster,
            active_world_ids: Vec::new(),
            contested_world_ids: Vec::new(),
            active_link_ids: Vec::new(),
            leading_team_ids: Vec::new(),
            at_risk_team_ids: Vec::new(),
            worlds: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
struct TournamentStandingAccumulator {
    assigned_agents: BTreeSet<String>,
    runtime_profiles_by_agent: BTreeMap<String, AgentRuntimeProfile>,
    dataset_row_count: usize,
    world_reward_total: f32,
    applied_score_delta: i32,
    active_death_marks: usize,
    active_death_mark_ticks: u64,
}

pub fn build_tournament_control_plane_summary(
    tournament: &WorldTournamentDefinition,
    teams: &[AgentTeamDefinition],
    worlds: &[WorldRealityDefinition],
    world_control_planes: &[WorldControlPlaneSummary],
    reward_ledgers: &[TeamRewardLedgerSummary],
    applied_world_states: &[AppliedWorldStateSummary],
) -> TournamentControlPlaneSummary {
    let mut by_team = BTreeMap::<String, TournamentStandingAccumulator>::new();
    for world_control_plane in world_control_planes {
        for team in &world_control_plane.teams {
            let entry = by_team.entry(team.team_id.clone()).or_default();
            for assignment in &team.assignments {
                entry.assigned_agents.insert(assignment.agent_id.clone());
                entry
                    .runtime_profiles_by_agent
                    .entry(assignment.agent_id.clone())
                    .or_insert(assignment.runtime_profile);
            }
        }
    }
    for ledger in reward_ledgers {
        let entry = by_team.entry(ledger.team_id.clone()).or_default();
        entry.dataset_row_count += ledger.dataset_row_count;
        entry.world_reward_total += ledger.world_reward_total;
    }
    for state in applied_world_states {
        for team_score in &state.team_scores {
            by_team
                .entry(team_score.team_id.clone())
                .or_default()
                .applied_score_delta += team_score.total_delta;
        }
        for death_mark in &state.death_marks {
            let entry = by_team.entry(death_mark.team_id.clone()).or_default();
            entry.active_death_marks += death_mark.applications;
            entry.active_death_mark_ticks += death_mark.total_duration_ticks;
        }
    }

    let standings = teams
        .iter()
        .map(|team| {
            let totals = by_team.remove(&team.team_id).unwrap_or_default();
            let mut controller_mix = totals.runtime_profiles_by_agent.values().fold(
                BTreeMap::<String, usize>::new(),
                |mut by_type, runtime_profile| {
                    *by_type
                        .entry(agent_type_key(runtime_profile.agent_type).to_string())
                        .or_default() += 1;
                    by_type
                },
            );
            let participating_world_ids = worlds
                .iter()
                .filter(|world| {
                    world
                        .active_team_ids
                        .iter()
                        .any(|team_id| team_id == &team.team_id)
                })
                .map(|world| world.world_id.clone())
                .collect();

            TournamentTeamStandingSummary {
                team_id: team.team_id.clone(),
                display_name: team.display_name.clone(),
                control_mode: team.control_mode,
                home_world_id: team.home_world_id.clone(),
                participating_world_ids,
                assigned_agent_count: totals.assigned_agents.len(),
                controller_mix: controller_mix
                    .iter_mut()
                    .map(|(agent_type, count)| AgentTypeCountSummary {
                        agent_type: agent_type.clone(),
                        count: *count,
                    })
                    .collect(),
                dataset_row_count: totals.dataset_row_count,
                world_reward_total: totals.world_reward_total,
                applied_score_delta: totals.applied_score_delta,
                active_death_marks: totals.active_death_marks,
                active_death_mark_ticks: totals.active_death_mark_ticks,
            }
        })
        .collect();

    TournamentControlPlaneSummary {
        tournament_id: tournament.tournament_id.clone(),
        standings,
    }
}

pub fn build_tournament_orchestration_summary(
    tournament: &WorldTournamentDefinition,
    worlds: &[WorldRealityDefinition],
    links: &[CrossWorldLinkDefinition],
    world_control_planes: &[WorldControlPlaneSummary],
    tournament_control_plane: &TournamentControlPlaneSummary,
    applied_world_states: &[AppliedWorldStateSummary],
) -> TournamentOrchestrationSummary {
    let tournament_world_ids = if tournament.world_ids.is_empty() {
        worlds
            .iter()
            .map(|world| world.world_id.clone())
            .collect::<BTreeSet<_>>()
    } else {
        tournament.world_ids.iter().cloned().collect::<BTreeSet<_>>()
    };
    let tournament_link_ids = if tournament.cross_world_link_ids.is_empty() {
        links.iter()
            .map(|link| link.link_id.clone())
            .collect::<BTreeSet<_>>()
    } else {
        tournament
            .cross_world_link_ids
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
    };
    let standings_by_team = tournament_control_plane
        .standings
        .iter()
        .map(|standing| (standing.team_id.clone(), standing))
        .collect::<BTreeMap<_, _>>();

    let worlds = worlds
        .iter()
        .filter(|world| tournament_world_ids.contains(&world.world_id))
        .map(|world| {
            let control_plane = world_control_planes
                .iter()
                .find(|summary| summary.world_id == world.world_id);
            let applied_world_state = applied_world_states
                .iter()
                .find(|summary| summary.world_id == world.world_id);

            let mut controller_mix = BTreeMap::<String, usize>::new();
            let assigned_agent_count = control_plane
                .map(|summary| {
                    summary
                        .teams
                        .iter()
                        .map(|team| {
                            for count in &team.controller_mix {
                                *controller_mix
                                    .entry(count.agent_type.clone())
                                    .or_default() += count.count;
                            }
                            team.assignments.len()
                        })
                        .sum()
                })
                .unwrap_or(0);
            let controller_mix = controller_mix
                .into_iter()
                .map(|(agent_type, count)| AgentTypeCountSummary { agent_type, count })
                .collect::<Vec<_>>();

            let active_link_ids = links
                .iter()
                .filter(|link| tournament_link_ids.contains(&link.link_id))
                .filter(|link| {
                    link.source_world_id == world.world_id || link.target_world_id == world.world_id
                })
                .map(|link| link.link_id.clone())
                .collect::<Vec<_>>();

            let applied_score_delta_total = applied_world_state
                .map(|state| {
                    state
                        .team_scores
                        .iter()
                        .map(|summary| summary.total_delta)
                        .sum()
                })
                .unwrap_or(0);
            let applied_death_mark_count = applied_world_state
                .map(|state| state.death_marks.iter().map(|summary| summary.applications).sum())
                .unwrap_or(0);
            let applied_death_mark_ticks = applied_world_state
                .map(|state| {
                    state
                        .death_marks
                        .iter()
                        .map(|summary| summary.total_duration_ticks)
                        .sum()
                })
                .unwrap_or(0);
            let objective_shift_count = applied_world_state
                .map(|state| {
                    state
                        .objective_state_shifts
                        .iter()
                        .map(|summary| summary.applications)
                        .sum()
                })
                .unwrap_or(0);
            let unresolved_objective_shift_count = applied_world_state
                .map(|state| {
                    state
                        .unresolved_objective_state_shifts
                        .iter()
                        .map(|summary| summary.applications)
                        .sum()
                })
                .unwrap_or(0);
            let progressed_quest_line_count = applied_world_state
                .map(|state| {
                    state
                        .quest_lines
                        .iter()
                        .filter(|quest_line| quest_line.progress_basis_points > 0)
                        .count()
                })
                .unwrap_or(0);
            let terminal_quest_line_count = applied_world_state
                .map(|state| state.quest_lines.iter().filter(|quest_line| quest_line.terminal).count())
                .unwrap_or(0);

            let mut at_risk_team_ids = world
                .active_team_ids
                .iter()
                .filter_map(|team_id| {
                    standings_by_team
                        .get(team_id)
                        .filter(|standing| standing.active_death_marks > 0)
                        .map(|_| team_id.clone())
                })
                .collect::<Vec<_>>();
            at_risk_team_ids.sort();

            let world_lead_candidates = world
                .active_team_ids
                .iter()
                .filter_map(|team_id| standings_by_team.get(team_id).copied())
                .collect::<Vec<_>>();
            let mut leading_team_ids = if let Some(max_score_delta) = world_lead_candidates
                .iter()
                .map(|standing| standing.applied_score_delta)
                .max()
            {
                let score_delta_leaders = world_lead_candidates
                    .iter()
                    .copied()
                    .filter(|standing| standing.applied_score_delta == max_score_delta)
                    .collect::<Vec<_>>();
                let max_world_reward = score_delta_leaders
                    .iter()
                    .map(|standing| standing.world_reward_total)
                    .max_by(|left, right| {
                        left.partial_cmp(right)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .unwrap_or(0.0);
                score_delta_leaders
                    .iter()
                    .filter(|standing| standing.world_reward_total == max_world_reward)
                    .map(|standing| standing.team_id.clone())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            leading_team_ids.sort();

            WorldTournamentOrchestrationSummary {
                world_id: world.world_id.clone(),
                display_name: world.display_name.clone(),
                role: world.role,
                active_team_ids: world.active_team_ids.clone(),
                linked_world_ids: world.linked_world_ids.clone(),
                active_link_ids,
                assigned_agent_count,
                controller_mix,
                applied_score_delta_total,
                applied_death_mark_count,
                applied_death_mark_ticks,
                objective_shift_count,
                unresolved_objective_shift_count,
                progressed_quest_line_count,
                terminal_quest_line_count,
                leading_team_ids,
                at_risk_team_ids,
            }
        })
        .collect::<Vec<_>>();

    let active_world_ids = worlds
        .iter()
        .map(|world| world.world_id.clone())
        .collect::<Vec<_>>();
    let contested_world_ids = worlds
        .iter()
        .filter(|world| world.active_team_ids.len() > 1)
        .map(|world| world.world_id.clone())
        .collect::<Vec<_>>();
    let active_link_ids = links
        .iter()
        .filter(|link| tournament_link_ids.contains(&link.link_id))
        .map(|link| link.link_id.clone())
        .collect::<Vec<_>>();

    let mut at_risk_team_ids = tournament_control_plane
        .standings
        .iter()
        .filter(|standing| standing.active_death_marks > 0)
        .map(|standing| standing.team_id.clone())
        .collect::<Vec<_>>();
    at_risk_team_ids.sort();

    let mut leading_team_ids = if let Some(max_score_delta) = tournament_control_plane
        .standings
        .iter()
        .map(|standing| standing.applied_score_delta)
        .max()
    {
        let score_delta_leaders = tournament_control_plane
            .standings
            .iter()
            .filter(|standing| standing.applied_score_delta == max_score_delta)
            .collect::<Vec<_>>();
        let max_world_reward = score_delta_leaders
            .iter()
            .map(|standing| standing.world_reward_total)
            .max_by(|left, right| {
                left.partial_cmp(right)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0.0);
        score_delta_leaders
            .iter()
            .filter(|standing| standing.world_reward_total == max_world_reward)
            .map(|standing| standing.team_id.clone())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    leading_team_ids.sort();

    let all_quest_lines_terminal = !applied_world_states.is_empty()
        && applied_world_states
            .iter()
            .flat_map(|state| state.quest_lines.iter())
            .all(|quest_line| quest_line.terminal);
    let no_unresolved_objective_shifts = applied_world_states
        .iter()
        .all(|state| state.unresolved_objective_state_shifts.is_empty());
    let no_active_death_marks = tournament_control_plane
        .standings
        .iter()
        .all(|standing| standing.active_death_marks == 0);

    let phase = if active_world_ids.is_empty() || tournament_control_plane.standings.is_empty() {
        TournamentOrchestrationPhase::Muster
    } else if matches!(
        tournament.elimination_mode,
        TournamentEliminationMode::Permadeath
    ) && !at_risk_team_ids.is_empty()
    {
        TournamentOrchestrationPhase::SuddenDeath
    } else if all_quest_lines_terminal && no_unresolved_objective_shifts && no_active_death_marks {
        TournamentOrchestrationPhase::Resolved
    } else {
        TournamentOrchestrationPhase::Active
    };

    TournamentOrchestrationSummary {
        tournament_id: tournament.tournament_id.clone(),
        phase,
        active_world_ids,
        contested_world_ids,
        active_link_ids,
        leading_team_ids,
        at_risk_team_ids,
        worlds,
    }
}

pub fn assign_roster_to_world_teams(
    roster: &[String],
    world: &WorldRealityDefinition,
    teams: &[AgentTeamDefinition],
) -> Vec<WorldAdmissionAssignment> {
    let active_teams = world
        .active_team_ids
        .iter()
        .filter_map(|team_id| teams.iter().find(|team| &team.team_id == team_id))
        .filter(|team| {
            team.allowed_world_ids
                .iter()
                .any(|world_id| world_id == &world.world_id)
        })
        .collect::<Vec<_>>();
    if active_teams.is_empty() {
        return Vec::new();
    }

    let mut roster = roster.to_vec();
    roster.sort();
    roster.dedup();

    let mut assignments = Vec::new();
    let mut team_slots = active_teams
        .iter()
        .map(|team| (team.team_id.clone(), 0u16))
        .collect::<BTreeMap<_, _>>();
    let mut team_index = 0usize;

    for agent_id in roster {
        let mut selected = None;
        for offset in 0..active_teams.len() {
            let candidate_index = (team_index + offset) % active_teams.len();
            let candidate = active_teams[candidate_index];
            let next_slot = *team_slots
                .get(&candidate.team_id)
                .expect("candidate team has slot entry");
            if next_slot < candidate.max_agents {
                selected = Some((candidate_index, candidate.team_id.clone(), next_slot));
                break;
            }
        }

        if let Some((selected_index, team_id, slot_index)) = selected {
            assignments.push(WorldAdmissionAssignment {
                agent_id,
                team_id: team_id.clone(),
                slot_index,
            });
            if let Some(slot) = team_slots.get_mut(&team_id) {
                *slot += 1;
            }
            team_index = (selected_index + 1) % active_teams.len();
        }
    }

    assignments
}

pub fn build_world_admission_summary(
    roster: &[String],
    world: &WorldRealityDefinition,
    teams: &[AgentTeamDefinition],
) -> WorldAdmissionSummary {
    WorldAdmissionSummary {
        world_id: world.world_id.clone(),
        assignments: assign_roster_to_world_teams(roster, world, teams),
    }
}

/// Aggregate score effect applied to a team inside a world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamDeltaSummary {
    pub team_id: String,
    pub total_delta: i32,
}

/// Aggregate death-mark pressure applied to a team inside a world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamDeathMarkSummary {
    pub team_id: String,
    pub applications: usize,
    pub total_duration_ticks: u64,
}

/// Aggregate delta keyed by an authored id such as faction, encounter table, or biome.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedDeltaSummary {
    pub id: String,
    pub total_delta: i32,
}

/// Aggregate authored quest/objective shift.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveShiftSummary {
    pub quest_graph_id: String,
    pub stage_tag: String,
    pub applications: usize,
}

/// Application counts for a concrete quest stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestStageApplicationSummary {
    pub stage_id: String,
    pub title: String,
    pub applications: usize,
}

/// Resolved quest-line progression for one world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestLineStateSummary {
    pub quest_graph_id: String,
    pub display_name: String,
    pub current_stage_ids: Vec<String>,
    pub completed_stage_ids: Vec<String>,
    pub pending_stage_ids: Vec<String>,
    pub next_stage_ids: Vec<String>,
    pub progress_basis_points: u16,
    pub terminal: bool,
    pub stage_applications: Vec<QuestStageApplicationSummary>,
}

/// Applied cross-world consequences resolved into a target world.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppliedWorldStateSummary {
    pub world_id: String,
    pub display_name: String,
    pub role: WorldRealityRole,
    pub team_scores: Vec<TeamDeltaSummary>,
    pub death_marks: Vec<TeamDeathMarkSummary>,
    pub faction_reputation_deltas: Vec<NamedDeltaSummary>,
    pub encounter_weight_deltas: Vec<NamedDeltaSummary>,
    pub resource_scarcity_deltas: Vec<NamedDeltaSummary>,
    pub objective_state_shifts: Vec<ObjectiveShiftSummary>,
    pub unresolved_objective_state_shifts: Vec<ObjectiveShiftSummary>,
    pub quest_lines: Vec<QuestLineStateSummary>,
}

/// Reward and controller mix rollup keyed by agent type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControllerEvaluationSummary {
    pub agent_type: String,
    pub row_count: usize,
    pub reward_total: f32,
    pub average_reward_per_row: f32,
}

/// Per-world evaluation rollup for replay-derived datasets plus applied effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldEvaluationSummary {
    pub world_id: String,
    pub display_name: String,
    pub role: WorldRealityRole,
    pub average_reward_per_row: f32,
    pub controller_mix: Vec<ControllerEvaluationSummary>,
    pub quest_line_count: usize,
    pub progressed_quest_line_count: usize,
    pub average_quest_progress_basis_points: u16,
    pub applied_score_delta_total: i32,
    pub applied_death_mark_count: usize,
    pub applied_death_mark_ticks: u64,
    pub applied_objective_shift_count: usize,
    pub applied_reputation_delta_total: i32,
    pub applied_encounter_delta_total: i32,
    pub applied_resource_delta_total: i32,
}

/// Top-level evaluation summary across a multi-world scenario.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioEvaluationSummary {
    pub controller_mix: Vec<ControllerEvaluationSummary>,
    pub worlds: Vec<WorldEvaluationSummary>,
}

impl ScenarioEvaluationSummary {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("scenario_evaluation_summary", self)
    }
}

/// Portable authority-facing topology artifact emitted by headless runners and
/// consumable by future remote runtime surfaces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteTopologyBundle {
    pub version: RuntimeContractVersion,
    pub scenario_id: String,
    pub profile_id: String,
    pub generated_at_unix_ms: u128,
    pub tournament: WorldTournamentDefinition,
    pub teams: Vec<AgentTeamDefinition>,
    pub worlds: Vec<WorldRealityDefinition>,
    pub links: Vec<CrossWorldLinkDefinition>,
    pub world_quest_bindings: Vec<WorldQuestBinding>,
    pub world_admissions: Vec<WorldAdmissionSummary>,
    #[serde(default)]
    pub world_control_planes: Vec<WorldControlPlaneSummary>,
    #[serde(default)]
    pub tournament_control_plane: TournamentControlPlaneSummary,
    #[serde(default)]
    pub tournament_orchestration: TournamentOrchestrationSummary,
    pub quest_graphs: Vec<QuestStateGraph>,
    pub applied_world_states: Vec<AppliedWorldStateSummary>,
    pub evaluation: ScenarioEvaluationSummary,
}

impl RemoteTopologyBundle {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("remote_topology_bundle", self)
    }
}

pub fn build_remote_topology_bundle(
    scenario_id: &str,
    profile_id: &str,
    generated_at_unix_ms: u128,
    tournament: &WorldTournamentDefinition,
    teams: &[AgentTeamDefinition],
    worlds: &[WorldRealityDefinition],
    links: &[CrossWorldLinkDefinition],
    quest_graphs: &[QuestStateGraph],
    world_quest_graph_ids: &BTreeMap<String, Vec<String>>,
    world_admissions: &[WorldAdmissionSummary],
    world_control_planes: &[WorldControlPlaneSummary],
    tournament_control_plane: &TournamentControlPlaneSummary,
    tournament_orchestration: &TournamentOrchestrationSummary,
    applied_world_states: &[AppliedWorldStateSummary],
    evaluation: &ScenarioEvaluationSummary,
) -> RemoteTopologyBundle {
    RemoteTopologyBundle {
        version: RuntimeContractVersion::V1,
        scenario_id: scenario_id.into(),
        profile_id: profile_id.into(),
        generated_at_unix_ms,
        tournament: tournament.clone(),
        teams: teams.to_vec(),
        worlds: worlds.to_vec(),
        links: links.to_vec(),
        world_quest_bindings: build_world_quest_bindings(world_quest_graph_ids),
        world_admissions: world_admissions.to_vec(),
        world_control_planes: world_control_planes.to_vec(),
        tournament_control_plane: tournament_control_plane.clone(),
        tournament_orchestration: tournament_orchestration.clone(),
        quest_graphs: quest_graphs.to_vec(),
        applied_world_states: applied_world_states.to_vec(),
        evaluation: evaluation.clone(),
    }
}

/// Shared parity report for topology artifacts emitted by headless runners and
/// consumed by remote benchmark/runtime paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteTopologyParitySummary {
    pub consistent: bool,
    pub teams_match: bool,
    pub worlds_match: bool,
    pub links_match: bool,
    pub quest_graphs_match: bool,
    pub world_quest_bindings_match: bool,
    pub world_admissions_match: bool,
    pub world_control_planes_match: bool,
    pub tournament_control_plane_match: bool,
    pub tournament_orchestration_match: bool,
    pub applied_world_states_match: bool,
    pub evaluation_match: bool,
    pub missing_world_quest_binding_ids: Vec<String>,
    pub unexpected_world_quest_binding_ids: Vec<String>,
    pub missing_world_admission_ids: Vec<String>,
    pub unexpected_world_admission_ids: Vec<String>,
    pub missing_world_control_plane_ids: Vec<String>,
    pub unexpected_world_control_plane_ids: Vec<String>,
    pub missing_applied_world_ids: Vec<String>,
    pub unexpected_applied_world_ids: Vec<String>,
    pub missing_evaluation_world_ids: Vec<String>,
    pub unexpected_evaluation_world_ids: Vec<String>,
}

impl RemoteTopologyParitySummary {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("remote_topology_parity_summary", self)
    }
}

pub fn build_remote_topology_parity_summary(
    teams: &[AgentTeamDefinition],
    worlds: &[WorldRealityDefinition],
    links: &[CrossWorldLinkDefinition],
    quest_graphs: &[QuestStateGraph],
    world_quest_bindings: &[WorldQuestBinding],
    world_admissions: &[WorldAdmissionSummary],
    world_control_planes: &[WorldControlPlaneSummary],
    tournament_control_plane: &TournamentControlPlaneSummary,
    tournament_orchestration: &TournamentOrchestrationSummary,
    applied_world_states: &[AppliedWorldStateSummary],
    evaluation: &ScenarioEvaluationSummary,
    topology: &RemoteTopologyBundle,
) -> RemoteTopologyParitySummary {
    let expected_binding_ids = world_quest_bindings
        .iter()
        .map(|binding| binding.world_id.clone())
        .collect::<BTreeSet<_>>();
    let actual_binding_ids = topology
        .world_quest_bindings
        .iter()
        .map(|binding| binding.world_id.clone())
        .collect::<BTreeSet<_>>();
    let expected_applied_ids = applied_world_states
        .iter()
        .map(|state| state.world_id.clone())
        .collect::<BTreeSet<_>>();
    let expected_admission_ids = world_admissions
        .iter()
        .map(|summary| summary.world_id.clone())
        .collect::<BTreeSet<_>>();
    let expected_control_plane_ids = world_control_planes
        .iter()
        .map(|summary| summary.world_id.clone())
        .collect::<BTreeSet<_>>();
    let actual_applied_ids = topology
        .applied_world_states
        .iter()
        .map(|state| state.world_id.clone())
        .collect::<BTreeSet<_>>();
    let actual_admission_ids = topology
        .world_admissions
        .iter()
        .map(|summary| summary.world_id.clone())
        .collect::<BTreeSet<_>>();
    let actual_control_plane_ids = topology
        .world_control_planes
        .iter()
        .map(|summary| summary.world_id.clone())
        .collect::<BTreeSet<_>>();
    let expected_evaluation_ids = evaluation
        .worlds
        .iter()
        .map(|world| world.world_id.clone())
        .collect::<BTreeSet<_>>();
    let actual_evaluation_ids = topology
        .evaluation
        .worlds
        .iter()
        .map(|world| world.world_id.clone())
        .collect::<BTreeSet<_>>();

    let missing_world_quest_binding_ids = expected_binding_ids
        .difference(&actual_binding_ids)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_world_quest_binding_ids = actual_binding_ids
        .difference(&expected_binding_ids)
        .cloned()
        .collect::<Vec<_>>();
    let missing_world_admission_ids = expected_admission_ids
        .difference(&actual_admission_ids)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_world_admission_ids = actual_admission_ids
        .difference(&expected_admission_ids)
        .cloned()
        .collect::<Vec<_>>();
    let missing_world_control_plane_ids = expected_control_plane_ids
        .difference(&actual_control_plane_ids)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_world_control_plane_ids = actual_control_plane_ids
        .difference(&expected_control_plane_ids)
        .cloned()
        .collect::<Vec<_>>();
    let missing_applied_world_ids = expected_applied_ids
        .difference(&actual_applied_ids)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_applied_world_ids = actual_applied_ids
        .difference(&expected_applied_ids)
        .cloned()
        .collect::<Vec<_>>();
    let missing_evaluation_world_ids = expected_evaluation_ids
        .difference(&actual_evaluation_ids)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_evaluation_world_ids = actual_evaluation_ids
        .difference(&expected_evaluation_ids)
        .cloned()
        .collect::<Vec<_>>();

    let teams_match = topology.teams == teams;
    let worlds_match = topology.worlds == worlds;
    let links_match = topology.links == links;
    let quest_graphs_match = topology.quest_graphs == quest_graphs;
    let world_quest_bindings_match = topology.world_quest_bindings == world_quest_bindings;
    let world_admissions_match = topology.world_admissions == world_admissions;
    let world_control_planes_match = topology.world_control_planes == world_control_planes;
    let tournament_control_plane_match =
        topology.tournament_control_plane == *tournament_control_plane;
    let tournament_orchestration_match =
        topology.tournament_orchestration == *tournament_orchestration;
    let applied_world_states_match = topology.applied_world_states == applied_world_states;
    let evaluation_match = topology.evaluation == *evaluation;
    let consistent = teams_match
        && worlds_match
        && links_match
        && quest_graphs_match
        && world_quest_bindings_match
        && world_admissions_match
        && world_control_planes_match
        && tournament_control_plane_match
        && tournament_orchestration_match
        && applied_world_states_match
        && evaluation_match;

    RemoteTopologyParitySummary {
        consistent,
        teams_match,
        worlds_match,
        links_match,
        quest_graphs_match,
        world_quest_bindings_match,
        world_admissions_match,
        world_control_planes_match,
        tournament_control_plane_match,
        tournament_orchestration_match,
        applied_world_states_match,
        evaluation_match,
        missing_world_quest_binding_ids,
        unexpected_world_quest_binding_ids,
        missing_world_admission_ids,
        unexpected_world_admission_ids,
        missing_world_control_plane_ids,
        unexpected_world_control_plane_ids,
        missing_applied_world_ids,
        unexpected_applied_world_ids,
        missing_evaluation_world_ids,
        unexpected_evaluation_world_ids,
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
    use std::collections::BTreeMap;

    use crate::action::{Action, AgentAction};
    use crate::agent::AgentType;
    use crate::id::AgentId;
    use crate::observation::Observation;
    use crate::telemetry::{TickTelemetryFrame, ToolCallStatus};
    use crate::toon::decode_toon_value;

    use super::{
        assign_roster_to_world_teams, build_remote_topology_bundle,
        build_remote_topology_parity_summary, build_tournament_control_plane_summary,
        build_tournament_orchestration_summary, build_world_admission_summary,
        build_world_control_plane_summary, build_world_quest_bindings, AgentCapabilities,
        AgentRole, AgentRuntimeProfile, AgentTeamDefinition, AgentTypeCountSummary,
        AppliedWorldStateSummary, ControllerEvaluationSummary, CrossWorldEffect,
        CrossWorldLinkDefinition, CrossWorldPropagation, EncounterSpawnEntry,
        FactionReputationTier, FactionReputationTrack, NamedDeltaSummary,
        ObjectiveShiftSummary, QuestLineStateSummary, QuestStageApplicationSummary,
        QuestStageDefinition, QuestStateGraph, RegionEncounterTable,
        RemoteAgentFallbackMode, RemoteAgentObservationBudget, RemoteAgentTransportContract,
        RemoteTopologyBundle, RuntimeContractVersion, ScenarioEvaluationSummary, TeamControlMode,
        TeamDeathMarkSummary, TeamDeltaSummary, TeamRewardLedgerSummary, ToolBudget, ToolCatalog,
        ToolDefinition, ToolInvocationRequest, ToolInvocationResult, ToolPolicy,
        TournamentControlPlaneSummary, TournamentEliminationMode, TournamentOrchestrationPhase,
        TournamentOrchestrationSummary, TournamentTeamStandingSummary, VersionedAgentAction,
        VersionedObservation, VersionedTickTelemetry, WorldAdmissionAssignment,
        WorldAdmissionSummary, WorldChunkDefinition, WorldControlAssignmentSummary,
        WorldControlPlaneSummary, WorldEvaluationSummary, WorldRealityDefinition,
        WorldRealityRole, WorldRegionDefinition, WorldTeamControlSummary,
        WorldTournamentDefinition, WorldTournamentOrchestrationSummary,
        REMOTE_AGENT_MAX_ACTIONS_PER_TICK,
        REMOTE_AGENT_MAX_AUDIBLE_EVENTS, REMOTE_AGENT_MAX_VISIBLE_ENTITIES,
        REMOTE_AGENT_OBSERVATION_STALE_AFTER_TICKS, REMOTE_AGENT_TIMEOUT_AFTER_TICKS,
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
    fn remote_agent_transport_contract_uses_shared_spacetimedb_defaults() {
        let profile = AgentRuntimeProfile::for_agent_type(AgentType::LlmAgent);
        let contract = RemoteAgentTransportContract::spacetimedb_default(profile);

        assert_eq!(
            contract.observation_budget,
            RemoteAgentObservationBudget {
                max_visible_entities: REMOTE_AGENT_MAX_VISIBLE_ENTITIES as u32,
                max_audible_events: REMOTE_AGENT_MAX_AUDIBLE_EVENTS as u32,
                stale_after_ticks: REMOTE_AGENT_OBSERVATION_STALE_AFTER_TICKS,
                timeout_after_ticks: REMOTE_AGENT_TIMEOUT_AFTER_TICKS,
            }
        );
        assert_eq!(
            contract.action_budget.max_actions_per_tick,
            u32::from(REMOTE_AGENT_MAX_ACTIONS_PER_TICK)
        );
        assert_eq!(
            contract.fallback_mode,
            RemoteAgentFallbackMode::RejectActionsUntilFreshObservation
        );
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

    #[test]
    fn creator_world_contracts_export_to_toon_documents() {
        let quest_graph = QuestStateGraph::new(
            "verdant-intro",
            "Verdant Intro",
            "speak-to-mara",
            vec![QuestStageDefinition {
                stage_id: "speak-to-mara".into(),
                title: "Speak to Mara".into(),
                objectives: vec!["Talk to Archivist Mara".into()],
                next_stage_ids: vec!["attune-spire".into()],
                reward_tags: vec!["intro-xp".into()],
            }],
        );
        let quest_value =
            decode_toon_value(&quest_graph.to_toon_document()).expect("quest graph should decode");
        assert_eq!(quest_value["document_type"], "quest_state_graph");
        assert_eq!(quest_value["payload"]["quest_id"], "verdant-intro");

        let reputation_track = FactionReputationTrack::new(
            "verdant-wardens",
            "Verdant Wardens",
            vec![FactionReputationTier {
                tier_id: "trusted".into(),
                label: "Trusted".into(),
                minimum_score: 25,
                perk_tags: vec!["discounts".into()],
            }],
        );
        let reputation_value = decode_toon_value(&reputation_track.to_toon_document())
            .expect("reputation track should decode");
        assert_eq!(
            reputation_value["document_type"],
            "faction_reputation_track"
        );
        assert_eq!(reputation_value["payload"]["faction_id"], "verdant-wardens");

        let encounter_table = RegionEncounterTable::new(
            "verdant-wildlife",
            "verdant-hollow",
            "wildlife",
            vec![EncounterSpawnEntry {
                archetype_id: "verdant-lynx".into(),
                weight: 8,
                min_count: 1,
                max_count: 2,
                required_stage_tags: vec!["patrol".into()],
                required_reputation_tiers: vec!["trusted".into()],
            }],
        );
        let encounter_value = decode_toon_value(&encounter_table.to_toon_document())
            .expect("encounter table should decode");
        assert_eq!(encounter_value["document_type"], "region_encounter_table");
        assert_eq!(encounter_value["payload"]["table_id"], "verdant-wildlife");

        let mut chunk = WorldChunkDefinition::new("0:0", "verdant-hollow", "verdant-hollow");
        chunk.quest_graph_ids.push("verdant-intro".into());
        chunk.faction_track_ids.push("verdant-wardens".into());
        chunk.encounter_table_ids.push("verdant-wildlife".into());
        chunk.neighbor_chunk_keys.push("1:0".into());
        let chunk_value =
            decode_toon_value(&chunk.to_toon_document()).expect("chunk definition should decode");
        assert_eq!(chunk_value["document_type"], "world_chunk_definition");
        assert_eq!(chunk_value["payload"]["chunk_key"], "0:0");

        let mut region =
            WorldRegionDefinition::new("verdant-hollow", "Verdant Hollow", "verdant-hollow");
        region.chunk_keys.push("0:0".into());
        region.active_quest_graph_ids.push("verdant-intro".into());
        region.dominant_faction_track_id = "verdant-wardens".into();
        region.encounter_table_ids.push("verdant-wildlife".into());
        let region_value =
            decode_toon_value(&region.to_toon_document()).expect("region definition should decode");
        assert_eq!(region_value["document_type"], "world_region_definition");
        assert_eq!(region_value["payload"]["region_id"], "verdant-hollow");
    }

    #[test]
    fn multi_world_contracts_export_to_toon_documents() {
        let mut team = AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime");
        team.control_mode = TeamControlMode::HybridCommand;
        team.objective_tags = vec!["hold-altar".into(), "protect-bank".into()];
        let team_value =
            decode_toon_value(&team.to_toon_document()).expect("team definition should decode");
        assert_eq!(team_value["document_type"], "agent_team_definition");
        assert_eq!(team_value["payload"]["team_id"], "iron-sigil");

        let mut world =
            WorldRealityDefinition::new("deadman-prime", "Deadman Prime", "deadman-seasonal");
        world.role = WorldRealityRole::Tournament;
        world.linked_world_ids.push("deadman-shadow".into());
        world.active_team_ids.push("iron-sigil".into());
        let world_value =
            decode_toon_value(&world.to_toon_document()).expect("world definition should decode");
        assert_eq!(world_value["document_type"], "world_reality_definition");
        assert_eq!(world_value["payload"]["world_id"], "deadman-prime");

        let mut link =
            CrossWorldLinkDefinition::new("prime-to-shadow", "deadman-prime", "deadman-shadow");
        link.trigger_tags = vec!["player-killed".into(), "altar-captured".into()];
        link.propagation = CrossWorldPropagation::Delayed { ticks: 300 };
        link.effects = vec![
            CrossWorldEffect::TeamScoreDelta {
                team_id: "iron-sigil".into(),
                delta: 5,
            },
            CrossWorldEffect::DeathMark {
                team_id: "iron-sigil".into(),
                duration_ticks: 600,
            },
        ];
        let link_value =
            decode_toon_value(&link.to_toon_document()).expect("link definition should decode");
        assert_eq!(link_value["document_type"], "cross_world_link_definition");
        assert_eq!(link_value["payload"]["source_world_id"], "deadman-prime");

        let mut tournament =
            WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup");
        tournament.world_ids = vec!["deadman-prime".into(), "deadman-shadow".into()];
        tournament.team_ids = vec!["iron-sigil".into()];
        tournament.cross_world_link_ids = vec!["prime-to-shadow".into()];
        tournament.max_agents_per_team = 10;
        tournament.elimination_mode = TournamentEliminationMode::Permadeath;
        tournament.reward_tags = vec!["season-points".into(), "reality-dominance".into()];
        let tournament_value = decode_toon_value(&tournament.to_toon_document())
            .expect("tournament definition should decode");
        assert_eq!(
            tournament_value["document_type"],
            "world_tournament_definition"
        );
        assert_eq!(
            tournament_value["payload"]["tournament_id"],
            "deadman-neural-cup"
        );
    }

    #[test]
    fn remote_topology_bundle_exports_quest_and_evaluation_state() {
        let mut tournament =
            WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup");
        tournament.world_ids = vec!["deadman-prime".into()];
        tournament.team_ids = vec!["iron-sigil".into()];

        let mut world =
            WorldRealityDefinition::new("deadman-prime", "Deadman Prime", "deadman-seasonal");
        world.role = WorldRealityRole::Tournament;
        world.active_team_ids = vec!["iron-sigil".into()];

        let quest_graph = QuestStateGraph::new(
            "deadman-prime-season",
            "Deadman Prime: Blood Season",
            "enter-bracket",
            vec![QuestStageDefinition {
                stage_id: "enter-bracket".into(),
                title: "Enter the Bracket".into(),
                objectives: vec!["Establish the team camp.".into()],
                next_stage_ids: vec!["wilds-under-siege".into()],
                reward_tags: vec!["season-open".into()],
            }],
        );
        let tournament_control_plane = TournamentControlPlaneSummary {
            tournament_id: "deadman-neural-cup".into(),
            standings: vec![TournamentTeamStandingSummary {
                team_id: "iron-sigil".into(),
                display_name: "Iron Sigil".into(),
                control_mode: TeamControlMode::DeveloperCaptain,
                home_world_id: "deadman-prime".into(),
                participating_world_ids: vec!["deadman-prime".into()],
                assigned_agent_count: 1,
                controller_mix: vec![AgentTypeCountSummary {
                    agent_type: "neural_agent".into(),
                    count: 1,
                }],
                dataset_row_count: 1,
                world_reward_total: 4.5,
                applied_score_delta: 8,
                active_death_marks: 1,
                active_death_mark_ticks: 600,
            }],
        };

        let bundle = build_remote_topology_bundle(
            "deadman-neural-cup",
            "ci-smoke",
            42,
            &tournament,
            &[AgentTeamDefinition::new(
                "iron-sigil",
                "Iron Sigil",
                "deadman-prime",
            )],
            &[world],
            &[CrossWorldLinkDefinition::new(
                "prime-to-shadow",
                "deadman-prime",
                "deadman-shadow",
            )],
            &[quest_graph],
            &BTreeMap::from([("deadman-prime".into(), vec!["deadman-prime-season".into()])]),
            &[WorldAdmissionSummary {
                world_id: "deadman-prime".into(),
                assignments: vec![WorldAdmissionAssignment {
                    agent_id: "agent-a".into(),
                    team_id: "iron-sigil".into(),
                    slot_index: 0,
                }],
            }],
            &[WorldControlPlaneSummary {
                world_id: "deadman-prime".into(),
                teams: vec![WorldTeamControlSummary {
                    team_id: "iron-sigil".into(),
                    assignments: vec![WorldControlAssignmentSummary {
                        agent_id: "agent-a".into(),
                        slot_index: 0,
                        runtime_profile: AgentRuntimeProfile::for_agent_type(
                            AgentType::NeuralAgent,
                        ),
                    }],
                    controller_mix: vec![AgentTypeCountSummary {
                        agent_type: "neural_agent".into(),
                        count: 1,
                    }],
                }],
            }],
            &tournament_control_plane,
            &TournamentOrchestrationSummary::default(),
            &[AppliedWorldStateSummary {
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                role: WorldRealityRole::Tournament,
                team_scores: vec![TeamDeltaSummary {
                    team_id: "iron-sigil".into(),
                    total_delta: 8,
                }],
                death_marks: vec![TeamDeathMarkSummary {
                    team_id: "iron-sigil".into(),
                    applications: 1,
                    total_duration_ticks: 600,
                }],
                faction_reputation_deltas: vec![NamedDeltaSummary {
                    id: "echo-order".into(),
                    total_delta: 2,
                }],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![ObjectiveShiftSummary {
                    quest_graph_id: "deadman-prime-season".into(),
                    stage_tag: "wilds-under-siege".into(),
                    applications: 1,
                }],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![QuestLineStateSummary {
                    quest_graph_id: "deadman-prime-season".into(),
                    display_name: "Deadman Prime: Blood Season".into(),
                    current_stage_ids: vec!["wilds-under-siege".into()],
                    completed_stage_ids: vec!["enter-bracket".into()],
                    pending_stage_ids: vec![],
                    next_stage_ids: vec![],
                    progress_basis_points: 5_000,
                    terminal: false,
                    stage_applications: vec![QuestStageApplicationSummary {
                        stage_id: "wilds-under-siege".into(),
                        title: "Wilds Under Siege".into(),
                        applications: 1,
                    }],
                }],
            }],
            &ScenarioEvaluationSummary {
                controller_mix: vec![ControllerEvaluationSummary {
                    agent_type: "neural_agent".into(),
                    row_count: 1,
                    reward_total: 4.5,
                    average_reward_per_row: 4.5,
                }],
                worlds: vec![WorldEvaluationSummary {
                    world_id: "deadman-prime".into(),
                    display_name: "Deadman Prime".into(),
                    role: WorldRealityRole::Tournament,
                    average_reward_per_row: 4.5,
                    controller_mix: vec![ControllerEvaluationSummary {
                        agent_type: "neural_agent".into(),
                        row_count: 1,
                        reward_total: 4.5,
                        average_reward_per_row: 4.5,
                    }],
                    quest_line_count: 1,
                    progressed_quest_line_count: 1,
                    average_quest_progress_basis_points: 5_000,
                    applied_score_delta_total: 8,
                    applied_death_mark_count: 1,
                    applied_death_mark_ticks: 600,
                    applied_objective_shift_count: 1,
                    applied_reputation_delta_total: 2,
                    applied_encounter_delta_total: 0,
                    applied_resource_delta_total: 0,
                }],
            },
        );

        let value = decode_toon_value(&bundle.to_toon_document())
            .expect("remote topology bundle should decode");
        assert_eq!(value["document_type"], "remote_topology_bundle");
        assert_eq!(value["payload"]["scenario_id"], "deadman-neural-cup");
        assert_eq!(
            value["payload"]["world_quest_bindings"][0]["quest_graph_ids"][0],
            "deadman-prime-season"
        );
        assert_eq!(
            value["payload"]["world_admissions"][0]["assignments"][0]["team_id"],
            "iron-sigil"
        );
        assert_eq!(
            value["payload"]["applied_world_states"][0]["quest_lines"][0]["current_stage_ids"][0],
            "wilds-under-siege"
        );
        assert_eq!(
            value["payload"]["evaluation"]["worlds"][0]["applied_score_delta_total"],
            8
        );
        assert_eq!(
            value["payload"]["tournament_control_plane"]["standings"][0]["team_id"],
            "iron-sigil"
        );
    }

    #[test]
    fn build_world_quest_bindings_sorts_bindings_by_world_id() {
        let bindings = build_world_quest_bindings(&BTreeMap::from([
            (
                "deadman-shadow".into(),
                vec!["deadman-shadow-hunt".into(), "deadman-shadow-rift".into()],
            ),
            ("deadman-prime".into(), vec!["deadman-prime-season".into()]),
        ]));

        assert_eq!(bindings.len(), 2);
        assert_eq!(bindings[0].world_id, "deadman-prime");
        assert_eq!(
            bindings[1].quest_graph_ids,
            vec![
                "deadman-shadow-hunt".to_string(),
                "deadman-shadow-rift".to_string()
            ]
        );
    }

    #[test]
    fn build_world_admission_summary_round_robins_roster_across_active_teams() {
        let mut iron_sigil = AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime");
        iron_sigil.allowed_world_ids = vec!["deadman-prime".into()];
        let mut gloam_mesh = AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow");
        gloam_mesh.allowed_world_ids = vec!["deadman-prime".into(), "deadman-shadow".into()];

        let mut world =
            WorldRealityDefinition::new("deadman-prime", "Deadman Prime", "deadman-seasonal");
        world.role = WorldRealityRole::Tournament;
        world.active_team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];
        let teams = vec![iron_sigil, gloam_mesh];
        let roster = vec![
            "agent-b".to_string(),
            "agent-a".to_string(),
            "agent-c".to_string(),
            "agent-d".to_string(),
        ];

        let summary = build_world_admission_summary(&roster, &world, &teams);

        assert_eq!(
            summary,
            WorldAdmissionSummary {
                world_id: "deadman-prime".into(),
                assignments: vec![
                    WorldAdmissionAssignment {
                        agent_id: "agent-a".into(),
                        team_id: "iron-sigil".into(),
                        slot_index: 0,
                    },
                    WorldAdmissionAssignment {
                        agent_id: "agent-b".into(),
                        team_id: "gloam-mesh".into(),
                        slot_index: 0,
                    },
                    WorldAdmissionAssignment {
                        agent_id: "agent-c".into(),
                        team_id: "iron-sigil".into(),
                        slot_index: 1,
                    },
                    WorldAdmissionAssignment {
                        agent_id: "agent-d".into(),
                        team_id: "gloam-mesh".into(),
                        slot_index: 1,
                    },
                ],
            }
        );
        assert_eq!(
            assign_roster_to_world_teams(&roster, &world, &teams),
            summary.assignments
        );
    }

    #[test]
    fn build_world_control_plane_summary_preserves_slots_and_controller_mix() {
        let admissions = WorldAdmissionSummary {
            world_id: "deadman-prime".into(),
            assignments: vec![
                WorldAdmissionAssignment {
                    agent_id: "agent-a".into(),
                    team_id: "iron-sigil".into(),
                    slot_index: 0,
                },
                WorldAdmissionAssignment {
                    agent_id: "agent-b".into(),
                    team_id: "iron-sigil".into(),
                    slot_index: 1,
                },
                WorldAdmissionAssignment {
                    agent_id: "agent-c".into(),
                    team_id: "gloam-mesh".into(),
                    slot_index: 0,
                },
            ],
        };
        let runtime_profiles = BTreeMap::from([
            (
                "agent-a".to_string(),
                AgentRuntimeProfile::for_agent_type(AgentType::NeuralAgent),
            ),
            (
                "agent-b".to_string(),
                AgentRuntimeProfile::for_agent_type(AgentType::LlmAgent),
            ),
            (
                "agent-c".to_string(),
                AgentRuntimeProfile::for_agent_type(AgentType::ScriptedNpc),
            ),
        ]);

        let summary = build_world_control_plane_summary(&admissions, &runtime_profiles);

        assert_eq!(summary.world_id, "deadman-prime");
        assert_eq!(summary.teams.len(), 2);
        assert_eq!(summary.teams[0].team_id, "gloam-mesh");
        assert_eq!(summary.teams[0].assignments[0].agent_id, "agent-c");
        assert_eq!(
            summary.teams[0].controller_mix,
            vec![AgentTypeCountSummary {
                agent_type: "scripted_npc".into(),
                count: 1,
            }]
        );
        assert_eq!(summary.teams[1].team_id, "iron-sigil");
        assert_eq!(summary.teams[1].assignments[0].slot_index, 0);
        assert_eq!(summary.teams[1].assignments[1].slot_index, 1);
        assert_eq!(
            summary.teams[1].controller_mix,
            vec![
                AgentTypeCountSummary {
                    agent_type: "llm_agent".into(),
                    count: 1,
                },
                AgentTypeCountSummary {
                    agent_type: "neural_agent".into(),
                    count: 1,
                },
            ]
        );
    }

    #[test]
    fn build_tournament_control_plane_summary_rolls_up_rewards_and_effects() {
        let mut iron_sigil = AgentTeamDefinition::new("iron-sigil", "Iron Sigil", "deadman-prime");
        iron_sigil.control_mode = TeamControlMode::HybridCommand;
        let mut gloam_mesh = AgentTeamDefinition::new("gloam-mesh", "Gloam Mesh", "deadman-shadow");
        gloam_mesh.control_mode = TeamControlMode::AutonomousSwarm;

        let worlds = vec![
            WorldRealityDefinition {
                version: RuntimeContractVersion::V1,
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                ruleset_id: "deadman".into(),
                role: WorldRealityRole::Tournament,
                linked_world_ids: vec!["deadman-shadow".into()],
                active_team_ids: vec!["iron-sigil".into(), "gloam-mesh".into()],
            },
            WorldRealityDefinition {
                version: RuntimeContractVersion::V1,
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                ruleset_id: "shadow".into(),
                role: WorldRealityRole::Shadow,
                linked_world_ids: vec!["deadman-prime".into()],
                active_team_ids: vec!["iron-sigil".into(), "gloam-mesh".into()],
            },
        ];
        let world_control_planes = vec![
            WorldControlPlaneSummary {
                world_id: "deadman-prime".into(),
                teams: vec![
                    WorldTeamControlSummary {
                        team_id: "gloam-mesh".into(),
                        assignments: vec![WorldControlAssignmentSummary {
                            agent_id: "agent-b".into(),
                            slot_index: 0,
                            runtime_profile: AgentRuntimeProfile::for_agent_type(
                                AgentType::LlmAgent,
                            ),
                        }],
                        controller_mix: vec![AgentTypeCountSummary {
                            agent_type: "llm_agent".into(),
                            count: 1,
                        }],
                    },
                    WorldTeamControlSummary {
                        team_id: "iron-sigil".into(),
                        assignments: vec![WorldControlAssignmentSummary {
                            agent_id: "agent-a".into(),
                            slot_index: 0,
                            runtime_profile: AgentRuntimeProfile::for_agent_type(
                                AgentType::NeuralAgent,
                            ),
                        }],
                        controller_mix: vec![AgentTypeCountSummary {
                            agent_type: "neural_agent".into(),
                            count: 1,
                        }],
                    },
                ],
            },
            WorldControlPlaneSummary {
                world_id: "deadman-shadow".into(),
                teams: vec![],
            },
        ];
        let reward_ledgers = vec![
            TeamRewardLedgerSummary {
                team_id: "iron-sigil".into(),
                dataset_row_count: 1,
                world_reward_total: 2.0,
            },
            TeamRewardLedgerSummary {
                team_id: "gloam-mesh".into(),
                dataset_row_count: 1,
                world_reward_total: 1.0,
            },
        ];
        let applied_world_states = vec![
            AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
                team_scores: vec![TeamDeltaSummary {
                    team_id: "iron-sigil".into(),
                    total_delta: 15,
                }],
                death_marks: vec![TeamDeathMarkSummary {
                    team_id: "gloam-mesh".into(),
                    applications: 2,
                    total_duration_ticks: 1200,
                }],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![],
            },
            AppliedWorldStateSummary {
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                role: WorldRealityRole::Tournament,
                team_scores: vec![TeamDeltaSummary {
                    team_id: "gloam-mesh".into(),
                    total_delta: 8,
                }],
                death_marks: vec![],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![],
            },
        ];
        let tournament = WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup");

        let summary = build_tournament_control_plane_summary(
            &tournament,
            &[iron_sigil, gloam_mesh],
            &worlds,
            &world_control_planes,
            &reward_ledgers,
            &applied_world_states,
        );

        assert_eq!(
            summary,
            TournamentControlPlaneSummary {
                tournament_id: "deadman-neural-cup".into(),
                standings: vec![
                    TournamentTeamStandingSummary {
                        team_id: "iron-sigil".into(),
                        display_name: "Iron Sigil".into(),
                        control_mode: TeamControlMode::HybridCommand,
                        home_world_id: "deadman-prime".into(),
                        participating_world_ids: vec![
                            "deadman-prime".into(),
                            "deadman-shadow".into()
                        ],
                        assigned_agent_count: 1,
                        controller_mix: vec![AgentTypeCountSummary {
                            agent_type: "neural_agent".into(),
                            count: 1,
                        }],
                        dataset_row_count: 1,
                        world_reward_total: 2.0,
                        applied_score_delta: 15,
                        active_death_marks: 0,
                        active_death_mark_ticks: 0,
                    },
                    TournamentTeamStandingSummary {
                        team_id: "gloam-mesh".into(),
                        display_name: "Gloam Mesh".into(),
                        control_mode: TeamControlMode::AutonomousSwarm,
                        home_world_id: "deadman-shadow".into(),
                        participating_world_ids: vec![
                            "deadman-prime".into(),
                            "deadman-shadow".into()
                        ],
                        assigned_agent_count: 1,
                        controller_mix: vec![AgentTypeCountSummary {
                            agent_type: "llm_agent".into(),
                            count: 1,
                        }],
                        dataset_row_count: 1,
                        world_reward_total: 1.0,
                        applied_score_delta: 8,
                        active_death_marks: 2,
                        active_death_mark_ticks: 1200,
                    },
                ],
            }
        );
    }

    #[test]
    fn tournament_orchestration_summary_rolls_up_world_pressure_and_phase() {
        let worlds = vec![
            WorldRealityDefinition {
                version: RuntimeContractVersion::V1,
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                ruleset_id: "deadman".into(),
                role: WorldRealityRole::Tournament,
                linked_world_ids: vec!["deadman-shadow".into()],
                active_team_ids: vec!["iron-sigil".into(), "gloam-mesh".into()],
            },
            WorldRealityDefinition {
                version: RuntimeContractVersion::V1,
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                ruleset_id: "shadow".into(),
                role: WorldRealityRole::Shadow,
                linked_world_ids: vec!["deadman-prime".into()],
                active_team_ids: vec!["iron-sigil".into(), "gloam-mesh".into()],
            },
        ];
        let world_control_planes = vec![
            WorldControlPlaneSummary {
                world_id: "deadman-prime".into(),
                teams: vec![
                    WorldTeamControlSummary {
                        team_id: "iron-sigil".into(),
                        assignments: vec![WorldControlAssignmentSummary {
                            agent_id: "agent-a".into(),
                            slot_index: 0,
                            runtime_profile: AgentRuntimeProfile::for_agent_type(
                                AgentType::NeuralAgent,
                            ),
                        }],
                        controller_mix: vec![AgentTypeCountSummary {
                            agent_type: "neural_agent".into(),
                            count: 1,
                        }],
                    },
                    WorldTeamControlSummary {
                        team_id: "gloam-mesh".into(),
                        assignments: vec![WorldControlAssignmentSummary {
                            agent_id: "agent-b".into(),
                            slot_index: 0,
                            runtime_profile: AgentRuntimeProfile::for_agent_type(
                                AgentType::LlmAgent,
                            ),
                        }],
                        controller_mix: vec![AgentTypeCountSummary {
                            agent_type: "llm_agent".into(),
                            count: 1,
                        }],
                    },
                ],
            },
            WorldControlPlaneSummary {
                world_id: "deadman-shadow".into(),
                teams: vec![],
            },
        ];
        let applied_world_states = vec![
            AppliedWorldStateSummary {
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                role: WorldRealityRole::Tournament,
                team_scores: vec![TeamDeltaSummary {
                    team_id: "iron-sigil".into(),
                    total_delta: 15,
                }],
                death_marks: vec![],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![ObjectiveShiftSummary {
                    quest_graph_id: "deadman-prime-season".into(),
                    stage_tag: "blood-round".into(),
                    applications: 1,
                }],
                unresolved_objective_state_shifts: vec![],
                quest_lines: vec![QuestLineStateSummary {
                    quest_graph_id: "deadman-prime-season".into(),
                    display_name: "Deadman Prime: Blood Season".into(),
                    current_stage_ids: vec!["blood-round".into()],
                    completed_stage_ids: vec!["enter-bracket".into()],
                    pending_stage_ids: vec!["crown-push".into()],
                    next_stage_ids: vec!["crown-push".into()],
                    progress_basis_points: 5_000,
                    terminal: false,
                    stage_applications: vec![QuestStageApplicationSummary {
                        stage_id: "blood-round".into(),
                        title: "Blood Round".into(),
                        applications: 1,
                    }],
                }],
            },
            AppliedWorldStateSummary {
                world_id: "deadman-shadow".into(),
                display_name: "Deadman Shadow".into(),
                role: WorldRealityRole::Shadow,
                team_scores: vec![TeamDeltaSummary {
                    team_id: "gloam-mesh".into(),
                    total_delta: 8,
                }],
                death_marks: vec![TeamDeathMarkSummary {
                    team_id: "gloam-mesh".into(),
                    applications: 2,
                    total_duration_ticks: 1200,
                }],
                faction_reputation_deltas: vec![],
                encounter_weight_deltas: vec![],
                resource_scarcity_deltas: vec![],
                objective_state_shifts: vec![ObjectiveShiftSummary {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    stage_tag: "marked-by-kills".into(),
                    applications: 2,
                }],
                unresolved_objective_state_shifts: vec![ObjectiveShiftSummary {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    stage_tag: "rift-collapse".into(),
                    applications: 1,
                }],
                quest_lines: vec![QuestLineStateSummary {
                    quest_graph_id: "deadman-shadow-hunt".into(),
                    display_name: "Deadman Shadow: Mirror Hunt".into(),
                    current_stage_ids: vec!["marked-by-kills".into()],
                    completed_stage_ids: vec!["shadow-observe".into()],
                    pending_stage_ids: vec!["rift-collapse".into()],
                    next_stage_ids: vec!["rift-collapse".into()],
                    progress_basis_points: 6_666,
                    terminal: false,
                    stage_applications: vec![QuestStageApplicationSummary {
                        stage_id: "marked-by-kills".into(),
                        title: "Marked by Kills".into(),
                        applications: 2,
                    }],
                }],
            },
        ];
        let tournament_control_plane = TournamentControlPlaneSummary {
            tournament_id: "deadman-neural-cup".into(),
            standings: vec![
                TournamentTeamStandingSummary {
                    team_id: "iron-sigil".into(),
                    display_name: "Iron Sigil".into(),
                    control_mode: TeamControlMode::HybridCommand,
                    home_world_id: "deadman-prime".into(),
                    participating_world_ids: vec![
                        "deadman-prime".into(),
                        "deadman-shadow".into(),
                    ],
                    assigned_agent_count: 1,
                    controller_mix: vec![AgentTypeCountSummary {
                        agent_type: "neural_agent".into(),
                        count: 1,
                    }],
                    dataset_row_count: 1,
                    world_reward_total: 2.0,
                    applied_score_delta: 15,
                    active_death_marks: 0,
                    active_death_mark_ticks: 0,
                },
                TournamentTeamStandingSummary {
                    team_id: "gloam-mesh".into(),
                    display_name: "Gloam Mesh".into(),
                    control_mode: TeamControlMode::AutonomousSwarm,
                    home_world_id: "deadman-shadow".into(),
                    participating_world_ids: vec![
                        "deadman-prime".into(),
                        "deadman-shadow".into(),
                    ],
                    assigned_agent_count: 1,
                    controller_mix: vec![AgentTypeCountSummary {
                        agent_type: "llm_agent".into(),
                        count: 1,
                    }],
                    dataset_row_count: 1,
                    world_reward_total: 1.0,
                    applied_score_delta: 8,
                    active_death_marks: 2,
                    active_death_mark_ticks: 1200,
                },
            ],
        };
        let mut tournament =
            WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup");
        tournament.world_ids = vec!["deadman-prime".into(), "deadman-shadow".into()];
        tournament.team_ids = vec!["iron-sigil".into(), "gloam-mesh".into()];
        tournament.cross_world_link_ids = vec!["prime-to-shadow".into()];
        tournament.elimination_mode = TournamentEliminationMode::Permadeath;

        let summary = build_tournament_orchestration_summary(
            &tournament,
            &worlds,
            &[CrossWorldLinkDefinition::new(
                "prime-to-shadow",
                "deadman-prime",
                "deadman-shadow",
            )],
            &world_control_planes,
            &tournament_control_plane,
            &applied_world_states,
        );

        assert_eq!(summary.phase, TournamentOrchestrationPhase::SuddenDeath);
        assert_eq!(summary.leading_team_ids, vec!["iron-sigil".to_string()]);
        assert_eq!(summary.at_risk_team_ids, vec!["gloam-mesh".to_string()]);
        assert_eq!(summary.contested_world_ids.len(), 2);
        assert_eq!(summary.active_link_ids, vec!["prime-to-shadow".to_string()]);
        assert_eq!(
            summary
                .worlds
                .iter()
                .find(|world| world.world_id == "deadman-prime")
                .expect("prime world summary present"),
            &WorldTournamentOrchestrationSummary {
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                role: WorldRealityRole::Tournament,
                active_team_ids: vec!["iron-sigil".into(), "gloam-mesh".into()],
                linked_world_ids: vec!["deadman-shadow".into()],
                active_link_ids: vec!["prime-to-shadow".into()],
                assigned_agent_count: 2,
                controller_mix: vec![
                    AgentTypeCountSummary {
                        agent_type: "llm_agent".into(),
                        count: 1,
                    },
                    AgentTypeCountSummary {
                        agent_type: "neural_agent".into(),
                        count: 1,
                    },
                ],
                applied_score_delta_total: 15,
                applied_death_mark_count: 0,
                applied_death_mark_ticks: 0,
                objective_shift_count: 1,
                unresolved_objective_shift_count: 0,
                progressed_quest_line_count: 1,
                terminal_quest_line_count: 0,
                leading_team_ids: vec!["iron-sigil".into()],
                at_risk_team_ids: vec!["gloam-mesh".into()],
            }
        );
    }

    #[test]
    fn remote_topology_parity_summary_flags_missing_bundle_sections() {
        let teams = vec![AgentTeamDefinition::new(
            "iron-sigil",
            "Iron Sigil",
            "deadman-prime",
        )];
        let worlds = vec![{
            let mut world =
                WorldRealityDefinition::new("deadman-prime", "Deadman Prime", "deadman-seasonal");
            world.role = WorldRealityRole::Tournament;
            world.active_team_ids = vec!["iron-sigil".into()];
            world
        }];
        let quest_graphs = vec![QuestStateGraph::new(
            "deadman-prime-season",
            "Deadman Prime: Blood Season",
            "enter-bracket",
            vec![QuestStageDefinition {
                stage_id: "enter-bracket".into(),
                title: "Enter the Bracket".into(),
                objectives: vec!["Establish camp.".into()],
                next_stage_ids: vec!["wilds-under-siege".into()],
                reward_tags: vec!["season-open".into()],
            }],
        )];
        let world_quest_bindings = build_world_quest_bindings(&BTreeMap::from([(
            "deadman-prime".into(),
            vec!["deadman-prime-season".into()],
        )]));
        let world_admissions = vec![WorldAdmissionSummary {
            world_id: "deadman-prime".into(),
            assignments: vec![WorldAdmissionAssignment {
                agent_id: "agent-a".into(),
                team_id: "iron-sigil".into(),
                slot_index: 0,
            }],
        }];
        let world_control_planes = vec![WorldControlPlaneSummary {
            world_id: "deadman-prime".into(),
            teams: vec![WorldTeamControlSummary {
                team_id: "iron-sigil".into(),
                assignments: vec![WorldControlAssignmentSummary {
                    agent_id: "agent-a".into(),
                    slot_index: 0,
                    runtime_profile: AgentRuntimeProfile::for_agent_type(AgentType::NeuralAgent),
                }],
                controller_mix: vec![AgentTypeCountSummary {
                    agent_type: "neural_agent".into(),
                    count: 1,
                }],
            }],
        }];
        let tournament_control_plane = TournamentControlPlaneSummary {
            tournament_id: "deadman-neural-cup".into(),
            standings: vec![TournamentTeamStandingSummary {
                team_id: "iron-sigil".into(),
                display_name: "Iron Sigil".into(),
                control_mode: TeamControlMode::DeveloperCaptain,
                home_world_id: "deadman-prime".into(),
                participating_world_ids: vec!["deadman-prime".into()],
                assigned_agent_count: 1,
                controller_mix: vec![AgentTypeCountSummary {
                    agent_type: "neural_agent".into(),
                    count: 1,
                }],
                dataset_row_count: 0,
                world_reward_total: 0.0,
                applied_score_delta: 0,
                active_death_marks: 0,
                active_death_mark_ticks: 0,
            }],
        };
        let applied_world_states = vec![AppliedWorldStateSummary {
            world_id: "deadman-prime".into(),
            display_name: "Deadman Prime".into(),
            role: WorldRealityRole::Tournament,
            team_scores: vec![],
            death_marks: vec![],
            faction_reputation_deltas: vec![],
            encounter_weight_deltas: vec![],
            resource_scarcity_deltas: vec![],
            objective_state_shifts: vec![],
            unresolved_objective_state_shifts: vec![],
            quest_lines: vec![],
        }];
        let evaluation = ScenarioEvaluationSummary {
            controller_mix: vec![],
            worlds: vec![WorldEvaluationSummary {
                world_id: "deadman-prime".into(),
                display_name: "Deadman Prime".into(),
                role: WorldRealityRole::Tournament,
                average_reward_per_row: 0.0,
                controller_mix: vec![],
                quest_line_count: 0,
                progressed_quest_line_count: 0,
                average_quest_progress_basis_points: 0,
                applied_score_delta_total: 0,
                applied_death_mark_count: 0,
                applied_death_mark_ticks: 0,
                applied_objective_shift_count: 0,
                applied_reputation_delta_total: 0,
                applied_encounter_delta_total: 0,
                applied_resource_delta_total: 0,
            }],
        };
        let mut topology = RemoteTopologyBundle {
            version: RuntimeContractVersion::V1,
            scenario_id: "deadman-neural-cup".into(),
            profile_id: "ci-smoke".into(),
            generated_at_unix_ms: 42,
            tournament: WorldTournamentDefinition::new("deadman-neural-cup", "Deadman Neural Cup"),
            teams: teams.clone(),
            worlds: worlds.clone(),
            links: vec![],
            world_quest_bindings: world_quest_bindings.clone(),
            world_admissions: world_admissions.clone(),
            world_control_planes: world_control_planes.clone(),
            tournament_control_plane: tournament_control_plane.clone(),
            tournament_orchestration: TournamentOrchestrationSummary::default(),
            quest_graphs: quest_graphs.clone(),
            applied_world_states: applied_world_states.clone(),
            evaluation: evaluation.clone(),
        };
        topology.world_quest_bindings.clear();
        topology.world_admissions.clear();
        topology.world_control_planes.clear();
        topology.tournament_control_plane = TournamentControlPlaneSummary::default();
        topology.evaluation.worlds.clear();

        let parity = build_remote_topology_parity_summary(
            &teams,
            &worlds,
            &[],
            &quest_graphs,
            &world_quest_bindings,
            &world_admissions,
            &world_control_planes,
            &tournament_control_plane,
            &TournamentOrchestrationSummary::default(),
            &applied_world_states,
            &evaluation,
            &topology,
        );

        assert!(!parity.consistent);
        assert!(!parity.world_quest_bindings_match);
        assert!(!parity.world_admissions_match);
        assert!(!parity.world_control_planes_match);
        assert!(!parity.tournament_control_plane_match);
        assert!(!parity.evaluation_match);
        assert_eq!(
            parity.missing_world_quest_binding_ids,
            vec!["deadman-prime".to_string()]
        );
        assert_eq!(
            parity.missing_world_admission_ids,
            vec!["deadman-prime".to_string()]
        );
        assert_eq!(
            parity.missing_world_control_plane_ids,
            vec!["deadman-prime".to_string()]
        );
        assert_eq!(
            parity.missing_evaluation_world_ids,
            vec!["deadman-prime".to_string()]
        );

        let parity_value =
            decode_toon_value(&parity.to_toon_document()).expect("parity document should decode");
        assert_eq!(
            parity_value["document_type"],
            "remote_topology_parity_summary"
        );
        assert!(!parity_value["payload"]["consistent"]
            .as_bool()
            .unwrap_or(true));
    }
}
