import { describe, expect, test } from "bun:test";

import { describeTargetAffordances, formatTargetSummary } from "./affordances";
import type { NetworkEntityMetadataSnapshot } from "./contracts";

function metadata(
  overrides: Partial<NetworkEntityMetadataSnapshot> = {}
): NetworkEntityMetadataSnapshot {
  return {
    kind: "Unknown",
    teamId: null,
    combatStyle: null,
    speciesId: null,
    speciesName: null,
    resourceSkill: null,
    resourceTier: null,
    encounterKind: null,
    atmosphere: null,
    atmosphereVolume: null,
    actorPresentation: null,
    combatPresentation: null,
    interaction: {
      canInspect: false,
      canInteract: false,
      canAttack: false,
      canGather: false,
      canLoot: false,
      canCapture: false,
      canCommandCompanion: false,
      canChat: false
    },
    ...overrides
  };
}

describe("target affordances", () => {
  test("formats target summaries with health and distance", () => {
    expect(
      formatTargetSummary(
        {
          id: 9,
          position: [30, 40],
          velocity: [0, 0],
          rotation: 0,
          health: 28,
          maxHealth: 40,
          label: "Monster-Wolf",
          metadata: metadata({
            kind: "WildCreature",
            interaction: {
              canInspect: true,
              canInteract: false,
              canAttack: true,
              canGather: false,
              canLoot: false,
              canCapture: true,
              canCommandCompanion: false,
              canChat: false
            }
          })
        },
        {
          id: 12,
          position: [10, 10],
          velocity: [0, 0],
          rotation: 0,
          label: "Player",
          metadata: metadata({
            kind: "Player",
            interaction: {
              canInspect: true,
              canInteract: true,
              canAttack: true,
              canGather: false,
              canLoot: false,
              canCapture: false,
              canCommandCompanion: false,
              canChat: true
            }
          })
        }
      )
    ).toBe("Monster-Wolf · E(9) · wild creature · 28/40 hp · 36u away");
  });

  test("describes capture and combat affordances for wild creatures", () => {
    expect(
      describeTargetAffordances({
        id: 9,
        position: [0, 0],
        velocity: [0, 0],
        rotation: 0,
        label: "Wild Creature",
        metadata: metadata({
          kind: "WildCreature",
          interaction: {
            canInspect: true,
            canInteract: false,
            canAttack: true,
            canGather: false,
            canLoot: false,
            canCapture: true,
            canCommandCompanion: false,
            canChat: false
          }
        })
      })
    ).toBe("Space attack · C capture · E inspect");
  });

  test("describes loot affordances for containers", () => {
    expect(
      describeTargetAffordances({
        id: 14,
        position: [0, 0],
        velocity: [0, 0],
        rotation: 0,
        label: "Loot Cache",
        metadata: metadata({
          kind: "LootContainer",
          interaction: {
            canInspect: true,
            canInteract: false,
            canAttack: false,
            canGather: false,
            canLoot: true,
            canCapture: false,
            canCommandCompanion: false,
            canChat: false
          }
        })
      })
    ).toBe("R loot · E inspect");
  });

  test("describes static scenery from authoritative metadata", () => {
    expect(
      describeTargetAffordances({
        id: 21,
        position: [0, 0],
        velocity: [0, 0],
        rotation: 0,
        label: "Shard Anchor",
        metadata: metadata({
          kind: "Scenery",
          interaction: {
            canInspect: true,
            canInteract: false,
            canAttack: false,
            canGather: false,
            canLoot: false,
            canCapture: false,
            canCommandCompanion: false,
            canChat: false
          }
        })
      })
    ).toBe("Static scenery");
  });
});
