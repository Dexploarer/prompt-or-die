import { decode as decodeToon } from "@toon-format/toon";

export type Vec3Tuple = [number, number, number];
export type Vec4Tuple = [number, number, number, number];
export type Vec2Tuple = [number, number];
export type RgbaTuple = [number, number, number, number];

export interface CameraState {
  x: number;
  y: number;
  zoom: number;
  rotation: number;
  viewportWidth: number;
  viewportHeight: number;
}

export interface RenderCommand {
  type: string;
  x: number;
  y: number;
  width: number;
  height: number;
  rotation: number;
  scaleX: number;
  scaleY: number;
  color: RgbaTuple;
  alpha: number;
  texture?: string;
  frame?: number;
  mesh?: string;
  material?: string;
  z?: number;
  transform3d?: {
    position: Vec3Tuple;
    rotation: Vec4Tuple;
    scale: Vec3Tuple;
  };
  billboard?: boolean;
  castShadows?: boolean;
  receiveShadows?: boolean;
  transparent?: boolean;
  doubleSided?: boolean;
  roughness?: number;
  metallic?: number;
  emissive?: Vec3Tuple;
  layer: number;
  visible: boolean;
  sourceEntity?: number;
}

export interface RenderFrame {
  camera: CameraState;
  commands: RenderCommand[];
  backgroundColor: RgbaTuple;
}

export interface ThreeJsInstance {
  position: Vec3Tuple;
  rotation: Vec4Tuple;
  scale: Vec3Tuple;
  color?: RgbaTuple;
  sourceEntity?: number;
}

export type ThreeJsRenderPhase = "opaque" | "transparent";

export interface ThreeJsMeshBatch {
  mesh: string;
  material: string;
  layer: number;
  phase: ThreeJsRenderPhase;
  sortDepth: number;
  renderOrder: number;
  transparent: boolean;
  doubleSided: boolean;
  castShadows: boolean;
  receiveShadows: boolean;
  tint: RgbaTuple;
  roughness: number;
  metallic: number;
  emissive: Vec3Tuple;
  depthWrite: boolean;
  depthTest: boolean;
  instances: ThreeJsInstance[];
}

export interface ThreeJsSpriteBatch {
  texture: string;
  frame: number;
  layer: number;
  billboard: boolean;
  phase: ThreeJsRenderPhase;
  sortDepth: number;
  renderOrder: number;
  transparent: boolean;
  depthWrite: boolean;
  depthTest: boolean;
  instances: ThreeJsInstance[];
}

export interface ThreeJsWebGpuHints {
  renderer: string;
  preferredBackend: string;
  fallbackBackend: string;
  useInstancing: boolean;
  sortMetric: string;
  sortOpaqueFrontToBack: boolean;
  preserveInstanceOrder: boolean;
  sortTransparentBackToFront: boolean;
  transparentInstancingStrategy: string;
  opaqueDepthWrite: boolean;
  transparentDepthWrite: boolean;
  maxPixelRatio: number;
}

export interface ThreeJsWebGpuFrame {
  camera: CameraState;
  backgroundColor: RgbaTuple;
  overlayCommands: RenderCommand[];
  meshBatches: ThreeJsMeshBatch[];
  spriteBatches: ThreeJsSpriteBatch[];
  hints: ThreeJsWebGpuHints;
}

export function parseThreeJsWebGpuFrame(
  frame: string | ThreeJsWebGpuFrame
): ThreeJsWebGpuFrame {
  return typeof frame === "string"
    ? (JSON.parse(frame) as ThreeJsWebGpuFrame)
    : frame;
}

export function parseRenderFrame(frame: string | RenderFrame): RenderFrame {
  return typeof frame === "string" ? (JSON.parse(frame) as RenderFrame) : frame;
}

export interface TelemetryRuntimeProfile {
  role: string;
  agent_type: string;
  capabilities: Record<string, boolean>;
}

export interface TelemetryTrajectorySample {
  tick: number;
  elapsed_secs: number;
  position: Vec2Tuple;
  velocity: Vec2Tuple;
  rotation: number;
}

export interface TelemetryTrajectoryFrame {
  start: TelemetryTrajectorySample;
  end: TelemetryTrajectorySample;
  displacement: Vec2Tuple;
  distance_travelled: number;
}

export interface TelemetryActionTrace {
  source: string;
  stage: string;
  action: Record<string, unknown>;
  rejection_reason?: string | null;
}

export interface TelemetryToolCallTrace {
  tick: number;
  tool_name: string;
  provider: string;
  status: string;
  latency_ms: number;
  request_units: number;
  response_units: number;
  error_message?: string | null;
}

export interface TelemetryAgentFrame {
  tick: number;
  agent_id: string;
  entity_id?: number | null;
  runtime_profile: TelemetryRuntimeProfile;
  visible_entity_count: number;
  audible_event_count: number;
  message_count: number;
  available_action_count: number;
  objective_count: number;
  trajectory?: TelemetryTrajectoryFrame | null;
  action_trace: TelemetryActionTrace[];
  tool_calls: TelemetryToolCallTrace[];
}

export interface TickTelemetryFrame {
  tick: number;
  agents: TelemetryAgentFrame[];
}

