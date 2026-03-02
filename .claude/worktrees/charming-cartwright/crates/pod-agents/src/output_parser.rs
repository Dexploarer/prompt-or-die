//! Structured output parser for mapping LLM JSON responses to `Action` enums.
//!
//! Provides a trait-based parsing pipeline:
//! - [`JsonOutputParser`] — parses structured JSON action arrays
//! - [`PlainTextParser`] — extracts actions from natural language fallback
//! - [`ChainedParser`] — tries parsers in priority order until one succeeds
//!
//! All parsers are lenient: unknown actions fall back to `Action::Idle` with a warning,
//! and case-insensitive matching is used throughout.

use glam::Vec2;
use pod_core::action::{AbilityTarget, Action, SpeakVolume};
use pod_core::id::EntityId;
use serde_json::Value;
use std::fmt;

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during output parsing.
#[derive(Debug, Clone)]
pub enum ParseError {
    /// The response is not valid JSON.
    InvalidJson(String),
    /// A required field is missing from the JSON.
    MissingField(String),
    /// The action name does not match any known variant.
    InvalidAction(String),
    /// A parameter value has the wrong type or is out of range.
    InvalidParameter(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::InvalidJson(msg) => write!(f, "Invalid JSON: {}", msg),
            ParseError::MissingField(msg) => write!(f, "Missing field: {}", msg),
            ParseError::InvalidAction(msg) => write!(f, "Invalid action: {}", msg),
            ParseError::InvalidParameter(msg) => write!(f, "Invalid parameter: {}", msg),
        }
    }
}

impl std::error::Error for ParseError {}

// ---------------------------------------------------------------------------
// Parser trait
// ---------------------------------------------------------------------------

/// Trait for parsing raw LLM text responses into game actions.
pub trait OutputParser: Send + Sync {
    /// Parse a raw string response into a list of actions.
    fn parse(&self, raw_response: &str) -> Result<Vec<Action>, ParseError>;

    /// Returns a format description string that can be injected into prompts
    /// to instruct the LLM on the expected output shape.
    fn expected_format(&self) -> &str;
}

// ---------------------------------------------------------------------------
// JsonOutputParser
// ---------------------------------------------------------------------------

/// Parses structured JSON responses into `Vec<Action>`.
///
/// Accepted formats:
/// ```json
/// {"actions": [{"type": "Move", "direction": [1.0, 0.0]}, {"type": "Attack"}]}
/// ```
/// or a single action object:
/// ```json
/// {"type": "Idle"}
/// ```
pub struct JsonOutputParser;

impl JsonOutputParser {
    pub fn new() -> Self {
        Self
    }

    /// Attempt to parse a single JSON object into an `Action`.
    fn parse_action_object(obj: &Value) -> Result<Action, ParseError> {
        let action_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ParseError::MissingField("type".into()))?;

        match action_type.to_ascii_lowercase().as_str() {
            "idle" => Ok(Action::Idle),

            "move" => {
                let direction = Self::parse_vec2_field(obj, "direction")?;
                Ok(Action::Move { direction })
            }

            "stop" => Ok(Action::Stop),

            "rotate" => {
                let angle = obj
                    .get("angle")
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32)
                    .ok_or_else(|| ParseError::MissingField("angle".into()))?;
                Ok(Action::Rotate { angle })
            }

            "lookat" | "look_at" => {
                let target = Self::parse_vec2_field(obj, "target")?;
                Ok(Action::LookAt { target })
            }

            "attack" => Ok(Action::Attack),

