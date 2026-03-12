#!/usr/bin/env bun

import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

type TransportCheck = {
  metric: string;
  passed: boolean;
  expected: string;
  observed: string;
};

type TransportScenarioSummary = {
  latest_tick: number;
  client_count: number;
  resumed_sessions: number;
  recovery_snapshots_sent: number;
  recovery_delivery_failures: number;
  total_pending_action_queue_depth: number;
  peak_pending_action_queue_depth: number;
  queue_pressure_client_count: number;
  total_inbound_messages: number;
  total_outbound_messages: number;
  total_inbound_bytes: number;
  total_outbound_bytes: number;
  action_batches_received: number;
  full_snapshots_sent: number;
  total_full_snapshot_bytes: number;
  max_full_snapshot_bytes: number;
  total_recovery_snapshot_bytes: number;
  full_snapshot_requests: number;
  state_deltas_sent: number;
  delta_messages_sent: number;
  total_delta_bytes: number;
  max_delta_bytes: number;
  total_delta_entities_updated: number;
  total_delta_entities_destroyed: number;
  timed_out_clients: number;
  queue_pressure_events: number;
};

type TransportScenario = {
  name: string;
  description: string;
  all_checks_passed: boolean;
  summary: TransportScenarioSummary;
  checks: TransportCheck[];
};

type TransportAggregate = {
  all_checks_passed: boolean;
  scenarios_passed: number;
  scenario_count: number;
  published_baseline_profile: string | null;
  checks: TransportCheck[];
  total_full_snapshot_bytes: number;
  total_recovery_snapshot_bytes: number;
  total_delta_bytes: number;
  total_delta_entities_updated: number;
  total_delta_entities_destroyed: number;
  total_queue_pressure_events: number;
  total_resumed_sessions: number;
  total_timed_out_clients: number;
  total_recovery_delivery_failures: number;
};

type TransportMeasurementsReport = {
  schema_version: number;
  profile: string;
  scenarios: TransportScenario[];
  aggregate: TransportAggregate;
};

type BrowserRouteMeasurement = {
  label: "main" | "worker";
  url: string;
  renderThread: string;
  requestedRenderThread: string;
  renderThreadFallbackReason: string | null;
  loadsCompleted: number;
  pendingAssets: number;
  assetLoadPerf: {
    geometryLoadsCompleted: number;
    spriteLoadsCompleted: number;
    averageGeometryLoadMs: number;
    averageSpriteLoadMs: number;
    slowestGeometryLoadMs: number;
    slowestSpriteLoadMs: number;
  };
  mainThreadPerf: {
    warmupMs: number | null;
    submissionsCompleted: number;
    averageSubmissionMs: number;
    slowestSubmissionMs: number;
    byKind: {
      frame: {
        submissionsCompleted: number;
        averageSubmissionMs: number;
        slowestSubmissionMs: number;
      };
      control: {
        submissionsCompleted: number;
        averageSubmissionMs: number;
        slowestSubmissionMs: number;
      };
      resize: {
        submissionsCompleted: number;
        averageSubmissionMs: number;
        slowestSubmissionMs: number;
      };
    };
  };
  runtimePerf: {
    warmupMs: number | null;
    frameBudgetMs: number;
    framesRendered: number;
    stableFrames: number;
    slowFrames: number;
    stableFramePercent: number;
    slowestFrameMs: number;
  };
  gates: {
    stableFramePercentFloor: number;
    stableFramePercentFloorPassed: boolean;
    completedAssetLoadsFloor: number;
    completedAssetLoadsFloorPassed: boolean;
    averageGeometryLoadMsCeiling: number;
    averageGeometryLoadMsCeilingPassed: boolean;
    averageSpriteLoadMsCeiling: number;
    averageSpriteLoadMsCeilingPassed: boolean;
    slowestGeometryLoadMsCeiling: number;
    slowestGeometryLoadMsCeilingPassed: boolean;
    slowestSpriteLoadMsCeiling: number;
    slowestSpriteLoadMsCeilingPassed: boolean;
    controlSubmissionCeiling: number | null;
    controlSubmissionCeilingPassed: boolean | null;
    resizeSubmissionCeiling: number | null;
    resizeSubmissionCeilingPassed: boolean | null;
  };
};

