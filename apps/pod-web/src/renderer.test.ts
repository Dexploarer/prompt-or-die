import { describe, expect, test } from "bun:test";

import { buildAmbientChunkDressingPlan } from "./renderer";
import {
  describeEnvironmentPreset,
  sampleLakeMask,
  sampleTerrainHeight,
  sampleTimeLapseEnvironment,
  WATER_LEVEL
} from "./landscape";

describe("pod-web renderer landscape helpers", () => {
  test("classifies bright flagship environments as daylight", () => {
    expect(
      describeEnvironmentPreset({
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
      })
    ).toBe("daylight");
  });

  test("builds a deterministic non-flat heightfield for the terrain mesh", () => {
    expect(sampleTerrainHeight(0, 0)).toBeCloseTo(sampleTerrainHeight(0, 0), 6);
    expect(sampleTerrainHeight(0, 0)).not.toBeCloseTo(sampleTerrainHeight(42, -18), 3);
    expect(sampleTerrainHeight(-36, 24)).not.toBeCloseTo(sampleTerrainHeight(68, 64), 3);
    expect(sampleTerrainHeight(18, -82)).toBeGreaterThan(sampleTerrainHeight(0, 0));
    expect(sampleTerrainHeight(-74, 4)).toBeGreaterThan(sampleTerrainHeight(12, 12));
  });

  test("carves a lagoon basin below the waterline", () => {
    expect(sampleLakeMask(18, -14)).toBeGreaterThan(0.8);
    expect(sampleTerrainHeight(18, -14)).toBeLessThan(WATER_LEVEL);
  });

  test("animates the flagship environment through a daylight cycle", () => {
    const morning = sampleTimeLapseEnvironment(
      {
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
      },
      0
    );
    const noon = sampleTimeLapseEnvironment(morning.environment, 45);

    expect(morning.timeOfDayHours).toBeGreaterThanOrEqual(0);
    expect(noon.environment.sunDirection[1]).toBeGreaterThan(morning.environment.sunDirection[1]);
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
        const [x, _y, z] = instance.position;
        expect(Math.hypot(x, z)).toBeGreaterThan(11.99);
        expect(sampleLakeMask(x, z)).toBeLessThan(0.21);
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
});
