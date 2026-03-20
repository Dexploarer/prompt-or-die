import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import {
  RUST_SDK_BOUNDARY_DOC_PATH,
  validateRustSdkBoundary,
} from "./verify_rust_sdk_boundary";

const repoRoot = resolve(import.meta.dir, "..");

describe("verify rust sdk boundary", () => {
  test("repo-owned Rust SDK boundary contract stays in sync", () => {
    const validation = validateRustSdkBoundary(repoRoot);

    expect(validation.ok).toBe(true);
    expect(validation.missing).toEqual([]);
  });

  test("boundary doc names the adapter lanes and readiness gates", () => {
    const markdown = readFileSync(resolve(repoRoot, RUST_SDK_BOUNDARY_DOC_PATH), "utf8");

    expect(markdown).toContain("## Stable contracts to depend on now");
    expect(markdown).toContain("## Adapter lanes");
    expect(markdown).toContain("rs_state_adapter");
    expect(markdown).toContain("rs_action_adapter");
    expect(markdown).toContain("rs_rollout_recorder");
    expect(markdown).toContain("rs_benchmark_runner");
    expect(markdown).toContain("## Readiness gates");
    expect(markdown).toContain("apply_rust_sdk_handoff_artifact()");
    expect(markdown).toContain("RustSdkAdapterHost");
    expect(markdown).toContain("RustSdkStateSnapshot");
    expect(markdown).toContain("RustSdkActionPlan");
    expect(markdown).toContain("build_rust_sdk_action_plan()");
    expect(markdown).toContain("bind_state_snapshot_action_entity()");
    expect(markdown).toContain("execute_action_plan()");
    expect(markdown).toContain("RustSdkActionExecutorError");
    expect(markdown).toContain("RustSdkAdapterSession");
    expect(markdown).toContain("RustSdkAdapterSessionError");
    expect(markdown).toContain("RustSdkFacade");
    expect(markdown).toContain("RustSdkFacadeConfig");
    expect(markdown).toContain("RustSdkFacadeError");
    expect(markdown).toContain(
      "pod_sdk::{RustSdkClient, RustSdkClientConfig, RustSdkClientError, RustSdkRuntimeMode, RustSdkActionPlan, RustSdkActionPlanError, build_rust_sdk_action_plan, RustSdkRolloutRecord, RustSdkRolloutRecordError, RustSdkBenchmarkCheck, RustSdkBenchmarkScenarioReport, RustSdkBenchmarkReport, RustSdkBenchmarkRun, RustSdkLiveSmokeConfig, RustSdkLiveSmokeReport, RustSdkLiveSmokeRun, run_rust_sdk_benchmark_suite, run_rust_sdk_live_smoke}",
    );
    expect(markdown).toContain("RustSdkAdapterLiveSmokeConfig");
    expect(markdown).toContain("run_rust_sdk_adapter_live_smoke()");
    expect(markdown).toContain("RustSdkRolloutRecorder");
    expect(markdown).toContain("run_rust_sdk_adapter_benchmark_suite()");
    expect(markdown).toContain("compatibility shims");
    expect(markdown).toContain(
      "cargo run -p pod-net --features spacetimedb --example rust_sdk_adapter_live_smoke -- --host http://127.0.0.1:3100 --db-name deadman-prime --fail-on-checks",
    );
    expect(markdown).toContain(
      "cargo run -p pod-sdk --example rust_sdk_live_smoke -- --host http://127.0.0.1:3100 --db-name deadman-prime --fail-on-checks",
    );
    expect(markdown).toContain(
      "cargo run -p pod-sdk --example rust_sdk_benchmark_suite -- --fail-on-checks",
    );
    expect(markdown).toContain("bun ./scripts/verify_rust_sdk_boundary.ts --check");
  });
});
