import { existsSync, readdirSync, readFileSync } from "node:fs";
import { dirname, relative, resolve, sep } from "node:path";

export type CliAudience =
  | "developer"
  | "user"
  | "agent"
  | "agent-developer";

export type CliArea =
  | "workspace"
  | "runtime"
  | "web"
  | "assets"
  | "benchmark"
  | "history"
  | "catalog";

export type CliKind =
  | "workspace-command"
  | "cargo-bin"
  | "cargo-example"
  | "bun-package-script"
  | "bun-script";

export type CliSurfaceEntry = {
  id: string;
  name: string;
  audiences: CliAudience[];
  area: CliArea;
  kind: CliKind;
  command: string;
  cwd: string;
  entrypoint: string | null;
  summary: string;
  machineReadable: boolean;
  outputArtifacts: string[];
  docs: string[];
  env: string[];
  notes: string[];
  coverage: string[];
};

export type ServerEnvironmentVariable = {
  name: string;
  defaultValue: string;
  usedBy: string[];
  description: string;
};

export type DiscoveredSurface = {
  key: string;
  kind: CliKind;
  command: string;
  cwd: string;
  entrypoint: string;
};

export type CliSurfaceCatalog = {
  schemaVersion: 1;
  sourceOfTruth: string;
  scope: string;
  commands: CliSurfaceEntry[];
  serverEnvironment: ServerEnvironmentVariable[];
  discoveredSurfaces: DiscoveredSurface[];
};

export type CliSurfaceValidation = {
  ok: boolean;
  duplicateIds: string[];
  missingEntrypoints: string[];
  missingDocs: string[];
  unknownCoverageKeys: string[];
  uncoveredDiscoveredSurfaces: DiscoveredSurface[];
  docPath: string;
  docInSync: boolean;
  generatedMarkdown: string;
};

export const CLI_DOC_PATH = "docs/cli-surface.md";
export const CLI_SOURCE_OF_TRUTH_PATH = "scripts/cli_surface.ts";
export const CLI_SCOPE =
  "The CLI catalog covers supported top-level workspace commands, cargo binaries, cargo examples, root Bun scripts, and packaged app scripts. It intentionally excludes one-off targeted test invocations and third-party prerequisite tools such as cargo, bun, bunx, Playwright, or SpacetimeDB commands that are not owned by this repository.";

