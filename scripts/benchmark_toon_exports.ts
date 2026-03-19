#!/usr/bin/env bun

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

import {
  DELIMITERS,
  decodeFromLines,
  encodeLines,
  type DelimiterKey,
  type JsonValue,
} from "@toon-format/toon";
import { countTokens } from "gpt-tokenizer";

import {
  buildBrokenUniformToonLines,
  collectToonStreamEvents,
  decodeBenchmarkToonLines,
  getPodBenchmarkDatasets,
  type PodBenchmarkDataset,
  type PodBenchmarkDatasetId,
  type PodExportFormat,
} from "./pod_sdk";
import {
  renderToonBenchmarkCharts,
  renderToonBenchmarkHtml,
  renderToonBenchmarkMarkdown,
} from "./toon_benchmark_report";

const TOON_INDENT = 2;
const TOON_KEY_FOLDING = "safe" as const;
const TOON_DECODE_OPTIONS = {
  indent: TOON_INDENT,
  strict: true,
  expandPaths: "safe",
} as const;

type BenchmarkProfile = "ci-smoke" | "default" | "extensive";

type Options = {
  profile: BenchmarkProfile;
  iterations: number;
  rounds: number;
  output: string;
  htmlOutput: string | null;
  markdownOutput: string | null;
  chartsDir: string | null;
  failOnChecks: boolean;
};

const BENCHMARK_PROFILES: Record<
  BenchmarkProfile,
  {
    iterations: number;
    rounds: number;
  }
> = {
  "ci-smoke": {
    iterations: 250,
    rounds: 3,
  },
  default: {
    iterations: 2000,
    rounds: 9,
  },
  extensive: {
    iterations: 4000,
    rounds: 12,
  },
};

type VariantId = "json-pretty" | "json-compact" | "toon-comma" | "toon-tab";

type LatencyStats = {
  rounds: number;
  iterationsPerRound: number;
  totalMs: number;
  meanNsPerOperation: number;
  medianNsPerOperation: number;
  p95NsPerOperation: number;
};

type VariantMeasurement = {
  variantId: VariantId;
  label: string;
  format: "json" | "toon";
  delimiter: DelimiterKey | null;
  bytes: number;
  tokens: number;
  lines: number;
  sample: string;
  roundtripMatches: boolean;
  streamEventCount: number | null;
  encode: LatencyStats;
  decode: LatencyStats;
};

type DatasetRecommendation = {
  preferredFormat: PodExportFormat;
  preferredVariant: VariantId;
  preferredToonDelimiter: DelimiterKey | null;
  rationale: string[];
};

type DatasetResult = {
  id: PodBenchmarkDatasetId;
  description: string;
  family: PodBenchmarkDataset["family"];
  exportTarget: PodBenchmarkDataset["exportTarget"];
  bestTokenVariant: VariantId;
  bestByteVariant: VariantId;
  bestToonVariant: VariantId;
  measurements: Record<VariantId, VariantMeasurement>;
  compactJsonBaseline: {
    bytes: number;
    tokens: number;
  };
  bestToonDeltaVsCompactJson: {
    bytes: number;
    percentBytes: number;
    tokens: number;
    percentTokens: number;
  };
  bestToonDeltaVsPrettyJson: {
    bytes: number;
    percentBytes: number;
    tokens: number;
    percentTokens: number;
  };
  recommendation: DatasetRecommendation;
};

type BenchmarkCheck = {
  metric: string;
  passed: boolean;
  expected: string;
  observed: string;
};

export type ToonExportBenchmarkReport = {
  schemaVersion: 1;
  generatedAtUnixMs: number;
  profile: BenchmarkProfile;
  iterations: number;
  rounds: number;
  variants: Array<{
    id: VariantId;
    label: string;
    format: "json" | "toon";
    delimiter: DelimiterKey | null;
  }>;
  datasets: DatasetResult[];
  validation: {
    strictRowWidthError: string;
    strictTruncationError: string;
  };
  checks: BenchmarkCheck[];
  allChecksPassed: boolean;
  decision: {
    shellControlPlane: "json";
    shellRationale: string[];
    exportRecommendations: Record<
      "events" | "world" | "multiverse",
      DatasetRecommendation
    >;
  };
};

type VariantDefinition = {
  id: VariantId;
  label: string;
  format: "json" | "toon";
  delimiter: DelimiterKey | null;
  encode: (value: JsonValue) => string;
  decode: (text: string) => JsonValue;
  streamEventCount: (text: string) => number | null;
};

