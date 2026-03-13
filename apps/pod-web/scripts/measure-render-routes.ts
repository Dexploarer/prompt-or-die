#!/usr/bin/env bun

import { chromium } from "@playwright/test";
import { resolve } from "node:path";
import {
  AVERAGE_GEOMETRY_LOAD_MS_CEILING,
  AVERAGE_SPRITE_LOAD_MS_CEILING,
  MAIN_STABLE_FRAME_PERCENT_FLOOR,
  MIN_COMPLETED_ASSET_LOADS,
  SLOWEST_GEOMETRY_LOAD_MS_CEILING,
  SLOWEST_SPRITE_LOAD_MS_CEILING,
  WORKER_CONTROL_SUBMISSION_CEILING,
  WORKER_RESIZE_SUBMISSION_CEILING,
  WORKER_STABLE_FRAME_PERCENT_FLOOR,
} from "../src/render-runtime-gates";

const ROUTE_WAIT_TIMEOUT_MS = 90_000;

type RouteTarget = {
  label: "main" | "worker";
  url: string;
};

type GameplayState = {
  frameSource: string;
  controlledPosition: [number, number] | null;
};

type RendererStats = {
  renderThread: string;
  requestedRenderThread: string;
  renderThreadFallbackReason: string | null;
  geometryLoadsCompleted: number;
  spriteLoadsCompleted: number;
  pendingGeometryAssets: number;
  pendingSpriteAssets: number;
  averageGeometryLoadMs: number;
  averageSpriteLoadMs: number;
  slowestGeometryLoadMs: number;
  slowestSpriteLoadMs: number;
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
};

export type RenderRouteAssetLoadPerf = {
  geometryLoadsCompleted: number;
  spriteLoadsCompleted: number;
  averageGeometryLoadMs: number;
  averageSpriteLoadMs: number;
  slowestGeometryLoadMs: number;
  slowestSpriteLoadMs: number;
};

type WindowWithPodRender = Window & {
  advanceTime: (ms: number) => Promise<void>;
  podRender: {
    requestGameplayFocus: () => boolean;
    resetPerfMetrics: () => void | Promise<void>;
    getGameplayState: () => GameplayState;
    getStats: () => RendererStats;
  };
};

export type RenderRouteMeasurement = {
  label: RouteTarget["label"];
  url: string;
  renderThread: string;
  requestedRenderThread: string;
  renderThreadFallbackReason: string | null;
  loadsCompleted: number;
  pendingAssets: number;
  assetLoadPerf: RenderRouteAssetLoadPerf;
  mainThreadPerf: RendererStats["mainThreadPerf"];
  runtimePerf: RendererStats["runtimePerf"];
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

export type RenderRouteComparison = {
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
};

export type RenderRouteMeasurementReport = {
  schemaVersion: number;
  generatedAtUnixMs: number;
  baseUrl: string;
  routes: RenderRouteMeasurement[];
  comparison: RenderRouteComparison | null;
};

type Options = {
  baseUrl: string;
  output: string;
  failOnGates: boolean;
};

const DEFAULT_BASE_URL = "http://127.0.0.1:4178";
const DEFAULT_OUTPUT = "artifacts/render-route-measurements.json";
const DEFAULT_ROUTE_TARGETS: RouteTarget[] = [
  {
    label: "main",
    url: "/?world=local-sandbox&backend=webgl2",
  },
  {
    label: "worker",
    url: "/?world=local-sandbox&renderThread=worker&backend=webgl2",
  },
];

function parseArgs(argv: string[]): Options {
  const options: Options = {
    baseUrl: DEFAULT_BASE_URL,
    output: DEFAULT_OUTPUT,
    failOnGates: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--base-url": {
        const value = argv[index + 1];
        if (!value) {
          throw new Error("missing value for --base-url");
        }
        options.baseUrl = value;
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
      case "--fail-on-gates":
        options.failOnGates = true;
        break;
      case "--help":
      case "-h":
        console.error(
          "Usage: bun run scripts/measure-render-routes.ts [--base-url URL] [--output PATH] [--fail-on-gates]",
        );
        process.exit(0);
      default:
        throw new Error(`unknown argument: ${current}`);
    }
  }

  return options;
}