type BrowserRouteMeasurementsReport = {
  schemaVersion: number;
  routes: BrowserRouteMeasurement[];
  comparison: {
    mainFrameSubmissions: number;
    workerFrameSubmissions: number;
    frameSubmissionReductionPercent: number;
    mainStableFramePercent: number;
    workerStableFramePercent: number;
    stableFramePercentDelta: number;
    mainSlowFrames: number;
    workerSlowFrames: number;
    slowFrameDelta: number;
    workerGatesPassed: boolean;
  } | null;
};

type CombinedReport = {
  profile: string;
  transportMeasurements: TransportMeasurementsReport | null;
  browserRouteMeasurements: BrowserRouteMeasurementsReport | null;
};

export type PublishedMoatSnapshot = {
  schemaVersion: 1;
  label: string;
  profile: "shard-target";
  transport: {
    sourceSchemaVersion: number;
    aggregate: TransportAggregate;
    scenarios: TransportScenario[];
  };
  browserRoutes: {
    sourceSchemaVersion: number;
    routes: Array<
      Omit<BrowserRouteMeasurement, "url"> & {
        routePath: string;
      }
    >;
    comparison: BrowserRouteMeasurementsReport["comparison"];
  };
};

type Options = {
  input: string;
  output: string;
  label: string;
};

const DEFAULT_INPUT = "artifacts/moat-benchmarks-shard-local.json";
const DEFAULT_OUTPUT_ROOT = "docs/benchmark-snapshots";

function roundMetric(value: number | null): number | null {
  if (value == null) {
    return null;
  }
  return Number(value.toFixed(1));
}

function defaultSnapshotLabel(now = new Date()): string {
  const year = now.getFullYear();
  const month = String(now.getMonth() + 1).padStart(2, "0");
  return `${year}-${month}`;
}

export function buildDefaultSnapshotOutputPath(label: string): string {
  return `${DEFAULT_OUTPUT_ROOT}/${label}-shard-target.json`;
}

function parseArgs(argv: string[]): Options {
  const label = defaultSnapshotLabel();
  const options: Options = {
    input: DEFAULT_INPUT,
    output: buildDefaultSnapshotOutputPath(label),
    label,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--input": {
        const value = argv[index + 1];
        if (!value) {
          throw new Error("missing value for --input");
        }
        options.input = value;
        index += 1;
        break;
      }
      case "--label": {
        const value = argv[index + 1];
        if (!value) {
          throw new Error("missing value for --label");
        }
        options.label = value;
        options.output = buildDefaultSnapshotOutputPath(value);
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
      case "--help":
      case "-h":
        console.error(
          "Usage: bun ./scripts/publish_moat_snapshots.ts [--input PATH] [--label YYYY-MM] [--output PATH]",
        );
        process.exit(0);
      default:
        throw new Error(`unknown argument: ${current}`);
    }
  }

  return options;
}

function normalizeRoutePath(url: string): string {
  try {
    const parsed = new URL(url);
    return `${parsed.pathname}${parsed.search}`;
  } catch {
    return url;
  }
}

function normalizeTransportMeasurements(
  report: TransportMeasurementsReport,
): PublishedMoatSnapshot["transport"] {
  return {
    sourceSchemaVersion: report.schema_version,
    aggregate: {
      ...report.aggregate,
      checks: report.aggregate.checks.map((check) => ({ ...check })),
    },
    scenarios: report.scenarios.map((scenario) => ({
      name: scenario.name,
      description: scenario.description,
      all_checks_passed: scenario.all_checks_passed,
      summary: {
        latest_tick: scenario.summary.latest_tick,
        client_count: scenario.summary.client_count,
        resumed_sessions: scenario.summary.resumed_sessions,
        recovery_snapshots_sent: scenario.summary.recovery_snapshots_sent,
        recovery_delivery_failures: scenario.summary.recovery_delivery_failures,
        total_pending_action_queue_depth:
          scenario.summary.total_pending_action_queue_depth,
        peak_pending_action_queue_depth:
          scenario.summary.peak_pending_action_queue_depth,
        queue_pressure_client_count: scenario.summary.queue_pressure_client_count,
        total_inbound_messages: scenario.summary.total_inbound_messages,
        total_outbound_messages: scenario.summary.total_outbound_messages,
        total_inbound_bytes: scenario.summary.total_inbound_bytes,
        total_outbound_bytes: scenario.summary.total_outbound_bytes,
        action_batches_received: scenario.summary.action_batches_received,
        full_snapshots_sent: scenario.summary.full_snapshots_sent,
        total_full_snapshot_bytes: scenario.summary.total_full_snapshot_bytes,
        max_full_snapshot_bytes: scenario.summary.max_full_snapshot_bytes,
        total_recovery_snapshot_bytes:
          scenario.summary.total_recovery_snapshot_bytes,
        full_snapshot_requests: scenario.summary.full_snapshot_requests,
        state_deltas_sent: scenario.summary.state_deltas_sent,
        delta_messages_sent: scenario.summary.delta_messages_sent,
        total_delta_bytes: scenario.summary.total_delta_bytes,
        max_delta_bytes: scenario.summary.max_delta_bytes,
        total_delta_entities_updated:
          scenario.summary.total_delta_entities_updated,
        total_delta_entities_destroyed:
          scenario.summary.total_delta_entities_destroyed,
        timed_out_clients: scenario.summary.timed_out_clients,
        queue_pressure_events: scenario.summary.queue_pressure_events,
      },
      checks: scenario.checks.map((check) => ({ ...check })),
    })),
  };
}

