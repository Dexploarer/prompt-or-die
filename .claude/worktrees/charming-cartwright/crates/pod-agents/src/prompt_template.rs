//! Prompt template system for formatting observations into LLM-consumable text.
//!
//! Provides configurable prompt formatting for the [`LlmAgent`](super::LlmAgent)
//! decision pipeline. Templates control how system context, observations, history,
//! and action instructions are assembled into chat messages.
//!
//! Two built-in templates:
//! - [`DefaultPromptTemplate`] — full-featured formatting with configurable sections
//! - [`CompactPromptTemplate`] — minimal output for token-constrained scenarios
//!
//! Use [`PromptBuilder`] to assemble a complete message list from template outputs.

use crate::llm_provider::ChatMessage;
use pod_core::observation::Observation;

// ---------------------------------------------------------------------------
// PromptTemplate trait
// ---------------------------------------------------------------------------

/// Trait for formatting game state into LLM prompt segments.
///
/// Implementations control the verbosity, structure, and content of each
/// prompt section. All agents share the same observation data — the template
/// determines how that data is presented to the model.
pub trait PromptTemplate: Send + Sync {
    /// Format the system message establishing agent identity and rules.
    fn format_system_prompt(&self, agent_name: &str, agent_role: &str) -> String;

    /// Format an observation into a user-message segment.
    fn format_observation(&self, obs: &Observation) -> String;

    /// Format instructions telling the model how to respond with actions.
    fn format_action_instructions(&self, available_actions: &[String]) -> String;

    /// Rough token count estimation (~4 characters per token).
    fn estimated_tokens(&self, text: &str) -> usize {
        // GPT/Claude tokenizers average roughly 4 characters per token for English.
        // This is intentionally conservative (overestimates slightly) to avoid
        // exceeding context limits.
        (text.len() + 3) / 4
    }
}

// ---------------------------------------------------------------------------
// DefaultPromptTemplate
// ---------------------------------------------------------------------------

/// Configuration for which sections the default template includes.
#[derive(Debug, Clone)]
pub struct TemplateSections {
    /// Include game-rules summary in the system prompt.
    pub include_game_rules: bool,
    /// Include response-format instructions in the system prompt.
    pub include_format_instructions: bool,
    /// Include velocity/rotation details in observation output.
    pub include_detailed_self_state: bool,
    /// Include audible events in observation output.
    pub include_audio: bool,
    /// Include objectives in observation output.
    pub include_objectives: bool,
}

impl Default for TemplateSections {
    fn default() -> Self {
        Self {
            include_game_rules: true,
            include_format_instructions: true,
            include_detailed_self_state: true,
            include_audio: true,
            include_objectives: true,
        }
    }
}

/// Full-featured prompt template with configurable sections.
///
/// Produces detailed system prompts, rich observation formatting (using
/// [`Observation::to_agent_prompt()`]), and explicit action instructions
/// with JSON output format.
#[derive(Debug, Clone)]
pub struct DefaultPromptTemplate {
    /// Which sections to include in generated prompts.
    pub sections: TemplateSections,
}

impl DefaultPromptTemplate {
    /// Create a new default template with all sections enabled.
    pub fn new() -> Self {
        Self {
            sections: TemplateSections::default(),
        }
    }

    /// Create with custom section configuration.
    pub fn with_sections(sections: TemplateSections) -> Self {
        Self { sections }
    }
}

