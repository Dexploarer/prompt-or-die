import { describe, expect, test } from "bun:test";
import { BoxGeometry, Group, Mesh, MeshStandardMaterial, NoColorSpace, SphereGeometry, Texture } from "three";

import {
  createProceduralSpriteTexture,
  createMeshMaterial,
  DefaultPodThreeAssetRegistry,
  extractRenderableGeometry,
  ManifestBackedPodThreeAssetRegistry,
  parsePodThreeAssetManifest,
  resolveMeshRuntimePath,
  resolveManifestMeshAsset,
  resolveManifestSpriteAsset,
  resolveSpriteRuntimePath,
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

async function withMockedPerformanceNow<T>(
  now: () => number,
  run: () => Promise<T> | T
): Promise<T> {
  const originalNow = performance.now;
  Object.defineProperty(performance, "now", {
    configurable: true,
    value: now
  });

  try {
    return await run();
  } finally {
    Object.defineProperty(performance, "now", {
      configurable: true,
      value: originalNow
    });
  }
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
          lods: {
            0: "/assets/meshes/rift-beast.gltf",
            1: "/assets/meshes/rift-beast.lod1.gltf"
          },
          meshoptLods: {
            0: "/assets/meshes/rift-beast.meshopt.glb",
            1: "/assets/meshes/rift-beast.lod1.meshopt.glb"
          },
          runtime: {
            selection: "explicit-lod",
            preferredEncoding: "meshopt",
            variants: {
              0: {
                sizeBytes: 12000,
                triangleCount: 320
              },
              1: {
                sizeBytes: 6000,
                triangleCount: 160
              }
            },
            compressedVariants: {
              0: {
                sizeBytes: 7000,
                triangleCount: 320
              },
              1: {
                sizeBytes: 3600,
                triangleCount: 160
              }
            }
          },
          aliases: ["monster", "wolf"],
          category: "creature",
          tags: ["wild", "beast"]
        }
      },
      sprites: {
        "selection-ring": {
          path: "/assets/textures/selection-ring.svg",
          ktx2Path: "/assets/textures/selection-ring.ktx2",
          runtime: {
            preferredEncoding: "source",
            variants: {
              source: {
                sizeBytes: 592,
                sizeBudgetBytes: 2048
              },
              ktx2: {
                sizeBytes: 1248,
                sizeBudgetBytes: 2048
              }
            }
          },
          colorSpace: "none",
          repeat: [2, 1],
          offset: [0.25, 0]
        }
      }
    });

    expect(manifest.meshes["rift-beast"]?.category).toBe("creature");
    expect(manifest.meshes["rift-beast"]?.runtime?.selection).toBe("explicit-lod");
    expect(manifest.meshes["rift-beast"]?.runtime?.preferredEncoding).toBe("meshopt");
    expect(manifest.meshes["rift-beast"]?.runtime?.variants?.[1]?.triangleCount).toBe(160);
    expect(manifest.meshes["rift-beast"]?.runtime?.compressedVariants?.[0]?.sizeBytes).toBe(7000);
    expect(manifest.meshes["rift-beast"]?.meshoptLods?.[1]).toBe(
      "/assets/meshes/rift-beast.lod1.meshopt.glb"
    );
    expect(manifest.sprites["selection-ring"]?.colorSpace).toBe("none");
    expect(manifest.sprites["selection-ring"]?.runtime?.preferredEncoding).toBe("source");
    expect(manifest.sprites["selection-ring"]?.runtime?.variants?.source?.sizeBytes).toBe(592);
    expect(manifest.sprites["selection-ring"]?.runtime?.variants?.ktx2?.sizeBytes).toBe(1248);
    expect(manifest.sprites["selection-ring"]?.repeat).toEqual([2, 1]);
  });

  test("accepts binary glb mesh paths for runtime-first asset delivery", () => {
    const manifest = parsePodThreeAssetManifest({
      version: 1,
      meshes: {
        "rift-beast": {
          path: "/assets/meshes/rift-beast.glb",
          aliases: ["monster"]
        }
      },
      sprites: {}
    });

    expect(manifest.meshes["rift-beast"]?.path).toBe("/assets/meshes/rift-beast.glb");
  });
});