const VARIANTS: VariantDefinition[] = [
  {
    id: "json-pretty",
    label: "Pretty JSON",
    format: "json",
    delimiter: null,
    encode: (value) => JSON.stringify(value, null, 2),
    decode: (text) => JSON.parse(text) as JsonValue,
    streamEventCount: () => null,
  },
  {
    id: "json-compact",
    label: "Compact JSON",
    format: "json",
    delimiter: null,
    encode: (value) => JSON.stringify(value),
    decode: (text) => JSON.parse(text) as JsonValue,
    streamEventCount: () => null,
  },
  {
    id: "toon-comma",
    label: "TOON (comma)",
    format: "toon",
    delimiter: "comma",
    encode: (value) =>
      Array.from(
        encodeLines(value, {
          indent: TOON_INDENT,
          delimiter: DELIMITERS.comma,
          keyFolding: TOON_KEY_FOLDING,
        }),
      ).join("\n"),
    decode: (text) => decodeFromLines(text.split(/\r?\n/), TOON_DECODE_OPTIONS),
    streamEventCount: (text) => collectToonStreamEvents(text).length,
  },
  {
    id: "toon-tab",
    label: "TOON (tab)",
    format: "toon",
    delimiter: "tab",
    encode: (value) =>
      Array.from(
        encodeLines(value, {
          indent: TOON_INDENT,
          delimiter: DELIMITERS.tab,
          keyFolding: TOON_KEY_FOLDING,
        }),
      ).join("\n"),
    decode: (text) => decodeFromLines(text.split(/\r?\n/), TOON_DECODE_OPTIONS),
    streamEventCount: (text) => collectToonStreamEvents(text).length,
  },
];

function printHelp() {
  console.error(
    "Usage: bun ./scripts/benchmark_toon_exports.ts [--profile ci-smoke|default|extensive] [--iterations N] [--rounds N] [--output artifacts/toon-export-benchmark.json] [--html-output artifacts/toon-export-benchmark.html] [--markdown-output artifacts/toon-export-benchmark.md] [--charts-dir artifacts/toon-export-benchmark-charts] [--fail-on-checks]",
  );
}

function parseArgs(argv: string[]): Options {
  let profile: BenchmarkProfile = "default";
  let explicitIterations: number | null = null;
  let explicitRounds: number | null = null;

  const options: Options = {
    profile,
    iterations: BENCHMARK_PROFILES.default.iterations,
    rounds: BENCHMARK_PROFILES.default.rounds,
    output: "artifacts/toon-export-benchmark.json",
    htmlOutput: null,
    markdownOutput: null,
    chartsDir: null,
    failOnChecks: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--profile": {
        const value = argv[index + 1];
        if (
          value !== "ci-smoke" &&
          value !== "default" &&
          value !== "extensive"
        ) {
          throw new Error(
            "missing supported value for --profile (ci-smoke|default|extensive)",
          );
        }
        profile = value;
        index += 1;
        break;
      }
      case "--iterations": {
        const value = Number(argv[index + 1]);
        if (!Number.isFinite(value) || value < 1) {
          throw new Error("missing positive numeric value for --iterations");
        }
        explicitIterations = Math.floor(value);
        index += 1;
        break;
      }
      case "--rounds": {
        const value = Number(argv[index + 1]);
        if (!Number.isFinite(value) || value < 1) {
          throw new Error("missing positive numeric value for --rounds");
        }
        explicitRounds = Math.floor(value);
        index += 1;
        break;
      }
      case "--output": {
        const value = argv[index + 1];
        if (!value) {
          throw new Error("missing value for --output");
        }
        options.output = value;
        index += 1;
        break;
      }
      case "--html-output": {
        const value = argv[index + 1];
        if (!value) {
          throw new Error("missing value for --html-output");
        }
        options.htmlOutput = value;
        index += 1;
        break;
      }
      case "--markdown-output": {
        const value = argv[index + 1];
        if (!value) {
          throw new Error("missing value for --markdown-output");
        }
        options.markdownOutput = value;
        index += 1;
        break;
      }
      case "--charts-dir": {
        const value = argv[index + 1];
        if (!value) {
          throw new Error("missing value for --charts-dir");
        }
        options.chartsDir = value;
        index += 1;
        break;
      }
      case "--fail-on-checks":
        options.failOnChecks = true;
        break;
      case "--help":
      case "-h":
        printHelp();
        process.exit(0);
      default:
        throw new Error(`unknown argument: ${current}`);
    }
  }

  const profileDefaults = BENCHMARK_PROFILES[profile];
  options.profile = profile;
  options.iterations = explicitIterations ?? profileDefaults.iterations;
  options.rounds = explicitRounds ?? profileDefaults.rounds;

  return options;
}

