//! # Decision Logger
//!
//! Central logging and replay system for all agent types.
//!
//! Collects per-tick decision data from `LlmAgent`, `ScriptedAgent`, `NeuralAgent`,
//! and `HumanAgent` into a unified `DecisionEntry` ring buffer. Supports flexible
//! querying, aggregate statistics, and conversion to the pod-core `ReplayFile` format
//! for deterministic replay.
//!
//! ## Usage
//!
//! ```rust
//! use pod_agents::decision_log::{DecisionLogger, DecisionEntry, DecisionMethod,
//!     DecisionTiming, ObservationSummary, LogFilter};
//! use pod_core::{AgentId, AgentType};
//! use glam::Vec2;
//! use std::collections::HashMap;
//!
//! let mut logger = DecisionLogger::with_capacity(500);
//!
//! let entry = DecisionEntry {
//!     tick: 1,
//!     agent_id: AgentId::new(),
//!     agent_type: AgentType::ScriptedNpc,
//!     observation_summary: ObservationSummary {
//!         visible_entity_count: 3,
//!         hostile_count: 1,
//!         friendly_count: 2,
//!         nearest_hostile_distance: Some(42.0),
//!         health_fraction: Some(0.8),
//!         position: Vec2::ZERO,
//!         active_objectives: 1,
//!     },
//!     decision_method: DecisionMethod::Scripted { rule_name: "patrol".into() },
//!     actions_produced: vec![],
//!     timing: DecisionTiming { observe_us: 10, decide_us: 20, total_us: 30 },
//!     tool_calls: vec![],
//!     metadata: HashMap::new(),
//! };
//!
//! logger.log(entry);
//! assert_eq!(logger.len(), 1);
//! ```

use std::collections::{HashMap, VecDeque};

use glam::Vec2;
use serde::{Deserialize, Serialize};

use pod_core::observation::Relationship;
use pod_core::replay::{DecisionTrace, ReplayFile, ReplayHeader};
use pod_core::{Action, AgentId, AgentToolCallTrace, AgentType, Observation};

// ─────────────────────────────────────────────────────────────────────────────
// ObservationSummary
// ─────────────────────────────────────────────────────────────────────────────

/// Compact summary of what an agent perceived in a single tick.
///
/// Derived from a full `Observation` but stripped down to the metrics that
/// matter for post-hoc analysis and replay. Keeps the `DecisionEntry` small
/// enough to live in the ring buffer without copying entire observation trees.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObservationSummary {
    /// Total number of entities in the agent's visible range.
    pub visible_entity_count: usize,
    /// Subset of visible entities with a `Hostile` relationship.
    pub hostile_count: usize,
    /// Subset of visible entities with a `Friendly` relationship.
    pub friendly_count: usize,
    /// Distance to the nearest hostile, if any hostiles are visible.
    pub nearest_hostile_distance: Option<f32>,
    /// Agent's own health as a fraction 0.0–1.0, if health is tracked.
    pub health_fraction: Option<f32>,
    /// Agent's world-space position this tick.
    pub position: Vec2,
    /// Number of incomplete objectives from the observation.
    pub active_objectives: usize,
}

