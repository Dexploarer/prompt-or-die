import {
  type BrowserAction,
  buildAuthoritativeWorldFrame,
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
  summarizeAgentTickRollup,
  summarizeAgentToolCallEvent,
  summarizeReplayFile,
  type TickRollupSummary,
  type ToolCallEventSummary,
  type ReplaySummary,
  type ShardIncidentSummary
} from "./contracts";
import {
  describeTargetAffordances,
  formatTargetSummary
} from "./affordances";
import {
  PodWebDirectConnectClient,
  type DirectConnectActionState,
  runtimeConfigFromLocation,
  type DirectConnectStatus
} from "./direct-connect";
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

declare global {
  interface Window {
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
      getBackend: () => string;
      getStats: () => ReturnType<PodThreeRenderRuntime["getStats"]>;
      getTelemetryStats: () => ReturnType<typeof telemetryStats>;
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
const connectionNode = connectionLabel;
const worldNode = worldLabel;
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

const renderer = await createPodRenderRuntime(canvas);
backendLabel.textContent = renderer.backend;
qualityLabel.textContent = renderer.qualityPreset;
const runtimeStatsLabel = statsLabel;
const telemetryState = createTelemetryOverlayState(300);
const runtimeConfig = runtimeConfigFromLocation(window.location);

let liveFrameSource: "demo" | "legacy" | "threejs" = "demo";
let latestFrame: string | ReturnType<typeof buildAuthoritativeWorldFrame> | null = null;
let lastTelemetryRevision = -1;
let latestReplaySummary: ReplaySummary | null = null;
let latestIncidentSummary: ShardIncidentSummary | null = null;
let latestToolEventSummary: ToolCallEventSummary | null = null;
let latestRollupSummary: TickRollupSummary | null = null;
let liveReplayDocuments = 0;
let liveIncidentDocuments = 0;
let latestSnapshot: NetworkWorldSnapshot | null = null;
let latestActionStatus: DirectConnectActionState = {
  pendingCount: 0,
  lastSubmittedTick: null,
  lastAcknowledgedTick: null,
  lastRejectedTick: null,
  lastRejectedReason: null,
  lastActionSummary: null
};
let latestFeedback = "Awaiting authoritative outcomes";
let recentWorldEvents: NetworkGameEvent[] = [];
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
      viewportWidth: canvas.clientWidth || window.innerWidth,
      viewportHeight: canvas.clientHeight || window.innerHeight
        });
        liveFrameSource = "threejs";
        frameSourceLabel.textContent = "authoritative websocket";
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
let lastMovementSignature = "stop";
let lastMovementSubmitAtMs = 0;

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
  if (!liveClient) {
    latestFeedback = "Direct-connect is not active";
    return;
  }
  if (!liveClient.submitActions(actions)) {
    latestFeedback = "Action could not be submitted";
  }
}

function movementDirection(): [number, number] | null {
  const horizontal =
    (pressedKeys.has("KeyD") || pressedKeys.has("ArrowRight") ? 1 : 0) -
    (pressedKeys.has("KeyA") || pressedKeys.has("ArrowLeft") ? 1 : 0);
  const vertical =
    (pressedKeys.has("KeyS") || pressedKeys.has("ArrowDown") ? 1 : 0) -
    (pressedKeys.has("KeyW") || pressedKeys.has("ArrowUp") ? 1 : 0);

  if (horizontal === 0 && vertical === 0) {
    return null;
  }

  const length = Math.hypot(horizontal, vertical);
  return [horizontal / length, vertical / length];
}

