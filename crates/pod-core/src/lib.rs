//! # Prompt or Die — Core Engine
//!
//! Agent-native game engine where "player" is an interface, not a species.
//! Human players and AI agents operate through the same pipeline:
//! Observe → Decide → Validate → Execute → Broadcast
//!
//! The engine doesn't know or care whether an agent is human or AI.
//!
//! ## Novel Systems
//!
//! This engine includes several innovative systems discovered through research:
//!
//! - **Replay System** (`replay`): Records every AI decision for perfect reproducibility
//! - **Orchestrator** (`orchestrator`): Efficiently batches many AI agents with priority scheduling
//! - **Enhanced Constraints** (`constraint`): Production-grade validation with budgets and reaction gates
//! - **Observation Filtering** (`observation_filter`): Compresses observations to fit LLM token budgets
//! - **Telemetry** (`telemetry`): Authoritative per-tick trajectories, action traces, and tool-call primitives
//! - **Acceptance Harness** (`acceptance`): Deterministic flagship MMO scenarios for parity and shard validation

#![allow(clippy::assign_op_pattern)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::derive_ord_xor_partial_ord)]
#![allow(clippy::let_and_return)]
#![allow(clippy::manual_clamp)]
#![allow(clippy::new_without_default)]
#![allow(clippy::nonminimal_bool)]
#![allow(clippy::ptr_arg)]
#![allow(clippy::too_many_arguments)]
#![allow(clippy::type_complexity)]
#![allow(clippy::unnecessary_lazy_evaluations)]
#![allow(clippy::unused_enumerate_index)]
#![allow(clippy::unwrap_or_default)]

pub mod acceptance;
pub mod action;
pub mod agent;
pub mod app;
pub mod component;
pub mod constraint;
pub mod contract;
pub mod event;
pub mod id;
pub mod observation;
pub mod observation_filter;
pub mod ops;
pub mod orchestrator;
pub mod replay;
pub mod telemetry;
pub mod tick;
pub mod toon;
pub mod world;

pub use acceptance::{
    run_flagship_mmo_acceptance, AcceptanceParityReport, FlagshipMmoAcceptanceConfig,
    FlagshipMmoAcceptanceResult, FlagshipMmoAcceptanceSummary, FlagshipMmoScaleTarget,
};
pub use action::*;
pub use agent::*;
pub use app::{
    App, AppContext, LastTickResult, Plugin, RegisteredTypeCategory, ResourceStore, SchedulePhase,
    TypeMetadata, TypeRegistry,
};
pub use component::*;
pub use constraint::{
    ActionBudget, ConstraintProfile, ConstraintViolation, CooldownTracker, ReactionTimeGate,
    ValidationPipeline,
};
pub use contract::{
    build_remote_topology_parity_summary, build_world_quest_bindings, AgentCapabilities, AgentRole,
    AgentRuntimeProfile, AgentTeamDefinition, AppliedWorldStateSummary,
    ControllerEvaluationSummary, CrossWorldEffect, CrossWorldLinkDefinition, CrossWorldPropagation,
    EncounterSpawnEntry, FactionReputationTier, FactionReputationTrack, NamedDeltaSummary,
    ObjectiveShiftSummary, QuestLineStateSummary, QuestStageApplicationSummary,
    QuestStageDefinition, QuestStateGraph, RegionEncounterTable, RemoteTopologyBundle,
    RemoteTopologyParitySummary, RuntimeContractVersion, ScenarioEvaluationSummary,
    TeamControlMode, TeamDeathMarkSummary, TeamDeltaSummary, ToolBudget, ToolCatalog,
    ToolDefinition, ToolInvocationRequest, ToolInvocationResult, ToolPolicy,
    TournamentEliminationMode, VersionedAgentAction, VersionedObservation, VersionedTickTelemetry,
    WorldChunkDefinition, WorldEvaluationSummary, WorldQuestBinding, WorldRealityDefinition,
    WorldRealityRole, WorldRegionDefinition, WorldTournamentDefinition,
    RUNTIME_CONTRACT_VERSION_V1,
};
pub use event::*;
pub use id::*;
pub use observation::*;
pub use observation_filter::{
    FilteredObservation, ObservationFilter, ObservationHistory, SalienceScore,
};
pub use ops::{
    summarize_focused_entity_debug, ClientTransportSummary, FocusedEntityDebugSummary,
    IncidentSeverity, ShardIncidentSummary, ShardTransportSummary,
};
pub use orchestrator::{AgentBatch, AgentOrchestrator, DecisionFreshness, PriorityScore};
pub use replay::{
    ActionOutcomeSummary, DecisionTrace, EncounterTransition, ReplayFile, ReplayHeader,
    ReplayPlayer, ReplayRecorder, ReplayTrainingSample, RewardAttributionSummary,
};
pub use telemetry::{
    ActionLifecycleStage, ActionSource, AgentActionTrace, AgentRewardSignal, AgentTelemetryFrame,
    AgentTickRollup, AgentToolCallEvent, AgentToolCallTrace, AgentTrajectoryFrame, RewardReason,
    RewardSource, TelemetryArchive, TelemetryConfig, TickTelemetryFrame, ToolCallStatus,
    TrajectorySample,
};
pub use toon::{
    decode_toon_document, decode_toon_string, decode_toon_value, encode_toon_document,
    encode_toon_string,
};
pub use world::{
    ChunkPopulationState, PopulationBreakdown, RegionPopulationState, ResolvedWorldChunkMetadata,
    World, WorldPopulationState, WorldStreamingMetadata,
};

/// Fixed tick rate — all agents operate on the same clock
pub const TICKS_PER_SECOND: u32 = 60;
pub const TICK_DURATION_SECS: f32 = 1.0 / TICKS_PER_SECOND as f32;
