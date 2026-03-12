export interface DirectConnectRuntimeConfig {
  url: string;
  playerName: string;
  debugTelemetry: boolean;
  reconnectDelayMs: number;
  pingIntervalMs: number;
  heartbeatTimeoutMs: number;
  maxPendingActionBatches: number;
}

export interface InitialHudState {
  feedback: string;
  connectionBadge: string;
  worldLabel: string;
  populationLabel: string;
  frameSourceLabel: string;
}

export type LocalWorldPresetId = "verdant-hollow" | "bootstrap-showcase";

export interface LocalWorldPresentation {
  presetId: LocalWorldPresetId;
  mode: "local-sandbox" | "bootstrap-showcase";
  url: string;
  worldName: string;
  bootFeedback: string;
  readyFeedback: string;
  statusDetail: string;
  connectionBadge: string;
  worldLabel: string;
  populationLabel: string;
  frameSourceLabel: string;
}

const DEFAULT_PLAYER_NAME = "WebPlayer";
const DEFAULT_RECONNECT_DELAY_MS = 1500;
const DEFAULT_PING_INTERVAL_MS = 2000;
const DEFAULT_HEARTBEAT_TIMEOUT_MS = 6500;
const DEFAULT_MAX_PENDING_ACTION_BATCHES = 6;
const DEFAULT_LOCAL_WORLD_PRESET: LocalWorldPresetId = "verdant-hollow";

const LOCAL_WORLD_PRESENTATIONS: Record<LocalWorldPresetId, LocalWorldPresentation> = {
  "verdant-hollow": {
    presetId: "verdant-hollow",
    mode: "local-sandbox",
    url: "local://verdant-hollow",
    worldName: "Verdant Hollow",
    bootFeedback: "Starting local browser shard",
    readyFeedback:
      "Local sandbox ready: click terrain to move, right-drag or use arrow keys for camera, wheel to zoom, WASD steer, Tab target, and double-click targets for default actions",
    statusDetail: "Local sandbox shard ready",
    connectionBadge: "local sandbox booting",
    worldLabel: "booting Verdant Hollow",
    populationLabel: "Seeding local region population",
    frameSourceLabel: "bootstrapping local sandbox"
  },
  "bootstrap-showcase": {
    presetId: "bootstrap-showcase",
    mode: "bootstrap-showcase",
    url: "local://resonant-shore",
    worldName: "Resonant Shore",
    bootFeedback: "Staging bootstrap showcase",
    readyFeedback:
      "Bootstrap showcase ready: step into the overlook, right-drag or use arrow keys for camera, wheel to zoom, WASD steer, Tab target, and double-click targets for default actions",
    statusDetail: "Bootstrap showcase shard ready",
    connectionBadge: "showcase route booting",
    worldLabel: "booting Resonant Shore",
    populationLabel: "Staging authored vista population",
    frameSourceLabel: "bootstrapping showcase shard"
  }
};

const LOCAL_WORLD_PRESET_ALIASES: Record<string, LocalWorldPresetId> = {
  "verdant-hollow": "verdant-hollow",
  sandbox: "verdant-hollow",
  "local-sandbox": "verdant-hollow",
  showcase: "bootstrap-showcase",
  "bootstrap-showcase": "bootstrap-showcase",
  bootstrap: "bootstrap-showcase"
};

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
    pingIntervalMs: parsePositiveInt(params.get("pingMs")) ?? DEFAULT_PING_INTERVAL_MS,
    heartbeatTimeoutMs:
      parsePositiveInt(params.get("heartbeatMs")) ?? DEFAULT_HEARTBEAT_TIMEOUT_MS,
    maxPendingActionBatches:
      parsePositiveInt(params.get("pendingBatches")) ?? DEFAULT_MAX_PENDING_ACTION_BATCHES
  };
}

export function resolveLocalWorldPresentation(
  presetId: LocalWorldPresetId
): LocalWorldPresentation {
  return { ...LOCAL_WORLD_PRESENTATIONS[presetId] };
}

export function localWorldPresetFromLocation(
  location: Pick<Location, "search">
): LocalWorldPresetId {
  const params = new URLSearchParams(location.search);
  const requested = params.get("world")?.trim().toLowerCase();
  if (!requested) {
    return DEFAULT_LOCAL_WORLD_PRESET;
  }
  return LOCAL_WORLD_PRESET_ALIASES[requested] ?? DEFAULT_LOCAL_WORLD_PRESET;
}

export function localWorldPresentationFromLocation(
  location: Pick<Location, "search">
): LocalWorldPresentation {
  return resolveLocalWorldPresentation(localWorldPresetFromLocation(location));
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

  const presentation = localWorldPresentationFromLocation(location);
  return {
    feedback: presentation.bootFeedback,
    connectionBadge: presentation.connectionBadge,
    worldLabel: presentation.worldLabel,
    populationLabel: presentation.populationLabel,
    frameSourceLabel: presentation.frameSourceLabel
  };
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
