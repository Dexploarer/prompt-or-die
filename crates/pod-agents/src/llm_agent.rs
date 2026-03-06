//! LLM-powered agent implementation with production-quality async decision system.
//!
//! Features:
//! - Double-buffered decision queue: observe → background LLM call → ready buffer
//! - Stale action replay with decay: if waiting for LLM, repeat last action but slower
//! - Pluggable PromptTemplate for observation formatting (Compact, Detailed, Tactical, JSON)
//! - Pluggable ActionParser for structured response parsing (JSON, KeyValue, Fallback chain)
//! - ConversationMemory: sliding window of past decisions included in prompts
//! - TokenBudget: per-tick and per-minute token tracking with enforcement
//! - Reaction time normalization: configurable delay (200-400ms) prevents inhuman response
//! - Decision trace recording: every observation→prompt→response→action for replay

use pod_core::action::{Action, AgentConstraints};
use pod_core::agent::{Agent, AgentType};
use pod_core::id::AgentId;
use pod_core::observation::Observation;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};
use log::{debug, warn, error};

use crate::llm_provider::{
    CompletionRequest, LlmProvider, MockProvider, TokenBudget, TokenUsage,
};
use crate::prompt_template::{PromptTemplate, ToonTemplate};
#[cfg(test)]
use crate::prompt_template::CompactTemplate;
use crate::action_parser::{ActionParser, FallbackParser};
use crate::conversation_memory::{ConversationMemory, MemoryConfig};

// ============================================================
// CONFIGURATION
// ============================================================

/// Configuration for LLM agent creation.
///
/// Observation formatting is handled by `PromptTemplate`, and response parsing
/// by `ActionParser` — both pluggable at construction time. This config holds
/// only the LLM call parameters and agent behavior settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmAgentConfig {
    pub model: String,
    pub system_prompt: String,
    pub temperature: f32,
    pub max_tokens: u32,
    pub reaction_time_ms: u64,
    pub enable_trace_recording: bool,
}

impl Default for LlmAgentConfig {
    fn default() -> Self {
        Self {
            model: "claude-3-5-sonnet-20241022".to_string(),
            system_prompt: default_system_prompt(),
            temperature: 0.7,
            max_tokens: 500,
            reaction_time_ms: 300, // ~18 ticks at 60fps
            enable_trace_recording: true,
        }
    }
}

// ============================================================
// DECISION TRACE
// ============================================================

/// A decision trace for deterministic replay and debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTrace {
    pub tick: u64,
    pub compressed_observation: String,
    pub llm_prompt: String,
    pub llm_response: String,
    pub parsed_actions: Vec<Action>,
    pub reasoning: String,
}

// ============================================================
// INTERNAL TYPES
// ============================================================

/// Response from a background LLM call, sent back via mpsc channel.
#[derive(Debug, Clone)]
struct LlmResult {
    actions: Vec<Action>,
    reasoning: String,
    #[allow(dead_code)]
    raw_response: String,
    usage: Option<TokenUsage>,
    trace: Option<DecisionTrace>,
}

/// Pending decision waiting for reaction delay to elapse.
#[derive(Debug)]
struct PendingDecision {
    #[allow(dead_code)]
    observation: Observation,
    submitted_at: Instant,
    config_reaction_ms: u64,
}

/// Double-buffered ready decision queue.
#[derive(Debug)]
struct DecisionBuffer {
    ready_actions: Option<Vec<Action>>,
    ready_reasoning: String,
    trace: Option<DecisionTrace>,
    pending: Option<PendingDecision>,
}

// ============================================================
// LLM AGENT
// ============================================================

/// LLM-powered agent with double-buffered async decision system.
///
/// Architecture:
/// 1. `observe()` formats the observation via `PromptTemplate`, appends memory,
///    checks `TokenBudget`, then spawns a background thread using `LlmProvider`.
/// 2. The background thread sends the result back via an `mpsc` channel.
/// 3. `decide()` drains the channel with `try_recv()`, applies reaction delay,
///    records to `ConversationMemory`, and returns actions. While waiting for
///    the LLM, it replays the last action with exponential decay.
pub struct LlmAgent {
    id: AgentId,
    config: LlmAgentConfig,
    constraints: AgentConstraints,

    // --- Phase 2 pluggable components ---
    provider: Arc<dyn LlmProvider>,
    template: Arc<dyn PromptTemplate>,
    parser: Arc<dyn ActionParser>,
    memory: ConversationMemory,
    budget: TokenBudget,

