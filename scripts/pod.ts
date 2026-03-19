#!/usr/bin/env bun

import { resolve } from "node:path";
import { createInterface, type Completer } from "node:readline";

import {
  buildCliSurfaceCatalog,
  CLI_AREAS,
  CLI_AUDIENCES,
  CLI_KINDS,
  filterCliSurfaceEntries,
  findCliSurfaceEntry,
  findCliSurfaceEntryByAlias,
  resolveCliSurfaceCommand,
  type CliArea,
  type CliAudience,
  type CliKind,
  type CliLifecycle,
  type CliSurfaceCatalog,
  type CliSurfaceEntry,
  type CliSurfaceValidation,
  validateCliSurfaceCatalog,
} from "./cli_surface";
import {
  POD_EXPORT_FORMATS,
  POD_EXPORT_TARGETS,
  parsePodExportFormat,
  parsePodExportTarget,
  renderPodExport,
  type PodExportFormat,
  type PodExportTarget,
} from "./pod_sdk";

const POD_SCHEMA_VERSION = 2;
const SHELL_PROTOCOL_VERSION = 3;
const RUN_OUTPUT_PREVIEW_LIMIT = 2048;
const AGENT_SHELL_ENCODING = "json";
const AGENT_SHELL_TRANSPORT = "stdio-json";
const AGENT_SHELL_FRAMING = "newline-delimited";

const HUMAN_SHELL_BUILTINS = [
  "help",
  "context",
  "history",
  "clear",
  "exit",
  "quit",
] as const;
const AGENT_SHELL_BUILTINS = ["help", "context", "exit"] as const;
const AGENT_SHELL_HOOK_BUILTINS = ["help", "context"] as const;
const AGENT_SHELL_EVENTS = [
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
] as const;
const AGENT_SHELL_HOOKABLE_EVENTS = [
  "process.started",
  "process.exited",
  "session.stdin.closed",
] as const;
const ROOT_SUBCOMMANDS = [
  "help",
  "shell",
  "list",
  "show",
  "env",
  "command",
  "export",
  "run",
] as const;
const LIST_FLAGS = [
  "--audience",
  "--area",
  "--kind",
  "--machine-readable",
  "--text",
  "--json",
] as const;
const ENTITY_FLAGS = ["--json"] as const;
const ENV_FLAGS = ["--effective", "--json"] as const;
const EXPORT_FLAGS = ["--format"] as const;
const RUN_FLAGS = ["--dry-run", "--env", "--json", "--"] as const;
const SHELL_FLAGS = ["--mode", "--quiet", "--agent"] as const;
const COMMON_FLAGS = ["--help", "--json", "--dry-run", "--"] as const;

type PodListOptions = {
  audience?: CliAudience;
  area?: CliArea;
  kind?: CliKind;
  machineReadableOnly: boolean;
  text?: string;
  json: boolean;
};

type PodEntityOptions = {
  id: string;
  json: boolean;
};

type PodEnvOptions = PodEntityOptions & {
  effective: boolean;
};

type PodExportOptions = {
  target: PodExportTarget;
  format: PodExportFormat;
};

type PodRunTarget =
  | { kind: "id"; value: string }
  | { kind: "alias"; value: string };

type PodRunOptions = {
  target: PodRunTarget;
  json: boolean;
  dryRun: boolean;
  envOverrides: Record<string, string>;
  extraArgs: string[];
};

type PodShellMode = "human" | "json";
type PodShellTransport = "human" | "agent";

type PodShellOptions = {
  mode: PodShellMode;
  quiet: boolean;
  transport: PodShellTransport;
};

type PodCommand =
  | { kind: "help" }
  | { kind: "shell"; options: PodShellOptions }
  | { kind: "list"; options: PodListOptions }
  | { kind: "show"; options: PodEntityOptions }
  | { kind: "env"; options: PodEnvOptions }
  | { kind: "command"; options: PodEntityOptions }
  | { kind: "export"; options: PodExportOptions }
  | { kind: "run"; options: PodRunOptions };

type CommandPayload = {
  schemaVersion: 2;
  generatedAtUnixMs: number;
  repoRoot: string;
};

type ListPayload = CommandPayload & {
  command: "list";
  filters: {
    audience: CliAudience | null;
    area: CliArea | null;
    kind: CliKind | null;
    machineReadableOnly: boolean;
    text: string | null;
  };
  commands: CliSurfaceEntry[];
};

type EnvironmentValue = {
  name: string;
  defaultValue: string;
  description: string;
  resolvedValue: string | null;
  source: "process" | "default" | "unset" | null;
};

type EntryPayload = CommandPayload & {
  command: "show" | "env" | "command";
  entry: CliSurfaceEntry;
  resolvedCommand: string;
  environment: EnvironmentValue[];
  effectiveEnvironment: boolean;
};

type ExportPayload = CommandPayload & {
  command: "export";
  target: PodExportTarget;
  format: PodExportFormat;
  documentType: string;
  description: string;
  contentType: string;
  preferredFormat: PodExportFormat;
  preferredToonDelimiter: string | null;
  byteLength: number;
  lineCount: number;
  text: string;
};

type RunStreamSummary = {
  preview: string;
  bytes: number;
  truncated: boolean;
};

type RunPayload = CommandPayload & {
  command: "run";
  requestedTarget: PodRunTarget;
  entry: CliSurfaceEntry;
  resolvedCommand: string;
  execution: {
    program: string;
    args: string[];
    cwd: string;
    lifecycle: CliLifecycle;
  };
  status: "dry-run" | "executed" | "refused";
  dryRun: boolean;
  ok: boolean;
  exitCode: number | null;
  refusalCode: string | null;
  refusalMessage: string | null;
  durationMs: number;
  envOverrides: Record<string, string>;
  extraArgs: string[];
  stdout: RunStreamSummary | null;
  stderr: RunStreamSummary | null;
};

type BunSubprocess = ReturnType<typeof Bun.spawn>;
type RunExecutionMode = "payload" | "attach" | "stream";

type RunExecutionHooks = {
  mode?: RunExecutionMode;
  onProcessStart?: (
    subprocess: BunSubprocess,
    metadata: {
      entry: CliSurfaceEntry;
      requestedTarget: PodRunTarget;
      resolvedCommand: string;
      cwd: string;
      mode: RunExecutionMode;
    },
  ) => void;
  onStdoutChunk?: (chunk: string) => void;
  onStderrChunk?: (chunk: string) => void;
  onProcessExit?: (payload: RunPayload) => void;
};

type HumanShellState = {
  sessionId: string;
  transport: "human";
  quiet: boolean;
  history: string[];
  activeProcess: BunSubprocess | null;
};

type AgentShellState = {
  sessionId: string;
  transport: "agent";
  history: string[];
  hooks: Map<string, RegisteredAgentShellHook>;
  activeProcesses: Map<string, AgentShellManagedProcess>;
  backgroundTasks: Set<Promise<void>>;
  hookQueue: Promise<void>;
  drainWaiters: Array<() => void>;
  stdinClosed: boolean;
};

type HumanShellBuiltinResult = {
  action: "continue" | "exit";
  stderrText?: string;
  clearScreen?: boolean;
};

type AgentShellRequest =
  | {
      type: "command";
      requestId: string;
      argv: string[];
    }
  | {
      type: "builtin";
      requestId: string;
      name: string;
    }
  | {
      type: "hook";
      requestId: string;
      action: "register";
      hook: AgentShellHookSpec;
    }
  | {
      type: "hook";
      requestId: string;
      action: "unregister";
      hookId: string;
    }
  | {
      type: "hook";
      requestId: string;
      action: "list";
    };

type AgentShellEvent = {
  type: string;
  protocolVersion: typeof SHELL_PROTOCOL_VERSION;
  sessionId: string;
  timestampUnixMs: number;
  requestId?: string;
  [key: string]: unknown;
};

type AgentShellEmitter = (
  type: AgentShellEvent["type"],
  payload?: Record<string, unknown>,
  requestId?: string,
) => AgentShellEvent;

type AgentShellContext = {
  catalog: CliSurfaceCatalog;
  repoRoot: string;
  state: AgentShellState;
  emit: AgentShellEmitter;
};

type AgentShellOrigin =
  | {
      kind: "request";
    }
  | {
      kind: "hook";
      hookId: string;
      triggerEventType: string;
      triggerRequestId: string | null;
    };

type AgentShellHookEventType = (typeof AGENT_SHELL_HOOKABLE_EVENTS)[number];

type AgentShellHookAction =
  | {
      type: "command";
      argv: string[];
    }
  | {
      type: "builtin";
      name: (typeof AGENT_SHELL_HOOK_BUILTINS)[number];
    };

type AgentShellHookSpec = {
  id: string;
  on: AgentShellHookEventType[];
  match?: {
    entryId?: string;
    requestId?: string;
    ok?: boolean;
    exitCode?: number | null;
    reason?: string;
  };
  action: AgentShellHookAction;
  maxTriggers?: number | null;
};

type RegisteredAgentShellHook = AgentShellHookSpec & {
  triggerCount: number;
};

type AgentShellManagedProcess = {
  jobId: string;
  requestId: string;
  entryId: string;
  origin: AgentShellOrigin;
  subprocess: BunSubprocess;
};

type AgentShellRequestResult =
  | {
      action: "continue";
    }
  | {
      action: "exit";
      requestId: string;
      reason: "builtin.exit";
      origin: AgentShellOrigin;
    };

