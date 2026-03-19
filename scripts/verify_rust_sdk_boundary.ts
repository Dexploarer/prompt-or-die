#!/usr/bin/env bun

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export const RUST_SDK_BOUNDARY_DOC_PATH = "docs/rust-sdk-boundary.md";

type Mode = "check" | "json";

type Options = {
  mode: Mode;
};

export type RustSdkBoundaryExpectation = {
  path: string;
  requiredSnippets: string[];
};

export type RustSdkBoundaryValidation = {
  ok: boolean;
  expectations: RustSdkBoundaryExpectation[];
  missing: Array<{
    path: string;
    missingSnippets: string[];
  }>;
};

const EXPECTATIONS: RustSdkBoundaryExpectation[] = [
  {
    path: "README.md",
    requiredSnippets: ["Rust SDK Boundary", "docs/rust-sdk-boundary.md"],
  },
  {
    path: "docs/README.md",
    requiredSnippets: ["Rust SDK Boundary", "./rust-sdk-boundary.md"],
  },
  {
    path: "docs/platform-stabilization.md",
    requiredSnippets: ["docs/rust-sdk-boundary.md", "crates/pod-sdk/src/lib.rs"],
  },
  {
    path: RUST_SDK_BOUNDARY_DOC_PATH,
    requiredSnippets: [
      "# POD Rust SDK Boundary",
      "## Stable contracts to depend on now",
      "RustSdkHandoffArtifact",
      "VersionedObservation",
      "VersionedAgentAction",
      "VersionedTickTelemetry",
      "RemoteTopologyBundle",
      "install_generated_binding_runtime()",
      "install_generated_sdk_runtime()",
      "apply_rust_sdk_handoff_artifact()",
      "RustSdkAdapterHost",
      "RustSdkAdapterRuntimeMode",
      "RustSdkStateSnapshot",
      "RustSdkActionPlan",
      "build_rust_sdk_action_plan()",
      "bind_state_snapshot_action_entity()",
      "execute_action_plan()",
      "RustSdkActionExecutorError",
      "RustSdkAdapterSession",
      "RustSdkAdapterSessionError",
      "RustSdkFacade",
      "RustSdkFacadeConfig",
      "RustSdkFacadeError",
      "pod_sdk::{RustSdkClient, RustSdkClientConfig, RustSdkRuntimeMode, run_rust_sdk_benchmark_suite, run_rust_sdk_live_smoke}",
      "RustSdkAdapterLiveSmokeConfig",
      "run_rust_sdk_adapter_live_smoke()",
      "RustSdkRolloutRecorder",
      "run_rust_sdk_adapter_benchmark_suite()",
      "cargo run -p pod-net --features spacetimedb --example rust_sdk_adapter_live_smoke -- --host http://127.0.0.1:3100 --db-name deadman-prime --fail-on-checks",
      "cargo run -p pod-net --features spacetimedb --example rust_sdk_adapter_benchmark_suite -- --fail-on-checks",
      "cargo run -p pod-sdk --example rust_sdk_live_smoke -- --host http://127.0.0.1:3100 --db-name deadman-prime --fail-on-checks",
      "cargo run -p pod-sdk --example rust_sdk_benchmark_suite -- --fail-on-checks",
      "## Adapter lanes",
      "rs_state_adapter",
      "rs_action_adapter",
      "rs_rollout_recorder",
      "rs_benchmark_runner",
      "## Readiness gates",
    ],
  },
  {
    path: "crates/pod-core/src/lib.rs",
    requiredSnippets: [
      "build_rust_sdk_handoff_fixture",
      "RustSdkHandoffArtifact",
      "VersionedAgentAction",
      "VersionedObservation",
      "VersionedTickTelemetry",
      "RemoteTopologyBundle",
      "ReplayTrainingSample",
    ],
  },
  {
    path: "Cargo.toml",
    requiredSnippets: ['"crates/pod-sdk"'],
  },
  {
    path: "crates/pod-net/Cargo.toml",
    requiredSnippets: [
      'name = "rust_sdk_adapter_benchmark_suite"',
      'name = "rust_sdk_adapter_live_smoke"',
      'required-features = ["spacetimedb"]',
    ],
  },
  {
    path: "crates/pod-sdk/Cargo.toml",
    requiredSnippets: [
      'name = "pod-sdk"',
      'name = "rust_sdk_benchmark_suite"',
      'name = "rust_sdk_live_smoke"',
    ],
  },
  {
    path: "crates/pod-core/examples/rust_sdk_handoff_fixture.rs",
    requiredSnippets: [
      "build_rust_sdk_handoff_fixture",
      "Usage: cargo run -p pod-core --example rust_sdk_handoff_fixture -- [--format json|toon] [--output PATH]",
    ],
  },
  {
    path: "crates/pod-core/src/app.rs",
    requiredSnippets: [
      'register_contract::<VersionedObservation>("VersionedObservation")',
      'register_contract::<VersionedAgentAction>("VersionedAgentAction")',
      'register_contract::<VersionedTickTelemetry>("VersionedTickTelemetry")',
      'register_contract::<RustSdkHandoffArtifact>("RustSdkHandoffArtifact")',
    ],
  },
  {
    path: "crates/pod-stdb/src/client.rs",
    requiredSnippets: [
      "pub fn install_generated_binding_runtime(&mut self) -> GeneratedBindingEndpoint",
      "pub fn install_generated_sdk_runtime(&mut self)",
      "pub fn apply_rust_sdk_handoff_artifact(",
    ],
  },
  {
    path: "crates/pod-sdk/src/lib.rs",
    requiredSnippets: [
      "pub use pod_net::RustSdkAdapterRuntimeMode as RustSdkRuntimeMode;",
      "RustSdkFacade as RustSdkClient",
      "RustSdkFacadeConfig as RustSdkClientConfig",
      "RustSdkFacadeError as RustSdkClientError",
      "pub type RustSdkLiveSmokeConfig = pod_net::RustSdkAdapterLiveSmokeConfig;",
      "pub fn run_rust_sdk_benchmark_suite()",
      "pub fn run_rust_sdk_live_smoke(",
      "pub enum RustSdkError",
    ],
  },
  {
    path: "crates/pod-net/src/client_stdb.rs",
    requiredSnippets: [
      "pub enum RustSdkAdapterRuntimeMode",
      "pub struct RustSdkAdapterHost",
      "pub struct RustSdkStateSnapshot",
      "pub struct RustSdkActionPlan",
      "pub fn build_rust_sdk_action_plan(",
      "pub enum RustSdkActionExecutorError",
      "pub struct RustSdkAdapterSession",
      "pub enum RustSdkAdapterSessionError",
      "pub struct RustSdkFacadeConfig",
      "pub struct RustSdkFacade",
      "pub enum RustSdkFacadeError",
      "pub struct RustSdkAdapterLiveSmokeConfig",
      "pub struct RustSdkAdapterLiveSmokeReport",
      "pub struct RustSdkAdapterLiveSmokeRun",
      "pub struct RustSdkRolloutRecord",
      "pub struct RustSdkRolloutRecorder",
      "pub fn run_rust_sdk_adapter_benchmark_suite(",
      "pub fn run_rust_sdk_adapter_live_smoke(",
      "pub fn apply_state_snapshot(",
      "pub fn bind_state_snapshot_action_entity(",
      "pub fn execute_action_plan(",
      "pub fn apply_handoff_json_document(",
      "pub fn apply_handoff_toon_document(",
      "pub fn install_generated_binding_runtime(&mut self) -> GeneratedBindingEndpoint",
      "pub fn install_generated_sdk_runtime(&mut self)",
      "pub fn apply_rust_sdk_handoff_artifact(",
    ],
  },
  {
    path: "crates/pod-sdk/examples/rust_sdk_live_smoke.rs",
    requiredSnippets: [
      "run_rust_sdk_live_smoke",
      "Usage: cargo run -p pod-sdk --example rust_sdk_live_smoke -- [--host URL] [--db-name NAME] [--auth-token TOKEN] [--timeout-ms MS] [--output PATH] [--replay-output PATH] [--training-output PATH] [--fail-on-checks]",
    ],
  },
  {
    path: "crates/pod-sdk/examples/rust_sdk_benchmark_suite.rs",
    requiredSnippets: [
      "run_rust_sdk_benchmark_suite",
      "Usage: cargo run -p pod-sdk --example rust_sdk_benchmark_suite -- [--output PATH] [--replay-output PATH] [--training-output PATH] [--fail-on-checks]",
    ],
  },
  {
    path: "crates/pod-net/examples/rust_sdk_adapter_live_smoke.rs",
    requiredSnippets: [
      "run_rust_sdk_adapter_live_smoke",
      "Usage: cargo run -p pod-net --features spacetimedb --example rust_sdk_adapter_live_smoke -- [--host URL] [--db-name NAME] [--auth-token TOKEN] [--timeout-ms MS] [--output PATH] [--replay-output PATH] [--training-output PATH] [--fail-on-checks]",
    ],
  },
  {
    path: "crates/pod-net/examples/rust_sdk_adapter_benchmark_suite.rs",
    requiredSnippets: [
      "run_rust_sdk_adapter_benchmark_suite",
      "Usage: cargo run -p pod-net --features spacetimedb --example rust_sdk_adapter_benchmark_suite -- [--output PATH] [--replay-output PATH] [--training-output PATH] [--fail-on-checks]",
    ],
  },
  {
    path: "crates/pod-net/src/lib.rs",
    requiredSnippets: [
      "RustSdkAdapterHost",
      "RustSdkAdapterRuntimeMode",
      "RustSdkStateSnapshot",
      "RustSdkActionPlan",
      "build_rust_sdk_action_plan",
      "RustSdkActionExecutorError",
      "RustSdkAdapterSession",
      "RustSdkAdapterSessionError",
      "RustSdkFacade",
      "RustSdkFacadeConfig",
      "RustSdkFacadeError",
      "RustSdkAdapterLiveSmokeConfig",
      "RustSdkAdapterLiveSmokeReport",
      "RustSdkAdapterLiveSmokeRun",
      "RustSdkRolloutRecorder",
      "run_rust_sdk_adapter_benchmark_suite",
      "run_rust_sdk_adapter_live_smoke",
    ],
  },
  {
    path: "scripts/pod_sdk.ts",
    requiredSnippets: [
      'export const POD_EXPORT_TARGETS = ["world", "events", "multiverse"] as const;',
    ],
  },
  {
    path: "scripts/cli_surface.ts",
    requiredSnippets: ['id: "verify-rust-sdk-boundary"'],
  },
];

