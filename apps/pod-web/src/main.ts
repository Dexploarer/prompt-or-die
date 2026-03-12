import {
  type AuthoritativeWorldFrameOptions,
  type BrowserAction,
  type CameraState,
  type NetworkEventBatch,
  type NetworkGameEvent,
  type NetworkEntitySnapshot,
  type NetworkWorldSnapshot,
  type ThreeJsWebGpuFrame,
  type ReplaySummary,
  type ShardIncidentSummary
} from "./contracts";
import {
  cameraDirectionInput,
  cameraRelativeMovementDirection,
  focusGameplaySurface,
  isGameplayKeyCode
} from "./controls-core";
import {
  type DirectConnectActionState,
  type DirectConnectStatus,
  type PodWebDirectConnectClient
} from "./direct-connect";
import type { PodThreeRenderRuntime } from "./render-runtime";
import type { PodWebLocalWorld } from "./local-world";
import { renderGameToText } from "./render-game-text";
import {
  initialHudStateFromLocation,
  localWorldPresentationFromLocation,
  runtimeConfigFromLocation
} from "./runtime-config";
import {
  resolveFixedTimeMs,
  shouldPauseInteractiveRuntime
} from "./runtime-flags";
import { entityUsesSwimSurface, surfaceModeFromEntity } from "./surface-mode";
import type { LiveDebugState } from "./live-debug";
import type {
  PodTelemetryOverlayState,
  PodTelemetryStats
} from "./telemetry";

type ContractsModule = typeof import("./contracts");
type FramePlanModule = typeof import("./frame-plan");
type DemoFrameFactory = typeof import("./sample-frame")["createDemoFrame"];
type GroundPickingModule = typeof import("./ground-picking");
type HudRuntimeModule = typeof import("./hud-runtime");
type DebugRuntimeModule = typeof import("./debug-runtime");

let contractsModulePromise: Promise<ContractsModule> | null = null;
let framePlanModulePromise: Promise<FramePlanModule> | null = null;
let demoFrameFactoryPromise: Promise<DemoFrameFactory> | null = null;
let groundPickingModulePromise: Promise<GroundPickingModule> | null = null;
let hudRuntimeModulePromise: Promise<HudRuntimeModule> | null = null;
let hudRuntimeModule: HudRuntimeModule | null = null;
let debugRuntimeModulePromise: Promise<DebugRuntimeModule> | null = null;
let debugRuntimeModule: DebugRuntimeModule | null = null;

function loadContractsModule(): Promise<ContractsModule> {
  if (!contractsModulePromise) {
    contractsModulePromise = import("./contracts");
  }
  return contractsModulePromise;
}

function loadFramePlanModule(): Promise<FramePlanModule> {
  if (!framePlanModulePromise) {
    framePlanModulePromise = import("./frame-plan");
  }
  return framePlanModulePromise;
}

async function loadDemoFrameFactory(): Promise<DemoFrameFactory> {
  if (!demoFrameFactoryPromise) {
    demoFrameFactoryPromise = import("./sample-frame").then(
      (module) => module.createDemoFrame
    );
  }
  return demoFrameFactoryPromise;
}

async function loadGroundPickingModule(): Promise<GroundPickingModule> {
  if (!groundPickingModulePromise) {
    groundPickingModulePromise = import("./ground-picking");
  }
  return groundPickingModulePromise;
}

async function loadHudRuntimeModule(): Promise<HudRuntimeModule> {
  if (!hudRuntimeModulePromise) {
    hudRuntimeModulePromise = import("./hud-runtime").then((module) => {
      hudRuntimeModule = module;
      return module;
    });
  }
  return hudRuntimeModulePromise;
}

function loadedHudRuntimeModule(): HudRuntimeModule | null {
  if (hudRuntimeModule) {
    return hudRuntimeModule;
  }
  void loadHudRuntimeModule();
  return null;
}

async function loadDebugRuntimeModule(): Promise<DebugRuntimeModule> {
  if (!debugRuntimeModulePromise) {
    debugRuntimeModulePromise = import("./debug-runtime").then((module) => {
      debugRuntimeModule = module;
      return module;
    });
  }
  return debugRuntimeModulePromise;
}

function loadedDebugRuntimeModule(): DebugRuntimeModule | null {
  return debugRuntimeModule;
}

function createInitialTelemetryState(maxSamples: number): PodTelemetryOverlayState {
  return {
    enabled: false,
    maxSamples: Math.max(maxSamples, 1),
    revision: 0,
    history: [],
    selectedAgentId: null,
    selectedEntityId: null
  };
}

function createInitialLiveDebugState(): LiveDebugState {
  return {
    latestToolEventSummary: null,
    latestRollupSummary: null,
    latestFocusedSummary: null,
    latestTransportSummary: null,
    liveReplayDocuments: 0,
    liveIncidentDocuments: 0,
    liveTransportDocuments: 0,
    toolEventsByEntity: new Map(),
    rollupsByEntity: new Map(),
    focusedSummariesByEntity: new Map()
  };
}

function setTelemetryEnabledFallback(
  state: PodTelemetryOverlayState,
  enabled: boolean
): void {
  if (state.enabled === enabled) {
    return;
  }
  state.enabled = enabled;
  state.revision += 1;
}

function resetTelemetryFallback(state: PodTelemetryOverlayState): void {
  state.history = [];
  state.selectedAgentId = null;
  state.selectedEntityId = null;
  state.revision += 1;
}

function telemetryStatsFallback(
  state: PodTelemetryOverlayState
): PodTelemetryStats {
  const latest = state.history.at(-1) ?? null;

  return {
    enabled: state.enabled,
    retainedTicks: state.history.length,
    tick: latest?.tickTelemetry.tick ?? null,
    selectedAgentId: state.selectedAgentId,
    selectedEntityId: state.selectedEntityId,
    selectedLabel: latest ? "Loading telemetry overlay" : "Telemetry idle",
    trajectorySamples: 0,
    trajectoryDistance: 0,
    submittedActions: 0,
    executedActions: 0,
    rejectedActions: 0,
    toolCalls: 0,
    toolErrors: 0,
    toolErrorRate: 0,
    lastToolStatus: null,
    lastToolLatencyMs: null,
    lastToolError: null,
    visibleEntities: 0,
    audibleEvents: 0,
    messages: 0,
    recoverySummary: latest ? "Telemetry warming" : "No telemetry",
    recoveryAttempts: 0,
    nextRetryTick: null
  };
}

declare global {
  interface Window {
    render_game_to_text: () => string;
    advanceTime: (ms: number) => Promise<void>;
    podRender: {
      render: (frame: string) => void;
      renderThreeJsWebGpuFrame: (frame: string) => void;
      renderTickTelemetry: (frame: string) => void;
      renderDebugDocument: (document: string) => void;
      renderReplayDocument: (document: string) => void;
      renderShardIncidentSummary: (document: string) => void;
      streamReplayDocument: (document: string) => void;
      streamShardIncidentSummary: (document: string) => void;
      resetTelemetry: () => void;
      resetDemo: () => void;
      requestGameplayFocus: () => boolean;
      getBackend: () => string;
      getStats: () => ReturnType<PodThreeRenderRuntime["getStats"]>;
      getTelemetryStats: () => PodTelemetryStats;
      getGameplayState: () => {
        renderThread: string;
        frameSource: string;
        worldMode: string | null;
        worldName: string | null;
        focused: boolean;
        controlledEntityId: number | null;
        controlledPosition: [number, number] | null;
        selectedTargetId: number | null;
        clickMoveTarget: [number, number] | null;
        movementSignature: string;
        latestFeedback: string;
      };
      getReplaySummary: () => ReplaySummary | null;
      getIncidentSummary: () => ShardIncidentSummary | null;
    };
  }
}

const canvas = document.querySelector<HTMLCanvasElement>("#pod-web-canvas");
const backendLabel = document.querySelector<HTMLElement>("#backend-label");
const frameSourceLabel = document.querySelector<HTMLElement>("#frame-source");
const connectionLabel = document.querySelector<HTMLElement>("#connection-label");
const worldLabel = document.querySelector<HTMLElement>("#world-label");
const populationLabel = document.querySelector<HTMLElement>("#population-label");
const populationHeatmapCanvas =
  document.querySelector<HTMLCanvasElement>("#population-heatmap");
