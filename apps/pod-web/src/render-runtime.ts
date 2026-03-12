import type {
  NetworkGameEvent,
  RenderFrame,
  TelemetryTrajectorySample,
  ThreeJsWebGpuFrame
} from "./contracts";
import {
  PodThreeWorldRenderer,
  createPodThreeMainThreadPerfTracker,
  recordPodThreeMainThreadSubmission,
  resetPodThreeMainThreadPerfTracker,
  snapshotPodThreeMainThreadPerfStats,
  type PodThreeMainThreadSubmissionKind,
  type RenderSurfaceMetrics,
  type PodThreeRendererStats,
  type PodThreeWorldRendererOptions
} from "./renderer";

export type PodRenderThread = "main" | "worker";
export type PodRenderThreadPreference = "auto" | PodRenderThread;

export interface PodRenderWorkerCapabilities {
  hasWorkerConstructor: boolean;
  hasOffscreenCanvas: boolean;
  hasCanvasTransferControl: boolean;
}

export type PodRenderWorkerFallbackReason =
  | "missing-worker-constructor"
  | "missing-offscreen-canvas"
  | "missing-canvas-transfer-control";

export interface PodThreeRenderRuntime {
  readonly backend: PodThreeRendererStats["backend"];
  readonly qualityPreset: PodThreeRendererStats["qualityPreset"];
  readonly renderThread: PodRenderThread;
  applyFrame(frame: ThreeJsWebGpuFrame): Promise<void>;
  applyLegacyFrame(frame: RenderFrame): Promise<void>;
  notifyWorldEvents(events: NetworkGameEvent[]): void | Promise<void>;
  setTelemetryTrail(samples: TelemetryTrajectorySample[]): void | Promise<void>;
  clearTelemetryTrail(): void | Promise<void>;
  resetPerfMetrics(): void | Promise<void>;
  getStats(): PodThreeRendererStats;
  dispose(): void;
}

function monotonicNowMs(): number {
  return typeof performance !== "undefined" && typeof performance.now === "function"
    ? performance.now()
    : Date.now();
}

interface RenderWorkerReadyMessage {
  type: "ready";
  backend: PodThreeRendererStats["backend"];
  qualityPreset: PodThreeRendererStats["qualityPreset"];
  stats: PodThreeRendererStats;
}

interface RenderWorkerStatsMessage {
  type: "stats";
  stats: PodThreeRendererStats;
}

interface RenderWorkerPerfMetricsResetMessage {
  type: "perfMetricsReset";
  stats: PodThreeRendererStats;
}

interface RenderWorkerRenderCompleteMessage {
  type: "renderComplete";
  stats: PodThreeRendererStats;
}

interface RenderWorkerErrorMessage {
  type: "error";
  message: string;
}

type RenderWorkerResponse =
  | RenderWorkerReadyMessage
  | RenderWorkerStatsMessage
  | RenderWorkerPerfMetricsResetMessage
  | RenderWorkerRenderCompleteMessage
  | RenderWorkerErrorMessage;

export type RenderWorkerRequest =
  | {
      type: "init";
      canvas: OffscreenCanvas;
      options: PodThreeWorldRendererOptions;
    }
  | {
      type: "applyFrame";
      frame: ThreeJsWebGpuFrame;
    }
  | {
      type: "applyLegacyFrame";
      frame: RenderFrame;
    }
  | {
      type: "notifyWorldEvents";
      events: NetworkGameEvent[];
    }
  | {
      type: "setTelemetryTrail";
      samples: TelemetryTrajectorySample[];
    }
  | {
      type: "clearTelemetryTrail";
    }
  | {
      type: "resetPerfMetrics";
    }
  | {
      type: "applyControlState";
      events: NetworkGameEvent[];
      telemetry:
        | {
            mode: "set";
            samples: TelemetryTrajectorySample[];
          }
        | {
            mode: "clear";
          }
        | null;
    }
  | {
      type: "resize";
      surfaceMetrics: RenderSurfaceMetrics;
    }
  | {
      type: "dispose";
    };

type RenderWorkerRenderRequest = Extract<
  RenderWorkerRequest,
  { type: "applyFrame" | "applyLegacyFrame" }
>;

type RenderWorkerTelemetryCommand = Extract<
  RenderWorkerRequest,
  { type: "applyControlState" }
>["telemetry"];

