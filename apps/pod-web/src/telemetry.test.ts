import { describe, expect, test } from "bun:test";

import type { TickTelemetryEnvelope } from "./contracts";
import {
  applyTickTelemetry,
  createTelemetryOverlayState,
  cycleTelemetrySelection,
  selectedTrajectorySamples,
  setTelemetryEnabled,
  telemetryStats
} from "./telemetry";

function telemetryFrame(
  tick: number,
  {
    agentId = "agent-a",
    entityId = 1001,
    position = tick,
    rejected = 0,
    toolStatus = "Succeeded"
  }: {
    agentId?: string;
    entityId?: number;
    position?: number;
    rejected?: number;
    toolStatus?: string;
  } = {}
): TickTelemetryEnvelope {
  return {
    tickTelemetry: {
      tick,
      agents: [
        {
          tick,
          agent_id: agentId,
          entity_id: entityId,
          runtime_profile: {
            role: "Player",
            agent_type: "Human",
            capabilities: {}
          },
          visible_entity_count: 6,
          audible_event_count: 2,
          message_count: 1,
          available_action_count: 4,
          objective_count: 1,
          trajectory: {
            start: {
              tick,
              elapsed_secs: tick / 60,
              position: [position, position],
              velocity: [1, 0],
              rotation: 0
            },
            end: {
              tick,
              elapsed_secs: (tick + 1) / 60,
              position: [position + 1, position + 0.5],
              velocity: [1, 0.5],
              rotation: 0.1
            },
            displacement: [1, 0.5],
            distance_travelled: 1.12
          },
          action_trace: [
            { source: "ExternalSubmission", stage: "Submitted", action: { Idle: null } },
            { source: "ExternalSubmission", stage: "Executed", action: { Idle: null } },
            ...new Array(rejected).fill(null).map(() => ({
              source: "ExternalSubmission",
              stage: "Rejected",
              action: { Idle: null },
              rejection_reason: "cooldown"
            }))
          ],
          tool_calls: [
            {
              tick,
              tool_name: "llm.complete",
              provider: "qwen",
              status: toolStatus,
              latency_ms: 48,
              request_units: 120,
              response_units: 40,
              error_message: toolStatus === "Succeeded" ? null : "timeout"
            }
          ]
        }
      ]
    },
    recovery: {
      authoritative_tick: tick,
      authoritative_digest: 42,
      predicted_tick: tick,
      predicted_digest: 42,
      presentation_tick: tick,
      desired_presentation_tick: tick,
      presentation_drift_ticks: 0.25,
      history_snapshots: 4,
      oldest_authoritative_tick: tick - 3,
      latest_authoritative_tick: tick,
      pending_action_batches: 1,
      replayed_action_count: 2,
      controlled_entity_drift: null,
      recovery: {
        awaiting_full_snapshot: false,
        request_attempts: 0,
        last_request_server_tick: null,
        last_request_digest: null,
        next_retry_tick: null
      }
    }
  };
}

describe("telemetry overlay state", () => {
  test("retains trajectory history and computes selected-agent stats", () => {
    const state = createTelemetryOverlayState(3);
    setTelemetryEnabled(state, true);
    applyTickTelemetry(state, telemetryFrame(10, { position: 10 }));
    applyTickTelemetry(state, telemetryFrame(11, { position: 11 }));
    applyTickTelemetry(state, telemetryFrame(12, { position: 12, rejected: 1, toolStatus: "TimedOut" }));
    applyTickTelemetry(state, telemetryFrame(13, { position: 13 }));

    const trajectory = selectedTrajectorySamples(state);
    const stats = telemetryStats(state);

    expect(trajectory).toHaveLength(3);
    expect(trajectory[0]?.position).toEqual([12, 11.5]);
    expect(trajectory.at(-1)?.position).toEqual([14, 13.5]);
    expect(stats.retainedTicks).toBe(3);
    expect(stats.rejectedActions).toBe(0);
    expect(stats.lastToolStatus).toBe("Succeeded");
    expect(stats.trajectoryDistance).toBeGreaterThan(3);
  });

  test("cycles between latest telemetry targets", () => {
    const state = createTelemetryOverlayState(4);
    applyTickTelemetry(state, {
      tickTelemetry: {
        tick: 1,
        agents: [
          telemetryFrame(1, { agentId: "agent-a", entityId: 1001 }).tickTelemetry.agents[0]!,
          telemetryFrame(1, { agentId: "agent-b", entityId: 1002 }).tickTelemetry.agents[0]!
        ]
      }
    });

    expect(telemetryStats(state).selectedEntityId).toBe(1001);
    cycleTelemetrySelection(state, 1);
    expect(telemetryStats(state).selectedEntityId).toBe(1002);
  });

  test("surfaces recovery and tool-call failures in the HUD summary", () => {
    const state = createTelemetryOverlayState(4);
    applyTickTelemetry(state, {
      ...telemetryFrame(22, {
        rejected: 2,
        toolStatus: "TimedOut"
      }),
      recovery: {
        authoritative_tick: 22,
        authoritative_digest: 99,
        predicted_tick: 21,
        predicted_digest: 77,
        presentation_tick: 20.5,
        desired_presentation_tick: 20,
        presentation_drift_ticks: 2.5,
        history_snapshots: 8,
        oldest_authoritative_tick: 14,
        latest_authoritative_tick: 22,
        pending_action_batches: 2,
        replayed_action_count: 4,
        controlled_entity_drift: null,
        recovery: {
          awaiting_full_snapshot: true,
          request_attempts: 3,
          last_request_server_tick: 22,
          last_request_digest: 99,
          next_retry_tick: 26
        }
      }
    });

    const stats = telemetryStats(state);
    expect(stats.toolErrors).toBe(1);
    expect(stats.recoverySummary).toContain("Awaiting full snapshot");
    expect(stats.nextRetryTick).toBe(26);
  });
});
