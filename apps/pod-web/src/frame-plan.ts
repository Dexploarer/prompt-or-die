import { Frustum, Matrix4, PerspectiveCamera, Quaternion, Sphere, Vector3 } from "three";

import type {
  CameraState,
  RgbaTuple,
  ThreeJsInstance,
  ThreeJsMeshBatch,
  ThreeJsSpriteBatch,
  ThreeJsWebGpuFrame
} from "./contracts";
import type { PodThreeQualityProfile } from "./quality";

const DEFAULT_UP = new Vector3(0, 1, 0);
const DEFAULT_RADIUS = 1.25;
const TEMP_VIEW_PROJECTION = new Matrix4();

export interface PodThreeCameraRigOptions {
  pitch?: number;
  height?: number;
  baseDistance?: number;
  minDistance?: number;
  maxDistance?: number;
  fov?: number;
  near?: number;
  far?: number;
}

export interface PodThreePlanningOptions extends PodThreeCameraRigOptions {
  frustumCulling?: boolean;
  meshCullDistance?: number;
  spriteCullDistance?: number;
  highDetailDistance?: number;
  mediumDetailDistance?: number;
  shadowDistance?: number;
  worldChunkSize?: number;
  preloadChunkRadius?: number;
}

export interface PlannedCameraPose {
  position: [number, number, number];
  target: [number, number, number];
  quaternion: Quaternion;
  fov: number;
  near: number;
  far: number;
}

export interface PlannedMeshBatch {
  key: string;
  batch: ThreeJsMeshBatch;
  lodLevel: 0 | 1 | 2;
  visibleCount: number;
  matrices: Matrix4[];
}

export interface PlannedSpriteBatch {
  key: string;
  batch: Omit<ThreeJsSpriteBatch, "instances">;
  tint: RgbaTuple;
  visibleCount: number;
  instances: ThreeJsInstance[];
  matrices: Matrix4[];
}

export interface PlannedMeshPrewarmRequest {
  key: string;
  batch: ThreeJsMeshBatch;
  lodLevel: 0 | 1 | 2;
  chunkKey: string;
}

export interface PlannedSpritePrewarmRequest {
  key: string;
  batch: Pick<ThreeJsSpriteBatch, "texture" | "frame">;
  chunkKey: string;
}

export interface PlannedFrame {
  camera: PlannedCameraPose;
  meshBatches: PlannedMeshBatch[];
  spriteBatches: PlannedSpriteBatch[];
  visibleWorldChunks: string[];
  preloadedWorldChunks: string[];
  prewarmMeshRequests: PlannedMeshPrewarmRequest[];
  prewarmSpriteRequests: PlannedSpritePrewarmRequest[];
}

export function buildCameraPose(
  camera: CameraState,
  options: PodThreeCameraRigOptions = {}
): PlannedCameraPose {
  const pitch = options.pitch ?? 0.42;
  const baseDistance = options.baseDistance ?? 28;
  const minDistance = options.minDistance ?? 18;
  const maxDistance = options.maxDistance ?? 48;
  const distance = clamp(baseDistance / Math.max(camera.zoom, 0.15), minDistance, maxDistance);
  const heightOffset = options.height ?? 3.5;
  const target = new Vector3(camera.x, 0, camera.y);
  const azimuth = camera.rotation;
  const horizontalDistance = Math.cos(pitch) * distance;
  const position = new Vector3(
    target.x + Math.sin(azimuth) * horizontalDistance,
    target.y + Math.sin(pitch) * distance + heightOffset,
    target.z + Math.cos(azimuth) * horizontalDistance
  );
  const quaternion = new Quaternion().setFromRotationMatrix(
    new Matrix4().lookAt(position, target, DEFAULT_UP)
  );

  return {
    position: position.toArray(),
    target: target.toArray(),
    quaternion,
    fov: options.fov ?? 52,
    near: options.near ?? 0.1,
    far: options.far ?? 1024
  };
}

