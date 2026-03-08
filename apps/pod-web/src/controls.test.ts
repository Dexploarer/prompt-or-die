import { describe, expect, test } from "bun:test";

import type { CameraState, NetworkEntitySnapshot } from "./contracts";
import {
  cameraRelativeMovementDirection,
  focusGameplaySurface,
  isGameplayKeyCode,
  pickWorldGroundPoint,
  resolvePointerTarget
} from "./controls";

const DEFAULT_CAMERA: CameraState = {
  x: 0,
  y: 0,
  zoom: 1.25,
  rotation: 0,
  pitch: 0.22,
  focusHeight: 1.8,
  followDistance: 9,
  shoulderOffset: 0,
  viewportWidth: 1280,
  viewportHeight: 720
};

function testEntity(
  id: number,
  position: [number, number],
  footprintRadius: number
): NetworkEntitySnapshot {
  return {
    id,
    position,
    velocity: [0, 0],
    rotation: 0,
    health: 10,
    maxHealth: 10,
    movementSpeed: 4,
    label: `E(${id})`,
    metadata: {
      kind: "Npc",
      chunkKey: null,
      regionId: null,
      regionName: null,
      teamId: null,
      questGraphIds: [],
      factionTrackId: null,
      encounterTableId: null,
      combatStyle: null,
      speciesId: null,
      speciesName: null,
      resourceSkill: null,
      resourceTier: null,
      encounterKind: null,
      faction: null,
      questAnchor: null,
      encounterProfile: null,
      spawnProfile: null,
      atmosphere: null,
      atmosphereVolume: null,
      actorPresentation: {
        profileId: `profile-${id}`,
        meshAssetId: null,
        materialPaletteId: "default",
        animationSetId: "humanoid",
        scaleMultiplier: 1,
        footprintRadius,
        selectionRingScale: 2.2,
        auraColor: [0, 0, 0, 0]
      },
      combatPresentation: null,
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
    }
  };
}

describe("pod-web controls", () => {
  test("maps WASD to camera-relative world movement", () => {
    expect(cameraRelativeMovementDirection(["KeyW"], 0)).toEqual([0, -1]);
    expect(cameraRelativeMovementDirection(["KeyD"], 0)?.[0]).toBeCloseTo(1, 6);
    expect(cameraRelativeMovementDirection(["KeyD"], 0)?.[1]).toBeCloseTo(0, 6);

    const rotatedForward = cameraRelativeMovementDirection(["KeyW"], Math.PI / 2);
    expect(rotatedForward?.[0]).toBeCloseTo(-1, 6);
    expect(rotatedForward?.[1]).toBeCloseTo(0, 6);
  });

  test("identifies gameplay hotkeys and movement keys", () => {
    expect(isGameplayKeyCode("KeyW")).toBe(true);
    expect(isGameplayKeyCode("KeyE")).toBe(true);
    expect(isGameplayKeyCode("Tab")).toBe(true);
    expect(isGameplayKeyCode("Escape")).toBe(false);
    expect(isGameplayKeyCode("ShiftLeft")).toBe(false);
  });

  test("focuses and labels the gameplay surface", () => {
    let focused = false;
    const surface = {
      tabIndex: -1,
      focus() {
        focused = true;
      },
      setAttribute(name: string, value: string) {
        if (name === "aria-label") {
          expect(value).toBe("Game world");
        }
      }
    };

    expect(focusGameplaySurface(surface)).toBe(true);
    expect(surface.tabIndex).toBe(0);
    expect(focused).toBe(true);
  });

  test("projects a center-screen click onto the ground near the camera focus", () => {
    const point = pickWorldGroundPoint(
      [DEFAULT_CAMERA.viewportWidth / 2, DEFAULT_CAMERA.viewportHeight * 0.78],
      { width: DEFAULT_CAMERA.viewportWidth, height: DEFAULT_CAMERA.viewportHeight },
      DEFAULT_CAMERA
    );

    expect(point).not.toBeNull();
    const distanceFromFocus = Math.hypot(point?.[0] ?? 999, point?.[1] ?? 999);
    expect(distanceFromFocus).toBeLessThan(6.5);
  });

  test("selects the closest entity whose footprint contains the click point", () => {
    const selected = resolvePointerTarget(
      [testEntity(2, [4.2, 2.1], 1.4), testEntity(3, [4.9, 2.3], 1.1)],
      [4.3, 2.15]
    );

    expect(selected?.id).toBe(2);
    expect(resolvePointerTarget([testEntity(5, [12, 12], 1.2)], [0, 0])).toBeNull();
  });
});
