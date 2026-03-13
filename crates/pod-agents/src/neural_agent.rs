//! Neural network policy agent with production-quality features.
//!
//! Features:
//! - Policy network interface trait for pluggable networks (ONNX, custom)
//! - Observation encoding: converts Observation to fixed-size float tensor
//! - Action decoding: maps network output back to Action enum
//! - Experience buffer: stores transitions for potential training
//! - Support for multi-action policies and continuous control

use glam::Vec2;
use log::debug;
use pod_core::action::{Action, AgentConstraints};
use pod_core::agent::{Agent, AgentIntrospection, AgentType};
use pod_core::id::AgentId;
use pod_core::observation::{Observation, Relationship};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;

pub const NEURAL_INTERFACE_VERSION: u32 = 1;
pub const NEURAL_FEATURE_COUNT: usize = 32;
pub const NEURAL_ACTION_COUNT: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeuralRuntimeSchema {
    pub interface_version: u32,
    pub feature_count: usize,
    pub action_count: usize,
}

impl NeuralRuntimeSchema {
    pub const fn current() -> Self {
        Self {
            interface_version: NEURAL_INTERFACE_VERSION,
            feature_count: NEURAL_FEATURE_COUNT,
            action_count: NEURAL_ACTION_COUNT,
        }
    }

    pub fn validate_model_metadata(
        &self,
        metadata: &NeuralModelMetadata,
    ) -> Result<(), NeuralSchemaError> {
        if metadata.runtime_schema.interface_version != self.interface_version {
            return Err(NeuralSchemaError::InterfaceVersionMismatch {
                expected: self.interface_version,
                got: metadata.runtime_schema.interface_version,
            });
        }
        if metadata.runtime_schema.feature_count != self.feature_count {
            return Err(NeuralSchemaError::FeatureCountMismatch {
                expected: self.feature_count,
                got: metadata.runtime_schema.feature_count,
            });
        }
        if metadata.runtime_schema.action_count != self.action_count {
            return Err(NeuralSchemaError::ActionCountMismatch {
                expected: self.action_count,
                got: metadata.runtime_schema.action_count,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeuralModelMetadata {
    pub model_name: String,
    pub runtime_schema: NeuralRuntimeSchema,
}

impl NeuralModelMetadata {
    pub fn current(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            runtime_schema: NeuralRuntimeSchema::current(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NeuralSchemaError {
    InterfaceVersionMismatch { expected: u32, got: u32 },
    FeatureCountMismatch { expected: usize, got: usize },
    ActionCountMismatch { expected: usize, got: usize },
}

impl fmt::Display for NeuralSchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NeuralSchemaError::InterfaceVersionMismatch { expected, got } => {
                write!(
                    f,
                    "neural interface version mismatch: expected {expected}, got {got}"
                )
            }
            NeuralSchemaError::FeatureCountMismatch { expected, got } => {
                write!(
                    f,
                    "neural feature count mismatch: expected {expected}, got {got}"
                )
            }
            NeuralSchemaError::ActionCountMismatch { expected, got } => {
                write!(
                    f,
                    "neural action count mismatch: expected {expected}, got {got}"
                )
            }
        }
    }
}

impl std::error::Error for NeuralSchemaError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NeuralCompatibilityStatus {
    Compatible,
    FallbackOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NeuralInferenceStatus {
    Ready,
    Fallback { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NeuralPolicyRuntimeStatus {
    pub model_name: String,
    pub runtime_schema: NeuralRuntimeSchema,
    pub compatibility: NeuralCompatibilityStatus,
    pub last_inference: NeuralInferenceStatus,
}

impl NeuralPolicyRuntimeStatus {
    pub fn ready(model_name: impl Into<String>) -> Self {
        Self {
            model_name: model_name.into(),
            runtime_schema: NeuralRuntimeSchema::current(),
            compatibility: NeuralCompatibilityStatus::Compatible,
            last_inference: NeuralInferenceStatus::Ready,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NeuralActionSchemaEntry {
    pub index: usize,
    pub key: &'static str,
    pub label: &'static str,
    pub build_action: fn() -> Action,
}

/// Experience transition for training
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Experience {
    pub observation_encoding: Vec<f32>,
    pub action_index: usize,
    pub reward: f32,
    pub next_observation_encoding: Option<Vec<f32>>,
    pub terminal: bool,
}

/// Policy network trait for pluggable network implementations
pub trait PolicyNetwork: Send {
    /// Forward pass: takes feature vector, returns action probabilities/values
    /// Output should be action_count elements
    fn forward(&self, features: &[f32]) -> Vec<f32>;

    /// Optional: get action logits (for training)
    fn get_logits(&self, features: &[f32]) -> Vec<f32> {
        self.forward(features)
    }

    /// Runtime-facing status for compatibility and fallback inspection.
    fn runtime_status(&self) -> NeuralPolicyRuntimeStatus {
        NeuralPolicyRuntimeStatus::ready("custom-policy")
    }
}

/// Default policy network: simple uniform distribution
pub struct UniformPolicyNetwork;

impl PolicyNetwork for UniformPolicyNetwork {
    fn forward(&self, _features: &[f32]) -> Vec<f32> {
        vec![1.0 / NEURAL_ACTION_COUNT as f32; NEURAL_ACTION_COUNT]
    }

    fn runtime_status(&self) -> NeuralPolicyRuntimeStatus {
        NeuralPolicyRuntimeStatus {
            model_name: "uniform-policy".to_string(),
            runtime_schema: NeuralRuntimeSchema::current(),
            compatibility: NeuralCompatibilityStatus::FallbackOnly,
            last_inference: NeuralInferenceStatus::Fallback {
                reason: "builtin uniform fallback policy".to_string(),
            },
        }
    }
}

/// Trait for selecting actions from network output
pub trait ActionSelector: Send {
    /// Take a feature vector and return an action index
    /// The agent will map this index to an actual Action
    fn select_action(&self, features: &[f32]) -> usize;
}

/// Default random action selector (for testing)
pub struct RandomActionSelector;

impl ActionSelector for RandomActionSelector {
    fn select_action(&self, features: &[f32]) -> usize {
        // Simple hash-based pseudo-random selection
        let sum: f32 = features.iter().sum();
        let seed = sum.abs() as u64;
        (seed.wrapping_mul(1103515245).wrapping_add(12345) as usize) % NEURAL_ACTION_COUNT
    }
}

/// Greedy action selector (max feature activation)
pub struct GreedyActionSelector;

impl ActionSelector for GreedyActionSelector {
    fn select_action(&self, features: &[f32]) -> usize {
        features
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
            .unwrap_or(0)
            .min(NEURAL_ACTION_COUNT - 1)
    }
}

/// Action index to Action mapping
pub const NEURAL_ACTION_SCHEMA: &[NeuralActionSchemaEntry] = &[
    NeuralActionSchemaEntry {
        index: 0,
        key: "idle",
        label: "Idle",
        build_action: || Action::Idle,
    },
    NeuralActionSchemaEntry {
        index: 1,
        key: "move_up",
        label: "Move Up",
        build_action: || Action::Move {
            direction: Vec2::new(0.0, -1.0),
        },
    },
    NeuralActionSchemaEntry {
        index: 2,
        key: "move_down",
        label: "Move Down",
        build_action: || Action::Move {
            direction: Vec2::new(0.0, 1.0),
        },
    },
    NeuralActionSchemaEntry {
        index: 3,
        key: "move_left",
        label: "Move Left",
        build_action: || Action::Move {
            direction: Vec2::new(-1.0, 0.0),
        },
    },
    NeuralActionSchemaEntry {
        index: 4,
        key: "move_right",
        label: "Move Right",
        build_action: || Action::Move {
            direction: Vec2::new(1.0, 0.0),
        },
    },
    NeuralActionSchemaEntry {
        index: 5,
        key: "stop",
        label: "Stop",
        build_action: || Action::Stop,
    },
    NeuralActionSchemaEntry {
        index: 6,
        key: "attack",
        label: "Attack",
        build_action: || Action::Attack,
    },
    NeuralActionSchemaEntry {
        index: 7,
        key: "interact",
        label: "Interact",
        build_action: || Action::Interact,
    },
    NeuralActionSchemaEntry {
        index: 8,
        key: "drop_slot_0",
        label: "Drop Slot 0",
        build_action: || Action::Drop { slot: 0 },
    },
    NeuralActionSchemaEntry {
        index: 9,
        key: "rotate_quarter_pi",
        label: "Rotate 45 Degrees",
        build_action: || Action::Rotate {
            angle: std::f32::consts::PI / 4.0,
        },
    },
];

fn index_to_action(index: usize) -> Action {
    debug_assert_eq!(NEURAL_ACTION_SCHEMA.len(), NEURAL_ACTION_COUNT);
    let idx = index % NEURAL_ACTION_COUNT;
    (NEURAL_ACTION_SCHEMA[idx].build_action)()
}

/// Neural network policy agent with experience replay
pub struct NeuralAgent {
    id: AgentId,
    selector: Box<dyn ActionSelector>,
    policy: Box<dyn PolicyNetwork>,
    constraints: AgentConstraints,
    last_observation: Option<Observation>,
    last_features: Option<Vec<f32>>,
    last_action: Option<usize>,

    /// Experience buffer for training
    experience_buffer: VecDeque<Experience>,
    max_buffer_size: usize,

    /// Timestep counter
    timestep: u64,
}

impl NeuralAgent {
    /// Create with a random policy and uniform network
    pub fn new() -> Self {
        Self::with_selector_and_network(
            Box::new(RandomActionSelector),
            Box::new(UniformPolicyNetwork),
        )
    }

    /// Create with a custom action selector
    pub fn with_selector(selector: Box<dyn ActionSelector>) -> Self {
        Self::with_selector_and_network(selector, Box::new(UniformPolicyNetwork))
    }

    /// Create with custom selector and policy network
    pub fn with_selector_and_network(
        selector: Box<dyn ActionSelector>,
        policy: Box<dyn PolicyNetwork>,
    ) -> Self {
        Self {
            id: AgentId::new(),
            selector,
            policy,
            constraints: AgentConstraints::default(),
            last_observation: None,
            last_features: None,
            last_action: None,
            experience_buffer: VecDeque::new(),
            max_buffer_size: 1000,
            timestep: 0,
        }
    }

    /// Create with a specific ID (for testing/loading)
    pub fn with_id(id: AgentId, selector: Box<dyn ActionSelector>) -> Self {
        Self {
            id,
            selector,
            policy: Box::new(UniformPolicyNetwork),
            constraints: AgentConstraints::default(),
            last_observation: None,
            last_features: None,
            last_action: None,
            experience_buffer: VecDeque::new(),
            max_buffer_size: 1000,
            timestep: 0,
        }
    }

    /// Get experience buffer
    pub fn experience_buffer(&self) -> &VecDeque<Experience> {
        &self.experience_buffer
    }

    pub fn runtime_schema() -> NeuralRuntimeSchema {
        NeuralRuntimeSchema::current()
    }

    pub fn action_schema() -> &'static [NeuralActionSchemaEntry] {
        NEURAL_ACTION_SCHEMA
    }

    /// Extract authoritative training samples for a specific agent from a replay.
    pub fn training_samples_from_replay(
        replay: &pod_core::ReplayFile,
        agent_id: AgentId,
    ) -> Vec<pod_core::ReplayTrainingSample> {
        replay
            .training_samples()
            .into_iter()
            .filter(|sample| sample.agent_id == agent_id)
            .collect()
    }

    /// Record experience transition (for training)
    pub fn record_experience(
        &mut self,
        reward: f32,
        next_observation: &Observation,
        terminal: bool,
    ) {
        if let (Some(features), Some(action_idx)) =
            (self.last_features.take(), self.last_action.take())
        {
            let next_features = Self::observation_to_features(next_observation);

            let exp = Experience {
                observation_encoding: features,
                action_index: action_idx,
                reward,
                next_observation_encoding: Some(next_features),
                terminal,
            };

            self.experience_buffer.push_back(exp);

            // Limit buffer size
            while self.experience_buffer.len() > self.max_buffer_size {
                self.experience_buffer.pop_front();
            }
        }
    }

    /// Convert observation to a fixed-size feature vector (tensor)
    /// Output: 32-element float vector (normalized to ~[-1, 1])
    pub fn observation_to_features(obs: &Observation) -> Vec<f32> {
        let mut features = Vec::new();

        // ===== SELF STATE (6 features) =====
        features.push((obs.self_state.position.x / 1000.0).clamp(-1.0, 1.0)); // position x
        features.push((obs.self_state.position.y / 1000.0).clamp(-1.0, 1.0)); // position y
        features.push((obs.self_state.velocity.x / 500.0).clamp(-1.0, 1.0)); // velocity x
        features.push((obs.self_state.velocity.y / 500.0).clamp(-1.0, 1.0)); // velocity y
        features.push((obs.self_state.rotation / std::f32::consts::PI).clamp(-1.0, 1.0)); // rotation
        let health_ratio = match (obs.self_state.health, obs.self_state.max_health) {
            (Some(h), Some(m)) if m > 0.0 => (h / m).clamp(0.0, 1.0),
            _ => 1.0,
        };
        features.push(health_ratio);

        // ===== VISIBLE ENTITIES (12 features) =====
        // Stats about nearby entities
        let mut friendly_closest: f32 = 1000.0;
        let mut hostile_closest: f32 = 1000.0;
        let mut friendly_count = 0;
        let mut hostile_count = 0;

        for entity in &obs.visible_entities {
            match entity.relationship {
                Relationship::Friendly => {
                    friendly_count += 1;
                    friendly_closest = friendly_closest.min(entity.distance);
                }
                Relationship::Hostile => {
                    hostile_count += 1;
                    hostile_closest = hostile_closest.min(entity.distance);
                }
                _ => {}
            }
        }

        // Entity threat features (4 features)
        features.push((hostile_count as f32).clamp(0.0, 1.0)); // hostile count (clamped)
        features.push((friendly_count as f32).clamp(0.0, 1.0)); // friendly count
        features.push(if hostile_closest < 1000.0 {
            1.0 - (hostile_closest / 1000.0) // closest threat proximity
        } else {
            0.0
        });
        features.push(if friendly_closest < 1000.0 {
            1.0 - (friendly_closest / 1000.0) // closest ally proximity
        } else {
            0.0
        });

        // Top 4 visible entities (8 features)
        let mut entity_features = vec![0.0; 8];
        for (i, entity) in obs.visible_entities.iter().take(4).enumerate() {
            let salience = Self::entity_salience(entity);
            entity_features[i * 2] = (entity.distance / 500.0).clamp(0.0, 1.0);
            entity_features[i * 2 + 1] = salience;
        }
        features.extend(entity_features);

        // ===== AUDIO/COMMUNICATION (4 features) =====
        features.push((obs.audible_events.len() as f32).clamp(0.0, 1.0)); // sound events
        features.push((obs.messages.len() as f32).clamp(0.0, 1.0)); // received messages

        // Most recent audible event distance
        if let Some(event) = obs.audible_events.first() {
            features.push((event.distance / 500.0).clamp(0.0, 1.0));
            features.push(event.intensity.clamp(0.0, 1.0));
        } else {
            features.push(0.0);
            features.push(0.0);
        }

        // ===== OBJECTIVES (4 features) =====
        let completed_objectives = obs.objectives.iter().filter(|o| o.completed).count();
        features.push((completed_objectives as f32).clamp(0.0, 1.0));
        features.push((obs.objectives.len() as f32).clamp(0.0, 1.0));

        // Most recent objective progress
        if let Some(obj) = obs.objectives.first() {
            features.push(obj.progress.clamp(0.0, 1.0));
            features.push(if obj.completed { 1.0 } else { 0.0 });
        } else {
            features.push(0.0);
            features.push(0.0);
        }

        // ===== GAME STATE (2 features) =====
        features.push((obs.tick as f32) / 1000.0); // normalized tick
        features.push((obs.elapsed_secs / 60.0).clamp(0.0, 1.0)); // normalized elapsed time

        // Pad to exactly the current schema width.
        while features.len() < NEURAL_FEATURE_COUNT {
            features.push(0.0);
        }
        features.truncate(NEURAL_FEATURE_COUNT);

        // Ensure all features are finite
        for f in &mut features {
            if !f.is_finite() {
                *f = 0.0;
            }
        }

        features
    }

    /// Calculate salience score for entity (0.0-1.0)
    fn entity_salience(entity: &pod_core::observation::VisibleEntity) -> f32 {
        let mut salience = 0.0;

        if matches!(entity.relationship, Relationship::Hostile) {
            salience += 0.6;
        } else if matches!(entity.relationship, Relationship::Friendly) {
            salience += 0.2;
        }

        let distance_factor = (1.0 - (entity.distance / 500.0).min(1.0)) * 0.3;
        salience += distance_factor;

        if let Some(hp) = entity.health_fraction {
            if hp < 0.5 {
                salience += 0.1;
            }
        }

        salience.clamp(0.0, 1.0)
    }
}

impl Default for NeuralAgent {
    fn default() -> Self {
        Self::new()
    }
}

impl Agent for NeuralAgent {
    fn id(&self) -> AgentId {
        self.id
    }

    fn agent_type(&self) -> AgentType {
        AgentType::NeuralAgent
    }

    fn observe(&mut self, observation: Observation) {
        self.last_observation = Some(observation);
    }

    fn decide(&mut self) -> Vec<Action> {
        self.timestep += 1;

        if let Some(ref observation) = self.last_observation {
            let features = Self::observation_to_features(observation);

            // Get action probabilities from policy network
            let action_probs = self.policy.forward(&features);

            // Select action using selector
            let action_index = self.selector.select_action(&action_probs);
            let action = index_to_action(action_index);

            // Store features and action for experience recording
            self.last_features = Some(features);
            self.last_action = Some(action_index);

            debug!(
                "NeuralAgent {} timestep {} selected action {} from policy",
                self.id, self.timestep, action_index
            );

            vec![action]
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

    fn introspect(&self) -> AgentIntrospection {
        let policy_status = self.policy.runtime_status();
        let last_action = self
            .last_action
            .and_then(|index| Self::action_schema().get(index))
            .map(|entry| entry.key)
            .unwrap_or("none");
        let last_inference = match &policy_status.last_inference {
            NeuralInferenceStatus::Ready => "ready".to_string(),
            NeuralInferenceStatus::Fallback { reason } => format!("fallback({reason})"),
        };
        let compatibility = match policy_status.compatibility {
            NeuralCompatibilityStatus::Compatible => "compatible",
            NeuralCompatibilityStatus::FallbackOnly => "fallback-only",
        };

        AgentIntrospection {
            agent_id: self.id,
            agent_type: AgentType::NeuralAgent,
            constraints: self.constraints.clone(),
            state_description: format!(
                "policy={} schema=v{} features={} actions={} compatibility={} inference={} last_action={} experience_buffer={} timestep={}",
                policy_status.model_name,
                policy_status.runtime_schema.interface_version,
                policy_status.runtime_schema.feature_count,
                policy_status.runtime_schema.action_count,
                compatibility,
                last_inference,
                last_action,
                self.experience_buffer.len(),
                self.timestep,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::component::Team;
    use pod_core::contract::{AgentCapabilities, AgentRole, AgentRuntimeProfile};
    use pod_core::observation::{SelfState, VisibleEntity};

    struct FixedActionSelector(usize);

    impl ActionSelector for FixedActionSelector {
        fn select_action(&self, _features: &[f32]) -> usize {
            self.0
        }
    }

    struct InspectableFallbackPolicy;

    impl PolicyNetwork for InspectableFallbackPolicy {
        fn forward(&self, _features: &[f32]) -> Vec<f32> {
            vec![1.0 / NEURAL_ACTION_COUNT as f32; NEURAL_ACTION_COUNT]
        }

        fn runtime_status(&self) -> NeuralPolicyRuntimeStatus {
            NeuralPolicyRuntimeStatus {
                model_name: "inspectable-fallback".to_string(),
                runtime_schema: NeuralRuntimeSchema::current(),
                compatibility: NeuralCompatibilityStatus::FallbackOnly,
                last_inference: NeuralInferenceStatus::Fallback {
                    reason: "synthetic test fallback".to_string(),
                },
            }
        }
    }

    fn make_test_observation() -> Observation {
        Observation {
            tick: 0,
            elapsed_secs: 0.0,
            self_state: SelfState {
                agent_id: AgentId::new(),
                entity_id: pod_core::id::EntityId(1),
                position: Vec2::new(100.0, 200.0),
                rotation: 0.0,
                velocity: Vec2::new(10.0, 0.0),
                health: Some(100.0),
                max_health: Some(100.0),
                team: Team::Team(1),
                cooldowns: vec![],
                ..Default::default()
            },
            visible_entities: vec![VisibleEntity {
                entity_id: pod_core::id::EntityId(2),
                entity_type: "enemy".to_string(),
                position: Vec2::new(200.0, 200.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                distance: 100.0,
                relationship: Relationship::Hostile,
                health_fraction: Some(0.5),
                ..Default::default()
            }],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![],
        }
    }

    #[test]
    fn test_observation_to_features() {
        let obs = make_test_observation();
        let features = NeuralAgent::observation_to_features(&obs);

        assert_eq!(features.len(), NEURAL_FEATURE_COUNT);
        assert!(!features.is_empty());
        // All features should be finite numbers
        for f in features {
            assert!(f.is_finite());
        }
    }

    #[test]
    fn test_random_action_selector() {
        let selector = RandomActionSelector;
        let features = vec![0.5; 20];
        let action_idx = selector.select_action(&features);
        assert!(action_idx < NEURAL_ACTION_COUNT);
    }

    #[test]
    fn test_greedy_action_selector() {
        let selector = GreedyActionSelector;
        let mut features = vec![0.1; 20];
        features[5] = 0.9; // Make index 5 the max
        let action_idx = selector.select_action(&features);
        assert_eq!(action_idx, 5);
    }

    #[test]
    fn test_neural_agent_decide() {
        let mut agent = NeuralAgent::new();
        let obs = make_test_observation();

        agent.observe(obs);
        let actions = agent.decide();

        assert_eq!(actions.len(), 1);
        // Action should be one of the valid ones
        match actions[0] {
            Action::Idle
            | Action::Move { .. }
            | Action::Stop
            | Action::Attack
            | Action::Interact
            | Action::Drop { .. }
            | Action::Rotate { .. } => {}
            _ => panic!("Unexpected action type"),
        }
    }

    #[test]
    fn test_index_to_action() {
        let action0 = index_to_action(0);
        assert!(matches!(action0, Action::Idle));

        let action1 = index_to_action(1);
        assert!(matches!(action1, Action::Move { .. }));

        let action_large = index_to_action(100);
        // Should wrap around
        let wrapped = index_to_action(100 % NEURAL_ACTION_COUNT);
        assert_eq!(format!("{:?}", action_large), format!("{:?}", wrapped));
    }

    #[test]
    fn neural_runtime_schema_matches_encoder_and_action_space() {
        let schema = NeuralAgent::runtime_schema();
        let features = NeuralAgent::observation_to_features(&make_test_observation());

        assert_eq!(schema.interface_version, NEURAL_INTERFACE_VERSION);
        assert_eq!(schema.feature_count, features.len());
        assert_eq!(schema.action_count, NEURAL_ACTION_SCHEMA.len());
    }

    #[test]
    fn neural_runtime_schema_rejects_mismatched_metadata() {
        let schema = NeuralRuntimeSchema::current();
        let metadata = NeuralModelMetadata {
            model_name: "bad-model".to_string(),
            runtime_schema: NeuralRuntimeSchema {
                interface_version: schema.interface_version,
                feature_count: schema.feature_count + 1,
                action_count: schema.action_count,
            },
        };

        let error = schema.validate_model_metadata(&metadata).unwrap_err();
        assert!(matches!(
            error,
            NeuralSchemaError::FeatureCountMismatch {
                expected: NEURAL_FEATURE_COUNT,
                got: 33
            }
        ));
    }

    #[test]
    fn neural_action_schema_is_indexed_and_named() {
        let schema = NeuralAgent::action_schema();
        assert_eq!(schema.len(), NEURAL_ACTION_COUNT);
        assert_eq!(schema[0].index, 0);
        assert_eq!(schema[0].key, "idle");
        assert_eq!(schema[6].key, "attack");
        assert_eq!(schema[9].label, "Rotate 45 Degrees");
    }

    #[test]
    fn neural_agent_introspection_reports_policy_runtime_status() {
        let mut agent = NeuralAgent::with_selector_and_network(
            Box::new(FixedActionSelector(0)),
            Box::new(InspectableFallbackPolicy),
        );
        agent.observe(make_test_observation());
        let _ = agent.decide();

        let info = agent.introspect();
        assert!(info.state_description.contains("policy=inspectable-fallback"));
        assert!(info.state_description.contains("compatibility=fallback-only"));
        assert!(info.state_description.contains("inference=fallback(synthetic test fallback)"));
        assert!(info.state_description.contains("last_action=idle"));
    }

    #[test]
    fn training_samples_from_replay_filter_by_agent() {
        let agent_id = AgentId::new();
        let other_agent = AgentId::new();
        let profile = AgentRuntimeProfile {
            role: AgentRole::Player,
            agent_type: AgentType::NeuralAgent,
            capabilities: AgentCapabilities::player_default(),
        };

        let first = pod_core::AgentTelemetryFrame::new(
            0, agent_id, None, profile, 0, 0, 0, 0, 0, None, None,
        );
        let second = pod_core::AgentTelemetryFrame::new(
            0,
            other_agent,
            None,
            profile,
            0,
            0,
            0,
            0,
            0,
            None,
            None,
        );
        let replay = pod_core::ReplayFile {
            header: pod_core::ReplayHeader {
                name: "training".into(),
                timestamp: 0,
                world_seed: 1,
                tick_count: 1,
                agent_count: 2,
                notes: String::new(),
            },
            traces: vec![],
            telemetry_windows: vec![pod_core::TickTelemetryFrame {
                tick: 0,
                agents: vec![first, second],
            }],
        };

        let samples = NeuralAgent::training_samples_from_replay(&replay, agent_id);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].agent_id, agent_id);
    }
}
