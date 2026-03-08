import type { NetworkChunkPopulationState, NetworkWorldPopulationState } from "./contracts";

export interface PopulationHeatmapFocus {
  chunkKey?: string | null;
  regionId?: string | null;
}

export interface PopulationHeatmapCell {
  chunkKey: string;
  regionId: string | null;
  regionName: string | null;
  gridX: number;
  gridY: number;
  activeEntityCount: number;
  ambientPopulationCap: number;
  spawnBudgetRemaining: number;
  pendingRespawns: number;
  nextRespawnTick: number | null;
  pressure: number;
  intensity: number;
  isFocused: boolean;
  isFocusedRegion: boolean;
}

export interface PopulationHeatmapModel {
  cells: PopulationHeatmapCell[];
  columns: number;
  rows: number;
  minGridX: number;
  maxGridX: number;
  minGridY: number;
  maxGridY: number;
  maxPendingRespawns: number;
  focusedCell: PopulationHeatmapCell | null;
  focusedRegionId: string | null;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

export function parsePopulationChunkKey(chunkKey: string): [number, number] | null {
  const [rawX, rawY] = chunkKey.split(":");
  if (rawX == null || rawY == null) {
    return null;
  }

  const x = Number.parseInt(rawX, 10);
  const y = Number.parseInt(rawY, 10);
  if (!Number.isFinite(x) || !Number.isFinite(y)) {
    return null;
  }

  return [x, y];
}

function chunkIntensity(
  chunk: NetworkChunkPopulationState,
  maxPendingRespawns: number
): number {
  const pressure = clamp(chunk.populationPressure, 0, 1.5);
  const respawnPressure =
    maxPendingRespawns <= 0 ? 0 : clamp(chunk.pendingRespawns / maxPendingRespawns, 0, 1);
  const occupancy =
    chunk.activeEntityCount > 0 || chunk.pendingRespawns > 0 ? 0.12 : 0.04;
  return clamp(Math.max(occupancy, pressure * 0.74 + respawnPressure * 0.26), 0, 1);
}

export function buildPopulationHeatmapModel(
  population: NetworkWorldPopulationState | null | undefined,
  focus: PopulationHeatmapFocus = {}
): PopulationHeatmapModel | null {
  if (!population || population.chunks.length === 0) {
    return null;
  }

  const parsedChunks = population.chunks
    .map((chunk) => {
      const grid = parsePopulationChunkKey(chunk.chunkKey);
      return grid ? { chunk, gridX: grid[0], gridY: grid[1] } : null;
    })
    .filter((chunk): chunk is { chunk: NetworkChunkPopulationState; gridX: number; gridY: number } => chunk != null);

  if (parsedChunks.length === 0) {
    return null;
  }

  const maxPendingRespawns = parsedChunks.reduce(
    (max, entry) => Math.max(max, entry.chunk.pendingRespawns),
    0
  );
  const minGridX = parsedChunks.reduce((min, entry) => Math.min(min, entry.gridX), Infinity);
  const maxGridX = parsedChunks.reduce((max, entry) => Math.max(max, entry.gridX), -Infinity);
  const minGridY = parsedChunks.reduce((min, entry) => Math.min(min, entry.gridY), Infinity);
  const maxGridY = parsedChunks.reduce((max, entry) => Math.max(max, entry.gridY), -Infinity);

  const cells = parsedChunks
    .map(({ chunk, gridX, gridY }) => ({
      chunkKey: chunk.chunkKey,
      regionId: chunk.regionId,
      regionName: chunk.regionName,
      gridX,
      gridY,
      activeEntityCount: chunk.activeEntityCount,
      ambientPopulationCap: chunk.ambientPopulationCap,
      spawnBudgetRemaining: chunk.spawnBudgetRemaining,
      pendingRespawns: chunk.pendingRespawns,
      nextRespawnTick: chunk.nextRespawnTick,
      pressure: chunk.populationPressure,
      intensity: chunkIntensity(chunk, maxPendingRespawns),
      isFocused: focus.chunkKey != null && chunk.chunkKey === focus.chunkKey,
      isFocusedRegion: focus.regionId != null && chunk.regionId === focus.regionId
    }))
    .sort((left, right) => {
      if (left.gridY !== right.gridY) {
        return left.gridY - right.gridY;
      }
      return left.gridX - right.gridX;
    });

  const focusedCell =
    cells.find((cell) => cell.isFocused) ??
    cells
      .filter((cell) => cell.isFocusedRegion)
      .sort((left, right) => right.intensity - left.intensity)[0] ??
    [...cells].sort((left, right) => right.intensity - left.intensity)[0] ??
    null;

  return {
    cells,
    columns: maxGridX - minGridX + 1,
    rows: maxGridY - minGridY + 1,
    minGridX,
    maxGridX,
    minGridY,
    maxGridY,
    maxPendingRespawns,
    focusedCell,
    focusedRegionId: focus.regionId ?? null
  };
}

function heatmapColor(cell: PopulationHeatmapCell, model: PopulationHeatmapModel): string {
  const hot = cell.intensity;
  const respawn =
    model.maxPendingRespawns <= 0
      ? 0
      : clamp(cell.pendingRespawns / model.maxPendingRespawns, 0, 1);
  const red = Math.round(34 + hot * 186 + respawn * 24);
  const green = Math.round(44 + (1 - hot) * 120 + respawn * 48);
  const blue = Math.round(72 + (1 - hot) * 82 + respawn * 36);
  const alpha = cell.isFocused ? 0.96 : cell.isFocusedRegion ? 0.82 : 0.72;
  return `rgba(${red}, ${green}, ${blue}, ${alpha})`;
}

export function formatPopulationHeatmapLegend(
  model: PopulationHeatmapModel | null
): string {
  if (!model || !model.focusedCell) {
    return "Awaiting shard population";
  }

  const cell = model.focusedCell;
  const region = cell.regionName ?? cell.regionId ?? "unassigned region";
  const respawns =
    cell.pendingRespawns > 0
      ? ` · respawns ${cell.pendingRespawns}${
          cell.nextRespawnTick != null ? ` @${cell.nextRespawnTick}` : ""
        }`
      : "";
  return `${cell.chunkKey} · ${region} · pressure ${cell.pressure.toFixed(2)}${respawns}`;
}

export function renderPopulationHeatmap(
  canvas: HTMLCanvasElement,
  model: PopulationHeatmapModel | null
): void {
  const context = canvas.getContext("2d");
  if (!context) {
    return;
  }

  const cssWidth = Math.max(1, Math.round(canvas.clientWidth || 320));
  const cssHeight = Math.max(1, Math.round(canvas.clientHeight || 138));
  const pixelRatio = Math.max(1, Math.floor(window.devicePixelRatio || 1));
  const deviceWidth = cssWidth * pixelRatio;
  const deviceHeight = cssHeight * pixelRatio;
  if (canvas.width !== deviceWidth || canvas.height !== deviceHeight) {
    canvas.width = deviceWidth;
    canvas.height = deviceHeight;
  }

  context.setTransform(1, 0, 0, 1, 0, 0);
  context.clearRect(0, 0, canvas.width, canvas.height);
  context.scale(pixelRatio, pixelRatio);

  context.fillStyle = "rgba(5, 10, 18, 0.92)";
  context.fillRect(0, 0, cssWidth, cssHeight);

  if (!model || model.cells.length === 0) {
    context.fillStyle = "rgba(156, 166, 182, 0.9)";
    context.font = '12px "IBM Plex Sans", sans-serif';
    context.fillText("Awaiting authoritative population", 14, 22);
    return;
  }

  const padding = 12;
  const gap = 6;
  const cellWidth =
    (cssWidth - padding * 2 - gap * Math.max(model.columns - 1, 0)) / model.columns;
  const cellHeight =
    (cssHeight - padding * 2 - gap * Math.max(model.rows - 1, 0)) / model.rows;
  const labelAllowed = cellWidth >= 32 && cellHeight >= 26;

  for (const cell of model.cells) {
    const column = cell.gridX - model.minGridX;
    const row = model.maxGridY - cell.gridY;
    const x = padding + column * (cellWidth + gap);
    const y = padding + row * (cellHeight + gap);

    context.fillStyle = heatmapColor(cell, model);
    context.fillRect(x, y, cellWidth, cellHeight);

    if (cell.pendingRespawns > 0) {
      context.fillStyle = "rgba(255, 214, 92, 0.95)";
      context.fillRect(x + cellWidth - 10, y + 2, 8, Math.max(6, cellHeight * 0.32));
    }

    context.lineWidth = cell.isFocused ? 2.5 : cell.isFocusedRegion ? 1.6 : 1;
    context.strokeStyle = cell.isFocused
      ? "rgba(242, 244, 247, 0.98)"
      : cell.isFocusedRegion
        ? "rgba(138, 245, 207, 0.86)"
        : "rgba(255, 255, 255, 0.10)";
    context.strokeRect(x + 0.5, y + 0.5, cellWidth - 1, cellHeight - 1);

    if (labelAllowed) {
      context.fillStyle = "rgba(242, 244, 247, 0.94)";
      context.font = '11px "IBM Plex Sans", sans-serif';
      context.fillText(cell.chunkKey, x + 6, y + 14);
      context.fillStyle = "rgba(138, 245, 207, 0.92)";
      context.fillText(`${cell.activeEntityCount}`, x + 6, y + cellHeight - 8);
    }
  }
}
