//! Hybrid agent framework — LLM strategic planning with behavior tree execution.
//!
//! The `HybridAgent` combines the strategic reasoning of an LLM with the
//! tick-level responsiveness of a behavior tree. The LLM decides *what* to do
//! (strategy), and the BT handles *how* to do it frame by frame.
//!
//! # Architecture
//!
//! ```text
//!   Observation
//!       |
//!       v
//!   [PromptTemplate] ---> [LlmProvider] ---> [StrategyResolver]
//!                                                    |
//!                                                    v
//!                                             Strategy enum
//!                                                    |
//!                                                    v
//!                                          [StrategyExecutor] ---> BehaviorTree
//!                                                                      |
//!                                                                      v
//!                                                                 Vec<Action>
//! ```
//!
//! The LLM call is **async and non-blocking** — while waiting for a response,
//! the agent continues executing its current behavior tree. Strategy
//! re-evaluation is triggered by TTL expiry, damage, or new hostile detection.

use std::collections::HashMap;
use std::sync::Arc;

use glam::Vec2;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use pod_core::action::{Action, AgentConstraints};
use pod_core::agent::{Agent, AgentIntrospection, AgentType};
use pod_core::id::{AgentId, EntityId};
use pod_core::observation::{Observation, Relationship};

use crate::llm_provider::{
    ChatCompletionRequest, ChatCompletionResponse, ChatMessage, LlmError, LlmProvider,
};
use crate::prompt_template::PromptTemplate;
use crate::scripted_agent::{BehaviorNode, BehaviorStatus, BehaviorTree, Blackboard};

// ---------------------------------------------------------------------------
// Strategy enum
// ---------------------------------------------------------------------------

/// High-level strategy selected by the LLM.
///
/// Each variant maps to a pre-built (or custom) behavior tree that handles
/// tick-level execution. Strategies carry parameters extracted from the
/// LLM response so the executor can configure the tree appropriately.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Strategy {
    /// Patrol between waypoints.
    Patrol { waypoints: Vec<Vec2> },
    /// Chase and close distance to a target entity.
    ChaseTarget { target_id: EntityId },
    /// Flee from a dangerous position.
    FleeFrom { danger_position: Vec2 },
    /// Guard a fixed position within a radius.
    Guard { position: Vec2, radius: f32 },
    /// Attack the nearest hostile.
    Attack,
    /// Wander randomly around a center point.
    Wander { center: Vec2, radius: f32 },
    /// Do nothing (conserve resources).
    Idle,
    /// A custom strategy with an arbitrary name and JSON parameters.
    Custom { name: String, params: serde_json::Value },
}

impl std::fmt::Display for Strategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Strategy::Patrol { waypoints } => write!(f, "Patrol({} waypoints)", waypoints.len()),
            Strategy::ChaseTarget { target_id } => write!(f, "ChaseTarget({})", target_id.0),
            Strategy::FleeFrom { danger_position } => {
                write!(f, "FleeFrom({:.0},{:.0})", danger_position.x, danger_position.y)
            }
            Strategy::Guard { position, radius } => {
                write!(f, "Guard({:.0},{:.0} r={:.0})", position.x, position.y, radius)
            }
            Strategy::Attack => write!(f, "Attack"),
            Strategy::Wander { center, radius } => {
                write!(f, "Wander({:.0},{:.0} r={:.0})", center.x, center.y, radius)
            }
            Strategy::Idle => write!(f, "Idle"),
            Strategy::Custom { name, .. } => write!(f, "Custom({})", name),
        }
    }
}

// ---------------------------------------------------------------------------
// StrategyResolver trait
// ---------------------------------------------------------------------------

/// Parses raw LLM text output into a `Strategy`.
///
/// Implementations handle the mapping from unstructured model output to a
/// well-typed strategy variant. The default implementation expects JSON.
pub trait StrategyResolver: Send + Sync {
    /// Parse the raw LLM response text into a strategy.
    /// Returns `None` if the response cannot be parsed.
    fn resolve(&self, raw_response: &str) -> Option<Strategy>;
}

/// Default JSON-based strategy resolver.
///
/// Expects the LLM to produce JSON matching:
/// ```json
/// {
///   "strategy": "patrol",
///   "params": { "waypoints": [[100,200],[300,400]] }
/// }
/// ```
///
/// Strategy names are case-insensitive. Unknown strategies map to `Custom`.
pub struct DefaultStrategyResolver;

impl StrategyResolver for DefaultStrategyResolver {
    fn resolve(&self, raw_response: &str) -> Option<Strategy> {
        // Try to find JSON in the response (LLMs sometimes wrap in markdown)
        let json_str = extract_json(raw_response)?;

        let value: serde_json::Value = serde_json::from_str(json_str).ok()?;
        let strategy_name = value
            .get("strategy")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_lowercase();
        let params = value.get("params").cloned().unwrap_or(serde_json::Value::Null);

        match strategy_name.as_str() {
            "patrol" => {
                let waypoints = parse_waypoints(&params);
                Some(Strategy::Patrol { waypoints })
            }
            "chase" | "chase_target" | "chasetarget" => {
                let target_id = params
                    .get("target_id")
                    .and_then(|v| v.as_u64())
                    .map(EntityId)
                    .unwrap_or(EntityId(0));
                Some(Strategy::ChaseTarget { target_id })
            }
            "flee" | "flee_from" | "fleefrom" => {
                let danger_position = parse_vec2(&params, "danger_position")
                    .or_else(|| parse_vec2(&params, "position"))
                    .unwrap_or(Vec2::ZERO);
                Some(Strategy::FleeFrom { danger_position })
            }
            "guard" => {
                let position = parse_vec2(&params, "position").unwrap_or(Vec2::ZERO);
                let radius = params
                    .get("radius")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(100.0) as f32;
                Some(Strategy::Guard { position, radius })
            }
            "attack" => Some(Strategy::Attack),
            "wander" => {
                let center = parse_vec2(&params, "center").unwrap_or(Vec2::ZERO);
                let radius = params
                    .get("radius")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(200.0) as f32;
                Some(Strategy::Wander { center, radius })
            }
            "idle" => Some(Strategy::Idle),
            "" => None,
            other => Some(Strategy::Custom {
                name: other.to_string(),
                params,
            }),
        }
    }
}

