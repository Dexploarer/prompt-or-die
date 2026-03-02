//! Structured output parsing: LLM response text → Vec<Action>.
//!
//! Supports multiple response formats from LLMs:
//! - JSON: `{"actions": [...], "reasoning": "..."}`
//! - Key-value: `ACTION: move up\nREASON: ...`
//! - Natural language: regex extraction from free-form text
//!
//! All parsers are fallible — unknown or malformed responses degrade gracefully to Idle.

use pod_core::action::{Action, AbilityTarget, SpeakVolume};
use pod_core::id::EntityId;
use glam::Vec2;
use serde::{Deserialize, Serialize};
use log::{debug, warn};

// ============================================================
// PARSE RESULT
// ============================================================

/// Result of parsing an LLM response into game actions.
#[derive(Debug, Clone)]
pub struct ActionParseResult {
    /// Parsed actions (always at least one — defaults to Idle)
    pub actions: Vec<Action>,
    /// Reasoning extracted from the response (if available)
    pub reasoning: String,
    /// Parse confidence (1.0 = clean JSON parse, lower = fuzzy match)
    pub confidence: f32,
    /// The raw response string
    pub raw_response: String,
    /// Any parse warnings (partial failures, unknown actions, etc.)
    pub warnings: Vec<String>,
}

/// Errors that can occur during action parsing.
#[derive(Debug, Clone)]
pub enum ActionParseError {
    /// Response was completely unparseable
    InvalidFormat(String),
    /// Response was empty
    EmptyResponse,
}

impl std::fmt::Display for ActionParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ActionParseError::InvalidFormat(msg) => write!(f, "Invalid format: {}", msg),
            ActionParseError::EmptyResponse => write!(f, "Empty response"),
        }
    }
}

impl std::error::Error for ActionParseError {}

// ============================================================
// PARSER TRAIT
// ============================================================

/// Trait for parsing LLM text responses into game actions.
pub trait ActionParser: Send + Sync {
    /// Parse a response string into actions.
    /// Should never panic — always returns a result (possibly with Idle fallback).
    fn parse(&self, response: &str) -> Result<ActionParseResult, ActionParseError>;

    /// Parser identifier
    fn name(&self) -> &str;
}

// ============================================================
// JSON PARSER (primary)
// ============================================================

/// Parses JSON responses like `{"actions": ["move up", "attack"], "reasoning": "..."}`.
/// This is the default and recommended parser.
pub struct JsonActionParser;

/// Intermediate JSON structure from LLM
#[derive(Deserialize)]
struct LlmJsonResponse {
    actions: Option<Vec<serde_json::Value>>,
    reasoning: Option<String>,
    #[serde(alias = "reason")]
    reason_alt: Option<String>,
}

impl ActionParser for JsonActionParser {
    fn parse(&self, response: &str) -> Result<ActionParseResult, ActionParseError> {
        let response = response.trim();
        if response.is_empty() {
            return Err(ActionParseError::EmptyResponse);
        }

        // Try to extract JSON from response (LLMs sometimes wrap in markdown)
        let json_str = extract_json(response).unwrap_or(response);

        let parsed: LlmJsonResponse = serde_json::from_str(json_str).map_err(|e| {
            ActionParseError::InvalidFormat(format!("JSON parse failed: {}", e))
        })?;

        let reasoning = parsed
            .reasoning
            .or(parsed.reason_alt)
            .unwrap_or_else(|| "No reasoning provided".to_string());

        let mut actions = Vec::new();
        let mut warnings = Vec::new();

        if let Some(action_values) = parsed.actions {
            for val in &action_values {
                match val {
                    serde_json::Value::String(s) => match parse_action_string(s) {
                        Ok(action) => actions.push(action),
                        Err(e) => warnings.push(format!("Unknown action '{}': {}", s, e)),
                    },
                    serde_json::Value::Object(obj) => match parse_action_object(obj) {
                        Ok(action) => actions.push(action),
                        Err(e) => warnings.push(format!("Invalid action object: {}", e)),
                    },
                    _ => warnings.push(format!("Unexpected action value: {}", val)),
                }
            }
        }

        if actions.is_empty() {
            actions.push(Action::Idle);
        }

        let confidence = if warnings.is_empty() { 1.0 } else { 0.7 };

        Ok(ActionParseResult {
            actions,
            reasoning,
            confidence,
            raw_response: response.to_string(),
            warnings,
        })
    }