function fail(message: string): never {
  throw new Error(message);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

function splitAgentShellFrames(buffer: string): {
  frames: string[];
  remainder: string;
} {
  const segments = buffer.split(/\r?\n/);
  const remainder = segments.pop() ?? "";
  return {
    frames: segments.filter((segment) => segment.trim().length > 0),
    remainder,
  };
}

async function* readAgentShellFrameStream(
  input: NodeJS.ReadStream,
): AsyncGenerator<string> {
  input.setEncoding("utf8");
  let buffer = "";

  for await (const chunk of input) {
    buffer += typeof chunk === "string" ? chunk : Buffer.from(chunk).toString("utf8");
    const { frames, remainder } = splitAgentShellFrames(buffer);
    buffer = remainder;
    for (const frame of frames) {
      yield frame;
    }
  }

  const trailingFrame = buffer.trim();
  if (trailingFrame.length > 0) {
    yield trailingFrame;
  }
}

function createHookRequestId(hookId: string): string {
  return `hook:${hookId}:${crypto.randomUUID()}`;
}

function signalAgentShellDrain(state: AgentShellState) {
  const waiters = [...state.drainWaiters];
  state.drainWaiters = [];
  for (const waiter of waiters) {
    waiter();
  }
}

async function waitForAgentShellDrainSignal(state: AgentShellState): Promise<void> {
  await new Promise<void>((resolve) => {
    state.drainWaiters.push(resolve);
  });
}

function trackAgentShellBackgroundTask(
  state: AgentShellState,
  task: Promise<void>,
) {
  state.backgroundTasks.add(task);
  signalAgentShellDrain(state);
  task.finally(() => {
    state.backgroundTasks.delete(task);
    signalAgentShellDrain(state);
  });
}

async function drainAgentShellBackgroundWork(
  state: AgentShellState,
): Promise<void> {
  while (true) {
    await state.hookQueue;
    if (state.backgroundTasks.size === 0) {
      return;
    }
    await waitForAgentShellDrainSignal(state);
  }
}

function renderCliHelpText(): string {
  return `Prompt or Die CLI
canonical command front door

Usage:
  bun ./scripts/pod.ts shell [--mode <human|json>] [--quiet|--agent]
  bun ./scripts/pod.ts list [--audience <audience>] [--area <area>] [--kind <kind>] [--machine-readable] [--text <query>] [--json]
  bun ./scripts/pod.ts show <id> [--json]
  bun ./scripts/pod.ts env <id> [--effective] [--json]
  bun ./scripts/pod.ts command <id> [--json]
  bun ./scripts/pod.ts export <world|events|multiverse> [--format <json|toon>]
  bun ./scripts/pod.ts run <id> [--dry-run] [--env KEY=VALUE] [--json] [-- ...]
  bun ./scripts/pod.ts <area> <alias> [--dry-run] [--env KEY=VALUE] [--json] [-- ...]

Examples:
  bun ./scripts/pod.ts shell
  bun ./scripts/pod.ts shell --agent
  bun ./scripts/pod.ts workspace check
  bun ./scripts/pod.ts runtime server --dry-run
  bun ./scripts/pod.ts web dev
  bun ./scripts/pod.ts show pod-server --json
  bun ./scripts/pod.ts env pod-server --effective --json
  bun ./scripts/pod.ts export events --format toon
  bun ./scripts/pod.ts export multiverse --format json
  bun ./scripts/pod.ts run pod-headless -- --profile ci-smoke
  bun ./scripts/pod.ts assets stage-import -- --output-root artifacts/staged-assets path/to/asset.glb

Machine shell:
  pod shell --agent uses newline-delimited JSON request/event objects on stdio.
  Large agent-facing world data should be requested through pod export ... --format toon.

Valid audiences: ${CLI_AUDIENCES.join(", ")}
Valid areas: ${CLI_AREAS.join(", ")}
Valid kinds: ${CLI_KINDS.join(", ")}`;
}

function printHelp() {
  process.stderr.write(`${renderCliHelpText()}\n`);
}

function parseAudience(value: string): CliAudience {
  if (!CLI_AUDIENCES.includes(value as CliAudience)) {
    fail(`unknown audience: ${value}`);
  }
  return value as CliAudience;
}

function parseArea(value: string): CliArea {
  if (!CLI_AREAS.includes(value as CliArea)) {
    fail(`unknown area: ${value}`);
  }
  return value as CliArea;
}

function parseKind(value: string): CliKind {
  if (!CLI_KINDS.includes(value as CliKind)) {
    fail(`unknown kind: ${value}`);
  }
  return value as CliKind;
}

function parseShellMode(value: string): PodShellMode {
  if (value !== "human" && value !== "json") {
    fail(`unknown shell mode: ${value}`);
  }
  return value;
}

function splitPassthrough(argv: string[]): {
  beforePassthrough: string[];
  extraArgs: string[];
} {
  const passthroughIndex = argv.indexOf("--");
  if (passthroughIndex === -1) {
    return { beforePassthrough: argv, extraArgs: [] };
  }
  return {
    beforePassthrough: argv.slice(0, passthroughIndex),
    extraArgs: argv.slice(passthroughIndex + 1),
  };
}

function parseEnvOverride(argument: string): [string, string] {
  const delimiterIndex = argument.indexOf("=");
  if (delimiterIndex <= 0) {
    fail(`invalid --env override: ${argument}`);
  }
  return [
    argument.slice(0, delimiterIndex),
    argument.slice(delimiterIndex + 1),
  ];
}

function parseRunFlags(argv: string[]): Omit<PodRunOptions, "target"> {
  const { beforePassthrough, extraArgs } = splitPassthrough(argv);
  const envOverrides: Record<string, string> = {};
  let json = false;
  let dryRun = false;

  for (let index = 0; index < beforePassthrough.length; index += 1) {
    const current = beforePassthrough[index];
    switch (current) {
      case "--json":
        json = true;
        break;
      case "--dry-run":
        dryRun = true;
        break;
      case "--env": {
        const value =
          beforePassthrough[index + 1] ?? fail("missing value for --env");
        const [key, resolvedValue] = parseEnvOverride(value);
        envOverrides[key] = resolvedValue;
        index += 1;
        break;
      }
      default:
        fail(`unknown argument: ${current}`);
    }
  }

  return {
    json,
    dryRun,
    envOverrides,
    extraArgs,
  };
}

function parseListArgs(argv: string[]): PodListOptions {
  const options: PodListOptions = {
    machineReadableOnly: false,
    json: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--audience":
        options.audience = parseAudience(
          argv[index + 1] ?? fail("missing value for --audience"),
        );
        index += 1;
        break;
      case "--area":
        options.area = parseArea(
          argv[index + 1] ?? fail("missing value for --area"),
        );
        index += 1;
        break;
      case "--kind":
        options.kind = parseKind(
          argv[index + 1] ?? fail("missing value for --kind"),
        );
        index += 1;
        break;
      case "--text":
        options.text = argv[index + 1] ?? fail("missing value for --text");
        index += 1;
        break;
      case "--machine-readable":
        options.machineReadableOnly = true;
        break;
      case "--json":
        options.json = true;
        break;
      default:
        fail(`unknown argument: ${current}`);
    }
  }

  return options;
}

function parseEntityArgs(argv: string[]): PodEntityOptions {
  const id = argv[0] ?? fail("missing required command id");
  const options: PodEntityOptions = { id, json: false };
  for (let index = 1; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--json":
        options.json = true;
        break;
      default:
        fail(`unknown argument: ${current}`);
    }
  }
  return options;
}

function parseEnvArgs(argv: string[]): PodEnvOptions {
  const id = argv[0] ?? fail("missing required command id");
  const options: PodEnvOptions = { id, json: false, effective: false };
  for (let index = 1; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--json":
        options.json = true;
        break;
      case "--effective":
        options.effective = true;
        break;
      default:
        fail(`unknown argument: ${current}`);
    }
  }
  return options;
}

function parseExportArgs(argv: string[]): PodExportOptions {
  const target = parsePodExportTarget(
    argv[0] ?? fail("missing required export target"),
  );
  let format: PodExportFormat = "json";

  for (let index = 1; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--format":
        format = parsePodExportFormat(
          argv[index + 1] ?? fail("missing value for --format"),
        );
        index += 1;
        break;
      default:
        fail(`unknown argument: ${current}`);
    }
  }

  return {
    target,
    format,
  };
}

function parseRunArgs(argv: string[]): PodRunOptions {
  const id = argv[0] ?? fail("missing required command id");
  return {
    target: { kind: "id", value: id },
    ...parseRunFlags(argv.slice(1)),
  };
}

function parseAliasRunArgs(area: CliArea, argv: string[]): PodRunOptions {
  const action = argv[0] ?? fail("missing required alias action");
  return {
    target: { kind: "alias", value: `${area} ${action}` },
    ...parseRunFlags(argv.slice(1)),
  };
}

function parseShellArgs(argv: string[]): PodShellOptions {
  let explicitMode: PodShellMode | null = null;
  let quiet = false;
  let agentRequested = false;

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--mode":
        explicitMode = parseShellMode(
          argv[index + 1] ?? fail("missing value for --mode"),
        );
        index += 1;
        break;
      case "--quiet":
        quiet = true;
        break;
      case "--agent":
        agentRequested = true;
        quiet = true;
        break;
      default:
        fail(`unknown argument: ${current}`);
    }
  }

  if (agentRequested && explicitMode === "human") {
    fail("--agent cannot be combined with --mode human");
  }

  const mode = agentRequested ? "json" : explicitMode ?? "human";
  return {
    mode,
    quiet,
    transport: mode === "json" ? "agent" : "human",
  };
}

