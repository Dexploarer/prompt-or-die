import { describe, expect, test } from "bun:test";

import {
  GENERATED_ASSET_PATHS,
  assertCleanGeneratedAssetOutputs,
  formatGeneratedAssetDriftError,
  parseGitStatusPorcelainPaths,
} from "./verify-generated-assets";

describe("verify generated assets", () => {
  test("parses modified, untracked, and renamed generated paths from porcelain output", () => {
    expect(
      parseGitStatusPorcelainPaths(
        [
          " M apps/pod-web/public/assets/pod-asset-manifest.json",
          "?? apps/pod-web/artifacts/staged-assets/pod-runtime-budget-report.json",
          "R  apps/pod-web/public/assets/textures/old.ktx2 -> apps/pod-web/public/assets/textures/new.ktx2",
        ].join("\n"),
      ),
    ).toEqual([
      "apps/pod-web/public/assets/pod-asset-manifest.json",
      "apps/pod-web/artifacts/staged-assets/pod-runtime-budget-report.json",
      "apps/pod-web/public/assets/textures/old.ktx2",
      "apps/pod-web/public/assets/textures/new.ktx2",
    ]);
  });

  test("formats a drift error with the standard remediation message", () => {
    expect(
      formatGeneratedAssetDriftError([
        "apps/pod-web/public/assets/pod-asset-manifest.json",
        "apps/pod-web/artifacts/staged-assets/pod-runtime-budget-report.json",
      ]),
    ).toContain("Re-run `cd apps/pod-web && bun run sync:assets`");
  });

  test("accepts clean generated outputs and rejects drift", () => {
    expect(() => assertCleanGeneratedAssetOutputs("")).not.toThrow();
    expect(() =>
      assertCleanGeneratedAssetOutputs(
        " M apps/pod-web/public/assets/pod-asset-manifest.json\n",
      ),
    ).toThrow("Generated pod-web asset outputs are out of date after sync:assets.");
  });

  test("tracks the generated asset roots under source, staged, and runtime outputs", () => {
    expect(GENERATED_ASSET_PATHS).toEqual([
      "apps/pod-web/artifacts/source-assets",
      "apps/pod-web/artifacts/staged-assets",
      "apps/pod-web/public/assets",
    ]);
  });
});
