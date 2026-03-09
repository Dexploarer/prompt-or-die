import { describe, expect, test } from "bun:test";

import { resolveQualityProfile } from "./quality";

describe("pod-web quality defaults", () => {
  test("does not enable the debug grid by default on flagship presets", () => {
    expect(
      resolveQualityProfile({
        backend: "webgpu",
        preferredPreset: "ultra",
        hardwareConcurrency: 12,
        deviceMemory: 16,
        devicePixelRatio: 2
      }).showGrid
    ).toBe(false);

    expect(
      resolveQualityProfile({
        backend: "webgpu",
        preferredPreset: "high",
        hardwareConcurrency: 8,
        deviceMemory: 8,
        devicePixelRatio: 2
      }).showGrid
    ).toBe(false);
  });

  test("allocates richer landscape surfaces on flagship presets", () => {
    const ultra = resolveQualityProfile({
      backend: "webgpu",
      preferredPreset: "ultra",
      hardwareConcurrency: 12,
      deviceMemory: 16,
      devicePixelRatio: 2
    });
    const performance = resolveQualityProfile({
      backend: "webgl2",
      preferredPreset: "performance",
      hardwareConcurrency: 4,
      deviceMemory: 4,
      devicePixelRatio: 1
    });

    expect(ultra.terrainTextureSize).toBeGreaterThan(performance.terrainTextureSize);
    expect(ultra.waterTextureSize).toBeGreaterThan(performance.waterTextureSize);
    expect(ultra.skyTextureSize).toBeGreaterThan(performance.skyTextureSize);
  });
});