export function tokenizeShellInput(input: string): string[] {
  const tokens: string[] = [];
  let current = "";
  let quote: "'" | '"' | null = null;
  let escaped = false;

  for (const character of input) {
    if (escaped) {
      current += character;
      escaped = false;
      continue;
    }

    if (character === "\\" && quote !== "'") {
      escaped = true;
      continue;
    }

    if (quote) {
      if (character === quote) {
        quote = null;
      } else {
        current += character;
      }
      continue;
    }

    if (character === "'" || character === '"') {
      quote = character;
      continue;
    }

    if (/\s/.test(character)) {
      if (current.length > 0) {
        tokens.push(current);
        current = "";
      }
      continue;
    }

    current += character;
  }

  if (quote) {
    fail("unterminated quoted string");
  }
  if (escaped) {
    current += "\\";
  }
  if (current.length > 0) {
    tokens.push(current);
  }

  return tokens;
}

export function parsePodArgs(argv: string[]): PodCommand {
  const [subcommand, ...rest] = argv;
  switch (subcommand) {
    case undefined:
    case "help":
    case "--help":
    case "-h":
      return { kind: "help" };
    case "shell":
      return { kind: "shell", options: parseShellArgs(rest) };
    case "list":
      return { kind: "list", options: parseListArgs(rest) };
    case "show":
      return { kind: "show", options: parseEntityArgs(rest) };
    case "env":
      return { kind: "env", options: parseEnvArgs(rest) };
    case "command":
      return { kind: "command", options: parseEntityArgs(rest) };
    case "export":
      return { kind: "export", options: parseExportArgs(rest) };
    case "run":
      return { kind: "run", options: parseRunArgs(rest) };
    default:
      if (CLI_AREAS.includes(subcommand as CliArea)) {
        return {
          kind: "run",
          options: parseAliasRunArgs(subcommand as CliArea, rest),
        };
      }
      fail(`unknown subcommand: ${subcommand}`);
  }
}

function buildValidationIssues(validation: CliSurfaceValidation): string[] {
  const issues: string[] = [];
  if (validation.duplicateIds.length > 0) {
    issues.push(`duplicate ids: ${validation.duplicateIds.join(", ")}`);
  }
  if (validation.duplicateAliases.length > 0) {
    issues.push(`duplicate aliases: ${validation.duplicateAliases.join(", ")}`);
  }
  if (validation.invalidAliasEntries.length > 0) {
    issues.push(`invalid aliases: ${validation.invalidAliasEntries.join(", ")}`);
  }
  if (validation.missingEntrypoints.length > 0) {
    issues.push(`missing entrypoints: ${validation.missingEntrypoints.join(", ")}`);
  }
  if (validation.missingDocs.length > 0) {
    issues.push(`missing docs: ${validation.missingDocs.join(", ")}`);
  }
  if (validation.unknownCoverageKeys.length > 0) {
    issues.push(`unknown coverage keys: ${validation.unknownCoverageKeys.join(", ")}`);
  }
  if (validation.uncoveredDiscoveredSurfaces.length > 0) {
    issues.push(
      `uncovered discovered surfaces: ${validation.uncoveredDiscoveredSurfaces
        .map((surface) => surface.key)
        .join(", ")}`,
    );
  }
  if (validation.invalidExecutionEntries.length > 0) {
    issues.push(
      `invalid execution metadata: ${validation.invalidExecutionEntries.join(", ")}`,
    );
  }
  if (validation.invalidCapabilityEntries.length > 0) {
    issues.push(
      `invalid capability metadata: ${validation.invalidCapabilityEntries.join(", ")}`,
    );
  }
  if (validation.invalidPassthroughEntries.length > 0) {
    issues.push(
      `invalid passthrough metadata: ${validation.invalidPassthroughEntries.join(", ")}`,
    );
  }
  if (validation.invalidInteractiveEntries.length > 0) {
    issues.push(
      `invalid interactive metadata: ${validation.invalidInteractiveEntries.join(", ")}`,
    );
  }
  return issues;
}

export function buildListPayload(
  catalog: CliSurfaceCatalog,
  repoRoot: string,
  options: PodListOptions,
): ListPayload {
  return {
    schemaVersion: POD_SCHEMA_VERSION,
    generatedAtUnixMs: Date.now(),
    repoRoot,
    command: "list",
    filters: {
      audience: options.audience ?? null,
      area: options.area ?? null,
      kind: options.kind ?? null,
      machineReadableOnly: options.machineReadableOnly,
      text: options.text ?? null,
    },
    commands: filterCliSurfaceEntries(catalog, {
      audience: options.audience,
      area: options.area,
      kind: options.kind,
      machineReadableOnly: options.machineReadableOnly,
      text: options.text,
    }),
  };
}

function buildEnvironmentContract(
  catalog: CliSurfaceCatalog,
  entry: CliSurfaceEntry,
  effective: boolean,
): EnvironmentValue[] {
  return catalog.serverEnvironment
    .filter((variable) => entry.env.includes(variable.name))
    .map((variable) => {
      if (!effective) {
        return {
          name: variable.name,
          defaultValue: variable.defaultValue,
          description: variable.description,
          resolvedValue: null,
          source: null,
        };
      }

      const processValue = process.env[variable.name];
      if (processValue != null) {
        return {
          name: variable.name,
          defaultValue: variable.defaultValue,
          description: variable.description,
          resolvedValue: processValue,
          source: "process",
        };
      }
      if (variable.defaultValue === "unset") {
        return {
          name: variable.name,
          defaultValue: variable.defaultValue,
          description: variable.description,
          resolvedValue: null,
          source: "unset",
        };
      }
      return {
        name: variable.name,
        defaultValue: variable.defaultValue,
        description: variable.description,
        resolvedValue: variable.defaultValue,
        source: "default",
      };
    });
}

export function buildEntryPayload(
  catalog: CliSurfaceCatalog,
  repoRoot: string,
  mode: EntryPayload["command"],
  id: string,
  effectiveEnvironment = false,
): EntryPayload {
  const entry = findCliSurfaceEntry(catalog, id);
  if (!entry) {
    fail(`unknown command id: ${id}`);
  }

  return {
    schemaVersion: POD_SCHEMA_VERSION,
    generatedAtUnixMs: Date.now(),
    repoRoot,
    command: mode,
    entry,
    resolvedCommand: resolveCliSurfaceCommand(entry),
    environment: buildEnvironmentContract(
      catalog,
      entry,
      effectiveEnvironment,
    ),
    effectiveEnvironment,
  };
}

export function buildExportPayload(
  repoRoot: string,
  options: PodExportOptions,
): ExportPayload {
  const exportDocument = renderPodExport(options.target, options.format);
  return {
    schemaVersion: POD_SCHEMA_VERSION,
    generatedAtUnixMs: exportDocument.generatedAtUnixMs,
    repoRoot,
    command: "export",
    target: exportDocument.target,
    format: exportDocument.format,
    documentType: exportDocument.documentType,
    description: exportDocument.description,
    contentType: exportDocument.contentType,
    preferredFormat: exportDocument.preferredFormat,
    preferredToonDelimiter: exportDocument.preferredToonDelimiter,
    byteLength: exportDocument.byteLength,
    lineCount: exportDocument.lineCount,
    text: exportDocument.text,
  };
}

function pad(value: string, width: number): string {
  return value.length >= width ? value : `${value}${" ".repeat(width - value.length)}`;
}

export function renderHumanList(payload: ListPayload): string {
  const lines = [
    "Prompt or Die CLI",
    "canonical command front door",
    "",
    `commands: ${payload.commands.length}`,
    "",
  ];

  const idWidth = Math.max(2, ...payload.commands.map((entry) => entry.id.length));
  const aliasWidth = Math.max(
    5,
    ...payload.commands.map((entry) => (entry.aliases[0] ?? "-").length),
  );
  const areaWidth = Math.max(4, ...payload.commands.map((entry) => entry.area.length));
  for (const entry of payload.commands) {
    lines.push(
      `${pad(entry.id, idWidth)}  ${pad(entry.aliases[0] ?? "-", aliasWidth)}  ${pad(entry.area, areaWidth)}  ${entry.summary}`,
    );
  }

  if (payload.commands.length === 0) {
    lines.push("No commands matched the current filters.");
  }

  return `${lines.join("\n")}\n`;
}

