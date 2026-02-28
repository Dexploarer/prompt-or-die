//! pod-agents — Production-quality advanced agent implementations for the Prompt or Die game engine.
//!
//! This crate provides three main agent types with cutting-edge async decision systems:
//!
//! 1. **LlmAgent** — LLM-powered agents (Claude, GPT, etc.) with production features:
//!    - Double-buffered async decision queue (observe → process → ready buffer)
//!    - Stale action replay with decay when waiting for LLM response
//!    - ACON-style observation compression to reduce token usage
//!    - Reaction time normalization (200-400ms) prevents inhuman response speed
//!    - Decision trace recording for deterministic replay and debugging
//!    - Batch-ready architecture for efficient multi-agent LLM calls
//!
//! 2. **ScriptedAgent** — Behavior tree and FSM based agents with full implementation:
//!    - Sequence, Selector, Parallel, Decorator nodes with proper status propagation
//!    - Blackboard for inter-node communication
//!    - Pre-built behaviors (patrol, chase, flee, guard, wander, attack)
//!    - Finite State Machine as alternative to behavior trees
//!    - Proper tree traversal with Running/Success/Failure status handling
//!
//! 3. **NeuralAgent** — Neural network policy agents with training support:
//!    - Pluggable PolicyNetwork trait for ONNX/custom models
//!    - Observation encoding: 32-element feature tensor from Observation
//!    - Action decoding: action index → Action enum
//!    - Experience buffer for offline training
//!    - Both discrete and continuous control support
//!
//! All agents implement the core `Agent` trait and can be mixed freely in the world.
//! None block the game tick loop — all decision-making is non-blocking.

mod llm_agent;
mod scripted_agent;
mod neural_agent;

pub use llm_agent::{LlmAgent, LlmAgentConfig, DecisionTrace};
pub use scripted_agent::{
    ScriptedAgent, BehaviorTree, BehaviorNode, BehaviorStatus, Blackboard,
    FiniteStateMachine, FsmState, FsmTransition,
    patrol, chase_target, chase_nearest_hostile, attack_nearest, flee_from, guard, wander,
    patrol_and_chase_on_threat, guard_with_defense,
};
pub use neural_agent::{
    NeuralAgent, ActionSelector, PolicyNetwork, Experience,
    RandomActionSelector, GreedyActionSelector, UniformPolicyNetwork,
};

/// Initialize the pod-agents module
pub fn init() {
    log::info!(
        "{} v{} initialized - Production-quality agent systems active",
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    );
}

/// Version info
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
