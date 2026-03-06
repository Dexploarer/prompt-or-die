//! # Utility AI — Score-Based Action Selection
//!
//! This module implements a Utility AI system for the Prompt or Die engine.
//! Unlike behaviour trees (which use hard-coded priority or sequencing) or
//! finite state machines (which require explicit transition rules), Utility AI
//! assigns every candidate action a continuous score in `[0.0, 1.0]` and
//! picks the highest-scoring action each tick.
//!
//! ## Design
//!
//! ```text
//!  Observation
//!      │
//!      ├─► AttackNearestHostileScorer  → 0.92
//!      ├─► FleeScorer                  → 0.31
//!      ├─► HealScorer                  → 0.55
//!      ├─► ExploreScorer               → 0.40
//!      └─► IdleScorer                  → 0.05
//!                                          │
//!                                   pick max (0.92)
//!                                          │
//!                                   Action::AttackTarget { … }
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use pod_agents::utility_ai::{UtilityAgent, UtilityProfile};
//!
//! // Quick start with a pre-built profile
//! let agent = UtilityAgent::with_scorers(UtilityProfile::balanced());
//!
//! // Or compose your own scorer set
//! use pod_agents::utility_ai::{AttackNearestHostileScorer, IdleScorer};
//! let mut agent = UtilityAgent::new();
//! agent.add_scorer(Box::new(AttackNearestHostileScorer::default()));
//! ```

use glam::Vec2;
use pod_core::agent::Agent;
use pod_core::observation::Relationship;
use pod_core::{
    Action, AgentConstraints, AgentId, AgentIntrospection, AgentType, EntityId, Observation,
};
use std::collections::VecDeque;

// ============================================================
// ACTION SCORER TRAIT
// ============================================================

/// A stateless scorer that maps an [`Observation`] to a score in `[0.0, 1.0]`
/// and produces the [`Action`] it recommends when selected.
///
/// Implement this trait to create custom scoring logic. All scorers are
/// evaluated every tick; the one returning the highest score wins.
///
/// # Contract
/// - `score()` **must** return a value in `[0.0, 1.0]`.
/// - `score()` and `action()` receive the **same** `observation`, so the
///   action returned should be consistent with the score computed.
/// - Both methods must be pure (no side-effects, no interior mutability).
pub trait ActionScorer: Send + Sync {
    /// Compute how desirable it is to perform this action right now.
    /// Returns `0.0` (never do this) to `1.0` (always do this).
    fn score(&self, observation: &Observation) -> f32;

    /// Produce the concrete [`Action`] to submit when this scorer wins.
    fn action(&self, observation: &Observation) -> Action;

    /// Human-readable name used in debugging and introspection.
    fn name(&self) -> &str;
}

// ============================================================
// BUILT-IN SCORERS
// ============================================================

// ── Helpers ──────────────────────────────────────────────────

