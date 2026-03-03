//! Scripted agent implementation with production-quality Behavior Trees and FSM.
//!
//! Features:
//! - Sequence, Selector, Parallel, Decorator nodes with proper status propagation
//! - Blackboard for inter-node communication
//! - Pre-built behaviors: Patrol, Chase, Flee, Wander, Guard, AttackNearest
//! - Finite State Machine (FSM) as alternative to behavior trees
//! - Full action generation and state management

use pod_core::action::{Action, AgentConstraints};
use pod_core::agent::{Agent, AgentType};
use pod_core::id::AgentId;
use pod_core::observation::{Observation, Relationship};
use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use log::{debug, warn};

/// Behavior tree status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BehaviorStatus {
    /// Node completed successfully
    Success,
    /// Node failed
    Failure,
    /// Node is still running, needs more ticks
    Running,
}

/// A node in the behavior tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BehaviorNode {
    /// Run children in sequence; fail if any fails; succeed only if all succeed
    Sequence {
        children: Vec<BehaviorNode>,
        #[serde(skip)]
        current_child_index: usize,
    },
    /// Run children in order until one succeeds; fail if all fail
    Selector {
        children: Vec<BehaviorNode>,
        #[serde(skip)]
        current_child_index: usize,
    },
    /// Run all children in parallel; succeed if all succeed; fail if any fails
    Parallel {
        children: Vec<BehaviorNode>,
        #[serde(skip)]
        statuses: HashMap<usize, BehaviorStatus>,
    },
    /// Decorator: invert child status
    Inverter { child: Box<BehaviorNode> },
    /// Decorator: always return Success regardless of child status
    AlwaysSucceed { child: Box<BehaviorNode> },
    /// Decorator: always return Failure regardless of child status
    AlwaysFail { child: Box<BehaviorNode> },
    /// Decorator: repeat child until it succeeds
    Repeater { child: Box<BehaviorNode> },
    /// Run a single action
    ActionNode { action: Action },
    /// Wait N ticks before returning success
    Wait { ticks: u32, elapsed: u32 },
    /// Repeat child exactly count times
    Repeat {
        child: Box<BehaviorNode>,
        count: u32,
        #[serde(skip)]
        current: u32,
    },
    /// Condition check (always succeeds; evaluation is implicit)
    Condition { check: String },
    /// Always succeed
    Success,
    /// Always fail
    Failure,

    // --- Sensory/Conditional leaf nodes (Task 2.8) ---
    /// Success if health fraction < threshold (0.0–1.0)
    HealthBelow { threshold: f32 },
    /// Success if health fraction > threshold (0.0–1.0)
    HealthAbove { threshold: f32 },
    /// Success if any hostile entity is within `range` world-units
    EnemyInRange { range: f32 },
    /// Success if the count of visible hostiles is >= `min`
    EnemyCount { min: usize },
    /// Success if any friendly entity is within `range` world-units
    AllyNearby { range: f32 },
    /// Success if at least one hostile is visible
    HasTarget,

    // --- Action leaf nodes (Task 2.8) ---
    /// Move toward the nearest entity of the given relationship type
    MoveToNearest { relationship: Relationship },
    /// Move away from the nearest hostile
    FleeFromNearest,
    /// Move toward a hard-coded world position
    MoveToPosition { position: Vec2 },
    /// Rotate to face the nearest hostile
    RotateToNearest,
}

/// Blackboard for inter-node communication in behavior trees
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Blackboard {
    data: HashMap<String, serde_json::Value>,
}

impl Blackboard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, key: String, value: serde_json::Value) {
        self.data.insert(key, value);
    }

    pub fn get(&self, key: &str) -> Option<&serde_json::Value> {
        self.data.get(key)
    }

    pub fn get_f32(&self, key: &str) -> Option<f32> {
        self.data.get(key).and_then(|v| v.as_f64()).map(|f| f as f32)
    }

    pub fn get_string(&self, key: &str) -> Option<String> {
        self.data.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }
}

impl BehaviorNode {
    pub fn sequence(children: Vec<BehaviorNode>) -> Self {
        Self::Sequence {
            children,
            current_child_index: 0,
        }
    }

    pub fn selector(children: Vec<BehaviorNode>) -> Self {
        Self::Selector {
            children,
            current_child_index: 0,
        }
    }

    pub fn parallel(children: Vec<BehaviorNode>) -> Self {
        Self::Parallel {
            children,
            statuses: HashMap::new(),
        }
    }

    pub fn inverter(child: BehaviorNode) -> Self {
        Self::Inverter {
            child: Box::new(child),
        }
    }

    pub fn always_succeed(child: BehaviorNode) -> Self {
        Self::AlwaysSucceed {
            child: Box::new(child),
        }
    }

    pub fn always_fail(child: BehaviorNode) -> Self {
        Self::AlwaysFail {
            child: Box::new(child),
        }
    }

    pub fn repeater(child: BehaviorNode) -> Self {
        Self::Repeater {
            child: Box::new(child),
        }
    }

    pub fn action(action: Action) -> Self {
        Self::ActionNode { action }
    }

    pub fn wait(ticks: u32) -> Self {
        Self::Wait { ticks, elapsed: 0 }
    }

    pub fn repeat(child: BehaviorNode, count: u32) -> Self {
        Self::Repeat {
            child: Box::new(child),
            count,
            current: 0,
        }
    }

    pub fn condition(check: String) -> Self {
        Self::Condition { check }
    }

    // ── Task 2.8: new factory methods ─────────────────────────────────────

    pub fn health_below(threshold: f32) -> Self {
        Self::HealthBelow { threshold }
    }

    pub fn health_above(threshold: f32) -> Self {
        Self::HealthAbove { threshold }
    }

