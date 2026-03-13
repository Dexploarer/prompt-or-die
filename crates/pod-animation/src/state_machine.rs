//! Animation state machine with transitions and conditions

use crate::keyframe::AnimationClip;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A state in the animation state machine
#[derive(Debug, Clone)]
pub struct AnimState {
    pub name: String,
    pub clip: AnimationClip,
}

impl AnimState {
    pub fn new(name: String, clip: AnimationClip) -> Self {
        Self { name, clip }
    }
}

/// Conditions that trigger transitions between animation states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransitionCondition {
    /// Check if a boolean parameter equals a value
    BoolParam { name: String, value: bool },
    /// Check if a float parameter exceeds a threshold
    FloatGreater { name: String, threshold: f32 },
    /// Check if a float parameter is below a threshold
    FloatLess { name: String, threshold: f32 },
    /// A one-time trigger that fires once then resets
    Trigger(String),
    /// Time elapsed in current state
    TimeElapsed(f32),
    /// Always transition (no condition)
    Always,
}

impl TransitionCondition {
    fn evaluate(
        &self,
        parameters: &AnimStateParameters,
        current_state_time: f32,
        triggered: &mut HashMap<String, bool>,
    ) -> bool {
        match self {
            TransitionCondition::BoolParam { name, value } => {
                parameters.get_bool(name) == Some(*value)
            }
            TransitionCondition::FloatGreater { name, threshold } => parameters
                .get_float(name)
                .map(|v| v > *threshold)
                .unwrap_or(false),
            TransitionCondition::FloatLess { name, threshold } => parameters
                .get_float(name)
                .map(|v| v < *threshold)
                .unwrap_or(false),
            TransitionCondition::Trigger(name) => {
                let was_triggered = triggered.get(name).copied().unwrap_or(false);
                if was_triggered {
                    triggered.insert(name.clone(), false);
                    true
                } else {
                    false
                }
            }
            TransitionCondition::TimeElapsed(duration) => current_state_time >= *duration,
            TransitionCondition::Always => true,
        }
    }
}

/// A transition from one state to another
#[derive(Debug, Clone)]
pub struct AnimTransition {
    pub from_state: String,
    pub to_state: String,
    pub condition: TransitionCondition,
    pub blend_duration: f32,
    pub has_exit_time: bool,
    pub exit_time: f32,
}

impl AnimTransition {
    pub fn new(
        from_state: String,
        to_state: String,
        condition: TransitionCondition,
        blend_duration: f32,
    ) -> Self {
        Self {
            from_state,
            to_state,
            condition,
            blend_duration,
            has_exit_time: false,
            exit_time: 0.0,
        }
    }

    pub fn with_exit_time(mut self, exit_time: f32) -> Self {
        self.has_exit_time = true;
        self.exit_time = exit_time;
        self
    }
}

/// Parameters for state machine conditions
#[derive(Debug, Clone, Default)]
pub struct AnimStateParameters {
    bools: HashMap<String, bool>,
    floats: HashMap<String, f32>,
}

impl AnimStateParameters {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_bool(&mut self, name: String, value: bool) {
        self.bools.insert(name, value);
    }

    pub fn set_float(&mut self, name: String, value: f32) {
        self.floats.insert(name, value);
    }

    pub fn trigger(&mut self, name: String) {
        self.bools.insert(format!("_trigger_{}", name), true);
    }

    pub fn get_bool(&self, name: &str) -> Option<bool> {
        self.bools.get(name).copied()
    }

    pub fn get_float(&self, name: &str) -> Option<f32> {
        self.floats.get(name).copied()
    }

    pub fn get_trigger(&self, name: &str) -> bool {
        self.bools
            .get(&format!("_trigger_{}", name))
            .copied()
            .unwrap_or(false)
    }

    pub fn clear_trigger(&mut self, name: &str) {
        self.bools.remove(&format!("_trigger_{}", name));
    }
}

/// Animation state machine component
#[derive(Debug, Clone)]
pub struct AnimStateMachine {
    pub name: String,
    states: HashMap<String, AnimState>,
    transitions: Vec<AnimTransition>,
    parameters: AnimStateParameters,
    triggered: HashMap<String, bool>,
    current_state: Option<String>,
    current_state_time: f32,
}

impl AnimStateMachine {
    pub fn new(name: String) -> Self {
        Self {
            name,
            states: HashMap::new(),
            transitions: Vec::new(),
            parameters: AnimStateParameters::new(),
            triggered: HashMap::new(),
            current_state: None,
            current_state_time: 0.0,
        }
    }

