import type { NetworkEntitySnapshot } from "./contracts";

export type EntitySurfaceMode = "ground" | "swim";

export function surfaceModeFromEntity(
  entity: NetworkEntitySnapshot | null | undefined
): EntitySurfaceMode {
  const animationSetId =
    entity?.metadata.actorPresentation?.animationSetId?.toLowerCase() ?? "";
  return animationSetId.includes("swim") ? "swim" : "ground";
}

export function entityUsesSwimSurface(
  entity: NetworkEntitySnapshot | null | undefined
): boolean {
  return surfaceModeFromEntity(entity) === "swim";
}
