use serde::{Deserialize, Serialize};

use crate::toon::encode_toon_document;

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

#[cfg(test)]
mod tests {
    use crate::toon::decode_toon_value;

    use super::{FocusedEntityDebugSummary, IncidentSeverity, ShardIncidentSummary};

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
}