export function renderHumanEntry(payload: EntryPayload): string {
  const lines = [
    payload.entry.name,
    payload.entry.summary,
    "",
    `id: ${payload.entry.id}`,
    `aliases: ${payload.entry.aliases.length > 0 ? payload.entry.aliases.map((alias) => `pod ${alias}`).join(", ") : "none"}`,
    `audiences: ${payload.entry.audiences.join(", ")}`,
    `area: ${payload.entry.area}`,
    `kind: ${payload.entry.kind}`,
    `cwd: ${payload.entry.execution.cwd}`,
    `lifecycle: ${payload.entry.execution.lifecycle}`,
    `passthrough: ${payload.entry.capabilities.supportsPassthrough ? "append-after-double-dash" : "disabled"}`,
    `command: ${payload.resolvedCommand}`,
    `machine-readable command output: ${payload.entry.machineReadable ? "yes" : "no"}`,
  ];

  if (payload.entry.outputArtifacts.length > 0) {
    lines.push(`outputs: ${payload.entry.outputArtifacts.join(", ")}`);
  }
  if (payload.entry.docs.length > 0) {
    lines.push(`docs: ${payload.entry.docs.join(", ")}`);
  }
  if (payload.environment.length > 0) {
    lines.push(
      "",
      payload.effectiveEnvironment ? "effective environment:" : "environment contract:",
    );
    for (const variable of payload.environment) {
      const resolved =
        payload.effectiveEnvironment
          ? variable.resolvedValue == null
            ? "<unset>"
            : variable.resolvedValue
          : variable.defaultValue;
      const suffix = payload.effectiveEnvironment
        ? ` [${variable.source ?? "n/a"}]`
        : "";
      lines.push(
        `  ${variable.name}=${resolved}${suffix}  ${variable.description}`,
      );
    }
  }
  if (payload.entry.notes.length > 0) {
    lines.push("", "notes:");
    for (const note of payload.entry.notes) {
      lines.push(`  - ${note}`);
    }
  }
  if (payload.entry.interactive) {
    lines.push(
      "",
      "interactive shell protocol:",
      `  transport: ${payload.entry.interactive.transport}`,
      `  encoding: ${payload.entry.interactive.encoding}`,
      `  framing: ${payload.entry.interactive.framing}`,
      `  protocol version: ${payload.entry.interactive.protocolVersion}`,
      `  request types: ${payload.entry.interactive.requestTypes.join(", ")}`,
      `  builtins: ${payload.entry.interactive.builtins.join(", ")}`,
      `  hookable events: ${payload.entry.interactive.hookableEvents.join(", ")}`,
      `  events: ${payload.entry.interactive.events.join(", ")}`,
    );
  }

  return `${lines.join("\n")}\n`;
}

function summarizeOutput(text: string): RunStreamSummary {
  const bytes = Buffer.byteLength(text, "utf8");
  return {
    preview: text.slice(0, RUN_OUTPUT_PREVIEW_LIMIT),
    bytes,
    truncated: text.length > RUN_OUTPUT_PREVIEW_LIMIT,
  };
}

async function pumpStreamText(
  stream: ReadableStream<Uint8Array> | null | undefined,
  onChunk?: (chunk: string) => void,
): Promise<string> {
  if (!stream) {
    return "";
  }

  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let text = "";

  while (true) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    const chunk = decoder.decode(value, { stream: true });
    if (chunk.length > 0) {
      text += chunk;
      onChunk?.(chunk);
    }
  }

  const tail = decoder.decode();
  if (tail.length > 0) {
    text += tail;
    onChunk?.(tail);
  }

  return text;
}

function resolveRunEntry(
  catalog: CliSurfaceCatalog,
  target: PodRunTarget,
): CliSurfaceEntry {
  const entry =
    target.kind === "id"
      ? findCliSurfaceEntry(catalog, target.value)
      : findCliSurfaceEntryByAlias(catalog, target.value);
  if (!entry) {
    fail(
      target.kind === "id"
        ? `unknown command id: ${target.value}`
        : `unknown command alias: ${target.value}`,
    );
  }
  return entry;
}

function buildRunPayloadBase(
  repoRoot: string,
  entry: CliSurfaceEntry,
  target: PodRunTarget,
  extraArgs: string[],
  envOverrides: Record<string, string>,
): Omit<
  RunPayload,
  "status" | "dryRun" | "ok" | "exitCode" | "refusalCode" | "refusalMessage" | "durationMs" | "stdout" | "stderr"
> {
  const argv = [entry.execution.program, ...entry.execution.args, ...extraArgs];
  return {
    schemaVersion: POD_SCHEMA_VERSION,
    generatedAtUnixMs: Date.now(),
    repoRoot,
    command: "run",
    requestedTarget: target,
    entry,
    resolvedCommand: resolveCliSurfaceCommand(entry, extraArgs),
    execution: {
      program: argv[0],
      args: argv.slice(1),
      cwd: entry.execution.cwd,
      lifecycle: entry.execution.lifecycle,
    },
    envOverrides: { ...envOverrides },
    extraArgs: [...extraArgs],
  };
}

function buildRefusedRunPayload(
  repoRoot: string,
  entry: CliSurfaceEntry,
  target: PodRunTarget,
  extraArgs: string[],
  envOverrides: Record<string, string>,
  refusalCode: string,
  refusalMessage: string,
): RunPayload {
  return {
    ...buildRunPayloadBase(repoRoot, entry, target, extraArgs, envOverrides),
    status: "refused",
    dryRun: false,
    ok: false,
    exitCode: null,
    refusalCode,
    refusalMessage,
    durationMs: 0,
    stdout: null,
    stderr: null,
  };
}

export function classifyShellRunMode(
  entry: CliSurfaceEntry,
  transport: PodShellTransport,
  options: PodRunOptions,
): RunExecutionMode {
  if (options.dryRun) {
    return "payload";
  }
  if (transport === "agent") {
    return entry.execution.lifecycle === "long-running" ? "stream" : "payload";
  }
  return options.json ? "payload" : "attach";
}

export async function executeRunCommand(
  catalog: CliSurfaceCatalog,
  repoRoot: string,
  options: PodRunOptions,
  hooks: RunExecutionHooks = {},
): Promise<RunPayload> {
  const entry = resolveRunEntry(catalog, options.target);
  const mode = hooks.mode ?? (options.json ? "payload" : "attach");
  const basePayload = buildRunPayloadBase(
    repoRoot,
    entry,
    options.target,
    options.extraArgs,
    options.envOverrides,
  );

  const disallowedEnvOverrides = Object.keys(options.envOverrides).filter(
    (key) => !entry.execution.allowedEnvOverrides.includes(key),
  );
  if (disallowedEnvOverrides.length > 0) {
    return buildRefusedRunPayload(
      repoRoot,
      entry,
      options.target,
      options.extraArgs,
      options.envOverrides,
      "ENV_OVERRIDE_NOT_ALLOWED",
      `env overrides not allowed for ${entry.id}: ${disallowedEnvOverrides.join(", ")}`,
    );
  }

  if (
    options.extraArgs.length > 0 &&
    entry.execution.passthrough !== "append-after-double-dash"
  ) {
    return buildRefusedRunPayload(
      repoRoot,
      entry,
      options.target,
      options.extraArgs,
      options.envOverrides,
      "PASSTHROUGH_NOT_ALLOWED",
      `passthrough args are not allowed for ${entry.id}`,
    );
  }

  if (options.dryRun) {
    return {
      ...basePayload,
      status: "dry-run",
      dryRun: true,
      ok: true,
      exitCode: 0,
      refusalCode: null,
      refusalMessage: null,
      durationMs: 0,
      stdout: null,
      stderr: null,
    };
  }

  if (mode === "payload" && entry.execution.lifecycle === "long-running") {
    return buildRefusedRunPayload(
      repoRoot,
      entry,
      options.target,
      options.extraArgs,
      options.envOverrides,
      "LONG_RUNNING_REQUIRES_ATTACH",
      `${entry.id} is long-running and must be attached without --json, streamed via pod shell --agent, or invoked with --dry-run`,
    );
  }

  const cwd =
    entry.execution.cwd === "."
      ? repoRoot
      : resolve(repoRoot, entry.execution.cwd);
  const argv = [entry.execution.program, ...entry.execution.args, ...options.extraArgs];
  const started = performance.now();

  if (mode === "payload") {
    const subprocess = Bun.spawn({
      cmd: argv,
      cwd,
      env: {
        ...process.env,
        ...options.envOverrides,
      },
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    });

    const [exitCode, stdoutText, stderrText] = await Promise.all([
      subprocess.exited,
      pumpStreamText(subprocess.stdout),
      pumpStreamText(subprocess.stderr),
    ]);

    const payload: RunPayload = {
      ...basePayload,
      status: "executed",
      dryRun: false,
      ok: exitCode === 0,
      exitCode,
      refusalCode: null,
      refusalMessage: null,
      durationMs: performance.now() - started,
      stdout: summarizeOutput(stdoutText),
      stderr: summarizeOutput(stderrText),
    };
    hooks.onProcessExit?.(payload);
    return payload;
  }

  if (mode === "stream") {
    const subprocess = Bun.spawn({
      cmd: argv,
      cwd,
      env: {
        ...process.env,
        ...options.envOverrides,
      },
      stdin: "ignore",
      stdout: "pipe",
      stderr: "pipe",
    });

    hooks.onProcessStart?.(subprocess, {
      entry,
      requestedTarget: options.target,
      resolvedCommand: basePayload.resolvedCommand,
      cwd: basePayload.execution.cwd,
      mode,
    });

    const [exitCode] = await Promise.all([
      subprocess.exited,
      pumpStreamText(subprocess.stdout, hooks.onStdoutChunk),
      pumpStreamText(subprocess.stderr, hooks.onStderrChunk),
    ]);

    const payload: RunPayload = {
      ...basePayload,
      status: "executed",
      dryRun: false,
      ok: exitCode === 0,
      exitCode,
      refusalCode: null,
      refusalMessage: null,
      durationMs: performance.now() - started,
      stdout: null,
      stderr: null,
    };
    hooks.onProcessExit?.(payload);
    return payload;
  }

  const subprocess = Bun.spawn({
    cmd: argv,
    cwd,
    env: {
      ...process.env,
      ...options.envOverrides,
    },
    stdin: "inherit",
    stdout: "inherit",
    stderr: "inherit",
  });

  hooks.onProcessStart?.(subprocess, {
    entry,
    requestedTarget: options.target,
    resolvedCommand: basePayload.resolvedCommand,
    cwd: basePayload.execution.cwd,
    mode,
  });

  const exitCode = await subprocess.exited;
  const payload: RunPayload = {
    ...basePayload,
    status: "executed",
    dryRun: false,
    ok: exitCode === 0,
    exitCode,
    refusalCode: null,
    refusalMessage: null,
    durationMs: performance.now() - started,
    stdout: null,
    stderr: null,
  };
  hooks.onProcessExit?.(payload);
  return payload;
}

