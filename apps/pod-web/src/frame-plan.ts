import { Matrix4, Quaternion, Vector3 } from "three";

import type {
  CameraState,
  RgbaTuple,
  ThreeJsInstance,
  ThreeJsMeshBatch,
  ThreeJsSpriteBatch,
  ThreeJsWebGpuFrame
} from "./contracts";

const DEFAULT_UP = new Vector3(0, 1, 0);

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
  matrices: Matrix4[];
}

export interface PlannedSpriteBatch {
  key: string;
  batch: Omit<ThreeJsSpriteBatch, "instances">;
  tint: RgbaTuple;
  instances: ThreeJsInstance[];
  matrices: Matrix4[];
}

export interface PlannedFrame {
  camera: PlannedCameraPose;
  meshBatches: PlannedMeshBatch[];
  spriteBatches: PlannedSpriteBatch[];
}

export function buildCameraPose(
  camera: CameraState,
  options: PodThreeCameraRigOptions = {}
): PlannedCameraPose {
  const pitch = options.pitch ?? Math.PI / 5;
  const baseDistance = options.baseDistance ?? 54;
  const minDistance = options.minDistance ?? 12;
  const maxDistance = options.maxDistance ?? 180;
  const distance = clamp(baseDistance / Math.max(camera.zoom, 0.15), minDistance, maxDistance);
  const heightOffset = options.height ?? 12;
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
    fov: options.fov ?? 55,
    near: options.near ?? 0.1,
    far: options.far ?? 1024
  };
}

export function buildFramePlan(
  frame: ThreeJsWebGpuFrame,
  options: PodThreeCameraRigOptions = {}
): PlannedFrame {
  const camera = buildCameraPose(frame.camera, options);

  return {
    camera,
    meshBatches: frame.meshBatches.map((batch) => ({
      key: createMeshBatchKey(batch),
      batch,
      matrices: batch.instances.map((instance) => composeInstanceMatrix(instance))
    })),
    spriteBatches: splitSpriteBatchesByTint(frame.spriteBatches).map((batch) => ({
      ...batch,
      matrices: batch.instances.map((instance) =>
        composeInstanceMatrix(
          instance,
          batch.batch.billboard ? camera.quaternion : undefined
        )
      )
    }))
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

function createMeshBatchKey(batch: ThreeJsMeshBatch): string {
  return [
    "mesh",
    batch.mesh,
    batch.material,
    batch.layer,
    batch.renderOrder,
    batch.phase,
    batch.transparent ? "transparent" : "opaque"
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