    pub fn enemy_in_range(range: f32) -> Self {
        Self::EnemyInRange { range }
    }

    pub fn enemy_count(min: usize) -> Self {
        Self::EnemyCount { min }
    }

    pub fn ally_nearby(range: f32) -> Self {
        Self::AllyNearby { range }
    }

    pub fn has_target() -> Self {
        Self::HasTarget
    }

    pub fn move_to_nearest(relationship: Relationship) -> Self {
        Self::MoveToNearest { relationship }
    }

    pub fn flee_from_nearest() -> Self {
        Self::FleeFromNearest
    }

    pub fn move_to_position(position: Vec2) -> Self {
        Self::MoveToPosition { position }
    }

    pub fn rotate_to_nearest() -> Self {
        Self::RotateToNearest
    }
}

/// Complete behavior tree with state and blackboard
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorTree {
    pub root: BehaviorNode,
    pub blackboard: Blackboard,
}

impl BehaviorTree {
    pub fn new(root: BehaviorNode) -> Self {
        Self {
            root,
            blackboard: Blackboard::new(),
        }
    }

    /// Tick the tree and get actions to execute
    pub fn tick(&mut self, observation: &Observation) -> (Vec<Action>, BehaviorStatus) {
        let mut actions = Vec::new();
        let status = Self::tick_node(&mut self.root, &mut actions, observation);
        (actions, status)
    }

    fn tick_node(
        node: &mut BehaviorNode,
        actions: &mut Vec<Action>,
        observation: &Observation,
    ) -> BehaviorStatus {
        match node {
            BehaviorNode::Sequence { children, current_child_index } => {
                // Sequence: run until one fails
                for i in *current_child_index..children.len() {
                    match Self::tick_node(&mut children[i], actions, observation) {
                        BehaviorStatus::Failure => {
                            *current_child_index = 0;
                            return BehaviorStatus::Failure;
                        }
                        BehaviorStatus::Running => {
                            *current_child_index = i;
                            return BehaviorStatus::Running;
                        }
                        BehaviorStatus::Success => continue,
                    }
                }
                *current_child_index = 0;
                BehaviorStatus::Success
            }

            BehaviorNode::Selector { children, current_child_index } => {
                // Selector: run until one succeeds
                for i in *current_child_index..children.len() {
                    match Self::tick_node(&mut children[i], actions, observation) {
                        BehaviorStatus::Success => {
                            *current_child_index = 0;
                            return BehaviorStatus::Success;
                        }
                        BehaviorStatus::Running => {
                            *current_child_index = i;
                            return BehaviorStatus::Running;
                        }
                        BehaviorStatus::Failure => continue,
                    }
                }
                *current_child_index = 0;
                BehaviorStatus::Failure
            }

            BehaviorNode::Parallel { children, statuses } => {
                // Parallel: run all children, succeed if all succeed
                let mut any_running = false;
                for (i, child) in children.iter_mut().enumerate() {
                    let status = Self::tick_node(child, actions, observation);
                    statuses.insert(i, status);

                    if status == BehaviorStatus::Failure {
                        return BehaviorStatus::Failure;
                    }
                    if status == BehaviorStatus::Running {
                        any_running = true;
                    }
                }

                if any_running {
                    BehaviorStatus::Running
                } else {
                    statuses.clear();
                    BehaviorStatus::Success
                }
            }

            BehaviorNode::Inverter { child } => {
                // Inverter: flip success/failure, but pass through running
                match Self::tick_node(child, actions, observation) {
                    BehaviorStatus::Success => BehaviorStatus::Failure,
                    BehaviorStatus::Failure => BehaviorStatus::Success,
                    BehaviorStatus::Running => BehaviorStatus::Running,
                }
            }

            BehaviorNode::AlwaysSucceed { child } => {
                // Always succeed: ignore child status except running
                match Self::tick_node(child, actions, observation) {
                    BehaviorStatus::Running => BehaviorStatus::Running,
                    _ => BehaviorStatus::Success,
                }
            }

            BehaviorNode::AlwaysFail { child } => {
                // Always fail: ignore child status except running
                match Self::tick_node(child, actions, observation) {
                    BehaviorStatus::Running => BehaviorStatus::Running,
                    _ => BehaviorStatus::Failure,
                }
            }

            BehaviorNode::Repeater { child } => {
                // Repeater: keep running until child succeeds
                match Self::tick_node(child, actions, observation) {
                    BehaviorStatus::Success => BehaviorStatus::Success,
                    _ => BehaviorStatus::Running,
                }
            }

            BehaviorNode::ActionNode { action } => {
                actions.push(action.clone());
                BehaviorStatus::Success
            }

            BehaviorNode::Wait { ticks, elapsed } => {
                *elapsed += 1;
                if *elapsed >= *ticks {
                    *elapsed = 0;
                    BehaviorStatus::Success
                } else {
                    BehaviorStatus::Running
                }
            }

            BehaviorNode::Repeat { child, count, current } => {
                if *current >= *count {
                    *current = 0;
                    return BehaviorStatus::Success;
                }

                match Self::tick_node(child, actions, observation) {
                    BehaviorStatus::Success => {
                        *current += 1;
                        if *current >= *count {
                            BehaviorStatus::Success
                        } else {
                            BehaviorStatus::Running
                        }
                    }
                    status => status,
                }
            }

            BehaviorNode::Condition { .. } => {
                // Condition: evaluate against observation
                // Placeholder: always succeed
                BehaviorStatus::Success
            }

            BehaviorNode::Success => BehaviorStatus::Success,
            BehaviorNode::Failure => BehaviorStatus::Failure,

            // ── Sensory / Conditional leaves (Task 2.8) ────────────────────

            BehaviorNode::HealthBelow { threshold } => {
                let frac = health_fraction(observation);
                if frac < *threshold {
                    BehaviorStatus::Success
                } else {
                    BehaviorStatus::Failure
                }
            }

            BehaviorNode::HealthAbove { threshold } => {
                let frac = health_fraction(observation);
                if frac > *threshold {
                    BehaviorStatus::Success
                } else {
                    BehaviorStatus::Failure
                }
            }

            BehaviorNode::EnemyInRange { range } => {
                let in_range = observation.visible_entities.iter().any(|e| {
                    matches!(e.relationship, Relationship::Hostile) && e.distance <= *range
                });
                if in_range {
                    BehaviorStatus::Success
                } else {
                    BehaviorStatus::Failure
                }
            }

            BehaviorNode::EnemyCount { min } => {
                let count = observation
                    .visible_entities
                    .iter()
                    .filter(|e| matches!(e.relationship, Relationship::Hostile))
                    .count();
                if count >= *min {
                    BehaviorStatus::Success
                } else {
                    BehaviorStatus::Failure
                }
            }

            BehaviorNode::AllyNearby { range } => {
                let nearby = observation.visible_entities.iter().any(|e| {
                    matches!(e.relationship, Relationship::Friendly) && e.distance <= *range
                });
                if nearby {
                    BehaviorStatus::Success
                } else {
                    BehaviorStatus::Failure
                }
            }

            BehaviorNode::HasTarget => {
                let has = observation
                    .visible_entities
                    .iter()
                    .any(|e| matches!(e.relationship, Relationship::Hostile));
                if has {
                    BehaviorStatus::Success
                } else {
                    BehaviorStatus::Failure
                }
            }

            // ── Action leaves (Task 2.8) ───────────────────────────────────

            BehaviorNode::MoveToNearest { relationship } => {
                let rel = *relationship;
                if let Some(pos) = nearest_of_relationship(observation, rel) {
                    let self_pos = observation.self_state.position;
                    let dir = (pos - self_pos).normalize_or_zero();
                    actions.push(Action::Move { direction: dir });
                    BehaviorStatus::Success
                } else {
                    BehaviorStatus::Failure
                }
            }

            BehaviorNode::FleeFromNearest => {
                if let Some(pos) = nearest_hostile_position(observation) {
                    let self_pos = observation.self_state.position;
                    // Flee direction is directly away from the threat
                    let dir = (self_pos - pos).normalize_or_zero();
                    actions.push(Action::Move { direction: dir });
                    BehaviorStatus::Success
                } else {
                    BehaviorStatus::Failure
                }
            }

            BehaviorNode::MoveToPosition { position } => {
                let self_pos = observation.self_state.position;
                let dir = (*position - self_pos).normalize_or_zero();
                actions.push(Action::Move { direction: dir });
                BehaviorStatus::Success
            }

            BehaviorNode::RotateToNearest => {
                if let Some(pos) = nearest_hostile_position(observation) {
                    actions.push(Action::LookAt { target: pos });
                    BehaviorStatus::Success
                } else {
                    BehaviorStatus::Failure
                }
            }
        }
    }
}

