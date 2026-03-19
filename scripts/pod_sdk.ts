import {
  DELIMITERS,
  decodeFromLines,
  decodeStreamSync,
  encodeLines,
  type DelimiterKey,
  type JsonStreamEvent,
  type JsonValue,
} from "@toon-format/toon";

export const POD_EXPORT_TARGETS = ["world", "events", "multiverse"] as const;
export const POD_EXPORT_FORMATS = ["json", "toon"] as const;

export type PodExportTarget = (typeof POD_EXPORT_TARGETS)[number];
export type PodExportFormat = (typeof POD_EXPORT_FORMATS)[number];

export type PodExportEnvelope = {
  schemaVersion: 1;
  generatedAtUnixMs: number;
  target: PodExportTarget;
  description: string;
  documentType: string;
  preferredFormat: PodExportFormat;
  preferredToonDelimiter: DelimiterKey | null;
  format: PodExportFormat;
  contentType: string;
  byteLength: number;
  lineCount: number;
  text: string;
};

export type PodBenchmarkDatasetId =
  | "uniform_tick_event_batch"
  | "toonscape_donor_tick_event_batch"
  | "semi_uniform_agent_logs"
  | "nested_world_snapshot"
  | "deep_multiverse_index";

export type PodBenchmarkDataset = {
  id: PodBenchmarkDatasetId;
  description: string;
  family:
    | "uniform-records"
    | "semi-uniform-logs"
    | "nested-world-snapshot"
    | "deep-multiverse-tree";
  exportTarget: PodExportTarget | null;
  preferredFormat: PodExportFormat;
  preferredToonDelimiter: DelimiterKey | null;
  value: JsonValue;
};

const EXPORT_SCHEMA_VERSION = 1;
const FIXED_GENERATED_AT_UNIX_MS = Date.UTC(2026, 2, 18, 12, 0, 0);
const TOON_ENCODE_INDENT = 2;
const TOON_ENCODE_KEY_FOLDING = "safe" as const;
const TOON_DECODE_OPTIONS = {
  indent: TOON_ENCODE_INDENT,
  strict: true,
  expandPaths: "safe",
} as const;
const TOON_STREAM_DECODE_OPTIONS = {
  indent: TOON_ENCODE_INDENT,
  strict: true,
} as const;

type ExportDataset = {
  target: PodExportTarget;
  description: string;
  documentType: string;
  preferredFormat: PodExportFormat;
  preferredToonDelimiter: DelimiterKey | null;
  value: JsonValue;
};

function countLines(text: string): number {
  if (text.length === 0) {
    return 0;
  }
  return text.split("\n").length;
}

function createEntity(index: number) {
  return {
    entity_id: `entity-${String(index + 1).padStart(3, "0")}`,
    agent_name: `operator-${String(index + 1).padStart(2, "0")}`,
    agent_type:
      index % 4 === 0
        ? "llm_agent"
        : index % 4 === 1
          ? "scripted_agent"
          : index % 4 === 2
            ? "neural_agent"
            : "human_player",
    team_id: `team-${(index % 3) + 1}`,
    zone_id: `zone-${(index % 4) + 1}`,
    position_x: Number((12.5 + index * 2.75).toFixed(2)),
    position_y: Number((8.25 + index * 1.5).toFixed(2)),
    health: 100 - (index % 6) * 7,
    energy: 42 + (index % 5) * 9,
    objective_id: `objective-${(index % 6) + 1}`,
    threat_level:
      index % 3 === 0 ? "high" : index % 3 === 1 ? "medium" : "low",
    inventory_weight: Number((18.2 + index * 0.9).toFixed(1)),
    last_action_tick: 118_800 + index * 3,
  };
}

function createObjective(index: number) {
  return {
    objective_id: `objective-${index + 1}`,
    label: index % 2 === 0 ? "secure_anchor" : "escort_convoy",
    stage:
      index % 4 === 0
        ? "contested"
        : index % 4 === 1
          ? "secured"
          : index % 4 === 2
            ? "infiltrating"
            : "staging",
    progress: Number((0.18 + index * 0.11).toFixed(2)),
    owning_team_id: `team-${(index % 3) + 1}`,
    remaining_ticks: 480 - index * 24,
  };
}

