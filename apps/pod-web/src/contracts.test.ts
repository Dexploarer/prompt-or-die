import { describe, expect, test } from "bun:test";
import { encode } from "@toon-format/toon";

import {
  parseAgentTickRollup,
  parseAgentToolCallEvent,
  parseLiveDebugDocument,
  parseReplayFile,
  parseShardIncidentSummary,
  parseTickTelemetryEnvelope,
  summarizeAgentTickRollup,
  summarizeAgentToolCallEvent,
  summarizeReplayFile
} from "./contracts";

describe("TOON contract parsing", () => {
  test("accepts versioned tick telemetry TOON documents", () => {
    const document = encode({
      document_type: "versioned_tick_telemetry",
      payload: {
        version: "V1",
        payload: {
          tick: 9,
          agents: []
        }
      }
    });

    const envelope = parseTickTelemetryEnvelope(document);
    expect(envelope.tickTelemetry.tick).toBe(9);
    expect(envelope.tickTelemetry.agents).toEqual([]);
  });

  test("accepts replay TOON documents and builds summaries", () => {
    const document = encode({
      document_type: "replay_file",
      payload: {
        header: {
          name: "flagship-mmo-loop",
          timestamp: 1_741_315_200,
          world_seed: 42,
          tick_count: 120,
          agent_count: 2,
          notes: "acceptance"
        },
        traces: [
          [
            {
              tick: 12,
              agent_id: "agent-a",
              observation_hash: 44,
              prompt_sent: "observe",
              raw_response: "idle",
              actions_taken: [{ Idle: null }],
              tool_calls: [
                {
                  tick: 12,
                  tool_name: "llm.complete",
                  provider: "qwen",
                  status: "Succeeded",
                  latency_ms: 48,
                  request_units: 120,
                  response_units: 40,
                  error_message: null
                }
              ],
              latency_ms: 48
            }
          ]
        ],
        telemetry_windows: [
          {
            tick: 12,
            agents: []
          }
        ],
        training_samples: [
          {
            tick: 12,
            agent_id: "agent-a",
            path_distance: 16.75,
            action_outcomes: {
              submitted: 1,
              executed: 1,
              rejected: 0,
              queued: 0
            },
            encounter_transition: null,
            tool_call_latency_ms: 48,
            tool_call_error_count: 0
          }
        ]
      }
    });

    const replay = parseReplayFile(document);
    const summary = summarizeReplayFile(replay);

    expect(replay.header.name).toBe("flagship-mmo-loop");
    expect(summary.traceCount).toBe(1);
    expect(summary.trainingSampleCount).toBe(1);
    expect(summary.totalPathDistance).toBe(16.75);
    expect(summary.latestTelemetryTick).toBe(12);
  });

  test("accepts shard incident summary TOON documents", () => {
    const document = encode({
      document_type: "shard_incident_summary",
      payload: {
        shard_id: "alpha-1",
        latest_tick: 360,
        severity: "Warning",
        summary: "Shard alpha-1 requires attention",
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
        notes: ["tool-call error rate exceeds 10%"]
      }
    });

    const summary = parseShardIncidentSummary(document);
    expect(summary.shard_id).toBe("alpha-1");
    expect(summary.severity).toBe("Warning");
    expect(summary.notes).toHaveLength(1);
  });

  test("accepts tool-call and rollup TOON documents", () => {
    const toolDocument = encode({
      document_type: "agent_tool_call_event",
      payload: {
        tick: 12,
        agent_entity_id: 44,
        trace: {
          tick: 12,
          tool_name: "llm.complete",
          provider: "qwen",
          status: "TimedOut",
          latency_ms: 48,
          request_units: 120,
          response_units: 40,
          error_message: "timeout"
        }
      }
    });
    const rollupDocument = encode({
      document_type: "agent_tick_rollup",
      payload: {
        tick_start: 1,
        tick_end: 60,
        agent_entity_id: 44,
        total_distance: 18.5,
        submitted_action_count: 4,
        executed_action_count: 3,
        rejected_action_count: 1,
        tool_call_count: 1,
        tool_error_count: 1,
        visible_entity_count: 12,
        audible_event_count: 3,
        message_count: 2,
        average_tool_latency_ms: 48
      }
    });

    const toolEvent = parseAgentToolCallEvent(toolDocument);
    const toolSummary = summarizeAgentToolCallEvent(toolEvent);
    expect(toolEvent.agent_entity_id).toBe(44);
    expect(toolSummary.status).toBe("TimedOut");
    expect(toolSummary.errorMessage).toBe("timeout");

    const rollup = parseAgentTickRollup(rollupDocument);
    const rollupSummary = summarizeAgentTickRollup(rollup);
    expect(rollup.agent_entity_id).toBe(44);
    expect(rollupSummary.totalDistance).toBe(18.5);
    expect(rollupSummary.toolErrorCount).toBe(1);
  });

  test("routes live debug documents by TOON document type", () => {
    const replayDocument = encode({
      document_type: "replay_file",
      payload: {
        header: {
          name: "flagship-mmo-loop",
          timestamp: 1_741_315_200,
          world_seed: 42,
          tick_count: 120,
          agent_count: 2,
          notes: "acceptance"
        },
        traces: [],
        telemetry_windows: [],
        training_samples: []
      }
    });

    const replay = parseLiveDebugDocument(replayDocument);
    expect(replay.kind).toBe("replay");

    const telemetry = parseLiveDebugDocument(
      encode({
        document_type: "versioned_tick_telemetry",
        payload: {
          version: "V1",
          payload: {
            tick: 9,
            agents: []
          }
        }
      })
    );
    expect(telemetry.kind).toBe("tickTelemetry");
    if (telemetry.kind !== "tickTelemetry") {
      throw new Error("expected tick telemetry document");
    }
    expect(telemetry.payload.tickTelemetry.tick).toBe(9);
  });
});
