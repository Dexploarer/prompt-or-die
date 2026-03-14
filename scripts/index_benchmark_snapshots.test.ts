import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { describe, expect, test } from "bun:test";

import type { BenchmarkSnapshotComparisonReport } from "./compare_moat_snapshots";
import {
  buildBenchmarkHistoryIndexOutputPath,
  buildBenchmarkHistoryReportOutputPath,
  buildBenchmarkSnapshotHistoryIndex,
  buildBenchmarkSnapshotHistoryMarkdown,
  parseArgs,
  readBenchmarkSnapshotHistoryEntries,
} from "./index_benchmark_snapshots";
import type { PublishedMoatSnapshot } from "./publish_moat_snapshots";

const repoRoot = resolve(import.meta.dir, "..");
const baselineSnapshotPath = resolve(
  repoRoot,
  "docs/benchmark-snapshots/2026-03-shard-target.json",
);
const baselineComparisonPath = resolve(
  repoRoot,
  "docs/benchmark-snapshots/2026-03-shard-target-comparison.json",
);

function loadBaselineSnapshot(): PublishedMoatSnapshot {
  return JSON.parse(readFileSync(baselineSnapshotPath, "utf8")) as PublishedMoatSnapshot;
}

function loadBaselineComparison(): BenchmarkSnapshotComparisonReport {
  return JSON.parse(
    readFileSync(baselineComparisonPath, "utf8"),
  ) as BenchmarkSnapshotComparisonReport;
}

