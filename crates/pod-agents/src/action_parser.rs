//! Structured output parsing: LLM response text → Vec<Action>.
//!
//! Supports multiple response formats from LLMs:
//! - JSON: `{"actions": [...], "reasoning": "..."}`
//! - TOON: `actions[2]: stop,attack`
//! - Key-value: `ACTION: move up\nREASON: ...`
//! - Natural language: regex extraction from free-form text
//!
//! All parsers are fallible — unknown or malformed responses degrade gracefully to Idle.

use glam::Vec2;
use log::{debug, warn};
use pod_core::action::{Action, CompanionCommand, SpeakVolume};
use pod_core::component::SkillKind;
use pod_core::id::EntityId;
use toon_format::decode_default as decode_toon;

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

impl ActionParser for JsonActionParser {
    fn parse(&self, response: &str) -> Result<ActionParseResult, ActionParseError> {
        let response = response.trim();
        if response.is_empty() {
            return Err(ActionParseError::EmptyResponse);
        }

        // Try to extract JSON from response (LLMs sometimes wrap in markdown)
        let json_str = extract_json(response).unwrap_or(response);

        let parsed: serde_json::Value = serde_json::from_str(json_str)
            .map_err(|e| ActionParseError::InvalidFormat(format!("JSON parse failed: {}", e)))?;
        parse_structured_action_payload(&parsed, response)
    }

    fn name(&self) -> &str {
        "json"
    }
}

// ============================================================
// TOON PARSER
// ============================================================

/// Parses official TOON outputs like `actions[2]: stop,attack`.
///
/// Example:
/// ```text
/// actions[2]: stop,attack
/// reasoning: Hold position until cleared.
/// ```
pub struct ToonActionParser;

impl ActionParser for ToonActionParser {
    fn parse(&self, response: &str) -> Result<ActionParseResult, ActionParseError> {
        let response = response.trim();
        if response.is_empty() {
            return Err(ActionParseError::EmptyResponse);
        }

        let parsed: serde_json::Value = decode_toon(response)
            .map_err(|e| ActionParseError::InvalidFormat(format!("TOON parse failed: {}", e)))?;
        parse_structured_action_payload(&parsed, response)
    }

    fn name(&self) -> &str {
        "toon"
    }
}

fn parse_structured_action_payload(
    value: &serde_json::Value,
    raw_response: &str,
) -> Result<ActionParseResult, ActionParseError> {
    let obj = value.as_object().ok_or_else(|| {
        ActionParseError::InvalidFormat("Structured response must decode to an object".to_string())
    })?;

    let mut actions = Vec::new();
    let mut warnings = Vec::new();

    if let Some(action_value) = obj.get("actions") {
        parse_action_value(action_value, &mut actions, &mut warnings);
    }

    if actions.is_empty() {
        actions.push(Action::Idle);
    }

    let reasoning = obj
        .get("reasoning")
        .or_else(|| obj.get("reason"))
        .map(stringify_reasoning)
        .unwrap_or_else(|| "No reasoning provided".to_string());

    let confidence = if warnings.is_empty() { 1.0 } else { 0.7 };

    Ok(ActionParseResult {
        actions,
        reasoning,
        confidence,
        raw_response: raw_response.to_string(),
        warnings,
    })
}

fn parse_action_value(
    value: &serde_json::Value,
    actions: &mut Vec<Action>,
    warnings: &mut Vec<String>,
) {
    match value {
        serde_json::Value::Array(items) => {
            for item in items {
                parse_action_value(item, actions, warnings);
            }
        }
        serde_json::Value::String(s) => match parse_action_string(s) {
            Ok(action) => actions.push(action),
            Err(e) => warnings.push(format!("Unknown action '{}': {}", s, e)),
        },
        serde_json::Value::Object(obj) => match parse_action_object(obj) {
            Ok(action) => actions.push(action),
            Err(e) => warnings.push(format!("Invalid action object: {}", e)),
        },
        other => warnings.push(format!("Unexpected action value: {}", other)),
    }
}

