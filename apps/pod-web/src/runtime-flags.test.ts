import { describe, expect, test } from "bun:test";

import {
  resolveFixedTimeMs,
  shouldPauseInteractiveRuntime
} from "./runtime-flags";

describe("runtime flags", () => {
  test("parses fixed-time query params for deterministic rendering", () => {
    expect(resolveFixedTimeMs("")).toBeNull();
    expect(resolveFixedTimeMs("?fixedTimeMs=0")).toBe(0);
    expect(resolveFixedTimeMs("?fixedTimeMs=1250")).toBe(1250);
    expect(resolveFixedTimeMs("?fixedTimeMs=-5")).toBeNull();
    expect(resolveFixedTimeMs("?fixedTimeMs=abc")).toBeNull();
  });

  test("parses paused-runtime query params for manual stepping", () => {
    expect(shouldPauseInteractiveRuntime("")).toBe(false);
    expect(shouldPauseInteractiveRuntime("?paused=1")).toBe(true);
    expect(shouldPauseInteractiveRuntime("?paused=true")).toBe(true);
    expect(shouldPauseInteractiveRuntime("?pause=yes")).toBe(true);
    expect(shouldPauseInteractiveRuntime("?paused=0")).toBe(false);
  });
});
