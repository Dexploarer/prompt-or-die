#!/usr/bin/env bun

import { mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, relative, resolve, sep } from "node:path";

import type { BenchmarkSnapshotComparisonReport } from "./compare_moat_snapshots";
import type { PublishedMoatSnapshot } from "./publish_moat_snapshots";

type Options = {
  inputDir: string;
  outputJson: string;
  outputMarkdown: string;
};

export type BenchmarkSnapshotHistoryEntry = {
  label: string;
  snapshotPath: string;
  comparisonPath: string | null;
  comparedAgainstLabel: string | null;
  snapshotSummary: {
    transportChecksPassed: boolean;
    workerRouteGatesPassed: boolean | null;
    headlessChecksPassed: boolean;
    topologyFeedChecksPassed: boolean;
    liveTopologyFeedChecksPassed: boolean | null;
    tournamentPhase: string;
    worldCount: number;
    totalDeltaBytes: number;
    totalQueuePressureEvents: number;
  };
  comparisonSummary: {
    baselineLabel: string;
    candidateLabel: string;
    regressions: number;
    improvements: number;
    changed: number;
    unchanged: number;
    comparedMetrics: number;
  } | null;
  comparisonHighlights: {
    regressions: BenchmarkSnapshotHistoryHighlight[];
    improvements: BenchmarkSnapshotHistoryHighlight[];
    changed: BenchmarkSnapshotHistoryHighlight[];
  } | null;
};

export type BenchmarkSnapshotHistoryHighlight = {
  category: string;
  metric: string;
  baseline: string;
  candidate: string;
  delta: number | null;
  envelope: string | null;
};

export type BenchmarkSnapshotHistoryIndex = {
  schemaVersion: 1;
  profile: "shard-target";
  generatedAtUnixMs: number;
  snapshotCount: number;
  latestLabel: string | null;
  latestSnapshotPath: string | null;
  latestComparisonPath: string | null;
  entries: BenchmarkSnapshotHistoryEntry[];
};

const DEFAULT_INPUT_DIR = "docs/benchmark-snapshots";
const SNAPSHOT_FILENAME_PATTERN = /^(\d{4}-W\d{2})-shard-target\.json$/;

export function buildBenchmarkHistoryIndexOutputPath(): string {
  return `${DEFAULT_INPUT_DIR}/index.json`;
}

export function buildBenchmarkHistoryReportOutputPath(): string {
  return `${DEFAULT_INPUT_DIR}/README.md`;
}

function printHelp() {
  console.error(
    "Usage: bun ./scripts/index_benchmark_snapshots.ts [--input-dir docs/benchmark-snapshots] [--output-json docs/benchmark-snapshots/index.json] [--output-markdown docs/benchmark-snapshots/README.md]",
  );
}

export function parseArgs(argv: string[]): Options {
  const options: Options = {
    inputDir: DEFAULT_INPUT_DIR,
    outputJson: buildBenchmarkHistoryIndexOutputPath(),
    outputMarkdown: buildBenchmarkHistoryReportOutputPath(),
  };

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--input-dir": {
        const value = argv[index + 1];
        if (!value) {
          throw new Error("missing value for --input-dir");
        }
        options.inputDir = value;
        index += 1;
        break;
      }
      case "--output-json": {
        const value = argv[index + 1];
        if (!value) {
          throw new Error("missing value for --output-json");
        }
        options.outputJson = value;
        index += 1;
        break;
      }
      case "--output-markdown": {
        const value = argv[index + 1];
        if (!value) {
          throw new Error("missing value for --output-markdown");
        }
        options.outputMarkdown = value;
        index += 1;
        break;
      }
      case "--help":
      case "-h":
        printHelp();
        process.exit(0);
      default:
        throw new Error(`unknown argument: ${current}`);
    }
  }

  return options;
}