    pub fn add_state(&mut self, state: AnimState) {
        self.states.insert(state.name.clone(), state);
        if self.current_state.is_none() {
            self.current_state = Some(self.states.keys().next().unwrap().clone());
        }
    }

    pub fn add_transition(&mut self, transition: AnimTransition) {
        self.transitions.push(transition);
    }

    pub fn set_bool(&mut self, name: String, value: bool) {
        self.parameters.set_bool(name, value);
    }

    pub fn set_float(&mut self, name: String, value: f32) {
        self.parameters.set_float(name, value);
    }

    pub fn trigger(&mut self, name: String) {
        self.triggered.insert(name, true);
    }

    pub fn update(&mut self, dt: f32) {
        self.current_state_time += dt;

        // Check all transitions from current state
        if let Some(current) = &self.current_state {
            let mut next_state = None;

            for transition in &self.transitions {
                if transition.from_state == *current {
                    // Check exit time if required
                    if transition.has_exit_time {
                        if self.current_state_time < transition.exit_time {
                            continue;
                        }
                    }

                    if transition.condition.evaluate(
                        &self.parameters,
                        self.current_state_time,
                        &mut self.triggered,
                    ) {
                        next_state = Some(transition.to_state.clone());
                        break;
                    }
                }
            }

            if let Some(new_state) = next_state {
                self.current_state = Some(new_state);
                self.current_state_time = 0.0;
            }
        }
    }

    pub fn get_current_state_name(&self) -> Option<&str> {
        self.current_state.as_deref()
    }

    pub fn get_current_clip(&self) -> Option<AnimationClip> {
        self.current_state
            .as_ref()
            .and_then(|name| self.states.get(name))
            .map(|state| state.clip.clone())
    }
}

/// Component that plays an animation state machine
#[derive(Debug, Clone)]
pub struct AnimStatePlayer {
    pub state_machine: AnimStateMachine,
}

impl AnimStatePlayer {
    pub fn new(state_machine: AnimStateMachine) -> Self {
        Self { state_machine }
    }

    pub fn set_bool(&mut self, name: String, value: bool) {
        self.state_machine.set_bool(name, value);
    }

    pub fn set_float(&mut self, name: String, value: f32) {
        self.state_machine.set_float(name, value);
    }

    pub fn trigger(&mut self, name: String) {
        self.state_machine.trigger(name);
    }

    pub fn update(&mut self, dt: f32) {
        self.state_machine.update(dt);
    }

    pub fn get_current_state(&self) -> Option<&str> {
        self.state_machine.get_current_state_name()
    }

    pub fn get_current_clip(&self) -> Option<AnimationClip> {
        self.state_machine.get_current_clip()
    }
}

// ============================================================
// Builder for easier state machine construction
// ============================================================

/// Builder for constructing AnimStateMachine
pub struct AnimStateMachineBuilder {
    name: String,
    states: HashMap<String, AnimState>,
    transitions: Vec<AnimTransition>,
}

impl AnimStateMachineBuilder {
    pub fn new(name: String) -> Self {
        Self {
            name,
            states: HashMap::new(),
            transitions: Vec::new(),
        }
    }

    pub fn add_state(mut self, state: AnimState) -> Self {
        self.states.insert(state.name.clone(), state);
        self
    }

    pub fn add_transition(mut self, transition: AnimTransition) -> Self {
        self.transitions.push(transition);
        self
    }

    pub fn build(self) -> AnimStateMachine {
        let mut machine = AnimStateMachine::new(self.name);

        for (_, state) in self.states {
            machine.add_state(state);
        }

        for transition in self.transitions {
            machine.add_transition(transition);
        }

        machine
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bool_parameter() {
        let mut params = AnimStateParameters::new();
        params.set_bool("is_moving".to_string(), true);
        assert_eq!(params.get_bool("is_moving"), Some(true));
    }

    #[test]
    fn test_float_parameter() {
        let mut params = AnimStateParameters::new();
        params.set_float("speed".to_string(), 5.5);
        assert_eq!(params.get_float("speed"), Some(5.5));
    }

    #[test]
    fn test_condition_evaluation() {
        let mut params = AnimStateParameters::new();
        params.set_float("speed".to_string(), 10.0);

        let cond = TransitionCondition::FloatGreater {
            name: "speed".to_string(),
            threshold: 5.0,
        };

        let mut triggered = HashMap::new();
        assert!(cond.evaluate(&params, 0.0, &mut triggered));
    }
}
