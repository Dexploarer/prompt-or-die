//! # Prompt or Die — Dedicated Game Server
//!
//! Authoritative game server that:
//! - Owns the canonical World and ticks it
//! - Accepts connections (TODO: when pod-net is ready)
//! - Distributes observations to agents
//! - Validates and executes actions
//! - Broadcasts events to connected clients
//!
//! Server is platform-agnostic and knows nothing about rendering,
//! only about game logic and network I/O.

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
    }

    impl ServerConfig {
        pub fn from_env() -> Self {
            // In a real application, you'd parse command-line args or env vars.
            // For now, use defaults or simple environment variable override.
            let bind_address = std::env::var("POD_BIND_ADDRESS")
                .unwrap_or_else(|_| "0.0.0.0:7777".to_string());

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

            let map_name = std::env::var("POD_MAP_NAME")
                .unwrap_or_else(|_| "default".to_string());

            ServerConfig {
                bind_address,
                tick_rate,
                max_clients,
                world_seed,
                map_name,
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
    use std::time::Instant;

    /// Server statistics tracker
    #[derive(Debug)]
    pub struct ServerStats {
        pub target_tick_rate: u32,
        pub ticks_completed: u64,
        pub last_second_start: Instant,
        pub ticks_this_second: u32,
        pub total_actions: usize,
        pub peak_entity_count: usize,
        pub peak_agent_count: usize,
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
                peak_entity_count: 0,
                peak_agent_count: 0,
            }
        }

        /// Record a completed tick
        pub fn record_tick(&mut self, entity_count: usize, actions_processed: usize) {
            self.ticks_completed += 1;
            self.ticks_this_second += 1;
            self.total_actions += actions_processed;
            self.peak_entity_count = self.peak_entity_count.max(entity_count);
        }

        /// Print periodic stats (called once per second)
        pub fn print_periodic(&self, world: &pod_core::World) {
            use log::info;

            let elapsed = self.last_second_start.elapsed().as_secs_f32();
            let tps = self.ticks_this_second as f32 / elapsed.max(0.001);
            let target = self.target_tick_rate as f32;
            let efficiency = (tps / target * 100.0).min(100.0);

            info!(
                "[STATS] Tick: {:<10} | TPS: {:.1}/{:.0} ({:.0}%) | Entities: {:<4} | Agents: {:<2} | Actions: {:<5}",
                world.tick,
                tps,
                target,
                efficiency,
                world.entity_count(),
                world.agent_count(),
                self.total_actions
            );
        }

        /// Print final shutdown stats
        pub fn print_final(&self) {
            use log::info;

            info!("═══════════════════════════════════════════════════════════");
            info!("FINAL SERVER STATISTICS");
            info!("═══════════════════════════════════════════════════════════");
            info!("Total ticks:          {}", self.ticks_completed);
            info!("Total actions:        {}", self.total_actions);
            info!("Peak entity count:    {}", self.peak_entity_count);
            info!("Peak agent count:     {}", self.peak_agent_count);
            info!("Target tick rate:     {} Hz", self.target_tick_rate);
            info!("═══════════════════════════════════════════════════════════");
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

    // Run the main game loop
    let result = run_game_loop(&mut world, &config, &mut stats, &shutdown_flag).await;

    // Print final stats and shutdown message
    if let Err(e) = result {
        error!("Game loop error: {}", e);
        std::process::exit(1);
    }

    info!("Server stopped cleanly");
    stats.print_final();

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
        // TODO: When pod-net is integrated, accept connection attempts here.
        // For now, this is a placeholder.
        // Example:
        //   if let Ok(Some(new_client)) = accept_pending_connection() {
        //       world.add_agent(new_client.into());
        //   }

        // ====== PHASE 2: TICK THE WORLD ======
        let tick_result = world.step();

        // ====== PHASE 3: BROADCAST EVENTS TO CLIENTS ======
        // TODO: When pod-net is integrated, serialize tick_result and send to all
        // connected clients. Example:
        //   for client in &mut connected_clients {
        //       client.send_tick_update(&tick_result)?;
        //   }

        // ====== PHASE 4: RECORD STATS ======
        stats.record_tick(tick_result.entity_count, tick_result.actions_processed);

        // ====== PHASE 5: PERIODIC LOGGING ======
        if last_stats_print.elapsed() >= Duration::from_secs(1) {
            stats.print_periodic(world);
            last_stats_print = Instant::now();
        }

        // ====== PHASE 6: SLEEP TO MAINTAIN TICK RATE ======
        let elapsed = tick_start.elapsed();
        if elapsed < tick_duration {
            tokio::time::sleep(tick_duration - elapsed).await;
        } else {
            warn!(
                "Tick {} took {:.2}ms (over budget by {:.2}ms)",
                world.tick,
                elapsed.as_secs_f32() * 1000.0,
                (elapsed - tick_duration).as_secs_f32() * 1000.0
            );
        }
    }

    Ok(())
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

"#,
        config.bind_address,
        config.tick_rate,
        config.max_clients,
        config.world_seed,
        config.map_name
    );
}
