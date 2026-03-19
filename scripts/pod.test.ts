import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";

import {
  buildCliSurfaceCatalog,
  renderCliExecutionCommand,
  type CliLifecycle,
  type CliSurfaceCatalog,
  type CliSurfaceEntry,
} from "./cli_surface";
import {
  buildEntryPayload,
  buildExportPayload,
  buildListPayload,
  classifyShellRunMode,
  decodeAgentShellEventDocuments,
  encodeAgentShellRequestDocument,
  executeHumanShellBuiltin,
  executeRunCommand,
  getHumanShellCompletions,
  parsePodArgs,
  renderHumanList,
  runAgentShellSessionForTest,
  tokenizeShellInput,
} from "./pod";

const repoRoot = resolve(import.meta.dir, "..");

function buildFixtureCatalog(options?: {
  fixtureMode?: "args" | "stream" | "repeat" | "fail" | "delayed-exit";
  lifecycle?: CliLifecycle;
  supportsPassthrough?: boolean;
  allowedEnvOverrides?: string[];
}): CliSurfaceCatalog {
  const fixtureMode = options?.fixtureMode ?? "args";
  const lifecycle = options?.lifecycle ?? "finite";
  const supportsPassthrough = options?.supportsPassthrough ?? true;
  const allowedEnvOverrides = options?.allowedEnvOverrides ?? ["POD_TEST_VALUE"];
  const execution = {
    program: "bun",
    args: ["./scripts/fixtures/pod_run_fixture.ts", fixtureMode],
    cwd: ".",
    lifecycle,
    passthrough: supportsPassthrough
      ? ("append-after-double-dash" as const)
      : ("disabled" as const),
    allowedEnvOverrides,
  };
  const entry: CliSurfaceEntry = {
    id: "fixture-runner",
    name: "Fixture Runner",
    aliases: ["benchmark fixture"],
    audiences: ["developer", "agent"],
    area: "benchmark",
    kind: "bun-script",
    command: renderCliExecutionCommand(execution),
    cwd: ".",
    entrypoint: "scripts/fixtures/pod_run_fixture.ts",
    summary: "Deterministic fixture command for CLI contract tests.",
    machineReadable: true,
    outputArtifacts: [],
    docs: ["README.md"],
    env: ["POD_TEST_VALUE"],
    notes: [],
    coverage: [],
    execution,
    capabilities: {
      supportsDryRun: true,
      supportsPassthrough,
      supportsEffectiveEnv: true,
      requiresNetwork: false,
      mutatesState: false,
      attachesToTerminal: lifecycle === "long-running",
    },
    interactive: null,
  };

  return {
    schemaVersion: 2,
    sourceOfTruth: "tests",
    scope: "test catalog",
    commands: [entry],
    serverEnvironment: [
      {
        name: "POD_TEST_VALUE",
        defaultValue: "unset",
        usedBy: ["fixture-runner"],
        description: "Fixture-only env override.",
      },
    ],
    discoveredSurfaces: [],
  };
}

async function runAgentShellCli(requests: Array<Record<string, unknown>>) {
  const subprocess = Bun.spawn({
    cmd: [process.execPath, "./scripts/pod.ts", "shell", "--agent"],
    cwd: repoRoot,
    stdin: "pipe",
    stdout: "pipe",
    stderr: "pipe",
  });

  for (const request of requests) {
    subprocess.stdin.write(
      `${encodeAgentShellRequestDocument(request)}\n`,
    );
  }
  subprocess.stdin.end();

  const [exitCode, stdoutText, stderrText] = await Promise.all([
    subprocess.exited,
    new Response(subprocess.stdout).text(),
    new Response(subprocess.stderr).text(),
  ]);

  return {
    exitCode,
    stderrText,
    events: decodeAgentShellEventDocuments(stdoutText) as Array<Record<string, unknown>>,
  };
}

