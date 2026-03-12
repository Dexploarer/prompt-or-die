import { describe, expect, test } from "bun:test";

import {
  applyCompressedSpriteVariantsToManifest,
  applyRuntimeVariantsToManifest,
  assertRuntimeBudgetReport,
  buildRuntimeBundleSpec,
  createRasterRingTexturePng,
  createRuntimeBudgetReport
} from "./sync-assets.mjs";

describe("buildRuntimeBundleSpec", () => {
  test("emits optional mesh lod variants and compressed sprite sidecars into the runtime bundle contract", () => {
    const manifest = {
      version: 1,
      meshes: {
        "rift-beast": {
          path: "/assets/meshes/rift-beast.glb"
        }
      },
      sprites: {
        "selection-ring": {
          path: "/assets/textures/selection-ring.png"
        },
        "mist-ring": {
          path: "/assets/textures/mist-ring.png"
        }
      }
    };

    const bundleSpec = buildRuntimeBundleSpec(
      manifest,
      {
        "selection-ring": "/tmp/selection-ring.ktx2"
      },
      {
        "rift-beast": {
          "0": "/tmp/rift-beast.glb",
          "1": "/tmp/rift-beast.lod1.glb",
          "2": "/tmp/rift-beast.lod2.glb"
        }
      },
      {
        "rift-beast": {
          "0": "/tmp/rift-beast.meshopt.glb",
          "2": "/tmp/rift-beast.lod2.meshopt.glb"
        }
      }
    );

    expect(bundleSpec.meshes["rift-beast"]).toEqual({
      source_path: "/tmp/rift-beast.glb",
      runtime_path: "/assets/meshes/rift-beast.glb",
      lod_variants: {
        "1": {
          source_path: "/tmp/rift-beast.lod1.glb",
          runtime_path: "/assets/meshes/rift-beast.lod1.glb"
        },
        "2": {
          source_path: "/tmp/rift-beast.lod2.glb",
          runtime_path: "/assets/meshes/rift-beast.lod2.glb"
        }
      },
      compressed_lod_variants: {
        "0": {
          source_path: "/tmp/rift-beast.meshopt.glb",
          runtime_path: "/assets/meshes/rift-beast.meshopt.glb"
        },
        "2": {
          source_path: "/tmp/rift-beast.lod2.meshopt.glb",
          runtime_path: "/assets/meshes/rift-beast.lod2.meshopt.glb"
        }
      }
    });

    expect(bundleSpec.sprites["selection-ring"]).toEqual({
      source_path: expect.stringContaining("/artifacts/source-assets/textures/selection-ring.png"),
      runtime_path: "/assets/textures/selection-ring.png",
      compressed_variant: {
        source_path: "/tmp/selection-ring.ktx2",
        runtime_path: "/assets/textures/selection-ring.ktx2"
      }
    });
    expect(bundleSpec.sprites["mist-ring"]).toEqual({
      source_path: expect.stringContaining("/artifacts/source-assets/textures/mist-ring.png"),
      runtime_path: "/assets/textures/mist-ring.png"
    });
  });
});

describe("createRasterRingTexturePng", () => {
  test("emits a raster sprite source that is large enough for supercompression to matter", () => {
    const bytes = createRasterRingTexturePng("mist");

    expect(bytes).toBeInstanceOf(Buffer);
    expect(bytes.byteLength).toBeGreaterThan(4096);
  });
});

