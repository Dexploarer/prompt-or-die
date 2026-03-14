#!/usr/bin/env bun

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

import type { PublishedMoatSnapshot } from "./publish_moat_snapshots";

export type ComparisonDirection =
  | "lower_is_better"
  | "higher_is_better"
  | "must_stay_true"
  | "must_match_baseline"
  | "must_stay_within_envelope"
  | "informational";

export type ComparisonStatus =
  | "improved"
  | "regressed"
  | "unchanged"
  | "changed";

export type SnapshotMetricComparison = {
  category: string;
  metric: string;
  direction: ComparisonDirection;
  status: ComparisonStatus;
  baseline: string;
  candidate: string;
  delta: number | null;
  envelope: string | null;
};

export type BenchmarkSnapshotComparisonReport = {
  schemaVersion: 2;
  profile: "shard-target";
  baselineLabel: string;
  candidateLabel: string;
  baselinePath: string;
  candidatePath: string;
  summary: {
    regressions: number;
    improvements: number;
    changed: number;
    unchanged: number;
    comparedMetrics: number;
  };
  comparisons: SnapshotMetricComparison[];
};

type Options = {
  baseline: string;
  candidate: string;
  output: string;
  failOnRegressions: boolean;
};

const DEFAULT_OUTPUT = "artifacts/benchmark-snapshot-comparison.json";

function printHelp() {
  console.error(
    "Usage: bun ./scripts/compare_moat_snapshots.ts --baseline docs/benchmark-snapshots/2026-W10-shard-target.json --candidate docs/benchmark-snapshots/2026-W11-shard-target.json [--output artifacts/benchmark-snapshot-comparison.json] [--fail-on-regressions]",
  );
}

export function parseArgs(argv: string[]): Options {
  const options: Options = {
    baseline: "",
    candidate: "",
    output: DEFAULT_OUTPUT,
    failOnRegressions: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--baseline":
        options.baseline = argv[index + 1] ?? "";
        index += 1;
        break;
      case "--candidate":
        options.candidate = argv[index + 1] ?? "";
        index += 1;
        break;
      case "--output":
        options.output = argv[index + 1] ?? options.output;
        index += 1;
        break;
      case "--fail-on-regressions":
        options.failOnRegressions = true;
        break;
      case "--help":
      case "-h":
        printHelp();
        process.exit(0);
      default:
        throw new Error(`unknown argument: ${current}`);
    }
  }

  if (!options.baseline) {
    throw new Error("missing required --baseline");
  }
  if (!options.candidate) {
    throw new Error("missing required --candidate");
  }

  return options;
}

function readSnapshot(path: string): PublishedMoatSnapshot {
  return JSON.parse(readFileSync(path, "utf8")) as PublishedMoatSnapshot;
}

function formatValue(value: boolean | number | string | null | undefined): string {
  if (value == null) {
    return "null";
  }
  if (typeof value === "string") {
    return value;
  }
  return String(value);
}

function compareNumberMetric(
  category: string,
  metric: string,
  baseline: number,
  candidate: number,
  direction: Exclude<ComparisonDirection, "must_stay_true" | "informational">,
): SnapshotMetricComparison {
  const delta = Number((candidate - baseline).toFixed(3));
  let status: ComparisonStatus = "unchanged";

  if (candidate !== baseline) {
    if (direction === "lower_is_better") {
      status = candidate < baseline ? "improved" : "regressed";
    } else {
      status = candidate > baseline ? "improved" : "regressed";
    }
  }

  return {
    category,
    metric,
    direction,
    status,
    baseline: formatValue(baseline),
    candidate: formatValue(candidate),
    delta,
    envelope: null,
  };
}

function compareBooleanMetric(
  category: string,
  metric: string,
  baseline: boolean,
  candidate: boolean,
): SnapshotMetricComparison {
  let status: ComparisonStatus = "unchanged";
  if (baseline !== candidate) {
    status = candidate ? "improved" : "regressed";
  }

  return {
    category,
    metric,
    direction: "must_stay_true",
    status,
    baseline: formatValue(baseline),
    candidate: formatValue(candidate),
    delta: null,
    envelope: null,
  };
}

