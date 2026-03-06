import { describe, expect, test } from "bun:test";

import type { ThreeJsWebGpuFrame } from "./contracts";
import { buildCameraPose, buildFramePlan, splitSpriteBatchesByTint } from "./frame-plan";

describe("buildCameraPose", () => {
  test("maps 2D camera target and zoom into a perspective rig", () => {
    const pose = buildCameraPose({
      x: 24,
      y: -10,
      zoom: 2,
      rotation: Math.PI / 2,
      viewportWidth: 1280,
      viewportHeight: 720
    });

    expect(pose.target).toEqual([24, 0, -10]);
    expect(pose.position[0]).toBeGreaterThan(24);
    expect(pose.position[2]).toBeCloseTo(-10, 5);
    expect(pose.position[1]).toBeGreaterThan(0);
  });
});

describe("splitSpriteBatchesByTint", () => {
  test("keeps per-instance alpha by splitting tint groups", () => {
    const [batch] = splitSpriteBatchesByTint([
      {
        texture: "mist",
        frame: 0,
        layer: 2,
        billboard: true,
        phase: "transparent",
        sortDepth: 14,
        renderOrder: 6,
        transparent: true,
        depthWrite: false,
        depthTest: true,
        instances: [
          {
            position: [0, 0, 14],
            rotation: [0, 0, 0, 1],
            scale: [2, 2, 1],
            color: [1, 1, 1, 0.25]
          },
          {
            position: [4, 0, 14],
            rotation: [0, 0, 0, 1],
            scale: [2, 2, 1],
            color: [0.5, 1, 0.75, 0.4]
          }
        ]
      }
    ]);

    const groups = splitSpriteBatchesByTint([
      {
        texture: "mist",
        frame: 0,
        layer: 2,
        billboard: true,
        phase: "transparent",
        sortDepth: 14,
        renderOrder: 6,
        transparent: true,
        depthWrite: false,
        depthTest: true,
        instances: [
          {
            position: [0, 0, 14],
            rotation: [0, 0, 0, 1],
            scale: [2, 2, 1],
            color: [1, 1, 1, 0.25]
          },
          {
            position: [4, 0, 14],
            rotation: [0, 0, 0, 1],
            scale: [2, 2, 1],
            color: [0.5, 1, 0.75, 0.4]
          }
        ]
      }
    ]);

    expect(groups).toHaveLength(2);
    expect(batch.batch.renderOrder).toBe(6);
    expect(groups[0]?.tint[3]).toBeGreaterThan(0);
    expect(groups[1]?.tint[3]).toBeGreaterThan(groups[0]?.tint[3] ?? 0);
  });
});

describe("buildFramePlan", () => {
  test("preserves transparent render ordering from the Rust bridge", () => {
    const frame: ThreeJsWebGpuFrame = {
      camera: {
        x: 0,
        y: 0,
        zoom: 1,
        rotation: 0,
        viewportWidth: 1280,
        viewportHeight: 720
      },
      backgroundColor: [0, 0, 0, 1],
      overlayCommands: [],
      meshBatches: [
        {
          mesh: "glass",
          material: "aether",
          layer: 1,
          phase: "transparent",
          sortDepth: 24,
          renderOrder: 0,
          transparent: true,
          doubleSided: true,
          castShadows: false,
          receiveShadows: true,
          tint: [0.5, 0.7, 1, 0.35],
          roughness: 0.2,
          metallic: 0.1,
          emissive: [0.1, 0.2, 0.3],
          depthWrite: false,
          depthTest: true,
          instances: [
            {
              position: [0, 0, 24],
              rotation: [0, 0, 0, 1],
              scale: [1, 1, 1]
            }
          ]
        }
      ],
      spriteBatches: [
        {
          texture: "mist",
          frame: 0,
          layer: 2,
          billboard: true,
          phase: "transparent",
          sortDepth: 18,
          renderOrder: 1,
          transparent: true,
          depthWrite: false,
          depthTest: true,
          instances: [
            {
              position: [0, 0, 18],
              rotation: [0, 0, 0, 1],
              scale: [2, 2, 1],
              color: [1, 1, 1, 0.25]
            }
          ]
        }
      ],
      hints: {
        renderer: "three/webgpu",
        preferredBackend: "webgpu",
        fallbackBackend: "webgl2",
        useInstancing: true,
        sortMetric: "world-z",
        sortOpaqueFrontToBack: true,
        preserveInstanceOrder: true,
        sortTransparentBackToFront: true,
        transparentInstancingStrategy: "shared-sort-depth",
        opaqueDepthWrite: true,
        transparentDepthWrite: false,
        maxPixelRatio: 2
      }
    };

    const plan = buildFramePlan(frame);
    expect(plan.meshBatches).toHaveLength(1);
    expect(plan.spriteBatches).toHaveLength(1);
    expect(plan.meshBatches[0]?.batch.renderOrder).toBe(0);
    expect(plan.spriteBatches[0]?.batch.renderOrder).toBe(1);
    expect(plan.spriteBatches[0]?.matrices).toHaveLength(1);
  });
});
