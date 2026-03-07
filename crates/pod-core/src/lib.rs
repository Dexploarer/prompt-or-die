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
pub mod orchestrator;
pub mod replay;
pub mod telemetry;
pub mod tick;
pub mod world;

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
    AgentCapabilities, AgentRole, AgentRuntimeProfile, RuntimeContractVersion,
    VersionedAgentAction, VersionedObservation, VersionedTickTelemetry,
    RUNTIME_CONTRACT_VERSION_V1,
};
pub use event::*;
pub use id::*;
pub use observation::*;
pub use observation_filter::{
    FilteredObservation, ObservationFilter, ObservationHistory, SalienceScore,
};
pub use orchestrator::{AgentBatch, AgentOrchestrator, DecisionFreshness, PriorityScore};
pub use replay::{DecisionTrace, ReplayFile, ReplayHeader, ReplayPlayer, ReplayRecorder};
pub use telemetry::{
    ActionLifecycleStage, ActionSource, AgentActionTrace, AgentTelemetryFrame, AgentToolCallTrace,
    AgentTrajectoryFrame, TelemetryArchive, TelemetryConfig, TickTelemetryFrame, ToolCallStatus,
    TrajectorySample,
};
pub use world::World;

/// Fixed tick rate — all agents operate on the same clock
pub const TICKS_PER_SECOND: u32 = 60;
pub const TICK_DURATION_SECS: f32 = 1.0 / TICKS_PER_SECOND as f32;
