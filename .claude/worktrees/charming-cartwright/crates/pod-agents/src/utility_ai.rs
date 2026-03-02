//! Utility AI system for score-based action selection.
//!
//! Utility AI evaluates all possible actions by scoring them through a set of
//! "considerations" (evaluation functions), then selects the highest-scoring action.
//!
//! Each action has:
//! - A list of `Consideration` evaluators that return 0.0..=1.0
//! - A `weight` multiplier (base importance)
//! - A `bonus` flat score addition
//!
//! Final score = weight * (c1 * c2 * ... * cN) + bonus
//!
//! This multiplicative combination means any single consideration returning 0 vetoes
//! the action entirely, which models hard prerequisites naturally.
//!
//! # Example
//!
//! ```rust,ignore
//! use pod_agents::utility_ai::*;
//! use pod_core::action::Action;
//!
//! let brain = UtilityBrain::new()
//!     .add_action(Action::Attack, |a| a
//!         .weight(1.5)
//!         .consideration(HealthConsideration::new(ResponseCurve::Linear))
//!         .consideration(DistanceToThreatConsideration::new(100.0, ResponseCurve::InverseLinear)))
//!     .add_action(Action::Idle, |a| a
//!         .weight(0.1)
//!         .bonus(0.01))
//!     .build();
//! ```

use pod_core::action::{Action, AgentConstraints};
use pod_core::agent::{Agent, AgentType};
use pod_core::id::AgentId;
use pod_core::observation::{Observation, Relationship};

// ============================================================
// RESPONSE CURVES
// ============================================================

/// Shaping functions applied to consideration outputs.
///
/// Response curves transform a raw 0.0..=1.0 input into a shaped 0.0..=1.0 output,
/// allowing designers to tune how sensitive each consideration is.
#[derive(Debug, Clone)]
pub enum ResponseCurve {
    /// f(x) = x — linear pass-through
    Linear,
    /// f(x) = x^2 — slow start, fast finish
    Quadratic,
    /// f(x) = 1.0 - x — inverted linear
    InverseLinear,
    /// Sigmoid curve: f(x) = 1 / (1 + e^(-steepness * (x - midpoint)))
    Logistic { midpoint: f32, steepness: f32 },
    /// Always returns the given constant, ignoring input
    Constant(f32),
}

impl ResponseCurve {
    /// Apply this curve to an input value, clamping the result to 0.0..=1.0.
    pub fn apply(&self, input: f32) -> f32 {
        let raw = match self {
            ResponseCurve::Linear => input,
            ResponseCurve::Quadratic => input * input,
            ResponseCurve::InverseLinear => 1.0 - input,
            ResponseCurve::Logistic { midpoint, steepness } => {
                1.0 / (1.0 + (-steepness * (input - midpoint)).exp())
            }
            ResponseCurve::Constant(v) => *v,
        };
        raw.clamp(0.0, 1.0)
    }
}

// ============================================================
// CONSIDERATION CONTEXT
// ============================================================

/// Data passed to each consideration for evaluation.
///
/// Bundles the agent's observation with the specific action being scored
/// and pre-computed convenience fields.
pub struct ConsiderationContext<'a> {
    /// The full observation for this tick
    pub observation: &'a Observation,
    /// The action being evaluated
    pub action: &'a Action,
    /// Agent's current health as a ratio (0.0 = dead, 1.0 = full), or None
    pub health_ratio: Option<f32>,
    /// Agent's current position
    pub position: glam::Vec2,
}

impl<'a> ConsiderationContext<'a> {
    /// Build a context from an observation and an action.
    pub fn from_observation(observation: &'a Observation, action: &'a Action) -> Self {
        let health_ratio = match (observation.self_state.health, observation.self_state.max_health)
        {
            (Some(h), Some(m)) if m > 0.0 => Some(h / m),
            _ => None,
        };
        Self {
            observation,
            action,
            health_ratio,
            position: observation.self_state.position,
        }
    }
}

// ============================================================
// CONSIDERATION TRAIT
// ============================================================

