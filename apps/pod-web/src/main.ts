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
    };
  }
}

const canvas = document.querySelector<HTMLCanvasElement>("#pod-web-canvas");
const backendLabel = document.querySelector<HTMLElement>("#backend-label");
const frameSourceLabel = document.querySelector<HTMLElement>("#frame-source");

if (!canvas || !backendLabel || !frameSourceLabel) {
  throw new Error("pod-web bootstrap failed: required DOM nodes are missing");
}

const renderer = await PodThreeWorldRenderer.create(canvas);
backendLabel.textContent = renderer.backend;

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

  requestAnimationFrame((nextTimestamp) => {
    void tick(nextTimestamp);
  });
}

void tick(performance.now());
