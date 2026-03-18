import { describe, expect, test } from "bun:test";
import { BoxGeometry, Scene } from "three";

import type { PodThreeAssetRegistry } from "./assets";
import {
  PodThreeWorldRenderer,
  buildAmbientChunkDressingPlan,
  resolveRuntimeAssetRegistryBootstrapOptions,
  resolveVisibleMeshBatchResources
} from "./renderer";
import type { PlannedMeshBatch } from "./frame-plan";
import {
  describeEnvironmentPreset,
  type LandscapeEnvironment,
  sampleBackcountryMask,
  sampleLakeMask,
  sampleTerrainMaterial,
  sampleTerrainHeight,
  sampleTimeLapseEnvironment,
  sampleValleyFloorMask,
  sampleWaterSurfaceStyle,
  WATER_LEVEL
} from "./landscape";
import { meshGroundAnchorHeight } from "./mesh-bounds";

describe("pod-web renderer landscape helpers", () => {
  const flagshipEnvironment: LandscapeEnvironment = {
    biomeId: "verdant-hollow",
    skyColor: [0.64, 0.8, 0.98, 1],
    fogColor: [0.72, 0.84, 0.78, 1],
    fogNear: 30,
    fogFar: 196,
    ambientColor: [0.82, 0.92, 0.88],
    ambientIntensity: 1.4,
    sunColor: [1, 0.96, 0.84],
    sunIntensity: 2.95,
    sunDirection: [30, 48, 18],
    fillColor: [0.48, 0.76, 0.94],
    fillIntensity: 0.88,
    fillDirection: [-18, 14, -10],
    rimColor: [0.4, 0.88, 0.78],
    rimIntensity: 8.5,
    groundColor: [0.19, 0.33, 0.21, 1],
    starfieldIntensity: 0.08
  };
  const resonantEnvironment: LandscapeEnvironment = {
    biomeId: "resonant-shore",
    skyColor: [0.58, 0.76, 0.95, 1],
    fogColor: [0.68, 0.8, 0.86, 1],
    fogNear: 24,
    fogFar: 182,
    ambientColor: [0.78, 0.88, 0.92],
    ambientIntensity: 1.48,
    sunColor: [1, 0.93, 0.8],
    sunIntensity: 3.08,
    sunDirection: [26, 42, 22],
    fillColor: [0.38, 0.78, 0.98],
    fillIntensity: 1.02,
    fillDirection: [-16, 12, -12],
    rimColor: [0.46, 0.92, 0.98],
    rimIntensity: 10.2,
    groundColor: [0.18, 0.26, 0.24, 1],
    starfieldIntensity: 0.05
  };

  test("classifies bright flagship environments as daylight", () => {
    expect(describeEnvironmentPreset(flagshipEnvironment)).toBe("daylight");
  });

  test("builds a deterministic non-flat heightfield for the terrain mesh", () => {
    expect(sampleTerrainHeight(0, 0)).toBeCloseTo(sampleTerrainHeight(0, 0), 6);
    expect(sampleTerrainHeight(0, 0)).not.toBeCloseTo(sampleTerrainHeight(42, -18), 3);
    expect(sampleTerrainHeight(-36, 24)).not.toBeCloseTo(sampleTerrainHeight(68, 64), 3);
    expect(sampleTerrainHeight(18, -82)).toBeGreaterThan(sampleTerrainHeight(0, 0));
    expect(sampleTerrainHeight(-74, 4)).toBeGreaterThan(sampleTerrainHeight(12, 12));
  });

  test("carves a scenic valley floor between the showcase landing and backcountry peaks", () => {
    const valleyFloor = sampleTerrainHeight(6, -58);
    const leftRange = sampleTerrainHeight(-42, -82);
    const rightRange = sampleTerrainHeight(44, -94);

    expect(sampleValleyFloorMask(6, -58)).toBeGreaterThan(0.45);
    expect(sampleBackcountryMask(-42, -82)).toBeGreaterThan(0.55);
    expect(sampleBackcountryMask(44, -94)).toBeGreaterThan(0.55);
    expect(leftRange).toBeGreaterThan(valleyFloor + 12);
    expect(rightRange).toBeGreaterThan(valleyFloor + 16);
  });

  test("carves a lagoon basin below the waterline", () => {
    expect(sampleLakeMask(18, -14)).toBeGreaterThan(0.8);
    expect(sampleTerrainHeight(18, -14)).toBeLessThan(WATER_LEVEL);
  });

  test("animates the flagship environment through a daylight cycle", () => {
    const morning = sampleTimeLapseEnvironment(flagshipEnvironment, 0);
    const noon = sampleTimeLapseEnvironment(morning.environment, 45);

    expect(morning.timeOfDayHours).toBeGreaterThanOrEqual(0);
    expect(noon.environment.sunDirection[1]).toBeGreaterThan(morning.environment.sunDirection[1]);
  });

  test("pushes shoreline terrain warmer than highland cliffs", () => {
    const shore = sampleTerrainMaterial(flagshipEnvironment, 0, -14);
    const highland = sampleTerrainMaterial(flagshipEnvironment, 44, -94);

    expect(shore.shoreMask).toBeGreaterThan(0.03);
    expect(highland.highlandMask).toBeGreaterThan(shore.highlandMask);
    expect(shore.tint[0] + shore.tint[1]).toBeGreaterThan(
      highland.tint[0] + highland.tint[1] - 0.01
    );
  });

  test("tints resonant-shore terrain toward warmer alpine stone than verdant shorelines", () => {
    const verdant = sampleTerrainMaterial(flagshipEnvironment, 0, -14);
    const resonant = sampleTerrainMaterial(resonantEnvironment, 0, -14);

    expect(resonant.shoreMask).toBeGreaterThan(0.03);
    expect(resonant.tint[0]).toBeGreaterThan(verdant.tint[0]);
    expect(resonant.tint[0] + resonant.tint[1]).toBeGreaterThan(
      verdant.tint[0] + verdant.tint[1]
    );
  });

  test("brightens the resonant-shore landing corridor toward the shoreline vista", () => {
    const landingPath = sampleTerrainMaterial(resonantEnvironment, 2.8, -4.8);
    const inlandShoulder = sampleTerrainMaterial(resonantEnvironment, -5.4, 0.6);

    expect(landingPath.brightness).toBeGreaterThan(inlandShoulder.brightness);
    expect(landingPath.tint[0] + landingPath.tint[1]).toBeGreaterThan(
      inlandShoulder.tint[0] + inlandShoulder.tint[1]
    );
  });

  test("pushes resonant-shore backcountry into brighter alpine tones", () => {
    const valley = sampleTerrainMaterial(resonantEnvironment, 6, -58);
    const highland = sampleTerrainMaterial(resonantEnvironment, 44, -94);

    expect(highland.brightness).toBeGreaterThanOrEqual(valley.brightness);
    expect(highland.tint[0] + highland.tint[1] + highland.tint[2]).toBeGreaterThan(
      valley.tint[0] + valley.tint[1] + valley.tint[2]
    );
    expect(highland.tint[0]).toBeGreaterThan(valley.tint[0]);
  });

  test("keeps water visuals deterministic while shifting with time of day", () => {
    const day = sampleWaterSurfaceStyle(flagshipEnvironment, 12);
    const nightEnvironment = sampleTimeLapseEnvironment(flagshipEnvironment, 0).environment;
    const night = sampleWaterSurfaceStyle(nightEnvironment, 12);
    const later = sampleWaterSurfaceStyle(flagshipEnvironment, 24);

    expect(day.opacity).toBeGreaterThan(night.opacity);
    expect(day.shallowColor[1]).toBeGreaterThan(night.shallowColor[1]);
    expect(day.textureOffset).not.toEqual(later.textureOffset);
    expect(sampleWaterSurfaceStyle(flagshipEnvironment, 12)).toEqual(day);
  });

  test("builds deterministic ambient chunk dressing outside the lagoon and hub", () => {
    const left = buildAmbientChunkDressingPlan({
      visibleChunkKeys: ["0:0", "1:0", "0:1"],
      preloadedChunkKeys: ["0:0", "1:0", "0:1", "1:1"],
      cameraPosition: [12, 0, 8],
      qualityPreset: "high",
      worldChunkSize: 24,
      highDetailDistance: 34,
      mediumDetailDistance: 110
    });
    const right = buildAmbientChunkDressingPlan({
      visibleChunkKeys: ["0:0", "1:0", "0:1"],
      preloadedChunkKeys: ["0:0", "1:0", "0:1", "1:1"],
      cameraPosition: [12, 0, 8],
      qualityPreset: "high",
      worldChunkSize: 24,
      highDetailDistance: 34,
      mediumDetailDistance: 110
    });

    expect(left.totalInstances).toBeGreaterThan(0);
    expect(
      left.meshBatches.map((batch) => ({
        key: batch.key,
        visibleCount: batch.visibleCount,
        positions: batch.instances.map((instance) => instance.position.map((value) => Number(value.toFixed(3))))
      }))
    ).toEqual(
      right.meshBatches.map((batch) => ({
        key: batch.key,
        visibleCount: batch.visibleCount,
        positions: batch.instances.map((instance) => instance.position.map((value) => Number(value.toFixed(3))))
      }))
    );

    for (const batch of left.meshBatches) {
      for (const instance of batch.instances) {
        const [x, y, z] = instance.position;
        expect(Math.hypot(x, z)).toBeGreaterThan(11.99);
        expect(sampleLakeMask(x, z)).toBeLessThan(0.21);
        expect(y).toBeCloseTo(
          sampleTerrainHeight(x, z) + meshGroundAnchorHeight(batch.batch.mesh, instance.scale[1]),
          5
        );
      }
    }
  });

  test("prewarms chunk dressing assets for nearby streamed regions", () => {
    const plan = buildAmbientChunkDressingPlan({
      visibleChunkKeys: ["0:0"],
      preloadedChunkKeys: ["0:0", "0:1", "1:0", "1:1"],
      cameraPosition: [6, 0, 6],
      qualityPreset: "ultra",
      worldChunkSize: 24,
      highDetailDistance: 42,
      mediumDetailDistance: 132
    });

    const meshes = new Set(plan.prewarmRequests.map((request) => request.batch.mesh));
    expect(meshes.has("canopy-tree")).toBe(true);
    expect(meshes.has("weathered-boulder")).toBe(true);
    expect(meshes.has("glass-spire")).toBe(true);
  });

  test("clusters alpine tree dressing lower than basalt dressing in the alpine backdrop", () => {
    const plan = buildAmbientChunkDressingPlan({
      visibleChunkKeys: [
        "-4:-4",
        "-4:-3",
        "-4:-2",
        "-4:-1",
        "-3:-4",
        "-3:-3",
        "-3:-2",
        "-3:-1",
        "-2:-4",
        "-2:-3",
        "-2:-2",
        "-2:-1",
        "-1:-4",
        "-1:-3",
        "-1:-2",
        "-1:-1",
        "0:-4",
        "0:-3",
        "0:-2",
        "0:-1",
        "1:-4",
        "1:-3",
        "1:-2",
        "1:-1",
        "2:-4",
        "2:-3",
        "2:-2",
        "2:-1"
      ],
      preloadedChunkKeys: [
        "-4:-4",
        "-4:-3",
        "-4:-2",
        "-4:-1",
        "-3:-4",
        "-3:-3",
        "-3:-2",
        "-3:-1",
        "-2:-4",
        "-2:-2",
        "-2:-1",
        "-1:-2",
        "-1:-1",
        "0:-2",
        "0:-1",
        "1:-2",
        "-2:-3",
        "-1:-3",
        "0:-3",
        "1:-3",
        "-1:-4",
        "0:-4",
        "1:-4",
        "2:-4",
        "2:-3",
        "2:-2"
      ],
      cameraPosition: [0, 0, -36],
      qualityPreset: "ultra",
      worldChunkSize: 24,
      highDetailDistance: 48,
      mediumDetailDistance: 132
    });

    const canopy = plan.meshBatches.find(
      (batch) => batch.batch.mesh === "canopy-tree" && batch.instances.length > 0
    );
    const sapling = plan.meshBatches.find(
      (batch) => batch.key.includes("pine-sapling") && batch.instances.length > 0
    );
    const basalt = plan.meshBatches.find((batch) => batch.batch.mesh === "basalt-column");
    const treeBatch = canopy ?? sapling;

    expect(treeBatch).toBeDefined();
    expect(basalt).toBeDefined();
    const canopyAverageHeight =
      (treeBatch?.instances.reduce((total, instance) => total + instance.position[1], 0) ?? 0) /
      Math.max(treeBatch?.instances.length ?? 1, 1);
    const basaltAverageHeight =
      (basalt?.instances.reduce((total, instance) => total + instance.position[1], 0) ?? 0) /
      Math.max(basalt?.instances.length ?? 1, 1);

    expect(canopyAverageHeight).toBeLessThan(basaltAverageHeight);
  });

  test("resolves visible mesh assets in parallel before awaiting completion", async () => {
    const visibleBatches: PlannedMeshBatch[] = [
      {
        key: "mesh:a",
        batch: {
          mesh: "glass-spire",
          material: "shore-tideglass",
          layer: 1,
          phase: "opaque",
          sortDepth: 0,
          renderOrder: 1,
          transparent: false,
          doubleSided: false,
          castShadows: true,
          receiveShadows: true,
          tint: [1, 1, 1, 1],
          roughness: 0.8,
          metallic: 0.1,
          emissive: [0, 0, 0],
          depthWrite: true,
          depthTest: true,
          instances: []
        },
        lodLevel: 0,
        visibleCount: 1,
        instances: [],
        matrices: []
      },
      {
        key: "mesh:b",
        batch: {
          mesh: "weathered-boulder",
          material: "shore-cairn",
          layer: 1,
          phase: "opaque",
          sortDepth: 0,
          renderOrder: 1,
          transparent: false,
          doubleSided: false,
          castShadows: true,
          receiveShadows: true,
          tint: [1, 1, 1, 1],
          roughness: 0.8,
          metallic: 0.1,
          emissive: [0, 0, 0],
          depthWrite: true,
          depthTest: true,
          instances: []
        },
        lodLevel: 0,
        visibleCount: 1,
        instances: [],
        matrices: []
      }
    ];
    const calls: string[] = [];
    const resolvers = new Map<string, (geometry: BoxGeometry) => void>();
    const assetRegistry: PodThreeAssetRegistry = {
      resolveGeometry(batch: PlannedMeshBatch["batch"]) {
        calls.push(batch.mesh);
        return new Promise<BoxGeometry>((resolve) => {
          resolvers.set(batch.mesh, resolve);
        });
      },
      resolveSpriteTexture() {
        throw new Error("not used in mesh resource resolution");
      }
    };

    const pending = resolveVisibleMeshBatchResources(
      visibleBatches,
      assetRegistry,
      { environmentIntensity: 1 } as never
    );

    expect(calls).toEqual(["glass-spire", "weathered-boulder"]);

    const resolveFirst = resolvers.get("glass-spire");
    const resolveSecond = resolvers.get("weathered-boulder");
    if (!resolveFirst || !resolveSecond) {
      throw new Error("expected both mesh resolutions to be pending");
    }
    resolveSecond(new BoxGeometry(1, 1, 1));
    resolveFirst(new BoxGeometry(1, 1, 1));
    const resolved = await pending;

    expect(resolved).toHaveLength(2);
    expect(resolved.map((entry) => entry.planned.key)).toEqual(["mesh:a", "mesh:b"]);
  });

  test("keeps worker asset bootstrap on source meshes and eager manifest loading", () => {
    expect(
      resolveRuntimeAssetRegistryBootstrapOptions({
        width: 1280,
        height: 720
      } as OffscreenCanvas)
    ).toEqual({
      preferCompressedMeshVariants: false,
      preferCompressedTextureVariants: false,
      preferNonBlockingFallbacks: false,
      lazyManifestLoad: false
    });

    expect(
      resolveRuntimeAssetRegistryBootstrapOptions({
        clientWidth: 1280,
        clientHeight: 720,
        width: 1280,
        height: 720
      } as HTMLCanvasElement)
    ).toEqual({
      preferCompressedMeshVariants: true,
      preferCompressedTextureVariants: false,
      preferNonBlockingFallbacks: true,
      lazyManifestLoad: true
    });
  });

  test("reuses cached fallback mesh materials across repeated syncs", async () => {
    const geometry = new BoxGeometry(1, 1, 1);
    const batch: PlannedMeshBatch = {
      key: "mesh:hero",
      batch: {
        mesh: "adventurer-avatar",
        material: "default",
        layer: 0,
        phase: "opaque",
        sortDepth: 0,
        renderOrder: 1,
        transparent: false,
        doubleSided: false,
        castShadows: true,
        receiveShadows: true,
        tint: [1, 1, 1, 1],
        roughness: 0.82,
        metallic: 0.08,
        emissive: [0, 0, 0],
        depthWrite: true,
        depthTest: true,
        instances: []
      },
      lodLevel: 0,
      visibleCount: 1,
      instances: [
        {
          position: [0, 0, 0],
          rotation: [0, 0, 0, 1],
          scale: [1, 1, 1],
          sourceEntity: 7
        }
      ],
      matrices: []
    };
    const renderer = Object.create(PodThreeWorldRenderer.prototype) as {
      scene: Scene;
      quality: { environmentIntensity: number };
      assetRegistry: PodThreeAssetRegistry;
      meshEntries: Map<string, { mesh: unknown; material: unknown }>;
      meshMaterialCache: Map<string, unknown>;
      entityPulseUntilMs: Map<number, number>;
      smoothedInstanceTransforms: Map<string, unknown>;
      sceneTimeMs: () => number;
      syncMeshBatches: (batches: PlannedMeshBatch[]) => Promise<void>;
    };
    renderer.scene = new Scene();
    renderer.quality = { environmentIntensity: 1 };
    renderer.assetRegistry = {
      resolveGeometry() {
        return geometry;
      },
      resolveSpriteTexture() {
        throw new Error("not used in mesh sync");
      }
    };
    renderer.meshEntries = new Map();
    renderer.meshMaterialCache = new Map();
    renderer.entityPulseUntilMs = new Map();
    renderer.smoothedInstanceTransforms = new Map();
    renderer.sceneTimeMs = () => 0;

    await renderer.syncMeshBatches([batch]);
    const firstEntry = renderer.meshEntries.get(batch.key);

    await renderer.syncMeshBatches([batch]);
    const secondEntry = renderer.meshEntries.get(batch.key);

    expect(firstEntry).toBeDefined();
    expect(secondEntry).toBe(firstEntry);
    expect(secondEntry?.material).toBe(firstEntry?.material);
    expect(renderer.meshMaterialCache.size).toBe(1);
  });
});