/// A single evaluation factor that scores an aspect of the game state.
///
/// Considerations return a value in 0.0..=1.0 where 0 means "this factor
/// completely opposes this action" and 1 means "this factor fully supports it."
pub trait Consideration: Send {
    /// Human-readable name for debugging.
    fn name(&self) -> &str;

    /// Evaluate this factor given the current context. Must return 0.0..=1.0.
    fn evaluate(&self, context: &ConsiderationContext) -> f32;
}

// ============================================================
// BUILT-IN CONSIDERATIONS
// ============================================================

/// Scores based on the agent's own health ratio.
///
/// With `Linear`: high health → high score (good for aggressive actions).
/// With `InverseLinear`: low health → high score (good for flee/heal actions).
pub struct HealthConsideration {
    curve: ResponseCurve,
}

impl HealthConsideration {
    pub fn new(curve: ResponseCurve) -> Self {
        Self { curve }
    }
}

impl Consideration for HealthConsideration {
    fn name(&self) -> &str {
        "health"
    }

    fn evaluate(&self, context: &ConsiderationContext) -> f32 {
        let ratio = context.health_ratio.unwrap_or(1.0);
        self.curve.apply(ratio)
    }
}

/// Scores based on distance to the nearest hostile entity.
///
/// Normalizes the distance against `max_range` so that an entity at `max_range`
/// or farther produces 1.0, and an entity at distance 0 produces 0.0 (before
/// the response curve is applied).
pub struct DistanceToThreatConsideration {
    max_range: f32,
    curve: ResponseCurve,
}

impl DistanceToThreatConsideration {
    pub fn new(max_range: f32, curve: ResponseCurve) -> Self {
        Self { max_range, curve }
    }
}

impl Consideration for DistanceToThreatConsideration {
    fn name(&self) -> &str {
        "distance_to_threat"
    }

    fn evaluate(&self, context: &ConsiderationContext) -> f32 {
        let nearest_hostile_dist = context
            .observation
            .visible_entities
            .iter()
            .filter(|e| matches!(e.relationship, Relationship::Hostile))
            .map(|e| e.distance)
            .fold(f32::MAX, f32::min);

        if nearest_hostile_dist == f32::MAX {
            // No threats visible — return curve applied to 1.0 (max distance)
            return self.curve.apply(1.0);
        }

        let normalized = (nearest_hostile_dist / self.max_range).clamp(0.0, 1.0);
        self.curve.apply(normalized)
    }
}

/// Scores based on distance to the nearest friendly entity.
///
/// Normalizes against `max_range`. Useful for grouping or support behaviors.
pub struct DistanceToAllyConsideration {
    max_range: f32,
    curve: ResponseCurve,
}

impl DistanceToAllyConsideration {
    pub fn new(max_range: f32, curve: ResponseCurve) -> Self {
        Self { max_range, curve }
    }
}

impl Consideration for DistanceToAllyConsideration {
    fn name(&self) -> &str {
        "distance_to_ally"
    }

    fn evaluate(&self, context: &ConsiderationContext) -> f32 {
        let nearest_ally_dist = context
            .observation
            .visible_entities
            .iter()
            .filter(|e| matches!(e.relationship, Relationship::Friendly))
            .map(|e| e.distance)
            .fold(f32::MAX, f32::min);

        if nearest_ally_dist == f32::MAX {
            return self.curve.apply(1.0);
        }

        let normalized = (nearest_ally_dist / self.max_range).clamp(0.0, 1.0);
        self.curve.apply(normalized)
    }
}

/// Scores based on the number of visible hostile entities.
///
/// Normalizes against `max_count` so that seeing `max_count` or more hostiles
/// produces 1.0 (before curve shaping).
pub struct ThreatCountConsideration {
    max_count: f32,
    curve: ResponseCurve,
}

impl ThreatCountConsideration {
    pub fn new(max_count: u32, curve: ResponseCurve) -> Self {
        Self {
            max_count: max_count as f32,
            curve,
        }
    }
}

