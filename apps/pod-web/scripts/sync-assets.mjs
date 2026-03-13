import { access, copyFile, mkdir, stat, writeFile } from "node:fs/promises";
import { execFile } from "node:child_process";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { deflateSync } from "node:zlib";

import * as THREE from "three";
import { GLTFExporter } from "three/examples/jsm/exporters/GLTFExporter.js";
import { mergeGeometries } from "three/examples/jsm/utils/BufferGeometryUtils.js";

if (typeof globalThis.FileReader === "undefined") {
  globalThis.FileReader = class FileReader {
    result = null;
    onloadend = null;
    onerror = null;

    async readAsArrayBuffer(blob) {
      try {
        this.result = await blob.arrayBuffer();
        this.onloadend?.();
      } catch (error) {
        this.onerror?.(error);
      }
    }

    async readAsDataURL(blob) {
      try {
        const buffer = Buffer.from(await blob.arrayBuffer());
        const mimeType = blob.type || "application/octet-stream";
        this.result = `data:${mimeType};base64,${buffer.toString("base64")}`;
        this.onloadend?.();
      } catch (error) {
        this.onerror?.(error);
      }
    }
  };
}

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appRoot = resolve(scriptDir, "..");
const repoRoot = resolve(appRoot, "..", "..");
const artifactsRoot = join(appRoot, "artifacts");
const fixtureMeshesRoot = join(appRoot, "fixtures", "meshes");
const fixtureTexturesRoot = join(appRoot, "fixtures", "textures");
const sourceAssetsRoot = join(artifactsRoot, "source-assets");
const sourceMeshesRoot = join(sourceAssetsRoot, "meshes");
const sourceTexturesRoot = join(sourceAssetsRoot, "textures");
const stagedAssetsRoot = join(artifactsRoot, "staged-assets");
const publicAssetsRoot = join(appRoot, "public", "assets");
const basisRoot = join(publicAssetsRoot, "basis");
const threeRoot = join(appRoot, "node_modules", "three", "examples", "jsm");
const execFileAsync = promisify(execFile);
const exporterNormalWarning =
  "THREE.GLTFExporter: Creating normalized normal attribute from the non-normalized one.";

const meshDefinitions = {
  "adventurer-avatar": () => createAdventurerAvatarGeometry(),
  "adventurer-hero": () => createAdventurerHeroGeometry(),
  "basalt-column": () => createBasaltColumnGeometry(),
  "canopy-tree": () => createCanopyTreeGeometry(),
  "glass-spire": () => createGlassSpireGeometry(),
  "rift-beast": () => createRiftBeastGeometry(),
  "spirit-companion": () => createSpiritCompanionGeometry(),
  "supply-crate": () => createSupplyCrateGeometry(),
  "weathered-boulder": () => createWeatheredBoulderGeometry()
};

const ringSpriteDefinitions = {
  "danger-ring": {
    variant: "danger"
  },
  "mist-ring": {
    variant: "mist"
  },
  "selection-ring": {
    variant: "selection"
  }
};

function createBaseManifest() {
  return {
    version: 1,
    meshes: {
      "adventurer-avatar": {
        path: "/assets/meshes/adventurer-avatar.glb",
        aliases: ["adventurer", "npc", "player", "traveler"],
        category: "character",
        tags: ["humanoid", "avatar", "npc"]
      },
      "adventurer-hero": {
        path: "/assets/meshes/adventurer-hero.glb",
        aliases: ["hero", "controlled-player", "main-player"],
        category: "character",
        tags: ["humanoid", "avatar", "hero"]
      },
      "basalt-column": {
        path: "/assets/meshes/basalt-column.glb",
        aliases: ["column", "pillar", "wall", "obsidian-wall"],
        category: "structure",
        tags: ["basalt", "stone", "structure"]
      },
      "canopy-tree": {
        path: "/assets/meshes/canopy-tree.glb",
        aliases: ["tree", "pine-tree", "forest-resource"],
        category: "flora",
        tags: ["woodcutting", "forest", "resource"]
      },
      "glass-spire": {
        path: "/assets/meshes/glass-spire.glb",
        aliases: ["spire", "crystal", "obelisk"],
        category: "structure",
        tags: ["glass", "tower", "magic"]
      },
      "rift-beast": {
        path: "/assets/meshes/rift-beast.glb",
        aliases: ["monster", "creature", "beast", "wolf"],
        category: "creature",
        tags: ["wild", "combat", "enemy"]
      },
      "spirit-companion": {
        path: "/assets/meshes/spirit-companion.glb",
        aliases: ["companion", "pet", "summon", "spirit"],
        category: "companion",
        tags: ["ally", "summon", "creature"]
      },
      "supply-crate": {
        path: "/assets/meshes/supply-crate.glb",
        aliases: ["crate", "cache", "loot", "chest"],
        category: "loot",
        tags: ["container", "supply", "reward"]
      },
      "weathered-boulder": {
        path: "/assets/meshes/weathered-boulder.glb",
        aliases: ["rock", "boulder", "ore-vein", "resource-stone"],
        category: "resource",
        tags: ["stone", "ore", "resource"]
      }
    },
    sprites: {
      "danger-ring": {
        path: "/assets/textures/danger-ring.png",
        aliases: ["critical-ring", "hostile-ring"],
        category: "ui",
        tags: ["danger", "warning", "ground-ring"],
        colorSpace: "srgb"
      },
      "mist-ring": {
        path: "/assets/textures/mist-ring.png",
        aliases: ["fog-ring", "shimmer-ring"],
        category: "effect",
        tags: ["mist", "magic", "atmosphere"],
        colorSpace: "srgb"
      },
      "selection-ring": {
        path: "/assets/textures/selection-ring.png",
        aliases: ["target-ring", "focus-ring"],
        category: "ui",
        tags: ["selection", "focus", "ground-ring"],
        colorSpace: "srgb"
      }
    }
  };
}

function buildCompressedRuntimeTexturePath(path) {
  const lastDotIndex = path.lastIndexOf(".");
  if (lastDotIndex < 0) {
    return `${path}.ktx2`;
  }
  return `${path.slice(0, lastDotIndex)}.ktx2`;
}

function buildMeshLodRuntimePath(path, lodLevel) {
  if (lodLevel === 0) {
    return path;
  }
  const lastDotIndex = path.lastIndexOf(".");
  if (lastDotIndex < 0) {
    return `${path}.lod${lodLevel}`;
  }
  return `${path.slice(0, lastDotIndex)}.lod${lodLevel}${path.slice(lastDotIndex)}`;
}