describe("runtime asset path selection", () => {
  test("uses explicit lod paths when runtime selection is explicit", () => {
    const manifest = parsePodThreeAssetManifest({
      version: 1,
      meshes: {
        "rift-beast": {
          path: "/assets/meshes/rift-beast.glb",
          lods: {
            0: "/assets/meshes/rift-beast.glb",
            1: "/assets/meshes/rift-beast.lod1.glb",
            2: "/assets/meshes/rift-beast.lod2.glb"
          },
          meshoptLods: {
            0: "/assets/meshes/rift-beast.meshopt.glb",
            2: "/assets/meshes/rift-beast.lod2.meshopt.glb"
          },
          runtime: {
            selection: "explicit-lod",
            preferredEncoding: "meshopt"
          }
        }
      },
      sprites: {}
    });

    expect(resolveMeshRuntimePath(manifest.meshes["rift-beast"], 0)).toBe(
      "/assets/meshes/rift-beast.meshopt.glb"
    );
    expect(resolveMeshRuntimePath(manifest.meshes["rift-beast"], 1)).toBe(
      "/assets/meshes/rift-beast.lod1.glb"
    );
    expect(resolveMeshRuntimePath(manifest.meshes["rift-beast"], 2)).toBe(
      "/assets/meshes/rift-beast.lod2.meshopt.glb"
    );
  });

  test("honors explicit sprite encoding preference before falling back", () => {
    const manifest = parsePodThreeAssetManifest({
      version: 1,
      meshes: {},
      sprites: {
        "selection-ring": {
          path: "/assets/textures/selection-ring.svg",
          ktx2Path: "/assets/textures/selection-ring.ktx2",
          runtime: {
            preferredEncoding: "ktx2"
          }
        }
      }
    });

    expect(resolveSpriteRuntimePath(manifest.sprites["selection-ring"], true)).toBe(
      "/assets/textures/selection-ring.ktx2"
    );
    expect(
      resolveSpriteRuntimePath(
        {
          ...manifest.sprites["selection-ring"],
          runtime: { preferredEncoding: "source" }
        },
        false
      )
    ).toBe("/assets/textures/selection-ring.svg");
  });
});

describe("extractRenderableGeometry", () => {
  test("merges multi-mesh gltf/glb scenes into one renderable geometry", () => {
    const root = new Group();
    const base = new Mesh(new BoxGeometry(1, 1, 1), new MeshStandardMaterial());
    const topper = new Mesh(new SphereGeometry(0.5, 8, 6), new MeshStandardMaterial());
    topper.position.set(2.5, 0.5, 0);
    root.add(base);
    root.add(topper);
    root.updateWorldMatrix(true, true);

    const geometry = extractRenderableGeometry(root, "/assets/meshes/multipart.glb");
    geometry.computeBoundingBox();

    expect(geometry.getAttribute("position")?.count).toBeGreaterThan(
      base.geometry.getAttribute("position")?.count ?? 0
    );
    expect(geometry.boundingBox?.max.x ?? 0).toBeGreaterThan(2.9);
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

  test("loads binary glb mesh assets through the manifest path", async () => {
    const loadedPaths: string[] = [];
    const registry = new ManifestBackedPodThreeAssetRegistry({
      manifest: parsePodThreeAssetManifest({
        version: 1,
        meshes: {
          "rift-beast": {
            path: "/assets/meshes/rift-beast.glb",
            aliases: ["monster"]
          }
        },
        sprites: {}
      }),
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
    expect(loadedPaths).toEqual(["/assets/meshes/rift-beast.glb"]);
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
      const stats = registry.getResidencyStats?.();
      expect(stats).toMatchObject({
        residentGeometryAssets: 0,
        residentSpriteAssets: 1,
        pendingGeometryAssets: 0,
        pendingSpriteAssets: 0,
        geometryLoadsCompleted: 0,
        spriteLoadsCompleted: 1
      });
      expect(stats?.averageGeometryLoadMs).toBe(0);
      expect(stats?.slowestGeometryLoadMs).toBe(0);
      expect(stats?.averageSpriteLoadMs ?? -1).toBeGreaterThanOrEqual(0);
      expect(stats?.slowestSpriteLoadMs ?? -1).toBeGreaterThanOrEqual(0);
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
    const stats = registry.getResidencyStats?.();
    expect(stats).toMatchObject({
      residentGeometryAssets: 1,
      residentSpriteAssets: 0,
      pendingGeometryAssets: 0,
      pendingSpriteAssets: 0,
      geometryLoadsCompleted: 1,
      spriteLoadsCompleted: 0
    });
    expect(stats?.averageGeometryLoadMs ?? -1).toBeGreaterThanOrEqual(0);
    expect(stats?.slowestGeometryLoadMs ?? -1).toBeGreaterThanOrEqual(0);
    expect(stats?.averageSpriteLoadMs).toBe(0);
    expect(stats?.slowestSpriteLoadMs).toBe(0);
  });

  test("tracks deterministic geometry and sprite load timing aggregates", async () => {
    let nowMs = 100;
    const registry = new ManifestBackedPodThreeAssetRegistry({
      manifest,
      fallbackRegistry: new DefaultPodThreeAssetRegistry(),
      geometryLoader: {
        async load() {
          nowMs += 7;
          return new SphereGeometry(1.2, 6, 4);
        }
      },
      textureLoader: {
        async load() {
          nowMs += 3;
          return new Texture();
        }
      }
    });

    await withMockedPerformanceNow(() => nowMs, async () => {
      await registry.resolveGeometry(meshBatch({ mesh: "monster" }), 0);
      await registry.resolveSpriteTexture({ texture: "selection-ring", frame: 0 }, 2);
    });

    expect(registry.getResidencyStats?.()).toEqual({
      residentGeometryAssets: 1,
      residentSpriteAssets: 1,
      pendingGeometryAssets: 0,
      pendingSpriteAssets: 0,
      geometryLoadsCompleted: 1,
      spriteLoadsCompleted: 1,
      averageGeometryLoadMs: 7,
      averageSpriteLoadMs: 3,
      slowestGeometryLoadMs: 7,
      slowestSpriteLoadMs: 3
    });
  });
});
