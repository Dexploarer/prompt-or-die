import {
  BoxGeometry,
  ConeGeometry,
  Color,
  CylinderGeometry,
  DataTexture,
  DodecahedronGeometry,
  FrontSide,
  Mesh,
  MeshBasicMaterial,
  MeshToonMaterial,
  MeshStandardMaterial,
  NearestFilter,
  NoColorSpace,
  PlaneGeometry,
  RepeatWrapping,
  SRGBColorSpace,
  Texture,
  TextureLoader
} from "three";
import type { BufferGeometry, Material, Side } from "three";

import type { RgbaTuple, ThreeJsMeshBatch, ThreeJsSpriteBatch } from "./contracts";
import type { PodThreeQualityProfile } from "./quality";

export interface ResolvedSpriteTexture {
  texture: Texture;
  repeat?: [number, number];
  offset?: [number, number];
}

export interface PodThreeAssetRegistry {
  resolveGeometry(
    batch: ThreeJsMeshBatch,
    lodLevel?: 0 | 1 | 2
  ): BufferGeometry | Promise<BufferGeometry>;
  resolveMeshMaterial?(
    batch: ThreeJsMeshBatch,
    lodLevel?: 0 | 1 | 2,
    quality?: PodThreeQualityProfile
  ): Material | Promise<Material>;
  resolveSpriteTexture(
    batch: Pick<ThreeJsSpriteBatch, "texture" | "frame">,
    anisotropy?: number
  ): ResolvedSpriteTexture | Promise<ResolvedSpriteTexture>;
}

export type PodThreeAssetCategory =
  | "character"
  | "companion"
  | "creature"
  | "effect"
  | "flora"
  | "loot"
  | "resource"
  | "structure"
  | "ui";

export interface PodThreeMeshAssetDescriptor {
  path: string;
  lods?: Partial<Record<0 | 1 | 2, string>>;
  aliases?: string[];
  category?: PodThreeAssetCategory;
  tags?: string[];
}

export interface PodThreeSpriteAssetDescriptor {
  path: string;
  ktx2Path?: string;
  aliases?: string[];
  category?: PodThreeAssetCategory;
  tags?: string[];
  repeat?: [number, number];
  offset?: [number, number];
  colorSpace?: "srgb" | "none";
}

export interface PodThreeAssetManifest {
  version: 1;
  meshes: Record<string, PodThreeMeshAssetDescriptor>;
  sprites: Record<string, PodThreeSpriteAssetDescriptor>;
}

export interface PodThreeGeometryLoader {
  load(path: string): Promise<BufferGeometry>;
}

export interface PodThreeTextureLoader {
  load(
    path: string,
    options: {
      anisotropy: number;
      colorSpace: "srgb" | "none";
    }
  ): Promise<Texture>;
}

export interface PodThreeCompressedTextureLoader {
  load(path: string, anisotropy: number): Promise<Texture>;
}

export interface ManifestBackedPodThreeAssetRegistryOptions {
  manifest: PodThreeAssetManifest;
  fallbackRegistry?: PodThreeAssetRegistry;
  geometryLoader: PodThreeGeometryLoader;
  textureLoader: PodThreeTextureLoader;
  compressedTextureLoader?: PodThreeCompressedTextureLoader | null;
}

export interface RuntimePodThreeAssetRegistryOptions {
  renderer: unknown;
  manifestUrl?: string;
  basisTranscoderPath?: string;
  fallbackRegistry?: PodThreeAssetRegistry;
  fetchImpl?: typeof fetch;
  geometryLoader?: PodThreeGeometryLoader;
  textureLoader?: PodThreeTextureLoader;
  compressedTextureLoader?: PodThreeCompressedTextureLoader | null;
}

export class DefaultPodThreeAssetRegistry implements PodThreeAssetRegistry {
  private geometryCache = new Map<string, BufferGeometry>();
  private textureCache = new Map<string, Texture>();