function compareInformationalMetric(
  category: string,
  metric: string,
  baseline: boolean | number | string | null | undefined,
  candidate: boolean | number | string | null | undefined,
): SnapshotMetricComparison {
  const normalizedBaseline = formatValue(baseline);
  const normalizedCandidate = formatValue(candidate);
  return {
    category,
    metric,
    direction: "informational",
    status:
      normalizedBaseline === normalizedCandidate ? "unchanged" : "changed",
    baseline: normalizedBaseline,
    candidate: normalizedCandidate,
    delta:
      typeof baseline === "number" && typeof candidate === "number"
        ? Number((candidate - baseline).toFixed(3))
        : null,
    envelope: null,
  };
}

function compareExactBaselineMetric(
  category: string,
  metric: string,
  baseline: boolean | number | string,
  candidate: boolean | number | string,
): SnapshotMetricComparison {
  return {
    category,
    metric,
    direction: "must_match_baseline",
    status: baseline === candidate ? "unchanged" : "regressed",
    baseline: formatValue(baseline),
    candidate: formatValue(candidate),
    delta:
      typeof baseline === "number" && typeof candidate === "number"
        ? Number((candidate - baseline).toFixed(3))
        : null,
    envelope: formatValue(baseline),
  };
}

function compareNumberEnvelopeMetric(
  category: string,
  metric: string,
  baseline: number,
  candidate: number,
  minDelta: number,
  maxDelta: number,
): SnapshotMetricComparison {
  const minimum = baseline + minDelta;
  const maximum = baseline + maxDelta;
  const delta = Number((candidate - baseline).toFixed(3));
  const withinEnvelope = candidate >= minimum && candidate <= maximum;

  let status: ComparisonStatus = "unchanged";
  if (!withinEnvelope) {
    status = "regressed";
  } else if (candidate !== baseline) {
    status = "changed";
  }

  return {
    category,
    metric,
    direction: "must_stay_within_envelope",
    status,
    baseline: formatValue(baseline),
    candidate: formatValue(candidate),
    delta,
    envelope: `[${formatValue(minimum)}, ${formatValue(maximum)}]`,
  };
}

function pushRouteComparisons(
  comparisons: SnapshotMetricComparison[],
  baseline: PublishedMoatSnapshot,
  candidate: PublishedMoatSnapshot,
) {
  const baselineComparison = baseline.browserRoutes.comparison;
  const candidateComparison = candidate.browserRoutes.comparison;
  comparisons.push(
    compareInformationalMetric(
      "browserRoutes",
      "comparison.present",
      Boolean(baselineComparison),
      Boolean(candidateComparison),
    ),
  );

  if (baselineComparison && candidateComparison) {
    comparisons.push(
      compareBooleanMetric(
        "browserRoutes",
        "comparison.workerGatesPassed",
        baselineComparison.workerGatesPassed,
        candidateComparison.workerGatesPassed,
      ),
      compareNumberMetric(
        "browserRoutes",
        "comparison.frameSubmissionReductionPercent",
        baselineComparison.frameSubmissionReductionPercent,
        candidateComparison.frameSubmissionReductionPercent,
        "higher_is_better",
      ),
      compareNumberMetric(
        "browserRoutes",
        "comparison.mainStableFramePercent",
        baselineComparison.mainStableFramePercent,
        candidateComparison.mainStableFramePercent,
        "higher_is_better",
      ),
      compareNumberMetric(
        "browserRoutes",
        "comparison.workerStableFramePercent",
        baselineComparison.workerStableFramePercent,
        candidateComparison.workerStableFramePercent,
        "higher_is_better",
      ),
      compareNumberMetric(
        "browserRoutes",
        "comparison.mainSlowFrames",
        baselineComparison.mainSlowFrames,
        candidateComparison.mainSlowFrames,
        "lower_is_better",
      ),
      compareNumberMetric(
        "browserRoutes",
        "comparison.workerSlowFrames",
        baselineComparison.workerSlowFrames,
        candidateComparison.workerSlowFrames,
        "lower_is_better",
      ),
    );
  }

  const labels = new Set([
    ...baseline.browserRoutes.routes.map((route) => route.label),
    ...candidate.browserRoutes.routes.map((route) => route.label),
  ]);

  for (const label of labels) {
    const baselineRoute = baseline.browserRoutes.routes.find(
      (route) => route.label === label,
    );
    const candidateRoute = candidate.browserRoutes.routes.find(
      (route) => route.label === label,
    );
    const category = `browserRoutes.${label}`;

    comparisons.push(
      compareInformationalMetric(
        category,
        "present",
        Boolean(baselineRoute),
        Boolean(candidateRoute),
      ),
    );

    if (!baselineRoute || !candidateRoute) {
      continue;
    }

    comparisons.push(
      compareNumberMetric(
        category,
        "runtimePerf.stableFramePercent",
        baselineRoute.runtimePerf.stableFramePercent,
        candidateRoute.runtimePerf.stableFramePercent,
        "higher_is_better",
      ),
      compareNumberMetric(
        category,
        "runtimePerf.slowFrames",
        baselineRoute.runtimePerf.slowFrames,
        candidateRoute.runtimePerf.slowFrames,
        "lower_is_better",
      ),
      compareNumberMetric(
        category,
        "assetLoadPerf.averageGeometryLoadMs",
        baselineRoute.assetLoadPerf.averageGeometryLoadMs,
        candidateRoute.assetLoadPerf.averageGeometryLoadMs,
        "lower_is_better",
      ),
      compareNumberMetric(
        category,
        "assetLoadPerf.averageSpriteLoadMs",
        baselineRoute.assetLoadPerf.averageSpriteLoadMs,
        candidateRoute.assetLoadPerf.averageSpriteLoadMs,
        "lower_is_better",
      ),
      compareNumberMetric(
        category,
        "mainThreadPerf.byKind.control.submissionsCompleted",
        baselineRoute.mainThreadPerf.byKind.control.submissionsCompleted,
        candidateRoute.mainThreadPerf.byKind.control.submissionsCompleted,
        "lower_is_better",
      ),
      compareNumberMetric(
        category,
        "mainThreadPerf.byKind.resize.submissionsCompleted",
        baselineRoute.mainThreadPerf.byKind.resize.submissionsCompleted,
        candidateRoute.mainThreadPerf.byKind.resize.submissionsCompleted,
        "lower_is_better",
      ),
    );
  }
}

