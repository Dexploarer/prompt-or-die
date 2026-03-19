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
  | "export"
  | "benchmark"
  | "history"
  | "catalog";

export type CliKind =
  | "workspace-command"
  | "cargo-bin"
  | "cargo-example"
  | "bun-package-script"
  | "bun-script";

export type CliLifecycle = "finite" | "long-running";

export type CliExecutionSpec = {
  program: string;
  args: string[];
  cwd: string;
  lifecycle: CliLifecycle;
  passthrough: "disabled" | "append-after-double-dash";
  allowedEnvOverrides: string[];
};

export type CliSurfaceCapabilities = {
  supportsDryRun: boolean;
  supportsPassthrough: boolean;
  supportsEffectiveEnv: boolean;
  requiresNetwork: boolean;
  mutatesState: boolean;
  attachesToTerminal: boolean;
};

export type CliInteractiveSpec = {
  transport: "stdio-json";
  encoding: "json";
  framing: "newline-delimited";
  protocolVersion: 3;
  requestTypes: string[];
  builtins: string[];
  hookableEvents: string[];
  events: string[];
};

export type CliSurfaceEntry = {
  id: string;
  name: string;
  aliases: string[];
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
  execution: CliExecutionSpec;
  capabilities: CliSurfaceCapabilities;
  interactive: CliInteractiveSpec | null;
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
  schemaVersion: 2;
  sourceOfTruth: string;
  scope: string;
  commands: CliSurfaceEntry[];
  serverEnvironment: ServerEnvironmentVariable[];
  discoveredSurfaces: DiscoveredSurface[];
};

export type CliSurfaceValidation = {
  ok: boolean;
  duplicateIds: string[];
  duplicateAliases: string[];
  invalidAliasEntries: string[];
  missingEntrypoints: string[];
  missingDocs: string[];
  unknownCoverageKeys: string[];
  uncoveredDiscoveredSurfaces: DiscoveredSurface[];
  invalidExecutionEntries: string[];
  invalidCapabilityEntries: string[];
  invalidPassthroughEntries: string[];
  invalidInteractiveEntries: string[];
  docPath: string;
  docInSync: boolean;
  generatedMarkdown: string;
};

export const CLI_DOC_PATH = "docs/cli-surface.md";
export const CLI_SOURCE_OF_TRUTH_PATH = "scripts/cli_surface.ts";
export const CLI_AUDIENCES: CliAudience[] = [
  "developer",
  "user",
  "agent",
  "agent-developer",
];
export const CLI_AREAS: CliArea[] = [
  "workspace",
  "runtime",
  "web",
  "assets",
  "export",
  "benchmark",
  "history",
  "catalog",
];
export const CLI_KINDS: CliKind[] = [
  "workspace-command",
  "cargo-bin",
  "cargo-example",
  "bun-package-script",
  "bun-script",
];
export const CLI_SCOPE =
  "The CLI catalog covers supported top-level workspace commands, cargo binaries, cargo examples, root Bun scripts, and packaged app scripts. It intentionally excludes one-off targeted test invocations and third-party prerequisite tools such as cargo, bun, bunx, Playwright, or SpacetimeDB commands that are not owned by this repository.";

type CliSurfaceEntryInput = Omit<CliSurfaceEntry, "command" | "cwd" | "interactive"> & {
  interactive?: CliInteractiveSpec | null;
};

