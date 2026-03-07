import { describe, expect, test } from "bun:test";

import { createMeshMaterial } from "./assets";
import type { ThreeJsMeshBatch } from "./contracts";

const QUALITY = {
  environmentIntensity: 1
} as const;

function meshBatch(
  overrides: Partial<ThreeJsMeshBatch> = {}
): ThreeJsMeshBatch {
  return {
    mesh: "basalt-column",
    material: "obsidian",
    layer: 0,
    phase: "opaque",
    sortDepth: 0,
    renderOrder: 0,
    transparent: false,
    doubleSided: false,
    castShadows: true,
    receiveShadows: true,
    tint: [1, 1, 1, 1],
    roughness: 0.92,
    metallic: 0.08,
    emissive: [0, 0, 0],
    depthWrite: true,
    depthTest: true,
    instances: [],
    ...overrides
  };
}

describe("createMeshMaterial", () => {
  test("uses toon shading for stylized opaque world geometry", () => {
    const material = createMeshMaterial(meshBatch(), 0, QUALITY);
    expect(material.type).toBe("MeshToonMaterial");
  });

  test("keeps transparent glass surfaces on the standard material path", () => {
    const material = createMeshMaterial(
      meshBatch({
        mesh: "glass-spire",
        material: "aether-glass",
        transparent: true,
        metallic: 0.4
      }),
      0,
      QUALITY
    );
    expect(material.type).toBe("MeshStandardMaterial");
  });
});
