//! # Deterministic Replay System
//!
//! Records every AI agent decision to enable perfect replay.
//!
//! For deterministic simulations, the critical insight is that AI agents are the
//! non-deterministic element. By recording the exact decision each agent made for
//! each observation, we can replay that decision later without re-querying the API.
//!
//! This enables:
//! - Perfect reproducibility of complex multi-agent scenarios
//! - Efficient testing of game balance changes
//! - Debugging of specific agent behaviors
//! - Sharing exact replay scenarios across teams

use crate::id::{AgentId, EntityId};
use crate::observation::Observation;
use crate::action::Action;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Records a single agent's decision for a tick
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTrace {
    /// Which tick this decision was made for
    pub tick: u64,
    /// Which agent made this decision
    pub agent_id: AgentId,
    /// Hash of the observation fed to the agent
    /// Used to detect if replay preconditions differ
    pub observation_hash: u64,
    /// The actual text sent to the AI (for debugging/logging)
    pub prompt_sent: String,
    /// The raw response from the AI (for debugging/logging)
    pub raw_response: String,
    /// The actions the AI decided on (parsed from raw_response)
    pub actions_taken: Vec<Action>,
    /// How long the API took to respond (ms)
    pub latency_ms: u32,
}

/// A complete recording of one simulation run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayFile {
    /// Metadata about when/why this was recorded
    pub header: ReplayHeader,
    /// Traces indexed by [tick][agent_id]
    pub traces: Vec<Vec<DecisionTrace>>,
}

/// Metadata about a replay recording
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayHeader {
    /// Human-readable name
    pub name: String,
    /// When recorded (unix timestamp)
    pub timestamp: u64,
    /// Random seed used in the world
    pub world_seed: u64,
    /// How many ticks were recorded
    pub tick_count: u64,
    /// Agent count when recorded
    pub agent_count: usize,
    /// Optional notes about what happened
    pub notes: String,
}

/// Records agent decisions as the simulation runs
pub struct ReplayRecorder {
    /// All traces collected so far
    pub traces: Vec<DecisionTrace>,
    /// Mapping of (tick, agent_id) -> trace for quick lookup
    index: HashMap<(u64, AgentId), usize>,
}

impl ReplayRecorder {
    pub fn new() -> Self {
        Self {
            traces: Vec::new(),
            index: HashMap::new(),
        }
    }

    /// Record a decision that an agent made
    pub fn record_decision(
        &mut self,
        tick: u64,
        agent_id: AgentId,
        observation: &Observation,
        prompt_sent: String,
        raw_response: String,
        actions_taken: Vec<Action>,
        latency_ms: u32,
    ) {
        let observation_hash = hash_observation(observation);

        let trace = DecisionTrace {
            tick,
            agent_id,
            observation_hash,
            prompt_sent,
            raw_response,
            actions_taken,
            latency_ms,
        };

        let index = self.traces.len();
        self.index.insert((tick, agent_id), index);
        self.traces.push(trace);
    }

    /// Get a specific decision
    pub fn get_decision(&self, tick: u64, agent_id: AgentId) -> Option<&DecisionTrace> {
        self.index
            .get(&(tick, agent_id))
            .and_then(|idx| self.traces.get(*idx))
    }

    /// Finalize into a ReplayFile
    pub fn finalize(self, header: ReplayHeader) -> ReplayFile {
        // Group traces by tick
        let mut traces_by_tick: HashMap<u64, Vec<DecisionTrace>> = HashMap::new();
        for trace in self.traces {
            traces_by_tick
                .entry(trace.tick)
                .or_insert_with(Vec::new)
                .push(trace);
        }

        // Build sequential array
        let max_tick = header.tick_count;
        let mut traces = vec![Vec::new(); max_tick as usize];
        for (tick, tick_traces) in traces_by_tick {
            if tick < max_tick {
                traces[tick as usize] = tick_traces;
            }
        }

        ReplayFile { header, traces }
    }
}

impl Default for ReplayRecorder {
    fn default() -> Self {
        Self::new()
    }
}

/// Plays back recorded decisions instead of making live API calls
pub struct ReplayPlayer {
    /// The recording to replay
    file: ReplayFile,
    /// Current position in playback
    current_tick: u64,
    /// Tracks which agents had a decision replayed this tick
    replayed_this_tick: HashMap<AgentId, bool>,
}

impl ReplayPlayer {
    pub fn new(file: ReplayFile) -> Self {
        Self {
            file,
            current_tick: 0,
            replayed_this_tick: HashMap::new(),
        }
    }

    /// Try to get the next decision for an agent
    /// Returns None if this agent has no recorded decision for this tick
    pub fn get_next_decision(&mut self, agent_id: AgentId) -> Option<DecisionTrace> {
        let traces = self.file.traces.get(self.current_tick as usize)?;
        traces
            .iter()
            .find(|t| t.agent_id == agent_id)
            .cloned()
    }

    /// Advance to the next tick
    pub fn advance_tick(&mut self) {
        self.current_tick += 1;
        self.replayed_this_tick.clear();
    }

    /// Get the tick being replayed
    pub fn current_tick(&self) -> u64 {
        self.current_tick
    }

    /// Get the header info
    pub fn header(&self) -> &ReplayHeader {
        &self.file.header
    }

    /// Check if replay is complete
    pub fn is_done(&self) -> bool {
        self.current_tick >= self.file.header.tick_count
    }
}

/// Compute a hash of an observation for comparison
/// Used to verify that replay preconditions match actual conditions
fn hash_observation(obs: &Observation) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();

    // Hash key observation properties
    obs.tick.hash(&mut hasher);
    obs.self_state.position.x.to_bits().hash(&mut hasher);
    obs.self_state.position.y.to_bits().hash(&mut hasher);
    obs.self_state.health.unwrap_or(0.0).to_bits().hash(&mut hasher);
    obs.visible_entities.len().hash(&mut hasher);

    // Hash entity IDs to catch world state changes
    for entity in &obs.visible_entities {
        entity.entity_id.0.hash(&mut hasher);
        entity.distance.to_bits().hash(&mut hasher);
    }

    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replay_recorder() {
        let mut recorder = ReplayRecorder::new();
        let agent_id = AgentId::new();

        let obs = Observation {
            tick: 0,
            elapsed_secs: 0.0,
            self_state: Default::default(),
            visible_entities: vec![],
            audible_events: vec![],
            messages: vec![],
            available_actions: vec![],
            objectives: vec![],
        };

        recorder.record_decision(
            0,
            agent_id,
            &obs,
            "Move forward".to_string(),
            "Action: Move".to_string(),
            vec![Action::Idle],
            100,
        );

        let decision = recorder.get_decision(0, agent_id);
        assert!(decision.is_some());
        assert_eq!(decision.unwrap().tick, 0);
    }

    #[test]
    fn test_replay_player() {
        let header = ReplayHeader {
            name: "test".to_string(),
            timestamp: 0,
            world_seed: 42,
            tick_count: 10,
            agent_count: 1,
            notes: String::new(),
        };

        let traces = vec![vec![DecisionTrace {
            tick: 0,
            agent_id: AgentId::new(),
            observation_hash: 0,
            prompt_sent: String::new(),
            raw_response: String::new(),
            actions_taken: vec![],
            latency_ms: 0,
        }]; 10];

        let file = ReplayFile { header, traces };
        let mut player = ReplayPlayer::new(file);

        assert_eq!(player.current_tick(), 0);
        assert!(!player.is_done());

        player.advance_tick();
        assert_eq!(player.current_tick(), 1);
    }
}
