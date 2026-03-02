//! TOON (Token-Oriented Object Notation) encoding for observations.
//!
//! Provides token-efficient serialization of [`Observation`] data using the
//! [TOON format](https://crates.io/crates/toon-format), which achieves ~40%
//! fewer tokens than JSON by:
//!
//! - Declaring field headers once for uniform arrays
//! - Using indentation instead of braces
//! - Minimizing quoting
//! - Explicit array lengths
//!
//! # Usage
//!
//! ```rust,ignore
//! use pod_agents::toon_encoding::{observation_to_toon, ToonPromptTemplate};
//! use pod_core::Observation;
//!
//! let obs: Observation = /* ... */;
//! let toon_str = observation_to_toon(&obs).unwrap();
//!
//! // Or use the PromptTemplate implementation:
//! let template = ToonPromptTemplate::new();
//! let formatted = template.format_observation(&obs);
//! ```

use pod_core::observation::Observation;
use toon_format::encode_default;

use crate::prompt_template::PromptTemplate;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur during TOON encoding.
#[derive(Debug)]
pub enum ToonEncodeError {
    /// Failed to convert value to `serde_json::Value` intermediate.
    SerializationError(String),
    /// Failed to encode the value as TOON.
    EncodingError(String),
}

impl std::fmt::Display for ToonEncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToonEncodeError::SerializationError(msg) => {
                write!(f, "TOON serialization error: {}", msg)
            }
            ToonEncodeError::EncodingError(msg) => {
                write!(f, "TOON encoding error: {}", msg)
            }
        }
    }
}

impl std::error::Error for ToonEncodeError {}

// ---------------------------------------------------------------------------
// Encoding functions
// ---------------------------------------------------------------------------

/// Encode an [`Observation`] to TOON format for token-efficient LLM consumption.
///
/// Converts the observation to a `serde_json::Value` intermediary, then encodes
/// it using `toon_format::encode_default`. This produces a compact string
/// representation that uses significantly fewer tokens than raw JSON.
pub fn observation_to_toon(obs: &Observation) -> Result<String, ToonEncodeError> {
    let value = serde_json::to_value(obs)
        .map_err(|e| ToonEncodeError::SerializationError(e.to_string()))?;
    encode_default(&value).map_err(|e| ToonEncodeError::EncodingError(e.to_string()))
}

/// Encode any serializable value to TOON format.
///
/// This is a convenience wrapper that converts `T` → `serde_json::Value` → TOON string.
pub fn to_toon<T: serde::Serialize>(value: &T) -> Result<String, ToonEncodeError> {
    let json_value = serde_json::to_value(value)
        .map_err(|e| ToonEncodeError::SerializationError(e.to_string()))?;
    encode_default(&json_value).map_err(|e| ToonEncodeError::EncodingError(e.to_string()))
}

/// Encode a `serde_json::Value` directly to TOON format.
///
/// Skips the `serde_json::to_value()` step when you already have a `Value`.
pub fn value_to_toon(value: &serde_json::Value) -> Result<String, ToonEncodeError> {
    encode_default(value).map_err(|e| ToonEncodeError::EncodingError(e.to_string()))
}

// ---------------------------------------------------------------------------
// ToonPromptTemplate
// ---------------------------------------------------------------------------

/// Prompt template that encodes observations in TOON format.
///
/// Uses the same system prompt and action instructions as
/// [`DefaultPromptTemplate`](crate::prompt_template::DefaultPromptTemplate),
/// but serializes observations using TOON for ~40% token savings on structured
/// data (entity lists, cooldowns, etc.).
///
/// Falls back to [`Observation::to_agent_prompt()`] if TOON encoding fails.
#[derive(Debug, Clone)]
pub struct ToonPromptTemplate;

