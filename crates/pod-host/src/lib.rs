use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pod_core::{
    build_authoritative_world, summarize_focused_entity_debug, AgentTickRollup, AgentToolCallEvent,
    AuthorityWorldConfig, FocusedEntityDebugSummary, IncidentSeverity,
    ShardGameplayIncidentTracker, ShardIncidentSummary, ShardTransportSummary, TelemetryArchive,
    TelemetryConfig, VersionedTickTelemetry, World,
};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, watch};

pub use pod_net::{parse_bind_target, DirectConnectTransportConfig, TransportPolicy};
use pod_net::{
    OpsDocumentArchiveSnapshot, OpsDocumentStream, ServerLifecycleCommand, ServerLifecyclePhase,
    ServerLifecycleState,
};

const OPS_DOCUMENT_CHANNEL_CAPACITY: usize = 256;
const OPS_DOCUMENT_HISTORY_LIMIT: usize = 256;
const ROLLUP_WINDOW_TICKS: u64 = 60;
const INCIDENT_EMIT_INTERVAL_TICKS: u64 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    pub ops_persistence: Option<OpsPersistenceConfig>,
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
        let ops_persistence = std::env::var("POD_OPS_ARCHIVE_DIR")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|archive_root_dir| OpsPersistenceConfig {
                archive_root_dir: PathBuf::from(archive_root_dir),
            });

        Self {
            tick_rate,
            world,
            transport_mode,
            direct_connect,
            ops_persistence,
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
        self.prepare_runtime_with_shard_id("direct-connect", load_map)
    }

    pub fn prepare_runtime_with_shard_id<F>(
        &self,
        shard_id: impl Into<String>,
        load_map: F,
    ) -> Result<AuthorityHostRuntime, AuthorityHostError>
    where
        F: FnOnce(&mut World, &str),
    {
        let world = self.build_world(load_map);

        if self.uses_direct_connect() {
            let runtime = DirectConnectAuthorityRuntime::new_with_shard_id(self, world, shard_id)?;
            Ok(AuthorityHostRuntime::DirectConnect(runtime))
        } else {
            Ok(AuthorityHostRuntime::Local(
                LocalAuthorityRuntime::try_new_with_shard_id(self, world, shard_id)?,
            ))
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

    fn build_ops_document_stream(
        &self,
        shard_id: &str,
    ) -> Result<OpsDocumentStream, AuthorityHostError> {
        match &self.ops_persistence {
            Some(config) => OpsDocumentStream::with_persistent_archive(
                OPS_DOCUMENT_HISTORY_LIMIT,
                OPS_DOCUMENT_CHANNEL_CAPACITY,
                config.archive_path_for_shard(shard_id),
            )
            .map_err(|err| {
                AuthorityHostError::OpsPersistence(format!(
                    "failed to open shard ops archive for `{shard_id}`: {err}"
                ))
            }),
            None => Ok(OpsDocumentStream::new(
                OPS_DOCUMENT_HISTORY_LIMIT,
                OPS_DOCUMENT_CHANNEL_CAPACITY,
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpsPersistenceConfig {
    pub archive_root_dir: PathBuf,
}

impl OpsPersistenceConfig {
    pub fn archive_path_for_shard(&self, shard_id: &str) -> PathBuf {
        self.archive_root_dir.join(format!("{shard_id}-ops.jsonl"))
    }

    pub fn archive_root_dir(&self) -> &Path {
        &self.archive_root_dir
    }

    pub fn archive_handle_for_shard(
        &self,
        shard: AuthorityShardSummary,
    ) -> AuthorityShardOpsArchiveHandle {
        let shard_id = shard.shard_id.clone();
        AuthorityShardOpsArchiveHandle {
            shard_id: shard_id.clone(),
            shard: Some(shard),
            archive_path: Some(self.archive_path_for_shard(&shard_id)),
        }
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

    pub fn ops_archive_handle(&self) -> AuthorityShardOpsArchiveHandle {
        let shard = self.summary();
        self.host
            .ops_persistence
            .as_ref()
            .map(|config| config.archive_handle_for_shard(shard.clone()))
            .unwrap_or_else(|| AuthorityShardOpsArchiveHandle {
                shard_id: shard.shard_id.clone(),
                shard: Some(shard),
                archive_path: None,
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
                let shard_summary = shard.summary();
                let runtime = shard
                    .host
                    .prepare_runtime_with_shard_id(&shard.shard_id, |world, map_name| {
                        load_map(world, map_name)
                    })
                    .map_err(|source| ShardSupervisorError::PrepareRuntime {
                        shard_id: shard.shard_id.clone(),
                        source,
                    })?;
                Ok(PreparedAuthorityShard {
                    control_plane: runtime.control_plane_handle(shard_summary.clone()),
                    ops_handle: runtime.ops_handle(shard_summary.clone()),
                    summary: shard_summary,
                    runtime,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(PreparedShardSupervisor { summary, shards })
    }

    pub fn ops_archive_handle(&self) -> ShardSupervisorOpsArchiveHandle {
        ShardSupervisorOpsArchiveHandle {
            shards: self
                .shards
                .iter()
                .map(AuthorityShardConfig::ops_archive_handle)
                .collect(),
        }
    }

    pub fn archive_service(
        &self,
        config: OpsArchiveServiceConfig,
    ) -> ShardSupervisorOpsArchiveService {
        self.ops_archive_handle().service(config)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityShardLifecyclePhase {
    Running,
    Draining,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityShardLifecycleState {
    pub phase: AuthorityShardLifecyclePhase,
    pub accepting_new_connections: bool,
    pub latest_tick: u64,
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityShardLifecycleCommandKind {
    Drain,
    Shutdown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthorityShardControlPlaneCommandError {
    UnsupportedLocalRuntime { shard_id: String },
    CommandChannelClosed { shard_id: String },
}

impl std::fmt::Display for AuthorityShardControlPlaneCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedLocalRuntime { shard_id } => write!(
                f,
                "shard '{shard_id}' uses a local runtime and does not accept direct-connect lifecycle commands"
            ),
            Self::CommandChannelClosed { shard_id } => write!(
                f,
                "shard '{shard_id}' is no longer accepting lifecycle commands"
            ),
        }
    }
}

impl std::error::Error for AuthorityShardControlPlaneCommandError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorityShardCommandRejection {
    pub shard_id: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardSupervisorLifecycleCommandResult {
    pub command: AuthorityShardLifecycleCommandKind,
    pub targeted_shard_count: usize,
    pub accepted_shard_ids: Vec<String>,
    pub rejected: Vec<AuthorityShardCommandRejection>,
}

#[derive(Clone, Debug)]
pub struct AuthorityShardOpsHandle {
    shard: AuthorityShardSummary,
    stream: OpsDocumentStream,
}

impl AuthorityShardOpsHandle {
    pub fn shard(&self) -> &AuthorityShardSummary {
        &self.shard
    }

    pub fn subscribe_documents(&self) -> broadcast::Receiver<String> {
        self.stream.subscribe()
    }

    pub fn recent_documents(&self) -> Vec<String> {
        self.stream.recent_documents()
    }

    pub fn retained_document_count(&self) -> usize {
        self.stream.retained_document_count()
    }

    pub fn persisted_document_count(&self) -> usize {
        self.stream.persisted_document_count()
    }

    pub fn archive_path(&self) -> Option<PathBuf> {
        self.stream.archive_path()
    }

    pub fn snapshot(&self) -> AuthorityShardOpsSnapshot {
        AuthorityShardOpsSnapshot {
            shard: self.shard.clone(),
            retained_document_count: self.retained_document_count(),
            persisted_document_count: self.persisted_document_count(),
            archive_path: self.archive_path(),
            recent_documents: self.recent_documents(),
        }
    }

    pub fn archive_handle(&self) -> AuthorityShardOpsArchiveHandle {
        AuthorityShardOpsArchiveHandle {
            shard_id: self.shard.shard_id.clone(),
            shard: Some(self.shard.clone()),
            archive_path: self.archive_path(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuthorityShardOpsSnapshot {
    pub shard: AuthorityShardSummary,
    pub retained_document_count: usize,
    pub persisted_document_count: usize,
    pub archive_path: Option<PathBuf>,
    pub recent_documents: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct AuthorityShardOpsArchiveHandle {
    shard_id: String,
    shard: Option<AuthorityShardSummary>,
    archive_path: Option<PathBuf>,
}

impl AuthorityShardOpsArchiveHandle {
    pub fn shard_id(&self) -> &str {
        &self.shard_id
    }

    pub fn shard(&self) -> Option<&AuthorityShardSummary> {
        self.shard.as_ref()
    }

    pub fn archive_path(&self) -> Option<&Path> {
        self.archive_path.as_deref()
    }

    pub fn snapshot(
        &self,
        recent_limit: usize,
    ) -> Result<AuthorityShardOpsArchiveSnapshot, AuthorityOpsArchiveError> {
        let Some(archive_path) = self.archive_path.clone() else {
            return Ok(AuthorityShardOpsArchiveSnapshot {
                shard_id: self.shard_id.clone(),
                shard: self.shard.clone(),
                archive_path: None,
                persisted_document_count: 0,
                recent_documents: Vec::new(),
            });
        };

        let snapshot =
            OpsDocumentArchiveSnapshot::load(&archive_path, recent_limit).map_err(|source| {
                AuthorityOpsArchiveError::ReadArchive {
                    shard_id: self.shard_id.clone(),
                    path: archive_path.clone(),
                    source,
                }
            })?;

        Ok(AuthorityShardOpsArchiveSnapshot {
            shard_id: self.shard_id.clone(),
            shard: self.shard.clone(),
            archive_path: Some(snapshot.archive_path),
            persisted_document_count: snapshot.persisted_document_count,
            recent_documents: snapshot.recent_documents,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityShardOpsArchiveSnapshot {
    pub shard_id: String,
    pub shard: Option<AuthorityShardSummary>,
    pub archive_path: Option<PathBuf>,
    pub persisted_document_count: usize,
    pub recent_documents: Vec<String>,
}

#[derive(Debug)]
pub enum AuthorityOpsArchiveError {
    ReadArchive {
        shard_id: String,
        path: PathBuf,
        source: std::io::Error,
    },
}

impl std::fmt::Display for AuthorityOpsArchiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReadArchive {
                shard_id,
                path,
                source,
            } => write!(
                f,
                "failed to read ops archive for shard '{shard_id}' at {}: {source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for AuthorityOpsArchiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ReadArchive { source, .. } => Some(source),
        }
    }
}

#[derive(Clone, Debug)]
pub struct AuthorityShardControlPlaneHandle {
    shard: AuthorityShardSummary,
    transport_summary_rx: Option<watch::Receiver<Option<ShardTransportSummary>>>,
    gameplay_incident_summary_rx: Option<watch::Receiver<Option<ShardIncidentSummary>>>,
    lifecycle_state_rx: Option<watch::Receiver<ServerLifecycleState>>,
    lifecycle_command_tx: Option<mpsc::UnboundedSender<ServerLifecycleCommand>>,
}

impl AuthorityShardControlPlaneHandle {
    pub fn snapshot(&self) -> AuthorityShardControlPlaneSummary {
        let lifecycle_state = self.lifecycle_state();
        let transport_summary = self
            .transport_summary_rx
            .as_ref()
            .and_then(|rx| rx.borrow().clone());
        let gameplay_incident_summary = self
            .gameplay_incident_summary_rx
            .as_ref()
            .and_then(|rx| rx.borrow().clone());
        let mut notes = Vec::new();

        match lifecycle_state.phase {
            AuthorityShardLifecyclePhase::Draining => notes.push(format!(
                "shard is draining{}",
                lifecycle_state
                    .reason
                    .as_deref()
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default()
            )),
            AuthorityShardLifecyclePhase::Stopped => notes.push(format!(
                "shard is stopped{}",
                lifecycle_state
                    .reason
                    .as_deref()
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default()
            )),
            AuthorityShardLifecyclePhase::Running => {}
        }

        match (
            &self.shard.transport_mode,
            &transport_summary,
            lifecycle_state.phase,
        ) {
            (
                AuthorityTransportMode::DirectConnect,
                None,
                AuthorityShardLifecyclePhase::Running,
            ) => {
                notes.push("waiting for first direct-connect transport summary".to_string());
            }
            (AuthorityTransportMode::DirectConnect, None, _) => {}
            (AuthorityTransportMode::Local, None, _) => {
                notes.push(
                    "local runtime does not emit direct-connect transport telemetry".to_string(),
                );
            }
            (_, Some(summary), _) => {
                if summary.queue_pressure_client_count > 0 {
                    notes.push(format!(
                        "{} clients currently exceed queue-pressure depth",
                        summary.queue_pressure_client_count
                    ));
                }
                if summary.timed_out_clients > 0 {
                    notes.push(format!(
                        "{} clients have timed out on this shard",
                        summary.timed_out_clients
                    ));
                }
                if summary.recovery_delivery_failures > 0 {
                    notes.push(format!(
                        "{} recovery snapshot deliveries have failed",
                        summary.recovery_delivery_failures
                    ));
                }
            }
        }

        if let Some(summary) = gameplay_incident_summary.as_ref() {
            notes.extend(summary.notes.iter().cloned());
        }

        let latest_tick = transport_summary
            .as_ref()
            .map(|summary| summary.latest_tick)
            .or((lifecycle_state.latest_tick > 0).then_some(lifecycle_state.latest_tick))
            .or(gameplay_incident_summary
                .as_ref()
                .map(|summary| summary.latest_tick));
        let severity = shard_control_plane_severity(
            &self.shard,
            &lifecycle_state,
            transport_summary.as_ref(),
            gameplay_incident_summary.as_ref(),
        );
        let incident_summary = build_control_plane_incident_summary(
            &self.shard,
            &lifecycle_state,
            latest_tick.unwrap_or_default(),
            severity,
            &notes,
            gameplay_incident_summary.as_ref(),
        );

        AuthorityShardControlPlaneSummary {
            shard: self.shard.clone(),
            severity,
            lifecycle_state,
            latest_tick,
            has_live_transport: transport_summary.is_some(),
            transport_summary,
            gameplay_incident_summary,
            incident_summary,
            notes,
        }
    }

    pub fn request_drain(
        &self,
        reason: impl Into<String>,
    ) -> Result<(), AuthorityShardControlPlaneCommandError> {
        self.send_lifecycle_command(ServerLifecycleCommand::BeginDrain {
            reason: reason.into(),
        })
    }

    pub fn request_shutdown(
        &self,
        reason: impl Into<String>,
    ) -> Result<(), AuthorityShardControlPlaneCommandError> {
        self.send_lifecycle_command(ServerLifecycleCommand::Shutdown {
            reason: reason.into(),
        })
    }

    fn lifecycle_state(&self) -> AuthorityShardLifecycleState {
        self.lifecycle_state_rx
            .as_ref()
            .map(|rx| authority_lifecycle_state_from_server_state(&rx.borrow()))
            .unwrap_or_else(|| AuthorityShardLifecycleState {
                phase: AuthorityShardLifecyclePhase::Running,
                accepting_new_connections: false,
                latest_tick: 0,
                reason: None,
            })
    }

    fn send_lifecycle_command(
        &self,
        command: ServerLifecycleCommand,
    ) -> Result<(), AuthorityShardControlPlaneCommandError> {
        let Some(tx) = self.lifecycle_command_tx.as_ref() else {
            return Err(
                AuthorityShardControlPlaneCommandError::UnsupportedLocalRuntime {
                    shard_id: self.shard.shard_id.clone(),
                },
            );
        };

        tx.send(command).map_err(
            |_| AuthorityShardControlPlaneCommandError::CommandChannelClosed {
                shard_id: self.shard.shard_id.clone(),
            },
        )
    }
}

#[derive(Clone, Debug)]
pub struct AuthorityShardControlPlaneSummary {
    pub shard: AuthorityShardSummary,
    pub severity: IncidentSeverity,
    pub lifecycle_state: AuthorityShardLifecycleState,
    pub latest_tick: Option<u64>,
    pub has_live_transport: bool,
    pub transport_summary: Option<ShardTransportSummary>,
    pub gameplay_incident_summary: Option<ShardIncidentSummary>,
    pub incident_summary: ShardIncidentSummary,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ShardSupervisorControlPlaneHandle {
    shards: Vec<AuthorityShardControlPlaneHandle>,
}

impl ShardSupervisorControlPlaneHandle {
    pub fn snapshot(&self) -> ShardSupervisorControlPlaneSummary {
        let shards = self
            .shards
            .iter()
            .map(AuthorityShardControlPlaneHandle::snapshot)
            .collect::<Vec<_>>();

        ShardSupervisorControlPlaneSummary {
            shard_count: shards.len(),
            healthy_shard_count: shards
                .iter()
                .filter(|summary| summary.severity == IncidentSeverity::Healthy)
                .count(),
            warning_shard_count: shards
                .iter()
                .filter(|summary| summary.severity == IncidentSeverity::Warning)
                .count(),
            critical_shard_count: shards
                .iter()
                .filter(|summary| summary.severity == IncidentSeverity::Critical)
                .count(),
            running_shard_count: shards
                .iter()
                .filter(|summary| {
                    summary.lifecycle_state.phase == AuthorityShardLifecyclePhase::Running
                })
                .count(),
            draining_shard_count: shards
                .iter()
                .filter(|summary| {
                    summary.lifecycle_state.phase == AuthorityShardLifecyclePhase::Draining
                })
                .count(),
            stopped_shard_count: shards
                .iter()
                .filter(|summary| {
                    summary.lifecycle_state.phase == AuthorityShardLifecyclePhase::Stopped
                })
                .count(),
            direct_connect_shard_count: shards
                .iter()
                .filter(|summary| summary.shard.transport_mode.uses_direct_connect())
                .count(),
            local_shard_count: shards
                .iter()
                .filter(|summary| !summary.shard.transport_mode.uses_direct_connect())
                .count(),
            reporting_transport_shard_count: shards
                .iter()
                .filter(|summary| summary.has_live_transport)
                .count(),
            total_client_count: shards
                .iter()
                .filter_map(|summary| summary.transport_summary.as_ref())
                .map(|summary| summary.client_count)
                .sum(),
            total_queue_pressure_client_count: shards
                .iter()
                .filter_map(|summary| summary.transport_summary.as_ref())
                .map(|summary| summary.queue_pressure_client_count)
                .sum(),
            total_timed_out_clients: shards
                .iter()
                .filter_map(|summary| summary.transport_summary.as_ref())
                .map(|summary| summary.timed_out_clients)
                .sum(),
            total_recovery_delivery_failures: shards
                .iter()
                .filter_map(|summary| summary.transport_summary.as_ref())
                .map(|summary| summary.recovery_delivery_failures)
                .sum(),
            latest_tick: shards
                .iter()
                .filter_map(|summary| summary.latest_tick)
                .max(),
            shards,
        }
    }

    pub fn request_drain_all(
        &self,
        reason: impl Into<String>,
    ) -> ShardSupervisorLifecycleCommandResult {
        self.broadcast_lifecycle_command(AuthorityShardLifecycleCommandKind::Drain, reason.into())
    }

    pub fn request_shutdown_all(
        &self,
        reason: impl Into<String>,
    ) -> ShardSupervisorLifecycleCommandResult {
        self.broadcast_lifecycle_command(
            AuthorityShardLifecycleCommandKind::Shutdown,
            reason.into(),
        )
    }

    fn broadcast_lifecycle_command(
        &self,
        command: AuthorityShardLifecycleCommandKind,
        reason: String,
    ) -> ShardSupervisorLifecycleCommandResult {
        let mut accepted_shard_ids = Vec::new();
        let mut rejected = Vec::new();

        for shard in &self.shards {
            let result = match command {
                AuthorityShardLifecycleCommandKind::Drain => shard.request_drain(reason.clone()),
                AuthorityShardLifecycleCommandKind::Shutdown => {
                    shard.request_shutdown(reason.clone())
                }
            };

            match result {
                Ok(()) => accepted_shard_ids.push(shard.shard.shard_id.clone()),
                Err(err) => rejected.push(AuthorityShardCommandRejection {
                    shard_id: shard.shard.shard_id.clone(),
                    reason: err.to_string(),
                }),
            }
        }

        ShardSupervisorLifecycleCommandResult {
            command,
            targeted_shard_count: self.shards.len(),
            accepted_shard_ids,
            rejected,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ShardSupervisorControlPlaneSummary {
    pub shard_count: usize,
    pub healthy_shard_count: usize,
    pub warning_shard_count: usize,
    pub critical_shard_count: usize,
    pub running_shard_count: usize,
    pub draining_shard_count: usize,
    pub stopped_shard_count: usize,
    pub direct_connect_shard_count: usize,
    pub local_shard_count: usize,
    pub reporting_transport_shard_count: usize,
    pub total_client_count: usize,
    pub total_queue_pressure_client_count: usize,
    pub total_timed_out_clients: u64,
    pub total_recovery_delivery_failures: u64,
    pub latest_tick: Option<u64>,
    pub shards: Vec<AuthorityShardControlPlaneSummary>,
}

#[derive(Clone, Debug)]
pub struct ShardSupervisorOpsHandle {
    shards: Vec<AuthorityShardOpsHandle>,
}

impl ShardSupervisorOpsHandle {
    pub fn shards(&self) -> &[AuthorityShardOpsHandle] {
        &self.shards
    }

    pub fn shard(&self, shard_id: &str) -> Option<&AuthorityShardOpsHandle> {
        self.shards
            .iter()
            .find(|handle| handle.shard.shard_id == shard_id)
    }

    pub fn snapshot(&self) -> ShardSupervisorOpsSnapshot {
        let shards = self
            .shards
            .iter()
            .map(AuthorityShardOpsHandle::snapshot)
            .collect::<Vec<_>>();

        ShardSupervisorOpsSnapshot {
            shard_count: shards.len(),
            total_retained_document_count: shards
                .iter()
                .map(|summary| summary.retained_document_count)
                .sum(),
            total_persisted_document_count: shards
                .iter()
                .map(|summary| summary.persisted_document_count)
                .sum(),
            shards,
        }
    }

    pub fn archive_handle(&self) -> ShardSupervisorOpsArchiveHandle {
        ShardSupervisorOpsArchiveHandle {
            shards: self
                .shards
                .iter()
                .map(AuthorityShardOpsHandle::archive_handle)
                .collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ShardSupervisorOpsSnapshot {
    pub shard_count: usize,
    pub total_retained_document_count: usize,
    pub total_persisted_document_count: usize,
    pub shards: Vec<AuthorityShardOpsSnapshot>,
}

#[derive(Clone, Debug)]
pub struct ShardSupervisorOpsArchiveHandle {
    shards: Vec<AuthorityShardOpsArchiveHandle>,
}

impl ShardSupervisorOpsArchiveHandle {
    pub fn shards(&self) -> &[AuthorityShardOpsArchiveHandle] {
        &self.shards
    }

    pub fn shard(&self, shard_id: &str) -> Option<&AuthorityShardOpsArchiveHandle> {
        self.shards
            .iter()
            .find(|handle| handle.shard_id() == shard_id)
    }

    pub fn snapshot(
        &self,
        recent_limit_per_shard: usize,
    ) -> Result<ShardSupervisorOpsArchiveSnapshot, AuthorityOpsArchiveError> {
        let shards = self
            .shards
            .iter()
            .map(|handle| handle.snapshot(recent_limit_per_shard))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ShardSupervisorOpsArchiveSnapshot {
            shard_count: shards.len(),
            archived_shard_count: shards
                .iter()
                .filter(|snapshot| snapshot.archive_path.is_some())
                .count(),
            total_persisted_document_count: shards
                .iter()
                .map(|snapshot| snapshot.persisted_document_count)
                .sum(),
            shards,
        })
    }

    pub fn service(&self, config: OpsArchiveServiceConfig) -> ShardSupervisorOpsArchiveService {
        ShardSupervisorOpsArchiveService {
            archive: self.clone(),
            config,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardSupervisorOpsArchiveSnapshot {
    pub shard_count: usize,
    pub archived_shard_count: usize,
    pub total_persisted_document_count: usize,
    pub shards: Vec<AuthorityShardOpsArchiveSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpsArchiveServiceConfig {
    pub bind_address: String,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
}

impl Default for OpsArchiveServiceConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:7610".to_string(),
            max_request_bytes: 64 * 1024,
            max_response_bytes: 512 * 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpsArchiveServiceClient {
    pub address: String,
    pub max_response_bytes: usize,
}

impl Default for OpsArchiveServiceClient {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:7610".to_string(),
            max_response_bytes: 512 * 1024,
        }
    }
}

impl OpsArchiveServiceClient {
    pub async fn query(
        &self,
        request: OpsArchiveServiceRequest,
    ) -> Result<OpsArchiveServiceResponse, OpsArchiveServiceError> {
        let mut socket = TcpStream::connect(&self.address).await.map_err(|source| {
            OpsArchiveServiceError::Connect {
                address: self.address.clone(),
                source,
            }
        })?;
        let encoded =
            serde_json::to_vec(&request).map_err(OpsArchiveServiceError::EncodeRequest)?;
        socket
            .write_all(&encoded)
            .await
            .map_err(OpsArchiveServiceError::WriteRequest)?;
        socket
            .shutdown()
            .await
            .map_err(OpsArchiveServiceError::WriteRequest)?;

        let response_bytes = match read_capped(&mut socket, self.max_response_bytes).await {
            Ok(bytes) => bytes,
            Err(ReadCappedError::Io(source)) => {
                return Err(OpsArchiveServiceError::ReadResponse(source))
            }
            Err(ReadCappedError::TooLarge) => {
                return Err(OpsArchiveServiceError::ResponseTooLarge {
                    max_response_bytes: self.max_response_bytes,
                })
            }
        };
        let response = serde_json::from_slice::<OpsArchiveServiceResponse>(&response_bytes)
            .map_err(OpsArchiveServiceError::DecodeResponse)?;
        match response {
            OpsArchiveServiceResponse::Error { message } => {
                Err(OpsArchiveServiceError::Remote { message })
            }
            response => Ok(response),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpsArchiveServiceRequest {
    Shard {
        shard_id: String,
        recent_limit: usize,
    },
    Supervisor {
        recent_limit_per_shard: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpsArchiveServiceResponse {
    Shard(AuthorityShardOpsArchiveSnapshot),
    Supervisor(ShardSupervisorOpsArchiveSnapshot),
    Error { message: String },
}

#[derive(Clone, Debug)]
pub struct ShardSupervisorOpsArchiveService {
    archive: ShardSupervisorOpsArchiveHandle,
    config: OpsArchiveServiceConfig,
}

impl ShardSupervisorOpsArchiveService {
    pub async fn serve(self) -> Result<(), OpsArchiveServiceError> {
        let listener = self.bind_listener().await?;
        self.serve_listener(listener).await
    }

    pub async fn serve_once(self) -> Result<(), OpsArchiveServiceError> {
        let listener = self.bind_listener().await?;
        self.serve_once_listener(listener).await
    }

    async fn bind_listener(&self) -> Result<TcpListener, OpsArchiveServiceError> {
        TcpListener::bind(&self.config.bind_address)
            .await
            .map_err(|source| OpsArchiveServiceError::Bind {
                address: self.config.bind_address.clone(),
                source,
            })
    }

    async fn serve_listener(self, listener: TcpListener) -> Result<(), OpsArchiveServiceError> {
        loop {
            let (mut socket, _) = listener
                .accept()
                .await
                .map_err(OpsArchiveServiceError::Accept)?;
            self.handle_socket(&mut socket).await?;
        }
    }

    async fn serve_once_listener(
        self,
        listener: TcpListener,
    ) -> Result<(), OpsArchiveServiceError> {
        let (mut socket, _) = listener
            .accept()
            .await
            .map_err(OpsArchiveServiceError::Accept)?;
        self.handle_socket(&mut socket).await
    }

    async fn handle_socket<S>(&self, socket: &mut S) -> Result<(), OpsArchiveServiceError>
    where
        S: AsyncRead + AsyncWrite + Unpin,
    {
        let response = match read_capped(socket, self.config.max_request_bytes).await {
            Ok(request_bytes) => {
                match serde_json::from_slice::<OpsArchiveServiceRequest>(&request_bytes) {
                    Ok(request) => match self.query(request) {
                        Ok(response) => response,
                        Err(err) => OpsArchiveServiceResponse::Error {
                            message: err.to_string(),
                        },
                    },
                    Err(err) => OpsArchiveServiceResponse::Error {
                        message: format!("failed to decode ops archive request: {err}"),
                    },
                }
            }
            Err(ReadCappedError::Io(source)) => {
                return Err(OpsArchiveServiceError::ReadRequest(source))
            }
            Err(ReadCappedError::TooLarge) => OpsArchiveServiceResponse::Error {
                message: format!(
                    "ops archive request exceeded {} bytes",
                    self.config.max_request_bytes
                ),
            },
        };

        self.write_response(socket, response).await
    }

    async fn write_response<S>(
        &self,
        socket: &mut S,
        response: OpsArchiveServiceResponse,
    ) -> Result<(), OpsArchiveServiceError>
    where
        S: AsyncWrite + Unpin,
    {
        let encoded =
            serde_json::to_vec(&response).map_err(OpsArchiveServiceError::EncodeResponse)?;
        if encoded.len() > self.config.max_response_bytes {
            return Err(OpsArchiveServiceError::ResponseTooLarge {
                max_response_bytes: self.config.max_response_bytes,
            });
        }

        socket
            .write_all(&encoded)
            .await
            .map_err(OpsArchiveServiceError::WriteResponse)?;
        socket
            .shutdown()
            .await
            .map_err(OpsArchiveServiceError::WriteResponse)?;
        Ok(())
    }

    fn query(
        &self,
        request: OpsArchiveServiceRequest,
    ) -> Result<OpsArchiveServiceResponse, OpsArchiveServiceError> {
        match request {
            OpsArchiveServiceRequest::Shard {
                shard_id,
                recent_limit,
            } => {
                let shard = self.archive.shard(&shard_id).ok_or_else(|| {
                    OpsArchiveServiceError::UnknownShard {
                        shard_id: shard_id.clone(),
                    }
                })?;
                let snapshot = shard
                    .snapshot(recent_limit)
                    .map_err(OpsArchiveServiceError::ArchiveQuery)?;
                Ok(OpsArchiveServiceResponse::Shard(snapshot))
            }
            OpsArchiveServiceRequest::Supervisor {
                recent_limit_per_shard,
            } => {
                let snapshot = self
                    .archive
                    .snapshot(recent_limit_per_shard)
                    .map_err(OpsArchiveServiceError::ArchiveQuery)?;
                Ok(OpsArchiveServiceResponse::Supervisor(snapshot))
            }
        }
    }
}

#[derive(Debug)]
pub enum OpsArchiveServiceError {
    Bind {
        address: String,
        source: std::io::Error,
    },
    Accept(std::io::Error),
    Connect {
        address: String,
        source: std::io::Error,
    },
    ReadRequest(std::io::Error),
    ReadResponse(std::io::Error),
    WriteRequest(std::io::Error),
    WriteResponse(std::io::Error),
    EncodeRequest(serde_json::Error),
    EncodeResponse(serde_json::Error),
    DecodeResponse(serde_json::Error),
    ResponseTooLarge {
        max_response_bytes: usize,
    },
    UnknownShard {
        shard_id: String,
    },
    Remote {
        message: String,
    },
    ArchiveQuery(AuthorityOpsArchiveError),
}

impl std::fmt::Display for OpsArchiveServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bind { address, source } => {
                write!(
                    f,
                    "failed to bind ops archive service on {address}: {source}"
                )
            }
            Self::Accept(source) => {
                write!(f, "failed to accept ops archive service client: {source}")
            }
            Self::Connect { address, source } => {
                write!(
                    f,
                    "failed to connect to ops archive service at {address}: {source}"
                )
            }
            Self::ReadRequest(source) => {
                write!(f, "failed to read ops archive service request: {source}")
            }
            Self::ReadResponse(source) => {
                write!(f, "failed to read ops archive service response: {source}")
            }
            Self::WriteRequest(source) => {
                write!(f, "failed to write ops archive service request: {source}")
            }
            Self::WriteResponse(source) => {
                write!(f, "failed to write ops archive service response: {source}")
            }
            Self::EncodeRequest(source) => {
                write!(f, "failed to encode ops archive service request: {source}")
            }
            Self::EncodeResponse(source) => {
                write!(f, "failed to encode ops archive service response: {source}")
            }
            Self::DecodeResponse(source) => {
                write!(f, "failed to decode ops archive service response: {source}")
            }
            Self::ResponseTooLarge { max_response_bytes } => write!(
                f,
                "ops archive service response exceeded {max_response_bytes} bytes"
            ),
            Self::UnknownShard { shard_id } => {
                write!(f, "unknown shard '{shard_id}'")
            }
            Self::Remote { message } => {
                write!(f, "ops archive service returned an error: {message}")
            }
            Self::ArchiveQuery(source) => write!(f, "{source}"),
        }
    }
}

impl std::error::Error for OpsArchiveServiceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bind { source, .. }
            | Self::Accept(source)
            | Self::Connect { source, .. }
            | Self::ReadRequest(source)
            | Self::ReadResponse(source)
            | Self::WriteRequest(source)
            | Self::WriteResponse(source) => Some(source),
            Self::EncodeRequest(source)
            | Self::EncodeResponse(source)
            | Self::DecodeResponse(source) => Some(source),
            Self::ArchiveQuery(source) => Some(source),
            Self::ResponseTooLarge { .. } | Self::UnknownShard { .. } | Self::Remote { .. } => None,
        }
    }
}

#[derive(Debug)]
enum ReadCappedError {
    Io(std::io::Error),
    TooLarge,
}

async fn read_capped<R>(reader: &mut R, max_bytes: usize) -> Result<Vec<u8>, ReadCappedError>
where
    R: AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];

    loop {
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(ReadCappedError::Io)?;
        if read == 0 {
            break;
        }
        if bytes.len() + read > max_bytes {
            return Err(ReadCappedError::TooLarge);
        }
        bytes.extend_from_slice(&buffer[..read]);
    }

    Ok(bytes)
}

fn authority_lifecycle_state_from_server_state(
    state: &ServerLifecycleState,
) -> AuthorityShardLifecycleState {
    AuthorityShardLifecycleState {
        phase: match state.phase {
            ServerLifecyclePhase::Running => AuthorityShardLifecyclePhase::Running,
            ServerLifecyclePhase::Draining => AuthorityShardLifecyclePhase::Draining,
            ServerLifecyclePhase::Stopped => AuthorityShardLifecyclePhase::Stopped,
        },
        accepting_new_connections: state.accepting_new_connections,
        latest_tick: state.latest_tick,
        reason: state.reason.clone(),
    }
}

fn shard_control_plane_severity(
    shard: &AuthorityShardSummary,
    lifecycle_state: &AuthorityShardLifecycleState,
    transport_summary: Option<&ShardTransportSummary>,
    gameplay_incident_summary: Option<&ShardIncidentSummary>,
) -> IncidentSeverity {
    if matches!(
        gameplay_incident_summary,
        Some(summary) if summary.severity == IncidentSeverity::Critical
    ) {
        return IncidentSeverity::Critical;
    }

    if matches!(
        transport_summary,
        Some(summary) if summary.recovery_delivery_failures > 0
    ) {
        return IncidentSeverity::Critical;
    }

    if matches!(
        transport_summary,
        Some(summary)
            if summary.queue_pressure_client_count > 0 || summary.timed_out_clients > 0
    ) {
        return IncidentSeverity::Warning;
    }

    if matches!(
        lifecycle_state.phase,
        AuthorityShardLifecyclePhase::Draining | AuthorityShardLifecyclePhase::Stopped
    ) {
        return IncidentSeverity::Warning;
    }

    if matches!(
        gameplay_incident_summary,
        Some(summary) if summary.severity == IncidentSeverity::Warning
    ) {
        return IncidentSeverity::Warning;
    }

    match transport_summary {
        Some(_) => IncidentSeverity::Healthy,
        None if shard.transport_mode.uses_direct_connect() => IncidentSeverity::Warning,
        None => IncidentSeverity::Healthy,
    }
}

fn build_control_plane_incident_summary(
    shard: &AuthorityShardSummary,
    lifecycle_state: &AuthorityShardLifecycleState,
    latest_tick: u64,
    severity: IncidentSeverity,
    notes: &[String],
    gameplay_incident_summary: Option<&ShardIncidentSummary>,
) -> ShardIncidentSummary {
    let summary = if notes.is_empty() {
        format!(
            "Shard {} is healthy at tick {}",
            shard.shard_id, latest_tick
        )
    } else {
        match lifecycle_state.phase {
            AuthorityShardLifecyclePhase::Draining => format!(
                "Shard {} is draining{}",
                shard.shard_id,
                lifecycle_state
                    .reason
                    .as_deref()
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default()
            ),
            AuthorityShardLifecyclePhase::Stopped => format!(
                "Shard {} has stopped{}",
                shard.shard_id,
                lifecycle_state
                    .reason
                    .as_deref()
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default()
            ),
            AuthorityShardLifecyclePhase::Running => {
                format!(
                    "Shard {} requires attention: {}",
                    shard.shard_id,
                    notes.join("; ")
                )
            }
        }
    };

    ShardIncidentSummary {
        shard_id: shard.shard_id.clone(),
        latest_tick,
        severity,
        summary,
        tick_budget_overrun_rate: gameplay_incident_summary
            .map(|summary| summary.tick_budget_overrun_rate)
            .unwrap_or_default(),
        action_rejection_rate: gameplay_incident_summary
            .map(|summary| summary.action_rejection_rate)
            .unwrap_or_default(),
        tool_call_error_rate: gameplay_incident_summary
            .map(|summary| summary.tool_call_error_rate)
            .unwrap_or_default(),
        average_tool_latency_ms: gameplay_incident_summary
            .map(|summary| summary.average_tool_latency_ms)
            .unwrap_or_default(),
        average_trajectory_distance: gameplay_incident_summary
            .map(|summary| summary.average_trajectory_distance)
            .unwrap_or_default(),
        peak_entity_count: gameplay_incident_summary
            .map(|summary| summary.peak_entity_count)
            .unwrap_or_default(),
        peak_agent_count: gameplay_incident_summary
            .map(|summary| summary.peak_agent_count)
            .unwrap_or_default(),
        capture_actions: gameplay_incident_summary
            .map(|summary| summary.capture_actions)
            .unwrap_or_default(),
        summon_actions: gameplay_incident_summary
            .map(|summary| summary.summon_actions)
            .unwrap_or_default(),
        gather_actions: gameplay_incident_summary
            .map(|summary| summary.gather_actions)
            .unwrap_or_default(),
        loot_actions: gameplay_incident_summary
            .map(|summary| summary.loot_actions)
            .unwrap_or_default(),
        notes: notes.to_vec(),
    }
}

#[derive(Debug, Clone)]
struct LocalAuthorityOpsStream {
    shard_id: String,
    archive: TelemetryArchive,
    pending_documents: VecDeque<String>,
}

impl LocalAuthorityOpsStream {
    fn new(shard_id: impl Into<String>) -> Self {
        Self {
            shard_id: shard_id.into(),
            archive: TelemetryArchive::with_capacity(TelemetryConfig::default().core_archive_ticks),
            pending_documents: VecDeque::new(),
        }
    }

    fn record_tick(
        &mut self,
        tick_result: &pod_core::tick::TickResult,
        incident: &ShardIncidentSummary,
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

        let emit_incident = !matches!(incident.severity, IncidentSeverity::Healthy)
            || (tick_result.tick + 1).is_multiple_of(INCIDENT_EMIT_INTERVAL_TICKS);
        if emit_incident {
            self.pending_documents
                .push_back(incident.to_toon_document());
        }
    }

    fn drain_documents(&mut self) -> Vec<String> {
        self.pending_documents.drain(..).collect()
    }

    fn focused_entity_summary(&self, entity_id: u64) -> Option<FocusedEntityDebugSummary> {
        summarize_focused_entity_debug(self.shard_id.clone(), &self.archive, entity_id)
    }

    fn focused_entity_document(&self, entity_id: u64) -> Option<String> {
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

#[derive(Debug)]
pub struct LocalAuthorityTickOutcome {
    pub tick_result: pod_core::tick::TickResult,
    pub tick_elapsed: Duration,
    pub tick_over_budget: bool,
}

pub struct LocalAuthorityRuntime {
    pub world: World,
    shard_id: String,
    gameplay_incident_tracker: ShardGameplayIncidentTracker,
    ops_stream: LocalAuthorityOpsStream,
    gameplay_incident_summary_tx: watch::Sender<Option<ShardIncidentSummary>>,
    lifecycle_state_tx: watch::Sender<ServerLifecycleState>,
    ops_document_stream: OpsDocumentStream,
    control_plane: AuthorityShardControlPlaneHandle,
    ops_handle: AuthorityShardOpsHandle,
}

impl LocalAuthorityRuntime {
    pub fn try_new_with_shard_id(
        config: &AuthorityHostConfig,
        world: World,
        shard_id: impl Into<String>,
    ) -> Result<Self, AuthorityHostError> {
        let shard_id = shard_id.into();
        let ops_document_stream = config.build_ops_document_stream(&shard_id)?;
        Ok(Self::new_with_ops_document_stream(
            config,
            world,
            shard_id,
            ops_document_stream,
        ))
    }

    pub fn new_with_shard_id(
        config: &AuthorityHostConfig,
        world: World,
        shard_id: impl Into<String>,
    ) -> Self {
        Self::new_with_ops_document_stream(
            config,
            world,
            shard_id,
            OpsDocumentStream::new(OPS_DOCUMENT_HISTORY_LIMIT, OPS_DOCUMENT_CHANNEL_CAPACITY),
        )
    }

    fn new_with_ops_document_stream(
        config: &AuthorityHostConfig,
        world: World,
        shard_id: impl Into<String>,
        ops_document_stream: OpsDocumentStream,
    ) -> Self {
        let shard_id = shard_id.into();
        let shard = AuthorityShardSummary {
            shard_id: shard_id.clone(),
            linked_shard_ids: Vec::new(),
            transport_mode: AuthorityTransportMode::Local,
            tick_rate: config.tick_rate,
            world_seed: config.world.world_seed,
            map_name: config.world.map_name.clone(),
            initial_idle_agents: config.world.initial_idle_agents,
            direct_connect_bind_address: None,
            direct_connect_websocket_port: None,
            direct_connect_max_clients: None,
        };
        let (gameplay_incident_summary_tx, gameplay_incident_summary_rx) = watch::channel(None);
        let (lifecycle_state_tx, lifecycle_state_rx) = watch::channel(ServerLifecycleState {
            shard_id: shard_id.clone(),
            phase: ServerLifecyclePhase::Running,
            accepting_new_connections: false,
            latest_tick: 0,
            reason: None,
        });

        Self {
            world,
            shard_id: shard_id.clone(),
            gameplay_incident_tracker: ShardGameplayIncidentTracker::new(),
            ops_stream: LocalAuthorityOpsStream::new(shard_id),
            gameplay_incident_summary_tx,
            lifecycle_state_tx,
            ops_document_stream: ops_document_stream.clone(),
            control_plane: AuthorityShardControlPlaneHandle {
                shard: shard.clone(),
                transport_summary_rx: None,
                gameplay_incident_summary_rx: Some(gameplay_incident_summary_rx),
                lifecycle_state_rx: Some(lifecycle_state_rx),
                lifecycle_command_tx: None,
            },
            ops_handle: AuthorityShardOpsHandle {
                shard,
                stream: ops_document_stream,
            },
        }
    }

    pub fn control_plane_handle(&self) -> AuthorityShardControlPlaneHandle {
        self.control_plane.clone()
    }

    pub fn ops_handle(&self) -> AuthorityShardOpsHandle {
        self.ops_handle.clone()
    }

    pub fn focused_entity_summary(&self, entity_id: u64) -> Option<FocusedEntityDebugSummary> {
        self.ops_stream.focused_entity_summary(entity_id)
    }

    pub fn focused_entity_document(&self, entity_id: u64) -> Option<String> {
        self.ops_stream.focused_entity_document(entity_id)
    }

    pub fn step(&mut self, tick_budget: Duration) -> LocalAuthorityTickOutcome {
        let tick_start = Instant::now();
        let tick_result = self.world.step();
        let tick_elapsed = tick_start.elapsed();
        let tick_over_budget = tick_elapsed > tick_budget;

        self.gameplay_incident_tracker.record_tick(
            &tick_result,
            self.world.agent_count(),
            tick_over_budget,
        );
        let incident = self
            .gameplay_incident_tracker
            .incident_summary(self.shard_id.clone(), tick_result.tick);
        self.ops_stream.record_tick(&tick_result, &incident);
        let _ = self.gameplay_incident_summary_tx.send(Some(incident));
        let _ = self.lifecycle_state_tx.send(ServerLifecycleState {
            shard_id: self.shard_id.clone(),
            phase: ServerLifecyclePhase::Running,
            accepting_new_connections: false,
            latest_tick: tick_result.tick,
            reason: None,
        });
        for document in self.ops_stream.drain_documents() {
            self.ops_document_stream.publish(document);
        }

        LocalAuthorityTickOutcome {
            tick_result,
            tick_elapsed,
            tick_over_budget,
        }
    }
}

pub enum AuthorityHostRuntime {
    Local(LocalAuthorityRuntime),
    DirectConnect(DirectConnectAuthorityRuntime),
}

impl AuthorityHostRuntime {
    fn control_plane_handle(
        &self,
        shard: AuthorityShardSummary,
    ) -> AuthorityShardControlPlaneHandle {
        match self {
            Self::Local(runtime) => {
                let mut handle = runtime.control_plane_handle();
                handle.shard = shard;
                handle
            }
            Self::DirectConnect(runtime) => {
                let mut handle = runtime.control_plane_handle();
                handle.shard = shard;
                handle
            }
        }
    }

    fn ops_handle(&self, shard: AuthorityShardSummary) -> AuthorityShardOpsHandle {
        match self {
            Self::Local(runtime) => {
                let mut handle = runtime.ops_handle();
                handle.shard = shard;
                handle
            }
            Self::DirectConnect(runtime) => {
                let mut handle = runtime.ops_handle();
                handle.shard = shard;
                handle
            }
        }
    }
}

pub struct DirectConnectAuthorityRuntime {
    server: pod_net::GameServer,
    control_plane: AuthorityShardControlPlaneHandle,
    ops_handle: AuthorityShardOpsHandle,
}

impl DirectConnectAuthorityRuntime {
    pub fn new(config: &AuthorityHostConfig, world: World) -> Result<Self, AuthorityHostError> {
        Self::new_with_shard_id(config, world, "direct-connect")
    }

    pub fn new_with_shard_id(
        config: &AuthorityHostConfig,
        world: World,
        shard_id: impl Into<String>,
    ) -> Result<Self, AuthorityHostError> {
        let shard_id = shard_id.into();
        let server_config = config
            .direct_connect
            .server_config(config.tick_rate)
            .map_err(|err| AuthorityHostError::TransportConfig(err.to_string()))?;
        let (transport_summary_tx, transport_summary_rx) = watch::channel(None);
        let (gameplay_incident_summary_tx, gameplay_incident_summary_rx) = watch::channel(None);
        let (lifecycle_command_tx, lifecycle_command_rx) = mpsc::unbounded_channel();
        let (lifecycle_state_tx, lifecycle_state_rx) =
            watch::channel(ServerLifecycleState::running(&shard_id));
        let ops_document_stream = config.build_ops_document_stream(&shard_id)?;
        let mut server = pod_net::GameServer::new_with_shard_id(server_config, world, &shard_id);
        server.install_transport_summary_watch(transport_summary_tx);
        server.install_incident_summary_watch(gameplay_incident_summary_tx);
        server.install_lifecycle_control(lifecycle_command_rx, lifecycle_state_tx);
        server.install_ops_document_stream(ops_document_stream.clone());
        let shard = AuthorityShardSummary {
            shard_id,
            linked_shard_ids: Vec::new(),
            transport_mode: AuthorityTransportMode::DirectConnect,
            tick_rate: config.tick_rate,
            world_seed: config.world.world_seed,
            map_name: config.world.map_name.clone(),
            initial_idle_agents: config.world.initial_idle_agents,
            direct_connect_bind_address: Some(config.bind_address().to_string()),
            direct_connect_websocket_port: Some(config.websocket_port()),
            direct_connect_max_clients: Some(config.max_clients()),
        };

        Ok(Self {
            server,
            control_plane: AuthorityShardControlPlaneHandle {
                shard: shard.clone(),
                transport_summary_rx: Some(transport_summary_rx),
                gameplay_incident_summary_rx: Some(gameplay_incident_summary_rx),
                lifecycle_state_rx: Some(lifecycle_state_rx),
                lifecycle_command_tx: Some(lifecycle_command_tx),
            },
            ops_handle: AuthorityShardOpsHandle {
                shard,
                stream: ops_document_stream,
            },
        })
    }

    pub fn control_plane_handle(&self) -> AuthorityShardControlPlaneHandle {
        self.control_plane.clone()
    }

    pub fn ops_handle(&self) -> AuthorityShardOpsHandle {
        self.ops_handle.clone()
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
    OpsPersistence(String),
    Initialize(String),
    Run(String),
}

impl std::fmt::Display for AuthorityHostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TransportConfig(message) => {
                write!(f, "invalid direct-connect transport config: {message}")
            }
            Self::OpsPersistence(message) => {
                write!(f, "failed to initialize ops persistence: {message}")
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
    control_plane: AuthorityShardControlPlaneHandle,
    ops_handle: AuthorityShardOpsHandle,
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

    pub fn control_plane_handle(&self) -> ShardSupervisorControlPlaneHandle {
        ShardSupervisorControlPlaneHandle {
            shards: self
                .shards
                .iter()
                .map(PreparedAuthorityShard::control_plane_handle)
                .collect(),
        }
    }

    pub fn ops_handle(&self) -> ShardSupervisorOpsHandle {
        ShardSupervisorOpsHandle {
            shards: self
                .shards
                .iter()
                .map(PreparedAuthorityShard::ops_handle)
                .collect(),
        }
    }

    pub fn archive_handle(&self) -> ShardSupervisorOpsArchiveHandle {
        ShardSupervisorOpsArchiveHandle {
            shards: self
                .shards
                .iter()
                .map(PreparedAuthorityShard::archive_handle)
                .collect(),
        }
    }

    pub fn archive_service(
        &self,
        config: OpsArchiveServiceConfig,
    ) -> ShardSupervisorOpsArchiveService {
        self.archive_handle().service(config)
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
                        AuthorityHostRuntime::Local(_) => {
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

impl PreparedAuthorityShard {
    pub fn control_plane_handle(&self) -> AuthorityShardControlPlaneHandle {
        self.control_plane.clone()
    }

    pub fn ops_handle(&self) -> AuthorityShardOpsHandle {
        self.ops_handle.clone()
    }

    pub fn archive_handle(&self) -> AuthorityShardOpsArchiveHandle {
        self.ops_handle.archive_handle()
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
        AuthorityHostConfig, AuthorityHostRuntime, AuthorityShardConfig,
        AuthorityShardControlPlaneHandle, AuthorityShardLifecycleCommandKind,
        AuthorityShardLifecyclePhase, AuthorityShardOpsHandle, AuthorityTransportMode,
        DirectConnectAuthorityRuntime, LocalAuthorityOpsStream, LocalAuthorityRuntime,
        OpsArchiveServiceClient, OpsArchiveServiceConfig, OpsArchiveServiceRequest,
        OpsArchiveServiceResponse, OpsPersistenceConfig, PreparedAuthorityShard,
        ShardSupervisorConfig, ShardSupervisorControlPlaneHandle, ShardSupervisorError,
        ShardSupervisorSummary, OPS_DOCUMENT_CHANNEL_CAPACITY,
    };
    use pod_core::tick::TickResult;
    use pod_core::{
        decode_toon_value, AuthorityWorldConfig, IncidentSeverity, ShardIncidentSummary,
        ShardTransportSummary, TickTelemetryFrame,
    };
    use pod_net::{
        DirectConnectTransportConfig, OpsDocumentStream, ServerLifecycleCommand,
        ServerLifecycleState, TransportPolicy,
    };
    use std::fs;
    use std::time::Duration;
    use tokio::sync::{mpsc, watch};

    #[test]
    fn authority_host_config_reads_runtime_mode_from_env() {
        let original_tick_rate = std::env::var_os("POD_TICK_RATE");
        let original_runtime_mode = std::env::var_os("POD_RUNTIME_MODE");
        let original_world_seed = std::env::var_os("POD_WORLD_SEED");
        let original_map_name = std::env::var_os("POD_MAP_NAME");
        let original_idle_agents = std::env::var_os("POD_INITIAL_IDLE_AGENTS");
        let original_ops_archive_dir = std::env::var_os("POD_OPS_ARCHIVE_DIR");

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
        assert_eq!(config.ops_persistence, None);

        restore_var("POD_TICK_RATE", original_tick_rate);
        restore_var("POD_RUNTIME_MODE", original_runtime_mode);
        restore_var("POD_WORLD_SEED", original_world_seed);
        restore_var("POD_MAP_NAME", original_map_name);
        restore_var("POD_INITIAL_IDLE_AGENTS", original_idle_agents);
        restore_var("POD_OPS_ARCHIVE_DIR", original_ops_archive_dir);
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
            AuthorityHostRuntime::Local(LocalAuthorityRuntime { world, .. }) => {
                assert_eq!(world.agent_count(), 2);
                assert!(world.entity_count() >= 1);
            }
            AuthorityHostRuntime::DirectConnect(_) => {
                panic!("expected local host runtime");
            }
        }
    }

    #[test]
    fn local_runtime_publishes_ops_documents_and_updates_control_plane() {
        let config = sample_config(AuthorityTransportMode::Local);
        let mut runtime = match config
            .prepare_runtime(|world, _map_name| {
                world
                    .spawn_at(1.0, 1.0)
                    .with_label("local-ops-marker", pod_core::Team::None)
                    .build();
            })
            .expect("local host runtime should build")
        {
            AuthorityHostRuntime::Local(runtime) => runtime,
            AuthorityHostRuntime::DirectConnect(_) => panic!("expected local host runtime"),
        };
        let mut documents = runtime.ops_handle().subscribe_documents();

        let outcome = runtime.step(Duration::from_secs_f32(1.0 / config.tick_rate as f32));
        assert_eq!(outcome.tick_result.tick, 0);

        let first_document = documents
            .try_recv()
            .expect("local runtime should publish a TOON ops document");
        let value = decode_toon_value(&first_document).expect("ops document should decode as TOON");
        assert_eq!(value["document_type"], "versioned_tick_telemetry");
        assert!(runtime.ops_handle().retained_document_count() >= 1);
        assert_eq!(runtime.ops_handle().persisted_document_count(), 0);
        assert_eq!(runtime.ops_handle().archive_path(), None);
        assert!(runtime
            .ops_handle()
            .recent_documents()
            .iter()
            .any(|document| decode_toon_value(document)
                .map(|value| value["document_type"] == "versioned_tick_telemetry")
                .unwrap_or(false)));

        let snapshot = runtime.control_plane_handle().snapshot();
        assert_eq!(snapshot.latest_tick, Some(0));
        assert_eq!(
            snapshot
                .gameplay_incident_summary
                .as_ref()
                .map(|summary| summary.latest_tick),
            Some(0)
        );
        assert_eq!(snapshot.shard.transport_mode, AuthorityTransportMode::Local);
    }

    #[test]
    fn shard_supervisor_ops_snapshot_rolls_up_retained_documents() {
        let config = sample_config(AuthorityTransportMode::Local);
        let mut runtime = LocalAuthorityRuntime::new_with_shard_id(
            &config,
            config.build_world(|world, _map_name| {
                world
                    .spawn_at(2.0, 2.0)
                    .with_label("ops-rollup-marker", pod_core::Team::None)
                    .build();
            }),
            "alpha-1",
        );

        runtime.step(Duration::from_secs_f32(1.0 / config.tick_rate as f32));

        let prepared = super::PreparedShardSupervisor {
            summary: ShardSupervisorSummary {
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
                control_plane: runtime.control_plane_handle(),
                ops_handle: runtime.ops_handle(),
                runtime: AuthorityHostRuntime::Local(runtime),
            }],
        };

        let snapshot = prepared.ops_handle().snapshot();
        assert_eq!(snapshot.shard_count, 1);
        assert!(snapshot.total_retained_document_count >= 1);
        assert_eq!(snapshot.total_persisted_document_count, 0);
        assert_eq!(snapshot.shards[0].shard.shard_id, "alpha-1");
        assert!(snapshot.shards[0]
            .recent_documents
            .iter()
            .any(|document| decode_toon_value(document)
                .map(|value| value["document_type"] == "versioned_tick_telemetry")
                .unwrap_or(false)));
    }

    #[test]
    fn local_runtime_persists_and_reloads_ops_history_from_archive() {
        let archive_root_dir = std::env::temp_dir().join(format!(
            "pod-host-ops-archive-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        let mut config = sample_config(AuthorityTransportMode::Local);
        config.ops_persistence = Some(OpsPersistenceConfig {
            archive_root_dir: archive_root_dir.clone(),
        });

        let archive_path = config
            .ops_persistence
            .as_ref()
            .expect("ops persistence should be configured")
            .archive_path_for_shard("alpha-1");

        let mut runtime = match config
            .prepare_runtime_with_shard_id("alpha-1", |world, _map_name| {
                world
                    .spawn_at(3.0, 3.0)
                    .with_label("persistent-ops-marker", pod_core::Team::None)
                    .build();
            })
            .expect("persistent local host runtime should build")
        {
            AuthorityHostRuntime::Local(runtime) => runtime,
            AuthorityHostRuntime::DirectConnect(_) => panic!("expected local host runtime"),
        };

        runtime.step(Duration::from_secs_f32(1.0 / config.tick_rate as f32));
        assert!(archive_path.exists());
        assert!(runtime.ops_handle().persisted_document_count() >= 1);
        assert_eq!(
            runtime.ops_handle().archive_path(),
            Some(archive_path.clone())
        );

        drop(runtime);

        let reloaded_runtime = LocalAuthorityRuntime::try_new_with_shard_id(
            &config,
            config.build_world(|_world, _map_name| {}),
            "alpha-1",
        )
        .expect("reloaded runtime should restore archive-backed ops stream");
        let reloaded_snapshot = reloaded_runtime.ops_handle().snapshot();
        assert!(reloaded_snapshot.persisted_document_count >= 1);
        assert_eq!(reloaded_snapshot.archive_path, Some(archive_path.clone()));
        assert!(reloaded_snapshot
            .recent_documents
            .iter()
            .any(|document| decode_toon_value(document)
                .map(|value| value["document_type"] == "versioned_tick_telemetry")
                .unwrap_or(false)));

        drop(reloaded_runtime);
        fs::remove_dir_all(&archive_root_dir).expect("temp archive root should be removable");
    }

    #[test]
    fn shard_and_supervisor_archive_handles_query_persisted_history() {
        let archive_root_dir = std::env::temp_dir().join(format!(
            "pod-host-ops-query-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        let mut host = sample_config(AuthorityTransportMode::Local);
        host.ops_persistence = Some(OpsPersistenceConfig {
            archive_root_dir: archive_root_dir.clone(),
        });
        let shard_config = AuthorityShardConfig {
            shard_id: "alpha-1".into(),
            linked_shard_ids: vec![],
            host: host.clone(),
        };

        let mut runtime = match host
            .prepare_runtime_with_shard_id("alpha-1", |world, _map_name| {
                world
                    .spawn_at(4.0, 4.0)
                    .with_label("archive-query-marker", pod_core::Team::None)
                    .build();
            })
            .expect("archive-backed local host runtime should build")
        {
            AuthorityHostRuntime::Local(runtime) => runtime,
            AuthorityHostRuntime::DirectConnect(_) => panic!("expected local host runtime"),
        };
        runtime.step(Duration::from_secs_f32(1.0 / host.tick_rate as f32));
        drop(runtime);

        let shard_snapshot = shard_config
            .ops_archive_handle()
            .snapshot(8)
            .expect("shard archive handle should query persisted history");
        assert_eq!(shard_snapshot.shard_id, "alpha-1");
        assert!(shard_snapshot.persisted_document_count >= 1);
        assert!(shard_snapshot
            .recent_documents
            .iter()
            .any(|document| decode_toon_value(document)
                .map(|value| value["document_type"] == "versioned_tick_telemetry")
                .unwrap_or(false)));

        let supervisor_snapshot = ShardSupervisorConfig {
            shards: vec![shard_config],
        }
        .ops_archive_handle()
        .snapshot(8)
        .expect("supervisor archive handle should query shard archives");
        assert_eq!(supervisor_snapshot.shard_count, 1);
        assert_eq!(supervisor_snapshot.archived_shard_count, 1);
        assert!(supervisor_snapshot.total_persisted_document_count >= 1);

        fs::remove_dir_all(&archive_root_dir).expect("temp archive root should be removable");
    }

    #[tokio::test]
    async fn supervisor_archive_service_queries_persisted_history_over_tcp() {
        let archive_root_dir = std::env::temp_dir().join(format!(
            "pod-host-ops-service-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        let mut host = sample_config(AuthorityTransportMode::Local);
        host.ops_persistence = Some(OpsPersistenceConfig {
            archive_root_dir: archive_root_dir.clone(),
        });
        let shard_config = AuthorityShardConfig {
            shard_id: "alpha-1".into(),
            linked_shard_ids: vec![],
            host: host.clone(),
        };

        let mut runtime = match host
            .prepare_runtime_with_shard_id("alpha-1", |world, _map_name| {
                world
                    .spawn_at(5.0, 5.0)
                    .with_label("archive-service-marker", pod_core::Team::None)
                    .build();
            })
            .expect("archive-backed local runtime should build")
        {
            AuthorityHostRuntime::Local(runtime) => runtime,
            AuthorityHostRuntime::DirectConnect(_) => panic!("expected local host runtime"),
        };
        runtime.step(Duration::from_secs_f32(1.0 / host.tick_rate as f32));
        drop(runtime);

        let service = ShardSupervisorConfig {
            shards: vec![shard_config],
        }
        .archive_service(OpsArchiveServiceConfig {
            bind_address: "127.0.0.1:0".to_string(),
            max_request_bytes: 16 * 1024,
            max_response_bytes: 256 * 1024,
        });

        let listener = service
            .bind_listener()
            .await
            .expect("archive service should bind an ephemeral listener");
        let address = listener
            .local_addr()
            .expect("archive service listener should expose a local address")
            .to_string();
        let service_task = tokio::spawn(service.serve_once_listener(listener));

        let response = OpsArchiveServiceClient {
            address,
            max_response_bytes: 256 * 1024,
        }
        .query(OpsArchiveServiceRequest::Supervisor {
            recent_limit_per_shard: 8,
        })
        .await
        .expect("archive service client should receive a supervisor snapshot");

        match response {
            OpsArchiveServiceResponse::Supervisor(snapshot) => {
                assert_eq!(snapshot.shard_count, 1);
                assert_eq!(snapshot.archived_shard_count, 1);
                assert!(snapshot.total_persisted_document_count >= 1);
                assert!(snapshot.shards[0].recent_documents.iter().any(|document| {
                    decode_toon_value(document)
                        .map(|value| value["document_type"] == "versioned_tick_telemetry")
                        .unwrap_or(false)
                }));
            }
            OpsArchiveServiceResponse::Shard(_) | OpsArchiveServiceResponse::Error { .. } => {
                panic!("expected supervisor archive snapshot response")
            }
        }

        service_task
            .await
            .expect("archive service task should join")
            .expect("archive service should exit cleanly after one request");
        fs::remove_dir_all(&archive_root_dir).expect("temp archive root should be removable");
    }

    #[test]
    fn local_ops_stream_emits_tick_and_incident_documents() {
        let mut stream = LocalAuthorityOpsStream::new("overworld-a");
        let tick_result = TickResult {
            tick: 59,
            events: vec![],
            entity_count: 12,
            actions_processed: 3,
            actions_rejected: 1,
            telemetry: TickTelemetryFrame {
                tick: 59,
                agents: Vec::new(),
            },
        };
        let incident = ShardIncidentSummary {
            shard_id: "overworld-a".into(),
            latest_tick: 59,
            severity: IncidentSeverity::Healthy,
            summary: "Shard overworld-a is healthy at tick 59".into(),
            tick_budget_overrun_rate: 0.0,
            action_rejection_rate: 0.0,
            tool_call_error_rate: 0.0,
            average_tool_latency_ms: 0.0,
            average_trajectory_distance: 0.0,
            peak_entity_count: 12,
            peak_agent_count: 5,
            capture_actions: 0,
            summon_actions: 0,
            gather_actions: 0,
            loot_actions: 0,
            notes: Vec::new(),
        };
        stream.record_tick(&tick_result, &incident);

        let documents = stream.drain_documents();
        assert!(documents.iter().any(|document| {
            decode_toon_value(document)
                .map(|value| value["document_type"] == "versioned_tick_telemetry")
                .unwrap_or(false)
        }));
        assert!(documents.iter().any(|document| {
            decode_toon_value(document)
                .map(|value| value["document_type"] == "shard_incident_summary")
                .unwrap_or(false)
        }));
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
            .all(|shard| matches!(shard.runtime, AuthorityHostRuntime::Local(_))));

        let control_plane = prepared.control_plane_handle().snapshot();
        assert_eq!(control_plane.shard_count, 2);
        assert_eq!(control_plane.reporting_transport_shard_count, 0);
        assert_eq!(control_plane.healthy_shard_count, 2);
        assert_eq!(control_plane.running_shard_count, 2);
    }

    #[test]
    fn shard_control_plane_rolls_up_live_transport_health() {
        let shard = AuthorityShardConfig {
            shard_id: "alpha-1".into(),
            linked_shard_ids: vec!["alpha-2".into()],
            host: sample_config(AuthorityTransportMode::DirectConnect),
        }
        .summary();
        let (tx, rx) = watch::channel(Some(ShardTransportSummary {
            shard_id: "alpha-1".into(),
            latest_tick: 77,
            client_count: 3,
            resumed_sessions: 1,
            recovery_snapshots_sent: 2,
            recovery_delivery_failures: 1,
            client_inactivity_timeout_ticks: 600,
            queue_pressure_warn_depth: 192,
            total_pending_action_queue_depth: 4,
            peak_pending_action_queue_depth: 8,
            queue_pressure_client_count: 1,
            total_inbound_messages: 10,
            total_outbound_messages: 20,
            total_inbound_bytes: 100,
            total_outbound_bytes: 200,
            action_batches_received: 4,
            full_snapshots_sent: 2,
            total_full_snapshot_bytes: 4096,
            max_full_snapshot_bytes: 2048,
            total_recovery_snapshot_bytes: 1024,
            full_snapshot_requests: 1,
            ping_requests: 3,
            state_deltas_sent: 9,
            delta_messages_sent: 9,
            total_delta_bytes: 512,
            max_delta_bytes: 128,
            total_delta_entities_updated: 20,
            total_delta_entities_destroyed: 2,
            event_batches_sent: 3,
            debug_documents_sent: 5,
            rejected_messages_sent: 1,
            timed_out_clients: 2,
            queue_pressure_events: 4,
            clients: Vec::new(),
        }));
        let (_command_tx, command_rx) = mpsc::unbounded_channel();
        let (lifecycle_tx, lifecycle_rx) = watch::channel(ServerLifecycleState::running("alpha-1"));
        let (gameplay_tx, gameplay_rx) = watch::channel(Some(ShardIncidentSummary {
            shard_id: "alpha-1".into(),
            latest_tick: 77,
            severity: IncidentSeverity::Warning,
            summary: "Shard alpha-1 requires attention: tool-call latency exceeds 750ms".into(),
            tick_budget_overrun_rate: 0.0,
            action_rejection_rate: 0.0,
            tool_call_error_rate: 0.0,
            average_tool_latency_ms: 900.0,
            average_trajectory_distance: 0.0,
            peak_entity_count: 12,
            peak_agent_count: 3,
            capture_actions: 0,
            summon_actions: 0,
            gather_actions: 0,
            loot_actions: 0,
            notes: vec!["tool-call latency exceeds 750ms".into()],
        }));
        lifecycle_tx
            .send(ServerLifecycleState {
                shard_id: "alpha-1".into(),
                phase: pod_net::ServerLifecyclePhase::Draining,
                accepting_new_connections: false,
                latest_tick: 77,
                reason: Some("deploy rollout".into()),
            })
            .expect("lifecycle watch should accept updated drain state");

        let handle = AuthorityShardControlPlaneHandle {
            shard,
            transport_summary_rx: Some(rx),
            gameplay_incident_summary_rx: Some(gameplay_rx),
            lifecycle_state_rx: Some(lifecycle_rx),
            lifecycle_command_tx: Some(_command_tx),
        };
        let snapshot = handle.snapshot();
        assert_eq!(snapshot.severity, IncidentSeverity::Critical);
        assert_eq!(snapshot.latest_tick, Some(77));
        assert!(snapshot.has_live_transport);
        assert_eq!(
            snapshot.lifecycle_state.phase,
            AuthorityShardLifecyclePhase::Draining
        );
        assert_eq!(
            snapshot
                .transport_summary
                .as_ref()
                .map(|summary| summary.client_count),
            Some(3)
        );
        assert!(snapshot
            .notes
            .iter()
            .any(|note| note.contains("recovery snapshot deliveries")));
        assert!(snapshot
            .notes
            .iter()
            .any(|note| note.contains("tool-call latency exceeds 750ms")));
        assert_eq!(
            snapshot
                .gameplay_incident_summary
                .as_ref()
                .map(|summary| summary.average_tool_latency_ms),
            Some(900.0)
        );
        assert!(snapshot.incident_summary.summary.contains("draining"));

        let supervisor = ShardSupervisorControlPlaneHandle {
            shards: vec![handle],
        }
        .snapshot();
        assert_eq!(supervisor.critical_shard_count, 1);
        assert_eq!(supervisor.draining_shard_count, 1);
        assert_eq!(supervisor.total_client_count, 3);
        assert_eq!(supervisor.total_timed_out_clients, 2);
        assert_eq!(supervisor.total_recovery_delivery_failures, 1);

        drop(tx);
        drop(gameplay_tx);
        drop(command_rx);
    }

    #[test]
    fn shard_supervisor_control_plane_broadcasts_shutdown_commands() {
        let shard = AuthorityShardConfig {
            shard_id: "alpha-1".into(),
            linked_shard_ids: vec![],
            host: sample_config(AuthorityTransportMode::DirectConnect),
        }
        .summary();
        let (command_tx, mut command_rx) = mpsc::unbounded_channel();
        let handle = AuthorityShardControlPlaneHandle {
            shard,
            transport_summary_rx: None,
            gameplay_incident_summary_rx: None,
            lifecycle_state_rx: Some(watch::channel(ServerLifecycleState::running("alpha-1")).1),
            lifecycle_command_tx: Some(command_tx),
        };

        let result = ShardSupervisorControlPlaneHandle {
            shards: vec![handle],
        }
        .request_shutdown_all("maintenance window");

        assert_eq!(result.command, AuthorityShardLifecycleCommandKind::Shutdown);
        assert_eq!(result.accepted_shard_ids, vec!["alpha-1".to_string()]);
        assert!(result.rejected.is_empty());
        assert_eq!(
            command_rx
                .try_recv()
                .expect("shutdown command should be queued"),
            ServerLifecycleCommand::Shutdown {
                reason: "maintenance window".into()
            }
        );
    }

    #[tokio::test]
    async fn shard_supervisor_run_rejects_local_runtimes() {
        let prepared = super::PreparedShardSupervisor {
            summary: ShardSupervisorSummary {
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
                runtime: AuthorityHostRuntime::Local(LocalAuthorityRuntime::new_with_shard_id(
                    &sample_config(AuthorityTransportMode::Local),
                    sample_config(AuthorityTransportMode::Local)
                        .build_world(|_world, _map_name| {}),
                    "alpha-1",
                )),
                control_plane: AuthorityShardControlPlaneHandle {
                    shard: AuthorityShardConfig {
                        shard_id: "alpha-1".into(),
                        linked_shard_ids: vec![],
                        host: sample_config(AuthorityTransportMode::Local),
                    }
                    .summary(),
                    transport_summary_rx: None,
                    gameplay_incident_summary_rx: None,
                    lifecycle_state_rx: None,
                    lifecycle_command_tx: None,
                },
                ops_handle: AuthorityShardOpsHandle {
                    shard: AuthorityShardConfig {
                        shard_id: "alpha-1".into(),
                        linked_shard_ids: vec![],
                        host: sample_config(AuthorityTransportMode::Local),
                    }
                    .summary(),
                    stream: OpsDocumentStream::new(
                        OPS_DOCUMENT_CHANNEL_CAPACITY,
                        OPS_DOCUMENT_CHANNEL_CAPACITY,
                    ),
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
            ops_persistence: None,
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
