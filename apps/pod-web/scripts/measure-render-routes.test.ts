import { describe, expect, test } from "bun:test";

import {
  buildRenderRouteComparison,
  buildRenderRouteMeasurement,
  buildRenderRouteMeasurementReport,
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

    expect(measurement.loadsCompleted).toBe(3);
    expect(measurement.pendingAssets).toBe(0);
    expect(measurement.gates.stableFramePercentFloor).toBe(50);
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

    expect(report.schemaVersion).toBe(1);
    expect(report.generatedAtUnixMs).toBe(1234);
    expect(report.baseUrl).toBe("http://127.0.0.1:4178");
    expect(report.routes).toHaveLength(1);
    expect(report.comparison).toBeNull();
  });
});