const populationHeatmapLegend =
  document.querySelector<HTMLElement>("#population-heatmap-legend");
const targetLabel = document.querySelector<HTMLElement>("#target-label");
const affordanceLabel = document.querySelector<HTMLElement>("#affordance-label");
const actionStatusLabel = document.querySelector<HTMLElement>("#action-status-label");
const feedbackLabel = document.querySelector<HTMLElement>("#feedback-label");
const eventFeedLabel = document.querySelector<HTMLElement>("#event-feed-label");
const qualityLabel = document.querySelector<HTMLElement>("#quality-label");
const statsLabel = document.querySelector<HTMLElement>("#stats-label");
const chatForm = document.querySelector<HTMLFormElement>("#chat-form");
const chatInput = document.querySelector<HTMLInputElement>("#chat-input");
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
const replaySummaryLabel = document.querySelector<HTMLElement>("#replay-summary");
const incidentSummaryLabel =
  document.querySelector<HTMLElement>("#incident-summary");
const toolEventSummaryLabel =
  document.querySelector<HTMLElement>("#tool-event-summary");
const rollupSummaryLabel =
  document.querySelector<HTMLElement>("#rollup-summary");
const telemetryPanel = document.querySelector<HTMLElement>("#telemetry-panel");
const telemetryPrev = document.querySelector<HTMLButtonElement>("#telemetry-prev");
const telemetryNext = document.querySelector<HTMLButtonElement>("#telemetry-next");

if (
  !canvas ||
  !backendLabel ||
  !frameSourceLabel ||
  !connectionLabel ||
  !worldLabel ||
  !populationLabel ||
  !populationHeatmapCanvas ||
  !populationHeatmapLegend ||
  !targetLabel ||
  !affordanceLabel ||
  !actionStatusLabel ||
  !feedbackLabel ||
  !eventFeedLabel ||
  !qualityLabel ||
  !statsLabel ||
  !chatForm ||
  !chatInput ||
  !telemetryToggle ||
  !telemetrySelectionLabel ||
  !telemetryTrailLabel ||
  !telemetryActionsLabel ||
  !telemetryToolsLabel ||
  !telemetryRecoveryLabel ||
  !telemetrySummaryLabel ||
  !replaySummaryLabel ||
  !incidentSummaryLabel ||
  !toolEventSummaryLabel ||
  !rollupSummaryLabel ||
  !telemetryPanel ||
  !telemetryPrev ||
  !telemetryNext
) {
  throw new Error("pod-web bootstrap failed: required DOM nodes are missing");
}

const telemetryToggleButton = telemetryToggle;
const renderCanvas = canvas;
const LOCAL_SANDBOX_STEP_MS = 1000 / 60;
focusGameplaySurface(renderCanvas);
const initialFixedTimeMs = resolveFixedTimeMs(window.location.search);
const interactiveRuntimePaused = shouldPauseInteractiveRuntime(window.location.search);
const frameSourceNode = frameSourceLabel;
const connectionNode = connectionLabel;
const worldNode = worldLabel;
const populationNode = populationLabel;
const populationHeatmapCanvasNode = populationHeatmapCanvas;
const populationHeatmapLegendNode = populationHeatmapLegend;
const targetNode = targetLabel;
const affordanceNode = affordanceLabel;
const actionStatusNode = actionStatusLabel;
const feedbackNode = feedbackLabel;
const eventFeedNode = eventFeedLabel;
const chatFormNode = chatForm;
const chatInputNode = chatInput;
const telemetrySelectionNode = telemetrySelectionLabel;
const telemetryTrailNode = telemetryTrailLabel;
const telemetryActionsNode = telemetryActionsLabel;
const telemetryToolsNode = telemetryToolsLabel;
const telemetryRecoveryNode = telemetryRecoveryLabel;
const telemetrySummaryNode = telemetrySummaryLabel;
const replaySummaryNode = replaySummaryLabel;
const incidentSummaryNode = incidentSummaryLabel;
const toolEventSummaryNode = toolEventSummaryLabel;
const rollupSummaryNode = rollupSummaryLabel;
const telemetryPanelNode = telemetryPanel;
const telemetryPrevButton = telemetryPrev;
const telemetryNextButton = telemetryNext;
const bootHudState = initialHudStateFromLocation(window.location);

function shouldShowDebugGrid(search: string): boolean {
  const params = new URLSearchParams(search);
  const value = params.get("grid") ?? params.get("debugGrid");
  return value === "1" || value === "true";
}

feedbackNode.textContent = bootHudState.feedback;
connectionNode.textContent = bootHudState.connectionBadge;
worldNode.textContent = bootHudState.worldLabel;
populationNode.textContent = bootHudState.populationLabel;
frameSourceNode.textContent = bootHudState.frameSourceLabel;
void loadHudRuntimeModule();

const { createPodRenderRuntime } = await import("./render-runtime");

const renderer = await createPodRenderRuntime(renderCanvas, {
  showGrid: shouldShowDebugGrid(window.location.search),
  fixedTimeMs: initialFixedTimeMs ?? undefined
});
backendLabel.textContent = renderer.backend;
qualityLabel.textContent = renderer.qualityPreset;
const runtimeStatsLabel = statsLabel;
const telemetryState = createInitialTelemetryState(300);
const runtimeConfig = runtimeConfigFromLocation(window.location);
const localWorldPresentation = localWorldPresentationFromLocation(window.location);
const offlinePlayerName =
  new URLSearchParams(window.location.search).get("player")?.trim() || "WebPlayer";
let localSandbox: PodWebLocalWorld | null = null;
if (!runtimeConfig) {
  const { PodWebLocalWorld } = await import("./local-world");
  localSandbox = new PodWebLocalWorld(offlinePlayerName, localWorldPresentation.presetId);
}

let liveFrameSource: "demo" | "legacy" | "threejs" = "demo";
let latestFrame: string | ThreeJsWebGpuFrame | null = null;
let currentRenderedCamera: CameraState | null = null;
let lastTelemetryRevision = -1;
let latestReplaySummary: ReplaySummary | null = null;
let latestIncidentSummary: ShardIncidentSummary | null = null;
const liveDebugState = createInitialLiveDebugState();
let lastPublishedDebugFocusEntityId: number | null | undefined = undefined;
let latestSnapshot: NetworkWorldSnapshot | null = null;
let latestActionStatus: DirectConnectActionState = {
  pendingCount: 0,
  lastSubmittedTick: null,
  lastAcknowledgedTick: null,
  lastRejectedTick: null,
  lastRejectedReason: null,
  lastActionSummary: null
};
let latestFeedback = runtimeConfig
  ? "Awaiting authoritative outcomes"
  : localWorldPresentation.readyFeedback;
let recentWorldEvents: NetworkGameEvent[] = [];
let manualFrameOverride = false;
let cameraImpact = 0;
let swimCameraBlend = 0;
let liveConnectionStatus: DirectConnectStatus | null = runtimeConfig
  ? {
      phase: "idle",
      detail: `Waiting to connect to ${runtimeConfig.url}`,
      url: runtimeConfig.url,
      tick: null,
      entityCount: 0,
      controlledEntity: null,
      authoritativeDigest: null,
      clientId: null,
      roundTripMs: null,
      jitterMs: null,
      lastPongServerTick: null,
      heartbeatAgeMs: null
    }
  : null;

if (runtimeConfig?.debugTelemetry) {
  setTelemetryEnabledFallback(telemetryState, true);
  void loadDebugRuntimeModule();
}

async function applyAuthoritativeSnapshot(
  snapshot: NetworkWorldSnapshot,
  frameOptions: AuthoritativeWorldFrameOptions,
  status: DirectConnectStatus | null,
  frameSource: string
): Promise<void> {
  latestSnapshot = snapshot;
  liveConnectionStatus = status;
  syncSelectedTarget();
  const { buildAuthoritativeWorldFrame } = await loadContractsModule();
  if (latestSnapshot !== snapshot) {
    return;
  }
  latestFrame = buildAuthoritativeWorldFrame(snapshot, frameOptions);
  liveFrameSource = "threejs";
  frameSourceNode.textContent = frameSource;
}

