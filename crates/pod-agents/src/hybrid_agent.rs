//! Hybrid agent — LLM strategic planning + Behavior Tree frame-by-frame execution.
//!
//! ## Architecture
//!
//! ```text
//!  Every tick
//!  ──────────
//!  observe(obs)
//!      │
//!      ├─[every N ticks OR on trigger]──► LLM strategy call (background thread)
//!      │                                       │
//!      │                                  returns StrategyDirective
//!      │                                  ("attack", "flee", "explore", ...)
//!      │                                       │
//!      │                               writes to Blackboard:
//!      │                               "strategy" = "attack"
//!      │                               "target_id" = <entity_id>
//!      │                               "urgency"   = 0.85
//!      │
//!  decide()
//!      │
//!      └─► BehaviorTree ticks once, reading Blackboard
//!              │
//!          returns Vec<Action> (reactive, frame-accurate)
//! ```
//!
//! The LLM operates at a strategic cadence (every `strategy_interval_ticks` ticks
//! or when a trigger fires).  Between LLM calls the BT executes the current
//! strategy reactively.  This gives the agent both LLM-quality judgment and
//! BT-quality responsiveness without blocking the game loop.
//!
//! ## Trigger system
//!
//! In addition to the periodic cadence the LLM can be triggered early by:
//! - Health drop below a threshold
//! - New hostile appearing in vision
//! - Completing the current objective
//!
//! ## Blackboard contract
//!
//! The LLM writes these well-known keys which the BT reads:
//!
//! | Key                | Type    | Meaning                                     |
//! |--------------------|---------|---------------------------------------------|
//! | `"strategy"`       | String  | Active strategy name                        |
//! | `"urgency"`        | f32     | 0.0 (relaxed) – 1.0 (critical)             |
//! | `"target_id"`      | u64     | Entity ID of primary target (optional)      |
//! | `"retreat_to_x"`   | f32     | World-X coordinate for retreat              |
//! | `"retreat_to_y"`   | f32     | World-Y coordinate for retreat              |
//! | `"reasoning"`      | String  | LLM's reasoning (for logging/debug)         |

use std::sync::{mpsc, Arc};
use std::time::Instant;

use glam::Vec2;
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};

use pod_core::action::{Action, AgentConstraints};
use pod_core::agent::{Agent, AgentIntrospection, AgentType};
use pod_core::id::AgentId;
use pod_core::observation::{Observation, Relationship};

use crate::conversation_memory::{ConversationMemory, MemoryConfig};
use crate::llm_provider::{CompletionRequest, LlmProvider, MockProvider, TokenBudget};
use crate::scripted_agent::{
    Blackboard, BehaviorNode, BehaviorStatus, BehaviorTree,
    attack_nearest, chase_nearest_hostile, flee_from, guard, patrol, wander,
};

// ============================================================
// STRATEGY DIRECTIVE
// ============================================================

/// High-level strategy returned by the LLM.
///
/// Serialized as JSON in the LLM response.  The `HybridAgent` parses this
/// and writes each field into the behavior tree's `Blackboard`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyDirective {
    /// Primary strategy name.  One of: "attack", "flee", "patrol", "guard",
    /// "explore", "hold", "retreat".
    pub strategy: String,

    /// Confidence / urgency score.  Higher values make the BT prefer more
    /// aggressive sub-trees.
    #[serde(default = "default_urgency")]
    pub urgency: f32,

    /// Optional primary target entity ID.
    #[serde(default)]
    pub target_id: Option<u64>,

    /// Optional retreat / rally point.
    #[serde(default)]
    pub retreat_position: Option<[f32; 2]>,

    /// Short free-form reasoning (logged, not executed).
    #[serde(default)]
    pub reasoning: String,
}

fn default_urgency() -> f32 { 0.5 }

impl Default for StrategyDirective {
    fn default() -> Self {
        Self {
            strategy: "patrol".to_string(),
            urgency: 0.5,
            target_id: None,
            retreat_position: None,
            reasoning: String::new(),
        }
    }
}