function buildCompressedMeshRuntimePath(path, lodLevel) {
  const lodPath = buildMeshLodRuntimePath(path, lodLevel);
  const lastDotIndex = lodPath.lastIndexOf(".");
  if (lastDotIndex < 0) {
    return `${lodPath}.meshopt`;
  }
  return `${lodPath.slice(0, lastDotIndex)}.meshopt${lodPath.slice(lastDotIndex)}`;
}

const meshLodSizeBudgets = {
  character: { 0: 57_344, 1: 28_672, 2: 16_384 },
  companion: { 0: 32_768, 1: 20_480, 2: 12_288 },
  creature: { 0: 32_768, 1: 20_480, 2: 12_288 },
  flora: { 0: 20_480, 1: 8_192, 2: 6_144 },
  loot: { 0: 12_288, 1: 7_168, 2: 5_120 },
  resource: { 0: 18_432, 1: 12_288, 2: 7_168 },
  structure: { 0: 24_576, 1: 11_264, 2: 7_168 }
};

const spriteSizeBudgets = {
  effect: {
    source: 10_240,
    ktx2: 2_048
  },
  ui: {
    source: 10_240,
    ktx2: 2_048
  }
};

function estimateTransferMs(sizeBytes) {
  return Number(((sizeBytes / 12_000) * 100).toFixed(2)) / 100;
}

function triangleCountForGeometry(geometry) {
  if (geometry.index) {
    return Math.floor(geometry.index.count / 3);
  }
  const position = geometry.getAttribute("position");
  return position ? Math.floor(position.count / 3) : 0;
}

function lodTriangleStride(lodLevel) {
  return lodLevel === 0 ? 1 : lodLevel === 1 ? 2 : 4;
}

function simplifyGeometryForLod(sourceGeometry, lodLevel) {
  if (lodLevel === 0) {
    return sourceGeometry.clone();
  }

  const normalized = sourceGeometry.index
    ? sourceGeometry.toNonIndexed()
    : sourceGeometry.clone();
  const position = normalized.getAttribute("position");
  if (!position) {
    return normalized;
  }

  const triangleCount = Math.max(Math.floor(position.count / 3), 1);
  const selectedTriangles = [];
  for (let triangleIndex = 0; triangleIndex < triangleCount; triangleIndex += lodTriangleStride(lodLevel)) {
    selectedTriangles.push(triangleIndex);
  }
  if (selectedTriangles.length === 0) {
    selectedTriangles.push(0);
  }

  const simplified = new THREE.BufferGeometry();
  for (const [attributeName, attribute] of Object.entries(normalized.attributes)) {
    const componentsPerTriangle = attribute.itemSize * 3;
    const OutputArray = attribute.array.constructor;
    const values = new OutputArray(selectedTriangles.length * componentsPerTriangle);
    let outputOffset = 0;

    for (const triangleIndex of selectedTriangles) {
      const inputStart = triangleIndex * componentsPerTriangle;
      const inputEnd = inputStart + componentsPerTriangle;
      values.set(attribute.array.subarray(inputStart, inputEnd), outputOffset);
      outputOffset += componentsPerTriangle;
    }

    simplified.setAttribute(
      attributeName,
      new THREE.BufferAttribute(values, attribute.itemSize, attribute.normalized)
    );
  }

  normalized.dispose();
  simplified.computeVertexNormals();
  simplified.normalizeNormals();
  simplified.computeBoundingBox();
  simplified.computeBoundingSphere();
  return simplified;
}

async function createMeshArtifacts(assetId, createGeometry, runtimePath) {
  const sourceGeometry = createGeometry();
  sourceGeometry.computeVertexNormals();
  sourceGeometry.normalizeNormals();

  const sourcePaths = [];
  const lodSourcePaths = {};
  const lodStats = {};

  for (const lodLevel of [0, 1, 2]) {
    const lodGeometry = simplifyGeometryForLod(sourceGeometry, lodLevel);
    const scene = new THREE.Scene();
    const mesh = new THREE.Mesh(
      lodGeometry,
      new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.8, metalness: 0.08 })
    );
    scene.add(mesh);

    const glb = await exportScene(scene, { binary: true });
    const runtimeVariantPath = buildMeshLodRuntimePath(runtimePath, lodLevel);
    const sourcePath = join(sourceMeshesRoot, runtimeVariantPath.split("/").pop());
    await writeFile(sourcePath, new Uint8Array(glb));
    sourcePaths.push(sourcePath);
    lodSourcePaths[String(lodLevel)] = sourcePath;
    lodStats[String(lodLevel)] = {
      runtimePath: runtimeVariantPath,
      sizeBytes: glb.byteLength,
      triangleCount: triangleCountForGeometry(lodGeometry),
      estimatedTransferMs: estimateTransferMs(glb.byteLength)
    };

    lodGeometry.dispose();
  }

  const sourceScene = new THREE.Scene();
  sourceScene.add(
    new THREE.Mesh(
      sourceGeometry.clone(),
      new THREE.MeshStandardMaterial({ color: 0xffffff, roughness: 0.8, metalness: 0.08 })
    )
  );
  const gltf = await exportScene(sourceScene, { binary: false });
  await writeFile(join(sourceMeshesRoot, `${assetId}.gltf`), `${JSON.stringify(gltf, null, 2)}\n`);
  sourceGeometry.dispose();

  return {
    sourcePaths,
    lodSourcePaths,
    lodStats
  };
}

async function copyAuthoredMeshFixtures(meshLodStats, meshLodVariantSources, manifest) {
  await mkdir(fixtureMeshesRoot, { recursive: true });

  const authoredMeshOverrideIds = new Set();

  await Promise.all(
    Object.keys(manifest.meshes).map(async (assetId) => {
      const runtimePath = manifest.meshes[assetId]?.path;
      if (typeof runtimePath !== "string") {
        return;
      }

      for (const level of ["0", "1", "2"]) {
        const fixtureName = level === "0" ? `${assetId}.glb` : `${assetId}.lod${level}.glb`;
        const fixturePath = join(fixtureMeshesRoot, fixtureName);
        try {
          await access(fixturePath);
        } catch {
          continue;
        }

        const sourcePath = join(sourceMeshesRoot, fixtureName);
        await copyFile(fixturePath, sourcePath);
        const metadata = await stat(sourcePath);
        meshLodVariantSources[assetId] ??= {};
        meshLodVariantSources[assetId][level] = sourcePath;
        meshLodStats[assetId] ??= {};
        meshLodStats[assetId][level] = {
          runtimePath: buildMeshLodRuntimePath(runtimePath, Number(level)),
          sizeBytes: metadata.size,
          triangleCount: meshLodStats[assetId]?.[level]?.triangleCount ?? 0,
          estimatedTransferMs: estimateTransferMs(metadata.size)
        };
        authoredMeshOverrideIds.add(assetId);
      }
    })
  );

  return authoredMeshOverrideIds;
}