impl ObservationSummary {
    /// Summarise a full `Observation` into the compact form used by the logger.
    pub fn from_observation(obs: &Observation) -> Self {
        let hostile_count = obs
            .visible_entities
            .iter()
            .filter(|e| matches!(e.relationship, Relationship::Hostile))
            .count();

        let friendly_count = obs
            .visible_entities
            .iter()
            .filter(|e| matches!(e.relationship, Relationship::Friendly))
            .count();

        let nearest_hostile_distance = obs
            .visible_entities
            .iter()
            .filter(|e| matches!(e.relationship, Relationship::Hostile))
            .map(|e| e.distance)
            .fold(None::<f32>, |acc, d| Some(acc.map_or(d, |a: f32| a.min(d))));

        let health_fraction = match (obs.self_state.health, obs.self_state.max_health) {
            (Some(hp), Some(max)) if max > 0.0 => Some(hp / max),
            _ => None,
        };

        let active_objectives = obs.objectives.iter().filter(|o| !o.completed).count();

        Self {
            visible_entity_count: obs.visible_entities.len(),
            hostile_count,
            friendly_count,
            nearest_hostile_distance,
            health_fraction,
            position: obs.self_state.position,
            active_objectives,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DecisionMethod
// ─────────────────────────────────────────────────────────────────────────────

/// Describes the mechanism that produced a tick's actions.
///
/// Each variant carries the fields that are meaningful for its decision
/// strategy, enabling fine-grained analytics per agent architecture.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionMethod {
    /// Decision produced by an LLM (Claude, GPT, etc.).
    LlmResponse {
        /// Model identifier, e.g. `"claude-sonnet-4-6"`.
        model: String,
        /// Tokens consumed by the prompt.
        prompt_tokens: u32,
        /// Tokens in the model's completion.
        completion_tokens: u32,
    },
    /// Decision produced by a behaviour tree traversal.
    BehaviorTree {
        /// Name of the leaf node that returned `Success` or `Running`.
        active_node: String,
        /// Depth in the tree at which the active node sits.
        tree_depth: u32,
    },
    /// Decision produced by a finite state machine.
    FiniteStateMachine {
        /// The state the FSM was in when it produced actions.
        current_state: String,
        /// The transition that fired this tick, if any.
        transition: Option<String>,
    },
    /// Decision produced by a utility-AI scoring pass.
    UtilityAi {
        /// All (action-name, utility-score) pairs evaluated this tick.
        scores: Vec<(String, f32)>,
        /// The action that won the scoring comparison.
        winner: String,
    },
    /// Decision produced by a neural-network policy forward pass.
    NeuralNetwork {
        /// Raw argmax index into the action vocabulary.
        action_index: usize,
        /// Softmax probability of the chosen action.
        confidence: f32,
    },
    /// Decision split across two systems (e.g. strategy from LLM, execution from BT).
    Hybrid {
        /// Which system chose *what* to do.
        strategy_source: String,
        /// Which system chose *how* to do it.
        execution_source: String,
    },
    /// Decision produced by a simple hand-authored rule.
    Scripted {
        /// Name of the rule or condition that fired.
        rule_name: String,
    },
}

// ─────────────────────────────────────────────────────────────────────────────
// DecisionTiming
// ─────────────────────────────────────────────────────────────────────────────

/// Microsecond-resolution timing for a single observe → decide cycle.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct DecisionTiming {
    /// Time spent building/compressing the `Observation` into agent input (µs).
    pub observe_us: u64,
    /// Time spent inside the agent's `decide()` call (µs).
    pub decide_us: u64,
    /// Wall-clock total for the full cycle (µs). May exceed `observe_us + decide_us`
    /// when overhead (serialisation, network, etc.) is included.
    pub total_us: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// DecisionEntry
// ─────────────────────────────────────────────────────────────────────────────

/// One agent's full decision record for a single tick.
///
/// Richer than `pod_core::replay::DecisionTrace`: it captures the observation
/// summary, agent type, decision method, and timing in addition to the raw
/// actions. The `metadata` map holds arbitrary key/value pairs for
/// agent-specific annotations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEntry {
    /// Game tick this decision belongs to.
    pub tick: u64,
    /// The agent that made the decision.
    pub agent_id: AgentId,
    /// Category of the agent (for segmented analytics).
    pub agent_type: AgentType,
    /// Compact summary of the observation the agent received.
    pub observation_summary: ObservationSummary,
    /// How the decision was produced.
    pub decision_method: DecisionMethod,
    /// Actions returned by `Agent::decide()` this tick.
    pub actions_produced: Vec<Action>,
    /// Timing breakdown for this cycle.
    pub timing: DecisionTiming,
    /// Tool/LLM side effects emitted while producing the decision.
    pub tool_calls: Vec<AgentToolCallTrace>,
    /// Arbitrary key/value metadata (agent-specific diagnostics, test tags, …).
    pub metadata: HashMap<String, String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// LogFilter
// ─────────────────────────────────────────────────────────────────────────────

/// Predicate used to select a subset of `DecisionEntry` records.
///
/// Multiple filters are combined with AND semantics: an entry must satisfy
/// **all** filters in the slice to be returned.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFilter {
    /// Only entries from this specific agent.
    AgentId(AgentId),
    /// Only entries from agents of this type.
    AgentType(AgentType),
    /// Only entries whose tick falls within `[start, end]` (inclusive).
    TickRange { start: u64, end: u64 },
    /// Only entries that contain at least one action whose debug string
    /// contains `action_type` as a substring (case-insensitive).
    ActionType(String),
    /// Only entries whose `timing.total_us` is at least this value.
    MinDecisionTime(u64),
}

impl LogFilter {
    fn matches(&self, entry: &DecisionEntry) -> bool {
        match self {
            LogFilter::AgentId(id) => entry.agent_id == *id,
            LogFilter::AgentType(t) => entry.agent_type == *t,
            LogFilter::TickRange { start, end } => entry.tick >= *start && entry.tick <= *end,
            LogFilter::ActionType(needle) => {
                let needle_lower = needle.to_lowercase();
                entry
                    .actions_produced
                    .iter()
                    .any(|a| format!("{:?}", a).to_lowercase().contains(&needle_lower))
            }
            LogFilter::MinDecisionTime(min_us) => entry.timing.total_us >= *min_us,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// LogStats
// ─────────────────────────────────────────────────────────────────────────────

/// Aggregate statistics computed over all entries in a `DecisionLogger`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogStats {
    /// Total number of entries currently held.
    pub total_entries: usize,
    /// Entry count broken down by `AgentType`.
    pub entries_by_agent_type: HashMap<AgentType, usize>,
    /// Mean `timing.total_us` across all entries (0 when empty).
    pub avg_decision_time_us: u64,
    /// Highest `timing.total_us` seen across all entries.
    pub max_decision_time_us: u64,
    /// `(min_tick, max_tick)` range covered by the buffer (both 0 when empty).
    pub tick_range: (u64, u64),
    /// Number of distinct `AgentId` values in the buffer.
    pub unique_agents: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// DecisionLogger
// ─────────────────────────────────────────────────────────────────────────────

/// Central logger that collects decision data from any agent type.
///
/// Internally backed by a `VecDeque` acting as a ring buffer. When `len()`
/// reaches `max_entries`, the oldest entry is evicted before each new one is
/// pushed. Logging is a no-op while `enabled == false`.
///
/// The logger is intentionally not `Send`/`Sync` — callers that need
/// cross-thread access should wrap it in a `Mutex`.
#[derive(Debug, Clone)]
pub struct DecisionLogger {
    entries: VecDeque<DecisionEntry>,
    max_entries: usize,
    filters: Vec<LogFilter>,
    enabled: bool,
}

impl Default for DecisionLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl DecisionLogger {
    /// Default capacity for the ring buffer (10 000 entries ≈ ~2–3 MB).
    const DEFAULT_CAPACITY: usize = 10_000;

    /// Create a logger with the default ring-buffer capacity.
    pub fn new() -> Self {
        Self {
            entries: VecDeque::with_capacity(Self::DEFAULT_CAPACITY),
            max_entries: Self::DEFAULT_CAPACITY,
            filters: Vec::new(),
            enabled: true,
        }
    }

    /// Create a logger with a custom ring-buffer capacity.
    ///
    /// Setting `max_entries` to `0` is valid but means the buffer never grows.
    pub fn with_capacity(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries.min(Self::DEFAULT_CAPACITY)),
            max_entries,
            filters: Vec::new(),
            enabled: true,
        }
    }

    // ── Toggle ────────────────────────────────────────────────────────────────

    /// Enable logging (default state).
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disable logging. All calls to `log()` become no-ops while disabled.
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Returns `true` if the logger is currently accepting entries.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    // ── Ingestion ─────────────────────────────────────────────────────────────

    /// Record a single decision entry.
    ///
    /// If the logger is disabled or if any active filter rejects the entry,
    /// the entry is silently discarded. When the buffer is at capacity the
    /// oldest entry is evicted (ring-buffer semantics).
    pub fn log(&mut self, entry: DecisionEntry) {
        if !self.enabled {
            return;
        }

        // Apply instance-level filters — entry must pass all of them.
        if !self.filters.iter().all(|f| f.matches(&entry)) {
            return;
        }

        if self.max_entries == 0 {
            return;
        }

        if self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }

        self.entries.push_back(entry);
    }

