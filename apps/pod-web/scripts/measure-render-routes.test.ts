import { describe, expect, test } from "bun:test";

import {
  assertRenderRouteMeasurementReportGates,
  buildRenderRouteComparison,
  buildRenderRouteMeasurement,
  buildRenderRouteMeasurementReport,
  collectRenderRouteMeasurementFailures,
} from "./measure-render-routes";

function createStats(overrides?: Partial<ReturnType<typeof createStats>>) {
  const stats = {
    renderThread: "main",
    requestedRenderThread: "auto",
    renderThreadFallbackReason: null,
    geometryLoadsCompleted: 2,
    spriteLoadsCompleted: 1,
    pendingGeometryAssets: 0,
    pendingSpriteAssets: 0,
    averageGeometryLoadMs: 42,
    averageSpriteLoadMs: 18,
    slowestGeometryLoadMs: 120,
    slowestSpriteLoadMs: 44,
    mainThreadPerf: {
      warmupMs: 12,
      submissionsCompleted: 12,
      averageSubmissionMs: 0.5,
      slowestSubmissionMs: 0.8,
      byKind: {
        frame: {
          submissionsCompleted: 12,
          averageSubmissionMs: 0.5,
          slowestSubmissionMs: 0.8,
        },
        control: {
          submissionsCompleted: 0,
          averageSubmissionMs: 0,
          slowestSubmissionMs: 0,
        },
        resize: {
          submissionsCompleted: 0,
          averageSubmissionMs: 0,
          slowestSubmissionMs: 0,
        },
      },
    },
    runtimePerf: {
      warmupMs: 16,
      frameBudgetMs: 16.7,
      framesRendered: 8,
      stableFrames: 8,
      slowFrames: 0,
      stableFramePercent: 100,
      slowestFrameMs: 15.9,
    },
  };

  return {
    ...stats,
    ...overrides,
    mainThreadPerf: {
      ...stats.mainThreadPerf,
      ...overrides?.mainThreadPerf,
      byKind: {
        ...stats.mainThreadPerf.byKind,
        ...overrides?.mainThreadPerf?.byKind,
        frame: {
          ...stats.mainThreadPerf.byKind.frame,
          ...overrides?.mainThreadPerf?.byKind?.frame,
        },
        control: {
          ...stats.mainThreadPerf.byKind.control,
          ...overrides?.mainThreadPerf?.byKind?.control,
        },
        resize: {
          ...stats.mainThreadPerf.byKind.resize,
          ...overrides?.mainThreadPerf?.byKind?.resize,
        },
      },
    },
    runtimePerf: {
      ...stats.runtimePerf,
      ...overrides?.runtimePerf,
    },
  };
}