function pushTopologyWorldComparisons(
  comparisons: SnapshotMetricComparison[],
  category: string,
  baselineWorlds:
    | PublishedMoatSnapshot["topologyFeed"]["worlds"]
    | NonNullable<PublishedMoatSnapshot["liveTopologyFeed"]>["worlds"],
  candidateWorlds:
    | PublishedMoatSnapshot["topologyFeed"]["worlds"]
    | NonNullable<PublishedMoatSnapshot["liveTopologyFeed"]>["worlds"],
) {
  const worldIds = new Set([
    ...baselineWorlds.map((world) => world.world_id),
    ...candidateWorlds.map((world) => world.world_id),
  ]);

  for (const worldId of worldIds) {
    const baselineWorld = baselineWorlds.find((world) => world.world_id === worldId);
    const candidateWorld = candidateWorlds.find((world) => world.world_id === worldId);
    const worldCategory = `${category}.${worldId}`;

    comparisons.push(
      compareInformationalMetric(
        worldCategory,
        "present",
        Boolean(baselineWorld),
        Boolean(candidateWorld),
      ),
    );

    if (!baselineWorld || !candidateWorld) {
      continue;
    }

    for (const path of ["authority_row", "generated_runtime"] as const) {
      const pathCategory = `${worldCategory}.${path}`;
      const baselinePath = baselineWorld[path];
      const candidatePath = candidateWorld[path];
      comparisons.push(
        compareBooleanMetric(
          pathCategory,
          "resolved_world_matches",
          baselinePath.resolved_world_matches,
          candidatePath.resolved_world_matches,
        ),
        compareBooleanMetric(
          pathCategory,
          "quest_binding_matches",
          baselinePath.quest_binding_matches,
          candidatePath.quest_binding_matches,
        ),
        compareBooleanMetric(
          pathCategory,
          "applied_world_state_matches",
          baselinePath.applied_world_state_matches,
          candidatePath.applied_world_state_matches,
        ),
        compareBooleanMetric(
          pathCategory,
          "evaluation_matches",
          baselinePath.evaluation_matches,
          candidatePath.evaluation_matches,
        ),
        compareBooleanMetric(
          pathCategory,
          "world_tournament_orchestration_matches",
          baselinePath.world_tournament_orchestration_matches ?? true,
          candidatePath.world_tournament_orchestration_matches ?? true,
        ),
        compareBooleanMetric(
          pathCategory,
          "tournament_control_plane_matches",
          baselinePath.tournament_control_plane_matches ?? true,
          candidatePath.tournament_control_plane_matches ?? true,
        ),
        compareBooleanMetric(
          pathCategory,
          "tournament_orchestration_matches",
          baselinePath.tournament_orchestration_matches ?? true,
          candidatePath.tournament_orchestration_matches ?? true,
        ),
        compareInformationalMetric(
          pathCategory,
          "resolved_world_id",
          baselinePath.resolved_world_id,
          candidatePath.resolved_world_id,
        ),
      );
    }
  }
}

