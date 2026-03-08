import { describe, expect, test } from "bun:test";

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
});
