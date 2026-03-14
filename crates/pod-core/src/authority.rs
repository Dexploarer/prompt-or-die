use crate::World;

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
            world.add_agent(Box::new(crate::IdleAgent::new()));
            log::info!("Spawned bootstrap NPC #{}", index + 1);
        }
    }
}

/// Transport-neutral world/bootstrap configuration for authority hosts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityWorldConfig {
    /// Seed for deterministic world generation.
    pub world_seed: u64,
    /// Map name to load.
    pub map_name: String,
    /// Number of initial idle NPC agents to inject into the authoritative shard.
    pub initial_idle_agents: usize,
}

impl AuthorityWorldConfig {
    pub fn from_env() -> Self {
        let world_seed = std::env::var("POD_WORLD_SEED")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(42);
        let map_name = std::env::var("POD_MAP_NAME").unwrap_or_else(|_| "default".to_string());
        let initial_idle_agents = std::env::var("POD_INITIAL_IDLE_AGENTS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(3);

        Self {
            world_seed,
            map_name,
            initial_idle_agents,
        }
    }

    pub fn world_bootstrap(&self) -> WorldBootstrapPlan {
        WorldBootstrapPlan {
            map_name: self.map_name.clone(),
            initial_idle_agents: self.initial_idle_agents,
        }
    }
}

pub fn build_authoritative_world<F>(config: &AuthorityWorldConfig, load_map: F) -> World
where
    F: FnOnce(&mut World, &str),
{
    log::info!("Initializing world with seed={}", config.world_seed);
    let mut world = World::new(config.world_seed);
    config.world_bootstrap().apply(&mut world, load_map);
    world
}

#[cfg(test)]
mod tests {
    use super::{build_authoritative_world, AuthorityWorldConfig};
    use std::cell::RefCell;

    #[test]
    fn authority_world_config_reads_bootstrap_env_defaults() {
        let original_seed = std::env::var_os("POD_WORLD_SEED");
        let original_map = std::env::var_os("POD_MAP_NAME");
        let original_agents = std::env::var_os("POD_INITIAL_IDLE_AGENTS");

        std::env::set_var("POD_WORLD_SEED", "17");
        std::env::set_var("POD_MAP_NAME", "verdant-hollow");
        std::env::set_var("POD_INITIAL_IDLE_AGENTS", "5");

        let config = AuthorityWorldConfig::from_env();
        assert_eq!(config.world_seed, 17);
        assert_eq!(config.map_name, "verdant-hollow");
        assert_eq!(config.initial_idle_agents, 5);

        restore_var("POD_WORLD_SEED", original_seed);
        restore_var("POD_MAP_NAME", original_map);
        restore_var("POD_INITIAL_IDLE_AGENTS", original_agents);
    }

    #[test]
    fn build_authoritative_world_applies_bootstrap_contract() {
        let config = AuthorityWorldConfig {
            world_seed: 7,
            map_name: "default".to_string(),
            initial_idle_agents: 3,
        };
        let loaded_map = RefCell::new(None::<String>);

        let world = build_authoritative_world(&config, |world, map_name| {
            *loaded_map.borrow_mut() = Some(map_name.to_string());
            world
                .spawn_at(0.0, 0.0)
                .with_label("bootstrap-marker", crate::component::Team::None)
                .build();
        });

        assert_eq!(loaded_map.borrow().as_deref(), Some("default"));
        assert_eq!(world.agent_count(), config.initial_idle_agents);
        assert!(world.entity_count() >= 1);
    }

    fn restore_var(key: &str, value: Option<std::ffi::OsString>) {
        if let Some(value) = value {
            std::env::set_var(key, value);
        } else {
            std::env::remove_var(key);
        }
    }
}