function shellQuote(token: string): string {
  if (/^[A-Za-z0-9_./:=+-]+$/.test(token)) {
    return token;
  }
  return `'${token.replace(/'/g, `'\\''`)}'`;
}

export function renderCliExecutionCommand(
  execution: Pick<CliExecutionSpec, "program" | "args">,
  extraArgs: string[] = [],
): string {
  return [execution.program, ...execution.args, ...extraArgs]
    .map((token) => shellQuote(token))
    .join(" ");
}

function createExecution(options: {
  program: string;
  args: string[];
  cwd?: string;
  lifecycle: CliLifecycle;
  passthrough?: boolean;
  allowedEnvOverrides?: string[];
}): CliExecutionSpec {
  return {
    program: options.program,
    args: [...options.args],
    cwd: options.cwd ?? ".",
    lifecycle: options.lifecycle,
    passthrough: options.passthrough ? "append-after-double-dash" : "disabled",
    allowedEnvOverrides: [...(options.allowedEnvOverrides ?? [])],
  };
}

function createCapabilities(
  overrides: Partial<CliSurfaceCapabilities> = {},
): CliSurfaceCapabilities {
  return {
    supportsDryRun: true,
    supportsPassthrough: false,
    supportsEffectiveEnv: false,
    requiresNetwork: false,
    mutatesState: true,
    attachesToTerminal: false,
    ...overrides,
  };
}

function createInteractiveShellSpec(options: {
  requestTypes: string[];
  builtins: string[];
  hookableEvents: string[];
  events: string[];
}): CliInteractiveSpec {
  return {
    transport: "stdio-json",
    encoding: "json",
    framing: "newline-delimited",
    protocolVersion: 3,
    requestTypes: [...options.requestTypes],
    builtins: [...options.builtins],
    hookableEvents: [...options.hookableEvents],
    events: [...options.events],
  };
}

function createEntry(input: CliSurfaceEntryInput): CliSurfaceEntry {
  return {
    ...input,
    aliases: [...input.aliases],
    command: renderCliExecutionCommand(input.execution),
    cwd: input.execution.cwd,
    execution: {
      ...input.execution,
      args: [...input.execution.args],
      allowedEnvOverrides: [...input.execution.allowedEnvOverrides],
    },
    capabilities: { ...input.capabilities },
    interactive: input.interactive
      ? {
          ...input.interactive,
          requestTypes: [...input.interactive.requestTypes],
          builtins: [...input.interactive.builtins],
          hookableEvents: [...input.interactive.hookableEvents],
          events: [...input.interactive.events],
        }
      : null,
  };
}

const POD_SERVER_ENV = [
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
];

export const CLI_SURFACE: CliSurfaceEntry[] = [
  createEntry({
    id: "workspace-build",
    name: "Workspace Build",
    aliases: ["workspace build"],
    audiences: ["developer", "agent-developer"],
    area: "workspace",
    kind: "workspace-command",
    entrypoint: null,
    summary: "Build every Rust package in the workspace.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["README.md", "AGENTS.md"],
    env: [],
    notes: [],
    coverage: [],
    execution: createExecution({
      program: "cargo",
      args: ["build", "--workspace"],
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "workspace-check",
    name: "Workspace Check",
    aliases: ["workspace check"],
    audiences: ["developer", "agent-developer"],
    area: "workspace",
    kind: "workspace-command",
    entrypoint: null,
    summary:
      "Run the fast Rust workspace compile gate used by CI and local review loops.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["README.md", "AGENTS.md"],
    env: [],
    notes: [],
    coverage: [],
    execution: createExecution({
      program: "cargo",
      args: ["check", "--workspace"],
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "workspace-test",
    name: "Workspace Test",
    aliases: ["workspace test"],
    audiences: ["developer", "agent-developer"],
    area: "workspace",
    kind: "workspace-command",
    entrypoint: null,
    summary: "Run the full Rust workspace test suite.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["README.md", "AGENTS.md"],
    env: [],
    notes: [],
    coverage: [],
    execution: createExecution({
      program: "cargo",
      args: ["test", "--workspace"],
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "workspace-clippy",
    name: "Workspace Clippy",
    aliases: ["workspace lint"],
    audiences: ["developer", "agent-developer"],
    area: "workspace",
    kind: "workspace-command",
    entrypoint: null,
    summary:
      "Run the workspace lint gate with warnings treated as failures.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["AGENTS.md"],
    env: [],
    notes: ["Mirrors the repo CI lint posture."],
    coverage: [],
    execution: createExecution({
      program: "cargo",
      args: ["clippy", "--workspace", "--", "-D", "warnings"],
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "prompt-or-die",
    name: "Desktop Runtime",
    aliases: ["runtime desktop"],
    audiences: ["user", "developer"],
    area: "runtime",
    kind: "cargo-bin",
    entrypoint: "apps/pod-desktop/src/main.rs",
    summary: "Launch the native desktop runtime entrypoint.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["README.md"],
    env: [],
    notes: [
      "No user-tunable CLI flags are currently parsed by the desktop binary.",
    ],
    coverage: ["cargo-bin:prompt-or-die"],
    execution: createExecution({
      program: "cargo",
      args: ["run", "--bin", "prompt-or-die"],
      lifecycle: "long-running",
    }),
    capabilities: createCapabilities({
      attachesToTerminal: true,
      requiresNetwork: false,
    }),
  }),
  createEntry({
    id: "pod-server",
    name: "Dedicated Server",
    aliases: ["runtime server"],
    audiences: ["user", "developer", "agent-developer"],
    area: "runtime",
    kind: "cargo-bin",
    entrypoint: "apps/pod-server/src/main.rs",
    summary:
      "Launch the dedicated authoritative server composition root.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["README.md"],
    env: POD_SERVER_ENV,
    notes: ["The server surface is env-driven today rather than flag-driven."],
    coverage: ["cargo-bin:pod-server"],
    execution: createExecution({
      program: "cargo",
      args: ["run", "--bin", "pod-server"],
      lifecycle: "long-running",
      allowedEnvOverrides: POD_SERVER_ENV,
    }),
    capabilities: createCapabilities({
      supportsEffectiveEnv: true,
      requiresNetwork: true,
      attachesToTerminal: true,
    }),
  }),
  createEntry({
    id: "pod-headless",
    name: "Headless Tournament Runner",
    aliases: ["runtime headless"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "runtime",
    kind: "cargo-bin",
    entrypoint: "apps/pod-headless/src/main.rs",
    summary:
      "Run the authoritative headless tournament and export report, dataset, and topology artifacts.",
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
    execution: createExecution({
      program: "cargo",
      args: [
        "run",
        "--bin",
        "pod-headless",
        "--",
        "--profile",
        "ci-smoke",
        "--output",
        "artifacts/pod-headless-report.json",
        "--dataset-output",
        "artifacts/pod-headless-dataset.json",
        "--topology-output",
        "artifacts/pod-headless-topology.json",
      ],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      requiresNetwork: false,
    }),
  }),
  createEntry({
    id: "controller-parity-benchmark",
    name: "Controller Parity Benchmark",
    aliases: ["benchmark controller-parity"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "benchmark",
    kind: "cargo-example",
    entrypoint: "crates/pod-agents/examples/controller_parity_benchmark.rs",
    summary:
      "Measure controller parity across runtime implementations and optionally fail on unmet checks.",
    machineReadable: true,
    outputArtifacts: ["artifacts/controller-parity.json"],
    docs: ["README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["cargo-example:pod-agents:controller_parity_benchmark"],
    execution: createExecution({
      program: "cargo",
      args: [
        "run",
        "-q",
        "-p",
        "pod-agents",
        "--example",
        "controller_parity_benchmark",
        "--",
        "--fail-on-checks",
        "--output",
        "artifacts/controller-parity.json",
      ],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      requiresNetwork: false,
    }),
  }),
  createEntry({
    id: "stage-import",
    name: "Stage Import",
    aliases: ["assets stage-import"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "assets",
    kind: "cargo-example",
    entrypoint: "crates/pod-assets/examples/stage_import.rs",
    summary:
      "Stage one or more authored assets into the content-addressed import pipeline and optionally emit JSON.",
    machineReadable: true,
    outputArtifacts: ["artifacts/staged-assets"],
    docs: ["README.md", "docs/asset-pipeline.md", "apps/pod-web/README.md"],
    env: [],
    notes: [
      "Supports `--materialize-runtime`, `--base-dir`, and `--bundle-spec` for runtime bundle generation.",
    ],
    coverage: ["cargo-example:pod-assets:stage_import"],
    execution: createExecution({
      program: "cargo",
      args: [
        "run",
        "-p",
        "pod-assets",
        "--example",
        "stage_import",
        "--",
        "--json",
        "--output-root",
        "artifacts/staged-assets",
        "path/to/asset.glb",
      ],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      requiresNetwork: false,
    }),
  }),
  createEntry({
    id: "moat-benchmark-suite",
    name: "Core Moat Benchmark Suite",
    aliases: ["benchmark moat"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "benchmark",
    kind: "cargo-example",
    entrypoint: "crates/pod-core/examples/moat_benchmark_suite.rs",
    summary:
      "Run the core moat benchmark suite for cost and throughput posture.",
    machineReadable: true,
    outputArtifacts: ["artifacts/moat-core.json"],
    docs: ["docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["cargo-example:pod-core:moat_benchmark_suite"],
    execution: createExecution({
      program: "cargo",
      args: [
        "run",
        "-p",
        "pod-core",
        "--example",
        "moat_benchmark_suite",
        "--release",
        "--",
        "--profile",
        "shard-target",
        "--monthly-host-cost-usd",
        "300",
        "--output",
        "artifacts/moat-core.json",
      ],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      requiresNetwork: false,
    }),
  }),
  createEntry({
    id: "transport-benchmark-suite",
    name: "Transport Benchmark Suite",
    aliases: ["benchmark transport"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "benchmark",
    kind: "cargo-example",
    entrypoint: "crates/pod-net/examples/transport_benchmark_suite.rs",
    summary:
      "Measure snapshot, recovery, delta, and queue-pressure transport behavior.",
    machineReadable: true,
    outputArtifacts: ["artifacts/transport-benchmark.json"],
    docs: ["docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["cargo-example:pod-net:transport_benchmark_suite"],
    execution: createExecution({
      program: "cargo",
      args: [
        "run",
        "-p",
        "pod-net",
        "--example",
        "transport_benchmark_suite",
        "--",
        "--profile",
        "shard-target",
        "--fail-on-checks",
        "--output",
        "artifacts/transport-benchmark.json",
      ],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      requiresNetwork: false,
    }),
  }),
  createEntry({
    id: "topology-feed-benchmark-suite",
    name: "Topology Feed Benchmark Suite",
    aliases: ["benchmark topology-feed"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "benchmark",
    kind: "cargo-example",
    entrypoint: "crates/pod-net/examples/topology_feed_benchmark_suite.rs",
    summary:
      "Validate authority-row and generated-runtime topology parity from a shared topology artifact.",
    machineReadable: true,
    outputArtifacts: ["artifacts/topology-feed-benchmark.json"],
    docs: ["README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [
      "Supports live generated SDK parity checks through `--generated-sdk-host` and related flags.",
    ],
    coverage: ["cargo-example:pod-net:topology_feed_benchmark_suite"],
    execution: createExecution({
      program: "cargo",
      args: [
        "run",
        "-p",
        "pod-net",
        "--features",
        "spacetimedb",
        "--example",
        "topology_feed_benchmark_suite",
        "--",
        "--topology-input",
        "artifacts/pod-headless-topology.json",
        "--fail-on-checks",
        "--output",
        "artifacts/topology-feed-benchmark.json",
      ],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      requiresNetwork: false,
    }),
  }),
  createEntry({
    id: "pod-web-dev",
    name: "pod-web Dev Server",
    aliases: ["web dev"],
    audiences: ["developer", "user"],
    area: "web",
    kind: "bun-package-script",
    entrypoint: "apps/pod-web/package.json",
    summary: "Run the browser client development server.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["README.md", "apps/pod-web/README.md", "docs/reference-bootstrap.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:dev"],
    execution: createExecution({
      program: "bun",
      args: ["run", "dev"],
      cwd: "apps/pod-web",
      lifecycle: "long-running",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      attachesToTerminal: true,
    }),
  }),
  createEntry({
    id: "pod-web-build",
    name: "pod-web Build",
    aliases: ["web build"],
    audiences: ["developer", "agent-developer"],
    area: "web",
    kind: "bun-package-script",
    entrypoint: "apps/pod-web/package.json",
    summary: "Build the browser client for production or preview.",
    machineReadable: false,
    outputArtifacts: ["apps/pod-web/dist"],
    docs: ["apps/pod-web/README.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:build"],
    execution: createExecution({
      program: "bun",
      args: ["run", "build"],
      cwd: "apps/pod-web",
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "pod-web-preview",
    name: "pod-web Preview",
    aliases: ["web preview"],
    audiences: ["developer", "user"],
    area: "web",
    kind: "bun-package-script",
    entrypoint: "apps/pod-web/package.json",
    summary: "Serve the built browser bundle locally for previewing.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["apps/pod-web/README.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:preview"],
    execution: createExecution({
      program: "bun",
      args: ["run", "preview"],
      cwd: "apps/pod-web",
      lifecycle: "long-running",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      attachesToTerminal: true,
    }),
  }),
  createEntry({
    id: "pod-web-sync-assets",
    name: "pod-web Sync Assets",
    aliases: ["assets sync-web"],
    audiences: ["developer", "agent-developer"],
    area: "assets",
    kind: "bun-package-script",
    entrypoint: "apps/pod-web/package.json",
    summary:
      "Generate the sample authored asset lane and materialize the runtime bundle into the browser app.",
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
    execution: createExecution({
      program: "bun",
      args: ["run", "sync:assets"],
      cwd: "apps/pod-web",
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "pod-web-verify-assets",
    name: "pod-web Verify Assets",
    aliases: ["assets verify-web"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "assets",
    kind: "bun-package-script",
    entrypoint: "apps/pod-web/package.json",
    summary:
      "Re-run the generated asset pipeline and fail if committed browser assets drift.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["apps/pod-web/README.md", "docs/asset-pipeline.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:verify:assets"],
    execution: createExecution({
      program: "bun",
      args: ["run", "verify:assets"],
      cwd: "apps/pod-web",
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "pod-web-measure-render-routes",
    name: "pod-web Measure Render Routes",
    aliases: ["web measure-render-routes"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "web",
    kind: "bun-package-script",
    entrypoint: "apps/pod-web/package.json",
    summary:
      "Capture render-route measurements for the main-thread and worker browser paths.",
    machineReadable: true,
    outputArtifacts: ["apps/pod-web/artifacts/render-route-measurements.json"],
    docs: ["apps/pod-web/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:measure:render-routes"],
    execution: createExecution({
      program: "bun",
      args: ["run", "measure:render-routes"],
      cwd: "apps/pod-web",
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "pod-web-measure-render-routes-check",
    name: "pod-web Measure Render Routes Check",
    aliases: ["web measure-render-routes-check"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "web",
    kind: "bun-package-script",
    entrypoint: "apps/pod-web/package.json",
    summary:
      "Capture render-route measurements and fail when the configured browser gates regress.",
    machineReadable: true,
    outputArtifacts: ["apps/pod-web/artifacts/render-route-measurements.json"],
    docs: ["apps/pod-web/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:measure:render-routes:check"],
    execution: createExecution({
      program: "bun",
      args: ["run", "measure:render-routes:check"],
      cwd: "apps/pod-web",
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "pod-web-typecheck",
    name: "pod-web Typecheck",
    aliases: ["web typecheck"],
    audiences: ["developer", "agent-developer"],
    area: "web",
    kind: "bun-package-script",
    entrypoint: "apps/pod-web/package.json",
    summary:
      "Run the browser client TypeScript compile gate without emitting files.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["apps/pod-web/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:typecheck"],
    execution: createExecution({
      program: "bun",
      args: ["run", "typecheck"],
      cwd: "apps/pod-web",
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "pod-web-test",
    name: "pod-web Unit Tests",
    aliases: ["web test"],
    audiences: ["developer", "agent-developer"],
    area: "web",
    kind: "bun-package-script",
    entrypoint: "apps/pod-web/package.json",
    summary: "Run the browser client Bun unit test suite.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["apps/pod-web/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:test"],
    execution: createExecution({
      program: "bun",
      args: ["run", "test"],
      cwd: "apps/pod-web",
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "pod-web-test-smoke",
    name: "pod-web Smoke Tests",
    aliases: ["web smoke"],
    audiences: ["developer", "agent-developer"],
    area: "web",
    kind: "bun-package-script",
    entrypoint: "apps/pod-web/package.json",
    summary:
      "Run the Playwright browser smoke tests covering showcase and worker-input paths.",
    machineReadable: false,
    outputArtifacts: [],
    docs: ["apps/pod-web/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-package-script:apps/pod-web:test:smoke"],
    execution: createExecution({
      program: "bun",
      args: ["run", "test:smoke"],
      cwd: "apps/pod-web",
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "bootstrap-reference-world",
    name: "Bootstrap Reference World",
    aliases: ["assets bootstrap-reference-world"],
    audiences: ["developer", "user", "agent", "agent-developer"],
    area: "runtime",
    kind: "bun-script",
    entrypoint: "scripts/bootstrap_reference_world.ts",
    summary:
      "Launch or measure the canonical first-world bootstrap flow for the browser client.",
    machineReadable: true,
    outputArtifacts: [],
    docs: ["docs/reference-bootstrap.md", "docs/benchmark-suite.md"],
    env: [],
    notes: ["`--measure` prints JSON with startup timing and resolved URL."],
    coverage: ["bun-script:scripts/bootstrap_reference_world.ts"],
    execution: createExecution({
      program: "bun",
      args: ["./scripts/bootstrap_reference_world.ts", "--hold"],
      lifecycle: "long-running",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      attachesToTerminal: true,
    }),
  }),
  createEntry({
    id: "pod-export-world",
    name: "Export World Snapshot",
    aliases: ["export world"],
    audiences: ["developer", "user", "agent", "agent-developer"],
    area: "export",
    kind: "workspace-command",
    entrypoint: "scripts/pod.ts",
    summary:
      "Export the canonical agent-facing world snapshot as JSON or TOON.",
    machineReadable: true,
    outputArtifacts: [],
    docs: ["README.md", "docs/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [
      "Use `--format json` for generic automation and `--format toon` when the world snapshot is being prepared for LLM context windows.",
      "The export data shape is intentionally deterministic and owned by the repo-local POD SDK facade in `scripts/pod_sdk.ts`.",
    ],
    coverage: [],
    execution: createExecution({
      program: "bun",
      args: ["./scripts/pod.ts", "export", "world", "--format", "json"],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      mutatesState: false,
    }),
  }),
  createEntry({
    id: "pod-export-events",
    name: "Export Tick Event Batch",
    aliases: ["export events"],
    audiences: ["developer", "user", "agent", "agent-developer"],
    area: "export",
    kind: "workspace-command",
    entrypoint: "scripts/pod.ts",
    summary:
      "Export the stable tick/event batch as JSON or TOON for replay-aware agent workflows.",
    machineReadable: true,
    outputArtifacts: [],
    docs: ["README.md", "docs/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [
      "This is the strongest TOON-fit dataset in the repo because the payload is a uniform array of event rows.",
      "Use `--format toon` to reproduce the benchmarked tabular export path.",
    ],
    coverage: [],
    execution: createExecution({
      program: "bun",
      args: ["./scripts/pod.ts", "export", "events", "--format", "toon"],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      mutatesState: false,
    }),
  }),
  createEntry({
    id: "pod-export-multiverse",
    name: "Export Multiverse Index",
    aliases: ["export multiverse"],
    audiences: ["developer", "user", "agent", "agent-developer"],
    area: "export",
    kind: "workspace-command",
    entrypoint: "scripts/pod.ts",
    summary:
      "Export the deep multiverse/branch index as JSON or TOON for topology proofs and agent audits.",
    machineReadable: true,
    outputArtifacts: [],
    docs: ["README.md", "docs/README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [
      "The benchmark suite keeps this shape honest: JSON remains the preferred format when the branch metadata tree is too deep for TOON to win cleanly.",
      "TOON remains available for parity and inspection, but it is not the default recommendation for this dataset.",
    ],
    coverage: [],
    execution: createExecution({
      program: "bun",
      args: ["./scripts/pod.ts", "export", "multiverse", "--format", "json"],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      mutatesState: false,
    }),
  }),
  createEntry({
    id: "run-moat-benchmarks",
    name: "Run Moat Benchmarks",
    aliases: ["benchmark run-moat-benchmarks"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "benchmark",
    kind: "bun-script",
    entrypoint: "scripts/run_moat_benchmarks.ts",
    summary:
      "Run the combined benchmark workflow across core, transport, topology, browser, and creator-time surfaces.",
    machineReadable: true,
    outputArtifacts: ["artifacts/moat-benchmarks.json"],
    docs: ["docs/benchmark-suite.md", "docs/reference-bootstrap.md"],
    env: [],
    notes: [],
    coverage: ["bun-script:scripts/run_moat_benchmarks.ts"],
    execution: createExecution({
      program: "bun",
      args: [
        "./scripts/run_moat_benchmarks.ts",
        "--profile",
        "shard-target",
        "--monthly-host-cost-usd",
        "300",
        "--output",
        "artifacts/moat-benchmarks.json",
      ],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
    }),
  }),
  createEntry({
    id: "toon-export-benchmark",
    name: "TOON Export Benchmark",
    aliases: ["benchmark toon-exports"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "benchmark",
    kind: "bun-script",
    entrypoint: "scripts/benchmark_toon_exports.ts",
    summary:
      "Benchmark JSON and TOON against real world, event, log, and multiverse datasets, then emit charts and a standalone results page.",
    machineReadable: true,
    outputArtifacts: [
      "artifacts/toon-export-benchmark.json",
      "artifacts/toon-export-benchmark.html",
      "artifacts/toon-export-benchmark.md",
      "artifacts/toon-export-benchmark-charts",
    ],
    docs: ["docs/benchmark-suite.md", "docs/cli-surface.md"],
    env: [],
    notes: [
      "Compares pretty JSON, compact JSON, TOON comma, and TOON tab across uniform records, semi-uniform logs, nested world snapshots, and deep multiverse metadata trees.",
      "Uses strict TOON decode validation and streaming decode event counts so the proof suite exercises the tabular/document workflow the official TOON docs optimize for.",
      "Use --profile extensive for the heavy report run and pair it with --html-output, --markdown-output, and --charts-dir to publish the visual benchmark bundle.",
      "Use --fail-on-checks to make CI fail if the event export stops justifying TOON or if the dataset recommendations drift from the measured winners.",
    ],
    coverage: ["bun-script:scripts/benchmark_toon_exports.ts"],
    execution: createExecution({
      program: "bun",
      args: [
        "./scripts/benchmark_toon_exports.ts",
        "--profile",
        "default",
        "--output",
        "artifacts/toon-export-benchmark.json",
        "--html-output",
        "artifacts/toon-export-benchmark.html",
        "--markdown-output",
        "artifacts/toon-export-benchmark.md",
        "--charts-dir",
        "artifacts/toon-export-benchmark-charts",
        "--fail-on-checks",
      ],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      mutatesState: false,
    }),
  }),
  createEntry({
    id: "run-shard-target-snapshot",
    name: "Run Shard Target Snapshot",
    aliases: ["history run-shard-target-snapshot"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "history",
    kind: "bun-script",
    entrypoint: "scripts/run_shard_target_snapshot.ts",
    summary:
      "Capture the weekly shard-target benchmark snapshot, publish retained artifacts, and optionally compare to baseline history.",
    machineReadable: true,
    outputArtifacts: [
      "artifacts/moat-benchmarks-shard-local.json",
      "docs/benchmark-snapshots/YYYY-Www-shard-target.json",
    ],
    docs: ["README.md", "docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-script:scripts/run_shard_target_snapshot.ts"],
    execution: createExecution({
      program: "bun",
      args: [
        "./scripts/run_shard_target_snapshot.ts",
        "--label",
        "YYYY-Www",
        "--output",
        "artifacts/moat-benchmarks-shard-local.json",
      ],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
    }),
  }),
  createEntry({
    id: "compare-moat-snapshots",
    name: "Compare Moat Snapshots",
    aliases: ["history compare-moat-snapshots"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "history",
    kind: "bun-script",
    entrypoint: "scripts/compare_moat_snapshots.ts",
    summary:
      "Diff two retained shard-target snapshots and surface regressions or improvements as structured JSON.",
    machineReadable: true,
    outputArtifacts: ["artifacts/benchmark-snapshot-comparison.json"],
    docs: ["docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-script:scripts/compare_moat_snapshots.ts"],
    execution: createExecution({
      program: "bun",
      args: [
        "./scripts/compare_moat_snapshots.ts",
        "--baseline",
        "docs/benchmark-snapshots/2026-W10-shard-target.json",
        "--candidate",
        "docs/benchmark-snapshots/2026-W11-shard-target.json",
        "--output",
        "artifacts/benchmark-snapshot-comparison.json",
        "--fail-on-regressions",
      ],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
    }),
  }),
  createEntry({
    id: "publish-moat-snapshots",
    name: "Publish Moat Snapshot",
    aliases: ["history publish-moat-snapshots"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "history",
    kind: "bun-script",
    entrypoint: "scripts/publish_moat_snapshots.ts",
    summary:
      "Normalize a live moat benchmark report into the retained benchmark snapshot format.",
    machineReadable: true,
    outputArtifacts: ["docs/benchmark-snapshots/YYYY-Www-shard-target.json"],
    docs: ["docs/benchmark-suite.md"],
    env: [],
    notes: [],
    coverage: ["bun-script:scripts/publish_moat_snapshots.ts"],
    execution: createExecution({
      program: "bun",
      args: [
        "./scripts/publish_moat_snapshots.ts",
        "--input",
        "artifacts/moat-benchmarks-shard-local.json",
        "--label",
        "YYYY-Www",
        "--output",
        "docs/benchmark-snapshots/YYYY-Www-shard-target.json",
      ],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
    }),
  }),
  createEntry({
    id: "index-benchmark-snapshots",
    name: "Index Benchmark Snapshots",
    aliases: ["history index-benchmark-snapshots"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "history",
    kind: "bun-script",
    entrypoint: "scripts/index_benchmark_snapshots.ts",
    summary:
      "Regenerate the retained benchmark snapshot JSON index and Markdown history view.",
    machineReadable: true,
    outputArtifacts: [
      "docs/benchmark-snapshots/index.json",
      "docs/benchmark-snapshots/README.md",
    ],
    docs: ["docs/benchmark-suite.md", "docs/benchmark-snapshots/README.md"],
    env: [],
    notes: [],
    coverage: ["bun-script:scripts/index_benchmark_snapshots.ts"],
    execution: createExecution({
      program: "bun",
      args: ["./scripts/index_benchmark_snapshots.ts"],
      lifecycle: "finite",
    }),
    capabilities: createCapabilities(),
  }),
  createEntry({
    id: "pod-shell",
    name: "Interactive POD Shell",
    aliases: ["catalog shell"],
    audiences: ["developer", "user", "agent", "agent-developer"],
    area: "catalog",
    kind: "workspace-command",
    entrypoint: "scripts/pod.ts",
    summary:
      "Start the interactive terminal shell for discovering, inspecting, and executing Prompt or Die commands.",
    machineReadable: true,
    outputArtifacts: [],
    docs: ["docs/cli-surface.md"],
    env: [],
    notes: [
      "Use `pod shell` for attached human sessions and `pod shell --agent` for newline-delimited JSON machine sessions.",
      "The agent transport writes JSON event objects to stdout, accepts JSON request objects on stdin, and keeps hooks plus long-running job semantics stable across stdin closure.",
      "TOON is intentionally out of the shell control plane; large LLM-facing payloads live under `pod export ... --format toon` instead.",
      "Long-running agent jobs continue after stdin closes until the managed job set drains, and lifecycle hooks can launch follow-up commands autonomously.",
    ],
    coverage: [],
    execution: createExecution({
      program: "bun",
      args: ["./scripts/pod.ts", "shell"],
      lifecycle: "long-running",
    }),
    capabilities: createCapabilities({
      supportsDryRun: false,
      attachesToTerminal: true,
      mutatesState: false,
    }),
    interactive: createInteractiveShellSpec({
      requestTypes: ["builtin", "command", "hook"],
      builtins: ["help", "context", "exit"],
      hookableEvents: [
        "process.started",
        "process.exited",
        "session.stdin.closed",
      ],
      events: [
        "session.started",
        "session.stdin.closed",
        "command.accepted",
        "command.result",
        "process.started",
        "process.stdout",
        "process.stderr",
        "process.exited",
        "hook.triggered",
        "error",
        "session.ended",
      ],
    }),
  }),
  createEntry({
    id: "pod",
    name: "Root POD CLI",
    aliases: [],
    audiences: ["developer", "user", "agent", "agent-developer"],
    area: "catalog",
    kind: "bun-script",
    entrypoint: "scripts/pod.ts",
    summary:
      "Canonical root CLI for discovering, inspecting, and executing supported Prompt or Die command surfaces.",
    machineReadable: true,
    outputArtifacts: [],
    docs: ["docs/cli-surface.md"],
    env: [],
    notes: [
      "Use `list`, `show`, `env`, `command`, `export`, `run`, and `shell` subcommands.",
    ],
    coverage: ["bun-script:scripts/pod.ts"],
    execution: createExecution({
      program: "bun",
      args: ["./scripts/pod.ts", "list"],
      lifecycle: "finite",
    }),
    capabilities: createCapabilities({
      mutatesState: false,
    }),
  }),
  createEntry({
    id: "verify-cli-surface",
    name: "Verify CLI Surface",
    aliases: ["catalog verify"],
    audiences: ["developer", "agent", "agent-developer"],
    area: "catalog",
    kind: "bun-script",
    entrypoint: "scripts/verify_cli_surface.ts",
    summary:
      "Validate that the CLI catalog covers every supported top-level command surface and that the generated docs are in sync.",
    machineReadable: true,
    outputArtifacts: ["docs/cli-surface.md"],
    docs: ["docs/cli-surface.md"],
    env: [],
    notes: [
      "Use `--json` for agent consumption and `--write` to regenerate the docs page.",
    ],
    coverage: ["bun-script:scripts/verify_cli_surface.ts"],
    execution: createExecution({
      program: "bun",
      args: ["./scripts/verify_cli_surface.ts", "--check"],
      lifecycle: "finite",
      passthrough: true,
    }),
    capabilities: createCapabilities({
      supportsPassthrough: true,
      mutatesState: false,
    }),
  }),
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
    description:
      "Optional shard ops archive directory for retained ops history.",
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
    description:
      "Toggle the browser WebSocket fallback for direct-connect clients.",
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
    schemaVersion: 2,
    sourceOfTruth: CLI_SOURCE_OF_TRUTH_PATH,
    scope: CLI_SCOPE,
    commands: CLI_SURFACE.map((entry) => ({
      ...entry,
      aliases: [...entry.aliases],
      audiences: [...entry.audiences],
      outputArtifacts: [...entry.outputArtifacts],
      docs: [...entry.docs],
      env: [...entry.env],
      notes: [...entry.notes],
      coverage: [...entry.coverage],
      execution: {
        ...entry.execution,
        args: [...entry.execution.args],
        allowedEnvOverrides: [...entry.execution.allowedEnvOverrides],
      },
      capabilities: { ...entry.capabilities },
      interactive: entry.interactive
        ? {
            ...entry.interactive,
            requestTypes: [...entry.interactive.requestTypes],
            builtins: [...entry.interactive.builtins],
            hookableEvents: [...entry.interactive.hookableEvents],
            events: [...entry.interactive.events],
          }
        : null,
    })),
    serverEnvironment: SERVER_ENVIRONMENT.map((entry) => ({
      ...entry,
      usedBy: [...entry.usedBy],
    })),
    discoveredSurfaces: discoverSupportedCliSurfaces(repoRoot),
  };
}

export type CliSurfaceFilters = {
  audience?: CliAudience;
  area?: CliArea;
  kind?: CliKind;
  machineReadableOnly?: boolean;
  text?: string;
};

export function findCliSurfaceEntry(
  catalog: CliSurfaceCatalog,
  id: string,
): CliSurfaceEntry | null {
  return catalog.commands.find((entry) => entry.id === id) ?? null;
}

export function findCliSurfaceEntryByAlias(
  catalog: CliSurfaceCatalog,
  alias: string,
): CliSurfaceEntry | null {
  return catalog.commands.find((entry) => entry.aliases.includes(alias)) ?? null;
}

export function filterCliSurfaceEntries(
  catalog: CliSurfaceCatalog,
  filters: CliSurfaceFilters,
): CliSurfaceEntry[] {
  const query = filters.text?.trim().toLowerCase() ?? "";
  return catalog.commands.filter((entry) => {
    if (filters.audience && !entry.audiences.includes(filters.audience)) {
      return false;
    }
    if (filters.area && entry.area !== filters.area) {
      return false;
    }
    if (filters.kind && entry.kind !== filters.kind) {
      return false;
    }
    if (filters.machineReadableOnly && !entry.machineReadable) {
      return false;
    }
    if (!query) {
      return true;
    }

    return [
      entry.id,
      entry.name,
      entry.summary,
      entry.command,
      entry.area,
      entry.kind,
      entry.execution.lifecycle,
      ...entry.aliases,
      ...entry.audiences,
      ...entry.docs,
      ...entry.env,
      ...entry.notes,
    ]
      .join(" ")
      .toLowerCase()
      .includes(query);
  });
}

export function resolveCliSurfaceCommand(
  entry: CliSurfaceEntry,
  extraArgs: string[] = [],
): string {
  const command = renderCliExecutionCommand(entry.execution, extraArgs);
  if (entry.execution.cwd === ".") {
    return command;
  }
  return `cd ${entry.execution.cwd} && ${command}`;
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

function renderAliasList(aliases: string[]): string {
  if (aliases.length === 0) {
    return "-";
  }
  return aliases.map((alias) => `\`pod ${escapeMarkdownCell(alias)}\``).join("<br>");
}