impl Default for DefaultPromptTemplate {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptTemplate for DefaultPromptTemplate {
    fn format_system_prompt(&self, agent_name: &str, agent_role: &str) -> String {
        let mut prompt = format!(
            "You are {}, an AI agent in the game Prompt or Die.\n\
             Role: {}\n",
            agent_name, agent_role
        );

        if self.sections.include_game_rules {
            prompt.push_str(
                "\n## Game Rules\n\
                 - The world runs at 60 ticks per second. You receive observations each decision cycle.\n\
                 - You may issue up to 3 actions per tick.\n\
                 - All agents (human and AI) operate under identical constraints.\n\
                 - Actions have cooldowns: attacks require 30 ticks (0.5s) between uses.\n\
                 - You can only interact with entities within your perception range.\n",
            );
        }

        if self.sections.include_format_instructions {
            prompt.push_str(
                "\n## Response Format\n\
                 Respond with a JSON object containing:\n\
                 - \"actions\": an array of action strings to execute this tick\n\
                 - \"reasoning\": a brief explanation of your decision (optional)\n\
                 \n\
                 Example:\n\
                 {\"actions\": [\"move north\", \"attack\"], \"reasoning\": \"enemy spotted nearby\"}\n",
            );
        }

        prompt
    }

    fn format_observation(&self, obs: &Observation) -> String {
        let mut output = String::from("## Current Observation\n");
        output.push_str(&obs.to_agent_prompt());
        output
    }

    fn format_action_instructions(&self, available_actions: &[String]) -> String {
        if available_actions.is_empty() {
            return "No actions available this tick. Respond with {\"actions\": [\"idle\"]}.".to_string();
        }

        let mut output = String::from("## Available Actions\n");
        for action in available_actions {
            output.push_str(&format!("- {}\n", action));
        }
        output.push_str(
            "\nChoose actions from the list above. Respond with a JSON object:\n\
             {\"actions\": [\"<action1>\", ...], \"reasoning\": \"<brief explanation>\"}\n",
        );
        output
    }
}

// ---------------------------------------------------------------------------
// CompactPromptTemplate
// ---------------------------------------------------------------------------

/// Minimal prompt template for token-constrained scenarios.
///
/// Produces abbreviated system prompts, key-stats-only observation format,
/// and terse action instructions. Useful when operating near context limits
/// or when using smaller/cheaper models.
#[derive(Debug, Clone)]
pub struct CompactPromptTemplate;

impl CompactPromptTemplate {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CompactPromptTemplate {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptTemplate for CompactPromptTemplate {
    fn format_system_prompt(&self, agent_name: &str, agent_role: &str) -> String {
        format!(
            "Agent {} ({}). Reply JSON: {{\"actions\":[...]}}", agent_name, agent_role
        )
    }

    fn format_observation(&self, obs: &Observation) -> String {
        let self_state = &obs.self_state;
        let mut output = format!(
            "T{} pos({:.0},{:.0}) hp{}/{}",
            obs.tick,
            self_state.position.x,
            self_state.position.y,
            self_state.health.unwrap_or(0.0) as i32,
            self_state.max_health.unwrap_or(0.0) as i32,
        );

        if !obs.visible_entities.is_empty() {
            output.push_str(" see:");
            for e in &obs.visible_entities {
                output.push_str(&format!(
                    " {}({:.0},{:?})",
                    e.entity_type, e.distance, e.relationship
                ));
            }
        }

        if !obs.messages.is_empty() {
            output.push_str(&format!(" msgs:{}", obs.messages.len()));
        }

        output
    }

    fn format_action_instructions(&self, available_actions: &[String]) -> String {
        if available_actions.is_empty() {
            return "No actions. {\"actions\":[\"idle\"]}".to_string();
        }

        format!("Actions: [{}]", available_actions.join(", "))
    }
}

// ---------------------------------------------------------------------------
// PromptBuilder
// ---------------------------------------------------------------------------

/// Builder pattern for assembling a complete prompt from template sections.
///
/// Collects system messages, observations, conversation history, and action
/// instructions, then assembles them into a `Vec<ChatMessage>` ready for
/// submission to an [`LlmProvider`](crate::llm_provider::LlmProvider).
///
/// # Example
///
/// ```rust,ignore
/// let messages = PromptBuilder::new()
///     .system(&template, "Warrior", "melee fighter")
///     .observation(&template, &obs)
///     .history(&past_messages)
///     .instruction(&template, &actions)
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct PromptBuilder {
    messages: Vec<ChatMessage>,
}

impl PromptBuilder {
    /// Create an empty prompt builder.
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
        }
    }