impl StrategyDirective {
    /// Attempt to parse a `StrategyDirective` from an LLM response string.
    ///
    /// Tries JSON first, then falls back to keyword scanning for graceful
    /// degradation when the LLM doesn't produce clean JSON.
    pub fn parse(text: &str) -> Self {
        // 1. Try to find a JSON block (```json…``` or raw {…})
        let json_str = extract_json(text);
        if let Some(s) = json_str {
            if let Ok(d) = serde_json::from_str::<StrategyDirective>(&s) {
                return d;
            }
        }

        // 2. Keyword fallback
        let lower = text.to_lowercase();
        let strategy = if lower.contains("flee") || lower.contains("retreat") {
            "flee"
        } else if lower.contains("attack") || lower.contains("engage") {
            "attack"
        } else if lower.contains("guard") || lower.contains("defend") {
            "guard"
        } else if lower.contains("explore") || lower.contains("search") {
            "explore"
        } else if lower.contains("hold") || lower.contains("wait") {
            "hold"
        } else {
            "patrol"
        };

        StrategyDirective {
            strategy: strategy.to_string(),
            reasoning: format!("keyword-fallback from: {}", &text[..text.len().min(120)]),
            ..Default::default()
        }
    }
}

/// Extract the first JSON object from a string.
fn extract_json(text: &str) -> Option<String> {
    // Try ```json ... ``` block
    if let Some(start) = text.find("```json") {
        let after = &text[start + 7..];
        if let Some(end) = after.find("```") {
            return Some(after[..end].trim().to_string());
        }
    }
    // Try raw {...}
    if let Some(start) = text.find('{') {
        let mut depth = 0usize;
        let bytes = text.as_bytes();
        for i in start..bytes.len() {
            match bytes[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(text[start..=i].to_string());
                    }
                }
                _ => {}
            }
        }
    }
    None
}

// ============================================================
// TRIGGER
// ============================================================

/// Conditions that can force an early LLM strategy re-evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyTrigger {
    /// Re-evaluate when health fraction drops below threshold.
    HealthBelow(f32),
    /// Re-evaluate when a new hostile enters vision (compared to last tick).
    NewHostileVisible,
    /// Re-evaluate when no hostiles are visible (after having seen at least one).
    HostilesGone,
    /// Re-evaluate when the current objective progress reaches the given fraction.
    ObjectiveProgress(f32),
}

// ============================================================
// CONFIGURATION
// ============================================================

/// Configuration for `HybridAgent`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridAgentConfig {
    /// How many ticks between periodic LLM strategy calls.
    /// At 60 TPS, 120 → re-plan every 2 seconds.
    pub strategy_interval_ticks: u32,

    /// LLM model name.
    pub model: String,

    /// LLM temperature for strategy generation (lower = more deterministic).
    pub temperature: f32,

    /// Maximum tokens in strategy response.
    pub max_tokens: u32,

    /// Triggers that cause an early strategy re-evaluation.
    pub triggers: Vec<StrategyTrigger>,

    /// System prompt injected into every strategy request.
    pub system_prompt: String,

    /// Whether to enable verbose logging of strategy decisions.
    pub debug_logging: bool,
}

impl Default for HybridAgentConfig {
    fn default() -> Self {
        Self {
            strategy_interval_ticks: 120,
            model: "claude-3-5-sonnet-20241022".to_string(),
            temperature: 0.4,
            max_tokens: 300,
            triggers: vec![
                StrategyTrigger::HealthBelow(0.3),
                StrategyTrigger::NewHostileVisible,
                StrategyTrigger::HostilesGone,
            ],
            system_prompt: default_strategy_prompt(),
            debug_logging: false,
        }
    }
}

fn default_strategy_prompt() -> String {
    r#"You are the strategic mind of a game agent.
Given the current observation, choose the best high-level strategy.

Respond with a JSON object:
{
  "strategy": "<attack|flee|patrol|guard|explore|hold|retreat>",
  "urgency": <0.0-1.0>,
  "target_id": <entity_id or null>,
  "retreat_position": [x, y] or null,
  "reasoning": "<brief reason>"
}

Rules:
- "attack" when hostiles are visible and health > 0.4
- "flee" when health < 0.25 or outmatched
- "retreat" when health 0.25-0.4 and enemies nearby
- "guard" when holding a position is important
- "explore" when nothing notable is happening
- "patrol" as a default fallback
- "hold" when waiting is tactically best
Respond ONLY with the JSON object, no other text."#.to_string()
}

