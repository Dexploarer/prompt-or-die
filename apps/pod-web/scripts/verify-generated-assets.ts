#!/usr/bin/env bun

import { resolve } from "node:path";

export const GENERATED_ASSET_PATHS = [
  "apps/pod-web/artifacts/source-assets",
  "apps/pod-web/artifacts/staged-assets",
  "apps/pod-web/public/assets",
] as const;

type CommandResult = {
  ok: boolean;
  exitCode: number;
  stdout: string;
  stderr: string;
};

function runCommand(argv: string[], cwd: string): CommandResult {
  const result = Bun.spawnSync(argv, {
    cwd,
    env: process.env,
    stdout: "pipe",
    stderr: "pipe",
  });

  return {
    ok: result.exitCode === 0,
    exitCode: result.exitCode ?? 1,
    stdout: Buffer.from(result.stdout).toString("utf8"),
    stderr: Buffer.from(result.stderr).toString("utf8"),
  };
}

export function parseGitStatusPorcelainPaths(output: string): string[] {
  const changedPaths: string[] = [];

  for (const line of output.split(/\r?\n/)) {
    if (!line.trim()) {
      continue;
    }
    const pathField = line.slice(3).trim();
    const renameTargets = pathField.split(" -> ").map((segment) => segment.trim());
    changedPaths.push(...renameTargets.filter(Boolean));
  }

  return changedPaths;
}

export function formatGeneratedAssetDriftError(changedPaths: string[]): string {
  const lines = [
    "Generated pod-web asset outputs are out of date after sync:assets.",
    "Re-run `cd apps/pod-web && bun run sync:assets` and commit the resulting generated files.",
  ];

  for (const path of changedPaths) {
    lines.push(`- ${path}`);
  }

  return lines.join("\n");
}

export function assertCleanGeneratedAssetOutputs(statusOutput: string): void {
  const changedPaths = parseGitStatusPorcelainPaths(statusOutput);
  if (changedPaths.length > 0) {
    throw new Error(formatGeneratedAssetDriftError(changedPaths));
  }
}

async function main() {
  const appRoot = resolve(import.meta.dir, "..");
  const repoRoot = resolve(appRoot, "..", "..");

  const syncCommand = runCommand(["bun", "run", "scripts/sync-assets.mjs"], appRoot);
  if (!syncCommand.ok) {
    throw new Error(
      `sync:assets failed:\n${syncCommand.stderr || syncCommand.stdout || "no output captured"}`,
    );
  }

  const gitStatusCommand = runCommand(
    ["git", "status", "--porcelain", "--", ...GENERATED_ASSET_PATHS],
    repoRoot,
  );
  if (!gitStatusCommand.ok) {
    throw new Error(
      `git status failed while checking generated assets:\n${
        gitStatusCommand.stderr || gitStatusCommand.stdout || "no output captured"
      }`,
    );
  }

  assertCleanGeneratedAssetOutputs(gitStatusCommand.stdout);
  console.log(
    JSON.stringify(
      {
        ok: true,
        checkedPaths: GENERATED_ASSET_PATHS,
      },
      null,
      2,
    ),
  );
}

if (import.meta.main) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exit(1);
  });
}