impl Consideration for ThreatCountConsideration {
    fn name(&self) -> &str {
        "threat_count"
    }

    fn evaluate(&self, context: &ConsiderationContext) -> f32 {
        let count = context
            .observation
            .visible_entities
            .iter()
            .filter(|e| matches!(e.relationship, Relationship::Hostile))
            .count() as f32;

        let normalized = (count / self.max_count).clamp(0.0, 1.0);
        self.curve.apply(normalized)
    }
}

/// Placeholder ammo consideration. Always returns 1.0.
///
/// Future: check inventory for ammo count and normalize.
pub struct AmmoConsideration;

impl AmmoConsideration {
    pub fn new() -> Self {
        Self
    }
}

impl Consideration for AmmoConsideration {
    fn name(&self) -> &str {
        "ammo"
    }

    fn evaluate(&self, _context: &ConsiderationContext) -> f32 {
        1.0
    }
}

/// Scores 1.0 if no cooldowns are active, 0.0 if any cooldown is active.
///
/// This is a hard gate: if the agent is on any cooldown, the action is vetoed.
pub struct CooldownConsideration;

impl CooldownConsideration {
    pub fn new() -> Self {
        Self
    }
}

impl Consideration for CooldownConsideration {
    fn name(&self) -> &str {
        "cooldown"
    }

    fn evaluate(&self, context: &ConsiderationContext) -> f32 {
        let any_active = context
            .observation
            .self_state
            .cooldowns
            .iter()
            .any(|cd| cd.remaining_ticks > 0);

        if any_active {
            0.0
        } else {
            1.0
        }
    }
}

// ============================================================
// UTILITY ACTION
// ============================================================

/// An action paired with the considerations that evaluate its desirability.
pub struct UtilityAction {
    /// The game action to perform if selected.
    pub action: Action,
    /// Evaluators that each return 0.0..=1.0.
    pub considerations: Vec<Box<dyn Consideration>>,
    /// Base weight multiplier for the combined score.
    pub weight: f32,
    /// Flat bonus added after the weighted product.
    pub bonus: f32,
}

impl UtilityAction {
    /// Score this action given an observation.
    ///
    /// score = weight * (c1 * c2 * ... * cN) + bonus
    pub fn score(&self, observation: &Observation) -> f32 {
        let context = ConsiderationContext::from_observation(observation, &self.action);

        let product = if self.considerations.is_empty() {
            1.0
        } else {
            self.considerations
                .iter()
                .map(|c| c.evaluate(&context))
                .fold(1.0, |acc, v| acc * v)
        };

        (self.weight * product + self.bonus).max(0.0)
    }
}

// ============================================================
// UTILITY ACTION BUILDER
// ============================================================

/// Builder for constructing a `UtilityAction` fluently.
pub struct UtilityActionBuilder {
    action: Action,
    considerations: Vec<Box<dyn Consideration>>,
    weight: f32,
    bonus: f32,
}

impl UtilityActionBuilder {
    fn new(action: Action) -> Self {
        Self {
            action,
            considerations: Vec::new(),
            weight: 1.0,
            bonus: 0.0,
        }
    }

    /// Set the base weight multiplier.
    pub fn weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }

    /// Set the flat bonus added to the final score.
    pub fn bonus(mut self, bonus: f32) -> Self {
        self.bonus = bonus;
        self
    }

    /// Add a consideration to this action's evaluation.
    pub fn consideration<C: Consideration + 'static>(mut self, c: C) -> Self {
        self.considerations.push(Box::new(c));
        self
    }

    fn build(self) -> UtilityAction {
        UtilityAction {
            action: self.action,
            considerations: self.considerations,
            weight: self.weight,
            bonus: self.bonus,
        }
    }
}

// ============================================================
// UTILITY BRAIN
// ============================================================

/// The core decision-maker that scores all candidate actions and selects the best.
///
/// Use the builder pattern to construct:
/// ```rust,ignore
/// let brain = UtilityBrain::new()
///     .add_action(Action::Attack, |a| a
///         .weight(1.5)
///         .consideration(HealthConsideration::new(ResponseCurve::Linear)))
///     .build();
/// ```
pub struct UtilityBrain {
    actions: Vec<UtilityAction>,
}