async function writeTextureArtifact(assetId, contents, stagedPaths) {
  const sourcePath = join(sourceTexturesRoot, `${assetId}.png`);
  await writeFile(sourcePath, contents);
  stagedPaths.push(sourcePath);
  return {
    sourcePath,
    runtimePath: `/assets/textures/${assetId}.png`,
    sizeBytes: Buffer.byteLength(contents),
    estimatedTransferMs: estimateTransferMs(Buffer.byteLength(contents))
  };
}

export function createRuntimeBudgetReport(manifest) {
  const meshes = Object.fromEntries(
    Object.entries(manifest.meshes).map(([assetId, descriptor]) => {
      const variants = descriptor.runtime?.variants ?? {};
      const compressedVariants = descriptor.runtime?.compressedVariants ?? {};
      const totalVariantBytes = Object.values(variants).reduce(
        (total, variant) => total + (variant?.sizeBytes ?? 0),
        0
      );
      const totalCompressedVariantBytes = Object.values(compressedVariants).reduce(
        (total, variant) => total + (variant?.sizeBytes ?? 0),
        0
      );
      return [
        assetId,
        {
          totalVariantBytes,
          totalCompressedVariantBytes,
          selection: descriptor.runtime?.selection ?? "base",
          preferredEncoding: descriptor.runtime?.preferredEncoding ?? "source",
          variants,
          compressedVariants
        }
      ];
    })
  );
  const sprites = Object.fromEntries(
    Object.entries(manifest.sprites).map(([assetId, descriptor]) => {
      const variants = descriptor.runtime?.variants ?? {};
      const totalVariantBytes = Object.values(variants).reduce(
        (total, variant) => total + (variant?.sizeBytes ?? 0),
        0
      );
      return [
        assetId,
        {
          totalVariantBytes,
          preferredEncoding: descriptor.runtime?.preferredEncoding ?? "auto",
          variants
        }
      ];
    })
  );

  return {
    version: 1,
    meshes,
    sprites
  };
}

export function assertRuntimeBudgetReport(report) {
  for (const [assetId, mesh] of Object.entries(report.meshes)) {
    const assertVariantSet = (variantEntries, label) => {
      for (const [level, variant] of variantEntries) {
        if ((variant.sizeBytes ?? 0) > (variant.sizeBudgetBytes ?? 0)) {
          throw new Error(
            `Mesh asset ${assetId} ${label} lod ${level} exceeds budget ${variant.sizeBudgetBytes} with ${variant.sizeBytes} bytes`
          );
        }
      }
      for (let index = 1; index < variantEntries.length; index += 1) {
        const [, previous] = variantEntries[index - 1];
        const [, current] = variantEntries[index];
        if ((current.sizeBytes ?? 0) > (previous.sizeBytes ?? 0)) {
          throw new Error(
            `Mesh asset ${assetId} ${label} lod sizes must decrease monotonically, got ${previous.sizeBytes} then ${current.sizeBytes}`
          );
        }
        if ((current.estimatedTransferMs ?? 0) > (previous.estimatedTransferMs ?? 0)) {
          throw new Error(
            `Mesh asset ${assetId} ${label} estimated transfer must decrease monotonically, got ${previous.estimatedTransferMs} then ${current.estimatedTransferMs}`
          );
        }
      }
    };

    const variantEntries = Object.entries(mesh.variants);
    assertVariantSet(variantEntries, "source");

    const compressedVariantEntries = Object.entries(mesh.compressedVariants ?? {});
    if (compressedVariantEntries.length > 0) {
      assertVariantSet(compressedVariantEntries, "meshopt");
    }

    if (
      mesh.preferredEncoding === "meshopt" &&
      compressedVariantEntries.length > 0 &&
      (mesh.totalCompressedVariantBytes ?? Number.POSITIVE_INFINITY) >
        (mesh.totalVariantBytes ?? Number.POSITIVE_INFINITY)
    ) {
      throw new Error(
        `Mesh asset ${assetId} prefers meshopt despite larger compressed set ${mesh.totalCompressedVariantBytes} > ${mesh.totalVariantBytes}`
      );
    }
  }

  for (const [assetId, sprite] of Object.entries(report.sprites)) {
    for (const variant of Object.values(sprite.variants)) {
      if ((variant.sizeBytes ?? 0) > (variant.sizeBudgetBytes ?? 0)) {
        throw new Error(
          `Sprite asset ${assetId} exceeds budget ${variant.sizeBudgetBytes} with ${variant.sizeBytes} bytes`
        );
      }
    }
  }
}

export function buildRuntimeBundleSpec(
  manifest,
  compressedSpriteVariantSources = {},
  meshLodVariantSources = {},
  compressedMeshVariantSources = {}
) {
  return {
    output_roots: {
      source: relative(appRoot, sourceAssetsRoot),
      staged: relative(appRoot, stagedAssetsRoot),
      runtime_public: relative(appRoot, publicAssetsRoot)
    },
    meshes: Object.fromEntries(
      Object.keys(manifest.meshes).map((assetId) => [
        assetId,
        {
          source_path: resolve(meshLodVariantSources[assetId]?.["0"] ?? join(sourceMeshesRoot, `${assetId}.glb`)),
          runtime_path: manifest.meshes[assetId].path,
          lod_variants: Object.fromEntries(
            Object.entries(meshLodVariantSources[assetId] ?? {})
              .filter(([level]) => level !== "0")
              .map(([level, sourcePath]) => [
                level,
                {
                  source_path: resolve(sourcePath),
                  runtime_path: buildMeshLodRuntimePath(manifest.meshes[assetId].path, Number(level))
                }
              ])
          ),
          compressed_lod_variants: Object.fromEntries(
            Object.entries(compressedMeshVariantSources[assetId] ?? {}).map(([level, sourcePath]) => [
              level,
              {
                source_path: resolve(sourcePath),
                runtime_path: buildCompressedMeshRuntimePath(
                  manifest.meshes[assetId].path,
                  Number(level)
                )
              }
            ])
          )
        }
      ])
    ),
    sprites: Object.fromEntries(
      Object.keys(manifest.sprites).map((assetId) => [
        assetId,
        {
          source_path: resolve(sourceTexturesRoot, `${assetId}.png`),
          runtime_path: manifest.sprites[assetId].path,
          ...(typeof compressedSpriteVariantSources[assetId] === "string"
            ? {
                compressed_variant: {
                  source_path: resolve(compressedSpriteVariantSources[assetId]),
                  runtime_path: buildCompressedRuntimeTexturePath(
                    manifest.sprites[assetId].path
                  )
                }
              }
            : {})
        }
      ])
    )
  };
}

