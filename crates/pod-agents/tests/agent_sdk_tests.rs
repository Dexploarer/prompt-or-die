//! Public-surface integration tests for `pod-agents`.
//!
//! These tests intentionally use only the crate's exported API so the
//! integration suite stays aligned with downstream usage.

use glam::Vec2;
use pod_agents::{
    ActionParser, CompactTemplate, CompletionRequest, CompletionResponse, ConversationMemory,
    DetailedTemplate, FallbackParser, JsonActionParser, JsonTemplate, KeyValueParser, LlmAgent,
    LlmAgentConfig, LlmError, LlmProvider, MemoryConfig, MockProvider, PromptTemplate,
    TacticalTemplate, TemplateRegistry, TokenBudget, TokenUsage, ToonActionParser, ToonTemplate,
};
use pod_core::action::Action;
use pod_core::agent::Agent;
use pod_core::component::Team;
use pod_core::id::{AgentId, EntityId};
use pod_core::observation::{Observation, Relationship, SelfState, VisibleEntity};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
            ..Default::default()
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
        entity_id: EntityId(77),
        entity_type: "player".to_string(),
        position: Vec2::new(120.0, 200.0),
        velocity: Vec2::ZERO,
        rotation: 0.0,
        distance: 20.0,
        relationship: Relationship::Hostile,
        health_fraction: Some(1.0),
        ..Default::default()
    });
    obs
}

fn extract_toon_observation(user_prompt: &str) -> serde_json::Value {
    let toon = user_prompt
        .split("Current observation:\n")
        .nth(1)
        .and_then(|rest| rest.split("\n\nRespond with a valid TOON object").next())
        .expect("prompt should include a TOON observation section");
    toon_format::decode_default(toon).expect("observation should decode as official TOON")
}

struct CapturingProvider {
    response: String,
    captured: Arc<Mutex<Option<CompletionRequest>>>,
}

impl CapturingProvider {
    fn new(response: impl Into<String>, captured: Arc<Mutex<Option<CompletionRequest>>>) -> Self {
        Self {
            response: response.into(),
            captured,
        }
    }
}

