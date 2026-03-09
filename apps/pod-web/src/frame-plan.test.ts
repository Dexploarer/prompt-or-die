import { describe, expect, test } from "bun:test";

import type { ThreeJsWebGpuFrame } from "./contracts";
import {
  buildCameraPose,
  buildFramePlan,
  computeCombatCameraPressure,
  sampleAnimatedInstanceTransform,
  splitSpriteBatchesByTint
} from "./frame-plan";
import { sampleSurfaceHeight, sampleTerrainHeight } from "./landscape";
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
      zoom: 1.1,
      rotation: Math.PI / 2,
      pitch: 0.34,
      followDistance: 13.5,
      focusHeight: 2.2,
      shoulderOffset: 0.9,
      viewportWidth: 1280,
      viewportHeight: 720
    });

    expect(pose.target).toEqual([24, sampleSurfaceHeight(24, -10) + 2.2, -10]);
    expect(pose.position[0]).toBeGreaterThan(24);
    expect(Math.abs(pose.position[2] + 10)).toBeLessThan(1.2);
    expect(pose.position[1]).toBeGreaterThan(pose.target[1]);
  });

  test("pulls the camera in front of terrain occlusion instead of burying it in cliffs", () => {
    const pose = buildCameraPose({
      x: 18,
      y: -14,
      zoom: 0.76,
      rotation: 0.12,
      pitch: 0.42,
      followDistance: 17,
      focusHeight: 2.2,
      shoulderOffset: 0.9,
      viewportWidth: 1280,
      viewportHeight: 720
    });

    const clearance = sampleTerrainHeight(pose.position[0], pose.position[2]) + 1.45;
    expect(pose.position[1]).toBeGreaterThanOrEqual(clearance - 0.01);
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
    expect(plan.meshBatches[2]?.visibleCount).toBe(1);
    expect(plan.meshBatches[2]?.batch.castShadows).toBe(false);
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

    expect(plan.worldChunkSize).toBe(24);
    expect(plan.visibleWorldChunks).toEqual(["0:0"]);
    expect(plan.preloadedWorldChunks).toContain("1:0");
    expect(plan.preloadedWorldChunks).toHaveLength(9);
    expect(plan.meshBatches).toHaveLength(1);
    expect(plan.spriteBatches).toHaveLength(0);
    expect(plan.prewarmMeshRequests).toHaveLength(2);
    expect(
      plan.prewarmMeshRequests.map((request) => [request.batch.mesh, request.lodLevel])
    ).toEqual([
      ["canopy-tree", 0],
      ["tower", 0]
    ]);
    expect(plan.prewarmSpriteRequests).toHaveLength(1);
    expect(plan.prewarmSpriteRequests[0]?.batch.texture).toBe("mist");
  });
});

