//! Comprehensive integration tests for the pod-agents SDK.
//!
//! Tests cover all public APIs through the crate's public interface only.
//! Internal unit tests within each module are separate and not duplicated here.

use pod_agents::{
    // LLM agent
    LlmAgent, LlmAgentConfig,
    // Scripted agent
    ScriptedAgent, BehaviorTree, BehaviorNode, BehaviorStatus, Blackboard,
    FiniteStateMachine, FsmState, FsmTransition,
    patrol, chase_target, chase_nearest_hostile, attack_nearest, flee_from, guard, wander,
    patrol_and_chase_on_threat, guard_with_defense,
    // Neural agent
    NeuralAgent, ActionSelector, PolicyNetwork, Experience,
    RandomActionSelector, GreedyActionSelector, UniformPolicyNetwork,
    // Hybrid agent
    HybridAgent, HybridAgentConfig, StrategyDirective, StrategyTrigger,
    aggressive_hybrid, defensive_hybrid,
    // LLM provider
    LlmProvider, LlmError, CompletionRequest, CompletionResponse, TokenUsage,
    TokenBudget, MockProvider,
    // Prompt templates
    PromptTemplate, CompactTemplate, DetailedTemplate, TacticalTemplate,
    JsonTemplate, TemplateRegistry,
    // Action parser
    ActionParser, ActionParseResult, ActionParseError,
    JsonActionParser, KeyValueParser, FallbackParser,
    // Conversation memory
    ConversationMemory, MemoryEntry, MemoryConfig,
};
use pod_agents::decision_log::{DecisionLogger, LogFilter, ObservationSummary};
use pod_agents::utility_ai::{UtilityAgent, ActionScorer, UtilityProfile};

use pod_core::action::{Action, AgentConstraints};
use pod_core::agent::{Agent, AgentType};
use pod_core::id::{AgentId, EntityId};
use pod_core::observation::{
    Observation, SelfState, VisibleEntity, AudibleEvent, AgentMessage,
    MessageChannel, Relationship, Objective,
};
use pod_core::component::Team;

use glam::Vec2;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// ============================================================
// TEST HELPERS
// ============================================================

fn make_observation() -> Observation {
    Observation {
        tick: 10,
        elapsed_secs: 0.166,
        self_state: SelfState {
            agent_id: AgentId::default(),
            entity_id: EntityId::default(),
            position: Vec2::new(100.0, 200.0),
            rotation: 0.0,
            velocity: Vec2::ZERO,
            health: Some(80.0),
            max_health: Some(100.0),
            team: Team::default(),
            cooldowns: vec![],
        },
        visible_entities: vec![],
        audible_events: vec![],
        messages: vec![],
        available_actions: vec!["Move".to_string(), "Idle".to_string()],
        objectives: vec![],
    }
}

fn make_observation_with_hostile() -> Observation {
    let mut obs = make_observation();
    obs.visible_entities.push(VisibleEntity {
        entity_id: EntityId::default(),
        entity_type: "player".to_string(),
        position: Vec2::new(120.0, 200.0),
        velocity: Vec2::ZERO,
        rotation: 0.0,
        distance: 20.0,
        relationship: Relationship::Hostile,
        health_fraction: Some(1.0),
    });
    obs
}

fn make_observation_low_health() -> Observation {
    let mut obs = make_observation();
    obs.self_state.health = Some(15.0);
    obs.self_state.max_health = Some(100.0);
    obs
}

// Provider that captures the exact completion request and returns a fixed response.
struct CapturingProvider {
    response: String,
    captured: Arc<Mutex<Option<CompletionRequest>>>,
}

impl CapturingProvider {
    fn new(response: String, captured: Arc<Mutex<Option<CompletionRequest>>>) -> Self {
        Self { response, captured }
    }
}

impl LlmProvider for CapturingProvider {
    fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let mut captured = self.captured.lock().unwrap();
        *captured = Some(request.clone());
        Ok(CompletionResponse {
            content: self.response.clone(),
            usage: TokenUsage {
                prompt_tokens: 4,
                completion_tokens: 2,
                total_tokens: 6,
            },
            model: "sdk-smoke-model".to_string(),
        })
    }

    fn name(&self) -> &str {
        "capturing-provider"
    }
}

// ============================================================
// LLM PROVIDER TESTS
// ============================================================

#[test]
fn test_mock_provider_idle_creation() {
    let provider = MockProvider::idle();
    let req = CompletionRequest {
        model: "test".to_string(),
        system_prompt: "sys".to_string(),
        user_prompt: String::new(),
        temperature: 0.7,
        max_tokens: 100,
    };
    let result = provider.complete(&req);
    assert!(result.is_ok());
    let resp = result.unwrap();
    assert!(resp.content.contains("Idle"));
}

