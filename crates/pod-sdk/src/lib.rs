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
    RustSdkActionExecutionMode, RustSdkActionIntent, RustSdkBankState, RustSdkDialogState,
    ServerMessage, RustSdkSelfStateSnapshot, RustSdkShopState, RustSdkStateSnapshot,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustSdkActionPlan {
    pub execution_mode: RustSdkActionExecutionMode,
    pub intent: RustSdkActionIntent,
}

impl From<pod_net::RustSdkActionPlan> for RustSdkActionPlan {
    fn from(plan: pod_net::RustSdkActionPlan) -> Self {
        Self {
            execution_mode: plan.execution_mode,
            intent: plan.intent,
        }
    }
}

impl From<RustSdkActionPlan> for pod_net::RustSdkActionPlan {
    fn from(plan: RustSdkActionPlan) -> Self {
        Self {
            execution_mode: plan.execution_mode,
            intent: plan.intent,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustSdkActionPlanError {
    UnsupportedAction {
        action: &'static str,
        reason: String,
    },
}

impl fmt::Display for RustSdkActionPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedAction { action, reason } => {
                write!(f, "unsupported Rust SDK action {action}: {reason}")
            }
        }
    }
}

impl std::error::Error for RustSdkActionPlanError {}

impl From<pod_net::RustSdkActionAdapterError> for RustSdkActionPlanError {
    fn from(error: pod_net::RustSdkActionAdapterError) -> Self {
        match error {
            pod_net::RustSdkActionAdapterError::UnsupportedAction { action, reason } => {
                Self::UnsupportedAction { action, reason }
            }
        }
    }
}

pub fn build_rust_sdk_action_plan(
    action: &Action,
) -> Result<RustSdkActionPlan, RustSdkActionPlanError> {
    pod_net::build_rust_sdk_action_plan(action)
        .map(Into::into)
        .map_err(Into::into)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustSdkRolloutRecordError {
    UnsupportedAction(RustSdkActionPlanError),
    SerializeActionPlans(String),
}

impl fmt::Display for RustSdkRolloutRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedAction(error) => error.fmt(f),
            Self::SerializeActionPlans(message) => {
                write!(f, "failed to serialize Rust SDK action plans: {message}")
            }
        }
    }
}

impl std::error::Error for RustSdkRolloutRecordError {}