let liveClient: PodWebDirectConnectClient | null = null;
if (runtimeConfig) {
  const { PodWebDirectConnectClient } = await import("./direct-connect");
  liveClient = new PodWebDirectConnectClient(runtimeConfig, {
    onFrame(snapshot, frameOptions, status) {
      void applyAuthoritativeSnapshot(
        snapshot,
        {
          ...frameOptions,
          viewportWidth: renderCanvas.clientWidth || window.innerWidth,
          viewportHeight: renderCanvas.clientHeight || window.innerHeight
        },
        status,
        "authoritative websocket"
      );
    },
    onEventBatch(batch) {
      applyAuthoritativeEventBatch(batch);
    },
    onDebugDocument(document) {
      void applyLiveDebugDocument(document);
    },
    onActionState(state) {
      latestActionStatus = state;
    },
    onStatus(status) {
      liveConnectionStatus = status;
    }
  });
}
const pressedKeys = new Set<string>();
let selectedTargetId: number | null = null;
let autoRetaliateEnabled = true;
let clickMoveTarget: [number, number] | null = null;
let lastMovementSignature = "stop";
let lastMovementSubmitAtMs = 0;
let orbitPointerId: number | null = null;
let orbitPointer: [number, number] | null = null;
let showcaseIntroDismissed = false;
let runtimeNowOverrideMs = initialFixedTimeMs;

const cameraRig = {
  initialized: false,
  yaw: 0,
  desiredYaw: 0,
  pitch: 0.34,
  desiredPitch: 0.34,
  zoom: 1.08,
  desiredZoom: 1.08
};

function clearPressedKeys(): void {
  pressedKeys.clear();
  lastMovementSignature = "stop";
}

function dismissShowcaseIntro(): void {
  showcaseIntroDismissed = true;
}

function systemNowMs(): number {
  return typeof performance !== "undefined" && typeof performance.now === "function"
    ? performance.now()
    : Date.now();
}

function currentRuntimeNowMs(): number {
  return runtimeNowOverrideMs ?? systemNowMs();
}

function isEditableTarget(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  );
}

function controlledEntity(): NetworkEntitySnapshot | null {
  const controlledId = liveConnectionStatus?.controlledEntity ?? null;
  if (latestSnapshot == null || controlledId == null) {
    return null;
  }

  return latestSnapshot.entities.find((entity) => entity.id === controlledId) ?? null;
}

function currentPopulationSummary(): string {
  if (!latestSnapshot) {
    return "No authoritative population state yet";
  }

  const controlled = controlledEntity();
  const regionId = controlled?.metadata.regionId ?? null;
  const chunkKey = controlled?.metadata.chunkKey ?? null;
  const region = regionId
    ? latestSnapshot.population.regions.find((entry) => entry.regionId === regionId) ?? null
    : null;
  const chunk = chunkKey
    ? latestSnapshot.population.chunks.find((entry) => entry.chunkKey === chunkKey) ?? null
    : null;

  if (!region && !chunk) {
    return `${latestSnapshot.population.regions.length} regions · ${latestSnapshot.population.chunks.length} chunks`;
  }

  const chunkSummary = chunk
    ? `${chunk.chunkKey} ${chunk.activeEntityCount} active · cap ${chunk.ambientPopulationCap} · budget ${chunk.spawnBudgetRemaining} · respawns ${chunk.pendingRespawns}${chunk.nextRespawnTick != null ? ` @${chunk.nextRespawnTick}` : ""}`
    : "unassigned chunk";
  const regionSummary = region
    ? `${region.regionName} ${region.activeEntityCount} active · ${region.activeChunkCount}/${region.chunkKeys.length} chunks hot · pressure ${region.populationPressure.toFixed(
        2
      )} · respawns ${region.pendingRespawns}${region.nextRespawnTick != null ? ` @${region.nextRespawnTick}` : ""}`
    : "unassigned region";
  return `${regionSummary} · ${chunkSummary}`;
}

function currentPopulationHeatmap(hudRuntime: HudRuntimeModule) {
  if (!latestSnapshot) {
    return null;
  }

  const controlled = controlledEntity();
  return hudRuntime.buildPopulationHeatmapModel(latestSnapshot.population, {
    chunkKey: controlled?.metadata.chunkKey ?? null,
    regionId: controlled?.metadata.regionId ?? null
  });
}

function targetableEntities(): NetworkEntitySnapshot[] {
  if (!latestSnapshot) {
    return [];
  }

  const selfId = liveConnectionStatus?.controlledEntity ?? null;
  const selfEntity = controlledEntity();

  return latestSnapshot.entities
    .filter((entity) => entity.id !== selfId)
    .filter((entity) => {
      if (entity.metadata.kind === "Scenery") {
        return false;
      }
      if (entity.metadata.kind !== "Unknown") {
        return true;
      }
      const label = entity.label?.toLowerCase() ?? "";
      return !label.includes("wall") && !label.includes("obstacle");
    })
    .sort((left, right) => {
      if (!selfEntity) {
        return left.id - right.id;
      }
      const leftDistance = Math.hypot(
        left.position[0] - selfEntity.position[0],
        left.position[1] - selfEntity.position[1]
      );
      const rightDistance = Math.hypot(
        right.position[0] - selfEntity.position[0],
        right.position[1] - selfEntity.position[1]
      );
      if (leftDistance !== rightDistance) {
        return leftDistance - rightDistance;
      }
      return left.id - right.id;
    });
}

function currentFrameCameraState() {
  return currentRenderedCamera;
}

function shortestAngleDelta(current: number, target: number): number {
  let delta = target - current;
  while (delta > Math.PI) {
    delta -= Math.PI * 2;
  }
  while (delta < -Math.PI) {
    delta += Math.PI * 2;
  }
  return delta;
}

function stepAngleToward(current: number, target: number, sharpness: number, deltaMs: number): number {
  const alpha = 1 - Math.exp(-(sharpness * deltaMs) / 1000);
  return current + shortestAngleDelta(current, target) * alpha;
}

function stepScalarToward(current: number, target: number, sharpness: number, deltaMs: number): number {
  const alpha = 1 - Math.exp(-(sharpness * deltaMs) / 1000);
  return current + (target - current) * alpha;
}

