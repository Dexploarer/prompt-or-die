import { expect, test, type Page } from "@playwright/test";
import {
  MAIN_STABLE_FRAME_PERCENT_FLOOR,
  WORKER_CONTROL_SUBMISSION_CEILING,
  WORKER_RESIZE_SUBMISSION_CEILING,
  WORKER_STABLE_FRAME_PERCENT_FLOOR,
} from "../src/render-runtime-gates";

test.setTimeout(45_000);

type GameplayState = {
  renderThread: string;
  frameSource: string;
  worldMode: string | null;
  worldName: string | null;
  focused: boolean;
  controlledEntityId: number | null;
  controlledPosition: [number, number] | null;
  selectedTargetId: number | null;
  clickMoveTarget: [number, number] | null;
  movementSignature: string;
  latestFeedback: string;
};

type RendererStats = {
  renderThread: string;
  requestedRenderThread: string;
  renderThreadFallbackReason: string | null;
  geometryLoadsCompleted: number;
  spriteLoadsCompleted: number;
  pendingGeometryAssets: number;
  pendingSpriteAssets: number;
  mainThreadPerf: {
    warmupMs: number | null;
    submissionsCompleted: number;
    averageSubmissionMs: number;
    slowestSubmissionMs: number;
    byKind: {
      frame: {
        submissionsCompleted: number;
        averageSubmissionMs: number;
        slowestSubmissionMs: number;
      };
      control: {
        submissionsCompleted: number;
        averageSubmissionMs: number;
        slowestSubmissionMs: number;
      };
      resize: {
        submissionsCompleted: number;
        averageSubmissionMs: number;
        slowestSubmissionMs: number;
      };
    };
  };
  runtimePerf: {
    warmupMs: number | null;
    frameBudgetMs: number;
    framesRendered: number;
    stableFrames: number;
    slowFrames: number;
    stableFramePercent: number;
    slowestFrameMs: number;
  };
};

declare global {
  interface Window {
    advanceTime: (ms: number) => Promise<void>;
    podRender: {
      requestGameplayFocus: () => boolean;
      getGameplayState: () => GameplayState;
      getStats: () => RendererStats;
    };
  }
}

async function waitForGameplayReady(page: Page) {
  await page.waitForFunction(() => {
    return (
      typeof window.podRender?.getGameplayState === "function" &&
      typeof window.podRender?.getStats === "function" &&
      window.podRender.getGameplayState().controlledPosition !== null &&
      window.podRender.getGameplayState().frameSource === "threejs" &&
      window.podRender.getStats().runtimePerf.framesRendered > 0
    );
  });
}

async function moveForward(
  page: Page,
  expectedRenderThread: "main" | "worker",
  expectedRequestedRenderThread: "auto" | "worker"
) {
  await expect(page.evaluate(() => window.podRender.requestGameplayFocus())).resolves.toBeTruthy();
  const before = await page.evaluate(() => window.podRender.getGameplayState());
  await page.keyboard.down("w");
  await page.evaluate(() => window.advanceTime(700));
  const during = await page.evaluate(() => window.podRender.getGameplayState());
  await page.keyboard.up("w");
  await page.evaluate(() => window.advanceTime(400));
  const after = await page.evaluate(() => window.podRender.getGameplayState());
  await page.waitForFunction(() => {
    const stats = window.podRender.getStats();
    return (
      stats.runtimePerf.framesRendered >= 2 &&
      stats.pendingGeometryAssets + stats.pendingSpriteAssets === 0
    );
  });
  const stats = await page.evaluate(() => window.podRender.getStats());

  expect(before.focused).toBeTruthy();
  expect(before.worldMode).toBe("local-sandbox");
  expect(before.worldName).toBe("Verdant Hollow");
  expect(before.controlledEntityId).not.toBeNull();
  expect(before.controlledPosition).not.toBeNull();
  expect(during.movementSignature).not.toBe("stop");
  expect(during.controlledPosition).not.toBeNull();
  expect(after.controlledPosition).not.toBeNull();
  const dx = during.controlledPosition![0] - before.controlledPosition![0];
  const dy = during.controlledPosition![1] - before.controlledPosition![1];
  expect(Math.hypot(dx, dy)).toBeGreaterThan(0.25);
  expect(after.movementSignature).toBe("stop");
  expect(stats.renderThread).toBe(expectedRenderThread);
  expect(stats.requestedRenderThread).toBe(expectedRequestedRenderThread);
  expect(stats.renderThreadFallbackReason).toBeNull();
  expect(stats.mainThreadPerf.warmupMs).not.toBeNull();
  expect(stats.mainThreadPerf.submissionsCompleted).toBeGreaterThanOrEqual(2);
  expect(stats.mainThreadPerf.averageSubmissionMs).toBeGreaterThanOrEqual(0);
  expect(stats.mainThreadPerf.slowestSubmissionMs).toBeGreaterThanOrEqual(
    stats.mainThreadPerf.averageSubmissionMs
  );
  expect(
    stats.mainThreadPerf.byKind.frame.submissionsCompleted +
      stats.mainThreadPerf.byKind.control.submissionsCompleted +
      stats.mainThreadPerf.byKind.resize.submissionsCompleted
  ).toBe(stats.mainThreadPerf.submissionsCompleted);
  expect(stats.mainThreadPerf.byKind.frame.submissionsCompleted).toBeGreaterThanOrEqual(2);
  if (expectedRenderThread === "worker") {
    expect(stats.mainThreadPerf.byKind.control.submissionsCompleted).toBeLessThanOrEqual(
      WORKER_CONTROL_SUBMISSION_CEILING
    );
    expect(stats.mainThreadPerf.byKind.resize.submissionsCompleted).toBeLessThanOrEqual(
      WORKER_RESIZE_SUBMISSION_CEILING
    );
  }
  expect(stats.runtimePerf.warmupMs).not.toBeNull();
  expect(stats.runtimePerf.frameBudgetMs).toBeGreaterThan(0);
  expect(stats.runtimePerf.framesRendered).toBeGreaterThanOrEqual(2);
  expect(stats.runtimePerf.stableFrames + stats.runtimePerf.slowFrames).toBe(
    stats.runtimePerf.framesRendered
  );
  expect(stats.runtimePerf.stableFramePercent).toBeGreaterThanOrEqual(0);
  expect(stats.runtimePerf.stableFramePercent).toBeLessThanOrEqual(100);
  expect(stats.runtimePerf.slowestFrameMs).toBeGreaterThan(0);
  if (expectedRenderThread === "main") {
    expect(stats.runtimePerf.stableFramePercent).toBeGreaterThanOrEqual(
      MAIN_STABLE_FRAME_PERCENT_FLOOR
    );
  } else {
    expect(stats.runtimePerf.stableFramePercent).toBeGreaterThanOrEqual(
      WORKER_STABLE_FRAME_PERCENT_FLOOR
    );
    expect(stats.runtimePerf.stableFrames).toBeGreaterThan(stats.runtimePerf.slowFrames);
  }
  expect(stats.geometryLoadsCompleted + stats.spriteLoadsCompleted).toBeGreaterThan(0);
}

test("main-thread local sandbox route accepts gameplay focus and movement input", async ({ page }) => {
  await page.goto("/?world=local-sandbox&backend=webgl2");
  await waitForGameplayReady(page);
  await moveForward(page, "main", "auto");
});

test("worker local sandbox route accepts gameplay focus and movement input", async ({ page }) => {
  await page.goto("/?world=local-sandbox&renderThread=worker&backend=webgl2");
  await waitForGameplayReady(page);
  await moveForward(page, "worker", "worker");
});