function renderHumanShellHelp(): string {
  return [
    "Interactive POD shell",
    "",
    "Builtins:",
    "  help          Show shell help",
    "  context       Show shell session details",
    "  history       Show session-local command history",
    "  clear         Clear the terminal",
    "  exit, quit    Leave the shell",
    "",
    "Everything else is parsed like a normal `pod` command:",
    "  list --machine-readable",
    "  show pod-server",
    "  env pod-server --effective",
    "  export world --format toon",
    "  runtime server --dry-run",
    "  assets stage-import -- --output-root artifacts/staged-assets path/to/asset.glb",
    "",
    "Tab completion covers builtins, subcommands, areas, aliases, ids, and common flags.",
    "The machine shell at `pod shell --agent` uses newline-delimited JSON objects.",
    "Use `pod export ... --format toon` when the payload is large, tabular, and LLM-facing.",
  ].join("\n");
}

function buildHumanShellContext(
  state: HumanShellState,
  catalog: CliSurfaceCatalog,
): string {
  return [
    `session: ${state.sessionId}`,
    `transport: ${state.transport}`,
    `protocol: tty-line`,
    `history entries: ${state.history.length}`,
    `catalog commands: ${catalog.commands.length}`,
  ].join("\n");
}

function buildHumanShellHistory(state: HumanShellState): string {
  if (state.history.length === 0) {
    return "No commands in session history.";
  }
  return state.history
    .map((entry, index) => `${String(index + 1).padStart(2, " ")}  ${entry}`)
    .join("\n");
}

export function executeHumanShellBuiltin(
  input: string,
  state: HumanShellState,
  catalog: CliSurfaceCatalog,
): HumanShellBuiltinResult | null {
  switch (input) {
    case "help":
      return {
        action: "continue",
        stderrText: renderHumanShellHelp(),
      };
    case "context":
      return {
        action: "continue",
        stderrText: buildHumanShellContext(state, catalog),
      };
    case "history":
      return {
        action: "continue",
        stderrText: buildHumanShellHistory(state),
      };
    case "clear":
      return {
        action: "continue",
        clearScreen: true,
      };
    case "exit":
    case "quit":
      return {
        action: "exit",
        stderrText: state.quiet ? undefined : "Leaving POD shell.",
      };
    default:
      return null;
  }
}

function tokenizeCompletionInput(line: string): {
  tokens: string[];
  currentPrefix: string;
} {
  const endsWithWhitespace = /\s$/.test(line);
  const source = line.trimEnd();

  if (source.length === 0) {
    return { tokens: [], currentPrefix: "" };
  }

  let tokens: string[];
  try {
    tokens = tokenizeShellInput(source);
  } catch {
    tokens = source.split(/\s+/).filter(Boolean);
  }

  if (endsWithWhitespace) {
    return { tokens, currentPrefix: "" };
  }

  return {
    tokens: tokens.slice(0, -1),
    currentPrefix: tokens.at(-1) ?? "",
  };
}

function getAliasActionsForArea(
  catalog: CliSurfaceCatalog,
  area: CliArea,
): string[] {
  return catalog.commands
    .flatMap((entry) =>
      entry.aliases
        .filter((alias) => alias.startsWith(`${area} `))
        .map((alias) => alias.slice(area.length + 1)),
    )
    .filter((value, index, values) => values.indexOf(value) === index)
    .sort();
}

function getFlagValueSuggestions(flag: string): string[] {
  switch (flag) {
    case "--audience":
      return [...CLI_AUDIENCES];
    case "--area":
      return [...CLI_AREAS];
    case "--kind":
      return [...CLI_KINDS];
    case "--format":
      return [...POD_EXPORT_FORMATS];
    default:
      return [];
  }
}

export function getHumanShellCompletions(
  line: string,
  catalog: CliSurfaceCatalog,
): string[] {
  const { tokens, currentPrefix } = tokenizeCompletionInput(line);
  const suggestions = new Set<string>();
  const addSuggestions = (values: Iterable<string>) => {
    for (const value of values) {
      if (value.startsWith(currentPrefix)) {
        suggestions.add(value);
      }
    }
  };

  if (tokens.length > 0) {
    const expectedValues = getFlagValueSuggestions(tokens.at(-1) ?? "");
    if (expectedValues.length > 0) {
      addSuggestions(expectedValues);
      return Array.from(suggestions).sort();
    }
  }

  if (tokens.length === 0) {
    addSuggestions(HUMAN_SHELL_BUILTINS);
    addSuggestions(ROOT_SUBCOMMANDS);
    addSuggestions(CLI_AREAS);
    addSuggestions(COMMON_FLAGS);
    return Array.from(suggestions).sort();
  }

  const firstToken = tokens[0];
  switch (firstToken) {
    case "list":
      addSuggestions(LIST_FLAGS);
      break;
    case "show":
    case "env":
    case "command":
    case "run":
      addSuggestions(catalog.commands.map((entry) => entry.id));
      addSuggestions(firstToken === "env" ? ENV_FLAGS : firstToken === "run" ? RUN_FLAGS : ENTITY_FLAGS);
      break;
    case "export":
      addSuggestions(POD_EXPORT_TARGETS);
      addSuggestions(EXPORT_FLAGS);
      break;
    case "shell":
      addSuggestions(SHELL_FLAGS);
      break;
    default:
      if (CLI_AREAS.includes(firstToken as CliArea)) {
        addSuggestions(getAliasActionsForArea(catalog, firstToken as CliArea));
        addSuggestions(RUN_FLAGS);
      } else {
        addSuggestions(HUMAN_SHELL_BUILTINS);
        addSuggestions(ROOT_SUBCOMMANDS);
        addSuggestions(CLI_AREAS);
      }
      break;
  }

  return Array.from(suggestions).sort();
}

function createHumanShellCompleter(catalog: CliSurfaceCatalog): Completer {
  return (line) => {
    const { currentPrefix } = tokenizeCompletionInput(line);
    const matches = getHumanShellCompletions(line, catalog);
    return [matches.length > 0 ? matches : getHumanShellCompletions("", catalog), currentPrefix];
  };
}

function createShellSessionId(): string {
  return crypto.randomUUID();
}

function createAgentEventEmitter(
  sessionId: string,
  writeEvent: (event: AgentShellEvent) => void,
): AgentShellEmitter {
  return (type, payload = {}, requestId) => {
    const event: AgentShellEvent = {
      type,
      protocolVersion: SHELL_PROTOCOL_VERSION,
      sessionId,
      timestampUnixMs: Date.now(),
      ...(requestId ? { requestId } : {}),
      ...payload,
    };
    writeEvent(event);
    return event;
  };
}

function buildAgentShellHelpPayload(catalog: CliSurfaceCatalog) {
  return {
    transport: AGENT_SHELL_TRANSPORT,
    encoding: AGENT_SHELL_ENCODING,
    framing: AGENT_SHELL_FRAMING,
    protocolVersion: SHELL_PROTOCOL_VERSION,
    builtins: [...AGENT_SHELL_BUILTINS],
    requestTypes: ["builtin", "command", "hook"],
    hookableEvents: [...AGENT_SHELL_HOOKABLE_EVENTS],
    commands: ROOT_SUBCOMMANDS.filter(
      (command) => command !== "shell" && command !== "help",
    ),
    areas: [...CLI_AREAS],
    aliases: catalog.commands
      .flatMap((entry) => entry.aliases)
      .filter((alias, index, aliases) => aliases.indexOf(alias) === index)
      .sort(),
    notes: [
      "Send builtin requests for help, context, and exit.",
      "Send command requests with argv arrays for standard pod commands.",
      "Send export commands with argv arrays such as ['export', 'events', '--format', 'toon'] for agent-facing world data.",
      "Send hook requests to register autonomous follow-up actions on process lifecycle events.",
      "Long-running commands stream process events and continue running after stdin closes until the managed job set drains.",
      "Machine transport uses one JSON object per line on stdin/stdout.",
      "TOON is reserved for large exported world/event/multiverse payloads, not shell control messages.",
    ],
  };
}

function buildAgentShellContextPayload(
  state: AgentShellState,
  catalog: CliSurfaceCatalog,
) {
  return {
    transport: AGENT_SHELL_TRANSPORT,
    encoding: AGENT_SHELL_ENCODING,
    framing: AGENT_SHELL_FRAMING,
    protocolVersion: SHELL_PROTOCOL_VERSION,
    sessionId: state.sessionId,
    historyLength: state.history.length,
    catalogCommands: catalog.commands.length,
    activeProcessCount: state.activeProcesses.size,
    hookCount: state.hooks.size,
    stdinClosed: state.stdinClosed,
  };
}

