import {
  applyNetworkStateDelta,
  buildAuthoritativeWorldFrame,
  encodeDirectConnectActionBatch,
  encodeDirectConnectConnectMessage,
  encodeDirectConnectDebugFocusMessage,
  encodeDirectConnectDebugTelemetryMessage,
  encodeDirectConnectFullSnapshotRequest,
  encodeDirectConnectPingMessage,
  parseDirectConnectServerMessage,
  type AuthoritativeWorldFrameOptions,
  type BrowserAction,
  type DirectConnectServerMessage,
  type NetworkEventBatch,
  type NetworkWorldSnapshot
} from "./contracts";

export interface DirectConnectRuntimeConfig {
  url: string;
  playerName: string;
  debugTelemetry: boolean;
  reconnectDelayMs: number;
  pingIntervalMs: number;
}

export interface DirectConnectStatus {
  phase:
    | "idle"
    | "connecting"
    | "connected"
    | "reconnecting"
    | "rejected"
    | "disconnected"
    | "error";
  detail: string;
  url: string;
  tick: number | null;
  entityCount: number;
  controlledEntity: number | null;
  authoritativeDigest: number | null;
  roundTripMs: number | null;
  jitterMs: number | null;
  lastPongServerTick: number | null;
}

export interface DirectConnectActionState {
  pendingCount: number;
  lastSubmittedTick: number | null;
  lastAcknowledgedTick: number | null;
  lastRejectedTick: number | null;
  lastRejectedReason: string | null;
  lastActionSummary: string | null;
}

export interface InitialHudState {
  feedback: string;
  connectionBadge: string;
  worldLabel: string;
  populationLabel: string;
  frameSourceLabel: string;
}

interface DirectConnectHandlers {
  onFrame: (
    snapshot: NetworkWorldSnapshot,
    frameOptions: AuthoritativeWorldFrameOptions,
    status: DirectConnectStatus
  ) => void;
  onEventBatch: (batch: NetworkEventBatch) => void;
  onDebugDocument: (document: string) => void;
  onActionState: (state: DirectConnectActionState) => void;
  onStatus: (status: DirectConnectStatus) => void;
}

const DEFAULT_PLAYER_NAME = "WebPlayer";
const DEFAULT_RECONNECT_DELAY_MS = 1500;
const DEFAULT_PING_INTERVAL_MS = 2000;

interface PendingActionBatch {
  tick: number;
  summary: string;
}

export function runtimeConfigFromLocation(
  location: Pick<Location, "search">
): DirectConnectRuntimeConfig | null {
  const params = new URLSearchParams(location.search);
  const explicitUrl = params.get("ws");
  const server = params.get("server");

  if (!explicitUrl && !server) {
    return null;
  }

  const url = explicitUrl ?? normalizeServerToWebSocket(server ?? "");
  return {
    url,
    playerName: params.get("player")?.trim() || DEFAULT_PLAYER_NAME,
    debugTelemetry: parseBooleanParam(params.get("debug")),
    reconnectDelayMs: parsePositiveInt(params.get("reconnectMs")) ?? DEFAULT_RECONNECT_DELAY_MS,
    pingIntervalMs: parsePositiveInt(params.get("pingMs")) ?? DEFAULT_PING_INTERVAL_MS
  };
}

export function initialHudStateFromLocation(
  location: Pick<Location, "search">
): InitialHudState {
  const runtimeConfig = runtimeConfigFromLocation(location);
  if (runtimeConfig) {
    return {
      feedback: `Connecting to ${runtimeConfig.url}`,
      connectionBadge: "connecting to shard",
      worldLabel: "waiting for shard snapshot",
      populationLabel: "Awaiting authoritative population state",
      frameSourceLabel: "bootstrapping direct-connect shard"
    };
  }

  return {
    feedback: "Starting local browser shard",
    connectionBadge: "local sandbox booting",
    worldLabel: "booting Verdant Hollow",
    populationLabel: "Seeding local region population",
    frameSourceLabel: "bootstrapping local sandbox"
  };
}

export class PodWebDirectConnectClient {
  private socket: WebSocket | null = null;
  private reconnectTimer: ReturnType<typeof globalThis.setTimeout> | null = null;
  private reconnectToken: string | null = null;
  private snapshot: NetworkWorldSnapshot | null = null;
  private controlledEntity: number | null = null;
  private authoritativeDigest: number | null = null;
  private lastEventTick = 0;
  private pendingActionBatches: PendingActionBatch[] = [];
  private closedExplicitly = false;
  private debugTelemetryEnabled: boolean;
  private debugFocusEntity: number | null = null;
  private pingTimer: ReturnType<typeof globalThis.setInterval> | null = null;
  private lastRoundTripSampleMs: number | null = null;
  private status: DirectConnectStatus;
  private actionState: DirectConnectActionState;

