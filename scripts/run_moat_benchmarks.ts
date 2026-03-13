#!/usr/bin/env bun

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

type Options = {
  profile: "ci-smoke" | "shard-target";
  output: string;
  monthlyHostCostUsd?: number;
  skipBrowser: boolean;
  skipCreator: boolean;
  creatorSeconds?: number;
  creatorCommand?: string;
};

type CommandSummary = {
  name: string;
  cwd: string;
  command: string;
  ok: boolean;
  exitCode: number;
  durationMs: number;
  stderrSnippet?: string;
};

type BrowserParityReport = {
  totalChecks: number;
  passedChecks: number;
  parityScore: number;
  checks: CommandSummary[];
};

type BrowserRouteMeasurementsReport = {
  schemaVersion: number;
  generatedAtUnixMs: number;
  baseUrl: string;
  routes: unknown[];
  comparison: unknown;
};

type TransportMeasurementsReport = {
  schema_version: number;
  generated_at_unix_ms: number;
  profile: string;
  scenarios: unknown[];
  aggregate: {
    all_checks_passed: boolean;
  };
};

type HeadlessTopologyParityReport = {
  consistent: boolean;
  teams_match: boolean;
  worlds_match: boolean;
  links_match: boolean;
  quest_graphs_match: boolean;
  world_quest_bindings_match: boolean;
  applied_world_states_match: boolean;
  evaluation_match: boolean;
  missing_world_quest_binding_ids: string[];
  unexpected_world_quest_binding_ids: string[];
  missing_applied_world_ids: string[];
  unexpected_applied_world_ids: string[];
  missing_evaluation_world_ids: string[];
  unexpected_evaluation_world_ids: string[];
};

type HeadlessTopologySourceReport = {
  schema_version: number;
  scenario: string;
  profile: string;
  teams: Array<{ team_id: string }>;
  worlds: Array<{ world_id: string }>;
  links: Array<{ link_id: string }>;
  world_quest_bindings: Array<{ world_id: string; quest_graph_ids: string[] }>;
  applied_world_states: Array<{ world_id: string }>;
  evaluation: {
    worlds: Array<{ world_id: string }>;
  };
  topology_parity: HeadlessTopologyParityReport;
};

type HeadlessTopologyCheck = {
  metric: string;
  passed: boolean;
  expected: string;
  observed: string;
};

type HeadlessTopologyMeasurementsReport = {
  sourceSchemaVersion: number;
  scenario: string;
  profile: string;
  teamCount: number;
  worldCount: number;
  linkCount: number;
  worldQuestBindingCount: number;
  appliedWorldStateCount: number;
  evaluationWorldCount: number;
  topologyParity: HeadlessTopologyParityReport;
  checks: HeadlessTopologyCheck[];
  allChecksPassed: boolean;
};

type TopologyFeedCheck = {
  metric: string;
  passed: boolean;
  expected: string;
  observed: string;
};

type TopologyFeedWorldPathReport = {
  resolved_world_id: string | null;
  resolved_world_matches: boolean;
  quest_binding_matches: boolean;
  applied_world_state_matches: boolean;
  evaluation_matches: boolean;
};

type TopologyFeedWorldReport = {
  world_id: string;
  authority_row: TopologyFeedWorldPathReport;
  generated_runtime: TopologyFeedWorldPathReport;
};

type TopologyFeedMeasurementsReport = {
  schema_version: number;
  scenario_id: string;
  profile_id: string;
  world_count: number;
  worlds: TopologyFeedWorldReport[];
  checks: TopologyFeedCheck[];
};

type CreatorTimeReport =
  | {
      status: "manual_pending";
      benchmarkSeconds: null;
      notes: string[];
    }
  | {
      status: "reported" | "scripted";
      benchmarkSeconds: number;
      notes: string[];
    };

type CombinedReport = {
  schemaVersion: number;
  generatedAtUnixMs: number;
  profile: Options["profile"];
  core: unknown;
  transportMeasurements: TransportMeasurementsReport;
  headlessTopology: HeadlessTopologyMeasurementsReport;
  topologyFeedMeasurements: TopologyFeedMeasurementsReport;
  browserNativeParity: BrowserParityReport | null;
  browserRouteMeasurements: BrowserRouteMeasurementsReport | null;
  creatorTimeToFirstAgentWorld: CreatorTimeReport;
};