describe("applyRuntimeVariantsToManifest", () => {
  test("projects mesh lod runtime metadata and sprite budgets into the shipped manifest", () => {
    const manifest = {
      version: 1,
      meshes: {
        "rift-beast": {
          path: "/assets/meshes/rift-beast.glb",
          category: "creature"
        }
      },
      sprites: {
        "selection-ring": {
          path: "/assets/textures/selection-ring.png",
          category: "ui"
        }
      }
    };
    const stagedManifest = {
      meshes: {
        "rift-beast": {
          runtime_path: "/assets/meshes/rift-beast.glb",
          lod_variants: {
            "1": {
              runtime_path: "/assets/meshes/rift-beast.lod1.glb"
            },
            "2": {
              runtime_path: "/assets/meshes/rift-beast.lod2.glb"
            }
          },
          compressed_lod_variants: {
            "0": {
              runtime_path: "/assets/meshes/rift-beast.meshopt.glb"
            },
            "2": {
              runtime_path: "/assets/meshes/rift-beast.lod2.meshopt.glb"
            }
          }
        }
      },
      sprites: {
        "selection-ring": {
          runtime_path: "/assets/textures/selection-ring.png",
          compressed_variant: {
            runtime_path: "/assets/textures/selection-ring.ktx2"
          }
        }
      }
    };

    const projected = applyRuntimeVariantsToManifest(
      manifest,
      stagedManifest,
      {
        "rift-beast": {
          "0": {
            runtimePath: "/assets/meshes/rift-beast.glb",
            sizeBytes: 20000,
            triangleCount: 320,
            estimatedTransferMs: 1.67
          },
          "1": {
            runtimePath: "/assets/meshes/rift-beast.lod1.glb",
            sizeBytes: 12000,
            triangleCount: 160,
            estimatedTransferMs: 1
          },
          "2": {
            runtimePath: "/assets/meshes/rift-beast.lod2.glb",
            sizeBytes: 7000,
            triangleCount: 80,
            estimatedTransferMs: 0.58
          }
        }
      },
      {
        "selection-ring": {
          sizeBytes: 8215,
          estimatedTransferMs: 0.68,
          compressedSizeBytes: 1821,
          compressedEstimatedTransferMs: 0.15
        }
      },
      {
        "rift-beast": {
          "0": {
            runtimePath: "/assets/meshes/rift-beast.meshopt.glb",
            sizeBytes: 12000,
            triangleCount: 320,
            estimatedTransferMs: 1
          },
          "2": {
            runtimePath: "/assets/meshes/rift-beast.lod2.meshopt.glb",
            sizeBytes: 4200,
            triangleCount: 80,
            estimatedTransferMs: 0.35
          }
        }
      }
    );

    expect(projected.meshes["rift-beast"].lods).toEqual({
      "0": "/assets/meshes/rift-beast.glb",
      "1": "/assets/meshes/rift-beast.lod1.glb",
      "2": "/assets/meshes/rift-beast.lod2.glb"
    });
    expect(projected.meshes["rift-beast"].meshoptLods).toEqual({
      "0": "/assets/meshes/rift-beast.meshopt.glb",
      "2": "/assets/meshes/rift-beast.lod2.meshopt.glb"
    });
    expect(projected.meshes["rift-beast"].runtime.selection).toBe("explicit-lod");
    expect(projected.meshes["rift-beast"].runtime.preferredEncoding).toBe("meshopt");
    expect(projected.meshes["rift-beast"].runtime.variants["1"]).toMatchObject({
      sizeBytes: 12000,
      triangleCount: 160,
      sizeBudgetBytes: 20480
    });
    expect(projected.meshes["rift-beast"].runtime.compressedVariants["0"]).toMatchObject({
      runtimePath: "/assets/meshes/rift-beast.meshopt.glb",
      sizeBytes: 12000,
      sizeBudgetBytes: 32768
    });
    expect(projected.sprites["selection-ring"].runtime.preferredEncoding).toBe("ktx2");
    expect(projected.sprites["selection-ring"].runtime.variants.source).toMatchObject({
      sizeBytes: 8215,
      sizeBudgetBytes: 10240
    });
    expect(projected.sprites["selection-ring"].runtime.variants.ktx2).toMatchObject({
      runtimePath: "/assets/textures/selection-ring.ktx2",
      sizeBytes: 1821,
      sizeBudgetBytes: 2048
    });
  });
});