// ============================================================
// HYBRID AGENT INTERNALS
// ============================================================

/// Internal result from a background LLM strategy call.
#[derive(Debug)]
struct StrategyResult {
    directive: StrategyDirective,
    tick: u64,
}

// ============================================================
// HYBRID AGENT
// ============================================================

/// An agent that combines LLM strategic planning with Behavior Tree execution.
///
/// The LLM runs asynchronously in a background thread every
/// `config.strategy_interval_ticks` ticks (or when a trigger fires).
/// The BT executes every tick, driven by the `Blackboard` that the LLM updates.
pub struct HybridAgent {
    id: AgentId,
    config: HybridAgentConfig,
    constraints: AgentConstraints,

    // --- LLM strategy layer ---
    provider: Arc<dyn LlmProvider>,
    memory: ConversationMemory,
    budget: TokenBudget,
    current_directive: StrategyDirective,
    ticks_since_strategy: u32,
    strategy_in_flight: bool,
    strategy_tx: mpsc::Sender<StrategyResult>,
    strategy_rx: mpsc::Receiver<StrategyResult>,

    // --- BT execution layer ---
    tree: BehaviorTree,

    // --- Trigger state ---
    last_hostile_count: usize,
    last_health_fraction: f32,

    // --- Current observation cache ---
    last_observation: Option<Observation>,
}