describe("sampleAnimatedInstanceTransform", () => {
  test("keeps static props grounded while animating hovering companions", () => {
    const staticProp = sampleAnimatedInstanceTransform(
      {
        position: [12, 4, -6],
        rotation: [0, 0, 0, 1],
        scale: [2, 3, 2],
        sourceEntity: 44,
        animationSetId: "static-prop",
        motionSpeed: 0
      },
      2.4
    );
    const hoveringCompanion = sampleAnimatedInstanceTransform(
      {
        position: [12, 4, -6],
        rotation: [0, 0, 0, 1],
        scale: [1, 1.2, 1],
        sourceEntity: 9,
        animationSetId: "companion-hover",
        motionSpeed: 0.2
      },
      2.4
    );

    expect(staticProp.position).toEqual([12, 4, -6]);
    expect(hoveringCompanion.position[1]).toBeGreaterThan(4.12);
    expect(hoveringCompanion.scale[1]).not.toBeCloseTo(1.2, 4);
  });

  test("differentiates critical rings from destination markers", () => {
    const criticalRing = sampleAnimatedInstanceTransform(
      {
        position: [4, 0.15, -3],
        rotation: [0, 0, 0, 1],
        scale: [1, 1, 1],
        sourceEntity: 0,
        animationSetId: "critical-ring",
        motionSpeed: 0
      },
      0.3
    );
    const destinationRing = sampleAnimatedInstanceTransform(
      {
        position: [4, 0.15, -3],
        rotation: [0, 0, 0, 1],
        scale: [1, 1, 1],
        sourceEntity: 0,
        animationSetId: "destination-ring",
        motionSpeed: 0
      },
      0.3
    );

    expect(criticalRing.scale[0]).toBeGreaterThan(destinationRing.scale[0]);
    expect(criticalRing.position[1]).toBeGreaterThan(destinationRing.position[1]);
  });

  test("gives combat banners a lighter float than health bars", () => {
    const banner = sampleAnimatedInstanceTransform(
      {
        position: [4, 2.2, -3],
        rotation: [0, 0, 0, 1],
        scale: [1.6, 0.22, 1],
        sourceEntity: 12,
        animationSetId: "combat-banner",
        motionSpeed: 0
      },
      0.8
    );
    const healthBar = sampleAnimatedInstanceTransform(
      {
        position: [4, 2.2, -3],
        rotation: [0, 0, 0, 1],
        scale: [1.2, 0.14, 1],
        sourceEntity: 12,
        animationSetId: "health-bar",
        motionSpeed: 0,
        healthRatio: 0.4
      },
      0.8
    );

    expect(banner.position[1]).toBeGreaterThan(healthBar.position[1]);
    expect(banner.scale[1]).toBeGreaterThan(0.22);
    expect(healthBar.scale[1]).toBeGreaterThan(0.14);
  });

  test("gives beasts a lower, heavier stance than humanoids", () => {
    const beast = sampleAnimatedInstanceTransform(
      {
        position: [2, 1.6, 5],
        rotation: [0, 0, 0, 1],
        scale: [1.4, 1.4, 2],
        sourceEntity: 5,
        animationSetId: "rift-beast",
        motionSpeed: 0.8
      },
      1.2
    );
    const humanoid = sampleAnimatedInstanceTransform(
      {
        position: [2, 1.6, 5],
        rotation: [0, 0, 0, 1],
        scale: [1.4, 1.4, 2],
        sourceEntity: 5,
        animationSetId: "hero-runescape",
        motionSpeed: 0.8
      },
      1.2
    );

    expect(beast.scale[1]).toBeLessThan(humanoid.scale[1]);
    expect(beast.scale[2]).toBeGreaterThan(humanoid.scale[2]);
    expect(Math.abs(beast.rotation[1])).toBeGreaterThan(Math.abs(humanoid.rotation[1]));
  });

  test("adds gait bounce and event pulse response to moving humanoids", () => {
    const neutral = sampleAnimatedInstanceTransform(
      {
        position: [0, 2, 0],
        rotation: [0, 0, 0, 1],
        scale: [1, 2, 1],
        sourceEntity: 3,
        animationSetId: "hero-runescape",
        motionSpeed: 0.9,
        controlled: true,
        healthRatio: 0.82
      },
      1.8,
      0
    );
    const pulsed = sampleAnimatedInstanceTransform(
      {
        position: [0, 2, 0],
        rotation: [0, 0, 0, 1],
        scale: [1, 2, 1],
        sourceEntity: 3,
        animationSetId: "hero-runescape",
        motionSpeed: 0.9,
        controlled: true,
        healthRatio: 0.82
      },
      1.8,
      1
    );

    expect(neutral.position[1]).toBeGreaterThan(2);
    expect(neutral.scale[1]).toBeLessThan(2);
    expect(pulsed.position[1]).toBeGreaterThan(neutral.position[1]);
    expect(pulsed.position[2]).toBeGreaterThan(neutral.position[2]);
    expect(pulsed.scale[0]).toBeGreaterThan(neutral.scale[0]);
  });

  test("gives pulsed beasts a heavier forward drive than pulsed humanoids", () => {
    const beast = sampleAnimatedInstanceTransform(
      {
        position: [0, 1.8, 0],
        rotation: [0, 0, 0, 1],
        scale: [1.4, 1.4, 1.8],
        sourceEntity: 5,
        animationSetId: "rift-beast",
        motionSpeed: 0.9
      },
      1.4,
      1
    );
    const humanoid = sampleAnimatedInstanceTransform(
      {
        position: [0, 1.8, 0],
        rotation: [0, 0, 0, 1],
        scale: [1.1, 1.9, 1.1],
        sourceEntity: 5,
        animationSetId: "hero-runescape",
        motionSpeed: 0.9
      },
      1.4,
      1
    );

    expect(beast.position[2]).toBeGreaterThan(humanoid.position[2]);
    expect(beast.scale[2]).toBeGreaterThan(humanoid.scale[2]);
  });

  test("gives swimmers forward glide while keeping beasts heavier than humanoids", () => {
    const humanoidSwim = sampleAnimatedInstanceTransform(
      {
        position: [3, 1.5, 4],
        rotation: [0, 0, 0, 1],
        scale: [1, 1.9, 1],
        sourceEntity: 8,
        animationSetId: "humanoid-swim",
        motionSpeed: 0.9,
        controlled: true
      },
      1.6
    );
    const beastSwim = sampleAnimatedInstanceTransform(
      {
        position: [3, 1.5, 4],
        rotation: [0, 0, 0, 1],
        scale: [1.3, 1.6, 1.9],
        sourceEntity: 8,
        animationSetId: "rift-beast-swim",
        motionSpeed: 0.9
      },
      1.6
    );

    expect(humanoidSwim.position[2]).toBeGreaterThan(4.05);
    expect(humanoidSwim.position[1]).toBeGreaterThan(1.55);
    expect(beastSwim.scale[0]).toBeGreaterThan(humanoidSwim.scale[0]);
    expect(Math.abs(beastSwim.rotation[2])).toBeLessThan(Math.abs(humanoidSwim.rotation[2]));
  });
});

describe("computeCombatCameraPressure", () => {
  test("raises camera pressure for close attackable targets", () => {
    const close = computeCombatCameraPressure(
      {
        position: [0, 0],
        health: 82,
        maxHealth: 100
      },
      {
        position: [2.5, 0],
        canAttack: true
      },
      0.1
    );
    const far = computeCombatCameraPressure(
      {
        position: [0, 0],
        health: 82,
        maxHealth: 100
      },
      {
        position: [16, 0],
        canAttack: true
      },
      0.1
    );

    expect(close.closeRangeBlend).toBeGreaterThan(far.closeRangeBlend);
    expect(close.targetPressure).toBeGreaterThan(far.targetPressure);
    expect(close.combatPressure).toBeGreaterThan(far.combatPressure);
  });

  test("still raises pressure from low health without an attackable target", () => {
    const pressured = computeCombatCameraPressure(
      {
        position: [0, 0],
        health: 12,
        maxHealth: 100
      },
      null,
      0
    );

    expect(pressured.lowHealthPressure).toBeGreaterThan(0.7);
    expect(pressured.combatPressure).toBeGreaterThan(0.6);
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