    // --- Async result channel ---
    result_tx: mpsc::Sender<LlmResult>,
    result_rx: mpsc::Receiver<LlmResult>,
    request_in_flight: bool,

    // --- Decision double buffer ---
    decision_buffer: DecisionBuffer,
    decision_traces: VecDeque<DecisionTrace>,

    // --- Stale action replay ---
    last_action: Option<Action>,
    stale_action_decay: f32,
    ticks_since_last_decision: u32,
}

impl LlmAgent {
    /// Create a new LLM agent with default components.
    ///
    /// Uses `MockProvider::idle()` (returns `{"actions":["Idle"]}`) as the default
    /// provider. Call `with_components()` for production use with a real provider.
    pub fn new(config: LlmAgentConfig) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            id: AgentId::new(),
            config,
            constraints: AgentConstraints::default(),
            provider: Arc::new(MockProvider::idle()),
            template: Arc::new(ToonTemplate),
            parser: Arc::new(FallbackParser::default_chain()),
            memory: ConversationMemory::new(MemoryConfig::default()),
            budget: TokenBudget::unlimited(),
            result_tx: tx,
            result_rx: rx,
            request_in_flight: false,
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

    /// Create with a specific ID (for testing/loading saved agents).
    pub fn with_id(id: AgentId, config: LlmAgentConfig) -> Self {
        let mut agent = Self::new(config);
        agent.id = id;
        agent
    }

