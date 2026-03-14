#![allow(clippy::unnecessary_cast)]
#![allow(clippy::wildcard_in_or_patterns)]

use pod_core::World;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportPolicy {
    pub snapshot_interval: u64,
    pub client_inactivity_timeout_ticks: u64,
    pub queue_pressure_warn_depth: usize,
}

impl Default for TransportPolicy {
    fn default() -> Self {
        Self {
            snapshot_interval: 10,
            client_inactivity_timeout_ticks: 600,
            queue_pressure_warn_depth: 192,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldBootstrapPlan {
    pub map_name: String,
    pub initial_idle_agents: usize,
}

impl WorldBootstrapPlan {
    pub fn apply<F>(&self, world: &mut World, load_map: F)
    where
        F: FnOnce(&mut World, &str),
    {
        log::info!("Loading map: {}", self.map_name);
        load_map(world, &self.map_name);

        log::info!(
            "Applying authoritative bootstrap: {} idle NPCs",
            self.initial_idle_agents
        );
        for index in 0..self.initial_idle_agents {
            world.add_agent(Box::new(pod_core::IdleAgent::new()));
            log::info!("Spawned bootstrap NPC #{}", index + 1);
        }
    }
}

/// Dedicated server configuration shared by the `pod-server` library and binary.
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
    /// Number of initial idle NPC agents to inject into the authoritative shard.
    pub initial_idle_agents: usize,
    /// Runtime mode: "local" (in-process loop) or "network" (pod-net QUIC server)
    pub runtime_mode: String,
    /// Dedicated transport policy composed into the direct-connect server.
    pub transport_policy: TransportPolicy,
}

impl ServerConfig {
    pub fn from_env() -> Self {
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
        let initial_idle_agents = std::env::var("POD_INITIAL_IDLE_AGENTS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);

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
        let defaults = TransportPolicy::default();
        let transport_policy = TransportPolicy {
            snapshot_interval: std::env::var("POD_SNAPSHOT_INTERVAL")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(defaults.snapshot_interval),
            client_inactivity_timeout_ticks: std::env::var("POD_CLIENT_INACTIVITY_TIMEOUT_TICKS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(defaults.client_inactivity_timeout_ticks),
            queue_pressure_warn_depth: std::env::var("POD_QUEUE_PRESSURE_WARN_DEPTH")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(defaults.queue_pressure_warn_depth),
        };

        Self {
            bind_address,
            enable_websocket,
            websocket_port,
            tick_rate,
            max_clients,
            world_seed,
            map_name,
            initial_idle_agents,
            runtime_mode,
            transport_policy,
        }
    }

    pub fn world_bootstrap(&self) -> WorldBootstrapPlan {
        WorldBootstrapPlan {
            map_name: self.map_name.clone(),
            initial_idle_agents: self.initial_idle_agents,
        }
    }

    pub fn network_server_config(
        &self,
    ) -> Result<pod_net::protocol::ServerConfig, Box<dyn std::error::Error + Send + Sync>> {
        let (bind_addr, bind_port) = parse_bind_target(&self.bind_address)?;
        Ok(pod_net::protocol::ServerConfig {
            max_clients: self.max_clients,
            tick_rate: self.tick_rate as u32,
            snapshot_interval: self.transport_policy.snapshot_interval,
            bind_addr,
            bind_port,
            enable_websocket: self.enable_websocket,
            websocket_port: self.websocket_port,
            client_inactivity_timeout_ticks: self.transport_policy.client_inactivity_timeout_ticks,
            queue_pressure_warn_depth: self.transport_policy.queue_pressure_warn_depth,
        })
    }
}

pub fn build_authoritative_world<F>(config: &ServerConfig, load_map: F) -> World
where
    F: FnOnce(&mut World, &str),
{
    log::info!("Initializing world with seed={}", config.world_seed);
    let mut world = World::new(config.world_seed);
    config.world_bootstrap().apply(&mut world, load_map);
    world
}

pub fn parse_bind_target(
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

#[cfg(test)]
mod tests {
    use super::{build_authoritative_world, parse_bind_target, ServerConfig, TransportPolicy};
    use std::cell::RefCell;

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
    fn server_config_builds_network_transport_policy_contract() {
        let original_bind = std::env::var_os("POD_BIND_ADDRESS");
        let original_snapshot_interval = std::env::var_os("POD_SNAPSHOT_INTERVAL");
        let original_timeout = std::env::var_os("POD_CLIENT_INACTIVITY_TIMEOUT_TICKS");
        let original_queue_warn = std::env::var_os("POD_QUEUE_PRESSURE_WARN_DEPTH");

        std::env::set_var("POD_BIND_ADDRESS", "127.0.0.1:8123");
        std::env::set_var("POD_SNAPSHOT_INTERVAL", "24");
        std::env::set_var("POD_CLIENT_INACTIVITY_TIMEOUT_TICKS", "900");
        std::env::set_var("POD_QUEUE_PRESSURE_WARN_DEPTH", "255");

        let config = ServerConfig::from_env();
        let net_config = config
            .network_server_config()
            .expect("network transport policy should compose");

        assert_eq!(net_config.bind_addr, "127.0.0.1");
        assert_eq!(net_config.bind_port, 8123);
        assert_eq!(net_config.snapshot_interval, 24);
        assert_eq!(net_config.client_inactivity_timeout_ticks, 900);
        assert_eq!(net_config.queue_pressure_warn_depth, 255);

        restore_var("POD_BIND_ADDRESS", original_bind);
        restore_var("POD_SNAPSHOT_INTERVAL", original_snapshot_interval);
        restore_var("POD_CLIENT_INACTIVITY_TIMEOUT_TICKS", original_timeout);
        restore_var("POD_QUEUE_PRESSURE_WARN_DEPTH", original_queue_warn);
    }

    #[test]
    fn build_authoritative_world_applies_bootstrap_contract() {
        let config = sample_server_config();
        let loaded_map = RefCell::new(None::<String>);

        let world = build_authoritative_world(&config, |world, map_name| {
            *loaded_map.borrow_mut() = Some(map_name.to_string());
            world
                .spawn_at(0.0, 0.0)
                .with_label("bootstrap-marker", pod_core::component::Team::None)
                .build();
        });

        assert_eq!(loaded_map.borrow().as_deref(), Some("default"));
        assert_eq!(world.agent_count(), config.initial_idle_agents);
        assert!(world.entity_count() >= 1);
    }

    fn sample_server_config() -> ServerConfig {
        ServerConfig {
            bind_address: "127.0.0.1:7777".to_string(),
            enable_websocket: true,
            websocket_port: 7778,
            max_clients: 32,
            tick_rate: 60,
            world_seed: 7,
            map_name: "default".to_string(),
            initial_idle_agents: 3,
            runtime_mode: "network".to_string(),
            transport_policy: TransportPolicy::default(),
        }
    }

    fn restore_var(key: &str, value: Option<std::ffi::OsString>) {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }
}
