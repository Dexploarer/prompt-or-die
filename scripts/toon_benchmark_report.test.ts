import { describe, expect, test } from "bun:test";
import { resolve } from "node:path";

import { buildToonExportBenchmarkReport } from "./benchmark_toon_exports";
import {
  renderToonBenchmarkCharts,
  renderToonBenchmarkHtml,
  renderToonBenchmarkMarkdown,
} from "./toon_benchmark_report";

const repoRoot = resolve(import.meta.dir, "..");

describe("toon benchmark report visuals", () => {
  test("renders chart artifacts, markdown, and an html results page", async () => {
    const report = await buildToonExportBenchmarkReport(repoRoot, {
      profile: "ci-smoke",
      iterations: 1,
      rounds: 1,
    });

    const charts = renderToonBenchmarkCharts(report);
    const html = renderToonBenchmarkHtml(report);
    const markdown = renderToonBenchmarkMarkdown(report);

    expect(charts.length).toBeGreaterThanOrEqual(10);
    expect(charts.map((chart) => chart.filename)).toContain(
      "overview-bytes-delta.svg",
    );
    expect(charts.map((chart) => chart.filename)).toContain(
      "uniform_tick_event_batch-efficiency.svg",
    );
    expect(charts.map((chart) => chart.filename)).toContain(
      "toonscape_donor_tick_event_batch-tokens.svg",
    );
    expect(charts.every((chart) => chart.svg.startsWith('<?xml version="1.0"'))).toBe(
      true,
    );

    expect(html).toContain("<!doctype html>");
    expect(html).toContain("TOON Export Benchmark Results");
    expect(html).toContain("Uniform Tick Event Batch");
    expect(html).toContain("Toonscape Donor Tick Event Batch");
    expect(html).toContain("winner-row");
    expect(html).toContain("<svg");
    expect(html).toContain("Best TOON delta vs compact JSON: bytes");

    expect(markdown).toContain("# TOON Export Benchmark Results");
    expect(markdown).toContain("Uniform Tick Event Batch");
    expect(markdown).toContain("Toonscape Donor Tick Event Batch");
    expect(markdown).toContain("Checks");
    expect(markdown).toContain("Shell control plane");
  });
});
