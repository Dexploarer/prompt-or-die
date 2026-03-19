import { describe, expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import {
  PLATFORM_STABILIZATION_DOC_PATH,
  validatePlatformDocs,
} from "./verify_platform_docs";

const repoRoot = resolve(import.meta.dir, "..");

describe("verify platform docs", () => {
  test("keeps the planning route and benchmark requirement docs aligned", () => {
    const validation = validatePlatformDocs(repoRoot);

    expect(validation.ok).toBe(true);
    expect(validation.missing).toEqual([]);
  });

  test("documents benchmark requirement tiers and shipping boundaries explicitly", () => {
    const markdown = readFileSync(
      resolve(repoRoot, PLATFORM_STABILIZATION_DOC_PATH),
      "utf8",
    );

    expect(markdown).toContain("# Platform Stabilization");
    expect(markdown).toContain("## Planning route");
    expect(markdown).toContain("## Benchmark requirement tiers");
    expect(markdown).toContain("Platform requirement gates");
    expect(markdown).toContain("Local tooling and proof surfaces");
    expect(markdown).toContain("## Public contract surfaces");
    expect(markdown).toContain("## Shipping, authz, and SDK boundaries");
    expect(markdown).toContain("ci-smoke");
    expect(markdown).toContain("shard-target");
    expect(markdown).toContain("OpsHttpAuthorizationPolicySource");
  });
});