    fn name(&self) -> &str {
        "json"
    }
}

// ============================================================
// KEY-VALUE PARSER
// ============================================================

/// Parses line-oriented responses like:
/// ```text
/// ACTION: move up
/// ACTION: attack
/// REASON: Enemy is close
/// ```
pub struct KeyValueParser;

impl ActionParser for KeyValueParser {
    fn parse(&self, response: &str) -> Result<ActionParseResult, ActionParseError> {
        let response = response.trim();
        if response.is_empty() {
            return Err(ActionParseError::EmptyResponse);
        }

        let mut actions = Vec::new();
        let mut reasoning = String::new();
        let mut warnings = Vec::new();

        for line in response.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(action_str) = line
                .strip_prefix("ACTION:")
                .or_else(|| line.strip_prefix("action:"))
                .or_else(|| line.strip_prefix("Action:"))
            {
                match parse_action_string(action_str.trim()) {
                    Ok(action) => actions.push(action),
                    Err(e) => warnings.push(format!("Unknown action '{}': {}", action_str.trim(), e)),
                }
            } else if let Some(reason) = line
                .strip_prefix("REASON:")
                .or_else(|| line.strip_prefix("reason:"))
                .or_else(|| line.strip_prefix("Reason:")
                .or_else(|| line.strip_prefix("REASONING:")))
            {
                reasoning = reason.trim().to_string();
            }
        }

        if actions.is_empty() {
            actions.push(Action::Idle);
        }

        let confidence = if warnings.is_empty() { 0.9 } else { 0.6 };

        Ok(ActionParseResult {
            actions,
            reasoning,
            confidence,
            raw_response: response.to_string(),
            warnings,
        })
    }

    fn name(&self) -> &str {
        "key-value"
    }
}

// ============================================================
// FALLBACK CHAIN PARSER
// ============================================================

/// Tries multiple parsers in order, using the first successful result.
/// Falls back to Idle if all parsers fail.
pub struct FallbackParser {
    parsers: Vec<Box<dyn ActionParser>>,
}

impl FallbackParser {
    pub fn new(parsers: Vec<Box<dyn ActionParser>>) -> Self {
        Self { parsers }
    }

    /// Default chain: JSON → KeyValue → Idle
    pub fn default_chain() -> Self {
        Self::new(vec![
            Box::new(JsonActionParser),
            Box::new(KeyValueParser),
        ])
    }
}

impl ActionParser for FallbackParser {
    fn parse(&self, response: &str) -> Result<ActionParseResult, ActionParseError> {
        for parser in &self.parsers {
            match parser.parse(response) {
                Ok(result) if result.confidence > 0.5 => {
                    debug!("Parser '{}' succeeded with confidence {:.2}", parser.name(), result.confidence);
                    return Ok(result);
                }
                Ok(result) => {
                    debug!("Parser '{}' low confidence {:.2}, trying next", parser.name(), result.confidence);
                }
                Err(e) => {
                    debug!("Parser '{}' failed: {}, trying next", parser.name(), e);
                }
            }
        }

        // All parsers failed — return Idle
        warn!("All parsers failed for response, defaulting to Idle");
        Ok(ActionParseResult {
            actions: vec![Action::Idle],
            reasoning: "All parsers failed — defaulting to idle".to_string(),
            confidence: 0.0,
            raw_response: response.to_string(),
            warnings: vec!["All parsers failed".to_string()],
        })
    }

    fn name(&self) -> &str {
        "fallback-chain"
    }
}