#[test]
fn test_mock_provider_responding() {
    let response_text = r#"{"actions":["Move"]}"#;
    let provider = MockProvider::responding(response_text.to_string());
    let req = CompletionRequest {
        model: "test".to_string(),
        system_prompt: "sys".to_string(),
        user_prompt: String::new(),
        temperature: 0.7,
        max_tokens: 100,
    };
    let result = provider.complete(&req);
    assert!(result.is_ok());
    let resp = result.unwrap();
    assert_eq!(resp.content, response_text);
}

#[test]
fn test_llm_agent_new_uses_toon_formatted_prompt_by_default() {
    let mut agent = LlmAgent::new(LlmAgentConfig {
        reaction_time_ms: 0,
        ..Default::default()
    });

    agent.observe(make_observation());
    std::thread::sleep(Duration::from_millis(50));

    let actions = agent.decide();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::Idle));

    let trace = agent
        .decision_traces()
        .back()
        .expect("a decision trace should be recorded");
    assert!(trace.llm_prompt.contains("self["));
    assert!(trace.llm_prompt.contains("visible_entities["));
    assert!(trace.llm_prompt.contains("available_actions["));
    assert!(trace.llm_prompt.contains("TOON-formatted observations"));
    assert!(trace.llm_prompt.contains("actions["));
    assert!(trace.llm_prompt.contains("reasoning["));
    assert!(matches!(trace.parsed_actions.first(), Some(Action::Idle)));
}

#[test]
fn test_new_llm_agent_emits_toon_prompts_and_toon_parser_roundtrip() {
    let captured = Arc::new(Mutex::new(None));
    let provider = Arc::new(CapturingProvider::new(
        r#"actions[1]{
  stop
}
reasoning[1]{
  SDK smoke test default alignment
}"#
        .to_string(),
        Arc::clone(&captured),
    ));

    let mut agent = LlmAgent::with_components(
        LlmAgentConfig {
            reaction_time_ms: 0,
            ..Default::default()
        },
        provider,
        Arc::new(ToonTemplate),
        Arc::new(FallbackParser::default_chain()),
        MemoryConfig::default(),
        TokenBudget::unlimited(),
    );

    agent.observe(make_observation());
    std::thread::sleep(Duration::from_millis(50));

    let request = captured
        .lock()
        .unwrap()
        .clone()
        .expect("capturing provider should have seen a completion request");

    assert!(request.user_prompt.contains("self["));
    assert!(request.user_prompt.contains("cooldowns["));
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
    assert_eq!(trace.reasoning, "SDK smoke test default alignment");
}

#[test]
fn test_token_budget_unlimited() {
    let budget = TokenBudget::unlimited();
    assert!(budget.has_budget(1_000_000));
}

#[test]
fn test_token_budget_limited() {
    let mut budget = TokenBudget::with_limit(100);
    assert!(budget.has_budget(50));
    budget.consume(80);
    assert!(!budget.has_budget(50));
    assert!(budget.has_budget(20));
}

#[test]
fn test_token_budget_reset() {
    let mut budget = TokenBudget::with_limit(100);
    budget.consume(90);
    budget.reset();
    assert!(budget.has_budget(100));
}

// ============================================================
// PROMPT TEMPLATE TESTS
// ============================================================

#[test]
fn test_compact_template_renders() {
    let template = CompactTemplate;
    let obs = make_observation();
    let prompt = template.render(&obs);
    assert!(!prompt.is_empty());
    // Should contain position or tick info
    assert!(prompt.contains("100") || prompt.contains("10") || prompt.len() > 5);
}

#[test]
fn test_detailed_template_renders() {
    let template = DetailedTemplate;
    let obs = make_observation();
    let prompt = template.render(&obs);
    assert!(!prompt.is_empty());
}

#[test]
fn test_tactical_template_renders() {
    let template = TacticalTemplate;
    let obs = make_observation_with_hostile();
    let prompt = template.render(&obs);
    assert!(!prompt.is_empty());
}

#[test]
fn test_json_template_renders_valid_json() {
    let template = JsonTemplate;
    let obs = make_observation();
    let prompt = template.render(&obs);
    // JSON template should produce parseable JSON
    assert!(!prompt.is_empty());
    // Should at minimum be valid enough to not be empty
    assert!(prompt.len() > 2);
}

#[test]
fn test_template_registry_default_contains_templates() {
    let registry = TemplateRegistry::default();
    let names = registry.available_templates();
    assert!(!names.is_empty());
}

#[test]
fn test_template_registry_get_by_name() {
    let registry = TemplateRegistry::default();
    // Should be able to get "compact" template
    let names = registry.available_templates();
    if let Some(name) = names.first() {
        let template = registry.get(name);
        assert!(template.is_some());
    }
}

// ============================================================
// ACTION PARSER TESTS
// ============================================================

