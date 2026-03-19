//! `pod-sdk` — packaged Rust SDK surface for Prompt or Die.
//!
//! This crate keeps the public Rust SDK entrypoint thin by re-exporting the
//! repo-owned `pod-net` facade and a small set of package-level helpers for
//! benchmark and live-smoke flows.

use std::fmt;

pub use pod_core::{
    build_rust_sdk_handoff_fixture, Action, AgentToolCallTrace, ReplayFile, ReplayHeader,
    ReplayTrainingSample, RustSdkHandoffArtifact,
};
pub use pod_net::{
    RustSdkActionExecutionMode, RustSdkActionIntent, RustSdkActionPlan, RustSdkBankState,
    RustSdkBenchmarkCheck, RustSdkBenchmarkReport, RustSdkBenchmarkRun,
    RustSdkBenchmarkScenarioReport, RustSdkDialogState, RustSdkFacade as RustSdkClient,
    RustSdkFacadeConfig as RustSdkClientConfig, RustSdkFacadeError as RustSdkClientError,
    RustSdkSelfStateSnapshot, RustSdkShopState, RustSdkStateSnapshot,
    RustSdkVisibleEntitySnapshot,
};
pub use pod_net::RustSdkAdapterRuntimeMode as RustSdkRuntimeMode;

pub type RustSdkLiveSmokeConfig = pod_net::RustSdkAdapterLiveSmokeConfig;
pub type RustSdkLiveSmokeReport = pod_net::RustSdkAdapterLiveSmokeReport;
pub type RustSdkLiveSmokeRun = pod_net::RustSdkAdapterLiveSmokeRun;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RustSdkError {
    Client(String),
    Benchmark(String),
}

impl fmt::Display for RustSdkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Client(message) => write!(f, "Rust SDK client error: {message}"),
            Self::Benchmark(message) => write!(f, "Rust SDK benchmark error: {message}"),
        }
    }
}

impl std::error::Error for RustSdkError {}

impl From<RustSdkClientError> for RustSdkError {
    fn from(error: RustSdkClientError) -> Self {
        Self::Client(error.to_string())
    }
}

impl From<pod_net::RustSdkRolloutRecorderError> for RustSdkError {
    fn from(error: pod_net::RustSdkRolloutRecorderError) -> Self {
        Self::Benchmark(error.to_string())
    }
}

/// Run the packaged deterministic Rust SDK benchmark suite.
pub fn run_rust_sdk_benchmark_suite() -> Result<RustSdkBenchmarkRun, RustSdkError> {
    pod_net::run_rust_sdk_adapter_benchmark_suite().map_err(Into::into)
}

/// Run the packaged live generated-SDK smoke harness.
pub fn run_rust_sdk_live_smoke(
    config: &RustSdkLiveSmokeConfig,
) -> Result<RustSdkLiveSmokeRun, RustSdkError> {
    RustSdkClient::run_live_smoke(config).map_err(Into::into)
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

        assert!(matches!(error, RustSdkError::Client(_)));
    }
}
