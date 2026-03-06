use pod_core::World;
use serde::{Deserialize, Serialize};

/// State transitions
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum StateTransition {
    /// Push a new state on top of the stack
    Push,
    /// Pop the current state
    Pop,
    /// Replace the current state
    Switch,
    /// Quit the game/application
    Quit,
}

/// Trait for game states with lifecycle hooks
pub trait GameState: Send + Sync {
    /// Called when the state is entered
    fn on_enter(&mut self, _world: &mut World) {}

    /// Called when the state is exited
    fn on_exit(&mut self, _world: &mut World) {}

    /// Update the state
    fn update(&mut self, _world: &mut World) -> StateTransition {
        StateTransition::Push
    }

    /// Render the state (non-interactive)
    fn render(&self) {}

    /// Get the state name for debugging
    fn name(&self) -> &'static str {
        "Unknown"
    }

    /// Check if this state handles pause
    fn handles_pause(&self) -> bool {
        false
    }
}

/// Stack-based state machine with Push/Pop/Switch
pub struct StateStack {
    states: Vec<Box<dyn GameState>>,
    pending_transition: Option<StateTransition>,
}

impl StateStack {
    pub fn new() -> Self {
        Self {
            states: Vec::new(),
            pending_transition: None,
        }
    }

    /// Push a new state
    pub fn push(&mut self, mut state: Box<dyn GameState>, world: &mut World) {
        state.on_enter(world);
        self.states.push(state);
        log::info!(
            "Pushed state: {}. Stack depth: {}",
            self.states.last().map(|s| s.name()).unwrap_or("?"),
            self.states.len()
        );
    }

    /// Pop the current state
    pub fn pop(&mut self, world: &mut World) -> Option<Box<dyn GameState>> {
        if let Some(mut state) = self.states.pop() {
            state.on_exit(world);
            log::info!("Popped state. Stack depth: {}", self.states.len());
            Some(state)
        } else {
            None
        }
    }

    /// Switch to a different state (pops current, pushes new)
    pub fn switch(&mut self, new_state: Box<dyn GameState>, world: &mut World) {
        if let Some(mut old) = self.states.pop() {
            old.on_exit(world);
            log::info!("Switched from state: {}", old.name());
        }
        self.push(new_state, world);
    }

    /// Update the current state
    pub fn update(&mut self, world: &mut World) -> bool {
        if let Some(state) = self.states.last_mut() {
            self.pending_transition = Some(state.update(world));
        }

        // Process pending transitions
        if let Some(transition) = self.pending_transition.take() {
            match transition {
                StateTransition::Push => true,
                StateTransition::Pop => {
                    self.pop(world);
                    self.states.len() > 0
                }
                StateTransition::Switch => true,
                StateTransition::Quit => false,
            }
        } else {
            true
        }
    }

    /// Render all states (bottom to top)
    pub fn render(&self) {
        for state in &self.states {
            state.render();
        }
    }

    /// Get the current state
    pub fn current(&self) -> Option<&dyn GameState> {
        self.states.last().map(|s| s.as_ref())
    }

    /// Get a mutable reference to the current state
    pub fn current_mut(&mut self) -> Option<&mut (dyn GameState + '_)> {
        if let Some(state) = self.states.last_mut() {
            Some(state.as_mut())
        } else {
            None
        }
    }

    /// Get the depth of the stack
    pub fn depth(&self) -> usize {
        self.states.len()
    }

    /// Check if the stack is empty
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// Clear the stack
    pub fn clear(&mut self, world: &mut World) {
        for state in self.states.drain(..) {
            let mut s = state;
            s.on_exit(world);
        }
    }

    /// Get all state names for debugging
    pub fn get_state_names(&self) -> Vec<&'static str> {
        self.states.iter().map(|s| s.name()).collect()
    }
}

impl Default for StateStack {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// BUILT-IN STATES
// ============================================================

/// Loading state for asset streaming
#[derive(Debug)]
pub struct LoadingState {
    progress: f32,
    target_scene: String,
}

impl LoadingState {
    pub fn new(target_scene: impl Into<String>) -> Self {
        Self {
            progress: 0.0,
            target_scene: target_scene.into(),
        }
    }