function normalizePathForPublication(path: string, root: string): string {
  const relativePath = relative(root, path);
  if (
    relativePath &&
    relativePath !== ".." &&
    !relativePath.startsWith(`..${sep}`) &&
    relativePath !== "."
  ) {
    return relativePath.replaceAll(sep, "/");
  }
  return path;
}

function readSnapshot(path: string): PublishedMoatSnapshot {
  return JSON.parse(readFileSync(path, "utf8")) as PublishedMoatSnapshot;
}

function readComparison(path: string): BenchmarkSnapshotComparisonReport {
  return JSON.parse(readFileSync(path, "utf8")) as BenchmarkSnapshotComparisonReport;
}

function allChecksPass(checks: Array<{ passed: boolean }>): boolean {
  return checks.every((check) => check.passed);
}

function formatMarkdownPath(path: string | null): string {
  if (path == null) {
    return "n/a";
  }
  return `./${basename(path)}`;
}

function formatNullableBoolean(value: boolean | null): string {
  if (value == null) {
    return "n/a";
  }
  return value ? "pass" : "fail";
}

function buildComparisonHighlights(
  comparison: BenchmarkSnapshotComparisonReport,
): NonNullable<BenchmarkSnapshotHistoryEntry["comparisonHighlights"]> {
  const pick = (status: "regressed" | "improved" | "changed") =>
    comparison.comparisons
      .filter((item) => item.status === status)
      .slice(0, 5)
      .map((item) => ({
        category: item.category,
        metric: item.metric,
        baseline: item.baseline,
        candidate: item.candidate,
        delta: item.delta,
        envelope: item.envelope,
      }));

  return {
    regressions: pick("regressed"),
    improvements: pick("improved"),
    changed: pick("changed"),
  };
}

export function buildBenchmarkSnapshotHistoryEntry(
  snapshot: PublishedMoatSnapshot,
  snapshotPath: string,
  comparison: BenchmarkSnapshotComparisonReport | null,
  comparisonPath: string | null,
): BenchmarkSnapshotHistoryEntry {
  return {
    label: snapshot.label,
    snapshotPath,
    comparisonPath,
    comparedAgainstLabel: comparison?.baselineLabel ?? null,
    snapshotSummary: {
      transportChecksPassed: snapshot.transport.aggregate.all_checks_passed,
      workerRouteGatesPassed:
        snapshot.browserRoutes.comparison?.workerGatesPassed ?? null,
      headlessChecksPassed: snapshot.headlessTopology.allChecksPassed,
      topologyFeedChecksPassed: allChecksPass(snapshot.topologyFeed.checks),
      liveTopologyFeedChecksPassed: snapshot.liveTopologyFeed
        ? allChecksPass(snapshot.liveTopologyFeed.checks)
        : null,
      tournamentPhase: snapshot.headlessTopology.tournamentOrchestration.phase,
      worldCount: snapshot.headlessTopology.worldCount,
      totalDeltaBytes: snapshot.transport.aggregate.total_delta_bytes,
      totalQueuePressureEvents:
        snapshot.transport.aggregate.total_queue_pressure_events,
    },
    comparisonSummary: comparison == null
      ? null
      : {
          baselineLabel: comparison.baselineLabel,
          candidateLabel: comparison.candidateLabel,
          regressions: comparison.summary.regressions,
          improvements: comparison.summary.improvements,
          changed: comparison.summary.changed,
          unchanged: comparison.summary.unchanged,
          comparedMetrics: comparison.summary.comparedMetrics,
        },
    comparisonHighlights:
      comparison == null ? null : buildComparisonHighlights(comparison),
  };
}