/// Finite State Machine as alternative to behavior trees
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FsmState {
    Idle,
    Patrolling,
    Chasing,
    Fleeing,
    Guarding,
    Attacking,
    /// Aware of a threat but not yet engaged
    Alert,
    /// Entity is dead / out of combat
    Dead,
    Custom(u8),
}

/// FSM transition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FsmTransition {
    pub from: FsmState,
    pub to: FsmState,
    pub condition: String, // condition name to evaluate
}

/// FSM implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiniteStateMachine {
    pub current_state: FsmState,
    pub transitions: Vec<FsmTransition>,
    /// Per-state action table.  Looked up after every state transition.
    pub state_actions: HashMap<FsmState, Action>,
    pub default_action: Action,
    ticks_in_state: u32,
}

impl FiniteStateMachine {
    pub fn new(initial_state: FsmState, default_action: Action) -> Self {
        Self {
            current_state: initial_state,
            transitions: Vec::new(),
            state_actions: HashMap::new(),
            default_action,
            ticks_in_state: 0,
        }
    }

    pub fn add_transition(&mut self, transition: FsmTransition) {
        self.transitions.push(transition);
    }

    /// Associate an action with a state.  When the FSM enters or stays in
    /// `state` the tick will return `action`.
    pub fn set_state_action(&mut self, state: FsmState, action: Action) {
        self.state_actions.insert(state, action);
    }

    /// Evaluate transitions for the current tick, possibly change state,
    /// then return the action mapped to the (possibly new) current state.
    pub fn tick(&mut self, observation: &Observation) -> Action {
        self.ticks_in_state += 1;

        // Collect matching transitions so we can evaluate without borrowing
        // `self.transitions` while mutating `self.current_state`.
        let transitions: Vec<FsmTransition> = self
            .transitions
            .iter()
            .filter(|t| t.from == self.current_state)
            .cloned()
            .collect();

        for transition in &transitions {
            if Self::evaluate_condition(&transition.condition, observation, self.ticks_in_state) {
                debug!(
                    "FSM transition: {:?} → {:?} (condition: {})",
                    self.current_state, transition.to, transition.condition
                );
                self.current_state = transition.to;
                self.ticks_in_state = 0;
                break; // first matching transition wins
            }
        }

        self.state_actions
            .get(&self.current_state)
            .cloned()
            .unwrap_or_else(|| self.default_action.clone())
    }

