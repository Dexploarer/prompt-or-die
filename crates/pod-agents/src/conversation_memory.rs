//! Conversation memory with sliding window and importance scoring.
//!
//! Maintains a rolling context of past observations, decisions, and outcomes
//! that can be included in LLM prompts for continuity. Token-aware truncation
//! ensures the memory fits within LLM context limits.
//!
//! Key features:
//! - Sliding window with configurable max entries
//! - Token budget enforcement for memory section of prompts
//! - Importance-based retention (combat events weighted higher)
//! - Compact serialization for LLM consumption

use pod_core::action::Action;
use pod_core::observation::{Observation, Relationship};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

// ============================================================
// MEMORY ENTRY
// ============================================================

/// A single memory entry recording what happened at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Game tick when this happened
    pub tick: u64,
    /// Compact summary of the observation
    pub observation_summary: String,
    /// Actions the agent took
    pub actions_taken: Vec<Action>,
    /// Agent's reasoning for those actions
    pub reasoning: String,
    /// Importance score (0.0 = routine, 1.0 = critical)
    pub importance: f32,
    /// Estimated tokens this entry consumes when serialized
    pub estimated_tokens: u32,
}

impl MemoryEntry {
    /// Create from observation and decision data
    pub fn from_decision(obs: &Observation, actions: &[Action], reasoning: &str) -> Self {
        let summary = Self::summarize_observation(obs);
        let importance = Self::compute_importance(obs, actions);
        let estimated_tokens = (summary.len() + reasoning.len() + 20) as u32 / 4;

        Self {
            tick: obs.tick,
            observation_summary: summary,
            actions_taken: actions.to_vec(),
            reasoning: reasoning.to_string(),
            importance,
            estimated_tokens,
        }
    }

    /// Create a compact observation summary
    fn summarize_observation(obs: &Observation) -> String {
        let mut parts = Vec::new();

        // Position and health
        parts.push(format!(
            "pos:({:.0},{:.0}) hp:{:.0}%",
            obs.self_state.position.x,
            obs.self_state.position.y,
            obs.self_state
                .health
                .zip(obs.self_state.max_health)
                .map(|(h, m)| if m > 0.0 { h / m * 100.0 } else { 0.0 })
                .unwrap_or(0.0)
        ));

        // Threat count
        let hostile_count = obs
            .visible_entities
            .iter()
            .filter(|e| matches!(e.relationship, Relationship::Hostile))
            .count();
        if hostile_count > 0 {
            let closest = obs
                .visible_entities
                .iter()
                .filter(|e| matches!(e.relationship, Relationship::Hostile))
                .map(|e| e.distance)
                .fold(f32::MAX, f32::min);
            parts.push(format!("threats:{} closest:{:.0}", hostile_count, closest));
        }

        // Notable events
        if !obs.audible_events.is_empty() {
            parts.push(format!("sounds:{}", obs.audible_events.len()));
        }
        if !obs.messages.is_empty() {
            parts.push(format!("msgs:{}", obs.messages.len()));
        }

        parts.join(" | ")
    }

    /// Compute importance score based on observation content and actions taken
    fn compute_importance(obs: &Observation, actions: &[Action]) -> f32 {
        let mut score: f32 = 0.0;

        // Combat events are important
        let has_hostiles = obs
            .visible_entities
            .iter()
            .any(|e| matches!(e.relationship, Relationship::Hostile));
        if has_hostiles {
            score += 0.4;
        }

        // Taking combat actions is important
        let has_combat = actions.iter().any(|a| {
            matches!(
                a,
                Action::Attack | Action::AttackTarget { .. } | Action::UseAbility { .. }
            )
        });
        if has_combat {
            score += 0.3;
        }

        // Low health is important
        if let (Some(hp), Some(max_hp)) = (obs.self_state.health, obs.self_state.max_health) {
            if max_hp > 0.0 && hp / max_hp < 0.3 {
                score += 0.3;
            }
        }

        // Communication is somewhat important
        let has_speech = actions.iter().any(|a| matches!(a, Action::Speak { .. }));
        if has_speech || !obs.messages.is_empty() {
            score += 0.1;
        }

        // Audio events add some importance
        if !obs.audible_events.is_empty() {
            score += 0.1;
        }

        score.clamp(0.0, 1.0)
    }