export function applyCompressedSpriteVariantsToManifest(manifest, stagedManifest) {
  if (!manifest?.sprites || !stagedManifest?.sprites) {
    return manifest;
  }

  return {
    ...manifest,
    sprites: Object.fromEntries(
      Object.entries(manifest.sprites).map(([assetId, descriptor]) => {
        const ktx2Path = stagedManifest.sprites[assetId]?.compressed_variant?.runtime_path;
        return [
          assetId,
          typeof ktx2Path === "string" ? { ...descriptor, ktx2Path } : { ...descriptor }
        ];
      })
    )
  };
}

export function applyCompressedMeshVariantsToManifest(manifest, stagedManifest) {
  if (!manifest?.meshes || !stagedManifest?.meshes) {
    return manifest;
  }

  return {
    ...manifest,
    meshes: Object.fromEntries(
      Object.entries(manifest.meshes).map(([assetId, descriptor]) => {
        const meshoptLods = Object.fromEntries(
          Object.entries(stagedManifest.meshes[assetId]?.compressed_lod_variants ?? {})
            .map(([level, variant]) => [level, variant.runtime_path])
            .filter(([, runtimePath]) => typeof runtimePath === "string")
        );
        return [
          assetId,
          Object.keys(meshoptLods).length > 0
            ? { ...descriptor, meshoptLods }
            : { ...descriptor }
        ];
      })
    )
  };
}

export function filterCompressedMeshVariantRecords(records, excludedAssetIds = new Set()) {
  return Object.fromEntries(
    Object.entries(records).filter(([assetId]) => !excludedAssetIds.has(assetId))
  );
}

function meshVariantSizeBudget(category, lodLevel) {
  return meshLodSizeBudgets[category]?.[lodLevel] ?? 49_152;
}

function spriteVariantSizeBudget(category, encoding = "source") {
  return spriteSizeBudgets[category]?.[encoding] ?? (encoding === "ktx2" ? 4_096 : 16_384);
}

function choosePreferredSpriteEncoding(sourceStats, compressedRuntimePath) {
  if (typeof compressedRuntimePath !== "string") {
    return "source";
  }

  const sourceSizeBytes = sourceStats?.sizeBytes ?? 0;
  const compressedSizeBytes = sourceStats?.compressedSizeBytes ?? Number.POSITIVE_INFINITY;
  return compressedSizeBytes <= sourceSizeBytes ? "ktx2" : "source";
}

function choosePreferredMeshEncoding(runtimeVariants, compressedRuntimeVariants) {
  const compressedEntries = Object.values(compressedRuntimeVariants ?? {});
  if (compressedEntries.length === 0) {
    return "source";
  }

  const sourceTotal = Object.values(runtimeVariants ?? {}).reduce(
    (total, variant) => total + (variant?.sizeBytes ?? 0),
    0
  );
  const compressedTotal = compressedEntries.reduce(
    (total, variant) => total + (variant?.sizeBytes ?? 0),
    0
  );
  return compressedTotal <= sourceTotal ? "meshopt" : "source";
}

export function applyRuntimeVariantsToManifest(
  manifest,
  stagedManifest,
  meshLodStats,
  spriteSourceStats,
  meshCompressedLodStats = {}
) {
  const withCompressedMeshes = applyCompressedMeshVariantsToManifest(manifest, stagedManifest);
  const withCompressedSprites = applyCompressedSpriteVariantsToManifest(
    withCompressedMeshes,
    stagedManifest
  );

  return {
    ...withCompressedSprites,
    meshes: Object.fromEntries(
      Object.entries(withCompressedSprites.meshes).map(([assetId, descriptor]) => {
        const stagedDescriptor = stagedManifest?.meshes?.[assetId] ?? {};
        const runtimeVariants = meshLodStats[assetId] ?? {};
        const compressedRuntimeVariants = Object.fromEntries(
          Object.entries(meshCompressedLodStats[assetId] ?? {}).map(([level, stats]) => [
            level,
            {
              ...stats,
              sizeBudgetBytes: meshVariantSizeBudget(descriptor.category, Number(level))
            }
          ])
        );
        const preferredEncoding = choosePreferredMeshEncoding(
          runtimeVariants,
          compressedRuntimeVariants
        );
        const lods = Object.fromEntries(
          Object.entries({
            0: stagedDescriptor.runtime_path ?? descriptor.path,
            ...(Object.fromEntries(
              Object.entries(stagedDescriptor.lod_variants ?? {}).map(([level, variant]) => [
                level,
                variant.runtime_path
              ])
            ))
          }).filter(([, value]) => typeof value === "string")
        );
        const meshoptLods = Object.fromEntries(
          Object.entries(stagedDescriptor.compressed_lod_variants ?? {})
            .map(([level, variant]) => [level, variant.runtime_path])
            .filter(([, value]) => typeof value === "string")
        );

        return [
          assetId,
          {
            ...descriptor,
            lods,
            ...(Object.keys(meshoptLods).length > 0 ? { meshoptLods } : {}),
            runtime: {
              selection: "explicit-lod",
              preferredEncoding,
              variants: Object.fromEntries(
                Object.entries(runtimeVariants).map(([level, stats]) => [
                  level,
                  {
                    ...stats,
                    sizeBudgetBytes: meshVariantSizeBudget(descriptor.category, Number(level))
                    }
                ])
              ),
              ...(Object.keys(compressedRuntimeVariants).length > 0
                ? { compressedVariants: compressedRuntimeVariants }
                : {})
            }
          }
        ];
      })
    ),
    sprites: Object.fromEntries(
      Object.entries(withCompressedSprites.sprites).map(([assetId, descriptor]) => {
        const stagedDescriptor = stagedManifest?.sprites?.[assetId] ?? {};
        const sourceStats = spriteSourceStats[assetId];
        const compressedRuntimePath = stagedDescriptor.compressed_variant?.runtime_path;
        const preferredEncoding = choosePreferredSpriteEncoding(
          sourceStats,
          compressedRuntimePath
        );
        return [
          assetId,
          {
            ...descriptor,
            runtime: {
              preferredEncoding,
              variants: {
                source: {
                  runtimePath: descriptor.path,
                  sizeBytes: sourceStats?.sizeBytes ?? 0,
                  estimatedTransferMs: sourceStats?.estimatedTransferMs ?? 0,
                  sizeBudgetBytes: spriteVariantSizeBudget(descriptor.category, "source")
                },
                ...(typeof compressedRuntimePath === "string"
                  ? {
                      ktx2: {
                        runtimePath: compressedRuntimePath,
                        sizeBytes: sourceStats?.compressedSizeBytes ?? 0,
                        estimatedTransferMs: sourceStats?.compressedEstimatedTransferMs ?? 0,
                        sizeBudgetBytes: spriteVariantSizeBudget(descriptor.category, "ktx2")
                      }
                    }
                  : {})
              }
            }
          }
        ];
      })
    )
  };
}

