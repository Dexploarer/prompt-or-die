import {
  parseRenderFrame,
  parseThreeJsWebGpuFrame,
  parseTickTelemetryEnvelope
} from "./contracts";
import { PodThreeWorldRenderer } from "./renderer";
import { createDemoFrame } from "./sample-frame";
import {
  applyTickTelemetry,
  createTelemetryOverlayState,
  cycleTelemetrySelection,
  resetTelemetry,
  selectedTrajectorySamples,
  setTelemetryEnabled,
  telemetryStats
} from "./telemetry";

declare global {
  interface Window {
    podRender: {
      render: (frame: string) => void;
      renderThreeJsWebGpuFrame: (frame: string) => void;
      renderTickTelemetry: (frame: string) => void;
      resetTelemetry: () => void;
      resetDemo: () => void;
      getBackend: () => string;
      getStats: () => ReturnType<PodThreeWorldRenderer["getStats"]>;
      getTelemetryStats: () => ReturnType<typeof telemetryStats>;
    };
  }
}

const canvas = document.querySelector<HTMLCanvasElement>("#pod-web-canvas");
const backendLabel = document.querySelector<HTMLElement>("#backend-label");
const frameSourceLabel = document.querySelector<HTMLElement>("#frame-source");
const qualityLabel = document.querySelector<HTMLElement>("#quality-label");
const statsLabel = document.querySelector<HTMLElement>("#stats-label");
const telemetryToggle = document.querySelector<HTMLButtonElement>("#telemetry-toggle");
const telemetrySelectionLabel =
  document.querySelector<HTMLElement>("#telemetry-selection");
const telemetryTrailLabel = document.querySelector<HTMLElement>("#telemetry-trail");
const telemetryActionsLabel =
  document.querySelector<HTMLElement>("#telemetry-actions");
const telemetryToolsLabel = document.querySelector<HTMLElement>("#telemetry-tools");
const telemetryRecoveryLabel =
  document.querySelector<HTMLElement>("#telemetry-recovery");
const telemetrySummaryLabel =
  document.querySelector<HTMLElement>("#telemetry-summary");
const telemetryPanel = document.querySelector<HTMLElement>("#telemetry-panel");
const telemetryPrev = document.querySelector<HTMLButtonElement>("#telemetry-prev");
const telemetryNext = document.querySelector<HTMLButtonElement>("#telemetry-next");

if (
  !canvas ||
  !backendLabel ||
  !frameSourceLabel ||
  !qualityLabel ||
  !statsLabel ||
  !telemetryToggle ||
  !telemetrySelectionLabel ||
  !telemetryTrailLabel ||
  !telemetryActionsLabel ||
  !telemetryToolsLabel ||
  !telemetryRecoveryLabel ||
  !telemetrySummaryLabel ||
  !telemetryPanel ||
  !telemetryPrev ||
  !telemetryNext
) {
  throw new Error("pod-web bootstrap failed: required DOM nodes are missing");
}

const telemetryToggleButton = telemetryToggle;
const telemetrySelectionNode = telemetrySelectionLabel;
const telemetryTrailNode = telemetryTrailLabel;
const telemetryActionsNode = telemetryActionsLabel;
const telemetryToolsNode = telemetryToolsLabel;
const telemetryRecoveryNode = telemetryRecoveryLabel;
const telemetrySummaryNode = telemetrySummaryLabel;
const telemetryPanelNode = telemetryPanel;
const telemetryPrevButton = telemetryPrev;
const telemetryNextButton = telemetryNext;

const renderer = await PodThreeWorldRenderer.create(canvas);
backendLabel.textContent = renderer.backend;
qualityLabel.textContent = renderer.quality.preset;
const runtimeStatsLabel = statsLabel;
const telemetryState = createTelemetryOverlayState(300);

let liveFrameSource: "demo" | "legacy" | "threejs" = "demo";
let latestFrameJson: string | null = null;
let lastTelemetryRevision = -1;

telemetryToggleButton.addEventListener("click", () => {
  setTelemetryEnabled(telemetryState, !telemetryState.enabled);
});
telemetryPrevButton.addEventListener("click", () => {
  cycleTelemetrySelection(telemetryState, -1);
});
telemetryNextButton.addEventListener("click", () => {
  cycleTelemetrySelection(telemetryState, 1);
});

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
  renderTickTelemetry(frame: string) {
    applyTickTelemetry(telemetryState, parseTickTelemetryEnvelope(frame));
  },
  resetTelemetry() {
    resetTelemetry(telemetryState);
    renderer.clearTelemetryTrail();
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
  },
  getTelemetryStats() {
    return telemetryStats(telemetryState);
  }
};

async function tick(timestamp: number): Promise<void> {
  if (lastTelemetryRevision !== telemetryState.revision) {
    const samples = telemetryState.enabled
      ? selectedTrajectorySamples(telemetryState)
      : [];
    if (samples.length > 1) {
      renderer.setTelemetryTrail(samples);
    } else {
      renderer.clearTelemetryTrail();
    }
    lastTelemetryRevision = telemetryState.revision;
  }

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
  renderTelemetryHud();

  requestAnimationFrame((nextTimestamp) => {
    void tick(nextTimestamp);
  });
}

function renderTelemetryHud(): void {
  const stats = telemetryStats(telemetryState);
  telemetryToggleButton.textContent = stats.enabled ? "Disable Telemetry" : "Enable Telemetry";
  telemetryPanelNode.dataset.telemetryEnabled = String(stats.enabled);
  telemetrySelectionNode.textContent = stats.selectedLabel;
  telemetryTrailNode.textContent = `${stats.trajectorySamples} samples · ${stats.trajectoryDistance.toFixed(
    2
  )}u`;
  telemetryActionsNode.textContent = `submitted ${stats.submittedActions} · executed ${stats.executedActions} · rejected ${stats.rejectedActions}`;
  telemetryToolsNode.textContent = stats.lastToolStatus
    ? `${stats.lastToolStatus} · ${stats.lastToolLatencyMs ?? 0}ms · ${stats.toolErrors}/${stats.toolCalls} errors`
    : "No tool calls";
  telemetryRecoveryNode.textContent =
    stats.nextRetryTick == null
      ? stats.recoverySummary
      : `${stats.recoverySummary} · retry @ ${stats.nextRetryTick}`;
  telemetrySummaryNode.textContent =
    stats.tick == null
      ? "Waiting for authoritative telemetry."
      : `tick ${stats.tick} · ${stats.visibleEntities} visible · ${stats.audibleEvents} audible · ${stats.messages} messages`;
}

void tick(performance.now());
