//! # Decision Logging and Replay System
//!
//! Higher-level decision logging that wraps `pod_core::replay::ReplayRecorder`
//! with agent-type-specific recording and rich querying capabilities.
//!
//! Provides:
//! - **DecisionLogger** — wraps core ReplayRecorder with specialized methods per agent type
//! - **DecisionFilter** — builder-pattern query API for filtering recorded decisions
//! - **DecisionStats** — aggregate statistics over recorded decisions
//! - **DecisionExporter** — export decisions to JSON, CSV, or ReplayFile format

use pod_core::action::Action;
use pod_core::id::AgentId;
use pod_core::observation::Observation;
use pod_core::replay::{DecisionTrace, ReplayFile, ReplayHeader, ReplayRecorder};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// DecisionLogger
// ---------------------------------------------------------------------------

/// Wraps `ReplayRecorder` with higher-level, agent-type-aware logging methods.
///
/// All decisions ultimately flow through the core recorder, but the logger
/// provides convenient helpers for LLM, scripted, and neural agent types
/// so callers don't need to manually construct prompt/response strings.
pub struct DecisionLogger {
    recorder: ReplayRecorder,
}

impl DecisionLogger {
    /// Create a new, empty decision logger.
    pub fn new() -> Self {
        Self {
            recorder: ReplayRecorder::new(),
        }
    }

    /// Record a generic decision. This delegates directly to the underlying
    /// `ReplayRecorder::record_decision`.
    pub fn log_decision(
        &mut self,
        tick: u64,
        agent_id: AgentId,
        observation: &Observation,
        prompt_sent: String,
        raw_response: String,
        actions_taken: Vec<Action>,
        latency_ms: u32,
    ) {
        self.recorder.record_decision(
            tick,
            agent_id,
            observation,
            prompt_sent,
            raw_response,
            actions_taken,
            latency_ms,
        );
    }

    /// Record a decision from an LLM-based agent.
    ///
    /// Captures the full prompt/response cycle with latency for debugging
    /// and replay of LLM interactions.
    pub fn log_llm_decision(
        &mut self,
        tick: u64,
        agent_id: AgentId,
        observation: &Observation,
        prompt: String,
        response: String,
        actions: Vec<Action>,
        latency_ms: u32,
    ) {
        self.recorder.record_decision(
            tick,
            agent_id,
            observation,
            prompt,
            response,
            actions,
            latency_ms,
        );
    }

    /// Record a decision from a scripted (behavior tree / FSM) agent.
    ///
    /// Scripted agents don't have prompts or raw responses — instead we
    /// record the behavior name as the "prompt" and leave the response empty.
    /// Latency is always 0 since scripted decisions are synchronous.
    pub fn log_scripted_decision(
        &mut self,
        tick: u64,
        agent_id: AgentId,
        observation: &Observation,
        behavior_name: &str,
        actions: Vec<Action>,
    ) {
        self.recorder.record_decision(
            tick,
            agent_id,
            observation,
            format!("[scripted] {}", behavior_name),
            String::new(),
            actions,
            0,
        );
    }

    /// Record a decision from a neural network agent.
    ///
    /// Neural agents produce an action index and confidence score rather
    /// than text. These are encoded into the prompt/response fields for
    /// unified storage in the DecisionTrace format.
    pub fn log_neural_decision(
        &mut self,
        tick: u64,
        agent_id: AgentId,
        observation: &Observation,
        action_index: usize,
        confidence: f32,
        actions: Vec<Action>,
    ) {
        self.recorder.record_decision(
            tick,
            agent_id,
            observation,
            format!("[neural] action_index={}", action_index),
            format!("confidence={:.4}", confidence),
            actions,
            0,
        );
    }

    /// Total number of decisions recorded.
    pub fn decision_count(&self) -> usize {
        self.recorder.traces.len()
    }

    /// Return all decisions made by a specific agent.
    pub fn decisions_for_agent(&self, agent_id: AgentId) -> Vec<&DecisionTrace> {
        self.recorder
            .traces
            .iter()
            .filter(|t| t.agent_id == agent_id)
            .collect()
    }