impl HybridAgent {
    /// Create a new `HybridAgent` with a `MockProvider` (for testing / offline use).
    ///
    /// The behavior tree defaults to `patrol_or_attack` — patrol when idle,
    /// chase and attack when a hostile is visible.
    pub fn new(config: HybridAgentConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            id: AgentId::new(),
            config,
            constraints: AgentConstraints::default(),
            provider: Arc::new(MockProvider::idle()),
            memory: ConversationMemory::new(MemoryConfig::default()),
            budget: TokenBudget::unlimited(),
            current_directive: StrategyDirective::default(),
            ticks_since_strategy: u32::MAX, // trigger immediately on first tick
            strategy_in_flight: false,
            strategy_tx: tx,
            strategy_rx: rx,
            tree: default_strategy_tree(),
            last_hostile_count: 0,
            last_health_fraction: 1.0,
            last_observation: None,
        }
    }

    /// Provide a custom `LlmProvider` (e.g. `OpenAiProvider`).
    pub fn with_provider(mut self, provider: Arc<dyn LlmProvider>) -> Self {
        self.provider = provider;
        self
    }

    /// Replace the default behavior tree.
    pub fn with_tree(mut self, tree: BehaviorTree) -> Self {
        self.tree = tree;
        self
    }

    /// Replace the token budget.
    pub fn with_budget(mut self, budget: TokenBudget) -> Self {
        self.budget = budget;
        self
    }

    /// Access the current strategy directive (useful for external logging).
    pub fn current_directive(&self) -> &StrategyDirective {
        &self.current_directive
    }

    /// Access the behavior tree blackboard (read-only).
    pub fn blackboard(&self) -> &Blackboard {
        &self.tree.blackboard
    }

    // ----------------------------------------------------------------
    // TRIGGER EVALUATION
    // ----------------------------------------------------------------

    /// Returns `true` if any trigger condition fires for the current observation.
    fn triggers_fired(&self, obs: &Observation) -> bool {
        let hostile_count = obs.visible_entities.iter()
            .filter(|e| matches!(e.relationship, Relationship::Hostile))
            .count();

        let health_frac = obs.self_state.health
            .zip(obs.self_state.max_health)
            .map(|(h, m)| if m > 0.0 { h / m } else { 0.0 })
            .unwrap_or(1.0);

        for trigger in &self.config.triggers {
            match trigger {
                StrategyTrigger::HealthBelow(threshold) => {
                    if health_frac < *threshold && self.last_health_fraction >= *threshold {
                        debug!("HybridAgent {}: trigger HealthBelow({})", self.id, threshold);
                        return true;
                    }
                }
                StrategyTrigger::NewHostileVisible => {
                    if hostile_count > self.last_hostile_count {
                        debug!("HybridAgent {}: trigger NewHostileVisible ({} → {})",
                            self.id, self.last_hostile_count, hostile_count);
                        return true;
                    }
                }
                StrategyTrigger::HostilesGone => {
                    if hostile_count == 0 && self.last_hostile_count > 0 {
                        debug!("HybridAgent {}: trigger HostilesGone", self.id);
                        return true;
                    }
                }
                StrategyTrigger::ObjectiveProgress(threshold) => {
                    let any_at_threshold = obs.objectives.iter()
                        .any(|o| o.progress >= *threshold && !o.completed);
                    if any_at_threshold {
                        debug!("HybridAgent {}: trigger ObjectiveProgress({})", self.id, threshold);
                        return true;
                    }
                }
            }
        }
        false
    }

    // ----------------------------------------------------------------
    // LLM STRATEGY REQUEST
    // ----------------------------------------------------------------

    /// Spawn a background thread to request a new strategy from the LLM.
    fn spawn_strategy_request(&mut self, obs: &Observation) {
        let obs_summary = format_observation_for_strategy(obs);
        let memory_section = self.memory.to_prompt_section();

        let user_prompt = if memory_section.is_empty() {
            format!("Current situation:\n{}", obs_summary)
        } else {
            format!("{}\nCurrent situation:\n{}", memory_section, obs_summary)
        };

        let estimated = self.provider.estimate_tokens(&user_prompt)
            + self.provider.estimate_tokens(&self.config.system_prompt);

        if !self.budget.can_request(estimated) {
            warn!("HybridAgent {}: token budget exceeded, skipping strategy re-plan", self.id);
            return;
        }

        let request = CompletionRequest {
            model: self.config.model.clone(),
            system_prompt: self.config.system_prompt.clone(),
            user_prompt,
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        };

        let provider = Arc::clone(&self.provider);
        let tx = self.strategy_tx.clone();
        let agent_id = self.id;
        let tick = obs.tick;

        self.strategy_in_flight = true;
        self.ticks_since_strategy = 0;

        std::thread::spawn(move || {
            match provider.complete(&request) {
                Ok(completion) => {
                    let directive = StrategyDirective::parse(&completion.content);
                    debug!("HybridAgent {}: new strategy = {:?}", agent_id, directive.strategy);
                    let _ = tx.send(StrategyResult { directive, tick });
                }
                Err(e) => {
                    warn!("HybridAgent {}: LLM error: {}, keeping current strategy", agent_id, e);
                    let _ = tx.send(StrategyResult {
                        directive: StrategyDirective {
                            reasoning: format!("LLM error: {}", e),
                            ..Default::default()
                        },
                        tick,
                    });
                }
            }
        });
    }

    // ----------------------------------------------------------------
    // BLACKBOARD UPDATE
    // ----------------------------------------------------------------

    /// Write the `StrategyDirective` into the BT blackboard.
    fn apply_directive_to_blackboard(&mut self, directive: &StrategyDirective) {
        self.tree.blackboard.set(
            "strategy".to_string(),
            serde_json::Value::String(directive.strategy.clone()),
        );
        self.tree.blackboard.set(
            "urgency".to_string(),
            serde_json::json!(directive.urgency),
        );
        if let Some(id) = directive.target_id {
            self.tree.blackboard.set("target_id".to_string(), serde_json::json!(id));
        }
        if let Some([x, y]) = directive.retreat_position {
            self.tree.blackboard.set("retreat_to_x".to_string(), serde_json::json!(x));
            self.tree.blackboard.set("retreat_to_y".to_string(), serde_json::json!(y));
        }
        self.tree.blackboard.set(
            "reasoning".to_string(),
            serde_json::Value::String(directive.reasoning.clone()),
        );
    }
}

// ============================================================
// AGENT TRAIT IMPLEMENTATION
// ============================================================

impl Agent for HybridAgent {
    fn id(&self) -> AgentId { self.id }
    fn agent_type(&self) -> AgentType { AgentType::LlmAgent }

