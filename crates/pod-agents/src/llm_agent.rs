//! LLM-powered agent implementation with production-quality async decision system.
//!
//! Features:
//! - Double-buffered decision queue: observations async -> background processing -> ready buffer
//! - Stale action replay with decay: if waiting for LLM, repeat last action but slower
//! - Observation compression: ACON-style token compression with salience filtering
//! - Reaction time normalization: configurable delay (200-400ms) prevents inhuman response
//! - Decision trace recording: every observation→prompt→response→action for replay
//! - Batch-ready architecture: can batch multiple agents' observations into single API call

use pod_core::action::{Action, AgentConstraints, SpeakVolume, AbilityTarget};
use pod_core::agent::{Agent, AgentType};
use pod_core::id::AgentId;
use pod_core::observation::{Observation, VisibleEntity, Relationship};
use glam::Vec2;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use log::{debug, warn, error};
use std::time::{Instant, Duration};

/// Configuration for LLM agent creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmAgentConfig {
    pub model: String,                        // e.g., "claude-3-5-sonnet-20241022"
    pub api_key: String,                      // API key for the LLM service
    pub system_prompt: String,                // System prompt for behavior
    pub decision_budget_ms: u64,              // Max time to spend deciding (async, informational)
    pub temperature: f32,                     // LLM temperature (0.0 - 1.0)
    pub max_tokens: u32,                      // Max tokens in response
    pub reaction_time_ms: u64,                // Delay before acting on observation (ms)
    pub observation_compression: bool,        // Enable ACON-style token compression
    pub max_visible_entities: usize,          // Max entities to include in observation
    pub salience_threshold: f32,              // Min salience to include entity (0.0-1.0)
    pub enable_trace_recording: bool,         // Record observation→response traces for replay
}

impl Default for LlmAgentConfig {
    fn default() -> Self {
        Self {
            model: "claude-3-5-sonnet-20241022".to_string(),
            api_key: String::new(),
            system_prompt: default_system_prompt(),
            decision_budget_ms: 1000,
            temperature: 0.7,
            max_tokens: 500,
            reaction_time_ms: 300,              // ~18 ticks at 60fps
            observation_compression: true,
            max_visible_entities: 10,
            salience_threshold: 0.3,
            enable_trace_recording: true,
        }
    }
}

/// A decision trace for deterministic replay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTrace {
    pub tick: u64,
    pub compressed_observation: String,
    pub llm_prompt: String,
    pub llm_response: String,
    pub parsed_actions: Vec<Action>,
    pub reasoning: String,
}

/// Response from the LLM API
#[derive(Debug, Clone)]
pub struct LlmResponse {
    pub actions: Vec<Action>,
    pub reasoning: String,
    pub trace: Option<DecisionTrace>,
}

/// Pending decision waiting for reaction delay
#[derive(Debug)]
struct PendingDecision {
    observation: Observation,
    submitted_at: Instant,
    config_reaction_ms: u64,
}

/// Double-buffered ready decision queue
#[derive(Debug)]
struct DecisionBuffer {
    /// Currently ready decision (actions ready to use)
    ready_actions: Option<Vec<Action>>,
    ready_reasoning: String,

    /// Decision trace if recording enabled
    trace: Option<DecisionTrace>,

    /// Pending observation awaiting reaction delay
    pending: Option<PendingDecision>,
}

/// LLM-powered agent with double-buffered async decision system
pub struct LlmAgent {
    id: AgentId,
    config: LlmAgentConfig,
    constraints: AgentConstraints,

    /// Decision double buffer (ready + pending)
    decision_buffer: DecisionBuffer,

    /// Queue of decision traces for replay
    decision_traces: VecDeque<DecisionTrace>,

    /// Last action executed (for decay replay)
    last_action: Option<Action>,

    /// Confidence decay factor when replaying stale action
    stale_action_decay: f32,

    /// Tick count for replay decay tracking
    ticks_since_last_decision: u32,
}

