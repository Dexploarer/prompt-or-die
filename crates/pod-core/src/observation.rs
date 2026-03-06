use crate::component::{
    CombatLoadout, CombatStyle, CompanionRoster, CreatureIdentity, EncounterState, Inventory,
    SkillProgress, Team,
};
use crate::contract::AgentRuntimeProfile;
use crate::id::{AgentId, EntityId};
use glam::Vec2;
use serde::{Deserialize, Serialize};

/// Complete observation delivered to an agent each tick.
/// This is the SAME structure for human and AI agents.
///
/// For humans: this drives what's rendered on screen.
/// For AI: this is serialized to JSON and sent to the decision backend.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
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
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SelfState {
    pub agent_id: AgentId,
    pub entity_id: EntityId,
    pub runtime_profile: AgentRuntimeProfile,
    pub position: Vec2,
    pub rotation: f32,
    pub velocity: Vec2,
    pub health: Option<f32>,
    pub max_health: Option<f32>,
    pub team: Team,
    pub cooldowns: Vec<CooldownState>,
    pub combat_loadout: Option<CombatLoadout>,
    pub skills: Vec<SkillProgress>,
    pub inventory: Option<Inventory>,
    pub companion_roster: Option<CompanionRoster>,
    pub encounter: Option<EncounterState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooldownState {
    pub name: String,
    pub remaining_ticks: u32,
    pub total_ticks: u32,
}

/// An entity visible to the observing agent.
/// Information is limited by what the agent can actually see.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    pub combat_style: Option<CombatStyle>,
    pub creature: Option<CreatureIdentity>,
}

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize)]
pub enum Relationship {
    Friendly,
    Hostile,
    #[default]
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

        prompt.push_str(&format!(
            "PROFILE: role={:?} type={:?} team={:?}\n",
            self.self_state.runtime_profile.role,
            self.self_state.runtime_profile.agent_type,
            self.self_state.team,
        ));

        if let Some(loadout) = &self.self_state.combat_loadout {
            prompt.push_str(&format!(
                "COMBAT: {:?} range={:.0} speed={} max_hit={:.0} auto_retaliate={}\n",
                loadout.style,
                loadout.attack_range,
                loadout.attack_speed_ticks,
                loadout.max_hit,
                loadout.auto_retaliate,
            ));
        }

        if !self.self_state.skills.is_empty() {
            let summary = self
                .self_state
                .skills
                .iter()
                .take(5)
                .map(|skill| format!("{:?}:{}", skill.kind, skill.level))
                .collect::<Vec<_>>()
                .join(", ");
            prompt.push_str(&format!("SKILLS: {summary}\n"));
        }

        if let Some(inventory) = &self.self_state.inventory {
            prompt.push_str(&format!(
                "INVENTORY: slots={}/{} coins={} weight={:.1}\n",
                inventory.items.len(),
                inventory.capacity,
                inventory.coins,
                inventory.carried_weight,
            ));
        }

        if let Some(roster) = &self.self_state.companion_roster {
            let active = roster
                .active_slot
                .map(|slot| slot.to_string())
                .unwrap_or_else(|| "none".to_string());
            prompt.push_str(&format!(
                "COMPANIONS: active={} party={}/{}\n",
                active,
                roster.creatures.len(),
                roster.party_capacity,
            ));
        }

        if let Some(encounter) = &self.self_state.encounter {
            prompt.push_str(&format!(
                "ENCOUNTER: {:?} threat={:.1} capture_allowed={} in_combat={}\n",
                encounter.kind,
                encounter.threat_level,
                encounter.capture_allowed,
                encounter.in_combat,
            ));
        }

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
                if let Some(style) = e.combat_style {
                    prompt.push_str(&format!(" style={style:?}"));
                }
                if let Some(creature) = &e.creature {
                    prompt.push_str(&format!(
                        " creature={} lvl={}",
                        creature.species_name, creature.level
                    ));
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
                let status = if o.completed {
                    "✓"
                } else {
                    &format!("{:.0}%", o.progress * 100.0)
                };
                prompt.push_str(&format!("  {} [{}]\n", o.description, status));
            }
        }

        prompt
    }
}
