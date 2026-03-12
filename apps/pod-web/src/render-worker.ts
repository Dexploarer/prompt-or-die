/// <reference lib="webworker" />

import { PodThreeWorldRenderer } from "./renderer";
import type { RenderWorkerRequest } from "./render-runtime";

const scope = self as DedicatedWorkerGlobalScope;

let renderer: PodThreeWorldRenderer | null = null;
let draining = false;
let queuedRenderCommand:
  | Extract<RenderWorkerRequest, { type: "applyFrame" | "applyLegacyFrame" }>
  | null = null;

scope.addEventListener("message", (event: MessageEvent<RenderWorkerRequest>) => {
  void handleMessage(event.data);
});

async function handleMessage(message: RenderWorkerRequest): Promise<void> {
  try {
    switch (message.type) {
      case "init":
        renderer = await PodThreeWorldRenderer.create(message.canvas, message.options);
        scope.postMessage({
          type: "ready",
          backend: renderer.backend,
          qualityPreset: renderer.quality.preset,
          stats: {
            ...renderer.getStats(),
            renderThread: "worker"
          }
        });
        return;
      case "applyFrame":
      case "applyLegacyFrame":
        queuedRenderCommand = message;
        await drainRenderQueue();
        return;
      case "setTelemetryTrail":
        renderer?.setTelemetryTrail(message.samples);
        return;
      case "notifyWorldEvents":
        renderer?.notifyWorldEvents(message.events);
        return;
      case "clearTelemetryTrail":
        renderer?.clearTelemetryTrail();
        return;
      case "applyControlState":
        if (message.telemetry?.mode === "set") {
          renderer?.setTelemetryTrail(message.telemetry.samples);
        } else if (message.telemetry?.mode === "clear") {
          renderer?.clearTelemetryTrail();
        }
        if (message.events.length > 0) {
          renderer?.notifyWorldEvents(message.events);
        }
        return;
      case "resize":
        renderer?.setSurfaceMetrics(message.surfaceMetrics);
        if (renderer) {
          scope.postMessage({
            type: "stats",
            stats: {
              ...renderer.getStats(),
              renderThread: "worker"
            }
          });
        }
        return;
      case "dispose":
        renderer?.dispose();
        renderer = null;
        queuedRenderCommand = null;
        return;
      default:
        return assertNever(message);
    }
  } catch (error) {
    postWorkerError(error);
  }
}

async function drainRenderQueue(): Promise<void> {
  if (draining || !renderer) {
    return;
  }

  draining = true;

  try {
    while (queuedRenderCommand && renderer) {
      const command = queuedRenderCommand;
      queuedRenderCommand = null;

      if (command.type === "applyFrame") {
        await renderer.applyFrame(command.frame);
      } else {
        await renderer.applyLegacyFrame(command.frame);
      }

      scope.postMessage({
        type: "renderComplete",
        stats: {
          ...renderer.getStats(),
          renderThread: "worker"
        }
      });
    }
  } catch (error) {
    postWorkerError(error);
  } finally {
    draining = false;
  }
}

function postWorkerError(error: unknown): void {
  const message = error instanceof Error ? error.message : String(error);
  scope.postMessage({
    type: "error",
    message
  });
}

function assertNever(value: never): never {
  throw new Error(`Unhandled render worker message: ${String(value)}`);
}