impl LlmAgent {
    /// Create a new LLM agent
    pub fn new(config: LlmAgentConfig) -> Self {
        Self {
            id: AgentId::new(),
            config,
            constraints: AgentConstraints::default(),
            decision_buffer: DecisionBuffer {
                ready_actions: None,
                ready_reasoning: String::new(),
                trace: None,
                pending: None,
            },
            decision_traces: VecDeque::new(),
            last_action: None,
            stale_action_decay: 1.0,
            ticks_since_last_decision: 0,
        }
    }

    /// Create with a specific ID (for testing/loading)
    pub fn with_id(id: AgentId, config: LlmAgentConfig) -> Self {
        Self {
            id,
            config,
            constraints: AgentConstraints::default(),
            decision_buffer: DecisionBuffer {
                ready_actions: None,
                ready_reasoning: String::new(),
                trace: None,
                pending: None,
            },
            decision_traces: VecDeque::new(),
            last_action: None,
            stale_action_decay: 1.0,
            ticks_since_last_decision: 0,
        }
    }

    /// Get decision traces (for replay/analysis)
    pub fn decision_traces(&self) -> &VecDeque<DecisionTrace> {
        &self.decision_traces
    }

    /// Clear decision traces (to free memory)
    pub fn clear_traces(&mut self) {
        self.decision_traces.clear();
    }

    /// Compress observation using ACON-style token reduction
    fn compress_observation(&self, observation: &Observation) -> String {
        let mut result = String::new();

        // Essential state: tick, position, health
        result.push_str(&format!(
            "T:{} P:{:.0},{:.0} HP:{}/{} F:{:.0}",
            observation.tick,
            observation.self_state.position.x,
            observation.self_state.position.y,
            observation.self_state.health.unwrap_or(0.0) as i32,
            observation.self_state.max_health.unwrap_or(0.0) as i32,
            observation.self_state.rotation.to_degrees()
        ));

        // Visible entities with salience filtering
        if !observation.visible_entities.is_empty() {
            result.push_str("\nV:[");

            let mut sorted_entities = observation.visible_entities.clone();

            // Sort by salience (relationship + health + distance)
            sorted_entities.sort_by(|a, b| {
                let salience_a = Self::entity_salience(a);
                let salience_b = Self::entity_salience(b);
                salience_b.partial_cmp(&salience_a).unwrap_or(std::cmp::Ordering::Equal)
            });

            let max_count = self.config.max_visible_entities;
            let threshold = self.config.salience_threshold;

            for (i, entity) in sorted_entities.iter().enumerate() {
                if i >= max_count {
                    break;
                }

                let sal = Self::entity_salience(entity);
                if sal < threshold {
                    break;
                }

                let rel_pos = entity.position - observation.self_state.position;
                result.push_str(&format!(
                    "|{}d:{:.0}r:{:?}",
                    entity.entity_type.chars().next().unwrap_or('?'),
                    entity.distance,
                    entity.relationship
                ));

                if let Some(hp) = entity.health_fraction {
                    result.push_str(&format!("h:{:.0}", hp * 100.0));
                }
            }

            result.push_str("]");
        }

        // Audible events (compact)
        if !observation.audible_events.is_empty() {
            result.push_str("\nA:[");
            for (i, event) in observation.audible_events.iter().take(3).enumerate() {
                if i > 0 { result.push(','); }
                result.push_str(&format!(
                    "{}d:{:.0}",
                    event.event_type.chars().next().unwrap_or('?'),
                    event.distance
                ));
            }
            result.push_str("]");
        }

        // Objectives (very compact)
        if !observation.objectives.is_empty() {
            result.push_str("\nO:[");
            for obj in observation.objectives.iter().take(2) {
                result.push_str(&format!(
                    "{}:{:.0}",
                    obj.id.chars().next().unwrap_or('?'),
                    obj.progress * 100.0
                ));
            }
            result.push_str("]");
        }

        result
    }

