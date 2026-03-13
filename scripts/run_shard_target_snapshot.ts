#!/usr/bin/env bun

import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

type Options = {
  label: string;
  host: string;
  port: number;
  generatedSdkTimeoutMs: number;
  keepSpacetime: boolean;
  reuseBrowserRoutes: boolean;
  output: string;
};

type CommandSummary = {
  name: string;
  cwd: string;
  command: string;
  ok: boolean;
  exitCode: number;
  durationMs: number;
  stderrSnippet?: string;
};

type BrowserRouteCaptureStatus = "passed" | "artifact_only" | "reused";

type RunSummary = {
  schemaVersion: 1;
  generatedAtUnixMs: number;
  label: string;
  profile: "shard-target";
  browserRouteStatus: BrowserRouteCaptureStatus;
  browserRouteGatePassed: boolean;
  paths: {
    moatReport: string;
    browserRoutes: string;
    liveTopologyFeed: string;
    snapshot: string;
    topologyExport: string;
    runSummary: string;
  };
  commands: CommandSummary[];
  warnings: string[];
};

const DEFAULT_MOAT_OUTPUT = "artifacts/moat-benchmarks-shard-local.json";
const DEFAULT_BROWSER_ROUTE_OUTPUT =
  "apps/pod-web/artifacts/render-route-measurements.json";
const DEFAULT_LIVE_TOPOLOGY_OUTPUT =
  "artifacts/topology-feed-live-shard-local.json";

export function formatMonthLabel(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  return `${year}-${month}`;
}

export function parseArgs(argv: string[]): Options {
  const options: Options = {
    label: formatMonthLabel(new Date()),
    host: "127.0.0.1",
    port: 3110,
    generatedSdkTimeoutMs: 5_000,
    keepSpacetime: false,
    reuseBrowserRoutes: false,
    output: "artifacts/shard-target-snapshot-run.json",
  };

  for (let index = 0; index < argv.length; index += 1) {
    const current = argv[index];
    switch (current) {
      case "--label":
        options.label = argv[index + 1] ?? options.label;
        index += 1;
        break;
      case "--host":
        options.host = argv[index + 1] ?? options.host;
        index += 1;
        break;
      case "--port": {
        const value = Number(argv[index + 1]);
        if (!Number.isFinite(value)) {
          throw new Error("missing numeric value for --port");
        }
        options.port = value;
        index += 1;
        break;
      }
      case "--generated-sdk-timeout-ms": {
        const value = Number(argv[index + 1]);
        if (!Number.isFinite(value)) {
          throw new Error("missing numeric value for --generated-sdk-timeout-ms");
        }
        options.generatedSdkTimeoutMs = value;
        index += 1;
        break;
      }
      case "--output":
        options.output = argv[index + 1] ?? options.output;
        index += 1;
        break;
      case "--keep-spacetime":
        options.keepSpacetime = true;
        break;
      case "--reuse-browser-routes":
        options.reuseBrowserRoutes = true;
        break;
      case "--help":
      case "-h":
        printHelp();
        process.exit(0);
      default:
        throw new Error(`unknown argument: ${current}`);
    }
  }

  return options;
}

export function resolveBrowserRouteStatus(
  commandOk: boolean,
  artifactExists: boolean,
  reused: boolean,
): BrowserRouteCaptureStatus | "failed" {
  if (reused) {
    return artifactExists ? "reused" : "failed";
  }
  if (commandOk) {
    return "passed";
  }
  return artifactExists ? "artifact_only" : "failed";
}

function printHelp() {
  console.error(
    "Usage: bun ./scripts/run_shard_target_snapshot.ts [--label YYYY-MM] [--host 127.0.0.1] [--port 3110] [--generated-sdk-timeout-ms 5000] [--reuse-browser-routes] [--keep-spacetime] [--output artifacts/shard-target-snapshot-run.json]",
  );
}

function runCommand(name: string, argv: string[], cwd: string): { summary: CommandSummary; stdout: string } {
  const started = performance.now();
  const processResult = Bun.spawnSync(argv, {
    cwd,
    env: process.env,
    stdout: "pipe",
    stderr: "pipe",
  });
  const durationMs = performance.now() - started;
  const stdout = Buffer.from(processResult.stdout).toString("utf8");
  const stderr = Buffer.from(processResult.stderr).toString("utf8");

  return {
    summary: {
      name,
      cwd,
      command: argv.join(" "),
      ok: processResult.exitCode === 0,
      exitCode: processResult.exitCode ?? 1,
      durationMs,
      stderrSnippet: processResult.exitCode === 0 ? undefined : stderr.slice(0, 1600),
    },
    stdout,
  };
}