export function buildSnapshotComparisonReport(
  baseline: PublishedMoatSnapshot,
  candidate: PublishedMoatSnapshot,
  baselinePath: string,
  candidatePath: string,
): BenchmarkSnapshotComparisonReport {
  const baselineTournamentOrchestration =
    baseline.headlessTopology.tournamentOrchestration ?? {
      phase: "unknown",
      activeWorldCount: null,
      contestedWorldCount: null,
      activeLinkCount: null,
      leadingTeamCount: null,
      atRiskTeamCount: null,
      pressureWorldCount: null,
      neuralSwarmWorldCount: null,
    };
  const candidateTournamentOrchestration =
    candidate.headlessTopology.tournamentOrchestration ?? {
      phase: "unknown",
      activeWorldCount: null,
      contestedWorldCount: null,
      activeLinkCount: null,
      leadingTeamCount: null,
      atRiskTeamCount: null,
      pressureWorldCount: null,
      neuralSwarmWorldCount: null,
    };
  const comparisons: SnapshotMetricComparison[] = [];

  comparisons.push(
    compareInformationalMetric("snapshot", "label", baseline.label, candidate.label),
    compareInformationalMetric("snapshot", "schemaVersion", baseline.schemaVersion, candidate.schemaVersion),
    compareInformationalMetric("snapshot", "profile", baseline.profile, candidate.profile),
  );

  comparisons.push(
    compareBooleanMetric(
      "transport",
      "aggregate.all_checks_passed",
      baseline.transport.aggregate.all_checks_passed,
      candidate.transport.aggregate.all_checks_passed,
    ),
    compareNumberMetric(
      "transport",
      "aggregate.total_full_snapshot_bytes",
      baseline.transport.aggregate.total_full_snapshot_bytes,
      candidate.transport.aggregate.total_full_snapshot_bytes,
      "lower_is_better",
    ),
    compareNumberMetric(
      "transport",
      "aggregate.total_recovery_snapshot_bytes",
      baseline.transport.aggregate.total_recovery_snapshot_bytes,
      candidate.transport.aggregate.total_recovery_snapshot_bytes,
      "lower_is_better",
    ),
    compareNumberMetric(
      "transport",
      "aggregate.total_delta_bytes",
      baseline.transport.aggregate.total_delta_bytes,
      candidate.transport.aggregate.total_delta_bytes,
      "lower_is_better",
    ),
    compareNumberMetric(
      "transport",
      "aggregate.total_queue_pressure_events",
      baseline.transport.aggregate.total_queue_pressure_events,
      candidate.transport.aggregate.total_queue_pressure_events,
      "lower_is_better",
    ),
    compareNumberMetric(
      "transport",
      "aggregate.total_timed_out_clients",
      baseline.transport.aggregate.total_timed_out_clients,
      candidate.transport.aggregate.total_timed_out_clients,
      "lower_is_better",
    ),
    compareNumberMetric(
      "transport",
      "aggregate.total_recovery_delivery_failures",
      baseline.transport.aggregate.total_recovery_delivery_failures,
      candidate.transport.aggregate.total_recovery_delivery_failures,
      "lower_is_better",
    ),
    compareNumberMetric(
      "transport",
      "aggregate.scenarios_passed",
      baseline.transport.aggregate.scenarios_passed,
      candidate.transport.aggregate.scenarios_passed,
      "higher_is_better",
    ),
  );

  const scenarioNames = new Set([
    ...baseline.transport.scenarios.map((scenario) => scenario.name),
    ...candidate.transport.scenarios.map((scenario) => scenario.name),
  ]);
  for (const scenarioName of scenarioNames) {
    const baselineScenario = baseline.transport.scenarios.find(
      (scenario) => scenario.name === scenarioName,
    );
    const candidateScenario = candidate.transport.scenarios.find(
      (scenario) => scenario.name === scenarioName,
    );
    const category = `transport.${scenarioName}`;
    comparisons.push(
      compareInformationalMetric(
        category,
        "present",
        Boolean(baselineScenario),
        Boolean(candidateScenario),
      ),
    );
    if (!baselineScenario || !candidateScenario) {
      continue;
    }
    comparisons.push(
      compareBooleanMetric(
        category,
        "all_checks_passed",
        baselineScenario.all_checks_passed,
        candidateScenario.all_checks_passed,
      ),
      compareNumberMetric(
        category,
        "summary.total_delta_bytes",
        baselineScenario.summary.total_delta_bytes,
        candidateScenario.summary.total_delta_bytes,
        "lower_is_better",
      ),
      compareNumberMetric(
        category,
        "summary.total_recovery_snapshot_bytes",
        baselineScenario.summary.total_recovery_snapshot_bytes,
        candidateScenario.summary.total_recovery_snapshot_bytes,
        "lower_is_better",
      ),
      compareNumberMetric(
        category,
        "summary.peak_pending_action_queue_depth",
        baselineScenario.summary.peak_pending_action_queue_depth,
        candidateScenario.summary.peak_pending_action_queue_depth,
        "lower_is_better",
      ),
    );
  }

  pushRouteComparisons(comparisons, baseline, candidate);

  comparisons.push(
    compareBooleanMetric(
      "headlessTopology",
      "allChecksPassed",
      baseline.headlessTopology.allChecksPassed,
      candidate.headlessTopology.allChecksPassed,
    ),
    compareBooleanMetric(
      "headlessTopology",
      "topologyParity.consistent",
      baseline.headlessTopology.topologyParity.consistent,
      candidate.headlessTopology.topologyParity.consistent,
    ),
    compareBooleanMetric(
      "headlessTopology",
      "topologyParity.world_quest_bindings_match",
      baseline.headlessTopology.topologyParity.world_quest_bindings_match,
      candidate.headlessTopology.topologyParity.world_quest_bindings_match,
    ),
    compareBooleanMetric(
      "headlessTopology",
      "topologyParity.world_admissions_match",
      baseline.headlessTopology.topologyParity.world_admissions_match ?? true,
      candidate.headlessTopology.topologyParity.world_admissions_match ?? true,
    ),
    compareBooleanMetric(
      "headlessTopology",
      "topologyParity.world_control_planes_match",
      baseline.headlessTopology.topologyParity.world_control_planes_match ?? true,
      candidate.headlessTopology.topologyParity.world_control_planes_match ?? true,
    ),
    compareBooleanMetric(
      "headlessTopology",
      "topologyParity.applied_world_states_match",
      baseline.headlessTopology.topologyParity.applied_world_states_match,
      candidate.headlessTopology.topologyParity.applied_world_states_match,
    ),
    compareBooleanMetric(
      "headlessTopology",
      "topologyParity.evaluation_match",
      baseline.headlessTopology.topologyParity.evaluation_match,
      candidate.headlessTopology.topologyParity.evaluation_match,
    ),
    compareBooleanMetric(
      "headlessTopology",
      "topologyParity.tournament_control_plane_match",
      baseline.headlessTopology.topologyParity.tournament_control_plane_match ??
        true,
      candidate.headlessTopology.topologyParity.tournament_control_plane_match ??
        true,
    ),
    compareBooleanMetric(
      "headlessTopology",
      "topologyParity.tournament_orchestration_match",
      baseline.headlessTopology.topologyParity.tournament_orchestration_match ??
        true,
      candidate.headlessTopology.topologyParity.tournament_orchestration_match ??
        true,
    ),
    compareInformationalMetric(
      "headlessTopology",
      "worldCount",
      baseline.headlessTopology.worldCount,
      candidate.headlessTopology.worldCount,
    ),
    compareInformationalMetric(
      "headlessTopology",
      "linkCount",
      baseline.headlessTopology.linkCount,
      candidate.headlessTopology.linkCount,
    ),
    compareExactBaselineMetric(
      "headlessTopology.tournamentOrchestration",
      "phase",
      baselineTournamentOrchestration.phase,
      candidateTournamentOrchestration.phase,
    ),
    compareNumberEnvelopeMetric(
      "headlessTopology.tournamentOrchestration",
      "activeWorldCount",
      baselineTournamentOrchestration.activeWorldCount,
      candidateTournamentOrchestration.activeWorldCount,
      0,
      0,
    ),
    compareNumberEnvelopeMetric(
      "headlessTopology.tournamentOrchestration",
      "contestedWorldCount",
      baselineTournamentOrchestration.contestedWorldCount,
      candidateTournamentOrchestration.contestedWorldCount,
      0,
      0,
    ),
    compareNumberEnvelopeMetric(
      "headlessTopology.tournamentOrchestration",
      "activeLinkCount",
      baselineTournamentOrchestration.activeLinkCount,
      candidateTournamentOrchestration.activeLinkCount,
      0,
      0,
    ),
    compareNumberEnvelopeMetric(
      "headlessTopology.tournamentOrchestration",
      "leadingTeamCount",
      baselineTournamentOrchestration.leadingTeamCount,
      candidateTournamentOrchestration.leadingTeamCount,
      0,
      0,
    ),
    compareNumberEnvelopeMetric(
      "headlessTopology.tournamentOrchestration",
      "atRiskTeamCount",
      baselineTournamentOrchestration.atRiskTeamCount,
      candidateTournamentOrchestration.atRiskTeamCount,
      0,
      0,
    ),
    compareNumberEnvelopeMetric(
      "headlessTopology.tournamentOrchestration",
      "pressureWorldCount",
      baselineTournamentOrchestration.pressureWorldCount,
      candidateTournamentOrchestration.pressureWorldCount,
      0,
      0,
    ),
    compareNumberEnvelopeMetric(
      "headlessTopology.tournamentOrchestration",
      "neuralSwarmWorldCount",
      baselineTournamentOrchestration.neuralSwarmWorldCount,
      candidateTournamentOrchestration.neuralSwarmWorldCount,
      0,
      0,
    ),
  );

  comparisons.push(
    compareInformationalMetric(
      "topologyFeed",
      "worldCount",
      baseline.topologyFeed.worldCount,
      candidate.topologyFeed.worldCount,
    ),
  );
  pushTopologyWorldComparisons(
    comparisons,
    "topologyFeed",
    baseline.topologyFeed.worlds,
    candidate.topologyFeed.worlds,
  );

  comparisons.push(
    compareInformationalMetric(
      "liveTopologyFeed",
      "present",
      Boolean(baseline.liveTopologyFeed),
      Boolean(candidate.liveTopologyFeed),
    ),
  );
  if (baseline.liveTopologyFeed && candidate.liveTopologyFeed) {
    comparisons.push(
      compareInformationalMetric(
        "liveTopologyFeed",
        "worldCount",
        baseline.liveTopologyFeed.worldCount,
        candidate.liveTopologyFeed.worldCount,
      ),
    );
    pushTopologyWorldComparisons(
      comparisons,
      "liveTopologyFeed",
      baseline.liveTopologyFeed.worlds,
      candidate.liveTopologyFeed.worlds,
    );
  }

  const summary = comparisons.reduce(
    (acc, comparison) => {
      acc.comparedMetrics += 1;
      if (comparison.status === "regressed") {
        acc.regressions += 1;
      } else if (comparison.status === "improved") {
        acc.improvements += 1;
      } else if (comparison.status === "changed") {
        acc.changed += 1;
      } else {
        acc.unchanged += 1;
      }
      return acc;
    },
    {
      regressions: 0,
      improvements: 0,
      changed: 0,
      unchanged: 0,
      comparedMetrics: 0,
    },
  );

  return {
    schemaVersion: 2,
    profile: "shard-target",
    baselineLabel: baseline.label,
    candidateLabel: candidate.label,
    baselinePath,
    candidatePath,
    summary,
    comparisons,
  };
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const repoRoot = resolve(import.meta.dir, "..");
  const baselinePath = resolve(repoRoot, options.baseline);
  const candidatePath = resolve(repoRoot, options.candidate);
  const outputPath = resolve(repoRoot, options.output);

  const report = buildSnapshotComparisonReport(
    readSnapshot(baselinePath),
    readSnapshot(candidatePath),
    baselinePath,
    candidatePath,
  );

  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`);

  console.error(
    `Compared ${report.baselineLabel} -> ${report.candidateLabel}: ${report.summary.regressions} regressions, ${report.summary.improvements} improvements, ${report.summary.changed} changed, ${report.summary.unchanged} unchanged`,
  );
  console.error(`Wrote ${outputPath}`);

  if (options.failOnRegressions && report.summary.regressions > 0) {
    process.exit(1);
  }
}

if (import.meta.main) {
  try {
    main();
  } catch (error) {
    console.error(
      error instanceof Error ? error.message : String(error),
    );
    process.exit(1);
  }
}
