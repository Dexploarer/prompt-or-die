import { describe, expect, it } from "bun:test";

import {
  formatMonthLabel,
  parseArgs,
  resolveBrowserRouteStatus,
} from "./run_shard_target_snapshot";

describe("run shard target snapshot", () => {
  it("formats month labels deterministically", () => {
    expect(formatMonthLabel(new Date("2026-03-13T12:00:00Z"))).toBe("2026-03");
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
      "--reuse-browser-routes",
      "--keep-spacetime",
      "--output",
      "artifacts/custom-run.json",
    ]);

    expect(options.label).toBe("2026-04");
    expect(options.host).toBe("127.0.0.2");
    expect(options.port).toBe(3200);
    expect(options.generatedSdkTimeoutMs).toBe(9000);
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