export interface EntityDrift {
  entity_id: number;
  position_error: number;
  velocity_error: number;
  rotation_error: number;
  health_error?: number | null;
  max_health_error?: number | null;
  movement_speed_error?: number | null;
}

export interface RecoveryRequestState {
  awaiting_full_snapshot: boolean;
  request_attempts: number;
  last_request_server_tick?: number | null;
  last_request_digest?: number | null;
  next_retry_tick?: number | null;
}

export interface CatchUpDiagnostics {
  authoritative_tick?: number | null;
  authoritative_digest?: number | null;
  predicted_tick?: number | null;
  predicted_digest?: number | null;
  presentation_tick?: number | null;
  desired_presentation_tick?: number | null;
  presentation_drift_ticks?: number | null;
  history_snapshots: number;
  oldest_authoritative_tick?: number | null;
  latest_authoritative_tick?: number | null;
  pending_action_batches: number;
  replayed_action_count: number;
  controlled_entity_drift?: EntityDrift | null;
  recovery: RecoveryRequestState;
}

export interface TickTelemetryEnvelope {
  tickTelemetry: TickTelemetryFrame;
  recovery?: CatchUpDiagnostics | null;
}

export interface ReplayHeader {
  name: string;
  timestamp: number;
  world_seed: number;
  tick_count: number;
  agent_count: number;
  notes: string;
}

export interface ReplayActionOutcomeSummary {
  submitted: number;
  executed: number;
  rejected: number;
  queued: number;
}

export interface ReplayTrainingSample {
  tick: number;
  agent_id: string;
  path_distance: number;
  action_outcomes: ReplayActionOutcomeSummary;
  encounter_transition?: Record<string, unknown> | null;
  tool_call_latency_ms: number;
  tool_call_error_count: number;
}

export interface ReplayDecisionTrace {
  tick: number;
  agent_id: string;
  observation_hash: number;
  prompt_sent: string;
  raw_response: string;
  actions_taken: Record<string, unknown>[];
  tool_calls: TelemetryToolCallTrace[];
  latency_ms: number;
}

export interface ReplayFileDocument {
  header: ReplayHeader;
  traces: ReplayDecisionTrace[][];
  telemetry_windows: TickTelemetryFrame[];
  training_samples: ReplayTrainingSample[];
}

export interface ReplaySummary {
  name: string;
  tickCount: number;
  agentCount: number;
  traceCount: number;
  trainingSampleCount: number;
  telemetryWindowCount: number;
  toolCallCount: number;
  toolCallErrors: number;
  totalPathDistance: number;
  notes: string;
  latestTelemetryTick: number | null;
}

export interface ShardIncidentSummary {
  shard_id: string;
  latest_tick: number;
  severity: string;
  summary: string;
  tick_budget_overrun_rate: number;
  action_rejection_rate: number;
  tool_call_error_rate: number;
  average_tool_latency_ms: number;
  average_trajectory_distance: number;
  peak_entity_count: number;
  peak_agent_count: number;
  capture_actions: number;
  summon_actions: number;
  gather_actions: number;
  loot_actions: number;
  notes: string[];
}

export interface AgentToolCallEventDocument {
  tick: number;
  agent_entity_id: number;
  trace: TelemetryToolCallTrace;
}

export interface AgentTickRollupDocument {
  tick_start: number;
  tick_end: number;
  agent_entity_id: number;
  total_distance: number;
  submitted_action_count: number;
  executed_action_count: number;
  rejected_action_count: number;
  tool_call_count: number;
  tool_error_count: number;
  visible_entity_count: number;
  audible_event_count: number;
  message_count: number;
  average_tool_latency_ms: number;
}

export interface ToolCallEventSummary {
  agentEntityId: number;
  toolName: string;
  provider: string;
  status: string;
  latencyMs: number;
  requestUnits: number;
  responseUnits: number;
  errorMessage: string | null;
}

export interface TickRollupSummary {
  agentEntityId: number;
  tickStart: number;
  tickEnd: number;
  totalDistance: number;
  rejectedActionCount: number;
  toolErrorCount: number;
  averageToolLatencyMs: number;
  visibleEntityCount: number;
  audibleEventCount: number;
  messageCount: number;
}

export type LiveDebugDocument =
  | { kind: "tickTelemetry"; documentType: string; payload: TickTelemetryEnvelope }
  | { kind: "toolCallEvent"; documentType: string; payload: AgentToolCallEventDocument }
  | { kind: "tickRollup"; documentType: string; payload: AgentTickRollupDocument }
  | { kind: "replay"; documentType: string; payload: ReplayFileDocument }
  | { kind: "incident"; documentType: string; payload: ShardIncidentSummary };

export interface NetworkEntitySnapshot {
  id: number;
  position: Vec2Tuple;
  velocity: Vec2Tuple;
  rotation: number;
  health?: number | null;
  maxHealth?: number | null;
  movementSpeed?: number | null;
  label?: string | null;
}

export interface NetworkWorldSnapshot {
  tick: number;
  entities: NetworkEntitySnapshot[];
}

export interface NetworkStateDelta {
  tick: number;
  updated: NetworkEntitySnapshot[];
  destroyed: number[];
}

