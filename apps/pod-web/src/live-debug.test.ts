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
  selectedFocusedDebugSummary,
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

function sampleFocusedSummary(agentEntityId: number) {
  return {
    shard_id: "direct-connect",
    entity_id: agentEntityId,
    latest_tick: 60,
    tool_call_count: 2,
    tool_error_count: 1,
    rejected_action_count: 1,
    total_distance: 22.75,
    average_tool_latency_ms: 41,
    visible_entity_count: 12,
    audible_event_count: 3,
    message_count: 2,
    latest_tool_name: "llm.complete",
    latest_tool_status: "Succeeded",
    latest_tool_error: null,
    notes: ["1 rejected action retained"]
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

  test("retains focused entity summaries alongside tool and rollup docs", () => {
    const state = createLiveDebugState();
    recordLiveDebugDocument(state, {
      kind: "focusedSummary",
      documentType: "focused_entity_debug_summary",
      payload: sampleFocusedSummary(1002)
    } satisfies LiveDebugDocument);

    expect(selectedFocusedDebugSummary(state, 1002)?.entity_id).toBe(1002);
    expect(selectedFocusedDebugSummary(state, null)?.total_distance).toBe(22.75);
    expect(selectedFocusedDebugSummary(state, 9999)?.entity_id).toBe(1002);
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
    recordLiveDebugDocument(state, {
      kind: "transport",
      documentType: "shard_transport_summary",
      payload: {
        shard_id: "direct-connect",
        latest_tick: 22,
        client_count: 1,
        resumed_sessions: 0,
        client_inactivity_timeout_ticks: 600,
        queue_pressure_warn_depth: 192,
        total_pending_action_queue_depth: 0,
        queue_pressure_client_count: 0,
        total_inbound_messages: 2,
        total_outbound_messages: 4,
        total_inbound_bytes: 64,
        total_outbound_bytes: 256,
        action_batches_received: 1,
        full_snapshot_requests: 0,
        ping_requests: 1,
        state_deltas_sent: 2,
        event_batches_sent: 1,
        debug_documents_sent: 3,
        rejected_messages_sent: 0,
        timed_out_clients: 0,
        queue_pressure_events: 0,
        clients: []
      }
    } as LiveDebugDocument);

    expect(state.liveReplayDocuments).toBe(1);
    expect(state.liveIncidentDocuments).toBe(1);
    expect(state.liveTransportDocuments).toBe(1);
    expect(state.latestTransportSummary?.client_count).toBe(1);

    resetLiveDebugState(state);

    expect(state.liveReplayDocuments).toBe(0);
    expect(state.liveIncidentDocuments).toBe(0);
    expect(state.liveTransportDocuments).toBe(0);
    expect(state.latestTransportSummary).toBeNull();
    expect(selectedToolEventSummary(state, null)).toBeNull();
    expect(selectedTickRollupSummary(state, null)).toBeNull();
    expect(selectedFocusedDebugSummary(state, null)).toBeNull();
  });
});