function createRecentEvent(index: number) {
  return {
    tick: 118_920 + index,
    event_type: index % 2 === 0 ? "observation" : "combat_resolution",
    actor_entity_id: `entity-${String((index % 12) + 1).padStart(3, "0")}`,
    target_entity_id: `entity-${String(((index + 5) % 12) + 1).padStart(3, "0")}`,
    accepted: index % 3 !== 0,
    score_delta: index % 2 === 0 ? 4 - (index % 5) : -2 + (index % 4),
  };
}

function buildEventsExportDataset(eventCount = 48): ExportDataset {
  const tickEventBatch = Array.from({ length: eventCount }, (_, index) => ({
    tick: 118_900 + index,
    world_id: `world-frontier-${(index % 2) + 1}`,
    branch_id: `branch-${Math.floor(index / 12) + 1}`,
    entity_id: `entity-${String((index % 24) + 1).padStart(3, "0")}`,
    team_id: `team-${(index % 3) + 1}`,
    agent_type:
      index % 4 === 0
        ? "llm_agent"
        : index % 4 === 1
          ? "scripted_agent"
          : index % 4 === 2
            ? "neural_agent"
            : "system",
    event_type:
      index % 3 === 0
        ? "observation"
        : index % 3 === 1
          ? "action_validation"
          : "combat_resolution",
    action:
      index % 4 === 0
        ? "probe_link"
        : index % 4 === 1
          ? "stabilize_anchor"
          : index % 4 === 2
            ? "escort_payload"
            : "redirect_swarm",
    result:
      index % 5 === 0
        ? "rejected"
        : index % 5 === 1
          ? "accepted"
          : index % 5 === 2
            ? "resolved"
            : index % 5 === 3
              ? "deferred"
              : "accepted",
    accepted: index % 5 !== 0,
    score_delta: index % 2 === 0 ? 3 + (index % 4) : -2 + (index % 3),
    health_after: 98 - (index % 7) * 6,
    latency_ms: Number((9.25 + (index % 6) * 2.1).toFixed(2)),
    target_world_id: index % 3 === 0 ? "world-frontier-2" : "world-frontier-1",
    target_entity_id: `entity-${String(((index + 7) % 24) + 1).padStart(3, "0")}`,
    observation:
      index % 2 === 0 ? "anchored_signal" : "pressure_spike",
  }));

  return {
    target: "events",
    description:
      "Stable tick/event batch export for agent replay, validation, and world-state compression.",
    documentType: "tick_event_batch",
    preferredFormat: "toon",
    preferredToonDelimiter: "tab",
    value: {
      document_type: "tick_event_batch",
      schema_version: EXPORT_SCHEMA_VERSION,
      generated_at_unix_ms: FIXED_GENERATED_AT_UNIX_MS,
      world_id: "world-frontier-1",
      tick_window: {
        start_tick: 118_900,
        end_tick: 118_900 + eventCount - 1,
      },
      tick_event_batch: tickEventBatch,
    },
  };
}