    // ── Filters ───────────────────────────────────────────────────────────────

    /// Append a filter that will be applied to every subsequent `log()` call.
    ///
    /// Filters are evaluated with AND semantics — all must pass for an entry
    /// to be stored.
    pub fn add_filter(&mut self, filter: LogFilter) {
        self.filters.push(filter);
    }

    /// Remove all active ingestion filters.
    pub fn clear_filters(&mut self) {
        self.filters.clear();
    }

    // ── Querying ──────────────────────────────────────────────────────────────

    /// Return all entries that satisfy **all** of the provided `filters`.
    ///
    /// The returned slice is ordered oldest-first (insertion order).
    pub fn query(&self, filters: &[LogFilter]) -> Vec<&DecisionEntry> {
        self.entries
            .iter()
            .filter(|e| filters.iter().all(|f| f.matches(e)))
            .collect()
    }

    /// Return all entries belonging to the given agent (oldest first).
    pub fn entries_for_agent(&self, agent_id: AgentId) -> Vec<&DecisionEntry> {
        self.query(&[LogFilter::AgentId(agent_id)])
    }

    /// Return all entries recorded on the given tick.
    pub fn entries_for_tick(&self, tick: u64) -> Vec<&DecisionEntry> {
        self.query(&[LogFilter::TickRange {
            start: tick,
            end: tick,
        }])
    }

