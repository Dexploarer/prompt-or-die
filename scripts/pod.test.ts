import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";

import { buildCliSurfaceCatalog } from "./cli_surface";
import {
  buildEntryPayload,
  buildListPayload,
  parsePodArgs,
  renderHumanList,
} from "./pod";

const repoRoot = resolve(import.meta.dir, "..");

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

  test("builds a filtered list payload", () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const payload = buildListPayload(catalog, repoRoot, {
      audience: "agent",
      area: "benchmark",
      kind: "cargo-example",
      machineReadableOnly: true,
      json: false,
      text: "topology",
    });

    expect(payload.commands.map((entry) => entry.id)).toEqual([
      "topology-feed-benchmark-suite",
    ]);
  });

  test("describes the server environment contract", () => {
    const catalog = buildCliSurfaceCatalog(repoRoot);
    const payload = buildEntryPayload(catalog, repoRoot, "env", "pod-server");

    expect(payload.entry.id).toBe("pod-server");
    expect(payload.environment.map((variable) => variable.name)).toContain(
      "POD_BIND_ADDRESS",
    );
    expect(payload.environment.map((variable) => variable.name)).toContain(
      "POD_WORLD_SEED",
    );
  });

  test("renders a human-readable list with the pod entrypoint", () => {
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
    expect(rendered).toContain("pod");
    expect(rendered).toContain("verify-cli-surface");
  });
});
