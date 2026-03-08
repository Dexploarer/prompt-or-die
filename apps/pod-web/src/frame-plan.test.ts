import { describe, expect, test } from "bun:test";

import type { ThreeJsWebGpuFrame } from "./contracts";
import { buildCameraPose, buildFramePlan, splitSpriteBatchesByTint } from "./frame-plan";
import { resolveQualityProfile } from "./quality";

function testEnvironment(): ThreeJsWebGpuFrame["environment"] {
  return {
    biomeId: "test-biome",
    skyColor: [0.04, 0.06, 0.1, 1],
    fogColor: [0.035, 0.06, 0.1, 1],
    fogNear: 28,
    fogFar: 180,
    ambientColor: [0.66, 0.82, 1],
    ambientIntensity: 1.2,
    sunColor: [1, 0.94, 0.82],
    sunIntensity: 2.6,
    sunDirection: [24, 42, 18],
    fillColor: [0.42, 0.74, 1],
    fillIntensity: 0.7,
    fillDirection: [-18, 14, -10],
    rimColor: [0.29, 0.76, 1],
    rimIntensity: 12,
    groundColor: [0.055, 0.09, 0.14, 1],
    starfieldIntensity: 0.9
  };
}

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
        rotation: Math.PI,
        viewportWidth: 1280,
        viewportHeight: 720
      },
      backgroundColor: [0, 0, 0, 1],
      environment: testEnvironment(),
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

  test("culls far instances and splits visible meshes into lod tiers", () => {
    const frame: ThreeJsWebGpuFrame = {
      camera: {
        x: 0,
        y: 0,
        zoom: 1,
        rotation: Math.PI,
        viewportWidth: 1280,
        viewportHeight: 720
      },
      backgroundColor: [0, 0, 0, 1],
      environment: testEnvironment(),
      overlayCommands: [],
      meshBatches: [
        {
          mesh: "tower",
          material: "stone",
          layer: 0,
          phase: "opaque",
          sortDepth: 0,
          renderOrder: 0,
          transparent: false,
          doubleSided: false,
          castShadows: true,
          receiveShadows: true,
          tint: [1, 1, 1, 1],
          roughness: 1,
          metallic: 0,
          emissive: [0, 0, 0],
          depthWrite: true,
          depthTest: true,
          instances: [
            { position: [0, 0, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
            { position: [0, 0, 24], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
            { position: [0, 0, 60], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
            { position: [0, 0, 240], rotation: [0, 0, 0, 1], scale: [1, 1, 1] }
          ]
        }
      ],
      spriteBatches: [],
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

    const plan = buildFramePlan(frame, {
      frustumCulling: false,
      fov: 140,
      pitch: 0.18,
      height: 0,
      baseDistance: 20,
      minDistance: 20,
      maxDistance: 20,
      meshCullDistance: 120,
      highDetailDistance: 21,
      mediumDetailDistance: 55,
      shadowDistance: 36
    });

    expect(plan.meshBatches).toHaveLength(3);
    expect(plan.meshBatches.map((batch) => batch.lodLevel)).toEqual([0, 1, 2]);
    expect(plan.meshBatches[0]?.visibleCount).toBe(1);
    expect(plan.meshBatches[0]?.batch.castShadows).toBe(true);
    expect(plan.meshBatches[1]?.visibleCount).toBe(1);
    expect(plan.meshBatches[1]?.batch.castShadows).toBe(false);
    expect(plan.meshBatches[2]?.visibleCount).toBe(1);
  });

  test("drops instances outside the distance budget", () => {
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
      environment: testEnvironment(),
      overlayCommands: [],
      meshBatches: [
        {
          mesh: "column",
          material: "stone",
          layer: 0,
          phase: "opaque",
          sortDepth: 0,
          renderOrder: 0,
          transparent: false,
          doubleSided: false,
          castShadows: true,
          receiveShadows: true,
          tint: [1, 1, 1, 1],
          roughness: 1,
          metallic: 0,
          emissive: [0, 0, 0],
          depthWrite: true,
          depthTest: true,
          instances: [
            { position: [0, 0, -10], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
            { position: [0, 0, -220], rotation: [0, 0, 0, 1], scale: [1, 1, 1] }
          ]
        }
      ],
      spriteBatches: [],
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

    const plan = buildFramePlan(frame, {
      frustumCulling: false,
      meshCullDistance: 120,
      highDetailDistance: 30,
      mediumDetailDistance: 80
    });

    expect(plan.meshBatches).toHaveLength(1);
    expect(plan.meshBatches[0]?.visibleCount).toBe(1);
  });

  test("tracks visible chunks separately from nearby warm chunks", () => {
    const frame: ThreeJsWebGpuFrame = {
      camera: {
        x: 0,
        y: 0,
        zoom: 1,
        rotation: Math.PI,
        viewportWidth: 1280,
        viewportHeight: 720
      },
      backgroundColor: [0, 0, 0, 1],
      environment: testEnvironment(),
      overlayCommands: [],
      meshBatches: [
        {
          mesh: "tower",
          material: "stone",
          layer: 0,
          phase: "opaque",
          sortDepth: 0,
          renderOrder: 0,
          transparent: false,
          doubleSided: false,
          castShadows: true,
          receiveShadows: true,
          tint: [1, 1, 1, 1],
          roughness: 1,
          metallic: 0,
          emissive: [0, 0, 0],
          depthWrite: true,
          depthTest: true,
          instances: [{ position: [0, 0, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] }]
        },
        {
          mesh: "canopy-tree",
          material: "leaf",
          layer: 0,
          phase: "opaque",
          sortDepth: 0,
          renderOrder: 1,
          transparent: false,
          doubleSided: false,
          castShadows: true,
          receiveShadows: true,
          tint: [1, 1, 1, 1],
          roughness: 1,
          metallic: 0,
          emissive: [0, 0, 0],
          depthWrite: true,
          depthTest: true,
          instances: [
            { position: [30, 0, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
            { position: [34, 0, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] }
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
          renderOrder: 2,
          transparent: true,
          depthWrite: false,
          depthTest: true,
          instances: [
            {
              position: [32, 0, 0],
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

    const plan = buildFramePlan(frame, {
      frustumCulling: false,
      fov: 140,
      pitch: 0,
      height: 0,
      baseDistance: 12,
      minDistance: 12,
      maxDistance: 12,
      meshCullDistance: 14,
      spriteCullDistance: 14,
      worldChunkSize: 24,
      preloadChunkRadius: 1
    });

    expect(plan.visibleWorldChunks).toEqual(["0:0"]);
    expect(plan.preloadedWorldChunks).toContain("1:0");
    expect(plan.preloadedWorldChunks).toHaveLength(9);
    expect(plan.meshBatches).toHaveLength(1);
    expect(plan.spriteBatches).toHaveLength(0);
    expect(plan.prewarmMeshRequests).toHaveLength(3);
    expect(
      plan.prewarmMeshRequests.map((request) => [request.batch.mesh, request.lodLevel])
    ).toEqual([
      ["canopy-tree", 0],
      ["canopy-tree", 1],
      ["tower", 0]
    ]);
    expect(plan.prewarmSpriteRequests).toHaveLength(1);
    expect(plan.prewarmSpriteRequests[0]?.batch.texture).toBe("mist");
  });
});

describe("resolveQualityProfile", () => {
  test("selects an ultra profile for stronger webgpu hardware", () => {
    const quality = resolveQualityProfile({
      backend: "webgpu",
      hardwareConcurrency: 12,
      deviceMemory: 16,
      devicePixelRatio: 2
    });

    expect(quality.preset).toBe("ultra");
    expect(quality.maxPixelRatio).toBe(2);
    expect(quality.enableShadows).toBe(true);
  });

  test("drops to performance on weaker webgl hardware", () => {
    const quality = resolveQualityProfile({
      backend: "webgl2",
      hardwareConcurrency: 2,
      deviceMemory: 2,
      devicePixelRatio: 1
    });

    expect(quality.preset).toBe("performance");
    expect(quality.enableShadows).toBe(false);
    expect(quality.meshCullDistance).toBeLessThan(quality.spriteCullDistance + 80);
  });
});