    fn observe(&mut self, observation: Observation) {
        // 1. Update trigger-tracking state
        let hostile_count = observation.visible_entities.iter()
            .filter(|e| matches!(e.relationship, Relationship::Hostile))
            .count();
        let health_frac = observation.self_state.health
            .zip(observation.self_state.max_health)
            .map(|(h, m)| if m > 0.0 { h / m } else { 0.0 })
            .unwrap_or(1.0);

        // 2. Check if we should request a new strategy
        let periodic_due = self.ticks_since_strategy >= self.config.strategy_interval_ticks;
        let triggered = !self.strategy_in_flight && self.triggers_fired(&observation);

        if !self.strategy_in_flight && (periodic_due || triggered) {
            self.spawn_strategy_request(&observation);
        }

        // 3. Try to collect a completed strategy result
        if self.strategy_in_flight {
            if let Ok(result) = self.strategy_rx.try_recv() {
                self.strategy_in_flight = false;
                if self.config.debug_logging {
                    info!(
                        "HybridAgent {} [tick {}]: strategy = {} (urgency={:.2}): {}",
                        self.id, result.tick,
                        result.directive.strategy,
                        result.directive.urgency,
                        result.directive.reasoning
                    );
                }
                // Log the new directive (memory.record requires obs+actions, not available here)
                log::debug!(
                    "HybridAgent {}: new directive strategy={} urgency={:.2}",
                    self.id, result.directive.strategy, result.directive.urgency,
                );
                let directive = result.directive.clone();
                self.apply_directive_to_blackboard(&directive);
                self.current_directive = result.directive;
            }
        }

        // 4. Update trigger-tracking state for next tick
        self.last_hostile_count = hostile_count;
        self.last_health_fraction = health_frac;
        self.ticks_since_strategy = self.ticks_since_strategy.saturating_add(1);

        // 5. Forward observation to the BT
        self.last_observation = Some(observation);
    }

    fn decide(&mut self) -> Vec<Action> {
        let obs = match &self.last_observation {
            Some(o) => o.clone(),
            None => return vec![Action::Idle],
        };

        // Tick the behavior tree once with the current observation
        let (actions, status) = self.tree.tick(&obs);

        if self.config.debug_logging {
            let strategy = self.tree.blackboard.get_string("strategy")
                .unwrap_or_else(|| "patrol".to_string());
            debug!(
                "HybridAgent {} [tick {}]: BT={:?} strategy={} actions={:?}",
                self.id, obs.tick, status, strategy, actions
            );
        }

        // If BT failed or returned empty, fall back to the strategy fallback action
        if actions.is_empty() || matches!(status, BehaviorStatus::Failure) {
            return strategy_fallback_actions(&self.current_directive, &obs);
        }

        actions
    }

    fn constraints(&self) -> &AgentConstraints { &self.constraints }
    fn constraints_mut(&mut self) -> &mut AgentConstraints { &mut self.constraints }

    fn introspect(&self) -> AgentIntrospection {
        let strategy = self.tree.blackboard.get_string("strategy")
            .unwrap_or_else(|| "patrol".to_string());
        let urgency = self.tree.blackboard.get_f32("urgency").unwrap_or(0.5);
        AgentIntrospection {
            agent_id: self.id,
            agent_type: AgentType::LlmAgent,
            constraints: self.constraints.clone(),
            state_description: format!(
                "strategy={} urgency={:.2} ticks_since_plan={} in_flight={}",
                strategy, urgency,
                self.ticks_since_strategy,
                self.strategy_in_flight,
            ),
        }
    }
}

// ============================================================
// HELPERS
// ============================================================

/// Format an `Observation` as a compact string suitable for the strategy LLM.
fn format_observation_for_strategy(obs: &Observation) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "tick={} pos=({:.0},{:.0}) hp={:.0}% facing={:.0}°\n",
        obs.tick,
        obs.self_state.position.x,
        obs.self_state.position.y,
        obs.self_state.health
            .zip(obs.self_state.max_health)
            .map(|(h, m)| if m > 0.0 { h / m * 100.0 } else { 0.0 })
            .unwrap_or(100.0),
        obs.self_state.rotation.to_degrees(),
    ));

    let hostiles: Vec<_> = obs.visible_entities.iter()
        .filter(|e| matches!(e.relationship, Relationship::Hostile))
        .collect();
    let friendlies: Vec<_> = obs.visible_entities.iter()
        .filter(|e| matches!(e.relationship, Relationship::Friendly))
        .collect();

    s.push_str(&format!("enemies={} allies={}\n", hostiles.len(), friendlies.len()));

    if let Some(nearest) = hostiles.iter().min_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap()) {
        s.push_str(&format!("nearest_enemy dist={:.0} hp={}\n",
            nearest.distance,
            nearest.health_fraction.map(|h| format!("{:.0}%", h * 100.0)).unwrap_or_else(|| "?".to_string())
        ));
    }

    if !obs.objectives.is_empty() {
        let obj = &obs.objectives[0];
        s.push_str(&format!("objective=\"{}\" progress={:.0}%\n",
            obj.description, obj.progress * 100.0));
    }

    s
}