/// Builder for constructing a `UtilityBrain`.
pub struct UtilityBrainBuilder {
    actions: Vec<UtilityAction>,
}

impl UtilityBrain {
    /// Start building a new UtilityBrain.
    pub fn new() -> UtilityBrainBuilder {
        UtilityBrainBuilder {
            actions: Vec::new(),
        }
    }

    /// Score all registered actions and return them sorted by score descending.
    pub fn evaluate(&self, observation: &Observation) -> Vec<(Action, f32)> {
        let mut scored: Vec<(Action, f32)> = self
            .actions
            .iter()
            .map(|ua| (ua.action.clone(), ua.score(observation)))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }

    /// Select the highest-scoring action. Returns `Action::Idle` if all scores are 0.
    pub fn select_best(&self, observation: &Observation) -> Action {
        let scored = self.evaluate(observation);

        scored
            .into_iter()
            .find(|(_, score)| *score > 0.0)
            .map(|(action, _)| action)
            .unwrap_or(Action::Idle)
    }
}

impl UtilityBrainBuilder {
    /// Add an action with its configuration via a builder closure.
    pub fn add_action<F>(mut self, action: Action, configure: F) -> Self
    where
        F: FnOnce(UtilityActionBuilder) -> UtilityActionBuilder,
    {
        let builder = UtilityActionBuilder::new(action);
        let configured = configure(builder);
        self.actions.push(configured.build());
        self
    }

    /// Finalize the brain.
    pub fn build(self) -> UtilityBrain {
        UtilityBrain {
            actions: self.actions,
        }
    }
}

// ============================================================
// UTILITY AGENT (implements Agent trait)
// ============================================================

/// An agent that uses Utility AI for decision-making.
///
/// Wraps a `UtilityBrain` and implements the standard `Agent` trait,
/// evaluated as `AgentType::ScriptedNpc`.
pub struct UtilityAgent {
    id: AgentId,
    agent_type: AgentType,
    constraints: AgentConstraints,
    brain: UtilityBrain,
    last_observation: Option<Observation>,
}

impl UtilityAgent {
    /// Create a new UtilityAgent with the given brain.
    pub fn new(brain: UtilityBrain) -> Self {
        Self {
            id: AgentId::new(),
            agent_type: AgentType::ScriptedNpc,
            constraints: AgentConstraints::default(),
            brain,
            last_observation: None,
        }
    }

    /// Create with a specific agent ID.
    pub fn with_id(mut self, id: AgentId) -> Self {
        self.id = id;
        self
    }

    /// Override the agent type (default is ScriptedNpc).
    pub fn with_agent_type(mut self, agent_type: AgentType) -> Self {
        self.agent_type = agent_type;
        self
    }

    /// Override the constraints.
    pub fn with_constraints(mut self, constraints: AgentConstraints) -> Self {
        self.constraints = constraints;
        self
    }

    /// Access the underlying brain for inspection.
    pub fn brain(&self) -> &UtilityBrain {
        &self.brain
    }
}

impl Agent for UtilityAgent {
    fn id(&self) -> AgentId {
        self.id
    }

    fn agent_type(&self) -> AgentType {
        self.agent_type
    }

    fn observe(&mut self, observation: Observation) {
        self.last_observation = Some(observation);
    }

    fn decide(&mut self) -> Vec<Action> {
        match &self.last_observation {
            Some(obs) => vec![self.brain.select_best(obs)],
            None => vec![Action::Idle],
        }
    }

    fn constraints(&self) -> &AgentConstraints {
        &self.constraints
    }

    fn constraints_mut(&mut self) -> &mut AgentConstraints {
        &mut self.constraints
    }
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;
    use pod_core::component::Team;
    use pod_core::id::EntityId;
    use pod_core::observation::*;

