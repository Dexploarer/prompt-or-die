import {
  type BrowserAction,
  buildAuthoritativeWorldFrame,
  type CameraState,
  type NetworkEventBatch,
  type NetworkGameEvent,
  type NetworkEntitySnapshot,
  type NetworkWorldSnapshot,
  parseLiveDebugDocument,
  parseRenderFrame,
  parseReplayFile,
  parseShardIncidentSummary,
  parseThreeJsWebGpuFrame,
  parseTickTelemetryEnvelope,
  summarizeReplayFile,
  withInteractionMarkers,
  withWorldEventMarkers,
  type ThreeJsWebGpuFrame,
  type ReplaySummary,
  type ShardIncidentSummary
} from "./contracts";
import {
  describeTargetAffordances,
  formatTargetSummary
} from "./affordances";
import {
  cameraRelativeMovementDirection,
  focusGameplaySurface,
  isGameplayKeyCode,
  pickWorldGroundPoint,
  resolvePointerTarget
} from "./controls";
import {
  initialHudStateFromLocation,
  PodWebDirectConnectClient,
  type DirectConnectActionState,
  runtimeConfigFromLocation,
  type DirectConnectStatus
} from "./direct-connect";
import { PodWebLocalWorld, renderGameToText } from "./local-world";
import { createDemoFrame } from "./sample-frame";
import { createPodRenderRuntime, type PodThreeRenderRuntime } from "./render-runtime";
import {
  applyTickTelemetry,
  createTelemetryOverlayState,
  cycleTelemetrySelection,
  resetTelemetry,
  selectedTrajectorySamples,
  setTelemetryEnabled,
  telemetryStats
} from "./telemetry";
import {
  buildPopulationHeatmapModel,
  formatPopulationHeatmapLegend,
  renderPopulationHeatmap
} from "./population-heatmap";
import {
  createLiveDebugState,
  recordLiveDebugDocument,
  resetLiveDebugState,
  selectedTickRollupSummary,
  selectedToolEventSummary
} from "./live-debug";

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
      getTelemetryStats: () => ReturnType<typeof telemetryStats>;
      getGameplayState: () => {
        renderThread: string;
        frameSource: string;
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
focusGameplaySurface(renderCanvas);
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

feedbackNode.textContent = bootHudState.feedback;
connectionNode.textContent = bootHudState.connectionBadge;
worldNode.textContent = bootHudState.worldLabel;
populationNode.textContent = bootHudState.populationLabel;
frameSourceNode.textContent = bootHudState.frameSourceLabel;

const renderer = await createPodRenderRuntime(renderCanvas);
backendLabel.textContent = renderer.backend;
qualityLabel.textContent = renderer.qualityPreset;
const runtimeStatsLabel = statsLabel;
const telemetryState = createTelemetryOverlayState(300);
const runtimeConfig = runtimeConfigFromLocation(window.location);
const offlinePlayerName =
  new URLSearchParams(window.location.search).get("player")?.trim() || "WebPlayer";
const localSandbox = runtimeConfig ? null : new PodWebLocalWorld(offlinePlayerName);

let liveFrameSource: "demo" | "legacy" | "threejs" = "demo";
let latestFrame: string | ReturnType<typeof buildAuthoritativeWorldFrame> | null = null;
let lastTelemetryRevision = -1;
let latestReplaySummary: ReplaySummary | null = null;
let latestIncidentSummary: ShardIncidentSummary | null = null;
const liveDebugState = createLiveDebugState();
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
  : "Local sandbox ready: click terrain to move, right-drag the camera, wheel to zoom, WASD steer, Tab target, and double-click targets for default actions";
let recentWorldEvents: NetworkGameEvent[] = [];
let manualFrameOverride = false;
let liveConnectionStatus: DirectConnectStatus | null = runtimeConfig
  ? {
      phase: "idle",
      detail: `Waiting to connect to ${runtimeConfig.url}`,
      url: runtimeConfig.url,
      tick: null,
      entityCount: 0,
      controlledEntity: null,
      authoritativeDigest: null
    }
  : null;

if (runtimeConfig?.debugTelemetry) {
  setTelemetryEnabled(telemetryState, true);
}

const liveClient = runtimeConfig
  ? new PodWebDirectConnectClient(runtimeConfig, {
      onFrame(snapshot, frameOptions, status) {
        latestSnapshot = snapshot;
        syncSelectedTarget();
        latestFrame = buildAuthoritativeWorldFrame(snapshot, {
          ...frameOptions,
          viewportWidth: renderCanvas.clientWidth || window.innerWidth,
          viewportHeight: renderCanvas.clientHeight || window.innerHeight
        });
        liveFrameSource = "threejs";
        frameSourceNode.textContent = "authoritative websocket";
        liveConnectionStatus = status;
      },
      onEventBatch(batch) {
        applyAuthoritativeEventBatch(batch);
      },
      onDebugDocument(document) {
        applyLiveDebugDocument(document);
      },
      onActionState(state) {
        latestActionStatus = state;
      },
      onStatus(status) {
        liveConnectionStatus = status;
      }
    })
  : null;
const pressedKeys = new Set<string>();
let selectedTargetId: number | null = null;
let autoRetaliateEnabled = true;
let clickMoveTarget: [number, number] | null = null;
let lastMovementSignature = "stop";
let lastMovementSubmitAtMs = 0;
let orbitPointerId: number | null = null;
let orbitPointer: [number, number] | null = null;

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

function currentPopulationHeatmap() {
  if (!latestSnapshot) {
    return null;
  }

  const controlled = controlledEntity();
  return buildPopulationHeatmapModel(latestSnapshot.population, {
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
  if (!latestFrame) {
    return null;
  }

  if (typeof latestFrame === "string") {
    if (liveFrameSource === "threejs") {
      return renderableThreeFrame(parseThreeJsWebGpuFrame(latestFrame)).camera;
    }
    return parseRenderFrame(latestFrame).camera;
  }

  return liveFrameSource === "threejs"
    ? renderableThreeFrame(latestFrame).camera
    : latestFrame.camera;
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

function currentCameraYaw(): number {
  return cameraRig.initialized
    ? cameraRig.yaw
    : currentFrameCameraState()?.rotation ?? 0;
}

function renderableThreeFrame(baseFrame: ThreeJsWebGpuFrame): ThreeJsWebGpuFrame {
  syncCameraRig(baseFrame.camera);
  const controlled = controlledEntity();
  const target = selectedTarget();
  const speed = controlled
    ? Math.hypot(controlled.velocity[0], controlled.velocity[1])
    : 0;
  const leadDistance = Math.min(1.4, speed * 0.16);
  const leadX =
    controlled && speed > 0.05 ? (controlled.velocity[0] / speed) * leadDistance : 0;
  const leadY =
    controlled && speed > 0.05 ? (controlled.velocity[1] / speed) * leadDistance : 0;

  const interactionFrame = withInteractionMarkers(
    {
      ...baseFrame,
      camera: {
        ...baseFrame.camera,
        rotation: cameraRig.yaw,
        pitch: cameraRig.pitch,
        zoom: cameraRig.zoom,
        focusHeight: baseFrame.camera.focusHeight ?? 2.2,
        followDistance: baseFrame.camera.followDistance ?? 13.5,
        shoulderOffset: baseFrame.camera.shoulderOffset ?? 0.9,
        leadX,
        leadY
      }
    },
    {
      moveTarget: clickMoveTarget,
      selectedTarget: target,
      controlledEntity: liveConnectionStatus?.controlledEntity ?? null,
      controlledSnapshot: controlled
    }
  );

  return withWorldEventMarkers(interactionFrame, {
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
    latestFeedback = `[${highlighted.kind}] ${highlighted.summary}`;
  }
}

function formatRecentEventFeed(): string {
  if (recentWorldEvents.length === 0) {
    return "No authoritative events yet";
  }

  return recentWorldEvents
    .map((event) => `[${event.tick}] ${event.summary}`)
    .join("\n");
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

renderCanvas.addEventListener("pointerdown", (event) => {
  focusGameplaySurface(renderCanvas);
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
  latestFeedback = `Move order · ${worldPoint[0].toFixed(1)}, ${worldPoint[1].toFixed(1)}`;
});

renderCanvas.addEventListener("pointermove", (event) => {
  if (orbitPointerId !== event.pointerId || orbitPointer == null) {
    return;
  }

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
  setTelemetryEnabled(telemetryState, !telemetryState.enabled);
  liveClient?.setDebugTelemetry(telemetryState.enabled);
});
telemetryPrevButton.addEventListener("click", () => {
  cycleTelemetrySelection(telemetryState, -1);
});
telemetryNextButton.addEventListener("click", () => {
  cycleTelemetrySelection(telemetryState, 1);
});

window.podRender = {
  render(frame: string) {
    manualFrameOverride = true;
    latestFrame = frame;
    liveFrameSource = "legacy";
    frameSourceNode.textContent = "legacy pod-render frame";
  },
  renderThreeJsWebGpuFrame(frame: string) {
    manualFrameOverride = true;
    latestFrame = frame;
    liveFrameSource = "threejs";
    frameSourceNode.textContent = "Three.js WebGPU frame";
  },
  renderTickTelemetry(frame: string) {
    applyTickTelemetry(telemetryState, parseTickTelemetryEnvelope(frame));
  },
  renderDebugDocument(document: string) {
    applyLiveDebugDocument(document);
  },
  renderReplayDocument(document: string) {
    latestReplaySummary = summarizeReplayFile(parseReplayFile(document));
  },
  renderShardIncidentSummary(document: string) {
    latestIncidentSummary = parseShardIncidentSummary(document);
  },
  streamReplayDocument(document: string) {
    applyLiveDebugDocument(document);
  },
  streamShardIncidentSummary(document: string) {
    applyLiveDebugDocument(document);
  },
  resetTelemetry() {
    resetTelemetry(telemetryState);
    renderer.clearTelemetryTrail();
  },
  resetDemo() {
    manualFrameOverride = false;
    recentWorldEvents = [];
    latestReplaySummary = null;
    latestIncidentSummary = null;
    resetLiveDebugState(liveDebugState);
    if (localSandbox) {
      localSandbox.reset();
      cameraRig.initialized = false;
      cameraRig.yaw = 0;
      cameraRig.desiredYaw = 0;
      cameraRig.pitch = 0.34;
      cameraRig.desiredPitch = 0.34;
      cameraRig.zoom = 1.08;
      cameraRig.desiredZoom = 1.08;
      latestFeedback =
        "Local sandbox ready: click terrain to move, right-drag the camera, wheel to zoom, WASD steer, Tab target, and double-click targets for default actions";
      clickMoveTarget = null;
      refreshLocalSandboxFrame();
      renderTelemetryHud();
      return;
    }
    latestFrame = null;
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
    return telemetryStats(telemetryState);
  },
  getGameplayState() {
    const controlled = controlledEntity();
    return {
      renderThread: renderer.getStats().renderThread,
      frameSource: liveFrameSource,
      focused: document.activeElement === renderCanvas,
      controlledEntityId: liveConnectionStatus?.controlledEntity ?? null,
      controlledPosition: controlled
        ? ([controlled.position[0], controlled.position[1]] as [number, number])
        : null,
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
      }
    );
  }

  return JSON.stringify({
    mode: liveFrameSource === "demo" ? "demo" : "bridge",
    feedback: latestFeedback,
    connection: liveConnectionStatus?.detail ?? "no active world",
    target: selectedTargetId
  });
};

window.advanceTime = async (ms: number) => {
  if (localSandbox && !manualFrameOverride) {
    localSandbox.step(ms);
    refreshLocalSandboxFrame();
    await renderCurrentFrame(performance.now());
    return;
  }

  await new Promise<void>((resolve) => {
    window.setTimeout(resolve, ms);
  });
};

function applyLiveDebugDocument(document: string): void {
  const parsed = parseLiveDebugDocument(document);
  recordLiveDebugDocument(liveDebugState, parsed);

  switch (parsed.kind) {
    case "tickTelemetry":
      applyTickTelemetry(telemetryState, parsed.payload);
      break;
    case "toolCallEvent":
      break;
    case "tickRollup":
      break;
    case "replay":
      latestReplaySummary = summarizeReplayFile(parsed.payload);
      break;
    case "incident":
      latestIncidentSummary = parsed.payload;
      break;
  }
}

function refreshLocalSandboxFrame(): void {
  if (!localSandbox) {
    return;
  }

  const batch = localSandbox.drainEventBatch();
  if (batch) {
    applyAuthoritativeEventBatch(batch);
  }

  latestSnapshot = localSandbox.snapshotState();
  latestActionStatus = localSandbox.currentActionState();
  liveConnectionStatus = localSandbox.currentStatus();
  syncSelectedTarget();
  latestFrame = buildAuthoritativeWorldFrame(latestSnapshot, {
    controlledEntity: localSandbox.controlledEntityId(),
    viewportWidth: renderCanvas.clientWidth || window.innerWidth,
    viewportHeight: renderCanvas.clientHeight || window.innerHeight
  });
  liveFrameSource = "threejs";
  frameSourceNode.textContent = "local sandbox shard";
}

let lastTickTimestamp = performance.now();

async function tick(timestamp: number): Promise<void> {
  const deltaMs = Math.min(Math.max(timestamp - lastTickTimestamp, 0), 250);
  lastTickTimestamp = timestamp;
  updateCameraRig(deltaMs);
  maybeSubmitMovement(timestamp);

  if (localSandbox && !manualFrameOverride) {
    localSandbox.step(deltaMs);
    refreshLocalSandboxFrame();
  }

  await renderCurrentFrame(timestamp);

  requestAnimationFrame((nextTimestamp) => {
    void tick(nextTimestamp);
  });
}

async function renderCurrentFrame(timestamp: number): Promise<void> {
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

  if (latestFrame) {
    if (liveFrameSource === "threejs") {
      const frame =
        typeof latestFrame === "string" ? parseThreeJsWebGpuFrame(latestFrame) : latestFrame;
      await renderer.applyFrame(renderableThreeFrame(frame));
    } else if (typeof latestFrame === "string") {
      await renderer.applyLegacyFrame(parseRenderFrame(latestFrame));
    } else {
      await renderer.applyFrame(renderableThreeFrame(latestFrame));
    }
  } else {
    await renderer.applyFrame(createDemoFrame(timestamp / 1000));
  }

  const stats = renderer.getStats();
  runtimeStatsLabel.textContent = `${stats.drawCalls} calls · ${stats.triangles} tris · ${stats.pixelRatio.toFixed(
    2
  )}x DPR · ${stats.frameMs.toFixed(1)}ms · ${stats.renderThread} thread · ${stats.environmentPreset} ${stats.timeOfDayHours.toFixed(
    1
  )}h · ${stats.landscapeMode} · ${stats.waterMode} · ambient ${stats.ambientInstances} · chunks ${
    stats.visibleWorldChunks
  } visible / ${stats.preloadedWorldChunks} warm · assets ${
    stats.residentGeometryAssets + stats.residentSpriteAssets
  } resident / ${stats.pendingGeometryAssets + stats.pendingSpriteAssets} pending`;
  renderTelemetryHud();
}

function renderTelemetryHud(): void {
  const stats = telemetryStats(telemetryState);
  const target = selectedTarget();
  const controlled = controlledEntity();
  const debugFocusEntityId = target?.id ?? controlled?.id ?? null;
  const focusedToolEvent = selectedToolEventSummary(liveDebugState, debugFocusEntityId);
  const focusedRollup = selectedTickRollupSummary(liveDebugState, debugFocusEntityId);
  const populationHeatmap = currentPopulationHeatmap();
  connectionNode.textContent = liveConnectionStatus
    ? `${liveConnectionStatus.phase} · ${liveConnectionStatus.detail}`
    : "offline demo / bridge mode";
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
  populationHeatmapLegendNode.textContent =
    formatPopulationHeatmapLegend(populationHeatmap);
  renderPopulationHeatmap(populationHeatmapCanvasNode, populationHeatmap);
  targetNode.textContent = formatTargetSummary(target, controlled);
  affordanceNode.textContent = describeTargetAffordances(target);
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
  telemetrySummaryNode.textContent =
    stats.tick == null
      ? "Waiting for authoritative telemetry."
      : `tick ${stats.tick} · ${stats.visibleEntities} visible · ${stats.audibleEvents} audible · ${stats.messages} messages`;
  replaySummaryNode.textContent = latestReplaySummary
    ? `${latestReplaySummary.name} · ${latestReplaySummary.traceCount} traces · ${latestReplaySummary.trainingSampleCount} samples · ${latestReplaySummary.totalPathDistance.toFixed(
        2
      )}u`
    : "No replay summary loaded";
  incidentSummaryNode.textContent = latestIncidentSummary
    ? `${latestIncidentSummary.severity} · ${latestIncidentSummary.summary} · stream ${liveDebugState.liveIncidentDocuments}`
    : "No shard incident summary loaded";
  toolEventSummaryNode.textContent = focusedToolEvent
    ? `E(${focusedToolEvent.agentEntityId}) · ${focusedToolEvent.toolName} · ${focusedToolEvent.status} · ${focusedToolEvent.latencyMs}ms${
        debugFocusEntityId != null ? " · focus" : ""
      }`
    : "No tool-call event loaded";
  rollupSummaryNode.textContent = focusedRollup
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
  refreshLocalSandboxFrame();
  renderTelemetryHud();
}
void tick(performance.now());
