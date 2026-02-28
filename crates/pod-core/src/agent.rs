use crate::action::{Action, AgentConstraints};
use crate::id::AgentId;
use crate::observation::Observation;
use serde::{Deserialize, Serialize};

/// The type of agent — used for logging/analytics only.
/// The engine treats all agents identically regardless of type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentType {
    /// Human player (input from keyboard/mouse/gamepad)
    Human,
    /// LLM-powered agent (Claude, GPT, etc.)
    LlmAgent,
    /// Neural network agent (trained policy)
    NeuralAgent,
    /// Scripted NPC (behavior tree / FSM)
    ScriptedNpc,
    /// System agent (game master, spawner, etc.)
    System,
}

/// The core agent trait. Every player — human or AI — implements this.
///
/// The engine calls these methods in order each tick:
/// 1. `observe()` — receive what you can see/hear
/// 2. `decide()` — choose actions based on observation
/// 3. Actions are validated against `constraints()`
/// 4. Valid actions are executed on the world
pub trait Agent: Send {
    /// Unique identifier
    fn id(&self) -> AgentId;

    /// What kind of agent (for analytics, not gameplay)
    fn agent_type(&self) -> AgentType;

    /// Receive observation for this tick.
    /// Called once per tick before decide().
    fn observe(&mut self, observation: Observation);

    /// Produce actions for this tick.
    /// May return empty vec (agent does nothing).
    /// For async agents (LLM), may return actions from a previous observation
    /// if the current decision isn't ready yet.
    fn decide(&mut self) -> Vec<Action>;

    /// Constraints that limit this agent's actions.
    /// Applied identically regardless of agent type.
    fn constraints(&self) -> &AgentConstraints;

    /// Mutable access to constraints (for status effects, etc.)
    fn constraints_mut(&mut self) -> &mut AgentConstraints;

    /// Called when the agent joins the game
    fn on_join(&mut self) {}

    /// Called when the agent leaves the game
    fn on_leave(&mut self) {}

    /// Called when the agent's controlled entity dies
    fn on_death(&mut self) {}

    /// Called when the agent's controlled entity respawns
    fn on_respawn(&mut self) {}
}

/// A slot in the world that an agent occupies
pub struct AgentSlot {
    pub agent: Box<dyn Agent>,
    pub entity_id: Option<hecs::Entity>,
    pub connected: bool,
    pub last_observation: Option<Observation>,
}

impl AgentSlot {
    pub fn new(agent: Box<dyn Agent>) -> Self {
        Self {
            agent,
            entity_id: None,
            connected: true,
            last_observation: None,
        }
    }
}

// ============================================================
// HUMAN AGENT — adapts input devices to the Agent trait
// ============================================================

/// Human player agent. Converts buffered input into actions.
pub struct HumanAgent {
    id: AgentId,
    constraints: AgentConstraints,
    input_buffer: Vec<Action>,
    last_observation: Option<Observation>,
}

impl HumanAgent {
    pub fn new() -> Self {
        Self {
            id: AgentId::new(),
            constraints: AgentConstraints::default(),
            input_buffer: Vec::new(),
            last_observation: None,
        }
    }

    /// Called by the input system when the human presses/releases keys.
    /// These are queued and drained on the next decide() call.
    pub fn push_input(&mut self, action: Action) {
        self.input_buffer.push(action);
    }
}

impl Agent for HumanAgent {
    fn id(&self) -> AgentId { self.id }
    fn agent_type(&self) -> AgentType { AgentType::Human }

    fn observe(&mut self, observation: Observation) {
        self.last_observation = Some(observation);
    }

    fn decide(&mut self) -> Vec<Action> {
        // Drain the input buffer — whatever the human pressed this tick
        std::mem::take(&mut self.input_buffer)
    }

    fn constraints(&self) -> &AgentConstraints { &self.constraints }
    fn constraints_mut(&mut self) -> &mut AgentConstraints { &mut self.constraints }
}

// ============================================================
// IDLE AGENT — placeholder, does nothing
// ============================================================

/// An agent that does nothing. Used for testing and as placeholder.
pub struct IdleAgent {
    id: AgentId,
    constraints: AgentConstraints,
}

impl IdleAgent {
    pub fn new() -> Self {
        Self {
            id: AgentId::new(),
            constraints: AgentConstraints::default(),
        }
    }
}

impl Agent for IdleAgent {
    fn id(&self) -> AgentId { self.id }
    fn agent_type(&self) -> AgentType { AgentType::ScriptedNpc }
    fn observe(&mut self, _observation: Observation) {}
    fn decide(&mut self) -> Vec<Action> { vec![Action::Idle] }
    fn constraints(&self) -> &AgentConstraints { &self.constraints }
    fn constraints_mut(&mut self) -> &mut AgentConstraints { &mut self.constraints }
}
