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

function formatLoadStat(value: number): string {
  return value < 10 ? value.toFixed(1) : value.toFixed(0);
}

function formatByteStat(value: number): string {
  if (value >= 1000) {
    return `${formatKilo(value)}B`;
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)}B`;
}

function formatAverageByteStat(totalBytes: number, samples: number): string {
  if (samples <= 0) {
    return "0B";
  }
  return formatByteStat(totalBytes / samples);
}

function formatWarmupStat(value: number | null): string {
  if (value == null) {
    return "warming";
  }
  if (value >= 1000) {
    return `${(value / 1000).toFixed(value >= 10_000 ? 0 : 1)}s`;
  }
  return `${value.toFixed(value >= 100 ? 0 : 1)}ms`;
}

function formatSubmissionStat(value: number): string {
  return value < 10 ? value.toFixed(1) : value.toFixed(0);
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
    `load ${formatLoadStat(stats.averageGeometryLoadMs)}/${formatLoadStat(
      stats.averageSpriteLoadMs
    )}ms`,
    `submit ${formatSubmissionStat(stats.mainThreadPerf.averageSubmissionMs)}ms`,
    `warm ${formatWarmupStat(stats.runtimePerf.warmupMs)}`,
    `stable ${stats.runtimePerf.stableFramePercent.toFixed(0)}%`
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

export function formatTransportDebugSummary(
  transport: ShardTransportSummaryDocument,
  sampleCount: number
): string {
  return [
    "transport",
    `${transport.client_count} clients`,
    `snap ${transport.full_snapshots_sent}x ${formatAverageByteStat(
      transport.total_full_snapshot_bytes,
      transport.full_snapshots_sent
    )} avg / ${formatByteStat(transport.max_full_snapshot_bytes)} max`,
    `delta ${transport.delta_messages_sent}x ${formatAverageByteStat(
      transport.total_delta_bytes,
      transport.delta_messages_sent
    )} avg / +${formatKilo(transport.total_delta_entities_updated)}/-${formatKilo(
      transport.total_delta_entities_destroyed
    )}`,
    `recover ${transport.recovery_snapshots_sent} / ${formatByteStat(
      transport.total_recovery_snapshot_bytes
    )}`,
    `q${transport.total_pending_action_queue_depth} now / peak ${transport.peak_pending_action_queue_depth}`,
    `pressure ${transport.queue_pressure_client_count} (${transport.queue_pressure_events})`,
    `timeouts ${transport.timed_out_clients}`,
    `${sampleCount} samples`
  ].join(" · ");
}