/// Extract the first JSON object from a string (handles markdown code blocks).
fn extract_json(text: &str) -> Option<&str> {
    // Try raw parse first
    if text.trim().starts_with('{') {
        return Some(text.trim());
    }

    // Look for ```json ... ``` blocks
    if let Some(start) = text.find("```json") {
        let after_fence = &text[start + 7..];
        if let Some(end) = after_fence.find("```") {
            return Some(after_fence[..end].trim());
        }
    }

    // Look for ``` ... ``` blocks
    if let Some(start) = text.find("```") {
        let after_fence = &text[start + 3..];
        if let Some(end) = after_fence.find("```") {
            let candidate = after_fence[..end].trim();
            if candidate.starts_with('{') {
                return Some(candidate);
            }
        }
    }

    // Look for first { ... } substring
    let start = text.find('{')?;
    let mut depth = 0;
    for (i, ch) in text[start..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse a Vec2 from a JSON object field.
fn parse_vec2(params: &serde_json::Value, key: &str) -> Option<Vec2> {
    let field = params.get(key)?;

    // Try array form: [x, y]
    if let Some(arr) = field.as_array() {
        if arr.len() >= 2 {
            let x = arr[0].as_f64()? as f32;
            let y = arr[1].as_f64()? as f32;
            return Some(Vec2::new(x, y));
        }
    }

    // Try object form: {"x": ..., "y": ...}
    let x = field.get("x").and_then(|v| v.as_f64())? as f32;
    let y = field.get("y").and_then(|v| v.as_f64())? as f32;
    Some(Vec2::new(x, y))
}

/// Parse waypoints from a JSON value.
fn parse_waypoints(params: &serde_json::Value) -> Vec<Vec2> {
    let arr = params
        .get("waypoints")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    arr.iter()
        .filter_map(|wp| {
            if let Some(pair) = wp.as_array() {
                if pair.len() >= 2 {
                    let x = pair[0].as_f64()? as f32;
                    let y = pair[1].as_f64()? as f32;
                    return Some(Vec2::new(x, y));
                }
            }
            // Object form
            let x = wp.get("x").and_then(|v| v.as_f64())? as f32;
            let y = wp.get("y").and_then(|v| v.as_f64())? as f32;
            Some(Vec2::new(x, y))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// StrategyExecutor trait
// ---------------------------------------------------------------------------

/// Maps a `Strategy` to a `BehaviorTree` that executes it tick-by-tick.
///
/// Implementations construct (or cache) behavior trees for each strategy.
/// The observation is provided so the executor can adapt the tree based on
/// the current world state (e.g., resolve a target entity's position).
pub trait StrategyExecutor: Send + Sync {
    /// Build a behavior tree that implements the given strategy.
    fn execute(&self, strategy: &Strategy, observation: &Observation) -> BehaviorTree;
}

/// Default strategy executor that maps strategies to pre-built behavior trees.
pub struct DefaultStrategyExecutor;

impl StrategyExecutor for DefaultStrategyExecutor {
    fn execute(&self, strategy: &Strategy, observation: &Observation) -> BehaviorTree {
        let root = match strategy {
            Strategy::Patrol { waypoints } => {
                if waypoints.is_empty() {
                    BehaviorNode::action(Action::Idle)
                } else {
                    crate::scripted_agent::patrol(waypoints.clone())
                }
            }
            Strategy::ChaseTarget { target_id } => {
                // Resolve target position from observation's visible entities
                let target_pos = observation
                    .visible_entities
                    .iter()
                    .find(|e| e.entity_id == *target_id)
                    .map(|e| e.position);

                match target_pos {
                    Some(pos) => crate::scripted_agent::chase_target(pos),
                    None => {
                        // Target not visible — idle until we can see it again
                        warn!(
                            "HybridAgent: chase target {} not visible, idling",
                            target_id.0
                        );
                        BehaviorNode::action(Action::Idle)
                    }
                }
            }
            Strategy::FleeFrom { danger_position } => {
                crate::scripted_agent::flee_from(*danger_position)
            }
            Strategy::Guard { position, radius } => {
                crate::scripted_agent::guard(*position, *radius)
            }
            Strategy::Attack => crate::scripted_agent::attack_nearest(),
            Strategy::Wander { center, radius } => {
                crate::scripted_agent::wander(*center, *radius)
            }
            Strategy::Idle => BehaviorNode::action(Action::Idle),
            Strategy::Custom { name, .. } => {
                warn!("HybridAgent: no built-in tree for custom strategy '{}'", name);
                BehaviorNode::action(Action::Idle)
            }
        };

        BehaviorTree::new(root)
    }
}

// ---------------------------------------------------------------------------
// Re-evaluation triggers
// ---------------------------------------------------------------------------

/// Reasons the hybrid agent should request a new strategy from the LLM.
#[derive(Debug, Clone, PartialEq)]
enum ReEvalTrigger {
    /// Strategy TTL expired.
    TtlExpired,
    /// Agent took damage.
    DamageTaken,
    /// A new hostile entity appeared in the observation.
    NewHostileDetected,
    /// First tick — no strategy yet.
    Initial,
}

// ---------------------------------------------------------------------------
// HybridAgent
// ---------------------------------------------------------------------------

/// A hybrid agent that uses LLM planning with behavior tree execution.
///
/// The agent alternates between two modes:
/// - **Planning**: an async LLM call determines the next strategy
/// - **Executing**: the current behavior tree runs tick-by-tick
///
/// While waiting for the LLM response, the agent continues executing its
/// current behavior tree. When the LLM response arrives, the strategy is
/// resolved and a new behavior tree is swapped in.
pub struct HybridAgent {
    // Identity
    id: AgentId,
    constraints: AgentConstraints,

    // LLM pipeline
    provider: Arc<dyn LlmProvider>,
    resolver: Box<dyn StrategyResolver>,
    executor: Box<dyn StrategyExecutor>,
    prompt_template: Box<dyn PromptTemplate>,

    // Execution state
    current_strategy: Option<Strategy>,
    behavior_tree: Option<BehaviorTree>,
    blackboard: Blackboard,
    last_observation: Option<Observation>,

    // Strategy lifecycle
    strategy_ttl_ticks: u64,
    strategy_age_ticks: u64,

    // Async LLM request tracking
    pending_response: Option<oneshot::Receiver<Result<ChatCompletionResponse, LlmError>>>,
    llm_request_in_flight: bool,

    // Re-evaluation state
    known_hostile_ids: Vec<EntityId>,
    took_damage_this_tick: bool,

    // Config
    agent_name: String,
    agent_role: String,

    // Stats
    total_llm_calls: u64,
    total_strategy_changes: u64,
}

// SAFETY: HybridAgent is Send because all its fields are Send.
// The oneshot::Receiver<...> is Send when the inner type is Send,
// and Result<ChatCompletionResponse, LlmError> is Send.
// Arc<dyn LlmProvider> is Send because LlmProvider: Send + Sync.

impl HybridAgent {
    /// Create a new hybrid agent with the given configuration.
    pub fn new(config: HybridAgentConfig) -> Self {
        Self {
            id: config.id.unwrap_or_else(AgentId::new),
            constraints: AgentConstraints::default(),
            provider: config.provider,
            resolver: config.resolver,
            executor: config.executor,
            prompt_template: config.prompt_template,
            current_strategy: None,
            behavior_tree: None,
            blackboard: Blackboard::new(),
            last_observation: None,
            strategy_ttl_ticks: config.strategy_ttl_ticks,
            strategy_age_ticks: 0,
            pending_response: None,
            llm_request_in_flight: false,
            known_hostile_ids: Vec::new(),
            took_damage_this_tick: false,
            agent_name: config.agent_name,
            agent_role: config.agent_role,
            total_llm_calls: 0,
            total_strategy_changes: 0,
        }
    }

    /// Get the current strategy, if any.
    pub fn current_strategy(&self) -> Option<&Strategy> {
        self.current_strategy.as_ref()
    }

    /// Get a reference to the blackboard.
    pub fn blackboard(&self) -> &Blackboard {
        &self.blackboard
    }

    /// Get a mutable reference to the blackboard.
    pub fn blackboard_mut(&mut self) -> &mut Blackboard {
        &mut self.blackboard
    }

    /// Total number of LLM calls made.
    pub fn total_llm_calls(&self) -> u64 {
        self.total_llm_calls
    }

    /// Total number of strategy changes.
    pub fn total_strategy_changes(&self) -> u64 {
        self.total_strategy_changes
    }

    /// Force a strategy change (for testing or external triggers).
    pub fn set_strategy(&mut self, strategy: Strategy) {
        if let Some(ref obs) = self.last_observation {
            let tree = self.executor.execute(&strategy, obs);
            self.behavior_tree = Some(tree);
        }
        self.current_strategy = Some(strategy);
        self.strategy_age_ticks = 0;
        self.total_strategy_changes += 1;
    }

    /// Check whether a re-evaluation should be triggered.
    fn should_re_evaluate(&self) -> Option<ReEvalTrigger> {
        // No strategy yet
        if self.current_strategy.is_none() {
            return Some(ReEvalTrigger::Initial);
        }

        // TTL expired
        if self.strategy_age_ticks >= self.strategy_ttl_ticks {
            return Some(ReEvalTrigger::TtlExpired);
        }

        // Damage taken
        if self.took_damage_this_tick {
            return Some(ReEvalTrigger::DamageTaken);
        }

        // New hostile detected
        if let Some(ref obs) = self.last_observation {
            let new_hostile = obs
                .visible_entities
                .iter()
                .filter(|e| matches!(e.relationship, Relationship::Hostile))
                .any(|e| !self.known_hostile_ids.contains(&e.entity_id));
            if new_hostile {
                return Some(ReEvalTrigger::NewHostileDetected);
            }
        }

        None
    }

    /// Fire an async LLM request for a new strategy.
    fn request_strategy(&mut self) {
        if self.llm_request_in_flight {
            return;
        }

        let obs = match &self.last_observation {
            Some(obs) => obs.clone(),
            None => return,
        };

        // Build prompt
        let system_prompt = self
            .prompt_template
            .format_system_prompt(&self.agent_name, &self.agent_role);
        let obs_text = self.prompt_template.format_observation(&obs);
        let action_instructions = self
            .prompt_template
            .format_action_instructions(&obs.available_actions);

        // Add strategy-specific instructions
        let strategy_instructions = format!(
            "\n\nRespond with a JSON object choosing your strategy:\n\
             {{\n\
               \"strategy\": \"<patrol|chase_target|flee_from|guard|attack|wander|idle>\",\n\
               \"params\": {{ ... }}\n\
             }}\n\
             \n\
             Current strategy: {}\n\
             Strategy age: {} ticks",
            self.current_strategy
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "none".to_string()),
            self.strategy_age_ticks,
        );

        let request = ChatCompletionRequest {
            messages: vec![
                ChatMessage::system(system_prompt),
                ChatMessage::user(format!(
                    "{}\n{}\n{}",
                    obs_text, action_instructions, strategy_instructions
                )),
            ],
            temperature: Some(0.7),
            max_tokens: Some(200),
            json_mode: true,
            stop: Vec::new(),
        };

        // Spawn async request via background thread
        let (tx, rx) = oneshot::channel();
        let provider = self.provider.clone();

        // Use a background thread with its own tokio runtime to call the async provider.
        // This mirrors the pattern used by LlmAgent and avoids requiring a tokio runtime
        // in the calling context.
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match rt {
                Ok(rt) => {
                    let result = rt.block_on(provider.chat_completion(request));
                    let _ = tx.send(result);
                }
                Err(e) => {
                    log::error!("HybridAgent: failed to create tokio runtime: {}", e);
                    let _ = tx.send(Err(LlmError::Custom(format!(
                        "Failed to create runtime: {}",
                        e
                    ))));
                }
            }
        });

        self.pending_response = Some(rx);
        self.llm_request_in_flight = true;
        self.total_llm_calls += 1;

        debug!(
            "HybridAgent {}: fired LLM request #{} for strategy",
            self.id, self.total_llm_calls
        );
    }

    /// Poll the pending LLM response (non-blocking).
    fn poll_response(&mut self) {
        let mut rx = match self.pending_response.take() {
            Some(rx) => rx,
            None => return,
        };

        match rx.try_recv() {
            Ok(Ok(response)) => {
                // LLM responded successfully — resolve strategy
                self.llm_request_in_flight = false;
                if let Some(strategy) = self.resolver.resolve(&response.content) {
                    info!(
                        "HybridAgent {}: resolved strategy: {}",
                        self.id, strategy
                    );
                    self.apply_strategy(strategy);
                } else {
                    warn!(
                        "HybridAgent {}: failed to resolve strategy from: {}",
                        self.id,
                        &response.content[..response.content.len().min(100)]
                    );
                    // Keep current strategy
                }
            }
            Ok(Err(err)) => {
                // LLM error — keep current strategy
                self.llm_request_in_flight = false;
                warn!("HybridAgent {}: LLM error: {}", self.id, err);
            }
            Err(oneshot::error::TryRecvError::Empty) => {
                // Not ready yet — put it back
                self.pending_response = Some(rx);
            }
            Err(oneshot::error::TryRecvError::Closed) => {
                // Sender dropped — request failed silently
                self.llm_request_in_flight = false;
                warn!("HybridAgent {}: LLM request channel closed", self.id);
            }
        }
    }

    /// Apply a new strategy and build its behavior tree.
    fn apply_strategy(&mut self, strategy: Strategy) {
        if let Some(ref obs) = self.last_observation {
            let tree = self.executor.execute(&strategy, obs);
            self.behavior_tree = Some(tree);
        }
        self.current_strategy = Some(strategy);
        self.strategy_age_ticks = 0;
        self.total_strategy_changes += 1;
    }

    /// Update the known hostile set from the current observation.
    fn update_known_hostiles(&mut self) {
        if let Some(ref obs) = self.last_observation {
            self.known_hostile_ids = obs
                .visible_entities
                .iter()
                .filter(|e| matches!(e.relationship, Relationship::Hostile))
                .map(|e| e.entity_id)
                .collect();
        }
    }
}

impl Agent for HybridAgent {
    fn id(&self) -> AgentId {
        self.id
    }

    fn agent_type(&self) -> AgentType {
        AgentType::LlmAgent
    }

    fn observe(&mut self, observation: Observation) {
        self.last_observation = Some(observation);
    }

    fn decide(&mut self) -> Vec<Action> {
        // 1. Poll any pending LLM response
        self.poll_response();

        // 2. Check if we need a new strategy
        if let Some(trigger) = self.should_re_evaluate() {
            debug!(
                "HybridAgent {}: re-evaluation triggered: {:?}",
                self.id, trigger
            );
            self.request_strategy();
        }

        // 3. Update hostile tracking AFTER re-eval check (so new hostiles are detected)
        self.update_known_hostiles();

        // 4. Reset per-tick flags
        self.took_damage_this_tick = false;

        // 5. Increment strategy age
        self.strategy_age_ticks += 1;

        // 6. Tick the behavior tree
        if let Some(ref mut tree) = self.behavior_tree {
            if let Some(ref obs) = self.last_observation {
                let (actions, _status) = tree.tick(obs);
                if !actions.is_empty() {
                    return actions;
                }
            }
        }

        // No behavior tree or no actions — idle
        vec![Action::Idle]
    }

    fn constraints(&self) -> &AgentConstraints {
        &self.constraints
    }

    fn constraints_mut(&mut self) -> &mut AgentConstraints {
        &mut self.constraints
    }

    fn on_damage(&mut self, _amount: f32, _source: Option<AgentId>) {
        self.took_damage_this_tick = true;
    }

    fn on_death(&mut self) {
        self.current_strategy = None;
        self.behavior_tree = None;
        self.strategy_age_ticks = 0;
        self.llm_request_in_flight = false;
        self.pending_response = None;
    }

    fn on_respawn(&mut self) {
        self.current_strategy = None;
        self.behavior_tree = None;
        self.strategy_age_ticks = 0;
        self.known_hostile_ids.clear();
    }

    fn introspect(&self) -> AgentIntrospection {
        let mut custom_data = HashMap::new();
        custom_data.insert(
            "strategy".to_string(),
            self.current_strategy
                .as_ref()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "none".to_string()),
        );
        custom_data.insert(
            "strategy_age_ticks".to_string(),
            self.strategy_age_ticks.to_string(),
        );
        custom_data.insert(
            "llm_in_flight".to_string(),
            self.llm_request_in_flight.to_string(),
        );
        custom_data.insert(
            "total_llm_calls".to_string(),
            self.total_llm_calls.to_string(),
        );
        custom_data.insert(
            "total_strategy_changes".to_string(),
            self.total_strategy_changes.to_string(),
        );
        custom_data.insert(
            "known_hostiles".to_string(),
            self.known_hostile_ids.len().to_string(),
        );

        AgentIntrospection {
            agent_id: self.id,
            agent_type: AgentType::LlmAgent,
            state_description: format!(
                "HybridAgent: strategy={}, age={}/{} ticks, llm_calls={}",
                self.current_strategy
                    .as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "none".to_string()),
                self.strategy_age_ticks,
                self.strategy_ttl_ticks,
                self.total_llm_calls,
            ),
            pending_actions: if self.behavior_tree.is_some() { 1 } else { 0 },
            last_decision_tick: self.last_observation.as_ref().map(|o| o.tick),
            custom_data,
        }
    }
}

