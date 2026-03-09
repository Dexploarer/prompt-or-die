import type {
  AgentTickRollupDocument,
  AgentToolCallEventDocument,
  FocusedEntityDebugSummaryDocument,
  LiveDebugDocument,
  ShardTransportSummaryDocument,
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
  latestFocusedSummary: FocusedEntityDebugSummaryDocument | null;
  latestTransportSummary: ShardTransportSummaryDocument | null;
  liveReplayDocuments: number;
  liveIncidentDocuments: number;
  liveTransportDocuments: number;
  toolEventsByEntity: Map<number, ToolCallEventSummary>;
  rollupsByEntity: Map<number, TickRollupSummary>;
  focusedSummariesByEntity: Map<number, FocusedEntityDebugSummaryDocument>;
}

export function createLiveDebugState(): LiveDebugState {
  return {
    latestToolEventSummary: null,
    latestRollupSummary: null,
    latestFocusedSummary: null,
    latestTransportSummary: null,
    liveReplayDocuments: 0,
    liveIncidentDocuments: 0,
    liveTransportDocuments: 0,
    toolEventsByEntity: new Map(),
    rollupsByEntity: new Map(),
    focusedSummariesByEntity: new Map()
  };
}

export function resetLiveDebugState(state: LiveDebugState): void {
  state.latestToolEventSummary = null;
  state.latestRollupSummary = null;
  state.latestFocusedSummary = null;
  state.latestTransportSummary = null;
  state.liveReplayDocuments = 0;
  state.liveIncidentDocuments = 0;
  state.liveTransportDocuments = 0;
  state.toolEventsByEntity.clear();
  state.rollupsByEntity.clear();
  state.focusedSummariesByEntity.clear();
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
    case "focusedSummary":
      state.latestFocusedSummary = document.payload as FocusedEntityDebugSummaryDocument;
      state.focusedSummariesByEntity.set(
        state.latestFocusedSummary.entity_id,
        state.latestFocusedSummary
      );
      break;
    case "transport":
      state.latestTransportSummary = document.payload as ShardTransportSummaryDocument;
      state.liveTransportDocuments += 1;
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

export function selectedFocusedDebugSummary(
  state: LiveDebugState,
  entityId: number | null
): FocusedEntityDebugSummaryDocument | null {
  if (entityId == null) {
    return state.latestFocusedSummary;
  }
  return (
    state.focusedSummariesByEntity.get(entityId) ?? state.latestFocusedSummary
  );
}
