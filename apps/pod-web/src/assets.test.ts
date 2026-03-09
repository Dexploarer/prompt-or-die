import { describe, expect, test } from "bun:test";
import { BoxGeometry, NoColorSpace, SphereGeometry, Texture } from "three";

import {
  createProceduralSpriteTexture,
  createMeshMaterial,
  DefaultPodThreeAssetRegistry,
  ManifestBackedPodThreeAssetRegistry,
  parsePodThreeAssetManifest,
  resolveManifestMeshAsset,
  resolveManifestSpriteAsset,
  shouldUseProceduralSpriteTexture
} from "./assets";
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
    expect((material as { gradientMap?: { colorSpace?: string } }).gradientMap?.colorSpace).toBe(
      NoColorSpace
    );
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

describe("createProceduralSpriteTexture", () => {
  test("routes shipped ring overlays through the procedural texture path", () => {
    expect(shouldUseProceduralSpriteTexture("/assets/textures/selection-ring.svg")).toBe(true);
    expect(shouldUseProceduralSpriteTexture("/assets/textures/danger-ring.svg")).toBe(true);
    expect(shouldUseProceduralSpriteTexture("/assets/textures/mist-ring.svg")).toBe(true);
    expect(shouldUseProceduralSpriteTexture("combat-banner")).toBe(true);
    expect(shouldUseProceduralSpriteTexture("health-bar")).toBe(true);
    expect(shouldUseProceduralSpriteTexture("/assets/textures/canopy-tree.png")).toBe(false);
  });

  test("creates a worker-safe ring texture for SVG overlay assets", () => {
    const texture = createProceduralSpriteTexture("/assets/textures/selection-ring.svg");
    const image = texture.image as { data: Uint8Array; width: number; height: number };
    const centerIndex = ((Math.floor(image.height / 2) * image.width) + Math.floor(image.width / 2)) * 4 + 3;
    const ringIndex = ((Math.floor(image.height / 2) * image.width) + Math.floor(image.width * 0.88)) * 4 + 3;

    expect(texture.name).toContain("selection-ring");
    expect(image.data[centerIndex]).toBeLessThan(32);
    expect(image.data[ringIndex]).toBeGreaterThan(image.data[centerIndex]);
  });

  test("creates a worker-safe bar texture for combat readability sprites", () => {
    const texture = createProceduralSpriteTexture("combat-banner");
    const image = texture.image as { data: Uint8Array; width: number; height: number };
    const centerIndex =
      ((Math.floor(image.height / 2) * image.width) + Math.floor(image.width / 2)) * 4 + 3;
    const cornerIndex = 3;

    expect(texture.name).toContain("combat-banner");
    expect(image.data[centerIndex]).toBeGreaterThan(200);
    expect(image.data[cornerIndex]).toBeLessThan(80);
  });
});

describe("parsePodThreeAssetManifest", () => {
  test("normalizes optional fields for creator-authored manifests", () => {
    const manifest = parsePodThreeAssetManifest({
      version: 1,
      meshes: {
        "rift-beast": {
          path: "/assets/meshes/rift-beast.gltf",
          aliases: ["monster", "wolf"],
          category: "creature",
          tags: ["wild", "beast"]
        }
      },
      sprites: {
        "selection-ring": {
          path: "/assets/textures/selection-ring.svg",
          ktx2Path: "/assets/textures/selection-ring.ktx2",
          colorSpace: "none",
          repeat: [2, 1],
          offset: [0.25, 0]
        }
      }
    });

    expect(manifest.meshes["rift-beast"]?.category).toBe("creature");
    expect(manifest.sprites["selection-ring"]?.colorSpace).toBe("none");
    expect(manifest.sprites["selection-ring"]?.repeat).toEqual([2, 1]);
  });
});

describe("manifest asset lookup", () => {
  const manifest = parsePodThreeAssetManifest({
    version: 1,
    meshes: {
      "rift-beast": {
        path: "/assets/meshes/rift-beast.gltf",
        aliases: ["monster", "wolf"],
        category: "creature",
        tags: ["wild", "beast", "combat"]
      },
      "weathered-boulder": {
        path: "/assets/meshes/weathered-boulder.gltf",
        aliases: ["rock", "ore-vein"],
        category: "resource",
        tags: ["stone", "boulder"]
      }
    },
    sprites: {
      "selection-ring": {
        path: "/assets/textures/selection-ring.svg",
        aliases: ["target-ring"],
        category: "ui",
        tags: ["selection", "highlight"]
      }
    }
  });

  test("matches creator assets by explicit aliases", () => {
    expect(resolveManifestMeshAsset(manifest, "wolf")?.path).toBe(
      "/assets/meshes/rift-beast.gltf"
    );
    expect(resolveManifestSpriteAsset(manifest, "target-ring")?.path).toBe(
      "/assets/textures/selection-ring.svg"
    );
  });

  test("matches creator assets by semantic token overlap", () => {
    expect(resolveManifestMeshAsset(manifest, "wild-beast")?.path).toBe(
      "/assets/meshes/rift-beast.gltf"
    );
    expect(resolveManifestMeshAsset(manifest, "resource-stone")?.path).toBe(
      "/assets/meshes/weathered-boulder.gltf"
    );
  });
});