    // ── Aggregation ───────────────────────────────────────────────────────────

    /// Compute aggregate statistics over the current buffer contents.
    pub fn summary_stats(&self) -> LogStats {
        if self.entries.is_empty() {
            return LogStats {
                total_entries: 0,
                entries_by_agent_type: HashMap::new(),
                avg_decision_time_us: 0,
                max_decision_time_us: 0,
                tick_range: (0, 0),
                unique_agents: 0,
            };
        }

        let mut by_type: HashMap<AgentType, usize> = HashMap::new();
        let mut unique_agents: std::collections::HashSet<AgentId> =
            std::collections::HashSet::new();
        let mut sum_us: u64 = 0;
        let mut max_us: u64 = 0;
        let mut min_tick = u64::MAX;
        let mut max_tick = u64::MIN;

        for entry in &self.entries {
            *by_type.entry(entry.agent_type).or_insert(0) += 1;
            unique_agents.insert(entry.agent_id);

            sum_us = sum_us.saturating_add(entry.timing.total_us);
            if entry.timing.total_us > max_us {
                max_us = entry.timing.total_us;
            }
            if entry.tick < min_tick {
                min_tick = entry.tick;
            }
            if entry.tick > max_tick {
                max_tick = entry.tick;
            }
        }

        let total = self.entries.len();
        let avg_us = if total > 0 { sum_us / total as u64 } else { 0 };

        LogStats {
            total_entries: total,
            entries_by_agent_type: by_type,
            avg_decision_time_us: avg_us,
            max_decision_time_us: max_us,
            tick_range: (min_tick, max_tick),
            unique_agents: unique_agents.len(),
        }
    }

    // ── Conversion ────────────────────────────────────────────────────────────

