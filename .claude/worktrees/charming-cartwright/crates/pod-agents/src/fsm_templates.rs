//! Production-quality FSM templates with observation-aware transitions.
//!
//! Provides:
//! - `FsmCondition` — composable conditions evaluated against `Observation`
//! - `FsmStateConfig` — per-state behavior with entry/exit actions
//! - `SmartFsm` — full FSM that evaluates conditions and transitions
//! - Pre-built templates: `combat_fsm`, `patrol_fsm`, `guard_fsm`

use crate::scripted_agent::FsmState;
use glam::Vec2;
use pod_core::action::Action;
use pod_core::observation::{Observation, Relationship};

// ---------------------------------------------------------------------------
// FsmCondition — composable, observation-aware transition conditions
// ---------------------------------------------------------------------------

/// A condition that can be evaluated against an `Observation` to decide
/// whether an FSM transition should fire.
pub enum FsmCondition {
    /// Hostile entity within range
    HostileInRange(f32),
    /// Health below threshold (0.0-1.0 fraction)
    HealthBelow(f32),
    /// Health above threshold (0.0-1.0 fraction)
    HealthAbove(f32),
    /// Been in current state for at least N ticks
    TicksInState(u32),
    /// No hostile entities visible
    NoHostilesVisible,
    /// Health is zero (dead)
    Dead,
    /// Always true (unconditional transition)
    Always,
    /// Both conditions must be true
    And(Box<FsmCondition>, Box<FsmCondition>),
    /// Either condition must be true
    Or(Box<FsmCondition>, Box<FsmCondition>),
    /// Negate a condition
    Not(Box<FsmCondition>),
}