describe("ManifestBackedPodThreeAssetRegistry", () => {
  const manifest = parsePodThreeAssetManifest({
    version: 1,
    meshes: {
      "rift-beast": {
        path: "/assets/meshes/rift-beast.gltf",
        aliases: ["monster"]
      }
    },
    sprites: {
      "selection-ring": {
        path: "/assets/textures/selection-ring.svg",
        ktx2Path: "/assets/textures/selection-ring.ktx2",
        colorSpace: "none",
        repeat: [1.5, 1.5]
      }
    }
  });

  test("loads manifest-backed geometry before falling back", async () => {
    const loadedPaths: string[] = [];
    const registry = new ManifestBackedPodThreeAssetRegistry({
      manifest,
      fallbackRegistry: new DefaultPodThreeAssetRegistry(),
      geometryLoader: {
        async load(path: string) {
          loadedPaths.push(path);
          return new SphereGeometry(1.2, 6, 4);
        }
      },
      textureLoader: {
        async load() {
          return new Texture();
        }
      }
    });

    const geometry = await registry.resolveGeometry(meshBatch({ mesh: "monster" }), 0);
    expect(loadedPaths).toEqual(["/assets/meshes/rift-beast.gltf"]);
    expect(geometry.type).toBe("SphereGeometry");
  });

  test("falls back to procedural geometry for unknown mesh ids", async () => {
    const registry = new ManifestBackedPodThreeAssetRegistry({
      manifest,
      fallbackRegistry: new DefaultPodThreeAssetRegistry(),
      geometryLoader: {
        async load() {
          return new SphereGeometry(1.2, 6, 4);
        }
      },
      textureLoader: {
        async load() {
          return new Texture();
        }
      }
    });

    const geometry = await registry.resolveGeometry(meshBatch({ mesh: "unknown-crate" }), 0);
    expect(geometry.type).toBe("BoxGeometry");
  });

  test("prefers compressed sprite textures when available", async () => {
    const paths: string[] = [];
    const registry = new ManifestBackedPodThreeAssetRegistry({
      manifest,
      fallbackRegistry: new DefaultPodThreeAssetRegistry(),
      geometryLoader: {
        async load() {
          return new BoxGeometry(1, 1, 1);
        }
      },
      textureLoader: {
        async load(path: string) {
          paths.push(`plain:${path}`);
          return new Texture();
        }
      },
      compressedTextureLoader: {
        async load(path: string) {
          paths.push(`ktx2:${path}`);
          return new Texture();
        }
      }
    });

    const resolved = await registry.resolveSpriteTexture(
      { texture: "selection-ring", frame: 0 },
      4
    );

    expect(paths).toEqual(["ktx2:/assets/textures/selection-ring.ktx2"]);
    expect(resolved.repeat).toEqual([1.5, 1.5]);
  });

  test("falls back to plain textures when no compressed loader is configured", async () => {
    const paths: string[] = [];
    const registry = new ManifestBackedPodThreeAssetRegistry({
      manifest,
      fallbackRegistry: new DefaultPodThreeAssetRegistry(),
      geometryLoader: {
        async load() {
          return new BoxGeometry(1, 1, 1);
        }
      },
      textureLoader: {
        async load(path: string, options: { colorSpace: "srgb" | "none" }) {
          paths.push(`${path}:${options.colorSpace}`);
          const texture = new Texture();
          texture.colorSpace = options.colorSpace === "none" ? NoColorSpace : texture.colorSpace;
          return texture;
        }
      }
    });

    const resolved = await registry.resolveSpriteTexture(
      { texture: "selection-ring", frame: 0 },
      2
    );

    expect(paths).toEqual(["/assets/textures/selection-ring.svg:none"]);
    expect(resolved.texture.colorSpace).toBe(NoColorSpace);
  });

  test("keeps a fallback sprite resident after a load failure instead of retrying every frame", async () => {
    let attempts = 0;
    const warnings: unknown[][] = [];
    const originalWarn = console.warn;
    console.warn = (...args: unknown[]) => {
      warnings.push(args);
    };

    const registry = new ManifestBackedPodThreeAssetRegistry({
      manifest,
      fallbackRegistry: new DefaultPodThreeAssetRegistry(),
      geometryLoader: {
        async load() {
          return new BoxGeometry(1, 1, 1);
        }
      },
      textureLoader: {
        async load() {
          attempts += 1;
          throw new Error("worker image decode failed");
        }
      }
    });

    try {
      const first = await registry.resolveSpriteTexture({ texture: "selection-ring", frame: 0 }, 2);
      const second = await registry.resolveSpriteTexture({ texture: "selection-ring", frame: 0 }, 2);

      expect(first.texture).toBe(second.texture);
      expect(attempts).toBe(1);
      expect(warnings).toHaveLength(1);
      expect(registry.getResidencyStats?.()).toEqual({
        residentGeometryAssets: 0,
        residentSpriteAssets: 1,
        pendingGeometryAssets: 0,
        pendingSpriteAssets: 0
      });
    } finally {
      console.warn = originalWarn;
    }
  });

  test("prefetches unique mesh assets in parallel and reports residency", async () => {
    const loadedPaths: string[] = [];
    const registry = new ManifestBackedPodThreeAssetRegistry({
      manifest,
      fallbackRegistry: new DefaultPodThreeAssetRegistry(),
      geometryLoader: {
        async load(path: string) {
          loadedPaths.push(path);
          return new SphereGeometry(1.2, 6, 4);
        }
      },
      textureLoader: {
        async load() {
          return new Texture();
        }
      }
    });

    await registry.prefetchMeshes?.([
      { batch: meshBatch({ mesh: "monster" }), lodLevel: 0 },
      { batch: meshBatch({ mesh: "rift-beast" }), lodLevel: 0 }
    ]);

    expect(loadedPaths).toEqual(["/assets/meshes/rift-beast.gltf"]);
    expect(registry.getResidencyStats?.()).toEqual({
      residentGeometryAssets: 1,
      residentSpriteAssets: 0,
      pendingGeometryAssets: 0,
      pendingSpriteAssets: 0
    });
  });
});