function roundMetric(value: number): number {
  return Number(value.toFixed(1));
}

function computeStableFramePercentFloor(label: RouteTarget["label"]): number {
  return label === "main"
    ? MAIN_STABLE_FRAME_PERCENT_FLOOR
    : WORKER_STABLE_FRAME_PERCENT_FLOOR;
}

export function buildRenderRouteMeasurement(
  label: RouteTarget["label"],
  url: string,
  stats: RendererStats,
): RenderRouteMeasurement {
  const stableFramePercentFloor = computeStableFramePercentFloor(label);
  const loadsCompleted = stats.geometryLoadsCompleted + stats.spriteLoadsCompleted;
  const pendingAssets = stats.pendingGeometryAssets + stats.pendingSpriteAssets;
  const controlCeiling = label === "worker" ? WORKER_CONTROL_SUBMISSION_CEILING : null;
  const resizeCeiling = label === "worker" ? WORKER_RESIZE_SUBMISSION_CEILING : null;
  const assetLoadPerf = {
    geometryLoadsCompleted: stats.geometryLoadsCompleted,
    spriteLoadsCompleted: stats.spriteLoadsCompleted,
    averageGeometryLoadMs: stats.averageGeometryLoadMs,
    averageSpriteLoadMs: stats.averageSpriteLoadMs,
    slowestGeometryLoadMs: stats.slowestGeometryLoadMs,
    slowestSpriteLoadMs: stats.slowestSpriteLoadMs,
  };

  return {
    label,
    url,
    renderThread: stats.renderThread,
    requestedRenderThread: stats.requestedRenderThread,
    renderThreadFallbackReason: stats.renderThreadFallbackReason,
    loadsCompleted,
    pendingAssets,
    assetLoadPerf,
    mainThreadPerf: stats.mainThreadPerf,
    runtimePerf: stats.runtimePerf,
    gates: {
      stableFramePercentFloor,
      stableFramePercentFloorPassed:
        stats.runtimePerf.stableFramePercent >= stableFramePercentFloor,
      completedAssetLoadsFloor: MIN_COMPLETED_ASSET_LOADS,
      completedAssetLoadsFloorPassed: loadsCompleted >= MIN_COMPLETED_ASSET_LOADS,
      averageGeometryLoadMsCeiling: AVERAGE_GEOMETRY_LOAD_MS_CEILING,
      averageGeometryLoadMsCeilingPassed:
        stats.averageGeometryLoadMs <= AVERAGE_GEOMETRY_LOAD_MS_CEILING,
      averageSpriteLoadMsCeiling: AVERAGE_SPRITE_LOAD_MS_CEILING,
      averageSpriteLoadMsCeilingPassed:
        stats.averageSpriteLoadMs <= AVERAGE_SPRITE_LOAD_MS_CEILING,
      slowestGeometryLoadMsCeiling: SLOWEST_GEOMETRY_LOAD_MS_CEILING,
      slowestGeometryLoadMsCeilingPassed:
        stats.slowestGeometryLoadMs <= SLOWEST_GEOMETRY_LOAD_MS_CEILING,
      slowestSpriteLoadMsCeiling: SLOWEST_SPRITE_LOAD_MS_CEILING,
      slowestSpriteLoadMsCeilingPassed:
        stats.slowestSpriteLoadMs <= SLOWEST_SPRITE_LOAD_MS_CEILING,
      controlSubmissionCeiling: controlCeiling,
      controlSubmissionCeilingPassed:
        controlCeiling == null
          ? null
          : stats.mainThreadPerf.byKind.control.submissionsCompleted <= controlCeiling,
      resizeSubmissionCeiling: resizeCeiling,
      resizeSubmissionCeilingPassed:
        resizeCeiling == null
          ? null
          : stats.mainThreadPerf.byKind.resize.submissionsCompleted <= resizeCeiling,
    },
  };
}