export async function synchronizeSampleAssets() {
  await mkdir(sourceMeshesRoot, { recursive: true });
  await mkdir(sourceTexturesRoot, { recursive: true });
  await mkdir(stagedAssetsRoot, { recursive: true });
  await mkdir(publicAssetsRoot, { recursive: true });
  await mkdir(basisRoot, { recursive: true });

  const originalConsoleWarn = console.warn;
  console.warn = (...args) => {
    if (args[0] === exporterNormalWarning) {
      return;
    }
    originalConsoleWarn(...args);
  };

  try {
    const baseManifest = createBaseManifest();
    const sourceMeshPaths = [];
    const meshLodVariantSources = {};
    const meshLodStats = {};
    for (const [assetId, createGeometry] of Object.entries(meshDefinitions)) {
      const runtimePath = baseManifest.meshes[assetId].path;
      const artifacts = await createMeshArtifacts(assetId, createGeometry, runtimePath);
      sourceMeshPaths.push(...artifacts.sourcePaths);
      meshLodVariantSources[assetId] = artifacts.lodSourcePaths;
      meshLodStats[assetId] = artifacts.lodStats;
    }
    const authoredMeshOverrideIds = await copyAuthoredMeshFixtures(
      meshLodStats,
      meshLodVariantSources,
      baseManifest
    );
    const authoredCompressedMeshFixtures = await copyAuthoredCompressedMeshFixtures(
      meshLodStats,
      baseManifest
    );
    const compressedMeshVariantSources = filterCompressedMeshVariantRecords(
      authoredCompressedMeshFixtures.compressedMeshVariantSources,
      authoredMeshOverrideIds
    );
    const compressedMeshLodStats = filterCompressedMeshVariantRecords(
      authoredCompressedMeshFixtures.compressedMeshLodStats,
      authoredMeshOverrideIds
    );
    sourceMeshPaths.push(
      ...Object.values(compressedMeshVariantSources).flatMap((variants) =>
        Object.values(variants)
      )
    );

    const sourceTexturePaths = [];
    const spriteSourceStats = {};
    for (const [assetId, { variant }] of Object.entries(ringSpriteDefinitions)) {
      spriteSourceStats[assetId] = await writeTextureArtifact(
        assetId,
        createRasterRingTexturePng(variant),
        sourceTexturePaths
      );
    }
    await copyAuthoredCompressedSpriteFixtures(baseManifest);
    const compressedSpriteVariantSources = await discoverCompressedSpriteVariantSources(baseManifest);
    await enrichSpriteStatsWithCompressedVariants(
      spriteSourceStats,
      compressedSpriteVariantSources
    );
    sourceTexturePaths.push(...Object.values(compressedSpriteVariantSources));
    const bundleSpecPath = join(stagedAssetsRoot, "pod-runtime-bundle-spec.json");
    const bundleSpec = buildRuntimeBundleSpec(
      baseManifest,
      compressedSpriteVariantSources,
      meshLodVariantSources,
      compressedMeshVariantSources
    );
    await writeFile(bundleSpecPath, `${JSON.stringify(bundleSpec, null, 2)}\n`);

    const { bundleManifest: stagedManifest } = await stageImports(
      [...sourceMeshPaths, ...sourceTexturePaths],
      bundleSpecPath
    );
    if (!stagedManifest) {
      throw new Error("pod-assets stage_import did not return a runtime bundle manifest");
    }

    await writeFile(
      join(stagedAssetsRoot, "pod-staged-asset-manifest.json"),
      `${JSON.stringify(stagedManifest, null, 2)}\n`
    );

    const manifest = applyRuntimeVariantsToManifest(
      baseManifest,
      stagedManifest,
      meshLodStats,
      spriteSourceStats,
      compressedMeshLodStats
    );
    await writeFile(
      join(publicAssetsRoot, "pod-asset-manifest.json"),
      `${JSON.stringify(manifest, null, 2)}\n`
    );
    const budgetReport = createRuntimeBudgetReport(manifest);
    assertRuntimeBudgetReport(budgetReport);
    await writeFile(
      join(stagedAssetsRoot, "pod-runtime-budget-report.json"),
      `${JSON.stringify(budgetReport, null, 2)}\n`
    );

    await copyFile(
      join(threeRoot, "libs", "basis", "basis_transcoder.js"),
      join(basisRoot, "basis_transcoder.js")
    );
    await copyFile(
      join(threeRoot, "libs", "basis", "basis_transcoder.wasm"),
      join(basisRoot, "basis_transcoder.wasm")
    );

    console.log("Synchronized pod-web sample assets and staged import manifest");
  } finally {
    console.warn = originalConsoleWarn;
  }
}

const invokedPath = process.argv[1] ? resolve(process.argv[1]) : null;
if (invokedPath === fileURLToPath(import.meta.url)) {
  await synchronizeSampleAssets();
}

function exportScene(scene, { binary }) {
  return new Promise((resolveExport, rejectExport) => {
    const exporter = new GLTFExporter();
    exporter.parse(
      scene,
      (result) => {
        if (binary) {
          if (!(result instanceof ArrayBuffer)) {
            rejectExport(new Error("Expected binary glTF export for pod-web sample assets"));
            return;
          }
          resolveExport(result);
          return;
        }
        if (result instanceof ArrayBuffer) {
          rejectExport(new Error("Expected JSON glTF export for pod-web sample assets"));
          return;
        }
        resolveExport(result);
      },
      rejectExport,
      {
        binary,
        onlyVisible: true
      }
    );
  });
}

async function writeTexture(fileName, contents, stagedPaths) {
  const sourcePath = join(sourceTexturesRoot, fileName);
  await writeFile(sourcePath, contents);
  stagedPaths.push(sourcePath);
}

async function stageImports(sourcePaths, bundleSpecPath) {
  const { stdout, stderr } = await execFileAsync(
    "cargo",
    [
      "run",
      "-q",
      "-p",
      "pod-assets",
      "--example",
      "stage_import",
      "--",
      "--json",
      "--materialize-runtime",
      "--output-root",
      stagedAssetsRoot,
      "--base-dir",
      appRoot,
      "--bundle-spec",
      bundleSpecPath,
      ...sourcePaths
    ],
    {
      cwd: repoRoot
    }
  );

  if (stderr?.trim()) {
    console.warn(stderr.trim());
  }

  const parsed = JSON.parse(stdout);
  if (!parsed || !Array.isArray(parsed.imports)) {
    throw new Error("pod-assets stage_import returned an invalid JSON payload");
  }
  return parsed;
}

