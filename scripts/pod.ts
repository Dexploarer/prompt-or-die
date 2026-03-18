#!/usr/bin/env bun

import { existsSync } from "node:fs";
import { resolve } from "node:path";

import {
  buildCliSurfaceCatalog,
  CLI_AREAS,
  CLI_AUDIENCES,
  CLI_KINDS,
  filterCliSurfaceEntries,
  findCliSurfaceEntry,
  resolveCliSurfaceCommand,
  type CliArea,
  type CliAudience,
  type CliKind,
  type CliSurfaceCatalog,
  type CliSurfaceEntry,
} from "./cli_surface";

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

type PodRunOptions = {
  id: string;
  json: boolean;
  dryRun: boolean;
};

type PodCommand =
  | { kind: "help" }
  | { kind: "list"; options: PodListOptions }
  | { kind: "show"; options: PodEntityOptions }
  | { kind: "env"; options: PodEntityOptions }
  | { kind: "command"; options: PodEntityOptions }
  | { kind: "run"; options: PodRunOptions };

type CommandPayload = {
  schemaVersion: 1;
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

type EntryPayload = CommandPayload & {
  command: "show" | "env" | "command";
  entry: CliSurfaceEntry;
  resolvedCommand: string;
  environment: Array<{
    name: string;
    defaultValue: string;
    description: string;
  }>;
};

type RunPayload = CommandPayload & {
  command: "run";
  entry: CliSurfaceEntry;
  resolvedCommand: string;
  workingDirectory: string;
  dryRun: boolean;
  ok: boolean;
  exitCode: number;
  durationMs: number;
};

function fail(message: string): never {
  throw new Error(message);
}

function printHelp() {
  console.error(`Prompt or Die CLI
deterministic command surface

Usage:
  bun ./scripts/pod.ts list [--audience <audience>] [--area <area>] [--kind <kind>] [--machine-readable] [--text <query>] [--json]
  bun ./scripts/pod.ts show <id> [--json]
  bun ./scripts/pod.ts env <id> [--json]
  bun ./scripts/pod.ts command <id> [--json]
  bun ./scripts/pod.ts run <id> [--dry-run] [--json]

Examples:
  bun ./scripts/pod.ts list
  bun ./scripts/pod.ts list --audience agent --machine-readable --json
  bun ./scripts/pod.ts show pod-headless
  bun ./scripts/pod.ts env pod-server
  bun ./scripts/pod.ts command bootstrap-reference-world
  bun ./scripts/pod.ts run bootstrap-reference-world

Valid audiences: ${CLI_AUDIENCES.join(", ")}
Valid areas: ${CLI_AREAS.join(", ")}
Valid kinds: ${CLI_KINDS.join(", ")}`);
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

function parseListArgs(argv: string[]): PodListOptions {
  const options: PodListOptions = {
    machineReadableOnly: false,
    json: false,
  };

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--audience":
        options.audience = parseAudience(argv[index + 1] ?? fail("missing value for --audience"));
        index += 1;
        break;
      case "--area":
        options.area = parseArea(argv[index + 1] ?? fail("missing value for --area"));
        index += 1;
        break;
      case "--kind":
        options.kind = parseKind(argv[index + 1] ?? fail("missing value for --kind"));
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

function parseRunArgs(argv: string[]): PodRunOptions {
  const id = argv[0] ?? fail("missing required command id");
  const options: PodRunOptions = { id, json: false, dryRun: false };
  for (let index = 1; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--json":
        options.json = true;
        break;
      case "--dry-run":
        options.dryRun = true;
        break;
      default:
        fail(`unknown argument: ${current}`);
    }
  }
  return options;
}

export function parsePodArgs(argv: string[]): PodCommand {
  const [subcommand, ...rest] = argv;
  switch (subcommand) {
    case undefined:
    case "help":
    case "--help":
    case "-h":
      return { kind: "help" };
    case "list":
      return { kind: "list", options: parseListArgs(rest) };
    case "show":
      return { kind: "show", options: parseEntityArgs(rest) };
    case "env":
      return { kind: "env", options: parseEntityArgs(rest) };
    case "command":
      return { kind: "command", options: parseEntityArgs(rest) };
    case "run":
      return { kind: "run", options: parseRunArgs(rest) };
    default:
      fail(`unknown subcommand: ${subcommand}`);
  }
}

function getIntegrityIssues(catalog: CliSurfaceCatalog, repoRoot: string): string[] {
  const discoveredKeys = new Set(catalog.discoveredSurfaces.map((surface) => surface.key));
  const coveredKeys = new Set(catalog.commands.flatMap((entry) => entry.coverage));
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
  const unknownCoverageKeys = Array.from(coveredKeys)
    .filter((key) => !discoveredKeys.has(key))
    .sort();
  const uncovered = catalog.discoveredSurfaces
    .filter((surface) => !coveredKeys.has(surface.key))
    .map((surface) => surface.key);

  const issues: string[] = [];
  if (duplicateIds.length > 0) {
    issues.push(`duplicate ids: ${duplicateIds.join(", ")}`);
  }
  if (missingEntrypoints.length > 0) {
    issues.push(`missing entrypoints: ${missingEntrypoints.join(", ")}`);
  }
  if (missingDocs.length > 0) {
    issues.push(`missing docs: ${missingDocs.join(", ")}`);
  }
  if (unknownCoverageKeys.length > 0) {
    issues.push(`unknown coverage keys: ${unknownCoverageKeys.join(", ")}`);
  }
  if (uncovered.length > 0) {
    issues.push(`uncovered discovered surfaces: ${uncovered.join(", ")}`);
  }
  return issues;
}

export function buildListPayload(
  catalog: CliSurfaceCatalog,
  repoRoot: string,
  options: PodListOptions,
): ListPayload {
  return {
    schemaVersion: 1,
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

export function buildEntryPayload(
  catalog: CliSurfaceCatalog,
  repoRoot: string,
  mode: EntryPayload["command"],
  id: string,
): EntryPayload {
  const entry = findCliSurfaceEntry(catalog, id);
  if (!entry) {
    fail(`unknown command id: ${id}`);
  }
  const environment = catalog.serverEnvironment
    .filter((variable) => entry.env.includes(variable.name))
    .map((variable) => ({
      name: variable.name,
      defaultValue: variable.defaultValue,
      description: variable.description,
    }));

  return {
    schemaVersion: 1,
    generatedAtUnixMs: Date.now(),
    repoRoot,
    command: mode,
    entry,
    resolvedCommand: resolveCliSurfaceCommand(entry),
    environment,
  };
}

function pad(value: string, width: number): string {
  return value.length >= width ? value : `${value}${" ".repeat(width - value.length)}`;
}

export function renderHumanList(payload: ListPayload): string {
  const lines = [
    "Prompt or Die CLI",
    "deterministic command surface",
    "",
    `commands: ${payload.commands.length}`,
    "",
  ];

  const idWidth = Math.max(2, ...payload.commands.map((entry) => entry.id.length));
  const areaWidth = Math.max(4, ...payload.commands.map((entry) => entry.area.length));
  const kindWidth = Math.max(4, ...payload.commands.map((entry) => entry.kind.length));
  for (const entry of payload.commands) {
    lines.push(
      `${pad(entry.id, idWidth)}  ${pad(entry.area, areaWidth)}  ${pad(entry.kind, kindWidth)}  ${entry.summary}`,
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
    `audiences: ${payload.entry.audiences.join(", ")}`,
    `area: ${payload.entry.area}`,
    `kind: ${payload.entry.kind}`,
    `cwd: ${payload.entry.cwd}`,
    `command: ${payload.resolvedCommand}`,
    `machine-readable: ${payload.entry.machineReadable ? "yes" : "no"}`,
  ];

  if (payload.entry.outputArtifacts.length > 0) {
    lines.push(`outputs: ${payload.entry.outputArtifacts.join(", ")}`);
  }
  if (payload.entry.docs.length > 0) {
    lines.push(`docs: ${payload.entry.docs.join(", ")}`);
  }
  if (payload.environment.length > 0) {
    lines.push("", "environment:");
    for (const variable of payload.environment) {
      lines.push(
        `  ${variable.name}=${variable.defaultValue}  ${variable.description}`,
      );
    }
  }
  if (payload.entry.notes.length > 0) {
    lines.push("", "notes:");
    for (const note of payload.entry.notes) {
      lines.push(`  - ${note}`);
    }
  }

  return `${lines.join("\n")}\n`;
}

async function runEntry(
  catalog: CliSurfaceCatalog,
  repoRoot: string,
  options: PodRunOptions,
): Promise<RunPayload> {
  const entry = findCliSurfaceEntry(catalog, options.id);
  if (!entry) {
    fail(`unknown command id: ${options.id}`);
  }

  const cwd =
    entry.cwd === "." ? repoRoot : resolve(repoRoot, entry.cwd);
  const resolvedCommand = resolveCliSurfaceCommand(entry);

  if (options.dryRun) {
    return {
      schemaVersion: 1,
      generatedAtUnixMs: Date.now(),
      repoRoot,
      command: "run",
      entry,
      resolvedCommand,
      workingDirectory: cwd,
      dryRun: true,
      ok: true,
      exitCode: 0,
      durationMs: 0,
    };
  }

  const started = performance.now();
  const processHandle = Bun.spawn({
    cmd: ["/bin/zsh", "-lc", entry.command],
    cwd,
    env: process.env,
    stdout: options.json ? "ignore" : "inherit",
    stderr: options.json ? "ignore" : "inherit",
  });
  const exitCode = await processHandle.exited;
  const durationMs = performance.now() - started;

  return {
    schemaVersion: 1,
    generatedAtUnixMs: Date.now(),
    repoRoot,
    command: "run",
    entry,
    resolvedCommand,
    workingDirectory: cwd,
    dryRun: false,
    ok: exitCode === 0,
    exitCode,
    durationMs,
  };
}

export async function main() {
  try {
    const command = parsePodArgs(process.argv.slice(2));
    if (command.kind === "help") {
      printHelp();
      return;
    }

    const repoRoot = resolve(import.meta.dir, "..");
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const integrityIssues = getIntegrityIssues(catalog, repoRoot);
    if (integrityIssues.length > 0) {
      fail(
        `CLI catalog integrity failed:\n- ${integrityIssues.join("\n- ")}\nRun bun ./scripts/verify_cli_surface.ts --check for the full report.`,
      );
    }

    switch (command.kind) {
      case "list": {
        const payload = buildListPayload(catalog, repoRoot, command.options);
        if (command.options.json) {
          console.log(JSON.stringify(payload, null, 2));
        } else {
          process.stdout.write(renderHumanList(payload));
        }
        return;
      }
      case "show": {
        const payload = buildEntryPayload(
          catalog,
          repoRoot,
          "show",
          command.options.id,
        );
        if (command.options.json) {
          console.log(JSON.stringify(payload, null, 2));
        } else {
          process.stdout.write(renderHumanEntry(payload));
        }
        return;
      }
      case "env": {
        const payload = buildEntryPayload(
          catalog,
          repoRoot,
          "env",
          command.options.id,
        );
        if (command.options.json) {
          console.log(JSON.stringify(payload, null, 2));
        } else if (payload.environment.length === 0) {
          process.stdout.write(`No environment contract for ${payload.entry.id}.\n`);
        } else {
          process.stdout.write(renderHumanEntry(payload));
        }
        return;
      }
      case "command": {
        const payload = buildEntryPayload(
          catalog,
          repoRoot,
          "command",
          command.options.id,
        );
        if (command.options.json) {
          console.log(JSON.stringify(payload, null, 2));
        } else {
          process.stdout.write(`${payload.resolvedCommand}\n`);
        }
        return;
      }
      case "run": {
        const payload = await runEntry(catalog, repoRoot, command.options);
        if (command.options.json) {
          console.log(JSON.stringify(payload, null, 2));
        } else if (command.options.dryRun) {
          process.stdout.write(`${payload.resolvedCommand}\n`);
        }
        process.exitCode = payload.exitCode;
        return;
      }
      case "help":
        return;
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error(message);
    process.exitCode = 1;
  }
}

if (import.meta.main) {
  void main();
}