    /// Helper: build a minimal observation with configurable fields.
    fn make_observation() -> Observation {
        Observation {
            tick: 1,
            elapsed_secs: 0.016,
            self_state: SelfState {
                agent_id: AgentId::new(),
                entity_id: EntityId(1),
                position: Vec2::ZERO,
                rotation: 0.0,
                velocity: Vec2::ZERO,
                health: Some(100.0),
                max_health: Some(100.0),
                team: Team::Team(1),
                cooldowns: vec![],
            },
            visible_entities: vec![],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![],
        }
    }

    fn make_hostile(distance: f32) -> VisibleEntity {
        VisibleEntity {
            entity_id: EntityId(99),
            entity_type: "enemy".into(),
            position: Vec2::new(distance, 0.0),
            velocity: Vec2::ZERO,
            rotation: 0.0,
            distance,
            relationship: Relationship::Hostile,
            health_fraction: Some(1.0),
        }
    }

    fn make_friendly(distance: f32) -> VisibleEntity {
        VisibleEntity {
            entity_id: EntityId(50),
            entity_type: "ally".into(),
            position: Vec2::new(distance, 0.0),
            velocity: Vec2::ZERO,
            rotation: 0.0,
            distance,
            relationship: Relationship::Friendly,
            health_fraction: Some(1.0),
        }
    }

    // ---- ResponseCurve tests ----

    #[test]
    fn test_response_curve_linear() {
        let curve = ResponseCurve::Linear;
        assert_eq!(curve.apply(0.0), 0.0);
        assert_eq!(curve.apply(0.5), 0.5);
        assert_eq!(curve.apply(1.0), 1.0);
    }

    #[test]
    fn test_response_curve_quadratic() {
        let curve = ResponseCurve::Quadratic;
        assert_eq!(curve.apply(0.0), 0.0);
        assert!((curve.apply(0.5) - 0.25).abs() < 1e-6);
        assert_eq!(curve.apply(1.0), 1.0);
    }

    #[test]
    fn test_response_curve_inverse_linear() {
        let curve = ResponseCurve::InverseLinear;
        assert_eq!(curve.apply(0.0), 1.0);
        assert_eq!(curve.apply(0.5), 0.5);
        assert_eq!(curve.apply(1.0), 0.0);
    }

    #[test]
    fn test_response_curve_logistic() {
        let curve = ResponseCurve::Logistic {
            midpoint: 0.5,
            steepness: 10.0,
        };
        // At midpoint, sigmoid should be ~0.5
        assert!((curve.apply(0.5) - 0.5).abs() < 1e-6);
        // At 0.0, should be close to 0
        assert!(curve.apply(0.0) < 0.01);
        // At 1.0, should be close to 1
        assert!(curve.apply(1.0) > 0.99);
    }

    #[test]
    fn test_response_curve_constant() {
        let curve = ResponseCurve::Constant(0.7);
        assert!((curve.apply(0.0) - 0.7).abs() < 1e-6);
        assert!((curve.apply(0.5) - 0.7).abs() < 1e-6);
        assert!((curve.apply(1.0) - 0.7).abs() < 1e-6);
    }

    #[test]
    fn test_response_curve_clamps_output() {
        // InverseLinear with input > 1 would give negative without clamping
        let curve = ResponseCurve::InverseLinear;
        assert_eq!(curve.apply(1.5), 0.0);

        // Quadratic with negative input would give positive but we test clamping
        let curve = ResponseCurve::Linear;
        assert_eq!(curve.apply(-0.5), 0.0);
    }

    // ---- Consideration tests ----

