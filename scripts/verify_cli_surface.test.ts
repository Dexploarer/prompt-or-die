import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";

import {
  buildCliSurfaceCatalog,
  renderCliSurfaceMarkdown,
  validateCliSurfaceCatalog,
} from "./cli_surface";

const repoRoot = resolve(import.meta.dir, "..");

describe("verify cli surface", () => {
  test("covers every supported discovered surface and keeps the docs in sync", () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const validation = validateCliSurfaceCatalog(catalog, repoRoot);

    expect(validation.duplicateIds).toEqual([]);
    expect(validation.duplicateAliases).toEqual([]);
    expect(validation.invalidAliasEntries).toEqual([]);
    expect(validation.missingEntrypoints).toEqual([]);
    expect(validation.missingDocs).toEqual([]);
    expect(validation.unknownCoverageKeys).toEqual([]);
    expect(validation.uncoveredDiscoveredSurfaces).toEqual([]);
    expect(validation.invalidExecutionEntries).toEqual([]);
    expect(validation.invalidCapabilityEntries).toEqual([]);
    expect(validation.invalidPassthroughEntries).toEqual([]);
    expect(validation.invalidInteractiveEntries).toEqual([]);
    expect(validation.docInSync).toBe(true);
    expect(validation.ok).toBe(true);
  });

  test("renders alias-first docs and schema version 2 catalog metadata", () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const markdown = renderCliSurfaceMarkdown(catalog, repoRoot);

    expect(catalog.schemaVersion).toBe(2);
    expect(markdown).toContain("# CLI Surface");
    expect(markdown).toContain("## Canonical root CLI");
    expect(markdown).toContain("## Interactive Shell");
    expect(markdown).toContain("bun ./scripts/pod.ts shell");
    expect(markdown).toContain("bun ./scripts/pod.ts shell --agent");
    expect(markdown).toContain("printf '{\"type\":\"builtin\"");
    expect(markdown).toContain("bun ./scripts/pod.ts workspace check");
    expect(markdown).toContain("bun ./scripts/pod.ts runtime server --dry-run");
    expect(markdown).toContain("bun ./scripts/pod.ts env pod-server --effective --json");
    expect(markdown).toContain("bun ./scripts/pod.ts export events --format toon");
    expect(markdown).toContain("bun ./scripts/pod.ts export multiverse --format json");
    expect(markdown).toContain("runtime server");
    expect(markdown).toContain("export world");
    expect(markdown).toContain("benchmark toon-exports");
    expect(markdown).toContain("toon-export-benchmark.html");
    expect(markdown).toContain("--html-output");
    expect(markdown).toContain("--charts-dir");
    expect(markdown).toContain("catalog verify");
    expect(markdown).toContain("pod-shell");
    expect(markdown).toContain("stdio-json");
    expect(markdown).toContain("newline-delimited");
    expect(markdown).toContain("session.stdin.closed");
    expect(markdown).toContain("hook.triggered");
    expect(markdown).toContain("session.started");
    expect(markdown).toContain("TOON is reserved for `pod export ... --format toon`");
    expect(markdown).toContain("## Dedicated server environment contract");
    expect(markdown).toContain("POD_TICK_RATE");
    expect(markdown).toContain("bun ./scripts/verify_cli_surface.ts --json");
  });
});
