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

use crate::action::Action;
use crate::component::{EncounterKind, EncounterState};
use crate::id::AgentId;
use crate::observation::Observation;
use crate::telemetry::{
    ActionLifecycleStage, AgentRewardSignal, AgentToolCallTrace, TickTelemetryFrame, ToolCallStatus,
};
use crate::toon::encode_toon_document;
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
    /// Tool/provider side effects associated with the decision.
    #[serde(default)]
    pub tool_calls: Vec<AgentToolCallTrace>,
    /// How long the API took to respond (ms)
    pub latency_ms: u32,
}

impl DecisionTrace {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("decision_trace", self)
    }
}

/// A complete recording of one simulation run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayFile {
    /// Metadata about when/why this was recorded
    pub header: ReplayHeader,
    /// Traces indexed by [tick][agent_id]
    pub traces: Vec<Vec<DecisionTrace>>,
    /// Optional authoritative telemetry windows embedded alongside decision traces.
    #[serde(default)]
    pub telemetry_windows: Vec<TickTelemetryFrame>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionOutcomeSummary {
    pub submitted: usize,
    pub executed: usize,
    pub rejected: usize,
    pub queued: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EncounterTransition {
    Joined {
        encounter_id: u64,
        kind: EncounterKind,
    },
    Left {
        encounter_id: u64,
        kind: EncounterKind,
    },
    CombatStateChanged {
        encounter_id: u64,
        in_combat: bool,
    },
    CaptureAvailabilityChanged {
        encounter_id: u64,
        capture_allowed: bool,
    },
    TargetChanged {
        encounter_id: u64,
        primary_target: Option<crate::id::EntityId>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayTrainingSample {
    pub tick: u64,
    pub agent_id: AgentId,
    pub path_distance: f32,
    pub action_outcomes: ActionOutcomeSummary,
    pub encounter_transition: Option<EncounterTransition>,
    pub tool_call_latency_ms: u32,
    pub tool_call_error_count: usize,
    pub reward_summary: RewardAttributionSummary,
}

impl ReplayTrainingSample {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("replay_training_sample", self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RewardAttributionSummary {
    pub signal_count: usize,
    pub total: f32,
    pub positive_total: f32,
    pub negative_total: f32,
    pub terminal: bool,
}

impl RewardAttributionSummary {
    pub fn from_signals(signals: &[AgentRewardSignal]) -> Self {
        let mut total = 0.0f32;
        let mut positive_total = 0.0f32;
        let mut negative_total = 0.0f32;
        let mut terminal = false;

        for signal in signals {
            total += signal.value;
            if signal.value >= 0.0 {
                positive_total += signal.value;
            } else {
                negative_total += signal.value;
            }
            terminal |= signal.terminal;
        }

        Self {
            signal_count: signals.len(),
            total,
            positive_total,
            negative_total,
            terminal,
        }
    }
}

impl ReplayFile {
    pub fn training_samples(&self) -> Vec<ReplayTrainingSample> {
        let mut samples = Vec::new();
        let mut previous_encounters = HashMap::<AgentId, EncounterState>::new();

        for window in &self.telemetry_windows {
            for agent in &window.agents {
                let mut action_outcomes = ActionOutcomeSummary::default();
                for trace in &agent.action_trace {
                    match trace.stage {
                        ActionLifecycleStage::Submitted => action_outcomes.submitted += 1,
                        ActionLifecycleStage::Executed => action_outcomes.executed += 1,
                        ActionLifecycleStage::Rejected => action_outcomes.rejected += 1,
                        ActionLifecycleStage::Queued => action_outcomes.queued += 1,
                    }
                }

                let encounter_transition = classify_encounter_transition(
                    previous_encounters.get(&agent.agent_id),
                    agent.encounter.as_ref(),
                );
                if let Some(encounter) = &agent.encounter {
                    previous_encounters.insert(agent.agent_id, encounter.clone());
                } else {
                    previous_encounters.remove(&agent.agent_id);
                }

                let tool_call_latency_ms = agent
                    .tool_calls
                    .iter()
                    .map(|trace| trace.latency_ms)
                    .sum::<u32>();
                let tool_call_error_count = agent
                    .tool_calls
                    .iter()
                    .filter(|trace| {
                        !matches!(
                            trace.status,
                            ToolCallStatus::Requested | ToolCallStatus::Succeeded
                        )
                    })
                    .count();
                let reward_summary = RewardAttributionSummary::from_signals(&agent.reward_signals);

                samples.push(ReplayTrainingSample {
                    tick: window.tick,
                    agent_id: agent.agent_id,
                    path_distance: agent
                        .trajectory
                        .as_ref()
                        .map(|trajectory| trajectory.distance_travelled)
                        .unwrap_or_default(),
                    action_outcomes,
                    encounter_transition,
                    tool_call_latency_ms,
                    tool_call_error_count,
                    reward_summary,
                });
            }
        }

        samples
    }

    pub fn to_toon_document(&self) -> String {
        #[derive(Serialize)]
        struct ReplayExport<'a> {
            header: &'a ReplayHeader,
            traces: &'a [Vec<DecisionTrace>],
            telemetry_windows: &'a [TickTelemetryFrame],
            training_samples: Vec<ReplayTrainingSample>,
        }

        encode_toon_document(
            "replay_file",
            &ReplayExport {
                header: &self.header,
                traces: &self.traces,
                telemetry_windows: &self.telemetry_windows,
                training_samples: self.training_samples(),
            },
        )
    }

    pub fn training_samples_to_toon_document(&self) -> String {
        encode_toon_document("replay_training_samples", &self.training_samples())
    }
}

fn classify_encounter_transition(
    previous: Option<&EncounterState>,
    current: Option<&EncounterState>,
) -> Option<EncounterTransition> {
    match (previous, current) {
        (None, Some(current)) => Some(EncounterTransition::Joined {
            encounter_id: current.encounter_id,
            kind: current.kind,
        }),
        (Some(previous), None) => Some(EncounterTransition::Left {
            encounter_id: previous.encounter_id,
            kind: previous.kind,
        }),
        (Some(previous), Some(current)) if previous.encounter_id != current.encounter_id => {
            Some(EncounterTransition::Joined {
                encounter_id: current.encounter_id,
                kind: current.kind,
            })
        }
        (Some(previous), Some(current)) if previous.in_combat != current.in_combat => {
            Some(EncounterTransition::CombatStateChanged {
                encounter_id: current.encounter_id,
                in_combat: current.in_combat,
            })
        }
        (Some(previous), Some(current)) if previous.capture_allowed != current.capture_allowed => {
            Some(EncounterTransition::CaptureAvailabilityChanged {
                encounter_id: current.encounter_id,
                capture_allowed: current.capture_allowed,
            })
        }
        (Some(previous), Some(current)) if previous.primary_target != current.primary_target => {
            Some(EncounterTransition::TargetChanged {
                encounter_id: current.encounter_id,
                primary_target: current.primary_target,
            })
        }
        _ => None,
    }
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
        tool_calls: Vec<AgentToolCallTrace>,
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
            tool_calls,
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
        self.finalize_with_telemetry(header, Vec::new())
    }

    /// Finalize into a `ReplayFile` with embedded authoritative telemetry windows.
    pub fn finalize_with_telemetry(
        self,
        header: ReplayHeader,
        telemetry_windows: Vec<TickTelemetryFrame>,
    ) -> ReplayFile {
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

        ReplayFile {
            header,
            traces,
            telemetry_windows,
        }
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
        traces.iter().find(|t| t.agent_id == agent_id).cloned()
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
    obs.self_state
        .health
        .unwrap_or(0.0)
        .to_bits()
        .hash(&mut hasher);
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
    use crate::contract::{AgentCapabilities, AgentRole, AgentRuntimeProfile};
    use crate::telemetry::{
        ActionLifecycleStage, ActionSource, AgentRewardSignal, AgentTelemetryFrame, RewardReason,
        RewardSource, TrajectorySample,
    };
    use crate::toon::decode_toon_value;
    use glam::Vec2;

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
            Vec::new(),
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

        let traces = vec![
            vec![DecisionTrace {
                tick: 0,
                agent_id: AgentId::new(),
                observation_hash: 0,
                prompt_sent: String::new(),
                raw_response: String::new(),
                actions_taken: vec![],
                tool_calls: vec![],
                latency_ms: 0,
            }];
            10
        ];

        let file = ReplayFile {
            header,
            traces,
            telemetry_windows: vec![],
        };
        let mut player = ReplayPlayer::new(file);

        assert_eq!(player.current_tick(), 0);
        assert!(!player.is_done());

        player.advance_tick();
        assert_eq!(player.current_tick(), 1);
    }

    #[test]
    fn replay_recorder_can_embed_telemetry_windows() {
        let recorder = ReplayRecorder::new();
        let header = ReplayHeader {
            name: "telemetry".into(),
            timestamp: 0,
            world_seed: 1,
            tick_count: 1,
            agent_count: 0,
            notes: String::new(),
        };
        let file = recorder.finalize_with_telemetry(header, vec![TickTelemetryFrame::empty(0)]);
        assert_eq!(file.telemetry_windows.len(), 1);
        assert_eq!(file.telemetry_windows[0].tick, 0);
    }

    #[test]
    fn replay_training_samples_capture_path_action_and_encounter_transitions() {
        let agent_id = AgentId::new();
        let profile = AgentRuntimeProfile {
            role: AgentRole::Player,
            agent_type: crate::agent::AgentType::LlmAgent,
            capabilities: AgentCapabilities::player_default(),
        };

        let mut first = AgentTelemetryFrame::new(
            0,
            agent_id,
            None,
            profile,
            0,
            0,
            0,
            0,
            0,
            Some(EncounterState {
                encounter_id: 7,
                kind: EncounterKind::WildCreature,
                threat_level: 0.5,
                primary_target: None,
                active_turn_owner: None,
                capture_allowed: false,
                in_combat: true,
            }),
            Some(TrajectorySample::new(0, 0.0, Vec2::ZERO, Vec2::ZERO, 0.0)),
        );
        first.update_trajectory_end(TrajectorySample::new(
            0,
            1.0 / 60.0,
            Vec2::new(3.0, 4.0),
            Vec2::ZERO,
            0.0,
        ));
        first.record_action(
            ActionSource::AgentDecision,
            ActionLifecycleStage::Submitted,
            Action::Attack,
            None,
        );
        first.record_action(
            ActionSource::AgentDecision,
            ActionLifecycleStage::Executed,
            Action::Attack,
            None,
        );
        first.record_tool_call(AgentToolCallTrace::new(
            0,
            "llm.complete",
            "mock",
            ToolCallStatus::ParseError,
            12,
            100,
            0,
            Some("bad json".into()),
        ));
        first.record_reward(AgentRewardSignal::new(
            0,
            RewardSource::ActionOutcome,
            RewardReason::ActionExecuted,
            0.05,
            false,
            None,
        ));
        first.record_reward(AgentRewardSignal::new(
            0,
            RewardSource::WorldEvent,
            RewardReason::DamageDealt,
            1.0,
            false,
            None,
        ));

        let mut second = AgentTelemetryFrame::new(
            1,
            agent_id,
            None,
            profile,
            0,
            0,
            0,
            0,
            0,
            Some(EncounterState {
                encounter_id: 7,
                kind: EncounterKind::WildCreature,
                threat_level: 0.2,
                primary_target: None,
                active_turn_owner: None,
                capture_allowed: true,
                in_combat: false,
            }),
            Some(TrajectorySample::new(
                1,
                1.0 / 60.0,
                Vec2::new(3.0, 4.0),
                Vec2::ZERO,
                0.0,
            )),
        );
        second.update_trajectory_end(TrajectorySample::new(
            1,
            2.0 / 60.0,
            Vec2::new(4.0, 4.0),
            Vec2::ZERO,
            0.0,
        ));
        second.record_action(
            ActionSource::AgentDecision,
            ActionLifecycleStage::Rejected,
            Action::CaptureCreature {
                target: crate::id::EntityId(9),
                tool_slot: Some(0),
            },
            Some("too healthy".into()),
        );
        second.record_reward(AgentRewardSignal::new(
            1,
            RewardSource::ActionOutcome,
            RewardReason::ActionRejected,
            -0.1,
            false,
            None,
        ));
        second.record_reward(AgentRewardSignal::new(
            1,
            RewardSource::WorldEvent,
            RewardReason::DeathTaken,
            -5.0,
            true,
            None,
        ));

        let file = ReplayFile {
            header: ReplayHeader {
                name: "training".into(),
                timestamp: 0,
                world_seed: 1,
                tick_count: 2,
                agent_count: 1,
                notes: String::new(),
            },
            traces: vec![],
            telemetry_windows: vec![
                TickTelemetryFrame {
                    tick: 0,
                    agents: vec![first],
                },
                TickTelemetryFrame {
                    tick: 1,
                    agents: vec![second],
                },
            ],
        };

        let samples = file.training_samples();
        assert_eq!(samples.len(), 2);
        assert!((samples[0].path_distance - 5.0).abs() < f32::EPSILON);
        assert_eq!(samples[0].action_outcomes.executed, 1);
        assert_eq!(samples[0].tool_call_error_count, 1);
        assert!((samples[0].reward_summary.total - 1.05).abs() < f32::EPSILON);
        assert!(!samples[0].reward_summary.terminal);
        assert!(matches!(
            samples[0].encounter_transition,
            Some(EncounterTransition::Joined {
                encounter_id: 7,
                kind: EncounterKind::WildCreature
            })
        ));
        assert!(matches!(
            samples[1].encounter_transition,
            Some(EncounterTransition::CombatStateChanged {
                encounter_id: 7,
                in_combat: false
            })
        ));
        assert!((samples[1].reward_summary.total + 5.1).abs() < f32::EPSILON);
        assert!(samples[1].reward_summary.terminal);
    }

    #[test]
    fn replay_exports_roundtrip_through_toon_documents() {
        let agent_id = AgentId::new();
        let file = ReplayFile {
            header: ReplayHeader {
                name: "toon-replay".into(),
                timestamp: 0,
                world_seed: 7,
                tick_count: 1,
                agent_count: 1,
                notes: String::new(),
            },
            traces: vec![vec![DecisionTrace {
                tick: 0,
                agent_id,
                observation_hash: 42,
                prompt_sent: "observe".into(),
                raw_response: "act".into(),
                actions_taken: vec![Action::Idle],
                tool_calls: vec![AgentToolCallTrace::success(
                    0,
                    "llm.complete",
                    "qwen",
                    25,
                    90,
                    18,
                )],
                latency_ms: 25,
            }]],
            telemetry_windows: vec![TickTelemetryFrame {
                tick: 0,
                agents: vec![{
                    let mut frame = AgentTelemetryFrame::new(
                        0,
                        agent_id,
                        None,
                        AgentRuntimeProfile {
                            role: AgentRole::Player,
                            agent_type: crate::agent::AgentType::NeuralAgent,
                            capabilities: AgentCapabilities::player_default(),
                        },
                        0,
                        0,
                        0,
                        0,
                        0,
                        None,
                        None,
                    );
                    frame.record_reward(AgentRewardSignal::new(
                        0,
                        RewardSource::WorldEvent,
                        RewardReason::DamageDealt,
                        1.5,
                        false,
                        None,
                    ));
                    frame
                }],
            }],
        };

        let replay_document = file.to_toon_document();
        let replay_value =
            decode_toon_value(&replay_document).expect("replay document should decode");
        assert_eq!(replay_value["document_type"], "replay_file");
        assert_eq!(replay_value["payload"]["header"]["name"], "toon-replay");
        assert_eq!(replay_value["payload"]["telemetry_windows"][0]["tick"], 0);

        let training_document = file.training_samples_to_toon_document();
        let training_value =
            decode_toon_value(&training_document).expect("training document should decode");
        assert_eq!(training_value["document_type"], "replay_training_samples");
        assert_eq!(training_value["payload"][0]["reward_summary"]["total"], 1.5);
    }
}