export function readBenchmarkSnapshotHistoryEntries(
  inputDir: string,
  root: string,
): BenchmarkSnapshotHistoryEntry[] {
  const resolvedInputDir = resolve(root, inputDir);
  const filenames = readdirSync(resolvedInputDir);

  const snapshotLabels = filenames
    .map((filename) => SNAPSHOT_FILENAME_PATTERN.exec(filename)?.[1] ?? null)
    .filter((label): label is string => label != null)
    .sort((left, right) => right.localeCompare(left));

  return snapshotLabels.map((label) => {
    const snapshotPath = resolve(resolvedInputDir, `${label}-shard-target.json`);
    const comparisonFilename = `${label}-shard-target-comparison.json`;
    const comparisonFullPath = resolve(resolvedInputDir, comparisonFilename);
    const comparisonExists = filenames.includes(comparisonFilename);

    return buildBenchmarkSnapshotHistoryEntry(
      readSnapshot(snapshotPath),
      normalizePathForPublication(snapshotPath, root),
      comparisonExists ? readComparison(comparisonFullPath) : null,
      comparisonExists
        ? normalizePathForPublication(comparisonFullPath, root)
        : null,
    );
  });
}

export function buildBenchmarkSnapshotHistoryIndex(
  entries: BenchmarkSnapshotHistoryEntry[],
  generatedAtUnixMs = Date.now(),
): BenchmarkSnapshotHistoryIndex {
  return {
    schemaVersion: 1,
    profile: "shard-target",
    generatedAtUnixMs,
    snapshotCount: entries.length,
    latestLabel: entries[0]?.label ?? null,
    latestSnapshotPath: entries[0]?.snapshotPath ?? null,
    latestComparisonPath: entries[0]?.comparisonPath ?? null,
    entries,
  };
}