// ============================================================
// ACTION STRING PARSING
// ============================================================

/// Parse a single action from a string like "move up", "attack", "idle".
pub fn parse_action_string(s: &str) -> Result<Action, String> {
    let s = s.trim().to_lowercase();

    if s == "idle" || s == "wait" || s == "observe" {
        return Ok(Action::Idle);
    }
    if s == "stop" || s == "halt" {
        return Ok(Action::Stop);
    }
    if s == "attack" || s == "strike" || s == "hit" {
        return Ok(Action::Attack);
    }
    if s == "interact" || s == "use" || s == "activate" {
        return Ok(Action::Interact);
    }

    // "move <direction>"
    if let Some(dir_str) = s.strip_prefix("move ") {
        let direction = parse_direction(dir_str.trim())?;
        return Ok(Action::Move { direction });
    }

    // "rotate <angle>"
    if let Some(angle_str) = s.strip_prefix("rotate ") {
        let angle: f32 = angle_str
            .trim()
            .parse()
            .map_err(|_| format!("Invalid angle: {}", angle_str))?;
        return Ok(Action::Rotate { angle });
    }

    // "look at (x, y)"
    if let Some(coords) = s.strip_prefix("look at ") {
        let target = parse_vec2(coords.trim())?;
        return Ok(Action::LookAt { target });
    }

    // "attack target <id>"
    if let Some(id_str) = s.strip_prefix("attack target ") {
        let id: u64 = id_str
            .trim()
            .parse()
            .map_err(|_| format!("Invalid entity id: {}", id_str))?;
        return Ok(Action::AttackTarget {
            target: EntityId(id),
        });
    }

    // "speak <message>"
    if let Some(msg) = s.strip_prefix("speak ").or_else(|| s.strip_prefix("say ")) {
        return Ok(Action::Speak {
            message: msg.to_string(),
            volume: SpeakVolume::Normal,
        });
    }

    // "whisper <message>"
    if let Some(msg) = s.strip_prefix("whisper ") {
        return Ok(Action::Speak {
            message: msg.to_string(),
            volume: SpeakVolume::Whisper,
        });
    }

    // "shout <message>"
    if let Some(msg) = s.strip_prefix("shout ") {
        return Ok(Action::Speak {
            message: msg.to_string(),
            volume: SpeakVolume::Shout,
        });
    }

    // "drop" / "drop <slot>"
    if s == "drop" {
        return Ok(Action::Drop { slot: 0 });
    }
    if let Some(slot_str) = s.strip_prefix("drop ") {
        let slot: u8 = slot_str
            .trim()
            .parse()
            .map_err(|_| format!("Invalid slot: {}", slot_str))?;
        return Ok(Action::Drop { slot });
    }

    // "use item <slot>"
    if let Some(slot_str) = s.strip_prefix("use item ") {
        let slot: u8 = slot_str
            .trim()
            .parse()
            .map_err(|_| format!("Invalid slot: {}", slot_str))?;
        return Ok(Action::UseItem { slot });
    }

    // "ability <slot>"
    if let Some(slot_str) = s.strip_prefix("ability ").or_else(|| s.strip_prefix("use ability ")) {
        let slot: u8 = slot_str
            .trim()
            .parse()
            .map_err(|_| format!("Invalid ability slot: {}", slot_str))?;
        return Ok(Action::UseAbility { slot, target: None });
    }

    // "signal <type> <data>"
    if let Some(rest) = s.strip_prefix("signal ") {
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        return Ok(Action::Signal {
            signal_type: parts[0].to_string(),
            data: parts.get(1).unwrap_or(&"").to_string(),
        });
    }

    // Unknown — not an error, but log it
    debug!("Unknown action string '{}', defaulting to Idle", s);
    Ok(Action::Idle)
}