fn stringify_reasoning(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_else(|_| "No reasoning provided".into()),
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
                    Err(e) => {
                        warnings.push(format!("Unknown action '{}': {}", action_str.trim(), e))
                    }
                }
            } else if let Some(reason) = line
                .strip_prefix("REASON:")
                .or_else(|| line.strip_prefix("reason:"))
                .or_else(|| {
                    line.strip_prefix("Reason:")
                        .or_else(|| line.strip_prefix("REASONING:"))
                })
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

    /// Default chain: JSON → TOON → KeyValue → Idle
    pub fn default_chain() -> Self {
        Self::new(vec![
            Box::new(JsonActionParser),
            Box::new(ToonActionParser),
            Box::new(KeyValueParser),
        ])
    }
}

impl ActionParser for FallbackParser {
    fn parse(&self, response: &str) -> Result<ActionParseResult, ActionParseError> {
        for parser in &self.parsers {
            match parser.parse(response) {
                Ok(result) if result.confidence > 0.5 => {
                    debug!(
                        "Parser '{}' succeeded with confidence {:.2}",
                        parser.name(),
                        result.confidence
                    );
                    return Ok(result);
                }
                Ok(result) => {
                    debug!(
                        "Parser '{}' low confidence {:.2}, trying next",
                        parser.name(),
                        result.confidence
                    );
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

    // "capture <id>"
    if let Some(id_str) = s.strip_prefix("capture ") {
        let id: u64 = id_str
            .trim()
            .parse()
            .map_err(|_| format!("Invalid entity id: {}", id_str))?;
        return Ok(Action::CaptureCreature {
            target: EntityId(id),
            tool_slot: None,
        });
    }

    // "summon companion <slot>"
    if let Some(slot_str) = s
        .strip_prefix("summon companion ")
        .or_else(|| s.strip_prefix("summon "))
    {
        let slot: u8 = slot_str
            .trim()
            .parse()
            .map_err(|_| format!("Invalid companion slot: {}", slot_str))?;
        return Ok(Action::SummonCompanion { slot });
    }

    // "loot <id>"
    if let Some(id_str) = s.strip_prefix("loot ") {
        let id: u64 = id_str
            .trim()
            .parse()
            .map_err(|_| format!("Invalid entity id: {}", id_str))?;
        return Ok(Action::Loot {
            target: EntityId(id),
        });
    }

    // "gather <skill> <id>"
    if let Some(rest) = s.strip_prefix("gather ") {
        let mut parts = rest.split_whitespace();
        let skill = parts
            .next()
            .ok_or_else(|| "Gather action missing skill".to_string())
            .and_then(parse_skill_kind_str)?;
        let id_str = parts
            .next()
            .ok_or_else(|| "Gather action missing entity id".to_string())?;
        let id: u64 = id_str
            .parse()
            .map_err(|_| format!("Invalid entity id: {}", id_str))?;
        return Ok(Action::GatherResource {
            target: EntityId(id),
            skill,
        });
    }

    // "auto retaliate on/off"
    if let Some(rest) = s.strip_prefix("auto retaliate ") {
        let enabled = match rest.trim() {
            "on" | "true" | "enable" | "enabled" => true,
            "off" | "false" | "disable" | "disabled" => false,
            other => return Err(format!("Invalid auto retaliate state: {}", other)),
        };
        return Ok(Action::SetAutoRetaliate { enabled });
    }

    // "command companion <slot> <command> [target]"
    if let Some(rest) = s.strip_prefix("command companion ") {
        let mut parts = rest.split_whitespace();
        let slot_str = parts
            .next()
            .ok_or_else(|| "Command companion missing slot".to_string())?;
        let slot: u8 = slot_str
            .parse()
            .map_err(|_| format!("Invalid companion slot: {}", slot_str))?;
        let command_str = parts
            .next()
            .ok_or_else(|| "Command companion missing command".to_string())?;
        let command = parse_companion_command(command_str)?;
        let target = if let Some(id_str) = parts.next() {
            Some(EntityId(
                id_str
                    .parse()
                    .map_err(|_| format!("Invalid entity id: {}", id_str))?,
            ))
        } else {
            None
        };
        return Ok(Action::CommandCompanion {
            slot,
            command,
            target,
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
    if let Some(slot_str) = s
        .strip_prefix("ability ")
        .or_else(|| s.strip_prefix("use ability "))
    {
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
        "capture" => {
            let target = obj
                .get("target")
                .and_then(|v| v.as_u64())
                .ok_or("Capture missing 'target'")?;
            let tool_slot = obj
                .get("tool_slot")
                .and_then(|v| v.as_u64())
                .map(|v| v as u8);
            Ok(Action::CaptureCreature {
                target: EntityId(target),
                tool_slot,
            })
        }
        "summon_companion" => {
            let slot = obj
                .get("slot")
                .and_then(|v| v.as_u64())
                .ok_or("SummonCompanion missing 'slot'")?;
            Ok(Action::SummonCompanion { slot: slot as u8 })
        }
        "command_companion" => {
            let slot = obj
                .get("slot")
                .and_then(|v| v.as_u64())
                .ok_or("CommandCompanion missing 'slot'")?;
            let command = obj
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or("CommandCompanion missing 'command'")
                .map_err(str::to_string)
                .and_then(parse_companion_command)?;
            let target = obj.get("target").and_then(|v| v.as_u64()).map(EntityId);
            Ok(Action::CommandCompanion {
                slot: slot as u8,
                command,
                target,
            })
        }
        "gather" => {
            let target = obj
                .get("target")
                .and_then(|v| v.as_u64())
                .ok_or("Gather missing 'target'")?;
            let skill = obj
                .get("skill")
                .and_then(|v| v.as_str())
                .ok_or("Gather missing 'skill'")
                .map_err(str::to_string)
                .and_then(parse_skill_kind_str)?;
            Ok(Action::GatherResource {
                target: EntityId(target),
                skill,
            })
        }
        "loot" => {
            let target = obj
                .get("target")
                .and_then(|v| v.as_u64())
                .ok_or("Loot missing 'target'")?;
            Ok(Action::Loot {
                target: EntityId(target),
            })
        }
        "set_auto_retaliate" => {
            let enabled = obj
                .get("enabled")
                .and_then(|v| v.as_bool())
                .ok_or("SetAutoRetaliate missing 'enabled'")?;
            Ok(Action::SetAutoRetaliate { enabled })
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
                let x: f32 = parts[0]
                    .trim()
                    .parse()
                    .map_err(|_| format!("Invalid x: {}", parts[0]))?;
                let y: f32 = parts[1]
                    .trim()
                    .parse()
                    .map_err(|_| format!("Invalid y: {}", parts[1]))?;
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

fn parse_skill_kind_str(s: &str) -> Result<SkillKind, String> {
    match s.trim().to_lowercase().as_str() {
        "attack" => Ok(SkillKind::Attack),
        "strength" => Ok(SkillKind::Strength),
        "defence" | "defense" => Ok(SkillKind::Defence),
        "ranged" | "range" => Ok(SkillKind::Ranged),
        "magic" | "mage" => Ok(SkillKind::Magic),
        "constitution" | "health" => Ok(SkillKind::Constitution),
        "mining" => Ok(SkillKind::Mining),
        "woodcutting" | "woodcut" => Ok(SkillKind::Woodcutting),
        "fishing" => Ok(SkillKind::Fishing),
        "cooking" => Ok(SkillKind::Cooking),
        "smithing" => Ok(SkillKind::Smithing),
        "crafting" => Ok(SkillKind::Crafting),
        "slayer" => Ok(SkillKind::Slayer),
        "taming" => Ok(SkillKind::Taming),
        "bonding" => Ok(SkillKind::Bonding),
        other => Err(format!("Unknown skill kind: {}", other)),
    }
}

fn parse_companion_command(s: &str) -> Result<CompanionCommand, String> {
    match s.trim().to_lowercase().as_str() {
        "attack" => Ok(CompanionCommand::Attack),
        "follow" => Ok(CompanionCommand::Follow),
        "guard" => Ok(CompanionCommand::Guard),
        "recall" => Ok(CompanionCommand::Recall),
        other => Err(format!("Unknown companion command: {}", other)),
    }
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
        assert!(matches!(
            parse_action_string("move north"),
            Ok(Action::Move { .. })
        ));
        assert!(matches!(
            parse_action_string("speak hello"),
            Ok(Action::Speak { .. })
        ));
        assert!(matches!(
            parse_action_string("rotate 45"),
            Ok(Action::Rotate { .. })
        ));
        assert!(matches!(
            parse_action_string("drop"),
            Ok(Action::Drop { .. })
        ));
        assert!(matches!(
            parse_action_string("ability 0"),
            Ok(Action::UseAbility { .. })
        ));
        assert!(matches!(
            parse_action_string("capture 42"),
            Ok(Action::CaptureCreature { .. })
        ));
        assert!(matches!(
            parse_action_string("summon companion 2"),
            Ok(Action::SummonCompanion { slot: 2 })
        ));
        assert!(matches!(
            parse_action_string("gather mining 19"),
            Ok(Action::GatherResource {
                skill: SkillKind::Mining,
                ..
            })
        ));
        assert!(matches!(
            parse_action_string("loot 8"),
            Ok(Action::Loot { .. })
        ));
        assert!(matches!(
            parse_action_string("auto retaliate on"),
            Ok(Action::SetAutoRetaliate { enabled: true })
        ));
    }

    #[test]
    fn test_parse_action_object_supports_mmo_verbs() {
        let action = parse_action_object(
            &serde_json::json!({
                "type": "command_companion",
                "slot": 1,
                "command": "attack",
                "target": 77
            })
            .as_object()
            .unwrap()
            .clone(),
        )
        .unwrap();
        assert!(matches!(
            action,
            Action::CommandCompanion {
                slot: 1,
                command: CompanionCommand::Attack,
                target: Some(EntityId(77))
            }
        ));
    }

    #[test]
    fn test_extract_json() {
        assert_eq!(
            extract_json(
                r#"```json
{"a": 1}
```"#
            ),
            Some(r#"{"a": 1}"#)
        );
        assert_eq!(extract_json(r#"blah {"a": 1} blah"#), Some(r#"{"a": 1}"#));
    }

    #[test]
    fn test_toon_parser_basic() {
        let parser = ToonActionParser;
        let response = r#"actions[2]: stop,attack
reasoning: Keep distance while advancing."#;
        let result = parser.parse(response).unwrap();
        assert_eq!(result.actions.len(), 2);
        assert_eq!(result.reasoning, "Keep distance while advancing.");
        assert!(matches!(result.actions[0], Action::Stop));
        assert!(matches!(result.actions[1], Action::Attack));
    }

    #[test]
    fn test_toon_parser_mismatched_count() {
        let parser = ToonActionParser;
        let response = r#"actions[2]: stop
reasoning: Bad count declaration."#;
        assert!(parser.parse(response).is_err());
    }

    #[test]
    fn test_toon_parser_unknown_rows() {
        let parser = ToonActionParser;
        let response = r#"actions[2]: move up,nonsense
reasoning: fallback behavior"#;
        let result = parser.parse(response).unwrap();
        assert_eq!(result.actions.len(), 2);
        assert!(matches!(result.actions[0], Action::Move { .. }));
        assert!(matches!(result.actions[1], Action::Idle));
        assert_eq!(result.confidence, 1.0);
    }

    #[test]
    fn test_fallback_parser_includes_toon() {
        let parser = FallbackParser::default_chain();
        let result = parser.parse(r#"actions[1]: stop"#).unwrap();
        assert!(matches!(result.actions[0], Action::Stop));
    }
}
