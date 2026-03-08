import type {
  RenderFrame,
  TelemetryTrajectorySample,
  ThreeJsWebGpuFrame
} from "./contracts";
import {
  PodThreeWorldRenderer,
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

export interface PodThreeRenderRuntime {
  readonly backend: PodThreeRendererStats["backend"];
  readonly qualityPreset: PodThreeRendererStats["qualityPreset"];
  readonly renderThread: PodRenderThread;
  applyFrame(frame: ThreeJsWebGpuFrame): Promise<void>;
  applyLegacyFrame(frame: RenderFrame): Promise<void>;
  setTelemetryTrail(samples: TelemetryTrajectorySample[]): void | Promise<void>;
  clearTelemetryTrail(): void | Promise<void>;
  getStats(): PodThreeRendererStats;
  dispose(): void;
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

interface RenderWorkerErrorMessage {
  type: "error";
  message: string;
}

type RenderWorkerResponse =
  | RenderWorkerReadyMessage
  | RenderWorkerStatsMessage
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
      type: "setTelemetryTrail";
      samples: TelemetryTrajectorySample[];
    }
  | {
      type: "clearTelemetryTrail";
    }
  | {
      type: "resize";
      surfaceMetrics: RenderSurfaceMetrics;
    }
  | {
      type: "dispose";
    };

export async function createPodRenderRuntime(
  canvas: HTMLCanvasElement,
  options: PodThreeWorldRendererOptions = {},
  search =
    typeof window === "object" ? window.location.search : "",
  workerFactory: ((url: URL, options: WorkerOptions) => Worker) | null = defaultWorkerFactory
): Promise<PodThreeRenderRuntime> {
  const preference = resolveRenderThreadPreference(search);
  const capabilities = detectRenderWorkerCapabilities(canvas);

  if (shouldUseRenderWorker(preference, capabilities) && workerFactory) {
    return await WorkerPodRenderRuntime.create(canvas, options, workerFactory);
  }

  if (preference === "worker" && !shouldUseRenderWorker(preference, capabilities)) {
    console.warn("Falling back to main-thread rendering; render worker prerequisites are missing");
  }

  const renderer = await PodThreeWorldRenderer.create(canvas, options);
  return new MainThreadPodRenderRuntime(renderer);
}

export function resolveRenderThreadPreference(search: string): PodRenderThreadPreference {
  const params = new URLSearchParams(search);
  const requested = params.get("renderThread")?.trim().toLowerCase();
  if (requested === "main" || requested === "worker") {
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

  constructor(private readonly renderer: PodThreeWorldRenderer) {
    this.backend = renderer.backend;
    this.qualityPreset = renderer.quality.preset;
  }

  async applyFrame(frame: ThreeJsWebGpuFrame): Promise<void> {
    await this.renderer.applyFrame(frame);
  }

  async applyLegacyFrame(frame: RenderFrame): Promise<void> {
    await this.renderer.applyLegacyFrame(frame);
  }

  setTelemetryTrail(samples: TelemetryTrajectorySample[]): void {
    this.renderer.setTelemetryTrail(samples);
  }

  clearTelemetryTrail(): void {
    this.renderer.clearTelemetryTrail();
  }

  getStats(): PodThreeRendererStats {
    return this.renderer.getStats();
  }

  dispose(): void {
    this.renderer.dispose();
  }
}

class WorkerPodRenderRuntime implements PodThreeRenderRuntime {
  static async create(
    canvas: HTMLCanvasElement,
    options: PodThreeWorldRendererOptions,
    workerFactory: (url: URL, options: WorkerOptions) => Worker
  ): Promise<WorkerPodRenderRuntime> {
    const worker = workerFactory(new URL("./render-worker.ts", import.meta.url), {
      type: "module"
    });
    const offscreenCanvas = canvas.transferControlToOffscreen();

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
            surfaceMetrics: readRenderSurfaceMetrics(canvas)
          }
        } satisfies RenderWorkerRequest,
        [offscreenCanvas]
      );
    });

    return new WorkerPodRenderRuntime(worker, ready, canvas);
  }

  readonly backend: PodThreeRendererStats["backend"];
  readonly qualityPreset: PodThreeRendererStats["qualityPreset"];
  readonly renderThread: PodRenderThread = "worker";
  private latestStats: PodThreeRendererStats;
  private readonly resizeObserver: ResizeObserver | null;
  private readonly handleWindowResize = () => {
    this.syncSurfaceMetrics();
  };

  private constructor(
    private readonly worker: Worker,
    ready: RenderWorkerReadyMessage,
    private readonly canvas: HTMLCanvasElement
  ) {
    this.backend = ready.backend;
    this.qualityPreset = ready.qualityPreset;
    this.latestStats = ready.stats;

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
        this.latestStats = event.data.stats;
        return;
      }

      if (event.data.type === "error") {
        console.error("pod-web render worker error:", event.data.message);
      }
    });

    this.syncSurfaceMetrics();
  }

  async applyFrame(frame: ThreeJsWebGpuFrame): Promise<void> {
    this.worker.postMessage({
      type: "applyFrame",
      frame
    } satisfies RenderWorkerRequest);
  }

  async applyLegacyFrame(frame: RenderFrame): Promise<void> {
    this.worker.postMessage({
      type: "applyLegacyFrame",
      frame
    } satisfies RenderWorkerRequest);
  }

  setTelemetryTrail(samples: TelemetryTrajectorySample[]): void {
    this.worker.postMessage({
      type: "setTelemetryTrail",
      samples
    } satisfies RenderWorkerRequest);
  }

  clearTelemetryTrail(): void {
    this.worker.postMessage({
      type: "clearTelemetryTrail"
    } satisfies RenderWorkerRequest);
  }

  getStats(): PodThreeRendererStats {
    return this.latestStats;
  }

  dispose(): void {
    this.resizeObserver?.disconnect();
    if (typeof window === "object") {
      window.removeEventListener("resize", this.handleWindowResize);
    }
    this.worker.postMessage({ type: "dispose" } satisfies RenderWorkerRequest);
    this.worker.terminate();
  }

  private syncSurfaceMetrics(): void {
    this.worker.postMessage({
      type: "resize",
      surfaceMetrics: readRenderSurfaceMetrics(this.canvas)
    } satisfies RenderWorkerRequest);
  }
}

const defaultWorkerFactory =
  typeof Worker === "function"
    ? (url: URL, options: WorkerOptions) => new Worker(url, options)
    : null;