    /// Create with fully customized components.
    ///
    /// # Example
    /// ```ignore
    /// let agent = LlmAgent::with_components(
    ///     LlmAgentConfig::default(),
    ///     Arc::new(OpenAiProvider::new(api_key, None, None)),
    ///     Arc::new(TacticalTemplate::default()),
    ///     Arc::new(FallbackParser::default_chain()),
    ///     MemoryConfig { max_entries: 30, ..Default::default() },
    ///     TokenBudget::new(2000, 20000),
    /// );
    /// ```
    pub fn with_components(
        config: LlmAgentConfig,
        provider: Arc<dyn LlmProvider>,
        template: Arc<dyn PromptTemplate>,
        parser: Arc<dyn ActionParser>,
        memory_config: MemoryConfig,
        budget: TokenBudget,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            id: AgentId::new(),
            config,
            constraints: AgentConstraints::default(),
            provider,
            template,
            parser,
            memory: ConversationMemory::new(memory_config),
            budget,
            result_tx: tx,
            result_rx: rx,
            request_in_flight: false,
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

    /// Get decision traces (for replay/analysis).
    pub fn decision_traces(&self) -> &VecDeque<DecisionTrace> {
        &self.decision_traces
    }

    /// Clear decision traces (to free memory).
    pub fn clear_traces(&mut self) {
        self.decision_traces.clear();
    }

    /// Access the conversation memory.
    pub fn memory(&self) -> &ConversationMemory {
        &self.memory
    }

    /// Access the token budget.
    pub fn budget(&self) -> &TokenBudget {
        &self.budget
    }

    /// Reset per-tick token budget. Call once per game tick from the tick loop.
    pub fn reset_tick_budget(&mut self) {
        self.budget.reset_tick();
    }

    // ----------------------------------------------------------------
    // INTERNAL: async LLM request
    // ----------------------------------------------------------------

    /// Spawn a background thread to call the LLM provider.
    ///
    /// Formats the observation with the template, appends conversation memory,
    /// checks the token budget, then fires off a blocking LLM call in a thread.
    /// The result is sent back via `self.result_tx`.
    fn spawn_llm_request(&mut self, observation: &Observation) {
        // Format observation using the pluggable template
        let obs_str = self.template.format_observation(observation);
        let action_instructions = self.template.action_instructions();

        // Build memory section
        let memory_section = self.memory.to_prompt_section();

        // Build system prompt via template
        let system_prompt = self.template.format_system_prompt(&self.config.system_prompt, action_instructions);

        // Combine user prompt: memory + observation
        let user_prompt = if memory_section.is_empty() {
            format!("Current observation:\n{}\n\n{}", obs_str, action_instructions)
        } else {
            format!(
                "{}\nCurrent observation:\n{}\n\n{}",
                memory_section, obs_str, action_instructions
            )
        };

        // Estimate tokens and check budget
        let estimated_tokens = self.provider.estimate_tokens(&user_prompt)
            + self.provider.estimate_tokens(&system_prompt);
        if !self.budget.can_request(estimated_tokens) {
            warn!(
                "LLM agent {} skipping request: token budget exceeded ({} estimated, budget full)",
                self.id, estimated_tokens
            );
            return;
        }

        let request = CompletionRequest {
            model: self.config.model.clone(),
            system_prompt,
            user_prompt: user_prompt.clone(),
            temperature: self.config.temperature,
            max_tokens: self.config.max_tokens,
        };

        let provider = Arc::clone(&self.provider);
        let parser = Arc::clone(&self.parser);
        let tx = self.result_tx.clone();
        let agent_id = self.id;
        let enable_trace = self.config.enable_trace_recording;
        let tick = observation.tick;
        let obs_compressed = obs_str.clone();

        self.request_in_flight = true;

        std::thread::spawn(move || {
            match provider.complete(&request) {
                Ok(completion) => {
                    // Parse the LLM response into actions
                    let (actions, reasoning, raw) = match parser.parse(&completion.content) {
                        Ok(result) => {
                            for w in &result.warnings {
                                warn!("LLM agent {} parse warning: {}", agent_id, w);
                            }
                            (result.actions, result.reasoning, result.raw_response)
                        }
                        Err(e) => {
                            warn!(
                                "LLM agent {} parse failed ({}), falling back to Idle",
                                agent_id, e
                            );
                            (vec![Action::Idle], "Parse error fallback".to_string(), completion.content.clone())
                        }
                    };

                    let trace = if enable_trace {
                        Some(DecisionTrace {
                            tick,
                            compressed_observation: obs_compressed,
                            llm_prompt: format!("{}\n\n{}", request.system_prompt, request.user_prompt),
                            llm_response: raw.clone(),
                            parsed_actions: actions.clone(),
                            reasoning: reasoning.clone(),
                        })
                    } else {
                        None
                    };

                    let result = LlmResult {
                        actions,
                        reasoning,
                        raw_response: raw,
                        usage: Some(completion.usage),
                        trace,
                    };

                    debug!(
                        "LLM agent {} got response: {} actions",
                        agent_id,
                        result.actions.len()
                    );
                    let _ = tx.send(result);
                }
                Err(e) => {
                    error!("LLM request failed for agent {}: {}", agent_id, e);
                    // Send an Idle fallback so the agent doesn't hang
                    let _ = tx.send(LlmResult {
                        actions: vec![Action::Idle],
                        reasoning: format!("LLM error: {}", e),
                        raw_response: String::new(),
                        usage: None,
                        trace: None,
                    });
                }
            }
        });
    }

    /// Check if pending observation is ready (reaction delay elapsed).
    fn check_reaction_delay(&mut self) -> Option<Vec<Action>> {
        if let Some(pending) = &self.decision_buffer.pending {
            let elapsed = pending.submitted_at.elapsed();
            if elapsed >= Duration::from_millis(pending.config_reaction_ms) {
                return self.decision_buffer.ready_actions.take();
            }
        }
        None
    }

    /// Apply decay to stale action (when waiting for LLM).
    fn create_decayed_action(&self, action: &Action) -> Action {
        match action {
            Action::Move { direction } => {
                let decayed_dir = *direction * self.stale_action_decay;
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

    /// Drain results from the background thread channel.
    fn drain_results(&mut self) {
        while let Ok(result) = self.result_rx.try_recv() {
            self.request_in_flight = false;

            // Record token usage
            if let Some(ref usage) = result.usage {
                self.budget.record_usage(usage);
            }

            // Store ready actions in the double buffer
            self.decision_buffer.ready_actions = Some(result.actions);
            self.decision_buffer.ready_reasoning = result.reasoning;
            self.decision_buffer.trace = result.trace;
        }
    }
}

// ============================================================
// AGENT TRAIT IMPLEMENTATION
// ============================================================

impl Agent for LlmAgent {
    fn id(&self) -> AgentId {
        self.id
    }

    fn agent_type(&self) -> AgentType {
        AgentType::LlmAgent
    }

    fn observe(&mut self, observation: Observation) {
        // Drain any results that arrived since last tick
        self.drain_results();

        // Start reaction delay timer
        self.decision_buffer.pending = Some(PendingDecision {
            observation: observation.clone(),
            submitted_at: Instant::now(),
            config_reaction_ms: self.config.reaction_time_ms,
        });

        // Only spawn a new request if none is in flight
        if !self.request_in_flight {
            self.spawn_llm_request(&observation);
        }

        // Reset stale action decay counter
        self.ticks_since_last_decision = 0;
        self.stale_action_decay = 1.0;
    }

    fn decide(&mut self) -> Vec<Action> {
        // Drain any newly arrived results
        self.drain_results();

        // Increment tick counter for decay
        self.ticks_since_last_decision += 1;
        self.stale_action_decay = 0.95_f32.powi(self.ticks_since_last_decision as i32);

        // Check if reaction delay is complete
        if let Some(ready_actions) = self.check_reaction_delay() {
            self.decision_buffer.pending = None;

            if !ready_actions.is_empty() {
                // Record last action for decay
                self.last_action = Some(ready_actions[0].clone());

                // Record to conversation memory
                let reasoning = self.decision_buffer.ready_reasoning.clone();
                if let Some(pending) = &self.decision_buffer.pending {
                    self.memory.record(
                        &pending.observation,
                        &ready_actions,
                        &reasoning,
                    );
                }

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

        // No new action ready — return decayed last action or idle
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

// ============================================================
// DEFAULT PROMPTS
// ============================================================

/// Default system prompt for LLM agents.
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
Follow the action format specified by the system instructions."#
        .to_string()
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::{CompletionResponse, LlmError};
    use pod_core::observation::*;
    use glam::Vec2;
    use std::sync::Arc;
    use std::sync::Mutex;

    fn make_obs(tick: u64, hostile: bool) -> Observation {
        let mut obs = Observation {
            tick,
            self_state: SelfState {
                position: Vec2::new(100.0, 200.0),
                health: Some(80.0),
                max_health: Some(100.0),
                ..Default::default()
            },
            ..Default::default()
        };
        if hostile {
            obs.visible_entities.push(VisibleEntity {
                entity_id: pod_core::id::EntityId(1),
                entity_type: "enemy".to_string(),
                position: Vec2::new(150.0, 200.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                distance: 50.0,
                relationship: Relationship::Hostile,
                health_fraction: Some(0.5),
            });
        }
        obs
    }

    struct CapturingProvider {
        captured: Arc<Mutex<Option<CompletionRequest>>>,
        response: String,
    }

    impl CapturingProvider {
        fn new(
            response: String,
            captured: Arc<Mutex<Option<CompletionRequest>>>,
        ) -> Self {
            Self { response, captured }
        }
    }

    impl LlmProvider for CapturingProvider {
        fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
            *self.captured.lock().unwrap() = Some(request.clone());
            Ok(CompletionResponse {
                content: self.response.clone(),
                usage: TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 1,
                    total_tokens: 2,
                },
                model: "capture-test".to_string(),
            })
        }

        fn name(&self) -> &str {
            "capture-provider"
        }
    }

    #[test]
    fn test_new_agent_defaults() {
        let agent = LlmAgent::new(LlmAgentConfig::default());
        assert_eq!(agent.agent_type(), AgentType::LlmAgent);
        assert!(agent.decision_traces().is_empty());
        assert!(agent.memory().is_empty());
        assert!(!agent.request_in_flight);
    }

    #[test]
    fn test_observe_spawns_request() {
        let mut agent = LlmAgent::new(LlmAgentConfig::default());
        let obs = make_obs(1, false);
        agent.observe(obs);
        assert!(agent.request_in_flight);
        assert!(agent.decision_buffer.pending.is_some());
    }

    #[test]
    fn test_new_agent_uses_toon_template_and_parser_by_default() {
        let captured = Arc::new(Mutex::new(None));
        let response = r#"actions[1]{
  stop
}
reasoning[1]{
  Default smoke test
}"#
        .to_string();
        let provider = Arc::new(CapturingProvider::new(response, Arc::clone(&captured)));

        let mut agent = LlmAgent::new(LlmAgentConfig {
            reaction_time_ms: 0,
            ..Default::default()
        });
        agent.provider = provider;

        let obs = make_obs(1, false);
        agent.observe(obs);
        std::thread::sleep(Duration::from_millis(50));

        let request = captured
            .lock()
            .unwrap()
            .clone()
            .expect("provider should have captured completion request");

        assert!(request.user_prompt.contains("self["));
        assert!(request.user_prompt.contains("visible_entities["));
        assert!(request.user_prompt.contains("available_actions["));
        assert!(request.system_prompt.contains("TOON-formatted observations"));
        assert!(request.system_prompt.contains("actions["));
        assert!(request.system_prompt.contains("reasoning["));

        let actions = agent.decide();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::Stop));

        let trace = agent
            .decision_traces()
            .back()
            .expect("a decision trace should be recorded");
        assert_eq!(trace.reasoning, "Default smoke test");
    }

    #[test]
    fn test_decide_returns_idle_initially() {
        let mut agent = LlmAgent::new(LlmAgentConfig {
            reaction_time_ms: 0, // instant reaction for testing
            ..Default::default()
        });
        let actions = agent.decide();
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], Action::Idle));
    }

