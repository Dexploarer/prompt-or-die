import { describe, expect, it } from "bun:test";

import {
  buildPublishedComparisonOutputPath,
  findLatestPriorSnapshotFilename,
  formatIsoWeekLabel,
  normalizeComparisonReportForPublication,
  parseArgs,
  resolveBrowserRouteStatus,
} from "./run_shard_target_snapshot";

describe("run shard target snapshot", () => {
  it("formats ISO week labels deterministically", () => {
    expect(formatIsoWeekLabel(new Date("2026-03-13T12:00:00Z"))).toBe("2026-W11");
  });

  it("builds the retained published comparison output path", () => {
    expect(buildPublishedComparisonOutputPath("2026-W11")).toBe(
      "docs/benchmark-snapshots/2026-W11-shard-target-comparison.json",
    );
  });

  it("normalizes retained comparison report paths for publication", () => {
    const normalized = normalizeComparisonReportForPublication(
      JSON.stringify({
        baselinePath: "/tmp/comparison-baseline.json",
        candidatePath: "/tmp/candidate.json",
        summary: { regressions: 0 },
      }),
      "docs/benchmark-snapshots/2026-W10-shard-target.json",
      "docs/benchmark-snapshots/2026-W11-shard-target.json",
    );

    expect(JSON.parse(normalized)).toEqual({
      baselinePath: "docs/benchmark-snapshots/2026-W10-shard-target.json",
      candidatePath: "docs/benchmark-snapshots/2026-W11-shard-target.json",
      summary: { regressions: 0 },
    });
  });

  it("selects the latest prior weekly shard-target snapshot", () => {
    expect(
      findLatestPriorSnapshotFilename(
        [
          "2026-W09-shard-target.json",
          "2026-W11-shard-target.json",
          "notes.md",
          "2026-W11-ci-smoke.json",
        ],
        "2026-W12",
      ),
    ).toBe("2026-W11-shard-target.json");

    expect(
      findLatestPriorSnapshotFilename(
        [
          "2026-W09-shard-target.json",
          "2026-W11-shard-target.json",
        ],
        "2026-W11",
      ),
    ).toBe("2026-W09-shard-target.json");

    expect(
      findLatestPriorSnapshotFilename(
        ["2026-W11-shard-target.json"],
        "2026-W11",
      ),
    ).toBeNull();
  });

  it("parses explicit runtime options", () => {
    const options = parseArgs([
      "--label",
      "2026-W12",
      "--host",
      "127.0.0.2",
      "--port",
      "3200",
      "--generated-sdk-timeout-ms",
      "9000",
      "--compare-baseline",
      "docs/benchmark-snapshots/2026-W11-shard-target.json",
      "--reuse-browser-routes",
      "--keep-spacetime",
      "--output",
      "artifacts/custom-run.json",
    ]);

    expect(options.label).toBe("2026-W12");
    expect(options.host).toBe("127.0.0.2");
    expect(options.port).toBe(3200);
    expect(options.generatedSdkTimeoutMs).toBe(9000);
    expect(options.compareBaseline).toBe(
      "docs/benchmark-snapshots/2026-W11-shard-target.json",
    );
    expect(options.reuseBrowserRoutes).toBe(true);
    expect(options.keepSpacetime).toBe(true);
    expect(options.output).toBe("artifacts/custom-run.json");
  });

  it("classifies browser route capture outcomes", () => {
    expect(resolveBrowserRouteStatus(true, true, false)).toBe("passed");
    expect(resolveBrowserRouteStatus(false, true, false)).toBe("artifact_only");
    expect(resolveBrowserRouteStatus(true, true, true)).toBe("reused");
    expect(resolveBrowserRouteStatus(true, false, true)).toBe("failed");
    expect(resolveBrowserRouteStatus(false, false, false)).toBe("failed");
  });
});
