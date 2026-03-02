//! Prompt template system for formatting observations into LLM-consumable prompts.
//!
//! Provides pluggable templates that control how game state is serialized for LLM
//! decision-making. Different templates optimize for different goals:
//! - CompactTemplate: Minimal tokens (ACON-style compression)
//! - DetailedTemplate: Full structured text (maximum information)
//! - TacticalTemplate: Combat-focused with threat analysis
//! - JsonTemplate: Machine-readable JSON for structured output models

use pod_core::observation::{Observation, Relationship, VisibleEntity};
use std::collections::HashMap;
use std::fmt::Write;

const JSON_ACTION_INSTRUCTION: &str = r#"
Respond with a JSON object like this:
{
  "actions": ["move up", "attack", "speak hello"],
  "reasoning": "Brief explanation of your decision"
}

Valid action formats:
- "idle" — do nothing
- "move up|down|left|right" — move in cardinal direction
- "move up-left|up-right|down-left|down-right" — diagonal movement
- "stop" — halt movement
- "rotate 45" — face an angle (degrees)
- "look at (100, 200)" — look toward a position
- "attack" — attack in facing direction
- "speak message" — say something
- "interact" — interact with nearest object
- "drop" — drop item

List multiple actions if needed. Actions are processed in order each tick.
"#;

const TOON_ACTION_INSTRUCTION: &str = r#"
Respond with TOON blocks exactly like this (with 2-space-indented rows and explicit counts):
actions[N]{
  move up
  attack
}
reasoning[M]{
  Brief explanation for this tick.
}

Important:
- Actions must be 2-space indented rows inside each block.
- [N] and [M] must exactly match the number of rows in each block.
- Unknown actions are ignored and will default to Idle.
"#;

// ============================================================
// TEMPLATE TRAIT
// ============================================================

/// Core trait for observation → prompt formatting.
/// All templates produce a string suitable for LLM user_prompt.
pub trait PromptTemplate: Send + Sync {
    /// Format an observation into a prompt string
    fn format_observation(&self, obs: &Observation) -> String;

    /// Compatibility shim for existing call sites that use `.render()`.
    fn render(&self, obs: &Observation) -> String {
        self.format_observation(obs)
    }

    /// Wrap a base system prompt with template-specific instructions
    fn format_system_prompt(&self, base_prompt: &str, action_instructions: &str) -> String {
        format!("{}\n\n{}", base_prompt, action_instructions)
    }

    /// Instructions that define the expected action response format.
    fn action_instructions(&self) -> &'static str {
        JSON_ACTION_INSTRUCTION
    }

    /// Template identifier
    fn name(&self) -> &str;

    /// Estimated token cost of formatting this observation
    fn estimate_tokens(&self, obs: &Observation) -> u32 {
        let formatted = self.format_observation(obs);
        (formatted.len() as u32) / 4
    }
}

// ============================================================
// COMPACT TEMPLATE (ACON-style)
// ============================================================

/// Minimal token usage with salience-based entity filtering.
/// Optimized for fast, cheap LLM calls.
pub struct CompactTemplate {
    pub max_entities: usize,
    pub salience_threshold: f32,
}

impl Default for CompactTemplate {
    fn default() -> Self {
        Self {
            max_entities: 10,
            salience_threshold: 0.3,
        }
    }
}

impl CompactTemplate {
    fn entity_salience(entity: &VisibleEntity) -> f32 {
        let mut salience = 0.0;
        if matches!(entity.relationship, Relationship::Hostile) {
            salience += 0.6;
        } else if matches!(entity.relationship, Relationship::Friendly) {
            salience += 0.2;
        }
        salience += (1.0 - (entity.distance / 500.0).min(1.0)) * 0.3;
        if let Some(hp) = entity.health_fraction {
            if hp < 0.5 {
                salience += 0.1;
            }
        }
        salience.clamp(0.0, 1.0)
    }
}