function buildBooleanCheck(
  metric: string,
  observed: boolean,
): HeadlessTopologyCheck {
  return {
    metric,
    passed: observed,
    expected: "true",
    observed: String(observed),
  };
}

export function buildHeadlessTopologyMeasurements(
  report: HeadlessTopologySourceReport,
): HeadlessTopologyMeasurementsReport {
  const checks = [
    buildBooleanCheck(
      "topology_parity.consistent",
      report.topology_parity.consistent,
    ),
    buildBooleanCheck(
      "topology_parity.teams_match",
      report.topology_parity.teams_match,
    ),
    buildBooleanCheck(
      "topology_parity.worlds_match",
      report.topology_parity.worlds_match,
    ),
    buildBooleanCheck(
      "topology_parity.links_match",
      report.topology_parity.links_match,
    ),
    buildBooleanCheck(
      "topology_parity.quest_graphs_match",
      report.topology_parity.quest_graphs_match,
    ),
    buildBooleanCheck(
      "topology_parity.world_quest_bindings_match",
      report.topology_parity.world_quest_bindings_match,
    ),
    buildBooleanCheck(
      "topology_parity.applied_world_states_match",
      report.topology_parity.applied_world_states_match,
    ),
    buildBooleanCheck(
      "topology_parity.evaluation_match",
      report.topology_parity.evaluation_match,
    ),
  ];

  return {
    sourceSchemaVersion: report.schema_version,
    scenario: report.scenario,
    profile: report.profile,
    teamCount: report.teams.length,
    worldCount: report.worlds.length,
    linkCount: report.links.length,
    worldQuestBindingCount: report.world_quest_bindings.length,
    appliedWorldStateCount: report.applied_world_states.length,
    evaluationWorldCount: report.evaluation.worlds.length,
    topologyParity: report.topology_parity,
    allChecksPassed: checks.every((check) => check.passed),
    checks,
  };
}

export function topologyFeedChecksPassed(
  report: TopologyFeedMeasurementsReport,
): boolean {
  return report.checks.every((check) => check.passed);
}