function parseHookAction(value: unknown): AgentShellHookAction {
  if (!isRecord(value)) {
    fail("hook actions must be objects");
  }

  switch (value.type) {
    case "builtin": {
      if (
        typeof value.name !== "string" ||
        !AGENT_SHELL_HOOK_BUILTINS.includes(
          value.name as (typeof AGENT_SHELL_HOOK_BUILTINS)[number],
        )
      ) {
        fail("hook builtin actions require a supported non-exit builtin name");
      }
      return {
        type: "builtin",
        name: value.name as (typeof AGENT_SHELL_HOOK_BUILTINS)[number],
      };
    }
    case "command": {
      if (
        !Array.isArray(value.argv) ||
        value.argv.length === 0 ||
        value.argv.some((entry) => typeof entry !== "string")
      ) {
        fail("hook command actions require argv as a non-empty string array");
      }
      return {
        type: "command",
        argv: [...(value.argv as string[])],
      };
    }
    default:
      fail(`unsupported hook action type: ${String(value.type ?? "<missing>")}`);
  }
}

function parseHookSpec(value: unknown): AgentShellHookSpec {
  if (!isRecord(value)) {
    fail("hook registrations require a hook object");
  }
  if (typeof value.id !== "string" || value.id.trim().length === 0) {
    fail("hooks require a non-empty id");
  }
  if (
    !Array.isArray(value.on) ||
    value.on.length === 0 ||
    value.on.some(
      (eventType) =>
        typeof eventType !== "string" ||
        !AGENT_SHELL_HOOKABLE_EVENTS.includes(
          eventType as AgentShellHookEventType,
        ),
    )
  ) {
    fail("hooks require one or more supported hookable event types");
  }

  const match = value.match;
  if (match != null && !isRecord(match)) {
    fail("hook match filters must be objects");
  }

  const maxTriggers = value.maxTriggers;
  if (
    maxTriggers != null &&
    (typeof maxTriggers !== "number" ||
      !Number.isInteger(maxTriggers) ||
      maxTriggers < 1)
  ) {
    fail("hook maxTriggers must be a positive integer when provided");
  }

  return {
    id: value.id,
    on: [...(value.on as AgentShellHookEventType[])],
    match:
      match == null
        ? undefined
        : {
            entryId:
              typeof match.entryId === "string" ? match.entryId : undefined,
            requestId:
              typeof match.requestId === "string" ? match.requestId : undefined,
            ok: typeof match.ok === "boolean" ? match.ok : undefined,
            exitCode:
              typeof match.exitCode === "number" ||
              match.exitCode === null
                ? (match.exitCode as number | null)
                : undefined,
            reason:
              typeof match.reason === "string" ? match.reason : undefined,
          },
    action: parseHookAction(value.action),
    maxTriggers:
      typeof maxTriggers === "number" ? maxTriggers : null,
  };
}

function parseAgentShellRequestPayload(parsed: unknown): AgentShellRequest {
  if (!isRecord(parsed)) {
    fail("agent shell request must be an object");
  }

  const request = parsed;
  const requestId = request.requestId;
  if (typeof requestId !== "string" || requestId.trim().length === 0) {
    fail("agent shell request must include a non-empty requestId");
  }

  switch (request.type) {
    case "builtin": {
      const name = request.name;
      if (typeof name !== "string" || name.trim().length === 0) {
        fail("builtin requests require a non-empty name");
      }
      return {
        type: "builtin",
        requestId,
        name,
      };
    }
    case "hook": {
      const action = request.action;
      if (action === "register") {
        return {
          type: "hook",
          requestId,
          action,
          hook: parseHookSpec(request.hook),
        };
      }
      if (action === "unregister") {
        if (
          typeof request.hookId !== "string" ||
          request.hookId.trim().length === 0
        ) {
          fail("hook unregister requests require a non-empty hookId");
        }
        return {
          type: "hook",
          requestId,
          action,
          hookId: request.hookId,
        };
      }
      if (action === "list") {
        return {
          type: "hook",
          requestId,
          action,
        };
      }
      fail(`unsupported hook action: ${String(action ?? "<missing>")}`);
    }
    case "command": {
      const argv = request.argv;
      if (!Array.isArray(argv) || argv.some((value) => typeof value !== "string")) {
        fail("command requests require argv as a string array");
      }
      if (argv.length === 0) {
        fail("command requests require at least one argv token");
      }
      return {
        type: "command",
        requestId,
        argv: argv as string[],
      };
    }
    default:
      fail(`unsupported request type: ${String(request.type ?? "<missing>")}`);
  }
}

function parseAgentShellRequestDocument(document: string): AgentShellRequest {
  return parseAgentShellRequestPayload(JSON.parse(document));
}

function listRegisteredHooks(state: AgentShellState) {
  return Array.from(state.hooks.values())
    .map((hook) => ({
      id: hook.id,
      on: [...hook.on],
      match: hook.match ?? null,
      action:
        hook.action.type === "command"
          ? {
              type: hook.action.type,
              argv: [...hook.action.argv],
            }
          : {
              type: hook.action.type,
              name: hook.action.name,
            },
      triggerCount: hook.triggerCount,
      maxTriggers: hook.maxTriggers ?? null,
    }))
    .sort((left, right) => left.id.localeCompare(right.id));
}

function hookMatchesEvent(
  hook: RegisteredAgentShellHook,
  event: AgentShellEvent,
): boolean {
  if (!hook.on.includes(event.type as AgentShellHookEventType)) {
    return false;
  }

  const match = hook.match;
  if (!match) {
    return true;
  }

  if (match.entryId != null && event.entryId !== match.entryId) {
    return false;
  }
  if (match.requestId != null && event.requestId !== match.requestId) {
    return false;
  }
  if (match.ok != null && event.ok !== match.ok) {
    return false;
  }
  if (match.exitCode !== undefined && event.exitCode !== match.exitCode) {
    return false;
  }
  if (match.reason != null && event.reason !== match.reason) {
    return false;
  }

  return true;
}