export const CLI_SURFACE: CliSurfaceEntry[] = [
  {
    id: "workspace-build",
    name: "Workspace Build",
    audiences: ["developer", "agent-developer"],
    area: "workspace",
    kind: "workspace-command",
    command: "cargo build --workspace",
    cwd: ".",
    entrypoint: null,
    summary: "Build every Rust package in the workspace.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["README.md", "AGENTS.md"],
    env: [],
    notes: [],
    coverage: [],
  },
  {
    id: "workspace-check",
    name: "Workspace Check",
    audiences: ["developer", "agent-developer"],
    area: "workspace",
    kind: "workspace-command",
    command: "cargo check --workspace",
    cwd: ".",
    entrypoint: null,
    summary: "Run the fast Rust workspace compile gate used by CI and local review loops.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["README.md", "AGENTS.md"],
    env: [],
    notes: [],
    coverage: [],
  },
  {
    id: "workspace-test",
    name: "Workspace Test",
    audiences: ["developer", "agent-developer"],
    area: "workspace",
    kind: "workspace-command",
    command: "cargo test --workspace",
    cwd: ".",
    entrypoint: null,
    summary: "Run the full Rust workspace test suite.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["README.md", "AGENTS.md"],
    env: [],
    notes: [],
    coverage: [],
  },
  {
    id: "workspace-clippy",
    name: "Workspace Clippy",
    audiences: ["developer", "agent-developer"],
    area: "workspace",
    kind: "workspace-command",
    command: "cargo clippy --workspace -- -D warnings",
    cwd: ".",
    entrypoint: null,
    summary: "Run the workspace lint gate with warnings treated as failures.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["AGENTS.md"],
    env: [],
    notes: ["Mirrors the repo CI lint posture."],
    coverage: [],
  },
  {
    id: "prompt-or-die",
    name: "Desktop Runtime",
    audiences: ["user", "developer"],
    area: "runtime",
    kind: "cargo-bin",
    command: "cargo run --bin prompt-or-die",
    cwd: ".",
    entrypoint: "apps/pod-desktop/src/main.rs",
    summary: "Launch the native desktop runtime entrypoint.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["README.md"],
    env: [],
    notes: ["No user-tunable CLI flags are currently parsed by the desktop binary."],
    coverage: ["cargo-bin:prompt-or-die"],
  },
  {
    id: "pod-server",
    name: "Dedicated Server",
    audiences: ["user", "developer", "agent-developer"],
    area: "runtime",
    kind: "cargo-bin",
    command: "cargo run --bin pod-server",
    cwd: ".",
    entrypoint: "apps/pod-server/src/main.rs",
    summary: "Launch the dedicated authoritative server composition root.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["README.md"],
    env: [
      "POD_TICK_RATE",
      "POD_RUNTIME_MODE",
      "POD_WORLD_SEED",
      "POD_MAP_NAME",
      "POD_INITIAL_IDLE_AGENTS",
      "POD_OPS_ARCHIVE_DIR",
      "POD_BIND_ADDRESS",
      "POD_MAX_CLIENTS",
      "POD_ENABLE_WEBSOCKET",
      "POD_WEBSOCKET_PORT",
      "POD_SNAPSHOT_INTERVAL",
      "POD_CLIENT_INACTIVITY_TIMEOUT_TICKS",
      "POD_QUEUE_PRESSURE_WARN_DEPTH",
    ],
    notes: [
      "The server surface is env-driven today rather than flag-driven.",
    ],
    coverage: ["cargo-bin:pod-server"],
  },
  {
    id: "pod-headless",
    name: "Headless Tournament Runner",
    audiences: ["developer", "agent", "agent-developer"],
    area: "runtime",
    kind: "cargo-bin",
    command:
      "cargo run --bin pod-headless -- --profile ci-smoke --output artifacts/pod-headless-report.json --dataset-output artifacts/pod-headless-dataset.json --topology-output artifacts/pod-headless-topology.json",
    cwd: ".",
    entrypoint: "apps/pod-headless/src/main.rs",
    summary: "Run the authoritative headless tournament and export report, dataset, and topology artifacts.",
    machineReadable: true,
    outputArtifacts: [
      "artifacts/pod-headless-report.json",
      "artifacts/pod-headless-dataset.json",
      "artifacts/pod-headless-topology.json",
    ],
    docs: ["README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [
      "Supports `--profile`, `--scenario`, `--output`, `--dataset-output`, and `--topology-output`.",
    ],
    coverage: ["cargo-bin:pod-headless"],
  },
  {
    id: "controller-parity-benchmark",
    name: "Controller Parity Benchmark",
    audiences: ["developer", "agent", "agent-developer"],
    area: "benchmark",
    kind: "cargo-example",
    command:
      "cargo run -q -p pod-agents --example controller_parity_benchmark -- --fail-on-checks --output artifacts/controller-parity.json",
    cwd: ".",
    entrypoint: "crates/pod-agents/examples/controller_parity_benchmark.rs",
    summary: "Measure controller parity across runtime implementations and optionally fail on unmet checks.",
    machineReadable: true,
    outputArtifacts: ["artifacts/controller-parity.json"],
    docs: ["README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["cargo-example:pod-agents:controller_parity_benchmark"],
  },
  {
    id: "stage-import",
    name: "Stage Import",
    audiences: ["developer", "agent", "agent-developer"],
    area: "assets",
    kind: "cargo-example",
    command:
      "cargo run -p pod-assets --example stage_import -- --json --output-root artifacts/staged-assets path/to/asset.glb",
    cwd: ".",
    entrypoint: "crates/pod-assets/examples/stage_import.rs",
    summary: "Stage one or more authored assets into the content-addressed import pipeline and optionally emit JSON.",
    machineReadable: true,
    outputArtifacts: ["artifacts/staged-assets"],
    docs: ["README.md", "docs/asset-pipeline.md", "apps/pod-web/README.md"],
    env: [],
    notes: [
      "Supports `--materialize-runtime`, `--base-dir`, and `--bundle-spec` for runtime bundle generation.",
    ],
    coverage: ["cargo-example:pod-assets:stage_import"],
  },
  {
    id: "moat-benchmark-suite",
    name: "Core Moat Benchmark Suite",
    audiences: ["developer", "agent", "agent-developer"],
    area: "benchmark",
    kind: "cargo-example",
    command:
      "cargo run -p pod-core --example moat_benchmark_suite --release -- --profile shard-target --monthly-host-cost-usd 300 --output artifacts/moat-core.json",
    cwd: ".",
    entrypoint: "crates/pod-core/examples/moat_benchmark_suite.rs",
    summary: "Run the core moat benchmark suite for cost and throughput posture.",
    machineReadable: true,
    outputArtifacts: ["artifacts/moat-core.json"],
    docs: ["docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["cargo-example:pod-core:moat_benchmark_suite"],
  },
  {
    id: "transport-benchmark-suite",
    name: "Transport Benchmark Suite",
    audiences: ["developer", "agent", "agent-developer"],
    area: "benchmark",
    kind: "cargo-example",
    command:
      "cargo run -p pod-net --example transport_benchmark_suite -- --profile shard-target --fail-on-checks --output artifacts/transport-benchmark.json",
    cwd: ".",
    entrypoint: "crates/pod-net/examples/transport_benchmark_suite.rs",
    summary: "Measure snapshot, recovery, delta, and queue-pressure transport behavior.",
    machineReadable: true,
    outputArtifacts: ["artifacts/transport-benchmark.json"],
    docs: ["docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["cargo-example:pod-net:transport_benchmark_suite"],
  },
  {
    id: "topology-feed-benchmark-suite",
    name: "Topology Feed Benchmark Suite",
    audiences: ["developer", "agent", "agent-developer"],
    area: "benchmark",
    kind: "cargo-example",
    command:
      "cargo run -p pod-net --features spacetimedb --example topology_feed_benchmark_suite -- --topology-input artifacts/pod-headless-topology.json --fail-on-checks --output artifacts/topology-feed-benchmark.json",
    cwd: ".",
    entrypoint: "crates/pod-net/examples/topology_feed_benchmark_suite.rs",
    summary: "Validate authority-row and generated-runtime topology parity from a shared topology artifact.",
    machineReadable: true,
    outputArtifacts: ["artifacts/topology-feed-benchmark.json"],
    docs: ["README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [
      "Supports live generated SDK parity checks through `--generated-sdk-host` and related flags.",
    ],
    coverage: ["cargo-example:pod-net:topology_feed_benchmark_suite"],
  },
  {
    id: "pod-web-dev",
    name: "pod-web Dev Server",
    audiences: ["developer", "user"],
    area: "web",
    kind: "bun-package-script",
    command: "bun run dev",
    cwd: "apps/pod-web",
    entrypoint: "apps/pod-web/package.json",
    summary: "Run the browser client development server.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["README.md", "apps/pod-web/README.md", "docs/reference-bootstrap.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:dev"],
  },
  {
    id: "pod-web-build",
    name: "pod-web Build",
    audiences: ["developer", "agent-developer"],
    area: "web",
    kind: "bun-package-script",
    command: "bun run build",
    cwd: "apps/pod-web",
    entrypoint: "apps/pod-web/package.json",
    summary: "Build the browser client for production or preview.",
    machineReadable: false,
    outputArtifacts: ["apps/pod-web/dist"],
    docs: ["apps/pod-web/README.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:build"],
  },
  {
    id: "pod-web-preview",
    name: "pod-web Preview",
    audiences: ["developer", "user"],
    area: "web",
    kind: "bun-package-script",
    command: "bun run preview",
    cwd: "apps/pod-web",
    entrypoint: "apps/pod-web/package.json",
    summary: "Serve the built browser bundle locally for previewing.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["apps/pod-web/README.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:preview"],
  },
  {
    id: "pod-web-sync-assets",
    name: "pod-web Sync Assets",
    audiences: ["developer", "agent-developer"],
    area: "assets",
    kind: "bun-package-script",
    command: "bun run sync:assets",
    cwd: "apps/pod-web",
    entrypoint: "apps/pod-web/package.json",
    summary: "Generate the sample authored asset lane and materialize the runtime bundle into the browser app.",
    machineReadable: false,
    outputArtifacts: [
      "apps/pod-web/artifacts/source-assets",
      "apps/pod-web/artifacts/staged-assets",
      "apps/pod-web/public/assets",
    ],
    docs: ["apps/pod-web/README.md", "docs/asset-pipeline.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:sync:assets"],
  },
  {
    id: "pod-web-verify-assets",
    name: "pod-web Verify Assets",
    audiences: ["developer", "agent", "agent-developer"],
    area: "assets",
    kind: "bun-package-script",
    command: "bun run verify:assets",
    cwd: "apps/pod-web",
    entrypoint: "apps/pod-web/package.json",
    summary: "Re-run the generated asset pipeline and fail if committed browser assets drift.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["apps/pod-web/README.md", "docs/asset-pipeline.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:verify:assets"],
  },
  {
    id: "pod-web-measure-render-routes",
    name: "pod-web Measure Render Routes",
    audiences: ["developer", "agent", "agent-developer"],
    area: "web",
    kind: "bun-package-script",
    command: "bun run measure:render-routes",
    cwd: "apps/pod-web",
    entrypoint: "apps/pod-web/package.json",
    summary: "Capture render-route measurements for the main-thread and worker browser paths.",
    machineReadable: true,
    outputArtifacts: ["apps/pod-web/artifacts/render-route-measurements.json"],
    docs: ["apps/pod-web/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:measure:render-routes"],
  },
  {
    id: "pod-web-measure-render-routes-check",
    name: "pod-web Measure Render Routes Check",
    audiences: ["developer", "agent", "agent-developer"],
    area: "web",
    kind: "bun-package-script",
    command: "bun run measure:render-routes:check",
    cwd: "apps/pod-web",
    entrypoint: "apps/pod-web/package.json",
    summary: "Capture render-route measurements and fail when the configured browser gates regress.",
    machineReadable: true,
    outputArtifacts: ["apps/pod-web/artifacts/render-route-measurements.json"],
    docs: ["apps/pod-web/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:measure:render-routes:check"],
  },
  {
    id: "pod-web-typecheck",
    name: "pod-web Typecheck",
    audiences: ["developer", "agent-developer"],
    area: "web",
    kind: "bun-package-script",
    command: "bun run typecheck",
    cwd: "apps/pod-web",
    entrypoint: "apps/pod-web/package.json",
    summary: "Run the browser client TypeScript compile gate without emitting files.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["apps/pod-web/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:typecheck"],
  },
  {
    id: "pod-web-test",
    name: "pod-web Unit Tests",
    audiences: ["developer", "agent-developer"],
    area: "web",
    kind: "bun-package-script",
    command: "bun run test",
    cwd: "apps/pod-web",
    entrypoint: "apps/pod-web/package.json",
    summary: "Run the browser client Bun unit test suite.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["apps/pod-web/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:test"],
  },
  {
    id: "pod-web-test-smoke",
    name: "pod-web Smoke Tests",
    audiences: ["developer", "agent-developer"],
    area: "web",
    kind: "bun-package-script",
    command: "bun run test:smoke",
    cwd: "apps/pod-web",
    entrypoint: "apps/pod-web/package.json",
    summary: "Run the Playwright browser smoke tests covering showcase and worker-input paths.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["apps/pod-web/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:test:smoke"],
  },
  {
    id: "bootstrap-reference-world",
    name: "Bootstrap Reference World",
    audiences: ["developer", "user", "agent", "agent-developer"],
    area: "runtime",
    kind: "bun-script",
    command: "bun ./scripts/bootstrap_reference_world.ts --hold",
    cwd: ".",
    entrypoint: "scripts/bootstrap_reference_world.ts",
    summary: "Launch or measure the canonical first-world bootstrap flow for the browser client.",
    machineReadable: true,
    outputArtifacts: [],
    docs: ["docs/reference-bootstrap.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [
      "`--measure` prints JSON with startup timing and resolved URL.",
    ],
    coverage: ["bun-script:scripts/bootstrap_reference_world.ts"],
  },
  {
    id: "run-moat-benchmarks",
    name: "Run Moat Benchmarks",
    audiences: ["developer", "agent", "agent-developer"],
    area: "benchmark",
    kind: "bun-script",
    command:
      "bun ./scripts/run_moat_benchmarks.ts --profile shard-target --monthly-host-cost-usd 300 --output artifacts/moat-benchmarks.json",
    cwd: ".",
    entrypoint: "scripts/run_moat_benchmarks.ts",
    summary: "Run the combined benchmark workflow across core, transport, topology, browser, and creator-time surfaces.",
    machineReadable: true,
    outputArtifacts: ["artifacts/moat-benchmarks.json"],
    docs: ["docs/benchmark-suite.md", "docs/reference-bootstrap.md"],
    env: [],
    notes: [],
    coverage: ["bun-script:scripts/run_moat_benchmarks.ts"],
  },
  {
    id: "run-shard-target-snapshot",
    name: "Run Shard Target Snapshot",
    audiences: ["developer", "agent", "agent-developer"],
    area: "history",
    kind: "bun-script",
    command:
      "bun ./scripts/run_shard_target_snapshot.ts --label YYYY-Www --output artifacts/moat-benchmarks-shard-local.json",
    cwd: ".",
    entrypoint: "scripts/run_shard_target_snapshot.ts",
    summary: "Capture the weekly shard-target benchmark snapshot, publish retained artifacts, and optionally compare to baseline history.",
    machineReadable: true,
    outputArtifacts: [
      "artifacts/moat-benchmarks-shard-local.json",
      "docs/benchmark-snapshots/YYYY-Www-shard-target.json",
    ],
    docs: ["README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-script:scripts/run_shard_target_snapshot.ts"],
  },
  {
    id: "compare-moat-snapshots",
    name: "Compare Moat Snapshots",
    audiences: ["developer", "agent", "agent-developer"],
    area: "history",
    kind: "bun-script",
    command:
      "bun ./scripts/compare_moat_snapshots.ts --baseline docs/benchmark-snapshots/2026-W10-shard-target.json --candidate docs/benchmark-snapshots/2026-W11-shard-target.json --output artifacts/benchmark-snapshot-comparison.json --fail-on-regressions",
    cwd: ".",
    entrypoint: "scripts/compare_moat_snapshots.ts",
    summary: "Diff two retained shard-target snapshots and surface regressions or improvements as structured JSON.",
    machineReadable: true,
    outputArtifacts: ["artifacts/benchmark-snapshot-comparison.json"],
    docs: ["docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-script:scripts/compare_moat_snapshots.ts"],
  },
  {
    id: "publish-moat-snapshots",
    name: "Publish Moat Snapshot",
    audiences: ["developer", "agent", "agent-developer"],
    area: "history",
    kind: "bun-script",
    command:
      "bun ./scripts/publish_moat_snapshots.ts --input artifacts/moat-benchmarks-shard-local.json --label YYYY-Www --output docs/benchmark-snapshots/YYYY-Www-shard-target.json",
    cwd: ".",
    entrypoint: "scripts/publish_moat_snapshots.ts",
    summary: "Normalize a live moat benchmark report into the retained benchmark snapshot format.",
    machineReadable: true,
    outputArtifacts: ["docs/benchmark-snapshots/YYYY-Www-shard-target.json"],
    docs: ["docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-script:scripts/publish_moat_snapshots.ts"],
  },
  {
    id: "index-benchmark-snapshots",
    name: "Index Benchmark Snapshots",
    audiences: ["developer", "agent", "agent-developer"],
    area: "history",
    kind: "bun-script",
    command: "bun ./scripts/index_benchmark_snapshots.ts",
    cwd: ".",
    entrypoint: "scripts/index_benchmark_snapshots.ts",
    summary: "Regenerate the retained benchmark snapshot JSON index and Markdown history view.",
    machineReadable: true,
    outputArtifacts: [
      "docs/benchmark-snapshots/index.json",
      "docs/benchmark-snapshots/README.md",
    ],
    docs: ["docs/benchmark-suite.md", "docs/benchmark-snapshots/README.md"],
    env: [],
    notes: [],
    coverage: ["bun-script:scripts/index_benchmark_snapshots.ts"],
  },
  {
    id: "verify-cli-surface",
    name: "Verify CLI Surface",
    audiences: ["developer", "agent", "agent-developer"],
    area: "catalog",
    kind: "bun-script",
    command: "bun ./scripts/verify_cli_surface.ts --check",
    cwd: ".",
    entrypoint: "scripts/verify_cli_surface.ts",
    summary: "Validate that the CLI catalog covers every supported top-level command surface and that the generated docs are in sync.",
    machineReadable: true,
    outputArtifacts: ["docs/cli-surface.md"],
    docs: ["docs/cli-surface.md"],
    env: [],
    notes: [
      "Use `--json` for agent consumption and `--write` to regenerate the docs page.",
    ],
    coverage: ["bun-script:scripts/verify_cli_surface.ts"],
  },
];

export const SERVER_ENVIRONMENT: ServerEnvironmentVariable[] = [
  {
    name: "POD_TICK_RATE",
    defaultValue: "60",
    usedBy: ["pod-server"],
    description: "Authority tick rate applied by the host runtime.",
  },
  {
    name: "POD_RUNTIME_MODE",
    defaultValue: "network",
    usedBy: ["pod-server"],
    description:
      "Authority transport mode. `network`, `direct-connect`, and `direct_connect` map to direct-connect; anything else falls back to local mode.",
  },
  {
    name: "POD_WORLD_SEED",
    defaultValue: "42",
    usedBy: ["pod-server"],
    description: "Deterministic world seed for authoritative bootstrap.",
  },
  {
    name: "POD_MAP_NAME",
    defaultValue: "default",
    usedBy: ["pod-server"],
    description: "Map name passed into the authoritative bootstrap loader.",
  },
  {
    name: "POD_INITIAL_IDLE_AGENTS",
    defaultValue: "3",
    usedBy: ["pod-server"],
    description: "Initial idle NPC count injected into the authoritative shard.",
  },
  {
    name: "POD_OPS_ARCHIVE_DIR",
    defaultValue: "unset",
    usedBy: ["pod-server"],
    description: "Optional shard ops archive directory for retained ops history.",
  },
  {
    name: "POD_BIND_ADDRESS",
    defaultValue: "0.0.0.0:7777",
    usedBy: ["pod-server"],
    description: "Direct-connect bind address in `host:port` form.",
  },
  {
    name: "POD_MAX_CLIENTS",
    defaultValue: "32",
    usedBy: ["pod-server"],
    description: "Maximum concurrent direct-connect clients.",
  },
  {
    name: "POD_ENABLE_WEBSOCKET",
    defaultValue: "true",
    usedBy: ["pod-server"],
    description: "Toggle the browser WebSocket fallback for direct-connect clients.",
  },
  {
    name: "POD_WEBSOCKET_PORT",
    defaultValue: "bind port + 1",
    usedBy: ["pod-server"],
    description: "Port for the direct-connect WebSocket fallback endpoint.",
  },
  {
    name: "POD_SNAPSHOT_INTERVAL",
    defaultValue: "10",
    usedBy: ["pod-server"],
    description: "Snapshot interval for direct-connect transport policy.",
  },
  {
    name: "POD_CLIENT_INACTIVITY_TIMEOUT_TICKS",
    defaultValue: "600",
    usedBy: ["pod-server"],
    description: "Client inactivity timeout in authoritative ticks.",
  },
  {
    name: "POD_QUEUE_PRESSURE_WARN_DEPTH",
    defaultValue: "192",
    usedBy: ["pod-server"],
    description: "Warn threshold for pending direct-connect action queue depth.",
  },
];

function normalizeRepoPath(path: string): string {
  return path.split(sep).join("/");
}

function readText(path: string): string {
  return readFileSync(path, "utf8");
}

function readCargoPackageName(manifestPath: string): string {
  const text = readText(manifestPath);
  const match = text.match(/\[package\][\s\S]*?^\s*name\s*=\s*"([^"]+)"/m);
  if (!match) {
    throw new Error(`could not read package name from ${manifestPath}`);
  }
  return match[1];
}

function readCargoBinNames(manifestPath: string): string[] {
  const text = readText(manifestPath);
  const binBlocks = text.split("[[bin]]").slice(1);
  const bins = binBlocks
    .map((block) => block.match(/^\s*name\s*=\s*"([^"]+)"/m)?.[1] ?? null)
    .filter((value): value is string => value != null);

  if (bins.length > 0) {
    return bins;
  }

  const packageName = readCargoPackageName(manifestPath);
  const manifestDir = dirname(manifestPath);
  if (existsSync(resolve(manifestDir, "src/main.rs"))) {
    return [packageName];
  }

  return [];
}

function discoverCargoBins(repoRoot: string): DiscoveredSurface[] {
  const surfaces: DiscoveredSurface[] = [];
  const appsDir = resolve(repoRoot, "apps");
  for (const entry of readdirSync(appsDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) {
      continue;
    }

    const manifestPath = resolve(appsDir, entry.name, "Cargo.toml");
    if (!existsSync(manifestPath)) {
      continue;
    }

    const manifestDir = dirname(manifestPath);
    for (const binName of readCargoBinNames(manifestPath)) {
      const mainPath = resolve(manifestDir, "src/main.rs");
      surfaces.push({
        key: `cargo-bin:${binName}`,
        kind: "cargo-bin",
        command: `cargo run --bin ${binName}`,
        cwd: ".",
        entrypoint: normalizeRepoPath(relative(repoRoot, mainPath)),
      });
    }
  }
  return surfaces.sort((left, right) => left.key.localeCompare(right.key));
}

function discoverCargoExamples(repoRoot: string): DiscoveredSurface[] {
  const surfaces: DiscoveredSurface[] = [];
  for (const rootDirName of ["crates", "apps"]) {
    const rootDir = resolve(repoRoot, rootDirName);
    for (const entry of readdirSync(rootDir, { withFileTypes: true })) {
      if (!entry.isDirectory()) {
        continue;
      }

      const packageDir = resolve(rootDir, entry.name);
      const manifestPath = resolve(packageDir, "Cargo.toml");
      const examplesDir = resolve(packageDir, "examples");
      if (!existsSync(manifestPath) || !existsSync(examplesDir)) {
        continue;
      }

      const packageName = readCargoPackageName(manifestPath);
      for (const example of readdirSync(examplesDir, { withFileTypes: true })) {
        if (!example.isFile() || !example.name.endsWith(".rs")) {
          continue;
        }
        const exampleName = example.name.replace(/\.rs$/, "");
        surfaces.push({
          key: `cargo-example:${packageName}:${exampleName}`,
          kind: "cargo-example",
          command: `cargo run -p ${packageName} --example ${exampleName} --`,
          cwd: ".",
          entrypoint: normalizeRepoPath(
            relative(repoRoot, resolve(examplesDir, example.name)),
          ),
        });
      }
    }
  }
  return surfaces.sort((left, right) => left.key.localeCompare(right.key));
}

function discoverPackageScripts(repoRoot: string): DiscoveredSurface[] {
  const surfaces: DiscoveredSurface[] = [];
  const appsDir = resolve(repoRoot, "apps");
  for (const entry of readdirSync(appsDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) {
      continue;
    }
    const packagePath = resolve(appsDir, entry.name, "package.json");
    if (!existsSync(packagePath)) {
      continue;
    }

    const packageJson = JSON.parse(readText(packagePath)) as {
      scripts?: Record<string, string>;
    };
    const scripts = packageJson.scripts ?? {};
    const cwd = normalizeRepoPath(relative(repoRoot, dirname(packagePath)));
    for (const scriptName of Object.keys(scripts).sort()) {
      surfaces.push({
        key: `bun-package-script:${cwd}:${scriptName}`,
        kind: "bun-package-script",
        command: `bun run ${scriptName}`,
        cwd,
        entrypoint: normalizeRepoPath(relative(repoRoot, packagePath)),
      });
    }
  }
  return surfaces.sort((left, right) => left.key.localeCompare(right.key));
}

function discoverBunScripts(repoRoot: string): DiscoveredSurface[] {
  const surfaces: DiscoveredSurface[] = [];
  const scriptsDir = resolve(repoRoot, "scripts");
  for (const entry of readdirSync(scriptsDir, { withFileTypes: true })) {
    if (!entry.isFile()) {
      continue;
    }
    if (entry.name.endsWith(".test.ts")) {
      continue;
    }
    if (!entry.name.endsWith(".ts") && !entry.name.endsWith(".mjs")) {
      continue;
    }

    const scriptPath = resolve(scriptsDir, entry.name);
    const contents = readText(scriptPath);
    if (!contents.startsWith("#!/usr/bin/env bun")) {
      continue;
    }

    const repoPath = normalizeRepoPath(relative(repoRoot, scriptPath));
    surfaces.push({
      key: `bun-script:${repoPath}`,
      kind: "bun-script",
      command: `bun ./${repoPath}`,
      cwd: ".",
      entrypoint: repoPath,
    });
  }
  return surfaces.sort((left, right) => left.key.localeCompare(right.key));
}

export function discoverSupportedCliSurfaces(repoRoot: string): DiscoveredSurface[] {
  return [
    ...discoverCargoBins(repoRoot),
    ...discoverCargoExamples(repoRoot),
    ...discoverPackageScripts(repoRoot),
    ...discoverBunScripts(repoRoot),
  ].sort((left, right) => left.key.localeCompare(right.key));
}

export function buildCliSurfaceCatalog(repoRoot: string): CliSurfaceCatalog {
  return {
    schemaVersion: 1,
    sourceOfTruth: CLI_SOURCE_OF_TRUTH_PATH,
    scope: CLI_SCOPE,
    commands: CLI_SURFACE.map((entry) => ({
      ...entry,
      audiences: [...entry.audiences],
      outputArtifacts: [...entry.outputArtifacts],
      docs: [...entry.docs],
      env: [...entry.env],
      notes: [...entry.notes],
      coverage: [...entry.coverage],
    })),
    serverEnvironment: SERVER_ENVIRONMENT.map((entry) => ({
      ...entry,
      usedBy: [...entry.usedBy],
    })),
    discoveredSurfaces: discoverSupportedCliSurfaces(repoRoot),
  };
}

function escapeMarkdownCell(value: string): string {
  return value.replace(/\|/g, "\\|").replace(/\n/g, "<br>");
}

function toDocsRelativeLink(repoRoot: string, repoPath: string): string {
  const docsDir = resolve(repoRoot, "docs");
  return normalizeRepoPath(relative(docsDir, resolve(repoRoot, repoPath)));
}

function renderAudiencePresence(entry: CliSurfaceEntry, audience: CliAudience): string {
  return entry.audiences.includes(audience) ? "yes" : "";
}

function renderLinkList(repoRoot: string, paths: string[]): string {
  if (paths.length === 0) {
    return "-";
  }
  return paths
    .map((path) => {
      const label = escapeMarkdownCell(path);
      return `[${label}](${toDocsRelativeLink(repoRoot, path)})`;
    })
    .join(", ");
}

export function renderCliSurfaceMarkdown(
  catalog: CliSurfaceCatalog,
  repoRoot: string,
): string {
  const lines: string[] = [
    "# CLI Surface",
    "",
    "_Generated by `bun ./scripts/verify_cli_surface.ts --write`. Do not hand-edit this file._",
    "",
    "This is the canonical Prompt or Die CLI catalog for developers, users, agents, and developers who use agents to build on the platform.",
    "",
    "## Scope",
    "",
    `- ${catalog.scope}`,
    `- Source of truth: \`${catalog.sourceOfTruth}\``,
    `- Verification command: \`bun ./scripts/verify_cli_surface.ts --check\``,
    `- Agent export: \`bun ./scripts/verify_cli_surface.ts --json\``,
    "",
    "## Audience matrix",
    "",
    "| ID | Developer | User | Agent | Agent-developer | Command |",
    "| --- | --- | --- | --- | --- | --- |",
  ];

  for (const entry of catalog.commands) {
    lines.push(
      `| ${escapeMarkdownCell(entry.id)} | ${renderAudiencePresence(entry, "developer")} | ${renderAudiencePresence(entry, "user")} | ${renderAudiencePresence(entry, "agent")} | ${renderAudiencePresence(entry, "agent-developer")} | \`${escapeMarkdownCell(entry.command)}\` |`,
    );
  }

  lines.push(
    "",
    "## Command catalog",
    "",
    "| ID | Area | Kind | Machine-readable | CWD | Entrypoint | Outputs | Summary | References |",
    "| --- | --- | --- | --- | --- | --- | --- | --- | --- |",
  );

  for (const entry of catalog.commands) {
    const entrypoint = entry.entrypoint
      ? `[${escapeMarkdownCell(entry.entrypoint)}](${toDocsRelativeLink(repoRoot, entry.entrypoint)})`
      : "-";
    const outputs =
      entry.outputArtifacts.length === 0
        ? "-"
        : entry.outputArtifacts.map((artifact) => `\`${escapeMarkdownCell(artifact)}\``).join(", ");
    lines.push(
      `| ${escapeMarkdownCell(entry.id)} | ${escapeMarkdownCell(entry.area)} | ${escapeMarkdownCell(entry.kind)} | ${entry.machineReadable ? "yes" : "no"} | \`${escapeMarkdownCell(entry.cwd)}\` | ${entrypoint} | ${outputs} | ${escapeMarkdownCell(entry.summary)} | ${renderLinkList(repoRoot, entry.docs)} |`,
    );
  }

  lines.push(
    "",
    "## Dedicated server environment contract",
    "",
    "These variables define the current `pod-server` runtime surface because the dedicated server is configured through environment rather than command-line flags.",
    "",
    "| Variable | Default | Used by | Description |",
    "| --- | --- | --- | --- |",
  );

  for (const variable of catalog.serverEnvironment) {
    lines.push(
      `| \`${escapeMarkdownCell(variable.name)}\` | \`${escapeMarkdownCell(variable.defaultValue)}\` | ${variable.usedBy.map((id) => `\`${escapeMarkdownCell(id)}\``).join(", ")} | ${escapeMarkdownCell(variable.description)} |`,
    );
  }

  lines.push(
    "",
    "## Validation workflow",
    "",
    "- `bun ./scripts/verify_cli_surface.ts --check` verifies catalog coverage and Markdown drift.",
    "- `bun ./scripts/verify_cli_surface.ts --write` regenerates this document from the source manifest.",
    "- `bun ./scripts/verify_cli_surface.ts --json` prints the full machine-readable catalog and validation report.",
    "",
  );

  return `${lines.join("\n")}\n`;
}

export function validateCliSurfaceCatalog(
  catalog: CliSurfaceCatalog,
  repoRoot: string,
): CliSurfaceValidation {
  const duplicateIds = catalog.commands
    .map((entry) => entry.id)
    .filter((id, index, ids) => ids.indexOf(id) !== index)
    .filter((id, index, ids) => ids.indexOf(id) === index);

  const missingEntrypoints = catalog.commands
    .map((entry) => entry.entrypoint)
    .filter((entrypoint): entrypoint is string => entrypoint != null)
    .filter((entrypoint) => !existsSync(resolve(repoRoot, entrypoint)));

  const missingDocs = catalog.commands
    .flatMap((entry) => entry.docs)
    .filter((path, index, paths) => paths.indexOf(path) === index)
    .filter((path) => !existsSync(resolve(repoRoot, path)));

  const discoveredKeys = new Set(catalog.discoveredSurfaces.map((surface) => surface.key));
  const coveredKeys = new Set(catalog.commands.flatMap((entry) => entry.coverage));

  const unknownCoverageKeys = Array.from(coveredKeys)
    .filter((key) => !discoveredKeys.has(key))
    .sort();

  const uncoveredDiscoveredSurfaces = catalog.discoveredSurfaces.filter(
    (surface) => !coveredKeys.has(surface.key),
  );

  const generatedMarkdown = renderCliSurfaceMarkdown(catalog, repoRoot);
  const docPath = CLI_DOC_PATH;
  const currentMarkdown = existsSync(resolve(repoRoot, docPath))
    ? readText(resolve(repoRoot, docPath))
    : null;
  const docInSync = currentMarkdown === generatedMarkdown;

  return {
    ok:
      duplicateIds.length === 0 &&
      missingEntrypoints.length === 0 &&
      missingDocs.length === 0 &&
      unknownCoverageKeys.length === 0 &&
      uncoveredDiscoveredSurfaces.length === 0 &&
      docInSync,
    duplicateIds,
    missingEntrypoints,
    missingDocs,
    unknownCoverageKeys,
    uncoveredDiscoveredSurfaces,
    docPath,
    docInSync,
    generatedMarkdown,
  };
}
