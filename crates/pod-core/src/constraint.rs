//! # Enhanced Action Validation
//!
//! Production-grade constraint system for controlling agent actions.
//!
//! Beyond simple cooldowns, this system enforces:
//! - Token budgets (actions become scarce as time passes)
//! - Reaction time gates (AI can't respond faster than humans)
//! - Named constraint profiles for different entity types
//! - Three-stage validation pipeline for clarity and extensibility

use crate::action::{Action, AgentConstraints};
use crate::id::AgentId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Tracks per-action cooldowns by tick
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooldownTracker {
    /// When each action was last used (tick number)
    action_timers: HashMap<String, u64>,
    /// Minimum ticks between each action type
    cooldowns: HashMap<String, u32>,
}

impl CooldownTracker {
    pub fn new() -> Self {
        Self {
            action_timers: HashMap::new(),
            cooldowns: HashMap::new(),
        }
    }

    /// Set cooldown duration for an action type
    pub fn set_cooldown(&mut self, action: String, cooldown_ticks: u32) {
        self.cooldowns.insert(action, cooldown_ticks);
    }

    /// Check if an action can be used now
    pub fn can_use(&self, action: &str, current_tick: u64) -> bool {
        if let Some(last_tick) = self.action_timers.get(action) {
            if let Some(cooldown) = self.cooldowns.get(action) {
                return current_tick - last_tick >= *cooldown as u64;
            }
        }
        true
    }

    /// Mark an action as just used
    pub fn use_action(&mut self, action: String, tick: u64) {
        self.action_timers.insert(action, tick);
    }

    /// Get remaining cooldown ticks for an action
    pub fn remaining_ticks(&self, action: &str, current_tick: u64) -> u32 {
        if let Some(last_tick) = self.action_timers.get(action) {
            if let Some(cooldown) = self.cooldowns.get(action) {
                let elapsed = current_tick - last_tick;
                if elapsed < *cooldown as u64 {
                    return (*cooldown - elapsed as u32).saturating_sub(1);
                }
            }
        }
        0
    }
}

impl Default for CooldownTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Token-based action budget system
/// Agents earn tokens over time, spend them on actions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionBudget {
    /// Current available tokens
    pub current_tokens: f32,
    /// Maximum tokens allowed (pool size)
    pub max_tokens: f32,
    /// Tokens earned per tick
    pub tokens_per_tick: f32,
    /// Cost of different action types
    token_costs: HashMap<String, f32>,
}

impl ActionBudget {
    pub fn new(max_tokens: f32, tokens_per_tick: f32) -> Self {
        Self {
            current_tokens: max_tokens / 2.0, // start at half
            max_tokens,
            tokens_per_tick,
            token_costs: HashMap::new(),
        }
    }

    /// Set the cost of an action in tokens
    pub fn set_action_cost(&mut self, action: String, cost: f32) {
        self.token_costs.insert(action, cost);
    }

    /// Add tokens (called each tick)
    pub fn tick(&mut self) {
        self.current_tokens = (self.current_tokens + self.tokens_per_tick).min(self.max_tokens);
    }

    /// Check if an action can be afforded
    pub fn can_afford(&self, action: &str) -> bool {
        let cost = self.token_costs.get(action).copied().unwrap_or(1.0);
        self.current_tokens >= cost
    }

    /// Spend tokens on an action
    pub fn spend(&mut self, action: &str) {
        let cost = self.token_costs.get(action).copied().unwrap_or(1.0);
        self.current_tokens = (self.current_tokens - cost).max(0.0);
    }

    /// Get fraction of budget available (0.0 to 1.0)
    pub fn fraction_available(&self) -> f32 {
        (self.current_tokens / self.max_tokens).min(1.0).max(0.0)
    }
}

impl Default for ActionBudget {
    fn default() -> Self {
        Self::new(100.0, 5.0)
    }
}

