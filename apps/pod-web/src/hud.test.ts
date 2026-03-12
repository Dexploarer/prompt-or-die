import { describe, expect, test } from "bun:test";

import {
  compactRuntimeStats,
  formatConnectionSummary,
  formatTransportDebugSummary,
  highlightEventFeedback
} from "./hud";
import type { NetworkGameEvent } from "./contracts";
import type { PodThreeRendererStats } from "./renderer";

function sampleStats(): PodThreeRendererStats {
  return {
    backend: "webgpu",
    qualityPreset: "ultra",
    renderThread: "worker",
    requestedRenderThread: "worker",
    renderThreadFallbackReason: null,
    environmentPreset: "daylight",
    timeOfDayHours: 12.2,
    landscapeMode: "cliff-lagoon-heightfield",
    waterMode: "animated-lagoon",
    drawCalls: 48,
    triangles: 117277,
    textures: 12,
    pixelRatio: 1.92,
    frameMs: 16.4,
    residentGeometryAssets: 8,
    residentSpriteAssets: 2,
    pendingGeometryAssets: 0,
    pendingSpriteAssets: 0,
    geometryLoadsCompleted: 8,
    spriteLoadsCompleted: 2,
    averageGeometryLoadMs: 3.4,
    averageSpriteLoadMs: 1.2,
    slowestGeometryLoadMs: 8.7,
    slowestSpriteLoadMs: 2.4,
    mainThreadPerf: {
      warmupMs: 180,
      submissionsCompleted: 12,
      averageSubmissionMs: 0.6,
      slowestSubmissionMs: 1.4,
      byKind: {
        frame: {
          submissionsCompleted: 10,
          averageSubmissionMs: 0.5,
          slowestSubmissionMs: 1.1
        },
        control: {
          submissionsCompleted: 1,
          averageSubmissionMs: 0.8,
          slowestSubmissionMs: 0.8
        },
        resize: {
          submissionsCompleted: 1,
          averageSubmissionMs: 1.4,
          slowestSubmissionMs: 1.4
        }
      }
    },
    runtimePerf: {
      warmupMs: 420,
      frameBudgetMs: 16.67,
      framesRendered: 12,
      stableFrames: 10,
      slowFrames: 2,
      stableFramePercent: 83.3,
      slowestFrameMs: 28.9
    },
    ambientInstances: 14,
    visibleWorldChunks: 4,
    preloadedWorldChunks: 8
  };
}

function sampleEvent(kind: string, summary: string): NetworkGameEvent {
  return {
    tick: 42,
    origin: [1, 2],
    kind,
    summary,
    entityIds: [1001]
  };
}