export function buildFramePlan(
  frame: ThreeJsWebGpuFrame,
  options: PodThreePlanningOptions = {}
): PlannedFrame {
  const camera = buildCameraPose(frame.camera, options);
  const viewCamera = createViewCamera(frame.camera, camera);
  const frustum = new Frustum().setFromProjectionMatrix(
    TEMP_VIEW_PROJECTION.multiplyMatrices(viewCamera.projectionMatrix, viewCamera.matrixWorldInverse)
  );
  const cameraPosition = new Vector3(...camera.position);
  const meshCullDistance = options.meshCullDistance ?? Number.POSITIVE_INFINITY;
  const spriteCullDistance = options.spriteCullDistance ?? Number.POSITIVE_INFINITY;
  const frustumCulling = options.frustumCulling ?? true;
  const highDetailDistance = options.highDetailDistance ?? 36;
  const mediumDetailDistance = options.mediumDetailDistance ?? 108;
  const shadowDistance = options.shadowDistance ?? 72;
  const worldChunkSize = options.worldChunkSize ?? 24;
  const preloadChunkRadius = options.preloadChunkRadius ?? 1;
  const visibleWorldChunks = new Set<string>();
  const meshPrewarmDistance =
    Number.isFinite(meshCullDistance)
      ? meshCullDistance + worldChunkSize * Math.max(preloadChunkRadius, 1)
      : Number.POSITIVE_INFINITY;
  const spritePrewarmDistance =
    Number.isFinite(spriteCullDistance)
      ? spriteCullDistance + worldChunkSize * Math.max(preloadChunkRadius, 1)
      : Number.POSITIVE_INFINITY;
  const meshBatches = planMeshBatches(
    frame.meshBatches,
    frustum,
    cameraPosition,
    highDetailDistance,
    mediumDetailDistance,
    meshCullDistance,
    shadowDistance,
    frustumCulling,
    visibleWorldChunks,
    worldChunkSize
  );
  const spriteBatches = planSpriteBatches(
    frame.spriteBatches,
    frustum,
    cameraPosition,
    spriteCullDistance,
    camera.quaternion,
    frustumCulling,
    visibleWorldChunks,
    worldChunkSize
  );
  const cameraChunkKey = chunkKeyFromCoordinates(frame.camera.x, frame.camera.y, worldChunkSize);
  const preloadedWorldChunks = expandChunkKeys(
    visibleWorldChunks.size > 0 ? visibleWorldChunks : [cameraChunkKey],
    preloadChunkRadius,
    cameraChunkKey
  );

  return {
    camera,
    meshBatches,
    spriteBatches,
    visibleWorldChunks: Array.from(visibleWorldChunks).sort((left, right) =>
      left.localeCompare(right)
    ),
    preloadedWorldChunks,
    prewarmMeshRequests: collectMeshPrewarmRequests(
      frame.meshBatches,
      preloadedWorldChunks,
      cameraPosition,
      highDetailDistance,
      mediumDetailDistance,
      meshPrewarmDistance,
      worldChunkSize
    ),
    prewarmSpriteRequests: collectSpritePrewarmRequests(
      frame.spriteBatches,
      preloadedWorldChunks,
      spritePrewarmDistance,
      cameraPosition,
      worldChunkSize
    )
  };
}

export function planningOptionsFromQuality(
  quality: Pick<
    PodThreeQualityProfile,
    | "meshCullDistance"
    | "spriteCullDistance"
    | "highDetailDistance"
    | "mediumDetailDistance"
    | "shadowDistance"
  >
): PodThreePlanningOptions {
  return {
    meshCullDistance: quality.meshCullDistance,
    spriteCullDistance: quality.spriteCullDistance,
    highDetailDistance: quality.highDetailDistance,
    mediumDetailDistance: quality.mediumDetailDistance,
    shadowDistance: quality.shadowDistance
  };
}