/// Enforces minimum reaction time (AI can't respond faster than humans)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReactionTimeGate {
    /// Minimum ticks an AI must wait before responding to stimuli
    pub min_reaction_ticks: u32,
    /// When the AI last observed something critical
    last_stimulus_tick: u64,
}

impl ReactionTimeGate {
    pub fn new(min_reaction_ticks: u32) -> Self {
        Self {
            min_reaction_ticks,
            last_stimulus_tick: 0,
        }
    }

    /// Record a stimulus (combat, threat, etc.)
    pub fn record_stimulus(&mut self, tick: u64) {
        self.last_stimulus_tick = tick;
    }

    /// Check if enough time has passed since stimulus
    pub fn can_react(&self, current_tick: u64) -> bool {
        current_tick - self.last_stimulus_tick >= self.min_reaction_ticks as u64
    }

    /// Ticks until reaction is allowed
    pub fn ticks_until_reaction(&self, current_tick: u64) -> u32 {
        let elapsed = current_tick - self.last_stimulus_tick;
        if elapsed >= self.min_reaction_ticks as u64 {
            0
        } else {
            (self.min_reaction_ticks as u64 - elapsed) as u32
        }
    }
}

impl Default for ReactionTimeGate {
    fn default() -> Self {
        Self::new(3) // ~50ms at 60 ticks/second
    }
}

/// Pre-built constraint profiles for different entity types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintProfile {
    /// Name of this profile
    pub name: String,
    /// Base constraints from AgentConstraints
    pub base_constraints: AgentConstraints,
    /// Action budget
    pub budget: ActionBudget,
    /// Reaction time gate
    pub reaction_time: ReactionTimeGate,
    /// Cooldowns by action name
    pub cooldowns: HashMap<String, u32>,
}

impl ConstraintProfile {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            base_constraints: AgentConstraints::default(),
            budget: ActionBudget::default(),
            reaction_time: ReactionTimeGate::default(),
            cooldowns: HashMap::new(),
        }
    }

    /// Warrior: high damage output, slower actions
    pub fn warrior() -> Self {
        let mut p = Self::new("warrior");
        p.base_constraints.actions_per_tick = 2;
        p.base_constraints.attack_cooldown = 20; // 333ms
        p.budget.tokens_per_tick = 3.0;
        p.budget.max_tokens = 50.0;
        p.cooldowns.insert("attack".to_string(), 20);
        p
    }

    /// Healer: support actions, moderate resource consumption
    pub fn healer() -> Self {
        let mut p = Self::new("healer");
        p.base_constraints.actions_per_tick = 3;
        p.base_constraints.attack_cooldown = 60; // 1 second
        p.budget.tokens_per_tick = 5.0;
        p.budget.max_tokens = 75.0;
        p.budget.set_action_cost("heal".to_string(), 2.0);
        p.cooldowns.insert("heal".to_string(), 60);
        p
    }

    /// Mage: powerful abilities, high cooldowns
    pub fn mage() -> Self {
        let mut p = Self::new("mage");
        p.base_constraints.actions_per_tick = 2;
        p.base_constraints.attack_cooldown = 40;
        p.budget.tokens_per_tick = 4.0;
        p.budget.max_tokens = 60.0;
        p.budget.set_action_cost("cast_spell".to_string(), 3.0);
        p.reaction_time.min_reaction_ticks = 5; // mages need more time
        p.cooldowns.insert("cast_spell".to_string(), 90);
        p
    }

    /// NPC: limited actions
    pub fn npc() -> Self {
        let mut p = Self::new("npc");
        p.base_constraints.actions_per_tick = 1;
        p.base_constraints.attack_cooldown = 60;
        p.budget.tokens_per_tick = 2.0;
        p.budget.max_tokens = 30.0;
        p
    }
}

/// Result of constraint validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintViolation {
    /// Action passed all checks
    Valid,
    /// Agent can't act (stunned, dead, etc.)
    AgentIncapacitated,
    /// Too many actions this tick
    ActionsPerTickExceeded,
    /// Specific action is on cooldown
    ActionOnCooldown { remaining_ticks: u32 },
    /// Insufficient action budget
    BudgetExhausted,
    /// AI responding too fast
    ReactionTimeTooFast { wait_ticks: u32 },
    /// Agent not in required state
    InvalidState(String),
}