function maybeSubmitMovement(timestamp: number): void {
  const direction = movementDirection();
  const signature = direction
    ? `${direction[0].toFixed(3)}:${direction[1].toFixed(3)}`
    : "stop";
  const resendDue = timestamp - lastMovementSubmitAtMs >= 90;

  if (direction) {
    if (signature !== lastMovementSignature || resendDue) {
      submitActions([{ kind: "move", direction }]);
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

window.addEventListener("keydown", (event) => {
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
});

window.addEventListener("keyup", (event) => {
  pressedKeys.delete(event.code);
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
    latestFrame = frame;
    liveFrameSource = "legacy";
    frameSourceLabel.textContent = "legacy pod-render frame";
  },
  renderThreeJsWebGpuFrame(frame: string) {
    latestFrame = frame;
    liveFrameSource = "threejs";
    frameSourceLabel.textContent = "Three.js WebGPU frame";
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
    latestFrame = null;
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
  },
  getReplaySummary() {
    return latestReplaySummary;
  },
  getIncidentSummary() {
    return latestIncidentSummary;
  }
};

function applyLiveDebugDocument(document: string): void {
  const parsed = parseLiveDebugDocument(document);

  switch (parsed.kind) {
    case "tickTelemetry":
      applyTickTelemetry(telemetryState, parsed.payload);
      break;
    case "toolCallEvent":
      latestToolEventSummary = summarizeAgentToolCallEvent(parsed.payload);
      break;
    case "tickRollup":
      latestRollupSummary = summarizeAgentTickRollup(parsed.payload);
      break;
    case "replay":
      latestReplaySummary = summarizeReplayFile(parsed.payload);
      liveReplayDocuments += 1;
      break;
    case "incident":
      latestIncidentSummary = parsed.payload;
      liveIncidentDocuments += 1;
      break;
  }
}

async function tick(timestamp: number): Promise<void> {
  maybeSubmitMovement(timestamp);

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
      await renderer.applyFrame(
        typeof latestFrame === "string" ? parseThreeJsWebGpuFrame(latestFrame) : latestFrame
      );
    } else if (typeof latestFrame === "string") {
      await renderer.applyLegacyFrame(parseRenderFrame(latestFrame));
    } else {
      await renderer.applyFrame(latestFrame);
    }
  } else {
    await renderer.applyFrame(createDemoFrame(timestamp / 1000));
  }

  const stats = renderer.getStats();
  runtimeStatsLabel.textContent = `${stats.drawCalls} calls · ${stats.triangles} tris · ${stats.pixelRatio.toFixed(
    2
  )}x DPR · ${stats.frameMs.toFixed(1)}ms · ${stats.renderThread} thread · assets ${
    stats.residentGeometryAssets + stats.residentSpriteAssets
  } resident / ${stats.pendingGeometryAssets + stats.pendingSpriteAssets} pending`;
  renderTelemetryHud();

  requestAnimationFrame((nextTimestamp) => {
    void tick(nextTimestamp);
  });
}

function renderTelemetryHud(): void {
  const stats = telemetryStats(telemetryState);
  const target = selectedTarget();
  const controlled = controlledEntity();
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
    ? `${latestIncidentSummary.severity} · ${latestIncidentSummary.summary} · stream ${liveIncidentDocuments}`
    : "No shard incident summary loaded";
  toolEventSummaryNode.textContent = latestToolEventSummary
    ? `E(${latestToolEventSummary.agentEntityId}) · ${latestToolEventSummary.toolName} · ${latestToolEventSummary.status} · ${latestToolEventSummary.latencyMs}ms`
    : "No tool-call event loaded";
  rollupSummaryNode.textContent = latestRollupSummary
    ? `E(${latestRollupSummary.agentEntityId}) · ticks ${latestRollupSummary.tickStart}-${latestRollupSummary.tickEnd} · ${latestRollupSummary.totalDistance.toFixed(
        2
      )}u · ${latestRollupSummary.toolErrorCount} tool errors`
    : "No telemetry rollup loaded";
  if (latestReplaySummary) {
    replaySummaryNode.textContent = `${latestReplaySummary.name} · ${latestReplaySummary.traceCount} traces · ${latestReplaySummary.trainingSampleCount} samples · ${latestReplaySummary.totalPathDistance.toFixed(
      2
    )}u · stream ${liveReplayDocuments}`;
  }
}

liveClient?.connect();
void tick(performance.now());
