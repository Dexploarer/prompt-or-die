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
        roundTripMs: 42,
        jitterMs: 6,
        lastPongServerTick: 128
      })
    ).toBe("connected · Authoritative tick 128 · net 42ms rtt / 6ms jitter");
  });
});
