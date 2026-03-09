import type { DirectConnectStatus } from "./direct-connect";
import type {
  NetworkGameEvent,
  ShardTransportSummaryDocument
} from "./contracts";
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
    return `Hit confirmed · ${event.summary}`;
  }
  if (kind.includes("kill")) {
    return `Target down · ${event.summary}`;
  }
  if (kind.includes("spawn")) {
    return `Back in the fight · ${event.summary}`;
  }
  if (kind.includes("capture")) {
    return `Capture secured · ${event.summary}`;
  }
  if (kind.includes("loot")) {
    return `Loot claimed · ${event.summary}`;
  }
  if (kind.includes("gather")) {
    return `Resource gathered · ${event.summary}`;
  }
  if (kind.includes("summon")) {
    return `Companion ready · ${event.summary}`;
  }
  if (kind.includes("command")) {
    return `Companion order · ${event.summary}`;
  }
  if (kind.includes("quest")) {
    return `Quest progress · ${event.summary}`;
  }

  return event.summary;
}

export function formatConnectionSummary(
  status: DirectConnectStatus | null,
  transport?: ShardTransportSummaryDocument | null
): string {
  if (!status) {
    return "offline demo / bridge mode";
  }

  const network =
    status.roundTripMs == null
      ? null
      : `net ${status.roundTripMs.toFixed(0)}ms rtt${
          status.jitterMs == null ? "" : ` / ${status.jitterMs.toFixed(0)}ms jitter`
        }`;

  const shardTransport =
    transport == null
      ? null
      : [
          `shard ${transport.client_count}c`,
          transport.resumed_sessions > 0 ? `resumes ${transport.resumed_sessions}` : null,
          transport.recovery_snapshots_sent > 0
            ? `recover ${transport.recovery_snapshots_sent}`
            : null,
          transport.recovery_delivery_failures > 0
            ? `recover-fail ${transport.recovery_delivery_failures}`
            : null,
          `q${transport.total_pending_action_queue_depth}`,
          transport.queue_pressure_client_count > 0
            ? `pressure ${transport.queue_pressure_client_count}`
            : null,
          transport.timed_out_clients > 0
            ? `timeouts ${transport.timed_out_clients}`
            : null,
          `${formatKilo(transport.total_outbound_bytes)}B out`
        ]
          .filter(Boolean)
          .join(" / ");

  return [status.phase, status.detail, network, shardTransport]
    .filter(Boolean)
    .join(" · ");
}
