use std::collections::VecDeque;

use glam::Vec2;
use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::component::EncounterState;
use crate::contract::AgentRuntimeProfile;
use crate::id::{AgentId, EntityId};
use crate::toon::encode_toon_document;

/// Shared retention defaults for telemetry consumers across runtime, browser,
/// and editor tooling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryConfig {
    pub core_archive_ticks: usize,
    pub browser_overlay_samples: usize,
    pub editor_timeline_ticks: usize,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            core_archive_ticks: 600,
            browser_overlay_samples: 300,
            editor_timeline_ticks: 600,
        }
    }
}

/// Single trajectory sample for one agent at a specific simulation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrajectorySample {
    pub tick: u64,
    pub elapsed_secs: f32,
    pub position: Vec2,
    pub velocity: Vec2,
    pub rotation: f32,
}

impl TrajectorySample {
    pub fn new(
        tick: u64,
        elapsed_secs: f32,
        position: Vec2,
        velocity: Vec2,
        rotation: f32,
    ) -> Self {
        Self {
            tick,
            elapsed_secs,
            position,
            velocity,
            rotation,
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("trajectory_sample", self)
    }
}

/// Start/end trajectory state for one agent during a single authoritative tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentTrajectoryFrame {
    pub start: TrajectorySample,
    pub end: TrajectorySample,
    pub displacement: Vec2,
    pub distance_travelled: f32,
}

impl AgentTrajectoryFrame {
    pub fn new(start: TrajectorySample, end: TrajectorySample) -> Self {
        let displacement = end.position - start.position;
        Self {
            start,
            end,
            displacement,
            distance_travelled: displacement.length(),
        }
    }