    /// Convert the current buffer into a `pod_core::ReplayFile`.
    ///
    /// Only entries produced by `LlmAgent` (i.e. `DecisionMethod::LlmResponse`)
    /// are mapped to `DecisionTrace` records, because `DecisionTrace` has fields
    /// (`prompt_sent`, `raw_response`, `latency_ms`) that are only meaningful for
    /// LLM decisions. All other agent types produce a minimal trace so that the
    /// replay file is well-formed and all ticks are represented.
    pub fn to_replay_file(&self, name: &str, world_seed: u64) -> ReplayFile {
        let tick_count = self
            .entries
            .iter()
            .map(|e| e.tick)
            .max()
            .map(|t| t + 1)
            .unwrap_or(0);

        let unique_agents: std::collections::HashSet<AgentId> =
            self.entries.iter().map(|e| e.agent_id).collect();

        let header = ReplayHeader {
            name: name.to_string(),
            timestamp: 0, // callers that need wall time can set this themselves
            world_seed,
            tick_count,
            agent_count: unique_agents.len(),
            notes: format!(
                "Generated by DecisionLogger from {} entries",
                self.entries.len()
            ),
        };

        // Build tick-indexed trace table.
        let mut traces: Vec<Vec<DecisionTrace>> = vec![Vec::new(); tick_count as usize];

        for entry in &self.entries {
            let tick_idx = entry.tick as usize;
            if tick_idx >= traces.len() {
                continue;
            }

            let (prompt_sent, raw_response, latency_ms) = match &entry.decision_method {
                DecisionMethod::LlmResponse {
                    model,
                    prompt_tokens,
                    completion_tokens,
                } => (
                    format!("model={} prompt_tokens={}", model, prompt_tokens),
                    format!("completion_tokens={}", completion_tokens),
                    // total_us / 1000 gives ms; clamp to u32::MAX
                    (entry.timing.total_us / 1_000).min(u32::MAX as u64) as u32,
                ),
                DecisionMethod::BehaviorTree {
                    active_node,
                    tree_depth,
                } => (
                    format!("bt_node={} depth={}", active_node, tree_depth),
                    String::new(),
                    (entry.timing.total_us / 1_000).min(u32::MAX as u64) as u32,
                ),
                DecisionMethod::FiniteStateMachine {
                    current_state,
                    transition,
                } => (
                    format!("fsm_state={}", current_state),
                    transition.clone().unwrap_or_default(),
                    (entry.timing.total_us / 1_000).min(u32::MAX as u64) as u32,
                ),
                DecisionMethod::UtilityAi { winner, .. } => (
                    format!("utility_winner={}", winner),
                    String::new(),
                    (entry.timing.total_us / 1_000).min(u32::MAX as u64) as u32,
                ),
                DecisionMethod::NeuralNetwork {
                    action_index,
                    confidence,
                } => (
                    format!("nn_action={} confidence={:.4}", action_index, confidence),
                    String::new(),
                    (entry.timing.total_us / 1_000).min(u32::MAX as u64) as u32,
                ),
                DecisionMethod::Hybrid {
                    strategy_source,
                    execution_source,
                } => (
                    format!(
                        "strategy={} execution={}",
                        strategy_source, execution_source
                    ),
                    String::new(),
                    (entry.timing.total_us / 1_000).min(u32::MAX as u64) as u32,
                ),
                DecisionMethod::Scripted { rule_name } => (
                    format!("rule={}", rule_name),
                    String::new(),
                    (entry.timing.total_us / 1_000).min(u32::MAX as u64) as u32,
                ),
            };

            // Derive a stable observation hash from position and entity count.
            let obs_hash = {
                use std::collections::hash_map::DefaultHasher;
                use std::hash::{Hash, Hasher};
                let mut h = DefaultHasher::new();
                entry.tick.hash(&mut h);
                entry.observation_summary.position.x.to_bits().hash(&mut h);
                entry.observation_summary.position.y.to_bits().hash(&mut h);
                entry.observation_summary.visible_entity_count.hash(&mut h);
                h.finish()
            };

            traces[tick_idx].push(DecisionTrace {
                tick: entry.tick,
                agent_id: entry.agent_id,
                observation_hash: obs_hash,
                prompt_sent,
                raw_response,
                actions_taken: entry.actions_produced.clone(),
                latency_ms,
            });
        }

        ReplayFile { header, traces }
    }

    // ── Utility ───────────────────────────────────────────────────────────────

    /// Number of entries currently held in the buffer.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the buffer contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Discard all entries (filters and enabled state are unchanged).
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::{Action, AgentId, AgentToolCallTrace, AgentType, Observation};

    // ── helpers ────────────────────────────────────────────────────────────