export type DirectConnectServerMessage =
  | {
      kind: "welcome";
      clientId: string;
      reconnectToken: string;
      tick: number;
      controlledEntity: number | null;
      authoritativeDigest: number;
      snapshot: NetworkWorldSnapshot;
    }
  | {
      kind: "stateDelta";
      tick: number;
      acknowledgedActionTick: number | null;
      authoritativeDigest: number;
      isFullSnapshot: boolean;
      delta: NetworkStateDelta;
    }
  | { kind: "eventBatch"; tick: number; events: unknown[] }
  | { kind: "tickTelemetry"; frameJson: string }
  | { kind: "debugDocument"; document: string }
  | { kind: "pong"; clientTimestamp: number; serverTimestamp: number }
  | { kind: "rejected"; reason: string };

export interface AuthoritativeWorldFrameOptions {
  controlledEntity?: number | null;
  viewportWidth?: number;
  viewportHeight?: number;
  backgroundColor?: RgbaTuple;
}

export type BrowserSpeakVolume = "Whisper" | "Normal" | "Shout";
export type BrowserCompanionCommand = "Attack" | "Follow" | "Guard" | "Recall";

export type BrowserAction =
  | { kind: "move"; direction: Vec2Tuple }
  | { kind: "stop" }
  | { kind: "rotate"; angle: number }
  | { kind: "lookAt"; target: Vec2Tuple }
  | { kind: "attack" }
  | { kind: "attackTarget"; target: number }
  | { kind: "interact" }
  | { kind: "interactWith"; target: number }
  | { kind: "gatherResource"; target: number; skill: string }
  | { kind: "loot"; target: number }
  | { kind: "captureCreature"; target: number; toolSlot?: number | null }
  | { kind: "summonCompanion"; slot: number }
  | {
      kind: "commandCompanion";
      slot: number;
      command: BrowserCompanionCommand;
      target?: number | null;
    }
  | {
      kind: "speak";
      message: string;
      volume: BrowserSpeakVolume;
    }
  | { kind: "setAutoRetaliate"; enabled: boolean }
  | { kind: "idle" };

interface ToonDocumentEnvelope<T = unknown> {
  document_type: string;
  payload: T;
}

