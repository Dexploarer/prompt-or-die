import { decode as decodeToon } from "@toon-format/toon";

import { sampleLandscapeSurface, sampleSurfaceHeight } from "./landscape";
import { meshGroundAnchorHeight } from "./mesh-bounds";

export type Vec3Tuple = [number, number, number];
export type Vec4Tuple = [number, number, number, number];
export type Vec2Tuple = [number, number];
export type RgbaTuple = [number, number, number, number];

export interface CameraState {
  x: number;
  y: number;
  zoom: number;
  rotation: number;
  fov?: number;
  pitch?: number;
  focusHeight?: number;
  followDistance?: number;
  shoulderOffset?: number;
  leadX?: number;
  leadY?: number;
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
  animationSetId?: string;
  motionSpeed?: number;
  healthRatio?: number | null;
  controlled?: boolean;
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

export interface ThreeJsEnvironment {
  biomeId: string;
  skyColor: RgbaTuple;
  fogColor: RgbaTuple;
  fogNear: number;
  fogFar: number;
  ambientColor: Vec3Tuple;
  ambientIntensity: number;
  sunColor: Vec3Tuple;
  sunIntensity: number;
  sunDirection: Vec3Tuple;
  fillColor: Vec3Tuple;
  fillIntensity: number;
  fillDirection: Vec3Tuple;
  rimColor: Vec3Tuple;
  rimIntensity: number;
  groundColor: RgbaTuple;
  starfieldIntensity: number;
}

export interface ThreeJsWebGpuFrame {
  camera: CameraState;
  backgroundColor: RgbaTuple;
  environment: ThreeJsEnvironment;
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

export interface FocusedEntityDebugSummaryDocument {
  shard_id: string;
  entity_id: number;
  latest_tick: number;
  tool_call_count: number;
  tool_error_count: number;
  rejected_action_count: number;
  total_distance: number;
  average_tool_latency_ms: number;
  visible_entity_count: number;
  audible_event_count: number;
  message_count: number;
  latest_tool_name?: string | null;
  latest_tool_status?: string | null;
  latest_tool_error?: string | null;
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
  | { kind: "focusedSummary"; documentType: string; payload: FocusedEntityDebugSummaryDocument }
  | { kind: "replay"; documentType: string; payload: ReplayFileDocument }
  | { kind: "incident"; documentType: string; payload: ShardIncidentSummary };

export type NetworkEntityKind =
  | "Unknown"
  | "Player"
  | "Npc"
  | "WildCreature"
  | "Companion"
  | "ResourceNode"
  | "LootContainer"
  | "Scenery";

export type NetworkCombatStyle = "Melee" | "Ranged" | "Magic" | "Summoning";

export type NetworkSkillKind =
  | "Attack"
  | "Strength"
  | "Defence"
  | "Ranged"
  | "Magic"
  | "Constitution"
  | "Mining"
  | "Woodcutting"
  | "Fishing"
  | "Cooking"
  | "Smithing"
  | "Crafting"
  | "Slayer"
  | "Taming"
  | "Bonding";

export type NetworkEncounterKind =
  | "OpenWorld"
  | "Duel"
  | "WildCreature"
  | "Boss"
  | "Raid";

export interface NetworkEntityInteractionHints {
  canInspect: boolean;
  canInteract: boolean;
  canAttack: boolean;
  canGather: boolean;
  canLoot: boolean;
  canCapture: boolean;
  canCommandCompanion: boolean;
  canChat: boolean;
}

export type NetworkFactionDisposition = "Friendly" | "Neutral" | "Hostile";

export interface NetworkFactionAffiliation {
  factionId: string;
  roleId: string;
  disposition: NetworkFactionDisposition;
  influenceRadius: number;
}

export interface NetworkQuestAnchor {
  questIds: string[];
  primaryPrompt: string;
  stageTags: string[];
}

export interface NetworkEncounterProfile {
  tableId: string;
  difficultyTier: number;
  recommendedPartySize: number;
  respawnTicks: number;
}

export interface NetworkSpawnProfile {
  profileId: string;
  biomeId: string;
  spawnGroup: string;
  respawnTicks: number;
  leashRadius: number;
}

export interface NetworkAtmosphereProfile {
  biomeId: string;
  skyColor: RgbaTuple;
  fogColor: RgbaTuple;
  fogNear: number;
  fogFar: number;
  ambientColor: Vec3Tuple;
  ambientIntensity: number;
  sunColor: Vec3Tuple;
  sunIntensity: number;
  sunDirection: Vec3Tuple;
  fillColor: Vec3Tuple;
  fillIntensity: number;
  fillDirection: Vec3Tuple;
  rimColor: Vec3Tuple;
  rimIntensity: number;
  groundColor: RgbaTuple;
  starfieldIntensity: number;
}

export interface NetworkAtmosphereVolume {
  radius: number;
  priority: number;
}

export interface NetworkActorPresentation {
  profileId: string;
  meshAssetId: string | null;
  materialPaletteId: string;
  animationSetId: string;
  scaleMultiplier: number;
  footprintRadius: number;
  selectionRingScale: number;
  auraColor: RgbaTuple;
}

export interface NetworkCombatPresentation {
  profileId: string;
  hitFlashColor: RgbaTuple;
  criticalRingColor: RgbaTuple;
  selectionRingColor: RgbaTuple;
  emissiveBoost: Vec3Tuple;
  impactScale: number;
}

export interface NetworkEntityMetadataSnapshot {
  kind: NetworkEntityKind;
  chunkKey: string | null;
  regionId: string | null;
  regionName: string | null;
  teamId: number | null;
  questGraphIds: string[];
  factionTrackId: string | null;
  encounterTableId: string | null;
  combatStyle: NetworkCombatStyle | null;
  speciesId: string | null;
  speciesName: string | null;
  resourceSkill: NetworkSkillKind | null;
  resourceTier: number | null;
  encounterKind: NetworkEncounterKind | null;
  faction: NetworkFactionAffiliation | null;
  questAnchor: NetworkQuestAnchor | null;
  encounterProfile: NetworkEncounterProfile | null;
  spawnProfile: NetworkSpawnProfile | null;
  atmosphere: NetworkAtmosphereProfile | null;
  atmosphereVolume: NetworkAtmosphereVolume | null;
  actorPresentation: NetworkActorPresentation | null;
  combatPresentation: NetworkCombatPresentation | null;
  interaction: NetworkEntityInteractionHints;
}

export interface NetworkEntitySnapshot {
  id: number;
  position: Vec2Tuple;
  velocity: Vec2Tuple;
  rotation: number;
  health?: number | null;
  maxHealth?: number | null;
  movementSpeed?: number | null;
  label?: string | null;
  metadata: NetworkEntityMetadataSnapshot;
}

export interface NetworkPopulationBreakdown {
  players: number;
  npcs: number;
  wildCreatures: number;
  companions: number;
  resourceNodes: number;
  lootContainers: number;
  scenery: number;
}

export interface NetworkChunkPopulationState {
  chunkKey: string;
  regionId: string | null;
  regionName: string | null;
  biomeId: string | null;
  questGraphIds: string[];
  factionTrackId: string | null;
  encounterTableIds: string[];
  counts: NetworkPopulationBreakdown;
  activeEntityCount: number;
  ambientPopulationCap: number;
  spawnBudgetRemaining: number;
  pendingRespawns: number;
  nextRespawnTick: number | null;
  populationPressure: number;
}

export interface NetworkRegionPopulationState {
  regionId: string;
  regionName: string;
  primaryBiomeId: string;
  chunkKeys: string[];
  activeQuestGraphIds: string[];
  dominantFactionTrackId: string | null;
  encounterTableIds: string[];
  activeChunkCount: number;
  counts: NetworkPopulationBreakdown;
  activeEntityCount: number;
  ambientPopulationCap: number;
  spawnBudgetRemaining: number;
  pendingRespawns: number;
  nextRespawnTick: number | null;
  populationPressure: number;
}

export interface NetworkWorldPopulationState {
  tick: number;
  chunks: NetworkChunkPopulationState[];
  regions: NetworkRegionPopulationState[];
}

export interface NetworkWorldSnapshot {
  tick: number;
  entities: NetworkEntitySnapshot[];
  population: NetworkWorldPopulationState;
}

export interface NetworkStateDelta {
  tick: number;
  updated: NetworkEntitySnapshot[];
  destroyed: number[];
  population: NetworkWorldPopulationState;
}

export interface NetworkGameEvent {
  tick: number;
  origin: Vec2Tuple | null;
  kind: string;
  summary: string;
  entityIds: number[];
}

export interface NetworkEventBatch {
  tick: number;
  events: NetworkGameEvent[];
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
  | { kind: "eventBatch"; tick: number; events: NetworkGameEvent[] }
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

function vec3Tuple(value: unknown): Vec3Tuple | null {
  if (
    Array.isArray(value) &&
    value.length >= 3 &&
    typeof value[0] === "number" &&
    typeof value[1] === "number" &&
    typeof value[2] === "number"
  ) {
    return [value[0], value[1], value[2]];
  }

  if (isRecord(value)) {
    const x = asNumber(value.x);
    const y = asNumber(value.y);
    const z = asNumber(value.z);
    if (x != null && y != null && z != null) {
      return [x, y, z];
    }
  }

  return null;
}

function rgbaTuple(value: unknown): RgbaTuple | null {
  if (
    Array.isArray(value) &&
    value.length >= 4 &&
    typeof value[0] === "number" &&
    typeof value[1] === "number" &&
    typeof value[2] === "number" &&
    typeof value[3] === "number"
  ) {
    return [value[0], value[1], value[2], value[3]];
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
    label: optionalString(value.label) ?? null,
    metadata: parseNetworkEntityMetadata(value.metadata)
  };
}

function parseNetworkEntityMetadata(value: unknown): NetworkEntityMetadataSnapshot {
  if (!isRecord(value)) {
    return defaultNetworkEntityMetadata();
  }

  const combatStyle = value.combat_style ?? value.combatStyle;
  const resourceSkill = value.resource_skill ?? value.resourceSkill;
  const encounterKind = value.encounter_kind ?? value.encounterKind;

  return {
    kind: isEntityKind(value.kind) ? value.kind : "Unknown",
    chunkKey: optionalString(value.chunk_key ?? value.chunkKey) ?? null,
    regionId: optionalString(value.region_id ?? value.regionId) ?? null,
    regionName: optionalString(value.region_name ?? value.regionName) ?? null,
    teamId: optionalNumber(value.team_id ?? value.teamId) ?? null,
    questGraphIds: parseStringArray(value.quest_graph_ids ?? value.questGraphIds) ?? [],
    factionTrackId:
      optionalString(value.faction_track_id ?? value.factionTrackId) ?? null,
    encounterTableId:
      optionalString(value.encounter_table_id ?? value.encounterTableId) ?? null,
    combatStyle: isCombatStyle(combatStyle) ? combatStyle : null,
    speciesId: optionalString(value.species_id ?? value.speciesId) ?? null,
    speciesName: optionalString(value.species_name ?? value.speciesName) ?? null,
    resourceSkill: isSkillKind(resourceSkill) ? resourceSkill : null,
    resourceTier: optionalNumber(value.resource_tier ?? value.resourceTier) ?? null,
    encounterKind: isEncounterKind(encounterKind) ? encounterKind : null,
    faction: parseFactionAffiliation(value.faction),
    questAnchor: parseQuestAnchor(value.quest_anchor ?? value.questAnchor),
    encounterProfile: parseEncounterProfile(
      value.encounter_profile ?? value.encounterProfile
    ),
    spawnProfile: parseSpawnProfile(value.spawn_profile ?? value.spawnProfile),
    atmosphere: parseAtmosphereProfile(value.atmosphere),
    atmosphereVolume: parseAtmosphereVolume(value.atmosphere_volume ?? value.atmosphereVolume),
    actorPresentation: parseActorPresentation(
      value.actor_presentation ?? value.actorPresentation
    ),
    combatPresentation: parseCombatPresentation(
      value.combat_presentation ?? value.combatPresentation
    ),
    interaction: parseInteractionHints(value.interaction)
  };
}

function parseStringArray(value: unknown): string[] | null {
  if (!Array.isArray(value)) {
    return null;
  }
  const strings = value.filter((entry): entry is string => typeof entry === "string");
  return strings.length === value.length ? strings : null;
}

function parseFactionAffiliation(value: unknown): NetworkFactionAffiliation | null {
  if (!isRecord(value)) {
    return null;
  }

  const factionId = optionalString(value.faction_id ?? value.factionId);
  const roleId = optionalString(value.role_id ?? value.roleId);
  const disposition = value.disposition;
  const influenceRadius = asNumber(value.influence_radius ?? value.influenceRadius);
  if (
    factionId == null ||
    roleId == null ||
    !isFactionDisposition(disposition) ||
    influenceRadius == null
  ) {
    return null;
  }

  return {
    factionId,
    roleId,
    disposition,
    influenceRadius
  };
}

function parseQuestAnchor(value: unknown): NetworkQuestAnchor | null {
  if (!isRecord(value)) {
    return null;
  }

  const questIds = parseStringArray(value.quest_ids ?? value.questIds);
  const primaryPrompt = optionalString(value.primary_prompt ?? value.primaryPrompt);
  const stageTags = parseStringArray(value.stage_tags ?? value.stageTags);
  if (questIds == null || primaryPrompt == null || stageTags == null) {
    return null;
  }

  return {
    questIds,
    primaryPrompt,
    stageTags
  };
}

function parseEncounterProfile(value: unknown): NetworkEncounterProfile | null {
  if (!isRecord(value)) {
    return null;
  }

  const tableId = optionalString(value.table_id ?? value.tableId);
  const difficultyTier = asNumber(value.difficulty_tier ?? value.difficultyTier);
  const recommendedPartySize = asNumber(
    value.recommended_party_size ?? value.recommendedPartySize
  );
  const respawnTicks = asNumber(value.respawn_ticks ?? value.respawnTicks);
  if (
    tableId == null ||
    difficultyTier == null ||
    recommendedPartySize == null ||
    respawnTicks == null
  ) {
    return null;
  }

  return {
    tableId,
    difficultyTier,
    recommendedPartySize,
    respawnTicks
  };
}

function parseSpawnProfile(value: unknown): NetworkSpawnProfile | null {
  if (!isRecord(value)) {
    return null;
  }

  const profileId = optionalString(value.profile_id ?? value.profileId);
  const biomeId = optionalString(value.biome_id ?? value.biomeId);
  const spawnGroup = optionalString(value.spawn_group ?? value.spawnGroup);
  const respawnTicks = asNumber(value.respawn_ticks ?? value.respawnTicks);
  const leashRadius = asNumber(value.leash_radius ?? value.leashRadius);
  if (
    profileId == null ||
    biomeId == null ||
    spawnGroup == null ||
    respawnTicks == null ||
    leashRadius == null
  ) {
    return null;
  }

  return {
    profileId,
    biomeId,
    spawnGroup,
    respawnTicks,
    leashRadius
  };
}

function parseAtmosphereProfile(value: unknown): NetworkAtmosphereProfile | null {
  if (!isRecord(value)) {
    return null;
  }

  const biomeId = optionalString(value.biome_id ?? value.biomeId);
  const skyColor = rgbaTuple(value.sky_color ?? value.skyColor);
  const fogColor = rgbaTuple(value.fog_color ?? value.fogColor);
  const fogNear = asNumber(value.fog_near ?? value.fogNear);
  const fogFar = asNumber(value.fog_far ?? value.fogFar);
  const ambientColor = vec3Tuple(value.ambient_color ?? value.ambientColor);
  const ambientIntensity = asNumber(value.ambient_intensity ?? value.ambientIntensity);
  const sunColor = vec3Tuple(value.sun_color ?? value.sunColor);
  const sunIntensity = asNumber(value.sun_intensity ?? value.sunIntensity);
  const sunDirection = vec3Tuple(value.sun_direction ?? value.sunDirection);
  const fillColor = vec3Tuple(value.fill_color ?? value.fillColor);
  const fillIntensity = asNumber(value.fill_intensity ?? value.fillIntensity);
  const fillDirection = vec3Tuple(value.fill_direction ?? value.fillDirection);
  const rimColor = vec3Tuple(value.rim_color ?? value.rimColor);
  const rimIntensity = asNumber(value.rim_intensity ?? value.rimIntensity);
  const groundColor = rgbaTuple(value.ground_color ?? value.groundColor);
  const starfieldIntensity = asNumber(value.starfield_intensity ?? value.starfieldIntensity);

  if (
    biomeId == null ||
    skyColor == null ||
    fogColor == null ||
    fogNear == null ||
    fogFar == null ||
    ambientColor == null ||
    ambientIntensity == null ||
    sunColor == null ||
    sunIntensity == null ||
    sunDirection == null ||
    fillColor == null ||
    fillIntensity == null ||
    fillDirection == null ||
    rimColor == null ||
    rimIntensity == null ||
    groundColor == null ||
    starfieldIntensity == null
  ) {
    return null;
  }

  return {
    biomeId,
    skyColor,
    fogColor,
    fogNear,
    fogFar,
    ambientColor,
    ambientIntensity,
    sunColor,
    sunIntensity,
    sunDirection,
    fillColor,
    fillIntensity,
    fillDirection,
    rimColor,
    rimIntensity,
    groundColor,
    starfieldIntensity
  };
}

function parseAtmosphereVolume(value: unknown): NetworkAtmosphereVolume | null {
  if (!isRecord(value)) {
    return null;
  }

  const radius = asNumber(value.radius);
  const priority = asNumber(value.priority);
  if (radius == null || priority == null) {
    return null;
  }

  return {
    radius,
    priority
  };
}

function parseActorPresentation(value: unknown): NetworkActorPresentation | null {
  if (!isRecord(value)) {
    return null;
  }

  const profileId = optionalString(value.profile_id ?? value.profileId);
  const materialPaletteId = optionalString(
    value.material_palette_id ?? value.materialPaletteId
  );
  const animationSetId = optionalString(value.animation_set_id ?? value.animationSetId);
  const scaleMultiplier = asNumber(value.scale_multiplier ?? value.scaleMultiplier);
  const footprintRadius = asNumber(value.footprint_radius ?? value.footprintRadius);
  const selectionRingScale = asNumber(
    value.selection_ring_scale ?? value.selectionRingScale
  );
  const auraColor = rgbaTuple(value.aura_color ?? value.auraColor);

  if (
    profileId == null ||
    materialPaletteId == null ||
    animationSetId == null ||
    scaleMultiplier == null ||
    footprintRadius == null ||
    selectionRingScale == null ||
    auraColor == null
  ) {
    return null;
  }

  return {
    profileId,
    meshAssetId: optionalString(value.mesh_asset_id ?? value.meshAssetId) ?? null,
    materialPaletteId,
    animationSetId,
    scaleMultiplier,
    footprintRadius,
    selectionRingScale,
    auraColor
  };
}

function parseCombatPresentation(value: unknown): NetworkCombatPresentation | null {
  if (!isRecord(value)) {
    return null;
  }

  const profileId = optionalString(value.profile_id ?? value.profileId);
  const hitFlashColor = rgbaTuple(value.hit_flash_color ?? value.hitFlashColor);
  const criticalRingColor = rgbaTuple(
    value.critical_ring_color ?? value.criticalRingColor
  );
  const selectionRingColor = rgbaTuple(
    value.selection_ring_color ?? value.selectionRingColor
  );
  const emissiveBoost = vec3Tuple(value.emissive_boost ?? value.emissiveBoost);
  const impactScale = asNumber(value.impact_scale ?? value.impactScale);

  if (
    profileId == null ||
    hitFlashColor == null ||
    criticalRingColor == null ||
    selectionRingColor == null ||
    emissiveBoost == null ||
    impactScale == null
  ) {
    return null;
  }

  return {
    profileId,
    hitFlashColor,
    criticalRingColor,
    selectionRingColor,
    emissiveBoost,
    impactScale
  };
}

function parseInteractionHints(value: unknown): NetworkEntityInteractionHints {
  if (!isRecord(value)) {
    return defaultInteractionHints();
  }

  return {
    canInspect: Boolean(value.can_inspect ?? value.canInspect),
    canInteract: Boolean(value.can_interact ?? value.canInteract),
    canAttack: Boolean(value.can_attack ?? value.canAttack),
    canGather: Boolean(value.can_gather ?? value.canGather),
    canLoot: Boolean(value.can_loot ?? value.canLoot),
    canCapture: Boolean(value.can_capture ?? value.canCapture),
    canCommandCompanion: Boolean(
      value.can_command_companion ?? value.canCommandCompanion
    ),
    canChat: Boolean(value.can_chat ?? value.canChat)
  };
}

function defaultInteractionHints(): NetworkEntityInteractionHints {
  return {
    canInspect: false,
    canInteract: false,
    canAttack: false,
    canGather: false,
    canLoot: false,
    canCapture: false,
    canCommandCompanion: false,
    canChat: false
  };
}

function defaultPopulationBreakdown(): NetworkPopulationBreakdown {
  return {
    players: 0,
    npcs: 0,
    wildCreatures: 0,
    companions: 0,
    resourceNodes: 0,
    lootContainers: 0,
    scenery: 0
  };
}

function parsePopulationBreakdown(value: unknown): NetworkPopulationBreakdown {
  if (!isRecord(value)) {
    return defaultPopulationBreakdown();
  }

  return {
    players: asNumber(value.players) ?? 0,
    npcs: asNumber(value.npcs) ?? 0,
    wildCreatures:
      asNumber(value.wild_creatures ?? value.wildCreatures) ?? 0,
    companions: asNumber(value.companions) ?? 0,
    resourceNodes:
      asNumber(value.resource_nodes ?? value.resourceNodes) ?? 0,
    lootContainers:
      asNumber(value.loot_containers ?? value.lootContainers) ?? 0,
    scenery: asNumber(value.scenery) ?? 0
  };
}

function defaultWorldPopulationState(tick = 0): NetworkWorldPopulationState {
  return {
    tick,
    chunks: [],
    regions: []
  };
}

function parseChunkPopulationState(value: unknown): NetworkChunkPopulationState | null {
  if (!isRecord(value)) {
    return null;
  }

  const chunkKey = optionalString(value.chunk_key ?? value.chunkKey);
  if (chunkKey == null) {
    return null;
  }

  return {
    chunkKey,
    regionId: optionalString(value.region_id ?? value.regionId) ?? null,
    regionName: optionalString(value.region_name ?? value.regionName) ?? null,
    biomeId: optionalString(value.biome_id ?? value.biomeId) ?? null,
    questGraphIds: parseStringArray(value.quest_graph_ids ?? value.questGraphIds) ?? [],
    factionTrackId:
      optionalString(value.faction_track_id ?? value.factionTrackId) ?? null,
    encounterTableIds:
      parseStringArray(value.encounter_table_ids ?? value.encounterTableIds) ?? [],
    counts: parsePopulationBreakdown(value.counts),
    activeEntityCount:
      asNumber(value.active_entity_count ?? value.activeEntityCount) ?? 0,
    ambientPopulationCap:
      asNumber(value.ambient_population_cap ?? value.ambientPopulationCap) ?? 0,
    spawnBudgetRemaining:
      asNumber(value.spawn_budget_remaining ?? value.spawnBudgetRemaining) ?? 0,
    pendingRespawns:
      asNumber(value.pending_respawns ?? value.pendingRespawns) ?? 0,
    nextRespawnTick:
      asNumber(value.next_respawn_tick ?? value.nextRespawnTick) ?? null,
    populationPressure:
      asNumber(value.population_pressure ?? value.populationPressure) ?? 0
  };
}

function parseRegionPopulationState(value: unknown): NetworkRegionPopulationState | null {
  if (!isRecord(value)) {
    return null;
  }

  const regionId = optionalString(value.region_id ?? value.regionId);
  const regionName = optionalString(value.region_name ?? value.regionName);
  const primaryBiomeId = optionalString(
    value.primary_biome_id ?? value.primaryBiomeId
  );
  if (regionId == null || regionName == null || primaryBiomeId == null) {
    return null;
  }

  return {
    regionId,
    regionName,
    primaryBiomeId,
    chunkKeys: parseStringArray(value.chunk_keys ?? value.chunkKeys) ?? [],
    activeQuestGraphIds:
      parseStringArray(value.active_quest_graph_ids ?? value.activeQuestGraphIds) ?? [],
    dominantFactionTrackId:
      optionalString(
        value.dominant_faction_track_id ?? value.dominantFactionTrackId
      ) ?? null,
    encounterTableIds:
      parseStringArray(value.encounter_table_ids ?? value.encounterTableIds) ?? [],
    activeChunkCount:
      asNumber(value.active_chunk_count ?? value.activeChunkCount) ?? 0,
    counts: parsePopulationBreakdown(value.counts),
    activeEntityCount:
      asNumber(value.active_entity_count ?? value.activeEntityCount) ?? 0,
    ambientPopulationCap:
      asNumber(value.ambient_population_cap ?? value.ambientPopulationCap) ?? 0,
    spawnBudgetRemaining:
      asNumber(value.spawn_budget_remaining ?? value.spawnBudgetRemaining) ?? 0,
    pendingRespawns:
      asNumber(value.pending_respawns ?? value.pendingRespawns) ?? 0,
    nextRespawnTick:
      asNumber(value.next_respawn_tick ?? value.nextRespawnTick) ?? null,
    populationPressure:
      asNumber(value.population_pressure ?? value.populationPressure) ?? 0
  };
}

function parseWorldPopulationState(
  value: unknown,
  tickFallback: number
): NetworkWorldPopulationState {
  if (!isRecord(value)) {
    return defaultWorldPopulationState(tickFallback);
  }

  const tick = asNumber(value.tick) ?? tickFallback;
  const chunks = Array.isArray(value.chunks)
    ? value.chunks
        .map((chunk) => parseChunkPopulationState(chunk))
        .filter((chunk): chunk is NetworkChunkPopulationState => chunk != null)
        .sort((left, right) => left.chunkKey.localeCompare(right.chunkKey))
    : [];
  const regions = Array.isArray(value.regions)
    ? value.regions
        .map((region) => parseRegionPopulationState(region))
        .filter((region): region is NetworkRegionPopulationState => region != null)
        .sort((left, right) => left.regionId.localeCompare(right.regionId))
    : [];

  return {
    tick,
    chunks,
    regions
  };
}

function defaultNetworkEntityMetadata(): NetworkEntityMetadataSnapshot {
  return {
    kind: "Unknown",
    chunkKey: null,
    regionId: null,
    regionName: null,
    teamId: null,
    questGraphIds: [],
    factionTrackId: null,
    encounterTableId: null,
    combatStyle: null,
    speciesId: null,
    speciesName: null,
    resourceSkill: null,
    resourceTier: null,
    encounterKind: null,
    faction: null,
    questAnchor: null,
    encounterProfile: null,
    spawnProfile: null,
    atmosphere: null,
    atmosphereVolume: null,
    actorPresentation: null,
    combatPresentation: null,
    interaction: defaultInteractionHints()
  };
}

function isFactionDisposition(value: unknown): value is NetworkFactionDisposition {
  return value === "Friendly" || value === "Neutral" || value === "Hostile";
}

function isEntityKind(value: unknown): value is NetworkEntityKind {
  return (
    typeof value === "string" &&
    [
      "Unknown",
      "Player",
      "Npc",
      "WildCreature",
      "Companion",
      "ResourceNode",
      "LootContainer",
      "Scenery"
    ].includes(value)
  );
}

function isCombatStyle(value: unknown): value is NetworkCombatStyle {
  return (
    typeof value === "string" &&
    ["Melee", "Ranged", "Magic", "Summoning"].includes(value)
  );
}

function isSkillKind(value: unknown): value is NetworkSkillKind {
  return (
    typeof value === "string" &&
    [
      "Attack",
      "Strength",
      "Defence",
      "Ranged",
      "Magic",
      "Constitution",
      "Mining",
      "Woodcutting",
      "Fishing",
      "Cooking",
      "Smithing",
      "Crafting",
      "Slayer",
      "Taming",
      "Bonding"
    ].includes(value)
  );
}

function isEncounterKind(value: unknown): value is NetworkEncounterKind {
  return (
    typeof value === "string" &&
    ["OpenWorld", "Duel", "WildCreature", "Boss", "Raid"].includes(value)
  );
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
    entities,
    population: parseWorldPopulationState(
      value.population ?? value.population_state,
      value.tick
    )
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
    destroyed,
    population: parseWorldPopulationState(
      value.population ?? value.population_state,
      value.tick
    )
  };
}

function entityRef(entityId: number | null | undefined): string {
  return entityId == null ? "Unknown" : `E(${entityId})`;
}

function entityIdsFromPayload(payload: JsonRecord): number[] {
  const ids = new Set<number>();
  for (const key of [
    "entity",
    "target",
    "source",
    "killer",
    "victim",
    "resource",
    "trigger",
    "item"
  ]) {
    const entityId = asNumber(payload[key]);
    if (entityId != null) {
      ids.add(entityId);
    }
  }
  return Array.from(ids.values()).sort((left, right) => left - right);
}

function parseNetworkGameEvent(value: unknown): NetworkGameEvent | null {
  if (!isRecord(value)) {
    return null;
  }

  const tick = asNumber(value.tick);
  const origin = vec2Tuple(value.origin);
  const eventRecord = isRecord(value.event) ? value.event : null;
  if (tick == null || !eventRecord) {
    return null;
  }

  const entries = Object.entries(eventRecord);
  if (entries.length === 0) {
    return null;
  }

  const [kind, rawPayload] = entries[0] ?? [];
  const payload = isRecord(rawPayload) ? rawPayload : {};
  const entityIds = entityIdsFromPayload(payload);

  switch (kind) {
    case "Damage": {
      const amount = asNumber(payload.amount) ?? 0;
      const source = asNumber(payload.source);
      const target = asNumber(payload.target);
      return {
        tick,
        origin,
        kind,
        summary: `${entityRef(source)} hit ${entityRef(target)} for ${amount.toFixed(1)}`,
        entityIds
      };
    }
    case "Kill":
      return {
        tick,
        origin,
        kind,
        summary: `${entityRef(asNumber(payload.killer))} defeated ${entityRef(
          asNumber(payload.victim)
        )}`,
        entityIds
      };
    case "Heal": {
      const amount = asNumber(payload.amount) ?? 0;
      return {
        tick,
        origin,
        kind,
        summary: `${entityRef(asNumber(payload.source))} healed ${entityRef(
          asNumber(payload.target)
        )} for ${amount.toFixed(1)}`,
        entityIds
      };
    }
    case "AgentSpoke": {
      const agentId = asString(payload.agent_id)?.slice(0, 8) ?? "agent";
      const message = asString(payload.message) ?? "";
      return {
        tick,
        origin,
        kind,
        summary: `${agentId}: ${message}`,
        entityIds
      };
    }
    case "CreatureCaptured":
      return {
        tick,
        origin,
        kind,
        summary: `Captured ${asString(payload.species_id) ?? "creature"}`,
        entityIds
      };
    case "CompanionSummoned":
      return {
        tick,
        origin,
        kind,
        summary: `Summoned ${asString(payload.species_id) ?? "companion"}`,
        entityIds
      };
    case "CompanionCommandIssued":
      return {
        tick,
        origin,
        kind,
        summary: `Companion ${asString(payload.command) ?? "command"}${
          asNumber(payload.target) != null ? ` ${entityRef(asNumber(payload.target))}` : ""
        }`,
        entityIds
      };
    case "ResourceGathered":
      return {
        tick,
        origin,
        kind,
        summary: `${entityRef(asNumber(payload.entity))} gathered ${
          asNumber(payload.quantity) ?? 0
        } ${asString(payload.item_id) ?? "resource"}`,
        entityIds
      };
    case "LootClaimed":
      return {
        tick,
        origin,
        kind,
        summary: `${entityRef(asNumber(payload.entity))} looted ${
          asNumber(payload.coins) ?? 0
        } coins`,
        entityIds
      };
    case "AutoRetaliateSet":
      return {
        tick,
        origin,
        kind,
        summary: `${entityRef(asNumber(payload.entity))} auto-retaliate ${
          payload.enabled === true ? "enabled" : "disabled"
        }`,
        entityIds
      };
    case "EntitySpawned":
      return {
        tick,
        origin,
        kind,
        summary: `${asString(payload.entity_type) ?? "Entity"} ${entityRef(
          asNumber(payload.entity)
        )} spawned`,
        entityIds
      };
    case "EntityDestroyed":
      return {
        tick,
        origin,
        kind,
        summary: `${entityRef(asNumber(payload.entity))} destroyed`,
        entityIds
      };
    default:
      return {
        tick,
        origin,
        kind,
        summary: kind.replace(/([A-Z])/g, " $1").trim(),
        entityIds
      };
  }
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

export function parseFocusedEntityDebugSummary(
  summary: string | FocusedEntityDebugSummaryDocument
): FocusedEntityDebugSummaryDocument {
  const parsed =
    typeof summary === "string" ? decodeStructuredString(summary) : summary;
  const focusedDocument = documentEnvelope(parsed);

  if (focusedDocument?.document_type === "focused_entity_debug_summary") {
    return focusedDocument.payload as FocusedEntityDebugSummaryDocument;
  }

  if (
    isRecord(parsed) &&
    typeof parsed.shard_id === "string" &&
    typeof parsed.entity_id === "number" &&
    typeof parsed.latest_tick === "number"
  ) {
    return parsed as unknown as FocusedEntityDebugSummaryDocument;
  }

  throw new Error("Invalid focused entity debug summary payload");
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
    case "focused_entity_debug_summary":
      return {
        kind: "focusedSummary",
        documentType: envelope.document_type,
        payload: parseFocusedEntityDebugSummary(document)
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
        .map((event) => parseNetworkGameEvent(event))
        .filter((event): event is NetworkGameEvent => event != null)
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

export function encodeDirectConnectDebugFocusMessage(
  entityId?: number | null
): string {
  return JSON.stringify({
    SetDebugFocus: {
      entity_id: entityId ?? null
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
      entities: delta.updated.slice().sort((left, right) => left.id - right.id),
      population: delta.population
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
    entities: Array.from(entitiesById.values()).sort((left, right) => left.id - right.id),
    population: delta.population
  };
}

const WORLD_TO_RENDER_SCALE = 1;
const GROUND_RING_ROTATION: Vec4Tuple = [-Math.SQRT1_2, 0, 0, Math.SQRT1_2];

interface EntityRenderProfile {
  mesh: string;
  material: string;
  tint: RgbaTuple;
  emissive: Vec3Tuple;
  scale: Vec3Tuple;
  groundOffset: number;
  layer: number;
  renderOrder: number;
  roughness: number;
  metallic: number;
  selectionRingScale: number;
  selectionRingColor: RgbaTuple;
  criticalRingColor: RgbaTuple;
  auraColor: RgbaTuple;
  impactScale: number;
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

function defaultEnvironment(): ThreeJsEnvironment {
  return {
    biomeId: "neutral-shard",
    skyColor: [0.64, 0.8, 0.98, 1],
    fogColor: [0.73, 0.84, 0.78, 1],
    fogNear: 30,
    fogFar: 196,
    ambientColor: [0.82, 0.92, 0.88],
    ambientIntensity: 1.4,
    sunColor: [1, 0.96, 0.84],
    sunIntensity: 2.95,
    sunDirection: [30, 48, 18],
    fillColor: [0.48, 0.76, 0.94],
    fillIntensity: 0.88,
    fillDirection: [-18, 14, -10],
    rimColor: [0.4, 0.88, 0.78],
    rimIntensity: 8.5,
    groundColor: [0.19, 0.33, 0.21, 1],
    starfieldIntensity: 0.08
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

function healthRatio(entity: NetworkEntitySnapshot): number | null {
  if (entity.health == null || entity.maxHealth == null || entity.maxHealth <= 0) {
    return null;
  }

  return clamp(entity.health / entity.maxHealth, 0, 1);
}

function motionSpeed(entity: NetworkEntitySnapshot): number {
  const speed = Math.hypot(entity.velocity[0], entity.velocity[1]);
  const maxSpeed = entity.movementSpeed ?? 0;
  if (maxSpeed > 0.001) {
    return clamp(speed / maxSpeed, 0, 1.6);
  }

  return clamp(speed / 6, 0, 1.6);
}

function resolveAnimationSetId(entity: NetworkEntitySnapshot): string {
  const actorPresentationId = entity.metadata.actorPresentation?.animationSetId?.trim();
  if (actorPresentationId) {
    return actorPresentationId;
  }

  switch (entity.metadata.kind) {
    case "Player":
    case "Npc":
      return "humanoid-explorer";
    case "WildCreature":
      return "beast-stalker";
    case "Companion":
      return "companion-hover";
    case "Scenery":
    case "ResourceNode":
    case "LootContainer":
      return "static-prop";
    default:
      return "humanoid-idle";
  }
}

function entityUsesWaterSurface(entity: NetworkEntitySnapshot): boolean {
  return (
    entity.metadata.kind === "Player" ||
    entity.metadata.kind === "Npc" ||
    entity.metadata.kind === "WildCreature" ||
    entity.metadata.kind === "Companion"
  );
}

function entityAnchorHeight(entity: NetworkEntitySnapshot): number {
  const worldX = entity.position[0] * WORLD_TO_RENDER_SCALE;
  const worldZ = entity.position[1] * WORLD_TO_RENDER_SCALE;
  const surface = sampleLandscapeSurface(worldX, worldZ);

  if (entityUsesWaterSurface(entity) && surface.isSwimmable) {
    return surface.surfaceHeight;
  }

  return surface.terrainHeight;
}

function entityKind(entity: NetworkEntitySnapshot): NetworkEntityKind {
  return entity.metadata.kind;
}

function fallbackLabel(entity: NetworkEntitySnapshot): string {
  return entity.label?.toLowerCase() ?? "";
}

function teamTint(teamId: number | null, controlled: boolean): RgbaTuple {
  if (controlled) {
    return [0.92, 0.84, 0.58, 1];
  }

  if (teamId == null) {
    return [0.36, 0.66, 0.88, 1];
  }

  const palette: RgbaTuple[] = [
    [0.78, 0.52, 0.34, 1],
    [0.38, 0.72, 0.94, 1],
    [0.54, 0.84, 0.48, 1],
    [0.88, 0.46, 0.46, 1]
  ];
  return palette[teamId % palette.length] ?? palette[0];
}

function addVec3(left: Vec3Tuple, right: Vec3Tuple): Vec3Tuple {
  return [left[0] + right[0], left[1] + right[1], left[2] + right[2]];
}

function finalizeEntityRenderProfile(
  baseProfile: Omit<
    EntityRenderProfile,
    | "selectionRingScale"
    | "selectionRingColor"
    | "criticalRingColor"
    | "auraColor"
    | "impactScale"
  >,
  entity: NetworkEntitySnapshot,
  isControlled: boolean
): EntityRenderProfile {
  const actor = entity.metadata.actorPresentation;
  const combat = entity.metadata.combatPresentation;
  const defaultSelectionRingColor: RgbaTuple = isControlled
    ? [0.62, 0.98, 0.84, 0.34]
    : [0.56, 0.82, 1, 0.16];

  return {
    ...baseProfile,
    mesh: actor?.meshAssetId ?? baseProfile.mesh,
    material:
      actor && actor.materialPaletteId !== "default"
        ? `${baseProfile.material}:${actor.materialPaletteId}`
        : baseProfile.material,
    emissive: addVec3(baseProfile.emissive, combat?.emissiveBoost ?? [0, 0, 0]),
    scale:
      actor != null
        ? [
            baseProfile.scale[0] * actor.scaleMultiplier,
            baseProfile.scale[1] * actor.scaleMultiplier,
            baseProfile.scale[2] * actor.scaleMultiplier
          ]
        : baseProfile.scale,
    groundOffset: baseProfile.groundOffset,
    selectionRingScale: actor?.selectionRingScale ?? 2.4,
    selectionRingColor: combat?.selectionRingColor ?? defaultSelectionRingColor,
    criticalRingColor: combat?.criticalRingColor ?? [0.92, 0.34, 0.3, 0.22],
    auraColor: actor?.auraColor ?? [0, 0, 0, 0],
    impactScale: combat?.impactScale ?? 1
  };
}

function resolveFrameEnvironment(
  snapshot: NetworkWorldSnapshot,
  focusEntity: NetworkEntitySnapshot | null
): ThreeJsEnvironment {
  const defaultValue = defaultEnvironment();
  if (!focusEntity) {
    return defaultValue;
  }

  const focusPosition = focusEntity.position;
  let selected:
    | {
        atmosphere: NetworkAtmosphereProfile;
        volume: NetworkAtmosphereVolume | null;
        distance: number;
      }
    | null = null;

  for (const entity of snapshot.entities) {
    const atmosphere = entity.metadata.atmosphere;
    if (!atmosphere) {
      continue;
    }

    const volume = entity.metadata.atmosphereVolume;
    const distance = Math.hypot(
      entity.position[0] - focusPosition[0],
      entity.position[1] - focusPosition[1]
    );
    if (volume && distance > volume.radius) {
      continue;
    }

    if (!selected) {
      selected = { atmosphere, volume, distance };
      continue;
    }

    const selectedPriority = selected.volume?.priority ?? 0;
    const nextPriority = volume?.priority ?? 0;
    if (nextPriority > selectedPriority) {
      selected = { atmosphere, volume, distance };
      continue;
    }
    if (nextPriority === selectedPriority && distance < selected.distance) {
      selected = { atmosphere, volume, distance };
    }
  }

  return selected?.atmosphere ?? defaultValue;
}

function entityRenderProfile(
  entity: NetworkEntitySnapshot,
  controlledEntity: number | null
): EntityRenderProfile {
  const kind = entityKind(entity);
  const label = fallbackLabel(entity);
  const band = healthBand(entity);
  const isControlled = controlledEntity != null && entity.id === controlledEntity;

  if (kind === "ResourceNode") {
    const isWood = entity.metadata.resourceSkill === "Woodcutting";
    return finalizeEntityRenderProfile({
      mesh: isWood ? "canopy-tree" : "weathered-boulder",
      material: isWood ? "forest-resource" : "ore-vein",
      tint: isWood ? [0.28, 0.62, 0.34, 1] : [0.74, 0.54, 0.28, 1],
      emissive: isWood ? [0.02, 0.05, 0.02] : [0.08, 0.04, 0.01],
      scale: isWood ? [2.0, 4.2, 2.0] : [1.85, 1.3, 1.7],
      groundOffset: isWood ? 0.18 : 0.12,
      layer: 1,
      renderOrder: 1,
      roughness: isWood ? 0.9 : 0.86,
      metallic: isWood ? 0.04 : 0.1
    }, entity, isControlled);
  }

  if (kind === "LootContainer") {
    return finalizeEntityRenderProfile({
      mesh: "supply-crate",
      material: "bronze-cache",
      tint: [0.78, 0.58, 0.34, 1],
      emissive: [0.03, 0.02, 0.01],
      scale: [1.18, 0.82, 0.92],
      groundOffset: 0.06,
      layer: 2,
      renderOrder: 2,
      roughness: 0.8,
      metallic: 0.12
    }, entity, isControlled);
  }

  if (kind === "WildCreature") {
    return finalizeEntityRenderProfile({
      mesh: "rift-beast",
      material: `rift-hide:${band}:${entity.metadata.speciesId ?? "wild"}`,
      tint:
        band === "critical"
          ? [0.86, 0.34, 0.28, 1]
          : band === "wounded"
            ? [0.82, 0.56, 0.34, 1]
            : [0.72, 0.52, 0.4, 1],
      emissive: band === "critical" ? [0.12, 0.03, 0.02] : [0.04, 0.02, 0.01],
      scale: [1.6, 1.9, 1.6],
      groundOffset: 0.08,
      layer: 3,
      renderOrder: 3,
      roughness: 0.82,
      metallic: 0.08
    }, entity, isControlled);
  }

  if (kind === "Companion") {
    return finalizeEntityRenderProfile({
      mesh: "spirit-companion",
      material: `summon-shell:${band}:${entity.metadata.speciesId ?? "companion"}`,
      tint: [0.42, 0.88, 0.74, 1],
      emissive: [0.06, 0.16, 0.12],
      scale: [1.0, 1.35, 1.0],
      groundOffset: 0.1,
      layer: 4,
      renderOrder: 4,
      roughness: 0.42,
      metallic: 0.12
    }, entity, isControlled);
  }

  if (kind === "Npc") {
    return finalizeEntityRenderProfile({
      mesh: "adventurer-avatar",
      material: `npc-cloth:${band}:${entity.metadata.combatStyle ?? "Melee"}`,
      tint:
        band === "critical"
          ? [0.82, 0.38, 0.34, 1]
          : teamTint(entity.metadata.teamId, false),
      emissive: [0.02, 0.03, 0.05],
      scale: [1.1, 1.9, 1.1],
      groundOffset: 0.08,
      layer: 5,
      renderOrder: 5,
      roughness: 0.66,
      metallic: 0.08
    }, entity, isControlled);
  }

  if (label.includes("wall")) {
    return finalizeEntityRenderProfile({
      mesh: "basalt-column",
      material: "obsidian-wall",
      tint: [0.24, 0.3, 0.38, 1],
      emissive: [0.01, 0.02, 0.03],
      scale: [3.2, 2.8, 0.95],
      groundOffset: 0.16,
      layer: 0,
      renderOrder: 0,
      roughness: 0.94,
      metallic: 0.06
    }, entity, isControlled);
  }

  if (label.includes("spire") || label.includes("obelisk") || label.includes("crystal")) {
    return finalizeEntityRenderProfile({
      mesh: "glass-spire",
      material: "glass-shrine",
      tint: [0.54, 0.76, 0.94, 1],
      emissive: [0.08, 0.14, 0.2],
      scale: [1.7, 4.0, 1.7],
      groundOffset: 0.24,
      layer: 1,
      renderOrder: 1,
      roughness: 0.22,
      metallic: 0.08
    }, entity, isControlled);
  }

  if (label.includes("tree") || label.includes("pine") || label.includes("birch")) {
    return finalizeEntityRenderProfile({
      mesh: "canopy-tree",
      material: "forest-canopy",
      tint: [0.34, 0.66, 0.4, 1],
      emissive: [0.02, 0.05, 0.02],
      scale: [2.1, 4.3, 2.1],
      groundOffset: 0.18,
      layer: 1,
      renderOrder: 1,
      roughness: 0.9,
      metallic: 0.04
    }, entity, isControlled);
  }

  if (label.includes("pillar") || label.includes("column")) {
    return finalizeEntityRenderProfile({
      mesh: "basalt-column",
      material: "rift-pillar",
      tint: [0.34, 0.38, 0.46, 1],
      emissive: [0.02, 0.03, 0.05],
      scale: [1.4, 3.6, 1.4],
      groundOffset: 0.16,
      layer: 1,
      renderOrder: 1,
      roughness: 0.9,
      metallic: 0.08
    }, entity, isControlled);
  }

  if (label.includes("obstacle") || label.includes("rock") || label.includes("boulder")) {
    return finalizeEntityRenderProfile({
      mesh: "weathered-boulder",
      material: "arena-stone",
      tint: [0.5, 0.42, 0.32, 1],
      emissive: [0.02, 0.015, 0.01],
      scale: [1.55, 1.12, 1.55],
      groundOffset: 0.08,
      layer: 1,
      renderOrder: 1,
      roughness: 0.96,
      metallic: 0.04
    }, entity, isControlled);
  }

  if (
    kind === "Unknown" &&
    (label.includes("monster") ||
      label.includes("creature") ||
      label.includes("beast") ||
      label.includes("npc"))
  ) {
    return finalizeEntityRenderProfile({
      mesh: label.includes("npc") ? "adventurer-avatar" : "rift-beast",
      material: label.includes("npc") ? `npc-cloth:${band}:legacy` : `rift-hide:${band}:legacy`,
      tint:
        band === "critical"
          ? [0.86, 0.34, 0.28, 1]
          : band === "wounded"
            ? [0.82, 0.56, 0.34, 1]
            : [0.72, 0.52, 0.4, 1],
      emissive: band === "critical" ? [0.12, 0.03, 0.02] : [0.04, 0.02, 0.01],
      scale: label.includes("npc") ? [1.1, 1.9, 1.1] : [1.6, 1.9, 1.6],
      groundOffset: label.includes("npc") ? 0.08 : 0.08,
      layer: label.includes("npc") ? 5 : 3,
      renderOrder: label.includes("npc") ? 5 : 3,
      roughness: 0.82,
      metallic: 0.08
    }, entity, isControlled);
  }

  if (
    kind === "Unknown" &&
    (label.includes("companion") ||
      label.includes("pet") ||
      label.includes("summon") ||
      label.includes("spirit"))
  ) {
    return finalizeEntityRenderProfile({
      mesh: "spirit-companion",
      material: `summon-shell:${band}:legacy`,
      tint: [0.42, 0.88, 0.74, 1],
      emissive: [0.06, 0.16, 0.12],
      scale: [1.0, 1.35, 1.0],
      groundOffset: 0.1,
      layer: 4,
      renderOrder: 4,
      roughness: 0.42,
      metallic: 0.12
    }, entity, isControlled);
  }
  return finalizeEntityRenderProfile({
    mesh: isControlled ? "adventurer-hero" : "adventurer-avatar",
    material: `traveler-cloth:${band}:${isControlled ? "hero" : kind.toLowerCase()}`,
    tint:
      band === "critical"
        ? [0.82, 0.38, 0.34, 1]
        : band === "wounded"
          ? [0.44, 0.76, 0.92, 1]
          : teamTint(entity.metadata.teamId, isControlled),
    emissive: isControlled ? [0.08, 0.06, 0.02] : [0.02, 0.03, 0.05],
    scale: isControlled ? [1.2, 2.0, 1.2] : [1.05, 1.85, 1.05],
    groundOffset: isControlled ? 0.1 : 0.08,
    layer: 6,
    renderOrder: 6,
    roughness: 0.64,
    metallic: 0.08
  }, entity, isControlled);
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
  const environment = resolveFrameEnvironment(snapshot, focus);
  const focusPosition = focus
    ? [focus.position[0] * WORLD_TO_RENDER_SCALE, focus.position[1] * WORLD_TO_RENDER_SCALE]
    : [0, 0];
  const focusSpeed = focus ? Math.hypot(focus.velocity[0], focus.velocity[1]) : 0;
  const focusSpeedFactor = clamp(focusSpeed / 5.2, 0, 1);
  const focusChunkKey = focus?.metadata.chunkKey ?? null;
  const focusRegionId = focus?.metadata.regionId ?? null;
  const cameraDistances = new Array<number>();

  for (const entity of snapshot.entities) {
    const dx = (entity.position[0] * WORLD_TO_RENDER_SCALE) - focusPosition[0];
    const dz = (entity.position[1] * WORLD_TO_RENDER_SCALE) - focusPosition[1];
    const distance = Math.hypot(dx, dz);
    const sameChunk = focusChunkKey != null && entity.metadata.chunkKey === focusChunkKey;
    const sameRegion = focusRegionId != null && entity.metadata.regionId === focusRegionId;
    const isImmediateInterest =
      entity.id === focus?.id ||
      distance <= 16 ||
      sameChunk ||
      (sameRegion && entity.metadata.kind !== "Scenery" && distance <= 22);

    if (isImmediateInterest) {
      cameraDistances.push(distance);
    }
  }

  cameraDistances.sort((left, right) => left - right);
  const cameraReferenceDistance =
    cameraDistances.length === 0
      ? 8
      : cameraDistances[
          Math.min(cameraDistances.length - 1, Math.floor(cameraDistances.length * 0.75))
        ] ?? 8;
  const cameraZoom = clamp(
    1.22 - cameraReferenceDistance * 0.016 + focusSpeedFactor * 0.04,
    0.98,
    1.2
  );

  const meshBatches = new Map<string, ThreeJsMeshBatch>();
  const spriteBatches = new Array<ThreeJsSpriteBatch>();

  for (const entity of snapshot.entities) {
    const profile = entityRenderProfile(entity, controlledEntity);
    const worldX = entity.position[0] * WORLD_TO_RENDER_SCALE;
    const worldZ = entity.position[1] * WORLD_TO_RENDER_SCALE;
    const groundHeight = entityAnchorHeight(entity);
    const anchorHeight = meshGroundAnchorHeight(profile.mesh, profile.scale[1]);
    const position: Vec3Tuple = [
      worldX,
      groundHeight + anchorHeight + profile.groundOffset,
      worldZ
    ];
    const instance: ThreeJsInstance = {
      position,
      rotation: yawQuaternion(entity.rotation),
      scale: profile.scale,
      sourceEntity: entity.id,
      animationSetId: resolveAnimationSetId(entity),
      motionSpeed: motionSpeed(entity),
      healthRatio: healthRatio(entity),
      controlled: controlledEntity != null && entity.id === controlledEntity
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
    if (profile.auraColor[3] > 0.001) {
      spriteBatches.push({
        texture: "mist-ring",
        frame: 0,
        layer: profile.layer + 1,
        billboard: false,
        phase: "transparent",
        sortDepth: position[2],
        renderOrder: profile.renderOrder + 10,
        transparent: true,
        depthWrite: false,
        depthTest: true,
        instances: [
          {
            position: [position[0], groundHeight + 0.12, position[2]],
            rotation: GROUND_RING_ROTATION,
            scale: [
              profile.selectionRingScale * 1.4,
              profile.selectionRingScale * 1.4,
              1
            ],
            color: profile.auraColor,
            sourceEntity: entity.id,
            animationSetId: "aura-ring"
          }
        ]
      });
    }
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
            position: [position[0], groundHeight + 0.08, position[2]],
            rotation: GROUND_RING_ROTATION,
            scale: [profile.selectionRingScale, profile.selectionRingScale, 1],
            color: profile.selectionRingColor,
            sourceEntity: entity.id,
            animationSetId: "selection-ring",
            controlled: true
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
            position: [position[0], groundHeight + 0.06, position[2]],
            rotation: GROUND_RING_ROTATION,
            scale: [
              profile.selectionRingScale * profile.impactScale,
              profile.selectionRingScale * profile.impactScale,
              1
            ],
            color: profile.criticalRingColor,
            sourceEntity: entity.id,
            animationSetId: "critical-ring",
            healthRatio: healthRatio(entity)
          }
        ]
      });
    }
  }

  return {
    camera: {
      x: focusPosition[0],
      y: focusPosition[1],
      zoom: cameraZoom,
      rotation: focus?.rotation ?? 0.48,
      fov: 52 + focusSpeedFactor * 4,
      pitch: 0.34 - focusSpeedFactor * 0.02,
      focusHeight: 2.2 + focusSpeedFactor * 0.18,
      followDistance: 13.5 + focusSpeedFactor * 1.4,
      shoulderOffset: 0.9,
      leadX: focus ? focus.velocity[0] * (0.34 + focusSpeedFactor * 0.12) : 0,
      leadY: focus ? focus.velocity[1] * (0.34 + focusSpeedFactor * 0.12) : 0,
      viewportWidth: options.viewportWidth ?? defaultViewportWidth(),
      viewportHeight: options.viewportHeight ?? defaultViewportHeight()
    },
    backgroundColor: options.backgroundColor ?? environment.skyColor,
    environment,
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

export interface InteractionMarkerOptions {
  moveTarget?: Vec2Tuple | null;
  selectedTarget?: NetworkEntitySnapshot | null;
  controlledEntity?: number | null;
  controlledSnapshot?: NetworkEntitySnapshot | null;
}

export interface WorldEventMarkerOptions {
  events: NetworkGameEvent[];
  worldSnapshot?: NetworkWorldSnapshot | null;
  currentTick?: number | null;
}

export function withInteractionMarkers(
  frame: ThreeJsWebGpuFrame,
  options: InteractionMarkerOptions
): ThreeJsWebGpuFrame {
  const markers = new Array<ThreeJsSpriteBatch>();
  const controlledEntity = options.controlledEntity ?? null;

  if (options.moveTarget) {
    const worldX = options.moveTarget[0] * WORLD_TO_RENDER_SCALE;
    const worldZ = options.moveTarget[1] * WORLD_TO_RENDER_SCALE;
    markers.push({
      texture: "selection-ring",
      frame: 0,
      layer: 7,
      billboard: false,
      phase: "transparent",
      sortDepth: worldZ,
      renderOrder: 18,
      transparent: true,
      depthWrite: false,
      depthTest: true,
      instances: [
        {
          position: [worldX, sampleSurfaceHeight(worldX, worldZ) + 0.05, worldZ],
          rotation: GROUND_RING_ROTATION,
          scale: [1.65, 1.65, 1],
          color: [0.86, 0.96, 1, 0.24],
          animationSetId: "destination-ring"
        }
      ]
    });

    const moveOrigin = options.controlledSnapshot?.position ?? null;
    if (moveOrigin) {
      const deltaX = options.moveTarget[0] - moveOrigin[0];
      const deltaY = options.moveTarget[1] - moveOrigin[1];
      const distance = Math.hypot(deltaX, deltaY);
      const breadcrumbCount = Math.min(4, Math.max(0, Math.floor(distance / 3.4)));

      if (breadcrumbCount > 0) {
        markers.push({
          texture: "mist-ring",
          frame: 0,
          layer: 6,
          billboard: false,
          phase: "transparent",
          sortDepth: worldZ,
          renderOrder: 17,
          transparent: true,
          depthWrite: false,
          depthTest: true,
          instances: Array.from({ length: breadcrumbCount }, (_, index) => {
            const t = (index + 1) / (breadcrumbCount + 1);
            const pathX = (moveOrigin[0] + deltaX * t) * WORLD_TO_RENDER_SCALE;
            const pathZ = (moveOrigin[1] + deltaY * t) * WORLD_TO_RENDER_SCALE;
            const scale = 0.74 + index * 0.08;
            return {
              position: [pathX, sampleSurfaceHeight(pathX, pathZ) + 0.04, pathZ],
              rotation: GROUND_RING_ROTATION,
              scale: [scale, scale, 1],
              color: [0.72, 0.92, 1, 0.12 + index * 0.03],
              animationSetId: "path-node"
            };
          })
        });
      }
    }
  }

  if (options.selectedTarget && options.selectedTarget.id !== controlledEntity) {
    const target = options.selectedTarget;
    const worldX = target.position[0] * WORLD_TO_RENDER_SCALE;
    const worldZ = target.position[1] * WORLD_TO_RENDER_SCALE;
    const interaction = target.metadata.interaction;
    const scale = target.metadata.actorPresentation?.selectionRingScale ?? 2.4;
    const useDangerRing = interaction.canAttack;
    const color: RgbaTuple = interaction.canCapture
      ? [0.46, 0.96, 0.72, 0.34]
      : interaction.canGather
        ? [0.94, 0.78, 0.36, 0.32]
        : interaction.canAttack
          ? [1, 0.38, 0.32, 0.34]
          : interaction.canInteract
            ? [0.54, 0.88, 1, 0.28]
            : [0.76, 0.86, 1, 0.24];
    markers.push({
      texture: useDangerRing ? "danger-ring" : "selection-ring",
      frame: 0,
      layer: 8,
      billboard: false,
      phase: "transparent",
      sortDepth: worldZ,
      renderOrder: 22,
      transparent: true,
      depthWrite: false,
      depthTest: true,
      instances: [
        {
          position: [worldX, sampleSurfaceHeight(worldX, worldZ) + 0.07, worldZ],
          rotation: GROUND_RING_ROTATION,
          scale: [scale * 1.08, scale * 1.08, 1],
          color,
          sourceEntity: target.id,
          animationSetId: "target-ring",
          healthRatio: healthRatio(target)
        }
      ]
    });

    const moveOrigin = options.controlledSnapshot?.position ?? null;
    if (moveOrigin && (interaction.canAttack || interaction.canCapture)) {
      const deltaX = target.position[0] - moveOrigin[0];
      const deltaY = target.position[1] - moveOrigin[1];
      const distance = Math.hypot(deltaX, deltaY);
      const tetherCount = Math.min(5, Math.max(2, Math.floor(distance / 3.6)));

      markers.push({
        texture: useDangerRing ? "danger-ring" : "selection-ring",
        frame: 0,
        layer: 7,
        billboard: false,
        phase: "transparent",
        sortDepth: worldZ,
        renderOrder: 21,
        transparent: true,
        depthWrite: false,
        depthTest: true,
        instances: Array.from({ length: tetherCount }, (_, index) => {
          const t = (index + 1) / (tetherCount + 1);
          const pathX = (moveOrigin[0] + deltaX * t) * WORLD_TO_RENDER_SCALE;
          const pathZ = (moveOrigin[1] + deltaY * t) * WORLD_TO_RENDER_SCALE;
          const scale = interaction.canAttack ? 0.72 + index * 0.08 : 0.64 + index * 0.06;

          return {
            position: [pathX, sampleSurfaceHeight(pathX, pathZ) + 0.05, pathZ],
            rotation: GROUND_RING_ROTATION,
            scale: [scale, scale, 1],
            color: interaction.canAttack
              ? [1, 0.38, 0.32, 0.12 + index * 0.03]
              : [0.48, 0.96, 0.78, 0.1 + index * 0.025],
            sourceEntity: target.id,
            animationSetId: "target-ring"
          };
        })
      });
    }
  }

  if (markers.length === 0) {
    return frame;
  }

  return {
    ...frame,
    spriteBatches: [...frame.spriteBatches, ...markers]
  };
}

export function withWorldEventMarkers(
  frame: ThreeJsWebGpuFrame,
  options: WorldEventMarkerOptions
): ThreeJsWebGpuFrame {
  if (options.events.length === 0) {
    return frame;
  }

  const currentTick = options.currentTick ?? options.worldSnapshot?.tick ?? null;
  const markers = new Array<ThreeJsSpriteBatch>();

  for (const event of options.events) {
    if (currentTick != null && currentTick - event.tick > 12) {
      continue;
    }

    const position = resolveEventMarkerPosition(event, options.worldSnapshot);
    if (!position) {
      continue;
    }

    const ageTicks = currentTick == null ? 0 : Math.max(0, currentTick - event.tick);
    const ageFade = clamp01(1 - ageTicks / 12);
    const markerStyle = eventMarkerStyle(event.kind, ageFade);
    markers.push({
      texture: markerStyle.texture,
      frame: 0,
      layer: markerStyle.layer,
      billboard: false,
      phase: "transparent",
      sortDepth: position[1],
      renderOrder: markerStyle.renderOrder,
      transparent: true,
      depthWrite: false,
      depthTest: true,
      instances: [
        {
          position: [
            position[0],
            sampleSurfaceHeight(position[0], position[1]) + markerStyle.height,
            position[1]
          ],
          rotation: GROUND_RING_ROTATION,
          scale: [markerStyle.scale, markerStyle.scale, 1],
          color: markerStyle.color,
          animationSetId: markerStyle.animationSetId
        }
      ]
    });
  }

  if (markers.length === 0) {
    return frame;
  }

  return {
    ...frame,
    spriteBatches: [...frame.spriteBatches, ...markers]
  };
}

export function legacyFrameToThreeJsFrame(frame: RenderFrame): ThreeJsWebGpuFrame {
  return {
    camera: frame.camera,
    backgroundColor: frame.backgroundColor,
    environment: defaultEnvironment(),
    overlayCommands: frame.commands.filter((command) => command.visible),
    meshBatches: [],
    spriteBatches: [],
    hints: defaultFrameHints()
  };
}

function resolveEventMarkerPosition(
  event: NetworkGameEvent,
  worldSnapshot: NetworkWorldSnapshot | null | undefined
): Vec2Tuple | null {
  if (event.origin) {
    return [event.origin[0] * WORLD_TO_RENDER_SCALE, event.origin[1] * WORLD_TO_RENDER_SCALE];
  }

  const entityPosition =
    worldSnapshot?.entities.find((entity) => event.entityIds.includes(entity.id))?.position ?? null;
  if (!entityPosition) {
    return null;
  }

  return [
    entityPosition[0] * WORLD_TO_RENDER_SCALE,
    entityPosition[1] * WORLD_TO_RENDER_SCALE
  ];
}

function eventMarkerStyle(kind: string, ageFade: number): {
  texture: ThreeJsSpriteBatch["texture"];
  animationSetId: string;
  renderOrder: number;
  layer: number;
  scale: number;
  height: number;
  color: RgbaTuple;
} {
  const normalized = kind.toLowerCase();

  if (
    normalized.includes("damage") ||
    normalized.includes("attack") ||
    normalized.includes("hit") ||
    normalized.includes("defeat")
  ) {
    return {
      texture: "danger-ring",
      animationSetId: "critical-ring",
      renderOrder: 23,
      layer: 9,
      scale: 1.55 + ageFade * 0.32,
      height: 0.09,
      color: [1, 0.42, 0.34, 0.18 + ageFade * 0.28]
    };
  }

  if (
    normalized.includes("capture") ||
    normalized.includes("summon") ||
    normalized.includes("command")
  ) {
    return {
      texture: "selection-ring",
      animationSetId: "aura-ring",
      renderOrder: 22,
      layer: 9,
      scale: 1.4 + ageFade * 0.26,
      height: 0.08,
      color: [0.48, 0.96, 0.78, 0.16 + ageFade * 0.24]
    };
  }

  if (normalized.includes("loot") || normalized.includes("gather")) {
    return {
      texture: "selection-ring",
      animationSetId: "destination-ring",
      renderOrder: 21,
      layer: 8,
      scale: 1.22 + ageFade * 0.22,
      height: 0.07,
      color: [0.98, 0.84, 0.36, 0.14 + ageFade * 0.2]
    };
  }

  return {
    texture: "mist-ring",
    animationSetId: "path-node",
    renderOrder: 20,
    layer: 8,
    scale: 0.96 + ageFade * 0.18,
    height: 0.06,
    color: [0.7, 0.9, 1, 0.12 + ageFade * 0.16]
  };
}

function clamp01(value: number): number {
  return Math.min(Math.max(value, 0), 1);
}