  constructor(
    private readonly config: DirectConnectRuntimeConfig,
    private readonly handlers: DirectConnectHandlers
  ) {
    this.debugTelemetryEnabled = config.debugTelemetry;
    this.status = {
      phase: "idle",
      detail: "Idle",
      url: config.url,
      tick: null,
      entityCount: 0,
      controlledEntity: null,
      authoritativeDigest: null,
      roundTripMs: null,
      jitterMs: null,
      lastPongServerTick: null
    };
    this.actionState = {
      pendingCount: 0,
      lastSubmittedTick: null,
      lastAcknowledgedTick: null,
      lastRejectedTick: null,
      lastRejectedReason: null,
      lastActionSummary: null
    };
  }

  connect(): void {
    this.closedExplicitly = false;
    this.clearReconnectTimer();
    this.updateStatus("connecting", `Connecting to ${this.config.url}`);

    const socket = new WebSocket(this.config.url);
    this.socket = socket;

    socket.addEventListener("open", () => {
      socket.send(
        encodeDirectConnectConnectMessage(this.config.playerName, this.reconnectToken)
      );
      if (this.debugTelemetryEnabled) {
        socket.send(encodeDirectConnectDebugTelemetryMessage(true));
      }
      socket.send(encodeDirectConnectDebugFocusMessage(this.debugFocusEntity));
      this.startPingLoop();
    });

    socket.addEventListener("message", (event) => {
      if (typeof event.data !== "string") {
        return;
      }

      try {
        this.handleServerMessage(parseDirectConnectServerMessage(event.data));
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        this.updateStatus("error", `Invalid server payload: ${message}`);
      }
    });

    socket.addEventListener("close", () => {
      this.socket = null;
      this.stopPingLoop();
      if (this.closedExplicitly) {
        this.updateStatus("disconnected", "Disconnected");
        return;
      }
      this.updateStatus("reconnecting", `Connection lost, retrying in ${this.config.reconnectDelayMs}ms`);
      this.scheduleReconnect();
    });

    socket.addEventListener("error", () => {
      this.updateStatus("error", "WebSocket transport error");
    });
  }

  disconnect(): void {
    this.closedExplicitly = true;
    this.clearReconnectTimer();
    this.stopPingLoop();
    this.socket?.close();
    this.socket = null;
    this.updateStatus("disconnected", "Disconnected");
  }

