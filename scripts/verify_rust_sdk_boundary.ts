#!/usr/bin/env bun

import { readFileSync } from "node:fs";
import { resolve } from "node:path";

export const RUST_SDK_BOUNDARY_DOC_PATH = "docs/rust-sdk-boundary.md";

type Mode = "check" | "json";

type Options = {
  mode: Mode;
};

export type RustSdkBoundaryExpectation = {
  path: string;
  requiredSnippets: string[];
};

export type RustSdkBoundaryValidation = {
  ok: boolean;
  expectations: RustSdkBoundaryExpectation[];
  missing: Array<{
    path: string;
    missingSnippets: string[];
  }>;
};

const EXPECTATIONS: RustSdkBoundaryExpectation[] = [
  {
    path: "README.md",
    requiredSnippets: ["Rust SDK Boundary", "docs/rust-sdk-boundary.md"],
  },
  {
    path: "docs/README.md",
    requiredSnippets: ["Rust SDK Boundary", "./rust-sdk-boundary.md"],
  },
  {
    path: "docs/platform-stabilization.md",
    requiredSnippets: ["docs/rust-sdk-boundary.md"],
  },
  {
    path: RUST_SDK_BOUNDARY_DOC_PATH,
    requiredSnippets: [
      "# POD Rust SDK Boundary",
      "## Stable contracts to depend on now",
      "RustSdkHandoffArtifact",
      "VersionedObservation",
      "VersionedAgentAction",
      "VersionedTickTelemetry",
      "RemoteTopologyBundle",
      "install_generated_binding_runtime()",
      "install_generated_sdk_runtime()",
      "## Adapter lanes",
      "rs_state_adapter",
      "rs_action_adapter",
      "rs_rollout_recorder",
      "rs_benchmark_runner",
      "## Readiness gates",
    ],
  },
  {
    path: "crates/pod-core/src/lib.rs",
    requiredSnippets: [
      "RustSdkHandoffArtifact",
      "VersionedAgentAction",
      "VersionedObservation",
      "VersionedTickTelemetry",
      "RemoteTopologyBundle",
      "ReplayTrainingSample",
    ],
  },
  {
    path: "crates/pod-core/src/app.rs",
    requiredSnippets: [
      'register_contract::<VersionedObservation>("VersionedObservation")',
      'register_contract::<VersionedAgentAction>("VersionedAgentAction")',
      'register_contract::<VersionedTickTelemetry>("VersionedTickTelemetry")',
      'register_contract::<RustSdkHandoffArtifact>("RustSdkHandoffArtifact")',
    ],
  },
  {
    path: "crates/pod-stdb/src/client.rs",
    requiredSnippets: [
      "pub fn install_generated_binding_runtime(&mut self) -> GeneratedBindingEndpoint",
      "pub fn install_generated_sdk_runtime(&mut self)",
    ],
  },
  {
    path: "crates/pod-net/src/client_stdb.rs",
    requiredSnippets: [
      "pub fn install_generated_binding_runtime(&mut self) -> GeneratedBindingEndpoint",
      "pub fn install_generated_sdk_runtime(&mut self)",
    ],
  },
  {
    path: "scripts/pod_sdk.ts",
    requiredSnippets: [
      'export const POD_EXPORT_TARGETS = ["world", "events", "multiverse"] as const;',
    ],
  },
  {
    path: "scripts/cli_surface.ts",
    requiredSnippets: ['id: "verify-rust-sdk-boundary"'],
  },
];

function printHelp() {
  console.error(
    "Usage: bun ./scripts/verify_rust_sdk_boundary.ts [--check|--json]\n\nDefaults to --check.\n  --check  Verify the repo-owned Rust SDK boundary doc and stable seam coverage\n  --json   Print the machine-readable validation report",
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

export function validateRustSdkBoundary(
  repoRoot: string,
): RustSdkBoundaryValidation {
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
  const validation = validateRustSdkBoundary(repoRoot);

  if (options.mode === "json") {
    console.log(JSON.stringify(validation, null, 2));
    return;
  }

  if (!validation.ok) {
    for (const entry of validation.missing) {
      console.error(`Missing required Rust SDK boundary snippets in ${entry.path}:`);
      for (const snippet of entry.missingSnippets) {
        console.error(`- ${snippet}`);
      }
    }
    process.exitCode = 1;
    return;
  }

  console.error("Rust SDK boundary verification passed.");
}

if (import.meta.main) {
  main();
}
