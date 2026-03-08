import { describe, expect, test } from "bun:test";

import type { NetworkWorldPopulationState } from "./contracts";
import {
  buildPopulationHeatmapModel,
  formatPopulationHeatmapLegend,
  parsePopulationChunkKey
} from "./population-heatmap";

function testPopulation(): NetworkWorldPopulationState {
  return {
    tick: 42,
    chunks: [
      {
        chunkKey: "-1:0",
        regionId: "verdant-heart",
        regionName: "Verdant Heart",
        biomeId: "verdant-hollow",
        questGraphIds: ["verdant-intro"],
        factionTrackId: "verdant-wardens",
        encounterTableIds: ["verdant-heart-wildlife"],
        counts: {
          players: 1,
          npcs: 0,
          wildCreatures: 6,
          companions: 1,
          resourceNodes: 0,
          lootContainers: 0,
          scenery: 2
        },
        activeEntityCount: 10,
        ambientPopulationCap: 8,
        spawnBudgetRemaining: 0,
        pendingRespawns: 3,
        nextRespawnTick: 88,
        populationPressure: 1
      },
      {
        chunkKey: "0:0",
        regionId: "verdant-heart",
        regionName: "Verdant Heart",
        biomeId: "verdant-hollow",
        questGraphIds: ["verdant-intro"],
        factionTrackId: "verdant-wardens",
        encounterTableIds: ["verdant-heart-wildlife"],
        counts: {
          players: 1,
          npcs: 1,
          wildCreatures: 2,
          companions: 0,
          resourceNodes: 1,
          lootContainers: 0,
          scenery: 1
        },
        activeEntityCount: 6,
        ambientPopulationCap: 8,
        spawnBudgetRemaining: 4,
        pendingRespawns: 0,
        nextRespawnTick: null,
        populationPressure: 0.5
      },
      {
        chunkKey: "0:1",
        regionId: "spirewatch",
        regionName: "Spirewatch",
        biomeId: "spirewatch",
        questGraphIds: ["spirewatch-scouting"],
        factionTrackId: "spirewatch-alliance",
        encounterTableIds: ["spirewatch-encounters"],
        counts: {
          players: 0,
          npcs: 2,
          wildCreatures: 1,
          companions: 0,
          resourceNodes: 1,
          lootContainers: 1,
          scenery: 3
        },
        activeEntityCount: 8,
        ambientPopulationCap: 10,
        spawnBudgetRemaining: 7,
        pendingRespawns: 1,
        nextRespawnTick: 93,
        populationPressure: 0.3
      }
    ],
    regions: []
  };
}

describe("parsePopulationChunkKey", () => {
  test("parses signed chunk coordinates", () => {
    expect(parsePopulationChunkKey("-2:3")).toEqual([-2, 3]);
    expect(parsePopulationChunkKey("bad")).toBeNull();
  });
});

describe("buildPopulationHeatmapModel", () => {
  test("builds a focused grid from authoritative chunk population", () => {
    const model = buildPopulationHeatmapModel(testPopulation(), {
      chunkKey: "-1:0",
      regionId: "verdant-heart"
    });

    expect(model).not.toBeNull();
    expect(model?.columns).toBe(2);
    expect(model?.rows).toBe(2);
    expect(model?.focusedCell?.chunkKey).toBe("-1:0");
    expect(model?.focusedCell?.pendingRespawns).toBe(3);
    expect(model?.focusedCell?.intensity ?? 0).toBeGreaterThan(0.9);
  });

  test("falls back to the hottest chunk when no focus is provided", () => {
    const model = buildPopulationHeatmapModel(testPopulation());

    expect(model?.focusedCell?.chunkKey).toBe("-1:0");
    expect(model?.maxPendingRespawns).toBe(3);
    expect(formatPopulationHeatmapLegend(model)).toContain("pressure 1.00");
    expect(formatPopulationHeatmapLegend(model)).toContain("respawns 3 @88");
  });
});