export async function createPodRenderRuntime(
  canvas: HTMLCanvasElement,
  options: PodThreeWorldRendererOptions = {},
  search =
    typeof window === "object" ? window.location.search : "",
  workerFactory: ((url: URL, options: WorkerOptions) => Worker) | null = defaultWorkerFactory
): Promise<PodThreeRenderRuntime> {
  const preference = resolveRenderThreadPreference(search);
  const backendPreference = resolveRendererBackendPreference(search);
  const capabilities = detectRenderWorkerCapabilities(canvas);
  const fallbackReason = resolveRenderWorkerFallbackReason(
    preference,
    capabilities,
    Boolean(workerFactory)
  );
  const defaultWorkerQualityPreset = preference === "worker" ? "performance" : undefined;
  const resolvedOptions: PodThreeWorldRendererOptions = {
    ...options,
    backendPreference,
    qualityPreset: options.qualityPreset ?? defaultWorkerQualityPreset,
    enableShadows:
      options.enableShadows ?? (preference === "worker" ? false : undefined)
  };

  if (shouldUseRenderWorker(preference, capabilities) && workerFactory) {
    return await WorkerPodRenderRuntime.create(
      canvas,
      resolvedOptions,
      workerFactory,
      preference
    );
  }

  if (fallbackReason) {
    console.warn(
      `Falling back to main-thread rendering; render worker prerequisite missing: ${fallbackReason}`
    );
  }

  const renderer = await PodThreeWorldRenderer.create(canvas, resolvedOptions);
  return new MainThreadPodRenderRuntime(renderer, preference, fallbackReason);
}

export function resolveRenderThreadPreference(search: string): PodRenderThreadPreference {
  const params = new URLSearchParams(search);
  const requested = params.get("renderThread")?.trim().toLowerCase();
  if (requested === "main" || requested === "worker") {
    return requested;
  }
  return "auto";
}

export function resolveRendererBackendPreference(
  search: string
): PodThreeWorldRendererOptions["backendPreference"] {
  const params = new URLSearchParams(search);
  const requested = params.get("backend")?.trim().toLowerCase();
  if (requested === "webgpu" || requested === "webgl2") {
    return requested;
  }
  return "auto";
}

export function shouldUseRenderWorker(
  preference: PodRenderThreadPreference,
  capabilities: PodRenderWorkerCapabilities
): boolean {
  if (preference !== "worker") {
    return false;
  }

  return (
    capabilities.hasWorkerConstructor &&
    capabilities.hasOffscreenCanvas &&
    capabilities.hasCanvasTransferControl
  );
}

export function resolveRenderWorkerFallbackReason(
  preference: PodRenderThreadPreference,
  capabilities: PodRenderWorkerCapabilities,
  workerFactoryAvailable = true
): PodRenderWorkerFallbackReason | null {
  if (preference !== "worker") {
    return null;
  }
  if (!workerFactoryAvailable || !capabilities.hasWorkerConstructor) {
    return "missing-worker-constructor";
  }
  if (!capabilities.hasOffscreenCanvas) {
    return "missing-offscreen-canvas";
  }
  if (!capabilities.hasCanvasTransferControl) {
    return "missing-canvas-transfer-control";
  }
  return null;
}

export function detectRenderWorkerCapabilities(
  canvas: HTMLCanvasElement | { transferControlToOffscreen?: unknown }
): PodRenderWorkerCapabilities {
  return {
    hasWorkerConstructor: typeof Worker === "function",
    hasOffscreenCanvas: typeof OffscreenCanvas === "function",
    hasCanvasTransferControl: typeof canvas.transferControlToOffscreen === "function"
  };
}

export function readRenderSurfaceMetrics(
  canvas: Pick<HTMLCanvasElement, "clientWidth" | "clientHeight" | "width" | "height">,
  devicePixelRatio =
    typeof window === "object" && typeof window.devicePixelRatio === "number"
      ? window.devicePixelRatio
      : 1
): RenderSurfaceMetrics {
  return {
    width: Math.max(canvas.clientWidth || canvas.width || 1, 1),
    height: Math.max(canvas.clientHeight || canvas.height || 1, 1),
    devicePixelRatio: Math.max(devicePixelRatio, 1)
  };
}

class MainThreadPodRenderRuntime implements PodThreeRenderRuntime {
  readonly backend: PodThreeRendererStats["backend"];
  readonly qualityPreset: PodThreeRendererStats["qualityPreset"];
  readonly renderThread: PodRenderThread = "main";
  private readonly mainThreadPerf = createPodThreeMainThreadPerfTracker(monotonicNowMs());