    /// Format this entry for inclusion in an LLM prompt
    pub fn to_prompt_line(&self) -> String {
        let action_strs: Vec<String> = self
            .actions_taken
            .iter()
            .map(|a| format!("{:?}", a))
            .collect();

        format!(
            "[T{}] {} → {} ({})",
            self.tick,
            self.observation_summary,
            action_strs.join(", "),
            self.reasoning
        )
    }
}

// ============================================================
// MEMORY CONFIG
// ============================================================

/// Configuration for conversation memory behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Maximum number of entries to keep
    pub max_entries: usize,
    /// Maximum tokens for memory section in prompts
    pub max_tokens: u32,
    /// Importance threshold for keeping old entries (0.0 = keep all, 1.0 = keep only critical)
    pub importance_threshold: f32,
    /// How often to record (every N ticks). Set to 1 for every tick.
    pub record_interval: u64,
    /// Include memory in LLM prompts
    pub include_in_prompts: bool,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            max_entries: 20,
            max_tokens: 500,
            importance_threshold: 0.0,
            record_interval: 60, // once per second at 60 TPS
            include_in_prompts: true,
        }
    }
}

// ============================================================
// CONVERSATION MEMORY
// ============================================================

/// Sliding window memory of past observations and decisions.
/// Token-aware — automatically truncates to fit within budget.
pub struct ConversationMemory {
    entries: VecDeque<MemoryEntry>,
    config: MemoryConfig,
    last_record_tick: u64,
}

impl ConversationMemory {
    pub fn new(config: MemoryConfig) -> Self {
        Self {
            entries: VecDeque::new(),
            config,
            last_record_tick: 0,
        }
    }

    /// Record a new memory entry from an observation and decision.
    /// Returns true if recorded, false if skipped (too soon or below threshold).
    pub fn record(&mut self, obs: &Observation, actions: &[Action], reasoning: &str) -> bool {
        // Check record interval (skip for first entry — allow initial recording)
        if !self.entries.is_empty()
            && obs.tick < self.last_record_tick + self.config.record_interval
        {
            // Still record high-importance events regardless of interval
            let entry = MemoryEntry::from_decision(obs, actions, reasoning);
            if entry.importance < 0.5 {
                return false;
            }
            self.push_entry(entry);
            return true;
        }

        let entry = MemoryEntry::from_decision(obs, actions, reasoning);

        // Check importance threshold
        if entry.importance < self.config.importance_threshold {
            return false;
        }

        self.last_record_tick = obs.tick;
        self.push_entry(entry);
        true
    }

