import { describe, expect, test } from "bun:test";

import {
  detectRenderWorkerCapabilities,
  readRenderSurfaceMetrics,
  resolveRendererBackendPreference,
  resolveRenderThreadPreference,
  shouldUseRenderWorker
} from "./render-runtime";

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
});