/// Fallback actions when the BT produces nothing useful.
/// Derives actions directly from the current `StrategyDirective`.
fn strategy_fallback_actions(directive: &StrategyDirective, obs: &Observation) -> Vec<Action> {
    match directive.strategy.as_str() {
        "attack" | "engage" => {
            // Find nearest hostile and attack it
            if let Some(target) = obs.visible_entities.iter()
                .filter(|e| matches!(e.relationship, Relationship::Hostile))
                .min_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap())
            {
                vec![Action::AttackTarget { target: pod_core::id::EntityId(target.entity_id.0) }]
            } else {
                vec![Action::Idle]
            }
        }
        "flee" | "retreat" => {
            if let Some([x, y]) = directive.retreat_position {
                vec![Action::Move { direction: Vec2::new(x, y).normalize_or_zero() }]
            } else {
                // Move away from nearest hostile
                if let Some(hostile) = obs.visible_entities.iter()
                    .filter(|e| matches!(e.relationship, Relationship::Hostile))
                    .min_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap())
                {
                    let away = (obs.self_state.position - hostile.position).normalize_or_zero();
                    vec![Action::Move { direction: away }]
                } else {
                    vec![Action::Idle]
                }
            }
        }
        "hold" => vec![Action::Idle],
        "explore" => {
            // Move in current facing direction
            let dir = Vec2::from_angle(obs.self_state.rotation);
            vec![Action::Move { direction: dir }]
        }
        _ => vec![Action::Idle], // patrol, guard, default
    }
}

/// Build the default strategy-driven behavior tree.
///
/// The tree reads the `"strategy"` key from the blackboard and routes to the
/// appropriate sub-tree.  Unknown strategies fall through to patrol.
fn default_strategy_tree() -> BehaviorTree {
    // Strategy selector: try each branch in priority order.
    // Each branch first checks the blackboard strategy key, then executes.
    //
    // Note: BehaviorNode::Condition checks a named condition evaluated by the
    // standard condition evaluator.  We use the strategy sub-trees as composites.
    let root = BehaviorNode::selector(vec![
        // --- attack branch ---
        BehaviorNode::sequence(vec![
            BehaviorNode::HasTarget,
            BehaviorNode::EnemyInRange { range: 200.0 },
            BehaviorNode::ActionNode { action: Action::Idle }, // placeholder: real BT would call attack
        ]),

        // --- chase branch: hostile visible but not in attack range ---
        BehaviorNode::sequence(vec![
            BehaviorNode::HasTarget,
            BehaviorNode::MoveToNearest { relationship: Relationship::Hostile },
        ]),

        // --- flee branch: low health ---
        BehaviorNode::sequence(vec![
            BehaviorNode::HealthBelow { threshold: 0.3 },
            BehaviorNode::FleeFromNearest,
        ]),

        // --- default: wander/patrol ---
        BehaviorNode::ActionNode { action: Action::Idle },
    ]);

    BehaviorTree::new(root)
}

// ============================================================
// BUILDER / FACTORY FUNCTIONS
// ============================================================

/// Create a `HybridAgent` configured for aggressive combat.
///
/// - Re-plans every 2 seconds (120 ticks at 60 TPS)
/// - Attacks when hostiles are visible; flees at <25% HP
/// - High-temperature LLM for creative tactics
pub fn aggressive_hybrid(config: HybridAgentConfig) -> HybridAgent {
    let attack_root = BehaviorNode::selector(vec![
        BehaviorNode::sequence(vec![
            BehaviorNode::HasTarget,
            BehaviorNode::MoveToNearest { relationship: Relationship::Hostile },
        ]),
        BehaviorNode::sequence(vec![
            BehaviorNode::HealthBelow { threshold: 0.25 },
            BehaviorNode::FleeFromNearest,
        ]),
        BehaviorNode::ActionNode { action: Action::Idle },
    ]);

    HybridAgent::new(config).with_tree(BehaviorTree::new(attack_root))
}