    /// Return all decisions made at a specific tick.
    pub fn decisions_at_tick(&self, tick: u64) -> Vec<&DecisionTrace> {
        self.recorder
            .traces
            .iter()
            .filter(|t| t.tick == tick)
            .collect()
    }

    /// Borrow the underlying recorder (read-only).
    pub fn recorder(&self) -> &ReplayRecorder {
        &self.recorder
    }

    /// Consume the logger and return the underlying recorder.
    pub fn into_recorder(self) -> ReplayRecorder {
        self.recorder
    }

    /// Borrow all traces.
    pub fn traces(&self) -> &[DecisionTrace] {
        &self.recorder.traces
    }
}

impl Default for DecisionLogger {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// DecisionFilter
// ---------------------------------------------------------------------------

/// Builder-pattern filter for querying recorded decisions.
///
/// # Example
/// ```ignore
/// let slow_llm = DecisionFilter::new()
///     .agent(my_agent)
///     .min_latency(500)
///     .tick_range(0, 1000)
///     .apply(&logger);
/// ```
#[derive(Debug, Clone, Default)]
pub struct DecisionFilter {
    agent_id: Option<AgentId>,
    tick_start: Option<u64>,
    tick_end: Option<u64>,
    min_latency_ms: Option<u32>,
    action_type_name: Option<String>,
}

impl DecisionFilter {
    /// Create a new filter with no constraints (matches everything).
    pub fn new() -> Self {
        Self::default()
    }

    /// Filter to decisions by a specific agent.
    pub fn agent(mut self, agent_id: AgentId) -> Self {
        self.agent_id = Some(agent_id);
        self
    }

    /// Filter to decisions within a tick range (inclusive on both ends).
    pub fn tick_range(mut self, start: u64, end: u64) -> Self {
        self.tick_start = Some(start);
        self.tick_end = Some(end);
        self
    }

    /// Filter to decisions with latency >= the given threshold.
    /// Useful for finding slow LLM decisions.
    pub fn min_latency(mut self, ms: u32) -> Self {
        self.min_latency_ms = Some(ms);
        self
    }

    /// Filter to decisions that contain a specific action type.
    ///
    /// The `action_name` is matched against the Debug representation of
    /// each `Action` variant (e.g., `"Move"`, `"Attack"`, `"Idle"`).
    pub fn action_type(mut self, action_name: &str) -> Self {
        self.action_type_name = Some(action_name.to_string());
        self
    }