export function splitSpriteBatchesByTint(
  batches: ThreeJsSpriteBatch[]
): Array<Omit<PlannedSpriteBatch, "matrices">> {
  const planned = new Array<Omit<PlannedSpriteBatch, "matrices">>();

  for (const batch of batches) {
    const groups = new Map<string, { tint: RgbaTuple; instances: ThreeJsInstance[] }>();

    for (const instance of batch.instances) {
      const tint = instance.color ?? [1, 1, 1, 1];
      const key = tint.map((channel) => channel.toFixed(4)).join(":");
      const group = groups.get(key);

      if (group) {
        group.instances.push(instance);
      } else {
        groups.set(key, { tint, instances: [instance] });
      }
    }

    let tintIndex = 0;
    for (const { tint, instances } of groups.values()) {
      planned.push({
        key: `${createSpriteBatchKey(batch)}:tint:${tintIndex}:${tint
          .map((channel) => channel.toFixed(4))
          .join("-")}`,
        batch: {
          texture: batch.texture,
          frame: batch.frame,
          layer: batch.layer,
          billboard: batch.billboard,
          phase: batch.phase,
          sortDepth: batch.sortDepth,
          renderOrder: batch.renderOrder,
          transparent: batch.transparent || tint[3] < 0.999,
          depthWrite: batch.transparent || tint[3] < 0.999 ? false : batch.depthWrite,
          depthTest: batch.depthTest
        },
        tint,
        visibleCount: instances.length,
        instances
      });
      tintIndex += 1;
    }
  }

  planned.sort((left, right) => {
    if (left.batch.renderOrder !== right.batch.renderOrder) {
      return left.batch.renderOrder - right.batch.renderOrder;
    }

    return left.key.localeCompare(right.key);
  });

  return planned;
}

export function composeInstanceMatrix(
  instance: ThreeJsInstance,
  rotationOverride?: Quaternion
): Matrix4 {
  const matrix = new Matrix4();
  matrix.compose(
    new Vector3(...instance.position),
    rotationOverride ?? new Quaternion(...instance.rotation),
    new Vector3(...instance.scale)
  );
  return matrix;
}

function createMeshBatchKey(batch: ThreeJsMeshBatch, lodLevel?: number): string {
  return [
    "mesh",
    batch.mesh,
    batch.material,
    batch.layer,
    batch.renderOrder,
    batch.phase,
    batch.transparent ? "transparent" : "opaque",
    lodLevel ?? "base"
  ].join(":");
}

