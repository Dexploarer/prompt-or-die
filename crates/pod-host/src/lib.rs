use std::collections::BTreeSet;

use pod_core::{build_authoritative_world, AuthorityWorldConfig, World};

pub use pod_net::{parse_bind_target, DirectConnectTransportConfig, TransportPolicy};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityTransportMode {
    Local,
    DirectConnect,
}

impl AuthorityTransportMode {
    pub fn from_env_value(value: &str) -> Self {
        if value.eq_ignore_ascii_case("network")
            || value.eq_ignore_ascii_case("direct-connect")
            || value.eq_ignore_ascii_case("direct_connect")
        {
            Self::DirectConnect
        } else {
            Self::Local
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::DirectConnect => "network",
        }
    }

    pub fn uses_direct_connect(self) -> bool {
        matches!(self, Self::DirectConnect)
    }
}

#[derive(Clone, Debug)]
pub struct AuthorityHostConfig {
    pub tick_rate: usize,
    pub world: AuthorityWorldConfig,
    pub transport_mode: AuthorityTransportMode,
    pub direct_connect: DirectConnectTransportConfig,
}

impl AuthorityHostConfig {
    pub fn from_env() -> Self {
        let tick_rate = std::env::var("POD_TICK_RATE")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(60);
        let world = AuthorityWorldConfig::from_env();
        let transport_mode = std::env::var("POD_RUNTIME_MODE")
            .map(|value| AuthorityTransportMode::from_env_value(&value))
            .unwrap_or(AuthorityTransportMode::DirectConnect);
        let direct_connect = DirectConnectTransportConfig::from_env();

        Self {
            tick_rate,
            world,
            transport_mode,
            direct_connect,
        }
    }

    pub fn build_world<F>(&self, load_map: F) -> World
    where
        F: FnOnce(&mut World, &str),
    {
        build_authoritative_world(&self.world, load_map)
    }

    pub fn uses_direct_connect(&self) -> bool {
        self.transport_mode.uses_direct_connect()
    }

    pub fn prepare_runtime<F>(
        &self,
        load_map: F,
    ) -> Result<AuthorityHostRuntime, AuthorityHostError>
    where
        F: FnOnce(&mut World, &str),
    {
        let world = self.build_world(load_map);

        if self.uses_direct_connect() {
            let runtime = DirectConnectAuthorityRuntime::new(self, world)?;
            Ok(AuthorityHostRuntime::DirectConnect(runtime))
        } else {
            Ok(AuthorityHostRuntime::Local { world })
        }
    }

    pub fn bind_address(&self) -> &str {
        &self.direct_connect.bind_address
    }

    pub fn enable_websocket(&self) -> bool {
        self.direct_connect.enable_websocket
    }

    pub fn websocket_port(&self) -> u16 {
        self.direct_connect.websocket_port
    }

    pub fn max_clients(&self) -> usize {
        self.direct_connect.max_clients
    }

    pub fn transport_policy(&self) -> &TransportPolicy {
        &self.direct_connect.transport_policy
    }
}

#[derive(Clone, Debug)]
pub struct AuthorityShardConfig {
    pub shard_id: String,
    pub linked_shard_ids: Vec<String>,
    pub host: AuthorityHostConfig,
}