    #[test]
    fn test_mock_provider_roundtrip() {
        let mock = MockProvider::idle();
        let request = CompletionRequest {
            model: "test".to_string(),
            system_prompt: "test".to_string(),
            user_prompt: "test".to_string(),
            temperature: 0.7,
            max_tokens: 100,
        };
        let result = mock.complete(&request);
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(response.content.contains("Idle"));
    }

    #[test]
    fn test_full_observe_decide_cycle() {
        // Use MockProvider which returns Idle immediately
        let mut agent = LlmAgent::new(LlmAgentConfig {
            reaction_time_ms: 0, // instant
            ..Default::default()
        });

        let obs = make_obs(1, false);
        agent.observe(obs);

        // Give the background thread a moment to complete
        std::thread::sleep(Duration::from_millis(50));

        let actions = agent.decide();
        // Should have received the mock response
        assert!(!actions.is_empty());
    }

    #[test]
    fn test_stale_action_decay() {
        let agent = LlmAgent::new(LlmAgentConfig::default());

        let move_action = Action::Move { direction: Vec2::new(1.0, 0.0) };
        let decayed = agent.create_decayed_action(&move_action);
        // With decay=1.0 (initial), should be same direction
        if let Action::Move { direction } = decayed {
            assert!((direction.x - 1.0).abs() < 0.01);
        } else {
            panic!("Expected Move action");
        }

        // Attack should decay to Idle
        let attack = Action::Attack;
        let decayed = agent.create_decayed_action(&attack);
        assert!(matches!(decayed, Action::Idle));
    }