export function renderCliSurfaceMarkdown(
  catalog: CliSurfaceCatalog,
  repoRoot: string,
): string {
  const shellEntry = catalog.commands.find((entry) => entry.id === "pod-shell");
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
    "## Canonical root CLI",
    "",
    "Use the root `pod` CLI as the human front door. Use stable IDs with `show`, `env`, `command`, `run`, and `--json` for automation.",
    "",
    "```bash",
    "bun ./scripts/pod.ts workspace check",
    "bun ./scripts/pod.ts shell",
    "printf '{\"type\":\"builtin\",\"requestId\":\"1\",\"name\":\"context\"}\\n{\"type\":\"builtin\",\"requestId\":\"2\",\"name\":\"exit\"}\\n' | bun ./scripts/pod.ts shell --agent",
    "bun ./scripts/pod.ts runtime server --dry-run",
    "bun ./scripts/pod.ts web dev",
    "bun ./scripts/pod.ts export events --format toon",
    "bun ./scripts/pod.ts export multiverse --format json",
    "bun ./scripts/pod.ts assets stage-import -- --output-root artifacts/staged-assets path/to/asset.glb",
    "bun ./scripts/pod.ts show pod-server --json",
    "bun ./scripts/pod.ts env pod-server --effective --json",
    "bun ./scripts/pod.ts run pod-headless -- --profile ci-smoke",
    "```",
  ];

  if (shellEntry?.interactive) {
    lines.push(
      "",
      "## Interactive Shell",
      "",
      "Use `bun ./scripts/pod.ts shell` for attached human sessions. Use `bun ./scripts/pod.ts shell --agent` for structured newline-delimited JSON sessions.",
      "",
      `- Transport: \`${shellEntry.interactive.transport}\``,
      `- Encoding: \`${shellEntry.interactive.encoding}\``,
      `- Framing: \`${shellEntry.interactive.framing}\``,
      `- Protocol version: \`${shellEntry.interactive.protocolVersion}\``,
      `- Request types: ${shellEntry.interactive.requestTypes.map((requestType) => `\`${requestType}\``).join(", ")}`,
      `- Builtins: ${shellEntry.interactive.builtins.map((builtin) => `\`${builtin}\``).join(", ")}`,
      `- Hookable events: ${shellEntry.interactive.hookableEvents.map((event) => `\`${event}\``).join(", ")}`,
      `- Events: ${shellEntry.interactive.events.map((event) => `\`${event}\``).join(", ")}`,
      "",
      "TOON is reserved for `pod export ... --format toon`, where the payload is large and LLM-facing rather than control-plane RPC.",
      "",
    );
  }

  lines.push(
    "## Audience matrix",
    "",
    "| ID | Canonical alias | Developer | User | Agent | Agent-developer | Command |",
    "| --- | --- | --- | --- | --- | --- | --- |",
  );

  for (const entry of catalog.commands) {
    const alias = entry.aliases[0] ? `\`pod ${escapeMarkdownCell(entry.aliases[0])}\`` : "-";
    lines.push(
      `| ${escapeMarkdownCell(entry.id)} | ${alias} | ${renderAudiencePresence(entry, "developer")} | ${renderAudiencePresence(entry, "user")} | ${renderAudiencePresence(entry, "agent")} | ${renderAudiencePresence(entry, "agent-developer")} | \`${escapeMarkdownCell(entry.command)}\` |`,
    );
  }

  lines.push(
    "",
    "## Command catalog",
    "",
    "| ID | Aliases | Area | Kind | Lifecycle | Passthrough | Machine-readable | CWD | Entrypoint | Outputs | Summary | References |",
    "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
  );

  for (const entry of catalog.commands) {
    const entrypoint = entry.entrypoint
      ? `[${escapeMarkdownCell(entry.entrypoint)}](${toDocsRelativeLink(repoRoot, entry.entrypoint)})`
      : "-";
    const outputs =
      entry.outputArtifacts.length === 0
        ? "-"
        : entry.outputArtifacts
            .map((artifact) => `\`${escapeMarkdownCell(artifact)}\``)
            .join(", ");
    lines.push(
      `| ${escapeMarkdownCell(entry.id)} | ${renderAliasList(entry.aliases)} | ${escapeMarkdownCell(entry.area)} | ${escapeMarkdownCell(entry.kind)} | ${escapeMarkdownCell(entry.execution.lifecycle)} | ${entry.capabilities.supportsPassthrough ? "yes" : "no"} | ${entry.machineReadable ? "yes" : "no"} | \`${escapeMarkdownCell(entry.execution.cwd)}\` | ${entrypoint} | ${outputs} | ${escapeMarkdownCell(entry.summary)} | ${renderLinkList(repoRoot, entry.docs)} |`,
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
    "- `bun ./scripts/verify_cli_surface.ts --check` verifies catalog coverage, aliases, structured execution metadata, and Markdown drift.",
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

  const aliasOwners = new Map<string, string>();
  const duplicateAliases: string[] = [];
  const invalidAliasEntries: string[] = [];
  const invalidExecutionEntries: string[] = [];
  const invalidCapabilityEntries: string[] = [];
  const invalidPassthroughEntries: string[] = [];
  const invalidInteractiveEntries: string[] = [];

  for (const entry of catalog.commands) {
    for (const alias of entry.aliases) {
      if (alias.trim().length === 0) {
        invalidAliasEntries.push(`${entry.id}:empty`);
        continue;
      }
      const owner = aliasOwners.get(alias);
      if (owner && owner !== entry.id) {
        duplicateAliases.push(alias);
      } else {
        aliasOwners.set(alias, entry.id);
      }
    }

    if (entry.command !== renderCliExecutionCommand(entry.execution)) {
      invalidExecutionEntries.push(`${entry.id}:command-mismatch`);
    }
    if (entry.cwd !== entry.execution.cwd) {
      invalidExecutionEntries.push(`${entry.id}:cwd-mismatch`);
    }
    if (entry.execution.program.trim().length === 0) {
      invalidExecutionEntries.push(`${entry.id}:missing-program`);
    }
    if (
      entry.capabilities.attachesToTerminal &&
      entry.execution.lifecycle !== "long-running"
    ) {
      invalidCapabilityEntries.push(`${entry.id}:attach-without-long-running`);
    }
    if (
      entry.capabilities.supportsEffectiveEnv &&
      entry.env.length === 0
    ) {
      invalidCapabilityEntries.push(`${entry.id}:effective-env-without-contract`);
    }
    if (
      entry.capabilities.supportsPassthrough !==
      (entry.execution.passthrough === "append-after-double-dash")
    ) {
      invalidPassthroughEntries.push(`${entry.id}:passthrough-capability-mismatch`);
    }
    if (
      entry.execution.allowedEnvOverrides.some(
        (variable) => !entry.env.includes(variable),
      )
    ) {
      invalidPassthroughEntries.push(`${entry.id}:env-override-outside-contract`);
    }
    if (
      entry.execution.passthrough === "disabled" &&
      entry.execution.allowedEnvOverrides.length > 0 &&
      entry.env.length === 0
    ) {
      invalidPassthroughEntries.push(`${entry.id}:env-override-without-env-contract`);
    }
    if (entry.interactive) {
      if (!entry.machineReadable) {
        invalidInteractiveEntries.push(`${entry.id}:interactive-without-machine-readable`);
      }
      if (!entry.capabilities.attachesToTerminal) {
        invalidInteractiveEntries.push(`${entry.id}:interactive-without-tty`);
      }
      if (entry.execution.lifecycle !== "long-running") {
        invalidInteractiveEntries.push(`${entry.id}:interactive-without-long-running`);
      }
      if (entry.interactive.transport !== "stdio-json") {
        invalidInteractiveEntries.push(`${entry.id}:interactive-invalid-transport`);
      }
      if (entry.interactive.encoding !== "json") {
        invalidInteractiveEntries.push(`${entry.id}:interactive-invalid-encoding`);
      }
      if (entry.interactive.framing !== "newline-delimited") {
        invalidInteractiveEntries.push(`${entry.id}:interactive-invalid-framing`);
      }
      if (entry.interactive.protocolVersion < 3) {
        invalidInteractiveEntries.push(`${entry.id}:interactive-invalid-protocol`);
      }
      if (entry.interactive.requestTypes.length === 0) {
        invalidInteractiveEntries.push(`${entry.id}:interactive-without-request-types`);
      }
      if (entry.interactive.builtins.length === 0) {
        invalidInteractiveEntries.push(`${entry.id}:interactive-without-builtins`);
      }
      if (entry.interactive.hookableEvents.length === 0) {
        invalidInteractiveEntries.push(`${entry.id}:interactive-without-hookable-events`);
      }
      if (entry.interactive.events.length === 0) {
        invalidInteractiveEntries.push(`${entry.id}:interactive-without-events`);
      }
    }
    if (entry.id === "pod-shell" && entry.interactive == null) {
      invalidInteractiveEntries.push(`${entry.id}:missing-interactive-metadata`);
    }
  }

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
      duplicateAliases.length === 0 &&
      invalidAliasEntries.length === 0 &&
      missingEntrypoints.length === 0 &&
      missingDocs.length === 0 &&
      unknownCoverageKeys.length === 0 &&
      uncoveredDiscoveredSurfaces.length === 0 &&
      invalidExecutionEntries.length === 0 &&
      invalidCapabilityEntries.length === 0 &&
      invalidPassthroughEntries.length === 0 &&
      invalidInteractiveEntries.length === 0 &&
      docInSync,
    duplicateIds,
    duplicateAliases: duplicateAliases.sort(),
    invalidAliasEntries: invalidAliasEntries.sort(),
    missingEntrypoints,
    missingDocs,
    unknownCoverageKeys,
    uncoveredDiscoveredSurfaces,
    invalidExecutionEntries: invalidExecutionEntries.sort(),
    invalidCapabilityEntries: invalidCapabilityEntries.sort(),
    invalidPassthroughEntries: invalidPassthroughEntries.sort(),
    invalidInteractiveEntries: invalidInteractiveEntries.sort(),
    docPath,
    docInSync,
    generatedMarkdown,
  };
}
