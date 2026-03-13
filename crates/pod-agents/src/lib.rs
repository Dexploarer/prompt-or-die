//! pod-agents — Production-quality advanced agent implementations for the Prompt or Die game engine.
//!
//! This crate provides four main agent types with cutting-edge async decision systems:
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
//!    - Versioned runtime schema for observation/action compatibility
//!    - Observation encoding: 32-element feature tensor from Observation
//!    - Action decoding: action index -> Action enum
//!    - Experience buffer for offline training
//!    - Both discrete and continuous control support
//!
//! 4. **HybridAgent** — LLM strategic planning + Behavior Tree frame-by-frame execution:
//!    - LLM sets high-level strategy every N ticks (configurable cadence)
//!    - BT executes reactively every tick, reads strategy from Blackboard
//!    - Trigger system for event-driven re-planning (health drops, new threats, etc.)
//!    - Factory functions: `aggressive_hybrid()`, `defensive_hybrid()`
//!
//! All agents implement the core `Agent` trait and can be mixed freely in the world.
//! None block the game tick loop — all decision-making is non-blocking.

#![allow(dead_code)]
#![allow(clippy::needless_range_loop)]
#![allow(clippy::vec_init_then_push)]
#![allow(unused_imports)]

mod llm_agent;
mod neural_agent;
pub mod onnx_network;
mod scripted_agent;
pub mod utility_ai;

// Phase 2: Enhanced Agent SDK modules
pub mod action_parser;
pub mod conversation_memory;
pub mod llm_provider;
pub mod prompt_template;

// Decision logging and replay
pub mod decision_log;

// Phase 2: Hybrid agent (Task 2.11)
pub mod hybrid_agent;

pub use llm_agent::{DecisionTrace, LlmAgent, LlmAgentConfig};
pub use neural_agent::{
    ActionSelector, Experience, GreedyActionSelector, NeuralActionSchemaEntry, NeuralAgent,
    NeuralCompatibilityStatus, NeuralInferenceStatus, NeuralModelMetadata,
    NeuralPolicyRuntimeStatus, NeuralRuntimeSchema, PolicyNetwork, RandomActionSelector,
    UniformPolicyNetwork, NEURAL_ACTION_COUNT, NEURAL_ACTION_SCHEMA, NEURAL_FEATURE_COUNT,
    NEURAL_INTERFACE_VERSION,
};
pub use scripted_agent::{
    attack_nearest, chase_nearest_hostile, chase_target, flee_from, guard, guard_with_defense,
    patrol, patrol_and_chase_on_threat, wander, BehaviorNode, BehaviorStatus, BehaviorTree,
    Blackboard, FiniteStateMachine, FsmState, FsmTransition, ScriptedAgent,
};

// Phase 2 re-exports
pub use action_parser::{
    ActionParseError, ActionParseResult, ActionParser, FallbackParser, JsonActionParser,
    KeyValueParser, ToonActionParser,
};
pub use conversation_memory::{ConversationMemory, MemoryConfig, MemoryEntry};
pub use llm_provider::{
    CompletionRequest, CompletionResponse, LlmError, LlmProvider, MockProvider, OpenAiProvider,
    TokenBudget, TokenUsage,
};
pub use prompt_template::{
    CompactTemplate, DetailedTemplate, JsonTemplate, PromptTemplate, TacticalTemplate,
    TemplateRegistry, ToonTemplate,
};

// Phase 2: Hybrid agent re-exports (Task 2.11)
pub use hybrid_agent::{
    aggressive_hybrid, defensive_hybrid, HybridAgent, HybridAgentConfig, StrategyDirective,
    StrategyTrigger,
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