            "attacktarget" | "attack_target" => {
                let target_id = obj
                    .get("target")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| ParseError::MissingField("target".into()))?;
                Ok(Action::AttackTarget {
                    target: EntityId(target_id),
                })
            }

            "useability" | "use_ability" => {
                let slot = obj
                    .get("slot")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u8)
                    .ok_or_else(|| ParseError::MissingField("slot".into()))?;

                let target = if let Some(target_val) = obj.get("target") {
                    if target_val.is_null() {
                        None
                    } else if let Some(arr) = target_val.as_array() {
                        // Positional target as [x, y]
                        if arr.len() == 2 {
                            let x = arr[0].as_f64().unwrap_or(0.0) as f32;
                            let y = arr[1].as_f64().unwrap_or(0.0) as f32;
                            Some(AbilityTarget::Position(Vec2::new(x, y)))
                        } else {
                            None
                        }
                    } else if let Some(id) = target_val.as_u64() {
                        Some(AbilityTarget::Entity(EntityId(id)))
                    } else if let Some(target_obj) = target_val.as_object() {
                        // Handle {"type": "position", "value": [x,y]} style
                        let kind = target_obj
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("position");
                        match kind.to_ascii_lowercase().as_str() {
                            "entity" => {
                                let id = target_obj
                                    .get("value")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                Some(AbilityTarget::Entity(EntityId(id)))
                            }
                            "direction" => {
                                let val = target_obj.get("value");
                                let dir = Self::parse_vec2_from_value(val)?;
                                Some(AbilityTarget::Direction(dir))
                            }
                            _ => {
                                let val = target_obj.get("value");
                                let pos = Self::parse_vec2_from_value(val)?;
                                Some(AbilityTarget::Position(pos))
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                Ok(Action::UseAbility { slot, target })
            }

            "interact" => Ok(Action::Interact),

            "interactwith" | "interact_with" => {
                let target_id = obj
                    .get("target")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| ParseError::MissingField("target".into()))?;
                Ok(Action::InteractWith {
                    target: EntityId(target_id),
                })
            }

            "pickup" | "pick_up" => {
                let target_id = obj
                    .get("target")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| ParseError::MissingField("target".into()))?;
                Ok(Action::Pickup {
                    target: EntityId(target_id),
                })
            }

            "drop" => {
                let slot = obj
                    .get("slot")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u8)
                    .ok_or_else(|| ParseError::MissingField("slot".into()))?;
                Ok(Action::Drop { slot })
            }

            "useitem" | "use_item" => {
                let slot = obj
                    .get("slot")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u8)
                    .ok_or_else(|| ParseError::MissingField("slot".into()))?;
                Ok(Action::UseItem { slot })
            }

            "speak" => {
                let message = obj
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();

                let volume = obj
                    .get("volume")
                    .and_then(|v| v.as_str())
                    .map(|s| match s.to_ascii_lowercase().as_str() {
                        "whisper" => SpeakVolume::Whisper,
                        "shout" => SpeakVolume::Shout,
                        _ => SpeakVolume::Normal,
                    })
                    .unwrap_or(SpeakVolume::Normal);

                Ok(Action::Speak { message, volume })
            }

            "signal" => {
                let signal_type = obj
                    .get("signal_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ping")
                    .to_string();
                let data = obj
                    .get("data")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                Ok(Action::Signal { signal_type, data })
            }

            "spawn" => {
                let prefab = obj
                    .get("prefab")
                    .and_then(|v| v.as_str())
                    .unwrap_or("default")
                    .to_string();
                let position = Self::parse_vec2_field(obj, "position").unwrap_or(Vec2::ZERO);
                Ok(Action::Spawn { prefab, position })
            }

            unknown => {
                log::warn!(
                    "Unknown action type '{}', falling back to Idle",
                    unknown
                );
                Ok(Action::Idle)
            }
        }
    }

    /// Extract a Vec2 from a JSON field that is either `[x, y]` or `{"x": ..., "y": ...}`.
    fn parse_vec2_field(obj: &Value, field: &str) -> Result<Vec2, ParseError> {
        let val = obj.get(field);
        Self::parse_vec2_from_value(val)
            .map_err(|_| ParseError::MissingField(field.into()))
    }

    fn parse_vec2_from_value(val: Option<&Value>) -> Result<Vec2, ParseError> {
        let val = val.ok_or_else(|| ParseError::InvalidParameter("missing vec2 value".into()))?;

        if let Some(arr) = val.as_array() {
            if arr.len() >= 2 {
                let x = arr[0]
                    .as_f64()
                    .ok_or_else(|| ParseError::InvalidParameter("vec2[0] not a number".into()))?
                    as f32;
                let y = arr[1]
                    .as_f64()
                    .ok_or_else(|| ParseError::InvalidParameter("vec2[1] not a number".into()))?
                    as f32;
                return Ok(Vec2::new(x, y));
            }
        }

        if let Some(obj) = val.as_object() {
            let x = obj
                .get("x")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| ParseError::InvalidParameter("vec2.x not a number".into()))?
                as f32;
            let y = obj
                .get("y")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| ParseError::InvalidParameter("vec2.y not a number".into()))?
                as f32;
            return Ok(Vec2::new(x, y));
        }

        Err(ParseError::InvalidParameter(
            "Expected array [x,y] or object {x,y}".into(),
        ))
    }

    /// Attempt to strip markdown code fences and extract JSON from a response.
    fn extract_json(raw: &str) -> &str {
        let trimmed = raw.trim();

        // Strip ```json ... ``` or ``` ... ```
        if let Some(rest) = trimmed.strip_prefix("```json") {
            if let Some(inner) = rest.strip_suffix("```") {
                return inner.trim();
            }
        }
        if let Some(rest) = trimmed.strip_prefix("```") {
            if let Some(inner) = rest.strip_suffix("```") {
                return inner.trim();
            }
        }

        trimmed
    }
}

