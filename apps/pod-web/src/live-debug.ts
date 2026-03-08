import type {
  AgentTickRollupDocument,
  AgentToolCallEventDocument,
  LiveDebugDocument,
  TickRollupSummary,
  ToolCallEventSummary
} from "./contracts";
import {
  summarizeAgentTickRollup,
  summarizeAgentToolCallEvent
} from "./contracts";

export interface LiveDebugState {
  latestToolEventSummary: ToolCallEventSummary | null;
  latestRollupSummary: TickRollupSummary | null;
  liveReplayDocuments: number;
  liveIncidentDocuments: number;
  toolEventsByEntity: Map<number, ToolCallEventSummary>;
  rollupsByEntity: Map<number, TickRollupSummary>;
}

export function createLiveDebugState(): LiveDebugState {
  return {
    latestToolEventSummary: null,
    latestRollupSummary: null,
    liveReplayDocuments: 0,
    liveIncidentDocuments: 0,
    toolEventsByEntity: new Map(),
    rollupsByEntity: new Map()
  };
}

export function resetLiveDebugState(state: LiveDebugState): void {
  state.latestToolEventSummary = null;
  state.latestRollupSummary = null;
  state.liveReplayDocuments = 0;
  state.liveIncidentDocuments = 0;
  state.toolEventsByEntity.clear();
  state.rollupsByEntity.clear();
}

export function recordLiveDebugDocument(
  state: LiveDebugState,
  document: LiveDebugDocument
): void {
  switch (document.kind) {
    case "toolCallEvent":
      state.latestToolEventSummary = summarizeAgentToolCallEvent(
        document.payload as AgentToolCallEventDocument
      );
      state.toolEventsByEntity.set(
        state.latestToolEventSummary.agentEntityId,
        state.latestToolEventSummary
      );
      break;
    case "tickRollup":
      state.latestRollupSummary = summarizeAgentTickRollup(
        document.payload as AgentTickRollupDocument
      );
      state.rollupsByEntity.set(
        state.latestRollupSummary.agentEntityId,
        state.latestRollupSummary
      );
      break;
    case "replay":
      state.liveReplayDocuments += 1;
      break;
    case "incident":
      state.liveIncidentDocuments += 1;
      break;
    case "tickTelemetry":
      break;
  }
}

export function selectedToolEventSummary(
  state: LiveDebugState,
  entityId: number | null
): ToolCallEventSummary | null {
  if (entityId == null) {
    return state.latestToolEventSummary;
  }
  return state.toolEventsByEntity.get(entityId) ?? state.latestToolEventSummary;
}

export function selectedTickRollupSummary(
  state: LiveDebugState,
  entityId: number | null
): TickRollupSummary | null {
  if (entityId == null) {
    return state.latestRollupSummary;
  }
  return state.rollupsByEntity.get(entityId) ?? state.latestRollupSummary;
}
