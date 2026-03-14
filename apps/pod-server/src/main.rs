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
use pod_core::World;
use pod_host::{AuthorityHostConfig as ServerConfig, AuthorityHostRuntime};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// INLINE MODULE DEFINITIONS
// ============================================================================

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

        let mut heart =
            WorldRegionDefinition::new("verdant-heart", "Verdant Heart", "verdant-hollow");
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
        chunk.encounter_table_ids = encounter_table_ids
            .into_iter()
            .map(str::to_string)
            .collect();
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
    use pod_core::{
        summarize_focused_entity_debug, AgentTickRollup, AgentToolCallEvent,
        FocusedEntityDebugSummary, IncidentSeverity, ShardGameplayIncidentTracker,
        ShardIncidentSummary, TelemetryArchive, TelemetryConfig, VersionedTickTelemetry,
    };
    use std::collections::{HashMap, VecDeque};
    use std::time::Instant;

    const ROLLUP_WINDOW_TICKS: u64 = 60;
    const INCIDENT_EMIT_INTERVAL_TICKS: u64 = 60;

    /// Server statistics tracker
    #[derive(Debug)]
    pub struct ServerStats {
        pub target_tick_rate: u32,
        pub last_second_start: Instant,
        pub ticks_this_second: u32,
        tracker: ShardGameplayIncidentTracker,
    }

    impl ServerStats {
        /// Create new stats tracker
        pub fn new(target_tick_rate: u32) -> Self {
            Self {
                target_tick_rate,
                last_second_start: Instant::now(),
                ticks_this_second: 0,
                tracker: ShardGameplayIncidentTracker::new(),
            }
        }

        /// Record a completed tick
        pub fn record_tick(
            &mut self,
            tick_result: &pod_core::tick::TickResult,
            agent_count: usize,
            tick_over_budget: bool,
        ) {
            self.ticks_this_second += 1;
            self.tracker
                .record_tick(tick_result, agent_count, tick_over_budget);
        }

        pub fn action_rejection_rate(&self) -> f32 {
            self.tracker.action_rejection_rate()
        }

        pub fn tool_call_error_rate(&self) -> f32 {
            self.tracker.tool_call_error_rate()
        }

        pub fn average_tool_latency_ms(&self) -> f32 {
            self.tracker.average_tool_latency_ms()
        }

        pub fn average_trajectory_distance(&self) -> f32 {
            self.tracker.average_trajectory_distance()
        }

        pub fn tick_budget_overrun_rate(&self) -> f32 {
            self.tracker.tick_budget_overrun_rate()
        }

        pub fn incident_summary(
            &self,
            shard_id: impl Into<String>,
            latest_tick: u64,
        ) -> ShardIncidentSummary {
            self.tracker.incident_summary(shard_id, latest_tick)
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
                self.tracker.total_actions(),
                self.action_rejection_rate() * 100.0,
                self.tool_call_error_rate() * 100.0,
                self.average_tool_latency_ms(),
                self.average_trajectory_distance(),
                self.tracker.capture_actions(),
                self.tracker.summon_actions(),
                self.tracker.gather_actions(),
                self.tracker.loot_actions(),
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
            info!("Total ticks:          {}", self.tracker.ticks_completed());
            info!("Total actions:        {}", self.tracker.total_actions());
            info!(
                "Rejected actions:     {}",
                self.tracker.total_actions_rejected()
            );
            info!("Peak entity count:    {}", self.tracker.peak_entity_count());
            info!("Peak agent count:     {}", self.tracker.peak_agent_count());
            info!("Target tick rate:     {} Hz", self.target_tick_rate);
            info!(
                "Tick overruns:        {} ({:.1}%)",
                self.tracker.tick_budget_overruns(),
                self.tick_budget_overrun_rate() * 100.0
            );
            info!(
                "Tool calls/errors:    {}/{} ({:.1}%)",
                self.tracker.total_tool_calls(),
                self.tracker.total_tool_call_errors(),
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
                self.tracker.capture_actions(),
                self.tracker.summon_actions(),
                self.tracker.gather_actions(),
                self.tracker.loot_actions()
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

            if (tick_result.tick + 1).is_multiple_of(ROLLUP_WINDOW_TICKS) {
                for rollup in self.rollups_for_tick(tick_result.tick) {
                    self.pending_documents.push_back(rollup.to_toon_document());
                }
            }

            let incident = stats.incident_summary(self.shard_id.clone(), tick_result.tick);
            let emit_incident = !matches!(incident.severity, IncidentSeverity::Healthy)
                || (tick_result.tick + 1).is_multiple_of(INCIDENT_EMIT_INTERVAL_TICKS);
            if emit_incident {
                self.pending_documents
                    .push_back(incident.to_toon_document());
            }
        }

        pub fn drain_documents(&mut self) -> Vec<String> {
            self.pending_documents.drain(..).collect()
        }

        pub fn focused_entity_summary(&self, entity_id: u64) -> Option<FocusedEntityDebugSummary> {
            summarize_focused_entity_debug(self.shard_id.clone(), &self.archive, entity_id)
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

            assert_eq!(stats.tracker.total_actions(), 3);
            assert_eq!(stats.tracker.total_actions_rejected(), 1);
            assert_eq!(stats.tracker.peak_entity_count(), 12);
            assert_eq!(stats.tracker.peak_agent_count(), 5);
            assert_eq!(stats.tracker.tick_budget_overruns(), 1);
            assert_eq!(stats.tracker.total_tool_calls(), 1);
            assert_eq!(stats.tracker.total_tool_call_errors(), 1);
            assert_eq!(stats.tracker.capture_actions(), 1);
            assert_eq!(stats.tracker.gather_actions(), 1);
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

            assert_eq!(stats.tracker.capture_actions(), result.summary.capture_actions);
            assert_eq!(stats.tracker.summon_actions(), result.summary.summon_actions);
            assert_eq!(stats.tracker.gather_actions(), result.summary.gather_actions);
            assert_eq!(stats.tracker.loot_actions(), result.summary.loot_actions);
            assert_eq!(stats.tracker.total_tool_calls(), result.summary.tool_calls);
            assert_eq!(
                stats.tracker.total_tool_call_errors(),
                result.summary.tool_call_errors
            );
            assert_eq!(stats.tracker.ticks_completed(), result.summary.ticks_completed);
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

    // Initialize server stats
    let mut stats = ServerStats::new(config.tick_rate as u32);
    let result = match config.prepare_runtime(map::load_default_map)? {
        AuthorityHostRuntime::DirectConnect(runtime) => runtime
            .run()
            .await
            .map_err(|err| -> Box<dyn std::error::Error + Send + Sync> { Box::new(err) }),
        AuthorityHostRuntime::Local { mut world } => {
            run_game_loop(&mut world, &config, &mut stats, &shutdown_flag).await
        }
    };

    // Print final stats and shutdown message
    if let Err(e) = result {
        error!("Game loop error: {}", e);
        std::process::exit(1);
    }

    info!("Server stopped cleanly");
    if !config.uses_direct_connect() {
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
        config.tick_rate,
        config.bind_address()
    );
    info!(
        "Entities: {}, Agents: {}, Max clients: {}",
        world.entity_count(),
        world.agent_count(),
        config.max_clients()
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
  Bootstrap NPCs: {}
  Runtime Mode:   {}
  Snapshot Every: {} ticks
  Client Timeout: {} ticks
  Queue Warn:     {}

"#,
        config.bind_address(),
        if config.enable_websocket() {
            "enabled"
        } else {
            "disabled"
        },
        if config.enable_websocket() {
            format!(" (port {})", config.websocket_port())
        } else {
            String::new()
        },
        config.tick_rate,
        config.max_clients(),
        config.world.world_seed,
        config.world.map_name,
        config.world.initial_idle_agents,
        config.transport_mode.as_str(),
        config.transport_policy().snapshot_interval,
        config.transport_policy().client_inactivity_timeout_ticks,
        config.transport_policy().queue_pressure_warn_depth
    );
}