impl Default for JsonOutputParser {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputParser for JsonOutputParser {
    fn parse(&self, raw_response: &str) -> Result<Vec<Action>, ParseError> {
        let cleaned = Self::extract_json(raw_response);

        let root: Value = serde_json::from_str(cleaned)
            .map_err(|e| ParseError::InvalidJson(e.to_string()))?;

        // Format 1: {"actions": [...]}
        if let Some(actions_arr) = root.get("actions").and_then(|v| v.as_array()) {
            let mut actions = Vec::with_capacity(actions_arr.len());
            for item in actions_arr {
                match Self::parse_action_object(item) {
                    Ok(action) => actions.push(action),
                    Err(e) => {
                        log::warn!("Skipping malformed action in array: {}", e);
                        // Skip individual bad actions rather than failing the whole batch
                    }
                }
            }
            if actions.is_empty() {
                actions.push(Action::Idle);
            }
            return Ok(actions);
        }

        // Format 2: single action object {"type": "..."}
        if root.get("type").is_some() {
            let action = Self::parse_action_object(&root)?;
            return Ok(vec![action]);
        }

        Err(ParseError::InvalidJson(
            "Expected {\"actions\": [...]} or {\"type\": \"...\"}".into(),
        ))
    }

    fn expected_format(&self) -> &str {
        r#"Respond with JSON in one of these formats:

Multiple actions:
{"actions": [{"type": "Move", "direction": [1.0, 0.0]}, {"type": "Attack"}]}

Single action:
{"type": "Idle"}

Available action types and their parameters:
- Idle (no params)
- Move { direction: [x, y] }
- Stop (no params)
- Rotate { angle: <radians> }
- LookAt { target: [x, y] }
- Attack (no params)
- AttackTarget { target: <entity_id> }
- UseAbility { slot: <0-255>, target?: [x, y] | <entity_id> | null }
- Interact (no params)
- InteractWith { target: <entity_id> }
- Pickup { target: <entity_id> }
- Drop { slot: <0-255> }
- UseItem { slot: <0-255> }
- Speak { message: "<text>", volume?: "whisper"|"normal"|"shout" }
- Signal { signal_type: "<type>", data: "<data>" }
"#
    }
}

// ---------------------------------------------------------------------------
// PlainTextParser
// ---------------------------------------------------------------------------

/// Fallback parser that extracts actions from natural language responses.
///
/// Looks for keywords like "move north", "attack", "stop", "speak: hello" etc.
/// Returns `Action::Idle` if nothing recognizable is found.
pub struct PlainTextParser;

impl PlainTextParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse a cardinal/ordinal direction string to a unit Vec2.
    fn direction_from_keyword(word: &str) -> Option<Vec2> {
        match word {
            "north" | "up" => Some(Vec2::new(0.0, 1.0)),
            "south" | "down" => Some(Vec2::new(0.0, -1.0)),
            "east" | "right" => Some(Vec2::new(1.0, 0.0)),
            "west" | "left" => Some(Vec2::new(-1.0, 0.0)),
            "northeast" | "up-right" | "upright" => {
                Some(Vec2::new(1.0, 1.0).normalize())
            }
            "northwest" | "up-left" | "upleft" => {
                Some(Vec2::new(-1.0, 1.0).normalize())
            }
            "southeast" | "down-right" | "downright" => {
                Some(Vec2::new(1.0, -1.0).normalize())
            }
            "southwest" | "down-left" | "downleft" => {
                Some(Vec2::new(-1.0, -1.0).normalize())
            }
            _ => None,
        }
    }
}

