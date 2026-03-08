//! # Prompt or Die — Dedicated Game Server
//!
//! Authoritative game server that:
//! - Owns the canonical World and ticks it
//! - Accepts connections through pod-net runtime mode
//! - Distributes observations to agents
//! - Validates and executes actions
//! - Broadcasts events to connected clients
//!
//! Server is platform-agnostic and knows nothing about rendering,
//! only about game logic and network I/O.

#![allow(clippy::unnecessary_cast)]
#![allow(clippy::wildcard_in_or_patterns)]

use log::{error, info, warn};
use pod_core::{IdleAgent, World};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// INLINE MODULE DEFINITIONS
// ============================================================================

#[allow(dead_code)]
mod config {
    /// Server configuration
    #[derive(Clone, Debug)]
    pub struct ServerConfig {
        /// Address to bind to (e.g., "0.0.0.0:7777")
        pub bind_address: String,
        /// Whether to expose the WebSocket fallback for browser direct-connect clients.
        pub enable_websocket: bool,
        /// Port for the WebSocket fallback endpoint.
        pub websocket_port: u16,
        /// Maximum number of concurrent clients
        pub max_clients: usize,
        /// Target tick rate in Hz (e.g., 60)
        pub tick_rate: usize,
        /// Seed for deterministic world generation
        pub world_seed: u64,
        /// Map name to load
        pub map_name: String,
        /// Runtime mode: "local" (in-process loop) or "network" (pod-net QUIC server)
        pub runtime_mode: String,
    }