impl FsmCondition {
    /// Evaluate this condition against the current observation and state duration.
    pub fn evaluate(&self, obs: &Observation, ticks_in_state: u32) -> bool {
        match self {
            FsmCondition::HostileInRange(range) => obs
                .visible_entities
                .iter()
                .any(|e| is_hostile(&e.relationship) && e.distance <= *range),

            FsmCondition::HealthBelow(threshold) => {
                health_fraction(obs).map_or(false, |frac| frac < *threshold)
            }

            FsmCondition::HealthAbove(threshold) => {
                health_fraction(obs).map_or(false, |frac| frac > *threshold)
            }

            FsmCondition::TicksInState(n) => ticks_in_state >= *n,

            FsmCondition::NoHostilesVisible => !obs
                .visible_entities
                .iter()
                .any(|e| is_hostile(&e.relationship)),

            FsmCondition::Dead => {
                health_fraction(obs).map_or(false, |frac| frac <= 0.0)
            }

            FsmCondition::Always => true,

            FsmCondition::And(a, b) => {
                a.evaluate(obs, ticks_in_state) && b.evaluate(obs, ticks_in_state)
            }

            FsmCondition::Or(a, b) => {
                a.evaluate(obs, ticks_in_state) || b.evaluate(obs, ticks_in_state)
            }

            FsmCondition::Not(inner) => !inner.evaluate(obs, ticks_in_state),
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Check if a relationship is hostile (pattern-match since Relationship
/// does not derive PartialEq).
fn is_hostile(r: &Relationship) -> bool {
    matches!(r, Relationship::Hostile)
}

/// Compute the agent's health fraction (0.0–1.0).
/// Returns `None` when health info is absent.
fn health_fraction(obs: &Observation) -> Option<f32> {
    match (obs.self_state.health, obs.self_state.max_health) {
        (Some(hp), Some(max)) if max > 0.0 => Some(hp / max),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// FsmStateConfig — per-state behavior
// ---------------------------------------------------------------------------

/// Configuration for a single FSM state: what action to produce and
/// optional entry/exit side-effects.
pub struct FsmStateConfig {
    /// Which FSM state this config belongs to.
    pub state: FsmState,
    /// Produces the action(s) for the agent while in this state.
    pub action_fn: Box<dyn Fn(&Observation) -> Action + Send + Sync>,
    /// One-shot action emitted when entering this state.
    pub on_enter: Option<Action>,
    /// One-shot action emitted when leaving this state.
    pub on_exit: Option<Action>,
}

// ---------------------------------------------------------------------------
// SmartTransition
// ---------------------------------------------------------------------------

/// A transition with a composable condition and a priority.
/// Higher-priority transitions are evaluated first.
pub struct SmartTransition {
    pub from: FsmState,
    pub to: FsmState,
    pub condition: FsmCondition,
    /// Higher values are checked first.
    pub priority: i32,
}

// ---------------------------------------------------------------------------
// SmartFsm — production FSM
// ---------------------------------------------------------------------------

/// A production-quality finite state machine that:
/// - evaluates `FsmCondition`s each tick against the current `Observation`
/// - transitions between states with entry/exit hooks
/// - records state history for debugging / analytics
pub struct SmartFsm {
    current_state: FsmState,
    /// Vec-backed map (FsmState has no Hash, and state count is small).
    states: Vec<(FsmState, FsmStateConfig)>,
    transitions: Vec<SmartTransition>,
    ticks_in_state: u32,
    previous_state: Option<FsmState>,
    state_history: Vec<(FsmState, u32)>,
    max_history: usize,
}

impl SmartFsm {
    /// Create a new `SmartFsm` starting in `initial_state`.
    pub fn new(initial_state: FsmState) -> Self {
        Self {
            current_state: initial_state,
            states: Vec::new(),
            transitions: Vec::new(),
            ticks_in_state: 0,
            previous_state: None,
            state_history: Vec::new(),
            max_history: 64,
        }
    }

    /// Register a state configuration.
    pub fn add_state(&mut self, config: FsmStateConfig) {
        self.states.push((config.state, config));
    }

    /// Register a transition.
    pub fn add_transition(
        &mut self,
        from: FsmState,
        to: FsmState,
        condition: FsmCondition,
        priority: i32,
    ) {
        self.transitions.push(SmartTransition {
            from,
            to,
            condition,
            priority,
        });
    }

    /// Advance one tick. Evaluates transitions (highest priority first),
    /// performs state change if a condition fires, and returns the actions
    /// for this tick (including any entry/exit actions on transition).
    pub fn tick(&mut self, obs: &Observation) -> Vec<Action> {
        let mut actions: Vec<Action> = Vec::new();

        // --- evaluate transitions from current state (sorted by priority desc) ---
        // Collect matching transition targets first so we don't borrow `self`
        // mutably while iterating transitions.
        let mut best_target: Option<FsmState> = None;
        let mut best_priority = i32::MIN;

        for t in &self.transitions {
            if t.from == self.current_state
                && t.priority > best_priority
                && t.condition.evaluate(obs, self.ticks_in_state)
            {
                best_target = Some(t.to);
                best_priority = t.priority;
            }
        }

        if let Some(next_state) = best_target {
            // --- exit current state ---
            if let Some(exit_action) = self.exit_action_for(self.current_state) {
                actions.push(exit_action);
            }

            // Record history
            self.record_history(self.current_state, self.ticks_in_state);

            self.previous_state = Some(self.current_state);
            self.current_state = next_state;
            self.ticks_in_state = 0;

            // --- enter new state ---
            if let Some(enter_action) = self.enter_action_for(self.current_state) {
                actions.push(enter_action);
            }
        }

        // --- produce state action ---
        self.ticks_in_state += 1;

        if let Some(action) = self.state_action(obs) {
            actions.push(action);
        } else {
            actions.push(Action::Idle);
        }

        actions
    }

    /// The FSM's current state.
    pub fn current_state(&self) -> FsmState {
        self.current_state
    }

    /// Ticks spent in the current state so far.
    pub fn ticks_in_state(&self) -> u32 {
        self.ticks_in_state
    }

    /// The state the FSM was in before the current one (if any).
    pub fn previous_state(&self) -> Option<FsmState> {
        self.previous_state
    }

    /// Read the state history (most recent entry last).
    pub fn state_history(&self) -> &[(FsmState, u32)] {
        &self.state_history
    }

    // -- internal helpers --------------------------------------------------

    fn state_action(&self, obs: &Observation) -> Option<Action> {
        self.states
            .iter()
            .find(|(s, _)| *s == self.current_state)
            .map(|(_, cfg)| (cfg.action_fn)(obs))
    }

    fn enter_action_for(&self, state: FsmState) -> Option<Action> {
        self.states
            .iter()
            .find(|(s, _)| *s == state)
            .and_then(|(_, cfg)| cfg.on_enter.clone())
    }

    fn exit_action_for(&self, state: FsmState) -> Option<Action> {
        self.states
            .iter()
            .find(|(s, _)| *s == state)
            .and_then(|(_, cfg)| cfg.on_exit.clone())
    }

    fn record_history(&mut self, state: FsmState, ticks: u32) {
        self.state_history.push((state, ticks));
        if self.state_history.len() > self.max_history {
            self.state_history.remove(0);
        }
    }
}

// ===========================================================================
// Pre-built FSM templates
// ===========================================================================

/// Combat FSM: **Idle <-> Alert <-> Combat <-> Dead**
///
/// - **Idle** — no enemies visible, agent stands still.
/// - **Alert** — enemy detected within `detection_range` but outside
///   `combat_range`; agent looks toward the threat.
/// - **Combat** — enemy within `combat_range`; agent attacks.
/// - **Dead** — health is zero; agent does nothing.
///
/// Transitions:
/// ```text
///   *  -> Dead       (health == 0)           priority 100
///   Idle -> Alert    (hostile in detection)   priority 10
///   Alert -> Combat  (hostile in combat)      priority 20
///   Alert -> Idle    (no hostiles)            priority  5
///   Combat -> Alert  (no hostile in combat)   priority 15
///   Combat -> Idle   (no hostiles)            priority  5
/// ```
pub fn combat_fsm(detection_range: f32, combat_range: f32) -> SmartFsm {
    let mut fsm = SmartFsm::new(FsmState::Idle);

    // --- states ---

    fsm.add_state(FsmStateConfig {
        state: FsmState::Idle,
        action_fn: Box::new(|_obs| Action::Idle),
        on_enter: None,
        on_exit: None,
    });

    // Custom(0) = Alert
    fsm.add_state(FsmStateConfig {
        state: FsmState::Custom(0),
        action_fn: Box::new(|obs| {
            if let Some(hostile) = nearest_hostile(obs) {
                Action::LookAt {
                    target: hostile.position,
                }
            } else {
                Action::Idle
            }
        }),
        on_enter: None,
        on_exit: None,
    });

    // Attacking = Combat
    fsm.add_state(FsmStateConfig {
        state: FsmState::Attacking,
        action_fn: Box::new(|obs| {
            if let Some(hostile) = nearest_hostile(obs) {
                Action::AttackTarget {
                    target: hostile.entity_id,
                }
            } else {
                Action::Attack
            }
        }),
        on_enter: None,
        on_exit: None,
    });

    // Custom(255) = Dead
    fsm.add_state(FsmStateConfig {
        state: FsmState::Custom(255),
        action_fn: Box::new(|_obs| Action::Idle),
        on_enter: Some(Action::Stop),
        on_exit: None,
    });

    // --- transitions ---

    // Any state -> Dead (highest priority)
    for from in [
        FsmState::Idle,
        FsmState::Custom(0),
        FsmState::Attacking,
    ] {
        fsm.add_transition(from, FsmState::Custom(255), FsmCondition::Dead, 100);
    }

    // Idle -> Alert
    fsm.add_transition(
        FsmState::Idle,
        FsmState::Custom(0),
        FsmCondition::HostileInRange(detection_range),
        10,
    );

    // Alert -> Combat
    fsm.add_transition(
        FsmState::Custom(0),
        FsmState::Attacking,
        FsmCondition::HostileInRange(combat_range),
        20,
    );

    // Alert -> Idle (no hostiles)
    fsm.add_transition(
        FsmState::Custom(0),
        FsmState::Idle,
        FsmCondition::NoHostilesVisible,
        5,
    );

    // Combat -> Alert (hostile outside combat range but still visible)
    fsm.add_transition(
        FsmState::Attacking,
        FsmState::Custom(0),
        FsmCondition::And(
            Box::new(FsmCondition::Not(Box::new(FsmCondition::HostileInRange(
                combat_range,
            )))),
            Box::new(FsmCondition::HostileInRange(detection_range)),
        ),
        15,
    );

    // Combat -> Idle (no hostiles at all)
    fsm.add_transition(
        FsmState::Attacking,
        FsmState::Idle,
        FsmCondition::NoHostilesVisible,
        5,
    );

    fsm
}

/// Patrol FSM: **Patrolling <-> Chasing <-> Attacking <-> Fleeing**
///
/// - **Patrolling** — follow waypoints.
/// - **Chasing** — hostile detected, move toward it.
/// - **Attacking** — hostile in attack range, attack.
/// - **Fleeing** — health too low, run away.
pub fn patrol_fsm(
    waypoints: Vec<Vec2>,
    detection_range: f32,
    flee_threshold: f32,
) -> SmartFsm {
    let mut fsm = SmartFsm::new(FsmState::Patrolling);

    let attack_range = detection_range * 0.3; // 30 % of detection range

    // --- states ---

    let wp = waypoints.clone();
    fsm.add_state(FsmStateConfig {
        state: FsmState::Patrolling,
        action_fn: Box::new(move |obs| {
            if wp.is_empty() {
                return Action::Idle;
            }
            // Simple: pick waypoint based on tick modulo
            let idx = (obs.tick as usize) % wp.len();
            let target = wp[idx];
            let dir = (target - obs.self_state.position).normalize_or_zero();
            Action::Move { direction: dir }
        }),
        on_enter: None,
        on_exit: None,
    });

    fsm.add_state(FsmStateConfig {
        state: FsmState::Chasing,
        action_fn: Box::new(|obs| {
            if let Some(hostile) = nearest_hostile(obs) {
                let dir =
                    (hostile.position - obs.self_state.position).normalize_or_zero();
                Action::Move { direction: dir }
            } else {
                Action::Idle
            }
        }),
        on_enter: None,
        on_exit: None,
    });

    fsm.add_state(FsmStateConfig {
        state: FsmState::Attacking,
        action_fn: Box::new(|obs| {
            if let Some(hostile) = nearest_hostile(obs) {
                Action::AttackTarget {
                    target: hostile.entity_id,
                }
            } else {
                Action::Attack
            }
        }),
        on_enter: None,
        on_exit: None,
    });

    fsm.add_state(FsmStateConfig {
        state: FsmState::Fleeing,
        action_fn: Box::new(|obs| {
            if let Some(hostile) = nearest_hostile(obs) {
                let away =
                    (obs.self_state.position - hostile.position).normalize_or_zero();
                Action::Move { direction: away }
            } else {
                Action::Move {
                    direction: Vec2::new(0.0, 1.0),
                }
            }
        }),
        on_enter: None,
        on_exit: None,
    });

    // --- transitions ---

    // Any state -> Fleeing when health low (highest non-dead priority)
    for from in [
        FsmState::Patrolling,
        FsmState::Chasing,
        FsmState::Attacking,
    ] {
        fsm.add_transition(
            from,
            FsmState::Fleeing,
            FsmCondition::HealthBelow(flee_threshold),
            50,
        );
    }

    // Patrolling -> Chasing
    fsm.add_transition(
        FsmState::Patrolling,
        FsmState::Chasing,
        FsmCondition::HostileInRange(detection_range),
        10,
    );

    // Chasing -> Attacking
    fsm.add_transition(
        FsmState::Chasing,
        FsmState::Attacking,
        FsmCondition::HostileInRange(attack_range),
        20,
    );

    // Attacking -> Chasing (target fled out of attack range but still in detection)
    fsm.add_transition(
        FsmState::Attacking,
        FsmState::Chasing,
        FsmCondition::And(
            Box::new(FsmCondition::Not(Box::new(FsmCondition::HostileInRange(
                attack_range,
            )))),
            Box::new(FsmCondition::HostileInRange(detection_range)),
        ),
        15,
    );

    // Chasing -> Patrolling (lost sight)
    fsm.add_transition(
        FsmState::Chasing,
        FsmState::Patrolling,
        FsmCondition::NoHostilesVisible,
        5,
    );

    // Attacking -> Patrolling (lost sight)
    fsm.add_transition(
        FsmState::Attacking,
        FsmState::Patrolling,
        FsmCondition::NoHostilesVisible,
        5,
    );

    // Fleeing -> Patrolling (health recovered and no hostiles)
    fsm.add_transition(
        FsmState::Fleeing,
        FsmState::Patrolling,
        FsmCondition::And(
            Box::new(FsmCondition::HealthAbove(flee_threshold)),
            Box::new(FsmCondition::NoHostilesVisible),
        ),
        5,
    );

    fsm
}

/// Guard FSM: **Guarding <-> Alert <-> Combat**
///
/// - **Guarding** — stay at `position`, face outward.
/// - **Alert** — enemy within `guard_radius`, look toward it.
/// - **Combat** — enemy within attack range, attack.
pub fn guard_fsm(position: Vec2, guard_radius: f32) -> SmartFsm {
    let mut fsm = SmartFsm::new(FsmState::Guarding);

    let attack_range = guard_radius * 0.4; // 40 % of guard radius

    // --- states ---

    fsm.add_state(FsmStateConfig {
        state: FsmState::Guarding,
        action_fn: Box::new(move |_obs| Action::LookAt { target: position }),
        on_enter: Some(Action::Stop),
        on_exit: None,
    });

    // Custom(0) = Alert
    fsm.add_state(FsmStateConfig {
        state: FsmState::Custom(0),
        action_fn: Box::new(|obs| {
            if let Some(hostile) = nearest_hostile(obs) {
                Action::LookAt {
                    target: hostile.position,
                }
            } else {
                Action::Idle
            }
        }),
        on_enter: None,
        on_exit: None,
    });

    // Attacking = Combat
    fsm.add_state(FsmStateConfig {
        state: FsmState::Attacking,
        action_fn: Box::new(|obs| {
            if let Some(hostile) = nearest_hostile(obs) {
                Action::AttackTarget {
                    target: hostile.entity_id,
                }
            } else {
                Action::Attack
            }
        }),
        on_enter: None,
        on_exit: None,
    });

    // --- transitions ---

    // Guarding -> Alert
    fsm.add_transition(
        FsmState::Guarding,
        FsmState::Custom(0),
        FsmCondition::HostileInRange(guard_radius),
        10,
    );

    // Alert -> Combat
    fsm.add_transition(
        FsmState::Custom(0),
        FsmState::Attacking,
        FsmCondition::HostileInRange(attack_range),
        20,
    );

    // Alert -> Guarding (no hostiles)
    fsm.add_transition(
        FsmState::Custom(0),
        FsmState::Guarding,
        FsmCondition::NoHostilesVisible,
        5,
    );

    // Combat -> Alert (hostile outside attack range but in guard radius)
    fsm.add_transition(
        FsmState::Attacking,
        FsmState::Custom(0),
        FsmCondition::And(
            Box::new(FsmCondition::Not(Box::new(FsmCondition::HostileInRange(
                attack_range,
            )))),
            Box::new(FsmCondition::HostileInRange(guard_radius)),
        ),
        15,
    );

    // Combat -> Guarding (no hostiles)
    fsm.add_transition(
        FsmState::Attacking,
        FsmState::Guarding,
        FsmCondition::NoHostilesVisible,
        5,
    );

    fsm
}

// ---------------------------------------------------------------------------
// Shared observation utilities used by template closures
// ---------------------------------------------------------------------------

/// Find the nearest hostile entity in the observation.
fn nearest_hostile(obs: &Observation) -> Option<&pod_core::observation::VisibleEntity> {
    obs.visible_entities
        .iter()
        .filter(|e| is_hostile(&e.relationship))
        .min_by(|a, b| {
            a.distance
                .partial_cmp(&b.distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::component::Team;
    use pod_core::id::{AgentId, EntityId};
    use pod_core::observation::{SelfState, VisibleEntity};

    // -- test helpers -------------------------------------------------------

    fn make_self_state(health: f32, max_health: f32) -> SelfState {
        SelfState {
            agent_id: AgentId::new(),
            entity_id: EntityId(0),
            position: Vec2::ZERO,
            rotation: 0.0,
            velocity: Vec2::ZERO,
            health: Some(health),
            max_health: Some(max_health),
            team: Team::None,
            cooldowns: vec![],
        }
    }

    fn make_obs(health: f32, max_health: f32) -> Observation {
        Observation {
            tick: 0,
            elapsed_secs: 0.0,
            self_state: make_self_state(health, max_health),
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
            entity_type: "enemy".to_string(),
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
            entity_type: "ally".to_string(),
            position: Vec2::new(distance, 0.0),
            velocity: Vec2::ZERO,
            rotation: 0.0,
            distance,
            relationship: Relationship::Friendly,
            health_fraction: Some(1.0),
        }
    }

    fn obs_with_hostile(health: f32, max_health: f32, enemy_dist: f32) -> Observation {
        let mut obs = make_obs(health, max_health);
        obs.visible_entities.push(make_hostile(enemy_dist));
        obs
    }

    // -- FsmCondition tests -------------------------------------------------

    #[test]
    fn test_hostile_in_range_true() {
        let obs = obs_with_hostile(100.0, 100.0, 30.0);
        assert!(FsmCondition::HostileInRange(50.0).evaluate(&obs, 0));
    }

    #[test]
    fn test_hostile_in_range_false() {
        let obs = obs_with_hostile(100.0, 100.0, 80.0);
        assert!(!FsmCondition::HostileInRange(50.0).evaluate(&obs, 0));
    }

    #[test]
    fn test_hostile_in_range_ignores_friendly() {
        let mut obs = make_obs(100.0, 100.0);
        obs.visible_entities.push(make_friendly(10.0));
        assert!(!FsmCondition::HostileInRange(50.0).evaluate(&obs, 0));
    }

    #[test]
    fn test_health_below() {
        let obs = make_obs(20.0, 100.0);
        assert!(FsmCondition::HealthBelow(0.3).evaluate(&obs, 0));
        assert!(!FsmCondition::HealthBelow(0.1).evaluate(&obs, 0));
    }

    #[test]
    fn test_health_above() {
        let obs = make_obs(80.0, 100.0);
        assert!(FsmCondition::HealthAbove(0.5).evaluate(&obs, 0));
        assert!(!FsmCondition::HealthAbove(0.9).evaluate(&obs, 0));
    }

    #[test]
    fn test_condition_and() {
        let obs = obs_with_hostile(20.0, 100.0, 30.0);
        let cond = FsmCondition::And(
            Box::new(FsmCondition::HostileInRange(50.0)),
            Box::new(FsmCondition::HealthBelow(0.3)),
        );
        assert!(cond.evaluate(&obs, 0));

        // Fails when one side is false
        let cond2 = FsmCondition::And(
            Box::new(FsmCondition::HostileInRange(50.0)),
            Box::new(FsmCondition::HealthBelow(0.1)),
        );
        assert!(!cond2.evaluate(&obs, 0));
    }

    #[test]
    fn test_condition_or() {
        let obs = make_obs(20.0, 100.0); // no hostile, low health
        let cond = FsmCondition::Or(
            Box::new(FsmCondition::HostileInRange(50.0)), // false
            Box::new(FsmCondition::HealthBelow(0.3)),      // true
        );
        assert!(cond.evaluate(&obs, 0));

        // Both false
        let cond2 = FsmCondition::Or(
            Box::new(FsmCondition::HostileInRange(50.0)),
            Box::new(FsmCondition::HealthBelow(0.1)),
        );
        assert!(!cond2.evaluate(&obs, 0));
    }

    #[test]
    fn test_condition_not() {
        let obs = make_obs(100.0, 100.0);
        let cond = FsmCondition::Not(Box::new(FsmCondition::HostileInRange(50.0)));
        assert!(cond.evaluate(&obs, 0)); // no hostile → HostileInRange false → Not true
    }

    #[test]
    fn test_no_hostiles_visible() {
        let obs = make_obs(100.0, 100.0);
        assert!(FsmCondition::NoHostilesVisible.evaluate(&obs, 0));

        let obs2 = obs_with_hostile(100.0, 100.0, 10.0);
        assert!(!FsmCondition::NoHostilesVisible.evaluate(&obs2, 0));
    }

    #[test]
    fn test_ticks_in_state() {
        let obs = make_obs(100.0, 100.0);
        assert!(FsmCondition::TicksInState(5).evaluate(&obs, 10));
        assert!(!FsmCondition::TicksInState(5).evaluate(&obs, 3));
    }

    #[test]
    fn test_dead_condition() {
        let obs = make_obs(0.0, 100.0);
        assert!(FsmCondition::Dead.evaluate(&obs, 0));

        let obs2 = make_obs(1.0, 100.0);
        assert!(!FsmCondition::Dead.evaluate(&obs2, 0));
    }

    // -- SmartFsm tests -----------------------------------------------------

    #[test]
    fn test_smart_fsm_transitions_on_condition() {
        let mut fsm = SmartFsm::new(FsmState::Idle);
        fsm.add_state(FsmStateConfig {
            state: FsmState::Idle,
            action_fn: Box::new(|_| Action::Idle),
            on_enter: None,
            on_exit: None,
        });
        fsm.add_state(FsmStateConfig {
            state: FsmState::Attacking,
            action_fn: Box::new(|_| Action::Attack),
            on_enter: None,
            on_exit: None,
        });
        fsm.add_transition(
            FsmState::Idle,
            FsmState::Attacking,
            FsmCondition::HostileInRange(50.0),
            10,
        );

        let obs = obs_with_hostile(100.0, 100.0, 30.0);
        let actions = fsm.tick(&obs);

        assert_eq!(fsm.current_state(), FsmState::Attacking);
        // Should contain at least the Attack action from the new state
        assert!(actions.iter().any(|a| matches!(a, Action::Attack)));
    }

    #[test]
    fn test_smart_fsm_stays_when_no_condition_met() {
        let mut fsm = SmartFsm::new(FsmState::Idle);
        fsm.add_state(FsmStateConfig {
            state: FsmState::Idle,
            action_fn: Box::new(|_| Action::Idle),
            on_enter: None,
            on_exit: None,
        });
        fsm.add_transition(
            FsmState::Idle,
            FsmState::Attacking,
            FsmCondition::HostileInRange(50.0),
            10,
        );

        let obs = make_obs(100.0, 100.0); // no hostile
        fsm.tick(&obs);

        assert_eq!(fsm.current_state(), FsmState::Idle);
    }

    #[test]
    fn test_smart_fsm_priority_ordering() {
        let mut fsm = SmartFsm::new(FsmState::Idle);
        fsm.add_state(FsmStateConfig {
            state: FsmState::Idle,
            action_fn: Box::new(|_| Action::Idle),
            on_enter: None,
            on_exit: None,
        });
        fsm.add_state(FsmStateConfig {
            state: FsmState::Chasing,
            action_fn: Box::new(|_| Action::Idle),
            on_enter: None,
            on_exit: None,
        });
        fsm.add_state(FsmStateConfig {
            state: FsmState::Fleeing,
            action_fn: Box::new(|_| Action::Idle),
            on_enter: None,
            on_exit: None,
        });

        // Both conditions true, but Fleeing has higher priority
        fsm.add_transition(
            FsmState::Idle,
            FsmState::Chasing,
            FsmCondition::Always,
            10,
        );
        fsm.add_transition(
            FsmState::Idle,
            FsmState::Fleeing,
            FsmCondition::Always,
            50,
        );

        let obs = make_obs(100.0, 100.0);
        fsm.tick(&obs);

        assert_eq!(fsm.current_state(), FsmState::Fleeing);
    }

    #[test]
    fn test_smart_fsm_state_history() {
        let mut fsm = SmartFsm::new(FsmState::Idle);
        fsm.add_state(FsmStateConfig {
            state: FsmState::Idle,
            action_fn: Box::new(|_| Action::Idle),
            on_enter: None,
            on_exit: None,
        });
        fsm.add_state(FsmStateConfig {
            state: FsmState::Attacking,
            action_fn: Box::new(|_| Action::Attack),
            on_enter: None,
            on_exit: None,
        });
        fsm.add_transition(
            FsmState::Idle,
            FsmState::Attacking,
            FsmCondition::TicksInState(3),
            10,
        );

        let obs = make_obs(100.0, 100.0);

        // Tick 3 times without transitioning (ticks_in_state < 3)
        fsm.tick(&obs);
        fsm.tick(&obs);
        assert_eq!(fsm.current_state(), FsmState::Idle);

        // On the 3rd tick, ticks_in_state reaches 2 internally before
        // comparison; the 4th tick will trigger (ticks_in_state == 3)
        fsm.tick(&obs);
        fsm.tick(&obs);

        assert_eq!(fsm.current_state(), FsmState::Attacking);
        assert_eq!(fsm.previous_state(), Some(FsmState::Idle));
        assert!(!fsm.state_history().is_empty());
        assert_eq!(fsm.state_history().last().unwrap().0, FsmState::Idle);
    }

    #[test]
    fn test_smart_fsm_entry_exit_actions() {
        let mut fsm = SmartFsm::new(FsmState::Idle);
        fsm.add_state(FsmStateConfig {
            state: FsmState::Idle,
            action_fn: Box::new(|_| Action::Idle),
            on_enter: None,
            on_exit: Some(Action::Stop),
        });
        fsm.add_state(FsmStateConfig {
            state: FsmState::Attacking,
            action_fn: Box::new(|_| Action::Attack),
            on_enter: Some(Action::Rotate { angle: 0.0 }),
            on_exit: None,
        });
        fsm.add_transition(
            FsmState::Idle,
            FsmState::Attacking,
            FsmCondition::Always,
            10,
        );

        let obs = make_obs(100.0, 100.0);
        let actions = fsm.tick(&obs);

        // Should contain: exit(Stop), enter(Rotate), state_action(Attack)
        assert!(actions.iter().any(|a| matches!(a, Action::Stop)));
        assert!(actions.iter().any(|a| matches!(a, Action::Rotate { .. })));
        assert!(actions.iter().any(|a| matches!(a, Action::Attack)));
    }

    // -- Template tests -----------------------------------------------------

    #[test]
    fn test_combat_fsm_idle_to_alert() {
        let mut fsm = combat_fsm(100.0, 30.0);
        assert_eq!(fsm.current_state(), FsmState::Idle);

        // Enemy at distance 60 — within detection (100) but outside combat (30)
        let obs = obs_with_hostile(100.0, 100.0, 60.0);
        fsm.tick(&obs);

        // Should transition to Alert (Custom(0))
        assert_eq!(fsm.current_state(), FsmState::Custom(0));
    }

    #[test]
    fn test_combat_fsm_alert_to_combat() {
        let mut fsm = combat_fsm(100.0, 30.0);

        // First tick: Idle -> Alert (enemy at 60, within detection 100)
        let obs_far = obs_with_hostile(100.0, 100.0, 60.0);
        fsm.tick(&obs_far);
        assert_eq!(fsm.current_state(), FsmState::Custom(0)); // Alert

        // Second tick: Alert -> Combat (enemy at 20, within combat 30)
        let obs_close = obs_with_hostile(100.0, 100.0, 20.0);
        fsm.tick(&obs_close);
        assert_eq!(fsm.current_state(), FsmState::Attacking);
    }

    #[test]
    fn test_combat_fsm_dead_on_zero_health() {
        let mut fsm = combat_fsm(100.0, 30.0);

        let obs = make_obs(0.0, 100.0);
        fsm.tick(&obs);

        // Should transition to Dead (Custom(255))
        assert_eq!(fsm.current_state(), FsmState::Custom(255));
    }

    #[test]
    fn test_guard_fsm_basic_structure() {
        let fsm = guard_fsm(Vec2::new(100.0, 100.0), 80.0);

        // Starts in Guarding
        assert_eq!(fsm.current_state(), FsmState::Guarding);

        // Has states registered
        assert!(!fsm.states.is_empty());

        // Has transitions registered
        assert!(!fsm.transitions.is_empty());
    }

    #[test]
    fn test_guard_fsm_guarding_to_alert() {
        let mut fsm = guard_fsm(Vec2::new(100.0, 100.0), 80.0);

        // Enemy within guard radius (80) but outside attack range (32 = 80*0.4)
        let obs = obs_with_hostile(100.0, 100.0, 50.0);
        fsm.tick(&obs);

        assert_eq!(fsm.current_state(), FsmState::Custom(0)); // Alert
    }

    #[test]
    fn test_patrol_fsm_basic() {
        let waypoints = vec![Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0)];
        let mut fsm = patrol_fsm(waypoints, 100.0, 0.2);

        assert_eq!(fsm.current_state(), FsmState::Patrolling);

        // No enemies — stays patrolling
        let obs = make_obs(100.0, 100.0);
        fsm.tick(&obs);
        assert_eq!(fsm.current_state(), FsmState::Patrolling);

        // Enemy appears — chases
        let obs2 = obs_with_hostile(100.0, 100.0, 50.0);
        fsm.tick(&obs2);
        assert_eq!(fsm.current_state(), FsmState::Chasing);
    }

    #[test]
    fn test_patrol_fsm_flee_on_low_health() {
        let waypoints = vec![Vec2::new(10.0, 0.0)];
        let mut fsm = patrol_fsm(waypoints, 100.0, 0.3);

        // Enemy in range AND health below 30 %
        let obs = obs_with_hostile(15.0, 100.0, 50.0);
        fsm.tick(&obs);

        // Flee has priority 50, chase has priority 10 — should flee
        assert_eq!(fsm.current_state(), FsmState::Fleeing);
    }
}
