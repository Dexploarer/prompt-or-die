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
    minY: -1.1,
    maxY: 1.1,
    footprintRadius: 0.45
  },
  "adventurer-hero": {
    minY: -1.205,
    maxY: 1.205,
    footprintRadius: 0.48
  },
  "basalt-column": {
    minY: -1.6,
    maxY: 1.6,
    footprintRadius: 0.9
  },
  "canopy-tree": {
    minY: -1.2,
    maxY: 1.2,
    footprintRadius: 1.1
  },
  "glass-spire": {
    minY: -1.5,
    maxY: 1.5,
    footprintRadius: 0.85
  },
  "rift-beast": {
    minY: -1.1,
    maxY: 1.1,
    footprintRadius: 1.1
  },
  "spirit-companion": {
    minY: -0.8081182837486267,
    maxY: 0.8081182837486267,
    footprintRadius: 0.81
  },
  "supply-crate": {
    minY: -0.55,
    maxY: 0.55,
    footprintRadius: 0.7
  },
  "weathered-boulder": {
    minY: -0.9341723322868347,
    maxY: 0.9341723322868347,
    footprintRadius: 0.94
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
