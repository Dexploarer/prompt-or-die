//! # pod-editor — editor crate scaffold
//!
//! Contains a lightweight dockable editor shell for POD phase implementation:
//! panel docking, hierarchy browsing, and lightweight inspector/property editing.

#![allow(clippy::bind_instead_of_map)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::derivable_impls)]
#![allow(clippy::io_other_error)]
#![allow(clippy::question_mark)]

use eframe::{App, Frame};
use egui::{CentralPanel, Context, ProgressBar, SidePanel, TopBottomPanel, Ui};
use pod_core::{
    decode_toon_document, decode_toon_value, encode_toon_document, Action, ActionLifecycleStage,
    AgentTelemetryFrame, AgentTickRollup, AgentToolCallEvent, AgentType, CreatureIdentity,
    CreatureTemperament, EncounterSpawnEntry, FactionReputationTier, FactionReputationTrack,
    FocusedEntityDebugSummary, PopulationBreakdown, QuestStageDefinition, QuestStateGraph,
    RegionEncounterTable, ReplayFile, ShardIncidentSummary, ShardTransportSummary,
    TelemetryConfig, TickTelemetryFrame, ToolCallStatus, TrajectorySample,
    VersionedTickTelemetry, WorldChunkDefinition, WorldPopulationState, WorldRegionDefinition,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EditorPanel {
    /// Main scene view and simulation viewport.
    Viewport,
    /// Entity hierarchy panel.
    Hierarchy,
    /// Component/properties editor for the selected entity.
    Inspector,
    /// Runtime logs and system messages.
    Console,
    /// Asset library and scene import surface.
    AssetBrowser,
    // Editor-focused workflows for advanced game-logic editing.
    BehaviorTree,
    FiniteStateMachine,
    LlmAgentConfig,
    Telemetry,
    SpacetimeDashboard,
}

impl Default for EditorPanel {
    fn default() -> Self {
        Self::Viewport
    }
}

impl EditorPanel {
    fn label(self) -> &'static str {
        match self {
            Self::Viewport => "Viewport",
            Self::Hierarchy => "Hierarchy",
            Self::Inspector => "Inspector",
            Self::Console => "Console",
            Self::AssetBrowser => "Asset Browser",
            Self::BehaviorTree => "Behavior Tree",
            Self::FiniteStateMachine => "FSM",
            Self::LlmAgentConfig => "LLM Agent",
            Self::Telemetry => "Telemetry",
            Self::SpacetimeDashboard => "SpacetimeDB",
        }
    }

    fn supports_dock(self) -> bool {
        !matches!(self, Self::Viewport)
    }

    fn default_dock_region(self) -> DockRegion {
        match self {
            Self::Hierarchy => DockRegion::Left,
            Self::Inspector => DockRegion::Right,
            Self::Console => DockRegion::Bottom,
            Self::AssetBrowser => DockRegion::Right,
            Self::BehaviorTree => DockRegion::Left,
            Self::FiniteStateMachine => DockRegion::Left,
            Self::LlmAgentConfig => DockRegion::Right,
            Self::Telemetry => DockRegion::Right,
            Self::SpacetimeDashboard => DockRegion::Right,
            Self::Viewport => DockRegion::Center,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ViewportMode {
    TwoD,
    ThreeD,
}

impl Default for ViewportMode {
    fn default() -> Self {
        Self::TwoD
    }
}

impl ViewportMode {
    fn label(self) -> &'static str {
        match self {
            Self::TwoD => "2D",
            Self::ThreeD => "3D",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Viewport3DState {
    pub camera_yaw: f32,
    pub camera_pitch: f32,
    pub zoom: f32,
}

impl Default for Viewport3DState {
    fn default() -> Self {
        Self {
            camera_yaw: 45.0,
            camera_pitch: 20.0,
            zoom: 10.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DockRegion {
    /// Left side of the editor.
    Left,
    /// Right side of the editor.
    Right,
    /// Bottom of the editor.
    Bottom,
    /// Main center viewport.
    Center,
    /// Not visible.
    Floating,
}

impl Default for DockRegion {
    fn default() -> Self {
        Self::Floating
    }
}

impl DockRegion {
    fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Bottom => "Bottom",
            Self::Center => "Center",
            Self::Floating => "Floating",
        }
    }

    fn is_visible(self) -> bool {
        !matches!(self, Self::Floating)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DockLayout {
    pub hierarchy: DockRegion,
    pub inspector: DockRegion,
    pub console: DockRegion,
    pub asset_browser: DockRegion,
    pub behavior_tree: DockRegion,
    pub finite_state_machine: DockRegion,
    pub llm_agent_config: DockRegion,
    pub telemetry: DockRegion,
    pub spacetime_dashboard: DockRegion,
    pub left_width: f32,
    pub right_width: f32,
    pub bottom_height: f32,
}

impl Default for DockLayout {
    fn default() -> Self {
        Self {
            hierarchy: EditorPanel::Hierarchy.default_dock_region(),
            inspector: EditorPanel::Inspector.default_dock_region(),
            console: EditorPanel::Console.default_dock_region(),
            asset_browser: EditorPanel::AssetBrowser.default_dock_region(),
            behavior_tree: EditorPanel::BehaviorTree.default_dock_region(),
            finite_state_machine: EditorPanel::FiniteStateMachine.default_dock_region(),
            llm_agent_config: EditorPanel::LlmAgentConfig.default_dock_region(),
            telemetry: EditorPanel::Telemetry.default_dock_region(),
            spacetime_dashboard: EditorPanel::SpacetimeDashboard.default_dock_region(),
            left_width: 230.0,
            right_width: 250.0,
            bottom_height: 180.0,
        }
    }
}

impl DockLayout {
    pub fn region_for_panel(&self, panel: EditorPanel) -> DockRegion {
        match panel {
            EditorPanel::Viewport => DockRegion::Center,
            EditorPanel::Hierarchy => self.hierarchy,
            EditorPanel::Inspector => self.inspector,
            EditorPanel::Console => self.console,
            EditorPanel::AssetBrowser => self.asset_browser,
            EditorPanel::BehaviorTree => self.behavior_tree,
            EditorPanel::FiniteStateMachine => self.finite_state_machine,
            EditorPanel::LlmAgentConfig => self.llm_agent_config,
            EditorPanel::Telemetry => self.telemetry,
            EditorPanel::SpacetimeDashboard => self.spacetime_dashboard,
        }
    }

    pub fn set_region(&mut self, panel: EditorPanel, region: DockRegion) {
        match panel {
            EditorPanel::Viewport => {}
            EditorPanel::Hierarchy => self.hierarchy = region,
            EditorPanel::Inspector => self.inspector = region,
            EditorPanel::Console => self.console = region,
            EditorPanel::AssetBrowser => self.asset_browser = region,
            EditorPanel::BehaviorTree => self.behavior_tree = region,
            EditorPanel::FiniteStateMachine => self.finite_state_machine = region,
            EditorPanel::LlmAgentConfig => self.llm_agent_config = region,
            EditorPanel::Telemetry => self.telemetry = region,
            EditorPanel::SpacetimeDashboard => self.spacetime_dashboard = region,
        }
    }

    pub fn is_visible(&self, panel: EditorPanel) -> bool {
        self.region_for_panel(panel).is_visible()
    }

    pub fn toggle_visibility(&mut self, panel: EditorPanel) {
        if !panel.supports_dock() {
            return;
        }
        let current = self.region_for_panel(panel);
        let next = if current.is_visible() {
            DockRegion::Floating
        } else {
            panel.default_dock_region()
        };
        self.set_region(panel, next);
    }

    pub fn set_left_width(&mut self, width: f32) {
        self.left_width = width.clamp(140.0, 420.0);
    }

    pub fn set_right_width(&mut self, width: f32) {
        self.right_width = width.clamp(140.0, 420.0);
    }

    pub fn set_bottom_height(&mut self, height: f32) {
        self.bottom_height = height.clamp(90.0, 300.0);
    }

    pub fn panels_for() -> [EditorPanel; 9] {
        [
            EditorPanel::Hierarchy,
            EditorPanel::Inspector,
            EditorPanel::Console,
            EditorPanel::AssetBrowser,
            EditorPanel::BehaviorTree,
            EditorPanel::FiniteStateMachine,
            EditorPanel::LlmAgentConfig,
            EditorPanel::Telemetry,
            EditorPanel::SpacetimeDashboard,
        ]
    }

    pub fn panels_in(&self, region: DockRegion) -> Vec<EditorPanel> {
        Self::panels_for()
            .into_iter()
            .filter(|panel| self.region_for_panel(*panel) == region)
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BehaviorTreeNodeKind {
    Sequence,
    Selector,
    Action,
    Condition,
}

impl BehaviorTreeNodeKind {
    fn label(self) -> &'static str {
        match self {
            Self::Sequence => "Sequence",
            Self::Selector => "Selector",
            Self::Action => "Action",
            Self::Condition => "Condition",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BehaviorTreeNode {
    pub id: u32,
    pub name: String,
    pub kind: BehaviorTreeNodeKind,
    pub children: Vec<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BehaviorTree {
    pub nodes: HashMap<u32, BehaviorTreeNode>,
    pub roots: Vec<u32>,
    pub selected_node: Option<u32>,
    pub next_node_id: u32,
}

impl Default for BehaviorTree {
    fn default() -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(
            1,
            BehaviorTreeNode {
                id: 1,
                name: "Root".to_string(),
                kind: BehaviorTreeNodeKind::Sequence,
                children: vec![2, 3],
            },
        );
        nodes.insert(
            2,
            BehaviorTreeNode {
                id: 2,
                name: "CheckTarget".to_string(),
                kind: BehaviorTreeNodeKind::Condition,
                children: vec![],
            },
        );
        nodes.insert(
            3,
            BehaviorTreeNode {
                id: 3,
                name: "Chase".to_string(),
                kind: BehaviorTreeNodeKind::Action,
                children: vec![],
            },
        );
        Self {
            nodes,
            roots: vec![1],
            selected_node: Some(1),
            next_node_id: 4,
        }
    }
}

impl BehaviorTree {
    pub fn add_node(&mut self, parent: Option<u32>, name: &str, kind: BehaviorTreeNodeKind) -> u32 {
        let id = self.next_node_id;
        self.next_node_id += 1;
        let node = BehaviorTreeNode {
            id,
            name: name.to_string(),
            kind,
            children: Vec::new(),
        };
        self.nodes.insert(id, node);
        match parent {
            Some(parent_id) => {
                if let Some(parent_node) = self.nodes.get_mut(&parent_id) {
                    parent_node.children.push(id);
                } else {
                    self.roots.push(id);
                }
            }
            None => self.roots.push(id),
        }
        self.selected_node = Some(id);
        id
    }

    pub fn remove_node(&mut self, node_id: u32) {
        let mut to_remove = Vec::new();
        self.collect_descendants(node_id, &mut to_remove);
        for remove_id in to_remove {
            self.nodes.remove(&remove_id);
            for node in self.nodes.values_mut() {
                node.children.retain(|child| *child != remove_id);
            }
            self.roots.retain(|root| *root != remove_id);
        }
        if self.selected_node == Some(node_id) {
            self.selected_node = self.roots.first().copied();
        }
        if self.nodes.is_empty() {
            *self = Self::default();
        }
    }

    fn collect_descendants(&self, node_id: u32, collected: &mut Vec<u32>) {
        if !self.nodes.contains_key(&node_id) {
            return;
        }
        collected.push(node_id);
        if let Some(node) = self.nodes.get(&node_id) {
            for child in &node.children {
                self.collect_descendants(*child, collected);
            }
        }
    }

    pub fn set_node_kind(&mut self, node_id: u32, kind: BehaviorTreeNodeKind) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.kind = kind;
        }
    }

    pub fn set_node_name(&mut self, node_id: u32, name: String) {
        if let Some(node) = self.nodes.get_mut(&node_id) {
            node.name = name;
        }
    }

    pub fn set_selected_node(&mut self, node_id: Option<u32>) {
        self.selected_node = if node_id.is_some() { node_id } else { None };
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FsmTransition {
    pub from: String,
    pub to: String,
    pub on: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FiniteStateMachine {
    pub states: Vec<String>,
    pub transitions: Vec<FsmTransition>,
    pub selected_state: Option<String>,
    pub selected_transition: Option<usize>,
}

impl Default for FiniteStateMachine {
    fn default() -> Self {
        Self {
            states: vec![
                "idle".to_string(),
                "alert".to_string(),
                "combat".to_string(),
                "dead".to_string(),
            ],
            transitions: vec![
                FsmTransition {
                    from: "idle".to_string(),
                    to: "alert".to_string(),
                    on: "enemy_seen".to_string(),
                },
                FsmTransition {
                    from: "alert".to_string(),
                    to: "combat".to_string(),
                    on: "in_combat".to_string(),
                },
                FsmTransition {
                    from: "combat".to_string(),
                    to: "dead".to_string(),
                    on: "health_zero".to_string(),
                },
            ],
            selected_state: Some("idle".to_string()),
            selected_transition: None,
        }
    }
}

impl FiniteStateMachine {
    pub fn ensure_state(&mut self, state: &str) -> bool {
        if self.states.iter().any(|value| value == state) {
            return false;
        }
        self.states.push(state.to_string());
        true
    }

    pub fn add_state(&mut self, state: &str) -> bool {
        self.ensure_state(state)
    }

    pub fn remove_state(&mut self, state: &str) {
        self.states.retain(|value| value != state);
        self.transitions
            .retain(|transition| transition.from != state && transition.to != state);
        if self.selected_state.as_deref() == Some(state) {
            self.selected_state = self.states.first().cloned();
        }
    }

    pub fn add_transition(&mut self, from: String, to: String, on: String) -> bool {
        if !self.states.contains(&from) || !self.states.contains(&to) || on.trim().is_empty() {
            return false;
        }
        self.transitions.push(FsmTransition { from, to, on });
        true
    }

    pub fn remove_transition(&mut self, index: usize) {
        if index < self.transitions.len() {
            self.transitions.remove(index);
            if self.selected_transition == Some(index) {
                self.selected_transition = None;
            }
            if let Some(selected) = self.selected_transition {
                if selected >= self.transitions.len() {
                    self.selected_transition = self.transitions.len().checked_sub(1);
                }
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmAgentConfig {
    pub enabled: bool,
    pub model: String,
    pub system_prompt: String,
    pub temperature: f32,
    pub tool_budget: u32,
    pub max_tokens: u32,
    pub memory_window: u32,
}

impl Default for LlmAgentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            model: "gpt-4o-mini".to_string(),
            system_prompt: "Act with game intent and deterministic fallback.".to_string(),
            temperature: 0.7,
            tool_budget: 2,
            max_tokens: 256,
            memory_window: 6,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TelemetryAgentSummary {
    pub agent_id: String,
    pub entity_id: Option<u64>,
    pub role: String,
    pub trajectory_distance: f32,
    pub rejected_actions: usize,
    pub tool_errors: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EncounterBalancePreview {
    pub table_id: String,
    pub biome_id: String,
    pub spawn_group: String,
    pub ambient_cap_per_chunk: u16,
    pub effective_cap: u32,
    pub attached_chunk_count: u32,
    pub hot_chunk_count: u32,
    pub active_spawned_actors: u32,
    pub spawn_budget_remaining: u32,
    pub pending_respawns: u32,
    pub next_respawn_tick: Option<u64>,
    pub population_pressure: f32,
    pub suggested_cap_delta: i16,
}

fn breakdown_active_spawned_actors(counts: PopulationBreakdown) -> u32 {
    counts.players + counts.npcs + counts.wild_creatures + counts.companions
}

fn suggested_encounter_cap_delta(
    population_pressure: f32,
    spawn_budget_remaining: u32,
    pending_respawns: u32,
) -> i16 {
    if population_pressure >= 0.95 || (population_pressure >= 0.82 && pending_respawns >= 2) {
        2
    } else if population_pressure >= 0.72 || spawn_budget_remaining == 0 {
        1
    } else if population_pressure <= 0.25 && pending_respawns == 0 {
        -2
    } else if population_pressure <= 0.45 && spawn_budget_remaining >= 4 {
        -1
    } else {
        0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpacetimeDashboardState {
    pub latest_tick: u64,
    pub connected_players: u32,
    pub action_rejection_rate: f32,
    pub tool_call_error_rate: f32,
    pub average_tool_latency_ms: f32,
    pub average_trajectory_distance: f32,
    pub agent_summaries: Vec<TelemetryAgentSummary>,
    pub visible_entity_count: usize,
    pub audible_event_count: usize,
    pub message_count: usize,
    pub capture_actions: usize,
    pub summon_actions: usize,
    pub gather_actions: usize,
    pub loot_actions: usize,
    #[serde(default)]
    pub latest_incident_summary: Option<ShardIncidentSummary>,
    #[serde(default)]
    pub latest_transport_summary: Option<ShardTransportSummary>,
    #[serde(default)]
    pub recent_tool_call_events: VecDeque<AgentToolCallEvent>,
    #[serde(default)]
    pub recent_tick_rollups: VecDeque<AgentTickRollup>,
    #[serde(default)]
    pub recent_focused_debug_summaries: VecDeque<FocusedEntityDebugSummary>,
    #[serde(default)]
    pub population_state: WorldPopulationState,
    pub last_reducer_call: String,
    pub reducer_calls: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectedEntityDebugSummary {
    pub entity_id: u64,
    pub latest_tick: u64,
    pub recent_tool_call_count: usize,
    pub recent_rollup_count: usize,
    pub rejected_actions: usize,
    pub tool_errors: usize,
    pub trajectory_distance: f32,
    pub average_tool_latency_ms: f32,
    pub latest_tool_call: Option<AgentToolCallEvent>,
    pub latest_rollup: Option<AgentTickRollup>,
}

impl Default for SpacetimeDashboardState {
    fn default() -> Self {
        Self {
            latest_tick: 0,
            connected_players: 0,
            action_rejection_rate: 0.0,
            tool_call_error_rate: 0.0,
            average_tool_latency_ms: 0.0,
            average_trajectory_distance: 0.0,
            agent_summaries: Vec::new(),
            visible_entity_count: 0,
            audible_event_count: 0,
            message_count: 0,
            capture_actions: 0,
            summon_actions: 0,
            gather_actions: 0,
            loot_actions: 0,
            latest_incident_summary: None,
            latest_transport_summary: None,
            recent_tool_call_events: VecDeque::new(),
            recent_tick_rollups: VecDeque::new(),
            recent_focused_debug_summaries: VecDeque::new(),
            population_state: WorldPopulationState::default(),
            last_reducer_call: "none".to_string(),
            reducer_calls: 0,
        }
    }
}

impl SpacetimeDashboardState {
    pub fn record_reducer_call(&mut self, name: impl Into<String>) {
        self.last_reducer_call = name.into();
        self.reducer_calls = self.reducer_calls.saturating_add(1);
    }

    pub fn apply_connect(&mut self, connected: bool) {
        self.connected_players = if connected { 1 } else { 0 };
    }

    pub fn record_tick_telemetry(&mut self, frame: &TickTelemetryFrame) {
        self.latest_tick = frame.tick;
        self.connected_players = frame
            .agents
            .iter()
            .filter(|agent| agent.runtime_profile.agent_type == AgentType::Human)
            .count() as u32;
        self.visible_entity_count = frame
            .agents
            .iter()
            .map(|agent| agent.visible_entity_count)
            .sum();
        self.audible_event_count = frame
            .agents
            .iter()
            .map(|agent| agent.audible_event_count)
            .sum();
        self.message_count = frame.agents.iter().map(|agent| agent.message_count).sum();

        let total_actions = frame
            .agents
            .iter()
            .map(|agent| agent.action_trace.len())
            .sum::<usize>();
        let rejected_actions = frame
            .agents
            .iter()
            .flat_map(|agent| agent.action_trace.iter())
            .filter(|trace| trace.stage == ActionLifecycleStage::Rejected)
            .count();
        self.action_rejection_rate = if total_actions == 0 {
            0.0
        } else {
            rejected_actions as f32 / total_actions as f32
        };

        let total_tool_calls = frame
            .agents
            .iter()
            .map(|agent| agent.tool_calls.len())
            .sum::<usize>();
        let tool_errors = frame
            .agents
            .iter()
            .flat_map(|agent| agent.tool_calls.iter())
            .filter(|trace| {
                !matches!(
                    trace.status,
                    ToolCallStatus::Requested | ToolCallStatus::Succeeded
                )
            })
            .count();
        self.tool_call_error_rate = if total_tool_calls == 0 {
            0.0
        } else {
            tool_errors as f32 / total_tool_calls as f32
        };
        self.average_tool_latency_ms = if total_tool_calls == 0 {
            0.0
        } else {
            frame
                .agents
                .iter()
                .flat_map(|agent| agent.tool_calls.iter())
                .map(|trace| trace.latency_ms as f32)
                .sum::<f32>()
                / total_tool_calls as f32
        };
        self.average_trajectory_distance = if frame.agents.is_empty() {
            0.0
        } else {
            frame
                .agents
                .iter()
                .map(|agent| {
                    agent
                        .trajectory
                        .as_ref()
                        .map(|trajectory| trajectory.distance_travelled)
                        .unwrap_or_default()
                })
                .sum::<f32>()
                / frame.agents.len() as f32
        };
        self.capture_actions = 0;
        self.summon_actions = 0;
        self.gather_actions = 0;
        self.loot_actions = 0;

        self.agent_summaries = frame
            .agents
            .iter()
            .map(|agent| TelemetryAgentSummary {
                agent_id: agent.agent_id.to_string(),
                entity_id: agent.entity_id.map(|entity_id| entity_id.0),
                role: format!("{:?}", agent.runtime_profile.role),
                trajectory_distance: agent
                    .trajectory
                    .as_ref()
                    .map(|trajectory| trajectory.distance_travelled)
                    .unwrap_or_default(),
                rejected_actions: agent
                    .action_trace
                    .iter()
                    .filter(|trace| trace.stage == ActionLifecycleStage::Rejected)
                    .count(),
                tool_errors: agent
                    .tool_calls
                    .iter()
                    .filter(|trace| {
                        !matches!(
                            trace.status,
                            ToolCallStatus::Requested | ToolCallStatus::Succeeded
                        )
                    })
                    .count(),
            })
            .collect();

        for trace in frame
            .agents
            .iter()
            .flat_map(|agent| agent.action_trace.iter())
        {
            if trace.stage != ActionLifecycleStage::Executed {
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

    pub fn apply_incident_summary(&mut self, summary: ShardIncidentSummary) {
        self.latest_tick = summary.latest_tick;
        self.action_rejection_rate = summary.action_rejection_rate;
        self.tool_call_error_rate = summary.tool_call_error_rate;
        self.average_tool_latency_ms = summary.average_tool_latency_ms;
        self.average_trajectory_distance = summary.average_trajectory_distance;
        self.capture_actions = summary.capture_actions;
        self.summon_actions = summary.summon_actions;
        self.gather_actions = summary.gather_actions;
        self.loot_actions = summary.loot_actions;
        self.latest_incident_summary = Some(summary);
    }

    pub fn apply_transport_summary(&mut self, summary: ShardTransportSummary) {
        self.latest_tick = self.latest_tick.max(summary.latest_tick);
        self.latest_transport_summary = Some(summary);
    }

    pub fn apply_tool_call_event(&mut self, event: AgentToolCallEvent) {
        retain_recent_debug_docs(
            &mut self.recent_tool_call_events,
            TelemetryConfig::default().editor_timeline_ticks,
        );
        self.latest_tick = self.latest_tick.max(event.tick);
        self.recent_tool_call_events.push_back(event);

        let total_tool_calls = self.recent_tool_call_events.len();
        let tool_errors = self
            .recent_tool_call_events
            .iter()
            .filter(|event| {
                !matches!(
                    event.trace.status,
                    ToolCallStatus::Requested | ToolCallStatus::Succeeded
                )
            })
            .count();
        self.tool_call_error_rate = if total_tool_calls == 0 {
            0.0
        } else {
            tool_errors as f32 / total_tool_calls as f32
        };
        self.average_tool_latency_ms = if total_tool_calls == 0 {
            0.0
        } else {
            self.recent_tool_call_events
                .iter()
                .map(|event| event.trace.latency_ms as f32)
                .sum::<f32>()
                / total_tool_calls as f32
        };
    }

    pub fn apply_tick_rollup(&mut self, rollup: AgentTickRollup) {
        retain_recent_debug_docs(
            &mut self.recent_tick_rollups,
            TelemetryConfig::default().editor_timeline_ticks,
        );
        self.latest_tick = self.latest_tick.max(rollup.tick_end);
        self.visible_entity_count = rollup.visible_entity_count as usize;
        self.audible_event_count = rollup.audible_event_count as usize;
        self.message_count = rollup.message_count as usize;
        self.recent_tick_rollups.push_back(rollup.clone());

        let total_actions = self
            .recent_tick_rollups
            .iter()
            .map(|rollup| {
                rollup.submitted_action_count
                    + rollup.executed_action_count
                    + rollup.rejected_action_count
            })
            .sum::<u32>();
        let total_rejected = self
            .recent_tick_rollups
            .iter()
            .map(|rollup| rollup.rejected_action_count)
            .sum::<u32>();
        self.action_rejection_rate = if total_actions == 0 {
            0.0
        } else {
            total_rejected as f32 / total_actions as f32
        };
        self.average_trajectory_distance = if self.recent_tick_rollups.is_empty() {
            0.0
        } else {
            self.recent_tick_rollups
                .iter()
                .map(|rollup| rollup.total_distance)
                .sum::<f32>()
                / self.recent_tick_rollups.len() as f32
        };

        match self
            .agent_summaries
            .iter_mut()
            .find(|summary| summary.entity_id == Some(rollup.agent_entity_id))
        {
            Some(summary) => {
                summary.trajectory_distance = rollup.total_distance;
                summary.rejected_actions = rollup.rejected_action_count as usize;
                summary.tool_errors = rollup.tool_error_count as usize;
            }
            None => self.agent_summaries.push(TelemetryAgentSummary {
                agent_id: format!("entity:{}", rollup.agent_entity_id),
                entity_id: Some(rollup.agent_entity_id),
                role: "Rollup".to_string(),
                trajectory_distance: rollup.total_distance,
                rejected_actions: rollup.rejected_action_count as usize,
                tool_errors: rollup.tool_error_count as usize,
            }),
        }
    }

    pub fn apply_focused_entity_debug_summary(&mut self, summary: FocusedEntityDebugSummary) {
        retain_recent_debug_docs(
            &mut self.recent_focused_debug_summaries,
            TelemetryConfig::default().editor_timeline_ticks,
        );
        self.latest_tick = self.latest_tick.max(summary.latest_tick);
        self.visible_entity_count = summary.visible_entity_count;
        self.audible_event_count = summary.audible_event_count;
        self.message_count = summary.message_count;
        self.average_tool_latency_ms = summary.average_tool_latency_ms;
        self.recent_focused_debug_summaries.push_back(summary);
    }

    pub fn apply_population_state(&mut self, population_state: WorldPopulationState) {
        self.latest_tick = self.latest_tick.max(population_state.tick);
        self.population_state = population_state;
    }

    pub fn debug_summary_for_entity(&self, entity_id: u64) -> Option<SelectedEntityDebugSummary> {
        let tool_events = self
            .recent_tool_call_events
            .iter()
            .filter(|event| event.agent_entity_id == entity_id)
            .cloned()
            .collect::<Vec<_>>();
        let rollups = self
            .recent_tick_rollups
            .iter()
            .filter(|rollup| rollup.agent_entity_id == entity_id)
            .cloned()
            .collect::<Vec<_>>();
        let latest_tool_call = tool_events.last().cloned();
        let latest_rollup = rollups.last().cloned();
        let latest_focused_summary = self
            .recent_focused_debug_summaries
            .iter()
            .rev()
            .find(|summary| summary.entity_id == entity_id)
            .cloned();
        let agent_summary = self
            .agent_summaries
            .iter()
            .find(|summary| summary.entity_id == Some(entity_id));

        if latest_tool_call.is_none()
            && latest_rollup.is_none()
            && latest_focused_summary.is_none()
            && agent_summary.is_none()
        {
            return None;
        }

        let recent_tool_call_count = tool_events.len();
        let recent_rollup_count = rollups.len();
        let tool_errors = if recent_tool_call_count > 0 {
            tool_events
                .iter()
                .filter(|event| {
                    !matches!(
                        event.trace.status,
                        ToolCallStatus::Requested | ToolCallStatus::Succeeded
                    )
                })
                .count()
        } else if let Some(rollup) = &latest_rollup {
            rollup.tool_error_count as usize
        } else if let Some(summary) = &latest_focused_summary {
            summary.tool_error_count
        } else {
            agent_summary.map(|summary| summary.tool_errors).unwrap_or_default()
        };
        let trajectory_distance = latest_rollup
            .as_ref()
            .map(|rollup| rollup.total_distance)
            .or_else(|| latest_focused_summary.as_ref().map(|summary| summary.total_distance))
            .or_else(|| agent_summary.map(|summary| summary.trajectory_distance))
            .unwrap_or_default();
        let rejected_actions = latest_rollup
            .as_ref()
            .map(|rollup| rollup.rejected_action_count as usize)
            .or_else(|| {
                latest_focused_summary
                    .as_ref()
                    .map(|summary| summary.rejected_action_count)
            })
            .or_else(|| agent_summary.map(|summary| summary.rejected_actions))
            .unwrap_or_default();
        let average_tool_latency_ms = if recent_tool_call_count > 0 {
            tool_events
                .iter()
                .map(|event| event.trace.latency_ms as f32)
                .sum::<f32>()
                / recent_tool_call_count as f32
        } else {
            latest_rollup
                .as_ref()
                .map(|rollup| rollup.average_tool_latency_ms)
                .or_else(|| {
                    latest_focused_summary
                        .as_ref()
                        .map(|summary| summary.average_tool_latency_ms)
                })
                .unwrap_or_default()
        };
        let latest_tick = latest_tool_call
            .as_ref()
            .map(|event| event.tick)
            .into_iter()
            .chain(latest_rollup.as_ref().map(|rollup| rollup.tick_end))
            .chain(
                latest_focused_summary
                    .as_ref()
                    .map(|summary| summary.latest_tick),
            )
            .max()
            .unwrap_or(self.latest_tick);

        Some(SelectedEntityDebugSummary {
            entity_id,
            latest_tick,
            recent_tool_call_count,
            recent_rollup_count,
            rejected_actions,
            tool_errors,
            trajectory_distance,
            average_tool_latency_ms,
            latest_tool_call,
            latest_rollup,
        })
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("editor_spacetime_dashboard", self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TelemetryPanelState {
    pub max_ticks: usize,
    pub timeline: VecDeque<TickTelemetryFrame>,
}

impl Default for TelemetryPanelState {
    fn default() -> Self {
        Self::with_capacity(TelemetryConfig::default().editor_timeline_ticks)
    }
}

impl TelemetryPanelState {
    pub fn with_capacity(max_ticks: usize) -> Self {
        Self {
            max_ticks: max_ticks.max(1),
            timeline: VecDeque::with_capacity(max_ticks.max(1)),
        }
    }

    pub fn record_tick(&mut self, frame: TickTelemetryFrame) {
        if self.timeline.len() >= self.max_ticks {
            self.timeline.pop_front();
        }
        self.timeline.push_back(frame);
    }

    pub fn replace_timeline(&mut self, frames: impl IntoIterator<Item = TickTelemetryFrame>) {
        self.timeline.clear();
        for frame in frames {
            self.record_tick(frame);
        }
    }

    pub fn latest(&self) -> Option<&TickTelemetryFrame> {
        self.timeline.back()
    }

    pub fn latest_agent_for_entity(&self, entity_id: u64) -> Option<&AgentTelemetryFrame> {
        self.timeline.iter().rev().find_map(|frame| {
            frame
                .agents
                .iter()
                .find(|agent| agent.entity_id.map(|id| id.0) == Some(entity_id))
        })
    }

    pub fn trajectory_for_entity(&self, entity_id: u64) -> Vec<TrajectorySample> {
        let mut samples = Vec::new();
        for frame in &self.timeline {
            if let Some(agent) = frame
                .agents
                .iter()
                .find(|agent| agent.entity_id.map(|id| id.0) == Some(entity_id))
            {
                if let Some(trajectory) = &agent.trajectory {
                    if samples.is_empty() {
                        samples.push(trajectory.start);
                    }
                    samples.push(trajectory.end);
                }
            }
        }
        samples
    }

    pub fn trajectory_distance_for_entity(&self, entity_id: u64) -> f32 {
        self.timeline
            .iter()
            .flat_map(|frame| frame.agents.iter())
            .filter(|agent| agent.entity_id.map(|id| id.0) == Some(entity_id))
            .map(|agent| {
                agent
                    .trajectory
                    .as_ref()
                    .map(|trajectory| trajectory.distance_travelled)
                    .unwrap_or_default()
            })
            .sum()
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("editor_telemetry_panel", self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorEntityWorldBinding {
    pub entity_id: u64,
    pub chunk_key: Option<String>,
    pub region_id: Option<String>,
    pub region_name: Option<String>,
    pub quest_graph_ids: Vec<String>,
    pub faction_track_id: Option<String>,
    pub encounter_table_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorWorldGraphState {
    pub chunk_size: f32,
    pub regions: Vec<WorldRegionDefinition>,
    pub chunks: Vec<WorldChunkDefinition>,
    pub quest_graphs: Vec<QuestStateGraph>,
    pub faction_tracks: Vec<FactionReputationTrack>,
    pub encounter_tables: Vec<RegionEncounterTable>,
    pub entity_bindings: Vec<EditorEntityWorldBinding>,
}

impl Default for EditorWorldGraphState {
    fn default() -> Self {
        let mut state = Self::flagship_seed();
        state.sync_entity_bindings(&SceneHierarchy::default(), &default_transforms_2d());
        state
    }
}

impl EditorWorldGraphState {
    pub fn flagship_seed() -> Self {
        let mut verdant_region =
            WorldRegionDefinition::new("verdant-heart", "Verdant Heart", "verdant-hollow");
        verdant_region.chunk_keys = vec!["0:0".into(), "0:-1".into()];
        verdant_region.active_quest_graph_ids = vec!["verdant-intro".into(), "lynx-patrol".into()];
        verdant_region.dominant_faction_track_id = "verdant-wardens".into();
        verdant_region.encounter_table_ids = vec![
            "verdant-heart-wildlife".into(),
            "verdant-heart-resources".into(),
        ];

        let mut spire_region =
            WorldRegionDefinition::new("spirewatch", "Spirewatch Rise", "spirewatch");
        spire_region.chunk_keys = vec!["0:1".into(), "1:1".into()];
        spire_region.active_quest_graph_ids = vec!["spire-attunement".into()];
        spire_region.dominant_faction_track_id = "ancient-spirekeepers".into();
        spire_region.encounter_table_ids = vec!["spirewatch-encounters".into()];

        let mut steppe_region =
            WorldRegionDefinition::new("ashen-steppe", "Ashen Steppe", "ashen-steppe");
        steppe_region.chunk_keys = vec!["1:-1".into(), "1:0".into()];
        steppe_region.active_quest_graph_ids = vec!["tempered-trail".into()];
        steppe_region.dominant_faction_track_id = "ashen-forgebound".into();
        steppe_region.encounter_table_ids = vec!["ashen-steppe-encounters".into()];

        let mut verdant_chunk = WorldChunkDefinition::new("0:0", "verdant-heart", "verdant-hollow");
        verdant_chunk.quest_graph_ids = vec!["verdant-intro".into(), "lynx-patrol".into()];
        verdant_chunk.faction_track_ids = vec!["verdant-wardens".into()];
        verdant_chunk.encounter_table_ids = vec!["verdant-heart-wildlife".into()];
        verdant_chunk.neighbor_chunk_keys = vec!["0:-1".into(), "1:-1".into(), "0:1".into()];

        let mut frontier_chunk = WorldChunkDefinition::new("1:-1", "ashen-steppe", "ashen-steppe");
        frontier_chunk.quest_graph_ids = vec!["tempered-trail".into()];
        frontier_chunk.faction_track_ids = vec!["ashen-forgebound".into()];
        frontier_chunk.encounter_table_ids = vec!["ashen-steppe-encounters".into()];
        frontier_chunk.neighbor_chunk_keys = vec!["0:0".into(), "1:0".into()];

        let mut spire_chunk = WorldChunkDefinition::new("0:1", "spirewatch", "spirewatch");
        spire_chunk.quest_graph_ids = vec!["spire-attunement".into()];
        spire_chunk.faction_track_ids = vec!["ancient-spirekeepers".into()];
        spire_chunk.encounter_table_ids = vec!["spirewatch-encounters".into()];
        spire_chunk.neighbor_chunk_keys = vec!["0:0".into(), "1:1".into()];

        Self {
            chunk_size: 8.0,
            regions: vec![verdant_region, spire_region, steppe_region],
            chunks: vec![verdant_chunk, frontier_chunk, spire_chunk],
            quest_graphs: vec![
                QuestStateGraph::new(
                    "verdant-intro",
                    "Verdant Hollow Introduction",
                    "speak-to-mara",
                    vec![
                        QuestStageDefinition {
                            stage_id: "speak-to-mara".into(),
                            title: "Speak to Mara".into(),
                            objectives: vec!["Find Archivist Mara in Verdant Heart".into()],
                            next_stage_ids: vec!["inspect-the-spire".into()],
                            reward_tags: vec!["unlock:verdant-heart".into()],
                        },
                        QuestStageDefinition {
                            stage_id: "inspect-the-spire".into(),
                            title: "Inspect the Spire".into(),
                            objectives: vec!["Reach the spire ridge and inspect the anchor".into()],
                            next_stage_ids: vec![],
                            reward_tags: vec!["unlock:spirewatch".into()],
                        },
                    ],
                ),
                QuestStateGraph::new(
                    "lynx-patrol",
                    "Lynx Patrol",
                    "spot-the-lynx",
                    vec![
                        QuestStageDefinition {
                            stage_id: "spot-the-lynx".into(),
                            title: "Spot the Lynx".into(),
                            objectives: vec!["Track a wild lynx through Verdant Heart".into()],
                            next_stage_ids: vec!["capture-the-lynx".into()],
                            reward_tags: vec!["encounter:verdant-heart-wildlife".into()],
                        },
                        QuestStageDefinition {
                            stage_id: "capture-the-lynx".into(),
                            title: "Capture the Lynx".into(),
                            objectives: vec!["Capture or pacify the wild lynx".into()],
                            next_stage_ids: vec![],
                            reward_tags: vec!["companion:lynx".into()],
                        },
                    ],
                ),
                QuestStateGraph::new(
                    "tempered-trail",
                    "Tempered Trail",
                    "reach-the-forge",
                    vec![QuestStageDefinition {
                        stage_id: "reach-the-forge".into(),
                        title: "Reach the Frontier Forge".into(),
                        objectives: vec!["Cross into the Ashen Steppe and report to Ivo".into()],
                        next_stage_ids: vec![],
                        reward_tags: vec!["unlock:ashen-steppe".into()],
                    }],
                ),
                QuestStateGraph::new(
                    "spire-attunement",
                    "Spire Attunement",
                    "align-the-anchor",
                    vec![QuestStageDefinition {
                        stage_id: "align-the-anchor".into(),
                        title: "Align the Anchor".into(),
                        objectives: vec!["Stabilize the spire anchor for regional travel".into()],
                        next_stage_ids: vec![],
                        reward_tags: vec!["unlock:moonstone".into()],
                    }],
                ),
            ],
            faction_tracks: vec![
                FactionReputationTrack::new(
                    "verdant-wardens",
                    "Verdant Wardens",
                    vec![
                        FactionReputationTier {
                            tier_id: "outsider".into(),
                            label: "Outsider".into(),
                            minimum_score: 0,
                            perk_tags: vec!["quest:verdant-intro".into()],
                        },
                        FactionReputationTier {
                            tier_id: "trusted".into(),
                            label: "Trusted".into(),
                            minimum_score: 30,
                            perk_tags: vec!["resource:moonstone".into()],
                        },
                    ],
                ),
                FactionReputationTrack::new(
                    "ancient-spirekeepers",
                    "Ancient Spirekeepers",
                    vec![FactionReputationTier {
                        tier_id: "recognized".into(),
                        label: "Recognized".into(),
                        minimum_score: 20,
                        perk_tags: vec!["quest:spire-attunement".into()],
                    }],
                ),
                FactionReputationTrack::new(
                    "ashen-forgebound",
                    "Ashen Forgebound",
                    vec![FactionReputationTier {
                        tier_id: "forge-friend".into(),
                        label: "Forge Friend".into(),
                        minimum_score: 25,
                        perk_tags: vec!["quest:tempered-trail".into()],
                    }],
                ),
            ],
            encounter_tables: vec![
                RegionEncounterTable::new(
                    "verdant-heart-wildlife",
                    "verdant-hollow",
                    "wildlife",
                    vec![EncounterSpawnEntry {
                        archetype_id: "wild-lynx".into(),
                        weight: 10,
                        min_count: 1,
                        max_count: 2,
                        required_stage_tags: vec!["hunt".into()],
                        required_reputation_tiers: vec![],
                    }],
                ),
                RegionEncounterTable::new(
                    "verdant-heart-resources",
                    "verdant-hollow",
                    "resources",
                    vec![EncounterSpawnEntry {
                        archetype_id: "moonstone-outcrop".into(),
                        weight: 6,
                        min_count: 1,
                        max_count: 1,
                        required_stage_tags: vec!["unlock:moonstone".into()],
                        required_reputation_tiers: vec!["trusted".into()],
                    }],
                ),
                RegionEncounterTable::new(
                    "spirewatch-encounters",
                    "spirewatch",
                    "anomalies",
                    vec![EncounterSpawnEntry {
                        archetype_id: "spire-anomaly".into(),
                        weight: 5,
                        min_count: 1,
                        max_count: 1,
                        required_stage_tags: vec!["unlock:spirewatch".into()],
                        required_reputation_tiers: vec!["recognized".into()],
                    }],
                ),
                RegionEncounterTable::new(
                    "ashen-steppe-encounters",
                    "ashen-steppe",
                    "forgefront",
                    vec![EncounterSpawnEntry {
                        archetype_id: "ember-stag".into(),
                        weight: 8,
                        min_count: 1,
                        max_count: 3,
                        required_stage_tags: vec!["unlock:ashen-steppe".into()],
                        required_reputation_tiers: vec!["forge-friend".into()],
                    }],
                ),
            ],
            entity_bindings: Vec::new(),
        }
    }

    pub fn binding_for_entity(&self, entity_id: u64) -> Option<&EditorEntityWorldBinding> {
        self.entity_bindings
            .iter()
            .find(|binding| binding.entity_id == entity_id)
    }

    pub fn sync_entity_bindings(
        &mut self,
        hierarchy: &SceneHierarchy,
        transforms: &HashMap<u64, Transform2D>,
    ) {
        self.entity_bindings.clear();
        for entity in hierarchy.entities() {
            let Some(transform) = transforms.get(&entity.entity_id) else {
                continue;
            };
            let chunk_key =
                editor_chunk_key_from_position(transform.x, transform.y, self.chunk_size);
            let chunk = self
                .chunks
                .iter()
                .find(|chunk| chunk.chunk_key == chunk_key);
            let region = chunk
                .and_then(|chunk| {
                    self.regions
                        .iter()
                        .find(|region| region.region_id == chunk.region_id)
                })
                .or_else(|| {
                    self.regions
                        .iter()
                        .find(|region| region.chunk_keys.iter().any(|value| value == &chunk_key))
                });

            let mut quest_graph_ids = chunk
                .map(|chunk| chunk.quest_graph_ids.clone())
                .unwrap_or_default();
            if let Some(region) = region {
                for quest_id in &region.active_quest_graph_ids {
                    if !quest_graph_ids.contains(quest_id) {
                        quest_graph_ids.push(quest_id.clone());
                    }
                }
            }

            let faction_track_id = chunk
                .and_then(|chunk| chunk.faction_track_ids.first().cloned())
                .or_else(|| {
                    region.and_then(|region| {
                        (!region.dominant_faction_track_id.is_empty())
                            .then(|| region.dominant_faction_track_id.clone())
                    })
                });
            let encounter_table_id = chunk
                .and_then(|chunk| chunk.encounter_table_ids.first().cloned())
                .or_else(|| region.and_then(|region| region.encounter_table_ids.first().cloned()));

            self.entity_bindings.push(EditorEntityWorldBinding {
                entity_id: entity.entity_id,
                chunk_key: Some(chunk_key),
                region_id: region.map(|region| region.region_id.clone()),
                region_name: region.map(|region| region.display_name.clone()),
                quest_graph_ids,
                faction_track_id,
                encounter_table_id,
            });
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("editor_world_graph", self)
    }
}

fn editor_chunk_key_from_position(x: f32, y: f32, chunk_size: f32) -> String {
    let chunk_size = chunk_size.max(0.001);
    let chunk_x = (x / chunk_size).floor() as i32;
    let chunk_y = (y / chunk_size).floor() as i32;
    format!("{chunk_x}:{chunk_y}")
}

fn retain_recent_debug_docs<T>(entries: &mut VecDeque<T>, max_entries: usize) {
    while entries.len() >= max_entries.max(1) {
        entries.pop_front();
    }
}

#[derive(Clone, Debug, Default)]
struct EditorHistory {
    undo_stack: Vec<EditorState>,
    redo_stack: Vec<EditorState>,
    max_depth: usize,
}

impl EditorHistory {
    fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            max_depth: 32,
        }
    }

    fn remember(&mut self, snapshot: EditorState) {
        self.undo_stack.push(snapshot);
        if self.undo_stack.len() > self.max_depth {
            let _ = self.undo_stack.drain(0..1);
        }
        self.redo_stack.clear();
    }

    fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    fn undo(&mut self, current: &mut EditorState) -> bool {
        if let Some(previous) = self.undo_stack.pop() {
            self.redo_stack.push(current.clone());
            *current = previous;
            true
        } else {
            false
        }
    }

    fn redo(&mut self, current: &mut EditorState) -> bool {
        if let Some(next) = self.redo_stack.pop() {
            self.undo_stack.push(current.clone());
            *current = next;
            true
        } else {
            false
        }
    }

    fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AssetKind {
    Mesh,
    Scene,
    Texture,
    Audio,
    Script,
    Other,
}

impl AssetKind {
    fn label(self) -> &'static str {
        match self {
            Self::Mesh => "Mesh",
            Self::Scene => "Scene",
            Self::Texture => "Texture",
            Self::Audio => "Audio",
            Self::Script => "Script",
            Self::Other => "Other",
        }
    }

    fn from_path(path: &str) -> Self {
        match PathBuf::from(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("")
            .to_ascii_lowercase()
            .as_str()
        {
            "gltf" | "glb" | "fbx" => Self::Mesh,
            "tscn" | "scn" | "unity" | "prefab" | "tmj" => Self::Scene,
            "png" | "jpg" | "jpeg" => Self::Texture,
            "wav" | "ogg" | "mp3" => Self::Audio,
            "lua" | "json" | "ron" => Self::Script,
            _ => Self::Other,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorAsset {
    pub id: String,
    pub path: String,
    pub label: String,
    pub kind: AssetKind,
    pub size_bytes: u64,
    pub enabled: bool,
}

impl EditorAsset {
    fn with_path(path: impl Into<String>) -> Self {
        let path = path.into();
        let label = PathBuf::from(&path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("asset")
            .to_string();
        Self {
            id: label.clone(),
            path: path.clone(),
            label,
            kind: AssetKind::from_path(&path),
            size_bytes: 1_024,
            enabled: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AssetBrowserState {
    pub assets: Vec<EditorAsset>,
    pub query: String,
    pub import_path: String,
    pub selected: Option<String>,
    pub filter: Option<AssetKind>,
}

impl Default for AssetBrowserState {
    fn default() -> Self {
        Self {
            assets: vec![
                EditorAsset {
                    id: "hero_character".to_string(),
                    path: "assets/characters/hero_character.glb".to_string(),
                    label: "hero_character".to_string(),
                    kind: AssetKind::Mesh,
                    size_bytes: 2_048_000,
                    enabled: true,
                },
                EditorAsset {
                    id: "environment_city".to_string(),
                    path: "assets/meshes/environment_city.gltf".to_string(),
                    label: "environment_city".to_string(),
                    kind: AssetKind::Mesh,
                    size_bytes: 6_400_000,
                    enabled: true,
                },
                EditorAsset {
                    id: "arena_blockout".to_string(),
                    path: "assets/scenes/arena_blockout.tscn".to_string(),
                    label: "arena_blockout".to_string(),
                    kind: AssetKind::Scene,
                    size_bytes: 18_432,
                    enabled: true,
                },
                EditorAsset {
                    id: "ui_theme".to_string(),
                    path: "assets/textures/ui_theme.png".to_string(),
                    label: "ui_theme".to_string(),
                    kind: AssetKind::Texture,
                    size_bytes: 64_000,
                    enabled: true,
                },
                EditorAsset {
                    id: "ambient_loop".to_string(),
                    path: "assets/audio/ambient_loop.wav".to_string(),
                    label: "ambient_loop".to_string(),
                    kind: AssetKind::Audio,
                    size_bytes: 3_840_000,
                    enabled: true,
                },
                EditorAsset {
                    id: "dialog_behavior".to_string(),
                    path: "assets/scripts/dialog_behavior.lua".to_string(),
                    label: "dialog_behavior".to_string(),
                    kind: AssetKind::Script,
                    size_bytes: 4_096,
                    enabled: true,
                },
            ],
            query: String::new(),
            import_path: String::new(),
            selected: None,
            filter: None,
        }
    }
}

impl AssetBrowserState {
    fn all_match_query(&self, asset: &EditorAsset, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let q = query.to_ascii_lowercase();
        asset.label.to_ascii_lowercase().contains(&q)
            || asset.path.to_ascii_lowercase().contains(&q)
    }

    fn visible_assets(&self) -> Vec<&EditorAsset> {
        self.assets
            .iter()
            .filter(|asset| {
                if let Some(filter_kind) = self.filter {
                    if asset.kind != filter_kind {
                        return false;
                    }
                }
                self.all_match_query(asset, &self.query)
            })
            .collect()
    }

    fn select(&mut self, asset_id: impl Into<String>) {
        self.selected = Some(asset_id.into());
    }

    fn clear_selection(&mut self) {
        self.selected = None;
    }

    fn import_asset(&mut self, path: impl Into<String>) {
        let path = path.into();
        if path.trim().is_empty() {
            return;
        }
        let asset = EditorAsset::with_path(path);
        self.select(asset.id.clone());
        self.assets.push(asset);
    }

    fn toggle_asset_enabled(&mut self, asset_id: &str) {
        if let Some(asset) = self.assets.iter_mut().find(|asset| asset.id == asset_id) {
            asset.enabled = !asset.enabled;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CreatorNodeKind {
    Model,
    Object,
    Monster,
    Entity,
    BehaviorTree,
    StateMachine,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CreatorAssociationKind {
    VisualModel,
    SceneEntity,
    EncounterTrigger,
    CompanionBond,
    BehaviorSource,
    StateSource,
    DependsOn,
}

impl CreatorAssociationKind {
    fn label(self) -> &'static str {
        match self {
            Self::VisualModel => "visual-model",
            Self::SceneEntity => "scene-entity",
            Self::EncounterTrigger => "encounter-trigger",
            Self::CompanionBond => "companion-bond",
            Self::BehaviorSource => "behavior-source",
            Self::StateSource => "state-source",
            Self::DependsOn => "depends-on",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatorEntityDefinition {
    pub id: String,
    pub name: String,
    pub entity_id: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatorModelDefinition {
    pub id: String,
    pub name: String,
    pub asset_id: String,
    pub asset_path: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatorObjectDefinition {
    pub id: String,
    pub name: String,
    pub entity_id: Option<u64>,
    pub model_id: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatorMonsterDefinition {
    pub id: String,
    pub name: String,
    pub entity_id: Option<u64>,
    pub model_id: Option<String>,
    pub creature: CreatureIdentity,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreatorAssociation {
    pub from_id: String,
    pub from_kind: CreatorNodeKind,
    pub to_id: String,
    pub to_kind: CreatorNodeKind,
    pub relation: CreatorAssociationKind,
    pub label: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreatorCatalog {
    pub entities: Vec<CreatorEntityDefinition>,
    pub models: Vec<CreatorModelDefinition>,
    pub objects: Vec<CreatorObjectDefinition>,
    pub monsters: Vec<CreatorMonsterDefinition>,
    pub associations: Vec<CreatorAssociation>,
}

fn creator_slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    let compact = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if compact.is_empty() {
        "entry".to_string()
    } else {
        compact
    }
}

impl CreatorCatalog {
    fn ensure_entity(&mut self, entity_id: u64, name: impl Into<String>) -> String {
        let id = format!("entity:{entity_id}");
        if !self.entities.iter().any(|entity| entity.id == id) {
            self.entities.push(CreatorEntityDefinition {
                id: id.clone(),
                name: name.into(),
                entity_id,
            });
        }
        id
    }

    fn ensure_model_for_asset(&mut self, asset: &EditorAsset) -> String {
        let id = format!("model:{}", asset.id);
        if !self.models.iter().any(|model| model.id == id) {
            self.models.push(CreatorModelDefinition {
                id: id.clone(),
                name: asset.label.clone(),
                asset_id: asset.id.clone(),
                asset_path: asset.path.clone(),
            });
        }
        id
    }

    fn associate(
        &mut self,
        from_id: impl Into<String>,
        from_kind: CreatorNodeKind,
        to_id: impl Into<String>,
        to_kind: CreatorNodeKind,
        relation: CreatorAssociationKind,
        label: impl Into<String>,
    ) {
        let association = CreatorAssociation {
            from_id: from_id.into(),
            from_kind,
            to_id: to_id.into(),
            to_kind,
            relation,
            label: label.into(),
        };

        if self.associations.iter().any(|existing| {
            existing.from_id == association.from_id
                && existing.from_kind == association.from_kind
                && existing.to_id == association.to_id
                && existing.to_kind == association.to_kind
                && existing.relation == association.relation
        }) {
            return;
        }

        self.associations.push(association);
    }

    fn create_object(
        &mut self,
        name: impl Into<String>,
        entity_id: Option<u64>,
        entity_label: Option<&str>,
        asset: Option<&EditorAsset>,
    ) -> String {
        let name = name.into();
        let id = format!("object:{}", creator_slug(&name));
        let model_id = asset.map(|asset| self.ensure_model_for_asset(asset));

        if let Some(existing) = self.objects.iter_mut().find(|object| object.id == id) {
            existing.entity_id = entity_id.or(existing.entity_id);
            if model_id.is_some() {
                existing.model_id = model_id.clone();
            }
        } else {
            self.objects.push(CreatorObjectDefinition {
                id: id.clone(),
                name: name.clone(),
                entity_id,
                model_id: model_id.clone(),
                tags: vec!["object".to_string()],
            });
        }

        if let Some(asset) = asset {
            let model_id = model_id.expect("model id should exist when asset is present");
            self.associate(
                id.clone(),
                CreatorNodeKind::Object,
                model_id,
                CreatorNodeKind::Model,
                CreatorAssociationKind::VisualModel,
                format!("{name} uses {}", asset.label),
            );
        }

        if let Some(entity_id) = entity_id {
            let entity_name = entity_label.unwrap_or("Entity");
            let entity_ref = self.ensure_entity(entity_id, entity_name.to_string());
            self.associate(
                id.clone(),
                CreatorNodeKind::Object,
                entity_ref,
                CreatorNodeKind::Entity,
                CreatorAssociationKind::SceneEntity,
                format!("{name} is placed on entity #{entity_id}"),
            );
        }

        id
    }

    fn create_monster(
        &mut self,
        name: impl Into<String>,
        entity_id: Option<u64>,
        entity_label: Option<&str>,
        asset: Option<&EditorAsset>,
        creature: CreatureIdentity,
    ) -> String {
        let name = name.into();
        let id = format!("monster:{}", creator_slug(&name));
        let model_id = asset.map(|asset| self.ensure_model_for_asset(asset));

        if let Some(existing) = self.monsters.iter_mut().find(|monster| monster.id == id) {
            existing.entity_id = entity_id.or(existing.entity_id);
            if model_id.is_some() {
                existing.model_id = model_id.clone();
            }
            existing.creature = creature.clone();
        } else {
            self.monsters.push(CreatorMonsterDefinition {
                id: id.clone(),
                name: name.clone(),
                entity_id,
                model_id: model_id.clone(),
                creature,
                tags: vec!["monster".to_string(), "creature".to_string()],
            });
        }

        if let Some(asset) = asset {
            let model_id = model_id.expect("model id should exist when asset is present");
            self.associate(
                id.clone(),
                CreatorNodeKind::Monster,
                model_id,
                CreatorNodeKind::Model,
                CreatorAssociationKind::VisualModel,
                format!("{name} uses {}", asset.label),
            );
        }

        if let Some(entity_id) = entity_id {
            let entity_name = entity_label.unwrap_or("Entity");
            let entity_ref = self.ensure_entity(entity_id, entity_name.to_string());
            self.associate(
                id.clone(),
                CreatorNodeKind::Monster,
                entity_ref,
                CreatorNodeKind::Entity,
                CreatorAssociationKind::SceneEntity,
                format!("{name} is bound to entity #{entity_id}"),
            );
        }

        id
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PlayState {
    Stopped,
    Playing,
    Paused,
}

impl PlayState {
    fn label(self) -> &'static str {
        match self {
            Self::Stopped => "Stopped",
            Self::Playing => "Playing",
            Self::Paused => "Paused",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HierarchyNode {
    pub entity_id: u64,
    pub label: String,
    pub children: Vec<HierarchyNode>,
}

impl HierarchyNode {
    fn new(entity_id: u64, label: impl Into<String>) -> Self {
        Self {
            entity_id,
            label: label.into(),
            children: Vec::new(),
        }
    }

    fn add_child(&mut self, child: HierarchyNode) {
        self.children.push(child);
    }

    fn contains_entity(&self, target: u64) -> bool {
        if self.entity_id == target {
            return true;
        }
        for child in &self.children {
            if child.contains_entity(target) {
                return true;
            }
        }
        false
    }

    fn render(&mut self, ui: &mut Ui, selected_entity: &mut Option<u64>) {
        if self.children.is_empty() {
            let active = *selected_entity == Some(self.entity_id);
            if ui.selectable_label(active, &self.label).clicked() {
                *selected_entity = Some(self.entity_id);
            }
            return;
        }

        let active = *selected_entity == Some(self.entity_id);
        ui.horizontal(|ui| {
            if ui.selectable_label(active, &self.label).clicked() {
                *selected_entity = Some(self.entity_id);
            }
            ui.menu_button("▼", |_| {});
        });
        ui.indent(format!("node-{}", self.entity_id), |ui| {
            for child in &mut self.children {
                child.render(ui, selected_entity);
            }
        });
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SceneHierarchy {
    pub roots: Vec<HierarchyNode>,
}

impl Default for SceneHierarchy {
    fn default() -> Self {
        let mut player = HierarchyNode::new(1001, "Player");
        player.add_child(HierarchyNode::new(1002, "Player Renderer"));
        player.add_child(HierarchyNode::new(1003, "Player Collider"));

        let mut world = HierarchyNode::new(1000, "World");
        world.add_child(player);
        world.add_child(HierarchyNode::new(1004, "Environment"));

        Self { roots: vec![world] }
    }
}

impl SceneHierarchy {
    pub fn contains_entity(&self, entity_id: u64) -> bool {
        self.roots
            .iter()
            .any(|node| node.contains_entity(entity_id))
    }

    pub fn entity_label(&self, entity_id: u64) -> Option<&str> {
        fn find_label(node: &HierarchyNode, entity_id: u64) -> Option<&str> {
            if node.entity_id == entity_id {
                return Some(node.label.as_str());
            }

            for child in &node.children {
                if let Some(label) = find_label(child, entity_id) {
                    return Some(label);
                }
            }

            None
        }

        self.roots
            .iter()
            .find_map(|node| find_label(node, entity_id))
    }

    pub fn entities(&self) -> Vec<&HierarchyNode> {
        fn collect<'a>(node: &'a HierarchyNode, output: &mut Vec<&'a HierarchyNode>) {
            output.push(node);
            for child in &node.children {
                collect(child, output);
            }
        }

        let mut entities = Vec::new();
        for root in &self.roots {
            collect(root, &mut entities);
        }
        entities
    }

    pub fn render(&mut self, ui: &mut Ui, selected_entity: &mut Option<u64>) {
        for node in &mut self.roots {
            node.render(ui, selected_entity);
        }
    }

    pub fn entity_count(&self) -> usize {
        fn count_nodes(node: &HierarchyNode) -> usize {
            1 + node.children.iter().map(count_nodes).sum::<usize>()
        }
        self.roots.iter().map(count_nodes).sum()
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct EntityPropertyMap {
    pub entries: HashMap<String, String>,
}

impl EntityPropertyMap {
    fn with_defaults(entity_id: u64) -> Self {
        let entries = match entity_id {
            1000 => [
                ("Name", "World"),
                ("Gravity", "-9.81"),
                ("Lighting", "Enabled"),
            ],
            1001 => [("Name", "Player"), ("Health", "100"), ("MoveSpeed", "4.5")],
            1002 => [
                ("Name", "Player Renderer"),
                ("Material", "default_player.mat"),
                ("Visible", "true"),
            ],
            1003 => [
                ("Name", "Player Collider"),
                ("Shape", "Capsule"),
                ("Radius", "0.5"),
            ],
            1004 => [
                ("Name", "Environment"),
                ("Biome", "City"),
                ("TimeOfDay", "Noon"),
            ],
            _ => [
                ("Name", "Entity"),
                ("Component", "Transform"),
                ("Visible", "true"),
            ],
        };
        Self {
            entries: entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Transform2D {
    pub x: f32,
    pub y: f32,
    pub rotation_deg: f32,
    pub scale: f32,
}

impl Default for Transform2D {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            rotation_deg: 0.0,
            scale: 1.0,
        }
    }
}

impl Transform2D {
    fn with_defaults(entity_id: u64) -> Self {
        match entity_id {
            1001 => Self {
                x: 4.0,
                y: 1.5,
                rotation_deg: 45.0,
                scale: 1.2,
            },
            1002 => Self {
                x: 3.9,
                y: 1.6,
                rotation_deg: 0.0,
                scale: 0.6,
            },
            1003 => Self {
                x: 4.2,
                y: 1.4,
                rotation_deg: 0.0,
                scale: 0.7,
            },
            1004 => Self {
                x: 12.0,
                y: -4.0,
                rotation_deg: 0.0,
                scale: 2.5,
            },
            _ => Self::default(),
        }
    }
}

fn default_transforms_2d() -> HashMap<u64, Transform2D> {
    [1000, 1001, 1002, 1003, 1004]
        .into_iter()
        .map(|entity_id| (entity_id, Transform2D::with_defaults(entity_id)))
        .collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorState {
    pub project_name: String,
    pub active_panel: EditorPanel,
    pub layout: DockLayout,
    pub selected_entity: Option<u64>,
    pub run_state: PlayState,
    pub viewport_mode: ViewportMode,
    pub viewport_3d: Viewport3DState,
    pub hierarchy: SceneHierarchy,
    pub inspector_data: HashMap<u64, EntityPropertyMap>,
    pub transforms_2d: HashMap<u64, Transform2D>,
    pub console_output: Vec<String>,
    pub asset_browser: AssetBrowserState,
    pub behavior_tree: BehaviorTree,
    pub fsm: FiniteStateMachine,
    pub llm_agent_config: LlmAgentConfig,
    pub telemetry: TelemetryPanelState,
    pub spacetime_dashboard: SpacetimeDashboardState,
    #[serde(default)]
    pub world_graph: EditorWorldGraphState,
    pub creator_catalog: CreatorCatalog,
    #[serde(default)]
    pub latest_replay: Option<ReplayFile>,
}

impl Default for EditorState {
    fn default() -> Self {
        let mut inspector_data = HashMap::new();
        for entity_id in [1000, 1001, 1002, 1003, 1004] {
            inspector_data.insert(entity_id, EntityPropertyMap::with_defaults(entity_id));
        }
        let transforms_2d = default_transforms_2d();

        let hierarchy = SceneHierarchy::default();
        let asset_browser = AssetBrowserState::default();
        let world_graph = EditorWorldGraphState::default();
        let mut creator_catalog = CreatorCatalog::default();
        if let Some(asset) = asset_browser
            .assets
            .iter()
            .find(|asset| asset.id == "hero_character")
        {
            creator_catalog.create_object(
                "player-avatar",
                Some(1001),
                hierarchy.entity_label(1001),
                Some(asset),
            );
        }

        Self {
            project_name: "Unnamed Project".to_string(),
            active_panel: EditorPanel::Viewport,
            layout: DockLayout::default(),
            selected_entity: Some(1001),
            run_state: PlayState::Stopped,
            viewport_mode: ViewportMode::TwoD,
            viewport_3d: Viewport3DState::default(),
            hierarchy,
            inspector_data,
            transforms_2d,
            console_output: vec![
                "prompt-or-die editor launched".to_string(),
                "dock: hierarchy(left), inspector(right), console(bottom)".to_string(),
            ],
            asset_browser,
            behavior_tree: BehaviorTree::default(),
            fsm: FiniteStateMachine::default(),
            llm_agent_config: LlmAgentConfig::default(),
            telemetry: TelemetryPanelState::default(),
            spacetime_dashboard: SpacetimeDashboardState::default(),
            world_graph,
            creator_catalog,
            latest_replay: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorSnapshotExport {
    pub project_name: String,
    pub active_panel: EditorPanel,
    pub selected_entity: Option<u64>,
    pub selected_asset: Option<String>,
    pub hierarchy: SceneHierarchy,
    pub selected_entity_properties: Option<HashMap<String, String>>,
    pub selected_entity_transform: Option<Transform2D>,
    pub selected_entity_world_binding: Option<EditorEntityWorldBinding>,
    pub selected_entity_trajectory: Vec<TrajectorySample>,
    pub selected_entity_debug_summary: Option<SelectedEntityDebugSummary>,
    pub assets: Vec<EditorAsset>,
    pub world_graph: EditorWorldGraphState,
    pub creator_catalog: CreatorCatalog,
    pub behavior_tree: BehaviorTree,
    pub fsm: FiniteStateMachine,
    pub latest_tick_telemetry: Option<TickTelemetryFrame>,
    pub spacetime_dashboard: SpacetimeDashboardState,
    pub latest_replay: Option<ReplayFile>,
}

impl EditorSnapshotExport {
    fn from_state(state: &EditorState) -> Self {
        let selected_entity_properties = state
            .selected_entity
            .and_then(|entity_id| state.inspector_data.get(&entity_id))
            .map(|properties| properties.entries.clone());
        let selected_entity_transform = state
            .selected_entity
            .and_then(|entity_id| state.transforms_2d.get(&entity_id))
            .cloned();
        let selected_entity_world_binding = state
            .selected_entity
            .and_then(|entity_id| state.world_graph.binding_for_entity(entity_id))
            .cloned();
        let selected_entity_trajectory = state
            .selected_entity
            .map(|entity_id| state.telemetry.trajectory_for_entity(entity_id))
            .unwrap_or_default();
        let selected_entity_debug_summary = state
            .selected_entity
            .and_then(|entity_id| state.spacetime_dashboard.debug_summary_for_entity(entity_id));

        Self {
            project_name: state.project_name.clone(),
            active_panel: state.active_panel,
            selected_entity: state.selected_entity,
            selected_asset: state.asset_browser.selected.clone(),
            hierarchy: state.hierarchy.clone(),
            selected_entity_properties,
            selected_entity_transform,
            selected_entity_world_binding,
            selected_entity_trajectory,
            selected_entity_debug_summary,
            assets: state.asset_browser.assets.clone(),
            world_graph: state.world_graph.clone(),
            creator_catalog: state.creator_catalog.clone(),
            behavior_tree: state.behavior_tree.clone(),
            fsm: state.fsm.clone(),
            latest_tick_telemetry: state.telemetry.latest().cloned(),
            spacetime_dashboard: state.spacetime_dashboard.clone(),
            latest_replay: state.latest_replay.clone(),
        }
    }

    pub fn to_toon_document(&self) -> String {
        encode_toon_document("editor_world_snapshot", self)
    }
}

#[derive(Clone, Debug)]
pub struct PodEditorApp {
    state: EditorState,
    history: EditorHistory,
    project_file_path: String,
    behavior_tree_new_node_name: String,
    fsm_new_state: String,
    fsm_from_state: String,
    fsm_to_state: String,
    fsm_trigger: String,
}

impl Default for PodEditorApp {
    fn default() -> Self {
        Self {
            state: EditorState::default(),
            history: EditorHistory::new(),
            project_file_path: "prompt-or-die.scene.json".to_string(),
            behavior_tree_new_node_name: "New Node".to_string(),
            fsm_new_state: "state".to_string(),
            fsm_from_state: "idle".to_string(),
            fsm_to_state: "idle".to_string(),
            fsm_trigger: "trigger".to_string(),
        }
    }
}

impl PodEditorApp {
    pub fn with_project_name(project_name: impl Into<String>) -> Self {
        Self {
            state: EditorState {
                project_name: project_name.into(),
                ..EditorState::default()
            },
            ..Self::default()
        }
    }

    pub fn active_panel(&self) -> EditorPanel {
        self.state.active_panel
    }

    pub fn set_active_panel(&mut self, panel: EditorPanel) {
        self.history.remember(self.state.clone());
        self.state.active_panel = panel;
    }

    pub fn selected_entity(&self) -> Option<u64> {
        self.state.selected_entity
    }

    pub fn set_selected_entity(&mut self, entity_id: Option<u64>) {
        self.history.remember(self.state.clone());
        if let Some(id) = entity_id {
            if self.state.hierarchy.contains_entity(id) {
                self.state.selected_entity = Some(id);
                self.state
                    .inspector_data
                    .entry(id)
                    .or_insert_with(|| EntityPropertyMap::with_defaults(id));
                self.state
                    .transforms_2d
                    .entry(id)
                    .or_insert_with(|| Transform2D::with_defaults(id));
                self.push_console(format!("Selected entity #{id}"));
            } else {
                self.state.selected_entity = None;
            }
        } else {
            self.state.selected_entity = None;
        }
    }

    pub fn set_dock_region(&mut self, panel: EditorPanel, region: DockRegion) {
        if panel.supports_dock() {
            self.history.remember(self.state.clone());
            self.state.layout.set_region(panel, region);
            self.push_console(format!("Docked {} to {}", panel.label(), region.label()));
        }
    }

    pub fn selected_entity_properties(&mut self) -> Option<&mut HashMap<String, String>> {
        self.state.selected_entity.and_then(|entity_id| {
            self.state
                .inspector_data
                .get_mut(&entity_id)
                .map(|props| &mut props.entries)
        })
    }

    pub fn viewport_mode(&self) -> ViewportMode {
        self.state.viewport_mode
    }

    pub fn set_viewport_mode(&mut self, mode: ViewportMode) {
        self.history.remember(self.state.clone());
        self.state.viewport_mode = mode;
        self.push_console(format!("Viewport mode: {}", mode.label()));
    }

    pub fn run_state(&self) -> PlayState {
        self.state.run_state
    }

    pub fn play(&mut self) {
        if self.state.run_state == PlayState::Playing {
            return;
        }
        self.history.remember(self.state.clone());
        self.state.run_state = PlayState::Playing;
        self.state.spacetime_dashboard.apply_connect(true);
        self.state.spacetime_dashboard.record_reducer_call("play");
        self.push_console("Simulation running".to_string());
    }

    pub fn pause(&mut self) {
        if self.state.run_state != PlayState::Playing {
            return;
        }
        self.history.remember(self.state.clone());
        self.state.run_state = PlayState::Paused;
        self.state.spacetime_dashboard.record_reducer_call("pause");
        self.push_console("Simulation paused".to_string());
    }

    pub fn stop(&mut self) {
        self.history.remember(self.state.clone());
        self.state.run_state = PlayState::Stopped;
        self.state.spacetime_dashboard.apply_connect(false);
        self.state.spacetime_dashboard.record_reducer_call("stop");
        self.push_console("Simulation stopped".to_string());
    }

    pub fn toggle_play_pause(&mut self) {
        match self.state.run_state {
            PlayState::Stopped => self.play(),
            PlayState::Playing => self.pause(),
            PlayState::Paused => self.play(),
        }
    }

    pub fn selected_asset_label(&self) -> Option<&str> {
        self.state.asset_browser.selected.as_deref()
    }

    fn selected_asset(&self) -> Option<&EditorAsset> {
        let selected = self.state.asset_browser.selected.as_deref()?;
        self.state
            .asset_browser
            .assets
            .iter()
            .find(|asset| asset.id == selected)
    }

    pub fn set_selected_asset(&mut self, asset_id: impl Into<String>) -> bool {
        let asset_id = asset_id.into();
        if !self
            .state
            .asset_browser
            .assets
            .iter()
            .any(|asset| asset.id == asset_id)
        {
            return false;
        }

        self.history.remember(self.state.clone());
        self.state.asset_browser.select(asset_id.clone());
        self.push_console(format!("Selected asset {asset_id}"));
        true
    }

    pub fn create_model_from_asset_id(&mut self, asset_id: &str) -> Option<String> {
        let asset = self
            .state
            .asset_browser
            .assets
            .iter()
            .find(|asset| asset.id == asset_id)
            .cloned()?;

        self.history.remember(self.state.clone());
        let model_id = self.state.creator_catalog.ensure_model_for_asset(&asset);
        self.push_console(format!("Created model {model_id} from {}", asset.label));
        Some(model_id)
    }

    pub fn create_object_from_selection(&mut self, name: impl Into<String>) -> Option<String> {
        let name = name.into();
        if name.trim().is_empty() {
            return None;
        }

        let selected_entity = self.state.selected_entity;
        let entity_label = selected_entity.and_then(|entity_id| {
            self.state
                .hierarchy
                .entity_label(entity_id)
                .map(str::to_string)
        });
        let selected_asset = self.selected_asset().cloned();

        self.history.remember(self.state.clone());
        let object_id = self.state.creator_catalog.create_object(
            name.clone(),
            selected_entity,
            entity_label.as_deref(),
            selected_asset.as_ref(),
        );
        self.push_console(format!("Created object {name} ({object_id})"));
        Some(object_id)
    }

    pub fn create_monster_from_selection(
        &mut self,
        name: impl Into<String>,
        temperament: CreatureTemperament,
    ) -> Option<String> {
        let name = name.into();
        if name.trim().is_empty() {
            return None;
        }

        let selected_entity = self.state.selected_entity;
        let entity_label = selected_entity.and_then(|entity_id| {
            self.state
                .hierarchy
                .entity_label(entity_id)
                .map(str::to_string)
        });
        let selected_asset = self.selected_asset().cloned();
        let creature = CreatureIdentity {
            species_id: creator_slug(&name),
            species_name: name.clone(),
            temperament,
            ..CreatureIdentity::default()
        };

        self.history.remember(self.state.clone());
        let monster_id = self.state.creator_catalog.create_monster(
            name.clone(),
            selected_entity,
            entity_label.as_deref(),
            selected_asset.as_ref(),
            creature,
        );
        self.push_console(format!("Created monster {name} ({monster_id})"));
        Some(monster_id)
    }

    pub fn associate_creator_content(
        &mut self,
        from_id: impl Into<String>,
        from_kind: CreatorNodeKind,
        to_id: impl Into<String>,
        to_kind: CreatorNodeKind,
        relation: CreatorAssociationKind,
        label: impl Into<String>,
    ) {
        let from_id = from_id.into();
        let to_id = to_id.into();
        let label = label.into();

        self.history.remember(self.state.clone());
        self.state.creator_catalog.associate(
            from_id.clone(),
            from_kind,
            to_id.clone(),
            to_kind,
            relation,
            label.clone(),
        );
        self.push_console(format!(
            "Associated {from_id} -> {to_id} ({})",
            relation.label()
        ));
    }

    pub fn export_snapshot_toon_document(&self) -> String {
        EditorSnapshotExport::from_state(&self.state).to_toon_document()
    }

    pub fn selected_entity_debug_summary(&self) -> Option<SelectedEntityDebugSummary> {
        self.state
            .selected_entity
            .and_then(|entity_id| self.state.spacetime_dashboard.debug_summary_for_entity(entity_id))
    }

    pub fn import_replay_toon_document(&mut self, document: &str) -> Result<(), String> {
        let replay: ReplayFile = decode_toon_document(document, "replay_file")?;
        let telemetry_windows = replay.telemetry_windows.clone();
        let replay_name = replay.header.name.clone();

        self.history.remember(self.state.clone());
        self.state.latest_replay = Some(replay);
        if !telemetry_windows.is_empty() {
            self.state
                .telemetry
                .replace_timeline(telemetry_windows.clone());
            if let Some(last_frame) = telemetry_windows.last() {
                self.state
                    .spacetime_dashboard
                    .record_tick_telemetry(last_frame);
            }
        }
        self.push_console(format!("Imported replay {replay_name}"));
        Ok(())
    }

    pub fn import_shard_incident_toon_document(&mut self, document: &str) -> Result<(), String> {
        let summary: ShardIncidentSummary =
            decode_toon_document(document, "shard_incident_summary")?;
        let shard_id = summary.shard_id.clone();

        self.history.remember(self.state.clone());
        self.state
            .spacetime_dashboard
            .apply_incident_summary(summary);
        self.push_console(format!("Imported shard incident summary for {shard_id}"));
        Ok(())
    }

    pub fn import_agent_tool_call_toon_document(&mut self, document: &str) -> Result<(), String> {
        let event: AgentToolCallEvent = decode_toon_document(document, "agent_tool_call_event")?;
        let agent_entity_id = event.agent_entity_id;

        self.history.remember(self.state.clone());
        self.state.spacetime_dashboard.apply_tool_call_event(event);
        self.push_console(format!(
            "Imported live tool-call event for entity {agent_entity_id}"
        ));
        Ok(())
    }

    pub fn import_agent_tick_rollup_toon_document(&mut self, document: &str) -> Result<(), String> {
        let rollup: AgentTickRollup = decode_toon_document(document, "agent_tick_rollup")?;
        let agent_entity_id = rollup.agent_entity_id;

        self.history.remember(self.state.clone());
        self.state.spacetime_dashboard.apply_tick_rollup(rollup);
        self.push_console(format!(
            "Imported telemetry rollup for entity {agent_entity_id}"
        ));
        Ok(())
    }

    pub fn import_focused_entity_debug_toon_document(
        &mut self,
        document: &str,
    ) -> Result<(), String> {
        let summary: FocusedEntityDebugSummary =
            decode_toon_document(document, "focused_entity_debug_summary")?;
        let entity_id = summary.entity_id;

        self.history.remember(self.state.clone());
        self.state
            .spacetime_dashboard
            .apply_focused_entity_debug_summary(summary);
        self.push_console(format!(
            "Imported focused debug summary for entity {entity_id}"
        ));
        Ok(())
    }

    pub fn import_world_population_toon_document(
        &mut self,
        document: &str,
    ) -> Result<(), String> {
        let population: WorldPopulationState =
            decode_toon_document(document, "world_population_state")?;
        let tick = population.tick;

        self.history.remember(self.state.clone());
        self.state
            .spacetime_dashboard
            .apply_population_state(population);
        self.push_console(format!("Imported world population state @ tick {tick}"));
        Ok(())
    }

    pub fn import_shard_transport_toon_document(
        &mut self,
        document: &str,
    ) -> Result<(), String> {
        let summary: ShardTransportSummary =
            decode_toon_document(document, "shard_transport_summary")?;
        let tick = summary.latest_tick;
        let client_count = summary.client_count;

        self.history.remember(self.state.clone());
        self.state
            .spacetime_dashboard
            .apply_transport_summary(summary);
        self.push_console(format!(
            "Imported shard transport summary @ tick {tick} for {client_count} clients"
        ));
        Ok(())
    }

    pub fn import_live_debug_toon_document(&mut self, document: &str) -> Result<(), String> {
        let document_type = decode_toon_value(document)?
            .get("document_type")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "Missing TOON document_type".to_string())?
            .to_string();

        match document_type.as_str() {
            "versioned_tick_telemetry" => {
                let telemetry: VersionedTickTelemetry =
                    decode_toon_document(document, "versioned_tick_telemetry")?;
                self.history.remember(self.state.clone());
                self.record_tick_telemetry(telemetry.payload);
                self.push_console("Imported live tick telemetry".to_string());
                Ok(())
            }
            "tick_telemetry_frame" => {
                let telemetry: TickTelemetryFrame =
                    decode_toon_document(document, "tick_telemetry_frame")?;
                self.history.remember(self.state.clone());
                self.record_tick_telemetry(telemetry);
                self.push_console("Imported live tick telemetry".to_string());
                Ok(())
            }
            "agent_tool_call_event" => self.import_agent_tool_call_toon_document(document),
            "agent_tick_rollup" => self.import_agent_tick_rollup_toon_document(document),
            "focused_entity_debug_summary" => {
                self.import_focused_entity_debug_toon_document(document)
            }
            "shard_transport_summary" => self.import_shard_transport_toon_document(document),
            "world_population_state" => self.import_world_population_toon_document(document),
            "replay_file" => {
                let replay: ReplayFile = decode_toon_document(document, "replay_file")?;
                let replay_name = replay.header.name.clone();
                let telemetry_windows = replay.telemetry_windows.clone();

                self.history.remember(self.state.clone());
                self.state.latest_replay = Some(replay);
                if self.state.telemetry.latest().is_none() && !telemetry_windows.is_empty() {
                    self.state
                        .telemetry
                        .replace_timeline(telemetry_windows.clone());
                    if let Some(last_frame) = telemetry_windows.last() {
                        self.state
                            .spacetime_dashboard
                            .record_tick_telemetry(last_frame);
                    }
                }
                self.push_console(format!("Streamed live replay {replay_name}"));
                Ok(())
            }
            "shard_incident_summary" => self.import_shard_incident_toon_document(document),
            _ => Err(format!(
                "Unsupported live debug document type `{document_type}`"
            )),
        }
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo(&mut self) {
        if self.history.undo(&mut self.state) {
            self.push_console("Undo performed".to_string());
        }
    }

    pub fn redo(&mut self) {
        if self.history.redo(&mut self.state) {
            self.push_console("Redo performed".to_string());
        }
    }

    pub fn set_project_file_path(&mut self, path: impl Into<String>) {
        self.project_file_path = path.into();
    }

    pub fn project_file_path(&self) -> &str {
        &self.project_file_path
    }

    pub fn save_project(&self, path: impl AsRef<Path>) -> io::Result<()> {
        let serialized = serde_json::to_string_pretty(&self.state).map_err(|error| {
            io::Error::new(io::ErrorKind::Other, format!("serialize failed: {error}"))
        })?;
        fs::write(path, serialized)
    }

    pub fn load_project(&mut self, path: impl AsRef<Path>) -> io::Result<()> {
        let content = fs::read_to_string(path)?;
        let loaded: EditorState = serde_json::from_str(&content).map_err(|error| {
            io::Error::new(io::ErrorKind::Other, format!("deserialize failed: {error}"))
        })?;
        self.state = loaded;
        self.refresh_world_graph_bindings();
        self.history.clear();
        self.push_console("Project loaded".to_string());
        Ok(())
    }

    pub fn import_asset_by_path(&mut self, path: impl Into<String>) {
        let path = path.into();
        if path.trim().is_empty() {
            return;
        }
        self.history.remember(self.state.clone());
        self.state.asset_browser.import_asset(path.clone());
        self.push_console(format!("Imported {path}"));
    }

    pub fn toggle_selected_asset_enabled(&mut self) {
        if let Some(selected) = self.state.asset_browser.selected.clone() {
            self.history.remember(self.state.clone());
            self.state.asset_browser.toggle_asset_enabled(&selected);
            self.push_console(format!("Toggled asset {selected}"));
        }
    }

    pub fn set_behavior_tree_selected_node(&mut self, node_id: Option<u32>) {
        self.history.remember(self.state.clone());
        self.state.behavior_tree.set_selected_node(node_id);
    }

    pub fn add_behavior_node(&mut self, kind: BehaviorTreeNodeKind) {
        self.history.remember(self.state.clone());
        let parent = self.state.behavior_tree.selected_node;
        let name = self.behavior_tree_new_node_name.clone();
        let _ = self.state.behavior_tree.add_node(parent, &name, kind);
        self.push_console(format!("Added behavior node {name}"));
    }

    pub fn remove_behavior_node(&mut self, node_id: u32) {
        self.history.remember(self.state.clone());
        self.state.behavior_tree.remove_node(node_id);
        self.push_console(format!("Removed behavior node #{node_id}"));
    }

    pub fn set_behavior_node_name(&mut self, node_id: u32, name: String) {
        self.history.remember(self.state.clone());
        self.state.behavior_tree.set_node_name(node_id, name);
    }

    pub fn set_behavior_node_kind(&mut self, node_id: u32, kind: BehaviorTreeNodeKind) {
        self.history.remember(self.state.clone());
        self.state.behavior_tree.set_node_kind(node_id, kind);
    }

    pub fn add_fsm_state(&mut self, state: impl Into<String>) -> bool {
        let state = state.into();
        if self.state.fsm.states.iter().any(|value| value == &state) {
            return false;
        }
        self.history.remember(self.state.clone());
        self.state.fsm.add_state(&state);
        true
    }

    pub fn remove_fsm_state(&mut self, state: &str) {
        self.history.remember(self.state.clone());
        self.state.fsm.remove_state(state);
    }

    pub fn add_fsm_transition(&mut self, from: String, to: String, on: String) -> bool {
        if !self.state.fsm.states.contains(&from) || !self.state.fsm.states.contains(&to) {
            return false;
        }
        self.history.remember(self.state.clone());
        self.state.fsm.add_transition(from, to, on)
    }

    pub fn remove_fsm_transition(&mut self, index: usize) {
        self.history.remember(self.state.clone());
        self.state.fsm.remove_transition(index);
    }

    pub fn set_llm_model(&mut self, value: impl Into<String>) {
        self.history.remember(self.state.clone());
        self.state.llm_agent_config.model = value.into();
    }

    pub fn set_llm_system_prompt(&mut self, value: impl Into<String>) {
        self.history.remember(self.state.clone());
        self.state.llm_agent_config.system_prompt = value.into();
    }

    pub fn set_spacetime_reducer_call(&mut self, name: impl Into<String>) {
        self.history.remember(self.state.clone());
        self.state
            .spacetime_dashboard
            .record_reducer_call(name.into());
    }

    pub fn record_tick_telemetry(&mut self, frame: TickTelemetryFrame) {
        self.state.telemetry.record_tick(frame.clone());
        self.state.spacetime_dashboard.record_tick_telemetry(&frame);
    }

    pub fn selected_entity_transform(&mut self) -> Option<&mut Transform2D> {
        self.state
            .selected_entity
            .and_then(|entity_id| self.state.transforms_2d.get_mut(&entity_id))
    }

    pub fn sync_world_region(&mut self, region: WorldRegionDefinition) {
        self.history.remember(self.state.clone());
        if let Some(existing) = self
            .state
            .world_graph
            .regions
            .iter_mut()
            .find(|existing| existing.region_id == region.region_id)
        {
            *existing = region.clone();
        } else {
            self.state.world_graph.regions.push(region.clone());
        }
        self.refresh_world_graph_bindings();
        self.push_console(format!("Synced world region {}", region.region_id));
    }

    pub fn sync_world_chunk(&mut self, chunk: WorldChunkDefinition) {
        self.history.remember(self.state.clone());
        if let Some(existing) = self
            .state
            .world_graph
            .chunks
            .iter_mut()
            .find(|existing| existing.chunk_key == chunk.chunk_key)
        {
            *existing = chunk.clone();
        } else {
            self.state.world_graph.chunks.push(chunk.clone());
        }
        self.refresh_world_graph_bindings();
        self.push_console(format!("Synced world chunk {}", chunk.chunk_key));
    }

    pub fn sync_world_quest_graph(&mut self, quest_graph: QuestStateGraph) {
        self.history.remember(self.state.clone());
        if let Some(existing) = self
            .state
            .world_graph
            .quest_graphs
            .iter_mut()
            .find(|existing| existing.quest_id == quest_graph.quest_id)
        {
            *existing = quest_graph.clone();
        } else {
            self.state
                .world_graph
                .quest_graphs
                .push(quest_graph.clone());
        }
        self.refresh_world_graph_bindings();
        self.push_console(format!("Synced quest graph {}", quest_graph.quest_id));
    }

    pub fn sync_world_faction_track(&mut self, faction_track: FactionReputationTrack) {
        self.history.remember(self.state.clone());
        if let Some(existing) = self
            .state
            .world_graph
            .faction_tracks
            .iter_mut()
            .find(|existing| existing.faction_id == faction_track.faction_id)
        {
            *existing = faction_track.clone();
        } else {
            self.state
                .world_graph
                .faction_tracks
                .push(faction_track.clone());
        }
        self.refresh_world_graph_bindings();
        self.push_console(format!("Synced faction track {}", faction_track.faction_id));
    }

    pub fn sync_world_encounter_table(&mut self, encounter_table: RegionEncounterTable) {
        self.history.remember(self.state.clone());
        if let Some(existing) = self
            .state
            .world_graph
            .encounter_tables
            .iter_mut()
            .find(|existing| existing.table_id == encounter_table.table_id)
        {
            *existing = encounter_table.clone();
        } else {
            self.state
                .world_graph
                .encounter_tables
                .push(encounter_table.clone());
        }
        self.refresh_world_graph_bindings();
        self.push_console(format!(
            "Synced encounter table {}",
            encounter_table.table_id
        ));
    }

    pub fn adjust_world_encounter_table_ambient_cap(
        &mut self,
        table_id: &str,
        delta: i16,
    ) -> bool {
        let Some(index) = self
            .state
            .world_graph
            .encounter_tables
            .iter()
            .position(|table| table.table_id == table_id)
        else {
            return false;
        };

        let current = self.state.world_graph.encounter_tables[index].ambient_cap;
        let next = if delta >= 0 {
            current.saturating_add(delta as u16)
        } else {
            current.saturating_sub((-delta) as u16).max(1)
        };
        if next == current {
            return false;
        }

        self.history.remember(self.state.clone());
        self.state.world_graph.encounter_tables[index].ambient_cap = next;
        self.refresh_world_graph_bindings();
        self.push_console(format!(
            "Adjusted encounter table {} ambient cap {} -> {}",
            table_id, current, next
        ));
        true
    }

    fn encounter_balance_previews(&self) -> Vec<EncounterBalancePreview> {
        let mut previews = self
            .state
            .world_graph
            .encounter_tables
            .iter()
            .map(|table| {
                let matching_chunks = self
                    .state
                    .spacetime_dashboard
                    .population_state
                    .chunks
                    .iter()
                    .filter(|chunk| {
                        chunk.encounter_table_ids
                            .iter()
                            .any(|table_id| table_id == &table.table_id)
                    })
                    .collect::<Vec<_>>();
                let attached_chunk_count = matching_chunks.len() as u32;
                let effective_cap = u32::from(table.ambient_cap) * attached_chunk_count.max(1);
                let active_spawned_actors = matching_chunks
                    .iter()
                    .map(|chunk| breakdown_active_spawned_actors(chunk.counts))
                    .sum::<u32>();
                let hot_chunk_count = matching_chunks
                    .iter()
                    .filter(|chunk| chunk.population_pressure >= 0.75)
                    .count() as u32;
                let pending_respawns = matching_chunks
                    .iter()
                    .map(|chunk| chunk.pending_respawns)
                    .sum::<u32>();
                let next_respawn_tick = matching_chunks
                    .iter()
                    .filter_map(|chunk| chunk.next_respawn_tick)
                    .min();
                let spawn_budget_remaining =
                    effective_cap.saturating_sub(active_spawned_actors);
                let population_pressure = if effective_cap == 0 {
                    0.0
                } else {
                    active_spawned_actors as f32 / effective_cap as f32
                };

                EncounterBalancePreview {
                    table_id: table.table_id.clone(),
                    biome_id: table.biome_id.clone(),
                    spawn_group: table.spawn_group.clone(),
                    ambient_cap_per_chunk: table.ambient_cap,
                    effective_cap,
                    attached_chunk_count,
                    hot_chunk_count,
                    active_spawned_actors,
                    spawn_budget_remaining,
                    pending_respawns,
                    next_respawn_tick,
                    population_pressure,
                    suggested_cap_delta: suggested_encounter_cap_delta(
                        population_pressure,
                        spawn_budget_remaining,
                        pending_respawns,
                    ),
                }
            })
            .collect::<Vec<_>>();
        previews.sort_by(|left, right| {
            right
                .population_pressure
                .partial_cmp(&left.population_pressure)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.pending_respawns.cmp(&left.pending_respawns))
                .then_with(|| left.table_id.cmp(&right.table_id))
        });
        previews
    }

    fn move_selected_entity(&mut self, dx: f32, dy: f32) {
        self.history.remember(self.state.clone());
        if let Some(transform) = self.selected_entity_transform() {
            transform.x += dx;
            transform.y += dy;
        }
        self.refresh_world_graph_bindings();
    }

    fn push_console(&mut self, line: impl Into<String>) {
        self.state.console_output.push(line.into());
        if self.state.console_output.len() > 300 {
            let _ = self.state.console_output.drain(0..50);
        }
    }

    fn refresh_world_graph_bindings(&mut self) {
        self.state
            .world_graph
            .sync_entity_bindings(&self.state.hierarchy, &self.state.transforms_2d);
    }

    fn build_dock_toolbar(&mut self, ui: &mut Ui) {
        if ui.button("Project").clicked() {
            let snapshot = self.export_snapshot_toon_document();
            self.push_console(format!("Project TOON snapshot {snapshot}"));
        }
        ui.separator();
        if ui
            .add_enabled(self.can_undo(), egui::Button::new("Undo"))
            .clicked()
        {
            self.undo();
        }
        if ui
            .add_enabled(self.can_redo(), egui::Button::new("Redo"))
            .clicked()
        {
            self.redo();
        }
        ui.separator();

        for panel in [
            EditorPanel::Hierarchy,
            EditorPanel::Inspector,
            EditorPanel::Console,
            EditorPanel::AssetBrowser,
            EditorPanel::BehaviorTree,
            EditorPanel::FiniteStateMachine,
            EditorPanel::LlmAgentConfig,
            EditorPanel::Telemetry,
            EditorPanel::SpacetimeDashboard,
        ] {
            let region = self.state.layout.region_for_panel(panel);
            let visible = self.state.layout.is_visible(panel);
            ui.menu_button(format!("{} [{}]", panel.label(), region.label()), |menu| {
                for &target in &[
                    DockRegion::Left,
                    DockRegion::Right,
                    DockRegion::Bottom,
                    DockRegion::Floating,
                ] {
                    if menu
                        .selectable_label(region == target, target.label())
                        .clicked()
                    {
                        self.set_dock_region(panel, target);
                    }
                }
            });
            if ui.button(if visible { "Hide" } else { "Show" }).clicked() {
                self.history.remember(self.state.clone());
                self.state.layout.toggle_visibility(panel);
                let action = if visible { "hidden" } else { "shown" };
                self.push_console(format!("Panel {} {}", panel.label(), action));
            }
            ui.separator();
        }
    }

    fn build_play_toolbar(&mut self, ui: &mut Ui) {
        if ui.button("Play").clicked() {
            self.play();
        }
        if ui.button("Pause").clicked() {
            self.pause();
        }
        if ui.button("Stop").clicked() {
            self.stop();
        }
        if ui.button("Play/Pause").clicked() {
            self.toggle_play_pause();
        }
        ui.separator();
        ui.label(format!("State: {}", self.run_state().label()));
        ui.separator();
    }

    fn build_project_toolbar(&mut self, ui: &mut Ui) {
        ui.label("Project file:");
        ui.text_edit_singleline(&mut self.project_file_path);
        if ui.button("Save").clicked() {
            match self.save_project(&self.project_file_path) {
                Ok(()) => self.push_console(format!("Saved {}", self.project_file_path)),
                Err(error) => self.push_console(format!("Save failed: {error}")),
            }
        }
        if ui.button("Load").clicked() {
            let path = self.project_file_path.clone();
            match self.load_project(&path) {
                Ok(()) => self.push_console(format!("Loaded {}", self.project_file_path)),
                Err(error) => self.push_console(format!("Load failed: {error}")),
            }
        }
        ui.separator();
        ui.label(format!(
            "Dashboard reducer calls: {}",
            self.state.spacetime_dashboard.reducer_calls
        ));
    }

    fn render_hierarchy(&mut self, ui: &mut Ui) {
        ui.heading("Entity Hierarchy");
        self.state
            .hierarchy
            .render(ui, &mut self.state.selected_entity);
    }

    fn render_inspector(&mut self, ui: &mut Ui) {
        ui.heading("Inspector");
        let maybe_props = self.state.selected_entity.and_then(|entity_id| {
            Some((
                entity_id,
                self.state
                    .inspector_data
                    .entry(entity_id)
                    .or_insert_with(|| EntityPropertyMap::with_defaults(entity_id)),
            ))
        });
        if let Some((entity_id, props)) = maybe_props {
            ui.label(format!("Editing entity #{entity_id}"));
            ui.separator();
            for (key, value) in props.entries.iter_mut() {
                ui.horizontal(|ui| {
                    ui.label(format!("{key}:"));
                    ui.text_edit_singleline(value);
                });
            }
            ui.separator();
            ui.label("World Graph");
            if let Some(binding) = self.state.world_graph.binding_for_entity(entity_id) {
                ui.label(format!(
                    "Region: {}",
                    binding.region_name.as_deref().unwrap_or("Unassigned")
                ));
                ui.label(format!(
                    "Chunk: {}",
                    binding.chunk_key.as_deref().unwrap_or("Unassigned")
                ));
                ui.label(format!(
                    "Faction track: {}",
                    binding.faction_track_id.as_deref().unwrap_or("None")
                ));
                ui.label(format!(
                    "Encounter table: {}",
                    binding.encounter_table_id.as_deref().unwrap_or("None")
                ));
                ui.label(format!(
                    "Quest graphs: {}",
                    if binding.quest_graph_ids.is_empty() {
                        "none".to_string()
                    } else {
                        binding.quest_graph_ids.join(", ")
                    }
                ));
            } else {
                ui.label("No streamed world binding for the selected entity.");
            }
        } else {
            ui.label("No entity selected.");
            ui.label("Select an entity from the hierarchy to inspect.");
        }
    }

    fn render_behavior_node(
        &self,
        ui: &mut Ui,
        node_id: u32,
        depth: usize,
        selected_node: Option<u32>,
    ) -> Option<u32> {
        let Some(node) = self.state.behavior_tree.nodes.get(&node_id) else {
            return None;
        };

        let prefix = "  ".repeat(depth);
        let active = selected_node == Some(node_id);
        let mut selected = None;
        if ui
            .selectable_label(
                active,
                format!("{prefix}[#{}] {} {}", node.id, node.kind.label(), node.name),
            )
            .clicked()
        {
            selected = Some(node_id);
        }
        for child_id in node.children.clone() {
            if let Some(inner) = self.render_behavior_node(ui, child_id, depth + 1, selected_node) {
                selected = Some(inner);
            }
        }
        selected
    }

    fn render_behavior_tree_editor(&mut self, ui: &mut Ui) {
        ui.heading("Behavior Tree");
        if let Some(entity_id) = self.state.selected_entity {
            ui.label(format!("Editing behavior tree for entity #{entity_id}"));
        } else {
            ui.label("No entity selected; editing global tree.");
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.behavior_tree_new_node_name);
            if ui.button("Sequence").clicked() {
                self.add_behavior_node(BehaviorTreeNodeKind::Sequence);
            }
            if ui.button("Selector").clicked() {
                self.add_behavior_node(BehaviorTreeNodeKind::Selector);
            }
            if ui.button("Action").clicked() {
                self.add_behavior_node(BehaviorTreeNodeKind::Action);
            }
            if ui.button("Condition").clicked() {
                self.add_behavior_node(BehaviorTreeNodeKind::Condition);
            }
        });
        ui.separator();

        let selected_node = self.state.behavior_tree.selected_node;
        let roots = self.state.behavior_tree.roots.clone();
        let mut requested_selection = None;
        for root in roots {
            if let Some(node_id) = self.render_behavior_node(ui, root, 0, selected_node) {
                requested_selection = Some(node_id);
            }
        }
        if let Some(node_id) = requested_selection {
            self.set_behavior_tree_selected_node(Some(node_id));
        }

        if let Some(node_id) = selected_node {
            ui.separator();
            let current_name = self
                .state
                .behavior_tree
                .nodes
                .get(&node_id)
                .map(|node| node.name.clone())
                .unwrap_or_default();
            let mut name = current_name;
            if ui.text_edit_singleline(&mut name).changed() {
                self.set_behavior_node_name(node_id, name);
            }
            let current_kind = self
                .state
                .behavior_tree
                .nodes
                .get(&node_id)
                .map(|node| node.kind);
            ui.horizontal(|ui| {
                ui.label("Type:");
                if let Some(current_kind) = current_kind {
                    for kind in [
                        BehaviorTreeNodeKind::Sequence,
                        BehaviorTreeNodeKind::Selector,
                        BehaviorTreeNodeKind::Action,
                        BehaviorTreeNodeKind::Condition,
                    ] {
                        if ui
                            .selectable_label(current_kind == kind, kind.label())
                            .clicked()
                        {
                            self.set_behavior_node_kind(node_id, kind);
                        }
                    }
                }
            });
            if ui.button("Delete selected node").clicked() {
                self.remove_behavior_node(node_id);
            }
        }
    }

    fn render_fsm_editor(&mut self, ui: &mut Ui) {
        ui.heading("FSM Editor");
        if let Some(state_name) = self.state.fsm.selected_state.clone() {
            ui.label(format!("Current state: {state_name}"));
        }
        ui.separator();
        ui.label("States");
        for state in self.state.fsm.states.clone() {
            let selected = self.state.fsm.selected_state.as_deref() == Some(&state);
            if ui.selectable_label(selected, &state).clicked() {
                self.history.remember(self.state.clone());
                self.state.fsm.selected_state = Some(state);
            }
        }
        ui.separator();
        ui.text_edit_singleline(&mut self.fsm_new_state);
        if ui.button("Add state").clicked() {
            if self.add_fsm_state(self.fsm_new_state.clone()) {
                self.push_console(format!("Added state {}", self.fsm_new_state));
            }
        }
        ui.separator();
        if let Some(state) = self.state.fsm.selected_state.clone() {
            ui.label("Selected state transitions");
            let mut transition_index_to_remove = None;
            for (index, transition) in self.state.fsm.transitions.iter().enumerate() {
                if transition.from == state && transition_index_to_remove.is_none() {
                    ui.horizontal(|ui| {
                        ui.label(format!(
                            "{} --{}--> {}",
                            transition.from, transition.on, transition.to
                        ));
                        if ui.button("remove").clicked() {
                            transition_index_to_remove = Some(index);
                        }
                    });
                }
            }
            if let Some(index) = transition_index_to_remove {
                self.remove_fsm_transition(index);
            }
        }
        ui.separator();
        ui.label("Add transition");
        if self.state.fsm.states.is_empty() {
            ui.label("Add at least one state first.");
            return;
        }
        if self.fsm_from_state.is_empty() {
            self.fsm_from_state = self.state.fsm.states[0].clone();
        }
        if self.fsm_to_state.is_empty() {
            self.fsm_to_state = self.state.fsm.states[0].clone();
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("fsm_from")
                .selected_text(self.fsm_from_state.clone())
                .show_ui(ui, |ui| {
                    for state in self.state.fsm.states.clone() {
                        ui.selectable_value(&mut self.fsm_from_state, state.clone(), state);
                    }
                });
            ui.label("→");
            egui::ComboBox::from_id_salt("fsm_to")
                .selected_text(self.fsm_to_state.clone())
                .show_ui(ui, |ui| {
                    for state in self.state.fsm.states.clone() {
                        ui.selectable_value(&mut self.fsm_to_state, state.clone(), state);
                    }
                });
        });
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut self.fsm_trigger);
            if ui.button("Add transition").clicked() {
                if self.add_fsm_transition(
                    self.fsm_from_state.clone(),
                    self.fsm_to_state.clone(),
                    self.fsm_trigger.clone(),
                ) {
                    self.push_console("Added FSM transition".to_string());
                }
            }
        });
        let mut transition_index_to_remove = None;
        for (index, transition) in self.state.fsm.transitions.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!(
                    "{} --{}--> {}",
                    transition.from, transition.on, transition.to
                ));
                if ui.button("remove").clicked() {
                    transition_index_to_remove = Some(index);
                }
            });
        }
        if let Some(index) = transition_index_to_remove {
            self.remove_fsm_transition(index);
        }
    }

    fn render_llm_agent_panel(&mut self, ui: &mut Ui) {
        ui.heading("LLM Agent Config");
        if ui
            .checkbox(&mut self.state.llm_agent_config.enabled, "Enabled")
            .changed()
        {
            self.history.remember(self.state.clone());
        }
        if ui
            .text_edit_singleline(&mut self.state.llm_agent_config.model)
            .changed()
        {
            let model = self.state.llm_agent_config.model.clone();
            self.set_llm_model(model);
        }
        ui.label("System prompt:");
        let mut prompt = self.state.llm_agent_config.system_prompt.clone();
        if ui.text_edit_multiline(&mut prompt).changed() {
            self.set_llm_system_prompt(prompt);
        }
        ui.horizontal(|ui| {
            ui.label("Temperature");
            if ui
                .add(
                    egui::DragValue::new(&mut self.state.llm_agent_config.temperature)
                        .range(0.0..=2.0),
                )
                .changed()
            {
                self.history.remember(self.state.clone());
            }
            ui.label("Tool budget");
            if ui
                .add(egui::DragValue::new(
                    &mut self.state.llm_agent_config.tool_budget,
                ))
                .changed()
            {
                self.history.remember(self.state.clone());
            }
        });
        ui.horizontal(|ui| {
            ui.label("Max tokens");
            if ui
                .add(egui::DragValue::new(
                    &mut self.state.llm_agent_config.max_tokens,
                ))
                .changed()
            {
                self.history.remember(self.state.clone());
            }
            ui.label("Memory window");
            if ui
                .add(egui::DragValue::new(
                    &mut self.state.llm_agent_config.memory_window,
                ))
                .changed()
            {
                self.history.remember(self.state.clone());
            }
        });
    }

    fn render_spacetime_panel(&mut self, ui: &mut Ui) {
        ui.heading("SpacetimeDB Dashboard");
        ui.label(format!(
            "Latest tick: {}",
            self.state.spacetime_dashboard.latest_tick
        ));
        ui.label(format!(
            "Connected players: {}",
            self.state.spacetime_dashboard.connected_players
        ));
        ui.label(format!(
            "Action rejection rate: {:.1}%",
            self.state.spacetime_dashboard.action_rejection_rate * 100.0
        ));
        ui.label(format!(
            "Tool-call error rate: {:.1}%",
            self.state.spacetime_dashboard.tool_call_error_rate * 100.0
        ));
        ui.label(format!(
            "Visible/audible/messages: {}/{}/{}",
            self.state.spacetime_dashboard.visible_entity_count,
            self.state.spacetime_dashboard.audible_event_count,
            self.state.spacetime_dashboard.message_count
        ));
        ui.label(format!(
            "Last reducer call: {}",
            self.state.spacetime_dashboard.last_reducer_call
        ));
        ui.label(format!(
            "Reducer calls: {}",
            self.state.spacetime_dashboard.reducer_calls
        ));
        ui.label(format!(
            "World graph: {} regions · {} chunks · {} quests · {} factions · {} encounter tables",
            self.state.world_graph.regions.len(),
            self.state.world_graph.chunks.len(),
            self.state.world_graph.quest_graphs.len(),
            self.state.world_graph.faction_tracks.len(),
            self.state.world_graph.encounter_tables.len()
        ));
        ui.label(format!(
            "Authoritative population: {} regions · {} chunks",
            self.state.spacetime_dashboard.population_state.regions.len(),
            self.state.spacetime_dashboard.population_state.chunks.len()
        ));
        let mut hottest_chunks = self
            .state
            .spacetime_dashboard
            .population_state
            .chunks
            .iter()
            .collect::<Vec<_>>();
        hottest_chunks.sort_by(|left, right| {
            right
                .population_pressure
                .partial_cmp(&left.population_pressure)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.pending_respawns.cmp(&left.pending_respawns))
                .then_with(|| left.chunk_key.cmp(&right.chunk_key))
        });
        if !hottest_chunks.is_empty() {
            ui.label("Population heat");
            for chunk in hottest_chunks.into_iter().take(5) {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{} · {} active · respawns {}",
                        chunk.chunk_key, chunk.active_entity_count, chunk.pending_respawns
                    ));
                    ui.add(
                        ProgressBar::new((chunk.population_pressure / 1.25).clamp(0.0, 1.0))
                            .desired_width(120.0)
                            .text(format!("{:.2}", chunk.population_pressure)),
                    );
                });
            }
        }
        ui.separator();
        if let Some(entity_id) = self.state.selected_entity {
            if let Some(binding) = self.state.world_graph.binding_for_entity(entity_id) {
                if let Some(region_id) = binding.region_id.as_deref() {
                    if let Some(region) = self
                        .state
                        .spacetime_dashboard
                        .population_state
                        .regions
                        .iter()
                        .find(|region| region.region_id == region_id)
                    {
                        ui.label(format!(
                            "Selected region pop: {} active · {}/{} chunks hot · cap {} · budget {} · respawns {}{}",
                            region.active_entity_count,
                            region.active_chunk_count,
                            region.chunk_keys.len(),
                            region.ambient_population_cap,
                            region.spawn_budget_remaining,
                            region.pending_respawns,
                            region
                                .next_respawn_tick
                                .map(|tick| format!(" @ {tick}"))
                                .unwrap_or_default()
                        ));
                    }
                }
                if let Some(chunk_key) = binding.chunk_key.as_deref() {
                    if let Some(chunk) = self
                        .state
                        .spacetime_dashboard
                        .population_state
                        .chunks
                        .iter()
                        .find(|chunk| chunk.chunk_key == chunk_key)
                    {
                        ui.label(format!(
                            "Selected chunk pop: {} active · pressure {:.2} · cap {} · respawns {}{}",
                            chunk.active_entity_count,
                            chunk.population_pressure,
                            chunk.ambient_population_cap,
                            chunk.pending_respawns,
                            chunk
                                .next_respawn_tick
                                .map(|tick| format!(" @ {tick}"))
                                .unwrap_or_default()
                        ));
                    }
                }
            }
            if let Some(summary) = self.state.spacetime_dashboard.debug_summary_for_entity(entity_id)
            {
                ui.label(format!(
                    "Selected debug focus: {} tool calls · {} rollups · {:.1}ms avg tool latency",
                    summary.recent_tool_call_count,
                    summary.recent_rollup_count,
                    summary.average_tool_latency_ms
                ));
                ui.label(format!(
                    "Selected debug totals: {:.2}u moved · {} rejected · {} tool errors",
                    summary.trajectory_distance,
                    summary.rejected_actions,
                    summary.tool_errors
                ));
            }
        }
        ui.separator();
        ui.label("Per-agent trajectory distance");
        if self.state.spacetime_dashboard.agent_summaries.is_empty() {
            ui.label("No authoritative telemetry recorded yet.");
        } else {
            for summary in &self.state.spacetime_dashboard.agent_summaries {
                ui.label(format!(
                    "{} · {} · {:.2}u · {} rejected · {} tool errors",
                    summary.role,
                    summary
                        .entity_id
                        .map(|entity_id| format!("E({entity_id})"))
                        .unwrap_or_else(|| summary.agent_id.clone()),
                    summary.trajectory_distance,
                    summary.rejected_actions,
                    summary.tool_errors
                ));
            }
        }
        ui.separator();
        if let Some(entity_id) = self.state.selected_entity {
            if let Some(binding) = self.state.world_graph.binding_for_entity(entity_id) {
                ui.label(format!(
                    "Selected entity binding: {} / {}",
                    binding
                        .region_name
                        .as_deref()
                        .unwrap_or("Unassigned region"),
                    binding.chunk_key.as_deref().unwrap_or("unassigned chunk")
                ));
                ui.separator();
            }
        }
        let previews = self.encounter_balance_previews();
        if previews.is_empty() {
            ui.label("Encounter balancing: no encounter tables authored yet.");
        } else {
            ui.label("Encounter balancing");
            let selected_table_id = self
                .state
                .selected_entity
                .and_then(|entity_id| self.state.world_graph.binding_for_entity(entity_id))
                .and_then(|binding| binding.encounter_table_id.clone());
            let mut pending_adjustment: Option<(String, i16)> = None;

            if let Some(selected_table_id) = selected_table_id.as_deref() {
                if let Some(selected_preview) = previews
                    .iter()
                    .find(|preview| preview.table_id == selected_table_id)
                {
                    ui.label(format!(
                        "Selected table: {} · {} · cap {} per chunk / {} effective · pressure {:.2} · respawns {}{}",
                        selected_preview.table_id,
                        selected_preview.spawn_group,
                        selected_preview.ambient_cap_per_chunk,
                        selected_preview.effective_cap,
                        selected_preview.population_pressure,
                        selected_preview.pending_respawns,
                        selected_preview
                            .next_respawn_tick
                            .map(|tick| format!(" @ {tick}"))
                            .unwrap_or_default()
                    ));
                    ui.horizontal(|ui| {
                        if ui.button("Cap -1").clicked() {
                            pending_adjustment =
                                Some((selected_preview.table_id.clone(), -1));
                        }
                        if ui.button("Cap +1").clicked() {
                            pending_adjustment =
                                Some((selected_preview.table_id.clone(), 1));
                        }
                    });
                }
            }

            for preview in previews.iter().take(4) {
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "{} · {} chunk(s) · {} active / {} cap · budget {} · pressure {:.2} · suggest {:+}",
                        preview.table_id,
                        preview.attached_chunk_count,
                        preview.active_spawned_actors,
                        preview.effective_cap,
                        preview.spawn_budget_remaining,
                        preview.population_pressure,
                        preview.suggested_cap_delta
                    ));
                    if ui.small_button("-1").clicked() {
                        pending_adjustment = Some((preview.table_id.clone(), -1));
                    }
                    if ui.small_button("+1").clicked() {
                        pending_adjustment = Some((preview.table_id.clone(), 1));
                    }
                });
            }

            if let Some((table_id, delta)) = pending_adjustment {
                let _ = self.adjust_world_encounter_table_ambient_cap(&table_id, delta);
            }
            ui.separator();
        }
        if ui.button("Simulate reducer").clicked() {
            self.set_spacetime_reducer_call("manual_reducer");
        }
    }

    fn render_telemetry_panel(&mut self, ui: &mut Ui) {
        ui.heading("Telemetry");
        ui.label(format!(
            "Retained ticks: {} / {}",
            self.state.telemetry.timeline.len(),
            self.state.telemetry.max_ticks
        ));
        if let Some(entity_id) = self.state.selected_entity {
            ui.label(format!("Selected entity: #{entity_id}"));
            let trajectory = self.state.telemetry.trajectory_for_entity(entity_id);
            let total_distance = self
                .state
                .telemetry
                .trajectory_distance_for_entity(entity_id);
            ui.label(format!(
                "Trajectory samples: {} · distance {:.2}u",
                trajectory.len(),
                total_distance
            ));

            if let Some(agent) = self.state.telemetry.latest_agent_for_entity(entity_id) {
                let submitted = agent
                    .action_trace
                    .iter()
                    .filter(|trace| trace.stage == ActionLifecycleStage::Submitted)
                    .count();
                let executed = agent
                    .action_trace
                    .iter()
                    .filter(|trace| trace.stage == ActionLifecycleStage::Executed)
                    .count();
                let rejected = agent
                    .action_trace
                    .iter()
                    .filter(|trace| trace.stage == ActionLifecycleStage::Rejected)
                    .count();
                ui.label(format!(
                    "Actions: submitted {} · executed {} · rejected {}",
                    submitted, executed, rejected
                ));
                ui.label(format!(
                    "Observations: {} visible · {} audible · {} messages",
                    agent.visible_entity_count, agent.audible_event_count, agent.message_count
                ));
                if let Some(tool) = agent.tool_calls.last() {
                    ui.label(format!(
                        "Tool call: {} via {} · {:?} · {}ms",
                        tool.tool_name, tool.provider, tool.status, tool.latency_ms
                    ));
                    if let Some(error) = &tool.error_message {
                        ui.label(format!("Last tool error: {error}"));
                    }
                } else {
                    ui.label("Tool calls: none");
                }
                if let Some(trajectory) = &agent.trajectory {
                    ui.label(format!(
                        "Latest segment: ({:.2}, {:.2}) → ({:.2}, {:.2})",
                        trajectory.start.position.x,
                        trajectory.start.position.y,
                        trajectory.end.position.x,
                        trajectory.end.position.y
                    ));
                }
            } else if let Some(summary) =
                self.state.spacetime_dashboard.debug_summary_for_entity(entity_id)
            {
                ui.label(format!(
                    "Entity-scoped raw debug telemetry @ tick {}",
                    summary.latest_tick
                ));
                ui.label(format!(
                    "Raw docs: {} tool calls · {} rollups",
                    summary.recent_tool_call_count, summary.recent_rollup_count
                ));
                ui.label(format!(
                    "Actions: {} rejected · distance {:.2}u",
                    summary.rejected_actions, summary.trajectory_distance
                ));
                if let Some(rollup) = &summary.latest_rollup {
                    ui.label(format!(
                        "Rollup window: {}-{} · {} visible · {} audible · {} messages",
                        rollup.tick_start,
                        rollup.tick_end,
                        rollup.visible_entity_count,
                        rollup.audible_event_count,
                        rollup.message_count
                    ));
                }
                if let Some(tool) = &summary.latest_tool_call {
                    ui.label(format!(
                        "Tool call: {} via {} · {:?} · {}ms",
                        tool.trace.tool_name,
                        tool.trace.provider,
                        tool.trace.status,
                        tool.trace.latency_ms
                    ));
                    if let Some(error) = &tool.trace.error_message {
                        ui.label(format!("Last tool error: {error}"));
                    }
                } else {
                    ui.label("Tool calls: none");
                }
            } else {
                ui.label("No telemetry recorded for the selected entity yet.");
            }
        } else {
            ui.label("No entity selected.");
            ui.label("Selection stays synced with the hierarchy and inspector.");
        }
    }

    fn render_console(&mut self, ui: &mut Ui) {
        ui.heading("Console");
        ui.separator();
        for line in &self.state.console_output {
            ui.label(line);
        }
    }

    fn render_asset_browser(&mut self, ui: &mut Ui) {
        ui.heading("Asset Browser");
        ui.horizontal(|ui| {
            ui.label("Filter:");
            let is_filter_set = self.state.asset_browser.filter.is_some();
            if ui.button("All").clicked() {
                self.state.asset_browser.filter = None;
            }
            ui.selectable_value(
                &mut self.state.asset_browser.filter,
                Some(AssetKind::Mesh),
                AssetKind::Mesh.label(),
            );
            ui.selectable_value(
                &mut self.state.asset_browser.filter,
                Some(AssetKind::Texture),
                AssetKind::Texture.label(),
            );
            ui.selectable_value(
                &mut self.state.asset_browser.filter,
                Some(AssetKind::Audio),
                AssetKind::Audio.label(),
            );
            ui.selectable_value(
                &mut self.state.asset_browser.filter,
                Some(AssetKind::Script),
                AssetKind::Script.label(),
            );
            if is_filter_set {
                ui.label("(custom filter active)");
            }
        });
        ui.horizontal(|ui| {
            ui.label("Query:");
            ui.text_edit_singleline(&mut self.state.asset_browser.query);
        });
        ui.horizontal(|ui| {
            ui.label("Import path:");
            ui.text_edit_singleline(&mut self.state.asset_browser.import_path);
            if ui.button("Import").clicked() {
                let to_import = self.state.asset_browser.import_path.clone();
                self.import_asset_by_path(to_import);
            }
        });
        ui.separator();

        let selected_asset = self.state.asset_browser.selected.clone();
        let mut selection_changed = None;
        let visible_assets = self.state.asset_browser.visible_assets();
        for asset in visible_assets {
            let active = selected_asset.as_deref() == Some(asset.id.as_str());
            if ui
                .selectable_label(active, format!("{} — {}", asset.label, asset.path))
                .clicked()
            {
                selection_changed = Some(asset.id.clone());
            }
        }
        if let Some(asset_id) = selection_changed {
            self.state.asset_browser.select(asset_id);
        } else if selected_asset.is_none() && !self.state.asset_browser.assets.is_empty() {
            self.state
                .asset_browser
                .select(self.state.asset_browser.assets[0].id.clone());
        }

        ui.separator();
        if let Some(selected_id) = self.state.asset_browser.selected.clone() {
            if let Some(index) = self
                .state
                .asset_browser
                .assets
                .iter()
                .position(|asset| asset.id == selected_id)
            {
                ui.label("Selected Asset");
                let label = self.state.asset_browser.assets[index].label.clone();
                let path = self.state.asset_browser.assets[index].path.clone();
                let bytes = self.state.asset_browser.assets[index].size_bytes;
                let kind = self.state.asset_browser.assets[index].kind.label();
                ui.label(format!("Label: {}", label));
                ui.label(format!("Kind: {kind}"));
                ui.label(format!("Path: {}", path));
                ui.label(format!("Bytes: {}", bytes));
                if ui
                    .checkbox(
                        &mut self.state.asset_browser.assets[index].enabled,
                        "Enabled",
                    )
                    .changed()
                {
                    self.push_console(format!("Toggled asset {label}"));
                }
                if ui.button("Toggle enabled").clicked() {
                    self.toggle_selected_asset_enabled();
                }
            }
        }
        ui.separator();
        if ui.button("Clear selection").clicked() {
            self.history.remember(self.state.clone());
            self.state.asset_browser.clear_selection();
        }
    }

    fn render_viewport(&mut self, ui: &mut Ui) {
        ui.heading("Viewport");
        ui.label(format!("Mode: {}", self.state.viewport_mode.label()));
        ui.separator();
        match self.state.viewport_mode {
            ViewportMode::TwoD => {
                ui.label("Scene preview + entity placement gizmos.");
                if let Some(entity_id) = self.state.selected_entity {
                    let maybe_transform = self.state.transforms_2d.get(&entity_id).cloned();
                    if let Some(transform) = maybe_transform {
                        ui.label(format!("Selected entity: #{entity_id}"));
                        ui.separator();
                        ui.label("Gizmo");
                        ui.horizontal(|ui| {
                            if ui.button("←").clicked() {
                                self.move_selected_entity(-0.25, 0.0);
                            }
                            if ui.button("→").clicked() {
                                self.move_selected_entity(0.25, 0.0);
                            }
                            if ui.button("↑").clicked() {
                                self.move_selected_entity(0.0, 0.25);
                            }
                            if ui.button("↓").clicked() {
                                self.move_selected_entity(0.0, -0.25);
                            }
                        });
                        ui.separator();
                        ui.label("Translate");
                        ui.horizontal(|ui| {
                            ui.label("X:");
                            let mut x = transform.x;
                            if ui.add(egui::DragValue::new(&mut x).speed(0.1)).changed() {
                                let entry = self
                                    .state
                                    .transforms_2d
                                    .get_mut(&entity_id)
                                    .expect("selected entity transform exists");
                                entry.x = x;
                                self.refresh_world_graph_bindings();
                            }
                            ui.label("Y:");
                            let mut y = transform.y;
                            if ui.add(egui::DragValue::new(&mut y).speed(0.1)).changed() {
                                let entry = self
                                    .state
                                    .transforms_2d
                                    .get_mut(&entity_id)
                                    .expect("selected entity transform exists");
                                entry.y = y;
                                self.refresh_world_graph_bindings();
                            }
                        });
                        ui.label("Transform");
                        ui.label(format!(
                            "x: {:.2}, y: {:.2}, rot: {:.1}°, scale: {:.2}",
                            transform.x, transform.y, transform.rotation_deg, transform.scale
                        ));
                        ui.separator();
                    } else {
                        ui.label("No transform available for the selected entity.");
                    }
                    ui.label(format!(
                        "Scene entity count: {}",
                        self.state.hierarchy.entity_count()
                    ));
                } else {
                    ui.label("No selected entity");
                }
            }
            ViewportMode::ThreeD => {
                ui.label("3D preview canvas (placeholder render surface).");
                ui.label("Camera controls");
                ui.horizontal(|ui| {
                    ui.label("Yaw:");
                    ui.add(egui::DragValue::new(&mut self.state.viewport_3d.camera_yaw).speed(1.0));
                    ui.label("Pitch:");
                    ui.add(
                        egui::DragValue::new(&mut self.state.viewport_3d.camera_pitch).speed(1.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Zoom:");
                    if ui
                        .add(egui::DragValue::new(&mut self.state.viewport_3d.zoom).speed(0.25))
                        .changed()
                    {
                        if self.state.viewport_3d.zoom < 1.0 {
                            self.state.viewport_3d.zoom = 1.0;
                        }
                    }
                });
                ui.label("3D controls are bound to the same selected entity tree.");
                ui.label(format!(
                    "Camera: yaw {:.1}°, pitch {:.1}°, zoom {:.1}",
                    self.state.viewport_3d.camera_yaw,
                    self.state.viewport_3d.camera_pitch,
                    self.state.viewport_3d.zoom
                ));
            }
        }
    }

    fn render_docked_panels(&mut self, ctx: &Context) {
        let left_panels = self.state.layout.panels_in(DockRegion::Left);
        if !left_panels.is_empty() {
            SidePanel::left("left_dock")
                .default_width(self.state.layout.left_width)
                .width_range(140.0..=420.0)
                .show(ctx, |ui| {
                    for &panel in &left_panels {
                        match panel {
                            EditorPanel::Hierarchy => self.render_hierarchy(ui),
                            EditorPanel::AssetBrowser => self.render_asset_browser(ui),
                            EditorPanel::BehaviorTree => self.render_behavior_tree_editor(ui),
                            EditorPanel::Inspector => self.render_inspector(ui),
                            EditorPanel::Console => self.render_console(ui),
                            EditorPanel::FiniteStateMachine => self.render_fsm_editor(ui),
                            EditorPanel::LlmAgentConfig => self.render_llm_agent_panel(ui),
                            EditorPanel::Telemetry => self.render_telemetry_panel(ui),
                            EditorPanel::SpacetimeDashboard => self.render_spacetime_panel(ui),
                            EditorPanel::Viewport => {}
                        }
                        ui.separator();
                    }
                });
        }

        let right_panels = self.state.layout.panels_in(DockRegion::Right);
        if !right_panels.is_empty() {
            SidePanel::right("right_dock")
                .default_width(self.state.layout.right_width)
                .width_range(140.0..=420.0)
                .show(ctx, |ui| {
                    for &panel in &right_panels {
                        match panel {
                            EditorPanel::Inspector => self.render_inspector(ui),
                            EditorPanel::AssetBrowser => self.render_asset_browser(ui),
                            EditorPanel::Hierarchy => self.render_hierarchy(ui),
                            EditorPanel::BehaviorTree => self.render_behavior_tree_editor(ui),
                            EditorPanel::FiniteStateMachine => self.render_fsm_editor(ui),
                            EditorPanel::LlmAgentConfig => self.render_llm_agent_panel(ui),
                            EditorPanel::Telemetry => self.render_telemetry_panel(ui),
                            EditorPanel::SpacetimeDashboard => self.render_spacetime_panel(ui),
                            EditorPanel::Console => self.render_console(ui),
                            EditorPanel::Viewport => {}
                        }
                        ui.separator();
                    }
                });
        }

        let bottom_panels = self.state.layout.panels_in(DockRegion::Bottom);
        if !bottom_panels.is_empty() {
            TopBottomPanel::bottom("bottom_dock")
                .default_height(self.state.layout.bottom_height)
                .show(ctx, |ui| {
                    for &panel in &bottom_panels {
                        match panel {
                            EditorPanel::Console => self.render_console(ui),
                            EditorPanel::AssetBrowser => self.render_asset_browser(ui),
                            EditorPanel::Hierarchy => self.render_hierarchy(ui),
                            EditorPanel::Inspector => self.render_inspector(ui),
                            EditorPanel::BehaviorTree => self.render_behavior_tree_editor(ui),
                            EditorPanel::FiniteStateMachine => self.render_fsm_editor(ui),
                            EditorPanel::LlmAgentConfig => self.render_llm_agent_panel(ui),
                            EditorPanel::Telemetry => self.render_telemetry_panel(ui),
                            EditorPanel::SpacetimeDashboard => self.render_spacetime_panel(ui),
                            EditorPanel::Viewport => {}
                        }
                        ui.separator();
                    }
                });
        }

        let center_panels = self.state.layout.panels_in(DockRegion::Center);
        CentralPanel::default().show(ctx, |ui| {
            if center_panels.contains(&EditorPanel::Viewport) {
                self.render_viewport(ui);
            } else {
                for &panel in &center_panels {
                    match panel {
                        EditorPanel::Hierarchy => self.render_hierarchy(ui),
                        EditorPanel::Inspector => self.render_inspector(ui),
                        EditorPanel::Console => self.render_console(ui),
                        EditorPanel::AssetBrowser => self.render_asset_browser(ui),
                        EditorPanel::BehaviorTree => self.render_behavior_tree_editor(ui),
                        EditorPanel::FiniteStateMachine => self.render_fsm_editor(ui),
                        EditorPanel::LlmAgentConfig => self.render_llm_agent_panel(ui),
                        EditorPanel::Telemetry => self.render_telemetry_panel(ui),
                        EditorPanel::SpacetimeDashboard => self.render_spacetime_panel(ui),
                        EditorPanel::Viewport => self.render_viewport(ui),
                    }
                    ui.separator();
                }
            }
        });
    }
}

impl App for PodEditorApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                self.build_dock_toolbar(ui);
                ui.separator();
                self.build_play_toolbar(ui);
                ui.separator();
                self.build_project_toolbar(ui);
                for mode in [ViewportMode::TwoD, ViewportMode::ThreeD] {
                    if ui
                        .selectable_label(self.viewport_mode() == mode, mode.label())
                        .clicked()
                    {
                        self.set_viewport_mode(mode);
                    }
                    ui.separator();
                }
                for panel in [
                    EditorPanel::Viewport,
                    EditorPanel::Hierarchy,
                    EditorPanel::Inspector,
                    EditorPanel::AssetBrowser,
                    EditorPanel::Console,
                    EditorPanel::BehaviorTree,
                    EditorPanel::FiniteStateMachine,
                    EditorPanel::LlmAgentConfig,
                    EditorPanel::Telemetry,
                    EditorPanel::SpacetimeDashboard,
                ] {
                    if ui
                        .selectable_label(self.active_panel() == panel, panel.label())
                        .clicked()
                    {
                        self.set_active_panel(panel);
                    }
                }
            });
        });

        if let Some(entity_id) = self.state.selected_entity {
            if !self.state.hierarchy.contains_entity(entity_id) {
                self.set_selected_entity(None);
            }
        }

        self.render_docked_panels(ctx);

        TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Project: {}", self.state.project_name));
                ui.separator();
                ui.label(format!("Active pane: {}", self.state.active_panel.label()));
                ui.separator();
                ui.label(format!("Run state: {}", self.run_state().label()));
                ui.separator();
                ui.label(format!(
                    "Selected asset: {}",
                    self.state
                        .asset_browser
                        .selected
                        .as_deref()
                        .unwrap_or("None")
                ));
                ui.separator();
                ui.label(format!(
                    "Selected entity: {}",
                    self.selected_entity()
                        .map_or_else(|| "None".to_string(), |id| id.to_string())
                ));
            });
        });

        let _ = ui::editor_root_id(self.state.active_panel);
    }
}

mod ui {
    use super::EditorPanel;
    use egui::Id;

    pub fn editor_root_id(active_panel: EditorPanel) -> Id {
        Id::new(format!("pod_editor::{:?}", active_panel))
    }
}

pub fn launch_headless_editor() -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Prompt or Die Editor",
        options,
        Box::new(|_cc| Ok(Box::new(PodEditorApp::default()))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use pod_core::{
        decode_toon_value, Action, ActionLifecycleStage, ActionSource, AgentCapabilities, AgentId,
        AgentRole, AgentRuntimeProfile, AgentTelemetryFrame, AgentToolCallTrace,
        ChunkPopulationState, CreatureTemperament, EntityId, FocusedEntityDebugSummary,
        RegionPopulationState, ReplayFile, ReplayHeader, ShardIncidentSummary,
        TickTelemetryFrame, ToolCallStatus,
    };

    fn sample_tick_telemetry(tick: u64, entity_id: u64) -> TickTelemetryFrame {
        let runtime_profile = AgentRuntimeProfile {
            role: AgentRole::Player,
            agent_type: AgentType::Human,
            capabilities: AgentCapabilities::player_default(),
        };
        let mut agent = AgentTelemetryFrame::new(
            tick,
            AgentId::new(),
            Some(EntityId(entity_id)),
            runtime_profile,
            7,
            2,
            1,
            4,
            1,
            None,
            Some(TrajectorySample::new(
                tick,
                tick as f32 / 60.0,
                glam::Vec2::new(tick as f32, entity_id as f32 / 1000.0),
                glam::Vec2::X,
                0.0,
            )),
        );
        agent.update_trajectory_end(TrajectorySample::new(
            tick,
            (tick + 1) as f32 / 60.0,
            glam::Vec2::new(tick as f32 + 1.0, entity_id as f32 / 1000.0 + 0.5),
            glam::Vec2::new(1.0, 0.5),
            0.1,
        ));
        agent.record_action(
            ActionSource::ExternalSubmission,
            ActionLifecycleStage::Submitted,
            Action::Idle,
            None,
        );
        agent.record_action(
            ActionSource::ExternalSubmission,
            ActionLifecycleStage::Executed,
            Action::Idle,
            None,
        );
        agent.record_action(
            ActionSource::ExternalSubmission,
            ActionLifecycleStage::Rejected,
            Action::Idle,
            Some("cooldown".to_string()),
        );
        agent.record_tool_call(AgentToolCallTrace::failure(
            tick,
            "llm.complete",
            "qwen",
            ToolCallStatus::TimedOut,
            48,
            "timeout",
        ));

        TickTelemetryFrame {
            tick,
            agents: vec![agent],
        }
    }

    fn sample_replay_file() -> ReplayFile {
        ReplayFile {
            header: ReplayHeader {
                name: "flagship-mmo-loop".to_string(),
                timestamp: 1_741_315_200,
                world_seed: 42,
                tick_count: 2,
                agent_count: 1,
                notes: "acceptance".to_string(),
            },
            traces: Vec::new(),
            telemetry_windows: vec![sample_tick_telemetry(15, 1001)],
        }
    }

    fn sample_tool_call_event(tick: u64, entity_id: u64) -> AgentToolCallEvent {
        AgentToolCallEvent::new(
            entity_id,
            AgentToolCallTrace::failure(
                tick,
                "llm.complete",
                "qwen",
                ToolCallStatus::TimedOut,
                48,
                "timeout",
            ),
        )
    }

    fn sample_tick_rollup(entity_id: u64) -> AgentTickRollup {
        AgentTickRollup {
            tick_start: 10,
            tick_end: 15,
            agent_entity_id: entity_id,
            total_distance: 16.75,
            submitted_action_count: 4,
            executed_action_count: 3,
            rejected_action_count: 1,
            tool_call_count: 1,
            tool_error_count: 1,
            visible_entity_count: 12,
            audible_event_count: 3,
            message_count: 2,
            average_tool_latency_ms: 48.0,
        }
    }

    fn sample_focused_entity_debug_summary(entity_id: u64) -> FocusedEntityDebugSummary {
        FocusedEntityDebugSummary {
            shard_id: "direct-connect".to_string(),
            entity_id,
            latest_tick: 18,
            tool_call_count: 2,
            tool_error_count: 1,
            rejected_action_count: 1,
            total_distance: 22.75,
            average_tool_latency_ms: 41.0,
            visible_entity_count: 12,
            audible_event_count: 3,
            message_count: 2,
            latest_tool_name: Some("llm.complete".to_string()),
            latest_tool_status: Some("Succeeded".to_string()),
            latest_tool_error: None,
            notes: vec!["1 rejected action retained".to_string()],
        }
    }

    fn sample_shard_transport_summary() -> ShardTransportSummary {
        ShardTransportSummary {
            shard_id: "direct-connect".to_string(),
            latest_tick: 24,
            client_count: 2,
            resumed_sessions: 1,
            recovery_snapshots_sent: 2,
            recovery_delivery_failures: 1,
            client_inactivity_timeout_ticks: 600,
            queue_pressure_warn_depth: 192,
            total_pending_action_queue_depth: 1,
            queue_pressure_client_count: 1,
            total_inbound_messages: 12,
            total_outbound_messages: 30,
            total_inbound_bytes: 512,
            total_outbound_bytes: 4096,
            action_batches_received: 4,
            full_snapshot_requests: 1,
            ping_requests: 3,
            state_deltas_sent: 12,
            event_batches_sent: 4,
            debug_documents_sent: 7,
            rejected_messages_sent: 1,
            timed_out_clients: 2,
            queue_pressure_events: 3,
            clients: vec![pod_core::ClientTransportSummary {
                client_id: "client-a".to_string(),
                player_name: Some("debug".to_string()),
                controlled_entity: Some(1001),
                session_resumes: 1,
                recovery_snapshots_sent: 1,
                recovery_delivery_failures: 1,
                last_seen_tick: 24,
                ticks_since_last_seen: 0,
                last_sent_tick: Some(24),
                pending_action_queue_depth: 1,
                queue_pressure: true,
                inbound_messages: 6,
                outbound_messages: 15,
                inbound_bytes: 256,
                outbound_bytes: 2048,
                action_batches_received: 2,
                full_snapshot_requests: 1,
                ping_requests: 2,
                state_deltas_sent: 6,
                event_batches_sent: 2,
                debug_documents_sent: 4,
                rejected_messages_sent: 1,
                debug_telemetry_enabled: true,
            }],
        }
    }

    #[test]
    fn editor_default_state_is_viewport() {
        let app = PodEditorApp::default();
        assert_eq!(app.active_panel(), EditorPanel::Viewport);
    }

    #[test]
    fn editor_allows_panel_switch() {
        let mut app = PodEditorApp::default();
        app.set_active_panel(EditorPanel::AssetBrowser);
        assert_eq!(app.active_panel(), EditorPanel::AssetBrowser);
    }

    #[test]
    fn dock_layout_toggles_optional_panels() {
        let mut app = PodEditorApp::default();
        assert!(app.state.layout.is_visible(EditorPanel::Hierarchy));
        app.state.layout.toggle_visibility(EditorPanel::Hierarchy);
        assert!(!app.state.layout.is_visible(EditorPanel::Hierarchy));
        app.state.layout.toggle_visibility(EditorPanel::Hierarchy);
        assert!(app.state.layout.is_visible(EditorPanel::Hierarchy));
    }

    #[test]
    fn scene_hierarchy_tracks_selection() {
        let mut app = PodEditorApp::default();
        assert_eq!(app.selected_entity(), Some(1001));
        app.set_selected_entity(Some(1004));
        assert_eq!(app.selected_entity(), Some(1004));
        app.set_selected_entity(Some(9999));
        assert_eq!(app.selected_entity(), None);
    }

    #[test]
    fn inspector_properties_are_mutable_for_selected_entity() {
        let mut app = PodEditorApp::default();
        {
            let props = app
                .selected_entity_properties()
                .expect("selected entity properties available");
            assert_eq!(props.get("Health"), Some(&"100".to_string()));
            props.insert("Health".to_string(), "88".to_string());
            assert_eq!(props.get("Health"), Some(&"88".to_string()));
        }
    }

    #[test]
    fn viewport_gizmo_moves_selected_entity_transform() {
        let mut app = PodEditorApp::default();
        {
            let transform = app
                .selected_entity_transform()
                .expect("transform exists for selected entity");
            transform.x = 1.0;
            transform.y = 1.0;
        }
        app.move_selected_entity(0.5, -0.25);
        assert_eq!(
            app.selected_entity_transform().expect("transform remains"),
            &Transform2D {
                x: 1.5,
                y: 0.75,
                rotation_deg: 45.0,
                scale: 1.2,
            }
        );
        let live = app
            .state
            .transforms_2d
            .get(&1001)
            .expect("live transform")
            .x;
        assert_eq!(live, 1.5);
    }

    #[test]
    fn viewport_mode_can_switch_to_three_d() {
        let mut app = PodEditorApp::default();
        assert_eq!(app.viewport_mode(), ViewportMode::TwoD);
        app.set_viewport_mode(ViewportMode::ThreeD);
        assert_eq!(app.viewport_mode(), ViewportMode::ThreeD);
        app.set_viewport_mode(ViewportMode::TwoD);
        assert_eq!(app.viewport_mode(), ViewportMode::TwoD);
    }

    #[test]
    fn asset_browser_can_import_and_filter() {
        let mut app = PodEditorApp::default();
        let total = app.state.asset_browser.assets.len();
        app.state
            .asset_browser
            .import_asset("assets/props/new_tree.glb");
        assert_eq!(app.state.asset_browser.assets.len(), total + 1);
        assert_eq!(
            app.state.asset_browser.selected,
            Some("new_tree".to_string())
        );

        app.state.asset_browser.filter = Some(AssetKind::Mesh);
        assert!(app
            .state
            .asset_browser
            .visible_assets()
            .iter()
            .all(|asset| { asset.kind == AssetKind::Mesh }));
        app.state.asset_browser.filter = None;
        assert_eq!(app.state.asset_browser.visible_assets().len(), total + 1);
    }

    #[test]
    fn asset_browser_recognizes_scene_assets() {
        let mut app = PodEditorApp::default();
        app.state
            .asset_browser
            .import_asset("assets/scenes/boss_room.tscn");
        app.state
            .asset_browser
            .import_asset("assets/scenes/overworld.tmj");
        app.state
            .asset_browser
            .import_asset("assets/prefabs/enemy.prefab");

        let imported = app
            .state
            .asset_browser
            .assets
            .iter()
            .find(|asset| asset.id == "boss_room")
            .expect("scene asset should be imported");
        assert_eq!(imported.kind, AssetKind::Scene);
        assert_eq!(
            app.state
                .asset_browser
                .assets
                .iter()
                .find(|asset| asset.id == "overworld")
                .expect("tiled scene asset should be imported")
                .kind,
            AssetKind::Scene
        );
        assert_eq!(
            app.state
                .asset_browser
                .assets
                .iter()
                .find(|asset| asset.id == "enemy")
                .expect("unity prefab asset should be imported")
                .kind,
            AssetKind::Scene
        );

        app.state.asset_browser.filter = Some(AssetKind::Scene);
        assert!(app
            .state
            .asset_browser
            .visible_assets()
            .iter()
            .all(|asset| asset.kind == AssetKind::Scene));
        assert!(app
            .state
            .asset_browser
            .visible_assets()
            .iter()
            .any(|asset| asset.id == "boss_room"));
        assert!(app
            .state
            .asset_browser
            .visible_assets()
            .iter()
            .any(|asset| asset.id == "overworld"));
        assert!(app
            .state
            .asset_browser
            .visible_assets()
            .iter()
            .any(|asset| asset.id == "enemy"));
    }

    #[test]
    fn play_controls_move_between_states() {
        let mut app = PodEditorApp::default();
        assert_eq!(app.run_state(), PlayState::Stopped);
        app.play();
        assert_eq!(app.run_state(), PlayState::Playing);
        app.pause();
        assert_eq!(app.run_state(), PlayState::Paused);
        app.stop();
        assert_eq!(app.run_state(), PlayState::Stopped);
    }

    #[test]
    fn workflow_panels_are_supported_in_layout() {
        let mut app = PodEditorApp::default();
        for panel in [
            EditorPanel::BehaviorTree,
            EditorPanel::FiniteStateMachine,
            EditorPanel::LlmAgentConfig,
            EditorPanel::Telemetry,
            EditorPanel::SpacetimeDashboard,
        ] {
            app.set_dock_region(panel, DockRegion::Left);
            assert_eq!(app.state.layout.region_for_panel(panel), DockRegion::Left);
            app.set_active_panel(panel);
            assert_eq!(app.active_panel(), panel);
        }
    }

    #[test]
    fn editor_undo_redo_restores_previous_transform() {
        let mut app = PodEditorApp::default();
        let original_x = app
            .selected_entity_transform()
            .expect("selected entity exists")
            .x;
        app.move_selected_entity(1.5, -0.5);
        app.undo();
        assert_eq!(
            app.selected_entity_transform()
                .expect("entity exists after undo")
                .x,
            original_x
        );
        app.redo();
        assert_eq!(
            app.selected_entity_transform()
                .expect("entity exists after redo")
                .x,
            original_x + 1.5
        );
    }

    #[test]
    fn telemetry_panel_tracks_selected_entity_history() {
        let mut app = PodEditorApp::default();
        app.record_tick_telemetry(sample_tick_telemetry(10, 1001));
        app.record_tick_telemetry(sample_tick_telemetry(11, 1001));

        let samples = app.state.telemetry.trajectory_for_entity(1001);
        assert_eq!(samples.len(), 3);
        assert_eq!(app.selected_entity(), Some(1001));
        assert_eq!(
            app.state
                .telemetry
                .latest_agent_for_entity(1001)
                .expect("selected entity telemetry")
                .message_count,
            1
        );
    }

    #[test]
    fn spacetime_dashboard_aggregates_authoritative_telemetry() {
        let mut app = PodEditorApp::default();
        app.record_tick_telemetry(sample_tick_telemetry(12, 1001));

        assert_eq!(app.state.spacetime_dashboard.latest_tick, 12);
        assert_eq!(app.state.spacetime_dashboard.connected_players, 1);
        assert_eq!(app.state.spacetime_dashboard.visible_entity_count, 7);
        assert_eq!(app.state.spacetime_dashboard.audible_event_count, 2);
        assert_eq!(app.state.spacetime_dashboard.message_count, 1);
        assert!(app.state.spacetime_dashboard.action_rejection_rate > 0.0);
        assert!(app.state.spacetime_dashboard.tool_call_error_rate > 0.0);
        assert_eq!(app.state.spacetime_dashboard.agent_summaries.len(), 1);
        assert_eq!(
            app.state.spacetime_dashboard.agent_summaries[0].entity_id,
            Some(1001)
        );
    }

    #[test]
    fn creator_catalog_links_models_objects_and_monsters() {
        let mut app = PodEditorApp::default();
        assert!(app
            .state
            .creator_catalog
            .objects
            .iter()
            .any(|object| object.id == "object:player-avatar"));

        assert!(app.set_selected_asset("hero_character"));
        let object_id = app
            .create_object_from_selection("oak-crate")
            .expect("object should be created");
        let monster_id = app
            .create_monster_from_selection("ember-fox", CreatureTemperament::Aggressive)
            .expect("monster should be created");
        app.associate_creator_content(
            monster_id.clone(),
            CreatorNodeKind::Monster,
            object_id.clone(),
            CreatorNodeKind::Object,
            CreatorAssociationKind::EncounterTrigger,
            "ember-fox guards the oak-crate",
        );

        assert!(app
            .state
            .creator_catalog
            .associations
            .iter()
            .any(|association| {
                association.from_id == object_id
                    && association.relation == CreatorAssociationKind::VisualModel
            }));
        assert!(app
            .state
            .creator_catalog
            .associations
            .iter()
            .any(|association| {
                association.from_id == monster_id
                    && association.relation == CreatorAssociationKind::VisualModel
            }));
        assert!(app
            .state
            .creator_catalog
            .associations
            .iter()
            .any(|association| {
                association.from_id == monster_id
                    && association.to_id == object_id
                    && association.relation == CreatorAssociationKind::EncounterTrigger
            }));
    }

    #[test]
    fn editor_world_graph_binds_default_entities_to_streamed_regions() {
        let app = PodEditorApp::default();
        let binding = app
            .state
            .world_graph
            .binding_for_entity(1001)
            .expect("player binding should exist");

        assert_eq!(binding.chunk_key.as_deref(), Some("0:0"));
        assert_eq!(binding.region_id.as_deref(), Some("verdant-heart"));
        assert_eq!(binding.region_name.as_deref(), Some("Verdant Heart"));
        assert_eq!(binding.faction_track_id.as_deref(), Some("verdant-wardens"));
        assert_eq!(
            binding.encounter_table_id.as_deref(),
            Some("verdant-heart-wildlife")
        );
        assert!(binding
            .quest_graph_ids
            .iter()
            .any(|id| id == "verdant-intro"));

        let environment = app
            .state
            .world_graph
            .binding_for_entity(1004)
            .expect("environment binding should exist");
        assert_eq!(environment.chunk_key.as_deref(), Some("1:-1"));
        assert_eq!(environment.region_id.as_deref(), Some("ashen-steppe"));
    }

    #[test]
    fn editor_world_graph_updates_when_selected_entity_moves_between_chunks() {
        let mut app = PodEditorApp::default();
        app.set_selected_entity(Some(1001));
        app.move_selected_entity(8.5, 0.0);

        let binding = app
            .state
            .world_graph
            .binding_for_entity(1001)
            .expect("binding should be refreshed after movement");
        assert_eq!(binding.chunk_key.as_deref(), Some("1:0"));
    }

    #[test]
    fn editor_world_graph_sync_methods_update_exported_snapshot() {
        let mut app = PodEditorApp::default();
        let mut region =
            WorldRegionDefinition::new("gloamwood-edge", "Gloamwood Edge", "gloamwood");
        region.chunk_keys.push("-1:0".into());
        region.active_quest_graph_ids.push("gloamwood-watch".into());
        region.dominant_faction_track_id = "gloamwood-coven".into();
        region
            .encounter_table_ids
            .push("gloamwood-encounters".into());
        app.sync_world_region(region);

        let mut chunk = WorldChunkDefinition::new("-1:0", "gloamwood-edge", "gloamwood");
        chunk.quest_graph_ids.push("gloamwood-watch".into());
        chunk.faction_track_ids.push("gloamwood-coven".into());
        chunk
            .encounter_table_ids
            .push("gloamwood-encounters".into());
        app.sync_world_chunk(chunk);

        app.sync_world_quest_graph(QuestStateGraph::new(
            "gloamwood-watch",
            "Gloamwood Watch",
            "enter-the-grove",
            vec![QuestStageDefinition {
                stage_id: "enter-the-grove".into(),
                title: "Enter the Grove".into(),
                objectives: vec!["Scout the gloamwood entrance".into()],
                next_stage_ids: vec![],
                reward_tags: vec!["unlock:gloamwood".into()],
            }],
        ));
        app.sync_world_faction_track(FactionReputationTrack::new(
            "gloamwood-coven",
            "Gloamwood Coven",
            vec![FactionReputationTier {
                tier_id: "favored".into(),
                label: "Favored".into(),
                minimum_score: 25,
                perk_tags: vec!["quest:gloamwood-watch".into()],
            }],
        ));
        app.sync_world_encounter_table(RegionEncounterTable::new(
            "gloamwood-encounters",
            "gloamwood",
            "grove",
            vec![EncounterSpawnEntry {
                archetype_id: "gloam-stalker".into(),
                weight: 5,
                min_count: 1,
                max_count: 2,
                required_stage_tags: vec!["unlock:gloamwood".into()],
                required_reputation_tiers: vec!["favored".into()],
            }],
        ));

        let snapshot = app.export_snapshot_toon_document();
        let value = decode_toon_value(&snapshot).expect("snapshot should decode");
        assert_eq!(
            value["payload"]["world_graph"]["regions"]
                .as_array()
                .expect("regions array")
                .len(),
            4
        );
        assert_eq!(
            value["payload"]["world_graph"]["quest_graphs"]
                .as_array()
                .expect("quest array")
                .last()
                .and_then(|entry| entry.get("quest_id"))
                .and_then(|entry| entry.as_str()),
            Some("gloamwood-watch")
        );
    }

    #[test]
    fn editor_snapshot_exports_to_toon_for_world_building_agents() {
        let mut app = PodEditorApp::default();
        assert!(app.set_selected_asset("hero_character"));
        let monster_id = app
            .create_monster_from_selection("storm-lark", CreatureTemperament::Loyal)
            .expect("monster should be created");
        app.record_tick_telemetry(sample_tick_telemetry(13, 1001));

        let snapshot = app.export_snapshot_toon_document();
        let value = decode_toon_value(&snapshot).expect("snapshot should decode");
        assert_eq!(value["document_type"], "editor_world_snapshot");
        assert_eq!(value["payload"]["selected_entity"], 1001);
        assert_eq!(
            value["payload"]["creator_catalog"]["monsters"][0]["id"],
            monster_id
        );
        assert_eq!(
            value["payload"]["selected_entity_world_binding"]["region_id"],
            "verdant-heart"
        );
        assert!(
            value["payload"]["world_graph"]["regions"]
                .as_array()
                .expect("regions array")
                .len()
                >= 3
        );
        assert_eq!(value["payload"]["latest_tick_telemetry"]["tick"], 13);
    }

    #[test]
    fn editor_telemetry_surfaces_export_to_toon_documents() {
        let mut app = PodEditorApp::default();
        app.record_tick_telemetry(sample_tick_telemetry(14, 1001));

        let telemetry_document = app.state.telemetry.to_toon_document();
        let telemetry_value =
            decode_toon_value(&telemetry_document).expect("telemetry document should decode");
        assert_eq!(telemetry_value["document_type"], "editor_telemetry_panel");
        assert_eq!(telemetry_value["payload"]["timeline"][0]["tick"], 14);

        let dashboard_document = app.state.spacetime_dashboard.to_toon_document();
        let dashboard_value =
            decode_toon_value(&dashboard_document).expect("dashboard document should decode");
        assert_eq!(
            dashboard_value["document_type"],
            "editor_spacetime_dashboard"
        );
        assert_eq!(dashboard_value["payload"]["latest_tick"], 14);

        let world_graph_document = app.state.world_graph.to_toon_document();
        let world_graph_value =
            decode_toon_value(&world_graph_document).expect("world graph should decode");
        assert_eq!(world_graph_value["document_type"], "editor_world_graph");
        assert_eq!(
            world_graph_value["payload"]["entity_bindings"][0]["region_id"],
            "verdant-heart"
        );
    }

    #[test]
    fn editor_imports_replay_toon_into_existing_telemetry_surfaces() {
        let mut app = PodEditorApp::default();
        let replay = sample_replay_file();

        app.import_replay_toon_document(&replay.to_toon_document())
            .expect("replay TOON should import");

        assert_eq!(
            app.state
                .latest_replay
                .as_ref()
                .expect("replay stored")
                .header
                .name,
            "flagship-mmo-loop"
        );
        assert_eq!(
            app.state.telemetry.latest().expect("telemetry synced").tick,
            15
        );
        assert_eq!(app.state.spacetime_dashboard.latest_tick, 15);
    }

    #[test]
    fn editor_imports_world_population_toon_into_dashboard_state() {
        let mut app = PodEditorApp::default();
        let population = WorldPopulationState {
            tick: 22,
            chunks: vec![ChunkPopulationState {
                chunk_key: "0:0".to_string(),
                region_id: Some("verdant-heart".to_string()),
                region_name: Some("Verdant Heart".to_string()),
                biome_id: Some("verdant-hollow".to_string()),
                quest_graph_ids: vec!["verdant-intro".to_string()],
                faction_track_id: Some("verdant-wardens".to_string()),
                encounter_table_ids: vec!["verdant-heart-wildlife".to_string()],
                counts: PopulationBreakdown {
                    players: 1,
                    wild_creatures: 2,
                    scenery: 3,
                    ..PopulationBreakdown::default()
                },
                active_entity_count: 6,
                ambient_population_cap: 8,
                spawn_budget_remaining: 2,
                pending_respawns: 1,
                next_respawn_tick: Some(48),
                population_pressure: 0.75,
            }],
            regions: vec![RegionPopulationState {
                region_id: "verdant-heart".to_string(),
                region_name: "Verdant Heart".to_string(),
                primary_biome_id: "verdant-hollow".to_string(),
                chunk_keys: vec!["0:0".to_string()],
                active_quest_graph_ids: vec!["verdant-intro".to_string()],
                dominant_faction_track_id: Some("verdant-wardens".to_string()),
                encounter_table_ids: vec!["verdant-heart-wildlife".to_string()],
                active_chunk_count: 1,
                counts: PopulationBreakdown {
                    players: 1,
                    wild_creatures: 2,
                    scenery: 3,
                    ..PopulationBreakdown::default()
                },
                active_entity_count: 6,
                ambient_population_cap: 8,
                spawn_budget_remaining: 2,
                pending_respawns: 1,
                next_respawn_tick: Some(48),
                population_pressure: 0.75,
            }],
        };

        app.import_world_population_toon_document(&population.to_toon_document())
            .expect("population TOON should import");

        assert_eq!(app.state.spacetime_dashboard.latest_tick, 22);
        assert_eq!(app.state.spacetime_dashboard.population_state.tick, 22);
        assert_eq!(
            app.state.spacetime_dashboard.population_state.regions[0].region_id,
            "verdant-heart"
        );
        assert_eq!(
            app.state.spacetime_dashboard.population_state.chunks[0].spawn_budget_remaining,
            2
        );
        assert_eq!(
            app.state.spacetime_dashboard.population_state.chunks[0].next_respawn_tick,
            Some(48)
        );
    }

    #[test]
    fn encounter_balance_previews_reflect_authoritative_population_pressure() {
        let mut app = PodEditorApp::default();
        let population = WorldPopulationState {
            tick: 22,
            chunks: vec![ChunkPopulationState {
                chunk_key: "0:0".to_string(),
                region_id: Some("verdant-heart".to_string()),
                region_name: Some("Verdant Heart".to_string()),
                biome_id: Some("verdant-hollow".to_string()),
                quest_graph_ids: vec!["verdant-intro".to_string()],
                faction_track_id: Some("verdant-wardens".to_string()),
                encounter_table_ids: vec!["verdant-heart-wildlife".to_string()],
                counts: PopulationBreakdown {
                    players: 1,
                    npcs: 2,
                    wild_creatures: 6,
                    companions: 1,
                    ..PopulationBreakdown::default()
                },
                active_entity_count: 10,
                ambient_population_cap: 12,
                spawn_budget_remaining: 2,
                pending_respawns: 2,
                next_respawn_tick: Some(48),
                population_pressure: 0.83,
            }],
            regions: Vec::new(),
        };

        app.import_world_population_toon_document(&population.to_toon_document())
            .expect("population TOON should import");

        let preview = app
            .encounter_balance_previews()
            .into_iter()
            .find(|preview| preview.table_id == "verdant-heart-wildlife")
            .expect("verdant preview");

        assert_eq!(preview.attached_chunk_count, 1);
        assert_eq!(preview.effective_cap, 12);
        assert_eq!(preview.active_spawned_actors, 10);
        assert_eq!(preview.pending_respawns, 2);
        assert_eq!(preview.next_respawn_tick, Some(48));
        assert_eq!(preview.suggested_cap_delta, 2);
    }

    #[test]
    fn encounter_table_ambient_cap_adjustments_are_clamped_and_persisted() {
        let mut app = PodEditorApp::default();
        let initial_cap = app
            .state
            .world_graph
            .encounter_tables
            .iter()
            .find(|table| table.table_id == "verdant-heart-wildlife")
            .map(|table| table.ambient_cap)
            .expect("seeded encounter table");
        assert_eq!(initial_cap, 12);

        assert!(app.adjust_world_encounter_table_ambient_cap(
            "verdant-heart-wildlife",
            3
        ));
        let boosted = app
            .state
            .world_graph
            .encounter_tables
            .iter()
            .find(|table| table.table_id == "verdant-heart-wildlife")
            .expect("boosted encounter table");
        assert_eq!(boosted.ambient_cap, 15);

        assert!(app.adjust_world_encounter_table_ambient_cap(
            "verdant-heart-wildlife",
            -99
        ));
        let clamped = app
            .state
            .world_graph
            .encounter_tables
            .iter()
            .find(|table| table.table_id == "verdant-heart-wildlife")
            .expect("clamped encounter table");
        assert_eq!(clamped.ambient_cap, 1);
    }

    #[test]
    fn editor_imports_shard_incident_toon_into_dashboard_state() {
        let mut app = PodEditorApp::default();
        let summary = ShardIncidentSummary {
            shard_id: "alpha-1".to_string(),
            latest_tick: 240,
            severity: pod_core::IncidentSeverity::Warning,
            summary: "Shard alpha-1 requires attention".to_string(),
            tick_budget_overrun_rate: 0.08,
            action_rejection_rate: 0.02,
            tool_call_error_rate: 0.11,
            average_tool_latency_ms: 820.0,
            average_trajectory_distance: 3.2,
            peak_entity_count: 512,
            peak_agent_count: 128,
            capture_actions: 4,
            summon_actions: 2,
            gather_actions: 7,
            loot_actions: 9,
            notes: vec!["tool-call error rate exceeds 10%".to_string()],
        };

        app.import_shard_incident_toon_document(&summary.to_toon_document())
            .expect("incident TOON should import");

        let incident = app
            .state
            .spacetime_dashboard
            .latest_incident_summary
            .as_ref()
            .expect("incident stored");
        assert_eq!(incident.shard_id, "alpha-1");
        assert_eq!(app.state.spacetime_dashboard.average_tool_latency_ms, 820.0);
        assert_eq!(app.state.spacetime_dashboard.capture_actions, 4);
    }

    #[test]
    fn editor_imports_shard_transport_toon_into_dashboard_state() {
        let mut app = PodEditorApp::default();
        let summary = sample_shard_transport_summary();

        app.import_shard_transport_toon_document(&summary.to_toon_document())
            .expect("transport TOON should import");

        let transport = app
            .state
            .spacetime_dashboard
            .latest_transport_summary
            .as_ref()
            .expect("transport stored");
        assert_eq!(transport.latest_tick, 24);
        assert_eq!(transport.client_count, 2);
        assert_eq!(transport.recovery_snapshots_sent, 2);
        assert_eq!(transport.recovery_delivery_failures, 1);
        assert_eq!(transport.clients[0].client_id, "client-a");
    }

    #[test]
    fn editor_imports_live_tool_call_and_rollup_toon_documents() {
        let mut app = PodEditorApp::default();
        let tool_event = sample_tool_call_event(16, 1001);
        let rollup = sample_tick_rollup(1001);

        app.import_agent_tool_call_toon_document(&tool_event.to_toon_document())
            .expect("tool-call TOON should import");
        app.import_agent_tick_rollup_toon_document(&rollup.to_toon_document())
            .expect("rollup TOON should import");

        assert_eq!(
            app.state.spacetime_dashboard.recent_tool_call_events.len(),
            1
        );
        assert_eq!(app.state.spacetime_dashboard.recent_tick_rollups.len(), 1);
        assert!(app.state.spacetime_dashboard.tool_call_error_rate > 0.0);
        assert_eq!(app.state.spacetime_dashboard.visible_entity_count, 12);
        assert_eq!(app.state.spacetime_dashboard.agent_summaries.len(), 1);
    }

    #[test]
    fn editor_selected_entity_debug_summary_survives_without_tick_timeline() {
        let mut app = PodEditorApp::default();
        let tool_event = sample_tool_call_event(16, 1001);
        let rollup = sample_tick_rollup(1001);

        app.import_agent_tool_call_toon_document(&tool_event.to_toon_document())
            .expect("tool-call TOON should import");
        app.import_agent_tick_rollup_toon_document(&rollup.to_toon_document())
            .expect("rollup TOON should import");

        let summary = app
            .selected_entity_debug_summary()
            .expect("selected entity debug summary available");
        assert_eq!(summary.entity_id, 1001);
        assert_eq!(summary.recent_tool_call_count, 1);
        assert_eq!(summary.recent_rollup_count, 1);
        assert_eq!(summary.rejected_actions, rollup.rejected_action_count as usize);
        assert_eq!(summary.tool_errors, rollup.tool_error_count as usize);
        assert_eq!(summary.trajectory_distance, rollup.total_distance);
        assert_eq!(
            summary
                .latest_tool_call
                .as_ref()
                .expect("tool-call present")
                .trace
                .tool_name,
            tool_event.trace.tool_name
        );
    }

    #[test]
    fn editor_imports_focused_entity_debug_summaries() {
        let mut app = PodEditorApp::default();
        let summary = sample_focused_entity_debug_summary(1001);

        app.import_focused_entity_debug_toon_document(&summary.to_toon_document())
            .expect("focused summary TOON should import");

        let selected = app
            .selected_entity_debug_summary()
            .expect("selected entity debug summary available");
        assert_eq!(selected.entity_id, 1001);
        assert_eq!(selected.latest_tick, 18);
        assert_eq!(selected.tool_errors, 1);
        assert_eq!(selected.trajectory_distance, 22.75);
        assert_eq!(selected.average_tool_latency_ms, 41.0);
    }

    #[test]
    fn editor_routes_live_debug_toon_documents_into_existing_surfaces() {
        let mut app = PodEditorApp::default();
        let replay = sample_replay_file();
        let incident = ShardIncidentSummary {
            shard_id: "alpha-2".to_string(),
            latest_tick: 360,
            severity: pod_core::IncidentSeverity::Warning,
            summary: "Shard alpha-2 requires attention".to_string(),
            tick_budget_overrun_rate: 0.08,
            action_rejection_rate: 0.02,
            tool_call_error_rate: 0.11,
            average_tool_latency_ms: 820.0,
            average_trajectory_distance: 3.2,
            peak_entity_count: 512,
            peak_agent_count: 128,
            capture_actions: 4,
            summon_actions: 2,
            gather_actions: 7,
            loot_actions: 9,
            notes: vec!["tool-call error rate exceeds 10%".to_string()],
        };

        app.import_live_debug_toon_document(
            &VersionedTickTelemetry::new(sample_tick_telemetry(18, 1001)).to_toon_document(),
        )
        .expect("telemetry TOON should route");
        app.import_live_debug_toon_document(&sample_tool_call_event(18, 1001).to_toon_document())
            .expect("tool-call TOON should route");
        app.import_live_debug_toon_document(&sample_tick_rollup(1001).to_toon_document())
            .expect("rollup TOON should route");
        app.import_live_debug_toon_document(
            &sample_focused_entity_debug_summary(1001).to_toon_document(),
        )
        .expect("focused summary TOON should route");
        app.import_live_debug_toon_document(&replay.to_toon_document())
            .expect("replay TOON should route");
        app.import_live_debug_toon_document(&incident.to_toon_document())
            .expect("incident TOON should route");
        app.import_live_debug_toon_document(&sample_shard_transport_summary().to_toon_document())
            .expect("transport TOON should route");

        assert_eq!(
            app.state.telemetry.latest().expect("telemetry stored").tick,
            18
        );
        assert_eq!(
            app.state
                .latest_replay
                .as_ref()
                .expect("replay stored")
                .header
                .name,
            "flagship-mmo-loop"
        );
        assert_eq!(
            app.state
                .spacetime_dashboard
                .latest_incident_summary
                .as_ref()
                .expect("incident stored")
                .shard_id,
            "alpha-2"
        );
        assert_eq!(
            app.selected_entity_debug_summary()
                .expect("focused summary available")
                .latest_tick,
            18
        );
        assert_eq!(
            app.state.spacetime_dashboard.recent_tool_call_events.len(),
            1
        );
        assert_eq!(app.state.spacetime_dashboard.recent_tick_rollups.len(), 1);
        assert_eq!(
            app.state
                .spacetime_dashboard
                .latest_transport_summary
                .as_ref()
                .expect("transport stored")
                .client_count,
            2
        );
    }

    #[test]
    fn behavior_tree_editor_supports_add_edit_remove() {
        let mut app = PodEditorApp::default();
        let original_count = app.state.behavior_tree.nodes.len();
        app.behavior_tree_new_node_name = "Search".to_string();
        app.add_behavior_node(BehaviorTreeNodeKind::Action);

        let added_node = app
            .state
            .behavior_tree
            .selected_node
            .expect("selected after add");
        let _ = app
            .state
            .behavior_tree
            .nodes
            .get(&added_node)
            .expect("added node exists");

        app.set_behavior_node_name(added_node, "TrackEnemy".to_string());
        app.set_behavior_node_kind(added_node, BehaviorTreeNodeKind::Condition);
        assert_eq!(
            app.state
                .behavior_tree
                .nodes
                .get(&added_node)
                .expect("node still exists")
                .name,
            "TrackEnemy"
        );
        assert_eq!(
            app.state
                .behavior_tree
                .nodes
                .get(&added_node)
                .expect("node still exists")
                .kind,
            BehaviorTreeNodeKind::Condition
        );

        app.remove_behavior_node(added_node);
        assert_eq!(app.state.behavior_tree.nodes.len(), original_count);
    }

    #[test]
    fn fsm_editor_supports_state_and_transition_lifecycle() {
        let mut app = PodEditorApp::default();
        assert!(app.add_fsm_state("alerted"));
        assert!(app.state.fsm.states.iter().any(|state| state == "alerted"));
        assert!(app.add_fsm_transition(
            "idle".to_string(),
            "alerted".to_string(),
            "see_enemy".to_string()
        ));
        let transition = app
            .state
            .fsm
            .transitions
            .iter()
            .position(|transition| {
                transition.from == "idle"
                    && transition.to == "alerted"
                    && transition.on == "see_enemy"
            })
            .expect("transition added");
        app.remove_fsm_transition(transition);
        assert_eq!(app.state.fsm.transitions.len(), 3);
        app.remove_fsm_state("alerted");
        assert!(!app.state.fsm.states.iter().any(|state| state == "alerted"));
    }

    #[test]
    fn llm_agent_panel_is_configurable() {
        let mut app = PodEditorApp::default();
        app.set_llm_model("gpt-test");
        app.set_llm_system_prompt("Testing prompt");
        assert_eq!(app.state.llm_agent_config.model, "gpt-test");
        assert_eq!(app.state.llm_agent_config.system_prompt, "Testing prompt");
    }

    #[test]
    fn spacetime_dashboard_records_reducer_calls() {
        let mut app = PodEditorApp::default();
        let start = app.state.spacetime_dashboard.reducer_calls;
        app.set_spacetime_reducer_call("tick");
        assert_eq!(app.state.spacetime_dashboard.reducer_calls, start + 1);
        assert_eq!(app.state.spacetime_dashboard.last_reducer_call, "tick");
        assert_eq!(app.state.spacetime_dashboard.latest_tick, 0);
    }

    #[test]
    fn project_file_save_and_load_roundtrip() {
        let mut app = PodEditorApp::default();
        let mut path = std::env::temp_dir();
        path.push(format!(
            "pod_editor_{}_project.json",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("timestamp available")
                .as_nanos()
        ));
        app.state.project_name = "Unit Test Project".to_string();
        app.set_llm_model("gpt-test");
        app.set_spacetime_reducer_call("manual_save");

        app.save_project(&path).expect("project saved");
        let loaded = PodEditorApp::default();
        let mut restored = loaded;
        restored.load_project(&path).expect("project loaded");

        assert_eq!(restored.state.project_name, "Unit Test Project");
        assert_eq!(restored.state.llm_agent_config.model, "gpt-test");
        assert_eq!(
            restored.state.behavior_tree.selected_node,
            app.state.behavior_tree.selected_node
        );
        assert_eq!(
            restored.state.spacetime_dashboard.reducer_calls,
            app.state.spacetime_dashboard.reducer_calls
        );

        let _ = fs::remove_file(path);
    }
}
