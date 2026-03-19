#!/usr/bin/env bun

import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";

import {
  buildCliSurfaceCatalog,
  CLI_DOC_PATH,
  type CliSurfaceValidation,
  validateCliSurfaceCatalog,
} from "./cli_surface";

type Mode = "check" | "write" | "json";

type Options = {
  mode: Mode;
};

function printHelp() {
  console.error(
    "Usage: bun ./scripts/verify_cli_surface.ts [--check|--write|--json]\n\nDefaults to --check.\n  --check  Verify CLI coverage and docs drift\n  --write  Regenerate docs/cli-surface.md from the source manifest\n  --json   Print the machine-readable catalog and validation report",
  );
}

function parseArgs(argv: string[]): Options {
  let mode: Mode = "check";
  for (const argument of argv) {
    switch (argument) {
      case "--check":
        mode = "check";
        break;
      case "--write":
        mode = "write";
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

function logValidationProblems(validation: CliSurfaceValidation) {
  if (validation.duplicateIds.length > 0) {
    console.error(`Duplicate command ids: ${validation.duplicateIds.join(", ")}`);
  }
  if (validation.duplicateAliases.length > 0) {
    console.error(
      `Duplicate command aliases: ${validation.duplicateAliases.join(", ")}`,
    );
  }
  if (validation.invalidAliasEntries.length > 0) {
    console.error(
      `Invalid command aliases: ${validation.invalidAliasEntries.join(", ")}`,
    );
  }
  if (validation.missingEntrypoints.length > 0) {
    console.error(
      `Missing entrypoints: ${validation.missingEntrypoints.join(", ")}`,
    );
  }
  if (validation.missingDocs.length > 0) {
    console.error(`Missing docs: ${validation.missingDocs.join(", ")}`);
  }
  if (validation.unknownCoverageKeys.length > 0) {
    console.error(
      `Unknown coverage keys: ${validation.unknownCoverageKeys.join(", ")}`,
    );
  }
  if (validation.uncoveredDiscoveredSurfaces.length > 0) {
    console.error("Uncovered discovered surfaces:");
    for (const surface of validation.uncoveredDiscoveredSurfaces) {
      console.error(
        `- ${surface.key}: \`${surface.command}\` (${surface.entrypoint})`,
      );
    }
  }
  if (validation.invalidExecutionEntries.length > 0) {
    console.error(
      `Invalid execution entries: ${validation.invalidExecutionEntries.join(", ")}`,
    );
  }
  if (validation.invalidCapabilityEntries.length > 0) {
    console.error(
      `Invalid capability entries: ${validation.invalidCapabilityEntries.join(", ")}`,
    );
  }
  if (validation.invalidPassthroughEntries.length > 0) {
    console.error(
      `Invalid passthrough entries: ${validation.invalidPassthroughEntries.join(", ")}`,
    );
  }
  if (validation.invalidInteractiveEntries.length > 0) {
    console.error(
      `Invalid interactive entries: ${validation.invalidInteractiveEntries.join(", ")}`,
    );
  }
  if (!validation.docInSync) {
    console.error(
      `Generated Markdown is out of sync with ${validation.docPath}. Run bun ./scripts/verify_cli_surface.ts --write.`,
    );
  }
}

function main() {
  const options = parseArgs(process.argv.slice(2));
  const repoRoot = resolve(import.meta.dir, "..");
  const catalog = buildCliSurfaceCatalog(repoRoot);

  if (options.mode === "json") {
    const validation = validateCliSurfaceCatalog(catalog, repoRoot);
    console.log(
      JSON.stringify(
        {
          ...catalog,
          validation: {
            ok: validation.ok,
            duplicateIds: validation.duplicateIds,
            duplicateAliases: validation.duplicateAliases,
            invalidAliasEntries: validation.invalidAliasEntries,
            missingEntrypoints: validation.missingEntrypoints,
            missingDocs: validation.missingDocs,
            unknownCoverageKeys: validation.unknownCoverageKeys,
            uncoveredDiscoveredSurfaces: validation.uncoveredDiscoveredSurfaces,
            invalidExecutionEntries: validation.invalidExecutionEntries,
            invalidCapabilityEntries: validation.invalidCapabilityEntries,
            invalidPassthroughEntries: validation.invalidPassthroughEntries,
            invalidInteractiveEntries: validation.invalidInteractiveEntries,
            docPath: validation.docPath,
            docInSync: validation.docInSync,
          },
        },
        null,
        2,
      ),
    );
    return;
  }

  if (options.mode === "write") {
    const preWriteValidation = validateCliSurfaceCatalog(catalog, repoRoot);
    const targetPath = resolve(repoRoot, CLI_DOC_PATH);
    mkdirSync(dirname(targetPath), { recursive: true });
    writeFileSync(targetPath, preWriteValidation.generatedMarkdown);

    const postWriteValidation = validateCliSurfaceCatalog(catalog, repoRoot);
    if (!postWriteValidation.ok) {
      logValidationProblems(postWriteValidation);
      process.exitCode = 1;
      return;
    }

    console.error(`Wrote ${CLI_DOC_PATH}`);
    return;
  }

  const validation = validateCliSurfaceCatalog(catalog, repoRoot);
  if (!validation.ok) {
    logValidationProblems(validation);
    process.exitCode = 1;
    return;
  }

  console.error("CLI surface verification passed.");
}

main();