impl PromptTemplate for CompactTemplate {
    fn format_observation(&self, obs: &Observation) -> String {
        let mut out = String::with_capacity(256);

        // Core state (single line)
        out.push_str(&format!(
            "T:{} P:{:.0},{:.0} HP:{}/{} F:{:.0}",
            obs.tick,
            obs.self_state.position.x,
            obs.self_state.position.y,
            obs.self_state.health.unwrap_or(0.0) as i32,
            obs.self_state.max_health.unwrap_or(0.0) as i32,
            obs.self_state.rotation.to_degrees()
        ));

        // Visible entities — sorted by salience, filtered
        if !obs.visible_entities.is_empty() {
            let mut sorted = obs.visible_entities.clone();
            sorted.sort_by(|a, b| {
                Self::entity_salience(b)
                    .partial_cmp(&Self::entity_salience(a))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            out.push_str("\nV:[");
            for (i, e) in sorted.iter().enumerate() {
                if i >= self.max_entities || Self::entity_salience(e) < self.salience_threshold {
                    break;
                }
                out.push_str(&format!(
                    "|{}d:{:.0}r:{:?}",
                    e.entity_type.chars().next().unwrap_or('?'),
                    e.distance,
                    e.relationship
                ));
                if let Some(hp) = e.health_fraction {
                    out.push_str(&format!("h:{:.0}", hp * 100.0));
                }
            }
            out.push(']');
        }

        // Audible events
        if !obs.audible_events.is_empty() {
            out.push_str("\nA:[");
            for (i, e) in obs.audible_events.iter().take(3).enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!(
                    "{}d:{:.0}",
                    e.event_type.chars().next().unwrap_or('?'),
                    e.distance
                ));
            }
            out.push(']');
        }

        // Objectives
        if !obs.objectives.is_empty() {
            out.push_str("\nO:[");
            for obj in obs.objectives.iter().take(2) {
                out.push_str(&format!(
                    "{}:{:.0}",
                    obj.id.chars().next().unwrap_or('?'),
                    obj.progress * 100.0
                ));
            }
            out.push(']');
        }

        out
    }

    fn name(&self) -> &str {
        "compact"
    }
}

// ============================================================
// DETAILED TEMPLATE
// ============================================================

/// Full structured text format. Maximum information, higher token cost.
/// Uses the same format as Observation::to_agent_prompt() but extensible.
pub struct DetailedTemplate;

impl PromptTemplate for DetailedTemplate {
    fn format_observation(&self, obs: &Observation) -> String {
        obs.to_agent_prompt()
    }

    fn name(&self) -> &str {
        "detailed"
    }
}

// ============================================================
// TACTICAL TEMPLATE
// ============================================================

/// Combat-focused template with threat analysis, positioning info, and tactical summary.
pub struct TacticalTemplate {
    pub threat_range: f32,
}

impl Default for TacticalTemplate {
    fn default() -> Self {
        Self { threat_range: 300.0 }
    }
}