function normalizeBrowserRouteMeasurements(
  report: BrowserRouteMeasurementsReport,
): PublishedMoatSnapshot["browserRoutes"] {
  return {
    sourceSchemaVersion: report.schemaVersion,
    routes: report.routes.map((route) => ({
      label: route.label,
      routePath: normalizeRoutePath(route.url),
      renderThread: route.renderThread,
      requestedRenderThread: route.requestedRenderThread,
      renderThreadFallbackReason: route.renderThreadFallbackReason,
      loadsCompleted: route.loadsCompleted,
      pendingAssets: route.pendingAssets,
      assetLoadPerf: {
        geometryLoadsCompleted: route.assetLoadPerf.geometryLoadsCompleted,
        spriteLoadsCompleted: route.assetLoadPerf.spriteLoadsCompleted,
        averageGeometryLoadMs: roundMetric(
          route.assetLoadPerf.averageGeometryLoadMs,
        ) as number,
        averageSpriteLoadMs: roundMetric(
          route.assetLoadPerf.averageSpriteLoadMs,
        ) as number,
        slowestGeometryLoadMs: roundMetric(
          route.assetLoadPerf.slowestGeometryLoadMs,
        ) as number,
        slowestSpriteLoadMs: roundMetric(
          route.assetLoadPerf.slowestSpriteLoadMs,
        ) as number,
      },
      mainThreadPerf: {
        warmupMs: roundMetric(route.mainThreadPerf.warmupMs),
        submissionsCompleted: route.mainThreadPerf.submissionsCompleted,
        averageSubmissionMs: roundMetric(
          route.mainThreadPerf.averageSubmissionMs,
        ) as number,
        slowestSubmissionMs: roundMetric(
          route.mainThreadPerf.slowestSubmissionMs,
        ) as number,
        byKind: {
          frame: {
            submissionsCompleted:
              route.mainThreadPerf.byKind.frame.submissionsCompleted,
            averageSubmissionMs: roundMetric(
              route.mainThreadPerf.byKind.frame.averageSubmissionMs,
            ) as number,
            slowestSubmissionMs: roundMetric(
              route.mainThreadPerf.byKind.frame.slowestSubmissionMs,
            ) as number,
          },
          control: {
            submissionsCompleted:
              route.mainThreadPerf.byKind.control.submissionsCompleted,
            averageSubmissionMs: roundMetric(
              route.mainThreadPerf.byKind.control.averageSubmissionMs,
            ) as number,
            slowestSubmissionMs: roundMetric(
              route.mainThreadPerf.byKind.control.slowestSubmissionMs,
            ) as number,
          },
          resize: {
            submissionsCompleted:
              route.mainThreadPerf.byKind.resize.submissionsCompleted,
            averageSubmissionMs: roundMetric(
              route.mainThreadPerf.byKind.resize.averageSubmissionMs,
            ) as number,
            slowestSubmissionMs: roundMetric(
              route.mainThreadPerf.byKind.resize.slowestSubmissionMs,
            ) as number,
          },
        },
      },
      runtimePerf: {
        warmupMs: roundMetric(route.runtimePerf.warmupMs),
        frameBudgetMs: roundMetric(route.runtimePerf.frameBudgetMs) as number,
        framesRendered: route.runtimePerf.framesRendered,
        stableFrames: route.runtimePerf.stableFrames,
        slowFrames: route.runtimePerf.slowFrames,
        stableFramePercent: roundMetric(
          route.runtimePerf.stableFramePercent,
        ) as number,
        slowestFrameMs: roundMetric(route.runtimePerf.slowestFrameMs) as number,
      },
      gates: {
        stableFramePercentFloor: route.gates.stableFramePercentFloor,
        stableFramePercentFloorPassed: route.gates.stableFramePercentFloorPassed,
        completedAssetLoadsFloor: route.gates.completedAssetLoadsFloor,
        completedAssetLoadsFloorPassed:
          route.gates.completedAssetLoadsFloorPassed,
        averageGeometryLoadMsCeiling: route.gates.averageGeometryLoadMsCeiling,
        averageGeometryLoadMsCeilingPassed:
          route.gates.averageGeometryLoadMsCeilingPassed,
        averageSpriteLoadMsCeiling: route.gates.averageSpriteLoadMsCeiling,
        averageSpriteLoadMsCeilingPassed:
          route.gates.averageSpriteLoadMsCeilingPassed,
        slowestGeometryLoadMsCeiling: route.gates.slowestGeometryLoadMsCeiling,
        slowestGeometryLoadMsCeilingPassed:
          route.gates.slowestGeometryLoadMsCeilingPassed,
        slowestSpriteLoadMsCeiling: route.gates.slowestSpriteLoadMsCeiling,
        slowestSpriteLoadMsCeilingPassed:
          route.gates.slowestSpriteLoadMsCeilingPassed,
        controlSubmissionCeiling: route.gates.controlSubmissionCeiling,
        controlSubmissionCeilingPassed:
          route.gates.controlSubmissionCeilingPassed,
        resizeSubmissionCeiling: route.gates.resizeSubmissionCeiling,
        resizeSubmissionCeilingPassed:
          route.gates.resizeSubmissionCeilingPassed,
      },
    })),
    comparison: report.comparison
      ? {
          mainFrameSubmissions: report.comparison.mainFrameSubmissions,
          workerFrameSubmissions: report.comparison.workerFrameSubmissions,
          frameSubmissionReductionPercent: roundMetric(
            report.comparison.frameSubmissionReductionPercent,
          ) as number,
          mainStableFramePercent: roundMetric(
            report.comparison.mainStableFramePercent,
          ) as number,
          workerStableFramePercent: roundMetric(
            report.comparison.workerStableFramePercent,
          ) as number,
          stableFramePercentDelta: roundMetric(
            report.comparison.stableFramePercentDelta,
          ) as number,
          mainSlowFrames: report.comparison.mainSlowFrames,
          workerSlowFrames: report.comparison.workerSlowFrames,
          slowFrameDelta: report.comparison.slowFrameDelta,
          workerGatesPassed: report.comparison.workerGatesPassed,
        }
      : null,
  };
}

export function normalizeShardTargetMoatSnapshot(
  report: CombinedReport,
  label: string,
): PublishedMoatSnapshot {
  if (report.profile !== "shard-target") {
    throw new Error(
      `expected shard-target moat report, received '${report.profile}'`,
    );
  }
  if (!report.transportMeasurements) {
    throw new Error("missing transportMeasurements in moat report");
  }
  if (!report.browserRouteMeasurements) {
    throw new Error("missing browserRouteMeasurements in moat report");
  }

  return {
    schemaVersion: 1,
    label,
    profile: "shard-target",
    transport: normalizeTransportMeasurements(report.transportMeasurements),
    browserRoutes: normalizeBrowserRouteMeasurements(
      report.browserRouteMeasurements,
    ),
  };
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const repoRoot = resolve(import.meta.dir, "..");
  const inputPath = resolve(repoRoot, options.input);
  const outputPath = resolve(repoRoot, options.output);
  const report = JSON.parse(readFileSync(inputPath, "utf8")) as CombinedReport;
  const snapshot = normalizeShardTargetMoatSnapshot(report, options.label);
  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, JSON.stringify(snapshot, null, 2));
  console.log(JSON.stringify(snapshot, null, 2));
}

if (import.meta.main) {
  try {
    main();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}
