import * as THREE from "three";

import type { CameraState, NetworkEntitySnapshot, Vec2Tuple } from "./contracts";
import { buildCameraPose } from "./frame-plan";
import { sampleSurfaceHeight } from "./landscape";

export interface GameplaySurface {
  tabIndex?: number;
  focus?: (options?: { preventScroll?: boolean }) => void;
  setAttribute?: (name: string, value: string) => void;
}

export interface CameraDirectionInput {
  yaw: number;
  pitch: number;
}

function normalizeDirection(x: number, y: number): Vec2Tuple | null {
  const length = Math.hypot(x, y);
  if (length <= Number.EPSILON) {
    return null;
  }

  return [x / length, y / length];
}

export function cameraRelativeMovementDirection(
  pressedKeys: Iterable<string>,
  cameraRotation: number
): Vec2Tuple | null {
  const activeKeys = new Set(pressedKeys);
  const forwardIntent =
    (activeKeys.has("KeyW") ? 1 : 0) -
    (activeKeys.has("KeyS") ? 1 : 0);
  const strafeIntent =
    (activeKeys.has("KeyD") ? 1 : 0) -
    (activeKeys.has("KeyA") ? 1 : 0);

  if (forwardIntent === 0 && strafeIntent === 0) {
    return null;
  }

  const forwardX = -Math.sin(cameraRotation);
  const forwardY = -Math.cos(cameraRotation);
  const rightX = Math.cos(cameraRotation);
  const rightY = -Math.sin(cameraRotation);

  return normalizeDirection(
    forwardX * forwardIntent + rightX * strafeIntent,
    forwardY * forwardIntent + rightY * strafeIntent
  );
}

export function cameraDirectionInput(
  pressedKeys: Iterable<string>
): CameraDirectionInput {
  const activeKeys = new Set(pressedKeys);
  return {
    yaw:
      (activeKeys.has("ArrowLeft") ? 1 : 0) -
      (activeKeys.has("ArrowRight") ? 1 : 0),
    pitch:
      (activeKeys.has("ArrowUp") ? 1 : 0) -
      (activeKeys.has("ArrowDown") ? 1 : 0)
  };
}

export function isGameplayKeyCode(code: string): boolean {
  return (
    code === "Tab" ||
    code === "Space" ||
    code === "Enter" ||
    code === "KeyE" ||
    code === "KeyG" ||
    code === "KeyR" ||
    code === "KeyC" ||
    code === "KeyF" ||
    code === "KeyP" ||
    code === "Digit1" ||
    code === "KeyW" ||
    code === "KeyA" ||
    code === "KeyS" ||
    code === "KeyD" ||
    code === "ArrowUp" ||
    code === "ArrowDown" ||
    code === "ArrowLeft" ||
    code === "ArrowRight"
  );
}

export function focusGameplaySurface(surface: GameplaySurface | null | undefined): boolean {
  if (!surface) {
    return false;
  }

  if (typeof surface.tabIndex === "number" && surface.tabIndex < 0) {
    surface.tabIndex = 0;
  }
  surface.setAttribute?.("aria-label", "Game world");
  surface.setAttribute?.("role", "application");
  surface.setAttribute?.("data-gameplay-surface", "true");

  if (typeof surface.focus === "function") {
    surface.focus({ preventScroll: true });
    return true;
  }

  return false;
}

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
