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
    expect(validation.missingEntrypoints).toEqual([]);
    expect(validation.missingDocs).toEqual([]);
    expect(validation.unknownCoverageKeys).toEqual([]);
    expect(validation.uncoveredDiscoveredSurfaces).toEqual([]);
    expect(validation.docInSync).toBe(true);
    expect(validation.ok).toBe(true);
  });

  test("renders the audience matrix and server environment contract", () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const markdown = renderCliSurfaceMarkdown(catalog, repoRoot);

    expect(markdown).toContain("# CLI Surface");
    expect(markdown).toContain("## Audience matrix");
    expect(markdown).toContain("## Dedicated server environment contract");
    expect(markdown).toContain("verify-cli-surface");
    expect(markdown).toContain("POD_TICK_RATE");
    expect(markdown).toContain("bun ./scripts/verify_cli_surface.ts --json");
  });
});