async function discoverCompressedSpriteVariantSources(manifest) {
  const entries = await Promise.all(
    Object.keys(manifest.sprites).map(async (assetId) => {
      const sourcePath = join(sourceTexturesRoot, `${assetId}.ktx2`);
      try {
        await access(sourcePath);
        return [assetId, sourcePath];
      } catch {
        return null;
      }
    })
  );

  return Object.fromEntries(entries.filter((entry) => entry !== null));
}

async function copyAuthoredCompressedMeshFixtures(meshLodStats, manifest) {
  await mkdir(fixtureMeshesRoot, { recursive: true });

  const compressedMeshVariantSources = {};
  const compressedMeshLodStats = {};

  await Promise.all(
    Object.keys(manifest.meshes).map(async (assetId) => {
      const runtimePath = manifest.meshes[assetId]?.path;
      if (typeof runtimePath !== "string") {
        return;
      }

      for (const level of Object.keys(meshLodStats[assetId] ?? {})) {
        const fixtureName =
          level === "0" ? `${assetId}.meshopt.glb` : `${assetId}.lod${level}.meshopt.glb`;
        const fixturePath = join(fixtureMeshesRoot, fixtureName);
        try {
          await access(fixturePath);
        } catch {
          continue;
        }

        const sourcePath = join(sourceMeshesRoot, fixtureName);
        await copyFile(fixturePath, sourcePath);
        const metadata = await stat(sourcePath);
        compressedMeshVariantSources[assetId] ??= {};
        compressedMeshVariantSources[assetId][level] = sourcePath;
        compressedMeshLodStats[assetId] ??= {};
        compressedMeshLodStats[assetId][level] = {
          runtimePath: buildCompressedMeshRuntimePath(runtimePath, Number(level)),
          sizeBytes: metadata.size,
          triangleCount: meshLodStats[assetId]?.[level]?.triangleCount ?? 0,
          estimatedTransferMs: estimateTransferMs(metadata.size)
        };
      }
    })
  );

  return {
    compressedMeshVariantSources,
    compressedMeshLodStats
  };
}

async function copyAuthoredCompressedSpriteFixtures(manifest) {
  await mkdir(fixtureTexturesRoot, { recursive: true });

  await Promise.all(
    Object.keys(manifest.sprites).map(async (assetId) => {
      const fixturePath = join(fixtureTexturesRoot, `${assetId}.ktx2`);
      try {
        await access(fixturePath);
      } catch {
        return;
      }

      await copyFile(fixturePath, join(sourceTexturesRoot, `${assetId}.ktx2`));
    })
  );
}

async function enrichSpriteStatsWithCompressedVariants(
  spriteSourceStats,
  compressedSpriteVariantSources
) {
  await Promise.all(
    Object.entries(compressedSpriteVariantSources).map(async ([assetId, sourcePath]) => {
      const metadata = await stat(sourcePath);
      spriteSourceStats[assetId] = {
        ...spriteSourceStats[assetId],
        compressedSizeBytes: metadata.size,
        compressedEstimatedTransferMs: estimateTransferMs(metadata.size)
      };
    })
  );
}

function transformGeometry(
  geometry,
  {
    position = [0, 0, 0],
    rotation = [0, 0, 0],
    scale = [1, 1, 1]
  } = {}
) {
  const clone = geometry.clone();
  clone.scale(scale[0], scale[1], scale[2]);
  clone.rotateX(rotation[0]);
  clone.rotateY(rotation[1]);
  clone.rotateZ(rotation[2]);
  clone.translate(position[0], position[1], position[2]);
  return clone;
}

function mergeParts(parts) {
  const normalizedParts = parts.map((part) =>
    part.index ? part.toNonIndexed() : part
  );
  const merged = mergeGeometries(normalizedParts, false);
  for (const part of normalizedParts) {
    part.dispose();
  }
  if (!merged) {
    throw new Error("Failed to merge generated POD mesh geometry");
  }
  merged.computeVertexNormals();
  merged.normalizeNormals();
  return merged;
}

function createAdventurerAvatarGeometry() {
  return mergeParts([
    transformGeometry(new THREE.CapsuleGeometry(0.13, 0.24, 3, 8), {
      position: [0, 0.02, 0]
    }),
    transformGeometry(new THREE.BoxGeometry(0.34, 0.18, 0.16), {
      position: [0, 0.1, 0]
    }),
    transformGeometry(new THREE.DodecahedronGeometry(0.14, 0), {
      position: [0, 0.36, 0]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.06, 0.07, 0.22, 6), {
      position: [-0.1, -0.26, 0]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.06, 0.07, 0.22, 6), {
      position: [0.1, -0.26, 0]
    }),
    transformGeometry(new THREE.BoxGeometry(0.06, 0.22, 0.07), {
      position: [-0.19, -0.02, 0],
      rotation: [0, 0, 0.26]
    }),
    transformGeometry(new THREE.BoxGeometry(0.06, 0.22, 0.07), {
      position: [0.19, -0.02, 0],
      rotation: [0, 0, -0.26]
    }),
    transformGeometry(new THREE.BoxGeometry(0.14, 0.08, 0.1), {
      position: [0, -0.38, 0.02]
    }),
    transformGeometry(new THREE.BoxGeometry(0.11, 0.16, 0.05), {
      position: [0.16, -0.05, -0.08],
      rotation: [0.1, 0, 0.42]
    })
  ]);
}

