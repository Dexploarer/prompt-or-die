import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { describe, expect, test } from "bun:test";

import {
  buildSnapshotComparisonReport,
  parseArgs,
} from "./compare_moat_snapshots";
import type { PublishedMoatSnapshot } from "./publish_moat_snapshots";

const repoRoot = resolve(import.meta.dir, "..");
const baselineSnapshotPath = resolve(
  repoRoot,
  "docs/benchmark-snapshots/2026-03-shard-target.json",
);

function loadBaselineSnapshot(): PublishedMoatSnapshot {
  return JSON.parse(readFileSync(baselineSnapshotPath, "utf8")) as PublishedMoatSnapshot;
}

describe("compare moat snapshots", () => {
  test("parseArgs requires baseline and candidate inputs", () => {
    expect(() => parseArgs([])).toThrow("missing required --baseline");
    expect(() => parseArgs(["--baseline", "baseline.json"])).toThrow(
      "missing required --candidate",
    );

    expect(
      parseArgs([
        "--baseline",
        "baseline.json",
        "--candidate",
        "candidate.json",
        "--fail-on-regressions",
      ]),
    ).toEqual({
      baseline: "baseline.json",
      candidate: "candidate.json",
      output: "artifacts/benchmark-snapshot-comparison.json",
      failOnRegressions: true,
    });
  });

  test("self-comparison of the committed shard-target snapshot is stable", () => {
    const baseline = loadBaselineSnapshot();
    const report = buildSnapshotComparisonReport(
      baseline,
      baseline,
      baselineSnapshotPath,
      baselineSnapshotPath,
    );

    expect(report.summary.regressions).toBe(0);
    expect(report.summary.improvements).toBe(0);
    expect(report.summary.changed).toBe(0);
    expect(report.summary.comparedMetrics).toBeGreaterThan(0);
    expect(
      report.comparisons.every((comparison) => comparison.status === "unchanged"),
    ).toBe(true);
  });

  test("comparison report surfaces regressions, improvements, and changed metadata", () => {
    const baseline = loadBaselineSnapshot();
    const candidate = structuredClone(baseline);

    candidate.label = "2026-04";
    candidate.transport.aggregate.total_delta_bytes += 128;
    candidate.transport.aggregate.total_queue_pressure_events = Math.max(
      0,
      candidate.transport.aggregate.total_queue_pressure_events - 1,
    );
    if (candidate.browserRoutes.comparison) {
      candidate.browserRoutes.comparison.workerGatesPassed = false;
    }
    candidate.topologyFeed.worlds[0].authority_row.quest_binding_matches = false;

    const report = buildSnapshotComparisonReport(
      baseline,
      candidate,
      "/tmp/baseline.json",
      "/tmp/candidate.json",
    );

    expect(report.summary.regressions).toBeGreaterThan(0);
    expect(report.summary.improvements).toBeGreaterThan(0);
    expect(report.summary.changed).toBeGreaterThan(0);

    expect(
      report.comparisons.find(
        (comparison) =>
          comparison.category === "transport" &&
          comparison.metric === "aggregate.total_delta_bytes",
      ),
    ).toMatchObject({
      status: "regressed",
      delta: 128,
    });

    expect(
      report.comparisons.find(
        (comparison) =>
          comparison.category === "transport" &&
          comparison.metric === "aggregate.total_queue_pressure_events",
      ),
    ).toMatchObject({
      status: "improved",
      delta: -1,
    });

    expect(
      report.comparisons.find(
        (comparison) =>
          comparison.category === "snapshot" && comparison.metric === "label",
      ),
    ).toMatchObject({
      status: "changed",
      candidate: "2026-04",
    });

    expect(
      report.comparisons.find(
        (comparison) =>
          comparison.category === "topologyFeed.deadman-prime.authority_row" &&
          comparison.metric === "quest_binding_matches",
      ),
    ).toMatchObject({
      status: "regressed",
      candidate: "false",
    });
  });

  test("cli writes output and can fail on regressions", () => {
    const tempDir = mkdtempSync(join(tmpdir(), "pod-snapshot-compare-"));
    const baselinePath = join(tempDir, "baseline.json");
    const candidatePath = join(tempDir, "candidate.json");
    const outputPath = join(tempDir, "comparison.json");

    try {
      const baseline = loadBaselineSnapshot();
      const candidate = structuredClone(baseline);
      candidate.transport.aggregate.total_delta_bytes += 64;

      writeFileSync(baselinePath, `${JSON.stringify(baseline, null, 2)}\n`);
      writeFileSync(candidatePath, `${JSON.stringify(candidate, null, 2)}\n`);

      const passResult = Bun.spawnSync(
        [
          "bun",
          "./scripts/compare_moat_snapshots.ts",
          "--baseline",
          baselinePath,
          "--candidate",
          baselinePath,
          "--output",
          outputPath,
        ],
        {
          cwd: repoRoot,
          env: process.env,
          stdout: "pipe",
          stderr: "pipe",
        },
      );
      expect(passResult.exitCode).toBe(0);

      const persisted = JSON.parse(
        readFileSync(outputPath, "utf8"),
      ) as ReturnType<typeof buildSnapshotComparisonReport>;
      expect(persisted.summary.regressions).toBe(0);

      const failResult = Bun.spawnSync(
        [
          "bun",
          "./scripts/compare_moat_snapshots.ts",
          "--baseline",
          baselinePath,
          "--candidate",
          candidatePath,
          "--output",
          outputPath,
          "--fail-on-regressions",
        ],
        {
          cwd: repoRoot,
          env: process.env,
          stdout: "pipe",
          stderr: "pipe",
        },
      );
      expect(failResult.exitCode).toBe(1);
    } finally {
      rmSync(tempDir, { force: true, recursive: true });
    }
  });
});