  resolveGeometry(batch: ThreeJsMeshBatch, lodLevel: 0 | 1 | 2 = 0): BufferGeometry {
    const cacheKey = `${batch.mesh}:lod:${lodLevel}`;
    const cached = this.geometryCache.get(cacheKey);
    if (cached) {
      return cached;
    }

    const meshName = batch.mesh.toLowerCase();
    let geometry: BufferGeometry;

    if (meshName.includes("spire") || meshName.includes("obelisk") || meshName.includes("spike")) {
      geometry = new ConeGeometry(1.2, 4.8, lodSegments([20, 12, 6], lodLevel));
    } else if (meshName.includes("rock") || meshName.includes("boulder")) {
      geometry =
        lodLevel === 0
          ? new DodecahedronGeometry(1.35, 1)
          : lodLevel === 1
            ? new DodecahedronGeometry(1.35, 0)
            : new BoxGeometry(2.2, 1.8, 1.9);
    } else if (
      meshName.includes("column") ||
      meshName.includes("tower") ||
      meshName.includes("pillar")
    ) {
      geometry = new CylinderGeometry(
        0.65,
        0.9,
        5.2,
        lodSegments([20, 12, 8], lodLevel)
      );
    } else if (meshName.includes("tree") || meshName.includes("pine")) {
      geometry = new ConeGeometry(1.4, 3.4, lodSegments([18, 10, 6], lodLevel));
    } else {
      geometry = new BoxGeometry(2, 2, 2);
    }

    this.geometryCache.set(cacheKey, geometry);
    return geometry;
  }

  resolveSpriteTexture(
    batch: Pick<ThreeJsSpriteBatch, "texture" | "frame">,
    anisotropy = 1
  ): ResolvedSpriteTexture {
    const key = `${batch.texture}:${batch.frame}`;
    const cached = this.textureCache.get(key);
    if (cached) {
      cached.anisotropy = anisotropy;
      return { texture: cached };
    }

    const texture = createRadialTexture(hashColor(batch.texture));
    texture.anisotropy = anisotropy;
    this.textureCache.set(key, texture);
    return { texture };
  }
}

export class ManifestBackedPodThreeAssetRegistry implements PodThreeAssetRegistry {
  private readonly fallbackRegistry: PodThreeAssetRegistry;
  private readonly geometryCache = new Map<string, Promise<BufferGeometry>>();
  private readonly textureCache = new Map<string, Promise<ResolvedSpriteTexture>>();
  private readonly meshDescriptorCache = new Map<string, PodThreeMeshAssetDescriptor | null>();
  private readonly spriteDescriptorCache = new Map<string, PodThreeSpriteAssetDescriptor | null>();

  constructor(private readonly options: ManifestBackedPodThreeAssetRegistryOptions) {
    this.fallbackRegistry = options.fallbackRegistry ?? new DefaultPodThreeAssetRegistry();
  }

  resolveGeometry(
    batch: ThreeJsMeshBatch,
    lodLevel: 0 | 1 | 2 = 0
  ): BufferGeometry | Promise<BufferGeometry> {
    const descriptor = this.resolveMeshDescriptor(batch.mesh);
    if (!descriptor) {
      return this.fallbackRegistry.resolveGeometry(batch, lodLevel);
    }

    const assetPath = descriptor.lods?.[lodLevel] ?? descriptor.path;
    const cacheKey = `${assetPath}|lod:${lodLevel}`;
    const cached = this.geometryCache.get(cacheKey);
    if (cached) {
      return cached;
    }

    const pending = this.options.geometryLoader.load(assetPath).catch(async (error) => {
      this.geometryCache.delete(cacheKey);
      console.warn(`Falling back to procedural mesh asset for ${batch.mesh}`, error);
      return await Promise.resolve(this.fallbackRegistry.resolveGeometry(batch, lodLevel));
    });
    this.geometryCache.set(cacheKey, pending);
    return pending;
  }

  resolveSpriteTexture(
    batch: Pick<ThreeJsSpriteBatch, "texture" | "frame">,
    anisotropy = 1
  ): ResolvedSpriteTexture | Promise<ResolvedSpriteTexture> {
    const descriptor = this.resolveSpriteDescriptor(batch.texture);
    if (!descriptor) {
      return this.fallbackRegistry.resolveSpriteTexture(batch, anisotropy);
    }

    const assetPath =
      descriptor.ktx2Path && this.options.compressedTextureLoader ? descriptor.ktx2Path : descriptor.path;
    const cacheKey = `${assetPath}|anisotropy:${anisotropy}`;
    const cached = this.textureCache.get(cacheKey);
    if (cached) {
      return cached;
    }

    const pending = this.loadSpriteTexture(descriptor, assetPath, anisotropy).catch(async (error) => {
      this.textureCache.delete(cacheKey);
      console.warn(`Falling back to procedural sprite asset for ${batch.texture}`, error);
      return await Promise.resolve(this.fallbackRegistry.resolveSpriteTexture(batch, anisotropy));
    });
    this.textureCache.set(cacheKey, pending);
    return pending;
  }