    /// Add the system message using the given template and agent identity.
    pub fn system(
        mut self,
        template: &dyn PromptTemplate,
        agent_name: &str,
        agent_role: &str,
    ) -> Self {
        let content = template.format_system_prompt(agent_name, agent_role);
        self.messages.push(ChatMessage::system(content));
        self
    }

    /// Add the current observation as a user message.
    pub fn observation(mut self, template: &dyn PromptTemplate, obs: &Observation) -> Self {
        let content = template.format_observation(obs);
        self.messages.push(ChatMessage::user(content));
        self
    }

    /// Insert conversation history messages (preserving their original roles).
    pub fn history(mut self, messages: &[ChatMessage]) -> Self {
        self.messages.extend(messages.iter().cloned());
        self
    }

    /// Add action instructions as a user message.
    pub fn instruction(
        mut self,
        template: &dyn PromptTemplate,
        available_actions: &[String],
    ) -> Self {
        let content = template.format_action_instructions(available_actions);
        self.messages.push(ChatMessage::user(content));
        self
    }

    /// Assemble all collected sections into a final message list.
    pub fn build(self) -> Vec<ChatMessage> {
        self.messages
    }

    /// Estimate total token usage across all messages.
    pub fn estimated_total_tokens(&self, template: &dyn PromptTemplate) -> usize {
        self.messages
            .iter()
            .map(|msg| template.estimated_tokens(&msg.content))
            .sum()
    }
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm_provider::ChatRole;
    use glam::Vec2;
    use pod_core::component::Team;
    use pod_core::id::{AgentId, EntityId};
    use pod_core::observation::*;

    /// Helper: build a minimal Observation for testing.
    fn make_observation() -> Observation {
        Observation {
            tick: 100,
            elapsed_secs: 1.667,
            self_state: SelfState {
                agent_id: AgentId::new(),
                entity_id: EntityId(10),
                position: Vec2::new(50.0, 75.0),
                rotation: 0.0,
                velocity: Vec2::ZERO,
                health: Some(80.0),
                max_health: Some(100.0),
                team: Team::None,
                cooldowns: vec![],
            },
            visible_entities: vec![VisibleEntity {
                entity_id: EntityId(20),
                entity_type: "enemy_soldier".to_string(),
                position: Vec2::new(60.0, 80.0),
                velocity: Vec2::new(1.0, 0.0),
                rotation: 0.0,
                distance: 12.0,
                relationship: Relationship::Hostile,
                health_fraction: Some(0.5),
            }],
            audible_events: vec![AudibleEvent {
                event_type: "explosion".to_string(),
                direction: Vec2::new(1.0, 0.0),
                distance: 50.0,
                intensity: 0.8,
            }],
            messages: vec![AgentMessage {
                from: AgentId::new(),
                content: "Watch out!".to_string(),
                channel: MessageChannel::Team,
            }],
            available_actions: vec![
                "move".to_string(),
                "attack".to_string(),
                "idle".to_string(),
            ],
            objectives: vec![Objective {
                id: "obj1".to_string(),
                description: "Survive 5 minutes".to_string(),
                progress: 0.33,
                completed: false,
            }],
        }
    }

    /// Helper: build an empty Observation.
    fn make_empty_observation() -> Observation {
        Observation {
            tick: 0,
            elapsed_secs: 0.0,
            self_state: SelfState {
                agent_id: AgentId::new(),
                entity_id: EntityId(10),
                position: Vec2::ZERO,
                rotation: 0.0,
                velocity: Vec2::ZERO,
                health: None,
                max_health: None,
                team: Team::None,
                cooldowns: vec![],
            },
            visible_entities: vec![],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![],
        }
    }

    // --- DefaultPromptTemplate tests ---