    fn make_entry(tick: u64, agent_id: AgentId, agent_type: AgentType) -> DecisionEntry {
        DecisionEntry {
            tick,
            agent_id,
            agent_type,
            observation_summary: ObservationSummary {
                visible_entity_count: 2,
                hostile_count: 1,
                friendly_count: 1,
                nearest_hostile_distance: Some(30.0),
                health_fraction: Some(0.75),
                position: Vec2::new(10.0, 20.0),
                active_objectives: 1,
            },
            decision_method: DecisionMethod::Scripted {
                rule_name: "patrol".into(),
            },
            actions_produced: vec![Action::Idle],
            timing: DecisionTiming {
                observe_us: 100,
                decide_us: 200,
                total_us: 300,
            },
            tool_calls: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    fn make_llm_entry(tick: u64, agent_id: AgentId) -> DecisionEntry {
        DecisionEntry {
            decision_method: DecisionMethod::LlmResponse {
                model: "claude-sonnet-4-6".into(),
                prompt_tokens: 256,
                completion_tokens: 64,
            },
            timing: DecisionTiming {
                observe_us: 500,
                decide_us: 120_000,
                total_us: 121_000,
            },
            ..make_entry(tick, agent_id, AgentType::LlmAgent)
        }
    }

    // ── test_log_entry ─────────────────────────────────────────────────────

    /// Basic logging and retrieval.
    #[test]
    fn test_log_entry() {
        let mut logger = DecisionLogger::new();
        let agent_id = AgentId::new();

        assert!(logger.is_empty());

        logger.log(make_entry(0, agent_id, AgentType::ScriptedNpc));
        logger.log(make_entry(1, agent_id, AgentType::ScriptedNpc));

        assert_eq!(logger.len(), 2);

        let all = logger.entries_for_agent(agent_id);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].tick, 0);
        assert_eq!(all[1].tick, 1);

        // entries_for_tick only returns the matching tick
        let tick1 = logger.entries_for_tick(1);
        assert_eq!(tick1.len(), 1);
        assert_eq!(tick1[0].agent_id, agent_id);
    }

    // ── test_ring_buffer_overflow ──────────────────────────────────────────

    /// When capacity is reached, oldest entries are evicted.
    #[test]
    fn test_ring_buffer_overflow() {
        let capacity = 5;
        let mut logger = DecisionLogger::with_capacity(capacity);
        let agent_id = AgentId::new();

        for tick in 0..10u64 {
            logger.log(make_entry(tick, agent_id, AgentType::ScriptedNpc));
        }

        // Should never exceed capacity
        assert_eq!(logger.len(), capacity);

        // The five entries kept must be the most recent (ticks 5–9)
        let remaining: Vec<u64> = logger.query(&[]).into_iter().map(|e| e.tick).collect();
        assert_eq!(remaining, vec![5, 6, 7, 8, 9]);
    }

    // ── test_filter_by_agent_type ──────────────────────────────────────────

    /// Query filters correctly select by agent type.
    #[test]
    fn test_filter_by_agent_type() {
        let mut logger = DecisionLogger::new();
        let npc_id = AgentId::new();
        let llm_id = AgentId::new();

        logger.log(make_entry(0, npc_id, AgentType::ScriptedNpc));
        logger.log(make_llm_entry(0, llm_id));
        logger.log(make_entry(1, npc_id, AgentType::ScriptedNpc));
        logger.log(make_llm_entry(1, llm_id));

        let npcs = logger.query(&[LogFilter::AgentType(AgentType::ScriptedNpc)]);
        assert_eq!(npcs.len(), 2);
        assert!(npcs.iter().all(|e| e.agent_type == AgentType::ScriptedNpc));

        let llms = logger.query(&[LogFilter::AgentType(AgentType::LlmAgent)]);
        assert_eq!(llms.len(), 2);
        assert!(llms.iter().all(|e| e.agent_type == AgentType::LlmAgent));
    }

    // ── test_filter_by_tick_range ──────────────────────────────────────────