export function buildRenderRouteComparison(
  routes: RenderRouteMeasurement[],
): RenderRouteComparison | null {
  const mainRoute = routes.find((route) => route.label === "main");
  const workerRoute = routes.find((route) => route.label === "worker");
  if (!mainRoute || !workerRoute) {
    return null;
  }

  const mainFrameSubmissions = mainRoute.mainThreadPerf.byKind.frame.submissionsCompleted;
  const workerFrameSubmissions = workerRoute.mainThreadPerf.byKind.frame.submissionsCompleted;
  const frameSubmissionReductionPercent =
    mainFrameSubmissions === 0
      ? 0
      : roundMetric(
          ((mainFrameSubmissions - workerFrameSubmissions) / mainFrameSubmissions) * 100,
        );

  return {
    mainFrameSubmissions,
    workerFrameSubmissions,
    frameSubmissionReductionPercent,
    mainStableFramePercent: mainRoute.runtimePerf.stableFramePercent,
    workerStableFramePercent: workerRoute.runtimePerf.stableFramePercent,
    stableFramePercentDelta: roundMetric(
      workerRoute.runtimePerf.stableFramePercent - mainRoute.runtimePerf.stableFramePercent,
    ),
    mainSlowFrames: mainRoute.runtimePerf.slowFrames,
    workerSlowFrames: workerRoute.runtimePerf.slowFrames,
    slowFrameDelta: workerRoute.runtimePerf.slowFrames - mainRoute.runtimePerf.slowFrames,
    workerGatesPassed:
      workerRoute.gates.controlSubmissionCeilingPassed === true &&
      workerRoute.gates.resizeSubmissionCeilingPassed === true,
  };
}

export function buildRenderRouteMeasurementReport(
  baseUrl: string,
  routes: RenderRouteMeasurement[],
  generatedAtUnixMs = Date.now(),
): RenderRouteMeasurementReport {
  return {
    schemaVersion: 2,
    generatedAtUnixMs,
    baseUrl,
    routes,
    comparison: buildRenderRouteComparison(routes),
  };
}

export function collectRenderRouteMeasurementFailures(
  report: RenderRouteMeasurementReport,
): string[] {
  const failures: string[] = [];

  for (const route of report.routes) {
    if (!route.gates.completedAssetLoadsFloorPassed) {
      failures.push(
        `${route.label} route completed only ${route.loadsCompleted} asset loads; expected at least ${route.gates.completedAssetLoadsFloor}`,
      );
    }
    if (route.gates.controlSubmissionCeilingPassed === false) {
      failures.push(
        `${route.label} route control submissions exceeded ${route.gates.controlSubmissionCeiling}`,
      );
    }
    if (route.gates.resizeSubmissionCeilingPassed === false) {
      failures.push(
        `${route.label} route resize submissions exceeded ${route.gates.resizeSubmissionCeiling}`,
      );
    }
  }

  return failures;
}

export function assertRenderRouteMeasurementReportGates(
  report: RenderRouteMeasurementReport,
): void {
  const failures = collectRenderRouteMeasurementFailures(report);
  if (failures.length > 0) {
    throw new Error(failures.join("\n"));
  }
}

async function isServerReady(baseUrl: string): Promise<boolean> {
  try {
    const response = await fetch(baseUrl);
    return response.ok;
  } catch {
    return false;
  }
}

async function waitForServer(baseUrl: string, timeoutMs: number): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await isServerReady(baseUrl)) {
      return;
    }
    await Bun.sleep(250);
  }
  throw new Error(`timed out waiting for dev server at ${baseUrl}`);
}

async function startServerIfNeeded(
  appRoot: string,
  baseUrl: string,
): Promise<ReturnType<typeof Bun.spawn> | null> {
  if (await isServerReady(baseUrl)) {
    return null;
  }

  const viteBinary = resolve(appRoot, "node_modules/.bin/vite");
  const serverProcess = Bun.spawn(
    [viteBinary, "--host", "127.0.0.1", "--port", "4178"],
    {
      cwd: appRoot,
      stdout: "ignore",
      stderr: "ignore",
      env: process.env,
    },
  );
  await waitForServer(baseUrl, 30_000);
  return serverProcess;
}

