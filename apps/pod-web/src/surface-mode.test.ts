import { expect, test } from "bun:test";

import type { NetworkEntitySnapshot } from "./contracts";
import {
  entityUsesSwimSurface,
  surfaceModeFromEntity
} from "./surface-mode";

function makeEntity(animationSetId: string | null): NetworkEntitySnapshot {
  return {
    id: 1,
    label: "Player",
    position: [0, 0],
    velocity: [0, 0],
    rotation: 0,
    health: 10,
    maxHealth: 10,
    movementSpeed: null,
    metadata: {
      kind: "Player",
      actorPresentation:
        animationSetId == null
          ? null
          : {
              profileId: "player",
              meshAssetId: "hero",
              materialPaletteId: "hero-cloth",
              animationSetId,
              scaleMultiplier: 1,
              footprintRadius: 0.8,
              selectionRingScale: 1,
              auraColor: [1, 1, 1, 1]
            },
      interaction: {
        canInspect: true,
        canInteract: false,
        canGather: false,
        canLoot: false,
        canAttack: false,
        canCapture: false,
        canCommandCompanion: false,
        canChat: false
      }
    }
  } as unknown as NetworkEntitySnapshot;
}

test("surfaceModeFromEntity derives swimming from the animation set id", () => {
  expect(surfaceModeFromEntity(makeEntity("humanoid-swim"))).toBe("swim");
  expect(entityUsesSwimSurface(makeEntity("rift-beast-swim"))).toBe(true);
});

test("surfaceModeFromEntity defaults to ground for missing or non-swim animations", () => {
  expect(surfaceModeFromEntity(makeEntity("humanoid-idle"))).toBe("ground");
  expect(surfaceModeFromEntity(makeEntity(null))).toBe("ground");
  expect(entityUsesSwimSurface(null)).toBe(false);
});