/// Three-stage validation pipeline
pub struct ValidationPipeline {
    /// Pre-filters (can we even consider this?)
    pre_filters: Vec<Box<dyn Fn(&Action, &AgentConstraints) -> Option<ConstraintViolation> + Send>>,
    /// Constraint checks (resource/cooldown/budget)
    constraint_checks: Vec<Box<dyn Fn(&Action, &AgentConstraints) -> Option<ConstraintViolation> + Send>>,
    /// World state verification (would this action be valid in world?)
    world_checks: Vec<Box<dyn Fn(&Action) -> Option<ConstraintViolation> + Send>>,
}

impl ValidationPipeline {
    pub fn new() -> Self {
        Self {
            pre_filters: Vec::new(),
            constraint_checks: Vec::new(),
            world_checks: Vec::new(),
        }
    }

    /// Validate an action through all stages
    pub fn validate(&self, action: &Action, constraints: &AgentConstraints) -> ConstraintViolation {
        // Stage 1: Pre-filters
        for filter in &self.pre_filters {
            if let Some(violation) = filter(action, constraints) {
                return violation;
            }
        }

        // Stage 2: Constraint checks
        for check in &self.constraint_checks {
            if let Some(violation) = check(action, constraints) {
                return violation;
            }
        }

        // Stage 3: World state checks
        for check in &self.world_checks {
            if let Some(violation) = check(action) {
                return violation;
            }
        }

        ConstraintViolation::Valid
    }

    /// Add a pre-filter
    pub fn add_pre_filter<F>(&mut self, filter: F)
    where
        F: Fn(&Action, &AgentConstraints) -> Option<ConstraintViolation> + Send + 'static,
    {
        self.pre_filters.push(Box::new(filter));
    }

    /// Add a constraint check
    pub fn add_constraint_check<F>(&mut self, check: F)
    where
        F: Fn(&Action, &AgentConstraints) -> Option<ConstraintViolation> + Send + 'static,
    {
        self.constraint_checks.push(Box::new(check));
    }

    /// Add a world state check
    pub fn add_world_check<F>(&mut self, check: F)
    where
        F: Fn(&Action) -> Option<ConstraintViolation> + Send + 'static,
    {
        self.world_checks.push(Box::new(check));
    }
}

impl Default for ValidationPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cooldown_tracker() {
        let mut tracker = CooldownTracker::new();
        tracker.set_cooldown("attack".to_string(), 10);

        assert!(tracker.can_use("attack", 0));
        tracker.use_action("attack".to_string(), 0);
        assert!(!tracker.can_use("attack", 5));
        assert!(tracker.can_use("attack", 10));
    }

    #[test]
    fn test_action_budget() {
        let mut budget = ActionBudget::new(100.0, 5.0);
        budget.set_action_cost("spell".to_string(), 20.0);

        assert!(budget.can_afford("spell"));
        budget.spend("spell");
        assert_eq!(budget.current_tokens, 80.0);

        budget.tick();
        assert_eq!(budget.current_tokens, 85.0);
    }

    #[test]
    fn test_reaction_time_gate() {
        let gate = ReactionTimeGate::new(10);
        assert!(gate.can_react(0)); // no stimulus yet

        let mut gate = ReactionTimeGate::new(10);
        gate.record_stimulus(0);
        assert!(!gate.can_react(5));
        assert!(gate.can_react(10));
    }

    #[test]
    fn test_constraint_profiles() {
        let warrior = ConstraintProfile::warrior();
        assert_eq!(warrior.name, "warrior");
        assert!(warrior.budget.can_afford("attack"));

        let healer = ConstraintProfile::healer();
        assert_eq!(healer.base_constraints.actions_per_tick, 3);
    }
}