impl Default for PlainTextParser {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputParser for PlainTextParser {
    fn parse(&self, raw_response: &str) -> Result<Vec<Action>, ParseError> {
        let lower = raw_response.to_ascii_lowercase();
        let lower = lower.trim();
        let mut actions = Vec::new();

        // Check for "speak: <message>" or "say: <message>"
        for prefix in &["speak:", "say:"] {
            if let Some(rest) = lower.strip_prefix(prefix) {
                let message = rest.trim().to_string();
                actions.push(Action::Speak {
                    message,
                    volume: SpeakVolume::Normal,
                });
            }
        }
        // Also check the original case for speak/say to preserve message casing
        if actions.is_empty() {
            let trimmed = raw_response.trim();
            for prefix in &["speak:", "Speak:", "say:", "Say:", "SPEAK:", "SAY:"] {
                if let Some(rest) = trimmed.strip_prefix(prefix) {
                    let message = rest.trim().to_string();
                    actions.push(Action::Speak {
                        message,
                        volume: SpeakVolume::Normal,
                    });
                    break;
                }
            }
        }

        if !actions.is_empty() {
            return Ok(actions);
        }

        // Check for movement keywords
        let words: Vec<&str> = lower.split_whitespace().collect();
        let has_move = words.contains(&"move") || words.contains(&"walk") || words.contains(&"go");

        if has_move {
            // Look for a direction keyword after "move"
            for word in &words {
                if let Some(dir) = Self::direction_from_keyword(word) {
                    actions.push(Action::Move { direction: dir });
                    break;
                }
            }
            // "move" without a direction — try to find any direction keyword
            if actions.is_empty() {
                // Default forward
                actions.push(Action::Move {
                    direction: Vec2::new(0.0, 1.0),
                });
            }
        }

        // Check for simple keyword actions
        if lower.contains("attack") || lower.contains("fight") || lower.contains("hit") {
            actions.push(Action::Attack);
        }
        if lower.contains("stop") || lower.contains("halt") {
            actions.push(Action::Stop);
        }
        if lower.contains("interact") || lower.contains("use") || lower.contains("activate") {
            actions.push(Action::Interact);
        }
        if lower == "idle" || lower.contains("wait") || lower.contains("do nothing") {
            actions.push(Action::Idle);
        }

        if actions.is_empty() {
            // Nothing recognizable — default to Idle
            log::debug!(
                "PlainTextParser: no action keywords found in response, defaulting to Idle"
            );
            actions.push(Action::Idle);
        }

        Ok(actions)
    }

    fn expected_format(&self) -> &str {
        r#"Respond with your intended action in plain text. Examples:
- "move north"
- "attack"
- "stop"
- "idle"
- "interact"
- "speak: Hello there!"
"#
    }
}

// ---------------------------------------------------------------------------
// ChainedParser
// ---------------------------------------------------------------------------

/// Tries a sequence of parsers in order, returning the first successful result.
///
/// The default chain is `JsonOutputParser → PlainTextParser`, ensuring that
/// structured responses are preferred but unstructured text still works.
pub struct ChainedParser {
    parsers: Vec<Box<dyn OutputParser>>,
}

impl ChainedParser {
    /// Create a new `ChainedParser` with the given parsers tried in order.
    pub fn new(parsers: Vec<Box<dyn OutputParser>>) -> Self {
        Self { parsers }
    }

