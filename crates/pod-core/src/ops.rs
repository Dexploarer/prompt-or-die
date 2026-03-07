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