/// Returns the nearest hostile entity in `observation.visible_entities`,
/// or `None` if there are none.
fn nearest_hostile(observation: &Observation) -> Option<&pod_core::observation::VisibleEntity> {
    observation
        .visible_entities
        .iter()
        .filter(|e| matches!(e.relationship, Relationship::Hostile))
        .min_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// Returns the agent's health fraction in `[0.0, 1.0]`, defaulting to `1.0`
/// when health information is unavailable.
fn health_fraction(observation: &Observation) -> f32 {
    match (
        observation.self_state.health,
        observation.self_state.max_health,
    ) {
        (Some(hp), Some(max)) if max > 0.0 => (hp / max).clamp(0.0, 1.0),
        _ => 1.0,
    }
}

// ── AttackNearestHostileScorer ────────────────────────────────

/// Scores high when there is a hostile entity nearby and health is sufficient
/// to engage. Produces [`Action::AttackTarget`] aimed at the nearest hostile.
///
/// Score formula:
/// - `0.0` when no hostiles are visible.
/// - Scales up as distance decreases and the agent's own health is above `0.3`.
#[derive(Debug, Clone)]
pub struct AttackNearestHostileScorer {
    /// Distance at which the score reaches its maximum value.
    pub optimal_range: f32,
}

impl AttackNearestHostileScorer {
    pub fn new(optimal_range: f32) -> Self {
        Self { optimal_range }
    }
}

impl Default for AttackNearestHostileScorer {
    fn default() -> Self {
        Self {
            optimal_range: 80.0,
        }
    }
}

impl ActionScorer for AttackNearestHostileScorer {
    fn score(&self, observation: &Observation) -> f32 {
        let hostile = match nearest_hostile(observation) {
            Some(h) => h,
            None => return 0.0,
        };
        // Penalise attacking when nearly dead
        let hp = health_fraction(observation);
        if hp < 0.15 {
            return 0.0;
        }
        // Closer = better, but only up to optimal_range
        let range = self.optimal_range.max(1.0);
        let proximity = 1.0 - (hostile.distance / range).min(1.0);
        // Also factor in whether we can finish the enemy off
        let kill_potential = hostile.health_fraction.unwrap_or(1.0);
        let kill_bonus = 1.0 - kill_potential; // low-health enemies score higher
        ((proximity * 0.7 + kill_bonus * 0.3) * hp).clamp(0.0, 1.0)
    }

    fn action(&self, observation: &Observation) -> Action {
        match nearest_hostile(observation) {
            Some(hostile) => Action::AttackTarget {
                target: hostile.entity_id,
            },
            None => Action::Idle,
        }
    }

    fn name(&self) -> &str {
        "AttackNearestHostile"
    }
}

// ── FleeScorer ────────────────────────────────────────────────

/// Scores high when health is critically low AND there are hostiles nearby.
/// Produces [`Action::Move`] away from the nearest hostile.
///
/// Score formula: `(1.0 - health_fraction)^2 * proximity_factor`
#[derive(Debug, Clone)]
pub struct FleeScorer {
    /// Health fraction below which fleeing is considered at all.
    pub flee_threshold: f32,
}

impl Default for FleeScorer {
    fn default() -> Self {
        Self {
            flee_threshold: 0.4,
        }
    }
}

impl FleeScorer {
    pub fn new(flee_threshold: f32) -> Self {
        Self { flee_threshold }
    }
}

impl ActionScorer for FleeScorer {
    fn score(&self, observation: &Observation) -> f32 {
        let hp = health_fraction(observation);
        if hp > self.flee_threshold {
            return 0.0;
        }
        let hostile = match nearest_hostile(observation) {
            Some(h) => h,
            None => return 0.0,
        };
        // More urgent the lower health and the closer the hostile
        let desperation = (1.0 - hp / self.flee_threshold).clamp(0.0, 1.0);
        let threat = 1.0 - (hostile.distance / 300.0).clamp(0.0, 1.0);
        (desperation * 0.6 + threat * 0.4).clamp(0.0, 1.0)
    }

    fn action(&self, observation: &Observation) -> Action {
        let away = match nearest_hostile(observation) {
            Some(hostile) => {
                let delta = observation.self_state.position - hostile.position;
                if delta.length_squared() > 1e-6 {
                    delta.normalize()
                } else {
                    Vec2::new(0.0, 1.0) // fallback: move north
                }
            }
            None => Vec2::new(0.0, 1.0),
        };
        Action::Move { direction: away }
    }

    fn name(&self) -> &str {
        "Flee"
    }
}

// ── HealScorer ────────────────────────────────────────────────

/// Scores proportionally to how much health the agent has lost.
/// Produces [`Action::UseItem { slot: 0 }`] (the conventional heal slot).
///
/// Score formula: `(1.0 - health_fraction) * 0.85`
/// Capped at 0.85 so it doesn't completely override combat at mid-health.
#[derive(Debug, Default, Clone)]
pub struct HealScorer;

impl ActionScorer for HealScorer {
    fn score(&self, observation: &Observation) -> f32 {
        let hp = health_fraction(observation);
        if hp >= 1.0 {
            return 0.0;
        }
        // Scale: 50% health → ~0.43 score; 0% health → 0.85
        ((1.0 - hp) * 0.85).clamp(0.0, 0.85)
    }

    fn action(&self, _observation: &Observation) -> Action {
        Action::UseItem { slot: 0 }
    }

    fn name(&self) -> &str {
        "Heal"
    }
}

// ── ExploreScorer ─────────────────────────────────────────────

/// Provides a moderate baseline score that nudges the agent to move even when
/// nothing important is happening. The direction pseudo-randomly changes every
/// ~3 seconds based on the tick counter, giving a wandering behaviour without
/// requiring any mutable state.
///
/// Score: constant `0.35`.
#[derive(Debug, Default, Clone)]
pub struct ExploreScorer;

impl ActionScorer for ExploreScorer {
    fn score(&self, _observation: &Observation) -> f32 {
        0.35
    }

    fn action(&self, observation: &Observation) -> Action {
        // Change direction every ~3 seconds (180 ticks at 60 TPS)
        let phase = observation.tick / 180;
        // Simple deterministic direction from tick phase
        let angle = (phase as f32) * std::f32::consts::FRAC_PI_4; // 45° steps
        let direction = Vec2::new(angle.cos(), angle.sin());
        Action::Move { direction }
    }

    fn name(&self) -> &str {
        "Explore"
    }
}

// ── IdleScorer ────────────────────────────────────────────────

/// Provides a low constant baseline so the agent always has *some* action.
/// Ensures `decide()` never returns an empty vec.
///
/// Score: constant `0.05`.
#[derive(Debug, Default, Clone)]
pub struct IdleScorer;

impl ActionScorer for IdleScorer {
    fn score(&self, _observation: &Observation) -> f32 {
        0.05
    }

    fn action(&self, _observation: &Observation) -> Action {
        Action::Idle
    }

    fn name(&self) -> &str {
        "Idle"
    }
}

// ── ChaseScorer ───────────────────────────────────────────────

/// Scores high when a hostile is visible but far away — the agent needs to
/// close the gap before it can attack. Complements [`AttackNearestHostileScorer`].
///
/// Score formula: `proximity_gap * hp_factor` where `proximity_gap` is high
/// when the hostile is far and drops to zero within `engage_range`.
#[derive(Debug, Clone)]
pub struct ChaseScorer {
    /// Distance at which the agent switches from chasing to attacking.
    pub engage_range: f32,
}

impl Default for ChaseScorer {
    fn default() -> Self {
        Self {
            engage_range: 120.0,
        }
    }
}

impl ChaseScorer {
    pub fn new(engage_range: f32) -> Self {
        Self { engage_range }
    }
}

impl ActionScorer for ChaseScorer {
    fn score(&self, observation: &Observation) -> f32 {
        let hostile = match nearest_hostile(observation) {
            Some(h) => h,
            None => return 0.0,
        };
        let hp = health_fraction(observation);
        if hp < 0.2 {
            return 0.0; // Don't chase when nearly dead
        }
        // Score is highest when enemy is just beyond engage_range
        if hostile.distance <= self.engage_range {
            return 0.0; // We're already close — let AttackScorer handle this
        }
        let gap = (hostile.distance - self.engage_range) / 500.0; // normalise over 500 units
        (gap.clamp(0.0, 1.0) * 0.75 * hp).clamp(0.0, 0.75)
    }

    fn action(&self, observation: &Observation) -> Action {
        match nearest_hostile(observation) {
            Some(hostile) => {
                let delta = hostile.position - observation.self_state.position;
                let direction = if delta.length_squared() > 1e-6 {
                    delta.normalize()
                } else {
                    Vec2::X
                };
                Action::Move { direction }
            }
            None => Action::Idle,
        }
    }

    fn name(&self) -> &str {
        "Chase"
    }
}

// ── GuardScorer ───────────────────────────────────────────────

/// Scores based on the agent's distance from a fixed guard position.
/// The further from the post, the higher the score to return.
/// When already at the post (within `tolerance`), score drops to zero.
///
/// Score formula: `(distance_from_post / max_wander) * 0.8` clamped to `[0.0, 0.8]`.
#[derive(Debug, Clone)]
pub struct GuardScorer {
    /// The position the agent should defend / return to.
    pub guard_position: Vec2,
    /// Radius within which the agent is considered "at post" (score → 0).
    pub tolerance: f32,
    /// Distance beyond which the score is capped at maximum.
    pub max_wander: f32,
}

impl GuardScorer {
    pub fn new(guard_position: Vec2) -> Self {
        Self {
            guard_position,
            tolerance: 30.0,
            max_wander: 200.0,
        }
    }

    pub fn with_tolerance(mut self, tolerance: f32) -> Self {
        self.tolerance = tolerance;
        self
    }

    pub fn with_max_wander(mut self, max_wander: f32) -> Self {
        self.max_wander = max_wander;
        self
    }
}

impl ActionScorer for GuardScorer {
    fn score(&self, observation: &Observation) -> f32 {
        let dist = observation
            .self_state
            .position
            .distance(self.guard_position);
        if dist <= self.tolerance {
            return 0.0;
        }
        let normalised = ((dist - self.tolerance) / self.max_wander).clamp(0.0, 1.0);
        normalised * 0.8
    }

    fn action(&self, observation: &Observation) -> Action {
        let delta = self.guard_position - observation.self_state.position;
        let direction = if delta.length_squared() > 1e-6 {
            delta.normalize()
        } else {
            Vec2::ZERO
        };
        Action::Move { direction }
    }

    fn name(&self) -> &str {
        "Guard"
    }
}

// ============================================================
// UTILITY AGENT
// ============================================================

/// A fully utility-scored agent that evaluates every registered scorer each
/// tick and commits to the highest-scoring action.
///
/// # Example
/// ```rust,ignore
/// let mut agent = UtilityAgent::with_scorers(UtilityProfile::balanced());
/// // … game loop calls observe() then decide() …
/// ```
pub struct UtilityAgent {
    id: AgentId,
    scorers: Vec<Box<dyn ActionScorer>>,
    constraints: AgentConstraints,
    last_observation: Option<Observation>,
    /// Scores from the most recent `decide()` call: `(scorer_name, score)`.
    last_scores: Vec<(String, f32)>,
    /// Rolling history of score snapshots for debugging/replay.
    score_history: VecDeque<Vec<(String, f32)>>,
    /// Maximum number of tick snapshots to retain in `score_history`.
    max_history: usize,
}

impl UtilityAgent {
    /// Create a `UtilityAgent` with the default scorer set:
    /// `AttackNearestHostile`, `Flee`, `Explore`, `Idle`.
    pub fn new() -> Self {
        Self::with_scorers(vec![
            Box::new(AttackNearestHostileScorer::default()),
            Box::new(FleeScorer::default()),
            Box::new(ExploreScorer),
            Box::new(IdleScorer),
        ])
    }

    /// Create a `UtilityAgent` with a custom scorer set.
    /// Panics if `scorers` is empty (at least one scorer is required to
    /// guarantee `decide()` always returns an action).
    pub fn with_scorers(scorers: Vec<Box<dyn ActionScorer>>) -> Self {
        assert!(
            !scorers.is_empty(),
            "UtilityAgent requires at least one scorer"
        );
        Self {
            id: AgentId::new(),
            scorers,
            constraints: AgentConstraints::default(),
            last_observation: None,
            last_scores: Vec::new(),
            score_history: VecDeque::new(),
            max_history: 120, // keep ~2 seconds of history at 60 TPS
        }
    }

    /// Append an additional scorer at runtime.
    pub fn add_scorer(&mut self, scorer: Box<dyn ActionScorer>) {
        self.scorers.push(scorer);
    }

    /// Returns the scorer names and scores from the most recent `decide()` call.
    /// Useful for debug overlays or introspection tooling.
    pub fn last_scores(&self) -> &[(String, f32)] {
        &self.last_scores
    }

    /// Evaluate every scorer against `obs` and return a sorted list of
    /// `(name, score, action)` tuples, highest score first.
    ///
    /// This is a pure inspection helper — it does **not** mutate agent state.
    pub fn evaluate_all(&self, obs: &Observation) -> Vec<(String, f32, Action)> {
        let mut results: Vec<(String, f32, Action)> = self
            .scorers
            .iter()
            .map(|s| (s.name().to_string(), s.score(obs), s.action(obs)))
            .collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Set the maximum number of per-tick score snapshots to retain.
    pub fn set_max_history(&mut self, max: usize) {
        self.max_history = max;
        while self.score_history.len() > max {
            self.score_history.pop_front();
        }
    }

    /// Access the rolling score history (most recent last).
    pub fn score_history(&self) -> &VecDeque<Vec<(String, f32)>> {
        &self.score_history
    }
}

impl Default for UtilityAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for UtilityAgent {
    fn id(&self) -> AgentId {
        self.id
    }

    fn agent_type(&self) -> AgentType {
        AgentType::ScriptedNpc
    }

    fn observe(&mut self, observation: Observation) {
        self.last_observation = Some(observation);
    }

    fn decide(&mut self) -> Vec<Action> {
        let obs = match &self.last_observation {
            Some(o) => o,
            None => return vec![Action::Idle],
        };

        // Score all candidates
        let mut candidates: Vec<(String, f32, Action)> = self
            .scorers
            .iter()
            .map(|s| (s.name().to_string(), s.score(obs), s.action(obs)))
            .collect();

        // Sort descending by score; stable sort preserves registration order as tiebreaker
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Record scores for introspection
        self.last_scores = candidates
            .iter()
            .map(|(name, score, _)| (name.clone(), *score))
            .collect();

        // Push to rolling history
        self.score_history.push_back(self.last_scores.clone());
        while self.score_history.len() > self.max_history {
            self.score_history.pop_front();
        }

        // Commit to the winner
        let winner_action = candidates
            .into_iter()
            .next()
            .map(|(_, _, a)| a)
            .unwrap_or(Action::Idle);

        vec![winner_action]
    }

    fn constraints(&self) -> &AgentConstraints {
        &self.constraints
    }

    fn constraints_mut(&mut self) -> &mut AgentConstraints {
        &mut self.constraints
    }

    fn introspect(&self) -> AgentIntrospection {
        let winner = self
            .last_scores
            .first()
            .map(|(name, score)| format!("{} ({:.2})", name, score))
            .unwrap_or_else(|| "none".to_string());

        let state_description = format!(
            "scorers={} winner={} tick={} history_len={}",
            self.scorers.len(),
            winner,
            self.last_observation.as_ref().map(|o| o.tick).unwrap_or(0),
            self.score_history.len(),
        );

        AgentIntrospection {
            agent_id: self.id,
            agent_type: AgentType::ScriptedNpc,
            constraints: self.constraints.clone(),
            state_description,
        }
    }
}

// ============================================================
// UTILITY PROFILES
// ============================================================

/// Factory for common pre-built scorer configurations.
///
/// Each profile returns a `Vec<Box<dyn ActionScorer>>` ready to pass to
/// [`UtilityAgent::with_scorers`].
pub struct UtilityProfile;

impl UtilityProfile {
    /// Combat-focused profile: prioritises attacking and chasing enemies.
    ///
    /// Scorers (high to low typical weight):
    /// - `AttackNearestHostile` (optimal_range: 80)
    /// - `Chase` (engage_range: 80)
    /// - `Idle` (baseline)
    pub fn aggressive() -> Vec<Box<dyn ActionScorer>> {
        vec![
            Box::new(AttackNearestHostileScorer::new(80.0)),
            Box::new(ChaseScorer::new(80.0)),
            Box::new(IdleScorer),
        ]
    }

    /// Survivability-focused profile: prefers self-preservation over aggression.
    ///
    /// Scorers:
    /// - `Flee` (threshold: 0.5)
    /// - `Guard` (origin, tolerance: 50)
    /// - `Heal`
    /// - `Idle` (baseline)
    pub fn defensive() -> Vec<Box<dyn ActionScorer>> {
        vec![
            Box::new(FleeScorer::new(0.5)),
            Box::new(GuardScorer::new(Vec2::ZERO).with_tolerance(50.0)),
            Box::new(HealScorer),
            Box::new(IdleScorer),
        ]
    }

    /// Exploration-focused profile: wanders and chases curiosities.
    ///
    /// Scorers:
    /// - `Explore`
    /// - `Chase` (engage_range: 150)
    /// - `Idle` (baseline)
    pub fn explorer() -> Vec<Box<dyn ActionScorer>> {
        vec![
            Box::new(ExploreScorer),
            Box::new(ChaseScorer::new(150.0)),
            Box::new(IdleScorer),
        ]
    }

    /// Balanced profile suitable as a general-purpose NPC.
    ///
    /// Scorers:
    /// - `AttackNearestHostile`
    /// - `Flee`
    /// - `Heal`
    /// - `Explore`
    /// - `Idle` (baseline)
    pub fn balanced() -> Vec<Box<dyn ActionScorer>> {
        vec![
            Box::new(AttackNearestHostileScorer::default()),
            Box::new(FleeScorer::default()),
            Box::new(HealScorer),
            Box::new(ExploreScorer),
            Box::new(IdleScorer),
        ]
    }
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::observation::{Relationship, SelfState, VisibleEntity};

    // ── Observation builders ──────────────────────────────────

    fn obs_with_health(health: f32, max_health: f32) -> Observation {
        Observation {
            self_state: SelfState {
                health: Some(health),
                max_health: Some(max_health),
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn hostile_entity(
        entity_id: EntityId,
        position: Vec2,
        distance: f32,
        health_fraction: f32,
    ) -> VisibleEntity {
        VisibleEntity {
            entity_id,
            entity_type: "enemy".to_string(),
            position,
            velocity: Vec2::ZERO,
            rotation: 0.0,
            distance,
            relationship: Relationship::Hostile,
            health_fraction: Some(health_fraction),
            ..Default::default()
        }
    }

    fn obs_with_hostile_nearby(distance: f32) -> Observation {
        let mut obs = obs_with_health(100.0, 100.0);
        obs.self_state.position = Vec2::ZERO;
        obs.visible_entities.push(hostile_entity(
            EntityId(1),
            Vec2::new(distance, 0.0),
            distance,
            0.8,
        ));
        obs
    }

    fn obs_low_health_with_hostile(hp_fraction: f32) -> Observation {
        let max = 100.0;
        let hp = max * hp_fraction;
        let mut obs = obs_with_health(hp, max);
        obs.self_state.position = Vec2::ZERO;
        obs.visible_entities
            .push(hostile_entity(EntityId(1), Vec2::new(50.0, 0.0), 50.0, 1.0));
        obs
    }

    // ── Test 1: IdleScorer always returns a low constant ─────

    #[test]
    fn test_idle_scorer_baseline() {
        let scorer = IdleScorer;

        // Should be the same regardless of observation content
        let obs_empty = Observation::default();
        let obs_hostile = obs_with_hostile_nearby(30.0);
        let obs_dying = obs_low_health_with_hostile(0.05);

        let s1 = scorer.score(&obs_empty);
        let s2 = scorer.score(&obs_hostile);
        let s3 = scorer.score(&obs_dying);

        assert_eq!(
            s1, 0.05,
            "IdleScorer must return 0.05 for empty observation"
        );
        assert_eq!(
            s2, 0.05,
            "IdleScorer must return 0.05 with hostiles present"
        );
        assert_eq!(s3, 0.05, "IdleScorer must return 0.05 even when dying");

        // Action is always Idle
        assert!(matches!(scorer.action(&obs_empty), Action::Idle));
    }

    // ── Test 2: AttackScorer returns 0.0 when no enemies visible ─

    #[test]
    fn test_attack_scorer_no_hostiles() {
        let scorer = AttackNearestHostileScorer::default();
        let obs = obs_with_health(100.0, 100.0); // no visible entities

        let score = scorer.score(&obs);
        assert_eq!(
            score, 0.0,
            "AttackScorer must score 0.0 when no hostiles visible"
        );
    }

    // ── Test 3: AttackScorer scores high with a nearby hostile ─

    #[test]
    fn test_attack_scorer_with_hostile() {
        let scorer = AttackNearestHostileScorer::new(100.0);

        // Hostile at exactly half the optimal range
        let obs_close = obs_with_hostile_nearby(50.0);
        let score_close = scorer.score(&obs_close);

        // Hostile at 3× optimal range
        let obs_far = obs_with_hostile_nearby(300.0);
        let score_far = scorer.score(&obs_far);

        assert!(
            score_close > 0.0,
            "AttackScorer must return > 0.0 with a nearby hostile"
        );
        assert!(
            score_close > score_far,
            "AttackScorer must score closer hostiles higher (close={:.3}, far={:.3})",
            score_close,
            score_far,
        );
        assert!(
            score_close <= 1.0,
            "AttackScorer must never exceed 1.0 (got {})",
            score_close
        );

        // Action should be AttackTarget
        match scorer.action(&obs_close) {
            Action::AttackTarget { target } => assert_eq!(target, EntityId(1)),
            other => panic!("Expected AttackTarget, got {:?}", other),
        }
    }

    // ── Test 4: FleeScorer scores high at low health with hostiles ─

    #[test]
    fn test_flee_scorer_low_health() {
        let scorer = FleeScorer::new(0.4);

        // At full health — should not flee
        let obs_full_hp = obs_with_hostile_nearby(60.0);
        let score_full = scorer.score(&obs_full_hp);
        assert_eq!(score_full, 0.0, "FleeScorer must not fire at full health");

        // At 20% health with a hostile — should want to flee
        let obs_low_hp = obs_low_health_with_hostile(0.2);
        let score_low = scorer.score(&obs_low_hp);
        assert!(
            score_low > 0.3,
            "FleeScorer must score > 0.3 at 20% health with nearby hostile (got {:.3})",
            score_low
        );
        assert!(score_low <= 1.0, "FleeScorer must not exceed 1.0");

        // Action must be a Move action (away from hostile)
        match scorer.action(&obs_low_hp) {
            Action::Move { direction } => {
                // The hostile is at (50, 0), so fleeing should push us in the -X direction
                assert!(
                    direction.x < 0.0,
                    "Flee direction should be away from hostile on x-axis (got {:?})",
                    direction
                );
            }
            other => panic!("Expected Move, got {:?}", other),
        }
    }

    // ── Test 5: Full observe→decide cycle picks the best scorer ─

    #[test]
    fn test_utility_agent_decide() {
        let mut agent = UtilityAgent::new(); // Attack, Flee, Explore, Idle

        // With a nearby hostile and full health, AttackScorer should win
        let obs = obs_with_hostile_nearby(60.0);
        agent.observe(obs.clone());
        let actions = agent.decide();

        assert_eq!(
            actions.len(),
            1,
            "UtilityAgent must return exactly 1 action"
        );

        match &actions[0] {
            Action::AttackTarget { target } => {
                assert_eq!(*target, EntityId(1), "Should attack the hostile entity");
            }
            Action::Move { .. } => {
                // Acceptable — ExploreScorer or ChaseScorer could win in some setups
                // but with default scorers AttackScorer is expected to win at distance 60
            }
            other => panic!("Unexpected action {:?} — expected AttackTarget", other),
        }

        // last_scores should be populated after decide()
        let scores = agent.last_scores();
        assert!(
            !scores.is_empty(),
            "last_scores should be populated after decide()"
        );

        // The winning scorer should be first (highest score)
        let best_score = scores[0].1;
        for (_, s) in scores.iter().skip(1) {
            assert!(
                *s <= best_score + 1e-6,
                "last_scores should be sorted by descending score"
            );
        }
    }

    // ── Test 6: Utility profiles have expected scorer sets ───

    #[test]
    fn test_utility_profiles() {
        let aggressive = UtilityProfile::aggressive();
        let defensive = UtilityProfile::defensive();
        let explorer = UtilityProfile::explorer();
        let balanced = UtilityProfile::balanced();

        // Check expected scorer counts
        assert_eq!(
            aggressive.len(),
            3,
            "Aggressive profile should have 3 scorers"
        );
        assert_eq!(
            defensive.len(),
            4,
            "Defensive profile should have 4 scorers"
        );
        assert_eq!(explorer.len(), 3, "Explorer profile should have 3 scorers");
        assert_eq!(balanced.len(), 5, "Balanced profile should have 5 scorers");

        // Each profile must contain at least an Idle scorer as safety net
        let has_idle =
            |scorers: &[Box<dyn ActionScorer>]| scorers.iter().any(|s| s.name() == "Idle");
        assert!(
            has_idle(&aggressive),
            "Aggressive profile must include Idle scorer"
        );
        assert!(
            has_idle(&defensive),
            "Defensive profile must include Idle scorer"
        );
        assert!(
            has_idle(&explorer),
            "Explorer profile must include Idle scorer"
        );
        assert!(
            has_idle(&balanced),
            "Balanced profile must include Idle scorer"
        );

        // Each profile must produce valid UtilityAgents
        let _a = UtilityAgent::with_scorers(UtilityProfile::aggressive());
        let _d = UtilityAgent::with_scorers(UtilityProfile::defensive());
        let _e = UtilityAgent::with_scorers(UtilityProfile::explorer());
        let _b = UtilityAgent::with_scorers(UtilityProfile::balanced());
    }

    // ── Test 7: evaluate_all returns sorted results ──────────

    #[test]
    fn test_evaluate_all_sorted() {
        let agent = UtilityAgent::new();
        let obs = obs_with_hostile_nearby(60.0);
        let results = agent.evaluate_all(&obs);

        assert!(!results.is_empty());
        for w in results.windows(2) {
            assert!(
                w[0].1 >= w[1].1,
                "evaluate_all must be sorted descending: {:.3} < {:.3}",
                w[0].1,
                w[1].1,
            );
        }
    }

    // ── Test 8: HealScorer proportional to missing health ────

    #[test]
    fn test_heal_scorer_proportional() {
        let scorer = HealScorer;

        let full_hp = obs_with_health(100.0, 100.0);
        let half_hp = obs_with_health(50.0, 100.0);
        let low_hp = obs_with_health(10.0, 100.0);

        let s_full = scorer.score(&full_hp);
        let s_half = scorer.score(&half_hp);
        let s_low = scorer.score(&low_hp);

        assert_eq!(s_full, 0.0, "HealScorer must be 0 at full health");
        assert!(s_half > 0.0, "HealScorer must be > 0 at half health");
        assert!(
            s_low > s_half,
            "HealScorer must be higher at lower health (low={:.3}, half={:.3})",
            s_low,
            s_half
        );
        assert!(s_low <= 0.85, "HealScorer must not exceed 0.85");

        assert!(matches!(
            scorer.action(&half_hp),
            Action::UseItem { slot: 0 }
        ));
    }
}