/// Parse an action from a JSON object (e.g., `{"type": "move", "direction": "north"}`)
fn parse_action_object(obj: &serde_json::Map<String, serde_json::Value>) -> Result<Action, String> {
    let action_type = obj
        .get("type")
        .or_else(|| obj.get("action"))
        .and_then(|v| v.as_str())
        .ok_or("Missing 'type' field")?;

    match action_type.to_lowercase().as_str() {
        "idle" | "wait" => Ok(Action::Idle),
        "stop" => Ok(Action::Stop),
        "attack" => Ok(Action::Attack),
        "interact" => Ok(Action::Interact),
        "move" => {
            if let Some(dir_str) = obj.get("direction").and_then(|v| v.as_str()) {
                let direction = parse_direction(dir_str)?;
                Ok(Action::Move { direction })
            } else if let (Some(x), Some(y)) = (
                obj.get("x").and_then(|v| v.as_f64()),
                obj.get("y").and_then(|v| v.as_f64()),
            ) {
                Ok(Action::Move {
                    direction: Vec2::new(x as f32, y as f32).normalize_or_zero(),
                })
            } else {
                Err("Move action missing 'direction' or 'x'/'y'".to_string())
            }
        }
        "rotate" => {
            let angle = obj
                .get("angle")
                .and_then(|v| v.as_f64())
                .ok_or("Rotate missing 'angle'")?;
            Ok(Action::Rotate {
                angle: angle as f32,
            })
        }
        "speak" | "say" => {
            let message = obj
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Ok(Action::Speak {
                message,
                volume: SpeakVolume::Normal,
            })
        }
        _ => Err(format!("Unknown action type: {}", action_type)),
    }
}

// ============================================================
// HELPER FUNCTIONS
// ============================================================

/// Parse direction strings: "north", "up", "up-right", etc.
pub fn parse_direction(s: &str) -> Result<Vec2, String> {
    let s = s.trim().to_lowercase();
    match s.as_str() {
        "up" | "north" | "n" | "forward" => Ok(Vec2::new(0.0, -1.0)),
        "down" | "south" | "s" | "backward" | "back" => Ok(Vec2::new(0.0, 1.0)),
        "left" | "west" | "w" => Ok(Vec2::new(-1.0, 0.0)),
        "right" | "east" | "e" => Ok(Vec2::new(1.0, 0.0)),
        "up-left" | "north-west" | "nw" => Ok(Vec2::new(-1.0, -1.0).normalize()),
        "up-right" | "north-east" | "ne" => Ok(Vec2::new(1.0, -1.0).normalize()),
        "down-left" | "south-west" | "sw" => Ok(Vec2::new(-1.0, 1.0).normalize()),
        "down-right" | "south-east" | "se" => Ok(Vec2::new(1.0, 1.0).normalize()),
        _ => {
            // Try parsing as "x,y" coordinates
            let parts: Vec<&str> = s.split(',').collect();
            if parts.len() == 2 {
                let x: f32 = parts[0].trim().parse().map_err(|_| format!("Invalid x: {}", parts[0]))?;
                let y: f32 = parts[1].trim().parse().map_err(|_| format!("Invalid y: {}", parts[1]))?;
                Ok(Vec2::new(x, y).normalize_or_zero())
            } else {
                Err(format!("Unknown direction: {}", s))
            }
        }
    }
}

/// Parse "(x, y)" into Vec2
pub fn parse_vec2(s: &str) -> Result<Vec2, String> {
    let s = s.trim().trim_matches(|c| c == '(' || c == ')');
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() != 2 {
        return Err("Expected format: (x, y)".to_string());
    }
    let x: f32 = parts[0]
        .trim()
        .parse()
        .map_err(|_| "Invalid x coordinate".to_string())?;
    let y: f32 = parts[1]
        .trim()
        .parse()
        .map_err(|_| "Invalid y coordinate".to_string())?;
    Ok(Vec2::new(x, y))
}