describe("hud formatting", () => {
  test("compacts renderer stats into a shorter gameplay-facing line", () => {
    expect(compactRuntimeStats(sampleStats())).toBe(
      "webgpu/worker · daylight 12.2h · 16.4ms @ 1.92x · 48 draw / 117k tris · 4/8 chunks · 10/0 assets · load 3.4/1.2ms · submit 0.6ms · warm 420ms · stable 83%"
    );
  });

  test("maps combat and world events into stronger feedback labels", () => {
    expect(highlightEventFeedback(sampleEvent("Damage", "12 to Rift Beast"))).toBe(
      "Hit confirmed · 12 to Rift Beast"
    );
    expect(highlightEventFeedback(sampleEvent("Capture", "Spirit Cub captured"))).toBe(
      "Capture secured · Spirit Cub captured"
    );
    expect(highlightEventFeedback(sampleEvent("Summon", "Companion recalled"))).toBe(
      "Companion ready · Companion recalled"
    );
    expect(highlightEventFeedback(sampleEvent("Dialogue", "Merchant greeted you"))).toBe(
      "Merchant greeted you"
    );
  });

  test("adds shard RTT and jitter without bloating the connection line", () => {
    expect(
      formatConnectionSummary({
        phase: "connected",
        detail: "Authoritative tick 128",
        url: "ws://127.0.0.1:7778",
        tick: 128,
        entityCount: 22,
        controlledEntity: 12,
        authoritativeDigest: 9912,
        clientId: "client-a",
        roundTripMs: 42,
        jitterMs: 6,
        lastPongServerTick: 128,
        heartbeatAgeMs: 120
      })
    ).toBe("connected · Authoritative tick 128 · net 42ms rtt / 6ms jitter");
  });

  test("surfaces shard pressure and timeout counts compactly", () => {
    expect(
      formatConnectionSummary(
        {
          phase: "connected",
          detail: "Authoritative tick 128",
          url: "ws://127.0.0.1:7778",
          tick: 128,
          entityCount: 22,
          controlledEntity: 12,
          authoritativeDigest: 9912,
          clientId: "client-a",
          roundTripMs: 42,
          jitterMs: 6,
          lastPongServerTick: 128,
          heartbeatAgeMs: 240
        },
        {
          shard_id: "direct-connect",
          latest_tick: 128,
          client_count: 3,
          resumed_sessions: 2,
          recovery_snapshots_sent: 4,
          recovery_delivery_failures: 1,
          client_inactivity_timeout_ticks: 600,
          queue_pressure_warn_depth: 192,
          total_pending_action_queue_depth: 9,
          peak_pending_action_queue_depth: 14,
          queue_pressure_client_count: 2,
          total_inbound_messages: 32,
          total_outbound_messages: 64,
          total_inbound_bytes: 1024,
          total_outbound_bytes: 20480,
          action_batches_received: 12,
          full_snapshots_sent: 4,
          total_full_snapshot_bytes: 8192,
          max_full_snapshot_bytes: 3072,
          total_recovery_snapshot_bytes: 4096,
          full_snapshot_requests: 1,
          ping_requests: 9,
          state_deltas_sent: 24,
          delta_messages_sent: 20,
          total_delta_bytes: 2048,
          max_delta_bytes: 384,
          total_delta_entities_updated: 32,
          total_delta_entities_destroyed: 5,
          event_batches_sent: 6,
          debug_documents_sent: 4,
          rejected_messages_sent: 1,
          timed_out_clients: 1,
          queue_pressure_events: 3,
          clients: []
        }
      )
    ).toBe(
      "connected · Authoritative tick 128 · net 42ms rtt / 6ms jitter · shard 3c / resumes 2 / recover 4 / recover-fail 1 / q9 / pressure 2 / timeouts 1 / 20kB out"
    );
  });

  test("formats transport metrics for the debug-side summary without touching the gameplay HUD", () => {
    expect(
      formatTransportDebugSummary(
        {
          shard_id: "direct-connect",
          latest_tick: 128,
          client_count: 3,
          resumed_sessions: 2,
          recovery_snapshots_sent: 4,
          recovery_delivery_failures: 1,
          client_inactivity_timeout_ticks: 600,
          queue_pressure_warn_depth: 192,
          total_pending_action_queue_depth: 9,
          peak_pending_action_queue_depth: 14,
          queue_pressure_client_count: 2,
          total_inbound_messages: 32,
          total_outbound_messages: 64,
          total_inbound_bytes: 1024,
          total_outbound_bytes: 20480,
          action_batches_received: 12,
          full_snapshots_sent: 4,
          total_full_snapshot_bytes: 8192,
          max_full_snapshot_bytes: 3072,
          total_recovery_snapshot_bytes: 4096,
          full_snapshot_requests: 1,
          ping_requests: 9,
          state_deltas_sent: 24,
          delta_messages_sent: 20,
          total_delta_bytes: 2048,
          max_delta_bytes: 384,
          total_delta_entities_updated: 32,
          total_delta_entities_destroyed: 5,
          event_batches_sent: 6,
          debug_documents_sent: 4,
          rejected_messages_sent: 1,
          timed_out_clients: 1,
          queue_pressure_events: 3,
          clients: []
        },
        7
      )
    ).toBe(
      "transport · 3 clients · snap 4x 2.0kB avg / 3.1kB max · delta 20x 102B avg / +32/-5 · recover 4 / 4.1kB · q9 now / peak 14 · pressure 2 (3) · timeouts 1 · 7 samples"
    );
  });
});
