import { describe, expect, test } from "bun:test";

import { compactRuntimeStats, formatConnectionSummary, highlightEventFeedback } from "./hud";
import type { NetworkGameEvent } from "./contracts";
import type { PodThreeRendererStats } from "./renderer";

function sampleStats(): PodThreeRendererStats {
  return {
    backend: "webgpu",
    qualityPreset: "ultra",
    renderThread: "worker",
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
      "webgpu/worker · daylight 12.2h · 16.4ms @ 1.92x · 48 draw / 117k tris · 4/8 chunks · 10/0 assets · ambient 14"
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
          client_inactivity_timeout_ticks: 600,
          queue_pressure_warn_depth: 192,
          total_pending_action_queue_depth: 9,
          queue_pressure_client_count: 2,
          total_inbound_messages: 32,
          total_outbound_messages: 64,
          total_inbound_bytes: 1024,
          total_outbound_bytes: 20480,
          action_batches_received: 12,
          full_snapshot_requests: 1,
          ping_requests: 9,
          state_deltas_sent: 24,
          event_batches_sent: 6,
          debug_documents_sent: 4,
          rejected_messages_sent: 1,
          timed_out_clients: 1,
          queue_pressure_events: 3,
          clients: []
        }
      )
    ).toBe(
      "connected · Authoritative tick 128 · net 42ms rtt / 6ms jitter · shard 3c / resumes 2 / q9 / pressure 2 / timeouts 1 / 20kB out"
    );
  });
});