    pub fn set_progress(&mut self, progress: f32) {
        self.progress = (progress).clamp(0.0, 1.0);
    }

    pub fn get_progress(&self) -> f32 {
        self.progress
    }
}

impl GameState for LoadingState {
    fn on_enter(&mut self, _world: &mut World) {
        log::info!("Loading state entered. Target: {}", self.target_scene);
    }

    fn update(&mut self, _world: &mut World) -> StateTransition {
        // Simulate loading progress
        self.progress += 0.1;
        if self.progress >= 1.0 {
            StateTransition::Pop
        } else {
            StateTransition::Push
        }
    }

    fn name(&self) -> &'static str {
        "Loading"
    }
}

/// Main menu state
#[derive(Debug)]
pub struct MainMenuState {
    selected_option: usize,
}

impl MainMenuState {
    pub fn new() -> Self {
        Self {
            selected_option: 0,
        }
    }

    pub fn select_option(&mut self, index: usize) {
        self.selected_option = index;
    }

    pub fn get_selected(&self) -> usize {
        self.selected_option
    }
}

impl GameState for MainMenuState {
    fn on_enter(&mut self, _world: &mut World) {
        log::info!("Main menu state entered");
    }

    fn update(&mut self, _world: &mut World) -> StateTransition {
        StateTransition::Push
    }

    fn name(&self) -> &'static str {
        "MainMenu"
    }
}

/// Playing state (active gameplay)
#[derive(Debug)]
pub struct PlayingState {
    scene_name: String,
}

impl PlayingState {
    pub fn new(scene_name: impl Into<String>) -> Self {
        Self {
            scene_name: scene_name.into(),
        }
    }

    pub fn get_scene(&self) -> &str {
        &self.scene_name
    }
}

impl GameState for PlayingState {
    fn on_enter(&mut self, _world: &mut World) {
        log::info!("Playing state entered. Scene: {}", self.scene_name);
    }

    fn update(&mut self, _world: &mut World) -> StateTransition {
        StateTransition::Push
    }

    fn name(&self) -> &'static str {
        "Playing"
    }
}

/// Paused state (overlaid on playing state)
#[derive(Debug)]
pub struct PausedState;

impl PausedState {
    pub fn new() -> Self {
        Self
    }
}

impl GameState for PausedState {
    fn on_enter(&mut self, _world: &mut World) {
        log::info!("Game paused");
    }

    fn on_exit(&mut self, _world: &mut World) {
        log::info!("Game resumed");
    }

    fn update(&mut self, _world: &mut World) -> StateTransition {
        StateTransition::Push
    }

    fn name(&self) -> &'static str {
        "Paused"
    }

    fn handles_pause(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_stack_push_pop() {
        let mut stack = StateStack::new();
        let mut world = World::new(42);

        stack.push(Box::new(LoadingState::new("Test")), &mut world);
        assert_eq!(stack.depth(), 1);

        stack.pop(&mut world);
        assert_eq!(stack.depth(), 0);
    }

    #[test]
    fn test_state_stack_names() {
        let mut stack = StateStack::new();
        let mut world = World::new(42);

        stack.push(Box::new(MainMenuState::new()), &mut world);
        stack.push(Box::new(PlayingState::new("Scene1")), &mut world);

        let names = stack.get_state_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"MainMenu"));
        assert!(names.contains(&"Playing"));
    }

    #[test]
    fn test_loading_state_progress() {
        let mut state = LoadingState::new("Scene");
        assert_eq!(state.get_progress(), 0.0);

        state.set_progress(0.5);
        assert_eq!(state.get_progress(), 0.5);

        state.set_progress(1.5); // Clamp to 1.0
        assert_eq!(state.get_progress(), 1.0);
    }

    #[test]
    fn test_paused_state_handles_pause() {
        let state = PausedState::new();
        assert!(state.handles_pause());

        let playing = PlayingState::new("Scene");
        assert!(!playing.handles_pause());
    }

    #[test]
    fn test_state_stack_is_empty() {
        let stack = StateStack::new();
        assert!(stack.is_empty());
    }

    #[test]
    fn test_main_menu_selection() {
        let mut state = MainMenuState::new();
        state.select_option(2);
        assert_eq!(state.get_selected(), 2);
    }
}
