use glam::Vec2;
use serde::{Deserialize, Serialize};
use crate::id::{AgentId, EntityId};
use crate::component::Team;

/// Complete observation delivered to an agent each tick.
/// This is the SAME structure for human and AI agents.
///
/// For humans: this drives what's rendered on screen.
/// For AI: this is serialized to JSON and sent to the decision backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// Current tick number
    pub tick: u64,
    /// Time elapsed since game start
    pub elapsed_secs: f32,

    /// The agent's own state
    pub self_state: SelfState,

    /// Entities visible to this agent (filtered by perception)
    pub visible_entities: Vec<VisibleEntity>,

    /// Events audible to this agent
    pub audible_events: Vec<AudibleEvent>,

    /// Messages received this tick
    pub messages: Vec<AgentMessage>,

    /// Available actions this tick (after constraint filtering)
    pub available_actions: Vec<String>,

    /// Game-specific objectives
    pub objectives: Vec<Objective>,
}

/// What the agent knows about itself (always full info)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfState {
    pub agent_id: AgentId,
    pub entity_id: EntityId,
    pub position: Vec2,
    pub rotation: f32,
    pub velocity: Vec2,
    pub health: Option<f32>,
    pub max_health: Option<f32>,
    pub team: Team,
    pub cooldowns: Vec<CooldownState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooldownState {
    pub name: String,
    pub remaining_ticks: u32,
    pub total_ticks: u32,
}

/// An entity visible to the observing agent.
/// Information is limited by what the agent can actually see.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisibleEntity {
    pub entity_id: EntityId,
    pub entity_type: String,
    pub position: Vec2,
    pub velocity: Vec2,
    pub rotation: f32,
    pub distance: f32,
    pub relationship: Relationship,
    /// Only visible if the entity is close enough or has visible health bar
    pub health_fraction: Option<f32>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Relationship {
    Friendly,
    Hostile,
    Neutral,
    Unknown,
}

/// An event the agent can hear (explosion, footstep, speech)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudibleEvent {
    pub event_type: String,
    /// Approximate direction (agents can't pinpoint sounds perfectly)
    pub direction: Vec2,
    /// Approximate distance
    pub distance: f32,
    /// Intensity (louder = more accurate direction)
    pub intensity: f32,
}

/// Message from another agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub from: AgentId,
    pub content: String,
    pub channel: MessageChannel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageChannel {
    Proximity, // heard by nearby agents
    Team,      // team-only
    Direct,    // whisper to specific agent
    Global,    // everyone
}

/// Game objective
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Objective {
    pub id: String,
    pub description: String,
    pub progress: f32, // 0.0 to 1.0
    pub completed: bool,
}

impl Observation {
    /// Serialize to a compact format for AI agent consumption
    pub fn to_agent_prompt(&self) -> String {
        // Structured text that an LLM can reason about
        let mut prompt = String::new();

        prompt.push_str(&format!(
            "TICK: {} | POS: ({:.0}, {:.0}) | HP: {}/{} | FACING: {:.0}°\n",
            self.tick,
            self.self_state.position.x,
            self.self_state.position.y,
            self.self_state.health.unwrap_or(0.0) as i32,
            self.self_state.max_health.unwrap_or(0.0) as i32,
            self.self_state.rotation.to_degrees(),
        ));

        if !self.visible_entities.is_empty() {
            prompt.push_str("VISIBLE:\n");
            for e in &self.visible_entities {
                prompt.push_str(&format!(
                    "  {} [{}] dist={:.0} rel={:?}",
                    e.entity_type, e.entity_id.0, e.distance, e.relationship
                ));
                if let Some(hp) = e.health_fraction {
                    prompt.push_str(&format!(" hp={:.0}%", hp * 100.0));
                }
                prompt.push('\n');
            }
        }

        if !self.audible_events.is_empty() {
            prompt.push_str("HEARD:\n");
            for e in &self.audible_events {
                prompt.push_str(&format!(
                    "  {} dir=({:.1},{:.1}) dist≈{:.0}\n",
                    e.event_type, e.direction.x, e.direction.y, e.distance
                ));
            }
        }

        if !self.messages.is_empty() {
            prompt.push_str("MESSAGES:\n");
            for m in &self.messages {
                prompt.push_str(&format!("  {}: {}\n", m.from, m.content));
            }
        }

        if !self.objectives.is_empty() {
            prompt.push_str("OBJECTIVES:\n");
            for o in &self.objectives {
                let status = if o.completed { "✓" } else { &format!("{:.0}%", o.progress * 100.0) };
                prompt.push_str(&format!("  {} [{}]\n", o.description, status));
            }
        }

        prompt
    }
}
