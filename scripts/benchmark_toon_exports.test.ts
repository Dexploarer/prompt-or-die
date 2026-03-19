import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";

import { buildToonExportBenchmarkReport } from "./benchmark_toon_exports";
import {
  buildBrokenUniformToonLines,
  decodeBenchmarkToonLines,
  decodePodExportToon,
  renderPodExport,
} from "./pod_sdk";

const repoRoot = resolve(import.meta.dir, "..");

describe("toon export benchmark", () => {
  test("recommends TOON only where the dataset actually wins", async () => {
    const report = await buildToonExportBenchmarkReport(repoRoot, {
      profile: "ci-smoke",
      iterations: 2,
      rounds: 1,
    });

    expect(report.schemaVersion).toBe(1);
    expect(report.profile).toBe("ci-smoke");
    expect(report.decision.shellControlPlane).toBe("json");
    expect(report.variants.map((variant) => variant.id)).toEqual([
      "json-pretty",
      "json-compact",
      "toon-comma",
      "toon-tab",
    ]);
    expect(report.allChecksPassed).toBe(true);

    const uniform = report.datasets.find(
      (dataset) => dataset.id === "uniform_tick_event_batch",
    );
    expect(uniform).toBeTruthy();
    expect(uniform?.recommendation.preferredFormat).toBe("toon");
    expect((uniform?.bestToonDeltaVsCompactJson.tokens ?? 0)).toBeGreaterThan(0);
    expect((uniform?.bestToonDeltaVsCompactJson.bytes ?? 0)).toBeGreaterThan(0);

    const toonscapeDonor = report.datasets.find(
      (dataset) => dataset.id === "toonscape_donor_tick_event_batch",
    );
    expect(toonscapeDonor).toBeTruthy();
    expect(toonscapeDonor?.recommendation.preferredFormat).toBe("toon");
    expect(
      toonscapeDonor?.bestToonDeltaVsCompactJson.percentTokens ?? 0,
    ).toBeGreaterThanOrEqual(70);
    expect(
      toonscapeDonor?.bestToonDeltaVsCompactJson.percentBytes ?? 0,
    ).toBeGreaterThanOrEqual(70);

    const multiverse = report.datasets.find(
      (dataset) => dataset.id === "deep_multiverse_index",
    );
    expect(multiverse).toBeTruthy();
    expect(multiverse?.recommendation.preferredFormat).toBe("json");

    const logs = report.datasets.find(
      (dataset) => dataset.id === "semi_uniform_agent_logs",
    );
    expect(logs).toBeTruthy();
    expect(
      (logs?.recommendation.preferredFormat === "toon") ||
        (logs?.bestToonDeltaVsPrettyJson.tokens ?? 0) > 0,
    ).toBe(true);

    expect(report.validation.strictRowWidthError).toContain("Expected");
    expect(report.validation.strictTruncationError).toContain("Expected");
  });

  test("roundtrips exported TOON payloads for world, events, and multiverse", () => {
    expect(decodePodExportToon("events", renderPodExport("events", "toon").text)).toEqual(
      JSON.parse(renderPodExport("events", "json").text),
    );
    expect(decodePodExportToon("world", renderPodExport("world", "toon").text)).toEqual(
      JSON.parse(renderPodExport("world", "json").text),
    );
    expect(
      decodePodExportToon("multiverse", renderPodExport("multiverse", "toon").text),
    ).toEqual(JSON.parse(renderPodExport("multiverse", "json").text));
  });

  test("strict validation catches row-width and truncation errors", () => {
    expect(() =>
      decodeBenchmarkToonLines(
        buildBrokenUniformToonLines("uniform_tick_event_batch", "row-width"),
      ),
    ).toThrow("Expected");
    expect(() =>
      decodeBenchmarkToonLines(
        buildBrokenUniformToonLines("uniform_tick_event_batch", "truncated"),
      ),
    ).toThrow("Expected");
  });
});