    /// TickRange filter returns entries within [start, end] inclusive.
    #[test]
    fn test_filter_by_tick_range() {
        let mut logger = DecisionLogger::new();
        let agent_id = AgentId::new();

        for tick in 0..10u64 {
            logger.log(make_entry(tick, agent_id, AgentType::ScriptedNpc));
        }

        let mid = logger.query(&[LogFilter::TickRange { start: 3, end: 6 }]);
        assert_eq!(mid.len(), 4); // ticks 3, 4, 5, 6

        let ticks: Vec<u64> = mid.iter().map(|e| e.tick).collect();
        assert_eq!(ticks, vec![3, 4, 5, 6]);

        // Edge: single-tick range
        let single = logger.query(&[LogFilter::TickRange { start: 7, end: 7 }]);
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].tick, 7);

        // Out-of-range returns nothing
        let none = logger.query(&[LogFilter::TickRange { start: 20, end: 30 }]);
        assert!(none.is_empty());
    }

    // ── test_summary_stats ─────────────────────────────────────────────────

    /// Aggregate statistics are computed correctly.
    #[test]
    fn test_summary_stats() {
        let mut logger = DecisionLogger::new();
        let npc_id = AgentId::new();
        let llm_id = AgentId::new();

        // 3 NPC entries and 2 LLM entries across ticks 0–4
        logger.log(make_entry(0, npc_id, AgentType::ScriptedNpc)); // total_us = 300
        logger.log(make_entry(1, npc_id, AgentType::ScriptedNpc)); // 300
        logger.log(make_entry(2, npc_id, AgentType::ScriptedNpc)); // 300
        logger.log(make_llm_entry(3, llm_id)); // total_us = 121_000
        logger.log(make_llm_entry(4, llm_id)); // 121_000

        let stats = logger.summary_stats();

        assert_eq!(stats.total_entries, 5);
        assert_eq!(stats.unique_agents, 2);
        assert_eq!(stats.tick_range, (0, 4));
        assert_eq!(
            *stats
                .entries_by_agent_type
                .get(&AgentType::ScriptedNpc)
                .unwrap(),
            3
        );
        assert_eq!(
            *stats
                .entries_by_agent_type
                .get(&AgentType::LlmAgent)
                .unwrap(),
            2
        );
        assert_eq!(stats.max_decision_time_us, 121_000);

        // avg = (300*3 + 121_000*2) / 5 = (900 + 242_000) / 5 = 242_900 / 5 = 48_580
        let expected_avg = (900u64 + 242_000u64) / 5;
        assert_eq!(stats.avg_decision_time_us, expected_avg);

        // Empty logger returns zeroed stats
        let empty_stats = DecisionLogger::new().summary_stats();
        assert_eq!(empty_stats.total_entries, 0);
        assert_eq!(empty_stats.tick_range, (0, 0));
        assert_eq!(empty_stats.unique_agents, 0);
    }

    // ── test_to_replay_file ────────────────────────────────────────────────

    /// Conversion to ReplayFile produces the correct structure.
    #[test]
    fn test_to_replay_file() {
        let mut logger = DecisionLogger::new();
        let agent_a = AgentId::new();
        let agent_b = AgentId::new();

        logger.log(make_entry(0, agent_a, AgentType::ScriptedNpc));
        logger.log(make_llm_entry(0, agent_b));
        logger.log(make_entry(1, agent_a, AgentType::ScriptedNpc));

        let replay = logger.to_replay_file("unit-test", 99);

        assert_eq!(replay.header.name, "unit-test");
        assert_eq!(replay.header.world_seed, 99);
        assert_eq!(replay.header.tick_count, 2); // ticks 0 and 1 → count = max+1 = 2
        assert_eq!(replay.header.agent_count, 2);

        // Tick 0 should have two traces
        assert_eq!(replay.traces[0].len(), 2);
        // Tick 1 should have one trace
        assert_eq!(replay.traces[1].len(), 1);

        // LLM trace should carry model info in prompt_sent
        let llm_trace = replay.traces[0]
            .iter()
            .find(|t| t.agent_id == agent_b)
            .expect("LLM trace missing");
        assert!(llm_trace.prompt_sent.contains("claude-sonnet-4-6"));

        // All traces must carry the correct tick
        for (tick_idx, tick_traces) in replay.traces.iter().enumerate() {
            for trace in tick_traces {
                assert_eq!(trace.tick, tick_idx as u64);
            }
        }
    }

    // ── test_enable_disable ────────────────────────────────────────────────

    /// Disabled logger silently discards all log() calls.
    #[test]
    fn test_enable_disable() {
        let mut logger = DecisionLogger::new();
        let agent_id = AgentId::new();

        assert!(logger.is_enabled());

        logger.log(make_entry(0, agent_id, AgentType::ScriptedNpc));
        assert_eq!(logger.len(), 1);

        logger.disable();
        assert!(!logger.is_enabled());

        // These must be silently dropped
        logger.log(make_entry(1, agent_id, AgentType::ScriptedNpc));
        logger.log(make_entry(2, agent_id, AgentType::ScriptedNpc));
        assert_eq!(logger.len(), 1, "disabled logger must not grow");

        logger.enable();
        assert!(logger.is_enabled());

        logger.log(make_entry(3, agent_id, AgentType::ScriptedNpc));
        assert_eq!(
            logger.len(),
            2,
            "re-enabled logger should accept new entries"
        );
    }

    // ── test_observation_summary_from_observation ──────────────────────────

    /// ObservationSummary::from_observation correctly derives each field.
    #[test]
    fn test_observation_summary_from_observation() {
        use pod_core::id::EntityId;
        use pod_core::observation::{Objective, VisibleEntity};

        let obs = Observation {
            tick: 5,
            self_state: pod_core::observation::SelfState {
                health: Some(50.0),
                max_health: Some(100.0),
                position: Vec2::new(3.0, 4.0),
                ..Default::default()
            },
            visible_entities: vec![
                VisibleEntity {
                    entity_id: EntityId(1),
                    entity_type: "enemy".into(),
                    position: Vec2::new(10.0, 0.0),
                    velocity: Vec2::ZERO,
                    rotation: 0.0,
                    distance: 7.0,
                    relationship: Relationship::Hostile,
                    health_fraction: None,
                    ..Default::default()
                },
                VisibleEntity {
                    entity_id: EntityId(2),
                    entity_type: "ally".into(),
                    position: Vec2::new(5.0, 0.0),
                    velocity: Vec2::ZERO,
                    rotation: 0.0,
                    distance: 15.0,
                    relationship: Relationship::Friendly,
                    health_fraction: None,
                    ..Default::default()
                },
            ],
            objectives: vec![
                Objective {
                    id: "obj1".into(),
                    description: "Survive".into(),
                    progress: 0.5,
                    completed: false,
                },
                Objective {
                    id: "obj2".into(),
                    description: "Done".into(),
                    progress: 1.0,
                    completed: true,
                },
            ],
            ..Default::default()
        };

        let summary = ObservationSummary::from_observation(&obs);

        assert_eq!(summary.visible_entity_count, 2);
        assert_eq!(summary.hostile_count, 1);
        assert_eq!(summary.friendly_count, 1);
        assert_eq!(summary.nearest_hostile_distance, Some(7.0));
        assert!((summary.health_fraction.unwrap() - 0.5).abs() < f32::EPSILON);
        assert_eq!(summary.position, Vec2::new(3.0, 4.0));
        assert_eq!(summary.active_objectives, 1); // only the non-completed one
    }

    #[test]
    fn decision_entries_preserve_tool_call_telemetry() {
        let mut logger = DecisionLogger::new();
        let agent_id = AgentId::new();
        let mut entry = make_llm_entry(8, agent_id);
        entry.tool_calls.push(AgentToolCallTrace::success(
            8, "pathfind", "openai", 22, 144, 64,
        ));

        logger.log(entry);

        let stored = logger.entries_for_agent(agent_id);
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].tool_calls.len(), 1);
        assert_eq!(stored[0].tool_calls[0].tool_name, "pathfind");
        assert_eq!(stored[0].tool_calls[0].provider, "openai");
    }
}