function buildToonscapeDonorDataset(eventCount = 192): PodBenchmarkDataset {
  const events = Array.from({ length: eventCount }, (_, index) => ({
    event_type: index % 6,
    actor_id: (index % 24) + 1,
    actor_type: index % 4,
    tick: 118_900 + Math.floor(index / 3),
    x: 130 + (index % 5),
    y: 131 + (index % 7),
    z: 0,
    sequence: index % 3,
    target_id: index % 2 === 0 ? ((index + 3) % 24) + 1 : null,
    target_type: index % 2 === 0 ? 1 : null,
    damage: index % 6 === 5 ? 0 : null,
    weapon_id: index % 6 === 5 ? index % 4 : null,
    spell_id: index % 8 === 0 ? index % 3 : null,
    attack_style: index % 6 === 5 ? index % 3 : null,
    hit_chance:
      index % 6 === 5
        ? Number((0.42 + (index % 5) * 0.09).toFixed(3))
        : null,
    max_hit: index % 6 === 5 ? 1 + (index % 4) : null,
    hp_before: index % 6 === 5 ? 3 + (index % 6) : null,
    hp_after: index % 6 === 5 ? 2 + (index % 6) : null,
    attack_roll: index % 6 === 5 ? 640 + index * 2 : null,
    defence_roll: index % 6 === 5 ? 512 + index * 2 : null,
    is_death: index % 17 === 0 ? true : null,
    xp_awarded: index % 6 === 5 ? 4 + (index % 7) : null,
    combat_duration: index % 6 === 5 ? 1 + (index % 4) : null,
    skill_type: index % 6 === 1 ? index % 5 : null,
    xp_gained: index % 6 === 1 ? 10 + (index % 20) : null,
    level_before: index % 6 === 1 ? 8 + (index % 4) : null,
    level_after: index % 18 === 0 ? 9 + (index % 4) : null,
    tool_id: index % 6 === 1 ? index % 5 : null,
    object_id: index % 6 === 1 ? 200 + (index % 9) : null,
    success: index % 6 === 1 ? index % 4 !== 0 : null,
    result_item_id: index % 6 === 1 ? 400 + (index % 6) : null,
    item_id: index % 6 === 2 ? 700 + (index % 8) : null,
    quantity: index % 6 === 2 ? 1 + (index % 3) : null,
    slot: index % 6 === 2 ? index % 28 : null,
    inventory_slots_used: index % 6 === 2 ? 1 : null,
    item_value: index % 6 === 2 ? 18 + (index % 12) : null,
    nearby_count: 1 + (index % 3),
    zone_id: index % 6,
    caused_by_event: index % 3 === 0 ? null : index - 1,
    metadata_flags: index % 4,
  }));

  return {
    id: "toonscape_donor_tick_event_batch",
    description:
      "Wide, null-heavy donor event batch modeled after Toonscape's combat/event schema.",
    family: "uniform-records",
    exportTarget: null,
    preferredFormat: "toon",
    preferredToonDelimiter: "tab",
    value: {
      document_type: "toonscape_donor_tick_event_batch",
      schema_version: EXPORT_SCHEMA_VERSION,
      generated_at_unix_ms: FIXED_GENERATED_AT_UNIX_MS,
      events,
    },
  };
}

function buildWorldExportDataset(entityCount = 18): ExportDataset {
  const entities = Array.from({ length: entityCount }, (_, index) =>
    createEntity(index),
  );
  const objectives = Array.from({ length: 6 }, (_, index) =>
    createObjective(index),
  );
  const zones = Array.from({ length: 4 }, (_, index) => ({
    zone_id: `zone-${index + 1}`,
    biome: index % 2 === 0 ? "storm-marsh" : "basalt-ridge",
    controller_team_id: `team-${(index % 3) + 1}`,
    stability: Number((0.61 + index * 0.08).toFixed(2)),
    occupancy_entities: 3 + index * 2,
  }));
  const recentEvents = Array.from({ length: 12 }, (_, index) =>
    createRecentEvent(index),
  );

  return {
    target: "world",
    description:
      "Nested world snapshot tuned for agent context windows, retrieval, and replay-aware planning.",
    documentType: "agent_world_snapshot",
    preferredFormat: "toon",
    preferredToonDelimiter: "tab",
    value: {
      document_type: "agent_world_snapshot",
      schema_version: EXPORT_SCHEMA_VERSION,
      generated_at_unix_ms: FIXED_GENERATED_AT_UNIX_MS,
      world: {
        world_id: "world-frontier-1",
        branch_id: "branch-2",
        label: "frontier-sigil",
        seed: "storm-seed-77",
        tick: 118_944,
        weather: "electrostatic-rain",
        narrative_arc: "contain_the_breach",
        authority_mode: "deterministic_authoritative",
      },
      strategic_snapshot: {
        objectives,
        zones,
        entities,
        resource_stocks: [
          {
            resource: "charge_cells",
            available: 182,
            committed: 64,
            incoming: 21,
          },
          {
            resource: "anchor_resin",
            available: 77,
            committed: 34,
            incoming: 9,
          },
          {
            resource: "repair_fabric",
            available: 140,
            committed: 48,
            incoming: 17,
          },
        ],
      },
      recent_events: recentEvents,
      decision_context: {
        active_hypotheses: [
          "shadow-team-intends-link-collapse",
          "reward-signal-favors-anchor-hold",
        ],
        forbidden_actions: ["friendly_fire", "out_of_band_spawn"],
        current_priority_stack: [
          "secure_anchor",
          "stabilize_convoy",
          "preserve_energy_budget",
        ],
      },
    },
  };
}

