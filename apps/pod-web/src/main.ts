import { parseRenderFrame, parseThreeJsWebGpuFrame } from "./contracts";
import { PodThreeWorldRenderer } from "./renderer";
import { createDemoFrame } from "./sample-frame";

declare global {
  interface Window {
    podRender: {
      render: (frame: string) => void;
      renderThreeJsWebGpuFrame: (frame: string) => void;
      resetDemo: () => void;
      getBackend: () => string;
      getStats: () => ReturnType<PodThreeWorldRenderer["getStats"]>;
    };
  }
}

const canvas = document.querySelector<HTMLCanvasElement>("#pod-web-canvas");
const backendLabel = document.querySelector<HTMLElement>("#backend-label");
const frameSourceLabel = document.querySelector<HTMLElement>("#frame-source");
const qualityLabel = document.querySelector<HTMLElement>("#quality-label");
const statsLabel = document.querySelector<HTMLElement>("#stats-label");

if (!canvas || !backendLabel || !frameSourceLabel || !qualityLabel || !statsLabel) {
  throw new Error("pod-web bootstrap failed: required DOM nodes are missing");
}

const renderer = await PodThreeWorldRenderer.create(canvas);
backendLabel.textContent = renderer.backend;
qualityLabel.textContent = renderer.quality.preset;
const runtimeStatsLabel = statsLabel;

let liveFrameSource: "demo" | "legacy" | "threejs" = "demo";
let latestFrameJson: string | null = null;

window.podRender = {
  render(frame: string) {
    latestFrameJson = frame;
    liveFrameSource = "legacy";
    frameSourceLabel.textContent = "legacy pod-render frame";
  },
  renderThreeJsWebGpuFrame(frame: string) {
    latestFrameJson = frame;
    liveFrameSource = "threejs";
    frameSourceLabel.textContent = "Three.js WebGPU frame";
  },
  resetDemo() {
    latestFrameJson = null;
    liveFrameSource = "demo";
    frameSourceLabel.textContent = "demo frame";
  },
  getBackend() {
    return renderer.backend;
  },
  getStats() {
    return renderer.getStats();
  }
};

async function tick(timestamp: number): Promise<void> {
  if (latestFrameJson) {
    if (liveFrameSource === "threejs") {
      await renderer.applyFrame(parseThreeJsWebGpuFrame(latestFrameJson));
    } else {
      await renderer.applyLegacyFrame(parseRenderFrame(latestFrameJson));
    }
  } else {
    await renderer.applyFrame(createDemoFrame(timestamp / 1000));
  }

  const stats = renderer.getStats();
  runtimeStatsLabel.textContent = `${stats.drawCalls} calls · ${stats.triangles} tris · ${stats.pixelRatio.toFixed(
    2
  )}x DPR · ${stats.frameMs.toFixed(1)}ms`;

  requestAnimationFrame((nextTimestamp) => {
    void tick(nextTimestamp);
  });
}

void tick(performance.now());
