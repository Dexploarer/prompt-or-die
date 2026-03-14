use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::tick::TickResult;
use crate::toon::encode_toon_document;
use crate::{ActionLifecycleStage, TelemetryArchive, ToolCallStatus};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncidentSeverity {
    #[default]
    Healthy,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShardIncidentSummary {
    pub shard_id: String,
    pub latest_tick: u64,
    pub severity: IncidentSeverity,
    pub summary: String,
    pub tick_budget_overrun_rate: f32,
    pub action_rejection_rate: f32,
    pub tool_call_error_rate: f32,
    pub average_tool_latency_ms: f32,
    pub average_trajectory_distance: f32,
    pub peak_entity_count: usize,
    pub peak_agent_count: usize,
    pub capture_actions: usize,
    pub summon_actions: usize,
    pub gather_actions: usize,
    pub loot_actions: usize,
    pub notes: Vec<String>,
}

impl ShardIncidentSummary {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("shard_incident_summary", self)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ShardGameplayIncidentTracker {
    ticks_completed: u64,
    total_actions: usize,
    total_actions_rejected: usize,
    peak_entity_count: usize,
    peak_agent_count: usize,
    tick_budget_overruns: u64,
    total_tool_calls: usize,
    total_tool_call_errors: usize,
    total_tool_latency_ms: u64,
    total_trajectory_distance: f32,
    total_agents_sampled: usize,
    capture_actions: usize,
    summon_actions: usize,
    gather_actions: usize,
    loot_actions: usize,
}

impl ShardGameplayIncidentTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_tick(
        &mut self,
        tick_result: &TickResult,
        agent_count: usize,
        tick_over_budget: bool,
    ) {
        self.ticks_completed += 1;
        self.total_actions += tick_result.actions_processed;
        self.total_actions_rejected += tick_result.actions_rejected;
        self.peak_entity_count = self.peak_entity_count.max(tick_result.entity_count);
        self.peak_agent_count = self.peak_agent_count.max(agent_count);
        if tick_over_budget {
            self.tick_budget_overruns += 1;
        }

        for agent in &tick_result.telemetry.agents {
            self.total_agents_sampled += 1;
            if let Some(trajectory) = &agent.trajectory {
                self.total_trajectory_distance += trajectory.distance_travelled;
            }

            for trace in &agent.tool_calls {
                self.total_tool_calls += 1;
                self.total_tool_latency_ms += trace.latency_ms as u64;
                if !matches!(
                    trace.status,
                    ToolCallStatus::Succeeded | ToolCallStatus::Requested
                ) {
                    self.total_tool_call_errors += 1;
                }
            }

            for trace in &agent.action_trace {
                if !matches!(trace.stage, ActionLifecycleStage::Executed) {
                    continue;
                }

                match &trace.action {
                    Action::CaptureCreature { .. } => self.capture_actions += 1,
                    Action::SummonCompanion { .. } => self.summon_actions += 1,
                    Action::GatherResource { .. } => self.gather_actions += 1,
                    Action::Loot { .. } => self.loot_actions += 1,
                    _ => {}
                }
            }
        }
    }

    pub fn action_rejection_rate(&self) -> f32 {
        let total = self.total_actions + self.total_actions_rejected;
        if total == 0 {
            return 0.0;
        }
        self.total_actions_rejected as f32 / total as f32
    }

    pub fn tool_call_error_rate(&self) -> f32 {
        if self.total_tool_calls == 0 {
            return 0.0;
        }
        self.total_tool_call_errors as f32 / self.total_tool_calls as f32
    }

    pub fn average_tool_latency_ms(&self) -> f32 {
        if self.total_tool_calls == 0 {
            return 0.0;
        }
        self.total_tool_latency_ms as f32 / self.total_tool_calls as f32
    }

    pub fn average_trajectory_distance(&self) -> f32 {
        if self.total_agents_sampled == 0 {
            return 0.0;
        }
        self.total_trajectory_distance / self.total_agents_sampled as f32
    }

    pub fn tick_budget_overrun_rate(&self) -> f32 {
        if self.ticks_completed == 0 {
            return 0.0;
        }
        self.tick_budget_overruns as f32 / self.ticks_completed as f32
    }

    pub fn incident_summary(
        &self,
        shard_id: impl Into<String>,
        latest_tick: u64,
    ) -> ShardIncidentSummary {
        let shard_id = shard_id.into();
        let tick_budget_overrun_rate = self.tick_budget_overrun_rate();
        let action_rejection_rate = self.action_rejection_rate();
        let tool_call_error_rate = self.tool_call_error_rate();
        let average_tool_latency_ms = self.average_tool_latency_ms();
        let average_trajectory_distance = self.average_trajectory_distance();

        let mut notes = Vec::new();
        if tick_budget_overrun_rate >= 0.05 {
            notes.push("tick budget overruns exceed 5%".to_string());
        }
        if action_rejection_rate >= 0.15 {
            notes.push("action rejection rate exceeds 15%".to_string());
        }
        if tool_call_error_rate >= 0.10 {
            notes.push("tool-call error rate exceeds 10%".to_string());
        }
        if average_tool_latency_ms >= 750.0 {
            notes.push("tool-call latency exceeds 750ms".to_string());
        }

        let sustained_critical = self.ticks_completed >= 10
            && (tick_budget_overrun_rate >= 0.10
                || action_rejection_rate >= 0.25
                || tool_call_error_rate >= 0.20);

        let severity = if sustained_critical {
            IncidentSeverity::Critical
        } else if !notes.is_empty() {
            IncidentSeverity::Warning
        } else {
            IncidentSeverity::Healthy
        };

        let summary = if notes.is_empty() {
            format!("Shard {shard_id} is healthy at tick {latest_tick}")
        } else {
            format!("Shard {shard_id} requires attention: {}", notes.join("; "))
        };

        ShardIncidentSummary {
            shard_id,
            latest_tick,
            severity,
            summary,
            tick_budget_overrun_rate,
            action_rejection_rate,
            tool_call_error_rate,
            average_tool_latency_ms,
            average_trajectory_distance,
            peak_entity_count: self.peak_entity_count,
            peak_agent_count: self.peak_agent_count,
            capture_actions: self.capture_actions,
            summon_actions: self.summon_actions,
            gather_actions: self.gather_actions,
            loot_actions: self.loot_actions,
            notes,
        }
    }

    pub fn ticks_completed(&self) -> u64 {
        self.ticks_completed
    }

    pub fn total_actions(&self) -> usize {
        self.total_actions
    }

    pub fn total_actions_rejected(&self) -> usize {
        self.total_actions_rejected
    }

    pub fn peak_entity_count(&self) -> usize {
        self.peak_entity_count
    }

    pub fn peak_agent_count(&self) -> usize {
        self.peak_agent_count
    }

    pub fn tick_budget_overruns(&self) -> u64 {
        self.tick_budget_overruns
    }

    pub fn total_tool_calls(&self) -> usize {
        self.total_tool_calls
    }

    pub fn total_tool_call_errors(&self) -> usize {
        self.total_tool_call_errors
    }

    pub fn capture_actions(&self) -> usize {
        self.capture_actions
    }

    pub fn summon_actions(&self) -> usize {
        self.summon_actions
    }

    pub fn gather_actions(&self) -> usize {
        self.gather_actions
    }

    pub fn loot_actions(&self) -> usize {
        self.loot_actions
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FocusedEntityDebugSummary {
    pub shard_id: String,
    pub entity_id: u64,
    pub latest_tick: u64,
    pub tool_call_count: usize,
    pub tool_error_count: usize,
    pub rejected_action_count: usize,
    pub total_distance: f32,
    pub average_tool_latency_ms: f32,
    pub visible_entity_count: usize,
    pub audible_event_count: usize,
    pub message_count: usize,
    pub latest_tool_name: Option<String>,
    pub latest_tool_status: Option<String>,
    pub latest_tool_error: Option<String>,
    pub notes: Vec<String>,
}

impl FocusedEntityDebugSummary {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("focused_entity_debug_summary", self)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientTransportSummary {
    pub client_id: String,
    pub player_name: Option<String>,
    pub controlled_entity: Option<u64>,
    pub session_resumes: u64,
    pub recovery_snapshots_sent: u64,
    pub recovery_delivery_failures: u64,
    pub last_seen_tick: u64,
    pub ticks_since_last_seen: u64,
    pub last_sent_tick: Option<u64>,
    pub pending_action_queue_depth: usize,
    pub peak_pending_action_queue_depth: usize,
    pub queue_pressure: bool,
    pub queue_pressure_events: u64,
    pub inbound_messages: u64,
    pub outbound_messages: u64,
    pub inbound_bytes: u64,
    pub outbound_bytes: u64,
    pub action_batches_received: u64,
    pub full_snapshots_sent: u64,
    pub full_snapshot_bytes: u64,
    pub max_full_snapshot_bytes: u64,
    pub recovery_snapshot_bytes_sent: u64,
    pub full_snapshot_requests: u64,
    pub ping_requests: u64,
    pub state_deltas_sent: u64,
    pub delta_messages_sent: u64,
    pub delta_bytes_sent: u64,
    pub max_delta_bytes: u64,
    pub delta_entities_updated: u64,
    pub delta_entities_destroyed: u64,
    pub event_batches_sent: u64,
    pub debug_documents_sent: u64,
    pub rejected_messages_sent: u64,
    pub debug_telemetry_enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShardTransportSummary {
    pub shard_id: String,
    pub latest_tick: u64,
    pub client_count: usize,
    pub resumed_sessions: u64,
    pub recovery_snapshots_sent: u64,
    pub recovery_delivery_failures: u64,
    pub client_inactivity_timeout_ticks: u64,
    pub queue_pressure_warn_depth: usize,
    pub total_pending_action_queue_depth: usize,
    pub peak_pending_action_queue_depth: usize,
    pub queue_pressure_client_count: usize,
    pub total_inbound_messages: u64,
    pub total_outbound_messages: u64,
    pub total_inbound_bytes: u64,
    pub total_outbound_bytes: u64,
    pub action_batches_received: u64,
    pub full_snapshots_sent: u64,
    pub total_full_snapshot_bytes: u64,
    pub max_full_snapshot_bytes: u64,
    pub total_recovery_snapshot_bytes: u64,
    pub full_snapshot_requests: u64,
    pub ping_requests: u64,
    pub state_deltas_sent: u64,
    pub delta_messages_sent: u64,
    pub total_delta_bytes: u64,
    pub max_delta_bytes: u64,
    pub total_delta_entities_updated: u64,
    pub total_delta_entities_destroyed: u64,
    pub event_batches_sent: u64,
    pub debug_documents_sent: u64,
    pub rejected_messages_sent: u64,
    pub timed_out_clients: u64,
    pub queue_pressure_events: u64,
    pub clients: Vec<ClientTransportSummary>,
}

impl ShardTransportSummary {
    pub fn to_toon_document(&self) -> String {
        encode_toon_document("shard_transport_summary", self)
    }
}

pub fn summarize_focused_entity_debug(
    shard_id: impl Into<String>,
    archive: &TelemetryArchive,
    entity_id: u64,
) -> Option<FocusedEntityDebugSummary> {
    let mut latest_tick = 0;
    let mut tool_call_count = 0usize;
    let mut tool_error_count = 0usize;
    let mut total_tool_latency_ms = 0u64;
    let mut rejected_action_count = 0usize;
    let mut total_distance = 0.0f32;
    let mut visible_entity_count = 0usize;
    let mut audible_event_count = 0usize;
    let mut message_count = 0usize;
    let mut latest_tool_name = None;
    let mut latest_tool_status = None;
    let mut latest_tool_error = None;

    for frame in archive.frames() {
        for agent in &frame.agents {
            let Some(agent_entity_id) = agent.entity_id else {
                continue;
            };
            if agent_entity_id.0 != entity_id {
                continue;
            }

            latest_tick = latest_tick.max(frame.tick);
            visible_entity_count = agent.visible_entity_count;
            audible_event_count = agent.audible_event_count;
            message_count = agent.message_count;
            if let Some(trajectory) = &agent.trajectory {
                total_distance += trajectory.distance_travelled;
            }
            rejected_action_count += agent
                .action_trace
                .iter()
                .filter(|trace| matches!(trace.stage, ActionLifecycleStage::Rejected))
                .count();
            for trace in &agent.tool_calls {
                tool_call_count += 1;
                total_tool_latency_ms += trace.latency_ms as u64;
                latest_tool_name = Some(trace.tool_name.clone());
                latest_tool_status = Some(format!("{:?}", trace.status));
                latest_tool_error = trace.error_message.clone();
                if !matches!(
                    trace.status,
                    ToolCallStatus::Succeeded | ToolCallStatus::Requested
                ) {
                    tool_error_count += 1;
                }
            }
        }
    }

    if latest_tick == 0 && tool_call_count == 0 && rejected_action_count == 0 {
        return None;
    }

    let average_tool_latency_ms = if tool_call_count == 0 {
        0.0
    } else {
        total_tool_latency_ms as f32 / tool_call_count as f32
    };

    let mut notes = Vec::new();
    if tool_error_count > 0 {
        notes.push(format!("{tool_error_count} tool-call errors retained"));
    }
    if rejected_action_count > 0 {
        notes.push(format!("{rejected_action_count} rejected actions retained"));
    }

    Some(FocusedEntityDebugSummary {
        shard_id: shard_id.into(),
        entity_id,
        latest_tick,
        tool_call_count,
        tool_error_count,
        rejected_action_count,
        total_distance,
        average_tool_latency_ms,
        visible_entity_count,
        audible_event_count,
        message_count,
        latest_tool_name,
        latest_tool_status,
        latest_tool_error,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use crate::telemetry::{AgentTelemetryFrame, TickTelemetryFrame, TrajectorySample};
    use crate::tick::TickResult;
    use crate::toon::decode_toon_value;
    use crate::{AgentId, AgentRuntimeProfile, EntityId, TelemetryArchive, ToolCallStatus};

    use super::{
        summarize_focused_entity_debug, ClientTransportSummary, FocusedEntityDebugSummary,
        IncidentSeverity, ShardGameplayIncidentTracker, ShardIncidentSummary,
        ShardTransportSummary,
    };

    #[test]
    fn shard_incident_summary_exports_to_toon() {
        let summary = ShardIncidentSummary {
            shard_id: "verdant-hollow".to_string(),
            latest_tick: 1440,
            severity: IncidentSeverity::Warning,
            summary: "tick budget drift".to_string(),
            tick_budget_overrun_rate: 0.12,
            action_rejection_rate: 0.03,
            tool_call_error_rate: 0.01,
            average_tool_latency_ms: 88.0,
            average_trajectory_distance: 4.5,
            peak_entity_count: 96,
            peak_agent_count: 24,
            capture_actions: 2,
            summon_actions: 1,
            gather_actions: 7,
            loot_actions: 5,
            notes: vec!["late spike".to_string()],
        };

        let value =
            decode_toon_value(&summary.to_toon_document()).expect("incident summary should decode");
        assert_eq!(value["document_type"], "shard_incident_summary");
        assert_eq!(value["payload"]["shard_id"], "verdant-hollow");
        assert_eq!(value["payload"]["severity"], "Warning");
    }

    #[test]
    fn shard_gameplay_incident_tracker_builds_shared_summary() {
        let mut tracker = ShardGameplayIncidentTracker::new();
        let tick_result = TickResult {
            tick: 9,
            events: Vec::new(),
            entity_count: 24,
            actions_processed: 8,
            actions_rejected: 2,
            telemetry: TickTelemetryFrame {
                tick: 9,
                agents: Vec::new(),
            },
        };

        tracker.record_tick(&tick_result, 5, true);
        let summary = tracker.incident_summary("alpha-1", tick_result.tick);

        assert_eq!(summary.shard_id, "alpha-1");
        assert_eq!(summary.latest_tick, 9);
        assert_eq!(summary.severity, IncidentSeverity::Warning);
        assert!(summary
            .notes
            .iter()
            .any(|note| note.contains("tick budget overruns exceed 5%")));
        assert_eq!(summary.peak_entity_count, 24);
        assert_eq!(summary.peak_agent_count, 5);
        assert!(summary.tick_budget_overrun_rate > 0.0);
    }

    #[test]
    fn focused_entity_debug_summary_exports_to_toon() {
        let summary = FocusedEntityDebugSummary {
            shard_id: "verdant-hollow".to_string(),
            entity_id: 41,
            latest_tick: 1440,
            tool_call_count: 3,
            tool_error_count: 1,
            rejected_action_count: 2,
            total_distance: 18.5,
            average_tool_latency_ms: 42.0,
            visible_entity_count: 9,
            audible_event_count: 2,
            message_count: 1,
            latest_tool_name: Some("llm.complete".to_string()),
            latest_tool_status: Some("Succeeded".to_string()),
            latest_tool_error: None,
            notes: vec!["2 rejected actions retained".to_string()],
        };

        let value = decode_toon_value(&summary.to_toon_document())
            .expect("focused entity summary should decode");
        assert_eq!(value["document_type"], "focused_entity_debug_summary");
        assert_eq!(value["payload"]["entity_id"], 41);
        assert_eq!(value["payload"]["tool_call_count"], 3);
        assert_eq!(value["payload"]["latest_tool_name"], "llm.complete");
    }

    #[test]
    fn focused_entity_debug_summary_aggregates_from_archive() {
        let mut archive = TelemetryArchive::with_capacity(16);
        let start = TrajectorySample::new(12, 12.0 / 60.0, glam::Vec2::ZERO, glam::Vec2::ZERO, 0.0);
        let end = TrajectorySample::new(
            12,
            13.0 / 60.0,
            glam::Vec2::new(2.0, 1.0),
            glam::Vec2::new(1.0, 0.0),
            0.0,
        );
        let mut agent = AgentTelemetryFrame::new(
            12,
            AgentId::new(),
            Some(EntityId(41)),
            AgentRuntimeProfile::default(),
            7,
            2,
            1,
            4,
            1,
            None,
            Some(start),
        );
        agent.update_trajectory_end(end);
        agent.record_tool_call(crate::AgentToolCallTrace::failure(
            12,
            "llm.complete",
            "qwen",
            ToolCallStatus::TimedOut,
            80,
            "timeout",
        ));
        archive.record_tick(TickTelemetryFrame {
            tick: 12,
            agents: vec![agent],
        });

        let summary = summarize_focused_entity_debug("alpha", &archive, 41)
            .expect("focused summary should aggregate");
        assert_eq!(summary.shard_id, "alpha");
        assert_eq!(summary.entity_id, 41);
        assert_eq!(summary.tool_call_count, 1);
        assert_eq!(summary.tool_error_count, 1);
        assert_eq!(summary.visible_entity_count, 7);
        assert_eq!(summary.latest_tool_name.as_deref(), Some("llm.complete"));
    }

    #[test]
    fn shard_transport_summary_exports_to_toon() {
        let summary = ShardTransportSummary {
            shard_id: "direct-connect".to_string(),
            latest_tick: 1440,
            client_count: 2,
            resumed_sessions: 1,
            recovery_snapshots_sent: 3,
            recovery_delivery_failures: 1,
            client_inactivity_timeout_ticks: 600,
            queue_pressure_warn_depth: 192,
            total_pending_action_queue_depth: 3,
            peak_pending_action_queue_depth: 5,
            queue_pressure_client_count: 1,
            total_inbound_messages: 21,
            total_outbound_messages: 44,
            total_inbound_bytes: 1024,
            total_outbound_bytes: 4096,
            action_batches_received: 8,
            full_snapshots_sent: 4,
            total_full_snapshot_bytes: 8192,
            max_full_snapshot_bytes: 3072,
            total_recovery_snapshot_bytes: 4096,
            full_snapshot_requests: 1,
            ping_requests: 5,
            state_deltas_sent: 19,
            delta_messages_sent: 15,
            total_delta_bytes: 2048,
            max_delta_bytes: 512,
            total_delta_entities_updated: 32,
            total_delta_entities_destroyed: 6,
            event_batches_sent: 7,
            debug_documents_sent: 11,
            rejected_messages_sent: 2,
            timed_out_clients: 3,
            queue_pressure_events: 5,
            clients: vec![ClientTransportSummary {
                client_id: "client-a".to_string(),
                player_name: Some("debug".to_string()),
                controlled_entity: Some(41),
                session_resumes: 1,
                recovery_snapshots_sent: 2,
                recovery_delivery_failures: 1,
                last_seen_tick: 1440,
                ticks_since_last_seen: 0,
                last_sent_tick: Some(1440),
                pending_action_queue_depth: 1,
                peak_pending_action_queue_depth: 4,
                queue_pressure: true,
                queue_pressure_events: 2,
                inbound_messages: 10,
                outbound_messages: 20,
                inbound_bytes: 512,
                outbound_bytes: 2048,
                action_batches_received: 4,
                full_snapshots_sent: 2,
                full_snapshot_bytes: 4096,
                max_full_snapshot_bytes: 3072,
                recovery_snapshot_bytes_sent: 2048,
                full_snapshot_requests: 1,
                ping_requests: 3,
                state_deltas_sent: 9,
                delta_messages_sent: 7,
                delta_bytes_sent: 1024,
                max_delta_bytes: 320,
                delta_entities_updated: 18,
                delta_entities_destroyed: 3,
                event_batches_sent: 4,
                debug_documents_sent: 6,
                rejected_messages_sent: 1,
                debug_telemetry_enabled: true,
            }],
        };

        let value = decode_toon_value(&summary.to_toon_document())
            .expect("transport summary should decode");
        assert_eq!(value["document_type"], "shard_transport_summary");
        assert_eq!(value["payload"]["client_count"], 2);
        assert_eq!(value["payload"]["resumed_sessions"], 1);
        assert_eq!(value["payload"]["recovery_snapshots_sent"], 3);
        assert_eq!(value["payload"]["recovery_delivery_failures"], 1);
        assert_eq!(value["payload"]["peak_pending_action_queue_depth"], 5);
        assert_eq!(value["payload"]["full_snapshots_sent"], 4);
        assert_eq!(value["payload"]["total_full_snapshot_bytes"], 8192);
        assert_eq!(value["payload"]["max_full_snapshot_bytes"], 3072);
        assert_eq!(value["payload"]["total_recovery_snapshot_bytes"], 4096);
        assert_eq!(value["payload"]["delta_messages_sent"], 15);
        assert_eq!(value["payload"]["total_delta_bytes"], 2048);
        assert_eq!(value["payload"]["max_delta_bytes"], 512);
        assert_eq!(value["payload"]["total_delta_entities_updated"], 32);
        assert_eq!(value["payload"]["total_delta_entities_destroyed"], 6);
        assert_eq!(value["payload"]["queue_pressure_client_count"], 1);
        assert_eq!(value["payload"]["timed_out_clients"], 3);
        assert_eq!(value["payload"]["clients"][0]["client_id"], "client-a");
        assert_eq!(value["payload"]["clients"][0]["session_resumes"], 1);
        assert_eq!(value["payload"]["clients"][0]["recovery_snapshots_sent"], 2);
        assert_eq!(
            value["payload"]["clients"][0]["recovery_delivery_failures"],
            1
        );
        assert_eq!(
            value["payload"]["clients"][0]["peak_pending_action_queue_depth"],
            4
        );
        assert_eq!(value["payload"]["clients"][0]["queue_pressure_events"], 2);
        assert_eq!(value["payload"]["clients"][0]["full_snapshots_sent"], 2);
        assert_eq!(value["payload"]["clients"][0]["full_snapshot_bytes"], 4096);
        assert_eq!(
            value["payload"]["clients"][0]["recovery_snapshot_bytes_sent"],
            2048
        );
        assert_eq!(value["payload"]["clients"][0]["delta_messages_sent"], 7);
        assert_eq!(value["payload"]["clients"][0]["delta_bytes_sent"], 1024);
        assert_eq!(value["payload"]["clients"][0]["delta_entities_updated"], 18);
        assert_eq!(
            value["payload"]["clients"][0]["delta_entities_destroyed"],
            3
        );
        assert_eq!(value["payload"]["clients"][0]["queue_pressure"], true);
        assert_eq!(
            value["payload"]["clients"][0]["debug_telemetry_enabled"],
            true
        );
    }
}