impl LlmProvider for CapturingProvider {
    fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse, LlmError> {
        *self.captured.lock().unwrap() = Some(request.clone());
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

#[test]
fn mock_provider_idle_creation() {
    let provider = MockProvider::idle();
    let req = CompletionRequest {
        model: "test".to_string(),
        system_prompt: "sys".to_string(),
        user_prompt: String::new(),
        temperature: 0.7,
        max_tokens: 100,
    };
    let resp = provider.complete(&req).unwrap();
    assert!(resp.content.contains("Idle"));
}

#[test]
fn mock_provider_custom_creation() {
    let provider = MockProvider::new(r#"{"actions":["stop"]}"#.to_string());
    let req = CompletionRequest {
        model: "test".to_string(),
        system_prompt: "sys".to_string(),
        user_prompt: String::new(),
        temperature: 0.7,
        max_tokens: 100,
    };
    let resp = provider.complete(&req).unwrap();
    assert_eq!(resp.content, r#"{"actions":["stop"]}"#);
}

#[test]
fn llm_agent_defaults_to_official_toon_prompting() {
    let mut agent = LlmAgent::new(LlmAgentConfig {
        reaction_time_ms: 0,
        ..Default::default()
    });

    agent.observe(make_observation());
    std::thread::sleep(Duration::from_millis(50));

    let actions = agent.decide();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::Idle));

    let trace = agent.decision_traces().back().unwrap();
    assert!(trace.llm_prompt.contains("official TOON"));
    assert!(trace.llm_prompt.contains("toonformat.dev"));
    assert!(trace.llm_prompt.contains("actions["));
    assert!(trace.llm_prompt.contains("reasoning:"));
}

#[test]
fn llm_agent_roundtrips_official_toon_prompt_and_response() {
    let captured = Arc::new(Mutex::new(None));
    let provider = Arc::new(CapturingProvider::new(
        "actions[1]: stop\nreasoning: SDK smoke test default alignment",
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

    let request = captured.lock().unwrap().clone().unwrap();
    let decoded_prompt = extract_toon_observation(&request.user_prompt);
    assert_eq!(decoded_prompt["tick"], 10);
    assert!(decoded_prompt["self_state"].is_object());
    assert!(decoded_prompt["available_actions"].is_array());
    assert!(request.system_prompt.contains("official TOON"));

    let actions = agent.decide();
    assert_eq!(actions.len(), 1);
    assert!(matches!(actions[0], Action::Stop));

    let trace = agent.decision_traces().back().unwrap();
    assert_eq!(trace.reasoning, "SDK smoke test default alignment");
}

#[test]
fn token_budget_smoke() {
    let mut budget = TokenBudget::new(100, 1000);
    assert!(budget.can_request(50));
    budget.record_usage(&TokenUsage {
        prompt_tokens: 20,
        completion_tokens: 30,
        total_tokens: 50,
    });
    assert!(!budget.can_request(60));
    assert!(budget.can_request(50));
    budget.reset_tick();
    assert!(budget.can_request(100));
}

#[test]
fn template_suite_renders_current_formats() {
    let obs = make_observation_with_hostile();

    let compact = CompactTemplate::default().render(&obs);
    assert!(!compact.is_empty());

    let detailed = DetailedTemplate.render(&obs);
    assert!(!detailed.is_empty());

    let tactical = TacticalTemplate::default().render(&obs);
    assert!(!tactical.is_empty());

    let json_prompt = JsonTemplate.render(&obs);
    let json_value: serde_json::Value = serde_json::from_str(&json_prompt).unwrap();
    assert_eq!(json_value["tick"], 10);

    let toon_prompt = ToonTemplate.render(&obs);
    let toon_value: serde_json::Value = toon_format::decode_default(&toon_prompt).unwrap();
    assert_eq!(toon_value["tick"], 10);
    assert!(toon_value["visible_entities"].is_array());
}

#[test]
fn template_registry_exposes_toon() {
    let registry = TemplateRegistry::default();
    assert!(registry.get("toon").is_some());
    assert!(registry.available_templates().contains(&"toon"));
}

#[test]
fn parser_suite_handles_json_toon_and_key_value() {
    let json_result = JsonActionParser
        .parse(r#"{"actions":["stop"],"reasoning":"json"}"#)
        .unwrap();
    assert!(matches!(json_result.actions[0], Action::Stop));
    assert_eq!(json_result.reasoning, "json");

    let toon_result = ToonActionParser
        .parse("actions[2]: stop,attack\nreasoning: toon")
        .unwrap();
    assert!(matches!(toon_result.actions[0], Action::Stop));
    assert!(matches!(toon_result.actions[1], Action::Attack));
    assert_eq!(toon_result.reasoning, "toon");

    let kv_result = KeyValueParser
        .parse("ACTION: stop\nREASON: key value")
        .unwrap();
    assert!(matches!(kv_result.actions[0], Action::Stop));

    let fallback_result = FallbackParser::default_chain()
        .parse("actions[1]: stop")
        .unwrap();
    assert!(matches!(fallback_result.actions[0], Action::Stop));
}

#[test]
fn conversation_memory_records_current_decision_shape() {
    let mut memory = ConversationMemory::new(MemoryConfig {
        max_entries: 3,
        max_tokens: 200,
        importance_threshold: 0.0,
        record_interval: 1,
        include_in_prompts: true,
    });

    let obs = make_observation_with_hostile();
    assert!(memory.record(&obs, &[Action::Attack], "Pressure the hostile target"));
    assert_eq!(memory.entries().len(), 1);

    let entry = memory.last_entry().unwrap();
    assert_eq!(entry.tick, obs.tick);
    assert_eq!(entry.reasoning, "Pressure the hostile target");
    assert!(!entry.actions_taken.is_empty());

    let prompt_section = memory.to_prompt_section();
    assert!(prompt_section.contains("MEMORY"));
    assert!(prompt_section.contains("Pressure the hostile target"));
}