function buildMultiverseExportDataset(branchCount = 6): ExportDataset {
  const branches = Array.from({ length: branchCount }, (_, index) => ({
    branch_id: `branch-${index + 1}`,
    parent_branch_id: index === 0 ? null : `branch-${index}`,
    fork_tick: 117_500 + index * 140,
    theorem: index % 2 === 0 ? "worldline-preservation" : "reward-loop-balance",
    confidence: Number((0.54 + index * 0.06).toFixed(2)),
    coordinator: {
      lead_team_id: `team-${(index % 3) + 1}`,
      admission_policy: index % 2 === 0 ? "open-with-proof" : "invite-only",
      resolution: {
        control_plane: index % 2 === 0 ? "primary" : "secondary",
        spillover_budget: 8 + index,
        rollback_window_ticks: 120 + index * 10,
      },
    },
    provenance: {
      hypothesis_id: `hypothesis-${index + 1}`,
      evaluation: {
        last_score: Number((0.31 + index * 0.08).toFixed(2)),
        last_update_tick: 118_400 + index * 37,
        regression_guard: {
          enabled: index % 2 === 0,
          floor: Number((0.22 + index * 0.03).toFixed(2)),
          fallback_branch_id: index === 0 ? null : `branch-${index}`,
        },
      },
      archival: {
        shard: `archive-${(index % 2) + 1}`,
        retained_snapshots: 4 + index,
        cold_storage_uri: `s3://pod-history/branch-${index + 1}/snapshot.json`,
      },
    },
  }));

  return {
    target: "multiverse",
    description:
      "Deep branch/multiverse index for proving world topology, fork lineage, and orchestration metadata.",
    documentType: "multiverse_branch_index",
    preferredFormat: "json",
    preferredToonDelimiter: "comma",
    value: {
      document_type: "multiverse_branch_index",
      schema_version: EXPORT_SCHEMA_VERSION,
      generated_at_unix_ms: FIXED_GENERATED_AT_UNIX_MS,
      theorem_pack: {
        world_model: "prompt-or-die",
        proof_target: "world-and-multiverse-coherence",
        runtime: {
          substrate: "spacetimedb-2x-target",
          transport: {
            authority_feed: "generated-bindings",
            fallback_feed: "deterministic-command-runtime",
          },
        },
      },
      multiverse: {
        active_branch_count: branchCount,
        branches,
        world_index: {
          frontier_worlds: {
            sigil: {
              world_id: "world-frontier-1",
              branch_id: "branch-2",
              lane: "alpha",
              status: "contested",
            },
            ember: {
              world_id: "world-frontier-2",
              branch_id: "branch-3",
              lane: "beta",
              status: "stabilizing",
            },
          },
          sanctuary_worlds: {
            echo: {
              world_id: "world-sanctuary-echo",
              branch_id: "branch-1",
              lane: "gamma",
              status: "secured",
            },
          },
        },
        routing_preferences: {
          cross_branch_handoff: {
            enabled: true,
            quorum: {
              minimum_worlds: 2,
              minimum_teams: 2,
              minimum_confidence: 0.72,
            },
            priority_order: {
              first: "worldline-preservation",
              second: "reward-loop-balance",
              third: "pressure-relief",
            },
          },
          backpressure_controls: {
            max_concurrent_replications: 3,
            freeze_on_divergence: true,
            divergence_floor: 0.18,
          },
        },
        metadata: {
          authoring: {
            kit: "pod-owned-rs-sdk-facade",
            generation_mode: "deterministic-fixture",
            formulas: {
              objective_shift_formula: "delta_score + death_marks + unresolved_objectives",
              routing_penalty_formula: "branch_depth * divergence + pressure_bias",
            },
          },
          notes: {
            operators: {
              primary_question:
                "Which branch keeps the worldline stable while preserving agent autonomy?",
              review_lane: "benchmark-proof",
            },
            compliance: {
              public_contract: "json-shell-control-plane",
              llm_facing_exports: "toon-when-tabular",
            },
          },
        },
      },
    },
  };
}