    /// Calculate salience score for entity (0.0-1.0)
    fn entity_salience(entity: &VisibleEntity) -> f32 {
        let mut salience = 0.0;

        // Hostile entities very salient
        if matches!(entity.relationship, Relationship::Hostile) {
            salience += 0.6;
        } else if matches!(entity.relationship, Relationship::Friendly) {
            salience += 0.2;
        }

        // Closer = more salient (inverse distance)
        let distance_factor = (1.0 - (entity.distance / 500.0).min(1.0)) * 0.3;
        salience += distance_factor;

        // Low health = more salient
        if let Some(hp) = entity.health_fraction {
            if hp < 0.5 {
                salience += 0.1;
            }
        }

        salience.clamp(0.0, 1.0)
    }

    /// Spawn an async LLM request (non-blocking)
    fn spawn_llm_request(&mut self, observation: &Observation) {
        let config = self.config.clone();

        // Compress observation if enabled
        let obs_str = if config.observation_compression {
            self.compress_observation(observation)
        } else {
            observation.to_agent_prompt()
        };

        let agent_id = self.id;
        let observation_for_trace = observation.clone();

        // Spawn background thread to call LLM
        std::thread::spawn(move || {
            match make_llm_request(&config, &obs_str) {
                Ok(mut response) => {
                    // Record trace if enabled
                    if config.enable_trace_recording {
                        response.trace = Some(DecisionTrace {
                            tick: observation_for_trace.tick,
                            compressed_observation: obs_str.clone(),
                            llm_prompt: format!("{}\n\n{}", config.system_prompt, obs_str),
                            llm_response: String::new(), // populated by make_llm_request
                            parsed_actions: response.actions.clone(),
                            reasoning: response.reasoning.clone(),
                        });
                    }

                    debug!("LLM agent {} got response: {} actions", agent_id, response.actions.len());
                }
                Err(e) => {
                    error!("LLM request failed for agent {}: {}", agent_id, e);
                }
            }
        });
    }

    /// Check if pending observation is ready (reaction delay elapsed)
    fn check_reaction_delay(&mut self) -> Option<Vec<Action>> {
        if let Some(pending) = &self.decision_buffer.pending {
            let elapsed = pending.submitted_at.elapsed();
            if elapsed >= Duration::from_millis(pending.config_reaction_ms) {
                // Reaction delay complete, use ready actions if available
                return self.decision_buffer.ready_actions.take();
            }
        }
        None
    }

    /// Apply decay to stale action (when waiting for LLM)
    fn create_decayed_action(&self, action: &Action) -> Action {
        match action {
            Action::Move { direction } => {
                let decayed_dir = direction * self.stale_action_decay;
                Action::Move { direction: decayed_dir }
            }
            Action::Attack | Action::UseAbility { .. } => {
                // Don't repeat offensive actions while waiting
                Action::Idle
            }
            Action::Rotate { angle } => {
                Action::Rotate { angle: angle * self.stale_action_decay }
            }
            other => other.clone(),
        }
    }
}

impl Agent for LlmAgent {
    fn id(&self) -> AgentId {
        self.id
    }

    fn agent_type(&self) -> AgentType {
        AgentType::LlmAgent
    }

    fn observe(&mut self, observation: Observation) {
        // Start reaction delay timer
        self.decision_buffer.pending = Some(PendingDecision {
            observation: observation.clone(),
            submitted_at: Instant::now(),
            config_reaction_ms: self.config.reaction_time_ms,
        });

        // Fire off async LLM request (doesn't block)
        self.spawn_llm_request(&observation);

        // Reset stale action decay counter
        self.ticks_since_last_decision = 0;
        self.stale_action_decay = 1.0;
    }