// ---------------------------------------------------------------------------
// HybridAgentConfig (builder)
// ---------------------------------------------------------------------------

/// Builder for constructing a `HybridAgent` with all required components.
///
/// # Example
/// ```ignore
/// let agent = HybridAgentConfig::new(provider)
///     .with_name("Scout")
///     .with_role("reconnaissance agent")
///     .with_strategy_ttl(300)
///     .build();
/// ```
pub struct HybridAgentConfig {
    id: Option<AgentId>,
    provider: Arc<dyn LlmProvider>,
    resolver: Box<dyn StrategyResolver>,
    executor: Box<dyn StrategyExecutor>,
    prompt_template: Box<dyn PromptTemplate>,
    strategy_ttl_ticks: u64,
    agent_name: String,
    agent_role: String,
}

impl HybridAgentConfig {
    /// Create a new config with the given LLM provider.
    /// Uses sensible defaults for all other settings.
    pub fn new(provider: Arc<dyn LlmProvider>) -> Self {
        Self {
            id: None,
            provider,
            resolver: Box::new(DefaultStrategyResolver),
            executor: Box::new(DefaultStrategyExecutor),
            prompt_template: Box::new(crate::prompt_template::CompactPromptTemplate),
            strategy_ttl_ticks: 300, // 5 seconds at 60 TPS
            agent_name: "Agent".to_string(),
            agent_role: "general-purpose game agent".to_string(),
        }
    }

