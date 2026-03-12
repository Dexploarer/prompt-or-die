import { describe, expect, test } from "bun:test";

import type { NetworkGameEvent, TelemetryTrajectorySample } from "./contracts";
import {
  createPodRenderRuntime,
  detectRenderWorkerCapabilities,
  readRenderSurfaceMetrics,
  type RenderWorkerRequest,
  resolveRendererBackendPreference,
  resolveRenderWorkerFallbackReason,
  resolveRenderThreadPreference,
  shouldUseRenderWorker
} from "./render-runtime";
import {
  createPodThreeMainThreadPerfTracker,
  POD_THREE_FRAME_STABILITY_BUDGET_MS,
  createPodThreeRuntimePerfTracker,
  recordPodThreeMainThreadSubmission,
  recordPodThreeRuntimePerfFrame,
  snapshotPodThreeMainThreadPerfStats,
  snapshotPodThreeRuntimePerfStats,
  type PodThreeRendererStats
} from "./renderer";

const BASE_RENDERER_STATS: PodThreeRendererStats = {
  backend: "webgl2",
  renderThread: "worker",
  requestedRenderThread: "worker",
  renderThreadFallbackReason: null,
  qualityPreset: "performance",
  environmentPreset: "daylight",
  landscapeMode: "cliff-lagoon-heightfield",
  waterMode: "animated-lagoon",
  timeOfDayHours: 12,
  pixelRatio: 1,
  drawCalls: 1,
  triangles: 12,
  textures: 1,
  frameMs: 4,
  residentGeometryAssets: 1,
  residentSpriteAssets: 1,
  pendingGeometryAssets: 0,
  pendingSpriteAssets: 0,
  geometryLoadsCompleted: 1,
  spriteLoadsCompleted: 1,
  averageGeometryLoadMs: 1.5,
  averageSpriteLoadMs: 1,
  slowestGeometryLoadMs: 1.5,
  slowestSpriteLoadMs: 1,
  mainThreadPerf: {
    warmupMs: null,
    submissionsCompleted: 0,
    averageSubmissionMs: 0,
    slowestSubmissionMs: 0,
    byKind: {
      frame: {
        submissionsCompleted: 0,
        averageSubmissionMs: 0,
        slowestSubmissionMs: 0
      },
      control: {
        submissionsCompleted: 0,
        averageSubmissionMs: 0,
        slowestSubmissionMs: 0
      },
      resize: {
        submissionsCompleted: 0,
        averageSubmissionMs: 0,
        slowestSubmissionMs: 0
      }
    }
  },
  runtimePerf: {
    warmupMs: 8,
    frameBudgetMs: Number(POD_THREE_FRAME_STABILITY_BUDGET_MS.toFixed(2)),
    framesRendered: 1,
    stableFrames: 1,
    slowFrames: 0,
    stableFramePercent: 100,
    slowestFrameMs: 4
  },
  ambientInstances: 0,
  visibleWorldChunks: 1,
  preloadedWorldChunks: 1
};

class FakeRenderWorker {
  readonly postedMessages: Array<{
    message: RenderWorkerRequest;
    transfer?: Transferable[];
  }> = [];
  private readonly messageListeners = new Set<(event: MessageEvent<unknown>) => void>();
  private readonly errorListeners = new Set<(event: ErrorEvent) => void>();
  terminated = false;

  addEventListener(
    type: "message" | "error",
    listener: ((event: MessageEvent<unknown>) => void) | ((event: ErrorEvent) => void)
  ): void {
    if (type === "message") {
      this.messageListeners.add(listener as (event: MessageEvent<unknown>) => void);
      return;
    }
    this.errorListeners.add(listener as (event: ErrorEvent) => void);
  }

  removeEventListener(
    type: "message" | "error",
    listener: ((event: MessageEvent<unknown>) => void) | ((event: ErrorEvent) => void)
  ): void {
    if (type === "message") {
      this.messageListeners.delete(listener as (event: MessageEvent<unknown>) => void);
      return;
    }
    this.errorListeners.delete(listener as (event: ErrorEvent) => void);
  }

  postMessage(message: RenderWorkerRequest, transfer?: Transferable[]): void {
    this.postedMessages.push({ message, transfer });
    if (message.type === "init") {
      this.dispatchMessage({
        type: "ready",
        backend: BASE_RENDERER_STATS.backend,
        qualityPreset: BASE_RENDERER_STATS.qualityPreset,
        stats: BASE_RENDERER_STATS
      });
    }
  }