function createSpriteBatchKey(batch: ThreeJsSpriteBatch): string {
  return [
    "sprite",
    batch.texture,
    batch.frame,
    batch.layer,
    batch.renderOrder,
    batch.phase,
    batch.billboard ? "billboard" : "flat"
  ].join(":");
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

function createViewCamera(
  frameCamera: CameraState,
  pose: PlannedCameraPose
): PerspectiveCamera {
  const camera = new PerspectiveCamera(
    pose.fov,
    frameCamera.viewportWidth / Math.max(frameCamera.viewportHeight, 1),
    pose.near,
    pose.far
  );
  camera.position.set(...pose.position);
  camera.quaternion.copy(pose.quaternion);
  camera.updateProjectionMatrix();
  camera.updateMatrixWorld(true);
  camera.matrixWorldInverse.copy(camera.matrixWorld).invert();
  return camera;
}

function planMeshBatches(
  batches: ThreeJsMeshBatch[],
  frustum: Frustum,
  cameraPosition: Vector3,
  highDetailDistance: number,
  mediumDetailDistance: number,
  meshCullDistance: number,
  shadowDistance: number,
  frustumCulling: boolean,
  visibleWorldChunks: Set<string>,
  worldChunkSize: number
): PlannedMeshBatch[] {
  const planned = new Array<PlannedMeshBatch>();

  for (const batch of batches) {
    const lodGroups = new Map<
      0 | 1 | 2,
      {
        matrices: Matrix4[];
        instances: ThreeJsInstance[];
        nearestDistance: number;
      }
    >();

    for (const instance of batch.instances) {
      const radius = estimateInstanceRadius(instance);
      const center = new Vector3(...instance.position);
      const distance = center.distanceTo(cameraPosition);

      if (distance - radius > meshCullDistance) {
        continue;
      }

      if (frustumCulling && !frustum.intersectsSphere(new Sphere(center, radius))) {
        continue;
      }

      visibleWorldChunks.add(chunkKeyFromInstance(instance, worldChunkSize));
      const lodLevel = classifyLod(distance, highDetailDistance, mediumDetailDistance);
      const group = lodGroups.get(lodLevel) ?? {
        matrices: [],
        instances: [],
        nearestDistance: Number.POSITIVE_INFINITY
      };
      group.instances.push(instance);
      group.matrices.push(composeInstanceMatrix(instance));
      group.nearestDistance = Math.min(group.nearestDistance, distance);
      lodGroups.set(lodLevel, group);
    }

    for (const [lodLevel, group] of lodGroups) {
      planned.push({
        key: createMeshBatchKey(batch, lodLevel),
        batch: {
          ...batch,
          castShadows:
            batch.castShadows && lodLevel < 2 && group.nearestDistance <= shadowDistance
        },
        lodLevel,
        visibleCount: group.instances.length,
        matrices: group.matrices
      });
    }
  }

  planned.sort((left, right) => {
    if (left.batch.renderOrder !== right.batch.renderOrder) {
      return left.batch.renderOrder - right.batch.renderOrder;
    }

    if (left.lodLevel !== right.lodLevel) {
      return left.lodLevel - right.lodLevel;
    }

    return left.key.localeCompare(right.key);
  });

  return planned;
}

function planSpriteBatches(
  batches: ThreeJsSpriteBatch[],
  frustum: Frustum,
  cameraPosition: Vector3,
  spriteCullDistance: number,
  cameraQuaternion: Quaternion,
  frustumCulling: boolean,
  visibleWorldChunks: Set<string>,
  worldChunkSize: number
): PlannedSpriteBatch[] {
  const visibleBatches = batches
    .map((batch) => ({
      ...batch,
      instances: batch.instances.filter((instance) => {
        const radius = estimateInstanceRadius(instance);
        const center = new Vector3(...instance.position);
        const distance = center.distanceTo(cameraPosition);

        if (distance - radius > spriteCullDistance) {
          return false;
        }

        const visible =
          !frustumCulling || frustum.intersectsSphere(new Sphere(center, radius));
        if (visible) {
          visibleWorldChunks.add(chunkKeyFromInstance(instance, worldChunkSize));
        }
        return visible;
      })
    }))
    .filter((batch) => batch.instances.length > 0);

  return splitSpriteBatchesByTint(visibleBatches).map((batch) => ({
    ...batch,
    visibleCount: batch.instances.length,
    matrices: batch.instances.map((instance) =>
      composeInstanceMatrix(
        instance,
        batch.batch.billboard ? cameraQuaternion : undefined
      )
    )
  }));
}

function classifyLod(
  distance: number,
  highDetailDistance: number,
  mediumDetailDistance: number
): 0 | 1 | 2 {
  if (distance <= highDetailDistance) {
    return 0;
  }

  if (distance <= mediumDetailDistance) {
    return 1;
  }

  return 2;
}

function estimateInstanceRadius(instance: ThreeJsInstance): number {
  return Math.max(...instance.scale) * DEFAULT_RADIUS;
}

function collectMeshPrewarmRequests(
  batches: ThreeJsMeshBatch[],
  preloadedChunkKeys: string[],
  cameraPosition: Vector3,
  highDetailDistance: number,
  mediumDetailDistance: number,
  meshPrewarmDistance: number,
  worldChunkSize: number
): PlannedMeshPrewarmRequest[] {
  const requests = new Map<string, PlannedMeshPrewarmRequest>();
  const preloadedChunkSet = new Set(preloadedChunkKeys);

  for (const batch of batches) {
    for (const instance of batch.instances) {
      const chunkKey = chunkKeyFromInstance(instance, worldChunkSize);
      if (!preloadedChunkSet.has(chunkKey)) {
        continue;
      }

      const distance = new Vector3(...instance.position).distanceTo(cameraPosition);
      if (distance - estimateInstanceRadius(instance) > meshPrewarmDistance) {
        continue;
      }

      const lodLevel = classifyLod(distance, highDetailDistance, mediumDetailDistance);
      const key = createMeshBatchKey(batch, lodLevel);
      if (!requests.has(key)) {
        requests.set(key, {
          key,
          batch,
          lodLevel,
          chunkKey
        });
      }
    }
  }

  return Array.from(requests.values()).sort((left, right) => left.key.localeCompare(right.key));
}

function collectSpritePrewarmRequests(
  batches: ThreeJsSpriteBatch[],
  preloadedChunkKeys: string[],
  spritePrewarmDistance: number,
  cameraPosition: Vector3,
  worldChunkSize: number
): PlannedSpritePrewarmRequest[] {
  const requests = new Map<string, PlannedSpritePrewarmRequest>();
  const preloadedChunkSet = new Set(preloadedChunkKeys);

  for (const batch of batches) {
    for (const instance of batch.instances) {
      const chunkKey = chunkKeyFromInstance(instance, worldChunkSize);
      if (!preloadedChunkSet.has(chunkKey)) {
        continue;
      }

      const distance = new Vector3(...instance.position).distanceTo(cameraPosition);
      if (distance - estimateInstanceRadius(instance) > spritePrewarmDistance) {
        continue;
      }

      const key = `${batch.texture}:frame:${batch.frame}`;
      if (!requests.has(key)) {
        requests.set(key, {
          key,
          batch: {
            texture: batch.texture,
            frame: batch.frame
          },
          chunkKey
        });
      }
    }
  }

  return Array.from(requests.values()).sort((left, right) => left.key.localeCompare(right.key));
}

function expandChunkKeys(
  sourceKeys: Iterable<string>,
  radius: number,
  fallbackChunkKey: string
): string[] {
  const expanded = new Set<string>();
  const normalizedRadius = Math.max(Math.floor(radius), 0);
  let didExpand = false;

  for (const sourceKey of sourceKeys) {
    didExpand = true;
    const [chunkX, chunkZ] = parseChunkKey(sourceKey);
    for (let offsetX = -normalizedRadius; offsetX <= normalizedRadius; offsetX += 1) {
      for (let offsetZ = -normalizedRadius; offsetZ <= normalizedRadius; offsetZ += 1) {
        expanded.add(formatChunkKey(chunkX + offsetX, chunkZ + offsetZ));
      }
    }
  }

  if (!didExpand) {
    expanded.add(fallbackChunkKey);
  }

  return Array.from(expanded).sort((left, right) => left.localeCompare(right));
}

function chunkKeyFromInstance(
  instance: Pick<ThreeJsInstance, "position">,
  worldChunkSize: number
): string {
  return chunkKeyFromCoordinates(instance.position[0], instance.position[2], worldChunkSize);
}

function chunkKeyFromCoordinates(x: number, z: number, worldChunkSize: number): string {
  return formatChunkKey(
    Math.floor(x / Math.max(worldChunkSize, 1)),
    Math.floor(z / Math.max(worldChunkSize, 1))
  );
}

function formatChunkKey(chunkX: number, chunkZ: number): string {
  return `${chunkX}:${chunkZ}`;
}

function parseChunkKey(chunkKey: string): [number, number] {
  const [rawX = "0", rawZ = "0"] = chunkKey.split(":");
  return [Number.parseInt(rawX, 10) || 0, Number.parseInt(rawZ, 10) || 0];
}