describe("runtime budget report", () => {
  test("captures per-variant sizes and validates monotonic lod reductions", () => {
    const report = createRuntimeBudgetReport({
      version: 1,
      meshes: {
        "rift-beast": {
          path: "/assets/meshes/rift-beast.glb",
          runtime: {
            selection: "explicit-lod",
            preferredEncoding: "meshopt",
            variants: {
              "0": { sizeBytes: 20000, sizeBudgetBytes: 32768, estimatedTransferMs: 1.67 },
              "1": { sizeBytes: 12000, sizeBudgetBytes: 20480, estimatedTransferMs: 1 },
              "2": { sizeBytes: 7000, sizeBudgetBytes: 12288, estimatedTransferMs: 0.58 }
            },
            compressedVariants: {
              "0": { sizeBytes: 12000, sizeBudgetBytes: 32768, estimatedTransferMs: 1 },
              "2": { sizeBytes: 4200, sizeBudgetBytes: 12288, estimatedTransferMs: 0.35 }
            }
          }
        }
      },
      sprites: {
        "selection-ring": {
          path: "/assets/textures/selection-ring.png",
          runtime: {
            preferredEncoding: "ktx2",
            variants: {
              source: { sizeBytes: 8215, sizeBudgetBytes: 10240, estimatedTransferMs: 0.68 },
              ktx2: { sizeBytes: 1821, sizeBudgetBytes: 2048, estimatedTransferMs: 0.15 }
            }
          }
        }
      }
    });

    expect(report.meshes["rift-beast"].totalVariantBytes).toBe(39000);
    expect(report.meshes["rift-beast"].totalCompressedVariantBytes).toBe(16200);
    expect(report.meshes["rift-beast"].preferredEncoding).toBe("meshopt");
    expect(report.sprites["selection-ring"].totalVariantBytes).toBe(10036);
    expect(() => assertRuntimeBudgetReport(report)).not.toThrow();
  });
});

describe("applyCompressedSpriteVariantsToManifest", () => {
  test("projects staged compressed sprite runtime paths into manifest ktx2Path entries", () => {
    const manifest = {
      version: 1,
      meshes: {},
      sprites: {
        "selection-ring": {
          path: "/assets/textures/selection-ring.png",
          aliases: ["target-ring"]
        },
        "mist-ring": {
          path: "/assets/textures/mist-ring.png"
        }
      }
    };
    const stagedManifest = {
      sprites: {
        "selection-ring": {
          runtime_path: "/assets/textures/selection-ring.png",
          compressed_variant: {
            runtime_path: "/assets/textures/selection-ring.ktx2"
          }
        },
        "mist-ring": {
          runtime_path: "/assets/textures/mist-ring.png"
        }
      }
    };

    const projected = applyCompressedSpriteVariantsToManifest(manifest, stagedManifest);

    expect(projected).not.toBe(manifest);
    expect(projected.sprites["selection-ring"]).toEqual({
      path: "/assets/textures/selection-ring.png",
      aliases: ["target-ring"],
      ktx2Path: "/assets/textures/selection-ring.ktx2"
    });
    expect(projected.sprites["mist-ring"]).toEqual({
      path: "/assets/textures/mist-ring.png"
    });
    expect(manifest.sprites["selection-ring"]).toEqual({
      path: "/assets/textures/selection-ring.png",
      aliases: ["target-ring"]
    });
  });

  test("ignores staged compressed sprite records that do not map to app manifest entries", () => {
    const manifest = {
      version: 1,
      meshes: {},
      sprites: {
        "danger-ring": {
          path: "/assets/textures/danger-ring.png"
        }
      }
    };
    const stagedManifest = {
      sprites: {
        "selection-ring": {
          runtime_path: "/assets/textures/selection-ring.png",
          compressed_variant: {
            runtime_path: "/assets/textures/selection-ring.ktx2"
          }
        }
      }
    };

    const projected = applyCompressedSpriteVariantsToManifest(manifest, stagedManifest);

    expect(projected.sprites["danger-ring"]).toEqual({
      path: "/assets/textures/danger-ring.png"
    });
  });
});
