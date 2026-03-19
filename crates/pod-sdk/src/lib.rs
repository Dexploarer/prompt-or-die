//! `pod-sdk` — packaged Rust SDK surface for Prompt or Die.
//!
//! This crate keeps the public Rust SDK entrypoint thin by wrapping the
//! repo-owned `pod-net` facade in package-native types and exposing canonical
//! package-level helpers for benchmark and live-smoke flows.

use std::fmt;

use pod_net::{RustSdkFacade, RustSdkFacadeConfig, RustSdkFacadeError};
use serde::{Deserialize, Serialize};

pub use pod_core::{
    build_rust_sdk_handoff_fixture, Action, AgentToolCallTrace, ReplayFile, ReplayHeader,
    ReplayTrainingSample, RustSdkHandoffArtifact,
};
pub use pod_net::{
    RustSdkActionExecutionMode, RustSdkActionIntent, RustSdkActionPlan, RustSdkBankState,
    RustSdkBenchmarkCheck, RustSdkBenchmarkReport, RustSdkBenchmarkRun,
    RustSdkBenchmarkScenarioReport, RustSdkDialogState, RustSdkRolloutRecord, ServerMessage,
    RustSdkSelfStateSnapshot, RustSdkShopState, RustSdkStateSnapshot,
    RustSdkVisibleEntitySnapshot,
};
pub use pod_net::RustSdkAdapterRuntimeMode as RustSdkRuntimeMode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustSdkClientConfig {
    pub host: String,
    pub db_name: String,
    pub auth_token: Option<String>,
    pub player_name: String,
    pub runtime_mode: RustSdkRuntimeMode,
}

impl Default for RustSdkClientConfig {
    fn default() -> Self {
        Self {
            host: "http://localhost:3000".into(),
            db_name: "prompt-or-die".into(),
            auth_token: None,
            player_name: "Player".into(),
            runtime_mode: RustSdkRuntimeMode::Emulated,
        }
    }
}

