import type {
  CatchUpDiagnostics,
  TelemetryAgentFrame,
  TelemetryTrajectorySample,
  TickTelemetryEnvelope
} from "./contracts";

export interface PodTelemetryStats {
  enabled: boolean;
  retainedTicks: number;
  tick: number | null;
  selectedAgentId: string | null;
  selectedEntityId: number | null;
  selectedLabel: string;
  trajectorySamples: number;
  trajectoryDistance: number;
  submittedActions: number;
  executedActions: number;
  rejectedActions: number;
  toolCalls: number;
  toolErrors: number;
  toolErrorRate: number;
  lastToolStatus: string | null;
  lastToolLatencyMs: number | null;
  lastToolError: string | null;
  visibleEntities: number;
  audibleEvents: number;
  messages: number;
  recoverySummary: string;
  recoveryAttempts: number;
  nextRetryTick: number | null;
}

export interface PodTelemetryOverlayState {
  enabled: boolean;
  maxSamples: number;
  revision: number;
  history: TickTelemetryEnvelope[];
  selectedAgentId: string | null;
  selectedEntityId: number | null;
}

interface TelemetryTarget {
  agentId: string;
  entityId: number | null;
  label: string;
}

export function createTelemetryOverlayState(
  maxSamples: number
): PodTelemetryOverlayState {
  return {
    enabled: false,
    maxSamples: Math.max(maxSamples, 1),
    revision: 0,
    history: [],
    selectedAgentId: null,
    selectedEntityId: null
  };
}

export function applyTickTelemetry(
  state: PodTelemetryOverlayState,
  frame: TickTelemetryEnvelope
): void {
  state.history.push(frame);
  if (state.history.length > state.maxSamples) {
    state.history.splice(0, state.history.length - state.maxSamples);
  }
  syncTelemetrySelection(state);
  state.revision += 1;
}

export function resetTelemetry(state: PodTelemetryOverlayState): void {
  state.history = [];
  state.selectedAgentId = null;
  state.selectedEntityId = null;
  state.revision += 1;
}

export function setTelemetryEnabled(
  state: PodTelemetryOverlayState,
  enabled: boolean
): void {
  if (state.enabled === enabled) {
    return;
  }
  state.enabled = enabled;
  state.revision += 1;
}

export function cycleTelemetrySelection(
  state: PodTelemetryOverlayState,
  direction: -1 | 1
): void {
  const targets = telemetryTargets(state);
  if (targets.length === 0) {
    state.selectedAgentId = null;
    state.selectedEntityId = null;
    return;
  }

  const currentIndex = targets.findIndex(
    (target) =>
      target.agentId === state.selectedAgentId &&
      target.entityId === state.selectedEntityId
  );
  const baseIndex = currentIndex === -1 ? 0 : currentIndex;
  const nextIndex =
    (baseIndex + direction + targets.length) % targets.length;
  const next = targets[nextIndex];
  if (!next) {
    return;
  }

  state.selectedAgentId = next.agentId;
  state.selectedEntityId = next.entityId;
  state.revision += 1;
}

export function selectedTrajectorySamples(
  state: PodTelemetryOverlayState
): TelemetryTrajectorySample[] {
  const selected = selectedAgentHistory(state);
  const samples: TelemetryTrajectorySample[] = [];

  for (const agent of selected) {
    const trajectory = agent.trajectory;
    if (!trajectory) {
      continue;
    }
    if (samples.length === 0) {
      samples.push(trajectory.start);
    }
    samples.push(trajectory.end);
  }

  if (samples.length <= state.maxSamples) {
    return samples;
  }

  return samples.slice(samples.length - state.maxSamples);
}