  private async loadSpriteTexture(
    descriptor: PodThreeSpriteAssetDescriptor,
    assetPath: string,
    anisotropy: number
  ): Promise<ResolvedSpriteTexture> {
    const texture =
      descriptor.ktx2Path &&
      this.options.compressedTextureLoader &&
      assetPath === descriptor.ktx2Path
        ? await this.options.compressedTextureLoader.load(assetPath, anisotropy)
        : await this.options.textureLoader.load(assetPath, {
            anisotropy,
            colorSpace: descriptor.colorSpace ?? "srgb"
          });

    return {
      texture,
      repeat: descriptor.repeat,
      offset: descriptor.offset
    };
  }

  private resolveMeshDescriptor(requested: string): PodThreeMeshAssetDescriptor | null {
    const cacheKey = normalizeAssetKey(requested);
    if (this.meshDescriptorCache.has(cacheKey)) {
      return this.meshDescriptorCache.get(cacheKey) ?? null;
    }

    const descriptor = resolveManifestMeshAsset(this.options.manifest, requested);
    this.meshDescriptorCache.set(cacheKey, descriptor);
    return descriptor;
  }

  private resolveSpriteDescriptor(requested: string): PodThreeSpriteAssetDescriptor | null {
    const cacheKey = normalizeAssetKey(requested);
    if (this.spriteDescriptorCache.has(cacheKey)) {
      return this.spriteDescriptorCache.get(cacheKey) ?? null;
    }

    const descriptor = resolveManifestSpriteAsset(this.options.manifest, requested);
    this.spriteDescriptorCache.set(cacheKey, descriptor);
    return descriptor;
  }
}