  constructor(
    private readonly renderer: PodThreeWorldRenderer,
    private readonly requestedRenderThread: PodRenderThreadPreference,
    private readonly fallbackReason: PodRenderWorkerFallbackReason | null
  ) {
    this.backend = renderer.backend;
    this.qualityPreset = renderer.quality.preset;
  }

  async applyFrame(frame: ThreeJsWebGpuFrame): Promise<void> {
    const startedAt = monotonicNowMs();
    await this.renderer.applyFrame(frame);
    this.recordSubmission(startedAt);
  }

  async applyLegacyFrame(frame: RenderFrame): Promise<void> {
    const startedAt = monotonicNowMs();
    await this.renderer.applyLegacyFrame(frame);
    this.recordSubmission(startedAt);
  }

  notifyWorldEvents(events: NetworkGameEvent[]): void {
    this.renderer.notifyWorldEvents(events);
  }

  setTelemetryTrail(samples: TelemetryTrajectorySample[]): void {
    this.renderer.setTelemetryTrail(samples);
  }

  clearTelemetryTrail(): void {
    this.renderer.clearTelemetryTrail();
  }

  resetPerfMetrics(): void {
    const nowMs = monotonicNowMs();
    resetPodThreeMainThreadPerfTracker(this.mainThreadPerf, nowMs);
    this.renderer.resetPerfMetrics(nowMs);
  }

  getStats(): PodThreeRendererStats {
    return this.decorateStats(this.renderer.getStats());
  }

  dispose(): void {
    this.renderer.dispose();
  }

  private recordSubmission(startedAt: number): void {
    const endedAt = monotonicNowMs();
    recordPodThreeMainThreadSubmission(
      this.mainThreadPerf,
      endedAt - startedAt,
      endedAt
    );
  }

  private decorateStats(stats: PodThreeRendererStats): PodThreeRendererStats {
    return {
      ...stats,
      renderThread: "main",
      requestedRenderThread: this.requestedRenderThread,
      renderThreadFallbackReason: this.fallbackReason,
      mainThreadPerf: snapshotPodThreeMainThreadPerfStats(this.mainThreadPerf)
    };
  }
}

class WorkerPodRenderRuntime implements PodThreeRenderRuntime {
  static async create(
    canvas: HTMLCanvasElement,
    options: PodThreeWorldRendererOptions,
    workerFactory: (url: URL, options: WorkerOptions) => Worker,
    requestedRenderThread: PodRenderThreadPreference
  ): Promise<WorkerPodRenderRuntime> {
    const worker = workerFactory(new URL("./render-worker.ts", import.meta.url), {
      type: "module"
    });
    const offscreenCanvas = canvas.transferControlToOffscreen();
    const surfaceMetrics = readRenderSurfaceMetrics(canvas);

    const ready = await new Promise<RenderWorkerReadyMessage>((resolve, reject) => {
      const handleMessage = (event: MessageEvent<RenderWorkerResponse>) => {
        const data = event.data;
        if (data.type === "ready") {
          worker.removeEventListener("message", handleMessage);
          worker.removeEventListener("error", handleError);
          resolve(data);
          return;
        }

        if (data.type === "error") {
          worker.removeEventListener("message", handleMessage);
          worker.removeEventListener("error", handleError);
          reject(new Error(data.message));
        }
      };

      const handleError = (event: ErrorEvent) => {
        worker.removeEventListener("message", handleMessage);
        worker.removeEventListener("error", handleError);
        reject(event.error ?? new Error(event.message));
      };

      worker.addEventListener("message", handleMessage);
      worker.addEventListener("error", handleError);
      worker.postMessage(
        {
          type: "init",
          canvas: offscreenCanvas,
          options: {
            ...options,
            surfaceMetrics
          }
        } satisfies RenderWorkerRequest,
        [offscreenCanvas]
      );
    });

    return new WorkerPodRenderRuntime(
      worker,
      ready,
      canvas,
      requestedRenderThread,
      surfaceMetrics
    );
  }

