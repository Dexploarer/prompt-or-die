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

            ServerConfig {
                bind_address,
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
    use pod_core::World;

    /// Load a map by name into the world
    pub fn load_default_map(world: &mut World, map_name: &str) {
        match map_name {
            "default" | _ => load_arena_map(world),
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
}

#[allow(dead_code)]
mod stats {
    use pod_core::action::Action;
    use pod_core::telemetry::ToolCallStatus;
    use std::time::Instant;

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

    #[cfg(test)]
    mod tests {
        use super::ServerStats;
        use glam::Vec2;
        use pod_core::acceptance::{run_flagship_mmo_acceptance, FlagshipMmoAcceptanceConfig};
        use pod_core::action::Action;
        use pod_core::agent::AgentType;
        use pod_core::contract::AgentRuntimeProfile;
        use pod_core::id::AgentId;
        use pod_core::telemetry::{
            ActionLifecycleStage, ActionSource, AgentTelemetryFrame, AgentToolCallTrace,
            TickTelemetryFrame, ToolCallStatus, TrajectorySample,
        };

        fn sample_tick() -> pod_core::tick::TickResult {
            let agent_id = AgentId::new();
            let mut agent = AgentTelemetryFrame::new(
                7,
                agent_id,
                None,
                AgentRuntimeProfile::for_agent_type(AgentType::LlmAgent),
                3,
                1,
                2,
                4,
                1,
                None,
                Some(TrajectorySample::new(7, 0.0, Vec2::ZERO, Vec2::ZERO, 0.0)),
            );
            agent.update_trajectory_end(TrajectorySample::new(
                7,
                1.0 / 60.0,
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
                7,
                "llm.complete",
                "mock",
                ToolCallStatus::ParseError,
                42,
                120,
                0,
                Some("bad json".into()),
            ));

            pod_core::tick::TickResult {
                tick: 7,
                events: vec![],
                entity_count: 12,
                actions_processed: 3,
                actions_rejected: 1,
                telemetry: TickTelemetryFrame {
                    tick: 7,
                    agents: vec![agent],
                },
            }
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
        enable_websocket: false,
        websocket_port: 0,
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
  Tick Rate:      {} Hz
  Max Clients:    {}
  World Seed:     {}
  Map:            {}
  Runtime Mode:   {}

"#,
        config.bind_address,
        config.tick_rate,
        config.max_clients,
        config.world_seed,
        config.map_name,
        config.runtime_mode
    );
}
