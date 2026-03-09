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
});