function parseArgs(argv: string[]): Options {
  const options: Options = {
    profile: "ci-smoke",
    output: "artifacts/moat-benchmarks.json",
    skipBrowser: false,
    skipCreator: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--profile": {
        const value = argv[index + 1];
        if (value !== "ci-smoke" && value !== "shard-target") {
          throw new Error("expected --profile ci-smoke|shard-target");
        }
        options.profile = value;
        index += 1;
        break;
      }
      case "--output": {
        const value = argv[index + 1];
        if (!value) {
          throw new Error("missing value for --output");
        }
        options.output = value;
        index += 1;
        break;
      }
      case "--monthly-host-cost-usd": {
        const value = Number(argv[index + 1]);
        if (!Number.isFinite(value)) {
          throw new Error("missing numeric value for --monthly-host-cost-usd");
        }
        options.monthlyHostCostUsd = value;
        index += 1;
        break;
      }
      case "--skip-browser":
        options.skipBrowser = true;
        break;
      case "--skip-creator":
        options.skipCreator = true;
        break;
      case "--creator-seconds": {
        const value = Number(argv[index + 1]);
        if (!Number.isFinite(value)) {
          throw new Error("missing numeric value for --creator-seconds");
        }
        options.creatorSeconds = value;
        index += 1;
        break;
      }
      case "--creator-command": {
        const value = argv[index + 1];
        if (!value) {
          throw new Error("missing value for --creator-command");
        }
        options.creatorCommand = value;
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

function printHelp() {
  console.error(
    "Usage: bun ./scripts/run_moat_benchmarks.ts [--profile ci-smoke|shard-target] [--output PATH] [--monthly-host-cost-usd VALUE] [--skip-browser] [--skip-creator] [--creator-seconds VALUE] [--creator-command \"...\"]",
  );
}

function runCommand(
  name: string,
  argv: string[],
  cwd: string,
): { summary: CommandSummary; stdout: string } {
  const started = performance.now();
  const processResult = Bun.spawnSync(argv, {
    cwd,
    env: process.env,
    stdout: "pipe",
    stderr: "pipe",
  });
  const durationMs = performance.now() - started;
  const stdout = Buffer.from(processResult.stdout).toString("utf8");
  const stderr = Buffer.from(processResult.stderr).toString("utf8");

  return {
    summary: {
      name,
      cwd,
      command: argv.join(" "),
      ok: processResult.exitCode === 0,
      exitCode: processResult.exitCode ?? 1,
      durationMs,
      stderrSnippet: processResult.exitCode === 0 ? undefined : stderr.slice(0, 1200),
    },
    stdout,
  };
}

function buildCreatorReport(repoRoot: string, options: Options): CreatorTimeReport {
  if (options.skipCreator) {
    return {
      status: "manual_pending",
      benchmarkSeconds: null,
      notes: [
        "Creator benchmark explicitly skipped for this run.",
        "Omit --skip-creator to measure the canonical bootstrap automatically.",
      ],
    };
  }

  if (typeof options.creatorSeconds === "number") {
    return {
      status: "reported",
      benchmarkSeconds: options.creatorSeconds,
      notes: [
        "Manual creator benchmark supplied at invocation time.",
        "Use the protocol in docs/benchmark-suite.md for consistent monthly measurements.",
      ],
    };
  }

  if (options.creatorCommand) {
    const result = runCommand(
      "creator-command",
      ["zsh", "-lc", options.creatorCommand],
      repoRoot,
    );
    if (!result.summary.ok) {
      throw new Error(
        `creator benchmark command failed:\n${result.summary.stderrSnippet ?? "no stderr captured"}`,
      );
    }
    return {
      status: "scripted",
      benchmarkSeconds: result.summary.durationMs / 1000,
      notes: [
        "Scripted creator benchmark executed from --creator-command.",
        "Keep the scripted flow aligned with the reference bootstrap path in docs/benchmark-suite.md.",
      ],
    };
  }

  const defaultBootstrap = runCommand(
    "reference-bootstrap",
    [
      "bun",
      "./scripts/bootstrap_reference_world.ts",
      "--measure",
      "--host",
      "127.0.0.1",
      "--port",
      "4178",
    ],
    repoRoot,
  );
  if (!defaultBootstrap.summary.ok) {
    throw new Error(
      `reference bootstrap benchmark failed:\n${defaultBootstrap.summary.stderrSnippet ?? "no stderr captured"}`,
    );
  }

  const bootstrap = JSON.parse(defaultBootstrap.stdout) as {
    startupTimeMs: number;
    url: string;
  };
  return {
    status: "scripted",
    benchmarkSeconds: bootstrap.startupTimeMs / 1000,
    notes: [
      `Measured from the canonical bootstrap at ${bootstrap.url}.`,
      "Override with --creator-command if a different official starter flow replaces the local sandbox bootstrap.",
    ],
  };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const repoRoot = resolve(import.meta.dir, "..");
  const tempDir = mkdtempSync(join(tmpdir(), "pod-moat-"));
  const topologyOutputPath = resolve(
    tempDir,
    `pod-headless-topology-${options.profile}.json`,
  );

  try {
    const coreArgs = [
      "cargo",
      "run",
      "-p",
      "pod-core",
      "--example",
      "moat_benchmark_suite",
      "--release",
      "--",
      "--profile",
      options.profile,
    ];
    if (typeof options.monthlyHostCostUsd === "number") {
      coreArgs.push("--monthly-host-cost-usd", String(options.monthlyHostCostUsd));
    }

    const coreCommand = runCommand("core-moat-benchmark", coreArgs, repoRoot);
    if (!coreCommand.summary.ok) {
      throw new Error(
        `core benchmark failed:\n${coreCommand.summary.stderrSnippet ?? "no stderr captured"}`,
      );
    }

    const core = JSON.parse(coreCommand.stdout);
    const transportCommand = runCommand(
      "transport-benchmark",
      [
        "cargo",
        "run",
        "-p",
        "pod-net",
        "--example",
        "transport_benchmark_suite",
        "--",
        "--profile",
        options.profile,
        "--fail-on-checks",
      ],
      repoRoot,
    );
    if (!transportCommand.summary.ok) {
      throw new Error(
        `transport benchmark failed:\n${transportCommand.summary.stderrSnippet ?? "no stderr captured"}`,
      );
    }
    const transportMeasurements = JSON.parse(
      transportCommand.stdout,
    ) as TransportMeasurementsReport;
    const headlessCommand = runCommand(
      "headless-topology-benchmark",
      [
        "cargo",
        "run",
        "-p",
        "pod-headless",
        "--",
        "--profile",
        options.profile,
        "--topology-output",
        topologyOutputPath,
      ],
      repoRoot,
    );
    if (!headlessCommand.summary.ok) {
      throw new Error(
        `headless topology benchmark failed:\n${headlessCommand.summary.stderrSnippet ?? "no stderr captured"}`,
      );
    }
    const headlessTopology = buildHeadlessTopologyMeasurements(
      JSON.parse(headlessCommand.stdout) as HeadlessTopologySourceReport,
    );
    if (!headlessTopology.allChecksPassed) {
      const failedChecks = headlessTopology.checks
        .filter((check) => !check.passed)
        .map(
          (check) =>
            `${check.metric} expected ${check.expected} observed ${check.observed}`,
        )
        .join("\n");
      throw new Error(`headless topology parity checks failed:\n${failedChecks}`);
    }

    const topologyFeedCommand = runCommand(
      "topology-feed-benchmark",
      [
        "cargo",
        "run",
        "-p",
        "pod-net",
        "--features",
        "spacetimedb",
        "--example",
        "topology_feed_benchmark_suite",
        "--",
        "--topology-input",
        topologyOutputPath,
        "--fail-on-checks",
      ],
      repoRoot,
    );
    if (!topologyFeedCommand.summary.ok) {
      throw new Error(
        `topology feed benchmark failed:\n${topologyFeedCommand.summary.stderrSnippet ?? "no stderr captured"}`,
      );
    }
    const topologyFeedMeasurements = JSON.parse(
      topologyFeedCommand.stdout,
    ) as TopologyFeedMeasurementsReport;
    if (!topologyFeedChecksPassed(topologyFeedMeasurements)) {
      const failedChecks = topologyFeedMeasurements.checks
        .filter((check) => !check.passed)
        .map(
          (check) =>
            `${check.metric} expected ${check.expected} observed ${check.observed}`,
        )
        .join("\n");
      throw new Error(`topology feed parity checks failed:\n${failedChecks}`);
    }

    let browserNativeParity: BrowserParityReport | null = null;
    let browserRouteMeasurements: BrowserRouteMeasurementsReport | null = null;
    if (!options.skipBrowser) {
      const routeMeasurementCommand = runCommand(
        "pod-web-render-route-measurements",
        ["bun", "run", "measure:render-routes:check"],
        `${repoRoot}/apps/pod-web`,
      );
      const checks = [
        runCommand("native-render-tests", ["cargo", "test", "-p", "pod-render", "--lib"], repoRoot)
          .summary,
        runCommand(
          "pod-web-verify-assets",
          ["bun", "run", "verify:assets"],
          `${repoRoot}/apps/pod-web`,
        ).summary,
        runCommand(
          "pod-web-typecheck",
          ["bun", "run", "typecheck"],
          `${repoRoot}/apps/pod-web`,
        ).summary,
        runCommand(
          "pod-web-unit-tests",
          ["bun", "test"],
          `${repoRoot}/apps/pod-web`,
        ).summary,
        runCommand(
          "pod-web-smoke-tests",
          ["bun", "run", "test:smoke"],
          `${repoRoot}/apps/pod-web`,
        ).summary,
        routeMeasurementCommand.summary,
      ];
      const passedChecks = checks.filter((check) => check.ok).length;
      browserNativeParity = {
        totalChecks: checks.length,
        passedChecks,
        parityScore: checks.length === 0 ? 0 : passedChecks / checks.length,
        checks,
      };
      if (routeMeasurementCommand.summary.ok) {
        browserRouteMeasurements = JSON.parse(
          routeMeasurementCommand.stdout,
        ) as BrowserRouteMeasurementsReport;
      }
    }

    const report: CombinedReport = {
      schemaVersion: 4,
      generatedAtUnixMs: Date.now(),
      profile: options.profile,
      core,
      transportMeasurements,
      headlessTopology,
      topologyFeedMeasurements,
      browserNativeParity,
      browserRouteMeasurements,
      creatorTimeToFirstAgentWorld: buildCreatorReport(repoRoot, options),
    };

    const outputPath = resolve(repoRoot, options.output);
    mkdirSync(dirname(outputPath), { recursive: true });
    writeFileSync(outputPath, JSON.stringify(report, null, 2));
    console.log(JSON.stringify(report, null, 2));
  } finally {
    rmSync(tempDir, { recursive: true, force: true });
  }
}

if (import.meta.main) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