describe("measure render routes", () => {
  test("projects worker route gates into the measurement surface", () => {
    const measurement = buildRenderRouteMeasurement(
      "worker",
      "http://127.0.0.1:4178/?world=local-sandbox&renderThread=worker&backend=webgl2",
      createStats({
        renderThread: "worker",
        requestedRenderThread: "worker",
        geometryLoadsCompleted: 8,
        spriteLoadsCompleted: 4,
        mainThreadPerf: {
          submissionsCompleted: 5,
          byKind: {
            frame: {
              submissionsCompleted: 5,
            },
          },
        },
        runtimePerf: {
          stableFrames: 6,
          slowFrames: 2,
          stableFramePercent: 75,
        },
      }),
    );

    expect(measurement.loadsCompleted).toBe(12);
    expect(measurement.pendingAssets).toBe(0);
    expect(measurement.assetLoadPerf.averageGeometryLoadMs).toBe(42);
    expect(measurement.gates.completedAssetLoadsFloor).toBe(10);
    expect(measurement.gates.completedAssetLoadsFloorPassed).toBeTrue();
    expect(measurement.gates.averageGeometryLoadMsCeiling).toBe(250);
    expect(measurement.gates.averageGeometryLoadMsCeilingPassed).toBeTrue();
    expect(measurement.gates.averageSpriteLoadMsCeiling).toBe(500);
    expect(measurement.gates.averageSpriteLoadMsCeilingPassed).toBeTrue();
    expect(measurement.gates.slowestGeometryLoadMsCeilingPassed).toBeTrue();
    expect(measurement.gates.slowestSpriteLoadMsCeilingPassed).toBeTrue();
    expect(measurement.gates.stableFramePercentFloor).toBe(0);
    expect(measurement.gates.stableFramePercentFloorPassed).toBeTrue();
    expect(measurement.gates.controlSubmissionCeiling).toBe(0);
    expect(measurement.gates.controlSubmissionCeilingPassed).toBeTrue();
    expect(measurement.gates.resizeSubmissionCeilingPassed).toBeTrue();
  });

  test("computes worker relief and stability deltas from paired routes", () => {
    const mainMeasurement = buildRenderRouteMeasurement(
      "main",
      "http://127.0.0.1:4178/?world=local-sandbox&backend=webgl2",
      createStats({
        mainThreadPerf: {
          submissionsCompleted: 76,
          byKind: {
            frame: {
              submissionsCompleted: 76,
            },
          },
        },
        runtimePerf: {
          stableFrames: 75,
          slowFrames: 1,
          stableFramePercent: 98.7,
        },
      }),
    );
    const workerMeasurement = buildRenderRouteMeasurement(
      "worker",
      "http://127.0.0.1:4178/?world=local-sandbox&renderThread=worker&backend=webgl2",
      createStats({
        renderThread: "worker",
        requestedRenderThread: "worker",
        mainThreadPerf: {
          submissionsCompleted: 5,
          byKind: {
            frame: {
              submissionsCompleted: 5,
            },
          },
        },
        runtimePerf: {
          stableFrames: 3,
          slowFrames: 1,
          stableFramePercent: 75,
        },
      }),
    );

    const comparison = buildRenderRouteComparison([mainMeasurement, workerMeasurement]);

    expect(comparison).not.toBeNull();
    expect(comparison?.mainFrameSubmissions).toBe(76);
    expect(comparison?.workerFrameSubmissions).toBe(5);
    expect(comparison?.frameSubmissionReductionPercent).toBe(93.4);
    expect(comparison?.stableFramePercentDelta).toBe(-23.7);
    expect(comparison?.slowFrameDelta).toBe(0);
    expect(comparison?.workerGatesPassed).toBeTrue();
  });

  test("packages route measurements into a deterministic report", () => {
    const report = buildRenderRouteMeasurementReport(
      "http://127.0.0.1:4178",
      [
        buildRenderRouteMeasurement(
          "main",
          "http://127.0.0.1:4178/?world=local-sandbox&backend=webgl2",
          createStats(),
        ),
      ],
      1234,
    );

    expect(report.schemaVersion).toBe(2);
    expect(report.generatedAtUnixMs).toBe(1234);
    expect(report.baseUrl).toBe("http://127.0.0.1:4178");
    expect(report.routes).toHaveLength(1);
    expect(report.comparison).toBeNull();
  });

  test("reports deterministic route gate failures", () => {
    const report = buildRenderRouteMeasurementReport("http://127.0.0.1:4178", [
      buildRenderRouteMeasurement(
        "worker",
        "http://127.0.0.1:4178/?world=local-sandbox&renderThread=worker&backend=webgl2",
        createStats({
          renderThread: "worker",
          requestedRenderThread: "worker",
          geometryLoadsCompleted: 3,
          spriteLoadsCompleted: 2,
          averageGeometryLoadMs: 320,
          averageSpriteLoadMs: 540,
          slowestGeometryLoadMs: 2200,
          slowestSpriteLoadMs: 1200,
          mainThreadPerf: {
            byKind: {
              control: {
                submissionsCompleted: 1,
              },
            },
          },
        }),
      ),
    ]);

    expect(report.routes[0]?.gates.averageGeometryLoadMsCeilingPassed).toBeFalse();
    expect(report.routes[0]?.gates.averageSpriteLoadMsCeilingPassed).toBeFalse();
    expect(report.routes[0]?.gates.slowestGeometryLoadMsCeilingPassed).toBeFalse();
    expect(report.routes[0]?.gates.slowestSpriteLoadMsCeilingPassed).toBeFalse();
    expect(collectRenderRouteMeasurementFailures(report)).toEqual([
      "worker route completed only 5 asset loads; expected at least 10",
      "worker route control submissions exceeded 0",
    ]);
    expect(() => assertRenderRouteMeasurementReportGates(report)).toThrow(
      "worker route completed only 5 asset loads; expected at least 10",
    );
  });

  test("fails explicitly when the main route falls below the stable-frame floor", () => {
    const report = buildRenderRouteMeasurementReport("http://127.0.0.1:4178", [
      buildRenderRouteMeasurement(
        "main",
        "http://127.0.0.1:4178/?world=local-sandbox&backend=webgl2",
        createStats({
          geometryLoadsCompleted: 8,
          spriteLoadsCompleted: 4,
          runtimePerf: {
            stableFrames: 44,
            slowFrames: 56,
            stableFramePercent: 44,
          },
        }),
      ),
    ]);

    expect(report.routes[0]?.gates.stableFramePercentFloorPassed).toBeFalse();
    expect(collectRenderRouteMeasurementFailures(report)).toEqual([
      "main route stable frame percent 44 fell below 45",
    ]);
    expect(() => assertRenderRouteMeasurementReportGates(report)).toThrow(
      "main route stable frame percent 44 fell below 45",
    );
  });
});