describe("index benchmark snapshots", () => {
  test("parseArgs uses the default retained history outputs", () => {
    expect(parseArgs([])).toEqual({
      inputDir: "docs/benchmark-snapshots",
      outputJson: "docs/benchmark-snapshots/index.json",
      outputMarkdown: "docs/benchmark-snapshots/README.md",
    });

    expect(
      parseArgs([
        "--input-dir",
        "tmp/snapshots",
        "--output-json",
        "tmp/index.json",
        "--output-markdown",
        "tmp/README.md",
      ]),
    ).toEqual({
      inputDir: "tmp/snapshots",
      outputJson: "tmp/index.json",
      outputMarkdown: "tmp/README.md",
    });
  });

  test("builds the default retained history output paths", () => {
    expect(buildBenchmarkHistoryIndexOutputPath()).toBe(
      "docs/benchmark-snapshots/index.json",
    );
    expect(buildBenchmarkHistoryReportOutputPath()).toBe(
      "docs/benchmark-snapshots/README.md",
    );
  });

  test("indexes retained snapshot and comparison history deterministically", () => {
    const tempRoot = mkdtempSync(join(tmpdir(), "pod-benchmark-history-"));
    const snapshotDir = join(tempRoot, "docs/benchmark-snapshots");

    try {
      mkdirSync(snapshotDir, { recursive: true });

      const baselineSnapshot = loadBaselineSnapshot();
      const baselineComparison = loadBaselineComparison();
      const candidateSnapshot = structuredClone(baselineSnapshot);
      const candidateComparison = structuredClone(baselineComparison);

      candidateSnapshot.label = "2026-04";
      candidateSnapshot.transport.aggregate.total_delta_bytes += 64;
      candidateComparison.baselineLabel = "2026-03";
      candidateComparison.candidateLabel = "2026-04";
      candidateComparison.baselinePath =
        "/Users/home/Desktop/prompt-or-die/docs/benchmark-snapshots/2026-03-shard-target.json";
      candidateComparison.candidatePath =
        "/Users/home/Desktop/prompt-or-die/docs/benchmark-snapshots/2026-04-shard-target.json";
      candidateComparison.summary = {
        regressions: 1,
        improvements: 0,
        changed: 1,
        unchanged: 180,
        comparedMetrics: 181,
      };
      candidateComparison.comparisons = [
        {
          category: "transport",
          metric: "aggregate.total_delta_bytes",
          direction: "lower_is_better",
          status: "regressed",
          baseline: "1904",
          candidate: "1968",
          delta: 64,
          envelope: null,
        },
        {
          category: "snapshot",
          metric: "label",
          direction: "informational",
          status: "changed",
          baseline: "2026-03",
          candidate: "2026-04",
          delta: null,
          envelope: null,
        },
      ];

      writeFileSync(
        join(snapshotDir, "2026-03-shard-target.json"),
        `${JSON.stringify(baselineSnapshot, null, 2)}\n`,
      );
      writeFileSync(
        join(snapshotDir, "2026-03-shard-target-comparison.json"),
        `${JSON.stringify(baselineComparison, null, 2)}\n`,
      );
      writeFileSync(
        join(snapshotDir, "2026-04-shard-target.json"),
        `${JSON.stringify(candidateSnapshot, null, 2)}\n`,
      );
      writeFileSync(
        join(snapshotDir, "2026-04-shard-target-comparison.json"),
        `${JSON.stringify(candidateComparison, null, 2)}\n`,
      );
      writeFileSync(join(snapshotDir, "notes.md"), "# ignore\n");

      const entries = readBenchmarkSnapshotHistoryEntries(
        "docs/benchmark-snapshots",
        tempRoot,
      );
      const index = buildBenchmarkSnapshotHistoryIndex(entries, 123);
      const markdown = buildBenchmarkSnapshotHistoryMarkdown(index);

      expect(index.latestLabel).toBe("2026-04");
      expect(index.latestSnapshotPath).toBe(
        "docs/benchmark-snapshots/2026-04-shard-target.json",
      );
      expect(index.latestComparisonPath).toBe(
        "docs/benchmark-snapshots/2026-04-shard-target-comparison.json",
      );
      expect(index.entries).toHaveLength(2);
      expect(index.entries[0]).toMatchObject({
        label: "2026-04",
        comparedAgainstLabel: "2026-03",
        comparisonSummary: {
          regressions: 1,
          changed: 1,
        },
        comparisonHighlights: {
          regressions: [
            {
              category: "transport",
              metric: "aggregate.total_delta_bytes",
            },
          ],
          changed: [
            {
              category: "snapshot",
              metric: "label",
            },
          ],
        },
        snapshotSummary: {
          totalDeltaBytes:
            baselineSnapshot.transport.aggregate.total_delta_bytes + 64,
        },
      });
      expect(markdown).toContain("[snapshot](./2026-04-shard-target.json)");
      expect(markdown).toContain("[comparison](./2026-04-shard-target-comparison.json)");
      expect(markdown).toContain("| 2026-04 |");
      expect(markdown).toContain("## Latest Snapshot Metrics");
      expect(markdown).toContain("## Latest Comparison Highlights");
      expect(markdown).toContain(
        "- transport.aggregate.total_delta_bytes: 1904 -> 1968 (delta 64)",
      );
    } finally {
      rmSync(tempRoot, { recursive: true, force: true });
    }
  });

  test("cli writes the retained history index and markdown report", () => {
    const tempRoot = mkdtempSync(join(tmpdir(), "pod-benchmark-history-cli-"));
    const snapshotDir = join(tempRoot, "snapshots");
    const outputJson = join(tempRoot, "index.json");
    const outputMarkdown = join(tempRoot, "README.md");

    try {
      mkdirSync(snapshotDir, { recursive: true });

      writeFileSync(
        join(snapshotDir, "2026-03-shard-target.json"),
        readFileSync(baselineSnapshotPath, "utf8"),
      );
      writeFileSync(
        join(snapshotDir, "2026-03-shard-target-comparison.json"),
        readFileSync(baselineComparisonPath, "utf8"),
      );

      const result = Bun.spawnSync(
        [
          "bun",
          "./scripts/index_benchmark_snapshots.ts",
          "--input-dir",
          snapshotDir,
          "--output-json",
          outputJson,
          "--output-markdown",
          outputMarkdown,
        ],
        {
          cwd: repoRoot,
          env: process.env,
          stdout: "pipe",
          stderr: "pipe",
        },
      );

      expect(result.exitCode).toBe(0);
      expect(
        JSON.parse(readFileSync(outputJson, "utf8")) as ReturnType<
          typeof buildBenchmarkSnapshotHistoryIndex
        >,
      ).toMatchObject({
        latestLabel: "2026-03",
        snapshotCount: 1,
      });
      expect(readFileSync(outputMarkdown, "utf8")).toContain(
        "# Benchmark Snapshot History",
      );
    } finally {
      rmSync(tempRoot, { recursive: true, force: true });
    }
  });
});
