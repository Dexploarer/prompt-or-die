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
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader,
};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, watch};

pub use pod_net::{parse_bind_target, DirectConnectTransportConfig, TransportPolicy};
use pod_net::{
    OpsDocumentArchiveReplaySnapshot, OpsDocumentArchiveSnapshot, OpsDocumentRecord,
    OpsDocumentStream, ServerLifecycleCommand, ServerLifecyclePhase, ServerLifecycleState,
};

const OPS_DOCUMENT_CHANNEL_CAPACITY: usize = 256;
const OPS_DOCUMENT_HISTORY_LIMIT: usize = 256;
const OPS_REPLAY_BOOKMARK_VERSION: u8 = 1;
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

    pub fn subscribe_documents(&self) -> broadcast::Receiver<OpsDocumentRecord> {
        self.stream.subscribe()
    }

    pub fn recent_documents(&self) -> Vec<String> {
        self.stream.recent_documents()
    }

    pub fn recent_document_records(&self) -> Vec<OpsDocumentRecord> {
        self.stream.recent_document_records()
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

    pub fn latest_sequence(&self) -> u64 {
        self.stream.latest_sequence()
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

    pub fn replay_snapshot(
        &self,
        after_sequence: u64,
        limit: usize,
    ) -> AuthorityShardOpsReplaySnapshot {
        if self.archive_path().is_some() {
            return self
                .archive_handle()
                .replay_snapshot(after_sequence, limit)
                .unwrap_or_else(|_| self.replay_snapshot_from_retained(after_sequence, limit));
        }

        self.replay_snapshot_from_retained(after_sequence, limit)
    }

    fn replay_snapshot_from_retained(
        &self,
        after_sequence: u64,
        limit: usize,
    ) -> AuthorityShardOpsReplaySnapshot {
        let records = self.recent_document_records();
        let first_retained_sequence = records.first().map(|record| record.sequence).unwrap_or(0);
        let last_available_sequence = self.latest_sequence();
        let gap_detected = last_available_sequence > 0
            && first_retained_sequence > after_sequence.saturating_add(1);
        let effective_after_sequence = if gap_detected {
            first_retained_sequence.saturating_sub(1)
        } else {
            after_sequence
        };
        let documents = records
            .into_iter()
            .filter(|record| record.sequence > effective_after_sequence)
            .take(limit)
            .map(AuthorityShardOpsReplayDocument::from)
            .collect::<Vec<_>>();
        let next_sequence = documents
            .last()
            .map(|document| document.sequence)
            .unwrap_or(after_sequence.min(last_available_sequence));
        let next_cursor = AuthorityShardOpsReplayCursor {
            shard_id: self.shard.shard_id.clone(),
            last_sequence: next_sequence,
        };

        AuthorityShardOpsReplaySnapshot {
            shard_id: self.shard.shard_id.clone(),
            shard: Some(self.shard.clone()),
            archive_path: None,
            persisted_document_count: 0,
            requested_after_sequence: after_sequence,
            gap_detected,
            has_more: next_sequence < last_available_sequence,
            last_available_sequence,
            next_bookmark: encode_shard_replay_bookmark(&next_cursor),
            next_cursor,
            documents,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityShardOpsReplayCursor {
    pub shard_id: String,
    pub last_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityShardOpsReplayDocument {
    pub sequence: u64,
    pub document: String,
}

impl From<OpsDocumentRecord> for AuthorityShardOpsReplayDocument {
    fn from(value: OpsDocumentRecord) -> Self {
        Self {
            sequence: value.sequence,
            document: value.document,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityShardOpsReplaySnapshot {
    pub shard_id: String,
    pub shard: Option<AuthorityShardSummary>,
    pub archive_path: Option<PathBuf>,
    pub persisted_document_count: usize,
    pub requested_after_sequence: u64,
    pub gap_detected: bool,
    pub has_more: bool,
    pub last_available_sequence: u64,
    pub next_bookmark: String,
    pub next_cursor: AuthorityShardOpsReplayCursor,
    pub documents: Vec<AuthorityShardOpsReplayDocument>,
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

    pub fn replay_snapshot(
        &self,
        after_sequence: u64,
        limit: usize,
    ) -> Result<AuthorityShardOpsReplaySnapshot, AuthorityOpsArchiveError> {
        let Some(archive_path) = self.archive_path.clone() else {
            let next_cursor = AuthorityShardOpsReplayCursor {
                shard_id: self.shard_id.clone(),
                last_sequence: 0,
            };
            return Ok(AuthorityShardOpsReplaySnapshot {
                shard_id: self.shard_id.clone(),
                shard: self.shard.clone(),
                archive_path: None,
                persisted_document_count: 0,
                requested_after_sequence: after_sequence,
                gap_detected: after_sequence > 0,
                has_more: false,
                last_available_sequence: 0,
                next_bookmark: encode_shard_replay_bookmark(&next_cursor),
                next_cursor,
                documents: Vec::new(),
            });
        };

        let snapshot =
            OpsDocumentArchiveReplaySnapshot::load_after(&archive_path, after_sequence, limit)
                .map_err(|source| AuthorityOpsArchiveError::ReadArchive {
                    shard_id: self.shard_id.clone(),
                    path: archive_path.clone(),
                    source,
                })?;
        let next_sequence = snapshot
            .documents
            .last()
            .map(|document| document.sequence)
            .unwrap_or(after_sequence.min(snapshot.last_available_sequence));
        let next_cursor = AuthorityShardOpsReplayCursor {
            shard_id: self.shard_id.clone(),
            last_sequence: next_sequence,
        };

        Ok(AuthorityShardOpsReplaySnapshot {
            shard_id: self.shard_id.clone(),
            shard: self.shard.clone(),
            archive_path: Some(snapshot.archive_path),
            persisted_document_count: snapshot.persisted_document_count,
            requested_after_sequence: after_sequence,
            gap_detected: false,
            has_more: snapshot.has_more,
            last_available_sequence: snapshot.last_available_sequence,
            next_bookmark: encode_shard_replay_bookmark(&next_cursor),
            next_cursor,
            documents: snapshot
                .documents
                .into_iter()
                .map(AuthorityShardOpsReplayDocument::from)
                .collect(),
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

    pub fn relay(&self, config: OpsRelayConfig) -> ShardSupervisorOpsRelayService {
        ShardSupervisorOpsRelayService {
            live: self.clone(),
            archive: self.archive_handle(),
            config,
        }
    }

    pub fn http_service(&self, config: OpsHttpServiceConfig) -> ShardSupervisorOpsHttpService {
        ShardSupervisorOpsHttpService {
            live: self.clone(),
            archive: self.archive_handle(),
            config,
        }
    }

    pub fn replay_snapshot(
        &self,
        cursor: &ShardSupervisorOpsReplayCursor,
        limit_per_shard: usize,
    ) -> ShardSupervisorOpsReplaySnapshot {
        let shards = self
            .shards
            .iter()
            .map(|handle| {
                handle.replay_snapshot(
                    cursor.last_sequence_for(&handle.shard.shard_id),
                    limit_per_shard,
                )
            })
            .collect::<Vec<_>>();
        let next_cursor = ShardSupervisorOpsReplayCursor {
            shards: shards
                .iter()
                .map(|snapshot| snapshot.next_cursor.clone())
                .collect(),
        };

        ShardSupervisorOpsReplaySnapshot {
            shard_count: shards.len(),
            total_persisted_document_count: shards
                .iter()
                .map(|snapshot| snapshot.persisted_document_count)
                .sum(),
            gap_detected_shard_count: shards
                .iter()
                .filter(|snapshot| snapshot.gap_detected)
                .count(),
            has_more_shard_count: shards.iter().filter(|snapshot| snapshot.has_more).count(),
            next_bookmark: encode_supervisor_replay_bookmark(&next_cursor),
            next_cursor,
            shards,
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardSupervisorOpsReplayCursor {
    pub shards: Vec<AuthorityShardOpsReplayCursor>,
}

impl ShardSupervisorOpsReplayCursor {
    fn last_sequence_for(&self, shard_id: &str) -> u64 {
        self.shards
            .iter()
            .find(|cursor| cursor.shard_id == shard_id)
            .map(|cursor| cursor.last_sequence)
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardSupervisorOpsReplaySnapshot {
    pub shard_count: usize,
    pub total_persisted_document_count: usize,
    pub gap_detected_shard_count: usize,
    pub has_more_shard_count: usize,
    pub next_bookmark: String,
    pub next_cursor: ShardSupervisorOpsReplayCursor,
    pub shards: Vec<AuthorityShardOpsReplaySnapshot>,
}

#[derive(Debug)]
pub enum OpsReplayBookmarkError {
    InvalidFormat {
        bookmark: String,
    },
    DecodePayload(serde_json::Error),
    ScopeMismatch {
        expected_scope: String,
        actual_scope: String,
    },
    UnsupportedVersion {
        version: u8,
    },
}

impl std::fmt::Display for OpsReplayBookmarkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFormat { bookmark } => {
                write!(f, "invalid replay bookmark format '{bookmark}'")
            }
            Self::DecodePayload(source) => {
                write!(f, "failed to decode replay bookmark payload: {source}")
            }
            Self::ScopeMismatch {
                expected_scope,
                actual_scope,
            } => write!(
                f,
                "replay bookmark scope mismatch: expected {expected_scope}, got {actual_scope}"
            ),
            Self::UnsupportedVersion { version } => {
                write!(f, "unsupported replay bookmark version {version}")
            }
        }
    }
}

impl std::error::Error for OpsReplayBookmarkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::DecodePayload(source) => Some(source),
            Self::InvalidFormat { .. }
            | Self::ScopeMismatch { .. }
            | Self::UnsupportedVersion { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct OpsReplayBookmarkEnvelope {
    version: u8,
    #[serde(flatten)]
    scope: OpsReplayBookmarkScope,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
enum OpsReplayBookmarkScope {
    Shard {
        cursor: AuthorityShardOpsReplayCursor,
    },
    Supervisor {
        cursor: ShardSupervisorOpsReplayCursor,
    },
}

impl OpsReplayBookmarkScope {
    fn scope_name(&self) -> &'static str {
        match self {
            Self::Shard { .. } => "shard",
            Self::Supervisor { .. } => "supervisor",
        }
    }
}

pub fn encode_shard_replay_bookmark(cursor: &AuthorityShardOpsReplayCursor) -> String {
    encode_replay_bookmark(OpsReplayBookmarkScope::Shard {
        cursor: cursor.clone(),
    })
}

pub fn decode_shard_replay_bookmark(
    bookmark: &str,
) -> Result<AuthorityShardOpsReplayCursor, OpsReplayBookmarkError> {
    match decode_replay_bookmark(bookmark)? {
        OpsReplayBookmarkScope::Shard { cursor } => Ok(cursor),
        other => Err(OpsReplayBookmarkError::ScopeMismatch {
            expected_scope: "shard".to_string(),
            actual_scope: other.scope_name().to_string(),
        }),
    }
}

pub fn encode_supervisor_replay_bookmark(cursor: &ShardSupervisorOpsReplayCursor) -> String {
    encode_replay_bookmark(OpsReplayBookmarkScope::Supervisor {
        cursor: cursor.clone(),
    })
}

pub fn decode_supervisor_replay_bookmark(
    bookmark: &str,
) -> Result<ShardSupervisorOpsReplayCursor, OpsReplayBookmarkError> {
    match decode_replay_bookmark(bookmark)? {
        OpsReplayBookmarkScope::Supervisor { cursor } => Ok(cursor),
        other => Err(OpsReplayBookmarkError::ScopeMismatch {
            expected_scope: "supervisor".to_string(),
            actual_scope: other.scope_name().to_string(),
        }),
    }
}

fn encode_replay_bookmark(scope: OpsReplayBookmarkScope) -> String {
    let payload = serde_json::to_vec(&OpsReplayBookmarkEnvelope {
        version: OPS_REPLAY_BOOKMARK_VERSION,
        scope,
    })
    .expect("replay bookmark payload should serialize");

    let mut bookmark = String::with_capacity(payload.len() * 2);
    for byte in payload {
        use std::fmt::Write as _;

        let _ = write!(&mut bookmark, "{byte:02x}");
    }
    bookmark
}

fn decode_replay_bookmark(
    bookmark: &str,
) -> Result<OpsReplayBookmarkScope, OpsReplayBookmarkError> {
    if bookmark.is_empty() || bookmark.len() % 2 != 0 {
        return Err(OpsReplayBookmarkError::InvalidFormat {
            bookmark: bookmark.to_string(),
        });
    }

    let bytes = bookmark.as_bytes();
    let mut payload = Vec::with_capacity(bytes.len() / 2);
    for index in (0..bytes.len()).step_by(2) {
        let high = decode_hex_nibble(bytes[index]).ok_or_else(|| {
            OpsReplayBookmarkError::InvalidFormat {
                bookmark: bookmark.to_string(),
            }
        })?;
        let low = decode_hex_nibble(bytes[index + 1]).ok_or_else(|| {
            OpsReplayBookmarkError::InvalidFormat {
                bookmark: bookmark.to_string(),
            }
        })?;
        payload.push((high << 4) | low);
    }

    let envelope = serde_json::from_slice::<OpsReplayBookmarkEnvelope>(&payload)
        .map_err(OpsReplayBookmarkError::DecodePayload)?;
    if envelope.version != OPS_REPLAY_BOOKMARK_VERSION {
        return Err(OpsReplayBookmarkError::UnsupportedVersion {
            version: envelope.version,
        });
    }

    Ok(envelope.scope)
}

fn decode_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
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

    pub fn replay_snapshot(
        &self,
        cursor: &ShardSupervisorOpsReplayCursor,
        limit_per_shard: usize,
    ) -> Result<ShardSupervisorOpsReplaySnapshot, AuthorityOpsArchiveError> {
        let shards = self
            .shards
            .iter()
            .map(|handle| {
                handle.replay_snapshot(cursor.last_sequence_for(handle.shard_id()), limit_per_shard)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = ShardSupervisorOpsReplayCursor {
            shards: shards
                .iter()
                .map(|snapshot| snapshot.next_cursor.clone())
                .collect(),
        };

        Ok(ShardSupervisorOpsReplaySnapshot {
            shard_count: shards.len(),
            total_persisted_document_count: shards
                .iter()
                .map(|snapshot| snapshot.persisted_document_count)
                .sum(),
            gap_detected_shard_count: shards
                .iter()
                .filter(|snapshot| snapshot.gap_detected)
                .count(),
            has_more_shard_count: shards.iter().filter(|snapshot| snapshot.has_more).count(),
            next_bookmark: encode_supervisor_replay_bookmark(&next_cursor),
            next_cursor,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpsHttpServiceConfig {
    pub bind_address: String,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_event_bytes: usize,
    pub auth_token: Option<String>,
}

impl Default for OpsHttpServiceConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:7612".to_string(),
            max_request_bytes: 64 * 1024,
            max_response_bytes: 512 * 1024,
            max_event_bytes: 512 * 1024,
            auth_token: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ShardSupervisorOpsHttpService {
    live: ShardSupervisorOpsHandle,
    archive: ShardSupervisorOpsArchiveHandle,
    config: OpsHttpServiceConfig,
}

impl ShardSupervisorOpsHttpService {
    pub async fn serve(self) -> Result<(), OpsHttpError> {
        let listener = self.bind_listener().await?;
        self.serve_listener(listener).await
    }

    pub async fn serve_once(self) -> Result<(), OpsHttpError> {
        let listener = self.bind_listener().await?;
        self.serve_once_listener(listener).await
    }

    async fn bind_listener(&self) -> Result<TcpListener, OpsHttpError> {
        TcpListener::bind(&self.config.bind_address)
            .await
            .map_err(|source| OpsHttpError::Bind {
                address: self.config.bind_address.clone(),
                source,
            })
    }

    async fn serve_listener(self, listener: TcpListener) -> Result<(), OpsHttpError> {
        loop {
            let (socket, _) = listener.accept().await.map_err(OpsHttpError::Accept)?;
            self.handle_socket(socket).await?;
        }
    }

    async fn serve_once_listener(self, listener: TcpListener) -> Result<(), OpsHttpError> {
        let (socket, _) = listener.accept().await.map_err(OpsHttpError::Accept)?;
        self.handle_socket(socket).await
    }

    async fn handle_socket(&self, socket: TcpStream) -> Result<(), OpsHttpError> {
        let (read_half, mut write_half) = socket.into_split();
        let mut reader = BufReader::new(read_half);
        let request = match read_http_request(&mut reader, self.config.max_request_bytes).await {
            Ok(Some(request)) => request,
            Ok(None) => return Ok(()),
            Err(err) if err.can_respond() => {
                self.write_error_response(&mut write_half, &err).await?;
                return Ok(());
            }
            Err(err) => return Err(err),
        };
        let route = match parse_http_route(&request.target) {
            Ok(route) => route,
            Err(err) => {
                self.write_error_response(&mut write_half, &err).await?;
                return Ok(());
            }
        };
        let provided_token = request.bearer_token();

        match route {
            OpsHttpRoute::ArchiveSupervisor {
                recent_limit_per_shard,
            } => match self
                .handle_archive_supervisor(&mut write_half, recent_limit_per_shard, provided_token)
                .await
            {
                Ok(()) => Ok(()),
                Err(err) if err.can_respond() => {
                    self.write_error_response(&mut write_half, &err).await?;
                    Ok(())
                }
                Err(err) => Err(err),
            },
            OpsHttpRoute::ArchiveShard {
                shard_id,
                recent_limit,
            } => match self
                .handle_archive_shard(&mut write_half, shard_id, recent_limit, provided_token)
                .await
            {
                Ok(()) => Ok(()),
                Err(err) if err.can_respond() => {
                    self.write_error_response(&mut write_half, &err).await?;
                    Ok(())
                }
                Err(err) => Err(err),
            },
            OpsHttpRoute::ReplaySupervisor {
                cursor,
                limit_per_shard,
            } => match self
                .handle_replay_supervisor(&mut write_half, cursor, limit_per_shard, provided_token)
                .await
            {
                Ok(()) => Ok(()),
                Err(err) if err.can_respond() => {
                    self.write_error_response(&mut write_half, &err).await?;
                    Ok(())
                }
                Err(err) => Err(err),
            },
            OpsHttpRoute::ReplayShard {
                shard_id,
                after_sequence,
                limit,
            } => match self
                .handle_replay_shard(
                    &mut write_half,
                    shard_id,
                    after_sequence,
                    limit,
                    provided_token,
                )
                .await
            {
                Ok(()) => Ok(()),
                Err(err) if err.can_respond() => {
                    self.write_error_response(&mut write_half, &err).await?;
                    Ok(())
                }
                Err(err) => Err(err),
            },
            OpsHttpRoute::StreamSupervisor {
                recent_limit_per_shard,
                cursor,
            } => {
                self.handle_stream_supervisor(
                    &mut reader,
                    &mut write_half,
                    recent_limit_per_shard,
                    cursor,
                    provided_token,
                )
                .await
            }
            OpsHttpRoute::StreamShard {
                shard_id,
                recent_limit,
                after_sequence,
            } => {
                self.handle_stream_shard(
                    &mut reader,
                    &mut write_half,
                    shard_id,
                    recent_limit,
                    after_sequence,
                    provided_token,
                )
                .await
            }
        }
    }

    async fn handle_archive_supervisor<W>(
        &self,
        writer: &mut W,
        recent_limit_per_shard: usize,
        provided_token: Option<&str>,
    ) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
    {
        self.authorize(provided_token)?;
        let snapshot = self
            .archive
            .snapshot(recent_limit_per_shard)
            .map_err(OpsHttpError::ArchiveQuery)?;
        self.write_json_response(writer, "200 OK", &snapshot).await
    }

    async fn handle_archive_shard<W>(
        &self,
        writer: &mut W,
        shard_id: String,
        recent_limit: usize,
        provided_token: Option<&str>,
    ) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
    {
        self.authorize(provided_token)?;
        let snapshot = self
            .archive
            .shard(&shard_id)
            .ok_or_else(|| OpsHttpError::UnknownShard {
                shard_id: shard_id.clone(),
            })?
            .snapshot(recent_limit)
            .map_err(OpsHttpError::ArchiveQuery)?;
        self.write_json_response(writer, "200 OK", &snapshot).await
    }

    async fn handle_replay_supervisor<W>(
        &self,
        writer: &mut W,
        cursor: ShardSupervisorOpsReplayCursor,
        limit_per_shard: usize,
        provided_token: Option<&str>,
    ) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
    {
        self.authorize(provided_token)?;
        let snapshot = self.live.replay_snapshot(&cursor, limit_per_shard);
        self.write_json_response(writer, "200 OK", &snapshot).await
    }

    async fn handle_replay_shard<W>(
        &self,
        writer: &mut W,
        shard_id: String,
        after_sequence: u64,
        limit: usize,
        provided_token: Option<&str>,
    ) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
    {
        self.authorize(provided_token)?;
        let snapshot = self
            .live
            .shard(&shard_id)
            .ok_or_else(|| OpsHttpError::UnknownShard {
                shard_id: shard_id.clone(),
            })?
            .replay_snapshot(after_sequence, limit);
        self.write_json_response(writer, "200 OK", &snapshot).await
    }

    async fn handle_stream_supervisor<R, W>(
        &self,
        reader: &mut R,
        writer: &mut W,
        recent_limit_per_shard: usize,
        cursor: Option<ShardSupervisorOpsReplayCursor>,
        provided_token: Option<&str>,
    ) -> Result<(), OpsHttpError>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let snapshot = self.build_stream_subscription(
            OpsHttpRoute::StreamSupervisor {
                recent_limit_per_shard,
                cursor,
            },
            provided_token,
        )?;
        self.stream_subscription(reader, writer, snapshot).await
    }

    async fn handle_stream_shard<R, W>(
        &self,
        reader: &mut R,
        writer: &mut W,
        shard_id: String,
        recent_limit: usize,
        after_sequence: Option<u64>,
        provided_token: Option<&str>,
    ) -> Result<(), OpsHttpError>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let snapshot = self.build_stream_subscription(
            OpsHttpRoute::StreamShard {
                shard_id,
                recent_limit,
                after_sequence,
            },
            provided_token,
        )?;
        self.stream_subscription(reader, writer, snapshot).await
    }

    async fn stream_subscription<R, W>(
        &self,
        reader: &mut R,
        writer: &mut W,
        subscription: OpsHttpStreamSubscription,
    ) -> Result<(), OpsHttpError>
    where
        R: AsyncBufRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        let OpsHttpStreamSubscription {
            initial_event,
            live_handles,
            mut bookmark_state,
        } = subscription;
        self.write_sse_headers(writer).await?;
        if let Err(err) = self.write_sse_initial_event(writer, initial_event).await {
            if err.is_disconnect() {
                return Ok(());
            }
            let _ = self.write_sse_error(writer, &err).await;
            return Ok(());
        }

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = watch::channel(false);
        for handle in live_handles {
            spawn_relay_forwarder(handle, event_tx.clone(), cancel_rx.clone());
        }
        drop(event_tx);

        loop {
            tokio::select! {
                maybe_event = event_rx.recv() => {
                    let Some(event) = maybe_event else {
                        break;
                    };
                    if let Err(err) = self
                        .write_sse_live_event(writer, event, &mut bookmark_state)
                        .await
                    {
                        if !err.is_disconnect() {
                            let _ = self.write_sse_error(writer, &err).await;
                        }
                        break;
                    }
                }
                client_read = read_capped_line(reader, self.config.max_request_bytes) => {
                    match client_read {
                        Ok(Some(_)) => {}
                        Ok(None) => break,
                        Err(ReadLineError::Io(source)) => return Err(OpsHttpError::ReadRequest(source)),
                        Err(ReadLineError::TooLarge) => {
                            let err = OpsHttpError::RequestTooLarge {
                                max_request_bytes: self.config.max_request_bytes,
                            };
                            let _ = self.write_sse_error(writer, &err).await;
                            break;
                        }
                    }
                }
            }
        }

        let _ = cancel_tx.send(true);
        match writer.shutdown().await {
            Ok(()) => Ok(()),
            Err(source)
                if matches!(
                    source.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::UnexpectedEof
                ) =>
            {
                Ok(())
            }
            Err(source) => Err(OpsHttpError::WriteResponse(source)),
        }
    }

    fn build_stream_subscription(
        &self,
        route: OpsHttpRoute,
        provided_token: Option<&str>,
    ) -> Result<OpsHttpStreamSubscription, OpsHttpError> {
        self.authorize(provided_token)?;
        match route {
            OpsHttpRoute::StreamShard {
                shard_id,
                recent_limit,
                after_sequence,
            } => {
                let live_handle =
                    self.live
                        .shard(&shard_id)
                        .ok_or_else(|| OpsHttpError::UnknownShard {
                            shard_id: shard_id.clone(),
                        })?;
                let (initial_event, bookmark_state) = match after_sequence {
                    Some(after_sequence) => {
                        let snapshot = live_handle.replay_snapshot(after_sequence, recent_limit);
                        let bookmark_state =
                            OpsHttpBookmarkState::Shard(snapshot.next_cursor.clone());
                        (OpsHttpInitialEvent::ShardReplay(snapshot), bookmark_state)
                    }
                    None => {
                        let archive_handle = self.archive.shard(&shard_id).ok_or_else(|| {
                            OpsHttpError::UnknownShard {
                                shard_id: shard_id.clone(),
                            }
                        })?;
                        let snapshot = archive_handle
                            .snapshot(recent_limit)
                            .map_err(OpsHttpError::ArchiveQuery)?;
                        (
                            OpsHttpInitialEvent::Relay(OpsRelayEvent::ShardSnapshot(snapshot)),
                            OpsHttpBookmarkState::Shard(AuthorityShardOpsReplayCursor {
                                shard_id: shard_id.clone(),
                                last_sequence: live_handle.latest_sequence(),
                            }),
                        )
                    }
                };
                Ok(OpsHttpStreamSubscription {
                    initial_event,
                    live_handles: vec![live_handle.clone()],
                    bookmark_state,
                })
            }
            OpsHttpRoute::StreamSupervisor {
                recent_limit_per_shard,
                cursor,
            } => {
                let (initial_event, bookmark_state) = match cursor {
                    Some(cursor) => {
                        let snapshot = self.live.replay_snapshot(&cursor, recent_limit_per_shard);
                        let bookmark_state =
                            OpsHttpBookmarkState::Supervisor(snapshot.next_cursor.clone());
                        (
                            OpsHttpInitialEvent::SupervisorReplay(snapshot),
                            bookmark_state,
                        )
                    }
                    None => {
                        let snapshot = self
                            .archive
                            .snapshot(recent_limit_per_shard)
                            .map_err(OpsHttpError::ArchiveQuery)?;
                        (
                            OpsHttpInitialEvent::Relay(OpsRelayEvent::SupervisorSnapshot(snapshot)),
                            OpsHttpBookmarkState::Supervisor(build_supervisor_replay_cursor(
                                self.live.shards(),
                            )),
                        )
                    }
                };
                Ok(OpsHttpStreamSubscription {
                    initial_event,
                    live_handles: self.live.shards().to_vec(),
                    bookmark_state,
                })
            }
            OpsHttpRoute::ArchiveSupervisor { .. }
            | OpsHttpRoute::ArchiveShard { .. }
            | OpsHttpRoute::ReplaySupervisor { .. }
            | OpsHttpRoute::ReplayShard { .. } => Err(OpsHttpError::UnknownRoute {
                path: "non-stream route cannot open an SSE subscription".to_string(),
            }),
        }
    }

    fn authorize(&self, provided_token: Option<&str>) -> Result<(), OpsHttpError> {
        match (&self.config.auth_token, provided_token) {
            (Some(expected), Some(provided)) if expected == provided => Ok(()),
            (Some(_), _) => Err(OpsHttpError::Unauthorized),
            (None, _) => Ok(()),
        }
    }

    async fn write_json_response<W, T>(
        &self,
        writer: &mut W,
        status: &str,
        value: &T,
    ) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
        T: Serialize,
    {
        let body = serde_json::to_vec(value).map_err(OpsHttpError::EncodeResponse)?;
        if body.len() > self.config.max_response_bytes {
            return Err(OpsHttpError::ResponseTooLarge {
                max_response_bytes: self.config.max_response_bytes,
            });
        }

        self.write_http_response(
            writer,
            status,
            "application/json; charset=utf-8",
            Some(&body),
            &[("Cache-Control", "no-store")],
        )
        .await
    }

    async fn write_error_response<W>(
        &self,
        writer: &mut W,
        error: &OpsHttpError,
    ) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
    {
        let body = serde_json::to_vec(&serde_json::json!({
            "error": error.to_string(),
        }))
        .map_err(OpsHttpError::EncodeResponse)?;
        if body.len() > self.config.max_response_bytes {
            return Err(OpsHttpError::ResponseTooLarge {
                max_response_bytes: self.config.max_response_bytes,
            });
        }

        self.write_http_response(
            writer,
            error.status_line(),
            "application/json; charset=utf-8",
            Some(&body),
            &[("Cache-Control", "no-store")],
        )
        .await
    }

    async fn write_http_response<W>(
        &self,
        writer: &mut W,
        status: &str,
        content_type: &str,
        body: Option<&[u8]>,
        extra_headers: &[(&str, &str)],
    ) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
    {
        let body = body.unwrap_or_default();
        let mut head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in extra_headers {
            head.push_str(name);
            head.push_str(": ");
            head.push_str(value);
            head.push_str("\r\n");
        }
        head.push_str("\r\n");

        writer
            .write_all(head.as_bytes())
            .await
            .map_err(OpsHttpError::WriteResponse)?;
        if !body.is_empty() {
            writer
                .write_all(body)
                .await
                .map_err(OpsHttpError::WriteResponse)?;
        }
        writer.flush().await.map_err(OpsHttpError::WriteResponse)?;
        writer
            .shutdown()
            .await
            .map_err(OpsHttpError::WriteResponse)?;
        Ok(())
    }

    async fn write_sse_headers<W>(&self, writer: &mut W) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
    {
        writer
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\n\r\n",
            )
            .await
            .map_err(OpsHttpError::WriteResponse)?;
        writer.flush().await.map_err(OpsHttpError::WriteResponse)?;
        Ok(())
    }

    async fn write_sse_event<W>(
        &self,
        writer: &mut W,
        event: OpsRelayEvent,
    ) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
    {
        let (event_name, data) = encode_sse_event_payload(event)?;
        self.write_sse_named_event(writer, event_name, &data).await
    }

    async fn write_sse_live_event<W>(
        &self,
        writer: &mut W,
        event: OpsRelayEvent,
        bookmark_state: &mut OpsHttpBookmarkState,
    ) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
    {
        match event {
            OpsRelayEvent::ShardDocument {
                shard_id,
                sequence,
                document,
                retained_document_count,
                persisted_document_count,
                archive_path,
            } => {
                let next_bookmark = bookmark_state.update_and_encode(&shard_id, sequence);
                let data = serde_json::to_string(&serde_json::json!({
                    "shard_id": shard_id,
                    "sequence": sequence,
                    "document": document,
                    "retained_document_count": retained_document_count,
                    "persisted_document_count": persisted_document_count,
                    "archive_path": archive_path,
                    "next_bookmark": next_bookmark,
                }))
                .map_err(OpsHttpError::EncodeEvent)?;
                self.write_sse_named_event(writer, "shard_document", &data)
                    .await
            }
            other => self.write_sse_event(writer, other).await,
        }
    }

    async fn write_sse_initial_event<W>(
        &self,
        writer: &mut W,
        event: OpsHttpInitialEvent,
    ) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
    {
        let (event_name, data) = encode_sse_initial_event_payload(event)?;
        self.write_sse_named_event(writer, event_name, &data).await
    }

    async fn write_sse_named_event<W>(
        &self,
        writer: &mut W,
        event_name: &str,
        data: &str,
    ) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
    {
        let frame = format!("event: {event_name}\ndata: {data}\n\n");
        if frame.len() > self.config.max_event_bytes {
            return Err(OpsHttpError::EventTooLarge {
                max_event_bytes: self.config.max_event_bytes,
            });
        }

        writer
            .write_all(frame.as_bytes())
            .await
            .map_err(OpsHttpError::WriteResponse)?;
        writer.flush().await.map_err(OpsHttpError::WriteResponse)?;
        Ok(())
    }

    async fn write_sse_error<W>(
        &self,
        writer: &mut W,
        error: &OpsHttpError,
    ) -> Result<(), OpsHttpError>
    where
        W: AsyncWrite + Unpin,
    {
        let data = serde_json::to_string(&serde_json::json!({
            "error": error.to_string(),
        }))
        .map_err(OpsHttpError::EncodeEvent)?;
        self.write_sse_named_event(writer, "error", &data).await
    }
}

#[derive(Debug)]
pub enum OpsHttpError {
    Bind {
        address: String,
        source: std::io::Error,
    },
    Accept(std::io::Error),
    ReadRequest(std::io::Error),
    WriteResponse(std::io::Error),
    EncodeResponse(serde_json::Error),
    EncodeEvent(serde_json::Error),
    RequestTooLarge {
        max_request_bytes: usize,
    },
    ResponseTooLarge {
        max_response_bytes: usize,
    },
    EventTooLarge {
        max_event_bytes: usize,
    },
    InvalidRequestLine {
        line: String,
    },
    InvalidHeader {
        line: String,
    },
    UnsupportedMethod {
        method: String,
    },
    InvalidQueryParameter {
        name: String,
        value: String,
    },
    ConflictingQueryParameters {
        names: String,
    },
    UnknownRoute {
        path: String,
    },
    Unauthorized,
    UnknownShard {
        shard_id: String,
    },
    ArchiveQuery(AuthorityOpsArchiveError),
}

impl OpsHttpError {
    fn can_respond(&self) -> bool {
        !matches!(
            self,
            Self::Bind { .. } | Self::Accept(_) | Self::ReadRequest(_) | Self::WriteResponse(_)
        )
    }

    fn is_disconnect(&self) -> bool {
        matches!(
            self,
            Self::WriteResponse(source)
                if matches!(
                    source.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::UnexpectedEof
                )
        )
    }

    fn status_line(&self) -> &'static str {
        match self {
            Self::Unauthorized => "401 Unauthorized",
            Self::UnknownRoute { .. } | Self::UnknownShard { .. } => "404 Not Found",
            Self::RequestTooLarge { .. } => "413 Payload Too Large",
            Self::InvalidRequestLine { .. }
            | Self::InvalidHeader { .. }
            | Self::UnsupportedMethod { .. }
            | Self::InvalidQueryParameter { .. }
            | Self::ConflictingQueryParameters { .. } => "400 Bad Request",
            Self::Bind { .. }
            | Self::Accept(_)
            | Self::ReadRequest(_)
            | Self::WriteResponse(_)
            | Self::EncodeResponse(_)
            | Self::EncodeEvent(_)
            | Self::ResponseTooLarge { .. }
            | Self::EventTooLarge { .. }
            | Self::ArchiveQuery(_) => "500 Internal Server Error",
        }
    }
}

impl std::fmt::Display for OpsHttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bind { address, source } => {
                write!(f, "failed to bind ops HTTP service on {address}: {source}")
            }
            Self::Accept(source) => write!(f, "failed to accept ops HTTP client: {source}"),
            Self::ReadRequest(source) => write!(f, "failed to read ops HTTP request: {source}"),
            Self::WriteResponse(source) => {
                write!(f, "failed to write ops HTTP response: {source}")
            }
            Self::EncodeResponse(source) => {
                write!(f, "failed to encode ops HTTP response: {source}")
            }
            Self::EncodeEvent(source) => write!(f, "failed to encode ops SSE event: {source}"),
            Self::RequestTooLarge { max_request_bytes } => {
                write!(f, "ops HTTP request exceeded {max_request_bytes} bytes")
            }
            Self::ResponseTooLarge { max_response_bytes } => {
                write!(f, "ops HTTP response exceeded {max_response_bytes} bytes")
            }
            Self::EventTooLarge { max_event_bytes } => {
                write!(f, "ops SSE event exceeded {max_event_bytes} bytes")
            }
            Self::InvalidRequestLine { line } => {
                write!(f, "invalid HTTP request line: {line}")
            }
            Self::InvalidHeader { line } => write!(f, "invalid HTTP header: {line}"),
            Self::UnsupportedMethod { method } => {
                write!(f, "unsupported HTTP method '{method}'")
            }
            Self::InvalidQueryParameter { name, value } => {
                write!(f, "invalid query parameter '{name}={value}'")
            }
            Self::ConflictingQueryParameters { names } => {
                write!(f, "conflicting query parameters: {names}")
            }
            Self::UnknownRoute { path } => write!(f, "unknown ops HTTP route '{path}'"),
            Self::Unauthorized => write!(f, "ops HTTP auth token was rejected"),
            Self::UnknownShard { shard_id } => write!(f, "unknown shard '{shard_id}'"),
            Self::ArchiveQuery(source) => write!(f, "{source}"),
        }
    }
}

impl std::error::Error for OpsHttpError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bind { source, .. }
            | Self::Accept(source)
            | Self::ReadRequest(source)
            | Self::WriteResponse(source) => Some(source),
            Self::EncodeResponse(source) | Self::EncodeEvent(source) => Some(source),
            Self::ArchiveQuery(source) => Some(source),
            Self::RequestTooLarge { .. }
            | Self::ResponseTooLarge { .. }
            | Self::EventTooLarge { .. }
            | Self::InvalidRequestLine { .. }
            | Self::InvalidHeader { .. }
            | Self::UnsupportedMethod { .. }
            | Self::InvalidQueryParameter { .. }
            | Self::ConflictingQueryParameters { .. }
            | Self::UnknownRoute { .. }
            | Self::Unauthorized
            | Self::UnknownShard { .. } => None,
        }
    }
}

#[derive(Clone, Debug)]
enum OpsHttpRoute {
    ArchiveSupervisor {
        recent_limit_per_shard: usize,
    },
    ArchiveShard {
        shard_id: String,
        recent_limit: usize,
    },
    ReplaySupervisor {
        cursor: ShardSupervisorOpsReplayCursor,
        limit_per_shard: usize,
    },
    ReplayShard {
        shard_id: String,
        after_sequence: u64,
        limit: usize,
    },
    StreamSupervisor {
        recent_limit_per_shard: usize,
        cursor: Option<ShardSupervisorOpsReplayCursor>,
    },
    StreamShard {
        shard_id: String,
        recent_limit: usize,
        after_sequence: Option<u64>,
    },
}

#[derive(Clone, Debug)]
enum OpsHttpInitialEvent {
    Relay(OpsRelayEvent),
    SupervisorReplay(ShardSupervisorOpsReplaySnapshot),
    ShardReplay(AuthorityShardOpsReplaySnapshot),
}

#[derive(Clone, Debug)]
enum OpsHttpBookmarkState {
    Shard(AuthorityShardOpsReplayCursor),
    Supervisor(ShardSupervisorOpsReplayCursor),
}

impl OpsHttpBookmarkState {
    fn update_and_encode(&mut self, shard_id: &str, sequence: u64) -> Option<String> {
        match self {
            Self::Shard(cursor) => {
                if cursor.shard_id != shard_id {
                    return None;
                }
                cursor.last_sequence = sequence;
                Some(encode_shard_replay_bookmark(cursor))
            }
            Self::Supervisor(cursor) => {
                if let Some(shard_cursor) = cursor
                    .shards
                    .iter_mut()
                    .find(|cursor| cursor.shard_id == shard_id)
                {
                    shard_cursor.last_sequence = sequence;
                } else {
                    cursor.shards.push(AuthorityShardOpsReplayCursor {
                        shard_id: shard_id.to_string(),
                        last_sequence: sequence,
                    });
                }
                Some(encode_supervisor_replay_bookmark(cursor))
            }
        }
    }
}

#[derive(Clone, Debug)]
struct OpsHttpStreamSubscription {
    initial_event: OpsHttpInitialEvent,
    live_handles: Vec<AuthorityShardOpsHandle>,
    bookmark_state: OpsHttpBookmarkState,
}

#[derive(Debug)]
struct ParsedHttpRequest {
    target: String,
    headers: HashMap<String, String>,
}

impl ParsedHttpRequest {
    fn bearer_token(&self) -> Option<&str> {
        let authorization = self.headers.get("authorization")?;
        let (scheme, token) = authorization.split_once(' ')?;
        if scheme.eq_ignore_ascii_case("bearer") {
            Some(token.trim())
        } else {
            None
        }
    }
}

async fn read_http_request<R>(
    reader: &mut R,
    max_request_bytes: usize,
) -> Result<Option<ParsedHttpRequest>, OpsHttpError>
where
    R: AsyncBufRead + Unpin,
{
    let request_line = match read_capped_line(reader, max_request_bytes).await {
        Ok(Some(line)) => line,
        Ok(None) => return Ok(None),
        Err(ReadLineError::Io(source)) => return Err(OpsHttpError::ReadRequest(source)),
        Err(ReadLineError::TooLarge) => {
            return Err(OpsHttpError::RequestTooLarge { max_request_bytes });
        }
    };
    let mut consumed_bytes = request_line.len();
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| OpsHttpError::InvalidRequestLine {
            line: request_line.clone(),
        })?;
    let target = parts
        .next()
        .ok_or_else(|| OpsHttpError::InvalidRequestLine {
            line: request_line.clone(),
        })?;
    let version = parts
        .next()
        .ok_or_else(|| OpsHttpError::InvalidRequestLine {
            line: request_line.clone(),
        })?;
    if parts.next().is_some() || !version.starts_with("HTTP/1.") {
        return Err(OpsHttpError::InvalidRequestLine { line: request_line });
    }
    if !method.eq_ignore_ascii_case("GET") {
        return Err(OpsHttpError::UnsupportedMethod {
            method: method.to_string(),
        });
    }

    let mut headers = HashMap::new();
    loop {
        let line = match read_capped_line(reader, max_request_bytes).await {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(ReadLineError::Io(source)) => return Err(OpsHttpError::ReadRequest(source)),
            Err(ReadLineError::TooLarge) => {
                return Err(OpsHttpError::RequestTooLarge { max_request_bytes });
            }
        };
        consumed_bytes += line.len();
        if consumed_bytes > max_request_bytes {
            return Err(OpsHttpError::RequestTooLarge { max_request_bytes });
        }
        if line.is_empty() {
            break;
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| OpsHttpError::InvalidHeader { line: line.clone() })?;
        headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
    }

    Ok(Some(ParsedHttpRequest {
        target: target.to_string(),
        headers,
    }))
}

fn parse_http_route(target: &str) -> Result<OpsHttpRoute, OpsHttpError> {
    let (path, query) = match target.split_once('?') {
        Some((path, query)) => (path, query),
        None => (target, ""),
    };
    let params = parse_http_query(query);

    match path {
        "/ops/archive/supervisor" => Ok(OpsHttpRoute::ArchiveSupervisor {
            recent_limit_per_shard: parse_http_query_usize(
                &params,
                "recent_limit_per_shard",
                OPS_DOCUMENT_HISTORY_LIMIT,
            )?,
        }),
        "/ops/replay/supervisor" => Ok(OpsHttpRoute::ReplaySupervisor {
            cursor: parse_http_supervisor_replay_query(&params)?.unwrap_or_default(),
            limit_per_shard: parse_http_query_usize(
                &params,
                "limit_per_shard",
                OPS_DOCUMENT_HISTORY_LIMIT,
            )?,
        }),
        "/ops/stream/supervisor" => Ok(OpsHttpRoute::StreamSupervisor {
            recent_limit_per_shard: parse_http_query_usize(
                &params,
                "recent_limit_per_shard",
                OPS_DOCUMENT_HISTORY_LIMIT,
            )?,
            cursor: parse_http_supervisor_replay_query(&params)?,
        }),
        _ if path.starts_with("/ops/archive/shard/") => {
            let shard_id = path.trim_start_matches("/ops/archive/shard/");
            if shard_id.is_empty() || shard_id.contains('/') {
                return Err(OpsHttpError::UnknownRoute {
                    path: path.to_string(),
                });
            }
            Ok(OpsHttpRoute::ArchiveShard {
                shard_id: shard_id.to_string(),
                recent_limit: parse_http_query_usize(
                    &params,
                    "recent_limit",
                    OPS_DOCUMENT_HISTORY_LIMIT,
                )?,
            })
        }
        _ if path.starts_with("/ops/replay/shard/") => {
            let shard_id = path.trim_start_matches("/ops/replay/shard/");
            if shard_id.is_empty() || shard_id.contains('/') {
                return Err(OpsHttpError::UnknownRoute {
                    path: path.to_string(),
                });
            }
            Ok(OpsHttpRoute::ReplayShard {
                shard_id: shard_id.to_string(),
                after_sequence: parse_http_shard_replay_query(&params, shard_id)?.unwrap_or(0),
                limit: parse_http_query_usize(&params, "limit", OPS_DOCUMENT_HISTORY_LIMIT)?,
            })
        }
        _ if path.starts_with("/ops/stream/shard/") => {
            let shard_id = path.trim_start_matches("/ops/stream/shard/");
            if shard_id.is_empty() || shard_id.contains('/') {
                return Err(OpsHttpError::UnknownRoute {
                    path: path.to_string(),
                });
            }
            Ok(OpsHttpRoute::StreamShard {
                shard_id: shard_id.to_string(),
                recent_limit: parse_http_query_usize(
                    &params,
                    "recent_limit",
                    OPS_DOCUMENT_HISTORY_LIMIT,
                )?,
                after_sequence: parse_http_shard_replay_query(&params, shard_id)?,
            })
        }
        _ => Err(OpsHttpError::UnknownRoute {
            path: path.to_string(),
        }),
    }
}

fn parse_http_query(query: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    for entry in query.split('&').filter(|entry| !entry.is_empty()) {
        let (name, value) = match entry.split_once('=') {
            Some((name, value)) => (name, value),
            None => (entry, ""),
        };
        params.insert(name.to_string(), value.to_string());
    }
    params
}

fn parse_http_query_usize(
    params: &HashMap<String, String>,
    name: &str,
    default: usize,
) -> Result<usize, OpsHttpError> {
    match params.get(name) {
        Some(value) if !value.is_empty() => {
            value
                .parse::<usize>()
                .map_err(|_| OpsHttpError::InvalidQueryParameter {
                    name: name.to_string(),
                    value: value.clone(),
                })
        }
        Some(_) | None => Ok(default),
    }
}

fn parse_http_query_u64(
    params: &HashMap<String, String>,
    name: &str,
    default: u64,
) -> Result<u64, OpsHttpError> {
    match params.get(name) {
        Some(value) if !value.is_empty() => {
            value
                .parse::<u64>()
                .map_err(|_| OpsHttpError::InvalidQueryParameter {
                    name: name.to_string(),
                    value: value.clone(),
                })
        }
        Some(_) | None => Ok(default),
    }
}

fn parse_http_replay_cursor(value: &str) -> Result<ShardSupervisorOpsReplayCursor, OpsHttpError> {
    if value.trim().is_empty() {
        return Ok(ShardSupervisorOpsReplayCursor::default());
    }

    let shards = value
        .split(',')
        .filter(|entry| !entry.trim().is_empty())
        .map(|entry| {
            let (shard_id, sequence) =
                entry
                    .split_once(':')
                    .ok_or_else(|| OpsHttpError::InvalidQueryParameter {
                        name: "cursor".to_string(),
                        value: value.to_string(),
                    })?;
            let last_sequence =
                sequence
                    .parse::<u64>()
                    .map_err(|_| OpsHttpError::InvalidQueryParameter {
                        name: "cursor".to_string(),
                        value: value.to_string(),
                    })?;
            Ok(AuthorityShardOpsReplayCursor {
                shard_id: shard_id.to_string(),
                last_sequence,
            })
        })
        .collect::<Result<Vec<_>, OpsHttpError>>()?;

    Ok(ShardSupervisorOpsReplayCursor { shards })
}

fn parse_http_supervisor_replay_query(
    params: &HashMap<String, String>,
) -> Result<Option<ShardSupervisorOpsReplayCursor>, OpsHttpError> {
    let bookmark = non_empty_http_query_value(params, "bookmark");
    let cursor = non_empty_http_query_value(params, "cursor");
    if bookmark.is_some() && cursor.is_some() {
        return Err(OpsHttpError::ConflictingQueryParameters {
            names: "bookmark,cursor".to_string(),
        });
    }

    if let Some(bookmark) = bookmark {
        return decode_supervisor_replay_bookmark(bookmark)
            .map(Some)
            .map_err(|_| OpsHttpError::InvalidQueryParameter {
                name: "bookmark".to_string(),
                value: bookmark.to_string(),
            });
    }

    cursor.map(parse_http_replay_cursor).transpose()
}

fn parse_http_shard_replay_query(
    params: &HashMap<String, String>,
    shard_id: &str,
) -> Result<Option<u64>, OpsHttpError> {
    let bookmark = non_empty_http_query_value(params, "bookmark");
    let after_sequence = non_empty_http_query_value(params, "after_sequence");
    if bookmark.is_some() && after_sequence.is_some() {
        return Err(OpsHttpError::ConflictingQueryParameters {
            names: "bookmark,after_sequence".to_string(),
        });
    }

    if let Some(bookmark) = bookmark {
        let cursor = decode_shard_replay_bookmark(bookmark).map_err(|_| {
            OpsHttpError::InvalidQueryParameter {
                name: "bookmark".to_string(),
                value: bookmark.to_string(),
            }
        })?;
        if cursor.shard_id != shard_id {
            return Err(OpsHttpError::InvalidQueryParameter {
                name: "bookmark".to_string(),
                value: bookmark.to_string(),
            });
        }
        return Ok(Some(cursor.last_sequence));
    }

    after_sequence
        .map(|_| parse_http_query_u64(params, "after_sequence", 0))
        .transpose()
}

fn non_empty_http_query_value<'a>(
    params: &'a HashMap<String, String>,
    name: &str,
) -> Option<&'a str> {
    params
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.is_empty())
}

fn encode_sse_initial_event_payload(
    event: OpsHttpInitialEvent,
) -> Result<(&'static str, String), OpsHttpError> {
    match event {
        OpsHttpInitialEvent::Relay(event) => encode_sse_event_payload(event),
        OpsHttpInitialEvent::SupervisorReplay(snapshot) => Ok((
            "supervisor_replay",
            serde_json::to_string(&snapshot).map_err(OpsHttpError::EncodeEvent)?,
        )),
        OpsHttpInitialEvent::ShardReplay(snapshot) => Ok((
            "shard_replay",
            serde_json::to_string(&snapshot).map_err(OpsHttpError::EncodeEvent)?,
        )),
    }
}

fn encode_sse_event_payload(event: OpsRelayEvent) -> Result<(&'static str, String), OpsHttpError> {
    match event {
        OpsRelayEvent::SupervisorSnapshot(snapshot) => Ok((
            "supervisor_snapshot",
            serde_json::to_string(&snapshot).map_err(OpsHttpError::EncodeEvent)?,
        )),
        OpsRelayEvent::ShardSnapshot(snapshot) => Ok((
            "shard_snapshot",
            serde_json::to_string(&snapshot).map_err(OpsHttpError::EncodeEvent)?,
        )),
        OpsRelayEvent::ShardDocument {
            shard_id,
            sequence,
            document,
            retained_document_count,
            persisted_document_count,
            archive_path,
        } => Ok((
            "shard_document",
            serde_json::to_string(&serde_json::json!({
                "shard_id": shard_id,
                "sequence": sequence,
                "document": document,
                "retained_document_count": retained_document_count,
                "persisted_document_count": persisted_document_count,
                "archive_path": archive_path,
            }))
            .map_err(OpsHttpError::EncodeEvent)?,
        )),
        OpsRelayEvent::Lagged { shard_id, skipped } => Ok((
            "lagged",
            serde_json::to_string(&serde_json::json!({
                "shard_id": shard_id,
                "skipped": skipped,
            }))
            .map_err(OpsHttpError::EncodeEvent)?,
        )),
        OpsRelayEvent::Error { message } => Ok((
            "error",
            serde_json::to_string(&serde_json::json!({
                "error": message,
            }))
            .map_err(OpsHttpError::EncodeEvent)?,
        )),
    }
}

fn build_supervisor_replay_cursor(
    handles: &[AuthorityShardOpsHandle],
) -> ShardSupervisorOpsReplayCursor {
    ShardSupervisorOpsReplayCursor {
        shards: handles
            .iter()
            .map(|handle| AuthorityShardOpsReplayCursor {
                shard_id: handle.shard().shard_id.clone(),
                last_sequence: handle.latest_sequence(),
            })
            .collect(),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpsRelayConfig {
    pub bind_address: String,
    pub max_request_bytes: usize,
    pub max_event_bytes: usize,
    pub auth_token: Option<String>,
}

impl Default for OpsRelayConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:7611".to_string(),
            max_request_bytes: 64 * 1024,
            max_event_bytes: 512 * 1024,
            auth_token: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpsRelayClient {
    pub address: String,
    pub auth_token: Option<String>,
    pub max_event_bytes: usize,
}

impl Default for OpsRelayClient {
    fn default() -> Self {
        Self {
            address: "127.0.0.1:7611".to_string(),
            auth_token: None,
            max_event_bytes: 512 * 1024,
        }
    }
}

impl OpsRelayClient {
    pub async fn subscribe_supervisor(
        &self,
        recent_limit_per_shard: usize,
    ) -> Result<OpsRelaySubscription, OpsRelayError> {
        self.subscribe(OpsRelayRequest::SubscribeSupervisor {
            recent_limit_per_shard,
            auth_token: self.auth_token.clone(),
        })
        .await
    }

    pub async fn subscribe_shard(
        &self,
        shard_id: impl Into<String>,
        recent_limit: usize,
    ) -> Result<OpsRelaySubscription, OpsRelayError> {
        self.subscribe(OpsRelayRequest::SubscribeShard {
            shard_id: shard_id.into(),
            recent_limit,
            auth_token: self.auth_token.clone(),
        })
        .await
    }

    async fn subscribe(
        &self,
        request: OpsRelayRequest,
    ) -> Result<OpsRelaySubscription, OpsRelayError> {
        let mut socket =
            TcpStream::connect(&self.address)
                .await
                .map_err(|source| OpsRelayError::Connect {
                    address: self.address.clone(),
                    source,
                })?;
        let encoded = serde_json::to_string(&request).map_err(OpsRelayError::EncodeRequest)?;
        socket
            .write_all(encoded.as_bytes())
            .await
            .map_err(OpsRelayError::WriteRequest)?;
        socket
            .write_all(b"\n")
            .await
            .map_err(OpsRelayError::WriteRequest)?;

        Ok(OpsRelaySubscription {
            reader: BufReader::new(socket),
            max_event_bytes: self.max_event_bytes,
        })
    }
}

pub struct OpsRelaySubscription {
    reader: BufReader<TcpStream>,
    max_event_bytes: usize,
}

impl OpsRelaySubscription {
    pub async fn next_event(&mut self) -> Result<Option<OpsRelayEvent>, OpsRelayError> {
        let line = match read_capped_line(&mut self.reader, self.max_event_bytes).await {
            Ok(Some(line)) => line,
            Ok(None) => return Ok(None),
            Err(ReadLineError::Io(source)) => return Err(OpsRelayError::ReadEvent(source)),
            Err(ReadLineError::TooLarge) => {
                return Err(OpsRelayError::EventTooLarge {
                    max_event_bytes: self.max_event_bytes,
                });
            }
        };
        let event =
            serde_json::from_str::<OpsRelayEvent>(&line).map_err(OpsRelayError::DecodeEvent)?;
        match event {
            OpsRelayEvent::Error { message } => Err(OpsRelayError::Remote { message }),
            event => Ok(Some(event)),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpsRelayRequest {
    SubscribeShard {
        shard_id: String,
        recent_limit: usize,
        auth_token: Option<String>,
    },
    SubscribeSupervisor {
        recent_limit_per_shard: usize,
        auth_token: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpsRelayEvent {
    ShardSnapshot(AuthorityShardOpsArchiveSnapshot),
    SupervisorSnapshot(ShardSupervisorOpsArchiveSnapshot),
    ShardDocument {
        shard_id: String,
        sequence: u64,
        document: String,
        retained_document_count: usize,
        persisted_document_count: usize,
        archive_path: Option<PathBuf>,
    },
    Lagged {
        shard_id: String,
        skipped: u64,
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Debug)]
pub struct ShardSupervisorOpsRelayService {
    live: ShardSupervisorOpsHandle,
    archive: ShardSupervisorOpsArchiveHandle,
    config: OpsRelayConfig,
}

impl ShardSupervisorOpsRelayService {
    pub async fn serve(self) -> Result<(), OpsRelayError> {
        let listener = self.bind_listener().await?;
        self.serve_listener(listener).await
    }

    pub async fn serve_once(self) -> Result<(), OpsRelayError> {
        let listener = self.bind_listener().await?;
        self.serve_once_listener(listener).await
    }

    async fn bind_listener(&self) -> Result<TcpListener, OpsRelayError> {
        TcpListener::bind(&self.config.bind_address)
            .await
            .map_err(|source| OpsRelayError::Bind {
                address: self.config.bind_address.clone(),
                source,
            })
    }

    async fn serve_listener(self, listener: TcpListener) -> Result<(), OpsRelayError> {
        loop {
            let (socket, _) = listener.accept().await.map_err(OpsRelayError::Accept)?;
            self.handle_socket(socket).await?;
        }
    }

    async fn serve_once_listener(self, listener: TcpListener) -> Result<(), OpsRelayError> {
        let (socket, _) = listener.accept().await.map_err(OpsRelayError::Accept)?;
        self.handle_socket(socket).await
    }

    async fn handle_socket(&self, socket: TcpStream) -> Result<(), OpsRelayError> {
        let (read_half, mut write_half) = socket.into_split();
        let mut reader = BufReader::new(read_half);
        let request_line = match read_capped_line(&mut reader, self.config.max_request_bytes).await
        {
            Ok(Some(line)) => line,
            Ok(None) => return Ok(()),
            Err(ReadLineError::Io(source)) => return Err(OpsRelayError::ReadRequest(source)),
            Err(ReadLineError::TooLarge) => {
                self.write_event(
                    &mut write_half,
                    OpsRelayEvent::Error {
                        message: format!(
                            "ops relay request exceeded {} bytes",
                            self.config.max_request_bytes
                        ),
                    },
                )
                .await?;
                return Ok(());
            }
        };
        let request = serde_json::from_str::<OpsRelayRequest>(&request_line)
            .map_err(OpsRelayError::DecodeRequest)?;
        let (initial_event, live_handles) = match self.build_subscription(request) {
            Ok(subscription) => subscription,
            Err(err) => {
                self.write_event(
                    &mut write_half,
                    OpsRelayEvent::Error {
                        message: err.to_string(),
                    },
                )
                .await?;
                return Ok(());
            }
        };
        self.write_event(&mut write_half, initial_event).await?;

        let (event_tx, mut event_rx) = mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = watch::channel(false);
        for handle in live_handles {
            spawn_relay_forwarder(handle, event_tx.clone(), cancel_rx.clone());
        }
        drop(event_tx);

        loop {
            tokio::select! {
                maybe_event = event_rx.recv() => {
                    let Some(event) = maybe_event else {
                        break;
                    };
                    if let Err(err) = self.write_event(&mut write_half, event).await {
                        if err.is_disconnect() {
                            break;
                        }
                        return Err(err);
                    }
                }
                client_read = read_capped_line(&mut reader, self.config.max_request_bytes) => {
                    match client_read {
                        Ok(Some(_)) => {}
                        Ok(None) => break,
                        Err(ReadLineError::Io(source)) => return Err(OpsRelayError::ReadRequest(source)),
                        Err(ReadLineError::TooLarge) => {
                            return Err(OpsRelayError::RequestTooLarge {
                                max_request_bytes: self.config.max_request_bytes,
                            });
                        }
                    }
                }
            }
        }

        let _ = cancel_tx.send(true);
        Ok(())
    }

    fn build_subscription(
        &self,
        request: OpsRelayRequest,
    ) -> Result<(OpsRelayEvent, Vec<AuthorityShardOpsHandle>), OpsRelayError> {
        match request {
            OpsRelayRequest::SubscribeShard {
                shard_id,
                recent_limit,
                auth_token,
            } => {
                self.authorize(auth_token.as_deref())?;
                let archive_handle =
                    self.archive
                        .shard(&shard_id)
                        .ok_or_else(|| OpsRelayError::UnknownShard {
                            shard_id: shard_id.clone(),
                        })?;
                let live_handle =
                    self.live
                        .shard(&shard_id)
                        .ok_or_else(|| OpsRelayError::UnknownShard {
                            shard_id: shard_id.clone(),
                        })?;
                let snapshot = archive_handle
                    .snapshot(recent_limit)
                    .map_err(OpsRelayError::ArchiveQuery)?;
                Ok((
                    OpsRelayEvent::ShardSnapshot(snapshot),
                    vec![live_handle.clone()],
                ))
            }
            OpsRelayRequest::SubscribeSupervisor {
                recent_limit_per_shard,
                auth_token,
            } => {
                self.authorize(auth_token.as_deref())?;
                let snapshot = self
                    .archive
                    .snapshot(recent_limit_per_shard)
                    .map_err(OpsRelayError::ArchiveQuery)?;
                Ok((
                    OpsRelayEvent::SupervisorSnapshot(snapshot),
                    self.live.shards().to_vec(),
                ))
            }
        }
    }

    fn authorize(&self, provided_token: Option<&str>) -> Result<(), OpsRelayError> {
        match (&self.config.auth_token, provided_token) {
            (Some(expected), Some(provided)) if expected == provided => Ok(()),
            (Some(_), _) => Err(OpsRelayError::Unauthorized),
            (None, _) => Ok(()),
        }
    }

    async fn write_event<W>(
        &self,
        writer: &mut W,
        event: OpsRelayEvent,
    ) -> Result<(), OpsRelayError>
    where
        W: AsyncWrite + Unpin,
    {
        let encoded = serde_json::to_string(&event).map_err(OpsRelayError::EncodeEvent)?;
        if encoded.len() + 1 > self.config.max_event_bytes {
            return Err(OpsRelayError::EventTooLarge {
                max_event_bytes: self.config.max_event_bytes,
            });
        }

        writer
            .write_all(encoded.as_bytes())
            .await
            .map_err(OpsRelayError::WriteEvent)?;
        writer
            .write_all(b"\n")
            .await
            .map_err(OpsRelayError::WriteEvent)?;
        writer.flush().await.map_err(OpsRelayError::WriteEvent)?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum OpsRelayError {
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
    ReadEvent(std::io::Error),
    WriteRequest(std::io::Error),
    WriteEvent(std::io::Error),
    EncodeRequest(serde_json::Error),
    EncodeEvent(serde_json::Error),
    DecodeRequest(serde_json::Error),
    DecodeEvent(serde_json::Error),
    RequestTooLarge {
        max_request_bytes: usize,
    },
    EventTooLarge {
        max_event_bytes: usize,
    },
    Unauthorized,
    UnknownShard {
        shard_id: String,
    },
    Remote {
        message: String,
    },
    ArchiveQuery(AuthorityOpsArchiveError),
}

impl OpsRelayError {
    fn is_disconnect(&self) -> bool {
        matches!(
            self,
            Self::WriteEvent(source)
                if matches!(
                    source.kind(),
                    std::io::ErrorKind::BrokenPipe
                        | std::io::ErrorKind::ConnectionAborted
                        | std::io::ErrorKind::ConnectionReset
                        | std::io::ErrorKind::UnexpectedEof
                )
        )
    }
}

impl std::fmt::Display for OpsRelayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bind { address, source } => {
                write!(f, "failed to bind ops relay on {address}: {source}")
            }
            Self::Accept(source) => write!(f, "failed to accept ops relay client: {source}"),
            Self::Connect { address, source } => {
                write!(f, "failed to connect to ops relay at {address}: {source}")
            }
            Self::ReadRequest(source) => write!(f, "failed to read ops relay request: {source}"),
            Self::ReadEvent(source) => write!(f, "failed to read ops relay event: {source}"),
            Self::WriteRequest(source) => write!(f, "failed to write ops relay request: {source}"),
            Self::WriteEvent(source) => write!(f, "failed to write ops relay event: {source}"),
            Self::EncodeRequest(source) => {
                write!(f, "failed to encode ops relay request: {source}")
            }
            Self::EncodeEvent(source) => write!(f, "failed to encode ops relay event: {source}"),
            Self::DecodeRequest(source) => {
                write!(f, "failed to decode ops relay request: {source}")
            }
            Self::DecodeEvent(source) => write!(f, "failed to decode ops relay event: {source}"),
            Self::RequestTooLarge { max_request_bytes } => {
                write!(f, "ops relay request exceeded {max_request_bytes} bytes")
            }
            Self::EventTooLarge { max_event_bytes } => {
                write!(f, "ops relay event exceeded {max_event_bytes} bytes")
            }
            Self::Unauthorized => write!(f, "ops relay auth token was rejected"),
            Self::UnknownShard { shard_id } => write!(f, "unknown shard '{shard_id}'"),
            Self::Remote { message } => write!(f, "ops relay returned an error: {message}"),
            Self::ArchiveQuery(source) => write!(f, "{source}"),
        }
    }
}

impl std::error::Error for OpsRelayError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bind { source, .. }
            | Self::Accept(source)
            | Self::Connect { source, .. }
            | Self::ReadRequest(source)
            | Self::ReadEvent(source)
            | Self::WriteRequest(source)
            | Self::WriteEvent(source) => Some(source),
            Self::EncodeRequest(source)
            | Self::EncodeEvent(source)
            | Self::DecodeRequest(source)
            | Self::DecodeEvent(source) => Some(source),
            Self::ArchiveQuery(source) => Some(source),
            Self::RequestTooLarge { .. }
            | Self::EventTooLarge { .. }
            | Self::Unauthorized
            | Self::UnknownShard { .. }
            | Self::Remote { .. } => None,
        }
    }
}

fn spawn_relay_forwarder(
    handle: AuthorityShardOpsHandle,
    event_tx: mpsc::UnboundedSender<OpsRelayEvent>,
    mut cancel_rx: watch::Receiver<bool>,
) {
    let shard_id = handle.shard().shard_id.clone();
    let mut subscription = handle.subscribe_documents();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                changed = cancel_rx.changed() => {
                    if changed.is_err() || *cancel_rx.borrow() {
                        break;
                    }
                }
                received = subscription.recv() => {
                    match received {
                        Ok(document) => {
                            if event_tx.send(OpsRelayEvent::ShardDocument {
                                shard_id: shard_id.clone(),
                                sequence: document.sequence,
                                document: document.document,
                                retained_document_count: handle.retained_document_count(),
                                persisted_document_count: handle.persisted_document_count(),
                                archive_path: handle.archive_path(),
                            }).is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Lagged(skipped)) => {
                            if event_tx.send(OpsRelayEvent::Lagged {
                                shard_id: shard_id.clone(),
                                skipped: skipped as u64,
                            }).is_err() {
                                break;
                            }
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
    });
}

#[derive(Debug)]
enum ReadCappedError {
    Io(std::io::Error),
    TooLarge,
}

#[derive(Debug)]
enum ReadLineError {
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

async fn read_capped_line<R>(
    reader: &mut R,
    max_bytes: usize,
) -> Result<Option<String>, ReadLineError>
where
    R: AsyncBufRead + Unpin,
{
    let mut line = String::new();
    let read = reader
        .read_line(&mut line)
        .await
        .map_err(ReadLineError::Io)?;
    if read == 0 {
        return Ok(None);
    }
    if line.len() > max_bytes {
        return Err(ReadLineError::TooLarge);
    }

    while matches!(line.as_bytes().last(), Some(b'\n' | b'\r')) {
        line.pop();
    }

    Ok(Some(line))
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

    pub fn ops_relay(&self, config: OpsRelayConfig) -> ShardSupervisorOpsRelayService {
        self.ops_handle().relay(config)
    }

    pub fn ops_http_service(&self, config: OpsHttpServiceConfig) -> ShardSupervisorOpsHttpService {
        self.ops_handle().http_service(config)
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
        decode_shard_replay_bookmark, decode_supervisor_replay_bookmark,
        encode_shard_replay_bookmark, encode_supervisor_replay_bookmark, AuthorityHostConfig,
        AuthorityHostRuntime, AuthorityShardConfig, AuthorityShardControlPlaneHandle,
        AuthorityShardLifecycleCommandKind, AuthorityShardLifecyclePhase, AuthorityShardOpsHandle,
        AuthorityShardOpsReplaySnapshot, AuthorityTransportMode, DirectConnectAuthorityRuntime,
        LocalAuthorityOpsStream, LocalAuthorityRuntime, OpsArchiveServiceClient,
        OpsArchiveServiceConfig, OpsArchiveServiceRequest, OpsArchiveServiceResponse,
        OpsHttpServiceConfig, OpsPersistenceConfig, OpsRelayClient, OpsRelayConfig, OpsRelayError,
        OpsRelayEvent, PreparedAuthorityShard, ShardSupervisorConfig,
        ShardSupervisorControlPlaneHandle, ShardSupervisorError, ShardSupervisorOpsArchiveSnapshot,
        ShardSupervisorOpsHandle, ShardSupervisorOpsReplaySnapshot, ShardSupervisorSummary,
        OPS_DOCUMENT_CHANNEL_CAPACITY,
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
    use serde::Deserialize;
    use std::collections::HashMap;
    use std::fs;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;
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
        let value = decode_toon_value(&first_document.document)
            .expect("ops document should decode as TOON");
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

    #[tokio::test]
    async fn supervisor_ops_relay_requires_auth_and_streams_live_documents() {
        let archive_root_dir = std::env::temp_dir().join(format!(
            "pod-host-ops-relay-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        let mut host = sample_config(AuthorityTransportMode::Local);
        host.ops_persistence = Some(OpsPersistenceConfig {
            archive_root_dir: archive_root_dir.clone(),
        });

        let mut runtime = match host
            .prepare_runtime_with_shard_id("alpha-1", |world, _map_name| {
                world
                    .spawn_at(6.0, 6.0)
                    .with_label("ops-relay-marker", pod_core::Team::None)
                    .build();
            })
            .expect("archive-backed local runtime should build")
        {
            AuthorityHostRuntime::Local(runtime) => runtime,
            AuthorityHostRuntime::DirectConnect(_) => panic!("expected local host runtime"),
        };
        runtime.step(Duration::from_secs_f32(1.0 / host.tick_rate as f32));

        let relay = ShardSupervisorOpsHandle {
            shards: vec![runtime.ops_handle()],
        }
        .relay(OpsRelayConfig {
            bind_address: "127.0.0.1:0".to_string(),
            max_request_bytes: 16 * 1024,
            max_event_bytes: 256 * 1024,
            auth_token: Some("relay-secret".to_string()),
        });
        let listener = relay
            .bind_listener()
            .await
            .expect("ops relay should bind an ephemeral listener");
        let address = listener
            .local_addr()
            .expect("ops relay listener should expose a local address")
            .to_string();
        let service_task = tokio::spawn(relay.serve_listener(listener));

        let mut rejected_subscription = OpsRelayClient {
            address: address.clone(),
            auth_token: Some("wrong-secret".to_string()),
            max_event_bytes: 256 * 1024,
        }
        .subscribe_supervisor(8)
        .await
        .expect("relay client should connect before auth is evaluated");
        match rejected_subscription.next_event().await {
            Err(OpsRelayError::Remote { message }) => {
                assert!(message.contains("rejected"));
            }
            other => panic!("expected auth rejection event, got {other:?}"),
        }
        drop(rejected_subscription);

        let mut subscription = OpsRelayClient {
            address,
            auth_token: Some("relay-secret".to_string()),
            max_event_bytes: 256 * 1024,
        }
        .subscribe_supervisor(8)
        .await
        .expect("authorized relay client should connect");
        match subscription
            .next_event()
            .await
            .expect("relay client should decode initial event")
            .expect("relay should emit an initial snapshot")
        {
            OpsRelayEvent::SupervisorSnapshot(snapshot) => {
                assert_eq!(snapshot.shard_count, 1);
                assert_eq!(snapshot.archived_shard_count, 1);
                assert!(snapshot.total_persisted_document_count >= 1);
            }
            other => panic!("expected supervisor snapshot event, got {other:?}"),
        }

        runtime.step(Duration::from_secs_f32(1.0 / host.tick_rate as f32));

        match subscription
            .next_event()
            .await
            .expect("relay client should decode live shard document")
            .expect("relay should stream a live shard document")
        {
            OpsRelayEvent::ShardDocument {
                shard_id,
                sequence,
                document,
                retained_document_count,
                persisted_document_count,
                archive_path,
            } => {
                assert_eq!(shard_id, "alpha-1");
                assert!(sequence >= 1);
                assert!(retained_document_count >= 1);
                assert!(persisted_document_count >= 1);
                assert!(archive_path.is_some());
                assert_eq!(
                    decode_toon_value(&document).expect("relay document should decode as TOON")
                        ["document_type"],
                    "versioned_tick_telemetry"
                );
            }
            other => panic!("expected streamed shard document event, got {other:?}"),
        }

        drop(subscription);
        service_task.abort();
        let _ = service_task.await;
        fs::remove_dir_all(&archive_root_dir).expect("temp archive root should be removable");
    }

    #[tokio::test]
    async fn supervisor_ops_http_service_serves_archive_json_and_sse_streams() {
        let archive_root_dir = std::env::temp_dir().join(format!(
            "pod-host-ops-http-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        let mut host = sample_config(AuthorityTransportMode::Local);
        host.ops_persistence = Some(OpsPersistenceConfig {
            archive_root_dir: archive_root_dir.clone(),
        });

        let mut runtime = match host
            .prepare_runtime_with_shard_id("alpha-1", |world, _map_name| {
                world
                    .spawn_at(7.0, 7.0)
                    .with_label("ops-http-marker", pod_core::Team::None)
                    .build();
            })
            .expect("archive-backed local runtime should build")
        {
            AuthorityHostRuntime::Local(runtime) => runtime,
            AuthorityHostRuntime::DirectConnect(_) => panic!("expected local host runtime"),
        };
        runtime.step(Duration::from_secs_f32(1.0 / host.tick_rate as f32));

        let service = ShardSupervisorOpsHandle {
            shards: vec![runtime.ops_handle()],
        }
        .http_service(OpsHttpServiceConfig {
            bind_address: "127.0.0.1:0".to_string(),
            max_request_bytes: 16 * 1024,
            max_response_bytes: 256 * 1024,
            max_event_bytes: 256 * 1024,
            auth_token: Some("http-secret".to_string()),
        });
        let listener = service
            .bind_listener()
            .await
            .expect("ops HTTP service should bind an ephemeral listener");
        let address = listener
            .local_addr()
            .expect("ops HTTP listener should expose a local address")
            .to_string();
        let service_task = tokio::spawn(service.serve_listener(listener));

        let unauthorized_request = format!(
            "GET /ops/archive/supervisor?recent_limit_per_shard=8 HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer wrong-secret\r\n\r\n"
        );
        let mut unauthorized_reader = send_http_request(&address, &unauthorized_request).await;
        let (status, headers) = read_http_response_head(&mut unauthorized_reader).await;
        assert!(status.contains("401 Unauthorized"));
        let body = read_http_response_body(&mut unauthorized_reader, &headers).await;
        let error = serde_json::from_slice::<serde_json::Value>(&body)
            .expect("unauthorized response should decode as JSON");
        assert!(error["error"]
            .as_str()
            .expect("unauthorized response should include an error message")
            .contains("rejected"));

        let archive_request = format!(
            "GET /ops/archive/supervisor?recent_limit_per_shard=8 HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer http-secret\r\n\r\n"
        );
        let mut archive_reader = send_http_request(&address, &archive_request).await;
        let (status, headers) = read_http_response_head(&mut archive_reader).await;
        assert!(status.contains("200 OK"));
        assert_eq!(
            headers.get("content-type").map(String::as_str),
            Some("application/json; charset=utf-8")
        );
        let body = read_http_response_body(&mut archive_reader, &headers).await;
        let snapshot = serde_json::from_slice::<ShardSupervisorOpsArchiveSnapshot>(&body)
            .expect("archive response should decode as a supervisor snapshot");
        assert_eq!(snapshot.shard_count, 1);
        assert_eq!(snapshot.archived_shard_count, 1);
        assert!(snapshot.total_persisted_document_count >= 1);

        let stream_request = format!(
            "GET /ops/stream/supervisor?recent_limit_per_shard=8 HTTP/1.1\r\nHost: {address}\r\nAccept: text/event-stream\r\nAuthorization: Bearer http-secret\r\n\r\n"
        );
        let mut stream_reader = send_http_request(&address, &stream_request).await;
        let (status, headers) = read_http_response_head(&mut stream_reader).await;
        assert!(status.contains("200 OK"));
        assert_eq!(
            headers.get("content-type").map(String::as_str),
            Some("text/event-stream")
        );
        let (event_name, event_data) = read_sse_event(&mut stream_reader).await;
        assert_eq!(event_name, "supervisor_snapshot");
        let initial_snapshot =
            serde_json::from_str::<ShardSupervisorOpsArchiveSnapshot>(&event_data)
                .expect("initial SSE payload should decode as a supervisor snapshot");
        assert_eq!(initial_snapshot.shard_count, 1);
        assert_eq!(initial_snapshot.archived_shard_count, 1);

        runtime.step(Duration::from_secs_f32(1.0 / host.tick_rate as f32));

        let (event_name, event_data) = read_sse_event(&mut stream_reader).await;
        assert_eq!(event_name, "shard_document");
        let shard_document = serde_json::from_str::<HttpShardDocumentEvent>(&event_data)
            .expect("live SSE payload should decode as a shard document event");
        assert_eq!(shard_document.shard_id, "alpha-1");
        assert!(shard_document.retained_document_count >= 1);
        assert!(shard_document.persisted_document_count >= 1);
        assert!(shard_document.archive_path.is_some());
        assert_eq!(
            decode_toon_value(&shard_document.document)
                .expect("SSE document should decode as TOON")["document_type"],
            "versioned_tick_telemetry"
        );
        assert!(shard_document.sequence >= 1);
        assert!(shard_document.next_bookmark.is_some());

        drop(stream_reader);
        service_task.abort();
        let _ = service_task.await;
        fs::remove_dir_all(&archive_root_dir).expect("temp archive root should be removable");
    }

    #[tokio::test]
    async fn supervisor_ops_http_service_replays_from_cursor_over_http_and_sse() {
        let archive_root_dir = std::env::temp_dir().join(format!(
            "pod-host-ops-http-replay-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ));
        let mut host = sample_config(AuthorityTransportMode::Local);
        host.ops_persistence = Some(OpsPersistenceConfig {
            archive_root_dir: archive_root_dir.clone(),
        });

        let mut runtime = match host
            .prepare_runtime_with_shard_id("alpha-1", |world, _map_name| {
                world
                    .spawn_at(8.0, 8.0)
                    .with_label("ops-http-replay-marker", pod_core::Team::None)
                    .build();
            })
            .expect("archive-backed local runtime should build")
        {
            AuthorityHostRuntime::Local(runtime) => runtime,
            AuthorityHostRuntime::DirectConnect(_) => panic!("expected local host runtime"),
        };
        runtime.step(Duration::from_secs_f32(1.0 / host.tick_rate as f32));
        runtime.step(Duration::from_secs_f32(1.0 / host.tick_rate as f32));

        let service = ShardSupervisorOpsHandle {
            shards: vec![runtime.ops_handle()],
        }
        .http_service(OpsHttpServiceConfig {
            bind_address: "127.0.0.1:0".to_string(),
            max_request_bytes: 16 * 1024,
            max_response_bytes: 256 * 1024,
            max_event_bytes: 256 * 1024,
            auth_token: Some("http-secret".to_string()),
        });
        let listener = service
            .bind_listener()
            .await
            .expect("ops HTTP service should bind an ephemeral listener");
        let address = listener
            .local_addr()
            .expect("ops HTTP listener should expose a local address")
            .to_string();
        let service_task = tokio::spawn(service.serve_listener(listener));

        let supervisor_replay_request = format!(
            "GET /ops/replay/supervisor?cursor=alpha-1:1&limit_per_shard=8 HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer http-secret\r\n\r\n"
        );
        let mut supervisor_replay_reader =
            send_http_request(&address, &supervisor_replay_request).await;
        let (status, headers) = read_http_response_head(&mut supervisor_replay_reader).await;
        assert!(status.contains("200 OK"));
        let body = read_http_response_body(&mut supervisor_replay_reader, &headers).await;
        let supervisor_replay = serde_json::from_slice::<ShardSupervisorOpsReplaySnapshot>(&body)
            .expect("supervisor replay response should decode");
        assert_eq!(supervisor_replay.shard_count, 1);
        assert_eq!(supervisor_replay.gap_detected_shard_count, 0);
        assert_eq!(supervisor_replay.has_more_shard_count, 0);
        assert_eq!(supervisor_replay.shards[0].requested_after_sequence, 1);
        assert_eq!(supervisor_replay.shards[0].documents.len(), 1);
        assert_eq!(supervisor_replay.shards[0].documents[0].sequence, 2);
        assert!(!supervisor_replay.next_bookmark.is_empty());
        assert_eq!(
            decode_toon_value(&supervisor_replay.shards[0].documents[0].document)
                .expect("supervisor replay document should decode as TOON")["document_type"],
            "versioned_tick_telemetry"
        );
        assert_eq!(
            supervisor_replay.next_cursor.shards[0].last_sequence,
            supervisor_replay.shards[0].next_cursor.last_sequence
        );
        let resumed_supervisor_cursor =
            decode_supervisor_replay_bookmark(&supervisor_replay.next_bookmark)
                .expect("supervisor replay bookmark should decode");
        assert_eq!(
            resumed_supervisor_cursor.shards[0].last_sequence,
            supervisor_replay.next_cursor.shards[0].last_sequence
        );

        let supervisor_bookmark_request = format!(
            "GET /ops/replay/supervisor?bookmark={}&limit_per_shard=8 HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer http-secret\r\n\r\n",
            supervisor_replay.next_bookmark
        );
        let mut supervisor_bookmark_reader =
            send_http_request(&address, &supervisor_bookmark_request).await;
        let (status, headers) = read_http_response_head(&mut supervisor_bookmark_reader).await;
        assert!(status.contains("200 OK"));
        let body = read_http_response_body(&mut supervisor_bookmark_reader, &headers).await;
        let resumed_supervisor = serde_json::from_slice::<ShardSupervisorOpsReplaySnapshot>(&body)
            .expect("supervisor bookmark replay should decode");
        assert_eq!(resumed_supervisor.shards[0].documents.len(), 0);
        assert_eq!(resumed_supervisor.shards[0].next_cursor.last_sequence, 2);

        let shard_replay_request = format!(
            "GET /ops/replay/shard/alpha-1?after_sequence=1&limit=8 HTTP/1.1\r\nHost: {address}\r\nAuthorization: Bearer http-secret\r\n\r\n"
        );
        let mut shard_replay_reader = send_http_request(&address, &shard_replay_request).await;
        let (status, headers) = read_http_response_head(&mut shard_replay_reader).await;
        assert!(status.contains("200 OK"));
        let body = read_http_response_body(&mut shard_replay_reader, &headers).await;
        let shard_replay = serde_json::from_slice::<AuthorityShardOpsReplaySnapshot>(&body)
            .expect("shard replay response should decode");
        assert_eq!(shard_replay.shard_id, "alpha-1");
        assert_eq!(shard_replay.requested_after_sequence, 1);
        assert!(!shard_replay.gap_detected);
        assert!(!shard_replay.has_more);
        assert_eq!(shard_replay.documents.len(), 1);
        assert_eq!(shard_replay.documents[0].sequence, 2);
        assert!(!shard_replay.next_bookmark.is_empty());
        let resumed_shard_cursor = decode_shard_replay_bookmark(&shard_replay.next_bookmark)
            .expect("shard replay bookmark should decode");
        assert_eq!(resumed_shard_cursor.shard_id, "alpha-1");
        assert_eq!(resumed_shard_cursor.last_sequence, 2);

        let stream_request = format!(
            "GET /ops/stream/shard/alpha-1?bookmark={}&recent_limit=8 HTTP/1.1\r\nHost: {address}\r\nAccept: text/event-stream\r\nAuthorization: Bearer http-secret\r\n\r\n",
            shard_replay.next_bookmark
        );
        let mut stream_reader = send_http_request(&address, &stream_request).await;
        let (status, headers) = read_http_response_head(&mut stream_reader).await;
        assert!(status.contains("200 OK"));
        assert_eq!(
            headers.get("content-type").map(String::as_str),
            Some("text/event-stream")
        );
        let (event_name, event_data) = read_sse_event(&mut stream_reader).await;
        assert_eq!(event_name, "shard_replay");
        let initial_replay = serde_json::from_str::<AuthorityShardOpsReplaySnapshot>(&event_data)
            .expect("initial replay SSE payload should decode");
        assert_eq!(initial_replay.shard_id, "alpha-1");
        assert_eq!(initial_replay.documents.len(), 0);
        assert_eq!(initial_replay.next_cursor.last_sequence, 2);
        assert_eq!(initial_replay.next_bookmark, shard_replay.next_bookmark);

        runtime.step(Duration::from_secs_f32(1.0 / host.tick_rate as f32));

        let (event_name, event_data) = read_sse_event(&mut stream_reader).await;
        assert_eq!(event_name, "shard_document");
        let shard_document = serde_json::from_str::<HttpShardDocumentEvent>(&event_data)
            .expect("live replay SSE payload should decode as a shard document event");
        assert_eq!(shard_document.shard_id, "alpha-1");
        assert_eq!(shard_document.sequence, 3);
        assert!(shard_document.persisted_document_count >= 3);
        assert_eq!(
            decode_toon_value(&shard_document.document)
                .expect("live replay SSE document should decode as TOON")["document_type"],
            "versioned_tick_telemetry"
        );
        assert!(shard_document.next_bookmark.is_some());
        assert_ne!(
            shard_document.next_bookmark.as_deref(),
            Some(shard_replay.next_bookmark.as_str())
        );

        drop(stream_reader);
        service_task.abort();
        let _ = service_task.await;
        fs::remove_dir_all(&archive_root_dir).expect("temp archive root should be removable");
    }

    #[test]
    fn replay_bookmarks_round_trip_for_shard_and_supervisor_cursors() {
        let shard_cursor = super::AuthorityShardOpsReplayCursor {
            shard_id: "alpha-1".to_string(),
            last_sequence: 42,
        };
        let shard_bookmark = encode_shard_replay_bookmark(&shard_cursor);
        assert_eq!(
            decode_shard_replay_bookmark(&shard_bookmark)
                .expect("shard replay bookmark should round-trip"),
            shard_cursor
        );

        let supervisor_cursor = super::ShardSupervisorOpsReplayCursor {
            shards: vec![
                super::AuthorityShardOpsReplayCursor {
                    shard_id: "alpha-1".to_string(),
                    last_sequence: 42,
                },
                super::AuthorityShardOpsReplayCursor {
                    shard_id: "alpha-2".to_string(),
                    last_sequence: 7,
                },
            ],
        };
        let supervisor_bookmark = encode_supervisor_replay_bookmark(&supervisor_cursor);
        assert_eq!(
            decode_supervisor_replay_bookmark(&supervisor_bookmark)
                .expect("supervisor replay bookmark should round-trip"),
            supervisor_cursor
        );
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

    #[derive(Debug, Deserialize)]
    struct HttpShardDocumentEvent {
        shard_id: String,
        sequence: u64,
        document: String,
        retained_document_count: usize,
        persisted_document_count: usize,
        archive_path: Option<std::path::PathBuf>,
        next_bookmark: Option<String>,
    }

    async fn send_http_request(address: &str, request: &str) -> BufReader<TcpStream> {
        let mut stream = TcpStream::connect(address)
            .await
            .expect("HTTP test client should connect");
        stream
            .write_all(request.as_bytes())
            .await
            .expect("HTTP test client should write the request");
        BufReader::new(stream)
    }

    async fn read_http_response_head(
        reader: &mut BufReader<TcpStream>,
    ) -> (String, HashMap<String, String>) {
        let status = super::read_capped_line(reader, 16 * 1024)
            .await
            .expect("HTTP response status line should read")
            .expect("HTTP response should include a status line");
        let mut headers = HashMap::new();
        loop {
            let line = super::read_capped_line(reader, 16 * 1024)
                .await
                .expect("HTTP response header line should read")
                .expect("HTTP response should terminate headers with a blank line");
            if line.is_empty() {
                break;
            }
            let (name, value) = line
                .split_once(':')
                .expect("HTTP response header should contain a ':' separator");
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
        (status, headers)
    }

    async fn read_http_response_body(
        reader: &mut BufReader<TcpStream>,
        headers: &HashMap<String, String>,
    ) -> Vec<u8> {
        let content_length = headers
            .get("content-length")
            .expect("HTTP response should include a content-length header")
            .parse::<usize>()
            .expect("content-length header should parse as usize");
        let mut body = vec![0_u8; content_length];
        reader
            .read_exact(&mut body)
            .await
            .expect("HTTP response body should read exactly");
        body
    }

    async fn read_sse_event(reader: &mut BufReader<TcpStream>) -> (String, String) {
        let mut event_name = String::new();
        let mut data_lines = Vec::new();
        loop {
            let line = super::read_capped_line(reader, 16 * 1024)
                .await
                .expect("SSE event line should read")
                .expect("SSE stream should remain open");
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("event: ") {
                event_name = value.to_string();
            } else if let Some(value) = line.strip_prefix("data: ") {
                data_lines.push(value.to_string());
            }
        }
        (event_name, data_lines.join("\n"))
    }
}