function percentile(samples: number[], ratio: number): number {
  const sorted = [...samples].sort((left, right) => left - right);
  const index = Math.min(
    sorted.length - 1,
    Math.max(0, Math.ceil(sorted.length * ratio) - 1),
  );
  return sorted[index] ?? 0;
}

function measureLatency(
  rounds: number,
  iterations: number,
  operation: () => unknown,
): LatencyStats {
  const samplesMs: number[] = [];

  operation();
  operation();

  for (let round = 0; round < rounds; round += 1) {
    const started = performance.now();
    for (let iteration = 0; iteration < iterations; iteration += 1) {
      operation();
    }
    samplesMs.push(performance.now() - started);
  }

  const totalMs = samplesMs.reduce((sum, sample) => sum + sample, 0);
  const nsPerRound = samplesMs.map(
    (sampleMs) => (sampleMs * 1_000_000) / iterations,
  );

  return {
    rounds,
    iterationsPerRound: iterations,
    totalMs,
    meanNsPerOperation:
      nsPerRound.reduce((sum, sample) => sum + sample, 0) / nsPerRound.length,
    medianNsPerOperation: percentile(nsPerRound, 0.5),
    p95NsPerOperation: percentile(nsPerRound, 0.95),
  };
}

function computeDelta(candidate: number, baseline: number) {
  const delta = baseline - candidate;
  return {
    absolute: delta,
    percent: baseline === 0 ? 0 : (delta / baseline) * 100,
  };
}

function preview(text: string): string {
  return text.split("\n").slice(0, 4).join("\n");
}

function getBestVariantBy(
  measurements: Record<VariantId, VariantMeasurement>,
  selector: (measurement: VariantMeasurement) => number,
): VariantId {
  return Object.values(measurements)
    .slice()
    .sort((left, right) => selector(left) - selector(right))[0]!.variantId;
}

function getBestToonVariant(
  measurements: Record<VariantId, VariantMeasurement>,
): VariantId {
  return (["toon-comma", "toon-tab"] as const)
    .map((variantId) => measurements[variantId])
    .sort((left, right) => {
      if (left.tokens !== right.tokens) {
        return left.tokens - right.tokens;
      }
      return left.bytes - right.bytes;
    })[0]!.variantId;
}

function buildRecommendation(
  dataset: PodBenchmarkDataset,
  measurements: Record<VariantId, VariantMeasurement>,
): DatasetRecommendation {
  const compact = measurements["json-compact"];
  const bestToon = measurements[getBestToonVariant(measurements)];
  const toonWins =
    bestToon.tokens < compact.tokens && bestToon.bytes < compact.bytes;

  if (toonWins) {
    return {
      preferredFormat: "toon",
      preferredVariant: bestToon.variantId,
      preferredToonDelimiter: bestToon.delimiter,
      rationale: [
        "The best TOON variant beats compact JSON on both bytes and tokens for this dataset.",
        dataset.family === "uniform-records"
          ? "The repeated row schema lets TOON pay the header cost once and stream values efficiently."
          : "The repeated substructures are uniform enough for TOON's tabular layout to stay ahead of compact JSON.",
      ],
    };
  }

  return {
    preferredFormat: "json",
    preferredVariant: "json-compact",
    preferredToonDelimiter: null,
    rationale: [
      "Compact JSON stays smaller or equally small once the nested metadata tree stops repeating cleanly.",
      dataset.family === "deep-multiverse-tree"
        ? "This is a deep config-style shape, so JSON remains the default even though TOON is still available for inspection."
        : "TOON remains available, but it is not the measured winner for the primary compact-machine representation.",
    ],
  };
}

function measureVariant(
  dataset: PodBenchmarkDataset,
  variant: VariantDefinition,
  options: Pick<Options, "iterations" | "rounds">,
): VariantMeasurement {
  const text = variant.encode(dataset.value);
  const encode = measureLatency(options.rounds, options.iterations, () => {
    variant.encode(dataset.value);
  });
  const decode = measureLatency(options.rounds, options.iterations, () => {
    variant.decode(text);
  });
  const decoded = variant.decode(text);

  return {
    variantId: variant.id,
    label: variant.label,
    format: variant.format,
    delimiter: variant.delimiter,
    bytes: Buffer.byteLength(text, "utf8"),
    tokens: countTokens(text),
    lines: text.length === 0 ? 0 : text.split("\n").length,
    sample: preview(text),
    roundtripMatches: JSON.stringify(decoded) === JSON.stringify(dataset.value),
    streamEventCount: variant.streamEventCount(text),
    encode,
    decode,
  };
}