function createAdventurerHeroGeometry() {
  return mergeParts([
    transformGeometry(new THREE.CapsuleGeometry(0.15, 0.28, 4, 8), {
      position: [0, 0.03, 0]
    }),
    transformGeometry(new THREE.BoxGeometry(0.42, 0.2, 0.18), {
      position: [0, 0.13, 0]
    }),
    transformGeometry(new THREE.DodecahedronGeometry(0.16, 0), {
      position: [0, 0.41, 0]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.07, 0.08, 0.25, 6), {
      position: [-0.11, -0.29, 0.01]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.07, 0.08, 0.25, 6), {
      position: [0.11, -0.29, 0.01]
    }),
    transformGeometry(new THREE.BoxGeometry(0.07, 0.24, 0.08), {
      position: [-0.23, -0.01, 0],
      rotation: [0, 0, 0.34]
    }),
    transformGeometry(new THREE.BoxGeometry(0.07, 0.24, 0.08), {
      position: [0.23, -0.01, 0],
      rotation: [0, 0, -0.34]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.03, 0.18, 0.56, 4), {
      position: [0, -0.02, -0.14],
      rotation: [0.22, 0, Math.PI]
    }),
    transformGeometry(new THREE.BoxGeometry(0.06, 0.44, 0.03), {
      position: [0.28, -0.02, -0.1],
      rotation: [0.1, 0.08, 0.06]
    }),
    transformGeometry(new THREE.BoxGeometry(0.16, 0.08, 0.11), {
      position: [0.3, 0.18, -0.1]
    }),
    transformGeometry(new THREE.BoxGeometry(0.16, 0.09, 0.11), {
      position: [-0.14, 0.2, 0]
    }),
    transformGeometry(new THREE.BoxGeometry(0.16, 0.09, 0.11), {
      position: [0.14, 0.2, 0]
    }),
    transformGeometry(new THREE.BoxGeometry(0.18, 0.08, 0.12), {
      position: [0, -0.43, 0.02]
    })
  ]);
}

function createBasaltColumnGeometry() {
  return mergeParts([
    transformGeometry(new THREE.CylinderGeometry(0.18, 0.22, 1.6, 6), {
      position: [0, 0.02, 0]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.14, 0.18, 1.2, 6), {
      position: [-0.26, -0.08, 0.18]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.15, 0.18, 1.36, 6), {
      position: [0.24, -0.02, -0.14]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.32, 0.34, 0.14, 6), {
      position: [0, -0.78, 0]
    })
  ]);
}

function createCanopyTreeGeometry() {
  return mergeParts([
    transformGeometry(new THREE.CylinderGeometry(0.08, 0.12, 1.28, 6), {
      position: [0, -0.18, 0]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.16, 0.11, 0.22, 6), {
      position: [0, -0.72, 0]
    }),
    transformGeometry(new THREE.ConeGeometry(0.92, 0.72, 7), {
      position: [0.02, -0.04, 0]
    }),
    transformGeometry(new THREE.ConeGeometry(0.76, 0.66, 7), {
      position: [-0.02, 0.24, 0.02]
    }),
    transformGeometry(new THREE.ConeGeometry(0.58, 0.58, 7), {
      position: [0.03, 0.5, -0.02]
    }),
    transformGeometry(new THREE.ConeGeometry(0.42, 0.46, 7), {
      position: [-0.02, 0.74, 0.03]
    }),
    transformGeometry(new THREE.ConeGeometry(0.24, 0.28, 7), {
      position: [0, 0.96, 0]
    })
  ]);
}

function createGlassSpireGeometry() {
  return mergeParts([
    transformGeometry(new THREE.OctahedronGeometry(0.34, 0), {
      position: [0, 0.12, 0],
      scale: [1, 1.48, 1]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.18, 0.24, 0.36, 6), {
      position: [0, -0.54, 0]
    }),
    transformGeometry(new THREE.ConeGeometry(0.18, 0.48, 5), {
      position: [0.24, -0.02, 0],
      rotation: [0, 0, -0.28]
    }),
    transformGeometry(new THREE.ConeGeometry(0.18, 0.44, 5), {
      position: [-0.22, 0.08, 0.06],
      rotation: [0.12, 0.22, 0.36]
    }),
    transformGeometry(new THREE.ConeGeometry(0.16, 0.38, 5), {
      position: [0.02, -0.02, -0.26],
      rotation: [0.32, 0, 0]
    })
  ]);
}

function createRiftBeastGeometry() {
  return mergeParts([
    transformGeometry(new THREE.CapsuleGeometry(0.18, 0.5, 3, 8), {
      position: [0, -0.02, 0],
      rotation: [0, 0, Math.PI * 0.5],
      scale: [1.08, 0.9, 0.82]
    }),
    transformGeometry(new THREE.ConeGeometry(0.18, 0.4, 6), {
      position: [0.36, 0.06, 0],
      rotation: [0, 0, -Math.PI * 0.5]
    }),
    transformGeometry(new THREE.BoxGeometry(0.18, 0.16, 0.24), {
      position: [0.18, 0.08, 0]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.04, 0.05, 0.34, 5), {
      position: [-0.18, -0.34, 0.16]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.04, 0.05, 0.34, 5), {
      position: [0.1, -0.34, 0.16]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.04, 0.05, 0.34, 5), {
      position: [-0.18, -0.34, -0.16]
    }),
    transformGeometry(new THREE.CylinderGeometry(0.04, 0.05, 0.34, 5), {
      position: [0.1, -0.34, -0.16]
    }),
    transformGeometry(new THREE.ConeGeometry(0.06, 0.28, 4), {
      position: [0.3, 0.2, 0.1],
      rotation: [0.1, 0, -0.7]
    }),
    transformGeometry(new THREE.ConeGeometry(0.06, 0.28, 4), {
      position: [0.3, 0.2, -0.1],
      rotation: [-0.1, 0, -0.7]
    }),
    transformGeometry(new THREE.ConeGeometry(0.05, 0.3, 5), {
      position: [-0.42, 0.02, 0],
      rotation: [0, 0, Math.PI * 0.7]
    })
  ]);
}

function createSpiritCompanionGeometry() {
  return mergeParts([
    transformGeometry(new THREE.IcosahedronGeometry(0.2, 0), {
      position: [0, 0, 0]
    }),
    transformGeometry(new THREE.TorusGeometry(0.34, 0.05, 6, 18), {
      position: [0, 0.02, 0],
      rotation: [Math.PI * 0.45, 0.18, 0]
    }),
    transformGeometry(new THREE.OctahedronGeometry(0.09, 0), {
      position: [0.34, 0.1, 0],
      scale: [0.65, 1.2, 0.65]
    }),
    transformGeometry(new THREE.OctahedronGeometry(0.08, 0), {
      position: [-0.26, -0.08, 0.16],
      scale: [0.6, 1.15, 0.6]
    }),
    transformGeometry(new THREE.OctahedronGeometry(0.08, 0), {
      position: [-0.04, 0.24, -0.28],
      scale: [0.55, 1.1, 0.55]
    })
  ]);
}

function createSupplyCrateGeometry() {
  return mergeParts([
    transformGeometry(new THREE.BoxGeometry(0.72, 0.42, 0.56), {
      position: [0, -0.02, 0]
    }),
    transformGeometry(new THREE.BoxGeometry(0.78, 0.08, 0.62), {
      position: [0, 0.19, 0]
    }),
    transformGeometry(new THREE.BoxGeometry(0.08, 0.46, 0.62), {
      position: [-0.35, -0.01, 0]
    }),
    transformGeometry(new THREE.BoxGeometry(0.08, 0.46, 0.62), {
      position: [0.35, -0.01, 0]
    }),
    transformGeometry(new THREE.BoxGeometry(0.78, 0.1, 0.08), {
      position: [0, 0, -0.25]
    }),
    transformGeometry(new THREE.BoxGeometry(0.78, 0.1, 0.08), {
      position: [0, 0, 0.25]
    })
  ]);
}