impl AuthorityShardConfig {
    pub fn summary(&self) -> AuthorityShardSummary {
        AuthorityShardSummary {
            shard_id: self.shard_id.clone(),
            linked_shard_ids: self.linked_shard_ids.clone(),
            transport_mode: self.host.transport_mode,
            tick_rate: self.host.tick_rate,
            world_seed: self.host.world.world_seed,
            map_name: self.host.world.map_name.clone(),
            initial_idle_agents: self.host.world.initial_idle_agents,
            direct_connect_bind_address: self
                .host
                .uses_direct_connect()
                .then(|| self.host.bind_address().to_string()),
            direct_connect_websocket_port: self
                .host
                .uses_direct_connect()
                .then(|| self.host.websocket_port()),
            direct_connect_max_clients: self
                .host
                .uses_direct_connect()
                .then(|| self.host.max_clients()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityShardSummary {
    pub shard_id: String,
    pub linked_shard_ids: Vec<String>,
    pub transport_mode: AuthorityTransportMode,
    pub tick_rate: usize,
    pub world_seed: u64,
    pub map_name: String,
    pub initial_idle_agents: usize,
    pub direct_connect_bind_address: Option<String>,
    pub direct_connect_websocket_port: Option<u16>,
    pub direct_connect_max_clients: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub struct ShardSupervisorConfig {
    pub shards: Vec<AuthorityShardConfig>,
}

impl ShardSupervisorConfig {
    pub fn summary(&self) -> Result<ShardSupervisorSummary, ShardSupervisorError> {
        self.validate()?;

        let shards = self
            .shards
            .iter()
            .map(AuthorityShardConfig::summary)
            .collect::<Vec<_>>();
        let direct_connect_shard_count = shards
            .iter()
            .filter(|summary| summary.transport_mode.uses_direct_connect())
            .count();
        let total_direct_connect_capacity = shards
            .iter()
            .filter_map(|summary| summary.direct_connect_max_clients)
            .sum();

        Ok(ShardSupervisorSummary {
            shard_count: shards.len(),
            local_shard_count: shards.len().saturating_sub(direct_connect_shard_count),
            direct_connect_shard_count,
            total_direct_connect_capacity,
            shards,
        })
    }

    pub fn prepare_runtimes<F>(
        &self,
        load_map: F,
    ) -> Result<PreparedShardSupervisor, ShardSupervisorError>
    where
        F: Copy + Fn(&mut World, &str),
    {
        let summary = self.summary()?;
        let shards = self
            .shards
            .iter()
            .map(|shard| {
                let runtime = shard
                    .host
                    .prepare_runtime(|world, map_name| load_map(world, map_name))
                    .map_err(|source| ShardSupervisorError::PrepareRuntime {
                        shard_id: shard.shard_id.clone(),
                        source,
                    })?;
                Ok(PreparedAuthorityShard {
                    summary: shard.summary(),
                    runtime,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PreparedShardSupervisor { summary, shards })
    }

    fn validate(&self) -> Result<(), ShardSupervisorError> {
        if self.shards.is_empty() {
            return Err(ShardSupervisorError::EmptyShardSet);
        }

        let mut shard_ids = BTreeSet::new();
        for shard in &self.shards {
            if !shard_ids.insert(shard.shard_id.clone()) {
                return Err(ShardSupervisorError::DuplicateShardId(
                    shard.shard_id.clone(),
                ));
            }
        }

        for shard in &self.shards {
            for linked_shard_id in &shard.linked_shard_ids {
                if !shard_ids.contains(linked_shard_id) {
                    return Err(ShardSupervisorError::UnknownLinkedShard {
                        shard_id: shard.shard_id.clone(),
                        linked_shard_id: linked_shard_id.clone(),
                    });
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardSupervisorSummary {
    pub shard_count: usize,
    pub local_shard_count: usize,
    pub direct_connect_shard_count: usize,
    pub total_direct_connect_capacity: usize,
    pub shards: Vec<AuthorityShardSummary>,
}

pub enum AuthorityHostRuntime {
    Local { world: World },
    DirectConnect(DirectConnectAuthorityRuntime),
}

pub struct DirectConnectAuthorityRuntime {
    server: pod_net::GameServer,
}

impl DirectConnectAuthorityRuntime {
    pub fn new(config: &AuthorityHostConfig, world: World) -> Result<Self, AuthorityHostError> {
        let server_config = config
            .direct_connect
            .server_config(config.tick_rate)
            .map_err(|err| AuthorityHostError::TransportConfig(err.to_string()))?;

        Ok(Self {
            server: pod_net::GameServer::new(server_config, world),
        })
    }

    pub async fn run(mut self) -> Result<(), AuthorityHostError> {
        self.server
            .initialize()
            .await
            .map_err(|err| AuthorityHostError::Initialize(err.to_string()))?;
        self.server
            .run()
            .await
            .map_err(|err| AuthorityHostError::Run(err.to_string()))?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthorityHostError {
    TransportConfig(String),
    Initialize(String),
    Run(String),
}

impl std::fmt::Display for AuthorityHostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TransportConfig(message) => {
                write!(f, "invalid direct-connect transport config: {message}")
            }
            Self::Initialize(message) => {
                write!(f, "failed to initialize direct-connect runtime: {message}")
            }
            Self::Run(message) => write!(f, "direct-connect runtime failed: {message}"),
        }
    }
}

impl std::error::Error for AuthorityHostError {}

pub struct PreparedAuthorityShard {
    pub summary: AuthorityShardSummary,
    pub runtime: AuthorityHostRuntime,
}

pub struct PreparedShardSupervisor {
    summary: ShardSupervisorSummary,
    shards: Vec<PreparedAuthorityShard>,
}

impl PreparedShardSupervisor {
    pub fn summary(&self) -> &ShardSupervisorSummary {
        &self.summary
    }

    pub fn shards(&self) -> &[PreparedAuthorityShard] {
        &self.shards
    }

    pub async fn run_direct_connect_until_failure(self) -> Result<(), ShardSupervisorError> {
        let local_set = tokio::task::LocalSet::new();

        local_set
            .run_until(async move {
                let (result_tx, mut result_rx) = tokio::sync::mpsc::unbounded_channel::<(
                    String,
                    Result<(), AuthorityHostError>,
                )>();

                for shard in self.shards {
                    let shard_id = shard.summary.shard_id.clone();
                    match shard.runtime {
                        AuthorityHostRuntime::DirectConnect(runtime) => {
                            let result_tx = result_tx.clone();
                            tokio::task::spawn_local(async move {
                                let _ = result_tx.send((shard_id, runtime.run().await));
                            });
                        }
                        AuthorityHostRuntime::Local { .. } => {
                            return Err(ShardSupervisorError::UnsupportedLocalRuntime(shard_id));
                        }
                    }
                }
                drop(result_tx);

                match result_rx.recv().await {
                    Some((shard_id, Ok(()))) => {
                        Err(ShardSupervisorError::ShardExitedUnexpectedly(shard_id))
                    }
                    Some((shard_id, Err(source))) => {
                        Err(ShardSupervisorError::RunRuntime { shard_id, source })
                    }
                    None => Err(ShardSupervisorError::Join(
                        "no shard runtime tasks were launched".to_string(),
                    )),
                }
            })
            .await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShardSupervisorError {
    EmptyShardSet,
    DuplicateShardId(String),
    UnknownLinkedShard {
        shard_id: String,
        linked_shard_id: String,
    },
    PrepareRuntime {
        shard_id: String,
        source: AuthorityHostError,
    },
    UnsupportedLocalRuntime(String),
    ShardExitedUnexpectedly(String),
    RunRuntime {
        shard_id: String,
        source: AuthorityHostError,
    },
    Join(String),
}

impl std::fmt::Display for ShardSupervisorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyShardSet => write!(f, "shard supervisor requires at least one shard"),
            Self::DuplicateShardId(shard_id) => {
                write!(f, "duplicate shard id '{shard_id}' in shard supervisor config")
            }
            Self::UnknownLinkedShard {
                shard_id,
                linked_shard_id,
            } => write!(
                f,
                "shard '{shard_id}' links to unknown shard '{linked_shard_id}'"
            ),
            Self::PrepareRuntime { shard_id, source } => {
                write!(f, "failed to prepare shard '{shard_id}': {source}")
            }
            Self::UnsupportedLocalRuntime(shard_id) => write!(
                f,
                "shard '{shard_id}' uses local runtime; direct-connect supervisor launch only supports network shards"
            ),
            Self::ShardExitedUnexpectedly(shard_id) => {
                write!(f, "shard '{shard_id}' exited unexpectedly")
            }
            Self::RunRuntime { shard_id, source } => {
                write!(f, "shard '{shard_id}' failed while running: {source}")
            }
            Self::Join(message) => write!(f, "shard supervisor task join failed: {message}"),
        }
    }
}

impl std::error::Error for ShardSupervisorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::PrepareRuntime { source, .. } | Self::RunRuntime { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuthorityHostConfig, AuthorityHostRuntime, AuthorityShardConfig, AuthorityTransportMode,
        DirectConnectAuthorityRuntime, PreparedAuthorityShard, ShardSupervisorConfig,
        ShardSupervisorError,
    };
    use pod_core::AuthorityWorldConfig;
    use pod_net::{DirectConnectTransportConfig, TransportPolicy};

    #[test]
    fn authority_host_config_reads_runtime_mode_from_env() {
        let original_tick_rate = std::env::var_os("POD_TICK_RATE");
        let original_runtime_mode = std::env::var_os("POD_RUNTIME_MODE");
        let original_world_seed = std::env::var_os("POD_WORLD_SEED");
        let original_map_name = std::env::var_os("POD_MAP_NAME");
        let original_idle_agents = std::env::var_os("POD_INITIAL_IDLE_AGENTS");

        std::env::set_var("POD_TICK_RATE", "30");
        std::env::set_var("POD_RUNTIME_MODE", "local");
        std::env::set_var("POD_WORLD_SEED", "17");
        std::env::set_var("POD_MAP_NAME", "verdant-hollow");
        std::env::set_var("POD_INITIAL_IDLE_AGENTS", "4");

        let config = AuthorityHostConfig::from_env();
        assert_eq!(config.tick_rate, 30);
        assert_eq!(config.transport_mode, AuthorityTransportMode::Local);
        assert_eq!(config.world.world_seed, 17);
        assert_eq!(config.world.map_name, "verdant-hollow");
        assert_eq!(config.world.initial_idle_agents, 4);

        restore_var("POD_TICK_RATE", original_tick_rate);
        restore_var("POD_RUNTIME_MODE", original_runtime_mode);
        restore_var("POD_WORLD_SEED", original_world_seed);
        restore_var("POD_MAP_NAME", original_map_name);
        restore_var("POD_INITIAL_IDLE_AGENTS", original_idle_agents);
    }

    #[test]
    fn prepare_runtime_returns_local_world_when_local_mode() {
        let config = sample_config(AuthorityTransportMode::Local);
        let runtime = config
            .prepare_runtime(|world, map_name| {
                assert_eq!(map_name, "verdant-hollow");
                world
                    .spawn_at(4.0, 2.0)
                    .with_label("local-bootstrap-marker", pod_core::Team::None)
                    .build();
            })
            .expect("local host runtime should build");

        match runtime {
            AuthorityHostRuntime::Local { world } => {
                assert_eq!(world.agent_count(), 2);
                assert!(world.entity_count() >= 1);
            }
            AuthorityHostRuntime::DirectConnect(_) => {
                panic!("expected local host runtime");
            }
        }
    }

    #[test]
    fn prepare_runtime_returns_direct_connect_runtime_when_network_mode() {
        let config = sample_config(AuthorityTransportMode::DirectConnect);
        let runtime = config
            .prepare_runtime(|_world, _map_name| {})
            .expect("direct-connect runtime should build");

        assert!(matches!(
            runtime,
            AuthorityHostRuntime::DirectConnect(DirectConnectAuthorityRuntime { .. })
        ));
    }

    #[test]
    fn shard_supervisor_summary_reports_capacity_and_links() {
        let config = ShardSupervisorConfig {
            shards: vec![
                AuthorityShardConfig {
                    shard_id: "alpha-1".into(),
                    linked_shard_ids: vec!["alpha-2".into()],
                    host: sample_config(AuthorityTransportMode::DirectConnect),
                },
                AuthorityShardConfig {
                    shard_id: "alpha-2".into(),
                    linked_shard_ids: vec!["alpha-1".into()],
                    host: sample_config(AuthorityTransportMode::Local),
                },
            ],
        };

        let summary = config.summary().expect("summary should build");
        assert_eq!(summary.shard_count, 2);
        assert_eq!(summary.direct_connect_shard_count, 1);
        assert_eq!(summary.local_shard_count, 1);
        assert_eq!(summary.total_direct_connect_capacity, 32);
        assert_eq!(
            summary.shards[0].linked_shard_ids,
            vec!["alpha-2".to_string()]
        );
        assert_eq!(
            summary.shards[0].direct_connect_bind_address.as_deref(),
            Some("127.0.0.1:7000")
        );
        assert_eq!(summary.shards[1].direct_connect_bind_address, None);
    }

    #[test]
    fn shard_supervisor_rejects_duplicate_and_unknown_links() {
        let duplicate = ShardSupervisorConfig {
            shards: vec![
                AuthorityShardConfig {
                    shard_id: "alpha-1".into(),
                    linked_shard_ids: vec![],
                    host: sample_config(AuthorityTransportMode::Local),
                },
                AuthorityShardConfig {
                    shard_id: "alpha-1".into(),
                    linked_shard_ids: vec![],
                    host: sample_config(AuthorityTransportMode::DirectConnect),
                },
            ],
        };
        assert_eq!(
            duplicate.summary(),
            Err(ShardSupervisorError::DuplicateShardId("alpha-1".into()))
        );

        let unknown_link = ShardSupervisorConfig {
            shards: vec![AuthorityShardConfig {
                shard_id: "alpha-1".into(),
                linked_shard_ids: vec!["alpha-2".into()],
                host: sample_config(AuthorityTransportMode::Local),
            }],
        };
        assert_eq!(
            unknown_link.summary(),
            Err(ShardSupervisorError::UnknownLinkedShard {
                shard_id: "alpha-1".into(),
                linked_shard_id: "alpha-2".into(),
            })
        );
    }

    #[test]
    fn shard_supervisor_prepares_multiple_local_worlds() {
        let config = ShardSupervisorConfig {
            shards: vec![
                AuthorityShardConfig {
                    shard_id: "alpha-1".into(),
                    linked_shard_ids: vec!["alpha-2".into()],
                    host: sample_config(AuthorityTransportMode::Local),
                },
                AuthorityShardConfig {
                    shard_id: "alpha-2".into(),
                    linked_shard_ids: vec!["alpha-1".into()],
                    host: sample_config(AuthorityTransportMode::Local),
                },
            ],
        };

        let prepared = config
            .prepare_runtimes(|world, map_name| {
                world
                    .spawn_at(0.0, 0.0)
                    .with_label(map_name, pod_core::Team::None)
                    .build();
            })
            .expect("supervisor should prepare local worlds");

        assert_eq!(prepared.summary().shard_count, 2);
        assert!(prepared
            .shards()
            .iter()
            .all(|shard| matches!(shard.runtime, AuthorityHostRuntime::Local { .. })));
    }

    #[tokio::test]
    async fn shard_supervisor_run_rejects_local_runtimes() {
        let prepared = super::PreparedShardSupervisor {
            summary: super::ShardSupervisorSummary {
                shard_count: 1,
                local_shard_count: 1,
                direct_connect_shard_count: 0,
                total_direct_connect_capacity: 0,
                shards: vec![AuthorityShardConfig {
                    shard_id: "alpha-1".into(),
                    linked_shard_ids: vec![],
                    host: sample_config(AuthorityTransportMode::Local),
                }
                .summary()],
            },
            shards: vec![PreparedAuthorityShard {
                summary: AuthorityShardConfig {
                    shard_id: "alpha-1".into(),
                    linked_shard_ids: vec![],
                    host: sample_config(AuthorityTransportMode::Local),
                }
                .summary(),
                runtime: AuthorityHostRuntime::Local {
                    world: sample_config(AuthorityTransportMode::Local)
                        .build_world(|_world, _map_name| {}),
                },
            }],
        };

        assert_eq!(
            prepared.run_direct_connect_until_failure().await,
            Err(ShardSupervisorError::UnsupportedLocalRuntime(
                "alpha-1".into()
            ))
        );
    }

    fn sample_config(transport_mode: AuthorityTransportMode) -> AuthorityHostConfig {
        AuthorityHostConfig {
            tick_rate: 60,
            world: AuthorityWorldConfig {
                world_seed: 7,
                map_name: "verdant-hollow".to_string(),
                initial_idle_agents: 2,
            },
            transport_mode,
            direct_connect: DirectConnectTransportConfig {
                bind_address: "127.0.0.1:7000".to_string(),
                enable_websocket: true,
                websocket_port: 7001,
                max_clients: 32,
                transport_policy: TransportPolicy::default(),
            },
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
