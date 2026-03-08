import type { NetworkGameEvent } from "./contracts";
import type { PodThreeRendererStats } from "./renderer";

function formatKilo(value: number): string {
  if (value >= 1000) {
    return `${(value / 1000).toFixed(value >= 10000 ? 0 : 1)}k`;
  }
  return `${value}`;
}

export function compactRuntimeStats(stats: PodThreeRendererStats): string {
  return [
    `${stats.backend}/${stats.renderThread}`,
    `${stats.environmentPreset} ${stats.timeOfDayHours.toFixed(1)}h`,
    `${stats.frameMs.toFixed(1)}ms @ ${stats.pixelRatio.toFixed(2)}x`,
    `${stats.drawCalls} draw / ${formatKilo(stats.triangles)} tris`,
    `${stats.visibleWorldChunks}/${stats.preloadedWorldChunks} chunks`,
    `${stats.residentGeometryAssets + stats.residentSpriteAssets}/${
      stats.pendingGeometryAssets + stats.pendingSpriteAssets
    } assets`,
    `ambient ${stats.ambientInstances}`
  ].join(" · ");
}

export function highlightEventFeedback(event: NetworkGameEvent | null): string {
  if (!event) {
    return "No authoritative world events yet";
  }

  const kind = event.kind.toLowerCase();
  if (kind.includes("damage")) {
    return `Combat hit · ${event.summary}`;
  }
  if (kind.includes("kill")) {
    return `Target down · ${event.summary}`;
  }
  if (kind.includes("capture")) {
    return `Capture result · ${event.summary}`;
  }
  if (kind.includes("loot")) {
    return `Loot secured · ${event.summary}`;
  }
  if (kind.includes("gather")) {
    return `Gather progress · ${event.summary}`;
  }
  if (kind.includes("summon")) {
    return `Companion active · ${event.summary}`;
  }

  return event.summary;
}