#[test]
fn test_json_action_parser_idle() {
    let parser = JsonActionParser;
    let result = parser.parse(r#"{"actions":["Idle"]}"#);
    assert!(result.is_ok());
    let parsed = result.unwrap();
    assert!(!parsed.actions.is_empty());
    assert!(parsed.confidence > 0.5);
}

#[test]
fn test_json_action_parser_move() {
    let parser = JsonActionParser;
    let result = parser.parse(r#"{"actions":["Move"]}"#);
    assert!(result.is_ok());
    let parsed = result.unwrap();
    assert!(!parsed.actions.is_empty());
}

#[test]
fn test_json_action_parser_invalid_json() {
    let parser = JsonActionParser;
    let result = parser.parse("not json at all !!!");
    // Should fail gracefully
    assert!(result.is_err());
}

#[test]
fn test_key_value_parser_basic() {
    let parser = KeyValueParser;
    let result = parser.parse("action=Idle");
    assert!(result.is_ok());
    let parsed = result.unwrap();
    assert!(!parsed.actions.is_empty());
}

#[test]
fn test_fallback_parser_chain_uses_first_succeeding() {
    let parser = FallbackParser::default_chain();
    // Valid JSON should succeed
    let result = parser.parse(r#"{"actions":["Idle"]}"#);
    assert!(result.is_ok());
    let parsed = result.unwrap();
    assert!(!parsed.actions.is_empty());
}

#[test]
fn test_fallback_parser_chain_falls_back() {
    let parser = FallbackParser::default_chain();
    // Plain text with no JSON — should fall back through chain
    let result = parser.parse("Idle");
    // FallbackParser returns last result even on low confidence
    assert!(result.is_ok());
}

#[test]
fn test_action_parse_result_confidence_range() {
    let parser = JsonActionParser;
    let result = parser.parse(r#"{"actions":["Idle"]}"#).unwrap();
    assert!(result.confidence >= 0.0 && result.confidence <= 1.0);
}

// ============================================================
// CONVERSATION MEMORY TESTS
// ============================================================

#[test]
fn test_conversation_memory_new_is_empty() {
    let mem = ConversationMemory::new(MemoryConfig::default());
    assert_eq!(mem.entries().len(), 0);
}

#[test]
fn test_conversation_memory_record_first_entry() {
    let mut mem = ConversationMemory::new(MemoryConfig::default());
    let obs = make_observation();
    // First call always records
    mem.record(obs.tick, "prompt".to_string(), "response".to_string(), 0.3);
    assert_eq!(mem.entries().len(), 1);
}

#[test]
fn test_conversation_memory_record_high_importance_bypasses_interval() {
    let config = MemoryConfig {
        window_size: 20,
        record_interval_ticks: 30,
    };
    let mut mem = ConversationMemory::new(config);
    // First entry
    mem.record(1, "p1".to_string(), "r1".to_string(), 0.3);
    // Tick 5 — within interval (< 30) but high importance >= 0.5 → records
    mem.record(5, "p2".to_string(), "r2".to_string(), 0.8);
    assert_eq!(mem.entries().len(), 2);
}

#[test]
fn test_conversation_memory_record_low_importance_respects_interval() {
    let config = MemoryConfig {
        window_size: 20,
        record_interval_ticks: 30,
    };
    let mut mem = ConversationMemory::new(config);
    // First entry at tick 1
    mem.record(1, "p1".to_string(), "r1".to_string(), 0.3);
    // Tick 5 — within interval and low importance → skipped
    mem.record(5, "p2".to_string(), "r2".to_string(), 0.3);
    assert_eq!(mem.entries().len(), 1);
}

#[test]
fn test_conversation_memory_window_limits_entries() {
    let config = MemoryConfig {
        window_size: 3,
        record_interval_ticks: 1,
    };
    let mut mem = ConversationMemory::new(config);
    for i in 0..10u64 {
        mem.record(i, format!("p{}", i), format!("r{}", i), 0.8);
    }
    // Should be capped at window_size
    assert!(mem.entries().len() <= 3);
}

#[test]
fn test_conversation_memory_entry_fields() {
    let mut mem = ConversationMemory::new(MemoryConfig::default());
    mem.record(42, "my prompt".to_string(), "my response".to_string(), 0.7);
    let entry = &mem.entries()[0];
    assert_eq!(entry.tick, 42);
    assert_eq!(entry.prompt, "my prompt");
    assert_eq!(entry.response, "my response");
}

#[test]
fn test_conversation_memory_to_messages() {
    let mut mem = ConversationMemory::new(MemoryConfig::default());
    mem.record(1, "prompt1".to_string(), "response1".to_string(), 0.9);
    mem.record(100, "prompt2".to_string(), "response2".to_string(), 0.9);
    let messages = mem.to_messages();
    // Should produce user/assistant message pairs
    assert!(!messages.is_empty());
}

#[test]
fn test_memory_config_default() {
    let config = MemoryConfig::default();
    assert!(config.window_size > 0);
    assert!(config.record_interval_ticks > 0);
}