impl PromptTemplate for TacticalTemplate {
    fn format_observation(&self, obs: &Observation) -> String {
        let mut out = String::with_capacity(512);

        // Status line
        let hp = obs.self_state.health.unwrap_or(0.0);
        let max_hp = obs.self_state.max_health.unwrap_or(100.0);
        let hp_pct = if max_hp > 0.0 { hp / max_hp } else { 0.0 };
        let status = if hp_pct > 0.7 {
            "HEALTHY"
        } else if hp_pct > 0.3 {
            "WOUNDED"
        } else {
            "CRITICAL"
        };

        out.push_str(&format!(
            "STATUS: {} ({:.0}%) | POS: ({:.0},{:.0}) | FACING: {:.0}°\n",
            status,
            hp_pct * 100.0,
            obs.self_state.position.x,
            obs.self_state.position.y,
            obs.self_state.rotation.to_degrees()
        ));

        // Threat assessment
        let hostiles: Vec<_> = obs
            .visible_entities
            .iter()
            .filter(|e| matches!(e.relationship, Relationship::Hostile))
            .collect();
        let friendlies: Vec<_> = obs
            .visible_entities
            .iter()
            .filter(|e| matches!(e.relationship, Relationship::Friendly))
            .collect();

        let immediate_threats: Vec<_> = hostiles
            .iter()
            .filter(|e| e.distance < self.threat_range)
            .collect();

        out.push_str(&format!(
            "THREATS: {} hostile ({} in range {:.0}) | {} friendly nearby\n",
            hostiles.len(),
            immediate_threats.len(),
            self.threat_range,
            friendlies.len()
        ));

        // Threat details (closest first)
        if !immediate_threats.is_empty() {
            out.push_str("DANGER:\n");
            let mut threats_sorted: Vec<_> = immediate_threats.clone();
            threats_sorted.sort_by(|a, b| {
                a.distance
                    .partial_cmp(&b.distance)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for t in threats_sorted.iter().take(5) {
                let hp_str = t
                    .health_fraction
                    .map(|h| format!(" hp:{:.0}%", h * 100.0))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "  {} dist:{:.0}{}\n",
                    t.entity_type, t.distance, hp_str
                ));
            }
        }

        // Cooldown status
        if !obs.self_state.cooldowns.is_empty() {
            out.push_str("COOLDOWNS: ");
            for cd in &obs.self_state.cooldowns {
                if cd.remaining_ticks > 0 {
                    out.push_str(&format!("{}:{} ", cd.name, cd.remaining_ticks));
                }
            }
            out.push('\n');
        }

        // Audio intel
        if !obs.audible_events.is_empty() {
            out.push_str("AUDIO: ");
            for e in obs.audible_events.iter().take(3) {
                out.push_str(&format!("{} @{:.0} ", e.event_type, e.distance));
            }
            out.push('\n');
        }

        // Messages
        for m in &obs.messages {
            out.push_str(&format!("COMMS: {}\n", m.content));
        }

        out
    }

    fn format_system_prompt(&self, base_prompt: &str, action_instructions: &str) -> String {
        format!(
            "{}\n\nYou are in a tactical combat scenario. Prioritize survival and efficient \
             threat elimination. Assess threats before acting. Retreat when health is critical.\n\n{}",
            base_prompt, action_instructions
        )
    }

    fn name(&self) -> &str {
        "tactical"
    }
}

// ============================================================
// JSON TEMPLATE
// ============================================================

/// Pure JSON observation format for models that prefer structured input.
pub struct JsonTemplate;

impl PromptTemplate for JsonTemplate {
    fn format_observation(&self, obs: &Observation) -> String {
        serde_json::to_string_pretty(obs).unwrap_or_else(|_| "{}".to_string())
    }

    fn name(&self) -> &str {
        "json"
    }
}

// ============================================================
// TOON TEMPLATE
// ============================================================

/// TOON-formatted template for strict block/row prompts and responses.
pub struct ToonTemplate;

impl ToonTemplate {
    fn push_section(output: &mut String, name: &str, rows: &[String]) {
        let _ = writeln!(output, "{}[{}]{{", name, rows.len());
        for row in rows {
            let _ = writeln!(output, "  {}", row);
        }
        output.push_str("}\n");
    }
}