export function buildBenchmarkSnapshotHistoryMarkdown(
  index: BenchmarkSnapshotHistoryIndex,
): string {
  const latestEntry = index.entries[0] ?? null;
  const lines = [
    "# Benchmark Snapshot History",
    "",
    "Generated by `bun ./scripts/index_benchmark_snapshots.ts`.",
    "",
    `- Generated at: ${new Date(index.generatedAtUnixMs).toISOString()}`,
    `- Snapshot count: ${index.snapshotCount}`,
    `- Latest snapshot: ${index.latestLabel ?? "n/a"}`,
    index.latestComparisonPath == null
      ? "- Latest comparison: n/a"
      : `- Latest comparison: [${basename(index.latestComparisonPath)}](${formatMarkdownPath(index.latestComparisonPath)})`,
    "",
    "| Label | Snapshot | Comparison | Baseline | Regressions | Changed | Transport | Worker | Headless | Feed | Live Feed | Phase | Worlds | Delta Bytes | Queue Pressure |",
    "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
  ];

  for (const entry of index.entries) {
    lines.push(
      `| ${entry.label} | [snapshot](${formatMarkdownPath(entry.snapshotPath)}) | ${
        entry.comparisonPath == null
          ? "n/a"
          : `[comparison](${formatMarkdownPath(entry.comparisonPath)})`
      } | ${entry.comparedAgainstLabel ?? "n/a"} | ${
        entry.comparisonSummary?.regressions ?? "n/a"
      } | ${entry.comparisonSummary?.changed ?? "n/a"} | ${
        entry.snapshotSummary.transportChecksPassed ? "pass" : "fail"
      } | ${formatNullableBoolean(entry.snapshotSummary.workerRouteGatesPassed)} | ${
        entry.snapshotSummary.headlessChecksPassed ? "pass" : "fail"
      } | ${entry.snapshotSummary.topologyFeedChecksPassed ? "pass" : "fail"} | ${
        formatNullableBoolean(entry.snapshotSummary.liveTopologyFeedChecksPassed)
      } | ${entry.snapshotSummary.tournamentPhase} | ${
        entry.snapshotSummary.worldCount
      } | ${entry.snapshotSummary.totalDeltaBytes} | ${
        entry.snapshotSummary.totalQueuePressureEvents
      } |`,
    );
  }

  if (latestEntry) {
    lines.push(
      "",
      "## Latest Snapshot Metrics",
      "",
      `- Label: ${latestEntry.label}`,
      `- Compared against: ${latestEntry.comparedAgainstLabel ?? "n/a"}`,
      `- Tournament phase: ${latestEntry.snapshotSummary.tournamentPhase}`,
      `- World count: ${latestEntry.snapshotSummary.worldCount}`,
      `- Transport checks: ${
        latestEntry.snapshotSummary.transportChecksPassed ? "pass" : "fail"
      }`,
      `- Worker route gates: ${formatNullableBoolean(
        latestEntry.snapshotSummary.workerRouteGatesPassed,
      )}`,
      `- Headless topology checks: ${
        latestEntry.snapshotSummary.headlessChecksPassed ? "pass" : "fail"
      }`,
      `- Topology feed checks: ${
        latestEntry.snapshotSummary.topologyFeedChecksPassed ? "pass" : "fail"
      }`,
      `- Live topology feed checks: ${formatNullableBoolean(
        latestEntry.snapshotSummary.liveTopologyFeedChecksPassed,
      )}`,
      `- Total delta bytes: ${latestEntry.snapshotSummary.totalDeltaBytes}`,
      `- Queue pressure events: ${
        latestEntry.snapshotSummary.totalQueuePressureEvents
      }`,
    );

    if (latestEntry.comparisonSummary && latestEntry.comparisonHighlights) {
      const pushHighlights = (
        title: string,
        highlights: BenchmarkSnapshotHistoryHighlight[],
      ) => {
        lines.push("", `### ${title}`, "");
        if (highlights.length === 0) {
          lines.push("- None.");
          return;
        }
        for (const highlight of highlights) {
          const delta =
            highlight.delta == null ? "" : ` (delta ${highlight.delta})`;
          const envelope =
            highlight.envelope == null ? "" : ` within ${highlight.envelope}`;
          lines.push(
            `- ${highlight.category}.${highlight.metric}: ${highlight.baseline} -> ${highlight.candidate}${delta}${envelope}`,
          );
        }
      };

      lines.push(
        "",
        "## Latest Comparison Highlights",
        "",
        `- Compared metrics: ${latestEntry.comparisonSummary.comparedMetrics}`,
        `- Regressions: ${latestEntry.comparisonSummary.regressions}`,
        `- Improvements: ${latestEntry.comparisonSummary.improvements}`,
        `- Changed: ${latestEntry.comparisonSummary.changed}`,
        `- Unchanged: ${latestEntry.comparisonSummary.unchanged}`,
      );
      pushHighlights("Regressions", latestEntry.comparisonHighlights.regressions);
      pushHighlights(
        "Improvements",
        latestEntry.comparisonHighlights.improvements,
      );
      pushHighlights("Changed", latestEntry.comparisonHighlights.changed);
    }
  }

  lines.push("");
  return `${lines.join("\n")}\n`;
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const repoRoot = resolve(import.meta.dir, "..");
  const entries = readBenchmarkSnapshotHistoryEntries(options.inputDir, repoRoot);
  const index = buildBenchmarkSnapshotHistoryIndex(entries);
  const markdown = buildBenchmarkSnapshotHistoryMarkdown(index);
  const outputJsonPath = resolve(repoRoot, options.outputJson);
  const outputMarkdownPath = resolve(repoRoot, options.outputMarkdown);

  mkdirSync(dirname(outputJsonPath), { recursive: true });
  mkdirSync(dirname(outputMarkdownPath), { recursive: true });
  writeFileSync(outputJsonPath, `${JSON.stringify(index, null, 2)}\n`);
  writeFileSync(outputMarkdownPath, markdown);

  console.log(
    JSON.stringify(
      {
        snapshotCount: index.snapshotCount,
        latestLabel: index.latestLabel,
        outputJson: outputJsonPath,
        outputMarkdown: outputMarkdownPath,
      },
      null,
      2,
    ),
  );
}

if (import.meta.main) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