    pub fn update_end(&mut self, end: TrajectorySample) {
        *self = Self::new(self.start, end);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionSource {
    AgentDecision,
    ExternalSubmission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionLifecycleStage {
    Submitted,
    Executed,
    Rejected,
    Queued,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RewardSource {
    ActionOutcome,
    WorldEvent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RewardReason {
    ActionExecuted,
    ActionRejected,
    ActionQueued,
    DamageDealt,
    DamageTaken,
    KillSecured,
    DeathTaken,
    SkillExperienceGained,
    CreatureCaptured,
    CompanionSummoned,
    CompanionCommandIssued,
    ResourceGathered,
    LootClaimed,
}

/// Authoritative reward signal attributed by the runtime from action outcomes
/// and emitted world events. This is the canonical source for replay-derived
/// training rewards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentRewardSignal {
    pub tick: u64,
    pub source: RewardSource,
    pub reason: RewardReason,
    pub value: f32,
    pub terminal: bool,
    pub tag: Option<String>,
}

impl AgentRewardSignal {
    pub fn new(
        tick: u64,
        source: RewardSource,
        reason: RewardReason,
        value: f32,
        terminal: bool,
        tag: Option<String>,
    ) -> Self {
        Self {
            tick,
            source,
            reason,
            value,
            terminal,
            tag,
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("agent_reward_signal", self)
    }
}

/// Action event recorded for parity/debug telemetry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentActionTrace {
    pub source: ActionSource,
    pub stage: ActionLifecycleStage,
    pub action: Action,
    pub rejection_reason: Option<String>,
}

impl AgentActionTrace {
    pub fn new(
        source: ActionSource,
        stage: ActionLifecycleStage,
        action: Action,
        rejection_reason: Option<String>,
    ) -> Self {
        Self {
            source,
            stage,
            action,
            rejection_reason,
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("agent_action_trace", self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolCallStatus {
    Requested,
    Succeeded,
    Failed,
    TimedOut,
    RateLimited,
    ParseError,
    ApiError,
    BudgetExceeded,
    Rejected,
}

/// Generic tool/LLM side-effect telemetry shared across embedded agent runtimes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentToolCallTrace {
    pub tick: u64,
    pub tool_name: String,
    pub provider: String,
    pub status: ToolCallStatus,
    pub latency_ms: u32,
    pub request_units: u32,
    pub response_units: u32,
    pub error_message: Option<String>,
}

impl AgentToolCallTrace {
    pub fn new(
        tick: u64,
        tool_name: impl Into<String>,
        provider: impl Into<String>,
        status: ToolCallStatus,
        latency_ms: u32,
        request_units: u32,
        response_units: u32,
        error_message: Option<String>,
    ) -> Self {
        Self {
            tick,
            tool_name: tool_name.into(),
            provider: provider.into(),
            status,
            latency_ms,
            request_units,
            response_units,
            error_message,
        }
    }

    pub fn success(
        tick: u64,
        tool_name: impl Into<String>,
        provider: impl Into<String>,
        latency_ms: u32,
        request_units: u32,
        response_units: u32,
    ) -> Self {
        Self::new(
            tick,
            tool_name,
            provider,
            ToolCallStatus::Succeeded,
            latency_ms,
            request_units,
            response_units,
            None,
        )
    }

    pub fn failure(
        tick: u64,
        tool_name: impl Into<String>,
        provider: impl Into<String>,
        status: ToolCallStatus,
        latency_ms: u32,
        error_message: impl Into<String>,
    ) -> Self {
        Self::new(
            tick,
            tool_name,
            provider,
            status,
            latency_ms,
            0,
            0,
            Some(error_message.into()),
        )
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("agent_tool_call_trace", self)
    }
}

/// Tool/provider side-effect event annotated with the authoritative entity that
/// triggered it. This is the TOON-facing payload used by debug consumers and
/// SpacetimeDB telemetry rows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentToolCallEvent {
    pub tick: u64,
    pub agent_entity_id: u64,
    pub trace: AgentToolCallTrace,
}

impl AgentToolCallEvent {
    pub fn new(agent_entity_id: u64, trace: AgentToolCallTrace) -> Self {
        Self {
            tick: trace.tick,
            agent_entity_id,
            trace,
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("agent_tool_call_event", self)
    }
}

/// Authoritative telemetry for one agent over one simulation tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTelemetryFrame {
    pub tick: u64,
    pub agent_id: AgentId,
    pub entity_id: Option<EntityId>,
    pub runtime_profile: AgentRuntimeProfile,
    pub visible_entity_count: usize,
    pub audible_event_count: usize,
    pub message_count: usize,
    pub available_action_count: usize,
    pub objective_count: usize,
    pub encounter: Option<EncounterState>,
    pub trajectory: Option<AgentTrajectoryFrame>,
    pub action_trace: Vec<AgentActionTrace>,
    pub tool_calls: Vec<AgentToolCallTrace>,
    #[serde(default)]
    pub reward_signals: Vec<AgentRewardSignal>,
}

impl AgentTelemetryFrame {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tick: u64,
        agent_id: AgentId,
        entity_id: Option<EntityId>,
        runtime_profile: AgentRuntimeProfile,
        visible_entity_count: usize,
        audible_event_count: usize,
        message_count: usize,
        available_action_count: usize,
        objective_count: usize,
        encounter: Option<EncounterState>,
        trajectory_start: Option<TrajectorySample>,
    ) -> Self {
        Self {
            tick,
            agent_id,
            entity_id,
            runtime_profile,
            visible_entity_count,
            audible_event_count,
            message_count,
            available_action_count,
            objective_count,
            encounter,
            trajectory: trajectory_start.map(|start| AgentTrajectoryFrame::new(start, start)),
            action_trace: Vec::new(),
            tool_calls: Vec::new(),
            reward_signals: Vec::new(),
        }
    }

    pub fn record_action(
        &mut self,
        source: ActionSource,
        stage: ActionLifecycleStage,
        action: Action,
        rejection_reason: Option<String>,
    ) {
        self.action_trace.push(AgentActionTrace::new(
            source,
            stage,
            action,
            rejection_reason,
        ));
    }

    pub fn record_tool_call(&mut self, trace: AgentToolCallTrace) {
        self.tool_calls.push(trace);
    }

    pub fn record_reward(&mut self, signal: AgentRewardSignal) {
        self.reward_signals.push(signal);
    }

    pub fn update_trajectory_end(&mut self, end: TrajectorySample) {
        if let Some(trajectory) = &mut self.trajectory {
            trajectory.update_end(end);
        } else {
            self.trajectory = Some(AgentTrajectoryFrame::new(end, end));
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("agent_telemetry_frame", self)
    }
}

/// Telemetry for the full authoritative tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickTelemetryFrame {
    pub tick: u64,
    pub agents: Vec<AgentTelemetryFrame>,
}

impl TickTelemetryFrame {
    pub fn empty(tick: u64) -> Self {
        Self {
            tick,
            agents: Vec::new(),
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("tick_telemetry_frame", self)
    }
}

/// Aggregate telemetry rollup for one authoritative agent over a retained tick
/// window. This mirrors the SpacetimeDB rollup row while keeping the detailed
/// document transport TOON-native for debug tooling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentTickRollup {
    pub tick_start: u64,
    pub tick_end: u64,
    pub agent_entity_id: u64,
    pub total_distance: f32,
    pub submitted_action_count: u32,
    pub executed_action_count: u32,
    pub rejected_action_count: u32,
    pub tool_call_count: u32,
    pub tool_error_count: u32,
    pub visible_entity_count: u32,
    pub audible_event_count: u32,
    pub message_count: u32,
    pub average_tool_latency_ms: f32,
}

impl AgentTickRollup {
    pub fn from_agent_frames(agent_entity_id: u64, frames: &[AgentTelemetryFrame]) -> Option<Self> {
        let tick_start = frames.first()?.tick;
        let tick_end = frames.last()?.tick;

        let total_distance = frames
            .iter()
            .map(|frame| {
                frame
                    .trajectory
                    .as_ref()
                    .map(|trajectory| trajectory.distance_travelled)
                    .unwrap_or_default()
            })
            .sum::<f32>();
        let submitted_action_count = frames
            .iter()
            .flat_map(|frame| frame.action_trace.iter())
            .filter(|trace| trace.stage == ActionLifecycleStage::Submitted)
            .count() as u32;
        let executed_action_count = frames
            .iter()
            .flat_map(|frame| frame.action_trace.iter())
            .filter(|trace| trace.stage == ActionLifecycleStage::Executed)
            .count() as u32;
        let rejected_action_count = frames
            .iter()
            .flat_map(|frame| frame.action_trace.iter())
            .filter(|trace| trace.stage == ActionLifecycleStage::Rejected)
            .count() as u32;
        let tool_call_count = frames
            .iter()
            .map(|frame| frame.tool_calls.len())
            .sum::<usize>() as u32;
        let tool_error_count = frames
            .iter()
            .flat_map(|frame| frame.tool_calls.iter())
            .filter(|trace| {
                !matches!(
                    trace.status,
                    ToolCallStatus::Requested | ToolCallStatus::Succeeded
                )
            })
            .count() as u32;
        let visible_entity_count = frames
            .iter()
            .map(|frame| frame.visible_entity_count as u32)
            .sum();
        let audible_event_count = frames
            .iter()
            .map(|frame| frame.audible_event_count as u32)
            .sum();
        let message_count = frames.iter().map(|frame| frame.message_count as u32).sum();
        let average_tool_latency_ms = if tool_call_count == 0 {
            0.0
        } else {
            frames
                .iter()
                .flat_map(|frame| frame.tool_calls.iter())
                .map(|trace| trace.latency_ms as f32)
                .sum::<f32>()
                / tool_call_count as f32
        };

        Some(Self {
            tick_start,
            tick_end,
            agent_entity_id,
            total_distance,
            submitted_action_count,
            executed_action_count,
            rejected_action_count,
            tool_call_count,
            tool_error_count,
            visible_entity_count,
            audible_event_count,
            message_count,
            average_tool_latency_ms,
        })
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("agent_tick_rollup", self)
    }
}

/// Ring buffer retaining recent authoritative telemetry for tooling/debugging.
#[derive(Debug, Clone)]
pub struct TelemetryArchive {
    frames: VecDeque<TickTelemetryFrame>,
    max_frames: usize,
}

impl Default for TelemetryArchive {
    fn default() -> Self {
        Self::with_capacity(TelemetryConfig::default().core_archive_ticks)
    }
}

impl TelemetryArchive {
    pub fn with_capacity(max_frames: usize) -> Self {
        Self {
            frames: VecDeque::with_capacity(max_frames.max(1)),
            max_frames,
        }
    }

    pub fn record_tick(&mut self, frame: TickTelemetryFrame) {
        if self.max_frames == 0 {
            return;
        }
        if self.frames.len() >= self.max_frames {
            self.frames.pop_front();
        }
        self.frames.push_back(frame);
    }

    pub fn frames(&self) -> &VecDeque<TickTelemetryFrame> {
        &self.frames
    }

    pub fn latest(&self) -> Option<&TickTelemetryFrame> {
        self.frames.back()
    }

    pub fn trajectory_for_agent(&self, agent_id: AgentId) -> Vec<TrajectorySample> {
        let mut samples = Vec::new();
        for frame in &self.frames {
            if let Some(agent_frame) = frame.agents.iter().find(|entry| entry.agent_id == agent_id)
            {
                if let Some(trajectory) = &agent_frame.trajectory {
                    if samples.is_empty() {
                        samples.push(trajectory.start);
                    }
                    samples.push(trajectory.end);
                }
            }
        }
        samples
    }

    pub fn to_toon_document(&self) -> String {
        #[derive(Serialize)]
        struct ArchiveExport<'a> {
            max_frames: usize,
            retained_frames: usize,
            latest_tick: Option<u64>,
            frames: &'a VecDeque<TickTelemetryFrame>,
        }

        encode_toon_document(
            "telemetry_archive",
            &ArchiveExport {
                max_frames: self.max_frames,
                retained_frames: self.frames.len(),
                latest_tick: self.latest().map(|frame| frame.tick),
                frames: &self.frames,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use crate::agent::AgentType;
    use crate::contract::{AgentCapabilities, AgentRole};

    use crate::toon::decode_toon_value;

    use super::{
        ActionLifecycleStage, AgentRewardSignal, AgentTelemetryFrame, AgentTickRollup,
        AgentToolCallEvent, AgentToolCallTrace, RewardReason, RewardSource, TelemetryArchive,
        TelemetryConfig, TickTelemetryFrame, ToolCallStatus, TrajectorySample,
    };

    #[test]
    fn telemetry_config_defaults_match_debug_surface_plan() {
        let config = TelemetryConfig::default();
        assert_eq!(config.core_archive_ticks, 600);
        assert_eq!(config.browser_overlay_samples, 300);
        assert_eq!(config.editor_timeline_ticks, 600);
    }

    #[test]
    fn trajectory_frame_tracks_distance() {
        let start = TrajectorySample::new(5, 0.5, Vec2::ZERO, Vec2::ZERO, 0.0);
        let end = TrajectorySample::new(5, 0.516, Vec2::new(3.0, 4.0), Vec2::X, 0.25);
        let frame = super::AgentTrajectoryFrame::new(start, end);

        assert_eq!(frame.displacement, Vec2::new(3.0, 4.0));
        assert!((frame.distance_travelled - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn telemetry_archive_reconstructs_agent_trajectory() {
        let agent_id = crate::id::AgentId::new();
        let runtime_profile = crate::contract::AgentRuntimeProfile {
            role: AgentRole::Player,
            agent_type: AgentType::Human,
            capabilities: AgentCapabilities::player_default(),
        };
        let start = TrajectorySample::new(0, 0.0, Vec2::ZERO, Vec2::ZERO, 0.0);
        let mid = TrajectorySample::new(0, 1.0 / 60.0, Vec2::new(1.0, 0.0), Vec2::X, 0.0);
        let end = TrajectorySample::new(1, 2.0 / 60.0, Vec2::new(2.0, 0.0), Vec2::X, 0.0);

        let mut archive = TelemetryArchive::with_capacity(4);
        let mut first = AgentTelemetryFrame::new(
            0,
            agent_id,
            Some(crate::id::EntityId(1)),
            runtime_profile,
            3,
            1,
            0,
            4,
            1,
            None,
            Some(start),
        );
        first.update_trajectory_end(mid);
        first.record_tool_call(AgentToolCallTrace::success(0, "observe", "demo", 4, 12, 20));
        first.record_reward(AgentRewardSignal::new(
            0,
            RewardSource::ActionOutcome,
            RewardReason::ActionExecuted,
            0.05,
            false,
            None,
        ));
        archive.record_tick(TickTelemetryFrame {
            tick: 0,
            agents: vec![first],
        });

        let mut second = AgentTelemetryFrame::new(
            1,
            agent_id,
            Some(crate::id::EntityId(1)),
            runtime_profile,
            2,
            0,
            1,
            3,
            1,
            None,
            Some(mid),
        );
        second.update_trajectory_end(end);
        second.record_tool_call(AgentToolCallTrace::failure(
            1,
            "act",
            "demo",
            ToolCallStatus::TimedOut,
            30,
            "timeout",
        ));
        archive.record_tick(TickTelemetryFrame {
            tick: 1,
            agents: vec![second],
        });

        let samples = archive.trajectory_for_agent(agent_id);
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0].position, Vec2::ZERO);
        assert_eq!(samples[1].position, Vec2::new(1.0, 0.0));
        assert_eq!(samples[2].position, Vec2::new(2.0, 0.0));
        assert_eq!(
            archive.latest().expect("latest frame").agents[0].tool_calls[0].status,
            ToolCallStatus::TimedOut
        );
        assert_eq!(
            archive.latest().expect("latest frame").agents[0]
                .reward_signals
                .len(),
            0
        );
    }

    #[test]
    fn telemetry_archive_respects_max_frame_capacity() {
        let mut archive = TelemetryArchive::with_capacity(2);
        archive.record_tick(TickTelemetryFrame::empty(1));
        archive.record_tick(TickTelemetryFrame::empty(2));
        archive.record_tick(TickTelemetryFrame::empty(3));

        let ticks = archive
            .frames()
            .iter()
            .map(|frame| frame.tick)
            .collect::<Vec<_>>();
        assert_eq!(ticks, vec![2, 3]);
    }

    #[test]
    fn telemetry_exports_roundtrip_through_toon_documents() {
        let mut archive = TelemetryArchive::with_capacity(2);
        archive.record_tick(TickTelemetryFrame::empty(7));

        let frame_document = archive.latest().expect("frame recorded").to_toon_document();
        let frame_value = decode_toon_value(&frame_document).expect("frame document should decode");
        assert_eq!(frame_value["document_type"], "tick_telemetry_frame");
        assert_eq!(frame_value["payload"]["tick"], 7);

        let archive_document = archive.to_toon_document();
        let archive_value =
            decode_toon_value(&archive_document).expect("archive document should decode");
        assert_eq!(archive_value["document_type"], "telemetry_archive");
        assert_eq!(archive_value["payload"]["retained_frames"], 1);
        assert_eq!(archive_value["payload"]["latest_tick"], 7);
    }

    #[test]
    fn tool_call_event_and_rollup_export_to_toon_documents() {
        let trace = AgentToolCallTrace::success(9, "llm.complete", "qwen", 21, 40, 18);
        let event = AgentToolCallEvent::new(144, trace.clone());
        let event_document = event.to_toon_document();
        let event_value =
            decode_toon_value(&event_document).expect("tool-call event should decode");
        assert_eq!(event_value["document_type"], "agent_tool_call_event");
        assert_eq!(event_value["payload"]["agent_entity_id"], 144);
        assert_eq!(event_value["payload"]["trace"]["tool_name"], "llm.complete");

        let runtime_profile = crate::contract::AgentRuntimeProfile {
            role: AgentRole::Player,
            agent_type: AgentType::Human,
            capabilities: AgentCapabilities::player_default(),
        };
        let mut frame = AgentTelemetryFrame::new(
            9,
            crate::id::AgentId::new(),
            Some(crate::id::EntityId(144)),
            runtime_profile,
            5,
            1,
            2,
            3,
            1,
            None,
            Some(TrajectorySample::new(9, 0.15, Vec2::ZERO, Vec2::X, 0.0)),
        );
        frame.update_trajectory_end(TrajectorySample::new(
            9,
            0.166,
            Vec2::new(3.0, 4.0),
            Vec2::new(1.0, 0.5),
            0.2,
        ));
        frame.record_action(
            crate::telemetry::ActionSource::ExternalSubmission,
            ActionLifecycleStage::Submitted,
            crate::action::Action::Idle,
            None,
        );
        frame.record_action(
            crate::telemetry::ActionSource::ExternalSubmission,
            ActionLifecycleStage::Executed,
            crate::action::Action::Idle,
            None,
        );
        frame.record_tool_call(trace);

        let rollup =
            AgentTickRollup::from_agent_frames(144, &[frame]).expect("rollup should be built");
        let rollup_document = rollup.to_toon_document();
        let rollup_value =
            decode_toon_value(&rollup_document).expect("rollup document should decode");
        assert_eq!(rollup_value["document_type"], "agent_tick_rollup");
        assert_eq!(rollup_value["payload"]["agent_entity_id"], 144);
        assert_eq!(rollup_value["payload"]["tool_call_count"], 1);
        assert_eq!(rollup_value["payload"]["visible_entity_count"], 5);
    }

    #[test]
    fn reward_signal_exports_to_toon_document() {
        let reward = AgentRewardSignal::new(
            12,
            RewardSource::WorldEvent,
            RewardReason::CreatureCaptured,
            4.0,
            false,
            Some("spark-mouse".into()),
        );
        let document = reward.to_toon_document();
        let value = decode_toon_value(&document).expect("reward document should decode");
        assert_eq!(value["document_type"], "agent_reward_signal");
        assert_eq!(value["payload"]["tick"], 12);
        assert_eq!(value["payload"]["tag"], "spark-mouse");
    }
}
