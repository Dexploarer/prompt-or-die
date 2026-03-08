import { describe, expect, test } from "bun:test";
import { encode } from "@toon-format/toon";

import {
  applyNetworkStateDelta,
  buildAuthoritativeWorldFrame,
  encodeDirectConnectActionBatch,
  encodeDirectConnectConnectMessage,
  encodeDirectConnectDebugTelemetryMessage,
  encodeDirectConnectFullSnapshotRequest,
  type NetworkWorldSnapshot,
  parseAgentTickRollup,
  parseAgentToolCallEvent,
  parseDirectConnectServerMessage,
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

  test("parses direct-connect welcome and delta messages", () => {
    const welcome = parseDirectConnectServerMessage(
      JSON.stringify({
        Welcome: {
          client_id: "client-1",
          reconnect_token: "reconnect-1",
          tick: 12,
          controlled_entity: 44,
          authoritative_digest: 991,
          snapshot: {
            tick: 12,
            entities: [
              {
                id: 44,
                position: [12, 18],
                velocity: [1, 0],
                rotation: 0.4,
                label: "Hero"
              }
            ]
          }
        }
      })
    );

    expect(welcome.kind).toBe("welcome");
    if (welcome.kind !== "welcome") {
      throw new Error("expected welcome");
    }
    expect(welcome.snapshot.entities[0]?.position).toEqual([12, 18]);

    const delta = parseDirectConnectServerMessage(
      JSON.stringify({
        StateDelta: {
          tick: 13,
          acknowledged_action_tick: 12,
          authoritative_digest: 992,
          is_full_snapshot: false,
          delta: {
            tick: 13,
            updated: [
              {
                id: 44,
                position: { x: 14, y: 19 },
                velocity: [2, 0],
                rotation: 0.55,
                label: "Hero"
              }
            ],
            destroyed: [99]
          }
        }
      })
    );

    expect(delta.kind).toBe("stateDelta");
    if (delta.kind !== "stateDelta") {
      throw new Error("expected delta");
    }
    expect(delta.delta.updated[0]?.position).toEqual([14, 19]);
    expect(delta.acknowledgedActionTick).toBe(12);
  });

  test("applies authoritative state deltas and builds a live world frame", () => {
    const baseline: NetworkWorldSnapshot = {
      tick: 10,
      entities: [
        {
          id: 1,
          position: [10, 10],
          velocity: [0, 0],
          rotation: 0,
          label: "Hero"
        },
        {
          id: 2,
          position: [20, 12],
          velocity: [0, 0],
          rotation: 0,
          label: "Wall-East"
        }
      ]
    };

    const next = applyNetworkStateDelta(
      baseline,
      {
        tick: 11,
        updated: [
          {
            id: 1,
            position: [14, 12],
            velocity: [4, 0],
            rotation: 0.3,
            label: "Hero"
          },
          {
            id: 3,
            position: [18, 20],
            velocity: [0, 0],
            rotation: 0,
            label: "Monster-Wolf"
          }
        ],
        destroyed: [2]
      },
      false
    );

    expect(next.tick).toBe(11);
    expect(next.entities.map((entity) => entity.id)).toEqual([1, 3]);

    const frame = buildAuthoritativeWorldFrame(next, {
      controlledEntity: 1,
      viewportWidth: 1440,
      viewportHeight: 900
    });

    expect(frame.camera.viewportWidth).toBe(1440);
    expect(frame.meshBatches.some((batch) => batch.mesh.includes("adventurer"))).toBe(true);
    expect(frame.meshBatches.some((batch) => batch.mesh.includes("rift-beast"))).toBe(true);
    expect(frame.spriteBatches.some((batch) => batch.texture === "selection-ring")).toBe(true);
  });

  test("encodes browser direct-connect client messages with Rust enum tags", () => {
    expect(JSON.parse(encodeDirectConnectConnectMessage("WebPlayer", "resume-1"))).toEqual({
      Connect: {
        player_name: "WebPlayer",
        reconnect_token: "resume-1"
      }
    });

    expect(JSON.parse(encodeDirectConnectDebugTelemetryMessage(true))).toEqual({
      SetDebugTelemetry: {
        enabled: true
      }
    });

    expect(JSON.parse(encodeDirectConnectFullSnapshotRequest(42, 99))).toEqual({
      RequestFullSnapshot: {
        last_known_tick: 42,
        last_known_digest: 99
      }
    });

    expect(
      JSON.parse(
        encodeDirectConnectActionBatch(77, [
          { kind: "move", direction: [1, 0] },
          { kind: "attackTarget", target: 9 },
          { kind: "speak", message: "hello", volume: "Normal" }
        ])
      )
    ).toEqual({
      ActionBatch: {
        tick: 77,
        actions: [
          { Move: { direction: [1, 0] } },
          { AttackTarget: { target: 9 } },
          { Speak: { message: "hello", volume: "Normal" } }
        ]
      }
    });
  });
});