function buildSemiUniformLogsDataset(logCount = 64): PodBenchmarkDataset {
  const logs = Array.from({ length: logCount }, (_, index) => {
    const base = {
      ts_unix_ms: FIXED_GENERATED_AT_UNIX_MS + index * 750,
      level:
        index % 5 === 0
          ? "warn"
          : index % 5 === 1
            ? "error"
            : index % 5 === 2
              ? "debug"
              : "info",
      subsystem:
        index % 4 === 0
          ? "world_sync"
          : index % 4 === 1
            ? "agent_runtime"
            : index % 4 === 2
              ? "quest_graph"
              : "transport",
      message:
        index % 3 === 0
          ? "world-state divergence detected"
          : index % 3 === 1
            ? "retrying remote topology feed"
            : "agent decision trace persisted",
      request_id: `req-${String(index + 1).padStart(4, "0")}`,
    } satisfies Record<string, JsonValue>;

    if (index % 2 === 0) {
      base.world_id = `world-frontier-${(index % 2) + 1}`;
    }
    if (index % 3 === 0) {
      base.branch_id = `branch-${(index % 6) + 1}`;
    }
    if (index % 4 === 0) {
      base.retry_budget = 3 - (index % 3);
    }
    if (index % 5 === 0) {
      base.error_code = index % 10 === 0 ? "DIVERGENCE" : "BACKPRESSURE";
    }
    if (index % 6 === 0) {
      base.agent_type = index % 12 === 0 ? "llm_agent" : "neural_agent";
    }

    return base;
  });

  return {
    id: "semi_uniform_agent_logs",
    description:
      "Semi-uniform operational logs with optional fields and mixed subsystems.",
    family: "semi-uniform-logs",
    exportTarget: null,
    preferredFormat: "json",
    preferredToonDelimiter: "tab",
    value: {
      document_type: "agent_runtime_logs",
      schema_version: EXPORT_SCHEMA_VERSION,
      generated_at_unix_ms: FIXED_GENERATED_AT_UNIX_MS,
      logs,
    },
  };
}

function buildExportDatasets(): Record<PodExportTarget, ExportDataset> {
  return {
    events: buildEventsExportDataset(),
    world: buildWorldExportDataset(),
    multiverse: buildMultiverseExportDataset(),
  };
}

function buildBenchmarkDatasetsFromExports(
  exportDatasets: Record<PodExportTarget, ExportDataset>,
): PodBenchmarkDataset[] {
  return [
    {
      id: "uniform_tick_event_batch",
      description:
        "Uniform tick/event rows modeled after a stable tick_event_batch export.",
      family: "uniform-records",
      exportTarget: "events",
      preferredFormat: exportDatasets.events.preferredFormat,
      preferredToonDelimiter: exportDatasets.events.preferredToonDelimiter,
      value: buildEventsExportDataset(192).value,
    },
    buildToonscapeDonorDataset(),
    buildSemiUniformLogsDataset(),
    {
      id: "nested_world_snapshot",
      description:
        "Nested world snapshot with repeated entity/objective arrays for agent context windows.",
      family: "nested-world-snapshot",
      exportTarget: "world",
      preferredFormat: exportDatasets.world.preferredFormat,
      preferredToonDelimiter: exportDatasets.world.preferredToonDelimiter,
      value: buildWorldExportDataset(28).value,
    },
    {
      id: "deep_multiverse_index",
      description:
        "Deep multiverse metadata tree with branch lineage and orchestration settings.",
      family: "deep-multiverse-tree",
      exportTarget: "multiverse",
      preferredFormat: exportDatasets.multiverse.preferredFormat,
      preferredToonDelimiter: exportDatasets.multiverse.preferredToonDelimiter,
      value: buildMultiverseExportDataset(8).value,
    },
  ];
}

function buildToonLines(
  value: JsonValue,
  delimiterKey: DelimiterKey,
): string[] {
  return Array.from(
    encodeLines(value, {
      indent: TOON_ENCODE_INDENT,
      delimiter: DELIMITERS[delimiterKey],
      keyFolding: TOON_ENCODE_KEY_FOLDING,
    }),
  );
}