function printHelp() {
  console.error(
    "Usage: bun ./scripts/verify_rust_sdk_boundary.ts [--check|--json]\n\nDefaults to --check.\n  --check  Verify the repo-owned Rust SDK boundary doc and stable seam coverage\n  --json   Print the machine-readable validation report",
  );
}

function parseArgs(argv: string[]): Options {
  let mode: Mode = "check";
  for (const argument of argv) {
    switch (argument) {
      case "--check":
        mode = "check";
        break;
      case "--json":
        mode = "json";
        break;
      case "--help":
      case "-h":
        printHelp();
        process.exit(0);
      default:
        throw new Error(`unknown argument: ${argument}`);
    }
  }

  return { mode };
}

function readText(repoRoot: string, path: string): string {
  return readFileSync(resolve(repoRoot, path), "utf8");
}

export function validateRustSdkBoundary(
  repoRoot: string,
): RustSdkBoundaryValidation {
  const missing = EXPECTATIONS.map((expectation) => {
    const text = readText(repoRoot, expectation.path);
    const missingSnippets = expectation.requiredSnippets.filter(
      (snippet) => !text.includes(snippet),
    );
    return {
      path: expectation.path,
      missingSnippets,
    };
  }).filter((entry) => entry.missingSnippets.length > 0);

  return {
    ok: missing.length === 0,
    expectations: EXPECTATIONS,
    missing,
  };
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const repoRoot = resolve(import.meta.dir, "..");
  const validation = validateRustSdkBoundary(repoRoot);

  if (options.mode === "json") {
    console.log(JSON.stringify(validation, null, 2));
    return;
  }

  if (!validation.ok) {
    for (const entry of validation.missing) {
      console.error(`Missing required Rust SDK boundary snippets in ${entry.path}:`);
      for (const snippet of entry.missingSnippets) {
        console.error(`- ${snippet}`);
      }
    }
    process.exitCode = 1;
    return;
  }

  console.error("Rust SDK boundary verification passed.");
}

if (import.meta.main) {
  main();
}
