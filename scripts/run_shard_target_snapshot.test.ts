import { describe, expect, it } from "bun:test";

import {
  findLatestPriorSnapshotFilename,
  formatMonthLabel,
  parseArgs,
  resolveBrowserRouteStatus,
} from "./run_shard_target_snapshot";

describe("run shard target snapshot", () => {
  it("formats month labels deterministically", () => {
    expect(formatMonthLabel(new Date("2026-03-13T12:00:00Z"))).toBe("2026-03");
  });

  it("selects the latest prior monthly shard-target snapshot", () => {
    expect(
      findLatestPriorSnapshotFilename(
        [
          "2026-01-shard-target.json",
          "2026-03-shard-target.json",
          "notes.md",
          "2026-03-ci-smoke.json",
        ],
        "2026-04",
      ),
    ).toBe("2026-03-shard-target.json");

    expect(
      findLatestPriorSnapshotFilename(
        [
          "2026-01-shard-target.json",
          "2026-03-shard-target.json",
        ],
        "2026-03",
      ),
    ).toBe("2026-01-shard-target.json");

    expect(
      findLatestPriorSnapshotFilename(
        ["2026-03-shard-target.json"],
        "2026-03",
      ),
    ).toBeNull();
  });

  it("parses explicit runtime options", () => {
    const options = parseArgs([
      "--label",
      "2026-04",
      "--host",
      "127.0.0.2",
      "--port",
      "3200",
      "--generated-sdk-timeout-ms",
      "9000",
      "--compare-baseline",
      "docs/benchmark-snapshots/2026-03-shard-target.json",
      "--reuse-browser-routes",
      "--keep-spacetime",
      "--output",
      "artifacts/custom-run.json",
    ]);

    expect(options.label).toBe("2026-04");
    expect(options.host).toBe("127.0.0.2");
    expect(options.port).toBe(3200);
    expect(options.generatedSdkTimeoutMs).toBe(9000);
    expect(options.compareBaseline).toBe(
      "docs/benchmark-snapshots/2026-03-shard-target.json",
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