    /// Default chain: JSON first, then plain text fallback.
    pub fn default_chain() -> Self {
        Self {
            parsers: vec![
                Box::new(JsonOutputParser::new()),
                Box::new(PlainTextParser::new()),
            ],
        }
    }
}

impl OutputParser for ChainedParser {
    fn parse(&self, raw_response: &str) -> Result<Vec<Action>, ParseError> {
        let mut last_error = ParseError::InvalidJson("No parsers configured".into());

        for parser in &self.parsers {
            match parser.parse(raw_response) {
                Ok(actions) => return Ok(actions),
                Err(e) => {
                    log::debug!("Parser failed, trying next: {}", e);
                    last_error = e;
                }
            }
        }

        Err(last_error)
    }

    fn expected_format(&self) -> &str {
        // Return the format of the first (preferred) parser
        self.parsers
            .first()
            .map(|p| p.expected_format())
            .unwrap_or("Respond with your intended action.")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // JsonOutputParser tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_json_multiple_actions() {

        let parser = JsonOutputParser::new();
        let input = r#"{"actions": [
            {"type": "Move", "direction": [1.0, 0.0]},
            {"type": "Attack"},
            {"type": "Idle"}
        ]}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 3);
        assert!(matches!(&actions[0], Action::Move { direction } if *direction == Vec2::new(1.0, 0.0)));
        assert!(matches!(&actions[1], Action::Attack));
        assert!(matches!(&actions[2], Action::Idle));
    }

    #[test]
    fn test_json_single_action() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "Stop"}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Stop));
    }

    #[test]
    fn test_json_invalid_json() {

        let parser = JsonOutputParser::new();
        let input = "this is not json at all";

        let result = parser.parse(input);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ParseError::InvalidJson(_)));
    }

    #[test]
    fn test_json_missing_type_field() {

        let parser = JsonOutputParser::new();
        // Valid JSON but missing "type" in one action
        let input = r#"{"actions": [{"direction": [1.0, 0.0]}]}"#;

        // Malformed actions are skipped, and if all are skipped we get Idle
        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Idle));
    }

    #[test]
    fn test_json_case_insensitive() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "IDLE"}"#;
        let actions = parser.parse(input).unwrap();
        assert!(matches!(&actions[0], Action::Idle));

        let input2 = r#"{"type": "mOvE", "direction": [0.0, 1.0]}"#;
        let actions2 = parser.parse(input2).unwrap();
        assert!(matches!(&actions2[0], Action::Move { .. }));
    }

    #[test]
    fn test_json_unknown_action_falls_back_to_idle() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "FlyToTheMoon"}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Idle));
    }

    #[test]
    fn test_json_speak_with_message() {

        let parser = JsonOutputParser::new();
        let input =
            r#"{"type": "Speak", "message": "Hello, friend!", "volume": "whisper"}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Speak { message, volume } => {
                assert_eq!(message, "Hello, friend!");
                assert!(matches!(volume, SpeakVolume::Whisper));
            }
            other => panic!("Expected Speak, got {:?}", other),
        }
    }

    #[test]
    fn test_json_move_with_object_direction() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "Move", "direction": {"x": -1.0, "y": 0.5}}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Move { direction } => {
                assert!((direction.x - (-1.0)).abs() < f32::EPSILON);
                assert!((direction.y - 0.5).abs() < f32::EPSILON);
            }
            other => panic!("Expected Move, got {:?}", other),
        }
    }

    #[test]
    fn test_json_attack_target() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "attack_target", "target": 42}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::AttackTarget { target } => {
                assert_eq!(target.0, 42);
            }
            other => panic!("Expected AttackTarget, got {:?}", other),
        }
    }

    #[test]
    fn test_json_use_ability_with_position_target() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "UseAbility", "slot": 2, "target": [100.0, 200.0]}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::UseAbility { slot, target } => {
                assert_eq!(*slot, 2);
                match target {
                    Some(AbilityTarget::Position(pos)) => {
                        assert!((pos.x - 100.0).abs() < f32::EPSILON);
                        assert!((pos.y - 200.0).abs() < f32::EPSILON);
                    }
                    other => panic!("Expected Position target, got {:?}", other),
                }
            }
            other => panic!("Expected UseAbility, got {:?}", other),
        }
    }

    #[test]
    fn test_json_drop_action() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "Drop", "slot": 3}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Drop { slot } => assert_eq!(*slot, 3),
            other => panic!("Expected Drop, got {:?}", other),
        }
    }

    #[test]
    fn test_json_signal_action() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "Signal", "signal_type": "danger", "data": "north sector"}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Signal {
                signal_type,
                data,
            } => {
                assert_eq!(signal_type, "danger");
                assert_eq!(data, "north sector");
            }
            other => panic!("Expected Signal, got {:?}", other),
        }
    }

    #[test]
    fn test_json_rotate_action() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "Rotate", "angle": 1.57}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Rotate { angle } => {
                assert!((angle - 1.57).abs() < 0.01);
            }
            other => panic!("Expected Rotate, got {:?}", other),
        }
    }

    #[test]
    fn test_json_lookat_action() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "look_at", "target": [50.0, 75.0]}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::LookAt { target } => {
                assert!((target.x - 50.0).abs() < f32::EPSILON);
                assert!((target.y - 75.0).abs() < f32::EPSILON);
            }
            other => panic!("Expected LookAt, got {:?}", other),
        }
    }

    #[test]
    fn test_json_with_code_fences() {

        let parser = JsonOutputParser::new();
        let input = "```json\n{\"type\": \"Idle\"}\n```";

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Idle));
    }

    #[test]
    fn test_json_empty_actions_array() {

        let parser = JsonOutputParser::new();
        let input = r#"{"actions": []}"#;

        // Empty array should yield Idle
        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Idle));
    }

    #[test]
    fn test_json_no_actions_or_type() {

        let parser = JsonOutputParser::new();
        let input = r#"{"reasoning": "I should wait"}"#;

        let result = parser.parse(input);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // PlainTextParser tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_plain_text_move_north() {

        let parser = PlainTextParser::new();
        let input = "I'll move north to explore.";

        let actions = parser.parse(input).unwrap();
        assert!(actions.iter().any(|a| matches!(a, Action::Move { direction } if direction.y > 0.0)));
    }

    #[test]
    fn test_plain_text_move_south() {

        let parser = PlainTextParser::new();
        let input = "move south";

        let actions = parser.parse(input).unwrap();
        assert!(actions.iter().any(|a| matches!(a, Action::Move { direction } if direction.y < 0.0)));
    }

    #[test]
    fn test_plain_text_attack() {

        let parser = PlainTextParser::new();
        let input = "I should attack the enemy";

        let actions = parser.parse(input).unwrap();
        assert!(actions.iter().any(|a| matches!(a, Action::Attack)));
    }

    #[test]
    fn test_plain_text_stop() {

        let parser = PlainTextParser::new();
        let input = "stop right there";

        let actions = parser.parse(input).unwrap();
        assert!(actions.iter().any(|a| matches!(a, Action::Stop)));
    }

    #[test]
    fn test_plain_text_speak() {

        let parser = PlainTextParser::new();
        let input = "speak: Hello, how are you?";

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Speak { message, .. } => {
                assert_eq!(message, "hello, how are you?");
            }
            other => panic!("Expected Speak, got {:?}", other),
        }
    }

    #[test]
    fn test_plain_text_no_keywords() {

        let parser = PlainTextParser::new();
        let input = "The weather is nice today.";

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Idle));
    }

    #[test]
    fn test_plain_text_idle() {

        let parser = PlainTextParser::new();
        let input = "I'll wait and observe";

        let actions = parser.parse(input).unwrap();
        assert!(actions.iter().any(|a| matches!(a, Action::Idle)));
    }

    #[test]
    fn test_plain_text_direction_east() {

        let parser = PlainTextParser::new();
        let input = "go east";

        let actions = parser.parse(input).unwrap();
        assert!(actions.iter().any(|a| matches!(a, Action::Move { direction } if direction.x > 0.0)));
    }

    #[test]
    fn test_plain_text_direction_west() {

        let parser = PlainTextParser::new();
        let input = "move west";

        let actions = parser.parse(input).unwrap();
        assert!(actions.iter().any(|a| matches!(a, Action::Move { direction } if direction.x < 0.0)));
    }

    // -----------------------------------------------------------------------
    // ChainedParser tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_chained_parser_json_succeeds() {

        let parser = ChainedParser::default_chain();
        let input = r#"{"type": "Attack"}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Attack));
    }

    #[test]
    fn test_chained_parser_falls_back_to_plain_text() {

        let parser = ChainedParser::default_chain();
        let input = "I want to move north and explore the area.";

        // JSON parser should fail, then PlainTextParser should succeed
        let actions = parser.parse(input).unwrap();
        assert!(actions.iter().any(|a| matches!(a, Action::Move { .. })));
    }

    #[test]
    fn test_chained_parser_empty_response() {

        let parser = ChainedParser::default_chain();
        let input = "";

        // JSON fails on empty string, plain text defaults to Idle
        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Idle));
    }

    #[test]
    fn test_chained_parser_expected_format() {
        let parser = ChainedParser::default_chain();
        let format = parser.expected_format();
        // Should return the JSON parser's format (first in chain)
        assert!(format.contains("actions"));
        assert!(format.contains("type"));
    }

    #[test]
    fn test_chained_parser_gibberish() {

        let parser = ChainedParser::default_chain();
        let input = "asdkfjasdlkfj asdkfjasd";

        // Should ultimately default to Idle via PlainTextParser
        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], Action::Idle));
    }

    // -----------------------------------------------------------------------
    // Edge case / integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_json_mixed_valid_and_invalid_actions() {

        let parser = JsonOutputParser::new();
        let input = r#"{"actions": [
            {"type": "Move", "direction": [0.0, 1.0]},
            {"type": "Attack"},
            {"direction": [1.0, 0.0]}
        ]}"#;

        // The third action is missing "type", so it gets skipped
        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 2);
        assert!(matches!(&actions[0], Action::Move { .. }));
        assert!(matches!(&actions[1], Action::Attack));
    }

    #[test]
    fn test_json_interact_with() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "interact_with", "target": 7}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::InteractWith { target } => assert_eq!(target.0, 7),
            other => panic!("Expected InteractWith, got {:?}", other),
        }
    }

    #[test]
    fn test_json_use_ability_no_target() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "use_ability", "slot": 0}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::UseAbility { slot, target } => {
                assert_eq!(*slot, 0);
                assert!(target.is_none());
            }
            other => panic!("Expected UseAbility, got {:?}", other),
        }
    }

    #[test]
    fn test_json_use_ability_null_target() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "use_ability", "slot": 1, "target": null}"#;

        let actions = parser.parse(input).unwrap();
        match &actions[0] {
            Action::UseAbility { slot, target } => {
                assert_eq!(*slot, 1);
                assert!(target.is_none());
            }
            other => panic!("Expected UseAbility, got {:?}", other),
        }
    }

    #[test]
    fn test_json_spawn_action() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "Spawn", "prefab": "goblin", "position": [10.0, 20.0]}"#;

        let actions = parser.parse(input).unwrap();
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            Action::Spawn { prefab, position } => {
                assert_eq!(prefab, "goblin");
                assert!((position.x - 10.0).abs() < f32::EPSILON);
                assert!((position.y - 20.0).abs() < f32::EPSILON);
            }
            other => panic!("Expected Spawn, got {:?}", other),
        }
    }

    #[test]
    fn test_json_speak_default_volume() {

        let parser = JsonOutputParser::new();
        let input = r#"{"type": "Speak", "message": "Help!"}"#;

        let actions = parser.parse(input).unwrap();
        match &actions[0] {
            Action::Speak { message, volume } => {
                assert_eq!(message, "Help!");
                assert!(matches!(volume, SpeakVolume::Normal));
            }
            other => panic!("Expected Speak, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_error_display() {
        let err = ParseError::InvalidJson("unexpected EOF".into());
        assert_eq!(format!("{}", err), "Invalid JSON: unexpected EOF");

        let err = ParseError::MissingField("type".into());
        assert_eq!(format!("{}", err), "Missing field: type");

        let err = ParseError::InvalidAction("Fly".into());
        assert_eq!(format!("{}", err), "Invalid action: Fly");

        let err = ParseError::InvalidParameter("negative slot".into());
        assert_eq!(format!("{}", err), "Invalid parameter: negative slot");
    }
}