    #[test]
    fn default_system_prompt_contains_identity_and_role() {
        let template = DefaultPromptTemplate::new();
        let prompt = template.format_system_prompt("Warrior", "melee fighter");

        assert!(prompt.contains("Warrior"), "should contain agent name");
        assert!(prompt.contains("melee fighter"), "should contain role");
        assert!(prompt.contains("Game Rules"), "should contain rules section");
        assert!(prompt.contains("Response Format"), "should contain format section");
    }

    #[test]
    fn default_system_prompt_respects_disabled_sections() {
        let template = DefaultPromptTemplate::with_sections(TemplateSections {
            include_game_rules: false,
            include_format_instructions: false,
            ..Default::default()
        });
        let prompt = template.format_system_prompt("Agent", "scout");

        assert!(prompt.contains("Agent"), "name is always present");
        assert!(!prompt.contains("Game Rules"), "rules should be excluded");
        assert!(!prompt.contains("Response Format"), "format should be excluded");
    }

    #[test]
    fn default_observation_formatting() {
        let template = DefaultPromptTemplate::new();
        let obs = make_observation();
        let output = template.format_observation(&obs);

        assert!(output.contains("Current Observation"), "should have header");
        assert!(output.contains("TICK: 100"), "should contain tick");
        assert!(output.contains("enemy_soldier"), "should list visible entities");
        assert!(output.contains("explosion"), "should list audible events");
        assert!(output.contains("Watch out!"), "should contain messages");
    }

    #[test]
    fn default_action_instructions_lists_actions() {
        let template = DefaultPromptTemplate::new();
        let actions = vec!["move".to_string(), "attack".to_string(), "idle".to_string()];
        let output = template.format_action_instructions(&actions);

        assert!(output.contains("Available Actions"), "should have header");
        assert!(output.contains("- move"), "should list move");
        assert!(output.contains("- attack"), "should list attack");
        assert!(output.contains("- idle"), "should list idle");
        assert!(output.contains("JSON"), "should mention JSON format");
    }

    #[test]
    fn default_action_instructions_handles_empty() {
        let template = DefaultPromptTemplate::new();
        let output = template.format_action_instructions(&[]);

        assert!(output.contains("idle"), "should suggest idle when no actions");
    }

    // --- CompactPromptTemplate tests ---

    #[test]
    fn compact_produces_shorter_output_than_default() {
        let default = DefaultPromptTemplate::new();
        let compact = CompactPromptTemplate::new();
        let obs = make_observation();

        let default_system = default.format_system_prompt("Test", "fighter");
        let compact_system = compact.format_system_prompt("Test", "fighter");
        assert!(
            compact_system.len() < default_system.len(),
            "compact system prompt ({}) should be shorter than default ({})",
            compact_system.len(),
            default_system.len()
        );

        let default_obs = default.format_observation(&obs);
        let compact_obs = compact.format_observation(&obs);
        assert!(
            compact_obs.len() < default_obs.len(),
            "compact observation ({}) should be shorter than default ({})",
            compact_obs.len(),
            default_obs.len()
        );

        let actions = vec!["move".to_string(), "attack".to_string()];
        let default_instr = default.format_action_instructions(&actions);
        let compact_instr = compact.format_action_instructions(&actions);
        assert!(
            compact_instr.len() < default_instr.len(),
            "compact instructions ({}) should be shorter than default ({})",
            compact_instr.len(),
            default_instr.len()
        );
    }

    #[test]
    fn compact_observation_includes_key_stats() {
        let compact = CompactPromptTemplate::new();
        let obs = make_observation();
        let output = compact.format_observation(&obs);

        assert!(output.contains("T100"), "should contain tick");
        assert!(output.contains("pos(50,75)"), "should contain position");
        assert!(output.contains("hp80/100"), "should contain health");
        assert!(output.contains("see:"), "should show visible entities");
    }

    // --- PromptBuilder tests ---