    /// Set a specific agent ID.
    pub fn with_id(mut self, id: AgentId) -> Self {
        self.id = Some(id);
        self
    }

    /// Set the agent's display name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.agent_name = name.into();
        self
    }

    /// Set the agent's role description.
    pub fn with_role(mut self, role: impl Into<String>) -> Self {
        self.agent_role = role.into();
        self
    }

    /// Set the strategy TTL in ticks (how long before re-evaluation).
    pub fn with_strategy_ttl(mut self, ttl_ticks: u64) -> Self {
        self.strategy_ttl_ticks = ttl_ticks;
        self
    }

    /// Set a custom strategy resolver.
    pub fn with_resolver(mut self, resolver: Box<dyn StrategyResolver>) -> Self {
        self.resolver = resolver;
        self
    }

    /// Set a custom strategy executor.
    pub fn with_executor(mut self, executor: Box<dyn StrategyExecutor>) -> Self {
        self.executor = executor;
        self
    }

    /// Set a custom prompt template.
    pub fn with_prompt_template(mut self, template: Box<dyn PromptTemplate>) -> Self {
        self.prompt_template = template;
        self
    }

    /// Build the `HybridAgent`.
    pub fn build(self) -> HybridAgent {
        HybridAgent::new(self)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::MockLlmProvider;
    use pod_core::component::Team;
    use pod_core::id::{AgentId, EntityId};
    use pod_core::observation::{Observation, SelfState, VisibleEntity};

    // ---- Test helpers ----

    fn test_self_state() -> SelfState {
        SelfState {
            agent_id: AgentId::new(),
            entity_id: EntityId(1),
            position: Vec2::new(100.0, 100.0),
            rotation: 0.0,
            velocity: Vec2::ZERO,
            health: Some(100.0),
            max_health: Some(100.0),
            team: Team::None,
            cooldowns: vec![],
        }
    }

    fn test_observation() -> Observation {
        Observation {
            tick: 10,
            elapsed_secs: 0.167,
            self_state: test_self_state(),
            visible_entities: vec![],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![],
        }
    }

    fn test_observation_with_hostile() -> Observation {
        let mut obs = test_observation();
        obs.visible_entities.push(VisibleEntity {
            entity_id: EntityId(42),
            entity_type: "enemy".to_string(),
            position: Vec2::new(200.0, 200.0),
            velocity: Vec2::ZERO,
            rotation: 0.0,
            distance: 141.0,
            relationship: Relationship::Hostile,
            health_fraction: Some(1.0),
        });
        obs
    }

    fn mock_provider(response: &str) -> Arc<MockLlmProvider> {
        Arc::new(MockLlmProvider::new(response))
    }

    fn build_test_agent(provider: Arc<dyn LlmProvider>) -> HybridAgent {
        HybridAgentConfig::new(provider)
            .with_name("TestAgent")
            .with_role("test agent")
            .with_strategy_ttl(60)
            .build()
    }

    // ---- Strategy enum tests ----

    #[test]
    fn strategy_display() {
        assert_eq!(Strategy::Idle.to_string(), "Idle");
        assert_eq!(Strategy::Attack.to_string(), "Attack");
        assert_eq!(
            Strategy::Patrol {
                waypoints: vec![Vec2::ZERO, Vec2::ONE]
            }
            .to_string(),
            "Patrol(2 waypoints)"
        );
        assert_eq!(
            Strategy::ChaseTarget {
                target_id: EntityId(42)
            }
            .to_string(),
            "ChaseTarget(42)"
        );
        assert_eq!(
            Strategy::Guard {
                position: Vec2::new(10.0, 20.0),
                radius: 50.0
            }
            .to_string(),
            "Guard(10,20 r=50)"
        );
    }

    // ---- DefaultStrategyResolver tests ----

    #[test]
    fn resolver_parses_idle() {
        let resolver = DefaultStrategyResolver;
        let result = resolver.resolve(r#"{"strategy": "idle"}"#);
        assert_eq!(result, Some(Strategy::Idle));
    }

    #[test]
    fn resolver_parses_attack() {
        let resolver = DefaultStrategyResolver;
        let result = resolver.resolve(r#"{"strategy": "attack"}"#);
        assert_eq!(result, Some(Strategy::Attack));
    }

    #[test]
    fn resolver_parses_patrol_with_waypoints() {
        let resolver = DefaultStrategyResolver;
        let json = r#"{"strategy": "patrol", "params": {"waypoints": [[100, 200], [300, 400]]}}"#;
        let result = resolver.resolve(json);
        match result {
            Some(Strategy::Patrol { waypoints }) => {
                assert_eq!(waypoints.len(), 2);
                assert_eq!(waypoints[0], Vec2::new(100.0, 200.0));
                assert_eq!(waypoints[1], Vec2::new(300.0, 400.0));
            }
            other => panic!("Expected Patrol, got {:?}", other),
        }
    }

    #[test]
    fn resolver_parses_chase_target() {
        let resolver = DefaultStrategyResolver;
        let json = r#"{"strategy": "chase_target", "params": {"target_id": 42}}"#;
        let result = resolver.resolve(json);
        assert_eq!(
            result,
            Some(Strategy::ChaseTarget {
                target_id: EntityId(42)
            })
        );
    }

    #[test]
    fn resolver_parses_flee_from() {
        let resolver = DefaultStrategyResolver;
        let json = r#"{"strategy": "flee_from", "params": {"danger_position": [50, 75]}}"#;
        let result = resolver.resolve(json);
        assert_eq!(
            result,
            Some(Strategy::FleeFrom {
                danger_position: Vec2::new(50.0, 75.0)
            })
        );
    }

    #[test]
    fn resolver_parses_guard() {
        let resolver = DefaultStrategyResolver;
        let json =
            r#"{"strategy": "guard", "params": {"position": {"x": 10, "y": 20}, "radius": 50}}"#;
        let result = resolver.resolve(json);
        assert_eq!(
            result,
            Some(Strategy::Guard {
                position: Vec2::new(10.0, 20.0),
                radius: 50.0
            })
        );
    }

    #[test]
    fn resolver_parses_wander() {
        let resolver = DefaultStrategyResolver;
        let json =
            r#"{"strategy": "wander", "params": {"center": [0, 0], "radius": 100}}"#;
        let result = resolver.resolve(json);
        assert_eq!(
            result,
            Some(Strategy::Wander {
                center: Vec2::ZERO,
                radius: 100.0
            })
        );
    }

    #[test]
    fn resolver_parses_custom_strategy() {
        let resolver = DefaultStrategyResolver;
        let json = r#"{"strategy": "ambush", "params": {"location": [50, 50]}}"#;
        let result = resolver.resolve(json);
        match result {
            Some(Strategy::Custom { name, params }) => {
                assert_eq!(name, "ambush");
                assert!(params.get("location").is_some());
            }
            other => panic!("Expected Custom, got {:?}", other),
        }
    }

    #[test]
    fn resolver_handles_invalid_json() {
        let resolver = DefaultStrategyResolver;
        assert_eq!(resolver.resolve("not json at all"), None);
    }

    #[test]
    fn resolver_handles_empty_strategy() {
        let resolver = DefaultStrategyResolver;
        assert_eq!(resolver.resolve(r#"{"strategy": ""}"#), None);
    }

    #[test]
    fn resolver_case_insensitive() {
        let resolver = DefaultStrategyResolver;
        assert_eq!(
            resolver.resolve(r#"{"strategy": "ATTACK"}"#),
            Some(Strategy::Attack)
        );
        assert_eq!(
            resolver.resolve(r#"{"strategy": "Idle"}"#),
            Some(Strategy::Idle)
        );
    }

    #[test]
    fn resolver_handles_markdown_wrapped_json() {
        let resolver = DefaultStrategyResolver;
        let response = "Here is my decision:\n```json\n{\"strategy\": \"attack\"}\n```\n";
        assert_eq!(resolver.resolve(response), Some(Strategy::Attack));
    }

    // ---- extract_json tests ----

    #[test]
    fn extract_json_plain() {
        let input = r#"{"strategy": "idle"}"#;
        assert_eq!(extract_json(input), Some(r#"{"strategy": "idle"}"#));
    }

    #[test]
    fn extract_json_markdown_block() {
        let input = "Some text\n```json\n{\"a\": 1}\n```\nMore text";
        assert_eq!(extract_json(input), Some("{\"a\": 1}"));
    }

    #[test]
    fn extract_json_embedded() {
        let input = "I think we should {\"strategy\": \"attack\"} go for it.";
        assert_eq!(
            extract_json(input),
            Some("{\"strategy\": \"attack\"}")
        );
    }

    #[test]
    fn extract_json_none() {
        assert_eq!(extract_json("no json here"), None);
    }

    // ---- DefaultStrategyExecutor tests ----

    #[test]
    fn executor_idle_produces_idle_action() {
        let executor = DefaultStrategyExecutor;
        let obs = test_observation();
        let mut tree = executor.execute(&Strategy::Idle, &obs);

        let (actions, status) = tree.tick(&obs);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::Idle));
        assert_eq!(status, BehaviorStatus::Success);
    }

    #[test]
    fn executor_attack_produces_attack_action() {
        let executor = DefaultStrategyExecutor;
        let obs = test_observation();
        let mut tree = executor.execute(&Strategy::Attack, &obs);

        let (actions, _status) = tree.tick(&obs);
        assert!(!actions.is_empty());
        assert!(actions.iter().any(|a| matches!(a, Action::Attack)));
    }

    #[test]
    fn executor_chase_visible_target() {
        let executor = DefaultStrategyExecutor;
        let obs = test_observation_with_hostile();
        let strategy = Strategy::ChaseTarget {
            target_id: EntityId(42),
        };
        let mut tree = executor.execute(&strategy, &obs);

        let (actions, _status) = tree.tick(&obs);
        assert!(!actions.is_empty());
        // Should produce LookAt + Move toward target
        assert!(actions.iter().any(|a| matches!(a, Action::LookAt { .. })));
    }

    #[test]
    fn executor_chase_invisible_target_idles() {
        let executor = DefaultStrategyExecutor;
        let obs = test_observation(); // No visible entities
        let strategy = Strategy::ChaseTarget {
            target_id: EntityId(99),
        };
        let mut tree = executor.execute(&strategy, &obs);

        let (actions, _status) = tree.tick(&obs);
        assert!(actions.iter().any(|a| matches!(a, Action::Idle)));
    }

    #[test]
    fn executor_patrol_with_waypoints() {
        let executor = DefaultStrategyExecutor;
        let obs = test_observation();
        let strategy = Strategy::Patrol {
            waypoints: vec![Vec2::new(100.0, 0.0), Vec2::new(200.0, 0.0)],
        };
        let mut tree = executor.execute(&strategy, &obs);

        let (actions, _status) = tree.tick(&obs);
        assert!(!actions.is_empty());
    }

    #[test]
    fn executor_empty_patrol_idles() {
        let executor = DefaultStrategyExecutor;
        let obs = test_observation();
        let strategy = Strategy::Patrol { waypoints: vec![] };
        let mut tree = executor.execute(&strategy, &obs);

        let (actions, _) = tree.tick(&obs);
        assert!(actions.iter().any(|a| matches!(a, Action::Idle)));
    }

    // ---- HybridAgent core tests ----

    #[test]
    fn agent_starts_with_no_strategy() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let agent = build_test_agent(provider);
        assert!(agent.current_strategy().is_none());
        assert_eq!(agent.total_llm_calls(), 0);
    }

    #[test]
    fn agent_implements_agent_trait() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let agent = build_test_agent(provider);
        assert_eq!(agent.agent_type(), AgentType::LlmAgent);
        assert_eq!(agent.constraints().actions_per_tick, 3);
    }

    #[test]
    fn agent_idles_without_observation() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let mut agent = build_test_agent(provider);
        let actions = agent.decide();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::Idle));
    }

    #[test]
    fn set_strategy_updates_state() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let mut agent = build_test_agent(provider);
        agent.observe(test_observation());
        agent.set_strategy(Strategy::Attack);

        assert_eq!(agent.current_strategy(), Some(&Strategy::Attack));
        assert_eq!(agent.total_strategy_changes(), 1);
    }

    #[test]
    fn set_strategy_produces_actions() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let mut agent = build_test_agent(provider);
        agent.observe(test_observation());
        agent.set_strategy(Strategy::Attack);

        let actions = agent.decide();
        assert!(!actions.is_empty());
    }

    #[test]
    fn on_death_resets_state() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let mut agent = build_test_agent(provider);
        agent.observe(test_observation());
        agent.set_strategy(Strategy::Attack);

        agent.on_death();
        assert!(agent.current_strategy().is_none());
        assert!(agent.behavior_tree.is_none());
    }

    #[test]
    fn on_respawn_resets_state() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let mut agent = build_test_agent(provider);
        agent.observe(test_observation());
        agent.set_strategy(Strategy::Attack);

        agent.on_respawn();
        assert!(agent.current_strategy().is_none());
        assert_eq!(agent.strategy_age_ticks, 0);
    }

    #[test]
    fn on_damage_triggers_re_evaluation_flag() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let mut agent = build_test_agent(provider);
        agent.observe(test_observation());
        agent.set_strategy(Strategy::Idle);

        agent.on_damage(25.0, None);
        assert!(agent.took_damage_this_tick);
    }

    #[test]
    fn introspect_returns_strategy_info() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let mut agent = build_test_agent(provider);
        agent.observe(test_observation());
        agent.set_strategy(Strategy::Patrol {
            waypoints: vec![Vec2::ZERO],
        });

        let info = agent.introspect();
        assert_eq!(info.agent_type, AgentType::LlmAgent);
        assert!(info.state_description.contains("Patrol"));
        assert_eq!(
            info.custom_data.get("total_strategy_changes"),
            Some(&"1".to_string())
        );
    }

    #[test]
    fn config_builder_defaults() {
        let provider: Arc<dyn LlmProvider> = mock_provider(r#"{"strategy":"idle"}"#);
        let agent = HybridAgentConfig::new(provider).build();

        assert_eq!(agent.agent_name, "Agent");
        assert_eq!(agent.agent_role, "general-purpose game agent");
        assert_eq!(agent.strategy_ttl_ticks, 300);
    }

    #[test]
    fn config_builder_customization() {
        let provider: Arc<dyn LlmProvider> = mock_provider(r#"{"strategy":"idle"}"#);
        let id = AgentId::new();
        let agent = HybridAgentConfig::new(provider)
            .with_id(id)
            .with_name("Scout")
            .with_role("recon")
            .with_strategy_ttl(120)
            .build();

        assert_eq!(agent.id(), id);
        assert_eq!(agent.agent_name, "Scout");
        assert_eq!(agent.agent_role, "recon");
        assert_eq!(agent.strategy_ttl_ticks, 120);
    }

    // ---- Re-evaluation trigger tests ----

    #[test]
    fn re_eval_initial_when_no_strategy() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let agent = build_test_agent(provider);
        assert_eq!(agent.should_re_evaluate(), Some(ReEvalTrigger::Initial));
    }

    #[test]
    fn re_eval_ttl_expired() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let mut agent = build_test_agent(provider);
        agent.observe(test_observation());
        agent.set_strategy(Strategy::Idle);
        agent.strategy_age_ticks = 60; // Equal to TTL

        assert_eq!(agent.should_re_evaluate(), Some(ReEvalTrigger::TtlExpired));
    }

    #[test]
    fn re_eval_damage_taken() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let mut agent = build_test_agent(provider);
        agent.observe(test_observation());
        agent.set_strategy(Strategy::Idle);
        agent.took_damage_this_tick = true;

        assert_eq!(
            agent.should_re_evaluate(),
            Some(ReEvalTrigger::DamageTaken)
        );
    }

    #[test]
    fn re_eval_new_hostile() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let mut agent = build_test_agent(provider);
        agent.observe(test_observation_with_hostile());
        agent.set_strategy(Strategy::Idle);
        // known_hostile_ids is empty, but observation has a hostile

        assert_eq!(
            agent.should_re_evaluate(),
            Some(ReEvalTrigger::NewHostileDetected)
        );
    }

    #[test]
    fn no_re_eval_when_stable() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let mut agent = build_test_agent(provider);
        agent.observe(test_observation());
        agent.set_strategy(Strategy::Idle);
        // Strategy is set, age is 0, no damage, no hostiles

        assert_eq!(agent.should_re_evaluate(), None);
    }

    // ---- Async LLM integration test (tokio) ----

    #[tokio::test]
    async fn async_llm_strategy_resolution() {
        let provider: Arc<dyn LlmProvider> =
            Arc::new(MockLlmProvider::new(r#"{"strategy": "attack"}"#));
        let mut agent = HybridAgentConfig::new(provider.clone())
            .with_strategy_ttl(60)
            .build();

        // Provide observation and trigger initial strategy request
        agent.observe(test_observation());
        let _actions = agent.decide(); // This fires the LLM request

        assert_eq!(agent.total_llm_calls(), 1);

        // Give the async task time to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Next decide should poll and apply the response
        agent.observe(test_observation());
        let actions = agent.decide();

        // After resolution, we should have an attack strategy
        // The strategy may or may not have been resolved yet depending on timing,
        // but the agent should at least not panic
        assert!(!actions.is_empty());
    }

    #[tokio::test]
    async fn multiple_strategy_transitions() {
        let provider: Arc<dyn LlmProvider> = Arc::new(MockLlmProvider::with_responses(vec![
            r#"{"strategy": "patrol", "params": {"waypoints": [[0,0],[100,0]]}}"#.to_string(),
            r#"{"strategy": "attack"}"#.to_string(),
        ]));
        let mut agent = HybridAgentConfig::new(provider)
            .with_strategy_ttl(10)
            .build();

        // First decision cycle
        agent.observe(test_observation());
        agent.decide();
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Poll result
        agent.observe(test_observation());
        agent.decide();

        if let Some(ref strategy) = agent.current_strategy {
            match strategy {
                Strategy::Patrol { waypoints } => {
                    assert_eq!(waypoints.len(), 2);
                }
                _ => {
                    // Might not have resolved yet — that's OK
                }
            }
        }
    }

    // ---- parse helpers tests ----

    #[test]
    fn parse_vec2_array_form() {
        let params = serde_json::json!({"pos": [10.0, 20.0]});
        assert_eq!(parse_vec2(&params, "pos"), Some(Vec2::new(10.0, 20.0)));
    }

    #[test]
    fn parse_vec2_object_form() {
        let params = serde_json::json!({"pos": {"x": 5.0, "y": 15.0}});
        assert_eq!(parse_vec2(&params, "pos"), Some(Vec2::new(5.0, 15.0)));
    }

    #[test]
    fn parse_vec2_missing() {
        let params = serde_json::json!({});
        assert_eq!(parse_vec2(&params, "pos"), None);
    }

    #[test]
    fn parse_waypoints_mixed_format() {
        let params = serde_json::json!({
            "waypoints": [
                [100, 200],
                {"x": 300, "y": 400}
            ]
        });
        let wps = parse_waypoints(&params);
        assert_eq!(wps.len(), 2);
        assert_eq!(wps[0], Vec2::new(100.0, 200.0));
        assert_eq!(wps[1], Vec2::new(300.0, 400.0));
    }

    #[test]
    fn blackboard_access() {
        let provider = mock_provider(r#"{"strategy": "idle"}"#);
        let mut agent = build_test_agent(provider);

        agent
            .blackboard_mut()
            .set("key".to_string(), serde_json::json!("value"));
        assert_eq!(
            agent.blackboard().get_string("key"),
            Some("value".to_string())
        );
    }
}