export async function createManifestBackedAssetRegistry(
  options: RuntimePodThreeAssetRegistryOptions
): Promise<PodThreeAssetRegistry> {
  const fallbackRegistry = options.fallbackRegistry ?? new DefaultPodThreeAssetRegistry();
  const fetchImpl =
    options.fetchImpl ?? (typeof fetch === "function" ? fetch.bind(globalThis) : null);
  if (!fetchImpl) {
    return fallbackRegistry;
  }

  try {
    const response = await fetchImpl(options.manifestUrl ?? "/assets/pod-asset-manifest.json");
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}`);
    }

    const manifest = parsePodThreeAssetManifest(await response.json());
    const runtimeLoaders = await createRuntimeAssetLoaders({
      renderer: options.renderer,
      basisTranscoderPath: options.basisTranscoderPath ?? "/assets/basis/"
    });

    return new ManifestBackedPodThreeAssetRegistry({
      manifest,
      fallbackRegistry,
      geometryLoader: options.geometryLoader ?? runtimeLoaders.geometryLoader,
      textureLoader: options.textureLoader ?? runtimeLoaders.textureLoader,
      compressedTextureLoader:
        options.compressedTextureLoader ?? runtimeLoaders.compressedTextureLoader
    });
  } catch (error) {
    console.warn("Falling back to procedural pod-web assets", error);
    return fallbackRegistry;
  }
}

export function parsePodThreeAssetManifest(input: unknown): PodThreeAssetManifest {
  if (!isRecord(input)) {
    throw new Error("Invalid POD asset manifest: expected object root");
  }

  return {
    version: 1,
    meshes: parseAssetRecord(input.meshes, parseMeshDescriptor),
    sprites: parseAssetRecord(input.sprites, parseSpriteDescriptor)
  };
}

export function resolveManifestMeshAsset(
  manifest: PodThreeAssetManifest,
  requested: string
): PodThreeMeshAssetDescriptor | null {
  return findBestAssetDescriptor(manifest.meshes, requested);
}

export function resolveManifestSpriteAsset(
  manifest: PodThreeAssetManifest,
  requested: string
): PodThreeSpriteAssetDescriptor | null {
  return findBestAssetDescriptor(manifest.sprites, requested);
}

export function normalizeAssetKey(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function createMeshMaterial(
  batch: ThreeJsMeshBatch,
  lodLevel: 0 | 1 | 2,
  quality: Pick<PodThreeQualityProfile, "environmentIntensity">
): Material {
  if (shouldUseToonShading(batch)) {
    const material = new MeshToonMaterial({
      color: new Color(batch.tint[0], batch.tint[1], batch.tint[2]),
      transparent: batch.transparent,
      opacity: batch.tint[3],
      emissive: new Color(batch.emissive[0], batch.emissive[1], batch.emissive[2]),
      emissiveIntensity: 0.4 + quality.environmentIntensity * 0.25,
      depthWrite: batch.depthWrite,
      depthTest: batch.depthTest,
      side: batch.doubleSided ? 2 : FrontSide,
      gradientMap: getToonGradientMap()
    });
    material.name = `pod-toon:${batch.mesh}:${batch.material}`;
    return material;
  }

  const material = new MeshStandardMaterial({
    color: new Color(batch.tint[0], batch.tint[1], batch.tint[2]),
    transparent: batch.transparent,
    opacity: batch.tint[3],
    roughness: batch.roughness,
    metalness: batch.metallic,
    emissive: new Color(batch.emissive[0], batch.emissive[1], batch.emissive[2]),
    depthWrite: batch.depthWrite,
    depthTest: batch.depthTest,
    side: batch.doubleSided ? 2 : FrontSide
  });
  material.dithering = true;
  material.envMapIntensity =
    quality.environmentIntensity * (lodLevel === 0 ? 1 : lodLevel === 1 ? 0.92 : 0.82);
  material.flatShading = lodLevel === 2;
  material.name = `pod-mesh:${batch.mesh}:${batch.material}`;
  return material;
}

export function createSpriteMaterial(
  resolved: ResolvedSpriteTexture,
  tint: RgbaTuple,
  transparent: boolean,
  depthWrite: boolean,
  depthTest: boolean,
  side: Side = FrontSide
): MeshBasicMaterial {
  const material = new MeshBasicMaterial({
    map: resolved.texture,
    color: new Color(tint[0], tint[1], tint[2]),
    transparent: transparent || tint[3] < 0.999,
    opacity: tint[3],
    depthWrite,
    depthTest,
    side
  });

  if (resolved.repeat || resolved.offset) {
    material.map?.matrix.identity();
    material.map?.repeat.set(resolved.repeat?.[0] ?? 1, resolved.repeat?.[1] ?? 1);
    material.map?.offset.set(resolved.offset?.[0] ?? 0, resolved.offset?.[1] ?? 0);
    if (material.map) {
      material.map.matrixAutoUpdate = false;
      material.map.updateMatrix();
    }
  }

  return material;
}

export const OVERLAY_PLANE_GEOMETRY = new PlaneGeometry(1, 1);
export const SPRITE_PLANE_GEOMETRY = new PlaneGeometry(1, 1);

function createRadialTexture(color: [number, number, number]): Texture {
  const size = 32;
  const pixels = new Uint8Array(size * size * 4);

  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      const index = (y * size + x) * 4;
      const dx = (x / (size - 1)) * 2 - 1;
      const dy = (y / (size - 1)) * 2 - 1;
      const distance = Math.sqrt(dx * dx + dy * dy);
      const alpha = Math.max(0, Math.min(1, 1 - distance));

      pixels[index] = color[0];
      pixels[index + 1] = color[1];
      pixels[index + 2] = color[2];
      pixels[index + 3] = Math.round(alpha * 255);
    }
  }

  const texture = new DataTexture(pixels, size, size);
  texture.needsUpdate = true;
  texture.colorSpace = SRGBColorSpace;
  texture.wrapS = RepeatWrapping;
  texture.wrapT = RepeatWrapping;
  texture.name = "pod-fallback-sprite";
  return texture;
}

function hashColor(input: string): [number, number, number] {
  let hash = 2166136261;
  for (let index = 0; index < input.length; index += 1) {
    hash ^= input.charCodeAt(index);
    hash = Math.imul(hash, 16777619);
  }

  return [
    96 + (hash & 0x7f),
    96 + ((hash >> 8) & 0x7f),
    96 + ((hash >> 16) & 0x7f)
  ];
}

function lodSegments(levels: [number, number, number], lodLevel: 0 | 1 | 2): number {
  return levels[lodLevel];
}

let toonGradientMap: Texture | null = null;

function getToonGradientMap(): Texture {
  if (toonGradientMap) {
    return toonGradientMap;
  }

  const gradient = new Uint8Array([
    48, 48, 48, 255, 116, 116, 116, 255, 184, 184, 184, 255, 255, 255, 255, 255
  ]);
  const texture = new DataTexture(gradient, 4, 1);
  texture.needsUpdate = true;
  texture.colorSpace = NoColorSpace;
  texture.minFilter = NearestFilter;
  texture.magFilter = NearestFilter;
  texture.generateMipmaps = false;
  texture.name = "pod-toon-gradient";
  toonGradientMap = texture;
  return texture;
}

function shouldUseToonShading(batch: ThreeJsMeshBatch): boolean {
  if (batch.transparent) {
    return false;
  }

  const token = `${batch.mesh} ${batch.material}`.toLowerCase();
  return (
    batch.metallic <= 0.2 &&
    /basalt|obsidian|stone|tower|column|pillar|rock|boulder|tree|pine|terrain|foliage/.test(
      token
    )
  );
}

function parseAssetRecord<T>(
  input: unknown,
  parseDescriptor: (input: unknown) => T
): Record<string, T> {
  if (!isRecord(input)) {
    return {};
  }

  const output: Record<string, T> = {};
  for (const [key, value] of Object.entries(input)) {
    output[key] = parseDescriptor(value);
  }
  return output;
}

function parseMeshDescriptor(input: unknown): PodThreeMeshAssetDescriptor {
  if (!isRecord(input) || typeof input.path !== "string") {
    throw new Error("Invalid POD mesh asset descriptor");
  }

  return {
    path: input.path,
    lods: parseLodRecord(input.lods),
    aliases: parseStringArray(input.aliases),
    category: parseAssetCategory(input.category),
    tags: parseStringArray(input.tags)
  };
}

function parseSpriteDescriptor(input: unknown): PodThreeSpriteAssetDescriptor {
  if (!isRecord(input) || typeof input.path !== "string") {
    throw new Error("Invalid POD sprite asset descriptor");
  }

  return {
    path: input.path,
    ktx2Path: typeof input.ktx2Path === "string" ? input.ktx2Path : undefined,
    aliases: parseStringArray(input.aliases),
    category: parseAssetCategory(input.category),
    tags: parseStringArray(input.tags),
    repeat: parseTuple2(input.repeat),
    offset: parseTuple2(input.offset),
    colorSpace: input.colorSpace === "none" ? "none" : "srgb"
  };
}

function parseLodRecord(input: unknown): Partial<Record<0 | 1 | 2, string>> | undefined {
  if (!isRecord(input)) {
    return undefined;
  }

  const lods: Partial<Record<0 | 1 | 2, string>> = {};
  for (const [key, value] of Object.entries(input)) {
    if ((key === "0" || key === "1" || key === "2") && typeof value === "string") {
      lods[Number(key) as 0 | 1 | 2] = value;
    }
  }
  return Object.keys(lods).length > 0 ? lods : undefined;
}

function parseStringArray(input: unknown): string[] | undefined {
  if (!Array.isArray(input)) {
    return undefined;
  }

  const values = input.filter((value): value is string => typeof value === "string");
  return values.length > 0 ? values : undefined;
}

function parseTuple2(input: unknown): [number, number] | undefined {
  if (
    Array.isArray(input) &&
    input.length === 2 &&
    typeof input[0] === "number" &&
    typeof input[1] === "number"
  ) {
    return [input[0], input[1]];
  }
  return undefined;
}

function parseAssetCategory(input: unknown): PodThreeAssetCategory | undefined {
  const normalized = typeof input === "string" ? normalizeAssetKey(input) : null;
  switch (normalized) {
    case "character":
    case "companion":
    case "creature":
    case "effect":
    case "flora":
    case "loot":
    case "resource":
    case "structure":
    case "ui":
      return normalized;
    default:
      return undefined;
  }
}

function findBestAssetDescriptor<T extends { aliases?: string[]; tags?: string[]; category?: string }>(
  record: Record<string, T>,
  requested: string
): T | null {
  const normalizedRequested = normalizeAssetKey(requested);
  if (!normalizedRequested) {
    return null;
  }

  let bestScore = 0;
  let bestDescriptor: T | null = null;
  for (const [key, descriptor] of Object.entries(record)) {
    const score = scoreAssetDescriptor(key, descriptor, normalizedRequested);
    if (score > bestScore) {
      bestScore = score;
      bestDescriptor = descriptor;
    }
  }

  return bestScore >= 8 ? bestDescriptor : null;
}

function scoreAssetDescriptor(
  key: string,
  descriptor: { aliases?: string[]; tags?: string[]; category?: string },
  requested: string
): number {
  const normalizedKey = normalizeAssetKey(key);
  if (normalizedKey === requested) {
    return 100;
  }

  const aliases = descriptor.aliases?.map(normalizeAssetKey) ?? [];
  if (aliases.includes(requested)) {
    return 96;
  }

  const requestedTokens = requested.split("-").filter(Boolean);
  let score = 0;

  const candidates = [
    normalizedKey,
    ...aliases,
    ...(descriptor.tags?.map(normalizeAssetKey) ?? []),
    descriptor.category ? normalizeAssetKey(descriptor.category) : ""
  ].filter(Boolean);

  for (const candidate of candidates) {
    if (candidate.includes(requested) || requested.includes(candidate)) {
      score = Math.max(score, 72);
    }

    const candidateTokens = candidate.split("-").filter(Boolean);
    let overlap = 0;
    for (const token of requestedTokens) {
      if (candidateTokens.includes(token)) {
        overlap += 1;
      }
    }
    score = Math.max(score, overlap * 12);
  }

  return score;
}

async function createRuntimeAssetLoaders(options: {
  renderer: unknown;
  basisTranscoderPath: string;
}): Promise<{
  geometryLoader: PodThreeGeometryLoader;
  textureLoader: PodThreeTextureLoader;
  compressedTextureLoader: PodThreeCompressedTextureLoader | null;
}> {
  const [{ GLTFLoader }, { KTX2Loader }, { MeshoptDecoder }] = await Promise.all([
    import("three/examples/jsm/loaders/GLTFLoader.js"),
    import("three/examples/jsm/loaders/KTX2Loader.js"),
    import("three/examples/jsm/libs/meshopt_decoder.module.js")
  ]);

  const textureLoader = new TextureLoader();
  const ktx2Loader = new KTX2Loader();
  ktx2Loader.setTranscoderPath(options.basisTranscoderPath);
  ktx2Loader.detectSupport(options.renderer as never);

  const geometryLoader = new GLTFLoader();
  geometryLoader.setMeshoptDecoder(MeshoptDecoder);
  geometryLoader.setKTX2Loader(ktx2Loader);

  return {
    geometryLoader: {
      async load(path: string): Promise<BufferGeometry> {
        const asset = await geometryLoader.loadAsync(path);
        return extractPrimaryGeometry(asset.scene, path);
      }
    },
    textureLoader: {
      async load(
        path: string,
        loaderOptions: { anisotropy: number; colorSpace: "srgb" | "none" }
      ): Promise<Texture> {
        const texture = await textureLoader.loadAsync(path);
        texture.colorSpace =
          loaderOptions.colorSpace === "none" ? NoColorSpace : SRGBColorSpace;
        texture.anisotropy = loaderOptions.anisotropy;
        texture.name = `pod-runtime-texture:${path}`;
        return texture;
      }
    },
    compressedTextureLoader: {
      async load(path: string, anisotropy: number): Promise<Texture> {
        const texture = await ktx2Loader.loadAsync(path);
        texture.colorSpace = SRGBColorSpace;
        texture.anisotropy = anisotropy;
        texture.name = `pod-runtime-ktx2:${path}`;
        return texture;
      }
    }
  };
}

function extractPrimaryGeometry(root: { traverse: Mesh["traverse"] }, path: string): BufferGeometry {
  let primaryGeometry: BufferGeometry | null = null;

  root.traverse((node) => {
    if (primaryGeometry || !(node instanceof Mesh)) {
      return;
    }

    node.updateWorldMatrix(true, false);
    const geometry = node.geometry.clone();
    geometry.applyMatrix4(node.matrixWorld);
    geometry.computeBoundingBox();
    geometry.computeBoundingSphere();
    primaryGeometry = geometry;
  });

  if (!primaryGeometry) {
    throw new Error(`No mesh geometry found in ${path}`);
  }

  return primaryGeometry;
}

function isRecord(input: unknown): input is Record<string, unknown> {
  return typeof input === "object" && input !== null && !Array.isArray(input);
}