async function waitForGameplayReady(page: import("@playwright/test").Page): Promise<void> {
  await page.waitForFunction(() => {
    const runtime = window as WindowWithPodRender;
    return (
      typeof runtime.podRender?.getGameplayState === "function" &&
      typeof runtime.podRender?.getStats === "function" &&
      runtime.podRender.getGameplayState().controlledPosition !== null &&
      runtime.podRender.getGameplayState().frameSource === "threejs" &&
      runtime.podRender.getStats().runtimePerf.framesRendered > 0
    );
  }, undefined, { timeout: ROUTE_WAIT_TIMEOUT_MS });
}

async function waitForRuntimeAssets(page: import("@playwright/test").Page): Promise<void> {
  await page.waitForFunction((minimumCompletedAssetLoads) => {
    const runtime = window as WindowWithPodRender;
    const stats = runtime.podRender.getStats();
    return (
      stats.pendingGeometryAssets + stats.pendingSpriteAssets === 0 &&
      stats.geometryLoadsCompleted + stats.spriteLoadsCompleted >= minimumCompletedAssetLoads
    );
  }, MIN_COMPLETED_ASSET_LOADS, { timeout: ROUTE_WAIT_TIMEOUT_MS });
}

async function advanceInBursts(
  page: import("@playwright/test").Page,
  durationsMs: number[],
): Promise<void> {
  for (const durationMs of durationsMs) {
    const framesBefore = await page.evaluate(
      () => (window as WindowWithPodRender).podRender.getStats().runtimePerf.framesRendered,
    );
    await page.evaluate(
      (stepMs) => (window as WindowWithPodRender).advanceTime(stepMs),
      durationMs,
    );
    await page.waitForFunction(
      (expectedFrames) =>
        (window as WindowWithPodRender).podRender.getStats().runtimePerf.framesRendered >
        expectedFrames,
      framesBefore,
      { timeout: ROUTE_WAIT_TIMEOUT_MS },
    );
  }
}

async function collectRouteMeasurement(
  browser: import("@playwright/test").Browser,
  baseUrl: string,
  routeTarget: RouteTarget,
): Promise<RenderRouteMeasurement> {
  const page = await browser.newPage({
    viewport: {
      width: 1440,
      height: 900,
    },
  });
  const url = `${baseUrl}${routeTarget.url}`;

  try {
    await page.goto(url);
    await waitForGameplayReady(page);
    await waitForRuntimeAssets(page);
    await page.evaluate(() => (window as WindowWithPodRender).podRender.resetPerfMetrics());
    await page.evaluate(() => (window as WindowWithPodRender).podRender.requestGameplayFocus());
    await page.keyboard.down("w");
    await advanceInBursts(page, [100, 100, 100, 100, 100, 100, 100]);
    await page.keyboard.up("w");
    await advanceInBursts(page, [100, 100, 100, 100]);
    const stats = await page.evaluate(() => (window as WindowWithPodRender).podRender.getStats());
    return buildRenderRouteMeasurement(routeTarget.label, url, stats);
  } finally {
    await page.close();
  }
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const appRoot = resolve(import.meta.dir, "..");
  const outputPath = resolve(appRoot, options.output);
  const serverProcess = await startServerIfNeeded(appRoot, options.baseUrl);

  try {
    const browser = await chromium.launch({
      headless: true,
      args: ["--use-angle=swiftshader", "--use-gl=angle", "--enable-unsafe-webgpu"],
    });
    try {
      const routes: RenderRouteMeasurement[] = [];
      for (const routeTarget of DEFAULT_ROUTE_TARGETS) {
        routes.push(await collectRouteMeasurement(browser, options.baseUrl, routeTarget));
      }
      const report = buildRenderRouteMeasurementReport(options.baseUrl, routes);
      await Bun.write(outputPath, JSON.stringify(report, null, 2));
      if (options.failOnGates) {
        assertRenderRouteMeasurementReportGates(report);
      }
      console.log(JSON.stringify(report, null, 2));
    } finally {
      await browser.close();
    }
  } finally {
    if (serverProcess) {
      serverProcess.kill();
      await serverProcess.exited;
    }
  }
}

if (import.meta.main) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