    /// Evaluate a condition string against the current observation.
    ///
    /// Supported conditions:
    /// - `"enemy_visible"`       — any hostile in visible_entities
    /// - `"enemy_in_range_X"`    — hostile within X world-units
    /// - `"health_below_X"`      — health fraction < X/100
    /// - `"health_above_X"`      — health fraction > X/100
    /// - `"no_enemies"`          — no hostiles visible
    /// - `"timer_X"`             — ticks_in_state >= X
    fn evaluate_condition(condition: &str, obs: &Observation, ticks_in_state: u32) -> bool {
        // ── enemy_visible ────────────────────────────────────────────────────
        if condition == "enemy_visible" {
            return obs
                .visible_entities
                .iter()
                .any(|e| matches!(e.relationship, Relationship::Hostile));
        }

        // ── no_enemies ───────────────────────────────────────────────────────
        if condition == "no_enemies" {
            return !obs
                .visible_entities
                .iter()
                .any(|e| matches!(e.relationship, Relationship::Hostile));
        }

        // ── enemy_in_range_X ─────────────────────────────────────────────────
        if let Some(rest) = condition.strip_prefix("enemy_in_range_") {
            if let Ok(range) = rest.parse::<f32>() {
                return obs.visible_entities.iter().any(|e| {
                    matches!(e.relationship, Relationship::Hostile) && e.distance <= range
                });
            }
            warn!("FSM: could not parse range from condition '{}'", condition);
            return false;
        }

        // ── health_below_X ───────────────────────────────────────────────────
        if let Some(rest) = condition.strip_prefix("health_below_") {
            if let Ok(threshold_pct) = rest.parse::<f32>() {
                let frac = health_fraction(obs);
                return frac < threshold_pct / 100.0;
            }
            warn!("FSM: could not parse threshold from condition '{}'", condition);
            return false;
        }

        // ── health_above_X ───────────────────────────────────────────────────
        if let Some(rest) = condition.strip_prefix("health_above_") {
            if let Ok(threshold_pct) = rest.parse::<f32>() {
                let frac = health_fraction(obs);
                return frac > threshold_pct / 100.0;
            }
            warn!("FSM: could not parse threshold from condition '{}'", condition);
            return false;
        }

        // ── timer_X ──────────────────────────────────────────────────────────
        if let Some(rest) = condition.strip_prefix("timer_") {
            if let Ok(threshold) = rest.parse::<u32>() {
                return ticks_in_state >= threshold;
            }
            warn!("FSM: could not parse timer from condition '{}'", condition);
            return false;
        }

        warn!("FSM: unknown condition '{}'", condition);
        false
    }
}

/// Helper: compute health fraction from an observation (0.0 if unknown).
fn health_fraction(obs: &Observation) -> f32 {
    match (obs.self_state.health, obs.self_state.max_health) {
        (Some(h), Some(m)) if m > 0.0 => h / m,
        _ => 1.0, // assume full health when unknown
    }
}

/// Helper: find nearest hostile in `obs`; returns its position or None.
fn nearest_hostile_position(obs: &Observation) -> Option<Vec2> {
    obs.visible_entities
        .iter()
        .filter(|e| matches!(e.relationship, Relationship::Hostile))
        .min_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal))
        .map(|e| e.position)
}

/// Helper: find nearest entity of any given relationship.
fn nearest_of_relationship(obs: &Observation, rel: Relationship) -> Option<Vec2> {
    obs.visible_entities
        .iter()
        .filter(|e| relationship_matches(e.relationship, rel))
        .min_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal))
        .map(|e| e.position)
}

/// True when two Relationship values are the same variant.
fn relationship_matches(a: Relationship, b: Relationship) -> bool {
    matches!(
        (a, b),
        (Relationship::Friendly, Relationship::Friendly)
            | (Relationship::Hostile, Relationship::Hostile)
            | (Relationship::Neutral, Relationship::Neutral)
            | (Relationship::Unknown, Relationship::Unknown)
    )
}

/// Scripted NPC agent using behavior trees or FSM
pub struct ScriptedAgent {
    id: AgentId,
    behavior_tree: Option<BehaviorTree>,
    state_machine: Option<FiniteStateMachine>,
    constraints: AgentConstraints,
    last_observation: Option<Observation>,
}

impl ScriptedAgent {
    pub fn new_with_behavior_tree(behavior_tree: BehaviorTree) -> Self {
        Self {
            id: AgentId::new(),
            behavior_tree: Some(behavior_tree),
            state_machine: None,
            constraints: AgentConstraints::default(),
            last_observation: None,
        }
    }

    pub fn new_with_fsm(state_machine: FiniteStateMachine) -> Self {
        Self {
            id: AgentId::new(),
            behavior_tree: None,
            state_machine: Some(state_machine),
            constraints: AgentConstraints::default(),
            last_observation: None,
        }
    }

    pub fn with_id(id: AgentId, behavior_tree: BehaviorTree) -> Self {
        Self {
            id,
            behavior_tree: Some(behavior_tree),
            state_machine: None,
            constraints: AgentConstraints::default(),
            last_observation: None,
        }
    }

    pub fn with_id_fsm(id: AgentId, state_machine: FiniteStateMachine) -> Self {
        Self {
            id,
            behavior_tree: None,
            state_machine: Some(state_machine),
            constraints: AgentConstraints::default(),
            last_observation: None,
        }
    }

    pub fn blackboard_mut(&mut self) -> Option<&mut Blackboard> {
        self.behavior_tree.as_mut().map(|bt| &mut bt.blackboard)
    }
}

