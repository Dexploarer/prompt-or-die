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

pub mod component;
pub mod agent;
pub mod action;
pub mod observation;
pub mod event;
pub mod world;
pub mod tick;
pub mod id;
pub mod replay;
pub mod orchestrator;
pub mod constraint;
pub mod observation_filter;

pub use component::*;
pub use agent::*;
pub use action::*;
pub use observation::*;
pub use event::*;
pub use world::World;
pub use id::*;
pub use replay::{ReplayRecorder, ReplayPlayer, ReplayFile, ReplayHeader, DecisionTrace};
pub use orchestrator::{AgentOrchestrator, AgentBatch, PriorityScore, DecisionFreshness};
pub use constraint::{
    CooldownTracker, ActionBudget, ReactionTimeGate, ConstraintProfile, ConstraintViolation,
    ValidationPipeline,
};
pub use observation_filter::{ObservationFilter, FilteredObservation, SalienceScore, ObservationHistory};

/// Fixed tick rate — all agents operate on the same clock
pub const TICKS_PER_SECOND: u32 = 60;
pub const TICK_DURATION_SECS: f32 = 1.0 / TICKS_PER_SECOND as f32;