function createWeatheredBoulderGeometry() {
  return mergeParts([
    transformGeometry(new THREE.DodecahedronGeometry(0.42, 0), {
      position: [0, -0.02, 0],
      scale: [1.18, 1.0, 1.06]
    }),
    transformGeometry(new THREE.DodecahedronGeometry(0.24, 0), {
      position: [0.26, 0.08, -0.06],
      scale: [1, 0.86, 0.9]
    }),
    transformGeometry(new THREE.DodecahedronGeometry(0.18, 0), {
      position: [-0.18, 0.16, 0.12],
      scale: [0.88, 0.72, 0.78]
    }),
    transformGeometry(new THREE.BoxGeometry(0.14, 0.34, 0.08), {
      position: [0.12, 0.12, 0.24],
      rotation: [0.14, 0.22, 0.3]
    })
  ]);
}

export function createRasterRingTexturePng(variant, size = 60) {
  return encodePngRgba(size, size, (x, y, width, height) =>
    ringPixelForVariant(variant, x, y, width, height)
  );
}

function ringPixelForVariant(variant, x, y, width, height) {
  const centerX = (width - 1) * 0.5;
  const centerY = (height - 1) * 0.5;
  const dx = x - centerX;
  const dy = y - centerY;
  const distance = Math.hypot(dx, dy);
  const normalizedDistance = distance / Math.max(width * 0.5, 1);
  const angle = Math.atan2(dy, dx);

  if (variant === "mist") {
    const swirl =
      (Math.sin(angle * 5 + normalizedDistance * 18) +
        Math.cos(angle * 7 - normalizedDistance * 11)) *
      0.5;
    const cloud =
      (Math.sin(x * 0.37) + Math.cos(y * 0.29) + Math.sin((x + y) * 0.19)) *
      0.3333333333;
    const ring = Math.max(0, 1 - Math.abs(normalizedDistance - 0.78) / 0.18);
    const alpha = clamp01(ring * (0.45 + 0.35 * cloud + 0.2 * swirl));
    const tint = clamp01(0.5 + 0.25 * cloud + 0.25 * swirl);
    return [
      Math.round(lerp(120, 215, tint)),
      Math.round(lerp(210, 251, tint)),
      Math.round(lerp(225, 255, tint)),
      Math.round(alpha * 255)
    ];
  }

  if (variant === "danger") {
    const jagged = Math.sin(angle * 8 + normalizedDistance * 20) * 0.5 + 0.5;
    const band = Math.max(0, 1 - Math.abs(normalizedDistance - (0.76 + 0.03 * jagged)) / 0.16);
    const spark = clamp01((Math.sin(x * 1.7) + Math.cos(y * 1.3)) * 0.25 + 0.75);
    const alpha = clamp01(band * (0.55 + 0.45 * spark));
    const tint = clamp01(0.35 + 0.65 * jagged);
    return [
      Math.round(lerp(243, 255, tint)),
      Math.round(lerp(91, 158, 1 - tint)),
      Math.round(lerp(74, 143, 1 - tint)),
      Math.round(alpha * 255)
    ];
  }

  const outerRadius = width * 0.48;
  const innerTransparentRadius = outerRadius * 0.55;
  const innerPeakRadius = outerRadius * 0.72;
  const outerFadeStartRadius = outerRadius * 0.88;
  const shimmer =
    ((Math.sin(x * 0.83 + y * 1.17) + Math.sin(x * 1.91 - y * 0.67)) * 0.5 + 1) *
    0.5;

  let alpha = 0;
  let blend = 0;

  if (distance >= innerTransparentRadius && distance < innerPeakRadius) {
    alpha = 0.92 * smoothstep(innerTransparentRadius, innerPeakRadius, distance);
  } else if (distance >= innerPeakRadius && distance < outerFadeStartRadius) {
    const t = smoothstep(innerPeakRadius, outerFadeStartRadius, distance);
    alpha = 0.92 + (0.45 - 0.92) * t;
    blend = t;
  } else if (distance >= outerFadeStartRadius && distance <= outerRadius) {
    const t = smoothstep(outerFadeStartRadius, outerRadius, distance);
    alpha = 0.45 * (1 - t);
    blend = 1;
  }

  const tint = clamp01(blend * 0.75 + shimmer * 0.25);
  return [
    Math.round(lerp(94, 215, tint)),
    Math.round(lerp(238, 255, tint)),
    Math.round(lerp(200, 242, tint)),
    Math.round(alpha * 255)
  ];
}

function encodePngRgba(width, height, pixelAt) {
  const rows = new Array(height);

  for (let y = 0; y < height; y += 1) {
    const row = Buffer.alloc(1 + width * 4);
    row[0] = 0;

    for (let x = 0; x < width; x += 1) {
      const offset = 1 + x * 4;
      const [r, g, b, a] = pixelAt(x, y, width, height);
      row[offset] = r;
      row[offset + 1] = g;
      row[offset + 2] = b;
      row[offset + 3] = a;
    }

    rows[y] = row;
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;

  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", deflateSync(Buffer.concat(rows), { level: 9 })),
    pngChunk("IEND", Buffer.alloc(0))
  ]);
}

function pngChunk(type, data) {
  const typeBuffer = Buffer.from(type, "ascii");
  const lengthBuffer = Buffer.alloc(4);
  lengthBuffer.writeUInt32BE(data.length, 0);
  const crcBuffer = Buffer.alloc(4);
  crcBuffer.writeUInt32BE(crc32(Buffer.concat([typeBuffer, data])), 0);
  return Buffer.concat([lengthBuffer, typeBuffer, data, crcBuffer]);
}

function crc32(buffer) {
  let crc = 0xffffffff;

  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc & 1) === 1 ? 0xedb88320 ^ (crc >>> 1) : crc >>> 1;
    }
  }

  return (crc ^ 0xffffffff) >>> 0;
}

function lerp(start, end, t) {
  return start + (end - start) * clamp01(t);
}

function clamp01(value) {
  return Math.min(Math.max(value, 0), 1);
}

function smoothstep(edge0, edge1, value) {
  if (edge0 === edge1) {
    return value >= edge1 ? 1 : 0;
  }
  const t = clamp01((value - edge0) / (edge1 - edge0));
  return t * t * (3 - 2 * t);
}