    /// Apply this filter against a `DecisionLogger` and return matching traces.
    pub fn apply<'a>(&self, logger: &'a DecisionLogger) -> Vec<&'a DecisionTrace> {
        logger
            .traces()
            .iter()
            .filter(|trace| self.matches(trace))
            .collect()
    }

    /// Check whether a single trace passes all filter criteria.
    fn matches(&self, trace: &DecisionTrace) -> bool {
        if let Some(agent_id) = self.agent_id {
            if trace.agent_id != agent_id {
                return false;
            }
        }
        if let Some(start) = self.tick_start {
            if trace.tick < start {
                return false;
            }
        }
        if let Some(end) = self.tick_end {
            if trace.tick > end {
                return false;
            }
        }
        if let Some(min_lat) = self.min_latency_ms {
            if trace.latency_ms < min_lat {
                return false;
            }
        }
        if let Some(ref action_name) = self.action_type_name {
            let has_action = trace.actions_taken.iter().any(|a| {
                let dbg = format!("{:?}", a);
                dbg.starts_with(action_name.as_str())
            });
            if !has_action {
                return false;
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// DecisionStats
// ---------------------------------------------------------------------------

/// Aggregate statistics computed from a `DecisionLogger`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionStats {
    /// Total number of recorded decisions.
    pub total_decisions: usize,
    /// Average latency across all decisions (ms).
    pub avg_latency_ms: f32,
    /// Maximum latency observed (ms).
    pub max_latency_ms: u32,
    /// Number of decisions per agent.
    pub decisions_per_agent: HashMap<AgentId, usize>,
    /// The most frequently occurring action type (Debug name).
    pub most_common_action: String,
    /// Distribution of action types across all decisions.
    pub action_distribution: HashMap<String, usize>,
}

impl DecisionStats {
    /// Compute aggregate statistics from a logger's recorded decisions.
    pub fn from_logger(logger: &DecisionLogger) -> Self {
        let traces = logger.traces();

        if traces.is_empty() {
            return Self {
                total_decisions: 0,
                avg_latency_ms: 0.0,
                max_latency_ms: 0,
                decisions_per_agent: HashMap::new(),
                most_common_action: String::new(),
                action_distribution: HashMap::new(),
            };
        }

        let total_decisions = traces.len();

        // Latency stats
        let total_latency: u64 = traces.iter().map(|t| t.latency_ms as u64).sum();
        let avg_latency_ms = total_latency as f32 / total_decisions as f32;
        let max_latency_ms = traces.iter().map(|t| t.latency_ms).max().unwrap_or(0);

        // Per-agent counts
        let mut decisions_per_agent: HashMap<AgentId, usize> = HashMap::new();
        for trace in traces {
            *decisions_per_agent.entry(trace.agent_id).or_insert(0) += 1;
        }

        // Action distribution
        let mut action_distribution: HashMap<String, usize> = HashMap::new();
        for trace in traces {
            for action in &trace.actions_taken {
                let name = action_variant_name(action);
                *action_distribution.entry(name).or_insert(0) += 1;
            }
        }

        let most_common_action = action_distribution
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(name, _)| name.clone())
            .unwrap_or_default();

        Self {
            total_decisions,
            avg_latency_ms,
            max_latency_ms,
            decisions_per_agent,
            most_common_action,
            action_distribution,
        }
    }
}

/// Extract the variant name from an Action for counting purposes.
fn action_variant_name(action: &Action) -> String {
    let dbg = format!("{:?}", action);
    // The Debug output looks like "Move { direction: ... }" or "Idle"
    // We want just the variant name.
    dbg.split_whitespace()
        .next()
        .unwrap_or("Unknown")
        .to_string()
}

// ---------------------------------------------------------------------------
// DecisionExporter
// ---------------------------------------------------------------------------

/// Export decisions to various external formats.
pub struct DecisionExporter;

/// Lightweight serializable view of a single decision for JSON/CSV export.
#[derive(Debug, Serialize, Deserialize)]
struct ExportedDecision {
    tick: u64,
    agent_id: String,
    observation_hash: u64,
    prompt_sent: String,
    raw_response: String,
    actions: Vec<String>,
    latency_ms: u32,
}

impl From<&DecisionTrace> for ExportedDecision {
    fn from(trace: &DecisionTrace) -> Self {
        Self {
            tick: trace.tick,
            agent_id: trace.agent_id.0.to_string(),
            observation_hash: trace.observation_hash,
            prompt_sent: trace.prompt_sent.clone(),
            raw_response: trace.raw_response.clone(),
            actions: trace
                .actions_taken
                .iter()
                .map(|a| format!("{:?}", a))
                .collect(),
            latency_ms: trace.latency_ms,
        }
    }
}

impl DecisionExporter {
    /// Serialize all decisions to a JSON string.
    pub fn to_json(logger: &DecisionLogger) -> String {
        let exported: Vec<ExportedDecision> =
            logger.traces().iter().map(ExportedDecision::from).collect();
        serde_json::to_string_pretty(&exported).unwrap_or_else(|_| "[]".to_string())
    }

    /// Export decisions as CSV (tick, agent_id, actions, latency_ms).
    pub fn to_csv(logger: &DecisionLogger) -> String {
        let mut csv = String::from("tick,agent_id,actions,latency_ms\n");
        for trace in logger.traces() {
            let actions: Vec<String> = trace
                .actions_taken
                .iter()
                .map(|a| action_variant_name(a))
                .collect();
            csv.push_str(&format!(
                "{},{},{},{}\n",
                trace.tick,
                trace.agent_id.0,
                actions.join(";"),
                trace.latency_ms,
            ));
        }
        csv
    }

    /// Convert a `DecisionLogger` into a core `ReplayFile`.
    pub fn to_replay_file(logger: DecisionLogger, header: ReplayHeader) -> ReplayFile {
        logger.into_recorder().finalize(header)
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::id::AgentId;
    use pod_core::observation::{Observation, SelfState};
    use pod_core::component::Team;
    use glam::Vec2;

    /// Helper: build a minimal observation for testing.
    fn test_observation(tick: u64) -> Observation {
        Observation {
            tick,
            elapsed_secs: tick as f32 / 60.0,
            self_state: SelfState {
                agent_id: AgentId::new(),
                entity_id: pod_core::id::EntityId(1),
                position: Vec2::ZERO,
                rotation: 0.0,
                velocity: Vec2::ZERO,
                health: Some(100.0),
                max_health: Some(100.0),
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

    // -----------------------------------------------------------------------
    // 1. Log LLM decision
    // -----------------------------------------------------------------------
    #[test]
    fn test_log_llm_decision() {
        let mut logger = DecisionLogger::new();
        let agent = AgentId::new();
        let obs = test_observation(0);

        logger.log_llm_decision(
            0,
            agent,
            &obs,
            "You see nothing.".to_string(),
            "Action: Idle".to_string(),
            vec![Action::Idle],
            250,
        );

        assert_eq!(logger.decision_count(), 1);
        let decisions = logger.decisions_for_agent(agent);
        assert_eq!(decisions.len(), 1);
        assert_eq!(decisions[0].latency_ms, 250);
        assert_eq!(decisions[0].prompt_sent, "You see nothing.");
    }

    // -----------------------------------------------------------------------
    // 2. Log scripted decision
    // -----------------------------------------------------------------------
    #[test]
    fn test_log_scripted_decision() {
        let mut logger = DecisionLogger::new();
        let agent = AgentId::new();
        let obs = test_observation(5);

        logger.log_scripted_decision(5, agent, &obs, "patrol_route_alpha", vec![
            Action::Move { direction: Vec2::X },
        ]);

        assert_eq!(logger.decision_count(), 1);
        let d = &logger.traces()[0];
        assert!(d.prompt_sent.contains("patrol_route_alpha"));
        assert_eq!(d.latency_ms, 0); // scripted is synchronous
    }

    // -----------------------------------------------------------------------
    // 3. Log neural decision
    // -----------------------------------------------------------------------
    #[test]
    fn test_log_neural_decision() {
        let mut logger = DecisionLogger::new();
        let agent = AgentId::new();
        let obs = test_observation(10);

        logger.log_neural_decision(10, agent, &obs, 3, 0.92, vec![Action::Attack]);

        assert_eq!(logger.decision_count(), 1);
        let d = &logger.traces()[0];
        assert!(d.prompt_sent.contains("action_index=3"));
        assert!(d.raw_response.contains("confidence=0.9200"));
    }

    // -----------------------------------------------------------------------
    // 4. Query by agent
    // -----------------------------------------------------------------------
    #[test]
    fn test_decisions_for_agent() {
        let mut logger = DecisionLogger::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let obs = test_observation(0);

        logger.log_llm_decision(0, a1, &obs, String::new(), String::new(), vec![Action::Idle], 100);
        logger.log_llm_decision(1, a2, &obs, String::new(), String::new(), vec![Action::Idle], 200);
        logger.log_llm_decision(2, a1, &obs, String::new(), String::new(), vec![Action::Attack], 150);

        let a1_decisions = logger.decisions_for_agent(a1);
        assert_eq!(a1_decisions.len(), 2);

        let a2_decisions = logger.decisions_for_agent(a2);
        assert_eq!(a2_decisions.len(), 1);
    }

    // -----------------------------------------------------------------------
    // 5. Query by tick
    // -----------------------------------------------------------------------
    #[test]
    fn test_decisions_at_tick() {
        let mut logger = DecisionLogger::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let obs = test_observation(0);

        logger.log_llm_decision(0, a1, &obs, String::new(), String::new(), vec![Action::Idle], 10);
        logger.log_llm_decision(0, a2, &obs, String::new(), String::new(), vec![Action::Attack], 20);
        logger.log_llm_decision(1, a1, &obs, String::new(), String::new(), vec![Action::Stop], 30);

        let tick0 = logger.decisions_at_tick(0);
        assert_eq!(tick0.len(), 2);

        let tick1 = logger.decisions_at_tick(1);
        assert_eq!(tick1.len(), 1);

        let tick99 = logger.decisions_at_tick(99);
        assert!(tick99.is_empty());
    }

    // -----------------------------------------------------------------------
    // 6. DecisionFilter — various criteria
    // -----------------------------------------------------------------------
    #[test]
    fn test_decision_filter() {
        let mut logger = DecisionLogger::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let obs = test_observation(0);

        // a1: tick 0 (latency 50), tick 5 (latency 500), tick 10 (latency 100)
        logger.log_llm_decision(0, a1, &obs, String::new(), String::new(), vec![Action::Idle], 50);
        logger.log_llm_decision(5, a1, &obs, String::new(), String::new(), vec![Action::Attack], 500);
        logger.log_llm_decision(10, a1, &obs, String::new(), String::new(), vec![Action::Idle], 100);
        // a2: tick 3 (latency 300)
        logger.log_llm_decision(3, a2, &obs, String::new(), String::new(), vec![Action::Stop], 300);

        // Filter: agent a1 only
        let results = DecisionFilter::new().agent(a1).apply(&logger);
        assert_eq!(results.len(), 3);

        // Filter: tick range 0..=5
        let results = DecisionFilter::new().tick_range(0, 5).apply(&logger);
        assert_eq!(results.len(), 3); // tick 0 (a1), tick 5 (a1), tick 3 (a2)

        // Filter: min latency 200
        let results = DecisionFilter::new().min_latency(200).apply(&logger);
        assert_eq!(results.len(), 2); // 500 (a1) and 300 (a2)

        // Filter: action type "Attack"
        let results = DecisionFilter::new().action_type("Attack").apply(&logger);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tick, 5);

        // Filter: combined — agent a1, min latency 100
        let results = DecisionFilter::new()
            .agent(a1)
            .min_latency(100)
            .apply(&logger);
        assert_eq!(results.len(), 2); // latency 500 and 100
    }

    // -----------------------------------------------------------------------
    // 7. DecisionStats computation
    // -----------------------------------------------------------------------
    #[test]
    fn test_decision_stats() {
        let mut logger = DecisionLogger::new();
        let a1 = AgentId::new();
        let a2 = AgentId::new();
        let obs = test_observation(0);

        logger.log_llm_decision(0, a1, &obs, String::new(), String::new(), vec![Action::Idle], 100);
        logger.log_llm_decision(1, a1, &obs, String::new(), String::new(), vec![Action::Idle], 200);
        logger.log_llm_decision(2, a2, &obs, String::new(), String::new(), vec![Action::Attack], 300);

        let stats = DecisionStats::from_logger(&logger);

        assert_eq!(stats.total_decisions, 3);
        assert!((stats.avg_latency_ms - 200.0).abs() < 0.1);
        assert_eq!(stats.max_latency_ms, 300);
        assert_eq!(stats.decisions_per_agent[&a1], 2);
        assert_eq!(stats.decisions_per_agent[&a2], 1);
        assert_eq!(stats.most_common_action, "Idle");
        assert_eq!(stats.action_distribution["Idle"], 2);
        assert_eq!(stats.action_distribution["Attack"], 1);
    }

    // -----------------------------------------------------------------------
    // 8. JSON export/import roundtrip
    // -----------------------------------------------------------------------
    #[test]
    fn test_json_export_roundtrip() {
        let mut logger = DecisionLogger::new();
        let agent = AgentId::new();
        let obs = test_observation(0);

        logger.log_llm_decision(
            0,
            agent,
            &obs,
            "prompt".to_string(),
            "response".to_string(),
            vec![Action::Idle],
            42,
        );

        let json = DecisionExporter::to_json(&logger);
        assert!(!json.is_empty());

        // Parse back
        let parsed: Vec<ExportedDecision> =
            serde_json::from_str(&json).expect("JSON should round-trip");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].tick, 0);
        assert_eq!(parsed[0].latency_ms, 42);
        assert_eq!(parsed[0].prompt_sent, "prompt");
        assert_eq!(parsed[0].raw_response, "response");
    }

    // -----------------------------------------------------------------------
    // 9. CSV export format
    // -----------------------------------------------------------------------
    #[test]
    fn test_csv_export_format() {
        let mut logger = DecisionLogger::new();
        let agent = AgentId::new();
        let obs = test_observation(0);

        logger.log_llm_decision(
            7,
            agent,
            &obs,
            String::new(),
            String::new(),
            vec![Action::Idle, Action::Stop],
            55,
        );

        let csv = DecisionExporter::to_csv(&logger);

        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "tick,agent_id,actions,latency_ms");
        assert!(lines[1].starts_with("7,"));
        assert!(lines[1].contains("Idle;Stop"));
        assert!(lines[1].ends_with(",55"));
    }

    // -----------------------------------------------------------------------
    // 10. Empty logger handling
    // -----------------------------------------------------------------------
    #[test]
    fn test_empty_logger() {
        let logger = DecisionLogger::new();

        assert_eq!(logger.decision_count(), 0);
        assert!(logger.decisions_for_agent(AgentId::new()).is_empty());
        assert!(logger.decisions_at_tick(0).is_empty());

        let stats = DecisionStats::from_logger(&logger);
        assert_eq!(stats.total_decisions, 0);
        assert_eq!(stats.avg_latency_ms, 0.0);
        assert_eq!(stats.max_latency_ms, 0);
        assert!(stats.most_common_action.is_empty());

        let json = DecisionExporter::to_json(&logger);
        assert_eq!(json, "[]");

        let csv = DecisionExporter::to_csv(&logger);
        assert_eq!(csv, "tick,agent_id,actions,latency_ms\n");
    }

    // -----------------------------------------------------------------------
    // 11. Decision count tracking
    // -----------------------------------------------------------------------
    #[test]
    fn test_decision_count_tracking() {
        let mut logger = DecisionLogger::new();
        let agent = AgentId::new();
        let obs = test_observation(0);

        assert_eq!(logger.decision_count(), 0);

        logger.log_llm_decision(0, agent, &obs, String::new(), String::new(), vec![], 0);
        assert_eq!(logger.decision_count(), 1);

        logger.log_scripted_decision(1, agent, &obs, "wander", vec![Action::Idle]);
        assert_eq!(logger.decision_count(), 2);

        logger.log_neural_decision(2, agent, &obs, 0, 0.5, vec![Action::Stop]);
        assert_eq!(logger.decision_count(), 3);
    }

    // -----------------------------------------------------------------------
    // 12. Replay file conversion
    // -----------------------------------------------------------------------
    #[test]
    fn test_replay_file_conversion() {
        let mut logger = DecisionLogger::new();
        let agent = AgentId::new();
        let obs = test_observation(0);

        logger.log_llm_decision(0, agent, &obs, String::new(), String::new(), vec![Action::Idle], 10);
        logger.log_llm_decision(1, agent, &obs, String::new(), String::new(), vec![Action::Stop], 20);

        let header = ReplayHeader {
            name: "test_replay".to_string(),
            timestamp: 1234567890,
            world_seed: 42,
            tick_count: 5,
            agent_count: 1,
            notes: "unit test".to_string(),
        };

        let replay = DecisionExporter::to_replay_file(logger, header);

        assert_eq!(replay.header.name, "test_replay");
        assert_eq!(replay.header.tick_count, 5);
        assert_eq!(replay.traces.len(), 5); // tick_count slots
        assert_eq!(replay.traces[0].len(), 1); // 1 decision at tick 0
        assert_eq!(replay.traces[1].len(), 1); // 1 decision at tick 1
        assert!(replay.traces[2].is_empty()); // no decisions at tick 2
    }

    // -----------------------------------------------------------------------
    // 13. Filter with no criteria matches everything
    // -----------------------------------------------------------------------
    #[test]
    fn test_filter_no_criteria() {
        let mut logger = DecisionLogger::new();
        let obs = test_observation(0);

        logger.log_llm_decision(0, AgentId::new(), &obs, String::new(), String::new(), vec![Action::Idle], 0);
        logger.log_llm_decision(1, AgentId::new(), &obs, String::new(), String::new(), vec![Action::Stop], 0);

        let results = DecisionFilter::new().apply(&logger);
        assert_eq!(results.len(), 2);
    }
}