export function parsePodExportTarget(value: string): PodExportTarget {
  if (!POD_EXPORT_TARGETS.includes(value as PodExportTarget)) {
    throw new Error(`unknown export target: ${value}`);
  }
  return value as PodExportTarget;
}

export function parsePodExportFormat(value: string): PodExportFormat {
  if (!POD_EXPORT_FORMATS.includes(value as PodExportFormat)) {
    throw new Error(`unknown export format: ${value}`);
  }
  return value as PodExportFormat;
}

export function getPodExportDataset(target: PodExportTarget): ExportDataset {
  return buildExportDatasets()[target];
}

export function getPodBenchmarkDatasets(): PodBenchmarkDataset[] {
  return buildBenchmarkDatasetsFromExports(buildExportDatasets());
}

export function renderPodExport(
  target: PodExportTarget,
  format: PodExportFormat,
): PodExportEnvelope {
  const dataset = getPodExportDataset(target);
  const text =
    format === "json"
      ? JSON.stringify(dataset.value, null, 2)
      : buildToonLines(
          dataset.value,
          dataset.preferredToonDelimiter ?? "comma",
        ).join("\n");

  return {
    schemaVersion: EXPORT_SCHEMA_VERSION,
    generatedAtUnixMs: FIXED_GENERATED_AT_UNIX_MS,
    target,
    description: dataset.description,
    documentType: dataset.documentType,
    preferredFormat: dataset.preferredFormat,
    preferredToonDelimiter: dataset.preferredToonDelimiter,
    format,
    contentType:
      format === "json" ? "application/json" : "application/toon",
    byteLength: Buffer.byteLength(text, "utf8"),
    lineCount: countLines(text),
    text,
  };
}

export function decodePodExportToon(
  target: PodExportTarget,
  text: string,
): JsonValue {
  const dataset = getPodExportDataset(target);
  if (dataset.preferredToonDelimiter == null) {
    throw new Error(`TOON is not configured for ${target}`);
  }
  const decoded = decodeFromLines(text.split(/\r?\n/), TOON_DECODE_OPTIONS);
  return decoded;
}

export function countToonStreamEvents(text: string): number {
  let count = 0;
  for (const _event of decodeStreamSync(
    text.split(/\r?\n/),
    TOON_STREAM_DECODE_OPTIONS,
  )) {
    count += 1;
  }
  return count;
}

export function collectToonStreamEvents(text: string): JsonStreamEvent[] {
  return Array.from(
    decodeStreamSync(text.split(/\r?\n/), TOON_STREAM_DECODE_OPTIONS),
  );
}

export function buildBrokenUniformToonLines(
  datasetId: PodBenchmarkDatasetId,
  mode: "row-width" | "truncated",
  delimiterKey: DelimiterKey = "tab",
): string[] {
  const dataset = getPodBenchmarkDatasets().find((entry) => entry.id === datasetId);
  if (!dataset) {
    throw new Error(`unknown benchmark dataset: ${datasetId}`);
  }
  const lines = buildToonLines(dataset.value, delimiterKey);
  if (mode === "truncated") {
    return lines.slice(0, -1);
  }
  const rowDelimiter = DELIMITERS[delimiterKey];
  const headerIndex = lines.findIndex(
    (line) => line.includes(rowDelimiter) && line.includes("{") && line.endsWith(":"),
  );
  if (headerIndex === -1 || headerIndex + 1 >= lines.length) {
    throw new Error(`dataset ${datasetId} did not produce a tabular TOON body`);
  }
  const broken = [...lines];
  const firstRow = broken[headerIndex + 1] ?? "";
  const firstDelimiterIndex = firstRow.lastIndexOf(rowDelimiter);
  if (firstDelimiterIndex <= 0) {
    throw new Error(`dataset ${datasetId} did not produce a multi-column row`);
  }
  broken[headerIndex + 1] = firstRow.slice(0, firstDelimiterIndex);
  return broken;
}

export function decodeBenchmarkToonLines(lines: string[]): JsonValue {
  return decodeFromLines(lines, TOON_DECODE_OPTIONS);
}