function clampScalar(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function blendAngle(current: number, target: number, alpha: number): number {
  return current + shortestAngleDelta(current, target) * alpha;
}

function syncCameraRig(camera: CameraState | null): void {
  if (!camera) {
    return;
  }

  if (!cameraRig.initialized) {
    cameraRig.initialized = true;
    cameraRig.yaw = camera.rotation;
    cameraRig.desiredYaw = camera.rotation;
    cameraRig.pitch = camera.pitch ?? 0.34;
    cameraRig.desiredPitch = camera.pitch ?? 0.34;
    cameraRig.zoom = camera.zoom;
    cameraRig.desiredZoom = camera.zoom;
  }
}

function updateCameraRig(deltaMs: number): void {
  if (!cameraRig.initialized) {
    return;
  }

  cameraRig.yaw = stepAngleToward(cameraRig.yaw, cameraRig.desiredYaw, 14, deltaMs);
  cameraRig.pitch = stepScalarToward(cameraRig.pitch, cameraRig.desiredPitch, 16, deltaMs);
  cameraRig.zoom = stepScalarToward(cameraRig.zoom, cameraRig.desiredZoom, 12, deltaMs);
}

function applyKeyboardCameraRig(deltaMs: number): void {
  if (!cameraRig.initialized) {
    return;
  }

  const direction = cameraDirectionInput(pressedKeys);
  if (direction.yaw === 0 && direction.pitch === 0) {
    return;
  }

  const seconds = deltaMs / 1000;
  cameraRig.desiredYaw += direction.yaw * seconds * 1.95;
  cameraRig.desiredPitch = Math.max(
    0.18,
    Math.min(0.72, cameraRig.desiredPitch + direction.pitch * seconds * 1.15)
  );
}

function currentCameraYaw(): number {
  return cameraRig.initialized
    ? cameraRig.yaw
    : currentFrameCameraState()?.rotation ?? 0;
}

async function renderableThreeFrame(baseFrame: ThreeJsWebGpuFrame): Promise<ThreeJsWebGpuFrame> {
  const [{ withCombatFocusMarkers, withInteractionMarkers, withWorldEventMarkers }, { computeCombatCameraPressure }] =
    await Promise.all([loadContractsModule(), loadFramePlanModule()]);
  syncCameraRig(baseFrame.camera);
  const localPresentation = localSandbox?.presentation() ?? null;
  const controlled = controlledEntity();
  const target = selectedTarget();
  const nowSeconds = currentRuntimeNowMs() / 1000;
  const speed = controlled
    ? Math.hypot(controlled.velocity[0], controlled.velocity[1])
    : 0;
  const leadDistance = Math.min(1.4, speed * 0.16);
  const combatCamera = computeCombatCameraPressure(
    controlled
      ? {
          position: controlled.position,
          health: controlled.health,
          maxHealth: controlled.maxHealth
        }
      : null,
    target
      ? {
          position: target.position,
          canAttack: target.metadata.interaction.canAttack
        }
      : null,
    cameraImpact
  );
  const combatPressure = combatCamera.combatPressure;
  const closeRangeBlend = combatCamera.closeRangeBlend;
  const leadScale = 1 + swimCameraBlend * 0.14 - combatPressure * 0.06;
  const leadX =
    controlled && speed > 0.05
      ? (controlled.velocity[0] / speed) * leadDistance * leadScale
      : 0;
  const leadY =
    controlled && speed > 0.05
      ? (controlled.velocity[1] / speed) * leadDistance * leadScale
      : 0;
  const shakeYaw = Math.sin(nowSeconds * 33 + 0.8) * cameraImpact * 0.022;
  const shakePitch = Math.sin(nowSeconds * 27 + 1.7) * cameraImpact * 0.014;
  const baseFocusHeight = baseFrame.camera.focusHeight ?? 2.2;
  const baseFollowDistance = baseFrame.camera.followDistance ?? 13.5;
  const baseShoulderOffset = baseFrame.camera.shoulderOffset ?? 0.9;
  const swimPitchOffset = swimCameraBlend * 0.068;
  const swimZoomOffset = swimCameraBlend * 0.082;
  const swimDistanceOffset = swimCameraBlend * 1.6;
  const swimShoulderOffset = baseShoulderOffset - swimCameraBlend * 0.42;
  const combatFovKick = combatPressure * 2.4 + cameraImpact * 3 + closeRangeBlend * 0.9;
  const swimFocusHeight = baseFocusHeight + swimCameraBlend * 0.12 + closeRangeBlend * 0.08;
  const swimFollowDistance =
    baseFollowDistance +
    swimDistanceOffset -
    combatPressure * 0.65 -
    closeRangeBlend * 0.95;
  const blendedShoulderOffset =
    baseShoulderOffset +
    (swimShoulderOffset - baseShoulderOffset) * swimCameraBlend +
    closeRangeBlend * 0.14;
  const showcaseIntroBlend =
    localPresentation?.presetId === "bootstrap-showcase" &&
    !showcaseIntroDismissed &&
    latestSnapshot != null &&
    latestSnapshot.tick < 210
      ? 1 - latestSnapshot.tick / 210
      : 0;
  const quietShake = 1 - showcaseIntroBlend * 0.85;
  const introLeadX = leadX + 1.6;
  const introLeadY = leadY + 0.65;
  const baseRotation = cameraRig.yaw + shakeYaw * quietShake;
  const basePitch = clampScalar(
    cameraRig.pitch +
      swimPitchOffset +
      shakePitch * quietShake -
      combatPressure * 0.012 -
      closeRangeBlend * 0.018,
    0.18,
    0.76
  );
  const baseZoom = clampScalar(
    cameraRig.zoom -
      swimZoomOffset -
      combatPressure * 0.035 -
      closeRangeBlend * 0.045 +
      cameraImpact * 0.028,
    0.72,
    1.65
  );

  const interactionFrame = withInteractionMarkers(
    {
      ...baseFrame,
      camera: {
        ...baseFrame.camera,
        rotation: blendAngle(baseRotation, 0.24, showcaseIntroBlend),
        pitch: clampScalar(
          basePitch * (1 - showcaseIntroBlend) + 0.44 * showcaseIntroBlend,
          0.18,
          0.76
        ),
        zoom: clampScalar(
          baseZoom * (1 - showcaseIntroBlend) + 0.9 * showcaseIntroBlend,
          0.72,
          1.65
        ),
        fov: clampScalar(
          (baseFrame.camera.fov ?? 52) + combatFovKick * (1 - showcaseIntroBlend) + 1.8 * showcaseIntroBlend,
          50,
          62
        ),
        focusHeight: swimFocusHeight * (1 - showcaseIntroBlend) + 2.72 * showcaseIntroBlend,
        followDistance: swimFollowDistance * (1 - showcaseIntroBlend) + 10.8 * showcaseIntroBlend,
        shoulderOffset: blendedShoulderOffset * (1 - showcaseIntroBlend) + 0.34 * showcaseIntroBlend,
        leadX: leadX * (1 - showcaseIntroBlend) + introLeadX * showcaseIntroBlend,
        leadY: leadY * (1 - showcaseIntroBlend) + introLeadY * showcaseIntroBlend
      }
    },
    {
      moveTarget: clickMoveTarget,
      selectedTarget: target,
      controlledEntity: liveConnectionStatus?.controlledEntity ?? null,
      controlledSnapshot: controlled
    }
  );

  const combatFrame = withCombatFocusMarkers(interactionFrame, {
    selectedTarget: target,
    controlledSnapshot: controlled
  });

  return withWorldEventMarkers(combatFrame, {
    events: recentWorldEvents,
    worldSnapshot: latestSnapshot,
    currentTick: latestSnapshot?.tick ?? null
  });
}

function selectedTarget(): NetworkEntitySnapshot | null {
  syncSelectedTarget();
  if (selectedTargetId == null) {
    return null;
  }
  return targetableEntities().find((entity) => entity.id === selectedTargetId) ?? null;
}

function eventTouchesEntity(event: NetworkGameEvent, entityId: number | null): boolean {
  return entityId != null && event.entityIds.includes(entityId);
}

function applyAuthoritativeEventBatch(batch: NetworkEventBatch): void {
  if (batch.events.length === 0) {
    return;
  }

  renderer.notifyWorldEvents(batch.events);
  recentWorldEvents = [...recentWorldEvents, ...batch.events].slice(-6);

  const controlledId = liveConnectionStatus?.controlledEntity ?? null;
  const highlighted =
    [...batch.events]
      .reverse()
      .find(
        (event) =>
          eventTouchesEntity(event, controlledId) ||
          eventTouchesEntity(event, selectedTargetId)
      ) ?? batch.events.at(-1);

  if (highlighted) {
    const hudRuntime = loadedHudRuntimeModule();
    latestFeedback = hudRuntime
      ? hudRuntime.highlightEventFeedback(highlighted)
      : highlighted.summary;
  }

  for (const event of batch.events) {
    if (!eventTouchesEntity(event, controlledId) && !eventTouchesEntity(event, selectedTargetId)) {
      continue;
    }
    const normalized = event.kind.toLowerCase();
    if (normalized.includes("damage") || normalized.includes("kill") || normalized.includes("defeat")) {
      cameraImpact = Math.min(1.2, cameraImpact + 0.48);
    } else if (
      normalized.includes("capture") ||
      normalized.includes("summon") ||
      normalized.includes("loot") ||
      normalized.includes("gather")
    ) {
      cameraImpact = Math.min(1.2, cameraImpact + 0.22);
    }
  }
}

function formatRecentEventFeed(): string {
  if (recentWorldEvents.length === 0) {
    return "No authoritative events yet";
  }

  return recentWorldEvents
    .slice(-2)
    .map((event) => {
      const hudRuntime = loadedHudRuntimeModule();
      return hudRuntime ? hudRuntime.highlightEventFeedback(event) : event.summary;
    })
    .join(" · ");
}

function formatActionStatus(): string {
  if (latestActionStatus.lastRejectedReason) {
    return `rejected @ ${
      latestActionStatus.lastRejectedTick ?? "?"
    } · ${latestActionStatus.lastRejectedReason}`;
  }

  if (latestActionStatus.pendingCount > 0) {
    return `pending ${latestActionStatus.pendingCount} · ${
      latestActionStatus.lastActionSummary ?? "action"
    }`;
  }

  if (latestActionStatus.lastAcknowledgedTick != null) {
    return `acknowledged @ ${latestActionStatus.lastAcknowledgedTick} · ${
      latestActionStatus.lastActionSummary ?? "action"
    }`;
  }

  return "idle";
}

function syncSelectedTarget(): void {
  const candidates = targetableEntities();
  if (candidates.length === 0) {
    selectedTargetId = null;
    return;
  }

  if (selectedTargetId == null || !candidates.some((entity) => entity.id === selectedTargetId)) {
    selectedTargetId = candidates[0]?.id ?? null;
  }
}

function cycleTargetSelection(delta: number): void {
  const candidates = targetableEntities();
  if (candidates.length === 0) {
    selectedTargetId = null;
    return;
  }

  const currentIndex = candidates.findIndex((entity) => entity.id === selectedTargetId);
  const nextIndex =
    currentIndex === -1
      ? 0
      : (currentIndex + delta + candidates.length) % candidates.length;
  selectedTargetId = candidates[nextIndex]?.id ?? null;
}

function submitActions(actions: BrowserAction[]): void {
  if (liveClient) {
    if (!liveClient.submitActions(actions)) {
      latestFeedback = "Action could not be submitted";
    }
    return;
  }

  if (!localSandbox) {
    latestFeedback = "Direct-connect is not active";
    return;
  }

  if (!localSandbox.submitActions(actions)) {
    latestFeedback = "Action could not be submitted";
  } else {
    latestActionStatus = localSandbox.currentActionState();
  }
}

function movementDirection(): [number, number] | null {
  return cameraRelativeMovementDirection(pressedKeys, currentCameraYaw());
}

function submitImmediateClickMovement(
  targetPoint: [number, number],
  timestamp: number
): void {
  const controlled = controlledEntity();
  if (!controlled) {
    return;
  }

  const dx = targetPoint[0] - controlled.position[0];
  const dy = targetPoint[1] - controlled.position[1];
  const distance = Math.hypot(dx, dy);
  if (distance <= 0.08) {
    return;
  }

  const direction: [number, number] = [dx / distance, dy / distance];
  submitActions([{ kind: "move", direction }]);
  lastMovementSignature = `${direction[0].toFixed(3)}:${direction[1].toFixed(3)}`;
  lastMovementSubmitAtMs = timestamp;
}

function maybeSubmitMovement(timestamp: number): void {
  const direction = movementDirection();
  const controlled = controlledEntity();
  const clickDirection =
    !direction && clickMoveTarget && controlled
      ? (() => {
          const dx = clickMoveTarget[0] - controlled.position[0];
          const dy = clickMoveTarget[1] - controlled.position[1];
          const distance = Math.hypot(dx, dy);

          if (distance <= 0.7) {
            clickMoveTarget = null;
            latestFeedback = `Arrived at ${controlled.position[0].toFixed(1)}, ${controlled.position[1].toFixed(1)}`;
            return null;
          }

          return [dx / distance, dy / distance] as [number, number];
        })()
      : null;
  const activeDirection = direction ?? clickDirection;
  const signature = activeDirection
    ? `${activeDirection[0].toFixed(3)}:${activeDirection[1].toFixed(3)}`
    : "stop";
  const resendDue = timestamp - lastMovementSubmitAtMs >= 90;

  if (direction) {
    clickMoveTarget = null;
  }

  if (activeDirection) {
    if (signature !== lastMovementSignature || resendDue) {
      submitActions([{ kind: "move", direction: activeDirection }]);
      lastMovementSignature = signature;
      lastMovementSubmitAtMs = timestamp;
    }
    return;
  }

  if (lastMovementSignature !== "stop") {
    submitActions([{ kind: "stop" }]);
    lastMovementSignature = "stop";
    lastMovementSubmitAtMs = timestamp;
  }
}

function defaultPointerAction(target: NetworkEntitySnapshot): BrowserAction | null {
  if (target.metadata.interaction.canLoot) {
    return { kind: "loot", target: target.id };
  }
  if (target.metadata.interaction.canGather) {
    return {
      kind: "gatherResource",
      target: target.id,
      skill: target.metadata.resourceSkill ?? "Mining"
    };
  }
  if (target.metadata.interaction.canCapture) {
    return { kind: "captureCreature", target: target.id };
  }
  if (target.metadata.interaction.canAttack) {
    return { kind: "attackTarget", target: target.id };
  }
  if (target.metadata.interaction.canInteract || target.metadata.interaction.canInspect) {
    return { kind: "interactWith", target: target.id };
  }
  return null;
}

function submitTargetedAction(
  actionBuilder: (target: NetworkEntitySnapshot) => BrowserAction
): void {
  const target = selectedTarget();
  if (!target) {
    latestFeedback = "Select a target first";
    return;
  }
  submitActions([actionBuilder(target)]);
}

function handleGameplayKeyDown(event: KeyboardEvent): void {
  if (isEditableTarget(event.target)) {
    return;
  }

  if (isGameplayKeyCode(event.code)) {
    dismissShowcaseIntro();
  }

  switch (event.code) {
    case "Tab":
      event.preventDefault();
      cycleTargetSelection(event.shiftKey ? -1 : 1);
      return;
    case "Space":
      event.preventDefault();
      submitTargetedAction((target) => ({ kind: "attackTarget", target: target.id }));
      return;
    case "KeyE":
      submitTargetedAction((target) => ({ kind: "interactWith", target: target.id }));
      return;
    case "KeyG":
      submitTargetedAction((target) => ({
        kind: "gatherResource",
        target: target.id,
        skill: "Mining"
      }));
      return;
    case "KeyR":
      submitTargetedAction((target) => ({ kind: "loot", target: target.id }));
      return;
    case "KeyC":
      submitTargetedAction((target) => ({ kind: "captureCreature", target: target.id }));
      return;
    case "Digit1":
      submitActions([{ kind: "summonCompanion", slot: 0 }]);
      return;
    case "KeyF":
      submitActions([
        {
          kind: "commandCompanion",
          slot: 0,
          command: "Follow",
          target: selectedTarget()?.id ?? null
        }
      ]);
      return;
    case "KeyP":
      autoRetaliateEnabled = !autoRetaliateEnabled;
      submitActions([{ kind: "setAutoRetaliate", enabled: autoRetaliateEnabled }]);
      return;
    case "Enter":
      event.preventDefault();
      chatInputNode.focus();
      return;
    default:
      break;
  }

  if (event.code.startsWith("Key") || event.code.startsWith("Arrow")) {
    if (event.code.startsWith("Arrow")) {
      event.preventDefault();
    }
    pressedKeys.add(event.code);
  }
}

function handleGameplayKeyUp(event: KeyboardEvent): void {
  pressedKeys.delete(event.code);
}

window.addEventListener("keydown", (event) => {
  if (event.target === renderCanvas) {
    return;
  }
  if (!isEditableTarget(event.target) && isGameplayKeyCode(event.code)) {
    focusGameplaySurface(renderCanvas);
  }
  handleGameplayKeyDown(event);
});

window.addEventListener("keyup", (event) => {
  if (event.target === renderCanvas) {
    return;
  }
  handleGameplayKeyUp(event);
});

renderCanvas.addEventListener("keydown", (event) => {
  if (!isGameplayKeyCode(event.code)) {
    return;
  }
  handleGameplayKeyDown(event);
  event.stopPropagation();
});

renderCanvas.addEventListener("keyup", (event) => {
  if (!isGameplayKeyCode(event.code)) {
    return;
  }
  handleGameplayKeyUp(event);
  event.stopPropagation();
});

chatFormNode.addEventListener("submit", (event) => {
  event.preventDefault();
  const message = chatInputNode.value.trim();
  if (message.length === 0) {
    latestFeedback = "Type a message first";
    return;
  }

  submitActions([
    {
      kind: "speak",
      message,
      volume: "Normal"
    }
  ]);
  chatInputNode.value = "";
});

renderCanvas.addEventListener("pointerdown", async (event) => {
  focusGameplaySurface(renderCanvas);
  dismissShowcaseIntro();
  if (event.button === 2) {
    orbitPointerId = event.pointerId;
    orbitPointer = [event.clientX, event.clientY];
    renderCanvas.setPointerCapture(event.pointerId);
    event.preventDefault();
    return;
  }

  if (event.button !== 0) {
    return;
  }

  const camera = currentFrameCameraState();
  if (!camera) {
    return;
  }

  const { pickWorldGroundPoint, resolvePointerTarget } = await loadGroundPickingModule();

  const rect = renderCanvas.getBoundingClientRect();
  const worldPoint = pickWorldGroundPoint(
    [event.clientX - rect.left, event.clientY - rect.top],
    { width: rect.width, height: rect.height },
    camera
  );
  if (!worldPoint) {
    return;
  }

  const target = resolvePointerTarget(targetableEntities(), worldPoint);
  if (target) {
    selectedTargetId = target.id;
    clickMoveTarget = null;
    const action = event.detail >= 2 ? defaultPointerAction(target) : null;
    latestFeedback =
      action != null ? `Default action on ${target.label}` : `Selected ${target.label}`;
    if (action) {
      submitActions([action]);
    }
    return;
  }

  selectedTargetId = null;
  clickMoveTarget = worldPoint;
  lastMovementSignature = "stop";
  submitImmediateClickMovement(worldPoint, currentRuntimeNowMs());
  latestFeedback = `Move order · ${worldPoint[0].toFixed(1)}, ${worldPoint[1].toFixed(1)}`;
});

renderCanvas.addEventListener("pointermove", (event) => {
  if (orbitPointerId !== event.pointerId || orbitPointer == null) {
    return;
  }

  dismissShowcaseIntro();
  const deltaX = event.clientX - orbitPointer[0];
  const deltaY = event.clientY - orbitPointer[1];
  orbitPointer = [event.clientX, event.clientY];
  cameraRig.desiredYaw -= deltaX * 0.008;
  cameraRig.desiredPitch = Math.max(0.18, Math.min(0.7, cameraRig.desiredPitch - deltaY * 0.0035));
});

renderCanvas.addEventListener("pointerup", (event) => {
  if (orbitPointerId !== event.pointerId) {
    return;
  }

  renderCanvas.releasePointerCapture(event.pointerId);
  orbitPointerId = null;
  orbitPointer = null;
});

renderCanvas.addEventListener("pointercancel", (event) => {
  if (orbitPointerId !== event.pointerId) {
    return;
  }

  orbitPointerId = null;
  orbitPointer = null;
});

renderCanvas.addEventListener("wheel", (event) => {
  focusGameplaySurface(renderCanvas);
  dismissShowcaseIntro();
  cameraRig.desiredZoom = Math.max(
    0.72,
    Math.min(1.65, cameraRig.desiredZoom - event.deltaY * 0.0009)
  );
  event.preventDefault();
}, { passive: false });

renderCanvas.addEventListener("contextmenu", (event) => {
  event.preventDefault();
});

window.addEventListener("blur", () => {
  clearPressedKeys();
});

document.addEventListener("visibilitychange", () => {
  if (document.visibilityState !== "visible") {
    clearPressedKeys();
  }
});

telemetryToggleButton.addEventListener("click", () => {
  const debugRuntime = loadedDebugRuntimeModule();
  if (debugRuntime) {
    debugRuntime.setTelemetryEnabled(telemetryState, !telemetryState.enabled);
  } else {
    setTelemetryEnabledFallback(telemetryState, !telemetryState.enabled);
    if (telemetryState.enabled) {
      void loadDebugRuntimeModule();
    }
  }
  liveClient?.setDebugTelemetry(telemetryState.enabled);
});
telemetryPrevButton.addEventListener("click", () => {
  const debugRuntime = loadedDebugRuntimeModule();
  if (debugRuntime) {
    debugRuntime.cycleTelemetrySelection(telemetryState, -1);
    return;
  }
  void loadDebugRuntimeModule();
});
telemetryNextButton.addEventListener("click", () => {
  const debugRuntime = loadedDebugRuntimeModule();
  if (debugRuntime) {
    debugRuntime.cycleTelemetrySelection(telemetryState, 1);
    return;
  }
  void loadDebugRuntimeModule();
});

window.podRender = {
  render(frame: string) {
    manualFrameOverride = true;
    latestFrame = frame;
    currentRenderedCamera = null;
    liveFrameSource = "legacy";
    frameSourceNode.textContent = "legacy pod-render frame";
  },
  renderThreeJsWebGpuFrame(frame: string) {
    manualFrameOverride = true;
    latestFrame = frame;
    currentRenderedCamera = null;
    liveFrameSource = "threejs";
    frameSourceNode.textContent = "Three.js WebGPU frame";
  },
  renderTickTelemetry(frame: string) {
    void applyTickTelemetryFrame(frame);
  },
  renderDebugDocument(document: string) {
    void applyLiveDebugDocument(document);
  },
  renderReplayDocument(document: string) {
    void applyReplayDocument(document);
  },
  renderShardIncidentSummary(document: string) {
    void applyShardIncidentSummaryDocument(document);
  },
  streamReplayDocument(document: string) {
    void applyLiveDebugDocument(document);
  },
  streamShardIncidentSummary(document: string) {
    void applyLiveDebugDocument(document);
  },
  resetTelemetry() {
    const debugRuntime = loadedDebugRuntimeModule();
    if (debugRuntime) {
      debugRuntime.resetTelemetry(telemetryState);
    } else {
      resetTelemetryFallback(telemetryState);
    }
    renderer.clearTelemetryTrail();
  },
  resetDemo() {
    manualFrameOverride = false;
    recentWorldEvents = [];
    latestReplaySummary = null;
    latestIncidentSummary = null;
    lastPublishedDebugFocusEntityId = undefined;
    const debugRuntime = loadedDebugRuntimeModule();
    if (debugRuntime) {
      debugRuntime.resetLiveDebugState(liveDebugState);
    } else {
      liveDebugState.latestToolEventSummary = null;
      liveDebugState.latestRollupSummary = null;
      liveDebugState.latestFocusedSummary = null;
      liveDebugState.latestTransportSummary = null;
      liveDebugState.liveReplayDocuments = 0;
      liveDebugState.liveIncidentDocuments = 0;
      liveDebugState.liveTransportDocuments = 0;
      liveDebugState.toolEventsByEntity.clear();
      liveDebugState.rollupsByEntity.clear();
      liveDebugState.focusedSummariesByEntity.clear();
    }
    if (localSandbox) {
      localSandbox.reset();
      showcaseIntroDismissed = false;
      runtimeNowOverrideMs = initialFixedTimeMs;
      cameraRig.initialized = false;
      cameraRig.yaw = 0;
      cameraRig.desiredYaw = 0;
      cameraRig.pitch = 0.34;
      cameraRig.desiredPitch = 0.34;
      cameraRig.zoom = 1.08;
      cameraRig.desiredZoom = 1.08;
      latestFeedback = localSandbox.presentation().readyFeedback;
      clickMoveTarget = null;
      currentRenderedCamera = null;
      lastTickTimestamp = currentRuntimeNowMs();
      void refreshLocalSandboxFrame();
      renderTelemetryHud();
      return;
    }
    latestFrame = null;
    currentRenderedCamera = null;
    liveFrameSource = "demo";
    frameSourceNode.textContent = "demo frame";
  },
  requestGameplayFocus() {
    return focusGameplaySurface(renderCanvas);
  },
  getBackend() {
    return renderer.backend;
  },
  getStats() {
    return renderer.getStats();
  },
  getTelemetryStats() {
    const debugRuntime = loadedDebugRuntimeModule();
    return debugRuntime
      ? debugRuntime.telemetryStats(telemetryState)
      : telemetryStatsFallback(telemetryState);
  },
  getGameplayState() {
    const controlled = controlledEntity();
    return {
      renderThread: renderer.getStats().renderThread,
      frameSource: liveFrameSource,
      worldMode: runtimeConfig ? "authoritative-shard" : localSandbox?.presentation().mode ?? null,
      worldName: runtimeConfig ? null : localSandbox?.presentation().worldName ?? null,
      focused: document.activeElement === renderCanvas,
      cameraYaw: cameraRig.yaw,
      cameraPitch: cameraRig.pitch,
      cameraZoom: cameraRig.zoom,
      controlledEntityId: liveConnectionStatus?.controlledEntity ?? null,
      controlledPosition: controlled
        ? ([controlled.position[0], controlled.position[1]] as [number, number])
        : null,
      controlledAnimationSetId:
        controlled?.metadata.actorPresentation?.animationSetId ?? null,
      controlledSurfaceMode: surfaceModeFromEntity(controlled),
      selectedTargetId,
      clickMoveTarget,
      movementSignature: lastMovementSignature,
      latestFeedback
    };
  },
  getReplaySummary() {
    return latestReplaySummary;
  },
  getIncidentSummary() {
    return latestIncidentSummary;
  }
};

window.render_game_to_text = () => {
  if (latestSnapshot) {
    return renderGameToText(
      latestSnapshot,
      liveConnectionStatus?.controlledEntity ?? null,
      selectedTargetId,
      latestActionStatus,
      latestFeedback,
      recentWorldEvents,
      localSandbox?.companionRoster() ?? [],
      localSandbox?.currentDebugState() ?? {
        activeChunkKeys: [],
        currentRegionId: null,
        currentRegionName: null,
        questGraphs: [],
        factionReputation: [],
        encounterTables: []
      },
      localSandbox?.presentation() ?? null
    );
  }

  return JSON.stringify({
    mode: liveFrameSource === "demo" ? "demo" : "bridge",
    feedback: latestFeedback,
    connection: liveConnectionStatus?.detail ?? "no active world",
    target: selectedTargetId
  });
};

async function applyTickTelemetryFrame(frame: string): Promise<void> {
  const { parseTickTelemetryEnvelope } = await loadContractsModule();
  const debugRuntime = await loadDebugRuntimeModule();
  debugRuntime.applyTickTelemetry(
    telemetryState,
    parseTickTelemetryEnvelope(frame)
  );
}

async function applyReplayDocument(document: string): Promise<void> {
  const { parseReplayFile, summarizeReplayFile } = await loadContractsModule();
  latestReplaySummary = summarizeReplayFile(parseReplayFile(document));
}

async function applyShardIncidentSummaryDocument(document: string): Promise<void> {
  const { parseShardIncidentSummary } = await loadContractsModule();
  latestIncidentSummary = parseShardIncidentSummary(document);
}

async function advanceInteractiveRuntime(deltaMs: number, timestamp: number): Promise<void> {
  applyKeyboardCameraRig(deltaMs);
  updateCameraRig(deltaMs);
  maybeSubmitMovement(timestamp);

  if (localSandbox && !manualFrameOverride) {
    localSandbox.step(deltaMs);
    await refreshLocalSandboxFrame();
  }

  const controlled = controlledEntity();
  const isSwimming = entityUsesSwimSurface(controlled);
  swimCameraBlend = stepScalarToward(swimCameraBlend, isSwimming ? 1 : 0, 6.4, deltaMs);
  cameraImpact = stepScalarToward(cameraImpact, 0, 9.2, deltaMs);

  await renderCurrentFrame(timestamp);
}

window.advanceTime = async (ms: number) => {
  if (localSandbox && !manualFrameOverride) {
    let remainingMs = Math.max(0, ms);
    while (remainingMs > 0) {
      const deltaMs = Math.min(remainingMs, LOCAL_SANDBOX_STEP_MS);
      const timestamp = lastTickTimestamp + deltaMs;
      if (runtimeNowOverrideMs != null) {
        runtimeNowOverrideMs = timestamp;
      }
      lastTickTimestamp = timestamp;
      await advanceInteractiveRuntime(deltaMs, timestamp);
      remainingMs -= deltaMs;
    }
    return;
  }

  await new Promise<void>((resolve) => {
    window.setTimeout(resolve, ms);
  });
};

async function applyLiveDebugDocument(document: string): Promise<void> {
  const { parseLiveDebugDocument, summarizeReplayFile } = await loadContractsModule();
  const debugRuntime = await loadDebugRuntimeModule();
  const parsed = parseLiveDebugDocument(document);
  debugRuntime.recordLiveDebugDocument(liveDebugState, parsed);

  switch (parsed.kind) {
    case "tickTelemetry":
      debugRuntime.applyTickTelemetry(telemetryState, parsed.payload);
      break;
    case "toolCallEvent":
      break;
    case "tickRollup":
      break;
    case "transport":
      break;
    case "focusedSummary":
      break;
    case "replay":
      latestReplaySummary = summarizeReplayFile(parsed.payload);
      break;
    case "incident":
      latestIncidentSummary = parsed.payload;
      break;
  }
}

async function refreshLocalSandboxFrame(): Promise<void> {
  if (!localSandbox) {
    return;
  }

  const batch = localSandbox.drainEventBatch();
  if (batch) {
    applyAuthoritativeEventBatch(batch);
  }

  latestSnapshot = localSandbox.snapshotState();
  latestActionStatus = localSandbox.currentActionState();
  await applyAuthoritativeSnapshot(
    latestSnapshot,
    {
      controlledEntity: localSandbox.controlledEntityId(),
      viewportWidth: renderCanvas.clientWidth || window.innerWidth,
      viewportHeight: renderCanvas.clientHeight || window.innerHeight
    },
    localSandbox.currentStatus(),
    localSandbox.presentation().frameSourceLabel
  );
}

let lastTickTimestamp = currentRuntimeNowMs();

async function tick(timestamp: number): Promise<void> {
  const effectiveTimestamp = runtimeNowOverrideMs ?? timestamp;
  const deltaMs = Math.min(Math.max(effectiveTimestamp - lastTickTimestamp, 0), 250);
  lastTickTimestamp = effectiveTimestamp;
  await advanceInteractiveRuntime(deltaMs, effectiveTimestamp);

  if (!interactiveRuntimePaused) {
    requestAnimationFrame((nextTimestamp) => {
      void tick(nextTimestamp);
    });
  }
}

async function renderCurrentFrame(timestamp: number): Promise<void> {
  if (lastTelemetryRevision !== telemetryState.revision) {
    const debugRuntime = loadedDebugRuntimeModule();
    if (debugRuntime) {
      const samples = telemetryState.enabled
        ? debugRuntime.selectedTrajectorySamples(telemetryState)
        : [];
      if (samples.length > 1) {
        renderer.setTelemetryTrail(samples);
      } else {
        renderer.clearTelemetryTrail();
      }
      lastTelemetryRevision = telemetryState.revision;
    } else if (telemetryState.enabled && telemetryState.history.length > 0) {
      void loadDebugRuntimeModule();
    }
  }

  if (latestFrame) {
    if (liveFrameSource === "threejs") {
      const frame =
        typeof latestFrame === "string"
          ? (await loadContractsModule()).parseThreeJsWebGpuFrame(latestFrame)
          : latestFrame;
      const renderFrame = await renderableThreeFrame(frame);
      currentRenderedCamera = renderFrame.camera;
      await renderer.applyFrame(renderFrame);
    } else if (typeof latestFrame === "string") {
      const parsedFrame = (await loadContractsModule()).parseRenderFrame(latestFrame);
      currentRenderedCamera = parsedFrame.camera;
      await renderer.applyLegacyFrame(parsedFrame);
    } else {
      const renderFrame = await renderableThreeFrame(latestFrame);
      currentRenderedCamera = renderFrame.camera;
      await renderer.applyFrame(renderFrame);
    }
  } else {
    const createDemoFrame = await loadDemoFrameFactory();
    const demoFrame = createDemoFrame(timestamp / 1000);
    currentRenderedCamera = demoFrame.camera;
    await renderer.applyFrame(demoFrame);
  }

  const stats = renderer.getStats();
  const hudRuntime = loadedHudRuntimeModule();
  runtimeStatsLabel.textContent = hudRuntime
    ? hudRuntime.compactRuntimeStats(stats)
    : `${stats.renderThread} · ${stats.triangles} tris · ${stats.drawCalls} draws`;
  renderTelemetryHud();
}

function renderTelemetryHud(): void {
  const debugRuntime = loadedDebugRuntimeModule();
  const stats = debugRuntime
    ? debugRuntime.telemetryStats(telemetryState)
    : telemetryStatsFallback(telemetryState);
  const target = selectedTarget();
  const controlled = controlledEntity();
  const debugFocusEntityId = target?.id ?? controlled?.id ?? null;
  if (liveClient && lastPublishedDebugFocusEntityId !== debugFocusEntityId) {
    liveClient.setDebugFocusEntity(debugFocusEntityId);
    lastPublishedDebugFocusEntityId = debugFocusEntityId;
  }
  const focusedDebugSummary = debugRuntime
    ? debugRuntime.selectedFocusedDebugSummary(liveDebugState, debugFocusEntityId)
    : liveDebugState.latestFocusedSummary;
  const latestTransportSummary = liveDebugState.latestTransportSummary;
  const focusedToolEvent = debugRuntime
    ? debugRuntime.selectedToolEventSummary(liveDebugState, debugFocusEntityId)
    : liveDebugState.latestToolEventSummary;
  const focusedRollup = debugRuntime
    ? debugRuntime.selectedTickRollupSummary(liveDebugState, debugFocusEntityId)
    : liveDebugState.latestRollupSummary;
  const hudRuntime = loadedHudRuntimeModule();

  if (!hudRuntime) {
    connectionNode.textContent = liveConnectionStatus?.detail ?? "demo scene";
    worldNode.textContent = liveConnectionStatus
      ? liveConnectionStatus.tick == null
        ? `awaiting snapshot · ${liveConnectionStatus.url}`
        : `tick ${liveConnectionStatus.tick} · ${liveConnectionStatus.entityCount} entities`
      : "demo scene";
    populationNode.textContent = currentPopulationSummary();
    populationHeatmapLegendNode.textContent = "loading tactical hud";
    const populationHeatmapContext = populationHeatmapCanvasNode.getContext("2d");
    populationHeatmapContext?.clearRect(
      0,
      0,
      populationHeatmapCanvasNode.width,
      populationHeatmapCanvasNode.height
    );
    targetNode.textContent = target?.label ?? "No target";
    affordanceNode.textContent = target?.metadata.kind ?? "No interactable target";
  } else {
    const populationHeatmap = currentPopulationHeatmap(hudRuntime);
    connectionNode.textContent = hudRuntime.formatConnectionSummary(
      liveConnectionStatus,
      latestTransportSummary
    );
    populationHeatmapLegendNode.textContent =
      hudRuntime.formatPopulationHeatmapLegend(populationHeatmap);
    hudRuntime.renderPopulationHeatmap(populationHeatmapCanvasNode, populationHeatmap);
    targetNode.textContent = hudRuntime.formatTargetSummary(target, controlled);
    affordanceNode.textContent = hudRuntime.describeTargetAffordances(target);
  }

  worldNode.textContent = liveConnectionStatus
    ? liveConnectionStatus.tick == null
      ? `awaiting snapshot · ${liveConnectionStatus.url}`
      : `tick ${liveConnectionStatus.tick} · ${liveConnectionStatus.entityCount} entities · ${
          liveConnectionStatus.controlledEntity == null
            ? "spectator"
            : `controlled E(${liveConnectionStatus.controlledEntity})`
        }`
    : "demo scene";
  populationNode.textContent = currentPopulationSummary();
  actionStatusNode.textContent = formatActionStatus();
  feedbackNode.textContent = latestFeedback;
  eventFeedNode.textContent = formatRecentEventFeed();
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
  telemetrySummaryNode.textContent = focusedDebugSummary
    ? `tick ${focusedDebugSummary.latest_tick} · focus E(${focusedDebugSummary.entity_id}) · ${focusedDebugSummary.total_distance.toFixed(
        2
      )}u · ${focusedDebugSummary.rejected_action_count} rejected`
    : stats.tick == null
      ? "Waiting for authoritative telemetry."
      : `tick ${stats.tick} · ${stats.visibleEntities} visible · ${stats.audibleEvents} audible · ${stats.messages} messages`;
  replaySummaryNode.textContent = latestReplaySummary
    ? `${latestReplaySummary.name} · ${latestReplaySummary.traceCount} traces · ${latestReplaySummary.trainingSampleCount} samples · ${latestReplaySummary.totalPathDistance.toFixed(
        2
      )}u`
    : "No replay summary loaded";
  incidentSummaryNode.textContent = latestIncidentSummary
    ? `${latestIncidentSummary.severity} · ${latestIncidentSummary.summary} · stream ${liveDebugState.liveIncidentDocuments}`
    : latestTransportSummary
      ? hudRuntime
        ? hudRuntime.formatTransportDebugSummary(
            latestTransportSummary,
            liveDebugState.liveTransportDocuments
          )
        : `transport · ${latestTransportSummary.client_count} clients · ${latestTransportSummary.total_pending_action_queue_depth} queued · ${latestTransportSummary.queue_pressure_client_count} pressured · ${latestTransportSummary.timed_out_clients} timed out · ${liveDebugState.liveTransportDocuments} samples`
      : "No shard incident summary loaded";
  toolEventSummaryNode.textContent = focusedToolEvent
    ? `E(${focusedToolEvent.agentEntityId}) · ${focusedToolEvent.toolName} · ${focusedToolEvent.status} · ${focusedToolEvent.latencyMs}ms${
        debugFocusEntityId != null ? " · focus" : ""
      }`
    : focusedDebugSummary?.latest_tool_name
      ? `E(${focusedDebugSummary.entity_id}) · ${focusedDebugSummary.latest_tool_name} · ${focusedDebugSummary.latest_tool_status ?? "Unknown"} · avg ${focusedDebugSummary.average_tool_latency_ms.toFixed(
          0
        )}ms${debugFocusEntityId != null ? " · focus" : ""}`
    : "No tool-call event loaded";
  rollupSummaryNode.textContent = focusedDebugSummary
    ? `E(${focusedDebugSummary.entity_id}) · ${focusedDebugSummary.visible_entity_count} visible · ${focusedDebugSummary.audible_event_count} audible · ${focusedDebugSummary.message_count} messages${
        debugFocusEntityId != null ? " · focus" : ""
      }`
    : focusedRollup
    ? `E(${focusedRollup.agentEntityId}) · ticks ${focusedRollup.tickStart}-${focusedRollup.tickEnd} · ${focusedRollup.totalDistance.toFixed(
        2
      )}u · ${focusedRollup.toolErrorCount} tool errors${
        debugFocusEntityId != null ? " · focus" : ""
      }`
    : "No telemetry rollup loaded";
  if (latestReplaySummary) {
    replaySummaryNode.textContent = `${latestReplaySummary.name} · ${latestReplaySummary.traceCount} traces · ${latestReplaySummary.trainingSampleCount} samples · ${latestReplaySummary.totalPathDistance.toFixed(
      2
    )}u · stream ${liveDebugState.liveReplayDocuments}`;
  }
}

liveClient?.connect();
if (localSandbox) {
  localSandbox.connect();
  void refreshLocalSandboxFrame();
  renderTelemetryHud();
}
lastTickTimestamp = currentRuntimeNowMs();
void tick(currentRuntimeNowMs());