async function handleAgentShellRequest(
  request: AgentShellRequest,
  context: AgentShellContext,
  origin: AgentShellOrigin = { kind: "request" },
): Promise<AgentShellRequestResult> {
  const { catalog, repoRoot, state, emit } = context;

  if (request.type === "builtin") {
    switch (request.name) {
      case "help":
        emit(
          "command.accepted",
          {
            builtin: request.name,
            origin,
          },
          request.requestId,
        );
        emit(
          "command.result",
          {
            builtin: request.name,
            origin,
            payload: buildAgentShellHelpPayload(catalog),
          },
          request.requestId,
        );
        return { action: "continue" };
      case "context":
        emit(
          "command.accepted",
          {
            builtin: request.name,
            origin,
          },
          request.requestId,
        );
        emit(
          "command.result",
          {
            builtin: request.name,
            origin,
            payload: buildAgentShellContextPayload(state, catalog),
          },
          request.requestId,
        );
        return { action: "continue" };
      case "exit":
        emit(
          "command.accepted",
          {
            builtin: request.name,
            origin,
          },
          request.requestId,
        );
        return {
          action: "exit",
          requestId: request.requestId,
          reason: "builtin.exit",
          origin,
        };
      default:
        emit(
          "error",
          {
            code: "UNSUPPORTED_BUILTIN",
            message: `unsupported builtin: ${request.name}`,
            origin,
          },
          request.requestId,
        );
        return { action: "continue" };
    }
  }

  if (request.type === "hook") {
    switch (request.action) {
      case "register": {
        state.hooks.set(request.hook.id, {
          ...request.hook,
          on: [...request.hook.on],
          action:
            request.hook.action.type === "command"
              ? {
                  type: "command",
                  argv: [...request.hook.action.argv],
                }
              : {
                  type: "builtin",
                  name: request.hook.action.name,
                },
          triggerCount: 0,
        });
        emit(
          "command.accepted",
          {
            requestType: request.type,
            action: request.action,
            hookId: request.hook.id,
            origin,
          },
          request.requestId,
        );
        emit(
          "command.result",
          {
            requestType: request.type,
            action: request.action,
            origin,
            payload: {
              hook: listRegisteredHooks(state).find(
                (hook) => hook.id === request.hook.id,
              ) ?? null,
              hookCount: state.hooks.size,
            },
          },
          request.requestId,
        );
        signalAgentShellDrain(state);
        return { action: "continue" };
      }
      case "unregister": {
        const removed = state.hooks.delete(request.hookId);
        emit(
          "command.accepted",
          {
            requestType: request.type,
            action: request.action,
            hookId: request.hookId,
            origin,
          },
          request.requestId,
        );
        emit(
          "command.result",
          {
            requestType: request.type,
            action: request.action,
            origin,
            payload: {
              removed,
              hookId: request.hookId,
              hookCount: state.hooks.size,
            },
          },
          request.requestId,
        );
        signalAgentShellDrain(state);
        return { action: "continue" };
      }
      case "list":
        emit(
          "command.accepted",
          {
            requestType: request.type,
            action: request.action,
            origin,
          },
          request.requestId,
        );
        emit(
          "command.result",
          {
            requestType: request.type,
            action: request.action,
            origin,
            payload: {
              hooks: listRegisteredHooks(state),
            },
          },
          request.requestId,
        );
        return { action: "continue" };
    }
  }

  let command: PodCommand;
  try {
    command = parsePodArgs(request.argv);
  } catch (error) {
    emit(
      "error",
      {
        code: "INVALID_COMMAND",
        message: error instanceof Error ? error.message : String(error),
        origin,
      },
      request.requestId,
    );
    return { action: "continue" };
  }

  if (command.kind === "shell") {
    emit(
      "error",
      {
        code: "NESTED_SHELL_NOT_SUPPORTED",
        message: "shell cannot be invoked from inside shell",
        origin,
      },
      request.requestId,
    );
    return { action: "continue" };
  }

  if (command.kind === "help") {
    emit(
      "error",
      {
        code: "HELP_IS_BUILTIN",
        message: "help is a builtin inside pod shell --agent",
        origin,
      },
      request.requestId,
    );
    return { action: "continue" };
  }

  emit(
    "command.accepted",
    {
      argv: [...request.argv],
      command: command.kind,
      origin,
    },
    request.requestId,
  );

  switch (command.kind) {
    case "list":
      emit(
        "command.result",
        {
          command: command.kind,
          origin,
          payload: buildListPayload(catalog, repoRoot, command.options),
        },
        request.requestId,
      );
      return { action: "continue" };
    case "show":
      emit(
        "command.result",
        {
          command: command.kind,
          origin,
          payload: buildEntryPayload(
            catalog,
            repoRoot,
            "show",
            command.options.id,
          ),
        },
        request.requestId,
      );
      return { action: "continue" };
    case "env":
      emit(
        "command.result",
        {
          command: command.kind,
          origin,
          payload: buildEntryPayload(
            catalog,
            repoRoot,
            "env",
            command.options.id,
            command.options.effective,
          ),
        },
        request.requestId,
      );
      return { action: "continue" };
    case "command":
      emit(
        "command.result",
        {
          command: command.kind,
          origin,
          payload: buildEntryPayload(
            catalog,
            repoRoot,
            "command",
            command.options.id,
          ),
        },
        request.requestId,
      );
      return { action: "continue" };
    case "export":
      emit(
        "command.result",
        {
          command: command.kind,
          origin,
          payload: buildExportPayload(repoRoot, command.options),
        },
        request.requestId,
      );
      return { action: "continue" };
    case "run": {
      const entry = resolveRunEntry(catalog, command.options.target);
      const mode = classifyShellRunMode(entry, "agent", command.options);

      if (mode !== "stream") {
        const payload = await executeRunCommand(catalog, repoRoot, command.options, {
          mode,
        });
        emit(
          "command.result",
          {
            command: command.kind,
            origin,
            payload,
          },
          request.requestId,
        );
        return { action: "continue" };
      }

      const jobId = crypto.randomUUID();
      const task = (async () => {
        try {
          const payload = await executeRunCommand(catalog, repoRoot, command.options, {
            mode,
            onProcessStart: (subprocess, metadata) => {
              state.activeProcesses.set(jobId, {
                jobId,
                requestId: request.requestId,
                entryId: metadata.entry.id,
                origin,
                subprocess,
              });
              emit(
                "process.started",
                {
                  jobId,
                  entryId: metadata.entry.id,
                  resolvedCommand: metadata.resolvedCommand,
                  lifecycle: metadata.entry.execution.lifecycle,
                  pid:
                    typeof subprocess.pid === "number" ? subprocess.pid : null,
                  origin,
                },
                request.requestId,
              );
              signalAgentShellDrain(state);
            },
            onStdoutChunk: (chunk) => {
              emit(
                "process.stdout",
                {
                  jobId,
                  entryId: entry.id,
                  chunk,
                  origin,
                },
                request.requestId,
              );
            },
            onStderrChunk: (chunk) => {
              emit(
                "process.stderr",
                {
                  jobId,
                  entryId: entry.id,
                  chunk,
                  origin,
                },
                request.requestId,
              );
            },
            onProcessExit: (result) => {
              state.activeProcesses.delete(jobId);
              emit(
                "process.exited",
                {
                  jobId,
                  entryId: entry.id,
                  resolvedCommand: result.resolvedCommand,
                  exitCode: result.exitCode,
                  ok: result.ok,
                  durationMs: result.durationMs,
                  origin,
                },
                request.requestId,
              );
              signalAgentShellDrain(state);
            },
          });

          if (payload.status !== "executed") {
            emit(
              "command.result",
              {
                command: command.kind,
                origin,
                payload,
              },
              request.requestId,
            );
          }
        } catch (error) {
          emit(
            "error",
            {
              code: "RUN_FAILED",
              message: error instanceof Error ? error.message : String(error),
              jobId,
              entryId: entry.id,
              origin,
            },
            request.requestId,
          );
        } finally {
          await state.hookQueue;
        }
      })();

      trackAgentShellBackgroundTask(state, task);
      return { action: "continue" };
    }
  }
}

async function executeHookAction(
  hook: RegisteredAgentShellHook,
  triggerEvent: AgentShellEvent,
  context: AgentShellContext,
) {
  const requestId = createHookRequestId(hook.id);
  context.emit(
    "hook.triggered",
    {
      hookId: hook.id,
      triggerEventType: triggerEvent.type,
      triggerRequestId: triggerEvent.requestId ?? null,
      action:
        hook.action.type === "command"
          ? {
              type: hook.action.type,
              argv: [...hook.action.argv],
            }
          : {
              type: hook.action.type,
              name: hook.action.name,
            },
    },
    requestId,
  );

  const result = await handleAgentShellRequest(
    hook.action.type === "command"
      ? {
          type: "command",
          requestId,
          argv: [...hook.action.argv],
        }
      : {
          type: "builtin",
          requestId,
          name: hook.action.name,
        },
    context,
    {
      kind: "hook",
      hookId: hook.id,
      triggerEventType: triggerEvent.type,
      triggerRequestId: triggerEvent.requestId ?? null,
    },
  );

  if (result.action === "exit") {
    signalAgentShellDrain(context.state);
  }
}

function queueHookDispatchForEvent(
  event: AgentShellEvent,
  context: AgentShellContext,
) {
  if (!AGENT_SHELL_HOOKABLE_EVENTS.includes(event.type as AgentShellHookEventType)) {
    return;
  }

  const matchingHooks = Array.from(context.state.hooks.values()).filter((hook) =>
    hookMatchesEvent(hook, event),
  );
  if (matchingHooks.length === 0) {
    return;
  }

  context.state.hookQueue = context.state.hookQueue
    .then(async () => {
      for (const hook of matchingHooks) {
        const latest = context.state.hooks.get(hook.id);
        if (!latest || !hookMatchesEvent(latest, event)) {
          continue;
        }
        if (
          latest.maxTriggers != null &&
          latest.triggerCount >= latest.maxTriggers
        ) {
          continue;
        }
        latest.triggerCount += 1;
        await executeHookAction(latest, event, context);
      }
    })
    .finally(() => {
      signalAgentShellDrain(context.state);
    });
}

export function encodeAgentShellRequestDocument(
  payload: Record<string, unknown>,
): string {
  return JSON.stringify(payload);
}

export function encodeAgentShellEventDocument(
  payload: Record<string, unknown>,
): string {
  return JSON.stringify(payload);
}

export function decodeAgentShellEventDocuments(
  transportText: string,
): AgentShellEvent[] {
  const { frames, remainder } = splitAgentShellFrames(transportText);
  const finalFrames = [...frames];
  const trailing = remainder.trim();
  if (trailing.length > 0) {
    finalFrames.push(trailing);
  }

  return finalFrames.map((frame) => JSON.parse(frame) as AgentShellEvent);
}

export async function runAgentShellSessionForTest(options: {
  documents: string[];
  catalog: CliSurfaceCatalog;
  repoRoot: string;
  sessionId?: string;
}): Promise<AgentShellEvent[]> {
  const events: AgentShellEvent[] = [];
  const state: AgentShellState = {
    sessionId: options.sessionId ?? "test-shell-session",
    transport: "agent",
    history: [],
    hooks: new Map(),
    activeProcesses: new Map(),
    backgroundTasks: new Set(),
    hookQueue: Promise.resolve(),
    drainWaiters: [],
    stdinClosed: false,
  };
  const baseEmit = createAgentEventEmitter(state.sessionId, (event) => {
    events.push(event);
  });
  const context: AgentShellContext = {
    catalog: options.catalog,
    repoRoot: options.repoRoot,
    state,
    emit: (type, payload, requestId) => {
      const event = baseEmit(type, payload, requestId);
      queueHookDispatchForEvent(event, context);
      return event;
    },
  };

  context.emit("session.started", {
    transport: AGENT_SHELL_TRANSPORT,
    encoding: AGENT_SHELL_ENCODING,
    framing: AGENT_SHELL_FRAMING,
    builtins: [...AGENT_SHELL_BUILTINS],
    events: [...AGENT_SHELL_EVENTS],
    hookableEvents: [...AGENT_SHELL_HOOKABLE_EVENTS],
  });

  let exitSignal: Extract<AgentShellRequestResult, { action: "exit" }> | null = null;

  for (const document of options.documents) {
    const trimmedDocument = document.trim();
    if (trimmedDocument.length === 0) {
      continue;
    }

    state.history.push(trimmedDocument);

    let request: AgentShellRequest;
    try {
      request = parseAgentShellRequestDocument(trimmedDocument);
    } catch (error) {
      context.emit("error", {
        code: "INVALID_REQUEST",
        message: error instanceof Error ? error.message : String(error),
      });
      continue;
    }

    const result = await handleAgentShellRequest(request, context);
    if (result.action === "exit") {
      exitSignal = result;
      break;
    }
  }

  if (exitSignal) {
    await drainAgentShellBackgroundWork(state);
    context.emit(
      "session.ended",
      {
        reason: exitSignal.reason,
        origin: exitSignal.origin,
      },
      exitSignal.requestId,
    );
    return events;
  }

  state.stdinClosed = true;
  context.emit("session.stdin.closed", {
    reason: "stdin.closed",
    activeProcessCount: state.activeProcesses.size,
    hookCount: state.hooks.size,
  });

  await drainAgentShellBackgroundWork(state);

  context.emit("session.ended", {
    reason: "stdin.closed",
  });
  return events;
}

