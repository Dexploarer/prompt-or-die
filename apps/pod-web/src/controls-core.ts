import type { Vec2Tuple } from "./contracts";

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