impl PromptTemplate for ToonTemplate {
    fn format_observation(&self, obs: &Observation) -> String {
        let mut out = String::new();

        let mut self_rows = Vec::new();
        let hp = obs
            .self_state
            .health
            .map_or("unknown".to_string(), |h| format!("{:.0}", h));
        let max_hp = obs
            .self_state
            .max_health
            .map_or("unknown".to_string(), |h| format!("{:.0}", h));
        let rotation = obs.self_state.rotation.to_degrees();
        self_rows.push(format!(
            "tick={} elapsed={:.2} position={:.0},{:.0} rotation={:.0} health={}/{} velocity={:.1},{:.1} team={:?} entity_id={:?} agent_id={:?}",
            obs.tick,
            obs.elapsed_secs,
            obs.self_state.position.x,
            obs.self_state.position.y,
            rotation,
            hp,
            max_hp,
            obs.self_state.velocity.x,
            obs.self_state.velocity.y,
            obs.self_state.team,
            obs.self_state.entity_id.0,
            obs.self_state.agent_id
        ));
        self_rows.push(format!("cooldowns={}", obs.self_state.cooldowns.len()));
        let cooldown_rows: Vec<String> = obs
            .self_state
            .cooldowns
            .iter()
            .map(|cd| {
                format!(
                    "{} rem={}/{}",
                    cd.name, cd.remaining_ticks, cd.total_ticks
                )
            })
            .collect();

        let visible_rows: Vec<String> = obs
            .visible_entities
            .iter()
            .map(|e| {
                let hp = e
                    .health_fraction
                    .map(|v| format!("{:.0}%", v * 100.0))
                    .unwrap_or_else(|| "unknown".to_string());
                format!(
                    "{} id={} pos={:.0},{:.0} vel={:.1},{:.1} rot={:.0} relation={:?} distance={:.0} hp={}",
                    e.entity_type,
                    e.entity_id.0,
                    e.position.x,
                    e.position.y,
                    e.velocity.x,
                    e.velocity.y,
                    e.rotation,
                    e.relationship,
                    e.distance,
                    hp
                )
            })
            .collect();

        let available_action_rows: Vec<String> = obs
            .available_actions
            .iter()
            .enumerate()
            .map(|(idx, action)| format!("{}: {}", idx, action))
            .collect();

        let message_rows: Vec<String> = obs
            .messages
            .iter()
            .map(|message| {
                format!(
                    "from={:?} channel={:?} content={}",
                    message.from, message.channel, message.content
                )
            })
            .collect();

        let objective_rows: Vec<String> = obs
            .objectives
            .iter()
            .map(|obj| {
                format!(
                    "{} progress={:.0}% completed={} desc={}",
                    obj.id,
                    obj.progress * 100.0,
                    obj.completed,
                    obj.description
                )
            })
            .collect();

        let audio_rows: Vec<String> = obs
            .audible_events
            .iter()
            .map(|event| format!("{} at {:.0} ({:.0})", event.event_type, event.distance, event.intensity))
            .collect();

        Self::push_section(&mut out, "self", &self_rows);
        Self::push_section(&mut out, "cooldowns", &cooldown_rows);
        Self::push_section(&mut out, "visible_entities", &visible_rows);
        Self::push_section(&mut out, "available_actions", &available_action_rows);
        Self::push_section(&mut out, "messages", &message_rows);
        Self::push_section(&mut out, "objectives", &objective_rows);
        Self::push_section(&mut out, "audio", &audio_rows);
        out
    }

    fn format_system_prompt(&self, base_prompt: &str, action_instructions: &str) -> String {
        format!(
            "{}\n\nTOON-formatted observations are required for this model. Preserve strict section counts.\n{}",
            base_prompt, action_instructions
        )
    }

    fn action_instructions(&self) -> &'static str {
        TOON_ACTION_INSTRUCTION
    }

    fn name(&self) -> &str {
        "toon"
    }
}

// ============================================================
// TEMPLATE REGISTRY
// ============================================================

/// Registry of named templates for runtime selection.
pub struct TemplateRegistry {
    templates: HashMap<String, Box<dyn PromptTemplate>>,
    default: String,
}

impl TemplateRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            templates: HashMap::new(),
            default: "toon".to_string(),
        };
        registry.register(Box::new(CompactTemplate::default()));
        registry.register(Box::new(DetailedTemplate));
        registry.register(Box::new(TacticalTemplate::default()));
        registry.register(Box::new(JsonTemplate));
        registry.register(Box::new(ToonTemplate));
        registry
    }

    pub fn register(&mut self, template: Box<dyn PromptTemplate>) {
        self.templates.insert(template.name().to_string(), template);
    }

    pub fn get(&self, name: &str) -> Option<&dyn PromptTemplate> {
        self.templates.get(name).map(|t| t.as_ref())
    }

    pub fn default_template(&self) -> &dyn PromptTemplate {
        self.templates
            .get(&self.default)
            .map(|t| t.as_ref())
            .expect("default template must exist")
    }

    pub fn set_default(&mut self, name: &str) {
        if self.templates.contains_key(name) {
            self.default = name.to_string();
        }
    }

    pub fn list(&self) -> Vec<&str> {
        self.templates.keys().map(|k| k.as_str()).collect()
    }

    /// Backward-compatible alias kept for existing callers/tests.
    pub fn available_templates(&self) -> Vec<&str> {
        self.list()
    }
}