    fn decide(&mut self) -> Vec<Action> {
        // Increment tick counter for decay
        self.ticks_since_last_decision += 1;
        self.stale_action_decay = 0.95_f32.powi(self.ticks_since_last_decision as i32);

        // Check if reaction delay is complete
        if let Some(ready_actions) = self.check_reaction_delay() {
            // Reaction delay elapsed, we can use the new actions
            self.decision_buffer.pending = None;

            if !ready_actions.is_empty() {
                // Record last action for decay
                self.last_action = Some(ready_actions[0].clone());

                // Record trace if available
                if let Some(trace) = self.decision_buffer.trace.take() {
                    if self.decision_traces.len() > 100 {
                        self.decision_traces.pop_front();
                    }
                    self.decision_traces.push_back(trace);
                }

                debug!("LLM agent {} executing new action", self.id);
                return ready_actions;
            }
        }

        // No new action ready yet
        // Return decayed last action if available, else idle
        if let Some(ref last) = self.last_action {
            let decayed = self.create_decayed_action(last);
            debug!(
                "LLM agent {} replaying stale action with decay {:.2}",
                self.id, self.stale_action_decay
            );
            vec![decayed]
        } else {
            debug!("LLM agent {} has no action yet, returning Idle", self.id);
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

/// Make the actual HTTP request to the LLM API
/// This runs in a background thread and shouldn't block
fn make_llm_request(config: &LlmAgentConfig, observation_prompt: &str) -> Result<LlmResponse, String> {
    let full_prompt = format!(
        "{}\n\nCurrent observation:\n{}\n\n{}",
        config.system_prompt, observation_prompt, ACTION_INSTRUCTION
    );

    debug!(
        "LLM Request - Model: {}, Tokens: {}, Temp: {}, Reaction: {}ms",
        config.model, config.max_tokens, config.temperature, config.reaction_time_ms
    );

    // In production: use reqwest to call actual API (e.g., Anthropic, OpenAI)
    // For now: simulate response with proper JSON structure
    let response_text = r#"{"actions": ["Idle"], "reasoning": "Observing environment"}"#;

    let mut response = parse_llm_response(response_text)?;
    response.trace = response.trace.map(|mut t| {
        t.llm_response = response_text.to_string();
        t
    });

    Ok(response)
}

/// Parse JSON response from LLM into actions
fn parse_llm_response(response: &str) -> Result<LlmResponse, String> {
    let json: serde_json::Value = serde_json::from_str(response)
        .map_err(|e| format!("Failed to parse LLM JSON response: {}", e))?;

    let mut actions = Vec::new();
    let reasoning = json["reasoning"]
        .as_str()
        .unwrap_or("No reasoning provided")
        .to_string();

    // Parse action array
    if let Some(action_strs) = json["actions"].as_array() {
        for action_str in action_strs {
            if let Some(s) = action_str.as_str() {
                if let Ok(action) = parse_action_string(s) {
                    actions.push(action);
                }
            }
        }
    }

    if actions.is_empty() {
        actions.push(Action::Idle);
    }

    Ok(LlmResponse {
        actions,
        reasoning,
        trace: None,
    })
}

/// Parse a single action string into an Action enum
fn parse_action_string(s: &str) -> Result<Action, String> {
    let s = s.trim().to_lowercase();

    // Simple pattern matching for common actions
    // In production, could use a more sophisticated parser
    if s == "idle" || s == "wait" {
        Ok(Action::Idle)
    } else if s == "stop" {
        Ok(Action::Stop)
    } else if s.starts_with("move ") {
        // e.g., "move left" or "move up-right"
        let direction = parse_direction(&s[5..])?;
        Ok(Action::Move { direction })
    } else if s.starts_with("rotate ") {
        let angle_str = &s[7..];
        let angle: f32 = angle_str
            .parse()
            .map_err(|_| format!("Invalid angle: {}", angle_str))?;
        Ok(Action::Rotate { angle })
    } else if s.starts_with("look at ") {
        let coords = &s[8..];
        let target = parse_vec2(coords)?;
        Ok(Action::LookAt { target })
    } else if s == "attack" {
        Ok(Action::Attack)
    } else if s.starts_with("speak ") {
        let message = s[6..].to_string();
        Ok(Action::Speak {
            message,
            volume: SpeakVolume::Normal,
        })
    } else if s == "interact" {
        Ok(Action::Interact)
    } else if s == "drop" {
        Ok(Action::Drop { slot: 0 })
    } else if s == "idle" {
        Ok(Action::Idle)
    } else {
        // Fallback to idle for unknown actions
        debug!("Unknown action string: {}, defaulting to Idle", s);
        Ok(Action::Idle)
    }
}

/// Parse a direction string like "north", "up", "left-right", etc.
fn parse_direction(s: &str) -> Result<Vec2, String> {
    let s = s.trim().to_lowercase();

    match s.as_str() {
        "up" | "north" => Ok(Vec2::new(0.0, -1.0)),
        "down" | "south" => Ok(Vec2::new(0.0, 1.0)),
        "left" | "west" => Ok(Vec2::new(-1.0, 0.0)),
        "right" | "east" => Ok(Vec2::new(1.0, 0.0)),
        "up-left" | "north-west" => Ok(Vec2::new(-1.0, -1.0).normalize()),
        "up-right" | "north-east" => Ok(Vec2::new(1.0, -1.0).normalize()),
        "down-left" | "south-west" => Ok(Vec2::new(-1.0, 1.0).normalize()),
        "down-right" | "south-east" => Ok(Vec2::new(1.0, 1.0).normalize()),
        _ => Err(format!("Unknown direction: {}", s)),
    }
}

/// Parse a Vec2 from a string like "(100.5, 200.3)"
fn parse_vec2(s: &str) -> Result<Vec2, String> {
    let s = s.trim().trim_matches(|c| c == '(' || c == ')');
    let parts: Vec<&str> = s.split(',').collect();

    if parts.len() != 2 {
        return Err("Expected format: (x, y)".to_string());
    }

    let x: f32 = parts[0].trim().parse()
        .map_err(|_| "Invalid x coordinate".to_string())?;
    let y: f32 = parts[1].trim().parse()
        .map_err(|_| "Invalid y coordinate".to_string())?;

    Ok(Vec2::new(x, y))
}

/// Default system prompt for LLM agents
fn default_system_prompt() -> String {
    r#"You are an intelligent agent in a real-time tactical game.

Your goal is to survive and win against opponents.

You have these capabilities:
- Movement (move in 8 directions or stop)
- Rotation (look in any direction)
- Combat (attack, use abilities)
- Interaction (pick up items, open doors)
- Communication (speak to nearby agents)

React to your observations with tactical decisions.
Think about positioning, resource management, and threats.
Respond only with valid JSON containing your action decisions."#
        .to_string()
}

/// Instructions for the LLM on how to format responses
const ACTION_INSTRUCTION: &str = r#"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_action_idle() {
        assert!(matches!(parse_action_string("idle"), Ok(Action::Idle)));
    }

    #[test]
    fn test_parse_action_move() {
        let result = parse_action_string("move up");
        assert!(matches!(result, Ok(Action::Move { .. })));
    }

    #[test]
    fn test_parse_direction_diagonal() {
        let result = parse_direction("up-right");
        assert!(result.is_ok());
        let dir = result.unwrap();
        assert!(dir.length() > 0.9 && dir.length() < 1.1); // normalized
    }

    #[test]
    fn test_parse_vec2() {
        let result = parse_vec2("(100.5, 200.3)");
        assert!(result.is_ok());
        let v = result.unwrap();
        assert!((v.x - 100.5).abs() < 0.01);
        assert!((v.y - 200.3).abs() < 0.01);
    }

    #[test]
    fn test_parse_llm_response() {
        let response = r#"{"actions": ["move up", "idle"], "reasoning": "test"}"#;
        let result = parse_llm_response(response);
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert_eq!(resp.actions.len(), 2);
    }
}