describe("pod cli", () => {
  test("parses list filters and json mode", () => {
    expect(
      parsePodArgs([
        "list",
        "--audience",
        "agent",
        "--area",
        "benchmark",
        "--kind",
        "cargo-example",
        "--machine-readable",
        "--text",
        "topology",
        "--json",
      ]),
    ).toEqual({
      kind: "list",
      options: {
        audience: "agent",
        area: "benchmark",
        kind: "cargo-example",
        machineReadableOnly: true,
        text: "topology",
        json: true,
      },
    });
  });

  test("parses alias runs with passthrough args", () => {
    expect(
      parsePodArgs([
        "runtime",
        "server",
        "--dry-run",
        "--json",
        "--env",
        "POD_WORLD_SEED=99",
        "--",
        "--profile",
        "ci-smoke",
      ]),
    ).toEqual({
      kind: "run",
      options: {
        target: { kind: "alias", value: "runtime server" },
        json: true,
        dryRun: true,
        envOverrides: {
          POD_WORLD_SEED: "99",
        },
        extraArgs: ["--profile", "ci-smoke"],
      },
    });
  });

  test("parses export targets and formats", () => {
    expect(parsePodArgs(["export", "world"])).toEqual({
      kind: "export",
      options: {
        target: "world",
        format: "json",
      },
    });
    expect(parsePodArgs(["export", "events", "--format", "toon"])).toEqual({
      kind: "export",
      options: {
        target: "events",
        format: "toon",
      },
    });
  });

  test("parses shell transport flags", () => {
    expect(parsePodArgs(["shell"])).toEqual({
      kind: "shell",
      options: {
        mode: "human",
        quiet: false,
        transport: "human",
      },
    });
    expect(parsePodArgs(["shell", "--agent"])).toEqual({
      kind: "shell",
      options: {
        mode: "json",
        quiet: true,
        transport: "agent",
      },
    });
    expect(parsePodArgs(["shell", "--mode", "json", "--quiet"])).toEqual({
      kind: "shell",
      options: {
        mode: "json",
        quiet: true,
        transport: "agent",
      },
    });
    expect(() => parsePodArgs(["shell", "--agent", "--mode", "human"])).toThrow(
      "--agent cannot be combined with --mode human",
    );
  });

  test("returns help for help aliases and rejects unknown subcommands", () => {
    expect(parsePodArgs(["help"])).toEqual({ kind: "help" });
    expect(parsePodArgs(["--help"])).toEqual({ kind: "help" });
    expect(parsePodArgs(["-h"])).toEqual({ kind: "help" });
    expect(() => parsePodArgs(["unknown"])).toThrow("unknown subcommand");
  });

  test("rejects malformed arguments", () => {
    expect(() => parsePodArgs(["list", "--audience"])).toThrow(
      "missing value for --audience",
    );
    expect(() => parsePodArgs(["list", "--area", "nope"])).toThrow(
      "unknown area: nope",
    );
    expect(() => parsePodArgs(["env", "pod-server", "--bogus"])).toThrow(
      "unknown argument: --bogus",
    );
    expect(() => parsePodArgs(["export", "nope"])).toThrow(
      "unknown export target: nope",
    );
    expect(() => parsePodArgs(["run", "pod-server", "--env", "broken"])).toThrow(
      "invalid --env override",
    );
  });

  test("tokenizes shell input with quotes, escapes, and empty lines", () => {
    expect(tokenizeShellInput("")).toEqual([]);
    expect(
      tokenizeShellInput(`run pod-headless -- --profile "ci smoke"`),
    ).toEqual(["run", "pod-headless", "--", "--profile", "ci smoke"]);
    expect(tokenizeShellInput(String.raw`show pod\-server`)).toEqual([
      "show",
      "pod-server",
    ]);
    expect(tokenizeShellInput(`env 'pod server' --effective`)).toEqual([
      "env",
      "pod server",
      "--effective",
    ]);
    expect(() => tokenizeShellInput(`show "unterminated`)).toThrow(
      "unterminated quoted string",
    );
  });

  test("builds shell completions for builtins, aliases, ids, and flags", () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);

    expect(getHumanShellCompletions("ru", catalog)).toContain("run");
    expect(getHumanShellCompletions("runtime ", catalog)).toContain("server");
    expect(getHumanShellCompletions("show pod", catalog)).toContain("pod-server");
    expect(getHumanShellCompletions("list --area ", catalog)).toContain("runtime");
    expect(getHumanShellCompletions("export ", catalog)).toContain("world");
    expect(getHumanShellCompletions("export world --format ", catalog)).toContain(
      "toon",
    );
    expect(getHumanShellCompletions("env pod-server --", catalog)).toContain(
      "--effective",
    );
  });

  test("executes human shell builtins without mode switching", () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const state = {
      sessionId: "test-shell",
      transport: "human" as const,
      quiet: false,
      history: ["context", "history"],
      activeProcess: null as ReturnType<typeof Bun.spawn> | null,
    };

    const helpResult = executeHumanShellBuiltin("help", state, catalog);
    const contextResult = executeHumanShellBuiltin("context", state, catalog);
    const historyResult = executeHumanShellBuiltin("history", state, catalog);
    const clearResult = executeHumanShellBuiltin("clear", state, catalog);
    const exitResult = executeHumanShellBuiltin("quit", state, catalog);

    expect(helpResult?.stderrText).toContain("Interactive POD shell");
    expect(contextResult?.stderrText).toContain("transport: human");
    expect(historyResult?.stderrText).toContain(" 1  context");
    expect(clearResult).toMatchObject({
      action: "continue",
      clearScreen: true,
    });
    expect(exitResult).toMatchObject({
      action: "exit",
    });
  });

  test("classifies shell run execution by transport and lifecycle", () => {
    const finiteEntry = buildFixtureCatalog().commands[0];
    const longRunningEntry = buildFixtureCatalog({
      fixtureMode: "stream",
      lifecycle: "long-running",
    }).commands[0];

    expect(
      classifyShellRunMode(longRunningEntry, "human", {
        target: { kind: "id", value: "fixture-runner" },
        json: false,
        dryRun: false,
        envOverrides: {},
        extraArgs: [],
      }),
    ).toBe("attach");
    expect(
      classifyShellRunMode(finiteEntry, "agent", {
        target: { kind: "alias", value: "benchmark fixture" },
        json: false,
        dryRun: false,
        envOverrides: {},
        extraArgs: [],
      }),
    ).toBe("payload");
    expect(
      classifyShellRunMode(longRunningEntry, "agent", {
        target: { kind: "alias", value: "benchmark fixture" },
        json: false,
        dryRun: false,
        envOverrides: {},
        extraArgs: [],
      }),
    ).toBe("stream");
  });

  test("builds a filtered list payload with schema version 2 and aliases", () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const payload = buildListPayload(catalog, repoRoot, {
      audience: "agent",
      area: "benchmark",
      kind: "cargo-example",
      machineReadableOnly: true,
      json: false,
      text: "topology",
    });

    expect(payload.schemaVersion).toBe(2);
    expect(payload.commands.map((entry) => entry.id)).toEqual([
      "topology-feed-benchmark-suite",
    ]);
    expect(payload.commands[0]?.aliases).toContain("benchmark topology-feed");
  });

  test("describes the server entry with aliases, capabilities, and execution metadata", () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const payload = buildEntryPayload(catalog, repoRoot, "show", "pod-server");

    expect(payload.schemaVersion).toBe(2);
    expect(payload.entry.aliases).toContain("runtime server");
    expect(payload.entry.capabilities.supportsEffectiveEnv).toBe(true);
    expect(payload.entry.execution.program).toBe("cargo");
    expect(payload.entry.execution.lifecycle).toBe("long-running");
    expect(payload.environment.map((variable) => variable.name)).toContain(
      "POD_BIND_ADDRESS",
    );
    expect(payload.effectiveEnvironment).toBe(false);
  });

  test("builds export payloads for TOON-facing world data", () => {
    const payload = buildExportPayload(repoRoot, {
      target: "events",
      format: "toon",
    });

    expect(payload.command).toBe("export");
    expect(payload.target).toBe("events");
    expect(payload.format).toBe("toon");
    expect(payload.contentType).toBe("application/toon");
    expect(payload.preferredFormat).toBe("toon");
    expect(payload.preferredToonDelimiter).toBe("tab");
    expect(payload.text).toContain("tick_event_batch");
  });

  test("resolves effective environment values", () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const original = process.env.POD_WORLD_SEED;
    process.env.POD_WORLD_SEED = "123";

    try {
      const payload = buildEntryPayload(
        catalog,
        repoRoot,
        "env",
        "pod-server",
        true,
      );
      const worldSeed = payload.environment.find(
        (variable) => variable.name === "POD_WORLD_SEED",
      );
      const opsArchive = payload.environment.find(
        (variable) => variable.name === "POD_OPS_ARCHIVE_DIR",
      );

      expect(payload.effectiveEnvironment).toBe(true);
      expect(worldSeed).toMatchObject({
        resolvedValue: "123",
        source: "process",
      });
      expect(opsArchive).toMatchObject({
        resolvedValue: null,
        source: "unset",
      });
    } finally {
      if (original == null) {
        delete process.env.POD_WORLD_SEED;
      } else {
        process.env.POD_WORLD_SEED = original;
      }
    }
  });

  test("renders a human-readable list with aliases", () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const payload = buildListPayload(catalog, repoRoot, {
      audience: undefined,
      area: "catalog",
      kind: undefined,
      machineReadableOnly: false,
      json: false,
      text: undefined,
    });
    const rendered = renderHumanList(payload);

    expect(rendered).toContain("Prompt or Die CLI");
    expect(rendered).toContain("catalog verify");
    expect(rendered).toContain("verify-cli-surface");
  });

  test("returns dry-run payloads for aliases and ids", async () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);

    const byId = await executeRunCommand(catalog, repoRoot, {
      target: { kind: "id", value: "pod-server" },
      json: true,
      dryRun: true,
      envOverrides: {},
      extraArgs: [],
    });
    const byAlias = await executeRunCommand(catalog, repoRoot, {
      target: { kind: "alias", value: "runtime server" },
      json: true,
      dryRun: true,
      envOverrides: {},
      extraArgs: [],
    });

    expect(byId.status).toBe("dry-run");
    expect(byAlias.status).toBe("dry-run");
    expect(byId.entry.id).toBe("pod-server");
    expect(byAlias.entry.id).toBe("pod-server");
    expect(byAlias.requestedTarget).toEqual({
      kind: "alias",
      value: "runtime server",
    });
  });

  test("refuses json execution for long-running commands", async () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const payload = await executeRunCommand(catalog, repoRoot, {
      target: { kind: "alias", value: "runtime server" },
      json: true,
      dryRun: false,
      envOverrides: {},
      extraArgs: [],
    });

    expect(payload.status).toBe("refused");
    expect(payload.refusalCode).toBe("LONG_RUNNING_REQUIRES_ATTACH");
    expect(payload.ok).toBe(false);
  });

  test("captures stdout, stderr, passthrough args, and env overrides for finite runs", async () => {
    const catalog = buildFixtureCatalog();
    const original = process.env.POD_TEST_VALUE;
    delete process.env.POD_TEST_VALUE;

    try {
      const payload = await executeRunCommand(catalog, repoRoot, {
        target: { kind: "alias", value: "benchmark fixture" },
        json: true,
        dryRun: false,
        envOverrides: {
          POD_TEST_VALUE: "from-test",
        },
        extraArgs: ["one", "two"],
      });

      expect(payload.status).toBe("executed");
      expect(payload.ok).toBe(true);
      expect(payload.execution.args.at(-2)).toBe("one");
      expect(payload.execution.args.at(-1)).toBe("two");
      expect(payload.stdout?.preview).toContain('["one","two"]');
      expect(payload.stderr?.preview).toContain("env:from-test");
      expect(payload.envOverrides).toEqual({
        POD_TEST_VALUE: "from-test",
      });
      expect(payload.refusalCode).toBeNull();
    } finally {
      if (original == null) {
        delete process.env.POD_TEST_VALUE;
      } else {
        process.env.POD_TEST_VALUE = original;
      }
    }
  });

  test("captures non-zero exits and truncates large output deterministically", async () => {
    const repeatCatalog = buildFixtureCatalog({
      fixtureMode: "repeat",
    });
    repeatCatalog.commands[0].execution.args = [
      "./scripts/fixtures/pod_run_fixture.ts",
      "repeat",
      "4096",
    ];
    repeatCatalog.commands[0].command = renderCliExecutionCommand(
      repeatCatalog.commands[0].execution,
    );

    const repeatPayload = await executeRunCommand(repeatCatalog, repoRoot, {
      target: { kind: "id", value: "fixture-runner" },
      json: true,
      dryRun: false,
      envOverrides: {},
      extraArgs: [],
    });

    expect(repeatPayload.stdout?.truncated).toBe(true);
    expect(repeatPayload.stderr?.truncated).toBe(true);

    const failCatalog = buildFixtureCatalog({
      fixtureMode: "fail",
    });
    const failPayload = await executeRunCommand(failCatalog, repoRoot, {
      target: { kind: "id", value: "fixture-runner" },
      json: true,
      dryRun: false,
      envOverrides: {},
      extraArgs: [],
    });

    expect(failPayload.status).toBe("executed");
    expect(failPayload.ok).toBe(false);
    expect(failPayload.exitCode).toBe(7);
    expect(failPayload.stderr?.preview).toContain("fixture failed");
  });

  test("refuses disallowed env overrides and passthrough when not enabled", async () => {
    const noEnvCatalog = buildFixtureCatalog({
      allowedEnvOverrides: [],
    });
    const envRefusal = await executeRunCommand(noEnvCatalog, repoRoot, {
      target: { kind: "id", value: "fixture-runner" },
      json: true,
      dryRun: false,
      envOverrides: {
        POD_TEST_VALUE: "forbidden",
      },
      extraArgs: [],
    });
    expect(envRefusal.status).toBe("refused");
    expect(envRefusal.refusalCode).toBe("ENV_OVERRIDE_NOT_ALLOWED");

    const noPassthroughCatalog = buildFixtureCatalog({
      supportsPassthrough: false,
    });
    const passthroughRefusal = await executeRunCommand(
      noPassthroughCatalog,
      repoRoot,
      {
        target: { kind: "id", value: "fixture-runner" },
        json: true,
        dryRun: false,
        envOverrides: {},
        extraArgs: ["one"],
      },
    );
    expect(passthroughRefusal.status).toBe("refused");
    expect(passthroughRefusal.refusalCode).toBe("PASSTHROUGH_NOT_ALLOWED");
  });

  test("runs agent shell builtins and survives malformed requests", async () => {
    const catalog = buildFixtureCatalog();
    const events = await runAgentShellSessionForTest({
      documents: [
        encodeAgentShellRequestDocument({
          type: "builtin",
          requestId: "ctx",
          name: "context",
        }),
        "not-json",
        encodeAgentShellRequestDocument({
          type: "builtin",
          requestId: "help",
          name: "help",
        }),
        encodeAgentShellRequestDocument({
          type: "builtin",
          requestId: "bye",
          name: "exit",
        }),
      ],
      catalog,
      repoRoot,
    });

    expect(events[0]).toMatchObject({
      type: "session.started",
      protocolVersion: 3,
      transport: "stdio-json",
      encoding: "json",
      framing: "newline-delimited",
    });
    expect(
      events.find((event) => event.type === "command.result" && event.requestId === "ctx"),
    ).toMatchObject({
      requestId: "ctx",
      builtin: "context",
    });
    expect(events.find((event) => event.type === "error")).toMatchObject({
      code: "INVALID_REQUEST",
    });
    expect(
      events.find((event) => event.type === "command.result" && event.requestId === "help"),
    ).toBeTruthy();
    expect(events.at(-1)).toMatchObject({
      type: "session.ended",
      requestId: "bye",
    });
  });

  test("wraps finite run payloads inside agent shell command results", async () => {
    const catalog = buildFixtureCatalog();
    const events = await runAgentShellSessionForTest({
      documents: [
        encodeAgentShellRequestDocument({
          type: "command",
          requestId: "run-1",
          argv: ["benchmark", "fixture", "--env", "POD_TEST_VALUE=from-agent"],
        }),
        encodeAgentShellRequestDocument({
          type: "builtin",
          requestId: "bye",
          name: "exit",
        }),
      ],
      catalog,
      repoRoot,
    });

    const runResult = events.find(
      (event) => event.type === "command.result" && event.requestId === "run-1",
    ) as Record<string, unknown> | undefined;

    expect(runResult?.command).toBe("run");
    expect((runResult?.payload as Record<string, unknown>)?.status).toBe("executed");
    expect(
      ((runResult?.payload as Record<string, unknown>)?.stdout as Record<string, unknown>)
        ?.preview,
    ).toContain("[]");
  });

  test("returns export payloads inside agent shell command results", async () => {
    const catalog = buildFixtureCatalog();
    const events = await runAgentShellSessionForTest({
      documents: [
        encodeAgentShellRequestDocument({
          type: "command",
          requestId: "export-1",
          argv: ["export", "world", "--format", "toon"],
        }),
        encodeAgentShellRequestDocument({
          type: "builtin",
          requestId: "bye",
          name: "exit",
        }),
      ],
      catalog,
      repoRoot,
    });

    const exportResult = events.find(
      (event) => event.type === "command.result" && event.requestId === "export-1",
    ) as Record<string, unknown> | undefined;

    expect(exportResult?.command).toBe("export");
    expect(exportResult?.payload).toMatchObject({
      command: "export",
      target: "world",
      format: "toon",
      contentType: "application/toon",
      preferredFormat: "toon",
    });
    expect(
      String((exportResult?.payload as Record<string, unknown>)?.text ?? ""),
    ).toContain("agent_world_snapshot");
  });

  test("streams long-running run output inside agent shell sessions", async () => {
    const catalog = buildFixtureCatalog({
      fixtureMode: "stream",
      lifecycle: "long-running",
    });
    const events = await runAgentShellSessionForTest({
      documents: [
        encodeAgentShellRequestDocument({
          type: "command",
          requestId: "stream-1",
          argv: ["benchmark", "fixture"],
        }),
        encodeAgentShellRequestDocument({
          type: "builtin",
          requestId: "bye",
          name: "exit",
        }),
      ],
      catalog,
      repoRoot,
    });

    const eventTypes = events
      .filter((event) => event.requestId === "stream-1")
      .map((event) => event.type);

    expect(eventTypes).toContain("command.accepted");
    expect(eventTypes).toContain("process.started");
    expect(eventTypes).toContain("process.stdout");
    expect(eventTypes).toContain("process.stderr");
    expect(eventTypes).toContain("process.exited");
    expect(eventTypes).not.toContain("command.result");

    const stdoutChunks = events
      .filter((event) => event.type === "process.stdout")
      .map((event) => String(event.chunk));
    const stderrChunks = events
      .filter((event) => event.type === "process.stderr")
      .map((event) => String(event.chunk));

    expect(stdoutChunks.join("")).toContain("stream:stdout:1");
    expect(stderrChunks.join("")).toContain("stream:stderr:2");
  });

  test("keeps autonomous long-running jobs alive after stdin closes and triggers hooks", async () => {
    const catalog = buildFixtureCatalog({
      fixtureMode: "delayed-exit",
      lifecycle: "long-running",
    });
    catalog.commands[0].execution.args = [
      "./scripts/fixtures/pod_run_fixture.ts",
      "delayed-exit",
      "5",
      "1",
    ];
    catalog.commands[0].command = renderCliExecutionCommand(
      catalog.commands[0].execution,
    );

    const events = await runAgentShellSessionForTest({
      documents: [
        encodeAgentShellRequestDocument({
          type: "hook",
          requestId: "hook-1",
          action: "register",
          hook: {
            id: "restart-on-failure",
            on: ["process.exited"],
            match: {
              entryId: "fixture-runner",
              ok: false,
            },
            action: {
              type: "command",
              argv: ["benchmark", "fixture"],
            },
            maxTriggers: 1,
          },
        }),
        encodeAgentShellRequestDocument({
          type: "command",
          requestId: "run-1",
          argv: ["benchmark", "fixture"],
        }),
      ],
      catalog,
      repoRoot,
    });

    expect(events.find((event) => event.type === "session.stdin.closed")).toBeTruthy();
    expect(
      events.filter((event) => event.type === "process.started"),
    ).toHaveLength(2);
    expect(
      events.filter((event) => event.type === "process.exited"),
    ).toHaveLength(2);
    expect(
      events.filter((event) => event.type === "hook.triggered"),
    ).toHaveLength(1);
    expect(events.at(-1)).toMatchObject({
      type: "session.ended",
      reason: "stdin.closed",
    });
  });

  test("accepts newline-delimited JSON requests through the real agent shell CLI", async () => {
    const session = await runAgentShellCli([
      {
        type: "builtin",
        requestId: "1",
        name: "context",
      },
      {
        type: "builtin",
        requestId: "2",
        name: "exit",
      },
    ]);

    expect(session.exitCode).toBe(0);
    expect(session.stderrText).toBe("");
    expect(session.events[0]).toMatchObject({
      type: "session.started",
      protocolVersion: 3,
      transport: "stdio-json",
      encoding: "json",
      framing: "newline-delimited",
    });
    expect(
      session.events.find(
        (event) => event.type === "command.result" && event.requestId === "1",
      ),
    ).toMatchObject({
      builtin: "context",
    });
    expect(session.events.at(-1)).toMatchObject({
      type: "session.ended",
      requestId: "2",
    });
  });
});
