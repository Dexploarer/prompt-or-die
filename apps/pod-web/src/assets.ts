import {
  BoxGeometry,
  ConeGeometry,
  Color,
  CylinderGeometry,
  DataTexture,
  DodecahedronGeometry,
  FrontSide,
  MeshBasicMaterial,
  MeshToonMaterial,
  MeshStandardMaterial,
  NearestFilter,
  NoColorSpace,
  PlaneGeometry,
  RepeatWrapping,
  SRGBColorSpace,
  Texture
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