impl Default for TemplateRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::observation::*;
    use glam::Vec2;

    fn test_observation() -> Observation {
        Observation {
            tick: 100,
            elapsed_secs: 1.67,
            self_state: SelfState {
                position: Vec2::new(150.0, 200.0),
                rotation: 1.57,
                health: Some(80.0),
                max_health: Some(100.0),
                ..Default::default()
            },
            visible_entities: vec![
                VisibleEntity {
                    entity_id: pod_core::id::EntityId(1),
                    entity_type: "warrior".to_string(),
                    position: Vec2::new(200.0, 220.0),
                    velocity: Vec2::ZERO,
                    rotation: 0.0,
                    distance: 55.0,
                    relationship: Relationship::Hostile,
                    health_fraction: Some(0.6),
                },
                VisibleEntity {
                    entity_id: pod_core::id::EntityId(2),
                    entity_type: "healer".to_string(),
                    position: Vec2::new(130.0, 180.0),
                    velocity: Vec2::ZERO,
                    rotation: 0.0,
                    distance: 25.0,
                    relationship: Relationship::Friendly,
                    health_fraction: Some(0.9),
                },
            ],
            audible_events: vec![AudibleEvent {
                event_type: "explosion".to_string(),
                direction: Vec2::new(1.0, 0.0),
                distance: 150.0,
                intensity: 0.8,
            }],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![Objective {
                id: "kill".to_string(),
                description: "Eliminate all hostiles".to_string(),
                progress: 0.3,
                completed: false,
            }],
        }
    }

    #[test]
    fn test_compact_template() {
        let template = CompactTemplate::default();
        let obs = test_observation();
        let result = template.format_observation(&obs);
        assert!(result.contains("T:100"));
        assert!(result.contains("P:150,200"));
        assert!(result.len() < 300); // compact should be short
    }

    #[test]
    fn test_detailed_template() {
        let template = DetailedTemplate;
        let obs = test_observation();
        let result = template.format_observation(&obs);
        assert!(result.contains("TICK: 100"));
        assert!(result.contains("VISIBLE:"));
    }

    #[test]
    fn test_tactical_template() {
        let template = TacticalTemplate::default();
        let obs = test_observation();
        let result = template.format_observation(&obs);
        assert!(result.contains("STATUS: HEALTHY"));
        assert!(result.contains("THREATS:"));
        assert!(result.contains("1 hostile"));
    }

    #[test]
    fn test_json_template() {
        let template = JsonTemplate;
        let obs = test_observation();
        let result = template.format_observation(&obs);
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["tick"], 100);
    }

    #[test]
    fn test_registry() {
        let registry = TemplateRegistry::new();
        assert!(registry.list().len() >= 5);
        assert!(registry.get("compact").is_some());
        assert!(registry.get("tactical").is_some());
        assert!(registry.get("toon").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_toon_template() {
        let template = ToonTemplate;
        let obs = test_observation();
        let result = template.format_observation(&obs);
        assert!(result.contains("self["));
        assert!(result.contains("visible_entities"));
        assert!(result.contains("objectives"));
    }

    #[test]
    fn test_token_estimation() {
        let template = CompactTemplate::default();
        let obs = test_observation();
        let tokens = template.estimate_tokens(&obs);
        assert!(tokens > 0);
        assert!(tokens < 200); // compact should be efficient
    }
}