  readonly backend: PodThreeRendererStats["backend"];
  readonly qualityPreset: PodThreeRendererStats["qualityPreset"];
  readonly renderThread: PodRenderThread = "worker";
  private latestStats: PodThreeRendererStats;
  private readonly mainThreadPerf = createPodThreeMainThreadPerfTracker(monotonicNowMs());
  private readonly resizeObserver: ResizeObserver | null;
  private queuedRenderCommand: RenderWorkerRenderRequest | null = null;
  private queuedWorldEvents: NetworkGameEvent[] = [];
  private queuedTelemetryCommand: RenderWorkerTelemetryCommand = null;
  private renderSubmissionInFlight = false;
  private controlFlushScheduled = false;
  private disposed = false;
  private lastSurfaceMetrics: RenderSurfaceMetrics;
  private readonly pendingPerfResetResolvers: Array<{
    resolve: () => void;
    reject: (error: Error) => void;
  }> = [];
  private readonly handleWindowResize = () => {
    this.syncSurfaceMetrics();
  };

  private constructor(
    private readonly worker: Worker,
    ready: RenderWorkerReadyMessage,
    private readonly canvas: HTMLCanvasElement,
    private readonly requestedRenderThread: PodRenderThreadPreference,
    initialSurfaceMetrics: RenderSurfaceMetrics
  ) {
    this.backend = ready.backend;
    this.qualityPreset = ready.qualityPreset;
    this.latestStats = this.decorateStats(ready.stats);
    this.lastSurfaceMetrics = initialSurfaceMetrics;

    this.resizeObserver =
      typeof ResizeObserver !== "undefined"
        ? new ResizeObserver(() => {
            this.syncSurfaceMetrics();
          })
        : null;
    this.resizeObserver?.observe(canvas);
    if (typeof window === "object") {
      window.addEventListener("resize", this.handleWindowResize);
    }

    this.worker.addEventListener("message", (event: MessageEvent<RenderWorkerResponse>) => {
      if (event.data.type === "stats") {
        this.latestStats = this.decorateStats(event.data.stats);
        return;
      }

      if (event.data.type === "perfMetricsReset") {
        this.latestStats = this.decorateStats(event.data.stats);
        this.resolvePendingPerfResetResolvers();
        return;
      }

      if (event.data.type === "renderComplete") {
        this.latestStats = this.decorateStats(event.data.stats);
        this.renderSubmissionInFlight = false;
        this.flushQueuedRenderCommand();
        return;
      }

      if (event.data.type === "error") {
        console.error("pod-web render worker error:", event.data.message);
        this.rejectPendingPerfResetResolvers(new Error(event.data.message));
        this.renderSubmissionInFlight = false;
        this.flushQueuedRenderCommand();
      }
    });

    this.syncSurfaceMetrics();
  }

  async applyFrame(frame: ThreeJsWebGpuFrame): Promise<void> {
    this.queueRenderCommand({
      type: "applyFrame",
      frame
    } satisfies RenderWorkerRequest);
  }

  async applyLegacyFrame(frame: RenderFrame): Promise<void> {
    this.queueRenderCommand({
      type: "applyLegacyFrame",
      frame
    } satisfies RenderWorkerRequest);
  }

  notifyWorldEvents(events: NetworkGameEvent[]): void {
    if (events.length === 0) {
      return;
    }
    this.queuedWorldEvents.push(...events);
    this.scheduleControlFlush();
  }

  setTelemetryTrail(samples: TelemetryTrajectorySample[]): void {
    this.queuedTelemetryCommand = {
      mode: "set",
      samples
    };
    this.scheduleControlFlush();
  }

  clearTelemetryTrail(): void {
    this.queuedTelemetryCommand = {
      mode: "clear"
    };
    this.scheduleControlFlush();
  }

  resetPerfMetrics(): Promise<void> {
    if (this.disposed) {
      return Promise.resolve();
    }

    const nowMs = monotonicNowMs();
    resetPodThreeMainThreadPerfTracker(this.mainThreadPerf, nowMs);
    this.latestStats = this.decorateStats(this.latestStats);

    return new Promise<void>((resolve, reject) => {
      this.pendingPerfResetResolvers.push({ resolve, reject });
      this.worker.postMessage({ type: "resetPerfMetrics" } satisfies RenderWorkerRequest);
    });
  }

  getStats(): PodThreeRendererStats {
    return this.latestStats;
  }