impl From<RustSdkActionPlanError> for RustSdkRolloutRecordError {
    fn from(error: RustSdkActionPlanError) -> Self {
        Self::UnsupportedAction(error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustSdkRolloutRecord {
    pub snapshot: RustSdkStateSnapshot,
    pub prompt_sent: String,
    pub raw_response: String,
    pub action_plans: Vec<RustSdkActionPlan>,
    pub actions_taken: Vec<Action>,
    pub tool_calls: Vec<AgentToolCallTrace>,
    pub latency_ms: u32,
}

impl RustSdkRolloutRecord {
    pub fn from_actions(
        snapshot: RustSdkStateSnapshot,
        prompt_sent: impl Into<String>,
        actions_taken: Vec<Action>,
        tool_calls: Vec<AgentToolCallTrace>,
        latency_ms: u32,
    ) -> Result<Self, RustSdkRolloutRecordError> {
        let action_plans = actions_taken
            .iter()
            .map(build_rust_sdk_action_plan)
            .collect::<Result<Vec<_>, _>>()?;
        let raw_response = serde_json::to_string(&action_plans)
            .map_err(|error| RustSdkRolloutRecordError::SerializeActionPlans(error.to_string()))?;

        Ok(Self {
            snapshot,
            prompt_sent: prompt_sent.into(),
            raw_response,
            action_plans,
            actions_taken,
            tool_calls,
            latency_ms,
        })
    }
}

impl From<pod_net::RustSdkRolloutRecord> for RustSdkRolloutRecord {
    fn from(record: pod_net::RustSdkRolloutRecord) -> Self {
        Self {
            snapshot: record.snapshot,
            prompt_sent: record.prompt_sent,
            raw_response: record.raw_response,
            action_plans: record.action_plans.into_iter().map(Into::into).collect(),
            actions_taken: record.actions_taken,
            tool_calls: record.tool_calls,
            latency_ms: record.latency_ms,
        }
    }
}

impl From<RustSdkRolloutRecord> for pod_net::RustSdkRolloutRecord {
    fn from(record: RustSdkRolloutRecord) -> Self {
        Self {
            snapshot: record.snapshot,
            prompt_sent: record.prompt_sent,
            raw_response: record.raw_response,
            action_plans: record.action_plans.into_iter().map(Into::into).collect(),
            actions_taken: record.actions_taken,
            tool_calls: record.tool_calls,
            latency_ms: record.latency_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RustSdkBenchmarkCheck {
    pub metric: String,
    pub passed: bool,
    pub expected: String,
    pub observed: String,
}

impl From<pod_net::RustSdkBenchmarkCheck> for RustSdkBenchmarkCheck {
    fn from(check: pod_net::RustSdkBenchmarkCheck) -> Self {
        Self {
            metric: check.metric,
            passed: check.passed,
            expected: check.expected,
            observed: check.observed,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustSdkBenchmarkScenarioReport {
    pub scenario_id: String,
    pub description: String,
    pub runtime_mode: RustSdkRuntimeMode,
    pub expected_action_key: String,
    pub observed_action_key: String,
    pub expected_execution_mode: RustSdkActionExecutionMode,
    pub observed_execution_mode: RustSdkActionExecutionMode,
    pub action_matches: bool,
    pub execution_mode_matches: bool,
    pub available_action_matches: bool,
    pub reducer_submission_count: u64,
    pub reducer_submission_matches: bool,
    pub tool_call_error_count: usize,
    pub training_sample_count: usize,
}

impl From<pod_net::RustSdkBenchmarkScenarioReport> for RustSdkBenchmarkScenarioReport {
    fn from(report: pod_net::RustSdkBenchmarkScenarioReport) -> Self {
        Self {
            scenario_id: report.scenario_id,
            description: report.description,
            runtime_mode: report.runtime_mode,
            expected_action_key: report.expected_action_key,
            observed_action_key: report.observed_action_key,
            expected_execution_mode: report.expected_execution_mode,
            observed_execution_mode: report.observed_execution_mode,
            action_matches: report.action_matches,
            execution_mode_matches: report.execution_mode_matches,
            available_action_matches: report.available_action_matches,
            reducer_submission_count: report.reducer_submission_count,
            reducer_submission_matches: report.reducer_submission_matches,
            tool_call_error_count: report.tool_call_error_count,
            training_sample_count: report.training_sample_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RustSdkBenchmarkReport {
    pub schema_version: u32,
    pub generated_at_unix_ms: u128,
    pub benchmark_id: String,
    pub replay_tick_count: u64,
    pub replay_training_sample_count: usize,
    pub scenarios: Vec<RustSdkBenchmarkScenarioReport>,
    pub checks: Vec<RustSdkBenchmarkCheck>,
}

impl RustSdkBenchmarkReport {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|check| check.passed)
    }
}

impl From<pod_net::RustSdkBenchmarkReport> for RustSdkBenchmarkReport {
    fn from(report: pod_net::RustSdkBenchmarkReport) -> Self {
        Self {
            schema_version: report.schema_version,
            generated_at_unix_ms: report.generated_at_unix_ms,
            benchmark_id: report.benchmark_id,
            replay_tick_count: report.replay_tick_count,
            replay_training_sample_count: report.replay_training_sample_count,
            scenarios: report.scenarios.into_iter().map(Into::into).collect(),
            checks: report.checks.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RustSdkBenchmarkRun {
    pub report: RustSdkBenchmarkReport,
    pub replay: ReplayFile,
    pub training_samples: Vec<ReplayTrainingSample>,
}

impl From<pod_net::RustSdkBenchmarkRun> for RustSdkBenchmarkRun {
    fn from(run: pod_net::RustSdkBenchmarkRun) -> Self {
        Self {
            report: run.report.into(),
            replay: run.replay,
            training_samples: run.training_samples,
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
            checks: report.checks.into_iter().map(Into::into).collect(),
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
        self.inner
            .record_rollout_step(record.into())
            .map_err(Into::into)
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
    pod_net::run_rust_sdk_adapter_benchmark_suite()
        .map(Into::into)
        .map_err(Into::into)
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

    #[test]
    fn packaged_rust_sdk_benchmark_report_tracks_passing_checks() {
        let report = RustSdkBenchmarkReport {
            schema_version: 1,
            generated_at_unix_ms: 123,
            benchmark_id: "bench".into(),
            replay_tick_count: 4,
            replay_training_sample_count: 1,
            scenarios: vec![RustSdkBenchmarkScenarioReport {
                scenario_id: "sample".into(),
                description: "sample".into(),
                runtime_mode: RustSdkRuntimeMode::Emulated,
                expected_action_key: "idle".into(),
                observed_action_key: "idle".into(),
                expected_execution_mode: RustSdkActionExecutionMode::Immediate,
                observed_execution_mode: RustSdkActionExecutionMode::Immediate,
                action_matches: true,
                execution_mode_matches: true,
                available_action_matches: true,
                reducer_submission_count: 1,
                reducer_submission_matches: true,
                tool_call_error_count: 0,
                training_sample_count: 1,
            }],
            checks: vec![RustSdkBenchmarkCheck {
                metric: "sample".into(),
                passed: true,
                expected: "1".into(),
                observed: "1".into(),
            }],
        };

        assert!(report.passed());
    }

    #[test]
    fn packaged_rust_sdk_action_plan_builder_wraps_repo_owned_plan() {
        let plan = build_rust_sdk_action_plan(&Action::AttackTarget {
            target: pod_core::EntityId(7),
        })
        .expect("attack target should lower");

        assert_eq!(plan.execution_mode, RustSdkActionExecutionMode::Immediate);
        assert!(
            matches!(
                plan.intent,
                RustSdkActionIntent::AttackEntity { entity_id: 7 }
            ),
            "attack target should lower into an entity attack intent"
        );
    }
}