async function runAgentShell(
  catalog: CliSurfaceCatalog,
  repoRoot: string,
): Promise<number> {
  const state: AgentShellState = {
    sessionId: createShellSessionId(),
    transport: "agent",
    history: [],
    hooks: new Map(),
    activeProcesses: new Map(),
    backgroundTasks: new Set(),
    hookQueue: Promise.resolve(),
    drainWaiters: [],
    stdinClosed: false,
  };
  const baseEmit = createAgentEventEmitter(state.sessionId, (event) => {
    process.stdout.write(
      `${encodeAgentShellEventDocument(event)}\n`,
    );
  });
  const context: AgentShellContext = {
    catalog,
    repoRoot,
    state,
    emit: (type, payload, requestId) => {
      const event = baseEmit(type, payload, requestId);
      queueHookDispatchForEvent(event, context);
      return event;
    },
  };

  context.emit("session.started", {
    transport: AGENT_SHELL_TRANSPORT,
    encoding: AGENT_SHELL_ENCODING,
    framing: AGENT_SHELL_FRAMING,
    builtins: [...AGENT_SHELL_BUILTINS],
    events: [...AGENT_SHELL_EVENTS],
    hookableEvents: [...AGENT_SHELL_HOOKABLE_EVENTS],
  });

  let ended = false;
  let exitSignal: Extract<AgentShellRequestResult, { action: "exit" }> | null =
    null;

  for await (const document of readAgentShellFrameStream(process.stdin)) {
    const trimmedDocument = document.trim();
    if (trimmedDocument.length === 0) {
      continue;
    }

    state.history.push(trimmedDocument);

    let request: AgentShellRequest;
    try {
      request = parseAgentShellRequestDocument(trimmedDocument);
    } catch (error) {
      context.emit("error", {
        code: "INVALID_REQUEST",
        message: error instanceof Error ? error.message : String(error),
      });
      continue;
    }

    const result = await handleAgentShellRequest(request, context);
    if (result.action === "exit") {
      ended = true;
      exitSignal = result;
      break;
    }
  }

  if (!ended) {
    state.stdinClosed = true;
    context.emit("session.stdin.closed", {
      reason: "stdin.closed",
      activeProcessCount: state.activeProcesses.size,
      hookCount: state.hooks.size,
    });

    await drainAgentShellBackgroundWork(state);

    context.emit("session.ended", {
      reason: "stdin.closed",
    });
  } else {
    await drainAgentShellBackgroundWork(state);
    context.emit(
      "session.ended",
      {
        reason: exitSignal?.reason ?? "builtin.exit",
        origin: exitSignal?.origin ?? { kind: "request" },
      },
      exitSignal?.requestId,
    );
  }

  return 0;
}

async function runHumanShell(
  catalog: CliSurfaceCatalog,
  repoRoot: string,
  options: PodShellOptions,
): Promise<number> {
  const state: HumanShellState = {
    sessionId: createShellSessionId(),
    transport: "human",
    quiet: options.quiet,
    history: [],
    activeProcess: null,
  };

  const readline = createInterface({
    input: process.stdin,
    output: process.stderr,
    terminal: Boolean(process.stdin.isTTY),
    completer: createHumanShellCompleter(catalog),
  });

  const handleSigint = () => {
    process.stderr.write("\n");
    if (state.activeProcess) {
      try {
        state.activeProcess.kill("SIGINT");
      } catch {
        // Ignore redundant signal attempts if the child already exited.
      }
      return;
    }
    readline.prompt();
  };

  process.on("SIGINT", handleSigint);

  if (!options.quiet) {
    process.stderr.write(
      `Prompt or Die interactive shell\nsession: ${state.sessionId}\nType 'help' for shell commands. Type 'quit' to exit.\n`,
    );
  }

  readline.setPrompt("pod> ");
  readline.prompt();

  try {
    for await (const rawLine of readline) {
      const line = rawLine.trim();
      if (line.length === 0) {
        readline.prompt();
        continue;
      }

      state.history.push(line);

      const builtinResult = executeHumanShellBuiltin(line, state, catalog);
      if (builtinResult) {
        if (builtinResult.clearScreen) {
          process.stderr.write("\u001bc");
        }
        if (builtinResult.stderrText) {
          process.stderr.write(`${builtinResult.stderrText}\n`);
        }
        if (builtinResult.action === "exit") {
          break;
        }
        readline.prompt();
        continue;
      }

      try {
        const command = parsePodArgs(tokenizeShellInput(line));
        if (command.kind === "shell") {
          process.stderr.write("Already inside the interactive shell.\n");
          readline.prompt();
          continue;
        }
        if (command.kind === "run") {
          const entry = resolveRunEntry(catalog, command.options.target);
          const mode = classifyShellRunMode(entry, "human", command.options);
          readline.pause();
          const payload = await executeRunCommand(catalog, repoRoot, command.options, {
            mode,
            onProcessStart: (subprocess) => {
              state.activeProcess = subprocess;
            },
            onProcessExit: () => {
              state.activeProcess = null;
            },
          });
          if (payload.status === "dry-run") {
            process.stdout.write(`${payload.resolvedCommand}\n`);
          } else if (payload.status === "refused") {
            if (command.options.json) {
              process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
            } else {
              process.stderr.write(
                `${payload.refusalMessage ?? payload.refusalCode ?? "run refused"}\n`,
              );
            }
          } else if (mode === "payload") {
            process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
          }
          readline.resume();
          readline.prompt();
          continue;
        }

        const exitCode = await executePodCommand(command, catalog, repoRoot);
        if (exitCode !== 0) {
          process.stderr.write(`command exited with code ${exitCode}\n`);
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        process.stderr.write(`${message}\n`);
      }

      readline.prompt();
    }
  } finally {
    process.off("SIGINT", handleSigint);
    readline.close();
  }

  return 0;
}

async function executeShellCommand(
  options: PodShellOptions,
  catalog: CliSurfaceCatalog,
  repoRoot: string,
): Promise<number> {
  if (options.transport === "agent") {
    return await runAgentShell(catalog, repoRoot);
  }
  return await runHumanShell(catalog, repoRoot, options);
}

async function executePodCommand(
  command: PodCommand,
  catalog: CliSurfaceCatalog,
  repoRoot: string,
): Promise<number> {
  switch (command.kind) {
    case "help":
      printHelp();
      return 0;
    case "shell":
      return await executeShellCommand(command.options, catalog, repoRoot);
    case "list": {
      const payload = buildListPayload(catalog, repoRoot, command.options);
      if (command.options.json) {
        process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
      } else {
        process.stdout.write(renderHumanList(payload));
      }
      return 0;
    }
    case "show": {
      const payload = buildEntryPayload(
        catalog,
        repoRoot,
        "show",
        command.options.id,
      );
      if (command.options.json) {
        process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
      } else {
        process.stdout.write(renderHumanEntry(payload));
      }
      return 0;
    }
    case "env": {
      const payload = buildEntryPayload(
        catalog,
        repoRoot,
        "env",
        command.options.id,
        command.options.effective,
      );
      if (command.options.json) {
        process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
      } else if (payload.environment.length === 0) {
        process.stdout.write(`No environment contract for ${payload.entry.id}.\n`);
      } else {
        process.stdout.write(renderHumanEntry(payload));
      }
      return 0;
    }
    case "command": {
      const payload = buildEntryPayload(
        catalog,
        repoRoot,
        "command",
        command.options.id,
      );
      if (command.options.json) {
        process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
      } else {
        process.stdout.write(`${payload.resolvedCommand}\n`);
      }
      return 0;
    }
    case "export": {
      const payload = buildExportPayload(repoRoot, command.options);
      process.stdout.write(payload.text);
      if (!payload.text.endsWith("\n")) {
        process.stdout.write("\n");
      }
      return 0;
    }
    case "run": {
      const payload = await executeRunCommand(catalog, repoRoot, command.options);
      if (command.options.json) {
        process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
      } else if (payload.status === "dry-run") {
        process.stdout.write(`${payload.resolvedCommand}\n`);
      } else if (payload.status === "refused") {
        process.stderr.write(
          `${payload.refusalMessage ?? payload.refusalCode ?? "run refused"}\n`,
        );
      }
      return payload.exitCode ?? (payload.ok ? 0 : 1);
    }
  }
}

export async function main() {
  try {
    const command = parsePodArgs(process.argv.slice(2));
    const repoRoot = resolve(import.meta.dir, "..");
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const validation = validateCliSurfaceCatalog(catalog, repoRoot);
    const integrityIssues = buildValidationIssues(validation);
    if (integrityIssues.length > 0) {
      fail(
        `CLI catalog integrity failed:\n- ${integrityIssues.join("\n- ")}\nRun bun ./scripts/verify_cli_surface.ts --check for the full report.`,
      );
    }

    process.exitCode = await executePodCommand(command, catalog, repoRoot);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error(message);
    process.exitCode = 1;
  }
}

if (import.meta.main) {
  void main();
}