impl From<RustSdkClientConfig> for RustSdkFacadeConfig {
    fn from(config: RustSdkClientConfig) -> Self {
        Self {
            client: pod_net::SpacetimeDBClientConfig {
                host: config.host,
                db_name: config.db_name,
                auth_token: config.auth_token,
                player_name: config.player_name,
                ..Default::default()
            },
            runtime_mode: config.runtime_mode,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustSdkLiveSmokeConfig {
    pub host: String,
    pub db_name: String,
    pub auth_token: Option<String>,
    pub timeout_ms: u64,
    pub poll_interval_ms: u64,
}

impl Default for RustSdkLiveSmokeConfig {
    fn default() -> Self {
        Self {
            host: "http://localhost:3000".into(),
            db_name: "prompt-or-die".into(),
            auth_token: None,
            timeout_ms: 5_000,
            poll_interval_ms: 10,
        }
    }
}

impl RustSdkLiveSmokeConfig {
    fn to_adapter_config(&self) -> pod_net::RustSdkAdapterLiveSmokeConfig {
        pod_net::RustSdkAdapterLiveSmokeConfig {
            host: self.host.clone(),
            db_name: self.db_name.clone(),
            auth_token: self.auth_token.clone(),
            timeout_ms: self.timeout_ms,
            poll_interval_ms: self.poll_interval_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RustSdkLiveSmokeReport {
    pub schema_version: u32,
    pub runtime_mode: RustSdkRuntimeMode,
    pub host: String,
    pub db_name: String,
    pub spawned_entity_id: u64,
    pub connected_agent_entity_id: Option<u64>,
    pub connected_agent_display_name: Option<String>,
    pub action_submission_entity_id: Option<u64>,
    pub action_submission_kind: Option<String>,
    pub observed_action_key: String,
    pub reducer_submission_count: u64,
    pub replay_training_sample_count: usize,
    pub checks: Vec<RustSdkBenchmarkCheck>,
}

impl RustSdkLiveSmokeReport {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }
}

impl From<pod_net::RustSdkAdapterLiveSmokeReport> for RustSdkLiveSmokeReport {
    fn from(report: pod_net::RustSdkAdapterLiveSmokeReport) -> Self {
        Self {
            schema_version: report.schema_version,
            runtime_mode: report.runtime_mode,
            host: report.host,
            db_name: report.db_name,
            spawned_entity_id: report.spawned_entity_id,
            connected_agent_entity_id: report.connected_agent_entity_id,
            connected_agent_display_name: report.connected_agent_display_name,
            action_submission_entity_id: report.action_submission_entity_id,
            action_submission_kind: report.action_submission_kind,
            observed_action_key: report.observed_action_key,
            reducer_submission_count: report.reducer_submission_count,
            replay_training_sample_count: report.replay_training_sample_count,
            checks: report.checks,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RustSdkLiveSmokeRun {
    pub report: RustSdkLiveSmokeReport,
    pub replay: ReplayFile,
    pub training_samples: Vec<ReplayTrainingSample>,
}

impl From<pod_net::RustSdkAdapterLiveSmokeRun> for RustSdkLiveSmokeRun {
    fn from(run: pod_net::RustSdkAdapterLiveSmokeRun) -> Self {
        Self {
            report: run.report.into(),
            replay: run.replay,
            training_samples: run.training_samples,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustSdkClientError {
    Client(String),
    Benchmark(String),
}

impl fmt::Display for RustSdkClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(message) => write!(f, "Rust SDK client error: {message}"),
            Self::Benchmark(message) => write!(f, "Rust SDK benchmark error: {message}"),
        }
    }
}

impl std::error::Error for RustSdkClientError {}

impl From<RustSdkFacadeError> for RustSdkClientError {
    fn from(error: RustSdkFacadeError) -> Self {
        Self::Client(error.to_string())
    }
}

impl From<pod_net::RustSdkRolloutRecorderError> for RustSdkClientError {
    fn from(error: pod_net::RustSdkRolloutRecorderError) -> Self {
        Self::Benchmark(error.to_string())
    }
}

pub struct RustSdkClient {
    inner: RustSdkFacade,
}

impl RustSdkClient {
    pub fn new(config: RustSdkClientConfig) -> Self {
        Self {
            inner: RustSdkFacade::new(config.into()),
        }
    }

    pub fn runtime_mode(&self) -> RustSdkRuntimeMode {
        self.inner.runtime_mode()
    }

    pub fn connect(&mut self) -> Result<(), RustSdkClientError> {
        self.inner.connect().map_err(Into::into)
    }

    pub fn poll_updates(&mut self) -> Vec<ServerMessage> {
        self.inner.poll_updates()
    }

    pub fn apply_handoff_artifact(
        &mut self,
        artifact: RustSdkHandoffArtifact,
    ) -> Result<(), RustSdkClientError> {
        self.inner.apply_handoff_artifact(artifact).map_err(Into::into)
    }

    pub fn apply_handoff_fixture(&mut self) -> Result<(), RustSdkClientError> {
        self.inner.apply_handoff_fixture().map_err(Into::into)
    }

    pub fn apply_handoff_json_document(
        &mut self,
        document: impl AsRef<str>,
    ) -> Result<(), RustSdkClientError> {
        self.inner
            .apply_handoff_json_document(document)
            .map_err(Into::into)
    }

    pub fn apply_handoff_toon_document(
        &mut self,
        document: impl AsRef<str>,
    ) -> Result<(), RustSdkClientError> {
        self.inner
            .apply_handoff_toon_document(document)
            .map_err(Into::into)
    }

    pub fn ingest_state_snapshot(
        &mut self,
        snapshot: &RustSdkStateSnapshot,
    ) -> Result<(), RustSdkClientError> {
        self.inner.ingest_state_snapshot(snapshot).map_err(Into::into)
    }

    pub fn execute_actions(
        &mut self,
        snapshot: RustSdkStateSnapshot,
        prompt_sent: impl Into<String>,
        actions: Vec<Action>,
        tool_calls: Vec<AgentToolCallTrace>,
        latency_ms: u32,
    ) -> Result<Vec<Action>, RustSdkClientError> {
        self.inner
            .execute_actions(snapshot, prompt_sent, actions, tool_calls, latency_ms)
            .map_err(Into::into)
    }

    pub fn record_rollout_step(
        &mut self,
        record: RustSdkRolloutRecord,
    ) -> Result<(), RustSdkClientError> {
        self.inner.record_rollout_step(record).map_err(Into::into)
    }

    pub fn finalize_replay(self, header: ReplayHeader) -> ReplayFile {
        self.inner.finalize_replay(header)
    }

    pub fn run_live_smoke(
        config: &RustSdkLiveSmokeConfig,
    ) -> Result<RustSdkLiveSmokeRun, RustSdkClientError> {
        run_rust_sdk_live_smoke(config)
    }
}

/// Run the packaged deterministic Rust SDK benchmark suite.
pub fn run_rust_sdk_benchmark_suite() -> Result<RustSdkBenchmarkRun, RustSdkClientError> {
    pod_net::run_rust_sdk_adapter_benchmark_suite().map_err(Into::into)
}

/// Run the packaged live generated-SDK smoke harness.
pub fn run_rust_sdk_live_smoke(
    config: &RustSdkLiveSmokeConfig,
) -> Result<RustSdkLiveSmokeRun, RustSdkClientError> {
    RustSdkFacade::run_live_smoke(&config.to_adapter_config())
        .map(Into::into)
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packaged_rust_sdk_client_defaults_to_emulated_runtime() {
        let client = RustSdkClient::new(RustSdkClientConfig::default());
        assert_eq!(client.runtime_mode(), RustSdkRuntimeMode::Emulated);
    }

    #[test]
    fn packaged_rust_sdk_benchmark_suite_reports_passing_checks() {
        let run =
            run_rust_sdk_benchmark_suite().expect("packaged Rust SDK benchmark should run");
        assert!(run.report.passed());
        assert!(!run.training_samples.is_empty());
    }

    #[test]
    fn packaged_rust_sdk_live_smoke_maps_client_errors() {
        let error = run_rust_sdk_live_smoke(&RustSdkLiveSmokeConfig {
            host: "http://127.0.0.1:1".into(),
            db_name: "missing-world".into(),
            timeout_ms: 25,
            poll_interval_ms: 5,
            ..Default::default()
        })
        .expect_err("closed localhost port should fail packaged live smoke");

        assert!(matches!(error, RustSdkClientError::Client(_)));
    }

    #[test]
    fn packaged_rust_sdk_live_smoke_report_tracks_passing_checks() {
        let report = RustSdkLiveSmokeReport {
            schema_version: 1,
            runtime_mode: RustSdkRuntimeMode::Emulated,
            host: "http://localhost:3000".into(),
            db_name: "prompt-or-die".into(),
            spawned_entity_id: 7,
            connected_agent_entity_id: Some(7),
            connected_agent_display_name: Some("sdk".into()),
            action_submission_entity_id: Some(7),
            action_submission_kind: Some("Idle".into()),
            observed_action_key: "idle".into(),
            reducer_submission_count: 2,
            replay_training_sample_count: 1,
            checks: vec![RustSdkBenchmarkCheck {
                metric: "sample".into(),
                passed: true,
                expected: "1".into(),
                observed: "1".into(),
            }],
        };

        assert!(report.passed());
    }
}
