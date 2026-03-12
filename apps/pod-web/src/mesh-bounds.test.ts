import { describe, expect, test } from "bun:test";

import {
  meshGroundAnchorHeight,
  meshVisualHeight,
  resolveMeshBounds
} from "./mesh-bounds";

describe("pod-web mesh bounds", () => {
  test("uses shipped mesh bounds for world anchoring", () => {
    expect(resolveMeshBounds("adventurer-hero")).toEqual({
      minY: -0.48,
      maxY: 0.57,
      footprintRadius: 0.32
    });
    expect(meshGroundAnchorHeight("adventurer-hero", 2)).toBeCloseTo(0.96, 6);
    expect(meshVisualHeight("rift-beast", 1.9)).toBeCloseTo(1.634, 6);
  });

  test("falls back safely for unknown meshes", () => {
    expect(resolveMeshBounds("unknown-placeholder")).toEqual({
      minY: -0.5,
      maxY: 0.5,
      footprintRadius: 0.7
    });
    expect(meshGroundAnchorHeight("unknown-placeholder", 3)).toBeCloseTo(1.5, 6);
  });
});