function buildDatasetResult(
  dataset: PodBenchmarkDataset,
  options: Pick<Options, "iterations" | "rounds">,
): DatasetResult {
  const measurements = Object.fromEntries(
    VARIANTS.map((variant) => [
      variant.id,
      measureVariant(dataset, variant, options),
    ]),
  ) as Record<VariantId, VariantMeasurement>;

  const compact = measurements["json-compact"];
  const pretty = measurements["json-pretty"];
  const bestToonVariant = getBestToonVariant(measurements);
  const bestToon = measurements[bestToonVariant];
  const recommendation = buildRecommendation(dataset, measurements);

  return {
    id: dataset.id,
    description: dataset.description,
    family: dataset.family,
    exportTarget: dataset.exportTarget,
    bestTokenVariant: getBestVariantBy(measurements, (measurement) => measurement.tokens),
    bestByteVariant: getBestVariantBy(measurements, (measurement) => measurement.bytes),
    bestToonVariant,
    measurements,
    compactJsonBaseline: {
      bytes: compact.bytes,
      tokens: compact.tokens,
    },
    bestToonDeltaVsCompactJson: {
      bytes: compact.bytes - bestToon.bytes,
      percentBytes: computeDelta(bestToon.bytes, compact.bytes).percent,
      tokens: compact.tokens - bestToon.tokens,
      percentTokens: computeDelta(bestToon.tokens, compact.tokens).percent,
    },
    bestToonDeltaVsPrettyJson: {
      bytes: pretty.bytes - bestToon.bytes,
      percentBytes: computeDelta(bestToon.bytes, pretty.bytes).percent,
      tokens: pretty.tokens - bestToon.tokens,
      percentTokens: computeDelta(bestToon.tokens, pretty.tokens).percent,
    },
    recommendation,
  };
}

function buildCheck(metric: string, passed: boolean, expected: string, observed: string): BenchmarkCheck {
  return {
    metric,
    passed,
    expected,
    observed,
  };
}

function findDataset(
  results: DatasetResult[],
  id: PodBenchmarkDatasetId,
): DatasetResult {
  const result = results.find((entry) => entry.id === id);
  if (!result) {
    throw new Error(`missing benchmark dataset result: ${id}`);
  }
  return result;
}

