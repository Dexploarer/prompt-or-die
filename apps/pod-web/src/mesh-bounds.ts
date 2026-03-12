export interface PodThreeMeshBounds {
  minY: number;
  maxY: number;
  footprintRadius: number;
}

const DEFAULT_MESH_BOUNDS: PodThreeMeshBounds = {
  minY: -0.5,
  maxY: 0.5,
  footprintRadius: 0.7
};

const SHIPPED_MESH_BOUNDS: Record<string, PodThreeMeshBounds> = {
  "adventurer-avatar": {
    minY: -0.44,
    maxY: 0.5,
    footprintRadius: 0.28
  },
  "adventurer-hero": {
    minY: -0.48,
    maxY: 0.57,
    footprintRadius: 0.32
  },
  "basalt-column": {
    minY: -0.85,
    maxY: 0.82,
    footprintRadius: 0.46
  },
  "canopy-tree": {
    minY: -0.88,
    maxY: 0.86,
    footprintRadius: 0.62
  },
  "glass-spire": {
    minY: -0.72,
    maxY: 0.62,
    footprintRadius: 0.45
  },
  "rift-beast": {
    minY: -0.52,
    maxY: 0.34,
    footprintRadius: 0.72
  },
  "spirit-companion": {
    minY: -0.2,
    maxY: 0.34,
    footprintRadius: 0.4
  },
  "supply-crate": {
    minY: -0.24,
    maxY: 0.23,
    footprintRadius: 0.39
  },
  "weathered-boulder": {
    minY: -0.44,
    maxY: 0.4,
    footprintRadius: 0.62
  }
};

export function normalizeMeshBoundsKey(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

export function resolveMeshBounds(meshId: string): PodThreeMeshBounds {
  const normalized = normalizeMeshBoundsKey(meshId);
  return SHIPPED_MESH_BOUNDS[normalized] ?? DEFAULT_MESH_BOUNDS;
}

export function meshGroundAnchorHeight(meshId: string, scaleY = 1): number {
  const bounds = resolveMeshBounds(meshId);
  return -bounds.minY * scaleY;
}

export function meshVisualHeight(meshId: string, scaleY = 1): number {
  const bounds = resolveMeshBounds(meshId);
  return (bounds.maxY - bounds.minY) * scaleY;
}