    /// Push an entry, evicting old ones to stay within limits.
    fn push_entry(&mut self, entry: MemoryEntry) {
        self.entries.push_back(entry);

        // Enforce max entries
        while self.entries.len() > self.config.max_entries {
            // Remove least important entry (not the most recent)
            if self.entries.len() > 1 {
                let min_idx = self
                    .entries
                    .iter()
                    .enumerate()
                    .skip(1) // keep at least the first
                    .take(self.entries.len() - 2) // and the last
                    .min_by(|(_, a), (_, b)| {
                        a.importance
                            .partial_cmp(&b.importance)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                self.entries.remove(min_idx);
            } else {
                self.entries.pop_front();
            }
        }

        // Enforce token budget
        self.trim_to_token_budget();
    }

    /// Remove oldest low-importance entries until within token budget.
    fn trim_to_token_budget(&mut self) {
        while self.total_tokens() > self.config.max_tokens && self.entries.len() > 1 {
            // Find and remove lowest importance entry
            let min_idx = self
                .entries
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| {
                    a.importance
                        .partial_cmp(&b.importance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.entries.remove(min_idx);
        }
    }

    /// Total estimated tokens across all entries.
    pub fn total_tokens(&self) -> u32 {
        self.entries.iter().map(|e| e.estimated_tokens).sum()
    }

    /// Format all memory entries as a prompt section.
    pub fn to_prompt_section(&self) -> String {
        if self.entries.is_empty() || !self.config.include_in_prompts {
            return String::new();
        }

        let mut section = String::from("MEMORY (recent events):\n");
        for entry in &self.entries {
            section.push_str(&entry.to_prompt_line());
            section.push('\n');
        }
        section
    }

    /// Number of entries currently stored.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether memory is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear all memory entries.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.last_record_tick = 0;
    }

    /// Get all entries (for serialization/debugging).
    pub fn entries(&self) -> &VecDeque<MemoryEntry> {
        &self.entries
    }

    /// Get the most recent entry.
    pub fn last_entry(&self) -> Option<&MemoryEntry> {
        self.entries.back()
    }
}

// ============================================================
// TESTS
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec2;
    use pod_core::observation::*;

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
                ..Default::default()
            });
        }
        obs
    }

    #[test]
    fn test_record_basic() {
        let mut mem = ConversationMemory::new(MemoryConfig {
            record_interval: 1,
            ..Default::default()
        });

        let obs = make_obs(1, false);
        assert!(mem.record(&obs, &[Action::Idle], "waiting"));
        assert_eq!(mem.len(), 1);
    }

    #[test]
    fn test_record_interval() {
        let mut mem = ConversationMemory::new(MemoryConfig {
            record_interval: 60,
            ..Default::default()
        });

        let obs1 = make_obs(1, false);
        assert!(mem.record(&obs1, &[Action::Idle], "first"));

        // Too soon — should be skipped (low importance)
        let obs2 = make_obs(30, false);
        assert!(!mem.record(&obs2, &[Action::Idle], "skipped"));

        // After interval — should record
        let obs3 = make_obs(61, false);
        assert!(mem.record(&obs3, &[Action::Idle], "recorded"));
        assert_eq!(mem.len(), 2);
    }

    #[test]
    fn test_high_importance_bypasses_interval() {
        let mut mem = ConversationMemory::new(MemoryConfig {
            record_interval: 60,
            ..Default::default()
        });

        let obs1 = make_obs(1, false);
        mem.record(&obs1, &[Action::Idle], "first");

        // High importance (combat) should bypass interval
        let obs2 = make_obs(5, true);
        assert!(mem.record(&obs2, &[Action::Attack], "combat!"));
        assert_eq!(mem.len(), 2);
    }

    #[test]
    fn test_max_entries_eviction() {
        let mut mem = ConversationMemory::new(MemoryConfig {
            max_entries: 3,
            record_interval: 1,
            max_tokens: u32::MAX,
            ..Default::default()
        });

        for i in 0..5 {
            let obs = make_obs(i, false);
            mem.record(&obs, &[Action::Idle], "routine");
        }

        assert!(mem.len() <= 3);
    }

    #[test]
    fn test_token_budget() {
        let mut mem = ConversationMemory::new(MemoryConfig {
            max_tokens: 50, // very tight
            record_interval: 1,
            max_entries: 100,
            ..Default::default()
        });

        for i in 0..10 {
            let obs = make_obs(i, false);
            mem.record(&obs, &[Action::Idle], "routine");
        }

        assert!(mem.total_tokens() <= 50);
    }

    #[test]
    fn test_prompt_section() {
        let mut mem = ConversationMemory::new(MemoryConfig {
            record_interval: 1,
            ..Default::default()
        });

        let obs = make_obs(100, true);
        mem.record(&obs, &[Action::Attack], "attacking enemy");

        let section = mem.to_prompt_section();
        assert!(section.contains("MEMORY"));
        assert!(section.contains("[T100]"));
    }

    #[test]
    fn test_importance_scoring() {
        let peaceful_obs = make_obs(1, false);
        let combat_obs = make_obs(2, true);

        let peaceful_entry = MemoryEntry::from_decision(&peaceful_obs, &[Action::Idle], "peace");
        let combat_entry = MemoryEntry::from_decision(&combat_obs, &[Action::Attack], "fight");

        assert!(combat_entry.importance > peaceful_entry.importance);
    }
}