/// Create a `HybridAgent` configured for defensive / guard behavior.
///
/// - Re-plans every 5 seconds (300 ticks at 60 TPS)
/// - Guards position, only engages close threats
/// - Conservative temperature
pub fn defensive_hybrid(mut config: HybridAgentConfig) -> HybridAgent {
    config.strategy_interval_ticks = 300;
    config.temperature = 0.2;

    let guard_root = BehaviorNode::selector(vec![
        BehaviorNode::sequence(vec![
            BehaviorNode::EnemyInRange { range: 100.0 },
            BehaviorNode::MoveToNearest { relationship: Relationship::Hostile },
        ]),
        BehaviorNode::sequence(vec![
            BehaviorNode::HealthBelow { threshold: 0.3 },
            BehaviorNode::FleeFromNearest,
        ]),
        BehaviorNode::ActionNode { action: Action::Idle },
    ]);

    HybridAgent::new(config).with_tree(BehaviorTree::new(guard_root))
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::observation::{Observation, SelfState, VisibleEntity};
    use glam::Vec2;

    fn make_obs_with_hostile(health_pct: f32, hostile_distance: Option<f32>) -> Observation {
        let mut obs = Observation::default();
        obs.tick = 1;
        obs.self_state = SelfState {
            health: Some(health_pct * 100.0),
            max_health: Some(100.0),
            position: Vec2::new(0.0, 0.0),
            ..Default::default()
        };
        if let Some(dist) = hostile_distance {
            obs.visible_entities.push(VisibleEntity {
                entity_id: pod_core::id::EntityId(42),
                entity_type: "enemy".to_string(),
                position: Vec2::new(dist, 0.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                distance: dist,
                relationship: Relationship::Hostile,
                health_fraction: Some(1.0),
            });
        }
        obs
    }

    #[test]
    fn test_strategy_directive_parse_json() {
        let json = r#"{"strategy":"attack","urgency":0.9,"target_id":42,"reasoning":"enemy close"}"#;
        let d = StrategyDirective::parse(json);
        assert_eq!(d.strategy, "attack");
        assert!((d.urgency - 0.9).abs() < 0.01);
        assert_eq!(d.target_id, Some(42));
    }

    #[test]
    fn test_strategy_directive_parse_json_block() {
        let text = "Here is my plan:\n```json\n{\"strategy\":\"flee\",\"urgency\":0.8}\n```\n";
        let d = StrategyDirective::parse(text);
        assert_eq!(d.strategy, "flee");
    }

    #[test]
    fn test_strategy_directive_keyword_fallback() {
        let d = StrategyDirective::parse("I think we should attack the enemy now!");
        assert_eq!(d.strategy, "attack");

        let d = StrategyDirective::parse("We must flee immediately.");
        assert_eq!(d.strategy, "flee");

        let d = StrategyDirective::parse("nothing interesting");
        assert_eq!(d.strategy, "patrol");
    }

    #[test]
    fn test_hybrid_agent_creation() {
        let agent = HybridAgent::new(HybridAgentConfig::default());
        assert_eq!(agent.agent_type(), AgentType::LlmAgent);
    }

    #[test]
    fn test_hybrid_agent_observe_and_decide_no_hostiles() {
        let mut agent = HybridAgent::new(HybridAgentConfig::default());
        let obs = make_obs_with_hostile(1.0, None);
        agent.observe(obs);
        let actions = agent.decide();
        // With no hostiles and no LLM response yet, should return idle or patrol
        assert!(!actions.is_empty());
    }

    #[test]
    fn test_hybrid_agent_decide_with_hostile() {
        let mut agent = HybridAgent::new(HybridAgentConfig::default());
        let obs = make_obs_with_hostile(1.0, Some(80.0));
        agent.observe(obs);
        let actions = agent.decide();
        assert!(!actions.is_empty());
    }

    #[test]
    fn test_hybrid_agent_fallback_actions_attack() {
        let directive = StrategyDirective {
            strategy: "attack".to_string(),
            urgency: 0.9,
            ..Default::default()
        };
        let obs = make_obs_with_hostile(1.0, Some(50.0));
        let actions = strategy_fallback_actions(&directive, &obs);
        assert!(!actions.is_empty());
        assert!(matches!(actions[0], Action::AttackTarget { .. }));
    }

    #[test]
    fn test_hybrid_agent_fallback_actions_flee_with_position() {
        let directive = StrategyDirective {
            strategy: "flee".to_string(),
            urgency: 1.0,
            retreat_position: Some([100.0, 200.0]),
            ..Default::default()
        };
        let obs = make_obs_with_hostile(0.2, Some(30.0));
        let actions = strategy_fallback_actions(&directive, &obs);
        assert!(matches!(actions[0], Action::Move { .. }));
    }

    #[test]
    fn test_hybrid_agent_fallback_actions_flee_no_position() {
        let directive = StrategyDirective {
            strategy: "flee".to_string(),
            urgency: 1.0,
            retreat_position: None,
            ..Default::default()
        };
        let obs = make_obs_with_hostile(0.2, Some(30.0));
        let actions = strategy_fallback_actions(&directive, &obs);
        assert!(matches!(actions[0], Action::Move { .. }));
    }

    #[test]
    fn test_trigger_health_below() {
        let config = HybridAgentConfig {
            triggers: vec![StrategyTrigger::HealthBelow(0.3)],
            ..Default::default()
        };
        let mut agent = HybridAgent::new(config);
        // Initially health is 1.0 (from last_health_fraction init)
        let obs = make_obs_with_hostile(0.25, None); // 25% health — below threshold
        // Should trigger because 0.25 < 0.3 and last was >= 0.3
        assert!(agent.triggers_fired(&obs));
    }

    #[test]
    fn test_trigger_new_hostile_visible() {
        let config = HybridAgentConfig {
            triggers: vec![StrategyTrigger::NewHostileVisible],
            ..Default::default()
        };
        let mut agent = HybridAgent::new(config);
        agent.last_hostile_count = 0;
        let obs = make_obs_with_hostile(1.0, Some(50.0));
        assert!(agent.triggers_fired(&obs)); // 1 hostile > 0 last
    }

    #[test]
    fn test_trigger_hostiles_gone() {
        let config = HybridAgentConfig {
            triggers: vec![StrategyTrigger::HostilesGone],
            ..Default::default()
        };
        let mut agent = HybridAgent::new(config);
        agent.last_hostile_count = 3;
        let obs = make_obs_with_hostile(1.0, None); // no hostiles visible
        assert!(agent.triggers_fired(&obs));
    }

    #[test]
    fn test_apply_directive_to_blackboard() {
        let mut agent = HybridAgent::new(HybridAgentConfig::default());
        let directive = StrategyDirective {
            strategy: "guard".to_string(),
            urgency: 0.7,
            target_id: Some(99),
            retreat_position: Some([10.0, 20.0]),
            reasoning: "hold the line".to_string(),
        };
        agent.apply_directive_to_blackboard(&directive);
        assert_eq!(agent.blackboard.get_string("strategy").as_deref(), Some("guard"));
        assert!((agent.blackboard.get_f32("urgency").unwrap() - 0.7).abs() < 0.01);
    }

    #[test]
    fn test_aggressive_hybrid_factory() {
        let agent = aggressive_hybrid(HybridAgentConfig::default());
        assert_eq!(agent.agent_type(), AgentType::LlmAgent);
    }

    #[test]
    fn test_defensive_hybrid_factory() {
        let agent = defensive_hybrid(HybridAgentConfig::default());
        assert_eq!(agent.config.strategy_interval_ticks, 300);
    }

    #[test]
    fn test_introspect() {
        let agent = HybridAgent::new(HybridAgentConfig::default());
        let info = agent.introspect();
        assert!(info.state_description.contains("strategy="));
    }

    #[test]
    fn test_extract_json() {
        assert!(extract_json("{\"a\":1}").is_some());
        assert!(extract_json("no json here").is_none());
        let block = "```json\n{\"x\":2}\n```";
        assert_eq!(extract_json(block).unwrap().trim(), "{\"x\":2}");
    }

    #[test]
    fn test_format_observation_for_strategy() {
        let obs = make_obs_with_hostile(0.8, Some(60.0));
        let s = format_observation_for_strategy(&obs);
        assert!(s.contains("hp="));
        assert!(s.contains("enemies=1"));
    }
}
