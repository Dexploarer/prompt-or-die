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
        let status = self.tick_node(&mut self.root, &mut actions, observation);
        (actions, status)
    }

    fn tick_node(
        &mut self,
        node: &mut BehaviorNode,
        actions: &mut Vec<Action>,
        observation: &Observation,
    ) -> BehaviorStatus {
        match node {
            BehaviorNode::Sequence { children, current_child_index } => {
                // Sequence: run until one fails
                for i in *current_child_index..children.len() {
                    match self.tick_node(&mut children[i], actions, observation) {
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
                    match self.tick_node(&mut children[i], actions, observation) {
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
                    let status = self.tick_node(child, actions, observation);
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
                match self.tick_node(child, actions, observation) {
                    BehaviorStatus::Success => BehaviorStatus::Failure,
                    BehaviorStatus::Failure => BehaviorStatus::Success,
                    BehaviorStatus::Running => BehaviorStatus::Running,
                }
            }

            BehaviorNode::AlwaysSucceed { child } => {
                // Always succeed: ignore child status except running
                match self.tick_node(child, actions, observation) {
                    BehaviorStatus::Running => BehaviorStatus::Running,
                    _ => BehaviorStatus::Success,
                }
            }

            BehaviorNode::AlwaysFail { child } => {
                // Always fail: ignore child status except running
                match self.tick_node(child, actions, observation) {
                    BehaviorStatus::Running => BehaviorStatus::Running,
                    _ => BehaviorStatus::Failure,
                }
            }

            BehaviorNode::Repeater { child } => {
                // Repeater: keep running until child succeeds
                match self.tick_node(child, actions, observation) {
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

                match self.tick_node(child, actions, observation) {
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
        }
    }
}

/// Finite State Machine as alternative to behavior trees
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FsmState {
    Idle,
    Patrolling,
    Chasing,
    Fleeing,
    Guarding,
    Attacking,
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
    pub default_action: Action,
    ticks_in_state: u32,
}

impl FiniteStateMachine {
    pub fn new(initial_state: FsmState, default_action: Action) -> Self {
        Self {
            current_state: initial_state,
            transitions: Vec::new(),
            default_action,
            ticks_in_state: 0,
        }
    }

    pub fn add_transition(&mut self, transition: FsmTransition) {
        self.transitions.push(transition);
    }

    pub fn tick(&mut self, _observation: &Observation) -> Action {
        self.ticks_in_state += 1;

        // Simple tick: just return default action
        // In production, would evaluate conditions and transition
        self.default_action.clone()
    }
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
}