  dispose(): void {
    this.resizeObserver?.disconnect();
    if (typeof window === "object") {
      window.removeEventListener("resize", this.handleWindowResize);
    }
    this.disposed = true;
    this.queuedRenderCommand = null;
    this.queuedWorldEvents = [];
    this.queuedTelemetryCommand = null;
    this.controlFlushScheduled = false;
    this.renderSubmissionInFlight = false;
    this.rejectPendingPerfResetResolvers(new Error("render worker disposed"));
    this.worker.postMessage({ type: "dispose" } satisfies RenderWorkerRequest);
    this.worker.terminate();
  }

  private syncSurfaceMetrics(): void {
    const surfaceMetrics = readRenderSurfaceMetrics(this.canvas);
    if (surfaceMetricsEqual(surfaceMetrics, this.lastSurfaceMetrics)) {
      return;
    }
    this.lastSurfaceMetrics = surfaceMetrics;
    this.postMessage(
      {
        type: "resize",
        surfaceMetrics
      } satisfies RenderWorkerRequest,
      "resize"
    );
  }

  private queueRenderCommand(command: RenderWorkerRenderRequest): void {
    this.queuedRenderCommand = command;
    this.flushQueuedRenderCommand();
  }

  private scheduleControlFlush(): void {
    if (this.controlFlushScheduled || this.disposed) {
      return;
    }
    this.controlFlushScheduled = true;
    scheduleMicrotask(() => {
      this.controlFlushScheduled = false;
      this.flushQueuedControlState();
    });
  }

  private flushQueuedControlState(): void {
    if (this.disposed) {
      return;
    }
    if (this.queuedWorldEvents.length === 0 && this.queuedTelemetryCommand == null) {
      return;
    }

    const events = this.queuedWorldEvents;
    const telemetry = this.queuedTelemetryCommand;
    this.queuedWorldEvents = [];
    this.queuedTelemetryCommand = null;
    this.postMessage(
      {
        type: "applyControlState",
        events,
        telemetry
      } satisfies RenderWorkerRequest,
      "control"
    );
  }

  private flushQueuedRenderCommand(): void {
    if (this.renderSubmissionInFlight || !this.queuedRenderCommand) {
      return;
    }

    this.flushQueuedControlState();
    const command = this.queuedRenderCommand;
    this.queuedRenderCommand = null;
    this.renderSubmissionInFlight = true;
    this.postMessage(command, "frame");
  }

  private postMessage(
    message: RenderWorkerRequest,
    kind: PodThreeMainThreadSubmissionKind
  ): void {
    const startedAt = monotonicNowMs();
    this.worker.postMessage(message);
    this.recordSubmission(startedAt, kind);
  }

  private recordSubmission(
    startedAt: number,
    kind: PodThreeMainThreadSubmissionKind = "frame"
  ): void {
    const endedAt = monotonicNowMs();
    recordPodThreeMainThreadSubmission(
      this.mainThreadPerf,
      endedAt - startedAt,
      endedAt,
      kind
    );
    this.latestStats = this.decorateStats(this.latestStats);
  }

  private resolvePendingPerfResetResolvers(): void {
    if (this.pendingPerfResetResolvers.length === 0) {
      return;
    }
    const pendingResolvers = this.pendingPerfResetResolvers.splice(0);
    for (const resolver of pendingResolvers) {
      resolver.resolve();
    }
  }

  private rejectPendingPerfResetResolvers(error: Error): void {
    if (this.pendingPerfResetResolvers.length === 0) {
      return;
    }
    const pendingResolvers = this.pendingPerfResetResolvers.splice(0);
    for (const resolver of pendingResolvers) {
      resolver.reject(error);
    }
  }

  private decorateStats(stats: PodThreeRendererStats): PodThreeRendererStats {
    return {
      ...stats,
      renderThread: "worker",
      requestedRenderThread: this.requestedRenderThread,
      renderThreadFallbackReason: null,
      mainThreadPerf: snapshotPodThreeMainThreadPerfStats(this.mainThreadPerf)
    };
  }
}

const defaultWorkerFactory =
  typeof Worker === "function"
    ? (url: URL, options: WorkerOptions) => new Worker(url, options)
    : null;

function surfaceMetricsEqual(
  left: RenderSurfaceMetrics,
  right: RenderSurfaceMetrics
): boolean {
  return (
    left.width === right.width &&
    left.height === right.height &&
    left.devicePixelRatio === right.devicePixelRatio
  );
}

function scheduleMicrotask(callback: () => void): void {
  if (typeof queueMicrotask === "function") {
    queueMicrotask(callback);
    return;
  }
  void Promise.resolve().then(callback);
}