    impl ServerConfig {
        pub fn from_env() -> Self {
            // In a real application, you'd parse command-line args or env vars.
            // For now, use defaults or simple environment variable override.
            let bind_address =
                std::env::var("POD_BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:7777".to_string());
            let default_websocket_port = bind_address
                .rsplit_once(':')
                .and_then(|(_, port)| port.parse::<u16>().ok())
                .and_then(|port| port.checked_add(1))
                .unwrap_or(7778);

            let tick_rate = std::env::var("POD_TICK_RATE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(60);

            let max_clients = std::env::var("POD_MAX_CLIENTS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(32);

            let world_seed = std::env::var("POD_WORLD_SEED")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(42);

            let map_name = std::env::var("POD_MAP_NAME").unwrap_or_else(|_| "default".to_string());
            let runtime_mode =
                std::env::var("POD_RUNTIME_MODE").unwrap_or_else(|_| "network".to_string());
            let enable_websocket = std::env::var("POD_ENABLE_WEBSOCKET")
                .ok()
                .and_then(|value| match value.to_ascii_lowercase().as_str() {
                    "1" | "true" | "yes" | "on" => Some(true),
                    "0" | "false" | "no" | "off" => Some(false),
                    _ => None,
                })
                .unwrap_or_else(|| runtime_mode.eq_ignore_ascii_case("network"));
            let websocket_port = std::env::var("POD_WEBSOCKET_PORT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(default_websocket_port);

            ServerConfig {
                bind_address,
                enable_websocket,
                websocket_port,
                tick_rate,
                max_clients,
                world_seed,
                map_name,
                runtime_mode,
            }
        }
    }
}

#[allow(dead_code)]
mod map {
    use pod_core::{
        EncounterSpawnEntry, RegionEncounterTable, Team, World, WorldChunkDefinition,
        WorldRegionDefinition,
    };

    /// Load a map by name into the world
    pub fn load_default_map(world: &mut World, map_name: &str) {
        match map_name {
            "arena" => load_arena_map(world),
            "default" | "verdant-hollow" | _ => load_verdant_hollow(world),
        }
    }

    fn load_arena_map(world: &mut World) {
        use log::info;
        use pod_core::component::Team;

        info!("Loading arena map...");

        // ====== BOUNDARIES ======
        // Top wall
        world
            .spawn_at(250.0, -50.0)
            .with_color(500.0, 100.0, [0.3, 0.3, 0.3, 1.0])
            .with_label("Wall-Top", Team::None)
            .build();

        // Bottom wall
        world
            .spawn_at(250.0, 550.0)
            .with_color(500.0, 100.0, [0.3, 0.3, 0.3, 1.0])
            .with_label("Wall-Bottom", Team::None)
            .build();

        // Left wall
        world
            .spawn_at(-50.0, 250.0)
            .with_color(100.0, 500.0, [0.3, 0.3, 0.3, 1.0])
            .with_label("Wall-Left", Team::None)
            .build();

        // Right wall
        world
            .spawn_at(550.0, 250.0)
            .with_color(100.0, 500.0, [0.3, 0.3, 0.3, 1.0])
            .with_label("Wall-Right", Team::None)
            .build();

        // ====== INTERNAL OBSTACLES ======
        // Center pillar
        world
            .spawn_at(250.0, 250.0)
            .with_color(60.0, 60.0, [0.5, 0.5, 0.5, 1.0])
            .with_label("Obstacle-Center", Team::None)
            .build();

        // Top-left obstacle
        world
            .spawn_at(120.0, 100.0)
            .with_color(80.0, 80.0, [0.6, 0.4, 0.2, 1.0])
            .with_label("Obstacle-TL", Team::None)
            .build();

        // Top-right obstacle
        world
            .spawn_at(380.0, 100.0)
            .with_color(80.0, 80.0, [0.6, 0.4, 0.2, 1.0])
            .with_label("Obstacle-TR", Team::None)
            .build();

        // Bottom-left obstacle
        world
            .spawn_at(120.0, 400.0)
            .with_color(80.0, 80.0, [0.6, 0.4, 0.2, 1.0])
            .with_label("Obstacle-BL", Team::None)
            .build();

        // Bottom-right obstacle
        world
            .spawn_at(380.0, 400.0)
            .with_color(80.0, 80.0, [0.6, 0.4, 0.2, 1.0])
            .with_label("Obstacle-BR", Team::None)
            .build();

        info!("Arena map loaded: 4 walls + 5 obstacles");
    }

    fn load_verdant_hollow(world: &mut World) {
        use log::info;

        let mut heart = WorldRegionDefinition::new(
            "verdant-heart",
            "Verdant Heart",
            "verdant-hollow",
        );
        heart.chunk_keys = vec!["-1:-1".into(), "-1:0".into(), "0:-1".into(), "0:0".into()];
        heart.active_quest_graph_ids = vec!["verdant-intro".into(), "tempered-trail".into()];
        heart.dominant_faction_track_id = "verdant-wardens".into();
        heart.encounter_table_ids = vec![
            "verdant-heart-wildlife".into(),
            "verdant-heart-resources".into(),
        ];

        let mut spirewatch =
            WorldRegionDefinition::new("spirewatch", "Spirewatch Rise", "verdant-hollow");
        spirewatch.chunk_keys = vec!["0:1".into(), "1:1".into()];
        spirewatch.active_quest_graph_ids = vec!["spire-attunement".into()];
        spirewatch.dominant_faction_track_id = "ancient-spirekeepers".into();
        spirewatch.encounter_table_ids = vec![
            "spirewatch-encounters".into(),
            "spirewatch-resources".into(),
        ];

        let chunks = vec![
            authored_chunk(
                "-1:-1",
                "verdant-heart",
                "verdant-hollow",
                vec!["-1:0", "0:-1"],
                vec!["verdant-heart-wildlife", "verdant-heart-resources"],
                vec!["verdant-intro"],
                vec!["verdant-wardens"],
            ),
            authored_chunk(
                "-1:0",
                "verdant-heart",
                "verdant-hollow",
                vec!["-1:-1", "0:0"],
                vec!["verdant-heart-wildlife", "verdant-heart-resources"],
                vec!["verdant-intro", "tempered-trail"],
                vec!["verdant-wardens"],
            ),
            authored_chunk(
                "0:-1",
                "verdant-heart",
                "verdant-hollow",
                vec!["-1:-1", "0:0", "0:1"],
                vec!["verdant-heart-wildlife", "verdant-heart-resources"],
                vec!["verdant-intro"],
                vec!["verdant-wardens"],
            ),
            authored_chunk(
                "0:0",
                "verdant-heart",
                "verdant-hollow",
                vec!["-1:0", "0:-1", "0:1", "1:1"],
                vec!["verdant-heart-wildlife", "verdant-heart-resources"],
                vec!["verdant-intro", "tempered-trail"],
                vec!["verdant-wardens"],
            ),
            authored_chunk(
                "0:1",
                "spirewatch",
                "verdant-hollow",
                vec!["0:0", "1:1"],
                vec!["spirewatch-encounters", "spirewatch-resources"],
                vec!["spire-attunement"],
                vec!["ancient-spirekeepers"],
            ),
            authored_chunk(
                "1:1",
                "spirewatch",
                "verdant-hollow",
                vec!["0:0", "0:1"],
                vec!["spirewatch-encounters", "spirewatch-resources"],
                vec!["spire-attunement"],
                vec!["ancient-spirekeepers"],
            ),
        ];

        let encounter_tables = vec![
            encounter_table(
                "verdant-heart-wildlife",
                "verdant-hollow",
                "wildlife",
                3,
                vec![spawn_entry("verdant-lynx", 8, 1, 2)],
            ),
            encounter_table(
                "verdant-heart-resources",
                "verdant-hollow",
                "resources",
                2,
                vec![spawn_entry("copper-vein-resource", 10, 1, 1)],
            ),
            encounter_table(
                "spirewatch-encounters",
                "verdant-hollow",
                "wildlife",
                2,
                vec![spawn_entry("rift-beast", 5, 1, 1)],
            ),
            encounter_table(
                "spirewatch-resources",
                "verdant-hollow",
                "resources",
                1,
                vec![spawn_entry("moonstone-outcrop-resource", 6, 1, 1)],
            ),
        ];

        world.set_streaming_metadata(160.0, chunks, vec![heart, spirewatch], encounter_tables);

        world
            .spawn_at(0.0, -20.0)
            .with_color(54.0, 54.0, [0.18, 0.54, 0.30, 1.0])
            .with_label("canopy tree", Team::None)
            .build();
        world
            .spawn_at(62.0, 68.0)
            .with_color(42.0, 60.0, [0.62, 0.74, 0.84, 1.0])
            .with_label("glass spire", Team::None)
            .build();
        world
            .spawn_at(-48.0, 36.0)
            .with_color(38.0, 38.0, [0.36, 0.30, 0.26, 1.0])
            .with_label("weathered boulder", Team::None)
            .build();
        world
            .spawn_at(28.0, -64.0)
            .with_color(30.0, 42.0, [0.44, 0.30, 0.22, 1.0])
            .with_label("warden totem", Team::None)
            .build();

        info!("Verdant Hollow loaded: streamed flagship region map");
    }

    fn authored_chunk(
        chunk_key: &str,
        region_id: &str,
        biome_id: &str,
        neighbors: Vec<&str>,
        encounter_table_ids: Vec<&str>,
        quest_graph_ids: Vec<&str>,
        faction_track_ids: Vec<&str>,
    ) -> WorldChunkDefinition {
        let mut chunk = WorldChunkDefinition::new(chunk_key, region_id, biome_id);
        chunk.neighbor_chunk_keys = neighbors.into_iter().map(str::to_string).collect();
        chunk.encounter_table_ids = encounter_table_ids.into_iter().map(str::to_string).collect();
        chunk.quest_graph_ids = quest_graph_ids.into_iter().map(str::to_string).collect();
        chunk.faction_track_ids = faction_track_ids.into_iter().map(str::to_string).collect();
        chunk
    }

    fn spawn_entry(
        archetype_id: &str,
        weight: u16,
        min_count: u8,
        max_count: u8,
    ) -> EncounterSpawnEntry {
        EncounterSpawnEntry {
            archetype_id: archetype_id.to_string(),
            weight,
            min_count,
            max_count,
            required_stage_tags: Vec::new(),
            required_reputation_tiers: Vec::new(),
        }
    }

    fn encounter_table(
        table_id: &str,
        biome_id: &str,
        spawn_group: &str,
        ambient_cap: u16,
        entries: Vec<EncounterSpawnEntry>,
    ) -> RegionEncounterTable {
        let mut table = RegionEncounterTable::new(table_id, biome_id, spawn_group, entries);
        table.ambient_cap = ambient_cap;
        table
    }
}

#[allow(dead_code)]
mod stats {
    use pod_core::action::Action;
    use pod_core::telemetry::ToolCallStatus;
    use pod_core::{
        AgentTickRollup, AgentToolCallEvent, FocusedEntityDebugSummary, IncidentSeverity,
        ShardIncidentSummary,
        TelemetryArchive, TelemetryConfig, VersionedTickTelemetry,
    };
    use std::collections::{HashMap, VecDeque};
    use std::time::Instant;

    const ROLLUP_WINDOW_TICKS: u64 = 60;
    const INCIDENT_EMIT_INTERVAL_TICKS: u64 = 60;

    /// Server statistics tracker
    #[derive(Debug)]
    pub struct ServerStats {
        pub target_tick_rate: u32,
        pub ticks_completed: u64,
        pub last_second_start: Instant,
        pub ticks_this_second: u32,
        pub total_actions: usize,
        pub total_actions_rejected: usize,
        pub peak_entity_count: usize,
        pub peak_agent_count: usize,
        pub tick_budget_overruns: u64,
        pub total_tool_calls: usize,
        pub total_tool_call_errors: usize,
        pub total_tool_latency_ms: u64,
        pub total_trajectory_distance: f32,
        pub total_agents_sampled: usize,
        pub capture_actions: usize,
        pub summon_actions: usize,
        pub gather_actions: usize,
        pub loot_actions: usize,
    }

    impl ServerStats {
        /// Create new stats tracker
        pub fn new(target_tick_rate: u32) -> Self {
            Self {
                target_tick_rate,
                ticks_completed: 0,
                last_second_start: Instant::now(),
                ticks_this_second: 0,
                total_actions: 0,
                total_actions_rejected: 0,
                peak_entity_count: 0,
                peak_agent_count: 0,
                tick_budget_overruns: 0,
                total_tool_calls: 0,
                total_tool_call_errors: 0,
                total_tool_latency_ms: 0,
                total_trajectory_distance: 0.0,
                total_agents_sampled: 0,
                capture_actions: 0,
                summon_actions: 0,
                gather_actions: 0,
                loot_actions: 0,
            }
        }

        /// Record a completed tick
        pub fn record_tick(
            &mut self,
            tick_result: &pod_core::tick::TickResult,
            agent_count: usize,
            tick_over_budget: bool,
        ) {
            self.ticks_completed += 1;
            self.ticks_this_second += 1;
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
                    if !matches!(trace.stage, pod_core::ActionLifecycleStage::Executed) {
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

        /// Print periodic stats (called once per second)
        pub fn print_periodic(&mut self, world: &pod_core::World) {
            use log::info;

            let elapsed = self.last_second_start.elapsed().as_secs_f32();
            let tps = self.ticks_this_second as f32 / elapsed.max(0.001);
            let target = self.target_tick_rate as f32;
            let efficiency = (tps / target * 100.0).min(100.0);

            info!(
                "[STATS] Tick: {:<10} | TPS: {:.1}/{:.0} ({:.0}%) | Entities: {:<4} | Agents: {:<2} | Actions: {:<5} | Reject: {:.1}% | ToolErr: {:.1}% | ToolMs: {:.1} | Path: {:.2} | MMO C/S/G/L: {}/{}/{}/{}",
                world.tick,
                tps,
                target,
                efficiency,
                world.entity_count(),
                world.agent_count(),
                self.total_actions,
                self.action_rejection_rate() * 100.0,
                self.tool_call_error_rate() * 100.0,
                self.average_tool_latency_ms(),
                self.average_trajectory_distance(),
                self.capture_actions,
                self.summon_actions,
                self.gather_actions,
                self.loot_actions,
            );

            self.last_second_start = Instant::now();
            self.ticks_this_second = 0;
        }

        /// Print final shutdown stats
        pub fn print_final(&self) {
            use log::info;

            info!("═══════════════════════════════════════════════════════════");
            info!("FINAL SERVER STATISTICS");
            info!("═══════════════════════════════════════════════════════════");
            info!("Total ticks:          {}", self.ticks_completed);
            info!("Total actions:        {}", self.total_actions);
            info!("Rejected actions:     {}", self.total_actions_rejected);
            info!("Peak entity count:    {}", self.peak_entity_count);
            info!("Peak agent count:     {}", self.peak_agent_count);
            info!("Target tick rate:     {} Hz", self.target_tick_rate);
            info!(
                "Tick overruns:        {} ({:.1}%)",
                self.tick_budget_overruns,
                self.tick_budget_overrun_rate() * 100.0
            );
            info!(
                "Tool calls/errors:    {}/{} ({:.1}%)",
                self.total_tool_calls,
                self.total_tool_call_errors,
                self.tool_call_error_rate() * 100.0
            );
            info!(
                "Avg tool latency:     {:.1} ms",
                self.average_tool_latency_ms()
            );
            info!(
                "Avg path distance:    {:.2}",
                self.average_trajectory_distance()
            );
            info!(
                "MMO loop C/S/G/L:     {}/{}/{}/{}",
                self.capture_actions, self.summon_actions, self.gather_actions, self.loot_actions
            );
            info!("═══════════════════════════════════════════════════════════");
        }
    }

    /// Live TOON document stream emitted by the authoritative server runtime.
    ///
    /// This keeps browser/editor/ops consumers on the same document path as the
    /// in-memory authoritative server loop rather than only the direct-connect
    /// or synthetic test paths.
    #[derive(Debug, Clone)]
    pub struct ShardOpsDebugStream {
        shard_id: String,
        archive: TelemetryArchive,
        pending_documents: VecDeque<String>,
    }

    impl ShardOpsDebugStream {
        pub fn new(shard_id: impl Into<String>) -> Self {
            Self {
                shard_id: shard_id.into(),
                archive: TelemetryArchive::with_capacity(
                    TelemetryConfig::default().core_archive_ticks,
                ),
                pending_documents: VecDeque::new(),
            }
        }

        pub fn record_tick(
            &mut self,
            tick_result: &pod_core::tick::TickResult,
            stats: &ServerStats,
        ) {
            self.archive.record_tick(tick_result.telemetry.clone());
            self.pending_documents.push_back(
                VersionedTickTelemetry::new(tick_result.telemetry.clone()).to_toon_document(),
            );

            for agent in &tick_result.telemetry.agents {
                let Some(entity_id) = agent.entity_id else {
                    continue;
                };
                for trace in &agent.tool_calls {
                    self.pending_documents.push_back(
                        AgentToolCallEvent::new(entity_id.0, trace.clone()).to_toon_document(),
                    );
                }
            }

            if (tick_result.tick + 1) % ROLLUP_WINDOW_TICKS == 0 {
                for rollup in self.rollups_for_tick(tick_result.tick) {
                    self.pending_documents.push_back(rollup.to_toon_document());
                }
            }

            let incident = stats.incident_summary(self.shard_id.clone(), tick_result.tick);
            let emit_incident = !matches!(incident.severity, IncidentSeverity::Healthy)
                || (tick_result.tick + 1) % INCIDENT_EMIT_INTERVAL_TICKS == 0;
            if emit_incident {
                self.pending_documents
                    .push_back(incident.to_toon_document());
            }
        }

        pub fn drain_documents(&mut self) -> Vec<String> {
            self.pending_documents.drain(..).collect()
        }

        pub fn focused_entity_summary(&self, entity_id: u64) -> Option<FocusedEntityDebugSummary> {
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

            for frame in self.archive.frames() {
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
                        .filter(|trace| {
                            matches!(trace.stage, pod_core::ActionLifecycleStage::Rejected)
                        })
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
                shard_id: self.shard_id.clone(),
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

        pub fn focused_entity_document(&self, entity_id: u64) -> Option<String> {
            self.focused_entity_summary(entity_id)
                .map(|summary| summary.to_toon_document())
        }

        fn rollups_for_tick(&self, tick_end: u64) -> Vec<AgentTickRollup> {
            let tick_start = tick_end.saturating_sub(ROLLUP_WINDOW_TICKS - 1);
            let mut frames_by_agent = HashMap::<u64, Vec<pod_core::AgentTelemetryFrame>>::new();

            for frame in self
                .archive
                .frames()
                .iter()
                .filter(|frame| frame.tick >= tick_start && frame.tick <= tick_end)
            {
                for agent in &frame.agents {
                    let Some(entity_id) = agent.entity_id else {
                        continue;
                    };
                    frames_by_agent
                        .entry(entity_id.0)
                        .or_default()
                        .push(agent.clone());
                }
            }

            let mut entity_ids: Vec<u64> = frames_by_agent.keys().copied().collect();
            entity_ids.sort_unstable();

            entity_ids
                .into_iter()
                .filter_map(|entity_id| {
                    frames_by_agent
                        .get(&entity_id)
                        .and_then(|frames| AgentTickRollup::from_agent_frames(entity_id, frames))
                })
                .collect()
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{ServerStats, ShardOpsDebugStream};
        use glam::Vec2;
        use pod_core::acceptance::{run_flagship_mmo_acceptance, FlagshipMmoAcceptanceConfig};
        use pod_core::action::Action;
        use pod_core::agent::AgentType;
        use pod_core::contract::AgentRuntimeProfile;
        use pod_core::id::{AgentId, EntityId};
        use pod_core::telemetry::{
            ActionLifecycleStage, ActionSource, AgentTelemetryFrame, AgentToolCallTrace,
            TickTelemetryFrame, ToolCallStatus, TrajectorySample,
        };
        use pod_core::{decode_toon_value, IncidentSeverity};

        fn sample_tick_at(tick: u64) -> pod_core::tick::TickResult {
            let agent_id = AgentId::new();
            let mut agent = AgentTelemetryFrame::new(
                tick,
                agent_id,
                Some(EntityId(41)),
                AgentRuntimeProfile::for_agent_type(AgentType::LlmAgent),
                3,
                1,
                2,
                4,
                1,
                None,
                Some(TrajectorySample::new(
                    tick,
                    tick as f32 / 60.0,
                    Vec2::ZERO,
                    Vec2::ZERO,
                    0.0,
                )),
            );
            agent.update_trajectory_end(TrajectorySample::new(
                tick,
                tick as f32 / 60.0 + 1.0 / 60.0,
                Vec2::new(3.0, 4.0),
                Vec2::ZERO,
                0.0,
            ));
            agent.record_action(
                ActionSource::AgentDecision,
                ActionLifecycleStage::Executed,
                Action::CaptureCreature {
                    target: pod_core::EntityId(9),
                    tool_slot: Some(0),
                },
                None,
            );
            agent.record_action(
                ActionSource::AgentDecision,
                ActionLifecycleStage::Executed,
                Action::GatherResource {
                    target: pod_core::EntityId(10),
                    skill: pod_core::component::SkillKind::Mining,
                },
                None,
            );
            agent.record_tool_call(AgentToolCallTrace::new(
                tick,
                "llm.complete",
                "mock",
                ToolCallStatus::ParseError,
                42,
                120,
                0,
                Some("bad json".into()),
            ));

            pod_core::tick::TickResult {
                tick,
                events: vec![],
                entity_count: 12,
                actions_processed: 3,
                actions_rejected: 1,
                telemetry: TickTelemetryFrame {
                    tick,
                    agents: vec![agent],
                },
            }
        }

        fn sample_tick() -> pod_core::tick::TickResult {
            sample_tick_at(7)
        }

        #[test]
        fn record_tick_accumulates_mmo_and_tool_metrics() {
            let mut stats = ServerStats::new(60);
            let tick = sample_tick();
            stats.record_tick(&tick, 5, true);

            assert_eq!(stats.total_actions, 3);
            assert_eq!(stats.total_actions_rejected, 1);
            assert_eq!(stats.peak_entity_count, 12);
            assert_eq!(stats.peak_agent_count, 5);
            assert_eq!(stats.tick_budget_overruns, 1);
            assert_eq!(stats.total_tool_calls, 1);
            assert_eq!(stats.total_tool_call_errors, 1);
            assert_eq!(stats.capture_actions, 1);
            assert_eq!(stats.gather_actions, 1);
            assert!((stats.average_trajectory_distance() - 5.0).abs() < f32::EPSILON);
            assert!(stats.action_rejection_rate() > 0.0);
            assert!(stats.tool_call_error_rate() > 0.0);
        }

        #[test]
        fn flagship_acceptance_metrics_align_with_server_stats() {
            let result = run_flagship_mmo_acceptance(FlagshipMmoAcceptanceConfig::ci_smoke())
                .expect("acceptance scenario should run");

            let mut stats = ServerStats::new(60);
            for tick in &result.tick_results {
                stats.record_tick(tick, result.summary.total_agents, false);
            }

            assert_eq!(stats.capture_actions, result.summary.capture_actions);
            assert_eq!(stats.summon_actions, result.summary.summon_actions);
            assert_eq!(stats.gather_actions, result.summary.gather_actions);
            assert_eq!(stats.loot_actions, result.summary.loot_actions);
            assert_eq!(stats.total_tool_calls, result.summary.tool_calls);
            assert_eq!(
                stats.total_tool_call_errors,
                result.summary.tool_call_errors
            );
            assert_eq!(stats.ticks_completed, result.summary.ticks_completed);
            assert!(result.parity_passed());
        }

        #[test]
        fn incident_summaries_export_to_toon_for_ops_agents() {
            let mut stats = ServerStats::new(60);
            let tick = sample_tick();
            stats.record_tick(&tick, 5, true);
            let summary = stats.incident_summary("overworld-a", tick.tick);
            assert_eq!(summary.severity, IncidentSeverity::Warning);

            let document = summary.to_toon_document();
            let value = decode_toon_value(&document).expect("incident summary should decode");
            assert_eq!(value["document_type"], "shard_incident_summary");
            assert_eq!(value["payload"]["shard_id"], "overworld-a");
            assert_eq!(value["payload"]["latest_tick"], 7);
        }

        #[test]
        fn shard_ops_debug_stream_emits_live_toon_documents() {
            let mut stats = ServerStats::new(60);
            let mut stream = ShardOpsDebugStream::new("overworld-a");

            for tick in 0..60 {
                let tick_result = sample_tick_at(tick);
                stats.record_tick(&tick_result, 5, false);
                stream.record_tick(&tick_result, &stats);
            }

            let documents = stream.drain_documents();
            assert!(documents.iter().any(|document| decode_toon_value(document)
                .map(|value| value["document_type"] == "versioned_tick_telemetry")
                .unwrap_or(false)));
            assert!(documents.iter().any(|document| decode_toon_value(document)
                .map(|value| value["document_type"] == "agent_tool_call_event")
                .unwrap_or(false)));
            assert!(documents.iter().any(|document| decode_toon_value(document)
                .map(|value| value["document_type"] == "agent_tick_rollup")
                .unwrap_or(false)));
            assert!(documents.iter().any(|document| decode_toon_value(document)
                .map(|value| value["document_type"] == "shard_incident_summary")
                .unwrap_or(false)));
        }

        #[test]
        fn shard_ops_debug_stream_builds_focused_entity_debug_documents() {
            let mut stats = ServerStats::new(60);
            let mut stream = ShardOpsDebugStream::new("overworld-a");

            for tick in 0..6 {
                let tick_result = sample_tick_at(tick);
                stats.record_tick(&tick_result, 5, false);
                stream.record_tick(&tick_result, &stats);
            }

            let summary = stream
                .focused_entity_summary(41)
                .expect("focused summary should exist");
            assert_eq!(summary.entity_id, 41);
            assert_eq!(summary.shard_id, "overworld-a");
            assert!(summary.tool_call_count >= 1);
            assert!(summary.total_distance > 0.0);

            let document = stream
                .focused_entity_document(41)
                .expect("focused document should exist");
            let value = decode_toon_value(&document).expect("focused document should decode");
            assert_eq!(value["document_type"], "focused_entity_debug_summary");
            assert_eq!(value["payload"]["entity_id"], 41);
            assert_eq!(value["payload"]["shard_id"], "overworld-a");
        }
    }
}

use config::ServerConfig;
use map::load_default_map;
use stats::ServerStats;

// ============================================================================
// MAIN ENTRY POINT
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Initialize logging
    if let Err(e) = env_logger::builder()
        .target(env_logger::Target::Stdout)
        .format_timestamp_millis()
        .try_init()
    {
        eprintln!("Warning: logging already initialized: {}", e);
    }

    // Parse configuration
    let config = ServerConfig::from_env();

    // Print banner
    print_banner(&config);

    // Setup graceful shutdown
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_ctrlc = Arc::clone(&shutdown_flag);

    ctrlc::set_handler(move || {
        info!("Received SIGINT, shutting down gracefully...");
        shutdown_flag_ctrlc.store(true, Ordering::SeqCst);
    })?;

    // Initialize world
    info!("Initializing world with seed={}", config.world_seed);
    let mut world = World::new(config.world_seed as u64);

    // Load the default map
    info!("Loading map: {}", config.map_name);
    load_default_map(&mut world, &config.map_name);

    // Add some initial AI agents for testing
    info!("Spawning initial NPCs...");
    for i in 0..3 {
        let agent = Box::new(IdleAgent::new());
        world.add_agent(agent);
        info!("Spawned NPC #{}", i + 1);
    }

    // Initialize server stats
    let mut stats = ServerStats::new(config.tick_rate as u32);

    let result = if config.runtime_mode.eq_ignore_ascii_case("network") {
        run_network_server(world, &config).await
    } else {
        run_game_loop(&mut world, &config, &mut stats, &shutdown_flag).await
    };

    // Print final stats and shutdown message
    if let Err(e) = result {
        error!("Game loop error: {}", e);
        std::process::exit(1);
    }

    info!("Server stopped cleanly");
    if !config.runtime_mode.eq_ignore_ascii_case("network") {
        stats.print_final();
    }

    Ok(())
}

// ============================================================================
// GAME LOOP
// ============================================================================

async fn run_game_loop(
    world: &mut World,
    config: &ServerConfig,
    stats: &mut ServerStats,
    shutdown_flag: &Arc<AtomicBool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tick_duration = Duration::from_secs_f32(1.0 / config.tick_rate as f32);
    let mut last_stats_print = Instant::now();

    info!(
        "Starting game loop: tick_rate={} Hz, bind={}",
        config.tick_rate, config.bind_address
    );
    info!(
        "Entities: {}, Agents: {}, Max clients: {}",
        world.entity_count(),
        world.agent_count(),
        config.max_clients
    );

    loop {
        // Check for shutdown signal
        if shutdown_flag.load(Ordering::SeqCst) {
            break;
        }

        let tick_start = Instant::now();

        // ====== PHASE 1: ACCEPT NEW CONNECTIONS ======
        process_local_connection_ingress(world, config);

        // ====== PHASE 2: TICK THE WORLD ======
        let tick_result = world.step();

        // ====== PHASE 3: BROADCAST EVENTS TO CLIENTS ======
        broadcast_local_tick_update(&tick_result);

        // ====== PHASE 4: RECORD STATS ======
        let tick_elapsed = tick_start.elapsed();
        let tick_over_budget = tick_elapsed > tick_duration;
        stats.record_tick(&tick_result, world.agent_count(), tick_over_budget);

        // ====== PHASE 5: PERIODIC LOGGING ======
        if last_stats_print.elapsed() >= Duration::from_secs(1) {
            stats.print_periodic(world);
            last_stats_print = Instant::now();
        }

        // ====== PHASE 6: SLEEP TO MAINTAIN TICK RATE ======
        if tick_elapsed < tick_duration {
            tokio::time::sleep(tick_duration - tick_elapsed).await;
        } else {
            warn!(
                "Tick {} took {:.2}ms (over budget by {:.2}ms)",
                world.tick,
                tick_elapsed.as_secs_f32() * 1000.0,
                (tick_elapsed - tick_duration).as_secs_f32() * 1000.0
            );
        }
    }

    Ok(())
}

fn process_local_connection_ingress(_world: &mut World, _config: &ServerConfig) {
    // Local mode intentionally runs without external clients.
}

fn broadcast_local_tick_update(_tick_result: &pod_core::tick::TickResult) {
    // Local mode does not publish network updates.
}

async fn run_network_server(
    world: World,
    config: &ServerConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (bind_addr, bind_port) = parse_bind_target(&config.bind_address)?;
    let net_config = pod_net::protocol::ServerConfig {
        max_clients: config.max_clients,
        tick_rate: config.tick_rate as u32,
        snapshot_interval: 10,
        bind_addr,
        bind_port,
        enable_websocket: config.enable_websocket,
        websocket_port: config.websocket_port,
    };

    let mut server = pod_net::GameServer::new(net_config, world);
    server
        .initialize()
        .await
        .map_err(|err| std::io::Error::other(err.to_string()))?;
    server
        .run()
        .await
        .map_err(|err| std::io::Error::other(err.to_string()))?;
    Ok(())
}

fn parse_bind_target(
    bind: &str,
) -> Result<(String, u16), Box<dyn std::error::Error + Send + Sync>> {
    let mut parts = bind.split(':');
    let host = parts.next().unwrap_or("0.0.0.0").to_string();
    let port = parts
        .next()
        .ok_or_else(|| format!("Invalid bind address '{bind}', expected host:port"))?
        .parse::<u16>()?;
    Ok((host, port))
}

// ============================================================================
// BANNER & FORMATTING
// ============================================================================

fn print_banner(config: &ServerConfig) {
    let banner = r#"
╔════════════════════════════════════════════════════════════╗
║        POD-SERVER — Prompt or Die Game Engine              ║
║                                                            ║
║              Dedicated Authoritative Server                ║
╚════════════════════════════════════════════════════════════╝
"#;

    println!("{}", banner);
    println!(
        r#"
Configuration:
  Bind Address:   {}
  WebSocket:      {}{}
  Tick Rate:      {} Hz
  Max Clients:    {}
  World Seed:     {}
  Map:            {}
  Runtime Mode:   {}

"#,
        config.bind_address,
        if config.enable_websocket {
            "enabled"
        } else {
            "disabled"
        },
        if config.enable_websocket {
            format!(" (port {})", config.websocket_port)
        } else {
            String::new()
        },
        config.tick_rate,
        config.max_clients,
        config.world_seed,
        config.map_name,
        config.runtime_mode
    );
}

#[cfg(test)]
mod runtime_tests {
    use super::{config::ServerConfig, map::load_default_map, parse_bind_target};
    use pod_core::{IdleAgent, World};

    #[test]
    fn parse_bind_target_splits_host_and_port() {
        let (host, port) = parse_bind_target("127.0.0.1:7000").expect("bind target should parse");
        assert_eq!(host, "127.0.0.1");
        assert_eq!(port, 7000);
    }

    #[test]
    fn server_config_defaults_websocket_to_bind_port_plus_one_in_network_mode() {
        let original_bind = std::env::var_os("POD_BIND_ADDRESS");
        let original_runtime = std::env::var_os("POD_RUNTIME_MODE");
        let original_ws_enabled = std::env::var_os("POD_ENABLE_WEBSOCKET");
        let original_ws_port = std::env::var_os("POD_WEBSOCKET_PORT");

        std::env::set_var("POD_BIND_ADDRESS", "127.0.0.1:8123");
        std::env::set_var("POD_RUNTIME_MODE", "network");
        std::env::remove_var("POD_ENABLE_WEBSOCKET");
        std::env::remove_var("POD_WEBSOCKET_PORT");

        let config = ServerConfig::from_env();
        assert!(config.enable_websocket);
        assert_eq!(config.websocket_port, 8124);

        restore_var("POD_BIND_ADDRESS", original_bind);
        restore_var("POD_RUNTIME_MODE", original_runtime);
        restore_var("POD_ENABLE_WEBSOCKET", original_ws_enabled);
        restore_var("POD_WEBSOCKET_PORT", original_ws_port);
    }

    #[test]
    fn default_map_seeds_streamed_population_for_authoritative_worlds() {
        let mut world = World::new(7);
        load_default_map(&mut world, "default");
        world.add_agent(Box::new(IdleAgent::new()));
        world.reconcile_streaming_population();

        let population = world.population_state();
        assert!(!population.regions.is_empty());
        assert!(!population.chunks.is_empty());
        assert!(
            population
                .chunks
                .iter()
                .any(|chunk| chunk.counts.wild_creatures > 0 || chunk.counts.resource_nodes > 0)
        );
    }

    fn restore_var(key: &str, value: Option<std::ffi::OsString>) {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }
}
