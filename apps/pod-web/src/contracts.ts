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

interface ToonDocumentEnvelope<T = unknown> {
  document_type: string;
  payload: T;
}

type JsonRecord = Record<string, unknown>;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
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

export function legacyFrameToThreeJsFrame(frame: RenderFrame): ThreeJsWebGpuFrame {
  return {
    camera: frame.camera,
    backgroundColor: frame.backgroundColor,
    overlayCommands: frame.commands.filter((command) => command.visible),
    meshBatches: [],
    spriteBatches: [],
    hints: {
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
    }
  };
}