    #[test]
    fn test_health_consideration_full_health() {
        let c = HealthConsideration::new(ResponseCurve::Linear);
        let obs = make_observation();
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Attack);
        assert!((c.evaluate(&ctx) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_health_consideration_half_health() {
        let c = HealthConsideration::new(ResponseCurve::Linear);
        let mut obs = make_observation();
        obs.self_state.health = Some(50.0);
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Attack);
        assert!((c.evaluate(&ctx) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_health_consideration_inverse_curve() {
        let c = HealthConsideration::new(ResponseCurve::InverseLinear);
        let mut obs = make_observation();
        obs.self_state.health = Some(20.0); // 20% health
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Idle);
        // InverseLinear(0.2) = 0.8
        assert!((c.evaluate(&ctx) - 0.8).abs() < 1e-6);
    }

    #[test]
    fn test_health_consideration_no_health_data() {
        let c = HealthConsideration::new(ResponseCurve::Linear);
        let mut obs = make_observation();
        obs.self_state.health = None;
        obs.self_state.max_health = None;
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Attack);
        // Defaults to 1.0 when no health data
        assert!((c.evaluate(&ctx) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_distance_to_threat_no_hostiles() {
        let c = DistanceToThreatConsideration::new(100.0, ResponseCurve::Linear);
        let obs = make_observation();
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Attack);
        // No hostiles → treated as max distance → Linear(1.0) = 1.0
        assert!((c.evaluate(&ctx) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_distance_to_threat_close_hostile() {
        let c = DistanceToThreatConsideration::new(100.0, ResponseCurve::Linear);
        let mut obs = make_observation();
        obs.visible_entities.push(make_hostile(25.0));
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Attack);
        // 25/100 = 0.25
        assert!((c.evaluate(&ctx) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_distance_to_threat_inverse_curve() {
        let c = DistanceToThreatConsideration::new(100.0, ResponseCurve::InverseLinear);
        let mut obs = make_observation();
        obs.visible_entities.push(make_hostile(25.0));
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Attack);
        // InverseLinear(0.25) = 0.75 → closer = higher score
        assert!((c.evaluate(&ctx) - 0.75).abs() < 1e-6);
    }

    #[test]
    fn test_distance_to_ally() {
        let c = DistanceToAllyConsideration::new(200.0, ResponseCurve::Linear);
        let mut obs = make_observation();
        obs.visible_entities.push(make_friendly(100.0));
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Idle);
        // 100/200 = 0.5
        assert!((c.evaluate(&ctx) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_distance_to_ally_no_allies() {
        let c = DistanceToAllyConsideration::new(200.0, ResponseCurve::Linear);
        let obs = make_observation();
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Idle);
        // No allies → 1.0
        assert!((c.evaluate(&ctx) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_threat_count_consideration() {
        let c = ThreatCountConsideration::new(4, ResponseCurve::Linear);
        let mut obs = make_observation();
        obs.visible_entities.push(make_hostile(50.0));
        obs.visible_entities.push(make_hostile(80.0));
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Attack);
        // 2/4 = 0.5
        assert!((c.evaluate(&ctx) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_threat_count_capped() {
        let c = ThreatCountConsideration::new(2, ResponseCurve::Linear);
        let mut obs = make_observation();
        obs.visible_entities.push(make_hostile(10.0));
        obs.visible_entities.push(make_hostile(20.0));
        obs.visible_entities.push(make_hostile(30.0));
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Attack);
        // 3/2 clamped to 1.0
        assert!((c.evaluate(&ctx) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_ammo_consideration() {
        let c = AmmoConsideration::new();
        let obs = make_observation();
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Attack);
        assert!((c.evaluate(&ctx) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cooldown_consideration_no_cooldowns() {
        let c = CooldownConsideration::new();
        let obs = make_observation();
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Attack);
        assert!((c.evaluate(&ctx) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cooldown_consideration_with_active_cooldown() {
        let c = CooldownConsideration::new();
        let mut obs = make_observation();
        obs.self_state.cooldowns.push(CooldownState {
            name: "attack".into(),
            remaining_ticks: 15,
            total_ticks: 30,
        });
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Attack);
        assert!((c.evaluate(&ctx) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn test_cooldown_consideration_with_expired_cooldown() {
        let c = CooldownConsideration::new();
        let mut obs = make_observation();
        obs.self_state.cooldowns.push(CooldownState {
            name: "attack".into(),
            remaining_ticks: 0,
            total_ticks: 30,
        });
        let ctx = ConsiderationContext::from_observation(&obs, &Action::Attack);
        assert!((c.evaluate(&ctx) - 1.0).abs() < 1e-6);
    }

    // ---- UtilityAction scoring tests ----

    #[test]
    fn test_utility_action_no_considerations() {
        let ua = UtilityAction {
            action: Action::Idle,
            considerations: vec![],
            weight: 1.0,
            bonus: 0.0,
        };
        let obs = make_observation();
        // No considerations → product = 1.0 → score = 1.0 * 1.0 + 0.0 = 1.0
        assert!((ua.score(&obs) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_utility_action_weight_and_bonus() {
        let ua = UtilityAction {
            action: Action::Idle,
            considerations: vec![],
            weight: 2.0,
            bonus: 0.5,
        };
        let obs = make_observation();
        // 2.0 * 1.0 + 0.5 = 2.5
        assert!((ua.score(&obs) - 2.5).abs() < 1e-6);
    }

    #[test]
    fn test_utility_action_multiplicative_considerations() {
        let ua = UtilityAction {
            action: Action::Attack,
            considerations: vec![
                Box::new(HealthConsideration::new(ResponseCurve::Linear)),
                Box::new(AmmoConsideration::new()),
            ],
            weight: 1.0,
            bonus: 0.0,
        };
        let mut obs = make_observation();
        obs.self_state.health = Some(50.0);
        // health=0.5, ammo=1.0 → product=0.5 → score=0.5
        assert!((ua.score(&obs) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_utility_action_zero_consideration_vetoes() {
        let ua = UtilityAction {
            action: Action::Attack,
            considerations: vec![
                Box::new(HealthConsideration::new(ResponseCurve::Linear)),
                Box::new(CooldownConsideration::new()),
            ],
            weight: 1.0,
            bonus: 0.0,
        };
        let mut obs = make_observation();
        obs.self_state.cooldowns.push(CooldownState {
            name: "attack".into(),
            remaining_ticks: 10,
            total_ticks: 30,
        });
        // cooldown=0.0 vetoes entire product → score = 1.0 * 0.0 + 0.0 = 0.0
        assert!((ua.score(&obs) - 0.0).abs() < 1e-6);
    }

    // ---- UtilityBrain tests ----

    #[test]
    fn test_brain_evaluate_returns_sorted() {
        let brain = UtilityBrain::new()
            .add_action(Action::Idle, |a| a.weight(0.1))
            .add_action(Action::Attack, |a| a.weight(2.0))
            .add_action(Action::Stop, |a| a.weight(0.5))
            .build();

        let obs = make_observation();
        let scored = brain.evaluate(&obs);

        assert_eq!(scored.len(), 3);
        // Highest score first
        assert!(scored[0].1 >= scored[1].1);
        assert!(scored[1].1 >= scored[2].1);
        // Attack (2.0) should be first
        assert!((scored[0].1 - 2.0).abs() < 1e-6);
    }

    #[test]
    fn test_brain_select_best() {
        let brain = UtilityBrain::new()
            .add_action(Action::Idle, |a| a.weight(0.1))
            .add_action(Action::Attack, |a| a.weight(2.0))
            .build();

        let obs = make_observation();
        let best = brain.select_best(&obs);
        assert!(matches!(best, Action::Attack));
    }

    #[test]
    fn test_brain_select_best_all_zero_returns_idle() {
        let brain = UtilityBrain::new()
            .add_action(Action::Attack, |a| a.weight(0.0))
            .add_action(Action::Stop, |a| a.weight(0.0))
            .build();

        let obs = make_observation();
        let best = brain.select_best(&obs);
        assert!(matches!(best, Action::Idle));
    }

    #[test]
    fn test_brain_select_best_empty_brain_returns_idle() {
        let brain = UtilityBrain::new().build();
        let obs = make_observation();
        let best = brain.select_best(&obs);
        assert!(matches!(best, Action::Idle));
    }

    #[test]
    fn test_brain_dynamic_scoring_health_changes_winner() {
        let brain = UtilityBrain::new()
            .add_action(Action::Attack, |a| {
                a.weight(1.5)
                    .consideration(HealthConsideration::new(ResponseCurve::Linear))
            })
            .add_action(Action::Idle, |a| {
                a.weight(1.0)
                    .consideration(HealthConsideration::new(ResponseCurve::InverseLinear))
            })
            .build();

        // Full health: Attack wins (1.5 * 1.0 = 1.5 vs 1.0 * 0.0 = 0.0)
        let mut obs = make_observation();
        let best = brain.select_best(&obs);
        assert!(matches!(best, Action::Attack));

        // Low health: Idle wins (1.5 * 0.1 = 0.15 vs 1.0 * 0.9 = 0.9)
        obs.self_state.health = Some(10.0);
        let best = brain.select_best(&obs);
        assert!(matches!(best, Action::Idle));
    }

    #[test]
    fn test_brain_bonus_can_tip_winner() {
        let brain = UtilityBrain::new()
            .add_action(Action::Attack, |a| a.weight(1.0))
            .add_action(Action::Idle, |a| a.weight(0.5).bonus(0.6))
            .build();

        let obs = make_observation();
        let scored = brain.evaluate(&obs);
        // Attack: 1.0 * 1.0 + 0.0 = 1.0
        // Idle: 0.5 * 1.0 + 0.6 = 1.1
        // Idle wins via bonus
        assert!(scored[0].1 > scored[1].1);
        let best = brain.select_best(&obs);
        assert!(matches!(best, Action::Idle));
    }

    // ---- Builder pattern tests ----

    #[test]
    fn test_builder_pattern_compiles_and_works() {
        let brain = UtilityBrain::new()
            .add_action(Action::Attack, |a| {
                a.weight(1.5)
                    .consideration(HealthConsideration::new(ResponseCurve::Linear))
                    .consideration(DistanceToThreatConsideration::new(
                        100.0,
                        ResponseCurve::InverseLinear,
                    ))
            })
            .add_action(Action::Idle, |a| {
                a.weight(1.0)
                    .consideration(HealthConsideration::new(ResponseCurve::InverseLinear))
            })
            .build();

        let obs = make_observation();
        let scored = brain.evaluate(&obs);
        assert_eq!(scored.len(), 2);
    }

    #[test]
    fn test_builder_default_weight_is_one() {
        let brain = UtilityBrain::new()
            .add_action(Action::Attack, |a| a)
            .build();

        let obs = make_observation();
        let scored = brain.evaluate(&obs);
        // Default weight 1.0, no considerations → product 1.0 → score 1.0
        assert!((scored[0].1 - 1.0).abs() < 1e-6);
    }

    // ---- UtilityAgent tests ----

    #[test]
    fn test_utility_agent_trait_impl() {
        let brain = UtilityBrain::new()
            .add_action(Action::Attack, |a| a.weight(2.0))
            .add_action(Action::Idle, |a| a.weight(0.1))
            .build();

        let mut agent = UtilityAgent::new(brain);
        assert_eq!(agent.agent_type(), AgentType::ScriptedNpc);

        // No observation yet → Idle
        let actions = agent.decide();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::Idle));

        // With observation → Attack (highest weight)
        agent.observe(make_observation());
        let actions = agent.decide();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::Attack));
    }

    #[test]
    fn test_utility_agent_custom_id_and_type() {
        let brain = UtilityBrain::new().build();
        let custom_id = AgentId::new();
        let agent = UtilityAgent::new(brain)
            .with_id(custom_id)
            .with_agent_type(AgentType::NeuralAgent);

        assert_eq!(agent.id(), custom_id);
        assert_eq!(agent.agent_type(), AgentType::NeuralAgent);
    }

    #[test]
    fn test_utility_agent_constraints_accessible() {
        let brain = UtilityBrain::new().build();
        let mut agent = UtilityAgent::new(brain);

        assert_eq!(agent.constraints().actions_per_tick, 3);
        agent.constraints_mut().actions_per_tick = 5;
        assert_eq!(agent.constraints().actions_per_tick, 5);
    }
}