    #[test]
    fn test_with_components() {
        let agent = LlmAgent::with_components(
            LlmAgentConfig::default(),
            Arc::new(MockProvider::idle()),
            Arc::new(CompactTemplate::default()),
            Arc::new(FallbackParser::default_chain()),
            MemoryConfig {
                max_entries: 5,
                ..Default::default()
            },
            TokenBudget::new(1000, 10000),
        );
        assert_eq!(agent.agent_type(), AgentType::LlmAgent);
    }

    #[test]
    fn test_token_budget_blocks_request() {
        let mut agent = LlmAgent::with_components(
            LlmAgentConfig::default(),
            Arc::new(MockProvider::idle()),
            Arc::new(CompactTemplate::default()),
            Arc::new(FallbackParser::default_chain()),
            MemoryConfig::default(),
            TokenBudget::new(1, 1), // extremely tight budget
        );

        // Exhaust the budget
        agent.budget.record_usage(&TokenUsage {
            prompt_tokens: 1,
            completion_tokens: 0,
            total_tokens: 1,
        });

        let obs = make_obs(1, false);
        agent.spawn_llm_request(&obs);
        // Should NOT have set request_in_flight because budget is exhausted
        assert!(!agent.request_in_flight);
    }

    #[test]
    fn test_with_id() {
        let id = AgentId::new();
        let agent = LlmAgent::with_id(id, LlmAgentConfig::default());
        assert_eq!(agent.id(), id);
    }

    #[test]
    fn test_clear_traces() {
        let mut agent = LlmAgent::new(LlmAgentConfig::default());
        // Manually push a trace
        agent.decision_traces.push_back(DecisionTrace {
            tick: 0,
            compressed_observation: String::new(),
            llm_prompt: String::new(),
            llm_response: String::new(),
            parsed_actions: vec![],
            reasoning: String::new(),
        });
        assert_eq!(agent.decision_traces().len(), 1);
        agent.clear_traces();
        assert!(agent.decision_traces().is_empty());
    }
}