type JsonRecord = Record<string, unknown>;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function asNumber(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function asString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function optionalNumber(value: unknown): number | null | undefined {
  if (value === undefined) {
    return undefined;
  }
  if (value === null) {
    return null;
  }
  return asNumber(value);
}

function optionalString(value: unknown): string | null | undefined {
  if (value === undefined) {
    return undefined;
  }
  if (value === null) {
    return null;
  }
  return asString(value);
}

function vec2Tuple(value: unknown): Vec2Tuple | null {
  if (
    Array.isArray(value) &&
    value.length >= 2 &&
    typeof value[0] === "number" &&
    typeof value[1] === "number"
  ) {
    return [value[0], value[1]];
  }

  if (isRecord(value)) {
    const x = asNumber(value.x);
    const y = asNumber(value.y);
    if (x != null && y != null) {
      return [x, y];
    }
  }

  return null;
}

function decodeStructuredString(value: string): unknown {
  try {
    return JSON.parse(value) as unknown;
  } catch {
    return decodeToon(value);
  }
}

function documentEnvelope(value: unknown): ToonDocumentEnvelope | null {
  if (!isRecord(value)) {
    return null;
  }

  if (typeof value.document_type !== "string" || !("payload" in value)) {
    return null;
  }

  return value as unknown as ToonDocumentEnvelope;
}

function variantPayload(value: unknown, variant: string): unknown | null {
  if (!isRecord(value) || !(variant in value)) {
    return null;
  }

  return value[variant];
}

function parseNetworkEntitySnapshot(value: unknown): NetworkEntitySnapshot | null {
  if (!isRecord(value)) {
    return null;
  }

  const id = asNumber(value.id);
  const position = vec2Tuple(value.position);
  const velocity = vec2Tuple(value.velocity);
  const rotation = asNumber(value.rotation);

  if (id == null || position == null || velocity == null || rotation == null) {
    return null;
  }

  return {
    id,
    position,
    velocity,
    rotation,
    health: optionalNumber(value.health),
    maxHealth: optionalNumber(value.max_health ?? value.maxHealth),
    movementSpeed: optionalNumber(value.movement_speed ?? value.movementSpeed),
    label: optionalString(value.label) ?? null
  };
}

function parseNetworkWorldSnapshot(value: unknown): NetworkWorldSnapshot | null {
  if (!isRecord(value) || typeof value.tick !== "number" || !Array.isArray(value.entities)) {
    return null;
  }

  const entities = value.entities
    .map((entity) => parseNetworkEntitySnapshot(entity))
    .filter((entity): entity is NetworkEntitySnapshot => entity != null)
    .sort((left, right) => left.id - right.id);

  return {
    tick: value.tick,
    entities
  };
}

function parseNetworkStateDelta(value: unknown): NetworkStateDelta | null {
  if (!isRecord(value) || typeof value.tick !== "number") {
    return null;
  }

  const updated = Array.isArray(value.updated)
    ? value.updated
        .map((entity) => parseNetworkEntitySnapshot(entity))
        .filter((entity): entity is NetworkEntitySnapshot => entity != null)
    : null;
  const destroyed = Array.isArray(value.destroyed)
    ? value.destroyed.filter((entityId): entityId is number => typeof entityId === "number")
    : null;

  if (updated == null || destroyed == null) {
    return null;
  }

  return {
    tick: value.tick,
    updated,
    destroyed
  };
}

function directTickTelemetryFrame(value: unknown): TickTelemetryFrame | null {
  if (!isRecord(value)) {
    return null;
  }
  if (typeof value.tick !== "number" || !Array.isArray(value.agents)) {
    return null;
  }
  return value as unknown as TickTelemetryFrame;
}

function directTickTelemetryEnvelope(value: unknown): TickTelemetryEnvelope | null {
  if (!isRecord(value)) {
    return null;
  }

  const tickTelemetry = directTickTelemetryFrame(value.tickTelemetry);
  if (tickTelemetry) {
    return {
      tickTelemetry,
      recovery: (value.recovery as CatchUpDiagnostics | null | undefined) ?? null
    };
  }

  const snakeCaseTickTelemetry = directTickTelemetryFrame(value.tick_telemetry);
  if (snakeCaseTickTelemetry) {
    return {
      tickTelemetry: snakeCaseTickTelemetry,
      recovery: (value.recovery as CatchUpDiagnostics | null | undefined) ?? null
    };
  }

  return null;
}

export function parseTickTelemetryEnvelope(
  frame: string | TickTelemetryFrame | TickTelemetryEnvelope
): TickTelemetryEnvelope {
  const parsed =
    typeof frame === "string"
      ? decodeStructuredString(frame)
      : frame;

  const envelopeDocument = documentEnvelope(parsed);
  if (envelopeDocument) {
    switch (envelopeDocument.document_type) {
      case "tick_telemetry_envelope":
        return parseTickTelemetryEnvelope(envelopeDocument.payload as TickTelemetryEnvelope);
      case "tick_telemetry_frame":
        return {
          tickTelemetry: parseTickTelemetryEnvelope(
            envelopeDocument.payload as TickTelemetryFrame
          ).tickTelemetry,
          recovery: null
        };
      case "versioned_tick_telemetry": {
        const versioned = envelopeDocument.payload;
        if (isRecord(versioned) && "payload" in versioned) {
          return {
            tickTelemetry: parseTickTelemetryEnvelope(
              versioned.payload as TickTelemetryFrame
            ).tickTelemetry,
            recovery: null
          };
        }
        break;
      }
    }
  }

  const directEnvelope = directTickTelemetryEnvelope(parsed);
  if (directEnvelope) {
    return directEnvelope;
  }

  const directFrame = directTickTelemetryFrame(parsed);
  if (directFrame) {
    return { tickTelemetry: directFrame };
  }

  throw new Error("Invalid tick telemetry payload");
}

export function parseReplayFile(
  replay: string | ReplayFileDocument
): ReplayFileDocument {
  const parsed =
    typeof replay === "string" ? decodeStructuredString(replay) : replay;
  const replayDocument = documentEnvelope(parsed);

  if (replayDocument?.document_type === "replay_file") {
    return replayDocument.payload as ReplayFileDocument;
  }

  if (
    isRecord(parsed) &&
    isRecord(parsed.header) &&
    Array.isArray(parsed.traces) &&
    Array.isArray(parsed.telemetry_windows) &&
    Array.isArray(parsed.training_samples)
  ) {
    return parsed as unknown as ReplayFileDocument;
  }

  throw new Error("Invalid replay payload");
}

export function summarizeReplayFile(replay: ReplayFileDocument): ReplaySummary {
  const traces = replay.traces.flat();
  const toolCallCount = traces.reduce(
    (count, trace) => count + trace.tool_calls.length,
    0
  );
  const toolCallErrors = traces.reduce(
    (count, trace) =>
      count +
      trace.tool_calls.filter(
        (toolCall) =>
          toolCall.status !== "Succeeded" && toolCall.status !== "Requested"
      ).length,
    0
  );
  const totalPathDistance = replay.training_samples.reduce(
    (distance, sample) => distance + sample.path_distance,
    0
  );

  return {
    name: replay.header.name,
    tickCount: replay.header.tick_count,
    agentCount: replay.header.agent_count,
    traceCount: traces.length,
    trainingSampleCount: replay.training_samples.length,
    telemetryWindowCount: replay.telemetry_windows.length,
    toolCallCount,
    toolCallErrors,
    totalPathDistance: Number(totalPathDistance.toFixed(2)),
    notes: replay.header.notes,
    latestTelemetryTick: replay.telemetry_windows.at(-1)?.tick ?? null
  };
}

export function parseShardIncidentSummary(
  summary: string | ShardIncidentSummary
): ShardIncidentSummary {
  const parsed =
    typeof summary === "string" ? decodeStructuredString(summary) : summary;
  const summaryDocument = documentEnvelope(parsed);

  if (summaryDocument?.document_type === "shard_incident_summary") {
    return summaryDocument.payload as ShardIncidentSummary;
  }

  if (
    isRecord(parsed) &&
    typeof parsed.shard_id === "string" &&
    typeof parsed.latest_tick === "number" &&
    typeof parsed.summary === "string"
  ) {
    return parsed as unknown as ShardIncidentSummary;
  }

  throw new Error("Invalid shard incident summary payload");
}

export function parseAgentToolCallEvent(
  event: string | AgentToolCallEventDocument
): AgentToolCallEventDocument {
  const parsed =
    typeof event === "string" ? decodeStructuredString(event) : event;
  const eventDocument = documentEnvelope(parsed);

  if (eventDocument?.document_type === "agent_tool_call_event") {
    return eventDocument.payload as AgentToolCallEventDocument;
  }

  if (
    isRecord(parsed) &&
    typeof parsed.tick === "number" &&
    typeof parsed.agent_entity_id === "number" &&
    isRecord(parsed.trace)
  ) {
    return parsed as unknown as AgentToolCallEventDocument;
  }

  throw new Error("Invalid agent tool-call event payload");
}

export function summarizeAgentToolCallEvent(
  event: AgentToolCallEventDocument
): ToolCallEventSummary {
  return {
    agentEntityId: event.agent_entity_id,
    toolName: event.trace.tool_name,
    provider: event.trace.provider,
    status: event.trace.status,
    latencyMs: event.trace.latency_ms,
    requestUnits: event.trace.request_units,
    responseUnits: event.trace.response_units,
    errorMessage: event.trace.error_message ?? null
  };
}

export function parseAgentTickRollup(
  rollup: string | AgentTickRollupDocument
): AgentTickRollupDocument {
  const parsed =
    typeof rollup === "string" ? decodeStructuredString(rollup) : rollup;
  const rollupDocument = documentEnvelope(parsed);

  if (rollupDocument?.document_type === "agent_tick_rollup") {
    return rollupDocument.payload as AgentTickRollupDocument;
  }

  if (
    isRecord(parsed) &&
    typeof parsed.tick_start === "number" &&
    typeof parsed.tick_end === "number" &&
    typeof parsed.agent_entity_id === "number"
  ) {
    return parsed as unknown as AgentTickRollupDocument;
  }

  throw new Error("Invalid agent tick rollup payload");
}

export function summarizeAgentTickRollup(
  rollup: AgentTickRollupDocument
): TickRollupSummary {
  return {
    agentEntityId: rollup.agent_entity_id,
    tickStart: rollup.tick_start,
    tickEnd: rollup.tick_end,
    totalDistance: Number(rollup.total_distance.toFixed(2)),
    rejectedActionCount: rollup.rejected_action_count,
    toolErrorCount: rollup.tool_error_count,
    averageToolLatencyMs: Number(rollup.average_tool_latency_ms.toFixed(2)),
    visibleEntityCount: rollup.visible_entity_count,
    audibleEventCount: rollup.audible_event_count,
    messageCount: rollup.message_count
  };
}

export function parseLiveDebugDocument(document: string): LiveDebugDocument {
  const parsed = decodeStructuredString(document);
  const envelope = documentEnvelope(parsed);

  switch (envelope?.document_type) {
    case "tick_telemetry_envelope":
    case "tick_telemetry_frame":
    case "versioned_tick_telemetry":
      return {
        kind: "tickTelemetry",
        documentType: envelope.document_type,
        payload: parseTickTelemetryEnvelope(document)
      };
    case "agent_tool_call_event":
      return {
        kind: "toolCallEvent",
        documentType: envelope.document_type,
        payload: parseAgentToolCallEvent(document)
      };
    case "agent_tick_rollup":
      return {
        kind: "tickRollup",
        documentType: envelope.document_type,
        payload: parseAgentTickRollup(document)
      };
    case "replay_file":
      return {
        kind: "replay",
        documentType: envelope.document_type,
        payload: parseReplayFile(document)
      };
    case "shard_incident_summary":
      return {
        kind: "incident",
        documentType: envelope.document_type,
        payload: parseShardIncidentSummary(document)
      };
    default:
      throw new Error("Unsupported live debug document payload");
  }
}

export function parseDirectConnectServerMessage(
  message: string | DirectConnectServerMessage
): DirectConnectServerMessage {
  if (typeof message !== "string") {
    return message;
  }

  const parsed = decodeStructuredString(message);

  const welcome = variantPayload(parsed, "Welcome");
  if (isRecord(welcome)) {
    const snapshot = parseNetworkWorldSnapshot(welcome.snapshot);
    if (
      snapshot &&
      typeof welcome.client_id === "string" &&
      typeof welcome.reconnect_token === "string" &&
      typeof welcome.tick === "number" &&
      typeof welcome.authoritative_digest === "number"
    ) {
      return {
        kind: "welcome",
        clientId: welcome.client_id,
        reconnectToken: welcome.reconnect_token,
        tick: welcome.tick,
        controlledEntity: optionalNumber(welcome.controlled_entity) ?? null,
        authoritativeDigest: welcome.authoritative_digest,
        snapshot
      };
    }
  }

  const stateDelta = variantPayload(parsed, "StateDelta");
  if (isRecord(stateDelta)) {
    const delta = parseNetworkStateDelta(stateDelta.delta);
    if (
      delta &&
      typeof stateDelta.tick === "number" &&
      typeof stateDelta.authoritative_digest === "number" &&
      typeof stateDelta.is_full_snapshot === "boolean"
    ) {
      return {
        kind: "stateDelta",
        tick: stateDelta.tick,
        acknowledgedActionTick: optionalNumber(stateDelta.acknowledged_action_tick) ?? null,
        authoritativeDigest: stateDelta.authoritative_digest,
        isFullSnapshot: stateDelta.is_full_snapshot,
        delta
      };
    }
  }

  const eventBatch = variantPayload(parsed, "EventBatch");
  if (
    isRecord(eventBatch) &&
    typeof eventBatch.tick === "number" &&
    Array.isArray(eventBatch.events)
  ) {
    return {
      kind: "eventBatch",
      tick: eventBatch.tick,
      events: eventBatch.events
    };
  }

  const tickTelemetry = variantPayload(parsed, "TickTelemetry");
  if (isRecord(tickTelemetry) && typeof tickTelemetry.frame_json === "string") {
    return {
      kind: "tickTelemetry",
      frameJson: tickTelemetry.frame_json
    };
  }

  const debugDocument = variantPayload(parsed, "DebugDocument");
  if (isRecord(debugDocument) && typeof debugDocument.document === "string") {
    return {
      kind: "debugDocument",
      document: debugDocument.document
    };
  }

  const pong = variantPayload(parsed, "Pong");
  if (
    isRecord(pong) &&
    typeof pong.client_ts === "number" &&
    typeof pong.server_ts === "number"
  ) {
    return {
      kind: "pong",
      clientTimestamp: pong.client_ts,
      serverTimestamp: pong.server_ts
    };
  }

  const rejected = variantPayload(parsed, "Rejected");
  if (isRecord(rejected) && typeof rejected.reason === "string") {
    return {
      kind: "rejected",
      reason: rejected.reason
    };
  }

  throw new Error("Invalid direct-connect server message payload");
}

export function encodeDirectConnectConnectMessage(
  playerName: string,
  reconnectToken?: string | null
): string {
  return JSON.stringify({
    Connect: {
      player_name: playerName,
      reconnect_token: reconnectToken ?? null
    }
  });
}

export function encodeDirectConnectDebugTelemetryMessage(enabled: boolean): string {
  return JSON.stringify({
    SetDebugTelemetry: {
      enabled
    }
  });
}

export function encodeDirectConnectFullSnapshotRequest(
  lastKnownTick?: number | null,
  lastKnownDigest?: number | null
): string {
  return JSON.stringify({
    RequestFullSnapshot: {
      last_known_tick: lastKnownTick ?? null,
      last_known_digest: lastKnownDigest ?? null
    }
  });
}

export function encodeDirectConnectActionBatch(
  tick: number,
  actions: BrowserAction[]
): string {
  return JSON.stringify({
    ActionBatch: {
      tick,
      actions: actions.map((action) => encodeBrowserAction(action))
    }
  });
}

export function applyNetworkStateDelta(
  currentSnapshot: NetworkWorldSnapshot | null,
  delta: NetworkStateDelta,
  isFullSnapshot: boolean
): NetworkWorldSnapshot {
  if (isFullSnapshot) {
    return {
      tick: delta.tick,
      entities: delta.updated.slice().sort((left, right) => left.id - right.id)
    };
  }

  if (currentSnapshot == null) {
    throw new Error("Cannot apply delta update without a baseline snapshot");
  }

  const entitiesById = new Map<number, NetworkEntitySnapshot>();
  for (const entity of currentSnapshot.entities) {
    entitiesById.set(entity.id, entity);
  }
  for (const entity of delta.updated) {
    entitiesById.set(entity.id, entity);
  }
  for (const entityId of delta.destroyed) {
    entitiesById.delete(entityId);
  }

  return {
    tick: delta.tick,
    entities: Array.from(entitiesById.values()).sort((left, right) => left.id - right.id)
  };
}

const WORLD_TO_RENDER_SCALE = 0.08;
const GROUND_RING_ROTATION: Vec4Tuple = [-Math.SQRT1_2, 0, 0, Math.SQRT1_2];

interface EntityRenderProfile {
  mesh: string;
  material: string;
  tint: RgbaTuple;
  emissive: Vec3Tuple;
  scale: Vec3Tuple;
  layer: number;
  renderOrder: number;
  roughness: number;
  metallic: number;
}

function yawQuaternion(yaw: number): Vec4Tuple {
  return [0, Math.sin(yaw * 0.5), 0, Math.cos(yaw * 0.5)];
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function defaultViewportWidth(): number {
  if (typeof window === "undefined") {
    return 1280;
  }
  return window.innerWidth;
}

function defaultViewportHeight(): number {
  if (typeof window === "undefined") {
    return 720;
  }
  return window.innerHeight;
}

function defaultFrameHints(): ThreeJsWebGpuHints {
  return {
    renderer: "three/webgpu",
    preferredBackend: "webgpu",
    fallbackBackend: "webgl2",
    useInstancing: true,
    sortMetric: "world-z",
    sortOpaqueFrontToBack: true,
    preserveInstanceOrder: true,
    sortTransparentBackToFront: true,
    transparentInstancingStrategy: "shared-sort-depth",
    opaqueDepthWrite: true,
    transparentDepthWrite: false,
    maxPixelRatio: 2
  };
}

function encodeBrowserAction(action: BrowserAction): unknown {
  switch (action.kind) {
    case "move":
      return { Move: { direction: action.direction } };
    case "stop":
      return { Stop: null };
    case "rotate":
      return { Rotate: { angle: action.angle } };
    case "lookAt":
      return { LookAt: { target: action.target } };
    case "attack":
      return { Attack: null };
    case "attackTarget":
      return { AttackTarget: { target: action.target } };
    case "interact":
      return { Interact: null };
    case "interactWith":
      return { InteractWith: { target: action.target } };
    case "gatherResource":
      return {
        GatherResource: {
          target: action.target,
          skill: action.skill
        }
      };
    case "loot":
      return { Loot: { target: action.target } };
    case "captureCreature":
      return {
        CaptureCreature: {
          target: action.target,
          tool_slot: action.toolSlot ?? null
        }
      };
    case "summonCompanion":
      return { SummonCompanion: { slot: action.slot } };
    case "commandCompanion":
      return {
        CommandCompanion: {
          slot: action.slot,
          command: action.command,
          target: action.target ?? null
        }
      };
    case "speak":
      return {
        Speak: {
          message: action.message,
          volume: action.volume
        }
      };
    case "setAutoRetaliate":
      return {
        SetAutoRetaliate: {
          enabled: action.enabled
        }
      };
    case "idle":
      return { Idle: null };
  }
}

function healthBand(entity: NetworkEntitySnapshot): "healthy" | "wounded" | "critical" | "neutral" {
  if (entity.health == null || entity.maxHealth == null || entity.maxHealth <= 0) {
    return "neutral";
  }

  const ratio = entity.health / entity.maxHealth;
  if (ratio < 0.35) {
    return "critical";
  }
  if (ratio < 0.7) {
    return "wounded";
  }
  return "healthy";
}

function entityRenderProfile(
  entity: NetworkEntitySnapshot,
  controlledEntity: number | null
): EntityRenderProfile {
  const label = entity.label?.toLowerCase() ?? "";
  const band = healthBand(entity);

  if (label.includes("wall")) {
    return {
      mesh: "basalt-column",
      material: "obsidian-wall",
      tint: [0.24, 0.3, 0.38, 1],
      emissive: [0.01, 0.02, 0.03],
      scale: [3.8, 3.4, 1.1],
      layer: 0,
      renderOrder: 0,
      roughness: 0.94,
      metallic: 0.06
    };
  }

  if (label.includes("obstacle") || label.includes("rock") || label.includes("boulder")) {
    return {
      mesh: "weathered-boulder",
      material: "arena-stone",
      tint: [0.5, 0.42, 0.32, 1],
      emissive: [0.02, 0.015, 0.01],
      scale: [2.1, 1.6, 2.1],
      layer: 1,
      renderOrder: 1,
      roughness: 0.96,
      metallic: 0.04
    };
  }

  if (
    label.includes("monster") ||
    label.includes("creature") ||
    label.includes("beast") ||
    label.includes("npc")
  ) {
    return {
      mesh: "rift-beast",
      material: `rift-hide:${band}`,
      tint:
        band === "critical"
          ? [0.86, 0.34, 0.28, 1]
          : band === "wounded"
            ? [0.82, 0.56, 0.34, 1]
            : [0.72, 0.52, 0.4, 1],
      emissive: band === "critical" ? [0.12, 0.03, 0.02] : [0.04, 0.02, 0.01],
      scale: [1.6, 1.9, 1.6],
      layer: 2,
      renderOrder: 2,
      roughness: 0.82,
      metallic: 0.08
    };
  }

  if (
    label.includes("companion") ||
    label.includes("pet") ||
    label.includes("summon") ||
    label.includes("spirit")
  ) {
    return {
      mesh: "spirit-companion",
      material: `summon-shell:${band}`,
      tint: [0.42, 0.88, 0.74, 1],
      emissive: [0.06, 0.16, 0.12],
      scale: [1.0, 1.35, 1.0],
      layer: 3,
      renderOrder: 3,
      roughness: 0.42,
      metallic: 0.12
    };
  }

  const isControlled = controlledEntity != null && entity.id === controlledEntity;
  return {
    mesh: isControlled ? "adventurer-hero" : "adventurer-avatar",
    material: `traveler-cloth:${band}:${isControlled ? "hero" : "party"}`,
    tint: isControlled
      ? [0.92, 0.84, 0.58, 1]
      : band === "critical"
        ? [0.82, 0.38, 0.34, 1]
        : band === "wounded"
          ? [0.44, 0.76, 0.92, 1]
          : [0.36, 0.66, 0.88, 1],
    emissive: isControlled ? [0.08, 0.06, 0.02] : [0.02, 0.03, 0.05],
    scale: isControlled ? [1.2, 2.0, 1.2] : [1.05, 1.85, 1.05],
    layer: 4,
    renderOrder: 4,
    roughness: 0.64,
    metallic: 0.08
  };
}

function meshBatchKey(profile: EntityRenderProfile): string {
  return [
    profile.mesh,
    profile.material,
    profile.layer,
    profile.renderOrder,
    profile.tint.join(":")
  ].join("|");
}

export function buildAuthoritativeWorldFrame(
  snapshot: NetworkWorldSnapshot,
  options: AuthoritativeWorldFrameOptions = {}
): ThreeJsWebGpuFrame {
  const controlledEntity = options.controlledEntity ?? null;
  const controlled = snapshot.entities.find((entity) => entity.id === controlledEntity) ?? null;
  const focus = controlled ?? snapshot.entities[0] ?? null;
  const focusPosition = focus
    ? [focus.position[0] * WORLD_TO_RENDER_SCALE, focus.position[1] * WORLD_TO_RENDER_SCALE]
    : [0, 0];

  let maxDistance = 10;
  for (const entity of snapshot.entities) {
    const dx = (entity.position[0] * WORLD_TO_RENDER_SCALE) - focusPosition[0];
    const dz = (entity.position[1] * WORLD_TO_RENDER_SCALE) - focusPosition[1];
    maxDistance = Math.max(maxDistance, Math.hypot(dx, dz));
  }

  const meshBatches = new Map<string, ThreeJsMeshBatch>();
  const spriteBatches = new Array<ThreeJsSpriteBatch>();

  for (const entity of snapshot.entities) {
    const profile = entityRenderProfile(entity, controlledEntity);
    const position: Vec3Tuple = [
      entity.position[0] * WORLD_TO_RENDER_SCALE,
      profile.scale[1] * 0.5,
      entity.position[1] * WORLD_TO_RENDER_SCALE
    ];
    const instance: ThreeJsInstance = {
      position,
      rotation: yawQuaternion(entity.rotation),
      scale: profile.scale,
      sourceEntity: entity.id
    };

    const batchKey = meshBatchKey(profile);
    const batch = meshBatches.get(batchKey);
    if (batch) {
      batch.instances.push(instance);
    } else {
      meshBatches.set(batchKey, {
        mesh: profile.mesh,
        material: profile.material,
        layer: profile.layer,
        phase: "opaque",
        sortDepth: 0,
        renderOrder: profile.renderOrder,
        transparent: false,
        doubleSided: false,
        castShadows: true,
        receiveShadows: true,
        tint: profile.tint,
        roughness: profile.roughness,
        metallic: profile.metallic,
        emissive: profile.emissive,
        depthWrite: true,
        depthTest: true,
        instances: [instance]
      });
    }

    const band = healthBand(entity);
    if (controlledEntity != null && entity.id === controlledEntity) {
      spriteBatches.push({
        texture: "selection-ring",
        frame: 0,
        layer: 8,
        billboard: false,
        phase: "transparent",
        sortDepth: position[2],
        renderOrder: 24,
        transparent: true,
        depthWrite: false,
        depthTest: true,
        instances: [
          {
            position: [position[0], 0.08, position[2]],
            rotation: GROUND_RING_ROTATION,
            scale: [2.6, 2.6, 1],
            color: [0.62, 0.98, 0.84, 0.34],
            sourceEntity: entity.id
          }
        ]
      });
    } else if (band === "critical") {
      spriteBatches.push({
        texture: "danger-ring",
        frame: 0,
        layer: 7,
        billboard: false,
        phase: "transparent",
        sortDepth: position[2],
        renderOrder: 20,
        transparent: true,
        depthWrite: false,
        depthTest: true,
        instances: [
          {
            position: [position[0], 0.06, position[2]],
            rotation: GROUND_RING_ROTATION,
            scale: [2.1, 2.1, 1],
            color: [0.92, 0.34, 0.3, 0.22],
            sourceEntity: entity.id
          }
        ]
      });
    }
  }

  return {
    camera: {
      x: focusPosition[0],
      y: focusPosition[1],
      zoom: clamp(1.45 - maxDistance * 0.025, 0.6, 1.45),
      rotation: focus?.rotation ?? 0.35,
      viewportWidth: options.viewportWidth ?? defaultViewportWidth(),
      viewportHeight: options.viewportHeight ?? defaultViewportHeight()
    },
    backgroundColor: options.backgroundColor ?? [0.04, 0.06, 0.1, 1],
    overlayCommands: [],
    meshBatches: Array.from(meshBatches.values()).sort((left, right) => {
      if (left.renderOrder !== right.renderOrder) {
        return left.renderOrder - right.renderOrder;
      }
      return left.material.localeCompare(right.material);
    }),
    spriteBatches,
    hints: defaultFrameHints()
  };
}

export function legacyFrameToThreeJsFrame(frame: RenderFrame): ThreeJsWebGpuFrame {
  return {
    camera: frame.camera,
    backgroundColor: frame.backgroundColor,
    overlayCommands: frame.commands.filter((command) => command.visible),
    meshBatches: [],
    spriteBatches: [],
    hints: defaultFrameHints()
  };
}
