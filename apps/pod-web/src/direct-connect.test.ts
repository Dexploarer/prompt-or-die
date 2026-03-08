import { describe, expect, test } from "bun:test";

import { runtimeConfigFromLocation } from "./direct-connect";

describe("direct-connect runtime config", () => {
  test("builds a websocket config from server query params", () => {
    const config = runtimeConfigFromLocation({
      search: "?server=127.0.0.1:7778&player=Scout&debug=1&reconnectMs=2500"
    } as Location);

    expect(config).not.toBeNull();
    expect(config?.url).toBe("ws://127.0.0.1:7778");
    expect(config?.playerName).toBe("Scout");
    expect(config?.debugTelemetry).toBe(true);
    expect(config?.reconnectDelayMs).toBe(2500);
  });

  test("returns null when no direct-connect params are present", () => {
    const config = runtimeConfigFromLocation({
      search: "?demo=1"
    } as Location);

    expect(config).toBeNull();
  });
});