  terminate(): void {
    this.terminated = true;
  }

  dispatchMessage(data: unknown): void {
    const event = { data } as MessageEvent<unknown>;
    for (const listener of [...this.messageListeners]) {
      listener(event);
    }
  }
}

describe("render worker runtime", () => {
  test("parses render thread preferences from the query string", () => {
    expect(resolveRenderThreadPreference("")).toBe("auto");
    expect(resolveRenderThreadPreference("?renderThread=main")).toBe("main");
    expect(resolveRenderThreadPreference("?renderThread=worker")).toBe("worker");
    expect(resolveRenderThreadPreference("?renderThread=unknown")).toBe("auto");
  });

  test("parses backend preferences from the query string", () => {
    expect(resolveRendererBackendPreference("")).toBe("auto");
    expect(resolveRendererBackendPreference("?backend=webgpu")).toBe("webgpu");
    expect(resolveRendererBackendPreference("?backend=webgl2")).toBe("webgl2");
    expect(resolveRendererBackendPreference("?backend=software")).toBe("auto");
  });

  test("only enables the render worker when explicitly requested and supported", () => {
    const supported = {
      hasWorkerConstructor: true,
      hasOffscreenCanvas: true,
      hasCanvasTransferControl: true
    };

    expect(shouldUseRenderWorker("auto", supported)).toBe(false);
    expect(shouldUseRenderWorker("main", supported)).toBe(false);
    expect(shouldUseRenderWorker("worker", supported)).toBe(true);
    expect(
      shouldUseRenderWorker("worker", {
        ...supported,
        hasCanvasTransferControl: false
      })
    ).toBe(false);
    expect(resolveRenderWorkerFallbackReason("worker", supported)).toBeNull();
    expect(
      resolveRenderWorkerFallbackReason(
        "worker",
        {
          ...supported,
          hasOffscreenCanvas: false
        },
        true
      )
    ).toBe("missing-offscreen-canvas");
    expect(
      resolveRenderWorkerFallbackReason(
        "worker",
        {
          ...supported,
          hasCanvasTransferControl: false
        },
        true
      )
    ).toBe("missing-canvas-transfer-control");
    expect(resolveRenderWorkerFallbackReason("worker", supported, false)).toBe(
      "missing-worker-constructor"
    );
  });

  test("detects transfer support from the canvas surface", () => {
    expect(
      detectRenderWorkerCapabilities({
        transferControlToOffscreen() {
          return {} as OffscreenCanvas;
        }
      })
    ).toEqual({
      hasWorkerConstructor: typeof Worker === "function",
      hasOffscreenCanvas: typeof OffscreenCanvas === "function",
      hasCanvasTransferControl: true
    });
  });

  test("measures logical canvas size and device pixel ratio for worker rendering", () => {
    expect(
      readRenderSurfaceMetrics(
        {
          clientWidth: 1280,
          clientHeight: 720,
          width: 300,
          height: 150
        },
        2
      )
    ).toEqual({
      width: 1280,
      height: 720,
      devicePixelRatio: 2
    });
  });

  test("tracks warmup and frame-stability counters deterministically", () => {
    const tracker = createPodThreeRuntimePerfTracker(100);

    expect(snapshotPodThreeRuntimePerfStats(tracker)).toEqual({
      warmupMs: null,
      frameBudgetMs: Number(POD_THREE_FRAME_STABILITY_BUDGET_MS.toFixed(2)),
      framesRendered: 0,
      stableFrames: 0,
      slowFrames: 0,
      stableFramePercent: 0,
      slowestFrameMs: 0
    });

    recordPodThreeRuntimePerfFrame(tracker, 8, 124);
    recordPodThreeRuntimePerfFrame(tracker, 24, 152);

    expect(snapshotPodThreeRuntimePerfStats(tracker)).toEqual({
      warmupMs: 24,
      frameBudgetMs: Number(POD_THREE_FRAME_STABILITY_BUDGET_MS.toFixed(2)),
      framesRendered: 2,
      stableFrames: 1,
      slowFrames: 1,
      stableFramePercent: 50,
      slowestFrameMs: 24
    });
  });

  test("tracks main-thread submission counters deterministically", () => {
    const tracker = createPodThreeMainThreadPerfTracker(50);

    expect(snapshotPodThreeMainThreadPerfStats(tracker)).toEqual({
      warmupMs: null,
      submissionsCompleted: 0,
      averageSubmissionMs: 0,
      slowestSubmissionMs: 0,
      byKind: {
        frame: {
          submissionsCompleted: 0,
          averageSubmissionMs: 0,
          slowestSubmissionMs: 0
        },
        control: {
          submissionsCompleted: 0,
          averageSubmissionMs: 0,
          slowestSubmissionMs: 0
        },
        resize: {
          submissionsCompleted: 0,
          averageSubmissionMs: 0,
          slowestSubmissionMs: 0
        }
      }
    });

    recordPodThreeMainThreadSubmission(tracker, 0.4, 66);
    recordPodThreeMainThreadSubmission(tracker, 1.6, 92, "control");

    expect(snapshotPodThreeMainThreadPerfStats(tracker)).toEqual({
      warmupMs: 16,
      submissionsCompleted: 2,
      averageSubmissionMs: 1,
      slowestSubmissionMs: 1.6,
      byKind: {
        frame: {
          submissionsCompleted: 1,
          averageSubmissionMs: 0.4,
          slowestSubmissionMs: 0.4
        },
        control: {
          submissionsCompleted: 1,
          averageSubmissionMs: 1.6,
          slowestSubmissionMs: 1.6
        },
        resize: {
          submissionsCompleted: 0,
          averageSubmissionMs: 0,
          slowestSubmissionMs: 0
        }
      }
    });
  });

  test("worker runtime does not post a duplicate resize after init surface sync", async () => {
    const originalWorker = globalThis.Worker;
    const originalOffscreenCanvas = globalThis.OffscreenCanvas;
    const fakeWorker = new FakeRenderWorker();

    try {
      globalThis.Worker = (function Worker() {}) as unknown as typeof Worker;
      globalThis.OffscreenCanvas = (function OffscreenCanvas() {}) as unknown as typeof OffscreenCanvas;

      const runtime = await createPodRenderRuntime(
        {
          clientWidth: 1280,
          clientHeight: 720,
          width: 1280,
          height: 720,
          transferControlToOffscreen() {
            return { tag: "offscreen" } as unknown as OffscreenCanvas;
          }
        } as unknown as HTMLCanvasElement,
        {},
        "?renderThread=worker&backend=webgl2",
        () => fakeWorker as unknown as Worker
      );

      expect(fakeWorker.postedMessages.map((entry) => entry.message.type)).toEqual(["init"]);
      expect(runtime.getStats().mainThreadPerf.byKind.resize.submissionsCompleted).toBe(0);
      runtime.dispose();
    } finally {
      if (originalWorker === undefined) {
        delete (globalThis as Record<string, unknown>).Worker;
      } else {
        globalThis.Worker = originalWorker;
      }
      if (originalOffscreenCanvas === undefined) {
        delete (globalThis as Record<string, unknown>).OffscreenCanvas;
      } else {
        globalThis.OffscreenCanvas = originalOffscreenCanvas;
      }
    }
  });

  test("worker runtime coalesces main-thread frame submissions until the worker finishes", async () => {
    const originalWorker = globalThis.Worker;
    const originalOffscreenCanvas = globalThis.OffscreenCanvas;
    const fakeWorker = new FakeRenderWorker();
    const frameA = { id: "frame-a" } as unknown as Parameters<
      Awaited<ReturnType<typeof createPodRenderRuntime>>["applyFrame"]
    >[0];
    const frameB = { id: "frame-b" } as unknown as typeof frameA;
    const frameC = { id: "frame-c" } as unknown as typeof frameA;

    try {
      globalThis.Worker = (function Worker() {}) as unknown as typeof Worker;
      globalThis.OffscreenCanvas = (function OffscreenCanvas() {}) as unknown as typeof OffscreenCanvas;

      const runtime = await createPodRenderRuntime(
        {
          clientWidth: 1280,
          clientHeight: 720,
          width: 1280,
          height: 720,
          transferControlToOffscreen() {
            return { tag: "offscreen" } as unknown as OffscreenCanvas;
          }
        } as unknown as HTMLCanvasElement,
        {},
        "?renderThread=worker&backend=webgl2",
        () => fakeWorker as unknown as Worker
      );

      await runtime.applyFrame(frameA);
      await runtime.applyFrame(frameB);
      await runtime.applyFrame(frameC);

      const renderMessagesBeforeAck = fakeWorker.postedMessages.filter(
        (entry) => entry.message.type === "applyFrame"
      );
      expect(renderMessagesBeforeAck).toHaveLength(1);
      expect(renderMessagesBeforeAck[0]?.message).toMatchObject({
        type: "applyFrame",
        frame: frameA
      });
      expect(runtime.getStats().mainThreadPerf.submissionsCompleted).toBe(1);
      expect(runtime.getStats().mainThreadPerf.byKind.frame.submissionsCompleted).toBe(1);

      fakeWorker.dispatchMessage({
        type: "renderComplete",
        stats: BASE_RENDERER_STATS
      });

      const renderMessagesAfterAck = fakeWorker.postedMessages.filter(
        (entry) => entry.message.type === "applyFrame"
      );
      expect(renderMessagesAfterAck).toHaveLength(2);
      expect(renderMessagesAfterAck[1]?.message).toMatchObject({
        type: "applyFrame",
        frame: frameC
      });
      expect(runtime.getStats().mainThreadPerf.submissionsCompleted).toBe(2);
      expect(runtime.getStats().mainThreadPerf.byKind.frame.submissionsCompleted).toBe(2);

      runtime.dispose();
    } finally {
      if (originalWorker === undefined) {
        delete (globalThis as Record<string, unknown>).Worker;
      } else {
        globalThis.Worker = originalWorker;
      }
      if (originalOffscreenCanvas === undefined) {
        delete (globalThis as Record<string, unknown>).OffscreenCanvas;
      } else {
        globalThis.OffscreenCanvas = originalOffscreenCanvas;
      }
    }
  });

  test("worker runtime batches control traffic and keeps only the latest telemetry command", async () => {
    const originalWorker = globalThis.Worker;
    const originalOffscreenCanvas = globalThis.OffscreenCanvas;
    const fakeWorker = new FakeRenderWorker();
    const eventA = {
      tick: 1,
      origin: [0, 0],
      kind: "Spawn",
      summary: "Scout arrived",
      entityIds: [1]
    } satisfies NetworkGameEvent;
    const eventB = {
      tick: 2,
      origin: [1, 0],
      kind: "Loot",
      summary: "Cache opened",
      entityIds: [1]
    } satisfies NetworkGameEvent;

    try {
      globalThis.Worker = (function Worker() {}) as unknown as typeof Worker;
      globalThis.OffscreenCanvas = (function OffscreenCanvas() {}) as unknown as typeof OffscreenCanvas;

      const runtime = await createPodRenderRuntime(
        {
          clientWidth: 1280,
          clientHeight: 720,
          width: 1280,
          height: 720,
          transferControlToOffscreen() {
            return { tag: "offscreen" } as unknown as OffscreenCanvas;
          }
        } as unknown as HTMLCanvasElement,
        {},
        "?renderThread=worker&backend=webgl2",
        () => fakeWorker as unknown as Worker
      );

      runtime.notifyWorldEvents([eventA]);
      runtime.notifyWorldEvents([eventB]);
      runtime.setTelemetryTrail([
        [0, 0, 0],
        [1, 1, 1]
      ] as unknown as TelemetryTrajectorySample[]);
      runtime.clearTelemetryTrail();
      await Promise.resolve();

      const controlMessages = fakeWorker.postedMessages.filter(
        (entry) => entry.message.type === "applyControlState"
      );
      expect(controlMessages).toHaveLength(1);
      expect(controlMessages[0]?.message).toEqual({
        type: "applyControlState",
        events: [eventA, eventB],
        telemetry: {
          mode: "clear"
        }
      });
      expect(runtime.getStats().mainThreadPerf.byKind.control.submissionsCompleted).toBe(1);

      runtime.dispose();
    } finally {
      if (originalWorker === undefined) {
        delete (globalThis as Record<string, unknown>).Worker;
      } else {
        globalThis.Worker = originalWorker;
      }
      if (originalOffscreenCanvas === undefined) {
        delete (globalThis as Record<string, unknown>).OffscreenCanvas;
      } else {
        globalThis.OffscreenCanvas = originalOffscreenCanvas;
      }
    }
  });
});