async function waitForServer(url: string, timeoutMs: number): Promise<void> {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    try {
      await fetch(url);
      return;
    } catch {
      await Bun.sleep(250);
    }
  }

  throw new Error(`timed out waiting for SpacetimeDB at ${url}`);
}

function readWorldIds(topologyPath: string): string[] {
  const topology = JSON.parse(readFileSync(topologyPath, "utf8")) as {
    worlds?: Array<{ world_id?: string }>;
  };
  const worldIds = (topology.worlds ?? [])
    .map((world) => world.world_id)
    .filter((worldId): worldId is string => typeof worldId === "string" && worldId.length > 0);
  if (worldIds.length === 0) {
    throw new Error(`no world ids found in ${topologyPath}`);
  }
  return worldIds;
}

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const repoRoot = resolve(import.meta.dir, "..");
  const podWebRoot = resolve(repoRoot, "apps/pod-web");
  const tempDir = mkdtempSync(join(tmpdir(), "pod-shard-target-"));
  const moatOutput = resolve(repoRoot, DEFAULT_MOAT_OUTPUT);
  const browserRouteOutput = resolve(repoRoot, DEFAULT_BROWSER_ROUTE_OUTPUT);
  const liveTopologyOutput = resolve(repoRoot, DEFAULT_LIVE_TOPOLOGY_OUTPUT);
  const summaryOutput = resolve(repoRoot, options.output);
  const snapshotOutput = resolve(
    repoRoot,
    `docs/benchmark-snapshots/${options.label}-shard-target.json`,
  );
  const topologyOutput = resolve(tempDir, "pod-headless-topology-shard.json");
  const scenarioReportOutput = resolve(tempDir, "pod-headless-report-shard.json");
  const datasetOutput = resolve(tempDir, "pod-headless-dataset-shard.json");
  const spacetimeUrl = `http://${options.host}:${options.port}`;
  const spacetimeDataDir = resolve(tempDir, "spacetime-data");
  const wasmPath = resolve(
    repoRoot,
    ".cargo-target/wasm32-unknown-unknown/release/pod_stdb.wasm",
  );

  mkdirSync(dirname(summaryOutput), { recursive: true });
  mkdirSync(dirname(moatOutput), { recursive: true });
  mkdirSync(dirname(liveTopologyOutput), { recursive: true });
  mkdirSync(spacetimeDataDir, { recursive: true });

  const commands: CommandSummary[] = [];
  const warnings: string[] = [];
  let browserRouteStatus: BrowserRouteCaptureStatus = "reused";
  let browserRouteGatePassed = true;
  let server: Bun.Subprocess | null = null;

  const record = (result: { summary: CommandSummary; stdout: string }) => {
    commands.push(result.summary);
    return result;
  };

  try {
    const moat = record(
      runCommand(
        "moat-shard-target",
        [
          "bun",
          "./scripts/run_moat_benchmarks.ts",
          "--profile",
          "shard-target",
          "--skip-browser",
          "--skip-creator",
          "--output",
          moatOutput,
        ],
        repoRoot,
      ),
    );
    if (!moat.summary.ok) {
      throw new Error(
        `shard-target moat benchmark failed:\n${moat.summary.stderrSnippet ?? "no stderr captured"}`,
      );
    }

    if (options.reuseBrowserRoutes) {
      browserRouteStatus = resolveBrowserRouteStatus(true, existsSync(browserRouteOutput), true);
      if (browserRouteStatus === "failed") {
        throw new Error(
          `browser route artifact missing at ${browserRouteOutput}; omit --reuse-browser-routes to regenerate it`,
        );
      }
      browserRouteGatePassed = true;
    } else {
      const browser = record(
        runCommand(
          "browser-routes",
          ["bun", "run", "measure:render-routes:check"],
          podWebRoot,
        ),
      );
      const artifactExists = existsSync(browserRouteOutput);
      const status = resolveBrowserRouteStatus(
        browser.summary.ok,
        artifactExists,
        false,
      );
      if (status === "failed") {
        throw new Error(
          `browser render-route capture failed without producing ${browserRouteOutput}:\n${browser.summary.stderrSnippet ?? "no stderr captured"}`,
        );
      }
      browserRouteStatus = status;
      browserRouteGatePassed = browser.summary.ok;
      if (status === "artifact_only") {
        warnings.push(
          "Browser render-route gates failed, but the artifact was still produced and used for snapshot publication.",
        );
      }
    }

    const headless = record(
      runCommand(
        "pod-headless-topology-export",
        [
          "cargo",
          "run",
          "-q",
          "-p",
          "pod-headless",
          "--",
          "--profile",
          "shard-target",
          "--scenario",
          "deadman-neural-cup",
          "--output",
          scenarioReportOutput,
          "--dataset-output",
          datasetOutput,
          "--topology-output",
          topologyOutput,
        ],
        repoRoot,
      ),
    );
    if (!headless.summary.ok) {
      throw new Error(
        `pod-headless topology export failed:\n${headless.summary.stderrSnippet ?? "no stderr captured"}`,
      );
    }

    const build = record(
      runCommand(
        "pod-stdb-wasm-build",
        [
          "cargo",
          "build",
          "-q",
          "-p",
          "pod-stdb",
          "--target",
          "wasm32-unknown-unknown",
          "--release",
          "--no-default-features",
          "--features",
          "module",
        ],
        repoRoot,
      ),
    );
    if (!build.summary.ok) {
      throw new Error(
        `pod-stdb wasm build failed:\n${build.summary.stderrSnippet ?? "no stderr captured"}`,
      );
    }

    server = Bun.spawn(
      [
        "spacetime",
        "start",
        "--listen-addr",
        `${options.host}:${options.port}`,
        "--data-dir",
        spacetimeDataDir,
        "--in-memory",
        "--non-interactive",
      ],
      {
        cwd: repoRoot,
        env: process.env,
        stdout: "pipe",
        stderr: "pipe",
      },
    );

    await waitForServer(spacetimeUrl, options.generatedSdkTimeoutMs);

    for (const worldId of readWorldIds(topologyOutput)) {
      const publish = record(
        runCommand(
          `publish-${worldId}`,
          [
            "spacetime",
            "publish",
            worldId,
            "--anonymous",
            "--server",
            spacetimeUrl,
            "--bin-path",
            wasmPath,
            "-y",
          ],
          repoRoot,
        ),
      );
      if (!publish.summary.ok) {
        throw new Error(
          `failed publishing ${worldId}:\n${publish.summary.stderrSnippet ?? "no stderr captured"}`,
        );
      }
    }

    const liveTopology = record(
      runCommand(
        "live-topology-feed",
        [
          "cargo",
          "run",
          "-q",
          "-p",
          "pod-net",
          "--features",
          "spacetimedb",
          "--example",
          "topology_feed_benchmark_suite",
          "--",
          "--topology-input",
          topologyOutput,
          "--output",
          liveTopologyOutput,
          "--generated-sdk-host",
          spacetimeUrl,
          "--generated-sdk-timeout-ms",
          String(options.generatedSdkTimeoutMs),
          "--fail-on-checks",
        ],
        repoRoot,
      ),
    );
    if (!liveTopology.summary.ok) {
      throw new Error(
        `live topology benchmark failed:\n${liveTopology.summary.stderrSnippet ?? "no stderr captured"}`,
      );
    }

    const publishSnapshot = record(
      runCommand(
        "publish-shard-snapshot",
        [
          "bun",
          "./scripts/publish_moat_snapshots.ts",
          "--input",
          moatOutput,
          "--browser-route-input",
          browserRouteOutput,
          "--live-topology-feed-input",
          liveTopologyOutput,
          "--label",
          options.label,
        ],
        repoRoot,
      ),
    );
    if (!publishSnapshot.summary.ok) {
      throw new Error(
        `monthly snapshot publish failed:\n${publishSnapshot.summary.stderrSnippet ?? "no stderr captured"}`,
      );
    }

    const summary: RunSummary = {
      schemaVersion: 1,
      generatedAtUnixMs: Date.now(),
      label: options.label,
      profile: "shard-target",
      browserRouteStatus,
      browserRouteGatePassed,
      paths: {
        moatReport: moatOutput,
        browserRoutes: browserRouteOutput,
        liveTopologyFeed: liveTopologyOutput,
        snapshot: snapshotOutput,
        topologyExport: topologyOutput,
        runSummary: summaryOutput,
      },
      commands,
      warnings,
    };

    writeFileSync(summaryOutput, `${JSON.stringify(summary, null, 2)}\n`);
    console.log(JSON.stringify(summary, null, 2));
  } finally {
    if (server && !options.keepSpacetime) {
      server.kill();
      await server.exited;
    }
    if (!options.keepSpacetime) {
      rmSync(tempDir, { recursive: true, force: true });
    }
  }
}

if (import.meta.main) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