export async function buildToonExportBenchmarkReport(
  _repoRoot: string,
  options: Pick<Options, "profile" | "iterations" | "rounds">,
): Promise<ToonExportBenchmarkReport> {
  const datasets = getPodBenchmarkDatasets().map((dataset) =>
    buildDatasetResult(dataset, options),
  );

  const rowWidthError = (() => {
    try {
      decodeBenchmarkToonLines(
        buildBrokenUniformToonLines("uniform_tick_event_batch", "row-width"),
      );
      return "no error";
    } catch (error) {
      return error instanceof Error ? error.message : String(error);
    }
  })();
  const truncationError = (() => {
    try {
      decodeBenchmarkToonLines(
        buildBrokenUniformToonLines("uniform_tick_event_batch", "truncated"),
      );
      return "no error";
    } catch (error) {
      return error instanceof Error ? error.message : String(error);
    }
  })();

  const uniform = findDataset(datasets, "uniform_tick_event_batch");
  const toonscapeDonor = findDataset(datasets, "toonscape_donor_tick_event_batch");
  const logs = findDataset(datasets, "semi_uniform_agent_logs");
  const world = findDataset(datasets, "nested_world_snapshot");
  const multiverse = findDataset(datasets, "deep_multiverse_index");

  const checks: BenchmarkCheck[] = [
    buildCheck(
      "uniform_tick_event_batch.toon_beats_compact_json_on_tokens",
      uniform.bestToonDeltaVsCompactJson.tokens > 0,
      "> 0",
      String(uniform.bestToonDeltaVsCompactJson.tokens),
    ),
    buildCheck(
      "uniform_tick_event_batch.toon_beats_compact_json_on_bytes",
      uniform.bestToonDeltaVsCompactJson.bytes > 0,
      "> 0",
      String(uniform.bestToonDeltaVsCompactJson.bytes),
    ),
    buildCheck(
      "toonscape_donor_tick_event_batch.toon_beats_compact_json_on_tokens_by_70pct",
      toonscapeDonor.bestToonDeltaVsCompactJson.percentTokens >= 70,
      ">= 70",
      toonscapeDonor.bestToonDeltaVsCompactJson.percentTokens.toFixed(1),
    ),
    buildCheck(
      "toonscape_donor_tick_event_batch.toon_beats_compact_json_on_bytes_by_70pct",
      toonscapeDonor.bestToonDeltaVsCompactJson.percentBytes >= 70,
      ">= 70",
      toonscapeDonor.bestToonDeltaVsCompactJson.percentBytes.toFixed(1),
    ),
    buildCheck(
      "nested_world_snapshot.recommendation_matches_export_surface",
      world.recommendation.preferredFormat === "toon",
      "toon",
      world.recommendation.preferredFormat,
    ),
    buildCheck(
      "deep_multiverse_index.recommendation_matches_export_surface",
      multiverse.recommendation.preferredFormat === "json",
      "json",
      multiverse.recommendation.preferredFormat,
    ),
    buildCheck(
      "semi_uniform_agent_logs.documents_tradeoff_or_toon_win",
      logs.recommendation.preferredFormat === "toon" ||
        logs.bestToonDeltaVsPrettyJson.tokens > 0,
      "documented tradeoff or TOON win",
      `${logs.recommendation.preferredFormat}; deltaVsPrettyTokens=${logs.bestToonDeltaVsPrettyJson.tokens}`,
    ),
    buildCheck(
      "toon_roundtrip_all_datasets",
      datasets.every(
        (dataset) =>
          dataset.measurements["toon-comma"].roundtripMatches &&
          dataset.measurements["toon-tab"].roundtripMatches,
      ),
      "true",
      String(
        datasets.every(
          (dataset) =>
            dataset.measurements["toon-comma"].roundtripMatches &&
            dataset.measurements["toon-tab"].roundtripMatches,
        ),
      ),
    ),
    buildCheck(
      "strict_validation.row_width_error",
      rowWidthError.includes("Expected"),
      "contains Expected",
      rowWidthError,
    ),
    buildCheck(
      "strict_validation.truncation_error",
      truncationError.includes("Expected"),
      "contains Expected",
      truncationError,
    ),
  ];

  const exportRecommendations = {
    events: uniform.recommendation,
    world: world.recommendation,
    multiverse: multiverse.recommendation,
  };

  return {
    schemaVersion: 1,
    generatedAtUnixMs: Date.now(),
    profile: options.profile,
    iterations: options.iterations,
    rounds: options.rounds,
    variants: VARIANTS.map((variant) => ({
      id: variant.id,
      label: variant.label,
      format: variant.format,
      delimiter: variant.delimiter,
    })),
    datasets,
    validation: {
      strictRowWidthError: rowWidthError,
      strictTruncationError: truncationError,
    },
    checks,
    allChecksPassed: checks.every((check) => check.passed),
    decision: {
      shellControlPlane: "json",
      shellRationale: [
        "The Toonscape donor pattern is uniform batched telemetry, not tiny shell RPC envelopes.",
        "pod shell --agent stays newline-delimited JSON so control messages remain simple, explicit, and easy to debug.",
        "TOON is reserved for world/event/multiverse exports where the dataset winner justifies it.",
      ],
      exportRecommendations,
    },
  };
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const repoRoot = resolve(import.meta.dir, "..");
  const report = await buildToonExportBenchmarkReport(repoRoot, options);
  const outputPath = resolve(repoRoot, options.output);

  mkdirSync(dirname(outputPath), { recursive: true });
  writeFileSync(outputPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");

  if (options.markdownOutput) {
    const markdownPath = resolve(repoRoot, options.markdownOutput);
    mkdirSync(dirname(markdownPath), { recursive: true });
    writeFileSync(
      markdownPath,
      `${renderToonBenchmarkMarkdown(report).trimEnd()}\n`,
      "utf8",
    );
  }

  if (options.htmlOutput) {
    const htmlPath = resolve(repoRoot, options.htmlOutput);
    mkdirSync(dirname(htmlPath), { recursive: true });
    writeFileSync(htmlPath, renderToonBenchmarkHtml(report), "utf8");
  }

  if (options.chartsDir) {
    const chartsPath = resolve(repoRoot, options.chartsDir);
    mkdirSync(chartsPath, { recursive: true });
    for (const chart of renderToonBenchmarkCharts(report)) {
      writeFileSync(resolve(chartsPath, chart.filename), chart.svg, "utf8");
    }
  }

  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);

  if (options.failOnChecks && !report.allChecksPassed) {
    process.exitCode = 1;
  }
}

if (import.meta.main) {
  void main();
}