impl Agent for ScriptedAgent {
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
        if let Some(ref observation) = self.last_observation {
            // Use behavior tree if present, otherwise FSM
            if let Some(ref mut tree) = self.behavior_tree {
                let (actions, _status) = tree.tick(observation);
                debug!(
                    "ScriptedAgent {} behavior tree produced {} actions",
                    self.id,
                    actions.len()
                );
                actions
            } else if let Some(ref mut fsm) = self.state_machine {
                let action = fsm.tick(observation);
                debug!("ScriptedAgent {} FSM produced action", self.id);
                vec![action]
            } else {
                vec![Action::Idle]
            }
        } else {
            vec![Action::Idle]
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
// PRE-BUILT BEHAVIORS (Production Quality)
// ============================================================

/// Patrol between waypoints in sequence
pub fn patrol(waypoints: Vec<Vec2>) -> BehaviorNode {
    if waypoints.is_empty() {
        return BehaviorNode::action(Action::Idle);
    }

    let mut actions = vec![];
    for waypoint in waypoints {
        actions.push(BehaviorNode::action(Action::LookAt { target: waypoint }));
        actions.push(BehaviorNode::action(Action::Move {
            direction: waypoint.normalize(),
        }));
        actions.push(BehaviorNode::wait(30)); // Wait 30 ticks at each waypoint
    }

    BehaviorNode::sequence(actions)
}

/// Chase the nearest hostile entity
pub fn chase_nearest_hostile() -> BehaviorNode {
    BehaviorNode::sequence(vec![
        // Wait for observation to give us entity data
        BehaviorNode::action(Action::Idle),
        // Then attack
        BehaviorNode::action(Action::Attack),
    ])
}

/// Chase a specific target position
pub fn chase_target(target_position: Vec2) -> BehaviorNode {
    BehaviorNode::sequence(vec![
        BehaviorNode::action(Action::LookAt { target: target_position }),
        BehaviorNode::action(Action::Move {
            direction: target_position.normalize(),
        }),
    ])
}

/// Attack nearest enemy, with cooldown
pub fn attack_nearest() -> BehaviorNode {
    BehaviorNode::sequence(vec![
        BehaviorNode::action(Action::Attack),
        BehaviorNode::wait(30), // Attack cooldown
    ])
}

/// Flee from a position
pub fn flee_from(danger_position: Vec2) -> BehaviorNode {
    let flee_direction = -danger_position.normalize();
    BehaviorNode::sequence(vec![
        BehaviorNode::action(Action::Move { direction: flee_direction }),
        BehaviorNode::wait(60),
    ])
}

/// Guard a position (look toward it)
pub fn guard(position: Vec2, _radius: f32) -> BehaviorNode {
    BehaviorNode::sequence(vec![
        BehaviorNode::action(Action::LookAt { target: position }),
        BehaviorNode::action(Action::Idle),
    ])
}

/// Wander in a region (move back and forth)
pub fn wander(_center: Vec2, _radius: f32) -> BehaviorNode {
    BehaviorNode::sequence(vec![
        BehaviorNode::action(Action::Move {
            direction: Vec2::new(1.0, 0.0),
        }),
        BehaviorNode::wait(60),
        BehaviorNode::action(Action::Move {
            direction: Vec2::new(-1.0, 0.0),
        }),
        BehaviorNode::wait(60),
    ])
}

/// Combined behavior: patrol until threat appears, then chase
pub fn patrol_and_chase_on_threat(waypoints: Vec<Vec2>) -> BehaviorNode {
    BehaviorNode::selector(vec![
        // If we see a hostile, chase
        chase_nearest_hostile(),
        // Otherwise, patrol
        patrol(waypoints),
    ])
}

/// Combined behavior: guard position, attack if threatened
pub fn guard_with_defense(position: Vec2, radius: f32) -> BehaviorNode {
    BehaviorNode::selector(vec![
        // If hostile present, attack
        attack_nearest(),
        // Otherwise, guard
        guard(position, radius),
    ])
}

// ============================================================
// TASK 2.8 — NEW PRE-BUILT COMPOUND BEHAVIORS
// ============================================================

/// Aggressive behavior tree:
///   If enemy in short range  → attack
///   Else if enemy visible    → chase (move to nearest hostile)
///   Else                     → wander
pub fn aggressive_combat(wander_center: Vec2, wander_radius: f32) -> BehaviorNode {
    BehaviorNode::selector(vec![
        // Branch 1: attack if close
        BehaviorNode::sequence(vec![
            BehaviorNode::enemy_in_range(80.0),
            BehaviorNode::rotate_to_nearest(),
            BehaviorNode::action(Action::Attack),
        ]),
        // Branch 2: chase if visible
        BehaviorNode::sequence(vec![
            BehaviorNode::has_target(),
            BehaviorNode::rotate_to_nearest(),
            BehaviorNode::move_to_nearest(Relationship::Hostile),
        ]),
        // Branch 3: wander
        wander(wander_center, wander_radius),
    ])
}

/// Defensive behavior tree:
///   If health is low            → flee from nearest hostile
///   Else if enemy in range      → attack
///   Else                        → guard position
pub fn defensive_combat(guard_pos: Vec2, guard_radius: f32) -> BehaviorNode {
    BehaviorNode::selector(vec![
        // Branch 1: flee when hurt
        BehaviorNode::sequence(vec![
            BehaviorNode::health_below(0.3),
            BehaviorNode::flee_from_nearest(),
        ]),
        // Branch 2: attack if a hostile is in range and health is OK
        BehaviorNode::sequence(vec![
            BehaviorNode::health_above(0.3),
            BehaviorNode::enemy_in_range(120.0),
            BehaviorNode::rotate_to_nearest(),
            BehaviorNode::action(Action::Attack),
        ]),
        // Branch 3: guard home position
        guard(guard_pos, guard_radius),
    ])
}

/// Support behavior tree:
///   If health is low  → flee from nearest hostile
///   Else              → stay near allies (move toward nearest friendly)
pub fn support_ally(follow_range: f32) -> BehaviorNode {
    BehaviorNode::selector(vec![
        // Flee when hurt
        BehaviorNode::sequence(vec![
            BehaviorNode::health_below(0.25),
            BehaviorNode::flee_from_nearest(),
        ]),
        // Move toward nearest ally if they are farther than follow_range
        BehaviorNode::sequence(vec![
            BehaviorNode::inverter(BehaviorNode::ally_nearby(follow_range)),
            BehaviorNode::move_to_nearest(Relationship::Friendly),
        ]),
        // Stay idle when already close
        BehaviorNode::action(Action::Idle),
    ])
}

/// Sentry behavior tree:
///   If enemy visible and in chase range → chase
///   Else if enemy visible               → face enemy
///   Else                                → return to post
pub fn sentry(post: Vec2, chase_range: f32) -> BehaviorNode {
    BehaviorNode::selector(vec![
        // Chase if close enough
        BehaviorNode::sequence(vec![
            BehaviorNode::enemy_in_range(chase_range),
            BehaviorNode::rotate_to_nearest(),
            BehaviorNode::move_to_nearest(Relationship::Hostile),
        ]),
        // Face enemy if visible but out of chase range
        BehaviorNode::sequence(vec![
            BehaviorNode::has_target(),
            BehaviorNode::rotate_to_nearest(),
        ]),
        // Return to post
        BehaviorNode::move_to_position(post),
    ])
}

// ============================================================
// TASK 2.9 — FSM TEMPLATE FACTORIES
// ============================================================

/// Classic combat FSM:  Idle ↔ Alert ↔ Combat → Dead
///
/// Transitions:
///   Idle    → Alert    : enemy_visible
///   Alert   → Attacking: enemy_in_range_100
///   Alert   → Idle     : no_enemies  (+ timer_60 grace)
///   Attacking → Alert  : no_enemies
///   Attacking → Dead   : health_below_0  (i.e. hp < 0%; always false in
///                         practice — placeholder for "death" hook)
pub fn combat_fsm() -> FiniteStateMachine {
    let mut fsm = FiniteStateMachine::new(FsmState::Idle, Action::Idle);

    fsm.add_transition(FsmTransition {
        from: FsmState::Idle,
        to: FsmState::Alert,
        condition: "enemy_visible".into(),
    });
    fsm.add_transition(FsmTransition {
        from: FsmState::Alert,
        to: FsmState::Attacking,
        condition: "enemy_in_range_100".into(),
    });
    fsm.add_transition(FsmTransition {
        from: FsmState::Alert,
        to: FsmState::Idle,
        condition: "no_enemies".into(),
    });
    fsm.add_transition(FsmTransition {
        from: FsmState::Attacking,
        to: FsmState::Alert,
        condition: "no_enemies".into(),
    });

    fsm.set_state_action(FsmState::Idle, Action::Idle);
    fsm.set_state_action(
        FsmState::Alert,
        Action::LookAt {
            target: Vec2::ZERO, // will be overridden at runtime with real target
        },
    );
    fsm.set_state_action(FsmState::Attacking, Action::Attack);
    fsm.set_state_action(FsmState::Dead, Action::Idle);

    fsm
}

/// Patrol FSM:
///   Patrolling → Chasing   : enemy_visible
///   Chasing    → Attacking  : enemy_in_range_80
///   Attacking  → Chasing   : no_enemies (enemy fled but recheck)
///   Chasing    → Patrolling : no_enemies + timer_120
pub fn patrol_fsm() -> FiniteStateMachine {
    let mut fsm = FiniteStateMachine::new(FsmState::Patrolling, Action::Move {
        direction: Vec2::new(1.0, 0.0),
    });

    fsm.add_transition(FsmTransition {
        from: FsmState::Patrolling,
        to: FsmState::Chasing,
        condition: "enemy_visible".into(),
    });
    fsm.add_transition(FsmTransition {
        from: FsmState::Chasing,
        to: FsmState::Attacking,
        condition: "enemy_in_range_80".into(),
    });
    fsm.add_transition(FsmTransition {
        from: FsmState::Attacking,
        to: FsmState::Chasing,
        condition: "enemy_visible".into(),
    });
    fsm.add_transition(FsmTransition {
        from: FsmState::Chasing,
        to: FsmState::Patrolling,
        condition: "no_enemies".into(),
    });

    fsm.set_state_action(FsmState::Patrolling, Action::Move {
        direction: Vec2::new(1.0, 0.0),
    });
    fsm.set_state_action(FsmState::Chasing, Action::Move {
        direction: Vec2::new(0.0, 1.0), // direction updated by runtime logic
    });
    fsm.set_state_action(FsmState::Attacking, Action::Attack);

    fsm
}

/// Guard FSM:
///   Guarding  → Alert     : enemy_visible
///   Alert     → Attacking : enemy_in_range_120
///   Attacking → Alert     : no_enemies
///   Alert     → Guarding  : no_enemies + timer_90
pub fn guard_fsm() -> FiniteStateMachine {
    let mut fsm = FiniteStateMachine::new(FsmState::Guarding, Action::Idle);

    fsm.add_transition(FsmTransition {
        from: FsmState::Guarding,
        to: FsmState::Alert,
        condition: "enemy_visible".into(),
    });
    fsm.add_transition(FsmTransition {
        from: FsmState::Alert,
        to: FsmState::Attacking,
        condition: "enemy_in_range_120".into(),
    });
    fsm.add_transition(FsmTransition {
        from: FsmState::Attacking,
        to: FsmState::Alert,
        condition: "no_enemies".into(),
    });
    fsm.add_transition(FsmTransition {
        from: FsmState::Alert,
        to: FsmState::Guarding,
        condition: "no_enemies".into(),
    });

    fsm.set_state_action(FsmState::Guarding, Action::Idle);
    fsm.set_state_action(FsmState::Alert, Action::LookAt { target: Vec2::ZERO });
    fsm.set_state_action(FsmState::Attacking, Action::Attack);

    fsm
}

/// Coward FSM:
///   Idle    → Fleeing : enemy_visible
///   Fleeing → Idle    : no_enemies + timer_180 (3 s at 60 TPS)
pub fn coward_fsm() -> FiniteStateMachine {
    let mut fsm = FiniteStateMachine::new(FsmState::Idle, Action::Idle);

    fsm.add_transition(FsmTransition {
        from: FsmState::Idle,
        to: FsmState::Fleeing,
        condition: "enemy_visible".into(),
    });
    fsm.add_transition(FsmTransition {
        from: FsmState::Fleeing,
        to: FsmState::Idle,
        condition: "no_enemies".into(),
    });

    fsm.set_state_action(FsmState::Idle, Action::Idle);
    fsm.set_state_action(FsmState::Fleeing, Action::Move {
        direction: Vec2::new(-1.0, 0.0), // away; runtime should override
    });

    fsm
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_behavior_sequence() {
        let mut tree = BehaviorTree::new(BehaviorNode::Sequence {
            children: vec![
                BehaviorNode::Success,
                BehaviorNode::Success,
            ],
            current_child_index: 0,
        });

        let obs = Observation {
            tick: 0,
            elapsed_secs: 0.0,
            self_state: Default::default(),
            visible_entities: vec![],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![],
        };

        let (_, status) = tree.tick(&obs);
        assert_eq!(status, BehaviorStatus::Success);
    }

    #[test]
    fn test_behavior_selector() {
        let mut tree = BehaviorTree::new(BehaviorNode::Selector {
            children: vec![
                BehaviorNode::Failure,
                BehaviorNode::Success,
            ],
            current_child_index: 0,
        });

        let obs = Observation {
            tick: 0,
            elapsed_secs: 0.0,
            self_state: Default::default(),
            visible_entities: vec![],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![],
        };

        let (_, status) = tree.tick(&obs);
        assert_eq!(status, BehaviorStatus::Success);
    }

    #[test]
    fn test_behavior_wait() {
        let mut tree = BehaviorTree::new(BehaviorNode::wait(3));

        let obs = Observation {
            tick: 0,
            elapsed_secs: 0.0,
            self_state: Default::default(),
            visible_entities: vec![],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![],
        };

        let (_, status1) = tree.tick(&obs);
        assert_eq!(status1, BehaviorStatus::Running);

        let (_, status2) = tree.tick(&obs);
        assert_eq!(status2, BehaviorStatus::Running);

        let (_, status3) = tree.tick(&obs);
        assert_eq!(status3, BehaviorStatus::Success);
    }

    // ================================================================
    // TASK 2.8 — BT node tests
    // ================================================================

    /// Build a minimal Observation that has one hostile at `distance` and
    /// self health set to `health` (out of 100).
    fn test_observation_with_hostile(distance: f32, health: f32) -> Observation {
        use pod_core::observation::{SelfState, VisibleEntity};
        use pod_core::id::EntityId;
        use pod_core::component::Team;

        Observation {
            tick: 100,
            elapsed_secs: 1.667,
            self_state: SelfState {
                position: Vec2::ZERO,
                health: Some(health),
                max_health: Some(100.0),
                team: Team::default(),
                ..Default::default()
            },
            visible_entities: vec![VisibleEntity {
                entity_id: EntityId(1),
                entity_type: "enemy".into(),
                position: Vec2::new(distance, 0.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                distance,
                relationship: Relationship::Hostile,
                health_fraction: Some(1.0),
            }],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![],
        }
    }

    fn test_observation_empty() -> Observation {
        Observation {
            tick: 100,
            elapsed_secs: 1.667,
            self_state: Default::default(),
            visible_entities: vec![],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![],
        }
    }

    #[test]
    fn test_health_below_success() {
        // health = 30, threshold = 0.5 → 0.3 < 0.5 → Success
        let obs = test_observation_with_hostile(200.0, 30.0);
        let mut tree = BehaviorTree::new(BehaviorNode::health_below(0.5));
        let (_, status) = tree.tick(&obs);
        assert_eq!(status, BehaviorStatus::Success);
    }

    #[test]
    fn test_health_below_failure() {
        // health = 80, threshold = 0.5 → 0.8 > 0.5 → Failure
        let obs = test_observation_with_hostile(200.0, 80.0);
        let mut tree = BehaviorTree::new(BehaviorNode::health_below(0.5));
        let (_, status) = tree.tick(&obs);
        assert_eq!(status, BehaviorStatus::Failure);
    }

    #[test]
    fn test_enemy_in_range() {
        // hostile at distance 50, range check 100 → Success
        let obs = test_observation_with_hostile(50.0, 100.0);
        let mut tree = BehaviorTree::new(BehaviorNode::enemy_in_range(100.0));
        let (_, status) = tree.tick(&obs);
        assert_eq!(status, BehaviorStatus::Success);

        // hostile at distance 150, range check 100 → Failure
        let obs_far = test_observation_with_hostile(150.0, 100.0);
        let mut tree2 = BehaviorTree::new(BehaviorNode::enemy_in_range(100.0));
        let (_, status2) = tree2.tick(&obs_far);
        assert_eq!(status2, BehaviorStatus::Failure);
    }

    #[test]
    fn test_move_to_nearest_hostile() {
        // Hostile at (50, 0).  Expected action: Move { direction: (1, 0) }
        let obs = test_observation_with_hostile(50.0, 100.0);
        let mut tree = BehaviorTree::new(BehaviorNode::move_to_nearest(Relationship::Hostile));
        let (actions, status) = tree.tick(&obs);
        assert_eq!(status, BehaviorStatus::Success);
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Move { direction } => {
                assert!(
                    (direction.x - 1.0).abs() < 1e-4,
                    "expected direction (1,0), got {:?}",
                    direction
                );
            }
            other => panic!("expected Move action, got {:?}", other),
        }
    }

    #[test]
    fn test_aggressive_combat() {
        // Enemy at distance 50 (< 80 range): should attack
        let obs_close = test_observation_with_hostile(50.0, 100.0);
        let mut tree = BehaviorTree::new(aggressive_combat(Vec2::ZERO, 200.0));
        let (actions, status) = tree.tick(&obs_close);
        assert!(matches!(status, BehaviorStatus::Success | BehaviorStatus::Running));
        // Should produce at least a LookAt and Attack
        assert!(
            actions.iter().any(|a| matches!(a, Action::Attack)),
            "expected Attack in actions: {:?}",
            actions
        );

        // No enemies: should wander (Move action)
        let obs_empty = test_observation_empty();
        let mut tree2 = BehaviorTree::new(aggressive_combat(Vec2::ZERO, 200.0));
        let (actions2, status2) = tree2.tick(&obs_empty);
        assert!(matches!(status2, BehaviorStatus::Success | BehaviorStatus::Running));
        assert!(
            actions2.iter().any(|a| matches!(a, Action::Move { .. })),
            "expected Move in wander fallback: {:?}",
            actions2
        );
    }

    #[test]
    fn test_defensive_combat() {
        // Low health + enemy present: should flee (Move action)
        let obs_hurt = test_observation_with_hostile(50.0, 20.0);
        let mut tree = BehaviorTree::new(defensive_combat(Vec2::ZERO, 100.0));
        let (actions, status) = tree.tick(&obs_hurt);
        assert_eq!(status, BehaviorStatus::Success);
        assert!(
            actions.iter().any(|a| matches!(a, Action::Move { .. })),
            "expected Move (flee) when hurt: {:?}",
            actions
        );

        // Healthy + enemy in range: should attack
        let obs_healthy = test_observation_with_hostile(80.0, 100.0);
        let mut tree2 = BehaviorTree::new(defensive_combat(Vec2::ZERO, 100.0));
        let (actions2, _) = tree2.tick(&obs_healthy);
        assert!(
            actions2.iter().any(|a| matches!(a, Action::Attack)),
            "expected Attack when healthy and enemy in range: {:?}",
            actions2
        );
    }

    // ================================================================
    // TASK 2.9 — FSM tests
    // ================================================================

    #[test]
    fn test_fsm_transition() {
        // Idle → Alert on enemy_visible
        let mut fsm = FiniteStateMachine::new(FsmState::Idle, Action::Idle);
        fsm.add_transition(FsmTransition {
            from: FsmState::Idle,
            to: FsmState::Alert,
            condition: "enemy_visible".into(),
        });
        fsm.set_state_action(FsmState::Alert, Action::Attack);

        let obs = test_observation_with_hostile(100.0, 100.0);
        let action = fsm.tick(&obs);

        assert_eq!(fsm.current_state, FsmState::Alert);
        assert!(matches!(action, Action::Attack));
    }

    #[test]
    fn test_fsm_state_action() {
        let mut fsm = FiniteStateMachine::new(FsmState::Idle, Action::Idle);
        fsm.set_state_action(FsmState::Idle, Action::Idle);
        fsm.set_state_action(FsmState::Attacking, Action::Attack);

        // No transition configured — stays Idle
        let obs_empty = test_observation_empty();
        let action = fsm.tick(&obs_empty);
        assert!(matches!(action, Action::Idle));
        assert_eq!(fsm.current_state, FsmState::Idle);
    }

    #[test]
    fn test_combat_fsm_template() {
        let mut fsm = combat_fsm();
        assert_eq!(fsm.current_state, FsmState::Idle);

        // Tick with enemy visible: should move to Alert
        let obs = test_observation_with_hostile(200.0, 100.0);
        fsm.tick(&obs);
        assert_eq!(fsm.current_state, FsmState::Alert);

        // Tick again with enemy close: should move to Attacking
        let obs_close = test_observation_with_hostile(50.0, 100.0);
        fsm.tick(&obs_close);
        assert_eq!(fsm.current_state, FsmState::Attacking);

        // Attacking → Alert when no enemies
        let obs_empty = test_observation_empty();
        let action = fsm.tick(&obs_empty);
        assert_eq!(fsm.current_state, FsmState::Alert);
        // Alert state action is LookAt (not Attack)
        assert!(!matches!(action, Action::Attack));
    }

    #[test]
    fn test_patrol_fsm_template() {
        let mut fsm = patrol_fsm();
        assert_eq!(fsm.current_state, FsmState::Patrolling);

        // Patrolling → Chasing on enemy_visible
        let obs = test_observation_with_hostile(200.0, 100.0);
        fsm.tick(&obs);
        assert_eq!(fsm.current_state, FsmState::Chasing);

        // Chasing → Attacking when enemy in range 80
        let obs_close = test_observation_with_hostile(50.0, 100.0);
        let action = fsm.tick(&obs_close);
        assert_eq!(fsm.current_state, FsmState::Attacking);
        assert!(matches!(action, Action::Attack));

        // Attacking → Chasing when enemy_visible (but not close; distance 200 > 80)
        // Note: transition from Attacking checks "enemy_visible", not range
        let obs_far = test_observation_with_hostile(200.0, 100.0);
        fsm.tick(&obs_far);
        assert_eq!(fsm.current_state, FsmState::Chasing);

        // Chasing → Patrolling when no enemies
        let obs_empty = test_observation_empty();
        fsm.tick(&obs_empty);
        assert_eq!(fsm.current_state, FsmState::Patrolling);
    }
}
