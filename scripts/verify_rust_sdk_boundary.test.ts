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
    expect(markdown).toContain("bun ./scripts/verify_rust_sdk_boundary.ts --check");
  });
});