/// Extract JSON from a response that may include markdown code fences.
fn extract_json(s: &str) -> Option<&str> {
    // Try ```json ... ```
    if let Some(start) = s.find("```json") {
        let content_start = start + 7;
        if let Some(end) = s[content_start..].find("```") {
            return Some(s[content_start..content_start + end].trim());
        }
    }
    // Try ``` ... ```
    if let Some(start) = s.find("```") {
        let content_start = start + 3;
        // Skip optional language tag on same line
        let line_end = s[content_start..].find('\n').unwrap_or(0);
        let actual_start = content_start + line_end;
        if let Some(end) = s[actual_start..].find("```") {
            return Some(s[actual_start..actual_start + end].trim());
        }
    }
    // Try to find JSON object directly
    if let Some(start) = s.find('{') {
        if let Some(end) = s.rfind('}') {
            if end > start {
                return Some(&s[start..=end]);
            }
        }
    }
    None
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_parser_basic() {
        let parser = JsonActionParser;
        let response = r#"{"actions": ["move up", "attack"], "reasoning": "Enemy spotted"}"#;
        let result = parser.parse(response).unwrap();
        assert_eq!(result.actions.len(), 2);
        assert!(matches!(result.actions[0], Action::Move { .. }));
        assert!(matches!(result.actions[1], Action::Attack));
        assert_eq!(result.reasoning, "Enemy spotted");
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn test_json_parser_with_code_fence() {
        let parser = JsonActionParser;
        let response = r#"Here's my decision:
```json
{"actions": ["idle"], "reasoning": "All clear"}
```"#;
        let result = parser.parse(response).unwrap();
        assert!(matches!(result.actions[0], Action::Idle));
    }

    #[test]
    fn test_json_parser_object_actions() {
        let parser = JsonActionParser;
        let response = r#"{"actions": [{"type": "move", "direction": "north"}, {"type": "attack"}], "reasoning": "test"}"#;
        let result = parser.parse(response).unwrap();
        assert_eq!(result.actions.len(), 2);
    }

    #[test]
    fn test_json_parser_empty() {
        let parser = JsonActionParser;
        assert!(parser.parse("").is_err());
    }

    #[test]
    fn test_kv_parser() {
        let parser = KeyValueParser;
        let response = "ACTION: move up\nACTION: attack\nREASON: Enemy is near";
        let result = parser.parse(response).unwrap();
        assert_eq!(result.actions.len(), 2);
        assert_eq!(result.reasoning, "Enemy is near");
    }

    #[test]
    fn test_fallback_parser() {
        let parser = FallbackParser::default_chain();
        // Valid JSON
        let result = parser
            .parse(r#"{"actions": ["attack"], "reasoning": "go"}"#)
            .unwrap();
        assert!(matches!(result.actions[0], Action::Attack));

        // Garbage input
        let result = parser.parse("asdfghjkl").unwrap();
        assert!(matches!(result.actions[0], Action::Idle));
    }

    #[test]
    fn test_parse_directions() {
        assert!(parse_direction("north").is_ok());
        assert!(parse_direction("up-right").is_ok());
        assert!(parse_direction("sw").is_ok());
        assert!(parse_direction("1.0,0.0").is_ok());
    }

    #[test]
    fn test_parse_action_strings() {
        assert!(matches!(parse_action_string("idle"), Ok(Action::Idle)));
        assert!(matches!(parse_action_string("attack"), Ok(Action::Attack)));
        assert!(matches!(parse_action_string("move north"), Ok(Action::Move { .. })));
        assert!(matches!(parse_action_string("speak hello"), Ok(Action::Speak { .. })));
        assert!(matches!(parse_action_string("rotate 45"), Ok(Action::Rotate { .. })));
        assert!(matches!(parse_action_string("drop"), Ok(Action::Drop { .. })));
        assert!(matches!(parse_action_string("ability 0"), Ok(Action::UseAbility { .. })));
    }

    #[test]
    fn test_extract_json() {
        assert_eq!(
            extract_json(r#"```json
{"a": 1}
```"#),
            Some(r#"{"a": 1}"#)
        );
        assert_eq!(extract_json(r#"blah {"a": 1} blah"#), Some(r#"{"a": 1}"#));
    }
}