impl ToonPromptTemplate {
    /// Create a new TOON prompt template.
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToonPromptTemplate {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptTemplate for ToonPromptTemplate {
    fn format_system_prompt(&self, agent_name: &str, agent_role: &str) -> String {
        // TOON doesn't help for prose-heavy system prompts — reuse the default.
        format!(
            "You are {}, an AI agent in the game Prompt or Die.\n\
             Role: {}\n\
             \n## Game Rules\n\
             - The world runs at 60 ticks per second. You receive observations each decision cycle.\n\
             - You may issue up to 3 actions per tick.\n\
             - All agents (human and AI) operate under identical constraints.\n\
             - Actions have cooldowns: attacks require 30 ticks (0.5s) between uses.\n\
             - You can only interact with entities within your perception range.\n\
             \n## Response Format\n\
             Respond with a JSON object containing:\n\
             - \"actions\": an array of action strings to execute this tick\n\
             - \"reasoning\": a brief explanation of your decision (optional)\n\
             \n\
             Example:\n\
             {{\"actions\": [\"move north\", \"attack\"], \"reasoning\": \"enemy spotted nearby\"}}\n",
            agent_name, agent_role
        )
    }

    fn format_observation(&self, obs: &Observation) -> String {
        match observation_to_toon(obs) {
            Ok(toon) => {
                format!("## Current Observation (TOON)\n{}", toon)
            }
            Err(e) => {
                log::warn!("TOON encoding failed, falling back to text: {}", e);
                let mut output = String::from("## Current Observation\n");
                output.push_str(&obs.to_agent_prompt());
                output
            }
        }
    }

    fn format_action_instructions(&self, available_actions: &[String]) -> String {
        // Same as DefaultPromptTemplate — action instructions are short prose.
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prompt_template::PromptTemplate;
    use glam::Vec2;
    use pod_core::component::Team;
    use pod_core::id::{AgentId, EntityId};
    use pod_core::observation::*;
    use serde_json::json;

    /// Helper: build a minimal Observation for testing.
    fn make_observation() -> Observation {
        Observation {
            tick: 100,
            elapsed_secs: 1.67,
            self_state: SelfState {
                agent_id: AgentId(uuid::Uuid::nil()),
                entity_id: EntityId(10),
                position: Vec2::new(50.0, 75.0),
                rotation: 1.57,
                velocity: Vec2::new(1.0, 0.0),
                health: Some(80.0),
                max_health: Some(100.0),
                team: Team::Team(1),
                cooldowns: vec![],
            },
            visible_entities: vec![VisibleEntity {
                entity_id: EntityId(20),
                entity_type: "enemy".to_string(),
                position: Vec2::new(100.0, 100.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                distance: 55.9,
                relationship: Relationship::Hostile,
                health_fraction: Some(0.75),
            }],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec!["move".into(), "attack".into()],
            objectives: vec![],
        }
    }

    /// Helper: build an Observation with multiple entities (where TOON header folding shines).
    fn make_observation_with_many_entities() -> Observation {
        let mut obs = make_observation();
        obs.visible_entities = (0..5)
            .map(|i| VisibleEntity {
                entity_id: EntityId(20 + i),
                entity_type: format!("enemy_{}", i),
                position: Vec2::new(100.0 + i as f32 * 10.0, 100.0),
                velocity: Vec2::ZERO,
                rotation: 0.0,
                distance: 50.0 + i as f32 * 5.0,
                relationship: Relationship::Hostile,
                health_fraction: Some(0.5 + i as f32 * 0.1),
            })
            .collect();
        obs
    }

    /// Helper: build an empty Observation.
    fn make_empty_observation() -> Observation {
        Observation {
            tick: 0,
            elapsed_secs: 0.0,
            self_state: SelfState {
                agent_id: AgentId(uuid::Uuid::nil()),
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

    // --- Test 1: Simple value encoding ---

    #[test]
    fn test_to_toon_simple_value() {
        let val = json!({"name": "Alice", "score": 42});
        let result = value_to_toon(&val);
        assert!(result.is_ok(), "should encode simple object");
        let toon = result.unwrap();
        assert!(toon.contains("Alice"), "should contain the name value");
        assert!(toon.contains("42"), "should contain the score value");
    }

    // --- Test 2: Nested object encoding ---

    #[test]
    fn test_to_toon_nested_object() {
        let val = json!({
            "player": {
                "name": "Bob",
                "stats": {
                    "hp": 100,
                    "mp": 50
                }
            }
        });
        let result = value_to_toon(&val);
        assert!(result.is_ok(), "should encode nested objects");
        let toon = result.unwrap();
        assert!(toon.contains("Bob"), "should contain nested name");
        assert!(toon.contains("100"), "should contain nested hp");
        assert!(toon.contains("50"), "should contain nested mp");
    }

    // --- Test 3: Array of uniform objects (TOON header folding) ---

    #[test]
    fn test_to_toon_array_of_objects() {
        let val = json!({
            "users": [
                {"id": 1, "name": "Alice", "active": true},
                {"id": 2, "name": "Bob", "active": false},
                {"id": 3, "name": "Carol", "active": true}
            ]
        });
        let result = value_to_toon(&val);
        assert!(result.is_ok(), "should encode array of uniform objects");
        let toon = result.unwrap();
        // TOON should be more compact than JSON for uniform arrays
        let json_str = serde_json::to_string(&val).unwrap();
        assert!(
            toon.len() < json_str.len(),
            "TOON ({} bytes) should be shorter than JSON ({} bytes) for uniform arrays",
            toon.len(),
            json_str.len()
        );
    }

    // --- Test 4: Basic observation encoding ---

    #[test]
    fn test_observation_to_toon_basic() {
        let obs = make_empty_observation();
        let result = observation_to_toon(&obs);
        assert!(result.is_ok(), "should encode empty observation");
        let toon = result.unwrap();
        assert!(!toon.is_empty(), "output should not be empty");
        // Should contain tick value
        assert!(toon.contains('0'), "should contain tick number");
    }

    // --- Test 5: Observation with entities ---

    #[test]
    fn test_observation_to_toon_with_entities() {
        let obs = make_observation();
        let result = observation_to_toon(&obs);
        assert!(result.is_ok(), "should encode observation with entities");
        let toon = result.unwrap();
        assert!(toon.contains("enemy"), "should contain entity type");
        assert!(toon.contains("Hostile"), "should contain relationship");
        assert!(toon.contains("100"), "should contain tick number");
    }

    // --- Test 6: TOON is shorter than pretty-printed JSON for entity-heavy observations ---

    #[test]
    fn test_toon_is_shorter_than_json() {
        let obs = make_observation_with_many_entities();
        let toon = observation_to_toon(&obs).expect("TOON encoding should succeed");
        // Compare against pretty-printed JSON, which is what LLM prompts
        // typically use (and what TOON is designed to replace).
        let json_pretty =
            serde_json::to_string_pretty(&obs).expect("JSON pretty-print should succeed");

        assert!(
            toon.len() < json_pretty.len(),
            "TOON ({} bytes) should be shorter than pretty JSON ({} bytes) for observations with many entities",
            toon.len(),
            json_pretty.len()
        );
    }

    // --- Test 7: ToonPromptTemplate produces output ---

    #[test]
    fn test_toon_prompt_template_format_observation() {
        let template = ToonPromptTemplate::new();
        let obs = make_observation();
        let output = template.format_observation(&obs);

        assert!(!output.is_empty(), "should produce output");
        assert!(
            output.contains("TOON"),
            "should indicate TOON format in header"
        );
        assert!(output.contains("enemy"), "should contain entity data");
    }

    // --- Test 8: ToonPromptTemplate system prompt ---

    #[test]
    fn test_toon_prompt_template_system_prompt() {
        let template = ToonPromptTemplate::new();
        let prompt = template.format_system_prompt("Warrior", "melee fighter");
        assert!(prompt.contains("Warrior"), "should contain agent name");
        assert!(prompt.contains("melee fighter"), "should contain agent role");
        assert!(prompt.contains("Game Rules"), "should contain game rules");
        assert!(prompt.contains("Response Format"), "should contain format instructions");
    }

    // --- Test 9: Direct Value encoding ---

    #[test]
    fn test_value_to_toon() {
        let value = json!(["hello", "world", 42, true, null]);
        let result = value_to_toon(&value);
        assert!(result.is_ok(), "should encode mixed array");
        let toon = result.unwrap();
        assert!(toon.contains("hello"), "should contain string element");
        assert!(toon.contains("42"), "should contain number element");
    }

    // --- Test 10: Error Display ---

    #[test]
    fn test_toon_encode_error_display() {
        let ser_err = ToonEncodeError::SerializationError("test ser error".into());
        let enc_err = ToonEncodeError::EncodingError("test enc error".into());

        let ser_msg = format!("{}", ser_err);
        let enc_msg = format!("{}", enc_err);

        assert!(
            ser_msg.contains("serialization"),
            "SerializationError display should mention serialization"
        );
        assert!(
            ser_msg.contains("test ser error"),
            "should contain inner message"
        );
        assert!(
            enc_msg.contains("encoding"),
            "EncodingError display should mention encoding"
        );
        assert!(
            enc_msg.contains("test enc error"),
            "should contain inner message"
        );
    }

    // --- Test 11: to_toon generic function ---

    #[test]
    fn test_to_toon_generic() {
        // Test with a direct serde-serializable struct (Observation)
        let obs = make_observation();
        let result = to_toon(&obs);
        assert!(result.is_ok(), "should encode Observation via generic to_toon");
        let toon = result.unwrap();
        assert!(toon.contains("enemy"), "should contain entity data");
    }

    // --- Test 12: ToonPromptTemplate action instructions ---

    #[test]
    fn test_toon_prompt_template_action_instructions() {
        let template = ToonPromptTemplate::new();

        let actions = vec!["move".to_string(), "attack".to_string(), "idle".to_string()];
        let output = template.format_action_instructions(&actions);
        assert!(output.contains("Available Actions"), "should have header");
        assert!(output.contains("- move"), "should list move");
        assert!(output.contains("- attack"), "should list attack");

        let empty_output = template.format_action_instructions(&[]);
        assert!(empty_output.contains("idle"), "should suggest idle when empty");
    }

    // --- Test 13: Error is std::error::Error ---

    #[test]
    fn test_toon_encode_error_is_std_error() {
        let err: Box<dyn std::error::Error> =
            Box::new(ToonEncodeError::SerializationError("test".into()));
        // Just verify it compiles and the source is None (no inner source).
        assert!(err.source().is_none());
    }
}
