import { describe, expect, test } from "bun:test";

import type {
  AgentTickRollupDocument,
  AgentToolCallEventDocument,
  LiveDebugDocument
} from "./contracts";
import {
  createLiveDebugState,
  recordLiveDebugDocument,
  resetLiveDebugState,
  selectedTickRollupSummary,
  selectedToolEventSummary
} from "./live-debug";

function sampleTool(agentEntityId: number, status = "TimedOut"): AgentToolCallEventDocument {
  return {
    tick: 18,
    agent_entity_id: agentEntityId,
    trace: {
      tick: 18,
      tool_name: "llm.complete",
      provider: "qwen",
      status,
      latency_ms: 48,
      request_units: 120,
      response_units: 64,
      error_message: status === "Succeeded" ? null : "timeout"
    }
  };
}

function sampleRollup(agentEntityId: number, tickEnd = 60): AgentTickRollupDocument {
  return {
    agent_entity_id: agentEntityId,
    tick_start: 1,
    tick_end: tickEnd,
    total_distance: 18.5,
    submitted_action_count: 3,
    executed_action_count: 2,
    rejected_action_count: 1,
    tool_call_count: 1,
    tool_error_count: 1,
    average_tool_latency_ms: 48,
    visible_entity_count: 12,
    audible_event_count: 3,
    message_count: 2
  };
}

describe("live debug state", () => {
  test("retains entity-scoped summaries and falls back to latest when unfocused", () => {
    const state = createLiveDebugState();
    recordLiveDebugDocument(state, {
      kind: "toolCallEvent",
      documentType: "agent_tool_call_event",
      payload: sampleTool(1001)
    } satisfies LiveDebugDocument);
    recordLiveDebugDocument(state, {
      kind: "tickRollup",
      documentType: "agent_tick_rollup",
      payload: sampleRollup(1002)
    } satisfies LiveDebugDocument);

    expect(selectedToolEventSummary(state, 1001)?.agentEntityId).toBe(1001);
    expect(selectedTickRollupSummary(state, 1002)?.agentEntityId).toBe(1002);
    expect(selectedToolEventSummary(state, null)?.agentEntityId).toBe(1001);
    expect(selectedTickRollupSummary(state, null)?.agentEntityId).toBe(1002);
    expect(selectedToolEventSummary(state, 9999)?.agentEntityId).toBe(1001);
  });

  test("tracks replay and incident stream counts and resets cleanly", () => {
    const state = createLiveDebugState();
    recordLiveDebugDocument(state, {
      kind: "replay",
      documentType: "replay_file",
      payload: {
        header: {
          name: "flagship",
          timestamp: 1_741_315_200,
          world_seed: 42,
          tick_count: 22,
          agent_count: 2,
          notes: "debug"
        },
        traces: [],
        telemetry_windows: [],
        training_samples: []
      }
    } as LiveDebugDocument);
    recordLiveDebugDocument(state, {
      kind: "incident",
      documentType: "shard_incident_summary",
      payload: {
        shard_id: "alpha-1",
        latest_tick: 22,
        severity: "Warning",
        summary: "tool-call rate high",
        tick_budget_overrun_rate: 0.08,
        action_rejection_rate: 0.02,
        tool_call_error_rate: 0.11,
        average_tool_latency_ms: 820,
        average_trajectory_distance: 3.2,
        peak_entity_count: 512,
        peak_agent_count: 128,
        capture_actions: 4,
        summon_actions: 2,
        gather_actions: 7,
        loot_actions: 9,
        notes: []
      }
    } as LiveDebugDocument);

    expect(state.liveReplayDocuments).toBe(1);
    expect(state.liveIncidentDocuments).toBe(1);

    resetLiveDebugState(state);

    expect(state.liveReplayDocuments).toBe(0);
    expect(state.liveIncidentDocuments).toBe(0);
    expect(selectedToolEventSummary(state, null)).toBeNull();
    expect(selectedTickRollupSummary(state, null)).toBeNull();
  });
});