export function telemetryStats(
  state: PodTelemetryOverlayState
): PodTelemetryStats {
  const latest = latestEnvelope(state);
  const selected = selectedAgentFrame(state);
  const trajectory = selectedTrajectorySamples(state);
  const submittedActions =
    selected?.action_trace.filter((trace) => trace.stage === "Submitted").length ??
    0;
  const executedActions =
    selected?.action_trace.filter((trace) => trace.stage === "Executed").length ??
    0;
  const rejectedActions =
    selected?.action_trace.filter((trace) => trace.stage === "Rejected").length ??
    0;
  const toolCalls = selected?.tool_calls.length ?? 0;
  const toolErrors =
    selected?.tool_calls.filter(
      (trace) => trace.status !== "Succeeded" && trace.status !== "Requested"
    ).length ?? 0;
  const lastTool = selected?.tool_calls.at(-1) ?? null;
  const recovery = latest?.recovery ?? null;

  return {
    enabled: state.enabled,
    retainedTicks: state.history.length,
    tick: latest?.tickTelemetry.tick ?? null,
    selectedAgentId: state.selectedAgentId,
    selectedEntityId: state.selectedEntityId,
    selectedLabel: selectedTelemetryLabel(state),
    trajectorySamples: trajectory.length,
    trajectoryDistance: Number(
      selectedAgentHistory(state)
        .reduce(
          (distance, frame) =>
            distance + (frame.trajectory?.distance_travelled ?? 0),
          0
        )
        .toFixed(2)
    ),
    submittedActions,
    executedActions,
    rejectedActions,
    toolCalls,
    toolErrors,
    toolErrorRate: toolCalls === 0 ? 0 : Number((toolErrors / toolCalls).toFixed(3)),
    lastToolStatus: lastTool?.status ?? null,
    lastToolLatencyMs: lastTool?.latency_ms ?? null,
    lastToolError: lastTool?.error_message ?? null,
    visibleEntities: selected?.visible_entity_count ?? 0,
    audibleEvents: selected?.audible_event_count ?? 0,
    messages: selected?.message_count ?? 0,
    recoverySummary: formatRecoverySummary(recovery),
    recoveryAttempts: recovery?.recovery.request_attempts ?? 0,
    nextRetryTick: recovery?.recovery.next_retry_tick ?? null
  };
}

function selectedAgentHistory(
  state: PodTelemetryOverlayState
): TelemetryAgentFrame[] {
  const agentId = state.selectedAgentId;
  if (!agentId) {
    return [];
  }

  return state.history
    .map((entry) =>
      entry.tickTelemetry.agents.find((agent) => agent.agent_id === agentId)
    )
    .filter((agent): agent is TelemetryAgentFrame => Boolean(agent));
}

function selectedAgentFrame(
  state: PodTelemetryOverlayState
): TelemetryAgentFrame | null {
  return selectedAgentHistory(state).at(-1) ?? null;
}

function latestEnvelope(
  state: PodTelemetryOverlayState
): TickTelemetryEnvelope | null {
  return state.history.at(-1) ?? null;
}

function telemetryTargets(state: PodTelemetryOverlayState): TelemetryTarget[] {
  const latest = latestEnvelope(state);
  if (!latest) {
    return [];
  }

  return latest.tickTelemetry.agents.map((agent) => ({
    agentId: agent.agent_id,
    entityId: agent.entity_id ?? null,
    label: `${agent.runtime_profile.role} · ${
      agent.entity_id == null ? agent.agent_id.slice(0, 8) : `E(${agent.entity_id})`
    }`
  }));
}

function syncTelemetrySelection(state: PodTelemetryOverlayState): void {
  const targets = telemetryTargets(state);
  if (targets.length === 0) {
    state.selectedAgentId = null;
    state.selectedEntityId = null;
    return;
  }

  const current = targets.find(
    (target) =>
      target.agentId === state.selectedAgentId &&
      target.entityId === state.selectedEntityId
  );
  if (current) {
    return;
  }

  const next = targets[0];
  if (!next) {
    return;
  }
  state.selectedAgentId = next.agentId;
  state.selectedEntityId = next.entityId;
}

function selectedTelemetryLabel(state: PodTelemetryOverlayState): string {
  const selected = telemetryTargets(state).find(
    (target) =>
      target.agentId === state.selectedAgentId &&
      target.entityId === state.selectedEntityId
  );
  return selected?.label ?? "No agent selected";
}

function formatRecoverySummary(recovery: CatchUpDiagnostics | null): string {
  if (!recovery) {
    return "No recovery telemetry";
  }

  if (recovery.recovery.awaiting_full_snapshot) {
    return `Awaiting full snapshot · ${recovery.recovery.request_attempts} request(s)`;
  }

  if (recovery.presentation_drift_ticks != null) {
    return `Presentation drift ${recovery.presentation_drift_ticks.toFixed(2)} ticks`;
  }

  return "Authority aligned";
}
