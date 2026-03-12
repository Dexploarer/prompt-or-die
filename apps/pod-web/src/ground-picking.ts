import * as THREE from "three";

import type { CameraState, NetworkEntitySnapshot, Vec2Tuple } from "./contracts";
import { buildCameraPose } from "./frame-plan";
import { sampleSurfaceHeight } from "./landscape";

export function pickWorldGroundPoint(
  pointer: Vec2Tuple,
  viewport: { width: number; height: number },
  cameraState: CameraState
): Vec2Tuple | null {
  if (viewport.width <= 0 || viewport.height <= 0) {
    return null;
  }

  const pose = buildCameraPose(cameraState, {});
  const camera = new THREE.PerspectiveCamera(
    pose.fov,
    viewport.width / Math.max(viewport.height, 1),
    pose.near,
    pose.far
  );
  camera.position.set(...pose.position);
  camera.quaternion.copy(pose.quaternion);
  camera.updateMatrixWorld(true);
  camera.updateProjectionMatrix();

  const ndc = new THREE.Vector2(
    (pointer[0] / viewport.width) * 2 - 1,
    -((pointer[1] / viewport.height) * 2 - 1)
  );
  const raycaster = new THREE.Raycaster();
  raycaster.setFromCamera(ndc, camera);

  const terrainDelta = (distance: number): number => {
    const point = raycaster.ray.at(distance, new THREE.Vector3());
    return point.y - sampleSurfaceHeight(point.x, point.z);
  };

  let previousDistance = camera.near;
  let previousDelta = terrainDelta(previousDistance);

  if (previousDelta <= 0) {
    const point = raycaster.ray.at(previousDistance, new THREE.Vector3());
    return [point.x, point.z];
  }

  const maxDistance = Math.min(camera.far, 420);
  const stepCount = 240;

  for (let step = 1; step <= stepCount; step += 1) {
    const distance = (maxDistance * step) / stepCount;
    const delta = terrainDelta(distance);

    if (delta <= 0) {
      let low = previousDistance;
      let high = distance;

      for (let iteration = 0; iteration < 10; iteration += 1) {
        const middle = (low + high) * 0.5;
        if (terrainDelta(middle) > 0) {
          low = middle;
        } else {
          high = middle;
        }
      }

      const hitPoint = raycaster.ray.at(high, new THREE.Vector3());
      return [hitPoint.x, hitPoint.z];
    }

    previousDistance = distance;
    previousDelta = delta;
  }

  if (previousDelta > 0) {
    return null;
  }

  const hitPoint = raycaster.ray.at(previousDistance, new THREE.Vector3());
  return [hitPoint.x, hitPoint.z];
}

export function resolvePointerTarget(
  entities: NetworkEntitySnapshot[],
  worldPoint: Vec2Tuple,
  extraRadius = 0.9
): NetworkEntitySnapshot | null {
  let selected: { entity: NetworkEntitySnapshot; distance: number } | null = null;

  for (const entity of entities) {
    const dx = entity.position[0] - worldPoint[0];
    const dy = entity.position[1] - worldPoint[1];
    const distance = Math.hypot(dx, dy);
    const footprint = entity.metadata.actorPresentation?.footprintRadius ?? 1.1;
    const threshold = footprint + extraRadius;

    if (distance > threshold) {
      continue;
    }

    if (!selected || distance < selected.distance) {
      selected = { entity, distance };
    }
  }

  return selected?.entity ?? null;
}