    #[test]
    fn builder_assembles_messages_in_order() {
        let template = DefaultPromptTemplate::new();
        let obs = make_observation();
        let actions = vec!["move".to_string(), "attack".to_string()];

        let messages = PromptBuilder::new()
            .system(&template, "Warrior", "melee fighter")
            .observation(&template, &obs)
            .instruction(&template, &actions)
            .build();

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, ChatRole::System);
        assert_eq!(messages[1].role, ChatRole::User);
        assert_eq!(messages[2].role, ChatRole::User);
        assert!(messages[0].content.contains("Warrior"));
        assert!(messages[1].content.contains("TICK: 100"));
        assert!(messages[2].content.contains("move"));
    }

    #[test]
    fn builder_includes_history() {
        let template = CompactPromptTemplate::new();
        let obs = make_empty_observation();
        let history = vec![
            ChatMessage::user("previous observation"),
            ChatMessage::assistant("{\"actions\": [\"idle\"]}"),
        ];

        let messages = PromptBuilder::new()
            .system(&template, "Scout", "reconnaissance")
            .history(&history)
            .observation(&template, &obs)
            .build();

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, ChatRole::System);
        assert_eq!(messages[1].role, ChatRole::User);
        assert_eq!(messages[1].content, "previous observation");
        assert_eq!(messages[2].role, ChatRole::Assistant);
        assert_eq!(messages[3].role, ChatRole::User);
    }

    // --- Token estimation tests ---

    #[test]
    fn token_estimation_is_reasonable() {
        let template = DefaultPromptTemplate::new();

        // 4 chars -> ~1 token
        assert_eq!(template.estimated_tokens("abcd"), 1);
        // 8 chars -> ~2 tokens
        assert_eq!(template.estimated_tokens("abcdefgh"), 2);
        // empty -> 0 tokens (with rounding: (0 + 3) / 4 = 0)
        assert_eq!(template.estimated_tokens(""), 0);
        // 100 chars -> 25 tokens
        let text_100 = "a".repeat(100);
        assert_eq!(template.estimated_tokens(&text_100), 25);
        // Realistic prompt should be in reasonable range
        let prompt = template.format_system_prompt("Test", "fighter");
        let tokens = template.estimated_tokens(&prompt);
        assert!(tokens > 10, "system prompt should be more than 10 tokens");
        assert!(tokens < 2000, "system prompt should be under 2000 tokens");
    }

    #[test]
    fn builder_estimated_total_tokens() {
        let template = DefaultPromptTemplate::new();
        let obs = make_observation();
        let actions = vec!["move".to_string()];

        let builder = PromptBuilder::new()
            .system(&template, "Agent", "role")
            .observation(&template, &obs)
            .instruction(&template, &actions);

        let total = builder.estimated_total_tokens(&template);
        assert!(total > 0, "total tokens should be positive");

        // Should equal sum of individual message estimates
        let messages = builder.build();
        let manual_total: usize = messages
            .iter()
            .map(|m| template.estimated_tokens(&m.content))
            .sum();
        assert_eq!(total, manual_total, "builder total should match manual sum");
    }

    // --- Empty observation handling ---

    #[test]
    fn empty_observation_does_not_panic() {
        let default_template = DefaultPromptTemplate::new();
        let compact_template = CompactPromptTemplate::new();
        let obs = make_empty_observation();

        let default_output = default_template.format_observation(&obs);
        let compact_output = compact_template.format_observation(&obs);

        assert!(!default_output.is_empty(), "default should produce output");
        assert!(!compact_output.is_empty(), "compact should produce output");
        // Should not contain entity sections when there are none
        assert!(!default_output.contains("VISIBLE:"), "no entities to show");
        assert!(!compact_output.contains("see:"), "no entities to show");
    }

    #[test]
    fn compact_messages_count_included_when_present() {
        let template = CompactPromptTemplate::new();
        let obs = make_observation();
        let output = template.format_observation(&obs);
        assert!(output.contains("msgs:1"), "should show message count");

        let empty_obs = make_empty_observation();
        let empty_output = template.format_observation(&empty_obs);
        assert!(!empty_output.contains("msgs:"), "should omit message count when 0");
    }
}