  setDebugTelemetry(enabled: boolean): void {
    this.debugTelemetryEnabled = enabled;
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(encodeDirectConnectDebugTelemetryMessage(enabled));
    }
  }

  setDebugFocusEntity(entityId: number | null): void {
    this.debugFocusEntity = entityId;
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(encodeDirectConnectDebugFocusMessage(entityId));
    }
  }

  currentStatus(): DirectConnectStatus {
    return { ...this.status };
  }

  currentActionState(): DirectConnectActionState {
    return { ...this.actionState };
  }

  snapshotState(): NetworkWorldSnapshot | null {
    return this.snapshot;
  }

  controlledEntityId(): number | null {
    return this.controlledEntity;
  }

  submitActions(actions: BrowserAction[]): boolean {
    if (actions.length === 0 || this.socket?.readyState !== WebSocket.OPEN) {
      return false;
    }

    const nextTick = (this.snapshot?.tick ?? this.status.tick ?? 0) + 1;
    const summary = summarizeActions(actions);
    this.pendingActionBatches.push({
      tick: nextTick,
      summary
    });
    this.actionState = {
      ...this.actionState,
      pendingCount: this.pendingActionBatches.length,
      lastSubmittedTick: nextTick,
      lastActionSummary: summary
    };
    this.handlers.onActionState(this.currentActionState());
    this.socket.send(encodeDirectConnectActionBatch(nextTick, actions));
    return true;
  }

  private handleServerMessage(message: DirectConnectServerMessage): void {
    switch (message.kind) {
      case "welcome":
        this.reconnectToken = message.reconnectToken;
        this.snapshot = message.snapshot;
        this.controlledEntity = message.controlledEntity;
        this.authoritativeDigest = message.authoritativeDigest;
        this.updateWorldStatus("connected", `Connected as ${this.config.playerName}`);
        this.emitFrame();
        break;
      case "stateDelta":
        try {
          this.snapshot = applyNetworkStateDelta(
            this.snapshot,
            message.delta,
            message.isFullSnapshot
          );
          this.authoritativeDigest = message.authoritativeDigest;
          this.reconcileAcknowledgedActions(message.acknowledgedActionTick);
          this.updateWorldStatus("connected", `Authoritative tick ${message.tick}`);
          this.emitFrame();
        } catch {
          this.socket?.send(
            encodeDirectConnectFullSnapshotRequest(
              this.snapshot?.tick ?? null,
              this.authoritativeDigest
            )
          );
          this.updateStatus("reconnecting", "Requested full snapshot recovery");
        }
        break;
      case "tickTelemetry":
        this.handlers.onDebugDocument(message.frameJson);
        break;
      case "debugDocument":
        this.handlers.onDebugDocument(message.document);
        break;
      case "eventBatch":
        if (message.tick < this.lastEventTick) {
          break;
        }
        this.lastEventTick = message.tick;
        this.handlers.onEventBatch({
          tick: message.tick,
          events: message.events
        });
        break;
      case "rejected":
        this.rejectLatestPendingAction(message.reason);
        if (this.status.phase === "connected") {
          this.updateWorldStatus("connected", `Rejected action: ${message.reason}`);
        } else {
          this.updateStatus("rejected", message.reason);
        }
        break;
      case "pong":
        this.recordPong(message.clientTimestamp, message.serverTimestamp);
        break;
    }
  }

  private emitFrame(): void {
    if (!this.snapshot) {
      return;
    }

    this.handlers.onFrame(
      this.snapshot,
      {
        controlledEntity: this.controlledEntity
      },
      this.currentStatus()
    );
  }

  private updateWorldStatus(
    phase: DirectConnectStatus["phase"],
    detail: string
  ): void {
    this.updateStatus(phase, detail, {
      tick: this.snapshot?.tick ?? null,
      entityCount: this.snapshot?.entities.length ?? 0,
      controlledEntity: this.controlledEntity,
      authoritativeDigest: this.authoritativeDigest
    });
  }

  private updateStatus(
    phase: DirectConnectStatus["phase"],
    detail: string,
    overrides: Partial<DirectConnectStatus> = {}
  ): void {
    this.status = {
      ...this.status,
      phase,
      detail,
      ...overrides
    };
    this.handlers.onStatus(this.currentStatus());
  }

  private scheduleReconnect(): void {
    this.clearReconnectTimer();
    this.reconnectTimer = globalThis.setTimeout(() => {
      this.connect();
    }, this.config.reconnectDelayMs);
  }

  private clearReconnectTimer(): void {
    if (this.reconnectTimer != null) {
      globalThis.clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
  }

  private startPingLoop(): void {
    this.stopPingLoop();
    this.sendPing();
    this.pingTimer = globalThis.setInterval(() => {
      this.sendPing();
    }, this.config.pingIntervalMs);
  }

  private stopPingLoop(): void {
    if (this.pingTimer != null) {
      globalThis.clearInterval(this.pingTimer);
      this.pingTimer = null;
    }
  }

  private sendPing(): void {
    if (this.socket?.readyState !== WebSocket.OPEN) {
      return;
    }

    this.socket.send(encodeDirectConnectPingMessage(Date.now()));
  }

  private recordPong(clientTimestamp: number, serverTick: number): void {
    const roundTripMs = Math.max(0, Date.now() - clientTimestamp);
    const jitterMs =
      this.lastRoundTripSampleMs == null
        ? 0
        : Math.abs(roundTripMs - this.lastRoundTripSampleMs);
    this.lastRoundTripSampleMs = roundTripMs;
    this.updateStatus(this.status.phase, this.status.detail, {
      roundTripMs,
      jitterMs,
      lastPongServerTick: serverTick
    });
  }

  private reconcileAcknowledgedActions(acknowledgedTick: number | null): void {
    if (acknowledgedTick == null) {
      return;
    }

    const acknowledged = this.pendingActionBatches
      .filter((batch) => batch.tick <= acknowledgedTick)
      .at(-1);
    this.pendingActionBatches = this.pendingActionBatches.filter(
      (batch) => batch.tick > acknowledgedTick
    );
    this.actionState = {
      ...this.actionState,
      pendingCount: this.pendingActionBatches.length,
      lastAcknowledgedTick: acknowledgedTick,
      lastActionSummary: acknowledged?.summary ?? this.actionState.lastActionSummary
    };
    this.handlers.onActionState(this.currentActionState());
  }

  private rejectLatestPendingAction(reason: string): void {
    const rejected = this.pendingActionBatches.pop() ?? null;
    this.actionState = {
      ...this.actionState,
      pendingCount: this.pendingActionBatches.length,
      lastRejectedTick: rejected?.tick ?? this.actionState.lastRejectedTick,
      lastRejectedReason: reason,
      lastActionSummary: rejected?.summary ?? this.actionState.lastActionSummary
    };
    this.handlers.onActionState(this.currentActionState());
  }
}

export function frameFromAuthoritativeSnapshot(
  snapshot: NetworkWorldSnapshot,
  options: AuthoritativeWorldFrameOptions = {}
) {
  return buildAuthoritativeWorldFrame(snapshot, options);
}

function normalizeServerToWebSocket(server: string): string {
  if (server.startsWith("ws://") || server.startsWith("wss://")) {
    return server;
  }

  if (server.startsWith("http://")) {
    return `ws://${server.slice("http://".length)}`;
  }

  if (server.startsWith("https://")) {
    return `wss://${server.slice("https://".length)}`;
  }

  return `ws://${server}`;
}

function parseBooleanParam(value: string | null): boolean {
  if (value == null) {
    return false;
  }

  switch (value.toLowerCase()) {
    case "1":
    case "true":
    case "yes":
    case "on":
      return true;
    default:
      return false;
  }
}

function parsePositiveInt(value: string | null): number | null {
  if (value == null) {
    return null;
  }

  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

function summarizeActions(actions: BrowserAction[]): string {
  return actions
    .map((action) => action.kind.replace(/([A-Z])/g, " $1").toLowerCase())
    .join(" + ");
}
