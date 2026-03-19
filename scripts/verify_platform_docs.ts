#!/usr/bin/env bun

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export const PLATFORM_STABILIZATION_DOC_PATH = "docs/platform-stabilization.md";

type Mode = "check" | "json";

type Options = {
  mode: Mode;
};

export type PlatformDocExpectation = {
  path: string;
  requiredSnippets: string[];
};

export type PlatformDocValidation = {
  ok: boolean;
  expectations: PlatformDocExpectation[];
  missing: Array<{
    path: string;
    missingSnippets: string[];
  }>;
};

const EXPECTATIONS: PlatformDocExpectation[] = [
  {
    path: "README.md",
    requiredSnippets: [
      "Platform Stabilization",
      "docs/platform-stabilization.md",
      "IMPLEMENTATION_PHASES.md",
      "IMPLEMENTATION_PLAN.md",
      "SESSION.md",
    ],
  },
  {
    path: "docs/README.md",
    requiredSnippets: [
      "Platform Stabilization",
      "./platform-stabilization.md",
      "IMPLEMENTATION_PHASES.md",
      "IMPLEMENTATION_PLAN.md",
      "SESSION.md",
    ],
  },
  {
    path: "docs/architecture.md",
    requiredSnippets: [
      "[Platform Stabilization](./platform-stabilization.md)",
    ],
  },
  {
    path: "docs/plugin-model.md",
    requiredSnippets: [
      "[Platform Stabilization](./platform-stabilization.md)",
    ],
  },
  {
    path: "docs/benchmark-suite.md",
    requiredSnippets: [
      "## Benchmark requirement tiers",
      "Platform requirement gate",
      "Local tooling / proof surface",
      "[Platform Stabilization](./platform-stabilization.md)",
    ],
  },
  {
    path: PLATFORM_STABILIZATION_DOC_PATH,
    requiredSnippets: [
      "# Platform Stabilization",
      "## Planning route",
      "## Benchmark requirement tiers",
      "## Public contract surfaces",
      "## Shipping, authz, and SDK boundaries",
      "ci-smoke",
      "shard-target",
      "OpsHttpAuthorizationPolicySource",
    ],
  },
  {
    path: "SESSION.md",
    requiredSnippets: [
      "IMPLEMENTATION_PHASES.md",
      "IMPLEMENTATION_PLAN.md",
      "progress.md",
      "active unchecked checklist",
      "historical log",
    ],
  },
  {
    path: "IMPLEMENTATION_PHASES.md",
    requiredSnippets: [
      "Historical log",
      "`IMPLEMENTATION_PLAN.md` remains the archival completion record",
      "use this file as the active execution checklist",
      "Planning docs agree on the same active route and the same archival/history split",
    ],
  },
  {
    path: "docs/implementation-reset-audit.md",
    requiredSnippets: [
      "Preserve `IMPLEMENTATION_PLAN.md` as the historical implementation record.",
      "Reset `IMPLEMENTATION_PHASES.md` into the new unchecked active execution plan.",
      "Track current execution in `SESSION.md`.",
    ],
  },
];

function printHelp() {
  console.error(
    "Usage: bun ./scripts/verify_platform_docs.ts [--check|--json]\n\nDefaults to --check.\n  --check  Verify platform-hardening docs and planning-route consistency\n  --json   Print the machine-readable validation report",
  );
}

function parseArgs(argv: string[]): Options {
  let mode: Mode = "check";
  for (const argument of argv) {
    switch (argument) {
      case "--check":
        mode = "check";
        break;
      case "--json":
        mode = "json";
        break;
      case "--help":
      case "-h":
        printHelp();
        process.exit(0);
      default:
        throw new Error(`unknown argument: ${argument}`);
    }
  }

  return { mode };
}

function readText(repoRoot: string, path: string): string {
  return readFileSync(resolve(repoRoot, path), "utf8");
}

export function validatePlatformDocs(repoRoot: string): PlatformDocValidation {
  const missing = EXPECTATIONS.map((expectation) => {
    const text = readText(repoRoot, expectation.path);
    const missingSnippets = expectation.requiredSnippets.filter(
      (snippet) => !text.includes(snippet),
    );
    return {
      path: expectation.path,
      missingSnippets,
    };
  }).filter((entry) => entry.missingSnippets.length > 0);

  return {
    ok: missing.length === 0,
    expectations: EXPECTATIONS,
    missing,
  };
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const repoRoot = resolve(import.meta.dir, "..");
  const validation = validatePlatformDocs(repoRoot);

  if (options.mode === "json") {
    console.log(JSON.stringify(validation, null, 2));
    return;
  }

  if (!validation.ok) {
    for (const entry of validation.missing) {
      console.error(`Missing required platform-doc snippets in ${entry.path}:`);
      for (const snippet of entry.missingSnippets) {
        console.error(`- ${snippet}`);
      }
    }
    process.exitCode = 1;
    return;
  }

  console.error("Platform doc verification passed.");
}

if (import.meta.main) {
  main();
}
