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

export function parseTickTelemetryEnvelope(
  frame: string | TickTelemetryFrame | TickTelemetryEnvelope
): TickTelemetryEnvelope {
  const parsed =
    typeof frame === "string"
      ? (JSON.parse(frame) as TickTelemetryFrame | TickTelemetryEnvelope)
      : frame;

  if ("agents" in parsed) {
    return { tickTelemetry: parsed };
  }

  if ("tickTelemetry" in parsed) {
    return parsed;
  }

  const snakeCase = parsed as {
    tick_telemetry?: TickTelemetryFrame;
    recovery?: CatchUpDiagnostics | null;
  };
  if (snakeCase.tick_telemetry) {
    return {
      tickTelemetry: snakeCase.tick_telemetry,
      recovery: snakeCase.recovery ?? null
    };
  }

  throw new Error("Invalid tick telemetry payload");
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
